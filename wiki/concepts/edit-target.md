---
title: "USD Edit Targets"
type: concept
tags: [usd, authoring, layers, composition]
created: "2026-04-05"
updated: "2026-04-05"
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

- **Current state**: BIF reads composed USD stages but writes flat exports -- no edit target support yet.
- **v0.14.0 milestone**: Layer-aware stage editing is the next major feature. This will require:
  - Exposing the layer stack in the UI
  - Letting users pick an edit target layer
  - Routing property edits through `SetEditTarget()` in the C++ bridge
- **Design doc**: `BIF_USD_WORKFLOW.md` describes the hybrid approach -- procedural nodes + layer awareness.
- **C++ bridge**: `bif_core::usd::cpp_bridge` will need `set_edit_target()` and `get_layer_stack()` FFI functions.
- References in `MILESTONES.md` and `docs/usd/composition.md` discuss the edit target roadmap.

## Related

- [[composition-arcs]] -- edit targets interact with LIVRPS strength ordering
- [[materialx]] -- material overrides authored to specific layers
- [[primvars]] -- primvar values can be overridden on different layers
