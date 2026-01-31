//! Batch render to disk functionality.
//!
//! Renders frame sequences to EXR files with optional AOVs (depth, normals).

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};

use bif_core::usd::cpp_bridge::UsdStage;
use bif_math::Mat4;
use bif_renderer::{
    format_frame_path, generate_buckets, render_bucket_with_aovs, write_exr, BvhNode, Camera,
    Color, ExrOutput, HdriEnvironment, RenderConfig, DEFAULT_BUCKET_SIZE,
};
use rayon::prelude::*;

use crate::ivar_state::{BatchRenderSettings, CameraSource};

/// Message sent during batch render.
#[derive(Debug)]
pub enum BatchMessage {
    /// Progress update.
    Progress {
        current_frame: i32,
        total_frames: i32,
        frame_progress: f32,
    },
    /// Frame completed successfully.
    FrameComplete { frame: i32, elapsed_secs: f32 },
    /// Batch render finished.
    Complete { total_elapsed_secs: f32 },
    /// Render was cancelled.
    Cancelled,
    /// Error occurred.
    Error(String),
}

/// Scene data required for batch rendering.
pub struct BatchSceneData {
    /// Pre-built BVH for the scene.
    pub world: Arc<BvhNode>,
    /// HDRI environment for lighting (optional).
    pub environment: Option<Arc<HdriEnvironment>>,
    /// USD stage for camera and animation queries (optional).
    pub stage: Option<Arc<UsdStage>>,
    /// Viewport camera (used if CameraSource::Viewport).
    pub viewport_camera: bif_math::Camera,
}

/// Start a batch render in a background thread.
///
/// Returns a receiver for progress messages and a cancel flag.
pub fn start_batch_render(
    settings: BatchRenderSettings,
    scene: BatchSceneData,
) -> (mpsc::Receiver<BatchMessage>, Arc<AtomicBool>) {
    let (tx, rx) = mpsc::channel();
    let cancel_flag = Arc::new(AtomicBool::new(false));
    let cancel_flag_clone = cancel_flag.clone();

    std::thread::spawn(move || {
        batch_render_loop(settings, scene, tx, cancel_flag_clone);
    });

    (rx, cancel_flag)
}

/// Main batch render loop.
fn batch_render_loop(
    settings: BatchRenderSettings,
    scene: BatchSceneData,
    tx: mpsc::Sender<BatchMessage>,
    cancel_flag: Arc<AtomicBool>,
) {
    let start_time = std::time::Instant::now();

    // Calculate frame list
    let frames: Vec<i32> = (settings.start_frame..=settings.end_frame)
        .step_by(settings.frame_step.max(1) as usize)
        .collect();
    let total_frames = frames.len() as i32;

    if total_frames == 0 {
        let _ = tx.send(BatchMessage::Error("No frames to render".to_string()));
        return;
    }

    // Build render config
    let render_config = RenderConfig {
        samples_per_pixel: settings.samples_per_pixel,
        max_depth: settings.max_depth,
        background: Color::ZERO,
        use_sky_gradient: false,
        environment: scene.environment.clone(),
    };

    // Render each frame
    for (frame_idx, &frame) in frames.iter().enumerate() {
        if cancel_flag.load(Ordering::Relaxed) {
            let _ = tx.send(BatchMessage::Cancelled);
            return;
        }

        let frame_start = std::time::Instant::now();

        // Build camera for this frame
        let camera = build_camera_for_frame(
            &settings.camera_source,
            frame as f64,
            settings.resolution_x,
            settings.resolution_y,
            &scene,
        );

        // Notify progress (starting frame)
        let _ = tx.send(BatchMessage::Progress {
            current_frame: frame_idx as i32 + 1,
            total_frames,
            frame_progress: 0.0,
        });

        // Render frame with AOVs
        let result = render_frame_with_aovs(
            &camera,
            &scene.world,
            &render_config,
            settings.resolution_x,
            settings.resolution_y,
            settings.include_aovs,
            &cancel_flag,
            |progress| {
                let _ = tx.send(BatchMessage::Progress {
                    current_frame: frame_idx as i32 + 1,
                    total_frames,
                    frame_progress: progress,
                });
            },
        );

        if cancel_flag.load(Ordering::Relaxed) {
            let _ = tx.send(BatchMessage::Cancelled);
            return;
        }

        // Build output path
        let filename = format_frame_path(&settings.output_pattern, frame);
        let output_path = Path::new(&settings.output_directory).join(&filename);

        // Write EXR
        if let Err(e) = write_exr(&result, &output_path, settings.compression) {
            let _ = tx.send(BatchMessage::Error(format!(
                "Failed to write {}: {}",
                output_path.display(),
                e
            )));
            return;
        }

        let frame_elapsed = frame_start.elapsed().as_secs_f32();
        let _ = tx.send(BatchMessage::FrameComplete {
            frame,
            elapsed_secs: frame_elapsed,
        });
    }

    let total_elapsed = start_time.elapsed().as_secs_f32();
    let _ = tx.send(BatchMessage::Complete {
        total_elapsed_secs: total_elapsed,
    });
}

/// Build the Ivar camera for a specific frame.
fn build_camera_for_frame(
    source: &CameraSource,
    time: f64,
    width: u32,
    height: u32,
    scene: &BatchSceneData,
) -> Camera {
    match source {
        CameraSource::Viewport => {
            // Use viewport camera settings
            let vc = &scene.viewport_camera;
            let mut camera = Camera::new()
                .with_resolution(width, height)
                .with_lens(vc.fov_y, 0.0, 1.0)
                .with_position(vc.position, vc.target, vc.up);
            camera.initialize();
            camera
        }
        CameraSource::UsdCamera(path) => {
            // Query USD stage for camera transform at time
            if let Some(ref stage) = scene.stage {
                if let Ok(xform) = stage.get_camera_xform_at_time(path, time) {
                    return camera_from_usd_transform(xform, width, height);
                }
            }
            // Fallback to viewport camera if USD query fails
            let vc = &scene.viewport_camera;
            let mut camera = Camera::new()
                .with_resolution(width, height)
                .with_lens(vc.fov_y, 0.0, 1.0)
                .with_position(vc.position, vc.target, vc.up);
            camera.initialize();
            camera
        }
    }
}

/// Create an Ivar camera from a USD transform matrix.
fn camera_from_usd_transform(xform: Mat4, width: u32, height: u32) -> Camera {
    // Extract position from the matrix (translation is in the 4th column)
    let position = xform.col(3).truncate();

    // Extract forward direction (negative Z in camera space)
    // USD cameras look down -Z, so we negate the Z column
    let forward = -xform.col(2).truncate().normalize();

    // Target is position + forward
    let target = position + forward;

    // Extract up vector from Y column
    let up = xform.col(1).truncate().normalize();

    // Default FOV (USD cameras have focal length, but we use a reasonable default)
    let fov_y = 45.0_f32;

    let mut camera = Camera::new()
        .with_resolution(width, height)
        .with_lens(fov_y, 0.0, 1.0)
        .with_position(position, target, up);
    camera.initialize();
    camera
}

/// Render a single frame with AOV capture.
#[allow(clippy::too_many_arguments)]
fn render_frame_with_aovs<F>(
    camera: &Camera,
    world: &Arc<BvhNode>,
    config: &RenderConfig,
    width: u32,
    height: u32,
    include_aovs: bool,
    cancel_flag: &Arc<AtomicBool>,
    progress_callback: F,
) -> ExrOutput
where
    F: Fn(f32) + Send + Sync,
{
    let buckets = generate_buckets(width, height, DEFAULT_BUCKET_SIZE);
    let total_buckets = buckets.len();

    // Allocate output buffers
    let pixel_count = (width * height) as usize;
    let mut beauty = vec![Color::ZERO; pixel_count];
    let mut depth = if include_aovs {
        Some(vec![f32::INFINITY; pixel_count])
    } else {
        None
    };
    let mut normal = if include_aovs {
        Some(vec![[0.0f32; 3]; pixel_count])
    } else {
        None
    };

    // Track completed buckets for progress
    let completed = std::sync::atomic::AtomicUsize::new(0);

    // Render buckets in parallel
    let results: Vec<_> = buckets
        .par_iter()
        .filter_map(|bucket| {
            if cancel_flag.load(Ordering::Relaxed) {
                return None;
            }

            let result = render_bucket_with_aovs(bucket, camera, world.as_ref(), config);

            // Update progress
            let done = completed.fetch_add(1, Ordering::Relaxed) + 1;
            progress_callback(done as f32 / total_buckets as f32);

            Some(result)
        })
        .collect();

    // Copy results to output buffers
    for result in results {
        let bucket = &result.bucket;
        for local_y in 0..bucket.height {
            for local_x in 0..bucket.width {
                let global_x = bucket.x + local_x;
                let global_y = bucket.y + local_y;
                let global_idx = (global_y * width + global_x) as usize;
                let local_idx = (local_y * bucket.width + local_x) as usize;

                beauty[global_idx] = result.pixels[local_idx];

                if let Some(ref mut d) = depth {
                    d[global_idx] = result.depths[local_idx];
                }
                if let Some(ref mut n) = normal {
                    n[global_idx] = result.normals[local_idx];
                }
            }
        }
    }

    ExrOutput {
        width,
        height,
        beauty,
        depth,
        normal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_camera_from_usd_transform() {
        // Identity matrix should give camera at origin looking down -Z
        let xform = Mat4::IDENTITY;
        let camera = camera_from_usd_transform(xform, 100, 100);

        // Position should be at origin
        assert!(camera.origin.length() < 0.001);
    }
}
