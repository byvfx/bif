use std::sync::mpsc;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Instant;

use bif_math::{Mat4, Vec3};

use bif_renderer::{
    render_bucket_with_aovs, BvhNode, Color, EmbreeScene, Hittable, LightList, OpenPbrSurface,
    RenderConfig,
};

use crate::batch_render::{self, BatchSceneData, SceneBuilderData};
use crate::ivar_state::{self, BatchRenderStatus, BuildStatus, IvarMessage};
use crate::Renderer;

impl Renderer {
    /// Upload Ivar image buffer to GPU texture, using selected AOV channel.
    pub(crate) fn upload_ivar_pixels(&self) {
        let Some(ref image) = self.ivar.ivar_state.image_buffer else {
            return;
        };
        let width = image.width;
        let height = image.height;

        let rgba = ivar_state::encode_aov_rgba(
            self.ivar.ivar_state.preview_aov,
            &self.ivar.ivar_state,
            image,
        );

        self.gpu.queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &self.ivar.ivar_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &rgba,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(4 * width),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
    }

    /// Build Ivar scene from viewport mesh data using instancing (async in background thread).
    ///
    /// NEW: Uses InstancedGeometry to build ONE BVH for the prototype mesh
    /// instead of duplicating 28M triangles. This reduces build time from
    /// ~4 seconds to ~40ms (100x faster) and memory from ~5GB to ~50MB.
    ///
    /// ASYNC: Runs on background thread to keep UI responsive during build.
    fn build_ivar_scene(&mut self) {
        // Check if already building or complete
        match self.ivar.ivar_state.build_status {
            BuildStatus::Building => {
                // Already building in background, skip
                return;
            }
            BuildStatus::Complete => {
                // Already built, skip
                return;
            }
            _ => {}
        }

        log::info!(
            "Starting background Ivar scene build: {} instances, {} tris/instance",
            self.scene.instances.transforms.len(),
            self.scene.mesh_data.indices.len() / 3
        );

        // Mark as building
        self.ivar.ivar_state.build_status = BuildStatus::Building;

        // When using multi-draw (combined mesh), transforms are already baked into vertices
        // Use single identity transform to avoid double-transforming
        let transforms = if self.multi_draw.enabled {
            log::info!(
                "Multi-prototype: using identity transform for Ivar (transforms baked into combined mesh)"
            );
            vec![Mat4::IDENTITY]
        } else {
            self.scene.instances.current.clone()
        };
        let scene_materials = self.scene.scene_materials.clone();
        let fallback_material = self.scene.scene_material.clone();
        let texture_base_dir = self.scene.texture_base_dir.clone();
        let tri_mat_ids: Vec<u32> = self
            .scene
            .mesh_data
            .triangle_material_ids
            .as_ref()
            .cloned()
            .unwrap_or_default();

        // Material ID distribution (enable via RUST_LOG=bif_viewport::ivar_build=debug)
        if log::log_enabled!(log::Level::Debug) {
            let mut counts = std::collections::HashMap::new();
            for &id in &tri_mat_ids {
                *counts.entry(id).or_insert(0u32) += 1;
            }
            log::debug!(
                "Ivar tri_mat_ids: {} tris, {} unique IDs: {:?}",
                tri_mat_ids.len(),
                counts.len(),
                counts
            );
        }

        // Extract indexed data for from_indexed() path
        let is_animated = !self.scene.vertex_animated_meshes.is_empty();
        let (positions, normals_soa, uvs_soa, indices) = if is_animated {
            // Animated: query USD for positions at current time, extract SOA
            let current_time = Some(self.timeline_state.current_frame);
            let updated = self.get_animated_vertices(current_time);
            let verts = updated.as_deref().unwrap_or(&self.scene.mesh_data.vertices);
            let positions: Vec<[f32; 3]> = verts.iter().map(|v| v.position).collect();
            let normals_soa: Vec<[f32; 3]> = verts.iter().map(|v| v.normal).collect();
            let uvs_soa: Vec<[f32; 2]> = verts.iter().map(|v| v.uv).collect();
            (
                positions,
                normals_soa,
                uvs_soa,
                self.scene.mesh_data.indices.clone(),
            )
        } else {
            // Static: extract SOA directly from mesh_data
            (
                self.scene.mesh_data.extract_positions(),
                self.scene.mesh_data.extract_normals(),
                self.scene.mesh_data.extract_uvs(),
                self.scene.mesh_data.indices.clone(),
            )
        };

        // Clone subdivision data for Embree limit-surface evaluation
        let subdiv_info = self.scene.mesh_data.subdiv_info.clone();

        // Create channel for build completion (returns BVH + materials for caching)
        let (tx, rx) = mpsc::channel();
        self.ivar.ivar_state.build_receiver = Some(rx);

        let vert_count = positions.len();
        let tri_count = indices.len() / 3;
        let instance_count = transforms.len();

        // If prewarm is in-flight, try to grab its result before spawning a redundant load
        if self.ivar.ivar_materials.is_none() {
            if let Some(ref rx) = self.async_channels.ivar_materials_receiver {
                // Brief blocking wait — prewarm may be nearly done
                use std::time::Duration;
                if let Ok((materials, texture_cache)) = rx.recv_timeout(Duration::from_millis(50)) {
                    log::info!(
                        "Grabbed pre-warmed materials ({}) before build",
                        materials.len()
                    );
                    self.ivar.ivar_materials = Some(materials);
                    self.ivar.ivar_texture_cache = Some(texture_cache);
                    self.async_channels.ivar_materials_receiver = None;
                }
            }
        }

        // Check for cached materials (cheap Arc clones)
        let cached_materials = self.ivar.ivar_materials.clone();

        // Take texture cache for the build thread if we need to build materials
        let thread_texture_cache = if cached_materials.is_none() {
            self.ivar.ivar_texture_cache.take()
        } else {
            None
        };

        // Spawn background thread to build scene
        std::thread::spawn(move || {
            let start_time = Instant::now();

            log::info!(
                "Background thread: Building Embree scene ({} tris, {} shared verts, {} instances)...",
                tri_count,
                vert_count,
                instance_count
            );

            // Use cached materials or build from scratch
            let (materials, ret_cache): (
                Vec<Arc<OpenPbrSurface>>,
                Option<bif_core::texture::TextureCache>,
            ) = if let Some(cached) = cached_materials {
                log::info!("Using cached materials ({} materials)", cached.len());
                (cached, None)
            } else {
                let mut cache =
                    thread_texture_cache.unwrap_or_else(|| match texture_base_dir.as_deref() {
                        Some(dir) => bif_core::texture::TextureCache::with_base_dir(dir),
                        None => bif_core::texture::TextureCache::new(),
                    });
                let mats =
                    batch_render::build_materials(&scene_materials, &fallback_material, &mut cache);
                cache.sweep_unreferenced();
                (mats, Some(cache))
            };

            // Clone materials for cache return (cheap Arc bumps)
            let materials_for_cache = materials.clone();

            // Build SubdivData from SubdivInfo if available
            let subd = subdiv_info.as_ref().map(|si| bif_renderer::SubdivData {
                face_vertex_counts: &si.face_vertex_counts,
                polygon_indices: &si.polygon_indices,
                crease_indices: &si.crease_indices,
                crease_lengths: &si.crease_lengths,
                crease_sharpnesses: &si.crease_sharpnesses,
                facevarying_uvs: si.facevarying_uvs.as_deref(),
                facevarying_uv_indices: si.facevarying_uv_indices.as_deref(),
            });

            // For subdivision, use original shared positions instead of triangulated vertices
            let (subd_positions, subd_ref) = if let Some(ref si) = subdiv_info {
                let pos: Vec<[f32; 3]> = si.positions.iter().map(|p| [p.x, p.y, p.z]).collect();
                log::info!(
                    "Using subdiv positions: {} shared verts (vs {} triangulated)",
                    pos.len(),
                    positions.len()
                );
                (Some(pos), subd.as_ref())
            } else {
                (None, None)
            };
            let effective_positions = subd_positions.as_deref().unwrap_or(&positions);

            // Use from_indexed() — keeps shared vertices, builds hit data in parallel
            let world = if let Some(embree_scene) = EmbreeScene::try_from_indexed(
                effective_positions,
                &normals_soa,
                &uvs_soa,
                &indices,
                transforms,
                materials,
                &tri_mat_ids,
                subd_ref,
            ) {
                log::info!("Using Embree (indexed) for hardware-accelerated ray tracing");
                let objects: Vec<Box<dyn Hittable + Send + Sync>> = vec![Box::new(embree_scene)];
                Arc::new(BvhNode::new(objects))
            } else {
                log::warn!("Embree not available - using CPU BVH (slower performance)");
                let objects: Vec<Box<dyn Hittable + Send + Sync>> = vec![];
                Arc::new(BvhNode::new(objects))
            };

            let elapsed = start_time.elapsed();
            log::info!(
                "Background thread: Ivar scene built in {:.2}ms",
                elapsed.as_secs_f64() * 1000.0
            );

            let _ = tx.send((world, materials_for_cache, ret_cache));
        });
    }

    /// Get animated vertex buffer at a specific time, or None if static.
    ///
    /// Queries USD for interpolated positions and returns updated vertex buffer.
    fn get_animated_vertices(&self, time: Option<f64>) -> Option<Vec<crate::gpu_types::Vertex>> {
        let t = time?;
        if self.scene.vertex_animated_meshes.is_empty() {
            return None;
        }

        let stage_mtx = self.scene.usd_stage.as_ref()?;
        let stage = stage_mtx.lock().expect("UsdStage mutex poisoned");

        // Multi-mesh scene: use mesh_ranges to update each animated mesh's vertices
        if let Some(ref ranges) = self.scene.mesh_data.mesh_ranges {
            let mut updated = self.scene.mesh_data.vertices.clone();

            for &mesh_idx in &self.scene.vertex_animated_meshes {
                let range = match ranges.iter().find(|r| r.usd_mesh_index == mesh_idx) {
                    Some(r) => r,
                    None => continue,
                };

                let positions = match stage.get_mesh_vertices_at_time(mesh_idx, t) {
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

        // Single-mesh fallback
        if self.scene.vertex_animated_meshes.len() != 1 {
            return None;
        }

        let mesh_idx = self.scene.vertex_animated_meshes[0];
        let positions = stage.get_mesh_vertices_at_time(mesh_idx, t).ok()?;

        let vertex_count = positions.len() / 3;
        if vertex_count != self.scene.mesh_data.vertices.len() {
            log::warn!(
                "Vertex count mismatch: USD {} vs mesh {} - using static",
                vertex_count,
                self.scene.mesh_data.vertices.len()
            );
            return None;
        }

        let mut updated = self.scene.mesh_data.vertices.clone();
        for (i, v) in updated.iter_mut().enumerate() {
            v.position = [positions[i * 3], positions[i * 3 + 1], positions[i * 3 + 2]];
        }
        Some(updated)
    }

    /// Build Ivar scene synchronously (blocking). Used for batch render.
    fn build_ivar_scene_sync(&mut self) -> Arc<BvhNode> {
        self.build_ivar_scene_at_time(None)
    }

    /// Build Ivar scene at a specific time. Used for animated batch render.
    fn build_ivar_scene_at_time(&mut self, time: Option<f64>) -> Arc<BvhNode> {
        let start_time = Instant::now();

        log::info!(
            "Building Ivar scene (sync): {} triangles, {} instances, time={:?}",
            self.scene.mesh_data.indices.len() / 3,
            self.scene.instances.transforms.len(),
            time
        );

        // Get vertex data — animated or static
        let updated = self.get_animated_vertices(time);
        let verts = updated.as_deref().unwrap_or(&self.scene.mesh_data.vertices);
        let positions: Vec<[f32; 3]> = verts.iter().map(|v| v.position).collect();
        let normals_soa: Vec<[f32; 3]> = verts.iter().map(|v| v.normal).collect();
        let uvs_soa: Vec<[f32; 2]> = verts.iter().map(|v| v.uv).collect();

        // Use cached materials or build from scratch
        let materials: Vec<Arc<OpenPbrSurface>> = if let Some(ref cached) = self.ivar.ivar_materials
        {
            log::info!("Using cached materials ({} materials)", cached.len());
            cached.clone()
        } else {
            let mut cache = self.ivar.ivar_texture_cache.take().unwrap_or_else(|| {
                match self.scene.texture_base_dir.as_deref() {
                    Some(dir) => bif_core::texture::TextureCache::with_base_dir(dir),
                    None => bif_core::texture::TextureCache::new(),
                }
            });
            let mats = batch_render::build_materials(
                &self.scene.scene_materials,
                &self.scene.scene_material,
                &mut cache,
            );
            cache.sweep_unreferenced();
            self.ivar.ivar_texture_cache = Some(cache);
            self.ivar.ivar_materials = Some(mats.clone());
            mats
        };

        let tri_mat_ids: Vec<u32> = self
            .scene
            .mesh_data
            .triangle_material_ids
            .as_ref()
            .cloned()
            .unwrap_or_default();

        let ivar_transforms = if self.multi_draw.enabled {
            vec![Mat4::IDENTITY]
        } else {
            self.scene.instances.current.clone()
        };

        let world = if let Some(embree_scene) = EmbreeScene::try_from_indexed(
            &positions,
            &normals_soa,
            &uvs_soa,
            &self.scene.mesh_data.indices,
            ivar_transforms,
            materials,
            &tri_mat_ids,
            None,
        ) {
            log::info!("Using Embree (indexed) for batch render");
            let objects: Vec<Box<dyn Hittable + Send + Sync>> = vec![Box::new(embree_scene)];
            Arc::new(BvhNode::new(objects))
        } else {
            log::warn!("Embree not available for batch render");
            let objects: Vec<Box<dyn Hittable + Send + Sync>> = vec![];
            Arc::new(BvhNode::new(objects))
        };

        let elapsed = start_time.elapsed();
        log::info!("Scene built in {:.2}ms", elapsed.as_secs_f64() * 1000.0);

        world
    }

    /// Invalidate cached Ivar scene (call when geometry changes or user requests rebuild).
    ///
    /// This will:
    /// 1. Clear the cached BVH
    /// 2. Reset build status to NotStarted
    /// 3. Cancel any active render
    ///
    /// Next time user switches to Ivar mode, scene will rebuild from scratch.
    pub fn invalidate_ivar_scene(&mut self) {
        log::info!("Invalidating Ivar scene cache");

        // Clear cached scene + cancel render
        self.ivar.ivar_state.invalidate_scene();
        self.ivar.ivar_state.build_receiver = None;
        self.ivar.ivar_state.cancel_flag = Arc::new(AtomicBool::new(false));
        self.ivar.ivar_state.buckets_completed = 0;
        self.ivar.ivar_state.image_buffer = None;

        log::info!("Ivar scene cache cleared - will rebuild on next render");
    }

    /// Invalidate cached OpenPbrSurface materials.
    ///
    /// Call when materials actually change (scene reload, material edit).
    /// Geometry-only changes (camera, transforms) should NOT call this.
    /// Intentionally keeps `ivar_texture_cache` alive — unchanged textures
    /// reuse existing Arc refs on next build, sweep_unreferenced() frees orphans.
    pub(crate) fn invalidate_ivar_materials(&mut self) {
        if self.ivar.ivar_materials.is_some() {
            log::info!("Invalidating cached Ivar materials");
        }
        self.ivar.ivar_materials = None;
        self.async_channels.ivar_materials_receiver = None;
    }

    /// Pre-warm OpenPbrSurface materials on a background thread.
    ///
    /// Spawns a thread to load textures and build materials so the first
    /// Ivar scene build can skip the expensive texture-loading step.
    /// Takes the persistent texture cache so unchanged textures aren't reloaded.
    #[allow(dead_code)] // Kept for future use — currently disabled to save RAM on large scenes
    pub(crate) fn prewarm_ivar_materials(&mut self) {
        // Skip if no materials to build
        if self.scene.scene_materials.is_empty() {
            return;
        }
        // Skip if already cached or already loading
        if self.ivar.ivar_materials.is_some()
            || self.async_channels.ivar_materials_receiver.is_some()
        {
            return;
        }

        let scene_materials = self.scene.scene_materials.clone();
        let fallback_material = self.scene.scene_material.clone();
        let texture_base_dir = self.scene.texture_base_dir.clone();

        // Take existing texture cache so unchanged textures reuse Arc refs
        let existing_cache = self.ivar.ivar_texture_cache.take();

        let (tx, rx) = mpsc::channel();
        self.async_channels.ivar_materials_receiver = Some(rx);

        std::thread::spawn(move || {
            let start = Instant::now();
            let mut cache = existing_cache.unwrap_or_else(|| match texture_base_dir.as_deref() {
                Some(dir) => bif_core::texture::TextureCache::with_base_dir(dir),
                None => bif_core::texture::TextureCache::new(),
            });
            let materials =
                batch_render::build_materials(&scene_materials, &fallback_material, &mut cache);
            cache.sweep_unreferenced();
            log::info!(
                "Pre-warmed {} materials in {:.2}ms",
                materials.len(),
                start.elapsed().as_secs_f64() * 1000.0
            );
            let _ = tx.send((materials, cache));
        });

        log::info!(
            "Started material pre-warm ({} materials)",
            self.scene.scene_materials.len()
        );
    }

    /// Poll for scene build completion (call each frame).
    ///
    /// Checks if background scene build is complete, and if so:
    /// 1. Stores the completed scene
    /// 2. Marks build as complete
    /// 3. Starts the render
    pub(crate) fn poll_scene_build(&mut self) {
        // Poll for pre-warmed materials (from prewarm_ivar_materials)
        if let Some(ref rx) = self.async_channels.ivar_materials_receiver {
            if let Ok((materials, texture_cache)) = rx.try_recv() {
                log::info!(
                    "Pre-warmed {} materials received on main thread",
                    materials.len()
                );
                self.ivar.ivar_materials = Some(materials);
                self.ivar.ivar_texture_cache = Some(texture_cache);
                self.async_channels.ivar_materials_receiver = None;
            }
        }

        // Only poll scene build if we're currently building
        if self.ivar.ivar_state.build_status != BuildStatus::Building {
            return;
        }

        let Some(ref receiver) = self.ivar.ivar_state.build_receiver else {
            return;
        };

        // Non-blocking check for completion
        if let Ok((world, materials, texture_cache)) = receiver.try_recv() {
            log::info!("Scene build completed, received on main thread");

            // Cache materials for future builds
            if self.ivar.ivar_materials.is_none() {
                self.ivar.ivar_materials = Some(materials);
            }

            // Restore texture cache from build thread
            if let Some(cache) = texture_cache {
                self.ivar.ivar_texture_cache = Some(cache);
            }

            // Store completed scene
            self.ivar.ivar_state.world = Some(world);
            self.ivar.ivar_state.build_status = BuildStatus::Complete;

            // Create radiance cache if enabled
            if self.ivar.ivar_state.radiance_cache_config.enabled {
                let cache = Arc::new(bif_renderer::RadianceCache::new(
                    self.ivar.ivar_state.radiance_cache_config.clone(),
                ));
                self.ivar.ivar_state.radiance_cache = Some(cache);
                log::info!("SHARC radiance cache created");
            }

            // Clear receiver
            self.ivar.ivar_state.build_receiver = None;

            // Start render at current scale (respects interaction state during build)
            log::info!("Starting Ivar render with built scene");
            self.restart_ivar_at_scale(self.ivar.ivar_state.current_scale);
        }
    }

    /// Create Ivar camera at explicit resolution.
    fn create_ivar_camera_at_resolution(&self, width: u32, height: u32) -> bif_renderer::Camera {
        let mut camera = bif_renderer::Camera::new()
            .with_resolution(width, height)
            .with_position(self.cam.camera.position, self.cam.camera.target, Vec3::Y)
            .with_lens(
                self.cam.camera.fov_y.to_degrees(),
                0.0, // No DOF for preview
                (self.cam.camera.target - self.cam.camera.position).length(),
            )
            .with_quality(
                self.ivar.ivar_state.samples_per_pixel,
                self.ivar.ivar_state.max_depth,
            );

        camera.initialize();
        camera
    }

    /// Create Ivar camera from viewport camera at full resolution.
    fn create_ivar_camera(&self) -> bif_renderer::Camera {
        let (_, _, vp_w, vp_h) = self.viewport_rect();
        self.create_ivar_camera_at_resolution(vp_w as u32, vp_h as u32)
    }

    /// Collect unique texture paths from current scene materials.
    #[cfg(feature = "oiio")]
    pub(crate) fn collect_material_texture_paths(&self) -> Vec<String> {
        let mut paths = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for mat in &self.scene.scene_materials {
            let candidates = [
                mat.base_color_texture.as_deref(),
                mat.specular_roughness_texture.as_deref(),
                mat.base_metalness_texture.as_deref(),
                mat.normal_texture.as_deref(),
                mat.emission_texture.as_deref(),
            ];
            for path in candidates.into_iter().flatten() {
                if seen.insert(path.to_string()) {
                    paths.push(path.to_string());
                }
            }
        }
        paths
    }

    /// Start Ivar progressive render (build scene + first pass at full res).
    pub(crate) fn start_ivar_render(&mut self) {
        self.build_ivar_scene();

        let Some(_) = self.ivar.ivar_state.world.as_ref() else {
            log::error!("Cannot start Ivar render: no scene");
            return;
        };

        self.restart_ivar_at_scale(1);
    }

    /// Restart Ivar rendering at a specific resolution scale without rebuilding BVH.
    ///
    /// Scale is a divisor: 1 = full res, 2 = half, 4 = quarter, 8 = eighth.
    /// Cancels any in-flight pass, recreates texture at scaled size, starts new pass.
    pub(crate) fn restart_ivar_at_scale(&mut self, scale: u32) {
        let Some(_) = self.ivar.ivar_state.world.as_ref() else {
            return;
        };

        let (_, _, vp_w, vp_h) = self.viewport_rect();
        // Round instead of truncate to minimize aspect ratio drift across scale steps
        let scaled_w = ((vp_w / scale as f32).round() as u32).max(1);
        let scaled_h = ((vp_h / scale as f32).round() as u32).max(1);

        // Check if GPU texture needs recreation before reset_accumulation replaces image_buffer
        let needs_texture = self
            .ivar
            .ivar_state
            .image_buffer
            .as_ref()
            .is_none_or(|img| img.width != scaled_w || img.height != scaled_h);

        // Set scale before reset_accumulation so it can skip AOVs at reduced scale
        self.ivar.ivar_state.current_scale = scale;
        self.ivar.ivar_state.reset_accumulation(scaled_w, scaled_h);
        self.ivar.ivar_state.buckets = Arc::new(bif_renderer::generate_buckets(
            scaled_w,
            scaled_h,
            bif_renderer::DEFAULT_BUCKET_SIZE,
        ));

        if needs_texture {
            let (tex, view) =
                crate::ivar_renderer::create_ivar_texture(&self.gpu.device, (scaled_w, scaled_h));
            self.ivar.ivar_texture = tex;
            self.ivar.ivar_texture_view = view;
            self.ivar.ivar_bind_group = crate::ivar_renderer::create_ivar_bind_group(
                &self.gpu.device,
                &self.ivar.ivar_bind_group_layout,
                &self.ivar.ivar_texture_view,
                &self.ivar.ivar_sampler,
            );
        }

        // Save camera snapshot
        self.ivar.ivar_state.last_camera_snapshot = Some(
            crate::ivar_state::CameraSnapshot::from_camera(&self.cam.camera),
        );

        log::info!(
            "Restarting Ivar at 1/{} scale ({}x{})",
            scale,
            scaled_w,
            scaled_h
        );
        self.start_progressive_pass();
    }

    /// Start a single progressive pass (1 SPP) in background thread.
    ///
    /// Requires `ivar_state.world` already built.
    pub(crate) fn start_progressive_pass(&mut self) {
        let Some(world) = self.ivar.ivar_state.world.clone() else {
            return;
        };

        let pass_number = self.ivar.ivar_state.accumulated_samples;
        let buckets = Arc::clone(&self.ivar.ivar_state.buckets);

        // Create fresh cancel flag + channel for this pass
        self.ivar.ivar_state.cancel_flag = Arc::new(AtomicBool::new(false));
        let cancel_flag = self.ivar.ivar_state.cancel_flag.clone();
        let (tx, rx) = mpsc::channel();
        self.ivar.ivar_state.receiver = Some(rx);
        self.ivar.ivar_state.buckets_completed = 0;

        // Use image buffer dimensions (may be scaled) for camera ray generation
        let ivar_camera = if let Some(ref img) = self.ivar.ivar_state.image_buffer {
            self.create_ivar_camera_at_resolution(img.width, img.height)
        } else {
            self.create_ivar_camera()
        };
        let config = RenderConfig {
            samples_per_pixel: 1, // 1 SPP per progressive pass
            max_depth: self.ivar.ivar_state.max_depth,
            background: Color::ZERO,
            use_sky_gradient: self.ivar.ivar_state.use_sky_gradient,
            environment: self.ivar.ivar_state.environment.clone(),
            lights: Arc::new(LightList::from(self.lights.scene_lights.as_slice())),
            pass_number,
            hdri_rotation: Some(self.ivar.ivar_state.hdri_rotation),
            hdri_intensity: Some(self.ivar.ivar_state.hdri_intensity),
            hdri_show_background: self.ivar.ivar_state.hdri_show_background,
            radiance_cache: self.ivar.ivar_state.radiance_cache.clone(),
            pixel_filter: self.ivar.ivar_state.pixel_filter,
            sampler_mode: self.ivar.ivar_state.sampler_mode,
        };

        log::trace!("Starting progressive pass {}", pass_number);

        rayon::spawn(move || {
            use rayon::prelude::*;

            buckets.par_iter().for_each(|bucket| {
                if cancel_flag.load(Ordering::Relaxed) {
                    return;
                }
                let result = render_bucket_with_aovs(bucket, &ivar_camera, world.as_ref(), &config);
                let _ = tx.send(IvarMessage::BucketComplete(Box::new(result)));
            });

            if !cancel_flag.load(Ordering::Relaxed) {
                let _ = tx.send(IvarMessage::PassComplete { pass_number });
            } else {
                let _ = tx.send(IvarMessage::Cancelled);
            }
        });
    }

    /// Poll for Ivar bucket completion messages (progressive accumulation).
    pub(crate) fn poll_ivar_messages(&mut self) {
        let Some(ref receiver) = self.ivar.ivar_state.receiver else {
            return;
        };

        // Collect all available messages (non-blocking), then drop borrow
        let messages: Vec<IvarMessage> = std::iter::from_fn(|| receiver.try_recv().ok()).collect();
        if messages.is_empty() {
            return;
        }

        for msg in messages {
            match msg {
                IvarMessage::BucketComplete(result) => {
                    let image_width = self
                        .ivar
                        .ivar_state
                        .image_buffer
                        .as_ref()
                        .map_or(0, |img| img.width);
                    let current_pass = self.ivar.ivar_state.accumulated_samples;

                    for local_y in 0..result.bucket.height {
                        for local_x in 0..result.bucket.width {
                            let global_x = result.bucket.x + local_x;
                            let global_y = result.bucket.y + local_y;
                            let pixel_idx = (local_y * result.bucket.width + local_x) as usize;
                            let global_idx = (global_y * image_width + global_x) as usize;

                            // Accumulate beauty into running sum
                            if pixel_idx < result.pixels.len() {
                                if let Some(ref mut accum) =
                                    self.ivar.ivar_state.accumulation_buffer
                                {
                                    if global_idx < accum.len() {
                                        // Weighted accumulation: pixels already contain
                                        // weight*color from render_pixel_with_aovs
                                        let pixel_weight = if pixel_idx < result.weights.len() {
                                            result.weights[pixel_idx]
                                        } else {
                                            1.0
                                        };
                                        accum[global_idx] +=
                                            result.pixels[pixel_idx] * pixel_weight;

                                        // Compute display average
                                        let avg = if let Some(ref mut wbuf) =
                                            self.ivar.ivar_state.weight_buffer
                                        {
                                            if global_idx < wbuf.len() {
                                                wbuf[global_idx] += pixel_weight;
                                                if wbuf[global_idx] > 0.0 {
                                                    accum[global_idx] / wbuf[global_idx]
                                                } else {
                                                    accum[global_idx]
                                                }
                                            } else {
                                                accum[global_idx] / (current_pass + 1) as f32
                                            }
                                        } else {
                                            // Box filter: equal weights, simple pass count
                                            accum[global_idx] / (current_pass + 1) as f32
                                        };

                                        if let Some(ref mut image) =
                                            self.ivar.ivar_state.image_buffer
                                        {
                                            image.set(global_x, global_y, avg);
                                        }
                                    }
                                }
                            }

                            // AOVs: only write from pass 0 (depth/normal don't benefit)
                            if current_pass == 0 {
                                if let Some(ref mut alpha) = self.ivar.ivar_state.alpha_buffer {
                                    if pixel_idx < result.alphas.len() && global_idx < alpha.len() {
                                        alpha[global_idx] = result.alphas[pixel_idx];
                                    }
                                }
                                if let Some(ref mut depth) = self.ivar.ivar_state.depth_buffer {
                                    if pixel_idx < result.depths.len() && global_idx < depth.len() {
                                        depth[global_idx] = result.depths[pixel_idx];
                                    }
                                }
                                if let Some(ref mut normal) = self.ivar.ivar_state.normal_buffer {
                                    if pixel_idx < result.normals.len() && global_idx < normal.len()
                                    {
                                        normal[global_idx] = result.normals[pixel_idx];
                                    }
                                }
                                if let Some(ref mut sn) = self.ivar.ivar_state.shading_normal_buffer
                                {
                                    if pixel_idx < result.shading_normals.len()
                                        && global_idx < sn.len()
                                    {
                                        sn[global_idx] = result.shading_normals[pixel_idx];
                                    }
                                }
                                if let Some(ref mut albedo) = self.ivar.ivar_state.albedo_buffer {
                                    if pixel_idx < result.albedos.len() && global_idx < albedo.len()
                                    {
                                        albedo[global_idx] = result.albedos[pixel_idx];
                                    }
                                }
                                if let Some(ref mut heatmap) =
                                    self.ivar.ivar_state.cache_heatmap_buffer
                                {
                                    if pixel_idx < result.cache_samples.len()
                                        && global_idx < heatmap.len()
                                    {
                                        heatmap[global_idx] = result.cache_samples[pixel_idx];
                                    }
                                }
                            }
                        }
                    }
                    self.ivar.ivar_state.buckets_completed += 1;
                }
                IvarMessage::PassComplete { pass_number } => {
                    self.ivar.ivar_state.accumulated_samples = pass_number + 1;
                    // Advance radiance cache frame between passes
                    if let Some(ref cache) = self.ivar.ivar_state.radiance_cache {
                        cache.advance_frame();
                    }
                    log::info!(
                        "Pass {} complete ({}/{} SPP)",
                        pass_number,
                        self.ivar.ivar_state.accumulated_samples,
                        self.ivar.ivar_state.target_spp
                    );
                    if self.ivar.ivar_state.accumulated_samples >= self.ivar.ivar_state.target_spp {
                        let elapsed = self.ivar.ivar_state.elapsed_secs();
                        self.ivar.ivar_state.final_render_secs = Some(elapsed);
                        self.ivar.ivar_state.render_complete = true;
                        self.nodes.node_graph_state.mark_ivar_render_complete();
                        log::info!(
                            "Progressive render complete: {} SPP in {:.2}s",
                            self.ivar.ivar_state.accumulated_samples,
                            elapsed
                        );
                        // Auto-denoise on completion
                        #[cfg(feature = "oidn")]
                        if self.ivar.ivar_state.auto_denoise {
                            self.denoise_ivar_result();
                        }
                    }
                    // Clear receiver so main loop can detect "no pass in flight"
                    self.ivar.ivar_state.receiver = None;
                }
                IvarMessage::RenderComplete { elapsed_secs } => {
                    self.ivar.ivar_state.final_render_secs = Some(elapsed_secs);
                    self.ivar.ivar_state.render_complete = true;
                    self.nodes.node_graph_state.mark_ivar_render_complete();
                    log::info!("Ivar render complete in {:.2}s", elapsed_secs);
                    #[cfg(feature = "oidn")]
                    if self.ivar.ivar_state.auto_denoise {
                        self.denoise_ivar_result();
                    }
                }
                IvarMessage::Cancelled => {
                    log::info!("Ivar render cancelled");
                    self.ivar.ivar_state.receiver = None;
                }
            }
        }
    }

    /// Denoise the current Ivar render result using OIDN (async).
    ///
    /// Clones beauty/albedo/normal buffers, spawns background thread with OIDN,
    /// stores receiver in `denoise.receiver`. Call `poll_denoise_result()` each
    /// frame to check for completion.
    pub(crate) fn denoise_ivar_result(&mut self) {
        let Some(ref image) = self.ivar.ivar_state.image_buffer else {
            log::warn!("No image buffer to denoise");
            return;
        };
        if self.ivar.ivar_state.denoise.in_progress {
            log::warn!("Denoise already in progress");
            return;
        }

        let width = image.width as usize;
        let height = image.height as usize;
        let beauty = image.pixels.clone();
        let albedo = self.ivar.ivar_state.albedo_buffer.clone();
        let normal = self.ivar.ivar_state.shading_normal_buffer.clone();

        let (tx, rx) = mpsc::channel();
        self.ivar.ivar_state.denoise.receiver = Some(rx);
        self.ivar.ivar_state.denoise.in_progress = true;

        log::info!("Denoising {}x{} image (async)...", width, height);

        std::thread::spawn(move || {
            let start = Instant::now();
            match bif_renderer::denoise_beauty(
                width,
                height,
                &beauty,
                albedo.as_deref(),
                normal.as_deref(),
            ) {
                Ok(result) => {
                    let elapsed = start.elapsed();
                    log::info!(
                        "Denoise complete in {:.0}ms",
                        elapsed.as_secs_f64() * 1000.0
                    );
                    let _ = tx.send(crate::ivar_state::DenoiseComplete {
                        beauty: result.beauty,
                    });
                }
                Err(e) => {
                    log::error!("Denoise failed: {}", e);
                    // Don't send — receiver will see channel closed
                }
            }
        });
    }

    /// Poll for async denoise completion (call each frame).
    pub(crate) fn poll_denoise_result(&mut self) {
        if !self.ivar.ivar_state.denoise.in_progress {
            return;
        }

        let Some(ref receiver) = self.ivar.ivar_state.denoise.receiver else {
            return;
        };

        match receiver.try_recv() {
            Ok(result) => {
                // Copy denoised pixels into image_buffer for display
                if let Some(ref mut image) = self.ivar.ivar_state.image_buffer {
                    for (i, &c) in result.beauty.iter().enumerate() {
                        if i < image.pixels.len() {
                            image.pixels[i] = c;
                        }
                    }
                }
                self.ivar.ivar_state.denoise.denoised_buffer = Some(result.beauty);
                self.ivar.ivar_state.denoise.is_denoised = true;
                self.ivar.ivar_state.denoise.in_progress = false;
                self.ivar.ivar_state.denoise.receiver = None;
                log::info!("Denoise result applied to display");
            }
            Err(mpsc::TryRecvError::Empty) => {
                // Still in progress
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                // Thread finished without sending (error case)
                self.ivar.ivar_state.denoise.in_progress = false;
                self.ivar.ivar_state.denoise.receiver = None;
                log::warn!("Denoise thread ended without result");
            }
        }
    }

    /// Start a batch render to disk
    pub(crate) fn start_batch_render(&mut self) {
        // Build scene synchronously if not already built
        if self.ivar.ivar_state.world.is_none() {
            log::info!("Building scene for batch render...");
            self.ivar.ivar_state.batch_status = BatchRenderStatus::Rendering {
                current_frame: 0,
                total_frames: self.ivar.ivar_state.batch_settings.frame_count(),
                frame_progress: 0.0,
            };

            // Build synchronously (same logic as async build_ivar_scene)
            let world = self.build_ivar_scene_sync();
            self.ivar.ivar_state.world = Some(world.clone());
            self.ivar.ivar_state.build_status = BuildStatus::Complete;
        }

        let Some(world) = self.ivar.ivar_state.world.clone() else {
            log::error!("Cannot start batch render: no scene");
            self.ivar.ivar_state.batch_status =
                BatchRenderStatus::Failed("No scene loaded".to_string());
            return;
        };

        // Check for animated geometry (vertex deformation OR transform animation)
        let has_vertex_animation = !self.scene.vertex_animated_meshes.is_empty();
        let has_transform_animation = self
            .scene
            .instance_animations
            .iter()
            .any(|a| a.as_ref().is_some_and(|anim| anim.is_animated()));
        let has_animated_geometry = has_vertex_animation || has_transform_animation;

        if has_animated_geometry {
            log::info!(
                "Animated geometry: vertex={}, transform={}",
                has_vertex_animation,
                has_transform_animation
            );
        }

        // Create scene builder for animated geometry
        let scene_builder: Option<batch_render::SceneBuilderFn> = if has_animated_geometry {
            let mut builder_data = SceneBuilderData {
                vertices: self.scene.mesh_data.vertices.clone(),
                indices: self.scene.mesh_data.indices.clone(),
                triangle_material_ids: self.scene.mesh_data.triangle_material_ids.clone(),
                scene_materials: self.scene.scene_materials.clone(),
                scene_material: self.scene.scene_material.clone(),
                texture_base_dir: self.scene.texture_base_dir.clone(),
                instance_transforms: self.scene.instances.transforms.clone(),
                instance_animations: self.scene.instance_animations.clone(),
                use_multi_draw: self.multi_draw.enabled,
                vertex_animated_meshes: self.scene.vertex_animated_meshes.clone(),
                stage: self.scene.usd_stage.clone(),
                mesh_ranges: self.scene.mesh_data.mesh_ranges.clone(),
                ivar_materials: self.ivar.ivar_materials.clone(),
                texture_cache: self.ivar.ivar_texture_cache.clone(),
            };
            Some(Box::new(move |time: f64| {
                builder_data.build_scene_at_time(time)
            }))
        } else {
            None
        };

        // Create scene data for batch render
        let scene_data = BatchSceneData {
            world,
            environment: self.ivar.ivar_state.environment.clone(),
            stage: self.scene.usd_stage.clone(),
            viewport_camera: self.cam.camera,
            has_animated_geometry,
            scene_builder,
            lights: Arc::new(LightList::from(self.lights.scene_lights.as_slice())),
            hdri_rotation: Some(self.ivar.ivar_state.hdri_rotation),
            hdri_intensity: Some(self.ivar.ivar_state.hdri_intensity),
        };

        // Clone settings and compute auto depth bounds if enabled
        let mut settings = self.ivar.ivar_state.batch_settings.clone();
        if settings.aov_settings.auto_depth_bounds && settings.aov_settings.include_depth {
            // Compute scene diagonal length for depth far
            let scene_size = (self.mesh_bounds_max - self.mesh_bounds_min).length();
            if scene_size > 0.0 {
                settings.aov_settings.depth_far = scene_size * 2.0;
                settings.aov_settings.depth_near = 0.01;
                log::info!(
                    "Auto depth bounds: near={}, far={} (scene_size={})",
                    settings.aov_settings.depth_near,
                    settings.aov_settings.depth_far,
                    scene_size
                );
            }
        }

        log::info!(
            "Starting batch render: frames {}-{} step {}, {}x{} @ {} SPP",
            settings.start_frame,
            settings.end_frame,
            settings.frame_step,
            settings.resolution_x,
            settings.resolution_y,
            settings.samples_per_pixel
        );

        // Start the batch render
        let (rx, cancel_flag) = batch_render::start_batch_render(settings, scene_data);
        self.async_channels.batch_receiver = Some(rx);
        self.async_channels.batch_cancel_flag = Some(cancel_flag);
        self.ivar.ivar_state.batch_status = BatchRenderStatus::Rendering {
            current_frame: 1,
            total_frames: self.ivar.ivar_state.batch_settings.frame_count(),
            frame_progress: 0.0,
        };
    }
}
