# Session Handoff - March 3, 2026

**Last Updated:** OIDN installed + tested, setup_usd_env.ps1 updated
**Next Milestone:** M29 validation, visual denoise test, Ivar Xform
**Project:** BIF - VFX Scene Assembler & Renderer

---

## Quick Status

| Status | Details |
|--------|---------|
| Complete | Milestones 0-23, M26 (OIDN denoising) |
| Current | M29 export validation |
| Tests | 95+ passing (69 renderer w/ OIDN, 24 viewport, 27+ bif_core) |
| Performance | 60 FPS viewport, 100K instances with LOD |

---

## Recent Work

### M26 OIDN Denoising (Mar 3, 2026)

| Change | Details |
|--------|---------|
| `Material::albedo()` | New trait method + impls for Lambertian, Metal, DisneyBSDF, DiffuseLight, DummyMaterial |
| Albedo AOV pipeline | `AovData.albedo` → `BucketResultWithAovs.albedos` → `IvarState.albedo_buffer` |
| `AovChannel::Albedo` | Viewport preview with gamma-corrected display |
| `denoise.rs` module | `denoise_beauty()` with beauty/albedo/normal guide buffers, cfg-gated |
| Feature flag `oidn` | Optional `oidn = "2.3"` dep, propagated through all 3 crates |
| Viewport button | "Denoise (OIDN)" after render complete, grayed-out when disabled |
| Batch denoise | Checkbox in AOV settings, denoise before EXR write |
| `ExrOutput.albedo` | Albedo buffer captured in batch render (OIDN input, not written to EXR) |
| Tests | 10+ new tests: dimension mismatch, material albedo, buffer lifecycle, denoise state |

### VFX Review Fixes (Feb 26, 2026 — Session 2)

| Change | Details |
|--------|---------|
| CachedSceneGraph | Pre-computed children_index, rebuild only on dirty flag |
| ProceduralPrimKind enum | Mesh/PointInstancer/Scope replaces optional fields |

---

## Current State

| Metric | Value |
|--------|-------|
| Build (dev) | ~10s |
| Tests | 95+ passing |
| Vulkan FPS | 60 (VSync with Fifo) |
| Crates | 6 (math, core, renderer, viewport, viewer, maketx) |

### What Works

**Denoising (M26):**
- Feature-gated OIDN: `cargo build --features oidn`
- Albedo AOV captured from all materials (Lambertian, Metal, Disney, etc.)
- Viewport: render → click "Denoise (OIDN)" → denoised beauty display
- Batch: checkbox auto-denoises before EXR write
- Without OIDN feature: UI shows grayed-out button, `DenoiseError::NotEnabled`

**USD Export (M29):**
- `export_scene()` writes prims, xform overrides, keyframes, point clouds, meshes
- Standalone cube export confirmed in usdview

**Scene Browser:**
- CompositeProvider merges USD stage + procedural prims

### Known Issues

- **OIDN visual test pending** — build+unit tests pass, need manual viewport/batch visual verification
- **Ivar doesn't reflect Xform transforms** — baked mesh_data path doesn't include Xform mods
- Xform prim_filter is V1 placeholder
- bif_core tests need USD DLLs (run via `setup_usd_env.ps1`)
- `test_should_restart_no_render` — pre-existing timing-sensitive test failure
- Renderer struct ~60 fields (God object)

---

## Next Session

**Goal:** Visual denoise test, M29 validation

1. Visual test: render scene → click Denoise (OIDN) → verify quality
2. Visual test: batch render with denoise checkbox
3. Investigate instancer prim_path nesting bug
4. Ivar: apply Xform transforms to baked mesh_data
5. USD asset authoring (Houdini component builder port)

---

## Quick Commands

```bash
# Build
cargo build                    # Dev (~10s)
cargo build --features oiio    # With OIIO support
cargo build --features oidn    # With OIDN denoising

# Test
cargo test -p bif_math         # 41 tests
cargo test -p bif_renderer     # 68 tests (includes denoise, material albedo)

# USD tests (require setup)
. .\setup_usd_env.ps1
cargo test -p bif_core -- --test-threads=1

# Run
cargo run -p bif_viewer                          # Without OIIO/OIDN
cargo run -p bif_viewer --features oidn          # With OIDN denoising
```

---

**Branch:** main
**Ready for:** Visual denoise test, M29 validation, Ivar Xform fix
