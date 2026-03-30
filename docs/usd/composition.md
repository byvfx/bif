# USD Composition

## LIVRPS Strength Ordering

When resolving opinions for a prim, USD evaluates composition arcs in this order (strongest to weakest), **within each LayerStack**:

1. **L — Local** opinions in the LayerStack (root layer + sublayers)
2. **I — Inherits** — class-based sharing (like OOP inheritance)
3. **V — VariantSets** — selected variant opinions
4. **R — References** — asset composition
5. **P — Payloads** — deferred references (loadable/unloadable)
6. **S — Specializes** — like inherits but weaker than references

Each arc target is itself resolved recursively through LIVRPS.

> Mnemonic: **L**ocal **I**nherits **V**ariants **R**eferences **P**ayloads **S**pecializes

## SubLayers

Stack layers together — like Photoshop layers. Opinions in earlier (stronger) sublayers win.

```usda
#usda 1.0
(
    subLayers = [
        @./overrides.usd@,    # strongest sublayer
        @./base_layout.usd@   # weakest sublayer
    ]
)
```

- SubLayers form a **LayerStack** — the ordered set of layers that compose as one unit
- Each sublayer can have its own sublayers (nestable)
- Can apply `SdfLayerOffset` (time offset + scale) to sublayers

## References

Primary mechanism for assembling assets. Compose a target prim's subtree into the referencing prim.

```usda
def "MyChar" (
    prepend references = @./char.usd@</Character>
)
{
    # Overrides go here — they're Local, stronger than the reference
    double3 xformOp:translate = (10, 0, 0)
}
```

```python
prim.GetReferences().AddReference('./char.usd', '/Character')
prim.GetReferences().AddReference('./char.usd')  # uses defaultPrim
prim.GetReferences().AddInternalReference('/OtherPrim')  # same stage
```

Key behaviors:
- If no target prim path specified, uses the referenced layer's `defaultPrim`
- Multiple references on one prim: `prepend` (stronger) vs `append` (weaker)
- Opinions in the referencing layer (Local) are stronger than referenced opinions
- Namespace of referenced subtree is remapped to the referencing prim's path

## Payloads

Like references, but **deferred** — not loaded unless explicitly requested. Critical for scalability.

```usda
def "HeavyAsset" (
    prepend payload = @./heavy_geo.usd@</Geo>
)
{
}
```

```python
# Loading control
stage = Usd.Stage.Open('scene.usd', Usd.Stage.LoadNone)  # Don't load payloads
stage.Load('/HeavyAsset')   # Load specific prim's payload
stage.Unload('/HeavyAsset') # Unload it
```

Best practice: Put heavy geometry/shading behind payloads; keep lightweight structure (transforms, metadata, material assignments) in references.

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

```python
vset = prim.GetVariantSets().AddVariantSet('color')
vset.AddVariant('red')
vset.SetVariantSelection('red')
with vset.GetVariantEditContext():
    # Author opinions inside the selected variant
    attr.Set(value)
```

## Inherits

Class-based opinion sharing. All prims that inherit from a class automatically receive its opinions (weaker than local, stronger than variants/references).

```usda
class "_TreeClass"
{
    color3f[] primvars:displayColor = [(0.1, 0.5, 0.1)]
}

def "Tree1" (
    prepend inherits = </_TreeClass>
)
{
}

def "Tree2" (
    prepend inherits = </_TreeClass>
)
{
    # Override: local wins over inherited
    color3f[] primvars:displayColor = [(0.8, 0.6, 0.1)]
}
```

Key: Editing the class automatically propagates to all inheriting prims (if no local override).

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

# Author inside a variant
vset = prim.GetVariantSet('color')
vset.SetVariantSelection('red')
with vset.GetVariantEditContext():
    prim.GetAttribute('color').Set(Gf.Vec3f(1,0,0))
```

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

## Composition Gotchas

1. **Time samples don't merge across layers** — strongest layer with any sample wins all samples
2. **Local opinions beat everything** — clear local opinions if you want variants/references to show through
3. **over vs def** — `over` contributes opinions only if a `def` exists at that path from another source
4. **Instancing limits editing** — can't directly edit beneath an instanceable prim; use composition arcs
5. **defaultPrim is per-layer** — always set it on publishable assets
6. **prepend vs append** — `prepend` is stronger, `append` is weaker; default is `prepend`
