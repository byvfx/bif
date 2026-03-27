# current bugs as of 2026-03-27

## Performance & Optimization

### Metrics & Profiling

- Implement performance metrics tracking (time, memory, throughput)
- Compare bif vs usdview on reference scenes (OpenUSD docs + custom assets)
- Index [OpenUSD v25 perf guide](https://openusd.org/release/ref_performance_metrics.html), audit codebase compliance
- Plan upgrade to USD v26 post-stabilization

### Selective Prim Loading

- Research Katana/Houdini prim-load UI patterns
- Implement selective USD load (avoid full-stage load on large scenes)
- Prioritize: artists need simple UI, optional deep control

## Features

### Tools & UI

- remove window that loads usd files.
- USD Stage Inspector (prim hierarchy, attributes, metadata) <-- look at usdview's implementation for reference and katana's USD tools
- USD Stage Outliner (scene graph view with search/filter)
- Paint setup for  instancing

### Materials

- Sub-surface scattering (skin, organic materials)
- Multi-layer/complex BRDF support
- Test with self-authored assets

### Viewport & Rendering

- Subdivision surface validation (re-test custom assets)
- Camera safe-area greybox overlay

## Architecture

- Double-check USD schema compliance (custom vs standard)

### Scene Assembly (Novel Approach)

- **Goal:** Simple top-layer for artists, deep layer optional for power users
- **Brainstorm needed:** How to make USD workflow intuitive without complexity
- **Open questions:** UI paradigm, prim workflow, export/save patterns
