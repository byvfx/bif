//! Material trait for surface scattering.

use crate::{hittable::HitRecord, Ray};
use bif_math::Vec3;
use rand::RngCore;
use std::f32::consts::PI;

/// Color type alias (RGB values typically 0-1)
pub type Color = Vec3;

/// Result of scattering a ray off a material.
#[derive(Debug, Clone, Copy)]
pub struct ScatterResult {
    /// Color attenuation (how much light is absorbed)
    pub attenuation: Color,
    /// The scattered ray
    pub scattered: Ray,
    /// Probability density function value for this sample
    pub pdf: f32,
}

/// Trait for materials that describe how light interacts with surfaces.
///
/// Uses `&mut dyn RngCore` for object safety (required for `dyn Material`).
pub trait Material: Send + Sync {
    /// Scatter an incoming ray.
    ///
    /// Returns Some(ScatterResult) if the ray scatters, or None if absorbed.
    fn scatter(
        &self,
        ray_in: &Ray,
        rec: &HitRecord,
        rng: &mut dyn RngCore,
    ) -> Option<ScatterResult>;

    /// Evaluate BSDF for given directions (for MIS).
    ///
    /// Returns the BSDF value f(wo, wi) for the given incoming and outgoing directions.
    fn bsdf(&self, _ray_in: &Ray, _rec: &HitRecord, _scattered: &Ray) -> Color {
        // Default: uniform BSDF
        Color::splat(1.0 / PI)
    }

    /// Get PDF for the given scattered direction.
    ///
    /// Returns the probability density of scattering in direction `scattered`.
    fn pdf(&self, _ray_in: &Ray, _rec: &HitRecord, _scattered: &Ray) -> f32 {
        // Default: cosine-weighted hemisphere
        1.0 / (2.0 * PI)
    }

    /// Get emitted light from this material.
    ///
    /// Returns the color of light emitted at the given UV coordinates and point.
    /// Most materials return black (no emission).
    fn emitted(&self, _u: f32, _v: f32, _p: Vec3) -> Color {
        Color::ZERO
    }
}

// =============================================================================
// RNG helper (object-safe)
// =============================================================================

/// Generate a random f32 in [0, 1) from an RngCore.
///
/// This is needed because `dyn RngCore` can't use `Rng::gen()` directly.
#[inline]
pub fn gen_f32(rng: &mut dyn RngCore) -> f32 {
    // Same algorithm as rand's Standard distribution for f32
    let bits = rng.next_u32();
    (bits >> 8) as f32 * (1.0 / (1u32 << 24) as f32)
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
    fn scatter(
        &self,
        ray_in: &Ray,
        rec: &HitRecord,
        rng: &mut dyn RngCore,
    ) -> Option<ScatterResult> {
        // Scatter in a random direction on the hemisphere around the normal
        let mut scatter_direction = rec.normal + random_unit_vector(rng);

        // Catch degenerate scatter direction
        if scatter_direction.length_squared() < 1e-8 {
            scatter_direction = rec.normal;
        }

        let scattered = Ray::new(rec.p, scatter_direction, ray_in.time());

        // Cosine-weighted PDF: cos(theta) / pi
        let cos_theta = rec.normal.dot(scatter_direction.normalize()).max(0.0);
        let pdf = (cos_theta / PI).max(0.0001); // Clamp to prevent div-by-zero

        Some(ScatterResult {
            attenuation: self.albedo,
            scattered,
            pdf,
        })
    }

    fn bsdf(&self, _ray_in: &Ray, rec: &HitRecord, scattered: &Ray) -> Color {
        let cos_theta = rec.normal.dot(scattered.direction().normalize()).max(0.0);
        self.albedo * cos_theta / PI
    }

    fn pdf(&self, _ray_in: &Ray, rec: &HitRecord, scattered: &Ray) -> f32 {
        let cos_theta = rec.normal.dot(scattered.direction().normalize()).max(0.0);
        (cos_theta / PI).max(0.0001)
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
    fn scatter(
        &self,
        ray_in: &Ray,
        rec: &HitRecord,
        rng: &mut dyn RngCore,
    ) -> Option<ScatterResult> {
        let reflected = reflect(ray_in.direction().normalize(), rec.normal);
        let scattered_dir = reflected + self.fuzz * random_unit_vector(rng);

        // Only scatter if the reflected ray is in the same hemisphere as the normal
        if scattered_dir.dot(rec.normal) > 0.0 {
            let scattered = Ray::new(rec.p, scattered_dir, ray_in.time());
            // Perfect specular has delta PDF, use 1.0 as placeholder
            Some(ScatterResult {
                attenuation: self.albedo,
                scattered,
                pdf: 1.0,
            })
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
    fn scatter(
        &self,
        ray_in: &Ray,
        rec: &HitRecord,
        rng: &mut dyn RngCore,
    ) -> Option<ScatterResult> {
        let attenuation = Color::ONE;
        let refraction_ratio = if rec.front_face {
            1.0 / self.ior
        } else {
            self.ior
        };

        let unit_direction = ray_in.direction().normalize();
        let cos_theta = (-unit_direction).dot(rec.normal).min(1.0);
        let sin_theta = (1.0 - cos_theta * cos_theta).sqrt();

        // Check for total internal reflection
        let cannot_refract = refraction_ratio * sin_theta > 1.0;

        let direction =
            if cannot_refract || Self::reflectance(cos_theta, refraction_ratio) > gen_f32(rng) {
                reflect(unit_direction, rec.normal)
            } else {
                refract(unit_direction, rec.normal, refraction_ratio)
            };

        let scattered = Ray::new(rec.p, direction, ray_in.time());
        // Perfect specular/transmission has delta PDF, use 1.0 as placeholder
        Some(ScatterResult {
            attenuation,
            scattered,
            pdf: 1.0,
        })
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
    fn scatter(
        &self,
        _ray_in: &Ray,
        _rec: &HitRecord,
        _rng: &mut dyn RngCore,
    ) -> Option<ScatterResult> {
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
pub fn reflect(v: Vec3, n: Vec3) -> Vec3 {
    v - 2.0 * v.dot(n) * n
}

/// Refract a vector through a surface.
#[inline]
pub fn refract(uv: Vec3, n: Vec3, etai_over_etat: f32) -> Vec3 {
    let cos_theta = (-uv).dot(n).min(1.0);
    let r_out_perp = etai_over_etat * (uv + cos_theta * n);
    let r_out_parallel = -(1.0 - r_out_perp.length_squared()).abs().sqrt() * n;
    r_out_perp + r_out_parallel
}

/// Generate a random unit vector on the unit sphere.
pub fn random_unit_vector(rng: &mut dyn RngCore) -> Vec3 {
    // Use rejection sampling for uniform distribution on sphere
    loop {
        let v = Vec3::new(
            gen_f32(rng) * 2.0 - 1.0,
            gen_f32(rng) * 2.0 - 1.0,
            gen_f32(rng) * 2.0 - 1.0,
        );
        let len_sq = v.length_squared();
        if len_sq > 1e-6 && len_sq <= 1.0 {
            return v / len_sq.sqrt();
        }
    }
}

/// Generate a random vector in the hemisphere around a normal (uniform).
pub fn random_in_hemisphere(normal: Vec3, rng: &mut dyn RngCore) -> Vec3 {
    let unit = random_unit_vector(rng);
    if unit.dot(normal) > 0.0 {
        unit
    } else {
        -unit
    }
}

/// Generate a cosine-weighted random direction in the hemisphere around a normal.
///
/// Uses Malley's method: sample uniformly on disk, project to hemisphere.
/// PDF = cos(theta) / PI
pub fn cosine_weighted_hemisphere(normal: Vec3, rng: &mut dyn RngCore) -> Vec3 {
    let r1 = gen_f32(rng);
    let r2 = gen_f32(rng);

    // Sample point on unit disk
    let sqrt_r1 = r1.sqrt();
    let theta = 2.0 * PI * r2;
    let x = sqrt_r1 * theta.cos();
    let y = sqrt_r1 * theta.sin();
    // Project to hemisphere: z = sqrt(1 - r1)
    let z = (1.0 - r1).sqrt();

    // Build orthonormal basis from normal
    let (tangent, bitangent) = build_tangent_basis(normal);

    // Transform to world space
    x * tangent + y * bitangent + z * normal
}

/// Build orthonormal tangent/bitangent from a normal.
fn build_tangent_basis(n: Vec3) -> (Vec3, Vec3) {
    let sign = if n.z >= 0.0 { 1.0 } else { -1.0 };
    let a = -1.0 / (sign + n.z);
    let b = n.x * n.y * a;

    let tangent = Vec3::new(1.0 + sign * n.x * n.x * a, sign * b, -sign * n.x);
    let bitangent = Vec3::new(b, sign + n.y * n.y * a, -n.y);

    (tangent, bitangent)
}
