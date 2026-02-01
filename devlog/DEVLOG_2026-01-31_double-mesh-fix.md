# Development Log - January 31, 2026

## Session Duration
~1.5 hours

## Goals
- Fix double mesh instances in USD loading / Ivar rendering

## What I Did

### Problem Investigation
User reported meshes appearing doubled when rendering with Ivar. Viewport (wgpu) looked correct but ray tracer showed duplicates.

### Root Cause
When loading USD files with multiple prototypes, the viewport uses a **combined mesh** approach for Ivar where instance transforms are baked into vertex positions. However, Embree scene builder was also applying the original instance transforms, causing geometry to be double-transformed.

**Flow before fix:**
1. `load_usd_file()` creates `mesh_data` via `combine_with_transforms()` (transforms baked in)
2. `instance_transforms` populated with original transforms
3. `build_ivar_scene()` passes `instance_transforms` to Embree
4. Embree applies transforms again → double transform!

### Changes Made

**C++ USD Bridge** (`cpp/usd_bridge/usd_bridge.cpp`):
- Added mesh path deduplication using `std::set<std::string>`
- Prevents same mesh from being cached twice if USD traverse visits it multiple times via different composition arcs

**Viewport** (`crates/bif_viewport/src/lib.rs`):
- `build_ivar_scene()`: When `use_multi_draw` is true, use single identity transform for Embree since transforms are baked into combined mesh
- `build_ivar_scene_sync()`: Same fix for synchronous batch rendering path

### Verification
- Log output: `Embree scene created: 1 instances` instead of `2 instances`
- Scene bounds correct after fix
- All 113 tests pass
- Clippy passes

## Learnings
- Multi-prototype scenes have transforms baked into combined mesh for Ivar
- Embree's instancing is separate from viewport GPU instancing
- Need to coordinate transform application between mesh combining and ray tracer

## Next Session
- Continue M19 geometry animation per frame
- Test with more complex multi-prototype scenes
