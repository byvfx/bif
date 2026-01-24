# Session Handoff - January 24, 2026

**Last Updated:** Subprocess .tx conversion, GUI button, IBL GPU compute
**Next Milestone:** 18 (Animation + Motion Blur)
**Project:** BIF - VFX Scene Assembler & Renderer

---

## Quick Status

| Status | Details |
|--------|---------|
| Complete | Milestones 0-17.2, IBL GPU Compute, NEE/MIS, .tx subprocess |
| Next | M18 (Animation + Motion Blur) |
| Tests | 135+ passing |
| Performance | 60 FPS viewport, 10K instances with LOD |

---

## Recent Work

### Subprocess .tx Conversion + GUI (Jan 24, 2026)

Arnold/Karma-style .tx workflow: pre-convert via subprocess, no auto-convert at load time.

| Component | Details |
|-----------|---------|
| `bif_maketx` | Standalone binary wrapping `oiio::make_tx` (crash-isolated) |
| `TextureCache` | `prefer_tx` flag (default false), `convert_textures_to_tx()` batch API |
| GUI | "Convert to .tx" button in Ivar Render node, async with status |
| C++ bridge | try/catch on load functions (won't catch SEH though) |

**Known issue:** OIIO crashes when reading .tx files back on Windows (SEH). Conversion works, loading doesn't. `prefer_tx` left at `false` for now.

### IBL GPU Compute + NEE/MIS (Jan 23-24, 2026)

Full 8-phase VFX code review implementation:

| Phase | Feature |
|-------|---------|
| 1 | Bug fixes: radical_inverse_vdc, sin_theta naming, partition_point CDF |
| 2 | Shader: ACES tonemap, max_mip uniform, normal inverse-transpose, skybox |
| 3 | CPU IBL: coarsen irradiance, cache BRDF LUT, parallelize prefiltered |
| 4 | Live preview: cubemaps at rotation=0, shader handles rotation |
| 5 | Generic RNG, remove redundant pdf Vec |
| 6 | Async IBL: background thread for HDR load + generation |
| 7 | NEE/MIS: environment direct lighting in path tracer |
| 8 | GPU compute: equirect→cubemap→irradiance→prefilter (all on GPU) |

---

## Current State

| Metric | Value |
|--------|-------|
| Build (dev) | ~5s |
| Tests | 135+ passing |
| Vulkan FPS | 60+ (VSync) |
| Crates | 6 (math, core, renderer, viewport, viewer, maketx) |

### What Works

**Viewport (GPU):**
- Textured PBR materials from USD
- Per-face materials via GeomSubsets
- HDRI environment: GPU compute IBL (irradiance + prefiltered + BRDF LUT)
- Skybox pass with rotation/intensity controls
- ACES tonemapping + gamma correction

**Textures:**
- OIIO loading with in-memory mipmap generation
- Subprocess .tx conversion (bif_maketx, crash-isolated)
- GUI button for batch pre-conversion
- Falls back to `image` crate without oiio feature

**Ivar (CPU Path Tracer):**
- Disney Principled BSDF
- NEE/MIS for HDRI direct lighting
- Full texture sampling (base_color, roughness, metallic, normal, opacity)
- Importance-sampled environment lighting
- Per-triangle materials, normal mapping, stochastic opacity

**USD Import:**
- USDA (pure Rust) + USDC (C++ bridge)
- UsdPreviewSurface + MaterialX standard_surface
- GeomSubset per-face material assignment

### Known Issues

- OIIO `load_texture_with_mips` crashes on .tx files on Windows (SEH)
- Vulkan viewport has some display issues (unrelated to textures)
- `prefer_tx` disabled until OIIO read-back is fixed

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
cargo build --features oiio    # With OIIO support

# Test
cargo test                     # All tests

# Run
cargo run -p bif_viewer                          # Without OIIO
cargo run -p bif_viewer --features oiio          # With OIIO

# USD environment (required for USDC)
. .\setup_usd_env.ps1

# Convert textures to .tx (standalone)
cargo run -p bif_maketx -- <input> <output>
```

---

**Branch:** main
**Ready for:** M18 (Animation + Motion Blur)
