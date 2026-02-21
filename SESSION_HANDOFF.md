# Session Handoff - February 21, 2026

**Last Updated:** SHARC cache benchmark (cache slower on simple scenes)
**Next Milestone:** M29 USD Export + Non-Destructive Layers
**Project:** BIF - VFX Scene Assembler & Renderer

---

## Quick Status

| Status | Details |
|--------|---------|
| Complete | Milestones 0-23 |
| Current | Planning next milestone |
| Tests | 68 viewport, 85 bif_core, 247+ total passing |
| Performance | 60 FPS viewport, 100K instances with LOD |

---

## Recent Work

### SHARC Cache Benchmark (Feb 21, 2026)

| Finding | Details |
|---------|---------|
| Benchmark | `cache_bench` example: Cornell box, 32 passes, cache OFF vs ON |
| Result | Cache **0.62x** (slower) — RwLock overhead dominates cheap 12-tri BVH |
| Hit rate | 5.1% after 32 passes, 59% occupancy |
| Conclusion | Need heavier scene to see benefit; consider lock-free atomics |

### M23: SHARC Radiance Cache (Feb 20, 2026)

| Feature | Details |
|---------|---------|
| RadianceCache | 64-shard `RwLock<Vec<CacheEntry>>`, GPU-compatible `#[repr(C)]` layout |
| Spatial hash | Position quantized to cells + dominant-axis normal (6 dirs, prevents light leak) |
| Cache READ | After `min_bounce_depth`, non-delta hits return cached radiance → skip remaining bounces |
| Cache WRITE | Stores surface-local radiance (emission + NEE) via EMA blending |
| Russian Roulette | bounce >= 3, throughput-proportional survival, unbiased via survivor boost |
| IPR | Cache persists across progressive passes, clears on camera move |
| Batch | Cache persists within frame, clears per-frame for animated scenes |
| Heatmap AOV | `CacheHeatmap` channel: sample count → black/red/yellow/green |
| UI | Enable/disable, cell size, buffer size, min samples, min bounce, hit rate/occupancy stats |
| Tests | 14 new tests (hash, cache, concurrency, staleness) |

### HDRI Perf Cleanup (Feb 20, 2026)

| Fix | Details |
|-----|---------|
| Param hoisting | HDRI rotation/intensity resolved once before bounce loop, not per-bounce |
| Ivar throttle | `should_restart()` gate prevents excessive cancel+spawn during slider drag |

---

## Current State

| Metric | Value |
|--------|-------|
| Build (dev) | ~10s |
| Tests | 247+ passing |
| Vulkan FPS | 60 (VSync with Fifo) |
| Crates | 6 (math, core, renderer, viewport, viewer, maketx) |

### What Works

**SHARC Radiance Cache (M23):**
- Spatially hashed radiance cache with normal disambiguation
- Cache read/write integrated into `ray_color()` and `ray_color_with_aovs()`
- Russian Roulette path termination (bounce >= 3)
- IPR: cache warms across progressive passes, clears on invalidation
- Batch: per-frame cache lifecycle for animated scenes
- Cache heatmap AOV for diagnosis
- egui controls for all cache parameters + live stats

**HDRI + Skybox:**
- HDRI rotation/intensity slider updates live in Ivar CPU path tracer
- Viewport skybox samples full-res base cubemap (512x512)

**Auto-Compute (M21.2):**
- Nodes auto-cook when inputs connect/change (Houdini-style)
- Scatter recompute → instancer auto-recomputes

### Known Issues

- Multi-prototype instancing not yet supported (single proto per instancer)
- OIIO `load_texture_with_mips` crashes on .tx files on Windows
- bif_core tests need USD DLLs (run via `setup_usd_env.ps1`)
- Renderer struct ~60 fields (God object)
- ~~embree.rs debug counter overflow after ~4.3B rays~~ (fixed: `wrapping_add`)
- ~~Ivar render timer keeps ticking after completion~~ (fixed: `final_render_secs` freeze)
- SHARC cache bias: stores emission+NEE only (not indirect) — converges via EMA but biased low

---

## Next Session

**Goal:** Heavier cache benchmark or M29 USD Export

1. Build complex test scene (USD instances) for meaningful cache A/B
2. Consider lock-free cache (atomics instead of RwLock) if overhead confirmed
3. M29 USD Export: stage authoring, opinion layers, PointInstancer export

---

## Quick Commands

```bash
# Build
cargo build                    # Dev (~10s)
cargo build --features oiio    # With OIIO support

# Test
cargo test -p bif_math         # 41 tests
cargo test -p bif_renderer     # Renderer + radiance cache tests

# Run
cargo run -p bif_viewer                          # Without OIIO
cargo run -p bif_viewer --features oiio          # With OIIO

# USD environment (required for USDC)
. .\setup_usd_env.ps1
```

---

**Branch:** main
**Ready for:** M29 USD Export or next milestone choice
