# Development Log - December 27, 2025 (Milestone 2)

**Session:** Milestone 2 - wgpu Window  
**Duration:** ~1 hour  
**Status:** ✅ Complete

## Goals

- ✅ Set up wgpu rendering context in `bif_render` crate
- ✅ Create window with winit in `bif_viewer` crate
- ✅ Render solid color clear screen
- ✅ Handle window events (resize, close)

## Work Completed

### Milestone 2: wgpu Window - Complete! ✅

#### Renderer (`bif_render/src/lib.rs`)

Built a complete `Renderer` struct managing wgpu state:

- **Initialization (`new()`):**
  - Creates wgpu instance with PRIMARY backends (Vulkan/DX12/Metal)
  - Creates surface from winit window
  - Requests adapter with HighPerformance preference
  - Requests device and queue
  - Configures surface with sRGB format, VSync (Fifo), window size
  
- **Resize handling (`resize()`):**
  - Updates internal size tracking
  - Reconfigures surface with new dimensions
  - Validates non-zero dimensions

- **Rendering (`render()`):**
  - Gets current surface texture
  - Creates command encoder
  - Begins render pass with clear color
  - Submits commands and presents

- **State management:**
  - Stores `Surface`, `Device`, `Queue`, `SurfaceConfiguration`
  - Tracks window size for resize operations

#### Viewer Application (`bif_viewer/src/main.rs`)

Implemented winit application with modern `ApplicationHandler` trait:

- **App struct:**
  - Holds `Window` and `Renderer` as `Option<T>` (lazy init)
  - Initialized on `resumed()` event

- **Event handling:**
  - `resumed()`: Creates window (1280x720), initializes renderer
  - `window_event()`: Handles close, resize, redraw
  - `RedrawRequested`: Renders frame with dark blue clear color (0.1, 0.2, 0.3)
  - Error handling: Surface lost → reconfigure, OOM → exit

- **Event loop:**
  - Uses `ControlFlow::Poll` for continuous rendering
  - `request_redraw()` after each frame for animation loop

#### Dependencies

Added `winit` to `bif_render/Cargo.toml` (needed for `Window` type in surface creation).

## Technical Details

### wgpu Setup

```rust
// Instance creation
let instance = Instance::new(wgpu::InstanceDescriptor {
    backends: wgpu::Backends::PRIMARY,
    ..Default::default()
});

// Surface configuration
let config = SurfaceConfiguration {
    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
    format: surface_format,  // sRGB preferred
    width: size.width,
    height: size.height,
    present_mode: wgpu::PresentMode::Fifo,  // VSync
    alpha_mode: surface_caps.alpha_modes[0],
    view_formats: vec![],
    desired_maximum_frame_latency: 2,
};
```

### Error Handling

Used `anyhow::Result` in renderer, so viewer needs to downcast errors to check for specific `wgpu::SurfaceError` types:

```rust
if let Err(e) = renderer.render(clear_color) {
    if let Some(surface_err) = e.downcast_ref::<wgpu::SurfaceError>() {
        match surface_err {
            wgpu::SurfaceError::Lost => { /* reconfigure */ }
            wgpu::SurfaceError::OutOfMemory => { /* exit */ }
            _ => { /* log */ }
        }
    }
}
```

## Learnings

### Rust Concepts

1. **winit 0.30 API** - Modern `ApplicationHandler` trait pattern
2. **Async in blocking context** - `pollster::block_on()` for renderer init
3. **Option<T> for lazy init** - Window/renderer created on `resumed()` event
4. **Arc<Window>** - Shared ownership for surface creation (wgpu requires `'static`)
5. **Error downcasting** - `anyhow::Error::downcast_ref()` to check error types

### wgpu Patterns

1. **Surface lifecycle:**
   - Surface → Adapter → Device/Queue → Configure
   - Reconfigure on resize
   
2. **Render loop:**
   - Get texture → Create view → Encoder → Render pass → Submit → Present
   
3. **VSync:** `PresentMode::Fifo` ensures frame pacing

4. **Surface errors:**
   - `Lost`: Device disconnected, need reconfigure
   - `OutOfMemory`: Fatal, should exit
   - `Timeout`/`Outdated`: Can retry

## Statistics

- **Files modified:** 4
  - `crates/bif_render/src/lib.rs` (134 lines)
  - `crates/bif_render/Cargo.toml` (added winit)
  - `crates/bif_viewer/src/main.rs` (118 lines)
- **Lines of code:** ~250 (production code)
- **Commits:** 1
- **Time spent:** ~1 hour

## Testing

✅ Window opens at 1280x720  
✅ Renders dark blue clear color  
✅ Handles resize events  
✅ Handles close events  
✅ GPU rendering working (Vulkan on Windows)

## Issues Encountered

1. **Missing winit in bif_render:**
   - Error: `failed to resolve: use of unresolved module or unlinked crate 'winit'`
   - Fix: Added `winit = { workspace = true }` to `bif_render/Cargo.toml`
   
2. **Error handling type mismatch:**
   - Error: `expected 'Error', found 'SurfaceError'` in pattern match
   - Fix: Used `downcast_ref()` to check for specific error types
   
3. **Verbose wgpu logs:**
   - Issue: Many "Device::maintain: waiting for submission" logs
   - Note: Normal for continuous rendering, can filter in production

## Next Session

### Milestone 3: Basic Rendering

1. **Create shader module:**
   - Write WGSL vertex and fragment shaders
   - Compile and load into wgpu
   - Estimated: 30 min

2. **Render a triangle:**
   - Set up vertex buffer
   - Create render pipeline
   - Draw single triangle
   - Estimated: 45 min

3. **Add camera:**
   - Camera struct with view/projection matrices
   - Uniform buffer for camera data
   - Bind group setup
   - Estimated: 1 hour

**Total estimated for Milestone 3: 2-2.5 hours**

## Notes

- Vulkan working well on Windows (RTX 4070 detected)
- Rockstar Games overlay warning can be ignored
- Event loop pattern is clean and extensible
- Ready to add actual rendering (shaders, pipelines, geometry)
- Clear color provides good visual confirmation rendering works

---

**Milestone 2 complete!** Window system working, ready for graphics programming.
