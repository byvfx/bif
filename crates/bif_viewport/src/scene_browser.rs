//! USD Scene Browser - Houdini-style table view for USD scene graph.
//!
//! Provides an interactive table widget for browsing USD prim hierarchy,
//! inspired by Houdini's Scene Graph Tree.
//!
//! # Features
//! - Table layout with columns: Path, Type, Children, Kind
//! - Expandable tree with lazy child loading
//! - Type icons for prim types (Mesh, Xform, Instancer, Scope, etc.)
//! - Search/filter functionality
//! - Selection synced with viewport
//!
//! # TODOs
//! - [ ] Visibility toggle columns (P, L, etc.)
//! - [ ] Kind column from USD metadata
//! - [ ] Keyboard navigation with arrow keys

use std::collections::{HashMap, HashSet};

use crate::node_graph::GraphNodeId;
use crate::theme;

/// Which view mode the scene browser is in.
#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum SceneBrowserViewMode {
    /// Show the full scene graph.
    #[default]
    FullScene,
    /// Show only prims from the selected node and its upstream chain.
    NodeContribution(GraphNodeId),
}

/// State for the scene browser UI.
#[derive(Default)]
pub struct SceneBrowserState {
    /// Currently selected prim path (if any)
    pub selected_path: Option<String>,

    /// Set of expanded prim paths
    pub expanded_paths: HashSet<String>,

    /// Search/filter text
    pub search_filter: String,

    /// Whether to show inactive prims
    pub show_inactive: bool,

    /// Scene browser view mode (full scene or node contribution).
    pub view_mode: SceneBrowserViewMode,
}

impl SceneBrowserState {
    /// Create a new scene browser state.
    pub fn new() -> Self {
        Self {
            selected_path: None,
            expanded_paths: HashSet::new(),
            search_filter: String::new(),
            show_inactive: true,
            view_mode: SceneBrowserViewMode::FullScene,
        }
    }

    /// Check if a prim path is expanded.
    pub fn is_expanded(&self, path: &str) -> bool {
        self.expanded_paths.contains(path)
    }

    /// Toggle expansion state of a prim.
    pub fn toggle_expanded(&mut self, path: &str) {
        if self.expanded_paths.contains(path) {
            self.expanded_paths.remove(path);
        } else {
            self.expanded_paths.insert(path.to_string());
        }
    }

    /// Expand a prim path.
    pub fn expand(&mut self, path: &str) {
        self.expanded_paths.insert(path.to_string());
    }

    /// Collapse a prim path.
    pub fn collapse(&mut self, path: &str) {
        self.expanded_paths.remove(path);
    }

    /// Expand all ancestors of a path (for revealing a deep prim).
    pub fn expand_to_path(&mut self, path: &str) {
        // Split path and expand each ancestor
        // e.g., "/World/Geo/Mesh" -> expand "/World", "/World/Geo"
        let mut current = String::new();
        for segment in path.split('/').filter(|s| !s.is_empty()) {
            current.push('/');
            current.push_str(segment);
            if current != path {
                self.expanded_paths.insert(current.clone());
            }
        }
    }

    /// Select a prim by path.
    pub fn select(&mut self, path: &str) {
        self.selected_path = Some(path.to_string());
    }

    /// Clear selection.
    pub fn clear_selection(&mut self) {
        self.selected_path = None;
    }

    /// Check if a prim matches the current filter.
    pub fn matches_filter(&self, path: &str, type_name: &str) -> bool {
        if self.search_filter.is_empty() {
            return true;
        }

        let filter_lower = self.search_filter.to_lowercase();
        path.to_lowercase().contains(&filter_lower)
            || type_name.to_lowercase().contains(&filter_lower)
    }
}

/// Get the icon and color for a USD prim type.
pub fn prim_type_icon(type_name: &str) -> (&'static str, egui::Color32) {
    match type_name {
        "Mesh" => ("◆", theme::PIN_SCENE),
        "Xform" => ("✦", theme::STATUS_WARNING),
        "PointInstancer" => ("⊕", theme::ACCENT_PRIMARY),
        "Scope" => ("▸", theme::TEXT_SECONDARY),
        "Camera" => ("◎", theme::STATUS_INFO),
        "Light" | "DistantLight" | "DomeLight" | "SphereLight" | "RectLight" => {
            ("✧", theme::KEYFRAME_FILL)
        }
        "Material" => ("●", theme::STATUS_OK),
        "Shader" => ("◇", theme::TEXT_SECONDARY),
        "Skeleton" | "SkelRoot" => ("⊞", theme::TEXT_SECONDARY),
        "" => ("◇", theme::TEXT_SECONDARY),
        _ => ("○", theme::TEXT_SECONDARY),
    }
}

/// Prim info for display in the scene browser.
/// This is a simplified version that can be created from either
/// the C++ bridge UsdPrimInfo or the Rust parser data.
#[derive(Clone, Debug)]
pub struct PrimDisplayInfo {
    /// Full prim path
    pub path: String,

    /// Just the prim name (last segment of path)
    pub name: String,

    /// Type name (e.g., "Mesh", "Xform")
    pub type_name: String,

    /// Whether prim is active
    pub is_active: bool,

    /// Whether prim has children
    pub has_children: bool,

    /// Child count
    pub child_count: usize,

    /// Kind (from USD metadata: component, assembly, group, etc.)
    pub kind: String,

    /// Visibility state (computed from inherited visibility)
    pub is_visible: bool,

    /// Which graph node produced this prim (for highlighting).
    pub source_node: Option<GraphNodeId>,
}

impl PrimDisplayInfo {
    /// Create from path and type info.
    pub fn new(
        path: String,
        type_name: String,
        is_active: bool,
        has_children: bool,
        child_count: usize,
    ) -> Self {
        let name = path.rsplit('/').next().unwrap_or(&path).to_string();

        Self {
            path,
            name,
            type_name,
            is_active,
            has_children,
            child_count,
            kind: String::new(), // Default to empty
            is_visible: true,    // Default to visible
            source_node: None,   // Default to unknown
        }
    }

    /// Create with all fields including kind and visibility.
    pub fn with_kind(
        path: String,
        type_name: String,
        is_active: bool,
        has_children: bool,
        child_count: usize,
        kind: String,
        is_visible: bool,
    ) -> Self {
        let name = path.rsplit('/').next().unwrap_or(&path).to_string();

        Self {
            path,
            name,
            type_name,
            is_active,
            has_children,
            child_count,
            kind,
            is_visible,
            source_node: None,
        }
    }

    /// Get the icon character and color for this prim's type.
    pub fn icon(&self) -> (&'static str, egui::Color32) {
        prim_type_icon(&self.type_name)
    }
}

/// Trait for providing prim hierarchy data to the scene browser.
///
/// This abstraction allows the scene browser to work with either:
/// - The C++ USD bridge (for USDC files)
/// - The pure Rust USDA parser (fallback)
pub trait PrimDataProvider {
    /// Get root prim paths.
    fn root_paths(&self) -> Vec<String>;

    /// Get prim info by path.
    fn get_prim_info(&self, path: &str) -> Option<PrimDisplayInfo>;

    /// Get child paths for a parent prim.
    fn get_children(&self, parent_path: &str) -> Vec<String>;
}

/// Render the scene browser UI in Houdini-style table layout.
///
/// Returns `Some(path)` if selection changed, `None` otherwise.
///
/// `highlight_node`: when set, prims from this node get a tinted background.
pub fn render_scene_browser(
    ui: &mut egui::Ui,
    state: &mut SceneBrowserState,
    provider: &dyn PrimDataProvider,
    highlight_node: Option<GraphNodeId>,
) -> Option<String> {
    let mut selection_changed: Option<String> = None;

    // Filter row at top (like Houdini)
    ui.horizontal(|ui| {
        ui.label("Filter:");
        let response = ui.text_edit_singleline(&mut state.search_filter);
        if response.changed() {
            // Could auto-expand matching paths here
        }
        if ui.button("✕").clicked() {
            state.search_filter.clear();
        }
        ui.separator();
        ui.checkbox(&mut state.show_inactive, "Inactive");
    });

    ui.separator();

    // Get root paths
    let root_paths = provider.root_paths();

    if root_paths.is_empty() {
        ui.vertical_centered(|ui| {
            ui.add_space(20.0);
            ui.label("No scene content");
            ui.label("Use the node graph to load a .usda/.usdc or add primitives");
        });
        return None;
    }

    // Table header
    let available_width = ui.available_width();
    let path_width = (available_width * 0.5).max(150.0);
    let type_width = 80.0;
    let children_width = 60.0;
    let kind_width = 80.0;
    let viz_width = 30.0;

    // Header row with column labels (Houdini style)
    ui.horizontal(|ui| {
        ui.set_min_height(20.0);
        ui.style_mut().visuals.widgets.inactive.bg_fill = theme::BG_SURFACE;

        // Scene Graph Path column header
        ui.allocate_ui_with_layout(
            egui::vec2(path_width, 20.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.label(egui::RichText::new("Scene Graph Path").strong().small());
            },
        );

        ui.separator();

        // Primitive Type column header
        ui.allocate_ui_with_layout(
            egui::vec2(type_width, 20.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.label(egui::RichText::new("Prim Type").strong().small());
            },
        );

        ui.separator();

        // Children column header
        ui.allocate_ui_with_layout(
            egui::vec2(children_width, 20.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.label(egui::RichText::new("Children").strong().small());
            },
        );

        ui.separator();

        // Kind column header
        ui.allocate_ui_with_layout(
            egui::vec2(kind_width, 20.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.label(egui::RichText::new("Kind").strong().small());
            },
        );

        ui.separator();

        // Visibility column header (like Houdini's eye icon)
        ui.allocate_ui_with_layout(
            egui::vec2(viz_width, 20.0),
            egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
            |ui| {
                ui.label(egui::RichText::new("👁").small());
            },
        );
    });

    ui.separator();

    // Scrollable table body
    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            // Collect column widths for consistent rendering
            let col_widths = ColumnWidths {
                path: path_width,
                prim_type: type_width,
                children: children_width,
                kind: kind_width,
                visibility: viz_width,
            };

            for root_path in root_paths {
                if let Some(new_selection) = render_prim_row(
                    ui,
                    state,
                    provider,
                    &root_path,
                    0,
                    &col_widths,
                    highlight_node,
                ) {
                    selection_changed = Some(new_selection);
                }
            }
        });

    selection_changed
}

/// Column widths for the table layout
struct ColumnWidths {
    path: f32,
    prim_type: f32,
    children: f32,
    kind: f32,
    visibility: f32,
}

/// Recursively render a prim row and its children in table format.
#[allow(clippy::only_used_in_recursion)] // highlight_node kept for future use (node-contribution tinting)
fn render_prim_row(
    ui: &mut egui::Ui,
    state: &mut SceneBrowserState,
    provider: &dyn PrimDataProvider,
    path: &str,
    depth: usize,
    col_widths: &ColumnWidths,
    highlight_node: Option<GraphNodeId>,
) -> Option<String> {
    let info = provider.get_prim_info(path)?;

    // Filter check
    if !state.show_inactive && !info.is_active {
        return None;
    }

    // For filtering, we need to check if this prim or any descendant matches
    let matches = state.matches_filter(&info.path, &info.type_name);

    // If using filter and this doesn't match, still render if has children that might match
    if !matches && !state.search_filter.is_empty() && !info.has_children {
        return None;
    }

    let mut selection_changed: Option<String> = None;
    let is_selected = state.selected_path.as_ref() == Some(&info.path);
    let is_expanded = state.is_expanded(&info.path);

    // Only the selected row gets a background — no node-source tinting.
    let row_bg = if is_selected {
        // Dim accent blue — readable white text on dark BG_PANEL.
        egui::Color32::from_rgba_unmultiplied(74, 144, 217, 75)
    } else {
        egui::Color32::TRANSPARENT
    };

    // Calculate indent for tree hierarchy
    let indent = depth as f32 * 16.0;

    // Render the row
    let row_response = ui.horizontal(|ui| {
        // Apply row background
        let row_rect = ui.available_rect_before_wrap();
        ui.painter().rect_filled(
            egui::Rect::from_min_size(row_rect.min, egui::vec2(ui.available_width(), 20.0)),
            0.0,
            row_bg,
        );

        // Path column with tree controls
        ui.allocate_ui_with_layout(
            egui::vec2(col_widths.path, 20.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.add_space(indent);

                // Expand/collapse button
                if info.has_children {
                    let expand_text = if is_expanded { "▼" } else { "▶" };
                    if ui.small_button(expand_text).clicked() {
                        state.toggle_expanded(&info.path);
                    }
                } else {
                    ui.add_space(18.0);
                }

                // Type icon (colored Unicode shape)
                let (icon_char, icon_color) = info.icon();
                ui.colored_label(icon_color, icon_char);

                // Prim name
                let name_text = if info.is_active {
                    egui::RichText::new(&info.name)
                } else {
                    egui::RichText::new(&info.name).color(theme::TEXT_DISABLED)
                };

                // Pass false — row bg is painted manually above; passing is_selected causes
                // egui to double-paint its own selection fill on top of our custom color.
                let name_response = ui.selectable_label(false, name_text);
                if name_response.clicked() {
                    state.select(&info.path);
                    selection_changed = Some(info.path.clone());
                }
                name_response.on_hover_text(&info.path);
            },
        );

        ui.separator();

        // Primitive Type column
        ui.allocate_ui_with_layout(
            egui::vec2(col_widths.prim_type, 20.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                let type_text = if info.type_name.is_empty() {
                    egui::RichText::new("-").color(theme::TEXT_DISABLED)
                } else {
                    egui::RichText::new(&info.type_name).small()
                };
                ui.label(type_text);
            },
        );

        ui.separator();

        // Children column
        ui.allocate_ui_with_layout(
            egui::vec2(col_widths.children, 20.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                if info.child_count > 0 {
                    ui.label(egui::RichText::new(format!("{}", info.child_count)).small());
                } else {
                    ui.label(egui::RichText::new("-").color(theme::TEXT_DISABLED).small());
                }
            },
        );

        ui.separator();

        // Kind column
        ui.allocate_ui_with_layout(
            egui::vec2(col_widths.kind, 20.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                if info.kind.is_empty() {
                    ui.label(egui::RichText::new("-").color(theme::TEXT_DISABLED).small());
                } else {
                    ui.label(egui::RichText::new(&info.kind).small());
                }
            },
        );

        ui.separator();

        // Visibility column (read-only for now)
        ui.allocate_ui_with_layout(
            egui::vec2(col_widths.visibility, 20.0),
            egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
            |ui| {
                let viz_icon = if info.is_visible { "👁" } else { "🚫" };
                let viz_text = egui::RichText::new(viz_icon).small();
                ui.label(viz_text).on_hover_text(if info.is_visible {
                    "Visible"
                } else {
                    "Hidden"
                });
            },
        );
    });

    // Handle row click for selection
    if row_response.response.clicked() && selection_changed.is_none() {
        state.select(&info.path);
        selection_changed = Some(info.path.clone());
    }

    // Render children if expanded
    if is_expanded && info.has_children {
        let children = provider.get_children(&info.path);
        for child_path in children {
            if let Some(new_selection) = render_prim_row(
                ui,
                state,
                provider,
                &child_path,
                depth + 1,
                col_widths,
                highlight_node,
            ) {
                selection_changed = Some(new_selection);
            }
        }
    }

    selection_changed
}

/// Empty provider for when no USD stage is loaded.
pub struct EmptyPrimProvider;

impl PrimDataProvider for EmptyPrimProvider {
    fn root_paths(&self) -> Vec<String> {
        Vec::new()
    }

    fn get_prim_info(&self, _path: &str) -> Option<PrimDisplayInfo> {
        None
    }

    fn get_children(&self, _parent_path: &str) -> Vec<String> {
        Vec::new()
    }
}

/// Provider that wraps a UsdStage from the C++ bridge.
use bif_core::usd::UsdStage;

impl PrimDataProvider for UsdStage {
    fn root_paths(&self) -> Vec<String> {
        self.root_prim_paths().unwrap_or_default()
    }

    fn get_prim_info(&self, path: &str) -> Option<PrimDisplayInfo> {
        self.get_prim_info_by_path(path).ok().map(|info| {
            let mut display = PrimDisplayInfo::new(
                info.path,
                info.type_name,
                info.is_active,
                info.has_children,
                info.child_count,
            );
            display.is_visible = info.visible;
            display
        })
    }

    fn get_children(&self, parent_path: &str) -> Vec<String> {
        self.child_prim_paths(parent_path).unwrap_or_default()
    }
}

/// Type-specific data for a procedural prim.
pub enum ProceduralPrimKind {
    /// Mesh prototype with geometry stats.
    Mesh {
        vertex_count: usize,
        triangle_count: usize,
    },
    /// PointInstancer with scatter stats.
    PointInstancer {
        point_count: usize,
        prototype_refs: Vec<String>,
    },
    /// Intermediate scope (auto-generated for path hierarchy).
    Scope,
}

impl ProceduralPrimKind {
    /// USD type name string for this kind.
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Mesh { .. } => "Mesh",
            Self::PointInstancer { .. } => "PointInstancer",
            Self::Scope => "Scope",
        }
    }
}

/// Procedural prim entry from working_scene.
pub struct ProceduralPrim {
    /// Full prim path (e.g., "/World/Cube1").
    pub path: String,
    /// Type-specific data.
    pub kind: ProceduralPrimKind,
    /// Which graph node produced this prim (None for auto-generated Scopes).
    pub source_node: Option<GraphNodeId>,
}

/// Cached scene graph data built from working_scene.
///
/// Rebuilt only when the scene changes (dirty flag), not every frame.
#[derive(Default)]
pub struct CachedSceneGraph {
    /// Procedural prims indexed by path.
    pub procedural_prims: HashMap<String, ProceduralPrim>,
    /// Pre-computed parent -> sorted children index for O(1) lookups.
    pub children_index: HashMap<String, Vec<String>>,
}

impl CachedSceneGraph {
    /// Count procedural prims per source node (excludes auto-generated Scopes).
    pub fn prim_count_by_node(&self) -> HashMap<GraphNodeId, usize> {
        let mut counts = HashMap::new();
        for prim in self.procedural_prims.values() {
            if let Some(node_id) = prim.source_node {
                *counts.entry(node_id).or_insert(0) += 1;
            }
        }
        counts
    }
}

/// Build a cached scene graph from the working scene.
///
/// Iterates prototypes and point clouds once, builds the prim HashMap
/// and pre-computes the parent->children index. Tags each prim with
/// its source graph node via reverse lookups on the node maps.
pub fn build_scene_graph_cache(
    scene: &bif_core::Scene,
    node_proto_map: &HashMap<GraphNodeId, Vec<usize>>,
    node_cloud_map: &HashMap<GraphNodeId, usize>,
) -> CachedSceneGraph {
    // Build reverse maps: proto_index -> node, cloud_id -> node
    let mut proto_to_node: HashMap<usize, GraphNodeId> = HashMap::new();
    for (&node_id, proto_ids) in node_proto_map {
        for &pid in proto_ids {
            proto_to_node.insert(pid, node_id);
        }
    }
    let mut cloud_to_node: HashMap<usize, GraphNodeId> = HashMap::new();
    for (&node_id, &cloud_id) in node_cloud_map {
        cloud_to_node.insert(cloud_id, node_id);
    }

    let mut procedural_prims = HashMap::new();

    // Add ALL prototypes as Mesh prims
    for (proto_idx, proto) in scene.prototypes.iter().enumerate() {
        let path = if proto.name.starts_with('/') {
            proto.name.to_string()
        } else {
            format!("/{}", proto.name)
        };
        let mesh = &proto.mesh;
        procedural_prims.insert(
            path.clone(),
            ProceduralPrim {
                path,
                kind: ProceduralPrimKind::Mesh {
                    vertex_count: mesh.positions.len(),
                    triangle_count: mesh.indices.len() / 3,
                },
                source_node: proto_to_node.get(&proto_idx).copied(),
            },
        );
    }

    // Add point clouds as PointInstancer prims
    for (cloud_idx, cloud) in scene.point_clouds.iter().enumerate() {
        if cloud.positions.is_empty() || cloud.name.is_empty() {
            continue;
        }
        let path = if cloud.name.starts_with('/') {
            cloud.name.clone()
        } else {
            format!("/{}", cloud.name)
        };
        let proto_refs: Vec<String> = cloud
            .prototype_ids
            .iter()
            .filter_map(|&pid| {
                scene.prototypes.get(pid).map(|p| {
                    if p.name.starts_with('/') {
                        p.name.to_string()
                    } else {
                        format!("/{}", p.name)
                    }
                })
            })
            .collect();
        procedural_prims.insert(
            path.clone(),
            ProceduralPrim {
                path,
                kind: ProceduralPrimKind::PointInstancer {
                    point_count: cloud.positions.len(),
                    prototype_refs: proto_refs,
                },
                source_node: cloud_to_node.get(&cloud_idx).copied(),
            },
        );
    }

    // Auto-generate intermediate Scope prims for missing path segments
    let all_paths: Vec<String> = procedural_prims.keys().cloned().collect();
    for path in &all_paths {
        let mut current = String::new();
        for segment in path.split('/').filter(|s| !s.is_empty()) {
            current.push('/');
            current.push_str(segment);
            if current != *path && !procedural_prims.contains_key(&current) {
                procedural_prims.insert(
                    current.clone(),
                    ProceduralPrim {
                        path: current.clone(),
                        kind: ProceduralPrimKind::Scope,
                        source_node: None,
                    },
                );
            }
        }
    }

    // Build parent -> children index
    let mut children_index: HashMap<String, Vec<String>> = HashMap::new();
    for path in procedural_prims.keys() {
        // Find parent by stripping last segment
        let parent = if let Some(idx) = path.rfind('/') {
            &path[..idx]
        } else {
            ""
        };
        children_index
            .entry(parent.to_string())
            .or_default()
            .push(path.clone());
    }
    // Sort each children list
    for children in children_index.values_mut() {
        children.sort();
    }

    CachedSceneGraph {
        procedural_prims,
        children_index,
    }
}

/// Merges USD stage + cached procedural prims into one scene graph.
pub struct CompositeProvider<'a> {
    usd_stage: Option<&'a dyn PrimDataProvider>,
    cache: &'a CachedSceneGraph,
}

impl<'a> CompositeProvider<'a> {
    /// Build a composite provider from a USD stage and pre-built cache.
    pub fn new(usd_stage: Option<&'a dyn PrimDataProvider>, cache: &'a CachedSceneGraph) -> Self {
        Self { usd_stage, cache }
    }

    /// Get procedural prim data for the property inspector.
    pub fn get_procedural_data(&self, path: &str) -> Option<&ProceduralPrim> {
        self.cache.procedural_prims.get(path)
    }
}

/// Check if `child` is a direct child of `parent` in prim path hierarchy.
///
/// Empty string `""` is treated as the pseudo-root (parent of all `/Foo` paths).
/// Used by tests; runtime lookups use the pre-computed `children_index`.
#[cfg(test)]
fn is_direct_child(parent: &str, child: &str) -> bool {
    if parent.is_empty() {
        // Root: direct children are single-segment paths like "/World"
        let trimmed = child.trim_start_matches('/');
        return !trimmed.is_empty() && !trimmed.contains('/');
    }
    let Some(suffix) = child.strip_prefix(parent) else {
        return false;
    };
    // Direct child: suffix is "/<name>" with no further slashes
    if let Some(rest) = suffix.strip_prefix('/') {
        !rest.is_empty() && !rest.contains('/')
    } else {
        false
    }
}

impl PrimDataProvider for CompositeProvider<'_> {
    fn root_paths(&self) -> Vec<String> {
        let mut roots: Vec<String> = Vec::new();

        // USD roots
        if let Some(usd) = self.usd_stage {
            roots.extend(usd.root_paths());
        }

        // Procedural root paths from pre-computed index (children of "")
        if let Some(proc_roots) = self.cache.children_index.get("") {
            roots.extend(proc_roots.iter().cloned());
        }

        roots.sort();
        roots.dedup();
        roots
    }

    fn get_prim_info(&self, path: &str) -> Option<PrimDisplayInfo> {
        // Procedural prims take priority (BIF-created data has richer stats)
        if let Some(proc_prim) = self.cache.procedural_prims.get(path) {
            // Warn if this shadows a USD prim
            if self
                .usd_stage
                .as_ref()
                .and_then(|u| u.get_prim_info(path))
                .is_some()
            {
                log::debug!("Procedural prim shadows USD prim at {:?}", path);
            }
            let children = self.get_children(path);
            let child_count = children.len();
            let mut info = PrimDisplayInfo::new(
                proc_prim.path.clone(),
                proc_prim.kind.type_name().to_string(),
                true,
                child_count > 0,
                child_count,
            );
            info.source_node = proc_prim.source_node;
            return Some(info);
        }

        // Fall back to USD stage
        if let Some(usd) = self.usd_stage {
            return usd.get_prim_info(path);
        }

        None
    }

    fn get_children(&self, parent_path: &str) -> Vec<String> {
        let mut children: Vec<String> = Vec::new();

        // USD children
        if let Some(usd) = self.usd_stage {
            children.extend(usd.get_children(parent_path));
        }

        // Procedural children from pre-computed index (O(1) lookup)
        if let Some(proc_children) = self.cache.children_index.get(parent_path) {
            children.extend(proc_children.iter().cloned());
        }

        children.sort();
        children.dedup();
        children
    }
}

/// Filters scene graph to show only prims from a set of upstream nodes.
///
/// Used in NodeContribution view mode: shows the scene "at" a selected node
/// by including prims from the selected node + all upstream nodes.
pub struct NodeFilteredProvider<'a> {
    inner: &'a dyn PrimDataProvider,
    cache: &'a CachedSceneGraph,
    /// The node whose contribution we're inspecting.
    selected_node: GraphNodeId,
    /// Set of allowed paths (prims from upstream nodes + ancestor Scopes).
    allowed_paths: HashSet<String>,
}

impl<'a> NodeFilteredProvider<'a> {
    /// Build a filtered provider showing prims from `upstream_nodes` only.
    ///
    /// `upstream_nodes` should include the selected node itself.
    pub fn new(
        inner: &'a dyn PrimDataProvider,
        cache: &'a CachedSceneGraph,
        selected_node: GraphNodeId,
        upstream_nodes: &HashSet<GraphNodeId>,
    ) -> Self {
        // Collect paths of prims whose source_node is in the upstream set
        let mut allowed_paths = HashSet::new();
        for prim in cache.procedural_prims.values() {
            if let Some(src) = prim.source_node {
                if upstream_nodes.contains(&src) {
                    allowed_paths.insert(prim.path.clone());
                    // Also include all ancestor paths for tree context
                    let mut current = String::new();
                    for segment in prim.path.split('/').filter(|s| !s.is_empty()) {
                        current.push('/');
                        current.push_str(segment);
                        allowed_paths.insert(current.clone());
                    }
                }
            }
        }
        Self {
            inner,
            cache,
            selected_node,
            allowed_paths,
        }
    }

    /// Whether a prim was produced by the selected node (for highlighting).
    pub fn is_from_selected_node(&self, path: &str) -> bool {
        self.cache
            .procedural_prims
            .get(path)
            .and_then(|p| p.source_node)
            .map(|src| src == self.selected_node)
            .unwrap_or(false)
    }
}

impl PrimDataProvider for NodeFilteredProvider<'_> {
    fn root_paths(&self) -> Vec<String> {
        self.inner
            .root_paths()
            .into_iter()
            .filter(|p| self.allowed_paths.contains(p))
            .collect()
    }

    fn get_prim_info(&self, path: &str) -> Option<PrimDisplayInfo> {
        if !self.allowed_paths.contains(path) {
            return None;
        }
        self.inner.get_prim_info(path)
    }

    fn get_children(&self, parent_path: &str) -> Vec<String> {
        self.inner
            .get_children(parent_path)
            .into_iter()
            .filter(|p| self.allowed_paths.contains(p))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scene_browser_state() {
        let mut state = SceneBrowserState::new();

        // Test expansion
        assert!(!state.is_expanded("/World"));
        state.expand("/World");
        assert!(state.is_expanded("/World"));
        state.collapse("/World");
        assert!(!state.is_expanded("/World"));

        // Test toggle
        state.toggle_expanded("/World");
        assert!(state.is_expanded("/World"));
        state.toggle_expanded("/World");
        assert!(!state.is_expanded("/World"));
    }

    #[test]
    fn test_expand_to_path() {
        let mut state = SceneBrowserState::new();
        state.expand_to_path("/World/Geo/Mesh/SubMesh");

        assert!(state.is_expanded("/World"));
        assert!(state.is_expanded("/World/Geo"));
        assert!(state.is_expanded("/World/Geo/Mesh"));
        // The target path itself should not be expanded
        assert!(!state.is_expanded("/World/Geo/Mesh/SubMesh"));
    }

    #[test]
    fn test_prim_type_icon() {
        assert_eq!(prim_type_icon("Mesh").0, "◆");
        assert_eq!(prim_type_icon("Mesh").1, theme::PIN_SCENE);
        assert_eq!(prim_type_icon("Xform").0, "✦");
        assert_eq!(prim_type_icon("Xform").1, theme::STATUS_WARNING);
        assert_eq!(prim_type_icon("PointInstancer").0, "⊕");
        assert_eq!(prim_type_icon("PointInstancer").1, theme::ACCENT_PRIMARY);
        assert_eq!(prim_type_icon("UnknownType").0, "○");
        assert_eq!(prim_type_icon("UnknownType").1, theme::TEXT_SECONDARY);
    }

    #[test]
    fn test_prim_display_info() {
        let info = PrimDisplayInfo::new(
            "/World/Geo/MyMesh".to_string(),
            "Mesh".to_string(),
            true,
            false,
            0,
        );

        assert_eq!(info.name, "MyMesh");
        assert_eq!(info.icon().0, "◆");
        assert_eq!(info.icon().1, theme::PIN_SCENE);
    }

    #[test]
    fn test_filter_matching() {
        let state = SceneBrowserState {
            search_filter: "mesh".to_string(),
            ..Default::default()
        };

        assert!(state.matches_filter("/World/MyMesh", "Mesh"));
        assert!(state.matches_filter("/World/Geo", "Mesh"));
        assert!(!state.matches_filter("/World/Geo", "Xform"));
    }

    #[test]
    fn test_is_direct_child() {
        // Root children
        assert!(is_direct_child("", "/World"));
        assert!(is_direct_child("", "/Foo"));
        assert!(!is_direct_child("", "/World/Geo"));
        assert!(!is_direct_child("", ""));

        // Normal parent-child
        assert!(is_direct_child("/World", "/World/Geo"));
        assert!(is_direct_child("/World", "/World/Cube1"));
        assert!(!is_direct_child("/World", "/World/Geo/Mesh"));
        assert!(!is_direct_child("/World", "/World"));
        assert!(!is_direct_child("/World", "/Other/Geo"));

        // Deeper nesting
        assert!(is_direct_child("/World/Geo", "/World/Geo/Mesh"));
        assert!(!is_direct_child("/World/Geo", "/World/Geo/Mesh/Sub"));

        // Prefix collision (e.g., "/Wo" is not parent of "/World")
        assert!(!is_direct_child("/Wo", "/World"));
    }

    #[test]
    fn test_procedural_prim_kind_type_name() {
        assert_eq!(
            ProceduralPrimKind::Mesh {
                vertex_count: 8,
                triangle_count: 12
            }
            .type_name(),
            "Mesh"
        );
        assert_eq!(
            ProceduralPrimKind::PointInstancer {
                point_count: 100,
                prototype_refs: vec![]
            }
            .type_name(),
            "PointInstancer"
        );
        assert_eq!(ProceduralPrimKind::Scope.type_name(), "Scope");
    }

    #[test]
    fn test_build_scene_graph_cache_children_index() {
        let scene = bif_core::Scene::new("test");
        let empty_proto = HashMap::new();
        let empty_cloud = HashMap::new();
        let cache = build_scene_graph_cache(&scene, &empty_proto, &empty_cloud);
        // Empty scene should produce empty cache
        assert!(cache.procedural_prims.is_empty());
        assert!(cache.children_index.is_empty());
    }

    #[test]
    fn test_source_node_tagging() {
        use crate::node_graph::GraphNodeId;
        use std::sync::Arc;

        let mut scene = bif_core::Scene::new("test");
        scene.prototypes.push(Arc::new(bif_core::Prototype {
            id: 0,
            name: Arc::from("/World/Cube"),
            mesh: Arc::new(bif_core::Mesh::new(vec![], vec![], None)),
            material: None,
        }));

        let node_a = GraphNodeId(42);
        let mut proto_map = HashMap::new();
        proto_map.insert(node_a, vec![0]);
        let empty_cloud = HashMap::new();

        let cache = build_scene_graph_cache(&scene, &proto_map, &empty_cloud);

        // Mesh prim should be tagged with node_a
        let prim = cache.procedural_prims.get("/World/Cube").unwrap();
        assert_eq!(prim.source_node, Some(node_a));

        // Auto-generated Scope "/World" should have no source
        let scope = cache.procedural_prims.get("/World").unwrap();
        assert_eq!(scope.source_node, None);
    }

    #[test]
    fn test_prim_count_by_node() {
        use crate::node_graph::GraphNodeId;
        use std::sync::Arc;

        let mut scene = bif_core::Scene::new("test");
        scene.prototypes.push(Arc::new(bif_core::Prototype {
            id: 0,
            name: Arc::from("/World/Cube"),
            mesh: Arc::new(bif_core::Mesh::new(vec![], vec![], None)),
            material: None,
        }));
        scene.prototypes.push(Arc::new(bif_core::Prototype {
            id: 1,
            name: Arc::from("/World/Sphere"),
            mesh: Arc::new(bif_core::Mesh::new(vec![], vec![], None)),
            material: None,
        }));

        let node_a = GraphNodeId(1);
        let node_b = GraphNodeId(2);
        let mut proto_map = HashMap::new();
        proto_map.insert(node_a, vec![0]);
        proto_map.insert(node_b, vec![1]);
        let empty_cloud = HashMap::new();

        let cache = build_scene_graph_cache(&scene, &proto_map, &empty_cloud);
        let counts = cache.prim_count_by_node();

        assert_eq!(counts.get(&node_a), Some(&1));
        assert_eq!(counts.get(&node_b), Some(&1));
    }

    #[test]
    fn test_node_filtered_provider() {
        use crate::node_graph::GraphNodeId;
        use std::sync::Arc;

        let mut scene = bif_core::Scene::new("test");
        scene.prototypes.push(Arc::new(bif_core::Prototype {
            id: 0,
            name: Arc::from("/World/Cube"),
            mesh: Arc::new(bif_core::Mesh::new(vec![], vec![], None)),
            material: None,
        }));
        scene.prototypes.push(Arc::new(bif_core::Prototype {
            id: 1,
            name: Arc::from("/World/Sphere"),
            mesh: Arc::new(bif_core::Mesh::new(vec![], vec![], None)),
            material: None,
        }));

        let node_a = GraphNodeId(1);
        let node_b = GraphNodeId(2);
        let mut proto_map = HashMap::new();
        proto_map.insert(node_a, vec![0]);
        proto_map.insert(node_b, vec![1]);
        let empty_cloud = HashMap::new();

        let cache = build_scene_graph_cache(&scene, &proto_map, &empty_cloud);
        let composite = CompositeProvider::new(None, &cache);

        // Filter to only node_a (upstream set = {node_a})
        let upstream = HashSet::from([node_a]);
        let filtered = NodeFilteredProvider::new(&composite, &cache, node_a, &upstream);

        // Should include /World/Cube and ancestor /World, but NOT /World/Sphere
        let roots = filtered.root_paths();
        assert!(roots.contains(&"/World".to_string()));

        let children = filtered.get_children("/World");
        assert!(children.contains(&"/World/Cube".to_string()));
        assert!(!children.contains(&"/World/Sphere".to_string()));

        // Selected node check
        assert!(filtered.is_from_selected_node("/World/Cube"));
        assert!(!filtered.is_from_selected_node("/World"));
    }
}
