# Session Handoff - January 31, 2026

**Last Updated:** M18.4 Multi-Prototype Ivar Fix
**Next Milestone:** M19 continued (geometry animation per frame)
**Project:** BIF - VFX Scene Assembler & Renderer

---

## Quick Status

| Status | Details |
|--------|---------|
| Complete | Milestones 0-18.4, M19 batch render with USD camera |
| Current | Multi-prototype Ivar rendering fixed |
| Tests | 137+ passing |
| Performance | 60 FPS viewport, 10K instances with LOD |

---

## Recent Work

### M18.4: Multi-Prototype Ivar Fix (Jan 31, 2026)

Fixed double mesh instances in Ivar rendering for multi-prototype USD scenes.

| Component | Details |
|-----------|---------|
| Problem | Meshes appeared duplicated in ray tracer (viewport correct) |
| Root cause | Combined mesh has baked transforms + Embree applied transforms again |
| C++ fix | Mesh path deduplication via `std::set` in `cache_stage_data()` |
| Rust fix | Use identity transform for Embree when `use_multi_draw` is true |

### M19: Batch Render to Disk (Jan 31, 2026)

Implemented batch rendering with USD camera animation support.

| Component | Details |
|-----------|---------|
| `batch_render.rs` | Frame sequence rendering with EXR output |
| FOV fix | Convert viewport FOV from radians to degrees |
| Fallback lighting | Sky gradient when no HDRI loaded |
| UNC paths | Fixed network path handling (`\\server\share\...`) |
| Sync build | Scene builds synchronously when Render clicked |
| USD camera | Matrix row/column fix - translation in row 3 |
| Viewport sync | "Sync Viewport to Camera" button for debugging |
| UI | Render button stays visible during render |

**Key fixes:**
- Black renders: FOV was in radians, renderer expected degrees
- Camera pos (0,0,0): USD matrix uses row-major, was reading col(3) instead of row(3)
- UNC paths: `canonicalize()` returns `\\?\UNC\...`, needed conversion back to `\\server\...`

### M18.3: USD Import Refinement (Jan 27, 2026)

Fixed USD files with relative references (`@./file.usda@`) failing to load.

---

## Current State

| Metric | Value |
|--------|-------|
| Build (dev) | ~5s |
| Tests | 137+ passing |
| Vulkan FPS | 60+ (VSync) |
| Crates | 6 (math, core, renderer, viewport, viewer, maketx) |

### What Works

**Batch Render:**
- EXR output with AOVs (depth, normals)
- USD camera animation (position changes per frame)
- Frame range with step
- Progress bar with cancellation
- ZIP compression

**USD Import:**
- USDA (pure Rust) + USDC (C++ bridge)
- Relative references (`@./file.usda@`)
- UNC network paths (`\\server\share\file.usd`)
- PointInstancer with external prototypes
- UsdPreviewSurface + MaterialX standard_surface
- Camera animation (xformOp time samples)

**Animation:**
- Timeline UI with playback controls
- Transform animation (xformOp time samples)
- Vertex animation for multi-mesh combined scenes
- USD camera sync to viewport

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

- Geometry animation not yet evaluated per frame in batch render (static BVH)
- OIIO `load_texture_with_mips` crashes on .tx files on Windows

---

## Next Session

**Goal:** Per-frame geometry animation in batch render

1. Rebuild BVH per frame with animated vertex positions
2. Or: Transform-only animation (cheaper, just update instance matrices)
3. Test with vertex-animated USD scenes

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

# Test camera animation
cargo run -p bif_viewer -- --usd assets/moving_cam_usd.usd_rop1.usda
```

---

**Branch:** main
**Ready for:** M19 continued (geometry animation)
