use anyhow::Result;
use std::sync::atomic::Ordering;

use crate::batch_render::BatchMessage;
use crate::environment_manager::IblResult;
use crate::gpu_types::InstanceData;
use crate::ivar_state::{self, BatchRenderStatus, BuildStatus, CameraSource, RenderMode};
use crate::node_graph::{render_node_graph, NodeGraphEvent};
use crate::property_inspector::{
    render_property_inspector, reset_transform_edit_cache, PrimProperties, TransformEdit,
};
use crate::scene_browser::{self, EmptyPrimProvider, PrimDataProvider};
use crate::Renderer;

impl Renderer {
    /// Render a frame with the given clear color
    pub fn render(
        &mut self,
        clear_color: wgpu::Color,
        window: &winit::window::Window,
    ) -> Result<()> {
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
                    // Set Ivar CPU environment
                    self.ivar_state.environment = Some(ivar_env);
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
        let ivar_progress = self.ivar_state.progress();
        let ivar_buckets_completed = self.ivar_state.buckets_completed;
        let ivar_total_buckets = self.ivar_state.buckets.len();
        let ivar_elapsed = self.ivar_state.elapsed_secs();
        let ivar_render_complete = self.ivar_state.render_complete;
        let ivar_spp = self.ivar_state.samples_per_pixel;

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
                                // Scene is built, show render progress

                                // Progress bar
                                let progress_bar = egui::ProgressBar::new(ivar_progress / 100.0)
                                    .text(format!("{:.1}%", ivar_progress));
                                ui.add(progress_bar);

                                // Stats
                                ui.label(format!(
                                    "Buckets: {} / {}",
                                    ivar_buckets_completed, ivar_total_buckets
                                ));
                                ui.label(format!("SPP: {}", ivar_spp));
                                ui.label(format!("Time: {:.1}s", ivar_elapsed));

                                if ivar_render_complete {
                                    ui.colored_label(egui::Color32::GREEN, "✓ Render Complete");
                                } else if ivar_buckets_completed > 0 {
                                    ui.colored_label(egui::Color32::YELLOW, "⟳ Rendering...");
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

                        // TODO: Add progressive multi-pass rendering (1 SPP preview → full SPP)
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
                        // Triangle count: full mesh triangles + 12 triangles per LOD box
                        let full_mesh_tris = triangles_per_instance * visible_instances;
                        let box_tris = 12 * lod_box_instances;
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
                        // Use USD stage if available, otherwise empty provider
                        let empty_provider = EmptyPrimProvider;
                        let provider: &dyn PrimDataProvider = match &self.usd_stage {
                            Some(stage) => stage.as_ref(),
                            None => &empty_provider,
                        };

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
                    let et_ref =
                        editable_transform.as_ref().map(|(idx, t)| (*idx, t));
                    render_property_inspector(
                        ui,
                        self.selected_prim_properties.as_ref(),
                        et_ref,
                    );
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
            let provider: Option<&dyn PrimDataProvider> = self
                .usd_stage
                .as_ref()
                .map(|s| s.as_ref() as &dyn PrimDataProvider);
            if let Some(info) = provider.and_then(|p| p.get_prim_info(&prim_path)) {
                self.selected_prim_properties = Some(PrimProperties::from_display_info(&info));
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
                    self.update_visible_instances();
                }
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
                    NodeGraphEvent::LoadUsdFile(path) => {
                        log::info!("Node graph: Loading USD file: {}", path);
                        match self.load_usd_scene(&path) {
                            Ok(()) => {
                                // Mark the node as loaded successfully
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
                    }
                    NodeGraphEvent::CreatePrimitive {
                        kind,
                        size,
                        node_id,
                    } => {
                        log::info!("Node graph: Creating {:?} primitive (size={})", kind, size);
                        match self.load_primitive(kind, size) {
                            Ok(proto_id) => {
                                self.node_proto_map.insert(node_id, proto_id);
                            }
                            Err(e) => {
                                log::error!("Failed to create primitive: {}", e);
                            }
                        }
                    }
                    NodeGraphEvent::SelectNode(_) => {
                        // Selection handled in render_node_graph
                    }
                    NodeGraphEvent::DeleteNode(node_id) => {
                        if let Some(proto_id) = self.node_proto_map.remove(&node_id) {
                            log::info!(
                                "Node graph: Deleting node {:?} → proto {}",
                                node_id,
                                proto_id
                            );
                            match self.remove_primitive(proto_id) {
                                Ok(()) => {
                                    // Re-index remaining entries: IDs above removed shift down
                                    for v in self.node_proto_map.values_mut() {
                                        if *v > proto_id {
                                            *v -= 1;
                                        }
                                    }
                                }
                                Err(e) => {
                                    log::error!("Failed to remove primitive: {}", e);
                                    self.node_proto_map.insert(node_id, proto_id);
                                }
                            }
                        } else {
                            log::info!("Node graph: Deleted node {:?} (no scene data)", node_id);
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
                // Check camera dirty and restart render if needed
                if self.ivar_state.check_camera_dirty(&self.camera)
                    && !self.ivar_state.render_complete
                {
                    log::info!("Camera moved - restarting Ivar render");
                    self.start_ivar_render();
                }

                // Poll for scene build completion
                self.poll_scene_build();

                // Poll for completed buckets
                self.poll_ivar_messages();

                // Upload current image buffer to texture (uses selected AOV channel)
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
