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

/// Resolve the USD prim path for an instance — use the instance's prim_path
/// if set, otherwise synthesise one from the prototype name and index.
fn resolve_prim_path(inst: &bif_core::Instance, scene: &bif_core::Scene, idx: usize) -> String {
    if !inst.prim_path.is_empty() {
        inst.prim_path.to_string()
    } else {
        let proto_name = scene
            .prototypes
            .get(inst.prototype_id)
            .map(|p| &*p.name)
            .unwrap_or("unknown");
        format!("/BIF/{}/{}", proto_name, idx)
    }
}

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

        let material_index_by_name: HashMap<Arc<str>, u32> = scene
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

        // Write instances (truncate at buffer capacity)
        if instances.len() > crate::MAX_INSTANCES as usize {
            log::warn!(
                "Instance count {} exceeds buffer capacity {}. Truncating.",
                instances.len(),
                crate::MAX_INSTANCES
            );
        }
        let write_count = instances.len().min(crate::MAX_INSTANCES as usize);
        self.queue.write_buffer(
            &self.instance_buffer,
            0,
            bytemuck::cast_slice(&instances[..write_count]),
        );

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
        self.culling.visible_count = write_count as u32;

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
        self.num_instances = write_count as u32;
        self.mesh_bounds_min = mesh_data.bounds_min;
        self.mesh_bounds_max = mesh_data.bounds_max;
        self.num_triangles = triangles_per_instance as u64 * write_count as u64;
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
            .map(|(idx, inst)| resolve_prim_path(inst, scene, idx))
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

    /// Generate a unique name for a primitive (e.g. "Cube", "Cube_2", "Cube_3").
    fn unique_primitive_name(&mut self, base: &str) -> String {
        let counter = self
            .primitive_name_counters
            .entry(base.to_string())
            .or_insert(0);
        *counter += 1;
        if *counter == 1 {
            base.to_string()
        } else {
            format!("{}_{}", base, counter)
        }
    }

    /// Remove a prototype and re-index all maps that reference prototype IDs.
    ///
    /// Updates `node_proto_map` and `instancer_results` to account for the
    /// index shift after removal. Returns `true` if the prototype existed.
    pub fn remove_and_reindex_prototype(&mut self, proto_id: usize) -> bool {
        if !self.working_scene.remove_prototype(proto_id) {
            log::error!("Prototype {} not found for removal", proto_id);
            return false;
        }
        // Re-index node_proto_map (unified: single + multi-proto nodes)
        for ids in self.node_proto_map.values_mut() {
            ids.retain(|id| *id != proto_id);
            for id in ids.iter_mut() {
                if *id > proto_id {
                    *id -= 1;
                }
            }
        }
        // Remove instancer results referencing deleted prototype
        self.instancer_results.retain(|_node_id, instances| {
            !instances.iter().any(|inst| inst.prototype_id == proto_id)
        });
        // Re-index remaining instancer_results prototype IDs
        for instances in self.instancer_results.values_mut() {
            for inst in instances.iter_mut() {
                if inst.prototype_id > proto_id {
                    inst.prototype_id -= 1;
                }
            }
        }
        true
    }

    /// Add a primitive to the working scene and rebuild GPU state.
    ///
    /// Returns the prototype ID in the working scene.
    pub fn load_primitive(&mut self, kind: bif_core::PrimitiveKind, size: f32) -> Result<usize> {
        use bif_core::primitives::{create_camera_wireframe, create_cube, create_sphere};

        let (mesh, base_name) = match kind {
            bif_core::PrimitiveKind::Cube => (create_cube(size), "Cube"),
            bif_core::PrimitiveKind::Sphere => (create_sphere(size, 32), "Sphere"),
            bif_core::PrimitiveKind::Camera => (create_camera_wireframe(), "Camera"),
        };

        let unique_name = self.unique_primitive_name(base_name);
        let proto_id = self
            .working_scene
            .add_prototype(Arc::new(mesh), unique_name.clone());
        self.working_scene
            .add_instance(proto_id, bif_core::Transform::default());

        // Register camera primitive as a scene camera
        if kind == bif_core::PrimitiveKind::Camera {
            let instance_index = self.working_scene.instance_count() - 1;
            self.working_scene.cameras.push(bif_core::SceneCamera {
                name: unique_name.clone(),
                instance_index,
                fov_y: 45.0_f32.to_radians(),
                near: 0.1,
                far: 1000.0,
            });
        }

        self.reload_working_scene()?;
        log::info!(
            "Primitive added to working scene: {} (size={}, proto_id={})",
            unique_name,
            size,
            proto_id
        );
        Ok(proto_id)
    }

    /// Add a primitive to the working scene by name (used by undo/redo).
    pub fn add_primitive_by_name(
        &mut self,
        kind: bif_core::PrimitiveKind,
        size: f32,
        name: &str,
    ) -> Result<usize> {
        use bif_core::primitives::{create_camera_wireframe, create_cube, create_sphere};

        let mesh = match kind {
            bif_core::PrimitiveKind::Cube => create_cube(size),
            bif_core::PrimitiveKind::Sphere => create_sphere(size, 32),
            bif_core::PrimitiveKind::Camera => create_camera_wireframe(),
        };

        let proto_id = self
            .working_scene
            .add_prototype(Arc::new(mesh), name.to_string());
        self.working_scene
            .add_instance(proto_id, bif_core::Transform::default());

        if kind == bif_core::PrimitiveKind::Camera {
            let instance_index = self.working_scene.instance_count() - 1;
            self.working_scene.cameras.push(bif_core::SceneCamera {
                name: name.to_string(),
                instance_index,
                fov_y: 45.0_f32.to_radians(),
                near: 0.1,
                far: 1000.0,
            });
        }

        self.reload_working_scene()?;
        Ok(proto_id)
    }

    /// Remove a prototype from the working scene and rebuild GPU state.
    pub fn remove_primitive(&mut self, proto_id: usize) -> Result<()> {
        if !self.working_scene.remove_prototype(proto_id) {
            anyhow::bail!("Prototype {} not found in working scene", proto_id);
        }
        self.reload_working_scene()?;
        log::info!("Removed prototype {} from working scene", proto_id);
        Ok(())
    }

    /// Rebuild all GPU state from the current working scene.
    ///
    /// Called after adding/removing primitives or merging USD data.
    pub fn reload_working_scene(&mut self) -> Result<()> {
        let scene = &self.working_scene;

        if scene.prototypes.is_empty() {
            // Empty scene — reset to blank
            self.num_indices = 0;
            self.num_instances = 0;
            self.num_triangles = 0;
            self.multi_draw.enabled = false;
            self.multi_draw.prototype_gpu_data.clear();
            self.multi_draw.instance_groups.clear();
            self.instance_transforms.clear();
            self.current_transforms.clear();
            self.instance_material_ids.clear();
            self.instance_prototype_ids.clear();
            self.instance_prim_paths.clear();
            self.instance_animations = scene.instance_animations().to_vec();
            self.scene_cameras = scene.cameras.clone();
            self.culling.instance_aabbs.clear();
            self.culling.visible_count = 0;
            self.culling.mark_dirty();
            self.pick_scene = None;
            self.mesh_data = MeshData {
                vertices: vec![],
                indices: vec![],
                bounds_min: Vec3::ZERO,
                bounds_max: Vec3::ZERO,
                triangle_material_ids: None,
                mesh_ranges: None,
            };
            // Invalidate Ivar
            self.ivar_state.world = None;
            self.ivar_state.build_status = BuildStatus::NotStarted;
            self.ivar_state.cancel_flag.store(true, Ordering::Relaxed);
            self.ivar_state.render_complete = false;
            return Ok(());
        }

        let use_multi_draw = scene.prototypes.len() > 1;

        // Compute active node set from display flag (None = everything active)
        let active_nodes: Option<std::collections::HashSet<egui_snarl::NodeId>> = self
            .node_graph_state
            .display_node
            .map(|dn| crate::node_graph::collect_upstream_nodes(dn, &self.node_graph_state.snarl));
        let is_node_active = |node_id: &egui_snarl::NodeId| {
            active_nodes.as_ref().is_none_or(|s| s.contains(node_id))
        };

        // Prototypes consumed by instancers or scatter surfaces — hide from viewport
        let instanced_proto_ids: std::collections::HashSet<usize> = self
            .instancer_results
            .iter()
            .filter(|(nid, _)| is_node_active(nid))
            .flat_map(|(_, insts)| insts.iter().map(|inst| inst.prototype_id))
            .collect();
        let scatter_surface_ids: std::collections::HashSet<usize> = self
            .node_scatter_surface_map
            .iter()
            .filter(|(nid, _)| is_node_active(nid))
            .map(|(_, &pid)| pid)
            .collect();
        // Also hide prototypes from inactive nodes (display flag gating)
        let display_hidden_proto_ids: std::collections::HashSet<usize> = self
            .node_proto_map
            .iter()
            .filter(|(nid, _)| !is_node_active(nid))
            .flat_map(|(_, pids)| pids.iter().copied())
            .collect();
        let hidden_proto_ids: std::collections::HashSet<usize> = instanced_proto_ids
            .union(&scatter_surface_ids)
            .copied()
            .chain(display_hidden_proto_ids.iter().copied())
            .collect();
        if !hidden_proto_ids.is_empty() {
            log::debug!(
                "Hiding prototypes: {:?} (instanced: {:?}, scatter surface: {:?})",
                hidden_proto_ids,
                instanced_proto_ids,
                scatter_surface_ids,
            );
        }

        // Create per-prototype GPU data
        let prototype_gpu_data: Vec<PrototypeGpuData> = scene
            .prototypes
            .iter()
            .enumerate()
            .map(|(proto_id, proto)| {
                let md = MeshData::from_core_mesh(&proto.mesh);

                let vertex_buffer =
                    self.device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some(&format!("WS Proto {} VB", proto_id)),
                            contents: bytemuck::cast_slice(&md.vertices),
                            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                        });

                let index_buffer =
                    self.device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some(&format!("WS Proto {} IB", proto_id)),
                            contents: bytemuck::cast_slice(&md.indices),
                            usage: wgpu::BufferUsages::INDEX,
                        });

                let triangle_material_buffer = md.triangle_material_ids.as_ref().map(|tri_mats| {
                    self.device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some(&format!("WS Proto {} TMB", proto_id)),
                            contents: bytemuck::cast_slice(tri_mats),
                            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                        })
                });

                PrototypeGpuData {
                    vertex_buffer,
                    index_buffer,
                    num_indices: md.indices.len() as u32,
                    num_vertices: md.vertices.len() as u32,
                    prototype_id: proto_id,
                    mesh_idx: proto_id,
                    triangle_material_buffer,
                    vertices: md.vertices,
                }
            })
            .collect();

        // Combined mesh_data for single-draw fallback and Ivar.
        // Single prototype: raw mesh (Ivar applies per-instance transforms).
        // Multi prototype: bake scene + instancer transforms into vertices
        // so Ivar can use a single identity transform.
        let mesh_data = if scene.prototypes.len() == 1 {
            MeshData::from_core_mesh(&scene.prototypes[0].mesh)
        } else if !scene.instances().is_empty() {
            let mut meshes_with_transforms: Vec<(&bif_core::Mesh, Mat4, usize)> = scene
                .instances()
                .iter()
                .enumerate()
                .filter(|(_idx, inst)| !hidden_proto_ids.contains(&inst.prototype_id))
                .filter_map(|(mesh_idx, inst)| {
                    scene
                        .prototypes
                        .get(inst.prototype_id)
                        .map(|proto| (proto.mesh.as_ref(), inst.model_matrix(), mesh_idx))
                })
                .collect();
            // Bake active instancer instances so Ivar (identity transform) can render them
            let base_idx = meshes_with_transforms.len();
            for (i, inst) in self
                .instancer_results
                .iter()
                .filter(|(nid, _)| is_node_active(nid))
                .flat_map(|(_, insts)| insts.iter())
                .enumerate()
            {
                if let Some(proto) = scene.prototypes.get(inst.prototype_id) {
                    meshes_with_transforms.push((
                        proto.mesh.as_ref(),
                        inst.model_matrix(),
                        base_idx + i,
                    ));
                }
            }
            MeshData::combine_with_transforms(&meshes_with_transforms)
        } else {
            let meshes_with_transforms: Vec<(&bif_core::Mesh, Mat4, usize)> = scene
                .prototypes
                .iter()
                .enumerate()
                .map(|(idx, proto)| (proto.mesh.as_ref(), Mat4::IDENTITY, idx))
                .collect();
            MeshData::combine_with_transforms(&meshes_with_transforms)
        };

        // Only rebuild GPU textures when materials actually changed (not on every
        // Xform drag or display toggle). Texture loading is expensive — disk I/O,
        // decode, GPU upload.
        if self.materials_dirty {
            self.gpu_textures = texture_loader::create_gpu_textures_for_scene(
                &self.device,
                &self.queue,
                scene,
                self.texture_base_dir.as_deref(),
            );

            let texture_view_refs: Vec<&wgpu::TextureView> =
                self.gpu_textures.views.iter().collect();
            self.texture_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("WS Texture Bind Group"),
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
            self.materials_dirty = false;
        }

        // Material table (use default for primitives)
        let material_table = if scene.materials.is_empty() {
            vec![crate::gpu_types::MaterialGpu::from_material(
                &bif_core::Material::default(),
                &self.gpu_textures,
            )]
        } else {
            scene
                .materials
                .iter()
                .map(|mat| {
                    crate::gpu_types::MaterialGpu::from_material(mat.as_ref(), &self.gpu_textures)
                })
                .collect()
        };
        self.material_table_len = material_table.len() as u32;
        self.material_table_buffer =
            self.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("WS Material Table Buffer"),
                    contents: bytemuck::cast_slice(&material_table),
                    usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                });

        // Triangle material buffer
        if let Some(ref tri_mats) = mesh_data.triangle_material_ids {
            self.triangle_material_buffer =
                self.device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("WS Triangle Material Buffer"),
                        contents: bytemuck::cast_slice(tri_mats),
                        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                    });
            self.has_triangle_materials = true;
        } else {
            self.triangle_material_buffer =
                self.device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("WS Triangle Material Buffer"),
                        contents: bytemuck::cast_slice(&[0xFFFFFFFFu32]),
                        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                    });
            self.has_triangle_materials = false;
        }

        // Rebuild material bind group
        self.material_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("WS Material Bind Group"),
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

        // Vertex and index buffers (combined, for single-draw fallback)
        self.vertex_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("WS Vertex Buffer"),
                contents: bytemuck::cast_slice(&mesh_data.vertices),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            });
        self.index_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("WS Index Buffer"),
                contents: bytemuck::cast_slice(&mesh_data.indices),
                usage: wgpu::BufferUsages::INDEX,
            });

        // Build material index lookup
        let material_index_by_name: HashMap<Arc<str>, u32> = scene
            .materials
            .iter()
            .enumerate()
            .map(|(idx, mat)| (mat.name.clone(), idx as u32))
            .collect();

        // Generate instances (pre-allocate for scene + instancer instances)
        let instancer_count: usize = self
            .instancer_results
            .iter()
            .filter(|(nid, _)| is_node_active(nid))
            .map(|(_, v)| v.len())
            .sum();
        let total_capacity = scene.instance_count() + instancer_count;
        let mut instance_transforms = Vec::with_capacity(total_capacity);
        let mut instance_material_ids = Vec::with_capacity(total_capacity);
        let mut instance_prototype_ids = Vec::with_capacity(total_capacity);
        let mut instances: Vec<InstanceData> = if scene.instances().is_empty() {
            scene
                .prototypes
                .iter()
                .enumerate()
                .filter(|(proto_id, _)| !hidden_proto_ids.contains(proto_id))
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
                .filter(|inst| !hidden_proto_ids.contains(&inst.prototype_id))
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

        // Append instancer-expanded instances (from active Point Instancer nodes)
        for inst in self
            .instancer_results
            .iter()
            .filter(|(nid, _)| is_node_active(nid))
            .flat_map(|(_, insts)| insts.iter())
        {
            let model_matrix = inst.model_matrix();
            instance_transforms.push(model_matrix);
            instance_prototype_ids.push(inst.prototype_id);
            let material_id = scene
                .prototypes
                .get(inst.prototype_id)
                .and_then(|p| p.material.as_ref())
                .and_then(|mat| material_index_by_name.get(&mat.name).copied())
                .unwrap_or(0);
            instance_material_ids.push(material_id);
            instances.push(InstanceData {
                model_matrix: model_matrix.to_cols_array_2d(),
                material_id,
            });
        }

        // Apply stage metadata corrections (axis + unit) if user toggled them on.
        if let Some(ref meta) = scene.stage_metadata {
            let mut correction = Mat4::IDENTITY;
            if self.apply_axis_correction && meta.up_axis == bif_core::usd::cpp_bridge::UpAxis::Z {
                // Rotate -90 degrees around X to convert Z-up → Y-up
                correction = Mat4::from_rotation_x(-std::f32::consts::FRAC_PI_2);
            }
            if self.apply_unit_scaling && (meta.meters_per_unit - 1.0).abs() > 1e-6 {
                let s = meta.meters_per_unit as f32;
                correction = Mat4::from_scale(bif_math::Vec3::splat(s)) * correction;
            }
            if correction != Mat4::IDENTITY {
                for inst in &mut instances {
                    let m = Mat4::from_cols_array_2d(&inst.model_matrix);
                    inst.model_matrix = (correction * m).to_cols_array_2d();
                }
            }
        }

        // Apply Xform node transforms in topological order (upstream-first).
        // Each Xform post-multiplies its T/R/S onto instances whose prototype
        // originated from any node upstream of the Xform's scene input.
        {
            use egui_snarl::{InPinId, NodeId as SnarlNodeId};

            // Collect active Xform nodes with their upstream depth for topological order.
            // Depth = max number of Xform nodes upstream (0 = no Xform parent).
            // Sorting by depth ensures upstream Xforms apply before downstream ones.
            struct XformEntry {
                node_id: SnarlNodeId,
                translate: [f32; 3],
                rotate: [f32; 3],
                scale: [f32; 3],
                depth: usize,
            }
            let mut xform_entries: Vec<XformEntry> = Vec::new();
            for (nid, node) in self.node_graph_state.snarl.node_ids() {
                if !is_node_active(&nid) {
                    continue;
                }
                if let crate::node_graph::SceneNode::Xform {
                    translate,
                    rotate,
                    scale,
                    ..
                } = node
                {
                    // Compute depth: count Xform nodes upstream of this one
                    let upstream = crate::node_graph::collect_upstream_nodes(
                        nid,
                        &self.node_graph_state.snarl,
                    );
                    let depth = upstream
                        .iter()
                        .filter(|&&uid| uid != nid)
                        .filter(|uid| {
                            matches!(
                                self.node_graph_state.snarl[**uid],
                                crate::node_graph::SceneNode::Xform { .. }
                            )
                        })
                        .count();
                    xform_entries.push(XformEntry {
                        node_id: nid,
                        translate: *translate,
                        rotate: *rotate,
                        scale: *scale,
                        depth,
                    });
                }
            }
            // Sort upstream-first (lowest depth first)
            xform_entries.sort_by_key(|e| e.depth);

            for entry in &xform_entries {
                // Build the Xform's 4x4 matrix from T/R/S (Euler XYZ degrees)
                let rx = entry.rotate[0].to_radians();
                let ry = entry.rotate[1].to_radians();
                let rz = entry.rotate[2].to_radians();
                let rotation = bif_math::Quat::from_euler(bif_math::EulerRot::XYZ, rx, ry, rz);
                let xform_mat = Mat4::from_scale_rotation_translation(
                    bif_math::Vec3::new(entry.scale[0], entry.scale[1], entry.scale[2]),
                    rotation,
                    bif_math::Vec3::new(entry.translate[0], entry.translate[1], entry.translate[2]),
                );

                // Skip identity transforms (T=0, R=0, S=1)
                if xform_mat.abs_diff_eq(Mat4::IDENTITY, 1e-7) {
                    continue;
                }

                // Walk upstream from this Xform's scene input to find source nodes
                let upstream = {
                    let in_pin = self.node_graph_state.snarl.in_pin(InPinId {
                        node: entry.node_id,
                        input: 0,
                    });
                    if let Some(remote) = in_pin.remotes.first() {
                        crate::node_graph::collect_upstream_nodes(
                            remote.node,
                            &self.node_graph_state.snarl,
                        )
                    } else {
                        continue; // No input connected
                    }
                };

                // Resolve which prototype IDs come from those upstream nodes
                let affected_proto_ids: std::collections::HashSet<usize> = self
                    .node_proto_map
                    .iter()
                    .filter(|(nid, _)| upstream.contains(nid))
                    .flat_map(|(_, pids)| pids.iter().copied())
                    .collect();

                if affected_proto_ids.is_empty() {
                    continue;
                }

                // Post-multiply onto affected instances
                for (i, proto_id) in instance_prototype_ids.iter().enumerate() {
                    if affected_proto_ids.contains(proto_id) {
                        instance_transforms[i] = xform_mat * instance_transforms[i];
                        instances[i].model_matrix = instance_transforms[i].to_cols_array_2d();
                    }
                }
            }
        }

        // Write instances to GPU (warn + truncate if exceeding buffer capacity)
        if instances.len() > crate::MAX_INSTANCES as usize {
            log::warn!(
                "Instance count {} exceeds buffer capacity {}. Truncating.",
                instances.len(),
                crate::MAX_INSTANCES
            );
        }
        let write_count = instances.len().min(crate::MAX_INSTANCES as usize);
        self.queue.write_buffer(
            &self.instance_buffer,
            0,
            bytemuck::cast_slice(&instances[..write_count]),
        );

        // Culling
        let prototype_aabb = Aabb::from_points(mesh_data.bounds_min, mesh_data.bounds_max);
        let instance_aabbs: Vec<Aabb> = instance_transforms
            .iter()
            .map(|transform| transform.transform_aabb(&prototype_aabb))
            .collect();
        let triangles_per_instance = if mesh_data.indices.is_empty() {
            0
        } else {
            mesh_data.indices.len() as u32 / 3
        };
        self.culling
            .set_prototype_aabb(&self.device, prototype_aabb, triangles_per_instance);
        self.culling.instance_aabbs = instance_aabbs;
        self.culling.visible_count = write_count as u32;

        // Update renderer state
        self.num_indices = mesh_data.indices.len() as u32;
        self.num_instances = write_count as u32;
        self.mesh_bounds_min = mesh_data.bounds_min;
        self.mesh_bounds_max = mesh_data.bounds_max;
        self.num_triangles = triangles_per_instance as u64 * write_count as u64;
        self.mesh_data = mesh_data;
        self.current_transforms = instance_transforms.clone();
        self.instance_transforms = instance_transforms;
        self.instance_material_ids = instance_material_ids;
        self.instance_prototype_ids = instance_prototype_ids;

        // Build prim paths for scene instances (skip instanced prototypes)
        let mut prim_paths: Vec<String> = scene
            .instances()
            .iter()
            .enumerate()
            .filter(|(_idx, inst)| !hidden_proto_ids.contains(&inst.prototype_id))
            .map(|(idx, inst)| resolve_prim_path(inst, scene, idx))
            .collect();
        // Extend for instancer-expanded instances (parallel to instance_transforms)
        let scene_inst_count = prim_paths.len();
        for (i, inst) in self
            .instancer_results
            .iter()
            .filter(|(nid, _)| is_node_active(nid))
            .flat_map(|(_, insts)| insts.iter())
            .enumerate()
        {
            let proto_name = scene
                .prototypes
                .get(inst.prototype_id)
                .map(|p| &*p.name)
                .unwrap_or("unknown");
            prim_paths.push(format!(
                "/BIF/{}/instancer_{}",
                proto_name,
                scene_inst_count + i
            ));
        }
        self.instance_prim_paths = prim_paths;

        // Animations: filtered scene instances + None entries for instancer instances
        let mut animations: Vec<_> = scene
            .instance_animations()
            .iter()
            .zip(scene.instances().iter())
            .filter(|(_, inst)| !hidden_proto_ids.contains(&inst.prototype_id))
            .map(|(anim, _)| anim.clone())
            .collect();
        animations.extend(std::iter::repeat_n(None, instancer_count));
        self.instance_animations = animations;

        self.last_evaluated_frame = 0.0;
        self.scene_material = scene
            .prototypes
            .first()
            .and_then(|p| p.material.as_ref())
            .map(|m| (**m).clone())
            .unwrap_or_default();
        self.scene_materials = scene.materials.clone();
        self.scene_cameras = scene.cameras.clone();

        // Reset stale scene camera selection
        if let crate::ivar_state::CameraSource::SceneCamera(idx) = self.viewport_camera_source {
            if idx >= self.scene_cameras.len() {
                self.viewport_camera_source = crate::ivar_state::CameraSource::Viewport;
                self.camera_locked = false;
            }
        }

        // Multi-draw state
        self.multi_draw.prototype_gpu_data = prototype_gpu_data;
        self.multi_draw.enabled = use_multi_draw;
        self.multi_draw.rebuild_instance_groups(
            &self.instance_transforms,
            &self.instance_prototype_ids,
            &self.instance_material_ids,
        );

        // Material uniform
        self.material_uniform = MaterialUniform::from_material(&self.scene_material);
        self.queue.write_buffer(
            &self.material_buffer,
            0,
            bytemuck::cast_slice(&[self.material_uniform]),
        );

        // Invalidate Ivar
        self.ivar_state.world = None;
        self.ivar_state.build_status = BuildStatus::NotStarted;
        self.ivar_state.cancel_flag.store(true, Ordering::Relaxed);
        self.ivar_state.render_complete = false;

        // Rebuild pick scene
        self.rebuild_pick_scene();

        log::info!(
            "Working scene reloaded: {} protos, {} instances, {} tris",
            self.working_scene.prototype_count(),
            self.num_instances,
            self.num_triangles
        );

        Ok(())
    }

    /// Start loading a USD scene asynchronously on a background thread.
    ///
    /// Progress is reported via `UsdLoadMessage` on an `mpsc` channel.
    /// Call `poll_usd_load()` each frame to check for completion
    /// and finalize GPU resources on the main thread.
    pub fn load_usd_scene_async<P: AsRef<std::path::Path>>(&mut self, path: P) {
        use crate::{UsdLoadMessage, UsdLoadProgress, UsdLoadStatus};

        let path = path.as_ref().to_path_buf();
        if !path.exists() {
            self.usd_load_status = UsdLoadStatus::Error(format!("File not found: {path:?}"));
            return;
        }

        let (tx, rx) = std::sync::mpsc::channel();
        self.usd_load_receiver = Some(rx);
        self.usd_load_status = UsdLoadStatus::Loading(UsdLoadProgress::OpeningStage);

        std::thread::spawn(move || {
            tx.send(UsdLoadMessage::Progress(UsdLoadProgress::OpeningStage))
                .ok();

            match bif_core::usd::load_usd_with_stage(&path) {
                Ok((scene, stage)) => {
                    tx.send(UsdLoadMessage::Complete {
                        scene: Box::new(scene),
                        stage,
                        path,
                    })
                    .ok();
                }
                Err(e) => {
                    tx.send(UsdLoadMessage::Failed(format!("Failed to load USD: {e}")))
                        .ok();
                }
            }
        });
    }

    /// Poll for async USD load completion. Call once per frame.
    ///
    /// When the load completes, finalizes GPU resources on the main thread
    /// by delegating to `finalize_usd_load`.
    pub fn poll_usd_load(&mut self) {
        use crate::{UsdLoadMessage, UsdLoadProgress, UsdLoadStatus};

        let Some(ref receiver) = self.usd_load_receiver else {
            return;
        };

        // Drain all available messages (non-blocking)
        loop {
            match receiver.try_recv() {
                Ok(UsdLoadMessage::Progress(progress)) => {
                    self.usd_load_status = UsdLoadStatus::Loading(progress);
                }
                Ok(UsdLoadMessage::Complete { scene, stage, path }) => {
                    self.usd_load_status = UsdLoadStatus::Loading(UsdLoadProgress::Finalizing);
                    if let Err(e) = self.finalize_usd_load(*scene, stage, &path) {
                        self.usd_load_status =
                            UsdLoadStatus::Error(format!("GPU finalize failed: {e}"));
                    } else {
                        self.usd_load_status = UsdLoadStatus::Idle;
                    }
                    self.usd_load_receiver = None;
                    return;
                }
                Ok(UsdLoadMessage::Failed(msg)) => {
                    log::error!("Async USD load failed: {msg}");
                    self.usd_load_status = UsdLoadStatus::Error(msg);
                    self.usd_load_receiver = None;
                    return;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.usd_load_status =
                        UsdLoadStatus::Error("Load thread disconnected".to_string());
                    self.usd_load_receiver = None;
                    return;
                }
            }
        }
    }

    /// Finalize a completed async USD load — create GPU resources on main thread.
    ///
    /// This is equivalent to the GPU-facing portion of `load_usd_scene`, called
    /// after the background thread produces a `Scene` + `UsdStage`.
    fn finalize_usd_load(
        &mut self,
        scene: bif_core::Scene,
        stage: bif_core::usd::UsdStage,
        path: &std::path::Path,
    ) -> Result<()> {
        // Delegate to the synchronous path which already handles everything
        // after the stage/scene are loaded. We stash them and call the existing
        // GPU finalization inline.
        self.finalize_usd_scene(scene, stage, path)
    }

    /// Load a USD scene file and update the viewport (synchronous).
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

        self.finalize_usd_scene(scene, stage, path)
    }

    /// Finalize a loaded USD scene — create GPU resources and update viewport state.
    ///
    /// Called by both `load_usd_scene` (sync) and `finalize_usd_load` (async).
    fn finalize_usd_scene(
        &mut self,
        scene: bif_core::Scene,
        stage: bif_core::usd::UsdStage,
        path: &std::path::Path,
    ) -> Result<()> {
        let viewport_load_start = Instant::now();

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

        let material_index_by_name: HashMap<Arc<str>, u32> = scene
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
        let write_count = instances.len().min(MAX_INSTANCES as usize);
        self.queue.write_buffer(
            &self.instance_buffer,
            0,
            bytemuck::cast_slice(&instances[..write_count]),
        );

        log::info!(
            "Created {} instances from USD scene (wrote {})",
            instances.len(),
            write_count
        );

        self.instance_material_ids = instance_material_ids;
        self.instance_prototype_ids = instance_prototype_ids;
        // Build prim path mapping for USD export
        self.instance_prim_paths = scene
            .instances()
            .iter()
            .enumerate()
            .map(|(idx, inst)| resolve_prim_path(inst, &scene, idx))
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
        self.num_instances = write_count as u32;
        self.culling.visible_count = write_count as u32;
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
        self.loaded_usd_path = Some(path.display().to_string());

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

        // Propagate stage metadata from the first USD scene loaded
        if self.working_scene.stage_metadata.is_none() {
            self.working_scene.stage_metadata = scene.stage_metadata.clone();
        }

        // Merge USD scene into working_scene so primitives added later coexist.
        // Track offsets for remapping IDs from the loaded scene to the working scene.
        let proto_offset = self.working_scene.prototype_count();
        let instance_offset = self.working_scene.instance_count();
        for proto in &scene.prototypes {
            self.working_scene
                .add_prototype(proto.mesh.clone(), proto.name.clone());
        }
        for (inst, anim) in scene.instances_with_animations() {
            let remapped_proto_id = inst.prototype_id + proto_offset;
            if let Some(anim) = anim {
                self.working_scene.add_animated_instance(
                    remapped_proto_id,
                    inst.transform.clone(),
                    anim.clone(),
                );
            } else {
                self.working_scene
                    .add_instance(remapped_proto_id, inst.transform.clone());
            }
        }
        let mat_source_dir = path.parent().map(|p| p.to_path_buf());
        for mat in &scene.materials {
            let mut with_dir = (**mat).clone();
            with_dir.source_dir = mat_source_dir.clone();
            self.working_scene.add_material(with_dir);
        }
        // Merge point clouds and sync next_cloud_id to avoid ID collisions
        for cloud in &scene.point_clouds {
            let mut merged = cloud.clone();
            merged.id = self.next_cloud_id;
            self.next_cloud_id += 1;
            self.working_scene.add_point_cloud(merged);
        }
        // Remap camera instance indices by the instance offset
        for cam in &scene.cameras {
            let mut remapped = cam.clone();
            remapped.instance_index += instance_offset;
            self.working_scene.cameras.push(remapped);
        }

        // Update lights from scene
        self.update_lights(&scene.lights);

        // Build pick scene for viewport selection
        self.rebuild_pick_scene();

        // Rebuild GPU state from accumulated working_scene so multi-USD materials resolve
        if let Err(e) = self.reload_working_scene() {
            log::error!("Failed to reload working scene after USD merge: {}", e);
        }

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
