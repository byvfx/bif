# BIF Getting Started Guide

**Status:** ✅ Milestones 0-11 Complete! (December 2025)

**For:** Side project development (10-20 hours/week)
**Approach:** Learn by building, understand every line
**Actual Timeline:** ~34 hours over 2 weeks (December 2025)

> **Note:** This guide was used to implement Milestones 0-11 and remains as a reference for understanding the implementation approach. For current status and next milestones, see [MILESTONES.md](MILESTONES.md) and [SESSION_HANDOFF.md](SESSION_HANDOFF.md).

## How to Use This Guide

Each **Milestone** represents a major capability unlock. Within each milestone:

- **Overview**: What you're building and why it matters
- **What You'll Learn**: Rust concepts you'll encounter
- **Prerequisites**: What you need from previous milestones
- **Tasks**: Concrete checkboxes (check them off as you go)
- **Key Concepts**: Deep dives to understand what's happening
- **Common Pitfalls**: What trips people up (so you can avoid it)
- **Validation**: How to know you're done

**Estimated time** = actual focused coding hours, not calendar weeks.

**When stuck**: Come back with specific questions. Debugging together is how you learn.

---

## Milestone 0: Environment Setup

**Est:** 2-5 hours  
**Goal:** Get Rust toolchain working, create project structure

### Overview

Before writing any BIF code, you need a working Rust environment and project skeleton. This milestone is about tooling, not coding.

### What You'll Learn

- Cargo workspaces (multi-crate projects)
- Rust project structure
- How to add dependencies

### Tasks

- [ ] Install Rust via rustup: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- [ ] Verify installation: `rustc --version` (should show 1.75+)
- [ ] Install Rust Analyzer extension for your editor (VSCode/CLion/Vim)
- [ ] Create project directory: `mkdir bif && cd bif`
- [ ] Initialize workspace `Cargo.toml`
- [ ] Create first crate: `cargo new crates/bif_math --lib`
- [ ] Test build: `cargo build`
- [ ] Create `docs/` directory and copy ARCHITECTURE.md there

### Key Concepts

**Cargo Workspaces**:

A workspace lets you have multiple related crates (libraries/binaries) in one repo. This is important for BIF because you'll have separate crates for math, scene, renderer, viewport, etc.

```toml
# Root Cargo.toml
[workspace]
members = [
    "crates/bif_math",
    "crates/bif_scene",
    # More crates later
]

# Shared dependencies (so all crates use same versions)
[workspace.dependencies]
glam = "0.25"
anyhow = "1.0"
```

**Why separate crates?**

- Faster compilation (only rebuild what changed)
- Clear module boundaries
- Can reuse crates in other projects

### Common Pitfalls

- **Cargo.toml location matters**: Workspace root vs crate root are different files
- **Rust Analyzer sometimes needs restart**: If autocomplete stops working, restart your editor
- **Path issues**: Make sure you're in the workspace root when running `cargo build`

### Validation

```bash
cargo build  # Should succeed with no errors
cargo test   # Should run (even if no tests yet)
tree crates  # Should show bif_math directory
```

---

## Milestone 1: Math Library Port

**Est:** 10-20 hours  
**Goal:** Port Vec3, Ray, AABB from your Go raytracer to Rust

### Overview

Your Go raytracer has solid math primitives. Rather than learning Rust on a blank slate, you'll port code you already understand. This teaches Rust ownership/borrowing with familiar algorithms.

### What You'll Learn

- Rust structs and impl blocks
- Operator overloading (Add, Sub, Mul, etc.)
- Copy vs Clone semantics
- Basic testing in Rust

### Prerequisites

- Milestone 0 complete
- Your Go raytracer code available for reference

### Tasks

- [ ] Create `crates/bif_math/src/vec3.rs`
- [ ] Port `Vec3` struct with x, y, z fields
- [ ] Implement `Vec3::dot()`, `Vec3::cross()`, `Vec3::length()`
- [ ] Implement operator overloads: `Add`, `Sub`, `Mul`, `Div`, `Neg`
- [ ] Add `Point3` and `Color` type aliases
- [ ] Create `crates/bif_math/src/ray.rs`
- [ ] Port `Ray` struct with origin, direction, time
- [ ] Implement `Ray::at(t)` method
- [ ] Create `crates/bif_math/src/aabb.rs`
- [ ] Port `AABB` struct with min, max
- [ ] Write basic tests for each type
- [ ] Make all tests pass: `cargo test`

### Key Concepts

#### Struct Definition vs Go

**Go:**

```go
type Vec3 struct {
    X, Y, Z float64
}
```

**Rust:**

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}
```

**What's different:**

- `pub` makes fields public (accessible outside module)
- `#[derive(...)]` auto-generates common traits
- `Debug` = can print with `{:?}`
- `Clone` = can explicitly `.clone()`
- `Copy` = copies implicitly (like Go for small structs)
- `PartialEq` = can compare with `==`

#### Operator Overloading

**Go:**

```go
func (v Vec3) Add(other Vec3) Vec3 {
    return Vec3{v.X + other.X, v.Y + other.Y, v.Z + other.Z}
}
// Use: result := v.Add(other)
```

**Rust:**

```rust
impl Add for Vec3 {
    type Output = Self;
    fn add(self, other: Self) -> Self {
        Self::new(self.x + other.x, self.y + other.y, self.z + other.z)
    }
}
// Use: result = v + other;  // Cleaner!
```

**Why Rust is better here:** Mathematical code looks like math: `a + b * c` instead of `a.Add(b.Mul(c))`

#### Copy Semantics (Important!)

```rust
#[derive(Copy, Clone)]
struct Vec3 { x: f32, y: f32, z: f32 }

let a = Vec3::new(1.0, 2.0, 3.0);
let b = a;  // This COPIES a, doesn't move it
let c = a;  // Can still use a! It was copied

// Without Copy:
struct Mesh { vertices: Vec<Vec3> }  // Vec<T> is NOT Copy
let m1 = Mesh { vertices: vec![] };
let m2 = m1;  // This MOVES m1
// let m3 = m1;  // ERROR! m1 was moved
```

**Rule of thumb:** Small types (3 floats, 1 pointer, etc.) can be `Copy`. Big types (Vec, String, etc.) should NOT be `Copy`.

#### Testing in Rust

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_vec3_add() {
        let a = Vec3::new(1.0, 2.0, 3.0);
        let b = Vec3::new(4.0, 5.0, 6.0);
        let c = a + b;
        assert_eq!(c, Vec3::new(5.0, 7.0, 9.0));
    }
    
    #[test]
    fn test_vec3_dot() {
        let a = Vec3::new(1.0, 0.0, 0.0);
        let b = Vec3::new(0.0, 1.0, 0.0);
        assert_eq!(a.dot(b), 0.0);
    }
}
```

Run with: `cargo test`

### Common Pitfalls

**Pitfall 1: Forgetting `pub`**

```rust
struct Vec3 { x: f32, y: f32, z: f32 }  // NOT public!
// Other crates can't use this

pub struct Vec3 { pub x: f32, pub y: f32, pub z: f32 }  // Public!
```

**Pitfall 2: Type mismatches**

```rust
impl Add for Vec3 {
    type Output = Vec3;  // Must specify output type
    fn add(self, other: Vec3) -> Vec3 { ... }  // Can't just return f32!
}
```

**Pitfall 3: Module visibility**

```rust
// crates/bif_math/src/lib.rs
pub mod vec3;  // Must re-export or other crates can't see it
pub mod ray;
pub mod aabb;

// Now other crates can: use bif_math::vec3::Vec3;
```

### Validation

```bash
cargo test --package bif_math  # All tests pass
cargo build                     # No warnings
cargo clippy                    # Linter passes (install: rustup component add clippy)
```

**Manual validation:**

```rust
use bif_math::vec3::Vec3;

let a = Vec3::new(1.0, 2.0, 3.0);
let b = Vec3::new(4.0, 5.0, 6.0);
let c = a + b;
println!("{:?}", c);  // Should print: Vec3 { x: 5.0, y: 7.0, z: 9.0 }
```

---

## Milestone 2: First Pixels (wgpu Window)

**Est:** 15-25 hours  
**Goal:** Open a window with wgpu and clear it to a color

### Overview

You'll create a basic wgpu window that just fills the screen with a solid color. This seems trivial but teaches the entire wgpu initialization dance. Once this works, adding geometry is "just" more wgpu API calls.

### What You'll Learn

- wgpu initialization (device, queue, surface)
- Window event loops (winit)
- Async/await in Rust
- Render passes

### Prerequisites

- Milestone 1 complete (math library working)
- Comfortable with Rust basics

### Tasks

- [ ] Create `crates/bif_viewport` crate
- [ ] Add dependencies: `wgpu`, `winit`, `pollster`, `env_logger`
- [ ] Create `Application` struct to own window and wgpu state
- [ ] Initialize `winit` window
- [ ] Initialize wgpu (request adapter, device, queue)
- [ ] Create surface and configure swap chain
- [ ] Implement event loop with window resize handling
- [ ] Create render pass that clears to cornflower blue
- [ ] Run and see blue window!
- [ ] Change clear color to test (red, green, your favorite color)

### Key Concepts

#### wgpu Initialization Flow

```rust
// 1. Create window
let event_loop = EventLoop::new();
let window = WindowBuilder::new()
    .with_title("BIF Viewport")
    .with_inner_size(winit::dpi::LogicalSize::new(1280, 720))
    .build(&event_loop)?;

// 2. Create wgpu instance
let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
    backends: wgpu::Backends::all(),  // Try Vulkan, Metal, DX12, etc.
    ..Default::default()
});

// 3. Create surface (OS-specific render target)
let surface = unsafe { instance.create_surface(&window) }?;

// 4. Request adapter (GPU)
let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions {
    power_preference: wgpu::PowerPreference::HighPerformance,
    compatible_surface: Some(&surface),
    ..Default::default()
}).await.unwrap();

// 5. Request device and queue
let (device, queue) = adapter.request_device(
    &wgpu::DeviceDescriptor {
        features: wgpu::Features::empty(),
        limits: wgpu::Limits::default(),
        label: None,
    },
    None,
).await?;

// 6. Configure surface
let surface_caps = surface.get_capabilities(&adapter);
let surface_format = surface_caps.formats[0];  // Usually Bgra8UnormSrgb

surface.configure(&device, &wgpu::SurfaceConfiguration {
    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
    format: surface_format,
    width: 1280,
    height: 720,
    present_mode: wgpu::PresentMode::Fifo,  // VSync
    alpha_mode: surface_caps.alpha_modes[0],
    view_formats: vec![],
});
```

**Why so many steps?** Each step represents a real concept:

- Instance = wgpu library itself
- Surface = window's drawable area
- Adapter = physical GPU
- Device = logical GPU (your program's view)
- Queue = command submission

#### Render Loop

```rust
event_loop.run(move |event, _, control_flow| {
    match event {
        Event::WindowEvent { event, .. } => match event {
            WindowEvent::CloseRequested => *control_flow = ControlFlow::Exit,
            WindowEvent::Resized(new_size) => {
                // Reconfigure surface with new size
                surface.configure(&device, &config);
            }
            _ => {}
        },
        Event::RedrawRequested(_) => {
            // Render frame here
            render_frame(&device, &queue, &surface);
        }
        Event::MainEventsCleared => {
            window.request_redraw();
        }
        _ => {}
    }
});
```

#### Clear Color Render Pass

```rust
fn render_frame(device: &Device, queue: &Queue, surface: &Surface) {
    // Get next frame
    let frame = surface.get_current_texture().unwrap();
    let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
    
    // Create command encoder
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("Render Encoder"),
    });
    
    // Render pass
    {
        let _render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.39,  // Cornflower blue
                        g: 0.58,
                        b: 0.93,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            ..Default::default()
        });
        // Render pass automatically ends when _render_pass drops
    }
    
    // Submit commands
    queue.submit(std::iter::once(encoder.finish()));
    frame.present();
}
```

**Key insight:** The render pass does NOTHING except clear. But this proves your entire wgpu pipeline works.

### Common Pitfalls

**Pitfall 1: Async/.await confusion**

wgpu uses async for GPU initialization:

```rust
let adapter = instance.request_adapter(...).await.unwrap();
//                                           ^^^^^ Must await!
```

Use `pollster::block_on()` to run async code in main:

```rust
fn main() {
    pollster::block_on(run());  // Blocks until async function completes
}

async fn run() {
    let adapter = instance.request_adapter(...).await;
    // ...
}
```

**Pitfall 2: Surface lifetime issues**

Surface must outlive the window:

```rust
// WRONG:
let surface = {
    let window = create_window();
    instance.create_surface(&window)
};  // window dropped here, surface now invalid!

// RIGHT:
let window = create_window();
let surface = instance.create_surface(&window);
// Keep both alive
```

**Pitfall 3: Forgetting to request redraw**

```rust
Event::MainEventsCleared => {
    window.request_redraw();  // Without this, nothing draws!
}
```

### Validation

**Success looks like:**

- Window opens with solid color
- Window resizes correctly (no black bars)
- Can change clear color in code and see it update
- No errors in console

**Debug if:**

- Window is black → Check surface configuration
- Window crashes on resize → Check resize handler
- Nothing appears → Check `request_redraw()` is called

---

## Milestone 3: Render a Triangle

**Est:** 10-15 hours  
**Goal:** Draw a hardcoded triangle using wgpu vertex/fragment shaders

### Overview

The "Hello World" of graphics: render a single triangle. This teaches the full GPU pipeline: vertices → vertex shader → rasterization → fragment shader → pixels.

### What You'll Learn

- Vertex buffers
- Shaders (WGSL)
- Render pipelines
- GPU memory management

### Prerequisites

- Milestone 2 complete (wgpu window working)
- Basic understanding of graphics pipeline

### Tasks

- [ ] Define vertex format (position + color)
- [ ] Create hardcoded triangle vertices in CPU memory
- [ ] Create wgpu vertex buffer
- [ ] Upload vertex data to GPU
- [ ] Write vertex shader (WGSL)
- [ ] Write fragment shader (WGSL)
- [ ] Create render pipeline
- [ ] Draw triangle in render pass
- [ ] See triangle on screen!
- [ ] Modify triangle positions/colors to understand pipeline

### Key Concepts

#### Vertex Data

```rust
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 3],  // x, y, z
    color: [f32; 3],     // r, g, b
}

// Hardcoded triangle
const VERTICES: &[Vertex] = &[
    Vertex { position: [0.0, 0.5, 0.0], color: [1.0, 0.0, 0.0] },   // Top (red)
    Vertex { position: [-0.5, -0.5, 0.0], color: [0.0, 1.0, 0.0] }, // Bottom-left (green)
    Vertex { position: [0.5, -0.5, 0.0], color: [0.0, 0.0, 1.0] },  // Bottom-right (blue)
];
```

**Why `#[repr(C)]`?** Ensures memory layout matches GPU expectations (no Rust padding weirdness).

**Why `bytemuck`?** Safely cast Rust structs to raw bytes for GPU upload.

#### Creating Vertex Buffer

```rust
use wgpu::util::DeviceExt;

let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
    label: Some("Vertex Buffer"),
    contents: bytemuck::cast_slice(VERTICES),  // Convert [Vertex] to &[u8]
    usage: wgpu::BufferUsages::VERTEX,
});
```

#### Shaders (WGSL)

**Vertex Shader** (transforms positions, passes color to fragment shader):

```wgsl
struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) color: vec3<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec3<f32>,
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = vec4<f32>(in.position, 1.0);
    out.color = in.color;
    return out;
}
```

**Fragment Shader** (colors pixels):

```wgsl
@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(in.color, 1.0);
}
```

#### Render Pipeline

```rust
let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
    label: Some("Shader"),
    source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
});

let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
    label: Some("Render Pipeline"),
    layout: None,  // No bind groups yet
    vertex: wgpu::VertexState {
        module: &shader,
        entry_point: "vs_main",
        buffers: &[wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,  // position
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x3,  // color
                },
            ],
        }],
    },
    fragment: Some(wgpu::FragmentState {
        module: &shader,
        entry_point: "fs_main",
        targets: &[Some(wgpu::ColorTargetState {
            format: surface_format,
            blend: Some(wgpu::BlendState::REPLACE),
            write_mask: wgpu::ColorWrites::ALL,
        })],
    }),
    primitive: wgpu::PrimitiveState {
        topology: wgpu::PrimitiveTopology::TriangleList,
        ..Default::default()
    },
    depth_stencil: None,
    multisample: wgpu::MultisampleState::default(),
    multiview: None,
});
```

#### Drawing

```rust
render_pass.set_pipeline(&pipeline);
render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
render_pass.draw(0..3, 0..1);  // 3 vertices, 1 instance
```

### Common Pitfalls

**Pitfall 1: Vertex attribute offsets**

If your vertex struct is:

```rust
struct Vertex {
    position: [f32; 3],  // 12 bytes
    color: [f32; 3],     // 12 bytes, starts at offset 12
}
```

Attributes MUST match:

```rust
attributes: &[
    wgpu::VertexAttribute { offset: 0, ... },     // position
    wgpu::VertexAttribute { offset: 12, ... },    // color at byte 12!
]
```

**Pitfall 2: Coordinate system**

wgpu uses:

- X: -1 (left) to +1 (right)
- Y: -1 (bottom) to +1 (top)
- Z: 0 (near) to 1 (far)

This is different from some other APIs!

**Pitfall 3: Shader compilation errors**

```rust
let shader = device.create_shader_module(...);
// If shader has errors, this panics with cryptic message
// Check console output carefully
```

Enable validation:

```rust
env_logger::init();  // In main()
// Now you get detailed wgpu error messages
```

### Validation

**Success:**

- Triangle appears with smooth color gradient (red/green/blue vertices)
- Triangle is centered in window
- No console errors

**Experiments:**

- Change vertex positions → triangle moves
- Change vertex colors → colors change
- Add a 4th vertex → see if you can make a quad (hint: need 2 triangles)

---

## Milestone 4: Scene Graph (Prototype/Instance)

**Est:** 20-30 hours  
**Goal:** Build Rust scene graph that can load a mesh and instance it 1000+ times

### Overview

This is where BIF's architecture comes alive. You'll implement the prototype/instance pattern that lets you load one mesh and render it thousands of times efficiently.

### What You'll Learn

- `Arc<T>` for shared ownership
- Trait objects (`dyn` traits)
- glTF loading
- HashMap/Vec data structures
- The power of instancing

### Prerequisites

- Milestone 3 complete (can render triangle)
- Understand Arc from ARCHITECTURE.md

### Tasks

- [ ] Create `crates/bif_scene` crate
- [ ] Define `Mesh` struct (vertices, normals, indices)
- [ ] Define `Prototype` struct (Arc<Mesh>, bounds, material ID)
- [ ] Define `Instance` struct (prototype_id, transform matrix)
- [ ] Define `Scene` struct (prototypes Vec, instances Vec)
- [ ] Implement `Scene::add_prototype(mesh) -> usize`
- [ ] Implement `Scene::add_instance(prototype_id, transform)`
- [ ] Add `gltf` dependency
- [ ] Write glTF loader function
- [ ] Load a simple cube.gltf
- [ ] Generate 1000 instances in a grid
- [ ] Update wgpu renderer to use instanced rendering
- [ ] See 1000 cubes rendered!

### Key Concepts

#### Arc for Shared Ownership

```rust
use std::sync::Arc;

struct Prototype {
    id: usize,
    mesh: Arc<Mesh>,  // Shared ownership
}

// Mesh is loaded once
let mesh = Arc::new(load_mesh("cube.gltf"));

// Many prototypes can share it
let proto1 = Prototype { id: 0, mesh: Arc::clone(&mesh) };
let proto2 = Prototype { id: 1, mesh: Arc::clone(&mesh) };

// Arc::clone is CHEAP (just increments ref count)
// Actual mesh data is not duplicated
```

**Memory:**

- 1 mesh loaded: 10 MB
- 1000 prototypes referencing it: 10 MB + (1000 × 8 bytes) = ~10 MB total

#### Scene Graph Structure

```rust
pub struct Scene {
    prototypes: Vec<Arc<Prototype>>,
    instances: Vec<Instance>,
}

impl Scene {
    pub fn add_prototype(&mut self, mesh: Mesh) -> usize {
        let id = self.prototypes.len();
        self.prototypes.push(Arc::new(Prototype {
            id,
            mesh: Arc::new(mesh),
            bounds: mesh.calculate_bounds(),
        }));
        id
    }
    
    pub fn add_instance(&mut self, prototype_id: usize, transform: Mat4) {
        self.instances.push(Instance {
            id: self.instances.len() as u32,
            prototype_id,
            transform,
            visible: true,
        });
    }
}
```

#### Loading glTF

```rust
use gltf;

pub fn load_gltf(path: &str) -> Result<Mesh> {
    let (document, buffers, _) = gltf::import(path)?;
    
    let mesh = document.meshes().next().unwrap();
    let primitive = mesh.primitives().next().unwrap();
    
    let reader = primitive.reader(|buffer| Some(&buffers[buffer.index()]));
    
    let positions: Vec<[f32; 3]> = reader
        .read_positions()
        .unwrap()
        .collect();
    
    let normals: Vec<[f32; 3]> = reader
        .read_normals()
        .unwrap()
        .collect();
    
    let indices: Vec<u32> = reader
        .read_indices()
        .unwrap()
        .into_u32()
        .collect();
    
    Ok(Mesh {
        vertices: positions.into_iter().map(Vec3::from).collect(),
        normals: normals.into_iter().map(Vec3::from).collect(),
        indices,
    })
}
```

#### Instanced Rendering

Instead of drawing each instance separately:

```rust
// SLOW (1000 draw calls)
for instance in instances {
    render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
    render_pass.draw(...);
}
```

Draw all instances at once:

```rust
// FAST (1 draw call)
render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
render_pass.set_vertex_buffer(1, instance_buffer.slice(..));  // Instance data
render_pass.draw_indexed(0..index_count, 0, 0..instance_count);
```

Instance buffer contains per-instance data (transform matrix):

```rust
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct InstanceData {
    transform: [[f32; 4]; 4],  // 4x4 matrix
}

let instance_data: Vec<InstanceData> = scene.instances.iter()
    .map(|inst| InstanceData {
        transform: inst.transform.to_cols_array_2d(),
    })
    .collect();

let instance_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
    label: Some("Instance Buffer"),
    contents: bytemuck::cast_slice(&instance_data),
    usage: wgpu::BufferUsages::VERTEX,
});
```

Shader sees both vertex and instance data:

```wgsl
struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
}

struct InstanceInput {
    @location(2) transform_0: vec4<f32>,  // Matrix row 0
    @location(3) transform_1: vec4<f32>,  // Matrix row 1
    @location(4) transform_2: vec4<f32>,  // Matrix row 2
    @location(5) transform_3: vec4<f32>,  // Matrix row 3
}

@vertex
fn vs_main(vertex: VertexInput, instance: InstanceInput) -> VertexOutput {
    let transform = mat4x4<f32>(
        instance.transform_0,
        instance.transform_1,
        instance.transform_2,
        instance.transform_3,
    );
    
    let world_pos = transform * vec4<f32>(vertex.position, 1.0);
    // ...
}
```

### Common Pitfalls

**Pitfall 1: Arc::clone() vs .clone()**

```rust
let mesh = Arc::new(big_mesh);

let copy1 = Arc::clone(&mesh);  // GOOD: Cheap ref count increment
let copy2 = (*mesh).clone();    // BAD: Deep clones entire mesh!
```

**Pitfall 2: Instance buffer layout**

Matrix is 4x4, but shader locations are vec4. Must split matrix into 4 vec4s:

```rust
// In vertex buffer layout:
attributes: &[
    VertexAttribute { shader_location: 2, offset: 0, format: Float32x4 },
    VertexAttribute { shader_location: 3, offset: 16, format: Float32x4 },
    VertexAttribute { shader_location: 4, offset: 32, format: Float32x4 },
    VertexAttribute { shader_location: 5, offset: 48, format: Float32x4 },
]
```

**Pitfall 3: Performance testing**

1000 instances should render at 60 FPS easily. If not:

- Check you're using instanced rendering (1 draw call, not 1000)
- Enable release mode: `cargo run --release`
- Check GPU usage (should be low for simple cubes)

### Validation

**Success metrics:**

- Load cube.gltf: < 100ms
- Create 1000 instances: < 10ms
- Render at 60 FPS
- Memory usage: < 50 MB

**Visual validation:**

- 1000 cubes in 10×10×10 grid
- All cubes visible
- Can rotate camera around scene

---

## Milestone 5: Camera Controls

**Est:** 5-10 hours  
**Goal:** Orbit camera with mouse, WASD movement

### Overview

Make the viewport interactive. You'll implement a camera that can orbit around the scene and move with keyboard controls.

### What You'll Learn

- Camera matrices (view, projection)
- Mouse input handling
- Keyboard input
- Bind groups (passing uniforms to shaders)

### Prerequisites

- Milestone 4 complete (scene graph with instances)

### Tasks

- [ ] Create `Camera` struct (position, target, up, fov, aspect)
- [ ] Implement `Camera::view_matrix()`
- [ ] Implement `Camera::projection_matrix()`
- [ ] Create camera uniform buffer
- [ ] Update shader to use camera uniforms
- [ ] Handle mouse drag for orbit
- [ ] Handle WASD for movement
- [ ] Handle scroll wheel for zoom
- [ ] Smooth camera movement
- [ ] See scene from different angles!

### Key Concepts

#### Camera Math

```rust
pub struct Camera {
    position: Vec3,
    target: Vec3,
    up: Vec3,
    fov: f32,      // Field of view in degrees
    aspect: f32,   // Width / height
    near: f32,
    far: f32,
}

impl Camera {
    pub fn view_matrix(&self) -> Mat4 {
        Mat4::look_at_rh(self.position, self.target, self.up)
    }
    
    pub fn projection_matrix(&self) -> Mat4 {
        Mat4::perspective_rh(
            self.fov.to_radians(),
            self.aspect,
            self.near,
            self.far,
        )
    }
    
    pub fn view_proj_matrix(&self) -> Mat4 {
        self.projection_matrix() * self.view_matrix()
    }
}
```

Use `glam` for matrix math (already in dependencies).

#### Camera Uniforms

```rust
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct CameraUniform {
    view_proj: [[f32; 4]; 4],
}

let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
    label: Some("Camera Buffer"),
    contents: bytemuck::cast_slice(&[CameraUniform {
        view_proj: camera.view_proj_matrix().to_cols_array_2d(),
    }]),
    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
});
```

Update each frame:

```rust
queue.write_buffer(
    &camera_buffer,
    0,
    bytemuck::cast_slice(&[CameraUniform {
        view_proj: camera.view_proj_matrix().to_cols_array_2d(),
    }]),
);
```

#### Bind Group (Connect buffer to shader)

```rust
let camera_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
    label: Some("Camera Bind Group Layout"),
    entries: &[wgpu::BindGroupLayoutEntry {
        binding: 0,
        visibility: wgpu::ShaderStages::VERTEX,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }],
});

let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
    label: Some("Camera Bind Group"),
    layout: &camera_bind_group_layout,
    entries: &[wgpu::BindGroupEntry {
        binding: 0,
        resource: camera_buffer.as_entire_binding(),
    }],
});

// In render pass:
render_pass.set_bind_group(0, &camera_bind_group, &[]);
```

#### Shader Update

```wgsl
struct CameraUniform {
    view_proj: mat4x4<f32>,
}

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

@vertex
fn vs_main(vertex: VertexInput, instance: InstanceInput) -> VertexOutput {
    let world_pos = instance.transform * vec4<f32>(vertex.position, 1.0);
    let clip_pos = camera.view_proj * world_pos;
    
    var out: VertexOutput;
    out.clip_position = clip_pos;
    return out;
}
```

#### Mouse Orbit

```rust
struct CameraController {
    last_mouse_pos: Option<(f32, f32)>,
    is_dragging: bool,
}

impl CameraController {
    pub fn handle_mouse_input(&mut self, camera: &mut Camera, event: &WindowEvent) {
        match event {
            WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => {
                self.is_dragging = *state == ElementState::Pressed;
            }
            WindowEvent::CursorMoved { position, .. } => {
                if self.is_dragging {
                    if let Some((last_x, last_y)) = self.last_mouse_pos {
                        let dx = position.x as f32 - last_x;
                        let dy = position.y as f32 - last_y;
                        
                        // Orbit around target
                        self.orbit_camera(camera, dx * 0.01, dy * 0.01);
                    }
                }
                self.last_mouse_pos = Some((position.x as f32, position.y as f32));
            }
            _ => {}
        }
    }
    
    fn orbit_camera(&self, camera: &mut Camera, yaw_delta: f32, pitch_delta: f32) {
        // Rotate camera around target
        // (Math details: convert to spherical coords, rotate, convert back)
    }
}
```

### Common Pitfalls

**Pitfall 1: Matrix multiplication order**

```rust
// WRONG:
let clip_pos = world_pos * camera.view_proj;

// RIGHT:
let clip_pos = camera.view_proj * world_pos;
```

Matrix multiplication is not commutative!

**Pitfall 2: Aspect ratio on resize**

```rust
WindowEvent::Resized(new_size) => {
    camera.aspect = new_size.width as f32 / new_size.height as f32;
    // Must recalculate projection matrix!
}
```

**Pitfall 3: Forgetting to update uniform buffer**

Update camera buffer EVERY frame in render loop, not just on input events.

### Validation

**Controls should work:**

- Left mouse drag → orbit camera
- WASD → move camera
- Scroll wheel → zoom in/out
- Camera never inverts or flips unexpectedly

---

## Milestone 6: CPU Path Tracer Port

**Est:** 30-40 hours  
**Goal:** Port your Go raytracer materials to Rust, render to image file

### Overview

Finally! Port the renderer you already understand. This milestone brings together everything: scene graph, math library, and your proven rendering algorithms.

### What You'll Learn

- Trait objects for materials
- Embree integration (FFI with C++)
- Parallel rendering with rayon
- Image output (PNG/EXR)

### Prerequisites

- Milestone 4 complete (scene graph works)
- Your Go raytracer code available

### Tasks

- [ ] Create `crates/bif_renderer` crate
- [ ] Define `Material` trait (scatter method)
- [ ] Port `Lambertian` material from Go
- [ ] Port `Metal` material from Go
- [ ] Port `Dielectric` material from Go
- [ ] Port `Emissive` material from Go
- [ ] Add `embree-sys` dependency
- [ ] Create Embree scene from BIF scene graph
- [ ] Implement ray-scene intersection via Embree
- [ ] Port IBL sampling from Go
- [ ] Port Next Event Estimation from Go
- [ ] Implement `trace_ray()` recursive function
- [ ] Implement `render()` function with rayon parallelism
- [ ] Render scene to PNG
- [ ] Compare output to Go renderer (should match!)

### Key Concepts

(This is a large milestone - I'll provide detailed concepts when you reach it)

**Material Trait:**

```rust
pub trait Material: Send + Sync {
    fn scatter(
        &self,
        ray: &Ray,
        hit: &HitRecord,
    ) -> Option<(Color, Ray)>;
    
    fn emitted(&self, u: f32, v: f32, p: Vec3) -> Color {
        Color::BLACK
    }
}
```

**Embree Integration:**

```rust
// Create Embree device
let device = embree::Device::new();
let scene = device.create_scene();

// Add meshes
for prototype in scene.prototypes {
    let geometry = scene.create_triangle_mesh(...);
    geometry.commit();
}

// Add instances
for instance in scene.instances {
    scene.attach_instance(prototype_geom, &instance.transform);
}

scene.commit();

// Trace ray
let hit = scene.intersect(ray);
```

### Validation

- Rendered image matches Go output
- Can render 1000+ instances
- Render time competitive with Go
- IBL looks correct

---

## Milestone 7: egui UI Integration

**Est:** 2-3 hours  
**Goal:** Add immediate-mode UI overlay with scene stats

**Status:** ✅ **COMPLETE** (December 27, 2025)

### Overview

Add egui for development UI - side panel with FPS, camera stats, mesh info, and controls. Pure Rust, fast iteration, perfect for prototyping.

### What You'll Learn

- Immediate-mode UI paradigm
- egui-wgpu integration
- Two-pass rendering (3D + UI overlay)
- Event consumption patterns

### Prerequisites

- Milestone 6 complete (depth testing)
- Understanding of wgpu render passes

### Tasks

- [x] Add egui dependencies (egui, egui-wgpu, egui-winit)
- [x] Initialize egui state in Renderer
- [x] Handle egui events before viewport
- [x] Create side panel with collapsible sections
- [x] Add FPS counter (updates every 0.5s)
- [x] Display camera stats (position, angles, FOV, planes)
- [x] Display mesh info (vertices, indices, bounds)
- [x] Two-pass rendering (3D scene, then UI overlay)

### Validation

- UI panel appears on left side
- FPS counter updates smoothly
- Stats reflect actual values
- UI doesn't block 3D viewport interaction

---

## Milestone 8: GPU Instancing

**Est:** 2-3 hours  
**Goal:** Replace dual-buffer hack with proper GPU instancing

**Status:** ✅ **COMPLETE** (December 27, 2025)

### Overview

Implement proper per-instance transforms using vertex attributes. Render 100 Lucy models with a single draw call. Proves scalability for 10K+ instances.

### What You'll Learn

- GPU instancing with wgpu
- Mat4 as vertex attributes (4 vec4s)
- Instance step mode vs vertex step mode
- Performance optimization

### Prerequisites

- Milestone 7 complete (egui)
- Understanding of vertex buffers

### Tasks

- [x] Remove temporary secondary buffers
- [x] Create InstanceData struct with model matrix
- [x] Update pipeline with instance vertex buffer
- [x] Generate 100 instances in 10x10 grid
- [x] Update shader to use per-instance transforms
- [x] Single draw_indexed call with instance count
- [x] Extend far clipping plane for distant instances

### Validation

- 100 Lucy models render correctly
- FPS remains at 60+ (VSync-limited)
- Single draw call in render pass
- Memory usage much lower than duplication

---

## Milestone 9: USD Import

**Est:** 4-6 hours  
**Goal:** Load USD scenes and render in viewport

**Status:** 🎯 **NEXT MILESTONE**

### Overview

Import production USD scenes instead of hardcoded meshes. Start with USDA (text format) parser to avoid C++ dependencies. This validates the architecture's USD compatibility early.

### What You'll Learn

- USD file format (.usda text)
- Scene graph structures
- UsdGeomMesh and UsdGeomPointInstancer
- Transform hierarchies

### Prerequisites

- Milestone 8 complete (instancing ready)
- Understanding of scene graphs

### Tasks

- [ ] Research USD format (start with USDA text)
- [ ] Create USD parser in bif_core
- [ ] Parse UsdGeomMesh primitives
- [ ] Parse UsdGeomPointInstancer for instances
- [ ] Extract vertex data and transform matrices
- [ ] Create test.usda file (1 mesh + 100 instances)
- [ ] Load USD in viewer instead of hardcoded OBJ
- [ ] Validate against usdview

### Validation

- BIF viewport matches usdview
- Instance count correct
- Transforms match visually
- Can load different USD files

---

## Milestone 10: CPU Path Tracer

**Est:** 15-20 hours  
**Goal:** Port Go raytracer for production rendering

### Overview

Create `bif_renderer` crate and port the proven Go implementation. This proves the dual rendering architecture: GPU viewport (fast preview) + CPU path tracer (production quality).

### What You'll Learn

- Path tracing algorithms
- BVH acceleration structures
- Material systems (PBR)
- Multi-threaded rendering

### Prerequisites

- Milestone 9 complete (USD scenes)
- Go raytracer for reference

### Tasks

- [ ] Create bif_renderer crate
- [ ] Port Hittable trait (sphere, triangle, mesh)
- [ ] Port Material trait (Lambert, Metal, Dielectric)
- [ ] Port BVH acceleration structure
- [ ] Implement ray casting and shading
- [ ] Port HDRI environment loading
- [ ] Multi-threaded bucket renderer
- [ ] Add "Render" button to egui UI
- [ ] Display progress bar during render
- [ ] Show result in viewport

### Validation

- Rendered image matches Go output
- Can render 1000+ instances
- Render time competitive with Go
- IBL looks correct

---

## Milestone 11: Qt Integration (Deferred to Phase 2)

**Est:** 30-50 hours  
**Goal:** Embed wgpu viewport in Qt window with production UI

### Overview

The big integration. Qt C++ ↔ Rust via cxx-qt. Deferred until after USD import and raytracer port to validate core architecture first. egui is sufficient for prototyping.

### What You'll Learn

- C++ FFI
- Qt C++ (similar to PyQt)
- cxx-qt bridging
- Cross-language debugging

### Prerequisites

- Milestones 1-10 complete
- Qt 6 installed
- Patience (FFI is hard)

### Tasks

*Detailed task list will be provided when you reach this milestone*

---

## Debugging Strategy

When stuck, follow this process:

### 1. Isolate the Problem

- Can you reproduce in a minimal example?
- What's the smallest code that shows the issue?

### 2. Check Common Issues

- Compilation error → Read error message carefully, check types
- Runtime panic → Check the backtrace (`RUST_BACKTRACE=1 cargo run`)
- Wrong output → Add `println!` debugging
- Performance issue → Profile with `cargo flamegraph`

### 3. Ask Good Questions

When asking for help, provide:

- What you're trying to do
- What you expected
- What actually happened
- Minimal code example
- Error messages (full text)

### 4. Use The Compiler

Rust compiler errors are detailed:

```
error[E0382]: borrow of moved value: `mesh`
  --> src/main.rs:10:5
   |
8  |     let m1 = mesh;
   |              ---- value moved here
9  |     let m2 = mesh;
   |              ^^^^ value borrowed here after move
```

Read the WHOLE message - it tells you exactly what's wrong.

---

## Resources

**Rust Learning:**

- [The Rust Book](https://doc.rust-lang.org/book/)
- [Rust By Example](https://doc.rust-lang.org/rust-by-example/)
- [Easy Rust](https://dhghomon.github.io/easy_rust/) (very beginner-friendly)

**wgpu:**

- [Learn wgpu](https://sotrh.github.io/learn-wgpu/) (excellent tutorial)
- [wgpu examples](https://github.com/gfx-rs/wgpu/tree/master/examples)

**USD:**

- [USD Tutorials](https://graphics.pixar.com/usd/docs/index.html)
- usdview (bundled with USD)

**When You Get Stuck:**

- Rust Discord
- /r/rust
- Rust Users Forum
- Or just come back and ask me!

---

**Good luck! Take your time, understand every line you write, and enjoy the journey.**
