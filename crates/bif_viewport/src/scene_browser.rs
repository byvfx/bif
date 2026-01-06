//! USD Scene Browser - Hierarchy view for USD scene graph.
//!
//! Provides an interactive tree widget for browsing USD prim hierarchy,
//! inspired by Gaffer's HierarchyView.
//!
//! # Features
//! - Expandable tree with lazy child loading
//! - Type icons for prim types (Mesh, Xform, Instancer, Scope, etc.)
//! - Search/filter functionality
//! - Keyboard navigation (arrow keys for expand/collapse)
//! - Selection synced with viewport
//!
//! # TODOs
//! - [ ] Focus vs Selection model (Gaffer-style: focus for viewer, selection for inspector)
//! - [ ] Keyboard navigation with arrow keys (↑↓ to expand/collapse, Shift+↓ expand all)

use std::collections::HashSet;

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
}

impl SceneBrowserState {
    /// Create a new scene browser state.
    pub fn new() -> Self {
        Self {
            selected_path: None,
            expanded_paths: HashSet::new(),
            search_filter: String::new(),
            show_inactive: true,
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

/// Get the icon for a USD prim type.
pub fn prim_type_icon(type_name: &str) -> &'static str {
    match type_name {
        "Mesh" => "🔷",           // Blue diamond for mesh
        "Xform" => "📐",          // Transform
        "PointInstancer" => "🔁", // Instancer
        "Scope" => "📁",          // Folder/scope
        "Camera" => "📷",         // Camera
        "Light" | "DistantLight" | "DomeLight" | "SphereLight" | "RectLight" => "💡",
        "Material" => "🎨",       // Material
        "Shader" => "🔲",         // Shader
        "Skeleton" => "🦴",       // Skeleton
        "SkelRoot" => "🦴",       // Skeleton root
        "" => "◇",                // Empty/unknown type
        _ => "○",                 // Default circle for other types
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
        let name = path
            .rsplit('/')
            .next()
            .unwrap_or(&path)
            .to_string();

        Self {
            path,
            name,
            type_name,
            is_active,
            has_children,
            child_count,
        }
    }

    /// Get the icon for this prim's type.
    pub fn icon(&self) -> &'static str {
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

/// Render the scene browser UI.
///
/// Returns `Some(path)` if selection changed, `None` otherwise.
pub fn render_scene_browser(
    ui: &mut egui::Ui,
    state: &mut SceneBrowserState,
    provider: &dyn PrimDataProvider,
) -> Option<String> {
    let mut selection_changed: Option<String> = None;

    // Search/filter box
    ui.horizontal(|ui| {
        ui.label("🔍");
        ui.text_edit_singleline(&mut state.search_filter);
        if ui.button("✕").clicked() {
            state.search_filter.clear();
        }
    });

    ui.separator();

    // Show checkbox for inactive prims
    ui.checkbox(&mut state.show_inactive, "Show inactive prims");

    ui.separator();

    // Prim hierarchy tree
    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            let root_paths = provider.root_paths();

            if root_paths.is_empty() {
                ui.label("No USD scene loaded");
                ui.label("Load a .usda or .usdc file to browse");
            } else {
                for root_path in root_paths {
                    if let Some(new_selection) =
                        render_prim_tree(ui, state, provider, &root_path, 0)
                    {
                        selection_changed = Some(new_selection);
                    }
                }
            }
        });

    selection_changed
}

/// Recursively render a prim and its children.
fn render_prim_tree(
    ui: &mut egui::Ui,
    state: &mut SceneBrowserState,
    provider: &dyn PrimDataProvider,
    path: &str,
    depth: usize,
) -> Option<String> {
    let Some(info) = provider.get_prim_info(path) else {
        return None;
    };

    // Filter check
    if !state.show_inactive && !info.is_active {
        return None;
    }

    // For filtering, we need to check if this prim or any descendant matches
    let matches = state.matches_filter(&info.path, &info.type_name);

    // If using filter and this doesn't match, still render if has children that might match
    // (This is a simplified approach - full impl would check descendants)
    if !matches && !state.search_filter.is_empty() && !info.has_children {
        return None;
    }

    let mut selection_changed: Option<String> = None;
    let is_selected = state.selected_path.as_ref() == Some(&info.path);
    let is_expanded = state.is_expanded(&info.path);

    // Indent based on depth
    let indent = depth as f32 * 16.0;

    ui.horizontal(|ui| {
        ui.add_space(indent);

        // Expand/collapse button (only if has children)
        if info.has_children {
            let expand_text = if is_expanded { "▼" } else { "▶" };
            if ui.small_button(expand_text).clicked() {
                state.toggle_expanded(&info.path);
            }
        } else {
            // Placeholder for alignment
            ui.add_space(20.0);
        }

        // Type icon
        ui.label(info.icon());

        // Prim name (selectable)
        let name_text = if info.is_active {
            egui::RichText::new(&info.name)
        } else {
            egui::RichText::new(&info.name).color(egui::Color32::GRAY)
        };

        let response = ui.selectable_label(is_selected, name_text);

        if response.clicked() {
            state.select(&info.path);
            selection_changed = Some(info.path.clone());
        }

        // Show type in tooltip
        response.on_hover_text(format!("{}\nType: {}", info.path, info.type_name));

        // Show child count if has children
        if info.has_children {
            ui.label(
                egui::RichText::new(format!("({})", info.child_count))
                    .small()
                    .color(egui::Color32::GRAY),
            );
        }
    });

    // Render children if expanded
    if is_expanded && info.has_children {
        let children = provider.get_children(&info.path);
        for child_path in children {
            if let Some(new_selection) = render_prim_tree(ui, state, provider, &child_path, depth + 1)
            {
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
            PrimDisplayInfo::new(
                info.path,
                info.type_name,
                info.is_active,
                info.has_children,
                info.child_count,
            )
        })
    }

    fn get_children(&self, parent_path: &str) -> Vec<String> {
        self.child_prim_paths(parent_path).unwrap_or_default()
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
        assert_eq!(prim_type_icon("Mesh"), "🔷");
        assert_eq!(prim_type_icon("Xform"), "📐");
        assert_eq!(prim_type_icon("PointInstancer"), "🔁");
        assert_eq!(prim_type_icon("UnknownType"), "○");
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
        assert_eq!(info.icon(), "🔷");
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
}
