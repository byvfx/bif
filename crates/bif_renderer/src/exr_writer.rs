//! EXR file output for batch rendering.
//!
//! Writes multi-channel OpenEXR files with beauty, depth, and normal AOVs.

use crate::Color;
use exr::image::write::WritableImage;
use exr::image::{Blocks, Encoding, Image, Layer, SpecificChannels};
use exr::math::Vec2;
use exr::meta::attribute::{Compression, LineOrder};
use exr::meta::header::LayerAttributes;
use half::f16;
use std::path::Path;

/// EXR compression options.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExrCompression {
    /// No compression (fastest write, largest files)
    None,
    /// RLE compression (fast, good for flat areas)
    Rle,
    /// ZIP compression per scanline (good balance)
    #[default]
    Zip,
    /// ZIP compression per tile (better for random access)
    Zips,
    /// PIZ wavelet compression (best for grainy images)
    Piz,
}

impl ExrCompression {
    /// Convert to exr crate compression type.
    pub fn to_exr(&self) -> Compression {
        match self {
            ExrCompression::None => Compression::Uncompressed,
            ExrCompression::Rle => Compression::RLE,
            ExrCompression::Zip => Compression::ZIP16,
            ExrCompression::Zips => Compression::ZIP1,
            ExrCompression::Piz => Compression::PIZ,
        }
    }

    /// Display name for UI.
    pub fn display_name(&self) -> &'static str {
        match self {
            ExrCompression::None => "None",
            ExrCompression::Rle => "RLE",
            ExrCompression::Zip => "ZIP",
            ExrCompression::Zips => "ZIPS",
            ExrCompression::Piz => "PIZ",
        }
    }

    /// All compression options for UI dropdown.
    pub fn all() -> &'static [ExrCompression] {
        &[
            ExrCompression::Zip,
            ExrCompression::Piz,
            ExrCompression::Zips,
            ExrCompression::Rle,
            ExrCompression::None,
        ]
    }
}

/// Render output with optional AOVs.
pub struct ExrOutput {
    /// Image width in pixels.
    pub width: u32,
    /// Image height in pixels.
    pub height: u32,
    /// Beauty pass (RGB linear f32).
    pub beauty: Vec<Color>,
    /// Alpha channel (1.0 = hit, 0.0 = miss). None if disabled.
    pub alpha: Option<Vec<f32>>,
    /// Depth AOV (Z buffer, world units). None if disabled.
    pub depth: Option<Vec<f32>>,
    /// World-space normals AOV. None if disabled.
    pub normal: Option<Vec<[f32; 3]>>,
    /// Albedo AOV (denoiser guide, not written to EXR). None if disabled.
    pub albedo: Option<Vec<[f32; 3]>>,
}

/// Error type for EXR operations.
#[derive(Debug)]
pub enum ExrError {
    /// IO error during file write.
    Io(std::io::Error),
    /// EXR library error.
    Exr(String),
    /// Invalid dimensions.
    InvalidDimensions { width: u32, height: u32 },
    /// Buffer size mismatch.
    BufferSizeMismatch { expected: usize, actual: usize },
}

impl std::fmt::Display for ExrError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExrError::Io(e) => write!(f, "IO error: {}", e),
            ExrError::Exr(e) => write!(f, "EXR error: {}", e),
            ExrError::InvalidDimensions { width, height } => {
                write!(f, "Invalid dimensions: {}x{}", width, height)
            }
            ExrError::BufferSizeMismatch { expected, actual } => {
                write!(
                    f,
                    "Buffer size mismatch: expected {}, got {}",
                    expected, actual
                )
            }
        }
    }
}

impl std::error::Error for ExrError {}

impl From<std::io::Error> for ExrError {
    fn from(e: std::io::Error) -> Self {
        ExrError::Io(e)
    }
}

/// Create encoding with specified compression.
fn make_encoding(compression: ExrCompression) -> Encoding {
    Encoding {
        compression: compression.to_exr(),
        blocks: Blocks::ScanLines,
        line_order: LineOrder::Increasing,
    }
}

/// Write an EXR file with beauty and optional AOVs.
///
/// Channels:
/// - `R`, `G`, `B`: Beauty pass (half float)
/// - `A`: Alpha (half float, only if alpha provided)
/// - `Z`: Depth (full float, only if depth provided)
/// - `N.X`, `N.Y`, `N.Z`: World normals (half float, only if normal provided)
pub fn write_exr(
    output: &ExrOutput,
    path: &Path,
    compression: ExrCompression,
) -> Result<(), ExrError> {
    let w = output.width as usize;
    let h = output.height as usize;
    let expected_pixels = w * h;

    // Validate dimensions
    if output.width == 0 || output.height == 0 {
        return Err(ExrError::InvalidDimensions {
            width: output.width,
            height: output.height,
        });
    }

    // Validate buffer sizes
    if output.beauty.len() != expected_pixels {
        return Err(ExrError::BufferSizeMismatch {
            expected: expected_pixels,
            actual: output.beauty.len(),
        });
    }

    if let Some(ref alpha) = output.alpha {
        if alpha.len() != expected_pixels {
            return Err(ExrError::BufferSizeMismatch {
                expected: expected_pixels,
                actual: alpha.len(),
            });
        }
    }

    if let Some(ref depth) = output.depth {
        if depth.len() != expected_pixels {
            return Err(ExrError::BufferSizeMismatch {
                expected: expected_pixels,
                actual: depth.len(),
            });
        }
    }

    if let Some(ref normal) = output.normal {
        if normal.len() != expected_pixels {
            return Err(ExrError::BufferSizeMismatch {
                expected: expected_pixels,
                actual: normal.len(),
            });
        }
    }

    // Write based on AOV configuration (alpha, depth, normal)
    match (&output.alpha, &output.depth, &output.normal) {
        (Some(a), Some(d), Some(n)) => {
            write_exr_rgba_depth_normal(output, a, d, n, path, compression)
        }
        (Some(a), Some(d), None) => write_exr_rgba_depth(output, a, d, path, compression),
        (Some(a), None, Some(n)) => write_exr_rgba_normal(output, a, n, path, compression),
        (Some(a), None, None) => write_exr_rgba(output, a, path, compression),
        (None, Some(d), Some(n)) => write_exr_rgb_depth_normal(output, d, n, path, compression),
        (None, Some(d), None) => write_exr_rgb_depth(output, d, path, compression),
        (None, None, Some(n)) => write_exr_rgb_normal(output, n, path, compression),
        (None, None, None) => write_exr_rgb(output, path, compression),
    }
}

/// Write EXR with RGB only.
fn write_exr_rgb(
    output: &ExrOutput,
    path: &Path,
    compression: ExrCompression,
) -> Result<(), ExrError> {
    let w = output.width as usize;
    let h = output.height as usize;
    let beauty = &output.beauty;

    let channels = SpecificChannels::build()
        .with_channel("R")
        .with_channel("G")
        .with_channel("B")
        .with_pixel_fn(|pos: Vec2<usize>| {
            let idx = pos.y() * w + pos.x();
            let c = beauty[idx];
            (f16::from_f32(c.x), f16::from_f32(c.y), f16::from_f32(c.z))
        });

    let layer = Layer::new(
        (w, h),
        LayerAttributes::named("main"),
        make_encoding(compression),
        channels,
    );

    let image = Image::from_layer(layer);
    image
        .write()
        .to_file(path)
        .map_err(|e| ExrError::Exr(format!("{}", e)))?;

    Ok(())
}

/// Write EXR with RGBA.
fn write_exr_rgba(
    output: &ExrOutput,
    alpha: &[f32],
    path: &Path,
    compression: ExrCompression,
) -> Result<(), ExrError> {
    let w = output.width as usize;
    let h = output.height as usize;
    let beauty = &output.beauty;

    let channels = SpecificChannels::build()
        .with_channel("R")
        .with_channel("G")
        .with_channel("B")
        .with_channel("A")
        .with_pixel_fn(|pos: Vec2<usize>| {
            let idx = pos.y() * w + pos.x();
            let c = beauty[idx];
            let a = alpha[idx];
            (
                f16::from_f32(c.x),
                f16::from_f32(c.y),
                f16::from_f32(c.z),
                f16::from_f32(a),
            )
        });

    let layer = Layer::new(
        (w, h),
        LayerAttributes::named("main"),
        make_encoding(compression),
        channels,
    );

    let image = Image::from_layer(layer);
    image
        .write()
        .to_file(path)
        .map_err(|e| ExrError::Exr(format!("{}", e)))?;

    Ok(())
}

/// Write EXR with RGB + depth (Z).
fn write_exr_rgb_depth(
    output: &ExrOutput,
    depth: &[f32],
    path: &Path,
    compression: ExrCompression,
) -> Result<(), ExrError> {
    let w = output.width as usize;
    let h = output.height as usize;
    let beauty = &output.beauty;

    let channels = SpecificChannels::build()
        .with_channel("R")
        .with_channel("G")
        .with_channel("B")
        .with_channel("Z")
        .with_pixel_fn(|pos: Vec2<usize>| {
            let idx = pos.y() * w + pos.x();
            let c = beauty[idx];
            let z = depth[idx];
            (
                f16::from_f32(c.x),
                f16::from_f32(c.y),
                f16::from_f32(c.z),
                z,
            )
        });

    let layer = Layer::new(
        (w, h),
        LayerAttributes::named("main"),
        make_encoding(compression),
        channels,
    );

    let image = Image::from_layer(layer);
    image
        .write()
        .to_file(path)
        .map_err(|e| ExrError::Exr(format!("{}", e)))?;

    Ok(())
}

/// Write EXR with RGBA + depth (Z).
fn write_exr_rgba_depth(
    output: &ExrOutput,
    alpha: &[f32],
    depth: &[f32],
    path: &Path,
    compression: ExrCompression,
) -> Result<(), ExrError> {
    let w = output.width as usize;
    let h = output.height as usize;
    let beauty = &output.beauty;

    let channels = SpecificChannels::build()
        .with_channel("R")
        .with_channel("G")
        .with_channel("B")
        .with_channel("A")
        .with_channel("Z")
        .with_pixel_fn(|pos: Vec2<usize>| {
            let idx = pos.y() * w + pos.x();
            let c = beauty[idx];
            let a = alpha[idx];
            let z = depth[idx];
            (
                f16::from_f32(c.x),
                f16::from_f32(c.y),
                f16::from_f32(c.z),
                f16::from_f32(a),
                z,
            )
        });

    let layer = Layer::new(
        (w, h),
        LayerAttributes::named("main"),
        make_encoding(compression),
        channels,
    );

    let image = Image::from_layer(layer);
    image
        .write()
        .to_file(path)
        .map_err(|e| ExrError::Exr(format!("{}", e)))?;

    Ok(())
}

/// Write EXR with RGB + normal (N.X, N.Y, N.Z).
fn write_exr_rgb_normal(
    output: &ExrOutput,
    normal: &[[f32; 3]],
    path: &Path,
    compression: ExrCompression,
) -> Result<(), ExrError> {
    let w = output.width as usize;
    let h = output.height as usize;
    let beauty = &output.beauty;

    let channels = SpecificChannels::build()
        .with_channel("R")
        .with_channel("G")
        .with_channel("B")
        .with_channel("N.X")
        .with_channel("N.Y")
        .with_channel("N.Z")
        .with_pixel_fn(|pos: Vec2<usize>| {
            let idx = pos.y() * w + pos.x();
            let c = beauty[idx];
            let n = normal[idx];
            (
                f16::from_f32(c.x),
                f16::from_f32(c.y),
                f16::from_f32(c.z),
                f16::from_f32(n[0]),
                f16::from_f32(n[1]),
                f16::from_f32(n[2]),
            )
        });

    let layer = Layer::new(
        (w, h),
        LayerAttributes::named("main"),
        make_encoding(compression),
        channels,
    );

    let image = Image::from_layer(layer);
    image
        .write()
        .to_file(path)
        .map_err(|e| ExrError::Exr(format!("{}", e)))?;

    Ok(())
}

/// Write EXR with RGBA + normal (N.X, N.Y, N.Z).
fn write_exr_rgba_normal(
    output: &ExrOutput,
    alpha: &[f32],
    normal: &[[f32; 3]],
    path: &Path,
    compression: ExrCompression,
) -> Result<(), ExrError> {
    let w = output.width as usize;
    let h = output.height as usize;
    let beauty = &output.beauty;

    let channels = SpecificChannels::build()
        .with_channel("R")
        .with_channel("G")
        .with_channel("B")
        .with_channel("A")
        .with_channel("N.X")
        .with_channel("N.Y")
        .with_channel("N.Z")
        .with_pixel_fn(|pos: Vec2<usize>| {
            let idx = pos.y() * w + pos.x();
            let c = beauty[idx];
            let a = alpha[idx];
            let n = normal[idx];
            (
                f16::from_f32(c.x),
                f16::from_f32(c.y),
                f16::from_f32(c.z),
                f16::from_f32(a),
                f16::from_f32(n[0]),
                f16::from_f32(n[1]),
                f16::from_f32(n[2]),
            )
        });

    let layer = Layer::new(
        (w, h),
        LayerAttributes::named("main"),
        make_encoding(compression),
        channels,
    );

    let image = Image::from_layer(layer);
    image
        .write()
        .to_file(path)
        .map_err(|e| ExrError::Exr(format!("{}", e)))?;

    Ok(())
}

/// Write EXR with RGB + depth (Z) + normal (N.X, N.Y, N.Z).
fn write_exr_rgb_depth_normal(
    output: &ExrOutput,
    depth: &[f32],
    normal: &[[f32; 3]],
    path: &Path,
    compression: ExrCompression,
) -> Result<(), ExrError> {
    let w = output.width as usize;
    let h = output.height as usize;
    let beauty = &output.beauty;

    let channels = SpecificChannels::build()
        .with_channel("R")
        .with_channel("G")
        .with_channel("B")
        .with_channel("Z")
        .with_channel("N.X")
        .with_channel("N.Y")
        .with_channel("N.Z")
        .with_pixel_fn(|pos: Vec2<usize>| {
            let idx = pos.y() * w + pos.x();
            let c = beauty[idx];
            let z = depth[idx];
            let n = normal[idx];
            (
                f16::from_f32(c.x),
                f16::from_f32(c.y),
                f16::from_f32(c.z),
                z,
                f16::from_f32(n[0]),
                f16::from_f32(n[1]),
                f16::from_f32(n[2]),
            )
        });

    let layer = Layer::new(
        (w, h),
        LayerAttributes::named("main"),
        make_encoding(compression),
        channels,
    );

    let image = Image::from_layer(layer);
    image
        .write()
        .to_file(path)
        .map_err(|e| ExrError::Exr(format!("{}", e)))?;

    Ok(())
}

/// Write EXR with RGBA + depth (Z) + normal (N.X, N.Y, N.Z).
fn write_exr_rgba_depth_normal(
    output: &ExrOutput,
    alpha: &[f32],
    depth: &[f32],
    normal: &[[f32; 3]],
    path: &Path,
    compression: ExrCompression,
) -> Result<(), ExrError> {
    let w = output.width as usize;
    let h = output.height as usize;
    let beauty = &output.beauty;

    let channels = SpecificChannels::build()
        .with_channel("R")
        .with_channel("G")
        .with_channel("B")
        .with_channel("A")
        .with_channel("Z")
        .with_channel("N.X")
        .with_channel("N.Y")
        .with_channel("N.Z")
        .with_pixel_fn(|pos: Vec2<usize>| {
            let idx = pos.y() * w + pos.x();
            let c = beauty[idx];
            let a = alpha[idx];
            let z = depth[idx];
            let n = normal[idx];
            (
                f16::from_f32(c.x),
                f16::from_f32(c.y),
                f16::from_f32(c.z),
                f16::from_f32(a),
                z,
                f16::from_f32(n[0]),
                f16::from_f32(n[1]),
                f16::from_f32(n[2]),
            )
        });

    let layer = Layer::new(
        (w, h),
        LayerAttributes::named("main"),
        make_encoding(compression),
        channels,
    );

    let image = Image::from_layer(layer);
    image
        .write()
        .to_file(path)
        .map_err(|e| ExrError::Exr(format!("{}", e)))?;

    Ok(())
}

/// Format a frame path with frame number substitution.
///
/// Replaces `####` patterns with zero-padded frame number.
/// The number of `#` characters determines padding width.
///
/// # Examples
/// - `"render.####.exr"` + frame 42 → `"render.0042.exr"`
/// - `"frame_##.exr"` + frame 7 → `"frame_07.exr"`
pub fn format_frame_path(pattern: &str, frame: i32) -> String {
    let mut result = pattern.to_string();

    // Find longest sequence of '#' characters
    let mut max_hashes = 0;
    let mut current_hashes = 0;

    for c in pattern.chars() {
        if c == '#' {
            current_hashes += 1;
            max_hashes = max_hashes.max(current_hashes);
        } else {
            current_hashes = 0;
        }
    }

    if max_hashes > 0 {
        let hash_pattern = "#".repeat(max_hashes);
        let padded = format!("{:0>width$}", frame.abs(), width = max_hashes);
        result = result.replace(&hash_pattern, &padded);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_frame_path_four_digits() {
        assert_eq!(format_frame_path("render.####.exr", 1), "render.0001.exr");
        assert_eq!(format_frame_path("render.####.exr", 42), "render.0042.exr");
        assert_eq!(
            format_frame_path("render.####.exr", 1234),
            "render.1234.exr"
        );
        assert_eq!(
            format_frame_path("render.####.exr", 12345),
            "render.12345.exr"
        );
    }

    #[test]
    fn test_format_frame_path_two_digits() {
        assert_eq!(format_frame_path("frame_##.exr", 7), "frame_07.exr");
        assert_eq!(format_frame_path("frame_##.exr", 99), "frame_99.exr");
    }

    #[test]
    fn test_format_frame_path_no_pattern() {
        assert_eq!(format_frame_path("static.exr", 5), "static.exr");
    }

    #[test]
    fn test_exr_compression_display() {
        assert_eq!(ExrCompression::Zip.display_name(), "ZIP");
        assert_eq!(ExrCompression::Piz.display_name(), "PIZ");
        assert_eq!(ExrCompression::None.display_name(), "None");
    }

    #[test]
    fn test_exr_output_validation() {
        let output = ExrOutput {
            width: 0,
            height: 100,
            beauty: vec![],
            alpha: None,
            depth: None,
            normal: None,
            albedo: None,
        };

        let result = write_exr(&output, Path::new("test.exr"), ExrCompression::Zip);
        assert!(matches!(result, Err(ExrError::InvalidDimensions { .. })));
    }

    #[test]
    fn test_exr_buffer_mismatch() {
        let output = ExrOutput {
            width: 10,
            height: 10,
            beauty: vec![Color::ZERO; 50], // Wrong size
            alpha: None,
            depth: None,
            normal: None,
            albedo: None,
        };

        let result = write_exr(&output, Path::new("test.exr"), ExrCompression::Zip);
        assert!(matches!(result, Err(ExrError::BufferSizeMismatch { .. })));
    }
}
