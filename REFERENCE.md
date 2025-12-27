# BIF Reference & Task Checklist

Quick reference for implementation tasks, code patterns, and performance notes.

See [GETTING_STARTED.md](GETTING_STARTED.md) for milestone-based progression.

## Crate Dependencies

```toml
[workspace.dependencies]
# Math
glam = "0.29"              # SIMD-optimized vector math

# Parallelism
rayon = "1.10"             # Data parallelism

# Image I/O
image = "0.25"             # PNG/JPG/EXR

# 3D Loading
gltf = "1.4"               # glTF 2.0 loader

# Random
rand = "0.8"

# UI (egui PoC)
egui = "0.29"
eframe = "0.29"            # egui + wgpu integration
egui_wgpu = "0.29"

# GPU
wgpu = "23.0"
winit = "0.30"
pollster = "0.3"           # Async executor

# Utilities
anyhow = "1.0"
thiserror = "2.0"
```

## Task Checklist by Subsystem

### Core Math (bif_math crate)

- [ ] `Vec3` using `glam::Vec3A` (SIMD-aligned)
- [ ] Type aliases: `Point3 = Vec3`, `Color = Vec3`
- [ ] Helpers: `dot()`, `cross()`, `reflect()`, `refract()`
- [ ] `Ray` struct (origin, direction, time)
- [ ] `Ray::at(t)` method
- [ ] `Interval` struct (min, max)
- [ ] `Interval` methods: `contains()`, `clamp()`, `expand()`
- [ ] `AABB` struct (x, y, z intervals)
- [ ] `AABB::hit()` ray-box intersection
- [ ] `AABB::longest_axis()`
- [ ] AABB combining

### Traits (bif_renderer crate)

```rust
// Hittable trait
pub trait Hittable: Send + Sync {
    fn hit(&self, r: &Ray, ray_t: Interval, rec: &mut HitRecord) -> bool;
    fn bounding_box(&self) -> AABB;
}

// Material trait
pub trait Material: Send + Sync {
    fn scatter(&self, r_in: &Ray, rec: &HitRecord) 
        -> Option<(Color, Ray)>;
    fn emitted(&self, u: f32, v: f32, p: Vec3) -> Color {
        Color::ZERO
    }
}

// Texture trait
pub trait Texture: Send + Sync {
    fn value(&self, u: f32, v: f32, p: Vec3) -> Color;
}
```

- [ ] Define `Hittable` trait with `Send + Sync`
- [ ] Define `Material` trait with optional `emitted()`
- [ ] Define `Texture` trait
- [ ] `HitRecord` struct (point, normal, t, u, v, material)

### Primitives (bif_renderer crate)

- [ ] `Sphere` with UV mapping
- [ ] Moving `Sphere` (motion blur)
- [ ] `Plane` (infinite)
- [ ] `Quad` with point sampling for area lights
- [ ] `Triangle` with Möller-Trumbore intersection
- [ ] `Box` compound (6 quads)
- [ ] `Pyramid` compound (base + 4 triangles)
- [ ] `Disk` / `Circle`

**Transform wrappers:**
- [ ] `Translate` with AABB update
- [ ] `RotateX`, `RotateY`, `RotateZ`
- [ ] `Scale` (non-uniform, cached inverse)
- [ ] `Transform` builder (SRT: Scale → Rotate → Translate)

### BVH Acceleration (bif_renderer crate)

```rust
pub enum BVHNode {
    Leaf {
        objects: Vec<Box<dyn Hittable>>,
        bbox: AABB,
    },
    Branch {
        left: Box<BVHNode>,
        right: Box<BVHNode>,
        bbox: AABB,
    },
}
```

- [ ] Recursive construction with longest axis split
- [ ] SAH (Surface Area Heuristic) for production
- [ ] Implement `Hittable` for `BVHNode`
- [ ] Parallel construction with `rayon`
- [ ] Pre-compute centroids for faster sorting
- [ ] Configurable leaf size (4-8 objects)
- [ ] Parallel threshold (8192+ primitives)

**Profiling note:** BVH traversal dominates CPU time (~82% in Go). Optimize AABB slab test.

### Materials (bif_materials crate)

- [ ] `Lambertian` with cosine-weighted hemisphere sampling
- [ ] `Lambertian` texture support
- [ ] `Metal` with perfect reflection
- [ ] `Metal` fuzz/roughness parameter
- [ ] `Dielectric` with Fresnel/Snell's law
- [ ] `Dielectric` total internal reflection
- [ ] `Emissive` emission-only material

**MIS support (Next Event Estimation):**
- [ ] PDF evaluation for each material
- [ ] `MaterialInfo` trait (is_specular, is_emissive, can_use_nee)
- [ ] Light sampling infrastructure
- [ ] Balance heuristic: `w = pdf_light / (pdf_light + pdf_brdf)`
- [ ] Firefly clamping (max component = 20.0)

### Textures (bif_materials crate)

- [ ] `SolidColor`
- [ ] `CheckerTexture` (3D procedural)
- [ ] `ImageTexture` (PNG/JPG loading)
- [ ] `NoiseTexture` (Perlin noise with turbulence)

### Camera (bif_renderer crate)

```rust
pub struct Camera {
    // Position
    position: Vec3,
    target: Vec3,
    up: Vec3,
    
    // Lens
    fov: f32,
    aspect: f32,
    aperture: f32,
    focus_dist: f32,
    
    // Render settings
    samples_per_pixel: u32,
    max_depth: u32,
    
    // Lights (for NEE)
    lights: Vec<Arc<dyn Hittable>>,
}
```

- [ ] Builder pattern for configuration
- [ ] Ray generation with DOF (defocus disk)
- [ ] Motion blur support (camera interpolation)
- [ ] Free camera mode (forward vector)
- [ ] Presets: `QuickPreview`, `StandardQuality`, `HighQuality`
- [ ] Background options (solid, sky gradient)
- [ ] `AddLight()` for NEE light collection

### CPU Renderer (bif_renderer crate)

- [ ] Progressive rendering with sample accumulation
- [ ] `trace_ray()` with recursive depth
- [ ] Next Event Estimation (direct lighting)
- [ ] IBL environment map sampling
- [ ] Parallel rendering with `rayon`
- [ ] Gamma correction (gamma 2.0)
- [ ] Tone mapping

**Profiling note:** `trace_ray()` allocation hotspot in Go. Reuse scratch buffers per thread.

### Scene Graph (bif_scene crate)

```rust
pub struct Scene {
    prototypes: Vec<Arc<Prototype>>,
    instances: Vec<Instance>,
    layers: Vec<Layer>,
}

pub struct Prototype {
    id: usize,
    mesh: Arc<Mesh>,
    material: Arc<Material>,
    bounds: AABB,
}

pub struct Instance {
    id: u32,
    prototype_id: usize,
    transform: Mat4,
    visible: bool,
}

pub struct Layer {
    name: String,
    enabled: bool,
    overrides: HashMap<u32, Override>,
}
```

- [ ] `Scene::add_prototype(mesh) -> usize`
- [ ] `Scene::add_instance(prototype_id, transform)`
- [ ] `Scene::create_layer(name)`
- [ ] Layer override system
- [ ] Instance culling by visibility

### File I/O (bif_io crate)

- [ ] glTF loader with `gltf` crate
- [ ] Extract vertices, normals, UVs, indices
- [ ] Calculate mesh bounds
- [ ] Image loading (PNG/JPG) with `image` crate
- [ ] Image export with gamma correction
- [ ] EXR support for HDR output

### GPU Viewport (bif_viewport crate)

**wgpu setup:**
- [ ] Window creation with `winit`
- [ ] wgpu instance/adapter/device/queue
- [ ] Surface configuration
- [ ] Event loop with resize handling

**Rendering:**
- [ ] Vertex buffer (mesh data)
- [ ] Instance buffer (transforms)
- [ ] Vertex shader (transform vertices)
- [ ] Fragment shader (basic PBR)
- [ ] Instanced draw calls
- [ ] Camera uniform buffer
- [ ] Bind groups

**Performance target:** 10K instances @ 60 FPS

### egui UI (bif_app crate)

- [ ] egui + wgpu integration via `eframe`
- [ ] Main window with central viewport
- [ ] Scene hierarchy panel (tree view)
- [ ] Properties panel (transform, material)
- [ ] Render settings panel
- [ ] Camera controls (orbit, pan, zoom)
- [ ] Progress overlay (samples, time)

### USD Bridge (bif_usd crate, optional)

**Phase 1: Export only**
- [ ] Write .usda text files
- [ ] Export geometry (UsdGeomMesh)
- [ ] Export instances (UsdGeomPointInstancer)
- [ ] Validate in usdview

**Phase 2: Import via C++ FFI**
- [ ] C++ shim (`usd_bridge.cpp`)
- [ ] Rust FFI wrapper
- [ ] Load USD stage
- [ ] Extract meshes
- [ ] Extract materials (UsdPreviewSurface)

## Code Patterns

### SIMD Vector Math

```rust
use glam::Vec3A;

// Always use Vec3A for SIMD
pub type Vec3 = Vec3A;
pub type Point3 = Vec3A;
pub type Color = Vec3A;

// Inline hot paths
#[inline]
pub fn dot(a: Vec3, b: Vec3) -> f32 {
    a.dot(b)
}

#[inline]
pub fn reflect(v: Vec3, n: Vec3) -> Vec3 {
    v - 2.0 * dot(v, n) * n
}
```

### Parallel Iteration with Rayon

```rust
use rayon::prelude::*;

// Parallel pixel rendering
pixels.par_iter_mut().for_each(|(x, y, pixel)| {
    let ray = camera.get_ray(*x, *y);
    *pixel = trace_ray(ray, scene, max_depth);
});

// Parallel BVH construction
if primitives.len() > PARALLEL_THRESHOLD {
    let (left, right) = rayon::join(
        || build_bvh(&left_prims),
        || build_bvh(&right_prims),
    );
}
```

### Material Trait Object

```rust
// Store materials as Arc<dyn Material>
pub struct Prototype {
    material: Arc<dyn Material>,
}

// Scatter ray
if let Some((attenuation, scattered)) = 
    prototype.material.scatter(&ray, &hit) {
    // Continue tracing
}
```

### Instance Rendering (wgpu)

```rust
// Per-instance data
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct InstanceData {
    transform: [[f32; 4]; 4],  // Mat4
}

// Upload instances
let instance_data: Vec<InstanceData> = scene.instances
    .iter()
    .map(|inst| InstanceData {
        transform: inst.transform.to_cols_array_2d(),
    })
    .collect();

let instance_buffer = device.create_buffer_init(&BufferInitDescriptor {
    contents: bytemuck::cast_slice(&instance_data),
    usage: BufferUsages::VERTEX,
});

// Draw all instances in one call
render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
render_pass.set_vertex_buffer(1, instance_buffer.slice(..));
render_pass.draw_indexed(0..index_count, 0, 0..instance_count);
```

## Performance Targets

**Scene Creation:**
- 10K instances: < 500ms
- 100K instances: < 5s

**Rendering:**
- Preview (10 SPP): < 1s first pixels
- Standard (100 SPP): target parity with Go
- High quality (500 SPP): 4-8x speedup with parallelism

**Viewport:**
- 10K instances @ 60 FPS
- 100K instances @ 30 FPS

**Memory:**
- 1M instances: < 60MB overhead (prototype sharing)

## Profiling Notes from Go

**CPU hotspots:**
- BVH traversal: ~82% (AABB slab test dominates)
- Triangle intersection: ~5-10%
- Material evaluation: secondary

**Allocation hotspots:**
- `trace_ray()` recursion allocates per call
- `sample_lights()` per-sample allocations
- Fix: Reuse per-thread scratch buffers

**Parallel efficiency:**
- Go channels add overhead (recv/send/select)
- Rust: Use `rayon` work-stealing for better load balance

**Optimization priorities:**
1. BVH traversal (SIMD AABB test)
2. Reduce allocations (scratch buffers)
3. Better memory layout (SoA for triangles)

## Testing Strategy

**Unit tests:**
- Vector math operations
- Ray-primitive intersection
- Material scattering
- BVH construction

**Integration tests:**
- Render test scenes
- Compare output with Go version
- Validate MIS (check for fireflies)
- Performance benchmarks

**Visual validation:**
- Cornell box
- Glass/metal spheres
- Textured objects
- Area lights with NEE

## Code Organization

```
crates/
├── bif_math/
│   ├── vec3.rs
│   ├── ray.rs
│   ├── interval.rs
│   └── aabb.rs
├── bif_scene/
│   ├── scene.rs
│   ├── prototype.rs
│   ├── instance.rs
│   └── layer.rs
├── bif_renderer/
│   ├── hittable.rs
│   ├── primitives/
│   │   ├── sphere.rs
│   │   ├── triangle.rs
│   │   └── quad.rs
│   ├── bvh.rs
│   ├── camera.rs
│   └── renderer.rs
├── bif_materials/
│   ├── material.rs
│   ├── lambertian.rs
│   ├── metal.rs
│   ├── dielectric.rs
│   └── textures/
├── bif_viewport/
│   ├── renderer.rs
│   └── shaders.wgsl
├── bif_io/
│   ├── gltf.rs
│   └── image.rs
└── bif_app/
    ├── main.rs
    └── ui/
```

---

**Note:** This is a reference checklist. See [GETTING_STARTED.md](GETTING_STARTED.md) for milestone-based implementation order.