---
title: "Borrow Checker Patterns"
type: concept
tags: [rust, patterns, concurrency]
created: "2026-04-07"
updated: "2026-04-07"
---

## Summary

Common patterns for working with Rust's borrow checker in BIF, especially around `MutexGuard` lifetimes and `&mut self` conflicts.

## Patterns

### 1. Guard Extraction

When a MutexGuard is used multiple times, extract into a named variable:

```rust
// Bad: temporary dropped too early
if let Ok(pos) = stage.lock().unwrap().get_vertices(idx, t) { ... }

// Good: guard lives for the block
let guard = stage.lock().expect("UsdStage mutex poisoned");
if let Ok(pos) = guard.get_vertices(idx, t) { ... }
```

### 2. Pre-Extraction (Lock-Then-Drop)

When you need data from a locked resource AND need `&mut self` afterward:

```rust
// Query under lock, capture owned results
let (xform, props) = {
    let stage = stage_mtx.lock().expect("...");
    (stage.get_xform(path, time), stage.get_props(path, time))
}; // guard dropped here

// Now safe to mutate self
self.update_camera();
```

### 3. as_deref on Option<MutexGuard>

Convert `Option<MutexGuard<T>>` to `Option<&T>` for trait object casting:

```rust
let guard = self.scene.usd_stage.as_ref().map(|s| s.lock().expect("..."));
let provider = guard.as_deref().map(|s| s as &dyn PrimDataProvider);
```

### 4. Map-Then-Drop for Mutations

When a locked mutation must complete before calling `&mut self`:

```rust
let result = self.scene.usd_stage.as_ref().map(|s| {
    s.lock().expect("...").set_variant_selection(&path, &set, &name)
});
// Guard dropped — safe to call &mut self
match result { ... }
```

### 5. Lock-Once-Before-Loop

Avoid lock-per-iteration in hot loops:

```rust
// Bad: locks N times
for &idx in &meshes {
    stage.lock().unwrap().get_vertices(idx, t);
}

// Good: locks once
let guard = stage.lock().expect("...");
for &idx in &meshes {
    guard.get_vertices(idx, t);
}
```

## Anti-Patterns

- **Double-locking non-reentrant Mutex** — `std::sync::Mutex` deadlocks if locked twice on same thread
- **`.lock().unwrap()`** — use `.expect("context")` for diagnostics

## See Also

- [[UsdStage Thread Safety]] — why Mutex is needed for UsdStage
