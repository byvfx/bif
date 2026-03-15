# Session Handoff - March 15, 2026

**Last Updated:** Backface culling fix, USD camera properties, Ivar UDIM atlas
**Next Milestone:** M29.5 egui upgrade
**Project:** BIF - VFX Scene Assembler & Renderer

---

## Quick Status

| Status | Details |
|--------|---------|
| Complete | Milestones 0-23, M26 (OIDN), M26.1 (Ivar material cache), M19.6 (viewport cleanup) |
| Current | Backface culling + USD camera props (FOV/near/far) + Ivar UDIM TextureCache atlas |
| Tests | 84 renderer, 41 math, 79 viewport, 93 bif_core |
| Performance | 60 FPS viewport, 100K instances with LOD, Ivar build ~185ms (2.5M tri combined mesh) |

---

## Recent Work

### Backface Culling + USD Camera Properties (Mar 15, 2026)

1. **Backface winding fix** — `FrontFace::Cw` → `FrontFace::Ccw` (USD rightHanded = CCW front faces). Walls no longer see-through.
2. **USD camera properties** — C++ bridge reads focal_length, vertical_aperture, clipping_range from UsdGeomCamera. Viewport syncs FOV/near/far when selecting USD camera. Camera direction extraction also fixed.
3. **Ivar UDIM TextureCache** — UDIM atlas stitching in `texture.rs` with `transform_uv()` for CPU-side sampling. `texture_loader.rs` delegates to `bif_core` for UDIM detection.

### Previous Session: ALab Material/Texture Binding (Mar 14, 2026)

- 7 bugs fixed in material/texture binding after instance proxy support
- C++ bridge traversal with `UsdTraverseInstanceProxies()`
- UDIM atlas stitching, purpose filtering, texture path normalization

---

## Blockers / Known Issues

- `test_should_restart_no_render` — known flaky timing test
- bif_viewport tests need USD DLLs (`setup_usd_env.ps1`)
- bif_viewer.exe locked during build if app is running
- `doubleSided` attribute + per-mesh `orientation` not yet read from USD (deferred — not needed for ALab)

---

## Next Steps

1. M29.5: egui 0.29→0.30 upgrade + egui-snarl 0.5→0.6 + vertical node layout
2. M30: Node graph save/load (`.bif`/`.bifa`) + evaluation modes + cache node
3. Future: `doubleSided` attribute + per-mesh `orientation` from USD (needs C++ bridge addition)
4. Future: horizontalAperture / anamorphic squeeze for non-standard USD cameras
