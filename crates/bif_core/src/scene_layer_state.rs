//! Stage-level layer state attached to [`crate::Scene`] (v0.14.0).
//!
//! Holds the inspection state the UI needs to render the layer stack,
//! opinion inspector, and layer-color dots — the sublayer tree, which
//! layer is the working target, which are muted, the current payload
//! policy, and a `prim_path → strongest-layer-index` map used for
//! color dots in the scene browser.
//!
//! Intentionally UI-agnostic so the v0.15 Qt port can reuse it verbatim.
//! Does NOT hold the [`crate::usd::cpp_bridge::UsdStage`] itself — opinion
//! traces are queried on demand against whatever stage handle the caller
//! has (typically `Arc<Mutex<UsdStage>>` in `bif_viewport`). Caching, if
//! desired later, can wrap this type without modifying it.

use std::collections::{HashMap, HashSet};

use crate::usd::cpp_bridge::{UsdBridgeError, UsdBridgeResult, UsdStage};
use crate::usd::edit_history::{EditHistory, EditOperation};
use crate::usd::layer::{LayerStack, PayloadPolicy};

/// Layer inspection state bound to a loaded USD stage.
#[derive(Clone, Debug, Default)]
pub struct SceneLayerState {
    /// Flattened sublayer tree (root + recursive sublayers).
    pub stack: LayerStack,
    /// Index into [`LayerStack::layers`] of the working (edit-target-
    /// candidate) layer. Defaults to the root layer at load time.
    pub working_layer: usize,
    /// Layer identifiers currently muted on the stage. Mirrors the
    /// per-`LayerInfo` `is_muted` flag and is kept in sync by
    /// [`SceneLayerState::set_muted`].
    pub muted: HashSet<String>,
    /// True when the UI should visually isolate the working layer —
    /// dim or hide opinions authored in other layers. v0.14 is read-only
    /// so this is purely a UI hint; enforcement lands with write paths.
    pub isolation_mode: bool,
    /// Payload-loading policy the stage was opened with.
    pub payload_policy: PayloadPolicy,
    /// `prim_path → index into stack.layers` pointing at the strongest
    /// opinion source for that prim. Drives the scene-browser color
    /// dots. Populated lazily by callers; may be partial.
    pub layer_for_prim: HashMap<String, usize>,
    /// Parallel USD opinion history for authored layer edits.
    pub edit_history: EditHistory,
}

impl SceneLayerState {
    /// Build fresh state from a loaded stage. Fetches the layer stack,
    /// seeds `muted` from the stage's currently-muted layers, and
    /// defaults `working_layer` to the root. `layer_for_prim` starts
    /// empty — call [`SceneLayerState::populate_layer_for_prim`] after
    /// you have the scene's prim path list.
    pub fn from_stage(stage: &UsdStage, payload_policy: PayloadPolicy) -> UsdBridgeResult<Self> {
        let stack = stage.get_layer_stack()?;
        let working_layer = stack.root_index;
        let muted: HashSet<String> = stack
            .layers
            .iter()
            .filter(|l| l.is_muted)
            .map(|l| l.identifier.clone())
            .collect();
        let working_layer_id = stack
            .layers
            .get(working_layer)
            .map(|l| l.identifier.clone())
            .unwrap_or_default();
        Ok(Self {
            stack,
            working_layer,
            muted,
            isolation_mode: false,
            payload_policy,
            layer_for_prim: HashMap::new(),
            edit_history: EditHistory::with_working_layer(working_layer_id),
        })
    }

    /// Resolve a layer index by its authored identifier.
    pub fn layer_index(&self, identifier: &str) -> Option<usize> {
        self.stack
            .find_by_identifier(identifier)
            .map(|(idx, _)| idx)
    }

    /// Returns true when `identifier` is currently muted.
    pub fn is_muted(&self, identifier: &str) -> bool {
        self.muted.contains(identifier)
    }

    /// Update muted state after a successful stage mute/unmute call.
    /// Keeps the [`LayerInfo::is_muted`] flag in sync.
    pub fn set_muted(&mut self, identifier: &str, muted: bool) {
        if muted {
            self.muted.insert(identifier.to_string());
        } else {
            self.muted.remove(identifier);
        }
        if let Some((idx, _)) = self.stack.find_by_identifier(identifier) {
            self.stack.layers[idx].is_muted = muted;
        }
    }

    /// Mutate the working-layer index. No-op if `index` is out of range.
    pub fn set_working_layer(&mut self, index: usize) {
        if index < self.stack.layers.len() {
            self.working_layer = index;
            if let Some(layer) = self.stack.layers.get(index) {
                self.edit_history
                    .set_working_layer(layer.identifier.clone());
            }
        }
    }

    /// Mutate the working-layer index after checking the target layer against
    /// the live stage. This does not cache any C++ layer pointer.
    pub fn set_edit_target(&mut self, index: usize, stage: &UsdStage) -> UsdBridgeResult<()> {
        let layer = self
            .stack
            .layers
            .get(index)
            .ok_or_else(|| UsdBridgeError::InvalidPrim(format!("layer index {index}")))?;
        let _can_edit = stage.layer_permission_to_edit(&layer.identifier)?;
        self.working_layer = index;
        self.edit_history
            .set_working_layer(layer.identifier.clone());
        Ok(())
    }

    /// Apply and record a USD edit against the active working layer.
    pub fn apply_edit_operation(
        &mut self,
        stage: &UsdStage,
        op: EditOperation,
    ) -> UsdBridgeResult<String> {
        let desc = self.edit_history.apply_and_record(stage, op)?;
        self.mark_working_layer_dirty(true);
        Ok(desc)
    }

    pub fn undo_usd_edit(&mut self, stage: &UsdStage) -> UsdBridgeResult<Option<String>> {
        let result = self.edit_history.undo(stage)?;
        if result.is_some() {
            self.mark_working_layer_dirty(true);
        }
        Ok(result)
    }

    pub fn redo_usd_edit(&mut self, stage: &UsdStage) -> UsdBridgeResult<Option<String>> {
        let result = self.edit_history.redo(stage)?;
        if result.is_some() {
            self.mark_working_layer_dirty(true);
        }
        Ok(result)
    }

    pub fn mark_working_layer_dirty(&mut self, dirty: bool) {
        if let Some(layer) = self.stack.layers.get_mut(self.working_layer) {
            layer.is_dirty = dirty;
        }
    }

    pub fn has_usd_undo(&self) -> bool {
        self.edit_history.can_undo()
    }

    pub fn has_usd_redo(&self) -> bool {
        self.edit_history.can_redo()
    }

    /// Populate `layer_for_prim` by walking the given prim paths and
    /// asking the stage which layer authored each prim's strongest
    /// opinion. Call once after stage load with the scene's prim list.
    ///
    /// Overwrites any existing entries. Prims with no resolvable stack
    /// or whose strongest-opinion layer isn't in the layer stack are
    /// silently skipped (not every prim has a USD origin — procedural
    /// prims won't match).
    pub fn populate_layer_for_prim<I, S>(&mut self, stage: &UsdStage, prim_paths: I)
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.layer_for_prim.clear();
        for path in prim_paths {
            let path = path.as_ref();
            let Ok(entries) = stage.get_prim_stack(path) else {
                continue;
            };
            let Some(strongest) = entries.first() else {
                continue;
            };
            if let Some(idx) = self.layer_index(&strongest.layer_identifier) {
                self.layer_for_prim.insert(path.to_string(), idx);
            }
        }
    }

    /// Color-dot accessor — returns the layer index authoring the
    /// strongest opinion for `prim_path`, if known.
    pub fn layer_for(&self, prim_path: &str) -> Option<usize> {
        self.layer_for_prim.get(prim_path).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usd::layer::{LayerInfo, LayerOffset};

    fn make_layer(identifier: &str, parent: Option<usize>, depth: u8, muted: bool) -> LayerInfo {
        LayerInfo {
            identifier: identifier.to_string(),
            display_name: identifier.to_string(),
            real_path: Default::default(),
            is_anonymous: false,
            is_dirty: false,
            is_muted: muted,
            permission_to_edit: true,
            offset: LayerOffset::default(),
            parent_index: parent,
            depth,
        }
    }

    fn make_state(layers: Vec<LayerInfo>) -> SceneLayerState {
        let working_layer_id = layers
            .first()
            .map(|l| l.identifier.clone())
            .unwrap_or_default();
        let muted: HashSet<String> = layers
            .iter()
            .filter(|l| l.is_muted)
            .map(|l| l.identifier.clone())
            .collect();
        SceneLayerState {
            stack: LayerStack {
                layers,
                root_index: 0,
            },
            working_layer: 0,
            muted,
            isolation_mode: false,
            payload_policy: PayloadPolicy::LoadAll,
            layer_for_prim: HashMap::new(),
            edit_history: EditHistory::with_working_layer(working_layer_id),
        }
    }

    #[test]
    fn default_state_is_empty() {
        let s = SceneLayerState::default();
        assert!(s.stack.layers.is_empty());
        assert_eq!(s.working_layer, 0);
        assert!(s.muted.is_empty());
        assert!(!s.isolation_mode);
        assert_eq!(s.payload_policy, PayloadPolicy::LoadAll);
        assert!(s.layer_for_prim.is_empty());
        assert!(s.edit_history.working_layer_id.is_empty());
    }

    #[test]
    fn layer_index_finds_by_identifier() {
        let state = make_state(vec![
            make_layer("root.usd", None, 0, false),
            make_layer("anim.usd", Some(0), 1, false),
        ]);
        assert_eq!(state.layer_index("root.usd"), Some(0));
        assert_eq!(state.layer_index("anim.usd"), Some(1));
        assert_eq!(state.layer_index("missing.usd"), None);
    }

    #[test]
    fn set_muted_updates_both_set_and_layer_flag() {
        let mut state = make_state(vec![
            make_layer("root.usd", None, 0, false),
            make_layer("anim.usd", Some(0), 1, false),
        ]);
        state.set_muted("anim.usd", true);
        assert!(state.is_muted("anim.usd"));
        assert!(state.stack.layers[1].is_muted);

        state.set_muted("anim.usd", false);
        assert!(!state.is_muted("anim.usd"));
        assert!(!state.stack.layers[1].is_muted);
    }

    #[test]
    fn set_muted_unknown_identifier_still_tracks_in_set() {
        // Even if the identifier isn't in our stack (e.g. transient session
        // layer), we record it so the UI can reflect the stage's state.
        let mut state = make_state(vec![make_layer("root.usd", None, 0, false)]);
        state.set_muted("session.usd", true);
        assert!(state.is_muted("session.usd"));
    }

    #[test]
    fn set_working_layer_clamps_on_out_of_range() {
        let mut state = make_state(vec![
            make_layer("root.usd", None, 0, false),
            make_layer("anim.usd", Some(0), 1, false),
        ]);
        state.set_working_layer(1);
        assert_eq!(state.working_layer, 1);
        assert_eq!(state.edit_history.working_layer_id, "anim.usd");

        // Out-of-range should be a no-op, not a panic.
        state.set_working_layer(99);
        assert_eq!(state.working_layer, 1);
        assert_eq!(state.edit_history.working_layer_id, "anim.usd");
    }

    #[test]
    fn mark_working_layer_dirty_updates_layer_flag() {
        let mut state = make_state(vec![
            make_layer("root.usd", None, 0, false),
            make_layer("anim.usd", Some(0), 1, false),
        ]);
        state.set_working_layer(1);
        state.mark_working_layer_dirty(true);
        assert!(state.stack.layers[1].is_dirty);
        state.mark_working_layer_dirty(false);
        assert!(!state.stack.layers[1].is_dirty);
    }

    #[test]
    fn layer_for_returns_stored_index() {
        let mut state = make_state(vec![
            make_layer("root.usd", None, 0, false),
            make_layer("anim.usd", Some(0), 1, false),
        ]);
        state.layer_for_prim.insert("/World/Hero".to_string(), 1);
        assert_eq!(state.layer_for("/World/Hero"), Some(1));
        assert_eq!(state.layer_for("/World/Prop"), None);
    }
}
