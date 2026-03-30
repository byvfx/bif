# Sdf — Scene Description Foundations

## Overview

Sdf provides the low-level data model for USD scene description. While most work should use the higher-level `Usd` API (which handles composition), Sdf is essential for:
- Direct layer manipulation (creating/editing specs without a stage)
- Understanding how data is stored before composition
- Working with file format plugins and asset resolution

## SdfLayer

A container for scene description — either file-backed or in-memory.

### Creation and Access

```python
# Create new file-backed layer
layer = Sdf.Layer.CreateNew('output.usda')

# Open existing (cached by identifier)
layer = Sdf.Layer.FindOrOpen('scene.usd')

# Find already-opened layer (returns None if not cached)
layer = Sdf.Layer.Find('scene.usd')

# In-memory only (cannot Save(), must Export())
layer = Sdf.Layer.CreateAnonymous('tag')
```

### Saving and Exporting

```python
layer.Save()                    # Save to its own path (file-backed only)
layer.Export('output.usdc')     # Export to a different file/format
layer.ExportToString()          # Get text representation
layer.ImportFromString(usda)    # Load from text
```

### Layer Properties

```python
layer.identifier         # File path or anonymous identifier
layer.realPath           # Resolved file system path
layer.anonymous          # True if in-memory only
layer.dirty              # True if modified since last save
layer.defaultPrim        # Name of the default root prim
layer.subLayerPaths      # List of sublayer paths
layer.startTimeCode      # Animation start
layer.endTimeCode        # Animation end
layer.timeCodesPerSecond # Frame rate
layer.framePrecision     # Decimal precision for time codes
```

### Layer Registry

- Sdf maintains an internal registry (cache) of opened layers
- Registry holds **weak references** — client must retain the strong ref
- `FindOrOpen()` returns existing cached layer if already open
- UsdStage retains all layers reached through composition automatically

### SubLayers

```python
layer.subLayerPaths.append('./overrides.usd')
layer.subLayerPaths.insert(0, './strongest.usd')  # Insert at strongest position
# With time offset
layer.subLayerOffsets[0] = Sdf.LayerOffset(offset=10, scale=1.0)
```

## SdfPath

Compact, thread-safe identifier for scene description locations. Used as keys throughout USD.

### Path Syntax

| Example | Type |
|---------|------|
| `/` | Absolute root |
| `/Root/Child/Grandchild` | Absolute prim path |
| `/Root/Child.visibility` | Property path |
| `/Root/Child.xformOp:translate` | Namespaced property |
| `/Root{variantSet=selection}/Child` | Variant selection path |
| `/Root.rel[/Target]` | Relationship target path |

### Construction and Navigation

```python
path = Sdf.Path('/World/Geo/Mesh')

path.GetParentPath()        # /World/Geo
path.AppendChild('SubMesh') # /World/Geo/Mesh/SubMesh
path.AppendProperty('points') # /World/Geo/Mesh.points
path.GetName()              # 'Mesh'
path.GetPrimPath()          # /World/Geo/Mesh (strips property)

path.IsAbsolutePath()       # True
path.IsPrimPath()           # True
path.IsPropertyPath()       # False
path.IsEmpty()              # False

# Path prefixes (ancestors)
path.GetPrefixes()          # [/World, /World/Geo, /World/Geo/Mesh]

# String conversion
str(path)                   # '/World/Geo/Mesh'
Sdf.Path('/World/Geo')      # From string
```

### Special Paths

```python
Sdf.Path.absoluteRootPath   # /
Sdf.Path.emptyPath          # Empty/invalid path
Sdf.Path.reflexiveRelativePath  # . (self)
```

## SdfPrimSpec

Low-level prim description within a layer (before composition):

```python
# Create a prim spec
primSpec = Sdf.PrimSpec(layer, 'MyPrim', Sdf.SpecifierDef)
primSpec = Sdf.PrimSpec(layer, 'MyOver', Sdf.SpecifierOver)
primSpec = Sdf.PrimSpec(layer, 'MyClass', Sdf.SpecifierClass)

# Set type
primSpec.typeName = 'Mesh'

# Add child prim
childSpec = Sdf.PrimSpec(primSpec, 'Child', Sdf.SpecifierDef)

# Access via layer
primSpec = layer.GetPrimAtPath(Sdf.Path('/MyPrim'))
```

### Specifiers

| Value | Keyword | Description |
|-------|---------|-------------|
| `Sdf.SpecifierDef` | `def` | Concrete prim definition |
| `Sdf.SpecifierOver` | `over` | Override opinions |
| `Sdf.SpecifierClass` | `class` | Abstract class |

## SdfAttributeSpec / SdfRelationshipSpec

Low-level property specs:

```python
# Create attribute spec
attrSpec = Sdf.AttributeSpec(primSpec, 'myAttr', Sdf.ValueTypeNames.Float)
attrSpec.default = 1.0

# Create relationship spec
relSpec = Sdf.RelationshipSpec(primSpec, 'myRel')
relSpec.targetPathList.explicitItems = [Sdf.Path('/Target')]
```

## File Format Plugins

Sdf's `SdfFileFormat` plugin mechanism allows:
- **Translating** other formats into USD on-the-fly (e.g., Alembic → USD)
- **Procedural generation** — a plugin that generates scene description dynamically
- Extension-based registry: `.abc` → AlembicFileFormat, `.usd`/`.usda`/`.usdc` → native

## Asset Resolution (Ar)

USD uses the `Ar` (Asset Resolution) library to resolve asset identifiers to actual file paths:

```python
from pxr import Ar
resolver = Ar.GetResolver()
resolved = resolver.Resolve('asset.usd')  # Returns resolved path
```

Pluggable: studios can provide custom resolvers for asset management systems.
