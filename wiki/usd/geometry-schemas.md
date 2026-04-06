---
title: "Geometry Schemas"
type: article
tags: [usd]
created: "2026-04-05"
updated: "2026-04-05"
sources: [../../docs/usd/schemas-geom.md, ../../docs/usd/datatypes.md]
---

# Geometry Schemas

UsdGeom provides the schema classes for all renderable geometry in USD, from meshes and curves to cameras and point instancers.

## Class Hierarchy

```text
UsdGeomImageable
+-- UsdGeomXformable
|   +-- UsdGeomGprim (base for geometric primitives)
|   |   +-- UsdGeomMesh
|   |   +-- UsdGeomBasisCurves
|   |   +-- UsdGeomNurbsCurves
|   |   +-- UsdGeomPoints
|   |   +-- UsdGeomCapsule, Cone, Cube, Cylinder, Sphere (intrinsics)
|   +-- UsdGeomPointInstancer
|   +-- UsdGeomCamera
|   +-- UsdGeomScope (namespace-only container)
+-- (no transform, just visibility/purpose)
```

## UsdGeomImageable

Base for anything that might be rendered. Provides:

- **visibility** (`inherited` | `invisible`) -- inherited down hierarchy
- **purpose** (`default` | `render` | `proxy` | `guide`) -- partitions scene for different consumers
- `ComputeVisibility()`, `ComputeWorldBound()`

### Purpose Categories

| Purpose | Use |
|---------|-----|
| `default` | All traversals |
| `render` | Final renders only |
| `proxy` | Lightweight/viewport |
| `guide` | Helper geometry (optional display) |

## UsdGeomXformable -- Transforms

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

Special features:

- **Suffix naming** for multiple ops of same type: `xformOp:translate:pivot`
- **Inverse ops** via `!invert!` prefix: `!invert!xformOp:translate:pivot`
- **Reset xform stack**: `!resetXformStack!` ignores all parent transforms

### XformCommonAPI

Simplified Scale-Rotate-Translate interface for DCC interop:

```python
UsdGeom.XformCommonAPI(prim).SetTranslate((10, 0, 0))
UsdGeom.XformCommonAPI(prim).SetRotate((0, 45, 0))
UsdGeom.XformCommonAPI(prim).SetScale((2, 2, 2))
```

## UsdGeomMesh

Encodes polygonal and subdivision meshes.

### Core Attributes

| Attribute | Type | Description |
|-----------|------|-------------|
| `points` | point3f[] | Vertex positions |
| `faceVertexCounts` | int[] | Number of vertices per face |
| `faceVertexIndices` | int[] | Indices into points array |
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

Vectorized instancing -- place many copies of prototype prims at different transforms. This is BIF's primary mechanism for scatter-based instancing.

### Core Attributes

| Attribute | Type | Description |
|-----------|------|-------------|
| `protoIndices` | int[] | Which prototype for each instance |
| `positions` | point3f[] | Instance positions |
| `orientations` | quath[] | Instance rotations (optional) |
| `scales` | float3[] | Instance scales (optional) |
| `velocities` | vector3f[] | For motion blur / interpolation |
| `invisibleIds` | int64[] | Instance indices to hide |
| `prototypes` | relationship | Targets the prototype prim children |

```usda
def PointInstancer "Forest" {
    int[] protoIndices = [0, 0, 1, 0, 1]
    point3f[] positions = [(0,0,0), (5,0,0), (10,0,0), (15,0,0), (20,0,0)]
    rel prototypes = [</Forest/Tree>, </Forest/Bush>]

    def Mesh "Tree" { ... }
    def Mesh "Bush" { ... }
}
```

**ComputeInstanceTransformsAtTime()** computes final 4x4 matrices from positions/orientations/scales, with optional velocity interpolation.

## UsdGeomCamera

```usda
def Camera "MainCam" {
    float focalLength = 50
    float horizontalAperture = 36
    float verticalAperture = 24
    float2 clippingRange = (0.1, 10000)
    token projection = "perspective"
}
```

Convention: +Y up, +X right, -Z forward (looking direction).

## Primvars (Primitive Variables)

Attributes that interpolate across geometric surfaces. Namespace: `primvars:`.

### Interpolation Modes

| Mode | # Elements | Description |
|------|-----------|-------------|
| `constant` | 1 | Whole mesh |
| `uniform` | # faces | Per-face |
| `vertex` / `varying` | # points | Per-point |
| `faceVarying` | # face-vertices | Per face-vertex (allows discontinuities) |

Common primvars: `primvars:st` (texture coords), `primvars:displayColor`, `primvars:displayOpacity`.

**Note:** USD convention uses `st` not `uv` for texture coordinates.

## Stage Metrics

```python
UsdGeom.SetStageUpAxis(stage, UsdGeom.Tokens.y)  # or .z
UsdGeom.SetStageMetersPerUnit(stage, 0.01)  # centimeters
```

## Common Data Types for Geometry

| Data | Recommended Type |
|------|-----------------|
| Vertex positions | `point3f[]` |
| Normals | `normal3f[]` |
| Texture coordinates | `texCoord2f[]` (as primvar) |
| Display color | `color3f[]` (as primvar) |
| Face vertex indices | `int[]` |
| Transform matrix | `matrix4d` |
| Instancer positions | `point3f[]` |
| Instancer orientations | `quath[]` |

**Transform role types matter:** `point3f` transforms with full matrix (including translation), `vector3f` ignores translation, `normal3f` transforms with inverse-transpose.

## How BIF Uses UsdGeom

- **Mesh loading** via C++ bridge reads `points`, `faceVertexCounts`, `faceVertexIndices`, normals, and UVs
- **PointInstancer** is the primary output for BIF's Scatter and PointInstancer nodes
- **Xformable** transforms are decomposed into translate/orient/scale for the viewport and authored as `xformOpOrder` on export
- **Axis correction** matrix is computed from stage metrics (`AxisCorrection` in `scene_pipeline.rs`)
- **Subdivision** support planned via OpenSubdiv with GPU acceleration

## Related

- [[stage-layer-prim|Stage, Layer, Prim]] -- USD object model
- [[shading-materials|Shading and Materials]] -- materials bind to geometry
- [[composition-arcs|Composition Arcs]] -- how geometry assets are composed
- [[bif-usd-integration|BIF USD Integration]] -- geometry in BIF's workflow
- [[wgpu Rendering Pipeline]] -- how geometry reaches the GPU
