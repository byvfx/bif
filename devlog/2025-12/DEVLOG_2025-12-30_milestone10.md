# Devlog: Milestone 10 - CPU Path Tracer

**Date:** 2025-12-30  
**Duration:** ~4 hours  
**Status:** ✅ Complete

## Objective

Port the Go raytracer from `legacy/go-raytracing/` to Rust, creating a fully functional CPU path tracer in the `bif_renderer` crate.

## Implementation Summary

### New Crate: `bif_renderer`

Created a complete path tracing library with the following modules:

```
crates/bif_renderer/src/
├── lib.rs          # Crate exports
├── ray.rs          # Ray primitive
├── hittable.rs     # Hittable trait, HitRecord, HittableList
├── material.rs     # Material trait + implementations
├── sphere.rs       # Sphere primitive
├── triangle.rs     # Triangle primitive (Möller-Trumbore)
├── camera.rs       # Camera with DOF support
├── bvh.rs          # BVH acceleration structure
└── renderer.rs     # Core path tracing logic
```

### Ray and Hit Record

```rust
pub struct Ray {
    pub origin: Vec3,
    pub direction: Vec3,
    pub time: f32,  // For motion blur
}

pub struct HitRecord<'a> {
    pub p: Vec3,
    pub normal: Vec3,
    pub material: &'a dyn Material,
    pub u: f32, pub v: f32,  // UV coords
    pub t: f32,
    pub front_face: bool,
}
```

Key design: `HitRecord` holds a reference to the material, requiring careful lifetime management across the `Hittable::hit()` trait method.

### Materials

Implemented four material types from the Go codebase:

| Material | Description |
|----------|-------------|
| `Lambertian` | Diffuse/matte surfaces |
| `Metal` | Reflective with configurable fuzz |
| `Dielectric` | Glass/water with Schlick approximation |
| `DiffuseLight` | Emissive surfaces for area lights |

### Primitives

**Sphere:**
- Quadratic ray-sphere intersection
- UV mapping for textures (spherical coordinates)

**Triangle:**
- Möller-Trumbore algorithm for fast intersection
- Barycentric coordinates for UV interpolation

### BVH Acceleration

Initially ported the complex Go BVH with separate primitive tracking, but hit a subtle bug where objects were being lost during tree construction.

**Root Cause:** The original implementation tracked indices separately from objects, and the partition logic got indices out of sync.

**Solution:** Simplified to sort objects directly:

```rust
fn build(mut objects: Vec<Box<dyn Hittable + Send + Sync>>) -> Self {
    let n = objects.len();
    
    if n <= LEAF_MAX_SIZE {
        return BvhNode::Leaf { objects, bbox: bounds };
    }
    
    // Sort by centroid on longest axis
    let axis = centroid_bounds.longest_axis();
    objects.sort_unstable_by(|a, b| {
        let a_c = a.bounding_box().centroid();
        let b_c = b.bounding_box().centroid();
        // Compare on axis...
    });
    
    // Split and recurse
    let mid = n / 2;
    let right_objects = objects.split_off(mid);
    let left = Self::build(objects);
    let right = Self::build(right_objects);
    
    BvhNode::Branch { left, right, bbox }
}
```

### Camera

Builder-pattern camera with:
- Configurable FOV and aspect ratio
- Depth of field (defocus blur)
- Motion blur support via ray time

### Renderer

Core path tracing loop:

```rust
pub fn ray_color(ray: &Ray, world: &dyn Hittable, depth: u32, config: &RenderConfig) -> Color {
    if depth == 0 { return Color::ZERO; }
    
    if !world.hit(ray, Interval::new(0.001, f32::INFINITY), &mut rec) {
        return sky_gradient(ray);  // Or background color
    }
    
    let emission = rec.material.emitted(rec.u, rec.v, rec.p);
    
    match rec.material.scatter(ray, &rec) {
        Some((attenuation, scattered)) => {
            emission + attenuation * ray_color(&scattered, world, depth - 1, config)
        }
        None => emission
    }
}
```

### Example: simple_render

Created a working example that renders the classic "Ray Tracing in One Weekend" scene:

- Ground sphere
- Three large spheres (glass, matte, metal)
- ~475 small random spheres
- Outputs to PNG

## Performance

| Scene | Resolution | SPP | Time |
|-------|------------|-----|------|
| RTIOW Final | 800x450 | 100 | ~52s |

Currently single-threaded. Rayon parallel rendering is next.

## Tests

14 unit tests covering:
- Ray construction and `at()` method
- Sphere intersection (hit/miss)
- Triangle intersection (Möller-Trumbore)
- Camera initialization and ray generation
- BVH construction (empty, single, multiple)
- Sky gradient and gamma correction
- Full pixel render pipeline

## Dependencies Added

```toml
[dependencies]
bif_math = { path = "../bif_math" }
glam.workspace = true
rand = "0.8"
rayon = "1.10"
image = "0.24"
```

## Commits

1. `db8c0e9` - bif_renderer crate with Ray, Hittable, Material
2. `ec257c6` - Add Sphere, Triangle, Camera, BVH
3. `f55e4e7` - Add renderer module and simple_render example

## Lessons Learned

1. **Lifetime management in traits:** When trait methods need to return references tied to `&self`, use `fn method<'a>(&'a self, rec: &mut Record<'a>)` pattern.

2. **BVH simplicity wins:** The Go codebase had optimized parallel construction with semaphores. For initial port, simpler is better - fix bugs first, optimize later.

3. **f32 vs f64:** bif_math uses f32 consistently (matches GPU). Tests initially used f64 constants which caused type mismatches.

## Next Steps (Milestone 11+)

- [ ] Parallel rendering with rayon
- [ ] Progressive rendering for interactive preview
- [ ] Triangle mesh support (integrate with bif_core)
- [ ] HDRI environment maps
- [ ] Multiple Importance Sampling (MIS)
