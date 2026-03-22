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

/// Free bulk mesh geometry (normals, UVs, subdivision) after Rust has copied it.
/// Keeps vertices/indices/paths for animation. Frees ~50% of mesh cache RAM.
void usd_bridge_free_mesh_geometry(UsdBridgeStage* stage);

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

// ============================================================================
// Stage Metadata (units/axis)
// ============================================================================

/// Up axis values
typedef enum UsdBridgeUpAxis {
    USD_BRIDGE_UP_AXIS_Y = 0,
    USD_BRIDGE_UP_AXIS_Z = 1,
} UsdBridgeUpAxis;

/// Stage metadata (metersPerUnit, upAxis, timeCodesPerSecond)
typedef struct UsdBridgeStageMetadata {
    /// Scene scale in meters (e.g. 0.01 = centimeters)
    double meters_per_unit;
    /// Up axis (Y or Z)
    UsdBridgeUpAxis up_axis;
    /// Time codes per second (default 24.0)
    double time_codes_per_second;
} UsdBridgeStageMetadata;

/// Get stage metadata (metersPerUnit, upAxis).
///
/// @param stage Stage handle
/// @param out_data Pointer to receive stage metadata
/// @return USD_BRIDGE_SUCCESS on success
UsdBridgeError usd_bridge_get_stage_metadata(
    const UsdBridgeStage* stage,
    UsdBridgeStageMetadata* out_data
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

/// Camera lens/clipping properties from UsdGeomCamera
typedef struct UsdBridgeCameraProperties {
    /// Focal length in mm
    float focal_length;
    /// Vertical aperture in mm
    float vertical_aperture;
    /// Near clipping plane in scene units
    float clip_near;
    /// Far clipping plane in scene units
    float clip_far;
    /// Horizontal aperture in mm
    float horizontal_aperture;
} UsdBridgeCameraProperties;

/// Get camera lens and clipping properties at a specific time.
///
/// @param stage Stage handle
/// @param camera_path Path to the UsdGeomCamera prim
/// @param time Time code to evaluate at
/// @param out_props Pointer to receive camera properties
/// @return USD_BRIDGE_SUCCESS on success
UsdBridgeError usd_bridge_get_camera_properties(
    const UsdBridgeStage* stage,
    const char* camera_path,
    double time,
    UsdBridgeCameraProperties* out_props
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

/// Mesh purpose enumeration (UsdGeomImageable purpose attribute)
typedef enum UsdBridgePurpose {
    USD_BRIDGE_PURPOSE_DEFAULT = 0,
    USD_BRIDGE_PURPOSE_RENDER = 1,
    USD_BRIDGE_PURPOSE_PROXY = 2,
    USD_BRIDGE_PURPOSE_GUIDE = 3,
} UsdBridgePurpose;

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

    /// Mesh purpose (default/render/proxy/guide)
    UsdBridgePurpose purpose;

    /// 1 if this mesh came from a native instance proxy, 0 otherwise
    int is_instance_proxy;

    /// Computed visibility: 1=visible, 0=invisible (considers inherited visibility)
    int visibility;

    /// Double-sided flag from UsdGeomMesh (1=double-sided, 0=single-sided)
    int double_sided;

    /// Subdivision scheme ("none", "catmullClark", "loop", "bilinear")
    const char* subdivision_scheme;

    /// Normals interpolation: 0=vertex, 1=faceVarying, 2=uniform, 3=constant
    int normals_interpolation;

    /// Display color (primvars:displayColor, RGB per-vertex or single, optional)
    const float* display_color;
    size_t display_color_count;

    /// Display opacity (primvars:displayOpacity, single value, default 1.0)
    float display_opacity;

    /// 1 if xformOpOrder contains !resetXformStack! (ignore parent transforms)
    int resets_xform_stack;

    /// Original face vertex counts (polygon topology, for subdivision surfaces)
    /// NULL if subdivisionScheme is "none" or "bilinear"
    const int32_t* face_vertex_counts;
    size_t face_count;

    /// Original face vertex indices (polygon topology, for subdivision surfaces)
    /// NULL if subdivisionScheme is "none" or "bilinear"
    const int32_t* face_vertex_indices;
    size_t face_vertex_index_count;

    /// Crease edge indices (pairs of vertex indices)
    const int32_t* crease_indices;
    size_t crease_index_count;

    /// Crease lengths (vertices per crease chain)
    const int32_t* crease_lengths;
    size_t crease_length_count;

    /// Crease sharpnesses (one per crease chain)
    const float* crease_sharpnesses;
    size_t crease_sharpness_count;
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
// Native Instance Data (for USD instanceable=true meshes)
// ============================================================================

/// Native instance data — one per occurrence of an instanced mesh
typedef struct UsdNativeInstanceData {
    /// Index into the meshes array for prototype geometry
    int proto_mesh_idx;
    /// World transform (4x4 column-major matrix)
    float transform[16];
    /// Material override index (-1 = use prototype material)
    int material_override_idx;
    /// Purpose (0=default, 1=render, 2=proxy, 3=guide)
    int purpose;
} UsdNativeInstanceData;

/// Get the number of native instances (from instanceable=true prims).
///
/// @param stage Stage handle
/// @param out_count Pointer to receive native instance count
/// @return USD_BRIDGE_SUCCESS on success
UsdBridgeError usd_bridge_get_native_instance_count(
    const UsdBridgeStage* stage,
    size_t* out_count
);

/// Get native instance data by index.
///
/// @param stage Stage handle
/// @param index Native instance index
/// @param out_data Pointer to receive native instance data
/// @return USD_BRIDGE_SUCCESS on success
UsdBridgeError usd_bridge_get_native_instance(
    const UsdBridgeStage* stage,
    size_t index,
    UsdNativeInstanceData* out_data
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

    /// Velocities (vec3 per instance, optional — for motion blur interpolation)
    const float* velocities;
    size_t velocity_count;

    /// Angular velocities (vec3 per instance, optional — for motion blur)
    const float* angular_velocities;
    size_t angular_velocity_count;

    /// Invisible instance IDs (instances to hide)
    const int64_t* invisible_ids;
    size_t invisible_id_count;
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

    /// Transmission weight (0=opaque, 1=fully transmissive glass)
    float transmission;

    /// Specular index of refraction (default 1.5 = glass/plastic)
    float specular_ior;

    /// Emissive color (RGB)
    float emissive_color[3];

    /// Texture paths (NULL if not used)
    const char* diffuse_texture;
    const char* roughness_texture;
    const char* metallic_texture;
    const char* normal_texture;
    const char* emissive_texture;
    const char* opacity_texture;

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

    /// Computed visibility: 1=visible, 0=invisible (considers inherited visibility)
    int visibility;

    /// 1 if prim has a payload arc
    int has_payload;

    /// 1 if prim's payload is currently loaded
    int is_loaded;

    /// Number of variant sets on this prim
    size_t variant_set_count;

    /// 1 if prim has inherit arcs
    int has_inherits;

    /// 1 if prim has specializes arcs
    int has_specializes;
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
// Payload Load/Unload
// ============================================================================

/// Load a prim's payload (makes payload content available).
UsdBridgeError usd_bridge_load_payload(
    UsdBridgeStage* stage,
    const char* prim_path
);

/// Unload a prim's payload (frees memory, content no longer traversable).
UsdBridgeError usd_bridge_unload_payload(
    UsdBridgeStage* stage,
    const char* prim_path
);

// (usd_bridge_edit_layer_add_payload declared below, after UsdBridgeEditLayer)

// ============================================================================
// Variant Query / Selection
// ============================================================================

/// Get the number of variant sets on a prim.
UsdBridgeError usd_bridge_get_variant_set_count(
    const UsdBridgeStage* stage,
    const char* prim_path,
    size_t* out_count
);

/// Get a variant set name by index.
UsdBridgeError usd_bridge_get_variant_set_name(
    const UsdBridgeStage* stage,
    const char* prim_path,
    size_t index,
    const char** out_name
);

/// Get the number of variants in a variant set.
UsdBridgeError usd_bridge_get_variant_count(
    const UsdBridgeStage* stage,
    const char* prim_path,
    const char* variant_set_name,
    size_t* out_count
);

/// Get a variant name by index within a variant set.
UsdBridgeError usd_bridge_get_variant_name(
    const UsdBridgeStage* stage,
    const char* prim_path,
    const char* variant_set_name,
    size_t index,
    const char** out_name
);

/// Get the current variant selection for a variant set.
UsdBridgeError usd_bridge_get_variant_selection(
    const UsdBridgeStage* stage,
    const char* prim_path,
    const char* variant_set_name,
    const char** out_selection
);

/// Set the variant selection for a variant set (triggers re-composition).
UsdBridgeError usd_bridge_set_variant_selection(
    UsdBridgeStage* stage,
    const char* prim_path,
    const char* variant_set_name,
    const char* variant_name
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
    USD_LIGHT_CYLINDER = 4,
    USD_LIGHT_DISK = 5,
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

    /// Cylinder light: length
    float length;

    /// ShapingAPI: cone angle in degrees (spotlight, 0 = no shaping)
    float shaping_cone_angle;

    /// ShapingAPI: cone softness (0-1)
    float shaping_cone_softness;

    /// ShapingAPI: focus (0 = uniform)
    float shaping_focus;

    /// ShapingAPI: IES profile path (NULL if none)
    const char* shaping_ies_file;

    /// Light linking: include paths (array of strings, NULL if no light linking)
    const char* const* light_link_includes;
    size_t light_link_include_count;

    /// Light linking: exclude paths (array of strings, NULL if none)
    const char* const* light_link_excludes;
    size_t light_link_exclude_count;
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

// ============================================================================
// UsdGeomPoints Data Extraction
// ============================================================================

/// Points data structure for FFI transfer (UsdGeomPoints — particle/point cloud)
typedef struct UsdBridgePointsData {
    /// Prim path
    const char* path;

    /// Point positions (x, y, z triplets)
    const float* positions;
    size_t point_count;

    /// Per-point widths (optional, may be NULL)
    const float* widths;
    size_t width_count;

    /// Per-point normals (optional, may be NULL)
    const float* normals;
    size_t normal_count;

    /// Per-point IDs (optional, may be NULL)
    const int64_t* ids;
    size_t id_count;

    /// World transform (4x4 column-major matrix)
    float transform[16];
} UsdBridgePointsData;

/// Get the number of UsdGeomPoints prims in the stage.
UsdBridgeError usd_bridge_get_points_count(
    const UsdBridgeStage* stage,
    size_t* out_count
);

/// Get points data by index.
UsdBridgeError usd_bridge_get_points(
    const UsdBridgeStage* stage,
    size_t index,
    UsdBridgePointsData* out_data
);

// ============================================================================
// UsdGeomBasisCurves Data Extraction
// ============================================================================

/// Curve type enumeration
typedef enum UsdBridgeCurveType {
    USD_CURVE_LINEAR = 0,
    USD_CURVE_CUBIC = 1,
} UsdBridgeCurveType;

/// Curve basis enumeration
typedef enum UsdBridgeCurveBasis {
    USD_CURVE_BASIS_BEZIER = 0,
    USD_CURVE_BASIS_BSPLINE = 1,
    USD_CURVE_BASIS_CATMULL_ROM = 2,
} UsdBridgeCurveBasis;

/// Curve wrap enumeration
typedef enum UsdBridgeCurveWrap {
    USD_CURVE_WRAP_NONPERIODIC = 0,
    USD_CURVE_WRAP_PERIODIC = 1,
    USD_CURVE_WRAP_PINNED = 2,
} UsdBridgeCurveWrap;

/// BasisCurves data structure for FFI transfer
typedef struct UsdBridgeCurvesData {
    /// Prim path
    const char* path;

    /// Control point positions (x, y, z triplets)
    const float* points;
    size_t point_count;

    /// Per-vertex or per-curve widths (optional)
    const float* widths;
    size_t width_count;

    /// Vertex counts per curve
    const int32_t* curve_vertex_counts;
    size_t curve_count;

    /// Curve type (linear or cubic)
    UsdBridgeCurveType type;

    /// Curve basis (bezier, bspline, catmull-rom) — only meaningful for cubic
    UsdBridgeCurveBasis basis;

    /// Wrap mode (nonperiodic, periodic, pinned)
    UsdBridgeCurveWrap wrap;

    /// World transform (4x4 column-major matrix)
    float transform[16];
} UsdBridgeCurvesData;

/// Get the number of UsdGeomBasisCurves prims in the stage.
UsdBridgeError usd_bridge_get_curves_count(
    const UsdBridgeStage* stage,
    size_t* out_count
);

/// Get curves data by index.
UsdBridgeError usd_bridge_get_curves(
    const UsdBridgeStage* stage,
    size_t index,
    UsdBridgeCurvesData* out_data
);

// ============================================================================
// Arbitrary Primvar Query API
// ============================================================================

/// Primvar data type enumeration
typedef enum UsdBridgePrimvarType {
    USD_PRIMVAR_FLOAT = 0,
    USD_PRIMVAR_FLOAT2 = 1,
    USD_PRIMVAR_FLOAT3 = 2,
    USD_PRIMVAR_INT = 3,
} UsdBridgePrimvarType;

/// Primvar interpolation enumeration
typedef enum UsdBridgePrimvarInterpolation {
    USD_PRIMVAR_INTERP_CONSTANT = 0,
    USD_PRIMVAR_INTERP_UNIFORM = 1,
    USD_PRIMVAR_INTERP_VERTEX = 2,
    USD_PRIMVAR_INTERP_FACE_VARYING = 3,
} UsdBridgePrimvarInterpolation;

/// Arbitrary primvar data for FFI transfer
typedef struct UsdBridgePrimvarData {
    /// Primvar name (e.g., "Cd", "pscale")
    const char* name;

    /// Data type
    UsdBridgePrimvarType type;

    /// Interpolation
    UsdBridgePrimvarInterpolation interpolation;

    /// Raw float data (for float/float2/float3 types)
    const float* float_data;

    /// Raw int data (for int type)
    const int32_t* int_data;

    /// Number of elements
    size_t element_count;
} UsdBridgePrimvarData;

/// Get the number of user primvars on a mesh (excludes built-in st, normals, displayColor).
UsdBridgeError usd_bridge_get_mesh_primvar_count(
    const UsdBridgeStage* stage,
    size_t mesh_index,
    size_t* out_count
);

/// Get a user primvar by index for a mesh.
UsdBridgeError usd_bridge_get_mesh_primvar(
    const UsdBridgeStage* stage,
    size_t mesh_index,
    size_t primvar_index,
    UsdBridgePrimvarData* out_data
);

// ============================================================================
// Collection Material Binding
// ============================================================================

/// Get the material path bound to a mesh via collection-based binding.
/// Falls back to direct binding if no collection binding found.
/// Returns empty string if no material is bound.
UsdBridgeError usd_bridge_get_mesh_collection_material_path(
    const UsdBridgeStage* stage,
    size_t mesh_index,
    const char** out_path
);

// ============================================================================
// UsdSkel Data Extraction
// ============================================================================

/// Skeleton data for FFI transfer
typedef struct UsdBridgeSkeletonData {
    /// Prim path
    const char* path;

    /// Joint paths (array of token strings, e.g., "Hips/Spine/Chest")
    const char* const* joint_paths;
    size_t joint_count;

    /// Bind transforms (4x4 column-major per joint, joint_count * 16 floats)
    const float* bind_transforms;

    /// Rest transforms (4x4 column-major per joint, joint_count * 16 floats)
    const float* rest_transforms;
} UsdBridgeSkeletonData;

/// Skin binding data for a skinned mesh
typedef struct UsdBridgeSkinBindingData {
    /// Mesh prim path
    const char* mesh_path;

    /// Skeleton prim path this mesh is bound to
    const char* skeleton_path;

    /// Joint indices per vertex (joint_indices_element_size per vertex)
    const int32_t* joint_indices;
    size_t joint_indices_count;

    /// Joint weights per vertex (same layout as joint_indices)
    const float* joint_weights;
    size_t joint_weights_count;

    /// Number of influences per vertex
    size_t joint_indices_element_size;

    /// Geom bind transform (4x4 column-major)
    float geom_bind_transform[16];
} UsdBridgeSkinBindingData;

/// Get the number of UsdSkelSkeleton prims.
UsdBridgeError usd_bridge_get_skeleton_count(
    const UsdBridgeStage* stage,
    size_t* out_count
);

/// Get skeleton data by index.
UsdBridgeError usd_bridge_get_skeleton(
    const UsdBridgeStage* stage,
    size_t index,
    UsdBridgeSkeletonData* out_data
);

/// Get skin binding data for a mesh (if it has UsdSkelBindingAPI).
/// Returns USD_BRIDGE_ERROR_INVALID_PRIM if mesh has no skin binding.
UsdBridgeError usd_bridge_get_skin_binding(
    const UsdBridgeStage* stage,
    size_t mesh_index,
    UsdBridgeSkinBindingData* out_data
);

// ============================================================================
// UsdVol Data Extraction
// ============================================================================

/// Volume data for FFI transfer
typedef struct UsdBridgeVolumeData {
    /// Prim path
    const char* path;

    /// OpenVDB file path (resolved)
    const char* vdb_file_path;

    /// Field name within the VDB file (e.g., "density", "temperature")
    const char* field_name;

    /// World transform (4x4 column-major matrix)
    float transform[16];
} UsdBridgeVolumeData;

/// Get the number of volume prims in the stage.
UsdBridgeError usd_bridge_get_volume_count(
    const UsdBridgeStage* stage,
    size_t* out_count
);

/// Get volume data by index.
UsdBridgeError usd_bridge_get_volume(
    const UsdBridgeStage* stage,
    size_t index,
    UsdBridgeVolumeData* out_data
);

// ============================================================================
// Edit Layer Export
// ============================================================================

/// Opaque handle to an edit layer (a writable USD stage).
typedef struct UsdBridgeEditLayer UsdBridgeEditLayer;

/// Create a new empty USD stage for writing edit opinions.
///
/// @param output_path Output file path (.usda or .usdc)
/// @param out_layer Pointer to receive the edit layer handle
/// @return USD_BRIDGE_SUCCESS on success
UsdBridgeError usd_bridge_create_edit_layer(
    const char* output_path,
    UsdBridgeEditLayer** out_layer
);

/// Write a transform opinion (xformOp:transform) at the given prim path and time.
///
/// @param layer Edit layer handle
/// @param prim_path USD prim path (e.g., "/World/mesh_0")
/// @param time Time code (-1 for default/static)
/// @param matrix_16 Column-major 4x4 transform matrix (16 floats)
/// @return USD_BRIDGE_SUCCESS on success
UsdBridgeError usd_bridge_write_xform_opinion(
    UsdBridgeEditLayer* layer,
    const char* prim_path,
    double time,
    const float* matrix_16
);

/// Save the edit layer to disk (does NOT free the handle).
///
/// @param layer Edit layer handle
/// @return USD_BRIDGE_SUCCESS on success
UsdBridgeError usd_bridge_save_edit_layer(
    UsdBridgeEditLayer* layer
);

/// Free the edit layer handle without saving.
///
/// @param layer Edit layer handle (safe to pass NULL)
void usd_bridge_free_edit_layer(
    UsdBridgeEditLayer* layer
);

/// Add a sublayer reference to the edit layer.
/// The sublayer's opinions will be composed under the edit layer's opinions.
///
/// @param layer Edit layer handle
/// @param sublayer_path File path to the sublayer USD file
/// @return USD_BRIDGE_SUCCESS on success
UsdBridgeError usd_bridge_edit_layer_add_sublayer(
    UsdBridgeEditLayer* layer,
    const char* sublayer_path
);

/// Add a reference on a prim (for prototype linking in instancers).
///
/// @param layer Edit layer handle
/// @param prim_path USD prim path to add the reference on
/// @param reference_file File path to the referenced USD file
/// @param reference_prim_path Prim path within the referenced file (NULL for default prim)
/// @return USD_BRIDGE_SUCCESS on success
UsdBridgeError usd_bridge_edit_layer_add_reference(
    UsdBridgeEditLayer* layer,
    const char* prim_path,
    const char* reference_file,
    const char* reference_prim_path
);

/// Set the default prim on the edit layer's stage.
///
/// @param layer Edit layer handle
/// @param prim_path USD prim path to set as default
/// @return USD_BRIDGE_SUCCESS on success
UsdBridgeError usd_bridge_edit_layer_set_default_prim(
    UsdBridgeEditLayer* layer,
    const char* prim_path
);

/// Write a UsdGeomPointInstancer prim with positions, orientations, scales,
/// prototype indices, and prototype relationship targets.
///
/// @param layer Edit layer handle
/// @param prim_path USD prim path for the PointInstancer
/// @param positions Flat array of float[3] per instance (count * 3 floats)
/// @param orientations Flat array of float[4] per instance (wxyz quats, count * 4), NULL for identity
/// @param scales Flat array of float[3] per instance (count * 3), NULL for uniform 1.0
/// @param proto_indices Prototype index per instance (count ints)
/// @param count Number of instances
/// @param prototype_paths Array of prototype prim path strings
/// @param prototype_count Number of prototype paths
/// @return USD_BRIDGE_SUCCESS on success
UsdBridgeError usd_bridge_write_point_instancer(
    UsdBridgeEditLayer* layer,
    const char* prim_path,
    const float* positions,
    const float* orientations,
    const float* scales,
    const int32_t* proto_indices,
    size_t count,
    const char* const* prototype_paths,
    size_t prototype_count
);

// ============================================================================
// Prim Authoring (define prims + set kind)
// ============================================================================

/// Prim specifier: how the prim is defined
typedef enum UsdBridgeSpecifier {
    USD_BRIDGE_SPECIFIER_DEFINE = 0,
    USD_BRIDGE_SPECIFIER_OVER = 1,
} UsdBridgeSpecifier;

/// Model kind for USD's Kind system
typedef enum UsdBridgeKind {
    USD_BRIDGE_KIND_NONE = 0,
    USD_BRIDGE_KIND_COMPONENT = 1,
    USD_BRIDGE_KIND_GROUP = 2,
    USD_BRIDGE_KIND_ASSEMBLY = 3,
    USD_BRIDGE_KIND_SUBCOMPONENT = 4,
} UsdBridgeKind;

/// Define or override a prim at the given path.
///
/// @param layer Edit layer handle
/// @param prim_path USD prim path (e.g., "/shot")
/// @param type_name Type name (e.g., "Scope", "Xform", "" for typeless)
/// @param specifier 0=Define, 1=Over
/// @return USD_BRIDGE_SUCCESS on success
UsdBridgeError usd_bridge_define_prim(
    UsdBridgeEditLayer* layer,
    const char* prim_path,
    const char* type_name,
    UsdBridgeSpecifier specifier
);

/// Write a UsdGeomMesh prim with positions, indices, optional normals and UVs.
///
/// @param layer Edit layer handle
/// @param prim_path USD prim path for the Mesh
/// @param points Flat array of float[3] per vertex (point_count * 3 floats)
/// @param point_count Number of vertices
/// @param indices Flat triangle indices (index_count must be multiple of 3)
/// @param index_count Number of indices
/// @param normals Flat array of float[3] per vertex (NULL to skip)
/// @param normal_count Number of normals (0 to skip)
/// @param uvs Flat array of float[2] per vertex (NULL to skip)
/// @param uv_count Number of UV coords (0 to skip)
/// @return USD_BRIDGE_SUCCESS on success
UsdBridgeError usd_bridge_write_mesh(
    UsdBridgeEditLayer* layer,
    const char* prim_path,
    const float* points,
    size_t point_count,
    const uint32_t* indices,
    size_t index_count,
    const float* normals,
    size_t normal_count,
    const float* uvs,
    size_t uv_count
);

/// Set the model kind on a prim via UsdModelAPI.
///
/// @param layer Edit layer handle
/// @param prim_path USD prim path
/// @param kind Kind value (0=none, 1=component, 2=group, 3=assembly, 4=subcomponent)
/// @return USD_BRIDGE_SUCCESS on success
UsdBridgeError usd_bridge_set_prim_kind(
    UsdBridgeEditLayer* layer,
    const char* prim_path,
    UsdBridgeKind kind
);

/// Add a payload arc on an edit layer prim.
UsdBridgeError usd_bridge_edit_layer_add_payload(
    UsdBridgeEditLayer* layer,
    const char* prim_path,
    const char* asset_path,
    const char* target_path
);

// ============================================================================
// Material Export
// ============================================================================

/// Write a UsdPreviewSurface material with optional texture connections.
/// Creates Material -> Shader (UsdPreviewSurface) network.
/// Texture paths may be NULL to skip texture connections.
///
/// @param layer Edit layer handle
/// @param mat_path Material prim path (e.g., "/World/Looks/Mat_0")
/// @param diffuse_color RGB diffuse color (3 floats)
/// @param metallic Metallic factor (0-1)
/// @param roughness Roughness factor (0-1)
/// @param specular Specular factor (0-1)
/// @param opacity Opacity (0-1)
/// @param emissive_color RGB emissive color (3 floats)
/// @param diffuse_tex Diffuse texture path (NULL to skip)
/// @param roughness_tex Roughness texture path (NULL to skip)
/// @param metallic_tex Metallic texture path (NULL to skip)
/// @param normal_tex Normal map texture path (NULL to skip)
/// @param emissive_tex Emissive texture path (NULL to skip)
/// @return USD_BRIDGE_SUCCESS on success
UsdBridgeError usd_bridge_write_material(
    UsdBridgeEditLayer* layer,
    const char* mat_path,
    const float* diffuse_color,
    float metallic,
    float roughness,
    float specular,
    float opacity,
    const float* emissive_color,
    const char* diffuse_tex,
    const char* roughness_tex,
    const char* metallic_tex,
    const char* normal_tex,
    const char* emissive_tex,
    float specular_ior,
    float transmission_weight
);

/// Bind a material to a prim via UsdShadeMaterialBindingAPI.
///
/// @param layer Edit layer handle
/// @param prim_path Target prim path
/// @param material_path Material prim path to bind
/// @return USD_BRIDGE_SUCCESS on success
UsdBridgeError usd_bridge_bind_material(
    UsdBridgeEditLayer* layer,
    const char* prim_path,
    const char* material_path
);

/// Write visibility attribute on a prim.
///
/// @param layer Edit layer handle
/// @param prim_path Target prim path
/// @param visible 1 for "inherited" (visible), 0 for "invisible"
/// @return USD_BRIDGE_SUCCESS on success
UsdBridgeError usd_bridge_write_visibility(
    UsdBridgeEditLayer* layer,
    const char* prim_path,
    int visible
);

/// Set stage metadata (metersPerUnit, upAxis, timeCodesPerSecond).
///
/// @param layer Edit layer handle
/// @param meters_per_unit Scene scale (e.g., 0.01 for cm)
/// @param up_axis Up axis (0=Y, 1=Z)
/// @param time_codes_per_second TCPS (e.g., 24.0)
/// @return USD_BRIDGE_SUCCESS on success
UsdBridgeError usd_bridge_set_stage_metadata(
    UsdBridgeEditLayer* layer,
    double meters_per_unit,
    int up_axis,
    double time_codes_per_second
);

// ============================================================================
// Camera Export
// ============================================================================

/// Write a UsdGeomCamera prim with lens properties and transform.
/// Call multiple times at different time values for animated cameras.
///
/// @param layer Edit layer handle
/// @param path Camera prim path (e.g., "/World/Camera")
/// @param focal_length Focal length in mm
/// @param h_aperture Horizontal aperture in mm
/// @param v_aperture Vertical aperture in mm
/// @param clip_near Near clip plane in scene units
/// @param clip_far Far clip plane in scene units
/// @param time Time code (-1 for default/static)
/// @param transform Column-major 4x4 world transform (16 floats)
/// @return USD_BRIDGE_SUCCESS on success
UsdBridgeError usd_bridge_write_camera(
    UsdBridgeEditLayer* layer,
    const char* path,
    float focal_length,
    float h_aperture,
    float v_aperture,
    float clip_near,
    float clip_far,
    double time,
    const float* transform
);

// ============================================================================
// Light Export
// ============================================================================

/// Write a UsdLux light prim with type-specific properties and transform.
///
/// @param layer Edit layer handle
/// @param path Light prim path (e.g., "/World/Lights/Key")
/// @param light_type Light type (0=Distant, 1=Sphere, 2=Rect, 3=Dome, 4=Cylinder, 5=Disk)
/// @param color RGB color (3 floats)
/// @param intensity Light intensity
/// @param exposure Exposure (power of 2 multiplier)
/// @param transform Column-major 4x4 world transform (16 floats)
/// @param angle Distant light angle in degrees
/// @param radius Sphere/Cylinder/Disk light radius
/// @param width Rect light width
/// @param height Rect light height
/// @param length Cylinder light length
/// @param texture_path Dome light texture (NULL to skip)
/// @param shaping_cone_angle ShapingAPI cone angle (0 to skip)
/// @param shaping_cone_softness ShapingAPI cone softness
/// @param shaping_focus ShapingAPI focus
/// @return USD_BRIDGE_SUCCESS on success
UsdBridgeError usd_bridge_write_light(
    UsdBridgeEditLayer* layer,
    const char* path,
    UsdBridgeLightType light_type,
    const float* color,
    float intensity,
    float exposure,
    const float* transform,
    float angle,
    float radius,
    float width,
    float height,
    float length,
    const char* texture_path,
    float shaping_cone_angle,
    float shaping_cone_softness,
    float shaping_focus
);

// ============================================================================
// Render Settings Export
// ============================================================================

/// Write a UsdRenderSettings prim.
///
/// @param layer Edit layer handle
/// @param path RenderSettings prim path (e.g., "/Render/Settings")
/// @param resolution_x Horizontal resolution in pixels
/// @param resolution_y Vertical resolution in pixels
/// @param camera_path Path to the render camera (NULL to skip)
/// @param pixel_aspect_ratio Pixel aspect ratio (1.0 for square pixels)
/// @return USD_BRIDGE_SUCCESS on success
UsdBridgeError usd_bridge_write_render_settings(
    UsdBridgeEditLayer* layer,
    const char* path,
    int resolution_x,
    int resolution_y,
    const char* camera_path,
    float pixel_aspect_ratio
);

// ============================================================================
// GeomSubset Export
// ============================================================================

/// Write a UsdGeomSubset child prim for per-face material assignment.
///
/// @param layer Edit layer handle
/// @param mesh_path Parent mesh prim path
/// @param subset_name Name for the subset prim (e.g., "mat_0")
/// @param face_indices Array of face indices belonging to this subset
/// @param face_count Number of face indices
/// @param material_path Material to bind to this subset (NULL to skip binding)
/// @return USD_BRIDGE_SUCCESS on success
UsdBridgeError usd_bridge_write_geom_subset(
    UsdBridgeEditLayer* layer,
    const char* mesh_path,
    const char* subset_name,
    const int* face_indices,
    size_t face_count,
    const char* material_path
);

// ============================================================================
// PointInstancer InvisibleIds Export
// ============================================================================

/// Write invisibleIds attribute on a UsdGeomPointInstancer.
///
/// @param layer Edit layer handle
/// @param instancer_path PointInstancer prim path
/// @param ids Array of instance IDs to hide
/// @param count Number of IDs
/// @return USD_BRIDGE_SUCCESS on success
UsdBridgeError usd_bridge_write_invisible_ids(
    UsdBridgeEditLayer* layer,
    const char* instancer_path,
    const int64_t* ids,
    size_t count
);

#ifdef __cplusplus
}
#endif

#endif // USD_BRIDGE_H
