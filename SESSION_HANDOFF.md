# Session Handoff - February 4, 2026

**Last Updated:** M19.3 Timeline Playback Fix
**Next Milestone:** Instance transform animation per frame
**Project:** BIF - VFX Scene Assembler & Renderer

---

## Quick Status

| Status | Details |
|--------|---------|
| Complete | Milestones 0-19.3, viewport camera, timeline playback |
| Current | Instance transform animation per frame |
| Tests | 142+ passing |
| Performance | 60 FPS viewport, 10K instances with LOD |

---

## Recent Work

### M19.3: Timeline Playback Fix (Feb 4, 2026)

Fixed animation playback using wall-clock time instead of delta-time accumulation.

| Component | Details |
|-----------|---------|
| Wall-clock playback | Uses `Instant::now()` instead of delta_time accumulation |
| Realtime toggle | "RT" checkbox: ON = wall-clock accurate, OFF = every frame |
| PresentMode | Changed Mailbox → Fifo for proper VSync |
| New methods | `play()`, `pause()`, `toggle_playback()`, `update()` |
| Scrub anchor | Reset playback anchor when user scrubs during playback |
| Tests | 5 new tests for playback, looping, pause, scrub |

**Root cause:** `PresentMode::Mailbox` allowed ~700fps, causing tiny delta_time (~1.4ms). Wall-clock time eliminates this issue.

### M19.5: Renderer Decomposition Phase 2 (Feb 2, 2026)

Extracted three more managers from the monolithic Renderer struct:

| Component | Fields | Description |
|-----------|--------|-------------|
| MultiDrawState | 3 | prototype_gpu_data, instance_groups, enabled |
| EnvironmentManager | 7 | IBL, skybox pipeline, async HDRI/tx loading |
| CullingManager | 12 | frustum culling, LOD box proxy, polygon budget |

**Total:** ~450 lines removed from lib.rs across 3 new modules.

### M19.4: Code Quality & Robustness (Feb 2, 2026)

VFX code review findings addressed. Seven commits total.

| Change | Details |
|--------|---------|
| Embree error handling | `EmbreeError` enum with `thiserror`, `new()` returns `Result` |
| Materials validation | Check empty vec before use, debug_assert in `hit()` |
| Light limit increase | 8 → 32 lights, matches common DCC limits |
| LightsManager | Extracted from Renderer (~91 lines removed) |
| GnomonRenderer | Extracted from Renderer (~213 lines removed) |
| Thread safety docs | Expanded UsdStage Send+Sync safety comments |

---

## Current State

| Metric | Value |
|--------|-------|
| Build (dev) | ~5s |
| Tests | 142+ passing |
| Vulkan FPS | 60 (VSync with Fifo) |
| Crates | 6 (math, core, renderer, viewport, viewer, maketx) |

### What Works

**Timeline Playback:**
- Play/pause with wall-clock accurate timing
- "RT" toggle for realtime vs every-frame mode
- Loop with seamless wrap to start frame
- Scrub during playback continues from new position
- Integer frame snap option

**Viewport Camera:**
- Camera dropdown in timeline (Viewport + USD cameras)
- Lock/unlock toggle for camera controls
- Camera syncs when USD camera selected
- Camera animates during playback

**Batch Render:**
- EXR output with AOVs (depth, normals)
- USD camera animation (position changes per frame)
- Instance transform animation (per-frame evaluation)
- Frame range with step
- Progress bar with cancellation

**USD Import:**
- USDA (pure Rust) + USDC (C++ bridge)
- Relative references, UNC paths
- UsdPreviewSurface + MaterialX
- Camera and transform animation

### Known Issues

- OIIO `load_texture_with_mips` crashes on .tx files on Windows

---

## Architecture Improvements (M19.4 + M19.5)

Renderer struct decomposition complete (Phase 1 + 2):

| Module | Fields | Purpose |
|--------|--------|---------|
| LightsManager | 4 | lights uniform, buffer, bind_group, scene_lights |
| GnomonRenderer | 5 | pipeline, vertex_buffer, uniform, buffer, bind_group, size |
| MultiDrawState | 3 | prototype_gpu_data, instance_groups, enabled |
| EnvironmentManager | 7 | IBL, skybox, async HDRI/tx loading |
| CullingManager | 12 | frustum culling, LOD box proxy, polygon budget |

**Total removed from Renderer:** ~600 lines across M19.4 and M19.5

---

## Next Session

**Goal:** Continue rendering improvements

1. Test batch render with animated instances (need animated USD test file)
2. Consider adding render progress to UI
3. Multi-draw support for animated scenes

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
cargo run -p bif_viewer -- --usd assets/animated_cube.usda
```

---

**Branch:** main
**Ready for:** Instance transform animation per frame
