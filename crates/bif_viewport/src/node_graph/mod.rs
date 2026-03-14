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

mod viewer;

pub mod ops;
pub(crate) use ops::{collect_upstream_nodes, propagate_dirty};

use std::sync::atomic::{AtomicU32, Ordering};

use egui_snarl::{ui::SnarlStyle, InPinId, NodeId, OutPinId, Snarl};

use viewer::SceneNodeViewer;

/// Global counters for auto-incrementing prim paths per primitive kind.
static CUBE_COUNTER: AtomicU32 = AtomicU32::new(1);
static SPHERE_COUNTER: AtomicU32 = AtomicU32::new(1);
static CAMERA_COUNTER: AtomicU32 = AtomicU32::new(1);
static INSTANCER_COUNTER: AtomicU32 = AtomicU32::new(1);

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
        /// USD prim path for export
        prim_path: String,
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
        /// USD prim path for export
        prim_path: String,
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
    /// USD Prim — defines organizational structure in the scene graph
    UsdPrim {
        /// USD prim path (e.g., "/shot", "/World")
        prim_path: String,
        /// Prim type (Scope, Xform, or None)
        prim_type: bif_core::usd::UsdPrimType,
        /// Model kind (Component, Group, Assembly, etc.)
        kind: bif_core::usd::UsdKind,
        /// Specifier (Define or Over)
        specifier: bif_core::usd::UsdSpecifier,
    },
    /// Graft Branches — merge multiple branches under a parent prim
    GraftBranches {
        /// Destination prim path (parent for all grafted branches)
        destination_path: String,
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
        let (size, prim_path) = match kind {
            bif_core::PrimitiveKind::Cube => {
                let n = CUBE_COUNTER.fetch_add(1, Ordering::Relaxed);
                (1.0, format!("/World/Cube{}", n))
            }
            bif_core::PrimitiveKind::Sphere => {
                let n = SPHERE_COUNTER.fetch_add(1, Ordering::Relaxed);
                (0.5, format!("/World/Sphere{}", n))
            }
            bif_core::PrimitiveKind::Camera => {
                let n = CAMERA_COUNTER.fetch_add(1, Ordering::Relaxed);
                (1.0, format!("/World/Camera{}", n))
            }
        };
        Self::Primitive {
            kind,
            size,
            is_created: false,
            prim_path,
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
        let n = INSTANCER_COUNTER.fetch_add(1, Ordering::Relaxed);
        Self::PointInstancer {
            instance_count: 0,
            is_instanced: false,
            is_computing: false,
            compute_failed: false,
            prim_path: format!("/World/instancer{}", n),
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

    /// Create a new USD Prim node
    pub fn usd_prim() -> Self {
        Self::UsdPrim {
            prim_path: "/root".to_string(),
            prim_type: bif_core::usd::UsdPrimType::Scope,
            kind: bif_core::usd::UsdKind::None,
            specifier: bif_core::usd::UsdSpecifier::Define,
        }
    }

    /// Create a new Graft Branches node
    pub fn graft_branches() -> Self {
        Self::GraftBranches {
            destination_path: "/shot".to_string(),
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
            SceneNode::UsdPrim { .. } => "USD Prim",
            SceneNode::GraftBranches { .. } => "Graft Branches",
            SceneNode::HdriEnvironment { .. } => "HDRI Environment",
        }
    }

    /// Get input pin count
    pub fn input_count(&self) -> usize {
        match self {
            SceneNode::UsdRead { .. } => 0,
            SceneNode::IvarRender { .. } => 2, // scene + environment
            SceneNode::Primitive { .. } => 1,  // scene pass-through input
            SceneNode::ScatterPoints { source, .. } => match source {
                bif_core::PointSource::Surface => 1,
                bif_core::PointSource::Grid | bif_core::PointSource::Sphere => 0,
            },
            SceneNode::PointInstancer { .. } => 2, // points + prototype
            SceneNode::UsdExport { .. } => 1,      // scene input
            SceneNode::Xform { .. } => 1,          // scene input
            SceneNode::UsdPrim { .. } => 1,        // pass-through scene input
            SceneNode::GraftBranches { .. } => 4,  // up to 4 branches
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
            SceneNode::UsdPrim { .. } => 1,
            SceneNode::GraftBranches { .. } => 1,
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
            SceneNode::Primitive { .. } => match index {
                0 => Some(("scene", PinType::Scene)),
                _ => None,
            },
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
            SceneNode::UsdPrim { .. } => match index {
                0 => Some(("scene", PinType::Scene)),
                _ => None,
            },
            SceneNode::GraftBranches { .. } => match index {
                0 => Some(("branch 1", PinType::Scene)),
                1 => Some(("branch 2", PinType::Scene)),
                2 => Some(("branch 3", PinType::Scene)),
                3 => Some(("branch 4", PinType::Scene)),
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
            SceneNode::UsdPrim { .. } => match index {
                0 => Some(("scene", PinType::Scene)),
                _ => None,
            },
            SceneNode::GraftBranches { .. } => match index {
                0 => Some(("scene", PinType::Scene)),
                _ => None,
            },
            SceneNode::HdriEnvironment { .. } => match index {
                0 => Some(("env", PinType::Environment)),
                _ => None,
            },
        }
    }

    /// Path label shown on the node (like Houdini's green prim path text).
    pub fn prim_path_label(&self) -> Option<&str> {
        match self {
            SceneNode::Primitive { prim_path, .. } => Some(prim_path.as_str()),
            SceneNode::PointInstancer { prim_path, .. } => Some(prim_path.as_str()),
            SceneNode::UsdPrim { prim_path, .. } => Some(prim_path.as_str()),
            SceneNode::GraftBranches { destination_path } => Some(destination_path.as_str()),
            SceneNode::UsdRead { file_path, .. } if !file_path.is_empty() => {
                // Show just the filename
                file_path
                    .rsplit(['/', '\\'])
                    .next()
                    .or(Some(file_path.as_str()))
            }
            SceneNode::UsdExport { output_path, .. } if !output_path.is_empty() => {
                file_name_from_path(output_path)
            }
            _ => None,
        }
    }
}

/// Extract filename from a path string.
fn file_name_from_path(path: &str) -> Option<&str> {
    path.rsplit(['/', '\\']).next()
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

    /// Add a USD Prim node at the given position
    pub fn add_usd_prim(&mut self, pos: egui::Pos2) -> NodeId {
        self.snarl.insert_node(pos, SceneNode::usd_prim())
    }

    /// Add a Graft Branches node at the given position
    pub fn add_graft_branches(&mut self, pos: egui::Pos2) -> NodeId {
        self.snarl.insert_node(pos, SceneNode::graft_branches())
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
        if ui.button("+ USD Prim").clicked() {
            state.add_usd_prim(egui::pos2(150.0, 100.0));
        }
        if ui.button("+ Graft").clicked() {
            state.add_graft_branches(egui::pos2(350.0, 200.0));
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
}
