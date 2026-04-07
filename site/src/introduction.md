# BIF

> USD Orchestration Tool for VFX. Compose layers. Instance massively. Render.

---

BIF is a USD orchestration tool for VFX — open a USD stage, browse the layer stack, pick your working layer, scatter and instance geometry, override materials, and render production-quality images. All edits author clean USD opinions directly.

Built from scratch in Rust with a USD-native pipeline. Inspired by Katana's layer awareness and Houdini's procedural power, in one tool.

**BIF is not** a general-purpose 3D package. It doesn't model, rig, or animate. It orchestrates scenes authored in Houdini, Maya, or Blender — composing, overriding, instancing, and rendering them.

## Features

| Feature | Description |
|---------|-------------|
| **USD-Native** | Full C++ bridge — USDA/USDC, references, payloads, deferred loading |
| **Massive Instancing** | 10K-1M instances with GPU LOD culling |
| **Dual Rendering** | Vulkan viewport (60 FPS) + Ivar CPU path tracer (OpenPBR Surface v1.1) |
| **Node Graph** | 10 node types — scatter, instance, export, cache, graft |
| **Materials** | OpenPBR + UsdPreviewSurface + MaterialX import |
| **Batch Render** | EXR sequences with AOVs, camera animation, OIDN denoising |
| **Persistence** | .bif/.bifa project files, auto/manual eval modes, cache nodes |
| **Embree 4** | Production ray tracing with subdivision surfaces |
| **HDRI Lighting** | GPU-computed IBL (irradiance + prefiltered + BRDF LUT) |
| **USD Export** | Round-trip with dual material export (UsdPreviewSurface + OpenPBR MaterialX) |

## Stats

| Metric | Value |
|--------|-------|
| Rust LOC | ~50,000 |
| Tests | 400+ |
| Node types | 10 |
| Viewport FPS | 60+ (VSync) |
| Max instances | 1M+ with LOD |

## Tech Stack

**Rust** · wgpu 22 (Vulkan/DX12/Metal) · egui 0.29 · Intel Embree 4 · Pixar USD 25.11 (C++) · glam (SIMD) · OpenImageIO (optional) · Intel OIDN (optional)

## Site Navigation

- **[Getting Started](reference/getting-started.md)** — Build, install, and run BIF
- **[Architecture](reference/architecture.md)** — Crate structure and design
- **[USD Reference](reference/usd/concepts.md)** — Curated USD documentation
- **[Changelog](reference/changelog.md)** — Release history
- **[Dev Diary](devlog/)** — Session-by-session development logs

## Links

- [GitHub Repository](https://github.com/byvfx/bif)
- [Roadmap](https://github.com/byvfx/bif/blob/main/MILESTONES.md)
- [License (MIT)](https://github.com/byvfx/bif/blob/main/LICENSE)
