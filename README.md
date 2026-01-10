# BIF - VFX Scene Assembler & Renderer

> Production-grade renderer inspired by Isotropix Clarisse, built in Rust

## 🎯 Current Status: Milestones 0-13b Complete! ✅

**Foundation Complete** - Scene Browser + Node Graph DONE

- ✅ Dual rendering: Vulkan viewport (60 FPS) + Ivar CPU path tracer
- ✅ GPU instancing: 100+ instances, single draw call
- ✅ USD support: USDA (pure Rust) + USDC/references (C++ bridge)
- ✅ Intel Embree 4: Production-quality ray tracing
- ✅ Node graph + scene browser + property inspector
- ✅ 60+ tests passing across 4 crates

**Next:** [Milestone 14](MILESTONES.md#milestone-14-materials-usdpreviewsurface--materialx-) (Materials/MaterialX)

---

## Quick Start

```bash
# Build and run
cargo run --package bif_viewer

# Run tests
cargo test

# Load USD scene
cargo run -p bif_viewer -- --usda assets/lucy_low.usda

# For USD C++ features (USDC, references), set up environment:
# See USD_SETUP.md for details
```

---

## Features

- **Massive Instancing:** 10K-1M instances via prototype/instance architecture
- **Dual Renderers:**
  - **GPU (Vulkan):** Real-time preview at 60+ FPS
  - **CPU (Ivar):** Production path tracing with progressive refinement
- **USD Workflow:** Import USDA/USDC scenes from Houdini/Maya
- **Intel Embree 4:** Production two-level BVH ray tracing
- **File References:** `@path.usda@</Prim>` resolved automatically

---

## Architecture

```text
bif/
├── crates/
│   ├── bif_math/       # Math primitives (Vec3, Ray, AABB, Camera, Transform)
│   ├── bif_core/       # Scene graph, USD parser, mesh data
│   ├── bif_viewport/   # GPU viewport (wgpu + Vulkan + egui)
│   ├── bif_renderer/   # CPU path tracer "Ivar" (Embree + progressive rendering)
│   └── bif_viewer/     # Application entry point
├── cpp/usd_bridge/     # C++ FFI bridge to Pixar USD
├── devlog/             # Development session logs
├── legacy/             # Original Go raytracer (reference)
├── docs/archive/       # Archived documentation
└── renders/            # Render output files
```

---

## Documentation

### Getting Started

- **[Milestones](MILESTONES.md)** - Complete milestone history with actual results + future roadmap
- **[Getting Started Guide](GETTING_STARTED.md)** - Milestone-by-milestone implementation guide
- **[Session Handoff](SESSION_HANDOFF.md)** - Current status and next steps

### Architecture & Design

- **[Architecture](ARCHITECTURE.md)** - Core principles, design decisions, rendering pipeline
- **[Houdini Export](HOUDINI_EXPORT.md)** - Best practices for USD export

### Development

- **[Reference](REFERENCE.md)** - Code patterns and best practices
- **[Dev Logs](devlog/)** - Session-by-session development history
- **[Claude Instructions](CLAUDE.md)** - AI assistant custom instructions

---

## Statistics (Milestones 0-13b)

| Metric | Value |
|--------|-------|
| Total LOC | ~7,500 |
| Tests Passing | 60+ ✅ |
| Milestones Complete | 13b + Freeze Fix |
| Build Time (dev) | ~5s |
| Runtime FPS | 60+ (VSync) |
| Instances Rendered | 100 (scalable to 10K+) |
| Total Triangles | 28M+ |
| Embree BVH Build | 28ms |

---

## Technology Stack

- **Language:** Rust 1.92+
- **GPU:** wgpu 22.1 (Vulkan/DX12/Metal)
- **Ray Tracing:** Intel Embree 4.4.0
- **Math:** glam 0.29 (SIMD)
- **UI:** egui 0.29 + egui-snarl 0.5 (node graph)
- **USD:** Pixar USD 25.11 (C++ bridge) + pure Rust parser
- **Format:** USD (USDA/USDC), OBJ (legacy)

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
# View Lucy model (140K vertices, 100 instances)
cargo run -p bif_viewer -- --usda assets/lucy_low.usda
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
- **Ivar:** CPU path tracer (progressive)

---

## Roadmap

See [MILESTONES.md](MILESTONES.md) for complete milestone history and future plans.

### ✅ Completed (Milestones 0-13b)

- Math library, wgpu viewport, camera controls
- OBJ/USD loading, GPU instancing, Embree 4
- egui UI, CPU path tracer "Ivar"
- USD C++ bridge (USDC, references)
- Scene browser, property inspector, node graph

### 🎯 Next Up

- **Milestone 14:** Materials (UsdPreviewSurface + MaterialX)
- **Milestone 15:** Animation + Motion Blur
- **Milestone 16:** Frame Rendering

### 🔮 Future

- Point instancing + scattering (M18)
- Viewport performance (M19)
- GPU path tracing + ReSTIR (M24)
- Qt 6 UI (M25+)

---

## Contributing

BIF is in active development. Contributions welcome in:

- Rust performance optimization
- Embree integration
- USD/MaterialX workflows
- Testing and documentation

See [MILESTONES.md](MILESTONES.md) for upcoming work.

---

## License

MIT License - See [LICENSE](LICENSE) for details

---

## Acknowledgments

- Inspired by **Isotropix Clarisse**, **Houdini**, and **Gaffer** (VFX scene assembly workflow)
- Built with **Rust**, **wgpu**, **egui**, **glam**, and **USD**

---

**Last Updated:** January 9, 2026
**Status:** Milestones 0-13b Complete ✅ | Next: Milestone 14 (GPU Instancing Optimization)