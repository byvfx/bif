use anyhow::Result;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use wgpu::util::DeviceExt;

use bif_core::usd::layer::PayloadPolicy;
use bif_math::{Aabb, Mat4, Mat4Ext, Vec3};

use crate::gpu_types::{InstanceData, MaterialGpu, MaterialUniform, PrototypeGpuData};
use crate::mesh_data::MeshData;
use crate::timeline::TimelineState;
use crate::{texture_loader, Renderer, MAX_INSTANCES};

use crate::scene_pipeline::resolve_prim_path;

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
            "Material: {} (metalness={:.2}, roughness={:.2})",
            scene_material.name,
            scene_material.base_metalness,
            scene_material.specular_roughness
        );

        log::info!(
            "Loaded {} vertices, {} indices from scene",
            mesh_data.vertices.len(),
            mesh_data.indices.len()
        );

        // Refresh textures (async — placeholders now, real textures stream in)
        let (gpu_textures, tile_paths) = texture_loader::prepare_texture_placeholders(
            &self.gpu.device,
            &self.gpu.queue,
            scene,
            None,
        );
        self.textures.gpu_textures = gpu_textures;
        self.async_channels.texture_load_receiver =
            Some(texture_loader::start_texture_loading_async(tile_paths));

        let mut material_table: Vec<MaterialGpu> = scene
            .materials
            .iter()
            .map(|mat| MaterialGpu::from_material(mat.as_ref(), &self.textures.gpu_textures))
            .collect();
        // Always append default grey as last entry — fallback for prototypes without materials
        let default_mat_index = material_table.len() as u32;
        material_table.push(MaterialGpu::from_material(
            &bif_core::Material::default(),
            &self.textures.gpu_textures,
        ));
        self.materials.table_len = material_table.len() as u32;
        self.materials.table_buffer =
            self.gpu
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Material Table Buffer"),
                    contents: bytemuck::cast_slice(&material_table),
                    usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                });

        // Triangle material buffer — cap to GPU limit
        if let Some(ref tri_mats) = mesh_data.triangle_material_ids {
            let max_buf = self.gpu.device.limits().max_storage_buffer_binding_size as usize;
            let buf_bytes = tri_mats.len() * std::mem::size_of::<u32>();
            let data = if buf_bytes > max_buf {
                let max_entries = max_buf / std::mem::size_of::<u32>();
                log::warn!(
                    "Triangle material buffer {} MB exceeds GPU limit {} MB — truncating",
                    buf_bytes / (1024 * 1024),
                    max_buf / (1024 * 1024)
                );
                &tri_mats[..max_entries]
            } else {
                tri_mats.as_slice()
            };
            self.materials.triangle_buffer =
                self.gpu
                    .device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Triangle Material Buffer"),
                        contents: bytemuck::cast_slice(data),
                        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                    });
            self.materials.has_triangle_materials = true;
        } else {
            self.materials.triangle_buffer =
                self.gpu
                    .device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Triangle Material Buffer"),
                        contents: bytemuck::cast_slice(&[0xFFFFFFFFu32]),
                        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                    });
            self.materials.has_triangle_materials = false;
        }

        // Rebuild material bind group
        self.materials.bind_group = self
            .gpu
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Material Bind Group"),
                layout: &self.materials.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.materials.buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: self.materials.table_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: self.materials.triangle_buffer.as_entire_binding(),
                    },
                ],
            });

        // Rebuild texture bind group
        let texture_view_refs: Vec<&wgpu::TextureView> =
            self.textures.gpu_textures.views.iter().collect();
        self.textures.bind_group = self
            .gpu
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Texture Bind Group"),
                layout: &self.textures.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureViewArray(&texture_view_refs),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.textures.sampler),
                    },
                ],
            });

        // Vertex and index buffers
        self.vertex_buffer =
            self.gpu
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Vertex Buffer"),
                    contents: bytemuck::cast_slice(&mesh_data.vertices),
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                });
        self.index_buffer = self
            .gpu
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
        let mut instance_purposes = Vec::with_capacity(scene.instance_count());
        let instances: Vec<InstanceData> = if scene.instances().is_empty() {
            scene
                .prototypes
                .iter()
                .enumerate()
                .map(|(proto_id, proto)| {
                    let model_matrix = Mat4::IDENTITY;
                    instance_transforms.push(model_matrix);
                    instance_prototype_ids.push(proto_id);
                    instance_purposes.push(bif_core::Purpose::Default);
                    let material_id = proto
                        .material
                        .as_ref()
                        .and_then(|mat| material_index_by_name.get(&mat.name).copied())
                        .unwrap_or(default_mat_index);
                    instance_material_ids.push(material_id);
                    InstanceData {
                        model_matrix: model_matrix.to_cols_array_2d(),
                        material_id,
                        tri_mat_offset: 0,
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
                    instance_purposes.push(inst.purpose);
                    let material_id = scene
                        .prototypes
                        .get(inst.prototype_id)
                        .and_then(|proto| proto.material.as_ref())
                        .and_then(|mat| material_index_by_name.get(&mat.name).copied())
                        .unwrap_or(default_mat_index);
                    instance_material_ids.push(material_id);
                    InstanceData {
                        model_matrix: model_matrix.to_cols_array_2d(),
                        material_id,
                        tri_mat_offset: 0,
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
        self.gpu.queue.write_buffer(
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
            .set_prototype_aabb(&self.gpu.device, prototype_aabb, triangles_per_instance);
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
        self.scene.mesh_data = mesh_data;
        self.scene.instances.current = instance_transforms.clone();
        self.scene.instances.transforms = instance_transforms;
        self.scene.instances.material_ids = instance_material_ids;
        self.scene.instances.prototype_ids = instance_prototype_ids;
        self.scene.instances.purposes = instance_purposes;
        // Build prim path mapping for USD export
        self.scene.instances.prim_paths = scene
            .instances()
            .iter()
            .enumerate()
            .map(|(idx, inst)| resolve_prim_path(inst, scene, idx))
            .collect();
        self.scene.instance_animations = scene.instance_animations().to_vec();
        self.scene.last_evaluated_frame = 0.0;
        self.scene.scene_material = scene_material.clone();
        self.scene.scene_materials = scene.materials.clone();
        self.scene.scene_cameras = scene.cameras.clone();
        // Reset stale scene camera selection
        if let crate::ivar_state::CameraSource::SceneCamera(idx) = self.cam.viewport_camera_source {
            if idx >= self.scene.scene_cameras.len() {
                self.cam.viewport_camera_source = crate::ivar_state::CameraSource::Viewport;
                self.cam.camera_locked = false;
            }
        }

        // Update material uniform
        self.materials.uniform = MaterialUniform::from_material(&scene_material);
        self.gpu.queue.write_buffer(
            &self.materials.buffer,
            0,
            bytemuck::cast_slice(&[self.materials.uniform]),
        );

        // Frame camera
        self.cam.camera.target = mesh_center;
        self.cam.camera.distance = camera_distance;
        self.cam.camera.near = camera_distance * 0.01;
        self.cam.camera.far = camera_distance * 20.0;
        self.cam.camera.update_position_from_angles();
        self.update_camera();

        // Invalidate Ivar scene + materials (new scene = new textures)
        self.invalidate_ivar_materials();
        self.ivar.ivar_texture_cache = None;
        self.ivar.ivar_state.invalidate_scene();

        // Update lights
        self.update_lights(&scene.lights);

        // Auto-connect first DomeLight with texture to HdriEnvironment
        // (skip if user already loaded an HDRI via the node graph)
        if !self.environment.hdri_loaded {
            if let Some(bif_core::Light::Dome {
                rotation,
                intensity,
                texture_path: Some(path),
            }) = scene.lights.iter().find(|l| {
                matches!(
                    l,
                    bif_core::Light::Dome {
                        texture_path: Some(_),
                        ..
                    }
                )
            }) {
                log::info!("Auto-loading DomeLight HDRI: {}", path);
                self.environment.start_hdri_load(
                    std::path::Path::new(path.as_ref()),
                    rotation.to_degrees(),
                    *intensity,
                    true,
                );
            }
        }

        log::info!(
            "Scene data loaded: {} triangles x {} instances",
            self.num_indices / 3,
            self.num_instances
        );

        // Build pick scene for viewport selection
        self.rebuild_pick_scene();

        // Ivar materials built on-demand when render starts (prewarm disabled to save RAM)

        Ok(())
    }

    /// Generate a unique name for a primitive (e.g. "Cube", "Cube_2", "Cube_3").
    fn unique_primitive_name(&mut self, base: &str) -> String {
        let counter = self
            .nodes
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
        if !self.scene.working_scene.remove_prototype(proto_id) {
            log::error!("Prototype {} not found for removal", proto_id);
            return false;
        }
        // Re-index node_proto_map (unified: single + multi-proto nodes)
        for ids in self.nodes.node_proto_map.values_mut() {
            ids.retain(|id| *id != proto_id);
            for id in ids.iter_mut() {
                if *id > proto_id {
                    *id -= 1;
                }
            }
        }
        // Remove instancer results referencing deleted prototype
        self.nodes.instancer_results.retain(|_node_id, instances| {
            !instances.iter().any(|inst| inst.prototype_id == proto_id)
        });
        // Re-index remaining instancer_results prototype IDs
        for instances in self.nodes.instancer_results.values_mut() {
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
            .scene
            .working_scene
            .add_prototype(Arc::new(mesh), unique_name.clone());
        self.scene
            .working_scene
            .add_instance(proto_id, bif_core::Transform::default());

        // Register camera primitive as a scene camera
        if kind == bif_core::PrimitiveKind::Camera {
            let instance_index = self.scene.working_scene.instance_count() - 1;
            self.scene
                .working_scene
                .cameras
                .push(bif_core::SceneCamera {
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
            .scene
            .working_scene
            .add_prototype(Arc::new(mesh), name.to_string());
        self.scene
            .working_scene
            .add_instance(proto_id, bif_core::Transform::default());

        if kind == bif_core::PrimitiveKind::Camera {
            let instance_index = self.scene.working_scene.instance_count() - 1;
            self.scene
                .working_scene
                .cameras
                .push(bif_core::SceneCamera {
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
        if !self.scene.working_scene.remove_prototype(proto_id) {
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
        self.nodes.scene_graph_dirty = true;
        let scene = &self.scene.working_scene;

        if scene.prototypes.is_empty() {
            // Empty scene — reset to blank
            self.num_indices = 0;
            self.num_instances = 0;
            self.num_triangles = 0;
            self.multi_draw.enabled = false;
            self.multi_draw.prototype_gpu_data.clear();
            self.multi_draw.instance_groups.clear();
            self.scene.instances.transforms.clear();
            self.scene.instances.current.clear();
            self.scene.instances.material_ids.clear();
            self.scene.instances.prototype_ids.clear();
            self.scene.instances.prim_paths.clear();
            self.scene.instances.purposes.clear();
            self.scene.instance_animations = scene.instance_animations().to_vec();
            self.scene.scene_cameras = scene.cameras.clone();
            self.culling.instance_aabbs.clear();
            self.culling.visible_count = 0;
            self.culling.mark_dirty();
            self.pick_scene = None;
            self.scene.mesh_data = MeshData::default();
            // Invalidate Ivar
            self.ivar.ivar_state.invalidate_scene();
            return Ok(());
        }

        let use_multi_draw = scene.prototypes.len() > 1;

        // Compute active node set from display flag (None = everything active)
        let active_nodes: Option<std::collections::HashSet<crate::node_graph::GraphNodeId>> =
            self.nodes.node_graph_state.display_node.map(|dn| {
                let snarl_dn: egui_snarl::NodeId = dn.into();
                crate::node_graph::collect_upstream_nodes(
                    snarl_dn,
                    &self.nodes.node_graph_state.snarl,
                )
                .into_iter()
                .map(crate::node_graph::GraphNodeId::from)
                .collect()
            });
        let is_node_active = |node_id: &crate::node_graph::GraphNodeId| {
            active_nodes.as_ref().is_none_or(|s| s.contains(node_id))
        };

        // Prototypes consumed by instancers or scatter surfaces — hide from viewport
        let instanced_proto_ids: std::collections::HashSet<usize> = self
            .nodes
            .instancer_results
            .iter()
            .filter(|(nid, _)| is_node_active(nid))
            .flat_map(|(_, insts)| insts.iter().map(|inst| inst.prototype_id))
            .collect();
        let scatter_surface_ids: std::collections::HashSet<usize> = self
            .nodes
            .node_scatter_surface_map
            .iter()
            .filter(|(nid, _)| is_node_active(nid))
            .map(|(_, &pid)| pid)
            .collect();
        // Also hide prototypes from inactive nodes (display flag gating)
        let display_hidden_proto_ids: std::collections::HashSet<usize> = self
            .nodes
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
        let is_instance_visible = |inst: &bif_core::Instance| {
            inst.prim_path.is_empty()
                || !self
                    .scene
                    .hidden_prim_paths
                    .contains(inst.prim_path.as_ref())
        };
        if !hidden_proto_ids.is_empty() {
            log::debug!(
                "Hiding prototypes: {:?} (instanced: {:?}, scatter surface: {:?})",
                hidden_proto_ids,
                instanced_proto_ids,
                scatter_surface_ids,
            );
        }

        // Create per-prototype GPU data and build compact triangle material buffer.
        // The compact buffer contains one copy of each prototype's per-triangle material
        // IDs (not duplicated per instance), keeping the GPU buffer small.
        let mut compact_tri_mats: Vec<u32> = Vec::new();
        let mut prototype_gpu_data: Vec<PrototypeGpuData> = Vec::new();
        // Synthetic flat-color materials for prims with primvars:displayColor but no binding.
        // Appended after scene.materials + default slot in the GPU material table.
        // +1 for the implicit default grey entry that sits between real and synthetic.
        let synthetic_mat_start = scene.materials.len() as u32 + 1;
        let mut synthetic_materials: Vec<bif_core::Material> = Vec::new();

        for (proto_id, proto) in scene.prototypes.iter().enumerate() {
            let md = MeshData::from_core_mesh(&proto.mesh);
            let num_triangles = md.indices.len() as u32 / 3;

            let vertex_buffer =
                self.gpu
                    .device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some(&format!("WS Proto {} VB", proto_id)),
                        contents: bytemuck::cast_slice(&md.vertices),
                        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    });

            let index_buffer =
                self.gpu
                    .device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some(&format!("WS Proto {} IB", proto_id)),
                        contents: bytemuck::cast_slice(&md.indices),
                        usage: wgpu::BufferUsages::INDEX,
                    });

            let triangle_material_buffer = md.triangle_material_ids.as_ref().map(|tri_mats| {
                self.gpu
                    .device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some(&format!("WS Proto {} TMB", proto_id)),
                        contents: bytemuck::cast_slice(tri_mats),
                        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                    })
            });

            // Track offset into compact buffer before appending this prototype
            let tri_mat_offset = compact_tri_mats.len() as u32;
            if let Some(ref tri_mats) = md.triangle_material_ids {
                compact_tri_mats.extend_from_slice(tri_mats);
            } else if let Some(dc) = md.display_color {
                // Synthesize a flat-color material from primvars:displayColor
                let mat_idx = synthetic_mat_start + synthetic_materials.len() as u32;
                synthetic_materials.push(bif_core::Material {
                    name: std::sync::Arc::from(format!("__display_color_{}", proto_id)),
                    base_color: bif_math::Vec3::new(dc[0], dc[1], dc[2]),
                    specular_roughness: 0.8,
                    base_metalness: 0.0,
                    ..bif_core::Material::default()
                });
                compact_tri_mats.extend(std::iter::repeat_n(mat_idx, num_triangles as usize));
            } else {
                // No per-face materials: fill with sentinel (0xFFFFFFFF)
                compact_tri_mats.extend(std::iter::repeat_n(0xFFFFFFFFu32, num_triangles as usize));
            }

            prototype_gpu_data.push(PrototypeGpuData {
                vertex_buffer,
                index_buffer,
                num_indices: md.indices.len() as u32,
                num_vertices: md.vertices.len() as u32,
                prototype_id: proto_id,
                mesh_idx: proto_id,
                triangle_material_buffer,
                tri_mat_offset,
                num_triangles,
                vertices: md.vertices,
            });
        }

        // Build material index lookup early so combine_with_transforms can use it
        let material_index_by_name: HashMap<Arc<str>, u32> = scene
            .materials
            .iter()
            .enumerate()
            .map(|(idx, mat)| (mat.name.clone(), idx as u32))
            .collect();
        let default_mat_index = scene.materials.len() as u32;

        // Combined mesh_data for single-draw fallback and Ivar.
        // Single prototype: raw mesh (Ivar applies per-instance transforms).
        // Multi prototype: bake scene + instancer transforms into vertices
        // so Ivar can use a single identity transform.
        let mesh_data = if scene.prototypes.len() == 1 {
            let mut md = MeshData::from_core_mesh(&scene.prototypes[0].mesh);
            // from_core_mesh sets triangle_material_ids=None when no GeomSubsets.
            // Fill with prototype's material index so Ivar uses correct material.
            if md.triangle_material_ids.is_none() {
                let mat_id = scene.prototypes[0]
                    .material
                    .as_ref()
                    .and_then(|mat| material_index_by_name.get(&mat.name).copied())
                    .unwrap_or(default_mat_index);
                let tri_count = md.indices.len() / 3;
                md.triangle_material_ids = Some(vec![mat_id; tri_count]);
            }
            md
        } else if !scene.instances().is_empty() {
            let active_purpose = self.display_settings.purpose_mode;
            let mut meshes_with_transforms: Vec<(&bif_core::Mesh, Mat4, usize, u32)> = scene
                .instances()
                .iter()
                .enumerate()
                .filter(|(_idx, inst)| !hidden_proto_ids.contains(&inst.prototype_id))
                .filter(|(_idx, inst)| is_instance_visible(inst))
                .filter(|(_idx, inst)| active_purpose.includes(inst.purpose))
                .filter_map(|(mesh_idx, inst)| {
                    scene.prototypes.get(inst.prototype_id).map(|proto| {
                        let mat_id = proto
                            .material
                            .as_ref()
                            .and_then(|mat| material_index_by_name.get(&mat.name).copied())
                            .unwrap_or(default_mat_index);
                        (proto.mesh.as_ref(), inst.model_matrix(), mesh_idx, mat_id)
                    })
                })
                .collect();
            // Bake active instancer instances so Ivar (identity transform) can render them
            let base_idx = meshes_with_transforms.len();
            for (i, inst) in self
                .nodes
                .instancer_results
                .iter()
                .filter(|(nid, _)| is_node_active(nid))
                .flat_map(|(_, insts)| insts.iter())
                .filter(|inst| active_purpose.includes(inst.purpose))
                .enumerate()
            {
                if let Some(proto) = scene.prototypes.get(inst.prototype_id) {
                    let mat_id = proto
                        .material
                        .as_ref()
                        .and_then(|mat| material_index_by_name.get(&mat.name).copied())
                        .unwrap_or(default_mat_index);
                    meshes_with_transforms.push((
                        proto.mesh.as_ref(),
                        inst.model_matrix(),
                        base_idx + i,
                        mat_id,
                    ));
                }
            }
            MeshData::combine_with_transforms(&meshes_with_transforms)
        } else {
            let meshes_with_transforms: Vec<(&bif_core::Mesh, Mat4, usize, u32)> = scene
                .prototypes
                .iter()
                .enumerate()
                .map(|(idx, proto)| {
                    let mat_id = proto
                        .material
                        .as_ref()
                        .and_then(|mat| material_index_by_name.get(&mat.name).copied())
                        .unwrap_or(default_mat_index);
                    (proto.mesh.as_ref(), Mat4::IDENTITY, idx, mat_id)
                })
                .collect();
            MeshData::combine_with_transforms(&meshes_with_transforms)
        };

        // Only rebuild GPU textures when materials actually changed (not on every
        // Xform drag or display toggle). Uses async loading — placeholders appear
        // instantly, real textures stream in via poll_texture_loads().
        if self.nodes.materials_dirty {
            // Cancel any in-flight texture loading from a previous rebuild
            self.async_channels.texture_load_receiver = None;

            let (gpu_textures, tile_paths) = texture_loader::prepare_texture_placeholders(
                &self.gpu.device,
                &self.gpu.queue,
                scene,
                self.scene.texture_base_dir.as_deref(),
            );
            self.textures.gpu_textures = gpu_textures;

            let texture_view_refs: Vec<&wgpu::TextureView> =
                self.textures.gpu_textures.views.iter().collect();
            self.textures.bind_group =
                self.gpu
                    .device
                    .create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("WS Texture Bind Group"),
                        layout: &self.textures.bind_group_layout,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::TextureViewArray(
                                    &texture_view_refs,
                                ),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::Sampler(&self.textures.sampler),
                            },
                        ],
                    });

            self.async_channels.texture_load_receiver =
                Some(texture_loader::start_texture_loading_async(tile_paths));

            self.nodes.materials_dirty = false;
        }

        // Material table (use default for primitives)
        let mut material_table: Vec<crate::gpu_types::MaterialGpu> = scene
            .materials
            .iter()
            .map(|mat| {
                crate::gpu_types::MaterialGpu::from_material(
                    mat.as_ref(),
                    &self.textures.gpu_textures,
                )
            })
            .collect();
        let default_mat_index = material_table.len() as u32;
        material_table.push(crate::gpu_types::MaterialGpu::from_material(
            &bif_core::Material::default(),
            &self.textures.gpu_textures,
        ));
        // Append synthetic display_color materials (indices synthetic_mat_start..)
        for syn_mat in &synthetic_materials {
            material_table.push(crate::gpu_types::MaterialGpu::from_material(
                syn_mat,
                &self.textures.gpu_textures,
            ));
        }
        self.materials.table_len = material_table.len() as u32;
        self.materials.table_buffer =
            self.gpu
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("WS Material Table Buffer"),
                    contents: bytemuck::cast_slice(&material_table),
                    usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                });

        // Triangle material buffer — use compact buffer (one copy per prototype).
        // Must always be full-sized: shader indexes by primitive_id + tri_mat_offset.
        self.materials.has_triangle_materials =
            compact_tri_mats.iter().any(|&id| id != 0xFFFFFFFFu32);
        if compact_tri_mats.is_empty() {
            compact_tri_mats.push(0xFFFFFFFFu32);
        }
        // Cap to GPU max_storage_buffer_binding_size (typically 128MB)
        let max_buf = self.gpu.device.limits().max_storage_buffer_binding_size as usize;
        let buf_bytes = compact_tri_mats.len() * std::mem::size_of::<u32>();
        if buf_bytes > max_buf {
            let max_entries = max_buf / std::mem::size_of::<u32>();
            log::warn!(
                "Triangle material buffer {} MB exceeds GPU limit {} MB — truncating ({} / {} entries)",
                buf_bytes / (1024 * 1024),
                max_buf / (1024 * 1024),
                max_entries,
                compact_tri_mats.len()
            );
            compact_tri_mats.truncate(max_entries);
        }
        self.materials.triangle_buffer =
            self.gpu
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("WS Triangle Material Buffer (compact)"),
                    contents: bytemuck::cast_slice(&compact_tri_mats),
                    usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                });

        // Rebuild material bind group
        self.materials.bind_group = self
            .gpu
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("WS Material Bind Group"),
                layout: &self.materials.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.materials.buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: self.materials.table_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: self.materials.triangle_buffer.as_entire_binding(),
                    },
                ],
            });

        // Vertex and index buffers (combined, for single-draw fallback + Ivar)
        // Guard: skip combined mesh if it exceeds GPU max buffer size
        let max_buf_size = self.gpu.device.limits().max_buffer_size as usize;
        let vb_size = std::mem::size_of_val(mesh_data.vertices.as_slice());
        let ib_size = std::mem::size_of_val(mesh_data.indices.as_slice());
        if vb_size > max_buf_size || ib_size > max_buf_size {
            log::warn!(
                "Combined mesh too large for GPU (verts={} MB, idx={} MB, limit={} MB) — skipping combined buffer, multi-draw only",
                vb_size / (1024 * 1024),
                ib_size / (1024 * 1024),
                max_buf_size / (1024 * 1024),
            );
            // Create minimal placeholder buffers
            let placeholder_vert = crate::gpu_types::Vertex {
                position: [0.0; 3],
                normal: [0.0; 3],
                color: [0.0; 3],
                uv: [0.0; 2],
                material_id: 0,
            };
            self.vertex_buffer =
                self.gpu
                    .device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("WS Vertex Buffer (placeholder)"),
                        contents: bytemuck::bytes_of(&placeholder_vert),
                        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    });
            self.index_buffer =
                self.gpu
                    .device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("WS Index Buffer (placeholder)"),
                        contents: bytemuck::cast_slice(&[0u32]),
                        usage: wgpu::BufferUsages::INDEX,
                    });
            self.scene.mesh_data.vertices.clear();
            self.scene.mesh_data.indices.clear();
        } else {
            self.vertex_buffer =
                self.gpu
                    .device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("WS Vertex Buffer"),
                        contents: bytemuck::cast_slice(&mesh_data.vertices),
                        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    });
            self.index_buffer =
                self.gpu
                    .device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("WS Index Buffer"),
                        contents: bytemuck::cast_slice(&mesh_data.indices),
                        usage: wgpu::BufferUsages::INDEX,
                    });
        }

        // Generate instances (pre-allocate for scene + instancer instances)
        let instancer_count: usize = self
            .nodes
            .instancer_results
            .iter()
            .filter(|(nid, _)| is_node_active(nid))
            .map(|(_, v)| v.len())
            .sum();
        let total_capacity = scene.instance_count() + instancer_count;
        let mut instance_transforms = Vec::with_capacity(total_capacity);
        let mut instance_material_ids = Vec::with_capacity(total_capacity);
        let mut instance_prototype_ids = Vec::with_capacity(total_capacity);
        let mut instance_purposes = Vec::with_capacity(total_capacity);
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
                    instance_purposes.push(bif_core::Purpose::Default);
                    let material_id = proto
                        .material
                        .as_ref()
                        .and_then(|mat| material_index_by_name.get(&mat.name).copied())
                        .unwrap_or(default_mat_index);
                    instance_material_ids.push(material_id);
                    InstanceData {
                        model_matrix: model_matrix.to_cols_array_2d(),
                        material_id,
                        tri_mat_offset: 0,
                    }
                })
                .collect()
        } else {
            scene
                .instances()
                .iter()
                .filter(|inst| !hidden_proto_ids.contains(&inst.prototype_id))
                .filter(|inst| is_instance_visible(inst))
                .map(|inst| {
                    let model_matrix = inst.model_matrix();
                    instance_transforms.push(model_matrix);
                    instance_prototype_ids.push(inst.prototype_id);
                    instance_purposes.push(inst.purpose);
                    let material_id = scene
                        .prototypes
                        .get(inst.prototype_id)
                        .and_then(|proto| proto.material.as_ref())
                        .and_then(|mat| material_index_by_name.get(&mat.name).copied())
                        .unwrap_or(default_mat_index);
                    instance_material_ids.push(material_id);
                    InstanceData {
                        model_matrix: model_matrix.to_cols_array_2d(),
                        material_id,
                        tri_mat_offset: 0,
                    }
                })
                .collect()
        };

        // Append instancer-expanded instances (from active Point Instancer nodes)
        for inst in self
            .nodes
            .instancer_results
            .iter()
            .filter(|(nid, _)| is_node_active(nid))
            .flat_map(|(_, insts)| insts.iter())
        {
            let model_matrix = inst.model_matrix();
            instance_transforms.push(model_matrix);
            instance_prototype_ids.push(inst.prototype_id);
            instance_purposes.push(inst.purpose);
            let material_id = scene
                .prototypes
                .get(inst.prototype_id)
                .and_then(|p| p.material.as_ref())
                .and_then(|mat| material_index_by_name.get(&mat.name).copied())
                .unwrap_or(default_mat_index);
            instance_material_ids.push(material_id);
            instances.push(InstanceData {
                model_matrix: model_matrix.to_cols_array_2d(),
                material_id,
                tri_mat_offset: 0,
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
            use egui_snarl::InPinId;

            // Collect active Xform nodes with their upstream depth for topological order.
            // Depth = max number of Xform nodes upstream (0 = no Xform parent).
            // Sorting by depth ensures upstream Xforms apply before downstream ones.
            struct XformEntry {
                node_id: egui_snarl::NodeId,
                translate: [f32; 3],
                rotate: [f32; 3],
                scale: [f32; 3],
                depth: usize,
            }
            let mut xform_entries: Vec<XformEntry> = Vec::new();
            for (nid, node) in self.nodes.node_graph_state.snarl.node_ids() {
                let graph_nid = crate::node_graph::GraphNodeId::from(nid);
                if !is_node_active(&graph_nid) {
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
                        &self.nodes.node_graph_state.snarl,
                    );
                    let depth = upstream
                        .iter()
                        .filter(|&&uid| uid != nid)
                        .filter(|uid| {
                            matches!(
                                self.nodes.node_graph_state.snarl[**uid],
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
                    let in_pin = self.nodes.node_graph_state.snarl.in_pin(InPinId {
                        node: entry.node_id,
                        input: 0,
                    });
                    if let Some(remote) = in_pin.remotes.first() {
                        crate::node_graph::collect_upstream_nodes(
                            remote.node,
                            &self.nodes.node_graph_state.snarl,
                        )
                    } else {
                        continue; // No input connected
                    }
                };

                // Resolve which prototype IDs come from those upstream nodes
                let affected_proto_ids: std::collections::HashSet<usize> = self
                    .nodes
                    .node_proto_map
                    .iter()
                    .filter(|(nid, _)| {
                        let snarl_nid: egui_snarl::NodeId = (**nid).into();
                        upstream.contains(&snarl_nid)
                    })
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
        self.gpu.queue.write_buffer(
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
            .set_prototype_aabb(&self.gpu.device, prototype_aabb, triangles_per_instance);
        self.culling.instance_aabbs = instance_aabbs;
        self.culling.visible_count = write_count as u32;

        // Update renderer state
        self.num_indices = mesh_data.indices.len() as u32;
        self.num_instances = write_count as u32;
        self.mesh_bounds_min = mesh_data.bounds_min;
        self.mesh_bounds_max = mesh_data.bounds_max;
        self.num_triangles = triangles_per_instance as u64 * write_count as u64;
        self.scene.mesh_data = mesh_data;
        self.scene.instances.current = instance_transforms.clone();
        self.scene.instances.transforms = instance_transforms;
        self.scene.instances.material_ids = instance_material_ids;
        self.scene.instances.prototype_ids = instance_prototype_ids;
        self.scene.instances.purposes = instance_purposes;

        // Build prim paths for scene instances (skip instanced prototypes)
        let mut prim_paths: Vec<String> = scene
            .instances()
            .iter()
            .enumerate()
            .filter(|(_idx, inst)| !hidden_proto_ids.contains(&inst.prototype_id))
            .filter(|(_idx, inst)| is_instance_visible(inst))
            .map(|(idx, inst)| resolve_prim_path(inst, scene, idx))
            .collect();
        // Extend for instancer-expanded instances (parallel to instance_transforms)
        let scene_inst_count = prim_paths.len();
        for (i, inst) in self
            .nodes
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
        self.scene.instances.prim_paths = prim_paths;

        // Animations: filtered scene instances + None entries for instancer instances
        let mut animations: Vec<_> = scene
            .instance_animations()
            .iter()
            .zip(scene.instances().iter())
            .filter(|(_, inst)| !hidden_proto_ids.contains(&inst.prototype_id))
            .filter(|(_, inst)| is_instance_visible(inst))
            .map(|(anim, _)| anim.clone())
            .collect();
        animations.extend(std::iter::repeat_n(None, instancer_count));
        self.scene.instance_animations = animations;

        self.scene.last_evaluated_frame = 0.0;
        self.scene.scene_material = scene
            .prototypes
            .first()
            .and_then(|p| p.material.as_ref())
            .map(|m| (**m).clone())
            .unwrap_or_default();
        self.scene.scene_materials = scene.materials.clone();
        self.scene.scene_cameras = scene.cameras.clone();

        // Reset stale scene camera selection
        if let crate::ivar_state::CameraSource::SceneCamera(idx) = self.cam.viewport_camera_source {
            if idx >= self.scene.scene_cameras.len() {
                self.cam.viewport_camera_source = crate::ivar_state::CameraSource::Viewport;
                self.cam.camera_locked = false;
            }
        }

        // Multi-draw state
        self.multi_draw.prototype_gpu_data = prototype_gpu_data;
        self.multi_draw.enabled = use_multi_draw;
        self.multi_draw.rebuild_instance_groups(
            &self.scene.instances.transforms,
            &self.scene.instances.prototype_ids,
            &self.scene.instances.material_ids,
            &self.scene.instances.purposes,
            self.display_settings.purpose_mode,
        );

        // Material uniform
        self.materials.uniform = MaterialUniform::from_material(&self.scene.scene_material);
        self.gpu.queue.write_buffer(
            &self.materials.buffer,
            0,
            bytemuck::cast_slice(&[self.materials.uniform]),
        );

        // Invalidate Ivar
        self.ivar.ivar_state.invalidate_scene();

        // Invalidate material cache when materials changed (built on-demand at Ivar render)
        if self.nodes.materials_dirty {
            self.invalidate_ivar_materials();
        }

        // Rebuild pick scene
        self.rebuild_pick_scene();

        log::info!(
            "Working scene reloaded: {} protos, {} instances, {} tris",
            self.scene.working_scene.prototype_count(),
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
            self.async_channels.usd_load_status =
                UsdLoadStatus::Error(format!("File not found: {path:?}"));
            return;
        }

        let (tx, rx) = std::sync::mpsc::channel();
        self.async_channels.usd_load_receiver = Some(rx);
        self.async_channels.usd_load_status = UsdLoadStatus::Loading(UsdLoadProgress::OpeningStage);

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

        let Some(ref receiver) = self.async_channels.usd_load_receiver else {
            return;
        };

        // Drain all available messages (non-blocking)
        loop {
            match receiver.try_recv() {
                Ok(UsdLoadMessage::Progress(progress)) => {
                    self.async_channels.usd_load_status = UsdLoadStatus::Loading(progress);
                }
                Ok(UsdLoadMessage::Complete { scene, stage, path }) => {
                    self.async_channels.usd_load_status =
                        UsdLoadStatus::Loading(UsdLoadProgress::Finalizing);
                    if let Err(e) = self.finalize_usd_load(*scene, stage, &path) {
                        self.async_channels.usd_load_status =
                            UsdLoadStatus::Error(format!("GPU finalize failed: {e}"));
                    } else {
                        self.async_channels.usd_load_status = UsdLoadStatus::Idle;
                    }
                    self.async_channels.usd_load_receiver = None;
                    return;
                }
                Ok(UsdLoadMessage::Failed(msg)) => {
                    log::error!("Async USD load failed: {msg}");
                    self.async_channels.usd_load_status = UsdLoadStatus::Error(msg);
                    self.async_channels.usd_load_receiver = None;
                    return;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.async_channels.usd_load_status =
                        UsdLoadStatus::Error("Load thread disconnected".to_string());
                    self.async_channels.usd_load_receiver = None;
                    return;
                }
            }
        }
    }

    /// Poll for async texture load completion. Call once per frame.
    ///
    /// Uploads completed textures from the background thread and rebuilds
    /// the texture bind group when new textures arrive.
    pub fn poll_texture_loads(&mut self) {
        let Some(ref receiver) = self.async_channels.texture_load_receiver else {
            return;
        };

        let max_dimension = self.gpu.device.limits().max_texture_dimension_2d;
        let mut uploaded = 0u32;

        // Pace uploads to avoid frame spikes (max 32 per frame)
        const MAX_UPLOADS_PER_FRAME: u32 = 32;
        loop {
            if uploaded >= MAX_UPLOADS_PER_FRAME {
                break;
            }
            match receiver.try_recv() {
                Ok(msg) => {
                    if texture_loader::upload_streamed_texture(
                        &self.gpu.device,
                        &self.gpu.queue,
                        &mut self.textures.gpu_textures,
                        msg,
                        max_dimension,
                        Some(&self.mipmap_generator),
                    ) {
                        uploaded += 1;
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    // Background thread done — no more textures coming
                    self.async_channels.texture_load_receiver = None;
                    break;
                }
            }
        }

        // Rebuild bind group if any textures were uploaded this frame
        if uploaded > 0 {
            let texture_view_refs: Vec<&wgpu::TextureView> =
                self.textures.gpu_textures.views.iter().collect();
            self.textures.bind_group =
                self.gpu
                    .device
                    .create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("Texture Bind Group (streamed)"),
                        layout: &self.textures.bind_group_layout,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::TextureViewArray(
                                    &texture_view_refs,
                                ),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::Sampler(&self.textures.sampler),
                            },
                        ],
                    });
            log::info!("Streamed {} textures to GPU", uploaded);
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
        self.finalize_usd_scene(scene, stage, path, PayloadPolicy::LoadAll)
    }

    /// Load a USD scene file and update the viewport (synchronous).
    ///
    /// This method reloads the viewport with a new USD file:
    /// 1. Loads the USD file via the C++ bridge
    /// 2. Converts geometry to GPU-ready buffers
    /// 3. Updates the scene browser with the new hierarchy
    /// 4. Invalidates the Ivar cache for re-rendering
    pub fn load_usd_scene<P: AsRef<std::path::Path>>(&mut self, path: P) -> Result<()> {
        let payload_policy = self
            .scene
            .layer_state
            .as_ref()
            .map(|s| s.payload_policy)
            .unwrap_or(PayloadPolicy::LoadAll);
        self.load_usd_scene_with_policy(path, payload_policy)
    }

    /// Load a USD scene file with an explicit payload policy.
    pub fn load_usd_scene_with_policy<P: AsRef<std::path::Path>>(
        &mut self,
        path: P,
        payload_policy: PayloadPolicy,
    ) -> Result<()> {
        use bif_core::usd::load_usd_with_stage_policy_muted;

        let path = path.as_ref();
        log::info!("Loading USD scene: {:?}", path);

        // Check if file exists
        if !path.exists() {
            return Err(anyhow::anyhow!("File not found: {:?}", path));
        }

        // v0.14.0 — preserve the user's layer-mute set across implicit
        // stage reopens (variant changes, mute toggles, etc.). The new
        // stage starts un-muted; we replay the mutes before payloads
        // load so the bridge caches geometry under the muted composition.
        let muted_snapshot: Vec<String> = self
            .scene
            .layer_state
            .as_ref()
            .map(|s| s.muted.iter().cloned().collect())
            .unwrap_or_default();

        // Load USD file via C++ bridge (handles usda, usdc, usd)
        let (scene, stage) = load_usd_with_stage_policy_muted(
            path,
            payload_policy,
            &muted_snapshot,
        )
        .map_err(|e| {
            log::error!("USD bridge error: {:?}", e);
            log::error!("Hint: Ensure USD environment is set up. Run: . .\\setup_usd_env.ps1");
            anyhow::anyhow!("Failed to load USD: {}", e)
        })?;

        self.finalize_usd_scene(scene, stage, path, payload_policy)
    }

    /// Finalize a loaded USD scene — create GPU resources and update viewport state.
    ///
    /// Called by both `load_usd_scene` (sync) and `finalize_usd_load` (async).
    fn finalize_usd_scene(
        &mut self,
        scene: bif_core::Scene,
        stage: bif_core::usd::UsdStage,
        path: &std::path::Path,
        payload_policy: PayloadPolicy,
    ) -> Result<()> {
        let viewport_load_start = Instant::now();
        self.scene.hidden_prim_paths.clear();

        // v0.14.0 — empty scenes are legitimate when the user mutes the
        // layer that provides the `def` (so composition strips the prim).
        // Install the stage + re-seed layer_state so the Layer Stack panel
        // can still unmute, then clear GPU state and return Ok.
        if scene.prototypes.is_empty() {
            log::info!(
                "Scene has no geometry after composition \
                 (likely from a mute that removed the def-providing layer) — \
                 clearing viewport; Layer Stack panel remains active for unmute"
            );

            if let Ok(mut layer_state) =
                bif_core::SceneLayerState::from_stage(&stage, payload_policy)
            {
                // No prim paths to map — prim_for_layer map stays empty.
                layer_state.populate_layer_for_prim(&stage, Vec::<String>::new());
                self.scene.layer_state = Some(layer_state);
            }

            self.scene.loaded_usd_path = Some(path.display().to_string());
            self.scene.usd_stage = Some(Arc::new(Mutex::new(stage)));
            self.scene.working_scene = bif_core::Scene::default();
            self.scene.mesh_data = MeshData::default();
            self.nodes.scene_graph_dirty = true;

            self.reload_working_scene()?;
            let elapsed = viewport_load_start.elapsed();
            log::info!(
                "Viewport cleared after empty-scene compose ({:>5.1}ms)",
                elapsed.as_secs_f64() * 1000.0
            );
            return Ok(());
        }

        // v0.14.0 — seed layer-aware state from the stage. We capture the
        // sublayer tree, edit target, and mute flags up front, then walk
        // every prim to build the `prim_path → strongest-layer-index` map
        // that drives scene-browser color dots. The caller threads the active
        // payload policy in so workspace-driven reloads preserve their mode.
        //
        // Stored on `SceneManager` directly (not `working_scene.layer_state`)
        // because this function doesn't assign the parsed `Scene` back into
        // `self.scene.working_scene` — any field set on the local `scene`
        // binding is lost at end of function.
        match bif_core::SceneLayerState::from_stage(&stage, payload_policy) {
            Ok(mut layer_state) => {
                let prim_paths: Vec<String> = stage
                    .all_prims()
                    .map(|prims| prims.into_iter().map(|p| p.path).collect())
                    .unwrap_or_default();
                layer_state.populate_layer_for_prim(&stage, &prim_paths);
                log::info!(
                    "Layer state: {} layers, {} muted, {} prims mapped",
                    layer_state.stack.layers.len(),
                    layer_state.muted.len(),
                    layer_state.layer_for_prim.len()
                );
                self.scene.layer_state = Some(layer_state);
            }
            Err(e) => {
                log::warn!("Failed to seed layer state from stage: {e}");
            }
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

        // Create per-prototype GPU data and build compact triangle material buffer
        let gpu_start = Instant::now();
        let mut compact_tri_mats: Vec<u32> = Vec::new();
        let mut prototype_gpu_data: Vec<PrototypeGpuData> = Vec::new();

        for (proto_id, proto) in scene.prototypes.iter().enumerate() {
            let mesh_data = MeshData::from_core_mesh(&proto.mesh);
            let num_triangles = mesh_data.indices.len() as u32 / 3;

            let vertex_buffer =
                self.gpu
                    .device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some(&format!("Prototype {} Vertex Buffer", proto_id)),
                        contents: bytemuck::cast_slice(&mesh_data.vertices),
                        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    });

            let index_buffer =
                self.gpu
                    .device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some(&format!("Prototype {} Index Buffer", proto_id)),
                        contents: bytemuck::cast_slice(&mesh_data.indices),
                        usage: wgpu::BufferUsages::INDEX,
                    });

            let triangle_material_buffer =
                mesh_data.triangle_material_ids.as_ref().map(|tri_mats| {
                    self.gpu
                        .device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some(&format!(
                                "Prototype {} Triangle Material Buffer",
                                proto_id
                            )),
                            contents: bytemuck::cast_slice(tri_mats),
                            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                        })
                });

            // Track offset into compact buffer before appending
            let tri_mat_offset = compact_tri_mats.len() as u32;
            if let Some(ref tri_mats) = mesh_data.triangle_material_ids {
                compact_tri_mats.extend_from_slice(tri_mats);
            } else {
                compact_tri_mats.extend(std::iter::repeat_n(0xFFFFFFFFu32, num_triangles as usize));
            }

            log::debug!(
                "Prototype {}: {} vertices, {} indices",
                proto_id,
                mesh_data.vertices.len(),
                mesh_data.indices.len()
            );

            prototype_gpu_data.push(PrototypeGpuData {
                vertex_buffer,
                index_buffer,
                num_indices: mesh_data.indices.len() as u32,
                num_vertices: mesh_data.vertices.len() as u32,
                prototype_id: proto_id,
                mesh_idx: proto_id,
                triangle_material_buffer,
                tri_mat_offset,
                num_triangles,
                vertices: mesh_data.vertices.clone(),
            });
        }

        // Build material index lookup early for combine_with_transforms
        let material_index_by_name: HashMap<Arc<str>, u32> = scene
            .materials
            .iter()
            .enumerate()
            .map(|(idx, mat)| (mat.name.clone(), idx as u32))
            .collect();
        let default_mat_index = scene.materials.len() as u32;

        // For backwards compatibility, also create a combined mesh_data for single-draw fallback
        // and for Ivar rendering (which expects a single mesh)
        let mesh_data = if scene.prototypes.len() == 1 {
            let mut md = MeshData::from_core_mesh(&scene.prototypes[0].mesh);
            // from_core_mesh sets triangle_material_ids=None when no GeomSubsets.
            // Fill with prototype's material index so Ivar uses correct material.
            if md.triangle_material_ids.is_none() {
                let mat_id = scene.prototypes[0]
                    .material
                    .as_ref()
                    .and_then(|mat| material_index_by_name.get(&mat.name).copied())
                    .unwrap_or(default_mat_index);
                let tri_count = md.indices.len() / 3;
                md.triangle_material_ids = Some(vec![mat_id; tri_count]);
            }
            md
        } else if !scene.instances().is_empty() {
            // Instanced scene: combine prototypes with instance transforms
            let mut meshes_with_transforms: Vec<(&bif_core::Mesh, Mat4, usize, u32)> = Vec::new();
            for (mesh_idx, inst) in scene.instances().iter().enumerate() {
                if let Some(proto) = scene.prototypes.get(inst.prototype_id) {
                    let mat_id = proto
                        .material
                        .as_ref()
                        .and_then(|mat| material_index_by_name.get(&mat.name).copied())
                        .unwrap_or(default_mat_index);
                    meshes_with_transforms.push((
                        &proto.mesh,
                        inst.model_matrix(),
                        mesh_idx,
                        mat_id,
                    ));
                }
            }
            MeshData::combine_with_transforms(&meshes_with_transforms)
        } else {
            // Direct meshes (no instancers): combine prototypes with identity transforms
            let meshes_with_transforms: Vec<(&bif_core::Mesh, Mat4, usize, u32)> = scene
                .prototypes
                .iter()
                .enumerate()
                .map(|(idx, proto)| {
                    let mat_id = proto
                        .material
                        .as_ref()
                        .and_then(|mat| material_index_by_name.get(&mat.name).copied())
                        .unwrap_or(default_mat_index);
                    (proto.mesh.as_ref(), Mat4::IDENTITY, idx, mat_id)
                })
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
            "Material: {} (metalness={:.2}, roughness={:.2})",
            scene_material.name,
            scene_material.base_metalness,
            scene_material.specular_roughness
        );

        log::info!(
            "Loaded {} vertices, {} indices from USD scene",
            mesh_data.vertices.len(),
            mesh_data.indices.len()
        );
        let gpu_time = gpu_start.elapsed();

        // Clean up stale .bif_cache/udim/ from old atlas stitching system
        let base_dir = path.parent();
        if let Some(dir) = base_dir {
            let stale_cache = dir.join(".bif_cache").join("udim");
            if stale_cache.exists() {
                match std::fs::remove_dir_all(&stale_cache) {
                    Ok(()) => log::info!("Cleaned up stale UDIM cache: {}", stale_cache.display()),
                    Err(e) => log::warn!("Failed to clean UDIM cache: {}", e),
                }
            }
        }

        // Prepare placeholder textures (instant) and start async loading
        let texture_start = Instant::now();
        let (gpu_textures, tile_paths) = texture_loader::prepare_texture_placeholders(
            &self.gpu.device,
            &self.gpu.queue,
            &scene,
            base_dir,
        );
        self.textures.gpu_textures = gpu_textures;
        // Start background texture loading — textures stream in via poll_texture_loads()
        self.async_channels.texture_load_receiver =
            Some(texture_loader::start_texture_loading_async(tile_paths));

        // Start background .tx conversion — next load of same scene uses cached .tx
        #[cfg(feature = "oiio")]
        {
            let tex_paths = texture_loader::collect_scene_texture_paths(&scene, base_dir);
            if !tex_paths.is_empty() {
                self.environment
                    .start_tx_conversion(tex_paths, base_dir.map(|p| p.to_path_buf()));
            }
        }

        let texture_time = texture_start.elapsed();
        let texture_count = self.textures.gpu_textures.textures.len();

        let mut material_table: Vec<MaterialGpu> = scene
            .materials
            .iter()
            .map(|mat| MaterialGpu::from_material(mat.as_ref(), &self.textures.gpu_textures))
            .collect();
        let default_mat_index = material_table.len() as u32;
        material_table.push(MaterialGpu::from_material(
            &bif_core::Material::default(),
            &self.textures.gpu_textures,
        ));
        self.materials.table_len = material_table.len() as u32;
        self.materials.table_buffer =
            self.gpu
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Material Table Buffer"),
                    contents: bytemuck::cast_slice(&material_table),
                    usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                });

        // Triangle material buffer — use compact buffer (one copy per prototype).
        // Must always be full-sized: the shader indexes by primitive_id + tri_mat_offset,
        // and out-of-bounds GPU storage reads return 0 (not sentinel 0xFFFFFFFF).
        self.materials.has_triangle_materials =
            compact_tri_mats.iter().any(|&id| id != 0xFFFFFFFFu32);
        if compact_tri_mats.is_empty() {
            compact_tri_mats.push(0xFFFFFFFFu32);
        }
        // Cap to GPU max_storage_buffer_binding_size
        let max_buf = self.gpu.device.limits().max_storage_buffer_binding_size as usize;
        let buf_bytes = compact_tri_mats.len() * std::mem::size_of::<u32>();
        if buf_bytes > max_buf {
            let max_entries = max_buf / std::mem::size_of::<u32>();
            log::warn!(
                "Triangle material buffer {} MB exceeds GPU limit {} MB — truncating ({} / {} entries)",
                buf_bytes / (1024 * 1024),
                max_buf / (1024 * 1024),
                max_entries,
                compact_tri_mats.len()
            );
            compact_tri_mats.truncate(max_entries);
        }
        self.materials.triangle_buffer =
            self.gpu
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Triangle Material Buffer (compact)"),
                    contents: bytemuck::cast_slice(&compact_tri_mats),
                    usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                });

        self.materials.bind_group = self
            .gpu
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Material Bind Group"),
                layout: &self.materials.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.materials.buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: self.materials.table_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: self.materials.triangle_buffer.as_entire_binding(),
                    },
                ],
            });

        let texture_view_refs: Vec<&wgpu::TextureView> =
            self.textures.gpu_textures.views.iter().collect();
        self.textures.bind_group = self
            .gpu
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Texture Bind Group"),
                layout: &self.textures.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureViewArray(&texture_view_refs),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.textures.sampler),
                    },
                ],
            });

        // Create new vertex/index buffers — guard against GPU max buffer size
        let max_buf_size = self.gpu.device.limits().max_buffer_size as usize;
        let vb_size = std::mem::size_of_val(mesh_data.vertices.as_slice());
        let ib_size = std::mem::size_of_val(mesh_data.indices.as_slice());
        let combined_too_large = vb_size > max_buf_size || ib_size > max_buf_size;
        if combined_too_large {
            log::warn!(
                "Combined mesh too large for GPU (verts={} MB, idx={} MB, limit={} MB) — multi-draw only",
                vb_size / (1024 * 1024),
                ib_size / (1024 * 1024),
                max_buf_size / (1024 * 1024),
            );
        }
        let placeholder_vert = crate::gpu_types::Vertex {
            position: [0.0; 3],
            normal: [0.0; 3],
            color: [0.0; 3],
            uv: [0.0; 2],
            material_id: 0,
        };
        let vertex_buffer = self
            .gpu
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Vertex Buffer"),
                contents: if combined_too_large {
                    bytemuck::bytes_of(&placeholder_vert)
                } else {
                    bytemuck::cast_slice(&mesh_data.vertices)
                },
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            });
        let index_buffer = self
            .gpu
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Index Buffer"),
                contents: if combined_too_large {
                    bytemuck::cast_slice(&[0u32])
                } else {
                    bytemuck::cast_slice(&mesh_data.indices)
                },
                usage: wgpu::BufferUsages::INDEX,
            });
        // If combined mesh was too large, clear it on self.scene.mesh_data after assignment
        // (finalize_usd_scene sets self.scene.mesh_data later)

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
                        .unwrap_or(default_mat_index);
                    instance_material_ids.push(material_id);
                    InstanceData {
                        model_matrix: model_matrix.to_cols_array_2d(),
                        material_id,
                        tri_mat_offset: 0,
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
                        .unwrap_or(default_mat_index);
                    instance_material_ids.push(material_id);
                    InstanceData {
                        model_matrix: model_matrix.to_cols_array_2d(),
                        material_id,
                        tri_mat_offset: 0,
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
        self.gpu.queue.write_buffer(
            &self.instance_buffer,
            0,
            bytemuck::cast_slice(&instances[..write_count]),
        );

        log::info!(
            "Created {} instances from USD scene (wrote {})",
            instances.len(),
            write_count
        );

        self.scene.instances.material_ids = instance_material_ids;
        self.scene.instances.prototype_ids = instance_prototype_ids;

        // Build prim path mapping for USD export
        self.scene.instances.prim_paths = scene
            .instances()
            .iter()
            .enumerate()
            .map(|(idx, inst)| resolve_prim_path(inst, &scene, idx))
            .collect();

        // Store animation data for viewport playback
        self.scene.instance_animations = scene.instance_animations().to_vec();
        self.scene.last_evaluated_frame = 0.0;

        let animated_count = self
            .scene
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
        self.scene.vertex_animated_meshes.clear();
        let mesh_count = stage.mesh_count().unwrap_or(0);
        for mesh_idx in 0..mesh_count {
            if let Ok(times) = stage.get_mesh_vertex_animation_times(mesh_idx) {
                if !times.is_empty() {
                    log::info!(
                        "Mesh {} has vertex animation ({} time samples)",
                        mesh_idx,
                        times.len()
                    );
                    self.scene.vertex_animated_meshes.push(mesh_idx);
                }
            }
        }

        // Detect skinned prototypes (v0.13.5 Phase 3). Walk prototype list, match
        // each one's skin to a stage skeleton index, and snapshot the data needed
        // for per-frame CPU LBS so the hot path is allocation-free.
        self.scene.skinned_meshes.clear();
        let skel_count = stage.skeleton_count().unwrap_or(0);
        if skel_count > 0 {
            use std::collections::HashMap;
            let mut skel_idx_by_path: HashMap<String, usize> = HashMap::new();
            for i in 0..skel_count {
                if let Ok(sk) = stage.get_skeleton(i) {
                    skel_idx_by_path.insert(sk.path, i);
                }
            }

            // USD mesh path → mesh_idx (matches MeshRange::usd_mesh_index)
            let mut mesh_idx_by_path: HashMap<String, usize> = HashMap::new();
            if let Ok(all_meshes) = stage.meshes() {
                for (idx, md) in all_meshes.iter().enumerate() {
                    mesh_idx_by_path.insert(md.path.clone(), idx);
                }
            }

            for (proto_id, proto) in scene.prototypes.iter().enumerate() {
                let Some(skin) = &proto.mesh.skin else {
                    continue;
                };
                let Some(bind_positions) = &proto.mesh.bind_positions else {
                    continue;
                };
                let Some(&skel_idx) = skel_idx_by_path.get(&skin.skeleton_path) else {
                    log::warn!(
                        "Proto '{}' references skeleton '{}' not found in stage",
                        proto.name,
                        skin.skeleton_path
                    );
                    continue;
                };
                let Some(&mesh_idx) = mesh_idx_by_path.get(&*proto.name) else {
                    log::warn!(
                        "Proto '{}' has skin binding but no matching stage mesh_idx",
                        proto.name
                    );
                    continue;
                };

                let vert_count = bind_positions.len();

                // v0.13.6: cache blend shape data alongside skin for per-frame eval
                let blend_shapes = proto.mesh.blend_shapes.clone();
                let bind_normals = proto.mesh.bind_normals.clone();
                let has_bs = blend_shapes.is_some();
                let blend_scratch_pos = if has_bs {
                    vec![bif_math::Vec3::ZERO; vert_count]
                } else {
                    Vec::new()
                };
                let blend_scratch_norm = if has_bs {
                    bind_normals
                        .as_ref()
                        .map(|bn| vec![bif_math::Vec3::ZERO; bn.len()])
                } else {
                    None
                };

                self.scene
                    .skinned_meshes
                    .push(crate::scene_manager::SkinnedMeshEntry {
                        proto_id,
                        mesh_idx,
                        skel_idx,
                        bind_positions: bind_positions.clone(),
                        skin: skin.clone(),
                        skinned_scratch: vec![bif_math::Vec3::ZERO; vert_count],
                        blend_shapes,
                        bind_normals,
                        blend_scratch_pos,
                        blend_scratch_norm,
                    });
            }
            if !self.scene.skinned_meshes.is_empty() {
                log::info!(
                    "UsdSkel: registered {} skinned prototypes for CPU LBS",
                    self.scene.skinned_meshes.len()
                );
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
        self.scene.mesh_data = mesh_data;
        self.scene.instances.current = instance_transforms.clone();
        self.scene.instances.transforms = instance_transforms;
        self.scene.scene_material = scene_material.clone();
        self.scene.scene_materials = scene.materials.clone();
        self.scene.scene_cameras = scene.cameras.clone();
        // Reset stale scene camera selection
        if let crate::ivar_state::CameraSource::SceneCamera(idx) = self.cam.viewport_camera_source {
            if idx >= self.scene.scene_cameras.len() {
                self.cam.viewport_camera_source = crate::ivar_state::CameraSource::Viewport;
                self.cam.camera_locked = false;
            }
        }
        self.scene.texture_base_dir = path.parent().map(|p| p.to_path_buf());

        // Store multi-draw state
        self.multi_draw.prototype_gpu_data = prototype_gpu_data;
        self.multi_draw.enabled = use_multi_draw;

        // Group instances by prototype for multi-draw rendering
        self.multi_draw.rebuild_instance_groups(
            &self.scene.instances.transforms,
            &self.scene.instances.prototype_ids,
            &self.scene.instances.material_ids,
            &self.scene.instances.purposes,
            self.display_settings.purpose_mode,
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
        self.materials.uniform = MaterialUniform::from_material(&scene_material);
        self.gpu.queue.write_buffer(
            &self.materials.buffer,
            0,
            bytemuck::cast_slice(&[self.materials.uniform]),
        );

        // Update culling manager with new prototype and instance AABBs
        let triangles_per_instance = self.num_indices / 3;
        self.culling
            .set_prototype_aabb(&self.gpu.device, prototype_aabb, triangles_per_instance);
        self.culling.instance_aabbs = instance_aabbs;
        self.culling.lod_box_count = 0;
        self.num_triangles = triangles_per_instance as u64 * self.num_instances as u64;
        log::info!(
            "Updated culling manager for prototype AABB: {:?} to {:?}",
            prototype_aabb.min_point(),
            prototype_aabb.max_point()
        );

        // Update USD stage for scene browser (wrapped in Arc<Mutex> for thread-safe sharing)
        let stage = Arc::new(Mutex::new(stage));

        // Log available cameras for batch render
        match stage
            .lock()
            .expect("UsdStage mutex poisoned")
            .camera_paths()
        {
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

        self.scene.usd_stage = Some(stage);
        // Canonicalize and strip Windows extended-length prefix.
        // canonicalize() returns \\?\C:\... for local paths and \\?\UNC\server\... for UNC paths.
        // Strip \\?\UNC\ → \\  (network path) or \\?\ → (local path).
        let canonical = std::fs::canonicalize(path)
            .unwrap_or_else(|_| path.to_path_buf())
            .display()
            .to_string();
        let clean_path = if let Some(unc) = canonical.strip_prefix(r"\\?\UNC\") {
            format!(r"\\{}", unc)
        } else if let Some(local) = canonical.strip_prefix(r"\\?\") {
            local.to_string()
        } else {
            canonical
        };
        self.scene.loaded_usd_path = Some(clean_path);

        // Reset scene browser selection
        self.selection.selected_prim_path = None;
        self.selection.selected_prim_properties = None;

        // Reset camera source to viewport (clear stale USD camera selection from previous scene)
        self.cam.viewport_camera_source = crate::ivar_state::CameraSource::Viewport;
        self.cam.selected_usd_camera = None;
        self.cam.camera_locked = false;

        // Reset batch render camera source to viewport
        self.ivar.ivar_state.batch_settings.camera_source =
            crate::ivar_state::CameraSource::default();

        // Update camera to frame the scene
        self.cam.camera.target = mesh_center;
        self.cam.camera.distance = camera_distance;
        self.cam.camera.near = camera_distance * 0.01;
        self.cam.camera.far = camera_distance * 20.0;
        self.cam.camera.update_position_from_angles();
        self.update_camera();

        // Invalidate Ivar scene + materials (new scene = new textures)
        self.invalidate_ivar_materials();
        self.ivar.ivar_texture_cache = None;
        self.ivar.ivar_state.invalidate_scene();

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
        } else if !self.scene.vertex_animated_meshes.is_empty() {
            // No scene timeline, but we have vertex animation - detect time range from vertex animation
            let mut min_time = f64::MAX;
            let mut max_time = f64::MIN;

            if let Some(ref usd_stage_mtx) = self.scene.usd_stage {
                let usd_stage = usd_stage_mtx.lock().expect("UsdStage mutex poisoned");
                for &mesh_idx in &self.scene.vertex_animated_meshes {
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

            for anim in self.scene.instance_animations.iter().flatten() {
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
        if self.scene.working_scene.stage_metadata.is_none() {
            self.scene.working_scene.stage_metadata = scene.stage_metadata.clone();
        }

        // Merge USD scene into working_scene so primitives added later coexist.
        // Track offsets for remapping IDs from the loaded scene to the working scene.
        let proto_offset = self.scene.working_scene.prototype_count();
        let instance_offset = self.scene.working_scene.instance_count();
        // Material offset: face_material_ids in the loaded scene are 0-based,
        // but after merge they must index into working_scene.materials which
        // already contains materials from prior scenes.
        let mat_offset = self.scene.working_scene.materials.len() as u32;
        for proto in &scene.prototypes {
            let mut remapped = (**proto).clone();
            // Remap per-face material IDs to account for existing materials
            if mat_offset > 0 {
                if let Some(ref mut ids) = Arc::make_mut(&mut remapped.mesh).face_material_ids {
                    for id in ids.iter_mut() {
                        *id += mat_offset;
                    }
                }
            }
            remapped.id = self.scene.working_scene.prototype_count();
            self.scene.working_scene.prototypes.push(Arc::new(remapped));
        }
        for (inst, anim) in scene.instances_with_animations() {
            let remapped_proto_id = inst.prototype_id + proto_offset;
            let prim_path = inst.prim_path.clone();
            let inst_idx = if let Some(anim) = anim {
                self.scene.working_scene.add_animated_instance_with_path(
                    remapped_proto_id,
                    inst.transform,
                    anim.clone(),
                    prim_path,
                )
            } else {
                self.scene.working_scene.add_instance_with_path(
                    remapped_proto_id,
                    inst.transform,
                    prim_path,
                )
            };
            // Preserve purpose from loaded scene
            self.scene
                .working_scene
                .set_instance_purpose(inst_idx, inst.purpose);
        }
        let mat_source_dir = path.parent().map(|p| p.to_path_buf());
        for mat in &scene.materials {
            let mut with_dir = (**mat).clone();
            with_dir.source_dir = mat_source_dir.clone();
            self.scene.working_scene.add_material(with_dir);
        }
        // Merge point clouds and sync next_cloud_id to avoid ID collisions
        for cloud in &scene.point_clouds {
            let mut merged = cloud.clone();
            merged.id = self.nodes.next_cloud_id;
            self.nodes.next_cloud_id += 1;
            self.scene.working_scene.add_point_cloud(merged);
        }
        // Remap camera instance indices by the instance offset
        for cam in &scene.cameras {
            let mut remapped = cam.clone();
            remapped.instance_index += instance_offset;
            self.scene.working_scene.cameras.push(remapped);
        }

        // Update lights from scene
        self.update_lights(&scene.lights);

        // Auto-connect first DomeLight with texture to HdriEnvironment
        // (skip if user already loaded an HDRI via the node graph)
        if !self.environment.hdri_loaded {
            if let Some(bif_core::Light::Dome {
                rotation,
                intensity,
                texture_path: Some(path),
            }) = scene.lights.iter().find(|l| {
                matches!(
                    l,
                    bif_core::Light::Dome {
                        texture_path: Some(_),
                        ..
                    }
                )
            }) {
                log::info!("Auto-loading DomeLight HDRI: {}", path);
                self.environment.start_hdri_load(
                    std::path::Path::new(path.as_ref()),
                    rotation.to_degrees(),
                    *intensity,
                    true,
                );
            }
        }

        // Upload curves/points prims to preview renderer
        if !self.scene.working_scene.curves.is_empty()
            || !self.scene.working_scene.points_prims.is_empty()
        {
            self.curve_preview.upload_curves(
                &self.gpu.device,
                &self.gpu.queue,
                &self.scene.working_scene.curves,
                &self.scene.working_scene.points_prims,
            );
            self.curve_preview.update_params(&self.gpu.queue);
            log::info!(
                "Curve preview: {} curves, {} points prims",
                self.scene.working_scene.curves.len(),
                self.scene.working_scene.points_prims.len()
            );
        }

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

        // Ivar materials built on-demand when render starts (prewarm disabled to save RAM)

        Ok(())
    }
}
