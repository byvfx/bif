# UsdGeom — Geometry Schemas

## Class Hierarchy

```
UsdGeomImageable
├── UsdGeomXformable
│   ├── UsdGeomGprim (base for geometric primitives)
│   │   ├── UsdGeomMesh
│   │   ├── UsdGeomBasisCurves
│   │   ├── UsdGeomNurbsCurves
│   │   ├── UsdGeomPoints
│   │   ├── UsdGeomNurbsPatch
│   │   ├── UsdGeomCapsule, Cone, Cube, Cylinder, Sphere (intrinsics)
│   ├── UsdGeomPointInstancer
│   ├── UsdGeomCamera
│   ├── UsdGeomBoundable
│   │   └── (lights inherit from here)
│   └── UsdGeomScope (namespace-only container)
└── (no transform, just visibility/purpose)
```

## UsdGeomImageable

Base for anything that might be rendered. Provides:
- **visibility** (`inherited` | `invisible`) — inherited down hierarchy
- **purpose** (`default` | `render` | `proxy` | `guide`) — partitions scene for different consumers
- `ComputeVisibility()`, `ComputeWorldBound()`

## UsdGeomXformable — Transforms

All geometry prims inherit from Xformable, providing an ordered stack of transform operations.

### XformOps

Transform is defined by an ordered list of typed operations stored in `xformOpOrder`:

| Op Type | Attribute | Value Type |
|---------|-----------|------------|
| Translate | `xformOp:translate` | double3 |
| Rotate (euler) | `xformOp:rotateXYZ` (or XZY, YXZ, etc.) | float3/double3 |
| Rotate (quaternion) | `xformOp:orient` | quatf/quatd |
| Scale | `xformOp:scale` | float3/double3 |
| Transform (4x4) | `xformOp:transform` | matrix4d |

```usda
def Xform "Obj" {
    double3 xformOp:translate = (10, 0, 0)
    float3 xformOp:rotateXYZ = (0, 45, 0)
    float3 xformOp:scale = (2, 2, 2)
    uniform token[] xformOpOrder = ["xformOp:translate", "xformOp:rotateXYZ", "xformOp:scale"]
}
```

**Suffix naming** for multiple ops of same type: `xformOp:translate:pivot`

**Inverse ops** via `!invert!` prefix in xformOpOrder: `!invert!xformOp:translate:pivot`

**Reset xform stack**: `!resetXformStack!` in xformOpOrder ignores all parent transforms.

### XformCommonAPI

Simplified interface for standard Scale-Rotate-Translate pattern:

```python
UsdGeom.XformCommonAPI(prim).SetTranslate((10, 0, 0))
UsdGeom.XformCommonAPI(prim).SetRotate((0, 45, 0))
UsdGeom.XformCommonAPI(prim).SetScale((2, 2, 2))
```

Best for DCC interop — conforms to a fixed S-R-T pattern most apps can import.

## UsdGeomMesh

Encodes polygonal and subdivision meshes.

### Core Attributes

| Attribute | Type | Description |
|-----------|------|-------------|
| `points` | point3f[] | Vertex positions |
| `faceVertexCounts` | int[] | Number of vertices per face |
| `faceVertexIndices` | int[] | Indices into points array per face-vertex |
| `normals` | normal3f[] | Surface normals (optional) |
| `subdivisionScheme` | token | `catmullClark` (default), `loop`, `bilinear`, `none` |
| `orientation` | token | `rightHanded` (default) or `leftHanded` |
| `extent` | float3[2] | Bounding box [min, max] |

```usda
def Mesh "Quad" {
    point3f[] points = [(0,0,0), (1,0,0), (1,1,0), (0,1,0)]
    int[] faceVertexCounts = [4]
    int[] faceVertexIndices = [0, 1, 2, 3]
    token subdivisionScheme = "none"  # Polygonal mesh
}
```

**Important:** Default subdivision is `catmullClark`. For polygonal meshes, explicitly set `subdivisionScheme = "none"`.

### Subdivision Attributes

| Attribute | Description |
|-----------|-------------|
| `interpolateBoundary` | How subdivision handles mesh boundaries |
| `faceVaryingLinearInterpolation` | How face-varying primvars interpolate |
| `creaseIndices`, `creaseLengths`, `creaseSharpnesses` | Crease edges |
| `cornerIndices`, `cornerSharpnesses` | Crease corners |

## UsdGeomPointInstancer

Vectorized instancing — place many copies of prototype prims at different transforms.

### Core Attributes

| Attribute | Type | Description |
|-----------|------|-------------|
| `protoIndices` | int[] | Which prototype for each instance |
| `positions` | point3f[] | Instance positions (same size as protoIndices) |
| `orientations` | quath[] | Instance rotations (optional) |
| `scales` | float3[] | Instance scales (optional) |
| `velocities` | vector3f[] | For motion blur / interpolation |
| `angularVelocities` | vector3f[] | Angular velocity for motion blur |
| `invisibleIds` | int64[] | Instance indices to hide |
| `prototypes` | relationship | Targets the prototype prim children |

```usda
def PointInstancer "Forest" {
    int[] protoIndices = [0, 0, 1, 0, 1]
    point3f[] positions = [(0,0,0), (5,0,0), (10,0,0), (15,0,0), (20,0,0)]
    quath[] orientations = [...]
    rel prototypes = [</Forest/Tree>, </Forest/Bush>]

    def Mesh "Tree" { ... }
    def Mesh "Bush" { ... }
}
```

**Mask:** `invisibleIds` hides specific instances without removing data.

**ComputeInstanceTransformsAtTime()** — computes final 4x4 matrices from positions/orientations/scales, with optional velocity interpolation.

## UsdGeomCamera

```usda
def Camera "MainCam" {
    float focalLength = 50        # mm
    float horizontalAperture = 36 # mm (sensor width)
    float verticalAperture = 24   # mm (sensor height)
    float2 clippingRange = (0.1, 10000)
    token projection = "perspective"  # or "orthographic"
}
```

Views scene in right-handed coordinates: **+Y up, +X right, -Z forward** (looking direction).

## Primvars (Primitive Variables)

Attributes that can interpolate across geometric surfaces. Namespace: `primvars:`.

### Interpolation Modes

| Mode | # Elements | Description |
|------|-----------|-------------|
| `constant` | 1 | Whole mesh |
| `uniform` | # faces | Per-face |
| `vertex` / `varying` | # points | Per-point (varying = linear interp on subdiv) |
| `faceVarying` | # face-vertices | Per face-vertex (allows discontinuities at edges) |

```python
primvar = UsdGeom.PrimvarsAPI(mesh).CreatePrimvar('st', Sdf.ValueTypeNames.TexCoord2fArray, UsdGeom.Tokens.faceVarying)
primvar.Set(uvs)
primvar.SetIndices(Vt.IntArray([...]))  # Optional: indexed primvar
```

Common primvars: `primvars:st` (texture coords), `primvars:displayColor`, `primvars:displayOpacity`

## Stage Metrics

```python
UsdGeom.SetStageUpAxis(stage, UsdGeom.Tokens.y)  # or .z
UsdGeom.GetStageUpAxis(stage)
UsdGeom.SetStageMetersPerUnit(stage, 0.01)  # centimeters
```

## Coordinate System & Winding

- **Right-handed** coordinate system
- **Right-hand rule** for surface normals by default
- `orientation` attribute: `rightHanded` (default) or `leftHanded` — per-gprim override
- Camera: +Y up, +X right, -Z forward
