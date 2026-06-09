# Issue #6 Phase 3a — `SceneNode::evaluate()` extraction — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move the per-node auto-compute logic out of `eval.rs::evaluate_node`'s big `match` onto a `SceneNode::evaluate()` method in a new `node_graph/behavior.rs`, reducing `evaluate_node` to a generic delegator — first slice of Phase 3 of the [issue #6](https://github.com/byvfx/bif/issues/6) RFC (the `behavior.rs` extraction).

**Architecture:** Introduce `EvalCtx` (precomputed, snarl-free connection data), `EvalOutcome { events, dirty }`, and `SceneNode::evaluate(&mut self, id, ctx, mode) -> EvalOutcome`. The node mutates its own status flags in place (`&mut self`); the delegator does a read-phase (build `EvalCtx` from the snarl) then a mutate-phase (borrow the node mutably, call `evaluate`). Because `EvalCtx` is plain data, `evaluate` is **fully unit-testable with no Snarl/GPU/USD**. Pure behavior-preserving move.

**Tech Stack:** Rust, `bif_viewport` crate. `evaluate` tests are dep-free. `cargo test -p bif_viewport` needs the USD env sourced for the test binary to link (CLAUDE.md Gotchas).

**Workflow:** Branch `refactor/issue6-phase3a-evaluate` (stacked on `refactor/issue6-phase2-scenecmd` — 3a's docs would otherwise conflict with PR #9's CHANGELOG/devlog/handoff edits; the code files don't overlap) → commits per task → PR → auto `claude-review` + manual `/vfx-code-reviewer` → squash-merge. Merge PR #9 (Phase 2) first.

---

## Design Decisions (settled during brainstorming)

1. **Decompose Phase 3 into 3a/3b/3c.** 3a = `evaluate()`; 3b = `register_prims()`; 3c = `apply()` (deferred — the `apply -> Vec<SceneCmd>` shape has a known tension: dispatch arms also call loaders/`reload`/GPU/flags that aren't `SceneCmd`s). This plan is **3a only**.
2. **`EvalCtx` is precomputed data, not a snarl borrow.** Required because `evaluate(&mut self)` borrows the node out of the snarl, so it can't also hold `&snarl` for connection queries. The delegator reads connections first, into `EvalCtx`.
3. **`&mut self` (not `&self` + status-descriptor).** Keeps ONE per-variant match (each arm reads fields and flips its own flag in place). The `&self`-pure alternative would need a second per-variant `apply_status` match — re-introducing the smear we're removing.
4. **Build `EvalCtx` for `0..node.input_count()`** (`SceneNode::input_count()` exists). `egui_snarl::Snarl::in_pin` is panic-safe for any index (`InPin::new` just filters wires), but bounding by `input_count()` is cleaner and avoids out-of-range queries entirely.
5. **`extract_scatter_params` is inlined + deleted.** Its fields are already bound in `evaluate`'s ScatterPoints arm; the free fn (which read the snarl) is no longer reachable.

## Current State (verified)

- `crates/bif_viewport/src/node_graph/eval.rs` (813 lines):
  - `collect_auto_compute_events(&mut Snarl, EvalMode, &mut HashSet<GraphNodeId>) -> Vec<NodeGraphEvent>` (eval.rs:35) — iterates node ids, calls `evaluate_node`.
  - `evaluate_node(node_id, &mut Snarl, EvalMode, &mut HashSet, &mut Vec<NodeGraphEvent>)` (eval.rs:53) — the `match &snarl[node_id]` with 5 active arms (Primitive, ScatterPoints, PointInstancer, Xform, UsdPrim) + `_ => {}`. Each reads fields + connections, then re-borrows `&mut snarl[node_id]` to flip status flags, pushing events or inserting into `dirty_nodes`.
  - `is_input_connected(node_id, input, &Snarl) -> Option<NodeId>` (eval.rs:16) — `snarl.in_pin(...).remotes.first().map(|r| r.node)`. **Stays** (delegator uses it).
  - `extract_scatter_params(&Snarl, NodeId) -> ScatterPointsParams` (eval.rs:220) — **deleted** in this plan.
  - `#[cfg(test)] mod tests` (eval.rs:269+) — existing real-Snarl tests (`test_primitive_auto_creates`, `test_scatter_*`, etc.). **Stay unchanged** as the regression gate.
- `SceneNode::input_count(&self) -> usize` exists (used by the `NodeViewer::inputs` impl, `viewer.rs`).
- `EvalMode` is `crate::persistence::EvalMode` (enum `Auto` / `Manual`).
- `ScatterPointsParams`, `NodeGraphEvent`, `GraphNodeId`, `SceneNode` are all in `crate::node_graph` (`super` from within the module).
- `node_graph/mod.rs` declares modules (`pub(crate) mod eval; mod node_outputs; mod viewer; pub mod node_id; pub mod ops;` + `pub use`s).

---

## File Structure

- **Create:** `crates/bif_viewport/src/node_graph/behavior.rs` — `EvalCtx`, `EvalOutcome`, `impl SceneNode { evaluate }`, and the pure `evaluate` unit tests. One responsibility: per-node behavior (this phase: evaluation; 3b/3c add `register_prims`/`apply` here).
- **Modify:** `crates/bif_viewport/src/node_graph/mod.rs` — add `mod behavior;`.
- **Modify:** `crates/bif_viewport/src/node_graph/eval.rs` — rewrite `evaluate_node` as a generic delegator; delete `extract_scatter_params`; add `use super::behavior::EvalCtx;`.

---

### Task 1: Add `behavior.rs` with `EvalCtx`, `EvalOutcome`, `SceneNode::evaluate` + pure tests

**Files:**
- Create: `crates/bif_viewport/src/node_graph/behavior.rs`
- Modify: `crates/bif_viewport/src/node_graph/mod.rs` (add `mod behavior;`)

- [ ] **Step 1: Create `behavior.rs`** with this exact content:

```rust
//! Per-node behavior: the cohesive region where each `SceneNode` variant's
//! logic lives, instead of being smeared across `eval.rs`, `scene_browser.rs`,
//! and `node_dispatch.rs` (issue #6 Phase 3).
//!
//! Phase 3a: `evaluate` (auto-compute readiness). 3b adds `register_prims`,
//! 3c adds `apply`.

use super::{GraphNodeId, NodeGraphEvent, ScatterPointsParams, SceneNode};
use crate::persistence::EvalMode;

/// Read-only, snarl-free view of a node's input connections, precomputed by
/// the caller so `evaluate` can take `&mut self` without also borrowing the
/// graph. `input_sources[i]` is the upstream node feeding input pin `i`.
pub(crate) struct EvalCtx {
    input_sources: Vec<Option<GraphNodeId>>,
}

impl EvalCtx {
    /// Build from a per-input list of upstream sources (index = input pin).
    pub(crate) fn new(input_sources: Vec<Option<GraphNodeId>>) -> Self {
        Self { input_sources }
    }

    /// Upstream node feeding input pin `input`, if connected.
    pub(crate) fn input_source(&self, input: usize) -> Option<GraphNodeId> {
        self.input_sources.get(input).copied().flatten()
    }

    /// Whether input pin `input` is connected.
    pub(crate) fn is_input_connected(&self, input: usize) -> bool {
        self.input_source(input).is_some()
    }
}

/// Result of evaluating one node: events to dispatch + whether the node should
/// be marked dirty (caller inserts it into `dirty_nodes`).
#[derive(Default)]
pub(crate) struct EvalOutcome {
    pub events: Vec<NodeGraphEvent>,
    pub dirty: bool,
}

impl SceneNode {
    /// Auto-compute readiness for this node. Mutates the node's own status
    /// flags in place; returns events to dispatch + a dirty signal. Pure of
    /// the graph (connections arrive via `ctx`) — unit-testable without a Snarl.
    pub(crate) fn evaluate(
        &mut self,
        id: GraphNodeId,
        ctx: &EvalCtx,
        mode: EvalMode,
    ) -> EvalOutcome {
        let mut out = EvalOutcome::default();
        match self {
            // --- Primitive: auto-create geometry when not yet created ------
            SceneNode::Primitive {
                is_created,
                kind,
                size,
                ..
            } if !*is_created => {
                let kind = *kind;
                let size = *size;
                if mode == EvalMode::Auto {
                    out.events.push(NodeGraphEvent::CreatePrimitive {
                        kind,
                        size,
                        node_id: id,
                    });
                    *is_created = true;
                } else {
                    out.dirty = true;
                }
            }

            // --- ScatterPoints: auto-compute when inputs satisfied ---------
            SceneNode::ScatterPoints {
                is_computed,
                source,
                count,
                max_point_limit,
                seed,
                scatter_mode,
                min_distance,
                align_to_normal,
                grid_size,
                grid_spacing,
                sphere_radius,
                sphere_on_surface,
                relax_iterations,
                scale_radii,
                max_relax_radius,
                scale_min,
                scale_max,
                rotation_range,
                ..
            } if !*is_computed => {
                let inputs_satisfied = match *source {
                    bif_core::PointSource::Surface => ctx.is_input_connected(0),
                    bif_core::PointSource::Grid | bif_core::PointSource::Sphere => true,
                };
                if inputs_satisfied {
                    if mode == EvalMode::Auto {
                        let params = ScatterPointsParams {
                            source: *source,
                            count: *count,
                            max_point_limit: *max_point_limit,
                            seed: *seed,
                            scatter_mode: *scatter_mode,
                            min_distance: *min_distance,
                            align_to_normal: *align_to_normal,
                            grid_size: *grid_size,
                            grid_spacing: *grid_spacing,
                            sphere_radius: *sphere_radius,
                            sphere_on_surface: *sphere_on_surface,
                            relax_iterations: *relax_iterations,
                            scale_radii: *scale_radii,
                            max_relax_radius: *max_relax_radius,
                            scale_min: *scale_min,
                            scale_max: *scale_max,
                            rotation_range: *rotation_range,
                            target_proto_id: None,
                        };
                        out.events.push(NodeGraphEvent::ScatterPointsCompute {
                            node_id: id,
                            params,
                        });
                        *is_computed = true;
                    } else {
                        out.dirty = true;
                    }
                }
            }

            // --- PointInstancer: auto-invalidate or auto-compute -----------
            SceneNode::PointInstancer {
                is_instanced,
                is_computing,
                compute_failed,
                instance_count,
                ..
            } => {
                let points_node = ctx.input_source(0);
                let proto_node = ctx.input_source(1);
                let both_connected = points_node.is_some() && proto_node.is_some();

                if !both_connected && *is_instanced {
                    // Inputs disconnected but still marked instanced.
                    out.events.push(NodeGraphEvent::InstancerInvalidate { node_id: id });
                    *is_instanced = false;
                    *is_computing = false;
                    *compute_failed = false;
                    *instance_count = 0;
                } else if both_connected && !*is_instanced && !*is_computing && !*compute_failed {
                    if mode == EvalMode::Auto {
                        if let (Some(points_source), Some(proto_source)) = (points_node, proto_node) {
                            out.events.push(NodeGraphEvent::PointInstancerCompute {
                                node_id: id,
                                points_source_node: points_source,
                                proto_source_node: proto_source,
                            });
                            *is_computing = true;
                        }
                    } else {
                        out.dirty = true;
                    }
                }
            }

            // --- Xform: rebuild scene once input is connected --------------
            SceneNode::Xform { is_applied, .. } if !*is_applied => {
                if ctx.is_input_connected(0) {
                    if mode == EvalMode::Auto {
                        out.events.push(NodeGraphEvent::XformChanged { node_id: id });
                        *is_applied = true;
                    } else {
                        out.dirty = true;
                    }
                }
            }

            // --- UsdPrim: register authored prim metadata ------------------
            SceneNode::UsdPrim { is_created, .. } if !*is_created => {
                if mode == EvalMode::Auto {
                    out.events.push(NodeGraphEvent::UsdPrimCreate { node_id: id });
                    *is_created = true;
                } else {
                    out.dirty = true;
                }
            }

            // All other node types have no auto-compute logic.
            _ => {}
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: usize) -> GraphNodeId {
        GraphNodeId::from(egui_snarl::NodeId(n))
    }

    #[test]
    fn primitive_auto_creates_and_marks_created() {
        let mut node = SceneNode::primitive(bif_core::PrimitiveKind::Cube);
        let ctx = EvalCtx::new(vec![]);
        let out = node.evaluate(id(0), &ctx, EvalMode::Auto);
        assert_eq!(out.events.len(), 1);
        assert!(matches!(
            out.events[0],
            NodeGraphEvent::CreatePrimitive { .. }
        ));
        assert!(!out.dirty);
        assert!(matches!(
            node,
            SceneNode::Primitive { is_created: true, .. }
        ));
    }

    #[test]
    fn primitive_manual_marks_dirty_no_event() {
        let mut node = SceneNode::primitive(bif_core::PrimitiveKind::Sphere);
        let ctx = EvalCtx::new(vec![]);
        let out = node.evaluate(id(0), &ctx, EvalMode::Manual);
        assert!(out.events.is_empty());
        assert!(out.dirty);
        assert!(matches!(
            node,
            SceneNode::Primitive { is_created: false, .. }
        ));
    }

    #[test]
    fn primitive_already_created_is_noop() {
        let mut node = SceneNode::primitive(bif_core::PrimitiveKind::Cube);
        if let SceneNode::Primitive { is_created, .. } = &mut node {
            *is_created = true;
        }
        let ctx = EvalCtx::new(vec![]);
        let out = node.evaluate(id(0), &ctx, EvalMode::Auto);
        assert!(out.events.is_empty());
        assert!(!out.dirty);
    }

    #[test]
    fn scatter_surface_needs_input() {
        let mut node = SceneNode::scatter_points(); // defaults to Surface source
        // No input connected -> no event, not dirty.
        let out = node.evaluate(id(0), &EvalCtx::new(vec![None]), EvalMode::Auto);
        assert!(out.events.is_empty());
        assert!(!out.dirty);
        // Input connected -> compute event + marked computed.
        let out = node.evaluate(id(0), &EvalCtx::new(vec![Some(id(1))]), EvalMode::Auto);
        assert_eq!(out.events.len(), 1);
        assert!(matches!(
            out.events[0],
            NodeGraphEvent::ScatterPointsCompute { .. }
        ));
        assert!(matches!(
            node,
            SceneNode::ScatterPoints { is_computed: true, .. }
        ));
    }

    #[test]
    fn instancer_computes_when_both_inputs_connected() {
        let mut node = SceneNode::point_instancer();
        let ctx = EvalCtx::new(vec![Some(id(1)), Some(id(2))]);
        let out = node.evaluate(id(0), &ctx, EvalMode::Auto);
        assert!(matches!(
            out.events.first(),
            Some(NodeGraphEvent::PointInstancerCompute {
                points_source_node,
                proto_source_node,
                ..
            }) if *points_source_node == id(1) && *proto_source_node == id(2)
        ));
        assert!(matches!(
            node,
            SceneNode::PointInstancer { is_computing: true, .. }
        ));
    }

    #[test]
    fn xform_waits_for_input() {
        let mut node = SceneNode::xform();
        let out = node.evaluate(id(0), &EvalCtx::new(vec![None]), EvalMode::Auto);
        assert!(out.events.is_empty());
        let out = node.evaluate(id(0), &EvalCtx::new(vec![Some(id(1))]), EvalMode::Auto);
        assert!(matches!(
            out.events.first(),
            Some(NodeGraphEvent::XformChanged { .. })
        ));
    }
}
```

> Verify during implementation (do not assume): the constructor names `SceneNode::primitive(PrimitiveKind)`, `SceneNode::scatter_points()`, `SceneNode::point_instancer()`, `SceneNode::xform()` exist with these signatures (they are used in `viewer.rs::show_graph_menu` and `eval.rs` tests). `scatter_points()` must default to `PointSource::Surface` for `scatter_surface_needs_input` to be meaningful — confirm against the constructor; if it defaults to Grid/Sphere, adjust that test to set `source` to Surface first (Grid/Sphere are always input-satisfied). `GraphNodeId::from(egui_snarl::NodeId(n))` is the same constructor used in `scene_cmd.rs` tests.

- [ ] **Step 2: Wire the module** — in `crates/bif_viewport/src/node_graph/mod.rs`, add `mod behavior;` with the other `mod` lines (e.g. after `pub(crate) mod eval;`). No `pub use` needed — `EvalCtx`/`EvalOutcome` are consumed only by `eval.rs` via `super::behavior::…`.

- [ ] **Step 3: Build + run the new tests**

```
cargo build -p bif_viewport
. .\setup_usd_env.ps1; cargo test -p bif_viewport behavior
```
Expected: build clean; 6 `behavior::tests` pass. Note: `evaluate` is `pub(crate)` and not yet called outside tests until Task 2 — if rustc/clippy flags dead_code on `evaluate`/`EvalCtx`/`EvalOutcome`, that's expected and resolved in Task 2 (callers land). Prefer NOT to add `#[allow(dead_code)]` if Task 1 and Task 2 are committed together; if committing Task 1 alone, a temporary `#[allow(dead_code)]` on the `impl`/structs is acceptable (removed in Task 2). Run `cargo fmt`.

- [ ] **Step 4: Commit**

```
git add crates/bif_viewport/src/node_graph/behavior.rs crates/bif_viewport/src/node_graph/mod.rs
git commit -m "feat(nodes): add SceneNode::evaluate in behavior.rs (issue #6 phase 3a)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 2: Reduce `evaluate_node` to a delegator; delete `extract_scatter_params`

**Files:** `crates/bif_viewport/src/node_graph/eval.rs`

- [ ] **Step 1: Add the import** — near the top of `eval.rs`, add:

```rust
use super::behavior::EvalCtx;
```

- [ ] **Step 2: Replace the whole `evaluate_node` fn** (eval.rs:52–215, the doc comment + the entire `match`) with the generic delegator:

```rust
/// Evaluate a single node for auto-compute readiness.
///
/// Read-phase: snapshot input connections into an [`EvalCtx`]. Mutate-phase:
/// borrow the node mutably and run its `evaluate`, fanning the outcome into
/// `events` / `dirty_nodes`. Per-variant logic lives in `behavior.rs`.
fn evaluate_node(
    node_id: NodeId,
    snarl: &mut Snarl<SceneNode>,
    eval_mode: EvalMode,
    dirty_nodes: &mut HashSet<GraphNodeId>,
    events: &mut Vec<NodeGraphEvent>,
) {
    let input_count = snarl[node_id].input_count();
    let input_sources: Vec<Option<GraphNodeId>> = (0..input_count)
        .map(|i| is_input_connected(node_id, i, snarl).map(GraphNodeId::from))
        .collect();
    let ctx = EvalCtx::new(input_sources);

    let graph_id = GraphNodeId::from(node_id);
    let outcome = snarl[node_id].evaluate(graph_id, &ctx, eval_mode);
    events.extend(outcome.events);
    if outcome.dirty {
        dirty_nodes.insert(graph_id);
    }
}
```

- [ ] **Step 3: Delete `extract_scatter_params`** (eval.rs:217–263, the doc comment + fn + its `unreachable!`). It is now unused (inlined into `SceneNode::evaluate`). After deletion, fix any now-unused imports rustc flags (e.g. `ScatterPointsParams` may no longer be needed in `eval.rs`; remove it from the `use super::{...}` line only if rustc says it's unused — the tests below may still reference it).

- [ ] **Step 4: Green gate**

```
cargo build -p bif_viewport
cargo clippy -p bif_viewport -- -D warnings
. .\setup_usd_env.ps1; cargo test -p bif_viewport
cargo fmt --check
```
Expected: build clean; no clippy warnings (incl. no dead_code — `evaluate` now has a caller; remove any temporary `#[allow(dead_code)]` added in Task 1); ALL `bif_viewport` tests pass — the existing `eval.rs` real-Snarl tests (`test_primitive_auto_creates`, `test_scatter_*`, `test_scatter_manual_marks_dirty`, etc.) + the 6 new `behavior` tests + everything else; fmt clean. **Zero behavior change.** (`test_should_restart_no_render` is known timing-flaky — rerun once if it alone fails.)

- [ ] **Step 5: Commit**

```
git add crates/bif_viewport/src/node_graph/eval.rs crates/bif_viewport/src/node_graph/behavior.rs
git commit -m "refactor(nodes): evaluate_node delegates to SceneNode::evaluate (issue #6 phase 3a)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 3: Per-PR docs + PR

**Files:** `CHANGELOG.md`, `devlog/2026-06/DEVLOG_2026-06-09.md` (append), `SESSION_HANDOFF.md`

- [ ] **Step 1: CHANGELOG `[Unreleased]`** — add a bullet under `### Changed`:

```markdown
- **`SceneNode::evaluate()` extraction ([#6](https://github.com/byvfx/bif/issues/6) phase 3a)**. Moved the per-node auto-compute logic out of `eval.rs::evaluate_node`'s match onto `SceneNode::evaluate(&mut self, id, &EvalCtx, EvalMode) -> EvalOutcome` in a new `node_graph/behavior.rs`. `EvalCtx` carries precomputed, snarl-free input-connection data so `evaluate` is pure of the graph and unit-testable without a Snarl/GPU/USD; `evaluate_node` is now a generic read-phase/mutate-phase delegator. `extract_scatter_params` inlined + removed. Behavior-preserving (existing real-Snarl eval tests green) + 6 new pure `evaluate` unit tests. First slice of the issue #6 `behavior.rs` extraction; 3b (`register_prims`) and 3c (`apply`) follow.
```

- [ ] **Step 2: Devlog** — append a short Phase 3a section to `devlog/2026-06/DEVLOG_2026-06-09.md` (Goals / What I Did / Learnings / Next: 3b `register_prims`).

- [ ] **Step 3: SESSION_HANDOFF.md** — update Current State (branch `refactor/issue6-phase3a-evaluate`, Phase 3a done) + add a brief session entry. Note: stacked on Phase 2 / PR #9.

- [ ] **Step 4: Commit + push + PR**

```
git add CHANGELOG.md devlog/2026-06/DEVLOG_2026-06-09.md SESSION_HANDOFF.md
git commit -m "docs: changelog + devlog + handoff for issue #6 phase 3a"
git push -u origin refactor/issue6-phase3a-evaluate
```
Open the PR with base `refactor/issue6-phase2-scenecmd` (stacked — keeps the diff to just 3a) OR base `main` noting #9 merges first. Then run `/vfx-code-reviewer`.

---

## Verification (end-to-end)

1. `cargo build -p bif_viewport && . .\setup_usd_env.ps1; cargo test -p bif_viewport` — green, no regressions (existing eval tests + 6 new).
2. `cargo clippy -p bif_viewport -- -D warnings` — clean.
3. Grep guard: `git grep -n "extract_scatter_params"` → no hits (deleted). `git grep -n "fn evaluate_node"` → one hit (the delegator, ~20 lines, no per-variant `match`).
4. Manual smoke (optional, full env): create Primitive / Scatter(grid) / Scatter-on-surface / PointInstancer / Xform / UsdPrim nodes → each still auto-computes exactly as before; disconnecting an instancer's input still auto-invalidates.

## Self-Review

- **Spec coverage:** RFC migration step (3) "move eval per-node logic into behavior.rs" + (4) "reduce eval.rs to a delegator" + (5) "trait-level tests" — all covered for the *evaluate* slice (Tasks 1–2 + the 6 pure tests). `register_prims`/`apply` are 3b/3c. ✓
- **Placeholder scan:** full `evaluate` body + delegator + 6 tests are concrete. The one flagged item (constructor names / `scatter_points()` default source) is a named verify-step with a fallback, not a placeholder. ✓
- **Type consistency:** `EvalCtx::new`/`input_source`/`is_input_connected`, `EvalOutcome { events, dirty }`, `evaluate(&mut self, id, &EvalCtx, EvalMode) -> EvalOutcome` — consistent across behavior.rs, the delegator, and the tests. ✓
- **Behavior preservation:** every original `evaluate_node` arm maps 1:1 (guards `is_created: false` → `if !*is_created`; `is_input_connected(node_id, i, snarl)` → `ctx.is_input_connected(i)` / `ctx.input_source(i)`; flag flips identical; `dirty_nodes.insert` ↔ `out.dirty`). PointInstancer invalidate flips the same 4 fields. ✓

## Unresolved questions

- **`scatter_points()` default `source`** — if it isn't `Surface`, the `scatter_surface_needs_input` test must set `source = Surface` first (otherwise Grid/Sphere are always input-satisfied and the "needs input" assertion is vacuous). Resolve at implementation against the constructor.
- **Commit granularity** — Task 1 leaves `evaluate` callerless (dead_code). Preference: commit Task 1 + Task 2 back-to-back (or squash) so `main` never carries a dead `#[allow]`. If kept separate, the temporary allow is removed in Task 2.
