use crate::ivar_state::{self, BuildStatus, RenderMode};
use crate::scene_browser::{self, CompositeProvider, PrimDataProvider};
use crate::{DisplaySettings, PurposeMode};

/// All data the left stats panel needs, passed by value or reference
/// to avoid borrowing `self` inside the closure.
pub(crate) struct StatsPanelParams<'a> {
    // Mutable state refs
    pub ivar_state: &'a mut crate::ivar_state::IvarState,
    pub scene_browser_state: &'a mut crate::scene_browser::SceneBrowserState,

    // Read-only state refs
    pub usd_stage: &'a Option<std::sync::Arc<bif_core::usd::UsdStage>>,
    pub timeline_state: &'a crate::TimelineState,
    pub cached_scene_graph: &'a crate::scene_browser::CachedSceneGraph,

    // Read-only display values (scalar copies)
    pub fps: f32,
    pub camera: &'a bif_math::Camera,
    pub num_instances: u32,
    pub visible_instances: u32,
    pub lod_box_instances: u32,
    pub triangles_per_instance: u32,
    pub mesh_bounds_min: bif_math::Vec3,
    pub mesh_bounds_max: bif_math::Vec3,
    pub size: (u32, u32),
    pub instance_count: usize,
    pub mesh_triangle_count: usize,
    pub edit_override_count: usize,
    pub edit_keyframe_count: usize,

    // Ivar read-only copies
    pub ivar_buckets_completed: usize,
    pub ivar_total_buckets: usize,
    pub ivar_elapsed: f32,
    pub ivar_render_complete: bool,
    pub ivar_accumulated_spp: u32,
    pub ivar_current_scale: u32,

    // Mutable value refs (local copies, written back by caller)
    pub render_mode: &'a mut RenderMode,
    pub gnomon_size: &'a mut u32,
    pub lod_max_polys: &'a mut u32,
    pub ivar_target_spp: &'a mut u32,
    pub ivar_nav_quality: &'a mut u32,

    /// Display settings (purpose toggle, LOD enable)
    pub display_settings: &'a mut DisplaySettings,
}

/// Renders the left stats/settings panel body.
pub(crate) fn render_stats_panel(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    p: &mut StatsPanelParams<'_>,
) {
    ui.heading("BIF Viewer");
    ui.separator();

    // Render Mode Dropdown (Houdini-style)
    ui.horizontal(|ui| {
        ui.label("Renderer:");
        egui::ComboBox::from_id_salt("render_mode")
            .selected_text(p.render_mode.display_name())
            .show_ui(ui, |ui| {
                ui.selectable_value(p.render_mode, RenderMode::Vulkan, "Vulkan");
                ui.selectable_value(p.render_mode, RenderMode::Ivar, "Ivar");
            });
    });

    // Show Ivar stats when in Ivar mode
    if *p.render_mode == RenderMode::Ivar {
        ui.separator();
        ui.label("Ivar Path Tracer");

        // Show build status or render progress
        match p.ivar_state.build_status {
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
                    p.instance_count, p.mesh_triangle_count
                ));
            }
            BuildStatus::Failed => {
                ui.colored_label(egui::Color32::RED, "⚠ Scene build failed");
            }
            BuildStatus::Complete => {
                // Progressive SPP progress
                let spp_progress = if *p.ivar_target_spp > 0 {
                    p.ivar_accumulated_spp as f32 / *p.ivar_target_spp as f32
                } else {
                    0.0
                };
                let progress_bar = egui::ProgressBar::new(spp_progress).text(format!(
                    "{}/{} SPP",
                    p.ivar_accumulated_spp, *p.ivar_target_spp
                ));
                ui.add(progress_bar);

                // Current pass bucket progress
                if !p.ivar_render_complete && p.ivar_total_buckets > 0 {
                    let bucket_frac = p.ivar_buckets_completed as f32 / p.ivar_total_buckets as f32;
                    ui.add(
                        egui::ProgressBar::new(bucket_frac)
                            .text(format!(
                                "Pass {}: {}/{}",
                                p.ivar_accumulated_spp,
                                p.ivar_buckets_completed,
                                p.ivar_total_buckets
                            ))
                            .desired_width(ui.available_width()),
                    );
                }

                // Target SPP slider
                ui.horizontal(|ui| {
                    ui.label("Target:");
                    ui.add(egui::Slider::new(p.ivar_target_spp, 1..=256).text("SPP"));
                });

                // Navigation preview quality slider
                ui.horizontal(|ui| {
                    ui.label("Nav Quality:");
                    ui.add(
                        egui::Slider::new(p.ivar_nav_quality, 1..=3).custom_formatter(
                            |v, _| match v as u32 {
                                1 => "1/2".to_string(),
                                2 => "1/4".to_string(),
                                3 => "1/8".to_string(),
                                _ => format!("1/{}", 2u32.pow(v as u32)),
                            },
                        ),
                    );
                });

                // Pixel filter dropdown
                ui.horizontal(|ui| {
                    ui.label("Filter:");
                    let current_filter = p.ivar_state.pixel_filter.filter;
                    egui::ComboBox::from_id_salt("pixel_filter")
                        .selected_text(current_filter.display_name())
                        .show_ui(ui, |ui| {
                            for &f in bif_renderer::PixelFilter::all() {
                                if ui
                                    .selectable_value(
                                        &mut p.ivar_state.pixel_filter.filter,
                                        f,
                                        f.display_name(),
                                    )
                                    .changed()
                                {
                                    p.ivar_state.pixel_filter.radius = f.default_radius();
                                    // Signal restart: mixing different filter weights is wrong
                                    ctx.data_mut(|d| {
                                        d.insert_temp(egui::Id::new("filter_changed"), true)
                                    });
                                }
                            }
                        });
                });

                // Sampler mode dropdown
                ui.horizontal(|ui| {
                    ui.label("Sampler:");
                    let current_sampler = p.ivar_state.sampler_mode;
                    egui::ComboBox::from_id_salt("sampler_mode")
                        .selected_text(current_sampler.display_name())
                        .show_ui(ui, |ui| {
                            for &s in bif_renderer::SamplerMode::all() {
                                if ui
                                    .selectable_value(
                                        &mut p.ivar_state.sampler_mode,
                                        s,
                                        s.display_name(),
                                    )
                                    .changed()
                                {
                                    ctx.data_mut(|d| {
                                        d.insert_temp(egui::Id::new("filter_changed"), true)
                                    });
                                }
                            }
                        });
                });

                // Show current preview scale when not at full res
                if p.ivar_current_scale > 1 {
                    ui.colored_label(
                        egui::Color32::YELLOW,
                        format!("Preview: 1/{}", p.ivar_current_scale),
                    );
                }

                ui.label(format!("Time: {:.1}s", p.ivar_elapsed));

                // Auto-denoise checkbox (feature-gated)
                #[cfg(feature = "oidn")]
                {
                    ui.checkbox(&mut p.ivar_state.auto_denoise, "Auto-denoise");
                }

                if p.ivar_render_complete {
                    ui.colored_label(egui::Color32::GREEN, "Render Complete");

                    // Denoise button (feature-gated)
                    #[cfg(feature = "oidn")]
                    {
                        if p.ivar_state.denoise.is_denoised {
                            ui.colored_label(egui::Color32::from_rgb(100, 200, 255), "Denoised");
                        } else if p.ivar_state.denoise.in_progress {
                            ui.colored_label(egui::Color32::YELLOW, "Denoising...");
                        } else if !p.ivar_state.auto_denoise
                            && ui.button("Denoise (OIDN)").clicked()
                        {
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
                } else if p.ivar_accumulated_spp > 0 {
                    ui.colored_label(egui::Color32::YELLOW, "Refining...");
                }
            }
        }

        // AOV preview dropdown
        ui.horizontal(|ui| {
            ui.label("Preview:");
            egui::ComboBox::from_id_salt("aov_preview")
                .selected_text(p.ivar_state.preview_aov.display_name())
                .show_ui(ui, |ui| {
                    for channel in ivar_state::AovChannel::all() {
                        ui.selectable_value(
                            &mut p.ivar_state.preview_aov,
                            *channel,
                            channel.display_name(),
                        );
                    }
                });
        });

        // SHARC Radiance Cache settings
        ui.separator();
        ui.collapsing("SHARC Cache", |ui| {
            let cfg = &mut p.ivar_state.radiance_cache_config;
            let mut changed = false;
            let mut enabled = cfg.enabled;
            if ui.checkbox(&mut enabled, "Enabled").changed() {
                cfg.enabled = enabled;
                changed = true;
            }
            ui.horizontal(|ui| {
                ui.label("Cell Size:");
                if ui
                    .add(egui::Slider::new(&mut cfg.cell_size, 0.01..=10.0).logarithmic(true))
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
                                .selectable_value(&mut cfg.buffer_size, size, name)
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
            if let Some(ref cache) = p.ivar_state.radiance_cache {
                ui.label(format!(
                    "Hit: {:.1}%  Occ: {:.1}%",
                    cache.hit_rate() * 100.0,
                    cache.occupancy() * 100.0
                ));
            }

            // Rebuild cache if config changed
            if changed {
                if cfg.enabled {
                    let cache = std::sync::Arc::new(bif_renderer::RadianceCache::new(cfg.clone()));
                    p.ivar_state.radiance_cache = Some(cache);
                } else {
                    p.ivar_state.radiance_cache = None;
                }
            }
        });

        // Rebuild Scene button
        ui.separator();
        // Note: Can't call self.invalidate_ivar_scene() here due to borrow rules
        // Using ctx.data_mut() to store the request
        if ui.button("Rebuild Scene").clicked() {
            ctx.data_mut(|d| d.insert_temp(egui::Id::new("rebuild_scene_requested"), true));
        }
        ui.label("↻ Rebuild if geometry changes");
    }

    ui.separator();

    // FPS Counter
    ui.label(format!("FPS: {:.1}", p.fps));
    ui.separator();

    // Scene Stats
    ui.collapsing("Scene Stats", |ui| {
        ui.label(format!("Instances: {} total", p.num_instances));
        let total_visible = p.visible_instances + p.lod_box_instances;
        ui.label(format!(
            "Visible: {} ({:.0}%)",
            total_visible,
            if p.num_instances > 0 {
                (total_visible as f32 / p.num_instances as f32) * 100.0
            } else {
                0.0
            }
        ));
        ui.label(format!("  Full mesh: {}", p.visible_instances));
        ui.label(format!("  Box LOD: {}", p.lod_box_instances));
        // Triangle count: full mesh tris + LOD_BOX_TRIANGLES per box
        const LOD_BOX_TRIANGLES: u32 = 12; // cube = 6 faces * 2 tris
        let full_mesh_tris = p.triangles_per_instance * p.visible_instances;
        let box_tris = LOD_BOX_TRIANGLES * p.lod_box_instances;
        ui.label(format!(
            "Triangles: {} ({}+{})",
            full_mesh_tris + box_tris,
            full_mesh_tris,
            box_tris
        ));
        ui.label(format!("Tris/Instance: {}", p.triangles_per_instance));

        ui.separator();
        ui.label("LOD Budget Control:");
        // Slider for max polys (in millions for readability)
        let max_millions = (*p.lod_max_polys as f32 / 1_000_000.0).max(0.1);
        let mut millions = max_millions;
        ui.add(
            egui::Slider::new(&mut millions, 0.1..=100.0)
                .logarithmic(true)
                .text("Max M tris")
                .suffix("M"),
        );
        if (millions - max_millions).abs() > 0.001 {
            *p.lod_max_polys = (millions * 1_000_000.0) as u32;
        }
        let budget_used = (full_mesh_tris as f32 / *p.lod_max_polys as f32 * 100.0).min(100.0);
        ui.label(format!("Budget: {:.0}% used", budget_used));

        ui.separator();
        ui.checkbox(&mut p.display_settings.lod_enabled, "Enable LOD");
        ui.horizontal(|ui| {
            ui.label("Purpose:");
            egui::ComboBox::from_id_salt("display_purpose")
                .selected_text(match p.display_settings.purpose_mode {
                    PurposeMode::Render => "Render",
                    PurposeMode::Proxy => "Proxy",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut p.display_settings.purpose_mode,
                        PurposeMode::Render,
                        "Render",
                    );
                    ui.selectable_value(
                        &mut p.display_settings.purpose_mode,
                        PurposeMode::Proxy,
                        "Proxy",
                    );
                });
        });
    });

    ui.separator();

    // Camera Stats
    ui.collapsing("Camera", |ui| {
        ui.label(format!(
            "Position: ({:.2}, {:.2}, {:.2})",
            p.camera.position.x, p.camera.position.y, p.camera.position.z
        ));
        ui.label(format!(
            "Target: ({:.2}, {:.2}, {:.2})",
            p.camera.target.x, p.camera.target.y, p.camera.target.z
        ));
        ui.label(format!("Distance: {:.2}", p.camera.distance));
        ui.label(format!("Yaw: {:.2}°", p.camera.yaw.to_degrees()));
        ui.label(format!("Pitch: {:.2}°", p.camera.pitch.to_degrees()));
        ui.label(format!("FOV: {:.2}°", p.camera.fov_y.to_degrees()));
        ui.label(format!("Near: {:.2}", p.camera.near));
        ui.label(format!("Far: {:.2}", p.camera.far));

        ui.label("Press F to frame mesh");
    });

    ui.separator();

    // Mesh Info
    ui.collapsing("Mesh Bounds", |ui| {
        let mesh_center = (p.mesh_bounds_min + p.mesh_bounds_max) * 0.5;
        let mesh_size = (p.mesh_bounds_max - p.mesh_bounds_min).length();

        ui.label(format!(
            "Bounds Min: ({:.2}, {:.2}, {:.2})",
            p.mesh_bounds_min.x, p.mesh_bounds_min.y, p.mesh_bounds_min.z
        ));
        ui.label(format!(
            "Bounds Max: ({:.2}, {:.2}, {:.2})",
            p.mesh_bounds_max.x, p.mesh_bounds_max.y, p.mesh_bounds_max.z
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
        ui.label(format!("Resolution: {}x{}", p.size.0, p.size.1));
        ui.label(format!("Aspect: {:.3}", p.size.0 as f32 / p.size.1 as f32));
        ui.add(egui::Slider::new(p.gnomon_size, 40..=120).text("Gnomon Size"));
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
        let settings = &mut p.ivar_state.batch_settings;

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
                    if let Some(ref stage) = p.usd_stage {
                        if let Ok(paths) = stage.camera_paths() {
                            for path in paths {
                                let is_selected = matches!(
                                    &settings.camera_source,
                                    ivar_state::CameraSource::UsdCamera(p) if p == &path
                                );
                                if ui.selectable_label(is_selected, &path).clicked() {
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
        if ui.button("Use Timeline Range").clicked() && p.timeline_state.has_range() {
            settings.start_frame = p.timeline_state.start_frame as i32;
            settings.end_frame = p.timeline_state.end_frame as i32;
        }

        ui.horizontal(|ui| {
            ui.label("Step:");
            ui.add(
                egui::DragValue::new(&mut settings.frame_step)
                    .speed(1.0)
                    .range(1..=100),
            );
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
                        ui.selectable_value(&mut settings.compression, *comp, comp.display_name());
                    }
                });
        });

        // Pixel filter
        ui.horizontal(|ui| {
            ui.label("Filter:");
            egui::ComboBox::from_id_salt("batch_pixel_filter")
                .selected_text(settings.pixel_filter.filter.display_name())
                .show_ui(ui, |ui| {
                    for &f in bif_renderer::PixelFilter::all() {
                        if ui
                            .selectable_value(
                                &mut settings.pixel_filter.filter,
                                f,
                                f.display_name(),
                            )
                            .changed()
                        {
                            settings.pixel_filter.radius = f.default_radius();
                        }
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
                    ui.add(
                        egui::DragValue::new(&mut aov.depth_near)
                            .speed(0.1)
                            .range(0.001..=aov.depth_far)
                            .prefix("Near: "),
                    );
                    ui.add(
                        egui::DragValue::new(&mut aov.depth_far)
                            .speed(10.0)
                            .range(aov.depth_near..=100000.0)
                            .prefix("Far: "),
                    );
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
                    d.insert_temp(
                        egui::Id::new("sync_viewport_to_usd_camera"),
                        cam_path.clone(),
                    )
                });
            }
        }

        ui.separator();

        // Render button and status on same line
        let status = &p.ivar_state.batch_status;
        let is_rendering = matches!(status, ivar_state::BatchRenderStatus::Rendering { .. });
        let can_render = !settings.output_directory.is_empty() && !is_rendering;

        ui.horizontal(|ui| {
            if ui
                .add_enabled(can_render, egui::Button::new("Render"))
                .clicked()
            {
                ctx.data_mut(|d| d.insert_temp(egui::Id::new("start_batch_render"), true));
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
            let overall = ((*current_frame - 1) as f32 + frame_progress) / *total_frames as f32;
            ui.add(
                egui::ProgressBar::new(overall)
                    .text(format!("Frame {}/{}", current_frame, total_frames)),
            );
            if ui.button("Cancel").clicked() {
                ctx.data_mut(|d| d.insert_temp(egui::Id::new("cancel_batch_render"), true));
            }
        }
    });

    // Export Edits
    let has_edits = p.edit_override_count > 0 || p.edit_keyframe_count > 0;
    if has_edits {
        ui.separator();
        ui.collapsing("Export Edits", |ui| {
            ui.label(format!(
                "{} overrides, {} keyframed",
                p.edit_override_count, p.edit_keyframe_count
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
            p.usd_stage
                .as_ref()
                .map(|s| s.as_ref() as &dyn PrimDataProvider),
            p.cached_scene_graph,
        );
        let provider: &dyn PrimDataProvider = &composite;

        // Store selection change request in temp data for processing after egui run
        if let Some(new_selection) =
            scene_browser::render_scene_browser(ui, p.scene_browser_state, provider)
        {
            ctx.data_mut(|d| {
                d.insert_temp(egui::Id::new("prim_selection_changed"), new_selection);
            });
        }
    });
}
