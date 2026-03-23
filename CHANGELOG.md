# Changelog

All notable changes to BIF will be documented in this file.

Format based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added

- **Stage::Load() eager population** — 8.3x USD loading speedup (8.8s→1s on Glasses.usd) by forcing eager USD composition
- **GPU buffer size guards** — cap triangle material, vertex, and index buffers to device limits with placeholder fallback (prevents crash on 342M vert scenes)
- **Chunked texture loading** — load 16 textures at a time (was all-at-once), paced GPU uploads (32/frame)
- **Texture count warning** — log when >511 textures exceed viewport GPU slot limit
- **Texture backpressure** — `sync_channel(32)` prevents unbounded RAM growth on 500+ texture scenes
- **Adaptive texture downscale** — auto 512px for 200+ textures, 1024px for 50+ (background thread downscale before channel send)
- **MAX_VIEWPORT_TEXTURES** — raised 512→2048 for production scenes
- **Free C++ mesh cache** — `usd_bridge_free_mesh_geometry()` frees normals/UVs/subdivision after Rust copy (~8GB on 335M vert scenes)
- **Texture streaming progress** — periodic log of loaded/total count
- **LoadNone deferred payloads** — `UsdStage::Open(LoadNone)` opens hierarchy only; `load_payloads()` loads geometry on demand
- **UDIM tile downscale before stitch** — tiles downscaled to adaptive size before atlas assembly (was full-res → OOM)
- **Pick scene size guard** — skip Embree pick BVH for >50M tris (prevents 25GB OOM)
- **C++ debug log flags** — `g_log_textures`, `g_log_timing`, `g_log_variants` toggle output sections
- **Parallel UV seam split** — 3-pass `cache_stage_data()` refactor using USD `WorkParallelForN`; per-mesh geometry extraction in parallel with per-thread `UsdGeomXformCache`

### Fixed

- **u32 overflow in triangle count display** — use u64 for large scene stats (607M tris × 13K instances)
- **Normals lost on meshes without UVs** — deferred normals copy was inside UV block; meshes with normals but no UVs got flat shading

- **Normals lost on meshes without UVs** — deferred normals copy was inside UV block; meshes with normals but no UVs got flat shading
- **UV seam split hash collisions** — PairHash uses bit mixing instead of MSVC identity hash

### Changed

- **Deferred normals copy** — skip 289ms wasted copy when UV seam split rebuilds normals
- **Bulk vertex/normal copy** — `assign()` replaces push_back loops in C++ bridge
- **UV seam split** — `std::map` → `std::unordered_map` (O(log n) → O(1))
- **Deferred Rust clones** — mesh dedup hash from references, clone only unique meshes
- **Disable Ivar prewarm** — materials built on-demand at render time (saves 8+ GB RAM on large scenes)
- **UDIM atlas cap** — 64MP → 32MP (1GB → 512MB max per atlas)
- **Remove texture upload clone** — skip `tex.data.clone()` when no downscale needed

## [0.12.0] - 2026-03-21

### Added

- **Purpose filtering** — USD purpose attr (render/proxy/guide) toggle in viewport; C++ bridge `compute_inherited_purpose()` with hierarchy walk for instance proxies
- **Purpose enum** — `Purpose` type on `Instance` with per-instance filtering in combined mesh build
- **Native instance purpose** — own purpose from scene hierarchy (not inherited from prototype mesh)
- **Material diagnostics** — debug-level Ivar material/texture logging (`RUST_LOG=bif_renderer=debug`)
- **USD export** — stage metadata, materials (UsdPreviewSurface + OpenPBR MaterialX), lights (4 types), cameras, visibility, GeomSubsets, invisible_ids roundtrip
- **Curves/Points import** — UsdGeomBasisCurves and UsdGeomPoints via C++ bridge with viewport preview
- **Bound Material inspector** — property inspector shows OpenPBR params for selected prim
- **Implicit geometry** — C++ bridge tessellates UsdGeomSphere/UsdGeomCube with dedup + native instances
- **DomeLight** — auto-HDRI, rotation, color temperature via Tanner Helland
- **Glass/transmission** — extract from MaterialX/UsdPreviewSurface, Snell's law refraction + TIR
- **USD spec compliance** — 15 FFI fields (visibility, doubleSided, subdivisionScheme, velocities, cameras, lights)
- **Embree subdivision** — Catmull-Clark from USD polygon topology + crease data
- **UDIM texture atlas** — probe/stitch pipeline for Ivar + viewport with memory caps
- **Blue noise sampling** — Cranley-Patterson rotation, pixel reconstruction filters
- **EventBus** — typed `AppEvent` enum replacing 23 string-keyed temp-data slots
- **Subsystem extraction** — SceneManager, SelectionManager, CameraState, IvarContext, NodeGraphContext
- **50+ new tests** — bif_math, bif_renderer, bif_viewer

### Fixed

- **UsdPreviewSurface specular** — `specularColor` was averaged to `specular_weight`, breaking dielectrics; now always 1.0 (IOR/Fresnel-controlled)
- **Ivar double-filtering** — purpose filter applied twice causing material index misalignment
- **OIIO mip buffer overflow** — `read_image()` always passed `miplevel=0` into smaller mip buffers
- **UDIM double V-flip** — `transform_uv()` returned pixel-space causing second flip in `sample()`
- **Memory leak** — `UsdEditLayer::save()` nulled pointer preventing Drop from freeing C++ handle
- **SHARC race** — two threads CAS-increment but only one's radiance survived
- **Point light specular ring** — MIS power heuristic crushed delta light specular peak
- **Shadow ray self-intersection** — offset along surface normal prevents acne
- **Ivar texture paths** — resolve relative paths via `material.source_dir`
- **Material dedup** — instance proxy materials cached once per prototype
- **Instance proxy binding** — resolve `bound_material_path` during traversal
- **Backface culling** — `FrontFace::Cw` → `FrontFace::Ccw` for USD rightHanded convention

### Changed

- **OpenPBR migration** — Disney Principled BSDF → OpenPBR Surface v1.1 across all 6 crates
- Renderer decomposed from ~75 fields into sub-structs
- `render()` (2,695 lines) split into 6 phase methods
- `node_graph.rs` (2,272 lines) split into module directory
- Winding convention CW → CCW throughout
- `ControlFlow::Poll` → `Wait` (was burning 100% CPU idle)
- Texture loading ~25-50x faster (raw u8 path, GPU mipmaps, async streaming)

### Removed

- Rust USDA parser (all USD loading via C++ bridge)
- Dead `instanced_geometry_bvh.rs` (274 lines, broken UB)
- Stub LayerStack/Composition property inspector tabs (will return in M30+)

## [0.11.0] - 2026-03-13

### Added

- Ivar material cache + pre-warm (background texture/material loading)
- Embree indexed geometry path (shared vertices, parallel hit data)
- `MeshData::extract_positions/normals/uvs()` SOA helpers
- Shared `build_materials()` helper

### Performance

- Ivar subsequent builds: 6.7s → ~47ms (cached materials)
- Embree indexed geometry: 37s → 47ms BVH build

## [0.1.0] - 2026-03-12

Initial versioned release. Viewport rendering, instancing, USD C++ bridge, Embree ray tracing, materials, MaterialX, animation, batch render, node graph, scatter, SHARC cache, OIDN denoising.
