# Changelog

All notable changes to BIF will be documented in this file.

Format based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added

- **USD export: stage metadata** — export writes upAxis, metersPerUnit, timeCodesPerSecond, defaultPrim
- **USD export: materials** — export writes UsdPreviewSurface materials + bind_material per mesh
- **USD export: lights** — export writes all 4 light types (Distant, Sphere, Rect, Dome) as UsdLux prims
- **USD export: cameras** — export writes UsdGeomCamera with FOV-to-focal-length conversion
- **USD export: visibility** — hidden_prim_paths on ExportConfig writes visibility:invisible
- **Curves/Points import** — loader reads UsdGeomBasisCurves and UsdGeomPoints via C++ bridge
- **CurvesPrim/PointsPrim** — new scene graph types for curves and points primitives
- **PointCloud.invisible_ids** — stores instancer invisibleIds for roundtrip export
- **GeomSubset export** — per-face material assignments via UsdGeomSubset child prims
- **C++ bridge: write_geom_subset** — creates GeomSubset with face indices + material binding
- **C++ bridge: write_invisible_ids** — sets invisibleIds on PointInstancer for visibility masking
- **OpenPBR MaterialX dual export** — write_material outputs both UsdPreviewSurface + OpenPBR MaterialX (ND_open_pbr_surface_surfaceshader) with specular_ior and transmission_weight
- **Curve/points viewport preview** — CurvePreviewRenderer draws BasisCurves as LineList segments and Points prims as cross-hairs
- **Bound Material inspector** — property inspector shows OpenPBR params (color swatches, scalars, textures, double-sided) for selected prim's material
- **Implicit geometry** — C++ bridge tessellates UsdGeomSphere/UsdGeomCube with dedup by radius/size, producing native instances with material overrides
- **Native instance material overrides** — Rust loader clones prototypes for instances with different materials (e.g. 3 spheres, 3 materials)
- **DomeLight auto-HDRI** — auto-connect first DomeLight texture to HdriEnvironment on USD load
- **DomeLight rotation** — extract Y-axis rotation from transform's Z basis vector
- **Color temperature** — C++ bridge converts UsdLux `colorTemperature` via Tanner Helland approximation
- **Light schema compat** — `get_light_attr()` helper tries non-prefixed then `inputs:` prefixed attribute names (old/new USD schema)
- **Default Sky toggle** — `use_sky_gradient` checkbox in Ivar render settings UI
- **Mesh orientation** — C++ bridge reverses winding for leftHanded meshes
- **`treatAsPoint`** — C++ bridge reads USD SphereLight `treatAsPoint` attribute, sets radius=0 for point light behavior
- **`LightSample.is_delta`** — delta light flag enables correct NEE without MIS weighting

### Fixed

- **Point light specular ring artifact** — MIS power heuristic crushed specular peak for delta lights; now skips MIS when `is_delta=true`
- **Shadow ray self-intersection** — shadow ray origin offset along surface normal prevents acne on curved geometry

### Removed

- **Rust USDA parser** — deleted parser.rs + SceneBuilder; all USD loading routes through C++ bridge

### Changed

- Winding convention switched from CW to CCW throughout (primitives, mesh normals, C++ tessellation)
- RectLight emits along local -Z axis, uses radiance-based emission (no distance falloff — PDF handles it)
- Default render background changed from gray (0.1) to black
- Batch render respects `use_sky_gradient` setting

### Fixed

- **EventBus** — typed `AppEvent` enum + `EventBus` replacing 23 egui temp-data string-keyed slots; compile-time checked, framework-agnostic
- **SelectionManager** — unified selection state (prim path, properties, instance index, scene browser, gizmo) extracted from Renderer
- **SceneManager** — consolidated 16 scene-related fields (geometry, instances, materials, USD stage, undo/redo) into single subsystem
- `MeshData::Default` impl for subsystem initialization
- `CameraProjection` enum (Perspective/Ortho) for typed camera projection events

### Fixed

- **OIIO mip read_image buffer overflow** — `read_image()` in mip loop always passed `miplevel=0`, reading full-res data into smaller mip buffers; caused crash when loading .tx files with embedded mipmaps
- **UDIM double V-flip** — `transform_uv()` returned pixel-space coords causing `sample()` to apply a second V-flip; now returns UV-space so CPU Ivar sampling matches GPU path
- UDIM atlas pixel cap now downscales tiles instead of erroring (prior commit, included in this changeset)

### Added

- `test_udim_sample_2x2_grid` — end-to-end 2x2 UDIM sample test exercising full `sample()` → `1.0-v` → pixel pipeline with 4 distinct tile colors

### Changed

- `TextureCache::prefer_tx` now defaults to `true` — .tx files loaded automatically when OIIO feature active
- .tx loading logs at `info` level for visibility
- UDIM tile size cap raised 2048→4096, atlas pixel budget 16M→64M (1 GB) for production textures
- Renderer struct reduced from ~50 fields to ~25 fields via 3 new subsystems
- `dispatch_deferred_events()` (280 lines of untyped temp-data polling) → `dispatch_events()` (~120 lines of typed match)
- `SceneInstances` visibility changed from `pub(crate)` to `pub` for SceneManager access
- Merged duplicate `SyncViewportToUsdCamera`/`SyncViewportCamera` into single `SyncUsdCamera` event
- `ExportEditLayer` event uses `PathBuf` instead of `String`
- `EventBus::drain()` preserves vec capacity across frames

### Changed

- **Breaking: Disney → OpenPBR material migration** — full removal of Disney Principled BSDF, replaced with OpenPBR Surface v1.1 parameters, IOR-based Fresnel, and ASWF-standard naming across all 6 crates
  - `DisneyBSDF` → `OpenPbrSurface` (bif_renderer)
  - `bif_core::Material` fields renamed: `diffuse_color`→`base_color`, `metallic`→`base_metalness`, `roughness`→`specular_roughness`, `specular`→`specular_weight`, `emissive_color`→`emission_color`, `opacity`→`geometry_opacity`, texture fields follow suit
  - Added `specular_ior` (default 1.5) — IOR-based F0 replaces Disney's abstract `specular*0.08`
  - GPU structs (`MaterialUniform`, `MaterialGpu`) repacked: `base_color[r,g,b,metalness]`, `specular_params[roughness,ior,weight,pad]`, new `emission` + `extra_params` fields (64→96 bytes)
  - WGSL shader uses IOR-based F0: `((ior-1)/(ior+1))^2 * weight`
  - Default roughness 0.5→0.3, default specular_weight 0.5→1.0, default base_color (0.5,0.5,0.5)→(0.8,0.8,0.8)
  - Stub fields for coat, fuzz, subsurface, emission_luminance, specular_color (stored, not evaluated)

### Fixed

- Ivar texture loading: resolve relative paths via material.source_dir (was black objects)
- Ivar default material: append fallback at index N in build_materials (was wrong textures/OOB)
- Ivar per-instance material binding: combine_with_transforms uses instance_material_id fallback
- Ivar texture load failures now logged instead of silently dropped
- Embree material index clamp uses saturating_sub for safety

### Added

- **Glass/transmission rendering** — extract `transmission` + `specular_IOR` from MaterialX and UsdPreviewSurface in C++ bridge, wire through Rust FFI → `Material.transmission_weight` → `OpenPbrSurface.scatter_transmission()` with Snell's law refraction, TIR, and Schlick Fresnel
  - UsdPreviewSurface heuristic: `opacity < 1` on dielectric → `transmission = 1 - opacity`
  - `OpenPbrSurface::glass(ior)` constructor, `is_delta()` updated for smooth glass
- USD spec compliance (sessions 1-3): 15 new FFI fields for read-side mesh/instancer/camera/stage
- Mesh read: visibility, doubleSided, subdivisionScheme, normalsInterpolation, displayColor/Opacity, resetXformStack
- Instancer read: velocities, angularVelocities, invisibleIds (filtered in loader)
- Camera read: horizontalAperture + aspect_ratio() helper
- Stage read: timeCodesPerSecond
- Lights: CylinderLight + DiskLight types, ShapingAPI (cone angle/softness/focus/IES)
- New UsdGeomPoints read support (positions, widths, normals, IDs)
- Arbitrary primvar query API (float/float2/float3/int, per-mesh)
- Material.double_sided field
- Scene browser shows real USD inherited visibility
- Embree Catmull-Clark subdivision: original polygon topology + crease data from USD, RTC_GEOMETRY_TYPE_SUBDIVISION
- Material export: dual UsdPreviewSurface + OpenPBR MaterialX network with 5 texture connections
- Material binding export via UsdShadeMaterialBindingAPI
- Visibility export (inherited/invisible)
- Stage metadata export (metersPerUnit, upAxis, timeCodesPerSecond)
- Camera export (UsdGeomCamera with animated xform support)
- Light export (all 6 UsdLux types + ShapingAPI)
- UsdRenderSettings export (resolution, camera, pixel aspect ratio)
- Payload read (has_payload, is_loaded on PrimInfo) + load/unload + payload write
- Variant query (set names, variant names, selection) + set_variant_selection (re-composes stage)
- BasisCurves read (points, widths, curveVertexCounts, type/basis/wrap)
- Light linking: UsdCollectionAPI include/exclude paths per light
- PrimInfo: has_inherits, has_specializes composition arc flags
- UsdSkel read: skeleton topology, bind/rest transforms, skin binding (joint indices/weights)
- UsdVol read: OpenVDB asset paths, field names, transforms
- Mesh struct: subdivision_scheme, face_vertex_counts, polygon_indices, crease data fields
- 50 new tests: bif_math camera/aabb/basis/frustum/interval (31), bif_renderer EXR negative frames (1), bif_viewer CLI parsing + click detection (19)
- `Renderer::needs_redraw()` for conditional redraw in viewer
- `#[must_use]` on pure functions in texture.rs and scene.rs
- Interval doc comment, debug_assert on inverted intervals
- Ivar material cache invalidation + prewarm (background texture/material loading before batch render)
- `normalize_path()` helpers in bif_core and bif_viewport for UNC path handling at filesystem boundaries

### Fixed

- UDIM texture loading for UNC network paths (forward-slash `//server/...` now treated as absolute)
- Viewport texture regression: normalize only at filesystem boundaries, not in cache/index_map keys
- UDIM atlas pixel budget constant `MAX_IVAR_ATLAS_PIXELS`

### Changed

- Renderer decomposed: 4 sub-structs (GpuContext, CameraState, IvarContext, NodeGraphContext) extract 30 fields
- CameraState GPU fields tightened to `pub(crate)` (only `camera` remains `pub`)
- `IvarState::invalidate_scene()` and `reset_on_resize()` deduplicate 6 reset blocks
- bif_math re-exports removed from bif_renderer public API (now `pub(crate)`)
- DenoiseError converted to thiserror derive
- Explicit glam re-exports (was `pub use glam::*` re-exporting 200+ items)
- Arvo's method for AABB transform (~2x faster)
- Prototype lookup uses HashMap (was O(N*P) linear scan)
- Mesh dedup hash samples 10 vertices (was 3)
- SphereLight uses solid-angle sampling (was ad-hoc inverse-square)
- SHARC radiance cache uses CAS loop (was racy lock-free write)
- Camera matrix path uses flat copy like meshes (was inconsistent transposition)
- Viewer CLI validates args, .expect()→graceful exit, conditional redraw
- Send+Sync safety comments expanded on UsdStage/UsdEditLayer

### Fixed

- **Memory leak:** UsdEditLayer::save() nulled pointer preventing Drop from freeing C++ handle
- **SHARC race:** two threads CAS-increment sample_count but only one's radiance survived
- **Instance hit:** re-normalization after inverse transform broke rec.t for non-uniform scales
- **AOV correlated noise:** missing seed finalization hash in render_bucket_with_aovs
- **Matrix convention:** camera xform used row/col transpose while meshes used flat copy
- **Culling OOB panic:** instance_aabbs/transforms length mismatch after partial scene update
- **Duplicate CullingResult:** consolidated to single definition in frustum_culling
- **Shader specular:** extreme values at grazing angles, clamped n_dot_v/n_dot_l ≥ 0.001
- **Distant light cone:** cos(1 - angle/2) → cos(angle/2)
- **Lambertian BSDF:** removed baked cos_theta that double-weighted NEE
- **is_delta():** returns false when roughness/metallic textures bound
- NaN guards on Camera::new, Camera::pan, Camera::set_from_matrix, Aabb::hit, Renderer throughput
- Texture::sample() 0-size panic guard
- HDRI pole singularity clamp widened
- EXR negative frame filename collision
- Hardcoded dev-machine path in validate.rs → requires USD_TOOLKIT_DIR env var
- prim_matches_path loose suffix → checks path component boundary
- Point instancer silent fallback → log::warn
- debug_usd_mesh normals bounds check validates all 3 indices
- C++ dead computation in matrix_to_float16, reserve()+shrink_to_fit()
- Removed duplicate Aabb::empty() (use Aabb::EMPTY)
- texture_loader use-after-move bug

- USD camera properties FFI — read focal_length, vertical_aperture, clipping_range from UsdGeomCamera
- `CameraProperties` struct with `fov_y()` computation (2*atan(aperture/2*focal))
- Viewport syncs FOV/near/far from USD cameras (graceful fallback on error)
- UDIM atlas stitching in Ivar TextureCache — probe/downscale/stitch pipeline with memory caps
- UDIM UV transform in `Texture::sample`/`sample_channel` via `transform_uv()`
- UDIM UV transformation in viewport shader — atlas grid metadata packed into MaterialGpu.extra_indices
- Per-triangle material IDs for Ivar combined mesh — post-fill from instance material bindings
- USD native instance support (`instanceable=true`) — `UsdTraverseInstanceProxies()` predicate
- `MeshPurpose` enum (Default/Render/Proxy/Guide) from `UsdGeomImageable` purpose attribute
- Native instance FFI: `usd_bridge_get_native_instance_count/get_native_instance`
- UDIM texture atlas stitching — detects `<UDIM>` tokens, scans tiles 1001-1100, builds atlas
- C++ bridge defense-in-depth clamping for camera properties
- UDIM atlas size overflow guard (256MB cap)
- `DisplaySettings` struct with `PurposeMode` toggle and `lod_enabled` flag
- UI controls for purpose mode (Render/Proxy) and LOD enable checkbox

### Fixed

- `mesh_material_paths` populated after mesh caching (was empty due to init ordering bug)
- Material dedup for instance proxies — same material visited per-instance now cached once (keyed on prototype path)
- Instance proxy mesh material binding — resolve `bound_material_path` during traversal (proxy paths fail `GetPrimAtPath()` afterward)
- Empty `face_material_ids` forcing material 0 on all faces — only populate when GeomSubsets exist
- Undersized triangle material buffer — GPU OOB reads returned 0 instead of 0xFFFFFFFF sentinel
- UDIM/relative texture path resolution — anchor against source layer via `SdfComputeAssetPathRelativeToLayer()`
- Texture index path normalization — backslash/forward-slash mismatch on Windows
- Purpose filtering — use `ComputePurpose()` for inherited purpose (was only reading directly-authored)
- MAX_VIEWPORT_TEXTURES 128→512 — ALab's 272 textures were truncated at 127
- UDIM shader UV underflow — signed math prevents garbage sampling on out-of-range UVs
- Box filter replaces nearest-neighbor for UDIM tile downscaling (anti-moire)
- UDIM tile scan range 1001–1100 → 1001–1200
- Material table rebuild after UDIM texture streaming — grid info unavailable at initial build
- Backface culling winding order — `FrontFace::Cw` → `FrontFace::Ccw` (USD rightHanded = CCW front faces)
- Camera direction extraction in `sync_viewport_to_usd_camera` — use row-based forward/up vectors

### Changed

- M19.6: Split `render()` (2,695 lines) into 6 phase methods + extracted helpers
- Extract left/stats panel to `render_ui.rs` with `StatsPanelParams` struct
- Group 15 flat `Renderer` fields into 3 sub-structs (`AsyncChannels`, `UiLayout`, `SceneInstances`)
- Split `node_graph.rs` (2,272 lines) into `node_graph/` module directory (mod.rs, viewer.rs, ops.rs)

## [0.11.0] - 2026-03-13

### Added

- Ivar material cache (`ivar_materials`) — persists `Vec<Arc<DisneyBSDF>>` across Ivar builds
- Material pre-warm (`prewarm_ivar_materials()`) — background thread builds materials on scene load
- Embree indexed geometry path (`try_from_indexed`, `from_indexed`) — shared vertices, parallel hit data build
- `EmbreePickScene::from_indexed()` — indexed pick scene for viewport selection
- `MeshData::extract_positions/normals/uvs()` — SOA extraction helpers
- Shared `build_materials()` helper deduplicating 4 material construction sites
- 4 unit tests for `EmbreeScene::from_indexed()` (basic hit, shared verts, instancing, error)
- Index buffer OOB validation with `log::warn` in Embree indexed path
- Prewarm/build race guard: 50ms `recv_timeout` before fallback to full material load

### Changed

- Ivar scene build sends materials back via channel for caching (subsequent builds skip texture loading)
- Batch render `SceneBuilderData` carries cached materials for per-frame reuse
- Material cache invalidated only on scene reload or material edit, not on camera/transform changes
- `SceneBuilderFn` changed from `Fn`+`Mutex` to `FnMut` (correct interior mutation semantics)
- Embree indexed path stores per-vertex UV/normals with index lookup (~6x memory savings vs per-triangle expansion)

### Performance

- Ivar subsequent builds: 6.7s → ~47ms (cached materials skip texture loading)
- Ivar first build with pre-warm: 6.7s → ~47ms (materials ready before render starts)
- Embree indexed geometry: 37s → 47ms BVH build (shared vertices)

## [0.1.0] - 2026-03-12

### Fixed

- Textures persisting from first USD scene when loading second scene — two bugs:
  - `add_prototype()` during scene merge dropped material bindings (replaced with full prototype clone)
  - `face_material_ids` not remapped by material offset after merge (GeomSubset indices pointed at wrong materials)

### Added

- GPU mipmap compute shader (`mipmap_downsample.wgsl`) — box-filter downsample on GPU
- `MipmapGenerator` compute pipeline for GPU-side mipmap generation
- Async texture streaming: placeholders load instantly, textures stream in via background thread
- `poll_texture_loads()` per-frame texture streaming with automatic bind group rebuild
- Viewport texture size limit (`DEFAULT_MAX_VIEWPORT_TEXTURE_SIZE = 2048`) — auto-downscales large textures
- `RawTexture` u8 loading path — bypasses f32 intermediate for viewport textures
- `downscale_raw_nearest()` — nearest-neighbor downscale operating on u8 RGBA data

### Changed

- **Texture loading ~25-50x faster**: eliminated triple format conversion (C++ float→u8, Rust u8→f32, GPU f32→u8)
- C++ OIIO bridge reads LDR textures as UINT8 directly (skip float allocation + per-pixel conversion)
- Viewport texture loading uses raw u8 path — `Rgba8UnormSrgb` GPU format handles sRGB decode in hardware
- OIIO texture loading parallelized with rayon (`par_iter` for both OIIO and non-OIIO paths)
- CPU mipmap generation disabled for viewport (GPU mipmaps replace it)
- USD scene finalization uses async texture streaming (instant scene display with placeholder textures)

### Added (continued)

- Blue noise camera jitter with Cranley-Patterson rotation (256x256 void-and-cluster texture)
- SamplerMode enum (WhiteNoise/BlueNoise) with UI dropdown, default BlueNoise
- Pixel reconstruction filters: Box, Gaussian, Mitchell-Netravali, Blackman-Harris
- Weighted progressive accumulation for non-box filters
- Auto-denoise on render completion (OIDN feature)
- Milestones 30-35 roadmap (project save, lights, materials, shader graph, timeline, render queue)
- CI/CD pipeline: GitHub Actions for fmt/clippy/test on push/PR, release builds on tags
- CHANGELOG.md for tracking release notes

### Changed

- `ray_color` now delegates to `ray_color_with_aovs` (eliminates ~160 lines of duplicate bounce loop)
- EXR writer: single `AnyChannels`-based function replaces 8 combinatorial variants (-370 lines)
- Unified Ray type: deleted `bif_renderer::Ray`, use `bif_math::Ray` everywhere
- USD module: wildcard re-exports replaced with explicit imports
- `ControlFlow::Poll` → `Wait` in winit event loop (was burning 100% CPU when idle)
- Lambertian scatter uses `cosine_weighted_hemisphere()` (Malley's method) instead of rejection sampling
- Embree vertex stride 12→16 bytes (SIMD alignment)
- `DEFAULT_FAR_PLANE` 100→10000 (was clipping VFX scenes)
- `CompositeProvider` dedup: `sort+dedup` replaces O(n²) `Vec::contains`
- `.usd` binary files now route to C++ bridge (was going to pure-Rust USDA parser)

### Fixed

- GPU mipmap sRGB crash — `upload_raw_texture` creates sRGB textures as `Rgba8Unorm` (storage-compatible) with `Rgba8UnormSrgb` view for correct hardware sRGB decode
- Normal transforms use inverse-transpose for non-uniform scale (instanced_geometry + Embree + viewport)
- UsdEditLayer double-free — `save()` nulls pointer before Drop runs
- NEE MIS light PDF — `sample_one` includes 1/N light selection probability
- Default BSDF/PDF mismatch — both return `1/(2*PI)` consistently
- `UsdMesh::triangulate` bounds check — break on truncated indices instead of panic
- USD toolkit path uses `USD_TOOLKIT_DIR` env var instead of hardcoded path
- Material `source_dir` derived from input file path for relative texture resolution
- USDA parser logs warnings on bad floats instead of silent `unwrap_or(0.0)`
- Batch render now uses viewport HDRI rotation/intensity instead of baked-in values
- Camera interaction magic numbers extracted to named constants
- Bucket RNG seed finalization — bit-mixing for uncorrelated seeds between adjacent passes
- ImageBuffer debug_assert bounds checking in get()/set()
- Mesh dedup hash — DefaultHasher with sampled vertices instead of weak XOR
- Duplicate `gen_f32` in light.rs removed (imports from material.rs)
- O(n²) mesh animation lookup replaced with `enumerate()`
- `#[inline]` on `Aabb::hit()` for cross-crate BVH inlining
- `axis_interval` catch-all `_` replaced with explicit match + panic

### Removed

- Dead `instanced_geometry_bvh.rs` (274 lines, broken UB, never compiled)
- CI: `bif_core` build.rs no longer panics when vcpkg not installed (graceful skip for clippy-only mode)
- CI: removed bif_renderer/bif_viewport test steps that can't link without USD env
- CI: build.rs vcpkg detection checks toolchain file + USD headers (fixes false positive on GH Actions `C:\vcpkg`)
- CI: removed invalid `pxr` port from vcpkg.json (USD not available as standard vcpkg port)
