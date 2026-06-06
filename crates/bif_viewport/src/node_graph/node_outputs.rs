//! Per-node output bookkeeping: the prototype indices and optional point-cloud
//! id a node owns in `working_scene`. Replaces the old parallel
//! `node_proto_map` + `node_cloud_map` (issue #6 Phase 1).

/// Outputs a single graph node has materialized into the working scene.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NodeOutputs {
    /// Prototype indices into `working_scene.prototypes` owned by this node.
    pub proto_ids: Vec<usize>,
    /// Point-cloud id into `working_scene.point_clouds`, if this node made one.
    pub cloud_id: Option<usize>,
}

impl NodeOutputs {
    /// True when the node owns no protos and no cloud (entry is prunable).
    pub fn is_empty(&self) -> bool {
        self.proto_ids.is_empty() && self.cloud_id.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_empty() {
        let o = NodeOutputs::default();
        assert!(o.is_empty());
        assert!(o.proto_ids.is_empty());
        assert_eq!(o.cloud_id, None);
    }

    #[test]
    fn populated_is_not_empty() {
        let mut o = NodeOutputs::default();
        o.proto_ids.push(3);
        assert!(!o.is_empty());
        o.proto_ids.clear();
        o.cloud_id = Some(0);
        assert!(!o.is_empty());
    }
}
