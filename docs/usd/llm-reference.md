# OpenUSD LLM-Optimized Reference

Consolidated from OpenUSD API docs + glossary. Use alongside `docs/usd/*.md` for deep dives.

---

## Core Object Model

```text
UsdStage ─────────────────────────────────────────────┐
  │                                                    │
  ├── GetRootLayer() → SdfLayer (persistent data)      │
  ├── GetSessionLayer() → SdfLayer (temp overrides)    │
  ├── GetPrimAtPath(path) → UsdPrim                    │
  ├── Traverse() → UsdPrimRange (depth-first iter)     │
  ├── SetEditTarget(layer) → direct authoring target   │
  └── Load()/Unload() → payload control                │
                                                       │
UsdPrim ───────────────────────────────────────────────┤
  │                                                    │
  ├── GetParent(), GetChildren(), GetDescendants()     │
  ├── GetAttribute(name) → UsdAttribute                │
  ├── GetRelationship(name) → UsdRelationship          │
  ├── GetPrimPath() → SdfPath                          │
  ├── IsActive(), SetActive(bool)                      │
  └── GetTypeName(), IsA<T>(), HasAPI<T>()             │
                                                       │
SdfLayer ──────────────────────────────────────────────┤
  │                                                    │
  ├── ImportFromFile(path), ExportToFile(path)         │
  ├── GetSubLayerPaths() → sublayer stack              │
  ├── GetPrimSpecAtPath(path) → SdfPrimSpec            │
  └── GetAssetInfo() → metadata                        │
```

---

## Composition: LIVRPS Strength Order

**Strong → Weak** (opinions applied left-to-right, stronger wins):

| Arc | Strength | Use Case | Key Feature |
|-----|----------|----------|-------------|
| **L**ocal | Strongest | Direct authoring on prim | Immediate override |
| **I**nherits | ↓ | Shared base classes | Live propagation across instances |
| **V**ariantSets | ↓ | Asset variations | Switchable variations (LOD, geo variant) |
| **R**eferences | ↓ | Asset assembly | Primary composition arc for reuse |
| **P**ayloads | ↓ | Heavy data | Lazy-loadable (load/unload) |
| **S**pecializes | Weakest | Base refinement | Base always weaker than derived |

### Quick Examples

```python
# Reference
prim.GetReferences().AddReference('@asset.usd@')

# Payload (lazy-load)
prim.GetPayloads().AddPayload('@heavy_geo.usd@')
stage.Unload(payload_path)

# Variant
vs = prim.GetVariantSet('geoVariant')
vs.SetVariantSelection('high')

# Inherits (live updates)
prim.GetInherits().AddInherit('/Assets/BaseMaterial')

# Specializes (base always weaker)
prim.GetSpecializes().AddSpecialize('/Assets/BaseMetal')
```text

---

## Stage Operations

### Open/Create

```python
stage = Usd.Stage.Open('scene.usd')                    # Open existing
stage = Usd.Stage.Open('scene.usd', Usd.Stage.LoadNone)  # Skip payloads
stage = Usd.Stage.CreateNew('new.usda')                # Create new
stage = Usd.Stage.CreateInMemory()                     # In-memory only
```

### Traversal

```python
# Default: active, defined, loaded, concrete prims
for prim in stage.Traverse():
    process(prim)

# Filtered traversal
predicate = Usd.PrimIsDefined & Usd.PrimIsActive & ~Usd.PrimIsAbstract
for prim in stage.Traverse(predicate):
    process(prim)

# Subtree from prim
for prim in Usd.PrimRange.Subtree(root_prim):
    if skip_condition:
        prim.PruneChildren()  # Skip subtree
        continue
```text

### Edit Targets

```python
# Author to specific layer
stage.SetEditTarget(Usd.EditTarget(layer))
prim.GetAttribute('size').Set(10.0)
stage.Save()
```

---

## Prim Operations

### Creation & Definition

```python
# Define with type
prim = stage.DefinePrim('/World/Geom', 'Xform')
mesh = stage.DefinePrim('/World/Geom/Mesh', 'Mesh')

# Override (no type, just opinions)
over = stage.OverridePrim('/World/Overrides')

# Check prim
prim.IsValid(), prim.IsActive(), prim.IsDefined()
prim.GetTypeName(), prim.GetPrimPath()
```text

### Attributes

```python
# Get/create attribute
attr = prim.GetAttribute('size')
attr = prim.CreateAttribute('customAttr', Sdf.ValueTypeNames.Float)

# Values
attr.Set(42.0)                    # Default time
attr.Set(42.0, Usd.TimeCode(10))  # At frame 10
val = attr.Get()                  # Default time
val = attr.Get(Usd.TimeCode(10))  # At frame 10

# Time samples
times = attr.GetTimeSamplesInInterval(Gf.Interval(1, 100))
attr.HasAuthoredValue(), attr.HasValue()
```

### Relationships

```python
rel = prim.CreateRelationship('target')
rel.AddTarget('/World/Geom/Mesh')
rel.SetTargets(['/World/A', '/World/B'])
targets = rel.GetForwardedTargets()  # Resolves through redirects
```text

---

## SdfPath Syntax

```

/                    # Absolute root
/World              # Absolute prim path
/World/Geom         # Nested prim
/World.Mesh         # Property (attribute or relationship)
/World.Mesh:primvar # Namespaced property
/World[target]      # Relationship target
/World{variant}=value  # Variant selection
../Sibling          # Relative parent

```text

```python
path = Sdf.Path('/World/Geom')
path.AppendChild('Mesh')        # → /World/Geom/Mesh
path.AppendProperty('size')     # → /World/Geom.size
path.GetParent()                # → /World
path.IsPrimPath(), path.IsPropertyPath()
path.StripAllVariantSelections()
```

---

## Geometry (UsdGeom)

### Mesh

```python
mesh = UsdGeom.Mesh(prim)
mesh.CreatePointsAttr(Vt.Vec3fArray([...]))
mesh.CreateFaceVertexCountsAttr(Vt.IntArray([3, 3, ...]))  # Triangles
mesh.CreateFaceVertexIndicesAttr(Vt.IntArray([0,1,2, ...]))
mesh.CreateSubdivisionSchemeAttr(UsdGeom.Tokens.bilinear)  # none, catmullClark, loop
```text

### Primvars (Per-Primitive Variables)

| Interpolation | Elements | Use Case |
|--------------|----------|----------|
| `constant` | 1 | Whole mesh uniform value |
| `uniform` | # faces | Per-face value |
| `vertex` | # points | Per-point, smooth interp |
| `faceVarying` | # face-vertices | UVs, normals (allow discontinuities) |

```python
primvarAPI = UsdGeom.PrimvarsAPI(mesh)
uv = primvarAPI.CreatePrimvar('st', Sdf.ValueTypeNames.TexCoord2fArray, UsdGeom.Tokens.faceVarying)
uv.Set(Vt.Vec2fArray([...]))
uv.SetIndices(Vt.IntArray([...]))  # Optional indexing
```

### Transforms (UsdGeomXformable)

```python
xform = UsdGeom.Xformable(prim)
xform.ClearXformOpOrder()
xformOp = xform.AddTranslateOp()
xformOp.Set(Gf.Vec3d(10, 0, 0))

# Or use simplified API
xformAPI = UsdGeom.XformCommonAPI(prim)
xformAPI.SetTranslate((10, 0, 0))
xformAPI.SetRotate((0, 45, 0))  # XYZ Euler degrees
xformAPI.SetScale((2, 2, 2))
```text

**Transform order:** Scale → Rotate → Translate (SRT convention)

### PointInstancer

```python
pi = UsdGeom.PointInstancer(prim)
pi.CreateProtoIndicesAttr(Vt.IntArray([0, 0, 1, 0]))      # Which prototype per instance
pi.CreatePositionsAttr(Vt.Vec3fArray([...]))              # Instance positions
pi.CreateOrientationsAttr(Vt.QuathArray([...]))           # Optional rotations
pi.CreateScalesAttr(Vt.Vec3fArray([...]))                 # Optional scales
pi.CreatePrototypesRel(['/Asset/Tree', '/Asset/Bush'])    # Prototype prims
pi.CreateInvisibleIdsAttr(Vt.Int64Array([2, 5]))          # Hide instances
```

### Camera

```python
cam = UsdGeom.Camera(prim)
cam.CreateProjectionAttr(UsdGeom.Tokens.perspective)  # or orthographic
cam.CreateFocalLengthAttr(50.0)           # mm
cam.CreateHorizontalApertureAttr(36.0)    # mm (sensor width)
cam.CreateClippingRangeAttr(Gf.Vec2f(0.1, 10000))
```text

**Convention:** +Y up, +X right, -Z forward (looking direction)

---

## Materials & Shading (UsdShade)

### Material Binding

```python
# Apply MaterialBindingAPI to geom prim
UsdShade.MaterialBindingAPI.Apply(geomPrim)

# Direct binding
bindingAPI = UsdShade.MaterialBindingAPI(geomPrim)
bindingAPI.Bind(material)

# Collection-based binding
bindingAPI.Bind(collection, material, UsdShade.Tokens.strongerThanDescendants)
```

### Material Structure

```usda
def Material "MyMaterial" {
    def Shader "PreviewSurface" {
        token outputs:out
    }
    token outputs:surface.connect = </Materials/MyMaterial/PreviewSurface.outputs:out>
}
```text

### Shader Connections

```python
# Connect texture to shader input
shader.CreateInput('diffuseColor', Sdf.ValueTypeNames.Color3f).ConnectToSource(
    textureShader.ConnectableAPI(), 'rgb'
)

# NodeGraph interface passthrough
ng.CreateInput('baseColor', Sdf.ValueTypeNames.Color3f)
ng.CreateOutput('result', Sdf.ValueTypeNames.Color3f)
ng.GetOutput('result').ConnectToSource(ng.GetInput('baseColor'))
```

**Rule:** Connected values win over authored values.

### UsdPreviewSurface (Metallic Workflow)

```python
shader.CreateInput('diffuseColor', Sdf.ValueTypeNames.Color3f).Set((0.8, 0.2, 0.1))
shader.CreateInput('metallic', Sdf.ValueTypeNames.Float).Set(1.0)
shader.CreateInput('roughness', Sdf.ValueTypeNames.Float).Set(0.3)
shader.CreateInput('ior', Sdf.ValueTypeNames.Float).Set(1.5)
```text

---

## Lighting (UsdLux)

### Light Types

| Type | Description | Key Attributes |
|------|-------------|----------------|
| `DistantLight` | Directional/sun | `angle`, `intensity` |
| `DomeLight` | HDRI environment | `texture:file` |
| `DiskLight` | Area disk | `radius`, `intensity` |
| `RectLight` | Area rectangle | `width`, `height` |
| `SphereLight` | Omnidirectional point | `radius`, `treatAsPoint` |
| `CylinderLight` | Area tube | `length`, `radius` |

### Common Light Attributes

```python
light.CreateIntensityAttr(100.0)
light.CreateExposureAttr(1.0)
light.CreateColorAttr(Gf.Vec3f(1, 0.9, 0.8))
light.CreateDiffuseAttr(1.0)
light.CreateSpecularAttr(1.0)
```

---

## Model Hierarchy

```text
assembly (published asset root)
  └── group (organizational container)
       └── component (terminal model, no child models)
            └── subcomponent (articulable parts)
```

```python
Usd.ModelAPI(prim).SetKind(Kind.Tokens.component)
kind = Usd.ModelAPI(prim).GetKind()
```text

## Purpose (Visibility Categories)

| Purpose | Include In |
|---------|-----------|
| `default` | All traversals |
| `render` | Final renders only |
| `proxy` | Lightweight/viewport |
| `guide` | Helper geometry (optional) |

```python
imageable = UsdGeom.Imageable(prim)
imageable.CreatePurposeAttr(UsdGeom.Tokens.render)
```

---

## File Formats

| Format | Description | Use Case |
|--------|-------------|----------|
| `.usda` | ASCII text | Debugging, small files, human-readable |
| `.usdc` | Binary crate | Production, fast I/O, compact |
| `.usd` | Either (auto-detect) | Default, lets USD choose |
| `.usdz` | Uncompressed zip | AR/iOS, single-package delivery |

---

## Common Patterns

### Layer-Relative Authoring

```python
# Stronger layer overrides weaker
layer_a = Sdf.Layer.FindOrOpen('base.usda')
layer_b = Sdf.Layer.FindOrOpen('override.usda')
stage = Usd.Stage.Open(layer_b)  # layer_b is root
# Opinions in layer_b win over layer_a (if layer_a sublayered)
```text

### Non-Destructive Delete

```python
prim.SetActive(False)  # Deactivate = hide + don't compose descendants
```

### Variant Selection

```python
prim.GetVariantSet('LOD').AddVariant('high')
prim.GetVariantSet('LOD').SetVariantSelection('high')
```text

### Time-Sampled Animation

```python
attr = prim.CreateAttribute('translate', Sdf.ValueTypeNames.Double3)
for frame in range(1, 101):
    attr.Set(compute_position(frame), Usd.TimeCode(frame))
```

---

## Key API Classes Quick Reference

| Class | Purpose |
|-------|---------|
| `UsdStage` | Scene container, composition, traversal |
| `UsdPrim` | Scenegraph object, attributes, relationships |
| `UsdAttribute` | Animated/static property values |
| `UsdRelationship` | Links between prims |
| `SdfLayer` | File-backed data storage |
| `SdfPath` | Path string manipulation |
| `SdfPrimSpec` | Layer-level prim specification |
| `UsdGeomMesh` | Polygonal/subdivision mesh |
| `UsdGeomXformable` | Transformable geometry |
| `UsdGeomPointInstancer` | Efficient instancing |
| `UsdShadeMaterial` | Material container |
| `UsdShadeShader` | Shader node |
| `UsdLuxDomeLight` | HDRI environment light |

---

## Gotchas

1. **Payloads not loaded by default with `LoadNone`** — must call `stage.Load(path)`
2. **Session layer is strongest** — never saved, for temporary overrides
3. **Primvars use `st` not `uv`** — `primvars:st` is the convention
4. **Transform matrices are row-major** — pre-multiply: `v' = v * M`
5. **Rotations in degrees** — not radians
6. **Connections win over values** — if input has both, connection is used
7. **Deactivation prevents composition** — descendants not even composed
8. **EditTarget must be in LayerStack** — can't author to arbitrary layer

---

## Deep API Lookups

When you need detailed method signatures:

- `https://openusd.org/release/api/class_usd_stage.html`
- `https://openusd.org/release/api/class_usd_prim.html`
- `https://openusd.org/release/api/class_sdf_layer.html`
- `https://openusd.org/release/api/class_usd_geom_mesh.html`

---

*Indexed from OpenUSD 25.x docs. For BIF-specific patterns, see `docs/usd/*.md`.*
