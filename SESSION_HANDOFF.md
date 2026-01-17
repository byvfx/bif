# Session Handoff - January 17, 2026

**Last Updated:** Milestone 16 Complete (MaterialX Support)
**Next Milestone:** 17 (Viewport PBR + Textures)
**Project:** BIF - VFX Scene Assembler & Renderer

---

## Quick Status

| Status | Details |
|--------|---------|
| Complete | Milestones 0-16 |
| Next | M17 (GPU Textures, Per-Instance Materials) |
| Tests | 93+ passing |
| Performance | 60 FPS viewport, 10K instances with LOD |

---

## Recent Work

### Milestone 16: MaterialX Support - Complete (Jan 17, 2026)

**Goal:** Import MaterialX materials from USD

**Key Achievements:**
- MaterialX standard_surface shader detection (Houdini exports)
- Property extraction: base_color, metalness, specular_roughness, opacity, emission
- Parent hierarchy traversal for inherited material bindings
- Automatic fallback: MaterialX → UsdPreviewSurface → default gray
- Tested with Houdini-exported Lucy model (orange material)

**Key Files:**
- [usd_bridge.cpp](cpp/usd_bridge/usd_bridge.cpp) - MaterialX shader parsing
- [cpp_bridge.rs](crates/bif_core/src/usd/cpp_bridge.rs) - FFI bindings

### Milestone 15: Materials - Complete (Jan 11, 2026)

**Goal:** Full material pipeline from USD to render

- UV coordinates throughout pipeline
- PBR Material struct with texture paths
- C++ bridge UsdPreviewSurface extraction
- Texture loading system (CPU)
- Disney Principled BSDF in Ivar
- Viewport PBR material support

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

**Ivar (CPU Path Tracer):**
- Disney Principled BSDF with Burley diffuse + GGX specular
- Materials from USD (UsdPreviewSurface + MaterialX)
- Metallic/roughness/specular properties

**Viewport (GPU):**
- PBR-inspired shading (Fresnel, metallic blend)
- Material properties from USD
- UVs pass through to fragment shader

**USD Import:**
- USDA (pure Rust) + USDC (C++ bridge)
- UsdPreviewSurface materials
- MaterialX standard_surface materials
- File references resolved

### Known Limitations

- Textures load to CPU but aren't sampled yet (M17)
- Single material per scene (per-instance materials in M17)
- No normal mapping yet

---

## Next Session: M17 Viewport Textures

**Goal:** Full textured PBR in Vulkan viewport

1. GPU texture upload (texture array)
2. Texture sampling in fragment shader
3. Per-instance material IDs
4. Normal mapping (stretch)

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

Status: Milestone 16 Complete!

Milestones 0-16 done
Disney BSDF in Ivar renderer
MaterialX + UsdPreviewSurface support
Viewport PBR shading
93+ tests passing

Current state:
- Materials from USD (MaterialX + UsdPreviewSurface)
- Disney BSDF renders in Ivar
- Viewport shows PBR shading
- Texture loading ready (CPU), not yet on GPU

Next: M17 (GPU Textures, Per-Instance Materials)

Let's implement GPU texture sampling next.
```

---

**Branch:** main
**Ready for:** M17 (Viewport Textures)
