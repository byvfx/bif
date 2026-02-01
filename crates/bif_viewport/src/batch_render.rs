//! Batch render to disk functionality.
//!
//! Renders frame sequences to EXR files with optional AOVs (depth, normals).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};

use bif_core::usd::cpp_bridge::UsdStage;
use bif_math::{Mat4, Vec3};
use bif_renderer::{
    format_frame_path, generate_buckets, render_bucket_with_aovs, write_exr, BvhNode, Camera,
    Color, DisneyBSDF, EmbreeScene, ExrOutput, HdriEnvironment, Hittable, RenderConfig,
    DEFAULT_BUCKET_SIZE,
};
use rayon::prelude::*;

use crate::gpu_types::Vertex;
use crate::ivar_state::{AovSettings, BatchRenderSettings, CameraSource};

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

/// Scene builder function type for per-frame geometry updates.
pub type SceneBuilderFn = Box<dyn Fn(f64) -> Arc<BvhNode> + Send + Sync>;

/// Triangle data type: (vertices, UVs, normals).
pub type TriangleData = (Vec<[Vec3; 3]>, Vec<[[f32; 2]; 3]>, Vec<[[f32; 3]; 3]>);

/// Data needed to rebuild the Embree scene for animated geometry.
#[derive(Clone)]
pub struct SceneBuilderData {
    /// Mesh vertices (GPU format with positions, normals, UVs).
    pub vertices: Vec<Vertex>,
    /// Triangle indices.
    pub indices: Vec<u32>,
    /// Per-triangle material IDs.
    pub triangle_material_ids: Option<Vec<u32>>,
    /// Scene materials (from USD).
    pub scene_materials: Vec<Arc<bif_core::Material>>,
    /// Fallback material.
    pub scene_material: bif_core::Material,
    /// Texture base directory.
    pub texture_base_dir: Option<PathBuf>,
    /// Instance transforms.
    pub instance_transforms: Vec<Mat4>,
    /// Whether using multi-draw mode (transforms baked into vertices).
    pub use_multi_draw: bool,
    /// Mesh indices with vertex animation.
    pub vertex_animated_meshes: Vec<usize>,
    /// USD stage for querying animated vertices.
    pub stage: Option<Arc<UsdStage>>,
}

impl SceneBuilderData {
    /// Build triangles at a specific time, querying USD for animated vertices.
    pub fn build_triangles_at_time(&self, time: f64) -> TriangleData {
        let tri_count = self.indices.len() / 3;
        let mut triangle_vertices = Vec::with_capacity(tri_count);
        let mut triangle_uvs: Vec<[[f32; 2]; 3]> = Vec::with_capacity(tri_count);
        let mut triangle_normals: Vec<[[f32; 3]; 3]> = Vec::with_capacity(tri_count);

        // Query animated vertices if applicable and update positions in-place
        // Clone vertices so we can update positions while keeping UVs/normals
        let vertices: Vec<Vertex> = if self.vertex_animated_meshes.is_empty()
            || self.use_multi_draw
            || self.vertex_animated_meshes.len() != 1
        {
            // No animation or unsupported config - use static vertices
            self.vertices.clone()
        } else {
            let mesh_idx = self.vertex_animated_meshes[0];
            match self
                .stage
                .as_ref()
                .and_then(|stage| stage.get_mesh_vertices_at_time(mesh_idx, time).ok())
            {
                Some(positions) => {
                    let vertex_count = positions.len() / 3;
                    if vertex_count != self.vertices.len() {
                        log::warn!(
                            "Vertex count mismatch: USD {} vs mesh {} - using static",
                            vertex_count,
                            self.vertices.len()
                        );
                        self.vertices.clone()
                    } else {
                        // Update positions while preserving UVs/normals
                        let mut updated = self.vertices.clone();
                        for (i, v) in updated.iter_mut().enumerate() {
                            v.position = [
                                positions[i * 3],
                                positions[i * 3 + 1],
                                positions[i * 3 + 2],
                            ];
                        }
                        updated
                    }
                }
                None => self.vertices.clone(),
            }
        };

        for i in (0..self.indices.len()).step_by(3) {
            let i0 = self.indices[i] as usize;
            let i1 = self.indices[i + 1] as usize;
            let i2 = self.indices[i + 2] as usize;

            triangle_vertices.push([
                Vec3::from_array(vertices[i0].position),
                Vec3::from_array(vertices[i1].position),
                Vec3::from_array(vertices[i2].position),
            ]);

            triangle_uvs.push([vertices[i0].uv, vertices[i1].uv, vertices[i2].uv]);

            triangle_normals.push([vertices[i0].normal, vertices[i1].normal, vertices[i2].normal]);
        }

        (triangle_vertices, triangle_uvs, triangle_normals)
    }

    /// Build Embree scene at a specific time.
    pub fn build_scene_at_time(&self, time: f64) -> Arc<BvhNode> {
        let (triangle_vertices, triangle_uvs, triangle_normals) =
            self.build_triangles_at_time(time);

        // Load materials with textures
        let mut texture_cache = match &self.texture_base_dir {
            Some(dir) => bif_core::texture::TextureCache::with_base_dir(dir.clone()),
            None => bif_core::texture::TextureCache::new(),
        };

        let materials: Vec<Arc<DisneyBSDF>> = if self.scene_materials.is_empty() {
            vec![Arc::new(DisneyBSDF::from_material_with_textures(
                &self.scene_material,
                &mut texture_cache,
            ))]
        } else {
            self.scene_materials
                .iter()
                .map(|mat| {
                    Arc::new(DisneyBSDF::from_material_with_textures(
                        mat.as_ref(),
                        &mut texture_cache,
                    ))
                })
                .collect()
        };

        let tri_mat_ids: Vec<u32> = self
            .triangle_material_ids
            .as_ref()
            .cloned()
            .unwrap_or_default();

        // Use identity if multi-draw (transforms already baked)
        let ivar_transforms = if self.use_multi_draw {
            vec![Mat4::IDENTITY]
        } else {
            self.instance_transforms.clone()
        };

        if let Some(embree_scene) = EmbreeScene::try_new(
            &triangle_vertices,
            &triangle_uvs,
            &triangle_normals,
            ivar_transforms,
            materials,
            &tri_mat_ids,
        ) {
            let objects: Vec<Box<dyn Hittable + Send + Sync>> = vec![Box::new(embree_scene)];
            Arc::new(BvhNode::new(objects))
        } else {
            log::warn!("Embree not available for animated scene rebuild");
            Arc::new(BvhNode::new(vec![]))
        }
    }
}

/// Scene data required for batch rendering.
pub struct BatchSceneData {
    /// Pre-built BVH for static scenes.
    pub world: Arc<BvhNode>,
    /// HDRI environment for lighting (optional).
    pub environment: Option<Arc<HdriEnvironment>>,
    /// USD stage for camera and animation queries (optional).
    pub stage: Option<Arc<UsdStage>>,
    /// Viewport camera (used if CameraSource::Viewport).
    pub viewport_camera: bif_math::Camera,
    /// Whether the scene has animated geometry requiring per-frame BVH rebuild.
    pub has_animated_geometry: bool,
    /// Scene builder for animated geometry (called per frame if has_animated_geometry).
    pub scene_builder: Option<SceneBuilderFn>,
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
        background: Color::new(0.1, 0.1, 0.1),
        use_sky_gradient: true, // Fallback lighting if no HDRI
        environment: scene.environment.clone(),
    };

    log::info!(
        "Batch render: camera_source={}, stage_present={}, animated_geometry={}",
        settings.camera_source.display_name(),
        scene.stage.is_some(),
        scene.has_animated_geometry
    );

    // Track current world (may be rebuilt per frame for animated geometry)
    let mut current_world = scene.world.clone();

    // Render each frame
    for (frame_idx, &frame) in frames.iter().enumerate() {
        if cancel_flag.load(Ordering::Relaxed) {
            let _ = tx.send(BatchMessage::Cancelled);
            return;
        }

        let frame_start = std::time::Instant::now();

        // Rebuild scene for animated geometry (skip first frame, already built)
        if scene.has_animated_geometry && frame_idx > 0 {
            if let Some(ref builder) = scene.scene_builder {
                log::info!("Rebuilding BVH for frame {}", frame);
                let rebuild_start = std::time::Instant::now();
                // Drop old scene before building new one to free Embree resources
                drop(current_world);
                current_world = builder(frame as f64);
                log::debug!(
                    "BVH rebuilt in {:.2}ms",
                    rebuild_start.elapsed().as_secs_f64() * 1000.0
                );
            }
        }

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
            &current_world,
            &render_config,
            settings.resolution_x,
            settings.resolution_y,
            &settings.aov_settings,
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
            // Note: viewport camera stores fov_y in radians, renderer expects degrees
            let vc = &scene.viewport_camera;
            let focus_distance = (vc.target - vc.position).length();
            let mut camera = Camera::new()
                .with_resolution(width, height)
                .with_lens(vc.fov_y.to_degrees(), 0.0, focus_distance)
                .with_position(vc.position, vc.target, vc.up);
            camera.initialize();
            camera
        }
        CameraSource::UsdCamera(path) => {
            // Query USD stage for camera transform at time
            if let Some(ref stage) = scene.stage {
                match stage.get_camera_xform_at_time(path, time) {
                    Ok(xform) => {
                        log::info!(
                            "USD camera '{}' at frame {}: pos={:?}",
                            path,
                            time,
                            xform.col(3).truncate()
                        );
                        return camera_from_usd_transform(xform, width, height);
                    }
                    Err(e) => {
                        log::warn!("Failed to get USD camera xform: {:?}", e);
                    }
                }
            } else {
                log::warn!("No USD stage available for camera query");
            }
            // Fallback to viewport camera if USD query fails
            log::warn!("Falling back to viewport camera");
            let vc = &scene.viewport_camera;
            let focus_distance = (vc.target - vc.position).length();
            let mut camera = Camera::new()
                .with_resolution(width, height)
                .with_lens(vc.fov_y.to_degrees(), 0.0, focus_distance)
                .with_position(vc.position, vc.target, vc.up);
            camera.initialize();
            camera
        }
    }
}

/// Create an Ivar camera from a USD transform matrix.
fn camera_from_usd_transform(xform: Mat4, width: u32, height: u32) -> Camera {
    // USD stores translation in row 3, not column 3
    let position = xform.row(3).truncate();

    // Extract forward direction (negative Z in camera space)
    // USD row-major: row 2 is the Z axis
    let forward = -Vec3::new(xform.row(0).z, xform.row(1).z, xform.row(2).z).normalize();

    // Target is position + forward (scale forward to reasonable distance)
    let target = position + forward * 10.0;

    // Extract up vector (Y axis) from rows
    let up = Vec3::new(xform.row(0).y, xform.row(1).y, xform.row(2).y).normalize();

    // Default FOV (USD cameras have focal length, but we use a reasonable default)
    let fov_y = 45.0_f32;

    log::debug!(
        "USD camera: pos={:?}, target={:?}, up={:?}, fov={}",
        position, target, up, fov_y
    );

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
    aov_settings: &AovSettings,
    cancel_flag: &Arc<AtomicBool>,
    progress_callback: F,
) -> ExrOutput
where
    F: Fn(f32) + Send + Sync,
{
    let buckets = generate_buckets(width, height, DEFAULT_BUCKET_SIZE);
    let total_buckets = buckets.len();

    // Allocate output buffers based on per-AOV settings
    let pixel_count = (width * height) as usize;
    let mut beauty = vec![Color::ZERO; pixel_count];
    let mut alpha = if aov_settings.include_alpha {
        Some(vec![0.0f32; pixel_count])
    } else {
        None
    };
    let mut depth = if aov_settings.include_depth {
        Some(vec![f32::INFINITY; pixel_count])
    } else {
        None
    };
    let mut normal = if aov_settings.include_normal {
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

                if let Some(ref mut a) = alpha {
                    a[global_idx] = result.alphas[local_idx];
                }
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
        alpha,
        depth,
        normal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_camera_from_usd_transform() {
        // Identity matrix should create a valid camera
        let xform = Mat4::IDENTITY;
        let camera = camera_from_usd_transform(xform, 100, 100);

        // Verify camera was created with correct resolution
        assert_eq!(camera.image_width, 100);
        assert_eq!(camera.image_height, 100);
    }
}
