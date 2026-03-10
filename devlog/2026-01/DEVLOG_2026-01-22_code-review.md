# Development Log - 2026-01-22 (Code Review Fixes)

## Goals
- Apply code review fixes across renderer codebase

## What I Did

### Phase 1: Hot path cleanup
- Gated `MISS_COUNT`/`HIT_COUNT` AtomicU32 statics + fetch_add in `#[cfg(debug_assertions)]`
- Added `debug_assert!` bounds checks for `prim_id` on uv/normal/tangent data
- Gated `AtomicU32`/`Ordering` import with `#[cfg(debug_assertions)]`

### Phase 2: Deduplicate build_orthonormal_basis
- Created `crates/bif_math/src/basis.rs` with Frisvad's method + tests
- Removed duplicate from `disney.rs` and `material.rs`
- Both now import `bif_math::build_orthonormal_basis`

### Phase 3: Remove unnecessary clone
- Removed `transforms.clone()` in viewport `build_ivar_scene` — value already owned

### Phase 4: Tangent storage optimization
- Store 1 tangent per triangle (was 3 identical copies)
- Hit path uses direct `tangent_data[prim_id]` lookup instead of barycentric interpolation

### Phase 5: Normal mapping correctness
- Gram-Schmidt: fallback to `build_orthonormal_basis` when `length_squared < 1e-8`
- Flip bitangent on back-face hits for TBN consistency
- `apply_normal_map`: guard normalize with length check, fallback to input normal

### Phase 6: Texture handling
- Added `TextureCache::load_linear()` — skips sRGB gamma, just divides by 255
- Rewrote `sample_channel` with bilinear interpolation (was nearest-neighbor)
- `from_material_with_textures`: data textures (normal/roughness/metallic/opacity) use `load_linear`

### Phase 7: Opacity pass-through
- Added `pass_through: bool` to `ScatterResult`
- Stochastic cutout sets `pass_through: true`
- Converted `ray_color` from recursive to iterative with throughput accumulation
- Pass-through scatters don't decrement depth counter

## Learnings
- Tangent is constant per triangle (computed from edges/UVs), no need to store per-vertex
- sRGB→linear only applies to perceptual color (albedo); data maps are already linear
- Iterative path tracing avoids stack overflow on deep pass-through chains

## Next Session
- Visual regression test with normal maps + opacity scene
- Consider OIIO path for `load_linear` (currently only image crate path)
