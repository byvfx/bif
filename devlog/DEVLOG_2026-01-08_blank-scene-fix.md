# Devlog: Blank Scene Startup Fix

**Date:** January 8, 2026
**Duration:** ~45 minutes
**Status:** ✅ Complete

## Overview

Fixed bug where lucy_low.obj was auto-loading when starting bif_viewer without CLI arguments. Now starts with a truly blank scene.

## Implementation

### Changes to Renderer::new()

**Removed (~40 lines):**
- lucy_low.obj file path and loading
- 10x10 grid instance generation
- Mesh-based camera positioning

**Replaced with:**
```rust
// Empty mesh data
let mesh_data = MeshData {
    vertices: vec![],
    indices: vec![],
    bounds_min: Vec3::new(0.0, 0.0, 0.0),
    bounds_max: Vec3::new(0.0, 0.0, 0.0),
};

// Default camera position
let mut camera = Camera::new(
    Vec3::new(0.0, 10.0, 50.0),  // Fixed position
    Vec3::new(0.0, 0.0, 0.0),    // Look at origin
    aspect,
);
camera.near = 0.1;
camera.far = 1000.0;

// Dummy buffers (wgpu requires non-zero size)
let dummy_vertex = Vertex {
    position: [0.0, 0.0, 0.0],
    normal: [0.0, 1.0, 0.0],
    color: [1.0, 1.0, 1.0],
};
// ... similar for index/instance buffers

// Zero counts
num_indices: 0,
num_instances: 0,
```

### Log Message Update

**main.rs line 193:**
```rust
log::info!("Starting with blank scene (load USD via node graph)");
```

## Behavior

| Scenario | Old | New |
|----------|-----|-----|
| No CLI args | Loads lucy_low.obj (10x10 grid) | Blank scene (clear color only) |
| --usd flag | Loads via C++ bridge | ✅ Unchanged |
| --usda flag | Loads via Rust parser | ✅ Unchanged |
| Node graph | N/A | ✅ Works (load_usd_scene) |

## Verification

```bash
# Build checks
cargo build              # ✅ Clean (0 warnings)
cargo clippy -- -D warnings  # ✅ Pass

# Runtime tests
bif_viewer                         # ✅ Blank scene
bif_viewer --usd test_cube.usda    # ✅ Loads cube
```

**Logs (blank scene):**
```
[INFO] Starting with blank scene (load USD via node graph)
[INFO] Initializing blank scene (no default geometry)
[INFO] Camera positioned at Vec3(0.0, 10.0, 50.0), looking at Vec3(0.0, 0.0, 0.0)
[INFO] Created empty vertex/index/instance buffers
```

## Files Changed

| File | Lines Changed | Description |
|------|---------------|-------------|
| `crates/bif_viewport/src/lib.rs` | ~40 removed, ~20 added | Empty scene init |
| `crates/bif_viewer/src/main.rs` | 1 modified | Log message |

## Known Issues Fixed

- ✅ SESSION_HANDOFF.md: "Lucy still loads when no CLI args"

## Next Steps

- Milestone 14: MaterialX integration
- Node graph: Add more node types (Merge, Transform)
