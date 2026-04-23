# BIF

> USD Orchestration Tool for VFX. Compose layers. Instance massively. Render.

<!-- TODO: add hero render image -->
<!-- ![BIF Render](renders/hero.png) -->

---

## What is BIF?

BIF is a USD orchestration tool for VFX — open a USD stage, browse the layer stack, pick your working layer, scatter and instance geometry, override materials, and render production-quality images. All edits author clean USD opinions directly.

Built from scratch in Rust with a USD-native pipeline. Inspired by Katana's layer awareness and Houdini's procedural power, in one tool.

**BIF is not** a general-purpose 3D package. It doesn't model, rig, or animate. It orchestrates scenes authored in Houdini, Maya, or Blender — composing, overriding, instancing, and rendering them.

---

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

---

## Quick Start

```bash
# Build and run
cargo run -p bif_viewer

# With USD scene (needs USD env)
. .\setup_usd_env.ps1
cargo run -p bif_viewer

# Optional features
cargo build --features oidn    # Intel OIDN denoising
cargo build --features oiio    # OpenImageIO .tx conversion
cargo run -p bif_viewer --features oiio,oidn  # Viewer with OIIO + OIDN
```

### Viewport Controls

| Input | Action |
|-------|--------|
| Left drag | Orbit |
| Middle drag | Pan |
| Scroll | Dolly |
| WASD + QE | Fly |
| F | Frame selection |

---

## Architecture

```text
bif/
├── crates/
│   ├── bif_math/       # Vec3, Ray, AABB, Camera, Transform
│   ├── bif_core/       # Scene graph, USD bridge, materials, textures
│   ├── bif_renderer/   # "Ivar" CPU path tracer (Embree, OpenPBR)
│   ├── bif_viewport/   # GPU viewport (wgpu/Vulkan, egui)
│   ├── bif_viewer/     # Application shell
│   └── bif_maketx/     # Standalone .tx converter
├── cpp/usd_bridge/     # C++ FFI to Pixar USD
└── benchmarks/         # bif_perf performance harness
```

---

## Stats

| Metric | Value |
|--------|-------|
| Rust LOC | ~50,000 |
| Tests | 400+ |
| Node types | 10 |
| Viewport FPS | 60+ (VSync) |
| Max instances | 1M+ with LOD |
| Embree BVH build | 28ms |

---

## Tech Stack

**Rust** · wgpu 22 (Vulkan/DX12/Metal) · egui 0.29 · Intel Embree 4 · Pixar USD 25.11 (C++) · glam (SIMD) · OpenImageIO (optional) · Intel OIDN (optional)

---

## Performance Benchmarking (bif_perf)

Modular USD performance metrics, best-practice auditing, and cross-tool comparison.

```bash
. .\setup_usd_env.ps1

cargo run -p bif_perf -- list                          # show metrics, scenes, targets
cargo run -p bif_perf -- run all -n 10                 # quick test, all local scenes
cargo run -p bif_perf -- run medium -n 50 --save       # save YAML to benchmarks/results/
cargo run -p bif_perf -- run assets/scene.usdc -n 100  # specific file
cargo run -p bif_perf -- run all --metrics stage_open,full_load -f yaml
cargo run -p bif_perf -- audit assets/scene.usdc       # maxperf.html best-practice checks
cargo run -p bif_perf -- compare baseline.yaml current.yaml  # regression detection
cargo run -p bif_perf -- download                      # show missing official USD assets
```

**Flags:** `-n` iterations, `-w` warmup, `-t` target (bif/usdview/houdini), `-f` format (terminal/yaml/csv), `-o` file, `--save`, `-m` metric filter

**Custom scenes:** Edit `benchmarks/src/scenes/mod.rs` or pass any USD path directly.

---

## Roadmap

See **[MILESTONES.md](MILESTONES.md)** for the full version-organized roadmap.

| Next | Theme |
|------|-------|
| v0.13.0 | Pipeline foundation *(in progress)* |
| v0.14.0 | USD debugging tools |
| v0.15.0 | Qt 6 migration |
| v0.16.0 | Viewport performance |
| ... | [Full roadmap](MILESTONES.md) |

---

## Building from Source

### Prerequisites

- Rust 1.86+ ([rustup](https://rustup.rs/))
- Visual Studio 2022 C++ workload (Windows) or cmake + pkg-config (Linux)
- Pixar USD 25.11 (for USD features)

```bash
git clone https://github.com/byvfx/bif.git
cd bif
cargo build
cargo test
```

See **[CLAUDE.md](CLAUDE.md)** for detailed build instructions and feature flags.

---

## Documentation

| Doc | Description |
|-----|-------------|
| [MILESTONES.md](MILESTONES.md) | Version roadmap |
| [ROADMAP_DETAIL.md](ROADMAP_DETAIL.md) | Detailed task breakdowns per version |
| [MILESTONES_HISTORY.md](MILESTONES_HISTORY.md) | Completed milestone history |
| [CHANGELOG.md](CHANGELOG.md) | Release notes |
| [ARCHITECTURE.md](ARCHITECTURE.md) | Design decisions |
| [HOUDINI_EXPORT.md](HOUDINI_EXPORT.md) | USD export best practices |
| [devlog/](devlog/) | Session-by-session development logs |

---

## Contributing

BIF is in active development. No contributions at this time, but feel free to open issues.

---

## License

MIT — See [LICENSE](LICENSE)

---

## Acknowledgments

Inspired by **Isotropix Clarisse**, **Foundry Katana**, and **Image Engine Gaffer**. Built with Rust, wgpu, egui, USD, and Embree.
