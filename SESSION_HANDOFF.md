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

**What 1a intentionally did NOT touch:**
- B2 (mute pipeline) — needs a renderer "refresh scene from live stage without file reload" method or a mute-survives-reload workaround. Clean split because the other blockers are pure wiring and this one needs an API design decision.
- Phase B stubs, About string, `bif_qt_spike` removal — Commit 2 housekeeping batch.
- O(N²) property inspector — Commit 3 self-contained perf cache.
- Ortho aspect ratio, QTimer gating, demo-seed guard — Commit 4.
- Scene browser lazy fetchMore — optional Commit 5.

## 🏁 2026-04-17 evening — 3-bugfix bundle (commit `962a3b7`) (historical)

**Prior session context preserved below.**

---

# Session Handoff — April 17, 2026 (Tier 1 + 3-bugfix bundle shipped)

**Last Updated:** 2026-04-17 evening. Tier 1 (`f2a67c2`) shipped first — edit-target pill + viewport edge tint + status-bar chip + breadcrumb layer segment + auto-pick strongest writable sublayer + window title + friendly schema labels + save-flow polish. Then a 3-bugfix bundle (`962a3b7`) closed user-observed gaps: animation playback didn't reach the renderer, loading a second USD left old prims in viewport AND scene tree, layer color dots never painted in the scene browser. Workspace builds + clippy `-D warnings` clean, fmt clean, schema_labels tests 4/4; two pre-existing test failures unchanged. User visually confirmed all 3 fixes (`"all is working"`) — they're testing more before signing off. **Next session: Tier 1.5 per-attribute opinion-resolution FFI** (wraps `UsdAttribute::GetPropertyStack` → per-row color, gates first-opinion guard + Tier 2 #9 inspector left-border). Or viewport toolbar (Tier 1 item #6.5, M effort) if user prefers artist-visible win first. Or knock out remaining BUGLIST items — camera-picker / orthographic views (no looking-through-camera support yet). User preference captured in memory: function before form — full design-system pass ("Graphite / Quiet Confidence" from `assets/stitch_bif_ui/obsidian_graphite/DESIGN.md`) deferred until all Tier 1/1.5/2 widgets are in place.

## 🏁 2026-04-17 evening — 3-bugfix bundle (commit `962a3b7`)

User-reported during dogfood after Tier 1 landed:

- **Animation playback doesn't update the viewport.** `Renderer::update_animation` was the eval loop but its only caller was the deleted egui frame loop (Phase F). Qt's QTimer advanced `current_frame` qproperty but the value never propagated into `Renderer::timeline_state`. **Fix:** extracted eval body into `Renderer::apply_animation_at_current_frame()` helper; added `Renderer::set_time(frame: f64)` that snaps `timeline_state.current_frame` then calls the helper (skipping wall-clock advance); added `BifShellState::on_frame_changed(frame: i32)` invokable that calls `with_viewport_mut(|vp| vp.renderer_mut().set_time(frame as f64))`; connected `current_frameChanged` → `on_frame_changed` in `window_builder.cpp` so both QTimer playback and slider drags propagate. The 0.5-frame `last_evaluated_frame` tolerance guards against re-eval storms.

- **Loading a second USD shows leftover prims** (viewport + scene browser tree). `Renderer::load_usd_scene` doesn't fully evict prior `SceneManager` state up front. First fix attempt cleared just `scene` (USD half) — viewport cleared but tree didn't because `nodes.cached_scene_graph` (procedural prim cache, the other half of `CompositeProvider`) survived. **Real fix:** added `Renderer::reset_scene_state()` to bif_viewport — single source of truth for "evict the previous stage entirely". Drains GPU, replaces `SceneManager`, clears all `nodes.*` caches (`cached_scene_graph`, `instancer_results`, `node_proto/cloud maps`, `node_prim_counts`, `primitive_name_counters`), bumps dirty flags, rebuilds pick BVH. Both `close_stage` (Ctrl+W) and the second-open reset call it; the helper closed a latent procedural-cache leak in `close_stage` too. Bumped `scene_browser_revision` + `layer_state_revision` after the reset so the model rebuilds against empty state before new data lands.

- **Scene browser layer color dots are missing.** `populate_subtree` and `rebuild_from_state` in `scene_browser_model.cpp` passed `/*color_index=*/-1` for every row. `PrimRowDelegate::paint` only draws the dot when `color_index >= 0 && color_index < 8`. **Fix:** added `prim_color_index_at(path)` invokable that reads `SceneLayerState::layer_for_prim` (prim_path → strongest opinion source layer, populated by `populate_layer_for_prim` after stage load) and returns palette index mod 8. Both row builders now call it.

**Considered alternative for #2 (rejected).** User suggested routing file-load through a UsdRead node graph instead. That would require reviving the egui-snarl evaluation pipeline that's currently in-tree dead code from Phase F — much bigger surgery for the same observable result. Direct `Renderer::load_usd_scene` + `reset_scene_state` keeps the simple path. Node-graph routing is a future architectural reorg (post Tier 2/3), not a bug-fix-session move.

**Cleanup:** dropped `use bif_viewport::SceneManager` import from `main_window.rs` — no longer needed after the `reset_scene_state` refactor (clippy caught it).

## 🏁 2026-04-17 — Tier 1 (commit `f2a67c2`)

**What changed (single commit on top of Phase F):**

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

**Non-obvious bits:**

- Pill can't live *inside* the breadcrumb `QToolBar` — `breadcrumb_set_path` calls `bar->clear()` on every selection change, which would nuke the pill. Refactored `build_central_area` to wrap [breadcrumb + pill] in a sibling `QHBoxLayout`.
- Viewport edge tint is achieved by wrapping the `QStackedWidget` in a `QFrame` with 2px contentsMargins — the frame's background fills the margins as a "border".
- `current_stage_path` is not a qproperty (plain `Option<PathBuf>`); adding one would require converting field + setter + all readers. Instead added the `current_stage_display` invokable and piggybacked on `layer_state_revisionChanged` which fires immediately after stage load sets both fields.

**Files touched (uncommitted on top of Phase F commit `e3d30bc`):**

```
 M BUGLIST.md                                              (+2 bugs)
 M crates/bif_qt/cpp/property_inspector_widget.cpp         (friendly labels)
 M crates/bif_qt/cpp/window_builder.cpp                    (pill/chip/tint/title)
 M crates/bif_qt/src/lib.rs                                (+schema_labels mod)
 M crates/bif_qt/src/main_window.rs                        (8 new invokables)
 A crates/bif_qt/src/schema_labels.rs                      (new module)
```

## 🏁 2026-04-16 evening — Tier 0 + Phase F
**Current Version:** v0.14.0 shipped on `main`; v0.15.0 in progress on `v0.15-qt`.
**Project:** BIF — USD Orchestration Tool for VFX.

## 🏁 2026-04-16 evening — Tier 0 + Phase F

**Tier 0 (parity gate before egui deletion):**

- **CompositeProvider routing.** New `with_scene_browser_provider` helper in `bif_qt::main_window` mirrors `with_stage` but builds a `CompositeProvider` (USD stage + procedural prim cache + synthetic `/BIF/`) per call. The 4 tree invokables + `prim_type_name_at` route through it. New `Renderer::cached_scene_graph()` accessor on `bif_viewport` exposes the cache without widening the private `nodes` field. Architectural parity with egui's data path; residual lucy.usd child-count gap under a couple sections is a downstream `UsdStage::child_prim_paths` issue (BUGLIST).
- **Three new invokables:** `prim_kind_at`, `prim_is_visible_at`, `prim_is_active_at` — sourced from `PrimDisplayInfo`. Specifier dropped (not in composed-stage info; egui doesn't show it either).
- **Scene browser model:** `columnCount()` 1→4, new `Columns` enum + `PrimRoles` for kind/visible/active/children-count, `PrimNode` extended, `populate_subtree` + `rebuild_from_state` pre-fetch the new fields, `headerData` + `data()` cover all columns. Header now shown with section resize modes.
- **Row chrome:** `PrimRowDelegate` paints an eye glyph (filled when visible, struck-through when hidden) before the existing layer color dot in column 0; inactive prims get reduced-alpha text via palette override. Read-only — toggle interactivity is Tier 1+.

**Phase F (egui bridge deletion — adapted scope):**

- **Renderer egui surface gone.** Deleted `egui_ctx` / `egui_state` / `egui_renderer` fields, `attach_egui` / `egui_state_mut` / `egui_ctx` / `reset_property_inspector_cache` methods. `Renderer::render` simplified: `render(&mut self, clear_color: wgpu::Color) -> Result<()>`. Internal `run_egui_frame` (~870 LOC) and `submit_gpu_frame`'s egui paint pass deleted. Cache-reset call in `selection_dispatch` removed.
- **`render_ui.rs` deleted** (~729 LOC).
- **Cargo.toml diet.** `bif_viewport`: dropped `egui-wgpu` + `egui-winit`; kept `egui` + `egui-snarl` for the in-tree dead panel modules. `bif_viewer`: dropped `wgpu` / `winit` / `egui` / `egui-wgpu` / `egui-winit` / `pollster`; added `bif_qt`. Confirmed via `cargo tree` — `egui-wgpu` and `egui-winit` no longer pulled by either.
- **`bif_viewer/src/main.rs`** rewritten 905 → 14 lines as a `bif_qt::run()` shim. CLI autoload (`--usd <path>`) is a regression — filed in BUGLIST. Use File → Open Stage instead.
- **`bif_qt` fallout — one line.** `viewport.rs::Viewport::render` dropped the `None` raw_input arg. Recon confirmed bif_qt rust has zero egui-typed imports otherwise.
- **`egui_panels_legacy` feature flag SKIPPED.** Original plan called for one to gate `property_inspector` / `layer_stack_panel` / `node_graph` / `theme`, but that required also gating `NodeGraphContext` (egui-snarl typed) — large refactor for a "future cannibalization" benefit only. Pragmatic call: leave panels in-tree as dead code; revisit when Qt replacements land.

**Files modified this session (uncommitted):**

```
 M BUGLIST.md
 M CHANGELOG.md
 M MILESTONES.md
 M SESSION_HANDOFF.md
 M crates/bif_qt/cpp/scene_browser_model.cpp
 M crates/bif_qt/cpp/scene_browser_model.h
 M crates/bif_qt/cpp/scene_browser_widget.cpp
 M crates/bif_qt/src/main_window.rs
 M crates/bif_qt/src/viewport.rs
 M crates/bif_viewer/Cargo.toml
 M crates/bif_viewer/src/main.rs
 M crates/bif_viewport/Cargo.toml
 M crates/bif_viewport/src/lib.rs
 M crates/bif_viewport/src/render.rs
 M crates/bif_viewport/src/selection_dispatch.rs
 D crates/bif_viewport/src/render_ui.rs
 M devlog/2026-04/DEVLOG_2026-04-16.md
 ?? wiki/journal/2026-04-16-tier0-phase-f.md
```

## 🏁 2026-04-16 (earlier) — Phase E.2 feature-complete

`bif_qt_shell` now reads the live USD stage end-to-end:

- **HiDPI:** `devicePixelRatioF()` threaded through `viewport_on_surface_ready` / `viewport_on_resize` bridge → `Viewport::new` / `Viewport::resize` → `Renderer`. No signal-signature change; captured inside existing lambdas.
- **Close Stage (Ctrl+W):** new `close_stage` invokable drains GPU, resets `Renderer::scene = SceneManager::new()`, rebuilds pick BVH, clears layer state + selection + path, bumps revisions, swaps central stack to first-launch.
- **Move 9 — Keyframes:** `demo_keyframes` deleted. `keyframe_count/at` + `jump_to_prev/next_keyframe` derive from the selected prim's `AnimatedTransform` via `selected_prim_keyframes` helper over `scene.working_scene.instances()` + `instance_animations()`. Timeline ruler refreshes on `selected_prim_pathChanged`.
- **Move 5 — Gizmo pick:** new `on_prim_pick(x, y)` invokable → `Renderer::pick_instance_at` → `scene.working_scene.instances()[idx].prim_path` → `denormalize_synthetic_path` (inlined) → sets `selected_prim_path` + `selected_prim_type` (latter via `UsdStage::get_prim_info_by_path`). `RenderWidget::mousePressEvent` multiplies pick coords by DPR before emit.
- **Move 7 — Scene Browser:** new `scene_browser_revision` qproperty. 6 new invokables backing the tree (`root_prim_count/path_at`, `child_prim_count/path_at`, `prim_type_name_at`, `prim_display_name_at`) via `PrimDataProvider` trait on `UsdStage`. `SceneBrowserModel` takes `BifShellState*`, connects `scene_browser_revisionChanged`, recursively rebuilds on stage load (depth cap 64). Falls back to demo tree when no stage loaded.
- **Move 8 — Property Inspector:** 9 new invokables backing real attributes (`selected_prim_attribute_count/name/type/value_at`) + composition arcs (`selected_prim_stack_count/layer/specifier/has_opinion/color_index_at`) via `UsdStage::get_prim_attributes` / `get_prim_stack`. `PropertyInspectorWidget` deletes ~55 lines of `FakeAttr` tables; opinion dot color derives from matching stack layer identifier to `SceneLayerState::stack.layers` index (mod 8).

**Uncommitted. 9 files modified:**

```
 M crates/bif_qt/cpp/property_inspector_widget.cpp
 M crates/bif_qt/cpp/render_widget.cpp
 M crates/bif_qt/cpp/scene_browser_model.cpp
 M crates/bif_qt/cpp/scene_browser_model.h
 M crates/bif_qt/cpp/scene_browser_widget.cpp
 M crates/bif_qt/cpp/timeline_widget.cpp
 M crates/bif_qt/cpp/window_builder.cpp
 M crates/bif_qt/src/main_window.rs
 M crates/bif_qt/src/viewport.rs
```

**Gotchas learned today:**

- **Inherent methods mask trait methods with shared names.** `UsdStage` has both `get_prim_info(index: usize)` inherent AND `impl PrimDataProvider` with `get_prim_info(path: &str)`. `stage.get_prim_info(&str)` dispatches to inherent → E0308. Fix: use the inherent `get_prim_info_by_path(&str)` — no trait-shadow path needed.
- **cxx-qt `self.as_mut().rust_mut()` requires `let mut r = ...`** at the binding site even though `rust_mut()` returns something pin-bound.
- **Move DPR-aware pick coord multiply into C++** (`mousePressEvent` emits physical pixels) — keeps all downstream Rust invokables DPR-agnostic and matches the existing `resized(pixelWidth(), pixelHeight())` pattern.
- **`denormalize_synthetic_path` is crate-private in `bif_viewport`** — simple enough to inline in bif_qt rather than widen API surface.

## ▶️ Pickup next session (2026-04-17+)

1. ✅ **Dogfood passed** (this session). Remaining gaps captured in `BUGLIST.md`; acceptable to ship around.
2. ✅ **Phase E.2 committed** as `834add4` (bundled commit — main_window.rs interleaving made clean splits messy).
3. ✅ **Wiki synced** — journal entries for 2026-04-15 and 2026-04-16, plus architecture + concept articles for Phase E.2 roadmap, cxx-qt patterns, paint-pause, HiDPI DPR threading, PrimDataProvider.
4. **Phase F — egui cleanup** ← next target. Delete `egui`/`egui-wgpu`/`egui-winit`/`egui-snarl` from `bif_viewer` + `bif_viewport` Cargo.toml. Delete `run_egui_frame` + ~900 lines of panel assembly. Retarget `bif_viewer` main at `bif_qt::run`. Estimate ~2h; mostly deletion.
5. **Phase G — validation + release plumbing.**
6. **Phase H — `v0.15.0` tag + merge back to main.**

### Phase F opening checklist

- Confirm `bif_viewport::Renderer::attach_egui` / `egui_state_mut` / `egui_ctx` are the only remaining egui surface on bif_viewport — delete them.
- `render(clear_color, raw_input: Option<RawInput>) -> Option<PlatformOutput>` collapses to `render(clear_color) -> Result<()>` once the egui branch is gone.
- `bif_viewer/src/main.rs` `create_renderer` compat shim becomes the entry point to `bif_qt::run`.
- egui-snarl lives in bif_viewer; node-graph UI moves away entirely (no Qt replacement yet — that's post-v0.15 work).
- Verify `bif_viewer` Cargo binary still builds and launches the Qt shell.

## Known gaps for v0.16

- Scene Browser tree walk is eager (not `canFetchMore`/`fetchMore`). Fine for `root.usda`; needs lazy fetch for 100K+ prim scenes.
- Property Inspector opinion dot uses strongest-layer color for every attribute row — real per-attribute opinion resolution deferred.
- Attribute value stringification is FFI-side `Debug`-summary — pretty-printing is v0.16.

---

## ✅ 2026-04-15 — prior-session (Phase E.2 first pass)

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
- **Move 1:** viewport wires to real Renderer.
- **Move 2:** `on_stage_path_opened` does real `Renderer::load_usd_scene`, populates `scene_layer_state`, bumps `layer_state_revision` → Layer Stack panel shows real layers.
- **Move 4:** `detect_timeline_from_stage` reads live stage's `UsdTimelineData` via existing `cpp_bridge` wrapper — writes `start_frame/end_frame/playback_fps` qproperties.
- **Move 6:** breadcrumb bar connected to `selected_prim_pathChanged` signal.
- Camera orbit / pan / zoom invokables drive the real `Renderer.cam.camera` via `with_viewport_mut`.
- Shared `trigger_open_stage` helper — File menu, first-launch Open button, recent-stage clicks all flow through one code path.

**Bugs fixed during dogfood:**

- D3D12 `OBJECT_DELETED_WHILE_STILL_IN_USE` crash on close → `Renderer::wait_for_gpu()` called in `viewport_on_shutdown`.
- `QFileDialog` z-order behind main window → paused the 16ms render tick around the dialog via new `RenderWidget::pausePainting/resumePainting` + member `m_tick`. Removed the `window->setVisible(false/true)` hack that was causing the app to briefly vanish.
- `showEvent` fired twice after `setVisible` reshow → new Renderer was dropped while GPU commands in-flight → guarded `viewport_on_surface_ready` against re-init.
- `wgpu_core` `Device::maintain` INFO spam at 60 FPS → env_logger filter `info,wgpu_core=warn,wgpu_hal=error,naga=warn`.

**Still demo data / stubbed (see BUGLIST):** Scene Browser tree, Property Inspector attributes, Timeline keyframes, Gizmo raycast, HiDPI scale, reset/close-stage action.

---

## 🚀 How to pick up next session

1. `. .\setup_qt_env.ps1` — Qt 6.8.3 LTS env (required for any bif_qt work).
2. `. .\setup_usd_env.ps1` — USD DLLs (required to load stages in bif_qt_shell).
3. `git checkout v0.15-qt`. `main` is v0.14.0-frozen.
4. Dogfood: `cargo run -p bif_qt --bin bif_qt_shell`. File → Open or launch-screen "Open USD" → pick `test_assets/layers/root.usda` (or any .usda in your test set). Verify:
   - Viewport renders geometry (if stage has any).
   - Layer Stack panel shows real layers (count > 0, mute toggle works).
   - Timeline `⇅` detect button populates Start/End/FPS.
   - Scene Browser click → breadcrumb updates at the top of the central area.
   - Alt+LMB drag orbits, MMB pans, wheel zooms.
5. Read **"Next target ordering"** below — picks the next move to ship.

---

## ▶️ Next target ordering (Phase E.2 moves 5, 7, 8, 9 + polish)

**Recommended order (small → medium):**

### 1. Move 9 — Timeline keyframes from `AnimatedTransform` (smallest, ~30min)
Replace `BifShellStateRust::demo_keyframes: Vec<i32>` with keyframes derived from the currently-selected prim's `AnimatedTransform`. Lives in `bif_core::animation` (per handoff code map). Read via `with_viewport_mut(|vp| vp.renderer_mut().scene.…)`. Rebuild keyframe list when `selected_prim_path` changes.

### 2. Move 6-sibling / polish — File → Close Stage + reset viewport (~30min)
Add a "Close Stage" menu item (Ctrl+W?) that:
- Calls a new invokable `close_stage` → clears `scene_layer_state`, clears `current_stage_path`, bumps `layer_state_revision`, sets status.
- Via ADR-007 bridge, resets the renderer: `with_viewport_mut(|vp| { vp.renderer_mut().scene = SceneManager::new(); vp.renderer_mut().rebuild_pick_scene(); })` — or whatever the bif_viewer "new scene" flow does.
- Swaps central_stack back to first-launch index (0).

### 3. Move 5 — Gizmo raycast on LMB (~1h)
`RenderWidget::primPickRequested(x, y)` already fires. Connect it to a new invokable `on_prim_pick(i32, i32)` that:
- `with_viewport_mut(|vp| vp.renderer_mut().pick_at(x, y))` — need to identify the existing pick-scene API. `bif_viewport::selection::ray_cast` is the entry point per handoff. `Renderer::rebuild_pick_scene` already exists; `Renderer::pick_scene: Option<EmbreePickScene>` holds the BVH.
- On hit, set `selected_prim_path` qproperty → breadcrumb + property inspector auto-update (already wired).

### 4. Move 7 — Scene Browser real data (~1.5h)
Replace `SceneBrowserModel::seed_demo_tree` (in `cpp/scene_browser_model.cpp`) with a traversal over `bif_core::CompositeProvider` built from the live stage. For 100K+ prims, use `canFetchMore` / `fetchMore` for lazy population. Trigger reset via the existing `layer_state_revisionChanged` signal (or add a new `scene_revision` qproperty).

### 5. Move 8 — Property Inspector real attributes (~1.5h)
Replace fake attrs in `cpp/property_inspector_widget.cpp` with a Rust-side invokable that returns `UsdPrim::GetAttributes()` for the selected prim. Composition arcs from `UsdStage::get_prim_stack` (already exists on `UsdStage`). Opinion-dot delegate already paints winning-layer color — just needs real data.

### 6. HiDPI scale factor (~20min)
Wire `QScreen::devicePixelRatio()` through the cxx-qt bridge → `Viewport::new` + `Viewport::resize`. Currently hardcoded `1.0`. Test on a 125%/150% monitor.

**Acceptance for Phase E.2 complete:** moves 5 + 7 + 8 + 9 shipped, Phase E.2 deliverables checklist below fully checked, manual dogfood end-to-end clean.

---

## 📋 Phase E.2 deliverables — updated checklist

- [x] `bif_qt::Viewport` renders real scenes (not a triangle) — **Move 1**
- [x] `File → Open` loads actual USD stage — **Move 2**
- [x] `BifShellState::scene_layer_state` populated from load — **Move 2**
- [x] Scene Browser shows real prim tree via `PrimDataProvider`/`UsdStage` — **Move 7 (2026-04-16)**
- [x] Property Inspector shows real attributes via `UsdStage::get_prim_attributes` — **Move 8 (2026-04-16)**
- [x] Composition arcs from `UsdStage::get_prim_stack` — **Move 8 (2026-04-16)**
- [x] Timeline keyframes from selected prim's `AnimatedTransform` — **Move 9 (2026-04-16)**
- [x] `detect_timeline_from_stage` reads real USD time metadata — **Move 4 + 4-opt**
- [x] Gizmo raycast via `Renderer::pick_instance_at` on LMB click — **Move 5 (2026-04-16)**
- [x] Breadcrumb connected to `selected_prim_pathChanged` — **Move 6**
- [x] AppEvent bridge chosen + implemented — **ADR-007 (β thread-local raw-pointer)**
- [ ] `bif_qt_shell` loads `test_assets/layers/root.usda` end-to-end (full dogfood) — **pending user validation 2026-04-16**

**11 of 12 done. Remaining: user-driven dogfood + commit.**

---

## 🧠 Gotchas learned 2026-04-15

- **`showEvent` can fire multiple times.** Window hide/show cycles re-trigger it. Guard `viewport_on_surface_ready` against re-init — resize-only when `cb.viewport.is_some()`.
- **Native `QFileDialog` fights wgpu paint loop on Windows.** The 16ms paint tick on the RenderWidget keeps the main window in foreground and the native dialog opens behind. Pause the tick around the dialog (not `setVisible` — that's ugly and made the app briefly vanish).
- **GPU resources need a drain before drop.** D3D12 throws `OBJECT_DELETED_WHILE_STILL_IN_USE` if the Renderer is dropped while frames are in-flight. Call `device.poll(wgpu::Maintain::Wait)` first.
- **`SceneManager::load_usd_scene` is actually on `Renderer`, not `SceneManager`.** Lives in `bif_viewport/src/scene_loader.rs` inside `impl Renderer`. Call as `vp.renderer_mut().load_usd_scene(path)`, not `vp.renderer_mut().scene.load_usd_scene(path)`.
- **`UsdStage::get_timeline()` already exists** — no new FFI was needed for Move 4. `UsdTimelineData { start_time_code, end_time_code, frames_per_second, has_authored_time_range }` at `bif_core/src/usd/cpp_bridge.rs:756`.
- **Swap central_stack to viewport BEFORE calling `on_stage_path_opened`.** The RenderWidget needs to be visible so `surfaceReady` fires and the Renderer becomes live before `load_usd_scene` runs. Otherwise `with_viewport_mut` returns `None` and the load no-ops.

---

## Quick Status

| Status | Details |
|--------|---------|
| Released | v0.1.0, v0.11.0, v0.12.0, v0.13.0, v0.13.5, v0.13.6, **v0.14.0 (2026-04-13)** — pushed to origin |
| **Active branch** | **`v0.15-qt`** — Qt migration. `main` stays v0.14.0 shippable until Phase H merge. |
| v0.15.0 Phase 0 ✅ | wgpu-into-QWidget spike gate PASSED 2026-04-13. Qt 6.8.3 LTS + MSVC 2022 + cxx 1.0 + qt-build-utils 0.7 toolchain proven in `crates/bif_qt_spike/`. ADR-006 authored. |
| v0.15.0 Phase A ✅ | `crates/bif_qt/` scaffolding landed 2026-04-13. First real `#[cxx_qt::bridge]` — `BifShellState` QObject with 2 qproperties + 1 qinvokable. Theme port (34 colors + stylesheet generator). C++ QMainWindow assembly with 4 dock placeholders + menu bar. |
| v0.15.0 Phase B ✅ | All 8 slices. Viewport + stylesheet + menu + Zen mode + workspaces + first-launch + breadcrumb + command palette. |
| v0.15.0 Phase C ✅ | 3 panels. Layer Stack · Scene Browser · Property Inspector. |
| v0.15.0 Phase D ✅ | 3 secondary panels. Timeline · Node Graph · Render Settings. Tabification between bottom + right. |
| v0.15.0 Phase E.1 ✅ | Input stubs. Viewport mouse/camera signals, keyboard shortcuts (F/Space/Left/Right/±Shift), ShortcutRegistry with QSettings override path, real QFileDialog, QTimer-driven timeline playback, fps + realtime + loop toggles, Nuke-style 3-zone toolbar, inline Start/End range + detect-from-stage button. 9 new qproperties, 6 new invokables. |
| v0.15.0 Phase E.2-prep ✅ | `bif_viewport::Renderer` decoupled from winit (2026-04-15). See top-of-file section for full API. `bif_viewer` preserved via `create_renderer` compat helper. |
| v0.15.0 Phase E.2 moves 1/2/4/6 ✅ | Viewport → real Renderer; File/Open → real `load_usd_scene`; timeline detect → real `UsdTimelineData`; breadcrumb → `selected_prim_pathChanged`. Camera orbit/pan/zoom wired. Shared `trigger_open_stage` helper (menu + launch screen + recents). D3D12-close crash + file-dialog z-order + viewport double-init all fixed. |
| v0.15.0 Phase E.2 moves 5/7/8/9 + Close + HiDPI ⏳ | **Feature-complete, uncommitted 2026-04-16.** Pick ray-cast, real Scene Browser tree, real Property Inspector attributes + arcs, AnimatedTransform keyframes, File→Close Stage (Ctrl+W), devicePixelRatioF() threaded through the viewport bridge. `scene_browser_revision` qproperty added. 14 new invokables on BifShellState. Build + clippy clean; pending dogfood. |
| v0.15.0 ADR-007 ✅ | BifShellState ↔ ViewportCallbacks bridge locked in: **β (thread-local raw-pointer)**. `with_viewport_mut(|vp| ...)` + `with_stage(|stage| ...)` helpers. See `wiki/architecture/adr/007-shell-state-to-viewport-bridge.md`. |
| Next | **Dogfood + commit 2026-04-16 changes**, then Phase F (egui cleanup). See top-of-file "Pickup next session" for dogfood checklist. |
| Tests | ~627 total (90 new in v0.14.0) + spike has no unit tests (deletion-scheduled) |
| Performance | 60 FPS viewport, 100K instances with LOD, Ivar build ~185ms |

---

## ➡️ v0.15.0 Qt Migration — Phase E Starting Notes

**Strategy (ADR-006):** Shell-first on `v0.15-qt`. Panels one-at-a-time. Merge at Phase H.

**Binding locked:** `cxx-qt 0.7` + `qt-build-utils 0.7` + Qt 6.8.3 LTS + LGPL dynamic linking. Phase A validated cxx-qt macros. C++ owns window assembly (QMainWindow / QDockWidget / QMenuBar) — cxx-qt-lib's QtWidgets coverage is thin and Rust-side boilerplate would be pure cost without ergonomic win. Rust owns QObjects (BifShellState + future panel models).

**Phase A/B/C/D cxx-qt + Qt gotchas (for Phase E authors):**
- `#[qinvokable]` declared inside `extern "RustQt"`, IMPLEMENTED in a regular impl block OUTSIDE the bridge (non-empty impls inside the bridge = compile error).
- `CxxQtType` trait must be in scope for `.rust()` / `.rust_mut()` accessors.
- cxx-qt-generated header path: `bif_qt/src/main_window.cxxqt.h` (crate + src prefix).
- Cannot mix `#[cxx::bridge]` + `#[cxx_qt::bridge]` in same crate. `extern "Rust"` AND `extern "RustQt"` CAN coexist inside one cxx-qt bridge.
- `type Foo = path::Foo;` is rejected in `extern "Rust"` — use `type Foo;` and resolve via parent-module `use crate::module::Foo;`.
- `#![allow(clippy::missing_safety_doc)]` at crate scope required — clippy can't see bridge-macro-expanded fn docs.
- C++ Q_OBJECT headers need `CxxQtBuilder::qobject_header("path/to.h")` for moc; the `.cpp` goes in `cc_builder.file()`.
- `QKeySequence::Quit` is empty on Windows — hardcode `Ctrl+Q`.
- `extern "Rust"` opaque types pass to C++ as `Foo*` raw pointers (no UniquePtr unless explicitly boxed).
- `rust::Str` ↔ `QString`: `QString::fromUtf8(s.data(), static_cast<int>(s.size()))`.
- **Qt's `QList`/`QVector` can't hold move-only types** like `std::unique_ptr` — use `std::vector`.
- **Custom tree delegates that shift paint rects MUST also override `editorEvent`** with the same shift, otherwise checkbox hit-testing misses the visual checkbox.
- **`self: &Self` in invokable impl blocks trips `clippy::needless_arbitrary_self_type`** — use plain `&self` in the impl body even though the bridge declaration requires `self: &BifShellState`.
- **Nested private C++ struct types aren't accessible from anonymous namespaces in the .cpp** — promote to public when helper builders need them.
- **Auto-generated property-changed signals are `<snake>Changed`** — for `#[qproperty(i32, layer_state_revision)]` the signal is `layer_state_revisionChanged`. Use that, not a custom `#[qsignal]`, for triggering model refreshes.
- **cxx-qt bridge `#[qinvokable]` read-only functions require `self: &BifShellState`, NOT `&self`.** Impl bodies use `&self` normally (clippy wants that). Mutators use `self: Pin<&mut BifShellState>`.
- **When reading `.rust()` inside a `Pin<&mut Self>` invokable**, bind the `self.as_ref()` temporary to a variable before calling `.rust()`, or the borrow dangles (E0716).
- **`QGraphicsView` consumes wheel events for its built-in scroll.** Override `wheelEvent` on a `QGraphicsView` subclass, not on the wrapping `QWidget` — wheel events don't propagate.
- **`QGraphicsPathItem` is NOT a QObject** — it can't be the connect context. Use the sender (also the QObject) as the context for 3-arg connect.
- **`BifNodeGraphicsItem::moved` signal pattern:** emit from `itemChange(ItemPositionHasChanged, ...)` on a `QGraphicsObject` subclass. Wires subscribe for re-routing.

## 📂 v0.15 Phase state map

| Surface | Ships in branch | What's real | What's stub / demo |
|---|---|---|---|
| **Build + env** | `setup_qt_env.ps1`, workspace Cargo.toml | Qt 6.8.3 LTS detection via `qt-build-utils`, MSVC flags | — |
| **Shell** | `crates/bif_qt/` + `bif_qt_shell` bin | QMainWindow, menu bar, status bar, dock system, command palette (Ctrl+P), breadcrumb bar, 4 workspace presets (QSettings persistence), first-launch screen, Zen mode (Ctrl+\\), dark stylesheet | — |
| **Viewport** | `cpp/render_widget.{h,cpp}` + `src/viewport.rs` | wgpu triangle + mouse orbit/pan/zoom signals + `primPickRequested` signal | Triangle instead of real Renderer; camera input echoes status bar, doesn't drive camera |
| **Layer Stack panel** | `cpp/layer_stack_{model,widget}.{h,cpp}` | `QListView` + model, color dot delegate, mute checkbox, double-click working layer, isolation toolbar | 3-layer hardcoded demo data in `BifShellState::seed_demo_layer_stack` |
| **Scene Browser** | `cpp/scene_browser_{model,widget}.{h,cpp}` | `QTreeView` + model, hierarchical filter, selection routes to `selected_prim_path` qproperty | 10-prim hardcoded demo tree; no lazy fetch yet |
| **Property Inspector** | `cpp/property_inspector_widget.{h,cpp}` | Tabs + composition arcs group + attributes table with opinion-dot delegate | Fake attrs per prim-type, composition arcs mirror layer stack |
| **Timeline** | `cpp/timeline_widget.{h,cpp}` | Nuke-style 3-zone toolbar, custom paint ruler + playhead + keyframe diamonds, QTimer advances frames on Play, fps/RT/Loop/Start/End controls, `↻⇅` detect button | Keyframes hardcoded; detect button is a status-only stub |
| **Node Graph** | `cpp/node_graph_widget.{h,cpp}` | `QGraphicsScene` + `NodeGraphView`, 5-node demo (UsdRead→Scatter→Xform→IvarRender + HdriEnv), wheel-zoom + middle-mouse pan, bezier wires refresh on node move | Demo graph only; can't create/delete/rewire nodes (v0.16) |
| **Render Settings** | `cpp/render_settings_widget.{h,cpp}` | QFormLayout with Path Tracer (spp/depth/SHARC) + Post-Processing (exposure/gamma/OIDN) | Values local; not wired to `bif_renderer::RenderConfig` |
| **Shortcut system** | `cpp/shortcut_registry.{h,cpp}` | QSettings-override path for every shortcut declared via ID | Preferences UI (v0.16) |
| **File → Open** | `window_builder.cpp` | Real `QFileDialog`, records `QSettings("recent_stages")`, flips central stack to viewport | `BifShellState::on_stage_path_opened` doesn't actually call `scene_loader::load_usd_scene` yet |

**One-liner to understand the shape:** `BifShellState` (cxx-qt QObject in `src/main_window.rs`) is the fat singleton that every panel binds to via `#[qproperty]` + `#[qinvokable]`. It holds title / status / workspace / layer state / selection / timeline. Phase C deliberately skipped the existing `EventBus`; Phase E.2 reopens that decision.

---

## ➡️ Phase E.2 Starting Notes — Real USD Wiring (~5–8h)

**Goal:** Replace demo data with real USD reads. This is the biggest integration step in v0.15 and the hardest architectural decision (AppEvent bridge).

### 🎯 Phase E.2 — ordered first moves

1. **Pull `bif_renderer::Renderer` into bif_qt's viewport.** Today `src/viewport.rs` owns a tiny wgpu pipeline drawing a triangle. Goal: `Viewport` holds a `bif_renderer::Renderer` instead, with the same HWND-fed `wgpu::Surface`. Two flavors of this, pick one:
   - (Lightweight) Keep `Viewport` in bif_qt, delegate rendering to `bif_renderer::Renderer::render(&camera, &scene)` — needs Renderer's public API to be reachable without `bif_viewport` types.
   - (Full) Pull `bif_viewport::Renderer` (the 75-field God object) + `SceneManager` + `EventBus` into bif_qt directly, deleting the egui UI parts. This is closer to Phase F scope — might as well do it now since Phase E.2 needs it.
   - **Recommended:** the full path. Phase E.2 + Phase F essentially merge.
2. **Wire `QFileDialog` result → real stage load.** Add `BifShellState::load_stage_at_path(QString)` invokable (replacing the `on_stage_path_opened` status echo). Inside: call `bif_core::scene_loader::load_usd_scene(path)` → `SceneManager::load_scene(scene)` → update `SceneLayerState` on self → bump `layer_state_revision` so the Layer Stack panel refreshes. Drop the hardcoded demos from `seed_demo_layer_stack` + `SceneBrowserModel::seed_demo_tree`.
3. **AppEvent bridge decision + implementation.** See "AppEvent bridge options" section below.
4. **`detect_timeline_from_stage` real impl.** Call `UsdStage::GetStartTimeCode() / GetEndTimeCode() / GetTimeCodesPerSecond()` → set `start_frame`, `end_frame`, `playback_fps` qproperties. File load should also call this automatically.
5. **Gizmo raycast on LMB.** `RenderWidget::primPickRequested(x, y)` already emits. Hook it to a new Rust handler that builds a ray via the existing `bif_viewport/src/selection.rs` code → sets `BifShellState::selected_prim_path`. Scene Browser already listens to this for sync.
6. **Breadcrumb wires to `selected_prim_pathChanged`.** The `breadcrumb_set_path` helper already exists in `window_builder.cpp` — connect it to the signal.
7. **Scene Browser real data.** Replace `SceneBrowserModel::seed_demo_tree` with a proper `bif_core::CompositeProvider` traversal. For 100K+ prims use `canFetchMore` / `fetchMore` for lazy population.
8. **Property Inspector real attributes.** Replace fake attrs with `UsdPrim::GetAttributes()` via new Rust invokables on `BifShellState`. Composition arcs via `UsdStage::get_prim_stack`.
9. **Timeline keyframes real.** Pull from the selected prim's `AnimatedTransform` keyframes.

### 🏛️ AppEvent bridge — the open decision

`bif_viewport::EventBus` + `AppEvent` + `dispatch_events` in `bif_viewport/src/*_dispatch.rs` are the existing dispatch system. Phase C–E.1 sidestepped it by mutating `BifShellState` directly. Real USD operations need dispatched handling (stage load → geometry extraction → GPU upload → scene tree refresh → selection reset → etc.). Three candidates:

| Option | Pattern | Pros | Cons |
|---|---|---|---|
| **(a) Global `Mutex<VecDeque<AppEvent>>`** polled per-frame | Rust singleton queue; panel invokables `push(AppEvent)`; a `QTimer` tick in `window_builder.cpp` drains + dispatches | Simple; fits cxx-qt pattern (invokables don't need extra state); drain can piggyback on paintEvent tick | Global state; harder to test; lock contention on high-frequency events (mouse drags) |
| **(b) cxx-qt QObject wrapping `EventBus`** passed into each panel | New `BifEventBus : QObject` with `#[qinvokable] fn push(event)`; held as child of `BifShellState`; panels access via `state->event_bus()` | Testable via dependency injection; clean ownership; fits Qt idioms | `AppEvent` is a Rust enum with heterogeneous payloads — bridging enum through cxx-qt is awkward, likely need a tagged-struct proxy |
| **(c) Per-panel Qt-native signals → Rust trampolines** | Each panel emits typed signals (`layerMuteToggled(int)`, `primSelected(QString)`, …); C++ signal connection calls Rust free fn that builds `AppEvent` + pushes to EventBus | Clean separation; panels don't know about AppEvent; easy incremental migration | Boilerplate trampolines — one per AppEvent variant (~28 variants); lots of small wiring |

**Recommendation: (a) + incremental (c) for high-frequency events.** Start with (a) — simplest plumbing, works immediately. Mouse drags (camera input, ruler scrub) that would churn through the queue can bypass via direct state writes (already the case in E.1). When the global queue hurts, migrate the hot paths to (c) per-panel signals. (b) is theoretically cleanest but the enum-bridge cost isn't worth it.

### 🗺️ Phase E.2 code map — where to look in the existing codebase

| Looking for | File |
|---|---|
| Scene load flow | `bif_viewport/src/scene_loader.rs::load_usd_scene` |
| SceneManager / Scene | `bif_core/src/scene.rs`, `bif_viewport/src/scene_manager.rs` |
| EventBus + AppEvent | `bif_viewport/src/event_bus.rs`, `bif_viewport/src/app_event.rs` |
| Dispatch modules | `bif_viewport/src/{render,node_graph,selection,project}_dispatch.rs` |
| Camera state + sensitivity | `bif_viewer/src/main.rs` (`ORBIT_SENSITIVITY`, `PAN_SENSITIVITY`) |
| Ray-cast / prim selection | `bif_viewport/src/selection.rs` |
| Renderer God object | `bif_renderer/src/lib.rs` or the renderer struct module |
| Existing egui panels (for reference) | `bif_viewport/src/{layer_stack_panel,scene_browser,property_inspector,timeline,render}.rs` |
| Timeline state + animated transforms | `bif_core/src/timeline.rs`, `bif_core/src/animation.rs` (look around `AnimatedTransform`) |

### 🏗️ Phase E.2 deliverables checklist

- [ ] `bif_qt::Viewport` renders real scenes (not a triangle)
- [ ] `File → Open` loads actual USD stage
- [ ] `BifShellState::scene_layer_state` populated from load, not demo seed
- [ ] Scene Browser shows real prim tree via CompositeProvider (+ lazy fetch for 100K+)
- [ ] Property Inspector shows real attributes via `UsdPrim::GetAttributes()`
- [ ] Composition arcs from `UsdStage::get_prim_stack`, not layer-stack mirror
- [ ] Timeline keyframes from selected prim's `AnimatedTransform`
- [ ] `detect_timeline_from_stage` reads real USD time metadata
- [ ] Gizmo raycast via `selection.rs` on LMB click
- [ ] Breadcrumb connected to `selected_prim_pathChanged`
- [ ] AppEvent bridge chosen + implemented
- [ ] `bif_qt_shell` loads `test_assets/layers/root.usda` end-to-end — Layer Stack shows 3 real layers, mute works, viewport updates

### ⚠️ Phase E.2 risk: scope overlap with Phase F

Phase E.2 as described is most of Phase F ("delete egui") bundled in. Two paths:
1. **Merge them** — Phase E.2 pulls Renderer/SceneManager/EventBus into bif_qt, egui-specific panel files in `bif_viewport` are deleted as we replace each one. Effectively the endgame of the migration.
2. **Keep separate** — Phase E.2 exposes Renderer/SceneManager via a new `bif_runtime` crate (or similar) that both `bif_qt` and `bif_viewport`'s legacy egui code depend on. Phase F then deletes `bif_viewport`'s egui parts.

**Recommendation:** Path 1. Path 2 duplicates effort.

---

## ✅ Rigid Mesh Offset Bug — FIXED (Apr 12, 2026)

**Root cause:** `SkinKind::Rigid` compression in `crates/bif_core/src/usd/loader.rs` assumed `element_size == 1, weight == 1.0`, but USD's `IsRigidlyDeformed()` is broader — it returns true for any per-prim binding, including multi-bone uniform influence (hair with 3 head/neck bones at w=0.333, fingernails with 2 tip bones at w=0.5). Taking only `joint_indices[0]` + `joint_weights[0]` collapsed each vertex by the fractional weight, visually shrinking the mesh toward its first bone.

**Fix:** loader now gates the compact `SkinKind::Rigid` path on `element_size == 1`. Multi-joint rigid meshes broadcast the single authored block across post-split vertices and flow through `SkinKind::PerVertex`.

**Regression guard:** new `skinning::tests::rigid_matches_pervertex_single_influence` — asserts `SkinKind::Rigid{J, 1.0}` produces identical output to `SkinKind::PerVertex{[J;N], [1.0;N], 1}` over a non-trivial palette.

**Diagnostic trail:** Python dump of `HumanFemale.walk.usd` via `UsdSkelSkinningQuery::ComputeJointInfluences` + joint-path resolution revealed the multi-joint rigid pattern (hair `elem=3, w=0.333`; nails `elem=2, w=0.5`; eyes/shoes `elem=1, w=1.0`). Single-joint meshes were unaffected — explains why shoes/eyes partially worked while hair/nails were visibly offset.

**Bug was pre-existing since v0.13.5.2** (commit `b61264e` introduced the compression). Not a v0.13.6 regression.

---

## Recent Work

### v0.13.6-dev Apr 12: Rigid Mesh Offset Bug Fixed (Apr 12, 2026)

- **Root cause:** `SkinKind::Rigid` compression in `bif_core/src/usd/loader.rs` assumed `element_size == 1, weight == 1.0`, collapsing multi-joint rigid bindings (hair `elem=3, w=0.333`; nails `elem=2, w=0.5`) to a single fractional-weight influence. Kernel then did `M·p·0.333`, visually shrinking each vertex toward its first bone's origin.
- **Fix:** loader gates the compact `SkinKind::Rigid` path on `element_size == 1`. Multi-joint rigid meshes now broadcast the single authored block across post-split vertices and flow through `SkinKind::PerVertex`.
- **Regression guard:** new `skinning::tests::rigid_matches_pervertex_single_influence` — locks `SkinKind::Rigid{J, 1.0}` to match equivalent `PerVertex{[J;N],[1.0;N], 1}`.
- **Diagnostic ladder:** (1) math equivalence test passed → kernel correct, bug upstream. (2) Python `ComputeJointInfluences` dump on `HumanFemale.walk.usd` revealed hair/nails are multi-joint rigid with fractional uniform weights — not the single-joint rigid the compression assumed.
- **Validated visually** on full HumanFemale walk cycle; hair, eyes, fingernails all in correct positions.
- Previous investigation notes:
  - Bisected v0.13.6 blend shape code via `#if 0` → ruled out as cause
  - C++ debug logging in `cache_skeleton_data` → verified loading data correct
  - Worktree A/B on commit `b61264e` → confirmed pre-existing v0.13.5.2 bug
  - Debug artifacts committed (`68c42e0`)
  - Removed `ARCHITECTURE_REFACTORS.md` (campaign closed, kept `ARCHITECTURE_REVIEW.md`)

### v0.13.6-dev Apr 11: UsdSkelBlendShape Implementation (Apr 11, 2026)

- **Full CPU blend shape pipeline** — C++ FFI (dense-expand at load, shape-order remap, per-frame `ComputeBlendShapeWeights` via cached `UsdSkelAnimQuery`), Rust FFI layer, `BlendShapeTarget`/`BlendShapeBinding` on `Mesh`, `apply_blend_shapes()` in skinning module, loader integration, per-frame playback hook (both inline and multi-draw paths).
- **Pipeline order:** blend shape deltas applied to `bind_positions` → scratch buffer → fed into `skin_positions`/`skin_normals`. Handles shapes-only meshes (no skin) and shapes+skin composition.
- **Test asset:** `two_bone_arm.usda` extended with 2 BlendShape prims (`squash`/`twist`) + animated weights over frames 0-36.
- **GPU stub:** `GpuBlendShapeLayout` in `bif_renderer` reserves data layout for future GPU path.
- **6 new unit tests** — all pass. Build + clippy clean.
- **TODO:** Manual validation on `HumanFemale.walk.usd` (has blink/face blend shapes per user). Wiki concept note. Version bump + release.

### v0.13.6-dev Apr 11: Architecture Refactor Campaign Closed (Apr 11, 2026)

- **Tracking docs synced.** `ARCHITECTURE_REFACTORS.md` phases 2-5 flipped from "Not started" → Complete with commit refs. `ARCHITECTURE_REVIEW.md` §10 gained a Status column; §2/§4/§9/§12 got resolution callouts. Both docs now archival.
- **Final state:** all 5 refactor phases + 7 of 8 review items shipped across v0.13.0-v0.13.5. ~79 new tests from the campaign (44 ffi_convert + 19 eval + 16 scene_pipeline). Remaining #4 (node graph extension checklist) shipped in `wiki/architecture/node-graph-system.md` as a terse 10-step reference card.
- **Phase 4.5 logged as deferred:** `scene_loader.rs` grew 2035 → 2413 LOC after Phase 4 (pipeline layer was additive, not a replacement). Trigger to resume: v0.14.0 layer-aware rewrite touching `finalize_usd_scene()`.
- **Test string cleanup:** `persistence.rs` `path_relativization_*` tests now use `#[cfg(windows)]` / `#[cfg(not(windows))]` constants instead of hardcoded `D:\\projects\\...` literals. `sample_project()` file_path dropped the `D:\\` prefix. 12/12 persistence tests green.
- **Pre-existing clippy breakage noted:** `cargo clippy --workspace -- -D warnings` fails with 56 errors (44 bif_core + 11 bif_viewport + 1 bif_perf) from a clippy version bump (`rust-1.92.0`). Confirmed unrelated to session via stash/repro against HEAD `b61264e`. Logged as separate follow-up. `cargo build` and `cargo fmt --check` are clean.

### v0.13.5 Apr 10: UsdSkel Import Complete (Apr 10, 2026)

- **All 4 phases done:** C++ SkelCache refactor, Mesh::skin wiring, CPU LBS module (8 unit tests), per-frame anim eval + viewport hookup. Plus 6 follow-up bugs fixed during HumanFemale validation: multi-draw skinning path, per-mesh joint-order remap, UV-seam vertex expansion, rigidly-deformed mesh broadcast, SkelRoot world xform override, and skipping per-frame xform animation for skinned meshes.
- **HumanFemale.walk.usd** loads coherent, all 77 skinned prototypes deform, walk animation plays correctly via the joint deformation pass. Hair, buttons, shoes all in correct positions.
- **New files:** `crates/bif_core/src/skinning.rs`, `wiki/usd/usdskel-import.md`, `test_assets/skel/two_bone_arm.usda`.
- **Tooling:** plumbed `skel_root_world_xform[16]` through 5 layers (C++ struct → header → ffi_raw → ffi_convert → cpp_bridge wrapper → loader). Multi-draw skinning path mirrors `update_vertex_animation`'s structure.
- **Remaining for release:** version bump `0.13.5-dev → 0.13.5`, MILESTONES.md Released section, release commit.

### v0.13.0 Apr 7: UsdStage Sync Fix + Architecture Audit (Apr 7, 2026)

- **Architecture audit** — reviewed ARCHITECTURE_REVIEW.md (5/8 done) and ARCHITECTURE_REFACTORS.md (3/5 phases complete). Mapped remaining work.
- **UsdStage Sync fix** — removed `unsafe impl Sync for UsdStage`, wrapped in `Arc<Mutex<UsdStage>>`. 10 files, ~20 callsites. Borrow-checker conflicts resolved with guard extraction and pre-extraction patterns.
- **setup_usd_env.sh** — added bin/usd plugin scan for PS1 parity.
- **Remaining:** SceneQuery API (bif_core trait), dispatch split (render/selection/project), Phase 2 Linux gaps.

### v0.13.0 Apr 6: Obsidian Knowledge Base (Apr 6, 2026)

- **wiki/ vault** — 42 articles across 8 sections (architecture, USD, rendering, concepts, rust, ui-ux, journal, raw). LLM-optimized indexes for Q&A. 4 templates (concept, adr, journal, article).
- **Devlog backlinks** — 92 devlog entries get `## Wiki Links` sections with Obsidian wikilinks
- **bif-commit updated** — Step 6 maintains wiki on each commit
- **CLAUDE.md updated** — Knowledge Base section with conventions

### v0.13.0 Apr 5: CPU Displacement + Selection Outline + Sync (Apr 5, 2026)

**Session 2 — CPU Vertex Displacement:**

- **CPU vertex displacement** — `displacement.rs` module: post-load pass samples heightmap per vertex, offsets along normal (USD 0.5 neutral). Bilinear sampling, sync image loader (PNG/JPG/EXR/TIF), `Mesh::recompute_bounds()`. Works in both viewport + Ivar. 14 unit tests.
- **C++ bridge MaterialX displacement fallback** — after MaterialX extraction, checks `GetSurfaceOutput()` for UsdPreviewSurface `inputs:displacement`. Handles Houdini's auto-generated preview shaders.
- **Test asset** — `displacement_test.usda` with manually patched UsdPreviewSurface displacement wiring (Houdini only generates MaterialX side).

**Known issues:**

- Dome light from Houdini USD not detected (needs investigation)
- MaterialX `ND_displacement_float/vector3` not natively extracted (workaround: UsdPreviewSurface fallback)

**Session 1 — Selection Outline + Tree/Viewport Sync:**

- **Selection outline rendering** — Replaced buggy `PolygonMode::Line` + `shading_mode` `queue.write_buffer` hack with dedicated `shaders/outline.wgsl`: normal-expanded back-face silhouette. Pipeline uses `cull_mode: Front` + `depth_compare: LessEqual` so only protruding rim passes depth test → clean Houdini-style silhouette. Dedicated `wireframe_cam_bind_group` with `shading_mode=2` baked in, updated per-frame.
- **Bidirectional tree ↔ viewport sync** — New `Renderer::select_at_screen()` handles viewport click flow (pick + set index + emit `PrimSelected` + reset gizmo + deselect on empty). `PrimSelected` handler now updates both `selected_prim_path` AND `scene_browser_state`, calls `expand_to_path()` to auto-reveal collapsed branches.
- **Robust prim_path lookup** — 3 fallbacks in `PrimSelected` handler: exact match → descendant prefix (parent Xform clicks) → synthetic `/BIF/{path}` prefix (handles empty `inst.prim_path` cases where `resolve_prim_path` synthesizes paths from proto names). `denormalize_synthetic_path()` strips `/BIF/` prefix + numeric `/{idx}` suffix for viewport → tree direction.
- **Viewport bounds guard** — `select_at_screen` early-returns on UI panel clicks so tree row clicks don't trigger deselect.
- **Dark-theme tree polish** — Removed green node-source highlight; only selected row painted. Fixed premultiplied-vs-unmultiplied alpha bug (`from_rgba_unmultiplied(74, 144, 217, 75)`). `selectable_label(false, ...)` prevents double-painting.

**Next priorities:**

1. Dome light bug — Houdini USD dome light not detected by C++ bridge
2. Native MaterialX displacement in C++ bridge (`ND_displacement_float/vector3`)
3. Embree displacement dicing (`rtcSetGeometryDisplacementFunction` callback)
4. Curves in Ivar (ribbon tessellation for BasisCurves)
5. OpenVDB volume rendering

### v0.13.0 Sessions Apr 2-4: Subdiv, Inspector, Display Color, Variants, Selection (Apr 4, 2026)

**Completed:**

- **Subdivision rendering** — Embree 4 Catmull-Clark with smooth limit-surface normals via rtcInterpolate (dPdu×dPdv). Tessellation rate 8. Fixed RTCBufferType enum values. Pre-UV-split positions via `vertices_orig` FFI.
- **USD attribute inspector** — Attributes tab in property panel, C++ bridge `usd_bridge_get_prim_attributes()`, primvars with interpolation.
- **Display color** — `primvars:displayColor` flows through pipeline to vertex color. ShadingMode toggle (Textured/DisplayColor).
- **Variant set UI** — Dropdowns in Attributes tab, `set_variant_selection()` + scene reload on change.
- **Selection sync** — Tree click maps prim_path → instance_index for viewport highlight. F to frame selected.
- **Displacement foundation** — Texture path + scale flows through FFI/Material. No vertex displacement yet.
- **Bug fixes** — Camera persistence, HDRI show_background, UNC path stripping, code review fixes (4 critical).

**WIP / Known Issues:**

- **Variant reload** — Currently does full file reload instead of re-extracting from live stage. Works but slow on large scenes. UNC path fix applied.
- **USD loader leaves `inst.prim_path` empty** for some load paths (observed on lucy.usd) — workaround via synthetic `/BIF/` path fallbacks in selection handler.

### v0.13.0 Phase 1: Bug Fixes + Subdivision Wiring (Apr 2, 2026)

- **v0.13.0 scope expanded** — from subdiv+displacement to full USD compatibility including OpenVDB. 5-phase plan (~70-108 hrs, target late May-mid June)
- **Camera persistence bug fixed** — reset viewport/batch camera source on new scene load
- **HDRI background toggle fixed** — removed is_loaded guard (blocked auto-loaded DomeLight HDRIs), added hdri_show_background to IvarState/RenderConfig/renderer
- **OCIO ACES** — verified already active in shader (Hill/Narkowicz approx), full OCIO deferred
- **Subdivision wired to Embree** — SubdivInfo preserves polygon topology through MeshData pipeline, Ivar passes SubdivData for Catmull-Clark limit surface. Single-mesh scenes only for now.
- **Plan file:** `.claude/plans/sharded-moseying-hickey.md`

### Qt UI Spec §17-25 + Wireframe Selection (Apr 4, 2026)

- `UI_DESIGN.md` extended from 16→25 sections: context menus, multi-select, undo feedback, long-op progress, reduced-motion accessibility, tree filter, error states, workspace storage, cxx-qt decision
- Click target spec corrected (24px→32px rows, 44px toolbar); node label 10px→12px
- `cxx-qt` decided as Qt/Rust binding strategy; drag-and-drop deferred to v0.16.0
- Wireframe selection overlay committed — `POLYGON_MODE_LINE` pipeline + `VariantChanged`/`FrameSelected` `AppEvent` variants

### Qt UI Design Spec Consolidation (Apr 1, 2026)

- `docs/ux/UI_DESIGN.md` promoted to single authoritative Qt UI spec (16 sections, ~730 lines)
- UX Architect + UX Researcher reviews conducted on batch 0 Stitch mockups, findings incorporated
- 14 Stitch mockups across 2 batches covering all 4 workspaces + first launch screen
- Key additions: Bjorn asset manager, active layer safety system, vertical code split layout (preferred), opinion encoding table, command palette details, Render workspace (renamed from Review), first launch onboarding
- 4 remaining mockup gaps: context menu, error states, 15+ node graph, tooltip design
- Doc hierarchy: UI_DESIGN.md (spec) + DCC_UI_RESEARCH.md (research) + DESIGN.md (tokens) + reviews (audit trail)

### MaterialX File Format Support (Mar 31, 2026)

- Rebuilt vcpkg USD 25.11 with `materialx` feature — adds `usdMtlx` plugin for `.mtlx` file references
- C++ bridge: usdMtlx plugin detection at startup, `resolve_mtlx_input()` follows Material interface connections, deep descendant shader search by `info:id`, refactored duplicated extraction into shared helper
- `setup_usd_env.ps1` scans both `bin/usd` and `lib/usd` for plugin resources
- **WIP**: Scalar values from Material interface inputs not resolving yet — needs debugging (textures load fine)
- **WIP**: Normal maps may not work correctly with composed MaterialX structure

### UI/UX Design Brainstorm (Mar 30, 2026)

- Designed viewport-dominant T-layout, layer color coding system, opinion stack, command palette
- Full design: `docs/ux/UI_DESIGN.md` | Research: `docs/ux/DCC_UI_RESEARCH.md`
- Updated MILESTONES.md + ROADMAP_DETAIL.md with UI features threaded into v0.14–v0.16
- Material editor designed: param sheet + node graph + floating lookdev orb ([design](docs/ux/MATERIAL_EDITOR_DESIGN.md))

### Power-Weighted Light Sampling (Mar 30, 2026)

- Replaced uniform 1/N light selection with power-weighted CDF in `LightList`
- Added `power()` to `Light` trait (DistantLight, SphereLight, RectLight)
- Foundation for hierarchical light tree (v0.20.0) and env map visibility cache (v0.22.0)
- Scoped two Octane-inspired features: many-light sampling + env visibility cache

### SHARC Cache + Max Depth UI (Mar 30, 2026)

- SHARC radiance cache now skips low-roughness surfaces (< 0.1) via new `Material::roughness()` trait — fixes blurred reflections on glossy/mirror materials
- Added Max Depth slider (1–32) to interactive Ivar render panel
- Roadmap trimmed: removed AI Integration version, renumbered

### GitHub Pages Site (Mar 30, 2026)

Set up mdBook-based site with auto-deployed dev diary (85 entries) + manual (USD reference, getting started, architecture, changelog). `scripts/generate-site.sh` auto-generates SUMMARY.md from devlog tree. GitHub Actions deploys on push. **Action needed:** enable Pages source = "GitHub Actions" in repo settings.

### PointInstancer Loading Fixes (Mar 30, 2026)

Fixed two bugs preventing time-sampled PointInstancer files (e.g., Pixar's PointInstancedMedCity.usd) from loading:

1. C++ bridge now uses stage startTimeCode / first sample instead of Default when reading instancer attrs
2. Rust loader maps parent Xform paths in prototype_map for instancer prototype resolution

Test file: `assets/PointInstancedMedCity.usd` (40K instances, 8 prototypes)

### Architecture Deepening: Phase 1 FFI Bridge Split (Mar 28-29, 2026)

Split monolithic `cpp_bridge.rs` (4,542 LOC) into 3 modules:

- `ffi_raw.rs` (898 lines) — `#[repr(C)]` types + `extern "C"` block
- `ffi_convert.rs` (2,054 lines) — 17 conversion functions + 44 tests (no C++ DLLs needed)
- `cpp_bridge.rs` slimmed to 3,651 lines (-20%)

Also created `ARCHITECTURE_REFACTORS.md` (5-phase plan) and `BIF_USD_WORKFLOW.md` (layer-aware editor spec).

**Next:** Wire UsdStage methods to delegate to ffi_convert (incremental), then Phase 2 (Linux cross-platform).

### Documentation Overhaul (Mar 27, 2026)

Reworked project documentation to correlate milestones with semantic versioning:

- **MILESTONES.md** — rewritten as lean semver roadmap (v0.13.0 through v0.23.0+)
- **MILESTONES_HISTORY.md** — new file, all completed milestones (M0-M31) moved here
- **ROADMAP_DETAIL.md** — new file, per-version task lists + acceptance criteria
- **README.md** — full rewrite, new positioning ("lightweight scene assembly"), updated stats
- **CHANGELOG.md** — targeting v0.13.0 note
- **Cargo.toml** — version bumped to 0.13.0-dev
- **bif-commit skill** — updated for new file structure
- **vfx-code-reviewer agent** — added version scope awareness

Key decisions informed by software architect + engineer reviews:

- Qt migration (v0.15.0) promoted before context system — avoids building UI twice
- M22/M25/M27 no longer deferred — all scheduled in roadmap
- 1.0 criteria defined (10 gates)

### M30 Complete (Mar 24-26, 2026)

All 6 phases landed: serde foundation, ProjectFile persistence, file menu + save/load UI, eval modes (Auto/Manual/OnMouseRelease), cache node with bypass toggle.

### M31 Complete (Mar 26, 2026)

Per-node scene graph visualization — source node tagging, prim count badges, filtered provider.

### M29.5 Complete (Mar 23, 2026)

egui UI overhaul — centralized theme, panel restructure, property inspector, menu bar, Unicode icons.

---

## 🎨 User preferences captured this migration

- **Ease-of-use over modal dialogs.** Inline controls on toolbars beat "Global Animation Options" popups. Timeline range/fps/loop/RT live on the toolbar, not a menu.
- **Nuke-inspired layouts** for timeline-style UIs. 3-zone toolbar (config / transport+counter / range) landed this session.
- **"Put our spin on it"** — take DCC conventions as input, not as specification. Loop toggle is a boolean now; grow to Repeat/Bounce/Stop/Continue enum when complexity justifies it.
- **Plan for customizability early.** User wants rebindable shortcuts — the ShortcutRegistry pattern (string-ID + QSettings override) lands in advance of the v0.16 Preferences dialog.
- **Auto-detect from USD by default, manual override available.** Timeline detects `timeCodesPerSecond` from the stage; user can override via the fps spinbox. Same shape for anything stage-derived.

---

## 🛠️ v0.15 Phase A–E.1 recent work

### Phase E.1 — Input + event wiring (session 8, `759a01f`)

- `RenderWidget` mouse input: Alt+LMB orbit / MMB pan / wheel zoom / unmodified LMB = primPickRequested. Signals forwarded through 4 new `on_camera_*` / `on_frame_selected` invokables on `BifShellState` (stubs until Phase E.2).
- `ShortcutRegistry` (new `cpp/shortcut_registry.{h,cpp}`) with string-ID lookup + `QSettings("shortcuts/<id>")` override path. Wired F, Space, Left/Right, Shift+Left/Right.
- Real `QFileDialog` on File → Open; writes `QSettings("recent_stages")` (max 10), flips central stack to viewport.
- QTimer-driven timeline playback with configurable fps, real-time mode, and loop toggle. Nuke-style 3-zone toolbar (config / transport+orange frame counter / range + detect).
- New qproperties: `playback_fps`, `realtime_playback`, `loop_playback`. New invokables: `jump_to_prev_keyframe`, `jump_to_next_keyframe`, `detect_timeline_from_stage` stub, `on_stage_path_opened`.

### Phase D — Secondary panels (session 7, `109d2d3`)

- Timeline (`cpp/timeline_widget.{h,cpp}`): custom `paintEvent` for ruler + playhead + keyframes. Scrub via mouse.
- Node Graph (`cpp/node_graph_widget.{h,cpp}`): `QGraphicsScene` + `NodeGraphView` subclass for wheel-zoom + middle-mouse pan. `BifNodeGraphicsItem : QGraphicsObject` with category-colored headers, pin lollipops. 5-node demo.
- Render Settings (`cpp/render_settings_widget.{h,cpp}`): `QFormLayout` in two styled `QGroupBox`es.

### Phase C — Core panels (session 6, `b859c2c`)

- Layer Stack (`cpp/layer_stack_{model,widget}.{h,cpp}`): `QListView` + `QAbstractListModel` reading `SceneLayerState` via 11 new `#[qinvokable]` methods. Color dot delegate + editorEvent-aligned checkboxes.
- Scene Browser (`cpp/scene_browser_{model,widget}.{h,cpp}`): `QTreeView` + `QAbstractItemModel` + hierarchical filter. Selection routes to `selected_prim_path`.
- Property Inspector (`cpp/property_inspector_widget.{h,cpp}`): QTabWidget + collapsible composition arcs + attribute table with opinion-dot delegate.
- New qproperties: `layer_state_revision` (bumps on mutation → triggers model refresh via `*Changed` signal), `selected_prim_path`, `selected_prim_type`.

### Phase B — Qt shell (sessions 4–5, `7499faa` + `1776fdc`)

- Viewport embedded (B.1), stylesheet (B.2), menu invokables (B.3), Zen mode Ctrl+\\ (B.4), workspace switcher with QSettings persistence (B.5), first-launch welcome screen (B.6), breadcrumb `QToolBar` (B.7), command palette Ctrl+P (B.8).
- Central layout: `QWidget(QVBoxLayout(breadcrumb, QStackedWidget(first_launch, viewport)))`.

### Phase A — `bif_qt` scaffolding (session 3, `8ce4271`)

- New `crates/bif_qt/`. First real `#[cxx_qt::bridge]` in BIF. `BifShellState` QObject with 2 qproperties + 1 qinvokable. 34-color theme port + Qt stylesheet generator. C++ window assembly with menu bar + 4 dock placeholders.

### Phase 0 — wgpu-into-QWidget spike (session 2, `c2440db`)

- `crates/bif_qt_spike/` proved wgpu + Qt coexistence on MSVC 2022 + Qt 6.8.3 LTS. ADR-006 authored. Toolchain validated (cxx + qt-build-utils + cc + moc). Deletion scheduled for Phase H.

---

## Next Steps (v0.15.0)

1. **Phase E.2 — Real USD wiring** (~5–8h). See checklist above. This is the big integration step.
2. **Phase F — egui cleanup** (~2h, may merge with E.2). Delete `egui` + `egui-wgpu` + `egui-winit` + `egui-snarl` from `bif_viewer` + `bif_viewport` Cargo.toml. Delete `bif_viewport::run_egui_frame` and the ~900 lines of panel-assembly code. Rename `bif_viewer`'s main to target `bif_qt::run`.
3. **Phase G — tests + validation** (~2h). Unit tests on Qt models (headless — no QApplication needed for model-only tests with mocks). Manual checklist from plan: load `test_assets/layers/root.usda`, mute shot.usda + anim.usda behaviors, HumanFemale.walk.usd scrub, workspace switch, command palette, Zen mode, project save/close/reopen.
4. **Phase H — release plumbing + tag** (~1h). Bump `0.15.0-dev → 0.15.0`. Promote CHANGELOG `[Unreleased]` → `[0.15.0]`. MILESTONES v0.15.0 → Released. Wiki post-mortem at `wiki/ui-ux/qt-migration.md`. Update ADR-006 with Phase A–G learnings. Release commit + `git tag -a v0.15.0`. Merge `v0.15-qt` → `main`.

## Follow-up debts across phases (v0.15.5 / v0.16)

- Command palette: fuzzy scorer (skim or inline), widen beyond menu commands to prims/layers/nodes via provider interface.
- Node Graph: can't create/delete/rewire nodes (v0.16). Orthogonal/manhattan wire routing option (TODO marker in `BifNodeWire::refresh`).
- Timeline: loop mode enum upgrade (Repeat/Bounce/Stop/Continue); consider scrub slider under the frame counter.
- Breadcrumb segment styling (placeholder-only as of Phase B.7; Phase E.2 populates).
- Preferences dialog (v0.16) — enumerates `ShortcutRegistry::registered_defaults()` with `QKeySequenceEdit` per entry.
- `BifShellState` is a fat singleton — v0.16 may split into `BifShellState` / `BifSceneState` / `BifSelectionState` if the invokable surface grows past maintainability.
- Theme polish per `docs/ux/UI_DESIGN.md` (25 sections) — Phase G spends time here; post-v0.15 iteration continues against Stitch mockups in `assets/stitch_bif_ui_01/`.
- Bjorn opinion stack, Material Editor (v0.21), rich per-panel UX per UI_DESIGN.md — future releases.

---
