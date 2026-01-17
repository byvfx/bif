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

## Summary Statistics (Milestones 0-13b)

| Metric | Value |
|--------|-------|
| **Total LOC** | ~7,500 |
| **Tests Passing** | 60+ ✅ |
| **Milestones Complete** | 13b + Freeze Fix |
| **Time Invested** | ~55 hours |
| **Commits** | 60+ |
| **Build Time (dev)** | ~5s |
| **Build Time (release)** | ~2m |
| **Runtime FPS** | 60+ (VSync-limited) |
| **Lucy Vertices** | 140,278 |
| **Lucy Indices** | 840,768 |
| **Instances Rendered** | 100 (GPU), 100 (Ivar/Embree) |
| **Total Triangles** | 28,055,600 |
| **Draw Calls** | 1 (instanced) |
| **Embree BVH Build** | 28ms |
| **UI Freeze** | **0ms** ✅ |

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

### Milestone 16: MaterialX Support 🎨

- **Goal:** Import MaterialX materials from USD and render with proper shading
- **Prerequisites:** Milestone 15 complete ✅
- **Status:** In Progress (awaiting test asset)
- **Key Achievements:**

  **Phase 1-4: C++ Bridge MaterialX Support ✅**
  - `is_materialx_standard_surface()` detection for `ND_standard_surface_*` shaders
  - `get_materialx_texture_path()` for `ND_image_*` texture nodes
  - Full standard_surface property extraction:
    - `base_color` → diffuse_color ✅
    - `metalness` → metallic ✅
    - `specular_roughness` → roughness ✅
    - `emission_color` × `emission` → emissive_color ✅
    - `opacity` (vec3 or scalar) → opacity ✅
    - `specular` → specular ✅
  - Automatic fallback: MaterialX → UsdPreviewSurface → default gray
  - `is_materialx` flag exposed via FFI

  **Phase 5: Rust FFI Integration ✅**
  - `UsdMaterialData.is_materialx: bool` field added
  - Logging when MaterialX materials detected
  - Materials flow to existing Disney BSDF unchanged

  **Phase 6: Validation (Pending)**
  - Awaiting Houdini-exported MaterialX test asset
  - Validate rendering in Ivar

- **Reference:** MaterialX Specification, USD MaterialX Schema

---

### Milestone 17: Viewport PBR + Textures 🖼️

- **Goal:** Textured PBR materials in Vulkan viewport
- **Prerequisites:** Milestone 16 complete
- **Key Tasks:**
  - GPU texture upload (texture array or bindless)
  - Texture bind group in render pipeline
  - Update basic.wgsl for texture sampling
  - Material ID per instance
  - Basic PBR lighting (metallic/roughness)
  - Normal mapping (stretch goal)
- **Reference:** wgpu texture examples, LearnOpenGL PBR

---

### Milestone 18: Animation + Motion Blur 🎬

- **Goal:** Load and render time-sampled USD data with motion blur
- **Estimated Time:** 15-20 hours
- **Key Tasks:**
  - Parse time-sampled attributes (`xformOp:translate.timeSamples`)
  - Timeline UI widget (frame slider, play/pause, frame range)
  - Animate transforms in Vulkan viewport
  - Motion blur in Ivar renderer (transformation + deformation)
  - Support `UsdSkelAnimation` basics (stretch goal)
- **Reference:** Arnold paper "Motion Blur Corner Cases"

---

### Milestone 19: Frame Rendering 🎞️

- **Goal:** Render animated sequences to disk
- **Estimated Time:** 8-12 hours
- **Key Tasks:**
  - Frame range UI (start/end/step)
  - Batch render loop with frame substitution
  - Progress tracking with cancellation
  - Output naming patterns (`render.####.exr`)
  - EXR output with AOVs (beauty, depth, normals)

---

### Milestone 20: Scene Interactivity + Keyframing 🎮

- **Goal:** Clarisse-style object manipulation and animation authoring
- **Estimated Time:** 15-20 hours
- **Key Tasks:**
  - Selection system (click raycast through Embree)
  - Transform gizmos (translate/rotate/scale)
  - Undo/redo stack for transforms
  - Keyframe transforms at current frame
  - Animate TRS (translation, rotation, scale) over time
  - Timeline integration (scrub to see animated transforms)
  - Export modified transforms back to USD layer

---

### Milestone 21: Point Instancing + Scattering 🌲

- **Goal:** Massive instancing via point clouds and scattering tools
- **Estimated Time:** 15-20 hours
- **Key Tasks:**
  - UsdGeomPointInstancer support (load from USD)
  - Point Instancer node in node graph
  - Scatter points on surface (random/Poisson disk)
  - Paint points tool (brush-based placement)
  - Per-point attributes (scale, rotation, ID)
  - Viewport preview of point clouds
  - Instance geometry onto points
- **Why Important:** Core BIF use case - scatter millions of instances

---

### Milestone 22: Viewport Performance ⚡

- **Goal:** Clarisse-like lazy loading and GPU optimization
- **Estimated Time:** 20-30 hours
- **Key Tasks:**
  - Upgrade to Vulkan 1.3 features:
    - Dynamic rendering (simplify render passes)
    - Buffer device address (bindless buffers)
    - Descriptor indexing (bindless textures)
    - Synchronization2 (cleaner barriers)
  - LOD/proxy system for distant objects
  - Lazy geometry loading (load on demand)
  - Frustum culling on CPU before GPU submit
  - Async texture streaming
  - GPU-driven rendering (indirect draws)
- **Reference:** [howtovulkan.com](https://howtovulkan.com) - Modern Vulkan patterns

---

### Milestone 23: Renderer Polish (Arnold-Inspired) 🔬

- **Goal:** Production-quality rendering techniques from research
- **Estimated Time:** 20-30 hours
- **Key Tasks (from Arnold research papers):**
  - Blue-noise dithered sampling (perceptually cleaner noise)
  - Variance-aware MIS (smarter sampler combining)
  - Robust BVH ray traversal (numerical stability)
  - BSSRDF importance sampling (subsurface scattering)
  - Specular manifold sampling (caustics, glints)
  - Area light importance sampling (soft shadows)
- **Reference:** [Arnold Research Papers](https://blogs.autodesk.com/media-and-entertainment/2024/01/04/autodesk-arnold-research-papers/)

---

### Milestone 24: Spectral Rendering 🌈

- **Goal:** Full wavelength simulation for accurate light behavior
- **Estimated Time:** 15-20 hours
- **Key Tasks:**
  - Spectral path tracing (wavelength arrays vs RGB)
  - Hero wavelength sampling for efficiency
  - Accurate dispersion (prisms, diamonds)
  - Fluorescence support (optional)
  - `--spectral` flag for reference renders
- **Why:** Ground truth for USD/MaterialX validation, scientific accuracy

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

### Milestone 26: Denoising (Intel OIDN) 🧹

- **Goal:** Production-quality denoising for faster convergence
- **Estimated Time:** 10-15 hours
- **Key Tasks:**
  - Intel Open Image Denoise integration
  - AOV outputs (albedo, normal) for denoiser input
  - Interactive denoising during progressive render
  - Final frame denoising
  - Preserve detail in denoised output

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

### Milestone 28+: Qt 6 UI Integration (Deferred)

- **Goal:** Replace egui with Qt 6 for production-grade UI
- **Status:** Deferred until core features complete
- **Estimated Time:** 50+ hours
- **Key Tasks:**
  - Qt 6 via cxx-qt (C++ ↔ Rust bridge)
  - Embed wgpu viewport in Qt widget
  - Docking windows, menus, shortcuts
  - Professional node editor
  - Outliner, property editor, timeline

---

### Milestone 29+: USD Export (Deferred)

- **Goal:** Write scene changes back to USD
- **Estimated Time:** 15-20 hours
- **Key Tasks:**
  - USD stage authoring via C++ bridge
  - Export modified transforms as USD layer
  - Export scattered points as PointInstancer
  - Non-destructive layer workflow

---

## Milestone Roadmap Summary

| # | Milestone | Focus | Status |
|---|-----------|-------|--------|
| 0-13b | Foundation | Math, viewport, USD, Embree, UI | ✅ Complete |
| 14 | GPU Instancing | 10K+ instances + frustum culling + LOD | ✅ Complete |
| 15 | Materials | UsdPreviewSurface + Disney BSDF | ✅ Complete |
| 16 | MaterialX | MaterialX standard_surface support | 🔄 In Progress |
| 17 | Viewport PBR | Textured PBR in Vulkan viewport | Planned |
| 18 | Animation | Time-sampled USD + motion blur | Planned |
| 19 | Frame Rendering | Batch render to disk | Planned |
| 20 | Interactivity | Move objects + keyframing | Planned |
| 21 | Point Instancing | Scatter + paint tools | Planned |
| 22 | Viewport Perf | Vulkan 1.3, lazy loading | Planned |
| 23 | Renderer Polish | Arnold-inspired techniques | Planned |
| 24 | Spectral | Wavelength simulation | Planned |
| 25 | Volumes | OpenVDB + fog/smoke | Planned |
| 26 | Denoising | Intel OIDN | Planned |
| 27 | GPU Path Tracing | wgpu compute + ReSTIR | Planned |
| 28+ | Qt 6 UI | Production interface | Deferred |
| 29+ | USD Export | Write back to USD | Deferred |

---

## Research References

### Arnold Research Papers

Key papers for Milestone 20 (Renderer Polish):

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

**Last Updated:** January 16, 2026
**Status:** Milestones 0-15 Complete ✅, M16 In Progress
**Current:** MaterialX standard_surface parsing implemented, awaiting test asset
**Next:** Validate M16 with Houdini-exported MaterialX USD
