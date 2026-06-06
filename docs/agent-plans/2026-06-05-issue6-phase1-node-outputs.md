# Issue #6 Phase 1 — `NodeOutputs` Merge — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Merge the two parallel per-node bookkeeping maps (`node_proto_map`, `node_cloud_map`) into one keyed `NodeOutputs` struct, eliminating the desync-prone dual-HashMap threading — Phase 1 of the [issue #6](https://github.com/byvfx/bif/issues/6) node-deepening RFC.

**Architecture:** Introduce `NodeOutputs { proto_ids: Vec<usize>, cloud_id: Option<usize> }` and replace `NodeGraphContext.node_proto_map` + `.node_cloud_map` with a single `node_outputs: HashMap<GraphNodeId, NodeOutputs>`. Pure mechanical, behavior-preserving: every read/write site translates 1:1 via the table below. No `SceneCmd`, no `behavior.rs` yet (those are Phases 2–3).

**Tech Stack:** Rust, `bif_viewport` crate. Tests are pure (no GPU/USD) — `cargo test -p bif_viewport` runs dep-free.

**Workflow:** Branch `refactor/issue6-node-outputs` → commits per task → PR → auto `claude-review` + manual `/vfx-code-reviewer` → squash-merge (per CLAUDE.md `Workflow › Branching & Review`).

---

## Current State (verified)

- **Maps declared:** `crates/bif_viewport/src/lib.rs:169-170` in `struct NodeGraphContext` (accessed as `self.nodes.node_proto_map` / `.node_cloud_map`).
  - `node_proto_map: HashMap<GraphNodeId, Vec<usize>>` — node → prototype indices into `self.scene.working_scene.prototypes`.
  - `node_cloud_map: HashMap<GraphNodeId, usize>` — node → one cloud id into `…point_clouds`.
- **NOT serialized** — `NodeGraphContext` is runtime Renderer state; the persisted artifact is `Snarl<SceneNode>` (persistence.rs touches neither map). So `NodeOutputs` needs **no serde derive**.
- **~36 access sites** across 5 files: `node_dispatch.rs`, `lib.rs`, `render.rs`, `scene_browser.rs`, `scene_loader.rs`.
- **`build_scene_graph_cache`** (`scene_browser.rs:709`) takes `&node_proto_map, &node_cloud_map` and builds reverse maps `proto_idx→node` / `cloud_id→node` (lines 715-725). 3 existing tests call it: `test_source_node_tagging`, `test_prim_count_by_node`, `test_build_scene_graph_cache_children_index`.

## Translation Table (apply at every site)

| Old (`node_proto_map` / `node_cloud_map`) | New (`node_outputs: HashMap<GraphNodeId, NodeOutputs>`) |
|---|---|
| `node_proto_map.get(&id)` → `Option<&Vec<usize>>` | `node_outputs.get(&id).map(\|o\| &o.proto_ids)` |
| `node_proto_map.insert(id, vec)` | `node_outputs.entry(id).or_default().proto_ids = vec` |
| `node_proto_map.entry(id).or_default().push(p)` | `node_outputs.entry(id).or_default().proto_ids.push(p)` |
| `node_proto_map.remove(&id)` | `if let Some(o)=node_outputs.get_mut(&id){o.proto_ids.clear()}` then prune (see note) |
| `node_cloud_map.get(&id)` → `Option<&usize>` | `node_outputs.get(&id).and_then(\|o\| o.cloud_id)` (yields `Option<usize>`) |
| `node_cloud_map.insert(id, cid)` | `node_outputs.entry(id).or_default().cloud_id = Some(cid)` |
| `node_cloud_map.remove(&id)` | `if let Some(o)=node_outputs.get_mut(&id){o.cloud_id=None}` then prune |
| `for (n, protos) in &node_proto_map` | `for (n,o) in &node_outputs { /* use &o.proto_ids */ }` |
| `for (n, &cid) in &node_cloud_map` | `for (n,o) in &node_outputs { if let Some(cid)=o.cloud_id {…} }` |

**Prune note:** after clearing a field, if `o.is_empty()` remove the entry (`node_outputs.retain(...)` or explicit remove) to match the old behavior where a node with no protos/cloud had no map entry. Where the old code did `node_proto_map.remove(&id)` on node delete, the new code removes the whole `node_outputs` entry: `node_outputs.remove(&id)`.

## Site Checklist (from exploration — verify each during migration)

- [ ] `lib.rs:169-170` — field declaration (replace two fields with one)
- [ ] `node_dispatch.rs` ~128-178 `CreatePrimitive` (proto insert)
- [ ] `node_dispatch.rs` ~180-353 `ScatterPointsCompute` (cloud insert, proto reads)
- [ ] `node_dispatch.rs` ~13-45 `LoadUsdFile` (proto bookkeeping)
- [ ] `node_dispatch.rs` ~623-692 `DeleteNode` (remove entry for node)
- [ ] `node_dispatch.rs` other arms reading either map
- [ ] `scene_loader.rs` `remove_and_reindex_prototype`, `reload_working_scene` (proto reindex reads/writes)
- [ ] `render.rs` ~135-136 (passes both maps to `build_scene_graph_cache`)
- [ ] `scene_browser.rs:709-725` (signature + reverse-map build)
- [ ] any `lib.rs` reads (e.g. counts)

> The compiler is the source of truth: after Task 2 Step 1, `cargo build -p bif_viewport` enumerates every remaining site. Fix each with the table.

---

## File Structure

- **Create:** `crates/bif_viewport/src/node_graph/node_outputs.rs` — the `NodeOutputs` struct + its unit tests. One responsibility: per-node output-id bookkeeping.
- **Modify:** `crates/bif_viewport/src/node_graph/mod.rs` — add `mod node_outputs; pub use node_outputs::NodeOutputs;`.
- **Modify:** `crates/bif_viewport/src/lib.rs` — swap the two fields for `node_outputs`.
- **Modify:** `crates/bif_viewport/src/{node_dispatch,scene_loader,render,scene_browser}.rs` — migrate access sites; change `build_scene_graph_cache` signature.
- **Modify (tests):** `scene_browser.rs` `#[cfg(test)]` — update the 3 callers to the new signature.

---

### Task 1: Add the `NodeOutputs` type

**Files:**
- Create: `crates/bif_viewport/src/node_graph/node_outputs.rs`
- Modify: `crates/bif_viewport/src/node_graph/mod.rs` (add module + re-export)
- Test: same file (`#[cfg(test)]` in `node_outputs.rs`)

- [ ] **Step 1: Write the failing test**

In a new file `crates/bif_viewport/src/node_graph/node_outputs.rs`:

```rust
//! Per-node output bookkeeping: the prototype indices and optional point-cloud
//! id a node owns in `working_scene`. Replaces the old parallel
//! `node_proto_map` + `node_cloud_map` (issue #6 Phase 1).

/// Outputs a single graph node has materialized into the working scene.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NodeOutputs {
    /// Prototype indices into `working_scene.prototypes` owned by this node.
    pub proto_ids: Vec<usize>,
    /// Point-cloud id into `working_scene.point_clouds`, if this node made one.
    pub cloud_id: Option<usize>,
}

impl NodeOutputs {
    /// True when the node owns no protos and no cloud (entry is prunable).
    pub fn is_empty(&self) -> bool {
        self.proto_ids.is_empty() && self.cloud_id.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_empty() {
        let o = NodeOutputs::default();
        assert!(o.is_empty());
        assert!(o.proto_ids.is_empty());
        assert_eq!(o.cloud_id, None);
    }

    #[test]
    fn populated_is_not_empty() {
        let mut o = NodeOutputs::default();
        o.proto_ids.push(3);
        assert!(!o.is_empty());
        o.proto_ids.clear();
        o.cloud_id = Some(0);
        assert!(!o.is_empty());
    }
}
```

- [ ] **Step 2: Wire the module**

In `crates/bif_viewport/src/node_graph/mod.rs`, add near the other `mod`/`pub use` lines:

```rust
mod node_outputs;
pub use node_outputs::NodeOutputs;
```

- [ ] **Step 3: Run the tests (verify they pass — pure struct, no red phase needed beyond compile)**

Run: `cargo test -p bif_viewport node_outputs`
Expected: 2 passed (`default_is_empty`, `populated_is_not_empty`).

- [ ] **Step 4: Commit**

```bash
git add crates/bif_viewport/src/node_graph/node_outputs.rs crates/bif_viewport/src/node_graph/mod.rs
git commit -m "feat(nodes): add NodeOutputs type (issue #6 phase 1)"
```

---

### Task 2: Migrate the maps to `node_outputs` (single compiling unit)

This is one atomic, compiler-driven mechanical change — the codebase will not compile until every site is migrated. Use the Translation Table + Site Checklist.

**Files:** `lib.rs`, `node_dispatch.rs`, `scene_loader.rs`, `render.rs`, `scene_browser.rs` (+ its tests).

- [ ] **Step 1: Swap the field declaration**

In `crates/bif_viewport/src/lib.rs:169-170`, replace:

```rust
    pub node_proto_map: std::collections::HashMap<node_graph::GraphNodeId, Vec<usize>>,
    pub node_cloud_map: std::collections::HashMap<node_graph::GraphNodeId, usize>,
```

with:

```rust
    pub node_outputs: std::collections::HashMap<node_graph::GraphNodeId, node_graph::NodeOutputs>,
```

Also update wherever `NodeGraphContext` is constructed (search `node_proto_map:` initializer) — replace the two `HashMap::new()` inits with one `node_outputs: HashMap::new(),`.

- [ ] **Step 2: Change `build_scene_graph_cache` signature + reverse-map build**

In `crates/bif_viewport/src/scene_browser.rs:709-725`, replace the two map params:

```rust
pub fn build_scene_graph_cache(
    scene: &dyn bif_core::SceneQuery,
    graph: &egui_snarl::Snarl<SceneNode>,
    node_outputs: &HashMap<GraphNodeId, NodeOutputs>,
) -> CachedSceneGraph {
    // Build reverse maps: proto_index -> node, cloud_id -> node
    let mut proto_to_node: HashMap<usize, GraphNodeId> = HashMap::new();
    let mut cloud_to_node: HashMap<usize, GraphNodeId> = HashMap::new();
    for (&node_id, outputs) in node_outputs {
        for &pid in &outputs.proto_ids {
            proto_to_node.insert(pid, node_id);
        }
        if let Some(cid) = outputs.cloud_id {
            cloud_to_node.insert(cid, node_id);
        }
    }
```

(The rest of the function body — `proto_to_node.get(...)`, `cloud_to_node.get(...)` — is unchanged.) Add `use crate::node_graph::NodeOutputs;` to the file's imports if not already in scope.

- [ ] **Step 3: Compiler-driven site migration**

Run `cargo build -p bif_viewport 2>&1`. For each error, apply the Translation Table. Walk the Site Checklist. Common shapes:
- `self.nodes.node_proto_map.entry(id).or_default().push(p)` → `self.nodes.node_outputs.entry(id).or_default().proto_ids.push(p)`
- `self.nodes.node_cloud_map.insert(id, cid)` → `self.nodes.node_outputs.entry(id).or_default().cloud_id = Some(cid)`
- `self.nodes.node_cloud_map.get(&id).copied()` → `self.nodes.node_outputs.get(&id).and_then(|o| o.cloud_id)`
- node-delete `node_proto_map.remove(&id); node_cloud_map.remove(&id);` → `self.nodes.node_outputs.remove(&id);`
- call sites passing both maps (`render.rs:135-136`) → pass `&self.nodes.node_outputs`.

After each field-clear that the old code followed with a `remove`, prune: `if self.nodes.node_outputs.get(&id).is_some_and(|o| o.is_empty()) { self.nodes.node_outputs.remove(&id); }`.

- [ ] **Step 4: Update the 3 `build_scene_graph_cache` tests**

In `scene_browser.rs` `#[cfg(test)]`, the existing callers pass `&empty_proto, &empty_cloud` (two maps). Update each to a single `NodeOutputs` map. Example for `test_source_node_tagging` (preserve its asserted behavior — a proto tagged to its node):

```rust
use crate::node_graph::NodeOutputs;
let mut node_outputs: HashMap<GraphNodeId, NodeOutputs> = HashMap::new();
node_outputs.insert(node_id, NodeOutputs { proto_ids: vec![0], cloud_id: None });
let cache = build_scene_graph_cache(&scene, &graph, &node_outputs);
// (assertions unchanged — same source_node tagging expected)
```

For the empty-case tests: `let node_outputs: HashMap<GraphNodeId, NodeOutputs> = HashMap::new();` and pass `&node_outputs`.

- [ ] **Step 5: Green gate**

Run, in order:
```bash
cargo build -p bif_viewport
cargo clippy -p bif_viewport -- -D warnings
cargo test -p bif_viewport
cargo fmt --check
```
Expected: build clean, no clippy warnings, all bif_viewport tests pass (the pre-existing 177 + the 2 new NodeOutputs tests), fmt clean. No behavior change — same tests, same assertions.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "refactor(nodes): merge node_proto_map+node_cloud_map into node_outputs (issue #6 phase 1)"
```

---

## Verification (end-to-end)

1. `cargo build -p bif_viewport && cargo test -p bif_viewport` — green, no regressions.
2. `cargo clippy -p bif_viewport -- -D warnings` — clean.
3. Manual smoke (optional, needs full env): `. .\setup_usd_env.ps1; . .\setup_qt_env.ps1; cargo run -p bif_viewer` — create a Primitive node + a Scatter node, confirm prims still appear in the scene browser tagged to their source node, and deleting a node clears its prims (exercises the merged map on insert/read/delete).
4. Grep guard: `git grep node_proto_map; git grep node_cloud_map` → only historical/devlog hits, no live code.

## Self-Review

- **Spec coverage:** RFC migration step (1) "introduce NodeOutputs, fold both maps into it" — covered by Tasks 1–2. Steps (2)–(5) are out of scope for Phase 1 (separate plans). ✓
- **Placeholders:** none — translation table + exact new type/signature provided; the per-site edits are a uniform mechanical rule the compiler enumerates (appropriate for a map-merge, not hand-wavy). ✓
- **Type consistency:** `NodeOutputs { proto_ids: Vec<usize>, cloud_id: Option<usize> }`, `is_empty()`, field `node_outputs` — names consistent across all tasks. ✓

## Unresolved questions

- Prune-on-empty vs keep-empty-entry: plan prunes to match old "no entry when empty" semantics. If any read site relied on `contains_key` meaning "node exists" (vs "node has outputs"), confirm during Step 3 — none seen in exploration, but verify the `DeleteNode` and scatter-recompute paths.
- `next_cloud_id` / `node_scatter_surface_map` / `node_prim_counts` stay as-is (not part of the proto/cloud merge) — confirm none are redundant with `node_outputs` (they aren't, per exploration).
