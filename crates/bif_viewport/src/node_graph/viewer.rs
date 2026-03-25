//! SceneNodeViewer: SnarlViewer implementation for the node graph UI.

use std::collections::HashSet;

use egui_snarl::{
    ui::{PinInfo, SnarlViewer},
    InPin, NodeId, OutPin, Snarl,
};

use super::ops::mark_node_dirty;
use super::{GraphNodeId, NodeGraphEvent, ScatterPointsParams, SceneNode};
use crate::persistence::EvalMode;
use crate::theme;

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
pub(crate) struct SceneNodeViewer<'a> {
    /// Events to be processed by the parent
    pub events: Vec<NodeGraphEvent>,
    /// Which node has the display flag (for blue indicator)
    pub display_node: Option<NodeId>,
    /// Currently selected node (for visual highlight)
    pub selected_node: Option<NodeId>,
    /// Current evaluation mode
    pub eval_mode: EvalMode,
    /// Nodes needing re-evaluation (for dirty indicator)
    pub dirty_nodes: &'a mut HashSet<GraphNodeId>,
}

impl<'a> SceneNodeViewer<'a> {
    pub fn new(
        display_node: Option<NodeId>,
        selected_node: Option<NodeId>,
        eval_mode: EvalMode,
        dirty_nodes: &'a mut HashSet<GraphNodeId>,
    ) -> Self {
        Self {
            events: Vec::new(),
            display_node,
            selected_node,
            eval_mode,
            dirty_nodes,
        }
    }
}

impl SnarlViewer<SceneNode> for SceneNodeViewer<'_> {
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

    fn show_header(
        &mut self,
        node: NodeId,
        _inputs: &[InPin],
        _outputs: &[OutPin],
        ui: &mut egui::Ui,
        _scale: f32,
        snarl: &mut Snarl<SceneNode>,
    ) {
        // Detect left-click on header to select node
        let header_rect = ui.max_rect();
        if ui.rect_contains_pointer(header_rect) && ui.input(|i| i.pointer.primary_clicked()) {
            self.events
                .push(NodeGraphEvent::SelectNode(GraphNodeId::from(node)));
        }

        // Visual highlight for selected node (accent stroke around header)
        let is_selected = self.selected_node == Some(node);
        if is_selected {
            let painter = ui.painter();
            let highlight_rect = header_rect.expand(2.0);
            painter.rect_stroke(
                highlight_rect,
                4.0,
                egui::Stroke::new(2.0, theme::ACCENT_PRIMARY),
            );
        }

        // Title label
        let title = snarl[node].name();
        let text_color = if is_selected {
            theme::TEXT_PRIMARY
        } else {
            theme::TEXT_SECONDARY
        };
        ui.colored_label(text_color, title);
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
        // Detect left-click on body to select it (fallback — header is primary target)
        if ui.rect_contains_pointer(ui.max_rect()) && ui.input(|i| i.pointer.primary_clicked()) {
            self.events
                .push(NodeGraphEvent::SelectNode(GraphNodeId::from(node_id)));
        }

        // Display flag indicator (blue dot like Houdini)
        if self.display_node == Some(node_id) {
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
                ui.painter()
                    .circle_filled(rect.center(), 4.0, theme::ACCENT_PRIMARY);
                ui.colored_label(theme::ACCENT_PRIMARY, "Display");
            });
        }

        let node = &mut snarl[node_id];

        match node {
            SceneNode::UsdRead {
                is_loaded, error, ..
            } => {
                if *is_loaded {
                    ui.colored_label(theme::STATUS_OK, "\u{2713} Loaded");
                } else if let Some(err) = error {
                    ui.colored_label(theme::STATUS_ERROR, format!("\u{2717} {}", err));
                }
            }
            SceneNode::IvarRender {
                is_rendering,
                is_converting_tx,
                ..
            } => {
                if *is_rendering {
                    ui.colored_label(theme::STATUS_WARNING, "Rendering...");
                } else if *is_converting_tx {
                    ui.colored_label(theme::STATUS_WARNING, "Converting .tx...");
                }
            }
            SceneNode::Primitive {
                kind,
                size,
                is_created,
                ..
            } => {
                // Auto-compute: create primitive when needed
                let graph_id = GraphNodeId::from(node_id);
                if !*is_created {
                    if self.eval_mode == EvalMode::Auto {
                        self.events.push(NodeGraphEvent::CreatePrimitive {
                            kind: *kind,
                            size: *size,
                            node_id: graph_id,
                        });
                        *is_created = true;
                    } else {
                        self.dirty_nodes.insert(graph_id);
                    }
                }
                if *is_created {
                    ui.colored_label(theme::STATUS_OK, "Created");
                } else if self.dirty_nodes.contains(&graph_id) {
                    ui.colored_label(theme::STATUS_WARNING, "Dirty");
                }
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
                ..
            } => {
                // Auto-compute: check if inputs are satisfied
                let inputs_satisfied = match source {
                    bif_core::PointSource::Surface => resolve_input_connection(inputs, 0).is_some(),
                    bif_core::PointSource::Grid | bif_core::PointSource::Sphere => true,
                };

                if !*is_computed && inputs_satisfied {
                    let graph_node_id = GraphNodeId::from(node_id);
                    if self.eval_mode != EvalMode::Auto {
                        self.dirty_nodes.insert(graph_node_id);
                    } else {
                        self.events.push(NodeGraphEvent::ScatterPointsCompute {
                            node_id: graph_node_id,
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
                        *is_computed = true;
                    } // end Auto branch
                }

                // Status display
                let scatter_graph_id = GraphNodeId::from(node_id);
                if *is_computed {
                    ui.colored_label(theme::STATUS_OK, format!("{} pts", count));
                } else if self.dirty_nodes.contains(&scatter_graph_id) {
                    ui.colored_label(theme::STATUS_WARNING, "Dirty");
                } else if !inputs_satisfied {
                    ui.colored_label(theme::STATUS_WARNING, "Waiting for input");
                }
            }
            SceneNode::PointInstancer {
                instance_count,
                is_instanced,
                is_computing,
                compute_failed,
                ..
            } => {
                let points_node = resolve_input_connection(inputs, 0);
                let proto_node = resolve_input_connection(inputs, 1);
                let both_connected = points_node.is_some() && proto_node.is_some();

                // Auto-invalidate: inputs disconnected but still marked instanced
                if !both_connected && *is_instanced {
                    self.events.push(NodeGraphEvent::InstancerInvalidate {
                        node_id: GraphNodeId::from(node_id),
                    });
                    *is_instanced = false;
                    *is_computing = false;
                    *compute_failed = false;
                    *instance_count = 0;
                }

                // Auto-compute: both inputs connected, not yet instanced, not failed
                let inst_graph_id = GraphNodeId::from(node_id);
                if both_connected && !*is_instanced && !*is_computing && !*compute_failed {
                    if self.eval_mode == EvalMode::Auto {
                        let (Some(points_source), Some(proto_source)) = (points_node, proto_node)
                        else {
                            return;
                        };
                        self.events.push(NodeGraphEvent::PointInstancerCompute {
                            node_id: inst_graph_id,
                            points_source_node: GraphNodeId::from(points_source),
                            proto_source_node: GraphNodeId::from(proto_source),
                        });
                        *is_computing = true;
                    } else {
                        self.dirty_nodes.insert(inst_graph_id);
                    }
                }

                // Status display
                if *is_instanced {
                    ui.colored_label(theme::STATUS_OK, format!("{} instances", instance_count));
                } else if *is_computing {
                    ui.colored_label(theme::STATUS_WARNING, "Computing...");
                } else if *compute_failed {
                    ui.colored_label(theme::STATUS_ERROR, "Compute failed");
                } else if self.dirty_nodes.contains(&inst_graph_id) {
                    ui.colored_label(theme::STATUS_WARNING, "Dirty");
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
                    ui.colored_label(theme::STATUS_WARNING, format!("Need: {}", need));
                }
            }
            SceneNode::UsdExport {
                is_exported,
                last_result,
                ..
            } => {
                if *is_exported {
                    if let Some(ref result) = last_result {
                        ui.colored_label(theme::STATUS_OK, result.as_str());
                    }
                } else if let Some(ref result) = last_result {
                    ui.colored_label(theme::STATUS_ERROR, result.as_str());
                }
            }
            SceneNode::Xform {
                translate, scale, ..
            } => {
                // Compact T/S summary
                ui.label(format!(
                    "T({:.1},{:.1},{:.1})",
                    translate[0], translate[1], translate[2]
                ));
                if scale.iter().any(|s| (*s - 1.0).abs() > 0.001) {
                    ui.label(format!(
                        "S({:.2},{:.2},{:.2})",
                        scale[0], scale[1], scale[2]
                    ));
                }
            }
            SceneNode::UsdPrim { prim_path, .. } => {
                ui.colored_label(theme::PIN_SCENE, prim_path.as_str());
            }
            SceneNode::GraftBranches { destination_path } => {
                // Show connected branch count (needs inputs from snarl)
                let connected = (0..4)
                    .filter(|&i| !inputs.get(i).is_none_or(|p| p.remotes.is_empty()))
                    .count();
                ui.label(format!("{}/4 branches", connected));
                ui.colored_label(theme::PIN_SCENE, destination_path.as_str());
            }
            SceneNode::HdriEnvironment {
                is_loaded,
                is_loading,
                error,
                ..
            } => {
                if *is_loading {
                    ui.colored_label(theme::STATUS_WARNING, "Generating IBL...");
                } else if *is_loaded {
                    ui.colored_label(theme::STATUS_OK, "Loaded");
                } else if let Some(err) = error {
                    ui.colored_label(theme::STATUS_ERROR, format!("Error: {}", err));
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
                node_id: GraphNodeId::from(to.id.node),
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
                self.events
                    .push(NodeGraphEvent::SetDisplayNode(GraphNodeId::from(node)));
                ui.close_menu();
            }
        }
        if ui.button("Delete").clicked() {
            self.events
                .push(NodeGraphEvent::DeleteNode(GraphNodeId::from(node)));
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
