---
title: wgpu Pipeline
type: article
tags: [rendering, wgpu]
created: "2026-04-05"
updated: "2026-04-05"
sources: [../../ARCHITECTURE.md, ../../FEATURES.md]
---

# wgpu Rendering Pipeline

BIF's GPU rendering is built on wgpu — the Rust implementation of the WebGPU standard. The pipeline handles scene rendering, material evaluation, and viewport display.

## Architecture

The renderer lives in the `bif_renderer` crate:

- **Renderer struct** (~75 fields — acknowledged God object, cleanup deferred)
- Handles pipeline creation, buffer management, draw calls
- Evaluates OpenPBR materials in shaders

### Pipeline Stages

1. **Scene traversal** — Walk evaluated node graph, collect renderable prims
2. **Buffer upload** — Vertex/index/instance data to GPU
3. **Render pass** — Draw geometry with material shaders
4. **Post-processing** — Optional OIDN denoising

## Key Features

- Physically-based rendering with OpenPBR materials
- Point instancer support (efficient GPU instancing)
- HDRI environment lighting
- Selection outlines (geometry ID pass)
- CPU vertex displacement (MaterialX fallback)
- Optional Intel OIDN denoising (`--features oidn`)
- Optional OpenImageIO texture handling (`--features oiio`)

## Viewport Integration

The `bif_viewport` crate manages:

- Camera controls (orbit, pan, zoom)
- Viewport overlays (grid, selection)
- Render result display in egui

## Known Issues

- Renderer is a God object — needs subsystem extraction (ongoing)
- `test_should_restart_no_render` is timing-sensitive (flaky)

## Related

- [[openpbr-surface|OpenPBR Surface]] — Material model
- [[materialx-bridge|MaterialX Bridge]] — CPU displacement fallback
- [[wgpu]] — The graphics API
- [[bvh|BVH]] — Acceleration structure for ray queries
- [[crate-structure|Crate Structure]] — bif_renderer and bif_viewport
