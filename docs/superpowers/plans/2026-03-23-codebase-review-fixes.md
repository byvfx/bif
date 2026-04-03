# Codebase Review Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix all findings from the 2026-03-23 triple code review (code, VFX, architecture) — 8 critical, ~21 important, ~18 nice-to-have.

**Architecture:** Changes are organized into 7 independent phases, each producing a clean commit. Phases 1-4 are localized fixes within existing files. Phases 5-7 involve structural refactors. Each phase can be executed by a parallel agent.

**Tech Stack:** Rust, wgpu, Embree 3, OpenPBR, USD C++ FFI

---

## Phase 1: Critical Renderer Correctness

**Commit message:** `fix: renderer correctness — energy conservation, shadow bias, light units, NaN guards`

### Task 1.1: OpenPBR Diffuse Energy Conservation

**Files:**

- Modify: `crates/bif_renderer/src/openpbr.rs:474` (bsdf)
- Modify: `crates/bif_renderer/src/openpbr.rs:646` (scatter_diffuse_textured)

- [ ] **Step 1: Fix `bsdf()` — multiply diffuse by `(1 - F_specular)`**

In `bsdf()` at line 474, the diffuse term must be attenuated by the specular Fresnel to conserve energy:

```rust
// Before:
let diffuse = base_color * fd * (1.0 - metalness) / PI;

// After:
let f0_scalar = ior_to_f0(self.specular_ior) * self.specular_weight;
let fresnel_out = schlick_weight(n_dot_v) * f0_scalar + (1.0 - schlick_weight(n_dot_v)) * f0_scalar;
// Simplified: use the actual Fresnel at n_dot_l for energy conservation
let f_diffuse_atten = 1.0 - schlick_scalar(f0_scalar, n_dot_l);
let diffuse = base_color * fd * (1.0 - metalness) * f_diffuse_atten / PI;
```

Where `schlick_scalar` is: `f0 + (1.0 - f0) * (1.0 - cos_theta).powi(5)`.

- [ ] **Step 2: Fix `scatter_diffuse_textured()` — same attenuation**

At line 646, apply the same `(1 - F)` factor to the attenuation:

```rust
// Before:
let attenuation = base_color * diffuse / PI + fuzz;

// After:
let f0_scalar = ior_to_f0(self.specular_ior) * self.specular_weight;
let f_atten = 1.0 - schlick_scalar(f0_scalar, n_dot_l);
let attenuation = base_color * diffuse * f_atten / PI + fuzz;
```

- [ ] **Step 3: Add `schlick_scalar` helper if not already present**

```rust
fn schlick_scalar(f0: f32, cos_theta: f32) -> f32 {
    f0 + (1.0 - f0) * (1.0 - cos_theta).powi(5)
}
```

- [ ] **Step 4: Run tests** `cargo test -p bif_renderer`

### Task 1.2: Shadow Ray Shading Normal Offset

**Files:**

- Modify: `crates/bif_renderer/src/renderer.rs:203`

- [ ] **Step 1: Use shading normal for shadow ray origin**

```rust
// Before:
let shadow_origin = rec.p + rec.normal * 0.001;

// After:
let shading_n = rec.material.shading_normal(&rec);
let shadow_origin = rec.p + shading_n * 0.001;
```

- [ ] **Step 2: Run tests** `cargo test -p bif_renderer`

### Task 1.3: Distant Light Degrees-to-Radians

**Files:**

- Modify: `crates/bif_renderer/src/light.rs:58` (constructor)
- Modify: `crates/bif_renderer/src/light.rs:67,77,90` (usage sites already use `self.angle` directly)

The `DistantLight` constructor receives `angle` in degrees from `scene.rs:452` ("Angular diameter in degrees"). The `sample()` and `pdf()` methods use `(1.0 - self.angle * 0.5).cos()` which treats the value as if it's a fraction, not degrees or radians. This formula is actually wrong for both units.

For a distant light with angular diameter `d` degrees, the correct `cos_max` is:

```text
cos_max = cos(d_radians / 2)
```

- [ ] **Step 1: Convert angle to radians in constructor and fix cos_max computation**

```rust
// In DistantLight::new():
Self {
    direction: direction.normalize(),
    color,
    intensity,
    angle: angle.to_radians(), // Store in radians
}

// In sample() and pdf():
let half_angle = self.angle * 0.5;
let cos_max = half_angle.cos();
```

- [ ] **Step 2: Update all test assertions if any reference the old angle behavior**
- [ ] **Step 3: Run tests** `cargo test -p bif_renderer`

### Task 1.4: Normal Matrix Zero-Scale Guard

**Files:**

- Modify: `crates/bif_renderer/src/embree.rs:438-441`

- [ ] **Step 1: Add determinant check before inverse**

```rust
let normal_matrices: Vec<Mat3> = transforms
    .iter()
    .map(|t| {
        let m = Mat3::from_mat4(*t);
        let det = m.determinant();
        if det.abs() < 1e-10 {
            Mat3::IDENTITY
        } else {
            m.inverse().transpose()
        }
    })
    .collect();
```

- [ ] **Step 2: Run tests** `cargo test -p bif_renderer`

### Task 1.5: SHARC NaN Guard After Cache Lookup

**Files:**

- Modify: `crates/bif_renderer/src/renderer.rs:187-189`

- [ ] **Step 1: Add finite check on cached value**

```rust
if let Some(cached) = c.lookup(rec.p, rec.normal) {
    if cached.x.is_finite() && cached.y.is_finite() && cached.z.is_finite() {
        accumulated += throughput * cached;
    }
    break;
}
```

- [ ] **Step 2: Run tests** `cargo test -p bif_renderer`

### Task 1.6: Point Light Epsilon Fix

**Files:**

- Modify: `crates/bif_renderer/src/light.rs:156`

- [ ] **Step 1: Replace additive epsilon with `max()`**

```rust
// Before:
let falloff = 1.0 / (distance * distance + 0.01);

// After:
let falloff = 1.0 / (distance * distance).max(0.001);
```

- [ ] **Step 2: Run tests** `cargo test -p bif_renderer`

---

## Phase 2: Safety & Robustness

**Commit message:** `fix: safety and robustness — Drop ordering, NaN guards, validation, nightly compat`

### Task 2.1: Embree Drop Safety Comment

**Files:**

- Modify: `crates/bif_renderer/src/embree.rs:1130-1155`

- [ ] **Step 1: Add field-order invariant comment**

```rust
impl Drop for EmbreeScene {
    fn drop(&mut self) {
        log::debug!(
            "Releasing Embree scene: {} instances, {} triangles",
            self.instance_count,
            self.triangle_count
        );
        // SAFETY: Release order matters. Top-level scene references prototype_scene
        // via Embree instances, so release top-level first. Device must be last.
        // After drop() returns, Rust drops remaining fields in declaration order.
        // The _vertex_data, _index_data, _transform_data fields MUST be declared
        // AFTER device/scene/prototype_scene so they outlive the Embree pointers.
        // Reordering struct fields will cause use-after-free.
        unsafe {
            rtcReleaseScene(self.scene);
            rtcReleaseScene(self.prototype_scene);
            rtcReleaseDevice(self.device);
        }
    }
}
```

### Task 2.2: Node Graph Unwrap to Let-Else

**Files:**

- Modify: `crates/bif_viewport/src/node_graph/viewer.rs:639-640`

- [ ] **Step 1: Replace unwraps with let-else**

```rust
// Before:
let points_source = points_node.unwrap();
let proto_source = proto_node.unwrap();

// After:
let (Some(points_source), Some(proto_source)) = (points_node, proto_node) else {
    return;
};
```

### Task 2.3: Crease Data Validation

**Files:**

- Modify: `crates/bif_renderer/src/embree.rs:620`

- [ ] **Step 1: Validate crease data before passing to Embree**

```rust
if !sd.crease_indices.is_empty() && !sd.crease_sharpnesses.is_empty() {
    if sd.crease_indices.len() % 2 != 0 {
        log::warn!("Odd crease index count ({}), skipping creases", sd.crease_indices.len());
    } else if sd.crease_sharpnesses.len() != sd.crease_indices.len() / 2 {
        log::warn!(
            "Crease sharpness count ({}) != edge count ({}), skipping creases",
            sd.crease_sharpnesses.len(),
            sd.crease_indices.len() / 2
        );
    } else {
        // existing crease buffer setup code
    }
}
```

### Task 2.4: NaN Guard — HDR direction_to_uv

**Files:**

- Modify: `crates/bif_core/src/hdr.rs:123-124`

- [ ] **Step 1: Guard against zero-length direction**

```rust
pub fn direction_to_uv(dir: [f32; 3], rotation: f32) -> (f32, f32) {
    let len = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
    if len < 1e-10 {
        return (0.5, 0.5);
    }
    // ... rest unchanged
```

### Task 2.5: NaN Guard — Texture Sample

**Files:**

- Modify: `crates/bif_core/src/texture.rs:187-188`

- [ ] **Step 1: Guard against NaN UV inputs**

```rust
pub fn sample(&self, u: f32, v: f32) -> Vec3 {
    if self.width == 0 || self.height == 0 {
        return Vec3::new(1.0, 0.0, 1.0);
    }
    if !u.is_finite() || !v.is_finite() {
        return Vec3::new(1.0, 0.0, 1.0); // Magenta debug color
    }
    // ... rest unchanged
```

### Task 2.6: UsdBridgeError from(Success) — Safe Fallback

**Files:**

- Modify: `crates/bif_core/src/usd/cpp_bridge.rs:951`

- [ ] **Step 1: Replace unreachable with safe error**

```rust
// Before:
UsdBridgeErrorCode::Success => unreachable!("Success is not an error"),

// After:
UsdBridgeErrorCode::Success => UsdBridgeError::Unknown("unexpected Success code".into()),
```

### Task 2.7: Replace `is_multiple_of` with Modulo

**Files:**

- Modify: `crates/bif_core/src/usd/loader.rs:52`

- [ ] **Step 1: Replace nightly-only method**

```rust
// Before:
if i > 0 && (s.len() - i).is_multiple_of(3) {

// After:
if i > 0 && (s.len() - i) % 3 == 0 {
```

### Task 2.8: Box Filter Half-Open Interval

**Files:**

- Modify: `crates/bif_renderer/src/filter.rs:101-107`

- [ ] **Step 1: Use half-open interval**

```rust
fn eval_box(dx: f32, dy: f32) -> f32 {
    if dx.abs() < 0.5 && dy.abs() < 0.5 {
        1.0
    } else {
        0.0
    }
}
```

- [ ] **Step 2: Update test assertion for boundary**

### Task 2.9: Run all tests

- [ ] `cargo test -p bif_renderer && cargo test -p bif_core && cargo test -p bif_viewport`

---

## Phase 3: OpenPBR & Material Improvements

**Commit message:** `fix: OpenPBR material — delta detection, per-vertex tangents, VNDF sampling`

### Task 3.1: Fix `is_delta()` for Transmission

**Files:**

- Modify: `crates/bif_renderer/src/openpbr.rs:521-524`

- [ ] **Step 1: Include transmission in delta check**

```rust
fn is_delta(&self) -> bool {
    self.specular_roughness < 0.001
        && (self.base_metalness > 0.999
            || self.transmission_weight > 0.5
            || (self.transmission_weight > 0.0 && self.specular_roughness < 0.001))
}
```

Simplified (since outer condition already checks roughness < 0.001):

```rust
fn is_delta(&self) -> bool {
    self.specular_roughness < 0.001
        && (self.base_metalness > 0.999 || self.transmission_weight > 0.0)
}
```

### Task 3.2: Per-Vertex Tangent Accumulation

**Files:**

- Modify: `crates/bif_renderer/src/embree.rs:390-428` (unindexed path tangents)

The current code computes one tangent per triangle. To get per-vertex tangents, accumulate tangents at each vertex and normalize. For the unindexed path (where each triangle has its own vertices), per-triangle tangents ARE per-vertex, so this is already correct for that path. The indexed path in `from_indexed` needs per-vertex accumulation.

- [ ] **Step 1: In `from_indexed`, compute per-vertex tangents by accumulating triangle tangents at shared vertices**

For each triangle, compute tangent from UV deltas. Add tangent to each of the 3 vertex tangent accumulators. After all triangles, normalize per-vertex tangents.

- [ ] **Step 2: Interpolate tangent in hit() using barycentric coordinates (same as normal interpolation)**
- [ ] **Step 3: Run tests** `cargo test -p bif_renderer`

### Task 3.3: VNDF GGX Sampling

**Files:**

- Modify: `crates/bif_renderer/src/openpbr.rs` (add `sample_ggx_vndf` function)

- [ ] **Step 1: Implement VNDF sampling (Heitz 2018)**

Replace `sample_ggx` with visible normal distribution sampling:

```rust
fn sample_ggx_vndf(wo: Vec3, alpha_x: f32, alpha_y: f32, rng: &mut dyn RngCore) -> Vec3 {
    // 1. Stretch view direction
    let v = Vec3::new(alpha_x * wo.x, alpha_y * wo.y, wo.z).normalize();
    // 2. Orthonormal basis
    let t1 = if v.z.abs() < 0.9999 {
        v.cross(Vec3::Z).normalize()
    } else {
        Vec3::X
    };
    let t2 = t1.cross(v);
    // 3. Sample point with polar coordinates
    let r1: f32 = gen_f32(rng);
    let r2: f32 = gen_f32(rng);
    let a = 1.0 / (1.0 + v.z);
    let r = r1.sqrt();
    let phi = if r2 < a {
        r2 / a * PI
    } else {
        PI + (r2 - a) / (1.0 - a) * PI
    };
    let p1 = r * phi.cos();
    let p2 = r * phi.sin() * if r2 < a { 1.0 } else { v.z };
    // 4. Compute normal
    let n = p1 * t1 + p2 * t2 + (1.0 - p1 * p1 - p2 * p2).max(0.0).sqrt() * v;
    Vec3::new(alpha_x * n.x, alpha_y * n.y, n.z.max(0.0)).normalize()
}
```

- [ ] **Step 2: Update `scatter_specular_textured` to use VNDF sampling**
- [ ] **Step 3: Update GGX PDF to use VNDF PDF: `D * G1 * VdotH / NdotV`**
- [ ] **Step 4: Run tests** `cargo test -p bif_renderer`

---

## Phase 4: Pipeline Quality

**Commit message:** `fix: pipeline quality — EXR metadata, shared Embree device, basis dedup, HDRI pole clamp`

### Task 4.1: EXR Color Space Metadata

**Files:**

- Modify: `crates/bif_renderer/src/exr_writer.rs:239-241`

- [ ] **Step 1: Add render metadata to layer attributes**

The `exr` crate's `LayerAttributes` supports custom text attributes. Add pipeline identification:

```rust
let mut attrs = LayerAttributes::named("main");
attrs.other.insert(
    exr::meta::attribute::Text::from("renderEngine"),
    exr::meta::attribute::AttributeValue::Text(
        exr::meta::attribute::Text::from("BIF/Ivar"),
    ),
);
```

### Task 4.2: Embree Device Sharing

**Files:**

- Modify: `crates/bif_renderer/src/embree.rs` (add `new_with_device` / `from_indexed_with_device`)

- [ ] **Step 1: Add device parameter variants**

Add `RTCDevice` parameter to constructors, keep existing `new()`/`from_indexed()` as convenience wrappers that create a device internally.

- [ ] **Step 2: Update callers in `bif_viewport/src/ivar_build.rs` to pass shared device**
- [ ] **Step 3: Run tests** `cargo test -p bif_renderer`

### Task 4.3: Orthonormal Basis Dedup

**Files:**

- Modify: `crates/bif_renderer/src/light.rs:474-482`

- [ ] **Step 1: Replace local `orthonormal_basis` with `bif_math::build_orthonormal_basis`**

```rust
// Before:
let (u, v) = orthonormal_basis(axis);

// After:
use bif_math::build_orthonormal_basis;
let (u, v) = build_orthonormal_basis(axis);
```

- [ ] **Step 2: Remove the local `orthonormal_basis` function from light.rs**
- [ ] **Step 3: Run tests** `cargo test -p bif_renderer`

### Task 4.4: SHARC TOCTOU Race Documentation

**Files:**

- Modify: `crates/bif_renderer/src/radiance_cache.rs:444` (add comment)

- [ ] **Step 1: Document the known race condition**

Add a comment block explaining the TOCTOU race in the EMA blend path, that it's acceptable for IPR preview quality, and that production-quality rendering should use a mutex or atomic CAS on packed values.

### Task 4.5: HDRI Pole Clamp — Resolution-Dependent

**Files:**

- Modify: `crates/bif_renderer/src/hdri.rs:157`

- [ ] **Step 1: Replace fixed epsilon with resolution-dependent clamp**

```rust
// Before:
let v_clamped = v.clamp(0.001 / PI, 1.0 - 0.001 / PI);

// After:
let half_texel = 0.5 / self.hdr.height as f32;
let v_clamped = v.clamp(half_texel, 1.0 - half_texel);
```

- [ ] **Step 2: Run tests** `cargo test -p bif_renderer`

---

## Phase 5: Data & API Cleanup

**Commit message:** `refactor: data cleanup — mesh dedup, Copy on Transform, API improvements`

### Task 5.1: Strengthen Mesh Dedup Hash

**Files:**

- Modify: `crates/bif_core/src/usd/loader.rs:153-186`

- [ ] **Step 1: Increase sample count and add normal/UV sampling**

```rust
let sample_count = vlen.min(50); // Was 10
// ... same sampling loop but with 50 samples

// Also hash normals if present
if !mesh_data.normals.is_empty() {
    let nlen = mesh_data.normals.len();
    let n_samples = nlen.min(20);
    for i in 0..n_samples {
        let idx = if n_samples <= 1 { 0 } else { i * (nlen - 1) / (n_samples - 1) };
        if let Some(n) = mesh_data.normals.get(idx) {
            n[0].to_bits().hash(&mut hasher);
            n[1].to_bits().hash(&mut hasher);
            n[2].to_bits().hash(&mut hasher);
        }
    }
}
```

### Task 5.2: `add_instance` Returns Index

**Files:**

- Modify: `crates/bif_core/src/scene.rs:620-637,667-676`
- Modify: all callers of `add_instance` and `set_last_instance_purpose`

- [ ] **Step 1: Make `add_instance` return the instance index**

```rust
pub fn add_instance(&mut self, prototype_id: usize, transform: Transform) -> usize {
    let idx = self.instances.len();
    self.instances.push(Instance::new(prototype_id, transform));
    self.instance_animations.push(None);
    idx
}
```

- [ ] **Step 2: Add `set_instance_purpose(index, purpose)` method**

```rust
pub fn set_instance_purpose(&mut self, index: usize, purpose: Purpose) {
    if let Some(inst) = self.instances.get_mut(index) {
        inst.purpose = purpose;
    }
}
```

- [ ] **Step 3: Update callers to use returned index instead of `set_last_instance_purpose`**
- [ ] **Step 4: Deprecate `set_last_instance_purpose`**

### Task 5.3: Derive Copy on Transform

**Files:**

- Modify: `crates/bif_core/src/scene.rs:322`

- [ ] **Step 1: Add Copy derive**

```rust
#[derive(Clone, Copy, Debug)]
pub struct Transform {
```

- [ ] **Step 2: Replace `.clone()` calls on Transform with direct copy (optional cleanup)**

### Task 5.4: HdrImage::downscale_to_max_dim — Return Option

**Files:**

- Modify: `crates/bif_core/src/hdr.rs:221-225`
- Modify: callers

- [ ] **Step 1: Return `Option<Self>` where None means no downscale needed**

```rust
pub fn downscale_to_max_dim(&self, max_dim: u32) -> Option<Self> {
    let max_side = self.width.max(self.height);
    if max_side <= max_dim {
        return None;
    }
    // ... rest returns Some(downscaled)
}
```

- [ ] **Step 2: Update callers to use `unwrap_or_else(|| self.clone())` or borrow original**

### Task 5.5: Remove Redundant Prototype::bounds

**Files:**

- Modify: `crates/bif_core/src/scene.rs:181,187-194`
- Modify: all references to `prototype.bounds`

- [ ] **Step 1: Remove `bounds` field, update constructor**
- [ ] **Step 2: Replace all `proto.bounds` with `proto.mesh.bounds`**
- [ ] **Step 3: Run tests** `cargo test`

### Task 5.6: IBL [f32;3] to Vec3

**Files:**

- Modify: `crates/bif_core/src/ibl.rs` (replace local math helpers)

- [ ] **Step 1: Replace local `normalize`, `dot`, `cross` with `Vec3` operations**
- [ ] **Step 2: Convert at API boundaries only**

### Task 5.7: Run all tests

- [ ] `cargo test`

---

## Phase 6: Architecture — GraphNodeId

**Commit message:** `refactor: introduce GraphNodeId — decouple node graph from egui_snarl`

### Task 6.1: Define GraphNodeId Type

**Files:**

- Create: `crates/bif_viewport/src/node_graph/node_id.rs`

- [ ] **Step 1: Create framework-agnostic node ID**

```rust
/// Framework-agnostic node identifier for the node graph.
///
/// Decouples scene evaluation and persistence from the UI framework.
/// Maps bidirectionally to `egui_snarl::NodeId` at the UI boundary.
#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct GraphNodeId(pub u64);

impl From<egui_snarl::NodeId> for GraphNodeId {
    fn from(id: egui_snarl::NodeId) -> Self {
        Self(id.0 as u64)
    }
}
```

### Task 6.2: Replace NodeId in NodeGraphContext

**Files:**

- Modify: `crates/bif_viewport/src/lib.rs:302-315`
- Modify: `crates/bif_viewport/src/node_graph/mod.rs`

- [ ] **Step 1: Change all HashMap keys from `egui_snarl::NodeId` to `GraphNodeId`**
- [ ] **Step 2: Convert at UI boundary in viewer.rs**

### Task 6.3: Update All Consumers

**Files:**

- Modify: `crates/bif_viewport/src/render.rs`
- Modify: `crates/bif_viewport/src/scene_loader.rs`

- [ ] **Step 1: Replace all `egui_snarl::NodeId` references in non-UI code with `GraphNodeId`**
- [ ] **Step 2: Run tests** `cargo test -p bif_viewport`

---

## Phase 7: Architecture — Renderer Decomposition

**Commit message:** `refactor: Renderer decomposition — GPU sub-structs, type extraction, convention docs`

### Task 7.1: Extract GpuMaterialState

**Files:**

- Modify: `crates/bif_viewport/src/lib.rs`

- [ ] **Step 1: Create sub-struct for material GPU state**

Move `material_uniform`, `material_buffer`, `material_bind_group_layout`, `material_bind_group`, `material_table_buffer`, `material_table_len`, `triangle_material_buffer`, `has_triangle_materials` into a `GpuMaterialState` struct.

- [ ] **Step 2: Update all references in render.rs, scene_loader.rs, etc.**

### Task 7.2: Extract GpuTextureState

**Files:**

- Modify: `crates/bif_viewport/src/lib.rs`

- [ ] **Step 1: Create sub-struct for texture GPU state**

Move `gpu_textures`, `texture_sampler`, `texture_bind_group_layout`, `texture_bind_group` into `GpuTextureState`.

- [ ] **Step 2: Update all references**

### Task 7.3: Move Type Definitions Out of lib.rs

**Files:**

- Create: `crates/bif_viewport/src/types.rs`
- Modify: `crates/bif_viewport/src/lib.rs`

- [ ] **Step 1: Move `UsdLoadStatus`, `SceneInstances`, `PurposeMode`, `DisplaySettings` to types.rs**
- [ ] **Step 2: Re-export from lib.rs**

### Task 7.4: Document State Mutation Convention

**Files:**

- Modify: `crates/bif_viewport/src/render.rs` (add module-level doc comment)

- [ ] **Step 1: Add convention documentation**

```rust
//! # State Mutation Convention
//!
//! **Direct mutation** (in egui closures): Simple boolean toggles with no side effects
//! (show_grid, show_ui, point_preview.visible). These are safe because they affect
//! only the next frame's rendering, with no cascading state changes.
//!
//! **EventBus**: Anything that triggers side effects (scene reload, camera sync,
//! undo/redo, node graph operations). Events are drained in `dispatch_events()`
//! for predictable ordering.
```

### Task 7.5: Run all tests

- [ ] `cargo test`

---

## Verification

After all phases:

1. `cargo build` — no warnings
2. `cargo test` — all tests pass
3. `cargo clippy -- -D warnings` — clean
4. `cargo fmt --check` — formatted
5. Visual check: load a USD scene with glossy materials, verify no energy conservation artifacts
