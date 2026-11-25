---
title: "Composition Arcs"
type: article
tags: [usd]
created: "2026-04-05"
updated: "2026-04-05"
sources: [../../docs/usd/composition.md, ../../docs/usd/concepts.md]
---

# Composition Arcs

USD's composition engine weaves multiple layers and files into a single scenegraph. The mechanism for combining scene description from different sources is called **composition arcs**. Understanding composition is essential for working with USD at any scale.

## LIVRPS Strength Ordering

When resolving opinions for a prim, USD evaluates composition arcs in this fixed order (strongest to weakest), within each LayerStack:

| # | Arc | Strength | Purpose |
|---|-----|----------|---------|
| 1 | **L** - Local | Strongest | Direct opinions in the LayerStack (root + sublayers) |
| 2 | **I** - Inherits | | Class-based sharing (like OOP inheritance) |
| 3 | **V** - VariantSets | | Selected variant opinions |
| 4 | **R** - References | | Asset composition |
| 5 | **P** - Payloads | | Deferred references (loadable/unloadable) |
| 6 | **S** - Specializes | Weakest | Like inherits but weaker than references |

Each arc target is itself resolved recursively through LIVRPS.

> Mnemonic: **L**ocal **I**nherits **V**ariants **R**eferences **P**ayloads **S**pecializes

## SubLayers

SubLayers stack layers together like Photoshop layers. Opinions in earlier (stronger) sublayers win.

```usda
#usda 1.0
(
    subLayers = [
        @./overrides.usd@,    # strongest sublayer
        @./base_layout.usd@   # weakest sublayer
    ]
)
```

Key properties:

- SubLayers form a **LayerStack** -- the ordered set of layers that compose as one unit
- Each sublayer can have its own sublayers (nestable)
- `SdfLayerOffset` (time offset + scale) can be applied to sublayers for time remapping

## References

The primary mechanism for assembling assets. Compose a target prim's subtree into the referencing prim.

```usda
def "MyChar" (
    prepend references = @./char.usd@</Character>
)
{
    # Overrides go here -- they're Local, stronger than the reference
    double3 xformOp:translate = (10, 0, 0)
}
```

Key behaviors:

- If no target prim path is specified, uses the referenced layer's `defaultPrim`
- Multiple references on one prim: `prepend` (stronger) vs `append` (weaker)
- Opinions in the referencing layer (Local) are always stronger than referenced opinions
- Namespace of referenced subtree is remapped to the referencing prim's path

## Payloads

Like references but **deferred** -- not loaded unless explicitly requested. Critical for scalability in large scenes.

```usda
def "HeavyAsset" (
    prepend payload = @./heavy_geo.usd@</Geo>
)
{
}
```

Loading control:

```python
stage = Usd.Stage.Open('scene.usd', Usd.Stage.LoadNone)  # Don't load payloads
stage.Load('/HeavyAsset')   # Load specific prim's payload
stage.Unload('/HeavyAsset') # Unload it
```

**Best practice:** Put heavy geometry/shading behind payloads; keep lightweight structure (transforms, metadata, material assignments) in references.

## VariantSets

Package multiple variations in a single asset. Downstream consumers select which variant is active.

```usda
def "Car" (
    prepend variantSets = "color"
    variants = {
        string color = "red"
    }
)
{
    variantSet "color" = {
        "red" {
            color3f[] primvars:displayColor = [(1, 0, 0)]
        }
        "blue" {
            color3f[] primvars:displayColor = [(0, 0, 1)]
        }
    }
}
```

Useful for LOD switching, material variations, geometry alternatives, and any switchable asset property.

## Inherits

Class-based opinion sharing. All prims that inherit from a class automatically receive its opinions (weaker than local, stronger than variants/references).

```usda
class "_TreeClass"
{
    color3f[] primvars:displayColor = [(0.1, 0.5, 0.1)]
}

def "Tree1" (
    prepend inherits = </_TreeClass>
) { }

def "Tree2" (
    prepend inherits = </_TreeClass>
)
{
    # Override: local wins over inherited
    color3f[] primvars:displayColor = [(0.8, 0.6, 0.1)]
}
```

Editing the class automatically propagates to all inheriting prims (if no local override exists).

## Specializes

Like inherits but **weaker than references**. Useful when you want a base definition that referenced assets can override.

```usda
class "_BaseMaterial" { ... }

def Material "ConcreteMaterial" (
    prepend specializes = </_BaseMaterial>
)
{
    # Referenced opinions beat specialized opinions
}
```

## Edit Targets

Control which layer receives authored opinions:

```python
stage.SetEditTarget(stage.GetRootLayer())        # Default
stage.SetEditTarget(stage.GetSessionLayer())      # Session-only edits
stage.SetEditTarget(Usd.EditTarget(some_layer))   # Specific layer
```

This is fundamental to BIF's layer-aware editing workflow -- the artist picks their working layer and all edits are directed there.

## Layer Offsets

Apply time remapping when sublayering or referencing:

```usda
(
    subLayers = [
        @./anim.usd@ (offset = 10; scale = 2.0)
    ]
)
```

- `offset`: shift time samples by N frames
- `scale`: stretch/compress time (scale=2 means half speed)

## Gotchas

1. **Time samples don't merge across layers** -- strongest layer with any sample wins all samples
2. **Local opinions beat everything** -- clear local opinions if you want variants/references to show through
3. **over vs def** -- `over` contributes opinions only if a `def` exists at that path from another source
4. **Instancing limits editing** -- can't directly edit beneath an instanceable prim; use composition arcs
5. **defaultPrim is per-layer** -- always set it on publishable assets
6. **prepend vs append** -- `prepend` is stronger (default), `append` is weaker

## How BIF Uses Composition

BIF's hybrid workflow relies heavily on composition arcs:

- **SubLayers** form the layer stack for shot assembly (layout, animation, FX, lighting layers)
- **References/Payloads** bring in published assets with selective loading
- **Edit targets** direct all artist edits to their specific working layer
- **VariantSets** enable asset variation switching in the scene browser

See [[bif-usd-integration|BIF USD Integration]] for the full workflow.

## Related

- [[stage-layer-prim|Stage, Layer, Prim]] -- core USD object model
- [[shading-materials|Shading and Materials]] -- material bindings use composition for overrides
- [[bif-usd-integration|BIF USD Integration]] -- how BIF leverages composition in its workflow
