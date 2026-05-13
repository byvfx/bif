# VFX Production Code Review — Visibility Toggle Fixes

**Commit:** `443b7e5` + `6966752`  
**Scope:** `scene_loader.rs`, `scene_browser.rs`, `scene_browser_model.cpp`, `scene_browser_widget.cpp/h`, `scene.rs`  
**Changes:** 6 files, +86/-15

---

## 1. Critical Issues

**None found.** No logic errors, race conditions, or safety bugs. All changes correctly scoped.

---

## 2. Important Improvements

### 2.1 Linear scan in `all_prims` amplified by recursive refresh

`get_prim_info_by_path` (line 3434, `usd_bridge.cpp`) scans `stage->all_prims` linearly:

```cpp
for (const auto& info : stage->all_prims) {
    if (info.path == path_str) { ... }
}
```

`refresh_child_visibility` calls `state->prim_is_visible_at()` for every expanded child node. Each call goes through `with_scene_browser_provider` → stage lock → `get_prim_info_by_path` → O(N) scan.

**Real-world impact:** For a 100K-prim scene with 50 expanded nodes = 5M string comparisons per visibility click. Each call also locks/unlocks the stage mutex 50+ times — Qt main thread blocks during this.

**Mitigation:** In practice, expanded node count is small (<100). The scan reads from in-memory `all_prims` vector — no USD C++ calls, so each lookup is fast. Acceptable for v0.16.5.

**Future:** If profiling shows this as a bottleneck, batch all visibility queries into a single `refresh_visibility_batch()` call within one stage lock scope, or use a `HashMap<String, usize>` index for O(1) path lookups in `all_prims`.

### 2.2 `get_prim_info` now always queries USD stage for procedural prims

Each procedural prim that shadows a USD prim now triggers `usd.get_prim_info(path)`. Previously only a debug check. This doubles the number of `all_prims` scans during tree rebuild.

**Impact:** Negligible — tree rebuild is infrequent (only on visibility clicks), and the second query is O(N) string comparison from memory.

### 2.3 `m_expanded_paths` QSet accumulates across resets

Set is cleared on `modelAboutToBeReset` and re-populated from the live tree. If a prim path was in the set but was removed from the stage between resets, `restore_expanded_state` silently ignores it (`contains` returns false). Graceful.

**Minor:** QSet uses heap allocation per path. For deeply nested scenes with 100s of expanded nodes, this is fine.

---

## 3. Suggestions

### 3.1 Remove redundant null check in `refresh_child_visibility`

```cpp
static void refresh_child_visibility(PrimNode* parent, BifShellState* state) {
    if (!parent || !state) return;  // state check is redundant — rebuild_from_state already validates m_state
```

The `!state` check is defensive — not wrong, just surplus. Keep if you prefer belt-and-suspenders.

### 3.2 Consider batching `prim_is_visible_at` queries

The recursive refresh calls `state->prim_is_visible_at()` for each node independently. If this becomes a bottleneck, batch into one function:

```rust
fn batch_prim_visibility(paths: Vec<QString>) -> Vec<bool>
```

This would lock the stage once, populate all results, and return. Cuts mutex lock/unlock from O(N) to O(1) per refresh.

### 3.3 `CachedSceneGraph` should distinguish USD vs procedural prototypes

The root cause of bug #2: `CachedSceneGraph` inserts ALL prototypes (including USD-loaded meshes) into `procedural_prims`. Long-term, add a `source: PrimSource::UsdRead | PrimSource::Procedural` flag so the cache can skip USD-origin prims entirely in `get_prim_info`. Deferred to v0.17.0 — not blocking.

---

## 4. Questions or Challenges

### 4.1 Does `refresh_usd_visibility_state` need to run for non-USD edits?

`reload_after_usd_edit` always calls `refresh_usd_visibility_state()` — even for material param or shader-id edits where visibility hasn't changed. This walks all instances and queries the USD stage each time. For frequent material tweaks, this is wasted work. Consider gating on the edit operation type:

```rust
if matches!(op, Some(EditOperation::Visibility { .. }) | None) {
    self.refresh_usd_visibility_state();
}
```

Deferred — not a correctness issue, just a minor perf note.

### 4.2 Should `add_instance_with_path` be the ONLY way to add instances?

With this fix, `add_instance()` (without path) is only used for procedural primitives (Primitive node, scatter, etc.) which legitimately have no USD prim path. The distinction is now load-bearing. Adding a comment in `scene.rs` documenting the contract would help prevent regressions:

```rust
/// Use ONLY for procedural (non-USD) instances.
/// USD instances MUST use add_instance_with_path().
pub fn add_instance(&mut self, ...)
```

---

## Summary

| Category | Count |
|----------|-------|
| Critical Issues | 0 |
| Important Improvements | 3 |
| Suggestions | 3 |
| Questions | 2 |

**Verdict:** ✅ Clean merge. Changes are correct, minimal, and focused. No blocking issues for production. The `all_prims` linear scan is a pre-existing concern — if it becomes a bottleneck, batch visibility queries into single-lock scope. Defer to v0.17.0.
