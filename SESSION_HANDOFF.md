# Session Handoff - February 6, 2026

**Last Updated:** M20 Code Review Fixes
**Next Milestone:** M21 Point Instancing + Scattering
**Project:** BIF - VFX Scene Assembler & Renderer

---

## Quick Status

| Status | Details |
|--------|---------|
| Complete | Milestones 0-20, full interactivity pipeline |
| Current | M21 Point Instancing + Scattering |
| Tests | 41 bif_math passing (bif_core needs USD DLLs) |
| Performance | 60 FPS viewport, 10K instances with LOD |

---

## Recent Work

### M20: Scene Interactivity + Keyframing (Feb 6, 2026)

9-phase milestone implementing full scene interaction pipeline:

| Phase | Feature | Key Files |
|-------|---------|-----------|
| 1 | Embree viewport picking | `pick_scene.rs`, `lib.rs` |
| 2 | Selection highlight | `basic.wgsl`, `gpu_types.rs` |
| 3 | Undo system + editable transforms | `undo.rs`, `property_inspector.rs` |
| 4 | Translate gizmo | `gizmo.rs`, `main.rs` |
| 5 | Keyframing | `undo.rs`, `timeline.rs`, `render.rs` |
| 6 | Primitive nodes | `primitives.rs`, `node_graph.rs` |
| 7 | Orthographic views | `camera.rs`, `ivar_state.rs` |
| 8 | USD edit layer export | `usd_bridge.cpp/.h`, `cpp_bridge.rs` |
| 9 | Polish + integration | All |

**Architecture decisions:**
- CPU ray via Embree (reuses existing BVH)
- `selected_instance_id` in CameraUniform for shader highlight
- egui `Painter` overlay for gizmo (no new wgpu pipeline)
- `Vec<Box<dyn UndoCommand>>` portable to Qt later
- Deferred event handling via egui temp data (can't mutate Renderer inside egui closure)

---

## Current State

| Metric | Value |
|--------|-------|
| Build (dev) | ~10s |
| Tests | 41+ passing (bif_math) |
| Vulkan FPS | 60 (VSync with Fifo) |
| Crates | 6 (math, core, renderer, viewport, viewer, maketx) |

### What Works

**Scene Interactivity (M20):**
- Click viewport to select instance (Embree raycast)
- Orange highlight on selected instance
- Translate gizmo with colored X/Y/Z axes
- Editable TRS in property inspector (DragValue fields)
- Undo/Redo (Ctrl+Z / Ctrl+Shift+Z)
- Set keyframes at current frame (K key)
- Diamond markers on timeline for keyframes
- Procedural cube/sphere/camera in node graph
- Orthographic views (Top/Front/Right/etc.)
- Export transform edits as USD sublayer

**Timeline Playback:**
- Wall-clock accurate timing
- Loop, scrub, integer frame snap
- Keyframe interpolation during playback

**USD Import/Export:**
- USDA + USDC via C++ bridge
- UsdPreviewSurface + MaterialX
- Camera and transform animation
- Edit layer export (xformOp overrides)

### Known Issues

- OIIO `load_texture_with_mips` crashes on .tx files on Windows
- bif_core tests need USD DLLs (run via `setup_usd_env.ps1`)
- Gizmo only supports translate (rotate/scale future work)
- Ortho picking untested with real scenes
- Renderer struct ~60 fields (God object) - extract sub-structs in future session

---

## Next Session

**Goal:** M21 Point Instancing + Scattering

1. UsdGeomPointInstancer support (already loads via C++ bridge)
2. Scatter points on surface (random/Poisson disk)
3. Paint points tool (brush-based placement)
4. Per-point attributes (scale, rotation, ID)

---

## Quick Commands

```bash
# Build
cargo build                    # Dev (~10s)
cargo build --features oiio    # With OIIO support

# Test
cargo test -p bif_math         # 41 tests
cargo test -p bif_renderer     # Embree tests

# Run
cargo run -p bif_viewer                          # Without OIIO
cargo run -p bif_viewer --features oiio          # With OIIO

# USD environment (required for USDC)
. .\setup_usd_env.ps1
```

---

**Branch:** main
**Ready for:** M21 Point Instancing + Scattering
