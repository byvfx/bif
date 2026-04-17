---
title: Phase E.2 finish — moves 5/7/8/9 + Close Stage + HiDPI
type: journal
tags: [qt, cxx-qt, phase-e2, v0.15, journal]
created: 2026-04-16
updated: 2026-04-16
---

# Phase E.2 finish — moves 5/7/8/9 + Close Stage + HiDPI

## Context

One continuous ~4h planning+implementation session. Goal: close every remaining Phase E.2 move and polish, so `bif_qt_shell` reads a live USD stage end-to-end. All 4 remaining moves + Close Stage + HiDPI landed on the `v0.15-qt` working tree. Build + clippy clean; dogfood pending.

## Moves shipped

- **HiDPI** — `devicePixelRatioF()` threaded through `viewport_on_surface_ready` / `viewport_on_resize` bridge → `Viewport::new` / `Viewport::resize` → `Renderer`. No signal-signature change; DPR captured inside existing lambdas. See [[../concepts/hidpi-dpr-threading|HiDPI DPR threading]].
- **Close Stage (Ctrl+W)** — new `close_stage` invokable drains GPU, replaces `Renderer::scene` with `SceneManager::new()`, rebuilds pick BVH, clears layer state + path + selection, bumps revisions, swaps central stack to first-launch. Paint tick paused around reset (matches Open flow).
- **Move 9 — Keyframes** — `demo_keyframes` field deleted. `keyframe_count/at` + `jump_to_prev/next_keyframe` derive from the selected prim's `AnimatedTransform` via `selected_prim_keyframes` helper. `TimelineRuler` connects `selected_prim_pathChanged` to repaint.
- **Move 5 — Gizmo pick** — new `on_prim_pick(x, y)` invokable. Ray-cast via `Renderer::pick_instance_at`, resolves prim_path through `denormalize_synthetic_path` (inlined), sets `selected_prim_path` + `selected_prim_type`. `RenderWidget::mousePressEvent` multiplies pick coords by DPR before emit.
- **Move 7 — Scene Browser** — new `scene_browser_revision` qproperty + 6 invokables (`root_prim_count/path_at`, `child_prim_count/path_at`, `prim_type_name_at`, `prim_display_name_at`) backed by [[../concepts/primdataprovider-trait|PrimDataProvider]] on `UsdStage`. `SceneBrowserModel` takes `BifShellState*`, recursively rebuilds on revision bump (depth cap 64). Falls back to demo tree when no stage loaded.
- **Move 8 — Property Inspector** — 9 new invokables backing real attrs (`selected_prim_attribute_count/name/type/value_at`) and composition arcs (`selected_prim_stack_count/layer/specifier/has_opinion/color_index_at`). ~55 lines of `FakeAttr` tables deleted from `PropertyInspectorWidget`.

## Gotchas learned

- **Inherent methods mask trait methods with shared names.** `UsdStage` has inherent `get_prim_info(index: usize)` AND `impl PrimDataProvider` with `get_prim_info(path: &str)`. `stage.get_prim_info(&str)` dispatches to the inherent → E0308. Fix: use `get_prim_info_by_path(&str)` — avoids the shadow entirely.
- **cxx-qt `self.as_mut().rust_mut()` requires `let mut r = ...`** at the binding site. E0596 first time through.
- **DPR multiply belongs in C++**, inside `RenderWidget::mousePressEvent`, not in the Rust invokable. Keeps all downstream Rust signatures DPR-agnostic and matches the existing `resized(pixelWidth(), pixelHeight())` physical-pixel pattern.
- **`denormalize_synthetic_path` is crate-private in `bif_viewport`**. Inlined 10 lines locally rather than widening the public surface for one caller.
- **Two-call `count` + `at(i)` marshalling** works cleanly for `Vec<T>` across the cxx-qt boundary without inventing a new QAbstractItemModel per list.

## Known gaps for v0.16

- Scene Browser tree walk is eager (recursive `child_prim_count` + `child_prim_path_at`). Fine for `root.usda`; needs `canFetchMore`/`fetchMore` for 100K+ prim scenes.
- Attribute values stringify FFI-side via `Debug`-summary — pretty-printing deferred.
- Opinion dot uses strongest-layer color for every attribute row; real per-attribute opinion resolution deferred.

## Next up

Dogfood `test_assets/layers/root.usda`. Verify layers + browser + inspector + timeline keyframes + pick + Close Stage + HiDPI on 125/150%. Then commit (plan says 4 grouped, but main_window.rs interleaving makes clean `git add -p` messy — single "Phase E.2 finish" commit is pragmatic). Then Phase F: rip egui out of `bif_viewer`/`bif_viewport`.

## See also

- [[2026-04-15-phase-e2-first-pass|2026-04-15 — Phase E.2 first pass]]
- [[../architecture/phase-e2-qt-migration|Phase E.2 Qt migration roadmap]]
- [[../architecture/cxx-qt-bridge-patterns|cxx-qt bridge patterns]]
- [[../architecture/adr/007-shell-state-to-viewport-bridge|ADR-007]]
- [[../../devlog/2026-04/DEVLOG_2026-04-16|devlog 2026-04-16]]
