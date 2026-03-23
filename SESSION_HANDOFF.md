# Session Handoff - March 22, 2026

**Last Updated:** Parallel UV seam split + .tx cache plan
**Next Milestone:** .tx viewport cache integration → M29.5 egui upgrade
**Project:** BIF - VFX Scene Assembler & Renderer

---

## Quick Status

| Status | Details |
|--------|---------|
| Complete | Milestones 0-23, M26 (OIDN), M26.1 (Ivar material cache), M19.6 (viewport cleanup) |
| Current | Purpose filtering done, USD spec compliance complete |
| Tests | 93 renderer (+4 glass), 72 math, 19 viewer, 79 viewport, 93 bif_core |
| Performance | 60 FPS viewport, 100K instances with LOD, Ivar build ~185ms (2.5M tri combined mesh) |

---

## Recent Work

### Parallel UV + .tx Texture Cache (Mar 22, 2026)

- Parallelized UV seam split: 3-pass `cache_stage_data()` refactor with `WorkParallelForN`
- Viewport .tx cache: `resolve_tx_path` prefers .tx over source, auto bg conversion on scene load
- Parallel .tx conversion via rayon, parallel Ivar texture pre-warm (9s→2.7s)
- Clear .tx cache UI button, UDIM .tx fallback
- Ivar already uses .tx via `TextureCache` with `prefer_tx: true` — no changes needed
- Awaiting ALab .tx benchmark (272 textures) and rt_base parallel UV benchmark

### Purpose Filtering + Specular Fix (Mar 21, 2026)

- Purpose enum (Default/Render/Proxy/Guide) on Instance, C++ bridge hierarchy walk for inherited purpose
- Viewport toggle in render settings, combined mesh filtering in reload_working_scene
- Fixed Ivar double-filtering that caused material index misalignment
- Fixed UsdPreviewSurface specularColor→specular_weight: was averaging RGB (broke dielectrics), now always 1.0
- Added debug-level material diagnostics (RUST_LOG gated)
- Tested with Nvidia Distributable assets (Glasses, BottleA, Hourglass, BookOpen)

### USD Export Gap Closure (Mar 20, 2026)

- All 10 phases complete — stage metadata, materials, lights, cameras, visibility, curves/points
- OpenPBR MaterialX dual export, GeomSubset export, invisible_ids roundtrip
- Net -955 lines (parser deletion outweighs new code)

---

## Known Issues

- Some Nvidia sample assets render grey — confirmed same in Houdini (asset issue, not BIF)
- Normal maps load correctly but visual impact needs more testing with high-detail assets
- `glass` material type in UsdPreviewSurface may not trigger transmission heuristic if roughness > 0.1

---

## Next Steps

1. M29.5: egui upgrade
2. M30: Persistence (node graph save/load)
3. Consider adding UsdPreviewSurface `specularColor` → OpenPBR `specular_color` tint mapping
