---
title: "USD Edit Targets"
type: concept
tags: [usd, authoring, layers, composition]
created: "2026-04-05"
updated: "2026-05-01"
---

## Summary

An edit target in USD tells the stage *which layer* should receive new opinions when you author changes. By default, edits go to the root layer, but in a multi-layer pipeline you often want to direct edits to a specific sublayer (e.g., an animation layer, a lighting override layer). The edit target is the mechanism that makes non-destructive, layer-based workflows possible.

## Details

### How It Works

```cpp
stage.SetEditTarget(UsdEditTarget(layer))
```

After setting the edit target, any authoring operation (setting attributes, creating prims, adding relationships) writes opinions to the targeted layer's PrimSpec, not the root layer.

### Layer Stack Context

A USD stage is composed from a **layer stack** -- an ordered list of sublayers. Each sublayer can contain opinions about any prim. The [[composition-arcs]] (LIVRPS) determine which opinion wins, but the edit target determines where *new* opinions are stored.

### Common Patterns

| Pattern | Edit Target | Purpose |
|---------|-------------|---------|
| Base asset authoring | Root layer | Define the canonical asset |
| Animation override | Anim sublayer | Non-destructive anim edits |
| Shot lighting | Lighting sublayer | Per-shot light tweaks |
| Department overhaul | Department layer | Isolated department changes |

### Pitfalls

- Setting an edit target to a **referenced layer** requires mapping paths through the reference arc. `UsdEditTarget` can take a `UsdEditTarget::ForLocalDirectVariant()` or a mapping function.
- Forgetting to reset the edit target is a common source of "where did my edit go?" bugs.
- Opinions authored on a weaker layer can be invisible if a stronger layer already has an opinion.

## In BIF

- **Current state (v0.16):** Working-layer edits are wired. Interactive viewport actions (Transform, Visibility, MaterialAssign, MaterialParamOverride, VariantSelect, ReplaceLayerContents, SetShaderId) route through `EditOperation` / `EditHistory` and write under `UsdEditContext(stage, working_layer)` in the C++ bridge. Ctrl+S saves the active working layer.
- **Procedural nodes** (Scatter, PointInstancer, etc.) still emit USD only at `export_scene()` time — node-driven opinion authoring is _Future_ work.
- **Design doc:** `BIF_USD_WORKFLOW.md` describes the hybrid approach — procedural nodes + layer awareness.
- **C++ bridge:** `cpp/usd_bridge/usd_bridge.cpp` exposes `UsdEditContext`-scoped writes via the FFI layer in `crates/bif_core/src/usd/cpp_bridge.rs`.
- **Architecture:** see [[adr/008-edit-operation-architecture|ADR 008]] for the dual-track identity and parallel undo decisions.

## Related

- [[composition-arcs]] -- edit targets interact with LIVRPS strength ordering
- [[materialx]] -- material overrides authored to specific layers
- [[primvars]] -- primvar values can be overridden on different layers
