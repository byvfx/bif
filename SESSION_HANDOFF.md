# Session Handoff - January 10, 2026

**Last Updated:** Performance optimizations from code review
**Next Milestone:** 15 (Materials/MaterialX)
**Project:** BIF - VFX Scene Assembler & Renderer

---

## Quick Status

| Status | Details |
|--------|---------|
| ✅ Complete | Milestones 0-14 + perf optimizations |
| 🎯 Next | Milestone 15 (Materials/MaterialX) |
| 📦 Tests | 89 passing |
| 🚀 Performance | 60 FPS viewport, 10K instances with LOD |

---

## Recent Work

### Performance Optimizations (Jan 10, 2026)

**Source:** vfx-code-reviewer agent analysis of Milestone 14

**Completed:**
- ✅ Pre-allocate culling scratch buffers (~80MB/sec allocation savings)
- ✅ Partial sort → O(n) partitioning via `select_nth_unstable`
- ✅ AABB transform: zero heap allocation
- ✅ Frustum caching when camera static
- ✅ Instance count warning when exceeding buffer capacity

**Deferred:**
- [ ] Merge instance buffers (modest benefit at current scale)

### Milestone 14: GPU Instancing Optimization ✅ (Jan 9, 2026)

**Goal:** Enable 10K+ instances with smart LOD system

**Implementation:**
- Frustum culling via Gribb/Hartmann plane extraction (bif_math::Frustum)
- Dynamic instance buffer with COPY_DST (per-frame visible upload)
- Distance-sorted LOD with polygon budget control
- Dual draw calls for near/far instances
- Polygon budget slider (0.1M-100M triangles, logarithmic scale)
- Budget percentage indicator in Scene Stats panel

**Files:**
- [frustum.rs](crates/bif_math/src/frustum.rs) - NEW ~200 LOC
- [aabb.rs](crates/bif_math/src/aabb.rs) - Added center(), min_point(), max_point()
- [lib.rs](crates/bif_viewport/src/lib.rs) - LOD buffers, update_visible_instances()

**Test:** lucy_10000.usda loads and renders at 60 FPS with LOD

---

### Critical Bug Fixes ✅ (Jan 9, 2026 - Earlier)

**vfx-code-reviewer agent identified 3 critical issues:**

1. **Color Overflow** - white rendered as black (256→255)
2. **Embree Crash** - no fallback when DLL missing
3. **Memory Management** - USD bridge lacked cleanup

**Fixes Applied:**
- renderer.rs: 255.0 * color (was 256.0)
- embree.rs: try_new() with graceful fallback
- usd_bridge.cpp: destructor + clear_cache()

**Files:**
- [renderer.rs](crates/bif_renderer/src/renderer.rs:101-103)
- [embree.rs](crates/bif_renderer/src/embree.rs:276-290)
- [usd_bridge.cpp](cpp/usd_bridge/usd_bridge.cpp:67,312)

### Bug Fix: USD Winding Order ✅ (Jan 9, 2026)

**Issue:** USD meshes rendering inside-out/incorrectly in viewport

**Root Cause:**

- USD uses **clockwise (CW)** vertex winding
- BIF configured for counter-clockwise (CCW)
- Caused backface culling to cull wrong faces + inverted normals

**Fix:**

- Changed wgpu `front_face: FrontFace::Cw`
- Updated normal computation: `edge2 × edge1` (CW) instead of `edge1 × edge2`
- Fixed in mesh.rs, triangle.rs, lib.rs
- Updated tests for CW convention

**Tools Added:**

- `debug_usd_mesh` binary - inspects USD geometry for debugging
- `test.ps1` - runs tests with USD DLLs loaded

**Validation:**

- Both water surface (no normals) + walker scan (has normals) render correctly
- All tests pass (17 core + 34 math)

**Files:**

- [mesh.rs](crates/bif_core/src/mesh.rs) - Normal computation
- [triangle.rs](crates/bif_renderer/src/triangle.rs) - Raytracer normals
- [lib.rs](crates/bif_viewport/src/lib.rs) - wgpu winding order
- [debug_usd_mesh.rs](crates/bif_viewer/src/bin/debug_usd_mesh.rs) - NEW
- [test.ps1](test.ps1) - NEW

**Commit:** e9abf84

### Milestone 13b: Node Graph + Dynamic USD Loading ✅ (Jan 6, 2026)

**Goal:** Nuke-style node graph for scene assembly

**Implementation:**

- egui-snarl 0.5 node graph with USD Read + Ivar Render nodes
- rfd 0.14 native file dialogs (Browse button)
- `load_usd_scene()` for dynamic USD loading from node graph
- Houdini-style table layout (Path, Type, Children, Kind, Visibility)

**Files:**

- [node_graph.rs](crates/bif_viewport/src/node_graph.rs) (NEW) - ~350 LOC
- [scene_browser.rs](crates/bif_viewport/src/scene_browser.rs) - Table redesign
- [lib.rs](crates/bif_viewport/src/lib.rs) - +load_usd_scene, +events

---

### Bug Fix: Blank Scene Startup ✅ (Jan 8, 2026)

**Issue:** lucy_low.obj auto-loaded when starting without CLI args

**Fix:**
- Removed legacy mesh loading from `Renderer::new()`
- Empty vertex/index/instance buffers (dummy data for wgpu)
- Camera defaults to `(0, 10, 50)` looking at origin
- USD now loads exclusively via node graph or CLI

**Files:**
- [lib.rs](crates/bif_viewport/src/lib.rs) - ~40 lines removed/refactored
- [main.rs](crates/bif_viewer/src/main.rs) - Log message update

---

### Milestone 13a: USD Scene Browser + Property Inspector ✅ (Jan 5, 2026)

**Goal:** Gaffer-style hierarchy browser + property inspector

**Implementation:**

- 7 new prim traversal APIs in C++ bridge
- `PrimDataProvider` trait abstraction for USD data
- Scene browser with expandable tree and type icons
- Property inspector with transforms and bounding boxes

---

## Current State

| Metric | Value |
|--------|-------|
| Build (dev) | ~5s ✅ |
| Build (release) | ~2m ✅ |
| Tests | 60+ passing |
| Vulkan FPS | 60+ (VSync) |
| Embree BVH | 28ms |
| Instances | 100 (28M triangles) |

### Known Issues

None currently.

---

## Next Session: Milestone 14

**Materials (UsdPreviewSurface + MaterialX)**

- Parse UsdPreviewSurface shader nodes
- Map to Ivar materials (Lambertian, Metal, Dielectric)
- Add Disney Principled BSDF
- Basic PBR in Vulkan viewport

See [MILESTONES.md](MILESTONES.md#milestone-14-materials-usdpreviewsurface--materialx-) for details.

---

## Quick Commands

```bash
# Build
cargo build                    # Dev (~5s)
cargo build --release          # Release (~2m)

# Test
cargo test                     # All tests

# Run
cargo run -p bif_viewer                                 # Empty viewport
cargo run -p bif_viewer -- --usda assets/lucy_100.usda  # Load USD

# USD environment (required for USDC/references)
. .\setup_usd_env.ps1
```

---

## Session Start Prompt Template

```text
I'm continuing work on BIF (VFX renderer in Rust).

#file:SESSION_HANDOFF.md
#file:MILESTONES.md
#file:CLAUDE.md
#codebase

Status: Milestone 14 Complete! ✅

✅ Milestones 0-14 done (GPU Instancing + LOD)
✅ 10K instances with frustum culling + box LOD
✅ 89 tests passing
✅ Dual renderers: Vulkan (60 FPS) + Ivar (Embree two-level BVH)
✅ USD C++ integration: USDC, references, scene browser, node graph

Current state:
- Frustum culling for massive instance counts
- Distance-based LOD (full mesh near, box proxy far)
- Node graph with USD Read + Ivar Render nodes
- Houdini-style scene browser (table layout)
- Dynamic USD loading via node graph

Next: Milestone 15 (Materials/MaterialX)

Let's implement UsdPreviewSurface material parsing.
```

---

**Branch:** main
**Ready for:** Milestone 15 (Materials)! 🚀
