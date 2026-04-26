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
