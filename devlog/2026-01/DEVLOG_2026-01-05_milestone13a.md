# Devlog: Milestone 13a - USD Scene Browser + Property Inspector

**Date:** January 5, 2026
**Duration:** ~3 hours
**Status:** ✅ Complete

## Overview

Added Gaffer-inspired Scene Browser and Property Inspector panels to the egui UI, enabling interactive USD hierarchy navigation.

## Implementation

### 1. C++ Bridge Extensions (`cpp/usd_bridge/`)

Added 7 new prim traversal APIs:

- `usd_bridge_prim_count()` - Total prim count
- `usd_bridge_get_prim_info()` - Type, active, children info
- `usd_bridge_get_prim_children()` - Child paths
- `usd_bridge_get_root_prims()` - Stage root prims
- `usd_bridge_get_world_transform()` - Computed world matrix
- `usd_bridge_get_local_transform()` - Local matrix
- `usd_bridge_get_bounding_box()` - World-space AABB

### 2. Scene Browser (`scene_browser.rs`)

~420 LOC tree widget with:

- `SceneBrowserState` - expanded/selected paths, filter
- `PrimDisplayInfo` - display-ready prim data
- `PrimDataProvider` trait - abstraction for USD source
- Type icons (🔷 Mesh, 📐 Xform, 🔁 Instancer, etc.)
- Search/filter box with clear button
- Show/hide inactive prims toggle

### 3. Property Inspector (`property_inspector.rs`)

~250 LOC panel showing:

- Prim path and type
- Local and world transforms (4x4 matrices)
- Bounding box (min/max/extent)
- Placeholder for USD attributes

### 4. Renderer Integration

- Added `load_usd_with_stage()` returning `(Scene, UsdStage)` tuple
- Added `usd_stage` field to `Renderer`
- Created `new_with_scene_and_stage()` constructor
- Implemented `PrimDataProvider` for `UsdStage`
- Wired panels into egui (left: browser, right: inspector)

## Key Decisions

### PrimDataProvider Trait

Abstraction allows scene browser to work with:
- C++ USD bridge (full USDC support)
- Pure Rust parser (fallback)
- Mock data (testing)

### Selection Model

Single selection synced between browser and inspector. Future: Gaffer-style focus vs selection for multi-select operations.

## Files Changed

| File | Change |
|------|--------|
| `cpp/usd_bridge/usd_bridge.cpp` | +223 lines (prim APIs) |
| `cpp/usd_bridge/usd_bridge.h` | +105 lines (headers) |
| `crates/bif_core/src/usd/cpp_bridge.rs` | +247 lines (FFI wrappers) |
| `crates/bif_core/src/usd/loader.rs` | +20 lines (load_with_stage) |
| `crates/bif_viewport/src/scene_browser.rs` | +419 lines (new) |
| `crates/bif_viewport/src/property_inspector.rs` | +252 lines (new) |
| `crates/bif_viewport/src/lib.rs` | +80 lines (integration) |

## Next Steps

- Milestone 13b: Node Graph for scene assembly
- Table layout like Houdini Scene Graph Tree
- Dynamic USD loading from node graph
