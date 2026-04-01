# BIF UI Mockup Review: UX Research Analysis

> **Status:** Findings incorporated into [UI_DESIGN.md](UI_DESIGN.md) on 2026-03-31. This file is the audit trail — the spec lives in UI_DESIGN.md.

**Date:** 2026-03-31
**Reviewer:** UX Researcher (AI-assisted)
**Artifacts Reviewed:**
- Assembly Workspace Detail (`assembly_workspace_detail/screen.png`)
- Assembly Workspace Qt Implementation (`assembly_workspace_qt_implementation/screen.png`)
- Lighting Workspace Qt Implementation (`lighting_workspace_qt_implementation/screen.png`)
- Materials Workspace (`materials_workspace/screen.png`)
- Design System: "Quiet Confidence" (`obsidian_graphite/DESIGN.md`)
- DCC UI Research (`docs/ux/DCC_UI_RESEARCH.md`)
- UI Design Brainstorm (`docs/ux/UI_DESIGN.md`)

**Target Users:**
- (A) Senior VFX TD familiar with USD composition, layers, and pipeline tooling
- (B) Junior artist new to scene assembly, may not understand USD opinions/layers
- (C) Lighting artist who wants to place/tweak lights without touching the node graph

---

## Executive Summary

The mockups demonstrate a strong foundational vision. The T-layout, workspace tabs, layer stack panel, and "Quiet Confidence" aesthetic are all well-aligned with DCC industry conventions and the project's own research findings. The design system's "no-line" tonal architecture is executed convincingly across all four workspace views, and the use of teal as the active-layer accent is distinctive without being distracting.

However, several usability risks emerge under close inspection, particularly around active layer safety (the single most consequential UX decision in a layer-aware USD editor), discoverability for non-expert users, and consistency gaps between the four workspace mockups. This review identifies 7 P0 (critical) issues, 9 P1 (important) issues, and 8 P2 (nice-to-have) improvements.

---

## 1. Task Flow Analysis

### Core Workflow: Open Stage, Browse, Select, Inspect, Edit, Export

Walking through the primary VFX assembly workflow against the Assembly Workspace Detail mockup:

**Step 1: Open USD Stage**
- Not directly visible in the mockups. The breadcrumb bar at the top shows `shot_010.usd > layout.usd > /world/hero_char`, implying the stage is already open.
- **Gap:** No visible "Open Stage" affordance. Where does the user go to open a new file? The top-left "BIF" logo area and the workspace tabs consume the full header. There is no File menu visible.
- **P1-01: Add a File/Stage menu or make the stage name in the breadcrumb bar a clickable entry point for open/close/recent operations.** Users from Houdini/Katana/Clarisse all expect a File menu or a prominent "Open" action.

**Step 2: Browse Scene Tree**
- The Scene Tree panel (left side) shows a standard hierarchy: `/world > hero_char > skeleton, mesh, environment`. Expand/collapse arrows are visible. Type icons (triangle for mesh, bone for skeleton) are present.
- The tree is clean and readable. The layer-colored dot next to `hero_char` (blue/teal) is visible but small.
- **Positive:** Scene tree placement matches the universal DCC convention (left side, vertical). Western reading order is respected. Persona (A) will feel at home immediately.
- **Gap for Persona (B):** No search/filter bar at the top of the scene tree. In a production scene with 10,000+ prims, scrolling is not viable. Houdini, Katana, and Blender all provide tree filtering.
- **P1-02: Add a search/filter input at the top of the Scene Tree panel.** This is documented in UI_DESIGN.md's recommendations but not visible in any mockup.

**Step 3: Select Prim**
- Clicking `hero_char` highlights it in teal. The viewport shows the corresponding object (the bust). The Inspector panel on the right populates with Transform, Visibility, and Prim Metadata.
- **Positive:** The selection-to-inspection flow is immediate and visible. This matches the universal DCC "click to inspect" pattern.
- **Gap:** The viewport selection highlight is not visible in the mockup. When a user clicks a prim in the scene tree, is the corresponding geometry highlighted in the viewport? This bidirectional selection sync is critical (Clarisse, Katana, and Blender all do it).
- **P2-01: Ensure viewport selection highlighting is visually distinct (wireframe outline, bounding box, or silhouette edge) and documented in the design system.**

**Step 4: Inspect Properties**
- The Inspector panel shows Transform (Translate, Rotate, Scale), Visibility (Purpose, Show toggle), and Prim Metadata (Type, Path, Composition).
- **Positive:** Clean single-column layout. Collapsible sections. Monospace values for numerical fields. This matches Resolve's inspector pattern.
- **Gap:** No layer attribution coloring is visible in this mockup. The UI_DESIGN.md specifies colored left-borders on property rows showing which layer set each value, and bold/gray/amber text to distinguish "your changes" from "inherited" from "overridden." None of this appears in the Assembly Workspace Detail view.
- **P0-01: Layer attribution must be visible in the property inspector from day one.** This is BIF's primary differentiator. Without it, the inspector is just another property panel. Show colored dots, bold/gray text, or left-border stripes per UI_DESIGN.md's specification. Even a simplified version (just the layer dot) is better than nothing.

**Step 5: Edit on Active Layer**
- The Layer Stack panel (lower-left) shows three layers: `lighting.usd` (checked, active), `layout.usd`, and `shot_010.usd`. The active layer has a teal checkbox.
- **Critical concern — see Section 5 (Error Prevention) for full analysis.**

**Step 6: Export**
- The Node Graph (bottom panel) shows an `UsdExport` node connected to the graph. Node tabs are visible: Assembly, Materials, Output.
- **Gap:** The export operation itself is not visible. Is "Export" triggered by the orange `UsdExport` node? Is there an Export button? For Persona (C), the path from "I'm done editing" to "my changes are saved" needs to be explicit.
- **P1-03: Add an explicit Export/Save action in the toolbar or viewport header.** The node graph is powerful but should not be the only way to trigger export. A "Save Layer" button in the Layer Stack panel or a toolbar shortcut (`Ctrl+S`) should be the primary path.

---

## 2. Mental Model Alignment

### Where BIF Matches DCC Conventions (Good)

| Convention | BIF Implementation | Industry Standard |
|---|---|---|
| T-layout (tree/viewport/inspector) | All four mockups | Clarisse, Katana, Blender, Nuke |
| Workspace tabs at top | ASSEMBLY / LIGHTING / MATERIALS / REVIEW | Blender, Resolve |
| Node graph at bottom | Tabbed bottom panel | Houdini, Nuke |
| Scene tree on left | Left panel in all views | Universal |
| Inspector on right | Right panel in all views | Universal |
| Dark theme, minimal chrome | "Quiet Confidence" design system | Resolve, Blender 4.x |

### Where BIF Deviates From Conventions

**Deviation 1: Icon sidebar on far left**
- All four mockups show a vertical icon sidebar on the far left (outside the scene tree). Icons include what appear to be: cursor/select, move, rotate, scale, and several others.
- **Problem:** This pattern is borrowed from Blender's T-panel / Figma's tool sidebar, but in DCC scene assembly tools (Clarisse, Katana, Houdini), there is typically no persistent tool sidebar. Manipulation tools (translate, rotate, scale) are accessed via keyboard shortcuts (W/E/R) or viewport gizmos.
- **Risk:** The sidebar consumes 40-50px of horizontal space permanently. On a 1920px display, that is 2.6% of screen width taken from the viewport.
- **Assessment:** This deviation is partially justified for Persona (B) (new users who do not know shortcuts), but it should be collapsible.
- **P1-04: Make the icon sidebar collapsible (hidden by default for expert users). Add tooltips with keyboard shortcut hints on every icon.** If icons are the only affordance (no labels), they must have excellent tooltips.

**Deviation 2: Layer Stack as a persistent left-panel section (not a tab)**
- In the Assembly Detail mockup, the Layer Stack is a separate section below the Scene Tree in the left panel, always visible.
- In Photoshop, layers are their own panel (often bottom-right). In Katana, the scene graph and node graph are separate panels. No DCC tool docks the layer stack directly beneath the scene tree in the same panel.
- **Assessment:** This deviation is well-justified. USD layers are tightly coupled to the scene tree (which prim is on which layer), so colocating them reduces eye travel. Photoshop's "layers + canvas" pairing is the closest precedent. Keep this.
- **Positive — no change needed.**

**Deviation 3: Breadcrumb bar shows file + layer + prim path**
- The breadcrumb shows: `shot_010.usd > layout.usd > /world/hero_char`
- This combines three different navigational concepts (stage file, active layer, selected prim) in one bar. Katana shows only the scene graph path. Houdini shows network path. No tool combines all three.
- **Assessment:** Justified and potentially excellent. For Persona (A), this is a power feature — instant visibility of "where am I, what layer am I editing, what's selected." For Persona (B), it may be confusing.
- **P2-02: Add tooltip explanations on each breadcrumb segment.** Hovering over `layout.usd` should say "Active authoring layer — edits go here." Hovering over the prim path should say "Currently selected prim."

**Deviation 4: Materials workspace shows project-level tree, not stage-level**
- The Materials Workspace mockup shows a left panel labeled "Project Alpha" with a tree: Materials > hero_wet, base_concrete, glass_clear; Geometry; Lights. This is a project organizer, not a USD stage tree.
- **Problem:** This is inconsistent with the Assembly workspace, which shows a USD scene tree (`/world/...`). Switching between workspaces changes the left panel's fundamental data model, which violates the principle of consistency.
- **P0-02: The left panel should always show the USD scene tree, regardless of workspace.** Filter or highlight material-relevant prims in the Materials workspace, but do not replace the scene tree with a project browser. Users must maintain spatial orientation when switching workspaces. If a project browser is needed, it should be a separate tab within the left panel, not a replacement.

---

## 3. Cognitive Load Assessment

### Concepts a User Must Track Simultaneously

In the Assembly Workspace Detail view, the user must understand:

1. **Scene hierarchy** (scene tree) — What prims exist and their parent/child relationships
2. **Active layer** (layer stack) — Where edits go
3. **Layer strength order** (layer stack) — Which layer wins in composition
4. **Property values** (inspector) — Current composed values
5. **Property provenance** (not yet visible) — Which layer set each value
6. **Node graph topology** (bottom panel) — How nodes connect to produce the stage
7. **Viewport state** (center) — 3D visualization of the composed result
8. **Breadcrumb context** (top bar) — Current file, layer, and selection

**That is 8 concurrent concepts.** For Persona (A), this is manageable. For Persona (B), this is overwhelming. For Persona (C), concepts 3, 5, and 6 are irrelevant to their task.

### Cognitive Load Reduction Recommendations

**P0-03: Implement workspace-specific progressive disclosure as designed in UI_DESIGN.md.**

The UI_DESIGN.md specifies three tiers of progressive disclosure, and the workspace presets are supposed to show only task-relevant panels. However, the mockups show nearly identical information density across all four workspaces. Specific changes:

| Workspace | Should Hide | Should Emphasize |
|---|---|---|
| Assembly | Render controls, light properties | Node graph, scene tree, full layer stack |
| Lighting | Node graph (collapse by default), material properties | Viewport (dominant), light inspector, render snapshots |
| Materials | Layer stack detail, instancing properties | Material parameter sheet, shader node graph, preview orb |
| Review | Node graph, scene tree, layer stack | Viewport (maximized), render controls, A/B comparison |

The Lighting mockup partially achieves this (viewport is dominant, property inspector shows light-specific attributes, render snapshots are visible at bottom). But the Assembly and Materials workspaces show too much simultaneously.

**P1-05: Default the node graph to collapsed in the Lighting workspace.** The lighting mockup shows bottom tabs for NODE GRAPH, USDA PREVIEW, and CONSOLE, but the node graph tab should not be active by default in this workspace. Persona (C) does not need it.

**P1-06: In the Materials workspace, the shader node graph (bottom panel) should be the dominant bottom element, with USDA Preview and Render Log as secondary tabs.** Currently the mockup shows this correctly, but the shader graph area is small relative to the viewport. Consider a 50/50 vertical split in this workspace.

---

## 4. Discoverability Analysis

### Icon Sidebar

The far-left icon sidebar contains approximately 8-10 icons. From the mockups, these appear to represent:
- Selection/cursor tool
- Move tool
- Rotate tool
- Scale tool
- Viewport navigation tools
- Some form of creation or measurement tools

**Problem: Icons without labels require memorization.** The DCC_UI_RESEARCH.md explicitly warns against this (Section 4, Anti-Patterns, "Tiny icons without labels" from Clarisse). The design system DESIGN.md does not specify what these icons represent or their tooltip content.

**P0-04: Every sidebar icon must have (a) a tooltip showing the tool name and keyboard shortcut, (b) a label visible in an expanded sidebar state, and (c) documentation in the design system.** Without this, Persona (B) will not know what half the icons do. Consider following Blender 4.x's approach: icons with labels by default, icon-only as a compact option.

### Workspace Tabs

The four workspace tabs (ASSEMBLY, LIGHTING, MATERIALS, REVIEW) are clearly labeled and prominently positioned at the top left. This is excellent.

**Positive:** Text labels, not icons. Correct placement (top of window, left-aligned). Current workspace is visually distinguished (underline or highlight).

**Minor gap:** The REVIEW workspace is not represented in the mockups. Users may not understand what "Review" means without trying it.
- **P2-03: Consider renaming REVIEW to RENDER or RENDER REVIEW to better communicate its purpose.** "Review" is ambiguous — it could mean "code review," "peer review," or "shot review." "Render" or "Render Review" is unambiguous.

### Bottom Panel Tabs

All mockups show tabbed bottom panels (NODE GRAPH, USDA PREVIEW, RENDER LOG, CONSOLE). These are text-labeled tabs, which is good.

**Gap in the Assembly Detail mockup:** The bottom panel tabs are small and low-contrast. In the mockup they read "NODE GRAPH | USDA PREVIEW | RENDER LOG" but the text is quite dim against the dark background.
- **P1-07: Increase contrast on bottom panel tab labels.** Use `--text-primary` (#d4d4d4) for the active tab and `--text-secondary` (#888888) for inactive tabs. The current rendering appears to use something dimmer than secondary.

### Search and Command Palette

Neither a search bar nor a command palette trigger is visible in any mockup. The UI_DESIGN.md specifies `Ctrl+P` for a command palette and the DCC_UI_RESEARCH.md identifies this as a critical feature borrowed from Houdini's TAB menu and VS Code.

**P1-08: Add a visible search trigger in the toolbar area.** Even if the command palette is keyboard-triggered (`Ctrl+P`), a search icon or "Search Assets..." field in the top bar (visible in the mockup header area) teaches users the feature exists. The Assembly Qt mockup shows "SEARCH ASSETS..." in the top bar, which is good, but the Assembly Detail mockup does not.

### The "Search Assets" Field

The Assembly Qt Implementation mockup shows a "HERO_ASSET_PRIM" text next to what appears to be "SEARCH ASSETS..." in the header. This is good but raises questions:
- Is this a search field or a display-only breadcrumb?
- Does it search the scene tree, the node graph, or both?
- **P2-04: Clarify the search scope visually.** Add a magnifying glass icon and placeholder text like "Search scene... (Ctrl+P)" to make it unmistakably interactive and to hint at the keyboard shortcut.

---

## 5. Error Prevention: Active Layer Safety

**This is the highest-priority section of this review.** In production VFX, editing the wrong layer can corrupt hours of work. If a lighting artist accidentally edits the layout layer, their changes override the layout department's work and may not be caught until downstream renders fail. The active layer indicator is not a cosmetic choice — it is a safety mechanism.

### Current State in Mockups

**Assembly Detail mockup:**
- The Layer Stack shows `lighting.usd` with a teal checkbox. The other layers (`layout.usd`, `shot_010.usd`) have no checkbox or a dimmed/unchecked state.
- The breadcrumb bar shows `layout.usd` as part of the path.
- The Inspector shows a small "Layout Layer" label in the upper-right corner.

**Assembly Qt Implementation mockup:**
- The Layer Stack shows `shot_assembly.usd` highlighted in teal/cyan, with `layout_base.001.usd` and `anim_reference.usd` below it.
- No other active layer indicator is visible.

**Lighting workspace mockup:**
- No layer stack panel is visible. No active layer indicator is visible anywhere.

**Materials workspace mockup:**
- No layer stack panel is visible. No active layer indicator is visible.

### Problems Identified

**P0-05: The active layer indicator is insufficient and inconsistent across workspaces.**

Specific issues:

1. **Not visible in Lighting and Materials workspaces.** When a lighting artist adjusts a light's intensity, which layer receives that edit? There is zero visual indication. This is a data-loss risk.

2. **The teal checkbox in the Layer Stack is the only indicator in Assembly views.** If the Layer Stack panel is collapsed or scrolled out of view, the user has no way to know which layer is active. No redundant indicator exists.

3. **No confirmation on layer switch.** If the user clicks a different layer to make it active, the edit target changes silently. Edits to 50 properties could go to the wrong layer before the user notices.

4. **No "wrong layer" warning.** If the user selects a prim that has no opinions on the current edit layer, there is no visual signal that "you are about to create new opinions on this layer for this prim." In pipeline workflows, this is a common source of "layer pollution."

### Recommendations for Active Layer Safety

**P0-05a: Add a persistent active-layer indicator to the viewport status bar in ALL workspaces.**
The UI_DESIGN.md specifies: `lighting.usd (edit) | 12,847 tris | 1.2M instances | Ivar: 64 spp | --------`
This status bar must be visible in every workspace, always. The layer name should use the layer's assigned color (teal, purple, gold, etc.) to be instantly recognizable. Even in maximized viewport mode (Review workspace), the status bar should remain.

**P0-05b: Add a layer-switch confirmation for layers that already contain opinions.**
When the user changes the active edit layer to a layer that already has authored opinions, show a brief non-blocking toast: "Now editing: lighting.usd (42 existing opinions)." This gives the user a moment to realize they switched context.

**P0-05c: Add a "guard" visual when about to create new opinions on a prim.**
When the user begins editing a property on a prim that has no existing opinions on the active layer, highlight the property row with a subtle amber border or icon, indicating: "This will create a new opinion on [layer name]." This is analogous to Git showing "new file" vs "modified file" — the distinction matters in production.

**P0-05d: Consider a "Layer Lock" confirmation for non-active layers.**
In the Layer Stack, layers that are NOT the edit target should show a lock icon. Attempting to drag-reorder layers (which changes composition strength) should require a confirmation, as reordering layers can change the composed result of every prim in the scene.

**P0-06: The Lighting workspace must show the active layer somewhere.**
Options (pick at least one):
- A compact layer indicator in the property inspector header ("Editing on: lighting.usd")
- The viewport status bar (recommended above)
- A subtle colored stripe along the top or bottom edge of the viewport matching the active layer color

---

## 6. Competitor Comparison

### vs. Houdini Network View

| Aspect | BIF | Houdini | Assessment |
|---|---|---|---|
| Node creation | Not shown (likely right-click menu) | TAB fuzzy search | **BIF needs a fuzzy-search node creation menu (P1-08 covers this via command palette)** |
| Node coloring | Blue (composition) / Orange (operation) | Color-coded by type + custom | **BIF's two-color system is simpler. Good for now, but add material (purple) and render (green) categories as node types expand** |
| Graph readability | Clean, minimal, 3-4 nodes visible | Dense, dozens of nodes, sticky notes | **BIF is cleaner at small scale but untested at production scale (50+ nodes). Plan for zoom LOD.** |
| Parameter panel | Single-column inspector | Multi-tab, 50+ params visible | **BIF is better. Progressive disclosure over parameter sprawl.** |
| Dive-in navigation | Not shown | Network boxes, dive-in | **Future consideration for BIF. Not needed at current node count.** |

**BIF does better:** Property inspector simplicity, visual cleanliness, workspace-focused UI.
**BIF does worse:** No visible node creation menu, no graph organization tools (sticky notes, backdrops), untested at scale.

### vs. Clarisse Browser

| Aspect | BIF | Clarisse | Assessment |
|---|---|---|---|
| Scene hierarchy | USD scene tree (prim-based) | Context-based browser | **Similar model. BIF's is more standard (matches USD concepts directly).** |
| Layer visibility | Layer stack panel with eye icons | Context visibility toggles | **BIF's is more explicit and Photoshop-familiar.** |
| Instancing display | Node graph exposes PointInstancer | Scatterer objects in browser | **BIF could show instance counts in scene tree (P2-05)** |
| Bulk editing | Not shown | Spreadsheet editor | **BIF lacks this. Critical for Persona (C) managing 50+ lights.** |
| Render view | Viewport with render preview | Dedicated Image panel, tileable | **Clarisse's multi-region render tiling is superior. BIF's Lighting workspace render snapshots are a good start.** |

**BIF does better:** Layer system is more explicit and visual, modern aesthetic, node graph provides procedural power Clarisse lacks.
**BIF does worse:** No spreadsheet editor (planned for post-v0.15), no render tiling.

### vs. Katana Scene Graph

| Aspect | BIF | Katana | Assessment |
|---|---|---|---|
| Opinion attribution | Planned (layer dots) but not visible in mockups | Color-coded attributes (white/gray/yellow) | **Katana is currently superior here. BIF must implement P0-01 to compete.** |
| Live vs static indicators | Not shown | Green=live, gray=static locations | **BIF should distinguish procedurally-generated prims from USD-loaded prims (P2-06)** |
| USDA code view | Tabbed bottom panel | Not available (external only) | **BIF is genuinely better. This is a differentiator.** |
| Material assignment | Not shown | Drag from material list to scene graph | **BIF should support drag-and-drop material assignment (P2-07)** |
| Render catalog | Lighting workspace shows snapshots | Full catalog with A/B comparison | **BIF's render snapshot thumbnails are promising. Needs expand-to-compare functionality.** |

**BIF does better:** USDA code preview (no current tool has this), cleaner visual design, workspace presets.
**BIF does worse:** Opinion attribution not yet visible, no live/static prim distinction, no material drag-and-drop.

### vs. Nuke Node Graph

| Aspect | BIF | Nuke | Assessment |
|---|---|---|---|
| Graph layout | Left-to-right flow | Top-to-bottom flow | **Both are valid. BIF's L-to-R matches Western reading direction.** |
| Node "solo" view | Not shown | View any node's output | **BIF should let users "solo" a node's contribution to the viewport (P2-08). Huge for debugging.** |
| Backdrop nodes | Not shown | Large colored rectangles for grouping | **Add backdrop/group nodes for graph organization at scale.** |
| Property panel | Inspector panel (right side) | Knob panel (properties bin) | **Similar approach. BIF's collapsible sections are cleaner.** |

---

## 7. Persona-Specific Assessment

### Persona A: Senior VFX TD (USD Expert)

**Will appreciate:**
- Breadcrumb bar showing stage + layer + prim path (instant context awareness)
- Layer stack panel with eye/lock icons (Photoshop-familiar pattern)
- USDA code preview tab (this person will live here)
- Node graph for procedural workflows
- Workspace tabs for task switching

**Will be frustrated by:**
- Lack of opinion attribution in the inspector (P0-01). This user expects to see which layer owns each property value.
- No command palette visible (P1-08). Power users want keyboard-driven workflows.
- No visible keyboard shortcut hints anywhere. This user will want to learn shortcuts fast.
- No "diff" view for active layer changes. The DCC research identifies this as a novel differentiator, but it is not in the mockups.

**Verdict:** 7/10. Solid foundation, but missing the expert-level layer attribution and keyboard workflow features that would make this user choose BIF over Katana.

### Persona B: Junior Artist (New to Scene Assembly)

**Will appreciate:**
- Clean, unintimidating layout (the "Quiet Confidence" aesthetic works here)
- Workspace tabs reduce confusion about "what should I be doing"
- Viewport is dominant — they can see their work

**Will be frustrated by:**
- What do the left sidebar icons mean? (P0-04)
- What is a "layer"? Why are there three? Which one am I editing? (P0-05 family)
- The scene tree has no search (P1-02)
- The Inspector shows "Prim Metadata: Type = UsdGeomMesh" — what does that mean? No human-readable labels.
- No onboarding experience. The DCC research recommends a first-launch walkthrough (Section 5.5), but nothing in the mockups suggests one.

**Action items for Persona B:**
- **P0-07: Add human-readable labels alongside USD technical names in the Inspector.** Show "Mesh" not "UsdGeomMesh." Show "Position" not "xformOp:translate." Show the technical name in a tooltip or secondary text. This is called out in the DCC research's Katana anti-patterns.
- **P1-09: Plan a first-launch onboarding overlay (even a simple labeled screenshot tour).** This is post-Qt but should be designed now.

### Persona C: Lighting Artist

**Will appreciate:**
- Lighting workspace mockup is strong. Viewport dominates. Property inspector shows light-specific attributes (Intensity, Exposure, Color Temperature). Render snapshots visible.
- The "A/B ACTIVE" button on render snapshots is excellent — lets them compare iterations.
- "NEW SNAPSHOT" button is a clear call to action.
- Shadows toggle and samples control are visible and relevant.

**Will be frustrated by:**
- No active layer indicator (P0-05/P0-06). Which layer are my light edits going to?
- No light list. The left panel in the Lighting workspace shows the viewport tools, but no list of lights in the scene. Persona (C) needs to see all lights, select them by name, and adjust properties. Currently they must use the viewport selection or (invisible) scene tree.
- **P1-10: Add a "Light List" panel or scene tree filter in the Lighting workspace.** Show only UsdLux prims, with type icons (rect, sphere, distant, dome). Let the user select lights from this list to populate the inspector. Clarisse and Katana both provide filtered light lists.
- The "STAR ATLAS" and "LEGACY SOURCE" tabs in the inspector are unclear.
- No spreadsheet/multi-select editing. Adjusting 50 lights one at a time is painful.

**Verdict:** 6/10. The Lighting workspace has the right structure but is missing a light list panel and the active layer indicator. With those additions, it would be 8/10.

---

## 8. Design System Compliance Check

Checking the mockups against the "Quiet Confidence" design system in `DESIGN.md`:

| Rule | Compliance | Notes |
|---|---|---|
| No-line rule (no 1px borders for sections) | Mostly compliant | Assembly Detail has subtle shadow gaps between panels. Good. |
| Surface hierarchy (background shifts for depth) | Compliant | Panel backgrounds differ from viewport background. Elevated areas visible. |
| Glass/gradient rule (floating HUDs use glassmorphism) | Partially compliant | Lighting workspace render controls in viewport appear to use glass effect. Assembly Detail viewport HUD (camera controls) is less clear. |
| Typography: Manrope for headlines, Inter for labels | Cannot verify from screenshots | Mockups are too low-resolution to confirm exact fonts. |
| Monospace for data values | Appears compliant | Inspector values (12.45, 0.88, etc.) appear monospaced. |
| Uppercase section headers with letter-spacing | Partially compliant | "TRANSFORM", "VISIBILITY", "PRIM METADATA" appear uppercase. "INSPECTOR" header does too. |
| 3px accent stripe on layer list items | Partially visible | Layer stack shows colored indicators but stripe thickness is hard to confirm. |
| No divider lines in lists | Mostly compliant | Scene tree and layer stack appear to use spacing, not lines. |
| `secondary` (#5dd9d0) for success states | Not testable | No success state visible in mockups. |
| No pure white text | Appears compliant | Text appears off-white, not #ffffff. |
| `md` radius (0.375rem) on elements | Appears compliant | Node bodies and buttons show subtle rounding. |

**Overall design system adherence: Good.** The mockups feel cohesive and match the "Technical Atelier" vision. The main gap is the layer attribution coloring system (planned but not yet visible).

---

## 9. Workspace-Specific Findings

### Assembly Workspace Detail

**Strengths:**
- Best-realized mockup of the four. All planned panels are visible.
- Node graph shows meaningful topology (UsdRead > Bfore > Combine > UsdExport with a Primitive and Xform branch).
- Layer stack is colocated with scene tree (good information architecture).
- Inspector is focused on the selected prim's core properties.

**Issues:**
- Node graph node labels are hard to read at this zoom level. The colored node headers (orange for "Bfore", teal for "Combine") are correct per the design system, but the text is very small.
- The "Assembly | Materials | Output" sub-tabs on the node graph are not clearly differentiated from the main NODE GRAPH / USDA PREVIEW / RENDER LOG tabs. Two levels of tabs in the bottom panel creates ambiguity.
- **P1-11: Clarify the tab hierarchy in the bottom panel.** The outer tabs (NODE GRAPH, USDA PREVIEW, RENDER LOG) switch the panel content. The inner tabs (Assembly, Materials, Output) filter within the node graph. Use visual differentiation (size, style, position) to make the hierarchy obvious. Consider making inner tabs a different style (pill buttons vs. underline tabs).

### Assembly Qt Implementation

**Strengths:**
- Shows a more realistic "in-progress" state with a rendering viewport ("USD MESH - REAL_GEOM").
- Rendering stats visible at bottom of viewport ("TRI: 6142, Prims: 139, Samples: 256").
- Layer stack shows three layers with the active one highlighted.

**Issues:**
- The Inspector panel appears truncated. Only "Transform" and "Attributes" sections are visible, and the Attributes section shows "schema::UsdGeomGprim" which is very technical.
- The node graph at bottom shows colored nodes but they are quite small and hard to read.
- The "RESOLVE COLORS" and "EXPORT GRAPH" buttons at the bottom-right are unclear. What do they do? These need clearer labels or tooltips.
- **P2-05: Rename "RESOLVE COLORS" to something more descriptive**, such as "Apply Color Overrides" or similar. "Resolve" is an overloaded term (DaVinci Resolve, USD composition resolution).

### Lighting Workspace Qt Implementation

**Strengths:**
- Best workspace differentiation of all four mockups. The viewport is clearly dominant (approximately 70% of screen area).
- Property Inspector is focused on light-specific attributes: Active Light name, Type (Rect Light), Status, Intensity, Exposure, Color Temperature, Shadows, Samples. This is excellent task-focused design.
- Render snapshots at bottom with "RENDER A" / "RENDER B" / "A/B ACTIVE" is a great feature for iterative lighting.
- "NEW SNAPSHOT" button with camera icon is a clear call-to-action.
- Viewport label "PERSPECTIVE | PATH TRACED (1024 SAMPLES)" is informative without being cluttered.
- The left sidebar shows viewport manipulation tools (move, rotate, scale, light-specific tools) which makes sense for this workspace.

**Issues:**
- No scene tree or light list visible. How does the user select a different light? They must click in the viewport, which does not scale to scenes with dozens of lights, especially lights that are off-screen or overlapping.
- No active layer indicator anywhere (P0-06, critical).
- The "VISUAL ATLAS" and "LEGACY SOURCES" tabs in the inspector are unexplained.
- Bottom tabs (NODE GRAPH, USDA PREVIEW, CONSOLE) are present but the selected tab is unclear. In the Lighting workspace, CONSOLE should probably be the default (for render output), not NODE GRAPH.

### Materials Workspace

**Strengths:**
- Material Parameter Sheet (right panel) is well-structured: Shading Model dropdown (OpenPBR), organized sections (BASE, SPECULAR, COAT, EMISSION).
- The shader node graph (bottom) shows a UsdUVTexture > OpenPBR Surface > MaterialOut chain with visible connections. Yellow connection lines are clear.
- The material preview orb (viewport overlay, bottom-right) is a nice touch for instant material feedback.
- The "UPDATE SHADER" button (bottom-right, blue accent) is a clear primary action.

**Issues:**
- Left panel shows "Project Alpha" with a project-level tree, not the USD stage tree (P0-02, covered above). This breaks consistency with other workspaces.
- The shader node graph is small. In a Materials workspace, the shader graph should have more vertical space, possibly a 50/50 split with the viewport, since material editing is a graph-heavy workflow.
- The "UsdUVTexture" node shows "rgb" and "Colorlit" outputs but the text is very small and hard to read.
- **P1-12: Consider a split-view option in the Materials workspace.** Let users toggle between "viewport dominant" (current) and "graph dominant" (shader graph takes 60% of vertical space) modes. Substance Designer uses a similar toggle.

---

## 10. Consolidated Recommendations

### P0 Critical (Must Fix Before Implementation)

| ID | Issue | Mockup | Recommendation |
|---|---|---|---|
| P0-01 | No layer attribution coloring in property inspector | Assembly Detail | Add colored dots/stripes and bold/gray/amber text per UI_DESIGN.md spec |
| P0-02 | Materials workspace replaces scene tree with project browser | Materials | Keep USD scene tree in all workspaces; add material filter |
| P0-03 | All workspaces show similar information density | All | Implement workspace-specific progressive disclosure |
| P0-04 | Icon sidebar has no labels or visible tooltips | All | Add tooltips with shortcuts; make sidebar collapsible; add label mode |
| P0-05 | Active layer indicator is insufficient and missing from 2 workspaces | Lighting, Materials | Add persistent viewport status bar with layer name in ALL workspaces |
| P0-06 | Lighting workspace has no layer indicator at all | Lighting | Add layer indicator to inspector header or viewport status bar |
| P0-07 | Inspector shows raw USD type names, not human-readable labels | Assembly Detail | Show "Mesh" not "UsdGeomMesh"; technical name in tooltip |

### P1 Important (Should Fix Before v0.15 Ship)

| ID | Issue | Mockup | Recommendation |
|---|---|---|---|
| P1-01 | No visible File/Open affordance | All | Add File menu or make breadcrumb stage name clickable |
| P1-02 | Scene tree has no search/filter | Assembly Detail | Add search bar at top of scene tree panel |
| P1-03 | No explicit Save/Export button outside node graph | Assembly Detail | Add "Save Layer" to layer stack panel or toolbar |
| P1-04 | Icon sidebar is not collapsible | All | Make collapsible; default collapsed for expert users |
| P1-05 | Node graph active by default in Lighting workspace | Lighting | Default to CONSOLE tab in Lighting workspace |
| P1-06 | Materials shader graph area is too small | Materials | Offer split-view toggle (viewport vs. graph dominant) |
| P1-07 | Bottom panel tab labels are low contrast | Assembly Detail | Use --text-primary for active tab, --text-secondary for inactive |
| P1-08 | No command palette or search trigger visible | All | Add search icon/field in top bar; implement Ctrl+P palette |
| P1-09 | No onboarding experience for new users | N/A | Design first-launch overlay tour |
| P1-10 | No light list panel in Lighting workspace | Lighting | Add filtered light list or scene tree filter |
| P1-11 | Two levels of tabs in bottom panel create ambiguity | Assembly Detail | Visually differentiate outer tabs vs. inner sub-tabs |
| P1-12 | Materials shader graph needs more space | Materials | Add viewport/graph split toggle |

### P2 Nice-to-Have (Post-Ship Polish)

| ID | Issue | Mockup | Recommendation |
|---|---|---|---|
| P2-01 | Viewport selection highlight not visible | Assembly Detail | Add wireframe/silhouette selection highlight |
| P2-02 | Breadcrumb segments have no tooltip explanations | Assembly Detail | Add hover tooltips explaining each segment |
| P2-03 | "REVIEW" workspace name is ambiguous | All | Consider "RENDER" or "RENDER REVIEW" |
| P2-04 | Search bar scope is unclear | Assembly Qt | Add magnifying glass icon and scope hint |
| P2-05 | "RESOLVE COLORS" button label is confusing | Assembly Qt | Rename to clearer label |
| P2-06 | No live/static prim distinction in scene tree | All | Show procedurally-generated vs. USD-loaded prims differently |
| P2-07 | No material drag-and-drop assignment | Materials | Support drag from material list to scene tree |
| P2-08 | No node "solo" view for debugging | Assembly Detail | Let users view any node's contribution in isolation |

---

## 11. Summary Scorecard

| Dimension | Score (1-10) | Notes |
|---|---|---|
| Layout and spatial organization | 9 | T-layout is correct, panel sizing is good, workspace tabs are well-placed |
| Visual design and aesthetic | 8 | "Quiet Confidence" is achieved. Cohesive across workspaces. Slightly too uniform (workspaces need more differentiation) |
| Task flow efficiency | 6 | Core flow works but missing search, command palette, save affordance |
| Error prevention (layer safety) | 4 | Active layer indicator is the biggest gap. Critical for production use |
| Discoverability | 5 | Icon sidebar is opaque, expert features are hidden, no onboarding |
| Expert user efficiency | 6 | No command palette, no keyboard shortcut visibility, no opinion attribution |
| Junior user friendliness | 5 | Raw USD terminology, no onboarding, high cognitive load |
| Competitor parity | 7 | Matches or exceeds Clarisse/Katana aesthetically, but missing Katana's opinion attribution and Clarisse's spreadsheet editor |
| Innovation (USDA preview, layer diff) | 8 | Strong vision, but not fully realized in mockups yet |
| **Overall** | **6.4** | Strong visual foundation. The layer safety and discoverability gaps are fixable. Prioritize P0 items. |

---

## 12. Recommended Next Steps

1. **Immediate (before Qt implementation begins):** Address P0-01 (layer attribution), P0-05 (active layer indicator), and P0-07 (human-readable labels). These are architectural decisions that affect widget design.

2. **During Qt wireframing:** Address P0-02 (consistent scene tree), P0-03 (workspace disclosure), P0-04 (icon sidebar). Create a second round of mockups incorporating these changes.

3. **During Qt implementation:** Address P1 items, especially P1-02 (scene tree search), P1-08 (command palette), and P1-10 (light list).

4. **Before v0.15 ship:** Conduct a lightweight usability test with 3-5 target users (ideally one from each persona). Have them perform the core workflow (open stage, find prim, inspect properties, identify which layer set a value, make an edit). Measure task completion rate and time. This will validate or invalidate the P0 fixes.

5. **Post-ship:** Address P2 items based on user feedback data.

---

*Review conducted against BIF mockups dated 2026-03-31. Revisit after Qt prototype is functional for task-based usability testing.*
