---
title: cxx-qt bridge patterns
type: article
tags: [architecture, qt, cxx-qt, ffi, v0.15]
created: 2026-04-16
updated: 2026-04-16
---

# cxx-qt bridge patterns

## Context

Phase E.2 shipped ~20 invokables connecting `BifShellState` (Rust QObject) to the live renderer and USD stage. A handful of idioms recurred; documenting them here so future invokables don't re-learn them. The runtime state bridge lives in [[adr/007-shell-state-to-viewport-bridge|ADR-007]] — this note covers the smaller cxx-qt-specific patterns.

## Invokable signatures

- `QString` is passed **by value** in invokable params (not `&QString`). Matches existing `on_stage_path_opened(path: QString)`.
- Return types marshalling across the bridge: primitive (`i32`, `f32`, `bool`), `QString`, and `Option<_>` of these. For `Vec<T>`, expose a two-call pattern:
  - `fn selected_prim_attribute_count(&self) -> i32`
  - `fn selected_prim_attribute_name_at(&self, i: i32) -> QString`
- This avoids per-list `QAbstractItemModel` subclasses for simple panels.

## Mutable self

`self.as_mut().rust_mut()` returns a binding that itself needs `mut`:

```rust
let mut r = self.as_mut().rust_mut();
r.current_stage_path = Some(path.to_path_buf());
```

Plain `let r = ...` fails with E0596 the first time through. The `Pin` is invisible at the use site — read/write fields as if it were a `&mut T`.

## qproperty change signals

Every `#[qproperty]` field gets a `fooChanged()` signal. The binding must **set through the cxx-qt setter** (`self.as_mut().set_foo(value)`) for the signal to fire; direct field mutation silently skips the notify. Widgets lean on this — e.g. `TimelineRuler` connects `selected_prim_pathChanged` to repaint keyframes.

## Revisions for bulk refresh

For data that doesn't fit a single qproperty (Scene Browser tree, layer stack, attribute tables), expose a monotonic `*_revision: i32` qproperty. Widgets `connect` to its `Changed` signal and rebuild by polling the `count` + `at(i)` invokables. Bump via a `bump_scene_browser_revision` helper after state changes.

## State access from invokables

`BifShellState` invokables see `Pin<&mut Self>` and have no static path to the live `Renderer`. Use the β thread-local bridge from [[adr/007-shell-state-to-viewport-bridge|ADR-007]]:

```rust
with_viewport_mut(|vp| vp.renderer_mut().pick_instance_at(x, y))
```

Returns `Option<R>` — `None` when `viewport_on_surface_ready` hasn't fired yet. Treat that as a soft error (status-bar hint), not a panic.

A sibling `with_stage(|stage| ...)` helper clones `Arc<Mutex<UsdStage>>` before locking to avoid re-entrant guard issues inside nested closures.

## Trait-vs-inherent method shadowing

When implementing a trait on an FFI type that already has an inherent method of the same name, inherent wins. `UsdStage` has inherent `get_prim_info(index: usize)` and `impl PrimDataProvider` with `get_prim_info(path: &str)`. `stage.get_prim_info(&str)` dispatches to the inherent → E0308. Fix: use an explicitly-named inherent (`get_prim_info_by_path`) or disambiguate via UFCS `<UsdStage as PrimDataProvider>::get_prim_info(&stage, path)`.

## Pixel coords

`RenderWidget` emits physical pixels in `resized(pixelWidth(), pixelHeight())` and `primPickRequested(x * dpr, y * dpr)` — DPR conversion happens at the event source, Rust invokables stay DPR-agnostic. See [[../concepts/hidpi-dpr-threading|HiDPI DPR threading]].

## See also

- [[adr/006-qt-via-cxx-qt|ADR-006: Qt via cxx-qt]]
- [[adr/007-shell-state-to-viewport-bridge|ADR-007]]
- [[phase-e2-qt-migration|Phase E.2 roadmap]]
- [[../concepts/paint-pause-pattern|Paint pause pattern]]
- [[../concepts/primdataprovider-trait|PrimDataProvider trait]]
