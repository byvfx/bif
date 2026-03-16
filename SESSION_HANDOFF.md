# Session Handoff - March 15, 2026

**Last Updated:** Full codebase code review — 12 blockers fixed, 30+ warnings, 50 new tests
**Next Milestone:** M29.5 egui upgrade
**Project:** BIF - VFX Scene Assembler & Renderer

---

## Quick Status

| Status | Details |
|--------|---------|
| Complete | Milestones 0-23, M26 (OIDN), M26.1 (Ivar material cache), M19.6 (viewport cleanup) |
| Current | Full codebase review complete — all phases shipped |
| Tests | 85 renderer (+1), 72 math (+31), 19 viewer (+19), 79 viewport, 93 bif_core |
| Performance | 60 FPS viewport, 100K instances with LOD, Ivar build ~185ms (2.5M tri combined mesh) |

---

## Recent Work

### Full Codebase Code Review (Mar 15, 2026)

Six parallel review agents audited all 6 crates + C++ bridge. Five phases of fixes:

1. **Quick wins:** UsdEditLayer leak, AOV seed hash, hardcoded dev path, C++ dead code
2. **Correctness:** SHARC race, instance hit t-scaling, matrix convention, light formulas, BSDF fixes
3. **Robustness:** NaN guards everywhere, 0-size texture, HDRI clamp, CLI validation, graceful errors
4. **Performance:** Arvo AABB transform, explicit glam re-exports, HashMap prototype lookup
5. **Tests:** 50 new tests across math/renderer/viewer

### Previous: Backface Culling + USD Camera Properties (Mar 15, 2026)

1. **Backface winding fix** — `FrontFace::Cw` → `FrontFace::Ccw`
2. **USD camera properties** — C++ bridge reads focal_length, vertical_aperture, clipping_range
3. **Ivar UDIM TextureCache** — UDIM atlas stitching with `transform_uv()`

---

## Blockers / Known Issues

- `test_should_restart_no_render` — known flaky timing test
- bif_viewport tests need USD DLLs (`setup_usd_env.ps1`)
- bif_viewer.exe locked during build if app is running
- `doubleSided` attribute + per-mesh `orientation` not yet read from USD

---

## Next Steps

1. M29.5: egui 0.29→0.30 upgrade + egui-snarl 0.5→0.6 + vertical node layout
2. M30: Node graph save/load (`.bif`/`.bifa`) + evaluation modes + cache node
3. Future: `doubleSided` attribute + per-mesh `orientation` from USD
4. Future: horizontalAperture / anamorphic squeeze for non-standard USD cameras
