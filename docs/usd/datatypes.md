# USD Data Types

## Basic Types

| Value Type Token | C++ Type | Rust Equivalent | Description |
|-----------------|----------|-----------------|-------------|
| `bool` | `bool` | `bool` | Boolean |
| `uchar` | `uint8_t` | `u8` | 8-bit unsigned |
| `int` | `int32_t` | `i32` | 32-bit signed |
| `uint` | `uint32_t` | `u32` | 32-bit unsigned |
| `int64` | `int64_t` | `i64` | 64-bit signed |
| `uint64` | `uint64_t` | `u64` | 64-bit unsigned |
| `half` | `GfHalf` | `f16` (half crate) | 16-bit float |
| `float` | `float` | `f32` | 32-bit float |
| `double` | `double` | `f64` | 64-bit float |
| `timecode` | `SdfTimeCode` | `f64` | Resolvable time |
| `string` | `std::string` | `String` | UTF-8 string |
| `token` | `TfToken` | `String` (interned) | Interned string, fast comparison |
| `asset` | `SdfAssetPath` | `String` (path) | Resolvable asset path |
| `opaque` | `SdfOpaqueValue` | — | Non-serializable |

## Vector Types

| Value Type Token | C++ Type | Components | Description |
|-----------------|----------|------------|-------------|
| `int2` | `GfVec2i` | 2 × int | |
| `half2` | `GfVec2h` | 2 × half | |
| `float2` | `GfVec2f` | 2 × float | |
| `double2` | `GfVec2d` | 2 × double | |
| `int3` | `GfVec3i` | 3 × int | |
| `half3` | `GfVec3h` | 3 × half | |
| `float3` | `GfVec3f` | 3 × float | |
| `double3` | `GfVec3d` | 3 × double | |
| `int4` | `GfVec4i` | 4 × int | |
| `half4` | `GfVec4h` | 4 × half | |
| `float4` | `GfVec4f` | 4 × float | |
| `double4` | `GfVec4d` | 4 × double | |

## Matrix Types

| Value Type Token | C++ Type | Size |
|-----------------|----------|------|
| `matrix2d` | `GfMatrix2d` | 2×2 double |
| `matrix3d` | `GfMatrix3d` | 3×3 double |
| `matrix4d` | `GfMatrix4d` | 4×4 double |

## Quaternion Types

| Value Type Token | C++ Type | Components |
|-----------------|----------|------------|
| `quath` | `GfQuath` | half (w, x, y, z) |
| `quatf` | `GfQuatf` | float (w, x, y, z) |
| `quatd` | `GfQuatd` | double (w, x, y, z) |

**Note:** USD quaternions are stored as (real, imaginary) = (w, x, y, z), scalar-first.

## Role Types

Roles assign semantic meaning to underlying numeric types. They share storage with their base type but convey intent:

| Value Type Token | Base Type | Semantic Role |
|-----------------|-----------|---------------|
| `point3h/f/d` | `GfVec3h/f/d` | Position in space |
| `normal3h/f/d` | `GfVec3h/f/d` | Surface normal (transforms differently) |
| `vector3h/f/d` | `GfVec3h/f/d` | Direction vector |
| `color3h/f/d` | `GfVec3h/f/d` | RGB color |
| `color4h/f/d` | `GfVec4h/f/d` | RGBA color |
| `texCoord2h/f/d` | `GfVec2h/f/d` | 2D texture coordinate |
| `texCoord3h/f/d` | `GfVec3h/f/d` | 3D texture coordinate |
| `frame4d` | `GfMatrix4d` | Coordinate frame |

**Important for transforms:** `point3f` transforms with full matrix (including translation), `vector3f` ignores translation, `normal3f` transforms with inverse-transpose.

## Array Types

Every scalar type has an array counterpart (VtArray in C++):
- `float[]`, `int[]`, `point3f[]`, `token[]`, etc.
- Arrays are the primary way to store per-vertex, per-face, and per-instance data

## SdfValueTypeNames Lookup

In Python, access type names via `Sdf.ValueTypeNames`:

```python
Sdf.ValueTypeNames.Float
Sdf.ValueTypeNames.Float3
Sdf.ValueTypeNames.Color3f
Sdf.ValueTypeNames.Point3f
Sdf.ValueTypeNames.Normal3f
Sdf.ValueTypeNames.TexCoord2f
Sdf.ValueTypeNames.Matrix4d
Sdf.ValueTypeNames.Token
Sdf.ValueTypeNames.Asset
Sdf.ValueTypeNames.Float3Array   # Array variant
Sdf.ValueTypeNames.IntArray
Sdf.ValueTypeNames.Point3fArray
```

In C++:

```cpp
SdfValueTypeNames->Float
SdfValueTypeNames->Float3
SdfValueTypeNames->Color3f
// etc.
```

## Common Attribute Type Choices

| Data | Recommended Type |
|------|-----------------|
| Vertex positions | `point3f[]` |
| Normals | `normal3f[]` |
| Texture coordinates | `texCoord2f[]` (as primvar) |
| Display color | `color3f[]` (as primvar) |
| Face vertex indices | `int[]` |
| Face vertex counts | `int[]` |
| Transform matrix | `matrix4d` |
| Instancer positions | `point3f[]` |
| Instancer orientations | `quath[]` |
| Instancer scales | `float3[]` |
| Instancer proto indices | `int[]` |
| Time samples | `double` (timecode) |
| Asset paths | `asset` |
| Enum-like tokens | `token` (with allowedTokens) |
