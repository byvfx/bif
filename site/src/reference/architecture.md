# Architecture

## Crate Structure

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

### bif_math

Foundation crate. Vec3, Ray, AABB, Camera, Transform types. 41 tests, no external dependencies beyond glam (SIMD math).

### bif_core

Scene graph, USD C++ bridge (via `cpp/usd_bridge/`), material system, texture loading. Contains `CompositeProvider` which merges USD stage data with procedural prims via `CachedSceneGraph`. USD export pipeline lives in `usd/export.rs`.

The C++ bridge builds via CMake (`build.rs` triggers it) and requires Visual Studio 2022.

### bif_renderer

"Ivar" — CPU path tracer using Intel Embree 4 for BVH acceleration. Implements OpenPBR Surface v1.1 materials with IOR-based Fresnel. Supports EXR output with AOVs, camera animation, and optional OIDN denoising.

### bif_viewport

Real-time GPU viewport using wgpu (Vulkan/DX12/Metal). 60+ FPS with GPU LOD culling for massive instancing (1M+ instances). egui integration for UI panels.

### bif_viewer

Application shell. Wires together viewport, renderer, node graph (egui-snarl), scene browser, and property inspector.

### bif_maketx

Standalone texture converter. Converts images to tiled, mipmapped .tx format for efficient rendering.

## Node Graph

10 node types built on egui-snarl:

| Node | Purpose |
|------|---------|
| UsdRead | Load USD stage |
| Primitive | Procedural geometry |
| Scatter | Distribute instances |
| PointInstancer | USD-native instancing |
| Xform | Transform operations |
| UsdExport | Write USD files |
| UsdPrim | USD prim operations |
| GraftBranches | Merge scene branches |
| HdriEnvironment | IBL lighting |
| IvarRender | CPU path trace render |

## Materials

- **OpenPBR Surface v1.1** — primary material model for Ivar renderer
- **UsdPreviewSurface** — viewport preview materials
- **MaterialX** — import support, dual export (OpenPBR + UsdPreviewSurface)

## Roadmap

| Version | Theme | Status |
|---------|-------|--------|
| v0.12.0 | USD Export | Released |
| v0.13.0 | Pipeline Foundation | In progress |
| v0.14.0 | Layer-Aware Stage | Planned |
| v0.15.0 | Qt Migration | Planned |

See [MILESTONES.md](https://github.com/byvfx/bif/blob/main/MILESTONES.md) for the full roadmap.
