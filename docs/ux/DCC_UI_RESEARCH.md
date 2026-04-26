# DCC Tool UI/UX Research for BIF

**Date:** 2026-03-30
**Purpose:** Inform BIF's Qt migration (v0.15.0) and layer-aware UI (v0.14.0) with evidence-based design patterns from industry-leading VFX and creative tools.
**Target Users:** VFX artists (lighting, layout, scene assembly), technical directors, pipeline TDs.

---

## Table of Contents

1. [Tool-by-Tool Analysis](#1-tool-by-tool-analysis)
2. [Research Question Findings](#2-research-question-findings)
3. [Recommendations for BIF](#3-recommendations-for-bif)
4. [Anti-Patterns to Avoid](#4-anti-patterns-to-avoid)
5. [Innovation Opportunities](#5-innovation-opportunities)

---

## 1. Tool-by-Tool Analysis

### 1.1 Clarisse iFX (Primary Inspiration)

**What it does well:**

- **Browser-centric workflow.** Clarisse's scene browser IS the product. Everything flows from a hierarchical scene tree that doubles as a project organizer. Items are drag-and-drop composable. This is the closest model to BIF's "stage tree" concept.
- **Image panel as first-class citizen.** Clarisse treats the render view not as a pop-up but as a docked panel equal to the 3D viewport. Artists can tile multiple render regions. BIF should consider this for Ivar renders.
- **Scatterer/Combiner paradigm.** Instances are managed through dedicated Scatterer and Combiner objects in the scene tree, not hidden inside node internals. The hierarchy is explicit. BIF's PointInstancer and Scatter nodes could surface instance counts and prototype lists directly in the scene browser.
- **Context system.** Clarisse contexts are folders that scope visibility and render membership. An artist can work inside `/seq01/sh010/lighting/` and only see relevant items. BIF's planned Context System (v0.19.0) maps directly to this.
- **Spreadsheet editor.** Clarisse provides a spreadsheet view for bulk property editing across many objects. Extremely efficient for lighting artists adjusting 50 lights at once.

**What it does poorly:**

- **Dated visual design.** The UI chrome feels mid-2010s. Icons are small and low-contrast. The color palette is muddy gray-brown. BIF should modernize this.
- **Cramped toolbars.** Horizontal toolbars with tiny icons and no labels. Discoverability is poor for new users.
- **No node graph.** Clarisse is entirely hierarchy-based. This works for scene assembly but makes procedural workflows (scatter rules, material blending) opaque. BIF's hybrid approach (node graph + scene tree) addresses this gap.
- **Weak undo feedback.** Undo/redo gives minimal visual feedback about what changed. Easy to lose track.

**What BIF should borrow:**

- Scene browser as primary navigation (already planned)
- Spreadsheet bulk editor for properties (future milestone)
- Context/scoping system for large scenes
- Render view as a docked first-class panel

---

### 1.2 Houdini (Node Graph Reference)

**What it does well:**

- **Network editor flexibility.** Houdini's node graph supports dive-in (entering a node to see its children), bookmarks, sticky notes, network boxes for grouping, and color-coded node shapes by type. The graph is infinitely zoomable with LOD — zoomed out, you see colored dots; zoomed in, you see full parameter previews.
- **Parameter pane binding.** Any node clicked in the network editor populates the parameter pane. The pane is context-sensitive — different node types show completely different UIs. Parameters group into folders/tabs with collapsible sections.
- **Radial menus (pie menus).** Houdini's TAB menu in the network editor is legendary. Press TAB, type a few characters, and fuzzy-match finds the node you want. BIF should implement a similar command palette for node creation.
- **Channel referencing.** Parameters can reference other parameters via expressions. The UI shows referenced values in a different color (green), making data flow visible outside the graph.
- **Multiple network editor panes.** Power users often have 2-3 network editors open at different hierarchy levels simultaneously. Houdini's pane system lets any panel type be placed anywhere.

**What it does poorly:**

- **Intimidating parameter panels.** A single node can have 50+ parameters spread across 8+ tabs. No progressive disclosure — everything is visible at once. New users are overwhelmed.
- **Inconsistent UI age.** Some panels (network editor, viewport) feel modern. Others (render settings, output driver parameters) feel like 2005. The mix is jarring.
- **Deep nesting confusion.** Diving 5 levels deep into node networks, users lose their location. The breadcrumb bar helps but is easy to miss. BIF should make hierarchy location visually prominent.
- **Dark-on-dark readability.** Default theme has low contrast in some areas. Parameter labels can be hard to read against the background.

**What BIF should borrow:**

- TAB/fuzzy-search node creation menu
- Node type color-coding by category (blue=composition, orange=operations already planned)
- Network LOD (simplified view when zoomed out)
- Sticky notes and network boxes for graph organization
- Parameter grouping with collapsible folders

**What BIF should avoid:**

- Showing all parameters by default (use progressive disclosure)
- Deep nesting without strong location awareness
- Mixing old and new UI paradigms

---

### 1.3 Nuke (Node Graph + Viewer + Properties)

**What it does well:**

- **The trinity layout.** Nuke's default layout is the gold standard for node-based compositing: node graph (bottom), viewer (top-left), properties (top-right). This three-panel pattern is what most node-based tools converge on. BIF's "three views of one truth" maps perfectly.
- **Knob panel simplicity.** Nuke's property panel (called "knobs") is clean. Each node shows only its relevant parameters. Parameters are grouped but not over-tabbed. Most nodes have < 15 visible knobs. This is a masterclass in restraint.
- **Viewer input selection.** In Nuke, you can view ANY node's output by connecting it to a Viewer node (or pressing 1/2/3). The viewport reflects what you're inspecting, not just the final output. BIF could let artists "solo" any node's contribution to the scene.
- **Backdrop nodes.** Large colored rectangles that sit behind groups of nodes, providing organizational context and documentation. Simpler than Houdini's network boxes.
- **Curve editor integration.** Animation curves are editable inline in the properties panel or in a dedicated curve editor panel. Smooth context switch.

**What it does poorly:**

- **No scene hierarchy.** Nuke is flat — no concept of scene tree or hierarchy. Everything is nodes in a 2D graph. For a compositing tool this works; for scene assembly it would not. BIF needs both.
- **Plugin UI inconsistency.** Third-party nodes (OFX plugins) often have wildly different UI styling than native nodes. BIF should enforce a style guide for any future plugin system.
- **Limited panel customization.** Nuke's panel layout is less flexible than Houdini's. You can rearrange but the options are more constrained.

**What BIF should borrow:**

- Clean knob/property panel design with minimal visible parameters
- Node "solo" viewing — inspect any node's output in the viewport
- Backdrop nodes for graph organization
- The viewer-graph-properties trinity layout as default

---

### 1.4 Katana (USD-Native Scene Assembly)

**What it does well:**

- **Scene graph as primary view.** Katana's scene graph panel is the most USD-aware UI in production. It shows the composed stage, lets you expand the hierarchy, and importantly shows which locations are "live" (computed by the graph) vs "static" (from USD). BIF's CachedSceneGraph already does this.
- **Attribute editor with opinion indicators.** Katana shows where each attribute value comes from — which node in the graph set it, whether it's inherited, whether it's an override. Color-coded: white = set here, gray = inherited, yellow = overridden upstream. This is exactly what BIF needs for layer attribution.
- **Graph State concept.** Katana's "Graph State" determines render-time vs interactive evaluation. Different states can show different levels of detail. Maps to BIF's PayloadPolicy concept.
- **Catalog (render history).** Katana keeps a catalog of previous renders, letting artists compare versions side-by-side. Powerful for iterative lighting work.
- **Material assignment via drag.** Materials are assigned by dragging from a material list to a scene graph location. Visual, intuitive, fast.

**What it does poorly:**

- **Steep learning curve.** Katana's UI assumes deep USD/rendering knowledge. No gentle onboarding. The graph-to-scene-graph relationship is confusing for beginners.
- **Dense, clinical UI.** Katana's interface is information-dense but cold. Gray on gray. It communicates "enterprise tool" not "creative tool." BIF should feel warmer.
- **Slow scene graph expansion.** On large scenes, expanding the scene graph can lag. BIF should virtualize the tree (only render visible rows).
- **No inline code preview.** Katana doesn't show you the underlying USD. You need to export and inspect externally. BIF's USDA code preview panel is a genuine differentiator.

**What BIF should borrow:**

- Opinion/attribution indicators on properties (color-coded by source layer)
- Render catalog for comparing iterations
- Material drag-and-drop assignment
- Scene graph "live location" indicators

**What BIF should avoid:**

- Clinical, cold visual design
- Assuming deep technical knowledge for basic tasks

---

### 1.5 Blender 4.x (Modern Redesign Reference)

**What it does well:**

- **The 2.8+ redesign.** Blender's UI overhaul (2.8-4.x) is the most successful DCC redesign in history. Key changes: consistent icon language, left-click select as default, simplified header bars, removal of redundant UI elements, and a cohesive dark theme with accent colors for selection (blue), active state (white), and errors (red).
- **Sidebar (N-panel) progressive disclosure.** Blender hides detailed properties in a slide-out sidebar. The main viewport is uncluttered by default. Properties appear on demand. This is the "show less, reveal more" philosophy BIF should adopt.
- **Workspace tabs.** Along the top, Blender has tabs for Layout, Modeling, Sculpting, UV Editing, Shading, Animation, Rendering, Compositing. Each tab reconfigures all panels for that task. One click switches your entire UI context. This directly answers the "task-focused UI" question.
- **Consistent property editor.** Every object type uses the same property editor panel with consistent tab icons (object, modifiers, materials, constraints, physics). The mental model is: select thing, see its properties in the same place, always.
- **Floating panels and popups.** Shift+A brings an "Add" menu. F3 brings a search-everything command palette. Right-click brings a context menu. No permanent toolbar needed — tools are summoned.
- **Proportional spacing.** Blender uses generous padding (8-12px) between property groups, clear section headers with horizontal rules, and consistent typography hierarchy. It breathes.

**What it does poorly:**

- **Modal complexity.** Blender's mode system (Object Mode, Edit Mode, Sculpt Mode) can be confusing. You can't select vertices in Object Mode. BIF should avoid modes that lock out functionality.
- **Modifier stack is hidden.** Modifiers are powerful but buried in a tab of the property editor. For a procedural tool like BIF, the node graph should be more prominent than a modifier stack.
- **Keymap learning curve.** Despite improvements, Blender still relies heavily on keyboard shortcuts. BIF should ensure all common operations are discoverable without memorization.

**What BIF should borrow:**

- Workspace tabs for task switching (Lighting, Layout, Material, Assembly)
- N-panel progressive disclosure sidebar
- Command palette (F3 / Ctrl+P style search)
- Consistent accent color language (selection=blue, active=white, warning=amber)
- Proportional spacing and visual breathing room

---

### 1.6 Blackmagic DaVinci Resolve (Premium Dark UI)

**What it does well:**

- **Page-based workflow.** Resolve has distinct "pages" (Media, Cut, Edit, Fusion, Color, Fairlight, Deliver). Each page is a completely different UI optimized for one task. This is workspace tabs taken to the extreme — and it works beautifully. Artists never see irrelevant controls.
- **Restrained color palette.** Resolve uses a very tight color palette: dark charcoal backgrounds (#1a1a1a to #2d2d2d), subtle 1px borders (#3a3a3a), and orange accent for active/selected items. This restraint is what makes it feel "premium." Most DCCs use too many colors.
- **Micro-interactions.** Subtle hover effects, smooth panel transitions, gentle fade-ins on disclosure triangles. These tiny details communicate quality. BIF's Qt implementation should budget time for micro-animations (100-200ms transitions).
- **Node graph in Fusion.** Resolve's Fusion page has a clean node graph that uses shape and color to distinguish node types. Merge nodes are triangular. Input/output nodes are rectangular. Color correction nodes are circular. Shape encodes function.
- **Inspector panel.** Right-side inspector shows properties of the selected item. Clean, single-column layout. Parameters are grouped with collapsible headers. This is simpler than Houdini's or Nuke's approaches.
- **Full-screen toggle.** Any panel can go full-screen with a single key. When you need to focus on color grading, the scopes fill the screen. When you need the timeline, it fills the screen. BIF's viewport should support this.

**What it does poorly:**

- **Memory-heavy UI.** Resolve's rich UI consumes significant GPU memory for its own rendering. BIF should keep UI rendering lightweight since the 3D viewport needs the GPU.
- **Fusion node graph feels separate.** The Fusion page feels like a different application grafted onto Resolve. Integration is visual but not deep. BIF's node graph should feel native, not bolted on.

**What BIF should borrow:**

- Tight, restrained color palette (2-3 background shades, 1 accent color)
- Page/workspace paradigm for task-focused UI
- Micro-animations for polish (hover, transitions, disclosure)
- Shape-coded nodes in the graph
- Full-screen panel toggle
- Single-column inspector with collapsible groups

---

## 2. Research Question Findings

### 2.1 Panel Layout Patterns

**Finding: The "T-layout" is the dominant pattern for 4+ panels.**

The most effective arrangement across all studied tools:

```text
+---------------------------------------------------+
|  Toolbar / Workspace Tabs                         |
+------------+------------------------+-------------+
|            |                        |             |
|  Scene     |    3D Viewport /       |  Property   |
|  Tree /    |    Render View         |  Inspector  |
|  Browser   |                        |             |
|            |                        |             |
|            +------------------------+-------------+
|            |   Node Graph / Code Preview          |
|            |                                      |
+------------+--------------------------------------+
```

**Evidence:**

- Clarisse, Katana, Blender all use left-tree, center-viewport, right-properties
- Nuke, Houdini, Resolve put the node graph below the viewport
- The scene tree is always on the left (Western reading order: scan left-to-right, hierarchy first)
- Properties/inspector is always on the right (detail view after selection)

**BIF recommendation:** Default to this T-layout. The four panels map to BIF's "three views of one truth" plus the scene browser:

1. **Left:** Scene browser (USD stage tree)
2. **Center-top:** 3D viewport (and/or Ivar render view)
3. **Center-bottom:** Node graph (or USDA code preview, tabbed)
4. **Right:** Property inspector (with layer attribution)

**Critical detail:** The center-bottom space should be TABBED between node graph and USDA code preview. Showing both simultaneously is too dense. Let artists switch between "visual flow" (node graph) and "ground truth" (USDA code) with a single click.

**Panel sizing heuristic:**

- Scene tree: 15-20% width
- Viewport: 45-55% width
- Inspector: 25-30% width
- Node graph/code: 30-40% height of center+bottom

### 2.2 Layer/Stack Visualization

**Finding: Color-coded attribution with strength indicators is the industry standard.**

Studied tools and their approaches:

| Tool | Layer Visualization | Override Indicator | Source Attribution |
| ------ | ------------------- | ------------------- | ------------------- |
| Photoshop | Vertical stack, thumbnails, eye icon for visibility | Bold layer name | Layers panel shows active layer in blue |
| After Effects | Vertical stack with transform columns | Override switch per property | Property links show expression icons |
| Katana | Scene graph with inherited/set/overridden colors | White=set, gray=inherited, yellow=overridden | Node name in tooltip |
| Clarisse | Context-based grouping | Bold for local overrides | Context path in property panel |
| Nuke | No layer concept | Knob value coloring (green=expression, blue=keyframe) | Node name in knob panel |
| Blender | Modifier/constraint stack | - | Source object in modifier |

**Best-in-class: Katana + Photoshop hybrid.**

BIF should combine:

1. **Photoshop's layer stack panel** for the sublayer list (vertical, reorderable, eye icons for muting, lock icons for read-only)
2. **Katana's attribute coloring** for the property inspector:
   - **Bold white text** = set on current edit layer (your changes)
   - **Normal gray text** = inherited from a weaker layer (not yours)
   - **Amber/orange text** = overridden by a stronger layer (someone else's layer wins)
   - **Strikethrough gray** = muted layer contribution
3. **Tooltip attribution**: Hover on any property value to see "Set by: lighting.usd (layer 3/4)" with the full opinion stack

**Layer stack panel design:**

```text
+----------------------------------+
|  Layer Stack                [+]  |
+----------------------------------+
|  [eye] [lock] lighting.usd  *   |  <- bold, blue highlight = edit target
|  [eye] [lock] fx.usd            |  <- normal, dimmed = read-only
|  [eye] [lock] animation.usd     |
|  [eye] [lock] layout.usd        |  <- weakest (bottom of stack)
+----------------------------------+
|  [Isolate] [Show All] [Diff]    |
+----------------------------------+
```

The `*` indicator and blue highlight mark the active edit layer. The `[Diff]` button shows changes only on the active layer (a USD diff view). This is the killer feature no current tool has well.

### 2.3 Information Density vs. Cleanliness

**Finding: Progressive disclosure is the universal solution. The best tools show 20% of controls by default and reveal 80% on demand.**

**Three-tier disclosure pattern** (observed across Blender, Resolve, Nuke):

| Tier | Visibility | Content | Example |
| ------ | ----------- | --------- | --------- |
| **Always visible** | Default | Core properties (name, transform, material) | Blender's header row |
| **One click away** | Collapsed section | Secondary properties (display, render settings) | Blender's N-panel |
| **Expert mode** | Hidden until toggled | Advanced/debug properties (primvar overrides, custom attributes) | Houdini's "Edit Parameter Interface" |

**Specific patterns that work:**

1. **Collapsible sections with memory.** Sections remember their open/closed state per node type. If you always expand "Material Bindings" on Mesh nodes, it stays expanded next time.
2. **Search-everything in properties.** Houdini, Blender, and VS Code all let you search/filter properties by name. Essential when a node has 30+ parameters. BIF should have a filter bar at the top of the property inspector.
3. **Contextual default hiding.** Show transform properties only for transformable prims. Show material properties only for prims that can be shaded. Show light properties only for lights. Never show irrelevant sections.
4. **Compact vs. expanded toggle.** Resolve's Inspector has a "compact mode" that shows just labels and values in a tight list, vs. "expanded mode" with sliders and color wheels. BIF's property inspector should support both.
5. **Status bar for non-critical info.** Frame count, polycount, memory usage, render progress -- these go in a bottom status bar, not in panels. Keeps panels focused.

**BIF-specific recommendation:** For the USDA code preview panel, use VS Code's approach: syntax-highlighted read-only view with code folding. Fold `def Mesh` blocks by default, show only the current selection's subtree expanded. Add a "Show Full Layer" toggle for TDs who want to see everything.

### 2.4 Context Switching (Task-Focused UI)

**Finding: Workspace presets with one-click switching are the modern standard. BIF should ship 4 built-in workspaces.**

**How top tools handle task modes:**

| Tool | Mechanism | Transition Speed | Customizable? |
| ------ | ----------- | ----------------- | --------------- |
| Blender | Workspace tabs (top bar) | Instant | Yes, save custom |
| Resolve | Pages (bottom bar) | ~200ms fade | No (fixed pages) |
| Houdini | Desktop presets (menu) | Instant | Yes, save custom |
| Nuke | Workspace layouts (menu) | Instant | Yes, save custom |
| Clarisse | No formal system | N/A | Manual rearrange |

**BIF should implement workspace tabs (Blender model) with these defaults:**

1. **Assembly** - Scene browser prominent, node graph large, viewport medium, code preview hidden
   - Payload policy: proxy placeholder mode
   - Scene tree: full expansion
   - Node graph: all node types visible

2. **Lighting** - Viewport dominant, property inspector wide, render catalog visible
   - Payload policy: camera-based deferred loading
   - Property inspector: light properties expanded by default
   - Spreadsheet editor available for multi-light editing

3. **Materials** - Viewport with material preview, property inspector showing shader params
   - Payload policy: selected asset only
   - Property inspector: material/shader properties expanded
   - USDA code preview: filtered to `/materials/` subtree

4. **Review** - Viewport full-size, minimal UI, render controls only
   - Payload policy: LoadAll
   - Minimal panels: just render controls and camera selection
   - Render catalog prominent for A/B comparison

**Transition behavior:** Switching workspace should:

- Animate panel resize (200ms ease-out, not instant -- feels more intentional)
- Preserve scroll positions in each panel
- Remember the last selection in each workspace independently
- Change the PayloadPolicy to match the task (automatic but overridable)

### 2.5 Code Preview Patterns

**Finding: Read-only code views need three things to be non-intimidating: syntax highlighting, folding, and context filtering.**

**Analysis of code-adjacent UIs in creative tools:**

| Tool | Code View | Target Audience | Key Design Choices |
| ------ | ----------- | ---------------- | ------------------- |
| VS Code | Full IDE | Developers | Syntax color, minimap, breadcrumb, folding |
| Houdini VEX editor | Embedded code editor | Technical artists | Syntax color, auto-complete, error highlighting |
| Nuke expression editor | Inline text field | Compositors | Minimal, shows result alongside expression |
| Katana | No code view | N/A | External USD inspection only |
| Blender text editor | Full panel | Technical users | Syntax color, line numbers, basic |

**BIF's USDA code preview should follow these principles:**

1. **Context-filtered by default.** When an artist selects a prim in the scene browser, the code panel scrolls to and highlights that prim's definition. Only the relevant `def` block is expanded. Everything else is folded. This makes a 10,000-line USDA file feel manageable.

2. **Layer-aware syntax highlighting.** Go beyond standard USDA syntax coloring:
   - **Blue text** for properties set on the current edit layer (your changes)
   - **Gray text** for properties from other layers (composed result)
   - **Amber gutter markers** for lines that differ from the base layer
   - This creates a visual "diff" without an explicit diff view

3. **No editing initially.** v0.15.0 ships read-only. v0.16.0 adds editing. This sets the right expectation: the code panel is for verification and understanding, not authoring. The property inspector is for authoring.

4. **Copy-friendly.** Artists and TDs will want to copy USDA snippets to paste into emails, Slack, or documentation. Ensure line numbers are not included in clipboard copies (common web/IDE mistake).

5. **Font choice matters.** Use a monospace font with good USD readability. Recommendation: JetBrains Mono or Cascadia Code. Both have good quote/bracket distinction and ligature support for `->` and `==`.

6. **Minimap for large files.** VS Code's minimap (right-side scrollbar showing code structure) helps TDs navigate large layers. Show it by default in the "Assembly" workspace, hide it in others.

### 2.6 Modern UI Trends in Creative Tools (2024-2026)

**Finding: The trend is toward "quiet confidence" -- less chrome, more content, subtle depth cues instead of hard borders.**

**Observed trends across Blender 4.x, Substance 3D, Figma, Resolve 19, Cinema 4D 2025:**

1. **Borderless panels with shadow separation.** Hard 1px borders between panels are being replaced by subtle drop shadows or 2-4px gaps with a slightly darker background. Creates depth without visual noise.
   - **BIF recommendation:** Use 2px gaps between panels with `#151515` gap color against `#1e1e1e` panel backgrounds. No visible borders.

2. **Rounded corners on internal elements.** Input fields, buttons, dropdown menus, tabs, and cards use 4-8px border radius. Panel edges remain square (they're docked). This creates a subtle "soft" feeling without looking cartoonish.
   - **BIF recommendation:** 6px radius on buttons, inputs, dropdown menus, node bodies. 0px on panel edges.

3. **Single accent color with intensity variations.** Instead of multiple colors for different states, modern tools use one accent hue at different intensities:
   - Selection: `#4C8BF5` (bright blue)
   - Hover: `#3A6BC5` (medium blue)
   - Active/focused: `#5A9BFF` (lighter blue)
   - Disabled: `#2A4B85` (dim blue)
   - **BIF recommendation:** Choose blue as the accent (maps to "composition" nodes already). Use orange sparingly for warnings/operation nodes only.

4. **Translucent overlays and floating panels.** Substance 3D and Figma use semi-transparent backgrounds for floating panels and tooltips. This maintains spatial awareness of the content beneath.
   - **BIF recommendation:** Use 85-90% opacity for floating panels, tooltips, and command palette overlay. Blur the background content slightly (4px Gaussian).

5. **Contextual toolbars.** Instead of permanent toolbars with every tool visible, modern tools show a contextual toolbar above the viewport that changes based on selection type. Select a light? See light-specific tools. Select a mesh? See mesh tools.
   - **BIF recommendation:** Replace any fixed toolbar with a contextual header bar in the viewport that adapts to the selected prim type.

6. **Type scale hierarchy.** Modern creative tools use a clear type scale:
   - Panel titles: 13-14px, medium weight, ALL CAPS or small caps
   - Property labels: 11-12px, regular weight
   - Values: 11-12px, medium weight (slightly bolder than labels)
   - Status/metadata: 10px, light weight, reduced opacity
   - **BIF recommendation:** Follow this exact scale. Use system font (Segoe UI on Windows, SF Pro on macOS, Noto Sans on Linux) for UI, monospace only for code preview.

7. **Reduced iconography.** The trend is away from icon-heavy toolbars toward text labels with small icons. Icons alone require memorization. Blender 4.x moved many toolbar entries from icon-only to icon+label.
   - **BIF recommendation:** All toolbar buttons should have text labels. Use icons as supplementary visual anchors, not as the sole affordance.

### 2.7 Accessibility in Dark UIs

**Finding: WCAG AA contrast (4.5:1 for text) is the minimum. The best tools exceed it while maintaining aesthetic quality.**

**Specific findings for 8+ hour daily use:**

1. **Background luminance sweet spot: 10-15% (HSL lightness).**
   - Too dark (#000000 to #0a0a0a): High contrast against text causes eye strain from "blooming" effect on light text
   - Too light (#303030+): Loses the immersive "content-first" feel; viewport looks washed out
   - Sweet spot: `#1a1a1a` to `#242424` for main panels, `#141414` for code/viewport areas
   - **Resolve uses:** `#1a1a1a` (main), `#2d2d2d` (panel headers)
   - **Blender uses:** `#232323` (main), `#303030` (headers)
   - **BIF recommendation:** `#1c1c1c` for main panels, `#141414` for viewport/code, `#282828` for headers and interactive areas

2. **Text contrast tiers (against #1c1c1c background):**

   | Text Purpose | Color | Contrast Ratio | Notes |
   | ------------- | ------- | --------------- | ------- |
   | Primary text (labels) | `#d4d4d4` | 10.5:1 | Comfortable for extended reading |
   | Secondary text (metadata) | `#888888` | 4.5:1 | Minimum AA, use sparingly |
   | Disabled text | `#555555` | 2.5:1 | Below AA, acceptable for disabled state |
   | Active/selected text | `#ffffff` | 13.5:1 | Used only for current selection |
   | Warning text | `#e8a838` | 6.2:1 | Amber, not red (reduces alarm fatigue) |
   | Error text | `#e85050` | 4.8:1 | Slightly desaturated red, still readable |

3. **Reduce pure white (#ffffff) usage.**
   - Pure white text on dark backgrounds causes halation (glow effect) especially on LCD displays and for users with astigmatism (~33% of population)
   - Use `#d4d4d4` to `#e0e0e0` for standard text, reserve `#ffffff` for selected/active items only
   - **Resolve and VS Code both follow this pattern**

4. **Warm vs. cool dark themes.**
   - Cool darks (blue-tinted #1a1c22) reduce eye strain slightly in dim environments and feel more "professional"
   - Warm darks (amber-tinted #221a1a) feel more "creative" and cozy but can cause color perception issues for color-critical work
   - Neutral dark (#1c1c1c) is safest for VFX where color accuracy matters
   - **BIF recommendation:** Neutral dark default. Offer warm/cool variants as theme options post-1.0.

5. **Focus indicators for keyboard navigation.**
   - All interactive elements need a visible focus ring (2px, accent color, 50% opacity)
   - Tab order should follow visual layout (left panel, then center, then right)
   - Screen reader support for scene tree and property inspector (ARIA labels on Qt widgets)
   - **Mandatory for studio adoption:** Many studios require accessibility compliance for internal tools

6. **High-contrast mode option.**
   - Some artists work in bright-lit offices (not color-grading suites)
   - Offer a "high contrast" variant: `#252525` backgrounds, `#eeeeee` text, `#6CABFF` accent
   - This should be a user preference, not a runtime switch

7. **Font size scaling.**
   - Default: 12px for property labels (sufficient for 1080p-4K displays)
   - Allow global UI scale factor (100%, 125%, 150%) for high-DPI or accessibility needs
   - Minimum touch/click target: 24px height for all interactive elements
   - **Qt makes this straightforward** via `QApplication::setFont` and style sheet scaling

---

## 3. Recommendations for BIF

### Priority 1: Ship with v0.14.0 (Layer-Aware Stage)

These are UI decisions needed before the layer awareness features land:

| Item | Recommendation | Reference Tool |
| ------ | --------------- | --------------- |
| Layer stack panel | Vertical list with eye/lock/edit-target icons | Photoshop + Katana |
| Opinion attribution | Color-coded property values (bold=yours, gray=inherited, amber=overridden) | Katana |
| Layer diff view | "Show changes" button filters scene tree to only prims modified on active layer | Git diff concept (novel for DCC tools) |
| Property inspector | Single-column, collapsible sections, search/filter bar at top | Resolve Inspector |

### Priority 2: Ship with v0.15.0 (Qt Migration)

These require Qt's richer widget toolkit:

| Item | Recommendation | Reference Tool |
| ------ | --------------- | --------------- |
| T-layout default | Left tree, center viewport+graph, right inspector | Nuke/Blender |
| Workspace tabs | 4 presets: Assembly, Lighting, Materials, Review | Blender + Resolve |
| USDA code preview | Syntax-highlighted, folded, context-filtered, read-only | VS Code |
| Command palette | Ctrl+P / TAB search for all actions and nodes | Houdini TAB + VS Code |
| Panel micro-animations | 200ms transitions on resize, fade on panel toggle | Resolve |
| Color system | Neutral dark #1c1c1c, blue accent, WCAG AA minimum | Resolve + VS Code |
| Contextual toolbar | Viewport header adapts to selected prim type | Blender 4.x |

### Priority 3: Post v0.15.0

| Item | Recommendation | Reference Tool |
| ------ | --------------- | --------------- |
| Spreadsheet editor | Bulk property editing for lights, materials, instances | Clarisse |
| Render catalog | History of renders with A/B comparison | Katana |
| Workspace customization | Save/load custom workspace layouts | Houdini/Nuke |
| Theme variants | Warm, cool, high-contrast options | General accessibility |
| Plugin UI style guide | Enforce consistent styling for third-party extensions | Nuke (anti-pattern to avoid) |

---

## 4. Anti-Patterns to Avoid

### From Houdini

- **Parameter overload.** Never show 50+ parameters without progressive disclosure. Default to collapsed sections, expand on demand.
- **Inconsistent UI age.** When migrating to Qt, ensure ALL panels get the new styling. No legacy egui panels left behind.
- **Deep nesting without breadcrumbs.** If BIF's node graph supports dive-in, always show a visible breadcrumb trail with click-to-jump-back.

### From Clarisse

- **Tiny icons without labels.** Every toolbar button needs a text label. Icons supplement, they don't replace.
- **No node graph.** BIF's hybrid approach (graph + tree) is the right call. Don't lose the graph.

### From Katana

- **Assuming technical knowledge.** Not every user knows USD. Property labels should use human-readable names ("Position" not "xformOp:translate") with technical names available on hover.
- **Cold, clinical aesthetic.** Add warmth through slightly rounded elements, generous spacing, and subtle hover animations.

### From Nuke

- **Plugin UI inconsistency.** If BIF ever has plugins, enforce a style API that constrains plugin UIs to the BIF design language.

### General Anti-Patterns

- **Modal dialogs for common operations.** Preference changes, export settings, and render settings should be panels, not blocking dialogs.
- **Undo without visual feedback.** Show a brief toast notification ("Undo: moved light_key to [0, 5, 0]") so artists know what changed.
- **Settings buried in menus.** Frequently-changed settings (viewport quality, render samples, display options) should be accessible from the viewport header, not Edit > Preferences > Viewport > Display.

---

## 5. Innovation Opportunities

These are areas where existing tools are weak and BIF can differentiate:

### 5.1 Live USDA Code Preview (No Current Tool Has This Well)

BIF's "three views of one truth" concept -- where the node graph, scene tree, and USDA code all show the same data -- is genuinely novel. No current production tool lets you:

1. Select a prim in the 3D viewport
2. See it highlighted in the scene tree
3. See its USDA definition in a code panel
4. See which layer each property comes from
5. All simultaneously, all live-updating

**This is BIF's primary UX differentiator.** Invest heavily in making this seamless.

### 5.2 Layer Diff View

No current DCC tool shows "what changed on this layer" as a first-class view. Git-style diff visualization applied to USD layers:

- Green highlights for new prims/properties added on this layer
- Blue highlights for properties overridden on this layer
- Red highlights for prims deactivated (USD deactivation) on this layer
- A "changes only" filter that hides everything untouched by the active layer

This would be enormously valuable for pipeline TDs reviewing artist work.

### 5.3 Intelligent Payload Loading Tied to Workspace

No tool currently ties scene loading strategy to the artist's task. BIF's PayloadPolicy + Workspace combination means:

- Switch to "Lighting" workspace: auto-load camera frustum geometry, lights, skip distant unlit geometry
- Switch to "Assembly" workspace: load bounding boxes for everything, full geo for nothing
- Switch to "Materials" workspace: load only the selected asset at full resolution

This reduces memory usage AND cognitive load simultaneously.

### 5.4 Command Palette with USD Awareness

Go beyond VS Code's command palette. BIF's should understand USD:

- Type "find light" to locate all UsdLux prims in the stage
- Type "override material on /world/hero" to create a material override on the active layer
- Type "load payload /world/env" to load a specific payload
- Type "diff lighting.usd" to show changes on the lighting layer

This bridges the gap between GUI and CLI workflows that TDs need.

### 5.5 Onboarding Without Dumbing Down

Ship a "first launch" experience that:

1. Shows the T-layout with labeled panel purposes ("This is your scene tree", "This is where you edit properties")
2. Opens a sample USD file (included with BIF) that has multiple layers, materials, lights
3. Walks through one task: "Select the key light, adjust its intensity, see the change in the code preview"
4. Takes < 2 minutes, can be dismissed permanently, and is re-accessible from Help menu

No VFX tool does this well. Most assume you'll read documentation or attend a training session.

---

## Appendix: Color System Specification

For implementation reference during Qt migration.

### Background Colors

```text
--bg-viewport:     #141414    /* Viewport and code panel background */
--bg-panel:        #1c1c1c    /* Main panel backgrounds */
--bg-header:       #282828    /* Panel headers, section headers */
--bg-input:        #2a2a2a    /* Input fields, dropdown backgrounds */
--bg-hover:        #333333    /* Hover state for list items */
--bg-selected:     #1a3a5c    /* Selected item background (blue-tinted) */
--bg-gap:          #151515    /* Gap between panels */
```

### Text Colors

```text
--text-primary:    #d4d4d4    /* Default label text */
--text-secondary:  #888888    /* Metadata, hints, secondary info */
--text-disabled:   #555555    /* Disabled controls */
--text-active:     #ffffff    /* Selected/focused item text */
--text-code:       #c8c8c8    /* Code preview text */
```

### Accent Colors

```text
--accent-blue:     #4C8BF5    /* Selection, edit-layer indicator, links */
--accent-blue-dim: #2A4B85    /* Inactive state of blue elements */
--accent-orange:   #E8A838    /* Warnings, operation nodes, override indicators */
--accent-red:      #E85050    /* Errors, destructive actions */
--accent-green:    #5CB85C    /* Success, new additions in diff view */
```

### Layer Attribution Colors (Property Inspector)

```text
--layer-own:       #ffffff    /* Property set on active edit layer (bold weight) */
--layer-inherited: #888888    /* Property inherited from weaker layer */
--layer-overridden:#E8A838    /* Property overridden by stronger layer */
--layer-muted:     #555555    /* Property from a muted layer (strikethrough) */
```

### Node Graph Colors

```text
--node-composition:#4C8BF5    /* Blue: UsdRead, UsdPrim, GraftBranches, UsdExport */
--node-operation:  #E8A838    /* Orange: Scatter, Xform, PointInstancer */
--node-material:   #9B59B6    /* Purple: Material nodes (future) */
--node-render:     #5CB85C    /* Green: IvarRender, HdriEnvironment */
--node-primitive:  #888888    /* Gray: Primitive (cube, sphere, etc.) */
```

---

## Appendix: Typography Specification

```text
--font-ui:         "Segoe UI", system-ui, sans-serif    /* Windows default */
--font-mono:       "Cascadia Code", "JetBrains Mono", "Consolas", monospace

--font-size-panel-title:  13px, weight 600, letter-spacing 0.5px
--font-size-label:        12px, weight 400
--font-size-value:        12px, weight 500
--font-size-status:       10px, weight 300, opacity 0.7
--font-size-code:         12px, weight 400 (monospace)

--line-height-ui:         1.4
--line-height-code:       1.5
```

---

## Appendix: Spacing Specification

```text
--spacing-panel-padding:     12px    /* Inside panel edges */
--spacing-section-gap:        8px    /* Between collapsible sections */
--spacing-property-row:       4px    /* Between property label-value rows */
--spacing-group-header:      16px    /* Above section headers */
--spacing-panel-gap:          2px    /* Between docked panels */
--spacing-button-padding:  8px 16px  /* Inside buttons */
--spacing-input-padding:   4px  8px  /* Inside input fields */
--spacing-icon-gap:           6px    /* Between icon and label */
--min-click-target:          24px    /* Minimum interactive element height */
--border-radius-element:      6px    /* Buttons, inputs, dropdowns, nodes */
--border-radius-panel:        0px    /* Panel edges (docked, no rounding) */
```

---

*Research compiled for BIF v0.14.0-v0.15.0 planning. Revisit after first Qt prototype for validation testing with target users.*
