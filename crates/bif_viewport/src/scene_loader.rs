use anyhow::Result;
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;

use wgpu::util::DeviceExt;

use bif_math::{Aabb, Mat4, Mat4Ext, Vec3};

use crate::gpu_types::{InstanceData, MaterialGpu, MaterialUniform, PrototypeGpuData};
use crate::ivar_state::BuildStatus;
use crate::mesh_data::MeshData;
use crate::timeline::TimelineState;
use crate::{texture_loader, Renderer, MAX_INSTANCES};

impl Renderer {
    /// Load scene data from a pre-parsed Scene (pure Rust USDA parser fallback).
    ///
    /// Populates GPU buffers from an already-parsed `Scene` without a USD stage.
    /// For full C++ bridge loading with scene browser, use `load_usd_scene()` instead.
    pub fn load_scene_data(&mut self, scene: &bif_core::Scene) -> Result<()> {
        if scene.prototypes.is_empty() {
            anyhow::bail!("Scene has no prototypes");
        }

        let proto = &scene.prototypes[0];
        let mesh_data = MeshData::from_core_mesh(&proto.mesh);

        let scene_material = proto
            .material
            .as_ref()
            .map(|m| (**m).clone())
            .unwrap_or_default();
        log::info!(
            "Material: {} (metallic={:.2}, roughness={:.2})",
            scene_material.name,
            scene_material.metallic,
            scene_material.roughness
        );

        log::info!(
            "Loaded {} vertices, {} indices from scene",
            mesh_data.vertices.len(),
            mesh_data.indices.len()
        );

        // Refresh textures and material table
        self.gpu_textures =
            texture_loader::create_gpu_textures_for_scene(&self.device, &self.queue, scene, None);

        let material_table = if scene.materials.is_empty() {
            vec![MaterialGpu::from_material(
                &bif_core::Material::default(),
                &self.gpu_textures,
            )]
        } else {
            scene
                .materials
                .iter()
                .map(|mat| MaterialGpu::from_material(mat.as_ref(), &self.gpu_textures))
                .collect()
        };
        self.material_table_len = material_table.len() as u32;
        self.material_table_buffer =
            self.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Material Table Buffer"),
                    contents: bytemuck::cast_slice(&material_table),
                    usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                });

        // Triangle material buffer
        if let Some(ref tri_mats) = mesh_data.triangle_material_ids {
            self.triangle_material_buffer =
                self.device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Triangle Material Buffer"),
                        contents: bytemuck::cast_slice(tri_mats),
                        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                    });
            self.has_triangle_materials = true;
        } else {
            self.triangle_material_buffer =
                self.device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Triangle Material Buffer"),
                        contents: bytemuck::cast_slice(&[0xFFFFFFFFu32]),
                        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                    });
            self.has_triangle_materials = false;
        }

        // Rebuild material bind group
        self.material_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Material Bind Group"),
            layout: &self.material_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.material_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.material_table_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.triangle_material_buffer.as_entire_binding(),
                },
            ],
        });

        // Rebuild texture bind group
        let texture_view_refs: Vec<&wgpu::TextureView> = self.gpu_textures.views.iter().collect();
        self.texture_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Texture Bind Group"),
            layout: &self.texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureViewArray(&texture_view_refs),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.texture_sampler),
                },
            ],
        });

        // Vertex and index buffers
        self.vertex_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Vertex Buffer"),
                contents: bytemuck::cast_slice(&mesh_data.vertices),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            });
        self.index_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Index Buffer"),
                contents: bytemuck::cast_slice(&mesh_data.indices),
                usage: wgpu::BufferUsages::INDEX,
            });

        let material_index_by_name: HashMap<String, u32> = scene
            .materials
            .iter()
            .enumerate()
            .map(|(idx, mat)| (mat.name.clone(), idx as u32))
            .collect();

        // Generate instances
        let mut instance_transforms = Vec::with_capacity(scene.instance_count());
        let mut instance_material_ids = Vec::with_capacity(scene.instance_count());
        let mut instance_prototype_ids = Vec::with_capacity(scene.instance_count());
        let instances: Vec<InstanceData> = if scene.instances().is_empty() {
            scene
                .prototypes
                .iter()
                .enumerate()
                .map(|(proto_id, proto)| {
                    let model_matrix = Mat4::IDENTITY;
                    instance_transforms.push(model_matrix);
                    instance_prototype_ids.push(proto_id);
                    let material_id = proto
                        .material
                        .as_ref()
                        .and_then(|mat| material_index_by_name.get(&mat.name).copied())
                        .unwrap_or(0);
                    instance_material_ids.push(material_id);
                    InstanceData {
                        model_matrix: model_matrix.to_cols_array_2d(),
                        material_id,
                    }
                })
                .collect()
        } else {
            scene
                .instances()
                .iter()
                .map(|inst| {
                    let model_matrix = inst.model_matrix();
                    instance_transforms.push(model_matrix);
                    instance_prototype_ids.push(inst.prototype_id);
                    let material_id = scene
                        .prototypes
                        .get(inst.prototype_id)
                        .and_then(|proto| proto.material.as_ref())
                        .and_then(|mat| material_index_by_name.get(&mat.name).copied())
                        .unwrap_or(0);
                    instance_material_ids.push(material_id);
                    InstanceData {
                        model_matrix: model_matrix.to_cols_array_2d(),
                        material_id,
                    }
                })
                .collect()
        };

        // Write instances
        self.queue
            .write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(&instances));

        // Compute prototype AABB for culling
        let prototype_aabb = Aabb::from_points(mesh_data.bounds_min, mesh_data.bounds_max);
        let instance_aabbs: Vec<Aabb> = instance_transforms
            .iter()
            .map(|transform| transform.transform_aabb(&prototype_aabb))
            .collect();

        let triangles_per_instance = mesh_data.indices.len() as u32 / 3;
        self.culling
            .set_prototype_aabb(&self.device, prototype_aabb, triangles_per_instance);
        self.culling.instance_aabbs = instance_aabbs;
        self.culling.visible_count = instances.len() as u32;

        // Calculate world bounds for camera framing
        let world_bounds = scene.world_bounds();
        let mesh_center = Vec3::new(
            (world_bounds.x.min + world_bounds.x.max) * 0.5,
            (world_bounds.y.min + world_bounds.y.max) * 0.5,
            (world_bounds.z.min + world_bounds.z.max) * 0.5,
        );
        let world_extent = Vec3::new(
            world_bounds.x.max - world_bounds.x.min,
            world_bounds.y.max - world_bounds.y.min,
            world_bounds.z.max - world_bounds.z.min,
        );
        let camera_distance = world_extent.length() * 1.5;

        // Update renderer state
        self.num_indices = mesh_data.indices.len() as u32;
        self.num_instances = instances.len() as u32;
        self.mesh_bounds_min = mesh_data.bounds_min;
        self.mesh_bounds_max = mesh_data.bounds_max;
        self.num_triangles = triangles_per_instance as u64 * instances.len() as u64;
        self.mesh_data = mesh_data;
        self.current_transforms = instance_transforms.clone();
        self.instance_transforms = instance_transforms;
        self.instance_material_ids = instance_material_ids;
        self.instance_prototype_ids = instance_prototype_ids;
        // Build prim path mapping for USD export
        self.instance_prim_paths = scene
            .instances()
            .iter()
            .enumerate()
            .map(|(idx, inst)| {
                let proto_name = scene
                    .prototypes
                    .get(inst.prototype_id)
                    .map(|p| p.name.as_str())
                    .unwrap_or("unknown");
                format!("/{}/instance_{}", proto_name, idx)
            })
            .collect();
        self.instance_animations = scene.instance_animations().to_vec();
        self.last_evaluated_frame = 0.0;
        self.scene_material = scene_material.clone();
        self.scene_materials = scene.materials.clone();
        self.scene_cameras = scene.cameras.clone();
        // Reset stale scene camera selection
        if let crate::ivar_state::CameraSource::SceneCamera(idx) = self.viewport_camera_source {
            if idx >= self.scene_cameras.len() {
                self.viewport_camera_source = crate::ivar_state::CameraSource::Viewport;
                self.camera_locked = false;
            }
        }

        // Update material uniform
        self.material_uniform = MaterialUniform::from_material(&scene_material);
        self.queue.write_buffer(
            &self.material_buffer,
            0,
            bytemuck::cast_slice(&[self.material_uniform]),
        );

        // Frame camera
        self.camera.target = mesh_center;
        self.camera.distance = camera_distance;
        self.camera.near = camera_distance * 0.01;
        self.camera.far = camera_distance * 20.0;
        self.camera.update_position_from_angles();
        self.update_camera();

        // Invalidate Ivar scene
        self.ivar_state.world = None;
        self.ivar_state.build_status = BuildStatus::NotStarted;
        self.ivar_state.cancel_flag.store(true, Ordering::Relaxed);
        self.ivar_state.render_complete = false;

        // Update lights
        self.update_lights(&scene.lights);

        log::info!(
            "Scene data loaded: {} triangles x {} instances",
            self.num_indices / 3,
            self.num_instances
        );

        // Build pick scene for viewport selection
        self.rebuild_pick_scene();

        Ok(())
    }

    /// Create and load a procedural primitive into the viewport.
    pub fn load_primitive(&mut self, kind: bif_core::PrimitiveKind, size: f32) -> Result<()> {
        use bif_core::primitives::{create_camera_wireframe, create_cube, create_sphere};

        let (mesh, name) = match kind {
            bif_core::PrimitiveKind::Cube => (create_cube(size), "Cube"),
            bif_core::PrimitiveKind::Sphere => (create_sphere(size, 32), "Sphere"),
            bif_core::PrimitiveKind::Camera => (create_camera_wireframe(), "Camera"),
        };

        let mut scene = bif_core::Scene::new(name);
        let proto_id = scene.add_prototype(Arc::new(mesh), name.to_string());
        scene.add_instance(proto_id, bif_core::Transform::default());

        // Register camera primitive as a scene camera (unique name)
        if kind == bif_core::PrimitiveKind::Camera {
            let instance_index = scene.instance_count() - 1;
            let cam_name = if scene.cameras.is_empty() {
                name.to_string()
            } else {
                format!("{}_{}", name, scene.cameras.len() + 1)
            };
            scene.cameras.push(bif_core::SceneCamera {
                name: cam_name,
                instance_index,
                fov_y: 45.0_f32.to_radians(),
                near: 0.1,
                far: 1000.0,
            });
        }

        self.load_scene_data(&scene)?;
        log::info!("Primitive loaded: {} (size={})", name, size);
        Ok(())
    }

    /// Load a USD scene file and update the viewport
    ///
    /// This method reloads the viewport with a new USD file:
    /// 1. Loads the USD file via the C++ bridge
    /// 2. Converts geometry to GPU-ready buffers
    /// 3. Updates the scene browser with the new hierarchy
    /// 4. Invalidates the Ivar cache for re-rendering
    pub fn load_usd_scene<P: AsRef<std::path::Path>>(&mut self, path: P) -> Result<()> {
        use bif_core::usd::load_usd_with_stage;

        let path = path.as_ref();
        log::info!("Loading USD scene: {:?}", path);
        let viewport_load_start = Instant::now();

        // Check if file exists
        if !path.exists() {
            return Err(anyhow::anyhow!("File not found: {:?}", path));
        }

        // Load USD file via C++ bridge (handles usda, usdc, usd)
        let (scene, stage) = load_usd_with_stage(path).map_err(|e| {
            log::error!("USD bridge error: {:?}", e);
            log::error!("Hint: Ensure USD environment is set up. Run: . .\\setup_usd_env.ps1");
            anyhow::anyhow!("Failed to load USD: {}", e)
        })?;

        if scene.prototypes.is_empty() {
            return Err(anyhow::anyhow!("Scene has no geometry"));
        }

        // Multi-draw architecture: create per-prototype GPU buffers
        // This allows proper instancing without baking transforms into vertices
        let use_multi_draw = scene.prototypes.len() > 1;

        if use_multi_draw {
            log::info!(
                "Scene has {} prototypes - using multi-draw with per-prototype buffers",
                scene.prototypes.len()
            );
        }

        // Create per-prototype GPU data
        let gpu_start = Instant::now();
        let prototype_gpu_data: Vec<PrototypeGpuData> = scene
            .prototypes
            .iter()
            .enumerate()
            .map(|(proto_id, proto)| {
                let mesh_data = MeshData::from_core_mesh(&proto.mesh);

                let vertex_buffer =
                    self.device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some(&format!("Prototype {} Vertex Buffer", proto_id)),
                            contents: bytemuck::cast_slice(&mesh_data.vertices),
                            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                        });

                let index_buffer =
                    self.device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some(&format!("Prototype {} Index Buffer", proto_id)),
                            contents: bytemuck::cast_slice(&mesh_data.indices),
                            usage: wgpu::BufferUsages::INDEX,
                        });

                // Per-triangle material buffer if present
                let triangle_material_buffer =
                    mesh_data.triangle_material_ids.as_ref().map(|tri_mats| {
                        self.device
                            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                label: Some(&format!(
                                    "Prototype {} Triangle Material Buffer",
                                    proto_id
                                )),
                                contents: bytemuck::cast_slice(tri_mats),
                                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                            })
                    });

                log::debug!(
                    "Prototype {}: {} vertices, {} indices",
                    proto_id,
                    mesh_data.vertices.len(),
                    mesh_data.indices.len()
                );

                PrototypeGpuData {
                    vertex_buffer,
                    index_buffer,
                    num_indices: mesh_data.indices.len() as u32,
                    num_vertices: mesh_data.vertices.len() as u32,
                    prototype_id: proto_id,
                    mesh_idx: proto_id,
                    triangle_material_buffer,
                    vertices: mesh_data.vertices.clone(),
                }
            })
            .collect();

        // For backwards compatibility, also create a combined mesh_data for single-draw fallback
        // and for Ivar rendering (which expects a single mesh)
        let mesh_data = if scene.prototypes.len() == 1 {
            MeshData::from_core_mesh(&scene.prototypes[0].mesh)
        } else if !scene.instances().is_empty() {
            // Instanced scene: combine prototypes with instance transforms
            let mut meshes_with_transforms: Vec<(&bif_core::Mesh, Mat4, usize)> = Vec::new();
            for (mesh_idx, inst) in scene.instances().iter().enumerate() {
                if let Some(proto) = scene.prototypes.get(inst.prototype_id) {
                    meshes_with_transforms.push((&proto.mesh, inst.model_matrix(), mesh_idx));
                }
            }
            MeshData::combine_with_transforms(&meshes_with_transforms)
        } else {
            // Direct meshes (no instancers): combine prototypes with identity transforms
            let meshes_with_transforms: Vec<(&bif_core::Mesh, Mat4, usize)> = scene
                .prototypes
                .iter()
                .enumerate()
                .map(|(idx, proto)| (proto.mesh.as_ref(), Mat4::IDENTITY, idx))
                .collect();
            MeshData::combine_with_transforms(&meshes_with_transforms)
        };

        // Get material from first prototype (or use default)
        let scene_material = scene
            .prototypes
            .first()
            .and_then(|p| p.material.as_ref())
            .map(|m| (**m).clone())
            .unwrap_or_default();
        log::info!(
            "Material: {} (metallic={:.2}, roughness={:.2})",
            scene_material.name,
            scene_material.metallic,
            scene_material.roughness
        );

        log::info!(
            "Loaded {} vertices, {} indices from USD scene",
            mesh_data.vertices.len(),
            mesh_data.indices.len()
        );
        let gpu_time = gpu_start.elapsed();

        // Refresh texture resources and material table for the new scene
        let texture_start = Instant::now();
        let base_dir = path.parent();
        self.gpu_textures = texture_loader::create_gpu_textures_for_scene(
            &self.device,
            &self.queue,
            &scene,
            base_dir,
        );
        let texture_time = texture_start.elapsed();
        let texture_count = self.gpu_textures.textures.len();

        let material_table = if scene.materials.is_empty() {
            vec![MaterialGpu::from_material(
                &bif_core::Material::default(),
                &self.gpu_textures,
            )]
        } else {
            scene
                .materials
                .iter()
                .map(|mat| MaterialGpu::from_material(mat.as_ref(), &self.gpu_textures))
                .collect()
        };
        self.material_table_len = material_table.len() as u32;
        self.material_table_buffer =
            self.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Material Table Buffer"),
                    contents: bytemuck::cast_slice(&material_table),
                    usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                });

        // Create triangle material buffer from mesh data
        if let Some(ref tri_mats) = mesh_data.triangle_material_ids {
            log::info!(
                "Creating triangle material buffer with {} entries",
                tri_mats.len()
            );
            self.triangle_material_buffer =
                self.device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Triangle Material Buffer"),
                        contents: bytemuck::cast_slice(tri_mats),
                        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                    });
            self.has_triangle_materials = true;
        } else {
            self.triangle_material_buffer =
                self.device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Triangle Material Buffer"),
                        contents: bytemuck::cast_slice(&[0xFFFFFFFFu32]),
                        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                    });
            self.has_triangle_materials = false;
        }

        self.material_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Material Bind Group"),
            layout: &self.material_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.material_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.material_table_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.triangle_material_buffer.as_entire_binding(),
                },
            ],
        });

        let texture_view_refs: Vec<&wgpu::TextureView> = self.gpu_textures.views.iter().collect();
        self.texture_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Texture Bind Group"),
            layout: &self.texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureViewArray(&texture_view_refs),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.texture_sampler),
                },
            ],
        });

        // Create new vertex buffer (COPY_DST needed for vertex animation updates)
        let vertex_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Vertex Buffer"),
                contents: bytemuck::cast_slice(&mesh_data.vertices),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            });

        // Create new index buffer
        let index_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Index Buffer"),
                contents: bytemuck::cast_slice(&mesh_data.indices),
                usage: wgpu::BufferUsages::INDEX,
            });

        let material_index_by_name: HashMap<String, u32> = scene
            .materials
            .iter()
            .enumerate()
            .map(|(idx, mat)| (mat.name.clone(), idx as u32))
            .collect();

        // Generate instances from scene
        // Multi-draw (wgpu viewport) uses per-instance transforms
        // For Ivar (ray tracing): if multi-prototype, transforms are baked into combined mesh_data
        // so build_ivar_scene* will use identity transform when use_multi_draw is true
        let mut instance_transforms = Vec::with_capacity(scene.instance_count());
        let mut instance_material_ids = Vec::with_capacity(scene.instance_count());
        let mut instance_prototype_ids = Vec::with_capacity(scene.instance_count());
        let instances: Vec<InstanceData> = if scene.instances().is_empty() {
            scene
                .prototypes
                .iter()
                .enumerate()
                .map(|(proto_id, proto)| {
                    let model_matrix = Mat4::IDENTITY;
                    instance_transforms.push(model_matrix);
                    instance_prototype_ids.push(proto_id);
                    let material_id = proto
                        .material
                        .as_ref()
                        .and_then(|mat| material_index_by_name.get(&mat.name).copied())
                        .unwrap_or(0);
                    instance_material_ids.push(material_id);
                    InstanceData {
                        model_matrix: model_matrix.to_cols_array_2d(),
                        material_id,
                    }
                })
                .collect()
        } else {
            scene
                .instances()
                .iter()
                .map(|inst| {
                    let model_matrix = inst.model_matrix();
                    instance_transforms.push(model_matrix);
                    instance_prototype_ids.push(inst.prototype_id);
                    let material_id = scene
                        .prototypes
                        .get(inst.prototype_id)
                        .and_then(|proto| proto.material.as_ref())
                        .and_then(|mat| material_index_by_name.get(&mat.name).copied())
                        .unwrap_or(0);
                    instance_material_ids.push(material_id);
                    InstanceData {
                        model_matrix: model_matrix.to_cols_array_2d(),
                        material_id,
                    }
                })
                .collect()
        };

        // Warn if instance count exceeds buffer capacity
        if instances.len() > MAX_INSTANCES as usize {
            log::warn!(
                "Instance count {} exceeds buffer capacity {}. Some instances will be truncated.",
                instances.len(),
                MAX_INSTANCES
            );
        }

        // Compute prototype AABB and per-instance world-space AABBs for frustum culling
        let prototype_aabb = Aabb::from_points(mesh_data.bounds_min, mesh_data.bounds_max);
        let instance_aabbs: Vec<Aabb> = instance_transforms
            .iter()
            .map(|transform| transform.transform_aabb(&prototype_aabb))
            .collect();

        // Write instances to dynamic buffer (reuse existing preallocated buffer)
        self.queue
            .write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(&instances));

        log::info!("Created {} instances from USD scene", instances.len());

        self.instance_material_ids = instance_material_ids;
        self.instance_prototype_ids = instance_prototype_ids;
        // Build prim path mapping for USD export
        self.instance_prim_paths = scene
            .instances()
            .iter()
            .enumerate()
            .map(|(idx, inst)| {
                let proto_name = scene
                    .prototypes
                    .get(inst.prototype_id)
                    .map(|p| p.name.as_str())
                    .unwrap_or("unknown");
                format!("/{}/instance_{}", proto_name, idx)
            })
            .collect();

        // Store animation data for viewport playback
        self.instance_animations = scene.instance_animations().to_vec();
        self.last_evaluated_frame = 0.0;

        let animated_count = self
            .instance_animations
            .iter()
            .filter(|opt| opt.as_ref().is_some_and(|anim| anim.is_animated()))
            .count();
        if animated_count > 0 {
            log::info!(
                "{} of {} instances have animation data",
                animated_count,
                instances.len()
            );
        }

        // Detect meshes with vertex animation (deformation)
        self.vertex_animated_meshes.clear();
        let mesh_count = stage.mesh_count().unwrap_or(0);
        for mesh_idx in 0..mesh_count {
            if let Ok(times) = stage.get_mesh_vertex_animation_times(mesh_idx) {
                if !times.is_empty() {
                    log::info!(
                        "Mesh {} has vertex animation ({} time samples)",
                        mesh_idx,
                        times.len()
                    );
                    self.vertex_animated_meshes.push(mesh_idx);
                }
            }
        }

        // Calculate world bounds for camera framing
        let world_bounds = scene.world_bounds();
        let mesh_center = Vec3::new(
            (world_bounds.x.min + world_bounds.x.max) * 0.5,
            (world_bounds.y.min + world_bounds.y.max) * 0.5,
            (world_bounds.z.min + world_bounds.z.max) * 0.5,
        );
        let world_extent = Vec3::new(
            world_bounds.x.max - world_bounds.x.min,
            world_bounds.y.max - world_bounds.y.min,
            world_bounds.z.max - world_bounds.z.min,
        );
        let mesh_size = world_extent.length();
        let camera_distance = mesh_size * 1.5;

        // Update renderer state
        self.vertex_buffer = vertex_buffer;
        self.index_buffer = index_buffer;
        self.num_indices = mesh_data.indices.len() as u32;
        // Note: instance_buffer is reused (dynamic), don't reassign
        self.num_instances = instances.len() as u32;
        self.culling.visible_count = instances.len() as u32;
        self.mesh_bounds_min = mesh_data.bounds_min;
        self.mesh_bounds_max = mesh_data.bounds_max;
        self.mesh_data = mesh_data;
        self.current_transforms = instance_transforms.clone();
        self.instance_transforms = instance_transforms;
        self.scene_material = scene_material.clone();
        self.scene_materials = scene.materials.clone();
        self.scene_cameras = scene.cameras.clone();
        // Reset stale scene camera selection
        if let crate::ivar_state::CameraSource::SceneCamera(idx) = self.viewport_camera_source {
            if idx >= self.scene_cameras.len() {
                self.viewport_camera_source = crate::ivar_state::CameraSource::Viewport;
                self.camera_locked = false;
            }
        }
        self.texture_base_dir = path.parent().map(|p| p.to_path_buf());

        // Store multi-draw state
        self.multi_draw.prototype_gpu_data = prototype_gpu_data;
        self.multi_draw.enabled = use_multi_draw;

        // Group instances by prototype for multi-draw rendering
        self.multi_draw.rebuild_instance_groups(
            &self.instance_transforms,
            &self.instance_prototype_ids,
            &self.instance_material_ids,
        );

        if use_multi_draw {
            log::info!(
                "Multi-draw: {} prototypes, {} total instances across groups",
                self.multi_draw.prototype_gpu_data.len(),
                self.multi_draw
                    .instance_groups
                    .values()
                    .map(|v| v.len())
                    .sum::<usize>()
            );
        }

        // Update material uniform buffer for viewport PBR
        self.material_uniform = MaterialUniform::from_material(&scene_material);
        self.queue.write_buffer(
            &self.material_buffer,
            0,
            bytemuck::cast_slice(&[self.material_uniform]),
        );

        // Update culling manager with new prototype and instance AABBs
        let triangles_per_instance = self.num_indices / 3;
        self.culling
            .set_prototype_aabb(&self.device, prototype_aabb, triangles_per_instance);
        self.culling.instance_aabbs = instance_aabbs;
        self.culling.lod_box_count = 0;
        self.num_triangles = triangles_per_instance as u64 * self.num_instances as u64;
        log::info!(
            "Updated culling manager for prototype AABB: {:?} to {:?}",
            prototype_aabb.min_point(),
            prototype_aabb.max_point()
        );

        // Update USD stage for scene browser (wrapped in Arc for batch render sharing)
        let stage = Arc::new(stage);

        // Log available cameras for batch render
        match stage.camera_paths() {
            Ok(paths) if !paths.is_empty() => {
                log::info!("Found {} USD camera(s): {:?}", paths.len(), paths);
            }
            Ok(_) => {
                log::info!("No USD cameras found in scene");
            }
            Err(e) => {
                log::warn!("Failed to query cameras: {:?}", e);
            }
        }

        self.usd_stage = Some(stage);

        // Reset scene browser selection
        self.selected_prim_path = None;
        self.selected_prim_properties = None;

        // Update camera to frame the scene
        self.camera.target = mesh_center;
        self.camera.distance = camera_distance;
        self.camera.near = camera_distance * 0.01;
        self.camera.far = camera_distance * 20.0;
        self.camera.update_position_from_angles();
        self.update_camera();

        // Invalidate Ivar scene cache
        self.ivar_state.world = None;
        self.ivar_state.build_status = BuildStatus::NotStarted;
        self.ivar_state.cancel_flag.store(true, Ordering::Relaxed);
        self.ivar_state.render_complete = false;

        // Initialize timeline from scene data
        if let Some(ref timeline) = scene.timeline {
            self.timeline_state.set_from_scene(
                timeline.start_frame,
                timeline.end_frame,
                timeline.fps,
            );
            log::info!(
                "Timeline initialized: frames {:.0}-{:.0} @ {:.0} fps",
                timeline.start_frame,
                timeline.end_frame,
                timeline.fps
            );
        } else if !self.vertex_animated_meshes.is_empty() {
            // No scene timeline, but we have vertex animation - detect time range from vertex animation
            let mut min_time = f64::MAX;
            let mut max_time = f64::MIN;

            if let Some(ref usd_stage) = self.usd_stage {
                for &mesh_idx in &self.vertex_animated_meshes {
                    if let Ok(times) = usd_stage.get_mesh_vertex_animation_times(mesh_idx) {
                        for &t in &times {
                            min_time = min_time.min(t);
                            max_time = max_time.max(t);
                        }
                    }
                }
            }

            if min_time < max_time {
                // Use USD default 24 fps if not specified
                let fps = 24.0;
                self.timeline_state.set_from_scene(min_time, max_time, fps);
                log::info!(
                    "Timeline initialized from vertex animation: frames {:.0}-{:.0} @ {:.0} fps",
                    min_time,
                    max_time,
                    fps
                );
            } else {
                self.timeline_state = TimelineState::default();
            }
        } else {
            let mut min_time = f64::MAX;
            let mut max_time = f64::MIN;

            for anim in self.instance_animations.iter().flatten() {
                if let Some(keyframes) = &anim.keyframes {
                    for kf in keyframes {
                        min_time = min_time.min(kf.time);
                        max_time = max_time.max(kf.time);
                    }
                }
            }

            if min_time < max_time {
                let fps = 24.0;
                self.timeline_state.set_from_scene(min_time, max_time, fps);
                log::info!(
                    "Timeline initialized from transform animation: frames {:.0}-{:.0} @ {:.0} fps",
                    min_time,
                    max_time,
                    fps
                );
            } else {
                self.timeline_state = TimelineState::default();
            }
        }

        log::info!(
            "USD scene loaded successfully: {} triangles x {} instances",
            self.num_indices / 3,
            self.num_instances
        );

        // Update lights from scene
        self.update_lights(&scene.lights);

        // Build pick scene for viewport selection
        self.rebuild_pick_scene();

        // Log viewport timing breakdown
        let total_viewport_time = viewport_load_start.elapsed();
        log::info!("Viewport Setup:");
        log::info!("  GPU buffers: {:>7.1}ms", gpu_time.as_secs_f64() * 1000.0);
        log::info!(
            "  Textures:    {:>7.1}ms ({} textures)",
            texture_time.as_secs_f64() * 1000.0,
            texture_count
        );
        log::info!(
            "  Total:       {:>7.1}ms",
            total_viewport_time.as_secs_f64() * 1000.0
        );

        Ok(())
    }
}
