# Session Handoff

> Active sessions: last 5. Older entries → [SESSION_HANDOFF_ARCHIVE.md](docs/archive/SESSION_HANDOFF_ARCHIVE.md)

## Current State

- **Branch:** `refactor/deepen-modules` (off `v0.16.8-dogfood-polish` tip; not yet pushed)
- **Version:** v0.16.8 base (commit `0d9f4c7`)
- **Status:** RFC #5 (deepen path-trace core) **complete** — `PathTracer` deep module + NEE/MIS boundary tests, 3 commits, 119 tests green. RFC #6 **paused** — implementation surfaced that `SceneCmd` over-fits the imperative dispatch arms (see [issue #6 comment](https://github.com/byvfx/bif/issues/6)); needs re-scope to `NodeOutputs` + node-routing consolidation before any code.
- **Next:** **Branch consolidation first** — `main` is stale (~v0.16.6 tag) while v0.16.7/v0.16.8 + #5 sit unmerged on stacked branches. Execution checklist: [`docs/agent-handoffs/2026-05-29-branch-consolidation.md`](docs/agent-handoffs/2026-05-29-branch-consolidation.md). Then re-scoped #6 (NodeOutputs) or v0.17.0.

---

# Session Handoff — 2026-05-28 (RFC #5 path-trace core deepening)

**Last Updated:** 2026-05-28 on `refactor/deepen-modules` (commit `546e6d4`).

**Context:** Whole-codebase architecture review (Ousterhout deep-module lens) → 6 deepening candidates → filed 2 RFCs ([#5](https://github.com/byvfx/bif/issues/5) path-trace core, [#6](https://github.com/byvfx/bif/issues/6) node-type). Both chose the pragmatic "common-caller" Design C.

**Done — RFC #5 (TDD, 2 commits):**
1. `3196226` — pure helpers `should_skip_cache` / `roulette_survival` extracted from `ray_color_with_aovs`; RR start named `DEFAULT_RR_START_BOUNCE`.
2. `546e6d4` — `PathTracer<'a>` deep module: owns scene+config, resolves HDRI/cache once in `new()`, single `trace()` entry, `sample_pixel`/`sample_pixel_color` hold SPP+filter loop. `ray_color_with_aovs`/`render_pixel`/`render_pixel_with_aovs` → thin wrappers (API unchanged). `bucket.rs` builds one tracer per bucket (per-frame construction). `rr_start_bounce` exposed via `with_rr_start_bounce`.

3. `cbdeda7` — NEE/MIS boundary tests via `PathTracer`: NEE illuminates a diffuse surface, shadow rays respect occlusion, area light exercises the MIS-weighted branch. (Caught two real semantics while writing: `DistantLight` angle>0 → cone pdf; `RectLight` one-sided emission.)

**Validation:** `bif_renderer` 119 + `bif_viewport` 177 tests pass; clippy clean; workspace builds; fmt clean.

**RFC #6 — paused.** Reading `node_dispatch.rs` showed the ~26 dispatch arms are imperative orchestrations (e.g. `CreatePrimitive` calls `load_primitive` which mutates + returns the id the "command" would need), so a global `SceneCmd` surface over-fits — it'd need `Custom(Box<dyn FnOnce>)`, defeating testability. Re-scope recorded on [issue #6](https://github.com/byvfx/bif/issues/6): do `NodeOutputs` (merge `node_proto_map` + `node_cloud_map`) + `CookNode`/node-routing consolidation; drop global `SceneCmd`. No #6 code written.

**Next:** Re-scoped #6 (NodeOutputs + routing). Push `refactor/deepen-modules` + open PR.

---

# Session Handoff — 2026-05-23/28 (v0.16.8 production crash chain)

**Last Updated:** 2026-05-28 on `v0.16.8-dogfood-polish` (commit `0d9f4c7`).

**Problem:** `STATUS_STACK_BUFFER_OVERRUN` crash loading `rt_010_base.usda` (production shot — heavy PointInstancer + large texture set). Five crash modes patched in cascade:

1. **OOM in instancer animation** — 24 GiB `Vec` allocation attempt. Added `AllocTooLarge` error variant + checked allocation guard with 4 GiB cap.
2. **OOM in combined mesh** — 16 GiB `Vec` allocation attempt. Pre-allocation guard in `mesh_data.rs`, cap at 1 GiB, fallback to multi-draw path.
3. **Device-lost panics on `Queue::submit`** — error scopes don't catch fatal panics across FFI boundary. Wrapped with `catch_unwind`.
4. **GPU health tracking** — process-global `GPU_UNHEALTHY` atomic checked at all render entry points to skip work after device-lost.
5. **Texture VRAM budget** — 1.5 GiB default cap with per-upload charge prediction; uploads beyond budget are skipped with a log warning.

**Result:** App no longer crashes. Loads `rt_010_base.usda` partially — VRAM budget exhausted at ~192/1650 textures (expected behavior; remaining textures logged as skipped).

**Validation:** Crash chain confirmed resolved on production shot. `cargo build` + clippy + fmt clean.

**Next:** Dynamic VRAM budget detection (query actual GPU VRAM vs. hardcoded 1.5 GiB default) deferred to v0.17. See `MILESTONES.md`.

---

# Session Handoff — 2026-05-16 (v0.16.7 keybinding editor)

**Last Updated:** 2026-05-16 on `v0.16.7-keybinding-editor` (commit `07b15a1`).

**Current work:** File ▸ Preferences dialog with keybinding UI.

- New `PreferencesDialog` modal: `QTreeWidget` with Action / Shortcut columns, populated from `ShortcutRegistry::registered_defaults()`. Double-click → `QKeySequenceEdit` inline editor. Reset All button.
- Changes persist to `QSettings("shortcuts/<id>")` — same path `ShortcutRegistry::lookup()` already reads on startup.
- Wired to File ▸ Preferences (no default shortcut to avoid conflicts).

**Limitations (deferred):**
- Widget-owned shortcuts (e.g. `QAbstractItemView` built-ins) bypass `ShortcutRegistry` and won't appear in the dialog.
- Hot reload deferred: changes take effect on next app launch.

**Validation:** `cargo build -p bif_qt` clean. `cargo test -p bif_qt --lib` pass. Clippy + fmt clean. Manual smoke: dialog opens, shortcut edit commits, persists across restart.

**Next:** Dialog validation (reject conflicting shortcuts). Convert File/Workspace menu items through shortcut registry. Hot reload via `QShortcut::setKey()`.

---

# Session Handoff — 2026-05-15/16 (v0.16.6 NaN AABB fix + close)

**Last Updated:** 2026-05-16. Branch `v0.16.6-ui-polish` merged to `main`, tagged `v0.16.6`.

**Fix (commit `982e0b3`):** NaN AABB panic on `comprehensive.usda` load. Two bugs:
1. `Renderer::compute_scene_bounds()` returned `Aabb::INFINITE` sentinel for empty/invalid scenes without NaN checking — downstream camera framing called `.center()` on NaN bounds → panic.
2. Degenerate USD xform ops in `scene_loader.rs` propagated NaN through matrix transforms; added `is_finite()` guard before applying.

**Fix:** `Aabb::is_valid()` check before framing; sentinel `Aabb::ZERO` for empty scenes. Matrix guard skips degenerate xforms.

**Validation:** `comprehensive.usda` loads without panic. `cargo test -p bif_viewport` 149/149. Code review via `vfx-code-reviewer` passed. Tagged `v0.16.6 - 2026-05-16`.

---

# Session Handoff — 2026-05-13 (v0.16.6 UI polish + v0.16.5 hotfix)

**Last Updated:** 2026-05-13 on `v0.16.6-ui-polish`.

**Current work:** Three commits closing out v0.16.5 and starting v0.16.6.

1. **v0.16.5 hotfix (`a38eaba`)** — post-`vfx-code-reviewer` follow-up on `611a136`. Added outer `catch(...)` to all 7 new CollectionAPI bridge fns (Tf/boost throws no longer unwind across `extern "C"` → UB). 4 collection mutators (`_apply` / `_add_target` / `_remove_target` / `_set_expansion_rule`) now gate on `stage->GetEditTarget().GetLayer()->PermissionToEdit()`, mirroring `usd_bridge_layer_write_*`. Also re-derived `viewport.rs` `CLEAR_COLOR` from `theme::SURFACE` (#131313 → 0.00518 linear) after the Graphite palette swap left it pointing at a stale value. Fast-forwarded to `main`.

2. **v0.16.6 — Collection Editor reskin (`dbbf5bc`)** — replaced QGroupBox + QInputDialog with three collapsible `SectionBlock` widgets (Includes / Excludes / Resolved Members). Inline `QLineEdit` edit row replaces the modal: Return commits, Esc cancels, focus-out commits non-empty / cancels empty. 4px left accent stripe per role via `QSS ::item { border-left: ... }`. Delete/Backspace removes selection. Empty-state italic hints. All styling driven from `theme.rs` (new `#sectionBlock` / `#sectionHeaderRow` / `#sectionIcon` / `#sectionEmptyHint` / `#includesList` / `#excludesList` / `#membersList` selectors + test).

3. **v0.16.6 — View ▸ Panels submenu + Reset Workspace Layout (`22a4a5f`)** — new submenu with 8 checkable QActions, one per dock. Defaults `Ctrl+Shift+1..8` routed through `bif_qt::shortcuts::lookup()` registry (added `panels.*` keys to `shortcut_registry.h`) so a future Preferences dialog can remap them. Bidirectional `QAction::toggled` ↔ `QDockWidget::visibilityChanged` sync via `QSignalBlocker` — closing a dock via X unchecks the menu entry, workspace switches propagate to checked state. Initial seeding deferred via `QTimer::singleShot(0, ...)` because docks are created later than the menu-action wiring. New `View ▸ Reset Workspace Layout` action: `QMessageBox::question` confirm → `QSettings.remove(state_key)` → `ws::apply_default_layout()`. All 8 panel toggles + reset added to command palette.

**Validation:** `cargo build -p bif_qt` clean on each commit. `cargo test -p bif_qt --lib` 15/15. `cargo test -p bif_core --lib collection -- --test-threads=1` 7/7. Clippy + fmt clean. User-confirmed manual dogfood: inline-add UX, panels submenu sync, Reset confirm dialog all behave correctly.

**Next:** Merge `v0.16.6-ui-polish` → `main` (likely fast-forward, since branched off `a38eaba` and no concurrent main commits). Run `vfx-code-reviewer` on the v0.16.6 work. Then candidates: keybinding-editor UI (registry exists, no front-end), material-icon TTF bundle, custom-workspaces save/load. v0.17.0 (Context System) still pending — highest-risk roadmap item.

**Files touched this session:**
- `cpp/usd_bridge/usd_bridge.cpp` (hotfix)
- `crates/bif_qt/src/viewport.rs` (hotfix)
- `crates/bif_qt/src/theme.rs` (Collection Editor selectors)
- `crates/bif_qt/cpp/collection_editor_widget.{h,cpp}` (full rewrite)
- `crates/bif_qt/cpp/shortcut_registry.h` (panels.* keys)
- `crates/bif_qt/cpp/window_builder.cpp` (Panels submenu, dock sync, Reset action, palette entries)
- `CHANGELOG.md`, `devlog/2026-05/DEVLOG_2026-05-13.md`

---

> Older sessions archived in [SESSION_HANDOFF_ARCHIVE.md](docs/archive/SESSION_HANDOFF_ARCHIVE.md)
