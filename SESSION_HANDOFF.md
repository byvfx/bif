# Session Handoff - January 9, 2026

**Last Updated:** USD Winding Order Fix
**Next Milestone:** 14 (Materials/MaterialX)
**Project:** BIF - VFX Scene Assembler & Renderer

---

## Quick Status

| Status | Details |
|--------|---------|
| ✅ Complete | Milestones 0-13b (Scene Browser + Node Graph) |
| 🎯 Next | Milestone 14 (Materials/MaterialX) |
| 📦 Tests | 60+ passing (51 total: 17 core, 34 math) |
| 🚀 Performance | 60 FPS viewport, 28ms Embree BVH |

---

## Recent Work

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

Status: Blank Scene Fix Complete! ✅

✅ Milestones 0-13b done (Scene Browser + Node Graph)
✅ Bug fix: Blank scene startup (no auto-load)
✅ 60+ tests passing
✅ Dual renderers: Vulkan (60 FPS) + Ivar (Embree two-level BVH)
✅ USD C++ integration: USDC, references, scene browser, node graph

Current state:
- Node graph with USD Read + Ivar Render nodes
- Houdini-style scene browser (table layout)
- Dynamic USD loading via node graph
- Blank scene startup (no legacy OBJ auto-load)
- Property inspector panel

Next: Milestone 14 (Materials/MaterialX)

Let's implement UsdPreviewSurface material parsing.
```

---

**Last Commit:** 8ce4cef (fix: Remove lucy_low.obj auto-load, start blank scene)
**Branch:** main
**Ready for:** Milestone 14 (Materials)! 🚀
