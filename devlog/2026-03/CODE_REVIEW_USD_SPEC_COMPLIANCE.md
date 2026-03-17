# Code Review: USD Spec Compliance (Sessions 1-8 + Phases A-D)

**Reviewer:** Claude Opus 4.6 (1M context)
**Date:** 2026-03-16
**Commits:** b07b8c6, 7d728d6, d3d60f2, 5449fb7, 09d6246, 8caf232, 6921589
**Scope:** 26 USD spec gaps across 7 commits, ~9200 new lines of code

---

## What Was Done Well

- Consistent FFI pattern: all 71 extern "C" functions follow the same `raw struct -> fill -> convert to Rust` model. Clean separation of concerns.
- Every `from_raw_parts` call is guarded by a null/zero check. Zero unchecked pointer dereferences found in 39 call sites.
- C++ side: every exported function that takes a stage pointer validates null before use.
- Error handling: `UsdBridgeError` enum maps cleanly to Rust `Result` with descriptive messages.
- The lazy caching pattern (`*_cached` flags) is well-structured with proper invalidation on `load_payload`, `unload_payload`, and `set_variant_selection`.
- `CString` usage is correct throughout -- Rust strings are converted via `CString::new()` with error handling for interior NULs.
- OpenPBR MaterialX parameter names (`base_weight`, `base_color`, `base_metalness`, `specular_weight`, `specular_roughness`, `geometry_opacity`, `emission_luminance`, `emission_color`) all match the current OpenPBR v1.1 spec.
- UsdPreviewSurface export uses `"specularLevel"` (correct per USD spec) and `"mtlx:surface"` output for MaterialX.
- Drop impl on `UsdStage` and `UsdEditLayer` properly closes/frees the C++ handle.
- Variant selection correctly invalidates all 10 cache flags, preventing stale data.
- `CachedLight` pointer lifetime is safe: `light_link_include_ptrs` holds `c_str()` pointers from the `light_link_includes` vector -- both stored in the same struct, so they share lifetime.

---

## Critical Issues (Must Fix)

### 1. CRITICAL: Embree Subdivision Index Buffer Use-After-Free

**File:** `crates/bif_renderer/src/embree.rs`, line 603-654

```rust
let subd_index_data: Vec<u32> =
    sd.polygon_indices.iter().map(|&i| i as u32).collect();
rtcSetSharedGeometryBuffer(geom, ..., subd_index_data.as_ptr(), ...);
// ...
let _ = subd_index_data; // Dropped -- pointer held by Embree via _face_data lifetime
```

`let _ = subd_index_data;` is a **drop binding** -- it drops the Vec immediately. Embree still holds a raw pointer to the freed buffer. The comment is wrong: `_face_data` is the face-vertex-counts buffer, not the index buffer.

The `EmbreeScene` struct stores `_face_data`, `_crease_index_data`, and `_crease_weight_data` to keep those alive, but has no field for the subdivision polygon indices.

**Mitigating factor:** Currently no caller passes `Some(SubdivData)` -- all call sites pass `None`. This code path is unreachable today, so it will not crash in production. But it will become a crash bug the moment someone enables subdivision rendering.

**Fix:** Add `_subd_index_data: Vec<u32>` field to `EmbreeScene`, store `subd_index_data` into it, and return it in the struct constructor. Replace `let _ = subd_index_data;` with assignment to the field.

---

## Important Issues (Should Fix)

### 2. UsdPreviewSurface Specular Read/Write Mismatch

**File:** `cpp/usd_bridge/usd_bridge.cpp`

- **Read** (line 1423): reads `"specularColor"` (Color3f), averages RGB to scalar
- **Write** (line 3769): writes `"specularLevel"` (Float)

These are different parameters in the UsdPreviewSurface spec. The read side should also read `"specularLevel"` (the Float parameter that was added in USD 24.08). The current read fallback to `specularColor` is acceptable for backwards compatibility, but the primary read should try `specularLevel` first and fall back to `specularColor`.

Round-trip is broken: export writes `specularLevel`, re-import reads `specularColor` and finds nothing.

### 3. Rust `&self` on Mutating FFI Calls (Interior Mutability Ambiguity)

**File:** `crates/bif_core/src/usd/cpp_bridge.rs`, lines 2998, 3010, 3112

`load_payload`, `unload_payload`, and `set_variant_selection` take `&self` but cast `self.raw` to `*mut` to call C++ functions that invalidate all caches. This is semantically `&mut self` work. The comment says "the C++ side handles internal mutability" but this breaks Rust's aliasing guarantees if there are ever concurrent callers.

Since BIF is single-threaded for USD access today, this is safe in practice. But consider either:
- Changing these to `&mut self` (correct Rust semantics), or
- Wrapping the raw pointer in a `Cell`/`UnsafeCell` and documenting the interior mutability contract.

### 4. C++ Destructor Missing Explicit Clear for New Caches

**File:** `cpp/usd_bridge/usd_bridge.cpp`, lines 338-349

`~UsdBridgeStage()` explicitly clears `meshes`, `instancers`, `materials`, `lights`, `mesh_material_paths`, `all_prims`, `root_paths`, `root_path_ptrs` but does NOT clear `points_prims`, `curves_prims`, `skeletons`, `skin_bindings`, `volumes`, `mesh_primvars`, `mesh_animations`, `instancer_animations`, `camera_animations`, `vertex_animations`.

While std::vector destruction handles this automatically, the explicit clear pattern was established for consistency and to ensure pointer stability during teardown (the `root_path_ptrs` and similar `*_ptrs` vectors hold raw `c_str()` pointers that could dangle if the ordering were wrong). Be consistent -- either clear all or remove the explicit clears entirely.

### 5. Light Linking: `CanContainPropertyName` Guard Is Unnecessary

**File:** `cpp/usd_bridge/usd_bridge.cpp`, line 1832

```cpp
if (UsdCollectionAPI::CanContainPropertyName(TfToken("collection:lightLink:includeRoot"))) {
```

This static method checks if a property name is valid for *any* collection, not whether this specific prim has a lightLink collection. The check always returns true for a properly-formed collection property name. The actual guard should be the `if (lightLink)` check on line 1834 (which exists). The outer check is misleading -- it looks like a feature gate but isn't one. Consider removing it or replacing with a version check if backward-compatibility is the intent.

### 6. Thread-Local String Lifetime for Variant Names

**File:** `cpp/usd_bridge/usd_bridge.cpp`, lines 4208-4209

```cpp
static thread_local std::vector<std::string> tl_variant_names;
static thread_local std::string tl_variant_selection;
```

The `out_name` / `out_selection` pointers returned to Rust point to `c_str()` of these thread-locals. The Rust side must copy the string before making another variant query call on the same thread, or the pointer invalidates. The Rust wrappers do correctly copy via `CStr::from_ptr().to_string_lossy().into_owned()`, so this is safe today. But it is fragile -- any future refactor that stores the raw pointer instead of copying would be a bug. Consider adding a comment on the Rust side noting this constraint.

---

## Suggestions (Nice to Have)

### 7. Missing Subdivision Test Coverage

No test exercises the `SubdivData` code path in `EmbreeScene::from_indexed`. All 4 Embree tests pass `None` for subd. Add at least one test that constructs a `SubdivData` with a simple quad and verifies the Embree scene builds without error. This would have caught issue #1.

### 8. OpenPBR Emission Luminance Scaling

**File:** `cpp/usd_bridge/usd_bridge.cpp`, line 3844

```cpp
.Set(emissive_lum * 1000.0f); // Scale to nits
```

The 1000x multiplier is arbitrary. OpenPBR `emission_luminance` is in nits (cd/m^2). A linear color value of 1.0 scaled by 1000 nits is a reasonable approximation for preview rendering, but it should be documented as an approximation. Consider making this configurable or at least noting the assumption in a comment.

### 9. Duplicate Cache Invalidation Code

`load_payload` (lines 4147-4157), `unload_payload` (lines 4166-4175), and `set_variant_selection` (lines 4286-4295) all have identical 10-flag cache invalidation blocks. Extract to a helper like `invalidate_all_caches(UsdBridgeStage*)`.

### 10. No Tests for New FFI Functions

The new skeleton, volume, curves, variant, and payload Rust wrappers have no direct unit tests in `cpp_bridge.rs`. The existing 5 tests are all pre-existing. Consider adding round-trip tests that exercise the new APIs (requires test USD files with skeleton/volume/variant data).

### 11. OpenPBR Texture Support Gap

The UsdPreviewSurface export correctly wires up texture readers for diffuse, roughness, metallic, normal, and emissive maps. However, the OpenPBR MaterialX network (lines 3822-3851) only sets scalar values -- no texture connections. If a material has textures, the OpenPBR representation loses them. Consider either wiring up MaterialX image nodes or documenting this as a known limitation.

---

## Summary

| Category | Count |
|----------|-------|
| Critical (must fix) | 1 |
| Important (should fix) | 5 |
| Suggestions | 5 |

The overall architecture is solid. The FFI layer follows a safe, consistent pattern with good null checks and error propagation. The one critical bug (subd index use-after-free) is currently unreachable but will bite hard when subdivision rendering is enabled. The specular read/write mismatch breaks round-trip fidelity. The remaining items are about robustness and code hygiene.

Recommended priority: Fix #1 (subd UAF) and #2 (specular mismatch) before any code touches the subdivision or material export paths.
