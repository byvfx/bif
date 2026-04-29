---
title: ADR-008 — Edit operation architecture (v0.16 C4a)
type: adr
status: accepted
tags: [architecture, usd, layers, editing, v0.16]
created: 2026-04-26
updated: 2026-04-26
---

# ADR-008 — Edit operation architecture (v0.16 C4a)

## Status

Accepted for the v0.16 C4a edit/save foundation.

## Context

The v0.16 audit found four blocking issues before C4 feature work:

- BIF had two edit models with no bridge: procedural `EditState` / `UndoStack`, and direct USD FFI calls.
- Variant selection wrote through the stage's current edit target instead of the selected working layer.
- `LayerInfo.is_dirty` existed but real USD writes did not flip it.
- `on_save` was still a status-bar stub.

The goal of C4a is a foundation, not the full material/USDA editing UI.

## Decisions

### D1 — Keep Parallel Undo Stacks

Procedural node edits remain in `EditState` / `UndoStack`. Authored USD opinions use `EditHistory`. The viewport owns a small action router that records whether the most recent action was procedural or USD-backed.

### D2 — Use Dual-Track Identity

The viewport keeps `instance_index` for picking and interactive transforms. `EditHistory` stores USD identity as `(SdfPath, AttrSlot)`. Translation happens at the viewport dispatch boundary.

### D3 — Extend SceneLayerState

The active edit target lives on `SceneLayerState`. It now owns `EditHistory` and keeps `edit_history.working_layer_id` in sync with `working_layer`.

Rust does not cache `SdfLayer*`. Every write resolves `layer_id -> SdfLayer` C++-side for the current stage.

### D4 — Composition Arc Edits Are EditOperations

Variant selections are represented as `EditOperation::VariantSelect`. Future payload/reference edits should follow the same pattern.

### D5 — Prefer Soft Conflict Warnings

If a stronger layer than the working layer already wins an opinion, BIF should warn but not block. C4a lays the substrate; richer conflict UI is later work.

## Consequences

- Undo is more complex in the short term, but procedural graph work is not forced through USD before the authoring model is ready.
- The instance-to-SdfPath boundary is explicit and testable.
- Stage reload discards `EditHistory` in v0.16; re-resolving history across reloads is deferred.
- Ctrl+S saves one working layer, not the whole stack.
- Variant edits now land on the selected working layer through `UsdEditContext`.

## Validation

C4a adds round-trip tests for transform, visibility, material binding, material parameter, and variant selection edits. Each test writes an opinion to the working layer, saves, reopens the stage, and verifies the opinion persists.
