---
title: UsdSkel Import
type: concept
tags: [usd, skel, skinning, animation, lbs, cpu]
created: 2026-04-10
updated: 2026-04-10
---

# UsdSkel Import

## Summary

BIF v0.13.5 adds UsdSkel-bound character import + CPU linear blend skinning (LBS) so rigged characters exported from Houdini/Maya load, render at bind pose, and deform per-frame when scrubbing the timeline. Math lives in [[bif_core::skinning]]; the USD-facing eval goes through a persistent `UsdSkelCache` on the C++ bridge.

## Details

### Why `UsdSkelCache` over raw attribute reads

The first scaffold of the skel FFI (present-but-dead in v0.13.0) called `UsdSkelSkeleton::GetBindTransformsAttr().Get()` directly. That path:

- Misses composition and layer overrides — attribute reads don't traverse the full resolution stack.
- Can't do time-varying eval — `GetBindTransformsAttr` is a time-invariant schema attribute, not the animated state.
- Doesn't unify SkelRoot binding + skinning queries, so the same data has to be scraped from three different places per-mesh.

v0.13.5 replaces it with `UsdSkelCache` + `UsdSkelRoot::ComputeSkelBindings` + `UsdSkelSkeletonQuery::ComputeJointSkelTransforms(time_code)`. The cache is a `UsdBridgeStage` member so subsequent `compute_skel_xforms(skel_idx, t)` calls are cheap topology walks, not full re-populations.

### Skinning math (LBS)

For each vertex `v` with influences `(joint_i, weight_i)`:

```
palette[j]       = joint_skel_xforms[j] * inv_bind[j] * geom_bind_transform
v_skinned        = Σ_i (weight_i * palette[joint_i].transform_point3(v_bind))
n_skinned        = normalize(Σ_i (weight_i * inverse_transpose_3x3(palette[joint_i]) * n_bind))
```

- `geom_bind_transform` moves the vertex from mesh-local into skel-local space once at bind time.
- `inv_bind` = `bind_world_xform.inverse()` — pre-computed once at load.
- `joint_skel_xforms` comes from `UsdSkelSkeletonQuery::ComputeJointSkelTransforms(time_code)`.
- Normals use the proper inverse-transpose of the upper 3×3 so joints with non-uniform scale don't tilt the surface incorrectly (the fast-path "same-matrix-as-position" approximation was rejected).

### Per-time-code eval API

```rust
pub fn compute_skel_xforms(
    &self,
    skel_index: usize,
    time_code: f64,
) -> UsdBridgeResult<Vec<Mat4>>
```

Cheap to call repeatedly at different times — the underlying `UsdSkelSkeletonQuery` is cached inside the C++ bridge's `UsdSkelCache`, so each call only runs the per-time topology walk + matrix compose.

### Gotcha: USD time code clamping

USD silently clamps out-of-range time code queries to the nearest authored keyframe. A test that queried frames 0 and 20 on HumanFemale.walk.usd (authored range **101**-129) returned the same bind-pose values at both times — max delta 0.0 — not because eval was broken but because both queries clamped to frame 101. Always query within the stage's `get_timeline()` range when asserting on animation deltas.

### Viewport hot path

`Renderer::update_animation` dispatches into `update_skinning(frame)` whenever `scene.skinned_meshes` is non-empty and the frame has advanced beyond the 0.5-frame tolerance. Per skinned entry:

1. `stage.compute_skel_xforms(skel_idx, frame)` — per-frame palette source.
2. `skinning::compute_skin_matrices(&skin, &xforms)` — collapses to one matrix per joint.
3. `skinning::skin_positions(&skin, &bind_positions, &palette, &mut scratch)` — LBS into pre-allocated scratch.
4. Write scratch into `mesh_data.vertices[range]` (single-mesh or combined-buffer via `MeshRange::usd_mesh_index`).
5. One `queue.write_buffer(&vertex_buffer, 0, ...)` at the end of the pass.

Multi-draw mode is deferred — per-prototype GPU buffers live separately from `mesh_data.vertices` and need a parallel update path.

## In BIF

- **C++**: `cpp/usd_bridge/usd_bridge.cpp` (`cache_skeleton_data`, `usd_bridge_compute_skel_skin_xforms`, `UsdBridgeStage::skel_cache`)
- **Rust FFI**: `crates/bif_core/src/usd/ffi_raw.rs`, `crates/bif_core/src/usd/cpp_bridge.rs` (`UsdStage::compute_skel_xforms`)
- **Math**: `crates/bif_core/src/skinning.rs`
- **Mesh types**: `crates/bif_core/src/mesh.rs` (`SkinBinding`, `Mesh::skin`, `Mesh::bind_positions`)
- **Loader**: `crates/bif_core/src/usd/loader.rs` (per-mesh `get_skin_binding` + inv-bind precompute)
- **Viewport scene**: `crates/bif_viewport/src/scene_manager.rs` (`SkinnedMeshEntry`)
- **Viewport registration**: `crates/bif_viewport/src/scene_loader.rs` (post-vertex-animation scan)
- **Viewport hot path**: `crates/bif_viewport/src/animation.rs` (`update_skinning`)
- **Fixture**: `test_assets/skel/two_bone_arm.usda`

## Related

- [[Stage Layer Prim]]
- [[BIF USD Integration]]
- [[Geometry Schemas]]
