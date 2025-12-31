//! Material trait for surface scattering.

use bif_math::Vec3;
use crate::{Ray, hittable::HitRecord};

/// Color type alias (RGB values typically 0-1)
pub type Color = Vec3;

/// Trait for materials that describe how light interacts with surfaces.
pub trait Material: Send + Sync {
    /// Scatter an incoming ray.
    /// 
    /// Returns Some((attenuation, scattered_ray)) if the ray scatters,
    /// or None if the ray is absorbed.
    fn scatter(&self, ray_in: &Ray, rec: &HitRecord) -> Option<(Color, Ray)>;
    
    /// Get emitted light from this material.
    /// 
    /// Returns the color of light emitted at the given UV coordinates and point.
    /// Most materials return black (no emission).
    fn emitted(&self, _u: f32, _v: f32, _p: Vec3) -> Color {
        Color::ZERO
    }
}

/// Material properties for optimization hints.
#[derive(Debug, Clone, Copy, Default)]
pub struct MaterialProperties {
    /// True if the material is perfectly specular (mirror, glass)
    pub is_pure_specular: bool,
    /// True if the material emits light
    pub is_emissive: bool,
    /// True if the material can use Next Event Estimation
    pub can_use_nee: bool,
}

/// Lambertian (diffuse) material.
#[derive(Clone)]
pub struct Lambertian {
    albedo: Color,
}

impl Lambertian {
    /// Create a new Lambertian material with the given albedo color.
    pub fn new(albedo: Color) -> Self {
        Self { albedo }
    }

    /// Get the material properties.
    pub fn properties() -> MaterialProperties {
        MaterialProperties {
            is_pure_specular: false,
            is_emissive: false,
            can_use_nee: true,
        }
    }
}

impl Material for Lambertian {
    fn scatter(&self, ray_in: &Ray, rec: &HitRecord) -> Option<(Color, Ray)> {
        // Scatter in a random direction on the hemisphere around the normal
        let mut scatter_direction = rec.normal + random_unit_vector();
        
        // Catch degenerate scatter direction
        if scatter_direction.length_squared() < 1e-8 {
            scatter_direction = rec.normal;
        }
        
        let scattered = Ray::new(rec.p, scatter_direction, ray_in.time());
        Some((self.albedo, scattered))
    }
}

/// Metal (specular) material.
pub struct Metal {
    albedo: Color,
    fuzz: f32,
}

impl Metal {
    /// Create a new Metal material.
    /// 
    /// - `albedo`: The color of the metal
    /// - `fuzz`: Roughness, 0.0 = perfect mirror, 1.0 = very rough
    pub fn new(albedo: Color, fuzz: f32) -> Self {
        Self {
            albedo,
            fuzz: fuzz.clamp(0.0, 1.0),
        }
    }
}

impl Material for Metal {
    fn scatter(&self, ray_in: &Ray, rec: &HitRecord) -> Option<(Color, Ray)> {
        let reflected = reflect(ray_in.direction().normalize(), rec.normal);
        let scattered_dir = reflected + self.fuzz * random_unit_vector();
        
        // Only scatter if the reflected ray is in the same hemisphere as the normal
        if scattered_dir.dot(rec.normal) > 0.0 {
            let scattered = Ray::new(rec.p, scattered_dir, ray_in.time());
            Some((self.albedo, scattered))
        } else {
            None
        }
    }
}

/// Dielectric (glass) material.
pub struct Dielectric {
    /// Index of refraction
    ior: f32,
}

impl Dielectric {
    /// Create a new Dielectric material.
    /// 
    /// - `ior`: Index of refraction (1.0 = air, 1.5 = glass, 2.4 = diamond)
    pub fn new(ior: f32) -> Self {
        Self { ior }
    }

    /// Schlick's approximation for reflectance
    fn reflectance(cosine: f32, ior: f32) -> f32 {
        let r0 = ((1.0 - ior) / (1.0 + ior)).powi(2);
        r0 + (1.0 - r0) * (1.0 - cosine).powi(5)
    }
}

impl Material for Dielectric {
    fn scatter(&self, ray_in: &Ray, rec: &HitRecord) -> Option<(Color, Ray)> {
        let attenuation = Color::ONE;
        let refraction_ratio = if rec.front_face { 1.0 / self.ior } else { self.ior };

        let unit_direction = ray_in.direction().normalize();
        let cos_theta = (-unit_direction).dot(rec.normal).min(1.0);
        let sin_theta = (1.0 - cos_theta * cos_theta).sqrt();

        // Check for total internal reflection
        let cannot_refract = refraction_ratio * sin_theta > 1.0;
        
        let direction = if cannot_refract || Self::reflectance(cos_theta, refraction_ratio) > rand::random() {
            reflect(unit_direction, rec.normal)
        } else {
            refract(unit_direction, rec.normal, refraction_ratio)
        };

        let scattered = Ray::new(rec.p, direction, ray_in.time());
        Some((attenuation, scattered))
    }
}

/// Diffuse light emitter.
pub struct DiffuseLight {
    emit: Color,
}

impl DiffuseLight {
    /// Create a new diffuse light with the given emission color.
    pub fn new(emit: Color) -> Self {
        Self { emit }
    }
}

impl Material for DiffuseLight {
    fn scatter(&self, _ray_in: &Ray, _rec: &HitRecord) -> Option<(Color, Ray)> {
        // Lights don't scatter rays
        None
    }

    fn emitted(&self, _u: f32, _v: f32, _p: Vec3) -> Color {
        self.emit
    }
}

// =============================================================================
// Helper functions
// =============================================================================

/// Reflect a vector about a normal.
#[inline]
fn reflect(v: Vec3, n: Vec3) -> Vec3 {
    v - 2.0 * v.dot(n) * n
}

/// Refract a vector through a surface.
#[inline]
fn refract(uv: Vec3, n: Vec3, etai_over_etat: f32) -> Vec3 {
    let cos_theta = (-uv).dot(n).min(1.0);
    let r_out_perp = etai_over_etat * (uv + cos_theta * n);
    let r_out_parallel = -(1.0 - r_out_perp.length_squared()).abs().sqrt() * n;
    r_out_perp + r_out_parallel
}

/// Generate a random unit vector on the unit sphere.
fn random_unit_vector() -> Vec3 {
    // Use rejection sampling for uniform distribution on sphere
    loop {
        let v = Vec3::new(
            rand::random::<f32>() * 2.0 - 1.0,
            rand::random::<f32>() * 2.0 - 1.0,
            rand::random::<f32>() * 2.0 - 1.0,
        );
        let len_sq = v.length_squared();
        if len_sq > 1e-6 && len_sq <= 1.0 {
            return v / len_sq.sqrt();
        }
    }
}
