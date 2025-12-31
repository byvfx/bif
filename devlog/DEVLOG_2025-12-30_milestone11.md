# Devlog: Milestone 11 - Ivar Viewport Integration

**Date:** 2025-12-30  
**Duration:** ~4 hours  
**Status:** ✅ Complete

## Objective

Integrate the `bif_renderer` CPU path tracer ("Ivar") into the viewport with seamless mode switching between Vulkan GPU preview and Ivar CPU path tracing.

## Implementation Summary

### Render Mode Switching

Added `RenderMode` enum to toggle between renderers:

```rust
pub enum RenderMode {
    Vulkan,  // Real-time GPU viewport rendering
    Ivar,    // CPU path tracer for production quality
}
```

Users can switch modes via dropdown in the egui side panel. The viewport maintains state for both renderers.

### Ivar State Management

```rust
struct IvarState {
    mode: RenderMode,
    samples_per_pixel: u32,
    max_depth: u32,
    world: Option<Arc<BvhNode>>,       // Shared scene
    buckets: Vec<Bucket>,              // Render buckets
    receiver: Option<Receiver<BucketResult>>,
    cancel_flag: Arc<AtomicBool>,
    image_buffer: Option<ImageBuffer>, // Accumulated result
    buckets_completed: usize,
    render_complete: bool,
}
```

### Background Bucket Rendering

Ivar renders in background threads using a bucket-based approach:
1. Scene divided into 64x64 pixel buckets
2. Each bucket rendered independently via rayon thread pool
3. Results sent via channel to main thread
4. Main thread composites into GPU texture for display

### Scene Building with Instancing

When switching to Ivar mode, the viewport builds a BVH from mesh data with full instancing support:

```rust
fn build_ivar_scene(&mut self) {
    // Create triangles for each instance by transforming vertices
    for transform in &self.instance_transforms {
        for (i0, i1, i2) in indices.chunks(3) {
            let v0 = transform.transform_point3(v0_local);
            let v1 = transform.transform_point3(v1_local);
            let v2 = transform.transform_point3(v2_local);
            let tri = Triangle::new(v0, v1, v2, Lambertian::new(grey));
            objects.push(Box::new(tri));
        }
    }
    self.ivar_state.world = Some(Arc::new(BvhNode::new(objects)));
}
```

**Performance:** 100 Lucy instances = 28,055,600 triangles built into BVH

## Bug Fix: Left-Handed Winding Order

### Problem

Meshes exported from Houdini displayed inverted normals in both Vulkan and Ivar renderers. Surfaces appeared dark (lit from behind) instead of properly shaded.

### Root Cause

USD files from Houdini specify `orientation = "leftHanded"`:

```usda
def Mesh "mesh_0" {
    uniform token orientation = "leftHanded"
    int[] faceVertexIndices = [0, 1, 3, 2, 4, 5, 7, 6, ...]
    ...
}
```

Left-handed orientation means clockwise (CW) winding when looking at front faces, opposite of the standard counter-clockwise (CCW) convention used by:
- wgpu's `FrontFace::Ccw` setting
- Cross product normal calculation (edge1 × edge2 points toward viewer for CCW)

The USD parser wasn't handling this attribute.

### Solution

**1. Added `left_handed` field to `UsdMesh`** ([types.rs](../crates/bif_core/src/usd/types.rs)):

```rust
pub struct UsdMesh {
    // ... existing fields
    pub left_handed: bool,
}
```

**2. Updated triangulation to reverse winding** ([types.rs](../crates/bif_core/src/usd/types.rs)):

```rust
pub fn triangulate(&self) -> Vec<u32> {
    if self.left_handed {
        // Reverse winding: swap i1 and i2
        indices.push(i0);
        indices.push(i2);
        indices.push(i1);
    } else {
        indices.push(i0);
        indices.push(i1);
        indices.push(i2);
    }
}
```

**3. Added orientation parsing** ([parser.rs](../crates/bif_core/src/usd/parser.rs)):

```rust
if trimmed.contains("orientation") && trimmed.contains("\"leftHanded\"") {
    mesh.left_handed = true;
}
```

### Result

Both Vulkan and Ivar now correctly render left-handed meshes from Houdini. Normals point outward, shading is correct.

## Bug Fix: Ivar Instancing

### Problem

Only the first instance rendered in Ivar mode while Vulkan showed all 100.

### Root Cause

`build_ivar_scene()` only created triangles for the prototype mesh at origin, ignoring instance transforms.

### Solution

1. Added `instance_transforms: Vec<Mat4>` to Renderer struct
2. Collected transforms during scene loading (same data as GPU instance buffer)
3. Updated `build_ivar_scene()` to iterate through all instances and transform vertices to world space

## Tests

All 15 `bif_core` tests pass.

## Files Modified

| File | Changes |
|------|---------|
| `crates/bif_core/src/usd/types.rs` | Added `left_handed` field, updated `triangulate()` |
| `crates/bif_core/src/usd/parser.rs` | Parse `orientation` attribute |
| `crates/bif_viewport/src/lib.rs` | Ivar integration, instancing support, render mode toggle |
| `crates/bif_viewer/src/main.rs` | Default to USDA loading, wgpu log suppression |

## Performance

| Metric | Value |
|--------|-------|
| **Vulkan FPS** | 60+ (VSync) |
| **Vulkan Draw Calls** | 1 (instanced) |
| **Ivar Triangles** | 28,055,600 |
| **Ivar BVH Build** | ~4 seconds |
| **Ivar Render** | Progressive @ 16 SPP |

## Milestone Complete ✅

Both renderers now support:
- ✅ USD mesh loading with left-handed orientation fix
- ✅ GPU instancing (Vulkan) / Baked instancing (Ivar)
- ✅ Render mode toggle in UI
- ✅ Progressive bucket rendering display
- ✅ Camera sync between modes
