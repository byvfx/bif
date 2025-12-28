# Development Log - Milestones 7-8: egui UI Integration & GPU Instancing

**Date:** December 27, 2025  
**Session Duration:** ~2.5 hours  
**Status:** ✅ Complete

---

## Objectives

### Milestone 7: egui UI Integration

- Integrate egui for immediate-mode UI overlay
- Add side panel with stats (FPS, camera, mesh info)
- Display controls help
- Handle UI event consumption

### Milestone 8: GPU Instancing (Replaces USD Export)

- Remove temporary secondary buffer hack
- Implement proper GPU instancing with per-instance transforms
- Render 100 Lucy models in 10x10 grid
- Single draw call for massive performance

---

## Implementation

### 1. egui Integration

**Files Modified:** `crates/bif_viewport/src/lib.rs`, `crates/bif_viewer/src/main.rs`

**Added Dependencies:**

```toml
egui = "0.29"
egui-wgpu = "0.29"
egui-winit = "0.29"
```

**Renderer Changes:**

Added egui state to `Renderer` struct:

```rust
// egui state
egui_ctx: egui::Context,
egui_state: egui_winit::State,
egui_renderer: egui_wgpu::Renderer,

// UI state
pub show_ui: bool,
pub fps: f32,
frame_count: u32,
fps_update_timer: f32,
```

**UI Implementation:**

- Left side panel (300px wide)
- Collapsible sections for Camera, Mesh Info, Viewport, Controls
- FPS counter updated every 0.5 seconds
- Real-time camera and mesh statistics

**Challenges Solved:**

1. **Parameter Count Issues:**
   - `egui_winit::State::new()` required 6 parameters (added `max_texture_side: None`)
   - `egui_wgpu::Renderer::new()` required 5 parameters (added `allow_srgb_render_target: false`)

2. **Borrow Checker:**
   - Closure in `egui_ctx.run()` borrowed entire `self`
   - Solution: Extract needed fields before closure to avoid complex borrows

3. **Lifetime Issues:**
   - egui renderer requires `'static` lifetime for RenderPass
   - Solution: Used `.forget_lifetime()` on RenderPass before passing to egui

4. **Depth Format Mismatch:**
   - Initially configured egui with `Depth24Plus` but render pass had `None`
   - Changed to `None` in initialization for consistency

**Two-Pass Rendering:**

```rust
// Pass 1: 3D scene with depth buffer
{
    let mut render_pass = encoder.begin_render_pass(...);
    // Draw 3D geometry
}

// Pass 2: egui overlay without depth
{
    let mut egui_pass = encoder.begin_render_pass(...)
        .forget_lifetime();
    egui_renderer.render(&mut egui_pass, ...);
}
```

---

### 2. GPU Instancing

**Rationale:** Replaced temporary "two buffer" hack with proper instancing for scalability.

**Files Modified:** `crates/bif_viewport/src/lib.rs`, `crates/bif_viewport/src/shaders/basic.wgsl`

**Removed:**

```rust
// Old: Separate buffers for each instance
vertex_buffer_2: Option<wgpu::Buffer>,
index_buffer_2: Option<wgpu::Buffer>,
num_indices_2: u32,
```

**Added:**

```rust
/// Instance data for GPU instancing
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct InstanceData {
    pub model_matrix: [[f32; 4]; 4],
}
```

**Vertex Buffer Layout:**

Each instance provides a 4x4 model matrix via 4 vec4 attributes:

```rust
impl InstanceData {
    const ATTRIBS: [wgpu::VertexAttribute; 4] =
        wgpu::vertex_attr_array![3 => Float32x4, 4 => Float32x4, 
                                 5 => Float32x4, 6 => Float32x4];

    pub fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<InstanceData>(),
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &Self::ATTRIBS,
        }
    }
}
```

**Pipeline Update:**

```rust
vertex: wgpu::VertexState {
    buffers: &[Vertex::desc(), InstanceData::desc()],
    // ...
}
```

**Shader Update:**

```wgsl
struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec3<f32>,
    @location(3) model_matrix_0: vec4<f32>,
    @location(4) model_matrix_1: vec4<f32>,
    @location(5) model_matrix_2: vec4<f32>,
    @location(6) model_matrix_3: vec4<f32>,
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    let model_matrix = mat4x4<f32>(
        in.model_matrix_0,
        in.model_matrix_1,
        in.model_matrix_2,
        in.model_matrix_3,
    );
    
    let world_position = model_matrix * vec4<f32>(in.position, 1.0);
    out.clip_position = camera.view_proj * world_position;
    
    let world_normal = (model_matrix * vec4<f32>(in.normal, 0.0)).xyz;
    out.normal = normalize(world_normal);
    
    return out;
}
```

**Instance Generation:**

100 Lucy models in 10x10 grid:

```rust
let grid_size = 10;
let spacing = mesh_size * 1.5; // 1.5x mesh size spacing

for x in 0..grid_size {
    for z in 0..grid_size {
        let offset_x = (x as f32 - grid_size as f32 / 2.0) * spacing;
        let offset_z = (z as f32 - grid_size as f32 / 2.0) * spacing;
        
        let model_matrix = Mat4::from_translation(
            Vec3::new(offset_x, 0.0, offset_z)
        );
        
        instances.push(InstanceData {
            model_matrix: model_matrix.to_cols_array_2d(),
        });
    }
}
```

**Single Draw Call:**

```rust
render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
render_pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
render_pass.draw_indexed(0..self.num_indices, 0, 0..self.num_instances);
```

**Camera Adjustments:**

- Far clipping plane: `10x` → `20x` distance (better view of distant instances)
- Near clipping plane: Kept at `0.01x` distance

---

## Statistics

### Mesh: Lucy Low Poly

- **Vertices:** 140,278
- **Indices:** 840,768
- **Triangles:** 280,256

### Instancing

- **Instances:** 100 (10x10 grid)
- **Total Triangles:** 28,025,600
- **Draw Calls:** 1 (instanced)
- **FPS:** 60+ (VSync-limited)

### Code Changes

- **Files Modified:** 2
- **Lines Changed:** ~150
- **Compilation Time:** ~2 minutes (release build)

---

## Visual Results

**Before (Milestone 6):**

- 2 Lucy models using duplicate buffers
- 2 separate draw calls

**After (Milestones 7-8):**

- 100 Lucy models using single buffer + instance data
- 1 instanced draw call
- egui panel showing:
  - FPS: 60+
  - Camera stats (position, target, distance, angles, FOV, near/far)
  - Mesh info (vertices, indices, instances, bounds, center, size)
  - Viewport info (resolution, aspect ratio)
  - Controls help (mouse/keyboard shortcuts)

---

## Learnings

### egui Integration

1. **Immediate Mode Philosophy:** Build UI every frame, no state management needed
2. **Lifetime Management:** egui-wgpu requires `'static` lifetime workarounds
3. **Event Handling:** UI should consume events before viewport to prevent clicks "falling through"
4. **Borrow Patterns:** Inline UI building avoids complex borrow checker issues

### GPU Instancing

1. **Mat4 as Instance Data:** 4x4 matrix = 4 vec4 vertex attributes (locations 3-6)
2. **Step Mode:** `VertexStepMode::Instance` advances per-instance, not per-vertex
3. **Transform Order:** Model → View → Projection (reverse multiplication in shader)
4. **Normal Transform:** Must transform normals by model matrix for correct lighting

### Performance Insights

- **Instancing Efficiency:** 100 instances with 1 draw call vs 100 draw calls = massive CPU savings
- **Memory Layout:** Instance buffer is tiny (100 × 64 bytes = 6.4KB) compared to vertex duplication (140K vertices × 100 × 36 bytes = 504MB saved!)
- **GPU Utilization:** Shader processes 14M vertices but only reads base mesh once

---

## Next Steps

### Immediate: USD Import (Milestone 9)

Instead of Qt integration, focus on USD import first:

1. Add `usd-rs` or write custom USDA parser
2. Load USD stage with primitives and instances
3. Map UsdGeomMesh → BIF MeshData
4. Map UsdGeomPointInstancer → instance buffer
5. Handle basic materials and transforms

**Rationale:** Proving USD compatibility early validates architecture before complex Qt work.

### After USD: CPU Path Tracer (Milestone 10)

Port Go raytracer to Rust:

1. Set up `bif_renderer` crate for CPU path tracing
2. Port core raytracing primitives (sphere, triangle, BVH)
3. Implement materials (Lambert, Metal, Dielectric)
4. Port HDRI environment map loading
5. Multi-threaded bucket rendering
6. Integrate with viewport as "Render" button

---

## Technical Debt

- [ ] Normal transform should use inverse transpose of model matrix (currently assumes uniform scale)
- [ ] Instance buffer is static - need dynamic updates for animation
- [ ] No frustum culling - rendering all 100 instances even if off-screen
- [ ] UI scaling needs testing on high-DPI displays
- [ ] egui textures need proper cleanup on shutdown

---

## Code Quality

**Tests:** 26/26 passing (no new tests this milestone)  
**Warnings:** 0  
**Clippy:** Clean  
**Build Time:** ~2 minutes (release)

---

## Session Notes

This was a highly productive session combining two milestones:

1. **egui integration** proved surprisingly straightforward once lifetime issues were understood
2. **GPU instancing** was a textbook implementation with excellent results
3. Both features working together show the foundation is solid for scaling to 10K+ instances

The decision to pivot from USD export → USD import was strategic: we need to prove we can read production scene files before worrying about Qt UI. The immediate-mode egui UI is perfect for development, and Qt can come later when we understand production workflows better.

**Mood:** 🚀 Excellent progress, architecture validated, ready for real scene data!

---

## Commands Used

```bash
# Add egui dependencies
cargo add egui egui-wgpu egui-winit --package bif_viewport

# Build and test
cargo build --release
cargo run --release

# Check performance
# (Observed 60 FPS with VSync, 100 instances, 28M triangles)
```

---

## Files Created/Modified

### Modified

- `crates/bif_viewport/src/lib.rs` (+150 lines)
  - Added `InstanceData` struct
  - Integrated egui state and rendering
  - Replaced dual buffers with instancing
  - Added FPS tracking
  
- `crates/bif_viewport/src/shaders/basic.wgsl` (+15 lines)
  - Added instance matrix inputs
  - Model matrix reconstruction
  - Normal transformation

- `crates/bif_viewer/src/main.rs` (+10 lines)
  - egui event handling
  - FPS update calls

- `crates/bif_viewport/Cargo.toml` (+3 lines)
  - egui dependencies

### Project Structure

```
crates/
  bif_viewport/
    src/
      lib.rs              (811 lines, +150)
      shaders/
        basic.wgsl        (40 lines, +15)
  bif_viewer/
    src/
      main.rs            (160 lines, +10)
```

---

**Total Session Time:** ~2.5 hours  
**Coffee Consumed:** 2 cups ☕☕  
**Bugs Fixed:** 5 (egui params, borrow checker, lifetime issues)  
**Satisfying Moments:** Seeing 100 Lucy models render at 60 FPS! 🎉
