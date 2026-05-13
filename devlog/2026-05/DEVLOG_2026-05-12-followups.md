# Development Log - 2026-05-12 (v0.16.3 followups)

## Session Duration
~1 hour

## Goals
Run `vfx-code-reviewer` on the v0.16.2 merge `3ef5131` and fix every non-deferred issue.

## What I Did

Spawned `vfx-code-reviewer` agent against the merge. Surfaced 5 issues, 3 actionable now, 2 deferred to v0.17.

### Fixes landed (branch `v0.16.3-followups`)

1. **`cpp/usd_bridge/usd_bridge.cpp`** — `usd_bridge_get_prim_relationships`: added `if (entries.empty()) return SUCCESS;` defensive guard before `new UsdBridgeRelationshipData[count]`. The existing `rels.empty()` early-return at the top already prevents the zero-allocation today, but bulletproofs against future loop changes that might filter entries. The Rust-side `convert_relationships_ptr` short-circuits on `count==0` before invoking the free, so `new T[0]` (which the standard says returns a unique non-null pointer) would have leaked silently.

2. **`crates/bif_qt/cpp/scene_browser_model.cpp`** — `refresh_node`: strengthened safety comment and added `Q_ASSERT(!node->children_populated || node->child_count == 0)`. Reviewer was concerned about begin/endInsertRows firing during the recursive walk in `find_source_index_for_path` — but the mutation is on a child node, not the ancestor being iterated, so Qt's row-insertion semantics keep the parent walk safe. The `children.empty()` guard is the load-bearing invariant; documented why and added a TODO to convert callers to two-pass pre-populate in v0.17.

3. **`crates/bif_qt/src/main_window.rs`** — both `on_stage_path_opened` (load-stage path) and `set_working_layer` invokable: capture `prev_working_layer` before mutating the Qt-side `BifShellState`, and revert it on `set_edit_target` failure / stage-mutex poisoning. Renderer-side `state.set_edit_target` already early-returns on `layer_permission_to_edit` failure without mutating `working_layer`, so the desync was Qt UI vs C++ stage. The status bar still surfaces the warning; this just stops the UI from claiming an edit target the stage never adopted.

### Deferred (v0.17.0)

- `collect_invisible_ancestors` in `selection_dispatch.rs` over-reports vs `MakeVisible`'s authored-only walk. Acknowledged in code; needs an FFI extension to expose the authored-opinion-only ancestor walk.
- `TF_VERIFY(thread_id == owner_thread)` runtime guard in `cache_prim_data`. Doc-only contract today; runtime guard slated for the `cpp_bridge.rs` split.

## Verifications
- `cargo build -p bif_qt` — clean
- `cargo clippy -p bif_qt -p bif_core -- -D warnings` — clean
- `cargo fmt --check` — clean
- `cargo test -p bif_core -- --test-threads=1` — pass (incl. `save_as_roundtrip`, `edit_op_roundtrip`, mute rejection)
- `cargo test -p bif_qt` — 12 passed

## Branching

- Cut `v0.16.3-followups` off `main` for these fixes.
- `v0.17.0` branch also created off `main` (clean, no commits) ready for the v0.17 scope: `cpp_bridge.rs` split, viewport perf, payload policies, `MakeVisible` authored-only ancestor FFI, FFI thread-id guard.

## Learnings
- The "high severity" `refresh_node` mid-traversal concern from the reviewer was actually safe-by-construction — Qt's row-insertion semantics scope invalidation to the parent's children, not the parent's ancestor walk. The existing guard makes it provably safe; the fix was to make the invariant explicit (assert + comment) so a future contributor doesn't accidentally remove the guard.
- `set_edit_target` desync was the most artist-visible bug — UI says "edit target = anim.usd" but Save lands on `root.usd`. Always revert UI state when its underlying authority op fails.

## Next Session
- Open PR for `v0.16.3-followups` → `main`.
- Switch to `v0.17.0`, scope first task: `cpp_bridge.rs` split (the monolith review pre-work).
