//! SceneNodeViewer: SnarlViewer implementation for the node graph UI.

use std::collections::{HashMap, HashSet};

use egui_snarl::{
    ui::{PinInfo, SnarlViewer},
    InPin, NodeId, OutPin, Snarl,
};

use super::ops::mark_node_dirty;
use super::{GraphNodeId, NodeGraphEvent, SceneNode};
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
    /// Nodes needing re-evaluation (for dirty indicator)
    pub dirty_nodes: &'a mut HashSet<GraphNodeId>,
    /// Prim count per node (for badge display)
    pub prim_counts: &'a HashMap<GraphNodeId, usize>,
}

impl<'a> SceneNodeViewer<'a> {
    pub fn new(
        display_node: Option<NodeId>,
        selected_node: Option<NodeId>,
        dirty_nodes: &'a mut HashSet<GraphNodeId>,
        prim_counts: &'a HashMap<GraphNodeId, usize>,
    ) -> Self {
        Self {
            events: Vec::new(),
            display_node,
            selected_node,
            dirty_nodes,
            prim_counts,
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

        // Title label + prim count badge
        let title = snarl[node].name();
        let text_color = if is_selected {
            theme::TEXT_PRIMARY
        } else {
            theme::TEXT_SECONDARY
        };
        ui.horizontal(|ui| {
            ui.colored_label(text_color, title);
            let gid = GraphNodeId::from(node);
            if let Some(&count) = self.prim_counts.get(&gid) {
                if count > 0 {
                    ui.colored_label(theme::TEXT_DISABLED, format!("[{}]", count));
                }
            }
        });
    }

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
            SceneNode::Primitive { is_created, .. } => {
                let graph_id = GraphNodeId::from(node_id);
                if *is_created {
                    ui.colored_label(theme::STATUS_OK, "Created");
                } else if self.dirty_nodes.contains(&graph_id) {
                    ui.colored_label(theme::STATUS_WARNING, "Dirty");
                }
            }
            SceneNode::ScatterPoints {
                source,
                count,
                is_computed,
                ..
            } => {
                let inputs_satisfied = match source {
                    bif_core::PointSource::Surface => resolve_input_connection(inputs, 0).is_some(),
                    bif_core::PointSource::Grid | bif_core::PointSource::Sphere => true,
                };

                let graph_node_id = GraphNodeId::from(node_id);
                if *is_computed {
                    ui.colored_label(theme::STATUS_OK, format!("{} pts", count));
                } else if self.dirty_nodes.contains(&graph_node_id) {
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
                let inst_graph_id = GraphNodeId::from(node_id);
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
            SceneNode::UsdPrim {
                prim_path,
                is_created,
                ..
            } => {
                ui.colored_label(theme::PIN_SCENE, prim_path.as_str());
                if *is_created {
                    ui.colored_label(theme::STATUS_OK, "Created");
                }
            }
            SceneNode::GraftBranches {
                destination_path,
                is_computed,
            } => {
                // Show connected branch count (needs inputs from snarl)
                let connected = (0..4)
                    .filter(|&i| !inputs.get(i).is_none_or(|p| p.remotes.is_empty()))
                    .count();
                ui.label(format!("{}/4 branches", connected));
                ui.colored_label(theme::PIN_SCENE, destination_path.as_str());
                if *is_computed {
                    ui.colored_label(theme::STATUS_OK, "Computed");
                }
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
            SceneNode::Cache {
                bypassed,
                is_cached,
                ..
            } => {
                if *bypassed {
                    ui.colored_label(theme::TEXT_SECONDARY, "Bypassed");
                } else if *is_cached {
                    ui.colored_label(theme::STATUS_OK, "Cached");
                } else {
                    ui.colored_label(theme::STATUS_WARNING, "Stale");
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

    fn has_graph_menu(&mut self, _pos: egui::Pos2, _snarl: &mut Snarl<SceneNode>) -> bool {
        true
    }

    fn show_graph_menu(
        &mut self,
        pos: egui::Pos2,
        ui: &mut egui::Ui,
        _scale: f32,
        snarl: &mut Snarl<SceneNode>,
    ) {
        ui.label("Add node");
        ui.separator();

        if ui.button("USD Read").clicked() {
            snarl.insert_node(pos, SceneNode::usd_read());
            ui.close_menu();
        }
        if ui.button("HDRI Environment").clicked() {
            snarl.insert_node(pos, SceneNode::hdri_environment());
            ui.close_menu();
        }
        if ui.button("Ivar Render").clicked() {
            snarl.insert_node(pos, SceneNode::ivar_render());
            ui.close_menu();
        }
        ui.separator();
        if ui.button("Cube").clicked() {
            snarl.insert_node(pos, SceneNode::primitive(bif_core::PrimitiveKind::Cube));
            ui.close_menu();
        }
        if ui.button("Sphere").clicked() {
            snarl.insert_node(pos, SceneNode::primitive(bif_core::PrimitiveKind::Sphere));
            ui.close_menu();
        }
        if ui.button("Camera").clicked() {
            snarl.insert_node(pos, SceneNode::primitive(bif_core::PrimitiveKind::Camera));
            ui.close_menu();
        }
        if ui.button("Scatter Points").clicked() {
            snarl.insert_node(pos, SceneNode::scatter_points());
            ui.close_menu();
        }
        if ui.button("Point Instancer").clicked() {
            snarl.insert_node(pos, SceneNode::point_instancer());
            ui.close_menu();
        }
        if ui.button("Xform").clicked() {
            snarl.insert_node(pos, SceneNode::xform());
            ui.close_menu();
        }
        if ui.button("USD Prim").clicked() {
            snarl.insert_node(pos, SceneNode::usd_prim());
            ui.close_menu();
        }
        if ui.button("Graft Branches").clicked() {
            snarl.insert_node(pos, SceneNode::graft_branches());
            ui.close_menu();
        }
        if ui.button("USD Export").clicked() {
            snarl.insert_node(pos, SceneNode::usd_export());
            ui.close_menu();
        }
        if ui.button("Cache").clicked() {
            snarl.insert_node(pos, SceneNode::cache());
            ui.close_menu();
        }
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
