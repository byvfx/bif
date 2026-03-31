//! Explicit light sources for path tracing.
//!
//! Supports USD light types: Distant, Point (Sphere), Rect, and Dome (via HDRI).
//! These lights are sampled via Next Event Estimation (NEE) alongside HDRI.

use crate::material::gen_f32;
use bif_math::Vec3;
use rand::RngCore;

/// Light sample result from NEE sampling.
#[derive(Debug, Clone, Copy)]
pub struct LightSample {
    /// Direction from shading point to light
    pub direction: Vec3,
    /// Light emission (radiance * geometric factor)
    pub emission: Vec3,
    /// PDF of sampling this direction
    pub pdf: f32,
    /// Distance to light (f32::INFINITY for distant lights)
    pub distance: f32,
    /// Whether this light is a delta distribution (point/distant with zero extent)
    pub is_delta: bool,
}

/// Rec.709 luminance from linear RGB.
fn luminance(c: Vec3) -> f32 {
    0.2126 * c.x + 0.7152 * c.y + 0.0722 * c.z
}

/// Trait for light sources that can be sampled for NEE.
pub trait Light: Send + Sync {
    /// Sample a direction from the shading point toward the light.
    /// Returns (direction, emission, pdf, distance).
    fn sample(&self, point: Vec3, rng: &mut dyn RngCore) -> LightSample;

    /// Evaluate PDF for a given direction from point.
    fn pdf(&self, point: Vec3, direction: Vec3) -> f32;

    /// Check if this is a delta light (point/distant with zero radius/angle).
    fn is_delta(&self) -> bool {
        false
    }

    /// Emitted power estimate for importance-weighted light selection.
    fn power(&self) -> f32;
}

/// Distant (directional) light - like the sun.
pub struct DistantLight {
    /// Direction the light is shining (toward scene)
    pub direction: Vec3,
    /// Light color
    pub color: Vec3,
    /// Light intensity
    pub intensity: f32,
    /// Angular diameter in radians (0 = point source).
    /// Stored in radians; callers pass degrees, converted in constructor.
    pub angle: f32,
}

impl DistantLight {
    /// Create a distant light. `angle_degrees` is the angular diameter in degrees
    /// (matching USD's UsdLuxDistantLight convention, e.g. 0.53 for sun disk).
    pub fn new(direction: Vec3, color: Vec3, intensity: f32, angle_degrees: f32) -> Self {
        Self {
            direction: direction.normalize(),
            color,
            intensity,
            angle: angle_degrees.to_radians(),
        }
    }
}

impl Light for DistantLight {
    fn sample(&self, _point: Vec3, rng: &mut dyn RngCore) -> LightSample {
        // For non-zero angle, jitter direction within cone
        // self.angle is angular diameter in radians; half-angle defines the cone
        let dir = if self.angle > 0.0 {
            let cos_max = (self.angle * 0.5).cos();
            sample_cone(-self.direction, cos_max, rng)
        } else {
            -self.direction
        };

        LightSample {
            direction: dir,
            emission: self.color * self.intensity,
            pdf: if self.angle > 0.0 {
                let cos_max = (self.angle * 0.5).cos();
                1.0 / (std::f32::consts::TAU * (1.0 - cos_max))
            } else {
                1.0 // Delta PDF
            },
            distance: f32::INFINITY,
            is_delta: self.angle <= 0.0,
        }
    }

    fn pdf(&self, _point: Vec3, direction: Vec3) -> f32 {
        if self.angle > 0.0 {
            let cos_theta = direction.dot(-self.direction);
            let cos_max = (self.angle * 0.5).cos();
            if cos_theta >= cos_max {
                1.0 / (std::f32::consts::TAU * (1.0 - cos_max))
            } else {
                0.0
            }
        } else {
            0.0 // Delta light
        }
    }

    fn is_delta(&self) -> bool {
        self.angle <= 0.0
    }

    fn power(&self) -> f32 {
        luminance(self.color) * self.intensity
    }
}

/// Point/Sphere light.
pub struct SphereLight {
    /// Light center position
    pub position: Vec3,
    /// Light color
    pub color: Vec3,
    /// Light intensity
    pub intensity: f32,
    /// Light radius (0 = point source)
    pub radius: f32,
}

impl SphereLight {
    pub fn new(position: Vec3, color: Vec3, intensity: f32, radius: f32) -> Self {
        Self {
            position,
            color,
            intensity,
            radius,
        }
    }
}

impl Light for SphereLight {
    fn sample(&self, point: Vec3, rng: &mut dyn RngCore) -> LightSample {
        let to_light = self.position - point;
        let distance = to_light.length();

        if self.radius > 0.0 && distance > self.radius {
            // Sample visible disk of sphere (cone sampling)
            let sin_theta_max = (self.radius / distance).min(1.0);
            let cos_theta_max = (1.0 - sin_theta_max * sin_theta_max).sqrt();
            let dir_to_center = to_light / distance;

            let sampled_dir = sample_cone(dir_to_center, cos_theta_max, rng);

            // Proper solid-angle falloff: sin²(θ_max) where θ_max = asin(radius/distance)
            // For far-field (distance >> radius), reduces to (radius/distance)² ≈ inverse-square
            let falloff = sin_theta_max * sin_theta_max;

            LightSample {
                direction: sampled_dir,
                emission: self.color * self.intensity * falloff,
                pdf: 1.0 / (std::f32::consts::TAU * (1.0 - cos_theta_max)),
                distance: distance.max(0.001),
                is_delta: false,
            }
        } else {
            // Point light or inside sphere
            let dir = to_light / distance;
            let falloff = 1.0 / (distance * distance).max(0.001);

            LightSample {
                direction: dir,
                emission: self.color * self.intensity * falloff,
                pdf: 1.0, // Delta PDF
                distance,
                is_delta: true,
            }
        }
    }

    fn pdf(&self, point: Vec3, direction: Vec3) -> f32 {
        let to_light = self.position - point;
        let distance = to_light.length();

        if self.radius > 0.0 && distance > self.radius {
            let sin_theta_max = (self.radius / distance).min(1.0);
            let cos_theta_max = (1.0 - sin_theta_max * sin_theta_max).sqrt();
            let dir_to_center = to_light / distance;

            let cos_theta = direction.dot(dir_to_center);
            if cos_theta >= cos_theta_max {
                1.0 / (std::f32::consts::TAU * (1.0 - cos_theta_max))
            } else {
                0.0
            }
        } else {
            0.0 // Delta or inside
        }
    }

    fn is_delta(&self) -> bool {
        self.radius <= 0.0
    }

    fn power(&self) -> f32 {
        luminance(self.color) * self.intensity
    }
}

/// Rectangular area light.
pub struct RectLight {
    /// Center position
    pub center: Vec3,
    /// U axis (width direction, length = half width)
    pub u_axis: Vec3,
    /// V axis (height direction, length = half height)
    pub v_axis: Vec3,
    /// Normal direction
    pub normal: Vec3,
    /// Light color
    pub color: Vec3,
    /// Light intensity
    pub intensity: f32,
}

impl RectLight {
    pub fn new(center: Vec3, u_axis: Vec3, v_axis: Vec3, color: Vec3, intensity: f32) -> Self {
        let normal = u_axis.cross(v_axis).normalize();
        Self {
            center,
            u_axis,
            v_axis,
            normal,
            color,
            intensity,
        }
    }

    /// Create from a transform matrix and dimensions.
    /// USD lights emit along local -Z, so normal uses -z_axis from the transform.
    pub fn from_transform(
        transform: bif_math::Mat4,
        width: f32,
        height: f32,
        color: Vec3,
        intensity: f32,
    ) -> Self {
        let center = transform.w_axis.truncate();
        let u_axis = transform.x_axis.truncate() * (width * 0.5);
        let v_axis = transform.y_axis.truncate() * (height * 0.5);
        let normal = (-transform.z_axis.truncate()).normalize();

        Self {
            center,
            u_axis,
            v_axis,
            normal,
            color,
            intensity,
        }
    }
}

impl Light for RectLight {
    fn sample(&self, point: Vec3, rng: &mut dyn RngCore) -> LightSample {
        // Uniform sampling on rectangle
        let u = gen_f32(rng) * 2.0 - 1.0;
        let v = gen_f32(rng) * 2.0 - 1.0;
        let light_point = self.center + self.u_axis * u + self.v_axis * v;

        let to_light = light_point - point;
        let distance = to_light.length();
        let direction = to_light / distance;

        // Area of rectangle
        let area = self.u_axis.length() * self.v_axis.length() * 4.0;

        // Cosine at light (light facing shading point)
        let cos_light = self.normal.dot(-direction).max(0.0);

        // Convert area PDF to solid angle PDF
        let pdf = if cos_light > 0.0 {
            (distance * distance) / (area * cos_light)
        } else {
            0.0
        };

        // Emission = radiance L_e (no distance falloff — PDF handles it)
        let emission = if cos_light > 0.0 {
            self.color * self.intensity
        } else {
            Vec3::ZERO
        };

        LightSample {
            direction,
            emission,
            pdf,
            distance,
            is_delta: false,
        }
    }

    fn pdf(&self, point: Vec3, direction: Vec3) -> f32 {
        // Check if direction hits the rectangle
        let denom = direction.dot(self.normal);
        if denom.abs() < 1e-6 {
            return 0.0;
        }

        let t = (self.center - point).dot(self.normal) / denom;
        if t <= 0.0 {
            return 0.0;
        }

        let hit_point = point + direction * t;
        let offset = hit_point - self.center;

        // Project onto local axes
        let u_len = self.u_axis.length();
        let v_len = self.v_axis.length();
        let u_coord = offset.dot(self.u_axis.normalize()) / u_len;
        let v_coord = offset.dot(self.v_axis.normalize()) / v_len;

        if u_coord.abs() <= 1.0 && v_coord.abs() <= 1.0 {
            let area = u_len * v_len * 4.0;
            let cos_light = self.normal.dot(-direction).abs();
            if cos_light > 0.0 {
                (t * t) / (area * cos_light)
            } else {
                0.0
            }
        } else {
            0.0
        }
    }

    fn power(&self) -> f32 {
        let area = self.u_axis.length() * self.v_axis.length() * 4.0;
        luminance(self.color) * self.intensity * area
    }
}

/// Collection of explicit lights for a scene.
/// Uses power-weighted importance sampling for light selection.
pub struct LightList {
    lights: Vec<Box<dyn Light>>,
    /// Cumulative distribution function built from light power estimates.
    /// Entry i = sum of normalized powers for lights 0..=i.
    power_cdf: Vec<f32>,
    /// Sum of all light powers (for normalization).
    total_power: f32,
}

impl Default for LightList {
    fn default() -> Self {
        Self {
            lights: Vec::new(),
            power_cdf: Vec::new(),
            total_power: 0.0,
        }
    }
}

impl std::fmt::Debug for LightList {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LightList")
            .field("count", &self.lights.len())
            .finish()
    }
}

impl LightList {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, light: Box<dyn Light>) {
        self.lights.push(light);
        self.rebuild_cdf();
    }

    pub fn len(&self) -> usize {
        self.lights.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lights.is_empty()
    }

    /// Rebuild the power CDF from current lights.
    fn rebuild_cdf(&mut self) {
        self.total_power = self.lights.iter().map(|l| l.power()).sum();

        if self.total_power <= 0.0 {
            // All lights have zero power — fall back to uniform
            let n = self.lights.len() as f32;
            self.power_cdf = self
                .lights
                .iter()
                .enumerate()
                .map(|(i, _)| (i + 1) as f32 / n)
                .collect();
            self.total_power = 0.0;
        } else {
            let mut cumulative = 0.0;
            self.power_cdf = self
                .lights
                .iter()
                .map(|l| {
                    cumulative += l.power() / self.total_power;
                    cumulative
                })
                .collect();
        }

        // Ensure last entry is exactly 1.0 to avoid floating-point edge cases
        if let Some(last) = self.power_cdf.last_mut() {
            *last = 1.0;
        }
    }

    /// Selection probability for light at index.
    fn selection_prob(&self, idx: usize) -> f32 {
        if self.lights.is_empty() {
            return 0.0;
        }
        if self.total_power <= 0.0 {
            // Uniform fallback
            return 1.0 / self.lights.len() as f32;
        }
        let prev = if idx > 0 {
            self.power_cdf[idx - 1]
        } else {
            0.0
        };
        self.power_cdf[idx] - prev
    }

    /// Sample one light weighted by emitted power.
    pub fn sample_one(&self, point: Vec3, rng: &mut dyn RngCore) -> Option<(LightSample, usize)> {
        if self.lights.is_empty() {
            return None;
        }

        // Binary search the power CDF
        let xi = gen_f32(rng);
        let idx = self
            .power_cdf
            .partition_point(|&c| c < xi)
            .min(self.lights.len() - 1);

        let mut sample = self.lights[idx].sample(point, rng);
        // Include power-weighted selection probability in PDF
        sample.pdf *= self.selection_prob(idx);
        Some((sample, idx))
    }

    /// Get light at index.
    pub fn get(&self, idx: usize) -> Option<&dyn Light> {
        self.lights.get(idx).map(|l| l.as_ref())
    }

    /// PDF for sampling a specific light.
    pub fn pdf_for_light(&self, idx: usize, point: Vec3, direction: Vec3) -> f32 {
        if let Some(light) = self.lights.get(idx) {
            light.pdf(point, direction) * self.selection_prob(idx)
        } else {
            0.0
        }
    }

    /// Combined PDF across all lights for a direction.
    pub fn pdf(&self, point: Vec3, direction: Vec3) -> f32 {
        if self.lights.is_empty() {
            return 0.0;
        }

        let mut total = 0.0;
        for (i, light) in self.lights.iter().enumerate() {
            total += light.pdf(point, direction) * self.selection_prob(i);
        }
        total
    }

    /// Iterate over lights.
    pub fn iter(&self) -> impl Iterator<Item = &dyn Light> {
        self.lights.iter().map(|l| l.as_ref())
    }
}

/// Create LightList from bif_core::Light scene lights.
impl From<&[bif_core::Light]> for LightList {
    fn from(lights: &[bif_core::Light]) -> Self {
        let mut list = LightList {
            lights: Vec::new(),
            power_cdf: Vec::new(),
            total_power: 0.0,
        };

        for light in lights {
            match light {
                bif_core::Light::Distant {
                    direction,
                    color,
                    intensity,
                    angle,
                } => {
                    list.lights.push(Box::new(DistantLight::new(
                        *direction, *color, *intensity, *angle,
                    )));
                }
                bif_core::Light::Point {
                    position,
                    color,
                    intensity,
                    radius,
                } => {
                    list.lights.push(Box::new(SphereLight::new(
                        *position, *color, *intensity, *radius,
                    )));
                }
                bif_core::Light::Rect {
                    transform,
                    color,
                    intensity,
                    width,
                    height,
                } => {
                    list.lights.push(Box::new(RectLight::from_transform(
                        *transform, *width, *height, *color, *intensity,
                    )));
                }
                bif_core::Light::Dome { .. } => {
                    // DomeLights are handled via HdriEnvironment, not as explicit lights
                }
            }
        }

        list.rebuild_cdf();
        list
    }
}

// Helper functions

/// Sample a direction uniformly within a cone around `axis` with `cos_max` defining the cone.
fn sample_cone(axis: Vec3, cos_max: f32, rng: &mut dyn RngCore) -> Vec3 {
    let r1 = gen_f32(rng);
    let r2 = gen_f32(rng);

    let cos_theta = 1.0 - r1 * (1.0 - cos_max);
    let sin_theta = (1.0 - cos_theta * cos_theta).sqrt();
    let phi = std::f32::consts::TAU * r2;

    // Local coordinates
    let x = sin_theta * phi.cos();
    let y = sin_theta * phi.sin();
    let z = cos_theta;

    // Transform to world (build orthonormal basis from axis)
    let (u, v) = bif_math::build_orthonormal_basis(axis);
    (u * x + v * y + axis * z).normalize()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    fn test_rng() -> StdRng {
        StdRng::seed_from_u64(42)
    }

    #[test]
    fn power_distant_light() {
        let light = DistantLight::new(Vec3::new(0.0, -1.0, 0.0), Vec3::ONE, 5.0, 0.53);
        assert!((light.power() - 5.0).abs() < 1e-4);
    }

    #[test]
    fn power_sphere_light() {
        let light = SphereLight::new(Vec3::ZERO, Vec3::new(1.0, 0.0, 0.0), 10.0, 0.5);
        // luminance(1,0,0) = 0.2126, * 10.0 = 2.126
        assert!((light.power() - 2.126).abs() < 1e-4);
    }

    #[test]
    fn power_rect_light() {
        // u_axis length=1 (half-width), v_axis length=1 (half-height) → area = 4.0
        let light = RectLight::new(
            Vec3::ZERO,
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::ONE,
            2.0,
        );
        // luminance(1,1,1) = 1.0, * 2.0 * 4.0 = 8.0
        assert!((light.power() - 8.0).abs() < 1e-4);
    }

    #[test]
    fn cdf_sums_to_one() {
        let mut list = LightList::new();
        list.add(Box::new(DistantLight::new(
            Vec3::NEG_Y,
            Vec3::ONE,
            1.0,
            0.0,
        )));
        list.add(Box::new(SphereLight::new(Vec3::ZERO, Vec3::ONE, 10.0, 0.5)));
        assert!((list.power_cdf.last().copied().unwrap() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn power_weighted_distribution() {
        let mut list = LightList::new();
        // Bright light: power = 100 * luminance(1,1,1) = 100
        list.add(Box::new(DistantLight::new(
            Vec3::NEG_Y,
            Vec3::ONE,
            100.0,
            0.53,
        )));
        // Dim light: power = 1 * luminance(1,1,1) = 1
        list.add(Box::new(DistantLight::new(
            Vec3::NEG_Y,
            Vec3::ONE,
            1.0,
            0.53,
        )));

        let mut rng = test_rng();
        let point = Vec3::ZERO;
        let mut bright_count = 0u32;
        let n = 10_000;

        for _ in 0..n {
            if let Some((_, idx)) = list.sample_one(point, &mut rng) {
                if idx == 0 {
                    bright_count += 1;
                }
            }
        }

        // Bright light should get ~99% of samples (100/101)
        let ratio = bright_count as f64 / n as f64;
        assert!(
            ratio > 0.95,
            "bright light sampled {ratio:.3}, expected ~0.99"
        );
    }

    #[test]
    fn equal_power_is_uniform() {
        let mut list = LightList::new();
        list.add(Box::new(DistantLight::new(
            Vec3::NEG_Y,
            Vec3::ONE,
            5.0,
            0.53,
        )));
        list.add(Box::new(DistantLight::new(
            Vec3::NEG_Y,
            Vec3::ONE,
            5.0,
            0.53,
        )));

        // Selection probs should be equal
        let p0 = list.selection_prob(0);
        let p1 = list.selection_prob(1);
        assert!((p0 - p1).abs() < 1e-6, "expected equal probs: {p0} vs {p1}");
        assert!((p0 - 0.5).abs() < 1e-6);
    }

    #[test]
    fn single_light_always_selected() {
        let mut list = LightList::new();
        list.add(Box::new(SphereLight::new(
            Vec3::new(0.0, 5.0, 0.0),
            Vec3::ONE,
            10.0,
            0.5,
        )));

        let mut rng = test_rng();
        for _ in 0..100 {
            let (_, idx) = list.sample_one(Vec3::ZERO, &mut rng).unwrap();
            assert_eq!(idx, 0);
        }
        assert!((list.selection_prob(0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn zero_power_falls_back_to_uniform() {
        let mut list = LightList::new();
        // Zero-intensity lights
        list.add(Box::new(DistantLight::new(
            Vec3::NEG_Y,
            Vec3::ONE,
            0.0,
            0.53,
        )));
        list.add(Box::new(DistantLight::new(
            Vec3::NEG_Y,
            Vec3::ONE,
            0.0,
            0.53,
        )));

        assert_eq!(list.total_power, 0.0);
        let p0 = list.selection_prob(0);
        let p1 = list.selection_prob(1);
        assert!((p0 - 0.5).abs() < 1e-6, "expected uniform fallback");
        assert!((p1 - 0.5).abs() < 1e-6);
    }

    #[test]
    fn pdf_uses_power_weights() {
        let mut list = LightList::new();
        // Bright sphere light at (0, 5, 0)
        list.add(Box::new(SphereLight::new(
            Vec3::new(0.0, 5.0, 0.0),
            Vec3::ONE,
            100.0,
            1.0,
        )));
        // Dim sphere light at (5, 0, 0)
        list.add(Box::new(SphereLight::new(
            Vec3::new(5.0, 0.0, 0.0),
            Vec3::ONE,
            1.0,
            1.0,
        )));

        let point = Vec3::ZERO;
        let dir_to_bright = Vec3::new(0.0, 1.0, 0.0);

        let combined_pdf = list.pdf(point, dir_to_bright);
        let bright_pdf = list.pdf_for_light(0, point, dir_to_bright);

        // Combined PDF should be dominated by the bright light's contribution
        assert!(
            combined_pdf > 0.0,
            "combined pdf should be positive for direction toward a light"
        );
        assert!(
            (combined_pdf - bright_pdf).abs() < 1e-4,
            "combined pdf should ~equal bright light's weighted pdf"
        );
    }
}
