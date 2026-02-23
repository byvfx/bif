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

/// Parameters for scatter points computation.
#[derive(Debug, Clone)]
pub struct ScatterPointsParams {
    /// Point generation source.
    pub source: bif_core::PointSource,
    /// Number of points to generate.
    pub count: u32,
    /// Safety cap on total points.
    pub max_point_limit: u32,
    /// Random seed for reproducibility.
    pub seed: u64,
    // Surface-specific
    /// Scatter distribution mode (Random / PoissonDisk).
    pub scatter_mode: bif_core::scatter::ScatterMode,
    /// Minimum distance for Poisson disk.
    pub min_distance: f32,
    /// Align point orientation to surface normal.
    pub align_to_normal: bool,
    // Grid-specific
    /// Grid bounding dimensions [X, Y, Z].
    pub grid_size: [f32; 3],
    /// Distance between grid points.
    pub grid_spacing: f32,
    // Sphere-specific
    /// Sphere radius.
    pub sphere_radius: f32,
    /// If true, points on surface only; false = volume fill.
    pub sphere_on_surface: bool,
    // Relax
    /// Number of repulsion relaxation iterations.
    pub relax_iterations: u32,
    /// Multiplier on relax radius.
    pub scale_radii: f32,
    /// Maximum relax radius cap.
    pub max_relax_radius: f32,
    // Per-point attrs
    /// Minimum per-point scale.
    pub scale_min: f32,
    /// Maximum per-point scale.
    pub scale_max: f32,
    /// Per-point rotation range in degrees.
    pub rotation_range: f32,
    /// Prototype ID to scatter on (Surface mode).
    pub target_proto_id: Option<usize>,
}

/// Events that the node graph can emit to the parent UI
#[derive(Debug, Clone)]
pub enum NodeGraphEvent {
    /// Load a USD file at the given path (from a specific UsdRead node)
    LoadUsdFile { path: String, node_id: NodeId },
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
    /// Create a procedural primitive
    CreatePrimitive {
        kind: bif_core::PrimitiveKind,
        size: f32,
        node_id: NodeId,
    },
    /// Compute scatter points
    ScatterPointsCompute {
        node_id: NodeId,
        params: ScatterPointsParams,
    },
    /// Expand point cloud into geometry instances via Point Instancer
    PointInstancerCompute {
        /// The instancer node itself
        node_id: NodeId,
        /// Node that provides the point cloud (Scatter Points)
        points_source_node: NodeId,
        /// Node that provides the prototype mesh (Primitive or UsdRead)
        proto_source_node: NodeId,
    },
    /// Update point preview appearance (size, color) without recompute.
    ///
    /// NOTE: All scatter nodes share a single `PointPreviewRenderer`. The last
    /// node to emit this event wins. Per-node rendering requires one renderer
    /// per scatter node or per-point color attributes in the storage buffer.
    PointPreviewUpdate {
        /// Source scatter node
        node_id: NodeId,
        /// Point size in pixels
        point_size: f32,
        /// Point color RGBA
        point_color: [f32; 4],
    },
    /// Invalidate an instancer (clear cached results, reload scene)
    InstancerInvalidate { node_id: NodeId },
    /// Export USD from a UsdExport node
    ExportUsd {
        node_id: NodeId,
        output_path: String,
        as_sublayer: bool,
        export_root: String,
    },
    /// Xform node T/R/S changed — rebuild scene transforms
    XformChanged { node_id: NodeId },
    /// Set display flag on a node (which node feeds viewport/export)
    SetDisplayNode(NodeId),
    /// Select a node (for keyboard delete, property inspector, etc.)
    SelectNode(NodeId),
    /// Delete a node by ID
    DeleteNode(NodeId),
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
    /// Procedural primitive (cube, sphere, camera wireframe)
    Primitive {
        /// Kind of primitive
        kind: bif_core::PrimitiveKind,
        /// Size parameter
        size: f32,
        /// Whether geometry has been created
        is_created: bool,
    },
    /// Scatter points on surface, grid, or sphere
    ScatterPoints {
        /// Point generation source
        source: bif_core::PointSource,
        /// Number of points to generate
        count: u32,
        /// Safety cap on total points
        max_point_limit: u32,
        /// Random seed
        seed: u64,
        /// Distribution mode (Surface only)
        scatter_mode: bif_core::scatter::ScatterMode,
        /// Minimum distance for Poisson disk (Surface only)
        min_distance: f32,
        /// Align to surface normal (Surface only)
        align_to_normal: bool,
        /// Grid dimensions [X, Y, Z] (Grid only)
        grid_size: [f32; 3],
        /// Distance between grid points (Grid only)
        grid_spacing: f32,
        /// Sphere radius (Sphere only)
        sphere_radius: f32,
        /// Surface-only vs volume fill (Sphere only)
        sphere_on_surface: bool,
        /// Lloyd relaxation iterations (0 = none)
        relax_iterations: u32,
        /// Multiplier on relax radius
        scale_radii: f32,
        /// Cap on relax radius
        max_relax_radius: f32,
        /// Minimum scale for per-point attrs
        scale_min: f32,
        /// Maximum scale for per-point attrs
        scale_max: f32,
        /// Rotation range in degrees
        rotation_range: f32,
        /// Prototype ID to scatter on (Surface: None = first)
        target_proto_id: Option<usize>,
        /// Whether scatter has been computed
        is_computed: bool,
        /// Point preview size in pixels
        point_size: f32,
        /// Point preview color (RGBA)
        point_color: [f32; 4],
    },
    /// Expand point clouds into geometry instances
    PointInstancer {
        /// Number of expanded instances (display only)
        instance_count: usize,
        /// Whether instancing has been computed
        is_instanced: bool,
        /// Whether a compute event has been emitted this frame (guards duplicate emission)
        is_computing: bool,
        /// Whether compute failed (prevents infinite retry loop)
        compute_failed: bool,
    },
    /// USD Export sink node — writes scene to disk
    UsdExport {
        /// Output file path (.usda or .usdc)
        output_path: String,
        /// Compose over original USD (add source as sublayer)
        as_sublayer: bool,
        /// Root prim path for BIF-authored prims (default "/BIF")
        export_root: String,
        /// Whether last export succeeded
        is_exported: bool,
        /// Status text from last export attempt
        last_result: Option<String>,
    },
    /// Transform node — applies translate/rotate/scale to upstream scene
    Xform {
        /// Translation offset
        translate: [f32; 3],
        /// Euler rotation in degrees (XYZ order)
        rotate: [f32; 3],
        /// Scale factors
        scale: [f32; 3],
        /// Prim filter (placeholder — non-functional in V1, always all upstream)
        prim_filter: String,
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

    /// Create a new Primitive node
    pub fn primitive(kind: bif_core::PrimitiveKind) -> Self {
        let size = match kind {
            bif_core::PrimitiveKind::Cube => 1.0,
            bif_core::PrimitiveKind::Sphere => 0.5,
            bif_core::PrimitiveKind::Camera => 1.0,
        };
        Self::Primitive {
            kind,
            size,
            is_created: false,
        }
    }

    /// Create a new Scatter Points node.
    pub fn scatter_points() -> Self {
        Self::ScatterPoints {
            source: bif_core::PointSource::Surface,
            count: 1000,
            max_point_limit: 1_000_000,
            seed: 42,
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

    /// Create a new Point Instancer node
    pub fn point_instancer() -> Self {
        Self::PointInstancer {
            instance_count: 0,
            is_instanced: false,
            is_computing: false,
            compute_failed: false,
        }
    }

    /// Create a new USD Export node
    pub fn usd_export() -> Self {
        Self::UsdExport {
            output_path: String::new(),
            as_sublayer: true,
            export_root: "/BIF".to_string(),
            is_exported: false,
            last_result: None,
        }
    }

    /// Create a new Xform node
    pub fn xform() -> Self {
        Self::Xform {
            translate: [0.0, 0.0, 0.0],
            rotate: [0.0, 0.0, 0.0],
            scale: [1.0, 1.0, 1.0],
            prim_filter: String::new(),
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
            SceneNode::Primitive { kind, .. } => match kind {
                bif_core::PrimitiveKind::Cube => "Cube",
                bif_core::PrimitiveKind::Sphere => "Sphere",
                bif_core::PrimitiveKind::Camera => "Camera",
            },
            SceneNode::ScatterPoints { .. } => "Scatter Points",
            SceneNode::PointInstancer { .. } => "Point Instancer",
            SceneNode::UsdExport { .. } => "USD Export",
            SceneNode::Xform { .. } => "Xform",
            SceneNode::HdriEnvironment { .. } => "HDRI Environment",
        }
    }

    /// Get input pin count
    pub fn input_count(&self) -> usize {
        match self {
            SceneNode::UsdRead { .. } => 0,
            SceneNode::IvarRender { .. } => 2, // scene + environment
            SceneNode::Primitive { .. } => 0,
            SceneNode::ScatterPoints { source, .. } => match source {
                bif_core::PointSource::Surface => 1,
                bif_core::PointSource::Grid | bif_core::PointSource::Sphere => 0,
            },
            SceneNode::PointInstancer { .. } => 2, // points + prototype
            SceneNode::UsdExport { .. } => 1,      // scene input
            SceneNode::Xform { .. } => 1,          // scene input
            SceneNode::HdriEnvironment { .. } => 0,
        }
    }

    /// Get output pin count
    pub fn output_count(&self) -> usize {
        match self {
            SceneNode::UsdRead { .. } => 1,
            SceneNode::IvarRender { .. } => 1,
            SceneNode::Primitive { .. } => 1,
            SceneNode::ScatterPoints { .. } => 1,
            SceneNode::PointInstancer { .. } => 1,
            SceneNode::UsdExport { .. } => 0, // sink node, no output
            SceneNode::Xform { .. } => 1,
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
            SceneNode::Primitive { .. } => None,
            SceneNode::ScatterPoints { source, .. } => match source {
                bif_core::PointSource::Surface => match index {
                    0 => Some(("scene", PinType::Scene)),
                    _ => None,
                },
                bif_core::PointSource::Grid | bif_core::PointSource::Sphere => None,
            },
            SceneNode::PointInstancer { .. } => match index {
                0 => Some(("points", PinType::Scene)),
                1 => Some(("proto", PinType::Scene)),
                _ => None,
            },
            SceneNode::UsdExport { .. } => match index {
                0 => Some(("scene", PinType::Scene)),
                _ => None,
            },
            SceneNode::Xform { .. } => match index {
                0 => Some(("scene", PinType::Scene)),
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
            SceneNode::Primitive { .. } => match index {
                0 => Some(("scene", PinType::Scene)),
                _ => None,
            },
            SceneNode::ScatterPoints { .. } => match index {
                0 => Some(("points", PinType::Scene)),
                _ => None,
            },
            SceneNode::PointInstancer { .. } => match index {
                0 => Some(("scene", PinType::Scene)),
                _ => None,
            },
            SceneNode::UsdExport { .. } => None, // sink node
            SceneNode::Xform { .. } => match index {
                0 => Some(("scene", PinType::Scene)),
                _ => None,
            },
            SceneNode::HdriEnvironment { .. } => match index {
                0 => Some(("env", PinType::Environment)),
                _ => None,
            },
        }
    }
}

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
pub struct SceneNodeViewer {
    /// Events to be processed by the parent
    pub events: Vec<NodeGraphEvent>,
    /// Which node has the display flag (for blue indicator)
    pub display_node: Option<NodeId>,
}

impl SceneNodeViewer {
    pub fn new(display_node: Option<NodeId>) -> Self {
        Self {
            events: Vec::new(),
            display_node,
        }
    }
}

impl Default for SceneNodeViewer {
    fn default() -> Self {
        Self::new(None)
    }
}

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
    use std::collections::VecDeque;

    let mut queue = VecDeque::from([start]);
    let mut visited = std::collections::HashSet::from([start]);

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
pub(crate) fn collect_upstream_nodes(
    start: NodeId,
    snarl: &Snarl<SceneNode>,
) -> std::collections::HashSet<NodeId> {
    use std::collections::{HashSet, VecDeque};

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
        // Detect click on node body to select it
        if ui.rect_contains_pointer(ui.max_rect()) && ui.input(|i| i.pointer.any_pressed()) {
            self.events.push(NodeGraphEvent::SelectNode(node_id));
        }

        // Display flag indicator (blue dot like Houdini)
        if self.display_node == Some(node_id) {
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
                ui.painter().circle_filled(
                    rect.center(),
                    4.0,
                    egui::Color32::from_rgb(80, 140, 255),
                );
                ui.colored_label(egui::Color32::from_rgb(80, 140, 255), "Display");
            });
        }

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
                            self.events.push(NodeGraphEvent::LoadUsdFile {
                                path: file_path.clone(),
                                node_id,
                            });
                        }
                    }

                    if ui.button("Load").clicked() && !file_path.is_empty() {
                        self.events.push(NodeGraphEvent::LoadUsdFile {
                            path: file_path.clone(),
                            node_id,
                        });
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
            SceneNode::Primitive {
                kind,
                size,
                is_created,
            } => {
                ui.horizontal(|ui| {
                    ui.label("Size:");
                    if ui
                        .add(egui::DragValue::new(size).speed(0.01).range(0.01..=100.0))
                        .changed()
                    {
                        *is_created = false; // Need to recreate
                    }
                });

                if !*is_created {
                    self.events.push(NodeGraphEvent::CreatePrimitive {
                        kind: *kind,
                        size: *size,
                        node_id,
                    });
                    *is_created = true;
                }
                ui.colored_label(egui::Color32::GREEN, "Created");
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
                point_size,
                point_color,
            } => {
                // Source dropdown
                ui.horizontal(|ui| {
                    ui.label("Source:");
                    egui::ComboBox::from_id_salt("scatter_source")
                        .selected_text(match source {
                            bif_core::PointSource::Surface => "Surface",
                            bif_core::PointSource::Grid => "Grid",
                            bif_core::PointSource::Sphere => "Sphere",
                        })
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_value(source, bif_core::PointSource::Surface, "Surface")
                                .changed()
                            {
                                *is_computed = false;
                            }
                            if ui
                                .selectable_value(source, bif_core::PointSource::Grid, "Grid")
                                .changed()
                            {
                                *is_computed = false;
                            }
                            if ui
                                .selectable_value(source, bif_core::PointSource::Sphere, "Sphere")
                                .changed()
                            {
                                *is_computed = false;
                            }
                        });
                });

                // Source-specific params
                match source {
                    bif_core::PointSource::Surface => {
                        ui.horizontal(|ui| {
                            ui.label("Count:");
                            if ui
                                .add(egui::DragValue::new(count).range(1..=100_000))
                                .changed()
                            {
                                *is_computed = false;
                            }
                        });

                        ui.horizontal(|ui| {
                            ui.label("Mode:");
                            let mut is_poisson =
                                *scatter_mode == bif_core::scatter::ScatterMode::PoissonDisk;
                            if ui.checkbox(&mut is_poisson, "Poisson Disk").changed() {
                                *scatter_mode = if is_poisson {
                                    bif_core::scatter::ScatterMode::PoissonDisk
                                } else {
                                    bif_core::scatter::ScatterMode::Random
                                };
                                *is_computed = false;
                            }
                        });

                        if *scatter_mode == bif_core::scatter::ScatterMode::PoissonDisk {
                            ui.horizontal(|ui| {
                                ui.label("Min Dist:");
                                if ui
                                    .add(
                                        egui::DragValue::new(min_distance)
                                            .speed(0.01)
                                            .range(0.01..=100.0),
                                    )
                                    .changed()
                                {
                                    *is_computed = false;
                                }
                            });
                        }

                        ui.horizontal(|ui| {
                            ui.label("Seed:");
                            let mut seed_val = *seed as i64;
                            if ui
                                .add(egui::DragValue::new(&mut seed_val).range(0..=999_999))
                                .changed()
                            {
                                *seed = seed_val as u64;
                                *is_computed = false;
                            }
                        });

                        if ui.checkbox(align_to_normal, "Align to Normal").changed() {
                            *is_computed = false;
                        }
                    }
                    bif_core::PointSource::Grid => {
                        ui.horizontal(|ui| {
                            ui.label("Size X:");
                            if ui
                                .add(
                                    egui::DragValue::new(&mut grid_size[0])
                                        .speed(0.1)
                                        .range(0.0..=1000.0),
                                )
                                .changed()
                            {
                                *is_computed = false;
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label("Size Y:");
                            if ui
                                .add(
                                    egui::DragValue::new(&mut grid_size[1])
                                        .speed(0.1)
                                        .range(0.0..=1000.0),
                                )
                                .changed()
                            {
                                *is_computed = false;
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label("Size Z:");
                            if ui
                                .add(
                                    egui::DragValue::new(&mut grid_size[2])
                                        .speed(0.1)
                                        .range(0.0..=1000.0),
                                )
                                .changed()
                            {
                                *is_computed = false;
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label("Spacing:");
                            if ui
                                .add(
                                    egui::DragValue::new(grid_spacing)
                                        .speed(0.01)
                                        .range(0.01..=100.0),
                                )
                                .changed()
                            {
                                *is_computed = false;
                            }
                        });
                    }
                    bif_core::PointSource::Sphere => {
                        ui.horizontal(|ui| {
                            ui.label("Count:");
                            if ui
                                .add(egui::DragValue::new(count).range(1..=100_000))
                                .changed()
                            {
                                *is_computed = false;
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label("Radius:");
                            if ui
                                .add(
                                    egui::DragValue::new(sphere_radius)
                                        .speed(0.1)
                                        .range(0.01..=1000.0),
                                )
                                .changed()
                            {
                                *is_computed = false;
                            }
                        });
                        if ui.checkbox(sphere_on_surface, "Surface Only").changed() {
                            *is_computed = false;
                        }
                        ui.horizontal(|ui| {
                            ui.label("Seed:");
                            let mut seed_val = *seed as i64;
                            if ui
                                .add(egui::DragValue::new(&mut seed_val).range(0..=999_999))
                                .changed()
                            {
                                *seed = seed_val as u64;
                                *is_computed = false;
                            }
                        });
                    }
                }

                // Common: max point limit
                ui.horizontal(|ui| {
                    ui.label("Max Pts:");
                    if ui
                        .add(egui::DragValue::new(max_point_limit).range(1..=1_000_000))
                        .changed()
                    {
                        *is_computed = false;
                    }
                });

                // Relax section
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Relax Iters:");
                    if ui
                        .add(egui::DragValue::new(relax_iterations).range(0..=100))
                        .changed()
                    {
                        *is_computed = false;
                    }
                });
                if *relax_iterations > 0 {
                    ui.horizontal(|ui| {
                        ui.label("Scale Radii:");
                        if ui
                            .add(
                                egui::DragValue::new(scale_radii)
                                    .speed(0.01)
                                    .range(0.01..=10.0),
                            )
                            .changed()
                        {
                            *is_computed = false;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Max Radius:");
                        if ui
                            .add(
                                egui::DragValue::new(max_relax_radius)
                                    .speed(0.01)
                                    .range(0.01..=100.0),
                            )
                            .changed()
                        {
                            *is_computed = false;
                        }
                    });
                }

                // Per-point attributes
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Scale:");
                    let changed_min = ui
                        .add(
                            egui::DragValue::new(scale_min)
                                .speed(0.01)
                                .range(0.01..=10.0),
                        )
                        .changed();
                    ui.label("-");
                    let changed_max = ui
                        .add(
                            egui::DragValue::new(scale_max)
                                .speed(0.01)
                                .range(0.01..=10.0),
                        )
                        .changed();
                    if changed_min || changed_max {
                        *is_computed = false;
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("Rotation:");
                    if ui
                        .add(
                            egui::DragValue::new(rotation_range)
                                .speed(1.0)
                                .suffix("deg")
                                .range(0.0..=360.0),
                        )
                        .changed()
                    {
                        *is_computed = false;
                    }
                });

                // Compute / Regenerate button
                let emit_event = |events: &mut Vec<NodeGraphEvent>| {
                    events.push(NodeGraphEvent::ScatterPointsCompute {
                        node_id,
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
                };

                // Auto-compute: check if inputs are satisfied
                let inputs_satisfied = match source {
                    bif_core::PointSource::Surface => resolve_input_connection(inputs, 0).is_some(),
                    bif_core::PointSource::Grid | bif_core::PointSource::Sphere => true,
                };

                if !*is_computed && inputs_satisfied {
                    emit_event(&mut self.events);
                    *is_computed = true;
                }

                // Status display
                if *is_computed {
                    ui.colored_label(egui::Color32::GREEN, "Computed");
                } else if !inputs_satisfied {
                    ui.colored_label(egui::Color32::YELLOW, "Waiting for input");
                }

                // Point preview controls (cosmetic only, no recompute)
                ui.separator();
                let mut preview_changed = false;
                ui.horizontal(|ui| {
                    ui.label("Pt Size:");
                    if ui
                        .add(
                            egui::DragValue::new(point_size)
                                .speed(0.5)
                                .range(1.0..=20.0),
                        )
                        .changed()
                    {
                        preview_changed = true;
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Color:");
                    if ui
                        .color_edit_button_rgba_unmultiplied(point_color)
                        .changed()
                    {
                        preview_changed = true;
                    }
                });
                if preview_changed {
                    self.events.push(NodeGraphEvent::PointPreviewUpdate {
                        node_id,
                        point_size: *point_size,
                        point_color: *point_color,
                    });
                }
            }
            SceneNode::PointInstancer {
                instance_count,
                is_instanced,
                is_computing,
                compute_failed,
            } => {
                let points_node = resolve_input_connection(inputs, 0);
                let proto_node = resolve_input_connection(inputs, 1);
                let both_connected = points_node.is_some() && proto_node.is_some();

                // Auto-invalidate: inputs disconnected but still marked instanced
                if !both_connected && *is_instanced {
                    self.events
                        .push(NodeGraphEvent::InstancerInvalidate { node_id });
                    *is_instanced = false;
                    *is_computing = false;
                    *compute_failed = false;
                    *instance_count = 0;
                }

                // Auto-compute: both inputs connected, not yet instanced, not failed
                if both_connected && !*is_instanced && !*is_computing && !*compute_failed {
                    let points_source = points_node.unwrap();
                    let proto_source = proto_node.unwrap();
                    self.events.push(NodeGraphEvent::PointInstancerCompute {
                        node_id,
                        points_source_node: points_source,
                        proto_source_node: proto_source,
                    });
                    *is_computing = true;
                }

                // Status display
                if *is_instanced {
                    ui.colored_label(
                        egui::Color32::GREEN,
                        format!("{} instances", instance_count),
                    );
                } else if *is_computing {
                    ui.colored_label(egui::Color32::YELLOW, "Computing...");
                } else if *compute_failed {
                    ui.colored_label(egui::Color32::RED, "Compute failed");
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
                    ui.colored_label(egui::Color32::YELLOW, format!("Need: {}", need));
                }
            }
            SceneNode::UsdExport {
                output_path,
                as_sublayer,
                export_root,
                is_exported,
                last_result,
            } => {
                ui.horizontal(|ui| {
                    ui.label("Path:");
                    ui.text_edit_singleline(output_path);
                });

                ui.horizontal(|ui| {
                    if ui.button("Browse...").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("USD Files", &["usda", "usdc"])
                            .set_file_name("export.usda")
                            .save_file()
                        {
                            *output_path = path.display().to_string();
                            *is_exported = false;
                            *last_result = None;
                        }
                    }
                });

                ui.checkbox(as_sublayer, "As Sublayer");

                ui.horizontal(|ui| {
                    ui.label("Root:");
                    ui.text_edit_singleline(export_root);
                });

                if !output_path.is_empty() && ui.button("Export").clicked() {
                    self.events.push(NodeGraphEvent::ExportUsd {
                        node_id,
                        output_path: output_path.clone(),
                        as_sublayer: *as_sublayer,
                        export_root: export_root.clone(),
                    });
                }

                if *is_exported {
                    if let Some(ref result) = last_result {
                        ui.colored_label(egui::Color32::GREEN, result.as_str());
                    }
                } else if let Some(ref result) = last_result {
                    // Error case
                    ui.colored_label(egui::Color32::RED, result.as_str());
                }
            }
            SceneNode::Xform {
                translate,
                rotate,
                scale,
                prim_filter,
            } => {
                let axis_colors = [
                    egui::Color32::from_rgb(220, 80, 80),  // X = red
                    egui::Color32::from_rgb(80, 200, 80),  // Y = green
                    egui::Color32::from_rgb(80, 120, 220), // Z = blue
                ];
                let axis_labels = ["X", "Y", "Z"];

                let mut changed = false;

                ui.label("Translate");
                ui.horizontal(|ui| {
                    for i in 0..3 {
                        ui.colored_label(axis_colors[i], axis_labels[i]);
                        if ui
                            .add(egui::DragValue::new(&mut translate[i]).speed(0.1))
                            .changed()
                        {
                            changed = true;
                        }
                    }
                });

                ui.label("Rotate");
                ui.horizontal(|ui| {
                    for i in 0..3 {
                        ui.colored_label(axis_colors[i], axis_labels[i]);
                        if ui
                            .add(egui::DragValue::new(&mut rotate[i]).speed(1.0).suffix("°"))
                            .changed()
                        {
                            changed = true;
                        }
                    }
                });

                ui.label("Scale");
                ui.horizontal(|ui| {
                    for i in 0..3 {
                        ui.colored_label(axis_colors[i], axis_labels[i]);
                        if ui
                            .add(
                                egui::DragValue::new(&mut scale[i])
                                    .speed(0.01)
                                    .range(0.001..=1000.0),
                            )
                            .changed()
                        {
                            changed = true;
                        }
                    }
                });

                // Prim filter (placeholder, non-functional V1)
                ui.horizontal(|ui| {
                    ui.label("Filter:");
                    ui.add_enabled(
                        false,
                        egui::TextEdit::singleline(prim_filter)
                            .hint_text("all prims (future)")
                            .desired_width(100.0),
                    );
                });

                if changed {
                    self.events.push(NodeGraphEvent::XformChanged { node_id });
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
                node_id: to.id.node,
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
        );
        if is_scene_output {
            let label = if self.display_node == Some(node) {
                "Clear Display"
            } else {
                "Set Display"
            };
            if ui.button(label).clicked() {
                self.events.push(NodeGraphEvent::SetDisplayNode(node));
                ui.close_menu();
            }
        }
        if ui.button("Delete").clicked() {
            self.events.push(NodeGraphEvent::DeleteNode(node));
            ui.close_menu();
        }
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
    /// Display flag: which node feeds viewport/export (like Houdini's blue flag).
    /// When set, only this node and its upstream deps are "active".
    pub display_node: Option<NodeId>,
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
            display_node: None,
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
            display_node: None,
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

    /// Add a Scatter Points node at the given position.
    pub fn add_scatter_points(&mut self, pos: egui::Pos2) -> NodeId {
        self.snarl.insert_node(pos, SceneNode::scatter_points())
    }

    /// Add a Point Instancer node at the given position.
    pub fn add_point_instancer(&mut self, pos: egui::Pos2) -> NodeId {
        self.snarl.insert_node(pos, SceneNode::point_instancer())
    }

    /// Add a Primitive node at the given position
    pub fn add_primitive(&mut self, kind: bif_core::PrimitiveKind, pos: egui::Pos2) -> NodeId {
        self.snarl.insert_node(pos, SceneNode::primitive(kind))
    }

    /// Add an Xform node at the given position
    pub fn add_xform(&mut self, pos: egui::Pos2) -> NodeId {
        self.snarl.insert_node(pos, SceneNode::xform())
    }

    /// Add a USD Export node at the given position
    pub fn add_usd_export(&mut self, pos: egui::Pos2) -> NodeId {
        self.snarl.insert_node(pos, SceneNode::usd_export())
    }

    /// Delete the selected node.
    ///
    /// Returns the NodeId so the caller can emit a `DeleteNode` event
    /// for scene cleanup. Does NOT remove from snarl — the event loop does that.
    pub fn delete_selected(&mut self) -> Option<NodeId> {
        self.selected_node.take()
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
    pub fn mark_hdri_loaded(
        &mut self,
        path: &str,
        load_secs: Option<f64>,
        compute_secs: Option<f64>,
    ) {
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
    let mut viewer = SceneNodeViewer::new(state.display_node);

    // Handle keyboard input for delete
    // TODO: macOS has no Delete key — add Backspace conditionally via cfg!(target_os = "macos")
    if ui.input(|i| i.key_pressed(egui::Key::Delete)) {
        if let Some(node_id) = state.delete_selected() {
            viewer.events.push(NodeGraphEvent::DeleteNode(node_id));
        }
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
        if ui.button("+ Cube").clicked() {
            state.add_primitive(bif_core::PrimitiveKind::Cube, egui::pos2(50.0, 300.0));
        }
        if ui.button("+ Sphere").clicked() {
            state.add_primitive(bif_core::PrimitiveKind::Sphere, egui::pos2(50.0, 400.0));
        }
        if ui.button("+ Camera").clicked() {
            state.add_primitive(bif_core::PrimitiveKind::Camera, egui::pos2(50.0, 500.0));
        }
        if ui.button("+ Scatter Points").clicked() {
            state.add_scatter_points(egui::pos2(200.0, 300.0));
        }
        if ui.button("+ Instancer").clicked() {
            state.add_point_instancer(egui::pos2(400.0, 300.0));
        }
        if ui.button("+ Xform").clicked() {
            state.add_xform(egui::pos2(250.0, 200.0));
        }
        if ui.button("+ USD Export").clicked() {
            state.add_usd_export(egui::pos2(600.0, 100.0));
        }
        ui.separator();
        if ui.button("Del Selected").clicked() {
            if let Some(node_id) = state.delete_selected() {
                viewer.events.push(NodeGraphEvent::DeleteNode(node_id));
            }
        }
    });

    ui.separator();

    // Render the snarl node graph (guard against degenerate panel size)
    let avail = ui.available_size();
    if avail.x > 1.0 && avail.y > 1.0 {
        state.snarl.show(
            &mut viewer,
            &state.style,
            egui::Id::new("scene_node_graph"),
            ui,
        );
    }

    // Process selection and deletion events before returning
    let mut events_out = Vec::new();
    for event in viewer.events {
        match &event {
            NodeGraphEvent::SetDisplayNode(_) => {
                // Toggle handled in render.rs (needs reload_working_scene)
            }
            NodeGraphEvent::SelectNode(id) => {
                state.selected_node = Some(*id);
            }
            NodeGraphEvent::DeleteNode(id) => {
                let id = *id;
                if state.selected_node == Some(id) {
                    state.selected_node = None;
                }
                if state.display_node == Some(id) {
                    state.display_node = None;
                }

                // Emit InstancerInvalidate for any downstream PointInstancer before removing.
                let output_count = state.snarl[id].output_count();
                for out_idx in 0..output_count {
                    let out_pin = state.snarl.out_pin(OutPinId {
                        node: id,
                        output: out_idx,
                    });
                    for remote in &out_pin.remotes {
                        let downstream = remote.node;
                        if matches!(
                            state.snarl[downstream],
                            SceneNode::PointInstancer {
                                is_instanced: true,
                                ..
                            }
                        ) {
                            events_out.push(NodeGraphEvent::InstancerInvalidate {
                                node_id: downstream,
                            });
                        }
                    }
                }
                // Recursively dirty all downstream nodes
                propagate_dirty(id, &mut state.snarl);

                state.snarl.remove_node(id);
                // Pass through so renderer can clean up scene data
                events_out.push(event);
                continue;
            }
            _ => {}
        }
        events_out.push(event);
    }

    events_out
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

    #[test]
    fn test_point_instancer_node() {
        let node = SceneNode::point_instancer();
        assert_eq!(node.name(), "Point Instancer");
        assert_eq!(node.input_count(), 2);
        assert_eq!(node.output_count(), 1);
        assert_eq!(node.input_pin(0), Some(("points", PinType::Scene)));
        assert_eq!(node.input_pin(1), Some(("proto", PinType::Scene)));
        assert_eq!(node.input_pin(2), None);
        assert_eq!(node.output_pin(0), Some(("scene", PinType::Scene)));
    }

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

        // Connect scatter → instancer input 0 (points)
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
        // Connect cube → instancer input 1 (proto)
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
