# Development Log - Milestones 5-6: Mesh Loading, Depth Testing, and Enhanced Controls

**Date:** December 27, 2025  
**Session Duration:** ~3 hours  
**Status:** ✅ Complete

---

## Objectives

### Milestone 5: OBJ Mesh Loading
- Load and render complex OBJ models (Lucy statue)
- Compute normals for models without normal data
- Auto-frame mesh in viewport

### Milestone 6: Depth Testing & Enhanced Controls
- Implement depth buffer for proper Z-ordering
- Add mouse scroll wheel for zoom (dolly)
- Add middle mouse button for panning (track)
- Complete Houdini-style viewport controls

---

## Implementation

### 1. OBJ Mesh Loading

**File:** `crates/bif_viewport/src/lib.rs`

Added `MeshData` struct with OBJ loading:
```rust
#[derive(Clone)]
pub struct MeshData {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub bounds_min: Vec3,
    pub bounds_max: Vec3,
}

impl MeshData {
    pub fn load_obj<P: AsRef<Path>>(path: P) -> Result<Self>
}
```

**Features:**
- Uses `tobj` crate for robust OBJ parsing
- Computes per-face normals when mesh lacks them
- Accumulates and normalizes vertex normals for smooth shading
- Calculates axis-aligned bounding box (AABB)

**Loaded Model:**
- Lucy statue (`lucy_low.obj`): 140,278 vertices, 840,768 indices
- Bounds: (-464.90, -0.02, -266.78) to (464.99, 1597.11, 266.93)
- Size: 1923.64 units

### 2. Normal Computation

For meshes without normals (like Lucy), implemented per-face normal computation:

```rust
// For each triangle face
let edge1 = p1 - p0;
let edge2 = p2 - p0;
let face_normal = edge1.cross(edge2).normalize();

// Accumulate at vertices
for &idx in &[i0, i1, i2] {
    normals[idx] += face_normal;
}

// Normalize accumulated normals
for normal in &mut normals {
    *normal = normal.normalize();
}
```

**Visualization:** Shader maps normals to RGB colors (X→R, Y→G, Z→B) for visual verification.

### 3. Auto-Frame Mesh

**File:** `crates/bif_viewport/src/lib.rs`

```rust
pub fn frame_mesh(&mut self) {
    let mesh_center = (self.mesh_bounds_min + self.mesh_bounds_max) * 0.5;
    let mesh_size = (self.mesh_bounds_max - self.mesh_bounds_min).length();
    let camera_distance = mesh_size * 1.5;
    
    self.camera.target = mesh_center;
    self.camera.distance = camera_distance;
    self.camera.update_position_from_angles();
    
    // Dynamic near/far planes based on distance
    self.camera.near = camera_distance * 0.01;
    self.camera.far = camera_distance * 10.0;
}
```

**Key:** F key triggers framing, auto-adjusting camera near/far planes.

### 4. Depth Testing

**File:** `crates/bif_viewport/src/lib.rs`

Added depth texture creation:
```rust
fn create_depth_texture(device: &Device, size: (u32, u32)) 
    -> (wgpu::Texture, wgpu::TextureView) 
{
    let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
        format: wgpu::TextureFormat::Depth24Plus,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        // ...
    });
    // ...
}
```

**Render Pass Configuration:**
```rust
depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
    view: &self.depth_view,
    depth_ops: Some(wgpu::Operations {
        load: wgpu::LoadOp::Clear(1.0),
        store: wgpu::StoreOp::Store,
    }),
    stencil_ops: None,
}),
```

**Pipeline Configuration:**
```rust
depth_stencil: Some(wgpu::DepthStencilState {
    format: wgpu::TextureFormat::Depth24Plus,
    depth_write_enabled: true,
    depth_compare: wgpu::CompareFunction::Less,
    // ...
}),
```

**Testing:** Added second Lucy instance offset +500 units in Z for occlusion verification.
> **Note:** Secondary buffers are temporary - will be replaced with proper GPU instancing later.

### 5. Enhanced Camera Controls

**File:** `crates/bif_viewer/src/main.rs`

#### Mouse Controls

**Left Mouse (Orbit/Tumble):**
```rust
WindowEvent::MouseInput { button: MouseButton::Left, .. } => {
    self.left_mouse_pressed = state == ElementState::Pressed;
}

// In CursorMoved
renderer.camera.orbit(
    -delta_x as f32 * sensitivity,
    -delta_y as f32 * sensitivity,
);
```

**Middle Mouse (Pan/Track):**
```rust
WindowEvent::MouseInput { button: MouseButton::Middle, .. } => {
    self.middle_mouse_pressed = state == ElementState::Pressed;
}

// In CursorMoved - scaled with camera distance
let distance_scale = renderer.camera.distance * 0.0001;
renderer.camera.pan(
    -delta_x as f32 * sensitivity * distance_scale,
    delta_y as f32 * sensitivity * distance_scale,
    0.0,
    1.0,
);
```

**Mouse Wheel (Dolly/Zoom):**
```rust
WindowEvent::MouseWheel { delta, .. } => {
    let scroll_amount = match delta {
        MouseScrollDelta::LineDelta(_, y) => y * 100.0,
        MouseScrollDelta::PixelDelta(pos) => pos.y as f32,
    };
    
    renderer.camera.dolly(-scroll_amount);
}
```

#### Keyboard Controls (WASD/QE)

**File:** `crates/bif_math/src/camera.rs`

Key fix: Scale movement speed with camera distance for consistent feel:
```rust
pub fn pan(&mut self, right: f32, up: f32, forward: f32, delta_time: f32) {
    // Scale speed with distance for consistent movement at any zoom level
    let speed = self.move_speed * self.distance * delta_time;
    
    let view_dir = (self.target - self.position).normalize();
    let right_dir = view_dir.cross(self.up).normalize();
    let up_dir = right_dir.cross(view_dir).normalize();
    
    let movement = right_dir * right * speed
        + up_dir * up * speed
        + view_dir * forward * speed;
    
    self.position += movement;
    self.target += movement;
}
```

**Critical:** Added `about_to_wait` event handler to continuously request redraw when keys are pressed:
```rust
fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
    if !self.keys_pressed.is_empty() {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}
```

---

## Controls Summary

| Input | Action | Status |
|-------|--------|--------|
| **Left Mouse + Drag** | Orbit/Tumble camera | ✅ |
| **Middle Mouse + Drag** | Pan/Track camera | ✅ |
| **Mouse Scroll** | Dolly/Zoom in-out | ✅ |
| **W / S** | Move forward/backward | ✅ |
| **A / D** | Strafe left/right | ✅ |
| **Q / E** | Move up/down | ✅ |
| **F** | Frame mesh in view | ✅ |

**Paradigm:** Matches Houdini viewport controls (tumble/track/dolly).

---

## Technical Challenges & Solutions

### Challenge 1: Lucy Disappeared After Loading
**Problem:** Camera far plane was 100.0, but Lucy needed 2885+ units.  
**Solution:** Dynamic near/far calculation based on camera distance:
```rust
camera.near = distance * 0.01;  // 28.85
camera.far = distance * 10.0;   // 28,854.6
```

### Challenge 2: Lucy Rendered as Solid Green
**Problem:** `lucy_low.obj` has no normal data, all vertices defaulted to [0,1,0].  
**Solution:** Compute per-face normals from geometry using cross products, accumulate and normalize at vertices.

### Challenge 3: WASD Movement Not Working
**Problem:** Window only requested redraw on events, not continuously.  
**Solution:** Added `about_to_wait` handler to request continuous redraw when keys are held.

### Challenge 4: WASD Movement Too Slow
**Problem:** `move_speed = 2.0` but Lucy is at ~2885 unit scale.  
**Solution:** Scale speed with camera distance: `move_speed * distance * delta_time`.

### Challenge 5: Middle Mouse Pan Too Sensitive
**Problem:** Initial sensitivity of 1.5 with 0.001 scale caused viewport to "jump".  
**Solution:** Reduced to 0.1 sensitivity with 0.0001 distance scale for fine control.

---

## Performance

**Metrics:**
- Lucy low-poly: 140K vertices, 841K indices
- Rendering: 60 FPS (VSync locked)
- Depth testing: No performance impact
- Memory: ~12 MB for mesh data (vertex + index buffers)

**Optimization Note:** Current implementation duplicates mesh data for second instance. Future work will use GPU instancing for efficiency.

---

## Testing

### Manual Testing
- ✅ Lucy loads and renders with computed normals
- ✅ Normal visualization shows colorful surface (RGB from XYZ)
- ✅ Depth testing: closer Lucy occludes farther Lucy correctly
- ✅ F key frames mesh perfectly
- ✅ All camera controls feel responsive and scale-appropriate
- ✅ Mouse scroll zooms smoothly
- ✅ Middle mouse pans without "jumps"
- ✅ WASD/QE movement works at consistent speed

### Automated Tests
- All 26 existing tests pass
- No regressions in core math or rendering

---

## Results

**Visual:**
- Lucy statue renders with smooth, colorful normal visualization
- Proper depth occlusion between two instances
- Interactive navigation feels professional and responsive

**Architecture:**
- Clean separation: math (camera) → viewport (renderer) → viewer (input)
- Event-driven input with continuous redraw for held keys
- Distance-scaled controls adapt to any mesh size

**Code Stats:**
- OBJ loading + normals: ~120 LOC
- Depth testing: ~60 LOC
- Enhanced controls: ~80 LOC
- Total added: ~260 LOC

---

## Known Issues & Future Work

### TODO: Replace Secondary Buffers with Instancing
Current implementation uses duplicate vertex/index buffers for second Lucy instance. This is inefficient.

**Future:** Implement GPU instancing with model matrix per-instance:
- Single vertex/index buffer
- Instance buffer with transforms
- Draw call with instance count

### Future Enhancements
1. **Camera Presets** - Front/Top/Side/Perspective views (Houdini-style)
2. **Smooth Camera Transitions** - Interpolate to new positions
3. **Selection/Focus** - Click object to orbit around it
4. **Grid & Ground Plane** - Visual reference for scale
5. **egui UI Panel** - Camera info, FPS counter, mesh stats

---

## Files Modified

- ✅ `crates/bif_math/src/camera.rs` - Distance-scaled movement, dolly method
- ✅ `crates/bif_viewer/src/main.rs` - Mouse wheel, middle mouse, continuous redraw
- ✅ `crates/bif_viewport/src/lib.rs` - OBJ loading, normals, depth testing, frame mesh
- ✅ `crates/bif_viewport/src/shaders/basic.wgsl` - Normal visualization
- ✅ `crates/bif_viewport/Cargo.toml` - Added tobj dependency
- ✅ `devlog/DEVLOG_2025-12-27_milestone5_6.md` - This file

---

## Lessons Learned

1. **Scale Matters** - Always scale movement/sensitivity with camera distance for consistent UX
2. **Continuous Redraw** - Input-driven applications need event loop management for held keys
3. **Dynamic Near/Far** - Camera planes must adapt to scene bounds to avoid clipping
4. **Normal Computation** - Many models lack normals; robust importers must compute them
5. **Incremental Testing** - Frame key (F) was critical for debugging camera issues
6. **Sensitivity Tuning** - User feedback essential for control feel; started too high, iterated down

---

**Milestones 5 & 6: Complete ✅**

Next: **Milestone 7: egui UI Integration** - Add side panel with scene stats and camera controls.
