---
title: "UsdGeom PointInstancer"
type: concept
tags: [usd, geometry, instancing, scene-assembly]
created: "2026-04-05"
updated: "2026-04-05"
---

## Summary

UsdGeomPointInstancer is USD's mechanism for efficiently placing thousands to millions of instances of prototype geometry at positions defined by a point cloud. Instead of duplicating mesh data per instance, it stores per-instance transforms (positions, orientations, scales, velocities) and indices into a set of prototype prims. This is the backbone of environment scattering in VFX -- trees in a forest, rocks on terrain, debris in destruction.

## Details

### Key Attributes

| Attribute | Type | Purpose |
|-----------|------|---------|
| `positions` | Vec3f[] | World-space position per instance |
| `orientations` | Quath[] | Rotation per instance |
| `scales` | Vec3f[] | Scale per instance |
| `velocities` | Vec3f[] | For motion blur interpolation |
| `protoIndices` | int[] | Which prototype each instance uses |
| `prototypes` | rel | Relationship to prototype prims |
| `invisibleIds` | int64[] | Instance IDs to hide (pruning) |

### Instance Transform

Final instance transform = `Translate(position) * Rotate(orientation) * Scale(scale)`, applied to the prototype's local geometry. USD computes this per-instance via `ComputeInstanceTransformsAtTime()`.

### Performance

- **Memory**: N instances share one prototype mesh -- 10,000 trees use the memory of ~1 tree + 10,000 transforms.
- **Nesting**: PointInstancers can instance other PointInstancers (nested instancing) for extreme scale.
- **Hydra**: The renderer receives instance indices and transforms, never expanded geometry.

### vs Scenegraph Instancing

USD also supports "native" scenegraph instancing (instanceable prims). PointInstancer is preferred when instance count is high and per-instance variation is limited to transform + prototype selection.

## In BIF

PointInstancer is deeply integrated across the codebase:

- **PointInstancer node** in the node graph (`bif_viewport::node_graph`) creates instancers from scatter results.
- **Scatter node** generates point clouds; PointInstancer node consumes them.
- **Scene browser** (`bif_viewport::scene_browser`) displays PointInstancers in the hierarchy.
- **Export** (`bif_core::usd::export`) writes `UsdGeomPointInstancer` prims with positions, orientations, scales, and prototype relationships.
- **C++ bridge** (`bif_core::usd::cpp_bridge`, `ffi_raw`) handles reading PointInstancer data from USD stages.
- **Renderer** (`bif_renderer::instanced_geometry`) renders instances efficiently using the instancer's transform arrays.

## Related

- [[primvars]] -- per-instance primvars can vary appearance across instances
- [[composition-arcs]] -- prototypes are often referenced from other USD files
- [[bvh]] -- instance BVHs for ray tracing instanced geometry
- [[openpbr]] -- material applied to prototypes, shared by all instances
