use anyhow::Result;
use std::sync::atomic::Ordering;

use crate::batch_render::BatchMessage;
use crate::environment_manager::IblResult;
use crate::gpu_types::InstanceData;
use crate::ivar_state::{self, BatchRenderStatus, BuildStatus, CameraSource, RenderMode};
use crate::node_graph::{render_node_graph, NodeGraphEvent, SceneNode};
use crate::property_inspector::{
    render_property_inspector, reset_transform_edit_cache, PrimProperties, TransformEdit,
};
use crate::scene_browser::{self, CompositeProvider, PrimDataProvider, ProceduralPrimKind};
use crate::Renderer;

impl Renderer {
    /// Render a frame with the given clear color
    pub fn render(
        &mut self,
        clear_color: wgpu::Color,
        window: &winit::window::Window,
    ) -> Result<()> {
        // Poll async USD load (non-blocking)
        self.poll_usd_load();

        // Rebuild cached scene graph if dirty
        if self.scene_graph_dirty {
            self.cached_scene_graph = scene_browser::build_scene_graph_cache(&self.working_scene);
            self.scene_graph_dirty = false;
        }

        // Process pending scene operations from undo/redo
        if !self.edit_state.pending_scene_ops.is_empty() {
            let ops: Vec<_> = self.edit_state.pending_scene_ops.drain(..).collect();
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
                        if self.working_scene.remove_prototype(proto_id) {
                            needs_reload = true;
                        }
                    }
                    bif_core::SceneOp::AddPointCloud { cloud } => {
                        self.working_scene.add_point_cloud(*cloud);

                        // Update point preview
                        let all_positions: Vec<bif_math::Vec3> = self
                            .working_scene
                            .point_clouds
                            .iter()
                            .flat_map(|c| c.positions.iter().copied())
                            .collect();
                        self.point_preview
                            .upload_points(&self.device, &self.queue, &all_positions);
                        self.point_preview_params_dirty = true;
                        self.point_preview.visible = true;
                    }
                    bif_core::SceneOp::RemovePointCloud { cloud_id } => {
                        if self.working_scene.remove_point_cloud(cloud_id) {
                            // Update point preview
                            let all_positions: Vec<bif_math::Vec3> = self
                                .working_scene
                                .point_clouds
                                .iter()
                                .flat_map(|c| c.positions.iter().copied())
                                .collect();
                            self.point_preview.upload_points(
                                &self.device,
                                &self.queue,
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

        // Poll for completed async IBL generation
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
                        &self.device,
                        &self.queue,
                        &hdr_pixels,
                        hdr_width,
                        hdr_height,
                        rotation_rad,
                        intensity,
                        show_background,
                    );
                    // Set Ivar CPU environment + initial params
                    self.ivar_state.environment = Some(ivar_env);
                    self.ivar_state.hdri_rotation = rotation_rad;
                    self.ivar_state.hdri_intensity = intensity;
                    self.node_graph_state.mark_hdri_loaded(
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
                    self.node_graph_state.mark_hdri_error(&source_path, message);
                    log::error!("HDRI load failed: {}", load_path);
                }
            }
        }

        // Poll for completed .tx conversion
        if let Some(status) = self.environment.poll_tx_result() {
            self.node_graph_state.mark_tx_conversion_complete(status);
        }

        // Poll for batch render messages
        let mut clear_batch_state = false;
        if let Some(ref rx) = self.batch_receiver {
            while let Ok(msg) = rx.try_recv() {
                match msg {
                    BatchMessage::Progress {
                        current_frame,
                        total_frames,
                        frame_progress,
                    } => {
                        self.ivar_state.batch_status = BatchRenderStatus::Rendering {
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
                        self.ivar_state.batch_status =
                            BatchRenderStatus::Complete { total_elapsed_secs };
                        clear_batch_state = true;
                    }
                    BatchMessage::Cancelled => {
                        log::info!("Batch render cancelled");
                        self.ivar_state.batch_status = BatchRenderStatus::Cancelled;
                        clear_batch_state = true;
                    }
                    BatchMessage::Error(e) => {
                        log::error!("Batch render error: {}", e);
                        self.ivar_state.batch_status = BatchRenderStatus::Failed(e);
                        clear_batch_state = true;
                    }
                }
            }
        }
        if clear_batch_state {
            self.batch_receiver = None;
            self.batch_cancel_flag = None;
        }

        // Sync selection highlight to GPU camera uniform
        let sel_id = self
            .selected_instance_index
            .map(|i| i as u32)
            .unwrap_or(crate::gpu_types::NO_SELECTION);
        if self.camera_uniform.selected_instance_id != sel_id {
            self.camera_uniform.selected_instance_id = sel_id;
            self.queue.write_buffer(
                &self.camera_buffer,
                0,
                bytemuck::cast_slice(&[self.camera_uniform]),
            );
            // Reset transform edit cache when selection changes
            reset_transform_edit_cache(&self.egui_ctx);
        }

        // Update frustum culling before rendering (in Vulkan mode)
        if self.ivar_state.mode == RenderMode::Vulkan {
            self.update_visible_instances();
        }

        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        // Prepare egui UI
        let raw_input = self.egui_state.take_egui_input(window);

        // Build UI - need to split borrow to avoid closure borrowing entire self
        let show_ui = self.show_ui;
        let fps = self.fps;
        let camera = &self.camera;
        let num_instances = self.num_instances;
        let visible_instances = self.culling.visible_count;
        let lod_box_instances = self.culling.lod_box_count;
        let triangles_per_instance = self.culling.triangles_per_instance;
        let mesh_bounds_min = self.mesh_bounds_min;
        let mesh_bounds_max = self.mesh_bounds_max;
        let size = self.size;
        let mut gnomon_size = self.gnomon.size;
        let mut lod_max_polys = self.culling.lod_max_polys;
        let mut left_panel_width = self.ui_left_panel_width;
        let mut right_panel_width = self.ui_right_panel_width;
        let mut top_panel_height = self.ui_top_panel_height;
        let mut bottom_panel_height = self.ui_bottom_panel_height;

        // Ivar state for UI
        let mut render_mode = self.ivar_state.mode;
        let ivar_buckets_completed = self.ivar_state.buckets_completed;
        let ivar_total_buckets = self.ivar_state.buckets.len();
        let ivar_elapsed = self.ivar_state.elapsed_secs();
        let ivar_render_complete = self.ivar_state.render_complete;
        let ivar_accumulated_spp = self.ivar_state.accumulated_samples;
        let mut ivar_target_spp = self.ivar_state.target_spp;
        let ivar_current_scale = self.ivar_state.current_scale;
        let mut ivar_nav_quality = self.ivar_state.interaction_quality;

        let full_output = self.egui_ctx.run(raw_input, |ctx| {
            if !show_ui {
                left_panel_width = 0.0;
                right_panel_width = 0.0;
                top_panel_height = 0.0;
                bottom_panel_height = 0.0;
                return;
            }

            let top_panel = egui::TopBottomPanel::top("top_panel")
                .exact_height(28.0)
                .show(ctx, |ui| {
                    ui.horizontal_centered(|ui| {
                        ui.label("BIF");
                        ui.separator();
                        ui.checkbox(&mut self.show_grid, "Grid");
                        ui.checkbox(&mut self.point_preview.visible, "Points");

                        // Show async USD load status
                        if let crate::UsdLoadStatus::Loading(ref progress) = self.usd_load_status {
                            ui.separator();
                            ui.spinner();
                            ui.label(progress.to_string());
                        } else if let crate::UsdLoadStatus::Error(ref msg) = self.usd_load_status {
                            ui.separator();
                            ui.colored_label(egui::Color32::RED, msg);
                        }

                        // Show stage metadata (metersPerUnit, upAxis) if available
                        if let Some(ref meta) = self.working_scene.stage_metadata {
                            ui.separator();
                            ui.colored_label(
                                egui::Color32::from_rgb(160, 160, 160),
                                format!("[{}]", meta),
                            );

                            // Axis/unit correction toggles (only when metadata exists)
                            let needs_axis = meta.up_axis
                                == bif_core::usd::cpp_bridge::UpAxis::Z;
                            let needs_scale =
                                (meta.meters_per_unit - 1.0).abs() > 1e-6;

                            if needs_axis
                                && ui
                                    .checkbox(
                                        &mut self.apply_axis_correction,
                                        "Z\u{2192}Y",
                                    )
                                    .on_hover_text("Rotate scene from Z-up to Y-up")
                                    .changed()
                            {
                                ctx.data_mut(|d| {
                                    d.insert_temp(
                                        egui::Id::new("stage_correction_changed"),
                                        true,
                                    );
                                });
                            }
                            if needs_scale
                                && ui
                                    .checkbox(
                                        &mut self.apply_unit_scaling,
                                        "\u{2192}m",
                                    )
                                    .on_hover_text(format!(
                                        "Scale from {:.4} to meters",
                                        meta.meters_per_unit
                                    ))
                                    .changed()
                            {
                                ctx.data_mut(|d| {
                                    d.insert_temp(
                                        egui::Id::new("stage_correction_changed"),
                                        true,
                                    );
                                });
                            }
                        }
                    });
                });
            top_panel_height = top_panel.response.rect.height();

            let stats_panel = egui::SidePanel::left("stats_panel")
                .default_width(300.0)
                .show(ctx, |ui| {
                    ui.heading("BIF Viewer");
                    ui.separator();

                    // Render Mode Dropdown (Houdini-style)
                    ui.horizontal(|ui| {
                        ui.label("Renderer:");
                        egui::ComboBox::from_id_salt("render_mode")
                            .selected_text(render_mode.display_name())
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut render_mode, RenderMode::Vulkan, "Vulkan");
                                ui.selectable_value(&mut render_mode, RenderMode::Ivar, "Ivar");
                            });
                    });

                    // Show Ivar stats when in Ivar mode
                    if render_mode == RenderMode::Ivar {
                        ui.separator();
                        ui.label("Ivar Path Tracer");

                        // Show build status or render progress
                        match self.ivar_state.build_status {
                            BuildStatus::NotStarted => {
                                ui.label("Preparing scene...");
                            }
                            BuildStatus::Building => {
                                // Show spinner while building
                                ui.horizontal(|ui| {
                                    ui.spinner();
                                    ui.label("Building scene geometry...");
                                });
                                ui.label(format!(
                                    "{} instances, {} tris/instance",
                                    self.instance_transforms.len(),
                                    self.mesh_data.indices.len() / 3
                                ));
                            }
                            BuildStatus::Failed => {
                                ui.colored_label(egui::Color32::RED, "⚠ Scene build failed");
                            }
                            BuildStatus::Complete => {
                                // Progressive SPP progress
                                let spp_progress = if ivar_target_spp > 0 {
                                    ivar_accumulated_spp as f32 / ivar_target_spp as f32
                                } else {
                                    0.0
                                };
                                let progress_bar = egui::ProgressBar::new(spp_progress)
                                    .text(format!(
                                        "{}/{} SPP",
                                        ivar_accumulated_spp, ivar_target_spp
                                    ));
                                ui.add(progress_bar);

                                // Current pass bucket progress
                                if !ivar_render_complete && ivar_total_buckets > 0 {
                                    let bucket_frac =
                                        ivar_buckets_completed as f32 / ivar_total_buckets as f32;
                                    ui.add(
                                        egui::ProgressBar::new(bucket_frac)
                                            .text(format!(
                                                "Pass {}: {}/{}",
                                                ivar_accumulated_spp,
                                                ivar_buckets_completed,
                                                ivar_total_buckets
                                            ))
                                            .desired_width(ui.available_width()),
                                    );
                                }

                                // Target SPP slider
                                ui.horizontal(|ui| {
                                    ui.label("Target:");
                                    ui.add(
                                        egui::Slider::new(&mut ivar_target_spp, 1..=256)
                                            .text("SPP"),
                                    );
                                });

                                // Navigation preview quality slider
                                ui.horizontal(|ui| {
                                    ui.label("Nav Quality:");
                                    ui.add(
                                        egui::Slider::new(&mut ivar_nav_quality, 1..=3)
                                            .custom_formatter(|v, _| {
                                                match v as u32 {
                                                    1 => "1/2".to_string(),
                                                    2 => "1/4".to_string(),
                                                    3 => "1/8".to_string(),
                                                    _ => format!("1/{}", 2u32.pow(v as u32)),
                                                }
                                            }),
                                    );
                                });

                                // Show current preview scale when not at full res
                                if ivar_current_scale > 1 {
                                    ui.colored_label(
                                        egui::Color32::YELLOW,
                                        format!("Preview: 1/{}", ivar_current_scale),
                                    );
                                }

                                ui.label(format!("Time: {:.1}s", ivar_elapsed));

                                if ivar_render_complete {
                                    ui.colored_label(egui::Color32::GREEN, "Render Complete");

                                    // Denoise button (feature-gated)
                                    #[cfg(feature = "oidn")]
                                    {
                                        if self.ivar_state.is_denoised {
                                            ui.colored_label(egui::Color32::from_rgb(100, 200, 255), "Denoised");
                                        } else if ui.button("Denoise (OIDN)").clicked() {
                                            ctx.data_mut(|d| {
                                                d.insert_temp(egui::Id::new("denoise_requested"), true)
                                            });
                                        }
                                    }
                                    #[cfg(not(feature = "oidn"))]
                                    {
                                        ui.add_enabled(false, egui::Button::new("Denoise (OIDN)"))
                                            .on_disabled_hover_text("Build with --features oidn");
                                    }
                                } else if ivar_accumulated_spp > 0 {
                                    ui.colored_label(egui::Color32::YELLOW, "Refining...");
                                }
                            }
                        }

                        // AOV preview dropdown
                        ui.horizontal(|ui| {
                            ui.label("Preview:");
                            egui::ComboBox::from_id_salt("aov_preview")
                                .selected_text(self.ivar_state.preview_aov.display_name())
                                .show_ui(ui, |ui| {
                                    for channel in ivar_state::AovChannel::all() {
                                        ui.selectable_value(
                                            &mut self.ivar_state.preview_aov,
                                            *channel,
                                            channel.display_name(),
                                        );
                                    }
                                });
                        });

                        // SHARC Radiance Cache settings
                        ui.separator();
                        ui.collapsing("SHARC Cache", |ui| {
                            let cfg = &mut self.ivar_state.radiance_cache_config;
                            let mut changed = false;
                            let mut enabled = cfg.enabled;
                            if ui.checkbox(&mut enabled, "Enabled").changed() {
                                cfg.enabled = enabled;
                                changed = true;
                            }
                            ui.horizontal(|ui| {
                                ui.label("Cell Size:");
                                if ui
                                    .add(
                                        egui::Slider::new(&mut cfg.cell_size, 0.01..=10.0)
                                            .logarithmic(true),
                                    )
                                    .changed()
                                {
                                    changed = true;
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.label("Buffer:");
                                let sizes = [
                                    (256 * 1024, "256K"),
                                    (512 * 1024, "512K"),
                                    (1024 * 1024, "1M"),
                                    (2 * 1024 * 1024, "2M"),
                                    (4 * 1024 * 1024, "4M"),
                                ];
                                egui::ComboBox::from_id_salt("sharc_buf_size")
                                    .selected_text(
                                        sizes
                                            .iter()
                                            .find(|(s, _)| *s == cfg.buffer_size)
                                            .map(|(_, n)| *n)
                                            .unwrap_or("Custom"),
                                    )
                                    .show_ui(ui, |ui| {
                                        for &(size, name) in &sizes {
                                            if ui
                                                .selectable_value(
                                                    &mut cfg.buffer_size,
                                                    size,
                                                    name,
                                                )
                                                .changed()
                                            {
                                                changed = true;
                                            }
                                        }
                                    });
                            });
                            ui.horizontal(|ui| {
                                ui.label("Min Samples:");
                                if ui
                                    .add(egui::Slider::new(&mut cfg.min_samples, 1..=16))
                                    .changed()
                                {
                                    changed = true;
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.label("Min Bounce:");
                                if ui
                                    .add(egui::Slider::new(&mut cfg.min_bounce_depth, 1..=4))
                                    .changed()
                                {
                                    changed = true;
                                }
                            });

                            // Cache stats
                            if let Some(ref cache) = self.ivar_state.radiance_cache {
                                ui.label(format!(
                                    "Hit: {:.1}%  Occ: {:.1}%",
                                    cache.hit_rate() * 100.0,
                                    cache.occupancy() * 100.0
                                ));
                            }

                            // Rebuild cache if config changed
                            if changed {
                                if cfg.enabled {
                                    let cache = std::sync::Arc::new(
                                        bif_renderer::RadianceCache::new(cfg.clone()),
                                    );
                                    self.ivar_state.radiance_cache = Some(cache);
                                } else {
                                    self.ivar_state.radiance_cache = None;
                                }
                            }
                        });

                        // Rebuild Scene button
                        ui.separator();
                        // Note: Can't call self.invalidate_ivar_scene() here due to borrow rules
                        // Using ctx.data_mut() to store the request
                        if ui.button("Rebuild Scene").clicked() {
                            ctx.data_mut(|d| {
                                d.insert_temp(egui::Id::new("rebuild_scene_requested"), true)
                            });
                        }
                        ui.label("↻ Rebuild if geometry changes");
                    }

                    ui.separator();

                    // FPS Counter
                    ui.label(format!("FPS: {:.1}", fps));
                    ui.separator();

                    // Scene Stats
                    ui.collapsing("Scene Stats", |ui| {
                        ui.label(format!("Instances: {} total", num_instances));
                        let total_visible = visible_instances + lod_box_instances;
                        ui.label(format!(
                            "Visible: {} ({:.0}%)",
                            total_visible,
                            if num_instances > 0 {
                                (total_visible as f32 / num_instances as f32) * 100.0
                            } else {
                                0.0
                            }
                        ));
                        ui.label(format!("  Full mesh: {}", visible_instances));
                        ui.label(format!("  Box LOD: {}", lod_box_instances));
                        // Triangle count: full mesh tris + LOD_BOX_TRIANGLES per box
                        const LOD_BOX_TRIANGLES: u32 = 12; // cube = 6 faces * 2 tris
                        let full_mesh_tris = triangles_per_instance * visible_instances;
                        let box_tris = LOD_BOX_TRIANGLES * lod_box_instances;
                        ui.label(format!(
                            "Triangles: {} ({}+{})",
                            full_mesh_tris + box_tris,
                            full_mesh_tris,
                            box_tris
                        ));
                        ui.label(format!("Tris/Instance: {}", triangles_per_instance));

                        ui.separator();
                        ui.label("LOD Budget Control:");
                        // Slider for max polys (in millions for readability)
                        let max_millions = (lod_max_polys as f32 / 1_000_000.0).max(0.1);
                        let mut millions = max_millions;
                        ui.add(
                            egui::Slider::new(&mut millions, 0.1..=100.0)
                                .logarithmic(true)
                                .text("Max M tris")
                                .suffix("M"),
                        );
                        if (millions - max_millions).abs() > 0.001 {
                            lod_max_polys = (millions * 1_000_000.0) as u32;
                        }
                        let budget_used =
                            (full_mesh_tris as f32 / lod_max_polys as f32 * 100.0).min(100.0);
                        ui.label(format!("Budget: {:.0}% used", budget_used));
                    });

                    ui.separator();

                    // Camera Stats
                    ui.collapsing("Camera", |ui| {
                        ui.label(format!(
                            "Position: ({:.2}, {:.2}, {:.2})",
                            camera.position.x, camera.position.y, camera.position.z
                        ));
                        ui.label(format!(
                            "Target: ({:.2}, {:.2}, {:.2})",
                            camera.target.x, camera.target.y, camera.target.z
                        ));
                        ui.label(format!("Distance: {:.2}", camera.distance));
                        ui.label(format!("Yaw: {:.2}°", camera.yaw.to_degrees()));
                        ui.label(format!("Pitch: {:.2}°", camera.pitch.to_degrees()));
                        ui.label(format!("FOV: {:.2}°", camera.fov_y.to_degrees()));
                        ui.label(format!("Near: {:.2}", camera.near));
                        ui.label(format!("Far: {:.2}", camera.far));

                        ui.label("Press F to frame mesh");
                    });

                    ui.separator();

                    // Mesh Info
                    ui.collapsing("Mesh Bounds", |ui| {
                        let mesh_center = (mesh_bounds_min + mesh_bounds_max) * 0.5;
                        let mesh_size = (mesh_bounds_max - mesh_bounds_min).length();

                        ui.label(format!(
                            "Bounds Min: ({:.2}, {:.2}, {:.2})",
                            mesh_bounds_min.x, mesh_bounds_min.y, mesh_bounds_min.z
                        ));
                        ui.label(format!(
                            "Bounds Max: ({:.2}, {:.2}, {:.2})",
                            mesh_bounds_max.x, mesh_bounds_max.y, mesh_bounds_max.z
                        ));
                        ui.label(format!(
                            "Center: ({:.2}, {:.2}, {:.2})",
                            mesh_center.x, mesh_center.y, mesh_center.z
                        ));
                        ui.label(format!("Size: {:.2}", mesh_size));
                    });

                    ui.separator();

                    // Viewport Info
                    ui.collapsing("Viewport", |ui| {
                        ui.label(format!("Resolution: {}x{}", size.0, size.1));
                        ui.label(format!("Aspect: {:.3}", size.0 as f32 / size.1 as f32));
                        ui.add(egui::Slider::new(&mut gnomon_size, 40..=120).text("Gnomon Size"));
                    });

                    ui.separator();

                    // Controls Help
                    ui.collapsing("Controls", |ui| {
                        ui.label("🖱️ Left Mouse: Tumble (orbit)");
                        ui.label("🖱️ Middle Mouse: Track (pan)");
                        ui.label("🖱️ Scroll Wheel: Dolly (zoom)");
                        ui.label("⌨️ W/A/S/D: Move forward/left/back/right");
                        ui.label("⌨️ Q/E: Move down/up");
                        ui.label("⌨️ F: Frame mesh");
                    });

                    ui.separator();

                    // Render to Disk
                    ui.collapsing("Render to Disk", |ui| {
                        let settings = &mut self.ivar_state.batch_settings;

                        // Camera source
                        ui.horizontal(|ui| {
                            ui.label("Camera:");
                            egui::ComboBox::from_id_salt("batch_camera")
                                .selected_text(settings.camera_source.display_name())
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut settings.camera_source,
                                        ivar_state::CameraSource::Viewport,
                                        "Viewport",
                                    );
                                    // List USD cameras if stage available
                                    if let Some(ref stage) = self.usd_stage {
                                        if let Ok(paths) = stage.camera_paths() {
                                            for path in paths {
                                                let is_selected = matches!(
                                                    &settings.camera_source,
                                                    ivar_state::CameraSource::UsdCamera(p) if p == &path
                                                );
                                                if ui
                                                    .selectable_label(is_selected, &path)
                                                    .clicked()
                                                {
                                                    settings.camera_source =
                                                        ivar_state::CameraSource::UsdCamera(path.clone());
                                                }
                                            }
                                        }
                                    }
                                });
                        });

                        // Frame range
                        ui.horizontal(|ui| {
                            ui.label("Frames:");
                            ui.add(
                                egui::DragValue::new(&mut settings.start_frame)
                                    .speed(1.0)
                                    .prefix(""),
                            );
                            ui.label("-");
                            ui.add(
                                egui::DragValue::new(&mut settings.end_frame)
                                    .speed(1.0)
                                    .prefix(""),
                            );
                        });

                        // Use timeline range button
                        if ui.button("Use Timeline Range").clicked()
                            && self.timeline_state.has_range()
                        {
                            settings.start_frame = self.timeline_state.start_frame as i32;
                            settings.end_frame = self.timeline_state.end_frame as i32;
                        }

                        ui.horizontal(|ui| {
                            ui.label("Step:");
                            ui.add(egui::DragValue::new(&mut settings.frame_step).speed(1.0).range(1..=100));
                        });

                        // Resolution
                        ui.horizontal(|ui| {
                            ui.label("Resolution:");
                            ui.add(
                                egui::DragValue::new(&mut settings.resolution_x)
                                    .speed(10.0)
                                    .range(64..=8192),
                            );
                            ui.label("x");
                            ui.add(
                                egui::DragValue::new(&mut settings.resolution_y)
                                    .speed(10.0)
                                    .range(64..=8192),
                            );
                        });

                        // Quality
                        ui.horizontal(|ui| {
                            ui.label("SPP:");
                            ui.add(
                                egui::DragValue::new(&mut settings.samples_per_pixel)
                                    .speed(1.0)
                                    .range(1..=1024),
                            );
                        });

                        ui.horizontal(|ui| {
                            ui.label("Max Depth:");
                            ui.add(
                                egui::DragValue::new(&mut settings.max_depth)
                                    .speed(1.0)
                                    .range(1..=32),
                            );
                        });

                        // Compression
                        ui.horizontal(|ui| {
                            ui.label("Compression:");
                            egui::ComboBox::from_id_salt("batch_compression")
                                .selected_text(settings.compression.display_name())
                                .show_ui(ui, |ui| {
                                    for comp in bif_renderer::ExrCompression::all() {
                                        ui.selectable_value(
                                            &mut settings.compression,
                                            *comp,
                                            comp.display_name(),
                                        );
                                    }
                                });
                        });

                        // Per-AOV checkboxes
                        ui.collapsing("AOVs", |ui| {
                            let aov = &mut settings.aov_settings;
                            ui.checkbox(&mut aov.include_alpha, "Alpha (A)");
                            ui.horizontal(|ui| {
                                ui.checkbox(&mut aov.include_depth, "Depth (Z)");
                                if aov.include_depth {
                                    ui.add(egui::DragValue::new(&mut aov.depth_near)
                                        .speed(0.1)
                                        .range(0.001..=aov.depth_far)
                                        .prefix("Near: "));
                                    ui.add(egui::DragValue::new(&mut aov.depth_far)
                                        .speed(10.0)
                                        .range(aov.depth_near..=100000.0)
                                        .prefix("Far: "));
                                }
                            });
                            if aov.include_depth {
                                ui.checkbox(&mut aov.auto_depth_bounds, "Auto bounds from scene");
                            }
                            ui.checkbox(&mut aov.include_normal, "Normal (N)");

                            // Denoise checkbox (feature-gated)
                            #[cfg(feature = "oidn")]
                            {
                                ui.checkbox(&mut aov.denoise_output, "Denoise (OIDN)");
                            }
                            #[cfg(not(feature = "oidn"))]
                            {
                                let mut dummy = false;
                                ui.add_enabled(false, egui::Checkbox::new(&mut dummy, "Denoise (OIDN)"))
                                    .on_disabled_hover_text("Build with --features oidn");
                            }
                        });

                        // Output path
                        ui.horizontal(|ui| {
                            ui.label("Output:");
                            ui.text_edit_singleline(&mut settings.output_pattern);
                        });

                        ui.horizontal(|ui| {
                            ui.label("Dir:");
                            ui.text_edit_singleline(&mut settings.output_directory);
                        });

                        // Sync viewport to USD camera button
                        if let ivar_state::CameraSource::UsdCamera(ref cam_path) = settings.camera_source {
                            if ui.button("Sync Viewport to Camera").clicked() {
                                ctx.data_mut(|d| {
                                    d.insert_temp(egui::Id::new("sync_viewport_to_usd_camera"), cam_path.clone())
                                });
                            }
                        }

                        ui.separator();

                        // Render button and status on same line
                        let status = &self.ivar_state.batch_status;
                        let is_rendering = matches!(status, ivar_state::BatchRenderStatus::Rendering { .. });
                        let can_render = !settings.output_directory.is_empty() && !is_rendering;

                        ui.horizontal(|ui| {
                            if ui.add_enabled(can_render, egui::Button::new("Render")).clicked() {
                                ctx.data_mut(|d| {
                                    d.insert_temp(egui::Id::new("start_batch_render"), true)
                                });
                            }

                            // Show status next to button
                            match status {
                                ivar_state::BatchRenderStatus::Idle => {
                                    if settings.output_directory.is_empty() {
                                        ui.label("Set output directory");
                                    }
                                }
                                ivar_state::BatchRenderStatus::Complete { total_elapsed_secs } => {
                                    ui.colored_label(
                                        egui::Color32::GREEN,
                                        format!("Done ({:.1}s)", total_elapsed_secs),
                                    );
                                }
                                ivar_state::BatchRenderStatus::Cancelled => {
                                    ui.colored_label(egui::Color32::YELLOW, "Cancelled");
                                }
                                ivar_state::BatchRenderStatus::Failed(msg) => {
                                    ui.colored_label(egui::Color32::RED, format!("Failed: {}", msg));
                                }
                                _ => {}
                            }
                        });

                        // Progress bar and cancel button when rendering
                        if let ivar_state::BatchRenderStatus::Rendering {
                            current_frame,
                            total_frames,
                            frame_progress,
                        } = status
                        {
                            let overall = ((*current_frame - 1) as f32 + frame_progress)
                                / *total_frames as f32;
                            ui.add(
                                egui::ProgressBar::new(overall)
                                    .text(format!("Frame {}/{}", current_frame, total_frames)),
                            );
                            if ui.button("Cancel").clicked() {
                                ctx.data_mut(|d| {
                                    d.insert_temp(egui::Id::new("cancel_batch_render"), true)
                                });
                            }
                        }
                    });

                    // Export Edits
                    let has_edits = !self.edit_state.transform_overrides.is_empty()
                        || !self.edit_state.keyframe_overrides.is_empty();
                    if has_edits {
                        ui.separator();
                        ui.collapsing("Export Edits", |ui| {
                            let override_count = self.edit_state.transform_overrides.len();
                            let keyframe_count = self.edit_state.keyframe_overrides.len();
                            ui.label(format!(
                                "{} overrides, {} keyframed",
                                override_count, keyframe_count
                            ));
                            if ui.button("Export as USD...").clicked() {
                                if let Some(path) = rfd::FileDialog::new()
                                    .add_filter("USD Files", &["usda", "usdc"])
                                    .set_file_name("edits.usda")
                                    .save_file()
                                {
                                    ctx.data_mut(|d| {
                                        d.insert_temp(
                                            egui::Id::new("export_edit_layer"),
                                            path.display().to_string(),
                                        );
                                    });
                                }
                            }
                        });
                    }

                    ui.separator();

                    // Scene Browser (collapsible)
                    ui.collapsing("Scene Browser", |ui| {
                        // Composite provider: lightweight wrapper over cached scene graph
                        let composite = CompositeProvider::new(
                            self.usd_stage.as_ref().map(|s| s.as_ref() as &dyn PrimDataProvider),
                            &self.cached_scene_graph,
                        );
                        let provider: &dyn PrimDataProvider = &composite;

                        // Store selection change request in temp data for processing after egui run
                        if let Some(new_selection) = scene_browser::render_scene_browser(
                            ui,
                            &mut self.scene_browser_state,
                            provider,
                        ) {
                            ctx.data_mut(|d| {
                                d.insert_temp(
                                    egui::Id::new("prim_selection_changed"),
                                    new_selection,
                                );
                            });
                        }
                    });
                });
            left_panel_width = stats_panel.response.rect.width();

            // Property Inspector (right panel)
            // Build editable transform for selected viewport instance
            let editable_transform: Option<(usize, bif_core::Transform)> =
                self.selected_instance_index.and_then(|idx| {
                    // Use edit override if present, otherwise decompose from current_transforms
                    let transform = if let Some(t) = self.edit_state.transform_overrides.get(&idx) {
                        t.clone()
                    } else if idx < self.current_transforms.len() {
                        bif_core::Transform::from_matrix(self.current_transforms[idx])
                    } else {
                        return None;
                    };
                    Some((idx, transform))
                });

            let property_panel = egui::SidePanel::right("property_panel")
                .default_width(280.0)
                .show(ctx, |ui| {
                    // If an Xform node is selected, show its T/R/S in the panel
                    let selected_xform_id = self.node_graph_state.selected_node.filter(|nid| {
                        matches!(
                            self.node_graph_state.snarl[*nid],
                            crate::node_graph::SceneNode::Xform { .. }
                        )
                    });

                    if let Some(xform_nid) = selected_xform_id {
                        if let crate::node_graph::SceneNode::Xform {
                            translate,
                            rotate,
                            scale,
                            prim_filter,
                        } = &mut self.node_graph_state.snarl[xform_nid]
                        {
                            let changed =
                                crate::property_inspector::render_xform_properties(
                                    ui, translate, rotate, scale, prim_filter,
                                );
                            if changed {
                                self.xform_property_changed = Some(xform_nid);
                            }
                        }
                        ui.separator();
                    }

                    let et_ref =
                        editable_transform.as_ref().map(|(idx, t)| (*idx, t));
                    render_property_inspector(
                        ui,
                        self.selected_prim_properties.as_ref(),
                        et_ref,
                    );

                    // Point cloud summary
                    if !self.working_scene.point_clouds.is_empty() {
                        ui.separator();
                        ui.heading("Point Clouds");
                        let total_points: usize = self
                            .working_scene
                            .point_clouds
                            .iter()
                            .map(|c| c.point_count())
                            .sum();
                        ui.label(format!(
                            "{} clouds, {} total points",
                            self.working_scene.point_clouds.len(),
                            total_points
                        ));
                        for cloud in &self.working_scene.point_clouds {
                            ui.label(format!(
                                "  {} - {} pts",
                                cloud.name,
                                cloud.point_count()
                            ));
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
                        let cam_display = match &self.viewport_camera_source {
                            CameraSource::SceneCamera(idx) => {
                                self.scene_cameras.get(*idx)
                                    .map(|c| c.name.as_str())
                                    .unwrap_or("Scene Camera")
                            }
                            other => other.display_name(),
                        };
                        egui::ComboBox::from_id_salt("viewport_camera")
                            .selected_text(cam_display)
                            .width(100.0)
                            .show_ui(ui, |ui| {
                                // Perspective viewport option
                                if ui
                                    .selectable_label(
                                        matches!(self.viewport_camera_source, CameraSource::Viewport),
                                        "Perspective",
                                    )
                                    .clicked()
                                {
                                    self.viewport_camera_source = CameraSource::Viewport;
                                    self.camera_locked = false;
                                    self.selected_usd_camera = None;
                                    ctx.data_mut(|d| {
                                        d.insert_temp(
                                            egui::Id::new("camera_projection_change"),
                                            "perspective".to_string(),
                                        );
                                    });
                                }
                                // USD cameras from stage
                                if let Some(ref stage) = self.usd_stage {
                                    if let Ok(paths) = stage.camera_paths() {
                                        for path in paths {
                                            let is_selected = matches!(
                                                &self.viewport_camera_source,
                                                CameraSource::UsdCamera(p) if p == &path
                                            );
                                            if ui.selectable_label(is_selected, &path).clicked() {
                                                self.viewport_camera_source =
                                                    CameraSource::UsdCamera(path.clone());
                                                self.selected_usd_camera = Some(path.clone());
                                                self.camera_locked = true;
                                                // Sync to camera immediately (via egui temp data)
                                                ctx.data_mut(|d| {
                                                    d.insert_temp(
                                                        egui::Id::new("sync_viewport_camera"),
                                                        path,
                                                    )
                                                });
                                            }
                                        }
                                    }
                                }
                                // Orthographic presets
                                ui.separator();
                                for preset in bif_math::OrthoPreset::all() {
                                    let is_selected = matches!(
                                        &self.viewport_camera_source,
                                        CameraSource::OrthoView(p) if p == preset
                                    );
                                    if ui
                                        .selectable_label(is_selected, preset.display_name())
                                        .clicked()
                                    {
                                        self.viewport_camera_source =
                                            CameraSource::OrthoView(*preset);
                                        self.camera_locked = false;
                                        self.selected_usd_camera = None;
                                        ctx.data_mut(|d| {
                                            d.insert_temp(
                                                egui::Id::new("camera_projection_change"),
                                                format!("ortho:{}", preset.display_name()),
                                            );
                                        });
                                    }
                                }
                                // Scene cameras (from Camera primitives)
                                if !self.scene_cameras.is_empty() {
                                    ui.separator();
                                    for (idx, cam) in self.scene_cameras.iter().enumerate() {
                                        let is_selected = matches!(
                                            &self.viewport_camera_source,
                                            CameraSource::SceneCamera(i) if *i == idx
                                        );
                                        if ui
                                            .selectable_label(is_selected, &cam.name)
                                            .clicked()
                                        {
                                            self.viewport_camera_source =
                                                CameraSource::SceneCamera(idx);
                                            self.camera_locked = true;
                                            self.selected_usd_camera = None;
                                            ctx.data_mut(|d| {
                                                d.insert_temp(
                                                    egui::Id::new("sync_scene_camera"),
                                                    idx as u64,
                                                );
                                            });
                                        }
                                    }
                                }
                            });

                        // Lock/Unlock toggle (show when USD or scene camera selected)
                        if matches!(self.viewport_camera_source, CameraSource::UsdCamera(_) | CameraSource::SceneCamera(_)) {
                            let icon = if self.camera_locked { "Lock" } else { "Free" };
                            if ui.button(icon).clicked() {
                                self.camera_locked = !self.camera_locked;
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
                            if ui.button(play_text).clicked() {
                                self.timeline_state.toggle_playback();
                            }

                            // Go to start
                            if ui.button("|◀").clicked() {
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
                                        egui::Color32::from_rgb(255, 200, 50),
                                        egui::Stroke::new(1.0, egui::Color32::from_rgb(180, 140, 30)),
                                    ));
                                }
                            }
                        }

                        // Frame range display (end)
                        ui.label(format!("{:.0}", end));

                        // Go to end (disabled if no animation)
                        ui.add_enabled_ui(has_animation, |ui| {
                            if ui.button("▶|").clicked() {
                                self.timeline_state.go_to_end();
                            }
                        });

                        // Loop toggle
                        ui.checkbox(&mut self.timeline_state.loop_playback, "Loop");

                        // Realtime toggle (wall-clock vs every-frame)
                        if ui
                            .checkbox(&mut self.timeline_state.realtime, "RT")
                            .on_hover_text(
                                "Realtime: ON = wall-clock accurate, OFF = every frame",
                            )
                            .changed()
                        {
                            self.timeline_state.reset_playback_anchor();
                        }

                        // Integer frame snap toggle
                        ui.checkbox(&mut self.timeline_state.snap_to_frames, "Int");

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
                    let events = render_node_graph(ui, &mut self.node_graph_state);
                    // Store events for processing after egui frame ends
                    for event in events {
                        ctx.data_mut(|d| {
                            let mut pending: Vec<NodeGraphEvent> = d
                                .get_temp(egui::Id::new("node_graph_events"))
                                .unwrap_or_default();
                            pending.push(event);
                            d.insert_temp(egui::Id::new("node_graph_events"), pending);
                        });
                    }
                });
            bottom_panel_height = node_graph_panel.response.rect.height() + timeline_height;

            // Draw translate gizmo overlay (after all panels, on foreground layer)
            if let Some(sel_idx) = self.selected_instance_index {
                if sel_idx < self.current_transforms.len() {
                    let transform = if let Some(t) = self.edit_state.transform_overrides.get(&sel_idx) {
                        t.clone()
                    } else {
                        bif_core::Transform::from_matrix(self.current_transforms[sel_idx])
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
                        &self.camera,
                        world_pos,
                        vp_rect,
                        &self.gizmo_state,
                        mouse_screen,
                    );

                    // Store hovered axis for next frame (can't mutate gizmo_state here)
                    ctx.data_mut(|d| {
                        d.insert_temp(egui::Id::new("gizmo_hovered_axis"), hovered as u8);
                    });
                }
            }
        });

        // Update gizmo hovered axis from egui frame
        let hovered_axis_raw: u8 = self
            .egui_ctx
            .data(|d| d.get_temp(egui::Id::new("gizmo_hovered_axis")).unwrap_or(0));
        if !self.gizmo_state.is_dragging {
            self.gizmo_state.hovered_axis = crate::gizmo::GizmoAxis::from_u8(hovered_axis_raw);
        }

        // Update gnomon size from UI
        self.gnomon.size = gnomon_size;
        // Update target SPP from UI (may resume rendering if increased)
        if ivar_target_spp != self.ivar_state.target_spp {
            self.ivar_state.target_spp = ivar_target_spp;
            if ivar_target_spp > self.ivar_state.accumulated_samples {
                self.ivar_state.render_complete = false;
            }
        }
        // Update interaction quality from UI slider
        self.ivar_state.interaction_quality = ivar_nav_quality;
        self.ui_left_panel_width = left_panel_width;
        self.ui_right_panel_width = right_panel_width;
        self.ui_top_panel_height = top_panel_height;
        self.ui_bottom_panel_height = bottom_panel_height;

        // Update camera aspect to match viewport (not full window)
        let (_, _, vp_w, vp_h) = self.viewport_rect();
        let new_aspect = vp_w / vp_h;
        if (self.camera.aspect - new_aspect).abs() > 0.001 {
            self.camera.set_aspect(new_aspect);
            self.update_camera();
        }

        // Update LOD max polys from UI
        self.culling.lod_max_polys = lod_max_polys;

        // Update render mode from UI - detect mode change
        let mode_changed = self.ivar_state.mode != render_mode;
        self.ivar_state.mode = render_mode;

        // Handle mode switch to Ivar - start render if needed
        if mode_changed && render_mode == RenderMode::Ivar {
            log::info!("Switched to Ivar mode - starting render");
            self.ivar_state.current_scale = 1;
            self.ivar_state.last_interaction_time = None;
            self.start_ivar_render();
        }

        // Handle rebuild scene request (stored in egui temp data)
        let rebuild_requested = self.egui_ctx.data(|d| {
            d.get_temp::<bool>(egui::Id::new("rebuild_scene_requested"))
                .unwrap_or(false)
        });
        if rebuild_requested {
            log::info!("Manual scene rebuild requested");
            self.invalidate_ivar_scene();
            // Clear the flag
            self.egui_ctx
                .data_mut(|d| d.remove::<bool>(egui::Id::new("rebuild_scene_requested")));
        }

        // Handle denoise request
        let denoise_requested = self.egui_ctx.data(|d| {
            d.get_temp::<bool>(egui::Id::new("denoise_requested"))
                .unwrap_or(false)
        });
        if denoise_requested {
            self.egui_ctx
                .data_mut(|d| d.remove::<bool>(egui::Id::new("denoise_requested")));
            self.denoise_ivar_result();
        }

        // Handle batch render start request
        let start_batch = self.egui_ctx.data(|d| {
            d.get_temp::<bool>(egui::Id::new("start_batch_render"))
                .unwrap_or(false)
        });
        if start_batch {
            self.egui_ctx
                .data_mut(|d| d.remove::<bool>(egui::Id::new("start_batch_render")));
            self.start_batch_render();
        }

        // Handle batch render cancel request
        let cancel_batch = self.egui_ctx.data(|d| {
            d.get_temp::<bool>(egui::Id::new("cancel_batch_render"))
                .unwrap_or(false)
        });
        if cancel_batch {
            self.egui_ctx
                .data_mut(|d| d.remove::<bool>(egui::Id::new("cancel_batch_render")));
            if let Some(ref flag) = self.batch_cancel_flag {
                flag.store(true, Ordering::Relaxed);
            }
        }

        // Handle sync viewport to USD camera request (from batch render panel)
        let sync_camera: Option<String> = self
            .egui_ctx
            .data(|d| d.get_temp(egui::Id::new("sync_viewport_to_usd_camera")));
        if let Some(camera_path) = sync_camera {
            self.egui_ctx
                .data_mut(|d| d.remove::<String>(egui::Id::new("sync_viewport_to_usd_camera")));
            self.sync_viewport_to_usd_camera(&camera_path);
        }

        // Handle viewport camera selection from timeline dropdown
        let sync_viewport_cam: Option<String> = self
            .egui_ctx
            .data(|d| d.get_temp(egui::Id::new("sync_viewport_camera")));
        if let Some(camera_path) = sync_viewport_cam {
            self.egui_ctx
                .data_mut(|d| d.remove::<String>(egui::Id::new("sync_viewport_camera")));
            self.sync_viewport_to_usd_camera(&camera_path);
        }

        // Handle scene camera selection from timeline dropdown
        let sync_scene_cam: Option<u64> = self
            .egui_ctx
            .data(|d| d.get_temp(egui::Id::new("sync_scene_camera")));
        if let Some(cam_idx) = sync_scene_cam {
            self.egui_ctx
                .data_mut(|d| d.remove::<u64>(egui::Id::new("sync_scene_camera")));
            self.sync_viewport_to_scene_camera(cam_idx as usize);
        }

        // Handle prim selection from scene browser
        let selected_prim: Option<String> = self
            .egui_ctx
            .data(|d| d.get_temp(egui::Id::new("prim_selection_changed")));
        if let Some(prim_path) = selected_prim {
            self.egui_ctx
                .data_mut(|d| d.remove::<String>(egui::Id::new("prim_selection_changed")));
            self.selected_prim_path = Some(prim_path.clone());
            reset_transform_edit_cache(&self.egui_ctx);
            let composite = CompositeProvider::new(
                self.usd_stage
                    .as_ref()
                    .map(|s| s.as_ref() as &dyn PrimDataProvider),
                &self.cached_scene_graph,
            );
            if let Some(info) = composite.get_prim_info(&prim_path) {
                let mut props = PrimProperties::from_display_info(&info);
                // Enrich with procedural data if available
                if let Some(proc_data) = composite.get_procedural_data(&prim_path) {
                    match &proc_data.kind {
                        ProceduralPrimKind::Mesh {
                            vertex_count,
                            triangle_count,
                        } => {
                            props = props
                                .with_attribute("Vertices", &vertex_count.to_string())
                                .with_attribute("Triangles", &triangle_count.to_string());
                        }
                        ProceduralPrimKind::PointInstancer {
                            point_count,
                            prototype_refs,
                        } => {
                            props = props.with_attribute("Points", &point_count.to_string());
                            if !prototype_refs.is_empty() {
                                props =
                                    props.with_attribute("Prototypes", &prototype_refs.join(", "));
                            }
                        }
                        ProceduralPrimKind::Scope => {}
                    }
                }
                self.selected_prim_properties = Some(props);
            } else {
                self.selected_prim_properties = Some(PrimProperties {
                    path: prim_path,
                    ..Default::default()
                });
            }
        }

        // Handle transform edit events from property inspector
        let transform_edit: Option<TransformEdit> = self
            .egui_ctx
            .data(|d| d.get_temp(egui::Id::new("transform_edit_event")));
        if let Some(edit) = transform_edit {
            self.egui_ctx
                .data_mut(|d| d.remove::<TransformEdit>(egui::Id::new("transform_edit_event")));

            if edit.committed {
                // Finalize: push undo command
                self.push_transform_command(
                    edit.instance_index,
                    edit.old_transform,
                    edit.new_transform,
                );
            } else {
                // Live preview: update GPU directly without undo
                let idx = edit.instance_index;
                let mat = edit.new_transform.to_matrix();
                if idx < self.current_transforms.len() {
                    self.current_transforms[idx] = mat;
                    self.culling.mark_dirty();
                    self.update_visible_instances();
                    // Restart Ivar at interaction scale during drag (throttled)
                    if self.ivar_state.mode == RenderMode::Ivar {
                        if self.ivar_state.should_restart() && self.ivar_state.world.is_some() {
                            self.restart_ivar_at_scale(self.ivar_state.interaction_scale());
                        }
                        self.ivar_state.last_interaction_time = Some(std::time::Instant::now());
                    }
                }
            }
        }

        // Handle Xform property changes from the property inspector panel
        if let Some(_xform_nid) = self.xform_property_changed.take() {
            if let Err(e) = self.reload_working_scene() {
                log::error!("Failed to reload after xform property change: {}", e);
            }
        }

        // Handle stage correction toggle (axis/unit) from top panel
        let stage_correction_changed: Option<bool> = self
            .egui_ctx
            .data(|d| d.get_temp(egui::Id::new("stage_correction_changed")));
        if stage_correction_changed == Some(true) {
            self.egui_ctx.data_mut(|d| {
                d.remove::<bool>(egui::Id::new("stage_correction_changed"));
            });
            if let Err(e) = self.reload_working_scene() {
                log::error!("Failed to reload after stage correction toggle: {}", e);
            }
        }

        // Handle set keyframe request from property inspector
        let keyframe_request: Option<u64> = self
            .egui_ctx
            .data(|d| d.get_temp(egui::Id::new("set_keyframe_request")));
        if let Some(instance_index) = keyframe_request {
            self.egui_ctx
                .data_mut(|d| d.remove::<u64>(egui::Id::new("set_keyframe_request")));
            self.set_keyframe(instance_index as usize);
        }

        // Update keyframe times for timeline markers
        self.update_keyframe_times();

        // Handle deferred camera projection change
        let projection_change: Option<String> = self
            .egui_ctx
            .data(|d| d.get_temp(egui::Id::new("camera_projection_change")));
        if let Some(change) = projection_change {
            self.egui_ctx
                .data_mut(|d| d.remove::<String>(egui::Id::new("camera_projection_change")));
            if change == "perspective" {
                self.camera.set_perspective();
            } else if let Some(preset_name) = change.strip_prefix("ortho:") {
                for preset in bif_math::OrthoPreset::all() {
                    if preset.display_name() == preset_name {
                        self.camera.set_ortho_preset(*preset);
                        break;
                    }
                }
            }
            self.update_camera();
        }

        // Handle edit layer export request
        let export_path: Option<String> = self
            .egui_ctx
            .data(|d| d.get_temp(egui::Id::new("export_edit_layer")));
        if let Some(path) = export_path {
            self.egui_ctx
                .data_mut(|d| d.remove::<String>(egui::Id::new("export_edit_layer")));
            match self.export_edit_layer(&path) {
                Ok(()) => log::info!("Edit layer exported to {}", path),
                Err(e) => log::error!("Failed to export edit layer: {}", e),
            }
        }

        // Handle node graph events (USD loading, render start, etc.)
        let node_graph_events: Vec<NodeGraphEvent> = self.egui_ctx.data(|d| {
            d.get_temp(egui::Id::new("node_graph_events"))
                .unwrap_or_default()
        });
        if !node_graph_events.is_empty() {
            // Clear the events
            self.egui_ctx
                .data_mut(|d| d.remove::<Vec<NodeGraphEvent>>(egui::Id::new("node_graph_events")));

            for event in node_graph_events {
                match event {
                    NodeGraphEvent::LoadUsdFile { path, node_id } => {
                        log::info!("Node graph: Loading USD file: {}", path);

                        // Remove old prototypes from this node (reload case)
                        if let Some(old_ids) = self.node_proto_map.remove(&node_id) {
                            for &pid in old_ids.iter().rev() {
                                self.remove_and_reindex_prototype(pid);
                            }
                        }

                        self.materials_dirty = true;
                        let proto_offset = self.working_scene.prototype_count();
                        match self.load_usd_scene(&path) {
                            Ok(()) => {
                                // Track which prototypes this UsdRead node owns
                                let new_proto_count = self.working_scene.prototype_count();
                                let proto_ids: Vec<usize> =
                                    (proto_offset..new_proto_count).collect();
                                if !proto_ids.is_empty() {
                                    log::info!("UsdRead {:?} owns protos {:?}", node_id, proto_ids);
                                    self.node_proto_map.insert(node_id, proto_ids);
                                }
                                self.node_graph_state.mark_node_loaded(&path);
                                log::info!("USD file loaded successfully: {}", path);
                            }
                            Err(e) => {
                                log::error!("Failed to load USD file: {}", e);
                                self.node_graph_state.mark_node_error(&path, e.to_string());
                            }
                        }
                    }
                    NodeGraphEvent::StartRender { spp } => {
                        log::info!("Node graph: Starting render with {} SPP", spp);
                        self.ivar_state.samples_per_pixel = spp;
                        self.ivar_state.mode = RenderMode::Ivar;
                        self.ivar_state.current_scale = 1;
                        self.ivar_state.last_interaction_time = None;
                        self.start_ivar_render();
                    }
                    NodeGraphEvent::ConvertTexturesToTx => {
                        #[cfg(feature = "oiio")]
                        {
                            let paths = self.collect_material_texture_paths();
                            if paths.is_empty() {
                                self.node_graph_state
                                    .mark_tx_conversion_complete("No textures".into());
                            } else {
                                log::info!("Converting {} textures to .tx", paths.len());
                                self.environment
                                    .start_tx_conversion(paths, self.texture_base_dir.clone());
                            }
                        }
                        #[cfg(not(feature = "oiio"))]
                        {
                            self.node_graph_state
                                .mark_tx_conversion_complete("OIIO not available".into());
                        }
                    }
                    NodeGraphEvent::LoadHdri {
                        path,
                        rotation,
                        intensity,
                        show_background,
                    } => {
                        log::info!("Node graph: Loading HDRI (async): {}", path);
                        self.node_graph_state.mark_hdri_loading(&path);
                        self.environment.start_hdri_load(
                            std::path::Path::new(&path),
                            rotation,
                            intensity,
                            show_background,
                        );
                    }
                    NodeGraphEvent::UpdateHdriParams {
                        rotation,
                        intensity,
                        show_background,
                    } => {
                        let rotation_rad = rotation.to_radians();
                        self.update_environment_params(intensity, rotation_rad, show_background);
                        // Restart Ivar so CPU path tracer reflects updated params.
                        // Throttled: slider drag fires every frame, avoid excessive cancel+spawn.
                        if self.ivar_state.mode == RenderMode::Ivar
                            && self.ivar_state.world.is_some()
                            && self.ivar_state.should_restart()
                        {
                            self.restart_ivar_at_scale(self.ivar_state.current_scale);
                        }
                    }
                    NodeGraphEvent::CreatePrimitive {
                        kind,
                        size,
                        node_id,
                    } => {
                        log::info!("Node graph: Creating {:?} primitive (size={})", kind, size);

                        // Read prim_path from the Primitive node for export naming
                        let prim_path = if let crate::node_graph::SceneNode::Primitive {
                            ref prim_path,
                            ..
                        } = &self.node_graph_state.snarl[node_id]
                        {
                            Some(prim_path.clone())
                        } else {
                            None
                        };

                        // Remove old prototype if re-creating (e.g. size change)
                        if let Some(old_ids) = self.node_proto_map.remove(&node_id) {
                            for &pid in old_ids.iter().rev() {
                                self.remove_and_reindex_prototype(pid);
                            }
                        }

                        self.materials_dirty = true;
                        match self.load_primitive(kind, size) {
                            Ok(proto_id) => {
                                // Update prototype name from node's prim_path (used during export)
                                if let Some(ref pp) = prim_path {
                                    if let Some(proto) =
                                        self.working_scene.prototypes.get_mut(proto_id)
                                    {
                                        std::sync::Arc::make_mut(proto).name = pp.as_str().into();
                                    }
                                }
                                self.node_proto_map.insert(node_id, vec![proto_id]);

                                // Recursively dirty all downstream nodes
                                crate::node_graph::propagate_dirty(
                                    node_id,
                                    &mut self.node_graph_state.snarl,
                                );

                                if let Err(e) = self.reload_working_scene() {
                                    log::error!("Failed to reload after primitive create: {}", e);
                                }
                            }
                            Err(e) => {
                                log::error!("Failed to create primitive: {}", e);
                            }
                        }
                    }
                    NodeGraphEvent::ScatterPointsCompute { node_id, params } => {
                        log::info!(
                            "Node graph: Scatter Points {:?}, count={}, seed={}",
                            params.source,
                            params.count,
                            params.seed
                        );

                        // Remove previous cloud for this node (if regenerating)
                        if let Some(old_cloud_id) = self.node_cloud_map.remove(&node_id) {
                            self.working_scene.remove_point_cloud(old_cloud_id);
                        }
                        // Clear old surface mapping (rebuilt in reload_working_scene)
                        self.node_scatter_surface_map.remove(&node_id);

                        let cloud = match params.source {
                            bif_core::PointSource::Surface => {
                                let scatter_mesh_idx = params.target_proto_id.unwrap_or(0);
                                if let Some(proto) =
                                    self.working_scene.prototypes.get(scatter_mesh_idx)
                                {
                                    let mesh = proto.mesh.clone();
                                    let mesh_transform = self
                                        .working_scene
                                        .instances()
                                        .iter()
                                        .find(|i| i.prototype_id == scatter_mesh_idx)
                                        .map(|i| i.model_matrix())
                                        .unwrap_or(bif_math::Mat4::IDENTITY);

                                    let config = bif_core::scatter::ScatterConfig {
                                        count: (params.count).min(params.max_point_limit) as usize,
                                        min_distance: params.min_distance,
                                        seed: params.seed,
                                        align_to_normal: params.align_to_normal,
                                        scale_range: (params.scale_min, params.scale_max),
                                        rotation_range: params.rotation_range.to_radians(),
                                    };

                                    let mut cloud = bif_core::scatter::scatter_on_surface(
                                        &mesh,
                                        &mesh_transform,
                                        params.scatter_mode,
                                        &config,
                                        vec![scatter_mesh_idx],
                                    );

                                    if params.relax_iterations > 0 {
                                        bif_core::scatter::repulsion_relax(
                                            &mut cloud.positions,
                                            params.relax_iterations,
                                            params.scale_radii,
                                            params.max_relax_radius,
                                            Some((&mesh, &mesh_transform)),
                                        );
                                    }

                                    // Track surface proto for hiding (rebuilt in reload_working_scene)
                                    self.node_scatter_surface_map
                                        .insert(node_id, scatter_mesh_idx);

                                    Some(cloud)
                                } else {
                                    log::warn!(
                                        "Scatter: no mesh at proto_id {} to scatter on",
                                        scatter_mesh_idx
                                    );
                                    None
                                }
                            }
                            bif_core::PointSource::Grid => {
                                let config = bif_core::scatter::ScatterConfig {
                                    seed: params.seed,
                                    scale_range: (params.scale_min, params.scale_max),
                                    rotation_range: params.rotation_range.to_radians(),
                                    ..Default::default()
                                };
                                let mut cloud = bif_core::scatter::generate_grid_points(
                                    params.grid_size,
                                    params.grid_spacing,
                                    params.max_point_limit,
                                    &config,
                                );
                                if params.relax_iterations > 0 {
                                    bif_core::scatter::repulsion_relax(
                                        &mut cloud.positions,
                                        params.relax_iterations,
                                        params.scale_radii,
                                        params.max_relax_radius,
                                        None,
                                    );
                                }
                                Some(cloud)
                            }
                            bif_core::PointSource::Sphere => {
                                let config = bif_core::scatter::ScatterConfig {
                                    seed: params.seed,
                                    scale_range: (params.scale_min, params.scale_max),
                                    rotation_range: params.rotation_range.to_radians(),
                                    ..Default::default()
                                };
                                let mut cloud = bif_core::scatter::generate_sphere_points(
                                    params.sphere_radius,
                                    params.count,
                                    params.sphere_on_surface,
                                    params.max_point_limit,
                                    &config,
                                );
                                if params.relax_iterations > 0 {
                                    bif_core::scatter::repulsion_relax(
                                        &mut cloud.positions,
                                        params.relax_iterations,
                                        params.scale_radii,
                                        params.max_relax_radius,
                                        None,
                                    );
                                }
                                Some(cloud)
                            }
                        };

                        if let Some(mut cloud) = cloud {
                            let cloud_id = self.next_cloud_id;
                            self.next_cloud_id += 1;
                            cloud.id = cloud_id;
                            self.node_cloud_map.insert(node_id, cloud_id);

                            let pt_count = cloud.positions.len();
                            self.working_scene.add_point_cloud(cloud);

                            // Upload point positions for preview
                            let all_positions: Vec<bif_math::Vec3> = self
                                .working_scene
                                .point_clouds
                                .iter()
                                .flat_map(|c| c.positions.iter().copied())
                                .collect();
                            self.point_preview.upload_points(
                                &self.device,
                                &self.queue,
                                &all_positions,
                            );
                            self.point_preview_params_dirty = true;

                            // Auto-enable point preview and sync color/size from node
                            self.point_preview.visible = true;
                            if let crate::node_graph::SceneNode::ScatterPoints {
                                point_size,
                                point_color,
                                ..
                            } = &self.node_graph_state.snarl[node_id]
                            {
                                self.point_preview.point_size = *point_size;
                                self.point_preview.color = *point_color;
                                self.point_preview_params_dirty = true;
                            }

                            log::info!("Scatter Points complete: {} points", pt_count);

                            // Reload scene to hide scatter surface geometry
                            if let Err(e) = self.reload_working_scene() {
                                log::error!("Failed to reload after scatter: {}", e);
                            }

                            // Recursively dirty all downstream nodes
                            crate::node_graph::propagate_dirty(
                                node_id,
                                &mut self.node_graph_state.snarl,
                            );
                        }
                    }
                    NodeGraphEvent::PointInstancerCompute {
                        node_id,
                        points_source_node,
                        proto_source_node,
                    } => {
                        // Resolve cloud ID from scatter node
                        let cloud_id = self.node_cloud_map.get(&points_source_node).copied();
                        // Resolve first prototype ID from primitive/USD node
                        let proto_id = self
                            .node_proto_map
                            .get(&proto_source_node)
                            .and_then(|ids| ids.first().copied());

                        // Helper: mark compute failed on the node (prevents infinite retry)
                        let mark_compute_failed =
                            |snarl: &mut egui_snarl::Snarl<crate::node_graph::SceneNode>,
                             nid: egui_snarl::NodeId| {
                                if let crate::node_graph::SceneNode::PointInstancer {
                                    is_computing,
                                    compute_failed,
                                    ..
                                } = &mut snarl[nid]
                                {
                                    *is_computing = false;
                                    *compute_failed = true;
                                }
                            };

                        // Read PointInstancer node's prim_path for export
                        let instancer_prim_path =
                            if let crate::node_graph::SceneNode::PointInstancer {
                                ref prim_path,
                                ..
                            } = &self.node_graph_state.snarl[node_id]
                            {
                                Some(prim_path.clone())
                            } else {
                                None
                            };

                        match (cloud_id, proto_id) {
                            (Some(cid), Some(pid)) => {
                                // Update cloud name from PointInstancer prim_path (used during export)
                                if let Some(ref prim_path) = instancer_prim_path {
                                    if let Some(cloud) = self
                                        .working_scene
                                        .point_clouds
                                        .iter_mut()
                                        .find(|c| c.id == cid)
                                    {
                                        cloud.name = prim_path.clone();
                                        // TODO: multi-prototype instancing not yet supported,
                                        // using first proto only. See node_proto_map .first().
                                        cloud.prototype_ids = vec![pid];
                                        self.scene_graph_dirty = true;
                                    }
                                }

                                // Find cloud in working scene by ID
                                let cloud =
                                    self.working_scene.point_clouds.iter().find(|c| c.id == cid);

                                if let Some(cloud) = cloud {
                                    let pt_count = cloud.positions.len();
                                    let expanded = cloud.expand_with_prototype(pid);
                                    let inst_count = expanded.len();
                                    self.instancer_results.insert(node_id, expanded);

                                    // Reload scene to rebuild GPU buffers with instancer instances
                                    if let Err(e) = self.reload_working_scene() {
                                        log::error!("Failed to reload after instancing: {}", e);
                                    }

                                    // Update node UI state
                                    if let crate::node_graph::SceneNode::PointInstancer {
                                        instance_count,
                                        is_instanced,
                                        is_computing,
                                        compute_failed,
                                        ..
                                    } = &mut self.node_graph_state.snarl[node_id]
                                    {
                                        *instance_count = inst_count;
                                        *is_instanced = true;
                                        *is_computing = false;
                                        *compute_failed = false;
                                    }

                                    log::info!(
                                        "Point Instancer: {} points x proto {} = {} instances",
                                        pt_count,
                                        pid,
                                        inst_count
                                    );
                                } else {
                                    log::warn!("Point Instancer: cloud {} not found in scene", cid);
                                    mark_compute_failed(&mut self.node_graph_state.snarl, node_id);
                                }
                            }
                            (None, _) => {
                                log::warn!(
                                    "Point Instancer: no cloud for source node {:?}",
                                    points_source_node
                                );
                                mark_compute_failed(&mut self.node_graph_state.snarl, node_id);
                            }
                            (_, None) => {
                                log::warn!(
                                    "Point Instancer: no prototype for source node {:?}",
                                    proto_source_node
                                );
                                mark_compute_failed(&mut self.node_graph_state.snarl, node_id);
                            }
                        }
                    }
                    NodeGraphEvent::InstancerInvalidate { node_id } => {
                        if self.instancer_results.remove(&node_id).is_some() {
                            if let Err(e) = self.reload_working_scene() {
                                log::error!("Failed to reload after instancer invalidate: {}", e);
                            }
                            log::info!("Instancer {:?} invalidated", node_id);
                        }
                    }
                    NodeGraphEvent::PointPreviewUpdate {
                        node_id,
                        point_size,
                        point_color,
                    } => {
                        // All scatter nodes share one PointPreviewRenderer — last emitter wins.
                        // node_id kept for diagnostics; per-node rendering is a future task.
                        log::trace!(
                            "PointPreviewUpdate from {:?}: size={}, color={:?}",
                            node_id,
                            point_size,
                            point_color,
                        );
                        self.point_preview.point_size = point_size;
                        self.point_preview.color = point_color;
                        self.point_preview_params_dirty = true;
                    }
                    NodeGraphEvent::ExportUsd {
                        node_id,
                        output_path,
                        as_sublayer,
                        export_root,
                    } => {
                        // Collect authored prims, graft prefix, and source USD path from upstream
                        let (authored_prims, graft_prefix, upstream_usd_path) =
                            collect_export_context(node_id, &self.node_graph_state.snarl);
                        // Auto-enable sublayer when upstream UsdRead exists
                        let effective_as_sublayer = as_sublayer || upstream_usd_path.is_some();
                        let config = bif_core::ExportConfig {
                            output_path: output_path.clone(),
                            source_usd_path: upstream_usd_path.or(self.loaded_usd_path.clone()),
                            as_sublayer: effective_as_sublayer,
                            export_root,
                            authored_prims,
                            graft_prefix,
                        };
                        match self.export_with_config(&config) {
                            Ok(result) => {
                                let status = format!("{}", result);
                                log::info!("USD export: {}", status);
                                // Update node status
                                if let SceneNode::UsdExport {
                                    is_exported,
                                    last_result,
                                    ..
                                } = &mut self.node_graph_state.snarl[node_id]
                                {
                                    *is_exported = true;
                                    *last_result = Some(status);
                                }
                            }
                            Err(e) => {
                                let err_msg = format!("{}", e);
                                log::error!("USD export failed: {}", err_msg);
                                if let SceneNode::UsdExport {
                                    is_exported,
                                    last_result,
                                    ..
                                } = &mut self.node_graph_state.snarl[node_id]
                                {
                                    *is_exported = false;
                                    *last_result = Some(format!("Error: {}", err_msg));
                                }
                            }
                        }
                    }
                    NodeGraphEvent::XformChanged { .. } => {
                        if let Err(e) = self.reload_working_scene() {
                            log::error!("Failed to reload after xform change: {}", e);
                        }
                    }
                    NodeGraphEvent::SetDisplayNode(id) => {
                        // Toggle: clicking the same node clears display
                        if self.node_graph_state.display_node == Some(id) {
                            self.node_graph_state.display_node = None;
                        } else {
                            self.node_graph_state.display_node = Some(id);
                        }
                        if let Err(e) = self.reload_working_scene() {
                            log::error!("Failed to reload after display change: {}", e);
                        }
                    }
                    NodeGraphEvent::SelectNode(_) => {
                        // Selection handled in render_node_graph
                    }
                    NodeGraphEvent::DeleteNode(node_id) => {
                        // Clean up scatter cloud
                        if let Some(cloud_id) = self.node_cloud_map.remove(&node_id) {
                            self.working_scene.remove_point_cloud(cloud_id);
                            let all_positions: Vec<bif_math::Vec3> = self
                                .working_scene
                                .point_clouds
                                .iter()
                                .flat_map(|c| c.positions.iter().copied())
                                .collect();
                            self.point_preview.upload_points(
                                &self.device,
                                &self.queue,
                                &all_positions,
                            );
                            self.point_preview_params_dirty = true;
                            log::info!("Deleted scatter node {:?} → cloud {}", node_id, cloud_id);
                        }

                        // Clean up scatter surface mapping (rebuilt in reload_working_scene)
                        self.node_scatter_surface_map.remove(&node_id);

                        // Clean up instancer results
                        if self.instancer_results.remove(&node_id).is_some() {
                            log::info!("Deleted instancer node {:?}", node_id);
                        }

                        // Clean up prototypes owned by this node
                        if let Some(proto_ids) = self.node_proto_map.remove(&node_id) {
                            log::info!("Deleting node {:?} → protos {:?}", node_id, proto_ids);
                            // Remove in reverse order so indices stay valid;
                            // remove_and_reindex_prototype handles re-indexing all maps
                            for &pid in proto_ids.iter().rev() {
                                self.remove_and_reindex_prototype(pid);
                            }
                            self.materials_dirty = true;
                        }

                        // Always reload after deletion — Xform/display-flag changes
                        // need scene rebuild even if no protos were directly owned
                        if let Err(e) = self.reload_working_scene() {
                            log::error!("Failed to reload after deletion: {}", e);
                        }

                        // If no loaded UsdRead nodes remain, clear USD stage state
                        let has_usd_read =
                            self.node_graph_state.snarl.node_ids().any(|(_, node)| {
                                matches!(
                                    node,
                                    SceneNode::UsdRead {
                                        is_loaded: true,
                                        ..
                                    }
                                )
                            });
                        if !has_usd_read {
                            self.usd_stage = None;
                            self.loaded_usd_path = None;
                            self.scene_browser_state =
                                crate::scene_browser::SceneBrowserState::new();
                            self.selected_prim_path = None;
                            self.selected_prim_properties = None;
                        }
                    }
                }
            }
        }

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
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        // Upload egui textures
        for (id, image_delta) in &full_output.textures_delta.set {
            self.egui_renderer
                .update_texture(&self.device, &self.queue, *id, image_delta);
        }

        // Prepare egui render pass
        self.egui_renderer.update_buffers(
            &self.device,
            &self.queue,
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
                self.point_preview.update_params(&self.queue, vp_w, vp_h);
                self.point_preview_last_vp = vp_size;
                self.point_preview_params_dirty = false;
            }
        }

        // Main render pass - dispatch based on render mode
        match self.ivar_state.mode {
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
                        .render_skybox(&mut skybox_pass, &self.camera_bind_group);
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
                    render_pass.set_bind_group(0, &self.camera_bind_group, &[]);
                    render_pass.set_bind_group(1, &self.material_bind_group, &[]);
                    render_pass.set_bind_group(2, &self.texture_bind_group, &[]);
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
                                self.queue.write_buffer(
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
                        .render(&mut render_pass, &self.camera_bind_group);

                    // Render ground grid after opaque geometry (transparent, reads depth)
                    if self.show_grid {
                        self.grid.render(&mut render_pass, &self.camera_bind_group);
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
                if self.ivar_state.check_camera_dirty(&self.camera) {
                    let scale = self.ivar_state.interaction_scale();
                    if self.ivar_state.should_restart() {
                        if self.ivar_state.world.is_some() {
                            self.restart_ivar_at_scale(scale);
                        } else {
                            // No BVH yet, record desired scale for when build completes
                            self.ivar_state.current_scale = scale;
                            self.start_ivar_render();
                        }
                    }
                    self.ivar_state.last_interaction_time = Some(std::time::Instant::now());
                }

                // 2. Poll for scene build completion
                self.poll_scene_build();

                // 3. Poll for completed buckets / pass completion (before settle timer
                //    so is_pass_in_flight() reflects actual state)
                self.poll_ivar_messages();

                // 4. Settle timer → refine to next resolution level
                if let Some(last_time) = self.ivar_state.last_interaction_time {
                    let elapsed_ms = last_time.elapsed().as_millis() as u32;
                    // Use shorter timeout for coarse→medium refinement
                    let threshold = if self.ivar_state.current_scale >= 4 {
                        self.ivar_state.settle_timeout_ms / 2
                    } else {
                        self.ivar_state.settle_timeout_ms
                    };
                    if elapsed_ms >= threshold
                        && self.ivar_state.current_scale > 1
                        && !self.ivar_state.is_pass_in_flight()
                    {
                        let next_scale = (self.ivar_state.current_scale / 2).max(1);
                        self.restart_ivar_at_scale(next_scale);
                        self.ivar_state.last_interaction_time = Some(std::time::Instant::now());
                    }
                }

                // 5. Full-res progressive: start next pass only at scale==1
                if self.ivar_state.current_scale == 1
                    && self.ivar_state.world.is_some()
                    && !self.ivar_state.is_pass_in_flight()
                    && self.ivar_state.needs_more_passes()
                {
                    self.start_progressive_pass();
                }

                // 6. First-time scene build
                if self.ivar_state.world.is_none()
                    && self.ivar_state.build_status == BuildStatus::NotStarted
                {
                    self.ivar_state.current_scale = 1;
                    self.ivar_state.last_interaction_time = None;
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
                    ivar_pass.set_pipeline(&self.ivar_pipeline);
                    ivar_pass.set_bind_group(0, &self.ivar_bind_group, &[]);
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

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }
}

/// Canonicalize a path and strip the Windows UNC `\\?\` prefix that
/// `std::fs::canonicalize` adds. USD's SdfLayer cannot resolve UNC paths.
fn canonicalize_for_usd(path: &str) -> String {
    let result = std::fs::canonicalize(path)
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| path.to_string());
    result.strip_prefix(r"\\?\").unwrap_or(&result).to_string()
}

/// Walk upstream from an export node and collect AuthoredPrims, graft prefix,
/// and the source USD path from an upstream UsdRead node.
fn collect_export_context(
    export_node: egui_snarl::NodeId,
    snarl: &egui_snarl::Snarl<SceneNode>,
) -> (Vec<bif_core::AuthoredPrim>, Option<String>, Option<String>) {
    let upstream = crate::node_graph::collect_upstream_nodes(export_node, snarl);
    let mut authored_prims = Vec::new();
    let mut graft_prefix: Option<String> = None;
    let mut source_usd_path: Option<String> = None;

    for &nid in &upstream {
        match &snarl[nid] {
            SceneNode::UsdPrim {
                prim_path,
                prim_type,
                kind,
                specifier,
            } => {
                authored_prims.push(bif_core::AuthoredPrim {
                    path: prim_path.clone(),
                    prim_type: *prim_type,
                    kind: *kind,
                    specifier: *specifier,
                });
            }
            SceneNode::GraftBranches { destination_path } => {
                graft_prefix = Some(destination_path.clone());
            }
            SceneNode::UsdRead {
                file_path,
                is_loaded: true,
                ..
            } if !file_path.is_empty() => {
                source_usd_path = Some(canonicalize_for_usd(file_path));
            }
            _ => {}
        }
    }

    (authored_prims, graft_prefix, source_usd_path)
}
