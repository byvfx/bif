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
fn save_without_permission_returns_error() {
    let fixture = helpers::LayeredStageFixture::new("save_without_permission");
    let stage = UsdStage::open(&fixture.root).expect("open stage");
    let working_id = helpers::working_layer_id(&stage);

    stage
        .set_layer_permission_to_edit(&working_id, false)
        .expect("disable working-layer permission");

    let err = stage
        .save_layer(&working_id)
        .expect_err("read-only layer save should fail");

    stage
        .set_layer_permission_to_edit(&working_id, true)
        .expect("restore working-layer permission");

    assert!(
        err.to_string().contains("not editable"),
        "save error should name the cause, got {err}"
    );
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
fn undo_after_target_switch_targets_recorded_layer() {
    let fixture = helpers::LayeredStageFixture::new("undo_target_switch");
    let stage = UsdStage::open(&fixture.root).expect("open stage");
    let (mut state, working_id) = state_for_working_layer(&stage);
    let asset_id = stage
        .get_layer_stack()
        .expect("layer stack")
        .layers
        .into_iter()
        .find(|l| l.identifier.ends_with("asset.usda"))
        .map(|l| l.identifier)
        .expect("asset layer");

    state
        .apply_edit_operation(
            &stage,
            EditOperation::Visibility {
                key: bif_core::usd::OpinionKey::new("/World/Cube", AttrSlot::Visibility),
                before: Some(true),
                after: false,
            },
        )
        .expect("apply visibility to working layer");

    let asset_idx = state.layer_index(&asset_id).expect("asset index");
    state
        .set_edit_target(asset_idx, &stage)
        .expect("switch edit target");

    state
        .undo_usd_edit(&stage)
        .expect("undo")
        .expect("undo desc");

    let working_text = stage
        .export_layer_as_string(&working_id)
        .expect("export working");
    let asset_text = stage
        .export_layer_as_string(&asset_id)
        .expect("export asset");
    assert!(
        working_text.contains("inherited"),
        "undo should restore visibility on the originally edited layer"
    );
    assert!(
        !asset_text.contains("visibility = \"inherited\""),
        "undo must not author the inverse into the later edit target"
    );
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
fn shader_swap_input_failure_rolls_back_id() {
    let fixture = helpers::LayeredStageFixture::new("shader_swap_rollback");
    let stage = UsdStage::open(&fixture.root).expect("open stage");
    let (mut state, working_id) = state_for_working_layer(&stage);
    let shader_path = "/Materials/Red/PreviewSurface";
    let original_id = "UsdPreviewSurface";

    state.edit_history.begin_group("swap shading model");
    state
        .apply_edit_operation(
            &stage,
            EditOperation::SetShaderId {
                key: bif_core::usd::OpinionKey::new(shader_path, AttrSlot::ShaderId),
                before: Some(original_id.to_string()),
                after: "OpenPBR".to_string(),
            },
        )
        .expect("set id");

    let bad_input = EditOperation::MaterialParamOverride {
        key: bif_core::usd::OpinionKey::new(shader_path, AttrSlot::Visibility),
        before: None,
        after: ShaderValue::Float(1.0),
    };
    let result = state.apply_edit_operation(&stage, bad_input);
    assert!(result.is_err(), "bad shader input op should fail");

    state.edit_history.cancel_group();
    stage
        .set_layer_shader_id(&working_id, shader_path, original_id)
        .expect("rollback shader id");

    let text = stage
        .export_layer_as_string(&working_id)
        .expect("export working");
    assert!(text.contains("UsdPreviewSurface"));
    assert!(!text.contains("OpenPBR"));
    assert_eq!(state.edit_history.undo_len(), 0);
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

/// Full visibility toggle cycle: apply visibility=false → verify stage
/// visibility state updated → undo restores → redo hides again. Checks
/// live `get_prim_info_by_path` rather than layer text.
#[test]
fn visibility_toggle_undo_redo_state() {
    let fixture = helpers::LayeredStageFixture::new("visibility_undo_redo");
    let stage = UsdStage::open(&fixture.root).expect("open stage");
    let (mut state, _working_id) = state_for_working_layer(&stage);

    let before = stage
        .get_prim_info_by_path("/World/Cube")
        .expect("get prim info")
        .visible;
    assert!(before, "fixture cube starts visible");

    // Hide
    state
        .apply_edit_operation(
            &stage,
            EditOperation::Visibility {
                key: bif_core::usd::OpinionKey::new("/World/Cube", AttrSlot::Visibility),
                before: Some(before),
                after: false,
            },
        )
        .expect("hide");
    assert!(
        !stage
            .get_prim_info_by_path("/World/Cube")
            .expect("prim info after hide")
            .visible,
        "cube should be invisible after toggle"
    );

    // Undo → visible again
    state
        .undo_usd_edit(&stage)
        .expect("undo")
        .expect("undo desc");
    assert!(
        stage
            .get_prim_info_by_path("/World/Cube")
            .expect("prim info after undo")
            .visible,
        "cube should be visible after undo"
    );

    // Redo → hidden again
    state
        .redo_usd_edit(&stage)
        .expect("redo")
        .expect("redo desc");
    assert!(
        !stage
            .get_prim_info_by_path("/World/Cube")
            .expect("prim info after redo")
            .visible,
        "cube should be invisible after redo"
    );
}

/// USDA Apply with visibility change: replace layer contents with USDA
/// that authors `visibility = "invisible"`, verify live visibility state
/// updates, then undo restores visibility.
#[test]
fn usda_apply_visibility_undo_state() {
    let fixture = helpers::LayeredStageFixture::new("usda_visibility");
    let stage = UsdStage::open(&fixture.root).expect("open stage");
    let (mut state, working_id) = state_for_working_layer(&stage);

    assert!(
        stage
            .get_prim_info_by_path("/World/Cube")
            .expect("get prim info")
            .visible,
        "fixture cube starts visible"
    );

    let before = stage
        .export_layer_as_string(&working_id)
        .expect("export before");

    // Use the bridge writer to produce valid USDA text with visibility.
    stage
        .write_layer_visibility(&working_id, "/World/Cube", false)
        .expect("write visibility");
    let usda_with_visibility = stage
        .export_layer_as_string(&working_id)
        .expect("export after visibility write");

    // Reset to `before` so ReplaceLayerContents has work to do.
    stage
        .import_layer_from_string(&working_id, &before)
        .expect("reset to before");

    let op = EditOperation::replace_layer(working_id.clone(), before, usda_with_visibility);
    state.apply_edit_operation(&stage, op).expect("apply usda");

    assert!(
        !stage
            .get_prim_info_by_path("/World/Cube")
            .expect("prim info after usda apply")
            .visible,
        "cube should be invisible after USDA Apply"
    );

    // Undo → visible again
    state
        .undo_usd_edit(&stage)
        .expect("undo")
        .expect("undo desc");
    assert!(
        stage
            .get_prim_info_by_path("/World/Cube")
            .expect("prim info after undo")
            .visible,
        "cube should be visible after USDA undo"
    );
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

#[test]
fn replace_layer_double_apply_idempotent() {
    let fixture = helpers::LayeredStageFixture::new("replace_layer_idempotent");
    let stage = UsdStage::open(&fixture.root).expect("open stage");
    let (mut state, working_id) = state_for_working_layer(&stage);
    let before = stage
        .export_layer_as_string(&working_id)
        .expect("export before");
    stage
        .write_layer_visibility(&working_id, "/World/Cube", false)
        .expect("seed visibility opinion");
    let after = stage
        .export_layer_as_string(&working_id)
        .expect("export after");

    stage
        .import_layer_from_string(&working_id, &before)
        .expect("reset to before");

    let op = EditOperation::replace_layer(working_id.clone(), before, after.clone());
    state
        .apply_edit_operation(&stage, op.clone())
        .expect("first replace");
    state
        .apply_edit_operation(&stage, op)
        .expect("second replace");

    let text = stage
        .export_layer_as_string(&working_id)
        .expect("export final");
    assert_eq!(text.trim(), after.trim());
}

#[test]
fn import_layer_from_garbage_leaves_layer_intact() {
    let fixture = helpers::LayeredStageFixture::new("rollback");
    let stage = UsdStage::open(&fixture.root).expect("open stage");
    let working_id = helpers::working_layer_id(&stage);
    let original = stage
        .export_layer_as_string(&working_id)
        .expect("export original");

    // Garbage that the USDA parser must reject. Live layer must be byte-identical after.
    let result = stage.import_layer_from_string(&working_id, "this is not valid usda {{{ ###");
    assert!(result.is_err(), "garbage USDA should be rejected");

    let after = stage
        .export_layer_as_string(&working_id)
        .expect("export after");
    assert_eq!(
        original.trim(),
        after.trim(),
        "rejected import must leave layer unchanged"
    );
}

// ---------------------------------------------------------------------------
// v0.16.2 visibility-fix regression coverage
// ---------------------------------------------------------------------------

/// Persisted `visibility="invisible"` is toggleable on reopen. Pre-fix, raw
/// `Set("inherited")` did not defeat the prior opinion on the same prim
/// (and certainly not an ancestor's). Post-fix uses `MakeVisible()` which
/// re-authors the leaf opinion idempotently.
#[test]
fn visibility_persisted_invisible_unhides_on_reopen() {
    let fixture = helpers::LayeredStageFixture::new("vis_unhide_reopen");

    // Step 1: hide /World/Cube + save the working layer.
    {
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
            .expect("hide cube");
        stage.save_layer(&working_id).expect("save working");
    }

    // Step 2: reopen — composed visibility on /World/Cube is invisible.
    let reopened = UsdStage::open(&fixture.root).expect("reopen");
    let cube = reopened
        .get_prim_info_by_path("/World/Cube")
        .expect("cube info");
    assert!(!cube.visible, "Cube should be invisible after save+reopen");

    // Step 3: toggle visible again on the reopened stage.
    let (mut state, working_id) = state_for_working_layer(&reopened);
    state
        .apply_edit_operation(
            &reopened,
            EditOperation::Visibility {
                key: bif_core::usd::OpinionKey::new("/World/Cube", AttrSlot::Visibility),
                before: Some(false),
                after: true,
            },
        )
        .expect("show cube");

    let cube_after = reopened
        .get_prim_info_by_path("/World/Cube")
        .expect("cube info 2");
    assert!(cube_after.visible, "Cube should be visible after un-hide");

    // Step 4: save and reopen — visible persists.
    reopened.save_layer(&working_id).expect("save 2");
    let reopened2 = UsdStage::open(&fixture.root).expect("reopen 2");
    let cube_final = reopened2
        .get_prim_info_by_path("/World/Cube")
        .expect("cube info 3");
    assert!(
        cube_final.visible,
        "Cube should still be visible after second round-trip"
    );
}

/// USD visibility is pruning: an ancestor `visibility="invisible"` hides
/// the entire subtree, and a descendant `visibility="inherited"` cannot
/// un-hide it. `MakeVisible()` walks ancestors and authors `inherited` on
/// each invisible parent — un-hiding a leaf must also expose its hidden
/// ancestor chain. Without this, ALab-style scenes with persisted
/// ancestor hides become permanently invisible.
#[test]
fn visibility_unhide_defeats_ancestor_pruning() {
    let fixture = helpers::LayeredStageFixture::new("vis_unhide_ancestor");

    // Hide /World (the ancestor) and save.
    {
        let stage = UsdStage::open(&fixture.root).expect("open stage");
        let (mut state, working_id) = state_for_working_layer(&stage);
        state
            .apply_edit_operation(
                &stage,
                EditOperation::Visibility {
                    key: bif_core::usd::OpinionKey::new("/World", AttrSlot::Visibility),
                    before: Some(true),
                    after: false,
                },
            )
            .expect("hide World");
        stage.save_layer(&working_id).expect("save");
    }

    // Reopen — both World and Cube are invisible via pruning.
    let reopened = UsdStage::open(&fixture.root).expect("reopen");
    let world = reopened.get_prim_info_by_path("/World").expect("World");
    let cube = reopened.get_prim_info_by_path("/World/Cube").expect("Cube");
    assert!(!world.visible, "World should be invisible");
    assert!(
        !cube.visible,
        "Cube should be invisible via ancestor pruning"
    );

    // Un-hide the descendant; MakeVisible walks up and authors `inherited`
    // on /World as well, so both end up visible.
    let (mut state, _) = state_for_working_layer(&reopened);
    state
        .apply_edit_operation(
            &reopened,
            EditOperation::Visibility {
                key: bif_core::usd::OpinionKey::new("/World/Cube", AttrSlot::Visibility),
                before: Some(false),
                after: true,
            },
        )
        .expect("show Cube");

    let world_after = reopened.get_prim_info_by_path("/World").expect("World 2");
    let cube_after = reopened
        .get_prim_info_by_path("/World/Cube")
        .expect("Cube 2");
    assert!(cube_after.visible, "Cube visible after un-hide");
    assert!(
        world_after.visible,
        "Ancestor World also un-hidden by MakeVisible walk"
    );
}

/// `set_layer_muted(root_id, true)` must reject with an error and leave
/// the stage state unchanged. Pre-fix, USD emitted a soft TF_CODING_ERROR
/// that bif's `layer_state.muted` ignored — a phantom mute replayed on
/// every reload, corrupting composition and leaving the scene browser
/// empty until the user manually toggled the layer back.
#[test]
fn set_layer_muted_rejects_root_layer() {
    let fixture = helpers::LayeredStageFixture::new("root_mute_reject");
    let stage = UsdStage::open(&fixture.root).expect("open stage");
    let stack = stage.get_layer_stack().expect("layer stack");
    let root_id = stack
        .layers
        .iter()
        .find(|l| l.identifier.ends_with("root.usda"))
        .expect("root layer")
        .identifier
        .clone();

    let result = stage.set_layer_muted(&root_id, true);
    assert!(result.is_err(), "root-layer mute must be rejected (got Ok)");

    let stack2 = stage.get_layer_stack().expect("layer stack 2");
    let root_after = stack2
        .layers
        .iter()
        .find(|l| l.identifier == root_id)
        .expect("root layer 2");
    assert!(
        !root_after.is_muted,
        "root layer must not be marked muted after rejection"
    );
}

/// Payload-rooted asset (`def Xform (payload = @./payload.usda@)`) populates
/// `all_prims` after `load_payloads`. Pre-fix, the bridge's `cache_prim_data`
/// ran against the unloaded composition under `UsdStage::Open(LoadNone)` —
/// `UsdPrimDefaultPredicate` excludes unloaded-payload prims so the cache
/// was set with 0 entries and a `prims_cached=true` flag prevented the
/// post-payload re-cache, leaving the scene browser empty.
#[test]
fn payload_root_scene_browser_populates_after_load() {
    let id = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("bif_payload_root_{id}_{nanos}"));
    std::fs::create_dir_all(&dir).expect("create dir");

    let payload_path = dir.join("payload.usda");
    let entry_path = dir.join("entry.usda");
    helpers::write(
        &payload_path,
        r#"#usda 1.0

def Mesh "geo"
{
    point3f[] points = [(0, 0, 0), (1, 0, 0), (0, 1, 0)]
    int[] faceVertexCounts = [3]
    int[] faceVertexIndices = [0, 1, 2]
}
"#,
    );
    helpers::write(
        &entry_path,
        r#"#usda 1.0
(
    defaultPrim = "asset"
)

def Xform "asset" (
    prepend payload = @./payload.usda@
)
{
}
"#,
    );

    let stage = UsdStage::open(&entry_path).expect("open stage");
    let _ = stage.load_payloads().expect("load payloads");
    let prims = stage.all_prims().expect("all_prims");
    assert!(
        !prims.is_empty(),
        "all_prims must include the payload-rooted prim hierarchy after load_payloads (got 0)"
    );
    assert!(
        prims.iter().any(|p| p.path == "/asset"),
        "expected /asset prim in all_prims after payload load; got {:?}",
        prims.iter().map(|p| p.path.as_str()).collect::<Vec<_>>()
    );

    let _ = std::fs::remove_dir_all(&dir);
}
