---
title: "MaterialX"
type: concept
tags: [materials, interchange, aswf, shading]
created: "2026-04-05"
updated: "2026-04-05"
---

## Summary

MaterialX is an open standard (ASWF) for representing materials, looks, and patterns as node graphs in XML. It provides a renderer-agnostic way to describe shading networks that can be compiled to GLSL, OSL, MDL, or any target. It serves as the interchange format between DCCs, look-dev tools, and renderers -- the "USD of materials."

## Details

### Core Abstractions

- **Document**: Root container (`.mtlx` file) holding node definitions and graphs.
- **NodeDef**: Declares a node's interface (inputs, outputs, types).
- **NodeGraph**: A directed acyclic graph of connected nodes.
- **Node**: An instance of a NodeDef with bound input values.
- **Input/Output**: Typed ports with values or connections.
- **GeomInfo**: Binds geometric properties (primvars) to material inputs.

### Standard Library

MaterialX ships a standard library with hundreds of nodes:

- **Math**: add, multiply, mix, remap, clamp, dot, cross
- **Texture**: image, tiledimage, triplanar
- **Procedural**: noise, fractal, checkerboard, worley
- **PBR**: conductor_bsdf, dielectric_bsdf, oren_nayar_diffuse, sheen
- **Surface models**: `ND_standard_surface`, `ND_open_pbr_surface_surfaceshader`

### Relationship to USD

- UsdShade can reference MaterialX node graphs via `info:id` on Shader prims.
- Hydra Storm and other delegates can consume MaterialX via code generation.
- The `UsdMtlx` plugin reads `.mtlx` files as USD layers.

## In BIF

- **Export bridge**: `bif_core::usd::export` writes MaterialX-compatible material descriptions. The C++ bridge (`cpp_bridge`) and FFI layers handle MaterialX shader encoding.
- **OpenPBR as MaterialX**: BIF's [[openpbr]] material model maps directly to the `ND_open_pbr_surface_surfaceshader` MaterialX node definition.
- **CPU fallback**: `bif_renderer` implements a CPU-side MaterialX bridge fallback for vertex displacement when GPU codegen is not available.
- **Future**: Full MaterialX node graph editing is planned for the material editor (see `docs/ux/MATERIAL_EDITOR_DESIGN.md`).

## Related

- [[openpbr]] -- the material model defined as a MaterialX graph
- [[composition-arcs]] -- materials can be referenced across USD assets
- [[primvars]] -- geometric data that feeds into MaterialX texture lookups
- [[edit-target]] -- layer where material opinions are authored
