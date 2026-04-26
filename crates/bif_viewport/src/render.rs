//! # State Mutation Convention
//!
//! **Direct mutation** (in egui closures): Simple boolean toggles with no side
//! effects (show_grid, point_preview.visible). Safe because they only
//! affect the next frame's rendering, with no cascading state changes.
//!
//! **EventBus**: Anything triggering side effects (scene reload, camera sync,
//! undo/redo, node graph operations). Events are drained in `dispatch_events()`
//! for predictable ordering.

use crate::batch_render::BatchMessage;
use crate::environment_manager::IblResult;
use crate::gpu_types::InstanceData;
use crate::ivar_state::{BatchRenderStatus, BuildStatus, RenderMode};
use crate::node_graph::NodeGraphEvent;
use crate::scene_browser::{self};
use crate::Renderer;
use anyhow::Result;

impl Renderer {
    /// Render a frame with the given clear color. Pure 3D path — no egui overlay.
    pub fn render(&mut self, clear_color: wgpu::Color) -> Result<()> {
        self.poll_async_work();
        self.poll_environment();
        self.dispatch_events();
        self.submit_gpu_frame(clear_color)
    }

    /// Phase 1: Poll async work — USD load, texture streaming, scene graph, scene ops.
    fn poll_async_work(&mut self) {
        // Poll async USD load (non-blocking)
        self.poll_usd_load();

        // Poll async texture streaming (non-blocking)
        self.poll_texture_loads();

        // Rebuild cached scene graph if dirty
        if self.nodes.scene_graph_dirty {
            self.nodes.cached_scene_graph = scene_browser::build_scene_graph_cache(
                &self.scene.working_scene,
                &self.nodes.node_proto_map,
                &self.nodes.node_cloud_map,
            );
            self.nodes.node_prim_counts = self.nodes.cached_scene_graph.prim_count_by_node();
            self.nodes.scene_graph_dirty = false;
        }

        // Process pending scene operations from undo/redo
        if !self.scene.edit_state.pending_scene_ops.is_empty() {
            let ops: Vec<_> = self.scene.edit_state.pending_scene_ops.drain(..).collect();
            let mut needs_reload = false;
            for op in ops {
                match op {
                    bif_core::SceneOp::AddPrimitive { kind, size, name } => {
                        if let Err(e) = self.add_primitive_by_name(kind, size, &name) {
                            log::error!("Undo/redo add primitive failed: {}", e);
                        } else {
                            needs_reload = false; // add_primitive_by_name already reloads
                        }
                    }
                    bif_core::SceneOp::RemovePrimitive { proto_id } => {
                        if self.scene.working_scene.remove_prototype(proto_id) {
                            needs_reload = true;
                        }
                    }
                    bif_core::SceneOp::AddPointCloud { cloud } => {
                        self.scene.working_scene.add_point_cloud(*cloud);

                        // Update point preview
                        let all_positions: Vec<bif_math::Vec3> = self
                            .scene
                            .working_scene
                            .point_clouds
                            .iter()
                            .flat_map(|c| c.positions.iter().copied())
                            .collect();
                        self.point_preview.upload_points(
                            &self.gpu.device,
                            &self.gpu.queue,
                            &all_positions,
                        );
                        self.point_preview_params_dirty = true;
                        self.point_preview.visible = true;
                    }
                    bif_core::SceneOp::RemovePointCloud { cloud_id } => {
                        if self.scene.working_scene.remove_point_cloud(cloud_id) {
                            // Update point preview
                            let all_positions: Vec<bif_math::Vec3> = self
                                .scene
                                .working_scene
                                .point_clouds
                                .iter()
                                .flat_map(|c| c.positions.iter().copied())
                                .collect();
                            self.point_preview.upload_points(
                                &self.gpu.device,
                                &self.gpu.queue,
                                &all_positions,
                            );
                            self.point_preview_params_dirty = true;
                        }
                    }
                }
            }
            if needs_reload {
                if let Err(e) = self.reload_working_scene() {
                    log::error!("Failed to reload working scene after undo/redo: {}", e);
                }
            }
        }
    }

    /// Phase 2: Poll environment — IBL, .tx conversion, batch render, selection sync, frustum culling.
    fn poll_environment(&mut self) {
        self.poll_ibl_result();
        self.poll_batch_messages();

        // Sync selection highlight to GPU camera uniform
        let sel_id = self.selection.gpu_highlight_id();
        if self.cam.camera_uniform.selected_instance_id != sel_id {
            self.cam.camera_uniform.selected_instance_id = sel_id;
            self.gpu.queue.write_buffer(
                &self.cam.camera_buffer,
                0,
                bytemuck::cast_slice(&[self.cam.camera_uniform]),
            );
        }

        // Update frustum culling before rendering (in Vulkan mode)
        if self.ivar.ivar_state.mode == RenderMode::Vulkan {
            self.update_visible_instances();
        }
    }

    /// Phase 4: Dispatch typed events from the EventBus.
    fn dispatch_events(&mut self) {
        use crate::app_event::AppEvent;

        let events = self.event_bus.drain();
        for event in events {
            match event {
                // Render events → render_dispatch.rs
                AppEvent::RenderModeChanged => self.handle_render_mode_changed(),
                AppEvent::RebuildScene => self.handle_rebuild_scene(),
                AppEvent::DenoiseRequested => self.handle_denoise_requested(),
                AppEvent::FilterChanged => self.handle_filter_changed(),
                AppEvent::StartBatchRender => self.handle_start_batch_render(),
                AppEvent::CancelBatchRender => self.handle_cancel_batch_render(),

                // Camera (thin, stays here)
                AppEvent::SyncUsdCamera(path) => self.sync_viewport_to_usd_camera(&path),
                AppEvent::SyncSceneCamera(idx) => self.sync_viewport_to_scene_camera(idx as usize),
                AppEvent::CameraProjectionChange(proj) => {
                    match proj {
                        crate::app_event::CameraProjection::Perspective => {
                            self.cam.camera.set_perspective();
                        }
                        crate::app_event::CameraProjection::Ortho(preset_name) => {
                            for preset in bif_math::OrthoPreset::all() {
                                if preset.display_name() == preset_name {
                                    self.cam.camera.set_ortho_preset(*preset);
                                    break;
                                }
                            }
                        }
                    }
                    self.update_camera();
                }

                // Selection events → selection_dispatch.rs
                AppEvent::PrimSelected(path) => self.handle_prim_selected(path),
                AppEvent::TransformEdit(edit) => self.handle_transform_edit(edit),
                AppEvent::StageCorrectionsChanged => self.handle_stage_corrections_changed(),
                AppEvent::SetKeyframe(idx) => self.handle_set_keyframe(idx),
                AppEvent::ExportEditLayer(path) => self.handle_export_edit_layer(path),
                AppEvent::FrameSelected => self.handle_frame_selected(),
                AppEvent::VariantChanged(p, s, v) => self.handle_variant_changed(p, s, v),

                // Node graph → node_dispatch.rs
                AppEvent::NodeGraph(node_events) => {
                    let has_mutation = node_events.iter().any(|e| {
                        !matches!(
                            e,
                            NodeGraphEvent::SelectNode(_) | NodeGraphEvent::SetDisplayNode(_)
                        )
                    });
                    if has_mutation {
                        self.project.mark_dirty();
                    }
                    for event in node_events {
                        self.handle_node_graph_event(event);
                    }
                }

                // Project events → project_dispatch.rs
                AppEvent::ProjectNew => self.handle_project_new(),
                AppEvent::ProjectOpen => self.handle_project_open(),
                AppEvent::ProjectSave => self.handle_project_save(),
                AppEvent::ProjectSaveAs => self.handle_project_save_as(),
                AppEvent::ProjectOpenRecent(path) => self.handle_project_open_recent(path),

                // Layer-aware stage (v0.14.0) — read-only inspection.
                // LayerSelected is purely informational in v0.14 (the panel
                // tracks its own focus for future keyboard nav / context
                // menus); no scene or stage side effect.
                AppEvent::LayerSelected(idx) => {
                    log::debug!("LayerSelected({idx})");
                }
                AppEvent::LayerMuteToggled { index, muted } => {
                    // Resolve index → identifier, then push the mute through
                    // to USD and mirror it in our SceneLayerState so the UI
                    // and the stage stay in sync.
                    let identifier = self
                        .scene
                        .layer_state
                        .as_ref()
                        .and_then(|s| s.stack.layers.get(index))
                        .map(|l| l.identifier.clone());
                    if let Some(identifier) = identifier {
                        if let Some(stage_arc) = self.scene.usd_stage.as_ref() {
                            match stage_arc.lock() {
                                Ok(stage) => {
                                    if let Err(e) = stage.set_layer_muted(&identifier, muted) {
                                        log::warn!(
                                            "Failed to {} layer {identifier}: {e}",
                                            if muted { "mute" } else { "unmute" }
                                        );
                                    }
                                }
                                Err(e) => log::warn!("UsdStage lock poisoned: {e}"),
                            }
                        }
                        if let Some(state) = self.scene.layer_state.as_mut() {
                            state.set_muted(&identifier, muted);
                        }
                        // v0.14.0 — `set_layer_muted` recomposes the stage
                        // but the C++ bridge's mesh cache + our GPU buffers
                        // are still pre-mute. Route the reload through the
                        // UsdRead node graph handler rather than calling
                        // `load_usd_scene` directly: it cleans up the old
                        // prototype ownership via `remove_and_reindex_prototype`
                        // + `compact_materials` before the reload, otherwise
                        // working_scene accumulates new prototypes on top of
                        // stale ones and the viewport shows two cubes.
                        //
                        // `load_usd_scene` snapshots `layer_state.muted` and
                        // replays it against the freshly-opened stage before
                        // payloads load, so the cache repopulates under the
                        // new composition.
                        if let Some(path) = self.scene.loaded_usd_path.clone() {
                            // Find the UsdRead node that currently owns the
                            // scene. There should be exactly one loaded via
                            // the node graph; if zero we fall back to a bare
                            // reload (e.g., someone opened the file outside
                            // the node-graph path).
                            let owning_node = self.nodes.node_proto_map.keys().next().copied();
                            if let Some(node_id) = owning_node {
                                self.handle_node_graph_event(NodeGraphEvent::LoadUsdFile {
                                    path: path.clone(),
                                    node_id,
                                });
                            } else if let Err(e) = self.load_usd_scene(std::path::Path::new(&path))
                            {
                                log::error!("Failed to reload after mute toggle: {e}");
                            }
                        }
                    }
                }
                AppEvent::WorkingLayerChanged(idx) => {
                    let stage_arc = self.scene.usd_stage.clone();
                    if let Some(state) = self.scene.layer_state.as_mut() {
                        if let Some(stage_arc) = stage_arc {
                            match stage_arc.lock() {
                                Ok(stage) => {
                                    if let Err(e) = state.set_edit_target(idx, &stage) {
                                        log::warn!("Failed to set edit target {idx}: {e}");
                                    }
                                }
                                Err(e) => log::warn!("UsdStage lock poisoned: {e}"),
                            }
                        } else {
                            state.set_working_layer(idx);
                        }
                    }
                }
                AppEvent::PayloadPolicyChanged(policy) => {
                    // Reopening the stage with a different policy is a v0.14.5
                    // concern (file watcher + reload UX land together). For now
                    // record the intent in scene state so downstream reads see
                    // the user's choice.
                    if let Some(state) = self.scene.layer_state.as_mut() {
                        state.payload_policy = policy;
                    }
                    log::info!(
                        "PayloadPolicyChanged({policy:?}) — stage reload deferred to v0.14.5"
                    );
                }
                AppEvent::IsolationModeToggled(on) => {
                    if let Some(state) = self.scene.layer_state.as_mut() {
                        state.isolation_mode = on;
                    }
                }
            }
        }

        // Handle Xform property changes (not from event bus — direct field flag)
        if let Some(_xform_nid) = self.nodes.xform_property_changed.take() {
            if let Err(e) = self.reload_working_scene() {
                log::error!("Failed to reload after xform property change: {}", e);
            }
        }

        // Update keyframe times for timeline markers
        self.update_keyframe_times();
    }

    // handle_node_graph_event() lives in node_dispatch.rs

    /// Poll for completed async IBL generation and .tx conversion.
    fn poll_ibl_result(&mut self) {
        if let Some(result) = self.environment.poll_ibl_result() {
            match result {
                IblResult::Success {
                    hdr_pixels,
                    hdr_width,
                    hdr_height,
                    ivar_env,
                    source_path,
                    load_path,
                    rotation_rad,
                    intensity,
                    show_background,
                    load_secs,
                } => {
                    let compute_secs = self.environment.apply_ibl_result(
                        &self.gpu.device,
                        &self.gpu.queue,
                        &hdr_pixels,
                        hdr_width,
                        hdr_height,
                        rotation_rad,
                        intensity,
                        show_background,
                    );
                    self.ivar.ivar_state.environment = Some(ivar_env);
                    self.ivar.ivar_state.hdri_rotation = rotation_rad;
                    self.ivar.ivar_state.hdri_intensity = intensity;
                    self.nodes.node_graph_state.mark_hdri_loaded(
                        &source_path,
                        Some(load_secs),
                        Some(compute_secs),
                    );
                    log::info!("HDRI loaded (GPU compute): {}", load_path);
                }
                IblResult::Error {
                    source_path,
                    load_path,
                    message,
                } => {
                    log::error!("Failed to load HDRI: {}", message);
                    self.nodes
                        .node_graph_state
                        .mark_hdri_error(&source_path, message);
                    log::error!("HDRI load failed: {}", load_path);
                }
            }
        }

        if let Some(status) = self.environment.poll_tx_result() {
            self.nodes
                .node_graph_state
                .mark_tx_conversion_complete(status);
        }
    }

    /// Poll for batch render messages from the background thread.
    fn poll_batch_messages(&mut self) {
        let mut clear_batch_state = false;
        if let Some(ref rx) = self.async_channels.batch_receiver {
            while let Ok(msg) = rx.try_recv() {
                match msg {
                    BatchMessage::Progress {
                        current_frame,
                        total_frames,
                        frame_progress,
                    } => {
                        self.ivar.ivar_state.batch_status = BatchRenderStatus::Rendering {
                            current_frame,
                            total_frames,
                            frame_progress,
                        };
                    }
                    BatchMessage::FrameComplete {
                        frame,
                        elapsed_secs,
                    } => {
                        log::info!("Batch frame {} complete in {:.1}s", frame, elapsed_secs);
                    }
                    BatchMessage::Complete { total_elapsed_secs } => {
                        log::info!("Batch render complete in {:.1}s", total_elapsed_secs);
                        self.ivar.ivar_state.batch_status =
                            BatchRenderStatus::Complete { total_elapsed_secs };
                        clear_batch_state = true;
                    }
                    BatchMessage::Cancelled => {
                        log::info!("Batch render cancelled");
                        self.ivar.ivar_state.batch_status = BatchRenderStatus::Cancelled;
                        clear_batch_state = true;
                    }
                    BatchMessage::Error(e) => {
                        log::error!("Batch render error: {}", e);
                        self.ivar.ivar_state.batch_status = BatchRenderStatus::Failed(e);
                        clear_batch_state = true;
                    }
                }
            }
        }
        if clear_batch_state {
            self.async_channels.batch_receiver = None;
            self.async_channels.batch_cancel_flag = None;
        }
    }

    /// Phase 5: Submit GPU render passes, present frame.
    fn submit_gpu_frame(&mut self, clear_color: wgpu::Color) -> Result<()> {
        let output = self.gpu.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        // Update point preview params before render pass (only when dirty or viewport resized).
        // Single writer for params buffer — upload_points never writes params.
        {
            let (_, _, vp_w, vp_h) = self.viewport_rect();
            let vp_size = (vp_w, vp_h);
            // f32 eq is safe: viewport dims derive from integer pixel sizes and
            // deterministic egui panel widths, not iterative floating-point math.
            if self.point_preview_params_dirty || vp_size != self.point_preview_last_vp {
                self.point_preview
                    .update_params(&self.gpu.queue, vp_w, vp_h);
                self.point_preview_last_vp = vp_size;
                self.point_preview_params_dirty = false;
            }
        }

        // Main render pass - dispatch based on render mode
        match self.ivar.ivar_state.mode {
            RenderMode::Vulkan => {
                // Standard GPU viewport rendering

                // Skybox pass (renders environment background before geometry)
                if self.environment.should_render_skybox() {
                    let mut skybox_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("Skybox Pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(clear_color),
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                            view: &self.depth_view,
                            depth_ops: Some(wgpu::Operations {
                                load: wgpu::LoadOp::Clear(1.0),
                                store: wgpu::StoreOp::Store,
                            }),
                            stencil_ops: None,
                        }),
                        timestamp_writes: None,
                        occlusion_query_set: None,
                    });
                    let (vp_x, vp_y, vp_w, vp_h) = self.viewport_rect();
                    let (sx, sy, sw, sh) = self.viewport_scissor();
                    skybox_pass.set_viewport(vp_x, vp_y, vp_w, vp_h, 0.0, 1.0);
                    skybox_pass.set_scissor_rect(sx, sy, sw, sh);
                    self.environment
                        .render_skybox(&mut skybox_pass, &self.cam.camera_bind_group);
                }

                // Geometry pass
                {
                    let color_load = if self.environment.should_render_skybox() {
                        wgpu::LoadOp::Load
                    } else {
                        wgpu::LoadOp::Clear(clear_color)
                    };
                    let depth_load = if self.environment.should_render_skybox() {
                        wgpu::LoadOp::Load
                    } else {
                        wgpu::LoadOp::Clear(1.0)
                    };

                    let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("Render Pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: color_load,
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                            view: &self.depth_view,
                            depth_ops: Some(wgpu::Operations {
                                load: depth_load,
                                store: wgpu::StoreOp::Store,
                            }),
                            stencil_ops: None,
                        }),
                        timestamp_writes: None,
                        occlusion_query_set: None,
                    });

                    // Constrain geometry to viewport area (excludes UI panels)
                    let (vp_x, vp_y, vp_w, vp_h) = self.viewport_rect();
                    let (sx, sy, sw, sh) = self.viewport_scissor();
                    render_pass.set_viewport(vp_x, vp_y, vp_w, vp_h, 0.0, 1.0);
                    render_pass.set_scissor_rect(sx, sy, sw, sh);

                    // Set common pipeline state
                    render_pass.set_pipeline(&self.pipeline);
                    render_pass.set_bind_group(0, &self.cam.camera_bind_group, &[]);
                    render_pass.set_bind_group(1, &self.materials.bind_group, &[]);
                    render_pass.set_bind_group(2, &self.textures.bind_group, &[]);
                    render_pass.set_bind_group(3, self.environment.bind_group(), &[]);
                    render_pass.set_bind_group(4, &self.lights.bind_group, &[]);

                    if self.multi_draw.enabled && !self.multi_draw.prototype_gpu_data.is_empty() {
                        // Multi-draw: iterate over each prototype's GPU data
                        let mut total_instances_drawn = 0u32;
                        let mut buffer_offset = 0u64;

                        for proto_data in &self.multi_draw.prototype_gpu_data {
                            if let Some(instances) = self
                                .multi_draw
                                .instance_groups
                                .get(&proto_data.prototype_id)
                            {
                                if instances.is_empty() {
                                    continue;
                                }

                                // Write instances for this prototype to the instance buffer
                                self.gpu.queue.write_buffer(
                                    &self.instance_buffer,
                                    buffer_offset,
                                    bytemuck::cast_slice(instances),
                                );

                                // Set buffers and draw
                                render_pass
                                    .set_vertex_buffer(0, proto_data.vertex_buffer.slice(..));
                                render_pass.set_vertex_buffer(
                                    1,
                                    self.instance_buffer.slice(buffer_offset..),
                                );
                                render_pass.set_index_buffer(
                                    proto_data.index_buffer.slice(..),
                                    wgpu::IndexFormat::Uint32,
                                );
                                render_pass.draw_indexed(
                                    0..proto_data.num_indices,
                                    0,
                                    0..instances.len() as u32,
                                );

                                total_instances_drawn += instances.len() as u32;
                                buffer_offset +=
                                    (instances.len() * std::mem::size_of::<InstanceData>()) as u64;
                            }
                        }

                        log::trace!(
                            "Multi-draw: {} prototypes, {} total instances",
                            self.multi_draw.prototype_gpu_data.len(),
                            total_instances_drawn
                        );
                    } else {
                        // Single-draw: use combined vertex/index buffers
                        log::trace!(
                            "Drawing {} indices x {} near instances + {} LOD box instances (of {} total)",
                            self.num_indices,
                            self.culling.visible_count,
                            self.culling.lod_box_count,
                            self.num_instances
                        );
                        render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
                        render_pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
                        render_pass.set_index_buffer(
                            self.index_buffer.slice(..),
                            wgpu::IndexFormat::Uint32,
                        );
                        render_pass.draw_indexed(
                            0..self.num_indices,
                            0,
                            0..self.culling.visible_count,
                        );
                    }

                    // Draw far instances as LOD box proxies (from same buffer, offset by near count)
                    if self.culling.lod_box_count > 0 {
                        let far_byte_offset = (self.culling.visible_count as usize
                            * std::mem::size_of::<InstanceData>())
                            as u64;
                        render_pass
                            .set_vertex_buffer(0, self.culling.lod_box_vertex_buffer().slice(..));
                        render_pass
                            .set_vertex_buffer(1, self.instance_buffer.slice(far_byte_offset..));
                        render_pass.set_index_buffer(
                            self.culling.lod_box_index_buffer().slice(..),
                            wgpu::IndexFormat::Uint32,
                        );
                        render_pass.draw_indexed(
                            0..self.culling.lod_box_num_indices(),
                            0,
                            0..self.culling.lod_box_count,
                        );
                    }

                    // Render point preview after geometry (transparent, reads depth)
                    self.point_preview
                        .render(&mut render_pass, &self.cam.camera_bind_group);

                    // Render curve/points preview (lines, reads depth)
                    self.curve_preview
                        .render(&mut render_pass, &self.cam.camera_bind_group);

                    // Render ground grid after opaque geometry (transparent, reads depth)
                    if self.show_grid {
                        self.grid
                            .render(&mut render_pass, &self.cam.camera_bind_group);
                    }
                }

                // Wireframe selection overlay (selected instance only)
                if let Some(sel_idx) = self.selection.selected_instance_index {
                    if let Some(proto_id) = self.scene.instances.prototype_ids.get(sel_idx) {
                        if let Some(proto_gpu) = self.multi_draw.prototype_gpu_data.get(*proto_id) {
                            // Build single-instance data for selected instance
                            let transform = self
                                .scene
                                .instances
                                .current
                                .get(sel_idx)
                                .copied()
                                .unwrap_or(bif_math::Mat4::IDENTITY);
                            let mat_id = self
                                .scene
                                .instances
                                .material_ids
                                .get(sel_idx)
                                .copied()
                                .unwrap_or(0);
                            let sel_instance = InstanceData {
                                model_matrix: transform.to_cols_array_2d(),
                                material_id: mat_id,
                                tri_mat_offset: 0,
                            };
                            self.gpu.queue.write_buffer(
                                &self.instance_buffer,
                                0,
                                bytemuck::cast_slice(&[sel_instance]),
                            );

                            let mut wf_pass =
                                encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                                    label: Some("Wireframe Selection Pass"),
                                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                        view: &view,
                                        resolve_target: None,
                                        ops: wgpu::Operations {
                                            load: wgpu::LoadOp::Load,
                                            store: wgpu::StoreOp::Store,
                                        },
                                    })],
                                    depth_stencil_attachment: Some(
                                        wgpu::RenderPassDepthStencilAttachment {
                                            view: &self.depth_view,
                                            depth_ops: Some(wgpu::Operations {
                                                load: wgpu::LoadOp::Load,
                                                store: wgpu::StoreOp::Store,
                                            }),
                                            stencil_ops: None,
                                        },
                                    ),
                                    timestamp_writes: None,
                                    occlusion_query_set: None,
                                });

                            let (vp_x, vp_y, vp_w, vp_h) = self.viewport_rect();
                            let (sx, sy, sw, sh) = self.viewport_scissor();
                            wf_pass.set_viewport(vp_x, vp_y, vp_w, vp_h, 0.0, 1.0);
                            wf_pass.set_scissor_rect(sx, sy, sw, sh);
                            wf_pass.set_pipeline(&self.wireframe_pipeline);
                            wf_pass.set_bind_group(0, &self.wireframe_cam_bind_group, &[]);
                            wf_pass.set_bind_group(1, &self.outline_params_bind_group, &[]);
                            wf_pass.set_vertex_buffer(0, proto_gpu.vertex_buffer.slice(..));
                            wf_pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
                            wf_pass.set_index_buffer(
                                proto_gpu.index_buffer.slice(..),
                                wgpu::IndexFormat::Uint32,
                            );
                            wf_pass.draw_indexed(0..proto_gpu.num_indices, 0, 0..1);
                        }
                    }
                }

                // Render gnomon in bottom-right corner
                {
                    let mut gnomon_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("Gnomon Render Pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Load, // Keep existing content
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: None, // No depth testing for gnomon
                        timestamp_writes: None,
                        occlusion_query_set: None,
                    });

                    // Set viewport to bottom-right corner of the active viewport
                    let gnomon_size = self.gnomon.size as f32;
                    let padding = 16.0;
                    let (vp_x, vp_y, vp_w, vp_h) = self.viewport_rect();

                    if vp_w >= gnomon_size + padding && vp_h >= gnomon_size + padding {
                        let x = (vp_x + vp_w - gnomon_size - padding).max(vp_x + padding);
                        let y = (vp_y + vp_h - gnomon_size - padding).max(vp_y + padding);

                        gnomon_pass.set_viewport(
                            x,           // x (right side)
                            y,           // y (bottom, wgpu uses top-left origin)
                            gnomon_size, // width
                            gnomon_size, // height
                            0.0,         // min_depth
                            1.0,         // max_depth
                        );

                        self.gnomon.render(&mut gnomon_pass);
                    }
                }
            }
            RenderMode::Ivar => {
                // 1. Camera dirty → interaction mode at lowest scale
                //    Throttle restarts to ~50ms so rayon can complete some buckets.
                if self.ivar.ivar_state.check_camera_dirty(&self.cam.camera) {
                    let scale = self.ivar.ivar_state.interaction_scale();
                    if self.ivar.ivar_state.should_restart() {
                        if self.ivar.ivar_state.world.is_some() {
                            self.restart_ivar_at_scale(scale);
                        } else {
                            // No BVH yet, record desired scale for when build completes
                            self.ivar.ivar_state.current_scale = scale;
                            self.start_ivar_render();
                        }
                    }
                    self.ivar.ivar_state.last_interaction_time = Some(std::time::Instant::now());
                }

                // 2. Poll for scene build completion
                self.poll_scene_build();

                // 2b. Poll for async denoise completion
                self.poll_denoise_result();

                // 3. Poll for completed buckets / pass completion (before settle timer
                //    so is_pass_in_flight() reflects actual state)
                self.poll_ivar_messages();

                // 4. Settle timer → refine to next resolution level
                if let Some(last_time) = self.ivar.ivar_state.last_interaction_time {
                    let elapsed_ms = last_time.elapsed().as_millis() as u32;
                    // Use shorter timeout for coarse→medium refinement
                    let threshold = if self.ivar.ivar_state.current_scale >= 4 {
                        self.ivar.ivar_state.settle_timeout_ms / 2
                    } else {
                        self.ivar.ivar_state.settle_timeout_ms
                    };
                    if elapsed_ms >= threshold
                        && self.ivar.ivar_state.current_scale > 1
                        && !self.ivar.ivar_state.is_pass_in_flight()
                    {
                        let next_scale = (self.ivar.ivar_state.current_scale / 2).max(1);
                        self.restart_ivar_at_scale(next_scale);
                        self.ivar.ivar_state.last_interaction_time =
                            Some(std::time::Instant::now());
                    }
                }

                // 5. Full-res progressive: start next pass only at scale==1
                if self.ivar.ivar_state.current_scale == 1
                    && self.ivar.ivar_state.world.is_some()
                    && !self.ivar.ivar_state.is_pass_in_flight()
                    && self.ivar.ivar_state.needs_more_passes()
                {
                    self.start_progressive_pass();
                }

                // 6. First-time scene build
                if self.ivar.ivar_state.world.is_none()
                    && self.ivar.ivar_state.build_status == BuildStatus::NotStarted
                {
                    self.ivar.ivar_state.current_scale = 1;
                    self.ivar.ivar_state.last_interaction_time = None;
                    self.start_ivar_render();
                }

                // 7. Upload current image buffer to texture (uses selected AOV channel)
                self.upload_ivar_pixels();

                // Render fullscreen quad with Ivar texture
                {
                    let mut ivar_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("Ivar Render Pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(clear_color),
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                    });

                    let (vp_x, vp_y, vp_w, vp_h) = self.viewport_rect();
                    let (sx, sy, sw, sh) = self.viewport_scissor();
                    ivar_pass.set_viewport(vp_x, vp_y, vp_w, vp_h, 0.0, 1.0);
                    ivar_pass.set_scissor_rect(sx, sy, sw, sh);
                    ivar_pass.set_pipeline(&self.ivar.ivar_pipeline);
                    ivar_pass.set_bind_group(0, &self.ivar.ivar_bind_group, &[]);
                    ivar_pass.draw(0..3, 0..1); // Single fullscreen triangle
                }
            }
        }

        self.gpu.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }
}
