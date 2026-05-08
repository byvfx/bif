# BIF Consolidated Code Review — 2026-03-23

**Reviewers:** Code Reviewer, VFX Pipeline Specialist, Software Architect
**Scope:** Full codebase (~48k LOC, 6 crates, 390 tests) @ commit c020aab

---

## Summary

| Severity | Code Review | VFX Review | Architecture | Total (deduplicated) |
|----------|------------|------------|-------------|---------------------|
| CRITICAL | 3 | 4 | 1 | 8 |
| IMPORTANT | 12 | 9 | 4 | 21 (some overlap) |
| NICE-TO-HAVE | 8 | 8 | 4 | 18 |

**Overall verdict:** Solid foundation. Clean crate boundaries, good error handling, proper FFI safety docs, 390 tests. The critical issues are real but bounded — mostly energy conservation in the renderer and one architectural coupling that blocks M30.

---

## CRITICAL — Fix Before Next Milestone

### 1. OpenPBR diffuse missing specular Fresnel attenuation [VFX-C1]

**File:** `crates/bif_renderer/src/openpbr.rs:469-483`
**Impact:** Energy creation at grazing angles — renders too bright on glossy dielectrics.
**Fix:** Multiply diffuse by `(1.0 - fresnel_at_angle)`. ~10 lines.

### 2. SHARC radiance cache stores direct-only (biased) [VFX-C2, Code-C1]

**File:** `crates/bif_renderer/src/renderer.rs:257-265`
**Impact:** Dark splotches in GI-heavy scenes. Cache converges to wrong answer. Torn reads can also produce NaN (Code-C1).
**Fix:** (a) Add NaN guard after cache lookup (quick). (b) Defer cache write to include indirect contribution (complex, consider for future milestone).

### 3. Shadow ray offset uses geometric normal, not shading normal [VFX-C3]

**File:** `crates/bif_renderer/src/renderer.rs:203`
**Impact:** Dark bands at normal map discontinuities (shadow terminator artifacts).
**Fix:** Use shading normal for offset. ~5 lines.

### 4. Distant light angle: degrees vs radians ambiguity [VFX-C4]

**File:** `crates/bif_renderer/src/light.rs:64-84`, `crates/bif_core/src/scene.rs:452`
**Impact:** Shadow softness completely wrong if units mismatch.
**Fix:** Verify conversion path from USD loader → scene → light constructor. Add `.to_radians()` if needed.

### 5. NodeGraphContext uses egui_snarl::NodeId in non-UI code [Arch-C1]

**File:** `crates/bif_viewport/src/lib.rs:302-315`
**Impact:** Blocks clean Qt migration. Blocks M30 persistence (serialized IDs depend on egui internals).
**Fix:** Introduce `GraphNodeId(u64)` newtype, bidirectional mapping. ~2 sessions.

### 6. EmbreeScene Drop ordering relies on field declaration order [Code-C2]

**File:** `crates/bif_renderer/src/embree.rs:1130-1143`
**Impact:** Potential use-after-free if struct fields reordered. Currently safe but fragile.
**Fix:** Document the field-order invariant with safety comment. ~5 min.

### 7. Production-path unwrap() in PointInstancer node [Code-C3]

**File:** `crates/bif_viewport/src/node_graph/viewer.rs:639-640`
**Impact:** App crash if guard logic ever changes.
**Fix:** Replace with `let (Some(a), Some(b)) = ... else { return; }`. ~2 lines.

### 8. Normal matrix crashes on non-invertible transforms [VFX-I6]

**File:** `crates/bif_renderer/src/embree.rs:438-441`
**Impact:** NaN/black instances for zero-scale transforms (common in USD for "hidden" instances).
**Fix:** Check determinant, fallback to `Mat3::IDENTITY`. ~5 lines.

---

## IMPORTANT — Fix Within Next 2-3 Milestones

### Rendering & Pipeline

| # | Finding | File | Source |
|---|---------|------|--------|
| I1 | OpenPBR `is_delta()` too loose for transmission (wastes shadow rays) | openpbr.rs:521 | VFX-I4 |
| I2 | Embree device created per scene (should be shared) | embree.rs:213 | VFX-I5 |
| I3 | SHARC lock-free write has TOCTOU race (flicker in cached regions) | radiance_cache.rs:444 | VFX-I1 |
| I4 | EXR missing color space metadata (chromaticities) | exr_writer.rs:239 | VFX-I2 |
| I5 | No mipmap LOD selection in path tracer (texture aliasing) | texture.rs:187 | VFX-I3 |
| I6 | Per-triangle tangents, not per-vertex (normal map seam artifacts) | embree.rs:390 | Code-I9 |
| I7 | Mesh dedup hash samples only 10 vertices (collision risk) | loader.rs:153 | Code-I5 |
| I8 | NaN guards missing on HDR direction_to_uv and texture UV | hdr.rs:123, texture.rs:187 | Code-I7/I8 |
| I9 | Crease data not validated before Embree FFI | embree.rs:620 | Code-I10 |
| I10 | Point light epsilon additive instead of max (wrong falloff near light) | light.rs:156 | VFX-I9 |

### Architecture & Maintainability

| # | Finding | File | Source |
|---|---------|------|--------|
| I11 | `run_egui_frame` 500+ lines mixing UI + state management | render.rs:135-700 | Arch-I1 |
| I12 | Node graph eval not separated from rendering (untestable) | node_graph/viewer.rs | Arch-I2 |
| I13 | Renderer methods doing pure logic require GPU context to test | render.rs | Arch-I3 |
| I14 | Dual state mutation paths (direct egui + EventBus) | render.rs | Arch-I4 |
| I15 | `set_last_instance_purpose` fragile "must call after" API | scene.rs:667 | Code-I6 |
| I16 | UsdStage Send+Sync relies on undocumented C++ thread-local behavior | cpp_bridge.rs:1701 | Code-I3 |
| I17 | `is_multiple_of` is nightly-only | loader.rs:52 | Code-I11 |

---

## NICE-TO-HAVE — When Convenient

- Extract `GpuMaterialState` + `GpuTextureState` sub-structs (12 fewer fields on Renderer)
- Move type definitions out of lib.rs (`UsdLoadStatus`, `SceneInstances`, etc.)
- Derive `Copy` on `Transform` (avoid clone overhead on 48-byte type)
- `HdrImage::downscale_to_max_dim` clones full image unnecessarily → use `Cow`
- VNDF GGX sampling for 2-4x convergence improvement on rough metals
- Texture cache LRU eviction policy
- Resolution-dependent pole clamping for HDRI PDF
- Orthonormal basis function dedup (bif_math vs bif_renderer/light.rs)
- Remove redundant `Prototype::bounds` (always == `mesh.bounds`)
- IBL module uses `[f32; 3]` arrays instead of Vec3
- `UsdBridgeError::from(Success)` uses `unreachable!()` instead of safe fallback
- Box filter boundary condition (half-open vs closed interval)

---

## Open Questions (Need Your Input)

1. **Double-sided material + transmission:** Does `front_face` flag handle IOR ratio correctly for back-face hits on double-sided geometry? (openpbr.rs:717)
2. **USD xform reset:** Is `resets_xform_stack` consumed correctly in the C++ bridge?
3. **HDRI rotation:** CDFs are rotation-independent only for Y-axis rotation — is this a valid constraint going forward?
4. **Area light MIS:** If emissive meshes are ever added, the current path tracer will double-count (NEE + direct hit without MIS weight).

---

## Strengths (Keep Doing)

All three reviewers independently praised:

- **Clean crate dependency graph** — 5-layer acyclic, no circular deps
- **FFI safety documentation** — Both USD and Embree unsafe blocks well-commented
- **EventBus pattern** — Typed, frame-scoped, Qt-migration ready
- **Error handling** — thiserror in libraries, anyhow in app, consistent throughout
- **Sub-struct extraction** — GpuContext, CameraState, IvarContext, SceneManager
- **PrimDataProvider trait** — Clean abstraction over USD vs procedural data
- **Test discipline** — 390 tests, AAA pattern, good edge case coverage
- **Feature flags** — oiio/oidn with graceful degradation

---

## Recommended Priority Order

**Immediate (before next feature work):**

1. #1 OpenPBR energy conservation (~10 lines)
2. #3 Shadow ray shading normal (~5 lines)
3. #4 Distant light degrees/radians (verify + fix)
4. #6 Embree Drop safety comment (~5 min)
5. #7 Node graph unwrap → pattern match (~2 lines)
6. #8 Normal matrix zero-scale guard (~5 lines)
7. NaN guard after SHARC cache lookup (Code-C1 quick fix)

**Before M30 (persistence):**
8. #5 GraphNodeId newtype (~2 sessions)
9. I12 Extract graph evaluation logic

**Before M33+ (Qt migration planning):**
10. I11 Per-panel state extraction
11. I13 Extract testable logic from Renderer methods

---

*Individual reports: `code_review_2026-03-23.md`, `vfx_review_2026-03-23.md`, `architecture_review_2026-03-23.md`*
