# Architecture

## Crate Structure

```text
bif/
├── crates/
│   ├── bif_math/       # Vec3, Ray, AABB, Camera, Transform
│   ├── bif_core/       # Scene graph, USD bridge, materials, textures
│   ├── bif_renderer/   # "Ivar" CPU path tracer (Embree, OpenPBR)
│   ├── bif_viewport/   # GPU viewport + interaction engine
│   ├── bif_viewer/     # Thin runner that boots bif_qt
│   ├── bif_maketx/     # Standalone .tx converter
│   └── bif_qt/         # Qt 6 shell (cxx-qt, docks, panels)
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

Real-time GPU viewport + interaction engine using wgpu (Vulkan/DX12/Metal). 60+ FPS with GPU LOD culling for massive instancing (1M+ instances). Owns the `Renderer`, the procedural node-graph data model, and the scene-browser providers. UI rendering lives in `bif_qt`, not here.

### bif_viewer

Thin entry point (`main.rs` only) — boots the `bif_qt` shell. No UI logic lives here.

### bif_qt

Qt 6 shell via cxx-qt. All panels (scene browser, layer stack, property inspector, node graph, render settings, USDA source, command palette) live in `crates/bif_qt/cpp/`; the Rust–C++ bridge and main window in `crates/bif_qt/src/`. (Replaced the egui UI in the v0.15.0 Qt migration.)

### bif_maketx

Standalone texture converter. Converts images to tiled, mipmapped .tx format for efficient rendering.

## Node Graph

10 node types. The graph data model uses `egui-snarl`; the editor itself is the Qt node-graph widget (`bif_qt/cpp/node_graph_widget`):

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
| v0.14.0 | Layer-Aware Stage | Released |
| v0.15.0 | Qt Migration | Released |
| v0.16.0 | Edit Operations + Save | Released |
| v0.16.8 | Crash hardening + keybinding editor | Latest |
| v0.17.0 | Context System | Next |
| v0.18.0 | Viewport Performance | Planned |

See [MILESTONES.md](https://github.com/byvfx/bif/blob/main/MILESTONES.md) for the full roadmap.
