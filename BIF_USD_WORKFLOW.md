# BIF USD Workflow Foundation

**Version:** 0.2.0
**Last Updated:** 2026-03-28
**Status:** Design specification — hybrid approach adopted (see Implementation Notes)

### Implementation Notes

**Hybrid approach (decided 2026-03-28):** BIF keeps its procedural node graph (scatter, instancer, etc.) as a differentiator while adding layer awareness underneath. This is not a full pivot to "layer editor only" — it's an evolution where edits author USD opinions continuously instead of only at export time. The existing data-flow node graph gains blue/orange color-coding (composition vs operation nodes) as a visual UX distinction, not an architectural rewrite.

**Shot templates:** Implemented as JSON-configurable presets stored in `~/.bif/templates/`. Users can add/edit/remove templates by editing JSON. Studios can override via `BIF_TEMPLATE_DIR` env var. Built-in presets (feature_film, commercial, lookdev) ship as defaults.

**Milestone threading:** This spec is implemented incrementally across v0.14-v0.19 — see [MILESTONES.md](MILESTONES.md) for the release schedule.

## Overview

BIF is a **layer-aware USD editor and scene assembler** — not a USD viewer, not a version control system, not a modeling tool. BIF fills a gap in the VFX pipeline: no tool today lets an artist open a master USD stage, see the layer stack, pick their working layer, selectively load only what they need, make edits that author clean USD opinions, and save back to disk — all with a visual node graph and live USDA preview.

### Design Philosophy

- **Be the best USD layer editor** — leave version control to ShotGrid, ftrack, git, etc.
- **Every edit authors USD** — no proprietary format, everything round-trips through `.usd` files
- **Load only what you need** — selective payload loading driven by the artist's task
- **Non-destructive by default** — edits go on the artist's layer, source data is never modified
- **Three views of one truth** — node graph, stage tree, and USDA code all show the same data

### Target Pipeline Position

```
Houdini/Maya (author assets as USD)
        │
        ▼
   ┌─────────┐
   │   BIF   │  ← Scene assembly, instancing, material overrides,
   │         │    lighting, shot finaling, rendering
   └────┬────┘
        │
        ▼
  USD on disk → Nuke/Comp (via EXR renders)
                 or farm submission
```

---

## Core Concepts

### The Master Stage

Every BIF session starts with a root USD stage file. This is the master scene file that references all departments' work through USD's composition arcs (sublayers, references, payloads, variants).

```
shot_010.usd  (master stage — opened in BIF)
│
├── sublayer: layout.usd        (from Layout dept — read-only in BIF)
├── sublayer: animation.usd     (from Anim dept — read-only in BIF)
├── sublayer: fx.usd            (from FX dept — read-only in BIF)
├── sublayer: lighting.usd      ← ARTIST'S EDIT LAYER (read-write)
│
├── reference: /assets/hero_char.usd     (payload)
├── reference: /assets/env_forest.usd    (payload)
└── reference: /assets/props/*.usd       (payloads)
```

The artist opens `shot_010.usd`, BIF shows them the full layer stack. They pick `lighting.usd` as their working layer. BIF loads only the payloads relevant to lighting work. They make edits — all authored as USD opinions on `lighting.usd`. They hit save, and that `.usd` file on disk is updated. Upstream tools see the change immediately.

### Layer Isolation

When an artist selects their working layer, BIF enters **layer isolation mode**:

- **Their layer**: fully editable, all edits write here
- **Other layers**: visible but locked — shown ghosted/dimmed in the viewport
- **Override indicators**: parameters changed by the artist's layer show bold/colored in the UI
- **Inherited values**: parameters coming from lower layers show normal weight

This prevents accidental edits to other departments' work. The artist can only break their own layer.

### Selective Payload Loading

USD payloads are the mechanism for deferred loading — a payload reference says "this geometry exists here, but don't load it until asked." BIF uses payload policies to control what's in memory:

```rust
/// Controls what geometry is loaded into memory for a given context.
#[derive(Clone, Debug)]
pub enum PayloadPolicy {
    /// Load everything — used for final renders
    LoadAll,
    
    /// Load only what the camera can see — used for layout/lighting
    CameraFrustum {
        camera_path: String,
        padding: f32,          // frustum expansion percentage
    },
    
    /// Load only bounding boxes — used for navigation/overview
    BoundingBoxOnly,
    
    /// Load nothing — empty starting layer
    None,
    
    /// Custom: artist manually picks what to load/unload
    Manual {
        loaded_prims: Vec<String>,
    },
}
```

**Task-driven loading**: BIF can infer what to load based on which layer the artist is editing. A lighting artist needs all geo visible to camera + light rigs. A layout artist needs everything but can tolerate lower LODs. An FX artist needs specific areas.

---

## Shot Templating

BIF provides a shot template system for quickly bootstrapping USD-based shots. This writes real `.usd` files to disk — immediately usable by any USD tool, not just BIF.

### Template Definition

```rust
/// A shot template defines the layer structure for a new shot.
pub struct ShotTemplate {
    pub name: String,
    pub description: String,
    pub layers: Vec<LayerTemplate>,
    pub default_references: Vec<ReferenceTemplate>,
}

pub struct LayerTemplate {
    pub name: String,              // "layout", "animation", "lighting", "fx"
    pub department: String,        // for UI grouping
    pub default_prims: Vec<String>, // pre-populate with common roots like "/world"
    pub payload_policy: PayloadPolicy,
    pub layer_order: u32,          // position in sublayer stack (LIVRPS strength)
}

pub struct ReferenceTemplate {
    pub prim_path: String,         // where to place the reference
    pub asset_path: String,        // USD file to reference
    pub as_payload: bool,          // true = deferred loading
}
```

### Built-in Presets

```rust
impl ShotTemplate {
    /// Standard VFX shot with typical department layers
    pub fn feature_film_shot(shot_name: &str) -> Self {
        Self {
            name: shot_name.to_string(),
            description: "Standard VFX shot with department layers".into(),
            layers: vec![
                LayerTemplate {
                    name: format!("{}_layout.usd", shot_name),
                    department: "Layout".into(),
                    default_prims: vec!["/world".into(), "/cameras".into()],
                    payload_policy: PayloadPolicy::CameraFrustum {
                        camera_path: "/cameras/main".into(),
                        padding: 0.2,
                    },
                    layer_order: 0, // weakest
                },
                LayerTemplate {
                    name: format!("{}_animation.usd", shot_name),
                    department: "Animation".into(),
                    default_prims: vec!["/world/characters".into()],
                    payload_policy: PayloadPolicy::CameraFrustum {
                        camera_path: "/cameras/main".into(),
                        padding: 0.1,
                    },
                    layer_order: 1,
                },
                LayerTemplate {
                    name: format!("{}_fx.usd", shot_name),
                    department: "FX".into(),
                    default_prims: vec!["/world/fx".into()],
                    payload_policy: PayloadPolicy::BoundingBoxOnly,
                    layer_order: 2,
                },
                LayerTemplate {
                    name: format!("{}_lighting.usd", shot_name),
                    department: "Lighting".into(),
                    default_prims: vec!["/world/lights".into()],
                    payload_policy: PayloadPolicy::LoadAll,
                    layer_order: 3, // strongest — lighting overrides win
                },
            ],
            default_references: vec![],
        }
    }

    /// Lightweight commercial/previz shot
    pub fn commercial() -> Self {
        Self {
            name: "commercial".into(),
            description: "Simple setup for commercials and previz".into(),
            layers: vec![
                LayerTemplate {
                    name: "scene.usd".into(),
                    department: "Scene".into(),
                    default_prims: vec!["/world".into()],
                    payload_policy: PayloadPolicy::LoadAll,
                    layer_order: 0,
                },
                LayerTemplate {
                    name: "overrides.usd".into(),
                    department: "Overrides".into(),
                    default_prims: vec![],
                    payload_policy: PayloadPolicy::LoadAll,
                    layer_order: 1,
                },
            ],
            default_references: vec![],
        }
    }
    
    /// Lookdev template — single asset, all layers for material iteration
    pub fn lookdev(asset_name: &str) -> Self {
        Self {
            name: format!("{}_lookdev", asset_name),
            description: "Lookdev setup for material authoring".into(),
            layers: vec![
                LayerTemplate {
                    name: format!("{}_base.usd", asset_name),
                    department: "Model".into(),
                    default_prims: vec![format!("/{}", asset_name)],
                    payload_policy: PayloadPolicy::LoadAll,
                    layer_order: 0,
                },
                LayerTemplate {
                    name: format!("{}_materials.usd", asset_name),
                    department: "Lookdev".into(),
                    default_prims: vec!["/materials".into()],
                    payload_policy: PayloadPolicy::LoadAll,
                    layer_order: 1,
                },
            ],
            default_references: vec![],
        }
    }
}
```

### Template Execution: Writing USD Files

```rust
impl ShotTemplate {
    /// Create the actual USD files on disk from this template.
    pub fn create_on_disk(&self, output_dir: &Path) -> Result<PathBuf> {
        std::fs::create_dir_all(output_dir)?;

        // Write each layer as an empty .usda file with default prims
        let mut layer_paths = Vec::new();
        for layer in &self.layers {
            let layer_path = output_dir.join(&layer.name);
            let mut usda = String::new();
            usda.push_str("#usda 1.0\n(\n");
            usda.push_str(&format!("    doc = \"BIF {} layer\"\n", layer.department));
            usda.push_str(")\n\n");

            for prim in &layer.default_prims {
                let prim_name = prim.trim_start_matches('/');
                usda.push_str(&format!("def Xform \"{}\" {{\n}}\n\n", prim_name));
            }

            std::fs::write(&layer_path, &usda)?;
            layer_paths.push(layer_path);
        }

        // Write the root stage that sublayers everything
        let root_path = output_dir.join(format!("{}.usd", self.name));
        let mut root_usda = String::new();
        root_usda.push_str("#usda 1.0\n(\n");
        root_usda.push_str("    subLayers = [\n");

        // Strongest layer first in sublayer list (USD LIVRPS: first sublayer wins)
        for layer in self.layers.iter().rev() {
            root_usda.push_str(&format!("        @./{}@,\n", layer.name));
        }
        root_usda.push_str("    ]\n)\n\n");

        // Add default references
        for ref_template in &self.default_references {
            let prim_name = ref_template.prim_path.trim_start_matches('/');
            if ref_template.as_payload {
                root_usda.push_str(&format!(
                    "def Xform \"{}\" (\n    payload = @{}@\n) {{\n}}\n\n",
                    prim_name, ref_template.asset_path
                ));
            } else {
                root_usda.push_str(&format!(
                    "def \"{}\" (\n    references = @{}@\n) {{\n}}\n\n",
                    prim_name, ref_template.asset_path
                ));
            }
        }

        std::fs::write(&root_path, &root_usda)?;

        Ok(root_path)
    }
}
```

### Template UI

The template form is one of the first things an artist sees when creating a new shot:

```
┌─────────────────────────────────────────────────┐
│  New Shot from Template                         │
├─────────────────────────────────────────────────┤
│                                                 │
│  Template:  [Feature Film Shot  ▾]              │
│  Shot Name: [sh010_______________]              │
│  Output:    [/show/seq01/sh010/  ] [Browse]     │
│                                                 │
│  Layers:                                        │
│  ┌─────────────────────────────────────────┐    │
│  │ ☑ layout.usd      (Layout)     [weak]  │    │
│  │ ☑ animation.usd   (Animation)          │    │
│  │ ☑ fx.usd          (FX)                 │    │
│  │ ☑ lighting.usd    (Lighting)   [strong]│    │
│  │ [+ Add Layer]                           │    │
│  └─────────────────────────────────────────┘    │
│                                                 │
│  Asset References:                              │
│  ┌─────────────────────────────────────────┐    │
│  │ /world/hero  ← hero_char.usd  [payload]│    │
│  │ /world/env   ← env_forest.usd [payload]│    │
│  │ [+ Add Reference]                       │    │
│  └─────────────────────────────────────────┘    │
│                                                 │
│             [Cancel]  [Create Shot]             │
└─────────────────────────────────────────────────┘
```

---

## Edit Operations

Every edit the artist makes in BIF is an `EditOperation`. Each operation knows how to author itself as USD on the active layer. This is the bridge between the UI and the `.usd` files on disk.

### Operation Types

```rust
/// Every user action that modifies the scene is an EditOperation.
/// Operations are recorded, undoable, and know how to write themselves as USD.
#[derive(Clone, Debug)]
pub enum EditOperation {
    // --- Transforms ---
    Transform {
        prim_path: String,
        matrix: Mat4,
    },

    // --- Vertex Edits (sparse) ---
    /// Move specific vertices without modifying the entire mesh.
    /// USD stores this as a points override on the edit layer.
    /// Only the changed verts are written — not the full mesh.
    PointEdit {
        prim_path: String,
        edits: Vec<(u32, Vec3)>,  // (vertex_index, new_position)
    },

    // --- Material Operations ---
    MaterialAssign {
        prim_path: String,
        material_path: String,
    },
    MaterialParamOverride {
        material_path: String,
        param_name: String,
        value: ParamValue,
    },
    MaterialCreate {
        material_path: String,
        material: OpenPbrMaterial,
    },

    // --- Visibility ---
    Visibility {
        prim_path: String,
        visible: bool,
    },

    // --- Animation (simple keyframe overrides) ---
    /// Override a value at a specific frame. Not a full animation system —
    /// just "I need this light to dim at frame 48."
    AnimKey {
        prim_path: String,
        attribute: String,
        time: f64,
        value: ParamValue,
    },

    // --- Instance Scattering (BIF's core feature) ---
    /// Scatter instances of a prototype across a surface.
    /// Authors a UsdGeomPointInstancer on the edit layer.
    ScatterInstances {
        instancer_path: String,
        prototype_paths: Vec<String>,
        transforms: Vec<Mat4>,
    },

    // --- Payload Control ---
    PayloadLoad {
        prim_path: String,
    },
    PayloadUnload {
        prim_path: String,
    },

    // --- Layer Management ---
    /// Add a new sublayer to the stage
    AddSubLayer {
        layer_path: String,
        position: LayerPosition,
    },
}

#[derive(Clone, Debug)]
pub enum ParamValue {
    Float(f32),
    Vec3(Vec3),
    Color3(Vec3),
    String(String),
    Bool(bool),
    Matrix4(Mat4),
}

#[derive(Clone, Debug)]
pub enum LayerPosition {
    Strongest,
    Weakest,
    Above(String),  // above this layer
    Below(String),
}
```

### USD Text Output

Every operation can serialize itself to USDA text. This powers both the live code preview panel and the actual file writing.

```rust
impl EditOperation {
    /// Generate the USD text representation of this operation.
    /// Used for: live USDA preview, writing to layer files, debugging.
    pub fn to_usda(&self) -> String {
        match self {
            EditOperation::Transform { prim_path, matrix } => {
                let (scale, rotation, translation) = decompose_matrix(matrix);
                let mut usda = format!("over \"{}\" {{\n", trim_path(prim_path));
                usda.push_str(&format!(
                    "    double3 xformOp:translate = ({}, {}, {})\n",
                    translation.x, translation.y, translation.z
                ));
                if rotation != Quat::IDENTITY {
                    usda.push_str(&format!(
                        "    quatd xformOp:orient = ({}, {}, {}, {})\n",
                        rotation.w, rotation.x, rotation.y, rotation.z
                    ));
                }
                if scale != Vec3::ONE {
                    usda.push_str(&format!(
                        "    double3 xformOp:scale = ({}, {}, {})\n",
                        scale.x, scale.y, scale.z
                    ));
                }
                usda.push_str("    uniform token[] xformOpOrder = [");
                usda.push_str("\"xformOp:translate\", \"xformOp:orient\", \"xformOp:scale\"");
                usda.push_str("]\n}\n");
                usda
            }

            EditOperation::PointEdit { prim_path, edits } => {
                // Sparse vertex override — only changed verts
                let mut usda = format!("over \"{}\" {{\n", trim_path(prim_path));
                usda.push_str("    # Sparse vertex edits from BIF\n");
                // In practice, USD requires writing the full points array
                // with only the changed values modified. BIF handles this
                // by reading the base points, applying edits, and writing
                // the full array as an override.
                usda.push_str("    point3f[] points = [ ... ] # modified\n");
                usda.push_str("}\n");
                usda
            }

            EditOperation::MaterialAssign { prim_path, material_path } => {
                format!(
                    "over \"{}\" {{\n    rel material:binding = <{}>\n}}\n",
                    trim_path(prim_path), material_path
                )
            }

            EditOperation::MaterialParamOverride { material_path, param_name, value } => {
                format!(
                    "over \"{}\" {{\n    {} = {}\n}}\n",
                    trim_path(material_path), param_name, value.to_usda()
                )
            }

            EditOperation::Visibility { prim_path, visible } => {
                let vis = if *visible { "\"inherited\"" } else { "\"invisible\"" };
                format!(
                    "over \"{}\" {{\n    token visibility = {}\n}}\n",
                    trim_path(prim_path), vis
                )
            }

            EditOperation::AnimKey { prim_path, attribute, time, value } => {
                format!(
                    "over \"{}\" {{\n    {} .timeSamples = {{\n        {}: {},\n    }}\n}}\n",
                    trim_path(prim_path), attribute, time, value.to_usda()
                )
            }

            EditOperation::ScatterInstances { instancer_path, prototype_paths, transforms } => {
                let mut usda = format!("def PointInstancer \"{}\" {{\n", trim_path(instancer_path));
                
                // Prototype references
                usda.push_str("    rel prototypes = [\n");
                for proto in prototype_paths {
                    usda.push_str(&format!("        <{}>,\n", proto));
                }
                usda.push_str("    ]\n");

                // Decompose transforms into positions, orientations, scales
                let mut positions = Vec::new();
                let mut orientations = Vec::new();
                let mut scales = Vec::new();
                let mut proto_indices = Vec::new();

                for (i, xform) in transforms.iter().enumerate() {
                    let (s, r, t) = decompose_matrix(xform);
                    positions.push(format!("({}, {}, {})", t.x, t.y, t.z));
                    orientations.push(format!("({}, {}, {}, {})", r.w, r.x, r.y, r.z));
                    scales.push(format!("({}, {}, {})", s.x, s.y, s.z));
                    proto_indices.push("0"); // TODO: multi-prototype support
                }

                usda.push_str(&format!("    point3f[] positions = [{}]\n", positions.join(", ")));
                usda.push_str(&format!("    quath[] orientations = [{}]\n", orientations.join(", ")));
                usda.push_str(&format!("    float3[] scales = [{}]\n", scales.join(", ")));
                usda.push_str(&format!("    int[] protoIndices = [{}]\n", proto_indices.join(", ")));
                usda.push_str("}\n");
                usda
            }

            _ => "# TODO: not yet implemented\n".to_string(),
        }
    }
}

impl ParamValue {
    pub fn to_usda(&self) -> String {
        match self {
            ParamValue::Float(v) => format!("{}", v),
            ParamValue::Vec3(v) => format!("({}, {}, {})", v.x, v.y, v.z),
            ParamValue::Color3(v) => format!("({}, {}, {})", v.x, v.y, v.z),
            ParamValue::String(s) => format!("\"{}\"", s),
            ParamValue::Bool(b) => if *b { "true".into() } else { "false".into() },
            ParamValue::Matrix4(m) => {
                let cols = m.to_cols_array();
                format!("( ({}, {}, {}, {}), ({}, {}, {}, {}), ({}, {}, {}, {}), ({}, {}, {}, {}) )",
                    cols[0], cols[1], cols[2], cols[3],
                    cols[4], cols[5], cols[6], cols[7],
                    cols[8], cols[9], cols[10], cols[11],
                    cols[12], cols[13], cols[14], cols[15])
            }
        }
    }
}

fn trim_path(path: &str) -> &str {
    path.trim_start_matches('/')
}
```

### Undo/Redo Stack

Operations are stored in an undo stack. Undo removes the operation from the active layer; redo re-applies it.

```rust
pub struct EditHistory {
    operations: Vec<EditOperation>,
    undo_stack: Vec<EditOperation>,
    redo_stack: Vec<EditOperation>,
    active_layer: String,
}

impl EditHistory {
    pub fn apply(&mut self, op: EditOperation) {
        // 1. Apply to the scene graph in memory
        // 2. Record for undo
        self.undo_stack.push(op.clone());
        self.redo_stack.clear();
        self.operations.push(op);
    }

    pub fn undo(&mut self) -> Option<EditOperation> {
        let op = self.undo_stack.pop()?;
        self.redo_stack.push(op.clone());
        Some(op)
    }

    pub fn save_to_layer(&self, layer_path: &Path) -> Result<()> {
        // Combine all operations into a single USDA layer file
        let mut usda = String::new();
        usda.push_str("#usda 1.0\n(\n");
        usda.push_str("    doc = \"BIF edit layer\"\n");
        usda.push_str(")\n\n");

        for op in &self.operations {
            usda.push_str(&op.to_usda());
            usda.push_str("\n");
        }

        std::fs::write(layer_path, &usda)?;
        Ok(())
    }
}
```

---

## The Three-Panel UI

BIF's editor shows three synchronized views of the same scene data. Each view is optimized for a different way of thinking about the scene.

### Layout

```
┌─────────────────────────────────────────────────────────────────┐
│  [File] [Edit] [View] [Render]    BIF - shot_010.usd           │
├───────────────────────┬─────────────────┬───────────────────────┤
│                       │                 │                       │
│  NODE GRAPH           │  STAGE TREE     │  USD CODE PREVIEW     │
│                       │                 │                       │
│  Composition +        │  Prim hierarchy │  Live USDA text of    │
│  operations flow      │  (USD stage)    │  the active edit      │
│                       │                 │  layer                │
│  [Ref]→[MatAssign]   │  /world         │                       │
│     │                 │   /hero_char    │  #usda 1.0            │
│  [Ref]→[Scatter]     │   /env          │  over "hero_char" {   │
│     │                 │    /trees       │    rel material:...   │
│  [SubLayer]→[Light]  │    /ground      │  }                    │
│     │                 │   /lights       │                       │
│     ▼                 │    /key_light   │  over "lights" {      │
│  [Edit Layer]         │    /fill_light  │    ...                │
│                       │                 │  }                    │
│                       │                 │                       │
├───────────────────────┴─────────────────┴───────────────────────┤
│                                                                 │
│                          VIEWPORT                               │
│                                                                 │
│         Shows composed result — all layers, all payloads        │
│         Interactive: 60 FPS rasterized                          │
│         Idle: progressive path traced refinement                │
│                                                                 │
│  [Working Layer: lighting.usd ▾]  [Payload: Camera Frustum ▾]  │
└─────────────────────────────────────────────────────────────────┘
```

### Node Graph: Composition + Operations

The node graph shows **how the scene is assembled**. There are two categories of nodes, visually distinguished by color:

**Composition Nodes (blue)** — structural, define what's in the scene:

- **USD Reference**: references an external `.usd` asset
- **Payload Gate**: load/unload toggle for deferred geometry
- **SubLayer**: represents a sublayer in the stack
- **Layer Stack**: shows the full sublayer ordering with strength

**Operation Nodes (orange)** — edits, author opinions on the edit layer:

- **Material Assign**: bind a material to a prim
- **Material Override**: tweak a material parameter
- **Transform Edit**: move/rotate/scale a prim
- **Point Edit**: nudge vertices
- **Scatter Instances**: BIF's core scattering tool
- **Visibility**: show/hide prims
- **Anim Key**: keyframe a value at a time

The key insight: **both node types do the same thing under the hood** — they author USD opinions. A material assignment is just `material:binding` on your layer. A transform edit is just `xformOp:translate` on your layer. The nodes are a friendlier way to visualize and build up a layer file.

```
COMPOSITION FLOW:

[USD Stage Root]
    │
    ├─[Reference: hero_char.usd]──→ [Material Override]──→ [Transform]
    │     (blue)                        (orange)              (orange)
    │                                  assigns new mtl       offset position
    │
    ├─[Reference: env_forest.usd]──→ [Scatter Instances]
    │     (blue)                        (orange)
    │                                  BIF's core feature
    │
    └─[SubLayer: lighting.usd]──→ [Light Edit]
          (blue)                     (orange)
                                    tweak intensity

         ALL operations author to → [Your Edit Layer] → saved as .usd
```

### Stage Tree: USD Prim Hierarchy

The tree view shows **what's in the scene** — the composed USD prim hierarchy. This maps directly to what you'd see in `usdview` or Houdini's Scene Graph Tree.

Features:

- **Prim icons**: Xform, Mesh, Material, Light, Camera, PointInstancer
- **Payload indicators**: loaded (solid), unloaded (hollow), partially loaded (half)
- **Layer indicators**: which layer a prim's opinions come from (colored dots)
- **Override markers**: bold name if the active layer has opinions on this prim
- **Right-click menu**: load/unload payload, assign material, hide, select in viewport

### USD Code Preview: Live USDA Output

The code panel shows the USDA text of the **active edit layer** in real-time. As the artist works — dragging a transform handle, assigning a material, tweaking a light — the code updates live.

This serves three purposes:

1. **Teaches artists USD** — they see the code their actions produce, building understanding over time
2. **Lets TDs debug** — no need to open `usdview` to check what BIF is doing to the files
3. **Builds trust** — the artist can see exactly what will be saved

Example of what the panel shows while working:

```usda
#usda 1.0
(
    doc = "BIF edit layer"
    subLayers = [
        @./lighting.usd@
    ]
)

over "hero_char" {
    # From Material Override node
    rel material:binding = </materials/hero_wet>

    # From Transform node
    double3 xformOp:translate = (0, 0.5, 0)
    uniform token[] xformOpOrder = ["xformOp:translate"]
}

over "lights" {
    over "key_light" {
        # From Anim Key node
        float inputs:intensity.timeSamples = {
            1: 500.0,
            48: 200.0,
        }
    }
}

def PointInstancer "scattered_trees" {
    # From Scatter Instances node
    rel prototypes = [</assets/tree_oak>]
    point3f[] positions = [(10, 0, 5), (12, 0, 8), ...]
    int[] protoIndices = [0, 0, ...]
}
```

---

## Render Context

BIF has two rendering modes that share the same scene but load data differently.

### Working Mode (Viewport)

The viewport shows a selective view of the scene for interactive work:

```
Working Mode:
  ├── Camera frustum geometry: fully loaded as prototypes
  ├── Near-camera instances: full materials, textured
  ├── Background geometry: bounding boxes only (wireframe)
  ├── Unloaded payloads: invisible (not in memory)
  └── Lights: all loaded (they're small)
```

This is controlled by the `PayloadPolicy` on the working layer. The viewport uses the GPU rasterizer at 60 FPS, upgrading to hybrid then progressive path tracing when the artist stops interacting (see `RenderCoordinator` in the GPU rendering architecture doc).

### Render Mode

When the artist initiates a render (or the progressive mode reaches production quality), BIF creates a `RenderContext` that resolves the full scene:

```rust
/// Render context: knows the full scene, loads geometry on demand.
pub struct RenderContext {
    /// Root USD stage path
    stage_path: PathBuf,
    
    /// Which layer the artist is working on (for override evaluation)
    working_layer: String,
    
    /// What's loaded in the viewport (for cache warming)
    viewport_policy: PayloadPolicy,
    
    /// On-demand prototype loading with LRU eviction
    prototype_cache: LruCache<PrototypeId, Arc<Mesh>>,
    
    /// Memory budget — evict prototypes when exceeded
    max_memory_bytes: usize,
    current_memory_bytes: usize,
}
```

The render flow uses BIF's prototype/instance architecture for memory efficiency:

```
Render Mode:
  ├── ALL instance bounding boxes loaded (cheap: just transforms)
  │     → Top-level Embree BVH built from bounding boxes
  │
  ├── Prototype geometry: loaded ON DEMAND as rays hit bounding boxes
  │     → First ray hits tree bounding box → load tree mesh → cache it
  │     → LRU eviction when memory budget exceeded
  │
  ├── Materials: resolved from composed USD stage
  │     → OpenPBR evaluation (see material authoring doc)
  │
  └── Textures: streamed via OIIO (mip-mapped .tx files)
```

### Prototype Loading States

```rust
/// A prototype's geometry can be in one of three states.
/// This enables rendering scenes that don't fit in memory.
pub enum PrototypeState {
    /// Viewport mode: just a bounding box for culling/wireframe display
    BoundingBox(AABB),

    /// Fully loaded: mesh data in memory, ready to intersect/render
    Loaded {
        mesh: Arc<Mesh>,
        embree_geom_id: u32,  // registered with Embree
    },

    /// Render mode: knows where the data lives, loads on first ray hit
    Deferred {
        usd_prim_path: String,
        bounds: AABB,
        estimated_memory_bytes: usize,
    },
}

impl PrototypeState {
    /// Called when a ray hits this prototype's bounding box.
    /// Transitions Deferred → Loaded by reading from USD.
    pub fn ensure_loaded(
        &mut self,
        usd_stage: &UsdStage,
        embree_scene: &mut EmbreeScene,
        cache: &mut LruCache<PrototypeId, Arc<Mesh>>,
    ) -> Result<&Arc<Mesh>> {
        match self {
            PrototypeState::Loaded { mesh, .. } => Ok(mesh),
            PrototypeState::Deferred { usd_prim_path, estimated_memory_bytes, .. } => {
                // Load mesh from USD
                let mesh = usd_stage.load_mesh(usd_prim_path)?;
                let mesh = Arc::new(mesh);

                // Register with Embree for ray intersection
                let geom_id = embree_scene.add_triangle_mesh(
                    &mesh.vertices, &mesh.indices
                );

                // Cache management
                cache.put(/* id */, mesh.clone());

                *self = PrototypeState::Loaded {
                    mesh: mesh.clone(),
                    embree_geom_id: geom_id,
                };

                Ok(&mesh)
            }
            PrototypeState::BoundingBox(_) => {
                anyhow::bail!("Cannot load: no USD prim path (viewport-only prototype)")
            }
        }
    }
}
```

This is the Clarisse-inspired approach: the scene can contain millions of instances referencing thousands of prototypes, but only the prototypes actually hit by rays need to be in memory. The LRU cache evicts prototypes that haven't been hit recently, keeping memory bounded.

---

## Vertex Editing

BIF is not a modeling tool. But artists need to nudge vertices when a prop clips through the ground or two objects intersect awkwardly. BIF provides minimal vertex editing that authors sparse USD overrides.

### What It Is

- Select a mesh in the viewport
- Switch to point edit mode
- Drag vertices with a soft-select falloff
- BIF writes a `points` override on the edit layer

### What It Is NOT

- No edge/face operations
- No topology changes (no extrude, bevel, subdivide)
- No sculpting
- No UV editing

### How It Authors USD

USD's `points` attribute is an array. Unfortunately, USD doesn't support sparse array overrides — you have to write the full array. BIF handles this by:

1. Reading the base `points` array from the composed stage
2. Applying the artist's vertex edits
3. Writing the full modified array as an override on the edit layer

```rust
impl EditOperation {
    /// Apply sparse vertex edits and generate the full points override.
    pub fn resolve_point_edit(
        &self,
        usd_stage: &UsdStage,
    ) -> Result<Vec<Vec3>> {
        if let EditOperation::PointEdit { prim_path, edits } = self {
            // Read base points from composed stage
            let mut points = usd_stage.read_points(prim_path)?;

            // Apply sparse edits
            for (index, new_pos) in edits {
                if (*index as usize) < points.len() {
                    points[*index as usize] = *new_pos;
                }
            }

            Ok(points)
        } else {
            anyhow::bail!("Not a PointEdit operation")
        }
    }
}
```

The edit layer stores the full points array, but BIF internally tracks which vertices were actually changed (for undo, display of edit markers, etc.).

---

## Animation Overrides

BIF provides simple keyframe overrides — not a full animation system. The use case is "I need this light to fade at frame 48" or "I want this prop to slide over 10 frames." For complex character animation, use Maya/Houdini.

### What BIF Supports

- Keyframing scalar values (float, vec3, color) at specific frames
- Linear interpolation between keys
- USD `timeSamples` output

### USD Output

```usda
over "lights" {
    over "key_light" {
        float inputs:intensity.timeSamples = {
            1: 500.0,
            24: 500.0,
            48: 200.0,
            72: 500.0,
        }
    }
}

over "props" {
    over "sliding_door" {
        double3 xformOp:translate.timeSamples = {
            1: (0, 0, 0),
            24: (3.5, 0, 0),
        }
    }
}
```

---

## Layer Diffing

BIF can show the artist exactly what their edit layer changes compared to the composed base scene. This is a visual diff — not a text diff of USDA files, but a semantic diff of USD opinions.

### Diff Display

```
Layer Diff: lighting.usd vs composed base
──────────────────────────────────────────

MODIFIED:
  /hero_char
    material:binding  was: </materials/hero_dry>
                      now: </materials/hero_wet>
    xformOp:translate was: (0, 0, 0)
                      now: (0, 0.5, 0)

  /lights/key_light
    inputs:intensity  was: 500.0
                      now: 200.0 (at frame 48)

ADDED:
  /scattered_trees    (PointInstancer, 847 instances)

HIDDEN:
  /env/trees/tree_042  visibility: invisible
```

This tells the artist: "here's everything your layer does to the scene." Invaluable for reviews, debugging, and understanding what will be saved.

---

## File I/O and Save Workflow

### Save Behavior

When the artist hits Save (Ctrl+S):

1. BIF collects all `EditOperation`s on the active layer
2. Generates the USDA text for each operation
3. Writes to the layer `.usd` file on disk
4. The master stage file is NOT modified (it just references the layers)

This means saving is fast (only writes one layer file) and safe (other departments' layers are untouched).

### Auto-Save

BIF auto-saves to a temporary file (`.bif_autosave_lighting.usd`) on a timer. On crash recovery, BIF offers to restore from the auto-save.

### File Watching

BIF watches the master stage's sublayer files for changes. If another artist saves to `animation.usd` while you're working on `lighting.usd`, BIF detects the change and offers to reload. This is not live collaboration — it's file-system-level change detection, like how IDEs reload files modified externally.

---

## Implementation Phases

### Phase 1: Layer-Aware Stage Loading (Weeks 1-3)

**What to build:**

- Open a USD stage via C++ FFI bridge
- Parse the sublayer stack
- Display the layer list in the UI
- Let the artist select a working layer
- Implement `PayloadPolicy::LoadAll` and `PayloadPolicy::BoundingBoxOnly`

**Validation:** Open a multi-layer USD stage from Houdini, see the layer stack, toggle layers on/off.

### Phase 2: Edit Operations + USDA Output (Weeks 4-6)

**What to build:**

- `EditOperation` enum with `to_usda()` for all types
- `EditHistory` with undo/redo
- Save to layer file on disk
- Live USDA code preview panel

**Validation:** Make edits in BIF, save, open the saved layer in `usdview`, see the edits composed correctly.

### Phase 3: Shot Templating (Week 7)

**What to build:**

- `ShotTemplate` struct and built-in presets
- Template creation UI (the form shown above)
- `create_on_disk()` to write `.usd` files

**Validation:** Create a shot from template, open in `usdview`, verify layer structure.

### Phase 4: Node Graph + Stage Tree (Weeks 8-12)

**What to build:**

- Stage tree showing prim hierarchy with payload/layer indicators
- Node graph showing composition arcs + operation nodes
- Synchronization: selecting in one view highlights in the others
- Right-click menus for common operations

**Validation:** Navigate a complex USD stage using both views, perform operations via nodes, see results in all three panels.

### Phase 5: Render Context (Weeks 13-16)

**What to build:**

- `RenderContext` with on-demand prototype loading
- `PrototypeState` enum with `BoundingBox` / `Loaded` / `Deferred`
- LRU cache for prototype eviction
- Integration with `RenderCoordinator` for viewport → progressive transition

**Validation:** Open a scene with more geometry than fits in memory, render it, verify LRU eviction works.

### Phase 6: Selective Payload Loading (Weeks 17-18)

**What to build:**

- `PayloadPolicy::CameraFrustum` with frustum culling
- `PayloadPolicy::Manual` with artist-driven load/unload
- Task-driven inference: suggest what to load based on active layer
- UI for payload management in stage tree

**Validation:** Open a heavy shot, load only camera-visible payloads, verify memory usage stays bounded.

### Phase 7: Layer Diffing + Vertex Editing (Weeks 19-22)

**What to build:**

- Layer diff computation: compare edit layer opinions vs composed base
- Diff display panel
- Point edit mode: soft-select, drag vertices, author `points` override
- Animation keyframing: simple key at time, linear interp, `timeSamples` output

**Validation:** Make various edits, verify diff shows exactly what changed, verify point edits round-trip through USD.

---

## Key Design Decisions

### BIF Edits = USD Opinions, Always

Every action the artist takes in BIF produces valid USD. There is no proprietary scene format. If BIF disappears tomorrow, the artist's work is still readable by any USD tool. This is the single most important design decision for open-source adoption.

### Layer Editor, Not Version Control

BIF doesn't track history of who changed what when. That's ShotGrid/ftrack/git territory. BIF is the best tool for authoring a single layer at a time. Studios integrate BIF into their existing asset management by having their pipeline set which layer file the artist edits and where it lives on disk.

### Prototype/Instance Split Enables Deferred Loading

The reason BIF can render scenes that don't fit in memory is that the instance list (transforms + prototype IDs) is tiny compared to the prototype geometry. Loading 1M instances at 64 bytes each = 64MB. The actual meshes they reference can be loaded on demand. This is architecturally clean because BIF was designed around instances from the start.

### Node Graph Shows Flow, Tree Shows Hierarchy

These are complementary, not redundant. The node graph answers "how is this scene built?" (composition + operations). The tree answers "what's in this scene?" (prim hierarchy). Artists switch between them depending on their task. The USDA preview answers "what exactly am I writing to disk?"

### Minimal Vertex Editing Is Intentional

BIF is not a modeler. Adding face/edge operations, topology changes, or sculpting would massively expand scope without serving BIF's core purpose (scene assembly). The vertex nudge feature exists because it's a common lighting/layout need ("this rock clips through the ground") and it maps cleanly to a USD `points` override. If artists need real modeling, they do it in Houdini/Maya/Blender and bring the result back as USD.

---

## Glossary

| Term | Meaning in BIF |
|------|----------------|
| **Master Stage** | The root `.usd` file that sublayers all department layers |
| **Edit Layer** | The `.usd` sublayer the artist is currently writing to |
| **Working Layer** | Same as edit layer — the layer BIF authors opinions on |
| **Payload** | USD mechanism for deferred geometry loading |
| **Prototype** | Shared geometry + material definition (loaded once) |
| **Instance** | Transform + prototype reference (lightweight) |
| **Opinion** | A USD value authored on a specific layer |
| **Composition Arc** | USD mechanism for combining layers (sublayer, reference, payload, variant, inherit, specialize) |
| **LIVRPS** | USD's composition strength order: Local, Inherits, Variants, References, Payloads, Specializes |
| **Edit Operation** | A single user action that produces USD output |
| **Layer Isolation** | Mode where only the edit layer is writable |
| **Render Context** | Configuration for resolving the full scene at render time |
