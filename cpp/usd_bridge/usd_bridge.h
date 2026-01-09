// USD Bridge - C API for Rust FFI
//
// Provides a thin C wrapper around Pixar's USD C++ library.
// This allows Rust code to load USDA/USD/USDC files and extract
// mesh and instancer data without bindgen complexity.

#ifndef USD_BRIDGE_H
#define USD_BRIDGE_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// ============================================================================
// Opaque Types
// ============================================================================

/// Opaque handle to a USD stage
typedef struct UsdBridgeStage UsdBridgeStage;

// ============================================================================
// Error Handling
// ============================================================================

/// Error codes returned by USD bridge functions
typedef enum UsdBridgeError {
    USD_BRIDGE_SUCCESS = 0,
    USD_BRIDGE_ERROR_NULL_POINTER = 1,
    USD_BRIDGE_ERROR_FILE_NOT_FOUND = 2,
    USD_BRIDGE_ERROR_INVALID_STAGE = 3,
    USD_BRIDGE_ERROR_INVALID_PRIM = 4,
    USD_BRIDGE_ERROR_OUT_OF_MEMORY = 5,
    USD_BRIDGE_ERROR_UNKNOWN = 99,
} UsdBridgeError;

/// Get human-readable error message for an error code
const char* usd_bridge_error_message(UsdBridgeError error);

// ============================================================================
// Stage Management
// ============================================================================

/// Open a USD stage from a file path.
/// Supports .usda (text), .usdc (binary), and .usd (either) formats.
/// References are automatically resolved.
///
/// @param path     Path to the USD file (UTF-8 encoded)
/// @param out_stage Pointer to receive the opened stage handle
/// @return USD_BRIDGE_SUCCESS on success, error code otherwise
UsdBridgeError usd_bridge_open_stage(const char* path, UsdBridgeStage** out_stage);

/// Close a USD stage and free all associated resources.
///
/// @param stage Stage handle to close (safe to pass NULL)
void usd_bridge_close_stage(UsdBridgeStage* stage);

/// Clear cached mesh/instancer data to free memory while keeping stage open.
/// Useful for memory management in long-running sessions.
///
/// @param stage Stage handle (safe to pass NULL)
void usd_bridge_clear_cache(UsdBridgeStage* stage);

// ============================================================================
// Scene Traversal
// ============================================================================

/// Get the number of mesh prims in the stage (UsdGeomMesh).
///
/// @param stage Stage handle
/// @param out_count Pointer to receive mesh count
/// @return USD_BRIDGE_SUCCESS on success
UsdBridgeError usd_bridge_get_mesh_count(
    const UsdBridgeStage* stage,
    size_t* out_count
);

/// Get the number of point instancer prims in the stage (UsdGeomPointInstancer).
///
/// @param stage Stage handle
/// @param out_count Pointer to receive instancer count
/// @return USD_BRIDGE_SUCCESS on success
UsdBridgeError usd_bridge_get_instancer_count(
    const UsdBridgeStage* stage,
    size_t* out_count
);

// ============================================================================
// Mesh Data Extraction
// ============================================================================

/// Mesh data structure for FFI transfer
typedef struct UsdBridgeMeshData {
    /// Prim path (e.g., "/World/Mesh")
    const char* path;

    /// Vertex positions (x, y, z triplets)
    const float* vertices;
    size_t vertex_count;

    /// Triangle indices (i0, i1, i2 triplets)
    const uint32_t* indices;
    size_t index_count;

    /// Vertex normals (optional, may be NULL)
    const float* normals;
    size_t normal_count;

    /// World transform (4x4 column-major matrix)
    float transform[16];
} UsdBridgeMeshData;

/// Get mesh data by index.
/// The returned data is owned by the stage and valid until stage is closed.
///
/// @param stage Stage handle
/// @param index Mesh index (0 to mesh_count-1)
/// @param out_data Pointer to receive mesh data
/// @return USD_BRIDGE_SUCCESS on success
UsdBridgeError usd_bridge_get_mesh(
    const UsdBridgeStage* stage,
    size_t index,
    UsdBridgeMeshData* out_data
);

// ============================================================================
// Point Instancer Data Extraction
// ============================================================================

/// Point instancer data structure for FFI transfer
typedef struct UsdBridgeInstancerData {
    /// Prim path (e.g., "/World/Instancer")
    const char* path;

    /// Prototype prim paths (array of strings)
    const char* const* prototype_paths;
    size_t prototype_count;

    /// Instance transforms (4x4 column-major matrices)
    const float* transforms;
    size_t instance_count;

    /// Prototype index per instance
    const int32_t* proto_indices;
} UsdBridgeInstancerData;

/// Get point instancer data by index.
/// The returned data is owned by the stage and valid until stage is closed.
///
/// @param stage Stage handle
/// @param index Instancer index (0 to instancer_count-1)
/// @param out_data Pointer to receive instancer data
/// @return USD_BRIDGE_SUCCESS on success
UsdBridgeError usd_bridge_get_instancer(
    const UsdBridgeStage* stage,
    size_t index,
    UsdBridgeInstancerData* out_data
);

// ============================================================================
// Export Functions
// ============================================================================

/// Export a stage to a file.
/// Format is determined by file extension (.usda, .usdc, .usd).
///
/// @param stage Stage handle
/// @param path Output file path (UTF-8 encoded)
/// @return USD_BRIDGE_SUCCESS on success
UsdBridgeError usd_bridge_export_stage(
    const UsdBridgeStage* stage,
    const char* path
);

// ============================================================================
// Prim Traversal (Scene Browser Support)
// ============================================================================

/// Prim info structure for scene hierarchy browsing
typedef struct UsdBridgePrimInfo {
    /// Prim path (e.g., "/World/Mesh")
    const char* path;

    /// Type name (e.g., "Mesh", "Xform", "PointInstancer", "Scope")
    const char* type_name;

    /// Whether prim is active (visible in composed scene)
    int is_active;

    /// Whether prim has children
    int has_children;

    /// Number of direct children
    size_t child_count;
} UsdBridgePrimInfo;

/// Get the total number of prims in the stage (including all types).
///
/// @param stage Stage handle
/// @param out_count Pointer to receive prim count
/// @return USD_BRIDGE_SUCCESS on success
UsdBridgeError usd_bridge_get_prim_count(
    const UsdBridgeStage* stage,
    size_t* out_count
);

/// Get prim info by index.
/// Index order is depth-first traversal order.
///
/// @param stage Stage handle
/// @param index Prim index (0 to prim_count-1)
/// @param out_info Pointer to receive prim info
/// @return USD_BRIDGE_SUCCESS on success
UsdBridgeError usd_bridge_get_prim_info(
    const UsdBridgeStage* stage,
    size_t index,
    UsdBridgePrimInfo* out_info
);

/// Get the root prim paths (direct children of the pseudo-root).
///
/// @param stage Stage handle
/// @param out_count Pointer to receive root count
/// @return USD_BRIDGE_SUCCESS on success
UsdBridgeError usd_bridge_get_root_prim_count(
    const UsdBridgeStage* stage,
    size_t* out_count
);

/// Get a root prim path by index.
///
/// @param stage Stage handle
/// @param index Root prim index
/// @param out_path Pointer to receive path string (owned by stage)
/// @return USD_BRIDGE_SUCCESS on success
UsdBridgeError usd_bridge_get_root_prim_path(
    const UsdBridgeStage* stage,
    size_t index,
    const char** out_path
);

/// Get child prim paths for a given parent path.
///
/// @param stage Stage handle
/// @param parent_path Path to parent prim (or "/" for root)
/// @param out_count Pointer to receive child count
/// @return USD_BRIDGE_SUCCESS on success
UsdBridgeError usd_bridge_get_children_count(
    const UsdBridgeStage* stage,
    const char* parent_path,
    size_t* out_count
);

/// Get a child prim path by index.
///
/// @param stage Stage handle
/// @param parent_path Path to parent prim
/// @param index Child index
/// @param out_path Pointer to receive path string (owned by stage)
/// @return USD_BRIDGE_SUCCESS on success
UsdBridgeError usd_bridge_get_child_path(
    const UsdBridgeStage* stage,
    const char* parent_path,
    size_t index,
    const char** out_path
);

/// Get prim info by path.
///
/// @param stage Stage handle
/// @param path Prim path (e.g., "/World/Mesh")
/// @param out_info Pointer to receive prim info
/// @return USD_BRIDGE_SUCCESS on success
UsdBridgeError usd_bridge_get_prim_info_by_path(
    const UsdBridgeStage* stage,
    const char* path,
    UsdBridgePrimInfo* out_info
);

#ifdef __cplusplus
}
#endif

#endif // USD_BRIDGE_H
