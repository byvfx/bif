# FEATURES

Last updated: 2026-04-01

## Rendering

- Add lights to the node graph and viewport, with support for USD light types (point, directional, spot, area).
- Add OpenSubdiv support with GPU-accelerated subdivision.
- Add camera safe-area overlay.
- Add viewport display modes: textured, display color, and unlit.
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
