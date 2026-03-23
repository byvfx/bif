//! OpenPBR Surface material implementation.
//!
//! Based on the Academy Software Foundation OpenPBR Surface v1.1 specification.
//! Uses physically-grounded IOR-based Fresnel instead of Disney's abstract
//! specular parameter. GGX microfacet distribution + Smith geometry unchanged.
//!
//! Supports optional texture maps for base_color, specular_roughness,
//! base_metalness, normals, and geometry_opacity.

use crate::material::{
    cosine_weighted_hemisphere, gen_f32, reflect, refract, Color, ScatterResult,
};
use crate::{hittable::HitRecord, Material, Ray};
use bif_core::texture::Texture;
use bif_math::{build_orthonormal_basis, Vec3};
use rand::RngCore;
use std::f32::consts::PI;
use std::path::Path;
use std::sync::Arc;

/// OpenPBR Surface material.
///
/// A physically-based material following the ASWF OpenPBR Surface v1.1 spec.
/// Uses IOR-based Fresnel for dielectric specular instead of Disney's abstract
/// specular parameter.
#[derive(Clone)]
pub struct OpenPbrSurface {
    // === Base ===
    /// Base layer weight (0-1). Scales diffuse and metal F0.
    pub base_weight: f32,

    /// Base color (albedo for dielectrics, reflectance for metals).
    pub base_color: Color,

    /// Base diffuse roughness (Oren-Nayar). Stub: 0 = Burley diffuse.
    #[allow(dead_code)]
    pub base_diffuse_roughness: f32,

    /// Metalness: 0 = dielectric, 1 = metal.
    pub base_metalness: f32,

    // === Specular ===
    /// Specular weight (scales dielectric reflection).
    pub specular_weight: f32,

    /// Specular color (edge tint). Stub: kept as (1,1,1), generalized Schlick later.
    #[allow(dead_code)]
    pub specular_color: Color,

    /// Specular roughness: 0 = smooth/glossy, 1 = rough.
    pub specular_roughness: f32,

    /// Index of refraction for dielectric specular. 1.5 = glass/plastic.
    pub specular_ior: f32,

    /// Specular roughness anisotropy. Stub: not evaluated in shader.
    #[allow(dead_code)]
    pub specular_roughness_anisotropy: f32,

    // === Coat (stub) ===
    /// Coat weight.
    #[allow(dead_code)]
    pub coat_weight: f32,

    /// Coat color tint.
    #[allow(dead_code)]
    pub coat_color: Color,

    /// Coat roughness.
    #[allow(dead_code)]
    pub coat_roughness: f32,

    /// Coat IOR.
    #[allow(dead_code)]
    pub coat_ior: f32,

    // === Fuzz ===
    /// Fuzz weight (grazing sheen for cloth-like materials).
    pub fuzz_weight: f32,

    /// Fuzz color.
    pub fuzz_color: Color,

    /// Fuzz roughness.
    #[allow(dead_code)]
    pub fuzz_roughness: f32,

    // === Subsurface (stub) ===
    /// Subsurface scattering weight. Basic approximation blend.
    pub subsurface_weight: f32,

    // === Emission ===
    /// Emission luminance in nits (stub: stored but not evaluated in path tracer).
    #[allow(dead_code)]
    pub emission_luminance: f32,

    /// Emission color.
    #[allow(dead_code)]
    pub emission_color: Color,

    // === Transmission ===
    /// Transmission weight (0=opaque, 1=fully transmissive glass).
    pub transmission_weight: f32,

    // === Geometry ===
    /// Opacity (0=transparent, 1=opaque).
    pub geometry_opacity: f32,

    // === Textures ===
    /// Base color / diffuse texture.
    pub base_color_texture: Option<Arc<Texture>>,

    /// Specular roughness texture (samples from R channel).
    pub specular_roughness_texture: Option<Arc<Texture>>,

    /// Base metalness texture (samples from R channel).
    pub base_metalness_texture: Option<Arc<Texture>>,

    /// Normal map texture.
    pub normal_texture: Option<Arc<Texture>>,

    /// Geometry opacity texture (samples from R channel).
    pub geometry_opacity_texture: Option<Arc<Texture>>,
}

impl Default for OpenPbrSurface {
    fn default() -> Self {
        Self {
            base_weight: 1.0,
            base_color: Color::new(0.8, 0.8, 0.8),
            base_diffuse_roughness: 0.0,
            base_metalness: 0.0,
            specular_weight: 1.0,
            specular_color: Color::ONE,
            specular_roughness: 0.3,
            specular_ior: 1.5,
            specular_roughness_anisotropy: 0.0,
            coat_weight: 0.0,
            coat_color: Color::ONE,
            coat_roughness: 0.0,
            coat_ior: 1.6,
            fuzz_weight: 0.0,
            fuzz_color: Color::ONE,
            fuzz_roughness: 0.5,
            subsurface_weight: 0.0,
            emission_luminance: 0.0,
            emission_color: Color::ONE,
            transmission_weight: 0.0,
            geometry_opacity: 1.0,
            base_color_texture: None,
            specular_roughness_texture: None,
            base_metalness_texture: None,
            normal_texture: None,
            geometry_opacity_texture: None,
        }
    }
}

impl OpenPbrSurface {
    /// Create a new OpenPBR Surface with default parameters.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a simple diffuse material.
    pub fn diffuse(color: Color) -> Self {
        Self {
            base_color: color,
            base_metalness: 0.0,
            specular_roughness: 1.0,
            specular_weight: 0.0,
            ..Default::default()
        }
    }

    /// Create a metallic material.
    pub fn metal(color: Color, roughness: f32) -> Self {
        Self {
            base_color: color,
            base_metalness: 1.0,
            specular_roughness: roughness,
            specular_weight: 1.0,
            ..Default::default()
        }
    }

    /// Create a glass material with full transmission.
    pub fn glass(ior: f32) -> Self {
        Self {
            base_color: Color::ONE,
            base_metalness: 0.0,
            specular_roughness: 0.0,
            specular_weight: 1.0,
            specular_ior: ior,
            transmission_weight: 1.0,
            ..Default::default()
        }
    }

    /// Create a glossy plastic-like material.
    pub fn plastic(color: Color, roughness: f32) -> Self {
        Self {
            base_color: color,
            base_metalness: 0.0,
            specular_roughness: roughness,
            specular_weight: 1.0,
            ..Default::default()
        }
    }

    /// Builder: set base color.
    pub fn with_base_color(mut self, color: Color) -> Self {
        self.base_color = color;
        self
    }

    /// Builder: set base metalness.
    pub fn with_base_metalness(mut self, metalness: f32) -> Self {
        self.base_metalness = metalness.clamp(0.0, 1.0);
        self
    }

    /// Builder: set specular roughness.
    pub fn with_specular_roughness(mut self, roughness: f32) -> Self {
        self.specular_roughness = roughness.clamp(0.0, 1.0);
        self
    }

    /// Builder: set specular weight.
    pub fn with_specular_weight(mut self, weight: f32) -> Self {
        self.specular_weight = weight.clamp(0.0, 1.0);
        self
    }

    /// Builder: set specular IOR.
    pub fn with_specular_ior(mut self, ior: f32) -> Self {
        self.specular_ior = ior.max(1.0);
        self
    }
}

/// Convert from bif_core::Material to OpenPbrSurface.
///
/// Maps OpenPBR material fields directly. Does not load textures.
/// Use `from_material_with_textures` with a TextureCache for full rendering.
impl From<&bif_core::Material> for OpenPbrSurface {
    fn from(mat: &bif_core::Material) -> Self {
        Self {
            base_weight: 1.0,
            base_color: mat.base_color,
            base_diffuse_roughness: 0.0,
            base_metalness: mat.base_metalness,
            specular_weight: mat.specular_weight,
            specular_color: Color::ONE,
            specular_roughness: mat.specular_roughness,
            specular_ior: mat.specular_ior,
            specular_roughness_anisotropy: 0.0,
            coat_weight: 0.0,
            coat_color: Color::ONE,
            coat_roughness: 0.0,
            coat_ior: 1.6,
            fuzz_weight: 0.0,
            fuzz_color: Color::ONE,
            fuzz_roughness: 0.5,
            subsurface_weight: 0.0,
            emission_luminance: mat.emission_luminance,
            emission_color: Color::new(
                mat.emission_color.x,
                mat.emission_color.y,
                mat.emission_color.z,
            ),
            transmission_weight: mat.transmission_weight,
            geometry_opacity: mat.geometry_opacity,
            base_color_texture: None,
            specular_roughness_texture: None,
            base_metalness_texture: None,
            normal_texture: None,
            geometry_opacity_texture: None,
        }
    }
}

impl OpenPbrSurface {
    /// Create from bif_core::Material with texture loading via cache.
    ///
    /// Resolves relative texture paths against `material.source_dir` (the USD
    /// layer directory), matching the viewport's `resolve_texture_path` behavior.
    pub fn from_material_with_textures(
        mat: &bif_core::Material,
        cache: &mut bif_core::texture::TextureCache,
    ) -> Self {
        let src = mat.source_dir.as_deref();

        let base_color_texture = mat
            .base_color_texture
            .as_ref()
            .and_then(|p| load_texture_logged(p, src, |r| cache.load(r)));

        let specular_roughness_texture = mat
            .specular_roughness_texture
            .as_ref()
            .and_then(|p| load_texture_logged(p, src, |r| cache.load_linear(r)));

        let base_metalness_texture = mat
            .base_metalness_texture
            .as_ref()
            .and_then(|p| load_texture_logged(p, src, |r| cache.load_linear(r)));

        let normal_texture = mat
            .normal_texture
            .as_ref()
            .and_then(|p| load_texture_logged(p, src, |r| cache.load_linear(r)));

        let geometry_opacity_texture = mat
            .geometry_opacity_texture
            .as_ref()
            .and_then(|p| load_texture_logged(p, src, |r| cache.load_linear(r)));

        // Material diagnostics
        log::info!(
            "OpenPBR '{}': color={:?} metal={:.2} rough={:.2} ior={:.2} spec_w={:.2} trans={:.2} | \
             albedo={} normal={} rough={} metal={} opacity={}",
            mat.name,
            mat.base_color,
            mat.base_metalness,
            mat.specular_roughness,
            mat.specular_ior,
            mat.specular_weight,
            mat.transmission_weight,
            base_color_texture.is_some(),
            normal_texture.is_some(),
            specular_roughness_texture.is_some(),
            base_metalness_texture.is_some(),
            geometry_opacity_texture.is_some(),
        );

        Self {
            base_weight: 1.0,
            base_color: mat.base_color,
            base_diffuse_roughness: 0.0,
            base_metalness: mat.base_metalness,
            specular_weight: mat.specular_weight,
            specular_color: Color::ONE,
            specular_roughness: mat.specular_roughness,
            specular_ior: mat.specular_ior,
            specular_roughness_anisotropy: 0.0,
            coat_weight: 0.0,
            coat_color: Color::ONE,
            coat_roughness: 0.0,
            coat_ior: 1.6,
            fuzz_weight: 0.0,
            fuzz_color: Color::ONE,
            fuzz_roughness: 0.5,
            subsurface_weight: 0.0,
            emission_luminance: mat.emission_luminance,
            emission_color: Color::new(
                mat.emission_color.x,
                mat.emission_color.y,
                mat.emission_color.z,
            ),
            transmission_weight: mat.transmission_weight,
            geometry_opacity: mat.geometry_opacity,
            base_color_texture,
            specular_roughness_texture,
            base_metalness_texture,
            normal_texture,
            geometry_opacity_texture,
        }
    }

    /// Sample base color at given UV, using texture if available.
    #[inline]
    pub fn sample_base_color(&self, u: f32, v: f32) -> Color {
        match &self.base_color_texture {
            Some(tex) => tex.sample(u, v),
            None => self.base_color,
        }
    }

    /// Sample specular roughness at given UV, using texture if available.
    #[inline]
    pub fn sample_specular_roughness(&self, u: f32, v: f32) -> f32 {
        match &self.specular_roughness_texture {
            Some(tex) => tex.sample_channel(u, v, 0),
            None => self.specular_roughness,
        }
    }

    /// Sample base metalness at given UV, using texture if available.
    #[inline]
    pub fn sample_base_metalness(&self, u: f32, v: f32) -> f32 {
        match &self.base_metalness_texture {
            Some(tex) => tex.sample_channel(u, v, 0),
            None => self.base_metalness,
        }
    }

    /// Sample geometry opacity at given UV, using texture if available.
    #[inline]
    pub fn sample_geometry_opacity(&self, u: f32, v: f32) -> f32 {
        match &self.geometry_opacity_texture {
            Some(tex) => self.geometry_opacity * tex.sample_channel(u, v, 0),
            None => self.geometry_opacity,
        }
    }

    /// Apply normal map to perturb the shading normal using tangent-space mapping.
    #[inline]
    pub fn apply_normal_map(
        &self,
        normal: Vec3,
        tangent: Vec3,
        bitangent: Vec3,
        u: f32,
        v: f32,
    ) -> Vec3 {
        match &self.normal_texture {
            Some(tex) => {
                let sampled = tex.sample(u, v);
                let map_normal = Vec3::new(
                    sampled.x * 2.0 - 1.0,
                    sampled.y * 2.0 - 1.0,
                    sampled.z * 2.0 - 1.0,
                );
                let world_normal =
                    tangent * map_normal.x + bitangent * map_normal.y + normal * map_normal.z;
                let len_sq = world_normal.length_squared();
                if len_sq > 1e-8 {
                    world_normal / len_sq.sqrt()
                } else {
                    normal
                }
            }
            None => normal,
        }
    }

    /// Check if this material has any textures bound.
    pub fn has_textures(&self) -> bool {
        self.base_color_texture.is_some()
            || self.specular_roughness_texture.is_some()
            || self.base_metalness_texture.is_some()
            || self.normal_texture.is_some()
            || self.geometry_opacity_texture.is_some()
    }
}

impl Material for OpenPbrSurface {
    fn bsdf(&self, ray_in: &Ray, rec: &HitRecord, scattered: &Ray) -> Color {
        let wo = -ray_in.direction().normalize();
        let wi = scattered.direction().normalize();
        let n = self.apply_normal_map(rec.normal, rec.tangent, rec.bitangent, rec.u, rec.v);

        let n_dot_l = n.dot(wi);
        let n_dot_v = n.dot(wo);
        if n_dot_l <= 0.0 || n_dot_v <= 0.0 {
            return Color::ZERO;
        }

        let base_color = self.sample_base_color(rec.u, rec.v);
        let metalness = self.sample_base_metalness(rec.u, rec.v);
        let roughness = self.sample_specular_roughness(rec.u, rec.v);
        let alpha = (roughness * roughness).max(0.001);

        let h = (wo + wi).normalize();
        let n_dot_h = n.dot(h).max(0.0);
        let l_dot_h = wi.dot(h).max(0.0);

        // Diffuse (Burley)
        let fd90 = 0.5 + 2.0 * roughness * l_dot_h * l_dot_h;
        let fl = schlick_weight(n_dot_l);
        let fv = schlick_weight(n_dot_v);
        let fd = lerp(1.0, fd90, fl) * lerp(1.0, fd90, fv);
        let diffuse = base_color * fd * (1.0 - metalness) / PI;

        // Specular (GGX with IOR-based Fresnel)
        let d = ggx_d(n_dot_h, alpha);
        let g = smith_g_ggx(n_dot_l, n_dot_v, alpha);
        let f0 = self.fresnel_0_textured(base_color, metalness);
        let f = schlick_fresnel3(f0, l_dot_h);
        let specular = f * (d * g / (4.0 * n_dot_l * n_dot_v).max(0.001));

        diffuse + specular
    }

    fn pdf(&self, ray_in: &Ray, rec: &HitRecord, scattered: &Ray) -> f32 {
        let wo = -ray_in.direction().normalize();
        let wi = scattered.direction().normalize();
        let n = self.apply_normal_map(rec.normal, rec.tangent, rec.bitangent, rec.u, rec.v);

        let n_dot_l = n.dot(wi);
        if n_dot_l <= 0.0 {
            return 0.0001;
        }

        let metalness = self.sample_base_metalness(rec.u, rec.v);
        let roughness = self.sample_specular_roughness(rec.u, rec.v);
        let alpha = (roughness * roughness).max(0.001);

        // IOR-based lobe selection weight
        let f0_scalar = ior_to_f0(self.specular_ior) * self.specular_weight;
        let diffuse_weight = (1.0 - metalness) * (1.0 - f0_scalar.clamp(0.0, 1.0));
        let specular_weight = 1.0 - diffuse_weight;
        let total = diffuse_weight + specular_weight;
        let p_diffuse = diffuse_weight / total;
        let p_specular = specular_weight / total;

        // Cosine-weighted hemisphere PDF
        let cos_pdf = (n_dot_l / PI).max(0.0001);

        // GGX PDF
        let h = (wo + wi).normalize();
        let n_dot_h = n.dot(h).max(0.0);
        let l_dot_h = wi.dot(h).max(0.0);
        let d = ggx_d(n_dot_h, alpha);
        let ggx_pdf = (d * n_dot_h / (4.0 * l_dot_h)).max(0.0001);

        (p_diffuse * cos_pdf + p_specular * ggx_pdf).max(0.0001)
    }

    fn is_delta(&self) -> bool {
        self.specular_roughness < 0.001
            && (self.base_metalness > 0.999 || self.transmission_weight > 0.5)
    }

    fn albedo(&self, u: f32, v: f32) -> Color {
        self.sample_base_color(u, v)
    }

    fn scatter(
        &self,
        ray_in: &Ray,
        rec: &HitRecord,
        rng: &mut dyn RngCore,
    ) -> Option<ScatterResult> {
        // Opacity check: stochastic alpha cutout
        let opacity = self.sample_geometry_opacity(rec.u, rec.v);
        if opacity < 1.0 {
            let r = gen_f32(rng);
            if r > opacity {
                let scattered = Ray::new(rec.p, ray_in.direction(), ray_in.time());
                return Some(ScatterResult {
                    attenuation: Color::ONE,
                    scattered,
                    pdf: 1.0,
                    pass_through: true,
                });
            }
        }

        // Transmission: dielectric refraction (glass)
        if self.transmission_weight > 0.0 && gen_f32(rng) < self.transmission_weight {
            return self.scatter_transmission(ray_in, rec, rng);
        }

        let wo = -ray_in.direction().normalize();
        let n = self.apply_normal_map(rec.normal, rec.tangent, rec.bitangent, rec.u, rec.v);

        let base_color = self.sample_base_color(rec.u, rec.v);
        let metalness = self.sample_base_metalness(rec.u, rec.v);
        let roughness = self.sample_specular_roughness(rec.u, rec.v);

        // IOR-based lobe selection
        let f0_scalar = ior_to_f0(self.specular_ior) * self.specular_weight;
        let diffuse_weight = (1.0 - metalness) * (1.0 - f0_scalar.clamp(0.0, 1.0));
        let specular_weight = 1.0 - diffuse_weight;

        let do_diffuse = gen_f32(rng) < diffuse_weight / (diffuse_weight + specular_weight);

        if do_diffuse {
            self.scatter_diffuse_textured(wo, n, rec.p, ray_in.time(), rng, base_color, roughness)
        } else {
            self.scatter_specular_textured(
                wo,
                n,
                rec.p,
                ray_in.time(),
                rng,
                base_color,
                metalness,
                roughness,
            )
        }
    }
}

impl OpenPbrSurface {
    /// Compute F0 (Fresnel at normal incidence) using IOR.
    ///
    /// For dielectrics: F0 = ((ior - 1) / (ior + 1))^2 * specular_weight
    /// For metals: F0 = base_color * base_weight
    /// Blended by metalness.
    fn fresnel_0_textured(&self, base_color: Color, metalness: f32) -> Color {
        let f0_dielectric = ior_to_f0(self.specular_ior) * self.specular_weight;
        let dielectric_f0 = Color::new(f0_dielectric, f0_dielectric, f0_dielectric);
        let metal_f0 = base_color * self.base_weight;
        lerp3(dielectric_f0, metal_f0, metalness)
    }

    /// Scatter with diffuse (Burley) lobe using texture-sampled values.
    #[allow(clippy::too_many_arguments)]
    fn scatter_diffuse_textured(
        &self,
        wo: Vec3,
        n: Vec3,
        hit_point: Vec3,
        time: f32,
        rng: &mut dyn RngCore,
        base_color: Color,
        roughness: f32,
    ) -> Option<ScatterResult> {
        let wi = cosine_weighted_hemisphere(n, rng);

        let n_dot_l = n.dot(wi).max(0.0);
        let n_dot_v = n.dot(wo).max(0.0);

        if n_dot_l <= 0.0 {
            return None;
        }

        let h = (wo + wi).normalize();
        let l_dot_h = wi.dot(h).max(0.0);

        let fd90 = 0.5 + 2.0 * roughness * l_dot_h * l_dot_h;
        let fl = schlick_weight(n_dot_l);
        let fv = schlick_weight(n_dot_v);
        let fd = lerp(1.0, fd90, fl) * lerp(1.0, fd90, fv);

        let fss90 = l_dot_h * l_dot_h * roughness;
        let fss = lerp(1.0, fss90, fl) * lerp(1.0, fss90, fv);
        let ss = 1.25 * (fss * (1.0 / (n_dot_l + n_dot_v).max(0.001) - 0.5) + 0.5);

        let diffuse = lerp(fd, ss, self.subsurface_weight);

        // Fuzz (grazing sheen)
        let fuzz = if self.fuzz_weight > 0.0 {
            schlick_weight(l_dot_h) * self.fuzz_weight * self.fuzz_color
        } else {
            Color::ZERO
        };

        let attenuation = base_color * diffuse / PI + fuzz;
        let scattered = Ray::new(hit_point, wi, time);
        let pdf = (n_dot_l / PI).max(0.0001);

        Some(ScatterResult {
            attenuation,
            scattered,
            pdf,
            pass_through: false,
        })
    }

    /// Scatter with specular (GGX) lobe using texture-sampled values.
    #[allow(clippy::too_many_arguments)]
    fn scatter_specular_textured(
        &self,
        wo: Vec3,
        n: Vec3,
        hit_point: Vec3,
        time: f32,
        rng: &mut dyn RngCore,
        base_color: Color,
        metalness: f32,
        roughness: f32,
    ) -> Option<ScatterResult> {
        let alpha = roughness * roughness;
        let alpha = alpha.max(0.001);

        let h = sample_ggx(n, alpha, rng);
        let wi = reflect(-wo, h);

        let n_dot_l = n.dot(wi);
        if n_dot_l <= 0.0 {
            return None;
        }

        let n_dot_v = n.dot(wo).max(0.0);
        let n_dot_h = n.dot(h).max(0.0);
        let l_dot_h = wi.dot(h).max(0.0);

        let d = ggx_d(n_dot_h, alpha);
        let g = smith_g_ggx(n_dot_l, n_dot_v, alpha);
        let f0 = self.fresnel_0_textured(base_color, metalness);
        let f = schlick_fresnel3(f0, l_dot_h);

        let weight = (g * l_dot_h) / (n_dot_h * n_dot_v.max(0.001));
        let attenuation = f * weight.max(0.0);
        let scattered = Ray::new(hit_point, wi, time);
        let pdf = (d * n_dot_h / (4.0 * l_dot_h)).max(0.0001);

        Some(ScatterResult {
            attenuation,
            scattered,
            pdf,
            pass_through: false,
        })
    }

    /// Scatter with dielectric refraction (glass/transmission).
    ///
    /// Uses Snell's law with total internal reflection and Schlick Fresnel,
    /// matching the existing `Dielectric` material logic.
    ///
    /// Note: no explicit ray origin bias needed — the renderer uses `tnear = 0.001`
    /// globally which prevents self-intersection for both reflected and refracted rays.
    fn scatter_transmission(
        &self,
        ray_in: &Ray,
        rec: &HitRecord,
        rng: &mut dyn RngCore,
    ) -> Option<ScatterResult> {
        let refraction_ratio = if rec.front_face {
            1.0 / self.specular_ior
        } else {
            self.specular_ior
        };

        let unit_dir = ray_in.direction().normalize();
        let cos_theta = (-unit_dir).dot(rec.normal).min(1.0);
        let sin_theta = (1.0 - cos_theta * cos_theta).sqrt();
        let cannot_refract = refraction_ratio * sin_theta > 1.0;

        // Schlick reflectance
        let r0 = ((1.0 - refraction_ratio) / (1.0 + refraction_ratio)).powi(2);
        let reflectance = r0 + (1.0 - r0) * (1.0 - cos_theta).powi(5);

        let direction = if cannot_refract || reflectance > gen_f32(rng) {
            reflect(unit_dir, rec.normal)
        } else {
            refract(unit_dir, rec.normal, refraction_ratio)
        };

        Some(ScatterResult {
            attenuation: Color::ONE,
            scattered: Ray::new(rec.p, direction, ray_in.time()),
            pdf: 1.0,
            pass_through: false,
        })
    }
}

// =============================================================================
// IOR helpers
// =============================================================================

/// Convert IOR to Fresnel reflectance at normal incidence (F0).
///
/// F0 = ((ior - 1) / (ior + 1))^2
/// IOR=1.5 → F0=0.04 (common dielectric)
#[inline]
fn ior_to_f0(ior: f32) -> f32 {
    let r = (ior - 1.0) / (ior + 1.0);
    r * r
}

// =============================================================================
// Texture path resolution
// =============================================================================

/// Resolve a texture path against the material's source directory.
fn resolve_texture_path(path: &str, source_dir: Option<&Path>) -> String {
    let p = Path::new(path);
    if p.is_absolute() || path.starts_with("//") || path.starts_with("\\\\") {
        return path.to_string();
    }
    if let Some(dir) = source_dir {
        return dir.join(p).to_string_lossy().into_owned();
    }
    path.to_string()
}

/// Resolve a texture path, load via the provided loader, and log failures.
fn load_texture_logged<F>(
    raw_path: &str,
    source_dir: Option<&Path>,
    loader: F,
) -> Option<Arc<Texture>>
where
    F: FnOnce(&str) -> Result<Arc<Texture>, bif_core::texture::TextureError>,
{
    let resolved = resolve_texture_path(raw_path, source_dir);
    match loader(&resolved) {
        Ok(tex) => Some(tex),
        Err(e) => {
            log::warn!("Ivar: texture load failed '{}': {}", resolved, e);
            None
        }
    }
}

// =============================================================================
// Helper functions
// =============================================================================

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
fn sample_ggx(n: Vec3, alpha: f32, rng: &mut dyn RngCore) -> Vec3 {
    let u1 = gen_f32(rng).clamp(0.0001, 0.9999);
    let u2 = gen_f32(rng);

    let theta = (alpha * u1.sqrt() / (1.0 - u1).sqrt()).atan();
    let phi = 2.0 * PI * u2;

    let sin_theta = theta.sin();
    let cos_theta = theta.cos();
    let sin_phi = phi.sin();
    let cos_phi = phi.cos();

    let h_local = Vec3::new(sin_theta * cos_phi, sin_theta * sin_phi, cos_theta);

    let (tangent, bitangent) = build_orthonormal_basis(n);
    h_local.x * tangent + h_local.y * bitangent + h_local.z * n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openpbr_default() {
        let mat = OpenPbrSurface::new();
        assert!((mat.base_metalness - 0.0).abs() < 0.001);
        assert!((mat.specular_roughness - 0.3).abs() < 0.001);
        assert!((mat.specular_ior - 1.5).abs() < 0.001);
        assert!((mat.specular_weight - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_openpbr_metal() {
        let mat = OpenPbrSurface::metal(Color::new(1.0, 0.8, 0.0), 0.1);
        assert!((mat.base_metalness - 1.0).abs() < 0.001);
        assert!((mat.specular_roughness - 0.1).abs() < 0.001);
    }

    #[test]
    fn test_ior_to_f0() {
        // IOR=1.5 should give F0 ~= 0.04
        let f0 = ior_to_f0(1.5);
        assert!((f0 - 0.04).abs() < 0.001);

        // IOR=1.0 (vacuum) should give F0 = 0.0
        let f0 = ior_to_f0(1.0);
        assert!((f0 - 0.0).abs() < 0.001);

        // IOR=2.5 should give F0 ~= 0.184
        let f0 = ior_to_f0(2.5);
        assert!((f0 - 0.184).abs() < 0.01);
    }

    #[test]
    fn test_openpbr_metal_f0_equals_base_color() {
        let mat = OpenPbrSurface {
            base_weight: 1.0,
            base_metalness: 1.0,
            base_color: Color::new(1.0, 0.8, 0.0),
            ..Default::default()
        };
        let f0 = mat.fresnel_0_textured(mat.base_color, 1.0);
        // For full metal, F0 = base_color * base_weight
        assert!((f0.x - 1.0).abs() < 0.001);
        assert!((f0.y - 0.8).abs() < 0.001);
        assert!((f0.z - 0.0).abs() < 0.001);
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
        assert!(t.dot(n).abs() < 0.001);
        assert!(b.dot(n).abs() < 0.001);
        assert!(t.dot(b).abs() < 0.001);
        assert!((t.length() - 1.0).abs() < 0.001);
        assert!((b.length() - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_openpbr_albedo_delegates_to_base_color() {
        let mat = OpenPbrSurface::diffuse(Color::new(0.9, 0.1, 0.3));
        let albedo = Material::albedo(&mat, 0.5, 0.5);
        assert!((albedo.x - 0.9).abs() < 0.001);
        assert!((albedo.y - 0.1).abs() < 0.001);
        assert!((albedo.z - 0.3).abs() < 0.001);
    }

    #[test]
    fn test_resolve_texture_path_absolute() {
        let abs = if cfg!(windows) {
            "C:\\textures\\diffuse.png"
        } else {
            "/textures/diffuse.png"
        };
        assert_eq!(resolve_texture_path(abs, None), abs);
        assert_eq!(resolve_texture_path(abs, Some(Path::new("/other"))), abs);
    }

    #[test]
    fn test_resolve_texture_path_unc() {
        let unc = "\\\\server\\share\\tex.png";
        assert_eq!(resolve_texture_path(unc, None), unc);
        assert_eq!(resolve_texture_path(unc, Some(Path::new("/other"))), unc);
    }

    #[test]
    fn test_resolve_texture_path_relative_with_source_dir() {
        let resolved =
            resolve_texture_path("textures/diffuse.png", Some(Path::new("/scenes/alab")));
        assert!(resolved.contains("scenes"));
        assert!(resolved.contains("diffuse.png"));
    }

    #[test]
    fn test_resolve_texture_path_relative_no_source_dir() {
        assert_eq!(
            resolve_texture_path("textures/diffuse.png", None),
            "textures/diffuse.png"
        );
    }

    #[test]
    fn test_openpbr_glass_constructor() {
        let mat = OpenPbrSurface::glass(1.5);
        assert!((mat.transmission_weight - 1.0).abs() < 0.001);
        assert!((mat.specular_ior - 1.5).abs() < 0.001);
        assert!((mat.specular_roughness - 0.0).abs() < 0.001);
        assert!((mat.base_metalness - 0.0).abs() < 0.001);
        assert!(mat.is_delta());
    }

    #[test]
    fn test_openpbr_glass_scatter_produces_rays() {
        use rand::SeedableRng;
        let mat = OpenPbrSurface::glass(1.5);
        let ray_in = Ray::new(Vec3::new(0.0, 1.0, 0.0), Vec3::new(0.0, -1.0, 0.0), 0.0);
        let dummy_mat = crate::material::Lambertian::new(Color::ZERO);
        let rec = HitRecord {
            p: Vec3::ZERO,
            normal: Vec3::new(0.0, 1.0, 0.0),
            tangent: Vec3::new(1.0, 0.0, 0.0),
            bitangent: Vec3::new(0.0, 0.0, 1.0),
            material: &dummy_mat,
            t: 1.0,
            u: 0.0,
            v: 0.0,
            front_face: true,
        };

        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        let result = mat.scatter(&ray_in, &rec, &mut rng);
        assert!(result.is_some());
        let sr = result.unwrap();
        // Glass should produce a ray (refracted or reflected), not absorb
        assert!(sr.scattered.direction().length() > 0.5);
        // Attenuation should be white (no color absorption)
        assert!((sr.attenuation.x - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_openpbr_default_no_transmission() {
        let mat = OpenPbrSurface::new();
        assert!((mat.transmission_weight - 0.0).abs() < 0.001);
        // Default should not be delta (not pure metal, no transmission)
        assert!(!mat.is_delta());
    }
}
