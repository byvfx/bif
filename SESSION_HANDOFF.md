# Session Handoff - March 20, 2026

**Last Updated:** USD export gap closure (10-phase plan, phases 1-4, 7-10 done)
**Next Milestone:** M29.5 egui upgrade (USD gap closure complete)
**Project:** BIF - VFX Scene Assembler & Renderer

---

## Quick Status

| Status | Details |
|--------|---------|
| Complete | Milestones 0-23, M26 (OIDN), M26.1 (Ivar material cache), M19.6 (viewport cleanup) |
| Current | USD spec compliance sessions 1-4 done (read-side complete + Embree subd) |
| Tests | 93 renderer (+4 glass), 72 math, 19 viewer, 79 viewport, 93 bif_core |
| Performance | 60 FPS viewport, 100K instances with LOD, Ivar build ~185ms (2.5M tri combined mesh) |

---

## Recent Work

### USD Export Gap Closure (Mar 20, 2026)

- Wired existing C++ bridge FFI into export_scene: stage metadata, materials w/ bindings, lights (4 types), cameras, visibility
- Added CurvesPrim/PointsPrim scene types, loader calls stage.curves()/points()
- Deleted Rust USDA parser (parser.rs + SceneBuilder), all loading via C++ bridge
- Added invisible_ids to PointCloud for instancer visibility roundtrip
- C++ bridge extended: write_geom_subset (Phase 6), write_invisible_ids (Phase 9) — fully wired
- OpenPBR MaterialX dual export done — write_material outputs both UsdPreviewSurface + OpenPBR
- CurvePreviewRenderer added — LineList rendering for curves + cross-hair points in viewport
- Roundtrip tests added for DomeLight, invisibleIds, GeomSubsets
- **All 10 phases complete**
- Net -955 lines (parser deletion outweighs new export code)

### Bound Material Inspector (Mar 19, 2026)

- Property inspector shows "Bound Material" section for selected mesh prims
- Displays all OpenPBR params: base color swatch+RGB, metalness, roughness, specular weight/IOR, transmission, opacity, emission (conditional), double-sided, texture paths
- Lookup: prim_path → instance → prototype → material
- User wants Houdini-style layout in future iteration

### Implicit Geometry + Lighting Overhaul (Mar 19, 2026)

- C++ bridge tessellates UsdGeomSphere/UsdGeomCube, dedup by radius/size → native instances with material overrides
- Rust loader processes native instances after material binding, clones prototypes for material overrides
- Switched winding from CW→CCW (primitives, normals, C++ tessellation, leftHanded orientation fix)
- DomeLight auto-HDRI, rotation extraction, color temperature, old/new schema compat
- RectLight radiance-based emission (no distance falloff), default sky toggle, black background
- **Known issue:** rough metal sphere may render differently from Karma — needs investigation (could be Ivar sample count or shading model differences)

### Enable .tx Loading + Fix OIIO Mip Crash (Mar 18, 2026)

- `prefer_tx` defaults to `true` — TextureCache loads .tx files when available
- Fixed buffer overflow in `oiio_load_texture_with_mips`: mip loop passed `miplevel=0` to `read_image()` instead of the actual mip index, reading full-res into smaller buffers. Latent bug — never triggered before because PNGs have 1 mip.
- .tx files must be pre-generated via `bif_maketx` (no auto-convert at load time)

### Architecture Decomposition (Mar 17, 2026 - session d)

Renderer God object (~50 fields) decomposed into 3 subsystems:
- **EventBus** — `AppEvent` enum (17 variants) replaces 23 egui temp-data string-keyed slots
- **SelectionManager** — consolidates 5 selection fields + gizmo_state
- **SceneManager** — consolidates 16 scene fields (geometry, materials, USD, undo/redo)
- Renderer now ~25 fields. Phases 5-7 (NodeGraphEvaluator, GpuBackend, EguiAdapter) remain.
- **Needs manual test:** load USD, Vulkan/Ivar switch, pick, undo/redo, batch render

### Glass/Transmission Rendering (Mar 17, 2026)

Implemented glass material support end-to-end:
- C++ bridge extracts `transmission` + `specular_IOR` from MaterialX and `ior` from UsdPreviewSurface
- UsdPreviewSurface heuristic: `opacity < 1` on dielectric → `transmission = 1 - opacity`
- `OpenPbrSurface::scatter_transmission()` — Snell's law refraction + TIR + Schlick Fresnel
- `glass(ior)` constructor, `is_delta()` updated for smooth glass
- 3 new tests, visually verified with test_balls.usd (chrome/glass/rough_metal)

### Disney → OpenPBR Migration (Mar 17, 2026)

Full removal of Disney Principled BSDF, replaced with OpenPBR Surface v1.1:
- `DisneyBSDF` → `OpenPbrSurface`, `disney.rs` → `openpbr.rs`
- All `bif_core::Material` fields renamed to OpenPBR naming
- IOR-based Fresnel: `F0 = ((ior-1)/(ior+1))^2` replaces `specular * 0.08`
- GPU structs repacked with emission + extra_params (64→96 bytes)
- WGSL shader updated with IOR-based F0
- 14 files modified across all 6 crates, all tests pass

### Ivar Texture Loading Fix (Mar 17, 2026)

Fixed 3 bugs causing black objects + wrong textures in Ivar renders:
- **Default material OOB:** build_materials() now appends fallback at index N, matching viewport convention
- **Relative path resolution:** OpenPbrSurface::from_material_with_textures() resolves paths via material.source_dir
- **Per-instance material binding:** combine_with_transforms() uses instance_material_id fallback for meshes without GeomSubsets
- Added texture load failure logging, saturating_sub safety in Embree, 4 new path resolution tests

### UDIM UNC Path Fix + Catch-up Commits (Mar 17, 2026)

Fixed UDIM texture loading regression for UNC network paths (`//server/share/...`):
- v1 attempt normalized paths at API boundaries, broke viewport index_map lookups
- v2 fix: normalize only at filesystem boundaries (`Path::exists()`, `image::open()`), add `starts_with("//")` for UNC absolute detection
- Also committed prior uncommitted work: material export C++ bridge, Rust FFI, scene material GC, Ivar material cache invalidation/prewarm

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
