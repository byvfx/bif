# BIF Code Reference

> Patterns and techniques from Milestones 0-18.5

**Last Updated:** February 1, 2026

---

## Table of Contents

1. [GPU Instancing](#1-gpu-instancing)
2. [C++ FFI Bridge](#2-c-ffi-bridge)
3. [Multi-Prototype Rendering](#3-multi-prototype-rendering)
4. [Animation System](#4-animation-system)
5. [Background Threading](#5-background-threading)
6. [Batch Rendering](#6-batch-rendering)
7. [Material Pipeline](#7-material-pipeline)
8. [Texture Loading](#8-texture-loading)
9. [Performance Profiling](#9-performance-profiling)
10. [Common Pitfalls](#10-common-pitfalls)

---

## 1. GPU Instancing

**Problem:** Render 10K+ instances without duplicating geometry

**Solution:** Per-instance transform buffer + instanced draw call

```rust
// crates/bif_viewport/src/lib.rs
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct InstanceData {
    pub model_matrix: [[f32; 4]; 4],
    pub material_id: u32,
}

// Single draw call for all instances
render_pass.draw_indexed(0..self.num_indices, 0, 0..visible_count);
```

**Key Files:**
- `crates/bif_viewport/src/lib.rs` - GPU instancing
- `crates/bif_viewport/src/shaders/basic.wgsl` - Per-instance transforms

**Performance:** 10K instances @ 60+ FPS with LOD culling

---

## 2. C++ FFI Bridge

**Problem:** USD requires C++ library (no pure Rust binding)

**Solution:** Extern "C" wrapper with CMake integration

```cpp
// cpp/usd_bridge/usd_bridge.cpp
extern "C" {
    UsdBridgeError usd_bridge_open_stage(const char* path, UsdBridgeStage** out);
    UsdBridgeError usd_bridge_get_mesh(const UsdBridgeStage* stage, size_t idx, UsdBridgeMeshData* out);
}
```

```rust
// crates/bif_core/src/usd/cpp_bridge.rs
extern "C" {
    fn usd_bridge_open_stage(path: *const c_char, out: *mut *mut UsdBridgeStage) -> i32;
}

pub struct UsdStage {
    ptr: *mut UsdBridgeStage,
}

// SAFETY: All data is pre-cached at load time, getters are read-only
unsafe impl Send for UsdStage {}
```

**Key Files:**
- `cpp/usd_bridge/` - C++ FFI bridge
- `crates/bif_core/src/usd/cpp_bridge.rs` - Rust wrapper
- `crates/bif_core/build.rs` - CMake automation

**Pattern:** Pre-cache all data at load time for thread safety

---

## 3. Multi-Prototype Rendering

**Problem:** Multiple mesh types with different geometry need efficient rendering

**Solution:** Per-prototype GPU buffers + instance grouping

```rust
// Multi-draw architecture
struct PrototypeGpuData {
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    num_indices: u32,
    triangle_material_buffer: Option<wgpu::Buffer>,
}

// Group instances by prototype
let instance_groups: HashMap<usize, Vec<InstanceData>> = ...;

// Render each group
for (proto_id, instances) in &instance_groups {
    let proto = &self.prototype_gpu_data[proto_id];
    render_pass.set_vertex_buffer(0, proto.vertex_buffer.slice(..));
    render_pass.draw_indexed(0..proto.num_indices, 0, 0..instances.len());
}
```

**For Ivar (ray tracing):** Combined mesh with baked transforms

```rust
// When multi-prototype, combine meshes for Ivar
let mesh_data = if scene.prototypes.len() > 1 {
    MeshData::combine_with_transforms(&meshes_with_transforms)
} else {
    MeshData::from_core_mesh(&scene.prototypes[0].mesh)
};

// Use identity transform for Embree (transforms already baked)
let ivar_transforms = if self.use_multi_draw {
    vec![Mat4::IDENTITY]
} else {
    self.instance_transforms.clone()
};
```

**Key Files:**
- `crates/bif_viewport/src/lib.rs` - Multi-draw rendering
- `crates/bif_viewport/src/mesh_data.rs` - `combine_with_transforms()`

---

## 4. Animation System

**Problem:** Animate transforms and vertices over time

**Solution:** Keyframe storage + interpolation

```rust
// crates/bif_core/src/scene.rs
pub struct AnimatedTransform {
    base: Transform,
    keyframes: Vec<TransformKeyframe>,
}

pub struct TransformKeyframe {
    pub time: f64,
    pub transform: Transform,
}

impl AnimatedTransform {
    pub fn evaluate(&self, time: f64) -> Transform {
        // Binary search + lerp between keyframes
        ...
    }
}
```

**Multi-mesh vertex animation:**

```rust
// crates/bif_viewport/src/mesh_data.rs
pub struct MeshRange {
    pub usd_mesh_index: usize,
    pub vertex_offset: usize,
    pub vertex_count: usize,
}

// Update correct vertex range per mesh
for range in &self.mesh_data.mesh_ranges {
    let vertices = stage.get_mesh_vertices_at_time(range.usd_mesh_index, time)?;
    // Update buffer at range.vertex_offset
}
```

**Key Files:**
- `crates/bif_core/src/scene.rs` - AnimatedTransform
- `crates/bif_viewport/src/mesh_data.rs` - MeshRange for multi-mesh
- `cpp/usd_bridge/usd_bridge.cpp` - Time-sampled vertex queries

---

## 5. Background Threading

**Problem:** BVH builds and scene loading freeze UI

**Solution:** Background thread + channel communication

```rust
// Spawn background thread
let (tx, rx) = mpsc::channel();
std::thread::spawn(move || {
    let scene = build_embree_scene(...);
    tx.send(scene).unwrap();
});

// Poll without blocking in render loop
if let Ok(scene) = self.build_receiver.try_recv() {
    self.ivar_scene = Some(scene);
    self.build_status = BuildStatus::Complete;
}
```

**Thread-safe USD stage:**

```rust
// Pre-cache all data at load time
fn usd_bridge_open_stage(...) {
    cache_stage_data(bridge);      // Meshes, materials
    cache_prim_data(bridge);       // Hierarchy
    cache_animation_data(bridge);  // Keyframes
}

// Getters are read-only, no mutation
fn usd_bridge_get_mesh(...) {
    // Just read from cache
    *out_data = stage->meshes[index];
}
```

**Key Files:**
- `crates/bif_viewport/src/lib.rs` - Background scene building
- `cpp/usd_bridge/usd_bridge.cpp` - Pre-caching pattern

---

## 6. Batch Rendering

**Problem:** Render frame sequences to disk

**Solution:** Frame loop with EXR output

```rust
// crates/bif_viewport/src/batch_render.rs
pub fn render_frame_sequence(
    settings: &BatchRenderSettings,
    world: &Arc<BvhNode>,
    camera_fn: impl Fn(f64) -> IvarCamera,
    progress_tx: Sender<BatchRenderProgress>,
) {
    for frame in (settings.start_frame..=settings.end_frame).step_by(settings.frame_step) {
        let camera = camera_fn(frame as f64);
        let pixels = render_frame(&camera, world, settings);

        let path = format_frame_path(&settings.output_path, frame);
        write_exr(&path, &pixels, settings.compression)?;

        progress_tx.send(BatchRenderProgress { current_frame: frame, ... })?;
    }
}
```

**USD camera evaluation:**

```rust
// Get camera transform at specific time
let transform = stage.get_camera_xform_at_time(&camera_path, frame as f64)?;
let camera = IvarCamera::from_usd_transform(transform, fov, aspect);
```

**Key Files:**
- `crates/bif_viewport/src/batch_render.rs` - Batch render loop
- `crates/bif_renderer/src/exr_writer.rs` - EXR output with AOVs
- `cpp/usd_bridge/usd_bridge.cpp` - `usd_bridge_get_camera_xform_at_time()`

---

## 7. Material Pipeline

**Problem:** Load UsdPreviewSurface + MaterialX materials

**Solution:** C++ extraction → Rust structs → Disney BSDF

```cpp
// cpp/usd_bridge/usd_bridge.cpp
struct CachedMaterial {
    float diffuse_color[3];
    float metallic, roughness, specular, opacity;
    std::string diffuse_texture;
    bool is_materialx;
};

// Detect MaterialX vs UsdPreviewSurface
if (is_materialx_standard_surface(shader_id)) {
    // Use base_color, metalness, specular_roughness
} else {
    // Use diffuseColor, metallic, roughness
}
```

```rust
// crates/bif_renderer/src/disney.rs
impl From<&bif_core::Material> for DisneyBSDF {
    fn from(mat: &Material) -> Self {
        DisneyBSDF {
            base_color: Vec3::from(mat.diffuse_color),
            metallic: mat.metallic,
            roughness: mat.roughness,
            ...
        }
    }
}
```

**Key Files:**
- `cpp/usd_bridge/usd_bridge.cpp` - Material extraction
- `crates/bif_core/src/scene.rs` - Material struct
- `crates/bif_renderer/src/disney.rs` - Disney BSDF

---

## 8. Texture Loading

**Problem:** Load textures efficiently with proper color space

**Solution:** Parallel loading + sRGB conversion

```rust
// crates/bif_viewport/src/texture_loader.rs
pub fn create_gpu_textures_for_scene(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    scene: &Scene,
    base_dir: Option<&Path>,
) -> GpuTextures {
    // Parallel texture loading with rayon
    let textures: Vec<_> = paths.par_iter()
        .map(|path| load_texture(path, base_dir))
        .collect();

    // Upload to GPU with mipmaps
    for tex in textures {
        let gpu_tex = device.create_texture(...);
        queue.write_texture(...);
    }
}
```

**sRGB to linear conversion:**

```rust
fn srgb_to_linear(srgb: u8) -> f32 {
    let s = srgb as f32 / 255.0;
    if s <= 0.04045 {
        s / 12.92
    } else {
        ((s + 0.055) / 1.055).powf(2.4)
    }
}
```

**Key Files:**
- `crates/bif_viewport/src/texture_loader.rs` - GPU texture upload
- `crates/bif_core/src/texture.rs` - TextureCache with OIIO support

---

## 9. Performance Profiling

**Problem:** Identify USD loading bottlenecks

**Solution:** Timing instrumentation in C++ bridge + Rust loader

```cpp
// cpp/usd_bridge/usd_bridge.cpp
using namespace std::chrono;
auto start = high_resolution_clock::now();
// ... operation ...
auto time_ms = duration_cast<milliseconds>(high_resolution_clock::now() - start).count();
std::cout << "[USD_BRIDGE] Operation: " << time_ms << "ms" << std::endl;
```

**Output breakdown:**
```
[USD_BRIDGE] Opening stage: scene.usd
[USD_BRIDGE]   Resolver context: 0ms
[USD_BRIDGE]   UsdStage::Open(): 127ms
[USD_BRIDGE]   cache_stage_data(): 333ms (N meshes, M instancers)
[USD_BRIDGE]     Materials:    8ms
[USD_BRIDGE]     Vertices:     170ms (219764 verts)
[USD_BRIDGE]     Triangulate:  6ms (345262 tris)
[USD_BRIDGE]     GeomSubsets:  2ms
[USD_BRIDGE]     Normals:      13ms
[USD_BRIDGE]     UVs:          128ms
[USD_BRIDGE]     Transforms:   0ms
```

**Key lesson:** Console I/O is extremely slow on Windows. Per-item logging caused 23x slowdown (117s → 5s after removal).

**Key Files:**
- `cpp/usd_bridge/usd_bridge.cpp` - C++ timing with `<chrono>`
- `crates/bif_core/src/usd/loader.rs` - Rust timing with `std::time::Instant`

---

## 10. Common Pitfalls

### Mat4 Row vs Column Major

**Problem:** USD uses row-major, glam uses column-major

```rust
// USD: translation in row 3 (indices 12,13,14)
// glam: translation in col 3 (w_axis)

// WRONG: Reading USD matrix as column-major
let pos = mat.col(3).truncate();

// CORRECT: USD matrix layout
let pos = Vec3::new(mat[12], mat[13], mat[14]); // Row 3
// Or convert properly in C++
```

### Combined Mesh + Embree Transforms

**Problem:** Double transform when mesh has baked transforms

```rust
// BAD: Baked transforms + Embree transforms
let mesh_data = combine_with_transforms(...); // Transforms baked
EmbreeScene::new(&triangles, instance_transforms, ...); // Applied again!

// GOOD: Identity for Embree when using combined mesh
let ivar_transforms = if use_multi_draw {
    vec![Mat4::IDENTITY]
} else {
    instance_transforms
};
```

### egui Borrow Checker

**Problem:** Cannot borrow `self` mutably inside closure

```rust
// BAD
egui_ctx.run(input, |ctx| {
    ui.label(format!("{}", self.fps)); // Borrow error!
});

// GOOD: Extract before closure
let fps = self.fps;
egui_ctx.run(input, |ctx| {
    ui.label(format!("{fps}"));
});
```

### USD Left-Handed Orientation

**Problem:** Houdini exports use left-handed winding

```rust
// Detect and swap indices
if mesh.left_handed {
    triangles.push([i0, i2, i1]); // Swap i1/i2
} else {
    triangles.push([i0, i1, i2]);
}
```

### UNC Network Paths

**Problem:** `canonicalize()` returns `\\?\UNC\...` format

```rust
// Convert back to standard UNC
fn normalize_unc_path(path: &Path) -> PathBuf {
    let s = path.to_string_lossy();
    if s.starts_with("\\\\?\\UNC\\") {
        PathBuf::from(format!("\\\\{}", &s[8..]))
    } else if s.starts_with("\\\\?\\") {
        PathBuf::from(&s[4..])
    } else {
        path.to_path_buf()
    }
}
```

---

## Dependencies

```toml
[workspace.dependencies]
# Math
glam = "0.29"

# GPU
wgpu = "22.1"
winit = "0.30"
bytemuck = "1.24"

# UI
egui = "0.29"
egui-wgpu = "0.29"
egui-snarl = "0.5"

# Ray Tracing
embree = "4.4.0"  # via vcpkg

# Rendering
image = "0.24"
rayon = "1.10"

# USD (C++ bridge)
pxr = "25.11"  # via vcpkg
```

---

## File Structure

```
bif/
├── crates/
│   ├── bif_math/       # Vec3, Ray, Aabb, Camera, Transform, Frustum
│   ├── bif_core/       # Scene, Mesh, Material, USD parser, Texture
│   ├── bif_viewport/   # GPU viewport, egui UI, node graph
│   ├── bif_renderer/   # Ivar CPU path tracer, Embree, Disney BSDF
│   ├── bif_viewer/     # Application entry point
│   └── bif_maketx/     # Standalone .tx converter
├── cpp/
│   ├── usd_bridge/     # C++ FFI to Pixar USD
│   └── oiio_bridge/    # C++ FFI to OpenImageIO (optional)
├── devlog/             # Session logs
└── assets/             # Test scenes
```

---

**See Also:**
- [MILESTONES.md](MILESTONES.md) - Complete history
- [SESSION_HANDOFF.md](SESSION_HANDOFF.md) - Current status
- [ARCHITECTURE.md](ARCHITECTURE.md) - Design principles
