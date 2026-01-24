//! Image-Based Lighting (IBL) prefiltering.
//!
//! Generates cubemap, irradiance map, prefiltered specular map,
//! and BRDF integration LUT from an equirectangular HDR image.

use std::f32::consts::PI;

use rayon::prelude::*;

use crate::hdr::HdrImage;

/// Size of the base cubemap face in pixels.
pub const CUBEMAP_SIZE: u32 = 256;
/// Size of each irradiance cubemap face.
pub const IRRADIANCE_SIZE: u32 = 32;
/// Size of the base prefiltered specular cubemap face.
pub const PREFILTERED_SIZE: u32 = 128;
/// Number of roughness mip levels for prefiltered specular.
pub const PREFILTERED_MIP_COUNT: u32 = 5;
/// Size of the BRDF integration LUT.
pub const BRDF_LUT_SIZE: u32 = 256;

/// A single cubemap face stored as RGBA f16-compatible floats.
pub struct CubemapFace {
    pub size: u32,
    /// RGBA pixels, row-major.
    pub pixels: Vec<[f32; 4]>,
}

impl CubemapFace {
    pub fn new(size: u32) -> Self {
        Self {
            size,
            pixels: vec![[0.0, 0.0, 0.0, 1.0]; (size * size) as usize],
        }
    }

    pub fn set_pixel(&mut self, x: u32, y: u32, rgb: [f32; 3]) {
        let idx = (y * self.size + x) as usize;
        self.pixels[idx] = [rgb[0], rgb[1], rgb[2], 1.0];
    }
}

/// BRDF integration lookup table (split-sum approximation).
pub struct BrdfLut {
    pub size: u32,
    /// RG pixels: (scale, bias) per texel.
    pub pixels: Vec<[f32; 2]>,
}

/// Complete set of environment maps for IBL rendering.
pub struct EnvironmentMaps {
    /// Base cubemap for skybox display (CUBEMAP_SIZE per face).
    pub cubemap: [CubemapFace; 6],
    /// Irradiance cubemap for diffuse IBL (IRRADIANCE_SIZE per face).
    pub irradiance: [CubemapFace; 6],
    /// Prefiltered specular cubemap, one set of 6 faces per mip level.
    pub prefiltered: Vec<[CubemapFace; 6]>,
    /// BRDF integration LUT.
    pub brdf_lut: BrdfLut,
    /// Number of mip levels in prefiltered map.
    pub mip_count: u32,
}

/// Cubemap face indices: +X, -X, +Y, -Y, +Z, -Z.
const FACE_COUNT: usize = 6;

/// Generate all IBL maps from an equirectangular HDR image.
///
/// Cubemaps are generated at rotation=0; the shader applies rotation live.
/// Uses rayon for parallel face/mip generation.
pub fn generate_environment_maps(hdr: &HdrImage) -> EnvironmentMaps {
    log::info!("Generating IBL environment maps...");

    // Generate base cubemap (rotation=0, shader handles rotation)
    let cubemap = generate_cubemap(hdr, CUBEMAP_SIZE);
    log::info!("  Cubemap {}x{} done", CUBEMAP_SIZE, CUBEMAP_SIZE);

    // Generate irradiance map
    let irradiance = generate_irradiance(hdr, IRRADIANCE_SIZE);
    log::info!("  Irradiance {}x{} done", IRRADIANCE_SIZE, IRRADIANCE_SIZE);

    // Generate prefiltered specular mip chain
    let prefiltered = generate_prefiltered(hdr, PREFILTERED_SIZE, PREFILTERED_MIP_COUNT);
    log::info!(
        "  Prefiltered {}x{} ({} mips) done",
        PREFILTERED_SIZE,
        PREFILTERED_SIZE,
        PREFILTERED_MIP_COUNT
    );

    // Generate BRDF LUT (independent of environment)
    let brdf_lut = generate_brdf_lut(BRDF_LUT_SIZE);
    log::info!("  BRDF LUT {}x{} done", BRDF_LUT_SIZE, BRDF_LUT_SIZE);

    EnvironmentMaps {
        cubemap,
        irradiance,
        prefiltered,
        brdf_lut,
        mip_count: PREFILTERED_MIP_COUNT,
    }
}

/// Convert a texel coordinate on a cubemap face to a world-space direction.
fn face_texel_to_dir(face: usize, x: u32, y: u32, size: u32) -> [f32; 3] {
    // Map texel to [-1, 1] range
    let u = 2.0 * (x as f32 + 0.5) / size as f32 - 1.0;
    let v = 2.0 * (y as f32 + 0.5) / size as f32 - 1.0;

    let dir = match face {
        0 => [1.0, -v, -u],  // +X
        1 => [-1.0, -v, u],  // -X
        2 => [u, 1.0, v],    // +Y
        3 => [u, -1.0, -v],  // -Y
        4 => [u, -v, 1.0],   // +Z
        5 => [-u, -v, -1.0], // -Z
        _ => unreachable!(),
    };

    normalize(dir)
}

/// Generate base cubemap from equirectangular HDR.
fn generate_cubemap(hdr: &HdrImage, size: u32) -> [CubemapFace; 6] {
    let faces: Vec<CubemapFace> = (0..FACE_COUNT)
        .into_par_iter()
        .map(|face| {
            let mut f = CubemapFace::new(size);
            for y in 0..size {
                for x in 0..size {
                    let dir = face_texel_to_dir(face, x, y, size);
                    let rgb = hdr.sample(dir, 0.0);
                    f.set_pixel(x, y, rgb);
                }
            }
            f
        })
        .collect();

    faces.try_into().unwrap_or_else(|_| unreachable!())
}

/// Generate irradiance cubemap via cosine-weighted hemisphere convolution.
fn generate_irradiance(hdr: &HdrImage, size: u32) -> [CubemapFace; 6] {
    let sample_delta = 0.1; // Angular step for hemisphere integration

    let faces: Vec<CubemapFace> = (0..FACE_COUNT)
        .into_par_iter()
        .map(|face| {
            let mut f = CubemapFace::new(size);
            for y in 0..size {
                for x in 0..size {
                    let normal = face_texel_to_dir(face, x, y, size);
                    let irr = convolve_irradiance(hdr, normal, sample_delta);
                    f.set_pixel(x, y, irr);
                }
            }
            f
        })
        .collect();

    faces.try_into().unwrap_or_else(|_| unreachable!())
}

/// Cosine-weighted hemisphere convolution for a single normal direction.
fn convolve_irradiance(
    hdr: &HdrImage,
    normal: [f32; 3],
    sample_delta: f32,
) -> [f32; 3] {
    // Build tangent frame from normal
    let (tangent, bitangent) = build_tangent_frame(normal);

    let mut irradiance = [0.0f32; 3];
    let mut sample_count = 0.0f32;

    let mut phi = 0.0f32;
    while phi < 2.0 * PI {
        let mut theta = 0.0f32;
        while theta < 0.5 * PI {
            // Spherical to tangent-space direction
            let sin_theta = theta.sin();
            let cos_theta = theta.cos();
            let sin_phi = phi.sin();
            let cos_phi = phi.cos();

            let tangent_sample = [sin_theta * cos_phi, sin_theta * sin_phi, cos_theta];

            // Transform to world space
            let sample_dir = [
                tangent_sample[0] * tangent[0]
                    + tangent_sample[1] * bitangent[0]
                    + tangent_sample[2] * normal[0],
                tangent_sample[0] * tangent[1]
                    + tangent_sample[1] * bitangent[1]
                    + tangent_sample[2] * normal[1],
                tangent_sample[0] * tangent[2]
                    + tangent_sample[1] * bitangent[2]
                    + tangent_sample[2] * normal[2],
            ];

            let color = hdr.sample(sample_dir, 0.0);

            // cos(theta) * sin(theta) is the hemisphere solid angle weighting
            let weight = cos_theta * sin_theta;
            irradiance[0] += color[0] * weight;
            irradiance[1] += color[1] * weight;
            irradiance[2] += color[2] * weight;
            sample_count += 1.0;

            theta += sample_delta;
        }
        phi += sample_delta;
    }

    let scale = PI / sample_count;
    [
        irradiance[0] * scale,
        irradiance[1] * scale,
        irradiance[2] * scale,
    ]
}

/// Generate prefiltered specular cubemap with multiple roughness mip levels.
fn generate_prefiltered(
    hdr: &HdrImage,
    base_size: u32,
    mip_count: u32,
) -> Vec<[CubemapFace; 6]> {
    (0..mip_count)
        .into_par_iter()
        .map(|mip| {
            let roughness = mip as f32 / (mip_count - 1).max(1) as f32;
            let mip_size = (base_size >> mip).max(1);
            let sample_count = if roughness == 0.0 { 1 } else { 1024 };

            let faces: Vec<CubemapFace> = (0..FACE_COUNT)
                .into_par_iter()
                .map(|face| {
                    let mut f = CubemapFace::new(mip_size);
                    for y in 0..mip_size {
                        for x in 0..mip_size {
                            let normal = face_texel_to_dir(face, x, y, mip_size);
                            let color =
                                prefilter_ggx(hdr, normal, roughness, sample_count);
                            f.set_pixel(x, y, color);
                        }
                    }
                    f
                })
                .collect();

            faces.try_into().unwrap_or_else(|_| unreachable!())
        })
        .collect()
}

/// Importance-sample GGX for a single direction and roughness.
fn prefilter_ggx(
    hdr: &HdrImage,
    normal: [f32; 3],
    roughness: f32,
    sample_count: u32,
) -> [f32; 3] {
    // For roughness 0, just sample the reflection direction
    if roughness == 0.0 {
        return hdr.sample(normal, 0.0);
    }

    let view = normal; // Assume V == N (split-sum approximation)
    let (tangent, bitangent) = build_tangent_frame(normal);

    let mut color = [0.0f32; 3];
    let mut total_weight = 0.0f32;

    for i in 0..sample_count {
        let xi = hammersley(i, sample_count);
        let h = importance_sample_ggx(xi, roughness, normal, tangent, bitangent);

        // Reflect view around half-vector
        let v_dot_h = dot(view, h);
        let l = [
            2.0 * v_dot_h * h[0] - view[0],
            2.0 * v_dot_h * h[1] - view[1],
            2.0 * v_dot_h * h[2] - view[2],
        ];

        let n_dot_l = dot(normal, l).max(0.0);
        if n_dot_l > 0.0 {
            let sample = hdr.sample(l, 0.0);
            color[0] += sample[0] * n_dot_l;
            color[1] += sample[1] * n_dot_l;
            color[2] += sample[2] * n_dot_l;
            total_weight += n_dot_l;
        }
    }

    if total_weight > 0.0 {
        color[0] /= total_weight;
        color[1] /= total_weight;
        color[2] /= total_weight;
    }

    color
}

/// Generate the BRDF integration LUT.
///
/// Each texel stores (scale, bias) for the split-sum approximation:
/// `F0 * scale + bias` gives the specular reflection contribution.
pub fn generate_brdf_lut(size: u32) -> BrdfLut {
    let pixels: Vec<[f32; 2]> = (0..size * size)
        .into_par_iter()
        .map(|idx| {
            let x = idx % size;
            let y = idx / size;

            let n_dot_v = (x as f32 + 0.5) / size as f32;
            let roughness = (y as f32 + 0.5) / size as f32;

            integrate_brdf(n_dot_v.max(0.001), roughness)
        })
        .collect();

    BrdfLut { size, pixels }
}

/// Integrate BRDF for a given NdotV and roughness.
fn integrate_brdf(n_dot_v: f32, roughness: f32) -> [f32; 2] {
    let v = [
        (1.0 - n_dot_v * n_dot_v).sqrt(), // sin
        0.0,
        n_dot_v, // cos
    ];
    let normal = [0.0, 0.0, 1.0];
    let tangent = [1.0, 0.0, 0.0];
    let bitangent = [0.0, 1.0, 0.0];

    let mut scale = 0.0f32;
    let mut bias = 0.0f32;
    let sample_count = 1024u32;

    for i in 0..sample_count {
        let xi = hammersley(i, sample_count);
        let h = importance_sample_ggx(xi, roughness, normal, tangent, bitangent);

        let v_dot_h = dot(v, h).max(0.0);
        let l = [
            2.0 * v_dot_h * h[0] - v[0],
            2.0 * v_dot_h * h[1] - v[1],
            2.0 * v_dot_h * h[2] - v[2],
        ];

        let n_dot_l = l[2].max(0.0); // normal is [0,0,1]
        let n_dot_h = h[2].max(0.0);

        if n_dot_l > 0.0 {
            let g = geometry_smith(normal, v, l, roughness);
            let g_vis = (g * v_dot_h) / (n_dot_h * n_dot_v).max(0.001);
            let fc = (1.0 - v_dot_h).powi(5);

            scale += (1.0 - fc) * g_vis;
            bias += fc * g_vis;
        }
    }

    [scale / sample_count as f32, bias / sample_count as f32]
}

// ─── Helper functions ──────────────────────────────────────────────────────

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len < 1e-10 {
        return [0.0, 1.0, 0.0];
    }
    [v[0] / len, v[1] / len, v[2] / len]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// Build an orthonormal tangent frame from a normal vector.
fn build_tangent_frame(normal: [f32; 3]) -> ([f32; 3], [f32; 3]) {
    let up = if normal[1].abs() < 0.999 {
        [0.0, 1.0, 0.0]
    } else {
        [1.0, 0.0, 0.0]
    };
    let tangent = normalize(cross(up, normal));
    let bitangent = cross(normal, tangent);
    (tangent, bitangent)
}

/// Hammersley quasi-random sequence point.
fn hammersley(i: u32, n: u32) -> [f32; 2] {
    [i as f32 / n as f32, radical_inverse_vdc(i)]
}

/// Van der Corput radical inverse (base 2).
fn radical_inverse_vdc(bits: u32) -> f32 {
    bits.reverse_bits() as f32 * 2.328_306_4e-10 // 1.0 / 0x100000000
}

/// Importance-sample the GGX NDF to get a half-vector.
fn importance_sample_ggx(
    xi: [f32; 2],
    roughness: f32,
    _normal: [f32; 3],
    tangent: [f32; 3],
    bitangent: [f32; 3],
) -> [f32; 3] {
    let a = roughness * roughness;

    let phi = 2.0 * PI * xi[0];
    let cos_theta = ((1.0 - xi[1]) / (1.0 + (a * a - 1.0) * xi[1])).sqrt();
    let sin_theta = (1.0 - cos_theta * cos_theta).sqrt();

    // Spherical to tangent-space
    let hx = phi.cos() * sin_theta;
    let hy = phi.sin() * sin_theta;
    let hz = cos_theta;

    // Transform to world space
    normalize([
        hx * tangent[0] + hy * bitangent[0] + hz * _normal[0],
        hx * tangent[1] + hy * bitangent[1] + hz * _normal[1],
        hx * tangent[2] + hy * bitangent[2] + hz * _normal[2],
    ])
}

/// Smith's geometry function for GGX (Schlick-GGX approximation).
fn geometry_smith(_normal: [f32; 3], v: [f32; 3], l: [f32; 3], roughness: f32) -> f32 {
    let n_dot_v = v[2].max(0.0); // normal is [0,0,1] for BRDF integration
    let n_dot_l = l[2].max(0.0);
    geometry_schlick_ggx(n_dot_v, roughness) * geometry_schlick_ggx(n_dot_l, roughness)
}

/// Schlick-GGX geometry sub-function (for IBL, k = roughness^2 / 2).
fn geometry_schlick_ggx(n_dot_v: f32, roughness: f32) -> f32 {
    let a = roughness;
    let k = (a * a) / 2.0;
    n_dot_v / (n_dot_v * (1.0 - k) + k)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hdr::HdrImage;

    fn test_hdr_path() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../legacy/go-raytracing/assets/hdri/abandoned_hall_01_1k.hdr")
    }

    #[test]
    fn cubemap_face_directions() {
        // Center of +X face should point in +X direction
        let dir = face_texel_to_dir(0, CUBEMAP_SIZE / 2, CUBEMAP_SIZE / 2, CUBEMAP_SIZE);
        assert!(dir[0] > 0.9, "+X face center should point +X: {:?}", dir);

        // Center of -X face should point in -X direction
        let dir = face_texel_to_dir(1, CUBEMAP_SIZE / 2, CUBEMAP_SIZE / 2, CUBEMAP_SIZE);
        assert!(dir[0] < -0.9, "-X face center should point -X: {:?}", dir);

        // Center of +Y face should point up
        let dir = face_texel_to_dir(2, CUBEMAP_SIZE / 2, CUBEMAP_SIZE / 2, CUBEMAP_SIZE);
        assert!(dir[1] > 0.9, "+Y face center should point +Y: {:?}", dir);
    }

    #[test]
    fn brdf_lut_values_sane() {
        let lut = generate_brdf_lut(64);
        assert_eq!(lut.pixels.len(), 64 * 64);

        // All values should be in [0, 1]
        for pixel in &lut.pixels {
            assert!(
                pixel[0] >= 0.0 && pixel[0] <= 1.0,
                "BRDF scale out of range: {}",
                pixel[0]
            );
            assert!(
                pixel[1] >= 0.0 && pixel[1] <= 1.0,
                "BRDF bias out of range: {}",
                pixel[1]
            );
        }

        // At NdotV=1.0, roughness=0 (bottom-left corner): scale should be ~1.0, bias ~0.0
        let corner = lut.pixels[(64 - 1) as usize]; // x=63 (NdotV~1), y=0 (roughness~0)
        assert!(
            corner[0] > 0.8,
            "Scale at NdotV=1, rough=0 should be ~1: {}",
            corner[0]
        );
    }

    #[test]
    fn radical_inverse_vdc_known_values() {
        assert!((radical_inverse_vdc(1) - 0.5).abs() < 1e-6);
        assert!((radical_inverse_vdc(2) - 0.25).abs() < 1e-6);
        assert_eq!(radical_inverse_vdc(0), 0.0);
    }

    #[test]
    fn hammersley_coverage() {
        // First point should be near origin
        let p = hammersley(0, 16);
        assert_eq!(p[0], 0.0);
        assert_eq!(p[1], 0.0);

        // Points should be in [0, 1)
        for i in 0..16 {
            let p = hammersley(i, 16);
            assert!(p[0] >= 0.0 && p[0] < 1.0);
            assert!(p[1] >= 0.0 && p[1] < 1.0);
        }
    }

    #[test]
    fn generate_cubemap_from_hdr() {
        let hdr = HdrImage::load(test_hdr_path()).expect("Failed to load HDR");
        let cubemap = generate_cubemap(&hdr, 16);

        assert_eq!(cubemap.len(), 6);
        for face in &cubemap {
            assert_eq!(face.size, 16);
            assert_eq!(face.pixels.len(), 16 * 16);
            // Check pixels are finite and non-negative
            for p in &face.pixels {
                assert!(p[0].is_finite() && p[0] >= 0.0);
                assert!(p[1].is_finite() && p[1] >= 0.0);
                assert!(p[2].is_finite() && p[2] >= 0.0);
            }
        }
    }

    #[test]
    fn irradiance_is_smooth() {
        let hdr = HdrImage::load(test_hdr_path()).expect("Failed to load HDR");
        let irradiance = generate_irradiance(&hdr, 4);

        // Irradiance should be relatively smooth - all values positive
        for face in &irradiance {
            for p in &face.pixels {
                assert!(p[0] >= 0.0 && p[0].is_finite());
                assert!(p[1] >= 0.0 && p[1].is_finite());
                assert!(p[2] >= 0.0 && p[2].is_finite());
            }
        }
    }
}
