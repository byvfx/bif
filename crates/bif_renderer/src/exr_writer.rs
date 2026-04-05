//! EXR file output for batch rendering.
//!
//! Writes multi-channel OpenEXR files with beauty, depth, and normal AOVs.

use crate::Color;
use exr::image::write::WritableImage;
use exr::image::{AnyChannel, AnyChannels, Blocks, Encoding, FlatSamples, Image, Layer};
use exr::math::Vec2;
use exr::meta::attribute::{Compression, LineOrder};
use exr::meta::header::LayerAttributes;
use exr::prelude::SmallVec;
use half::f16;
use std::path::Path;

/// EXR compression options.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
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
    /// World-space geometric normals AOV. None if disabled.
    pub normal: Option<Vec<[f32; 3]>>,
    /// World-space shading normals (after normal map). None if disabled.
    pub shading_normal: Option<Vec<[f32; 3]>>,
    /// Albedo AOV (denoiser guide, not written to EXR). None if disabled.
    pub albedo: Option<Vec<[f32; 3]>>,
}

/// Error type for EXR operations.
#[derive(Debug, thiserror::Error)]
pub enum ExrError {
    /// IO error during file write.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    /// EXR library error.
    #[error("EXR error: {0}")]
    Exr(String),
    /// Invalid dimensions.
    #[error("Invalid dimensions: {width}x{height}")]
    InvalidDimensions { width: u32, height: u32 },
    /// Buffer size mismatch.
    #[error("Buffer size mismatch: expected {expected}, got {actual}")]
    BufferSizeMismatch { expected: usize, actual: usize },
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

    // Build channel buffers dynamically based on which AOVs are present
    let beauty = &output.beauty;
    let mut channels: SmallVec<[AnyChannel<FlatSamples>; 4]> = SmallVec::new();

    // Beauty (RGB) — half float
    let r: Vec<f16> = beauty.iter().map(|c| f16::from_f32(c.x)).collect();
    let g: Vec<f16> = beauty.iter().map(|c| f16::from_f32(c.y)).collect();
    let b: Vec<f16> = beauty.iter().map(|c| f16::from_f32(c.z)).collect();
    channels.push(make_f16_channel("R", r));
    channels.push(make_f16_channel("G", g));
    channels.push(make_f16_channel("B", b));

    // Alpha — half float
    if let Some(ref alpha) = output.alpha {
        let a: Vec<f16> = alpha.iter().map(|v| f16::from_f32(*v)).collect();
        channels.push(make_f16_channel("A", a));
    }

    // Geometric normal — half float
    if let Some(ref normal) = output.normal {
        let nx: Vec<f16> = normal.iter().map(|n| f16::from_f32(n[0])).collect();
        let ny: Vec<f16> = normal.iter().map(|n| f16::from_f32(n[1])).collect();
        let nz: Vec<f16> = normal.iter().map(|n| f16::from_f32(n[2])).collect();
        channels.push(make_f16_channel("N.X", nx));
        channels.push(make_f16_channel("N.Y", ny));
        channels.push(make_f16_channel("N.Z", nz));
    }

    // Shading normal (after normal map) — half float
    if let Some(ref sn) = output.shading_normal {
        let nx: Vec<f16> = sn.iter().map(|n| f16::from_f32(n[0])).collect();
        let ny: Vec<f16> = sn.iter().map(|n| f16::from_f32(n[1])).collect();
        let nz: Vec<f16> = sn.iter().map(|n| f16::from_f32(n[2])).collect();
        channels.push(make_f16_channel("Ns.X", nx));
        channels.push(make_f16_channel("Ns.Y", ny));
        channels.push(make_f16_channel("Ns.Z", nz));
    }

    // Depth — full f32 (not converted to half)
    if let Some(ref depth) = output.depth {
        channels.push(make_f32_channel("Z", depth.clone()));
    }

    let any_channels = AnyChannels::sort(channels);

    // TODO: Add chromaticities (Rec.709/sRGB) and render engine metadata
    // when the exr crate exposes attribute APIs for these fields.
    // Production EXR files need this for correct color display in Nuke/RV.
    let layer = Layer::new(
        (w, h),
        LayerAttributes::named("main"),
        make_encoding(compression),
        any_channels,
    );

    let image = Image::from_layer(layer);
    image
        .write()
        .to_file(path)
        .map_err(|e| ExrError::Exr(format!("{}", e)))?;

    Ok(())
}

/// Create a half-float (f16) channel.
fn make_f16_channel(name: &str, data: Vec<f16>) -> AnyChannel<FlatSamples> {
    AnyChannel {
        name: exr::meta::attribute::Text::from(name),
        sample_data: FlatSamples::F16(data),
        quantize_linearly: false,
        sampling: Vec2(1, 1),
    }
}

/// Create a full-float (f32) channel.
fn make_f32_channel(name: &str, data: Vec<f32>) -> AnyChannel<FlatSamples> {
    AnyChannel {
        name: exr::meta::attribute::Text::from(name),
        sample_data: FlatSamples::F32(data),
        quantize_linearly: false,
        sampling: Vec2(1, 1),
    }
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
        let prefix = if frame < 0 { "neg" } else { "" };
        let padded = format!("{prefix}{:0>width$}", frame.abs(), width = max_hashes);
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
            shading_normal: None,
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
            shading_normal: None,
            albedo: None,
        };

        let result = write_exr(&output, Path::new("test.exr"), ExrCompression::Zip);
        assert!(matches!(result, Err(ExrError::BufferSizeMismatch { .. })));
    }

    #[test]
    fn test_format_frame_path_negative_frame() {
        assert_eq!(
            format_frame_path("render.####.exr", -1),
            "render.neg0001.exr"
        );
        assert_eq!(
            format_frame_path("render.####.exr", -42),
            "render.neg0042.exr"
        );
        // Positive frames unchanged
        assert_eq!(format_frame_path("render.####.exr", 1), "render.0001.exr");
    }
}
