// OIIO Bridge - C API for Rust FFI
//
// Provides a thin C wrapper around OpenImageIO for texture loading
// with mipmap support and .tx file conversion.

#ifndef OIIO_BRIDGE_H
#define OIIO_BRIDGE_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// ============================================================================
// Error Handling
// ============================================================================

/// Error codes returned by OIIO bridge functions
typedef enum OiioBridgeError {
    OIIO_BRIDGE_SUCCESS = 0,
    OIIO_BRIDGE_ERROR_NULL_POINTER = 1,
    OIIO_BRIDGE_ERROR_FILE_NOT_FOUND = 2,
    OIIO_BRIDGE_ERROR_INVALID_FORMAT = 3,
    OIIO_BRIDGE_ERROR_READ_FAILED = 4,
    OIIO_BRIDGE_ERROR_WRITE_FAILED = 5,
    OIIO_BRIDGE_ERROR_OUT_OF_MEMORY = 6,
    OIIO_BRIDGE_ERROR_UNKNOWN = 99,
} OiioBridgeError;

/// Get human-readable error message for an error code
const char* oiio_bridge_error_message(OiioBridgeError error);

/// Get the last error message from OIIO (thread-local)
const char* oiio_bridge_get_last_error(void);

// ============================================================================
// Texture Data Structures
// ============================================================================

/// Single mip level data
typedef struct OiioMipLevel {
    /// RGBA u8 pixel data (width * height * 4 bytes)
    uint8_t* data;
    /// Width of this mip level
    uint32_t width;
    /// Height of this mip level
    uint32_t height;
    /// Size in bytes
    size_t byte_size;
} OiioMipLevel;

/// Texture data with all mip levels
typedef struct OiioTextureData {
    /// Array of mip levels (index 0 = base level)
    OiioMipLevel* mip_levels;
    /// Number of mip levels
    uint32_t mip_count;
    /// Base width (mip level 0)
    uint32_t width;
    /// Base height (mip level 0)
    uint32_t height;
    /// Number of channels (3 or 4)
    uint32_t channels;
    /// Whether source was linear (EXR/HDR) or sRGB
    int is_linear;
} OiioTextureData;

/// HDR image data (linear float RGB)
typedef struct OiioHdrImage {
    /// RGB float pixel data (width * height * 3 floats)
    float* data;
    /// Width in pixels
    uint32_t width;
    /// Height in pixels
    uint32_t height;
    /// Number of channels (always 3 for HDR output)
    uint32_t channels;
} OiioHdrImage;

// ============================================================================
// Texture Loading
// ============================================================================

/// Load a texture file with all mip levels.
/// Supports: PNG, JPG, EXR, HDR, TIFF, TX
/// Returns RGBA u8 data (sRGB textures are kept as-is, linear converted to u8).
///
/// @param path     Path to the texture file (UTF-8)
/// @param out_data Pointer to receive texture data
/// @return OIIO_BRIDGE_SUCCESS on success
OiioBridgeError oiio_load_texture(const char* path, OiioTextureData** out_data);

/// Load a texture and generate mipmaps if not present.
/// If file is .tx with mipmaps, loads them directly.
/// Otherwise generates mipmaps from base level.
///
/// @param path     Path to the texture file (UTF-8)
/// @param out_data Pointer to receive texture data
/// @return OIIO_BRIDGE_SUCCESS on success
OiioBridgeError oiio_load_texture_with_mips(const char* path, OiioTextureData** out_data);

/// Free texture data allocated by oiio_load_texture
///
/// @param data Texture data to free (safe to pass NULL)
void oiio_free_texture(OiioTextureData* data);

/// Load an HDR image as linear float RGB.
/// Supports EXR, HDR, TX (and any OIIO-readable float format).
///
/// @param path     Path to the HDR image file (UTF-8)
/// @param out_data Pointer to receive HDR image data
/// @return OIIO_BRIDGE_SUCCESS on success
OiioBridgeError oiio_load_hdr(const char* path, OiioHdrImage** out_data);

/// Free HDR image data allocated by oiio_load_hdr
///
/// @param data HDR image data to free (safe to pass NULL)
void oiio_free_hdr(OiioHdrImage* data);

// ============================================================================
// .tx Conversion (maketx equivalent)
// ============================================================================

/// Conversion options for make_tx
typedef struct OiioTxOptions {
    /// Tile size (typically 64)
    uint32_t tile_size;
    /// Compression method: "none", "zip", "zips", "dwaa", "dwab"
    const char* compression;
    /// Generate mipmaps
    int generate_mips;
    /// Force conversion even if .tx exists and is newer
    int force;
} OiioTxOptions;

/// Default .tx conversion options
OiioTxOptions oiio_tx_default_options(void);

/// Convert a texture to .tx format with tiling and mipmaps.
/// Equivalent to OpenImageIO's maketx utility.
///
/// @param input_path   Path to source texture (PNG, JPG, EXR, etc.)
/// @param output_path  Path for output .tx file
/// @param options      Conversion options (NULL for defaults)
/// @return OIIO_BRIDGE_SUCCESS on success
OiioBridgeError oiio_make_tx(
    const char* input_path,
    const char* output_path,
    const OiioTxOptions* options
);

/// Check if a .tx file is valid (exists and newer than source).
///
/// @param source_path Path to source texture
/// @param tx_path     Path to .tx file
/// @return 1 if .tx is valid, 0 if conversion needed
int oiio_tx_is_valid(const char* source_path, const char* tx_path);

// ============================================================================
// Utility Functions
// ============================================================================

/// Get OIIO version string (e.g., "2.5.1.0")
const char* oiio_get_version(void);

/// Check if a file format is supported for reading
int oiio_can_read(const char* path);

/// Get the number of mip levels that would be generated for a given dimension
uint32_t oiio_calculate_mip_count(uint32_t width, uint32_t height);

#ifdef __cplusplus
}
#endif

#endif // OIIO_BRIDGE_H
