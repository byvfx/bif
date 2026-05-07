//! Pure evaluation logic for the node graph.
//!
//! Decoupled from egui — computes which nodes need cooking based on
//! status flags and connectivity, independent of UI visibility.

use std::collections::HashSet;

use egui_snarl::{InPinId, NodeId, Snarl};

use super::{GraphNodeId, NodeGraphEvent, ScatterPointsParams, SceneNode};
use crate::persistence::EvalMode;

/// Check whether a node's input at `input_index` is connected.
///
/// Returns `Some(NodeId)` of the upstream node if connected, `None` otherwise.
fn is_input_connected(
    node_id: NodeId,
    input_index: usize,
    snarl: &Snarl<SceneNode>,
) -> Option<NodeId> {
    let in_pin = snarl.in_pin(InPinId {
        node: node_id,
        input: input_index,
    });
    in_pin.remotes.first().map(|r| r.node)
}

/// Evaluate all nodes and collect auto-compute events.
///
/// This replaces the auto-compute checks that were inside `show_body()`.
/// Unlike `show_body`, this evaluates ALL nodes regardless of UI visibility.
///
/// Returns events to be processed. Mutates node status flags (e.g.
/// `is_created`, `is_computed`, `is_computing`) as side effects.
pub(crate) fn collect_auto_compute_events(
    snarl: &mut Snarl<SceneNode>,
    eval_mode: EvalMode,
    dirty_nodes: &mut HashSet<GraphNodeId>,
) -> Vec<NodeGraphEvent> {
    let mut events = Vec::new();

    // Collect node IDs first to avoid borrow conflict with `snarl`.
    let node_ids: Vec<NodeId> = snarl.node_ids().map(|(id, _)| id).collect();

    for node_id in node_ids {
        evaluate_node(node_id, snarl, eval_mode, dirty_nodes, &mut events);
    }

    events
}

/// Evaluate a single node for auto-compute readiness.
fn evaluate_node(
    node_id: NodeId,
    snarl: &mut Snarl<SceneNode>,
    eval_mode: EvalMode,
    dirty_nodes: &mut HashSet<GraphNodeId>,
    events: &mut Vec<NodeGraphEvent>,
) {
    match &snarl[node_id] {
        // --- Primitive: auto-create geometry when not yet created --------
        SceneNode::Primitive {
            is_created: false,
            kind,
            size,
            ..
        } => {
            let kind = *kind;
            let size = *size;
            let graph_id = GraphNodeId::from(node_id);

            if eval_mode == EvalMode::Auto {
                events.push(NodeGraphEvent::CreatePrimitive {
                    kind,
                    size,
                    node_id: graph_id,
                });
                // Mutate flag — mark created so we don't re-emit.
                if let SceneNode::Primitive { is_created, .. } = &mut snarl[node_id] {
                    *is_created = true;
                }
            } else {
                dirty_nodes.insert(graph_id);
            }
        }

        // --- ScatterPoints: auto-compute when inputs satisfied ----------
        SceneNode::ScatterPoints {
            is_computed: false,
            source,
            ..
        } => {
            let source = *source;
            let inputs_satisfied = match source {
                bif_core::PointSource::Surface => is_input_connected(node_id, 0, snarl).is_some(),
                bif_core::PointSource::Grid | bif_core::PointSource::Sphere => true,
            };

            if !inputs_satisfied {
                return;
            }

            let graph_node_id = GraphNodeId::from(node_id);

            if eval_mode == EvalMode::Auto {
                // Read all params before mutating.
                let params = extract_scatter_params(snarl, node_id);
                events.push(NodeGraphEvent::ScatterPointsCompute {
                    node_id: graph_node_id,
                    params,
                });
                if let SceneNode::ScatterPoints { is_computed, .. } = &mut snarl[node_id] {
                    *is_computed = true;
                }
            } else {
                dirty_nodes.insert(graph_node_id);
            }
        }

        // --- PointInstancer: auto-compute when both inputs connected ----
        SceneNode::PointInstancer {
            is_instanced,
            is_computing,
            compute_failed,
            ..
        } => {
            let is_instanced = *is_instanced;
            let is_computing = *is_computing;
            let compute_failed = *compute_failed;

            let points_node = is_input_connected(node_id, 0, snarl);
            let proto_node = is_input_connected(node_id, 1, snarl);
            let both_connected = points_node.is_some() && proto_node.is_some();

            // Auto-invalidate: inputs disconnected but still marked instanced.
            if !both_connected && is_instanced {
                events.push(NodeGraphEvent::InstancerInvalidate {
                    node_id: GraphNodeId::from(node_id),
                });
                if let SceneNode::PointInstancer {
                    is_instanced,
                    is_computing,
                    compute_failed,
                    instance_count,
                    ..
                } = &mut snarl[node_id]
                {
                    *is_instanced = false;
                    *is_computing = false;
                    *compute_failed = false;
                    *instance_count = 0;
                }
                return;
            }

            // Auto-compute: both inputs connected, not yet instanced, not failed.
            let inst_graph_id = GraphNodeId::from(node_id);
            if both_connected && !is_instanced && !is_computing && !compute_failed {
                if eval_mode == EvalMode::Auto {
                    let (Some(points_source), Some(proto_source)) = (points_node, proto_node)
                    else {
                        return;
                    };
                    events.push(NodeGraphEvent::PointInstancerCompute {
                        node_id: inst_graph_id,
                        points_source_node: GraphNodeId::from(points_source),
                        proto_source_node: GraphNodeId::from(proto_source),
                    });
                    if let SceneNode::PointInstancer { is_computing, .. } = &mut snarl[node_id] {
                        *is_computing = true;
                    }
                } else {
                    dirty_nodes.insert(inst_graph_id);
                }
            }
        }

        // --- Xform: rebuild scene once input is connected ---------------
        SceneNode::Xform {
            is_applied: false, ..
        } => {
            if is_input_connected(node_id, 0, snarl).is_none() {
                return;
            }

            let graph_id = GraphNodeId::from(node_id);
            if eval_mode == EvalMode::Auto {
                events.push(NodeGraphEvent::XformChanged { node_id: graph_id });
                if let SceneNode::Xform { is_applied, .. } = &mut snarl[node_id] {
                    *is_applied = true;
                }
            } else {
                dirty_nodes.insert(graph_id);
            }
        }

        // --- UsdPrim: register authored prim metadata -------------------
        SceneNode::UsdPrim {
            is_created: false, ..
        } => {
            let graph_id = GraphNodeId::from(node_id);
            if eval_mode == EvalMode::Auto {
                events.push(NodeGraphEvent::UsdPrimCreate { node_id: graph_id });
                if let SceneNode::UsdPrim { is_created, .. } = &mut snarl[node_id] {
                    *is_created = true;
                }
            } else {
                dirty_nodes.insert(graph_id);
            }
        }

        // All other node types have no auto-compute logic.
        _ => {}
    }
}

/// Extract `ScatterPointsParams` from a `ScatterPoints` node.
///
/// Panics if `node_id` does not point at a `ScatterPoints` variant.
fn extract_scatter_params(snarl: &Snarl<SceneNode>, node_id: NodeId) -> ScatterPointsParams {
    match &snarl[node_id] {
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
            ..
        } => ScatterPointsParams {
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
        _ => unreachable!("evaluate_node guarantees ScatterPoints variant"),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use egui_snarl::{OutPinId, Snarl};

    /// Helper: create a ScatterPoints node with Grid source (no input needed).
    fn scatter_grid() -> SceneNode {
        SceneNode::ScatterPoints {
            source: bif_core::PointSource::Grid,
            count: 100,
            max_point_limit: 1_000_000,
            seed: 1,
            scatter_mode: bif_core::scatter::ScatterMode::Random,
            min_distance: 0.5,
            align_to_normal: false,
            grid_size: [10.0, 0.0, 10.0],
            grid_spacing: 1.0,
            sphere_radius: 5.0,
            sphere_on_surface: true,
            relax_iterations: 0,
            scale_radii: 1.0,
            max_relax_radius: 2.0,
            scale_min: 0.8,
            scale_max: 1.2,
            rotation_range: 360.0,
            target_proto_id: None,
            is_computed: false,
            point_size: 5.0,
            point_color: [0.0, 0.9, 0.9, 0.8],
        }
    }

    /// Helper: create a ScatterPoints node with Surface source (needs input).
    fn scatter_surface() -> SceneNode {
        SceneNode::ScatterPoints {
            source: bif_core::PointSource::Surface,
            count: 100,
            max_point_limit: 1_000_000,
            seed: 1,
            scatter_mode: bif_core::scatter::ScatterMode::Random,
            min_distance: 0.5,
            align_to_normal: false,
            grid_size: [10.0, 0.0, 10.0],
            grid_spacing: 1.0,
            sphere_radius: 5.0,
            sphere_on_surface: true,
            relax_iterations: 0,
            scale_radii: 1.0,
            max_relax_radius: 2.0,
            scale_min: 0.8,
            scale_max: 1.2,
            rotation_range: 360.0,
            target_proto_id: None,
            is_computed: false,
            point_size: 5.0,
            point_color: [0.0, 0.9, 0.9, 0.8],
        }
    }

    /// Helper: connect output 0 of `from` to input `to_input` of `to`.
    fn connect(snarl: &mut Snarl<SceneNode>, from: NodeId, to: NodeId, to_input: usize) {
        snarl.connect(
            OutPinId {
                node: from,
                output: 0,
            },
            InPinId {
                node: to,
                input: to_input,
            },
        );
    }

    // --- Primitive tests ------------------------------------------------

    #[test]
    fn test_primitive_auto_creates() {
        let mut snarl = Snarl::new();
        let id = snarl.insert_node(
            egui::pos2(0.0, 0.0),
            SceneNode::primitive(bif_core::PrimitiveKind::Cube),
        );

        let mut dirty = HashSet::new();
        let events = collect_auto_compute_events(&mut snarl, EvalMode::Auto, &mut dirty);

        assert_eq!(events.len(), 1);
        assert!(matches!(
            &events[0],
            NodeGraphEvent::CreatePrimitive {
                kind: bif_core::PrimitiveKind::Cube,
                ..
            }
        ));
        // Flag should be set.
        assert!(matches!(
            &snarl[id],
            SceneNode::Primitive {
                is_created: true,
                ..
            }
        ));
        assert!(dirty.is_empty());
    }

    #[test]
    fn test_primitive_manual_marks_dirty() {
        let mut snarl = Snarl::new();
        let id = snarl.insert_node(
            egui::pos2(0.0, 0.0),
            SceneNode::primitive(bif_core::PrimitiveKind::Sphere),
        );

        let mut dirty = HashSet::new();
        let events = collect_auto_compute_events(&mut snarl, EvalMode::Manual, &mut dirty);

        assert!(events.is_empty());
        assert!(dirty.contains(&GraphNodeId::from(id)));
    }

    #[test]
    fn test_primitive_already_created_no_event() {
        let mut snarl = Snarl::new();
        let id = snarl.insert_node(
            egui::pos2(0.0, 0.0),
            SceneNode::primitive(bif_core::PrimitiveKind::Cube),
        );
        // Pre-set the flag.
        if let SceneNode::Primitive { is_created, .. } = &mut snarl[id] {
            *is_created = true;
        }

        let mut dirty = HashSet::new();
        let events = collect_auto_compute_events(&mut snarl, EvalMode::Auto, &mut dirty);

        assert!(events.is_empty());
        assert!(dirty.is_empty());
    }

    // --- ScatterPoints tests --------------------------------------------

    #[test]
    fn test_scatter_auto_computes_grid() {
        let mut snarl = Snarl::new();
        let id = snarl.insert_node(egui::pos2(0.0, 0.0), scatter_grid());

        let mut dirty = HashSet::new();
        let events = collect_auto_compute_events(&mut snarl, EvalMode::Auto, &mut dirty);

        assert_eq!(events.len(), 1);
        assert!(matches!(
            &events[0],
            NodeGraphEvent::ScatterPointsCompute { .. }
        ));
        // Flag should be set.
        assert!(matches!(
            &snarl[id],
            SceneNode::ScatterPoints {
                is_computed: true,
                ..
            }
        ));
    }

    #[test]
    fn test_scatter_needs_input_surface() {
        let mut snarl = Snarl::new();
        let _id = snarl.insert_node(egui::pos2(0.0, 0.0), scatter_surface());

        let mut dirty = HashSet::new();
        let events = collect_auto_compute_events(&mut snarl, EvalMode::Auto, &mut dirty);

        // No input connected — should not compute.
        assert!(events.is_empty());
    }

    #[test]
    fn test_scatter_surface_with_input() {
        let mut snarl = Snarl::new();
        let read_id = snarl.insert_node(egui::pos2(0.0, 0.0), SceneNode::usd_read());
        let scatter_id = snarl.insert_node(egui::pos2(200.0, 0.0), scatter_surface());
        connect(&mut snarl, read_id, scatter_id, 0);

        let mut dirty = HashSet::new();
        let events = collect_auto_compute_events(&mut snarl, EvalMode::Auto, &mut dirty);

        assert_eq!(events.len(), 1);
        assert!(matches!(
            &events[0],
            NodeGraphEvent::ScatterPointsCompute { .. }
        ));
    }

    #[test]
    fn test_scatter_manual_marks_dirty() {
        let mut snarl = Snarl::new();
        let id = snarl.insert_node(egui::pos2(0.0, 0.0), scatter_grid());

        let mut dirty = HashSet::new();
        let events = collect_auto_compute_events(&mut snarl, EvalMode::Manual, &mut dirty);

        assert!(events.is_empty());
        assert!(dirty.contains(&GraphNodeId::from(id)));
    }

    // --- PointInstancer tests -------------------------------------------

    #[test]
    fn test_instancer_auto_computes() {
        let mut snarl = Snarl::new();
        let scatter_id = snarl.insert_node(egui::pos2(0.0, 0.0), scatter_grid());
        let prim_id = snarl.insert_node(
            egui::pos2(0.0, 100.0),
            SceneNode::primitive(bif_core::PrimitiveKind::Cube),
        );
        let inst_id = snarl.insert_node(egui::pos2(200.0, 50.0), SceneNode::point_instancer());
        connect(&mut snarl, scatter_id, inst_id, 0); // points
        connect(&mut snarl, prim_id, inst_id, 1); // proto

        let mut dirty = HashSet::new();
        let events = collect_auto_compute_events(&mut snarl, EvalMode::Auto, &mut dirty);

        // Should have events for: Primitive create, Scatter compute, Instancer compute.
        let instancer_events: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, NodeGraphEvent::PointInstancerCompute { .. }))
            .collect();
        assert_eq!(instancer_events.len(), 1);

        // Verify is_computing flag set.
        assert!(matches!(
            &snarl[inst_id],
            SceneNode::PointInstancer {
                is_computing: true,
                ..
            }
        ));
    }

    #[test]
    fn test_instancer_missing_points() {
        let mut snarl = Snarl::new();
        let prim_id = snarl.insert_node(
            egui::pos2(0.0, 0.0),
            SceneNode::primitive(bif_core::PrimitiveKind::Cube),
        );
        let inst_id = snarl.insert_node(egui::pos2(200.0, 0.0), SceneNode::point_instancer());
        connect(&mut snarl, prim_id, inst_id, 1); // proto only

        let mut dirty = HashSet::new();
        let events = collect_auto_compute_events(&mut snarl, EvalMode::Auto, &mut dirty);

        // No instancer compute event (primitive create is fine).
        let instancer_events: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, NodeGraphEvent::PointInstancerCompute { .. }))
            .collect();
        assert!(instancer_events.is_empty());
    }

    #[test]
    fn test_instancer_missing_proto() {
        let mut snarl = Snarl::new();
        let scatter_id = snarl.insert_node(egui::pos2(0.0, 0.0), scatter_grid());
        let inst_id = snarl.insert_node(egui::pos2(200.0, 0.0), SceneNode::point_instancer());
        connect(&mut snarl, scatter_id, inst_id, 0); // points only

        let mut dirty = HashSet::new();
        let events = collect_auto_compute_events(&mut snarl, EvalMode::Auto, &mut dirty);

        let instancer_events: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, NodeGraphEvent::PointInstancerCompute { .. }))
            .collect();
        assert!(instancer_events.is_empty());
    }

    #[test]
    fn test_instancer_already_computing_no_duplicate() {
        let mut snarl = Snarl::new();
        let scatter_id = snarl.insert_node(egui::pos2(0.0, 0.0), scatter_grid());
        let prim_id = snarl.insert_node(
            egui::pos2(0.0, 100.0),
            SceneNode::primitive(bif_core::PrimitiveKind::Cube),
        );
        let inst_id = snarl.insert_node(egui::pos2(200.0, 50.0), SceneNode::point_instancer());
        connect(&mut snarl, scatter_id, inst_id, 0);
        connect(&mut snarl, prim_id, inst_id, 1);

        // Pre-set is_computing.
        if let SceneNode::PointInstancer { is_computing, .. } = &mut snarl[inst_id] {
            *is_computing = true;
        }

        let mut dirty = HashSet::new();
        let events = collect_auto_compute_events(&mut snarl, EvalMode::Auto, &mut dirty);

        let instancer_events: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, NodeGraphEvent::PointInstancerCompute { .. }))
            .collect();
        assert!(instancer_events.is_empty());
    }

    #[test]
    fn test_instancer_compute_failed_no_retry() {
        let mut snarl = Snarl::new();
        let scatter_id = snarl.insert_node(egui::pos2(0.0, 0.0), scatter_grid());
        let prim_id = snarl.insert_node(
            egui::pos2(0.0, 100.0),
            SceneNode::primitive(bif_core::PrimitiveKind::Cube),
        );
        let inst_id = snarl.insert_node(egui::pos2(200.0, 50.0), SceneNode::point_instancer());
        connect(&mut snarl, scatter_id, inst_id, 0);
        connect(&mut snarl, prim_id, inst_id, 1);

        // Pre-set compute_failed.
        if let SceneNode::PointInstancer { compute_failed, .. } = &mut snarl[inst_id] {
            *compute_failed = true;
        }

        let mut dirty = HashSet::new();
        let events = collect_auto_compute_events(&mut snarl, EvalMode::Auto, &mut dirty);

        let instancer_events: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, NodeGraphEvent::PointInstancerCompute { .. }))
            .collect();
        assert!(instancer_events.is_empty());
    }

    #[test]
    fn test_instancer_invalidate_on_disconnect() {
        let mut snarl = Snarl::new();
        let inst_id = snarl.insert_node(egui::pos2(0.0, 0.0), SceneNode::point_instancer());

        // Pre-set is_instanced (simulates previously-computed state).
        if let SceneNode::PointInstancer {
            is_instanced,
            instance_count,
            ..
        } = &mut snarl[inst_id]
        {
            *is_instanced = true;
            *instance_count = 500;
        }

        let mut dirty = HashSet::new();
        let events = collect_auto_compute_events(&mut snarl, EvalMode::Auto, &mut dirty);

        assert_eq!(events.len(), 1);
        assert!(matches!(
            &events[0],
            NodeGraphEvent::InstancerInvalidate { .. }
        ));
        // Flags should be reset.
        assert!(matches!(
            &snarl[inst_id],
            SceneNode::PointInstancer {
                is_instanced: false,
                is_computing: false,
                compute_failed: false,
                instance_count: 0,
                ..
            }
        ));
    }

    // --- Other node types -----------------------------------------------

    #[test]
    fn test_usd_read_no_auto_compute() {
        let mut snarl = Snarl::new();
        let _id = snarl.insert_node(egui::pos2(0.0, 0.0), SceneNode::usd_read());

        let mut dirty = HashSet::new();
        let events = collect_auto_compute_events(&mut snarl, EvalMode::Auto, &mut dirty);

        assert!(events.is_empty());
        assert!(dirty.is_empty());
    }

    #[test]
    fn test_xform_auto_emits_when_connected() {
        let mut snarl = Snarl::new();
        let read_id = snarl.insert_node(egui::pos2(0.0, 0.0), SceneNode::usd_read());
        let xform_id = snarl.insert_node(egui::pos2(200.0, 0.0), SceneNode::xform());
        connect(&mut snarl, read_id, xform_id, 0);

        let mut dirty = HashSet::new();
        let events = collect_auto_compute_events(&mut snarl, EvalMode::Auto, &mut dirty);

        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], NodeGraphEvent::XformChanged { .. }));
        assert!(matches!(
            &snarl[xform_id],
            SceneNode::Xform {
                is_applied: true,
                ..
            }
        ));
    }

    #[test]
    fn test_usd_prim_auto_registers() {
        let mut snarl = Snarl::new();
        let prim_id = snarl.insert_node(egui::pos2(0.0, 0.0), SceneNode::usd_prim());

        let mut dirty = HashSet::new();
        let events = collect_auto_compute_events(&mut snarl, EvalMode::Auto, &mut dirty);

        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], NodeGraphEvent::UsdPrimCreate { .. }));
        assert!(matches!(
            &snarl[prim_id],
            SceneNode::UsdPrim {
                is_created: true,
                ..
            }
        ));
    }

    #[test]
    fn test_graft_branches_has_no_auto_eval_while_held() {
        let mut snarl = Snarl::new();
        let read_id = snarl.insert_node(egui::pos2(0.0, 0.0), SceneNode::usd_read());
        let graft_id = snarl.insert_node(egui::pos2(200.0, 0.0), SceneNode::graft_branches());

        let mut dirty = HashSet::new();
        let events = collect_auto_compute_events(&mut snarl, EvalMode::Auto, &mut dirty);
        assert!(events.is_empty());

        connect(&mut snarl, read_id, graft_id, 0);
        let events = collect_auto_compute_events(&mut snarl, EvalMode::Auto, &mut dirty);

        assert!(events.is_empty());
        assert!(dirty.is_empty());
    }

    // --- Multi-node tests -----------------------------------------------

    #[test]
    fn test_multiple_nodes_all_evaluated() {
        let mut snarl = Snarl::new();
        let _prim_id = snarl.insert_node(
            egui::pos2(0.0, 0.0),
            SceneNode::primitive(bif_core::PrimitiveKind::Cube),
        );
        let _scatter_id = snarl.insert_node(egui::pos2(0.0, 100.0), scatter_grid());
        let _read_id = snarl.insert_node(egui::pos2(0.0, 200.0), SceneNode::usd_read());

        let mut dirty = HashSet::new();
        let events = collect_auto_compute_events(&mut snarl, EvalMode::Auto, &mut dirty);

        // Primitive creates + scatter computes = 2 events; UsdRead emits nothing.
        assert_eq!(events.len(), 2);
        assert!(events
            .iter()
            .any(|e| matches!(e, NodeGraphEvent::CreatePrimitive { .. })));
        assert!(events
            .iter()
            .any(|e| matches!(e, NodeGraphEvent::ScatterPointsCompute { .. })));
    }

    #[test]
    fn test_already_computed_nodes_skipped() {
        let mut snarl = Snarl::new();

        // Primitive already created.
        let prim_id = snarl.insert_node(
            egui::pos2(0.0, 0.0),
            SceneNode::primitive(bif_core::PrimitiveKind::Cube),
        );
        if let SceneNode::Primitive { is_created, .. } = &mut snarl[prim_id] {
            *is_created = true;
        }

        // Scatter already computed.
        let scatter_id = snarl.insert_node(egui::pos2(0.0, 100.0), scatter_grid());
        if let SceneNode::ScatterPoints { is_computed, .. } = &mut snarl[scatter_id] {
            *is_computed = true;
        }

        // Instancer already instanced.
        let inst_id = snarl.insert_node(egui::pos2(0.0, 200.0), SceneNode::point_instancer());
        connect(&mut snarl, scatter_id, inst_id, 0);
        connect(&mut snarl, prim_id, inst_id, 1);
        if let SceneNode::PointInstancer { is_instanced, .. } = &mut snarl[inst_id] {
            *is_instanced = true;
        }

        let mut dirty = HashSet::new();
        let events = collect_auto_compute_events(&mut snarl, EvalMode::Auto, &mut dirty);

        assert!(events.is_empty());
        assert!(dirty.is_empty());
    }

    // --- is_input_connected tests ---------------------------------------

    #[test]
    fn test_is_input_connected_returns_none_when_unconnected() {
        let mut snarl = Snarl::new();
        let id = snarl.insert_node(egui::pos2(0.0, 0.0), SceneNode::point_instancer());

        assert!(is_input_connected(id, 0, &snarl).is_none());
        assert!(is_input_connected(id, 1, &snarl).is_none());
    }

    #[test]
    fn test_is_input_connected_returns_upstream_node() {
        let mut snarl = Snarl::new();
        let read_id = snarl.insert_node(egui::pos2(0.0, 0.0), SceneNode::usd_read());
        let scatter_id = snarl.insert_node(egui::pos2(200.0, 0.0), scatter_surface());
        connect(&mut snarl, read_id, scatter_id, 0);

        assert_eq!(is_input_connected(scatter_id, 0, &snarl), Some(read_id));
    }

    // --- Instancer manual mode ------------------------------------------

    #[test]
    fn test_instancer_manual_marks_dirty() {
        let mut snarl = Snarl::new();
        let scatter_id = snarl.insert_node(egui::pos2(0.0, 0.0), scatter_grid());
        let prim_id = snarl.insert_node(
            egui::pos2(0.0, 100.0),
            SceneNode::primitive(bif_core::PrimitiveKind::Cube),
        );
        let inst_id = snarl.insert_node(egui::pos2(200.0, 50.0), SceneNode::point_instancer());
        connect(&mut snarl, scatter_id, inst_id, 0);
        connect(&mut snarl, prim_id, inst_id, 1);

        let mut dirty = HashSet::new();
        let events = collect_auto_compute_events(&mut snarl, EvalMode::Manual, &mut dirty);

        // No instancer compute events in manual mode.
        let instancer_events: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, NodeGraphEvent::PointInstancerCompute { .. }))
            .collect();
        assert!(instancer_events.is_empty());
        assert!(dirty.contains(&GraphNodeId::from(inst_id)));
    }
}
