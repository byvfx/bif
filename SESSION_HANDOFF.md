# Session Handoff - December 30, 2025

**Last Updated:** End of Milestone 9  
**Next Session Starts:** Milestone 10 (CPU Path Tracer)  
**Project:** BIF - VFX Scene Assembler & Renderer

---

## Quick Status

✅ **Milestones Complete:** 9/10 (90%)  
🎯 **Current State:** USDA import working, Lucy model renders with smooth shading  
📦 **Tests Passing:** 26/26 + USD tests  
🚀 **Next Goal:** CPU path tracer (port Go raytracer to Rust)

---

## Project Overview

**BIF** is a VFX production renderer inspired by Isotropix Clarisse, focused on:

- Massive instancing (10K-1M instances)
- USD-compatible workflows
- Dual rendering: GPU viewport (real-time) + CPU path tracer (production)
- Non-destructive layer-based editing

### Current Phase

**Phase 1 Foundation** - Building core architecture and proving GPU instancing at scale.

**Architecture Decision:** Pivoting milestone order to validate USD compatibility early:
- ~~Milestone 8: Qt Integration~~ → Deferred to Phase 2
- ~~Milestone 9: USD Import~~ → ✅ Complete (USDA parser, mesh loading, viewport integration)
- **Milestone 10: CPU Path Tracer** - Port Go raytracer for production rendering

---

## Completed Milestones

### ✅ Milestone 0: Environment Setup

- Cargo workspace with 4 crates
- Git repository with LFS for large files
- Go raytracer preserved in `legacy/go-raytracing/`

### ✅ Milestone 1: Math Library

**Location:** `crates/bif_math/src/`

Ported from Go implementation:

- `Ray` - Ray with origin, direction, time (6 tests)
- `Interval` - Min/max range operations (10 tests)
- `Aabb` - Axis-aligned bounding box with hit testing (6 tests)
- `Camera` - 3D camera with view-projection matrices (4 tests)

**Stats:** 26 tests passing, ~400 LOC

### ✅ Milestone 2: wgpu Window

**Location:** `crates/bif_viewport/src/lib.rs`

- Vulkan backend (auto-selected on Windows)
- Surface configuration with VSync
- Dark blue clear color (0.1, 0.2, 0.3)
- Window resize handling
- Error recovery for surface loss

**Stats:** ~250 LOC, 1 hour

### ✅ Milestone 3: Triangle + Camera

**Location:**

- `crates/bif_viewport/src/shaders/basic.wgsl`
- `crates/bif_math/src/camera.rs`

**Rendering Pipeline:**

- WGSL vertex/fragment shaders
- Vertex buffer with position + color attributes
- Uniform buffer for camera matrices
- Bind group for GPU data transfer

**Camera System:**

- Perspective projection (45° FOV)
- Look-at view matrix (right-handed)
- Automatic aspect ratio updates on resize
- Position: (0, 0, 3), Target: (0, 0, 0)

**Visual Output:**

- RGB triangle with smooth color interpolation
- 60 FPS with VSync enabled
- Proper 3D perspective

**Post-Milestone:** Renamed `bif_render` → `bif_viewport` to clarify it's the GPU preview, not the production renderer.

**Stats:** ~200 LOC, 1.5 hours, 4 new tests

---

### ✅ Milestone 4: Camera Controls

**Location:**

- `crates/bif_math/src/camera.rs` - Camera control methods
- `crates/bif_viewer/src/main.rs` - Input event handling

**Camera Control System:**

- **Mouse Orbit:** Left-click drag to rotate around target
  - Yaw (horizontal) and pitch (vertical) angles
  - Spherical coordinates maintain distance from target
  - Pitch clamped to prevent gimbal lock
  
- **Keyboard Movement:** WASD + QE for 6DOF movement
  - W/S: Forward/backward along view direction
  - A/D: Strafe left/right
  - Q/E: Move up/down
  - Smooth velocity-based movement with delta time
  
- **Camera Update Pipeline:**
  - Input events modify camera state
  - `update_camera()` writes new matrices to GPU
  - Changes reflected in next frame

**Controls Philosophy:**

> **Goal:** Emulate Houdini viewport controls (tumble/track/dolly paradigm)
> - Tumble (current): Left-click orbit around target
> - Track (future): Middle-click pan camera and target together
> - Dolly (future): Scroll wheel zoom in/out from target

**Stats:** ~150 LOC, 1 hour

---

### ✅ Milestone 5: OBJ Mesh Loading

**Location:**
- `crates/bif_viewport/src/lib.rs` - MeshData struct and OBJ loading
- `crates/bif_viewport/Cargo.toml` - Added tobj dependency

**OBJ Loading System:**

- **MeshData struct:** Vertices, indices, AABB bounds
- **tobj integration:** Robust OBJ file parsing
- **Normal computation:** Per-face normals for models lacking them
  - Cross product of edges for face normal
  - Accumulate at vertices and normalize for smooth shading
- **Lucy model:** 140,278 vertices, 840,768 indices loaded successfully

**Auto-Framing:**

- F key to frame mesh in viewport
- Dynamic near/far planes based on mesh size
- `camera.near = distance * 0.01`
- `camera.far = distance * 10.0`

**Stats:** ~120 LOC, 2 hours

---

### ✅ Milestone 6: Depth Testing + Enhanced Controls

**Location:**
- `crates/bif_viewport/src/lib.rs` - Depth texture creation
- `crates/bif_viewer/src/main.rs` - Mouse wheel and middle mouse handling
- `crates/bif_math/src/camera.rs` - Distance-scaled movement

**Depth Testing:**

- **Format:** Depth24Plus with Less comparison
- **Testing:** Second Lucy instance at +500 Z offset
- **Result:** Proper occlusion between instances
- **Note:** Secondary buffers temporary - will use GPU instancing later

**Enhanced Camera Controls:**

- **Mouse Scroll:** Dolly (zoom in/out) with wheel
- **Middle Mouse:** Pan/track with click-drag
- **Distance Scaling:** All movement scaled by camera distance
  - WASD: `move_speed * distance * delta_time`
  - Middle pan: `sensitivity * distance * 0.0001`
- **Continuous Redraw:** `about_to_wait` handler for held keys

**Complete Houdini Paradigm:**
- Tumble (left mouse orbit) ✅
- Track (middle mouse pan) ✅
- Dolly (scroll wheel zoom) ✅

**Stats:** ~140 LOC, 2 hours

---

### ✅ Milestone 7: egui UI Integration

**Location:**
- `crates/bif_viewport/src/lib.rs` - egui state and rendering
- `crates/bif_viewport/Cargo.toml` - egui dependencies
- `crates/bif_viewer/src/main.rs` - egui event handling

**egui Integration:**

- **Dependencies:** egui 0.29, egui-wgpu 0.29, egui-winit 0.29
- **UI Architecture:** Immediate-mode side panel (300px)
- **Two-pass rendering:**
  1. 3D scene with depth buffer
  2. egui overlay without depth (uses `.forget_lifetime()` for wgpu compatibility)

**Side Panel Features:**

- **FPS Counter:** Real-time at 60+, updates every 0.5s
- **Camera Stats:** Position, target, distance, yaw, pitch, FOV, near/far
- **Mesh Info:** Vertices, indices, instances, bounds, center, size
- **Viewport Info:** Resolution, aspect ratio
- **Controls Help:** Mouse/keyboard shortcuts

**Challenges Solved:**

- egui initialization requires specific parameter counts (6 for State, 5 for Renderer)
- Borrow checker: Extract UI data before `egui_ctx.run()` closure
- Lifetime issues: Use `.forget_lifetime()` on RenderPass for egui's `'static` requirement
- Event consumption: UI processes events first to prevent click-through

**Stats:** ~100 LOC, 1.5 hours

---

### ✅ Milestone 8: GPU Instancing

**Location:**
- `crates/bif_viewport/src/lib.rs` - InstanceData struct, instancing logic
- `crates/bif_viewport/src/shaders/basic.wgsl` - Per-instance transforms

**Replaced:** Temporary dual-buffer hack with proper GPU instancing

**InstanceData Design:**

```rust
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct InstanceData {
    pub model_matrix: [[f32; 4]; 4],  // 4 vec4 attributes (locations 3-6)
}
```

**Shader Update:**

- Reconstruct model matrix from 4 vec4 inputs
- Transform vertex position: `model * position`
- Transform normals: `model * normal` (assumes uniform scale)

**Instance Generation:**

- 100 Lucy models in 10x10 grid
- Spacing: 1.5x mesh size
- Single draw call: `draw_indexed(0..indices, 0, 0..100)`

**Performance:**

- **Before:** 2 instances, 2 draw calls, 504MB of duplicate geometry
- **After:** 100 instances, 1 draw call, 6.4KB instance buffer
- **FPS:** 60+ (VSync-limited), 28M triangles rendered
- **Memory Saved:** ~504MB

**Camera Adjustments:**

- Far plane: 10x → 20x distance (better view of distant instances)
- Near plane: 0.01x distance (unchanged)
- Spacing: 2x → 1.5x mesh size (tighter grid)

**Stats:** ~150 LOC, 1 hour

---

### ✅ Milestone 9: USD Import

**Location:**
- `crates/bif_core/src/usd/` - USDA parser module
- `crates/bif_core/src/mesh.rs` - Mesh data with USD loading
- `crates/bif_core/src/scene.rs` - Scene graph structure

**USDA Parser:**

- **Pure Rust:** No C++ USD dependencies
- **Supported Types:** UsdGeomMesh, UsdGeomPointInstancer, Xform
- **Mesh Loading:** positions, normals (optional), faceVertexCounts, faceVertexIndices
- **Triangulation:** Converts N-gon faces to triangles via fan triangulation

**CLI Integration:**

```bash
cargo run -p bif_viewer -- --usda assets/lucy_low.usda
```

**Viewport Changes:**

- `new_with_scene()` - Accept pre-loaded mesh data
- `FrontFace::Cw` - Fixed winding order for Houdini/USD compatibility
- Gnomon axis indicator in corner

**Documentation:**

- `HOUDINI_EXPORT.md` - Best practices for Houdini USD export
- Key insight: Use **point normals**, not vertex normals

**Stats:** ~1,500 LOC, ~4 hours, 15+ tests

---

## Crate Architecture

```
bif/
├── crates/
│   ├── bif_math/       # Math primitives (Vec3, Ray, Interval, Aabb, Camera)
│   ├── bif_core/       # Scene graph (placeholder for now)
│   ├── bif_viewport/   # Real-time GPU viewport (wgpu + Vulkan)
│   └── bif_viewer/     # Application entry point (winit event loop)
├── legacy/
│   └── go-raytracing/  # Original Go implementation (reference)
└── devlog/             # Session development logs
```

### Key Design Decisions

**1. Viewport vs Renderer Distinction**

- **`bif_viewport`** = Real-time GPU (Vulkan/DX12/Metal via wgpu)
  - Purpose: Interactive preview for artists
  - Speed: 60+ FPS
  - Quality: Good enough for scene composition
  
- **`bif_renderer`** = CPU path tracer (future)
  - Purpose: Production-quality final renders
  - Speed: Minutes per frame
  - Quality: Physically accurate, all features

This matches Clarisse, Houdini, Maya architecture.

**2. Math Library Strategy**

- Use `glam` for SIMD Vec3 operations
- Wrap with custom types (Ray, Interval, Aabb, Camera)
- Port algorithms from proven Go implementation

**3. UI Strategy**

- **Now:** egui for prototyping (pure Rust, fast iteration)
- **Later:** Qt 6 for production (industry standard, docking, shortcuts)

---

## Technical Context

### GPU Backend

- **Current:** Vulkan (Windows) via wgpu
- **Alternatives:** DX12 (Windows fallback), Metal (macOS), WebGPU (web)
- **Selection:** Automatic via `wgpu::Backends::PRIMARY`

### Notable: Vulkan Layer Warning

```
[INFO] Unable to find layer: VK_LAYER_ROCKSTAR_GAMES_social_club
```

**Explanation:** Harmless - Rockstar Games left a Vulkan layer registration in registry. App works perfectly, just verbose logging. Can reduce to `LevelFilter::Warn` if annoying.

### Dependencies

- **glam** 0.29 - SIMD math library
- **wgpu** 22.1 - GPU abstraction (Vulkan/DX12/Metal)
- **winit** 0.30 - Window management
- **bytemuck** 1.24 - Zero-copy GPU buffer casting
- **anyhow** 1.0 - Error handling
- **tobj** 4.0 - OBJ file parser
- **pollster** 0.3 - Async runtime for wgpu init

### Build Configuration

```toml
[profile.dev]
opt-level = 1  # Faster dev builds with some optimization
```

---

## Statistics

| Metric | Value |
|--------|-------|
| **Total LOC** | ~3,600 |
| **Tests Passing** | 40+ ✅ |
| **Commits** | 30+ |
| **Time Invested** | ~20.5 hours |
| **Milestones Complete** | 9/10 (90%) |
| **Build Time (dev)** | ~5s |
| **Build Time (release)** | ~2m |
| **Runtime FPS** | 60+ (VSync) |
| **Lucy Vertices** | 140,278 |
| **Lucy Indices** | 840,768 |
| **Instances Rendered** | 100 |
| **Total Triangles** | 28,025,600 |
| **Draw Calls** | 1 (instanced) |

---

## Next Session: Milestone 10

### 🎯 CPU Path Tracer Port

**Rationale:** Port the proven Go raytracer to Rust for production-quality renders. This enables the dual-renderer architecture (GPU viewport for preview + CPU path tracer for final).

**Implementation Plan:**

1. **Create `bif_renderer` Crate**
   - New crate in workspace for CPU rendering
   - Separate from viewport (different purpose)

2. **Port Core Types from Go**
   - `Hittable` trait (sphere, triangle, mesh)
   - `Material` trait (Lambert, Metal, Dielectric, Emissive)
   - `BVH` acceleration structure for ray-mesh intersection

3. **Port Rendering Pipeline**
   - Ray casting and path tracing
   - Multi-threaded bucket renderer (rayon)
   - Color accumulation and tone mapping
   - HDRI environment loading (optional)

4. **Integration**
   - "Render" button in egui UI
   - Progress bar during render
   - Display result in viewport or save to file

**Files to Create:**

- `crates/bif_renderer/Cargo.toml`
- `crates/bif_renderer/src/lib.rs`
- `crates/bif_renderer/src/ray.rs` - Ray type (or reuse bif_math)
- `crates/bif_renderer/src/hittable.rs` - Hittable trait and implementations
- `crates/bif_renderer/src/material.rs` - Material trait and types
- `crates/bif_renderer/src/bvh.rs` - Bounding volume hierarchy
- `crates/bif_renderer/src/renderer.rs` - Path tracer core

**Reference:** `legacy/go-raytracing/rt/` - Proven Go implementation

**Estimated Time:** 6-8 hours (depends on parsing complexity)

---

## After Path Tracer: Phase 2

### Qt Integration & Advanced Features

After the CPU path tracer is working:

1. **Qt 6 UI** - Production-grade interface with docking
2. **USD References** - `references = @path@</prim>` for asset reuse
3. **Materials** - UsdShade support
4. **Layers** - Non-destructive scene composition

---

## Important Files for Next Session

### Must Read

1. **This file** (`SESSION_HANDOFF.md`) - Current status
2. **`CLAUDE.md`** - Your custom AI instructions
3. **`ARCHITECTURE.md`** - System design and principles
4. **`devlog/DEVLOG_2025-12-30_milestone9.md`** - Latest session log
5. **`HOUDINI_EXPORT.md`** - USD export best practices

### Reference (Can Use #codebase)

- `crates/bif_math/src/camera.rs` - Complete camera implementation
- `crates/bif_viewport/src/lib.rs` - Renderer with instancing
- `crates/bif_core/src/usd/` - USDA parser implementation
- `legacy/go-raytracing/rt/` - Reference for path tracer port

### Don't Need to Read

- Cargo.toml files (standard structure)
- Test files (unless debugging)
- Legacy Go code (only for algorithm reference when porting)

---

## Quick Commands

### Build & Run

```bash
cargo build                    # Dev build (opt-level=1)
cargo build --release          # Release build
cargo test                     # All tests (26 tests)
cargo run --package bif_viewer # Run application
```

### Git Workflow

```bash
git status
git add .
git commit -m "feat: description"
git push origin main
```

### Debugging

```bash
# Set log level to reduce noise
RUST_LOG=warn cargo run

# Check tests
cargo test --package bif_math -- --nocapture
```

---

## Known Issues / Quirks

### Non-Issues (Ignore These)

- **Rockstar Vulkan layer warning** - Harmless, from old game install
- **Verbose wgpu logs** - Normal for development, shows frame submissions
- **CRLF warnings** - Windows line endings, git handles automatically

### Actual Issues

None currently! Everything working as expected. 🎉

---

## Token-Saving Tips for Next Session

### Use These Patterns

✅ **Attach files:** Use `#file:SESSION_HANDOFF.md` syntax  
✅ **Use codebase:** Claude can read with `#codebase`  
✅ **Reference devlogs:** Point to specific milestone logs  
✅ **Assume Rust knowledge:** Don't need to explain Copy/Clone/&self anymore  

### Avoid These

❌ Re-explaining project goals (it's in ARCHITECTURE.md)  
❌ Asking about crate structure (it's documented here)  
❌ Questioning basic Rust concepts (you've internalized them)  
❌ Long code reviews of working features  

---

## Session Start Prompt Template

```
I'm continuing work on BIF (VFX renderer in Rust).

#file:SESSION_HANDOFF.md
#file:CLAUDE.md
#file:devlog/DEVLOG_2025-12-30_milestone9.md
#codebase

Status: Just completed Milestone 9:

✅ USDA parser (pure Rust, no C++ deps)
✅ UsdGeomMesh loading with positions, normals, indices
✅ CLI --usda argument for loading USD files
✅ Fixed winding order (FrontFace::Cw) for Houdini compatibility
✅ Gnomon axis indicator in viewport corner
✅ Lucy model renders correctly with smooth shading
✅ HOUDINI_EXPORT.md documentation

Current state: USDA import working, 100 GPU-instanced Lucy models at 60 FPS

Ready to start Milestone 10: CPU Path Tracer (6-8 hours estimated)
Goal: Port Go raytracer to Rust - Hittable trait, Materials, BVH, multi-threaded bucket renderer

Let's begin by creating the bif_renderer crate and porting the core ray types.
```

---

## Final Checklist

- ✅ All code committed
- ✅ All tests passing (26/26)
- ✅ Documentation updated
- ✅ Devlogs complete
- ✅ Handoff document created
- ✅ Next milestone defined

**You're ready for the next session!** 🚀

---

**Last Commit:** `Fix winding order for USD/Houdini compatibility`  
**Branch:** `main`  
**Build Status:** ✅ Successful  
**Test Status:** ✅ All passing
