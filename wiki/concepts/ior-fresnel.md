---
title: "IOR-Based Fresnel"
type: concept
tags: [rendering, materials, pbr, physics]
created: "2026-04-05"
updated: "2026-04-05"
---

## Summary

Fresnel equations describe how much light is reflected vs refracted at a surface boundary, as a function of the viewing angle and the indices of refraction (IOR) of the two media. In PBR rendering, IOR-based Fresnel replaces Disney's abstract "specular" parameter with a physically meaningful IOR value, giving artists a single number (e.g., 1.5 for glass) that correctly determines both normal-incidence reflectance (F0) and the angle-dependent reflection curve.

## Details

### The Physics

When light hits a boundary between two media with IOR n1 and n2:

- **F0** (reflectance at normal incidence) = `((n1 - n2) / (n1 + n2))^2`
- **F90** (reflectance at grazing angle) approaches 1.0 for all dielectrics.
- The transition from F0 to F90 follows the Fresnel equations (or approximations thereof).

### Common IOR Values

| Material | IOR | F0 |
|----------|-----|-----|
| Air | 1.0 | -- |
| Water | 1.33 | 0.02 |
| Glass/Plastic | 1.5 | 0.04 |
| Diamond | 2.42 | 0.17 |
| Metals | Complex IOR | 0.5-1.0 |

### Schlick Approximation

Full Fresnel is expensive. Schlick's approximation is standard in real-time and offline PBR:

```text
F(theta) = F0 + (1 - F0) * (1 - cos(theta))^5
```

This is accurate for dielectrics. For metals, F0 = base_color (the metal's reflectance spectrum) and the Schlick curve still applies.

### IOR vs Disney Specular

Disney's original "specular" parameter remaps IOR through `ior = 2 / (1 - sqrt(0.08 * specular)) - 1`, defaulting to specular=0.5 which gives IOR=1.5. [[openpbr]] skips this indirection and exposes IOR directly, which is more intuitive for technical artists and physically unambiguous.

## In BIF

IOR-based Fresnel is implemented in the renderer:

- **OpenPbrSurface** (`crates/bif_renderer/src/openpbr.rs`): `specular_ior` field (default 1.5) drives F0 calculation.
- **F0 derivation**: `f0 = ((ior - 1) / (ior + 1))^2` computed at material setup time.
- **Schlick evaluation**: Used per-hit in the specular lobe sampling path.
- **Metals**: When `base_metalness = 1.0`, F0 = base_color (complex IOR is approximated by treating color as reflectance).
- **Material module**: `crates/bif_renderer/src/material.rs` contains the `reflect`, `refract`, and Fresnel utility functions.
- **Filter**: `crates/bif_renderer/src/filter.rs` also references Fresnel for denoising/filtering weights.

## Related

- [[openpbr]] -- the material model that uses IOR-based Fresnel
- [[materialx]] -- Fresnel behavior encoded in MaterialX standard PBR nodes
- [[bvh]] -- intersection testing that determines the hit point where Fresnel is evaluated
