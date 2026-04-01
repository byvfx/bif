# UX Architect Review: BIF Qt Mockups

> **Status:** Findings incorporated into [UI_DESIGN.md](UI_DESIGN.md) on 2026-03-31. This file is the audit trail — the spec lives in UI_DESIGN.md.

**Reviewer:** ArchitectUX Agent
**Date:** 2026-03-31
**Mockups Reviewed:**
1. Assembly Workspace Detail (`assembly_workspace_detail/screen.png`)
2. Assembly Workspace Qt Implementation (`assembly_workspace_qt_implementation/screen.png`)
3. Lighting Workspace Qt Implementation (`lighting_workspace_qt_implementation/screen.png`)
4. Materials Workspace (`materials_workspace/screen.png`)

**Reference Documents:**
- `assets/stitch_bif_ui/obsidian_graphite/DESIGN.md` (Quiet Confidence design system)
- `docs/ux/DCC_UI_RESEARCH.md` (DCC tool research)
- `docs/ux/UI_DESIGN.md` (BIF UI brainstorm)

---

## Executive Summary

The mockups represent a strong vision for BIF's Qt UI. The T-layout is structurally sound, the workspace-switching concept is well-executed, and the "Quiet Confidence" aesthetic reads as genuinely premium. However, there are specific issues around design system compliance, component consistency across workspaces, opinion visualization legibility, and information density that need resolution before implementation begins.

**Overall Assessment:** 7.5/10 -- strong foundation with specific areas needing tightening.

---

## 1. Layout Architecture

### T-Layout Evaluation

The T-layout is correctly implemented across all four mockups: left dock (scene tree + layer stack), center viewport, right inspector, bottom node graph. This matches the research findings from DCC_UI_RESEARCH.md Section 2.1 and the decisions in UI_DESIGN.md.

**Strengths:**
- Viewport is visually dominant in all workspaces, occupying approximately 50-55% of horizontal space and 60-65% of vertical space. This matches the "viewport owns 60%+" target from UI_DESIGN.md.
- The bottom node graph area is correctly tabbed (Node Graph | USDA Preview | Render Log / Console), confirming the decision to share this space rather than show all three simultaneously.
- The left dock correctly stacks Scene Tree above Layer Stack, matching the Clarisse-inspired "browser-centric" pattern.

**Issues:**

**1.1 -- Panel Proportion Inconsistency Across Workspaces**

The Assembly Detail mockup shows the left dock at roughly 20% width, which is appropriate. But the Assembly Qt Implementation mockup compresses the left dock to approximately 15%, making the scene tree items feel cramped. The layer stack items ("shot_010_v2.usd", "layout_base.v2.usd", "anim_reference.usd") are truncated.

*Recommendation:* Enforce a minimum left dock width of 220px. Scene tree items with USD paths need horizontal room. At 15% of a 1920px screen, the left dock is only 288px -- workable but tight for paths like `/root_assembly/hero_asset_prim`. At lower resolutions (1680px), 15% yields only 252px. Set 220px as the `QDockWidget::setMinimumWidth()` floor.

**1.2 -- Right Inspector Width Varies Too Much**

In the Assembly Detail mockup, the right inspector is approximately 25% width -- appropriate for Transform/Visibility/Prim Metadata sections. In the Lighting workspace, it narrows to roughly 20% to give more viewport space. But the Lighting inspector contains slider controls for Intensity, Exposure, and Color Temperature that need horizontal room for the value + slider combination.

*Recommendation:* Set the right inspector minimum width to 280px. Slider controls need at least 120px of track length to be usable for fine-grained value scrubbing, plus label and value display. Below 280px total, the slider becomes too compressed for VFX-precision adjustments (lighting artists scrub in 0.01 increments).

**1.3 -- Bottom Dock Height in Lighting Workspace**

The Lighting workspace correctly minimizes the bottom dock (node graph area) to make the viewport dominant. The tab bar is visible but the content area is collapsed. This is good -- lighting artists rarely need the node graph. However, the collapsed state shows only the tab labels and no grab handle or visual affordance for expanding.

*Recommendation:* When the bottom dock is collapsed, show a 4px drag handle bar with a subtle hover state (`surface_container_high` #2a2a2a on hover). Double-click to expand to 30% height. This matches the "thin grab bar, not a button" spec from UI_DESIGN.md.

**1.4 -- Resizability Concerns for Qt Implementation**

All four mockups show fixed-feeling proportions. In Qt, `QSplitter` handles inter-panel resizing. The implementation needs to define:
- Minimum sizes for all four zones (left: 220px, right: 280px, bottom: 32px collapsed / 180px minimum expanded, viewport: 400x300px minimum)
- Snap-to-collapse thresholds (drag left dock below 100px and it collapses entirely)
- Double-click-to-reset behavior on splitter handles (restore default proportions)
- Per-workspace proportion memory (Lighting workspace remembers its own splitter positions separately from Assembly)

---

## 2. Design System Compliance

### Obsidian Graphite / "Quiet Confidence" Adherence

**2.1 -- The "No-Line Rule" (DESIGN.md Section 2)**

*Assembly Detail mockup:* Mostly compliant. Panel separation is achieved through tonal shifts rather than borders. The gap between the scene tree and viewport reads as a shadow gap, not a border. However, there is a visible 1px border around the Layer Stack section and around the Inspector panel's input fields for Transform values (Translate, Rotate, Scale). The DESIGN.md explicitly states: "Traditional 1px solid borders are prohibited for sectioning" and input fields should use `surface_container_lowest` (#0e0e0e) background with no border.

*Lighting workspace:* Better compliance. The property inspector uses recessed input fields without visible borders. The slider tracks appear borderless. But the "ATTRIBUTES" section header has what appears to be a horizontal divider line beneath it. The DESIGN.md's Cards & Lists section says "Forbid divider lines" and instead use "Spacing Scale 3 (0.6rem) of vertical white space."

*Materials workspace:* The Node Graph area at bottom shows nodes with visible borders around each node card. The DESIGN.md does not explicitly address node graph styling, but the general principle of tonal shifts over borders should apply. Node bodies should use `surface_container_high` (#2a2a2a) against the `surface` (#131313) graph background, with no outline.

*Recommendation:* Audit every mockup for 1px borders and replace with:
- Panel sections: shadow gaps using `surface_container_lowest` (#0e0e0e) 1px gap
- Input fields: `surface_container_lowest` (#0e0e0e) background, no border, 1px glow `primary_container` (#4a9eff) on focus only
- Section headers: 0.6rem (approximately 10px) vertical whitespace below, no horizontal rule
- Node bodies: tonal shift only, border appears only on selection (1px `primary_container`)

**2.2 -- Surface Hierarchy Compliance**

The Assembly Detail mockup shows the correct layering:
- Base/viewport area: dark, approximately `surface` (#131313) -- correct
- Left dock (scene tree): slightly lighter, approximately `surface_container` (#20201f) -- correct
- Inspector panel: matches left dock tone -- correct
- Input fields within inspector: darker recessed tone -- correct per "cut into the panel surface" rule

The Assembly Qt Implementation mockup appears to use a slightly different tone for the left dock -- it reads darker than the detail mockup, closer to `surface` than `surface_container`. This may be a rendering artifact but should be verified.

*Recommendation:* Create a Qt stylesheet constant file mapping every surface tier to its exact hex value. Reference DESIGN.md surface hierarchy:
```
surface:                    #131313  (viewport, base)
surface_container_lowest:   #0e0e0e  (shadow gaps, recessed inputs)
surface_container_low:      #1b1b1b  (nested inputs inside panels)
surface_container:          #20201f  (primary work panels)
surface_container_high:     #2a2a2a  (elevated elements, hover states)
surface_container_highest:  #353535  (floating menus, context menus)
```

**2.3 -- Typography Compliance**

*Assembly Detail mockup:* Section headers ("SCENE TREE", "LAYER STACK", "INSPECTOR", "TRANSFORM", "VISIBILITY", "PRIM METADATA") appear to use uppercase with letter spacing -- correct per DESIGN.md Section 3 ("label-sm, Inter, 0.6875rem, Uppercase, 0.05em letter-spacing"). Property values appear monospaced -- correct.

*Lighting workspace:* Section headers ("PROPERTY INSPECTOR", "ATTRIBUTES") follow the same pattern. The slider value labels ("456.88 cd", "12.5 EV", "6500 K") use monospace -- correct for data entry.

*Materials workspace:* The "MATERIAL PARAMETER SHEET" header is properly uppercase. Property labels ("Color", "Metallic", "Roughness") appear in regular case, which is correct -- they are property labels, not section headers.

*Issue:* The workspace tab labels in the top navigation bar ("ASSEMBLY", "LIGHTING", "MATERIALS", "REVIEW") appear to use the same size as panel section headers. Per DESIGN.md Section 3, these should use `headline-sm` (Manrope, 1.5rem) for "major mode switching." The mockups show them at what appears to be approximately 0.75-0.875rem -- too small for workspace switching, which is the highest-level navigation action.

*Recommendation:* Increase workspace tab font size to `headline-sm` (1.5rem / 24px, Manrope). This creates clear visual hierarchy: workspace tabs are larger than panel section headers, which are larger than property labels. Current mockups flatten this hierarchy.

**2.4 -- The "Glass & Gradient" Rule**

The Assembly Detail mockup shows a translucent HUD overlay at the bottom of the viewport (viewport mode tabs: "Node Graph", "USDA Preview", "Render Log" plus viewport controls). This partially follows the glassmorphism spec but does not appear to have the specified `20px backdrop-blur` -- the viewport content behind it is still sharp.

The Lighting workspace shows viewport control buttons (play, camera, grid icons) centered at the bottom of the viewport. These appear to float over the rendered scene, which is the correct behavior for a HUD, but the background treatment is unclear -- they may be using a solid dark background rather than the specified `surface_variant` (#353535) at 70% opacity with blur.

*Recommendation:* All floating viewport HUDs must use:
```css
background: rgba(53, 53, 53, 0.7);  /* surface_variant at 70% */
backdrop-filter: blur(20px);
border-radius: 0.375rem;            /* md radius */
```
In Qt, this requires `QGraphicsBlurEffect` or a custom widget with `setAttribute(Qt::WA_TranslucentBackground)` and manual compositing. If the blur is too expensive during render, fall back to 90% opacity solid `surface_variant` -- the blur is a "nice to have" but the translucency is essential to maintain viewport immersion.

---

## 3. Component Consistency

### Cross-Workspace Component Audit

**3.1 -- Node Graph Nodes**

*Assembly Detail:* Shows three nodes (UsdRead, XForm, Primitive) with colored headers (blue for UsdRead, orange-amber for XForm, orange for Primitive) and a dark body. Connection wires are visible. Node bodies appear to have rounded corners.

*Assembly Qt Implementation:* Shows four nodes (ROOT_COMP, MESH/SCENE, BRANCH, and another) with similar colored headers but the body proportions look different -- the header is taller relative to the body, and the node width appears wider. The "warning" triangle badge on the BRANCH node is a good addition not seen in the detail mockup.

*Materials workspace:* Shows three nodes (UsdUVTexture, OpenPBR Surface, MaterialOut) with different proportions again -- the OpenPBR Surface node has visible input/output port labels ("base_color", "roughness", "surface") that are not present in the Assembly mockup nodes.

*Issue:* Node visual language is inconsistent across workspaces. Header height, body width, port label visibility, and overall proportions vary. The node graph should use a single component definition regardless of workspace.

*Recommendation:* Define a canonical node component spec:
- Header: 28px height, colored by node category (composition=#4C8BF5, operation=#E8A838, material=#9B59B6, render=#5CB85C, primitive=#888888), `md` radius on top corners only
- Body: `surface_container_high` (#2a2a2a) background, `md` radius on bottom corners, minimum width 140px
- Ports: 10px circles, left side for inputs, right side for outputs, 8px monospace labels visible when zoom level > 80%
- Badge area: top-right corner of header for warning/error indicators
- Selected state: 2px `primary_container` (#4a9eff) outline, no other border
- Width adjusts to content but snaps to 20px grid increments

**3.2 -- Property Rows**

*Assembly Detail:* Transform properties show a label ("Translate", "Rotate", "Scale") followed by three monospace input fields for X, Y, Z values. The fields have visible borders and appear on a darker background. The "Active Layer" label in the top-right of the Inspector section links to the layer attribution system.

*Lighting workspace:* Properties show a different layout -- "Intensity" has a label, a slider, and a value with unit suffix ("456.88 cd"). "Color Temperature" has a label, a colored slider (blue-to-orange gradient), and a value with unit ("6500 K"). This is a different row format than the three-field vector layout.

*Materials workspace:* Properties show label-value pairs in a compact list ("Color" with a swatch, "Metallic" with a single value "0.800", "Roughness" with "0.120"). No sliders visible. Different again from both Assembly and Lighting.

*Assessment:* The variation is actually appropriate here. Property row layout should adapt to data type:
- Vec3 (Transform): three-field row with X/Y/Z labels
- Float with range (Intensity, Roughness): slider + value
- Color: swatch + hex/float value
- Enum (Shading Model): dropdown
- Bool (Shadow enabled): toggle switch
- String (Path): text field with browse button

This is not an inconsistency -- it is type-driven formatting, which matches Houdini's and Nuke's approach. The DCC_UI_RESEARCH.md Section 1.2 notes Houdini's "different node types show completely different UIs" as a strength.

*Recommendation:* Codify a property row component library with one layout per USD value type:
| USD Type | Widget | Min Width |
|----------|--------|-----------|
| GfVec3f/d | 3x input fields (X,Y,Z labeled) | 240px |
| float/double | slider + value field | 200px |
| bool | toggle switch | 120px |
| SdfAssetPath | text field + browse icon | 200px |
| GfVec3f (color) | color swatch + value | 200px |
| TfToken (enum) | dropdown | 160px |
| string | text field | 200px |

**3.3 -- Layer Stack Items**

*Assembly Detail:* The Layer Stack shows three items ("lighting.usd", "layout.usd", "shot_010.usd") with checkbox icons (check = active), eye icons (visibility), and lock icons. The active layer "lighting.usd" has a checkbox and appears highlighted. The 3px left accent stripe is visible in teal on the active layer.

*Assembly Qt Implementation:* Layer stack shows three items ("shot_010_v2.usd", "layout_base.v2.usd", "anim_reference.usd") with similar formatting but the accent stripe colors differ (teal for top, blue for second, gray for third). The highlight treatment looks slightly different -- the active layer has a more prominent background highlight.

*Issue:* The accent stripe colors are inconsistent. In the detail mockup, only the active layer has a teal stripe. In the Qt implementation, each layer has its own stripe color. The design system (DESIGN.md Section 5, "The Stripe Rule") specifies "a 3px vertical accent stripe on the far left of the list item using the USD Layer Colors (Teal, Purple, etc.)."

*Recommendation:* Each layer should always show its assigned color stripe, regardless of active state. The active/edit-target layer gets an additional background highlight (`surface_container_high` #2a2a2a or a subtle tint of the accent blue). This matches the Photoshop layer panel model where every layer has its own color tag but the active layer has a distinct highlight.

**3.4 -- Scene Tree Items**

*Assembly Detail:* Tree items show disclosure triangles, prim type icons, and labels. Selected item ("hero_char") has a blue highlight background. Child items are indented with what appears to be 16-20px per level.

*Assembly Qt Implementation:* Similar structure but the tree appears more compact. The prim type icons are slightly different in style.

*Materials workspace:* The tree shows a different structure -- "Project Alpha" as root, with "Materials", "Geometry", "Lights" as top-level groups. Material items ("hero_wet", "base_concrete", "glass_clear") have colored dots (the layer color indicators from UI_DESIGN.md).

*Issue:* The indentation amount and icon style should be identical across workspaces. The Materials workspace tree looks like it uses different icons (folder-style for groups vs. the Assembly workspace's USD prim-type icons).

*Recommendation:* Standardize tree item component:
- Indentation: 16px per level (DESIGN.md says 0.4rem = 6.4px for sublayers, but for the main scene tree, 16px is standard for readability)
- Icon set: USD prim-type icons (Mesh, Xform, Scope, Material, Light) must be the same glyphs across all workspaces
- Row height: 24px (matches `--min-click-target: 24px` from DCC_UI_RESEARCH.md)
- Selected state: `--bg-selected` (#1a3a5c) background, `--text-active` (#ffffff) text
- Layer dot: 6px circle, positioned 4px left of the prim icon, using the layer's assigned color

---

## 4. Information Density

### Comparison to Industry DCCs

**4.1 -- Assembly Workspace Density**

The Assembly Detail mockup shows approximately 5 visible tree items, 3 layer stack items, 3 transform property groups (each with 3 fields), and 3 metadata properties. This is lower density than Clarisse (which typically shows 15-20 tree items and 10+ properties simultaneously) but higher than Blender's default layout.

*Assessment:* Slightly too sparse for VFX professionals. Scene assembly artists working with stages containing 50+ root prims need to see more of the tree without scrolling. The current tree shows only 5 items in what appears to be approximately 300px of vertical space.

*Recommendation:* Reduce tree item row height from what appears to be approximately 28px to 22px. This gains roughly 2-3 more visible items in the same space. Keep the 24px minimum for interactive elements by ensuring the clickable area extends to the full row width even if the visual height is 22px. This is a common pattern in Houdini and Nuke where tree items are 20-22px tall for density but the click target extends to 24px via padding.

**4.2 -- Lighting Workspace Density**

The Lighting workspace correctly prioritizes viewport space and reduces panel density. The property inspector shows approximately 6 properties (Active Light, Type, Status, Intensity, Exposure, Color Temperature, Shadows toggle, Samples). This is appropriate -- a lighting artist selected on a single light needs exactly these controls.

*Assessment:* Good density for lighting workflow. The render snapshot gallery at the bottom (showing "Render A" and "Render B" thumbnails) is an excellent addition that matches Katana's render catalog concept from the DCC research.

*Issue:* The render snapshot thumbnails appear quite small (approximately 120x80px). At this size, it is difficult to compare lighting differences between iterations.

*Recommendation:* Allow the snapshot gallery to expand vertically on hover or click, showing thumbnails at 240x160px. Add an A/B comparison mode where clicking two snapshots shows them side-by-side in the viewport (a feature identified in DCC_UI_RESEARCH.md Section 1.4 as a Katana strength).

**4.3 -- Materials Workspace Density**

The Materials workspace shows the scene tree, a lookdev viewport with a character model, a material preview sphere (floating in the viewport), and the Material Parameter Sheet on the right. The parameter sheet shows OpenPBR parameters grouped under "BASE" (Color, Metallic, Roughness) and "SPECULAR" (IOR, Weight) sections with "COAT" and "EMISSION" collapsed.

*Assessment:* Good progressive disclosure. Showing only BASE expanded by default with other groups collapsed is correct -- most material edits start with base color and roughness. The collapsible sections follow the three-tier disclosure pattern from DCC_UI_RESEARCH.md Section 2.3.

*Issue:* The material preview sphere is floating inside the viewport, which creates visual competition with the character model. When both are visible, the eye jumps between two 3D renders.

*Recommendation:* Move the material preview sphere to the right panel, positioned above the Material Parameter Sheet as a dedicated preview thumbnail (approximately 200x200px). This matches Blender's material preview placement and keeps the viewport focused on the scene context. The floating sphere approach works in Substance Painter where the preview IS the viewport, but in BIF where the viewport shows scene context, it creates dual-attention conflict.

**4.4 -- Node Graph Density**

Across all workspaces, the node graph shows 3-4 nodes with connections. This is fine for the mockups but the real concern is scalability. A typical BIF assembly scene might have 15-30 nodes.

*Recommendation:* Include at least one mockup showing a more complex graph (15+ nodes) to validate:
- Node readability at zoom-out levels
- Connection routing when many wires cross
- Whether the bottom dock height (30-40% of screen) is sufficient
- LOD behavior (at what zoom level do port labels disappear, then node labels, then node bodies become colored dots)

---

## 5. Implementation Feasibility (Qt C++)

### Potential Challenges

**5.1 -- Glassmorphism / Backdrop Blur**

The DESIGN.md specifies `backdrop-filter: blur(20px)` for floating HUDs. In Qt, there is no direct equivalent of CSS `backdrop-filter`. Achieving this requires:
- Rendering the viewport to an offscreen buffer
- Applying a Gaussian blur to the relevant region
- Compositing the HUD widget over the blurred region

This is expensive during active rendering and may cause frame drops in the viewport.

*Recommendation:* Implement the blur as a progressive enhancement:
1. Default: solid `surface_variant` (#353535) at 85% opacity (no blur, fast)
2. When viewport is idle (no active render, no orbit): apply 8px blur (reduced from 20px for performance)
3. Never blur during viewport interaction or active Ivar rendering

**5.2 -- Shadow Gaps Between Panels**

The "No-Line Rule" uses 1-2px shadow gaps (`surface_container_lowest` #0e0e0e) between panels. In Qt, this is achieved by setting `QSplitter::setHandleWidth(2)` and styling the splitter handle with the gap color. This is straightforward.

*Implementation:* Set splitter handle background to `#0e0e0e` with 2px width. No special complexity.

**5.3 -- Color-Gradient Slider Tracks**

The Lighting workspace shows a blue-to-orange gradient on the Color Temperature slider. In Qt, `QSlider` groove styling supports gradients via stylesheets:
```css
QSlider::groove:horizontal {
    background: qlineargradient(x1:0, y1:0, x2:1, y2:0,
        stop:0 #4a9eff, stop:1 #e8a838);
}
```
This is straightforward and well-supported.

**5.4 -- Node Graph Rendering**

The mockups show a node graph with colored nodes, connection wires, and potentially animated elements. Qt's `QGraphicsScene` / `QGraphicsView` framework is appropriate for this. However, at 30+ nodes with real-time connection wire rendering, performance matters.

*Recommendation:* Use `QGraphicsView` with:
- `ViewportUpdateMode::SmartViewportUpdate` (only repaint changed regions)
- LOD rendering via `QGraphicsItem::levelOfDetailFromTransform()`
- Cubic bezier connection wires drawn in `QPainter` with anti-aliasing
- Node bodies as `QGraphicsWidget` subclasses for proper layout

This is a well-trodden path. The egui-snarl node graph can inform the data model but the Qt rendering will be completely new. Budget 2-3 weeks of dedicated work for a production-quality node graph widget.

**5.5 -- Workspace Tab Switching with Panel Reconfiguration**

Switching workspaces needs to reconfigure all panel sizes, visibility, and content. In Qt, this maps to:
- Save current workspace's `QSplitter` sizes and panel visibility
- Apply the target workspace's saved sizes
- Animate the transition (200ms per DESIGN.md)

*Implementation concern:* Animating `QSplitter` resize requires a `QPropertyAnimation` on custom properties since `QSplitter::setSizes()` is not animatable directly. The common workaround is a `QTimer`-based interpolation that calls `setSizes()` on each frame. This works but needs careful implementation to avoid visual jank.

*Recommendation:* Start with instant switching (no animation). Add the 200ms animation as a polish pass. The workspace switching functionality is more important than the transition smoothness.

**5.6 -- Monospace Font Alignment in Property Rows**

The DESIGN.md requires JetBrains Mono for all numerical inputs. Qt's `QFont` system handles this well, but ensure:
- The font is bundled with BIF (not all systems have JetBrains Mono installed)
- Fall back to Cascadia Code -> Consolas -> system monospace
- Test alignment of decimal points across multiple property rows at different DPI scales

---

## 6. Opinion Visualization

### Evaluation of the Layer Attribution System

The mockups implement opinion visualization through colored left-border stripes on property rows in the inspector. Based on the Assembly Detail mockup:
- Teal left-border = property set on active edit layer (the artist's own changes)
- Blue left-border = property from a sublayer
- Gray left-border = default/fallback value
- The "Active Layer" label in the inspector header links to which layer is the edit target

**6.1 -- Strengths**

The 3px left-border stripe approach is elegant and space-efficient. It does not consume horizontal space that would be needed for property labels and values. The color immediately communicates "who set this" without requiring interaction. This is a direct implementation of the "Stripe Rule" from DESIGN.md Section 5.

The choice of teal for the active layer is good -- it is the most visually prominent of the layer colors and creates a clear "these are YOUR changes" signal. This matches the UI_DESIGN.md layer color table where Layout (the typical first/base layer) gets teal.

**6.2 -- Concerns**

**Color Distinguishability:** Teal (#4ecdc4) and blue (#4a9eff) are only about 30 degrees apart on the hue wheel. Under the dim lighting conditions where many VFX artists work, these could be difficult to distinguish, especially for the approximately 8% of males with color vision deficiency. The gray (#888888) default is easily distinguishable from both, but the teal-blue distinction is the critical one (active layer vs. sublayer) and it is the weakest.

*Recommendation:* Increase hue separation. Options:
- Option A: Change the active layer indicator from teal to a brighter cyan (#00e5ff) -- more green, further from blue
- Option B: Change the sublayer indicator from blue to purple (#9b59b6) -- matches the "Animation" layer color and is far from teal
- Option C: Add a secondary signal: active-layer properties get a bold font weight in addition to the teal stripe, while sublayer properties use regular weight. This redundant encoding (color + weight) survives color vision deficiency.

*Strong recommendation:* Implement Option C regardless of color choice. Redundant encoding is an accessibility best practice and is specifically called out in the DCC_UI_RESEARCH.md Section 2.2 where Katana uses "white=set here" (bold implied) vs "gray=inherited" (regular weight).

**6.3 -- Missing States**

The mockups do not show two important opinion states:
1. **Overridden:** A property where the active layer's value is being overridden by a stronger layer. Per UI_DESIGN.md, this should be amber/orange text. This is the "someone else's layer wins" state and is critical for debugging composition issues.
2. **Muted:** A property from a muted (eye-icon-toggled-off) layer. Per UI_DESIGN.md, this should be strikethrough gray text.

*Recommendation:* Create additional mockups showing:
- A Transform property with an amber left-border and amber value text, indicating the active layer set this value but a stronger layer overrides it
- A property with strikethrough text from a muted layer
- The expanded opinion stack view (described in UI_DESIGN.md Section 2B) showing multiple layer contributions per property

**6.4 -- Opinion Stack Expansion**

The mockups show only the collapsed state of property rows. The UI_DESIGN.md describes an expandable opinion stack per property:
```
Transform                    [gold dot] (12, 0.5, 3)  <- WINNING
  |- lighting.usd   [teal]  (12, 0.5, 3)    [strongest]
  |- layout.usd     [blue]  (10, 0, 3)      [weaker]
  |- (default)               (0, 0, 0)       [fallback]
```

This is BIF's most innovative UX feature and it is not shown in any mockup.

*Recommendation:* Create a dedicated mockup showing the opinion stack expanded for at least one property. Show:
- The expand/collapse affordance (a disclosure triangle or "2 opinions" clickable label)
- The stacked layer values with their respective color stripes
- Visual indication of which value "wins" (bold, checkmark, or "WINNING" badge)
- How the expanded state affects layout below (does it push other properties down, or does it use an overlay/popover?)

Preference: push content down, not a popover. Popovers obscure other properties the artist may be comparing.

---

## 7. Accessibility

### Contrast Ratio Analysis

**7.1 -- Text Contrast**

Evaluating against the DCC_UI_RESEARCH.md Appendix contrast specifications:

| Element | Observed Approx. Color | Background | Estimated Ratio | WCAG AA (4.5:1) |
|---------|----------------------|------------|----------------|-----------------|
| Panel section headers (uppercase) | ~#808080 | ~#20201f | ~4.5:1 | Borderline PASS |
| Property labels | ~#d4d4d4 | ~#20201f | ~10.5:1 | PASS |
| Property values (monospace) | ~#e5e2e1 | ~#0e0e0e | ~14:1 | PASS |
| Workspace tab labels (inactive) | ~#808080 | ~#131313 | ~4.5:1 | Borderline PASS |
| Workspace tab labels (active) | ~#ffffff | ~#131313 | ~16:1 | PASS |
| Status bar text | ~#707070 | ~#131313 | ~3.5:1 | FAIL |
| Layer stack item (inactive) | ~#707070 | ~#20201f | ~3.5:1 | FAIL |

*Issues found:*
- The viewport status bar text ("FPS: 24.30 | GPU: 42*C | Mem: 6.4GB" in the Lighting workspace) appears to use a low-contrast color that would fail WCAG AA.
- Inactive layer stack items appear too dim. While the design intent is to de-emphasize non-active layers, the text should still meet AA contrast (4.5:1 minimum).

*Recommendation:*
- Status bar text: use `--text-secondary` (#888888) minimum, which achieves 4.5:1 against #131313
- Inactive layer items: use `--text-secondary` (#888888) not #707070. The active/inactive distinction can be reinforced through font weight (bold for active) rather than pushing the inactive text below AA contrast.

**7.2 -- Interactive Target Sizes**

The DCC_UI_RESEARCH.md specifies a minimum 24px click target height.

*Potential violations:*
- The eye and lock icons in the Layer Stack appear to be approximately 16x16px icon size. Even if the click target extends beyond the icon to 24px via padding, the visual target is small for frequent use. Lighting artists toggle layer visibility frequently.
- The viewport HUD controls (play, camera, grid buttons at bottom of viewport in Lighting workspace) appear to be approximately 20x20px.
- The bottom dock tab labels ("NODE GRAPH", "USDA PREVIEW", "CONSOLE") appear compact.

*Recommendation:*
- Layer stack eye/lock icons: 16px icon within a 28px row height, with the entire row acting as a click target for selection and dedicated icon hit areas of 24x24px
- Viewport HUD buttons: minimum 32x32px (these are used during creative flow and should be easy to hit without precision)
- Bottom dock tabs: 28px minimum height with 8px horizontal padding on labels

**7.3 -- Keyboard Navigation**

None of the mockups show focus states for keyboard navigation. The DCC_UI_RESEARCH.md Section 2.7 specifies: "All interactive elements need a visible focus ring (2px, accent color, 50% opacity)."

*Recommendation:* Design and document the focus ring for:
- Tree items (scene tree, layer stack)
- Property input fields
- Buttons and toggles
- Node graph nodes
- Workspace tabs
- Bottom dock tabs

The focus ring should be 2px `primary_container` (#4a9eff) at 50% opacity, offset 2px from the element edge. In Qt, implement via `QStyle::drawPrimitive(PE_FrameFocusRect)` override in the custom style.

**7.4 -- Font Size**

The smallest text visible in the mockups appears to be approximately 10px (status bar, metadata). The DCC_UI_RESEARCH.md allows 10px for "status/metadata" at weight 300 with reduced opacity. This is at the absolute minimum for legibility.

*Recommendation:* Support a global UI scale factor (100%, 125%, 150%) applied via `QApplication::setFont` scaling. This is a mandatory feature for artists on 4K displays at 100% OS scaling (where 10px text becomes genuinely unreadable) and for accessibility compliance at studios with inclusive tool requirements.

---

## 8. Workspace-Specific Findings

### 8.1 -- Assembly Workspace Detail

**Strengths:**
- The breadcrumb bar ("shot_010.usd > layout.usd > /world/hero_char") at the top of the viewport is excellent. Matches the UI_DESIGN.md spec exactly. Each segment being clickable for navigation is a key usability feature.
- The gizmo/axis indicator in the viewport (visible in the lower-left area of the 3D view) provides spatial orientation.
- The "Active Layer" indicator in the Inspector header creates an immediate link between property values and their layer context.

**Issues:**
- The breadcrumb bar text appears quite small and low-contrast against the viewport header. It should be the most readable text in the viewport since it answers "where am I?" -- the single most important question during scene assembly.
- The node graph shows node category tabs ("Assembly", "Materials", "Output") in the bottom-left. This filtering concept is not described in the design documents and needs specification.

### 8.2 -- Assembly Workspace Qt Implementation

**Strengths:**
- The "USD_PRIM - REAL_GEOM" label in the viewport center confirms the selected prim type, which is useful for confirming payload loading state.
- The rendering mode info at the bottom ("TRI: 65.4k | Prims: 1.0k | Samples: 256") provides essential performance context.

**Issues:**
- The overall contrast is noticeably lower than the detail mockup. The left panel is very dark, almost indistinguishable from the viewport background. The surface tier separation is insufficient.
- The node graph nodes appear larger and more spaced out than in the detail mockup, suggesting the mockups were created at different scales or by different people. Normalize the component sizes.

### 8.3 -- Lighting Workspace Qt Implementation

**Strengths:**
- The viewport is maximized, correctly reflecting the lighting artist's priority.
- The render snapshot gallery at the bottom is a strong feature.
- The "PERSPECTIVE | PATH TRACED (1024 SAMPLES)" header badge clearly communicates rendering state.
- The "A/B ACTIVE" toggle on the render snapshot is an excellent touch -- directly enables comparison.
- Property sliders have colored tracks (blue-orange for Color Temperature) that communicate value range semantically.

**Issues:**
- The left dock (scene tree) is completely absent. While the design intent is to maximize viewport, lighting artists still need to select lights from a list. The mockup shows no scene tree -- only the inspector on the right.
- The light list should be accessible via a collapsible left panel or a dropdown/search in the inspector header. "ACTIVE LIGHT: key_light_01" with a dropdown to switch lights would work for the minimized-panel approach.
- The "TIDAL ATLAS | LEGACY SOURCE" tabs at the bottom of the inspector are not explained. If these are render AOV or light group selectors, they need clearer labeling.

*Recommendation:* Add a minimal light list to the Lighting workspace. Options:
- A collapsible left panel (default collapsed) with just lights listed
- A dropdown in the inspector header to switch between lights
- A floating light list popover triggered by a toolbar button

### 8.4 -- Materials Workspace

**Strengths:**
- The "SHADING MODEL: OpenPBR" dropdown confirms the material system. This matches BIF's OpenPBR commitment from the project memory.
- The collapsible parameter sections (BASE expanded, SPECULAR expanded, COAT and EMISSION collapsed) demonstrate progressive disclosure correctly.
- The "UPDATE SHADER" button (blue, prominent, bottom of inspector) follows the DESIGN.md primary button spec (#a4c9ff background).
- The node graph shows a clear UsdUVTexture -> OpenPBR Surface -> MaterialOut flow that is easy to follow.

**Issues:**
- The MaterialOut node shows what appears to be a texture thumbnail (a thumbs-up emoji image) which looks like placeholder content. In production, this should show a small preview render of the material output.
- The "USE NORM..." label on one of the node ports is truncated. Node port labels need to either fully display or have a tooltip on hover. Truncated labels in a material graph are dangerous -- "USE NORMAL" vs "USE NORMALMAP" vs "USE NORM_SCALE" are very different connections.
- The floating material preview sphere in the viewport overlaps with the character model. As noted in Section 4.3, this should move to the inspector panel.

---

## 9. Summary of Recommendations

### Critical (Must Fix Before Implementation)

| # | Issue | Section | Effort |
|---|-------|---------|--------|
| C1 | Increase teal/blue hue separation for opinion colors OR add redundant bold/regular weight encoding | 6.2 | Low |
| C2 | Fix status bar and inactive layer text contrast to meet WCAG AA (4.5:1) | 7.1 | Low |
| C3 | Add opinion stack expanded state mockup (BIF's key differentiator) | 6.4 | Medium |
| C4 | Normalize node component dimensions across all workspaces | 3.1 | Medium |
| C5 | Add light selection mechanism to Lighting workspace (collapsed panel or dropdown) | 8.3 | Medium |

### Important (Fix During Implementation)

| # | Issue | Section | Effort |
|---|-------|---------|--------|
| I1 | Remove all 1px borders from input fields and section dividers per No-Line Rule | 2.1 | Low |
| I2 | Increase workspace tab font to headline-sm (1.5rem Manrope) for hierarchy clarity | 2.3 | Low |
| I3 | Set minimum panel widths (left: 220px, right: 280px) | 1.1, 1.2 | Low |
| I4 | Move material preview sphere from viewport to inspector panel | 4.3, 8.4 | Low |
| I5 | Design and document keyboard focus ring states | 7.3 | Medium |
| I6 | Implement global UI scale factor (100%/125%/150%) | 7.4 | Medium |
| I7 | Implement glassmorphism as progressive enhancement (solid fallback default) | 5.1 | Medium |

### Nice to Have (Polish Pass)

| # | Issue | Section | Effort |
|---|-------|---------|--------|
| N1 | Animated workspace transitions (200ms) | 5.5 | Medium |
| N2 | Expandable render snapshot gallery with A/B comparison | 4.2 | High |
| N3 | Create 15+ node graph mockup for scalability validation | 4.4 | Low |
| N4 | Reduce tree item height to 22px for density | 4.1 | Low |
| N5 | Node graph LOD behavior spec (zoom levels -> port labels -> node labels -> dots) | 4.4 | Medium |

---

## 10. Design System Gap Analysis

Items specified in DESIGN.md or UI_DESIGN.md but not shown in any mockup:

1. **Command Palette (Ctrl+P)** -- No mockup shows the search overlay. This is a Tier 1 feature for v0.15.0 and needs its own mockup showing the translucent overlay with search results.
2. **Overridden property state** (amber text/border) -- Only teal and blue opinion states shown.
3. **Muted layer property state** (strikethrough) -- Not shown.
4. **Viewport wireframe overlay** with layer colors -- UI_DESIGN.md mentions "Optional colored wireframe overlay showing layer ownership." No mockup shows this.
5. **Context menu** -- No right-click context menu mockup exists. These are critical for discoverability alongside the command palette.
6. **Error/warning states** -- No mockup shows an error condition (failed USD load, missing reference, broken material connection).
7. **Tooltip design** -- The DESIGN.md specifies `surface_container_highest` (#353535) but no mockup shows a tooltip.
8. **Zen mode** -- UI_DESIGN.md specifies `Ctrl+\` toggles all docks. No mockup shows viewport-only mode.
9. **USDA Code Preview tab content** -- The tab exists in all mockups but no mockup shows the actual syntax-highlighted code view.
10. **Review workspace** -- Only Assembly, Lighting, and Materials workspaces have mockups. The Review workspace (viewport maximized, minimal UI) is missing.

*Recommendation:* Prioritize mockups for items 1 (Command Palette), 3 (opinion states), and 9 (USDA Code Preview) before starting Qt implementation. These are core differentiators.

---

*Review completed 2026-03-31. Ready for discussion and iteration before Qt implementation begins.*
