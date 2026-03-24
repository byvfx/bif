/// Framework-agnostic node identifier for the node graph.
///
/// Decouples scene evaluation and persistence from the UI framework (egui_snarl).
/// Maps bidirectionally to `egui_snarl::NodeId` at the UI boundary.
#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct GraphNodeId(pub u64);

impl From<egui_snarl::NodeId> for GraphNodeId {
    fn from(id: egui_snarl::NodeId) -> Self {
        // egui_snarl::NodeId wraps a usize internally
        Self(id.0 as u64)
    }
}

impl From<GraphNodeId> for egui_snarl::NodeId {
    fn from(id: GraphNodeId) -> Self {
        egui_snarl::NodeId(id.0 as usize)
    }
}

impl std::fmt::Display for GraphNodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "node:{}", self.0)
    }
}
