---
title: HiDPI DPR threading
type: concept
tags: [qt, wgpu, hidpi, v0.15]
created: 2026-04-16
updated: 2026-04-16
---

# HiDPI DPR threading

## Summary

`bif_qt_shell` must render + pick in **physical pixels** on 125% / 150% HiDPI displays. `QScreen::devicePixelRatioF()` flows from the Qt layer down through the cxx-qt bridge into `bif_viewport::Renderer`, where `scale_factor: f32` is stored and used for resize and egui `pixels_per_point`.

## The path

```
RenderWidget::devicePixelRatioF()            // Qt owns the authoritative DPR
  → window_builder.cpp lambdas               // capture inside existing signal handlers
  → viewport_on_surface_ready(hwnd, w, h, dpr)
  → viewport_on_resize(w, h, dpr)
  → Viewport::new(hwnd, hinstance, w, h, dpr)
  → Viewport::resize(w, h, dpr)
  → Renderer::new(..., scale_factor)
  → Renderer::resize(size, scale_factor)
```

No signal signature change — DPR is captured inside the existing lambda bodies via `this->devicePixelRatioF()` before forwarding to the cxx-qt invokable.

## Pixel coord convention

`RenderWidget` emits **physical pixels** consistently:

- `resized(pixelWidth(), pixelHeight())` — already established pattern.
- `primPickRequested(x * dpr, y * dpr)` — `mousePressEvent` multiplies before emit.

Rust invokables downstream stay DPR-agnostic. One conversion point at the event source.

## Why not do it in Rust

Putting the DPR multiply in the invokable would force every new pick/coord invokable to re-derive it. Keeping it in C++ at the event source mirrors `resized` and gives one place to change if the Qt-side policy ever shifts (e.g. per-screen DPR on multi-monitor).

## Before Phase E.2

`scale_factor` was hardcoded to `1.0` during move 1. Geometry still rendered but under-scaled on HiDPI, and pick rays missed by the DPR factor.

## See also

- [[../architecture/phase-e2-qt-migration|Phase E.2 roadmap]]
- [[../architecture/cxx-qt-bridge-patterns|cxx-qt bridge patterns]]
