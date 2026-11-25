---
title: "OpenPBR Surface"
type: concept
tags: [materials, pbr, aswf, rendering]
created: "2026-04-05"
updated: "2026-04-05"
---

## Summary

OpenPBR Surface is the Academy Software Foundation's (ASWF) physically-based material model, designed as a vendor-neutral shading standard for VFX and animation. Version 1.1 defines a layered material with base, specular, coat, transmission, subsurface, thin-film, emission, and geometry layers. It replaces Disney's "principled" parameterization with more physically-grounded controls, notably using IOR-based Fresnel instead of abstract specular parameters.

## Details

### Key Differences from Disney BSDF

| Aspect | Disney | OpenPBR |
|--------|--------|---------|
| Specular control | Abstract 0-1 "specular" param | Physical IOR (default 1.5) |
| Fresnel | Schlick approximation with artist knob | IOR-derived F0, Schlick or full Fresnel |
| Coat | Single clearcoat layer | Dedicated coat with IOR, roughness, color |
| Subsurface | Coupled to base | Separate subsurface layer with MFP |
| Standard body | Ad hoc | ASWF specification document |

### Layer Stack (top to bottom)

1. **Geometry** -- opacity, thin-walled flag, normal mapping
2. **Coat** -- clearcoat with own IOR and roughness
3. **Emission** -- emissive contribution
4. **Specular** -- dielectric reflection, IOR-driven
5. **Metal** -- metallic reflection using base_color as F0
6. **Transmission** -- glass/liquid refraction
7. **Subsurface** -- diffusion below surface
8. **Base** -- Lambertian/Oren-Nayar diffuse

### MaterialX Representation

OpenPBR is defined as a [[materialx]] node graph (`ND_open_pbr_surface_surfaceshader`), making it portable across any renderer that supports MaterialX.

## In BIF

BIF implements OpenPBR in `crates/bif_renderer/src/openpbr.rs`:

- `OpenPbrSurface` struct with base, specular, coat, transmission, emission, and geometry layers.
- **IOR-based Fresnel**: `specular_ior` (default 1.5) drives dielectric F0 via `(ior-1)^2/(ior+1)^2`. See [[ior-fresnel]].
- **Texture support**: base_color, specular_roughness, base_metalness, normals, geometry_opacity -- including UDIM tile sets.
- **GGX microfacet** distribution + Smith geometry term for specular lobe.
- Coat and transmission layers are stubbed but structurally present.
- Chosen over Disney BSDF per project decision -- see `MEMORY.md` reference `project_openpbr.md`.

## Related

- [[ior-fresnel]] -- the Fresnel model OpenPBR uses
- [[materialx]] -- OpenPBR's interchange format
- [[bvh]] -- acceleration structure that finds surface hits for material evaluation
- [[primvars]] -- texture coordinates and per-vertex data consumed by materials
