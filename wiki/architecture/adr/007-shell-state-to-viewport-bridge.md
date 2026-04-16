---
title: "ADR-007 — BifShellState ↔ ViewportCallbacks bridge"
type: adr
tags: [architecture, qt, cxx-qt, renderer, ownership]
created: 2026-04-15
updated: 2026-04-15
status: accepted
---

# ADR-007 — `BifShellState` ↔ `ViewportCallbacks` bridge

## Context

`bif_qt::Viewport` hosts the real [[Renderer]] (as of Phase E.2 move 1, 2026-04-15). The `ViewportCallbacks` struct owns the live `Viewport` and its `Renderer` — allocated by `app.rs` on the stack, handed to C++ as a `*mut ViewportCallbacks` for the duration of the Qt event loop.

Separately, `BifShellState` (a cxx-qt QObject in `main_window.rs`) owns the Qt-facing state — menu-action invokables, timeline qproperties, layer-stack state surface, etc.

Phase E.2 move 2 ("wire `QFileDialog` result → real stage load") creates a hard dependency from `BifShellState` to `ViewportCallbacks`: the invokable needs to call `vp.renderer_mut().scene.load_usd_scene(&path)`. These two Rust singletons live in disjoint scopes:

- `ViewportCallbacks` is allocated in `app.rs::run`, passed to C++ as a raw pointer.
- `BifShellState` is constructed by C++ inside `bif_qt_run_shell`, accessible from Rust only through the cxx-qt bridge's `Pin<&mut BifShellState>` in invokables.

## Decision

**β — raw pointer held in a `thread_local!` cell**, installed from `app.rs` before the Qt event loop starts, accessed from `BifShellState` invokables through a `with_viewport_mut(|vp| ...)` helper that encapsulates the single `unsafe` block.

```rust
// main_window.rs
thread_local! {
    static VIEWPORT_CALLBACKS: Cell<*mut ViewportCallbacks> =
        const { Cell::new(std::ptr::null_mut()) };
}

pub fn install_viewport_callbacks(cb: *mut ViewportCallbacks) {
    VIEWPORT_CALLBACKS.with(|slot| slot.set(cb));
}

fn with_viewport_mut<R>(f: impl FnOnce(&mut Viewport) -> R) -> Option<R> {
    VIEWPORT_CALLBACKS.with(|slot| {
        let ptr = slot.get();
        if ptr.is_null() { return None; }
        // SAFETY: UI-thread only; ViewportCallbacks lives on app.rs's
        // stack for the entire event loop; pointer installed before
        // any invokable can fire.
        let cb = unsafe { &mut *ptr };
        cb.viewport_mut().map(f)
    })
}
```

## Alternatives considered

### α — `Arc<Mutex<CommandQueue>>` polled per frame by `ViewportCallbacks`

**Why rejected.** The queue buys three things — thread safety, frame coherence, testability. Qt UI is single-threaded, so thread safety is irrelevant. `SceneManager::load_usd_scene` is already synchronous and blocks on GPU upload; there's no mid-frame-coherence issue a queue would avoid. Testability is the only real benefit, and we can swap β for α with zero invokable changes if we ever need it. Over-engineering for the current constraint.

### γ — `Renderer.scene: Arc<Mutex<SceneManager>>`

**Why rejected.** Invasive — every `self.scene.*` access site inside `Renderer` (dozens) grows a lock. Buys nothing the other options don't; creates a new class of deadlock risk.

## Safety rationale for β

1. **Single-threaded access.** Qt UI invokables run on the main thread. The viewport-frame callback (`viewport_on_frame`) is invoked from `RenderWidget::paintEvent`, also main-thread. Rust's `thread_local!<Cell<*mut _>>` is lock-free and safe under single-threaded access.
2. **Straight-line pointer lifetime.** `ViewportCallbacks` is owned by `app.rs::run`'s local binding; it lives on the stack until `bif_qt_run_shell` returns. `install_viewport_callbacks(&mut viewport_cb as *mut _)` runs before `bif_qt_run_shell`. No invokable can fire before the event loop starts. When the event loop exits and the stack frame unwinds, no more invokables can fire.
3. **Null-check + `Option` return.** `with_viewport_mut` returns `None` if the pointer is null (not installed) or if the `Viewport` inside `ViewportCallbacks` is `None` (surface not yet ready / already shut down). Invokables must handle `None` gracefully.
4. **No aliasing.** Only `BifShellState` invokables use `VIEWPORT_CALLBACKS` to reach the renderer; `app.rs` never reads it back. `ViewportCallbacks`'s own C++ callbacks (`viewport_on_frame` / `viewport_on_resize`) receive `&mut ViewportCallbacks` through a different path (direct cxx-qt `extern "Rust"`) — they do not go through this thread_local. So a single callsite owns the raw-pointer deref at a time.

## Reversibility

If we ever need cross-thread access (e.g., async scene load on a background thread), swap the internals of `with_viewport_mut` to dereference an `Arc<Mutex<ViewportCallbacks>>` without touching any invokable call sites. The `with_viewport_mut(|vp| ...)` closure API survives the migration.

## Trade-offs accepted

- **Global-ish state.** `thread_local` is cleaner than a `static`, but still singleton. Only one Qt shell per thread is a reasonable constraint for a desktop app.
- **One `unsafe` block.** Encapsulated in a two-line function with a documented invariant.
- **No compile-time guarantee that `install_viewport_callbacks` was called.** If someone forgets, invokables silently no-op (return `None`). An `expect("viewport callbacks not installed")` inside `with_viewport_mut` would catch this at runtime — acceptable, and cleaner than forcing every call-site to handle the install-ordering invariant.

## Consequences

- Move 2 (`load_stage_at_path` / extended `on_stage_path_opened`) ships against this bridge.
- Move 4 (timeline detection) can be revisited to read from the live stage in the renderer's `SceneManager` instead of reopening a throwaway stage (follow-up optimization).
- Future invokables that need to touch the renderer (camera orbit/pan/zoom, frame-selected, node-graph ops) follow the same pattern: `with_viewport_mut(|vp| vp.renderer_mut().…)`.
