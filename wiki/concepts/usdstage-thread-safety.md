---
title: "UsdStage Thread Safety"
type: concept
tags: [rust, ffi, concurrency, usd]
created: "2026-04-07"
updated: "2026-04-07"
---

## Summary

`UsdStage` wraps a raw C++ pointer to OpenUSD's `UsdStageRefPtr`. The C++ USD library is not thread-safe for concurrent mutations. In Rust, this means `UsdStage` can be `Send` (transferable between threads) but NOT `Sync` (not safe for concurrent `&UsdStage` access).

## Details

### Why Not Sync

Several `UsdStage` methods mutate C++ state through `&self` (shared reference) by casting the raw pointer:
- `set_variant_selection()` — changes variant selection on the stage
- `load_payload()` / `unload_payload()` — modifies payload loading state

These perform interior mutation invisible to Rust's borrow checker. Concurrent `&UsdStage` access from multiple threads could cause data races in the C++ layer.

### Why Send Is Safe

The raw pointer is exclusively owned — only one `UsdStage` wraps a given `UsdBridgeStageRaw` at a time. `Drop` releases it. Moving to another thread transfers ownership cleanly.

### Access Pattern: Arc<Mutex<UsdStage>>

Shared access uses `Arc<Mutex<UsdStage>>`:
- `Mutex<T>: Sync` requires only `T: Send` (not `T: Sync`)
- So `Arc<Mutex<UsdStage>>` is `Send + Sync` even though `UsdStage` is only `Send`
- Each access site calls `.lock().expect("UsdStage mutex poisoned")`

### Borrow Checker Interactions

MutexGuard lifetime causes common patterns:
1. **Guard extraction** — `let guard = mtx.lock().expect(...)` for multi-use sites
2. **Pre-extraction** — query data under lock, drop guard, then mutate `self`
3. **as_deref pattern** — `Option<MutexGuard<T>>.as_deref()` gives `Option<&T>` cleanly

## BIF Context

- `scene_manager.rs` holds `Option<Arc<Mutex<UsdStage>>>`
- Batch render clones the Arc for worker threads — Mutex serializes FFI calls
- Animation paths lock once before loops (not per-iteration) for performance

## See Also

- [[Borrow Checker Patterns]] — MutexGuard lifetime patterns
- `crates/bif_core/src/usd/cpp_bridge.rs` lines 839-848 — safety comment
