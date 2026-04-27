#[path = "_helpers.rs"]
mod helpers;

use bif_core::usd::{AttrSlot, EditOperation, PayloadPolicy, ShaderValue, UsdStage};
use bif_core::SceneLayerState;

fn state_for_working_layer(stage: &UsdStage) -> (SceneLayerState, String) {
    let working_id = helpers::working_layer_id(stage);
    let mut state =
        SceneLayerState::from_stage(stage, PayloadPolicy::LoadAll).expect("scene layer state");
    let idx = state.layer_index(&working_id).expect("working layer index");
    state.set_edit_target(idx, stage).expect("set edit target");
    (state, working_id)
}

#[test]
fn transform_roundtrips_after_save_reopen() {
    let fixture = helpers::LayeredStageFixture::new("transform");
    let stage = UsdStage::open(&fixture.root).expect("open stage");
    let (mut state, working_id) = state_for_working_layer(&stage);
    let mut after = bif_math::Mat4::IDENTITY.to_cols_array();
    after[12] = 3.0;

    state
        .apply_edit_operation(
            &stage,
            EditOperation::Transform {
                key: bif_core::usd::OpinionKey::new("/World/Cube", AttrSlot::Xform),
                before: Some(bif_math::Mat4::IDENTITY.to_cols_array()),
                after,
            },
        )
        .expect("apply transform");
    stage.save_layer(&working_id).expect("save working layer");

    let reopened = UsdStage::open(&fixture.root).expect("reopen stage");
    let text = reopened
        .export_layer_as_string(&working_id)
        .expect("export working layer");
    assert!(text.contains("xformOp:transform"));
    assert!(text.contains("3"));
}

#[test]
fn visibility_roundtrips_after_save_reopen() {
    let fixture = helpers::LayeredStageFixture::new("visibility");
    let stage = UsdStage::open(&fixture.root).expect("open stage");
    let (mut state, working_id) = state_for_working_layer(&stage);

    state
        .apply_edit_operation(
            &stage,
            EditOperation::Visibility {
                key: bif_core::usd::OpinionKey::new("/World/Cube", AttrSlot::Visibility),
                before: Some(true),
                after: false,
            },
        )
        .expect("apply visibility");
    stage.save_layer(&working_id).expect("save working layer");

    let reopened = UsdStage::open(&fixture.root).expect("reopen stage");
    let text = reopened
        .export_layer_as_string(&working_id)
        .expect("export working layer");
    assert!(text.contains("visibility"));
    assert!(text.contains("invisible"));
}

#[test]
fn material_assign_roundtrips_after_save_reopen() {
    let fixture = helpers::LayeredStageFixture::new("material");
    let stage = UsdStage::open(&fixture.root).expect("open stage");
    let (mut state, working_id) = state_for_working_layer(&stage);

    state
        .apply_edit_operation(
            &stage,
            EditOperation::MaterialAssign {
                key: bif_core::usd::OpinionKey::new("/World/Cube", AttrSlot::MaterialBinding),
                before: None,
                after: "/Materials/Red".to_string(),
            },
        )
        .expect("apply material binding");
    stage.save_layer(&working_id).expect("save working layer");

    let reopened = UsdStage::open(&fixture.root).expect("reopen stage");
    let text = reopened
        .export_layer_as_string(&working_id)
        .expect("export working layer");
    assert!(text.contains("material:binding"));
    assert!(text.contains("/Materials/Red"));
}

#[test]
fn material_param_override_roundtrips_after_save_reopen() {
    let fixture = helpers::LayeredStageFixture::new("material_param");
    let stage = UsdStage::open(&fixture.root).expect("open stage");
    let (mut state, working_id) = state_for_working_layer(&stage);

    state
        .apply_edit_operation(
            &stage,
            EditOperation::MaterialParamOverride {
                key: bif_core::usd::OpinionKey::new(
                    "/Materials/Red/PreviewSurface",
                    AttrSlot::ShaderInput {
                        shader_path: "/Materials/Red/PreviewSurface".to_string(),
                        name: "roughness".to_string(),
                    },
                ),
                before: Some(ShaderValue::Float(0.2)),
                after: ShaderValue::Float(0.75),
            },
        )
        .expect("apply shader input");
    stage.save_layer(&working_id).expect("save working layer");

    let reopened = UsdStage::open(&fixture.root).expect("reopen stage");
    let text = reopened
        .export_layer_as_string(&working_id)
        .expect("export working layer");
    assert!(text.contains("inputs:roughness"));
    assert!(text.contains("0.75"));
}

#[test]
fn variant_selection_roundtrips_after_save_reopen() {
    let fixture = helpers::LayeredStageFixture::new("variant");
    let stage = UsdStage::open(&fixture.root).expect("open stage");
    let (mut state, working_id) = state_for_working_layer(&stage);

    state
        .apply_edit_operation(
            &stage,
            EditOperation::VariantSelect {
                key: bif_core::usd::OpinionKey::new(
                    "/Model",
                    AttrSlot::VariantSelection {
                        vset: "lod".to_string(),
                    },
                ),
                before: Some("low".to_string()),
                after: "high".to_string(),
            },
        )
        .expect("apply variant");
    stage.save_layer(&working_id).expect("save working layer");

    let reopened = UsdStage::open(&fixture.root).expect("reopen stage");
    assert_eq!(
        reopened
            .get_variant_selection("/Model", "lod")
            .expect("variant selection"),
        "high"
    );
    let text = reopened
        .export_layer_as_string(&working_id)
        .expect("export working layer");
    assert!(text.contains("high"));
}

#[test]
fn set_shader_id_roundtrips() {
    // C4b-3 — verify the SetShaderId variant authors `info:id` and
    // that the inverse re-authors the original id.
    let fixture = helpers::LayeredStageFixture::new("set_shader_id");
    let stage = UsdStage::open(&fixture.root).expect("open stage");
    let (mut state, working_id) = state_for_working_layer(&stage);

    let shader_path = "/Materials/Red/PreviewSurface";
    let before_id = stage
        .get_bound_shader_id("/World/Cube") // unbound, returns "" — fine
        .unwrap_or_default();
    // Apply: swap to a new id.
    state
        .apply_edit_operation(
            &stage,
            EditOperation::SetShaderId {
                key: bif_core::usd::OpinionKey::new(shader_path, AttrSlot::ShaderId),
                before: Some("UsdPreviewSurface".to_string()),
                after: "OpenPBR".to_string(),
            },
        )
        .expect("apply shader id");

    let after_text = stage
        .export_layer_as_string(&working_id)
        .expect("export after");
    assert!(after_text.contains("info:id"));
    assert!(after_text.contains("OpenPBR"));

    // Undo restores the prior id.
    state
        .undo_usd_edit(&stage)
        .expect("undo")
        .expect("undo desc");
    let restored = stage
        .export_layer_as_string(&working_id)
        .expect("export restored");
    assert!(restored.contains("UsdPreviewSurface"));
    let _ = before_id; // kept for future direct-id assertions
}

#[test]
fn shading_model_swap_undoes_atomically() {
    // C4b-3 — the begin_group/end_group dance around a shading
    // model swap must collapse multiple op records into a single
    // UndoFrame so one Ctrl+Z reverts the whole sequence.
    let fixture = helpers::LayeredStageFixture::new("shading_swap_atomic");
    let stage = UsdStage::open(&fixture.root).expect("open stage");
    let (mut state, _working_id) = state_for_working_layer(&stage);

    let shader_path = "/Materials/Red/PreviewSurface";
    state.edit_history.begin_group("swap shading model");
    state
        .apply_edit_operation(
            &stage,
            EditOperation::SetShaderId {
                key: bif_core::usd::OpinionKey::new(shader_path, AttrSlot::ShaderId),
                before: Some("UsdPreviewSurface".to_string()),
                after: "OpenPBR".to_string(),
            },
        )
        .expect("set id");
    state
        .apply_edit_operation(
            &stage,
            EditOperation::MaterialParamOverride {
                key: bif_core::usd::OpinionKey::new(
                    shader_path,
                    AttrSlot::ShaderInput {
                        shader_path: shader_path.to_string(),
                        name: "base_color".to_string(),
                    },
                ),
                before: None,
                after: ShaderValue::Color3f([0.5, 0.6, 0.7]),
            },
        )
        .expect("remap input");
    state.edit_history.end_group();

    // Two records should have collapsed into one frame.
    assert_eq!(state.edit_history.undo_len(), 1);
    state
        .undo_usd_edit(&stage)
        .expect("undo")
        .expect("undo desc");
    // Single Ctrl+Z reverted both.
    assert_eq!(state.edit_history.undo_len(), 0);
    assert!(state.edit_history.can_redo());
}

#[test]
fn bound_material_inputs_returns_shader_inputs() {
    // Fixture has /Materials/Red/PreviewSurface (UsdPreviewSurface
    // with `roughness=0.2`) but doesn't bind it. Bind on the working
    // layer, then the new C4b-1 FFI should report the surface shader
    // path + at least the roughness input.
    let fixture = helpers::LayeredStageFixture::new("material_inputs_ffi");
    let stage = UsdStage::open(&fixture.root).expect("open stage");
    let (mut state, _working_id) = state_for_working_layer(&stage);

    state
        .apply_edit_operation(
            &stage,
            EditOperation::MaterialAssign {
                key: bif_core::usd::OpinionKey::new("/World/Cube", AttrSlot::MaterialBinding),
                before: None,
                after: "/Materials/Red".to_string(),
            },
        )
        .expect("bind material");

    let (shader_path, inputs) = stage
        .get_bound_material_inputs("/World/Cube")
        .expect("get bound material inputs");

    assert_eq!(shader_path, "/Materials/Red/PreviewSurface");
    assert!(!inputs.is_empty(), "shader should have at least one input");
    let roughness = inputs
        .iter()
        .find(|i| i.name == "roughness")
        .expect("roughness input present");
    assert_eq!(roughness.type_name, "float");
    assert!(roughness.value.starts_with("0.2"));
}

/// Mirrors the surface that `Renderer::dispatch_visibility` exercises:
/// read computed visibility as `before`, build `EditOperation::Visibility`,
/// apply, save → reopen text contains the new opinion, then undo restores
/// the bit on the working layer.
#[test]
fn visibility_roundtrips_via_dispatcher() {
    let fixture = helpers::LayeredStageFixture::new("visibility_dispatcher");
    let stage = UsdStage::open(&fixture.root).expect("open stage");
    let (mut state, working_id) = state_for_working_layer(&stage);

    // Read current `before` like the dispatcher does.
    let before = stage
        .get_prim_info_by_path("/World/Cube")
        .expect("get prim info")
        .visible;
    assert!(before, "fixture cube starts visible");

    state
        .apply_edit_operation(
            &stage,
            EditOperation::Visibility {
                key: bif_core::usd::OpinionKey::new("/World/Cube", AttrSlot::Visibility),
                before: Some(before),
                after: false,
            },
        )
        .expect("dispatch visibility");

    // Apply path lands the opinion on the working layer.
    let after_text = stage
        .export_layer_as_string(&working_id)
        .expect("export after");
    assert!(after_text.contains("invisible"));

    // Undo restores the bit and re-asserts the inverse opinion.
    state
        .undo_usd_edit(&stage)
        .expect("undo")
        .expect("undo desc");
    let restored_text = stage
        .export_layer_as_string(&working_id)
        .expect("export restored");
    assert!(restored_text.contains("inherited"));
}

#[test]
fn replace_layer_contents_roundtrips() {
    let fixture = helpers::LayeredStageFixture::new("replace_layer");
    let stage = UsdStage::open(&fixture.root).expect("open stage");
    let (mut state, working_id) = state_for_working_layer(&stage);

    // Use existing visibility writer to author a valid `before`,
    // then capture text. Reset, then ReplaceLayerContents should
    // round-trip both the apply (matches `after`) and the inverse
    // (matches `before`).
    let before = stage
        .export_layer_as_string(&working_id)
        .expect("export before");

    stage
        .write_layer_visibility(&working_id, "/World/Cube", false)
        .expect("seed visibility opinion");
    let after = stage
        .export_layer_as_string(&working_id)
        .expect("export after");
    assert_ne!(before, after, "visibility seed should change layer text");

    // Reset to `before` so apply has work to do.
    stage
        .import_layer_from_string(&working_id, &before)
        .expect("reset to before");

    state
        .apply_edit_operation(
            &stage,
            EditOperation::replace_layer(working_id.clone(), before.clone(), after.clone()),
        )
        .expect("apply replace");

    let after_text = stage
        .export_layer_as_string(&working_id)
        .expect("export after");
    assert!(after_text.contains("invisible"));

    state
        .undo_usd_edit(&stage)
        .expect("undo")
        .expect("undo desc");
    let restored = stage
        .export_layer_as_string(&working_id)
        .expect("export restored");
    assert_eq!(restored.trim(), before.trim());
}
