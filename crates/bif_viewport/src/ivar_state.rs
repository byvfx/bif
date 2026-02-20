//! Ivar CPU path tracer state management.
//!
//! Contains render mode selection, build status tracking, and progressive rendering state.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
};
use std::time::Instant;

use bif_math::{Camera, Vec3};
use bif_renderer::{
    generate_buckets, Bucket, BucketResultWithAovs, BvhNode, ImageBuffer, DEFAULT_BUCKET_SIZE,
};

/// Render mode selection: GPU viewport or Ivar CPU path tracer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RenderMode {
    /// Real-time GPU viewport rendering (wgpu).
    #[default]
    Vulkan,
    /// Ivar CPU path tracer for production quality.
    Ivar,
}

/// AOV channel for viewport preview.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AovChannel {
    /// Beauty pass (RGB).
    #[default]
    Beauty,
    /// Alpha channel (grayscale).
    Alpha,
    /// Depth (normalized grayscale).
    Depth,
    /// Normal (RGB from XYZ).
    Normal,
}

impl AovChannel {
    /// Display name for UI dropdown.
    pub fn display_name(&self) -> &'static str {
        match self {
            AovChannel::Beauty => "Beauty",
            AovChannel::Alpha => "Alpha",
            AovChannel::Depth => "Depth",
            AovChannel::Normal => "Normal",
        }
    }

    /// All channels for UI dropdown.
    pub fn all() -> &'static [AovChannel] {
        &[
            AovChannel::Beauty,
            AovChannel::Alpha,
            AovChannel::Depth,
            AovChannel::Normal,
        ]
    }
}

impl RenderMode {
    /// Get display name for UI.
    pub fn display_name(&self) -> &'static str {
        match self {
            RenderMode::Vulkan => "Vulkan",
            RenderMode::Ivar => "Ivar",
        }
    }
}

/// Scene build status for async scene construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BuildStatus {
    /// Scene has not been built yet.
    #[default]
    NotStarted,
    /// Scene is currently being built in background thread.
    Building,
    /// Scene build completed successfully.
    Complete,
    /// Scene build failed.
    Failed,
}

/// Camera source for batch rendering.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum CameraSource {
    /// Use current viewport camera (free perspective orbit).
    #[default]
    Viewport,
    /// Use a specific USD camera by path (e.g., "/cameras/Camera").
    UsdCamera(String),
    /// Standard orthographic view preset.
    OrthoView(bif_math::OrthoPreset),
    /// Scene graph camera (from Camera primitive), by index into scene.cameras.
    SceneCamera(usize),
}

impl CameraSource {
    /// Display name for UI.
    pub fn display_name(&self) -> &str {
        match self {
            CameraSource::Viewport => "Perspective",
            CameraSource::UsdCamera(path) => path,
            CameraSource::OrthoView(preset) => preset.display_name(),
            CameraSource::SceneCamera(_) => "Scene Camera",
        }
    }
}

/// Per-AOV settings for batch rendering.
#[derive(Debug, Clone)]
pub struct AovSettings {
    /// Include alpha channel in EXR output.
    pub include_alpha: bool,
    /// Include depth (Z) channel in EXR output.
    pub include_depth: bool,
    /// Include normal (N.X, N.Y, N.Z) channels in EXR output.
    pub include_normal: bool,
    /// Near clipping distance for depth visualization.
    pub depth_near: f32,
    /// Far clipping distance for depth visualization.
    pub depth_far: f32,
    /// Auto-compute depth bounds from scene AABB.
    pub auto_depth_bounds: bool,
}

impl Default for AovSettings {
    fn default() -> Self {
        Self {
            include_alpha: true,
            include_depth: true,
            include_normal: true,
            depth_near: 0.01,
            depth_far: 10000.0,
            auto_depth_bounds: false,
        }
    }
}

/// Settings for batch rendering to disk.
#[derive(Debug, Clone)]
pub struct BatchRenderSettings {
    /// Start frame number.
    pub start_frame: i32,
    /// End frame number.
    pub end_frame: i32,
    /// Frame step (1 = every frame, 2 = every other frame, etc.).
    pub frame_step: i32,
    /// Output directory path.
    pub output_directory: String,
    /// Output filename pattern (e.g., "render.####.exr").
    pub output_pattern: String,
    /// Render width in pixels.
    pub resolution_x: u32,
    /// Render height in pixels.
    pub resolution_y: u32,
    /// Samples per pixel for quality.
    pub samples_per_pixel: u32,
    /// Maximum ray bounce depth.
    pub max_depth: u32,
    /// Per-AOV settings (alpha, depth, normal).
    pub aov_settings: AovSettings,
    /// EXR compression setting.
    pub compression: bif_renderer::ExrCompression,
    /// Camera source (viewport or USD camera).
    pub camera_source: CameraSource,
}

impl Default for BatchRenderSettings {
    fn default() -> Self {
        Self {
            start_frame: 1,
            end_frame: 100,
            frame_step: 1,
            output_directory: String::new(),
            output_pattern: "render.####.exr".to_string(),
            resolution_x: 1920,
            resolution_y: 1080,
            samples_per_pixel: 64,
            max_depth: 8,
            aov_settings: AovSettings::default(),
            compression: bif_renderer::ExrCompression::default(),
            camera_source: CameraSource::default(),
        }
    }
}

impl BatchRenderSettings {
    /// Calculate total number of frames to render.
    pub fn frame_count(&self) -> i32 {
        if self.frame_step <= 0 || self.end_frame < self.start_frame {
            return 0;
        }
        (self.end_frame - self.start_frame) / self.frame_step + 1
    }
}

/// Status of a batch render job.
#[derive(Debug, Clone, Default)]
pub enum BatchRenderStatus {
    /// No batch render in progress.
    #[default]
    Idle,
    /// Rendering frames.
    Rendering {
        /// Current frame being rendered.
        current_frame: i32,
        /// Total frames to render.
        total_frames: i32,
        /// Progress within current frame (0.0 - 1.0).
        frame_progress: f32,
    },
    /// Render completed successfully.
    Complete {
        /// Total time taken in seconds.
        total_elapsed_secs: f32,
    },
    /// Render was cancelled by user.
    Cancelled,
    /// Render failed with error.
    Failed(String),
}

impl BatchRenderStatus {
    /// Check if batch render is currently active.
    pub fn is_rendering(&self) -> bool {
        matches!(self, BatchRenderStatus::Rendering { .. })
    }

    /// Get overall progress percentage (0-100).
    pub fn overall_progress(&self) -> f32 {
        match self {
            BatchRenderStatus::Rendering {
                current_frame,
                total_frames,
                frame_progress,
            } => {
                if *total_frames <= 0 {
                    return 0.0;
                }
                let frames_done = (*current_frame - 1) as f32;
                ((frames_done + frame_progress) / *total_frames as f32) * 100.0
            }
            BatchRenderStatus::Complete { .. } => 100.0,
            _ => 0.0,
        }
    }
}

/// Snapshot of camera state for dirty detection.
#[derive(Debug, Clone, Copy)]
pub struct CameraSnapshot {
    pub position: Vec3,
    pub target: Vec3,
    pub fov_y: f32,
}

impl Default for CameraSnapshot {
    fn default() -> Self {
        // Use extreme values that will always trigger a change detection
        Self {
            position: Vec3::splat(f32::MAX),
            target: Vec3::splat(f32::MAX),
            fov_y: f32::MAX,
        }
    }
}

impl CameraSnapshot {
    /// Create snapshot from viewport camera.
    pub fn from_camera(camera: &Camera) -> Self {
        Self {
            position: camera.position,
            target: camera.target,
            fov_y: camera.fov_y,
        }
    }

    /// Check if camera has changed significantly.
    pub fn has_changed(&self, other: &Self) -> bool {
        const EPSILON: f32 = 0.0001;
        (self.position - other.position).length() > EPSILON
            || (self.target - other.target).length() > EPSILON
            || (self.fov_y - other.fov_y).abs() > EPSILON
    }
}

/// Message from Ivar background render thread.
#[derive(Debug)]
pub enum IvarMessage {
    /// A bucket has been completed with AOV data.
    BucketComplete(BucketResultWithAovs),
    /// Entire render is complete.
    RenderComplete { elapsed_secs: f32 },
    /// A single progressive pass is complete.
    PassComplete { pass_number: u32 },
    /// Render was cancelled.
    Cancelled,
}

/// Minimum interval (ms) between Ivar render restarts during interaction.
const RESTART_THROTTLE_MS: u64 = 50;

/// State for Ivar progressive rendering.
pub struct IvarState {
    /// Current render mode.
    pub mode: RenderMode,
    /// Accumulated image buffer.
    pub image_buffer: Option<ImageBuffer>,
    /// List of buckets for current render (Arc-shared with render threads).
    pub buckets: Arc<Vec<Bucket>>,
    /// Number of buckets completed.
    pub buckets_completed: usize,
    /// Whether render is complete.
    pub render_complete: bool,
    /// Cancel flag for background thread.
    pub cancel_flag: Arc<AtomicBool>,
    /// Receiver for bucket completion messages.
    pub receiver: Option<mpsc::Receiver<IvarMessage>>,
    /// Last camera snapshot for dirty detection.
    pub last_camera_snapshot: Option<CameraSnapshot>,
    /// Time when render started.
    pub render_start_time: Option<Instant>,
    /// Cached world geometry (BVH of triangles).
    /// TODO: Invalidate world cache when scene is reloaded or modified
    /// TODO: Add "Rebuild Scene" button to manually invalidate cached BVH
    pub world: Option<Arc<BvhNode>>,
    /// Scene build status for async construction.
    pub build_status: BuildStatus,
    /// Receiver for scene build completion.
    pub build_receiver: Option<mpsc::Receiver<Arc<BvhNode>>>,
    /// Samples per pixel for batch rendering.
    pub samples_per_pixel: u32,
    /// Max bounce depth.
    pub max_depth: u32,
    /// HDRI environment for lighting.
    pub environment: Option<Arc<bif_renderer::HdriEnvironment>>,
    /// Current HDRI rotation override (radians) for live updates.
    pub hdri_rotation: f32,
    /// Current HDRI intensity override for live updates.
    pub hdri_intensity: f32,
    /// Batch render settings.
    pub batch_settings: BatchRenderSettings,
    /// Batch render status.
    pub batch_status: BatchRenderStatus,
    /// AOV channel to preview in viewport.
    pub preview_aov: AovChannel,
    /// Alpha buffer for AOV preview (stored per pixel).
    pub alpha_buffer: Option<Vec<f32>>,
    /// Depth buffer for AOV preview (stored per pixel).
    pub depth_buffer: Option<Vec<f32>>,
    /// Normal buffer for AOV preview (stored per pixel as [x, y, z]).
    pub normal_buffer: Option<Vec<[f32; 3]>>,
    /// Running sum per pixel for progressive accumulation.
    pub accumulation_buffer: Option<Vec<Vec3>>,
    /// Number of completed progressive passes.
    pub accumulated_samples: u32,
    /// Target SPP for progressive rendering (render until this).
    pub target_spp: u32,
    /// Current render resolution divisor (1 = full, 2 = half, 4 = quarter, etc.).
    pub current_scale: u32,
    /// Navigation quality exponent: 1 = 1/2, 2 = 1/4, 3 = 1/8.
    /// Derive scale with `2u32.pow(interaction_quality)`.
    pub interaction_quality: u32,
    /// Last time camera or scene changed (for progressive refinement settle timer).
    pub last_interaction_time: Option<Instant>,
    /// Milliseconds to wait before refining to next resolution level.
    pub settle_timeout_ms: u32,
}

impl Default for IvarState {
    fn default() -> Self {
        Self {
            mode: RenderMode::Vulkan,
            image_buffer: None,
            buckets: Arc::new(Vec::new()),
            buckets_completed: 0,
            render_complete: false,
            cancel_flag: Arc::new(AtomicBool::new(false)),
            receiver: None,
            last_camera_snapshot: None,
            render_start_time: None,
            world: None,
            build_status: BuildStatus::NotStarted,
            build_receiver: None,
            samples_per_pixel: 16, // Lower for interactive preview
            max_depth: 8,
            environment: None,
            hdri_rotation: 0.0,
            hdri_intensity: 1.0,
            batch_settings: BatchRenderSettings::default(),
            batch_status: BatchRenderStatus::default(),
            preview_aov: AovChannel::default(),
            alpha_buffer: None,
            depth_buffer: None,
            normal_buffer: None,
            accumulation_buffer: None,
            accumulated_samples: 0,
            target_spp: 16,
            current_scale: 1,
            interaction_quality: 2,
            last_interaction_time: None,
            settle_timeout_ms: 300,
        }
    }
}

impl IvarState {
    /// Reset render state (call when starting new render).
    pub fn reset_render(&mut self, width: u32, height: u32) {
        // Cancel any existing render
        self.cancel_flag.store(true, Ordering::Relaxed);

        // Create new cancel flag
        self.cancel_flag = Arc::new(AtomicBool::new(false));

        // Clear state
        let pixel_count = (width * height) as usize;
        self.image_buffer = Some(ImageBuffer::new(width, height));
        self.buckets = Arc::new(generate_buckets(width, height, DEFAULT_BUCKET_SIZE));
        self.buckets_completed = 0;
        self.render_complete = false;
        self.receiver = None;
        self.render_start_time = Some(Instant::now());

        // Allocate AOV buffers
        self.alpha_buffer = Some(vec![0.0; pixel_count]);
        self.depth_buffer = Some(vec![f32::INFINITY; pixel_count]);
        self.normal_buffer = Some(vec![[0.0; 3]; pixel_count]);
    }

    /// Check if camera has moved and render needs restart.
    pub fn check_camera_dirty(&mut self, camera: &Camera) -> bool {
        let current = CameraSnapshot::from_camera(camera);

        match &self.last_camera_snapshot {
            Some(last) if !last.has_changed(&current) => false,
            _ => {
                self.last_camera_snapshot = Some(current);
                true
            }
        }
    }

    /// Get render progress as percentage.
    pub fn progress(&self) -> f32 {
        if self.buckets.is_empty() {
            return 0.0;
        }
        (self.buckets_completed as f32 / self.buckets.len() as f32) * 100.0
    }

    /// Get elapsed render time in seconds.
    pub fn elapsed_secs(&self) -> f32 {
        self.render_start_time
            .map(|t| t.elapsed().as_secs_f32())
            .unwrap_or(0.0)
    }

    /// Reset accumulation state for progressive rendering.
    ///
    /// Clears accumulation buffer and cancels any in-flight pass.
    /// Keeps BVH cached. Skips AOV buffers at reduced scale (not useful at low res).
    /// When dimensions change, resamples old image buffer (nearest-neighbor) instead
    /// of creating a black buffer to avoid flash.
    pub fn reset_accumulation(&mut self, width: u32, height: u32) {
        // Cancel any in-flight render
        self.cancel_flag.store(true, Ordering::Relaxed);
        self.cancel_flag = Arc::new(AtomicBool::new(false));
        self.receiver = None;

        let pixel_count = (width * height) as usize;

        // Reuse accumulation buffer when size matches, otherwise allocate
        let reuse_accum = self
            .accumulation_buffer
            .as_ref()
            .is_some_and(|buf| buf.len() == pixel_count);
        if reuse_accum {
            self.accumulation_buffer.as_mut().unwrap().fill(Vec3::ZERO);
        } else {
            self.accumulation_buffer = Some(vec![Vec3::ZERO; pixel_count]);
        }

        self.accumulated_samples = 0;
        self.render_complete = false;
        self.buckets_completed = 0;
        self.render_start_time = Some(Instant::now());

        // Keep existing display pixels when dimensions match (avoids black flash
        // during camera orbit / transform drag). On dimension change, resample
        // old buffer with nearest-neighbor (stale but visible, not black).
        let reuse = self
            .image_buffer
            .as_ref()
            .is_some_and(|img| img.width == width && img.height == height);
        if !reuse {
            self.image_buffer = if let Some(ref old) = self.image_buffer {
                // Nearest-neighbor resample: stale but visible, not black
                let mut buf = ImageBuffer::new(width, height);
                for y in 0..height {
                    for x in 0..width {
                        let src_x = (x as f32 * old.width as f32 / width as f32) as u32;
                        let src_y = (y as f32 * old.height as f32 / height as f32) as u32;
                        let idx = (src_y.min(old.height - 1) * old.width + src_x.min(old.width - 1))
                            as usize;
                        buf.pixels[(y * width + x) as usize] = old.pixels[idx];
                    }
                }
                Some(buf)
            } else {
                Some(ImageBuffer::new(width, height))
            };
        }

        // Only allocate AOV buffers at full resolution (not useful at low res).
        // Reuse with .fill() when size matches.
        if self.current_scale <= 1 {
            let reuse_alpha = self
                .alpha_buffer
                .as_ref()
                .is_some_and(|buf| buf.len() == pixel_count);
            if reuse_alpha {
                self.alpha_buffer.as_mut().unwrap().fill(0.0);
                self.depth_buffer.as_mut().unwrap().fill(f32::INFINITY);
                self.normal_buffer.as_mut().unwrap().fill([0.0; 3]);
            } else {
                self.alpha_buffer = Some(vec![0.0; pixel_count]);
                self.depth_buffer = Some(vec![f32::INFINITY; pixel_count]);
                self.normal_buffer = Some(vec![[0.0; 3]; pixel_count]);
            }
        } else {
            self.alpha_buffer = None;
            self.depth_buffer = None;
            self.normal_buffer = None;
        }
    }

    /// Check if more progressive passes are needed.
    pub fn needs_more_passes(&self) -> bool {
        self.accumulated_samples < self.target_spp
    }

    /// Check if a progressive pass is currently in flight.
    pub fn is_pass_in_flight(&self) -> bool {
        self.receiver.is_some() && !self.render_complete
    }

    /// Check if a restart is allowed (not throttled).
    ///
    /// Allows restart if min interval passed OR at least one bucket completed.
    /// Bucket check self-tunes: complex scenes take longer per bucket, so
    /// throttle naturally waits longer.
    pub fn should_restart(&self) -> bool {
        let elapsed_ms = self
            .render_start_time
            .map(|t| t.elapsed().as_millis() as u64)
            .unwrap_or(u64::MAX); // no render yet → always allow
        elapsed_ms >= RESTART_THROTTLE_MS || self.buckets_completed > 0
    }

    /// Derive interaction scale divisor from quality exponent.
    ///
    /// Quality 1 → scale 2, quality 2 → scale 4, quality 3 → scale 8.
    pub fn interaction_scale(&self) -> u32 {
        2u32.pow(self.interaction_quality)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_mode_display_name() {
        assert_eq!(RenderMode::Vulkan.display_name(), "Vulkan");
        assert_eq!(RenderMode::Ivar.display_name(), "Ivar");
    }

    #[test]
    fn test_camera_snapshot_has_changed() {
        let snap1 = CameraSnapshot {
            position: Vec3::ZERO,
            target: Vec3::Z,
            fov_y: 45.0,
        };
        let snap2 = CameraSnapshot {
            position: Vec3::X * 0.001, // small change
            target: Vec3::Z,
            fov_y: 45.0,
        };
        assert!(snap1.has_changed(&snap2));

        let snap3 = CameraSnapshot {
            position: Vec3::ZERO,
            target: Vec3::Z,
            fov_y: 45.0,
        };
        assert!(!snap1.has_changed(&snap3));
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn test_ivar_state_progress() {
        let mut state = IvarState::default();
        state.buckets = Arc::new(generate_buckets(100, 100, 32));
        let total = state.buckets.len();
        state.buckets_completed = total / 2;
        assert!((state.progress() - 50.0).abs() < 1.0);
    }

    #[test]
    fn test_ivar_state_progress_empty() {
        let state = IvarState::default();
        assert_eq!(state.progress(), 0.0);
    }

    #[test]
    fn test_build_status_default() {
        let state = IvarState::default();
        assert_eq!(state.build_status, BuildStatus::NotStarted);
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn test_batch_render_settings_frame_count() {
        let mut settings = BatchRenderSettings::default();
        settings.start_frame = 1;
        settings.end_frame = 10;
        settings.frame_step = 1;
        assert_eq!(settings.frame_count(), 10);

        settings.frame_step = 2;
        assert_eq!(settings.frame_count(), 5); // 1, 3, 5, 7, 9

        settings.start_frame = 1;
        settings.end_frame = 1;
        settings.frame_step = 1;
        assert_eq!(settings.frame_count(), 1);
    }

    #[test]
    fn test_batch_render_status_progress() {
        let status = BatchRenderStatus::Rendering {
            current_frame: 5,
            total_frames: 10,
            frame_progress: 0.5,
        };
        // (4 + 0.5) / 10 * 100 = 45%
        assert!((status.overall_progress() - 45.0).abs() < 0.1);

        let complete = BatchRenderStatus::Complete {
            total_elapsed_secs: 60.0,
        };
        assert_eq!(complete.overall_progress(), 100.0);

        let idle = BatchRenderStatus::Idle;
        assert_eq!(idle.overall_progress(), 0.0);
    }

    #[test]
    fn test_camera_source_display() {
        assert_eq!(CameraSource::Viewport.display_name(), "Perspective");
        assert_eq!(
            CameraSource::UsdCamera("/cameras/main".to_string()).display_name(),
            "/cameras/main"
        );
    }

    #[test]
    fn test_interaction_quality_to_scale() {
        let mut state = IvarState::default();
        state.interaction_quality = 1;
        assert_eq!(state.interaction_scale(), 2);
        state.interaction_quality = 2;
        assert_eq!(state.interaction_scale(), 4);
        state.interaction_quality = 3;
        assert_eq!(state.interaction_scale(), 8);
    }

    #[test]
    fn test_default_interaction_quality() {
        let state = IvarState::default();
        assert_eq!(state.interaction_quality, 2); // Default: 1/4 scale
        assert_eq!(state.interaction_scale(), 4);
        assert_eq!(state.current_scale, 1); // Start at full res
    }

    #[test]
    fn test_reset_accumulation_skips_aov_at_reduced_scale() {
        let mut state = IvarState::default();

        // Full res: AOVs allocated
        state.current_scale = 1;
        state.reset_accumulation(100, 100);
        assert!(state.alpha_buffer.is_some());
        assert!(state.depth_buffer.is_some());
        assert!(state.normal_buffer.is_some());

        // Reduced scale: AOVs skipped
        state.current_scale = 4;
        state.reset_accumulation(25, 25);
        assert!(state.alpha_buffer.is_none());
        assert!(state.depth_buffer.is_none());
        assert!(state.normal_buffer.is_none());
    }

    #[test]
    fn test_reset_accumulation_clears_state() {
        let mut state = IvarState::default();
        state.accumulated_samples = 10;
        state.render_complete = true;
        state.buckets_completed = 42;
        state.current_scale = 1;

        state.reset_accumulation(100, 100);

        assert_eq!(state.accumulated_samples, 0);
        assert!(!state.render_complete);
        assert_eq!(state.buckets_completed, 0);
        assert!(state.image_buffer.is_some());
        assert!(state.accumulation_buffer.is_some());
        assert!(state.render_start_time.is_some());
    }

    #[test]
    fn test_settle_timeout_default() {
        let state = IvarState::default();
        assert_eq!(state.settle_timeout_ms, 300);
    }

    #[test]
    fn test_reset_accumulation_reuses_buffer_when_dims_match() {
        let mut state = IvarState::default();
        state.current_scale = 1;

        // First reset creates a fresh buffer
        state.reset_accumulation(100, 100);
        // Write a non-zero pixel so we can verify it survives
        let marker = Vec3::new(0.8, 0.4, 0.2);
        if let Some(buf) = state.image_buffer.as_mut() {
            buf.pixels[0] = marker;
        }

        // Second reset at same dims should keep existing pixels
        state.reset_accumulation(100, 100);
        let buf = state.image_buffer.as_ref().unwrap();
        assert_eq!(buf.width, 100);
        assert_eq!(buf.height, 100);
        assert_eq!(
            buf.pixels[0], marker,
            "pixel should survive same-size reset"
        );
    }

    #[test]
    fn test_reset_accumulation_resamples_buffer_when_dims_differ() {
        let mut state = IvarState::default();
        state.current_scale = 1;

        state.reset_accumulation(100, 100);
        let marker = Vec3::new(0.8, 0.4, 0.2);
        if let Some(buf) = state.image_buffer.as_mut() {
            buf.pixels[0] = marker;
        }

        // Different dimensions → nearest-neighbor resample (not black)
        state.reset_accumulation(200, 200);
        let buf = state.image_buffer.as_ref().unwrap();
        assert_eq!(buf.width, 200);
        assert_eq!(buf.height, 200);
        // Top-left pixel should be resampled from old top-left
        assert_eq!(
            buf.pixels[0], marker,
            "pixel should be resampled, not black"
        );
    }

    #[test]
    fn test_needs_more_passes() {
        let mut state = IvarState::default();
        state.target_spp = 16;

        state.accumulated_samples = 0;
        assert!(state.needs_more_passes());

        state.accumulated_samples = 15;
        assert!(state.needs_more_passes());

        state.accumulated_samples = 16;
        assert!(!state.needs_more_passes());

        state.accumulated_samples = 17;
        assert!(!state.needs_more_passes());
    }

    #[test]
    fn test_reset_accumulation_creates_buffer_when_none() {
        let mut state = IvarState::default();
        state.current_scale = 1;
        assert!(state.image_buffer.is_none());

        state.reset_accumulation(100, 100);

        let buf = state.image_buffer.as_ref().unwrap();
        assert_eq!(buf.width, 100);
        assert_eq!(buf.height, 100);
        assert_eq!(buf.pixels[0], Vec3::ZERO, "new buffer should be zeroed");
    }

    #[test]
    fn test_should_restart_no_render() {
        let state = IvarState::default();
        // No render_start_time → always allow
        assert!(state.should_restart());
    }

    #[test]
    fn test_should_restart_with_completed_bucket() {
        let mut state = IvarState::default();
        state.render_start_time = Some(Instant::now());
        state.buckets_completed = 1;
        // Bucket completed → allow restart regardless of time
        assert!(state.should_restart());
    }
}
