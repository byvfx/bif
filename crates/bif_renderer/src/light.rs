//! Explicit light sources for path tracing.
//!
//! Supports USD light types: Distant, Point (Sphere), Rect, and Dome (via HDRI).
//! These lights are sampled via Next Event Estimation (NEE) alongside HDRI.

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
}

/// Distant (directional) light - like the sun.
pub struct DistantLight {
    /// Direction the light is shining (toward scene)
    pub direction: Vec3,
    /// Light color
    pub color: Vec3,
    /// Light intensity
    pub intensity: f32,
    /// Angular diameter in radians (0 = point source)
    pub angle: f32,
}

impl DistantLight {
    pub fn new(direction: Vec3, color: Vec3, intensity: f32, angle: f32) -> Self {
        Self {
            direction: direction.normalize(),
            color,
            intensity,
            angle,
        }
    }
}

impl Light for DistantLight {
    fn sample(&self, _point: Vec3, rng: &mut dyn RngCore) -> LightSample {
        // For non-zero angle, jitter direction within cone
        let dir = if self.angle > 0.0 {
            let cos_max = (1.0 - self.angle * 0.5).cos();
            sample_cone(-self.direction, cos_max, rng)
        } else {
            -self.direction
        };

        LightSample {
            direction: dir,
            emission: self.color * self.intensity,
            pdf: if self.angle > 0.0 {
                let cos_max = (1.0 - self.angle * 0.5).cos();
                1.0 / (std::f32::consts::TAU * (1.0 - cos_max))
            } else {
                1.0 // Delta PDF
            },
            distance: f32::INFINITY,
        }
    }

    fn pdf(&self, _point: Vec3, direction: Vec3) -> f32 {
        if self.angle > 0.0 {
            let cos_theta = direction.dot(-self.direction);
            let cos_max = (1.0 - self.angle * 0.5).cos();
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

            // Compute actual distance to sphere surface
            let actual_distance = distance - self.radius;

            // Inverse square falloff
            let falloff = 1.0 / (actual_distance * actual_distance + 0.01);

            LightSample {
                direction: sampled_dir,
                emission: self.color * self.intensity * falloff,
                pdf: 1.0 / (std::f32::consts::TAU * (1.0 - cos_theta_max)),
                distance: actual_distance.max(0.001),
            }
        } else {
            // Point light or inside sphere
            let dir = to_light / distance;
            let falloff = 1.0 / (distance * distance + 0.01);

            LightSample {
                direction: dir,
                emission: self.color * self.intensity * falloff,
                pdf: 1.0, // Delta PDF
                distance,
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
    pub fn from_transform(
        transform: bif_math::Mat4,
        width: f32,
        height: f32,
        color: Vec3,
        intensity: f32,
    ) -> Self {
        let center = transform.w_axis.truncate();
        let u_axis = transform.x_axis.truncate().normalize() * (width * 0.5);
        let v_axis = transform.y_axis.truncate().normalize() * (height * 0.5);
        Self::new(center, u_axis, v_axis, color, intensity)
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

        // Falloff based on distance
        let falloff = 1.0 / (distance * distance + 0.01);

        LightSample {
            direction,
            emission: self.color * self.intensity * falloff * cos_light,
            pdf,
            distance,
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
}

/// Collection of explicit lights for a scene.
#[derive(Default)]
pub struct LightList {
    lights: Vec<Box<dyn Light>>,
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
        Self { lights: Vec::new() }
    }

    pub fn add(&mut self, light: Box<dyn Light>) {
        self.lights.push(light);
    }

    pub fn len(&self) -> usize {
        self.lights.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lights.is_empty()
    }

    /// Sample all lights uniformly, returning combined contribution.
    /// For MIS, each light is sampled independently and weighted.
    pub fn sample_one(&self, point: Vec3, rng: &mut dyn RngCore) -> Option<(LightSample, usize)> {
        if self.lights.is_empty() {
            return None;
        }

        // Uniform light selection
        let idx = (gen_f32(rng) * self.lights.len() as f32) as usize;
        let idx = idx.min(self.lights.len() - 1);

        let sample = self.lights[idx].sample(point, rng);
        Some((sample, idx))
    }

    /// Get light at index.
    pub fn get(&self, idx: usize) -> Option<&dyn Light> {
        self.lights.get(idx).map(|l| l.as_ref())
    }

    /// PDF for sampling a specific light.
    pub fn pdf_for_light(&self, idx: usize, point: Vec3, direction: Vec3) -> f32 {
        if let Some(light) = self.lights.get(idx) {
            // Uniform selection probability * light pdf
            light.pdf(point, direction) / self.lights.len() as f32
        } else {
            0.0
        }
    }

    /// Combined PDF across all lights for a direction.
    pub fn pdf(&self, point: Vec3, direction: Vec3) -> f32 {
        if self.lights.is_empty() {
            return 0.0;
        }

        // Sum of weighted PDFs
        let mut total = 0.0;
        for light in &self.lights {
            total += light.pdf(point, direction);
        }
        total / self.lights.len() as f32
    }

    /// Iterate over lights.
    pub fn iter(&self) -> impl Iterator<Item = &dyn Light> {
        self.lights.iter().map(|l| l.as_ref())
    }
}

/// Create LightList from bif_core::Light scene lights.
impl From<&[bif_core::Light]> for LightList {
    fn from(lights: &[bif_core::Light]) -> Self {
        let mut list = LightList::new();

        for light in lights {
            match light {
                bif_core::Light::Distant {
                    direction,
                    color,
                    intensity,
                    angle,
                } => {
                    list.add(Box::new(DistantLight::new(
                        *direction, *color, *intensity, *angle,
                    )));
                }
                bif_core::Light::Point {
                    position,
                    color,
                    intensity,
                    radius,
                } => {
                    list.add(Box::new(SphereLight::new(
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
                    list.add(Box::new(RectLight::from_transform(
                        *transform, *width, *height, *color, *intensity,
                    )));
                }
                bif_core::Light::Dome { .. } => {
                    // DomeLights are handled via HdriEnvironment, not as explicit lights
                }
            }
        }

        list
    }
}

// Helper functions

/// Sample a direction uniformly within a cone around `axis` with cos_max defining the cone.
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
    let (u, v) = orthonormal_basis(axis);
    (u * x + v * y + axis * z).normalize()
}

/// Build orthonormal basis from a single vector.
fn orthonormal_basis(n: Vec3) -> (Vec3, Vec3) {
    let sign = if n.z >= 0.0 { 1.0 } else { -1.0 };
    let a = -1.0 / (sign + n.z);
    let b = n.x * n.y * a;
    let u = Vec3::new(1.0 + sign * n.x * n.x * a, sign * b, -sign * n.x);
    let v = Vec3::new(b, sign + n.y * n.y * a, -n.y);
    (u, v)
}

/// Generate random f32 in [0, 1).
fn gen_f32(rng: &mut dyn RngCore) -> f32 {
    (rng.next_u32() as f64 / u32::MAX as f64) as f32
}
