# Session Handoff - March 6, 2026

**Last Updated:** Fix CI: vcpkg toolchain detection + vcpkg install in check job
**Next Milestone:** Visual denoise test, VFX code review, Ivar Xform
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

### Fix CI vcpkg detection (Mar 6, 2026)

| Change | Details |
|--------|---------|
| `build.rs` | Check vcpkg.cmake toolchain file (not just dir), pass resolved path to `build_usd_bridge()` |
| `ci.yml` | Add `lukka/run-vcpkg@v11` to check job (installs USD/OIIO, sets VCPKG_ROOT) |

### Fix CI Check Job (Mar 6, 2026)

| Change | Details |
|--------|---------|
| `build.rs` | Graceful vcpkg skip — early return when vcpkg not found, allows clippy without USD env |
| `ci.yml` | Removed bif_renderer/bif_viewport test steps (can't link without USD libs) |

### CI/CD Pipeline + CHANGELOG (Mar 5, 2026)

| Change | Details |
|--------|---------|
| `ci.yml` | GitHub Actions: `check` (fmt/clippy/test) on push/PR, `release` (vcpkg+OIDN build + DLL bundle) on v* tags |
| `vcpkg.json` | Manifest for USD (`pxr`) + OIIO (`openimageio`) deps |
| `CHANGELOG.md` | Keep a Changelog format, auto-updated during `/bif-commit`, used for release notes |
| `bif-commit.md` | Added CHANGELOG update step, co-author → Opus 4.6 |
| `CLAUDE.md` | CHANGELOG added to pre-commit checklist |

### VFX Review Fixes — OIDN (Mar 4, 2026)

| Change | Details |
|--------|---------|
| `denoise.rs` | Warn normal-without-albedo, bytemuck `cast_slice` zero-copy (no manual f32→u8) |
| `material.rs` | `Dielectric::albedo()` Schlick F0 `(1-ior)/(1+ior)` squared |
| `hittable.rs` | `DummyMaterial::albedo()` doc comment explaining white default |
| `renderer.rs` | First-hit albedo only (`depth == 0`), skip recursive bounces |
| `batch_render.rs` | Conditional albedo alloc (only when OIDN enabled + denoise checked), removed `allow(unused_mut)` |
| `ivar_state.rs` | `DenoiseState` sub-struct extracted from `IvarState` (channel, thread handle, flag) |
| `ivar_build.rs` | Async denoise: spawns thread + channel + poll loop instead of blocking UI |
| `render.rs` | Poll denoise result, show "Denoising..." label in viewport |

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
- Albedo AOV captured from all materials (Lambertian, Metal, Disney, Dielectric, etc.)
- Viewport: render → click "Denoise (OIDN)" → async denoise with "Denoising..." label
- Batch: checkbox auto-denoises before EXR write (conditional albedo alloc)
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

**Goal:** Visual denoise test, VFX code review, Ivar Xform

1. Visual test: render scene → click Denoise (OIDN) → verify async denoise quality
2. Visual test: batch render with denoise checkbox
3. VFX code review on commit `71aae62`
4. Investigate instancer prim_path nesting bug
5. Ivar: apply Xform transforms to baked mesh_data
6. USD asset authoring (Houdini component builder port)

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
**Ready for:** Visual denoise test, VFX code review on `71aae62`, Ivar Xform fix
