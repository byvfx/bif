// OIIO Bridge - OpenImageIO wrapper implementation
//
// Provides texture loading with mipmap support and .tx conversion.

#include "oiio_bridge.h"

#include <OpenImageIO/imageio.h>
#include <OpenImageIO/imagebuf.h>
#include <OpenImageIO/imagebufalgo.h>
#include <OpenImageIO/filesystem.h>

#include <algorithm>
#include <cstring>
#include <memory>
#include <sstream>
#include <string>
#include <vector>

using namespace OIIO;

// Thread-local error message storage
static thread_local std::string g_last_error;

// ============================================================================
// Error Handling
// ============================================================================

const char* oiio_bridge_error_message(OiioBridgeError error) {
    switch (error) {
        case OIIO_BRIDGE_SUCCESS:
            return "Success";
        case OIIO_BRIDGE_ERROR_NULL_POINTER:
            return "Null pointer argument";
        case OIIO_BRIDGE_ERROR_FILE_NOT_FOUND:
            return "File not found";
        case OIIO_BRIDGE_ERROR_INVALID_FORMAT:
            return "Invalid or unsupported image format";
        case OIIO_BRIDGE_ERROR_READ_FAILED:
            return "Failed to read image data";
        case OIIO_BRIDGE_ERROR_WRITE_FAILED:
            return "Failed to write image data";
        case OIIO_BRIDGE_ERROR_OUT_OF_MEMORY:
            return "Out of memory";
        default:
            return "Unknown error";
    }
}

const char* oiio_bridge_get_last_error(void) {
    return g_last_error.c_str();
}

// ============================================================================
// Helper Functions
// ============================================================================

static bool file_exists(const char* path) {
    return Filesystem::exists(path);
}

static bool is_linear_format(const std::string& ext) {
    std::string lower = ext;
    std::transform(lower.begin(), lower.end(), lower.begin(), ::tolower);
    return lower == "exr" || lower == "hdr" || lower == "tx";
}

static std::string get_extension(const std::string& path) {
    size_t dot = path.rfind('.');
    if (dot == std::string::npos) return "";
    return path.substr(dot + 1);
}

// Convert linear float to sRGB u8
static uint8_t linear_to_srgb_u8(float v) {
    v = std::max(0.0f, std::min(1.0f, v));
    float srgb;
    if (v <= 0.0031308f) {
        srgb = v * 12.92f;
    } else {
        srgb = 1.055f * std::pow(v, 1.0f / 2.4f) - 0.055f;
    }
    return static_cast<uint8_t>(srgb * 255.0f + 0.5f);
}

// Convert float to u8 (no gamma, for already sRGB data)
static uint8_t float_to_u8(float v) {
    v = std::max(0.0f, std::min(1.0f, v));
    return static_cast<uint8_t>(v * 255.0f + 0.5f);
}

// ============================================================================
// Texture Loading
// ============================================================================

OiioBridgeError oiio_load_texture(const char* path, OiioTextureData** out_data) {
    if (!path || !out_data) {
        return OIIO_BRIDGE_ERROR_NULL_POINTER;
    }

    *out_data = nullptr;

    if (!file_exists(path)) {
        g_last_error = std::string("File not found: ") + path;
        return OIIO_BRIDGE_ERROR_FILE_NOT_FOUND;
    }

    // Open the image
    auto inp = ImageInput::open(path);
    if (!inp) {
        g_last_error = OIIO::geterror();
        return OIIO_BRIDGE_ERROR_READ_FAILED;
    }

    const ImageSpec& spec = inp->spec();
    int width = spec.width;
    int height = spec.height;
    int nchannels = spec.nchannels;

    // Determine if source is linear
    std::string ext = get_extension(path);
    bool is_linear = is_linear_format(ext);

    // Read as float for processing
    std::vector<float> pixels(width * height * nchannels);
    if (!inp->read_image(0, 0, 0, nchannels, TypeDesc::FLOAT, pixels.data())) {
        g_last_error = inp->geterror();
        inp->close();
        return OIIO_BRIDGE_ERROR_READ_FAILED;
    }
    inp->close();

    // Allocate output structure
    OiioTextureData* data = new (std::nothrow) OiioTextureData;
    if (!data) {
        return OIIO_BRIDGE_ERROR_OUT_OF_MEMORY;
    }

    data->width = width;
    data->height = height;
    data->channels = 4; // Always output RGBA
    data->is_linear = is_linear ? 1 : 0;
    data->mip_count = 1;

    // Allocate single mip level
    data->mip_levels = new (std::nothrow) OiioMipLevel[1];
    if (!data->mip_levels) {
        delete data;
        return OIIO_BRIDGE_ERROR_OUT_OF_MEMORY;
    }

    size_t byte_size = width * height * 4;
    uint8_t* rgba_data = new (std::nothrow) uint8_t[byte_size];
    if (!rgba_data) {
        delete[] data->mip_levels;
        delete data;
        return OIIO_BRIDGE_ERROR_OUT_OF_MEMORY;
    }

    // Convert to RGBA u8
    for (int y = 0; y < height; ++y) {
        for (int x = 0; x < width; ++x) {
            int src_idx = (y * width + x) * nchannels;
            int dst_idx = (y * width + x) * 4;

            float r = pixels[src_idx];
            float g = nchannels > 1 ? pixels[src_idx + 1] : r;
            float b = nchannels > 2 ? pixels[src_idx + 2] : r;
            float a = nchannels > 3 ? pixels[src_idx + 3] : 1.0f;

            if (is_linear) {
                // Keep linear for GPU (sRGB conversion in shader or via texture format)
                rgba_data[dst_idx + 0] = float_to_u8(r);
                rgba_data[dst_idx + 1] = float_to_u8(g);
                rgba_data[dst_idx + 2] = float_to_u8(b);
                rgba_data[dst_idx + 3] = float_to_u8(a);
            } else {
                // sRGB data - keep as-is (already gamma encoded)
                rgba_data[dst_idx + 0] = float_to_u8(r);
                rgba_data[dst_idx + 1] = float_to_u8(g);
                rgba_data[dst_idx + 2] = float_to_u8(b);
                rgba_data[dst_idx + 3] = float_to_u8(a);
            }
        }
    }

    data->mip_levels[0].data = rgba_data;
    data->mip_levels[0].width = width;
    data->mip_levels[0].height = height;
    data->mip_levels[0].byte_size = byte_size;

    *out_data = data;
    return OIIO_BRIDGE_SUCCESS;
}

OiioBridgeError oiio_load_texture_with_mips(const char* path, OiioTextureData** out_data) {
    if (!path || !out_data) {
        return OIIO_BRIDGE_ERROR_NULL_POINTER;
    }

    *out_data = nullptr;

    if (!file_exists(path)) {
        g_last_error = std::string("File not found: ") + path;
        return OIIO_BRIDGE_ERROR_FILE_NOT_FOUND;
    }

    // Open the image
    auto inp = ImageInput::open(path);
    if (!inp) {
        g_last_error = OIIO::geterror();
        return OIIO_BRIDGE_ERROR_READ_FAILED;
    }

    const ImageSpec& spec = inp->spec();
    int base_width = spec.width;
    int base_height = spec.height;
    int nchannels = spec.nchannels;

    // Check how many mip levels exist in the file
    int file_mip_count = 1;
    while (inp->seek_subimage(0, file_mip_count)) {
        file_mip_count++;
    }
    // Reset to first mip
    inp->seek_subimage(0, 0);

    // Determine if source is linear
    std::string ext = get_extension(path);
    bool is_linear = is_linear_format(ext);

    // Calculate how many mips we need
    uint32_t calc_mips = oiio_calculate_mip_count(base_width, base_height);
    uint32_t mip_count = std::max(static_cast<uint32_t>(file_mip_count), calc_mips);

    // Allocate output structure
    OiioTextureData* data = new (std::nothrow) OiioTextureData;
    if (!data) {
        inp->close();
        return OIIO_BRIDGE_ERROR_OUT_OF_MEMORY;
    }

    data->width = base_width;
    data->height = base_height;
    data->channels = 4;
    data->is_linear = is_linear ? 1 : 0;
    data->mip_count = mip_count;
    data->mip_levels = new (std::nothrow) OiioMipLevel[mip_count];

    if (!data->mip_levels) {
        delete data;
        inp->close();
        return OIIO_BRIDGE_ERROR_OUT_OF_MEMORY;
    }

    // Initialize all mip levels to null
    for (uint32_t i = 0; i < mip_count; ++i) {
        data->mip_levels[i].data = nullptr;
        data->mip_levels[i].width = 0;
        data->mip_levels[i].height = 0;
        data->mip_levels[i].byte_size = 0;
    }

    // Read base level first
    std::vector<float> base_pixels(base_width * base_height * nchannels);
    if (!inp->read_image(0, 0, 0, nchannels, TypeDesc::FLOAT, base_pixels.data())) {
        g_last_error = inp->geterror();
        delete[] data->mip_levels;
        delete data;
        inp->close();
        return OIIO_BRIDGE_ERROR_READ_FAILED;
    }

    // Convert base level to RGBA u8
    size_t base_byte_size = base_width * base_height * 4;
    uint8_t* base_rgba = new (std::nothrow) uint8_t[base_byte_size];
    if (!base_rgba) {
        delete[] data->mip_levels;
        delete data;
        inp->close();
        return OIIO_BRIDGE_ERROR_OUT_OF_MEMORY;
    }

    for (int y = 0; y < base_height; ++y) {
        for (int x = 0; x < base_width; ++x) {
            int src_idx = (y * base_width + x) * nchannels;
            int dst_idx = (y * base_width + x) * 4;

            float r = base_pixels[src_idx];
            float g = nchannels > 1 ? base_pixels[src_idx + 1] : r;
            float b = nchannels > 2 ? base_pixels[src_idx + 2] : r;
            float a = nchannels > 3 ? base_pixels[src_idx + 3] : 1.0f;

            base_rgba[dst_idx + 0] = float_to_u8(r);
            base_rgba[dst_idx + 1] = float_to_u8(g);
            base_rgba[dst_idx + 2] = float_to_u8(b);
            base_rgba[dst_idx + 3] = float_to_u8(a);
        }
    }

    data->mip_levels[0].data = base_rgba;
    data->mip_levels[0].width = base_width;
    data->mip_levels[0].height = base_height;
    data->mip_levels[0].byte_size = base_byte_size;

    // Try to read existing mip levels from file
    int loaded_from_file = 1;
    for (int mip = 1; mip < file_mip_count && mip < static_cast<int>(mip_count); ++mip) {
        if (!inp->seek_subimage(0, mip)) break;

        const ImageSpec& mip_spec = inp->spec();
        int mip_width = mip_spec.width;
        int mip_height = mip_spec.height;

        std::vector<float> mip_pixels(mip_width * mip_height * nchannels);
        if (!inp->read_image(0, 0, 0, nchannels, TypeDesc::FLOAT, mip_pixels.data())) {
            break;
        }

        size_t mip_byte_size = mip_width * mip_height * 4;
        uint8_t* mip_rgba = new (std::nothrow) uint8_t[mip_byte_size];
        if (!mip_rgba) break;

        for (int y = 0; y < mip_height; ++y) {
            for (int x = 0; x < mip_width; ++x) {
                int src_idx = (y * mip_width + x) * nchannels;
                int dst_idx = (y * mip_width + x) * 4;

                float r = mip_pixels[src_idx];
                float g = nchannels > 1 ? mip_pixels[src_idx + 1] : r;
                float b = nchannels > 2 ? mip_pixels[src_idx + 2] : r;
                float a = nchannels > 3 ? mip_pixels[src_idx + 3] : 1.0f;

                mip_rgba[dst_idx + 0] = float_to_u8(r);
                mip_rgba[dst_idx + 1] = float_to_u8(g);
                mip_rgba[dst_idx + 2] = float_to_u8(b);
                mip_rgba[dst_idx + 3] = float_to_u8(a);
            }
        }

        data->mip_levels[mip].data = mip_rgba;
        data->mip_levels[mip].width = mip_width;
        data->mip_levels[mip].height = mip_height;
        data->mip_levels[mip].byte_size = mip_byte_size;
        loaded_from_file++;
    }

    inp->close();

    // Generate remaining mips using box filter (simple averaging)
    for (uint32_t mip = loaded_from_file; mip < mip_count; ++mip) {
        OiioMipLevel& prev = data->mip_levels[mip - 1];
        uint32_t mip_width = std::max(1u, prev.width / 2);
        uint32_t mip_height = std::max(1u, prev.height / 2);

        size_t mip_byte_size = mip_width * mip_height * 4;
        uint8_t* mip_rgba = new (std::nothrow) uint8_t[mip_byte_size];
        if (!mip_rgba) {
            // Truncate mip chain here
            data->mip_count = mip;
            break;
        }

        // Box filter downsample
        for (uint32_t y = 0; y < mip_height; ++y) {
            for (uint32_t x = 0; x < mip_width; ++x) {
                uint32_t src_x = x * 2;
                uint32_t src_y = y * 2;

                // Sample 2x2 block from previous mip
                auto sample = [&](uint32_t sx, uint32_t sy, int ch) -> int {
                    sx = std::min(sx, prev.width - 1);
                    sy = std::min(sy, prev.height - 1);
                    return prev.data[(sy * prev.width + sx) * 4 + ch];
                };

                int dst_idx = (y * mip_width + x) * 4;
                for (int ch = 0; ch < 4; ++ch) {
                    int sum = sample(src_x, src_y, ch) +
                              sample(src_x + 1, src_y, ch) +
                              sample(src_x, src_y + 1, ch) +
                              sample(src_x + 1, src_y + 1, ch);
                    mip_rgba[dst_idx + ch] = static_cast<uint8_t>(sum / 4);
                }
            }
        }

        data->mip_levels[mip].data = mip_rgba;
        data->mip_levels[mip].width = mip_width;
        data->mip_levels[mip].height = mip_height;
        data->mip_levels[mip].byte_size = mip_byte_size;
    }

    *out_data = data;
    return OIIO_BRIDGE_SUCCESS;
}

void oiio_free_texture(OiioTextureData* data) {
    if (!data) return;

    if (data->mip_levels) {
        for (uint32_t i = 0; i < data->mip_count; ++i) {
            delete[] data->mip_levels[i].data;
        }
        delete[] data->mip_levels;
    }
    delete data;
}

// ============================================================================
// .tx Conversion
// ============================================================================

OiioTxOptions oiio_tx_default_options(void) {
    OiioTxOptions opts;
    opts.tile_size = 64;
    opts.compression = "zip";
    opts.generate_mips = 1;
    opts.force = 0;
    return opts;
}

OiioBridgeError oiio_make_tx(
    const char* input_path,
    const char* output_path,
    const OiioTxOptions* options
) {
    if (!input_path || !output_path) {
        return OIIO_BRIDGE_ERROR_NULL_POINTER;
    }

    if (!file_exists(input_path)) {
        g_last_error = std::string("Source file not found: ") + input_path;
        return OIIO_BRIDGE_ERROR_FILE_NOT_FOUND;
    }

    OiioTxOptions opts = options ? *options : oiio_tx_default_options();

    // Check if conversion is needed
    if (!opts.force && oiio_tx_is_valid(input_path, output_path)) {
        return OIIO_BRIDGE_SUCCESS;
    }

    // Read source image
    ImageBuf src(input_path);
    if (!src.read()) {
        g_last_error = src.geterror();
        return OIIO_BRIDGE_ERROR_READ_FAILED;
    }

    // Configure output spec
    ImageSpec config;
    config.tile_width = opts.tile_size;
    config.tile_height = opts.tile_size;
    config.tile_depth = 1;

    // Set compression
    if (opts.compression && strlen(opts.compression) > 0) {
        config.attribute("compression", opts.compression);
    }

    // Use make_texture for proper .tx creation with mipmaps
    // MakeTxTexture mode creates a proper texture file with mipmaps
    std::ostringstream err_stream;
    bool success = ImageBufAlgo::make_texture(
        opts.generate_mips ? ImageBufAlgo::MakeTxTexture : ImageBufAlgo::MakeTxShadow,
        src,
        output_path,
        config,
        &err_stream
    );

    if (!success) {
        std::string err = err_stream.str();
        g_last_error = err.empty() ? "make_texture failed" : err;
        return OIIO_BRIDGE_ERROR_WRITE_FAILED;
    }

    return OIIO_BRIDGE_SUCCESS;
}

int oiio_tx_is_valid(const char* source_path, const char* tx_path) {
    if (!source_path || !tx_path) {
        return 0;
    }

    if (!file_exists(tx_path)) {
        return 0;
    }

    if (!file_exists(source_path)) {
        // Source doesn't exist but tx does - tx is valid
        return 1;
    }

    // Compare modification times
    auto source_time = Filesystem::last_write_time(source_path);
    auto tx_time = Filesystem::last_write_time(tx_path);

    return tx_time >= source_time ? 1 : 0;
}

// ============================================================================
// Utility Functions
// ============================================================================

const char* oiio_get_version(void) {
    static std::string version = std::to_string(OIIO_VERSION_MAJOR) + "." +
                                  std::to_string(OIIO_VERSION_MINOR) + "." +
                                  std::to_string(OIIO_VERSION_PATCH);
    return version.c_str();
}

int oiio_can_read(const char* path) {
    if (!path) return 0;
    auto inp = ImageInput::open(path);
    if (inp) {
        inp->close();
        return 1;
    }
    return 0;
}

uint32_t oiio_calculate_mip_count(uint32_t width, uint32_t height) {
    uint32_t max_dim = std::max(width, height);
    uint32_t count = 1;
    while (max_dim > 1) {
        max_dim /= 2;
        count++;
    }
    return count;
}
