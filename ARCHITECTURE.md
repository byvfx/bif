# BIF Architecture

**Version:** 0.1.0  
**Last Updated:** 2024-12-26

## Vision

BIF is a production scene assembler and renderer for VFX, inspired by Isotropix Clarisse.

**Core Focus:**

- **Massive scalability** - 10K to 1M instances via prototype/instance architecture
- **USD-compatible** - Load, author, and export USD with MaterialX materials
- **Dual rendering** - GPU viewport (interactive) + CPU path tracer (production)
- **Non-destructive workflow** - Layer-based overrides, no destructive edits

**Target Pipeline:**

```
Houdini/Maya (USD) → BIF (scatter/instance/render) → USD → Nuke/Comp
```

## Core Principles

### 1. Prototype/Instance Everything

Every object is either a **prototype** (shared geometry/material) or an **instance** (transform + overrides).

```rust
struct Prototype {
    id: usize,
    mesh: Arc<Mesh>,           // Shared geometry
    material: Arc<Material>,   // Default material
    bounds: AABB,
}

struct Instance {
    prototype_id: usize,       // Reference to prototype
    transform: Mat4,           // Unique transform
    layer_overrides: Vec<Override>,
}
```

**Memory efficiency:**

- 10MB mesh × 100K instances = 10MB + (100K × 64 bytes) ≈ 16MB
- Without instancing = 1TB (impossible)

### 2. USD-Compatible Scene Graph

BIF's scene graph maps cleanly to USD but doesn't use USD C++ internally initially.

**Rust → USD Mapping:**

| BIF Type | USD Equivalent | Purpose |
|----------|---------------|---------|
| `Scene` | `UsdStage` | Root container |
| `Prototype` | `UsdGeomMesh` | Shared geometry |
| `Instance` | `UsdGeomPointInstancer` | Instance transforms |
| `Layer` | `SubLayer` | Non-destructive overrides |

**Phased Implementation:**

1. Phase 1: Pure Rust scene graph (USD-compatible design)
2. Phase 2: USD export (write .usda text files)
3. Phase 3: USD C++ import (load .usda files)

### 3. Dual Rendering Architecture

**GPU Viewport (wgpu):**

- Real-time preview (60+ FPS)
- Instanced rendering (10K+ instances)
- Basic PBR shading
- Interactive scene assembly

**CPU Path Tracer:**

- Production quality renders
- Physically-based lighting
- Embree BVH acceleration
- IBL, materials, Next Event Estimation

```
         Scene Graph (Rust)
              │
    ┌─────────┴──────────┐
    │                    │
GPU Viewport      CPU Path Tracer
  (wgpu)              (Embree)
    │                    │
  Window            Image File
  60 FPS            .exr/.png
```

### 4. Embree for BVH Acceleration

**Decision:** Use Intel Embree instead of custom BVH.

**Rationale:**

- Production-proven (Arnold, Blender Cycles, etc.)
- SIMD optimized (4-8x faster than scalar)
- Built-in motion blur support
- Instance-aware traversal

```rust
// Embree handles BVH construction
let scene = embree::Scene::new(device);

// Add prototype once
let geom_id = scene.add_triangle_mesh(&prototype.vertices, &prototype.indices);

// Instance 10,000 times
for instance in instances {
    scene.add_instance(geom_id, &instance.transform);
}

scene.commit();  // Embree builds optimized BVH
```

### 5. egui for PoC, Qt 6 for Production

**PoC:** egui + wgpu (pure Rust)  
**Production:** Qt 6 via cxx-qt (optional)

**Rationale:**

| Phase | Framework | Why |
|-------|-----------|-----|
| **PoC** | egui | Fast iteration, pure Rust, validate workflow |
| **Production** | Qt 6 | Professional features (if egui insufficient) |

**PoC Phase (Current):**

- egui immediate-mode UI
- Embedded wgpu viewport
- Scene hierarchy, properties, render settings
- Fast iteration, single language
- Validate architecture before committing to Qt complexity

**Production Phase (Optional):**

- Migrate to Qt only if egui hits limitations
- Industry-standard docking/menus/shortcuts
- Worth FFI complexity for large productions

**Decision:** Start simple (egui), upgrade only if needed.

## Scene Graph Design

### Core Types

```rust
pub struct Scene {
    prototypes: Vec<Arc<Prototype>>,
    instances: Vec<Instance>,
    layers: Vec<Layer>,
}

pub struct Mesh {
    vertices: Vec<Vec3>,
    normals: Vec<Vec3>,
    uvs: Vec<Vec2>,
    indices: Vec<u32>,
}

pub struct Layer {
    name: String,
    enabled: bool,
    overrides: HashMap<u32, Override>,  // instance_id → override
}

pub enum Override {
    Transform(Mat4),
    Visibility(bool),
    Material(Arc<Material>),
}
```

### Layer System (Non-Destructive Edits)

Layers allow temporary changes without modifying base instances:

```rust
// Base: 1000 trees
for i in 0..1000 {
    scene.add_instance(tree_prototype, transform);
}

// Layer 1: Hide near camera
let layer = scene.create_layer("hide_near_camera");
for instance in near_instances {
    layer.add_override(instance.id, Override::Visibility(false));
}

// Layer 2: LOD for distant
let layer2 = scene.create_layer("LOD_distant");
for instance in distant_instances {
    layer2.add_override(instance.id, Override::Prototype(low_poly_tree));
}

// Toggle without rebuilding
scene.set_layer_enabled("hide_near_camera", false);
```

## Material System & USD Integration

### Three-Layer Material Architecture

```
Layer 1: BIF Internal Materials (Rust Traits)
         CPU path tracer production rendering
         
Layer 2: wgpu Viewport Shaders (WGSL)
         GPU real-time preview approximation
         
Layer 3: USD/MaterialX (Interchange)
         Import/export to DCCs
```

### Layer 1: BIF Internal Materials

```rust
pub trait Material: Send + Sync {
    fn scatter(&self, ray: &Ray, hit: &HitRecord) -> Option<(Color, Ray)>;
    fn emitted(&self, u: f32, v: f32, p: Vec3) -> Color;
}

// Core materials
pub struct Lambertian { albedo: Arc<dyn Texture> }
pub struct Metal { albedo: Color, fuzz: f32 }
pub struct Dielectric { ior: f32 }
pub struct Emissive { emit: Arc<dyn Texture> }
```

### Layer 2: Viewport Shaders

```wgsl
struct Material {
    base_color: vec3<f32>,
    roughness: f32,
    metallic: f32,
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Basic PBR for 60 FPS viewport
    let diffuse = max(dot(N, L), 0.0);
    let specular = pow(max(dot(R, V), 0.0), 32.0);
    return vec4<f32>(material.base_color * diffuse + specular, 1.0);
}
```

### Layer 3: USD/MaterialX Integration

**Phased approach:**

#### Phase 1: No USD Materials (Months 1-6)

- Export geometry only
- Materials stay in BIF

#### Phase 2: UsdPreviewSurface (Month 7-8)

- Import/export basic USD materials
- Simple 1:1 mapping to BIF materials

```rust
fn import_usd_preview_surface(shader: &UsdShader) -> Box<dyn Material> {
    let base_color = shader.GetInput("diffuseColor").Get();
    let metallic = shader.GetInput("metallic").Get();
    let roughness = shader.GetInput("roughness").Get();
    
    if metallic > 0.5 {
        Box::new(Metal::new(base_color, roughness))
    } else {
        Box::new(Lambertian::new(base_color))
    }
}
```

#### Phase 3: MaterialX Import (Month 9-10)

- Pattern matching for common MaterialX graphs
- Handle 80% of cases, fallback for complex

```rust
enum MaterialXPattern {
    ConstantPbr { base_color, metalness, roughness },
    TexturedPbr { base_color_tex, metal_rough_tex },
    Unsupported,
}

fn classify_materialx(mtlx: &MaterialX) -> MaterialXPattern {
    // Detect common patterns
    // Fallback to Unsupported for exotic materials
}
```

#### Phase 4: MaterialX Export (Month 11-12)

- Export BIF materials to MaterialX
- Full bidirectional workflow

### USD C++ FFI Bridge

Required for production USD import/export:

```
┌─────────────────────┐
│   BIF (Rust)        │
└──────────┬──────────┘
           │ FFI
┌──────────▼──────────┐
│  C++ USD Bridge     │
│  ┌──────────────┐   │
│  │ USD Library  │   │
│  │ MaterialX    │   │
│  └──────────────┘   │
└─────────────────────┘
```

**C++ Shim:**

```cpp
extern "C" {
    void* usd_open_stage(const char* path);
    int usd_get_mesh_vertices(void* stage, const char* prim_path, 
                               float* out_vertices, int max_count);
    void usd_close_stage(void* stage);
}
```

**Rust Wrapper:**

```rust
pub struct UsdStage {
    ptr: *mut c_void,
}

impl UsdStage {
    pub fn open(path: &str) -> Result<Self>;
    pub fn load_mesh(&self, prim_path: &str) -> Result<Mesh>;
}
```

## Rendering Architecture

### GPU Viewport (wgpu)

**Purpose:** Interactive preview for scene assembly

**Pipeline:**

- Vertex shader: Transform vertices with instance matrices
- Fragment shader: Basic PBR shading
- Instanced rendering: 1 draw call for all instances

**Performance Target:**

- 10K instances @ 60 FPS
- 100K instances @ 30 FPS

### CPU Path Tracer

**Purpose:** Production-quality renders

**Features:**

- Embree BVH for ray intersection
- Path tracing with multiple importance sampling
- IBL with cosine-weighted sampling
- Next Event Estimation for direct lighting
- Progressive refinement

**Ray Tracing Loop:**

```rust
fn trace_ray(ray: Ray, scene: &Scene, depth: u32) -> Color {
    if depth == 0 { return Color::BLACK; }
    
    // Embree intersection
    let hit = scene.embree_scene.intersect(ray)?;
    
    let instance = &scene.instances[hit.instance_id];
    let prototype = &scene.prototypes[instance.prototype_id];
    
    // Material response
    let (attenuation, scattered) = prototype.material.scatter(ray, hit)?;
    
    // Direct lighting (NEE)
    let direct = sample_lights(hit, scene);
    
    // Indirect lighting (recursive)
    let indirect = attenuation * trace_ray(scattered, scene, depth - 1);
    
    direct + indirect
}
```

## Development Roadmap

### Phase 1: Core Foundation (Months 1-6)

- Math library and scene graph
- wgpu viewport with instanced rendering
- CPU path tracer with Embree
- egui UI for workflow validation

### Phase 2: Production Ready (Months 7-8)

- USD export (geometry + materials)
- Scene assembly workflow complete
- Decide: egui sufficient or migrate to Qt?

### Phase 3: Optional Qt Migration (Months 9-12)

- Qt 6 integration (if egui insufficient)
- Professional UI polish
- Industry-standard features

### Phase 4: Advanced Features (Future)

- GPU path tracing (wgpu compute)
- Denoising (OIDN)
- USD C++ import (full bidirectional)
- MaterialX integration
- Python scripting API

## File Structure

```
bif/
├── Cargo.toml              # Rust workspace
├── docs/
│   ├── ARCHITECTURE.md     # This file
│   ├── GETTING_STARTED.md  # Implementation guide
│   └── claude.md           # AI assistant instructions
├── crates/
│   ├── bif_math/           # Vec3, Ray, AABB, transforms
│   ├── bif_scene/          # Scene graph, instances, layers
│   ├── bif_renderer/       # CPU path tracer
│   ├── bif_viewport/       # GPU viewport (wgpu)
│   ├── bif_materials/      # Material system
│   ├── bif_io/             # glTF, image loading
│   └── bif_app/            # Main application
├── cpp/
│   └── usd_bridge/         # USD C++ FFI (later)
└── assets/                 # Test scenes, HDRIs
```

## Design Decisions

### 1. Rust Over Go

- GPU capabilities via wgpu (essential)
- Better C++ FFI for USD/Embree
- Zero-cost abstractions, no GC pauses

### 2. Embree Over Custom BVH

- Production-proven, SIMD optimized
- Instance-aware traversal
- Less code to maintain

### 3. USD-Compatible Over USD-Native

- Start simple, validate with USD export
- Defer USD C++ until proven necessary
- Can always add later

### 4. Dual Rendering (GPU + CPU)

- GPU: Interactive assembly (60 FPS)
- CPU: Production quality
- Best of both worlds

### 5. Qt 6 Over egui

- Industry standard UI framework
- Professional docking/MDI
- Worth complexity for long-term

## Non-Goals

**BIF is NOT:**

- Blender (no modeling/sculpting)
- Houdini (no procedural SOPs initially)
- Maya (no rigging/character animation)
- USD editor (USD is interchange, not internal format)

---

**Document Status:** Living document - update as design evolves
