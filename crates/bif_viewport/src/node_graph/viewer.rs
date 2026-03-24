//! SceneNodeViewer: SnarlViewer implementation for the node graph UI.

use egui_snarl::{
    ui::{PinInfo, SnarlViewer},
    InPin, NodeId, OutPin, Snarl,
};

use super::ops::mark_node_dirty;
use super::{NodeGraphEvent, ScatterPointsParams, SceneNode};

/// Resolve which node is connected to a given input pin.
///
/// Returns `Some(NodeId)` of the upstream node if pin is connected, `None` otherwise.
fn resolve_input_connection(inputs: &[InPin], input_index: usize) -> Option<NodeId> {
    inputs
        .get(input_index)
        .and_then(|pin| pin.remotes.first())
        .map(|out_pin_id| out_pin_id.node)
}

/// Viewer implementation for the scene node graph
pub(crate) struct SceneNodeViewer {
    /// Events to be processed by the parent
    pub events: Vec<NodeGraphEvent>,
    /// Which node has the display flag (for blue indicator)
    pub display_node: Option<NodeId>,
}

impl SceneNodeViewer {
    pub fn new(display_node: Option<NodeId>) -> Self {
        Self {
            events: Vec::new(),
            display_node,
        }
    }
}

impl Default for SceneNodeViewer {
    fn default() -> Self {
        Self::new(None)
    }
}

impl SnarlViewer<SceneNode> for SceneNodeViewer {
    fn title(&mut self, node: &SceneNode) -> String {
        node.name().to_string()
    }

    fn inputs(&mut self, node: &SceneNode) -> usize {
        node.input_count()
    }

    fn outputs(&mut self, node: &SceneNode) -> usize {
        node.output_count()
    }

    fn show_input(
        &mut self,
        pin: &InPin,
        ui: &mut egui::Ui,
        _scale: f32,
        snarl: &mut Snarl<SceneNode>,
    ) -> PinInfo {
        let node = &snarl[pin.id.node];
        if let Some((name, pin_type)) = node.input_pin(pin.id.input) {
            ui.label(name);
            PinInfo::circle().with_fill(pin_type.color())
        } else {
            PinInfo::circle()
        }
    }

    fn show_output(
        &mut self,
        pin: &OutPin,
        ui: &mut egui::Ui,
        _scale: f32,
        snarl: &mut Snarl<SceneNode>,
    ) -> PinInfo {
        let node = &snarl[pin.id.node];
        if let Some((name, pin_type)) = node.output_pin(pin.id.output) {
            ui.label(name);
            PinInfo::circle().with_fill(pin_type.color())
        } else {
            PinInfo::circle()
        }
    }

    // TODO: decouple auto-compute from show_body — cook triggers should come from
    // dependency graph evaluation, not UI rendering (nodes scrolled out of view won't cook)
    fn show_body(
        &mut self,
        node_id: NodeId,
        inputs: &[InPin],
        _outputs: &[OutPin],
        ui: &mut egui::Ui,
        _scale: f32,
        snarl: &mut Snarl<SceneNode>,
    ) {
        // Detect click on node body to select it
        if ui.rect_contains_pointer(ui.max_rect()) && ui.input(|i| i.pointer.any_pressed()) {
            self.events.push(NodeGraphEvent::SelectNode(node_id));
        }

        // Display flag indicator (blue dot like Houdini)
        if self.display_node == Some(node_id) {
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
                ui.painter().circle_filled(
                    rect.center(),
                    4.0,
                    egui::Color32::from_rgb(80, 140, 255),
                );
                ui.colored_label(egui::Color32::from_rgb(80, 140, 255), "Display");
            });
        }

        let node = &mut snarl[node_id];

        match node {
            SceneNode::UsdRead {
                file_path,
                is_loaded,
                error,
            } => {
                ui.horizontal(|ui| {
                    ui.label("File:");
                    if ui.text_edit_singleline(file_path).changed() {
                        // Reset status when path changes
                        *is_loaded = false;
                        *error = None;
                    }
                });

                ui.horizontal(|ui| {
                    if ui.button("Browse...").clicked() {
                        // Open file dialog
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("USD Files", &["usda", "usdc", "usd"])
                            .add_filter("All Files", &["*"])
                            .pick_file()
                        {
                            *file_path = path.display().to_string();
                            *is_loaded = false;
                            *error = None;
                            // Emit load event
                            self.events.push(NodeGraphEvent::LoadUsdFile {
                                path: file_path.clone(),
                                node_id,
                            });
                        }
                    }

                    if ui.button("Load").clicked() && !file_path.is_empty() {
                        self.events.push(NodeGraphEvent::LoadUsdFile {
                            path: file_path.clone(),
                            node_id,
                        });
                    }
                });

                if *is_loaded {
                    ui.colored_label(egui::Color32::GREEN, "\u{2713} Loaded");
                } else if let Some(err) = error {
                    ui.colored_label(egui::Color32::RED, format!("\u{2717} {}", err));
                }
            }
            SceneNode::IvarRender {
                spp,
                is_rendering,
                is_converting_tx,
                tx_status,
            } => {
                ui.horizontal(|ui| {
                    ui.label("SPP:");
                    ui.add(egui::DragValue::new(spp).range(1..=1024));
                });

                if *is_rendering {
                    ui.colored_label(egui::Color32::YELLOW, "Rendering...");
                } else if ui.button("Render").clicked() {
                    self.events.push(NodeGraphEvent::StartRender { spp: *spp });
                    *is_rendering = true;
                }

                ui.separator();

                if *is_converting_tx {
                    ui.colored_label(egui::Color32::YELLOW, "Converting .tx...");
                } else {
                    ui.horizontal(|ui| {
                        if ui.button("Convert to .tx").clicked() {
                            self.events.push(NodeGraphEvent::ConvertTexturesToTx);
                            *is_converting_tx = true;
                            *tx_status = None;
                        }
                        if ui.button("Clear .tx").clicked() {
                            self.events.push(NodeGraphEvent::ClearTxCache);
                        }
                    });
                }
                if let Some(status) = tx_status {
                    ui.colored_label(egui::Color32::GREEN, status.as_str());
                }
            }
            SceneNode::Primitive {
                kind,
                size,
                is_created,
                prim_path,
            } => {
                ui.horizontal(|ui| {
                    ui.label("Size:");
                    if ui
                        .add(egui::DragValue::new(size).speed(0.01).range(0.01..=100.0))
                        .changed()
                    {
                        *is_created = false; // Need to recreate
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Path:");
                    ui.text_edit_singleline(prim_path);
                });

                if !*is_created {
                    self.events.push(NodeGraphEvent::CreatePrimitive {
                        kind: *kind,
                        size: *size,
                        node_id,
                    });
                    *is_created = true;
                }
                ui.colored_label(egui::Color32::GREEN, "Created");
            }
            SceneNode::ScatterPoints {
                source,
                count,
                max_point_limit,
                seed,
                scatter_mode,
                min_distance,
                align_to_normal,
                grid_size,
                grid_spacing,
                sphere_radius,
                sphere_on_surface,
                relax_iterations,
                scale_radii,
                max_relax_radius,
                scale_min,
                scale_max,
                rotation_range,
                target_proto_id: _,
                is_computed,
                point_size,
                point_color,
            } => {
                // Source dropdown
                ui.horizontal(|ui| {
                    ui.label("Source:");
                    egui::ComboBox::from_id_salt("scatter_source")
                        .selected_text(match source {
                            bif_core::PointSource::Surface => "Surface",
                            bif_core::PointSource::Grid => "Grid",
                            bif_core::PointSource::Sphere => "Sphere",
                        })
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_value(source, bif_core::PointSource::Surface, "Surface")
                                .changed()
                            {
                                *is_computed = false;
                            }
                            if ui
                                .selectable_value(source, bif_core::PointSource::Grid, "Grid")
                                .changed()
                            {
                                *is_computed = false;
                            }
                            if ui
                                .selectable_value(source, bif_core::PointSource::Sphere, "Sphere")
                                .changed()
                            {
                                *is_computed = false;
                            }
                        });
                });

                // Source-specific params
                match source {
                    bif_core::PointSource::Surface => {
                        ui.horizontal(|ui| {
                            ui.label("Count:");
                            if ui
                                .add(egui::DragValue::new(count).range(1..=100_000))
                                .changed()
                            {
                                *is_computed = false;
                            }
                        });

                        ui.horizontal(|ui| {
                            ui.label("Mode:");
                            let mut is_poisson =
                                *scatter_mode == bif_core::scatter::ScatterMode::PoissonDisk;
                            if ui.checkbox(&mut is_poisson, "Poisson Disk").changed() {
                                *scatter_mode = if is_poisson {
                                    bif_core::scatter::ScatterMode::PoissonDisk
                                } else {
                                    bif_core::scatter::ScatterMode::Random
                                };
                                *is_computed = false;
                            }
                        });

                        if *scatter_mode == bif_core::scatter::ScatterMode::PoissonDisk {
                            ui.horizontal(|ui| {
                                ui.label("Min Dist:");
                                if ui
                                    .add(
                                        egui::DragValue::new(min_distance)
                                            .speed(0.01)
                                            .range(0.01..=100.0),
                                    )
                                    .changed()
                                {
                                    *is_computed = false;
                                }
                            });
                        }

                        ui.horizontal(|ui| {
                            ui.label("Seed:");
                            let mut seed_val = *seed as i64;
                            if ui
                                .add(egui::DragValue::new(&mut seed_val).range(0..=999_999))
                                .changed()
                            {
                                *seed = seed_val as u64;
                                *is_computed = false;
                            }
                        });

                        if ui.checkbox(align_to_normal, "Align to Normal").changed() {
                            *is_computed = false;
                        }
                    }
                    bif_core::PointSource::Grid => {
                        ui.horizontal(|ui| {
                            ui.label("Size X:");
                            if ui
                                .add(
                                    egui::DragValue::new(&mut grid_size[0])
                                        .speed(0.1)
                                        .range(0.0..=1000.0),
                                )
                                .changed()
                            {
                                *is_computed = false;
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label("Size Y:");
                            if ui
                                .add(
                                    egui::DragValue::new(&mut grid_size[1])
                                        .speed(0.1)
                                        .range(0.0..=1000.0),
                                )
                                .changed()
                            {
                                *is_computed = false;
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label("Size Z:");
                            if ui
                                .add(
                                    egui::DragValue::new(&mut grid_size[2])
                                        .speed(0.1)
                                        .range(0.0..=1000.0),
                                )
                                .changed()
                            {
                                *is_computed = false;
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label("Spacing:");
                            if ui
                                .add(
                                    egui::DragValue::new(grid_spacing)
                                        .speed(0.01)
                                        .range(0.01..=100.0),
                                )
                                .changed()
                            {
                                *is_computed = false;
                            }
                        });
                    }
                    bif_core::PointSource::Sphere => {
                        ui.horizontal(|ui| {
                            ui.label("Count:");
                            if ui
                                .add(egui::DragValue::new(count).range(1..=100_000))
                                .changed()
                            {
                                *is_computed = false;
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label("Radius:");
                            if ui
                                .add(
                                    egui::DragValue::new(sphere_radius)
                                        .speed(0.1)
                                        .range(0.01..=1000.0),
                                )
                                .changed()
                            {
                                *is_computed = false;
                            }
                        });
                        if ui.checkbox(sphere_on_surface, "Surface Only").changed() {
                            *is_computed = false;
                        }
                        ui.horizontal(|ui| {
                            ui.label("Seed:");
                            let mut seed_val = *seed as i64;
                            if ui
                                .add(egui::DragValue::new(&mut seed_val).range(0..=999_999))
                                .changed()
                            {
                                *seed = seed_val as u64;
                                *is_computed = false;
                            }
                        });
                    }
                }

                // Common: max point limit
                ui.horizontal(|ui| {
                    ui.label("Max Pts:");
                    if ui
                        .add(egui::DragValue::new(max_point_limit).range(1..=1_000_000))
                        .changed()
                    {
                        *is_computed = false;
                    }
                });

                // Relax section
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Relax Iters:");
                    if ui
                        .add(egui::DragValue::new(relax_iterations).range(0..=100))
                        .changed()
                    {
                        *is_computed = false;
                    }
                });
                if *relax_iterations > 0 {
                    ui.horizontal(|ui| {
                        ui.label("Scale Radii:");
                        if ui
                            .add(
                                egui::DragValue::new(scale_radii)
                                    .speed(0.01)
                                    .range(0.01..=10.0),
                            )
                            .changed()
                        {
                            *is_computed = false;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Max Radius:");
                        if ui
                            .add(
                                egui::DragValue::new(max_relax_radius)
                                    .speed(0.01)
                                    .range(0.01..=100.0),
                            )
                            .changed()
                        {
                            *is_computed = false;
                        }
                    });
                }

                // Per-point attributes
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Scale:");
                    let changed_min = ui
                        .add(
                            egui::DragValue::new(scale_min)
                                .speed(0.01)
                                .range(0.01..=10.0),
                        )
                        .changed();
                    ui.label("-");
                    let changed_max = ui
                        .add(
                            egui::DragValue::new(scale_max)
                                .speed(0.01)
                                .range(0.01..=10.0),
                        )
                        .changed();
                    if changed_min || changed_max {
                        *is_computed = false;
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("Rotation:");
                    if ui
                        .add(
                            egui::DragValue::new(rotation_range)
                                .speed(1.0)
                                .suffix("deg")
                                .range(0.0..=360.0),
                        )
                        .changed()
                    {
                        *is_computed = false;
                    }
                });

                // Compute / Regenerate button
                let emit_event = |events: &mut Vec<NodeGraphEvent>| {
                    events.push(NodeGraphEvent::ScatterPointsCompute {
                        node_id,
                        params: ScatterPointsParams {
                            source: *source,
                            count: *count,
                            max_point_limit: *max_point_limit,
                            seed: *seed,
                            scatter_mode: *scatter_mode,
                            min_distance: *min_distance,
                            align_to_normal: *align_to_normal,
                            grid_size: *grid_size,
                            grid_spacing: *grid_spacing,
                            sphere_radius: *sphere_radius,
                            sphere_on_surface: *sphere_on_surface,
                            relax_iterations: *relax_iterations,
                            scale_radii: *scale_radii,
                            max_relax_radius: *max_relax_radius,
                            scale_min: *scale_min,
                            scale_max: *scale_max,
                            rotation_range: *rotation_range,
                            target_proto_id: None,
                        },
                    });
                };

                // Auto-compute: check if inputs are satisfied
                let inputs_satisfied = match source {
                    bif_core::PointSource::Surface => resolve_input_connection(inputs, 0).is_some(),
                    bif_core::PointSource::Grid | bif_core::PointSource::Sphere => true,
                };

                if !*is_computed && inputs_satisfied {
                    emit_event(&mut self.events);
                    *is_computed = true;
                }

                // Status display
                if *is_computed {
                    ui.colored_label(egui::Color32::GREEN, "Computed");
                } else if !inputs_satisfied {
                    ui.colored_label(egui::Color32::YELLOW, "Waiting for input");
                }

                // Point preview controls (cosmetic only, no recompute)
                ui.separator();
                let mut preview_changed = false;
                ui.horizontal(|ui| {
                    ui.label("Pt Size:");
                    if ui
                        .add(
                            egui::DragValue::new(point_size)
                                .speed(0.5)
                                .range(1.0..=20.0),
                        )
                        .changed()
                    {
                        preview_changed = true;
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Color:");
                    if ui
                        .color_edit_button_rgba_unmultiplied(point_color)
                        .changed()
                    {
                        preview_changed = true;
                    }
                });
                if preview_changed {
                    self.events.push(NodeGraphEvent::PointPreviewUpdate {
                        node_id,
                        point_size: *point_size,
                        point_color: *point_color,
                    });
                }
            }
            SceneNode::PointInstancer {
                instance_count,
                is_instanced,
                is_computing,
                compute_failed,
                prim_path,
            } => {
                ui.horizontal(|ui| {
                    ui.label("Path:");
                    ui.text_edit_singleline(prim_path);
                });

                let points_node = resolve_input_connection(inputs, 0);
                let proto_node = resolve_input_connection(inputs, 1);
                let both_connected = points_node.is_some() && proto_node.is_some();

                // Auto-invalidate: inputs disconnected but still marked instanced
                if !both_connected && *is_instanced {
                    self.events
                        .push(NodeGraphEvent::InstancerInvalidate { node_id });
                    *is_instanced = false;
                    *is_computing = false;
                    *compute_failed = false;
                    *instance_count = 0;
                }

                // Auto-compute: both inputs connected, not yet instanced, not failed
                if both_connected && !*is_instanced && !*is_computing && !*compute_failed {
                    let (Some(points_source), Some(proto_source)) = (points_node, proto_node)
                    else {
                        return;
                    };
                    self.events.push(NodeGraphEvent::PointInstancerCompute {
                        node_id,
                        points_source_node: points_source,
                        proto_source_node: proto_source,
                    });
                    *is_computing = true;
                }

                // Status display
                if *is_instanced {
                    ui.colored_label(
                        egui::Color32::GREEN,
                        format!("{} instances", instance_count),
                    );
                } else if *is_computing {
                    ui.colored_label(egui::Color32::YELLOW, "Computing...");
                } else if *compute_failed {
                    ui.colored_label(egui::Color32::RED, "Compute failed");
                } else {
                    let mut need = String::new();
                    if points_node.is_none() {
                        need.push_str("points");
                    }
                    if proto_node.is_none() {
                        if !need.is_empty() {
                            need.push_str(", ");
                        }
                        need.push_str("proto");
                    }
                    ui.colored_label(egui::Color32::YELLOW, format!("Need: {}", need));
                }
            }
            SceneNode::UsdExport {
                output_path,
                as_sublayer,
                export_root,
                is_exported,
                last_result,
            } => {
                ui.horizontal(|ui| {
                    ui.label("Path:");
                    ui.text_edit_singleline(output_path);
                });

                ui.horizontal(|ui| {
                    if ui.button("Browse...").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("USD Files", &["usda", "usdc"])
                            .set_file_name("export.usda")
                            .save_file()
                        {
                            *output_path = path.display().to_string();
                            *is_exported = false;
                            *last_result = None;
                        }
                    }
                });

                ui.checkbox(as_sublayer, "As Sublayer");

                ui.horizontal(|ui| {
                    ui.label("Root:");
                    ui.text_edit_singleline(export_root);
                });

                if !output_path.is_empty() && ui.button("Export").clicked() {
                    self.events.push(NodeGraphEvent::ExportUsd {
                        node_id,
                        output_path: output_path.clone(),
                        as_sublayer: *as_sublayer,
                        export_root: export_root.clone(),
                    });
                }

                if *is_exported {
                    if let Some(ref result) = last_result {
                        ui.colored_label(egui::Color32::GREEN, result.as_str());
                    }
                } else if let Some(ref result) = last_result {
                    // Error case
                    ui.colored_label(egui::Color32::RED, result.as_str());
                }
            }
            SceneNode::Xform {
                translate,
                rotate,
                scale,
                prim_filter,
            } => {
                let axis_colors = [
                    egui::Color32::from_rgb(220, 80, 80),  // X = red
                    egui::Color32::from_rgb(80, 200, 80),  // Y = green
                    egui::Color32::from_rgb(80, 120, 220), // Z = blue
                ];
                let axis_labels = ["X", "Y", "Z"];

                let mut changed = false;

                ui.label("Translate");
                ui.horizontal(|ui| {
                    for i in 0..3 {
                        ui.colored_label(axis_colors[i], axis_labels[i]);
                        if ui
                            .add(egui::DragValue::new(&mut translate[i]).speed(0.1))
                            .changed()
                        {
                            changed = true;
                        }
                    }
                });

                ui.label("Rotate");
                ui.horizontal(|ui| {
                    for i in 0..3 {
                        ui.colored_label(axis_colors[i], axis_labels[i]);
                        if ui
                            .add(
                                egui::DragValue::new(&mut rotate[i])
                                    .speed(1.0)
                                    .suffix("\u{b0}"),
                            )
                            .changed()
                        {
                            changed = true;
                        }
                    }
                });

                ui.label("Scale");
                ui.horizontal(|ui| {
                    for i in 0..3 {
                        ui.colored_label(axis_colors[i], axis_labels[i]);
                        if ui
                            .add(
                                egui::DragValue::new(&mut scale[i])
                                    .speed(0.01)
                                    .range(0.001..=1000.0),
                            )
                            .changed()
                        {
                            changed = true;
                        }
                    }
                });

                // Prim filter (placeholder, non-functional V1)
                ui.horizontal(|ui| {
                    ui.label("Filter:");
                    ui.add_enabled(
                        false,
                        egui::TextEdit::singleline(prim_filter)
                            .hint_text("all prims (future)")
                            .desired_width(100.0),
                    );
                });

                if changed {
                    self.events.push(NodeGraphEvent::XformChanged { node_id });
                }
            }
            SceneNode::UsdPrim {
                prim_path,
                prim_type,
                kind,
                specifier,
            } => {
                ui.horizontal(|ui| {
                    ui.label("Path:");
                    ui.text_edit_singleline(prim_path);
                });
                // Type combo
                ui.horizontal(|ui| {
                    ui.label("Type:");
                    egui::ComboBox::from_id_salt("prim_type")
                        .selected_text(format!("{}", prim_type))
                        .show_ui(ui, |ui| {
                            for t in bif_core::usd::UsdPrimType::ALL {
                                ui.selectable_value(prim_type, t, format!("{}", t));
                            }
                        });
                });
                // Kind combo
                ui.horizontal(|ui| {
                    ui.label("Kind:");
                    egui::ComboBox::from_id_salt("prim_kind")
                        .selected_text(format!("{}", kind))
                        .show_ui(ui, |ui| {
                            for k in bif_core::usd::UsdKind::ALL {
                                ui.selectable_value(kind, k, format!("{}", k));
                            }
                        });
                });
                // Specifier combo
                ui.horizontal(|ui| {
                    ui.label("Spec:");
                    egui::ComboBox::from_id_salt("prim_spec")
                        .selected_text(format!("{}", specifier))
                        .show_ui(ui, |ui| {
                            for s in bif_core::usd::UsdSpecifier::ALL {
                                ui.selectable_value(specifier, s, format!("{}", s));
                            }
                        });
                });
                // Prim path label
                ui.colored_label(egui::Color32::from_rgb(100, 200, 100), prim_path.as_str());
            }
            SceneNode::GraftBranches { destination_path } => {
                ui.horizontal(|ui| {
                    ui.label("Dest:");
                    ui.text_edit_singleline(destination_path);
                });
                // Show connected branch count
                let connected = (0..4)
                    .filter(|&i| !inputs.get(i).is_none_or(|p| p.remotes.is_empty()))
                    .count();
                ui.label(format!("{}/4 branches connected", connected));
                // Destination path label
                ui.colored_label(
                    egui::Color32::from_rgb(100, 200, 100),
                    destination_path.as_str(),
                );
            }
            SceneNode::HdriEnvironment {
                file_path,
                is_loaded,
                is_loading,
                rotation,
                intensity,
                show_background,
                error,
                last_load_secs,
                last_compute_secs,
            } => {
                ui.horizontal(|ui| {
                    ui.label("File:");
                    ui.add_enabled_ui(!*is_loading, |ui| {
                        if ui.text_edit_singleline(file_path).changed() {
                            *is_loaded = false;
                            *error = None;
                        }
                    });
                });

                ui.horizontal(|ui| {
                    ui.add_enabled_ui(!*is_loading, |ui| {
                        if ui.button("Browse...").clicked() {
                            if let Some(path) = rfd::FileDialog::new()
                                .add_filter("HDR Files", &["hdr", "exr"])
                                .add_filter("All Files", &["*"])
                                .pick_file()
                            {
                                *file_path = path.display().to_string();
                                *is_loaded = false;
                                *error = None;
                                self.events.push(NodeGraphEvent::LoadHdri {
                                    path: file_path.clone(),
                                    rotation: *rotation,
                                    intensity: *intensity,
                                    show_background: *show_background,
                                });
                            }
                        }

                        if ui.button("Load").clicked() && !file_path.is_empty() {
                            self.events.push(NodeGraphEvent::LoadHdri {
                                path: file_path.clone(),
                                rotation: *rotation,
                                intensity: *intensity,
                                show_background: *show_background,
                            });
                        }
                    });
                });

                let mut params_changed = false;

                ui.horizontal(|ui| {
                    ui.label("Rotation:");
                    if ui
                        .add(egui::DragValue::new(rotation).speed(1.0).suffix("deg"))
                        .changed()
                    {
                        params_changed = true;
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("Intensity:");
                    if ui
                        .add(
                            egui::DragValue::new(intensity)
                                .speed(0.01)
                                .range(0.0..=10.0),
                        )
                        .changed()
                    {
                        params_changed = true;
                    }
                });

                if let Some(load_secs) = last_load_secs {
                    ui.label(format!("Load: {:.2}s", load_secs));
                }
                if let Some(compute_secs) = last_compute_secs {
                    ui.label(format!("IBL: {:.2}s", compute_secs));
                }

                if ui.checkbox(show_background, "Show Background").changed() {
                    params_changed = true;
                }

                if params_changed && *is_loaded {
                    self.events.push(NodeGraphEvent::UpdateHdriParams {
                        rotation: *rotation,
                        intensity: *intensity,
                        show_background: *show_background,
                    });
                }

                if *is_loading {
                    ui.colored_label(egui::Color32::YELLOW, "Generating IBL...");
                } else if *is_loaded {
                    ui.colored_label(egui::Color32::GREEN, "Loaded");
                } else if let Some(err) = error {
                    ui.colored_label(egui::Color32::RED, format!("Error: {}", err));
                }
            }
        }
    }

    fn has_body(&mut self, _node: &SceneNode) -> bool {
        true
    }

    fn connect(&mut self, from: &OutPin, to: &InPin, snarl: &mut Snarl<SceneNode>) {
        // Check if connection is valid (pin types must match)
        let from_node = &snarl[from.id.node];
        let to_node = &snarl[to.id.node];

        if let (Some((_, from_type)), Some((_, to_type))) = (
            from_node.output_pin(from.id.output),
            to_node.input_pin(to.id.input),
        ) {
            if from_type == to_type {
                snarl.connect(from.id, to.id);
                // Dirty the target so auto-compute re-triggers
                mark_node_dirty(to.id.node, snarl);
            }
        }
    }

    fn disconnect(&mut self, from: &OutPin, to: &InPin, snarl: &mut Snarl<SceneNode>) {
        // Emit invalidation before disconnecting so stale results get cleaned up
        if matches!(
            snarl[to.id.node],
            SceneNode::PointInstancer {
                is_instanced: true,
                ..
            }
        ) {
            self.events.push(NodeGraphEvent::InstancerInvalidate {
                node_id: to.id.node,
            });
        }
        snarl.disconnect(from.id, to.id);
        // Dirty the target so auto-compute re-evaluates
        mark_node_dirty(to.id.node, snarl);
    }

    fn has_node_menu(&mut self, _node: &SceneNode) -> bool {
        true
    }

    fn show_node_menu(
        &mut self,
        node: NodeId,
        _inputs: &[InPin],
        _outputs: &[OutPin],
        ui: &mut egui::Ui,
        _scale: f32,
        snarl: &mut Snarl<SceneNode>,
    ) {
        // Show "Set/Clear Display" for scene-output nodes
        let is_scene_output = matches!(
            snarl[node],
            SceneNode::UsdRead { .. }
                | SceneNode::PointInstancer { .. }
                | SceneNode::Primitive { .. }
                | SceneNode::IvarRender { .. }
                | SceneNode::Xform { .. }
                | SceneNode::UsdPrim { .. }
                | SceneNode::GraftBranches { .. }
        );
        if is_scene_output {
            let label = if self.display_node == Some(node) {
                "Clear Display"
            } else {
                "Set Display"
            };
            if ui.button(label).clicked() {
                self.events.push(NodeGraphEvent::SetDisplayNode(node));
                ui.close_menu();
            }
        }
        if ui.button("Delete").clicked() {
            self.events.push(NodeGraphEvent::DeleteNode(node));
            ui.close_menu();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_snarl::{InPinId, OutPinId, Snarl};

    #[test]
    fn test_resolve_input_connection() {
        let mut snarl = Snarl::<SceneNode>::new();
        let scatter_id = snarl.insert_node(egui::pos2(0.0, 0.0), SceneNode::scatter_points());
        let cube_id = snarl.insert_node(
            egui::pos2(0.0, 100.0),
            SceneNode::primitive(bif_core::PrimitiveKind::Cube),
        );
        let instancer_id = snarl.insert_node(egui::pos2(200.0, 0.0), SceneNode::point_instancer());

        // Before connecting: both inputs should resolve to None
        let pins_before: Vec<InPin> = (0..2)
            .map(|i| {
                snarl.in_pin(InPinId {
                    node: instancer_id,
                    input: i,
                })
            })
            .collect();
        assert!(resolve_input_connection(&pins_before, 0).is_none());
        assert!(resolve_input_connection(&pins_before, 1).is_none());

        // Connect scatter -> instancer input 0 (points)
        snarl.connect(
            OutPinId {
                node: scatter_id,
                output: 0,
            },
            InPinId {
                node: instancer_id,
                input: 0,
            },
        );
        // Connect cube -> instancer input 1 (proto)
        snarl.connect(
            OutPinId {
                node: cube_id,
                output: 0,
            },
            InPinId {
                node: instancer_id,
                input: 1,
            },
        );

        // Re-read pins after connecting
        let pins_after: Vec<InPin> = (0..2)
            .map(|i| {
                snarl.in_pin(InPinId {
                    node: instancer_id,
                    input: i,
                })
            })
            .collect();
        assert_eq!(resolve_input_connection(&pins_after, 0), Some(scatter_id));
        assert_eq!(resolve_input_connection(&pins_after, 1), Some(cube_id));
        // Out of bounds returns None
        assert!(resolve_input_connection(&pins_after, 5).is_none());
    }
}
