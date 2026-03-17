//! Disney Principled BSDF implementation.
//!
//! Based on the 2012 Disney paper "Physically Based Shading at Disney"
//! and the 2015 extension for clearcoat and sheen.
//!
//! Supports optional texture maps for base_color, roughness, metallic, and normals.

use crate::material::{cosine_weighted_hemisphere, gen_f32, reflect, Color, ScatterResult};
use crate::{hittable::HitRecord, Material, Ray};
use bif_core::texture::Texture;
use bif_math::{build_orthonormal_basis, Vec3};
use rand::RngCore;
use std::f32::consts::PI;
use std::path::Path;
use std::sync::Arc;

/// Disney Principled BSDF material.
///
/// A physically-based material with intuitive artist-friendly parameters.
/// Supports optional texture maps that override scalar values when present.
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
    /// TODO: Implement clearcoat lobe sampling
    #[allow(dead_code)]
    pub clearcoat: f32,

    /// Clearcoat gloss: 0 = satin, 1 = gloss
    /// TODO: Implement clearcoat lobe sampling
    #[allow(dead_code)]
    pub clearcoat_gloss: f32,

    /// Subsurface: blend to subsurface approximation
    pub subsurface: f32,

    /// Anisotropic: aspect ratio for anisotropic reflection
    /// TODO: Implement anisotropic GGX sampling
    #[allow(dead_code)]
    pub anisotropic: f32,

    // =========================================================================
    // Texture maps (optional, override scalar values when present)
    // =========================================================================
    /// Base color / diffuse texture
    pub diffuse_texture: Option<Arc<Texture>>,

    /// Roughness texture (samples from R channel)
    pub roughness_texture: Option<Arc<Texture>>,

    /// Metallic texture (samples from R channel)
    pub metallic_texture: Option<Arc<Texture>>,

    /// Normal map texture
    pub normal_texture: Option<Arc<Texture>>,

    /// Opacity (0=transparent, 1=opaque)
    pub opacity: f32,

    /// Opacity texture (samples from R channel)
    pub opacity_texture: Option<Arc<Texture>>,
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
            diffuse_texture: None,
            roughness_texture: None,
            metallic_texture: None,
            normal_texture: None,
            opacity: 1.0,
            opacity_texture: None,
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
/// Note: This version does not load textures. Use `from_material_with_textures`
/// if you have a TextureCache available.
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
            diffuse_texture: None,
            roughness_texture: None,
            metallic_texture: None,
            normal_texture: None,
            opacity: mat.opacity,
            opacity_texture: None,
        }
    }
}

impl DisneyBSDF {
    /// Create a DisneyBSDF from a bif_core::Material, loading textures via cache.
    ///
    /// This is the preferred method when you have access to a TextureCache,
    /// as it will load and bind texture maps for proper rendering.
    ///
    /// Resolves relative texture paths against `material.source_dir` (the USD
    /// layer directory), matching the viewport's `resolve_texture_path` behavior.
    pub fn from_material_with_textures(
        mat: &bif_core::Material,
        cache: &mut bif_core::texture::TextureCache,
    ) -> Self {
        let src = mat.source_dir.as_deref();

        // Load textures via cache (returns Arc<Texture>)
        // Diffuse uses sRGB→linear; data textures (normal/roughness/metallic/opacity) use linear
        let diffuse_texture = mat
            .diffuse_texture
            .as_ref()
            .and_then(|p| load_texture_logged(p, src, |r| cache.load(r)));

        let roughness_texture = mat
            .roughness_texture
            .as_ref()
            .and_then(|p| load_texture_logged(p, src, |r| cache.load_linear(r)));

        let metallic_texture = mat
            .metallic_texture
            .as_ref()
            .and_then(|p| load_texture_logged(p, src, |r| cache.load_linear(r)));

        let normal_texture = mat
            .normal_texture
            .as_ref()
            .and_then(|p| load_texture_logged(p, src, |r| cache.load_linear(r)));

        let opacity_texture = mat
            .opacity_texture
            .as_ref()
            .and_then(|p| load_texture_logged(p, src, |r| cache.load_linear(r)));

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
            diffuse_texture,
            roughness_texture,
            metallic_texture,
            normal_texture,
            opacity: mat.opacity,
            opacity_texture,
        }
    }

    /// Sample base color at given UV, using texture if available.
    #[inline]
    pub fn sample_base_color(&self, u: f32, v: f32) -> Color {
        match &self.diffuse_texture {
            Some(tex) => tex.sample(u, v),
            None => self.base_color,
        }
    }

    /// Sample roughness at given UV, using texture if available.
    #[inline]
    pub fn sample_roughness(&self, u: f32, v: f32) -> f32 {
        match &self.roughness_texture {
            Some(tex) => tex.sample_channel(u, v, 0), // R channel
            None => self.roughness,
        }
    }

    /// Sample metallic at given UV, using texture if available.
    #[inline]
    pub fn sample_metallic(&self, u: f32, v: f32) -> f32 {
        match &self.metallic_texture {
            Some(tex) => tex.sample_channel(u, v, 0), // R channel
            None => self.metallic,
        }
    }

    /// Sample opacity at given UV, using texture if available.
    #[inline]
    pub fn sample_opacity(&self, u: f32, v: f32) -> f32 {
        match &self.opacity_texture {
            Some(tex) => self.opacity * tex.sample_channel(u, v, 0),
            None => self.opacity,
        }
    }

    /// Apply normal map to perturb the shading normal using tangent-space mapping.
    ///
    /// Samples the normal texture, remaps from [0,1] to [-1,1], and transforms
    /// from tangent space to world space using the TBN matrix.
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
                // Remap from [0,1] to [-1,1]
                let map_normal = Vec3::new(
                    sampled.x * 2.0 - 1.0,
                    sampled.y * 2.0 - 1.0,
                    sampled.z * 2.0 - 1.0,
                );
                // Transform from tangent space to world space: T*x + B*y + N*z
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
        self.diffuse_texture.is_some()
            || self.roughness_texture.is_some()
            || self.metallic_texture.is_some()
            || self.normal_texture.is_some()
            || self.opacity_texture.is_some()
    }
}

impl Material for DisneyBSDF {
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
        let metallic = self.sample_metallic(rec.u, rec.v);
        let roughness = self.sample_roughness(rec.u, rec.v);
        let alpha = (roughness * roughness).max(0.001);

        let h = (wo + wi).normalize();
        let n_dot_h = n.dot(h).max(0.0);
        let l_dot_h = wi.dot(h).max(0.0);

        // Diffuse (Burley)
        let fd90 = 0.5 + 2.0 * roughness * l_dot_h * l_dot_h;
        let fl = schlick_weight(n_dot_l);
        let fv = schlick_weight(n_dot_v);
        let fd = lerp(1.0, fd90, fl) * lerp(1.0, fd90, fv);
        let diffuse = base_color * fd * (1.0 - metallic) / PI;

        // Specular (GGX)
        let d = ggx_d(n_dot_h, alpha);
        let g = smith_g_ggx(n_dot_l, n_dot_v, alpha);
        let f0 = self.fresnel_0_textured(base_color, metallic);
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

        let metallic = self.sample_metallic(rec.u, rec.v);
        let roughness = self.sample_roughness(rec.u, rec.v);
        let alpha = (roughness * roughness).max(0.001);

        let diffuse_weight = (1.0 - metallic) * (1.0 - self.specular * 0.5);
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
        self.roughness < 0.001 && self.metallic > 0.999
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
        let opacity = self.sample_opacity(rec.u, rec.v);
        if opacity < 1.0 {
            let r = gen_f32(rng);
            if r > opacity {
                // Pass through: continue ray in same direction
                let scattered = Ray::new(rec.p, ray_in.direction(), ray_in.time());
                return Some(ScatterResult {
                    attenuation: Color::ONE,
                    scattered,
                    pdf: 1.0,
                    pass_through: true,
                });
            }
        }

        let wo = -ray_in.direction().normalize();

        // Apply normal map if present
        let n = self.apply_normal_map(rec.normal, rec.tangent, rec.bitangent, rec.u, rec.v);

        // Sample material parameters from textures at hit UV coordinates
        let base_color = self.sample_base_color(rec.u, rec.v);
        let metallic = self.sample_metallic(rec.u, rec.v);
        let roughness = self.sample_roughness(rec.u, rec.v);

        // Decide between diffuse and specular based on material parameters
        let diffuse_weight = (1.0 - metallic) * (1.0 - self.specular * 0.5);
        let specular_weight = 1.0 - diffuse_weight;

        let do_diffuse = gen_f32(rng) < diffuse_weight / (diffuse_weight + specular_weight);

        if do_diffuse {
            // Diffuse scattering (Burley diffuse approximation)
            self.scatter_diffuse_textured(wo, n, rec.p, ray_in.time(), rng, base_color, roughness)
        } else {
            // Specular scattering (GGX microfacet)
            self.scatter_specular_textured(
                wo,
                n,
                rec.p,
                ray_in.time(),
                rng,
                base_color,
                metallic,
                roughness,
            )
        }
    }
}

impl DisneyBSDF {
    /// Scatter with diffuse (Burley) lobe.
    /// Note: Currently unused as we always use texture-sampling version.
    /// Kept for potential optimization when no textures are bound.
    #[allow(dead_code)]
    fn scatter_diffuse(
        &self,
        wo: Vec3,
        n: Vec3,
        hit_point: Vec3,
        time: f32,
        rng: &mut dyn RngCore,
    ) -> Option<ScatterResult> {
        // Sample cosine-weighted hemisphere (PDF = cos(theta) / PI)
        let wi = cosine_weighted_hemisphere(n, rng);

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
        let ss = 1.25 * (fss * (1.0 / (n_dot_l + n_dot_v).max(0.001) - 0.5) + 0.5);

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

        // Cosine-weighted hemisphere PDF
        let pdf = (n_dot_l / PI).max(0.0001);

        Some(ScatterResult {
            attenuation,
            scattered,
            pdf,
            pass_through: false,
        })
    }

    /// Scatter with specular (GGX) lobe.
    /// Note: Currently unused as we always use texture-sampling version.
    #[allow(dead_code)]
    fn scatter_specular(
        &self,
        wo: Vec3,
        n: Vec3,
        hit_point: Vec3,
        time: f32,
        rng: &mut dyn RngCore,
    ) -> Option<ScatterResult> {
        // Use GGX importance sampling
        let alpha = self.roughness * self.roughness;
        let alpha = alpha.max(0.001); // Prevent division by zero

        // Sample GGX microfacet normal
        let h = sample_ggx(n, alpha, rng);
        let wi = reflect(-wo, h);

        // Check if scattering direction is valid
        let n_dot_l = n.dot(wi);
        if n_dot_l <= 0.0 {
            return None;
        }

        let n_dot_v = n.dot(wo).max(0.0);
        let n_dot_h = n.dot(h).max(0.0);
        let l_dot_h = wi.dot(h).max(0.0);

        // GGX distribution
        let d = ggx_d(n_dot_h, alpha);

        // Schlick-GGX geometry (Smith)
        let g = smith_g_ggx(n_dot_l, n_dot_v, alpha);

        // Fresnel
        let f0 = self.fresnel_0();
        let f = schlick_fresnel3(f0, l_dot_h);

        // Specular BRDF: D * G * F / (4 * NdotL * NdotV)
        // With GGX importance sampling (PDF = D * NdotH / (4 * LdotH)):
        // weight = BRDF / PDF = G * F * LdotH / (NdotH * NdotV)
        let weight = (g * l_dot_h) / (n_dot_h * n_dot_v.max(0.001));

        let attenuation = f * weight.max(0.0);
        let scattered = Ray::new(hit_point, wi, time);

        // GGX importance sampling PDF: D * n_dot_h / (4 * l_dot_h)
        let pdf = (d * n_dot_h / (4.0 * l_dot_h)).max(0.0001);

        Some(ScatterResult {
            attenuation,
            scattered,
            pdf,
            pass_through: false,
        })
    }

    /// Compute F0 (Fresnel at normal incidence) based on material parameters.
    /// Note: Currently unused as we always use texture-sampling version.
    #[allow(dead_code)]
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

    /// Compute F0 with textured base_color and metallic.
    fn fresnel_0_textured(&self, base_color: Color, metallic: f32) -> Color {
        let dielectric_f0 = 0.08 * self.specular;

        let c_tint = if base_color.length_squared() > 0.0 {
            base_color / luminance(base_color)
        } else {
            Color::ONE
        };
        let c_spec = lerp3(
            Color::new(dielectric_f0, dielectric_f0, dielectric_f0),
            dielectric_f0 * c_tint,
            self.specular_tint,
        );

        lerp3(c_spec, base_color, metallic)
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

        let diffuse = lerp(fd, ss, self.subsurface);

        let sheen = if self.sheen > 0.0 {
            let c_tint = if base_color.length_squared() > 0.0 {
                base_color / luminance(base_color)
            } else {
                Color::ONE
            };
            let c_sheen = lerp3(Color::ONE, c_tint, self.sheen_tint);
            schlick_weight(l_dot_h) * self.sheen * c_sheen
        } else {
            Color::ZERO
        };

        let attenuation = base_color * diffuse / PI + sheen;
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
        metallic: f32,
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
        let f0 = self.fresnel_0_textured(base_color, metallic);
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
}

// =============================================================================
// Texture path resolution
// =============================================================================

/// Resolve a texture path against the material's source directory.
///
/// Matches the viewport's `texture_loader.rs::resolve_texture_path` behavior:
/// absolute paths and UNC paths pass through unchanged; relative paths are
/// joined with `source_dir` (the USD layer directory).
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
fn sample_ggx(n: Vec3, alpha: f32, rng: &mut dyn RngCore) -> Vec3 {
    // Clamp to avoid degenerate half vectors at extremes
    let u1 = gen_f32(rng).clamp(0.0001, 0.9999);
    let u2 = gen_f32(rng);

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

    #[test]
    fn test_disney_albedo_delegates_to_base_color() {
        let mat = DisneyBSDF::diffuse(Color::new(0.9, 0.1, 0.3));
        // Without texture, albedo should return base_color
        let albedo = Material::albedo(&mat, 0.5, 0.5);
        assert!((albedo.x - 0.9).abs() < 0.001);
        assert!((albedo.y - 0.1).abs() < 0.001);
        assert!((albedo.z - 0.3).abs() < 0.001);
    }

    #[test]
    fn test_resolve_texture_path_absolute() {
        // Absolute paths pass through unchanged
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
        // UNC paths pass through unchanged
        let unc = "\\\\server\\share\\tex.png";
        assert_eq!(resolve_texture_path(unc, None), unc);
        assert_eq!(resolve_texture_path(unc, Some(Path::new("/other"))), unc);
    }

    #[test]
    fn test_resolve_texture_path_relative_with_source_dir() {
        let resolved =
            resolve_texture_path("textures/diffuse.png", Some(Path::new("/scenes/alab")));
        // Should join source_dir + relative path
        assert!(resolved.contains("scenes"));
        assert!(resolved.contains("diffuse.png"));
    }

    #[test]
    fn test_resolve_texture_path_relative_no_source_dir() {
        // No source_dir → return as-is
        assert_eq!(
            resolve_texture_path("textures/diffuse.png", None),
            "textures/diffuse.png"
        );
    }
}
