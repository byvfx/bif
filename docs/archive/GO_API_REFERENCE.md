# Go Raytracer API Reference

This document provides a complete API reference for the Go raytracer implementation (`rt/` package). Use this as a lookup when porting to Rust—each section shows the Go signature, behavior, and Rust equivalent.

---

## Table of Contents

1. [Core Math Types](#core-math-types)
2. [Ray and Interval](#ray-and-interval)
3. [Bounding Boxes (AABB)](#bounding-boxes-aabb)
4. [Interfaces (Traits)](#interfaces-traits)
5. [Primitives](#primitives)
6. [Materials](#materials)
7. [Textures](#textures)
8. [Camera System](#camera-system)
9. [BVH Acceleration](#bvh-acceleration)
10. [Transforms](#transforms)
11. [Renderers](#renderers)
12. [Utility Functions](#utility-functions)
13. [File I/O](#file-io)
14. [Constants and Configuration](#constants-and-configuration)

---

## Core Math Types

### Vec3 (`rt/vec3.go`)

The fundamental 3D vector type. Also aliased as `Point3` and `Color`.

| Go | Rust Equivalent |
|----|-----------------|
| `type Vec3 struct { X, Y, Z float64 }` | `glam::Vec3A` or custom `Vec3 { x: f32, y: f32, z: f32 }` |
| `type Point3 = Vec3` | `type Point3 = Vec3;` |
| `type Color = Vec3` | `type Color = Vec3;` |

#### Constructors

```go
// Go
func NewVec3(x, y, z float64) Vec3

// Rust
impl Vec3 {
    pub fn new(x: f32, y: f32, z: f32) -> Self
}
```

#### Methods

| Go Method | Signature | Rust Equivalent |
|-----------|-----------|-----------------|
| `Add` | `(v Vec3) Add(u Vec3) Vec3` | `impl Add for Vec3` |
| `Sub` | `(v Vec3) Sub(u Vec3) Vec3` | `impl Sub for Vec3` |
| `Mult` | `(v Vec3) Mult(u Vec3) Vec3` | `v * u` (component-wise) |
| `Scale` | `(v Vec3) Scale(t float64) Vec3` | `impl Mul<f32> for Vec3` |
| `Div` | `(v Vec3) Div(t float64) Vec3` | `impl Div<f32> for Vec3` |
| `Neg` | `(v Vec3) Neg() Vec3` | `impl Neg for Vec3` |
| `Len2` | `(v Vec3) Len2() float64` | `v.length_squared()` |
| `Len` | `(v Vec3) Len() float64` | `v.length()` |
| `Unit` | `(v Vec3) Unit() Vec3` | `v.normalize()` |
| `NearZero` | `(v Vec3) NearZero() bool` | `v.abs_diff_eq(Vec3::ZERO, 1e-8)` |
| `String` | `(v Vec3) String() string` | `impl Display for Vec3` |

#### Free Functions

| Go Function | Signature | Rust Equivalent |
|-------------|-----------|-----------------|
| `Dot` | `Dot(a, b Vec3) float64` | `a.dot(b)` |
| `Cross` | `Cross(a, b Vec3) Vec3` | `a.cross(b)` |
| `Reflect` | `Reflect(v, n Vec3) Vec3` | `v - n * 2.0 * v.dot(n)` |
| `Refract` | `Refract(uv, n Vec3, etaiOverEtat float64) Vec3` | Custom impl (see below) |
| `RandomVec3` | `RandomVec3() Vec3` | `Vec3::new(rng.gen(), rng.gen(), rng.gen())` |
| `RandomVec3Range` | `RandomVec3Range(min, max float64) Vec3` | `Vec3::new(rng.gen_range(min..max), ...)` |
| `RandomUnitVector` | `RandomUnitVector() Vec3` | Rejection sampling in unit sphere |
| `RandomOnHemiSphere` | `RandomOnHemiSphere(normal Vec3) Vec3` | `if dot > 0 { v } else { -v }` |
| `RandomInUnitDisk` | `RandomInUnitDisk() Vec3` | Rejection sampling |

#### Rust Refract Implementation

```rust
pub fn refract(uv: Vec3, n: Vec3, etai_over_etat: f32) -> Vec3 {
    let cos_theta = (-uv).dot(n).min(1.0);
    let r_out_perp = (uv + n * cos_theta) * etai_over_etat;
    let r_out_parallel = n * -(1.0 - r_out_perp.length_squared()).abs().sqrt();
    r_out_perp + r_out_parallel
}
```

---

## Ray and Interval

### Ray (`rt/ray.go`)

| Go | Rust |
|----|------|
| `type Ray struct { orig Point3; dir Vec3; tm float64 }` | `struct Ray { origin: Point3, direction: Vec3, time: f32 }` |

#### Methods

| Go Method | Signature | Rust Equivalent |
|-----------|-----------|-----------------|
| `NewRay` | `NewRay(origin Point3, direction Vec3, time float64) Ray` | `Ray::new(origin, direction, time)` |
| `Origin` | `(r Ray) Origin() Point3` | `ray.origin` (public field) |
| `Direction` | `(r Ray) Direction() Vec3` | `ray.direction` (public field) |
| `At` | `(r Ray) At(t float64) Point3` | `ray.at(t: f32) -> Point3` |
| `Time` | `(r Ray) Time() float64` | `ray.time` (public field) |

### Interval (`rt/interval.go`)

| Go | Rust |
|----|------|
| `type Interval struct { Min, Max float64 }` | `struct Interval { min: f32, max: f32 }` |

#### Static Constants

| Go | Rust |
|----|------|
| `EmptyInterval` | `Interval::EMPTY` |
| `UniverseInterval` | `Interval::UNIVERSE` |

#### Methods

| Go Method | Signature | Rust Equivalent |
|-----------|-----------|-----------------|
| `NewInterval` | `NewInterval(min, max float64) Interval` | `Interval::new(min, max)` |
| `NewIntervalFromIntervals` | `NewIntervalFromIntervals(a, b Interval) Interval` | `Interval::union(a, b)` |
| `Size` | `(i Interval) Size() float64` | `i.size()` |
| `Contains` | `(i Interval) Contains(x float64) bool` | `i.contains(x)` — `min <= x <= max` |
| `Surrounds` | `(i Interval) Surrounds(x float64) bool` | `i.surrounds(x)` — `min < x < max` |
| `Clamp` | `(i Interval) Clamp(x float64) float64` | `x.clamp(i.min, i.max)` |
| `Expand` | `(i Interval) Expand(delta float64) Interval` | `Interval::new(i.min - delta, i.max + delta)` |
| `Add` | `(i Interval) Add(displacement float64) Interval` | `Interval::new(i.min + d, i.max + d)` |

---

## Bounding Boxes (AABB)

### AABB (`rt/aabb.go`)

| Go | Rust |
|----|------|
| `type AABB struct { X, Y, Z Interval }` | `struct AABB { x: Interval, y: Interval, z: Interval }` |

#### Static Constants

| Go | Rust |
|----|------|
| `EmptyAABB` | `AABB::EMPTY` |
| `UniverseAABB` | `AABB::UNIVERSE` |

#### Constructors

| Go Function | Rust Equivalent |
|-------------|-----------------|
| `NewAABB()` | `AABB::empty()` |
| `NewAABBFromIntervals(x, y, z Interval)` | `AABB::from_intervals(x, y, z)` |
| `NewAABBFromPoints(a, b Point3)` | `AABB::from_points(a, b)` |
| `NewAABBFromBoxes(box0, box1 AABB)` | `AABB::union(box0, box1)` |

#### Methods

| Go Method | Signature | Behavior | Rust Equivalent |
|-----------|-----------|----------|-----------------|
| `AxisInterval` | `(box AABB) AxisInterval(n int) Interval` | Returns X/Y/Z interval by index | `box.axis(n)` |
| `Hit` | `(box AABB) Hit(r Ray, rayT Interval) bool` | Slab intersection test | `box.hit(ray, t_interval)` |
| `LongestAxis` | `(box AABB) LongestAxis() int` | Returns 0/1/2 for X/Y/Z | `box.longest_axis()` |
| `Centroid` | `(box AABB) Centroid() Vec3` | Center point of box | `box.centroid()` |
| `Translate` | `(box AABB) Translate(offset Vec3) AABB` | Offset all intervals | `box.translate(offset)` |
| `padToMinimums` | Private: ensures min thickness | Avoids degenerate boxes | Handle in constructor |

---

## Interfaces (Traits)

### Hittable (`rt/hittable.go`)

```go
// Go
type Hittable interface {
    Hit(r Ray, rayT Interval, rec *HitRecord) bool
    BoundingBox() AABB
}
```

```rust
// Rust
pub trait Hittable: Send + Sync {
    fn hit(&self, ray: &Ray, t_range: Interval, rec: &mut HitRecord) -> bool;
    fn bounding_box(&self) -> AABB;
}
```

### HitRecord

| Go Field | Type | Rust Equivalent |
|----------|------|-----------------|
| `P` | `Point3` | `p: Point3` |
| `Normal` | `Vec3` | `normal: Vec3` |
| `Mat` | `Material` | `material: Arc<dyn Material>` |
| `U`, `V` | `float64` | `u: f32, v: f32` |
| `T` | `float64` | `t: f32` |
| `FrontFace` | `bool` | `front_face: bool` |

#### HitRecord Methods

| Go Method | Behavior |
|-----------|----------|
| `SetFaceNormal(r Ray, outwardNormal Vec3)` | Sets `FrontFace` and flips normal if needed |

### Material (`rt/material.go`)

```go
// Go
type Material interface {
    Scatter(rIn Ray, rec *HitRecord, attenuation *Color, scattered *Ray) bool
    Emitted(u, v float64, p Point3) Color
}

type PDFEvaluator interface {
    PDF(wi, wo, normal Vec3) float64
}

type MaterialInfo interface {
    Properties() MaterialProperties
}
```

```rust
// Rust
pub trait Material: Send + Sync {
    fn scatter(&self, r_in: &Ray, rec: &HitRecord) -> Option<(Color, Ray)>;
    fn emitted(&self, u: f32, v: f32, p: Point3) -> Color {
        Color::ZERO
    }
    fn pdf(&self, wi: Vec3, wo: Vec3, normal: Vec3) -> f32 {
        0.0
    }
    fn properties(&self) -> MaterialProperties {
        MaterialProperties::default()
    }
}
```

### MaterialProperties

| Go Field | Type | Purpose |
|----------|------|---------|
| `isPureSpecular` | `bool` | Delta distribution (mirror/glass) |
| `isEmissive` | `bool` | Light source |
| `CanUseNEE` | `bool` | Can use Next Event Estimation |

### Texture (`rt/texture.go`)

```go
// Go
type Texture interface {
    Value(u, v float64, p Point3) Color
}
```

```rust
// Rust
pub trait Texture: Send + Sync {
    fn value(&self, u: f32, v: f32, p: Point3) -> Color;
}
```

---

## Primitives

### Sphere (`rt/sphere.go`)

| Field | Type | Notes |
|-------|------|-------|
| `Center` | `Ray` | Origin = center, Direction = velocity (for motion blur) |
| `Radius` | `float64` | Always positive |
| `Mat` | `Material` | |
| `bbox` | `AABB` | Pre-computed |

#### Constructors

| Function | Purpose |
|----------|---------|
| `NewSphere(center Point3, radius float64, mat Material)` | Static sphere |
| `NewMovingSphere(center1, center2 Point3, radius float64, mat Material)` | Motion blur |

#### Methods

| Method | Behavior | Notes |
|--------|----------|---------|
| `SphereCenter(time float64) Point3` | Interpolates position for motion blur | |
| `Hit(r Ray, rayT Interval, rec *HitRecord) bool` | Quadratic intersection | |
| `BoundingBox() AABB` | Returns pre-computed bbox | |

#### UV Mapping

```go
func getSphereUV(p Point3) (u, v float64) {
    theta := math.Acos(-p.Y)
    phi := math.Atan2(-p.Z, p.X) + math.Pi
    u = phi / (2 * math.Pi)
    v = theta / math.Pi
    return u, v
}
```

### Triangle (`rt/triangle.go`)

| Field | Type | Notes |
|-------|------|-------|
| `v0, v1, v2` | `Point3` | Vertices |
| `normal` | `Vec3` | Pre-computed, unit length |
| `mat` | `Material` | |
| `bbox` | `AABB` | Pre-computed |
| `D` | `float64` | Plane constant (optional) |

#### Algorithm

Uses **Möller-Trumbore intersection**:

1. Compute edge vectors `edge1 = v1 - v0`, `edge2 = v2 - v0`
2. Compute determinant via cross products
3. Calculate barycentric coordinates `u, v`
4. Check if inside triangle: `u >= 0`, `v >= 0`, `u + v <= 1`

**Rust Port Note**: Consider pre-computing and storing edge vectors to avoid recomputation in `hit()`.

### Quad (`rt/quad.go`)

| Field | Type | Notes |
|-------|------|-------|
| `Q` | `Point3` | Corner point |
| `u, v` | `Vec3` | Edge vectors |
| `w` | `Vec3` | `n / dot(n, n)` for projection |
| `normal` | `Vec3` | Unit normal |
| `D` | `float64` | Plane constant |
| `mat` | `Material` | |
| `bbox` | `AABB` | Pre-computed |

#### Methods for Area Lights

| Method | Purpose |
|--------|---------|
| `SamplePoint() Point3` | Random point on surface |
| `Area() float64` | Surface area |
| `PdfValue(origin, direction) float64` | PDF for light sampling |

### Plane (`rt/plane.go`)

Infinite plane defined by point and normal.

| Field | Type |
|-------|------|
| `Point` | `Point3` |
| `Normal` | `Vec3` |
| `Mat` | `Material` |

### Circle/Disk (`rt/circle.go`)

| Field | Type |
|-------|------|
| `Center` | `Point3` |
| `Normal` | `Vec3` |
| `Radius` | `float64` |
| `Mat` | `Material` |

### Compound Primitives (`rt/primitives.go`)

| Function | Creates |
|----------|---------|
| `NewBox(a, b Point3, mat Material)` | 6 quads forming a box |
| `NewPyramid(base Point3, size, height float64, mat Material)` | Base quad + 4 triangles |

---

## Materials

### Lambertian (`rt/material.go`)

Diffuse material with cosine-weighted hemisphere sampling.

| Field | Type |
|-------|------|
| `tex` | `Texture` |

```go
// Scatter: Cosine-weighted hemisphere sampling
scatterDirection := rec.Normal.Add(RandomUnitVector())
if scatterDirection.NearZero() {
    scatterDirection = rec.Normal
}

// PDF: cosTheta / π
func (l *Lambertian) PDF(wi, wo, normal Vec3) float64 {
    cosTheta := Dot(normal, wo)
    if cosTheta < 0 { return 0 }
    return cosTheta / math.Pi
}
```

### Metal (`rt/material.go`)

Reflective material with optional fuzz.

| Field | Type | Notes |
|-------|------|-------|
| `Albedo` | `Color` | |
| `Fuzz` | `float64` | 0 = perfect mirror, 1 = very rough |

```go
// Scatter: Perfect reflection + fuzz
reflected := Reflect(rIn.Direction(), rec.Normal)
reflected = reflected.Unit().Add(RandomUnitVector().Scale(m.Fuzz))
// Only valid if dot(scattered.Direction, normal) > 0
```

**Important**: `CanUseNEE = false` for metals to maintain proper specular appearance.

### Dielectric (`rt/material.go`)

Glass/refractive material with Schlick approximation for Fresnel.

| Field | Type |
|-------|------|
| `RefractionIndex` | `float64` |

```go
// Schlick approximation
func reflectance(cosine, refIdx float64) float64 {
    r0 := (1 - refIdx) / (1 + refIdx)
    r0 = r0 * r0
    return r0 + (1-r0)*math.Pow(1-cosine, 5)
}

// Total internal reflection check
cannotRefract := ri*sinTheta > 1.0
```

### DiffuseLight (`rt/material.go`)

Emissive material for area lights.

| Field | Type |
|-------|------|
| `tex` | `Texture` |

```go
// Scatter: returns false (no scattering)
// Emitted: returns texture color
```

---

## Textures

### SolidColor

| Field | Type |
|-------|------|
| `Albedo` | `Color` |

### CheckerTexture

3D procedural checker pattern.

| Field | Type | Notes |
|-------|------|
| `invScale` | `float64` | `1.0 / scale` |
| `even` | `Texture` |
| `odd` | `Texture` |

```go
func (c *CheckerTexture) Value(u, v float64, p Point3) Color {
    xInteger := int(math.Floor(c.invScale*p.X))
    yInteger := int(math.Floor(c.invScale*p.Y))
    zInteger := int(math.Floor(c.invScale*p.Z))
    isEven := (xInteger+yInteger+zInteger)%2 == 0
    // ...
}
```

### NoiseTexture

Perlin noise with turbulence.

| Field | Type |
|-------|------|
| `noise` | `*Perlin` |
| `scale` | `float64` |

### ImageTexture (`rt/image_texture.go`)

Loads PNG/JPEG images.

| Field | Type |
|-------|------|
| `image` | `image.Image` |
| `width, height` | `int` |

---

## Camera System

### Camera (`rt/camera.go`)

| Field | Type | Default | Notes |
|-------|------|---------|-------|
| `AspectRatio` | `float64` | `1.0` | |
| `ImageWidth` | `int` | `800` | |
| `ImageHeight` | `int` | Computed | |
| `SamplesPerPixel` | `int` | `10` | |
| `MaxDepth` | `int` | `50` | Max bounces |
| `Vfov` | `float64` | `90` | Vertical FOV in degrees |
| `LookFrom` | `Point3` | `(0,0,0)` | Camera position |
| `LookAt` | `Point3` | `(0,0,-1)` | Look target |
| `Vup` | `Vec3` | `(0,1,0)` | Up vector |
| `DefocusAngle` | `float64` | `0.0` | DOF aperture angle |
| `FocusDist` | `float64` | `1.0` | Focus distance |
| `CameraMotion` | `bool` | `false` | Motion blur |
| `FreeCamera` | `bool` | `false` | Use Forward instead of LookAt |
| `Forward` | `Vec3` | `(0,0,-1)` | Direction for free camera |
| `Background` | `Color` | Black | Background color |
| `UseSkyGradient` | `bool` | `false` | Blue-white gradient |
| `Lights` | `[]Hittable` | `nil` | For NEE |

### CameraBuilder

Fluent builder pattern:

```go
camera := NewCameraBuilder().
    SetResolution(1200, 16.0/9.0).
    SetQuality(500, 50).                              // SPP, MaxDepth
    SetPosition(lookFrom, lookAt, vup).
    SetLens(vfov, defocusAngle, focusDist).
    EnableSkyGradient(true).
    Build()
```

### Camera Presets

| Preset | Resolution | SPP | Depth | Purpose |
|--------|------------|-----|-------|---------|
| `QuickPreview()` | 400 | 10 | 10 | Fast iteration |
| `StandardQuality()` | 600 | 100 | 50 | Default |
| `HighQuality()` | 1200 | 500 | 50 | Final render |

### Key Camera Methods

| Method | Purpose |
|--------|---------|
| `Initialize()` | Computes derived values (pixel delta, defocus disk, etc.) |
| `GetRay(i, j int) Ray` | Generates ray for pixel with AA jitter |
| `RayColor(r Ray, depth int, world Hittable) Color` | Path tracing with NEE |
| `AddLight(light Hittable)` | Register light for NEE |

---

## BVH Acceleration

### BVHNode (`rt/bvh.go`)

| Field | Type |
|-------|------|
| `left` | `Hittable` |
| `right` | `Hittable` |
| `bbox` | `AABB` |

### BVHLeaf

Holds multiple primitives for cache locality.

| Field | Type |
|-------|------|
| `objects` | `[]Hittable` |
| `bbox` | `AABB` |

### Constants

| Constant | Value | Purpose |
|----------|-------|---------|
| `bvhParallelThreshold` | `8192` | Min objects for parallel build |
| `bvhLeafMaxSize` | `4` | Max primitives per leaf |

### Construction Algorithm

1. Pre-compute bounding boxes and centroids (parallel for >10k objects)
2. Sort by longest axis centroid
3. Recursively split at midpoint
4. Create leaf nodes when `n <= bvhLeafMaxSize`
5. Use semaphore to limit concurrent goroutines

**Rust Port**: Use `rayon` for parallel construction, `Arc<dyn Hittable>` for node storage.

---

## Transforms

### Transform Builder (`rt/transform.go`)

Order: **Scale → Rotate (X→Y→Z) → Translate**

```go
transform := NewTransform().
    SetScale(Vec3{X: 2, Y: 2, Z: 2}).
    SetRotationY(45).
    SetPosition(Vec3{X: 0, Y: 1, Z: 0})
result := transform.Apply(obj)
```

### Individual Transforms

| Type | Fields |
|------|--------|
| `Translate` | `Obj Hittable`, `Offset Vec3` |
| `RotateX` | `Obj Hittable`, `SinTheta, CosTheta float64` |
| `RotateY` | `Obj Hittable`, `SinTheta, CosTheta float64` |
| `RotateZ` | `Obj Hittable`, `SinTheta, CosTheta float64` |
| `Scale` | `Obj Hittable`, `Factor Vec3`, `InvFactor Vec3` |

Each transform:

1. Transforms the incoming ray into local space
2. Calls inner object's `Hit()`
3. Transforms hit record back to world space
4. Pre-computes transformed bounding box

---

## Renderers

### ProgressiveRenderer (`rt/renderer.go`)

no need to port this as-is; use as reference if needed.

Scanline-based progressive rendering.

| Field | Type |
|-------|------|
| `framebuffer` | `*image.RGBA` |
| `camera` | `*Camera` |
| `world` | `Hittable` |
| `currentRow` | `int` |
| `completed` | `bool` |

Implements `ebiten.Game` interface for live display.

### BucketRenderer (`rt/bucket_renderer.go`)

Parallel tile-based rendering with multi-pass progressive refinement.

| Field | Type | Notes |
|-------|------|-------|
| `framebuffer` | `*image.RGBA` | Thread-safe via `Mutex` |
| `camera` | `*Camera` | |
| `world` | `Hittable` | |
| `buckets` | `[]Bucket` | Spiral-ordered from center |
| `completedCount` | `atomic.Int32` | Progress tracking |
| `totalPasses` | `int` | 3: Preview → Medium → Final |
| `numWorkers` | `int` | Usually `runtime.NumCPU()` |

### Bucket Structure

| Field | Type | Notes |
|-------|------|-------|
| `X, Y` | `int` | Top-left corner |
| `Width, Height` | `int` | Tile dimensions |

### Multi-Pass Strategy

| Pass | SPP | Depth | Purpose |
|------|-----|-------|---------|
| 0 | 1 | 3 | Quick preview |
| 1 | SPP/4 | Depth/2 | Medium quality |
| 2 | Full | Full | Final render |

### Bucket Ordering

**Spiral from center** (V-Ray style): Sort by distance from image center for better visual feedback.

---

## Utility Functions

### Random (`rt/utils.go`)

| Function | Purpose |
|----------|---------|
| `RandomDouble() float64` | [0, 1) |
| `RandomDoubleRange(min, max float64) float64` | [min, max) |
| `DegreesToRadians(degrees float64) float64` | Conversion |

### Render Stats (`rt/utils.go`)

| Function | Purpose |
|----------|---------|
| `PrintRenderSettings(camera, objectCount)` | Pre-render info |
| `PrintRenderStats(duration, width, height)` | Post-render stats |
| `FormatDuration(d time.Duration) string` | Human-readable time |
| `PrintMemStats()` | Memory usage report |

### Timer (`rt/utils.go`)

```go
timer := NewTimer("BVH Construction")
// ... work ...
elapsed := timer.Stop()
```

---

## File I/O

### OBJ Loader (`rt/obj_loader.go`)

```go
func LoadOBJ(filename string, mat Material) (*HittableList, error)
func LoadOBJWithTransform(filename string, mat Material, transform *Transform) (*HittableList, error)
```

Returns a `HittableList` of triangles.

### Image Loader (`rt/image_loader.go`)

```go
func LoadImage(filename string) (image.Image, error)
```

Supports PNG and JPEG.

### Image Export

```go
func (r *ProgressiveRenderer) SaveImage(filename string) error
```

Saves framebuffer as PNG.

---

## Constants and Configuration

### Render Statistics

Global struct tracking:

- Total rays cast
- BVH construction time
- Ray-AABB tests
- Ray-primitive tests
- Sample count

### Profiler (`rt/profiler.go`)

| Config Field | Type |
|--------------|------|
| `Enabled` | `bool` |
| `CPUProfile` | `bool` |
| `MemProfile` | `bool` |
| `TraceEnabled` | `bool` |
| `BlockProfile` | `bool` |
| `OutputDir` | `string` |
| `SampleRate` | `int` |

---

## Scene System

### HittableList (`rt/hittable_list.go`)

Container for multiple hittables.

| Method | Purpose |
|--------|---------|
| `Add(obj Hittable)` | Add object |
| `Clear()` | Remove all |
| `Hit(...)` | Tests all objects, returns closest |
| `BoundingBox()` | Union of all object boxes |

### Scene Functions (`rt/scenes.go`)

Pre-built test scenes:

| Function | Description |
|----------|-------------|
| `RandomScene()` | Classic "Ray Tracing in One Weekend" final scene |
| `RandomSceneWithConfig(config)` | Configurable version |
| `CornellBox()` | Cornell box with area light |
| `PrimitivesScene()` | Demo of all primitive types |
| `TextureDemo()` | Texture showcase |
| ... | (more scenes available) |

---

## Porting Priority

### Phase 1 - Foundation (Days 1-3)

1. `Vec3`, `Ray`, `Interval`, `AABB`
2. `Hittable` trait, `HitRecord`
3. `Sphere`, `Plane` primitives

### Phase 2 - Materials (Days 4-5)

1. `Material` trait
2. `Lambertian`, `Metal`, `Dielectric`
3. `SolidColor` texture

### Phase 3 - Acceleration (Days 6-8)

1. `BVHNode`, `BVHLeaf`
2. Parallel construction with `rayon`

### Phase 4 - Camera & Render (Days 9-12)

1. `Camera` with builder
2. Basic scanline renderer
3. `BucketRenderer` with parallel tiles

### Phase 5 - Advanced (Week 3+)

1. `Triangle`, OBJ loading
2. All textures
3. Transforms
4. MIS/NEE
5. Progressive multi-pass

---

## Quick Reference: Go → Rust Type Mapping

| Go Type | Rust Type |
|---------|-----------|
| `float64` | `f32` (prefer) or `f64` |
| `interface{}` | `dyn Trait` or enum |
| `*T` (pointer) | `&T`, `Box<T>`, or `Arc<T>` |
| `[]T` (slice) | `Vec<T>` or `&[T]` |
| `map[K]V` | `HashMap<K, V>` |
| `sync.Mutex` | `std::sync::Mutex` |
| `sync.WaitGroup` | `rayon` or `crossbeam::scope` |
| `atomic.Int32` | `AtomicI32` |
| `chan T` | `crossbeam::channel` |
| `go func()` | `rayon::spawn` or `std::thread::spawn` |
| `image.RGBA` | `image::RgbaImage` |
