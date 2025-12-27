# BIF - Scene Assembler & Renderer

Production VFX scene assembler and renderer inspired by Isotropix Clarisse.

**Core Focus:**
- Massive instancing (10K-1M instances)
- USD-compatible workflows
- Dual rendering (GPU viewport + CPU path tracer)
- Non-destructive layer-based editing

## Quick Start

### Prerequisites

```bash
# Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# System dependencies (Ubuntu/Debian)
sudo apt-get install cmake pkg-config libssl-dev python3-dev

# USD (optional, for later phases)
# Pre-built: https://developer.nvidia.com/usd-downloads
# Or via vcpkg: vcpkg install usd
```

### Build & Run

```bash
# Clone repository
git clone https://github.com/yourusername/bif.git
cd bif

# Build Rust workspace
cargo build --release

# Run application
cargo run --release --bin bif
```

## Documentation

- **[ARCHITECTURE.md](docs/ARCHITECTURE.md)** - Design decisions, core principles, material/USD integration
- **[GETTING_STARTED.md](docs/GETTING_STARTED.md)** - Step-by-step implementation guide with milestones
- **[REFERENCE.md](docs/REFERENCE.md)** - Detailed task checklist and code patterns
- **[claude.md](docs/claude.md)** - AI assistant instructions for this project

## Current Status

**Phase:** Porting Go raytracer to Rust foundation

**Completed (Go Implementation):**
- Path tracer with multiple importance sampling
- BVH acceleration
- Materials: Lambertian, Metal, Dielectric, Emissive
- IBL with cosine-weighted sampling
- Next Event Estimation
- Camera with DOF and motion blur

**In Progress (Rust):**
- Core math library (Vec3, Ray, AABB)
- Scene graph with prototype/instance architecture
- egui + wgpu viewport
- CPU path tracer port

## Repository Structure

```
bif/
├── README.md
├── docs/
│   ├── ARCHITECTURE.md      # Design & decisions
│   ├── GETTING_STARTED.md   # Implementation guide
│   ├── REFERENCE.md         # Task checklist
│   └── claude.md            # AI instructions
├── crates/
│   ├── bif_math/           # Vec3, Ray, AABB
│   ├── bif_scene/          # Scene graph, instances, layers
│   ├── bif_renderer/       # CPU path tracer
│   ├── bif_viewport/       # GPU viewport (wgpu)
│   ├── bif_materials/      # Material system
│   ├── bif_io/             # glTF, image loading
│   └── bif_app/            # Main application (egui UI)
├── cpp/                    # C++ components (USD/MaterialX, later)
│   └── usd_bridge/
└── assets/                 # Test scenes, HDRIs
```

## UI Development Path

**PoC Phase (Current):** egui + wgpu
- Pure Rust, fast iteration
- Validate workflow and architecture
- Good enough for development

**Production Phase (Future):** Qt 6
- Professional UI framework
- Industry-standard features
- Docking, menus, shortcuts

See [ARCHITECTURE.md](docs/ARCHITECTURE.md) for details.

## Contributing

Project is in early development. Contributions welcome in:
- Rust performance optimization
- USD/MaterialX integration
- GPU rendering (wgpu compute)
- Qt UI development (future)

## License

MIT

## Acknowledgments

- Inspired by Isotropix Clarisse
- Based on "Ray Tracing in One Weekend" series
- Built with Rust, wgpu, USD, MaterialX