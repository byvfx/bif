//! Disney Principled BSDF implementation.
//!
//! Based on the 2012 Disney paper "Physically Based Shading at Disney"
//! and the 2015 extension for clearcoat and sheen.

use crate::material::{random_in_hemisphere, reflect, Color};
use crate::{hittable::HitRecord, Material, Ray};
use bif_math::Vec3;
use std::f32::consts::PI;

/// Disney Principled BSDF material.
///
/// A physically-based material with intuitive artist-friendly parameters.
#[derive(Clone)]
pub struct DisneyBSDF {
    /// Base color (albedo for dielectrics, reflectance for metals)
    pub base_color: Color,

    /// Metallic: 0 = dielectric, 1 = metal
    pub metallic: f32,

    /// Roughness: 0 = smooth/glossy, 1 = rough/diffuse
    pub roughness: f32,

    /// Specular: controls Fresnel reflectance at normal incidence
    pub specular: f32,

    /// Specular tint: tints the specular towards base_color
    pub specular_tint: f32,

    /// Sheen: additional grazing component for cloth-like materials
    pub sheen: f32,

    /// Sheen tint: tints the sheen towards base_color
    pub sheen_tint: f32,

    /// Clearcoat: second specular lobe for car paint, lacquered wood
    pub clearcoat: f32,

    /// Clearcoat gloss: 0 = satin, 1 = gloss
    pub clearcoat_gloss: f32,

    /// Subsurface: blend to subsurface approximation
    pub subsurface: f32,

    /// Anisotropic: aspect ratio for anisotropic reflection
    pub anisotropic: f32,
}

impl Default for DisneyBSDF {
    fn default() -> Self {
        Self {
            base_color: Color::new(0.8, 0.8, 0.8),
            metallic: 0.0,
            roughness: 0.5,
            specular: 0.5,
            specular_tint: 0.0,
            sheen: 0.0,
            sheen_tint: 0.5,
            clearcoat: 0.0,
            clearcoat_gloss: 1.0,
            subsurface: 0.0,
            anisotropic: 0.0,
        }
    }
}

impl DisneyBSDF {
    /// Create a new Disney BSDF with default parameters.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a simple diffuse material.
    pub fn diffuse(color: Color) -> Self {
        Self {
            base_color: color,
            metallic: 0.0,
            roughness: 1.0,
            specular: 0.0,
            ..Default::default()
        }
    }

    /// Create a metallic material.
    pub fn metal(color: Color, roughness: f32) -> Self {
        Self {
            base_color: color,
            metallic: 1.0,
            roughness,
            specular: 1.0,
            ..Default::default()
        }
    }

    /// Create a glossy plastic-like material.
    pub fn plastic(color: Color, roughness: f32) -> Self {
        Self {
            base_color: color,
            metallic: 0.0,
            roughness,
            specular: 0.5,
            ..Default::default()
        }
    }

    /// Builder method to set base color.
    pub fn with_base_color(mut self, color: Color) -> Self {
        self.base_color = color;
        self
    }

    /// Builder method to set metallic.
    pub fn with_metallic(mut self, metallic: f32) -> Self {
        self.metallic = metallic.clamp(0.0, 1.0);
        self
    }

    /// Builder method to set roughness.
    pub fn with_roughness(mut self, roughness: f32) -> Self {
        self.roughness = roughness.clamp(0.0, 1.0);
        self
    }

    /// Builder method to set specular.
    pub fn with_specular(mut self, specular: f32) -> Self {
        self.specular = specular.clamp(0.0, 1.0);
        self
    }
}

/// Convert from bif_core::Material (UsdPreviewSurface-based) to DisneyBSDF.
///
/// This maps the USD material properties to Disney BSDF parameters:
/// - diffuse_color → base_color
/// - metallic → metallic
/// - roughness → roughness
/// - specular → specular
///
/// Note: Texture support requires additional integration (Phase 8).
impl From<&bif_core::Material> for DisneyBSDF {
    fn from(mat: &bif_core::Material) -> Self {
        Self {
            base_color: mat.diffuse_color,
            metallic: mat.metallic,
            roughness: mat.roughness,
            specular: mat.specular,
            specular_tint: 0.0,
            sheen: 0.0,
            sheen_tint: 0.5,
            clearcoat: 0.0,
            clearcoat_gloss: 1.0,
            subsurface: 0.0,
            anisotropic: 0.0,
        }
    }
}

impl Material for DisneyBSDF {
    fn scatter(&self, ray_in: &Ray, rec: &HitRecord) -> Option<(Color, Ray)> {
        let wo = -ray_in.direction().normalize();
        let n = rec.normal;

        // Decide between diffuse and specular based on material parameters
        let diffuse_weight = (1.0 - self.metallic) * (1.0 - self.specular * 0.5);
        let specular_weight = 1.0 - diffuse_weight;

        let do_diffuse =
            rand::random::<f32>() < diffuse_weight / (diffuse_weight + specular_weight);

        if do_diffuse {
            // Diffuse scattering (Burley diffuse approximation)
            self.scatter_diffuse(wo, n, rec.p, ray_in.time())
        } else {
            // Specular scattering (GGX microfacet)
            self.scatter_specular(wo, n, rec.p, ray_in.time())
        }
    }
}

impl DisneyBSDF {
    /// Scatter with diffuse (Burley) lobe.
    fn scatter_diffuse(
        &self,
        wo: Vec3,
        n: Vec3,
        hit_point: Vec3,
        time: f32,
    ) -> Option<(Color, Ray)> {
        // Sample cosine-weighted hemisphere
        let wi = random_in_hemisphere(n);

        // Burley diffuse
        let n_dot_l = n.dot(wi).max(0.0);
        let n_dot_v = n.dot(wo).max(0.0);

        if n_dot_l <= 0.0 {
            return None;
        }

        // Fresnel-weighted diffuse (Burley 2012)
        let h = (wo + wi).normalize();
        let l_dot_h = wi.dot(h).max(0.0);

        let fd90 = 0.5 + 2.0 * self.roughness * l_dot_h * l_dot_h;
        let fl = schlick_weight(n_dot_l);
        let fv = schlick_weight(n_dot_v);
        let fd = lerp(1.0, fd90, fl) * lerp(1.0, fd90, fv);

        // Subsurface approximation blend
        let fss90 = l_dot_h * l_dot_h * self.roughness;
        let fss = lerp(1.0, fss90, fl) * lerp(1.0, fss90, fv);
        let ss = 1.25 * (fss * (1.0 / (n_dot_l + n_dot_v) - 0.5) + 0.5);

        let diffuse = lerp(fd, ss, self.subsurface);

        // Sheen
        let sheen = if self.sheen > 0.0 {
            let c_tint = if self.base_color.length_squared() > 0.0 {
                self.base_color / luminance(self.base_color)
            } else {
                Color::ONE
            };
            let c_sheen = lerp3(Color::ONE, c_tint, self.sheen_tint);
            schlick_weight(l_dot_h) * self.sheen * c_sheen
        } else {
            Color::ZERO
        };

        let attenuation = self.base_color * diffuse / PI + sheen;
        let scattered = Ray::new(hit_point, wi, time);

        Some((attenuation, scattered))
    }

    /// Scatter with specular (GGX) lobe.
    fn scatter_specular(
        &self,
        wo: Vec3,
        n: Vec3,
        hit_point: Vec3,
        time: f32,
    ) -> Option<(Color, Ray)> {
        // Use GGX importance sampling
        let alpha = self.roughness * self.roughness;
        let alpha = alpha.max(0.001); // Prevent division by zero

        // Sample GGX microfacet normal
        let h = sample_ggx(n, alpha);
        let wi = reflect(-wo, h);

        // Check if scattering direction is valid
        let n_dot_l = n.dot(wi);
        if n_dot_l <= 0.0 {
            return None;
        }

        let n_dot_v = n.dot(wo).max(0.0);
        let n_dot_h = n.dot(h).max(0.0);
        let l_dot_h = wi.dot(h).max(0.0);

        // GGX distribution (computed for reference, cancels in importance sampling)
        let _d = ggx_d(n_dot_h, alpha);

        // Schlick-GGX geometry (Smith)
        let g = smith_g_ggx(n_dot_l, n_dot_v, alpha);

        // Fresnel
        let f0 = self.fresnel_0();
        let f = schlick_fresnel3(f0, l_dot_h);

        // Specular BRDF: D * G * F / (4 * NdotL * NdotV)
        // But since we're importance sampling D, we need to adjust the weight
        let weight = g * l_dot_h / (n_dot_h * n_dot_v);

        let attenuation = f * weight.max(0.0);
        let scattered = Ray::new(hit_point, wi, time);

        Some((attenuation, scattered))
    }

    /// Compute F0 (Fresnel at normal incidence) based on material parameters.
    fn fresnel_0(&self) -> Color {
        // For dielectrics, F0 is based on specular parameter (maps to IOR)
        // specular=0.5 corresponds to IOR=1.5 (common glass/plastic)
        let dielectric_f0 = 0.08 * self.specular;

        // Tint the specular towards base color if specular_tint > 0
        let c_tint = if self.base_color.length_squared() > 0.0 {
            self.base_color / luminance(self.base_color)
        } else {
            Color::ONE
        };
        let c_spec = lerp3(
            Color::new(dielectric_f0, dielectric_f0, dielectric_f0),
            dielectric_f0 * c_tint,
            self.specular_tint,
        );

        // Blend between dielectric and metallic
        lerp3(c_spec, self.base_color, self.metallic)
    }
}

// =============================================================================
// Helper functions
// =============================================================================

/// Luminance of a color (Rec. 709).
#[inline]
fn luminance(c: Color) -> f32 {
    0.2126 * c.x + 0.7152 * c.y + 0.0722 * c.z
}

/// Linear interpolation.
#[inline]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + t * (b - a)
}

/// Linear interpolation for colors.
#[inline]
fn lerp3(a: Color, b: Color, t: f32) -> Color {
    a + t * (b - a)
}

/// Schlick weight for Fresnel.
#[inline]
fn schlick_weight(cos_theta: f32) -> f32 {
    let x = (1.0 - cos_theta).clamp(0.0, 1.0);
    let x2 = x * x;
    x2 * x2 * x // (1 - cos_theta)^5
}

/// Schlick Fresnel approximation.
#[inline]
fn schlick_fresnel3(f0: Color, cos_theta: f32) -> Color {
    f0 + (Color::ONE - f0) * schlick_weight(cos_theta)
}

/// GGX/Trowbridge-Reitz distribution.
#[inline]
fn ggx_d(n_dot_h: f32, alpha: f32) -> f32 {
    let a2 = alpha * alpha;
    let denom = n_dot_h * n_dot_h * (a2 - 1.0) + 1.0;
    a2 / (PI * denom * denom)
}

/// Smith G for GGX.
#[inline]
fn smith_g_ggx(n_dot_l: f32, n_dot_v: f32, alpha: f32) -> f32 {
    let a2 = alpha * alpha;
    let g1_l = 2.0 * n_dot_l / (n_dot_l + (a2 + (1.0 - a2) * n_dot_l * n_dot_l).sqrt());
    let g1_v = 2.0 * n_dot_v / (n_dot_v + (a2 + (1.0 - a2) * n_dot_v * n_dot_v).sqrt());
    g1_l * g1_v
}

/// Sample GGX microfacet normal in world space.
fn sample_ggx(n: Vec3, alpha: f32) -> Vec3 {
    let u1: f32 = rand::random();
    let u2: f32 = rand::random();

    // Sample half vector in tangent space
    let theta = (alpha * u1.sqrt() / (1.0 - u1).sqrt()).atan();
    let phi = 2.0 * PI * u2;

    let sin_theta = theta.sin();
    let cos_theta = theta.cos();
    let sin_phi = phi.sin();
    let cos_phi = phi.cos();

    // Local half vector
    let h_local = Vec3::new(sin_theta * cos_phi, sin_theta * sin_phi, cos_theta);

    // Transform to world space
    let (tangent, bitangent) = build_orthonormal_basis(n);
    h_local.x * tangent + h_local.y * bitangent + h_local.z * n
}

/// Build an orthonormal basis from a normal vector.
fn build_orthonormal_basis(n: Vec3) -> (Vec3, Vec3) {
    let sign = if n.z >= 0.0 { 1.0 } else { -1.0 };
    let a = -1.0 / (sign + n.z);
    let b = n.x * n.y * a;

    let tangent = Vec3::new(1.0 + sign * n.x * n.x * a, sign * b, -sign * n.x);
    let bitangent = Vec3::new(b, sign + n.y * n.y * a, -n.y);

    (tangent, bitangent)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_disney_default() {
        let mat = DisneyBSDF::new();
        assert!((mat.metallic - 0.0).abs() < 0.001);
        assert!((mat.roughness - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_disney_metal() {
        let mat = DisneyBSDF::metal(Color::new(1.0, 0.8, 0.0), 0.1);
        assert!((mat.metallic - 1.0).abs() < 0.001);
        assert!((mat.roughness - 0.1).abs() < 0.001);
    }

    #[test]
    fn test_luminance() {
        assert!((luminance(Color::ONE) - 1.0).abs() < 0.001);
        assert!((luminance(Color::ZERO) - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_schlick_weight() {
        assert!((schlick_weight(1.0) - 0.0).abs() < 0.001);
        assert!((schlick_weight(0.0) - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_orthonormal_basis() {
        let n = Vec3::new(0.0, 1.0, 0.0);
        let (t, b) = build_orthonormal_basis(n);

        // Check orthogonality
        assert!(t.dot(n).abs() < 0.001);
        assert!(b.dot(n).abs() < 0.001);
        assert!(t.dot(b).abs() < 0.001);

        // Check unit length
        assert!((t.length() - 1.0).abs() < 0.001);
        assert!((b.length() - 1.0).abs() < 0.001);
    }
}
