# UsdPreviewSurface Specification

Interchange-focused PBR surface shader supporting both metallic and specular workflows.

## Core Nodes

| Node ID | Purpose |
|---------|---------|
| `UsdPreviewSurface` | PBR surface shader |
| `UsdUVTexture` | Texture reader/sampler |
| `UsdPrimvarReader_*` | Read primvar data (float, float2, float3, etc.) |
| `UsdTransform2d` | 2D texture coordinate transform (SRT) |

## UsdPreviewSurface Inputs

| Input | Type | Default | Description |
|-------|------|---------|-------------|
| `diffuseColor` | color3f | (0.18, 0.18, 0.18) | Albedo (metallic) or diffuse (specular workflow) |
| `emissiveColor` | color3f | (0, 0, 0) | Emissive component |
| `useSpecularWorkflow` | int | 0 | 0 = metallic, 1 = specular |
| `specularColor` | color3f | (0, 0, 0) | Specular color (specular workflow only) |
| `metallic` | float | 0.0 | Metalness (metallic workflow only, 0-1) |
| `roughness` | float | 0.5 | Surface roughness (0 = mirror, 1 = diffuse) |
| `clearcoat` | float | 0.0 | Clear coat intensity |
| `clearcoatRoughness` | float | 0.01 | Clear coat roughness |
| `opacity` | float | 1.0 | Surface opacity (0 = transparent) |
| `opacityThreshold` | float | 0.0 | Alpha cutoff (>0 enables cutout) |
| `ior` | float | 1.5 | Index of refraction |
| `normal` | normal3f | (0, 0, 1) | Tangent-space normal map input |
| `displacement` | float | 0.0 | Displacement amount |
| `occlusion` | float | 1.0 | Ambient occlusion (0 = fully occluded) |

**Outputs:** `surface` (token), `displacement` (token)

### Metallic Workflow (default)

- `useSpecularWorkflow = 0`
- `metallic` controls metal vs dielectric (0 or 1, intermediate for transition)
- `diffuseColor` = albedo
- F0 derived from `ior` for dielectrics, from `diffuseColor` for metals

### Specular Workflow

- `useSpecularWorkflow = 1`
- `specularColor` = F0 reflectance at normal incidence
- `diffuseColor` = diffuse albedo
- `metallic` ignored

## UsdUVTexture

Reads and samples a texture file.

| Input | Type | Default | Description |
|-------|------|---------|-------------|
| `file` | asset | — | Texture file path |
| `st` | float2 | (0, 0) | Texture coordinates (connect to PrimvarReader) |
| `wrapS` | token | `useMetadata` | `black`, `clamp`, `repeat`, `mirror`, `useMetadata` |
| `wrapT` | token | `useMetadata` | Same options |
| `fallback` | float4 | (0,0,0,1) | Value when texture unavailable |
| `scale` | float4 | (1,1,1,1) | Post-read scale (for range remapping) |
| `bias` | float4 | (0,0,0,0) | Post-read bias: `result = texture * scale + bias` |
| `sourceColorSpace` | token | `auto` | `raw`, `sRGB`, `auto` |

**Outputs:** `r` (float), `g` (float), `b` (float), `a` (float), `rgb` (float3), `rgba` (float4)

### Normal Map Pattern

For normal maps, use `scale` and `bias` to remap [0,1] → [-1,1]:

```text
scale = (2, 2, 2, 1)
bias = (-1, -1, -1, 0)
sourceColorSpace = "raw"
```

Connect `rgb` output to `UsdPreviewSurface.normal` input.

## UsdPrimvarReader

Reads primvar values from geometry bound to the material.

| Variant | Output Type | Common Use |
|---------|-------------|------------|
| `UsdPrimvarReader_float` | float | Single channel data |
| `UsdPrimvarReader_float2` | float2 | Texture coordinates (`st`) |
| `UsdPrimvarReader_float3` | float3 | Color, position |
| `UsdPrimvarReader_float4` | float4 | RGBA data |
| `UsdPrimvarReader_int` | int | Integer primvar |
| `UsdPrimvarReader_string` | string | String primvar |
| `UsdPrimvarReader_normal` | normal3f | Normal data |
| `UsdPrimvarReader_point` | point3f | Position data |

| Input | Type | Description |
|-------|------|-------------|
| `varname` | token/string | Name of the primvar to read (e.g., `st`) |
| `fallback` | (matches output) | Value if primvar not found |

**Output:** `result` (type matches variant)

## UsdTransform2d

2D affine transform for texture coordinates: `result = in * scale * rotate + translation`

| Input | Type | Default | Description |
|-------|------|---------|-------------|
| `in` | float2 | (0, 0) | Input UV to transform |
| `rotation` | float | 0.0 | Counter-clockwise rotation in degrees |
| `scale` | float2 | (1, 1) | Scale around origin |
| `translation` | float2 | (0, 0) | Translation offset |

**Output:** `result` (float2)

**Note:** Transforms coordinates, not the texture itself. Apply inverse to get expected visual result.

## Complete Example

```usda
def Material "PBRMaterial"
{
    token outputs:surface.connect = </PBRMaterial/Surface.outputs:surface>

    def Shader "Surface" {
        uniform token info:id = "UsdPreviewSurface"
        color3f inputs:diffuseColor.connect = </PBRMaterial/DiffuseTex.outputs:rgb>
        float inputs:roughness = 0.4
        float inputs:metallic = 0.0
        normal3f inputs:normal.connect = </PBRMaterial/NormalTex.outputs:rgb>
        token outputs:surface
    }

    def Shader "DiffuseTex" {
        uniform token info:id = "UsdUVTexture"
        asset inputs:file = @albedo.png@
        float2 inputs:st.connect = </PBRMaterial/STReader.outputs:result>
        token inputs:sourceColorSpace = "sRGB"
        float3 outputs:rgb
    }

    def Shader "NormalTex" {
        uniform token info:id = "UsdUVTexture"
        asset inputs:file = @normal.png@
        float2 inputs:st.connect = </PBRMaterial/STReader.outputs:result>
        token inputs:sourceColorSpace = "raw"
        float4 inputs:scale = (2, 2, 2, 1)
        float4 inputs:bias = (-1, -1, -1, 0)
        float3 outputs:rgb
    }

    def Shader "STReader" {
        uniform token info:id = "UsdPrimvarReader_float2"
        token inputs:varname = "st"
        float2 outputs:result
    }
}
```
