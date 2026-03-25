# BIF Milestones

Complete milestone history and future roadmap for the BIF VFX renderer project.

---

## Completed Milestones ✅

### Milestone 0: Environment Setup ✅

- **Completed:** 2024-12-26
- **Time Invested:** ~1 hour
- **Key Achievements:**
  - Cargo workspace with 4 crates
  - Git repository with LFS for large files
  - Go raytracer preserved in `legacy/go-raytracing/`
- **Devlog:** Initial setup (pre-devlog system)

---

### Milestone 1: Math Library ✅

- **Completed:** 2024-12-27
- **Time Invested:** ~4 hours
- **Location:** `crates/bif_math/src/`
- **Key Achievements:**
  - Ported from Go implementation
  - `Ray` - Ray with origin, direction, time (6 tests)
  - `Interval` - Min/max range operations (10 tests)
  - `Aabb` - Axis-aligned bounding box with hit testing (6 tests)
  - `Camera` - 3D camera with view-projection matrices (4 tests)
  - **Stats:** 26 tests passing, ~400 LOC
- **Devlog:** [devlog/DEVLOG_2025-12-27_milestone1.md](devlog/DEVLOG_2025-12-27_milestone1.md)

---

### Milestone 2: wgpu Window ✅

- **Completed:** 2024-12-27
- **Time Invested:** ~1 hour
- **Location:** `crates/bif_viewport/src/lib.rs`
- **Key Achievements:**
  - Vulkan backend (auto-selected on Windows)
  - Surface configuration with VSync
  - Dark blue clear color (0.1, 0.2, 0.3)
  - Window resize handling
  - Error recovery for surface loss
  - **Stats:** ~250 LOC
- **Devlog:** [devlog/DEVLOG_2025-12-27_milestone2.md](devlog/DEVLOG_2025-12-27_milestone2.md)

---

### Milestone 3: Triangle + Camera ✅

- **Completed:** 2024-12-27
- **Time Invested:** ~1.5 hours
- **Location:**
  - `crates/bif_viewport/src/shaders/basic.wgsl`
  - `crates/bif_math/src/camera.rs`
- **Key Achievements:**
  - WGSL vertex/fragment shaders
  - Vertex buffer with position + color attributes
  - Uniform buffer for camera matrices
  - Bind group for GPU data transfer
  - Perspective projection (45° FOV)
  - RGB triangle with smooth color interpolation at 60 FPS
  - **Stats:** ~200 LOC, 4 new tests
- **Devlog:** [devlog/DEVLOG_2025-12-27_milestone3.md](devlog/DEVLOG_2025-12-27_milestone3.md)
- **Post-Milestone:** Renamed `bif_render` → `bif_viewport` for clarity

---

### Milestone 4: Camera Controls ✅

- **Completed:** 2024-12-27
- **Time Invested:** ~1 hour
- **Location:**
  - `crates/bif_math/src/camera.rs` - Camera control methods
  - `crates/bif_viewer/src/main.rs` - Input event handling
- **Key Achievements:**
  - Mouse orbit (left-click drag) around target
  - Keyboard movement (WASD + QE for 6DOF)
  - Houdini-style viewport controls (tumble/track/dolly)
  - Distance-scaled movement for better UX
  - **Stats:** ~150 LOC
- **Devlog:** [devlog/DEVLOG_2025-12-27_milestone4.md](devlog/DEVLOG_2025-12-27_milestone4.md)

---

### Milestone 5: OBJ Mesh Loading ✅

- **Completed:** 2024-12-27
- **Time Invested:** ~2 hours
- **Location:** `crates/bif_viewport/src/lib.rs`
- **Key Achievements:**
  - MeshData struct with vertices, indices, AABB bounds
  - tobj integration for OBJ parsing
  - Per-face normal computation for smooth shading
  - Lucy model loaded: 140,278 vertices, 840,768 indices
  - Auto-framing (F key) with dynamic near/far planes
  - **Stats:** ~120 LOC
- **Devlog:** [devlog/DEVLOG_2025-12-27_milestone5_6.md](devlog/DEVLOG_2025-12-27_milestone5_6.md)

---

### Milestone 6: Depth Testing + Enhanced Controls ✅

- **Completed:** 2024-12-27
- **Time Invested:** ~2 hours
- **Location:**
  - `crates/bif_viewport/src/lib.rs` - Depth texture
  - `crates/bif_viewer/src/main.rs` - Input handling
- **Key Achievements:**
  - Depth24Plus format with proper occlusion
  - Mouse scroll (dolly/zoom)
  - Middle mouse (pan/track)
  - Distance-scaled movement for all controls
  - Complete Houdini paradigm: tumble, track, dolly
  - **Stats:** ~140 LOC
- **Devlog:** [devlog/DEVLOG_2025-12-27_milestone5_6.md](devlog/DEVLOG_2025-12-27_milestone5_6.md)

---

### Milestone 7: egui UI Integration ✅

- **Completed:** 2024-12-27
- **Time Invested:** ~1.5 hours
- **Location:**
  - `crates/bif_viewport/src/lib.rs` - egui state and rendering
  - `crates/bif_viewport/Cargo.toml` - egui dependencies
- **Key Achievements:**
  - egui 0.29 integration (egui-wgpu, egui-winit)
  - Immediate-mode side panel (300px)
  - Two-pass rendering (3D scene + UI overlay)
  - FPS counter, camera stats, mesh info, controls help
  - Solved borrow checker and lifetime challenges
  - **Stats:** ~100 LOC
- **Devlog:** [devlog/DEVLOG_2025-12-27_milestone7_8.md](devlog/DEVLOG_2025-12-27_milestone7_8.md)

---

### Milestone 8: GPU Instancing ✅

- **Completed:** 2024-12-27
- **Time Invested:** ~1 hour
- **Location:**
  - `crates/bif_viewport/src/lib.rs` - InstanceData struct
  - `crates/bif_viewport/src/shaders/basic.wgsl` - Per-instance transforms
- **Key Achievements:**
  - Replaced dual-buffer hack with proper GPU instancing
  - InstanceData with 4x4 model matrix (4 vec4 attributes)
  - 100 Lucy models in 10x10 grid, single draw call
  - Performance: 60+ FPS (VSync-limited), 28M triangles
  - Memory saved: ~504MB
  - **Stats:** ~150 LOC
- **Devlog:** [devlog/DEVLOG_2025-12-27_milestone7_8.md](devlog/DEVLOG_2025-12-27_milestone7_8.md)

---

### Milestone 9: USD Import ✅

- **Completed:** 2024-12-30
- **Time Invested:** ~4 hours
- **Location:**
  - `crates/bif_core/src/usd/` - USDA parser module
  - `crates/bif_core/src/mesh.rs` - Mesh data with USD loading
  - `crates/bif_core/src/scene.rs` - Scene graph structure
- **Key Achievements:**
  - Pure Rust USDA parser (no C++ dependencies)
  - Supported types: UsdGeomMesh, UsdGeomPointInstancer, Xform
  - Triangulation of N-gon faces via fan triangulation
  - CLI integration: `cargo run -p bif_viewer -- --usda assets/lucy_low.usda`
  - Viewport: FrontFace::Cw for Houdini/USD compatibility
  - **Stats:** ~1,500 LOC, 15+ tests
- **Devlog:** [devlog/DEVLOG_2025-12-30_milestone9.md](devlog/DEVLOG_2025-12-30_milestone9.md)
- **Documentation:** [HOUDINI_EXPORT.md](HOUDINI_EXPORT.md) - Best practices guide

---

### Milestone 10: CPU Path Tracer "Ivar" ✅

- **Completed:** 2024-12-30
- **Time Invested:** ~4 hours
- **Location:** `crates/bif_renderer/src/`
- **Key Achievements:**
  - Complete CPU path tracer named "Ivar"
  - Ray/HitRecord with lifetime-annotated material references
  - Materials: Lambertian, Metal, Dielectric, DiffuseLight
  - Primitives: Sphere (UV), Triangle (Möller-Trumbore)
  - BVH with median-split (fixed object loss bug)
  - Camera with DOF support, builder pattern
  - PNG output via `image` crate 0.24
  - Performance: 479 objects @ 800x450, 100spp in ~52s
  - **Stats:** ~1,200 LOC, 14 tests
- **Devlog:** [devlog/DEVLOG_2025-12-30_milestone10.md](devlog/DEVLOG_2025-12-30_milestone10.md)

---

### Milestone 11: Ivar Viewport Integration ✅

- **Completed:** 2024-12-30
- **Time Invested:** ~4 hours
- **Location:**
  - `crates/bif_viewport/src/lib.rs` - Render mode toggle, Ivar integration
  - `crates/bif_core/src/usd/types.rs` - Left-handed orientation fix
- **Key Achievements:**
  - Render mode toggle: Vulkan (real-time) ↔ Ivar (path tracer)
  - Ivar instancing: All instance transforms applied during BVH build
  - Left-handed winding fix for USD files from Houdini
  - Progressive bucket rendering (64x64 pixels)
  - Parallel bucket rendering via rayon thread pool
  - Performance: 100 instances = 28M triangles, BVH build ~4s
  - **Stats:** ~400 LOC
- **Devlog:** [devlog/DEVLOG_2025-12-30_milestone11.md](devlog/DEVLOG_2025-12-30_milestone11.md)

---

### Freeze Fix: Instance-Aware BVH + Background Threading ✅

- **Completed:** 2024-12-31
- **Time Invested:** ~6 hours
- **Problem:** 4-second UI freeze when switching to Ivar mode
- **Location:**
  - `crates/bif_math/src/transform.rs` (NEW) - Mat4 extension methods
  - `crates/bif_renderer/src/instanced_geometry.rs` (NEW) - Instance-aware BVH
  - `crates/bif_viewport/src/lib.rs` - Background threading, UI updates
- **Key Achievements:**
  - **Instance-Aware BVH:** ONE prototype BVH (280K triangles), 100 transforms separate
  - Per-instance ray transformation: world→local→test→world
  - **Background Threading:** Scene build moved to `std::thread::spawn`
  - Non-blocking `mpsc::channel()` with `try_recv()` polling
  - UI updates: Spinner during build, rebuild button
  - **Performance:**
    - Triangles in BVH: 28M → 280K (100x reduction)
    - BVH build time: 4000ms → 40ms (100x faster)
    - Memory usage: ~5GB → ~50MB (100x reduction)
    - UI freeze: 4s → **0ms** ✅
  - **Trade-off:** Rendering ~3x slower (linear instance search O(100))
  - **Tests:** 13 new tests (8 transform + 5 instanced_geometry)
  - **Stats:** ~700 LOC added
- **Devlog:** [devlog/DEVLOG_2025-12-31_freeze-fix.md](devlog/DEVLOG_2025-12-31_freeze-fix.md)

---

### Milestone 12: Embree 4 Integration ✅

- **Completed:** 2026-01-01
- **Time Invested:** ~8 hours
- **Location:**
  - `crates/bif_renderer/src/embree.rs` (NEW) - Manual FFI bindings
  - `crates/bif_renderer/build.rs` (NEW) - Link embree4.lib
  - `crates/bif_viewport/src/lib.rs` - EmbreeScene integration
- **Key Achievements:**
  - Embree 4.4.0 via vcpkg (no embree-sys crate exists)
  - Manual FFI bindings (~600 LOC) - educational approach
  - Two-level BVH: prototype mesh (280K tris) + instance transforms
  - Implements `Hittable` trait for seamless Ivar integration
  - **Performance:** 28ms BVH build for 100 instances
  - **Debugging:** Fixed 6 issues (enum values, API changes, memory lifetime)
- **Stats:** ~600 LOC, error checking after all FFI calls
- **Devlog:** [devlog/DEVLOG_2026-01-01_milestone12.md](devlog/DEVLOG_2026-01-01_milestone12.md)

---

## Summary Statistics (as of M19.4)

| Metric | Value |
|--------|-------|
| **Total LOC** | ~28K Rust + C++ bridges |
| **Tests Passing** | 142+ ✅ |
| **Milestones Complete** | 0-19.4 (~25 sub-milestones) |
| **Time Invested** | ~55+ hours documented |
| **Crates** | 6 (math, core, renderer, viewport, viewer, maketx) |
| **lib.rs** | ~1,622 lines (M19.6 cleanup done) |
| **bif_viewport total** | ~10K lines (19 files) |

---

### Milestone 13: USD C++ Integration ✅

- **Completed:** 2026-01-04
- **Time Invested:** ~4 hours
- **Location:**
  - `cpp/usd_bridge/` - C++ FFI bridge
  - `crates/bif_core/src/usd/cpp_bridge.rs` - Rust wrapper
  - `crates/bif_core/build.rs` - CMake automation
- **Key Achievements:**
  - Pixar USD 25.11 via vcpkg
  - C++ FFI bridge with extern "C" functions
  - CMake build integrated into cargo via build.rs
  - ~500 LOC Rust FFI wrapper
  - Full support for USDC binary and file references
  - Environment setup script and documentation
- **Key Discoveries:**
  - USD requires `PXR_PLUGINPATH_NAME` environment variable
  - USD 25.11 changed `GetForwardedTargets()` API
  - vcpkg USD puts import libs in bin/ not lib/
- **Stats:** 3 new integration tests, 18 tests total in bif_core
- **Devlog:** (Embedded in SESSION_HANDOFF.md)

---

### Milestone 13a: USD Scene Browser + Property Inspector ✅

- **Completed:** 2026-01-05
- **Time Invested:** ~6 hours
- **Location:**
  - `crates/bif_viewport/src/scene_browser.rs` - USD hierarchy tree view
  - `crates/bif_viewport/src/property_inspector.rs` - Property panel
  - `cpp/usd_bridge/usd_bridge.cpp` - Prim traversal APIs
- **Key Achievements:**
  - 7 new prim traversal APIs in C++ bridge
  - `PrimDataProvider` trait abstraction for USD data
  - Scene browser with expandable tree and type icons
  - Property inspector with transforms and bounding boxes
- **Devlog:** [devlog/DEVLOG_2026-01-05_milestone13a.md](devlog/DEVLOG_2026-01-05_milestone13a.md)

---

### Milestone 13b: Node Graph + Dynamic USD Loading ✅

- **Completed:** 2026-01-06
- **Time Invested:** ~4 hours
- **Location:**
  - `crates/bif_viewport/src/node_graph.rs` (NEW) - Node graph system
  - `crates/bif_viewport/src/scene_browser.rs` - Houdini-style table
  - `crates/bif_viewport/src/lib.rs` - Dynamic loading
- **Key Achievements:**
  - egui-snarl 0.5 node graph with USD Read + Ivar Render nodes
  - rfd 0.14 native file dialogs (Browse button)
  - `load_usd_scene()` for dynamic USD loading from node graph
  - Houdini-style table layout (Path, Type, Children, Kind, Visibility)
  - Delete key to remove selected nodes
- **Stats:** ~350 LOC, 11 tests
- **Devlog:** [devlog/DEVLOG_2026-01-06_milestone13b.md](devlog/DEVLOG_2026-01-06_milestone13b.md)

---

## Upcoming Milestones 🎯

### Milestone 14: GPU Instancing Optimization (10K+ Instances) ✅

- **Goal:** Enable massive instancing (10K+ instances) with smart LOD system
- **Prerequisites:** Milestone 13b complete ✅, bbox culling added ✅
- **Status:** Complete ✅
- **Completed:** January 9, 2026
- **Target:** 10K Lucy instances (~700M triangles) @ 60 FPS
- **Key Tasks:**
  
  **Phase 1: Enhanced GPU Instancing ✅**
  - ✅ Upgrade existing GPU instancing to handle 10K+ instances
  - ✅ CPU frustum culling before GPU submission (Frustum module in bif_math)
  - ✅ Dynamic instance buffer with COPY_DST (only visible instances uploaded)
  - ✅ Precomputed world-space AABBs per instance
  - ✅ UI shows visible vs total instances (near/far LOD split)
  - Per-instance material IDs (deferred to M15)
  
  **Phase 2: LOD System ✅**
  - ✅ Box proxy for distant instances
  - ✅ Full mesh for near instances
  - ✅ Distance-based LOD selection per frame
  - ✅ Dual draw calls (full mesh + box LOD)
  - ✅ Polygon budget slider (UI control, 0.1M-100M triangles)
  - ✅ Budget percentage indicator in Scene Stats panel
  - Can upgrade to proper LOD meshes later
  
  **Phase 3: Performance Validation ✅**
  - ✅ Test 10K Lucy scene @ 60 FPS target
  - ✅ Frustum culling + LOD working together

- **Performance Target:** 10K instances = ~700M triangles (realistic for modern GPU)
- **Reference:** UE5 Nanite, Unity DOTS instancing

---

### Milestone 15: Materials (UsdPreviewSurface + Disney BSDF) ✅

- **Goal:** Import USD materials and render with proper shading
- **Prerequisites:** Milestone 14 complete ✅
- **Status:** Complete ✅ (Phase 8 - viewport textures deferred to M16)
- **Completed:** January 11, 2026
- **Key Achievements:**

  **Phase 1: UV Coordinate Support ✅**
  - Added `uvs: Option<Vec<[f32; 2]>>` to Mesh struct
  - Extract primvars:st from USD via C++ bridge
  - UV attribute in viewport vertex shader (location 3)

  **Phase 2: Material Data Structures ✅**
  - PBR Material struct: diffuse, metallic, roughness, specular, emissive
  - Texture paths for all material channels
  - Material binding to prototypes

  **Phase 3: C++ Bridge Material Extraction ✅**
  - UsdBridgeMaterialData struct in C++ bridge
  - Parse UsdPreviewSurface shader network
  - Extract texture connections from UsdUVTexture nodes
  - Link usd_usdShade library

  **Phase 4: Rust Material Loading ✅**
  - UsdMaterialData struct and FFI bindings
  - Load materials from USD stage
  - Bind materials to prototypes in Scene

  **Phase 5: Texture Loading System ✅**
  - TextureCache with image crate
  - sRGB to linear conversion
  - Bilinear texture sampling
  - Base directory path resolution

  **Phase 6: Disney Principled BSDF ✅**
  - Full Disney BSDF implementation in Ivar
  - Burley diffuse lobe with subsurface
  - GGX specular with importance sampling
  - Metallic/dielectric Fresnel blending
  - Sheen for cloth-like materials

  **Phase 7: Ivar Material Integration ✅**
  - From<&bif_core::Material> impl for DisneyBSDF
  - Scene materials flow to Ivar renderer
  - EmbreeScene uses loaded materials

  **Phase 8: Viewport Texture Support (Deferred)**
  - GPU texture upload and sampling (M16)
  - Basic PBR in Vulkan viewport (M16)

- **Tests:** 93+ passing
- **Reference:** Disney Principled BSDF paper (2012, 2015)

---

### Milestone 16: MaterialX Support ✅

- **Goal:** Import MaterialX materials from USD and render with proper shading
- **Completed:** 2025-01-17
- **Prerequisites:** Milestone 15 complete ✅
- **Key Achievements:**

  **C++ Bridge MaterialX Support**
  - `is_materialx_standard_surface()` detection for `ND_standard_surface_*` shaders
  - `get_materialx_texture_path()` for `ND_image_*` texture nodes
  - Full standard_surface property extraction:
    - `base_color` → diffuse_color
    - `metalness` → metallic
    - `specular_roughness` → roughness
    - `emission_color` × `emission` → emissive_color
    - `opacity` (vec3 or scalar) → opacity
    - `specular` → specular
  - Automatic fallback: MaterialX → UsdPreviewSurface → default gray
  - `is_materialx` flag exposed via FFI
  - Houdini USD support: search child prims for `mtlxstandard_surface`
  - Parent hierarchy traversal for inherited material bindings

  **Rust FFI Integration**
  - `UsdMaterialData.is_materialx: bool` field added
  - Materials flow to existing Disney BSDF unchanged

  **Validation**
  - Tested with Houdini-exported MaterialX USD (Lucy model)
  - Orange material (base_color=1,0.5,0) renders correctly in Ivar

- **Reference:** MaterialX Specification, USD MaterialX Schema

---

### Milestone 17: Viewport PBR + Textures ✅

- **Goal:** Textured PBR materials in Vulkan viewport
- **Completed:** 2026-01-18
- **Prerequisites:** Milestone 16 complete ✅
- **Key Achievements:**
  - GPU texture upload (binding_array with 64 texture slots)
  - Texture bind group with sampler in render pipeline
  - Updated basic.wgsl for texture sampling (diffuse maps)
  - Per-vertex material IDs via GeomSubset support
  - Per-instance material ID fallback
  - Parallel texture loading (25s → 1s with rayon + sRGB LUT)
  - Texture downscaling for GPU limits (8192 max dimension)
- **Key Files:**
  - `crates/bif_viewport/src/lib.rs` - GPU texture upload, parallel loading
  - `crates/bif_viewport/src/shaders/basic.wgsl` - Texture sampling
  - `cpp/usd_bridge/usd_bridge.cpp` - GeomSubset extraction
- **Reference:** wgpu texture examples, LearnOpenGL PBR

---

### Milestone 17.1: OIIO + .tx Texture Pipeline ✅

- **Goal:** Replace image crate with OpenImageIO for industry-standard .tx support
- **Completed:** 2026-01-21
- **Prerequisites:** M17 complete ✅
- **Key Achievements:**
  - C++ OIIO bridge with FFI (`cpp/oiio_bridge/`)
  - Automatic .tx conversion via `ImageBufAlgo::make_texture()`
  - Mipmap generation and loading (box filter downsample)
  - GPU upload with trilinear + anisotropic filtering (16x)
  - Feature-gated: `--features oiio` to enable
  - Falls back to `image` crate when OIIO not enabled
- **Key Files:**
  - `cpp/oiio_bridge/` - C++ FFI bridge to OpenImageIO
  - `crates/bif_core/src/oiio.rs` - Rust FFI wrapper
  - `crates/bif_core/src/texture.rs` - TextureCache with OIIO/.tx support
  - `crates/bif_renderer/src/disney.rs` - Texture fields in DisneyBSDF
  - `crates/bif_viewport/src/lib.rs` - Mipmap GPU upload
- **Usage:**
  - Build without OIIO: `cargo build` (uses image crate, no mipmaps)
  - Build with OIIO: `cargo build --features oiio` (requires vcpkg openimageio)
- **Devlog:** [devlog/DEVLOG_2026-01-21.md](devlog/DEVLOG_2026-01-21.md)
- **Reference:** OpenImageIO 3.0 documentation, VFX Reference Platform

---

### Milestone 18: Animation + Timeline ✅

- **Completed:** 2026-01-26
- **Goal:** Load and render time-sampled USD data
- **Key Achievements:**
  - Timeline UI (play/pause, frame slider, loop, fps display)
  - AnimatedTransform with keyframe storage + lerp interpolation
  - C++ bridge: timeline metadata, xform samples, vertex animation API
  - Multi-mesh rendering via combined vertex buffer
- **Devlog:** [devlog/DEVLOG_2026-01-26.md](devlog/DEVLOG_2026-01-26.md)

---

### Milestone 18.1: Vertex Animation for Multi-Mesh ✅

- **Completed:** 2026-01-25
- **Goal:** Fix vertex animation when multiple meshes are combined
- **Problem:** Combined buffer (ground 100 verts + cube 8 verts = 108) caused vertex count mismatch with USD's per-mesh animation data
- **Solution:**
  - Added `MeshRange` struct tracking USD mesh index, vertex offset, vertex count
  - `combine_with_transforms()` now builds `mesh_ranges` during combining
  - `update_vertex_animation()` uses `mesh_ranges` to update correct vertex range
- **Key Files:**
  - `crates/bif_viewport/src/mesh_data.rs` - MeshRange, mesh_ranges field
  - `crates/bif_viewport/src/lib.rs` - update_vertex_animation rewrite

---

### Milestone 18.2: Thread Safety + Instance Encapsulation ✅

- **Completed:** 2026-01-26
- **Goal:** Make UsdStage thread-safe for background loading
- **Key Achievements:**
  - Pre-cache all data at load time in `usd_bridge_open_stage()`
  - All getter functions read from immutable caches
  - `thread_local` storage for `get_mesh_vertices_at_time()` return buffer
  - `unsafe impl Send for UsdStage` with documented safety invariants
  - Encapsulated `Scene.instances` behind accessor method

---

### Milestone 18.3: USD Import Refinement ✅

- **Completed:** 2026-01-27
- **Goal:** Fix USD files with relative references failing to load
- **Problem:** `@./lucy_low.usda@` references not resolved by C++ bridge
- **Solution:**
  - Added `ArResolverContextBinder` for asset resolution context
  - Normalized Windows backslashes to forward slashes
  - Added `ar` and `usdShade` libraries to CMake
- **Key Files:**
  - `cpp/usd_bridge/usd_bridge.cpp` - Resolver context, path normalization
  - `cpp/usd_bridge/CMakeLists.txt` - Added dependencies
- **Tests:** `test_load_relative_reference_usda`, `test_load_pointinstancer_external_prototype`
- **Devlog:** [devlog/DEVLOG_2026-01-27_usd-refinement.md](devlog/DEVLOG_2026-01-27_usd-refinement.md)

---

### Milestone 18.4: Multi-Prototype Ivar Fix ✅

- **Completed:** 2026-01-31
- **Goal:** Fix double mesh instances in Ivar rendering for multi-prototype USD scenes
- **Problem:** Meshes appeared duplicated in ray tracer (viewport correct)
- **Root Cause:** Combined mesh has transforms baked into vertices, but Embree was also applying instance transforms → double transform
- **Solution:**
  - C++ bridge: Added mesh path deduplication via `std::set<std::string>` to prevent duplicate caching
  - `build_ivar_scene()`: Use single identity transform for Embree when `use_multi_draw` is true
  - `build_ivar_scene_sync()`: Same fix for batch rendering path
- **Key Files:**
  - `cpp/usd_bridge/usd_bridge.cpp` - Mesh deduplication in `cache_stage_data()`
  - `crates/bif_viewport/src/lib.rs` - Identity transform for combined mesh
- **Devlog:** [devlog/DEVLOG_2026-01-31_double-mesh-fix.md](devlog/DEVLOG_2026-01-31_double-mesh-fix.md)

---

### Milestone 18.5: USD Implementation Polish ✅

- **Completed:** 2026-02-01
- **Goal:** Fix bugs and add timing visibility for USD loading
- **Key Changes:**
  - **Overflow fix:** Changed `num_triangles` from `u32` to `u64` to handle large scenes (100K triangles × 100K instances)
  - **Console I/O fix:** Removed per-mesh/per-keyframe C++ logging that was causing massive slowdown
  - **Load timing:** Added instrumentation to measure stage open, mesh extract, materials, instancers, GPU buffers, textures
  - **Granular profiling:** Added breakdown timing inside `cache_stage_data()` (vertices, triangulate, subsets, normals, UVs, transforms)
- **Performance Results:**
  | Scene | Before | After | Speedup |
  |-------|--------|-------|---------|
  | Spaceship (921 meshes, 3.3M verts) | 117s | 5s | **23x** |
  | Palm tree (1 mesh, 220K verts) | 2.5s | 0.37s | **7x** |
- **Root Cause:** `std::cout`/`fprintf` per-mesh logging - Windows console I/O is extremely slow
- **Key Files:**
  - `cpp/usd_bridge/usd_bridge.cpp` - C++ timing with `<chrono>`, removed verbose logging
  - `crates/bif_viewport/src/lib.rs` - Overflow fix, viewport timing
  - `crates/bif_core/src/usd/loader.rs` - Rust timing
- **Future Work Identified:** Texture loading bottleneck (22s for 13 large textures)

---

### Milestone 19: Frame Rendering 🎞️ (In Progress)

- **Goal:** Render animated sequences to disk
- **Status:** Camera + vertex animation working, instance transform animation TODO
- **Started:** January 31, 2026
- **Key Achievements:**

  **Phase 1: Batch Render Infrastructure ✅**
  - Frame range UI (start/end/step) in Render to Disk panel
  - Batch render loop with frame substitution (`render.####.exr`)
  - Progress bar with frame count and cancellation
  - EXR output with AOVs (beauty, depth, normals)
  - ZIP compression option

  **Phase 2: USD Camera Animation ✅**
  - USD camera selection dropdown (lists cameras from stage)
  - Camera transform evaluation at each frame time
  - Fixed matrix row/column major conversion (USD row 3 = translation)
  - "Sync Viewport to Camera" button for debugging
  - Viewport camera syncs to USD camera position

  **Phase 3: Bug Fixes ✅**
  - FOV conversion: viewport radians → renderer degrees
  - Fallback lighting: sky gradient when no HDRI
  - UNC network paths: `\\?\UNC\...` → `\\server\...`
  - Sync scene build on Render click (no pre-render required)

  **Phase 4: Vertex Animation in Batch Render ✅ (M19.1)**
  - `build_triangles_at_time()`: Extract triangles with USD time query
  - `SceneBuilderData`: Holds mesh/material data for per-frame rebuilds
  - `batch_render_loop`: Rebuilds Embree BVH each frame for animated geometry
  - `EmbreeScene::drop()`: Logs resource cleanup for memory tracking
  - Static scenes skip per-frame rebuild (no animation detected)

  **Phase 4b: Multi-Prototype Vertex Animation Fix ✅ (M19.1c)**
  - Fixed `use_multi_draw` gate blocking animation in Ivar/batch render
  - GUI Ivar: Build triangles on main thread with `build_triangles_at_time(current_time)`
  - Auto-invalidate Ivar scene cache when vertex animation frame changes
  - Both batch render and GUI Ivar now animate multi-prototype scenes

  **Phase 5: USD Light Support ✅ (M19.2)**
  - C++ bridge: UsdLux extraction (Distant, Sphere, Rect, Dome)
  - Rust FFI: `UsdLightData`, `Light` enum in scene graph
  - Viewport: Direct lighting with Cook-Torrance BRDF in shader
  - Ivar: NEE sampling for explicit lights alongside HDRI
  - Key files: `cpp/usd_bridge.cpp`, `bif_core/scene.rs`, `bif_viewport/basic.wgsl`, `bif_renderer/light.rs`

  **Phase 5b: Viewport Camera Selection (WIP) (M19.3)**
  - Camera dropdown in timeline panel (Viewport + USD cameras)
  - Lock/Free toggle for camera controls
  - Camera sync on selection and during playback
  - Timeline UI improvements (numbered frames, start/end labels)
  - Fixed RT playback stall (slider `.integer()` rounding reset anchor each frame)

  **Phase 5c: Code Quality & Robustness ✅ (M19.4)**
  - Convert Embree panics to Result-based error handling (`EmbreeError` enum)
  - Add materials vector validation (prevent underflow in `hit()`)
  - Increase viewport light limit (8 → 32, matches common DCC limits)
  - Extract `LightsManager` from monolithic `Renderer` struct
  - Extract `GnomonRenderer` from monolithic `Renderer` struct
  - Improve UsdStage thread safety documentation

  **Phase 6: Instance Transform Animation (TODO)**
  - Per-frame instance matrix evaluation
  - Test with transform-animated USD scenes

- **Key Files:**
  - `crates/bif_viewport/src/batch_render.rs` - Batch render loop,
  -  SceneBuilderData
  - `crates/bif_viewport/src/lib.rs` - UI, viewport camera sync, build_triangles_at_time
  - `crates/bif_renderer/src/embree.rs` - Embree scene with Drop logging
  - `crates/bif_core/src/usd/cpp_bridge.rs` - UNC path fix, get_mesh_vertices_at_time
  - `cpp/usd_bridge/usd_bridge.cpp` - Camera xform evaluation, vertex animation

---

### Milestone 19.6: lib.rs Cleanup 🧹

- **Goal:** Split monolithic lib.rs into maintainable modules
- **Estimated Time:** 8-10 hours
- **Key Tasks:**
  - Split `Renderer` UI drawing → `ui.rs` module
  - Split scene loading logic → `scene_loader.rs`
  - Split wgpu pipeline setup → `pipeline.rs` or similar
  - Goal: lib.rs under 2,000 lines, clear module boundaries
  - No feature changes, just structural
- **Why Now:** lib.rs at ~4,800 lines after 5 extraction passes. egui immediate-mode pushes everything into one render loop. Every future milestone adds more code here. Clean up before M20+ adds selection, gizmos, undo.

---

### Milestone 20: Scene Interactivity + Keyframing ✅

- **Completed:** 2026-02-06
- **Time Invested:** ~12 hours (2 sessions)
- **Key Achievements:**
  - Embree viewport picking (click to select instance via CPU raycast)
  - Selection highlight (orange tint via `instance_index` in WGSL shader)
  - Command-pattern undo system (`UndoStack` with `TransformCommand`, `KeyframeCommand`)
  - Translate gizmo (egui `Painter` overlay, 3-axis colored arrows)
  - Keyframing (insert/update at current frame, diamond markers on timeline)
  - Procedural primitives (cube/sphere/camera wireframe) in node graph
  - Orthographic view presets (Top/Front/Right/etc.) in camera dropdown
  - USD edit layer export (transform overrides + keyframes via C++ bridge)
  - Editable TRS fields in property inspector with live preview
  - Ctrl+Z / Ctrl+Shift+Z for undo/redo
- **New Files:**
  - `bif_core/src/undo.rs` - Undo system (EditState, UndoStack, commands)
  - `bif_core/src/primitives.rs` - Procedural geometry (cube, sphere, camera)
  - `bif_renderer/src/pick_scene.rs` - Embree pick scene for viewport selection
  - `bif_viewport/src/gizmo.rs` - Translate gizmo overlay
- **Stats:** +2,777 lines, 23 files changed, 4 new files

---

### Milestone 21: Point Instancing + Scattering ✅

- **Completed:** 2026-02-13
- **Time Invested:** ~8 hours (1 session)
- **Key Achievements:**
  - `PointCloud` first-class scene type with `expand()` → instances
  - USD PointInstancer loading creates PointCloud (preserves authoring data)
  - Scatter on surface: random (area-weighted CDF) + Poisson disk (spatial hash rejection)
  - Deterministic scatter via `StdRng::seed_from_u64()`
  - MAX_INSTANCES bumped 10K → 100K
  - Point preview renderer (wgpu PointList pipeline, cyan dots, size slider)
  - Scatter node in node graph with Compute/Regenerate buttons
  - Undo/redo for scatter operations (ScatterCommand + SceneOp variants)
  - Point cloud summary in property inspector
- **New Files:**
  - `bif_core/src/point_cloud.rs` - PointCloud type, expand(), DistributionMethod
  - `bif_core/src/scatter.rs` - scatter_on_surface(), random + Poisson disk modes
  - `bif_viewport/src/point_preview.rs` - wgpu PointList renderer
  - `bif_viewport/src/shaders/point_preview.wgsl` - Point preview shader
- **Stats:** ~2,500 lines, 13 files changed, 4 new files, 7 new tests (61 total bif_core)

---

### Milestone 21.1: Point Instancer Node ✅

- **Completed:** 2026-02-15
- **Time Invested:** ~2 hours
- **Key Achievements:**
  - Point Instancer node: 2 inputs (points + proto), resolves snarl connections for data flow
  - `resolve_input_connection()` helper — first real use of node graph wiring
  - Instance/Re-instance buttons, connection status UI
  - Instancer results stored separately (`HashMap<NodeId, Vec<Instance>>`) to avoid index fragility
  - `reload_working_scene()` appends instancer instances to GPU buffers
  - DeleteNode cleanup removes instancer results + reloads scene
  - Culling manager buffer overflow fix: clamp writes to `max_instances` capacity
  - 2 new tests, 68 total viewport tests passing
- **Files Changed:** `node_graph.rs`, `render.rs`, `scene_loader.rs`, `lib.rs`, `culling_manager.rs`

---

### Milestone 21.2: Auto-Compute + Code Review Fixes ✅

- **Completed:** 2026-02-15
- **Time Invested:** ~3 hours
- **Key Achievements:**
  - Houdini-style auto-compute: nodes cook automatically when inputs connect/change
  - Dirty propagation on connect/disconnect/delete (marks downstream nodes for recompute)
  - Scatter→Instancer chain: scatter recompute triggers instancer recompute
  - Prototype hiding: source geometry hidden when consumed by instancer
  - `expand_with_prototype()` avoids clone+modify pattern
  - BTreeMap for deterministic instancer iteration
  - Parallel array desync fix (prim_paths + animations for instancer instances)
  - Ivar rendering includes instancer instances
  - Culling truncation warning
  - 14 code review items resolved
- **Files Changed:** `point_cloud.rs`, `node_graph.rs`, `render.rs`, `scene_loader.rs`, `lib.rs`, `culling_manager.rs`

---

### Milestone 29: USD Import and Export + Non-Destructive Layers 💾 (In Progress)

- **Goal:** Close the pipeline loop: import → modify → render → **export**
- **Estimated Time:** 20-25 hours
- **Status:** Core export pipeline implemented, needs real-world validation
- **Completed:**
  - ✅ Phase 1: Instance prim path tracking (real USD paths for round-trip)
  - ✅ Phase 2: C++ bridge — sublayer, reference, default prim APIs
  - ✅ Phase 3: C++ bridge — PointInstancer write
  - ✅ Phase 4: Core `export_scene()` in bif_core (GUI-agnostic)
  - ✅ Phase 5: Display flag in node graph (Houdini-style blue flag)
  - ✅ Phase 6: UsdExport sink node + UI
  - ✅ Phase 7: 6 round-trip validation tests passing
  - ✅ Fix `write_xform` to DefinePrim(Xform) fallback on empty stages
  - ✅ Xform node (T/R/S transform SOP) with property inspector
  - ✅ Multi-USD material fix (per-material source_dir, texture resolution)
  - ✅ Display flag rendering gating (BFS upstream walk)
  - ✅ node_proto_ids for multi-proto UsdRead node tracking + cleanup
  - ✅ Phase 8: UsdPrim + GraftBranches nodes (scene assembly)
  - ✅ Phase 9: C++ bridge — define_prim + set_prim_kind (Specifier, Kind, PrimType)
  - ✅ Phase 10: Export collects source USD path from upstream UsdRead (sublayer fix)
  - ✅ Phase 11: Canonicalize paths, diagnostic logging, auto-enable sublayer
  - ✅ Phase 12: C++ `write_mesh` bridge (UsdGeomMesh with points, indices, normals, UVs, extent)
  - ✅ Phase 13: Export prototype meshes alongside PointInstancers (fixes dangling proto refs)
  - ✅ Phase 14: Prim paths on Primitive + PointInstancer nodes (auto-increment)
  - ✅ Phase 15: Primitive scene input pass-through
  - ✅ Phase 16: Material fallback fix (default grey at end of table, not index 0)
  - ✅ Phase 17: Export fix — cloud.prototype_ids, remove starts_with('/') guard, standalone mesh export
  - ✅ Phase 18: CompositeProvider — live scene graph merging USD + procedural prims
- **Remaining:**
  - Validate exported USD in Houdini/usdview (standalone cube confirmed working)
  - Documentation for USD export workflow
  - Xform prim_filter (V1 placeholder — always all upstream)
  - Ivar CPU renderer doesn't reflect Xform transforms yet
  - look in to usd asset authoring like houidinis component builder.  get some screen shots to help port it over.
  - Add "Edit Layer" concept in node graph (blue display flag) that gates export and allows non-destructive modifications on top of reference layer
  - Export validation with real-world USD scenes (beyond simple test cases)
  -  
- **Architecture:**
  - Separate "edit layer" authored on top of reference layer
  - User modifications stored as opinions, not destructive edits
  - Export produces `.usda`/`.usdc` sublayer that composes with original
  - All export logic in `bif_core` (no egui dependency) — clean Qt port path

---

### Milestone 26: Denoising (Intel OIDN) 🧹 ✅

- **Goal:** Production-quality denoising for faster convergence
- **Status:** Complete
- **Key Tasks:**
  - ✅ `Material::albedo()` trait method + impls (Lambertian, Metal, DisneyBSDF)
  - ✅ Albedo AOV pipeline: `AovData` → `BucketResultWithAovs` → `IvarState` → viewport preview
  - ✅ `AovChannel::Albedo` viewport preview with gamma-corrected display
  - ✅ `denoise.rs` module: `denoise_beauty()` with beauty/albedo/normal guide buffers
  - ✅ Feature-gated `oidn` crate (`--features oidn`, propagated through all crates)
  - ✅ Viewport "Denoise (OIDN)" button after render complete
  - ✅ Batch render denoise checkbox (denoise before EXR write)
  - ✅ Stub when OIDN disabled (grayed-out UI, `DenoiseError::NotEnabled`)
  - ✅ Tests: dimension mismatch, material albedo, buffer lifecycle, denoise state

---

### Milestone 26.1: Ivar Material Cache + Pre-warm ⚡ ✅

- **Completed:** 2026-03-12
- **Goal:** Cache built DisneyBSDF materials across Ivar builds; pre-warm on scene load
- **Problem:** TextureCache created fresh every Ivar build → 6.7s texture loading each time
- **Solution (unified approach — 3 parts):**
  1. **`ivar_materials` cache** on Renderer — `Vec<Arc<DisneyBSDF>>` persists across builds
  2. **Channel return** — background build thread sends materials back alongside BVH for caching
  3. **Pre-warm on scene load** — `prewarm_ivar_materials()` spawns background thread to build materials immediately after scene load, so first Ivar build is also fast
- **Invalidation:** Materials cleared only on scene reload or material edit, NOT on camera/transform/geometry changes
- **Batch render:** `SceneBuilderData` carries `ivar_materials` for per-frame reuse
- **Performance:**
  | Scenario | Before | After |
  |----------|--------|-------|
  | First Ivar (cold) | 6.7s | 6.7s |
  | First Ivar (pre-warmed) | 6.7s | ~47ms |
  | Subsequent rebuilds | 6.7s | ~47ms |
  | Batch render per frame | 6.7s | ~47ms |
- **Key Files:**
  - `crates/bif_viewport/src/lib.rs` — `ivar_materials`, `ivar_materials_receiver` fields
  - `crates/bif_viewport/src/ivar_build.rs` — cache usage, `prewarm_ivar_materials()`, `invalidate_ivar_materials()`
  - `crates/bif_viewport/src/batch_render.rs` — `SceneBuilderData.ivar_materials`
  - `crates/bif_viewport/src/scene_loader.rs` — prewarm + invalidate calls

---

### Milestone 25: Volumes + OpenVDB 🌫️

- **Goal:** Render fog, smoke, clouds, and VDB volumes
- **Estimated Time:** 20-30 hours
- **Key Tasks:**
  - OpenVDB integration via C++ bridge
  - UsdVolume support (load from USD)
  - Null-scattering path integral formulation
  - Equi-angular sampling for point lights in media
  - Delta tracking for heterogeneous volumes
  - Viewport volume preview (ray marching)
- **Reference:** Arnold papers on participating media

---

### Milestone 22: Viewport Performance ⚡

- **Goal:** Clarisse-like lazy loading and GPU optimization
- **Estimated Time:** 20-30 hours
- **Already Done:** Frustum culling ✅, LOD system ✅, polygon budget ✅
- **Why Deferred:** Not hitting viewport limits yet. This is optimization, not features.
- **Remaining Tasks:**
  - Upgrade to Vulkan 1.3 features:
    - Dynamic rendering (simplify render passes)
    - Buffer device address (bindless buffers)
    - Descriptor indexing (bindless textures)
    - Synchronization2 (cleaner barriers)
  - Lazy geometry loading (load on demand)
  - Async texture streaming
  - GPU-driven rendering (indirect draws)
- **Reference:** [howtovulkan.com](https://howtovulkan.com) - Modern Vulkan patterns

---

### Milestone 23: SHARC Radiance Cache ✅

- **Completed:** 2026-02-20
- **Time Invested:** ~4 hours
- **Goal:** Spatially Hashed Radiance Cache (idTech 8 inspired) for secondary bounce reuse
- **Key Achievements:**
  - `RadianceCache` with dual backend: lock-free `AtomicU32::from_ptr` (default) + 64-shard `RwLock` fallback (GPU-compatible `#[repr(C)]` layout)
  - Spatial hash with dominant-axis normal disambiguation (6 directions, prevents floor/ceiling leak)
  - EMA blending for cache updates, staleness eviction via `max_age` frames
  - Cache READ in bounce loop: skips remaining bounces when cached radiance available
  - Cache WRITE: stores surface-local radiance (emission + NEE) after each hit
  - Russian Roulette path termination (bounce >= 3, throughput-proportional survival)
  - `auto_cell_size()` from scene AABB for sensible defaults
  - IPR integration: cache persists across progressive passes, clears on camera move
  - Batch integration: cache persists within frame, clears per-frame for animated scenes
  - Cache Heatmap AOV (sample count visualization: black → red → yellow → green)
  - egui controls: enable/disable, cell size, buffer size, min samples, min bounce, hit rate/occupancy stats
  - Lock-free path: CAS on `sample_count` guards writes, atomic loads for reads, zero contention
  - A/B/C benchmark (`cache_bench`): OFF vs RwLock vs lock-free with energy validation
  - 18 unit tests for hash, cache, concurrency, staleness, lock-free CAS contention
- **Key Files:**
  - `crates/bif_renderer/src/radiance_cache.rs` (NEW) — core SHARC implementation
  - `crates/bif_renderer/src/renderer.rs` — cache integration + Russian Roulette
  - `crates/bif_viewport/src/ivar_state.rs` — cache config + AOV channel
  - `crates/bif_viewport/src/ivar_build.rs` — cache lifecycle + heatmap piping
  - `crates/bif_viewport/src/batch_render.rs` — batch cache lifecycle
- **Reference:** idTech 8 GI talk, GI_ID_METHOD.md

---

### Milestone 27: GPU Path Tracing (wgpu Compute) ⚡

- **Goal:** Massively parallel path tracing on GPU
- **Estimated Time:** 30-40 hours
- **Key Tasks:**
  - wgpu compute shader path tracer
  - GPU BVH construction and traversal
  - ReSTIR (basic reservoir sampling first)
  - Spatiotemporal resampling (full ReSTIR)
  - Shared memory optimizations
  - Wavefront path tracing architecture
- **Why:** 10-100x speedup over CPU, near-real-time quality

---

### Milestone 28: Qt 6 UI Integration 🖥️

- **Goal:** Replace egui with Qt 6 for production-grade UI
- **Estimated Time:** 50+ hours
- **Why Last:** egui is functional for development. Qt matters when other people use BIF.
- **Key Tasks:**
  - Qt 6 via cxx-qt (C++ ↔ Rust bridge)
  - Embed wgpu viewport in Qt widget
  - QDockWidget - true floating/docking panels
  - QTreeView with model/view separation
  - Professional node editor (QGraphicsScene)
  - QMenuBar, QToolBar, QShortcut - standard DCC conventions
  - QUndoStack integration (replace simple undo stack from M20)
---

### Milestone 28.1: Integrated Asset Browser (File Browser) 📂

- **Goal:** Dockable panel for browsing project directories, previewing assets, and drag-and-drop into BIF
- **Prerequisites:** M28 (Qt 6 UI) complete
- **Estimated Time:** 20-25 hours
- **Architecture:**
  - `bif_core::asset_browser` — UI-agnostic core: directory scanning, format registry trait, thumbnail generation, asset metadata
  - `AssetFormat` trait — extensible format registration (ships with USD, image textures, HDRI, OBJ)
  - `AssetThumbnail` — decode first mip for textures/HDRIs, type icons for USD/unknown
  - Qt panel: directory tree (left) + file grid with thumbnails (right)
- **Key Tasks:**

  **Format Registry**
  - `AssetFormat` trait: `extensions()`, `icon()`, `can_thumbnail()`, `generate_thumbnail()`, `node_type()`
  - Ships with: USD (.usd/.usda/.usdc), textures (.exr/.tx/.png/.jpg), HDRI (.hdr/.exr), OBJ (.obj)
  - New formats added without touching browser code

  **Directory Scanner**
  - Async recursive scan with background thread
  - File watcher for live updates (notify crate)
  - Respect `.bifignore` for excluding paths

  **Thumbnail Pipeline**
  - Background thread pool for thumbnail generation
  - Thumbnail cache on disk (`.bif_thumbs/`)
  - Texture/HDRI decode via existing image crate / OIIO
  - Type icons for USD and unknown formats

  **Drag-and-Drop — Node Graph**
  - Drop creates appropriate node (UsdRead, HdriEnvironment, etc.) at cursor position
  - Format registry maps file type → node type

  **Drag-and-Drop — Viewport**
  - Drop USD at raycast hit point (reuse Embree pick infrastructure from M20)
  - Creates UsdRead + Xform positioned at hit location

  **Drag-and-Drop — Panels**
  - Drop texture onto material slot in property inspector
  - Drop HDRI onto environment panel

  **Panel UI**
  - Path breadcrumb bar with navigation
  - Grid/list view toggle
  - Sort by name/date/size/type
  - Filter by format

- **Reference:** Clarisse iFX browser, Houdini file chooser, Blender asset browser

---

### Milestone 28.2: Asset Library + Catalog 📚

- **Goal:** SQLite-backed asset catalog with tagging, search, favorites, and collections
- **Prerequisites:** M28.1 (Asset Browser) complete
- **Estimated Time:** 15-20 hours
- **Architecture:**
  - `bif_core::asset_library` — SQLite via `rusqlite`, FTS5 for full-text search
  - Assets registered from browser (right-click → "Add to Library") or bulk import
  - Schema: assets, tags (many-to-many), collections, smart collections (saved searches)
- **Key Tasks:**

  **SQLite Schema + CRUD**
  - Assets table: path, format, tags, metadata, thumbnail_hash, created, modified
  - Tags with many-to-many junction table
  - Collections and smart collections (saved filter queries)

  **Asset Registration**
  - Scan directory → register all discovered assets
  - File watcher integration (from M28.1) for auto-registration
  - Bulk import with progress

  **Search + Filtering**
  - FTS5 full-text search on name + tags
  - Filter by format, collection, date range
  - Search bar with tag autocomplete

  **Collections**
  - User-created collections (manual grouping)
  - Smart collections (saved search queries, auto-populate)
  - Drag assets between collections
  - Favorites (starred assets, quick access)

  **UI Integration**
  - Toggle between file browser / library view in same panel
  - Tag editor (inline tag chips, add/remove)
  - Search bar with autocomplete in panel header

- **Reference:** Blender asset browser (SQLite), Substance Source, Megascans plugin
---

### Milestone 29.5: egui UI Overhaul ✅

- **Status:** Complete (Mar 23, 2026)
- **Goal:** Modernize egui UI before Qt migration — theme, layout, discoverability
- **What was done:**
  - Centralized theme system (theme.rs) with 28 color constants + apply_theme()
  - Panel restructure: scene browser promoted to top of left panel, stats to viewport overlay
  - Menu bar: File/View/Render with Ctrl+O shortcut
  - Node params moved from inline show_body() to property inspector (all 10 types)
  - Unicode geometric icons replacing emoji, tooltips everywhere, welcome screen
  - Node selection via header click with accent highlight
  - Removed dead show_ui toggle, extracted open_usd_file_dialog helper
- **Remaining for future:** egui 0.30 upgrade + vertical node layout (snarl 0.6+), middle-mouse pan (needs snarl fork or upgrade)

---

### Milestone 30: Node Graph Persistence & Evaluation Modes 💾 ✅

- **Goal:** Save/load node graphs + Houdini-style evaluation control
- **Estimated Time:** 15-20 hours
- **Why:** Foundation — everything else is lost without save. Eval modes prevent unnecessary recomputation.
- **Save/Load:**
  - `#[derive(Serialize, Deserialize)]` on `SceneNode` enum + `NodeGraphState` + key types
  - Serialize `Snarl<SceneNode>` graph (nodes, connections, positions)
  - Dual format: `.bif` binary (bincode) for fast load + `.bifa` ascii (JSON pretty-print) for human-readable/diffable
  - File → New / Open / Save / Save As menu, recent files list, dirty flag, unsaved changes prompt
  - Stores: node graph, HDRI path, viewport camera, render settings
  - Does NOT store: USD stage data (referenced by path), rendered images
- **Evaluation Modes:**
  - Auto: current behavior (immediate re-eval on any change)
  - Manual: explicit "cook" button, dirty nodes shown visually
  - On-Mouse-Release: re-eval when slider/drag released (not during)
  - Toggle in node graph toolbar
- **Cache Node:** New `SceneNode::Cache` variant
  - Stores evaluated scene snapshot at that point in chain
  - Bypass toggle (re-evaluate through cache vs use cached)
  - Visual indicator (green = cached, yellow = stale)
  - Writes `.bif_cache` files to disk for persistent caching

---

### Milestone 31: Per-Node Scene Graph Visualization 🌲

- **Goal:** Click any node → see scene graph tree at that evaluation point
- **Estimated Time:** 10-15 hours
- **Why:** Makes the node workflow click — users can see what each node contributes
- **Already Built (80%):** CompositeProvider, CachedSceneGraph, scene browser, PrimDataProvider trait
- **Key Tasks:**
  - Per-node evaluation: click node → evaluate chain up to that node only → show resulting tree
  - Highlight prims added/modified by selected node (color-coded in scene browser)
  - Dual-panel layout: node graph + live scene graph tree side-by-side
  - Node contribution markers: each prim tagged with which node produced it
  - Evaluation display: badge/overlay on each node showing prim count it contributes

---

### Milestone 32: USD Composition Inspector & Opinion Trace 🔍

- **Goal:** Trace USD attribute opinions — show which layer "wins" and why
- **Estimated Time:** 15-20 hours
- **Why:** Essential for a USD-focused tool. Without this, debugging composition is guesswork.
- **Reference:** [havocado/usd-opinion-trace](https://github.com/havocado/usd-opinion-trace) (reimplement natively, don't clone)
- **C++ Bridge Extensions:**
  - Expose `PcpPrimIndex` — composition arcs per prim
  - Expose `SdfLayerStack` — layer ordering
  - Expose opinion queries — which layer sets which attribute
- **Composition Inspector Panel:**
  - Tree view of composition arcs (references, payloads, sublayers, inherits, variants)
  - Click arc → jump to source layer/prim
- **Opinion Trace Panel:**
  - Select attribute → see full opinion stack (all layers)
  - Winning opinion highlighted, blocking reason explained
  - LIVRPS ordering visualized

---


- **Goal:** usdview-parity debugging: variants, layers, metadata, namespace editing
- **Estimated Time:** 10-15 hours
- **Key Tasks:**
  - **Variant Set Selector:** Interactive variant switching in property inspector
  - **Layer Stack Viewer:** Which layers contribute to selected prim
  - **Prim Metadata Inspector:** kind, purpose, apiSchemas, custom data
  - **Namespace Editor:** Rename/reparent prims (writes to edit layer)

---

### Milestone 34: Pipeline Integration (Python Hooks) 🐍

- **Goal:** Embedded Python (PyO3) for pipeline extensibility
- **Estimated Time:** 15-20 hours
- **Why:** Every VFX DCC ships embedded Python. BIF should integrate with studio pipelines, not replace them.
- **Key Tasks:**
  - **Embedded Python (PyO3):** Python interpreter in BIF process
  - Expose bif_core types to Python (Scene, Prim, Material, etc.)
  - **Hook points:** pre-load, post-load, pre-export, post-export, pre-render, post-render
  - **Configurable paths:** Template-based output paths (`$SHOW/$SHOT/$ASSET/...`)
  - **Metadata propagation:** Custom USD attributes carried through pipeline
- **Approach:** Bundle Python interpreter (like Houdini hython, Maya mayapy). PyO3 static linking. ~30-50MB dist size.

---

### Milestone 35: Public API Cleanup (Framework Phase 1) 📚

- **Goal:** Make bif_core/bif_renderer usable as Rust libraries
- **Estimated Time:** 10-15 hours
- **Why:** Foundation for "Arch Linux for VFX" framework vision
- **Key Tasks:**
  - **bif_core:** Clean public API, doc comments on all public items, examples
  - **bif_renderer:** Clean entry point for headless rendering, examples
  - **bif_math:** Already clean, add examples
  - Internal only but crates.io-ready code quality

---

### Milestone 36+: Framework Phase 2 (Widget Extraction) 🧩

- **Goal:** Extract reusable UI crates + dynamic node registry + DCC connectors
- **Estimated Time:** 40+ hours (multi-milestone)
- **Widget Extraction:**
  - `bif_node_graph` — configurable node graph widget
  - `bif_scene_browser` — scene browser widget
  - `bif_viewport_3d` — 3D viewport widget
- **Dynamic Node Registry:** Trait objects replacing `SceneNode` enum, `.dll`/`.so` plugin loading
- **Houdini Live-Link:** Shared USD stage over network (UsdUtilsStageCache), first DCC connector
- **Python Bindings:** PyO3 bindings for bif_core/bif_renderer

---

### Milestone 37: Lights Authoring 💡

- **Goal:** Light creation UI + graph nodes for scene lighting without USD
- **Estimated Time:** 10-15 hours
- **Why:** Renderer already has full light support — just need creation UI + graph nodes.
- **Key Tasks:**
  - Light graph nodes: DistantLight, PointLight, RectLight (output Scene pin)
  - Property inspector: edit color, intensity, position, radius, angle
  - Viewport light visualization (wireframe icons)
  - Lights in scene browser hierarchy
  - Export lights to USD
- **Existing Code:** `bif_renderer/src/light.rs` (DistantLight, SphereLight, RectLight + NEE/MIS)

---

### Milestone 38: Material Authoring 🎨

- **Goal:** Property editor + basic shader graph for material creation
- **Estimated Time:** 20-25 hours
- **Key Tasks:**
  - Material creation: presets (diffuse, metal, glass, emissive)
  - Property inspector: PBR params (diffuse color, roughness, metallic, specular, textures)
  - Texture slot assignment (file picker)
  - Material assignment to prims
  - Material library panel
  - Basic shader graph in Materials context: simple nodes (Mix, Multiply, Texture, Output)
  - Export authored materials to USD (UsdPreviewSurface)
- **Existing Code:** `bif_core/src/scene.rs` Material struct, `bif_renderer/src/material.rs`

---

### Milestone 39: Context System 🔀

- **Goal:** Houdini-style contexts — biggest refactor, needs M30+ first
- **Estimated Time:** 30-40 hours
- **Key Tasks:**
  - Refactor `SceneNode` into context-specific enums or trait-based system
  - **Assembly context** (current graph): UsdRead, Scatter, Instancer, Xform, Export
  - **Materials context**: shader graph nodes from M38, expanded
  - **Animation context**: timeline-focused, keyframe/expression nodes
  - Context switcher UI (tabs or dropdown)
  - Each context has its own `Snarl` graph
  - Shared data flows between contexts

---

### Milestone 40: MaterialX Authoring 🧪

- **Goal:** Full MaterialX `standard_surface` node graph with round-trip XML export
- **Estimated Time:** 25-30 hours
- **Why:** M38 covers basic PBR presets — this is the full shader graph experience in the Materials context (M39)
- **Key Tasks:**
  - Full `standard_surface` node graph (beyond M38 presets)
  - MaterialX node types: Math, Color, Texture, Noise, Normal map, etc.
  - MaterialX XML export/import (round-trip)
  - Connect to existing MaterialX parsing from M16
  - Node preview thumbnails (sphere render per node)
  - Layer/blend material stacking
- **Existing Code:** `bif_core/src/materialx.rs` (M16 parser), `bif_renderer/src/material.rs`

---

### ~~Milestone 24: Spectral Rendering~~ (Cut)

> **Status:** Cut from roadmap — zero production value for BIF's goals.
> Re-add as a learning exercise if desired, but not on the critical path.
>
> Original scope: Spectral path tracing, hero wavelength sampling, dispersion, fluorescence.

---

## Milestone Roadmap Summary

### Completed

| # | Milestone | Focus | Status |
|---|-----------|-------|--------|
| 0-13b | Foundation | Math, viewport, USD, Embree, UI | ✅ Complete |
| 14 | GPU Instancing | 10K+ instances + frustum culling + LOD | ✅ Complete |
| 15 | Materials | UsdPreviewSurface + Disney BSDF | ✅ Complete |
| 16 | MaterialX | MaterialX standard_surface support | ✅ Complete |
| 17 | Viewport PBR | Textured PBR in Vulkan viewport | ✅ Complete |
| 17.1 | OIIO/.tx | OpenImageIO texture pipeline (feature-gated) | ✅ Complete |
| 18 | Animation | Time-sampled USD + timeline UI | ✅ Complete |
| 18.1-18.5 | Animation Polish | Thread safety, USD fixes, multi-prototype | ✅ Complete |
| 19-21.2 | Frame Rendering + Interactivity | Batch render, animation, picking, scatter | ✅ Complete |
| 23 | SHARC Radiance Cache | idTech 8 cache + Russian Roulette + heatmap AOV | ✅ Complete |
| 26 | OIDN Denoising | Intel OIDN + albedo AOV pipeline | ✅ Complete |
| 26.1 | Ivar Material Cache | Pre-warm + persist DisneyBSDF materials | ✅ Complete |

### Active & Planned

| Order | # | Milestone | What it unlocks |
|-------|---|-----------|-----------------|
| 1 | 29 | USD export (in-progress) | Full pipeline: import→modify→render→export |
| 2 | 29.5 | egui 0.30 + vertical nodes | Sandwich layout, prerequisite for M30 |
| 3 | 30 | Node persistence + eval modes | Save/load `.bif`/`.bifa`, cache node, cook modes |
| 4 | 31 | Per-node scene graph viz | Click node → see tree at that eval point |
| 5 | 32 | Composition inspector + opinion trace | USD debugging: which layer wins and why |
| 6 | 33 | USD debugging tools | Variants, layer stack, metadata, namespace |
| 7 | 34 | Pipeline integration (PyO3) | Embedded Python hooks for studio pipelines |
| 8 | 35 | Public API cleanup | bif_core/bif_renderer as Rust libraries |
| 9 | 36+ | Framework extraction | Widget crates, plugin system, DCC connectors |
| 10 | 37 | Lights authoring | Lit scenes without USD, viewport light viz |
| 11 | 38 | Material authoring | PBR editing + basic shader graph |
| 12 | 39 | Context system | Assembly/Materials/Animation contexts |
| 13 | 40 | MaterialX authoring | Full standard_surface node graph + XML round-trip |
| — | 25 | Volumes/OpenVDB | Smoke, fog, clouds |
| — | 22 | Viewport perf | Handle production scenes |
| — | 27 | GPU path tracing | Near-realtime quality |
| — | 28 | Qt 6 UI | Professional interface |

### Dissolved / Cut

| # | Milestone | Status | Reason |
|---|-----------|--------|--------|
| 24 | Spectral Rendering | Cut | Zero production value for BIF's goals |
| — | MDL Support | Skipped | MaterialX is ASWF standard; MDL is Nvidia-only. Convert on export if needed |

---

## Research References

### Arnold Research Papers

Key papers (cherry-pick into relevant milestones as needed):

| Paper | Year | Application |
|-------|------|-------------|
| Blue-Noise Dithered Sampling | 2016 | Perceptually better noise |
| BSSRDF Importance Sampling | 2013 | Subsurface scattering |
| Robust BVH Ray Traversal | 2013 | Numerical stability |
| Area-Preserving Spherical Rectangles | 2013 | Soft shadows |
| Importance Sampling in Participating Media | 2012 | Volume rendering |
| Variance-Aware MIS | 2019 | Reduce fireflies |
| Null-Scattering Path Integral | 2019 | Heterogeneous volumes |
| Specular Manifold Sampling | 2020 | Caustics and glints |

**Source:** [Arnold Research Papers](https://blogs.autodesk.com/media-and-entertainment/2024/01/04/autodesk-arnold-research-papers/)

### Other Key References

| Resource | Topics |
|----------|--------|
| [PBR Book](https://pbr-book.org/) | Comprehensive rendering theory |
| [Ray Tracing Gems 1 & 2](https://www.realtimerendering.com/raytracinggems/) | Practical GPU techniques |
| Disney Principled BSDF | Industry-standard material model |
| [howtovulkan.com](https://howtovulkan.com) | Modern Vulkan 1.3 patterns |
| Intel OIDN | Production denoising |
| NVIDIA ReSTIR | Real-time path tracing |

---

## Milestone Organization Principles

1. **Complete one milestone before starting the next** - No partial work
2. **Each milestone must be testable and demoable** - Visual proof or test coverage
3. **Milestones build on each other** - Later milestones depend on earlier foundation
4. **Deferred != Canceled** - Just prioritizing core features first
5. **Time estimates are guidelines** - Side project, 10-20 hrs/week realistic
6. **Document learnings in devlogs** - Each milestone gets a devlog entry

---

**Last Updated:** March 13, 2026
**Status:** Milestones 0-23 + M26 + M26.1 complete
**Current:** M29 USD export (in-progress)
**Next:** M29.5 (egui upgrade) → M30 (persistence/eval) → M31 (per-node viz) → M32 (opinion trace) → M33 (USD debug) → M34 (Python hooks) → M35 (API cleanup) → M36+ (framework)
