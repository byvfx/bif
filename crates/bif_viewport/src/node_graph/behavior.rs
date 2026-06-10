//! Per-node behavior: the cohesive region where each `SceneNode` variant's
//! logic lives, instead of being smeared across `eval.rs`, `scene_browser.rs`,
//! and `node_dispatch.rs` (issue #6 Phase 3).
//!
//! Phase 3a: `evaluate` (auto-compute readiness). 3b adds `register_prims`,
//! 3c adds `apply`.

use super::{GraphNodeId, NodeGraphEvent, ScatterPointsParams, SceneNode};
use crate::persistence::EvalMode;

/// Read-only, snarl-free view of a node's input connections, precomputed by
/// the caller so `evaluate` can take `&mut self` without also borrowing the
/// graph. `input_sources[i]` is the upstream node feeding input pin `i`.
pub(crate) struct EvalCtx {
    input_sources: Vec<Option<GraphNodeId>>,
}

impl EvalCtx {
    /// Build from a per-input list of upstream sources (index = input pin).
    pub(crate) fn new(input_sources: Vec<Option<GraphNodeId>>) -> Self {
        Self { input_sources }
    }

    /// Upstream node feeding input pin `input`, if connected.
    pub(crate) fn input_source(&self, input: usize) -> Option<GraphNodeId> {
        self.input_sources.get(input).copied().flatten()
    }

    /// Whether input pin `input` is connected.
    pub(crate) fn is_input_connected(&self, input: usize) -> bool {
        self.input_source(input).is_some()
    }
}

/// Result of evaluating one node: events to dispatch + whether the node should
/// be marked dirty (caller inserts it into `dirty_nodes`).
#[derive(Default)]
pub(crate) struct EvalOutcome {
    pub events: Vec<NodeGraphEvent>,
    pub dirty: bool,
}

impl SceneNode {
    /// Auto-compute readiness for this node. Mutates the node's own status
    /// flags in place; returns events to dispatch + a dirty signal. Pure of
    /// the graph (connections arrive via `ctx`) — unit-testable without a Snarl.
    pub(crate) fn evaluate(
        &mut self,
        id: GraphNodeId,
        ctx: &EvalCtx,
        mode: EvalMode,
    ) -> EvalOutcome {
        let mut out = EvalOutcome::default();
        match self {
            // --- Primitive: auto-create geometry when not yet created ------
            SceneNode::Primitive {
                is_created,
                kind,
                size,
                ..
            } if !*is_created => {
                let kind = *kind;
                let size = *size;
                if mode == EvalMode::Auto {
                    out.events.push(NodeGraphEvent::CreatePrimitive {
                        kind,
                        size,
                        node_id: id,
                    });
                    *is_created = true;
                } else {
                    out.dirty = true;
                }
            }

            // --- ScatterPoints: auto-compute when inputs satisfied ---------
            SceneNode::ScatterPoints {
                is_computed,
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
            } if !*is_computed => {
                let inputs_satisfied = match *source {
                    bif_core::PointSource::Surface => ctx.is_input_connected(0),
                    bif_core::PointSource::Grid | bif_core::PointSource::Sphere => true,
                };
                if inputs_satisfied {
                    if mode == EvalMode::Auto {
                        let params = ScatterPointsParams {
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
                        };
                        out.events.push(NodeGraphEvent::ScatterPointsCompute {
                            node_id: id,
                            params,
                        });
                        *is_computed = true;
                    } else {
                        out.dirty = true;
                    }
                }
            }

            // --- PointInstancer: auto-invalidate or auto-compute -----------
            SceneNode::PointInstancer {
                is_instanced,
                is_computing,
                compute_failed,
                instance_count,
                ..
            } => {
                let points_node = ctx.input_source(0);
                let proto_node = ctx.input_source(1);
                let both_connected = points_node.is_some() && proto_node.is_some();

                if !both_connected && *is_instanced {
                    // Inputs disconnected but still marked instanced.
                    out.events
                        .push(NodeGraphEvent::InstancerInvalidate { node_id: id });
                    *is_instanced = false;
                    *is_computing = false;
                    *compute_failed = false;
                    *instance_count = 0;
                } else if both_connected && !*is_instanced && !*is_computing && !*compute_failed {
                    if mode == EvalMode::Auto {
                        if let (Some(points_source), Some(proto_source)) = (points_node, proto_node)
                        {
                            out.events.push(NodeGraphEvent::PointInstancerCompute {
                                node_id: id,
                                points_source_node: points_source,
                                proto_source_node: proto_source,
                            });
                            *is_computing = true;
                        }
                    } else {
                        out.dirty = true;
                    }
                }
            }

            // --- Xform: rebuild scene once input is connected --------------
            SceneNode::Xform { is_applied, .. } if !*is_applied => {
                if ctx.is_input_connected(0) {
                    if mode == EvalMode::Auto {
                        out.events
                            .push(NodeGraphEvent::XformChanged { node_id: id });
                        *is_applied = true;
                    } else {
                        out.dirty = true;
                    }
                }
            }

            // --- UsdPrim: register authored prim metadata ------------------
            SceneNode::UsdPrim { is_created, .. } if !*is_created => {
                if mode == EvalMode::Auto {
                    out.events
                        .push(NodeGraphEvent::UsdPrimCreate { node_id: id });
                    *is_created = true;
                } else {
                    out.dirty = true;
                }
            }

            // All other node types have no auto-compute logic.
            _ => {}
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: usize) -> GraphNodeId {
        GraphNodeId::from(egui_snarl::NodeId(n))
    }

    #[test]
    fn primitive_auto_creates_and_marks_created() {
        let mut node = SceneNode::primitive(bif_core::PrimitiveKind::Cube);
        let ctx = EvalCtx::new(vec![]);
        let out = node.evaluate(id(0), &ctx, EvalMode::Auto);
        assert_eq!(out.events.len(), 1);
        assert!(matches!(
            out.events[0],
            NodeGraphEvent::CreatePrimitive { .. }
        ));
        assert!(!out.dirty);
        assert!(matches!(
            node,
            SceneNode::Primitive {
                is_created: true,
                ..
            }
        ));
    }

    #[test]
    fn primitive_manual_marks_dirty_no_event() {
        let mut node = SceneNode::primitive(bif_core::PrimitiveKind::Sphere);
        let ctx = EvalCtx::new(vec![]);
        let out = node.evaluate(id(0), &ctx, EvalMode::Manual);
        assert!(out.events.is_empty());
        assert!(out.dirty);
        assert!(matches!(
            node,
            SceneNode::Primitive {
                is_created: false,
                ..
            }
        ));
    }

    #[test]
    fn primitive_already_created_is_noop() {
        let mut node = SceneNode::primitive(bif_core::PrimitiveKind::Cube);
        if let SceneNode::Primitive { is_created, .. } = &mut node {
            *is_created = true;
        }
        let ctx = EvalCtx::new(vec![]);
        let out = node.evaluate(id(0), &ctx, EvalMode::Auto);
        assert!(out.events.is_empty());
        assert!(!out.dirty);
    }

    #[test]
    fn scatter_surface_needs_input() {
        let mut node = SceneNode::scatter_points(); // defaults to Surface source
                                                    // No input connected -> no event, not dirty.
        let out = node.evaluate(id(0), &EvalCtx::new(vec![None]), EvalMode::Auto);
        assert!(out.events.is_empty());
        assert!(!out.dirty);
        // Input connected -> compute event + marked computed.
        let out = node.evaluate(id(0), &EvalCtx::new(vec![Some(id(1))]), EvalMode::Auto);
        assert_eq!(out.events.len(), 1);
        assert!(matches!(
            out.events[0],
            NodeGraphEvent::ScatterPointsCompute { .. }
        ));
        assert!(matches!(
            node,
            SceneNode::ScatterPoints {
                is_computed: true,
                ..
            }
        ));
    }

    #[test]
    fn instancer_computes_when_both_inputs_connected() {
        let mut node = SceneNode::point_instancer();
        let ctx = EvalCtx::new(vec![Some(id(1)), Some(id(2))]);
        let out = node.evaluate(id(0), &ctx, EvalMode::Auto);
        assert!(matches!(
            out.events.first(),
            Some(NodeGraphEvent::PointInstancerCompute {
                points_source_node,
                proto_source_node,
                ..
            }) if *points_source_node == id(1) && *proto_source_node == id(2)
        ));
        assert!(matches!(
            node,
            SceneNode::PointInstancer {
                is_computing: true,
                ..
            }
        ));
    }

    #[test]
    fn xform_waits_for_input() {
        let mut node = SceneNode::xform();
        let out = node.evaluate(id(0), &EvalCtx::new(vec![None]), EvalMode::Auto);
        assert!(out.events.is_empty());
        let out = node.evaluate(id(0), &EvalCtx::new(vec![Some(id(1))]), EvalMode::Auto);
        assert!(matches!(
            out.events.first(),
            Some(NodeGraphEvent::XformChanged { .. })
        ));
    }
}
