# BIF Material Editor Design

## Status

- **Author:** ArchitectUX
- **Date:** 2026-03-30
- **Target milestone:** v0.21.0 (MaterialX Authoring), with foundations in v0.16.0 (Edit Operations)
- **Depends on:** v0.14.0 (layer-aware stage), v0.15.0 (Qt migration)

---

## 1. Architecture: How It Fits Into BIF

### The Two Graph Problem

BIF already has a **scene assembly graph** (egui_snarl) with 11 node types: UsdRead, IvarRender, Primitive, ScatterPoints, PointInstancer, UsdExport, Xform, UsdPrim, GraftBranches, Cache, HdriEnvironment. This graph flows `Scene`, `Image`, and `Environment` pin types.

Materials are a **different domain**. Mixing material nodes into the scene graph creates the Blender problem: one enormous graph doing everything, visual spaghetti, cognitive overload.

**Decision: Separate material graph, same framework.**

```text
Scene Graph (existing)          Material Graph (new)
========================        ========================
UsdRead -> Xform -> Export      Texture -> OpenPBR -> MaterialOut
Primitive -> Scatter -> PI      Noise -> Mix -> UsdPreview -> MaterialOut
HdriEnvironment -> IvarRender   Image -> Ramp -> MtlX Standard -> MaterialOut
```

The material graph lives in its own tab in the bottom dock, alongside the scene node graph:

```text
┌────────────────┬──────────────────────────────────┬───────────────┐
│  SCENE TREE    │         V I E W P O R T          │  PROPERTIES   │
│  + Layers      │                                  │  + Material   │
│                │   [lookdev sphere floats here]    │    Params     │
│                │                                  │               │
├────────────────┴──────────────────────────────────┴───────────────┤
│  Scene Graph  |  MATERIAL GRAPH  |  USDA Preview  |  Render Log  │
└──────────────────────────────────────────────────────────────────┘
```

### Entry Points

How you get to the material editor:

1. **Scene browser:** Select a prim with a material binding -> right panel shows material card -> click "Edit Material" -> bottom dock switches to Material Graph tab, material loaded
2. **Scene graph:** Select a UsdRead/Primitive node -> right panel shows assigned material -> same flow
3. **Command palette:** `Ctrl+P` -> "Edit Material" / "New Material" / "Assign Material"
4. **Bottom dock tab:** Click "Material Graph" tab directly, pick material from dropdown
5. **"Materials" workspace preset:** Viewport + material graph + lookdev sphere + material properties. One click from workspace switcher.

### USD Integration

Every material edit authors opinions on the **active edit target layer**. The material graph is a visual editor for UsdShade prims — not a parallel representation.

```text
Material Graph Node          USD Prim Created
========================     ========================
MaterialOut "hero_wet"   ->  /Materials/hero_wet  (UsdShadeMaterial)
OpenPBR Surface          ->  /Materials/hero_wet/OpenPBR  (UsdShadeShader, id=OpenPBR)
UsdPreviewSurface        ->  /Materials/hero_wet/Preview  (UsdShadeShader, id=UsdPreviewSurface)
Texture "base_color"     ->  /Materials/hero_wet/base_color_tex  (UsdShadeShader, id=UsdUVTexture)
PrimvarReader "st"       ->  /Materials/hero_wet/st_reader  (UsdShadeShader, id=UsdPrimvarReader_float2)
```

Layer color coding applies: if you're editing on the "lookdev" layer (tagged green), all material nodes and property borders show green. Switch layers, see which opinions came from where.

---

## 2. Two-Mode Design: Parameter Sheet vs Node Graph

### The Core Insight

90% of material work is: pick a shading model, adjust 5-8 sliders, assign textures. The remaining 10% needs a full node graph (custom blends, procedural patterns, multi-layer composites). The editor must serve both without the simple case feeling like overkill.

### Mode A: Parameter Sheet (Default)

When you select a material, the right-panel properties inspector shows a **compact parameter sheet**. No graph needed for simple materials.

```text
┌─────────────────────────────┐
│  ┌───────┐                  │
│  │  orb  │  hero_wet        │
│  │ preview│  OpenPBR Surface │
│  │  128px │  lookdev.usd [*]│  <- layer badge + dirty indicator
│  └───────┘                  │
│                             │
│  Shading Model  [OpenPBR v] │  <- dropdown: OpenPBR / UsdPreview / MaterialX
│                             │
│  BASE ─────────────────     │  <- collapsible section headers
│  Color       [████] #8B4513 │  <- swatch + hex, click swatch = picker
│    [base_color.exr    ] [x] │  <- texture slot (drag-drop or browse)
│  Metallic    [====|===] 0.0 │  <- slider
│    [metalness.exr     ] [x] │  <- texture slot
│  Roughness   [==|=====] 0.4 │  <- slider
│    [roughness.exr     ] [x] │  <- texture slot
│                             │
│  SPECULAR ─────────────     │  <- collapsed by default for simple mats
│  IOR         [==|=====] 1.5 │
│  Weight      [========] 1.0 │
│                             │
│  COAT ─────────────────     │  <- collapsed
│  Weight      [========] 0.0 │
│                             │
│  EMISSION ─────────────     │  <- collapsed
│  Luminance   [========] 0.0 │
│  Color       [████] #000000 │
│                             │
│  [Open in Graph]  [Assign]  │
│                             │
│  USDA Preview:              │
│  ┌─────────────────────────┐│
│  │ def Shader "OpenPBR" {  ││  <- live USDA, read-only, 6 lines max
│  │   token info:id = ...   ││
│  │   color3f inputs:base.. ││
│  │ }                       ││
│  └─────────────────────────┘│
└─────────────────────────────┘
```

**Interaction details:**

- Sections auto-expand when non-default values are present
- Texture slots: drag from asset browser or click folder icon to browse
- Color swatches: click opens a floating HSV picker with hex input
- Sliders: click-drag, double-click for numeric entry, right-click for "Reset to Default"
- Shading model dropdown triggers conversion (see section 3)
- "Open in Graph" switches bottom dock to Material Graph tab with this material loaded
- Inline USDA preview shows the authored opinions in real-time (max 6 lines, expandable)

### Mode B: Node Graph (Power Mode)

Full node graph in the bottom dock tab. Same snarl-based framework as the scene graph, different node types and pin types.

```text
┌──────────────────────────────────────────────────────────────────┐
│  Material: hero_wet [v]   Layer: lookdev [*]   [+ Node]  [Fit]  │
│ ─────────────────────────────────────────────────────────────────│
│                                                                  │
│  ┌──────────────┐     ┌───────────────────┐    ┌──────────────┐ │
│  │ UsdUVTexture │     │  OpenPBR Surface  │    │ MaterialOut  │ │
│  │──────────────│     │───────────────────│    │──────────────│ │
│  │ file: base.. │     │ base_color   [in]●──●──│ surface  [in]│ │
│  │ wrapS: repea │     │ metallic     0.0 │    │              │ │
│  │ ○ rgb ───────│─●   │ roughness    0.4 │    │ ┌──────────┐ │ │
│  │ ○ r          │ │   │ specular_ior 1.5 │    │ │  preview  │ │ │
│  │ ○ a          │ │   │ coat_weight  0.0 │    │ │   orb     │ │ │
│  └──────────────┘ │   │ ...              │    │ │  128x128  │ │ │
│                   │   │                  │    │ └──────────┘ │ │
│  ┌──────────────┐ │   │ ● surface [out]──│─●  └──────────────┘ │
│  │ UsdUVTexture │ │   └───────────────────┘                     │
│  │──────────────│ │                                              │
│  │ file: rough..│ │                                              │
│  │ ○ r ─────────│─●──> roughness                                │
│  └──────────────┘                                                │
│                                                                  │
│  ┌──────────────┐                                                │
│  │PrimvarReader │                                                │
│  │──────────────│                                                │
│  │ varname: st  │                                                │
│  │ ○ result ────│─●──> (connected to both UsdUVTexture.st)      │
│  └──────────────┘                                                │
│                                                                  │
└──────────────────────────────────────────────────────────────────┘
```

**Key differences from scene graph:**

- Material-specific pin types (Color3f, Float, Normal3f, Token, Point2f) with distinct colors
- Shader nodes show inline parameter values (not just in property inspector)
- MaterialOut node has an embedded preview orb
- Right-click canvas: add node menu filtered by category (Shaders, Textures, Math, Utility)
- Nodes are narrower and shorter than scene nodes — materials have many nodes, screen real estate matters

### Mode Transition

The two modes are **views of the same data**, not separate systems. The parameter sheet is a flattened view of the graph. Editing a slider in the parameter sheet updates the corresponding node parameter in the graph (and vice versa).

```text
Parameter Sheet                  Node Graph (equivalent)
===================              ==========================
Base Color: #8B4513          <-> OpenPBR node, base_color input = (0.545, 0.271, 0.075)
  [base_color.exr]           <-> UsdUVTexture node connected to base_color input
Roughness: 0.4               <-> OpenPBR node, roughness input = 0.4
  [roughness.exr]            <-> UsdUVTexture node connected to roughness input
```

When you connect a texture to a parameter in the graph, the parameter sheet shows the texture slot filled. When you set a texture in the parameter sheet, it creates a UsdUVTexture + PrimvarReader node pair in the graph.

**"Open in Graph" button:** Scrolls the graph to center on the material's shader node, selects it.

**"Collapse to Sheet" button (in graph):** Switches bottom dock focus away, material stays editable via right-panel parameter sheet.

---

## 3. Shading Models and Node Types

### Three Models, One Material

A single UsdShadeMaterial can have multiple shader outputs for different render contexts:

```text
def Material "hero_wet" {
    # BIF's native renderer uses OpenPBR
    token outputs:bif:surface.connect = </Materials/hero_wet/OpenPBR.outputs:surface>
    
    # USD standard preview
    token outputs:surface.connect = </Materials/hero_wet/Preview.outputs:surface>
    
    # MaterialX for interchange
    token outputs:mtlx:surface.connect = </Materials/hero_wet/MtlxStandard.outputs:surface>
}
```

BIF renders with OpenPBR (via Ivar path tracer). UsdPreviewSurface is exported for compatibility. MaterialX is for interchange with other DCCs.

### Shading Model Dropdown Behavior

| Action | Result |
| -------- | -------- |
| Select "OpenPBR" | Shows OpenPBR params. Creates/edits `outputs:bif:surface` shader. |
| Select "UsdPreviewSurface" | Shows UsdPreview params. Creates/edits `outputs:surface` shader. |
| Select "MaterialX" | Unlocks full node graph. Creates/edits `outputs:mtlx:surface` network. |
| Switch OpenPBR -> UsdPreview | Auto-converts params where mappings exist (see below). Warns about lossy conversion. |

### Auto-Conversion Table

| OpenPBR | UsdPreviewSurface | Notes |
| --------- | ------------------- | ------- |
| base_color | diffuseColor | Direct 1:1 |
| base_metalness | metallic | Direct 1:1 |
| specular_roughness | roughness | Direct 1:1 |
| specular_ior | ior | Direct 1:1 |
| coat_weight | clearcoat | Direct 1:1 |
| coat_roughness | clearcoatRoughness | Direct 1:1 |
| geometry_opacity | opacity | Direct 1:1 |
| emission_color | emissiveColor | Direct 1:1 |
| transmission_weight | -- | No UsdPreview equivalent (lossy) |
| subsurface_weight | -- | No UsdPreview equivalent (lossy) |
| fuzz_weight | -- | No UsdPreview equivalent (lossy) |
| base_diffuse_roughness | -- | No UsdPreview equivalent |

Conversion dialog: "Converting OpenPBR to UsdPreviewSurface. 3 parameters have no equivalent and will be lost: transmission, subsurface, fuzz. Continue?"

### Node Type Catalog

#### A. Shader Nodes (output: surface token)

| Node | info:id | Parameters | Use |
| ------ | --------- | ------------ | ----- |
| **OpenPBR Surface** | `OpenPBR` | base_weight, base_color, base_metalness, base_diffuse_roughness, specular_weight, specular_color, specular_roughness, specular_ior, specular_roughness_anisotropy, coat_weight, coat_color, coat_roughness, coat_ior, fuzz_weight, fuzz_color, fuzz_roughness, subsurface_weight, emission_luminance, emission_color, transmission_weight, geometry_opacity | BIF native |
| **UsdPreviewSurface** | `UsdPreviewSurface` | diffuseColor, emissiveColor, useSpecularWorkflow, specularColor, metallic, roughness, clearcoat, clearcoatRoughness, opacity, opacityThreshold, ior, normal, displacement, occlusion | USD standard |
| **MtlX Standard Surface** | `ND_standard_surface_surfaceshader` | base, base_color, diffuse_roughness, metalness, specular, specular_color, specular_roughness, specular_IOR, specular_anisotropy, specular_rotation, transmission, transmission_color, transmission_depth, subsurface, subsurface_color, subsurface_radius, subsurface_scale, sheen, sheen_color, sheen_roughness, coat, coat_color, coat_roughness, coat_IOR, emission, emission_color, thin_walled | MaterialX interchange |

#### B. Texture Nodes (output: color/float/normal channels)

| Node | info:id | Inputs | Outputs | Use |
| ------ | --------- | -------- | --------- | ----- |
| **UsdUVTexture** | `UsdUVTexture` | file (asset), st (float2), wrapS/wrapT (token), fallback, scale, bias | rgb (color3f), r/g/b/a (float) | Texture sampling |
| **PrimvarReader Float2** | `UsdPrimvarReader_float2` | varname (token), fallback | result (float2) | Read UVs |
| **PrimvarReader Float** | `UsdPrimvarReader_float` | varname (token), fallback | result (float) | Read float primvar |
| **PrimvarReader Color** | `UsdPrimvarReader_float3` | varname (token), fallback | result (color3f) | Read color primvar |
| **Transform2d** | `UsdTransform2d` | in (float2), rotation (float), scale (float2), translation (float2) | result (float2) | UV transforms |

#### C. MaterialX Pattern Nodes (v0.21.0+, full node graph mode only)

| Node | Purpose | Outputs |
| ------ | --------- | --------- |
| **MtlX Image** | Texture with colorspace | out (color3/float) |
| **MtlX Constant** | Constant value | out |
| **MtlX Multiply** | A * B | out |
| **MtlX Add** | A + B | out |
| **MtlX Mix** | lerp(A, B, t) | out |
| **MtlX Noise2d/3d** | Perlin noise | out |
| **MtlX Ramp** | Gradient ramp | out |
| **MtlX NormalMap** | Tangent-space normal decode | out (normal3) |
| **MtlX Clamp** | Clamp to range | out |
| **MtlX Range** | Remap value range | out |
| **MtlX Dot** | Pass-through / type cast | out |

#### D. Utility Nodes

| Node | Purpose | Outputs |
| ------ | --------- | --------- |
| **MaterialOut** | Terminal node, connects shader to material | -- (sink) |
| **Color Constant** | Pick a color value | out (color3f) |
| **Float Constant** | Pick a float value | out (float) |
| **Preview** | Render preview orb at any point in graph | image |

### Pin Types for Material Graph

New `MaterialPinType` enum (separate from scene graph's `PinType`):

| Pin Type | Color | Shape | Example |
| ---------- | ------- | ------- | --------- |
| Surface | `#E8E8E8` (white) | Diamond | Shader output -> MaterialOut |
| Color3f | `#E8D44D` (yellow) | Circle | base_color, diffuseColor |
| Float | `#A0A0A0` (gray) | Circle | roughness, metallic, opacity |
| Normal3f | `#7B68EE` (purple) | Circle | normal map output |
| Float2 | `#4DB8FF` (light blue) | Circle | UV coordinates |
| Token | `#FF8C69` (salmon) | Square | varname, wrapS |
| Asset | `#69D969` (green) | Square | file paths |
| Displacement | `#C08050` (brown) | Diamond | displacement output |

These colors follow the Blender/Substance convention that artists already know.

---

## 4. Preview Approach: The Lookdev Orb

### Design: Floating Viewport Orb

The sleekest approach is a **lookdev preview sphere** that floats in the main viewport — not trapped inside a tiny panel or node thumbnail.

```text
┌──────────────────────────────────────────────────┐
│                                                  │
│                  V I E W P O R T                 │
│                                                  │
│                                                  │
│                                                  │
│                                                  │
│                                    ┌──────────┐  │
│                                    │          │  │
│                                    │  preview │  │
│                                    │   orb    │  │
│                                    │  192x192 │  │
│                                    │          │  │
│                                    └──────────┘  │
│                                                  │
└──────────────────────────────────────────────────┘
```

**Behavior:**

- Appears when a material is selected (in any panel — browser, graph, properties)
- Floats in bottom-right corner of viewport, above the bottom dock
- Default: 192x192px sphere on neutral gray/checker background
- Click to cycle preview shapes: sphere (default), cube, plane, custom mesh
- Drag edge to resize (128 to 384px)
- Right-click: "Pin Preview" (stays visible), "Pop Out" (separate window), "Hide"
- Semi-transparent backdrop `#1c1c1c` at 90% opacity, 6px radius, shadow gap border

### Rendering Strategy

Material preview uses Ivar (existing CPU path tracer) with optimizations for responsiveness:

| Interaction | Preview Behavior |
| ------------- | ----------------- |
| Parameter drag (in progress) | 1 SPP, immediate (~16ms for 192px sphere) |
| Parameter release | Progressive refinement: 1 -> 4 -> 16 -> 64 SPP |
| Texture change | Show loading spinner, then progressive refinement |
| Idle (converged) | Hold at 64 SPP (clean enough for preview) |

**Implementation detail:** The preview sphere is a hardcoded unit sphere BVH (no USD stage needed). The material is applied directly as a `dyn Material` to the preview geometry. Environment comes from the scene's HDRI if loaded, otherwise a neutral studio HDRI baked in as a default asset.

```rust
// Pseudocode for preview render pipeline
struct MaterialPreview {
    sphere_bvh: Arc<Bvh>,           // Prebuilt, never changes
    env_map: Arc<EnvironmentMap>,    // From scene or default studio
    preview_texture: wgpu::Texture,  // Display target
    current_spp: u32,
    target_spp: u32,
    material: Arc<dyn Material>,     // Currently previewed material
    dirty: bool,                     // True when material params change
    shape: PreviewShape,             // Sphere, Cube, Plane, Custom
}

enum PreviewShape {
    Sphere,   // Default — shows reflections, fresnel, roughness well
    Cube,     // Shows anisotropy, UV seams
    Plane,    // Shows texture tiling, displacement
    Custom(PathBuf),  // User-selected mesh
}
```

### Alternative: Node Thumbnail Preview

The MaterialOut node in the graph also gets a small preview (128x128). This is secondary to the floating orb but useful when working purely in graph mode.

The MaterialOut node thumbnail uses the same render pipeline, downsampled. It updates only when the node is visible in the graph viewport (no off-screen rendering waste).

---

## 5. Component Mockups

### 5A. Full "Materials" Workspace

```text
┌────────────────┬──────────────────────────────────┬───────────────────────┐
│ SCENE TREE     │                                  │ MATERIAL PROPERTIES   │
│ ───────────    │      V I E W P O R T             │ ─────────────────     │
│ v World        │                                  │ ┌───────┐             │
│   v Materials  │   (lookdev scene or              │ │preview│ hero_wet    │
│     hero_wet * │    selected object)              │ │ orb   │ OpenPBR     │
│     ground_dry │                                  │ └───────┘             │
│     glass_thin │                                  │                       │
│   v Geometry   │                    ┌──────────┐  │ Model [OpenPBR    v]  │
│     hero_mesh  │                    │  mat orb │  │                       │
│     ground     │                    │  192x192 │  │ BASE ────────────     │
│                │                    └──────────┘  │ Color    [████] #8B.. │
│ LAYERS         │                                  │   [base_color.exr] x  │
│ ───────────    │                                  │ Metallic [====|==] 0  │
│ ● session.usd  │                                  │ Roughness [=|====] .4 │
│ ● lookdev.usd *│                                  │                       │
│ ○ base.usd     │                                  │ SPECULAR ────────     │
│                │                                  │ IOR      [==|===] 1.5 │
│                │                                  │                       │
│                │                                  │ [Open in Graph]       │
├────────────────┴──────────────────────────────────┴───────────────────────┤
│  Scene Graph  | [MATERIAL GRAPH] |  USDA Preview  |  Render Log          │
│ ─────────────────────────────────────────────────────────────────────────│
│                                                                          │
│  ┌──────────────┐    ┌────────────────────┐    ┌───────────────┐        │
│  │ UsdUVTexture │    │   OpenPBR Surface  │    │  MaterialOut  │        │
│  │ base_color.. │    │                    │    │               │        │
│  │  ○ rgb ──────│─●──│● base_color        │    │  ● surface ───│        │
│  └──────────────┘    │  metallic     0.0  │    │  ┌─────────┐ │        │
│                      │  roughness    0.4  │    │  │ preview │ │        │
│  ┌──────────────┐    │  specular_ior 1.5  │    │  │  thumb  │ │        │
│  │ UsdUVTexture │    │                    │    │  └─────────┘ │        │
│  │ roughness..  │    │  ● surface ────────│─●──│              │        │
│  │  ○ r ────────│─●──│● roughness         │    └───────────────┘        │
│  └──────────────┘    └────────────────────┘                              │
│                                                                          │
└──────────────────────────────────────────────────────────────────────────┘
```

### 5B. Simple Material (Parameter Sheet Only)

For an artist who just needs to tweak a material — no graph at all:

```text
┌────────────────┬──────────────────────────────────┬───────────────────────┐
│ SCENE TREE     │                                  │ MATERIAL PROPERTIES   │
│                │      V I E W P O R T             │                       │
│ v World        │                                  │ ┌───────┐             │
│   v Geometry   │  (scene with hero_mesh           │ │ ◐     │ plastic_r  │
│     hero_mesh  │   selected, material             │ │  orb  │ OpenPBR    │
│                │   highlighted on surface)        │ └───────┘             │
│                │                                  │                       │
│                │                                  │ BASE ────────────     │
│                │                    ┌──────────┐  │ Color    [████] #C23  │
│                │                    │  192x192 │  │ Metallic [========] 0 │
│                │                    │  preview │  │ Roughness [===|==] .6 │
│                │                    └──────────┘  │                       │
│                │                                  │ All other sections    │
│                │                                  │ collapsed (defaults)  │
│                │                                  │                       │
│                │                                  │ [Open in Graph]       │
│                │                                  │ [Assign to Selected]  │
├────────────────┴──────────────────────────────────┴───────────────────────┤
│  [Scene Graph] |  USDA Preview  |  Render Log                            │
│  (normal bottom dock — no material graph tab needed)                     │
└──────────────────────────────────────────────────────────────────────────┘
```

### 5C. MaterialX Full Graph (Power Mode)

```text
┌────────────────┬──────────────────────────────────┬───────────────────────┐
│ SCENE TREE     │         V I E W P O R T          │ NODE PROPERTIES       │
│                │                                  │ ─────────────────     │
│                │                                  │ MtlX Mix              │
│                │                                  │                       │
│                │                                  │ fg:  [connected]      │
│                │                    ┌──────────┐  │ bg:  [connected]      │
│                │                    │  preview │  │ mix: [===|=====] 0.3  │
│                │                    └──────────┘  │                       │
│                │                                  │ USDA:                 │
│                │                                  │ ┌───────────────────┐ │
│                │                                  │ │ def Shader "Mix"{ │ │
│                │                                  │ │  token info:id =  │ │
│                │                                  │ │   "ND_mix_color3" │ │
│                │                                  │ └───────────────────┘ │
├────────────────┴──────────────────────────────────┴───────────────────────┤
│  Scene Graph  | [MATERIAL GRAPH] |  USDA Preview  |  Render Log          │
│ ─────────────────────────────────────────────────────────────────────────│
│  Material: bark_layered [v]   Layer: lookdev [*]  [+ Node]  [Fit] [Auto]│
│                                                                          │
│  ┌───────────┐   ┌───────────┐   ┌─────────────────┐   ┌────────────┐  │
│  │ MtlX Image│   │ MtlX Image│   │ MtlX Mix        │   │ MtlX       │  │
│  │ bark.exr  │   │ moss.exr  │   │                  │   │ Standard   │  │
│  │ ○ out ────│─● │ ○ out ────│─● │● fg    ● out ────│─● │ Surface    │  │
│  └───────────┘ │ └───────────┘ │ │● bg              │ │ │            │  │
│                │               │ │● mix             │ │ │● base_color│  │
│                └───────────────│─┘                   │ │ ● surface ──│─●│
│                                └─────────────────────┘ └────────────┘  ││
│  ┌───────────┐                                         ┌────────────┐  ││
│  │MtlX Noise │   ┌───────────┐                         │MaterialOut │  ││
│  │ Perlin 2D │   │MtlX Range │                         │            │──┘│
│  │ ○ out ────│─● │ ○ out ────│─●──> mix input          │ ● surface  │   │
│  └───────────┘ │ │● in       │                         │ ┌────────┐ │   │
│                └─┘           │                         │ │ thumb  │ │   │
│                              │                         │ └────────┘ │   │
│                              │                         └────────────┘   │
└──────────────────────────────────────────────────────────────────────────┘
```

### 5D. Material Assignment Flow

```text
Step 1: Select geometry in viewport or scene tree
Step 2: Right panel shows geometry properties with "Material" section:

┌─────────────────────────┐
│ PROPERTIES              │
│ hero_mesh  (Mesh)       │
│                         │
│ TRANSFORM ──────────    │
│ ...                     │
│                         │
│ MATERIAL ───────────    │
│ Bound: hero_wet         │
│ Source: lookdev.usd     │
│ Purpose: allPurpose     │
│                         │
│ [Edit]  [Reassign v]   │
│         ┌─────────────┐│
│         │ hero_wet    *││  <- * = currently bound
│         │ ground_dry   ││
│         │ glass_thin   ││
│         │ ───────────  ││
│         │ + New Material│
│         │ Browse...    ││
│         └─────────────┘│
└─────────────────────────┘

Step 3: Click "Edit" -> loads material in property inspector + preview orb appears
Step 4: Or click "Reassign" dropdown -> pick from scene materials or create new
```

---

## 6. Texture Workflow

### Connecting Textures in Parameter Sheet Mode

Click the texture slot button `[base_color.exr]` or drag-drop a file:

1. BIF creates a `UsdUVTexture` shader prim connected to the parameter
2. BIF creates a `UsdPrimvarReader_float2` for UV coords (shared across texture nodes)
3. The texture slot shows the filename; `[x]` button disconnects and removes the shader prim
4. If the file is a UDIM pattern (`base_color.<UDIM>.exr`), BIF detects it and sets the asset path accordingly

```text
User action:                    USD authored:
========================        ========================
Drag "base_color.exr"          def Shader "base_color_tex" {
  onto Base Color slot              uniform token info:id = "UsdUVTexture"
                                    asset inputs:file = @base_color.exr@
                                    float2 inputs:st.connect = <../st_reader.outputs:result>
                                    token inputs:wrapS = "repeat"
                                    token inputs:wrapT = "repeat"
                                }
                                # OpenPBR node:
                                # color3f inputs:base_color.connect = <../base_color_tex.outputs:rgb>
```

### Texture Node Details in Graph Mode

When working in the full node graph, texture nodes expose all controls:

```text
┌─────────────────────────┐
│ UsdUVTexture            │
│─────────────────────────│
│ file: base_color.exr    │  <- click to browse, drag to replace
│ ┌───────────────────┐   │
│ │ [texture preview] │   │  <- 64x64 thumbnail of the texture
│ │  (first tile if   │   │
│ │   UDIM detected)  │   │
│ └───────────────────┘   │
│ wrapS: [repeat    v]    │
│ wrapT: [repeat    v]    │
│ fallback: [████] black  │
│ scale:  [========] 1.0  │
│ bias:   [========] 0.0  │
│ ● st [float2] ←────     │  <- input: UV coordinates
│                         │
│ ○ rgb [color3f] ────→   │  <- outputs
│ ○ r   [float]   ────→   │
│ ○ g   [float]   ────→   │
│ ○ b   [float]   ────→   │
│ ○ a   [float]   ────→   │
└─────────────────────────┘
```

### Texture Conversion Integration

BIF already has `.tx` conversion (via OIIO feature flag). The material editor hooks into this:

- When a texture is assigned, check if a `.tx` version exists
- Status indicator on the texture slot: green dot = `.tx` available, yellow = raw file
- "Convert All to .tx" button in the material properties header
- Uses existing `NodeGraphEvent::ConvertTexturesToTx` pipeline

### UDIM Support

BIF's renderer already handles UDIM via `UdimTileSet`. The material editor:

- Detects `<UDIM>` or `.<UDIM>.` patterns in filenames
- Shows "UDIM" badge on the texture slot
- Texture thumbnail shows the 1001 tile
- USD asset path uses `@base_color.<UDIM>.exr@` format

---

## 7. Implementation Phasing

### Phase 1: Material Property Sheet (v0.16.0 — Edit Operations)

**Scope:** Read-only material inspection + basic parameter editing via property inspector.

- Extend `property_inspector` to detect material bindings on selected prims
- Show material card (name, type, key params) when material-bound prim is selected
- OpenPBR and UsdPreviewSurface parameter display (read from USD stage)
- Basic parameter editing: sliders author opinions on active layer
- No graph, no preview orb, no texture assignment yet

**New types:**

```rust
// bif_core/src/usd/material.rs
pub enum ShadingModel {
    OpenPbr,
    UsdPreviewSurface,
    MaterialX,
}

pub struct MaterialDescription {
    pub prim_path: String,
    pub shading_model: ShadingModel,
    pub params: HashMap<String, MaterialParamValue>,
    pub texture_inputs: HashMap<String, String>,  // param_name -> asset_path
}

pub enum MaterialParamValue {
    Float(f32),
    Color3f([f32; 3]),
    Int(i32),
    Token(String),
    Asset(String),
}
```

### Phase 2: Material Preview (v0.17.0 — Viewport Performance)

**Scope:** Lookdev preview orb in viewport.

- `MaterialPreview` struct with prebuilt sphere BVH
- Background thread renders preview at low SPP
- Floating orb widget in viewport (bottom-right corner)
- Progressive refinement on parameter change
- Preview shape cycling (sphere, cube, plane)

### Phase 3: Material Graph Foundation (v0.20.0 — Scene Authoring)

**Scope:** Node graph for materials, texture workflow.

- `MaterialPinType` enum with typed connections
- `MaterialNode` enum (shader nodes, texture nodes, utility nodes)
- Material graph tab in bottom dock
- Texture node creation from parameter sheet drag-drop
- PrimvarReader auto-creation for UV coords
- Bidirectional sync: parameter sheet <-> graph

### Phase 4: MaterialX Full Graph (v0.21.0 — MaterialX Authoring)

**Scope:** Full MaterialX node graph with math/pattern nodes.

- MaterialX pattern nodes (noise, ramp, mix, math ops)
- MaterialX Standard Surface shader node
- XML round-trip (read/write MaterialX documents)
- Per-node preview thumbnails in graph
- Custom material export with full shader networks

---

## 8. Keyboard Shortcuts

| Shortcut | Action |
| ---------- | -------- |
| `M` | Toggle material editor focus (when material selected) |
| `Ctrl+Shift+M` | Open Materials workspace |
| `T` | Cycle preview shape (sphere/cube/plane) |
| `Ctrl+P` -> "mat" | Command palette filtered to material commands |
| `N` (in material graph) | Add node menu |
| `F` (in material graph) | Frame all / fit to view |
| `L` (in material graph) | Auto-layout nodes |
| `1-9` (in material graph) | Quick-add: 1=Texture, 2=PrimvarReader, 3=Mix, etc. |
| `Ctrl+D` | Duplicate selected material node |
| `X` / `Delete` | Delete selected material node |
| `Ctrl+C/V` | Copy/paste material nodes |
| `Shift+drag` | Disconnect wire by dragging off pin |

---

## 9. Visual Design Specifics

All material editor UI follows the established BIF theme from `theme.rs`:

| Element | Value | Constant |
| --------- | ------- | ---------- |
| Panel background | `rgb(34, 38, 44)` | `BG_PANEL` |
| Node body | `rgb(42, 47, 54)` | `BG_SURFACE` |
| Node header (OpenPBR) | `rgb(74, 144, 217)` | `ACCENT_PRIMARY` |
| Node header (UsdPreview) | `rgb(60, 180, 80)` | `STATUS_OK` |
| Node header (MaterialX) | `rgb(220, 170, 50)` | `STATUS_WARNING` |
| Node header (Texture) | `rgb(100, 150, 220)` | `STATUS_INFO` |
| Node header (Math/Utility) | `rgb(140, 145, 155)` | `TEXT_SECONDARY` |
| Selected node border | `rgb(74, 144, 217)` 2px | `ACCENT_PRIMARY` |
| Wire (connected) | Pin source color at 80% opacity | -- |
| Wire (dragging) | Pin source color, dashed | -- |
| Preview orb border | 1px shadow gap, `rgb(26, 29, 33)` | `BG_BASE` |
| Corner radius | 6px everywhere | -- |

### Node Header Color Coding

Material nodes use header color to instantly communicate the shading model:

- **Blue** = OpenPBR (BIF native) — the primary recommendation
- **Green** = UsdPreviewSurface — USD standard, "safe for export"
- **Gold** = MaterialX — full power mode, interchange format
- **Light blue** = Texture/Pattern nodes — data providers
- **Gray** = Utility nodes — constants, math, converters

This mirrors the layer color coding concept: you can glance at a graph and immediately know which ecosystem each node belongs to.

---

## 10. Unresolved Questions

1. **Dual shader authoring:** When editing OpenPBR params, should BIF auto-generate a matching UsdPreviewSurface network for export compatibility? Or only on explicit "Export" action? Auto-gen adds complexity but prevents "my material looks wrong in usdview" surprises.

2. **MaterialX scope:** v0.21.0 says "standard_surface graph, XML round-trip." Should the material graph also support arbitrary MaterialX nodes beyond standard_surface (e.g., custom OSL, displacement networks)? Or keep it curated?

3. **Preview environment:** Should the lookdev preview orb use the scene's HDRI, a fixed studio HDRI, or let the user pick? Scene HDRI is more realistic but changes preview when you swap environments.

4. **Performance budget for preview:** 192px sphere at 1 SPP should be ~16ms on modern CPU. Is this acceptable for interactive drag? Or do we need a GPU rasterized fallback for the preview?

5. **Material library / presets:** Should BIF ship with preset materials (plastic, metal, glass, skin, etc.) that users can start from? Where do these live — embedded assets or a user-editable library folder?

6. **Graph persistence in USD:** Material graph layout (node positions, zoom) is UI state, not USD data. Store in `.bif` project file alongside scene graph layout? Or a sidecar `.biflayout` file?

7. **egui vs Qt timing:** The parameter sheet (Phase 1) ships in v0.16.0 which is still egui. The full material graph (Phase 3-4) ships after Qt migration. Should the parameter sheet be designed egui-first and ported, or should we wait for Qt for the graph and build it once?
