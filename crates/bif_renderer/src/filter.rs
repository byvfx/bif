//! Pixel reconstruction filters for path tracing.
//!
//! Filters weight samples based on their sub-pixel offset from the pixel center,
//! improving image quality over the implicit box filter (equal-weight averaging).

use serde::{Deserialize, Serialize};
use std::f32::consts::PI;

/// Available pixel reconstruction filters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum PixelFilter {
    /// Equal-weight box filter (radius 0.5). Equivalent to no filter.
    #[default]
    Box,
    /// Gaussian filter — smooth, slight blurring.
    Gaussian,
    /// Mitchell-Netravali B=1/3, C=1/3 — balanced sharpness/ringing.
    Mitchell,
    /// Blackman-Harris 4-term window — wide, minimal ringing.
    BlackmanHarris,
}

impl PixelFilter {
    /// Display name for UI.
    pub fn display_name(&self) -> &'static str {
        match self {
            PixelFilter::Box => "Box",
            PixelFilter::Gaussian => "Gaussian",
            PixelFilter::Mitchell => "Mitchell",
            PixelFilter::BlackmanHarris => "Blackman-Harris",
        }
    }

    /// All variants for UI dropdown.
    pub fn all() -> &'static [PixelFilter] {
        &[
            PixelFilter::Box,
            PixelFilter::Gaussian,
            PixelFilter::Mitchell,
            PixelFilter::BlackmanHarris,
        ]
    }

    /// Default radius for this filter type.
    pub fn default_radius(&self) -> f32 {
        match self {
            PixelFilter::Box => 0.5,
            PixelFilter::Gaussian => 1.5,
            PixelFilter::Mitchell => 2.0,
            PixelFilter::BlackmanHarris => 2.0,
        }
    }
}

/// Pixel filter configuration: filter type + radius.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PixelFilterConfig {
    /// Filter type.
    pub filter: PixelFilter,
    /// Filter radius in pixels.
    pub radius: f32,
}

impl Default for PixelFilterConfig {
    fn default() -> Self {
        Self {
            filter: PixelFilter::Box,
            radius: 0.5,
        }
    }
}

impl PixelFilterConfig {
    /// Create config with filter's default radius.
    pub fn new(filter: PixelFilter) -> Self {
        Self {
            filter,
            radius: filter.default_radius(),
        }
    }

    /// Whether this is the box filter (fast path: skip weight computation).
    pub fn is_box(&self) -> bool {
        self.filter == PixelFilter::Box
    }

    /// Evaluate filter weight for a sample at sub-pixel offset (dx, dy).
    ///
    /// Offsets are relative to pixel center, typically in [-0.5, 0.5] from
    /// the jitter sampling. Returns 0.0 for samples outside the filter radius.
    pub fn evaluate(&self, dx: f32, dy: f32) -> f32 {
        match self.filter {
            PixelFilter::Box => eval_box(dx, dy),
            PixelFilter::Gaussian => eval_gaussian(dx, dy, self.radius),
            PixelFilter::Mitchell => eval_mitchell(dx, dy, self.radius),
            PixelFilter::BlackmanHarris => eval_blackman_harris(dx, dy, self.radius),
        }
    }
}

/// Box filter: weight 1.0 if within half-pixel, else 0.0.
fn eval_box(dx: f32, dy: f32) -> f32 {
    if dx.abs() < 0.5 && dy.abs() < 0.5 {
        1.0
    } else {
        0.0
    }
}

/// Gaussian filter: exp(-alpha * r^2) - exp(-alpha * radius^2).
///
/// The subtraction ensures the filter goes to zero at the radius boundary.
fn eval_gaussian(dx: f32, dy: f32, radius: f32) -> f32 {
    let r2 = dx * dx + dy * dy;
    let radius2 = radius * radius;
    if r2 >= radius2 {
        return 0.0;
    }
    let alpha = 2.0; // Controls falloff steepness
    let g = (-alpha * r2).exp() - (-alpha * radius2).exp();
    g.max(0.0)
}

/// Mitchell-Netravali filter (B=1/3, C=1/3), separable.
///
/// Evaluated as product of 1D Mitchell in x and y, scaled by radius.
fn eval_mitchell(dx: f32, dy: f32, radius: f32) -> f32 {
    mitchell_1d(dx / radius * 2.0) * mitchell_1d(dy / radius * 2.0)
}

/// 1D Mitchell-Netravali with B=1/3, C=1/3.
///
/// Input x should be in [-2, 2] range (pre-scaled by caller).
fn mitchell_1d(x: f32) -> f32 {
    let x = x.abs();
    let b: f32 = 1.0 / 3.0;
    let c: f32 = 1.0 / 3.0;

    if x >= 2.0 {
        0.0
    } else if x >= 1.0 {
        let x2 = x * x;
        let x3 = x2 * x;
        ((-b - 6.0 * c) * x3
            + (6.0 * b + 30.0 * c) * x2
            + (-12.0 * b - 48.0 * c) * x
            + (8.0 * b + 24.0 * c))
            / 6.0
    } else {
        let x2 = x * x;
        let x3 = x2 * x;
        ((12.0 - 9.0 * b - 6.0 * c) * x3 + (-18.0 + 12.0 * b + 6.0 * c) * x2 + (6.0 - 2.0 * b))
            / 6.0
    }
}

/// Blackman-Harris 4-term window function, evaluated radially.
fn eval_blackman_harris(dx: f32, dy: f32, radius: f32) -> f32 {
    let r = (dx * dx + dy * dy).sqrt();
    if r >= radius {
        return 0.0;
    }
    blackman_harris_1d(r / radius)
}

/// 1D Blackman-Harris 4-term window. Input t in [0, 1] where 0=center, 1=edge.
///
/// Standard BH window peaks at the center of its support. We remap so
/// t=0 maps to the window center (peak) and t=1 maps to the edge (zero).
fn blackman_harris_1d(t: f32) -> f32 {
    let a0: f32 = 0.35875;
    let a1: f32 = 0.48829;
    let a2: f32 = 0.14128;
    let a3: f32 = 0.01168;
    // Map t=0 → center of window (x=pi), t=1 → edge (x=0 or 2*pi)
    // BH window: w(x) = a0 - a1*cos(x) + a2*cos(2x) - a3*cos(3x) for x in [0, 2*pi]
    // Peak at x=pi (center). We want t=0 → peak, t=1 → edge.
    let x = PI * (1.0 - t);
    a0 - a1 * x.cos() + a2 * (2.0 * x).cos() - a3 * (3.0 * x).cos()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_box_filter_uniform() {
        let cfg = PixelFilterConfig::default();
        assert_eq!(cfg.evaluate(0.0, 0.0), 1.0);
        assert_eq!(cfg.evaluate(0.3, 0.3), 1.0);
        assert_eq!(cfg.evaluate(-0.4, 0.4), 1.0);
        assert_eq!(cfg.evaluate(0.6, 0.0), 0.0);
    }

    #[test]
    fn test_gaussian_center_higher() {
        let cfg = PixelFilterConfig::new(PixelFilter::Gaussian);
        let center = cfg.evaluate(0.0, 0.0);
        let edge = cfg.evaluate(0.5, 0.5);
        assert!(center > edge, "center {center} should be > edge {edge}");
        assert!(center > 0.0);
        assert!(edge > 0.0);
    }

    #[test]
    fn test_gaussian_outside_zero() {
        let cfg = PixelFilterConfig::new(PixelFilter::Gaussian);
        assert_eq!(cfg.evaluate(2.0, 0.0), 0.0);
        assert_eq!(cfg.evaluate(0.0, 2.0), 0.0);
    }

    #[test]
    fn test_mitchell_center_positive() {
        let cfg = PixelFilterConfig::new(PixelFilter::Mitchell);
        let center = cfg.evaluate(0.0, 0.0);
        assert!(center > 0.0, "Mitchell center = {center}");
    }

    #[test]
    fn test_mitchell_symmetry() {
        let cfg = PixelFilterConfig::new(PixelFilter::Mitchell);
        let pos = cfg.evaluate(0.3, 0.0);
        let neg = cfg.evaluate(-0.3, 0.0);
        assert!(
            (pos - neg).abs() < 1e-6,
            "Mitchell should be symmetric: {pos} vs {neg}"
        );
    }

    #[test]
    fn test_blackman_harris_center() {
        let cfg = PixelFilterConfig::new(PixelFilter::BlackmanHarris);
        let center = cfg.evaluate(0.0, 0.0);
        let edge = cfg.evaluate(1.0, 0.0);
        assert!(center > edge, "BH center {center} should be > edge {edge}");
        assert!(center > 0.0);
    }

    #[test]
    fn test_blackman_harris_outside_zero() {
        let cfg = PixelFilterConfig::new(PixelFilter::BlackmanHarris);
        assert_eq!(cfg.evaluate(3.0, 0.0), 0.0);
    }

    #[test]
    fn test_filter_config_default_is_box() {
        let cfg = PixelFilterConfig::default();
        assert_eq!(cfg.filter, PixelFilter::Box);
        assert!(cfg.is_box());
    }

    #[test]
    fn test_box_filter_identical_to_uniform() {
        // Box filter at center and interior return 1.0 (uniform weight)
        let cfg = PixelFilterConfig::default();
        let offsets = [(0.0, 0.0), (0.1, -0.3), (-0.49, 0.49)];
        for (dx, dy) in offsets {
            assert_eq!(cfg.evaluate(dx, dy), 1.0, "box at ({dx}, {dy})");
        }
        // Half-open interval: boundary (0.5, 0.5) maps to 0.0
        assert_eq!(cfg.evaluate(0.5, 0.5), 0.0, "box at boundary (0.5, 0.5)");
    }
}
