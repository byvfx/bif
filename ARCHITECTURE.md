# BIF Architecture

**Version:** 0.3.0
**Last Updated:** March 2, 2026 (Milestones 0-23 Complete, M29 In Progress)

## Vision

BIF is a production scene assembler and renderer for VFX, inspired by Isotropix Clarisse.

**Core Focus:**

- **Massive scalability** - 10K to 1M instances via prototype/instance architecture
- **USD-compatible** - Load, author, and export USD with MaterialX materials
- **Dual rendering** - GPU viewport (interactive) + CPU path tracer (production)
- **Non-destructive workflow** - Layer-based overrides, no destructive edits

**Target Pipeline:**

Houdini/Maya (USD) → BIF (scatter/instance/render) → USD → Nuke/Comp

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

**Implementation Status:**

1. ✅ **Milestones 0-11:** Pure Rust USDA parser (import text files)
2. ✅ **Milestone 13:** USD C++ integration (USDC binary + references)
3. ✅ **Milestone 29 (in-progress):** Full bidirectional USD — import, modify, render, export

### 3. Dual Rendering Architecture

**GPU Viewport (wgpu):**

- Real-time preview (60+ FPS)
- Instanced rendering (10K+ instances)
- Basic PBR shading
- Interactive scene assembly

**CPU Path Tracer ("Ivar"):**

- Production quality renders
- Physically-based lighting
- BVH acceleration (instance-aware in Milestones 0-11, Embree in Milestone 12)
- Materials, progressive rendering

```text
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

### 4. BVH Acceleration Strategy

**Milestones 0-11:** Instance-aware BVH in pure Rust (deprecated)

- ONE prototype BVH (280K triangles)
- 100 transforms stored separately
- Per-instance ray transformation: world→local→test→world

**Milestone 12+ (Current):** Intel Embree 4 integration ✅

- Two-level BVH: O(log instances + log primitives)
- SIMD optimized (4-8x faster than scalar)
- Production-proven (Arnold, Cycles, etc.)
- Build time: 28ms for 100 instances
- Motion blur ready

**Implementation:** See [embree.rs](crates/bif_renderer/src/embree.rs)

```rust
// Milestone 12: Embree handles BVH construction
let scene = embree::Scene::new(device);

// Add prototype once
let geom_id = scene.add_triangle_mesh(&prototype.vertices, &prototype.indices);

// Instance 10,000 times
for instance in instances {
    scene.add_instance(geom_id, &instance.transform);
}

scene.commit();  // Embree builds optimized two-level BVH
```

### 5. egui for PoC, Qt 6 for Production

**PoC:** egui + wgpu (pure Rust) ✅ Current
**Production:** Qt 6 via cxx-qt (Milestone 25+)

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
    pub prototypes: Vec<Arc<Prototype>>,
    instances: Vec<Instance>,               // Private, accessor-guarded
    instance_animations: Vec<Option<AnimatedTransform>>,
    pub materials: Vec<Arc<Material>>,
    pub lights: Vec<Light>,
    pub cameras: Vec<SceneCamera>,
    pub point_clouds: Vec<PointCloud>,
    pub timeline: Option<TimelineInfo>,
    pub stage_metadata: Option<UsdStageMetadata>,
    pub name: String,
}

// Non-destructive edits live outside Scene:
pub struct EditState {
    pub transform_overrides: HashMap<usize, Transform>,
    pub keyframe_overrides: HashMap<usize, AnimatedTransform>,
    // + pending scene ops for undo/redo
}
pub struct UndoStack { commands: Vec<Box<dyn UndoCommand>>, cursor: usize }
```

### Non-Destructive Edits (EditState + Undo)

Transform overrides and keyframes live in `EditState`, separate from the base `Scene`. The `UndoStack` tracks all mutations as `UndoCommand` objects (move, keyframe, delete, etc.). Export merges EditState overrides back onto the USD stage via `export_scene()`.

```rust
// Gizmo move → EditState override (base Scene untouched)
edit_state.transform_overrides.insert(instance_idx, new_transform);
undo_stack.push(MoveCommand { idx, old, new });

// Export applies overrides to USD
export_scene(&scene, &edit_state, &prim_paths, &config)?;
```

## Material System & USD Integration

### Three-Layer Material Architecture

```text
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

## Node Graph Architecture

**Library:** egui-snarl (immediate-mode node graph for egui)

**10 Node Types:**

| Node | Purpose |
|------|---------|
| UsdRead | Load USD file from disk |
| Primitive | Generate procedural geometry (cube, sphere, etc.) |
| ScatterPoints | Scatter points on surface |
| PointInstancer | Instance prototypes at point positions |
| Xform | Transform override |
| UsdExport | Export scene to USD file |
| UsdPrim | Reference a specific prim from loaded stage |
| GraftBranches | Merge multiple scene branches |
| HdriEnvironment | Load HDRI for IBL + background |
| IvarRender | Trigger CPU path trace render |

**Data flow:** Nodes auto-compute on dirty propagation. Node evaluation populates `working_scene` which the viewport and renderer consume.

**Implementation:** `crates/bif_viewport/src/node_graph.rs`

## USD Export Pipeline

**Entry point:** `bif_core::usd::export::export_scene()`

```rust
pub fn export_scene(
    scene: &Scene,
    edit_state: &EditState,
    instance_prim_paths: &[String],
    config: &ExportConfig,
) -> Result<ExportResult, UsdBridgeError>
```

**Write order:** authored prims → xform overrides → keyframes → point instancers → meshes

**Composition:** Export creates a new USD layer. When `use_sublayer_composition` is set, the source USD is added as a sublayer so edits compose over the original non-destructively.

**Implementation:** `crates/bif_core/src/usd/export.rs`

## Scene Browser / CompositeProvider

The scene browser merges the loaded USD hierarchy with BIF-generated procedural prims into a unified tree view.

**Key types:**
- `CachedSceneGraph` — cached tree of USD prim hierarchy, rebuilt on dirty flag
- `ProceduralPrimKind` — enum for BIF-generated prims (ScatterPoints, PointInstancer, Primitive, etc.)
- `CompositeProvider` — merges USD stage tree + procedural prims for the scene browser UI

**Pattern:** Dirty-flag rebuild — scene graph cache invalidated when USD stage changes or nodes recompute.

**Implementation:** `crates/bif_viewport/src/scene_browser.rs`, `crates/bif_viewport/src/scene_loader.rs`

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

### Milestones 0-23: Complete ✅ (Dec 2025 – Feb 2026)

- ✅ M0-11: Math, wgpu viewport, CPU path tracer, USD USDA parser, instance-aware BVH
- ✅ M12: Intel Embree 4 integration (10K+ instances)
- ✅ M13: USD C++ bridge (USDC binary, references)
- ✅ M14: GPU instancing + frustum culling + LOD
- ✅ M15: UsdPreviewSurface + Disney Principled BSDF
- ✅ M16: MaterialX standard_surface
- ✅ M17: Textured PBR viewport + GeomSubsets
- ✅ M17.1: OpenImageIO .tx texture pipeline
- ✅ M18-18.5: USD animation, timeline UI, multi-prototype
- ✅ M19-19.5: Batch render (EXR + AOVs), frame rendering
- ✅ M20: Scene interactivity (picking, gizmos, undo)
- ✅ M21-21.2: Point instancing, scattering, node graph
- ✅ M23: SHARC radiance cache + Russian roulette

**274+ tests across 6 crates**

### In Progress

- 🔧 **M29:** USD export — import→modify→render→export pipeline (most phases done)

### Next Up

| Order | # | What it unlocks |
|-------|---|-----------------|
| 1 | 29 | USD export (in-progress) — full pipeline |
| 2 | 26 | Denoising (OIDN) — clean renders |
| 3 | 25 | Volumes/OpenVDB — smoke, fog |
| 4 | 22 | Viewport perf — production scenes |
| 5 | 27 | GPU path tracing — near-realtime |
| 6 | 28 | Qt 6 UI — professional interface |

**See:** [MILESTONES.md](MILESTONES.md) for complete milestone details

## File Structure

```
bif/
├── Cargo.toml                  # Rust workspace
├── crates/
│   ├── bif_math/               # Math primitives (Vec3, Ray, Aabb, Camera, Transform)
│   ├── bif_core/               # Scene graph, USD, mesh, materials, textures
│   │   └── src/
│   │       ├── scene.rs        # Scene struct
│   │       ├── undo.rs         # EditState + UndoStack
│   │       ├── point_cloud.rs  # Point cloud data
│   │       ├── scatter.rs      # Scatter algorithms
│   │       ├── primitives.rs   # Procedural geometry
│   │       └── usd/
│   │           ├── export.rs   # USD export pipeline (ExportConfig, export_scene)
│   │           └── cpp_bridge.rs # Rust↔C++ FFI wrapper
│   ├── bif_viewport/           # GPU viewport (wgpu + Vulkan + egui)
│   │   └── src/
│   │       ├── node_graph.rs   # egui-snarl node graph (10 node types)
│   │       ├── scene_browser.rs # CompositeProvider, CachedSceneGraph
│   │       ├── scene_loader.rs # USD loading orchestration
│   │       ├── render.rs       # Viewport render pipeline
│   │       ├── ivar_build.rs   # Ivar scene building
│   │       ├── ivar_state.rs   # Ivar render state machine
│   │       ├── ivar_renderer.rs # Ivar render integration
│   │       ├── property_inspector.rs # Property editing UI
│   │       └── point_preview.rs # Point cloud preview
│   ├── bif_renderer/           # CPU path tracer "Ivar" (Embree + Disney BSDF)
│   │   └── src/
│   │       ├── radiance_cache.rs # SHARC radiance cache
│   │       ├── pick_scene.rs   # Ray-based object picking
│   │       └── bucket.rs       # Bucket rendering
│   ├── bif_viewer/             # Application entry point
│   └── bif_maketx/             # Standalone .tx converter (OIIO subprocess)
├── cpp/
│   ├── usd_bridge/             # C++ FFI bridge to Pixar USD
│   └── oiio_bridge/            # C++ FFI bridge to OpenImageIO
├── devlog/                     # Development session logs
├── legacy/                     # Original Go raytracer (reference)
├── renders/                    # Render output files
└── assets/                     # Test scenes, meshes, HDRIs
```

## Design Decisions

### 1. Rust Over Go

- GPU capabilities via wgpu (essential)
- Better C++ FFI for USD/Embree
- Zero-cost abstractions, no GC pauses

### 2. Instance-Aware BVH → Embree (M0-11 → M12)

**Decision:** Started with instance-aware BVH in pure Rust, migrated to Embree at M12.

- M0-11: Proved architecture with pure Rust BVH (100 instances)
- M12: Embree for production scale (10K+ instances, two-level BVH, SIMD)

### 3. USD-Compatible → USD-Native (M0-11 → M13 → M29)

- M0-11: Pure Rust USDA parser for text files
- M13: USD C++ bridge added for USDC binary + references
- M29: Full bidirectional export via `export_scene()`

### 4. Dual Rendering (GPU + CPU)

- GPU: Interactive assembly (60 FPS)
- CPU: Production quality
- Best of both worlds

### 5. egui for Development, Qt 6 Optional

**Decision:** Started with egui (pure Rust), Qt migration deferred until egui hits limitations.

- egui + egui-snarl proved sufficient through M23 (node graph, scene browser, property inspector)
- Qt 6 remains an option for professional UI needs (M28)

### 6. Node Graph (egui-snarl)

**Decision:** Used egui-snarl for node-based workflow instead of a custom graph implementation.

- 10 node types cover the full import→scatter→instance→render→export pipeline
- Auto-compute dirty propagation keeps scene synchronized
- Avoids the complexity of a custom graph editor

## Non-Goals

**BIF is NOT:**

- Blender (no modeling/sculpting)
- Houdini (no procedural SOPs initially)
- Maya (no rigging/character animation)
- USD editor (USD is interchange, not internal format)

---

**Document Status:** Living document — updated for M0-23 complete, M29 in progress

**See Also:**
- [MILESTONES.md](MILESTONES.md) - Complete milestone history and roadmap
- [SESSION_HANDOFF.md](SESSION_HANDOFF.md) - Current status and next steps
- [REFERENCE.md](REFERENCE.md) - Code patterns and best practices
