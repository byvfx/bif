//! Graph operation functions: dirty propagation and upstream collection.

use std::collections::{HashSet, VecDeque};

use egui_snarl::{InPinId, NodeId, OutPinId, Snarl};

use super::SceneNode;

/// Reset a node's computed state so it re-triggers auto-compute next frame.
pub(crate) fn mark_node_dirty(node_id: NodeId, snarl: &mut Snarl<SceneNode>) {
    match &mut snarl[node_id] {
        SceneNode::PointInstancer {
            is_instanced,
            is_computing,
            compute_failed,
            ..
        } => {
            *is_instanced = false;
            *is_computing = false;
            *compute_failed = false;
        }
        SceneNode::Primitive { is_created, .. } => {
            *is_created = false;
        }
        SceneNode::ScatterPoints { is_computed, .. } => {
            *is_computed = false;
        }
        _ => {}
    }
}

/// BFS-walk all downstream nodes from `start` and mark them dirty.
///
/// `start` itself is NOT dirtied — only its transitive downstream dependents.
pub(crate) fn propagate_dirty(start: NodeId, snarl: &mut Snarl<SceneNode>) {
    let mut queue = VecDeque::from([start]);
    let mut visited = HashSet::from([start]);

    while let Some(current) = queue.pop_front() {
        if current != start {
            mark_node_dirty(current, snarl);
        }

        let output_count = snarl[current].output_count();
        for out_idx in 0..output_count {
            let out_pin = snarl.out_pin(OutPinId {
                node: current,
                output: out_idx,
            });
            for remote in &out_pin.remotes {
                if visited.insert(remote.node) {
                    queue.push_back(remote.node);
                }
            }
        }
    }
}

/// BFS-walk all upstream nodes from `start` (following input connections).
///
/// Returns a set containing `start` and every node reachable by walking
/// backwards through input pins. Used to determine the active subgraph
/// when a display flag is set.
pub(crate) fn collect_upstream_nodes(start: NodeId, snarl: &Snarl<SceneNode>) -> HashSet<NodeId> {
    let mut visited = HashSet::from([start]);
    let mut queue = VecDeque::from([start]);

    while let Some(current) = queue.pop_front() {
        let input_count = snarl[current].input_count();
        for in_idx in 0..input_count {
            let in_pin = snarl.in_pin(InPinId {
                node: current,
                input: in_idx,
            });
            for remote in &in_pin.remotes {
                if visited.insert(remote.node) {
                    queue.push_back(remote.node);
                }
            }
        }
    }

    visited
}
