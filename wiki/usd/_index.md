---
title: USD Knowledge Map
type: index
updated: "2026-04-05"
---

# USD (Universal Scene Description)

BIF's core scene description is built on USD. These articles cover how USD works and how BIF integrates with it.

## Articles

- [[composition-arcs|Composition Arcs]] — LIVRPS, sublayers, references, payloads, variants
- [[stage-layer-prim|Stage, Layer, Prim]] — Core USD concepts and value resolution
- [[shading-materials|Shading and Materials]] — UsdShade, material binding, shader connections
- [[geometry-schemas|Geometry Schemas]] — UsdGeom mesh, xformable, point instancer, cameras
- [[bif-usd-integration|BIF USD Integration]] — Hybrid workflow, C++ bridge, export pipeline

## Source Reference Docs

These curated docs live outside the vault:

- [concepts.md](../../docs/usd/concepts.md) — Stage, Layer, Prim, value resolution
- [composition.md](../../docs/usd/composition.md) — LIVRPS and all composition arcs
- [schemas-geom.md](../../docs/usd/schemas-geom.md) — UsdGeom schemas
- [schemas-shade.md](../../docs/usd/schemas-shade.md) — UsdShade schemas
- [schemas-lux.md](../../docs/usd/schemas-lux.md) — UsdLux light types
- [sdf-foundations.md](../../docs/usd/sdf-foundations.md) — SdfLayer, SdfPath, PrimSpec
- [datatypes.md](../../docs/usd/datatypes.md) — All USD types with Rust equivalents
- [toolset.md](../../docs/usd/toolset.md) — usdcat, usdview, usdchecker, etc.
- [preview-surface.md](../../docs/usd/preview-surface.md) — UsdPreviewSurface spec
- [faq.md](../../docs/usd/faq.md) — Common USD gotchas
- [llm-reference.md](../../docs/usd/llm-reference.md) — LLM-optimized reference

## Key Concepts

- [[composition-arcs|Composition Arcs]] — The LIVRPS precedence rule
- [[primvars|Primvars]] — Primitive variables for shading
- [[point-instancer|Point Instancer]] — Efficient instancing
- [[edit-target|Edit Target]] — Layer-directed editing

## See Also

- [[architecture/adr/003-hybrid-usd-workflow|ADR 003: Hybrid USD Workflow]]
- [[architecture/adr/004-cpp-bridge-for-usd|ADR 004: C++ Bridge for USD]]
