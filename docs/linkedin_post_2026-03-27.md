# LinkedIn Post — BIF 6-Week Update (Mar 27, 2026)

## Video Capture Checklist

Record ~5-10 seconds of each. Ordered by visual impact.

1. **USD Export Round-Trip** — Load USD → scatter instances → export → open in Houdini/usdview. Show UsdExport node + blue display flag.
2. **Node Graph** — Show graph with Xform, UsdPrim, GraftBranches, Cache nodes connected. Show cache bypass toggle (green/yellow indicators).
3. **Save/Load .bif Files** — File > Save As, File > Open, recent files list, unsaved changes prompt, dirty indicator in title bar.
4. **Eval Modes** — Toggle Auto/Manual/OnMouseRelease in toolbar. Show dirty node indicators. Hit Cook All.
5. **UI Overhaul** — Theme, menu bar (File/View/Render), scene browser left panel, property inspector, welcome screen, viewport stats overlay.
6. **Per-Node Scene Browsing** — Click node → browser filters to that node's prims. Show `[N]` prim count badges. Scene/Node tab.
7. **Glass/Transmission** — Glass object with refraction. Before/after.
8. **OIDN Denoising** — Noisy render → denoise → clean. Before/after.
9. **SHARC Radiance Cache** — Faster convergence with cache.
10. **OpenPBR Materials** — MaterialX material with OpenPBR params in property inspector.
11. **UDIM Textures** — UDIM-textured asset rendering across UV tiles.
12. **Curves & Points** — BasisCurves or Points in viewport.
13. **Implicit Geometry** — UsdGeomSphere / UsdGeomCube tessellation.
14. **DomeLight + HDRI** — Auto-HDRI, live rotation slider, color temperature.
15. **Performance** (text overlay) — 8.3x USD load, 60x texture load, 140x scene build.
16. **Purpose Filtering** — Toggle render/proxy/guide visibility.
17. **Blue Noise Sampling** — Before/after at low sample counts.
18. **Embree Subdivision** — Catmull-Clark subdivision surfaces.

---

## Post

Building a VFX scene assembler in Rust from scratch.

Here's 6 weeks of progress on BIF — a DCC tool inspired by Clarisse and Houdini, focused on USD scene assembly and path-traced rendering.

What shipped:

USD Round-Trip Pipeline
- Full USD export with sublayer composition
- Materials (OpenPBR + UsdPreviewSurface), lights, cameras, visibility
- Import curves, points, implicit geometry, subdivision surfaces
- 8 USD spec compliance sessions covering the full read/write surface

Node Graph Evolution
- 4 new node types: Xform, UsdPrim, GraftBranches, Cache
- Houdini-style evaluation modes (Auto / Manual / OnMouseRelease)
- Display flag gating for render and export
- Per-node scene graph visualization with prim count badges

Project Persistence
- Save/load .bif project files (binary + human-readable JSON)
- File menu with New/Open/Save/Recent Files
- Dirty tracking, unsaved changes prompt

Rendering Quality
- VNDF GGX sampling (2-4x convergence on rough metals)
- Glass/transmission with Snell's law refraction
- Blue noise sampling + pixel reconstruction filters
- Intel OIDN denoising integration
- SHARC radiance cache for faster convergence
- DomeLight with live HDRI rotation + color temperature

Performance
- USD loading 8.3x faster
- Texture pipeline 60x faster (125s to 2s)
- Ivar scene builds 140x faster with material cache
- UDIM per-tile loading eliminates atlas stitching

UI Overhaul
- Centralized theme system
- Menu bar, viewport stats overlay, property inspector
- Welcome screen, tooltips, node selection highlighting

Architecture
- EventBus replacing ad-hoc string messaging
- Renderer decomposed into focused subsystems
- 160+ tests, 3 code reviews, 50+ new tests this period

6 crates. ~35K lines of Rust. Still a side project at 10-20 hrs/week.

Built with Rust, wgpu, Embree, USD (C++ bridge), MaterialX, OIDN, and egui.

#rust #vfx #usd #rendering #gamedev #cgi #opensource #pathtracing
