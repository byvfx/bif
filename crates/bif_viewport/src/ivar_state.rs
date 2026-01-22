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
    generate_buckets, Bucket, BucketResult, BvhNode, ImageBuffer, DEFAULT_BUCKET_SIZE,
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

/// Snapshot of camera state for dirty detection.
#[derive(Debug, Clone, Copy)]
pub struct CameraSnapshot {
    pub position: Vec3,
    pub target: Vec3,
    pub fov_y: f32,
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
    /// A bucket has been completed.
    BucketComplete(BucketResult),
    /// Entire render is complete.
    RenderComplete { elapsed_secs: f32 },
    /// Render was cancelled.
    Cancelled,
}

/// State for Ivar progressive rendering.
pub struct IvarState {
    /// Current render mode.
    pub mode: RenderMode,
    /// Accumulated image buffer.
    pub image_buffer: Option<ImageBuffer>,
    /// List of buckets for current render.
    pub buckets: Vec<Bucket>,
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
    /// Samples per pixel for rendering.
    /// TODO: Expose SPP in UI
    pub samples_per_pixel: u32,
    /// Max bounce depth.
    pub max_depth: u32,
}

impl Default for IvarState {
    fn default() -> Self {
        Self {
            mode: RenderMode::Vulkan,
            image_buffer: None,
            buckets: Vec::new(),
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
        self.image_buffer = Some(ImageBuffer::new(width, height));
        self.buckets = generate_buckets(width, height, DEFAULT_BUCKET_SIZE);
        self.buckets_completed = 0;
        self.render_complete = false;
        self.receiver = None;
        self.render_start_time = Some(Instant::now());
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
    fn test_ivar_state_progress() {
        let mut state = IvarState::default();
        state.buckets = generate_buckets(100, 100, 32);
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
}
