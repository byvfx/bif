---
title: PrimDataProvider trait
type: concept
tags: [usd, ffi, scene-browser, v0.15]
created: 2026-04-16
updated: 2026-04-16
---

# PrimDataProvider trait

## Summary

Abstraction over "something that can answer prim-hierarchy + display-info queries" — implemented on `UsdStage` (and planned for future `CompositeProvider` that merges USD + procedural). The Scene Browser and Property Inspector invokables read through this trait instead of talking to USD directly, keeping the FFI surface stable when procedural prims land.

## Methods used by Phase E.2

- `root_paths() -> Vec<String>`
- `get_children(path: &str) -> Vec<String>`
- `get_prim_info(path: &str) -> Option<PrimDisplayInfo>` — display name, type name, etc.

## Shadowing gotcha

`UsdStage` has an **inherent** `get_prim_info(index: usize) -> UsdBridgeResult<UsdPrimInfo>` AND this trait's `get_prim_info(path: &str)`. Calling `stage.get_prim_info(&str)` dispatches to the inherent — compiler tries to coerce `&str` → `usize`, fails with E0308.

Workarounds:

- Use the inherent escape hatch: `get_prim_info_by_path(&str)`.
- Or disambiguate via UFCS: `<UsdStage as PrimDataProvider>::get_prim_info(&stage, path)`.

Phase E.2 moves 5/7/8 use the `_by_path` inherent for prim-type lookup (gizmo pick → `selected_prim_type`) and the trait for bulk hierarchy traversal (Scene Browser).

## Where it's called

- `crates/bif_qt/src/main_window.rs` — Scene Browser invokables (`root_prim_count`, `child_prim_count`, etc.) and `selected_prim_type` resolution on pick.
- `crates/bif_core/src/usd/` — trait definition + `impl PrimDataProvider for UsdStage`.

## Future

A `CompositeProvider` that merges the USD stage tree with procedural prims (see [[../architecture/scene-browser|Scene Browser]]) will implement this trait too. Swapping providers will not ripple through the invokable layer.

## See also

- [[../architecture/scene-browser|Scene Browser]]
- [[../architecture/cxx-qt-bridge-patterns|cxx-qt bridge patterns]]
- [[../architecture/adr/005-layer-aware-read-model|ADR-005: layer-aware read model]]
