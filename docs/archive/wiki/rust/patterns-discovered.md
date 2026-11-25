---
title: "Rust Patterns Discovered in BIF"
type: article
tags: [rust, patterns, learning]
created: "2026-04-05"
updated: "2026-04-05"
---

## Summary

A living document of Rust patterns encountered, understood, and applied while building BIF. Each pattern includes where it appears in the codebase and what problem it solves.

## Patterns

### Enum Dispatch (over Trait Objects)

**What**: Using an enum with variants instead of `Box<dyn Trait>` for polymorphism.

**Where**: `BvhNode` in `crates/bif_renderer/src/bvh.rs` -- `Branch`, `Leaf`, `Empty` variants instead of a `dyn BvhNode` trait.

**Why**: Avoids heap allocation per node, enables better cache locality, no vtable indirection. The compiler knows all variants at compile time, so it can optimize match arms.

**Trade-off**: Closed set of types -- can't add new variants without modifying the enum. Fine when you control all variants (BVH nodes), bad when extensibility is needed.

### Feature Flags for Optional Dependencies

**What**: Cargo feature flags to conditionally compile functionality.

**Where**: `oiio` and `oidn` features in `Cargo.toml`. The viewer degrades gracefully without them.

**Why**: Not every developer has OIDN or OpenImageIO installed. Feature flags keep the build working without optional C/C++ dependencies.

**Pattern**: `#[cfg(feature = "oidn")]` guards + fallback paths in the UI.

### FFI via build.rs + CMake

**What**: Using `build.rs` to invoke CMake to build a C++ library, then linking it via `cc` or manual link directives.

**Where**: `bif_core/build.rs` builds `cpp/usd_bridge/` -- the C++ USD bridge.

**Why**: USD is a C++ library. Rust can't call C++ directly, so a C-compatible bridge layer is built by CMake, and Rust calls it via `extern "C"` FFI (`bif_core::usd::ffi_raw`).

**Lessons learned**: Path handling on Windows is painful. `--test-threads=1` required because USD's C++ side is not thread-safe.

### Trait Objects with Send + Sync Bounds

**What**: `Box<dyn Hittable + Send + Sync>` -- trait objects that are safe to share across threads.

**Where**: BVH leaf nodes in `bif_renderer/src/bvh.rs`.

**Why**: The renderer wants to trace rays in parallel. All geometry in the BVH must be safe to access from multiple threads. The `Send + Sync` bounds enforce this at compile time.

### Arc<Mutex<T>> for Shared Mutable State

**What**: Reference-counted pointer with interior mutability.

**Where**: Scene graph data shared between the viewport and renderer. Texture caches. USD stage handle.

**Why**: Multiple subsystems need read/write access to the same data. `Arc` enables shared ownership, `Mutex` serializes writes.

**Caution**: Deadlocks are possible. BIF avoids holding locks across async boundaries.

### Immediate-Mode UI (egui)

**What**: UI is a function of state -- no persistent widget objects. Every frame, you describe the entire UI from scratch.

**Where**: All of `bif_viewport` and `bif_viewer`.

**Why**: Simple mental model, no widget lifecycle management, easy to prototype. The trade-off is performance (full UI rebuild each frame) and limited styling.

**BIF-specific**: All UI logic is kept separate from domain logic to enable future Qt migration. See `project_qt_migration.md`.

### Serde for Persistence

**What**: Derive `Serialize`/`Deserialize` on structs for automatic JSON/binary serialization.

**Where**: `bif_viewport::persistence` -- node graph state saved/loaded via serde.

**Why**: Zero boilerplate persistence. Serde handles nested structs, enums, Options, Vecs automatically.

## Patterns to Learn Next

- **Type-state pattern**: Encoding state machine transitions in the type system (e.g., `Pipeline<Building>` vs `Pipeline<Ready>`).
- **Newtype pattern**: Wrapping primitive types for type safety (`struct NodeId(u64)`).
- **Builder pattern**: Constructing complex structs step-by-step (relevant for the ~75-field Renderer).
- **Error handling with thiserror/anyhow**: Structured error types vs quick-and-dirty `anyhow::Result`.
