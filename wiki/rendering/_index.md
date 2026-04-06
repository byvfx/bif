---
title: Rendering Index
type: index
updated: "2026-04-05"
---

# Rendering

BIF's rendering pipeline — wgpu-based with OpenPBR materials and MaterialX support.

## Articles

- [[openpbr-surface|OpenPBR Surface]] — BIF's material model (OpenPBR v1.1, IOR-based Fresnel)
- [[wgpu-pipeline|wgpu Pipeline]] — GPU rendering architecture
- [[materialx-bridge|MaterialX Bridge]] — Material exchange and CPU vertex displacement fallback

## Key Facts

- Renderer is a God object (~75 fields) — cleanup deferred
- Uses wgpu (Rust WebGPU implementation)
- OpenPBR Surface v1.1 for physically-based materials
- MaterialX for material interchange
- Optional OIDN denoising (feature flag)
- Optional OIIO texture handling (feature flag)

## See Also

- [[architecture/adr/001-openpbr-over-disney|ADR 001: OpenPBR Over Disney]]
- [[concepts/_index|Concepts]] — BVH, IOR Fresnel, OpenPBR, MaterialX
