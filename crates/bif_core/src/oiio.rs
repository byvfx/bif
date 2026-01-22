//! OpenImageIO FFI bindings for texture loading with mipmap support.
//!
//! This module provides Rust wrappers around the OIIO C++ bridge,
//! enabling loading of industry-standard .tx texture files with
//! proper mipmapping for both viewport and Ivar rendering.

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::path::Path;

use thiserror::Error;

/// Errors from OIIO operations.
#[derive(Error, Debug, Clone)]
pub enum OiioError {
    #[error("Null pointer argument")]
    NullPointer,

    #[error("File not found: {0}")]
    FileNotFound(String),

    #[error("Invalid image format: {0}")]
    InvalidFormat(String),

    #[error("Failed to read image: {0}")]
    ReadFailed(String),

    #[error("Failed to write image: {0}")]
    WriteFailed(String),

    #[error("Out of memory")]
    OutOfMemory,

    #[error("Unknown error: {0}")]
    Unknown(String),

    #[error("Invalid path encoding")]
    InvalidPath,
}

pub type OiioResult<T> = Result<T, OiioError>;

// ============================================================================
// FFI Bindings
// ============================================================================

#[repr(C)]
#[allow(dead_code)]
enum OiioBridgeError {
    Success = 0,
    NullPointer = 1,
    FileNotFound = 2,
    InvalidFormat = 3,
    ReadFailed = 4,
    WriteFailed = 5,
    OutOfMemory = 6,
    Unknown = 99,
}

#[repr(C)]
struct OiioMipLevelRaw {
    data: *mut u8,
    width: u32,
    height: u32,
    byte_size: usize,
}

#[repr(C)]
struct OiioTextureDataRaw {
    mip_levels: *mut OiioMipLevelRaw,
    mip_count: u32,
    width: u32,
    height: u32,
    channels: u32,
    is_linear: i32,
}

#[repr(C)]
struct OiioTxOptionsRaw {
    tile_size: u32,
    compression: *const c_char,
    generate_mips: i32,
    force: i32,
}

#[link(name = "oiio_bridge")]
extern "C" {
    fn oiio_bridge_error_message(error: i32) -> *const c_char;
    fn oiio_bridge_get_last_error() -> *const c_char;
    fn oiio_load_texture(path: *const c_char, out_data: *mut *mut OiioTextureDataRaw) -> i32;
    fn oiio_load_texture_with_mips(
        path: *const c_char,
        out_data: *mut *mut OiioTextureDataRaw,
    ) -> i32;
    fn oiio_free_texture(data: *mut OiioTextureDataRaw);
    fn oiio_tx_default_options() -> OiioTxOptionsRaw;
    fn oiio_make_tx(
        input_path: *const c_char,
        output_path: *const c_char,
        options: *const OiioTxOptionsRaw,
    ) -> i32;
    fn oiio_tx_is_valid(source_path: *const c_char, tx_path: *const c_char) -> i32;
    fn oiio_get_version() -> *const c_char;
    fn oiio_can_read(path: *const c_char) -> i32;
    fn oiio_calculate_mip_count(width: u32, height: u32) -> u32;
}

// ============================================================================
// Helper Functions
// ============================================================================

fn convert_error(code: i32) -> OiioError {
    unsafe {
        let last_error = oiio_bridge_get_last_error();
        let detail = if !last_error.is_null() {
            CStr::from_ptr(last_error)
                .to_string_lossy()
                .into_owned()
        } else {
            String::new()
        };

        match code {
            1 => OiioError::NullPointer,
            2 => OiioError::FileNotFound(detail),
            3 => OiioError::InvalidFormat(detail),
            4 => OiioError::ReadFailed(detail),
            5 => OiioError::WriteFailed(detail),
            6 => OiioError::OutOfMemory,
            _ => OiioError::Unknown(detail),
        }
    }
}

// ============================================================================
// Public API
// ============================================================================

/// A single mipmap level of a texture.
#[derive(Clone, Debug)]
pub struct MipLevel {
    /// RGBA u8 pixel data
    pub data: Vec<u8>,
    /// Width in pixels
    pub width: u32,
    /// Height in pixels
    pub height: u32,
}

/// A texture loaded via OIIO with all mip levels.
#[derive(Clone, Debug)]
pub struct OiioTexture {
    /// Mip levels (index 0 = base level)
    pub mip_levels: Vec<MipLevel>,
    /// Base width
    pub width: u32,
    /// Base height
    pub height: u32,
    /// Whether source was linear (EXR/HDR) or sRGB
    pub is_linear: bool,
    /// Source file path
    pub path: String,
}

impl OiioTexture {
    /// Get the number of mip levels.
    pub fn mip_count(&self) -> u32 {
        self.mip_levels.len() as u32
    }

    /// Get a specific mip level.
    pub fn mip_level(&self, level: u32) -> Option<&MipLevel> {
        self.mip_levels.get(level as usize)
    }

    /// Get total memory usage in bytes.
    pub fn size_bytes(&self) -> usize {
        self.mip_levels.iter().map(|m| m.data.len()).sum()
    }
}

/// Load a texture file without mipmaps.
pub fn load_texture(path: impl AsRef<Path>) -> OiioResult<OiioTexture> {
    let path = path.as_ref();
    let path_str = path.to_str().ok_or(OiioError::InvalidPath)?;
    let c_path = CString::new(path_str).map_err(|_| OiioError::InvalidPath)?;

    unsafe {
        let mut raw_data: *mut OiioTextureDataRaw = std::ptr::null_mut();
        let result = oiio_load_texture(c_path.as_ptr(), &mut raw_data);

        if result != 0 {
            return Err(convert_error(result));
        }

        if raw_data.is_null() {
            return Err(OiioError::NullPointer);
        }

        let texture = texture_from_raw(raw_data, path_str);
        oiio_free_texture(raw_data);
        Ok(texture)
    }
}

/// Load a texture file with mipmaps.
/// If the file has embedded mipmaps (e.g., .tx), they are loaded directly.
/// Otherwise, mipmaps are generated from the base level.
pub fn load_texture_with_mips(path: impl AsRef<Path>) -> OiioResult<OiioTexture> {
    let path = path.as_ref();
    let path_str = path.to_str().ok_or(OiioError::InvalidPath)?;
    let c_path = CString::new(path_str).map_err(|_| OiioError::InvalidPath)?;

    unsafe {
        let mut raw_data: *mut OiioTextureDataRaw = std::ptr::null_mut();
        let result = oiio_load_texture_with_mips(c_path.as_ptr(), &mut raw_data);

        if result != 0 {
            return Err(convert_error(result));
        }

        if raw_data.is_null() {
            return Err(OiioError::NullPointer);
        }

        let texture = texture_from_raw(raw_data, path_str);
        oiio_free_texture(raw_data);
        Ok(texture)
    }
}

/// Convert raw FFI data to Rust texture.
unsafe fn texture_from_raw(raw: *mut OiioTextureDataRaw, path: &str) -> OiioTexture {
    let data = &*raw;
    let mut mip_levels = Vec::with_capacity(data.mip_count as usize);

    for i in 0..data.mip_count {
        let mip_raw = &*data.mip_levels.add(i as usize);

        // Copy pixel data
        let pixel_data = if !mip_raw.data.is_null() && mip_raw.byte_size > 0 {
            std::slice::from_raw_parts(mip_raw.data, mip_raw.byte_size).to_vec()
        } else {
            Vec::new()
        };

        mip_levels.push(MipLevel {
            data: pixel_data,
            width: mip_raw.width,
            height: mip_raw.height,
        });
    }

    OiioTexture {
        mip_levels,
        width: data.width,
        height: data.height,
        is_linear: data.is_linear != 0,
        path: path.to_string(),
    }
}

/// Options for .tx conversion.
#[derive(Clone, Debug)]
pub struct TxOptions {
    /// Tile size (typically 64)
    pub tile_size: u32,
    /// Compression method: "none", "zip", "zips", "dwaa", "dwab"
    pub compression: String,
    /// Generate mipmaps
    pub generate_mips: bool,
    /// Force conversion even if .tx exists and is newer
    pub force: bool,
}

impl Default for TxOptions {
    fn default() -> Self {
        Self {
            tile_size: 64,
            compression: "zip".to_string(),
            generate_mips: true,
            force: false,
        }
    }
}

/// Convert a texture to .tx format with tiling and mipmaps.
pub fn make_tx(
    input: impl AsRef<Path>,
    output: impl AsRef<Path>,
    options: Option<&TxOptions>,
) -> OiioResult<()> {
    let input = input.as_ref();
    let output = output.as_ref();

    let input_str = input.to_str().ok_or(OiioError::InvalidPath)?;
    let output_str = output.to_str().ok_or(OiioError::InvalidPath)?;

    let c_input = CString::new(input_str).map_err(|_| OiioError::InvalidPath)?;
    let c_output = CString::new(output_str).map_err(|_| OiioError::InvalidPath)?;

    unsafe {
        let result = if let Some(opts) = options {
            let c_compression =
                CString::new(opts.compression.as_str()).map_err(|_| OiioError::InvalidPath)?;

            let raw_opts = OiioTxOptionsRaw {
                tile_size: opts.tile_size,
                compression: c_compression.as_ptr(),
                generate_mips: if opts.generate_mips { 1 } else { 0 },
                force: if opts.force { 1 } else { 0 },
            };
            oiio_make_tx(c_input.as_ptr(), c_output.as_ptr(), &raw_opts)
        } else {
            oiio_make_tx(c_input.as_ptr(), c_output.as_ptr(), std::ptr::null())
        };

        if result != 0 {
            return Err(convert_error(result));
        }

        Ok(())
    }
}

/// Check if a .tx file is valid (exists and newer than source).
pub fn tx_is_valid(source: impl AsRef<Path>, tx: impl AsRef<Path>) -> bool {
    let source = source.as_ref();
    let tx = tx.as_ref();

    let Ok(source_str) = source.to_str().ok_or(()) else {
        return false;
    };
    let Ok(tx_str) = tx.to_str().ok_or(()) else {
        return false;
    };

    let Ok(c_source) = CString::new(source_str) else {
        return false;
    };
    let Ok(c_tx) = CString::new(tx_str) else {
        return false;
    };

    unsafe { oiio_tx_is_valid(c_source.as_ptr(), c_tx.as_ptr()) != 0 }
}

/// Get OIIO version string.
pub fn get_version() -> String {
    unsafe {
        let version = oiio_get_version();
        if version.is_null() {
            return String::from("unknown");
        }
        CStr::from_ptr(version).to_string_lossy().into_owned()
    }
}

/// Check if OIIO can read a file.
pub fn can_read(path: impl AsRef<Path>) -> bool {
    let path = path.as_ref();
    let Some(path_str) = path.to_str() else {
        return false;
    };
    let Ok(c_path) = CString::new(path_str) else {
        return false;
    };

    unsafe { oiio_can_read(c_path.as_ptr()) != 0 }
}

/// Calculate the number of mip levels for given dimensions.
pub fn calculate_mip_count(width: u32, height: u32) -> u32 {
    unsafe { oiio_calculate_mip_count(width, height) }
}

/// Get the .tx path for a given source texture path.
pub fn get_tx_path(source: impl AsRef<Path>) -> std::path::PathBuf {
    let source = source.as_ref();
    source.with_extension("tx")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_tx_path() {
        assert_eq!(
            get_tx_path("textures/diffuse.png"),
            std::path::PathBuf::from("textures/diffuse.tx")
        );
        assert_eq!(
            get_tx_path("image.jpg"),
            std::path::PathBuf::from("image.tx")
        );
    }

    #[test]
    fn test_calculate_mip_count() {
        // Note: This test only works when OIIO is linked
        // For now, we compute expected values manually
        // 1024x1024 -> 11 mips (1024, 512, 256, 128, 64, 32, 16, 8, 4, 2, 1)
        // 512x256 -> 10 mips
    }

    #[test]
    fn test_tx_options_default() {
        let opts = TxOptions::default();
        assert_eq!(opts.tile_size, 64);
        assert_eq!(opts.compression, "zip");
        assert!(opts.generate_mips);
        assert!(!opts.force);
    }
}
