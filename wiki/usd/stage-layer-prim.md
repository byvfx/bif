---
title: "Stage, Layer, Prim"
type: article
tags: [usd]
created: "2026-04-05"
updated: "2026-04-05"
sources: [../../docs/usd/concepts.md, ../../docs/usd/sdf-foundations.md, ../../docs/usd/faq.md]
---

# Stage, Layer, Prim

USD organizes scene description into a hierarchy of **prims** (primitives) containing **properties** (attributes + relationships), stored in **layers**, and presented as a composed **stage**.

## Stage (UsdStage)

The outermost container. Opens a root layer, recursively composes all referenced/sublayered layers via the composition engine, and presents the result as a single scenegraph.

```python
stage = Usd.Stage.Open('scene.usd')      # Open existing
stage = Usd.Stage.CreateNew('scene.usda') # Create new
stage.Save()                               # Save all dirty non-session layers
```

Key operations:

- `GetPrimAtPath(SdfPath)` -- retrieve a composed prim
- `Traverse()` -- depth-first iteration over active, defined, loaded, concrete prims
- `GetRootLayer()` / `GetSessionLayer()` -- access underlying layers
- `SetEditTarget(layer)` -- direct authoring to a specific layer
- `GetDefaultPrim()` / `SetDefaultPrim(prim)` -- the prim used when referencing without explicit path
- `Load()` / `Unload()` -- control payload loading

### Traversal

```python
# Default: active, defined, loaded, concrete prims
for prim in stage.Traverse():
    print(prim.GetPath())

# With custom predicate
predicate = Usd.PrimIsDefined & Usd.PrimIsActive & ~Usd.PrimIsAbstract
for prim in stage.Traverse(predicate):
    process(prim)
```

## Layer (SdfLayer)

A persistent (file) or in-memory (anonymous) container of scene description. Layers hold **specs** (PrimSpecs, PropertySpecs) -- the raw, uncomposed opinions.

```python
layer = Sdf.Layer.FindOrOpen('asset.usd')   # Cached by identifier
layer = Sdf.Layer.CreateNew('new.usda')
layer = Sdf.Layer.CreateAnonymous()          # In-memory only
layer.Save()
layer.Export('output.usdc')                  # Save to different format
```

Key concepts:

- Layers are **cached** by identifier -- `FindOrOpen` returns existing if already open
- Client must retain the `SdfLayerRefPtr`; the registry holds only weak refs
- Layers reached by composition arcs are retained by the stage automatically
- **Anonymous layers** cannot be `Save()`d -- must `Export()` instead

### Layer Properties

| Property | Description |
|----------|-------------|
| `identifier` | File path or anonymous identifier |
| `realPath` | Resolved file system path |
| `anonymous` | True if in-memory only |
| `dirty` | True if modified since last save |
| `defaultPrim` | Name of the default root prim |
| `subLayerPaths` | List of sublayer paths |
| `startTimeCode` / `endTimeCode` | Animation range |
| `timeCodesPerSecond` | Frame rate |

### Session Layer

Sits above the root layer in composition strength. Used for transient, non-persistent opinions (viewport overrides, selection state). Not saved with `stage.Save()`.

### Anonymous Layers

In-memory-only layers with no file backing. Used for:

- Session layers (temporary overrides during interactive editing)
- Procedurally generated scene description
- Scratch layers for undo/redo systems

## Prim (UsdPrim)

The sole persistent scenegraph object on a stage. Contains properties and child prims.

```python
prim = stage.GetPrimAtPath('/World/Mesh')
prim.GetTypeName()    # e.g. 'Mesh'
prim.GetChildren()    # child prims
prim.GetProperties()  # all properties
prim.GetAttribute('points')
prim.GetRelationship('material:binding')
```

### Prim Specifiers: def vs over vs class

| Specifier | Purpose | Appears in Traversal? |
|-----------|---------|----------------------|
| `def` | **Define** a concrete prim | Yes |
| `over` | **Override** -- speculative opinions if concrete prim exists | No (unless def exists elsewhere) |
| `class` | **Abstract** -- never traversed, used as inherit/specialize target | No |

### Active / Inactive

- `prim.SetActive(False)` -- non-destructive deletion; prim and descendants won't compose
- Can be overridden in stronger layers to re-activate

## Properties

### Attributes (UsdAttribute)

- Has a typed value that can vary over time
- Value sources: default value, time samples, or connections
- `attr.Set(value)` / `attr.Set(value, timeCode)` / `attr.Get()`

### Relationships (UsdRelationship)

- Multi-target pointer to other prims/properties
- Targets are automatically remapped when namespaces change via composition
- Primary use: material bindings, collection membership

## Value Resolution

When reading an attribute value, USD resolves through composition in strength order:

1. **Time samples** (strongest animated source wins)
2. **Default value** (if no time samples)
3. **Fallback value** from schema (if nothing authored)

**Key rule:** The first (strongest) layer with *any* time sample for an attribute provides *all* time samples. Composition does not merge time samples across layers.

## Metadata

Non-time-varying data on prims, properties, or layers:

- `active`, `hidden`, `documentation`, `comment`
- `kind` (model hierarchy classification)
- `customData` (arbitrary user dictionary)
- `assetInfo` (identifier, name, version)

## Model Hierarchy

USD defines a **kind** taxonomy for organizing scenes:

| Kind | Description |
|------|-------------|
| `model` | Base -- everything in model hierarchy |
| `group` | Container for other models |
| `assembly` | Important group (usually a published asset) |
| `component` | Terminal model -- no child models allowed |
| `subcomponent` | Articulable sub-parts within a component |

Model hierarchy enables fast scene navigation -- traverse only to component level, then expand on demand.

## Instancing

**Native Instancing:** Multiple prims share identical subtrees. Mark with `instanceable = true`. USD deduplicates the composed subtree into a shared prototype. Read-only beneath instance roots.

**Point Instancing:** `UsdGeomPointInstancer` -- vectorized placement of prototypes at many positions/orientations/scales. Far more scalable for large instance counts (thousands+).

## SdfPath

Compact, thread-safe identifier for scene description locations:

| Example | Type |
|---------|------|
| `/` | Absolute root |
| `/Root/Child/Grandchild` | Absolute prim path |
| `/Root/Child.visibility` | Property path |
| `/Root{variantSet=selection}/Child` | Variant selection path |

```python
path = Sdf.Path('/World/Geo/Mesh')
path.GetParentPath()        # /World/Geo
path.AppendChild('SubMesh') # /World/Geo/Mesh/SubMesh
path.GetName()              # 'Mesh'
path.GetPrefixes()          # [/World, /World/Geo, /World/Geo/Mesh]
```

## File Formats

| Extension | Format | Use |
|-----------|--------|-----|
| `.usda` | Text (ASCII) | Human-readable, debugging |
| `.usdc` | Binary (crate) | Production -- fast, compact, memory-mappable |
| `.usd` | Either | Determined by content (best practice for references) |
| `.usdz` | Zip archive | Delivery -- bundles USD + textures |

## How BIF Uses These Concepts

- BIF opens a **master stage** as the root, showing the full layer stack
- The **scene browser** (CompositeProvider) merges USD stage + procedural prims via CachedSceneGraph
- **Layer isolation mode** lets artists pick their working layer; all edits target that layer via `SetEditTarget`
- The C++ bridge (`bif_core/src/usd/cpp_bridge.rs`) handles all SdfLayer/UsdStage operations

## Related

- [[composition-arcs|Composition Arcs]] -- how layers are composed together
- [[geometry-schemas|Geometry Schemas]] -- UsdGeomMesh, transforms, point instancers
- [[shading-materials|Shading and Materials]] -- UsdShade material system
- [[bif-usd-integration|BIF USD Integration]] -- BIF's hybrid layer-aware workflow
