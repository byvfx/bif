# DevLog - Milestone 3: Triangle Rendering with Camera

**Date:** 2025-12-27  
**Duration:** ~1.5 hours  
**Status:** ✅ Complete

## Overview
Implemented basic 3D rendering with a triangle, WGSL shaders, and a camera system with view-projection matrices. The triangle now renders through a proper perspective camera pipeline.

## Objectives
- [x] Create WGSL shader module with vertex and fragment stages
- [x] Implement Camera struct with view and projection matrices
- [x] Set up uniform buffer for camera data
- [x] Integrate camera with shader pipeline using bind groups
- [x] Render a textured RGB triangle with perspective projection
- [x] Handle window resize with automatic aspect ratio updates

## Technical Implementation

### 1. WGSL Shader Creation
**File:** `crates/bif_render/src/shaders/basic.wgsl`

Created a complete shader with:
- **Camera Uniform Binding:**
  ```wgsl
  struct CameraUniform {
      view_proj: mat4x4<f32>,
  }
  @group(0) @binding(0) var<uniform> camera: CameraUniform;
  ```

- **Vertex Shader:**
  - Takes position (location 0) and color (location 1) inputs
  - Applies camera view-projection matrix: `camera.view_proj * vec4(in.position, 1.0)`
  - Passes interpolated color to fragment shader

- **Fragment Shader:**
  - Outputs interpolated vertex colors with alpha=1.0
  - Creates smooth RGB gradient across triangle

### 2. Camera System
**File:** `crates/bif_math/src/camera.rs`

Implemented full 3D camera with:
```rust
pub struct Camera {
    position: Vec3,
    target: Vec3,
    up: Vec3,
    fov_y: f32,
    aspect: f32,
    near: f32,
    far: f32,
}
```

**Methods:**
- `view_matrix()` - Right-handed look-at matrix using `Mat4::look_at_rh()`
- `projection_matrix()` - Right-handed perspective using `Mat4::perspective_rh()`
- `view_projection_matrix()` - Combined matrix (projection × view)
- `set_aspect()` - Updates aspect ratio on window resize

**Defaults:**
- Position: (0, 0, 3)
- Target: (0, 0, 0)
- Up: (0, 1, 0)
- FOV: 45°
- Near: 0.1, Far: 100.0

**Tests:** 4 passing tests for creation, view matrix, projection matrix, and aspect ratio updates.

### 3. Uniform Buffer Integration
**File:** `crates/bif_render/src/lib.rs`

Added uniform buffer system:
```rust
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct CameraUniform {
    view_proj: [[f32; 4]; 4],
}
```

**Key Implementation Details:**
- Used `bytemuck` traits for zero-copy GPU upload
- Created bind group layout matching shader `@group(0) @binding(0)`
- Uniform buffer has `UNIFORM | COPY_DST` usage flags
- Matrix converted with `Mat4::to_cols_array_2d()`
- Updated in `resize()` when aspect ratio changes

### 4. Render Pipeline Updates
Modified pipeline to include:
- Bind group layout for camera uniform at group 0
- Automatic aspect ratio update on window resize
- Proper matrix upload to GPU before rendering

**Vertex Buffer:**
```rust
const VERTICES: &[Vertex] = &[
    Vertex { position: [0.0, 0.5, 0.0], color: [1.0, 0.0, 0.0] },   // Top (red)
    Vertex { position: [-0.5, -0.5, 0.0], color: [0.0, 1.0, 0.0] }, // Bottom-left (green)
    Vertex { position: [0.5, -0.5, 0.0], color: [0.0, 0.0, 1.0] },  // Bottom-right (blue)
];
```

## Learnings

### 1. Uniform Buffer Workflow
- **COPY_DST flag is mandatory** for `write_buffer()` operations
- Bind group layouts must **exactly match** shader declarations
- Use `Mat4::to_cols_array_2d()` for WGSL-compatible matrix format
- `bytemuck::Pod` trait enables safe zero-copy casting

### 2. Camera Mathematics
- Right-handed coordinate system (RH) is standard for wgpu
- Projection matrix encodes perspective distortion
- View matrix transforms world to camera space
- **Order matters:** `projection × view × position`, not `view × projection`

### 3. Window Resize Handling
- Must update surface configuration **AND** camera aspect ratio
- Uniform buffer needs re-upload after aspect change
- Surface can be lost during resize (handle `SurfaceError::Lost`)

### 4. Shader Integration
- `@group(0) @binding(0)` maps to bind group index 0
- Vertex attributes use `@location(N)` for indexing
- `@builtin(position)` is clip-space position, not world position
- Fragment interpolation is automatic for `@location` variables

## Issues Encountered

### 1. Missing Dependency
**Error:** `winit` not in `bif_render` dependencies  
**Solution:** Added `winit = { workspace = true }` to `bif_render/Cargo.toml`

### 2. Verbose wgpu Logs
**Issue:** Terminal flooded with `Device::maintain` logs  
**Solution:** Accepted as normal for development (can filter with `RUST_LOG` in production)

## Statistics
- **Lines Added:** ~200 LOC
- **Files Modified:** 5
- **Tests Added:** 4 (Camera tests)
- **Tests Passing:** 26/26 (all bif_math tests)
- **Build Time:** 3.54s
- **Rendering:** 60 FPS (VSync enabled)

## Visual Result
✅ **Triangle renders successfully** with:
- Dark blue background (0.1, 0.2, 0.3)
- RGB color gradient (red → green → blue)
- Perspective projection from camera at (0, 0, 3)
- Proper aspect ratio handling on resize

## Next Steps (Milestone 4 Ideas)
1. **Camera Controls:**
   - Mouse orbit (drag to rotate around target)
   - WASD keyboard movement
   - Scroll wheel zoom

2. **Geometry Loading:**
   - OBJ file loader
   - USD file support
   - Basic mesh rendering

3. **UI Integration:**
   - egui panels around viewport
   - Camera settings panel
   - Performance metrics display

4. **Depth Testing:**
   - Enable depth buffer
   - Render multiple triangles
   - Z-fighting prevention

## Commit
```
Complete Milestone 3: Add triangle rendering with camera system

- Create WGSL shaders with camera uniform binding
- Implement Camera with view/projection matrices (4 tests)
- Set up uniform buffer with bind groups
- Integrate camera with renderer pipeline
- Handle aspect ratio updates on window resize
```

---
**Total Development Time (All Milestones):** ~2.5 hours  
**Milestones Completed:** 3/∞
