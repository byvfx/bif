# Session Handoff

> Active sessions: last 5. Older entries → [SESSION_HANDOFF_ARCHIVE.md](docs/archive/SESSION_HANDOFF_ARCHIVE.md)

## Current State

- **Branch:** `v0.16.8-dogfood-polish`
- **Version:** v0.16.8 (commit `0d9f4c7`)
- **Status:** Production-shot crash chain fixed (OOM + device-lost). App loads `rt_010_base.usda` partially — VRAM exhausted at ~192/1650 textures.
- **Next:** Dynamic VRAM budget detection deferred to v0.17. Candidates: dogfood polish on this branch, or begin v0.17.0 (`cpp_bridge.rs` split, payload policies).

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

> Older sessions archived in [SESSION_HANDOFF_ARCHIVE.md](docs/archive/SESSION_HANDOFF_ARCHIVE.md)
