# SESSION_HANDOFF Archive

> Entries older than the last 5 active sessions. Active sessions → [SESSION_HANDOFF.md](../../SESSION_HANDOFF.md)

---

# Session Handoff — 2026-05-12 (v0.16.2 close-out: GUI foundation polish)

**Last Updated:** 2026-05-12 on `v0.16.2-bugfixes`.

**Current work:** Pre-merge foundation review for v0.16.2 → main. Audited the Qt UI surface (panel inventory, TODO/stub punch list, golden-path tracing, regression risks) and landed the three foundation-polish items that were blocking a clean close-out:

1. **File → Save As wired to a real dialog.** `actions.save_as` now opens `QFileDialog::getSaveFileName` pre-filled with the active edit-target identifier (full path + filename — opens to the right directory, user can rename in place). New `on_save_as_to_path` invokable calls `UsdStage::export_layer_as_string` + `std::fs::write`; `.usda` appended when no extension. Working-layer identity unchanged. Binary `.usdc` path deferred to v0.17 with `cpp_bridge.rs` split.
2. **Help → About modal dialog.** Replaces status-bar stub with `QMessageBox::about` showing `CARGO_PKG_VERSION` + repo link. New `about_dialog_body` invokable so version can't drift from `Cargo.toml`. Status bar still gets the short line.
3. **Edit-target sync failures surfaced to status bar.** Both `on_stage_path_opened` and `set_working_layer` were `log::warn!`-and-continue on `state.set_edit_target` failure — user would see "Loaded ✓" while edits would silently land on the wrong layer. Failure now appends `⚠ edit target sync failed: …` to the load message and replaces the layer-stack double-click OK with a warning.

**Changes:**
- `crates/bif_qt/src/main_window.rs` — new `on_save_as_to_path` and `about_dialog_body` invokables; edit-target-sync sites refactored to return `Option<String>` and surface to status bar.
- `crates/bif_qt/cpp/window_builder.cpp` — Save As action rewired to open `QFileDialog` with pre-fill; Help/About action shows `QMessageBox::about`.
- `crates/bif_core/tests/save_as_roundtrip.rs` — new file. Two tests cover the open → edit → export → write → reopen contract.
- `CHANGELOG.md` — three new entries under `[Unreleased]` (Save As, Help/About, edit-target-sync).
- `MILESTONES.md` — v0.16.2 row added to Released table with deferred-to-v0.17 list.

**Validation:** `cargo build -p bif_qt`, `cargo clippy --workspace -- -D warnings`, `cargo fmt --check` all clean. `cargo test -p bif_core` 239 passed (incl. 2 new save_as_roundtrip tests under `--test-threads=1`). `cargo test -p bif_qt` 12 passed.

**Next:** Commit punch list, merge `v0.16.2-bugfixes` → main (no-ff), post-merge smoke on `test_balls.usd`, tag `v0.16.2` after smoke. Then v0.17.0 (Context System) — first tasks: `cpp_bridge.rs` split, `cache_prim_data` thread-safety, defer-GPU-upload for invisible prototypes.

**Foundation review notes (audit findings, no action this branch):**
- Golden path (launch → open → pick → inspect → edit → save) is solid end-to-end.
- File/New Stage, Recent Stages list, edge routing in node graph → all intentional Phase B stubs, deferred to v0.17.
- Viewport-not-ready silent fallbacks at three sites in `main_window.rs` — defensive; consider status-bar messaging in a future polish pass.
- Stage-mutex poison paths return silent `None` — current code is single-threaded so unreachable; no action.

---

# Session Handoff — 2026-05-12 (viewport pick → scene browser tree sync)

**Last Updated:** 2026-05-12 on `v0.16.2-bugfixes` (commit `837dfff`).

**Current work:** Dogfood pass surfaced two bugs. Shipped Bug 1; Bug 2 deferred.

**Bug 1 fixed — viewport→tree highlight sync.** Three stacked bugs in `crates/bif_qt/cpp/scene_browser_widget.cpp` + `scene_browser_model.{cpp,h}`:
1. `find_source_index_for_path` walked via column `ColName` (=1); `SceneBrowserModel::rowCount` returns 0 for `parent.column() > 0` — recursion never descended. Switched to column 0 (PathRole is column-agnostic).
2. `selected_prim_pathChanged` connect was guarded by `if (m_state)` even though the panel is always built with a state. Drop guard, add `Q_ASSERT`.
3. Initial rebuild can race with scene_browser_provider returning 0 children, locking a node as a fake-leaf (`children_populated=true` permanently). Added `SceneBrowserModel::refresh_node()` to force-repopulate on descent through a stuck node. Defensive — only acts when `node->children.empty()`.

Plus `path_matches()` helper strips `/BIF/...` synthetic prefix in either direction so loader-synthesized paths resolve against composed USD-form tree rows.

**Changes:**
- `crates/bif_qt/cpp/scene_browser_widget.cpp` — column-0 navigation, path_matches, refresh_node call, unconditional connect.
- `crates/bif_qt/cpp/scene_browser_model.h/.cpp` — new `refresh_node(QModelIndex)`.

**Validation:** `cargo build -p bif_qt`, `cargo clippy -p bif_qt -- -D warnings`, `cargo fmt --check`, `cargo test -p bif_qt` all clean. Manual repro on HumanFemale.walk.usd: viewport click on a deeply nested mesh now scrolls + highlights the tree row.

**Bug 2 deferred — render regression.** HumanFemale.walk.usd: UVs scrambled (face/arms), shoes oversized. Basket.usd + "all props" lose textures entirely. Plus "viewport selection only lets me select a couple things" — pick BVH likely polluted. Hypothesis: commit `e55b3d6` (2026-05-10, invisible-mesh skip removed) is the root cause across all symptoms. Plan: `~/.claude/plans/implementation-passes-dogfood-tests-rippling-flurry.md`.

**Next:** Bug 2 triage — bisect `e55b3d6` against HumanFemale + Basket, then chase the three hypotheses (mesh_dedup proto-id collision, faceVarying UV seam-split OOB, shoe rigid-skinning).

---

# Session Handoff — 2026-05-10 (visibility round-trip + payload-root scene browser)

**Last Updated:** 2026-05-10 on `v0.16.2-bugfixes`.

**Current work:** Three visibility / scene-browser bugs fixed:

1. Persisted `visibility="invisible"` can now be toggled visible on reopen. `usd_bridge_write_visibility` + `usd_bridge_layer_write_visibility` use `UsdGeomImageable::MakeVisible()`/`MakeInvisible()` — walks ancestors, defeats USD pruning. Loader no longer skips invisible meshes (they're prototypes now; visibility filtered at instance level via `hidden_prim_paths` + `reload_instance_visibility`).
2. Single-layer / payload-rooted USDs (test_balls.usd, ALab entry.usda) populate the scene browser on first open. `usd_bridge_load_payloads` resets `prims_cached=false` and unconditionally re-runs `cache_prim_data` after `stage->Load()`. The LoadNone open had populated `all_prims` against an unloaded composition (UsdPrimDefaultPredicate excludes unloaded-payload prims → 0 roots), and the cache flag-gated the post-payload recache into a no-op.
3. Root-layer mute attempts no longer corrupt layer state. `usd_bridge_stage_mute_layer` rejects via `SdfLayer::Find` + `SdfLayerHandle` equality compare against `GetRootLayer()`. USD's soft `TF_CODING_ERROR` was previously ignored, letting `layer_state.muted` record a phantom mute that replayed on every reload.

Plus `SceneBrowserModel` deferred-rebuild `QTimer::singleShot(0)` guarded on `m_root->children.empty()` for the "model constructed after revision bump" race.

**Changes:**
- `cpp/usd_bridge/usd_bridge.cpp` — visibility helpers, mute gate, cache recache
- `crates/bif_core/src/usd/loader.rs:213` — removed invisible-mesh skip
- `crates/bif_qt/cpp/scene_browser_model.cpp` — `<QTimer>` include + guarded deferred rebuild

**Validation:** `cargo build`, `cargo clippy -- -D warnings`, `cargo fmt --check` clean. 4 visibility + 3 mute + 1 export-visibility tests pass under `--test-threads=1`. Manual repro on `test_balls.usd` and ALab `entry.usda` confirms all three bugs fixed. Code-reviewed via `vfx-code-reviewer` + `Code Reviewer` in parallel; both must-fix items applied.

**Next:** File MILESTONES TODOs (ancestor unhide UX surface, visibility time samples, defer-GPU-upload for invisible prototypes, `cache_prim_data` thread-safety annotation). Add three missing tests.

---

# Session Handoff — 2026-05-09 (visibility toggle perf: skip full GPU rebuild)

**Last Updated:** 2026-05-09 on `v0.16.2-bugfixes`.

**Current work:** Visibility eye-icon toggles no longer trigger full `reload_working_scene()` (GPU buffer rebuilds, texture reload from disk, Embree BVH rebuild). Instead:
- `reload_after_usd_edit()` dispatches per EditOperation variant
- Visibility → `reload_instance_visibility()` (instance groups + culling only)
- Transform commit → skip entirely (local fast path handles GPU write)
- MaterialParamOverride → keep `reload_working_scene()` but skip `materials_dirty` (no texture I/O)

**Changes:**
- `crates/bif_viewport/src/scene_loader.rs` — new `reload_instance_visibility()` (~170 lines), masks visible instances from ground-truth arrays, rebuilds only multi-draw instance groups + culling
- `crates/bif_viewport/src/lib.rs` — dispatch in `reload_after_usd_edit()`, post-hit vis filter in `pick_instance_at`
- `crates/bif_viewport/src/types.rs` — `full_material_ids`, `full_purposes`, `all_prim_paths` on `SceneInstances`

**Validation:** `cargo build -p bif_viewport`, clippy, fmt clean. 162 bif_viewport tests pass. Manual smoke: visibility toggle is instant, hide→unhide works, no culling mismatch warnings.

**Next:** More dogfood testing on texture-heavy scenes. Then merge to main or cut release.
- `scene_browser_widget.cpp/h`: save/restore expanded state across model resets

**Validation:** Rust `cargo build` + `cargo fmt` clean. Manual Qt smoke confirms eye icon updates + tree doesn't collapse.

**Next:** Merge to main or cut release.

---

# Session Handoff — 2026-05-07 (visibility toggle + USDA Apply: tests + docs)

**Last Updated:** 2026-05-07 on `v0.16.1-followups`.

**Current work:** Added regression tests for visibility toggle and USDA Apply undo/redo state verification, updated BUGLIST / CHANGELOG / SESSION_HANDOFF docs to reflect the dogfood fixes committed on 2026-05-05 (`b69f5e5`). The viewport refresh pipeline (`reload_after_usd_edit`, `refresh_usd_visibility_state`, eye glyph in scene browser, removed Property Inspector checkbox, Qt undo/redo revision bumps) is already committed and verified.

**Validation:** `cargo build`, `cargo clippy -- -D warnings`, `cargo fmt --check`. New tests: `visibility_toggle_undo_redo_state`, `usda_apply_visibility_undo_state` in `edit_op_roundtrip.rs`. Manual Qt smoke pending.

**Known limitation:** Geometry-changing USDA Apply (new prims, different meshes) requires a new `extract_scene_from_stage` function to re-extract geometry from the in-memory USD stage — deferred to v0.17.0. Currently USDA Apply only refreshes visibility/material state; structural changes to prims are invisible until a full stage reload.

**Next action:** Manual Qt smoke on dogfood scene (eye toggle, USDA Apply, undo, redo). Then merge to main or cut release.

---

# Session Handoff — 2026-05-05 (dogfood viewport edit refresh)

**Last Updated:** 2026-05-05 on `v0.16.1-followups`.

**Current work:** Dogfood edit repair is ready to commit on the real `G:\__projects\_programming\rust\bif` checkout. The Qt scene browser now has a dedicated fixed-width visibility column; the Prim/name column keeps tree expansion and row selection. The Property Inspector visibility checkbox is removed.

**Edit refresh path:** `Renderer::reload_after_usd_edit()` now handles post-edit viewport refresh for USD edits, undo, redo, material params, material binding, shading swaps, visibility, and USDA Apply. It refreshes hidden prim state, syncs material data from the live USD stage, marks materials dirty, and reloads the working scene. The USD bridge invalidates caches after layer import.

**Validation:** `. .\setup_qt_env.ps1; . .\setup_usd_env.ps1; cargo build`, `cargo clippy -- -D warnings`, `cargo fmt --check`, `cargo test -p bif_viewport`, and `cargo test -p bif_core -- --test-threads=1` all pass on `G:`. The `bif_core` suite still emits noisy expected USD diagnostics during negative USDA parse/xform tests, but exits green.

**Left unstaged intentionally:** `test_assets/scene/root.usda` is modified as an LFS pointer and looks like dogfood/manual-save dirt. Do not commit it unless a fixture update is intentional.

**Next action:** Manual Qt smoke on the dogfood scene: eye toggle, material params, material bind, shading swap, USDA Apply, undo, redo. Then design the material binding/node-graph workflow before expanding the material UI.

---

# Session Handoff — 2026-05-03 (viewport fixes + test fixtures)

**Last Updated:** 2026-05-03. Three commits on `v0.16.1-followups`:

1. **UNC path fix** — `cpp/usd_bridge/usd_bridge.cpp`: guard `std::replace` at both path-normalization sites to skip `\\server\share\...` paths. USD's AR on Windows cannot resolve `//server/share/...` forward-slash UNC form.
2. **Comprehensive USD test fixtures** — `test_assets/comprehensive.usda` (sublayer root + comp_overrides/comp_base) and `test_assets/scene/` (root/anim/shot_overrides/geo). Full feature coverage: variants, UsdPreviewSurface network, lights, camera, mesh primvars, PointInstancer, collections, timeSamples.
3. **`primvars:displayColor` Vulkan fallback** — `MeshData` now carries `display_color`; `scene_loader.rs` synthesizes a flat-color `bif_core::Material` per prototype at the sentinel injection site, appended after the default-grey slot in the GPU material table.

**MILESTONES**: v0.16.5 bullet added for displayColor; Backlog item added for animated xformOp freeze.

**Known gaps (tracked):** Cylinder/Capsule/Cone tessellation (backlog), animated xformOp frozen at default time (backlog), `TallSpire` Cylinder in PointInstancer still invisible.

**Validation:** `cargo build -p bif_viewport` clean (13.98s). `cargo fmt --check` clean. Build/clippy on full workspace require Qt in PATH — not available in this shell; all touched crates are pure Rust except bif_qt.

**Next action:** v0.16.1 dogfood pass (HiDPI visual, visibility toggle redesign, USDA Apply fix). Then v0.16.5 Graphite styling pass.

---

# Session Handoff - April 30, 2026 (`v0.16.1-followups` review fixes)

**Last Updated:** 2026-04-30 (post review). `vfx-code-reviewer` audited the eight v0.16.1-followups commits and produced a Critical/Major/Minor punch list. Four review action items resolved on top of the sweep:

- **C3** `cpp_bridge.rs:2264-2265` — `get_prim_attributes` now uses `cstr(prim_path)?` instead of the manual `CString::new` + `InvalidPrim("invalid path")` that was missed by the centralization refactor.
- **C2** `tests/ffi_contract.rs` — added `layer_save_error_message_pointer_is_reused_per_thread` to lock the layer-save `error_buf` thread-local same-thread invalidation contract (mirrors the existing export-buffer test).
- **M3** `crates/bif_qt/src/main_window.rs:78, 102, 1534, 2200` — extended stage-mutex poison logging to the four remaining silent `.lock().ok()` chains via `inspect_err`.
- **M5** Inspector lambda guards — confirmed false alarm; all six `state->on_set_material_param` lambdas plus the shading-model `QTimer::singleShot` deferral already had `if (!state) return;`.

**Validation:** `cargo fmt --check` clean on the three touched files. `cargo clippy -p bif_core -p bif_qt --tests -- -D warnings` reports zero hits on touched files (44 pre-existing `useless_vec` errors in `crates/bif_core/src/usd/ffi_convert.rs` are unrelated rustc 1.92 stricter rules). `cargo test -p bif_core --test ffi_contract --test edit_op_roundtrip -- --test-threads=1` green at 17 tests including the new `error_buf` contract test. `cargo test -p bif_core --lib usd::` 119 / 119.

**Current state:** Release prep, merge, tag, and push are not done. Manual dogfood smoke and synthetic HiDPI visual verification are still pending. M4's scale_factor=2.0 dogfood remains the gating manual check before v0.16.1 release prep.

**Next action:** the long-list dogfood pass against `cargo run -p bif_viewer`, including the synthetic HiDPI visual verification. Then v0.16.1 release prep, merge, tag, push.

---

# Session Handoff — April 28, 2026 (v0.16.0 shipped on main)

**Last Updated:** 2026-04-28. v0.16.0 ship-closeout: Code Reviewer agent ran on `finish-qt-ui` (16 commits, ~7.7k LOC). Two real ship-blockers fixed in `03e9622`: USDA Apply now snapshots-and-rolls-back if `TransferContent` throws mid-mutation, and all new C-ABI entry points have `catch (...)` so non-`std::exception` USD throws can never unwind across the FFI boundary. New regression test `import_layer_from_garbage_leaves_layer_intact` locks the rollback contract. Workspace bumped to `0.16.0`. CHANGELOG `[Unreleased]` promoted to `[0.16.0] - 2026-04-28`. MILESTONES table now lists v0.16.0 as shipped 2026-04-28; `Latest release` line + CLAUDE.md status both bumped.

**Validation:** `cargo fmt --check`, `cargo clippy --all -- -D warnings`, and `cargo build --all` clean. Test suite green: `bif_math` 74, `bif_renderer` 109, `bif_viewport` 157, `bif_core` 236 lib + 4 + 11 integration (single-threaded with `setup_usd_env.ps1`). `bif_viewer` has no in-tree tests by design. New rollback test confirmed running.

**Next action:** push `main` + annotated `v0.16.0` tag (push not done by autonomous session — local tag pending). Then move to v0.16.5 Graphite styling pass and the v0.16.1 follow-up bug list (TfErrorMark on save, atomic shader-swap composite op, QPointer captures in property_inspector lambdas, modal-reentrancy guards, mutex-poison logging, additional negative tests).

## 🏁 2026-04-28 — v0.16.0 ship closeout

- **Code Reviewer pass on `finish-qt-ui`.** Triaged into 5 BLOCKER / 7 MAJOR / 6 NIT findings. Verified by reading the actual code and downgraded paranoid blockers (thread_local contract is documented and copy-on-receive in Rust; outline-color expect on a hardcoded literal; `ReplaceLayerContents` correctly ignores `working_layer_id` in favor of the recorded layer id — adding the suggested debug_assert would have broken legitimate cross-layer redo). Real ship-blockers: TransferContent rollback + FFI exception hardening. Real majors: tracked in BUGLIST for v0.16.1.
- **TransferContent rollback (`usd_bridge.cpp:6680+`).** `usd_bridge_layer_import_from_string` now `ExportToString`s the live layer into a `std::string` snapshot before `TransferContent`. If transfer throws, restore from the snapshot. Inner `try { ... } catch (...)` so non-`std::exception` USD throws are caught.
- **`catch (...)` on all new C-ABI entries.** Added to `_layer_save`, `_layer_export_as_string`, `_layer_import_from_string`, `_parse_usda`, `_layer_get_attr_value`, `_layer_permission_to_edit`, `_layer_set_permission_to_edit`, `_layer_set_shader_input`, `_layer_set_shader_id`, `_prim_get_bound_shader_id`, `_prim_get_bound_material_inputs`. Matches the `catch (...)` pattern used by older entries (e.g. `set_variant_selection`).
- **New regression test.** `import_layer_from_garbage_leaves_layer_intact` (in `tests/edit_op_roundtrip.rs`) feeds garbage USDA, asserts the call returns `Err`, and asserts the layer text is byte-identical after — confirms the parse-first guard plus the rollback contract.
- **BUGLIST followups for v0.16.1.** Captured: TfErrorMark on save, atomic shader-swap composite op, QPointer captures in property_inspector lambdas, modal-reentrancy guards on QMessageBox::question, mutex-poison logging, additional negative tests, `cstr(s)` helper in `cpp_bridge.rs`, thread_local contract test.

---

# Session Handoff — April 27, 2026 (`finish-qt-ui` C4b editor features shipped)

**Last Updated:** 2026-04-27. Branch `finish-qt-ui` shipped the full C4b editor tranche on top of C4a/dogfood. The editor now exposes per-prim Visibility (Property Inspector header checkbox), Material binding + per-input editors grouped by OpenPBR/UsdPreviewSurface section (Material Sheet tab with sRGB→linear color picker), wholesale layer USDA edits (`View → USDA Source` dock with Apply-only validation), and shading-model swap (Material Sheet header `QComboBox` with atomic-undo + lossy-param `QMessageBox`). All edits route through `EditHistory` and save through Ctrl+S.

**Validation:** `cargo build` / `cargo clippy --workspace -- -D warnings` / `cargo fmt --check` clean. `cargo test -p bif_core --test edit_op_roundtrip --test edit_history -- --test-threads=1` green at 10 tests. New round-trip coverage includes `replace_layer_contents_roundtrips`, `visibility_roundtrips_via_dispatcher`, `bound_material_inputs_returns_shader_inputs`, `set_shader_id_roundtrips`, `shading_model_swap_undoes_atomically`. Manual `usdview` reopen of a Ctrl+S output remains the recommended human-in-the-loop sanity check; not run from the autonomous session.

**Next action:** v0.16.5 Graphite styling pass — the function-first work is complete. v0.17 picks up `cpp_bridge.rs` split, payload policies, file-watcher save-conflict prompt, line/col in USDA parse errors, real-time USDA parse, drag-drop material binding, lookdev orb, and `usd_bridge_layer_clear_attr`.

## 🏁 2026-04-27 — `finish-qt-ui` C4b editor features

- **C4b-Carry-2 (`4dc7392`):** New `EditOperation::ReplaceLayerContents` + `AttrSlot::LayerContents`. Round-trip test covers apply → undo restoring captured `before` text.
- **C4b-Carry-1 (`53b8b53`):** `Renderer::dispatch_visibility` + `on_set_visibility` qinvokable + `Visible` checkbox in the Property Inspector header.
- **C4b-1 (`ca31587`):** New FFI `usd_bridge_prim_get_bound_material_inputs` + safe wrapper. `Renderer::dispatch_material_param_override` / `dispatch_material_assign`. Material Sheet tab grouped by OpenPBR / UsdPreviewSurface section with per-type editors and sRGB→linear at the color picker boundary. `Bind…` header action.
- **C4b-2 (`8455b8e`):** `View → USDA Source` dock with `QPlainTextEdit` and Apply button. New `Renderer::dispatch_replace_layer_contents` validates via `parse_usda` then routes through `ReplaceLayerContents`. Red status label surfaces parse / dispatch errors. New `usda_panel_widget.{h,cpp}` registered in `build.rs`.
- **C4b-3 (`622db71`):** New FFI `usd_bridge_layer_set_shader_id` + `usd_bridge_prim_get_bound_shader_id`. `EditOperation::SetShaderId` variant + `AttrSlot::ShaderId` slot. `dispatch_swap_shading_model` wraps id swap and best-effort param remap (`base_color` ↔ `diffuseColor`, `specular_roughness` ↔ `roughness`, etc.) in `begin_group` / `end_group`. Material Sheet header `QComboBox`. `QMessageBox` lossy-param warning.
- **C4b-4 doc sync:** `CHANGELOG.md`, `MILESTONES.md`, `FEATURES.md`, `SESSION_HANDOFF.md`, devlog.

---

## 🏁 2026-04-27 — `finish-qt-ui` C4a dogfood transform gizmo

- **Qt can move selected prims again.** `RenderWidget` forwards hover/primary-drag/release events to Rust; the renderer owns gizmo hit testing, drag preview, and release commit.
- **Selection resolves real and synthetic paths.** Viewport and tree selection now normalize `/BIF/.../<instance>` paths and parent/mesh-child paths before resolving a movable instance.
- **Working-layer transform authoring survives real USD xform ops.** The bridge writes matrices to transform ops, vectors to translate ops, and adds a transform op only when no compatible op exists.
- **Rendering validation issue fixed.** `outline.wgsl` padding now matches the Rust uniform layout, avoiding the wgpu 32-vs-48 byte validation panic.
- **Dogfood state.** User confirmed the gizmo appears and movement works; `Ctrl+S` remains working-layer-only from C4a.

---

# Session Handoff — April 26, 2026 (`finish-qt-ui` C4a edit foundation)

## 🏁 2026-04-26 — `finish-qt-ui` C4a edit foundation

- **Edit history exists in core.** `bif_core::usd::edit_history` owns `EditOperation`, `EditHistory`, `OpinionKey`, `AttrSlot`, `ShaderValue`, and grouped USD undo frames.
- **Working-layer FFI writes are wired.** The USD bridge can save, parse, export/import layer text, read layer-specific attr values, and author transform/visibility/material/shader-input/variant opinions on the selected layer without caching `SdfLayer*` in Rust.
- **Viewport dispatch now bridges instance identity to USD identity.** Transform edits and variant selections route through `EditHistory`; procedural undo remains parallel behind the viewport action router.
- **Ctrl+S now saves the working layer.** `on_save` resolves the active layer id, calls `save_layer`, clears dirty state in shell + viewport mirrors, and reports `Saved <id>` / `Save failed: ...`.
- **Docs are resynced.** `BIF_USD_WORKFLOW.md`, milestones, roadmap, feature notes, ADR-008, and the prior C4 handoff now reflect C4a/C4b split and remove the doc-only claims flagged by the audit.

## 🏁 2026-04-25 — `finish-qt-ui` C3 navigation

- **Camera navigation is now surfaced in the menu bar.** `View → Look Through…` reuses the same camera list as the breadcrumb picker, routes every selection through `on_select_camera`, and stays aligned with stage camera refreshes.
- **Orthographic switching is now a first-class Qt action.** A new View-menu toggle switches into the existing aspect-correct ortho path and back to perspective without adding a parallel camera code path.
- **Workspace presets now drive behavior, not just dock visibility.** The shell now uses `Assembly / Lighting / Materials / Review`, persists the active preset, restores older saved `"render"` state as `Review`, and applies distinct default dock/tab emphasis per workspace.
- **Workspace changes now own payload policy.** `BifShellState` stores the current payload policy, stage open/reload paths thread it through the core loader and viewport loader, and policy-changing workspace switches confirm before reloading an already-open stage.

---

# Session Handoff — April 23, 2026 (`finish-qt-ui` C2 quick wins landed)

**Last Updated:** 2026-04-23. Branch `finish-qt-ui` now has the C2 quick-wins tranche from `docs/agent-handoffs/2026-04-22-finish-qt-ui.md` landed: Render Settings now drives selection-outline width/color, `.usd*` files can be dropped onto the Qt shell to open stages, Property Inspector rows expose the full opinion stack as a rich tooltip, and Ivar render/status is surfaced in both Render Settings and the new Render menu/status bar. Automated validation is green on the current tree: `cargo build`, `cargo clippy -- -D warnings`, `cargo fmt --check`, `cargo test -p bif_math`, `cargo test -p bif_renderer`, `cargo test -p bif_viewport`, `cargo test -p bif_viewer`, `cargo test -p bif_qt`, and `cargo test -p bif_core -- --test-threads=1`. The `bif_core` suite still emits the known noisy USD secondary-thread diagnostics after completion, but the suite itself passes.

**Next action:** start C3 navigation on `finish-qt-ui`: surface a `View → Look Through…` camera action, finish the ortho/workspace navigation path, and wire workspace-specific payload policy handling with a confirm dialog when a stage is already loaded.

## 🏁 2026-04-23 — `finish-qt-ui` C2 quick wins

- **Selection outline controls are live.** Render Settings now binds directly to shell qproperties/invokables for outline width and color, and the renderer/shader path now uses an `OutlineParams` uniform instead of a hard-coded WGSL constant.
- **Stage open is easier to hit.** Dragging a `.usd`, `.usda`, `.usdc`, or `.usdz` file onto the Qt shell routes through the same `trigger_open_stage` flow as the File menu and recent-stage surfaces.
- **Property inspection exposes composition context.** Attribute rows now show a rich HTML tooltip that preserves the raw USD attribute name and enumerates the full opinion stack with winning-layer emphasis and layer-color markers.
- **Ivar render/status is surfaced as a first-class Qt action.** Render Settings adds an `Ivar Render` button and live status label, the menu bar adds a Render menu entry, and the status bar mirrors in-progress state off the existing frame pump.
- **Regression coverage expanded with the quick wins.** Qt tests now cover outline-color conversion and opinion-tooltip HTML escaping, and the renderer default outline color matches the live shell conversion path.

---

# Session Handoff — April 23, 2026 (`finish-qt-ui` C1 foundations ready)

**Last Updated:** 2026-04-23. Branch `finish-qt-ui` now has the C1 foundations tranche from `docs/agent-handoffs/2026-04-22-finish-qt-ui.md` ready to land: Qt Edit menu undo/redo actions are wired to the live `bif_core::UndoStack`, edit-target layer picking now uses real USD `SdfLayer::PermissionToEdit()`, and the node graph dock stays hidden by default behind a persisted experimental toggle. Automated validation is green in a Qt/USD-ready shell: `cargo build`, `cargo clippy -- -D warnings`, `cargo fmt --check`, `cargo test -p bif_math`, `cargo test -p bif_renderer`, `cargo test -p bif_viewport`, `cargo test -p bif_viewer`, `cargo test -p bif_qt pick_strongest_writable_sublayer_skips_locked_layers`, and `cargo test -p bif_core -- --test-threads=1`. The `bif_core` suite still emits the known noisy USD secondary-thread diagnostics after completion, but the suite itself passes.

**Next action:** commit the C1 foundations tranche on `finish-qt-ui`, then continue C2 quick wins: outline width/color controls, drag-and-drop stage open, property-row opinion tooltip, and the Ivar render trigger/status path.

## 🏁 2026-04-23 — `finish-qt-ui` C1 foundations

- **Undo/redo surfaced in the Qt shell.** `BifShellState` now mirrors `can_undo` / `can_redo`, Edit menu actions call renderer undo/redo, and action enable state refreshes off the viewport frame pump plus stage open/close transitions.
- **Writable-layer detection is now real.** `LayerInfo` carries a USD-backed `permission_to_edit` bit from the C++ bridge, and `pick_strongest_writable_sublayer` now skips locked sublayers instead of relying on the old anonymous-layer heuristic.
- **Node graph stays in-tree but hidden by default.** The dock is still present, but it only reappears when the experimental preview toggle is enabled and persists through QSettings.
- **Regression coverage added for the new foundations.** Core USD tests now assert the reported `PermissionToEdit` state, and Qt tests cover the writable-layer picker when a locked layer sits above a writable sublayer.

---

# Session Handoff — April 22, 2026 (v0.15.0 shipped on main)

**Last Updated:** 2026-04-22. v0.15.0 ship-closeout landed on `main`: property inspector stack caching, ortho/timeline/demo-tree cleanup, lazy scene-browser `fetchMore`, release docs bump, and CHANGELOG split. Agent-config work from 2026-04-22 ships inside the v0.15.0 release notes. Workspace/package version is `0.15.0`; release validation is green in a Qt/USD-ready shell; next active milestone is v0.16.0. The deferred Obsidian Graphite / "Quiet Confidence" styling pass is now explicitly split into a follow-on `v0.16.5` docket so the editor tranche stays function-first.

**Next action:** start v0.16.0 kickoff work on Edit Operations + Save, keeping Graphite styling work parked in `v0.16.5`. If publishing this ship state externally, push `main` plus the annotated `v0.15.0` tag and confirm the Pages deploy succeeds on the main-branch push.

## 🏁 2026-04-22 — v0.15.0 ship closeout

- **Cleanup batch landed on `main`.** Property inspector composition arcs now read from a selected-prim stack cache instead of re-querying the prim stack per row. Scene browser now lazy-loads one level at a time via `fetchMore`, and the demo tree no longer reappears after a real stage has been loaded and then closed.
- **Playback/viewport polish.** Orthographic preset switches now refresh aspect from the live viewport rect before updating the camera. Timeline playback stops ticking while the app is minimized/inactive and resumes cleanly when the window becomes active again.
- **Release docs updated.** `CLAUDE.md` now points at v0.16/v0.17+, `CHANGELOG.md` promotes the prior `[Unreleased]` block to `## [0.15.0] - 2026-04-22`, and the main roadmap docs (`README.md`, `MILESTONES.md`, `ROADMAP_DETAIL.md`) now treat v0.15.0 as shipped instead of "merge pending."
- **Remaining external follow-up.** `main` and the local `v0.15.0` tag still need to be pushed, and Pages still needs remote verification after that push.

---

# Session Handoff — April 22, 2026 (shared agent-config layer + Codex skill path)

**Last Updated:** 2026-04-22. New branch `agent-config-unification` establishes a repo-owned shared agent-config layer: canonical docs now live under `agents/` (project guidance, handoff contract, roles, workflows), Claude/Kilo wrappers are thin adapters, plans/handoffs now have repo homes under `docs/agent-plans/` and `docs/agent-handoffs/`, and Codex has a repo-generated `bif-commit` skill artifact plus installer path. Checks for this branch are clean **when the Qt env is loaded** (`. .\setup_qt_env.ps1; cargo build`, `. .\setup_qt_env.ps1; cargo clippy -- -D warnings`, `cargo fmt --check`). Unrelated artifacts remain intentionally uncommitted: `assets/screenshots/bif_ui_15.png`, `assets/screenshots/bif_ui_15_timeline.png`, `review/`.

**Next action:** commit the branch-scoped agent-config changes, then merge `agent-config-unification` → `main`. After merge, run `pwsh -File scripts/install-codex-skills.ps1` from a clean checkout if you want the repo-owned Codex `bif-commit` skill available globally.

## 🏁 2026-04-22 — shared agent-config layer + Codex skill path

- **Canonical shared source added under `agents/`.** `agents/base/PROJECT.md`, `agents/base/HANDOFF_CONTRACT.md`, shared roles, shared workflows, a Codex `bif-commit` walkthrough, and a lightweight generated Codex artifact area now live in-repo.
- **Claude and Kilo wrappers slimmed.** `.claude/*` and `.kilo/*` workflow/agent files now point at canonical docs under `agents/` instead of carrying full repo logic inline.
- **Plan/handoff repo paths established.** Architecture plans live in `docs/agent-plans/`; execution-ready handoffs live in `docs/agent-handoffs/`. Claude gets a `/save-plan` command that saves plans into those repo paths.
- **Sync/install scripts added.** `scripts/sync-agent-config.ps1` regenerates wrappers and repo-local Codex artifacts. `scripts/install-codex-skills.ps1` copies repo-generated Codex skills into `~/.codex/skills`.
- **Validation result.** `sync-agent-config.ps1` is idempotent on the current branch, and the repo-generated Codex `bif-commit` artifact dry-runs clean. Build/clippy initially failed only because Qt was not loaded in the shell; rerunning with `setup_qt_env.ps1` fixed that immediately.

---

# Session Handoff — April 21, 2026 (polish batch + site regen — v0.15-qt ready for merge)

**Last Updated:** 2026-04-21. Option 2 polish batch landed: `on_about` uses `BIF_QT_VERSION` (was hardcoded `v0.14.0 (Phase B shell)`), bare `env_logger::init()` removed from `bif_viewer/src/main.rs` (was shadowing `bif_qt::run`'s tuned wgpu filter), `bif_qt_spike` crate deleted (ADR-006 shipped, gate passed), orphan `on_open_stage` invokable deleted (zero C++ callers). Site regenerated via `scripts/generate-site.sh` — touched `SUMMARY.md` + `reference/changelog.md`. **Site deploy root cause identified:** `.github/workflows/pages.yml` is gated to `branches: [main]`, so feature branches never trigger Pages builds. Site will update on merge. **Next action:** merge `v0.15-qt` → `main` with `--no-ff` (preserves the 40+ phase-by-phase commit history for the Qt migration). Remaining review items (property inspector O(N²) cache, ortho aspect + QTimer gating + demo-seed guard, optional scene browser lazy fetchMore) stay as post-merge cleanup on `main`.

## 🏁 2026-04-21 — polish batch + site regen

- **About string** — `crate::BIF_QT_VERSION` const replaces hardcoded `v0.14.0 (Phase B shell)`.
- **env_logger dedup** — bare `init()` removed from `bif_viewer/src/main.rs`; env_logger dep dropped from `bif_viewer/Cargo.toml`. Call-site comment documents why the init must live in `bif_qt::run()` alone (the bare one silently wins and drops the tuned wgpu filter).
- **`bif_qt_spike` crate deleted** — workspace member removed, directory `rm -rf`'d, `setup_qt_env.ps1` doc comment updated. ~600 LOC + full wgpu/cxx-build/qt-build-utils double-compile gone from every build.
- **Orphan `on_open_stage` deleted** — zero C++ call sites; live path is `on_stage_path_opened`. Declaration + impl removed.
- **Site regen** — devlog mirror + USD docs mirror re-copied; `SUMMARY.md` + `reference/changelog.md` updated.
- **Pages workflow investigation** — `pages.yml` filters `branches: [main]`, so no feature-branch deploys. Site updates post-merge.

## Merge instructions (next action)

```bash
git checkout main
git pull
git merge --no-ff v0.15-qt -m "Merge branch 'v0.15-qt' — Qt migration + review blockers + polish batch"
git push
# Pages workflow fires on main push → site rebuilds + deploys
```

Don't squash — the 40+ phase-by-phase commits are useful context for future debugging of the Qt migration.

---

# Session Handoff — April 20, 2026 (v0.15-qt PUSHED — review blockers + dogfood fixes live)

**Last Updated:** 2026-04-20 end-of-day. **`v0.15-qt` is pushed to `origin`** (tracking `origin/v0.15-qt`, PR URL https://github.com/byvfx/bif/pull/new/v0.15-qt). Two sessions' work shipped: review blockers 1a + 1b + dogfood pick-path fix + LOD toggle restoration + FEATURES note for selection-outline polish. Dogfood results: pick now returns real paths + tree highlights, mute works on real stages, teardown stable; selection outline IS drawn (`outline.wgsl` back-face normal-expanded silhouette, labeled "Selection Outline Pipeline" at `bif_viewport/src/lib.rs:632` but stored in var named `wireframe_pipeline`) but too subtle for the default docked-viewport size (`OUTLINE_SIZE=0.004` NDC → ~1.6px in an 800px viewport). Deferred to FEATURES.md as a "promote to uniform + spinbox in Render Settings" followup rather than chased tonight.

**Next session priorities** (pick order — none of them block each other):
1. **Commit 2 — housekeeping batch.** Delete `bif_qt_spike` crate (self-labelled "delete after ADR-006", ADR-006 merged), remove duplicate `env_logger::init()` in `bif_viewer/src/main.rs` (bare `init()` shadows the tuned wgpu filter in `bif_qt::run()`), fix stale `"v0.14.0 (Phase B shell)"` About string → use `BIF_QT_VERSION` const, rename Phase B `on_new_stage`/`on_open_stage` stubs to `*_stub` so they don't shadow the live `on_stage_path_opened`. All low-risk text/structural edits.
2. **Commit 3 — property inspector O(N²) snapshot cache.** `main_window.rs:1547+` invokables re-lock the stage + re-walk attributes per Qt paint cycle; for a 200-attr prim that's ~800 stack walks per repaint. Cache `Vec<AttrInfo>` on `selected_prim_pathChanged`; invalidate on `layer_state_revision`. Pattern mirrors the existing `scene_layer_state` mirror.
3. **Commit 4 — ortho aspect + QTimer gating + demo-seed guard.** `bif_viewport/src/lib.rs:1233` — `apply_ortho_view` ignores viewport aspect; fix with `width = size * aspect`, frame from scene AABB. `render_widget.cpp:60` — gate `m_tick.start()` on `viewport_on_surface_ready` returning `true` (silent-black-viewport bug on adapter init failure). `main_window.rs:1061` + `scene_browser_model.cpp:147` — gate demo seed behind `has_stage` runtime check so first-launch looks empty until a real stage loads.
4. **Commit 5 (optional)** — scene browser lazy `fetchMore` + per-parent cache. Full virtualization deferred to Phase E.3 proper.
5. **FEATURES.md followup** — selection outline width knob + color.

**Still untracked locally (not pushed):** `assets/screenshots/bif_ui_15.png`, `bif_ui_15_timeline.png`, `review/v0.15-qt-vfx.md`, `review/v0.15-qt-general.md`. Session artifacts — commit-or-ignore decision deferred to user.

**Plan file for the 5-commit sequence:** `C:\Users\brandon\.claude\plans\v015-qt-consolidated-fixup-plan.md`. Re-read at start of next session before jumping in.

## 🏁 2026-04-20 end-of-day — commits pushed on v0.15-qt

Stack (all pushed to origin):

| Commit | Change |
|---|---|
| `b4f080e` | docs: FEATURES — selection outline knob followup |
| `e68eac5` | fix(qt): pick path alignment + restore LOD toggle (Ctrl+L) |
| `89e0b04` | fix(qt): v0.15-qt review blocker 1b — layer mute wired through renderer |
| `70dde20` | fix(qt): v0.15-qt review blockers 1a — selection sync + teardown + timeline clamp |
| `f532f91` | docs: camera-picker changelog + devlog + wiki |

## 🏁 2026-04-20 earlier — dogfood fix (pick path alignment + LOD toggle) — commit `e68eac5`

- **Selection fix.** `bif_qt/src/main_window.rs::on_prim_pick` → `r.scene.instances.prim_paths.get(idx)` (was `r.scene.working_scene.instances().get(idx).prim_path`). Early-return + `log::warn!` when `raw.is_empty()` so we see stage/pick desync in the logs instead of a silent empty status. Added `log::debug!` showing `idx`, `raw`, denormalized `path`, `type_name` on every hit.
- **LOD toggle.**
  - Rust: `lod_enabled: bool` field on `BifShellStateRust` (default `true`, matches `DisplaySettings::default`); `#[qproperty(bool, lod_enabled)]`; `#[qinvokable] on_set_lod_enabled(bool)` implementation updates qprop first then mirrors onto `Renderer::display_settings.lod_enabled` via `with_viewport_mut` (the `display_settings` field is already `pub` on `Renderer` — no new renderer API).
  - C++: `QAction* toggle_lod` added to `MenuActions`. View menu gets "Viewport &LOD" (Ctrl+L, checkable, default-checked). `QAction::toggled` → `shell_state->on_set_lod_enabled(checked)`. Command palette: "View: Toggle Viewport LOD".
- **Non-obvious bit.** Pick indices align with `scene.instances.prim_paths`, not `scene.working_scene.instances()`. The split looks symmetric; it's not — first-principles reading misses this. When wiring selection in the future, copy the lookup from `Renderer::select_at_screen` directly.
- **Selection outline investigation.** Confirmed the outline IS drawn (see `bif_viewport/src/shaders/outline.wgsl` + pipeline at `lib.rs:632`), but `OUTLINE_SIZE=0.004` NDC constant renders ~1.6px in an 800px docked viewport — effectively invisible. Filed to FEATURES.md for proper uniform + slider treatment. Commit `b4f080e`.

---

# Session Handoff — April 19, 2026 (v0.15-qt review pass + Commits 1a + 1b shipped)

**Last Updated:** 2026-04-19 late evening. Third review pass on v0.15-qt (40+ unpushed commits) — two reviewer subagents + a behavior-focused manual pass. Agents found 0 blockers / 8 Major / 12 Minor / 6 Nits (scaffolding + hygiene). Manual pass found 3 real blockers (selection fork, mute UI-only, teardown leak) + 2 Medium + 1 Low — behavioral gaps the agents missed because they never exercised cross-panel sync. Consolidated plan at `C:\Users\brandon\.claude\plans\v015-qt-consolidated-fixup-plan.md` (5 commits, blockers split first). **Shipped this session:** Commit 1a (B1 selection sync + B3 teardown + M1 timeline clamp) and Commit 1b (B2 mute pipeline). 1b was much simpler than feared — `scene_loader.rs:1481-1508` already preserves mutes across reopens via `load_usd_with_stage_muted`, so the fix is pure wiring: mutate `renderer.scene.layer_state.muted`, call `load_usd_scene(&current_stage_path)`, refresh shell mirror. `cargo build` + `clippy -D warnings` + `fmt --check` clean. **Next session:** push 1a + 1b (or dogfood first), then commits 2–5 (housekeeping, property inspector O(N²) cache, ortho aspect + QTimer gating + demo-seed guard, optional scene browser lazy fetchMore).

## 🏁 2026-04-19 late evening — v0.15-qt Commit 1b (B2 mute pipeline)

**The fix was simpler than planned.** Research revealed `scene_loader.rs::load_usd_scene` (lines 1496-1508) already snapshots `scene.layer_state.muted` at entry and replays via `load_usd_with_stage_muted` *before* payloads fetch — the loader was designed for repeated re-entry on mute/variant changes ("implicit stage reopens" per comment at 1492-1495). No new `Renderer::refresh_scene_from_live_stage()` API needed.

**Implementation (bif_qt/src/main_window.rs::set_layer_muted):**
1. Update shell mirror (existing behavior).
2. If no `current_stage_path` (demo stack): stop — no regression from Phase C.1.
3. Mutate `renderer.scene.layer_state.muted` via `with_viewport_mut`; call `r.load_usd_scene(&path)`. Loader picks up the muted set, reopens with mutes applied pre-payload, re-extracts geometry, rebuilds GPU via `reload_working_scene`.
4. Refresh shell mirror from freshly-composed stage; bump `layer_state_revision` + `scene_browser_revision` so property inspector / tree / color dots re-resolve.
5. Status bar shows `"Layer muted: <identifier>"` or the reload error.

**Key decision — skip `reset_scene_state`.** That wipes `scene.layer_state` which would erase the mute snapshot the loader depends on. `finalize_usd_scene` already overwrites the USD halves of `SceneManager` internally. Post-Phase-F node-graph restoration may want a "reset node caches but preserve layer_state" variant — left as a TODO comment.

**Non-obvious bits:**
- `SceneLayerState::muted` is the authoritative set; `LayerInfo::is_muted` per-layer flags are derived — `SceneLayerState::from_stage` reconstructs them from the composed stage after reload. Don't need to manually flip per-layer flags.
- Qt `scene_layer_state` is a mirror of `renderer.scene.layer_state`. Mutations must flow to the renderer side *before* reload so the loader's snapshot sees them, then mirror refreshes from renderer after.
- Empty-scene case (mute removes the def-providing layer) is handled at `scene_loader.rs:1528-1557` — viewport clears, `layer_state` stays populated so the Layer Stack panel can unmute. Nothing for bif_qt to do.

## 🏁 2026-04-19 evening — v0.15-qt Commit 1a (B1 + B3 + M1)

**Three review passes** before push: `vfx-code-reviewer` (USD/FFI/GPU lens), `Code Reviewer` (general correctness/hygiene), and a manual behavioral pass. Agents converged on Major-tier hygiene (double `env_logger::init`, dead `bif_qt_spike` crate, O(N²) property inspector, 1817-line God module) but **missed every cross-panel behavioral bug** — selection didn't sync across panels, layer mute was UI-only on real stages, close-stage leaked playback/frame state. The manual pass caught these because it asked "click X in panel A → does panel B highlight?" rather than reading diffs line-by-line.

**Commit 1a** closes the pure-wiring blockers; B2 (mute pipeline) needs a renderer API addition and got split into 1b.

- **B1 — three-panel selection sync.** Viewport clicks now route through `Renderer::select_at_screen` (the actual renderer-side selection path — updates `selection.selected_instance_index`, resets gizmo, clears on empty-space click). New `Renderer::select_prim_by_path(&str)` wraps `(pub(crate))` `handle_prim_selected` for tree clicks. New `on_tree_prim_selected` invokable on `BifShellState`; `SceneBrowserWidget::on_selection_changed` routes through it so tree clicks drive viewport gizmo + outline, not just shell qprops.
- **B3 — stage teardown.** `close_stage` sets `is_playing=false` + resets `current_frame=start_frame` *before* renderer reset (timer was advancing `current_frame` onto the first-launch screen otherwise). `on_stage_path_opened` also resets `is_playing` at entry so the prior stage's playback doesn't run against the new stage's frame. `Renderer::reset_scene_state` now calls `selection.clear()` so stale `selected_prim_path` + instance index + gizmo state don't resolve to wrong rows.
- **M1 — `detect_timeline_from_stage` clamps `current_frame`.** After writing range qprops, clamps the frame into `[start, end]`. Spinbox widget already clamps its display, but the qproperty drives the playback timer + render eval — out-of-range values made playback start from the wrong frame until user interaction.

**Non-obvious bits:**
- `cxx-qt` i32 qproperty getters return `&i32` — need `*` deref before passing into `.clamp()` or back through setters. Caught at compile time; three sites.
- `select_at_screen` already did full "pick + update selection + clear on miss" plumbing — the Qt side was duplicating selection state by bypassing it with `pick_instance_at` directly.
- `SelectionManager::clear()` (selection.rs:36) already exists — extending `reset_scene_state` is a one-liner.

---

# Session Handoff — April 17, 2026 (Tier 1 + 3-bugfix bundle shipped)

**Last Updated:** 2026-04-17 evening. Tier 1 (`f2a67c2`) shipped first — edit-target pill + viewport edge tint + status-bar chip + breadcrumb layer segment + auto-pick strongest writable sublayer + window title + friendly schema labels + save-flow polish. Then a 3-bugfix bundle (`962a3b7`) closed user-observed gaps: animation playback didn't reach the renderer, loading a second USD left old prims in viewport AND scene tree, layer color dots never painted in the scene browser. Workspace builds + clippy `-D warnings` clean, fmt clean, schema_labels tests 4/4; two pre-existing test failures unchanged. User visually confirmed all 3 fixes (`"all is working"`) — they're testing more before signing off. **Next session: Tier 1.5 per-attribute opinion-resolution FFI** (wraps `UsdAttribute::GetPropertyStack` → per-row color, gates first-opinion guard + Tier 2 #9 inspector left-border). Or viewport toolbar (Tier 1 item #6.5, M effort) if user prefers artist-visible win first. Or knock out remaining BUGLIST items — camera-picker / orthographic views (no looking-through-camera support yet). User preference captured in memory: function before form — full design-system pass ("Graphite / Quiet Confidence" from `assets/stitch_bif_ui/obsidian_graphite/DESIGN.md`) deferred until all Tier 1/1.5/2 widgets are in place.

## 🏁 2026-04-17 evening — 3-bugfix bundle (commit `962a3b7`)

User-reported during dogfood after Tier 1 landed:

- **Animation playback doesn't update the viewport.** `Renderer::update_animation` was the eval loop but its only caller was the deleted egui frame loop (Phase F). Qt's QTimer advanced `current_frame` qproperty but the value never propagated into `Renderer::timeline_state`. **Fix:** extracted eval body into `Renderer::apply_animation_at_current_frame()` helper; added `Renderer::set_time(frame: f64)` that snaps `timeline_state.current_frame` then calls the helper (skipping wall-clock advance); added `BifShellState::on_frame_changed(frame: i32)` invokable that calls `with_viewport_mut(|vp| vp.renderer_mut().set_time(frame as f64))`; connected `current_frameChanged` → `on_frame_changed` in `window_builder.cpp` so both QTimer playback and slider drags propagate. The 0.5-frame `last_evaluated_frame` tolerance guards against re-eval storms.

- **Loading a second USD shows leftover prims** (viewport + scene browser tree). `Renderer::load_usd_scene` doesn't fully evict prior `SceneManager` state up front. First fix attempt cleared just `scene` (USD half) — viewport cleared but tree didn't because `nodes.cached_scene_graph` (procedural prim cache, the other half of `CompositeProvider`) survived. **Real fix:** added `Renderer::reset_scene_state()` to bif_viewport — single source of truth for "evict the previous stage entirely". Drains GPU, replaces `SceneManager`, clears all `nodes.*` caches (`cached_scene_graph`, `instancer_results`, `node_proto/cloud maps`, `node_prim_counts`, `primitive_name_counters`), bumps dirty flags, rebuilds pick BVH. Both `close_stage` (Ctrl+W) and the second-open reset call it; the helper closed a latent procedural-cache leak in `close_stage` too. Bumped `scene_browser_revision` + `layer_state_revision` after the reset so the model rebuilds against empty state before new data lands.

- **Scene browser layer color dots are missing.** `populate_subtree` and `rebuild_from_state` in `scene_browser_model.cpp` passed `/*color_index=*/-1` for every row. `PrimRowDelegate::paint` only draws the dot when `color_index >= 0 && color_index < 8`. **Fix:** added `prim_color_index_at(path)` invokable that reads `SceneLayerState::layer_for_prim` (prim_path → strongest opinion source layer, populated by `populate_layer_for_prim` after stage load) and returns palette index mod 8. Both row builders now call it.

## 🏁 2026-04-17 — Tier 1 (commit `f2a67c2`)

- **4 new edit-target invokables** on `BifShellState`: `active_edit_target_is_set` / `_name` / `_identifier` / `_color_index`. All read `scene_layer_state.working_layer`; all C++ consumers refresh on `layer_state_revisionChanged` (no new signal).
- **`current_stage_display`** invokable — leaf-name of the current stage path for breadcrumb + title.
- **`compose_title`** invokable — `BIF — stage.usda[*] · Editing: layer`. Dirty asterisk derives from `LayerInfo::is_dirty` (latent until v0.16 write path flips it).
- **Auto-pick** — new `pick_strongest_writable_sublayer` helper + `is_writable_layer` helper. On stage load success, re-picks `working_layer` to the first `!is_anonymous && !is_muted` layer. Real `SdfLayer::PermissionToEdit()` FFI deferred to Tier 1.5.
- **Status toast** — stage-load message is now `"Loaded: <path>  •  Edit target: <layer>"`.
- **Pill** — right side of breadcrumb row, color dot + `"Editing: <name>"` label, pill background tinted with layer color. Hides when no stage.
- **Status-bar chip** — compact variant, permanent widget on right of status bar.
- **Viewport edge tint** — 2px inner border on a `QFrame` wrapping the viewport stack, colored by edit target.
- **Breadcrumb layer segment** — `stage.usda › layer (edit) › prim › path`.
- **Schema labels** — new `crates/bif_qt/src/schema_labels.rs` with two functions (`friendly_attribute_name`, `friendly_prim_type`). Covers xformOp, primvars, UsdGeom, UsdLux, UsdGeomCamera, common prim types. Property inspector calls the invokables; raw USD name appears in tooltip. Unknown names pass through unchanged.
- **Save-flow polish** — `on_save` / `on_save_as` branch on edit-target presence so the "no stage" case reads differently from "write not wired yet".

---

# Session Handoff — April 15-16, 2026 (Phase E.2 + Phase F — egui deleted)

## ✅ 2026-04-15 — Phase E.2 first pass

Commits on `v0.15-qt` (newest first):

```
cc44a45 feat(qt): share Open Stage flow + fix file dialog UX via paint pause
d290c9d fix(qt): guard viewport_on_surface_ready against double-init + quiet wgpu_hal
6777874 feat(qt): wire camera orbit/pan/zoom + fix GPU cleanup on exit
3c6be37 fix(qt): filter wgpu log spam + fix QFileDialog z-order on Windows
6fe81ed feat(qt): v0.15.0 Phase E.2 moves 4-opt + 6 — live stage timeline + breadcrumb wire
8107bb7 feat(qt): v0.15.0 Phase E.2 move 2 — real stage load + ADR-007 bridge
0f006b2 feat(qt): v0.15.0 Phase E.2 move 4 — real timeline detect from USD stage
ae4ed7c feat(qt): v0.15.0 Phase E.2 move 1 — bif_qt::Viewport hosts real Renderer
264394f refactor(viewport): decouple Renderer from winit (v0.15 Phase E.2-prep)
```

**Feature deliveries:**

- `bif_viewport::Renderer` decoupled from winit — API surface now `(surface, device, queue, config, size, scale_factor)` + optional `attach_egui`. `bif_viewer` compat shim preserved.
- `bif_qt::Viewport` hosts the real `bif_viewport::Renderer` — triangle deleted. HWND-built wgpu primitives feed `Renderer::new(...)`, headless (no egui) path for rendering.
- ADR-007 locks in the **β (thread-local raw-pointer) bridge** between `BifShellState` invokables and `ViewportCallbacks`. `with_viewport_mut(|vp| ...)` helper. `install_viewport_callbacks(&mut viewport_cb)` called from `app.rs::run` before the Qt event loop.
- Moves 1, 2, 4, 6 delivered. Camera orbit/pan/zoom wired. Shared `trigger_open_stage` helper.

**Bugs fixed during dogfood:**

- D3D12 `OBJECT_DELETED_WHILE_STILL_IN_USE` crash on close → `Renderer::wait_for_gpu()` called in `viewport_on_shutdown`.
- `QFileDialog` z-order behind main window → paused the 16ms render tick around the dialog via new `RenderWidget::pausePainting/resumePainting` + member `m_tick`. Removed the `window->setVisible(false/true)` hack that was causing the app to briefly vanish.
- `showEvent` fired twice after `setVisible` reshow → new Renderer was dropped while GPU commands in-flight → guarded `viewport_on_surface_ready` against re-init.
- `wgpu_core` `Device::maintain` INFO spam at 60 FPS → env_logger filter `info,wgpu_core=warn,wgpu_hal=error,naga=warn`.

---

## Earlier Sessions (pre-April 15, 2026)

### v0.13.6-dev Apr 12: Rigid Mesh Offset Bug Fixed

- **Root cause:** `SkinKind::Rigid` compression assumed `element_size == 1`, collapsing multi-joint rigid bindings (hair `elem=3, w=0.333`; nails `elem=2, w=0.5`) to fractional-weight influence.
- **Fix:** loader gates `SkinKind::Rigid` path on `element_size == 1`. Multi-joint flows through `SkinKind::PerVertex`.
- **Regression guard:** `rigid_matches_pervertex_single_influence` test.

### v0.13.6-dev Apr 11: UsdSkelBlendShape + Architecture Refactor Closed

- Full CPU blend shape pipeline: C++ FFI dense-expand, shape-order remap, per-frame `ComputeBlendShapeWeights`, `apply_blend_shapes()` in skinning module, per-frame playback.
- Architecture refactor campaign closed: all 5 phases + 7/8 review items shipped across v0.13.0-v0.13.5. ~79 new tests.

### v0.13.5 Apr 10: UsdSkel Import Complete

- All 4 phases done: C++ SkelCache refactor, Mesh::skin wiring, CPU LBS module, per-frame eval + viewport hookup.
- HumanFemale.walk.usd loads coherent, all 77 skinned prototypes deform.

### v0.13.0 Apr 7: UsdStage Sync Fix

- Removed `unsafe impl Sync for UsdStage`, wrapped in `Arc<Mutex<UsdStage>>`. 10 files, ~20 callsites.

### v0.13.0 Apr 5-6: CPU Displacement + Selection Outline + Obsidian Wiki

- CPU vertex displacement: `displacement.rs`, bilinear sampling, `Mesh::recompute_bounds()`, 14 unit tests.
- Selection outline: dedicated `outline.wgsl`, normal-expanded back-face silhouette.
- Obsidian wiki: 42 articles across 8 sections.

### Earlier (Mar-Apr 2026)

- v0.13.0 Apr 2-4: Subdiv, Inspector, Display Color, Variants, Selection
- Mar 31: MaterialX file format support
- Mar 30: Power-weighted light sampling, SHARC cache, GitHub Pages site, PointInstancer loading fixes
- Mar 28-29: Architecture deepening — Phase 1 FFI bridge split
- Mar 27: Documentation overhaul (MILESTONES.md, ROADMAP_DETAIL.md, README.md)
- Mar 24-26: M30 + M31 complete (project persistence, eval modes, scene graph visualization)
- Mar 23: M29.5 egui UI overhaul

---
