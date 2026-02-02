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
// Timeline / Animation
// ============================================================================

/// Timeline metadata from the USD stage
typedef struct UsdBridgeTimelineData {
    /// Start time code (first frame)
    double start_time_code;
    /// End time code (last frame)
    double end_time_code;
    /// Frames per second
    double frames_per_second;
    /// 1 if the stage has authored time range metadata, 0 otherwise
    int has_authored_time_range;
} UsdBridgeTimelineData;

/// Get timeline metadata from the stage.
///
/// @param stage Stage handle
/// @param out_data Pointer to receive timeline data
/// @return USD_BRIDGE_SUCCESS on success
UsdBridgeError usd_bridge_get_timeline(
    const UsdBridgeStage* stage,
    UsdBridgeTimelineData* out_data
);

/// A single transform sample at a specific time
typedef struct UsdBridgeXformSample {
    /// Time code for this sample
    double time;
    /// 4x4 column-major transform matrix
    float transform[16];
} UsdBridgeXformSample;

/// Animated mesh data with time samples
typedef struct UsdBridgeAnimatedMeshData {
    /// Mesh index this animation applies to
    size_t mesh_index;
    /// Array of transform samples (NULL if static)
    const UsdBridgeXformSample* xform_samples;
    /// Number of transform samples (0 if static)
    size_t xform_sample_count;
} UsdBridgeAnimatedMeshData;

/// Get animated transform samples for a mesh.
/// Returns the xform time samples for the mesh's world transform.
///
/// @param stage Stage handle
/// @param mesh_index Mesh index
/// @param out_data Pointer to receive animation data
/// @return USD_BRIDGE_SUCCESS on success
UsdBridgeError usd_bridge_get_mesh_animation(
    const UsdBridgeStage* stage,
    size_t mesh_index,
    UsdBridgeAnimatedMeshData* out_data
);

/// Animated instancer data - transforms at multiple time samples
typedef struct UsdBridgeAnimatedInstancerData {
    /// Instancer index this animation applies to
    size_t instancer_index;
    /// Array of time sample values
    const double* time_samples;
    /// Number of time samples
    size_t time_sample_count;
    /// Number of instances
    size_t instance_count;
    /// Flattened transforms: [time_idx * instance_count + instance_idx] -> float[16]
    /// Total size: time_sample_count * instance_count * 16 floats
    const float* transforms;
} UsdBridgeAnimatedInstancerData;

/// Get animated instance transforms for a point instancer.
///
/// @param stage Stage handle
/// @param instancer_index Instancer index
/// @param out_data Pointer to receive animation data
/// @return USD_BRIDGE_SUCCESS on success
UsdBridgeError usd_bridge_get_instancer_animation(
    const UsdBridgeStage* stage,
    size_t instancer_index,
    UsdBridgeAnimatedInstancerData* out_data
);

/// Get animated transform samples for a camera by path.
///
/// @param stage Stage handle
/// @param camera_path Path to the UsdGeomCamera prim
/// @param out_samples Pointer to receive sample array (owned by stage, freed on close)
/// @param out_count Pointer to receive sample count
/// @return USD_BRIDGE_SUCCESS on success, USD_BRIDGE_ERROR_INVALID_PRIM if not found
UsdBridgeError usd_bridge_get_camera_xform_samples(
    const UsdBridgeStage* stage,
    const char* camera_path,
    const UsdBridgeXformSample** out_samples,
    size_t* out_count
);

/// Get the number of cameras in the stage.
///
/// @param stage Stage handle
/// @param out_count Pointer to receive camera count
/// @return USD_BRIDGE_SUCCESS on success
UsdBridgeError usd_bridge_get_camera_count(
    const UsdBridgeStage* stage,
    size_t* out_count
);

/// Get a camera path by index.
///
/// @param stage Stage handle
/// @param index Camera index (0 to camera_count-1)
/// @param out_path Pointer to receive path string (owned by stage)
/// @return USD_BRIDGE_SUCCESS on success
UsdBridgeError usd_bridge_get_camera_path(
    const UsdBridgeStage* stage,
    size_t index,
    const char** out_path
);

/// Evaluate camera transform at a specific time.
/// Returns the interpolated world transform matrix at the given time.
///
/// @param stage Stage handle
/// @param camera_path Path to the UsdGeomCamera prim
/// @param time Time code to evaluate at
/// @param out_transform Pointer to receive 16 floats (4x4 column-major matrix)
/// @return USD_BRIDGE_SUCCESS on success
UsdBridgeError usd_bridge_get_camera_xform_at_time(
    const UsdBridgeStage* stage,
    const char* camera_path,
    double time,
    float* out_transform
);

/// Vertex animation info for a mesh
typedef struct UsdBridgeVertexAnimationInfo {
    /// 1 if mesh has animated vertices, 0 otherwise
    int has_animated_vertices;
    /// Number of time samples (0 if not animated)
    size_t time_sample_count;
    /// Array of time sample values (NULL if not animated)
    const double* time_samples;
} UsdBridgeVertexAnimationInfo;

/// Check if a mesh has animated vertices (point deformation).
///
/// @param stage Stage handle
/// @param mesh_index Mesh index
/// @param out_info Pointer to receive animation info
/// @return USD_BRIDGE_SUCCESS on success
UsdBridgeError usd_bridge_get_mesh_vertex_animation_info(
    const UsdBridgeStage* stage,
    size_t mesh_index,
    UsdBridgeVertexAnimationInfo* out_info
);

/// Get mesh vertices at a specific time.
/// For animated meshes, this returns the interpolated vertex positions.
///
/// @param stage Stage handle
/// @param mesh_index Mesh index
/// @param time Time code to sample at
/// @param out_vertices Pointer to receive vertex array (x,y,z triplets)
/// @param out_vertex_count Pointer to receive vertex count
/// @return USD_BRIDGE_SUCCESS on success
UsdBridgeError usd_bridge_get_mesh_vertices_at_time(
    const UsdBridgeStage* stage,
    size_t mesh_index,
    double time,
    const float** out_vertices,
    size_t* out_vertex_count
);

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

    /// UV coordinates (optional, may be NULL - u, v pairs from primvars:st)
    const float* uvs;
    size_t uv_count;

    /// Per-triangle material IDs (from GeomSubsets, one per triangle)
    /// May be NULL if no GeomSubsets, triangle_count = index_count / 3
    const uint32_t* face_material_ids;
    size_t triangle_count;

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
// Material Data Extraction (UsdPreviewSurface)
// ============================================================================

/// Material data structure for FFI transfer (UsdPreviewSurface or MaterialX)
typedef struct UsdBridgeMaterialData {
    /// Material prim path (e.g., "/World/Looks/Material_0")
    const char* path;

    /// Diffuse/albedo color (RGB, 0-1)
    float diffuse_color[3];

    /// Metallic factor (0=dielectric, 1=metal)
    float metallic;

    /// Roughness factor (0=smooth, 1=rough)
    float roughness;

    /// Specular factor (for non-metallic surfaces)
    float specular;

    /// Opacity (0=transparent, 1=opaque)
    float opacity;

    /// Emissive color (RGB)
    float emissive_color[3];

    /// Texture paths (NULL if not used)
    const char* diffuse_texture;
    const char* roughness_texture;
    const char* metallic_texture;
    const char* normal_texture;
    const char* emissive_texture;

    /// Material source type (1=MaterialX, 0=UsdPreviewSurface or default)
    int is_materialx;
} UsdBridgeMaterialData;

/// Get the number of materials in the stage.
///
/// @param stage Stage handle
/// @param out_count Pointer to receive material count
/// @return USD_BRIDGE_SUCCESS on success
UsdBridgeError usd_bridge_get_material_count(
    const UsdBridgeStage* stage,
    size_t* out_count
);

/// Get material data by index.
/// The returned data is owned by the stage and valid until stage is closed.
///
/// @param stage Stage handle
/// @param index Material index (0 to material_count-1)
/// @param out_data Pointer to receive material data
/// @return USD_BRIDGE_SUCCESS on success
UsdBridgeError usd_bridge_get_material(
    const UsdBridgeStage* stage,
    size_t index,
    UsdBridgeMaterialData* out_data
);

/// Get the material path bound to a mesh.
/// Returns empty string if no material is bound.
///
/// @param stage Stage handle
/// @param mesh_index Mesh index
/// @param out_path Pointer to receive material path (owned by stage)
/// @return USD_BRIDGE_SUCCESS on success
UsdBridgeError usd_bridge_get_mesh_material_path(
    const UsdBridgeStage* stage,
    size_t mesh_index,
    const char** out_path
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

// ============================================================================
// Light Data Extraction (UsdLux)
// ============================================================================

/// Light type enumeration
typedef enum UsdBridgeLightType {
    USD_LIGHT_DISTANT = 0,
    USD_LIGHT_SPHERE = 1,
    USD_LIGHT_RECT = 2,
    USD_LIGHT_DOME = 3,
} UsdBridgeLightType;

/// Light data structure for FFI transfer
typedef struct UsdBridgeLightData {
    /// Prim path (e.g., "/World/Lights/Key")
    const char* path;

    /// Light type
    UsdBridgeLightType type;

    /// Light color (RGB, 0-1)
    float color[3];

    /// Light intensity
    float intensity;

    /// Exposure (power of 2 multiplier)
    float exposure;

    /// World transform (4x4 column-major matrix)
    float transform[16];

    /// Distant light: angular diameter in degrees
    float angle;

    /// Sphere light: radius
    float radius;

    /// Rect light: width
    float width;

    /// Rect light: height
    float height;

    /// Dome light: texture path (NULL if none)
    const char* texture_path;
} UsdBridgeLightData;

/// Get the number of lights in the stage.
///
/// @param stage Stage handle
/// @param out_count Pointer to receive light count
/// @return USD_BRIDGE_SUCCESS on success
UsdBridgeError usd_bridge_get_light_count(
    const UsdBridgeStage* stage,
    size_t* out_count
);

/// Get light data by index.
/// The returned data is owned by the stage and valid until stage is closed.
///
/// @param stage Stage handle
/// @param index Light index (0 to light_count-1)
/// @param out_data Pointer to receive light data
/// @return USD_BRIDGE_SUCCESS on success
UsdBridgeError usd_bridge_get_light(
    const UsdBridgeStage* stage,
    size_t index,
    UsdBridgeLightData* out_data
);

#ifdef __cplusplus
}
#endif

#endif // USD_BRIDGE_H
