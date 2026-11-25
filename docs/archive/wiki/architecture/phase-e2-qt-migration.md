---
title: Phase E.2 — Qt migration roadmap
type: article
tags: [architecture, qt, cxx-qt, phase-e2, v0.15]
created: 2026-04-16
updated: 2026-04-16
---

# Phase E.2 — Qt migration roadmap

## Context

Phase E.2 of v0.15.0 is the integration pass that gets `bif_qt_shell` reading a live USD stage end-to-end — every Qt widget backed by real Rust/USD data instead of demo stubs. Runs on branch `v0.15-qt`. See [[adr/006-qt-via-cxx-qt|ADR-006]] for the binding choice and [[adr/007-shell-state-to-viewport-bridge|ADR-007]] for the runtime bridge enabling these moves.

## The 9 moves

| # | Move | Status | Note |
|---|------|--------|------|
| 1 | `bif_qt::Viewport` hosts real `bif_viewport::Renderer` | ✅ shipped | Required prior winit decoupling of Renderer |
| 2 | `File → Open` drives real `Renderer::load_usd_scene` + populates `scene_layer_state` | ✅ shipped | Enabled by ADR-007 β bridge |
| 4 | `detect_timeline_from_stage` reads live `UsdTimelineData` | ✅ shipped | Reused existing `cpp_bridge.rs:756` |
| 6 | Breadcrumb follows `selected_prim_pathChanged` | ✅ shipped | 1 `QObject::connect` in `window_builder.cpp` |
| 5 | Gizmo pick (LMB → `pick_instance_at`) | ✅ shipped | DPR multiply in `RenderWidget::mousePressEvent` |
| 7 | Scene Browser backed by real `UsdStage` via `PrimDataProvider` | ✅ shipped | Eager walk, depth cap 64 |
| 8 | Property Inspector backed by real attrs + composition arcs | ✅ shipped | FFI stringified `Debug`-summary |
| 9 | Timeline keyframes from selected prim's `AnimatedTransform` | ✅ shipped | `TimelineRuler` repaints on selection |
| — | Close Stage (Ctrl+W) polish | ✅ shipped | Drains GPU + resets scene + central stack |
| — | HiDPI DPR threading polish | ✅ shipped | See [[../concepts/hidpi-dpr-threading|concept]] |

(Move 3 was folded into others during scoping.)

## Prerequisite refactor (2026-04-15)

Before move 1 could land, `bif_viewport::Renderer` had to stop depending on `Arc<winit::Window>`. New API:

- `Renderer::new(surface, device, queue, config, size, scale_factor)` — synchronous, caller owns wgpu primitive construction
- `attach_egui(state: egui_winit::State)` — optional overlay; `bif_viewer` calls it, `bif_qt` does not
- `render(clear_color, raw_input: Option<RawInput>) -> Option<PlatformOutput>` — caller drives egui/winit handshake
- `set_dialog_focus_hook(Fn(bool))` — windowing-system-agnostic z-order hook

`bif_viewer` gained a `create_renderer(window)` compat shim.

## Phase F (next)

Delete `egui`/`egui-wgpu`/`egui-winit`/`egui-snarl` from `bif_viewer` + `bif_viewport`. Drop `run_egui_frame` + ~900 lines of panel assembly. Point `bif_viewer` main at `bif_qt::run`.

## Phase G / H

Validation checklist + release plumbing. Tag `v0.15.0` and merge `v0.15-qt` → `main`.

## See also

- [[adr/006-qt-via-cxx-qt|ADR-006]]
- [[adr/007-shell-state-to-viewport-bridge|ADR-007]]
- [[cxx-qt-bridge-patterns|cxx-qt bridge patterns]]
- [[adr/002-egui-temporary-ui|ADR-002: egui temporary UI]]
- [[../journal/2026-04-15-phase-e2-first-pass|Journal 2026-04-15]]
- [[../journal/2026-04-16-phase-e2-finish|Journal 2026-04-16]]
