//! Scene-mutation command surface (issue #6 Phase 2).
//!
//! [`SceneCmd`] names the desync-prone per-node bookkeeping mutations that were
//! previously inlined across `node_dispatch.rs`: the coupling between
//! `node_outputs` (which protos / cloud a node owns) and `working_scene` (the
//! actual prototype / point-cloud storage), plus the point-preview GPU upload
//! derived from the clouds.
//!
//! Coarse, already-factored operations (`reload_working_scene`,
//! `load_primitive`, `load_usd_scene`, `export_scene`, `compact_materials`)
//! stay as direct method calls — they are not the brittle surface. Phase 3 will
//! move the per-node command *construction* into `node_graph/behavior.rs`; this
//! phase only establishes [`Renderer::execute`] as the single mutation site.

use crate::node_graph::GraphNodeId;
use crate::Renderer;

/// A single scene-bookkeeping mutation, applied by [`Renderer::execute`].
#[derive(Debug)]
pub enum SceneCmd {
    /// Drop all prototypes a node owns: take its `proto_ids`, remove + reindex
    /// each from `working_scene` (reverse order keeps indices valid). Does NOT
    /// `compact_materials` — callers that need GC do it explicitly.
    RemoveNodeProtos { node: GraphNodeId },
    /// Record the prototype indices a node now owns (overwrites prior set).
    RecordProtos {
        node: GraphNodeId,
        proto_ids: Vec<usize>,
    },
    /// Drop the point cloud a node owns: take its `cloud_id`, remove it from
    /// `working_scene`, and clear the node's scatter-surface mapping.
    RemoveNodeCloud { node: GraphNodeId },
    /// Assign a fresh cloud id, record it on the node, add the cloud to
    /// `working_scene`. The passed `cloud`'s `id` is overwritten.
    AddCloud {
        node: GraphNodeId,
        cloud: Box<bif_core::PointCloud>,
    },
    /// Re-derive all point positions from `working_scene.point_clouds` and
    /// upload them to the point-preview renderer (sets params-dirty).
    UploadPointPreview,
    /// Mark the scene graph dirty (cache rebuild on next frame).
    MarkSceneGraphDirty,
}

impl Renderer {
    /// Apply one [`SceneCmd`]. The single site that mutates the
    /// `node_outputs ↔ working_scene` proto/cloud coupling. Infallible: any
    /// fallible follow-up (e.g. `reload_working_scene`) stays at the call site.
    pub(crate) fn execute(&mut self, cmd: SceneCmd) {
        match cmd {
            SceneCmd::RemoveNodeProtos { node } => {
                let proto_ids = self
                    .nodes
                    .node_outputs
                    .get_mut(&node)
                    .map(|o| std::mem::take(&mut o.proto_ids));
                if self
                    .nodes
                    .node_outputs
                    .get(&node)
                    .is_some_and(|o| o.is_empty())
                {
                    self.nodes.node_outputs.remove(&node);
                }
                if let Some(proto_ids) = proto_ids {
                    // Reverse order so indices stay valid; reindex handles maps.
                    for &pid in proto_ids.iter().rev() {
                        self.remove_and_reindex_prototype(pid);
                    }
                }
            }
            SceneCmd::RecordProtos { node, proto_ids } => {
                self.nodes.node_outputs.entry(node).or_default().proto_ids = proto_ids;
            }
            SceneCmd::RemoveNodeCloud { node } => {
                let cloud_id = self
                    .nodes
                    .node_outputs
                    .get_mut(&node)
                    .and_then(|o| o.cloud_id.take());
                if self
                    .nodes
                    .node_outputs
                    .get(&node)
                    .is_some_and(|o| o.is_empty())
                {
                    self.nodes.node_outputs.remove(&node);
                }
                if let Some(cloud_id) = cloud_id {
                    self.scene.working_scene.remove_point_cloud(cloud_id);
                }
                // Surface mapping is rebuilt in reload_working_scene.
                self.nodes.node_scatter_surface_map.remove(&node);
            }
            SceneCmd::AddCloud { node, cloud } => {
                let cloud_id = self.nodes.next_cloud_id;
                self.nodes.next_cloud_id += 1;
                let mut cloud = *cloud;
                cloud.id = cloud_id;
                self.nodes.node_outputs.entry(node).or_default().cloud_id = Some(cloud_id);
                self.scene.working_scene.add_point_cloud(cloud);
            }
            SceneCmd::UploadPointPreview => {
                let all_positions: Vec<bif_math::Vec3> = self
                    .scene
                    .working_scene
                    .point_clouds
                    .iter()
                    .flat_map(|c| c.positions.iter().copied())
                    .collect();
                self.point_preview
                    .upload_points(&self.gpu.device, &self.gpu.queue, &all_positions);
                self.point_preview_params_dirty = true;
            }
            SceneCmd::MarkSceneGraphDirty => {
                self.nodes.scene_graph_dirty = true;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node_graph::GraphNodeId;

    // execute() needs a GPU-backed Renderer, so it is exercised via the
    // existing integration tests + manual smoke. These cheap tests cover the
    // enum's own surface.

    #[test]
    fn scene_cmd_is_debug() {
        let cmd = SceneCmd::RecordProtos {
            node: GraphNodeId::from(egui_snarl::NodeId(0)),
            proto_ids: vec![1, 2, 3],
        };
        let s = format!("{cmd:?}");
        assert!(s.contains("RecordProtos"));
        assert!(s.contains('3'));
    }

    #[test]
    fn mark_scene_graph_dirty_is_unit_variant() {
        let cmd = SceneCmd::MarkSceneGraphDirty;
        assert_eq!(format!("{cmd:?}"), "MarkSceneGraphDirty");
    }
}
