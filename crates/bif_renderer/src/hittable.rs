//! Hittable trait and HitRecord for ray-object intersection.

use crate::{Material, Ray, ScatterResult};
use bif_math::{Aabb, Interval, Vec3};
use rand::RngCore;

/// A dummy material used for HitRecord::default().
/// Always absorbs light (returns None from scatter).
struct DummyMaterial;

impl Material for DummyMaterial {
    fn scatter(&self, _ray_in: &Ray, _rec: &HitRecord, _rng: &mut dyn RngCore) -> Option<ScatterResult> {
        None
    }
}

/// Static dummy material instance for Default impl.
static DUMMY_MATERIAL: DummyMaterial = DummyMaterial;

/// Record of a ray-object intersection.
#[derive(Clone)]
pub struct HitRecord<'a> {
    /// Point of intersection
    pub p: Vec3,
    /// Surface normal at intersection (always points against ray)
    pub normal: Vec3,
    /// Material at the intersection point
    pub material: &'a dyn Material,
    /// UV texture coordinates
    pub u: f32,
    pub v: f32,
    /// Parameter t where the intersection occurs
    pub t: f32,
    /// Whether the ray hit the front face (outside) of the surface
    pub front_face: bool,
}

impl<'a> Default for HitRecord<'a> {
    fn default() -> Self {
        Self {
            p: Vec3::ZERO,
            normal: Vec3::ZERO,
            material: &DUMMY_MATERIAL,
            u: 0.0,
            v: 0.0,
            t: 0.0,
            front_face: false,
        }
    }
}

impl<'a> HitRecord<'a> {
    /// Set the face normal based on ray direction and outward normal.
    ///
    /// The normal is always stored pointing against the ray direction,
    /// so we need to track whether we hit the front or back face.
    pub fn set_face_normal(&mut self, ray: &Ray, outward_normal: Vec3) {
        // If the ray and normal point in the same direction, we're inside
        self.front_face = ray.direction().dot(outward_normal) < 0.0;

        // Normal always points against the ray
        self.normal = if self.front_face {
            outward_normal
        } else {
            -outward_normal
        };
    }
}

/// Trait for objects that can be hit by rays.
pub trait Hittable: Send + Sync {
    /// Test if a ray hits this object within the given interval.
    ///
    /// Returns true if hit, and fills in the hit record.
    fn hit<'a>(&'a self, ray: &Ray, ray_t: Interval, rec: &mut HitRecord<'a>) -> bool;

    /// Get the axis-aligned bounding box of this object.
    fn bounding_box(&self) -> Aabb;
}

/// A list of hittable objects.
pub struct HittableList {
    objects: Vec<Box<dyn Hittable>>,
    bbox: Aabb,
}

impl HittableList {
    /// Create a new empty hittable list.
    pub fn new() -> Self {
        Self {
            objects: Vec::new(),
            bbox: Aabb::EMPTY,
        }
    }

    /// Add an object to the list.
    pub fn add(&mut self, object: Box<dyn Hittable>) {
        self.bbox = Aabb::surrounding(&self.bbox, &object.bounding_box());
        self.objects.push(object);
    }

    /// Clear all objects from the list.
    pub fn clear(&mut self) {
        self.objects.clear();
        self.bbox = Aabb::EMPTY;
    }

    /// Get the number of objects.
    pub fn len(&self) -> usize {
        self.objects.len()
    }

    /// Check if the list is empty.
    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }
}

impl Default for HittableList {
    fn default() -> Self {
        Self::new()
    }
}

impl Hittable for HittableList {
    fn hit<'a>(&'a self, ray: &Ray, ray_t: Interval, rec: &mut HitRecord<'a>) -> bool {
        let mut hit_anything = false;
        let mut closest_so_far = ray_t.max;

        for object in &self.objects {
            let interval = Interval::new(ray_t.min, closest_so_far);
            if object.hit(ray, interval, rec) {
                hit_anything = true;
                closest_so_far = rec.t;
            }
        }

        hit_anything
    }

    fn bounding_box(&self) -> Aabb {
        self.bbox
    }
}
