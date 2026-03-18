//! Unified selection state — prim path, properties, instance index, scene browser.
//!
//! Framework-agnostic: no egui imports. UI reads state, emits `AppEvent`s.

use crate::gpu_types::NO_SELECTION;
use crate::property_inspector::PrimProperties;
use crate::scene_browser::SceneBrowserState;

/// Manages all selection state in the viewport.
pub struct SelectionManager {
    /// Selected USD prim path (from scene browser).
    pub selected_prim_path: Option<String>,
    /// Computed properties for the selected prim.
    pub selected_prim_properties: Option<PrimProperties>,
    /// Selected instance index (from viewport pick or scene browser).
    pub selected_instance_index: Option<usize>,
    /// Scene browser UI state (tree expansion, filtering).
    pub scene_browser_state: SceneBrowserState,
    /// Translate gizmo state.
    pub gizmo_state: crate::gizmo::GizmoState,
}

impl SelectionManager {
    /// Create with default (empty) selection.
    pub fn new() -> Self {
        Self {
            selected_prim_path: None,
            selected_prim_properties: None,
            selected_instance_index: None,
            scene_browser_state: SceneBrowserState::new(),
            gizmo_state: crate::gizmo::GizmoState::new(),
        }
    }

    /// Clear all selection state.
    pub fn clear(&mut self) {
        self.selected_prim_path = None;
        self.selected_prim_properties = None;
        self.selected_instance_index = None;
        self.scene_browser_state = SceneBrowserState::new();
        self.gizmo_state.reset();
    }

    /// GPU highlight instance ID (u32 sentinel if nothing selected).
    pub fn gpu_highlight_id(&self) -> u32 {
        self.selected_instance_index
            .map(|i| i as u32)
            .unwrap_or(NO_SELECTION)
    }
}

impl Default for SelectionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_selection_is_empty() {
        let sel = SelectionManager::new();
        assert!(sel.selected_prim_path.is_none());
        assert!(sel.selected_instance_index.is_none());
        assert_eq!(sel.gpu_highlight_id(), NO_SELECTION);
    }

    #[test]
    fn gpu_highlight_id_maps_index() {
        let mut sel = SelectionManager::new();
        sel.selected_instance_index = Some(42);
        assert_eq!(sel.gpu_highlight_id(), 42);
    }

    #[test]
    fn clear_resets_all() {
        let mut sel = SelectionManager::new();
        sel.selected_prim_path = Some("/root/mesh".to_string());
        sel.selected_instance_index = Some(5);
        sel.clear();
        assert!(sel.selected_prim_path.is_none());
        assert!(sel.selected_instance_index.is_none());
        assert_eq!(sel.gpu_highlight_id(), NO_SELECTION);
    }
}
