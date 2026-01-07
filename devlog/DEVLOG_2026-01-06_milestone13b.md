# Devlog: Milestone 13b - Node Graph + Dynamic USD Loading

**Date:** January 6, 2026
**Duration:** ~2 hours
**Status:** ✅ Complete

## Overview

Implemented Nuke-style node graph for scene assembly and upgraded the Scene Browser to Houdini-style table layout.

## Implementation

### 1. Node Graph (`node_graph.rs`)

~400 LOC using egui-snarl 0.5:

**Node Types:**
- `UsdRead` - File path + Browse button + Load status
- `IvarRender` - SPP slider + Render button

**Features:**
- Type-safe pin connections (Scene→Scene, Image→Image)
- `NodeGraphEvent` enum for parent communication
- Native file dialog via `rfd` crate
- Delete key removes selected nodes
- Context menu for adding nodes

**Events:**
```rust
pub enum NodeGraphEvent {
    LoadUsdFile(String),
    StartRender { spp: u32 },
}
```

### 2. Dynamic USD Loading

Added `load_usd_scene()` to Renderer (~80 lines):

1. Load USD via C++ bridge
2. Create GPU buffers (vertex, index, instance)
3. Update scene browser with new hierarchy
4. Frame camera on scene bounds
5. Invalidate Ivar BVH cache

### 3. Houdini-Style Scene Browser

Redesigned table layout with columns:

| Scene Graph Path | Prim Type | Children | Kind | 👁 |
|------------------|-----------|----------|------|-----|
| Tree with indent | Type name | Count    | USD  | Viz |

- Header row with column labels
- `ColumnWidths` struct for consistent sizing
- Added `kind` and `is_visible` fields to `PrimDisplayInfo`

### 4. Empty Viewport Startup

Modified `main.rs` to start with empty viewport when no CLI args:
- No `--usda`/`--usd` flag → empty renderer
- User adds USD Read node → Browse → loads scene

## Dependencies Added

```toml
egui-snarl = "0.5"  # Node graph widget
rfd = "0.14"        # Native file dialogs
```

## Key Fixes

### 1. node_ids() Returns Tuples

```rust
// Wrong: let ids = self.snarl.node_ids();
// Right: node_ids() returns (NodeId, &SceneNode) tuples
let node_ids: Vec<_> = self.snarl.node_ids()
    .map(|(id, _)| id)
    .collect();
```

### 2. Event Processing After egui

Node events stored in `egui::Context` data and processed after `egui_ctx.run()` completes, avoiding borrow conflicts.

## Files Changed

| File | Change |
|------|--------|
| `crates/bif_viewport/Cargo.toml` | +2 lines (egui-snarl, rfd) |
| `crates/bif_viewport/src/node_graph.rs` | +350 lines (new) |
| `crates/bif_viewport/src/scene_browser.rs` | +200/-80 lines (table layout) |
| `crates/bif_viewport/src/lib.rs` | +150 lines (load_usd_scene, events) |
| `crates/bif_viewer/src/main.rs` | +10/-20 lines (empty startup) |

## Tests

All 11 tests passing:
- `test_node_creation` - Node types and pin counts
- `test_pin_types` - Pin type matching
- `test_node_graph_state` - Default nodes
- `test_empty_node_graph` - Empty start
- Scene browser tests unchanged

## Next Steps

- Milestone 14: MaterialX integration
- Add more node types (Merge, Transform, Sublayer)
- Multi-select in scene browser
