//! Layer-aware USD edit history.
//!
//! This history is intentionally parallel to [`crate::undo::UndoStack`].
//! Procedural node edits stay in `UndoStack`; authored USD opinions use
//! `EditHistory` and are translated from viewport instance identity at the
//! boundary.

use std::collections::HashMap;

use crate::usd::cpp_bridge::{UsdBridgeResult, UsdStage};

const DEFAULT_HISTORY_CAP: usize = 256;

/// USD opinion identity used by edit history.
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct OpinionKey {
    pub prim_path: String,
    pub attr: AttrSlot,
}

impl OpinionKey {
    pub fn new(prim_path: impl Into<String>, attr: AttrSlot) -> Self {
        Self {
            prim_path: prim_path.into(),
            attr,
        }
    }
}

/// Authored USD slot for an edit.
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub enum AttrSlot {
    Xform,
    Visibility,
    MaterialBinding,
    ShaderInput {
        shader_path: String,
        name: String,
    },
    VariantSelection {
        vset: String,
    },
    PayloadLoad,
    Reference {
        index: u32,
    },
    /// Whole-layer USDA replace — `prim_path` carries the layer id.
    LayerContents,
    /// Surface-shader `info:id` token swap. `prim_path` carries the
    /// shader prim path. Used by the shading-model dropdown. C4b-3.
    ShaderId,
}

/// USD shader input value supported by the v0.16 foundation.
#[derive(Clone, Debug, PartialEq)]
pub enum ShaderValue {
    Bool(bool),
    Int(i32),
    Float(f32),
    Double(f64),
    Token(String),
    String(String),
    Color3f([f32; 3]),
    Vec3f([f32; 3]),
}

impl ShaderValue {
    pub fn value_type(&self) -> &'static str {
        match self {
            ShaderValue::Bool(_) => "bool",
            ShaderValue::Int(_) => "int",
            ShaderValue::Float(_) => "float",
            ShaderValue::Double(_) => "double",
            ShaderValue::Token(_) => "token",
            ShaderValue::String(_) => "string",
            ShaderValue::Color3f(_) => "color3f",
            ShaderValue::Vec3f(_) => "float3",
        }
    }

    pub fn value_string(&self) -> String {
        match self {
            ShaderValue::Bool(v) => v.to_string(),
            ShaderValue::Int(v) => v.to_string(),
            ShaderValue::Float(v) => v.to_string(),
            ShaderValue::Double(v) => v.to_string(),
            ShaderValue::Token(v) | ShaderValue::String(v) => v.clone(),
            ShaderValue::Color3f(v) | ShaderValue::Vec3f(v) => {
                format!("{},{},{}", v[0], v[1], v[2])
            }
        }
    }
}

/// User edit that authors a USD opinion on the active working layer.
#[derive(Clone, Debug, PartialEq)]
pub enum EditOperation {
    Transform {
        key: OpinionKey,
        before: Option<[f32; 16]>,
        after: [f32; 16],
    },
    Visibility {
        key: OpinionKey,
        before: Option<bool>,
        after: bool,
    },
    MaterialAssign {
        key: OpinionKey,
        before: Option<String>,
        after: String,
    },
    MaterialParamOverride {
        key: OpinionKey,
        before: Option<ShaderValue>,
        after: ShaderValue,
    },
    VariantSelect {
        key: OpinionKey,
        before: Option<String>,
        after: String,
    },
    /// Wholesale replace of the working layer's USDA text.
    /// Authored by the USDA panel Apply path. `before` is the
    /// pre-replace serialized layer (captured immediately before
    /// import); the inverse re-imports `before`. `key` carries
    /// the layer identifier in `prim_path` and `AttrSlot::LayerContents`.
    ReplaceLayerContents {
        key: OpinionKey,
        before: String,
        after: String,
    },
    /// Author `info:id` on a surface shader. C4b-3.
    SetShaderId {
        key: OpinionKey,
        before: Option<String>,
        after: String,
    },
}

impl EditOperation {
    pub fn key(&self) -> &OpinionKey {
        match self {
            EditOperation::Transform { key, .. }
            | EditOperation::Visibility { key, .. }
            | EditOperation::MaterialAssign { key, .. }
            | EditOperation::MaterialParamOverride { key, .. }
            | EditOperation::VariantSelect { key, .. }
            | EditOperation::ReplaceLayerContents { key, .. }
            | EditOperation::SetShaderId { key, .. } => key,
        }
    }

    /// Construct a ReplaceLayerContents op for `layer_id`.
    pub fn replace_layer(layer_id: impl Into<String>, before: String, after: String) -> Self {
        EditOperation::ReplaceLayerContents {
            key: OpinionKey::new(layer_id, AttrSlot::LayerContents),
            before,
            after,
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            EditOperation::Transform { .. } => "USD transform",
            EditOperation::Visibility { .. } => "USD visibility",
            EditOperation::MaterialAssign { .. } => "USD material",
            EditOperation::MaterialParamOverride { .. } => "USD material parameter",
            EditOperation::VariantSelect { .. } => "USD variant",
            EditOperation::ReplaceLayerContents { .. } => "USDA layer replace",
            EditOperation::SetShaderId { .. } => "USD shader id",
        }
    }

    pub fn apply(&self, stage: &UsdStage, working_layer_id: &str) -> UsdBridgeResult<String> {
        match self {
            EditOperation::Transform { key, after, .. } => {
                stage.write_layer_xform(working_layer_id, &key.prim_path, -1.0, after)?;
            }
            EditOperation::Visibility { key, after, .. } => {
                stage.write_layer_visibility(working_layer_id, &key.prim_path, *after)?;
            }
            EditOperation::MaterialAssign { key, after, .. } => {
                stage.bind_layer_material(working_layer_id, &key.prim_path, after)?;
            }
            EditOperation::MaterialParamOverride { key, after, .. } => {
                let AttrSlot::ShaderInput { shader_path, name } = &key.attr else {
                    return Err(crate::usd::cpp_bridge::UsdBridgeError::InvalidPrim(
                        "MaterialParamOverride requires ShaderInput key".to_string(),
                    ));
                };
                stage.set_layer_shader_input(
                    working_layer_id,
                    shader_path,
                    name,
                    after.value_type(),
                    &after.value_string(),
                )?;
            }
            EditOperation::VariantSelect { key, after, .. } => {
                let AttrSlot::VariantSelection { vset } = &key.attr else {
                    return Err(crate::usd::cpp_bridge::UsdBridgeError::InvalidPrim(
                        "VariantSelect requires VariantSelection key".to_string(),
                    ));
                };
                stage.set_variant_selection(&key.prim_path, vset, after, working_layer_id)?;
            }
            EditOperation::ReplaceLayerContents { key, after, .. } => {
                if !matches!(key.attr, AttrSlot::LayerContents) {
                    return Err(crate::usd::cpp_bridge::UsdBridgeError::InvalidPrim(
                        "ReplaceLayerContents requires LayerContents key".to_string(),
                    ));
                }
                stage.import_layer_from_string(&key.prim_path, after)?;
            }
            EditOperation::SetShaderId { key, after, .. } => {
                if !matches!(key.attr, AttrSlot::ShaderId) {
                    return Err(crate::usd::cpp_bridge::UsdBridgeError::InvalidPrim(
                        "SetShaderId requires ShaderId key".to_string(),
                    ));
                }
                stage.set_layer_shader_id(working_layer_id, &key.prim_path, after)?;
            }
        }
        Ok(self.description().to_string())
    }

    fn inverse(&self) -> Option<Self> {
        match self {
            EditOperation::Transform { key, before, after } => {
                before.map(|before| EditOperation::Transform {
                    key: key.clone(),
                    before: Some(*after),
                    after: before,
                })
            }
            EditOperation::Visibility { key, before, after } => {
                before.map(|before| EditOperation::Visibility {
                    key: key.clone(),
                    before: Some(*after),
                    after: before,
                })
            }
            EditOperation::MaterialAssign { key, before, after } => {
                before.as_ref().map(|before| EditOperation::MaterialAssign {
                    key: key.clone(),
                    before: Some(after.clone()),
                    after: before.clone(),
                })
            }
            EditOperation::MaterialParamOverride { key, before, after } => {
                before
                    .as_ref()
                    .map(|before| EditOperation::MaterialParamOverride {
                        key: key.clone(),
                        before: Some(after.clone()),
                        after: before.clone(),
                    })
            }
            EditOperation::VariantSelect { key, before, after } => {
                before.as_ref().map(|before| EditOperation::VariantSelect {
                    key: key.clone(),
                    before: Some(after.clone()),
                    after: before.clone(),
                })
            }
            EditOperation::ReplaceLayerContents { key, before, after } => {
                Some(EditOperation::ReplaceLayerContents {
                    key: key.clone(),
                    before: after.clone(),
                    after: before.clone(),
                })
            }
            EditOperation::SetShaderId { key, before, after } => {
                before.as_ref().map(|before| EditOperation::SetShaderId {
                    key: key.clone(),
                    before: Some(after.clone()),
                    after: before.clone(),
                })
            }
        }
    }
}

/// Undo unit for USD edits.
#[derive(Clone, Debug, PartialEq)]
pub enum UndoFrame {
    Single {
        layer_id: String,
        operation: EditOperation,
    },
    Group {
        label: String,
        layer_id: String,
        operations: Vec<EditOperation>,
    },
}

impl UndoFrame {
    fn label(&self) -> String {
        match self {
            UndoFrame::Single { operation, .. } => operation.description().to_string(),
            UndoFrame::Group { label, .. } => label.clone(),
        }
    }

    fn operations(&self) -> &[EditOperation] {
        match self {
            UndoFrame::Single { operation, .. } => std::slice::from_ref(operation),
            UndoFrame::Group { operations, .. } => operations,
        }
    }

    fn layer_id(&self) -> &str {
        match self {
            UndoFrame::Single { layer_id, .. } | UndoFrame::Group { layer_id, .. } => layer_id,
        }
    }
}

/// Parallel undo/redo stack for USD layer opinions.
#[derive(Clone, Debug)]
pub struct EditHistory {
    pub working_layer_id: String,
    current_state: HashMap<OpinionKey, EditOperation>,
    undo: Vec<UndoFrame>,
    redo: Vec<UndoFrame>,
    open_group: Option<(String, String, Vec<EditOperation>)>,
    cap: usize,
}

impl Default for EditHistory {
    fn default() -> Self {
        Self::new()
    }
}

impl EditHistory {
    pub fn new() -> Self {
        Self {
            working_layer_id: String::new(),
            current_state: HashMap::new(),
            undo: Vec::new(),
            redo: Vec::new(),
            open_group: None,
            cap: DEFAULT_HISTORY_CAP,
        }
    }

    pub fn with_working_layer(working_layer_id: impl Into<String>) -> Self {
        let mut history = Self::new();
        history.set_working_layer(working_layer_id);
        history
    }

    pub fn set_working_layer(&mut self, working_layer_id: impl Into<String>) {
        self.working_layer_id = working_layer_id.into();
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }

    pub fn redo_len(&self) -> usize {
        self.redo.len()
    }

    pub fn current_state_len(&self) -> usize {
        self.current_state.len()
    }

    pub fn set_cap_for_tests(&mut self, cap: usize) {
        self.cap = cap.max(1);
        self.enforce_cap();
    }

    pub fn begin_group(&mut self, label: impl Into<String>) {
        if self.open_group.is_none() {
            self.open_group = Some((label.into(), self.working_layer_id.clone(), Vec::new()));
        }
    }

    pub fn end_group(&mut self) {
        let Some((label, layer_id, operations)) = self.open_group.take() else {
            return;
        };
        if operations.is_empty() {
            return;
        }
        if operations.len() == 1 {
            self.push_undo(UndoFrame::Single {
                layer_id,
                operation: operations[0].clone(),
            });
        } else {
            self.push_undo(UndoFrame::Group {
                label,
                layer_id,
                operations,
            });
        }
    }

    pub fn cancel_group(&mut self) {
        self.open_group = None;
    }

    pub fn record(&mut self, op: EditOperation) {
        self.current_state.insert(op.key().clone(), op.clone());
        self.redo.clear();
        if let Some((_, _, operations)) = self.open_group.as_mut() {
            operations.push(op);
            return;
        }
        self.push_undo(UndoFrame::Single {
            layer_id: self.working_layer_id.clone(),
            operation: op,
        });
    }

    pub fn apply_and_record(
        &mut self,
        stage: &UsdStage,
        op: EditOperation,
    ) -> UsdBridgeResult<String> {
        let desc = op.apply(stage, &self.working_layer_id)?;
        self.record(op);
        Ok(desc)
    }

    pub fn undo(&mut self, stage: &UsdStage) -> UsdBridgeResult<Option<String>> {
        let Some(frame) = self.undo.pop() else {
            return Ok(None);
        };
        let layer_id = frame.layer_id().to_string();
        for op in frame
            .operations()
            .iter()
            .rev()
            .filter_map(EditOperation::inverse)
        {
            op.apply(stage, &layer_id)?;
            self.current_state.insert(op.key().clone(), op);
        }
        let label = frame.label();
        self.redo.push(frame);
        Ok(Some(label))
    }

    pub fn redo(&mut self, stage: &UsdStage) -> UsdBridgeResult<Option<String>> {
        let Some(frame) = self.redo.pop() else {
            return Ok(None);
        };
        let layer_id = frame.layer_id().to_string();
        for op in frame.operations() {
            op.apply(stage, &layer_id)?;
            self.current_state.insert(op.key().clone(), op.clone());
        }
        let label = frame.label();
        self.push_undo(frame);
        Ok(Some(label))
    }

    fn push_undo(&mut self, frame: UndoFrame) {
        self.undo.push(frame);
        self.enforce_cap();
    }

    fn enforce_cap(&mut self) {
        let overflow = self.undo.len().saturating_sub(self.cap);
        if overflow > 0 {
            self.undo.drain(0..overflow);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn xform_op(path: &str, after: f32) -> EditOperation {
        let mut mat = bif_math::Mat4::IDENTITY.to_cols_array();
        mat[12] = after;
        EditOperation::Transform {
            key: OpinionKey::new(path, AttrSlot::Xform),
            before: Some(bif_math::Mat4::IDENTITY.to_cols_array()),
            after: mat,
        }
    }

    #[test]
    fn record_clears_redo_and_tracks_current_state() {
        let mut history = EditHistory::new();
        history.record(xform_op("/World/A", 1.0));
        assert!(history.can_undo());
        assert_eq!(history.undo_len(), 1);
        assert_eq!(history.current_state_len(), 1);
    }

    #[test]
    fn group_accumulates_as_single_frame() {
        let mut history = EditHistory::new();
        history.begin_group("move two prims");
        history.record(xform_op("/World/A", 1.0));
        history.record(xform_op("/World/B", 2.0));
        assert_eq!(history.undo_len(), 0);
        history.end_group();
        assert_eq!(history.undo_len(), 1);
        assert!(matches!(history.undo[0], UndoFrame::Group { .. }));
    }

    #[test]
    fn empty_group_is_dropped() {
        let mut history = EditHistory::new();
        history.begin_group("empty");
        history.end_group();
        assert!(!history.can_undo());
    }

    #[test]
    fn cap_enforced_at_256_by_default() {
        let mut history = EditHistory::new();
        for i in 0..300 {
            history.record(xform_op(&format!("/World/A{i}"), i as f32));
        }
        assert_eq!(history.undo_len(), 256);
    }

    #[test]
    fn set_working_layer_updates_layer_id() {
        let mut history = EditHistory::new();
        history.set_working_layer("shot.usda");
        assert_eq!(history.working_layer_id, "shot.usda");
    }
}
