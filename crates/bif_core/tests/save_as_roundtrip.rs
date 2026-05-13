//! Integration coverage for the Qt File → Save As flow added in v0.16.2.
//!
//! The Qt shell's `on_save_as_to_path` invokable calls
//! `UsdStage::export_layer_as_string` and then writes the returned text
//! to disk via `std::fs::write`. A Qt-level test would need a live
//! `QApplication`, which is fragile on Windows under `cargo test`, so we
//! cover the same pipeline at the `bif_core` layer (matches the plan's
//! documented fallback in `before-i-close-out-crystalline-piglet.md`).
//!
//! Run with `--test-threads=1` after sourcing `setup_usd_env.ps1`.

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

/// Mirrors the Qt `on_save_as_to_path` flow:
///   open → edit → export_layer_as_string → std::fs::write → reopen
///
/// Confirms that the edits authored on the working layer land in the
/// Save-As destination file and are parseable as a standalone USDA stage.
#[test]
fn save_as_writes_working_layer_to_new_path() {
    let fixture = helpers::LayeredStageFixture::new("save_as_roundtrip");
    let stage = UsdStage::open(&fixture.root).expect("open stage");
    let (mut state, working_id) = state_for_working_layer(&stage);

    // Author a translate so the working layer has something to export.
    let mut after = bif_math::Mat4::IDENTITY.to_cols_array();
    after[12] = 7.5;
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

    // Qt shell path: export to string, write to user-chosen path.
    let text = stage
        .export_layer_as_string(&working_id)
        .expect("export working layer as string");
    assert!(!text.is_empty(), "exported layer should not be empty");

    let target_path = fixture.dir.join("save_as_destination.usda");
    std::fs::write(&target_path, text.as_bytes()).expect("write Save As destination");
    assert!(target_path.exists(), "destination file should exist");

    // Note: Save As is export-only — the Qt invokable never touches the
    // working-layer index. That guarantee is enforced at the Qt shell
    // layer (`on_save_as_to_path`) and is not re-asserted here because
    // `SceneLayerState::from_stage` rebuilds the working-layer index
    // from the stage rather than reading the prior state.

    // Reopen the destination as a standalone stage and verify the edit.
    let reopened = UsdStage::open(&target_path).expect("reopen Save As destination");
    let dest_root_id = reopened
        .get_layer_stack()
        .expect("dest layer stack")
        .layers
        .into_iter()
        .next()
        .map(|l| l.identifier)
        .expect("dest root layer");
    let dest_text = reopened
        .export_layer_as_string(&dest_root_id)
        .expect("re-export destination root");
    assert!(
        dest_text.contains("xformOp:transform"),
        "destination should retain xformOp:transform, got:\n{dest_text}"
    );
    assert!(
        dest_text.contains("7.5"),
        "destination should retain the translate value 7.5, got:\n{dest_text}"
    );
}

/// Empty / cancelled path → no write. The Qt invokable trims and bails
/// before reaching std::fs::write, so we just confirm the export side
/// still works when nothing is written.
#[test]
fn save_as_export_succeeds_for_minimal_edits() {
    let fixture = helpers::LayeredStageFixture::new("save_as_minimal");
    let stage = UsdStage::open(&fixture.root).expect("open stage");
    let working_id = helpers::working_layer_id(&stage);

    // No edits applied. export_layer_as_string should still produce a
    // valid USDA header so Save As never fails on an empty working layer.
    let text = stage
        .export_layer_as_string(&working_id)
        .expect("export empty working layer");
    assert!(
        text.contains("#usda"),
        "exported text must include #usda header, got:\n{text}"
    );
}
