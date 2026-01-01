# Devlog: Milestone 12 - Embree 4 Integration

**Date:** 2026-01-01
**Duration:** ~8 hours (two sessions)
**Status:** ✅ Complete

## Objective

Replace instance-aware BVH with Intel Embree 4 for high-performance ray tracing with two-level BVH. Target: 10K+ instance scalability with <100ms build time.

## Implementation Summary

### Embree Setup

Installed Embree 4.4.0 via vcpkg (24-minute build including TBB):
```powershell
vcpkg install embree:x64-windows
```

Created manual FFI bindings instead of using bindgen (avoids libclang dependency, educational for learning FFI):
- `crates/bif_renderer/src/embree.rs` - ~600 LOC
- `crates/bif_renderer/build.rs` - Links embree4.lib

### Two-Level BVH Architecture

```
Top-Level Scene (TLAS)
├── Instance 0 → Prototype Scene
├── Instance 1 → Prototype Scene
├── ...
└── Instance 99 → Prototype Scene (same scene, different transform)

Prototype Scene (BLAS)
└── Triangle Mesh (280K triangles)
```

Each instance stores a 4x4 transform matrix. Embree handles ray transformation automatically.

### Key Structures

```rust
pub struct EmbreeScene<M: Material + Clone + 'static> {
    device: RTCDevice,
    scene: RTCScene,
    prototype_scene: RTCScene,  // Must stay alive for instances
    material: Arc<M>,

    // Keep buffers alive (Embree holds pointers)
    _vertex_data: Vec<f32>,
    _index_data: Vec<u32>,
    _transform_data: Vec<[f32; 16]>,

    instance_count: usize,
    triangle_count: usize,
}
```

## Debugging Journey

### Issue 1: Wrong Enum Values (Multi-hour debug)

**Symptom:** `rtcSetSharedGeometryBuffer` returned error 3 (INVALID_OPERATION)

**Root Cause:** Guessed enum values instead of reading Embree headers:
- `RTCFormat::Float3` was 12, should be 0x9003
- `RTCFormat::UInt3` was 9, should be 0x5003
- `RTCGeometryType::Triangle` was 1, should be 0
- `RTCGeometryType::Instance` was 8, should be 121

**Fix:** Read actual values from `vcpkg/installed/x64-windows/include/embree4/rtcore_common.h`:
```rust
enum RTCFormat {
    UInt3 = 0x5003,
    Float3 = 0x9003,
    Float4x4ColumnMajor = 0x9244,
}

enum RTCGeometryType {
    Triangle = 0,
    Instance = 121,
}
```

**Lesson:** Always read actual headers for C library integration. Don't guess enum values.

### Issue 2: Missing Index Buffer

**Symptom:** Error 3 after fixing enum values

**Root Cause:** Tried non-indexed triangles, but Embree triangle meshes require both vertex AND index buffers.

**Fix:** Added proper index buffer setup:
```rust
rtcSetSharedGeometryBuffer(
    geom,
    RTCBufferType::Vertex as u32,
    0, RTCFormat::Float3 as u32,
    vertex_data.as_ptr(), 0, 12, vertex_count,
);

rtcSetSharedGeometryBuffer(
    geom,
    RTCBufferType::Index as u32,
    0, RTCFormat::UInt3 as u32,
    index_data.as_ptr(), 0, 12, triangle_count,
);
```

### Issue 3: Wrong Buffer Type Constant

**Symptom:** Still error 3 after adding index buffer

**Root Cause:** Passed `0` for buffer type thinking it was `RTC_BUFFER_TYPE_VERTEX`, but 0 = INDEX, 1 = VERTEX.

**Fix:** Use typed enum with explicit cast:
```rust
RTCBufferType::Vertex as u32  // 1, not 0
```

### Issue 4: Wrong Transform Format

**Symptom:** Scene built but nothing rendered (only sky visible)

**Root Cause:** Used hardcoded `23` for transform format (old Embree 3 value), should be `0x9244` for Embree 4.

**Fix:**
```rust
rtcSetGeometryTransform(
    inst_geom,
    0,  // time step
    RTCFormat::Float4x4ColumnMajor as u32,  // 0x9244
    transform_array.as_ptr(),
);
```

### Issue 5: Embree 4 API Change (rtcIntersect1)

**Symptom:** Access violation when rendering

**Root Cause:** Embree 4 changed `rtcIntersect1` signature:
```c
// Embree 3:
rtcIntersect1(scene, &context, &rayhit);

// Embree 4:
rtcIntersect1(scene, &rayhit, NULL);  // context moved to optional args
```

**Fix:** Updated FFI declaration and removed deprecated RTCIntersectContext.

### Issue 6: Premature Scene Release

**Symptom:** Access violation during ray tracing

**Root Cause:** Released prototype scene after creating instances, but instances hold references to it.

**Fix:** Keep prototype_scene alive for entire EmbreeScene lifetime:
```rust
impl<M: Material + Clone + 'static> Drop for EmbreeScene<M> {
    fn drop(&mut self) {
        unsafe {
            rtcReleaseScene(self.scene);
            rtcReleaseScene(self.prototype_scene);  // Release AFTER top-level
            rtcReleaseDevice(self.device);
        }
    }
}
```

## Performance Results

| Metric | Before (Instance-Aware BVH) | After (Embree) |
|--------|---------------------------|----------------|
| BVH Build Time | 40ms | **28ms** |
| Triangles in BVH | 280K (one prototype) | 280K (one prototype) |
| Instance Search | O(100) linear | O(log 100) hierarchical |
| Memory | ~50MB | ~60MB |
| Render (100 inst) | ~3x slower | **Native speed** |

The 28ms build time for 100 instances with 280K triangles is excellent. Embree's two-level BVH eliminates the O(n) instance search overhead.

## Files Changed

| File | Changes |
|------|---------|
| `crates/bif_renderer/src/embree.rs` (NEW) | ~600 LOC - Manual FFI bindings, EmbreeScene |
| `crates/bif_renderer/build.rs` (NEW) | Link embree4.lib from vcpkg |
| `crates/bif_renderer/src/lib.rs` | Export EmbreeScene |
| `crates/bif_viewport/src/lib.rs` | Use EmbreeScene instead of InstancedGeometry |
| `crates/bif_core/src/mesh.rs` | Add extract_triangle_vertices() helper |

## Tests

Embree integration is validated by:
- Scene build without errors (error checking after each Embree call)
- Scene bounds correctly computed
- Ray hits detected (HIT_COUNT > 0 in debug builds)
- Render produces visible geometry

## Key Learnings

1. **Always read actual C headers** - Don't guess enum values, especially for non-standard enums like RTCFormat (0x5003, 0x9003, etc.)

2. **Check API version differences** - Embree 3 → 4 changed rtcIntersect1 signature significantly

3. **Memory lifetime for FFI** - Embree holds pointers to user buffers; buffers must outlive the scene

4. **Two-level BVH is elegant** - One prototype, many instances, Embree handles ray transformation

5. **Error checking after every call** - `rtcGetDeviceError()` is essential for debugging FFI issues

## Milestone 12 Complete ✅

- ✅ Embree 4 integrated via manual FFI (~600 LOC)
- ✅ Two-level BVH: prototype mesh + instance transforms
- ✅ Build time <100ms (28ms for 100 instances)
- ✅ Rendering works with correct geometry
- ✅ All FFI enum values verified against headers
- ✅ Memory safety (buffers kept alive, proper Drop impl)

## Next Steps

- Milestone 13: USD C++ Integration for USDC binary format and references
- Consider: Feature flag for Embree (fallback to instance-aware BVH for systems without Embree)
- Consider: Benchmark with 1K and 10K instances
