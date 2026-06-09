# Issue #6 Phase 2 — `SceneCmd` + `Renderer::execute()` — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Introduce a `SceneCmd` command surface and a single `Renderer::execute()` mutation site, then route the desync-prone per-node bookkeeping in `node_dispatch.rs`'s 21 dispatch arms through it — Phase 2 of the [issue #6](https://github.com/byvfx/bif/issues/6) node-deepening RFC.

**Architecture:** Add `SceneCmd` (a 6-verb enum) naming the brittle `node_outputs ↔ working_scene` proto/cloud coupling + the point-preview upload that the RFC targeted. `Renderer::execute(&mut self, cmd: SceneCmd)` (infallible, returns `()`) is the ONE place that applies those mutations. Dispatch arms keep their computation and their coarse already-factored calls (`reload_working_scene`, `load_primitive`, `load_usd_scene`, `export_scene`, `compact_materials`) inline, but emit `SceneCmd`s for the bookkeeping. Pure behavior-preserving refactor — no `behavior.rs`, no `apply()` yet (Phase 3).

**Tech Stack:** Rust, `bif_viewport` crate. The new enum + `execute()` are CPU-only except `UploadPointPreview` (GPU). `cargo test -p bif_viewport` runs dep-free but needs the USD env sourced for the binary to link (see Gotchas in CLAUDE.md).

**Workflow:** Branch `refactor/issue6-phase2-scenecmd` → commits per task → PR → auto `claude-review` + manual `/vfx-code-reviewer` → squash-merge (per CLAUDE.md `Workflow › Branching & Review`).

---

## Design Decisions (settled during brainstorming)

1. **Scope:** all 21 arms reviewed; route the bookkeeping arms through `execute()` in one PR.
2. **No `Custom` escape hatch.** Reading the 4 "complex" arms showed the tangled parts are *computation/loaders*, not mutations: `load_usd_scene`/`load_primitive`/`export_scene`/scatter point-gen stay inline. Every routed mutation decomposes into a named verb.
3. **Coupling-scoped command surface (not full routing).** `SceneCmd` wraps only the desync-prone `node_outputs ↔ working_scene` proto/cloud coupling + preview upload. `reload_working_scene`, `load_primitive`, `load_usd_scene`, `export_scene`, `compact_materials`, dirty-flag writes, and node-UI-flag writes stay as direct calls — they are coarse and already well-factored, and wrapping them would force `Result` plumbing through arms that return `()` for zero safety gain.
4. **`execute()` is per-command + infallible (`-> ()`).** Arms that need a returned value (e.g. `load_primitive` → `proto_id`) call those methods directly, then emit `SceneCmd`s with the computed data. No arm needs a value *back* from `execute()`.
5. **Testing — honest scope.** `Renderer` is a GPU-backed ~75-field object; `execute()` cannot be cheaply unit-tested in Phase 2. Gate = regression (existing `bif_viewport` tests green + zero behavior change) + a `SceneCmd` `Debug`/construction sanity test. A real test-`Renderer` harness is **penciled in** as documented future work (see *Testing & Future Work*), not built now.

---

## Current State (verified)

- **Dispatch:** `crates/bif_viewport/src/node_dispatch.rs:11` `pub(crate) fn handle_node_graph_event(&mut self, event: NodeGraphEvent)` — one `match` with **21 arms**, 1015-line file, **0 tests**.
- **Coupling fields** (`crates/bif_viewport/src/lib.rs`, inside the `nodes` sub-struct unless noted):
  - `node_outputs: HashMap<GraphNodeId, NodeOutputs>` — Phase 1's merged map (`NodeOutputs { proto_ids: Vec<usize>, cloud_id: Option<usize> }`, with `is_empty()`), `crate::node_graph::NodeOutputs` (re-exported `node_graph/mod.rs:23`).
  - `next_cloud_id: usize` (lib.rs:170), `node_scatter_surface_map: HashMap<GraphNodeId, usize>` (lib.rs:171), `scene_graph_dirty: bool` (lib.rs:175).
  - `self.scene.working_scene` (`SceneManager`, lib.rs:362) — `bif_core` `Scene`/working scene.
  - `self.point_preview: PointPreviewRenderer` (lib.rs:410), `self.point_preview_params_dirty: bool` (lib.rs:414), `self.gpu.device`/`self.gpu.queue`.
- **Methods reused by `execute()` (verified signatures):**
  - `remove_and_reindex_prototype(&mut self, proto_id: usize) -> bool` (scene_loader.rs:399)
  - `working_scene.add_point_cloud(cloud: PointCloud) -> usize` (bif_core scene.rs:769)
  - `working_scene.remove_point_cloud(cloud_id: usize) -> bool` (scene.rs:788)
  - `point_preview.upload_points(&self, device: &wgpu::Device, queue: &wgpu::Queue, positions: &[Vec3])` (point_preview.rs:203)
  - `bif_core::PointCloud` (re-exported bif_core lib.rs:40); fields `id`, `positions`, `name`, `prototype_ids`.
- **Stays inline (coarse / returns values):** `reload_working_scene() -> Result<()>` (scene_loader.rs:533), `load_primitive(kind, size) -> Result<usize>` (scene_loader.rs:436), `load_usd_scene(path) -> Result<()>` (scene_loader.rs:1743), `export_scene(...)` (bif_core), `working_scene.compact_materials()` (scene.rs:892), `working_scene.prototype_count()` (scene.rs:816).

## The `SceneCmd` Surface (6 verbs)

```rust
/// A single scene-bookkeeping mutation, applied by [`Renderer::execute`].
#[derive(Debug)]
pub enum SceneCmd {
    /// Drop all prototypes a node owns: take its `proto_ids`, remove + reindex
    /// each from `working_scene` (reverse order keeps indices valid). Does NOT
    /// `compact_materials` — callers that need GC do it explicitly.
    RemoveNodeProtos { node: GraphNodeId },
    /// Record the prototype indices a node now owns (overwrites prior set).
    RecordProtos { node: GraphNodeId, proto_ids: Vec<usize> },
    /// Drop the point cloud a node owns: take its `cloud_id`, remove it from
    /// `working_scene`, and clear the node's scatter-surface mapping.
    RemoveNodeCloud { node: GraphNodeId },
    /// Assign a fresh cloud id, record it on the node, add the cloud to
    /// `working_scene`. The passed `cloud`'s `id` is overwritten.
    AddCloud { node: GraphNodeId, cloud: bif_core::PointCloud },
    /// Re-derive all point positions from `working_scene.point_clouds` and
    /// upload them to the point-preview renderer (sets params-dirty).
    UploadPointPreview,
    /// Mark the scene graph dirty (cache rebuild on next frame).
    MarkSceneGraphDirty,
}
```

## Per-Arm Classification (all 21)

| # | Arm | Verdict | `SceneCmd`s emitted (rest stays inline) |
|---|-----|---------|------------------------------------------|
| 1 | `LoadUsdFile` | route | `RemoveNodeProtos` (old); `RecordProtos` (new range, only if non-empty). `compact_materials`/`load_usd_scene`/`materials_dirty`/`mark_node_loaded` direct |
| 2 | `StartRender` | out of scope | — (ivar state) |
| 3 | `ConvertTexturesToTx` | out of scope | — (tx/env) |
| 4 | `ClearTxCache` | out of scope | — (tx cache) |
| 5 | `LoadHdri` | out of scope | — (env) |
| 6 | `UpdateHdriParams` | out of scope | — (env + ivar) |
| 7 | `CreatePrimitive` | route | `RemoveNodeProtos` (old); `RecordProtos { vec![proto_id] }`. `load_primitive`/name-set/`propagate_dirty`/`reload` direct |
| 8 | `ScatterPointsCompute` | route | `RemoveNodeCloud` (old); `AddCloud`; `UploadPointPreview`. point-gen/preview-appearance/`reload`/`propagate_dirty` direct |
| 9 | `PointInstancerCompute` | route | `MarkSceneGraphDirty` (inside the existing condition). cloud iter_mut/`instancer_results`/`reload`/node-flags direct |
| 10 | `InstancerInvalidate` | direct only | — (`instancer_results.remove` + `reload`) |
| 11 | `PointPreviewUpdate` | out of scope | — (preview appearance) |
| 12 | `ExportUsd` | direct only | — (reads + `export_scene` + node flags; **no working-scene mutation**) |
| 13 | `XformChanged` | direct only | — (node flag + `reload`) |
| 14 | `UsdPrimCreate` | route | `MarkSceneGraphDirty`. node flag direct |
| 15 | `GraftBranchesCompute` | route | `MarkSceneGraphDirty`. node flag direct |
| 16 | `SetDisplayNode` | direct only | — (toggle + `reload`) |
| 17 | `SelectNode` | none | — (no-op) |
| 18 | `DeleteNode` | route | `RemoveNodeCloud`; conditional `UploadPointPreview`; `RemoveNodeProtos`. `instancer_results`/`compact_materials`/`materials_dirty`/`reload`/usd-stage-reset direct |
| 19 | `CacheToggleBypass` | none | — (log only) |
| 20 | `CacheClear` | none | — (log only) |
| 21 | `CookNode` | none | — (re-dispatches via `handle_node_graph_event`) |

**Routed arms:** 1, 7, 8, 9, 14, 15, 18 (7 arms). The other 14 are out-of-scope, direct-only, or no-op — left untouched.

---

## File Structure

- **Create:** `crates/bif_viewport/src/scene_cmd.rs` — the `SceneCmd` enum **and** `impl Renderer { fn execute }`. One responsibility: the scene-bookkeeping mutation surface. Co-located because `execute` is the enum's only consumer.
- **Modify:** `crates/bif_viewport/src/lib.rs` — add `mod scene_cmd; pub use scene_cmd::SceneCmd;` near the other `mod`/`pub use` lines.
- **Modify:** `crates/bif_viewport/src/node_dispatch.rs` — route arms 1, 7, 8, 9, 14, 15, 18 through `self.execute(...)`.

`SceneCmd` lives in `bif_viewport`, not `node_graph/`, because it names *`Renderer`/`working_scene`* mutations, not graph concepts. (Phase 3's `apply()` — which *constructs* `SceneCmd`s from a `SceneNode` — will live in `node_graph/behavior.rs` and import `SceneCmd` from here.)

---

### Task 1: Add the `SceneCmd` enum + `Renderer::execute()`

**Files:**
- Create: `crates/bif_viewport/src/scene_cmd.rs`
- Modify: `crates/bif_viewport/src/lib.rs` (add `mod` + re-export)
- Test: same file (`#[cfg(test)]` in `scene_cmd.rs`)

- [ ] **Step 1: Create the file with the enum, `execute()`, and a sanity test**

Create `crates/bif_viewport/src/scene_cmd.rs`:

```rust
//! Scene-mutation command surface (issue #6 Phase 2).
//!
//! [`SceneCmd`] names the desync-prone per-node bookkeeping mutations that were
//! previously inlined across `node_dispatch.rs`: the coupling between
//! `node_outputs` (which protos / cloud a node owns) and `working_scene` (the
//! actual prototype / point-cloud storage), plus the point-preview GPU upload
//! derived from the clouds.
//!
//! Coarse, already-factored operations (`reload_working_scene`,
//! `load_primitive`, `load_usd_scene`, `export_scene`, `compact_materials`)
//! stay as direct method calls — they are not the brittle surface. Phase 3 will
//! move the per-node command *construction* into `node_graph/behavior.rs`; this
//! phase only establishes [`Renderer::execute`] as the single mutation site.

use crate::node_graph::GraphNodeId;
use crate::Renderer;

/// A single scene-bookkeeping mutation, applied by [`Renderer::execute`].
#[derive(Debug)]
pub enum SceneCmd {
    /// Drop all prototypes a node owns: take its `proto_ids`, remove + reindex
    /// each from `working_scene` (reverse order keeps indices valid). Does NOT
    /// `compact_materials` — callers that need GC do it explicitly.
    RemoveNodeProtos { node: GraphNodeId },
    /// Record the prototype indices a node now owns (overwrites prior set).
    RecordProtos {
        node: GraphNodeId,
        proto_ids: Vec<usize>,
    },
    /// Drop the point cloud a node owns: take its `cloud_id`, remove it from
    /// `working_scene`, and clear the node's scatter-surface mapping.
    RemoveNodeCloud { node: GraphNodeId },
    /// Assign a fresh cloud id, record it on the node, add the cloud to
    /// `working_scene`. The passed `cloud`'s `id` is overwritten.
    AddCloud {
        node: GraphNodeId,
        cloud: bif_core::PointCloud,
    },
    /// Re-derive all point positions from `working_scene.point_clouds` and
    /// upload them to the point-preview renderer (sets params-dirty).
    UploadPointPreview,
    /// Mark the scene graph dirty (cache rebuild on next frame).
    MarkSceneGraphDirty,
}

impl Renderer {
    /// Apply one [`SceneCmd`]. The single site that mutates the
    /// `node_outputs ↔ working_scene` proto/cloud coupling. Infallible: any
    /// fallible follow-up (e.g. `reload_working_scene`) stays at the call site.
    pub(crate) fn execute(&mut self, cmd: SceneCmd) {
        match cmd {
            SceneCmd::RemoveNodeProtos { node } => {
                let proto_ids = self
                    .nodes
                    .node_outputs
                    .get_mut(&node)
                    .map(|o| std::mem::take(&mut o.proto_ids));
                if self
                    .nodes
                    .node_outputs
                    .get(&node)
                    .is_some_and(|o| o.is_empty())
                {
                    self.nodes.node_outputs.remove(&node);
                }
                if let Some(proto_ids) = proto_ids {
                    // Reverse order so indices stay valid; reindex handles maps.
                    for &pid in proto_ids.iter().rev() {
                        self.remove_and_reindex_prototype(pid);
                    }
                }
            }
            SceneCmd::RecordProtos { node, proto_ids } => {
                self.nodes.node_outputs.entry(node).or_default().proto_ids = proto_ids;
            }
            SceneCmd::RemoveNodeCloud { node } => {
                let cloud_id = self
                    .nodes
                    .node_outputs
                    .get_mut(&node)
                    .and_then(|o| o.cloud_id.take());
                if self
                    .nodes
                    .node_outputs
                    .get(&node)
                    .is_some_and(|o| o.is_empty())
                {
                    self.nodes.node_outputs.remove(&node);
                }
                if let Some(cloud_id) = cloud_id {
                    self.scene.working_scene.remove_point_cloud(cloud_id);
                }
                // Surface mapping is rebuilt in reload_working_scene.
                self.nodes.node_scatter_surface_map.remove(&node);
            }
            SceneCmd::AddCloud { node, mut cloud } => {
                let cloud_id = self.nodes.next_cloud_id;
                self.nodes.next_cloud_id += 1;
                cloud.id = cloud_id;
                self.nodes.node_outputs.entry(node).or_default().cloud_id = Some(cloud_id);
                self.scene.working_scene.add_point_cloud(cloud);
            }
            SceneCmd::UploadPointPreview => {
                let all_positions: Vec<bif_math::Vec3> = self
                    .scene
                    .working_scene
                    .point_clouds
                    .iter()
                    .flat_map(|c| c.positions.iter().copied())
                    .collect();
                self.point_preview
                    .upload_points(&self.gpu.device, &self.gpu.queue, &all_positions);
                self.point_preview_params_dirty = true;
            }
            SceneCmd::MarkSceneGraphDirty => {
                self.nodes.scene_graph_dirty = true;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node_graph::GraphNodeId;

    // execute() needs a GPU-backed Renderer, so it is exercised via the
    // existing integration tests + manual smoke (see plan: Testing & Future
    // Work). These cheap tests cover the enum's own surface.

    #[test]
    fn scene_cmd_is_debug() {
        let cmd = SceneCmd::RecordProtos {
            node: GraphNodeId::from(egui_snarl::NodeId(0)),
            proto_ids: vec![1, 2, 3],
        };
        let s = format!("{cmd:?}");
        assert!(s.contains("RecordProtos"));
        assert!(s.contains('3'));
    }

    #[test]
    fn mark_scene_graph_dirty_is_unit_variant() {
        // Compile-time guard that the no-field variant exists and is cheap.
        let cmd = SceneCmd::MarkSceneGraphDirty;
        assert_eq!(format!("{cmd:?}"), "MarkSceneGraphDirty");
    }
}
```

> Verify during implementation: `GraphNodeId::from(egui_snarl::NodeId(0))` — confirm the `From<egui_snarl::NodeId>` impl + the `NodeId` tuple constructor against `node_graph/node_id.rs`. If the constructor differs (e.g. `NodeId::from(0usize)`), adjust the test's node literal only.

- [ ] **Step 2: Wire the module**

In `crates/bif_viewport/src/lib.rs`, add near the other `mod` / `pub use` lines (e.g. by the `node_dispatch`/`scene_loader` mod declarations):

```rust
mod scene_cmd;
pub use scene_cmd::SceneCmd;
```

- [ ] **Step 3: Build + run the new tests**

```bash
cargo build -p bif_viewport
cargo test -p bif_viewport scene_cmd
```
Expected: build clean; 2 passed (`scene_cmd_is_debug`, `mark_scene_graph_dirty_is_unit_variant`). `execute()` is `pub(crate)` and currently uncalled — that is fine (it is `impl Renderer`, so no dead-code warning for an inherent method used next task; if clippy flags it, the Task 2 call sites land in the same PR).

- [ ] **Step 4: Commit**

```bash
git add crates/bif_viewport/src/scene_cmd.rs crates/bif_viewport/src/lib.rs
git commit -m "feat(nodes): add SceneCmd + Renderer::execute (issue #6 phase 2)"
```

---

### Task 2: Route the 7 bookkeeping arms through `execute()`

One behavior-preserving pass over `node_dispatch.rs`. Each arm below shows the **exact** edit. Build green after each arm (or batch, then build once). The non-`SceneCmd` lines (loaders, `reload`, flags, node-UI) stay exactly as they are.

> **Task 1 deviations to account for:** (a) `SceneCmd::AddCloud.cloud` is `Box<bif_core::PointCloud>`, so Step 3 boxes the cloud (`cloud: Box::new(cloud)`). (b) `scene_cmd.rs` `execute()` carries a temporary `#[allow(dead_code)]` (+ its `// Task 2 wires callers …` comment) — **remove it** in Step 7 once callers exist, then confirm clippy is clean.

**Files:** `crates/bif_viewport/src/node_dispatch.rs`, `crates/bif_viewport/src/scene_cmd.rs` (remove the `#[allow(dead_code)]`)

- [ ] **Step 1: `LoadUsdFile` (arm ~13)**

Replace the old-proto cleanup block (lib lines ~17-36: the `get_mut`/`std::mem::take`/prune/`for pid ... remove_and_reindex_prototype`/`compact_materials`) with:

```rust
                // Remove old prototypes from this node (reload case)
                let had_protos = self
                    .nodes
                    .node_outputs
                    .get(&node_id)
                    .is_some_and(|o| !o.proto_ids.is_empty());
                self.execute(crate::SceneCmd::RemoveNodeProtos { node: node_id });
                if had_protos {
                    // GC orphaned materials so new load starts with clean offsets
                    self.scene.working_scene.compact_materials();
                }
```

Then replace the new-range record (the `if !proto_ids.is_empty() { ... node_outputs.entry(node_id).or_default().proto_ids = proto_ids; }` block in the `Ok(())` branch) with:

```rust
                        if !proto_ids.is_empty() {
                            log::info!("UsdRead {:?} owns protos {:?}", node_id, proto_ids);
                            self.execute(crate::SceneCmd::RecordProtos {
                                node: node_id,
                                proto_ids,
                            });
                        }
```

(`materials_dirty = true`, `proto_offset`/`load_usd_scene`/`mark_node_loaded`/`mark_node_error` stay unchanged.)

- [ ] **Step 2: `CreatePrimitive` (arm ~145)**

Replace the old-proto removal block (the `get_mut`/`take`/prune/`for pid ... remove_and_reindex_prototype`) with:

```rust
                // Remove old prototype if re-creating (e.g. size change)
                self.execute(crate::SceneCmd::RemoveNodeProtos { node: node_id });
```

Replace the proto-record line (`self.nodes.node_outputs.entry(node_id).or_default().proto_ids = vec![proto_id];`) with:

```rust
                        self.execute(crate::SceneCmd::RecordProtos {
                            node: node_id,
                            proto_ids: vec![proto_id],
                        });
```

(`load_primitive`, the `prim_path` name-set via `prototypes.get_mut`, `propagate_dirty`, `reload_working_scene` stay unchanged.)

- [ ] **Step 3: `ScatterPointsCompute` (arm ~214)**

Replace the old-cloud removal block (the `get_mut`/`cloud_id.take()`/prune/`remove_point_cloud(old_cloud_id)` AND the `node_scatter_surface_map.remove(&node_id)` on the next line) with:

```rust
                // Remove previous cloud for this node (if regenerating).
                // Also clears the scatter-surface mapping (rebuilt in reload).
                self.execute(crate::SceneCmd::RemoveNodeCloud { node: node_id });
```

In the `if let Some(mut cloud) = cloud { ... }` block, replace the id-assign + record + `add_point_cloud` + preview-derive-and-upload lines:

```rust
                    let cloud_id = self.nodes.next_cloud_id;
                    self.nodes.next_cloud_id += 1;
                    cloud.id = cloud_id;
                    self.nodes.node_outputs.entry(node_id).or_default().cloud_id = Some(cloud_id);

                    let pt_count = cloud.positions.len();
                    self.scene.working_scene.add_point_cloud(cloud);

                    // Upload point positions for preview
                    let all_positions: Vec<bif_math::Vec3> = self
                        .scene
                        .working_scene
                        .point_clouds
                        .iter()
                        .flat_map(|c| c.positions.iter().copied())
                        .collect();
                    self.point_preview.upload_points(
                        &self.gpu.device,
                        &self.gpu.queue,
                        &all_positions,
                    );
                    self.point_preview_params_dirty = true;
```

with:

```rust
                    let pt_count = cloud.positions.len();
                    self.execute(crate::SceneCmd::AddCloud {
                        node: node_id,
                        cloud: Box::new(cloud),
                    });
                    self.execute(crate::SceneCmd::UploadPointPreview);
```

(The `point_preview.visible = true` line and the `point_size`/`color` sync from the `ScatterPoints` node, plus `reload_working_scene` and `propagate_dirty`, stay unchanged. Note `let cloud = ...` no longer needs `mut` in the outer binding once the id-assign moves into `AddCloud` — change `if let Some(mut cloud)` to `if let Some(cloud)`.)

- [ ] **Step 4: `PointInstancerCompute` (arm ~402)**

Inside the `if let Some(ref prim_path) = instancer_prim_path { if let Some(cloud) = ...iter_mut()... { ... } }` block, replace the `self.nodes.scene_graph_dirty = true;` line with:

```rust
                                self.execute(crate::SceneCmd::MarkSceneGraphDirty);
```

(Everything else — `cloud.name`/`prototype_ids` via `iter_mut`, `expand_with_prototype`, `instancer_results.insert`, `reload_working_scene`, node-UI flags — stays unchanged.)

- [ ] **Step 5: `UsdPrimCreate` (arm ~643) and `GraftBranchesCompute` (arm ~652)**

In each, replace the trailing `self.nodes.scene_graph_dirty = true;` with:

```rust
                self.execute(crate::SceneCmd::MarkSceneGraphDirty);
```

(The `is_created` / `is_computed` node-flag sets stay unchanged.)

- [ ] **Step 6: `DeleteNode` (arm ~675)**

Replace the body from `let removed_outputs = self.nodes.node_outputs.remove(&node_id);` through the proto-cleanup block (up to and including the `materials_dirty = true;`) — i.e. lib lines ~677-720 — with:

```rust
                // Snapshot what this node owns before tearing it down, so we
                // preserve the "only re-upload preview if a cloud existed" and
                // "only compact if protos existed" behavior.
                let had_cloud = self
                    .nodes
                    .node_outputs
                    .get(&node_id)
                    .and_then(|o| o.cloud_id)
                    .is_some();
                let had_protos = self
                    .nodes
                    .node_outputs
                    .get(&node_id)
                    .is_some_and(|o| !o.proto_ids.is_empty());

                // Clean up scatter cloud (+ surface mapping) and refresh preview.
                self.execute(crate::SceneCmd::RemoveNodeCloud { node: node_id });
                if had_cloud {
                    self.execute(crate::SceneCmd::UploadPointPreview);
                    log::info!("Deleted scatter node {:?} → cloud removed", node_id);
                }

                // Clean up instancer results
                if self.nodes.instancer_results.remove(&node_id).is_some() {
                    log::info!("Deleted instancer node {:?}", node_id);
                }

                // Clean up prototypes owned by this node
                self.execute(crate::SceneCmd::RemoveNodeProtos { node: node_id });
                if had_protos {
                    // GC orphaned materials left behind by removed prototypes
                    self.scene.working_scene.compact_materials();
                    self.nodes.materials_dirty = true;
                }
```

(The trailing `reload_working_scene` and the "no UsdRead nodes remain → clear USD stage / selection" block stay unchanged. Note: `RemoveNodeCloud` already does `node_scatter_surface_map.remove(&node_id)`, so delete the now-redundant standalone `self.nodes.node_scatter_surface_map.remove(&node_id);` line if it remains.)

- [ ] **Step 7: Build + green gate**

```bash
cargo build -p bif_viewport
cargo clippy -p bif_viewport -- -D warnings
. .\setup_usd_env.ps1; cargo test -p bif_viewport
cargo fmt --check
```
Expected: build clean; no clippy warnings; all pre-existing `bif_viewport` tests pass + the 2 new `scene_cmd` tests; fmt clean. **No behavior change.**

- [ ] **Step 8: Commit**

```bash
git add crates/bif_viewport/src/node_dispatch.rs
git commit -m "refactor(nodes): route dispatch bookkeeping through SceneCmd::execute (issue #6 phase 2)"
```

---

### Task 3: Per-PR docs + verification

**Files:** `CHANGELOG.md`, `devlog/2026-06/DEVLOG_2026-06-08.md`, `SESSION_HANDOFF.md`

- [ ] **Step 1: Grep guard — confirm routed arms no longer inline the coupling**

```bash
git grep -n "node_outputs.entry" crates/bif_viewport/src/node_dispatch.rs
git grep -n "add_point_cloud\|remove_point_cloud" crates/bif_viewport/src/node_dispatch.rs
```
Expected: **no hits** in `node_dispatch.rs` (all moved into `scene_cmd.rs`). Any remaining hit is an arm that still inlines the coupling — route it or justify in the devlog.

- [ ] **Step 2: CHANGELOG `[Unreleased]`** — add under `### Changed` (or `### Internal`):

```markdown
- Node dispatch: introduce `SceneCmd` + `Renderer::execute()`; route per-node
  proto/cloud bookkeeping through one mutation site (issue #6 phase 2).
```

- [ ] **Step 3: Devlog** — create `devlog/2026-06/DEVLOG_2026-06-08.md` per the CLAUDE.md template (Goals / What I Did / Learnings / Next Session: Phase 3 = move `evaluate`/`register_prims`/`apply` into `node_graph/behavior.rs`).

- [ ] **Step 4: SESSION_HANDOFF.md** — update current state to "issue #6 Phase 2 merged (SceneCmd surface); Phase 3 next".

- [ ] **Step 5: Commit**

```bash
git add CHANGELOG.md devlog/2026-06/DEVLOG_2026-06-08.md SESSION_HANDOFF.md
git commit -m "docs: changelog + devlog for issue #6 phase 2"
```

- [ ] **Step 6: Open PR**

```bash
gh pr create --fill --base main
```
Then run `/vfx-code-reviewer` on the PR diff before merging.

---

## Verification (end-to-end)

1. `cargo build -p bif_viewport && . .\setup_usd_env.ps1; cargo test -p bif_viewport` — green, no regressions.
2. `cargo clippy -p bif_viewport -- -D warnings` — clean.
3. **Manual smoke** (needs full env): `. .\setup_usd_env.ps1; . .\setup_qt_env.ps1; cargo run -p bif_viewer`. Exercise each routed arm:
   - **Primitive** node → prim appears; change size → re-creates cleanly (RemoveNodeProtos + RecordProtos).
   - **Scatter** node on a mesh → points appear in preview; regenerate → old cloud gone, preview refreshed (RemoveNodeCloud + AddCloud + UploadPointPreview).
   - **PointInstancer** → instances appear; scene-browser updates (MarkSceneGraphDirty).
   - **UsdPrim** / **GraftBranches** → scene browser reflects the new prim.
   - **Load USD** via UsdRead, then reload same node → protos swap cleanly.
   - **Delete** a Scatter node and a Primitive node → cloud + protos removed, preview refreshed, materials GC'd, scene reloads.
4. Grep guard from Task 3 Step 1 passes.

## Testing & Future Work — penciled test-`Renderer`

`execute()`'s CPU-only verbs (`RemoveNodeProtos`, `RecordProtos`, `RemoveNodeCloud`, `AddCloud`) touch only `self.nodes.node_outputs`, `self.scene.working_scene`, and `self.nodes.next_cloud_id` — no GPU. They are *unit-testable in principle* but blocked today because `Renderer::new()` requires a wgpu device. **Penciled for Phase 3** (do NOT build now):

- **Option A (preferred):** Phase 3 extracts the CPU verb bodies into a free function `apply_cpu_cmd(outputs: &mut HashMap<GraphNodeId, NodeOutputs>, scene: &mut bif_core::Scene, next_cloud_id: &mut usize, cmd: &SceneCmd)`; `execute()` borrows the three disjoint fields and delegates (GPU `UploadPointPreview` stays on `Renderer`). The free fn is testable with a stub `Scene` + empty maps — no `Renderer` at all. This is the natural shape once `behavior.rs::apply()` *constructs* `SceneCmd`s.
- **Option B:** a `#[cfg(test)] Renderer::new_headless()` building a minimal struct (most fields `Default`/dummy) — heavier, GPU-less device still needed for `point_preview`; only viable if a `wgpu` test adapter is wired. Lower priority.

Phase 2 ships with the regression gate (existing `bif_viewport` suite green + manual smoke) + the 2 enum sanity tests. This is honest about coverage rather than faking a red phase for a GPU-bound method.

## Self-Review

- **Spec coverage:** RFC migration step (2) "add `SceneCmd` + `execute()`, route dispatch arms through it one at a time" — Tasks 1–2. Steps (1) done in Phase 1; (3)–(5) are Phase 3 (`behavior.rs`, delegators, trait-test module), explicitly out of scope. ✓
- **Placeholder scan:** every routed arm has exact before/after code; the 6 verbs + full `execute()` body are concrete; no "TBD"/"handle edge cases". The one flagged check (`GraphNodeId::from(NodeId(0))` constructor) is a named verify-step with a fallback, not a placeholder. ✓
- **Type consistency:** `SceneCmd::{RemoveNodeProtos, RecordProtos, RemoveNodeCloud, AddCloud, UploadPointPreview, MarkSceneGraphDirty}`, field names `node`/`proto_ids`/`cloud`, `execute(&mut self, cmd: SceneCmd) -> ()`, `bif_core::PointCloud` — consistent across enum def, `execute()` body, and all Task 2 call sites. ✓
- **Behavior preservation:** `RemoveNodeProtos` deliberately omits `compact_materials` (callers gate it on `had_protos`); `DeleteNode` preserves "upload preview only if a cloud existed" via the `had_cloud` snapshot; `LoadUsdFile`/`CreatePrimitive` proto-prune semantics match Phase 1's translation table. ✓

## Unresolved questions

- **`compact_materials` placement in `LoadUsdFile`:** original always compacted when old protos existed (after removal, before load). New code gates on `had_protos` snapshot taken *before* `RemoveNodeProtos` — equivalent, but confirm during smoke that a UsdRead *reload* still starts with clean material offsets.
- **`GraphNodeId` ↔ `egui_snarl::NodeId` constructor** in the Task 1 test (`NodeId(0)` tuple vs `from(0usize)`) — resolve against `node_graph/node_id.rs` at implementation; affects test literal only.
- **`scene_graph_dirty` as a verb vs direct field write:** included `MarkSceneGraphDirty` for the 3 arms that set it; if `/vfx-code-reviewer` deems a 1-field flag unworthy of a verb, dropping it back to a direct write is trivial and non-blocking.
