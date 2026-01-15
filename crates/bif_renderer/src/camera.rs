//! Camera for ray generation.

use crate::{gen_f32, Ray};
use bif_math::Vec3;
use rand::RngCore;

/// Camera for generating rays into the scene.
#[derive(Clone)]
pub struct Camera {
    // Image settings
    pub image_width: u32,
    pub image_height: u32,
    pub samples_per_pixel: u32,
    pub max_depth: u32,

    // Camera positioning
    look_from: Vec3,
    look_at: Vec3,
    vup: Vec3,

    // Lens settings
    vfov: f32,          // Vertical field of view in degrees
    defocus_angle: f32, // Variation angle of rays through each pixel
    focus_dist: f32,    // Distance from camera to plane of perfect focus

    // Background color
    pub background: Vec3,

    // Cached computed values (set by initialize())
    center: Vec3,
    pixel00_loc: Vec3,
    pixel_delta_u: Vec3,
    pixel_delta_v: Vec3,
    u: Vec3,
    v: Vec3,
    w: Vec3,
    defocus_disk_u: Vec3,
    defocus_disk_v: Vec3,
    samples_scale: f32,
}

impl Camera {
    /// Create a new camera with default settings.
    pub fn new() -> Self {
        Self {
            image_width: 800,
            image_height: 450,
            samples_per_pixel: 10,
            max_depth: 50,
            look_from: Vec3::new(0.0, 0.0, 0.0),
            look_at: Vec3::new(0.0, 0.0, -1.0),
            vup: Vec3::new(0.0, 1.0, 0.0),
            vfov: 90.0,
            defocus_angle: 0.0,
            focus_dist: 1.0,
            background: Vec3::ZERO,
            // Cached values (initialized to defaults)
            center: Vec3::ZERO,
            pixel00_loc: Vec3::ZERO,
            pixel_delta_u: Vec3::ZERO,
            pixel_delta_v: Vec3::ZERO,
            u: Vec3::X,
            v: Vec3::Y,
            w: Vec3::Z,
            defocus_disk_u: Vec3::ZERO,
            defocus_disk_v: Vec3::ZERO,
            samples_scale: 0.1,
        }
    }

    /// Set image resolution.
    pub fn with_resolution(mut self, width: u32, height: u32) -> Self {
        self.image_width = width;
        self.image_height = height;
        self
    }

    /// Set quality settings.
    pub fn with_quality(mut self, samples: u32, max_depth: u32) -> Self {
        self.samples_per_pixel = samples;
        self.max_depth = max_depth;
        self
    }

    /// Set camera position.
    pub fn with_position(mut self, look_from: Vec3, look_at: Vec3, vup: Vec3) -> Self {
        self.look_from = look_from;
        self.look_at = look_at;
        self.vup = vup;
        self
    }

    /// Set lens settings.
    pub fn with_lens(mut self, vfov: f32, defocus_angle: f32, focus_dist: f32) -> Self {
        self.vfov = vfov;
        self.defocus_angle = defocus_angle;
        self.focus_dist = focus_dist;
        self
    }

    /// Set background color.
    pub fn with_background(mut self, color: Vec3) -> Self {
        self.background = color;
        self
    }

    /// Initialize the camera (must be called before generating rays).
    pub fn initialize(&mut self) {
        self.samples_scale = 1.0 / self.samples_per_pixel as f32;
        self.center = self.look_from;

        // Calculate viewport dimensions
        let theta = self.vfov.to_radians();
        let h = (theta / 2.0).tan();
        let viewport_height = 2.0 * h * self.focus_dist;
        let viewport_width = viewport_height * (self.image_width as f32 / self.image_height as f32);

        // Calculate camera basis vectors
        self.w = (self.look_from - self.look_at).normalize();
        self.u = self.vup.cross(self.w).normalize();
        self.v = self.w.cross(self.u);

        // Calculate viewport vectors
        let viewport_u = viewport_width * self.u;
        let viewport_v = -viewport_height * self.v;

        // Calculate pixel delta vectors
        self.pixel_delta_u = viewport_u / self.image_width as f32;
        self.pixel_delta_v = viewport_v / self.image_height as f32;

        // Calculate upper left pixel location
        let viewport_upper_left =
            self.center - self.focus_dist * self.w - viewport_u / 2.0 - viewport_v / 2.0;

        self.pixel00_loc = viewport_upper_left + 0.5 * (self.pixel_delta_u + self.pixel_delta_v);

        // Calculate defocus disk basis vectors
        let defocus_radius = self.focus_dist * (self.defocus_angle / 2.0).to_radians().tan();
        self.defocus_disk_u = self.u * defocus_radius;
        self.defocus_disk_v = self.v * defocus_radius;
    }

    /// Generate a ray for pixel (i, j) with random sampling.
    pub fn get_ray(&self, i: u32, j: u32, rng: &mut dyn RngCore) -> Ray {
        let offset = sample_square(rng);

        let pixel_sample = self.pixel00_loc
            + ((i as f32) + offset.x) * self.pixel_delta_u
            + ((j as f32) + offset.y) * self.pixel_delta_v;

        let ray_origin = if self.defocus_angle <= 0.0 {
            self.center
        } else {
            self.defocus_disk_sample(rng)
        };

        let ray_direction = pixel_sample - ray_origin;
        let ray_time = gen_f32(rng);

        Ray::new(ray_origin, ray_direction, ray_time)
    }

    /// Sample a point on the defocus disk.
    fn defocus_disk_sample(&self, rng: &mut dyn RngCore) -> Vec3 {
        let p = random_in_unit_disk(rng);
        self.center + p.x * self.defocus_disk_u + p.y * self.defocus_disk_v
    }

    /// Get the samples scale factor (1 / samples_per_pixel).
    pub fn samples_scale(&self) -> f32 {
        self.samples_scale
    }
}

impl Default for Camera {
    fn default() -> Self {
        Self::new()
    }
}

/// Sample a random point in the unit square [-0.5, 0.5] x [-0.5, 0.5].
fn sample_square(rng: &mut dyn RngCore) -> Vec3 {
    Vec3::new(gen_f32(rng) - 0.5, gen_f32(rng) - 0.5, 0.0)
}

/// Sample a random point in the unit disk.
fn random_in_unit_disk(rng: &mut dyn RngCore) -> Vec3 {
    loop {
        let p = Vec3::new(
            gen_f32(rng) * 2.0 - 1.0,
            gen_f32(rng) * 2.0 - 1.0,
            0.0,
        );
        if p.length_squared() < 1.0 {
            return p;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    #[test]
    fn test_camera_initialize() {
        let mut camera = Camera::new()
            .with_resolution(800, 600)
            .with_position(
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(0.0, 0.0, -1.0),
                Vec3::new(0.0, 1.0, 0.0),
            )
            .with_lens(90.0, 0.0, 1.0);

        camera.initialize();

        assert_eq!(camera.center, Vec3::ZERO);
        assert!((camera.w - Vec3::Z).length() < 0.001);
    }

    #[test]
    fn test_camera_ray_direction() {
        let mut camera = Camera::new()
            .with_resolution(100, 100)
            .with_position(Vec3::ZERO, Vec3::new(0.0, 0.0, -1.0), Vec3::Y)
            .with_lens(90.0, 0.0, 1.0);

        camera.initialize();

        let mut rng = StdRng::seed_from_u64(42);

        // Center ray should point roughly towards -Z
        let ray = camera.get_ray(50, 50, &mut rng);
        assert!(ray.direction().z < 0.0);
    }
}
