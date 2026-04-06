---
title: "Primvars (Primitive Variables)"
type: concept
tags: [usd, geometry, shading]
created: "2026-04-05"
updated: "2026-04-05"
---

## Summary

Primvars (primitive variables) are USD's mechanism for attaching arbitrary data to geometry that can vary across a primitive's surface. They are the USD equivalent of renderman's "varying" and "facevarying" attributes, and they are the primary way texture coordinates, vertex colors, and custom per-face data travel from geometry to shaders.

## Details

### Interpolation Modes

| Mode | Meaning | Example |
|------|---------|---------|
| `constant` | One value for entire prim | Material ID |
| `uniform` | One value per face/element | Face group index |
| `varying` | One value per vertex (bilinear interp) | Legacy UVs |
| `vertex` | One value per vertex (subdivision-aware) | Subdivision UVs |
| `faceVarying` | One value per face-vertex | Most common for UVs, allows seams |

### Key Properties

- **Namespace prefix**: `primvars:` in the attribute name (e.g., `primvars:st`, `primvars:displayColor`).
- **Indices**: Primvars can be indexed to save memory -- store unique values + an index array. Critical for faceVarying UVs where many face-vertices share the same UV.
- **Inheritance**: Constant primvars inherit down the hierarchy (a primvar on `/World` is visible to `/World/Mesh`). Non-constant primvars do not inherit.
- **displayColor/displayOpacity**: Special built-in primvars for viewport preview.

### Primvars vs Attributes

Regular attributes are just metadata on a prim. Primvars are attributes with interpolation semantics -- the renderer knows *how* to interpolate them across the surface.

## In BIF

- **UV loading**: `bif_core::usd::loader` reads `primvars:st` (faceVarying) for texture mapping, handling indexed primvars.
- **Vertex colors**: `primvars:displayColor` is read for viewport display in `bif_viewport`.
- **Export**: `bif_core::usd::export` writes primvars for UVs and display colors on exported meshes.
- **MaterialX texcoord**: The [[materialx]] texture nodes reference primvar names to know which UV set to sample.

## Related

- [[composition-arcs]] -- primvars participate in composition like any attribute
- [[point-instancer]] -- per-instance primvars for varying instance appearance
- [[openpbr]] -- material model that consumes primvar-driven textures
- `docs/usd/schemas-geom.md` -- UsdGeom primvar details
