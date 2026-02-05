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
    /// Pre-convert scene textures to .tx format
    ConvertTexturesToTx,
    /// Load an HDRI environment map
    LoadHdri {
        path: String,
        rotation: f32,
        intensity: f32,
        show_background: bool,
    },
    /// Update HDRI parameters (no reload needed)
    UpdateHdriParams {
        rotation: f32,
        intensity: f32,
        show_background: bool,
    },
}

/// Pin types for node connections
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PinType {
    /// USD Scene data (stage/prims)
    Scene,
    /// Rendered image output
    Image,
    /// Environment lighting data
    Environment,
}

impl PinType {
    /// Get the color for this pin type
    pub fn color(&self) -> egui::Color32 {
        match self {
            PinType::Scene => egui::Color32::from_rgb(100, 200, 100), // Green for scene data
            PinType::Image => egui::Color32::from_rgb(200, 150, 50),  // Orange for images
            PinType::Environment => egui::Color32::from_rgb(100, 150, 255), // Blue for environment
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
        /// Whether .tx conversion is in progress
        is_converting_tx: bool,
        /// Status message from last .tx conversion
        tx_status: Option<String>,
    },
    /// HDRI environment map for IBL lighting
    HdriEnvironment {
        /// Path to the HDR file
        file_path: String,
        /// Whether the file is loaded
        is_loaded: bool,
        /// Whether IBL is currently being generated
        is_loading: bool,
        /// Rotation in degrees
        rotation: f32,
        /// Intensity multiplier
        intensity: f32,
        /// Whether to show environment as background
        show_background: bool,
        /// Error message if loading failed
        error: Option<String>,
        /// Last HDRI load time (seconds)
        last_load_secs: Option<f64>,
        /// Last IBL compute time (seconds)
        last_compute_secs: Option<f64>,
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
            is_converting_tx: false,
            tx_status: None,
        }
    }

    /// Create a new HDRI Environment node
    pub fn hdri_environment() -> Self {
        Self::HdriEnvironment {
            file_path: String::new(),
            is_loaded: false,
            is_loading: false,
            rotation: 0.0,
            intensity: 1.0,
            show_background: true,
            error: None,
            last_load_secs: None,
            last_compute_secs: None,
        }
    }

    /// Get the display name for this node
    pub fn name(&self) -> &'static str {
        match self {
            SceneNode::UsdRead { .. } => "USD Read",
            SceneNode::IvarRender { .. } => "Ivar Render",
            SceneNode::HdriEnvironment { .. } => "HDRI Environment",
        }
    }

    /// Get input pin count
    pub fn input_count(&self) -> usize {
        match self {
            SceneNode::UsdRead { .. } => 0,
            SceneNode::IvarRender { .. } => 2, // scene + environment
            SceneNode::HdriEnvironment { .. } => 0,
        }
    }

    /// Get output pin count
    pub fn output_count(&self) -> usize {
        match self {
            SceneNode::UsdRead { .. } => 1,
            SceneNode::IvarRender { .. } => 1,
            SceneNode::HdriEnvironment { .. } => 1,
        }
    }

    /// Get input pin info
    pub fn input_pin(&self, index: usize) -> Option<(&'static str, PinType)> {
        match self {
            SceneNode::UsdRead { .. } => None,
            SceneNode::IvarRender { .. } => match index {
                0 => Some(("scene", PinType::Scene)),
                1 => Some(("env", PinType::Environment)),
                _ => None,
            },
            SceneNode::HdriEnvironment { .. } => None,
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
            SceneNode::HdriEnvironment { .. } => match index {
                0 => Some(("env", PinType::Environment)),
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
                } else if ui.button("Convert to .tx").clicked() {
                    self.events.push(NodeGraphEvent::ConvertTexturesToTx);
                    *is_converting_tx = true;
                    *tx_status = None;
                }
                if let Some(status) = tx_status {
                    ui.colored_label(egui::Color32::GREEN, status.as_str());
                }
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

    /// Add an HDRI Environment node at the given position
    pub fn add_hdri_environment(&mut self, pos: egui::Pos2) -> NodeId {
        self.snarl.insert_node(pos, SceneNode::hdri_environment())
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

    pub fn mark_ivar_render_complete(&mut self) {
        let node_ids: Vec<_> = self.snarl.node_ids().map(|(id, _)| id).collect();
        for node_id in node_ids {
            if let SceneNode::IvarRender { is_rendering, .. } = &mut self.snarl[node_id] {
                *is_rendering = false;
            }
        }
    }

    /// Mark .tx conversion as complete with a status message.
    pub fn mark_tx_conversion_complete(&mut self, status: String) {
        let node_ids: Vec<_> = self.snarl.node_ids().map(|(id, _)| id).collect();
        for node_id in node_ids {
            if let SceneNode::IvarRender {
                is_converting_tx,
                tx_status,
                ..
            } = &mut self.snarl[node_id]
            {
                *is_converting_tx = false;
                *tx_status = Some(status.clone());
            }
        }
    }

    /// Mark an HDRI Environment node as loading (IBL generation in progress).
    pub fn mark_hdri_loading(&mut self, path: &str) {
        let node_ids: Vec<_> = self.snarl.node_ids().map(|(id, _)| id).collect();
        for node_id in node_ids {
            if let SceneNode::HdriEnvironment {
                file_path,
                is_loading,
                ..
            } = &mut self.snarl[node_id]
            {
                if file_path == path {
                    *is_loading = true;
                }
            }
        }
    }

    /// Mark an HDRI Environment node as loaded.
    pub fn mark_hdri_loaded(&mut self, path: &str, load_secs: Option<f64>, compute_secs: Option<f64>) {
        let node_ids: Vec<_> = self.snarl.node_ids().map(|(id, _)| id).collect();
        for node_id in node_ids {
            if let SceneNode::HdriEnvironment {
                file_path,
                is_loaded,
                is_loading,
                error,
                last_load_secs,
                last_compute_secs,
                ..
            } = &mut self.snarl[node_id]
            {
                if file_path == path {
                    *is_loaded = true;
                    *is_loading = false;
                    *error = None;
                    if let Some(secs) = load_secs {
                        *last_load_secs = Some(secs);
                    }
                    if let Some(secs) = compute_secs {
                        *last_compute_secs = Some(secs);
                    }
                }
            }
        }
    }

    /// Mark an HDRI Environment node as having an error.
    pub fn mark_hdri_error(&mut self, path: &str, err_msg: String) {
        let node_ids: Vec<_> = self.snarl.node_ids().map(|(id, _)| id).collect();
        for node_id in node_ids {
            if let SceneNode::HdriEnvironment {
                file_path,
                is_loaded,
                is_loading,
                error,
                last_load_secs,
                last_compute_secs,
                ..
            } = &mut self.snarl[node_id]
            {
                if file_path == path {
                    *is_loaded = false;
                    *is_loading = false;
                    *error = Some(err_msg.clone());
                    *last_load_secs = None;
                    *last_compute_secs = None;
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
        if ui.button("+ HDRI Env").clicked() {
            state.add_hdri_environment(egui::pos2(50.0, 200.0));
        }
        if ui.button("+ Ivar Render").clicked() {
            state.add_ivar_render(egui::pos2(350.0, 100.0));
        }
        ui.separator();
        if ui.button("Del Selected").clicked() {
            state.delete_selected();
        }
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
        assert_eq!(render_node.input_count(), 2); // scene + environment
        assert_eq!(render_node.output_count(), 1);

        let hdri_node = SceneNode::hdri_environment();
        assert_eq!(hdri_node.name(), "HDRI Environment");
        assert_eq!(hdri_node.input_count(), 0);
        assert_eq!(hdri_node.output_count(), 1);
    }

    #[test]
    fn test_pin_types() {
        let read_node = SceneNode::usd_read();
        assert_eq!(read_node.output_pin(0), Some(("scene", PinType::Scene)));

        let render_node = SceneNode::ivar_render();
        assert_eq!(render_node.input_pin(0), Some(("scene", PinType::Scene)));
        assert_eq!(
            render_node.input_pin(1),
            Some(("env", PinType::Environment))
        );
        assert_eq!(render_node.output_pin(0), Some(("image", PinType::Image)));

        let hdri_node = SceneNode::hdri_environment();
        assert_eq!(hdri_node.output_pin(0), Some(("env", PinType::Environment)));
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
