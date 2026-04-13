---
title: UsdSkel Import
type: concept
tags: [usd, skel, skinning, animation, lbs, cpu]
created: 2026-04-10
updated: 2026-04-12
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

### Gotcha: `IsRigidlyDeformed()` is broader than single-joint

`UsdSkelSkinningQuery::IsRigidlyDeformed()` returns true for any mesh whose binding is **per-prim** (same influence block across every vertex) — **not just single-joint**. Uniform multi-bone bindings qualify: HumanFemale's hair binds to 3 head/neck joints at w=0.333 each; fingernails bind to 2 finger-tip joints at w=0.5 each.

A compact `SkinKind::Rigid { joint_idx, weight }` encoding that extracts only `joint_indices[0]` + `joint_weights[0]` works for single-joint rigid (eyes, shoes) but silently mis-renders multi-joint rigid: the fractional weight scales every vertex toward the first bone's origin. This was the v0.13.5.2 → v0.13.6 bug on HumanFemale hair/nails. The fix gates the compact encoding on `element_size == 1`; multi-joint rigid meshes broadcast the authored block to per-vertex layout and run through the standard `SkinKind::PerVertex` path. Regression guard: `skinning::tests::rigid_matches_pervertex_single_influence`.

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

## Known Limitations

Surfaced by code review and HumanFemale validation but deferred to follow-up versions:

- **Animated SkelRoot xform not handled.** The static instance transform we use for skinned meshes is the SkelRoot's world xform at load time. If the SkelRoot itself has time-sampled xformOps (character on a moving platform, or a Skel under an animated parent group), the character will deform in place but the whole rig will be frozen at its load-frame world position. Fix: pull `AnimatedTransform` from the SkelRoot prim, not the mesh prim, when `is_skinned`. → v0.13.6 or v0.14.0.

- **Invalid joint indices silently drop their weight contribution.** The C++ bridge emits `-1` as a sentinel when a mesh's local joint index can't be remapped to a skeleton joint; the Rust loader converts that to `u32::MAX`, which the bounds check in `skin_positions` skips. The dropped influence's weight is not redistributed across remaining joints, so the affected vertex is pulled toward the origin proportional to the lost weight. Defensive vs hard-warn — current choice is silent. For VFX rigs in practice this never fires; the malformed-skin path is for robustness against bad data.

- **Multi-instance non-identity transforms in combined-buffer mode.** When the renderer uses the combined vertex buffer path (one prototype, multiple instances at different transforms), the skinned positions are written once into mesh-local space. Multiple instances of the same skinned prototype with different world transforms aren't supported in this path. Multi-draw mode handles per-prototype correctly. Surfaces only with crowd-style instanced characters.

- **Per-frame normal-matrix recomputation.** `skin_normals` rebuilds the per-joint inverse-transpose 3×3 matrix on every call. For per-frame normal upload, this should be hoisted into the per-frame palette and computed alongside positions. Currently dormant because Phase 3 doesn't yet upload skinned normals to the GPU per frame.

- **Hand-rolled joint-order remap.** `cache_skeleton_data` builds the mesh-local → skel-global joint mapping by hand via `UsdSkelBindingAPI::GetJointsAttr` matched against `UsdSkelSkeletonQuery::GetJointOrder`. The canonical USD pattern is `UsdSkelSkinningQuery::GetJointMapper()`, which handles identity/null cases internally. Functionally equivalent today; cleanup target for v0.13.6.

- **Rigidly-deformed mesh broadcast wastes memory.** ~~Cleanup target for v0.13.6.~~ **Resolved in v0.13.5.2 / v0.13.6:** `SkinKind::Rigid { joint_idx, weight }` enum variant on `SkinBinding` compacts single-joint rigid bindings (eyes, shoes) to 8 bytes. Multi-joint rigid bindings (hair, nails) intentionally broadcast through `SkinKind::PerVertex` — see the `IsRigidlyDeformed()` gotcha above for why compaction must be gated on `element_size == 1`.

## Performance Notes

- Per-skeleton joint xforms are deduped across all `SkinnedMeshEntry` iterations in `update_skinning` via a `HashMap<skel_idx, Vec<Mat4>>` cache built once per call. For HumanFemale's 77 prototypes bound to a single skeleton, this collapses 77 FFI calls per frame to 1.
- Per-entry `skinned_scratch: Vec<Vec3>` is pre-allocated at registration time so the LBS hot path is allocation-free.
- `inv_bind_matrices` is precomputed once per skeleton at load (`bind_world.inverse()` per joint), not recomputed per frame.

## Related

- [[Stage Layer Prim]]
- [[BIF USD Integration]]
- [[Geometry Schemas]]
