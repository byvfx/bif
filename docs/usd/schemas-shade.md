# UsdShade — Shading Schemas

## Overview

UsdShade provides schemas for creating shading networks and materials. Key classes:

| Class | Purpose |
|-------|---------|
| `UsdShadeMaterial` | Container for render-context-specific shading networks |
| `UsdShadeShader` | Individual shading node (surface, texture, pattern, etc.) |
| `UsdShadeNodeGraph` | Container for reusable shader sub-networks |
| `UsdShadeInput` / `UsdShadeOutput` | Connectable attribute schemas on shaders/materials |
| `UsdShadeConnectableAPI` | API for creating connections between inputs/outputs |
| `UsdShadeMaterialBindingAPI` | API for binding materials to geometry |

## Creating a Material

```python
from pxr import Usd, UsdShade, Sdf

stage = Usd.Stage.CreateNew('material.usda')
material = UsdShade.Material.Define(stage, '/Model/Materials/MyMaterial')
```

```usda
def Material "MyMaterial"
{
}
```

## Creating Shaders

```python
shader = UsdShade.Shader.Define(stage, '/Model/Materials/MyMaterial/PBR')
shader.CreateIdAttr('UsdPreviewSurface')

# Set inputs
shader.CreateInput('diffuseColor', Sdf.ValueTypeNames.Color3f).Set((0.8, 0.2, 0.1))
shader.CreateInput('roughness', Sdf.ValueTypeNames.Float).Set(0.4)
shader.CreateInput('metallic', Sdf.ValueTypeNames.Float).Set(0.0)

# Create output
shader.CreateOutput('surface', Sdf.ValueTypeNames.Token)

# Connect material to shader output
material.CreateSurfaceOutput().ConnectToSource(shader.ConnectableAPI(), 'surface')
```

```usda
def Shader "PBR" {
    uniform token info:id = "UsdPreviewSurface"
    color3f inputs:diffuseColor = (0.8, 0.2, 0.1)
    float inputs:roughness = 0.4
    float inputs:metallic = 0.0
    token outputs:surface
}
```

## Shader Connections

Connections link outputs to inputs, creating dataflow networks:

```python
# Connect texture output to shader input
shader.CreateInput('diffuseColor', Sdf.ValueTypeNames.Color3f).ConnectToSource(
    textureShader.ConnectableAPI(), 'rgb'
)
```

**Rule:** Valid shader connections win over authored input values. If an input has both a connection and a direct value, the connected value is used.

## Texturing Pattern

```python
# 1. PrimvarReader to fetch texture coordinates
stReader = UsdShade.Shader.Define(stage, '/Mat/stReader')
stReader.CreateIdAttr('UsdPrimvarReader_float2')
stReader.CreateInput('varname', Sdf.ValueTypeNames.Token).Set('st')
stReader.CreateOutput('result', Sdf.ValueTypeNames.Float2)

# 2. Texture sampler
texture = UsdShade.Shader.Define(stage, '/Mat/diffuseTexture')
texture.CreateIdAttr('UsdUVTexture')
texture.CreateInput('file', Sdf.ValueTypeNames.Asset).Set('albedo.png')
texture.CreateInput('st', Sdf.ValueTypeNames.Float2).ConnectToSource(
    stReader.ConnectableAPI(), 'result'
)
texture.CreateOutput('rgb', Sdf.ValueTypeNames.Float3)

# 3. Connect to surface shader
pbrShader.CreateInput('diffuseColor', Sdf.ValueTypeNames.Color3f).ConnectToSource(
    texture.ConnectableAPI(), 'rgb'
)
```

## Interface Inputs (Public Parameters)

Materials can expose "public" inputs that downstream consumers can set, which propagate to internal shader inputs via connections:

```python
# Create interface input on material
stInput = material.CreateInput('frame:stPrimvarName', Sdf.ValueTypeNames.Token)
stInput.Set('st')

# Connect internal shader to interface
stReader.CreateInput('varname', Sdf.ValueTypeNames.Token).ConnectToSource(
    material.ConnectableAPI(), 'frame:stPrimvarName'
)
```

This lets consumers change the primvar name without editing the shader network internals.

## NodeGraph (Sub-networks)

Reusable shader groups with their own interface inputs/outputs:

```python
nodeGraph = UsdShade.NodeGraph.Define(stage, '/Mat/TextureGroup')
# Add shaders inside the node graph
# Expose outputs on the node graph for connection to the material
```

## Material Binding

### Applying the BindingAPI

Prims must apply `MaterialBindingAPI` to use material bindings:

```python
UsdShade.MaterialBindingAPI.Apply(geomPrim)
UsdShade.MaterialBindingAPI(geomPrim).Bind(material)
```

### Direct Binding

```usda
def Mesh "MyMesh" (
    prepend apiSchemas = ["MaterialBindingAPI"]
)
{
    rel material:binding = </Materials/MyMaterial>
}
```

### Collection-Based Binding

Bind a set of prims to a material via a collection:

```python
bindingAPI = UsdShade.MaterialBindingAPI(prim)
bindingAPI.Bind(collection, material, bindingName, bindingStrength)
```

### Material Purpose

Bindings can target specific render contexts:

- `allPurpose` (default) — used by all renderers
- `preview` — for preview/viewport rendering
- `full` — for final/production rendering

```python
UsdShade.MaterialBindingAPI(prim).Bind(material, materialPurpose='preview')
```

### Binding Inheritance

Material bindings **inherit down the prim hierarchy**. A prim without its own binding uses its nearest ancestor's binding.

## Render Contexts

Materials can contain different shader networks for different renderers:

```python
# Default (universal) surface output
material.CreateSurfaceOutput().ConnectToSource(previewShader, 'surface')

# Renderer-specific output
material.CreateSurfaceOutput('ri').ConnectToSource(rendermanShader, 'surface')
material.CreateSurfaceOutput('arnold').ConnectToSource(arnoldShader, 'surface')
```

Renderers request their context first, fall back to universal output.

## Connectability Rules

| Source Type | Can Connect To |
|-------------|---------------|
| Shader output | Shader input, NodeGraph output, Material output |
| NodeGraph output | Shader input (in enclosing scope), Material output |
| Material/NodeGraph interface input | Shader input (within the container) |

Connections cannot cross encapsulation boundaries upward — a shader inside a NodeGraph can't directly connect to something outside it (must go through NodeGraph interface).
