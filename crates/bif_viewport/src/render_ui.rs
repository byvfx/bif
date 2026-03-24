use crate::app_event::{AppEvent, EventBus};
use crate::ivar_state::{self, BuildStatus, RenderMode};
use crate::theme;
use crate::{DisplaySettings, PurposeMode};

/// All data the left render-settings panel needs, passed by value or reference
/// to avoid borrowing `self` inside the closure.
pub(crate) struct StatsPanelParams<'a> {
    // Mutable state refs
    pub ivar_state: &'a mut crate::ivar_state::IvarState,

    // Read-only state refs
    pub usd_stage: &'a Option<std::sync::Arc<bif_core::usd::UsdStage>>,
    pub timeline_state: &'a crate::TimelineState,

    // Read-only display values (scalar copies)
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
    pub ivar_target_spp: &'a mut u32,
    pub ivar_nav_quality: &'a mut u32,
    pub lod_max_polys: &'a mut u32,
    pub display_settings: &'a mut DisplaySettings,
}

/// Renders the left stats/settings panel body.
pub(crate) fn render_stats_panel(
    ui: &mut egui::Ui,
    event_bus: &mut EventBus,
    p: &mut StatsPanelParams<'_>,
) {
    // Render Mode Dropdown (Houdini-style)
    ui.horizontal(|ui| {
        ui.label("Renderer:");
        egui::ComboBox::from_id_salt("render_mode")
            .selected_text(p.render_mode.display_name())
            .show_ui(ui, |ui| {
                ui.selectable_value(p.render_mode, RenderMode::Vulkan, "Vulkan");
                ui.selectable_value(p.render_mode, RenderMode::Ivar, "Ivar");
            })
            .response
            .on_hover_text("Switch between Vulkan rasterizer and Ivar path tracer");
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
                ui.colored_label(theme::STATUS_ERROR, "⚠ Scene build failed");
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
                    ui.add(egui::Slider::new(p.ivar_target_spp, 1..=256).text("SPP"))
                        .on_hover_text("Samples per pixel — higher = less noise, slower");
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
                    )
                    .on_hover_text("Preview resolution during camera movement");
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
                                    event_bus.emit(AppEvent::FilterChanged);
                                }
                            }
                        })
                        .response
                        .on_hover_text("Anti-aliasing filter applied to the final image");
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
                                    event_bus.emit(AppEvent::FilterChanged);
                                }
                            }
                        })
                        .response
                        .on_hover_text("Random sampling strategy for path tracing");
                });

                // Default sky toggle
                if ui
                    .checkbox(&mut p.ivar_state.use_sky_gradient, "Default Sky")
                    .on_hover_text("Use default gradient sky when no HDRI is loaded")
                    .changed()
                {
                    event_bus.emit(AppEvent::FilterChanged);
                }

                // Show current preview scale when not at full res
                if p.ivar_current_scale > 1 {
                    ui.colored_label(
                        theme::STATUS_WARNING,
                        format!("Preview: 1/{}", p.ivar_current_scale),
                    );
                }

                ui.label(format!("Time: {:.1}s", p.ivar_elapsed));

                // Auto-denoise checkbox (feature-gated)
                #[cfg(feature = "oidn")]
                {
                    ui.checkbox(&mut p.ivar_state.auto_denoise, "Auto-denoise")
                        .on_hover_text("Automatically denoise when render completes");
                }

                if p.ivar_render_complete {
                    ui.colored_label(theme::STATUS_OK, "Render Complete");

                    // Denoise button (feature-gated)
                    #[cfg(feature = "oidn")]
                    {
                        if p.ivar_state.denoise.is_denoised {
                            ui.colored_label(theme::STATUS_INFO, "Denoised");
                        } else if p.ivar_state.denoise.in_progress {
                            ui.colored_label(theme::STATUS_WARNING, "Denoising...");
                        } else if !p.ivar_state.auto_denoise
                            && ui
                                .button("Denoise (OIDN)")
                                .on_hover_text("Run Intel OIDN denoiser on current render")
                                .clicked()
                        {
                            event_bus.emit(AppEvent::DenoiseRequested);
                        }
                    }
                    #[cfg(not(feature = "oidn"))]
                    {
                        ui.add_enabled(false, egui::Button::new("Denoise (OIDN)"))
                            .on_disabled_hover_text("Build with --features oidn");
                    }
                } else if p.ivar_accumulated_spp > 0 {
                    ui.colored_label(theme::STATUS_WARNING, "Refining...");
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
                })
                .response
                .on_hover_text("Select render channel to preview in the viewport");
        });

        // SHARC Radiance Cache settings
        ui.separator();
        ui.collapsing("SHARC Cache", |ui| {
            let cfg = &mut p.ivar_state.radiance_cache_config;
            let mut changed = false;
            let mut enabled = cfg.enabled;
            if ui
                .checkbox(&mut enabled, "Enabled")
                .on_hover_text("Enable radiance cache for faster indirect lighting")
                .changed()
            {
                cfg.enabled = enabled;
                changed = true;
            }
            ui.horizontal(|ui| {
                ui.label("Cell Size:");
                if ui
                    .add(egui::Slider::new(&mut cfg.cell_size, 0.01..=10.0).logarithmic(true))
                    .on_hover_text("Spatial resolution of cache cells (smaller = more detail)")
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
                    })
                    .response
                    .on_hover_text("Memory allocated for radiance cache entries");
            });
            ui.horizontal(|ui| {
                ui.label("Min Samples:");
                if ui
                    .add(egui::Slider::new(&mut cfg.min_samples, 1..=16))
                    .on_hover_text("Minimum samples before a result is stored in the cache")
                    .changed()
                {
                    changed = true;
                }
            });
            ui.horizontal(|ui| {
                ui.label("Min Bounce:");
                if ui
                    .add(egui::Slider::new(&mut cfg.min_bounce_depth, 1..=4))
                    .on_hover_text("Minimum bounce depth before cache lookups are used")
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
        if ui
            .button("Rebuild Scene")
            .on_hover_text("Force rebuild of Embree scene geometry")
            .clicked()
        {
            event_bus.emit(AppEvent::RebuildScene);
        }
        ui.label("↻ Rebuild if geometry changes");
    }

    // Display settings (LOD, purpose)
    ui.separator();
    ui.collapsing("Display", |ui| {
        ui.checkbox(&mut p.display_settings.lod_enabled, "Enable LOD")
            .on_hover_text(
                "Replace distant instances with bounding boxes to reduce triangle count",
            );
        // LOD budget slider (in millions)
        let max_millions = (*p.lod_max_polys as f32 / 1_000_000.0).max(0.1);
        let mut millions = max_millions;
        ui.add(
            egui::Slider::new(&mut millions, 0.1..=100.0)
                .logarithmic(true)
                .text("Max M tris")
                .suffix("M"),
        )
        .on_hover_text("Triangle budget — instances beyond this limit are replaced with boxes");
        if (millions - max_millions).abs() > 0.001 {
            *p.lod_max_polys = (millions * 1_000_000.0) as u32;
        }
        ui.horizontal(|ui| {
            ui.label("Purpose:");
            egui::ComboBox::from_id_salt("display_purpose")
                .selected_text(match p.display_settings.purpose_mode {
                    PurposeMode::Render => "Default+Render",
                    PurposeMode::Proxy => "Default+Proxy",
                    PurposeMode::All => "All",
                    PurposeMode::Guide => "Default+Guide",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut p.display_settings.purpose_mode,
                        PurposeMode::Render,
                        "Default+Render",
                    );
                    ui.selectable_value(
                        &mut p.display_settings.purpose_mode,
                        PurposeMode::Proxy,
                        "Default+Proxy",
                    );
                    ui.selectable_value(
                        &mut p.display_settings.purpose_mode,
                        PurposeMode::All,
                        "All",
                    );
                    ui.selectable_value(
                        &mut p.display_settings.purpose_mode,
                        PurposeMode::Guide,
                        "Default+Guide",
                    );
                })
                .response
                .on_hover_text("USD display purpose filter — controls which prims are shown");
        });
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
                event_bus.emit(AppEvent::SyncUsdCamera(cam_path.clone()));
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
                event_bus.emit(AppEvent::StartBatchRender);
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
                        theme::STATUS_OK,
                        format!("Done ({:.1}s)", total_elapsed_secs),
                    );
                }
                ivar_state::BatchRenderStatus::Cancelled => {
                    ui.colored_label(theme::STATUS_WARNING, "Cancelled");
                }
                ivar_state::BatchRenderStatus::Failed(msg) => {
                    ui.colored_label(theme::STATUS_ERROR, format!("Failed: {}", msg));
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
                event_bus.emit(AppEvent::CancelBatchRender);
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
                    event_bus.emit(AppEvent::ExportEditLayer(path));
                }
            }
        });
    }
}
