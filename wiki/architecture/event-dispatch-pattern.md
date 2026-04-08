---
title: "Event Dispatch Pattern"
type: article
tags: [architecture, events, patterns]
created: "2026-04-07"
updated: "2026-04-07"
---

## Summary

BIF uses an `EventBus` pattern where UI pushes `AppEvent` variants during the frame, and `dispatch_events()` drains them at a predictable point in the render loop. Event handlers are split across category files for maintainability.

## Architecture

### EventBus (app_event.rs)

```text
UI code → event_bus.emit(AppEvent::PrimSelected(path))
         ↓
render loop → dispatch_events() drains all pending events
         ↓
         match event → delegates to handler method in category file
```

### Dispatch Files

| File | Category | Handlers |
|------|----------|----------|
| `render_dispatch.rs` | Render | render mode, rebuild, denoise, filter, batch start/cancel |
| `selection_dispatch.rs` | Selection | prim select, transform edit, corrections, keyframe, export, frame, variants |
| `project_dispatch.rs` | Project | new, open, save, save-as, open-recent |
| `node_dispatch.rs` | Node graph | USD load, render start, export, scatter, instancer, etc. |

### Thin Router (render.rs)

`dispatch_events()` is ~50 lines — a match statement that delegates to handler methods. Camera events and node graph delegation stay inline (thin one-liners).

### Handler Pattern

Each file contains `impl Renderer` blocks with `pub(crate)` methods:

```rust
// selection_dispatch.rs
impl Renderer {
    pub(crate) fn handle_prim_selected(&mut self, path: String) { ... }
    pub(crate) fn handle_transform_edit(&mut self, edit: TransformEdit) { ... }
}
```

## Direct Mutation Convention

- **Direct mutation** (in egui closures): Simple boolean toggles with no side effects
- **EventBus**: Anything triggering side effects (scene reload, camera sync, undo/redo)

## See Also

- [[Architecture Review]] — dispatch split is Phase 5
- `crates/bif_viewport/src/app_event.rs` — AppEvent enum (20 variants)
