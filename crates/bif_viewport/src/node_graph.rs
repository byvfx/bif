//! Node Graph for Scene Assembly
//!
//! Provides a Nuke-style node graph for compositing USD scenes
//! and connecting them to render outputs.
//!
//! # Node Types
//! - **USD Read**: Load a USD file (.usda, .usdc, .usd)
//! - **Ivar Render**: CPU path trace the connected scene
//!
//! # Future Nodes (TODO)
//! - Merge: Combine multiple USD stages
//! - Transform: Apply transform to stage
//! - Sublayer: USD layer composition
//! - Variant: Switch USD variant sets

use egui_snarl::{
    ui::{PinInfo, SnarlStyle, SnarlViewer},
    InPin, InPinId, NodeId, OutPin, OutPinId, Snarl,
};

/// Events that the node graph can emit to the parent UI
#[derive(Debug, Clone)]
pub enum NodeGraphEvent {
    /// Load a USD file at the given path
    LoadUsdFile(String),
    /// Start an Ivar render with the given SPP
    StartRender { spp: u32 },
}

/// Pin types for node connections
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PinType {
    /// USD Scene data (stage/prims)
    Scene,
    /// Rendered image output
    Image,
}

impl PinType {
    /// Get the color for this pin type
    pub fn color(&self) -> egui::Color32 {
        match self {
            PinType::Scene => egui::Color32::from_rgb(100, 200, 100), // Green for scene data
            PinType::Image => egui::Color32::from_rgb(200, 150, 50),  // Orange for images
        }
    }
}

/// A node in the scene graph
#[derive(Clone)]
pub enum SceneNode {
    /// Load a USD file
    UsdRead {
        /// Path to the USD file
        file_path: String,
        /// Whether the file is loaded successfully
        is_loaded: bool,
        /// Error message if loading failed
        error: Option<String>,
    },
    /// Render the scene with Ivar CPU path tracer
    IvarRender {
        /// Samples per pixel
        spp: u32,
        /// Whether currently rendering
        is_rendering: bool,
    },
}

impl SceneNode {
    /// Create a new USD Read node
    pub fn usd_read() -> Self {
        Self::UsdRead {
            file_path: String::new(),
            is_loaded: false,
            error: None,
        }
    }

    /// Create a new USD Read node with a file path
    pub fn usd_read_with_path(path: String) -> Self {
        Self::UsdRead {
            file_path: path,
            is_loaded: false,
            error: None,
        }
    }

    /// Create a new Ivar Render node
    pub fn ivar_render() -> Self {
        Self::IvarRender {
            spp: 16,
            is_rendering: false,
        }
    }

    /// Get the display name for this node
    pub fn name(&self) -> &'static str {
        match self {
            SceneNode::UsdRead { .. } => "USD Read",
            SceneNode::IvarRender { .. } => "Ivar Render",
        }
    }

    /// Get input pin count
    pub fn input_count(&self) -> usize {
        match self {
            SceneNode::UsdRead { .. } => 0,
            SceneNode::IvarRender { .. } => 1,
        }
    }

    /// Get output pin count
    pub fn output_count(&self) -> usize {
        match self {
            SceneNode::UsdRead { .. } => 1,
            SceneNode::IvarRender { .. } => 1,
        }
    }

    /// Get input pin info
    pub fn input_pin(&self, index: usize) -> Option<(&'static str, PinType)> {
        match self {
            SceneNode::UsdRead { .. } => None,
            SceneNode::IvarRender { .. } => match index {
                0 => Some(("scene", PinType::Scene)),
                _ => None,
            },
        }
    }

    /// Get output pin info
    pub fn output_pin(&self, index: usize) -> Option<(&'static str, PinType)> {
        match self {
            SceneNode::UsdRead { .. } => match index {
                0 => Some(("scene", PinType::Scene)),
                _ => None,
            },
            SceneNode::IvarRender { .. } => match index {
                0 => Some(("image", PinType::Image)),
                _ => None,
            },
        }
    }
}

/// Viewer implementation for the scene node graph
pub struct SceneNodeViewer {
    /// Events to be processed by the parent
    pub events: Vec<NodeGraphEvent>,
}

impl SceneNodeViewer {
    pub fn new() -> Self {
        Self { events: Vec::new() }
    }
}

impl Default for SceneNodeViewer {
    fn default() -> Self {
        Self::new()
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

    fn show_body(
        &mut self,
        node_id: NodeId,
        _inputs: &[InPin],
        _outputs: &[OutPin],
        ui: &mut egui::Ui,
        _scale: f32,
        snarl: &mut Snarl<SceneNode>,
    ) {
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
                            self.events
                                .push(NodeGraphEvent::LoadUsdFile(file_path.clone()));
                        }
                    }

                    if ui.button("Load").clicked() && !file_path.is_empty() {
                        self.events
                            .push(NodeGraphEvent::LoadUsdFile(file_path.clone()));
                    }
                });

                if *is_loaded {
                    ui.colored_label(egui::Color32::GREEN, "✓ Loaded");
                } else if let Some(err) = error {
                    ui.colored_label(egui::Color32::RED, format!("✗ {}", err));
                }
            }
            SceneNode::IvarRender { spp, is_rendering } => {
                ui.horizontal(|ui| {
                    ui.label("SPP:");
                    ui.add(egui::DragValue::new(spp).range(1..=1024));
                });

                if *is_rendering {
                    ui.colored_label(egui::Color32::YELLOW, "⟳ Rendering...");
                } else if ui.button("Render").clicked() {
                    self.events.push(NodeGraphEvent::StartRender { spp: *spp });
                    *is_rendering = true;
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
                // Valid connection
                snarl.connect(from.id, to.id);
            }
        }
    }

    fn disconnect(&mut self, from: &OutPin, to: &InPin, snarl: &mut Snarl<SceneNode>) {
        snarl.disconnect(from.id, to.id);
    }
}

/// State for the node graph panel
pub struct NodeGraphState {
    /// The node graph data
    pub snarl: Snarl<SceneNode>,
    /// Visual style for the graph
    pub style: SnarlStyle,
    /// Currently selected node (if any)
    pub selected_node: Option<NodeId>,
}

impl Default for NodeGraphState {
    fn default() -> Self {
        Self::new()
    }
}

impl NodeGraphState {
    /// Create a new node graph state
    pub fn new() -> Self {
        let snarl = Snarl::new();

        // Start with empty graph - user will add nodes
        Self {
            snarl,
            style: SnarlStyle::default(),
            selected_node: None,
        }
    }

    /// Create a new node graph state with default nodes
    pub fn with_default_nodes() -> Self {
        let mut snarl = Snarl::new();

        // Add default nodes for demonstration
        let read_node = snarl.insert_node(egui::pos2(100.0, 100.0), SceneNode::usd_read());
        let render_node = snarl.insert_node(egui::pos2(400.0, 100.0), SceneNode::ivar_render());

        // Connect them
        snarl.connect(
            OutPinId {
                node: read_node,
                output: 0,
            },
            InPinId {
                node: render_node,
                input: 0,
            },
        );

        Self {
            snarl,
            style: SnarlStyle::default(),
            selected_node: None,
        }
    }

    /// Add a USD Read node at the given position
    pub fn add_usd_read(&mut self, pos: egui::Pos2) -> NodeId {
        self.snarl.insert_node(pos, SceneNode::usd_read())
    }

    /// Add an Ivar Render node at the given position
    pub fn add_ivar_render(&mut self, pos: egui::Pos2) -> NodeId {
        self.snarl.insert_node(pos, SceneNode::ivar_render())
    }

    /// Delete the selected node
    pub fn delete_selected(&mut self) {
        if let Some(node_id) = self.selected_node.take() {
            self.snarl.remove_node(node_id);
        }
    }

    /// Mark a USD Read node as loaded
    pub fn mark_node_loaded(&mut self, file_path: &str) {
        // Collect node IDs first to avoid borrow issues
        let node_ids: Vec<_> = self.snarl.node_ids().map(|(id, _)| id).collect();
        for node_id in node_ids {
            if let SceneNode::UsdRead {
                file_path: path,
                is_loaded,
                error,
            } = &mut self.snarl[node_id]
            {
                if path == file_path {
                    *is_loaded = true;
                    *error = None;
                }
            }
        }
    }

    /// Mark a USD Read node as having an error
    pub fn mark_node_error(&mut self, file_path: &str, err_msg: String) {
        // Collect node IDs first to avoid borrow issues
        let node_ids: Vec<_> = self.snarl.node_ids().map(|(id, _)| id).collect();
        for node_id in node_ids {
            if let SceneNode::UsdRead {
                file_path: path,
                is_loaded,
                error,
            } = &mut self.snarl[node_id]
            {
                if path == file_path {
                    *is_loaded = false;
                    *error = Some(err_msg.clone());
                }
            }
        }
    }
}

/// Render the node graph UI
/// Returns any events that should be processed by the parent
pub fn render_node_graph(ui: &mut egui::Ui, state: &mut NodeGraphState) -> Vec<NodeGraphEvent> {
    let mut viewer = SceneNodeViewer::new();

    // Handle keyboard input for delete
    if ui.input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace)) {
        state.delete_selected();
    }

    // Context menu for adding nodes
    ui.horizontal(|ui| {
        ui.label("Nodes:");
        if ui.button("+ USD Read").clicked() {
            state.add_usd_read(egui::pos2(50.0, 50.0));
        }
        if ui.button("+ Ivar Render").clicked() {
            state.add_ivar_render(egui::pos2(200.0, 50.0));
        }
        ui.separator();
        if ui.button("🗑 Delete Selected").clicked() {
            state.delete_selected();
        }
        ui.separator();
        ui.label("(Del key to delete, drag to pan, scroll to zoom)");
    });

    ui.separator();

    // Render the snarl node graph
    state.snarl.show(
        &mut viewer,
        &state.style,
        egui::Id::new("scene_node_graph"),
        ui,
    );

    viewer.events
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_creation() {
        let read_node = SceneNode::usd_read();
        assert_eq!(read_node.name(), "USD Read");
        assert_eq!(read_node.input_count(), 0);
        assert_eq!(read_node.output_count(), 1);

        let render_node = SceneNode::ivar_render();
        assert_eq!(render_node.name(), "Ivar Render");
        assert_eq!(render_node.input_count(), 1);
        assert_eq!(render_node.output_count(), 1);
    }

    #[test]
    fn test_pin_types() {
        let read_node = SceneNode::usd_read();
        assert_eq!(read_node.output_pin(0), Some(("scene", PinType::Scene)));

        let render_node = SceneNode::ivar_render();
        assert_eq!(render_node.input_pin(0), Some(("scene", PinType::Scene)));
        assert_eq!(render_node.output_pin(0), Some(("image", PinType::Image)));
    }

    #[test]
    fn test_node_graph_state() {
        let state = NodeGraphState::with_default_nodes();
        // Should have 2 default nodes
        assert!(state.snarl.node_ids().count() >= 2);
    }

    #[test]
    fn test_empty_node_graph() {
        let state = NodeGraphState::new();
        // Should start empty
        assert_eq!(state.snarl.node_ids().count(), 0);
    }
}
