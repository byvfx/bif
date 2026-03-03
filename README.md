# BIF - VFX Scene Assembler & Renderer

> Production-grade renderer inspired by Isotropix Clarisse, built in Rust

## Current Status: Milestone 29 In Progress

**USD Export Pipeline** — full import→modify→render→export workflow

- Dual rendering: Vulkan viewport (60 FPS) + Ivar CPU path tracer
- **USD export:** Sublayer composition, xform/keyframe overrides, point instancers
- **Batch render:** EXR sequences with AOVs, USD camera animation
- GPU instancing: 10K+ instances with LOD culling
- USD support: USDA (pure Rust) + USDC/references (C++ bridge)
- **Animation:** Timeline UI, transform + vertex animation, viewport camera sync
- Intel Embree 4: Production-quality ray tracing
- Materials: UsdPreviewSurface + MaterialX standard_surface
- Disney Principled BSDF with NEE/MIS environment lighting
- HDRI environment maps: GPU compute IBL (irradiance + prefiltered + BRDF LUT)
- Textured PBR viewport with per-face materials (GeomSubsets)
- OpenImageIO integration with subprocess .tx conversion (optional)
- **Node graph:** 10 node types (scatter, instance, export, etc.)
- Scene browser + property inspector + undo/redo
- SHARC radiance cache (idTech 8 inspired)
- 274+ tests passing across 6 crates

**Next:** Complete M29 export pipeline

---

## Quick Start

```bash
# Build and run
cargo run --package bif_viewer

# Build with OIIO support (optional, requires vcpkg openimageio)
cargo build --features oiio

# Build with OIDN denoising (optional, requires Intel OIDN)
# Set OIDN_DIR to your OIDN install path, ensure DLLs in PATH
cargo build --features oidn

# Run tests
cargo test

# Load USD scene (needs USD env)
. .\setup_usd_env.ps1
cargo run -p bif_viewer -- --usd assets/lucy/usd/assets/lucy/lucy.usd
```

---

## Features

- **Massive Instancing:** 10K-1M instances via prototype/instance architecture
- **Dual Renderers:**
  - **GPU (Vulkan):** Real-time textured PBR at 60+ FPS
  - **CPU (Ivar):** Production path tracing with Disney BSDF
- **USD Workflow:** Import USDA/USDC scenes from Houdini/Maya
- **Materials:** UsdPreviewSurface + MaterialX standard_surface
- **Textures:** Per-face materials via GeomSubsets, parallel loading
- **OIDN Denoising:** Optional Intel OIDN for final-frame denoising (viewport + batch)
- **OIIO Support:** Optional OpenImageIO with subprocess .tx conversion and mipmaps
- **IBL:** GPU compute environment maps (irradiance, prefiltered, BRDF LUT)
- **Intel Embree 4:** Production two-level BVH ray tracing
- **File References:** `@path.usda@</Prim>` resolved automatically

---

## Architecture

```text
bif/
├── crates/
│   ├── bif_math/       # Math primitives (Vec3, Ray, AABB, Camera, Transform)
│   ├── bif_core/       # Scene graph, USD parser, mesh data, materials, textures
│   ├── bif_viewport/   # GPU viewport (wgpu + Vulkan + egui)
│   ├── bif_renderer/   # CPU path tracer "Ivar" (Embree + Disney BSDF)
│   ├── bif_viewer/     # Application entry point
│   └── bif_maketx/     # Standalone .tx converter (subprocess, OIIO)
├── cpp/
│   ├── usd_bridge/     # C++ FFI bridge to Pixar USD
│   └── oiio_bridge/    # C++ FFI bridge to OpenImageIO (optional)
├── devlog/             # Development session logs
├── legacy/             # Original Go raytracer (reference)
└── renders/            # Render output files
```

---

## Documentation

### Getting Started

- **[Milestones](MILESTONES.md)** - Complete history + roadmap
- **[Session Handoff](SESSION_HANDOFF.md)** - Current status and next steps

### Architecture & Design

- **[Architecture](ARCHITECTURE.md)** - Core principles, design decisions
- **[Houdini Export](HOUDINI_EXPORT.md)** - Best practices for USD export

### Development

- **[Reference](REFERENCE.md)** - Code patterns and best practices
- **[Dev Logs](devlog/)** - Session-by-session history
- **[Claude Instructions](CLAUDE.md)** - AI assistant instructions

---

## Statistics (Milestone 23)

| Metric | Value |
|--------|-------|
| Total LOC | ~12,000 |
| Tests Passing | 274+ |
| Milestones Complete | 23 |
| Node Types | 10 |
| Build Time (dev) | ~5s |
| Runtime FPS | 60+ (VSync) |
| Instances Rendered | 10K+ with LOD |
| Total Triangles | 28M+ |
| Embree BVH Build | 28ms |
| USD Load (921 meshes) | 5s |
| USD Load (220K verts) | 0.37s |

---

## Technology Stack

- **Language:** Rust 1.92+
- **GPU:** wgpu 22.1 (Vulkan/DX12/Metal)
- **Ray Tracing:** Intel Embree 4.4.0
- **Math:** glam 0.29 (SIMD)
- **UI:** egui 0.29 + egui-snarl 0.5 (node graph)
- **USD:** Pixar USD 25.11 (C++ bridge) + pure Rust parser
- **Textures:** OpenImageIO 3.0 (optional, via vcpkg)
- **Format:** USD (USDA/USDC), OBJ (legacy), .tx (tiled mipmapped)

---

## Building from Source

### Prerequisites

```bash
# Rust toolchain (1.86+)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# System dependencies (Windows)
# Visual Studio 2022 with C++ Desktop Development workload

# System dependencies (Linux)
sudo apt-get install cmake pkg-config libssl-dev
```

### Build

```bash
# Clone repository
git clone https://github.com/byvfx/bif.git
cd bif

# Build workspace
cargo build

# Run viewer
cargo run --package bif_viewer

# Run tests
cargo test
```

---

## Usage Examples

### Load and View USD Scene

```bash
# Set up USD environment first
. .\setup_usd_env.ps1

# View Lucy model with MaterialX material
cargo run -p bif_viewer -- --usd assets/lucy/usd/assets/lucy/lucy.usd
```

**Viewport Controls:**

- **Left Mouse:** Orbit camera around target
- **Middle Mouse:** Pan camera and target
- **Scroll Wheel:** Dolly (zoom in/out)
- **WASD + QE:** 6DOF camera movement
- **F:** Frame mesh in viewport

### Toggle Renderers

Use the egui side panel to switch between:

- **Vulkan:** Real-time GPU rendering (60 FPS)
- **Ivar:** CPU path tracer (progressive, Disney BSDF)

---

## Roadmap

See [MILESTONES.md](MILESTONES.md) for complete history and future plans.

### Completed (Milestone 23)

- Math library, wgpu viewport, camera controls
- OBJ/USD loading, GPU instancing, Embree 4
- egui UI, CPU path tracer "Ivar"
- USD C++ bridge (USDC, references)
- Scene browser, property inspector, node graph (10 node types)
- UsdPreviewSurface + MaterialX materials
- Disney Principled BSDF with NEE/MIS
- Textured PBR viewport with GeomSubsets
- OpenImageIO + .tx texture pipeline (subprocess)
- HDRI IBL: GPU compute prefiltering, async loading
- Animation: Timeline UI, transform + vertex animation, batch EXR render
- Scene interactivity: Picking, gizmos, undo/redo, keyframing
- Point instancing + scattering (scatter points, PointInstancer node)
- SHARC radiance cache + Russian roulette path termination
- USD export pipeline (sublayer composition, xform overrides)

### Next Up

- **Milestone 29:** USD Export (in-progress — most phases done)
- **Milestone 26:** Denoising (OIDN) — albedo AOV pipeline + OIDN integration
- **Milestone 25:** Volumes/OpenVDB

### Future

- GPU path tracing + ReSTIR
- Qt 6 UI

---

## Contributing

BIF is in active development.

- no contributions at this time, but feel free to open issues or reach out if you're interested in collaborating!

See [MILESTONES.md](MILESTONES.md) for upcoming work.

---

## License

MIT License - See [LICENSE](LICENSE) for details

---

## Acknowledgments

- Inspired by **Isotropix Clarisse**, **Houdini**, and **Gaffer**
- Built with **Rust**, **wgpu**, **egui**, **glam**, and **USD**

---

**Last Updated:** March 2, 2026
**Status:** M23 Complete, M29 In Progress | USD export pipeline, node graph, SHARC cache
