# UsdLux — Lighting Schemas

## Overview

UsdLux provides a representation for lights and related components common across graphics environments. Goal: portable lighting setups between creation environments and renderers.

## Light Types

### Boundable Lights (inherit from UsdGeomBoundable)

| Type | Description | Key Attributes |
|------|-------------|----------------|
| `RectLight` | One-sided rectangular emitter | `width`, `height`, `texture:file` |
| `DiskLight` | One-sided circular emitter | `radius` |
| `SphereLight` | Omnidirectional sphere | `radius`, `treatAsPoint` |
| `CylinderLight` | Outward-emitting cylinder | `radius`, `length`, `treatAsLine` |

### Non-Boundable Lights (inherit from UsdGeomXformable)

| Type | Description | Key Attributes |
|------|-------------|----------------|
| `DistantLight` | Directional (sun-like), along -Z | `angle` (angular diameter), `intensity` |
| `DomeLight` | Environment/IBL, inward-facing | `texture:file`, `texture:format` |

### API-Based Lights

| API | Purpose |
|-----|---------|
| `LightAPI` | Makes any Xformable-derived prim "be a light" |
| `MeshLightAPI` | Apply to UsdGeomMesh for mesh-emissive lighting |
| `VolumeLightAPI` | Apply to UsdVolVolume for volumetric emission |

## Common Light Attributes (via LightAPI)

| Attribute | Type | Default | Description |
|-----------|------|---------|-------------|
| `intensity` | float | 1.0 | Linear intensity multiplier |
| `exposure` | float | 0.0 | Logarithmic: `intensity * 2^exposure` |
| `color` | color3f | (1,1,1) | Light color |
| `enableColorTemperature` | bool | false | Use color temperature instead |
| `colorTemperature` | float | 6500 | Kelvin (warm ~2700, daylight ~6500) |
| `diffuse` | float | 1.0 | Diffuse contribution multiplier |
| `specular` | float | 1.0 | Specular contribution multiplier |
| `normalize` | bool | false | Normalize by area (consistent brightness when resizing) |

## Example

```usda
def RectLight "KeyLight" {
    float inputs:intensity = 500
    float inputs:exposure = 0
    color3f inputs:color = (1, 0.95, 0.9)
    float inputs:width = 2
    float inputs:height = 2
    bool inputs:normalize = true

    double3 xformOp:translate = (5, 5, 5)
    float3 xformOp:rotateXYZ = (-45, 45, 0)
    uniform token[] xformOpOrder = ["xformOp:translate", "xformOp:rotateXYZ"]
}

def DomeLight "EnvLight" {
    asset inputs:texture:file = @./env.hdr@
    float inputs:intensity = 1.0
    token inputs:texture:format = "latlong"  # or "mirroredBall", "angular"
}

def DistantLight "Sun" {
    float inputs:intensity = 50000
    float inputs:angle = 0.53   # Solar disk angular diameter in degrees
    color3f inputs:color = (1, 0.98, 0.95)
}
```

## Shadow API (UsdLuxShadowAPI)

Applied to lights for shadow control:

| Attribute | Type | Description |
|-----------|------|-------------|
| `shadow:enable` | bool | Enable/disable shadows |
| `shadow:color` | color3f | Shadow color |
| `shadow:distance` | float | Max shadow distance (-1 = infinite) |
| `shadow:falloff` | float | Shadow falloff distance |
| `shadow:falloffGamma` | float | Gamma for shadow falloff |

## Shaping API (UsdLuxShapingAPI)

Spotlight-like directional control, applicable to any light:

| Attribute | Type | Description |
|-----------|------|-------------|
| `shaping:focus` | float | Focus factor for beam |
| `shaping:focusTint` | color3f | Tint for focused region |
| `shaping:cone:angle` | float | Cone half-angle in degrees |
| `shaping:cone:softness` | float | Softness of cone edge (0-1) |
| `shaping:ies:file` | asset | IES profile for photometric distribution |

## Light Filters

Modify light output procedurally. Linked via `filters` relationship on the light:

```usda
def RectLight "Key" {
    rel filters = [</Key/Blocker>]

    def LightFilter "Blocker" {
        uniform token info:id = "PxrBlockerLightFilter"
    }
}
```

## Light Linking (UsdLuxLightListAPI)

Control which geometry a light affects:

```python
# On a prim (typically a model root)
listAPI = UsdLux.LightListAPI(prim)
# Stored as: light:list relationship
```

Light linking is typically managed at the scene assembly level, not on individual lights.

## DomeLight Specifics

- Texture formats: `latlong` (equirectangular), `mirroredBall`, `angular`, `cubeMapVerticalCross`
- `DomeLight_1` — newer version with correct orientation (texture facing inward, consistent with RenderMan)
- Portal lights (`DomeLight.portals` relationship) — rectangular portals that guide dome light sampling for interior scenes
