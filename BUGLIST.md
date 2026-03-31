# current bugs as of 2026-03-27

## Performance & Optimization

### Metrics & Profiling

- Implement performance metrics tracking (time, memory, throughput)
- Compare bif vs usdview on reference scenes (OpenUSD docs + custom assets)
- Index [OpenUSD v25 perf guide](https://openusd.org/release/ref_performance_metrics.html), audit codebase compliance
- Plan upgrade to USD v26 post-stabilization
- Test on  SSD for  speeds
- Look into Rendermans denoising
- Look into how you would get ridgid body animation data into usd and then into bif for viewport display and rendering, in houdini they use RBD prodecurals

### Selective Prim Loading

- Prioritize: artists need simple UI, optional deep control

## Features

### Tools & UI

- remove window that loads usd files.
- USD Stage Inspector (prim hierarchy, attributes, metadata) <-- look at usdview's implementation for reference and katana's USD tools
- USD Stage Outliner (scene graph view with search/filter)
- Paint setup for  instancing
- Physics painter
- bug with the previous camera imported and it if you load another usd the camera stays in the list. you need to be in the camera view to duplicate it.
- saved plan for rbd intergrations under ./claude/plans
### Materials

- Sub-surface scattering (skin, organic materials)
- Multi-layer/complex BRDF support
- Test with self-authored assets
- Check to  see if proxies have materials if not use displace color

### Viewport & Rendering

- add lights
- get OCIO ACES working in the viewport
- Subdivision surface validation (re-test custom assets)
- OpenSubdiv support (GPU-accelerated subdivision surfaces)
- Camera safe-area greybox overlay
- Fix show background in hdri properties
- Get display color of prims to display in viewport as option to textured, also make one with no lighting.
- Check depth and add to ui
- store IBL on disk and load it in the viewport and that way you wouldnt need to recompute that each time

## Architecture
  VFX review summary (other suggestions noted for future):
  
- Use GetBracketingTimeSamples instead of GetTimeSamples for perf on large clips
- Extract evalTime logic to a helper function
- Consider exposing resolved evalTime to Rust for timeline scrubbing
- Compound prototype support (multi-mesh Xforms) as future work

- Double-check USD schema compliance (custom vs standard)
- Add USD skeletal animation support (joints, skinning, blendshapes)

### Scene Assembly (Novel Approach)

- **Goal:** Simple top-layer for artists, deep layer optional for power users
- **Brainstorm needed:** How to make USD workflow intuitive without complexity
- **Open questions:** UI paradigm, prim workflow, export/save patterns
