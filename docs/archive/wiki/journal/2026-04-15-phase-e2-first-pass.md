---
title: Phase E.2 first pass — winit decoupling + moves 1/2/4/6
type: journal
tags: [qt, cxx-qt, phase-e2, v0.15, journal]
created: 2026-04-15
updated: 2026-04-15
---

# Phase E.2 first pass — winit decoupling + moves 1/2/4/6

## Context

First real day of Phase E.2 (v0.15.0 Qt migration — getting `bif_qt_shell` reading a live USD stage end-to-end). Session ran ~8h across 10 commits on `v0.15-qt`.

## The rescope

Handoff asked for moves 1/2/4 directly. Explore agent caught a blocker: `bif_viewport::Renderer::new()` took `Arc<winit::Window>` and egui was woven through via `egui_winit::State`. `bif_qt` only has an HWND. Moves would have blown past scope.

Chose Path C: decouple `Renderer` from winit first. New API: `new(surface, device, queue, config, size, scale_factor)` synchronous; caller builds wgpu primitives. `attach_egui(state)` is optional — `bif_viewer` calls it, `bif_qt` does not. `render(clear_color, raw_input: Option<RawInput>) -> Option<PlatformOutput>` passes the winit handshake out to the caller.

Installable dialog-focus hook (`set_dialog_focus_hook(Fn(bool))`) replaces the 5 internal `with_dialog_focus` call sites that used to reach into winit directly. Windowing-system-agnostic at the Renderer boundary.

## Moves shipped

- **Move 1** — `bif_qt::Viewport` holds a real `bif_viewport::Renderer`, not a triangle. HWND → instance/surface/adapter/device/queue → `Renderer::new`. Headless (`None` raw_input) path.
- **Move 4** — `detect_timeline_from_stage` invokable reads the renderer's live stage via `UsdTimelineData` (already existed in `cpp_bridge.rs:756`). No new FFI.
- **Move 2** — `on_stage_path_opened` drives real scene load via the new ADR-007 bridge. On success clones `scene.layer_state` onto `BifShellState`, bumps `layer_state_revision` — Layer Stack panel shows real layers.
- **Move 6** — breadcrumb wires to `selected_prim_pathChanged` in `window_builder.cpp`.

## ADR-007 — the bridge

`BifShellState` invokables see `Pin<&mut Self>`; no native path to `ViewportCallbacks::viewport.renderer.scene`. Two Rust singletons, disjoint scopes.

Flipped α → β. α (`Arc<Mutex<CommandQueue>>`) was thread-safety-flavored — doesn't apply. Qt UI is single-threaded, scene load is synchronous, no frame-coherence problem. β (`thread_local!<Cell<*mut ViewportCallbacks>>` + `with_viewport_mut(|vp| ...)` helper) reduces to 3 lines of `unsafe` with a documented invariant: single-threaded + stack-owned + install-before-event-loop. See [[../architecture/adr/007-shell-state-to-viewport-bridge|ADR-007]].

## Dogfood bugs → fixes

- **wgpu INFO spam** at 60 FPS → `env_logger` filter `info,wgpu_core=warn,wgpu_hal=error,naga=warn`.
- **QFileDialog behind main window** — 16ms paint tick stole z-order. Replaced the ugly `setVisible(false/true)` workaround with `RenderWidget::pausePainting/resumePainting` (tick now a member `m_tick`). [[../concepts/paint-pause-pattern|Paint pause pattern]].
- **D3D12 `OBJECT_DELETED_WHILE_STILL_IN_USE`** on close → `Renderer::wait_for_gpu()` in `viewport_on_shutdown`.
- **Double-init** of Renderer after `setVisible` reshow → `viewport_on_surface_ready` now guards on `cb.viewport.is_some()`.
- **Camera stubs** → `on_camera_orbit/pan/zoom` wired through `with_viewport_mut` to `Renderer.cam.camera`.

## Shared Open Stage flow

Launch-screen "Open USD" and recent-stage clicks weren't loading. Extracted `trigger_open_stage(window, shell_state, central_stack, viewport, pre_picked_path)` in `window_builder.cpp`. Menu → picker. Launch-screen → picker. Recent → skip picker. One code path, paint-pause built in. Swap `central_stack->setCurrentIndex(1)` **before** calling `on_stage_path_opened` — `with_viewport_mut` returns `None` until `surfaceReady` fires.

## Learnings

- **Handoff scope estimates hide coupling.** winit-through-egui had 8 touchpoints across 2 files, 5 of which were structurally through `egui_winit::State`. Explore agent caught this before I burned time on a dead end.
- **Per-frame egui/winit handshake is conceptually small** — `take_egui_input` pre, `handle_platform_output` post. `Option<RawInput>` in / `Option<PlatformOutput>` out lets `bif_viewport` stay egui-aware but winit-blind.
- **α's sales pitch was thread-safety-flavored.** Worth stopping to interrogate which constraint a design is actually solving.
- **`rebuild_pick_scene` + `wait_for_gpu` + `SceneManager::new()` is the full teardown**. No custom reset path needed.

## See also

- [[../architecture/adr/007-shell-state-to-viewport-bridge|ADR-007: BifShellState ↔ ViewportCallbacks bridge]]
- [[../architecture/adr/006-qt-via-cxx-qt|ADR-006: Qt via cxx-qt]]
- [[2026-04-16-phase-e2-finish|2026-04-16 — Phase E.2 finish]]
- [[../../devlog/2026-04/DEVLOG_2026-04-15|devlog 2026-04-15]]
