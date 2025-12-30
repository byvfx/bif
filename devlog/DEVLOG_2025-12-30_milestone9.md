# Devlog: Milestone 9 - USD Import

**Date:** 2025-12-30  
**Duration:** ~3 hours  
**Status:** ✅ Complete

## Objective

Implement USDA (ASCII) file parsing to load `UsdGeomMesh` and `UsdGeomPointInstancer` prims, enabling USD scene compatibility in the BIF renderer.

## Approach

Chose **Option B: Custom USDA parser** over USD C++ bindings for simplicity:
- No C++ dependencies or build complexity
- Sufficient for geometry-only scope (Milestone 9)
- Line-by-line parsing with TODO for nom/pest robustness later

## Implementation

### Scene Graph Types (`bif_core`)

Created USD-compatible scene graph per ARCHITECTURE.md design:

```rust
// mesh.rs
pub struct Mesh {
    pub positions: Vec<Vec3>,
    pub normals: Option<Vec<Vec3>>,  // Optional - computed if missing
    pub indices: Vec<u32>,
    pub bounds: Aabb,
}

// scene.rs
pub struct Scene {
    pub prototypes: Vec<Arc<Prototype>>,
    pub instances: Vec<Instance>,
    pub name: String,
}

pub struct Prototype {
    pub id: usize,
    pub name: String,
    pub mesh: Arc<Mesh>,
    pub bounds: Aabb,
}

pub struct Instance {
    pub prototype_id: usize,
    pub transform: Transform,
}
```

### USD Parser (`bif_core/usd/`)

Modular structure:
- `types.rs` - Intermediate USD prim representations
- `parser.rs` - Line-by-line USDA tokenizer
- `loader.rs` - High-level `load_usda(path) -> Result<Scene>`

Supported syntax:
```usda
def Mesh "Cube" {
    point3f[] points = [(0, 0, 0), (1, 0, 0), ...]
    int[] faceVertexCounts = [4, 4, 4, 4, 4, 4]
    int[] faceVertexIndices = [0, 1, 2, 3, ...]
    normal3f[] normals = [...]  # Optional
}

def PointInstancer "Grid" {
    int[] protoIndices = [0, 0, 0, ...]
    point3f[] positions = [(0, 0, 0), (2, 0, 0), ...]
    
    def Mesh "Proto" { ... }  # Inline prototype
}

def Xform "World" {
    double3 xformOp:translate = (1, 2, 3)
    double3 xformOp:rotateXYZ = (0, 45, 0)
    double3 xformOp:scale = (2, 2, 2)
}
```

### Normals Handling

Per user requirement - check if USDA has normals, use existing or generate:

```rust
impl Mesh {
    pub fn ensure_normals(&mut self) {
        if self.normals.is_none() {
            self.compute_normals();  // Smooth vertex normals
        }
    }
}
```

### Test Files

Created three test USDA files in `assets/`:
- `test_cube.usda` - Unit cube with explicit normals
- `test_grid.usda` - 3x3 PointInstancer (9 instances of 1 prototype)
- `test_transform.usda` - Hierarchical Xform transforms

## Challenges

### Parser Bug: Type Array Brackets

Initial parser found `[` in `point3f[]` type declaration instead of value array:

```
point3f[] points = [(0, 0, 0), ...]
        ^-- Parser found this first!
```

**Fix:** Search for `=` first, then find `[` after it.

### xformOpOrder Interference

Lines like `uniform token[] xformOpOrder = ["xformOp:translate"]` were triggering `xformOp:translate` parsing.

**Fix:** Skip lines containing `xformOpOrder`.

### Depth Buffer / Winding Order Issue

**Symptom:** Back faces showing through front faces in rendered mesh.

**Root Cause:** USD/Houdini exports meshes with **clockwise** winding order, but wgpu defaults to **counter-clockwise** for front faces.

**Fix:** Changed `FrontFace::Ccw` to `FrontFace::Cw` in mesh render pipelines:

```rust
primitive: wgpu::PrimitiveState {
    front_face: wgpu::FrontFace::Cw,  // USD/Houdini uses clockwise winding
    cull_mode: Some(wgpu::Face::Back),
    // ...
}
```

### Point Normals vs Vertex Normals (Houdini)

**Issue:** Houdini vertex normals (per-face-corner) can cause inverted shading in BIF.

**Solution:** Use **point normals** in Houdini before export:
- Attribute Promote SOP: `N` from Vertex → Point, Average method

**Documentation:** Created `HOUDINI_EXPORT.md` with export best practices.

## Test Results

```
running 15 tests
test mesh::tests::test_bounds_computation ... ok
test mesh::tests::test_compute_normals ... ok
test usd::loader::tests::test_load_point_instancer ... ok
test usd::parser::tests::test_parse_simple_mesh ... ok
... (all 15 pass)
```

Example output:
```
$ cargo run --example load_usda -- assets/test_grid.usda

=== Scene: assets/test_grid.usda ===
Prototypes: 1
Instances: 9
Total triangles: 108

--- Instances ---
  [0] Proto 0 at (-2.00, 0.00, -2.00)
  [1] Proto 0 at (0.00, 0.00, -2.00)
  ...
  [8] Proto 0 at (2.00, 0.00, 2.00)
```

## Files Changed

| File | Change |
|------|--------|
| `crates/bif_core/src/lib.rs` | Export mesh, scene, usd modules |
| `crates/bif_core/src/mesh.rs` | New - Mesh with compute_normals() |
| `crates/bif_core/src/scene.rs` | New - Scene, Prototype, Instance, Transform |
| `crates/bif_core/src/usd/mod.rs` | New - USD module |
| `crates/bif_core/src/usd/types.rs` | New - UsdMesh, UsdPointInstancer, XformOp |
| `crates/bif_core/src/usd/parser.rs` | New - USDA parser |
| `crates/bif_core/src/usd/loader.rs` | New - load_usda() |
| `crates/bif_core/Cargo.toml` | Add log, dev-dependencies |
| `crates/bif_viewport/src/lib.rs` | Add from_core_mesh(), new_with_scene() |
| `crates/bif_viewer/src/main.rs` | Add CLI args, --usda support |
| `assets/test_*.usda` | New - Test files |

## Viewport Integration

**Completed:**

1. **`MeshData::from_core_mesh()`** - Converts `bif_core::Mesh` to GPU vertex format
   - Triangulates faces using face vertex counts/indices
   - Interleaves positions and normals into `MeshVertex` format
   - Computes normals if not present in source mesh

2. **`Renderer::new_with_scene()`** - Alternative constructor accepting `bif_core::Scene`
   - Extracts mesh and instance data from scene prototypes
   - Creates GPU buffers for vertex data and instance transforms
   - Integrates with existing camera/lighting pipeline

3. **CLI argument parsing** in bif_viewer:
   - `--usda <path>` or `-u <path>` to load USDA file
   - Positional argument for .usda files
   - `--help` for usage information

**Usage:**
```bash
cargo run -p bif_viewer -- --usda assets/test_grid.usda
cargo run -p bif_viewer -- assets/test_cube.usda
```

## Grey Material & Gnomon

**Completed:**

1. **Grey Placeholder Material** - Headlight-style diffuse lighting
   - Base grey color `(0.5, 0.5, 0.5)` 
   - View-space normal for headlight effect (surfaces facing camera are brightest)
   - Ambient + diffuse lighting in fragment shader

2. **Gnomon Axis Indicator** - 3D orientation widget in bottom-right corner
   - XYZ axes rendered as colored lines (Red=X, Green=Y, Blue=Z)
   - Rotates with camera to show current orientation
   - Separate render pass with custom viewport
   - Uses camera view rotation matrix (translation zeroed out)

3. **Enhanced UI Panel**
   - Scene Stats: Instances, Triangles, Polygons (estimated)
   - Gnomon Size slider (40-120px range)
   - Reorganized collapsible sections

## TODOs

- [ ] `frame_scene()` - Auto-frame camera to scene world bounds
- [ ] Track actual polygon count from source mesh (currently estimated as triangles * 2/3)
- [ ] Upgrade parser to nom/pest for robustness

## Next Steps (Milestone 10)

1. **USD References** - `references = @path@</prim>` for asset reuse
2. **Materials** - UsdShade support
3. **Binary USD** - .usdc format via cxx bindings

## Metrics

- **Lines of code:** ~1,500 (parser, loader, types, tests, viewport integration)
- **Test coverage:** 15 unit tests
- **Build time:** <3s incremental
- **Dependencies added:** 0 (uses existing thiserror, log)
