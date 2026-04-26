# Finish The Qt UI — Execution Handoff

Date: 2026-04-22
Source plan: `C:\Users\brandon\.claude\plans\ok-we-got-qt-proud-flute.md`

## Goal

Close out the v0.15.0 Qt migration followups and land the v0.16.0 UI surface (editable USDA panel, material param sheet, undo/redo feedback, workspace presets) plus a Qt trigger for Ivar render, so BIF becomes a real layer-aware USD editor through the Qt shell. Register the deferred Graphite polish pass as its own milestone (v0.16.5) so styling stays off the critical path.

## Scope

In scope (five commits, sequential):

- **C0** — Add `v0.16.5 — Qt Polish (Graphite)` entry to `MILESTONES.md` and cross-reference it in `SESSION_HANDOFF.md` + `FEATURES.md`. No code change.
- **C1 — Foundations** — undo/redo UI wired to existing `bif_core::UndoStack`; real `SdfLayer::PermissionToEdit()` FFI replacing the writable-sublayer heuristic; node graph dock tab hidden by default behind an experimental flag.
- **C2 — Quick wins** — selection outline width + color uniform wired to Render Settings; drag-and-drop USD open on main window; opinion-stack full hover tooltip on property rows; Ivar Render button + status-bar progress in Render Settings / `Render` menu.
- **C3 — Navigation** — `View → Look Through…` camera picker populated from `usd_camera_count()`; orthographic toggle; workspace presets (Assembly / Lighting / Materials / Review) persisted via QSettings with per-preset payload policy.
- **C4 — Editing (superseded)** — superseded by `docs/agent-handoffs/2026-04-24-v0.16-c4a-edit-foundation.md`. C4 is now split into C4a foundation (edit history, working-layer FFI, save, variant fix) and C4b UI features (editable USDA panel, material sheet, shading model dropdown).

## Files Or Modules

**Docket (C0):**
- `MILESTONES.md`
- `SESSION_HANDOFF.md`
- `FEATURES.md`

**Qt shell (all commits):**
- `crates/bif_qt/src/main_window.rs` — `BifShellState` qobject; ~10 new qinvokables, new qproperties (`can_undo`, `can_redo`, `outline_width`, `outline_color`, `material_inputs`, ...)
- `crates/bif_qt/cpp/window_builder.cpp` — menus (Edit undo/redo, View Look Through + Workspace, Render Ivar), toolbar workspace dropdown, dock gate for node graph, drag-drop event filter
- `crates/bif_qt/cpp/property_inspector_widget.cpp` — new Material Sheet tab, opinion tooltip on rows
- `crates/bif_qt/cpp/render_settings_widget.cpp` — outline width spinbox, outline color picker, Ivar Render button
- `crates/bif_qt/cpp/scene_browser_widget.cpp` — alt drop target
- `crates/bif_qt/cpp/node_graph_widget.cpp` + `.h` — hide-by-default plumbing

**Renderer + shaders:**
- `crates/bif_viewport/src/shaders/outline.wgsl:39` — replace `OUTLINE_SIZE` const with `OutlineParams` UBO
- `crates/bif_viewport/src/lib.rs:~632` — outline pipeline bind group update
- `crates/bif_viewport/src/ivar_build.rs:737` — `start_ivar_render()` (consumed by new qinvokable)

**Core + FFI:**
- `crates/bif_core/src/undo.rs` — existing `UndoStack` (lines 211–272); wire to UI only
- `cpp/usd_bridge/usd_bridge.cpp` + `.h` — add: `usd_bridge_layer_permission_to_edit`, `usd_bridge_layer_export_as_string`, `usd_bridge_layer_import_from_string`, `usd_bridge_parse_usda`, `usd_bridge_prim_get_bound_material_inputs`
- `crates/bif_core/src/usd/ffi_raw.rs` + `crates/bif_core/src/usd/cpp_bridge.rs` — safe wrappers for the new FFI
- `crates/bif_qt/src/main_window.rs:137 pick_strongest_writable_sublayer` — replace heuristic with real PermissionToEdit check

## Constraints

- **Function before form** — no styling, no design-system work, no color palette changes. The Graphite pass is explicitly scheduled at v0.16.5 after this lands.
- **Commit batching must be respected** — C0 → C1 → C2 → C3 → C4, in order. C4 depends on C1 (undo + PermissionToEdit). Other parallelism is OK within a commit.
- **No Renderer God-object cleanup** — leave the ~75-field `Renderer` struct alone; `display_settings` is already `pub`.
- **USDA panel scope is edit-target layer only** — not full composed stage, not per-prim snippet. Serialize via `usd_bridge_layer_export_as_string` of the active edit target, parse user text into a scratch anonymous layer, diff, commit as one `EditOperation::ReplaceLayerContents`.
- **Lookdev orb is OUT** — deferred to v0.17.
- **Node graph widget stays in-tree** but hidden; do not delete or relocate the C++ files.
- **USD env required for bif_core tests** — run with `. .\setup_usd_env.ps1` and `--test-threads=1` per `CLAUDE.md`.
- **Qt env required for full workspace build** — `. .\setup_qt_env.ps1` before `cargo build` / clippy / fmt, per recent handoffs.
- **No `--no-verify`, no `--amend` on previously pushed commits, no force push to main.** Conventional commit style matches `0322831`, `770e77d`, `9caa2c5` — terse imperative subject, optional scope.
- **Target dir:** desktop uses `C:/Users/brandon/.cargo-target` (dual-machine SMB-avoidance setup already configured).
- **Commit messages must sacrifice grammar for brevity** per project CLAUDE.md.

## Checks To Run

Per commit:

```bash
. .\setup_qt_env.ps1
cargo build
cargo clippy -- -D warnings
cargo fmt --check
cargo test -p bif_math
cargo test -p bif_renderer
cargo test -p bif_viewport
cargo test -p bif_viewer

# USD-dependent tests (bif_core requires USD env + single thread)
. .\setup_usd_env.ps1
cargo test -p bif_core -- --test-threads=1
```

Plus docket-specific verification on C0:

```bash
# Confirm MILESTONES.md renders the new v0.16.5 row before v0.17.0
grep -n "v0.16.5" MILESTONES.md
# If site regen is wired into the commit workflow:
scripts/generate-site.sh   # or the PowerShell equivalent
```

UI-visible verification is listed per-commit under Acceptance Criteria.

## Acceptance Criteria

**C0 — Docket:**
- `MILESTONES.md` has a `v0.16.5 — Qt Polish (Graphite)` row in the Next Releases table, slotted between v0.16.0 and v0.17.0.
- Detail section references `assets/stitch_bif_ui/obsidian_graphite/DESIGN.md`.
- `SESSION_HANDOFF.md` mentions v0.16.5 under next-session context.
- `FEATURES.md` marks Graphite as docket-tracked rather than backlog.
- No code or test changes in this commit.

**C1 — Foundations:**
- Opening a multi-layer stage, performing a mock edit, pressing Ctrl+Z reverts it; Ctrl+Shift+Z re-applies; Edit menu shows enable/disable state bound to `can_undo`/`can_redo`.
- `pick_strongest_writable_sublayer` picks the correct layer when the root stack contains a locked sublayer (validated by loading a fixture with a non-writable layer mixed in).
- Node graph tab absent from default dock layout; reappears with the experimental flag / debug toggle.
- All checks pass.

**C2 — Quick wins:**
- Outline width spinbox in Render Settings changes the visible silhouette thickness in an 800px viewport across the 0.001–0.05 range.
- Color picker updates outline color live without viewport flicker.
- Dropping a `.usda` / `.usd` / `.usdc` / `.usdz` file onto the main window loads it via `on_stage_path_opened`.
- Hovering any property row in Property Inspector shows a rich-HTML tooltip enumerating the full layer contribution stack.
- "Ivar Render" button in Render Settings triggers `start_ivar_render()`; status bar reflects progress from the `IvarMessage` pump.

**C3 — Navigation:**
- `View → Look Through…` lists cameras from the loaded stage; selecting one switches the viewport through the existing `on_select_camera` path.
- Ortho toggle action switches to an orthographic aspect-correct projection.
- Workspace presets (Assembly / Lighting / Materials / Review) each rearrange docks and flip `PayloadPolicy` appropriately; QSettings persist the last-active preset across app restart.
- Payload-policy change prompts a confirm dialog when a stage is currently loaded.

**C4 — Editing (superseded):**
- See `docs/agent-handoffs/2026-04-24-v0.16-c4a-edit-foundation.md` for the C4a foundation acceptance criteria.
- C4b keeps the user-facing feature work: editable USDA panel, material sheet, and shading model dropdown.

## Out Of Scope

- Any visual/design-system work (Graphite) — belongs in v0.16.5.
- Lookdev orb — deferred to v0.17.
- Node graph evaluation pipeline revival — its own future milestone.
- Asset browser / asset library — deferred post-v0.15.0 by MILESTONES.md.
- Drag-and-drop beyond opening stage files (no prim reparenting, no material drag).
- Wacom pressure/tilt — deferred.
- Renderer God-object refactor (~75 fields) — leave alone.
- Material editor node graph (v0.21 / MaterialX milestone).
- Multi-layer save on Ctrl+S — edit-target only this pass.
- Real-time USDA parse on keystroke — validate on Apply only.
- Per-prim USDA snippet mode — out of scope for editable panel.

## Unresolved Questions

- **Save semantics on Ctrl+S** — edit-target only vs save-all-dirty-layers. Recommendation: edit-target only for v0.16; revisit multi-layer save in v0.17+.
- **Outline color space** — QColor returns sRGB; rest of renderer uses linear. Recommendation: convert at the UI boundary and store linear.
- **Payload-policy workspace switch** — silent reload vs confirm dialog. Recommendation: confirm dialog when a stage is loaded; silent when empty.
- **Material sheet tab auto-activation** — preserve user's active tab vs auto-switch to sheet when bound material exists. Recommendation: preserve user's tab; do not auto-switch.
- **Preset switching selection/time** — preserve across swaps vs reset. Recommendation: preserve.
- **Shading model switch round-trip** — author both shaders or only the selected one. Recommendation: only selected; warn on export if MaterialX-only params would be lost when round-tripping UsdPreviewSurface.
- **Node graph gate mechanism** — compile-time `#[cfg(feature = "node_graph_preview")]` vs runtime debug-menu toggle. Recommendation: runtime toggle so binary is unchanged; zero build-matrix impact.
- **EditOperation granularity for compound edits** (shading-model switch, multi-prim transform) — one batched op vs many atomic ops in one undo group. Recommendation: `EditHistory::begin_group()` / `end_group()` so undo sees a single step but ops stay atomic internally.
