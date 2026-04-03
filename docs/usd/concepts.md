# USD Core Concepts

## Object Model

USD organizes scene description into a hierarchy of **prims** (primitives) containing **properties** (attributes + relationships), stored in **layers**, and presented as a composed **stage**.

### Stage (UsdStage)

The outermost container. Opens a root layer, recursively composes all referenced/sublayered layers via the composition engine, and presents the result as a single scenegraph.

```text
# Python
stage = Usd.Stage.Open('scene.usd')      # Open existing
stage = Usd.Stage.CreateNew('scene.usda') # Create new
stage.Save()                               # Save all dirty non-session layers
```

Key operations:

- `GetPrimAtPath(SdfPath)` — retrieve a composed prim
- `Traverse()` — depth-first iteration over active, defined, loaded, concrete prims
- `GetRootLayer()` / `GetSessionLayer()` — access underlying layers
- `SetEditTarget(layer)` — direct authoring to a specific layer
- `GetDefaultPrim()` / `SetDefaultPrim(prim)` — the prim used when referencing without explicit path
- `Load()` / `Unload()` — control payload loading

### Layer (SdfLayer)

A persistent (file) or in-memory (anonymous) container of scene description. Layers hold **specs** (PrimSpecs, PropertySpecs) — the raw, uncomposed opinions.

```text
layer = Sdf.Layer.FindOrOpen('asset.usd')
layer = Sdf.Layer.CreateNew('new.usda')
layer = Sdf.Layer.CreateAnonymous()        # In-memory only
layer.Save()
layer.Export('output.usdc')                 # Save to different file/format
```

Key concepts:

- Layers are **cached** by identifier — `FindOrOpen` returns existing if already open
- Client must retain the `SdfLayerRefPtr`; the registry holds only weak refs
- Layers reached by composition arcs are retained by the stage automatically
- **Anonymous layers** cannot be `Save()`d — must `Export()` instead

### Prim (UsdPrim)

The sole persistent scenegraph object on a stage. Contains properties and child prims.

```text
prim = stage.GetPrimAtPath('/World/Mesh')
prim.GetTypeName()    # e.g. 'Mesh'
prim.GetChildren()    # child prims
prim.GetProperties()  # all properties
prim.GetAttribute('points')
prim.GetRelationship('material:binding')
```

#### Prim Specifiers: def vs over vs class

| Specifier | Purpose | Appears in Traversal? |
|-----------|---------|----------------------|
| `def` | **Define** a concrete prim | Yes |
| `over` | **Override** — speculative opinions applied if a concrete prim exists at this path | No (unless a def exists elsewhere) |
| `class` | **Abstract** prim — never traversed, used as inherit/specialize target | No |

#### Active / Inactive

- `prim.SetActive(False)` — non-destructive deletion; prim and descendants won't compose
- Can be overridden in stronger layers to re-activate

### Property (UsdProperty)

Base class for Attributes and Relationships.

**Attribute (UsdAttribute):**

- Has a typed value (see datatypes.md) that can vary over time
- Value sources: default value, time samples, or connections
- `attr.Set(value)` / `attr.Set(value, timeCode)` / `attr.Get()`
- Created via schema API or `prim.CreateAttribute(name, typeName)`

**Relationship (UsdRelationship):**

- Multi-target pointer to other prims/properties
- Targets are automatically remapped when namespaces change via composition
- `rel.SetTargets([path1, path2])` / `rel.GetTargets()`
- Primary use: material bindings, collection membership

### Metadata

Non-time-varying data on prims, properties, or layers. Examples:

- `active`, `hidden`, `documentation`, `comment`
- `kind` (model hierarchy classification)
- `customData` (arbitrary user dictionary)
- `assetInfo` (identifier, name, version)

## Value Resolution

When reading an attribute value, USD resolves through the composition in strength order:

1. Time samples (strongest animated source wins)
2. Default value (if no time samples)
3. Fallback value from schema (if nothing authored)

**Key rule:** The first (strongest) layer with *any* time sample for an attribute provides *all* time samples. Composition does not merge time samples across layers.

## Model Hierarchy

USD defines a **kind** taxonomy for organizing scenes:

| Kind | Description |
|------|-------------|
| `model` | Base — everything in model hierarchy |
| `group` | Container for other models |
| `assembly` | Important group (usually a published asset) |
| `component` | Terminal model — no child models allowed |
| `subcomponent` | Articulable sub-parts within a component |

```text
Usd.ModelAPI(prim).SetKind(Kind.Tokens.component)
Usd.ModelAPI(prim).GetKind()
```

Model hierarchy enables fast scene navigation — traverse only to component level, then expand on demand.

## Instancing

**Native Instancing:** Multiple prims share identical subtrees. Mark with `instanceable = true`. USD deduplicates the composed subtree into a shared prototype. Read-only beneath instance roots (override via composition, not direct edit).

**Point Instancing:** `UsdGeomPointInstancer` — vectorized placement of prototypes at many positions/orientations/scales. Far more scalable for large instance counts (thousands+).

## Stage Traversal

```python
# Default: active, defined, loaded, concrete prims
for prim in stage.Traverse():
    print(prim.GetPath())

# With custom predicate
predicate = Usd.TraverseInstanceProxies()
for prim in stage.Traverse(predicate):
    ...

# Pre-and-post order
it = iter(Usd.PrimRange.PreAndPostVisit(stage.GetPseudoRoot()))
for prim in it:
    is_post = it.IsPostVisit()
```
