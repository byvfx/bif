---
title: "wgpu"
type: concept
tags: [graphics, gpu, rust, rendering]
created: "2026-04-05"
updated: "2026-04-05"
---

## Summary

wgpu is a Rust implementation of the WebGPU API that runs natively on Vulkan, Metal, DX12, and OpenGL backends (as well as in browsers via WebGPU). It provides a safe, modern GPU abstraction for rendering and compute without the complexity of raw Vulkan or the platform lock-in of Metal/DX12. For Rust graphics projects, it is the de facto standard GPU API.

## Details

### Architecture

- **Instance** -> **Adapter** -> **Device** + **Queue**: the initialization chain.
- **Device**: Logical GPU handle for creating resources.
- **Queue**: Submits command buffers for execution.
- **Surface**: Swapchain abstraction for presenting to a window.

### Key Resources

| Resource | Purpose |
|----------|---------|
| `Buffer` | Vertex, index, uniform, storage data |
| `Texture` | Images, render targets, depth buffers |
| `Sampler` | Texture filtering/wrapping config |
| `BindGroup` | Bundle of resources bound to a shader |
| `RenderPipeline` | Vertex + fragment shader + render state |
| `ComputePipeline` | Compute shader dispatch |
| `CommandEncoder` | Records GPU commands before submission |

### Shader Language

wgpu uses **WGSL** (WebGPU Shading Language) natively but also accepts SPIR-V and GLSL via naga (its shader translation layer). Naga compiles shaders to the backend's native format.

### Safety Model

wgpu validates all API usage at runtime (in debug mode) and ensures no undefined behavior. This is a significant advantage over raw Vulkan where validation is opt-in and UB is easy to trigger.

### vs Other Options

| Option | Pros | Cons |
|--------|------|------|
| wgpu | Safe, cross-platform, Rust-native | Slightly higher overhead than raw APIs |
| vulkano | Thin Vulkan wrapper | Vulkan-only, more complexity |
| ash | Raw Vulkan bindings | Maximum control, maximum footguns |
| gfx-hal | Low-level abstraction | Deprecated in favor of wgpu |

## In BIF

wgpu is the GPU backend for BIF's viewport rendering:

- **Viewport** (`bif_viewport`): Uses wgpu for real-time mesh display, selection outlines, grid rendering, and HDRI environment previews.
- **Viewer** (`bif_viewer`): The application window integrates wgpu with egui via `egui-wgpu`.
- **CPU raytracer**: The `bif_renderer` crate does *not* use wgpu -- it's a CPU path tracer. wgpu handles only the interactive viewport.
- **Vertex displacement**: CPU vertex displacement results are uploaded to wgpu buffers for display.
- **Dependencies**: Listed in `Cargo.toml` for `bif_viewport` and `bif_viewer`.

## Related

- [[egui-snarl]] -- the node graph UI rendered alongside wgpu viewport
- [[bvh]] -- CPU-side acceleration (wgpu handles rasterization, not ray tracing)
- [[openpbr]] -- material preview in viewport uses simplified wgpu shaders
