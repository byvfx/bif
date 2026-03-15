# Session Handoff - March 15, 2026

**Last Updated:** ALab texture limit, Ivar materials, UDIM UV transform fixes
**Next Milestone:** M29.5 egui upgrade
**Project:** BIF - VFX Scene Assembler & Renderer

---

## Quick Status

| Status | Details |
|--------|---------|
| Complete | Milestones 0-23, M26 (OIDN), M26.1 (Ivar material cache), M19.6 (viewport cleanup) |
| Current | ALab texture/material fixes — 272 textures loading, UDIM UV transform, Ivar tri_mat_ids |
| Tests | 84 renderer, 41 math, 79 viewport, 93 bif_core |
| Performance | 60 FPS viewport, 100K instances with LOD, Ivar build ~185ms (2.5M tri combined mesh) |

---

## Recent Work

### ALab Texture/Material Fixes (Mar 15, 2026)

Three fixes for ALab rendering:

1. **MAX_VIEWPORT_TEXTURES 128→512** — ALab has 272 textures, was truncated at 127
2. **Ivar per-triangle material IDs** — combined mesh got empty tri_mat_ids, everything rendered as material 0. Post-fills per-instance material IDs after combine (60 unique materials now assigned)
3. **UDIM UV transformation** — atlas stitcher worked but shader didn't transform UVs for multi-tile layout. Added grid metadata through load/upload pipeline, packed into MaterialGpu.extra_indices, shader transforms UVs with signed-math clamping

VFX code review findings addressed:
- Unsigned integer underflow in shader UDIM UV math (critical)
- UDIM grid lookup filtering index 0 (default white texture)
- has_udim flag only set after successful upload

### Pre-existing Issues (not caused by these changes)

- **Viewport walls see-through** — back-face culling (`cull_mode: Back`) eats single-sided ALab walls
- **Ivar green tint** — TextureCache doesn't handle UDIM `<UDIM>` patterns, materials fall back to flat base_color

### Previous Session: ALab Material/Texture Binding (Mar 14, 2026)

- 7 bugs fixed in material/texture binding after instance proxy support
- C++ bridge traversal with `UsdTraverseInstanceProxies()`
- UDIM atlas stitching, purpose filtering, texture path normalization

---

## Blockers / Known Issues

- `test_should_restart_no_render` — known flaky timing test
- bif_viewport tests need USD DLLs (`setup_usd_env.ps1`)
- bif_viewer.exe locked during build if app is running
- Ivar TextureCache lacks UDIM support — UDIM materials render as flat color in ray tracer

---

## Next Steps

1. M29.5: egui 0.29→0.30 upgrade + egui-snarl 0.5→0.6 + vertical node layout
2. M30: Node graph save/load (`.bif`/`.bifa`) + evaluation modes + cache node
3. Future: Add UDIM support to TextureCache for Ivar ray tracer
4. Future: Investigate back-face culling for single-sided USD geometry (double-sided attribute)
