# FEATURES

Last updated: 2026-03-31

## Rendering

- Add lights to the viewport.
- Add OpenSubdiv support with GPU-accelerated subdivision.
- Add camera safe-area overlay.
- Add viewport display modes: textured, display color, and unlit.
- Add IBL disk caching and reload to avoid recomputing each session.
- Add sub-surface scattering support (skin and organic materials).
- Add multi-layer BRDF support.
- Add renderpass support for AOVs and custom outputs, using the nodes and whatever comes with USD.
  
## USD

- Build a USD Stage Inspector (prim hierarchy, attributes, metadata).
- Build a USD Stage Outliner (scene graph view with search and filtering).
- Add USD skeletal animation support (joints, skinning, blendshapes).
- Add compound prototype support for multi-mesh Xforms.
- When editing materials, have some way for the user to preview the models it's attached to, like a drop-down, and then it would load it in the viewport. Then the user can go back to the shot camera.
  
## Tools

- Add paint setup workflow for instancing.
- Add a physics-based painter tool.
- Add Multi shot workflow for layout and animation blocking.

## UX

- Remove the extra USD file loading window.
