# BIF USD Workflow Foundation

**Version:** 0.3.0
**Last Updated:** 2026-05-01
**Status:** Living design note. v0.16.0 (C4a + C4b) is shipped. Sections marked _Future_ describe design intent, not shipped behavior.

## Implementation Notes

**Hybrid approach (decided 2026-03-28):** BIF is a **USD Orchestration Tool** — it keeps its procedural node graph (scatter, instancer, etc.) as a differentiator while adding layer awareness underneath. Edits author USD opinions continuously instead of only at export time. The existing data-flow node graph uses blue/orange color-coding (composition vs operation nodes) as a visual UX distinction, not an architectural rewrite. Orchestration scope: arrange, compose, override, instance. Not: model, rig, animate, simulate.

**Shot templates:** Future workflow, not shipped. Keep template creation out of the v0.16 edit/save foundation.

**v0.16 C4a + C4b shipped (v0.16.0, 2026-04-28):** `EditOperation` + `EditHistory`, stage-layer FFI writes, variant selections authored through `UsdEditContext` on the working layer, Ctrl+S saving the active working layer, ADR-008 for the edit architecture, plus the C4b editor surface (Apply-only USDA panel, Material Sheet, shading-model dropdown, transform gizmo). `EditOperation` now also covers `ReplaceLayerContents` (USDA round-trip) and `SetShaderId` (shading-model swap).

**Milestone threading:** This spec is implemented incrementally across v0.14-v0.19 — see [MILESTONES.md](MILESTONES.md) for the release schedule.

## Overview

BIF is a **USD Orchestration Tool** — not a USD viewer, not a version control system, not a modeling tool. BIF fills a gap in the VFX pipeline: no tool today lets an artist open a master USD stage, see the layer stack, pick their working layer, selectively load only what they need, make edits that author clean USD opinions, and save back to disk — all with a visual node graph and an Apply-only USDA preview.

### Design Philosophy

- **Be the best USD layer editor** — leave version control to ShotGrid, ftrack, git, etc.
- **Every edit authors USD** — no proprietary format, everything round-trips through `.usd` files
- **Load only what you need** — selective payload loading driven by the artist's task
- **Non-destructive by default** — edits go on the artist's layer, source data is never modified
- **Three views of one truth** — node graph, stage tree, and USDA code all show the same data

### Target Pipeline Position

```text
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

```text
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

Current implementation has two payload policies: `LoadAll` and `LoadNone`. Richer task-driven loading remains deferred to viewport-performance work.

**Task-driven loading**: BIF can infer what to load based on which layer the artist is editing. A lighting artist needs all geo visible to camera + light rigs. A layout artist needs everything but can tolerate lower LODs. An FX artist needs specific areas.

---

## Future Shot Templating

Shot-template creation is not part of the v0.16 edit/save foundation. It remains a future workflow for creating a root stage plus department layers from studio presets. Until that lands, docs should not imply template files, environment overrides, or built-in presets are shipped.

---
## Edit Operations

Every authored USD change in v0.16 is represented as an `EditOperation` and recorded in `EditHistory` on `SceneLayerState`.

Current operation surface (v0.16):

- Transform
- Visibility
- MaterialAssign
- MaterialParamOverride
- VariantSelect
- ReplaceLayerContents (USDA Apply round-trip)
- SetShaderId (shading-model swap)

Writes go through the USD C++ bridge under `UsdEditContext(stage, working_layer)`. The viewport keeps `instance_index` for interactive selection and translates to `(SdfPath, AttrSlot)` only at the edit boundary. This is the dual-track identity decision captured in ADR-008.

`EditHistory` runs parallel to the existing procedural `EditState` / `UndoStack`. Procedural node edits stay in the existing stack. USD layer opinions use `EditHistory`. _Future._ Cross-stack undo unification is deferred (ADR-008); today Ctrl+Z routes per-stack.

v0.16 deliberately stops short of node-to-opinion authoring for scatter/instancers, point edits, auto-save, and schema-registry validation. Those remain later work.

---
## The Three-Panel UI

BIF's editor shows three synchronized views of the same scene data. Each view is optimized for a different way of thinking about the scene.

### Layout

```text
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

_Future direction._ The blue/orange categorization, the Composition node types listed below (USD Reference, Payload Gate, SubLayer, Layer Stack), and node-driven opinion authoring are not yet implemented in v0.16. Today only interactive viewport edits route through `EditOperation`/`EditHistory`; procedural nodes still emit USD only at `export_scene()` time.

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

```text
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

### USD Code Preview: Active Edit Layer USDA

The code panel shows the USDA text of the **active edit layer**. Editing is Apply-only — type, hit Apply, and parse errors come back inline. The panel re-reads on focus and on apply. _Future._ Live keystroke-level preview is deferred.

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

_Future architecture (v0.17+ target). The `RenderContext` and `PrototypeState` types below are design sketches, not shipped APIs._

BIF has two rendering modes that share the same scene but load data differently.

### Working Mode (Viewport)

The viewport shows a selective view of the scene for interactive work:

```text
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

```text
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

_Future. No node or interactive tool exists in v0.16. Sketch retained for forward design._

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

_Future. No keyframe authoring tool exists in v0.16. Sketch retained for forward design._

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

_Future. Layer diff UI is not implemented in v0.16._

BIF can show the artist exactly what their edit layer changes compared to the composed base scene. This is a visual diff — not a text diff of USDA files, but a semantic diff of USD opinions.

### Diff Display

```text
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

Ctrl+S saves the active working layer only.

Current save path (v0.16):

1. Resolve `SceneLayerState.working_layer` to the layer identifier.
2. Check the layer through USD `PermissionToEdit()`.
3. Call `UsdStage::save_layer(&id)`, which resolves the `SdfLayer` C++-side and calls `SdfLayer::Save()`.
4. Clear the matching `LayerInfo.is_dirty` bit in shell and viewport state.
5. Report `Saved <id>` or `Save failed: ...`.

The root/master stage is not modified by Ctrl+S unless the root itself is the selected working layer. Multi-layer save, Save As, auto-save recovery, file-watcher conflict prompts, and sublayer dirty bubbling are deferred.

---
## Implementation Phases

### Phase 1: Layer-Aware Stage Loading (v0.14, shipped)

- Open USD stage through C++ FFI
- Read root + recursive sublayer stack
- Track mute state, edit-target candidate, payload policy, and strongest-opinion layer map
- Support `PayloadPolicy::LoadAll` and `PayloadPolicy::LoadNone`

### Phase 2: Qt Layer-Aware Shell (v0.15, shipped)

- Qt shell with layer stack, scene browser, property inspector, timeline, and render panels
- Real stage load/close, selection sync, lazy scene tree, and edit-target status surfaces

### Phase 3: Edit Foundation (v0.16 C4a, shipped)

- `EditOperation` / `EditHistory`
- Working-layer FFI writes and save
- Variant selections authored to the working layer
- Dirty bit and per-stack undo
- ADR-008

### Phase 4: Editor Features (v0.16 C4b, shipped)

- Apply-only USDA layer panel (live preview deferred)
- Material parameter sheet
- Shading-model dropdown
- Transform gizmo

### Deferred

- Rich payload policies and render-context loading
- Shot-template creation
- Point edits and layer diff UI
- Auto-save / file-watcher conflict prompts
- Schema-registry validation

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

## Variant Set Handling

Variant selection is a USD opinion. In v0.16, `VariantSelect` is an `EditOperation`, and the bridge writes it under `UsdEditContext(stage, working_layer)` instead of authoring into the session/root default target.

Current behavior:

- Read variant set names, variant names, and current selection from the composed stage.
- Record selection changes in `EditHistory`.
- Author the selection opinion on the active working layer.
- Reload after the selection changes so composed geometry reflects the variant.

Creating new variant sets and editing variant contents remains future work.

---
## Future Schema Validation

v0.16 relies on hard errors from the USD bridge and `PermissionToEdit()` checks. A richer validation layer that knows schema-specific constraints, configurable warning levels, and inline USDA diagnostics is deferred.

Near-term rule: writes should fail clearly when the USD API rejects the operation; they should not run a parallel hand-written schema system.

---
## Eager Node Evaluation + Stale Opinion Cleanup

_Future. Today `EditHistory.current_state` overwrites by `OpinionKey` and procedural node re-eval routes through `export_scene()`. Per-node `authored_paths` tracking and the `clear_authored_opinions` cleanup pass below are not implemented._

### Evaluation Model

Operation nodes evaluate **eagerly** — every parameter change triggers immediate re-evaluation. This keeps the USDA preview panel and viewport in sync with the artist's changes.

### Stale Opinion Cleanup

When a scatter node re-evaluates (e.g. artist changes seed), previous opinions at `/world/scatter_01` are stale. The cleanup model:

1. Each operation node tracks its **authored prim paths** (set of paths it has written to the current-state map)
2. On re-evaluation, node calls `clear_authored_opinions(previous_paths)` before writing new opinions
3. Stale opinions are removed and fresh ones written atomically
4. The undo entry captures both removal of old opinions and addition of new ones

```rust
impl OperationNode {
    /// Paths this node has authored to the current-state map.
    authored_paths: HashSet<OpinionKey>,

    pub fn re_evaluate(&mut self, history: &mut EditHistory, params: &NodeParams) {
        // 1. Clear previous opinions from current-state map
        for key in &self.authored_paths {
            history.clear_opinion(key);
        }

        // 2. Evaluate with new params
        let new_ops = self.evaluate(params);

        // 3. Apply new opinions and track paths
        self.authored_paths.clear();
        for op in new_ops {
            let keys = history.apply(op);
            self.authored_paths.extend(keys);
        }
    }
}
```

### Conflict Resolution

If two nodes write to the same prim path, last-write-wins within the current-state map. The UI warns when this happens (yellow indicator on conflicting nodes). Rare in practice — node paths are usually unique.

### `export_scene()` Coexistence

- **v0.14-v0.15:** `export_scene()` remains the batch export path. Continuous layer authoring runs in parallel for the preview panel.
- **v0.16+:** `export_scene()` becomes a "flatten + export" convenience — reads the composed stage and writes a single flattened layer. Both paths coexist, they serve different purposes.

---

## Session Layer

_Future. `LayerInfo::is_anonymous` exists in the layer model, but no anonymous session-layer edit target is wired in v0.16. Solo / viewport-hide flows below are unimplemented._

Viewport-only state (solo, hide, display overrides) must not pollute the edit layer. BIF uses a USD anonymous session layer for temporary state that is never saved to disk.

```rust
/// Anonymous in-memory layer for viewport-only state.
/// Never written to disk. Discarded on session close.
pub struct SessionLayer {
    /// USD anonymous layer (strongest — overrides everything for viewport display)
    layer: UsdAnonymousLayer,
}

impl SessionLayer {
    /// Solo a prim: hide everything else in the viewport
    pub fn solo(&mut self, prim_path: &str) { /* set visibility opinions */ }

    /// Temporarily hide a prim in viewport only
    pub fn viewport_hide(&mut self, prim_path: &str) { /* visibility = invisible */ }

    /// Display color override (e.g. wireframe color for selection)
    pub fn set_display_color(&mut self, prim_path: &str, color: Vec3) { /* ... */ }

    /// Clear all viewport overrides
    pub fn reset(&mut self) { self.layer.clear(); }
}
```

The session layer sits above all other layers in composition strength. It affects viewport display but is invisible to save, export, and render operations.

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
| **Current-State Map** | `HashMap<OpinionKey, AuthoredOpinion>` — the edit layer's current opinions, keyed by (prim, property) |
| **Session Layer** | Anonymous in-memory USD layer for viewport-only state (solo, hide) — never saved |
| **Variant Set** | USD mechanism for switchable alternatives (LODs, render/proxy) on a prim |
| **Schema Validation** | Pre-write check that opinions match USD schema types and constraints |
| **Eager Evaluation** | _Future._ Operation nodes re-evaluate immediately on every parameter change |
| **Apply-only USDA panel** | The C4b USDA editor: read-on-focus, write-on-Apply. Keystroke-level live preview is deferred |
