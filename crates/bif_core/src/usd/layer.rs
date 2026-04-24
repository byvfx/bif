//! Safe Rust types for USD layer-aware stage inspection (v0.14.0).
//!
//! These types are UI-agnostic and intentionally read-only — BIF v0.14.0 can
//! display the sublayer stack, inspect opinion sources per attribute, mute
//! layers, and choose a payload policy, but it does not yet author opinions.
//! Write paths land in v0.16.
//!
//! Types here are populated by conversion functions in [`super::ffi_convert`]
//! from the C FFI structs defined in [`super::ffi_raw`]. They flow up into
//! [`super::cpp_bridge::UsdStage`] methods as the Rust-facing API.

use std::path::PathBuf;

/// Payload loading policy for [`super::cpp_bridge::UsdStage::open_with_policy`].
///
/// BoundingBoxOnly is intentionally omitted — USD has no native mode for it,
/// and the two native modes cover the v0.14.0 use case. See the plan for the
/// deferred-policy decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PayloadPolicy {
    /// Load every payload eagerly (default USD behavior).
    #[default]
    LoadAll,
    /// Open hierarchy only; load payloads lazily on demand.
    LoadNone,
}

/// Time offset + scale authored on a sublayer reference (SdfLayerOffset).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayerOffset {
    pub offset: f64,
    pub scale: f64,
}

impl Default for LayerOffset {
    fn default() -> Self {
        Self {
            offset: 0.0,
            scale: 1.0,
        }
    }
}

impl LayerOffset {
    pub fn is_identity(&self) -> bool {
        self.offset == 0.0 && self.scale == 1.0
    }
}

/// Info about one layer in the stage's layer stack (root + recursive sublayers).
///
/// Populated from `UsdBridgeLayerInfoRaw`; all `String` fields are owned
/// (strings were strdup'd across the FFI boundary and copied into Rust).
#[derive(Debug, Clone)]
pub struct LayerInfo {
    /// Authored USD layer identifier — unique within the stage; may be
    /// an asset path (e.g. `@./shot.usd@`) or an anonymous-layer tag.
    pub identifier: String,
    /// Short display name (basename of identifier).
    pub display_name: String,
    /// Resolved filesystem path. Empty for anonymous / in-memory layers.
    pub real_path: PathBuf,
    /// True for anonymous (in-memory) layers that cannot be saved to disk.
    pub is_anonymous: bool,
    /// True if the layer has unsaved edits.
    pub is_dirty: bool,
    /// True if this layer is currently muted on the stage.
    pub is_muted: bool,
    /// True when USD permits authored edits on this layer
    /// (`SdfLayer::PermissionToEdit()`).
    pub permission_to_edit: bool,
    /// Time offset + scale applied to this layer's opinions when composed.
    pub offset: LayerOffset,
    /// Index into [`LayerStack::layers`] of this layer's parent in the
    /// sublayer tree. `None` for the root layer.
    pub parent_index: Option<usize>,
    /// Depth in the sublayer tree. 0 = root; 1+ = sublayers.
    pub depth: u8,
}

/// Flattened sublayer tree (root at [`Self::root_index`]; each entry's
/// [`LayerInfo::parent_index`] points back to its parent in the `layers` vec).
#[derive(Debug, Clone, Default)]
pub struct LayerStack {
    pub layers: Vec<LayerInfo>,
    pub root_index: usize,
}

impl LayerStack {
    /// Find a layer by its authored identifier.
    pub fn find_by_identifier(&self, identifier: &str) -> Option<(usize, &LayerInfo)> {
        self.layers
            .iter()
            .enumerate()
            .find(|(_, l)| l.identifier == identifier)
    }

    /// Iterate sublayers of a given parent index (non-recursive, direct children only).
    pub fn children_of(&self, parent: usize) -> impl Iterator<Item = (usize, &LayerInfo)> {
        self.layers
            .iter()
            .enumerate()
            .filter(move |(_, l)| l.parent_index == Some(parent))
    }
}

/// Current edit target — informational only in v0.14.0 (read-only release).
#[derive(Debug, Clone)]
pub struct EditTarget {
    pub layer_identifier: String,
}

/// One prim spec entry from `UsdPrim::GetPrimStack`.
///
/// Each entry identifies a layer that authors an opinion on the prim,
/// with the spec's specifier and whether it has any authored fields.
#[derive(Debug, Clone)]
pub struct PrimStackEntry {
    pub layer_identifier: String,
    pub path: String,
    pub specifier: PrimSpecifier,
    pub has_authored_opinions: bool,
}

/// USD `SdfSpecifier` — how a prim spec relates to the composed prim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrimSpecifier {
    /// `def` — defines a new prim with a type.
    Def,
    /// `over` — authors overrides on an existing prim.
    Over,
    /// `class` — defines a class prim (can be inherited from).
    Class,
}

impl PrimSpecifier {
    pub fn from_u8(raw: u8) -> Self {
        match raw {
            0 => Self::Def,
            2 => Self::Class,
            _ => Self::Over,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Def => "def",
            Self::Over => "over",
            Self::Class => "class",
        }
    }
}

/// One opinion source contributing to an attribute's composed value.
///
/// Returned by [`super::cpp_bridge::UsdStage::get_attribute_opinions`] for a
/// given `(prim_path, attribute_name)`. The strongest opinion is at index 0
/// in the returned `Vec` (`winning_index` on the FFI side, preserved here by
/// the `is_winning` flag).
#[derive(Debug, Clone)]
pub struct OpinionSource {
    /// Layer identifier authoring this opinion.
    pub layer_identifier: String,
    /// Display string of the authored value (`TfStringify` on the C++ side).
    /// Intentionally NOT deserializable — v0.14 is read-only.
    pub value_display: String,
    /// USD type name token (e.g. `"float3"`, `"token"`). Empty if no opinion.
    pub value_type: String,
    /// True for the strongest opinion (the value the composed stage sees).
    pub is_winning: bool,
}
