# Dogfood Viewport Edit Fixes — Execution Handoff

**Date:** 2026-05-04  
**Branch:** `v0.16.1-followups`

---

## Goal

Fix the two root causes that make virtually every user edit invisible in the viewport and every Ctrl+Z a no-op. After this lands: material param changes, visibility toggles, USDA Apply, and shading-model swaps all reflect immediately in the wgpu viewport. Ctrl+Z/Ctrl+Y revert/reapply them. Replace the Property Inspector visibility checkbox with the eye glyph already painted in the scene browser tree.

---

## Scope

### In scope

1. **RC-1 — `apply_usd_edit` missing reload** (`lib.rs`) — after writing a USD opinion, call `reload_working_scene()` so the in-memory renderer scene reflects the change.
2. **RC-2 — `undo()` / `redo()` USD arm missing reload** (`lib.rs`) — after `undo_usd_edit` / `redo_usd_edit` succeeds, call `reload_working_scene()` so the viewport reflects the reverted/reapplied state.
3. **Eye glyph click wiring** (`scene_browser_widget.cpp`) — add `editorEvent()` to `PrimRowDelegate`; hit-test the painted `eye_rect` and call `on_set_visibility` on click.
4. **Remove visibility checkbox** (`property_inspector_widget.cpp`) — delete the `Visible` checkbox and its `toggled` connection; the scene browser eye becomes the sole toggle.
5. **USDA Apply retest** — verify items 8 and 12 pass after RC-1 lands; if not, audit `dispatch_replace_layer_contents` working-layer resolution.

### Out of scope

See *Out Of Scope* section below.

---

## Files Or Modules

| File | Change |
|------|--------|
| `crates/bif_viewport/src/lib.rs` | Add `reload_working_scene()` to `apply_usd_edit` and to USD arms of `undo()` / `redo()` |
| `crates/bif_qt/cpp/scene_browser_widget.cpp` | Add `editorEvent()` to `PrimRowDelegate`; pass `m_state` to delegate constructor |
| `crates/bif_qt/cpp/property_inspector_widget.cpp` | Remove `Visible` checkbox (~line 192–202) and its `toggled` signal connection |
| `crates/bif_viewport/src/selection_dispatch.rs` | Inspect only — audit `dispatch_replace_layer_contents` if USDA Apply still fails after RC-1 |

---

## Constraints

- **Read `reload_working_scene()` before touching `apply_usd_edit`.** Confirm it reloads material bindings and visibility state from USD — not just geometry. If it only rebuilds geometry, material param changes (items 14–15) need a separate material-cache invalidation step in addition to the reload.
- **Confirm `PrimPathRole` name** on `SceneBrowserModel` before implementing `editorEvent`. The role may differ from `PrimPathRole`; check `scene_browser_model.h`.
- **`editorEvent` must return `false` for non-eye clicks** — tree expand/collapse, selection, and drag must continue to work normally.
- `PrimRowDelegate` is defined inline in `scene_browser_widget.cpp` (not a separate header). Its constructor currently takes no args — update to accept `BifShellState* state` and store it as a member.
- No new public API surface, no new crates.
- All existing tests must pass. Add targeted regression tests for undo/redo if feasible.

---

## Implementation Detail

### Phase 1 — `apply_usd_edit` reload (`lib.rs` ~line 1652)

After `self.project.mark_dirty()` and before `Ok(desc)`:

```rust
if let Err(e) = self.reload_working_scene() {
    log::warn!("scene reload after USD edit failed: {e}");
}
Ok(desc)
```

Fixes dogfood items: **1, 8, 14, 15, 19, 21**

### Phase 2 — `undo()` USD arm reload (`lib.rs` ~line 1710)

```rust
UndoActionKind::Usd => {
    let stage_arc = self.scene.usd_stage.clone()?;
    let mut layer_state = self.scene.layer_state.take()?;
    let result = {
        let stage = stage_arc.lock().ok()?;
        layer_state.undo_usd_edit(&stage)
    };
    self.scene.layer_state = Some(layer_state);
    if let Err(e) = self.reload_working_scene() {   // ADD
        log::warn!("scene reload after USD undo failed: {e}");
    }
    result.ok().flatten()?
}
```

Mirror the same reload in the `redo()` USD arm.

Fixes dogfood items: **2, 3, 10, 16, 18, 25**

### Phase 3 — Eye glyph click (`scene_browser_widget.cpp`)

**Update delegate construction (~line 172):**
```cpp
m_view->setItemDelegate(new PrimRowDelegate(m_state, this));
```

**Update `PrimRowDelegate` constructor** to accept and store `BifShellState* state`.

**Add `editorEvent()` override:**
```cpp
bool editorEvent(QEvent* event, QAbstractItemModel* /*model*/,
                 const QStyleOptionViewItem& option,
                 const QModelIndex& index) override
{
    if (event->type() != QEvent::MouseButtonRelease) return false;
    auto* me = static_cast<QMouseEvent*>(event);
    const int cy = option.rect.center().y();
    const int ex = option.rect.left() + 2;
    const QRectF eye_rect(ex, cy - 4, kEyeWidth, 8);
    if (!eye_rect.contains(me->pos())) return false;
    const QString path = index.data(SceneBrowserModel::PrimPathRole).toString();
    const bool is_visible = index.data(SceneBrowserModel::IsVisibleRole).toBool();
    if (m_state && !path.isEmpty())
        m_state->on_set_visibility(path, !is_visible);
    return true;
}
```

Constants `kEyeWidth = 12`, `eye_rect` formula already defined in `paint()` — use the same values.

**Phase 4 — Remove Property Inspector checkbox (`property_inspector_widget.cpp` ~line 192–202)**

Delete the `Visible` QCheckBox widget, its label, and the `toggled` → `on_set_visibility` signal connection. The scene browser eye is now the sole toggle.

*Note:* `layer_state_revisionChanged` → `on_layer_state_changed()` → `populate_material_sheet()` is **already connected** at line 331. Do not re-add it.

### Phase 5 — USDA Apply retest (conditional)

After Phases 1–2 build and pass smoke test, retest dogfood items 8 and 12:
- If fixed: done.
- If still broken: inspect `dispatch_replace_layer_contents` in `selection_dispatch.rs` — verify `layer_id` from `on_apply_usda` matches a real file-backed layer identifier (not a display name), and that `parse_usda` errors surface correctly.

---

## Checks To Run

```powershell
# USD env required for bif_core tests
. .\setup_usd_env.ps1

cargo build 2>&1 | Select-String "error"        # must be clean
cargo clippy -- -D warnings                      # must pass
cargo test -p bif_viewport                       # 149 tests
cargo test -p bif_core -- --test-threads=1       # needs USD DLLs
cargo fmt --check
```

Then manual smoke test:
```powershell
. .\setup_qt_env.ps1; . .\setup_usd_env.ps1
cargo run -p bif_viewer
```

---

## Acceptance Criteria

- [ ] Open `test_assets/layers/root.usda`, select a mesh, adjust roughness spinbox → viewport updates on commit (no reselect needed)
- [ ] Change base_color via color picker → viewport reflects new color
- [ ] Ctrl+Z after roughness change → viewport reverts to previous value
- [ ] Ctrl+Y → reapplies
- [ ] Click eye glyph in scene browser → prim hides/shows in viewport immediately
- [ ] Ctrl+Z → visibility reverts in viewport
- [ ] Property Inspector no longer shows a `Visible` checkbox
- [ ] Clicking eye glyph does not break tree expand/collapse or row selection
- [ ] USDA panel: type `over "/World/Cube" { token visibility = "invisible" }` → Apply → viewport hides prim (no error)
- [ ] Ctrl+Z → prim reappears
- [ ] Shading model swap (UsdPreviewSurface → OpenPBR) → Ctrl+Z → reverts in one shot
- [ ] `cargo test -p bif_viewport` passes (149 tests)
- [ ] `cargo clippy -- -D warnings` clean
- [ ] `cargo build` clean (no warnings)

---

## Out Of Scope

| Item | Reason |
|------|--------|
| Item 11 — USDA save: authored-only opinions | Needs design session — separate handoff |
| Item 19 — material bind UI in node graph | Architectural decision, separate handoff |
| Layer stack eye icons | Follow-up; check `layer_stack_widget.cpp` first |
| Items 26–30, 31–37 — dogfood regression suite | Run after this lands |
| OCIO ACES full implementation | Separate track |
| AOVs | Feature work, not bug |
| Viewport Vulkan/render switch | Separate track |
| C++ bridge test crashes (`lucy_100` etc.) | Pre-existing, unrelated |

---

## Unresolved Questions

1. Does `reload_working_scene()` reload material binding and shader param values from the USD stage, or only geometry/instance data? If geometry-only, Phase 1 won't fix material param viewport updates (items 14–15) and a targeted material-cache invalidation call will be needed alongside it.
2. What is the exact role name for prim path on `SceneBrowserModel`? Check `scene_browser_model.h` — it may be `PrimPathRole`, `PathRole`, or a custom string role. Use the correct name in `editorEvent`.
