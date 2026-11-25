---
title: "Shading and Materials"
type: article
tags: [usd]
created: "2026-04-05"
updated: "2026-04-05"
sources: [../../docs/usd/schemas-shade.md, ../../docs/usd/preview-surface.md]
---

# Shading and Materials

UsdShade provides schemas for creating shading networks and materials in USD. It defines how materials are structured, connected, and bound to geometry.

## Key Classes

| Class | Purpose |
|-------|---------|
| `UsdShadeMaterial` | Container for render-context-specific shading networks |
| `UsdShadeShader` | Individual shading node (surface, texture, pattern) |
| `UsdShadeNodeGraph` | Container for reusable shader sub-networks |
| `UsdShadeInput` / `UsdShadeOutput` | Connectable attribute schemas |
| `UsdShadeConnectableAPI` | API for creating connections between inputs/outputs |
| `UsdShadeMaterialBindingAPI` | API for binding materials to geometry |

## Material Structure

A material is a container that holds shader nodes and exposes surface/displacement outputs:

```usda
def Material "MyMaterial"
{
    token outputs:surface.connect = </MyMaterial/PBR.outputs:surface>

    def Shader "PBR" {
        uniform token info:id = "UsdPreviewSurface"
        color3f inputs:diffuseColor = (0.8, 0.2, 0.1)
        float inputs:roughness = 0.4
        float inputs:metallic = 0.0
        token outputs:surface
    }
}
```

## Shader Connections

Connections link outputs to inputs, creating dataflow networks:

```python
shader.CreateInput('diffuseColor', Sdf.ValueTypeNames.Color3f).ConnectToSource(
    textureShader.ConnectableAPI(), 'rgb'
)
```

**Rule:** Valid shader connections win over authored input values. If an input has both a connection and a direct value, the connected value is used.

### Connectability Rules

| Source Type | Can Connect To |
|-------------|---------------|
| Shader output | Shader input, NodeGraph output, Material output |
| NodeGraph output | Shader input (in enclosing scope), Material output |
| Material/NodeGraph interface input | Shader input (within the container) |

Connections cannot cross encapsulation boundaries upward -- a shader inside a NodeGraph can't directly connect to something outside it.

## Texturing Pattern

The standard texturing workflow uses three node types:

```usda
def Material "TexturedMaterial"
{
    token outputs:surface.connect = </TexturedMaterial/Surface.outputs:surface>

    def Shader "Surface" {
        uniform token info:id = "UsdPreviewSurface"
        color3f inputs:diffuseColor.connect = </TexturedMaterial/DiffuseTex.outputs:rgb>
        token outputs:surface
    }

    def Shader "DiffuseTex" {
        uniform token info:id = "UsdUVTexture"
        asset inputs:file = @albedo.png@
        float2 inputs:st.connect = </TexturedMaterial/STReader.outputs:result>
        token inputs:sourceColorSpace = "sRGB"
        float3 outputs:rgb
    }

    def Shader "STReader" {
        uniform token info:id = "UsdPrimvarReader_float2"
        token inputs:varname = "st"
        float2 outputs:result
    }
}
```

## UsdPreviewSurface

The interchange-focused PBR shader supporting metallic and specular workflows.

### Key Inputs

| Input | Type | Default | Description |
|-------|------|---------|-------------|
| `diffuseColor` | color3f | (0.18, 0.18, 0.18) | Albedo / diffuse color |
| `metallic` | float | 0.0 | Metalness (0-1) |
| `roughness` | float | 0.5 | Surface roughness |
| `ior` | float | 1.5 | Index of refraction |
| `clearcoat` | float | 0.0 | Clear coat intensity |
| `opacity` | float | 1.0 | Surface opacity |
| `normal` | normal3f | (0, 0, 1) | Tangent-space normal map |
| `displacement` | float | 0.0 | Displacement amount |
| `emissiveColor` | color3f | (0, 0, 0) | Emissive component |
| `occlusion` | float | 1.0 | Ambient occlusion |

### Normal Map Convention

For normal maps, use scale and bias to remap [0,1] to [-1,1]:

```text
scale = (2, 2, 2, 1)
bias = (-1, -1, -1, 0)
sourceColorSpace = "raw"
```

## UsdUVTexture

Reads and samples a texture file.

| Input | Type | Description |
|-------|------|-------------|
| `file` | asset | Texture file path |
| `st` | float2 | Texture coordinates |
| `wrapS`/`wrapT` | token | Wrap mode: `black`, `clamp`, `repeat`, `mirror` |
| `sourceColorSpace` | token | `raw`, `sRGB`, `auto` |
| `scale` / `bias` | float4 | Post-read transform: `result = texture * scale + bias` |

Outputs: `r`, `g`, `b`, `a` (float), `rgb` (float3), `rgba` (float4).

## Material Binding

### Direct Binding

```usda
def Mesh "MyMesh" (
    prepend apiSchemas = ["MaterialBindingAPI"]
)
{
    rel material:binding = </Materials/MyMaterial>
}
```

### Material Purpose

Bindings can target specific render contexts:

- `allPurpose` (default) -- used by all renderers
- `preview` -- for preview/viewport rendering
- `full` -- for final/production rendering

### Binding Inheritance

Material bindings **inherit down the prim hierarchy**. A prim without its own binding uses its nearest ancestor's binding.

## Render Contexts

Materials can contain different shader networks for different renderers:

```python
material.CreateSurfaceOutput().ConnectToSource(previewShader, 'surface')
material.CreateSurfaceOutput('ri').ConnectToSource(rendermanShader, 'surface')
```

Renderers request their context first, fall back to universal output.

## Interface Inputs

Materials can expose "public" inputs that downstream consumers can set, propagating to internal shader inputs via connections. This lets consumers change parameters without editing the shader network internals.

## How BIF Uses UsdShade

- BIF exports materials as **dual output**: UsdPreviewSurface + OpenPBR MaterialX (`outputs:surface` + `outputs:mtlx:surface`)
- The C++ bridge reads both UsdPreviewSurface and MaterialX materials, with `is_materialx` flag for differentiation
- Material assignments use `MaterialBindingAPI` with `rel material:binding`
- The `MaterialCreate` and `MaterialParamOverride` edit operations author material opinions on the active layer

See [[openpbr-surface|OpenPBR Surface]] for BIF's internal material model and [[materialx-bridge|MaterialX Bridge]] for the MaterialX export path.

## Related

- [[stage-layer-prim|Stage, Layer, Prim]] -- USD object model foundation
- [[geometry-schemas|Geometry Schemas]] -- geometry that materials bind to
- [[openpbr-surface|OpenPBR Surface]] -- BIF's physically-based material implementation
- [[materialx-bridge|MaterialX Bridge]] -- MaterialX integration in BIF
- [[bif-usd-integration|BIF USD Integration]] -- overall USD workflow
