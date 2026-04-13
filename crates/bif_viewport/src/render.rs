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
use crate::ivar_state::{BatchRenderStatus, BuildStatus, CameraSource, RenderMode};
use crate::node_graph::ops::collect_upstream_nodes;
use crate::node_graph::{render_node_graph, GraphNodeId, NodeGraphEvent, SceneNode};
use crate::property_inspector::{render_property_inspector, reset_property_inspector_cache};
use crate::scene_browser::{
    self, CompositeProvider, NodeFilteredProvider, PrimDataProvider, SceneBrowserViewMode,
};
use crate::theme;
use crate::Renderer;
use anyhow::Result;

/// Open a USD file dialog and load into the first UsdRead node in the graph.
fn open_usd_file_dialog(
    snarl: &mut egui_snarl::Snarl<SceneNode>,
    event_bus: &mut crate::app_event::EventBus,
) {
    let Some(path) = rfd::FileDialog::new()
        .add_filter("USD Files", &["usd", "usda", "usdc", "usdz"])
        .pick_file()
    else {
        return;
    };
    let Some(nid) = snarl
        .node_ids()
        .find(|(_, node)| matches!(node, SceneNode::UsdRead { .. }))
        .map(|(id, _)| id)
    else {
        return;
    };
    let path_str = path.display().to_string();
    if let SceneNode::UsdRead {
        file_path,
        is_loaded,
        error,
    } = &mut snarl[nid]
    {
        *file_path = path_str.clone();
        *is_loaded = false;
        *error = None;
    }
    event_bus.emit(crate::app_event::AppEvent::NodeGraph(vec![
        NodeGraphEvent::LoadUsdFile {
            path: path_str,
            node_id: crate::node_graph::GraphNodeId::from(nid),
        },
    ]));
}

impl Renderer {
    /// Render a frame with the given clear color.
    pub fn render(
        &mut self,
        clear_color: wgpu::Color,
        window: &winit::window::Window,
    ) -> Result<()> {
        self.poll_async_work();
        self.poll_environment();
        let full_output = self.run_egui_frame(window);
        self.dispatch_events();
        self.submit_gpu_frame(clear_color, window, full_output)?;
        Ok(())
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
            // Reset transform edit cache when selection changes
            reset_property_inspector_cache(&self.egui_ctx);
        }

        // Update frustum culling before rendering (in Vulkan mode)
        if self.ivar.ivar_state.mode == RenderMode::Vulkan {
            self.update_visible_instances();
        }
    }

    /// Phase 3: Run the egui frame — build all UI panels and return FullOutput.
    fn run_egui_frame(&mut self, window: &winit::window::Window) -> egui::FullOutput {
        // Prepare egui UI
        let raw_input = self.egui_state.take_egui_input(window);

        // Build UI - need to split borrow to avoid closure borrowing entire self
        let mut left_panel_width = self.ui_layout.left_panel_width;
        let mut right_panel_width = self.ui_layout.right_panel_width;
        let mut top_panel_height = self.ui_layout.top_panel_height;
        let mut bottom_panel_height = self.ui_layout.bottom_panel_height;

        // Ivar state for UI
        let mut render_mode = self.ivar.ivar_state.mode;
        let ivar_buckets_completed = self.ivar.ivar_state.buckets_completed;
        let ivar_total_buckets = self.ivar.ivar_state.buckets.len();
        let ivar_elapsed = self.ivar.ivar_state.elapsed_secs();
        let ivar_render_complete = self.ivar.ivar_state.render_complete;
        let ivar_accumulated_spp = self.ivar.ivar_state.accumulated_samples;
        let mut ivar_target_spp = self.ivar.ivar_state.target_spp;
        let ivar_current_scale = self.ivar.ivar_state.current_scale;
        let mut ivar_nav_quality = self.ivar.ivar_state.interaction_quality;
        let mut lod_max_polys = self.culling.lod_max_polys;
        let mut display_settings = self.display_settings.clone();

        // Take event bus out of self so it can be passed into the closure
        // without conflicting with `self` borrows. Restored after the closure.
        let mut event_bus = std::mem::take(&mut self.event_bus);
        let mut gizmo_hovered: u8 = 0;

        let full_output = self.egui_ctx.run(raw_input, |ctx| {
            // Keyboard shortcuts
            if ctx.input(|i| i.key_pressed(egui::Key::N) && i.modifiers.command) {
                event_bus.emit(crate::app_event::AppEvent::ProjectNew);
            }
            if ctx
                .input(|i| i.key_pressed(egui::Key::O) && i.modifiers.command && !i.modifiers.shift)
            {
                event_bus.emit(crate::app_event::AppEvent::ProjectOpen);
            }
            if ctx
                .input(|i| i.key_pressed(egui::Key::S) && i.modifiers.command && !i.modifiers.shift)
            {
                event_bus.emit(crate::app_event::AppEvent::ProjectSave);
            }
            if ctx
                .input(|i| i.key_pressed(egui::Key::S) && i.modifiers.command && i.modifiers.shift)
            {
                event_bus.emit(crate::app_event::AppEvent::ProjectSaveAs);
            }
            // F to frame selected instance
            if ctx.input(|i| i.key_pressed(egui::Key::F) && !i.modifiers.command) {
                event_bus.emit(crate::app_event::AppEvent::FrameSelected);
            }

            let top_panel = egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
                egui::menu::bar(ui, |ui| {
                    // File menu
                    ui.menu_button("File", |ui| {
                        if ui
                            .add(egui::Button::new("New").shortcut_text("Ctrl+N"))
                            .clicked()
                        {
                            event_bus.emit(crate::app_event::AppEvent::ProjectNew);
                            ui.close_menu();
                        }
                        if ui
                            .add(egui::Button::new("Open...").shortcut_text("Ctrl+O"))
                            .clicked()
                        {
                            event_bus.emit(crate::app_event::AppEvent::ProjectOpen);
                            ui.close_menu();
                        }
                        if ui
                            .add(egui::Button::new("Save").shortcut_text("Ctrl+S"))
                            .clicked()
                        {
                            event_bus.emit(crate::app_event::AppEvent::ProjectSave);
                            ui.close_menu();
                        }
                        if ui
                            .add(egui::Button::new("Save As...").shortcut_text("Ctrl+Shift+S"))
                            .clicked()
                        {
                            event_bus.emit(crate::app_event::AppEvent::ProjectSaveAs);
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.button("Open USD...").clicked() {
                            open_usd_file_dialog(
                                &mut self.nodes.node_graph_state.snarl,
                                &mut event_bus,
                            );
                            ui.close_menu();
                        }
                        if ui.button("Export Edits...").clicked() {
                            if let Some(path) = rfd::FileDialog::new()
                                .add_filter("USD Files", &["usda", "usdc"])
                                .set_file_name("edits.usda")
                                .save_file()
                            {
                                event_bus.emit(crate::app_event::AppEvent::ExportEditLayer(path));
                            }
                            ui.close_menu();
                        }
                        if !self.recent_files.paths.is_empty() {
                            ui.separator();
                            ui.menu_button("Recent Files", |ui| {
                                for path in &self.recent_files.paths {
                                    let label = path
                                        .file_name()
                                        .map(|n| n.to_string_lossy().into_owned())
                                        .unwrap_or_else(|| path.display().to_string());
                                    if ui
                                        .button(&label)
                                        .on_hover_text(path.display().to_string())
                                        .clicked()
                                    {
                                        event_bus.emit(
                                            crate::app_event::AppEvent::ProjectOpenRecent(
                                                path.clone(),
                                            ),
                                        );
                                        ui.close_menu();
                                    }
                                }
                            });
                        }
                        ui.separator();
                        if ui.button("Quit").clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });

                    // View menu
                    ui.menu_button("View", |ui| {
                        ui.checkbox(&mut self.show_grid, "Grid")
                            .on_hover_text("Show ground grid");
                        ui.checkbox(&mut self.point_preview.visible, "Points")
                            .on_hover_text("Show scatter point preview");
                    });

                    // Render menu
                    ui.menu_button("Render", |ui| {
                        if ui.button("Vulkan Preview").clicked() {
                            render_mode = RenderMode::Vulkan;
                            ui.close_menu();
                        }
                        if ui.button("Ivar Render").clicked() {
                            render_mode = RenderMode::Ivar;
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.button("Rebuild Scene").clicked() {
                            event_bus.emit(crate::app_event::AppEvent::RebuildScene);
                            ui.close_menu();
                        }
                    });

                    ui.separator();

                    // Status area: USD load status
                    if let crate::UsdLoadStatus::Loading(ref progress) =
                        self.async_channels.usd_load_status
                    {
                        ui.spinner();
                        ui.label(progress.to_string());
                        ui.separator();
                    } else if let crate::UsdLoadStatus::Error(ref msg) =
                        self.async_channels.usd_load_status
                    {
                        ui.colored_label(theme::STATUS_ERROR, msg);
                        ui.separator();
                    }

                    // Stage metadata and correction toggles
                    if let Some(ref meta) = self.scene.working_scene.stage_metadata {
                        ui.colored_label(theme::TEXT_SECONDARY, format!("[{}]", meta));

                        let needs_axis = meta.up_axis == bif_core::usd::cpp_bridge::UpAxis::Z;
                        let needs_scale = (meta.meters_per_unit - 1.0).abs() > 1e-6;

                        if needs_axis
                            && ui
                                .checkbox(&mut self.apply_axis_correction, "Z\u{2192}Y")
                                .on_hover_text("Rotate scene from Z-up to Y-up")
                                .changed()
                        {
                            event_bus.emit(crate::app_event::AppEvent::StageCorrectionsChanged);
                        }
                        if needs_scale
                            && ui
                                .checkbox(&mut self.apply_unit_scaling, "\u{2192}m")
                                .on_hover_text(format!(
                                    "Scale from {:.4} to meters",
                                    meta.meters_per_unit
                                ))
                                .changed()
                        {
                            event_bus.emit(crate::app_event::AppEvent::StageCorrectionsChanged);
                        }
                    }
                });
            });
            top_panel_height = top_panel.response.rect.height();

            let stats_panel = egui::SidePanel::left("left_panel")
                .default_width(300.0)
                .show(ctx, |ui| {
                    // Scene Browser (always visible, top section)
                    // Tab bar: Scene | Node (when a node is selected)
                    let selected_node = self.nodes.node_graph_state.selected_node;
                    let view_mode = &mut self.selection.scene_browser_state.view_mode;
                    ui.horizontal(|ui| {
                        if ui
                            .selectable_label(
                                *view_mode == SceneBrowserViewMode::FullScene,
                                "Scene",
                            )
                            .clicked()
                        {
                            *view_mode = SceneBrowserViewMode::FullScene;
                        }
                        if let Some(node_id) = selected_node {
                            let node_name = self.nodes.node_graph_state.snarl
                                [egui_snarl::NodeId::from(node_id)]
                            .name();
                            let is_node_mode =
                                matches!(*view_mode, SceneBrowserViewMode::NodeContribution(_));
                            if ui
                                .selectable_label(is_node_mode, format!("Node: {}", node_name))
                                .clicked()
                            {
                                *view_mode = SceneBrowserViewMode::NodeContribution(node_id);
                            }
                            // Auto-update if already in node mode and selection changed
                            if let SceneBrowserViewMode::NodeContribution(prev) = *view_mode {
                                if prev != node_id {
                                    *view_mode = SceneBrowserViewMode::NodeContribution(node_id);
                                }
                            }
                        } else if matches!(*view_mode, SceneBrowserViewMode::NodeContribution(_)) {
                            // No node selected — fall back to full scene
                            *view_mode = SceneBrowserViewMode::FullScene;
                        }
                    });

                    // Give scene browser ~60% of panel height
                    let available = ui.available_height();
                    let browser_height = (available * 0.6).max(200.0);

                    let view_mode = self.selection.scene_browser_state.view_mode;
                    egui::ScrollArea::vertical()
                            .id_salt("scene_browser_scroll")
                            .max_height(browser_height)
                            .show(ui, |ui| {
                                let stage_guard = self
                                    .scene
                                    .usd_stage
                                    .as_ref()
                                    .map(|s| s.lock().expect("UsdStage mutex poisoned"));
                                let composite = CompositeProvider::new(
                                    stage_guard.as_deref().map(|s| s as &dyn PrimDataProvider),
                                    &self.nodes.cached_scene_graph,
                                );

                                let highlight = self.nodes.node_graph_state.selected_node;
                                match view_mode {
                                    SceneBrowserViewMode::FullScene => {
                                        let provider: &dyn PrimDataProvider = &composite;
                                        if let Some(new_selection) =
                                            scene_browser::render_scene_browser(
                                                ui,
                                                &mut self.selection.scene_browser_state,
                                                provider,
                                                highlight,
                                            )
                                        {
                                            event_bus.emit(
                                                crate::app_event::AppEvent::PrimSelected(
                                                    new_selection,
                                                ),
                                            );
                                        }
                                    }
                                    SceneBrowserViewMode::NodeContribution(node_id) => {
                                        let snarl_id = egui_snarl::NodeId::from(node_id);
                                        let upstream = collect_upstream_nodes(
                                            snarl_id,
                                            &self.nodes.node_graph_state.snarl,
                                        );
                                        let upstream_gids: std::collections::HashSet<GraphNodeId> =
                                            upstream.into_iter().map(GraphNodeId::from).collect();
                                        let filtered = NodeFilteredProvider::new(
                                            &composite,
                                            &self.nodes.cached_scene_graph,
                                            node_id,
                                            &upstream_gids,
                                        );
                                        let provider: &dyn PrimDataProvider = &filtered;
                                        if let Some(new_selection) =
                                            scene_browser::render_scene_browser(
                                                ui,
                                                &mut self.selection.scene_browser_state,
                                                provider,
                                                Some(node_id),
                                            )
                                        {
                                            event_bus.emit(
                                                crate::app_event::AppEvent::PrimSelected(
                                                    new_selection,
                                                ),
                                            );
                                        }
                                    }
                                }
                            });

                    ui.separator();

                    // Render Settings (bottom section, scrollable)
                    egui::ScrollArea::vertical()
                        .id_salt("render_settings_scroll")
                        .show(ui, |ui| {
                            ui.heading("Render");
                            crate::render_ui::render_stats_panel(
                                ui,
                                &mut event_bus,
                                &mut crate::render_ui::StatsPanelParams {
                                    ivar_state: &mut self.ivar.ivar_state,
                                    usd_stage: &self.scene.usd_stage,
                                    timeline_state: &self.timeline_state,
                                    instance_count: self.scene.instances.transforms.len(),
                                    mesh_triangle_count: self.scene.mesh_data.indices.len() / 3,
                                    edit_override_count: self
                                        .scene
                                        .edit_state
                                        .transform_overrides
                                        .len(),
                                    edit_keyframe_count: self
                                        .scene
                                        .edit_state
                                        .keyframe_overrides
                                        .len(),
                                    ivar_buckets_completed,
                                    ivar_total_buckets,
                                    ivar_elapsed,
                                    ivar_render_complete,
                                    ivar_accumulated_spp,
                                    ivar_current_scale,
                                    render_mode: &mut render_mode,
                                    ivar_target_spp: &mut ivar_target_spp,
                                    ivar_nav_quality: &mut ivar_nav_quality,
                                    lod_max_polys: &mut lod_max_polys,
                                    display_settings: &mut display_settings,
                                },
                            );
                        });
                });
            left_panel_width = stats_panel.response.rect.width();

            // Property Inspector (right panel)
            // Build editable transform for selected viewport instance
            let editable_transform: Option<(usize, bif_core::Transform)> =
                self.selection.selected_instance_index.and_then(|idx| {
                    // Use edit override if present, otherwise decompose from current_transforms
                    let transform =
                        if let Some(t) = self.scene.edit_state.transform_overrides.get(&idx) {
                            *t
                        } else if idx < self.scene.instances.current.len() {
                            bif_core::Transform::from_matrix(self.scene.instances.current[idx])
                        } else {
                            return None;
                        };
                    Some((idx, transform))
                });

            let property_panel = egui::SidePanel::right("property_panel")
                .default_width(280.0)
                .show(ctx, |ui| {
                    // Show selected node properties in inspector
                    if let Some(selected_nid) = self.nodes.node_graph_state.selected_node {
                        let snarl_id: egui_snarl::NodeId = selected_nid.into();
                        let node = &mut self.nodes.node_graph_state.snarl[snarl_id];
                        let node_events = crate::property_inspector::render_node_properties(
                            ui,
                            node,
                            selected_nid,
                        );
                        // Track xform property changes for live update
                        let has_xform_change = node_events
                            .iter()
                            .any(|e| matches!(e, NodeGraphEvent::XformChanged { .. }));
                        if has_xform_change {
                            self.nodes.xform_property_changed = Some(selected_nid);
                        }
                        if !node_events.is_empty() {
                            event_bus.emit(crate::app_event::AppEvent::NodeGraph(node_events));
                        }
                        ui.separator();
                    }

                    let et_ref = editable_transform.as_ref().map(|(idx, t)| (*idx, t));
                    render_property_inspector(
                        ui,
                        &mut event_bus,
                        self.selection.selected_prim_properties.as_ref(),
                        et_ref,
                    );

                    // Point cloud summary
                    if !self.scene.working_scene.point_clouds.is_empty() {
                        ui.separator();
                        ui.heading("Point Clouds");
                        let total_points: usize = self
                            .scene
                            .working_scene
                            .point_clouds
                            .iter()
                            .map(|c| c.point_count())
                            .sum();
                        ui.label(format!(
                            "{} clouds, {} total points",
                            self.scene.working_scene.point_clouds.len(),
                            total_points
                        ));
                        for cloud in &self.scene.working_scene.point_clouds {
                            ui.label(format!("  {} - {} pts", cloud.name, cloud.point_count()));
                        }
                    }
                });
            right_panel_width = property_panel.response.rect.width();

            // Timeline panel (always visible, like Houdini/Maya/Blender)
            let timeline_panel = egui::TopBottomPanel::bottom("timeline_panel")
                .exact_height(32.0)
                .show(ctx, |ui| {
                    let has_animation = self.timeline_state.has_range();

                    ui.horizontal_centered(|ui| {
                        // Camera dropdown
                        let cam_display = match &self.cam.viewport_camera_source {
                            CameraSource::SceneCamera(idx) => self
                                .scene
                                .scene_cameras
                                .get(*idx)
                                .map(|c| c.name.as_str())
                                .unwrap_or("Scene Camera"),
                            other => other.display_name(),
                        };
                        egui::ComboBox::from_id_salt("viewport_camera")
                            .selected_text(cam_display)
                            .width(100.0)
                            .show_ui(ui, |ui| {
                                // Perspective viewport option
                                if ui
                                    .selectable_label(
                                        matches!(
                                            self.cam.viewport_camera_source,
                                            CameraSource::Viewport
                                        ),
                                        "Perspective",
                                    )
                                    .clicked()
                                {
                                    self.cam.viewport_camera_source = CameraSource::Viewport;
                                    self.cam.camera_locked = false;
                                    self.cam.selected_usd_camera = None;
                                    event_bus.emit(
                                        crate::app_event::AppEvent::CameraProjectionChange(
                                            crate::app_event::CameraProjection::Perspective,
                                        ),
                                    );
                                }
                                // USD cameras from stage
                                if let Some(ref stage_mtx) = self.scene.usd_stage {
                                    if let Ok(paths) = stage_mtx
                                        .lock()
                                        .expect("UsdStage mutex poisoned")
                                        .camera_paths()
                                    {
                                        for path in paths {
                                            let is_selected = matches!(
                                                &self.cam.viewport_camera_source,
                                                CameraSource::UsdCamera(p) if p == &path
                                            );
                                            if ui.selectable_label(is_selected, &path).clicked() {
                                                self.cam.viewport_camera_source =
                                                    CameraSource::UsdCamera(path.clone());
                                                self.cam.selected_usd_camera = Some(path.clone());
                                                self.cam.camera_locked = true;
                                                event_bus.emit(
                                                    crate::app_event::AppEvent::SyncUsdCamera(path),
                                                );
                                            }
                                        }
                                    }
                                }
                                // Orthographic presets
                                ui.separator();
                                for preset in bif_math::OrthoPreset::all() {
                                    let is_selected = matches!(
                                        &self.cam.viewport_camera_source,
                                        CameraSource::OrthoView(p) if p == preset
                                    );
                                    if ui
                                        .selectable_label(is_selected, preset.display_name())
                                        .clicked()
                                    {
                                        self.cam.viewport_camera_source =
                                            CameraSource::OrthoView(*preset);
                                        self.cam.camera_locked = false;
                                        self.cam.selected_usd_camera = None;
                                        event_bus.emit(
                                            crate::app_event::AppEvent::CameraProjectionChange(
                                                crate::app_event::CameraProjection::Ortho(
                                                    preset.display_name().to_string(),
                                                ),
                                            ),
                                        );
                                    }
                                }
                                // Scene cameras (from Camera primitives)
                                if !self.scene.scene_cameras.is_empty() {
                                    ui.separator();
                                    for (idx, cam) in self.scene.scene_cameras.iter().enumerate() {
                                        let is_selected = matches!(
                                            &self.cam.viewport_camera_source,
                                            CameraSource::SceneCamera(i) if *i == idx
                                        );
                                        if ui.selectable_label(is_selected, &cam.name).clicked() {
                                            self.cam.viewport_camera_source =
                                                CameraSource::SceneCamera(idx);
                                            self.cam.camera_locked = true;
                                            self.cam.selected_usd_camera = None;
                                            event_bus.emit(
                                                crate::app_event::AppEvent::SyncSceneCamera(
                                                    idx as u64,
                                                ),
                                            );
                                        }
                                    }
                                }
                            });

                        // Lock/Unlock toggle (show when USD or scene camera selected)
                        if matches!(
                            self.cam.viewport_camera_source,
                            CameraSource::UsdCamera(_) | CameraSource::SceneCamera(_)
                        ) {
                            let icon = if self.cam.camera_locked {
                                "Lock"
                            } else {
                                "Free"
                            };
                            if ui
                                .button(icon)
                                .on_hover_text(if self.cam.camera_locked {
                                    "Camera locked to USD/scene camera — click to free"
                                } else {
                                    "Camera free — click to lock to USD/scene camera"
                                })
                                .clicked()
                            {
                                self.cam.camera_locked = !self.cam.camera_locked;
                            }
                        }

                        ui.separator();

                        // Play/Pause button (disabled if no animation)
                        ui.add_enabled_ui(has_animation, |ui| {
                            let play_text = if self.timeline_state.is_playing {
                                "⏸"
                            } else {
                                "▶"
                            };
                            let play_tooltip = if self.timeline_state.is_playing {
                                "Pause animation playback"
                            } else {
                                "Play animation"
                            };
                            if ui.button(play_text).on_hover_text(play_tooltip).clicked() {
                                self.timeline_state.toggle_playback();
                            }

                            // Go to start
                            if ui.button("|◀").on_hover_text("Go to first frame").clicked() {
                                self.timeline_state.go_to_start();
                            }
                        });

                        // Frame range display (start)
                        let (start, end) = if has_animation {
                            (
                                self.timeline_state.start_frame as f32,
                                self.timeline_state.end_frame as f32,
                            )
                        } else {
                            (1.0, 100.0)
                        };
                        ui.label(format!("{:.0}", start));

                        // Frame slider with frame numbers shown
                        let mut frame = self.timeline_state.current_frame as f32;
                        let slider = egui::Slider::new(&mut frame, start..=end)
                            .show_value(true)
                            .integer();
                        let slider_resp = ui.add_sized([200.0, 18.0], slider);
                        if slider_resp.changed() && slider_resp.is_pointer_button_down_on() {
                            self.timeline_state.current_frame = frame as f64;
                            self.timeline_state.reset_playback_anchor();
                        }

                        // Draw keyframe diamond markers on the slider
                        if !self.timeline_state.keyframe_times.is_empty() {
                            let slider_rect = slider_resp.rect;
                            let range = end - start;
                            if range > 0.0 {
                                let painter = ui.painter_at(slider_rect);
                                for &time in &self.timeline_state.keyframe_times {
                                    let t = ((time as f32) - start) / range;
                                    let x = slider_rect.left() + t * slider_rect.width();
                                    let y = slider_rect.center().y;
                                    let size = 4.0;
                                    // Diamond shape
                                    let points = vec![
                                        egui::pos2(x, y - size),
                                        egui::pos2(x + size, y),
                                        egui::pos2(x, y + size),
                                        egui::pos2(x - size, y),
                                    ];
                                    painter.add(egui::Shape::convex_polygon(
                                        points,
                                        theme::KEYFRAME_FILL,
                                        egui::Stroke::new(1.0, theme::KEYFRAME_STROKE),
                                    ));
                                }
                            }
                        }

                        // Frame range display (end)
                        ui.label(format!("{:.0}", end));

                        // Go to end (disabled if no animation)
                        ui.add_enabled_ui(has_animation, |ui| {
                            if ui.button("▶|").on_hover_text("Go to last frame").clicked() {
                                self.timeline_state.go_to_end();
                            }
                        });

                        // Loop toggle
                        ui.checkbox(&mut self.timeline_state.loop_playback, "Loop")
                            .on_hover_text("Loop animation playback");

                        // Realtime toggle (wall-clock vs every-frame)
                        if ui
                            .checkbox(&mut self.timeline_state.realtime, "RT")
                            .on_hover_text("Realtime: ON = wall-clock accurate, OFF = every frame")
                            .changed()
                        {
                            self.timeline_state.reset_playback_anchor();
                        }

                        // Integer frame snap toggle
                        ui.checkbox(&mut self.timeline_state.snap_to_frames, "Int")
                            .on_hover_text("Snap playback to integer frames");

                        // FPS display
                        ui.label(format!("@{:.0}fps", self.timeline_state.fps));
                    });
                });
            let timeline_height = timeline_panel.response.rect.height();

            // Node Graph (bottom panel)
            let node_graph_panel = egui::TopBottomPanel::bottom("node_graph_panel")
                .default_height(200.0)
                .resizable(true)
                .show(ctx, |ui| {
                    let events = render_node_graph(
                        ui,
                        &mut self.nodes.node_graph_state,
                        &self.nodes.node_prim_counts,
                    );
                    if !events.is_empty() {
                        event_bus.emit(crate::app_event::AppEvent::NodeGraph(events));
                    }
                });
            bottom_panel_height = node_graph_panel.response.rect.height() + timeline_height;

            // Viewport stats overlay (top-left of viewport area)
            {
                let overlay_offset = egui::pos2(left_panel_width + 8.0, top_panel_height + 8.0);
                egui::Area::new(egui::Id::new("viewport_stats"))
                    .fixed_pos(overlay_offset)
                    .interactable(false)
                    .show(ctx, |ui| {
                        egui::Frame::none()
                            .fill(theme::BG_OVERLAY_BACKDROP)
                            .inner_margin(egui::Margin::same(6.0))
                            .rounding(4.0)
                            .show(ui, |ui| {
                                ui.style_mut().spacing.item_spacing.y = 1.0;
                                ui.colored_label(
                                    theme::TEXT_SECONDARY,
                                    format!("{:.0} fps", self.fps),
                                );
                                ui.colored_label(
                                    theme::TEXT_SECONDARY,
                                    format!(
                                        "{}/{} instances",
                                        self.culling.visible_count + self.culling.lod_box_count,
                                        self.num_instances
                                    ),
                                );
                                let full_tris = self.culling.triangles_per_instance as u64
                                    * self.culling.visible_count as u64;
                                let box_tris = 12u64 * self.culling.lod_box_count as u64;
                                let total_tris = full_tris + box_tris;
                                let tris_str = if total_tris > 1_000_000 {
                                    format!("{:.1}M tris", total_tris as f64 / 1_000_000.0)
                                } else if total_tris > 1_000 {
                                    format!("{:.1}K tris", total_tris as f64 / 1_000.0)
                                } else {
                                    format!("{} tris", total_tris)
                                };
                                ui.colored_label(theme::TEXT_SECONDARY, tris_str);
                                // Show render SPP when in Ivar mode
                                if self.ivar.ivar_state.mode == crate::ivar_state::RenderMode::Ivar
                                    && self.ivar.ivar_state.accumulated_samples > 0
                                {
                                    ui.colored_label(
                                        theme::TEXT_SECONDARY,
                                        format!(
                                            "{}/{} spp",
                                            self.ivar.ivar_state.accumulated_samples,
                                            self.ivar.ivar_state.target_spp
                                        ),
                                    );
                                }
                            });
                    });
            }

            // Draw translate gizmo overlay (after all panels, on foreground layer)
            if let Some(sel_idx) = self.selection.selected_instance_index {
                if sel_idx < self.scene.instances.current.len() {
                    let transform =
                        if let Some(t) = self.scene.edit_state.transform_overrides.get(&sel_idx) {
                            *t
                        } else {
                            bif_core::Transform::from_matrix(self.scene.instances.current[sel_idx])
                        };
                    let world_pos = transform.translation;
                    let vp_rect = (
                        left_panel_width,
                        top_panel_height,
                        (self.size.0 as f32 - left_panel_width - right_panel_width).max(1.0),
                        (self.size.1 as f32 - top_panel_height - bottom_panel_height).max(1.0),
                    );

                    let painter = ctx.layer_painter(egui::LayerId::new(
                        egui::Order::Foreground,
                        egui::Id::new("gizmo_layer"),
                    ));

                    let mouse_screen = ctx.input(|i| i.pointer.hover_pos().map(|p| (p.x, p.y)));

                    let hovered = crate::gizmo::draw_gizmo(
                        &painter,
                        &self.cam.camera,
                        world_pos,
                        vp_rect,
                        &self.selection.gizmo_state,
                        mouse_screen,
                    );

                    // Store hovered axis — read back after egui closure ends
                    gizmo_hovered = hovered as u8;
                }
            }
        });

        // Update gizmo hovered axis from egui frame
        if !self.selection.gizmo_state.is_dragging {
            self.selection.gizmo_state.hovered_axis =
                crate::gizmo::GizmoAxis::from_u8(gizmo_hovered);
        }

        // Update target SPP from UI (may resume rendering if increased)
        if ivar_target_spp != self.ivar.ivar_state.target_spp {
            self.ivar.ivar_state.target_spp = ivar_target_spp;
            if ivar_target_spp > self.ivar.ivar_state.accumulated_samples {
                self.ivar.ivar_state.render_complete = false;
            }
        }
        // Update interaction quality from UI slider
        self.ivar.ivar_state.interaction_quality = ivar_nav_quality;
        self.ui_layout.left_panel_width = left_panel_width;
        self.ui_layout.right_panel_width = right_panel_width;
        self.ui_layout.top_panel_height = top_panel_height;
        self.ui_layout.bottom_panel_height = bottom_panel_height;

        // Update camera aspect to match viewport (not full window)
        let (_, _, vp_w, vp_h) = self.viewport_rect();
        let new_aspect = vp_w / vp_h;
        if (self.cam.camera.aspect - new_aspect).abs() > 0.001 {
            self.cam.camera.set_aspect(new_aspect);
            self.update_camera();
        }

        // Update LOD max polys and display settings from UI
        self.culling.lod_max_polys = lod_max_polys;
        let purpose_changed = self.display_settings.purpose_mode != display_settings.purpose_mode;
        self.display_settings = display_settings;

        // Purpose mode changed — full rebuild (combined mesh must be re-baked for Ivar).
        if purpose_changed {
            log::info!(
                "Purpose mode changed to {:?}",
                self.display_settings.purpose_mode
            );
            if let Err(e) = self.reload_working_scene() {
                log::error!("Failed to reload after purpose change: {}", e);
            }
        }

        // Write render mode back; detect mode change via event bus
        let mode_changed = self.ivar.ivar_state.mode != render_mode;
        self.ivar.ivar_state.mode = render_mode;
        if mode_changed {
            event_bus.emit(crate::app_event::AppEvent::RenderModeChanged);
        }

        // Restore event bus to self
        self.event_bus = event_bus;

        full_output
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

                // Layer-aware stage (v0.14.0) — panels land in subsequent
                // Phase D commits. For now we log so emitters can be wired
                // in advance; handlers fill in as the UI comes online.
                AppEvent::LayerSelected(idx) => {
                    log::debug!("LayerSelected({idx}) — panel not yet wired");
                }
                AppEvent::LayerMuteToggled { index, muted } => {
                    log::debug!(
                        "LayerMuteToggled(index={index}, muted={muted}) — panel not yet wired"
                    );
                }
                AppEvent::WorkingLayerChanged(idx) => {
                    log::debug!("WorkingLayerChanged({idx}) — panel not yet wired");
                }
                AppEvent::PayloadPolicyChanged(policy) => {
                    log::debug!("PayloadPolicyChanged({policy:?}) — panel not yet wired");
                }
                AppEvent::IsolationModeToggled(on) => {
                    log::debug!("IsolationModeToggled({on}) — panel not yet wired");
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

    /// Phase 5: Tessellate egui, submit GPU render passes, present frame.
    fn submit_gpu_frame(
        &mut self,
        clear_color: wgpu::Color,
        window: &winit::window::Window,
        full_output: egui::FullOutput,
    ) -> Result<()> {
        let output = self.gpu.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        self.egui_state
            .handle_platform_output(window, full_output.platform_output);

        let screen_descriptor = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [self.size.0, self.size.1],
            pixels_per_point: window.scale_factor() as f32,
        };

        let paint_jobs = self
            .egui_ctx
            .tessellate(full_output.shapes, full_output.pixels_per_point);

        let mut encoder = self
            .gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        // Upload egui textures
        for (id, image_delta) in &full_output.textures_delta.set {
            self.egui_renderer
                .update_texture(&self.gpu.device, &self.gpu.queue, *id, image_delta);
        }

        // Prepare egui render pass
        self.egui_renderer.update_buffers(
            &self.gpu.device,
            &self.gpu.queue,
            &mut encoder,
            &paint_jobs,
            &screen_descriptor,
        );

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
                            wf_pass.set_bind_group(1, &self.materials.bind_group, &[]);
                            wf_pass.set_bind_group(2, &self.textures.bind_group, &[]);
                            wf_pass.set_bind_group(3, self.environment.bind_group(), &[]);
                            wf_pass.set_bind_group(4, &self.lights.bind_group, &[]);
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

        // Render egui on top
        {
            let mut egui_pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("egui Render Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                })
                .forget_lifetime(); // Need 'static lifetime for egui renderer

            self.egui_renderer
                .render(&mut egui_pass, &paint_jobs, &screen_descriptor);
        }

        // Free egui textures
        for id in &full_output.textures_delta.free {
            self.egui_renderer.free_texture(id);
        }

        self.gpu.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }
}
