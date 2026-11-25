---
title: "ADR-001: OpenPBR Over Disney BSDF"
type: adr
tags: [architecture, materials]
created: "2026-04-05"
updated: "2026-04-05"
sources: [ARCHITECTURE.md, MILESTONES.md, memory/project_openpbr.md]
---

# ADR-001: OpenPBR Over Disney BSDF

## Context

BIF needed a physically-based shading model for its CPU path tracer (Ivar) and for MaterialX material export. The two leading candidates were:

- **Disney Principled BSDF** — the de facto standard since 2012, used by Blender Cycles, Renderman, Arnold
- **OpenPBR Surface** — the ASWF (Academy Software Foundation) successor, designed for MaterialX interoperability

BIF had already implemented UsdPreviewSurface (M15) and MaterialX standard_surface import (M16). The v0.12.0 release needed to commit to a primary shading model for the internal renderer and MaterialX export path.

## Decision

**Use OpenPBR Surface v1.1 as BIF's primary shading model.** Disney BSDF support exists for backward compatibility but OpenPBR is the native model.

The `OpenPbrSurface` struct in bif_renderer implements IOR-based Fresnel and maps directly to MaterialX's `open_pbr_surface` node type.

## Alternatives Considered

| Option | Pros | Cons | Verdict |
|--------|------|------|---------|
| **OpenPBR** | ASWF standard, MaterialX-native, forward-looking, IOR-based (physically correct) | Newer, less tool support today | **Chosen** |
| **Disney Principled** | Ubiquitous, well-documented, proven in production | Legacy model, less direct MaterialX mapping, being superseded | Rejected as primary |
| **Both equally** | Maximum compatibility | Doubles maintenance, confuses material identity | Rejected |

## Consequences

**Positive:**

- MaterialX export uses `open_pbr_surface` node directly — no lossy conversion
- IOR-based Fresnel is more physically correct than Disney's specular model
- BIF aligns with the ASWF direction (future-proof)
- OpenPBR parameter names map cleanly to industry terminology

**Negative:**

- Some existing USD assets use Disney/standard_surface — BIF must still import these and convert
- OpenPBR has less documentation and fewer community examples than Disney
- Auto-conversion from OpenPBR to UsdPreviewSurface is lossy for transmission, subsurface, and fuzz parameters

**Neutral:**

- UsdPreviewSurface remains the export format for universal USD compatibility
- A shading model dropdown in the material editor (v0.16.0) lets artists switch between OpenPBR, UsdPreviewSurface, and MaterialX standard_surface with auto-conversion warnings

## Related

- [[material-editor|Material Editor]] — Shading model dropdown and auto-conversion table
- [[crate-structure|Crate Structure]] — OpenPbrSurface lives in bif_renderer
