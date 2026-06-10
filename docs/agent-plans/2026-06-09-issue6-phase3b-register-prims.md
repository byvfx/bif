# Issue #6 Phase 3b — `SceneNode::register_prims()` extraction — Implementation Plan

> Final slice of the issue #6 node-deepening RFC. After this, the RFC is closed (3c `apply()` was assessed as not-fitting — see closeout note).

**Goal:** Move the one per-`SceneNode`-variant arm in `scene_browser::build_scene_graph_cache` (the `UsdPrim` authored-prim registration) onto `SceneNode::register_prims()` in `behavior.rs`, behind a `ProcPrimSink` — establishing the per-node prim-registration extension point.

**Architecture:** `ProcPrimSink<'a>` (in `scene_browser.rs`, owns the prim-cache representation) wraps `&mut HashMap<String, ProceduralPrim>` and exposes `add_authored(path, prim_type, source_node)`. `SceneNode::register_prims(&self, id, &mut ProcPrimSink)` (in `behavior.rs`) decides *which* nodes register *what* — currently only `UsdPrim`. The graph loop in `build_scene_graph_cache` becomes `node.register_prims(...)`. Scene-driven prototype/cloud registration is unchanged (it was never a per-variant match).

**Scope note (why this is small):** `build_scene_graph_cache` is scene-driven — prototype/cloud prims come from iterating `scene.prototypes()`/`scene.point_clouds()`, not from matching `SceneNode`s. The ONLY per-variant logic is the `UsdPrim` arm. So 3b extracts one arm + the sink extension point.

**Tech Stack:** Rust, `bif_viewport`. `register_prims` test is dep-free. `cargo test -p bif_viewport` needs USD env sourced for the binary to link.

**Workflow:** Branch `refactor/issue6-phase3b-register-prims` (off `main`, which now has #9 + #11). Implemented inline (small; ~40 LOC). → PR → auto `claude-review` → squash-merge → close issue #6.

## Current State (verified)

- `scene_browser.rs:709` `build_scene_graph_cache(scene, graph, node_outputs) -> CachedSceneGraph`. The per-variant arm (the only one) is the graph-node loop at ~787–805:
  ```rust
  for (node_id, node) in graph.node_ids() {
      match node {
          SceneNode::UsdPrim { prim_path, prim_type, .. } if !prim_path.is_empty() => {
              procedural_prims.insert(prim_path.clone(), ProceduralPrim {
                  path: prim_path.clone(),
                  kind: prim_type_to_procedural_kind(*prim_type),
                  source_node: Some(GraphNodeId::from(node_id)),
              });
          }
          _ => {}
      }
  }
  ```
- `ProceduralPrim` (scene_browser.rs:671), `ProceduralPrimKind` (:638), `CachedSceneGraph` (:684) — all `pub` in `scene_browser.rs`. `prim_type_to_procedural_kind` (:852, private fn) used ONLY at :798. `procedural_prims: HashMap<String, ProceduralPrim>` local in the fn.
- `scene_browser.rs` imports `GraphNodeId, NodeOutputs, SceneNode` from `crate::node_graph`.
- `behavior.rs` (from 3a) holds `EvalCtx`/`EvalOutcome`/`SceneNode::evaluate`. 3b adds `register_prims` here.
- Existing `scene_browser` `#[cfg(test)]` tests (`test_source_node_tagging`, `test_prim_count_by_node`, `test_build_scene_graph_cache_children_index`) call `build_scene_graph_cache` and assert UsdPrim/proto tagging — they STAY as the integration regression gate.

## File Structure
- **Modify:** `scene_browser.rs` — add `ProcPrimSink` + `new` + `add_authored`; replace the UsdPrim loop body with `node.register_prims(...)`.
- **Modify:** `node_graph/behavior.rs` — add `SceneNode::register_prims` + a pure unit test.

---

### Task 1 (inline, TDD): `ProcPrimSink` + `register_prims`

- [ ] **Step 1: Add `ProcPrimSink` to `scene_browser.rs`** (near `ProceduralPrim`, ~after line 682):

```rust
/// Sink for procedural prims contributed by graph nodes, via
/// [`SceneNode::register_prims`]. Wraps the cache's prim map so node behavior
/// stays decoupled from the cache representation (issue #6 Phase 3b).
pub(crate) struct ProcPrimSink<'a> {
    prims: &'a mut HashMap<String, ProceduralPrim>,
}

impl<'a> ProcPrimSink<'a> {
    /// Wrap a prim map for the duration of registration.
    pub(crate) fn new(prims: &'a mut HashMap<String, ProceduralPrim>) -> Self {
        Self { prims }
    }

    /// Register an authored USD prim (from a `UsdPrim` node). Keyed by path.
    pub(crate) fn add_authored(
        &mut self,
        path: String,
        prim_type: bif_core::usd::UsdPrimType,
        source_node: GraphNodeId,
    ) {
        let kind = prim_type_to_procedural_kind(prim_type);
        self.prims.insert(
            path.clone(),
            ProceduralPrim {
                path,
                kind,
                source_node: Some(source_node),
            },
        );
    }
}
```

- [ ] **Step 2: Replace the UsdPrim loop** in `build_scene_graph_cache` (the `for (node_id, node) in graph.node_ids() { match node { UsdPrim ... } } ` block) with:

```rust
    // Add authored graph-only prims. These do not own geometry yet, but they
    // must be visible for node-assembly dogfooding and export context.
    {
        let mut sink = ProcPrimSink::new(&mut procedural_prims);
        for (node_id, node) in graph.node_ids() {
            node.register_prims(GraphNodeId::from(node_id), &mut sink);
        }
    }
```

(The explicit `{ }` block drops `sink` before the subsequent `procedural_prims.keys()` Scope-synthesis step — though NLL would also release it, the block is clearer.)

- [ ] **Step 3: Add `register_prims` to `behavior.rs`** (after the `evaluate` impl). Add `use crate::scene_browser::ProcPrimSink;` to the imports:

```rust
impl SceneNode {
    /// Register this node's authored procedural prims into the scene-graph
    /// cache. Most nodes contribute nothing — their geometry is registered
    /// from `working_scene` by `build_scene_graph_cache`. Authored-prim nodes
    /// (`UsdPrim`) add an entry. This is the per-node extension point for the
    /// prim cache (issue #6 Phase 3b).
    pub(crate) fn register_prims(&self, id: GraphNodeId, sink: &mut ProcPrimSink) {
        match self {
            SceneNode::UsdPrim {
                prim_path,
                prim_type,
                ..
            } if !prim_path.is_empty() => {
                sink.add_authored(prim_path.clone(), *prim_type, id);
            }
            _ => {}
        }
    }
}
```

- [ ] **Step 4: Add a pure unit test** in `behavior.rs` `#[cfg(test)] mod tests`:

```rust
    #[test]
    fn usd_prim_registers_authored_prim() {
        use crate::scene_browser::{ProcPrimSink, ProceduralPrim};
        use std::collections::HashMap;

        let mut prims: HashMap<String, ProceduralPrim> = HashMap::new();
        let mut sink = ProcPrimSink::new(&mut prims);

        // UsdPrim with a path registers.
        let mut node = SceneNode::usd_prim();
        if let SceneNode::UsdPrim { prim_path, .. } = &mut node {
            *prim_path = "/World/Foo".to_string();
        }
        node.register_prims(id(7), &mut sink);

        // Empty-path UsdPrim registers nothing.
        let empty = SceneNode::usd_prim();
        empty.register_prims(id(8), &mut sink);

        // A non-authored node (Primitive) registers nothing.
        let prim = SceneNode::primitive(bif_core::PrimitiveKind::Cube);
        prim.register_prims(id(9), &mut sink);

        assert_eq!(prims.len(), 1);
        let entry = prims.get("/World/Foo").expect("authored prim registered");
        assert_eq!(entry.source_node, Some(id(7)));
    }
```

> Verify during implementation: `SceneNode::usd_prim()` exists and its default `prim_path` is empty (so the empty-path branch is exercised) — confirm against the constructor; if it defaults non-empty, set it empty for the `empty` case. `ProceduralPrim.source_node` field is accessible (`pub`) for the assertion. The `id(n)` helper already exists in `behavior.rs` tests (from 3a).

- [ ] **Step 5: Green gate**

```
cargo build -p bif_viewport
cargo clippy -p bif_viewport -- -D warnings
. .\setup_usd_env.ps1; cargo test -p bif_viewport
cargo fmt --check
```
Expected: build/clippy/fmt clean; ALL bif_viewport tests pass = existing scene_browser tests (UsdPrim tagging unchanged) + the new `usd_prim_registers_authored_prim` + the 6 evaluate tests + rest. Behavior-preserving.

- [ ] **Step 6: Grep guard + commit**

```
git grep -n "match node" crates/bif_viewport/src/scene_browser.rs   # the per-variant UsdPrim match is gone
git add crates/bif_viewport/src/scene_browser.rs crates/bif_viewport/src/node_graph/behavior.rs
git commit -m "refactor(nodes): extract UsdPrim registration to SceneNode::register_prims (issue #6 phase 3b)"
```

---

### Task 2: Docs + PR + close issue #6

- [ ] CHANGELOG `[Unreleased]` bullet (phase 3b). Devlog append. SESSION_HANDOFF update (RFC complete).
- [ ] PR → `/vfx-code-reviewer` → squash-merge.
- [ ] **Close issue #6** with a closeout note: phases 1/2/3a/3b landed; 3c (`apply()`) intentionally not done — after Phase 2 routed the bookkeeping via `SceneCmd`, the remaining `node_dispatch` arms are irreducible Renderer orchestration (loaders/IO/`reload`/GPU/flags) that a `Vec<SceneCmd>` return can't meaningfully capture.

## Verification
1. build + `cargo test -p bif_viewport` green (USD env sourced).
2. clippy clean.
3. `git grep "match node" scene_browser.rs` → gone; UsdPrim registration now in `behavior.rs::register_prims`.
4. Smoke: a `UsdPrim` node with a path still appears in the scene browser tagged to its node.

## Unresolved questions
- `SceneNode::usd_prim()` default `prim_path` (empty vs not) — adjust the test's empty-case if non-empty. Resolve at implementation.
- behavior.rs → scene_browser import (`ProcPrimSink`) creates a node_graph↔scene_browser mutual module dep — fine in-crate, but if it feels wrong, `ProcPrimSink` could instead live in `behavior.rs` (it would then need `ProceduralPrim`/`prim_type_to_procedural_kind` made `pub(crate)` + imported). Chosen: sink in scene_browser (owns prim-cache repr).
