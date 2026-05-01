---
title: Layer-Aware Stage
type: concept
tags: [usd, layers, composition, opinion, sdflayer, v0.14, v0.16]
created: 2026-04-13
updated: 2026-05-01
---

# Layer-Aware Stage

## Summary

BIF v0.14.0 lifts the lid on USD composition: load a multi-layer stage and BIF exposes the sublayer tree, the per-prim spec stack (`UsdPrim::GetPrimStack`), and every layer's contribution to each attribute value (`UsdAttribute::GetPropertyStack`). Users mute sublayers, pick a working layer, and see layer authorship inline via color dots and tooltips. v0.16.0 added the edit/save path on top: working-layer FFI writes through `UsdEditContext`, `EditHistory`, and Ctrl+S save. `PayloadPolicy` still ships only `LoadAll` and `LoadNone`; selective task-driven loading is _Future_ work.

## Data model

Safe Rust types live in `crates/bif_core/src/usd/layer.rs` — UI-agnostic, ready for the v0.15 Qt port.

- `LayerStack { layers: Vec<LayerInfo>, root_index: usize }` — flattened sublayer tree
- `LayerInfo { identifier, display_name, real_path, is_anonymous, is_dirty, is_muted, offset, parent_index, depth }`
- `LayerOffset { offset, scale }`
- `PrimStackEntry { layer_identifier, path, specifier, has_authored_opinions }`
- `PrimSpecifier::{Def, Over, Class}`
- `OpinionSource { layer_identifier, value_display, value_type, is_winning }` — value rendered via `TfStringify` (read-only)
- `EditTarget { layer_identifier }` — informational in v0.14
- `PayloadPolicy::{LoadAll, LoadNone}`

Runtime state lives on `bif_viewport::SceneManager::layer_state: Option<SceneLayerState>`. It is **not** a field on `bif_core::Scene` — see ADR-005 for why.

### `SceneLayerState` fields

```rust
pub struct SceneLayerState {
    pub stack: LayerStack,
    pub working_layer: usize,
    pub muted: HashSet<String>,
    pub isolation_mode: bool,
    pub payload_policy: PayloadPolicy,
    pub layer_for_prim: HashMap<String, usize>,  // color-dot map
}
```

## Composition cheatsheet (LIVRPS)

Layer strength, strongest → weakest:

1. **L**ocal opinions — what this prim spec authors directly
2. **I**nherits — `inherits = </Class>` references
3. **V**ariants — active `variantSet` selection
4. **R**eferences — composed external prims via `references`
5. **P**ayloads — deferred references (`payload`)
6. **S**pecializes — `specializes = </Template>` (weakest)

Within a layer stack, **root layer wins over sublayers**, and within sublayer siblings the **listed-first sublayer wins**. `UsdAttribute::GetPropertyStack` returns specs strongest-first, which is why `BIF OpinionSource` preserves that order and marks index 0 as winning.

## FFI surface

C bridge (`cpp/usd_bridge/usd_bridge.h`/`.cpp`):

| C API | USD call |
|---|---|
| `usd_bridge_stage_get_layer_stack` | `UsdStage::GetRootLayer` + recursive `SdfLayer::GetSubLayerPaths` |
| `usd_bridge_stage_get_edit_target` | `UsdStage::GetEditTarget` |
| `usd_bridge_stage_mute_layer` | `UsdStage::MuteLayer` / `UnmuteLayer` |
| `usd_bridge_prim_get_prim_stack` | `UsdPrim::GetPrimStack` |
| `usd_bridge_attr_get_opinion_sources` | `UsdAttribute::GetPropertyStack` + `TfStringify(VtValue)` |
| `usd_bridge_layer_get_offset` | `SdfLayerOffset` from root's `GetSubLayerOffsets` |
| `usd_bridge_open_stage_with_policy` | `UsdStage::Open(path, LoadAll \| LoadNone)` |

Matching Rust methods hang off `UsdStage` (`crates/bif_core/src/usd/cpp_bridge.rs`).

## UI integration

All rendered by `bif_viewport` against `self.scene.layer_state.as_ref()`. None of this coupling is egui-specific beyond the render calls themselves — Qt port (v0.15) replaces the panel layer wholesale.

- **Layer Stack panel** (`crates/bif_viewport/src/layer_stack_panel.rs`) — collapsing header at the top of the left sidebar. Indented tree; per-row color dot, working-layer radio, mute checkbox, strikethrough on muted, bold on working, authored-offset label when non-identity.
- **Scene browser color dots** — 3 px dot between type icon and prim name. Colors from `theme::LAYER_COLORS` indexed by `SceneLayerState::layer_for_prim`.
- **Composition Arcs section** in the property inspector's Attributes tab — `CollapsingHeader("Composition Arcs (N)")`, strongest-first, specifier chip (`def` / `over` / `class`), arrow marker on the winner.
- **Per-attribute opinion dot** — attributes whose `OpinionSource` stack has ≥2 entries get a color dot prefix in the Grid. Tooltip lists every contributing layer with its authored display value.

## Events

`bif_viewport::app_event::AppEvent` variants added in v0.14:

- `LayerSelected(usize)` — panel focus (UI-only)
- `LayerMuteToggled { index, muted }` — drives `UsdStage::set_layer_muted` + `SceneLayerState::set_muted`
- `WorkingLayerChanged(usize)` — drives `EditHistory.working_layer_id` in v0.16
- `PayloadPolicyChanged(PayloadPolicy)` — `LoadAll` / `LoadNone` only; richer policies are _Future_
- `IsolationModeToggled(bool)` — UI hint; full isolation behavior remains _Future_ work

## Gotchas

- **USD forbids muting the root layer.** `UsdStage::MuteLayer` emits `Coding Error: Cannot mute cache's root layer` and does nothing. The Layer Stack panel lets the user try, and `UsdStage::set_layer_muted` passes the call through. The panel then reads the unchanged `is_muted` flag on refresh and the UI self-corrects. Test `test_mute_layer_recomposes_opinions` captures this — it mutes a sublayer, not the root.
- **`finalize_usd_scene` does not store the full `Scene`.** It pulls individual fields (`mesh_data`, `instance_animations`, `materials`, `cameras`) into `self.scene.*` and drops the rest. That's why `SceneLayerState` lives on `SceneManager`, not on `bif_core::Scene` — an earlier attempt set it on the local `scene` binding and the write was silently dropped at end of function.
- **Value strings are display-only.** `TfStringify(VtValue)` is not round-trippable. Any future edit path must reach back through the value-typed API; v0.14 is purely read.

## In BIF

- **C++ bridge:** `cpp/usd_bridge/usd_bridge.cpp` (layer-aware block, ~L6290+)
- **Rust FFI:** `crates/bif_core/src/usd/ffi_raw.rs`, `crates/bif_core/src/usd/ffi_convert.rs`
- **Safe types:** `crates/bif_core/src/usd/layer.rs`
- **`UsdStage` methods:** `crates/bif_core/src/usd/cpp_bridge.rs` layer-aware impl block
- **Runtime state:** `crates/bif_core/src/scene_layer_state.rs`
- **Scene loader wiring:** `crates/bif_viewport/src/scene_loader.rs::finalize_usd_scene`
- **Panel:** `crates/bif_viewport/src/layer_stack_panel.rs`
- **Dispatch:** `crates/bif_viewport/src/render.rs::dispatch_events`
- **Fixture:** `test_assets/layers/{root,shot,anim}.usda`

## Related

- [[Composition Arcs]]
- [[SDF Foundations]]
- [[Stage Layer Prim]]
- [[adr/005-layer-aware-read-model|ADR-005 — Layer-aware read model]]
