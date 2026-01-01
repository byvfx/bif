# BIF Code Reference - Milestones 0-11 Patterns

> Best practices and patterns learned from implementing Milestones 0-11

**Last Updated:** December 31, 2025

This document captures actual code patterns, solutions, and lessons from completing Milestones 0-11. For milestone history, see [MILESTONES.md](MILESTONES.md).

---

## Code Patterns

### 1. Instance-Aware Rendering

**Problem:** Rendering 100+ instances without duplicating geometry

**Solution:** Per-instance transform buffer + instanced draw call

**Example:**

```rust
// See: crates/bif_viewport/src/lib.rs:1200-1250
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct InstanceData {
    pub model_matrix: [[f32; 4]; 4],
}

// Generate instances
let instance_data: Vec<InstanceData> = self.instance_transforms
    .iter()
    .map(|transform| InstanceData {
        model_matrix: transform.to_cols_array_2d(),
    })
    .collect();

// Single draw call for all 100 instances
render_pass.draw_indexed(0..self.index_count, 0, 0..instance_count);
```

**Key Files:**

- [bif_viewport/src/lib.rs:1200-1250](crates/bif_viewport/src/lib.rs#L1200-L1250) - GPU instancing
- [bif_viewport/src/shaders/basic.wgsl](crates/bif_viewport/src/shaders/basic.wgsl) - Shader instance handling

**Performance:** 100 instances @ 60+ FPS, single draw call, 28M triangles

---

### 2. Background Scene Building

**Problem:** 4-second UI freeze when building BVH for 28M triangles

**Solution:** Background thread + non-blocking status polling

**Example:**

```rust
// See: crates/bif_viewport/src/lib.rs:1651-1736
enum BuildStatus {
    NotStarted,
    Building,
    Complete { scene: IvarScene },
    Failed(String),
}

// Spawn background thread
let (tx, rx) = mpsc::channel();
std::thread::spawn(move || {
    let scene = build_ivar_scene(...);
    tx.send(scene).unwrap();
});

// Poll without blocking
if let Ok(scene) = self.scene_rx.try_recv() {
    self.ivar_scene = Some(scene);
    self.build_status = BuildStatus::Complete { scene };
}
```

**Key Files:**

- [bif_viewport/src/lib.rs:1651-1736](crates/bif_viewport/src/lib.rs#L1651-L1736) - Background threading
- [bif_renderer/src/instanced_geometry.rs](crates/bif_renderer/src/instanced_geometry.rs) - Instance-aware BVH

**Performance:** 0ms UI freeze (was 4s), ~40ms BVH build time

---

### 3. USD Left-Handed Orientation Fix

**Problem:** Meshes from Houdini render inside-out due to winding order

**Solution:** Detect `orientation = "leftHanded"` and swap triangle indices

**Example:**

```rust
// See: crates/bif_core/src/usd/parser.rs:150-180
if mesh.left_handed {
    // Swap i1 and i2 to convert left-handed → CCW for GPU/Ivar
    triangles.push([i0, i2, i1]);
} else {
    triangles.push([i0, i1, i2]);
}
```

**Key Files:**

- [bif_core/src/usd/parser.rs:150-180](crates/bif_core/src/usd/parser.rs#L150-L180) - Orientation detection
- [bif_core/src/usd/types.rs](crates/bif_core/src/usd/types.rs) - `left_handed` field
- [HOUDINI_EXPORT.md](HOUDINI_EXPORT.md) - Best practices guide

**Rationale:** Houdini USD exports use left-handed coordinates, but both Vulkan and Ivar expect CCW winding.

---

### 4. egui Integration with wgpu

**Problem:** egui requires specific initialization and lifetime management with wgpu

**Solution:** Two-pass rendering (3D scene, then egui overlay) with `.forget_lifetime()`

**Example:**

```rust
// See: crates/bif_viewport/src/lib.rs:1800-1900
// Pass 1: 3D scene
{
    let mut render_pass = encoder.begin_render_pass(&RenderPassDescriptor {
        depth_stencil_attachment: Some(...),
        ...
    });
    render_pass.draw_indexed(0..index_count, 0, 0..instance_count);
}

// Pass 2: egui overlay
{
    let mut render_pass = encoder.begin_render_pass(&RenderPassDescriptor {
        depth_stencil_attachment: None,  // No depth for UI
        ...
    }).forget_lifetime();  // Required for egui's 'static requirement

    self.egui_renderer.render(&mut render_pass, &paint_jobs, &screen_descriptor);
}
```

**Key Files:**

- [bif_viewport/src/lib.rs:1800-1900](crates/bif_viewport/src/lib.rs#L1800-L1900) - egui rendering

**Challenges Solved:**

- egui `State::new()` requires 6 parameters, `Renderer::new()` requires 5
- Borrow checker: Extract UI data BEFORE `egui_ctx.run()` closure
- Lifetime issues: Use `.forget_lifetime()` on RenderPass for egui's `'static` requirement

---

### 5. Progressive Bucket Rendering

**Problem:** Long-running Ivar renders need to show progress

**Solution:** Divide image into buckets, render in parallel, composite incrementally

**Example:**

```rust
// See: crates/bif_renderer/src/lib.rs:200-300
const BUCKET_SIZE: u32 = 64;

// Generate buckets
let buckets: Vec<(u32, u32, u32, u32)> = (0..height)
    .step_by(BUCKET_SIZE)
    .flat_map(|y| {
        (0..width).step_by(BUCKET_SIZE).map(move |x| {
            (x, y,
             (x + BUCKET_SIZE).min(width),
             (y + BUCKET_SIZE).min(height))
        })
    })
    .collect();

// Render buckets in parallel
buckets.par_iter().for_each(|(x0, y0, x1, y1)| {
    for y in *y0..*y1 {
        for x in *x0..*x1 {
            let color = render_pixel(x, y, scene, camera);
            tx.send((x, y, color)).unwrap();
        }
    }
});
```

**Key Files:**

- [bif_renderer/src/lib.rs:200-300](crates/bif_renderer/src/lib.rs#L200-L300) - Bucket system

**Performance:** Progressive display allows UI interaction during render

---

### 6. Mat4 Transform Operations

**Problem:** Need to transform rays and AABBs for instance-aware BVH

**Solution:** Extension trait on `Mat4` with `glam` integration

**Example:**

```rust
// See: crates/bif_math/src/transform.rs
pub trait Mat4Ext {
    fn transform_vector3(&self, v: Vec3) -> Vec3;
    fn transform_aabb(&self, aabb: &Aabb) -> Aabb;
}

impl Mat4Ext for Mat4 {
    fn transform_vector3(&self, v: Vec3) -> Vec3 {
        let v4 = self.mul_vec4(v.extend(0.0));
        Vec3::new(v4.x, v4.y, v4.z)
    }

    fn transform_aabb(&self, aabb: &Aabb) -> Aabb {
        // Transform all 8 corners and recompute bounds
        ...
    }
}
```

**Key Files:**

- [bif_math/src/transform.rs](crates/bif_math/src/transform.rs) - Mat4 extensions

**Tests:** 8 unit tests covering identity, translation, rotation, AABB transforms

---

### 7. Instance-Aware BVH Architecture

**Problem:** Building 28M triangles (100 instances × 280K triangles) caused 4s freeze

**Solution:** ONE BVH for prototype, transform rays per-instance

**Example:**

```rust
// See: crates/bif_renderer/src/instanced_geometry.rs
pub struct InstancedGeometry {
    prototype_bvh: BvhNode,           // ONE BVH (280K triangles)
    instance_transforms: Vec<Mat4>,   // 100 transforms
}

impl Hittable for InstancedGeometry {
    fn hit(&self, ray: &Ray, ray_t: Interval) -> bool {
        for transform in &self.instance_transforms {
            // Transform ray: world → local
            let inverse = transform.inverse();
            let local_ray = Ray {
                origin: inverse.transform_point3(ray.origin),
                direction: inverse.transform_vector3(ray.direction),
                ...
            };

            // Test against prototype BVH
            if self.prototype_bvh.hit(&local_ray, ray_t) {
                // Transform hit back: local → world
                ...
                return true;
            }
        }
        false
    }
}
```

**Key Files:**

- [bif_renderer/src/instanced_geometry.rs](crates/bif_renderer/src/instanced_geometry.rs) - Full implementation

**Performance:**

- BVH build: 4000ms → 40ms (100x faster)
- Memory: 5GB → 50MB (100x reduction)
- Trade-off: ~3x slower rendering due to linear instance search O(100)

**Tests:** 5 unit tests (identity transform, multiple instances, correctness, rotation)

---

### 8. USD USDA Parser (Pure Rust)

**Problem:** Need to load USD files without C++ dependencies

**Solution:** Hand-written parser for USDA text format

**Example:**

```rust
// See: crates/bif_core/src/usd/parser.rs
pub fn parse_usda(path: &Path) -> Result<UsdScene> {
    let content = fs::read_to_string(path)?;
    let mut scene = UsdScene::default();

    // Parse primitives
    for line in content.lines() {
        if line.contains("def Mesh") {
            let mesh = parse_mesh(&mut lines)?;
            scene.meshes.push(mesh);
        }
        if line.contains("def PointInstancer") {
            let instancer = parse_point_instancer(&mut lines)?;
            scene.instancers.push(instancer);
        }
    }

    Ok(scene)
}
```

**Supported:**

- `UsdGeomMesh` - positions, normals, faceVertexCounts, faceVertexIndices
- `UsdGeomPointInstancer` - protoIndices, positions, orientations, scales
- `orientation = "leftHanded"` detection
- N-gon triangulation via fan triangulation

**Key Files:**

- [bif_core/src/usd/parser.rs](crates/bif_core/src/usd/parser.rs) - USDA Parser
- [bif_core/src/usd/types.rs](crates/bif_core/src/usd/types.rs) - USD types

**Tests:** 15 tests in `bif_core` covering mesh loading, instancing, orientation

**Limitations:** Text format only (no USDC binary), no references yet

---

## Performance Targets vs Actuals

| Target | Actual (Milestones 0-11) | Notes |
|--------|--------------------------|-------|
| 10K instances @ 60 FPS | 100 instances @ 60+ FPS | VSync-limited, Milestone 12 (Embree) needed for 10K+ |
| BVH build < 100ms | ~40ms | Instance-aware approach |
| Memory for 100 instances | ~50MB | 100x reduction vs duplicating geometry |
| Ivar render time | ~52s (479 objects, 800x450, 100spp) | Acceptable baseline, ~3x slower than Embree would be |
| UI freeze on mode switch | **0ms** | Was 4s before background threading |

---

## Testing Strategy

### Unit Tests (60+ passing)

**bif_math (26 tests):**

- `Vec3` operations (dot, cross, length)
- `Ray::at()` position calculation
- `Interval` contains/clamp/expand
- `Aabb` hit testing, combining, longest axis
- `Camera` view/projection matrices
- `Transform` Mat4 extensions

**bif_renderer (19 tests):**

- Material scattering (Lambertian, Metal, Dielectric)
- BVH construction and hit testing
- Sphere/Triangle hit testing
- InstancedGeometry correctness

**bif_core (15 tests):**

- USD mesh parsing
- Point instancer parsing
- Triangulation (quads, N-gons)

**Pattern:**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transform_point() {
        let transform = Mat4::from_translation(Vec3::new(1.0, 2.0, 3.0));
        let point = Vec3::ZERO;
        let result = transform.transform_point3(point);
        assert_eq!(result, Vec3::new(1.0, 2.0, 3.0));
    }
}
```

### Integration Tests

- Load [assets/lucy_low.usda](assets/lucy_low.usda) and verify vertex count (140,278)
- Render 100 instances and measure FPS (60+)
- Switch Vulkan ↔ Ivar and verify no freeze (0ms)
- Compare output with reference renders

---

## Code Organization Principles

1. **Crate Separation:** Math → Core → Viewport/Renderer → Viewer
   - `bif_math`: No dependencies, pure math
   - `bif_core`: Depends on `bif_math`, scene graph + USD
   - `bif_viewport`, `bif_renderer`: Depend on `bif_core`, `bif_math`
   - `bif_viewer`: Application entry, depends on all

2. **Shared Ownership:** Use `Arc<T>` for geometry, materials
   - Cheap clones across thread boundaries
   - Example: `Arc<Mesh>` shared by 100 instances

3. **Background Work:** Move expensive operations off main thread
   - BVH builds in `std::thread::spawn`
   - Ivar rendering in rayon thread pool
   - Use `mpsc::channel()` for completion notification

4. **Progressive Display:** Update UI during long operations
   - Bucket rendering with incremental compositing
   - Status enums (NotStarted → Building → Complete)

5. **Instance-Aware:** Build BVH once, transform rays per-instance
   - 100x memory savings for 100 instances
   - Trade-off: ~3x slower rendering (acceptable for 100, not 10K+)

---

## Common Pitfalls

### 1. Forgetting to Mark Instances as Modified

**Problem:** Adding transforms but GPU buffer not updated

**Solution:**

```rust
self.instance_transforms.push(transform);
self.needs_instance_buffer_update = true;  // DON'T FORGET!
```

### 2. egui Borrow Checker Issues

**Problem:** Cannot borrow `self` mutably inside `egui_ctx.run()` closure

**Solution:** Extract data BEFORE closure

```rust
// BAD
egui_ctx.run(input, |ctx| {
    ui.label(format!("FPS: {}", self.fps));  // Borrow error!
});

// GOOD
let fps = self.fps;
egui_ctx.run(input, |ctx| {
    ui.label(format!("FPS: {fps}"));
});
```

### 3. Mat4 Column-Major vs Row-Major

**Problem:** glam uses column-major, but GPU expects specific format

**Solution:** Always use `.to_cols_array_2d()` for GPU upload

```rust
// Correct
let matrix_data = transform.to_cols_array_2d();  // [[f32; 4]; 4]
```

### 4. USD USDA Parsing Edge Cases

**Challenges:**

- Missing normals → compute from face geometry
- N-gon faces (5+ vertices) → fan triangulation
- Left-handed orientation → swap triangle indices i1/i2

**Solution:** See [bif_core/src/usd/parser.rs](crates/bif_core/src/usd/parser.rs)

### 5. wgpu Surface Loss on Resize

**Problem:** Window resize invalidates surface

**Solution:** Recreate surface configuration

```rust
if self.config.width != new_width || self.config.height != new_height {
    self.config.width = new_width;
    self.config.height = new_height;
    self.surface.configure(&self.device, &self.config);
}
```

---

## Dependencies (Actual)

```toml
[workspace.dependencies]
# Math
glam = "0.29"              # SIMD-optimized vector math

# GPU
wgpu = "22.1"              # Vulkan/DX12/Metal abstraction
winit = "0.30"             # Window management
bytemuck = "1.24"          # Zero-copy GPU buffer casting
pollster = "0.3"           # Async executor for wgpu init

# UI
egui = "0.29"              # Immediate-mode UI
egui-wgpu = "0.29"         # egui + wgpu integration
egui-winit = "0.29"        # egui + winit integration

# I/O
tobj = "4.0"               # OBJ file parser (legacy)
image = "0.24"             # PNG/JPG loading (Rust 1.86 compatible)

# Parallelism
rayon = "1.10"             # Data parallelism

# Utilities
anyhow = "1.0"             # Error handling
```

---

## File Structure (Actual)

```
bif/
├── Cargo.toml              # Rust workspace
├── crates/
│   ├── bif_math/           # Vec3, Ray, Interval, Aabb, Camera, Transform
│   │   ├── src/
│   │   │   ├── lib.rs      # Re-exports
│   │   │   ├── vec3.rs     # (empty, uses glam directly)
│   │   │   ├── ray.rs      # Ray struct
│   │   │   ├── interval.rs # Interval struct
│   │   │   ├── aabb.rs     # AABB struct
│   │   │   ├── camera.rs   # Camera with view/proj matrices
│   │   │   └── transform.rs # Mat4Ext trait (NEW in Freeze Fix)
│   ├── bif_core/           # Scene graph, USD parser, mesh data
│   │   ├── src/
│   │   │   ├── mesh.rs     # Mesh struct
│   │   │   ├── scene.rs    # Scene struct (minimal)
│   │   │   └── usd/        # USD parser
│   │   │       ├── mod.rs
│   │   │       ├── parser.rs  # USDA text parser
│   │   │       └── types.rs   # UsdMesh, UsdPointInstancer
│   ├── bif_viewport/       # GPU viewport (wgpu + Vulkan + egui)
│   │   ├── src/
│   │   │   ├── lib.rs      # Renderer struct (~2000 LOC)
│   │   │   └── shaders/
│   │   │       └── basic.wgsl  # Vertex + fragment shaders
│   ├── bif_renderer/       # CPU path tracer "Ivar"
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── hittable.rs    # Hittable trait
│   │   │   ├── material.rs    # Material trait
│   │   │   ├── materials/     # Lambertian, Metal, Dielectric, DiffuseLight
│   │   │   ├── sphere.rs      # Sphere primitive
│   │   │   ├── triangle.rs    # Triangle primitive (Möller-Trumbore)
│   │   │   ├── bvh.rs         # BVH acceleration
│   │   │   ├── instanced_geometry.rs  # Instance-aware BVH (NEW in Freeze Fix)
│   │   │   ├── camera.rs      # Ivar camera (separate from viewport camera)
│   │   │   └── renderer.rs    # Progressive rendering
│   └── bif_viewer/         # Application entry point
│       └── src/
│           └── main.rs     # winit event loop
├── legacy/
│   └── go-raytracing/      # Original Go raytracer (reference)
├── devlog/                 # Development session logs
│   ├── DEVLOG_2025-12-27_milestone1.md
│   ├── ...
│   └── DEVLOG_2025-12-31_freeze-fix.md
├── docs/archive/           # Archived documentation
│   └── GO_API_REFERENCE.md
├── renders/                # Render output files
│   ├── output.png
│   └── output.ppm
└── assets/                 # Test scenes, meshes
    └── lucy_low.usda
```

---

## Next Steps

For Milestone 12 (Embree Integration) and beyond, see:

- [MILESTONES.md](MILESTONES.md) - Complete roadmap
- [SESSION_HANDOFF.md](SESSION_HANDOFF.md) - Current status
- [ARCHITECTURE.md](ARCHITECTURE.md) - System design

---

**Last Updated:** December 31, 2025 (Milestones 0-11 Complete)
