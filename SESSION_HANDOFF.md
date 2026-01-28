# Session Handoff - January 27, 2026

**Last Updated:** M18.3 USD Import Refinement Complete
**Next Milestone:** 19 (Frame Rendering)
**Project:** BIF - VFX Scene Assembler & Renderer

---

## Quick Status

| Status | Details |
|--------|---------|
| Complete | Milestones 0-18.3, USD relative reference resolution |
| Next | M19 (Frame Rendering) |
| Tests | 137+ passing |
| Performance | 60 FPS viewport, 10K instances with LOD |

---

## Recent Work

### M18.3: USD Import Refinement (Jan 27, 2026)

Fixed USD files with relative references (`@./file.usda@`) failing to load.

| Component | Details |
|-----------|---------|
| `usd_bridge.cpp` | Added ArResolverContextBinder for asset resolution |
| `CMakeLists.txt` | Added `ar` and `usdShade` libraries |
| `lucy_100.usda` | Fixed USDA syntax (proper xformOpOrder) |
| Tests | Added `test_load_relative_reference_usda`, `test_load_pointinstancer_external_prototype` |

**The fix:** USD's `ArResolver` needs a context to resolve `@./relative.usda@` paths. Without `ArResolverContextBinder`, USD doesn't know the base directory. Also normalized Windows backslashes to forward slashes.

### M18.2: Thread Safety + Instance Encapsulation (Jan 26, 2026)

Made UsdStage thread-safe with pre-caching at load time.

### M18.1: Vertex Animation for Multi-Mesh (Jan 25, 2026)

Fixed vertex animation failing when multiple meshes are combined into single buffer.

| Component | Details |
|-----------|---------|
| `MeshRange` | New struct: usd_mesh_index, vertex_offset, vertex_count |
| `mesh_ranges` | New field in MeshData for tracking per-mesh ranges |
| `combine_with_transforms` | Now accepts mesh_idx, builds mesh_ranges |
| `update_vertex_animation` | Uses mesh_ranges to update correct vertex range |

---

## Current State

| Metric | Value |
|--------|-------|
| Build (dev) | ~5s |
| Tests | 137+ passing |
| Vulkan FPS | 60+ (VSync) |
| Crates | 6 (math, core, renderer, viewport, viewer, maketx) |

### What Works

**USD Import:**
- USDA (pure Rust) + USDC (C++ bridge)
- **Relative references** (`@./file.usda@`) now resolve correctly
- **PointInstancer with external prototypes** working
- UsdPreviewSurface + MaterialX standard_surface
- Timeline metadata extraction

**Animation:**
- Timeline UI with playback controls
- Transform animation (xformOp time samples)
- Vertex animation for multi-mesh combined scenes
- AnimatedTransform with keyframe interpolation

**Viewport (GPU):**
- Textured PBR materials from USD
- Per-face materials via GeomSubsets
- HDRI environment: GPU compute IBL
- Skybox pass with rotation/intensity controls

**Ivar (CPU Path Tracer):**
- Disney Principled BSDF
- NEE/MIS for HDRI direct lighting
- Full texture sampling

### Known Issues

- USD camera toggle not implemented
- OIIO `load_texture_with_mips` crashes on .tx files on Windows

---

## Next Session: M19 Frame Rendering

**Goal:** Render animated sequences to disk

1. Frame range UI (start/end/step)
2. Batch render loop with frame substitution
3. Progress tracking with cancellation
4. Output naming patterns (`render.####.exr`)
5. EXR output with AOVs (beauty, depth, normals)

---

## Quick Commands

```bash
# Build
cargo build                    # Dev (~5s)
cargo build --features oiio    # With OIIO support

# Test
cargo test                     # All tests

# Run
cargo run -p bif_viewer                          # Without OIIO
cargo run -p bif_viewer --features oiio          # With OIIO

# USD environment (required for USDC)
. .\setup_usd_env.ps1

# Test relative references
cargo run -p bif_viewer -- --usd assets/lucy_100.usda
cargo run -p bif_viewer -- --usd assets/lucy_100_fixed.usda
```

---

**Branch:** main
**Ready for:** M19 (Frame Rendering)
