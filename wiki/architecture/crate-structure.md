---
title: "Crate Structure"
type: article
tags: [architecture]
created: "2026-04-05"
updated: "2026-04-05"
sources: [ARCHITECTURE.md, ARCHITECTURE_REVIEW.md, ARCHITECTURE_REFACTORS.md]
---

# Crate Structure

BIF is organized as a Cargo workspace with 6 crates. Dependencies flow strictly upward — no circular dependencies. This layering is the most important architectural invariant to preserve.

## Dependency Graph

```text
bif_math (leaf — zero internal deps)
    ^
bif_core (depends on bif_math)
    ^
bif_renderer (depends on bif_core + bif_math)
    ^
bif_viewport (depends on bif_core + bif_renderer + bif_math)
    ^
bif_viewer (depends on bif_viewport + bif_core + bif_math)

bif_maketx (standalone CLI tool)
```

## Crate Details

### bif_math (~2,000 LOC, 74 tests)

Pure math primitives with zero internal dependencies. The leaf crate.

- `Vec3`, `Ray`, `Aabb`, `Camera`, `Transform`, `Frustum`
- Extension trait `Mat4Ext` on glam::Mat4
- Excellent test coverage — all pure functions

### bif_core (~13,900 LOC, 163 tests)

Scene graph, USD bridge, mesh data, textures, undo system. The domain model.

- `Scene` struct — root container (prototypes, instances, materials, lights, cameras)
- `EditState` + `UndoStack` — non-destructive editing with undo/redo
- `usd/cpp_bridge.rs` — Rust wrapper around C++ USD FFI (split into ffi_raw.rs, ffi_convert.rs, cpp_bridge.rs after Phase 1 refactor)
- `usd/export.rs` — `export_scene()` pipeline for writing USD files
- Textures, scatter algorithms, procedural primitives, point clouds
- Error handling: typed errors via `thiserror` (`UsdBridgeError`, `TextureError`, `LoadError`, etc.)
- **Tests require** `setup_usd_env.ps1` and `--test-threads=1` (USD C++ not thread-safe)

### bif_renderer (~10,000 LOC, 111 tests)

CPU path tracer "Ivar" with Embree acceleration.

- Embree 4 integration for two-level BVH (instance-aware)
- `Material` trait — BRDF scattering interface (Lambertian, Metal, Dielectric, Emissive, OpenPBR)
- `Hittable` trait — ray intersection abstraction
- `Light` trait — light sampling interface
- SHARC radiance cache, Russian roulette path termination
- Bucket rendering via Rayon parallelism
- Progressive refinement, multiple importance sampling, NEE
- Ray-based object picking (`pick_scene.rs`)
- Optional features: `oidn` (Intel denoising), `oiio` (OpenImageIO)
- Error handling: `thiserror` for `EmbreeError`, `ExrError`, `PickError`

### bif_viewport (~14,000+ LOC, 149 tests)

GPU viewport (wgpu), node graph, scene browser, and all UI integration.

- `Renderer` struct — the central coordinator (~75-99 fields, known God object, decomposition planned)
- wgpu render pipeline: instanced rendering, PBR shading, frustum culling, multi-draw
- `node_graph/` — egui-snarl based node graph (see [[node-graph-system|Node Graph System]])
- `scene_browser.rs` — CompositeProvider + CachedSceneGraph (see [[scene-browser|Scene Browser]])
- `scene_loader.rs` — USD loading orchestration (2,350 LOC, 0 tests — GPU-dependent)
- `ivar_build.rs` / `ivar_state.rs` / `ivar_renderer.rs` — Ivar integration
- `property_inspector.rs` — property editing UI
- Error handling: `anyhow::Result` (application-level)

### bif_viewer (~800 LOC, 19 tests)

Thin application shell. Entry point using winit.

- Creates window, initializes wgpu, launches the Renderer
- Minimal logic — delegates everything to bif_viewport
- Error handling: `anyhow::Result`

### bif_maketx (~200 LOC, 0 tests)

Standalone CLI tool for converting textures to `.tx` format via OIIO subprocess.

## Known Issues

| Issue | Severity | Location | Status |
|-------|----------|----------|--------|
| Renderer God object (75-99 fields) | High | bif_viewport/lib.rs | Decomposition planned (sub-structs: GpuContext, SceneState, CameraState, NodeGraphContext, EditContext) |
| `unsafe impl Sync for UsdStage` | Medium | bif_core/cpp_bridge.rs | Soundness hole — should use Mutex |
| 0 tests on render.rs (2,582 LOC) | Critical | bif_viewport/render.rs | GPU-dependent, needs logic extraction |
| 0 tests on scene_loader.rs (2,350 LOC) | Critical | bif_viewport/scene_loader.rs | GPU-dependent, needs logic extraction |
| Windows-only build | High | bif_core/build.rs | Phase 2 refactor planned |

## Error Handling Pattern

Library crates use typed `thiserror` errors. Application crates use `anyhow`. This is the correct Rust pattern — typed errors for programmatic handling in libraries, ergonomic `anyhow` at the application boundary.

## Related

- [[node-graph-system|Node Graph System]] — Node graph architecture details
- [[scene-browser|Scene Browser]] — Scene browser merging logic
- [[004-cpp-bridge-for-usd|ADR 004: C++ Bridge for USD]] — Why the C++ bridge exists
