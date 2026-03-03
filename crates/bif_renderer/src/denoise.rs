//! Intel OIDN denoising wrapper.
//!
//! Feature-gated behind `oidn`. When disabled, `denoise_beauty` returns an error.
//! Accepts beauty (required) + optional albedo/normal guide buffers.

use crate::Color;

/// Result of a successful denoise operation.
pub struct DenoiseResult {
    /// Denoised beauty buffer (same dimensions as input).
    pub beauty: Vec<Color>,
}

/// Errors that can occur during denoising.
#[derive(Debug)]
pub enum DenoiseError {
    /// OIDN feature not compiled in.
    NotEnabled,
    /// Buffer dimension mismatch.
    DimensionMismatch {
        expected: usize,
        actual: usize,
        buffer: &'static str,
    },
    /// OIDN filter execution error.
    FilterError(String),
}

impl std::fmt::Display for DenoiseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DenoiseError::NotEnabled => write!(f, "OIDN feature not enabled"),
            DenoiseError::DimensionMismatch {
                expected,
                actual,
                buffer,
            } => write!(
                f,
                "{} buffer size mismatch: expected {}, got {}",
                buffer, expected, actual
            ),
            DenoiseError::FilterError(msg) => write!(f, "OIDN filter error: {}", msg),
        }
    }
}

impl std::error::Error for DenoiseError {}

/// Denoise a beauty image using Intel OIDN.
///
/// - `width`, `height`: image dimensions
/// - `beauty`: input beauty buffer (linear HDR, one `Color` per pixel)
/// - `albedo`: optional albedo guide (improves texture detail preservation)
/// - `normal`: optional normal guide (improves edge preservation)
///
/// Returns denoised beauty buffer on success.
pub fn denoise_beauty(
    width: usize,
    height: usize,
    beauty: &[Color],
    albedo: Option<&[[f32; 3]]>,
    normal: Option<&[[f32; 3]]>,
) -> Result<DenoiseResult, DenoiseError> {
    let pixel_count = width * height;

    // Validate buffer sizes
    if beauty.len() != pixel_count {
        return Err(DenoiseError::DimensionMismatch {
            expected: pixel_count,
            actual: beauty.len(),
            buffer: "beauty",
        });
    }
    if let Some(a) = albedo {
        if a.len() != pixel_count {
            return Err(DenoiseError::DimensionMismatch {
                expected: pixel_count,
                actual: a.len(),
                buffer: "albedo",
            });
        }
    }
    if let Some(n) = normal {
        if n.len() != pixel_count {
            return Err(DenoiseError::DimensionMismatch {
                expected: pixel_count,
                actual: n.len(),
                buffer: "normal",
            });
        }
    }

    denoise_impl(width, height, beauty, albedo, normal)
}

#[cfg(feature = "oidn")]
fn denoise_impl(
    width: usize,
    height: usize,
    beauty: &[Color],
    albedo: Option<&[[f32; 3]]>,
    normal: Option<&[[f32; 3]]>,
) -> Result<DenoiseResult, DenoiseError> {
    let pixel_count = width * height;

    // Convert Color (Vec3) to flat f32 array for OIDN (RGB interleaved)
    let mut input: Vec<f32> = Vec::with_capacity(pixel_count * 3);
    for c in beauty {
        input.push(c.x);
        input.push(c.y);
        input.push(c.z);
    }

    let mut output = vec![0.0f32; pixel_count * 3];

    // Flatten albedo/normal guide buffers
    let albedo_flat: Option<Vec<f32>> = albedo.map(|a| {
        let mut flat = Vec::with_capacity(pixel_count * 3);
        for px in a {
            flat.extend_from_slice(px);
        }
        flat
    });

    let normal_flat: Option<Vec<f32>> = normal.map(|n| {
        let mut flat = Vec::with_capacity(pixel_count * 3);
        for px in n {
            flat.extend_from_slice(px);
        }
        flat
    });

    let device = oidn::Device::new();

    let mut filter = oidn::RayTracing::new(&device);
    filter.hdr(true).image_dimensions(width, height);

    // Attach guide buffers
    match (&albedo_flat, &normal_flat) {
        (Some(a), Some(n)) => {
            filter.albedo_normal(a, n);
        }
        (Some(a), None) => {
            filter.albedo(a);
        }
        _ => {}
    }

    filter
        .filter(&input, &mut output)
        .map_err(|e| DenoiseError::FilterError(format!("{:?}", e)))?;

    // Convert back to Vec<Color>
    let result: Vec<Color> = output
        .chunks_exact(3)
        .map(|c| Color::new(c[0], c[1], c[2]))
        .collect();

    log::info!(
        "OIDN denoise complete: {}x{} ({} pixels)",
        width,
        height,
        pixel_count
    );

    Ok(DenoiseResult { beauty: result })
}

#[cfg(not(feature = "oidn"))]
fn denoise_impl(
    _width: usize,
    _height: usize,
    _beauty: &[Color],
    _albedo: Option<&[[f32; 3]]>,
    _normal: Option<&[[f32; 3]]>,
) -> Result<DenoiseResult, DenoiseError> {
    Err(DenoiseError::NotEnabled)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_denoise_dimension_mismatch_beauty() {
        let beauty = vec![Color::ZERO; 10]; // Wrong size for 4x4
        let result = denoise_beauty(4, 4, &beauty, None, None);
        assert!(matches!(
            result,
            Err(DenoiseError::DimensionMismatch {
                buffer: "beauty",
                ..
            })
        ));
    }

    #[test]
    fn test_denoise_dimension_mismatch_albedo() {
        let beauty = vec![Color::ZERO; 16];
        let albedo = vec![[0.0; 3]; 10]; // Wrong size
        let result = denoise_beauty(4, 4, &beauty, Some(&albedo), None);
        assert!(matches!(
            result,
            Err(DenoiseError::DimensionMismatch {
                buffer: "albedo",
                ..
            })
        ));
    }

    #[test]
    fn test_denoise_dimension_mismatch_normal() {
        let beauty = vec![Color::ZERO; 16];
        let normal = vec![[0.0; 3]; 10]; // Wrong size
        let result = denoise_beauty(4, 4, &beauty, None, Some(&normal));
        assert!(matches!(
            result,
            Err(DenoiseError::DimensionMismatch {
                buffer: "normal",
                ..
            })
        ));
    }

    #[cfg(not(feature = "oidn"))]
    #[test]
    fn test_denoise_disabled_returns_error() {
        let beauty = vec![Color::ONE; 16];
        let result = denoise_beauty(4, 4, &beauty, None, None);
        assert!(matches!(result, Err(DenoiseError::NotEnabled)));
    }

    #[cfg(feature = "oidn")]
    #[test]
    fn test_denoise_beauty_only() {
        let beauty = vec![Color::new(0.5, 0.3, 0.1); 16];
        let result = denoise_beauty(4, 4, &beauty, None, None);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().beauty.len(), 16);
    }

    #[cfg(feature = "oidn")]
    #[test]
    fn test_denoise_with_albedo_normal() {
        let beauty = vec![Color::new(0.5, 0.3, 0.1); 16];
        let albedo = vec![[0.8, 0.6, 0.4]; 16];
        let normal = vec![[0.0, 1.0, 0.0]; 16];
        let result = denoise_beauty(4, 4, &beauty, Some(&albedo), Some(&normal));
        assert!(result.is_ok());
        assert_eq!(result.unwrap().beauty.len(), 16);
    }
}
