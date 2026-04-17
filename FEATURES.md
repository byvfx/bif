# FEATURES

Last updated: 2026-04-16

## Rendering

- Add lights to the node graph and viewport, with support for USD light types (point, directional, spot, area).
- Add OpenSubdiv support with GPU-accelerated subdivision.
- Add camera safe-area overlay.
- Viewport display modes (Katana/Houdini-style):
  - Shaded (textured with lighting — default)
  - Wireframe (edges only, no fill)
  - Wireframe on Shaded (shaded + edge overlay)
  - Flat Shaded (per-face normals, no textures)
  - Smooth Shaded (smooth normals, no textures)
  - Display Color (`primvars:displayColor` with lighting)
  - Unlit / Constant (albedo only, no lighting)
  - Points (vertex dots, good for dense scatter previews)
  - Hidden Line (wireframe with hidden-edge removal)
  - Bounding Box (AABB per prim, fastest for huge scenes)
- Purpose display filtering (USD render/proxy/guide):
  - Toggle visibility per purpose: Render, Proxy, Guide
  - Default: Render + Proxy visible, Guide hidden
  - Quick-switch toolbar buttons (like Katana's purpose toggles)
  - When Proxy visible + Render hidden, show proxy geo in place of full-res (USD purpose swap pattern)
  - Guide geometry drawn with dashed/stippled wireframe to distinguish from scene content
- Add IBL disk caching and reload to avoid recomputing each session keep file in the same area as the usd, and lets have a housekeeping mechanism that deletes old ones after a certain amount of time or disk usage.
- Add sub-surface scattering support (skin and organic materials).
- Add multi-layer BRDF support.
- Add renderpass support for AOVs and custom outputs, using the nodes and whatever comes with USD.
  
## USD

- Add USD skeletal animation support (joints, skinning, blendshapes).
- Add compound prototype support for multi-mesh Xforms.
- When editing materials, have some way for the user to preview the models it's attached to, like a drop-down, and then it would load it in the viewport. Then the user can go back to the shot camera.
  
## Tools

- Add paint setup workflow for instancing.
- Add a physics-based painter tool.
- Add Multi shot workflow for layout and animation blocking.
- Add some tools that will track app usage with the UI to help us understand how artists are using the tool and where they are getting stuck, and then we can use that data to improve the UX.

## UX

- Remove the extra USD file loading window.
- NEED a way to see the USD primvars and values.
