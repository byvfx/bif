//! Batch render to disk functionality.
//!
//! Renders frame sequences to EXR files with optional AOVs (depth, normals).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};

use bif_core::usd::cpp_bridge::UsdStage;
use bif_math::Mat4;
use bif_renderer::{
    format_frame_path, generate_buckets, render_bucket_with_aovs, write_exr, BvhNode, Camera,
    Color, DisneyBSDF, EmbreeScene, ExrOutput, HdriEnvironment, Hittable, LightList, RenderConfig,
    DEFAULT_BUCKET_SIZE,
};
use rayon::prelude::*;

use crate::gpu_types::Vertex;
use crate::ivar_state::{AovSettings, BatchRenderSettings, CameraSource};
use crate::mesh_data::MeshRange;

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

/// Build DisneyBSDF materials from scene materials, loading textures from disk.
///
/// Shared helper to avoid duplicating material construction logic.
pub fn build_materials(
    scene_materials: &[Arc<bif_core::Material>],
    fallback: &bif_core::Material,
    texture_base_dir: Option<&Path>,
) -> Vec<Arc<DisneyBSDF>> {
    let mut texture_cache = match texture_base_dir {
        Some(dir) => bif_core::texture::TextureCache::with_base_dir(dir.to_path_buf()),
        None => bif_core::texture::TextureCache::new(),
    };
    let materials = if scene_materials.is_empty() {
        vec![Arc::new(DisneyBSDF::from_material_with_textures(
            fallback,
            &mut texture_cache,
        ))]
    } else {
        scene_materials
            .iter()
            .map(|mat| {
                Arc::new(DisneyBSDF::from_material_with_textures(
                    mat.as_ref(),
                    &mut texture_cache,
                ))
            })
            .collect()
    };
    log::info!(
        "Built {} materials (textures cached: {})",
        materials.len(),
        texture_cache.len()
    );
    materials
}

/// Scene builder function type for per-frame geometry updates.
///
/// `FnMut` because `SceneBuilderData::build_scene_at_time` caches materials internally.
/// Batch render calls this sequentially per frame — no concurrent access needed.
pub type SceneBuilderFn = Box<dyn FnMut(f64) -> Arc<BvhNode> + Send>;

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
    /// Instance transforms (static, base transforms).
    pub instance_transforms: Vec<Mat4>,
    /// Instance animations (parallel to instance_transforms, None = static).
    pub instance_animations: Vec<Option<bif_core::AnimatedTransform>>,
    /// Whether using multi-draw mode (transforms baked into vertices).
    pub use_multi_draw: bool,
    /// Mesh indices with vertex animation.
    pub vertex_animated_meshes: Vec<usize>,
    /// USD stage for querying animated vertices.
    pub stage: Option<Arc<UsdStage>>,
    /// Mesh ranges for multi-mesh scenes (vertex offset/count per mesh).
    pub mesh_ranges: Option<Vec<MeshRange>>,
    /// Cached DisneyBSDF materials (avoids re-loading textures per frame).
    pub ivar_materials: Option<Vec<Arc<DisneyBSDF>>>,
}

impl SceneBuilderData {
    /// Get animated vertex buffer at a specific time, or None if static.
    fn get_animated_vertices(&self, time: f64) -> Option<Vec<Vertex>> {
        if self.vertex_animated_meshes.is_empty() {
            return None;
        }

        if let Some(ref ranges) = self.mesh_ranges {
            let mut updated = self.vertices.clone();
            let stage = self.stage.as_ref()?;

            for &mesh_idx in &self.vertex_animated_meshes {
                let range = match ranges.iter().find(|r| r.usd_mesh_index == mesh_idx) {
                    Some(r) => r,
                    None => continue,
                };

                let positions = match stage.get_mesh_vertices_at_time(mesh_idx, time) {
                    Ok(p) => p,
                    Err(_) => continue,
                };

                let vertex_count = positions.len() / 3;
                if vertex_count != range.vertex_count as usize {
                    log::warn!(
                        "Vertex count mismatch for mesh {}: USD {} vs range {}",
                        mesh_idx,
                        vertex_count,
                        range.vertex_count
                    );
                    continue;
                }

                let start = range.vertex_offset as usize;
                for (i, v) in updated[start..start + vertex_count].iter_mut().enumerate() {
                    v.position = [positions[i * 3], positions[i * 3 + 1], positions[i * 3 + 2]];
                }
            }
            return Some(updated);
        }

        if self.vertex_animated_meshes.len() == 1 {
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
                        return None;
                    }
                    let mut updated = self.vertices.clone();
                    for (i, v) in updated.iter_mut().enumerate() {
                        v.position = [positions[i * 3], positions[i * 3 + 1], positions[i * 3 + 2]];
                    }
                    Some(updated)
                }
                None => None,
            }
        } else {
            None
        }
    }

    /// Build Embree scene at a specific time.
    pub fn build_scene_at_time(&mut self, time: f64) -> Arc<BvhNode> {
        // Get vertex data — animated or static
        let updated = self.get_animated_vertices(time);
        let verts = updated.as_deref().unwrap_or(&self.vertices);
        let positions: Vec<[f32; 3]> = verts.iter().map(|v| v.position).collect();
        let normals_soa: Vec<[f32; 3]> = verts.iter().map(|v| v.normal).collect();
        let uvs_soa: Vec<[f32; 2]> = verts.iter().map(|v| v.uv).collect();

        // Use cached materials or build from scratch
        let materials: Vec<Arc<DisneyBSDF>> = if let Some(ref cached) = self.ivar_materials {
            cached.clone()
        } else {
            let mats = build_materials(
                &self.scene_materials,
                &self.scene_material,
                self.texture_base_dir.as_deref(),
            );
            self.ivar_materials = Some(mats.clone());
            mats
        };

        let tri_mat_ids: Vec<u32> = self
            .triangle_material_ids
            .as_ref()
            .cloned()
            .unwrap_or_default();

        let ivar_transforms = if self.use_multi_draw {
            vec![Mat4::IDENTITY]
        } else {
            self.evaluate_transforms_at_time(time)
        };

        if let Some(embree_scene) = EmbreeScene::try_from_indexed(
            &positions,
            &normals_soa,
            &uvs_soa,
            &self.indices,
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

    /// Evaluate instance transforms at a specific time.
    ///
    /// Uses animated transforms where available, falling back to static transforms.
    fn evaluate_transforms_at_time(&self, time: f64) -> Vec<Mat4> {
        debug_assert_eq!(
            self.instance_transforms.len(),
            self.instance_animations.len(),
            "instance_transforms and instance_animations must have same length"
        );
        self.instance_transforms
            .iter()
            .zip(self.instance_animations.iter())
            .map(|(base_transform, anim)| {
                if let Some(anim) = anim {
                    // Evaluate animated transform at this time
                    anim.evaluate(time).to_matrix()
                } else {
                    // Use static transform
                    *base_transform
                }
            })
            .collect()
    }

    /// Check if this scene has any transform animations.
    pub fn has_transform_animations(&self) -> bool {
        self.instance_animations
            .iter()
            .any(|a| a.as_ref().is_some_and(|anim| anim.is_animated()))
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
    /// Explicit lights (USD lights) for NEE.
    pub lights: Arc<LightList>,
    /// HDRI rotation override from viewport slider (radians).
    pub hdri_rotation: Option<f32>,
    /// HDRI intensity override from viewport slider.
    pub hdri_intensity: Option<f32>,
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
    mut scene: BatchSceneData,
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
    // Batch render: create a per-frame cache for multi-SPP accumulation.
    // For animated scenes, cache is cleared per frame in the loop below.
    let batch_cache = if settings.radiance_cache_config.enabled {
        Some(Arc::new(bif_renderer::RadianceCache::new(
            settings.radiance_cache_config.clone(),
        )))
    } else {
        None
    };
    let render_config = RenderConfig {
        samples_per_pixel: settings.samples_per_pixel,
        max_depth: settings.max_depth,
        background: Color::new(0.1, 0.1, 0.1),
        use_sky_gradient: true, // Fallback lighting if no HDRI
        environment: scene.environment.clone(),
        lights: scene.lights.clone(),
        pass_number: 0,
        hdri_rotation: scene.hdri_rotation,
        hdri_intensity: scene.hdri_intensity,
        radiance_cache: batch_cache.clone(),
        pixel_filter: settings.pixel_filter,
        sampler_mode: settings.sampler_mode,
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

        // Rebuild scene for animated geometry
        // Always rebuild for all frames to ensure correct transforms at each time
        if scene.has_animated_geometry {
            // Clear radiance cache — geometry moved, cached values are stale
            if let Some(ref cache) = batch_cache {
                cache.clear();
            }
            if let Some(ref mut builder) = scene.scene_builder {
                log::info!("Building BVH for frame {}", frame);
                let rebuild_start = std::time::Instant::now();
                // Drop old scene before building new one to free Embree resources
                drop(current_world);
                current_world = builder(frame as f64);
                log::debug!(
                    "BVH built in {:.2}ms",
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

        // Denoise beauty pass before writing EXR (shadows immutable `result`)
        #[cfg(feature = "oidn")]
        let result = if settings.aov_settings.denoise_output {
            let w = result.width as usize;
            let h = result.height as usize;
            match bif_renderer::denoise_beauty(
                w,
                h,
                &result.beauty,
                result.albedo.as_deref(),
                result.normal.as_deref(),
            ) {
                Ok(denoised) => {
                    log::info!("Batch frame {} denoised", frame);
                    ExrOutput {
                        beauty: denoised.beauty,
                        ..result
                    }
                }
                Err(e) => {
                    log::warn!("Batch denoise failed for frame {}: {}", frame, e);
                    result
                }
            }
        } else {
            result
        };

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
        CameraSource::OrthoView(_) | CameraSource::SceneCamera(_) => {
            // Ortho/scene cameras use viewport camera (already synced to the correct view)
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
///
/// The C++ bridge flat-copies USD row-major data; `from_cols_array()` implicitly
/// transposes to glam's column-vector convention, so col(n) = USD basis row n:
///   col(0) = X axis, col(1) = Y (up), col(2) = Z, col(3) = translation.
fn camera_from_usd_transform(xform: Mat4, width: u32, height: u32) -> Camera {
    let position = xform.col(3).truncate();

    // Camera looks down -Z in its local space
    let forward = -xform.col(2).truncate().normalize();

    let target = position + forward * 10.0;

    let up = xform.col(1).truncate().normalize();

    // Default FOV (USD cameras have focal length, but we use a reasonable default)
    let fov_y = 45.0_f32;

    log::debug!(
        "USD camera: pos={:?}, target={:?}, up={:?}, fov={}",
        position,
        target,
        up,
        fov_y
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
    // Albedo only needed for OIDN denoising — skip allocation when disabled
    let mut albedo = if aov_settings.denoise_output {
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
            let done = completed.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
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
                if let Some(ref mut a) = albedo {
                    a[global_idx] = result.albedos[local_idx];
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
        albedo,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bif_core::{AnimatedTransform, Transform, TransformKeyframe};
    use bif_math::Vec3;

    #[test]
    fn test_camera_from_usd_transform() {
        // Identity matrix should create a valid camera
        let xform = Mat4::IDENTITY;
        let camera = camera_from_usd_transform(xform, 100, 100);

        // Verify camera was created with correct resolution
        assert_eq!(camera.image_width, 100);
        assert_eq!(camera.image_height, 100);
    }

    fn make_test_builder_data() -> SceneBuilderData {
        SceneBuilderData {
            vertices: vec![],
            indices: vec![],
            triangle_material_ids: None,
            scene_materials: vec![],
            scene_material: bif_core::Material::default(),
            texture_base_dir: None,
            instance_transforms: vec![Mat4::IDENTITY, Mat4::IDENTITY],
            instance_animations: vec![None, None],
            use_multi_draw: false,
            vertex_animated_meshes: vec![],
            stage: None,
            mesh_ranges: None,
            ivar_materials: None,
        }
    }

    #[test]
    fn test_evaluate_static_transforms() {
        let mut data = make_test_builder_data();
        data.instance_transforms = vec![
            Mat4::from_translation(Vec3::new(1.0, 0.0, 0.0)),
            Mat4::from_translation(Vec3::new(2.0, 0.0, 0.0)),
        ];
        data.instance_animations = vec![None, None];

        let result = data.evaluate_transforms_at_time(10.0);

        assert_eq!(result.len(), 2);
        // Static transforms should be returned unchanged
        assert_eq!(result[0], data.instance_transforms[0]);
        assert_eq!(result[1], data.instance_transforms[1]);
    }

    #[test]
    fn test_evaluate_animated_transforms() {
        let mut data = make_test_builder_data();
        data.instance_transforms = vec![Mat4::IDENTITY, Mat4::IDENTITY];

        // Create animated transform with keyframes at frame 1 and 24
        let keyframes = vec![
            TransformKeyframe {
                time: 1.0,
                transform: Transform {
                    translation: Vec3::new(0.0, 0.0, 0.0),
                    ..Default::default()
                },
            },
            TransformKeyframe {
                time: 24.0,
                transform: Transform {
                    translation: Vec3::new(10.0, 0.0, 0.0),
                    ..Default::default()
                },
            },
        ];
        let animated = AnimatedTransform::with_keyframes(Transform::default(), keyframes);

        data.instance_animations = vec![Some(animated), None];

        // At frame 1, translation should be (0, 0, 0)
        let result_f1 = data.evaluate_transforms_at_time(1.0);
        let pos_f1 = result_f1[0].col(3);
        assert!((pos_f1.x - 0.0).abs() < 0.01);

        // At frame 24, translation should be (10, 0, 0)
        let result_f24 = data.evaluate_transforms_at_time(24.0);
        let pos_f24 = result_f24[0].col(3);
        assert!((pos_f24.x - 10.0).abs() < 0.01);

        // At frame 12.5 (midpoint), translation should be ~(5, 0, 0)
        let result_mid = data.evaluate_transforms_at_time(12.5);
        let pos_mid = result_mid[0].col(3);
        assert!((pos_mid.x - 5.0).abs() < 0.5, "pos_mid.x = {}", pos_mid.x);
    }

    #[test]
    fn test_has_transform_animations() {
        let mut data = make_test_builder_data();

        // No animations
        assert!(!data.has_transform_animations());

        // Static-only AnimatedTransform (no keyframes)
        data.instance_animations = vec![Some(AnimatedTransform::static_only(Transform::default()))];
        assert!(!data.has_transform_animations());

        // With actual keyframes
        let keyframes = vec![TransformKeyframe {
            time: 1.0,
            transform: Transform::default(),
        }];
        data.instance_animations = vec![Some(AnimatedTransform::with_keyframes(
            Transform::default(),
            keyframes,
        ))];
        assert!(data.has_transform_animations());
    }
}
