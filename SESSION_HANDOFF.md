# Session Handoff - January 18, 2026

**Last Updated:** Milestone 17 Complete (Viewport PBR + Textures)
**Next Milestone:** 18 (Animation + Motion Blur)
**Project:** BIF - VFX Scene Assembler & Renderer

---

## Quick Status

| Status | Details |
|--------|---------|
| Complete | Milestones 0-17 |
| Next | M18 (Animation + Motion Blur) |
| Tests | 93+ passing |
| Performance | 60 FPS viewport, 10K instances with LOD |

---

## Recent Work

### Milestone 17: Viewport PBR + Textures - Complete (Jan 18, 2026)

**Goal:** Textured PBR materials in Vulkan viewport

**Key Achievements:**
- GPU texture upload with binding_array (64 texture slots)
- Per-vertex material IDs via GeomSubset extraction
- Per-instance material ID fallback
- Parallel texture loading (25s → 1s with rayon + sRGB LUT)
- Texture downscaling for GPU limits (8192 max dimension)

**Key Files:**
- [usd_bridge.cpp](cpp/usd_bridge/usd_bridge.cpp) - GeomSubset extraction, material caching order
- [lib.rs](crates/bif_viewport/src/lib.rs) - Parallel texture loading, GPU upload
- [basic.wgsl](crates/bif_viewport/src/shaders/basic.wgsl) - Texture sampling

**Critical Fixes:**
- Materials must be cached BEFORE meshes for GeomSubset lookup
- Skip subsets without material bindings (e.g., Houdini `__subdivs__`)

### Milestone 16: MaterialX Support - Complete (Jan 17, 2026)

**Goal:** Import MaterialX materials from USD

**Key Achievements:**
- MaterialX standard_surface shader detection (Houdini exports)
- Property extraction: base_color, metalness, specular_roughness, opacity, emission
- Parent hierarchy traversal for inherited material bindings
- Automatic fallback: MaterialX → UsdPreviewSurface → default gray

---

## Current State

| Metric | Value |
|--------|-------|
| Build (dev) | ~5s |
| Build (release) | ~2m |
| Tests | 93+ passing |
| Vulkan FPS | 60+ (VSync) |
| Embree BVH | 28ms |
| Instances | 10K+ with LOD |

### What Works Now

**Viewport (GPU):**
- Textured PBR materials from USD
- Per-face materials via GeomSubsets
- binding_array with 64 texture slots
- Parallel texture loading (25x faster)

**Ivar (CPU Path Tracer):**
- Disney Principled BSDF with Burley diffuse + GGX specular
- Materials from USD (UsdPreviewSurface + MaterialX)
- Metallic/roughness/specular properties

**USD Import:**
- USDA (pure Rust) + USDC (C++ bridge)
- UsdPreviewSurface materials
- MaterialX standard_surface materials
- GeomSubset per-face material assignment
- File references resolved

### Known Limitations

- No normal mapping yet
- GPU upload still ~11s (downscaling in upload phase)
- No .tx texture support (OpenEXR tiled/mipmapped) - planned for M23

---

## Next Session: M18 Animation + Motion Blur

**Goal:** Load and render time-sampled USD data

1. Parse time-sampled attributes (`xformOp:translate.timeSamples`)
2. Timeline UI widget (frame slider, play/pause)
3. Animate transforms in Vulkan viewport
4. Motion blur in Ivar renderer (stretch)

---

## Quick Commands

```bash
# Build
cargo build                    # Dev (~5s)
cargo build --release          # Release (~2m)

# Test
cargo test                     # All tests (needs USD env)

# Run
cargo run -p bif_viewer                                 # Empty viewport
cargo run -p bif_viewer -- --usd assets/lucy/usd/assets/lucy/lucy.usd  # Load USD

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

Status: Milestone 17 Complete!

Milestones 0-17 done
- Textured PBR viewport
- Per-face materials (GeomSubsets)
- Parallel texture loading (25x faster)
- 93+ tests passing

Current state:
- Materials from USD (MaterialX + UsdPreviewSurface)
- GeomSubsets for per-face material assignment
- GPU texture sampling working
- Disney BSDF renders in Ivar

Next: M18 (Animation + Motion Blur)

Let's implement timeline and animation next.
```

---

**Branch:** main
**Ready for:** M18 (Animation + Motion Blur)
