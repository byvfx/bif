# Development Log - January 31, 2026

## Session: M19.1 Ivar Vertex Animation in Batch Render

## Session Duration

~1.5 hours

## Goals

- Implement per-frame BVH rebuild for vertex-animated geometry in batch render
- Static scenes should skip rebuild for performance

## What I Did

### Phase 1: Extract Triangle Builder

- Extracted triangle building logic from `build_ivar_scene_sync()` into `build_triangles_at_time()`
- New function queries USD for animated vertices at a specific time
- Falls back to static `mesh_data` vertices when no animation

### Phase 2-3: Per-Frame Scene Rebuild

- Created `SceneBuilderData` struct in `batch_render.rs` with all data needed to rebuild scene
- `SceneBuilderData::build_scene_at_time()` builds complete Embree scene at given time
- Added `has_animated_geometry` flag and `scene_builder` callback to `BatchSceneData`
- `batch_render_loop` now conditionally rebuilds Embree scene each frame

### Phase 4: Memory Management

- Added debug logging to `EmbreeScene::drop()` to track resource cleanup
- Logs instance/triangle counts when scene is released
- Helps verify no memory leaks during long batch renders

### Key Decisions

- Used closure + `SceneBuilderData` rather than passing full ViewportState to batch thread
- Clone mesh/material data into builder data (acceptable memory cost for animation support)
- Skip first frame rebuild since initial scene is already built

## Files Modified

- `crates/bif_viewport/src/lib.rs` - `build_triangles_at_time()`, SceneBuilderData creation
- `crates/bif_viewport/src/batch_render.rs` - `SceneBuilderData`, `TriangleData` type, rebuild logic
- `crates/bif_renderer/src/embree.rs` - Drop logging

## Learnings

- Embree device can be reused across scene rebuilds but we recreate for simplicity
- Rust closure captures require cloning data when passing to threads
- `TriangleData` type alias avoids clippy type_complexity warning

## Test Plan

1. Load `assets/palm_coconut_field_2/palm_coconut_field_2.usd` (vertex animated)
2. Batch render frames 1-10
3. Check logs for "Rebuilding BVH for frame N" messages
4. EXR sequence should show palm animation
5. Memory usage should stay constant

## Next Session

- M19.2: Instance transform animation per frame
- Evaluate instance matrices at each frame time
- Potentially cheaper than full BVH rebuild for transform-only animation

## Wiki Links

- [[bvh|BVH]] — BVH rebuild considerations for vertex animation
- [[geometry-schemas|Geometry Schemas]] — vertex displacement animation in Ivar renderer
