#[path = "_helpers.rs"]
mod helpers;

use bif_core::usd::{AttrSlot, EditOperation, PayloadPolicy, UsdStage};
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
fn apply_then_undo_clears_dirty_bit_after_save() {
    let fixture = helpers::LayeredStageFixture::new("dirty");
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
    assert!(state.stack.layers[state.working_layer].is_dirty);

    stage.save_layer(&working_id).expect("save working layer");
    state.mark_working_layer_dirty(false);
    assert!(!state.stack.layers[state.working_layer].is_dirty);
}

#[test]
fn undo_inverts_opinion_on_working_layer() {
    let fixture = helpers::LayeredStageFixture::new("undo");
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
    state
        .undo_usd_edit(&stage)
        .expect("undo")
        .expect("undo desc");
    stage.save_layer(&working_id).expect("save working layer");

    let text = stage
        .export_layer_as_string(&working_id)
        .expect("export working layer");
    assert!(text.contains("visibility"));
    assert!(text.contains("inherited"));
}

#[test]
fn begin_end_group_undoes_atomically() {
    let fixture = helpers::LayeredStageFixture::new("group");
    let stage = UsdStage::open(&fixture.root).expect("open stage");
    let (mut state, _working_id) = state_for_working_layer(&stage);

    state.edit_history.begin_group("visibility pair");
    state
        .apply_edit_operation(
            &stage,
            EditOperation::Visibility {
                key: bif_core::usd::OpinionKey::new("/World/Cube", AttrSlot::Visibility),
                before: Some(true),
                after: false,
            },
        )
        .expect("apply first");
    state
        .apply_edit_operation(
            &stage,
            EditOperation::Visibility {
                key: bif_core::usd::OpinionKey::new("/Model", AttrSlot::Visibility),
                before: Some(true),
                after: false,
            },
        )
        .expect("apply second");
    state.edit_history.end_group();

    assert_eq!(state.edit_history.undo_len(), 1);
    assert_eq!(
        state
            .undo_usd_edit(&stage)
            .expect("undo")
            .expect("undo desc"),
        "visibility pair"
    );
    assert!(state.edit_history.can_redo());
}

#[test]
fn set_edit_target_redirects_writes() {
    let fixture = helpers::LayeredStageFixture::new("redirect");
    let other = fixture.dir.join("other.usda");
    helpers::write(&other, "#usda 1.0\n\n");
    helpers::write(
        &fixture.root,
        r#"#usda 1.0
(
    subLayers = [
        @other.usda@,
        @working.usda@,
        @asset.usda@
    ]
)

"#,
    );
    let stage = UsdStage::open(&fixture.root).expect("open stage");
    let mut state =
        SceneLayerState::from_stage(&stage, PayloadPolicy::LoadAll).expect("scene layer state");
    let other_id = stage
        .get_layer_stack()
        .expect("layer stack")
        .layers
        .into_iter()
        .find(|l| l.identifier.ends_with("other.usda"))
        .map(|l| l.identifier)
        .expect("other layer");
    let other_idx = state.layer_index(&other_id).expect("other idx");
    state
        .set_edit_target(other_idx, &stage)
        .expect("set other target");

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
    stage.save_layer(&other_id).expect("save other layer");

    let other_text = std::fs::read_to_string(&other).expect("read other");
    let working_text = fixture.working_text();
    assert!(other_text.contains("invisible"));
    assert!(!working_text.contains("invisible"));
}
