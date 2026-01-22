# Session Handoff - January 21, 2026

**Last Updated:** Milestone 17.1 Complete (OIIO + .tx Texture Pipeline)
**Next Milestone:** 18 (Animation + Motion Blur)
**Project:** BIF - VFX Scene Assembler & Renderer

---

## Quick Status

| Status | Details |
|--------|---------|
| Complete | Milestones 0-17.1 |
| Next | M18 (Animation + Motion Blur) |
| Tests | 93+ passing |
| Performance | 60 FPS viewport, 10K instances with LOD |

---

## Recent Work

### Milestone 17.1: OIIO + .tx Texture Pipeline - Complete (Jan 21, 2026)

**Goal:** Industry-standard texture workflow with OpenImageIO

**Key Achievements:**
- C++ OIIO bridge (`cpp/oiio_bridge/`) with FFI
- Automatic .tx conversion via `ImageBufAlgo::make_texture()`
- Mipmap generation (box filter downsample)
- GPU upload with trilinear + anisotropic filtering (16x)
- Feature-gated: `--features oiio` to enable
- Falls back to `image` crate when OIIO not enabled

**Key Files:**
- [oiio_bridge.cpp](cpp/oiio_bridge/oiio_bridge.cpp) - C++ OIIO implementation
- [oiio.rs](crates/bif_core/src/oiio.rs) - Rust FFI wrapper
- [texture.rs](crates/bif_core/src/texture.rs) - TextureCache with OIIO support

**Build Commands:**
```bash
cargo build                    # Without OIIO (uses image crate)
cargo build --features oiio    # With OIIO (requires vcpkg openimageio)
```

### Milestone 17: Viewport PBR + Textures - Complete (Jan 18, 2026)

**Goal:** Textured PBR materials in Vulkan viewport

**Key Achievements:**
- GPU texture upload with binding_array (64 texture slots)
- Per-vertex material IDs via GeomSubset extraction
- Per-instance material ID fallback
- Parallel texture loading (25s -> 1s with rayon + sRGB LUT)
- Texture downscaling for GPU limits (8192 max dimension)

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

**Textures (OIIO mode):**
- Auto-.tx conversion on first load
- Mipmap generation and loading
- Trilinear + anisotropic filtering

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

- Ivar doesn't sample textures yet (DisneyBSDF has fields but scatter() doesn't use them)
- No normal mapping yet
- GPU upload still ~3s for large textures

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
cargo build --features oiio    # With OIIO support

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

Status: Milestone 17.1 Complete!

Milestones 0-17.1 done
- Textured PBR viewport
- Per-face materials (GeomSubsets)
- OIIO + .tx pipeline (feature-gated)
- 93+ tests passing

Current state:
- Materials from USD (MaterialX + UsdPreviewSurface)
- GeomSubsets for per-face material assignment
- GPU texture sampling working
- OIIO auto-converts to .tx with mipmaps
- Disney BSDF renders in Ivar (no textures yet)

Next: M18 (Animation + Motion Blur)

Let's implement timeline and animation next.
```

---

**Branch:** main
**Ready for:** M18 (Animation + Motion Blur)
