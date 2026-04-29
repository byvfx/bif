---
title: ADR-005 — Layer-aware read model (v0.14.0)
type: adr
status: accepted
tags: [architecture, usd, layers, v0.14]
created: 2026-04-13
updated: 2026-04-26
---

# ADR-005 — Layer-aware read model (v0.14.0)

## Status

Accepted — shipped in v0.14.0 (2026-04-13).

## Context

BIF v0.13.x loads and renders USD composed stages but is completely layer-blind. Users cannot inspect the sublayer stack, identify which layer authored a particular value, mute layers, or choose a working layer. The 2026-03-28 hybrid-workflow decision (ADR-003) committed BIF to layer awareness as a core differentiator; v0.14 is that promise's first delivery, intentionally scoped to **inspection only** — editing lands in v0.16.

## Decisions

### 1. Read-only in v0.14

BIF v0.14 exposes the layer stack, prim stack, opinion stack, and mute toggle, but does **not** author any opinions. Rationale:

- The read path is independently useful (artists need to see layer attribution before they trust BIF with edits).
- Opinion-value serialization (`VtValue` round-trip) is non-trivial; we render values via `TfStringify` for display only.
- A read-only release forces the data types to be UI-agnostic (no write-side coupling).

Consequence: `EditTarget`, `WorkingLayerChanged`, `IsolationModeToggled`, and `PayloadPolicyChanged` are all informational in v0.14 — they update `SceneLayerState` for UI presentation but have no side effect on the stage. `UsdStage::set_layer_muted` is the only layer-modifying call exposed.

### 2. UI-agnostic types in `bif_core`

All safe types — `LayerStack`, `LayerInfo`, `LayerOffset`, `PrimStackEntry`, `OpinionSource`, `EditTarget`, `PayloadPolicy`, `SceneLayerState` — live in `bif_core` with no egui / Qt references. `bif_viewport` owns the panel rendering layer; v0.15's Qt port will replace panels without touching data.

Consequence: the `LayerStackPanel` + `render_attribute_layer_dot` + scene-browser color dots are the churn surface at the v0.15 boundary. Types + runtime state flow through unchanged.

### 3. `SceneLayerState` on `SceneManager`, not `Scene`

An earlier draft attached `layer_state` to `bif_core::Scene`. It didn't work: `scene_loader::finalize_usd_scene` takes the parsed `bif_core::Scene` by value and pulls individual fields (`mesh_data`, `instance_animations`, `materials`, `cameras`) into `self.scene.*`, then discards the rest. Any field we added to `Scene` was written on the local binding and dropped at end of function.

Fix: hoist `layer_state` onto `SceneManager` directly. Scene loader writes `self.scene.layer_state = Some(state)`, readers go through `self.scene.layer_state.as_ref()`.

Consequence: `bif_core::Scene` has no v0.14-specific state. `SceneLayerState` is still re-exported from `bif_core::lib` for callers, but nothing in `bif_core` owns it at runtime.

### 4. Drop `PayloadPolicy::BoundingBoxOnly` from v0.14

USD has no native bounding-box-only load mode. Implementing one requires `LoadNone` + selective `Load()` driven by authored `extentsHint` primvars, which works only on well-behaved assets. Deferred to v0.16 (or later) if artists ask.

`PayloadPolicy` in v0.14 is `{ LoadAll, LoadNone }` only.

### 5. No LRU opinion cache

The phase plan called for an LRU cache on `SceneLayerState::opinion_trace` to amortize stage queries across UI refreshes. Dropped from v0.14 because clean interior mutability (`Mutex<LruCache>`) breaks `Scene: Clone`. Opinion queries run eagerly at selection time in `selection_dispatch::build_prim_properties` (~N calls for an N-attribute prim). If a hot path emerges, the UI layer can maintain its own per-selection cache without changing `SceneLayerState`.

### 6. No Qt trait abstraction now

v0.15 migrates egui → Qt. The plan considered introducing a `LayerStackView` trait to shield the panel code from egui. Rejected: one consumer today, no benefit from indirection; the clean split is already at the `bif_core::SceneLayerState` boundary. v0.15 does the Qt rewrite in one focused pass.

## Constraints honored

- **USD forbids muting the root layer.** Our FFI surfaces the call but the USD bridge silently no-ops (with a `TF_WARN`). Integration test `test_mute_layer_recomposes_opinions` mutes a sublayer instead.
- **Pin to USD 25.11.** `UsdPrim::GetPrimStack` + `UsdAttribute::GetPropertyStack` + `TfStringify` are stable across 25.x; `cpp/CMakeLists.txt` pins the vcpkg port.

## Outcomes

- ~90 new tests (convert + fixture + integration), all green except two pre-existing USD bridge crashes unrelated to v0.14.
- 22 commits on the v0.14.0 release stack, no regressions on v0.13.x skinning/blend shapes.
- UI-agnostic data contract makes the v0.15 Qt port a pure rendering-layer rewrite.

## Consequences

- v0.14.5 follow-up (file watcher + node graph color-coding + Layer Stack display node) can land incrementally without architectural changes.
- v0.16 editing gained the write-side architecture in [[008-edit-operation-architecture|ADR-008]]: `EditOperation` / `EditHistory`, working-layer FFI writes through `UsdEditContext`, and Ctrl+S save for the active layer.
- If v0.16 needs value-typed round-trip (not just display strings), the FFI must grow a `set_attribute_value(path, attr, VtValue)` entry point and a matching Rust-side type bridge.
