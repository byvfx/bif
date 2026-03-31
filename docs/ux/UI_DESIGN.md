# BIF UI/UX Design Brainstorm

## Context

BIF's UI needs to make USD **visually understandable at a glance**. USD is already complex — layers, composition arcs, opinions, payloads — and no existing tool makes this intuitive. BIF's differentiator isn't just being a USD editor, it's being the tool where you *finally get* what USD is doing. Built for personal use, potentially open source. Sleek, modern, uncluttered — but power-user functional.

Current state: egui with 5 panels (left scene browser, right property inspector, top menu, bottom timeline, center viewport). Qt migration planned for v0.15.0.

---

## Design Philosophy: "Quiet Confidence"

**Reference points:** Resolve's dark professionalism, Figma's clean panels, Blender 4.x's simplified toolbars, VS Code's command palette. NOT Houdini's parameter sprawl or Maya's toolbar overload.

**Core principle:** Show the *meaning* of USD, not the *mechanism*. Artists should understand composition through visual metaphors (colors, spatial relationships, icons) without reading USDA text.

---

## 1. Layout: Viewport-Dominant T-Layout

The workflow doc's three-panel-above-viewport is too cramped. Replace with **viewport-dominant dock layout**:

```
┌────────────────┬──────────────────────────────────┬───────────────┐
│                │                                  │               │
│  SCENE TREE    │         V I E W P O R T          │  PROPERTIES   │
│  + Layer Stack │                                  │  + Opinion    │
│                │   (dominant — 60%+ of screen)    │    Inspector  │
│  [tree view]   │                                  │               │
│  [layers tab]  │                                  │  [context-    │
│                │                                  │   sensitive]  │
├────────────────┴──────────────────────────────────┴───────────────┤
│  NODE GRAPH  |  USDA CODE PREVIEW  |  RENDER LOG     [tabbed]    │
└──────────────────────────────────────────────────────────────────┘
```

**Why this beats the spec's layout:**
- Viewport owns the center — this is a visual tool, not a code editor
- Left/right docks are narrow (250-300px) — just enough for tree + properties
- Bottom dock is **tabbed** — node graph, USDA preview, and render log share space. You rarely need all three simultaneously
- Matches Clarisse, Katana, Blender, Nuke — artists already know this pattern

**Panel behaviors:**
- All docks collapsible with single click (thin grab bar, not a button)
- Double-click dock edge → auto-fit to content width
- `Tab` key cycles bottom dock tabs
- `Ctrl+\` toggles all docks (zen mode — viewport only)
- Panels remember size per-session

---

## 2. Making USD Visually Understandable

This is the core innovation. Three systems working together:

### A. Layer Color Coding (the "paint" metaphor)

Every layer gets an auto-assigned color from an 8-color palette:

| Layer | Color | Hex |
|-------|-------|-----|
| Layout | Teal | `#4ecdc4` |
| Animation | Purple | `#9b59b6` |
| FX | Orange | `#e67e22` |
| Lighting | Gold | `#f1c40f` |
| Materials | Pink | `#e84393` |
| Custom 1-3 | Blue/Green/Red | varies |

These colors appear **everywhere** consistently:
- **Scene tree**: Tiny colored dot next to each prim showing which layer has the strongest opinion
- **Property inspector**: Colored left-border on each property row showing which layer set it
- **Viewport**: Optional colored wireframe overlay showing layer ownership
- **Node graph**: Node header color matches its target layer
- **USDA preview**: Syntax coloring by layer origin (not just keyword highlighting)

**The "aha" moment:** An artist looks at the property inspector and instantly sees "ah, the transform is gold (lighting) but the material is pink (materials dept) and visibility is teal (layout)." No need to understand `GetPrimStack()` — the colors tell the story.

### B. Opinion Stack Visualization (the "layer cake")

When you select a prim, the property inspector shows a **mini layer stack** per property:

```
Transform                    [gold dot] (12, 0.5, 3)
  └─ layers: lighting ■ layout ■ (2 opinions)
     click to expand ▸

Material Binding             [pink dot] /mtls/hero_wet
  └─ single opinion (materials layer)

Visibility                   [teal dot] inherited
  └─ no override — using default
```

Expanding shows the full opinion stack:
```
Transform                    [gold dot] (12, 0.5, 3)  ← WINNING
  ├─ lighting.usd   ■ (12, 0.5, 3)    [strongest]
  ├─ layout.usd     ■ (10, 0, 3)      [weaker]
  └─ (default)        (0, 0, 0)        [fallback]
```

**Key insight:** Collapsed by default (clean), expandable on demand (powerful). Most artists just need to see the winning value + which layer owns it.

### C. Node Graph: Blue/Orange + Layer Awareness

Nodes already have blue (composition) vs orange (operation) color coding from the spec. Add:

- **Layer badge**: Small colored pill on each node showing which layer it targets
- **Flow visualization**: Animated dots flowing along connections when evaluation is happening
- **Ghosted nodes**: Nodes targeting non-active layers are 40% opacity (layer isolation)
- **"What changed" mode**: Toggle to dim all nodes except those that modified the current selection

---

## 3. Progressive Disclosure (Three Tiers)

### Tier 1 — Always Visible (the "glance")
- Scene tree with prim names + type icons + layer dots
- Selected prim's key properties (transform, material, visibility)
- Active layer indicator in viewport status bar
- Render progress (thin bar, not a dialog)

### Tier 2 — One Click Away
- Full property list for selected prim
- Layer stack panel (tab in left dock)
- Node graph (bottom dock tab)
- USDA code preview (bottom dock tab)
- Material thumbnail previews

### Tier 3 — Expert Mode
- Opinion stack expansion per property
- Composition arc visualization
- Raw USD attribute metadata
- Payload memory usage
- Per-prototype instance counts

---

## 4. Navigation: Command Palette + Breadcrumbs

### Command Palette (`Ctrl+P`)
Fuzzy-search everything:
- Prim paths: `/world/hero_char/body`
- Commands: `assign material`, `toggle visibility`
- Layers: `switch to lighting.usd`
- Node types: `add scatter node`
- Settings: `render quality`

This is the #1 feature for reducing clutter — anything that would need a toolbar button or menu item is also in the palette.

### Breadcrumb Bar (top of viewport)
Shows current context:
```
shot_010.usd > lighting.usd (edit layer) > /world/hero_char (selected)
```
Each segment is clickable (switch stage, switch layer, navigate to prim).

### Workspace Presets
4 built-in layouts that reconfigure panels + payload policy:
- **Assembly**: Node graph prominent, all layers visible, LoadAll
- **Lighting**: Viewport dominant, light properties, CameraFrustum loading
- **Materials**: Material editor + lookdev viewport, material layer active
- **Review**: Viewport maximized, render settings, minimal UI

Switch via `Ctrl+1/2/3/4` or workspace tabs in top bar.

---

## 5. Visual Design Language

### Color Palette
```
Background:        #1c1c1c (not pure black — easier on eyes)
Panel background:  #252525
Panel border:      none — use 1px shadow gap (#111) between panels
Elevated surface:  #2d2d2d (floating menus, tooltips)
Primary text:      #d4d4d4 (not pure white — reduces glare)
Secondary text:    #808080
Accent (select):   #4a9eff (single blue, used sparingly)
Success:           #4ecdc4
Warning:           #f1c40f
Error:             #e74c3c
```

### Typography
- UI labels: 12px, medium weight
- Property values: 13px monospace (editable fields)
- Tree items: 13px, regular weight
- Section headers: 11px uppercase, secondary text color, letter-spaced

### Spacing & Shape
- 6px border radius on input fields, buttons, cards
- 8px padding inside panels
- 4px gap between list items
- No visible panel borders — shadow gaps only (Resolve style)
- 24px minimum click target (accessibility)

### Micro-interactions
- Panel collapse: 150ms ease-out slide
- Selection highlight: 100ms fade-in
- Layer dot color: instant (no animation — it's state, not action)
- Hover tooltips: 400ms delay (don't flash on mouse movement)

---

## 6. Specific Component Designs

### Layer Stack Widget (left dock tab)
```
┌─ Layers ──────────────────────┐
│                               │
│  ■ lighting.usd    [eye] [✎] │  ← active (highlighted)
│  ■ fx.usd          [eye] [ ] │  ← visible but locked
│  ■ animation.usd   [eye] [ ] │  ← visible but locked
│  ■ layout.usd      [eye] [ ] │  ← visible but locked
│                               │
│  Drag to reorder strength     │
│  [+ Add Layer]                │
└───────────────────────────────┘
```
- Colored squares match the layer palette
- Eye icon = mute/unmute (like Photoshop)
- Pencil icon = set as edit target
- Active layer has subtle glow/highlight
- Drag reorder changes sublayer strength (with confirmation)

### Viewport Status Bar (bottom of viewport)
```
lighting.usd (edit) │ 12,847 tris │ 1.2M instances │ Ivar: 64 spp │ ██████░░ 
```
Single line. No chrome. Just the facts.

### Material Preview Cards
In property inspector when a material is selected:
```
┌──────────────────────┐
│  ┌──────┐            │
│  │ orb  │  hero_wet  │
│  │ prev │  OpenPBR   │
│  └──────┘            │
│  Base Color  #8B4513 │
│  Roughness   0.35    │
│  Metallic    0.0     │
│  IOR         1.5     │
│  [Edit Material ▸]   │
└──────────────────────┘
```
Thumbnail orb render + key params. Click to expand full material editor.

---

## 7. BIF's Innovation Opportunities

Things no existing DCC does well that BIF can own:

1. **Layer-aware coloring everywhere** — No tool consistently color-codes layer ownership across all panels. This alone would make USD 10x more understandable.

2. **Live USDA preview** — Katana doesn't show you the USD your edits produce. BIF does. For learning USD, this is invaluable.

3. **Workspace-driven payload loading** — Switching to "Lighting" workspace automatically adjusts what's loaded in memory. No manual payload toggling.

4. **Opinion stack on hover** — Hover a property, see instantly which layers contribute and who wins. No need to open a separate inspector.

5. **Command palette for USD** — No DCC has a VS Code-style palette. For a tool with deep functionality, this eliminates toolbar clutter entirely.

---

## Decisions Made

- **Node graph**: Bottom dock tab, tabbed with USDA preview + render log. Viewport stays dominant.
- **Input style**: Hybrid — command palette (`Ctrl+P`) + right-click context menus + minimal toolbar. Everything has a hotkey AND a mouse path.
- **USD clarity**: All three systems (layer coloring, opinion stack, live USDA) are equally important and work as a unified system.

## Resolved

1. **Multi-monitor** — Yes, design for pop-out panels from the start. Qt QDockWidget supports this natively.
2. **Scene tree at 100K+ prims** — Design for virtualized tree early. Don't retrofit.
3. **USDA code panel** — Read-only in v0.15, editable in v0.16+.
4. **Layer colors** — Both: auto-assigned defaults from palette + user/studio configurable override.

## Implementation Phasing

This is a **design brainstorm**, not an implementation plan. These ideas thread across multiple releases:

| Idea | Target Release | Notes |
|------|---------------|-------|
| Layer color coding | v0.14 (egui proof-of-concept) → v0.15 (Qt full) | Core innovation — start early |
| Opinion stack | v0.14 (basic) → v0.16 (full hover) | Needs FFI for `GetPrimStack` |
| Live USDA preview | v0.14 (basic text) → v0.15 (syntax highlighted) | Already in spec |
| T-layout with docks | v0.15 (Qt migration) | Can't do proper docking in egui |
| Command palette | v0.15 (Qt) | egui text input too limited |
| Workspace presets | v0.16+ | Depends on payload policies |
| Breadcrumb bar | v0.15 (Qt) | Simple but needs proper toolbar |
| Micro-interactions | v0.15+ (Qt) | egui animation is primitive |
