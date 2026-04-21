---
title: Paint pause pattern
type: concept
tags: [qt, wgpu, gpu, concurrency]
created: 2026-04-16
updated: 2026-04-16
---

# Paint pause pattern

## Summary

`RenderWidget` drives a 16ms `QTimer` that submits one wgpu frame per tick. When the main thread is about to block on a modal operation — native `QFileDialog`, `Close Stage` reset, future async scene loads — continuing to submit frames causes two distinct bugs. Pause the tick around the modal work; resume after.

## Why

1. **Z-order fight.** On Windows, a 60 FPS paint keeps the main `QWidget::window()` in the foreground. The native file picker renders underneath and appears to not open at all. User opens File → Open → nothing happens.
2. **GPU-in-flight on reset.** Dropping the current `Renderer` (as Close Stage does) while commands are still queued triggers D3D12 `OBJECT_DELETED_WHILE_STILL_IN_USE`. A `wait_for_gpu()` (`device.poll(Maintain::Wait)`) is mandatory before the drop, and suppressing new frames during the drop is cleaner than racing the timer.

## The shape

```cpp
// render_widget.h
void pausePainting();
void resumePainting();
// m_tick is a member QTimer, not a function-local static
```

```cpp
// file dialog / close stage call site
render_widget->pausePainting();
auto path = QFileDialog::getOpenFileName(...);
render_widget->resumePainting();
```

## Rejected alternative

Early fix was `window->setVisible(false); dialog(); window->setVisible(true)`. It worked but made the whole app vanish for a beat — awful UX. Worse, `showEvent` re-fired after the reshow and re-triggered `viewport_on_surface_ready`, double-initializing the Renderer. The paint-pause fix is surgical; nothing visibly changes except the inner viewport stops updating.

## Where it's used

- `trigger_open_stage` in `crates/bif_qt/cpp/window_builder.cpp`
- `close_stage` flow in `crates/bif_qt/cpp/window_builder.cpp`

## See also

- [[../architecture/adr/007-shell-state-to-viewport-bridge|ADR-007]]
- [[../architecture/cxx-qt-bridge-patterns|cxx-qt bridge patterns]]
- [[../journal/2026-04-15-phase-e2-first-pass|Journal 2026-04-15]] — where this was first hit
