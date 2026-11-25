---
title: OpenPBR Surface
type: article
tags: [rendering, materials]
created: "2026-04-05"
updated: "2026-04-05"
sources: [../../FEATURES.md]
---

# OpenPBR Surface

BIF uses OpenPBR Surface v1.1 as its material model — a physically-based shading model that supersedes Disney BSDF with better energy conservation and IOR-based Fresnel.

## Why OpenPBR (Not Disney)

See [[architecture/adr/001-openpbr-over-disney|ADR 001]] for full rationale. Key reasons:

- Industry-standard (MaterialX native)
- IOR-based Fresnel is more physically correct than F0 parameterization
- Better energy conservation
- Direct MaterialX export path

## Key Parameters

OpenPBR Surface v1.1 parameters implemented in BIF:

- **Base:** color, weight, roughness, metalness
- **Specular:** weight, color, roughness, IOR, anisotropy
- **Transmission:** weight, color, depth
- **Subsurface:** weight, color, radius, scale
- **Coat:** weight, color, roughness, IOR
- **Emission:** color, luminance
- **Thin film:** thickness, IOR
- **Geometry:** opacity, thin-walled

## IOR-Based Fresnel

Unlike Disney BSDF (which uses an F0 reflectance parameter), OpenPBR derives Fresnel from physical index of refraction:

- Dielectrics: IOR typically 1.0-2.5 (glass ~1.5, water ~1.33, diamond ~2.42)
- Conductors: complex IOR (n + ik), handled via metalness parameter
- More physically accurate at grazing angles

See [[ior-fresnel|IOR Fresnel]] for the math.

## Implementation in BIF

- `OpenPbrSurface` struct in `bif_renderer`
- Maps to UsdPreviewSurface for USD export (with approximations)
- Maps to MaterialX OpenPBR for MaterialX export (direct)
- Renderer evaluates in fragment shader via wgpu pipeline

## Related

- [[ior-fresnel|IOR Fresnel]] — Physics of IOR-based reflectance
- [[materialx-bridge|MaterialX Bridge]] — Material export pipeline
- [[openpbr|OpenPBR]] — Concept note
- [[wgpu-pipeline|wgpu Pipeline]] — Where materials get evaluated
