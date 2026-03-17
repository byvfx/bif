# Session Handoff - March 16, 2026

**Last Updated:** USD spec compliance sessions 1-6 (read-side + Embree subd + full export: material/camera/light/render settings)
**Next Milestone:** USD spec compliance sessions 7-8 (payloads/variants/advanced), then M29.5 egui upgrade
**Project:** BIF - VFX Scene Assembler & Renderer

---

## Quick Status

| Status | Details |
|--------|---------|
| Complete | Milestones 0-23, M26 (OIDN), M26.1 (Ivar material cache), M19.6 (viewport cleanup) |
| Current | USD spec compliance sessions 1-4 done (read-side complete + Embree subd) |
| Tests | 85 renderer (+1), 72 math (+31), 19 viewer (+19), 79 viewport, 93 bif_core |
| Performance | 60 FPS viewport, 100K instances with LOD, Ivar build ~185ms (2.5M tri combined mesh) |

---

## Recent Work

### USD Spec Compliance Sessions 1-4 (Mar 16, 2026)

Read-side remediation across C++ bridge, Rust FFI, and loader:
- **Session 1 (Mesh):** visibility, doubleSided, subdivisionScheme, normalsInterpolation, horizontalAperture
- **Session 2 (Instancer+Stage):** PI velocities/angularVelocities/invisibleIds, timeCodesPerSecond, displayColor/Opacity, resetXformStack
- **Session 3 (Lights+Schemas):** CylinderLight, DiskLight, ShapingAPI, UsdGeomPoints, arbitrary primvar query API
- **Session 4 (Embree Subd):** Catmull-Clark via RTC_GEOMETRY_TYPE_SUBDIVISION, polygon topology + crease data from USD

Loader skips invisible meshes, filters invisible instancer IDs. Scene browser shows real inherited visibility.

Next: Session 5 (export materials+bindings), Sessions 6-8. Use OpenPBR for MaterialX export.

### Architecture Cleanup: Renderer Decomposition (Mar 16, 2026)

Extracted 4 sub-structs from Renderer (was 99 fields, 3 already extracted):
- **GpuContext** (4 fields): surface, device, queue, config
- **CameraState** (7 fields): camera, uniform, buffer, bind_group, viewport source, lock state
- **IvarContext** (8 fields): ivar_state, GPU texture/pipeline/materials
- **NodeGraphContext** (11 fields): graph state, node maps, instancer results, caches

Also: removed bif_math re-exports from bif_renderer, converted DenoiseError to thiserror.

Phase 1 (remove UsdStage Send+Sync) was **skipped** — deep dive confirmed the unsafe impls are sound and required for batch_render's rayon parallelism.

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
