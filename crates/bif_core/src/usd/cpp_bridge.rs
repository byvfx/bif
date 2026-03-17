//! USD C++ Bridge - Rust FFI wrapper.
//!
//! Provides safe Rust bindings to the USD C++ library via the usd_bridge C shim.
//! Supports loading USDA, USD, and USDC files with automatic reference resolution.
//!
//! # Example
//!
//! ```ignore
//! use bif_core::usd::cpp_bridge::UsdStage;
//!
//! let stage = UsdStage::open("scene.usdc")?;
//! for mesh in stage.meshes() {
//!     println!("Mesh: {} with {} vertices", mesh.path, mesh.vertices.len());
//! }
//! ```

use std::ffi::{CStr, CString};
use std::path::Path;
use std::ptr;

use bif_math::{Mat4, Vec3};
use thiserror::Error;

// ============================================================================
// FFI Declarations
// ============================================================================

/// Opaque stage handle (matches C struct)
#[repr(C)]
struct UsdBridgeStageRaw {
    _private: [u8; 0],
}

/// Opaque edit layer handle (matches C struct)
#[repr(C)]
struct UsdBridgeEditLayerRaw {
    _private: [u8; 0],
}

/// Error codes from C API
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum UsdBridgeErrorCode {
    Success = 0,
    NullPointer = 1,
    FileNotFound = 2,
    InvalidStage = 3,
    InvalidPrim = 4,
    OutOfMemory = 5,
    Unknown = 99,
}

/// Mesh purpose from C API
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum UsdBridgePurposeRaw {
    Default = 0,
    Render = 1,
    Proxy = 2,
    Guide = 3,
}

/// Mesh data from C API
#[repr(C)]
struct UsdBridgeMeshDataRaw {
    path: *const std::ffi::c_char,
    vertices: *const f32,
    vertex_count: usize,
    indices: *const u32,
    index_count: usize,
    normals: *const f32,
    normal_count: usize,
    uvs: *const f32,
    uv_count: usize,
    face_material_ids: *const u32,
    triangle_count: usize,
    transform: [f32; 16],
    purpose: UsdBridgePurposeRaw,
    is_instance_proxy: i32,
    visibility: i32,
    double_sided: i32,
    subdivision_scheme: *const std::ffi::c_char,
    normals_interpolation: i32,
    display_color: *const f32,
    display_color_count: usize,
    display_opacity: f32,
    resets_xform_stack: i32,
    face_vertex_counts: *const i32,
    face_count: usize,
    face_vertex_indices: *const i32,
    face_vertex_index_count: usize,
    crease_indices: *const i32,
    crease_index_count: usize,
    crease_lengths: *const i32,
    crease_length_count: usize,
    crease_sharpnesses: *const f32,
    crease_sharpness_count: usize,
}

/// Native instance data from C API
#[repr(C)]
struct UsdNativeInstanceDataRaw {
    proto_mesh_idx: i32,
    transform: [f32; 16],
    material_override_idx: i32,
}

/// Instancer data from C API
#[repr(C)]
struct UsdBridgeInstancerDataRaw {
    path: *const std::ffi::c_char,
    prototype_paths: *const *const std::ffi::c_char,
    prototype_count: usize,
    transforms: *const f32,
    instance_count: usize,
    proto_indices: *const i32,
    velocities: *const f32,
    velocity_count: usize,
    angular_velocities: *const f32,
    angular_velocity_count: usize,
    invisible_ids: *const i64,
    invisible_id_count: usize,
}

/// Prim info from C API (for scene browser)
#[repr(C)]
struct UsdBridgePrimInfoRaw {
    path: *const std::ffi::c_char,
    type_name: *const std::ffi::c_char,
    is_active: i32,
    has_children: i32,
    child_count: usize,
    visibility: i32,
    has_payload: i32,
    is_loaded: i32,
    variant_set_count: usize,
    has_inherits: i32,
    has_specializes: i32,
}

/// Timeline data from C API
#[repr(C)]
struct UsdBridgeTimelineDataRaw {
    start_time_code: f64,
    end_time_code: f64,
    frames_per_second: f64,
    has_authored_time_range: i32,
}

/// A single transform sample at a specific time
#[repr(C)]
#[derive(Clone, Copy)]
struct UsdBridgeXformSampleRaw {
    time: f64,
    transform: [f32; 16],
}

/// Animated mesh data from C API
#[repr(C)]
struct UsdBridgeAnimatedMeshDataRaw {
    mesh_index: usize,
    xform_samples: *const UsdBridgeXformSampleRaw,
    xform_sample_count: usize,
}

/// Animated instancer data from C API
#[repr(C)]
struct UsdBridgeAnimatedInstancerDataRaw {
    instancer_index: usize,
    time_samples: *const f64,
    time_sample_count: usize,
    instance_count: usize,
    transforms: *const f32,
}

/// Vertex animation info from C API
#[repr(C)]
struct UsdBridgeVertexAnimationInfoRaw {
    has_animated_vertices: i32,
    time_sample_count: usize,
    time_samples: *const f64,
}

/// Camera properties from C API
#[repr(C)]
struct UsdBridgeCameraPropertiesRaw {
    focal_length: f32,
    vertical_aperture: f32,
    clip_near: f32,
    clip_far: f32,
    horizontal_aperture: f32,
}

/// Light type enumeration from C API
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsdBridgeLightType {
    Distant = 0,
    Sphere = 1,
    Rect = 2,
    Dome = 3,
    Cylinder = 4,
    Disk = 5,
}

/// Light data from C API
#[repr(C)]
struct UsdBridgeLightDataRaw {
    path: *const std::ffi::c_char,
    light_type: UsdBridgeLightType,
    color: [f32; 3],
    intensity: f32,
    exposure: f32,
    transform: [f32; 16],
    angle: f32,
    radius: f32,
    width: f32,
    height: f32,
    texture_path: *const std::ffi::c_char,
    length: f32,
    shaping_cone_angle: f32,
    shaping_cone_softness: f32,
    shaping_focus: f32,
    shaping_ies_file: *const std::ffi::c_char,
    light_link_includes: *const *const std::ffi::c_char,
    light_link_include_count: usize,
    light_link_excludes: *const *const std::ffi::c_char,
    light_link_exclude_count: usize,
}

/// Skeleton data from C API
#[repr(C)]
struct UsdBridgeSkeletonDataRaw {
    path: *const std::ffi::c_char,
    joint_paths: *const *const std::ffi::c_char,
    joint_count: usize,
    bind_transforms: *const f32,
    rest_transforms: *const f32,
}

/// Skin binding data from C API
#[repr(C)]
struct UsdBridgeSkinBindingDataRaw {
    mesh_path: *const std::ffi::c_char,
    skeleton_path: *const std::ffi::c_char,
    joint_indices: *const i32,
    joint_indices_count: usize,
    joint_weights: *const f32,
    joint_weights_count: usize,
    joint_indices_element_size: usize,
    geom_bind_transform: [f32; 16],
}

/// Volume data from C API
#[repr(C)]
struct UsdBridgeVolumeDataRaw {
    path: *const std::ffi::c_char,
    vdb_file_path: *const std::ffi::c_char,
    field_name: *const std::ffi::c_char,
    transform: [f32; 16],
}

/// Points data from C API (UsdGeomPoints)
#[repr(C)]
struct UsdBridgePointsDataRaw {
    path: *const std::ffi::c_char,
    positions: *const f32,
    point_count: usize,
    widths: *const f32,
    width_count: usize,
    normals: *const f32,
    normal_count: usize,
    ids: *const i64,
    id_count: usize,
    transform: [f32; 16],
}

/// Primvar data type from C API
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum UsdBridgePrimvarTypeRaw {
    Float = 0,
    Float2 = 1,
    Float3 = 2,
    Int = 3,
}

/// Primvar interpolation from C API
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum UsdBridgePrimvarInterpolationRaw {
    Constant = 0,
    Uniform = 1,
    Vertex = 2,
    FaceVarying = 3,
}

/// Primvar data from C API
#[repr(C)]
struct UsdBridgePrimvarDataRaw {
    name: *const std::ffi::c_char,
    primvar_type: UsdBridgePrimvarTypeRaw,
    interpolation: UsdBridgePrimvarInterpolationRaw,
    float_data: *const f32,
    int_data: *const i32,
    element_count: usize,
}

/// Curve type from C API
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum UsdBridgeCurveTypeRaw {
    Linear = 0,
    Cubic = 1,
}

/// Curve basis from C API
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum UsdBridgeCurveBasisRaw {
    Bezier = 0,
    Bspline = 1,
    CatmullRom = 2,
}

/// Curve wrap from C API
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum UsdBridgeCurveWrapRaw {
    Nonperiodic = 0,
    Periodic = 1,
    Pinned = 2,
}

/// BasisCurves data from C API
#[repr(C)]
struct UsdBridgeCurvesDataRaw {
    path: *const std::ffi::c_char,
    points: *const f32,
    point_count: usize,
    widths: *const f32,
    width_count: usize,
    curve_vertex_counts: *const i32,
    curve_count: usize,
    curve_type: UsdBridgeCurveTypeRaw,
    basis: UsdBridgeCurveBasisRaw,
    wrap: UsdBridgeCurveWrapRaw,
    transform: [f32; 16],
}

/// Up axis value from C API (populated by FFI)
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum UsdBridgeUpAxisRaw {
    Y = 0,
    Z = 1,
}

/// Stage metadata from C API
#[repr(C)]
struct UsdBridgeStageMetadataRaw {
    meters_per_unit: f64,
    up_axis: UsdBridgeUpAxisRaw,
    time_codes_per_second: f64,
}

/// Material data from C API (UsdPreviewSurface or MaterialX)
#[repr(C)]
struct UsdBridgeMaterialDataRaw {
    path: *const std::ffi::c_char,
    diffuse_color: [f32; 3],
    metallic: f32,
    roughness: f32,
    specular: f32,
    opacity: f32,
    emissive_color: [f32; 3],
    diffuse_texture: *const std::ffi::c_char,
    roughness_texture: *const std::ffi::c_char,
    metallic_texture: *const std::ffi::c_char,
    normal_texture: *const std::ffi::c_char,
    emissive_texture: *const std::ffi::c_char,
    is_materialx: i32,
}

/// Prim specifier from C API
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UsdBridgeSpecifierRaw {
    Define = 0,
    Over = 1,
}

/// Model kind from C API
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UsdBridgeKindRaw {
    None = 0,
    Component = 1,
    Group = 2,
    Assembly = 3,
    Subcomponent = 4,
}

#[link(name = "usd_bridge")]
extern "C" {
    fn usd_bridge_error_message(error: UsdBridgeErrorCode) -> *const std::ffi::c_char;

    fn usd_bridge_open_stage(
        path: *const std::ffi::c_char,
        out_stage: *mut *mut UsdBridgeStageRaw,
    ) -> UsdBridgeErrorCode;

    fn usd_bridge_close_stage(stage: *mut UsdBridgeStageRaw);

    fn usd_bridge_get_mesh_count(
        stage: *const UsdBridgeStageRaw,
        out_count: *mut usize,
    ) -> UsdBridgeErrorCode;

    fn usd_bridge_get_instancer_count(
        stage: *const UsdBridgeStageRaw,
        out_count: *mut usize,
    ) -> UsdBridgeErrorCode;

    fn usd_bridge_get_mesh(
        stage: *const UsdBridgeStageRaw,
        index: usize,
        out_data: *mut UsdBridgeMeshDataRaw,
    ) -> UsdBridgeErrorCode;

    fn usd_bridge_get_instancer(
        stage: *const UsdBridgeStageRaw,
        index: usize,
        out_data: *mut UsdBridgeInstancerDataRaw,
    ) -> UsdBridgeErrorCode;

    fn usd_bridge_get_native_instance_count(
        stage: *const UsdBridgeStageRaw,
        out_count: *mut usize,
    ) -> UsdBridgeErrorCode;

    fn usd_bridge_get_native_instance(
        stage: *const UsdBridgeStageRaw,
        index: usize,
        out_data: *mut UsdNativeInstanceDataRaw,
    ) -> UsdBridgeErrorCode;

    fn usd_bridge_export_stage(
        stage: *const UsdBridgeStageRaw,
        path: *const std::ffi::c_char,
    ) -> UsdBridgeErrorCode;

    // Prim traversal APIs (for scene browser)
    fn usd_bridge_get_prim_count(
        stage: *const UsdBridgeStageRaw,
        out_count: *mut usize,
    ) -> UsdBridgeErrorCode;

    fn usd_bridge_get_prim_info(
        stage: *const UsdBridgeStageRaw,
        index: usize,
        out_info: *mut UsdBridgePrimInfoRaw,
    ) -> UsdBridgeErrorCode;

    fn usd_bridge_get_root_prim_count(
        stage: *const UsdBridgeStageRaw,
        out_count: *mut usize,
    ) -> UsdBridgeErrorCode;

    fn usd_bridge_get_root_prim_path(
        stage: *const UsdBridgeStageRaw,
        index: usize,
        out_path: *mut *const std::ffi::c_char,
    ) -> UsdBridgeErrorCode;

    fn usd_bridge_get_children_count(
        stage: *const UsdBridgeStageRaw,
        parent_path: *const std::ffi::c_char,
        out_count: *mut usize,
    ) -> UsdBridgeErrorCode;

    fn usd_bridge_get_child_path(
        stage: *const UsdBridgeStageRaw,
        parent_path: *const std::ffi::c_char,
        index: usize,
        out_path: *mut *const std::ffi::c_char,
    ) -> UsdBridgeErrorCode;

    fn usd_bridge_get_prim_info_by_path(
        stage: *const UsdBridgeStageRaw,
        path: *const std::ffi::c_char,
        out_info: *mut UsdBridgePrimInfoRaw,
    ) -> UsdBridgeErrorCode;

    // Material APIs
    fn usd_bridge_get_material_count(
        stage: *const UsdBridgeStageRaw,
        out_count: *mut usize,
    ) -> UsdBridgeErrorCode;

    fn usd_bridge_get_material(
        stage: *const UsdBridgeStageRaw,
        index: usize,
        out_data: *mut UsdBridgeMaterialDataRaw,
    ) -> UsdBridgeErrorCode;

    fn usd_bridge_get_mesh_material_path(
        stage: *const UsdBridgeStageRaw,
        mesh_index: usize,
        out_path: *mut *const std::ffi::c_char,
    ) -> UsdBridgeErrorCode;

    // Timeline APIs
    fn usd_bridge_get_timeline(
        stage: *const UsdBridgeStageRaw,
        out_data: *mut UsdBridgeTimelineDataRaw,
    ) -> UsdBridgeErrorCode;

    // Stage metadata APIs
    fn usd_bridge_get_stage_metadata(
        stage: *const UsdBridgeStageRaw,
        out_data: *mut UsdBridgeStageMetadataRaw,
    ) -> UsdBridgeErrorCode;

    // Animation APIs
    fn usd_bridge_get_mesh_animation(
        stage: *const UsdBridgeStageRaw,
        mesh_index: usize,
        out_data: *mut UsdBridgeAnimatedMeshDataRaw,
    ) -> UsdBridgeErrorCode;

    fn usd_bridge_get_instancer_animation(
        stage: *const UsdBridgeStageRaw,
        instancer_index: usize,
        out_data: *mut UsdBridgeAnimatedInstancerDataRaw,
    ) -> UsdBridgeErrorCode;

    fn usd_bridge_get_camera_xform_samples(
        stage: *const UsdBridgeStageRaw,
        camera_path: *const std::ffi::c_char,
        out_samples: *mut *const UsdBridgeXformSampleRaw,
        out_count: *mut usize,
    ) -> UsdBridgeErrorCode;

    fn usd_bridge_get_camera_count(
        stage: *const UsdBridgeStageRaw,
        out_count: *mut usize,
    ) -> UsdBridgeErrorCode;

    fn usd_bridge_get_camera_path(
        stage: *const UsdBridgeStageRaw,
        index: usize,
        out_path: *mut *const std::ffi::c_char,
    ) -> UsdBridgeErrorCode;

    fn usd_bridge_get_camera_xform_at_time(
        stage: *const UsdBridgeStageRaw,
        camera_path: *const std::ffi::c_char,
        time: f64,
        out_transform: *mut f32,
    ) -> UsdBridgeErrorCode;

    fn usd_bridge_get_camera_properties(
        stage: *const UsdBridgeStageRaw,
        camera_path: *const std::ffi::c_char,
        time: f64,
        out_props: *mut UsdBridgeCameraPropertiesRaw,
    ) -> UsdBridgeErrorCode;

    fn usd_bridge_get_mesh_vertex_animation_info(
        stage: *const UsdBridgeStageRaw,
        mesh_index: usize,
        out_info: *mut UsdBridgeVertexAnimationInfoRaw,
    ) -> UsdBridgeErrorCode;

    fn usd_bridge_get_mesh_vertices_at_time(
        stage: *const UsdBridgeStageRaw,
        mesh_index: usize,
        time: f64,
        out_vertices: *mut *const f32,
        out_vertex_count: *mut usize,
    ) -> UsdBridgeErrorCode;

    // Light APIs
    fn usd_bridge_get_light_count(
        stage: *const UsdBridgeStageRaw,
        out_count: *mut usize,
    ) -> UsdBridgeErrorCode;

    fn usd_bridge_get_light(
        stage: *const UsdBridgeStageRaw,
        index: usize,
        out_data: *mut UsdBridgeLightDataRaw,
    ) -> UsdBridgeErrorCode;

    // Points APIs (UsdGeomPoints)
    fn usd_bridge_get_points_count(
        stage: *const UsdBridgeStageRaw,
        out_count: *mut usize,
    ) -> UsdBridgeErrorCode;

    fn usd_bridge_get_points(
        stage: *const UsdBridgeStageRaw,
        index: usize,
        out_data: *mut UsdBridgePointsDataRaw,
    ) -> UsdBridgeErrorCode;

    // Primvar query APIs
    fn usd_bridge_get_mesh_primvar_count(
        stage: *const UsdBridgeStageRaw,
        mesh_index: usize,
        out_count: *mut usize,
    ) -> UsdBridgeErrorCode;

    fn usd_bridge_get_mesh_primvar(
        stage: *const UsdBridgeStageRaw,
        mesh_index: usize,
        primvar_index: usize,
        out_data: *mut UsdBridgePrimvarDataRaw,
    ) -> UsdBridgeErrorCode;

    // Edit layer export
    fn usd_bridge_create_edit_layer(
        output_path: *const std::ffi::c_char,
        out_layer: *mut *mut UsdBridgeEditLayerRaw,
    ) -> UsdBridgeErrorCode;

    fn usd_bridge_write_xform_opinion(
        layer: *mut UsdBridgeEditLayerRaw,
        prim_path: *const std::ffi::c_char,
        time: f64,
        matrix_16: *const f32,
    ) -> UsdBridgeErrorCode;

    fn usd_bridge_save_edit_layer(layer: *mut UsdBridgeEditLayerRaw) -> UsdBridgeErrorCode;

    fn usd_bridge_free_edit_layer(layer: *mut UsdBridgeEditLayerRaw);

    fn usd_bridge_edit_layer_add_sublayer(
        layer: *mut UsdBridgeEditLayerRaw,
        sublayer_path: *const std::ffi::c_char,
    ) -> UsdBridgeErrorCode;

    fn usd_bridge_edit_layer_add_reference(
        layer: *mut UsdBridgeEditLayerRaw,
        prim_path: *const std::ffi::c_char,
        reference_file: *const std::ffi::c_char,
        reference_prim_path: *const std::ffi::c_char,
    ) -> UsdBridgeErrorCode;

    fn usd_bridge_edit_layer_set_default_prim(
        layer: *mut UsdBridgeEditLayerRaw,
        prim_path: *const std::ffi::c_char,
    ) -> UsdBridgeErrorCode;

    fn usd_bridge_write_point_instancer(
        layer: *mut UsdBridgeEditLayerRaw,
        prim_path: *const std::ffi::c_char,
        positions: *const f32,
        orientations: *const f32,
        scales: *const f32,
        proto_indices: *const i32,
        count: usize,
        prototype_paths: *const *const std::ffi::c_char,
        prototype_count: usize,
    ) -> UsdBridgeErrorCode;

    // Mesh authoring
    fn usd_bridge_write_mesh(
        layer: *mut UsdBridgeEditLayerRaw,
        prim_path: *const std::ffi::c_char,
        points: *const f32,
        point_count: usize,
        indices: *const u32,
        index_count: usize,
        normals: *const f32,
        normal_count: usize,
        uvs: *const f32,
        uv_count: usize,
    ) -> UsdBridgeErrorCode;

    // Prim authoring
    fn usd_bridge_define_prim(
        layer: *mut UsdBridgeEditLayerRaw,
        prim_path: *const std::ffi::c_char,
        type_name: *const std::ffi::c_char,
        specifier: UsdBridgeSpecifierRaw,
    ) -> UsdBridgeErrorCode;

    fn usd_bridge_set_prim_kind(
        layer: *mut UsdBridgeEditLayerRaw,
        prim_path: *const std::ffi::c_char,
        kind: UsdBridgeKindRaw,
    ) -> UsdBridgeErrorCode;

    // Material export
    fn usd_bridge_write_material(
        layer: *mut UsdBridgeEditLayerRaw,
        mat_path: *const std::ffi::c_char,
        diffuse_color: *const f32,
        metallic: f32,
        roughness: f32,
        specular: f32,
        opacity: f32,
        emissive_color: *const f32,
        diffuse_tex: *const std::ffi::c_char,
        roughness_tex: *const std::ffi::c_char,
        metallic_tex: *const std::ffi::c_char,
        normal_tex: *const std::ffi::c_char,
        emissive_tex: *const std::ffi::c_char,
    ) -> UsdBridgeErrorCode;

    fn usd_bridge_bind_material(
        layer: *mut UsdBridgeEditLayerRaw,
        prim_path: *const std::ffi::c_char,
        material_path: *const std::ffi::c_char,
    ) -> UsdBridgeErrorCode;

    fn usd_bridge_write_visibility(
        layer: *mut UsdBridgeEditLayerRaw,
        prim_path: *const std::ffi::c_char,
        visible: i32,
    ) -> UsdBridgeErrorCode;

    fn usd_bridge_set_stage_metadata(
        layer: *mut UsdBridgeEditLayerRaw,
        meters_per_unit: f64,
        up_axis: i32,
        time_codes_per_second: f64,
    ) -> UsdBridgeErrorCode;

    // Camera export
    fn usd_bridge_write_camera(
        layer: *mut UsdBridgeEditLayerRaw,
        path: *const std::ffi::c_char,
        focal_length: f32,
        h_aperture: f32,
        v_aperture: f32,
        clip_near: f32,
        clip_far: f32,
        time: f64,
        transform: *const f32,
    ) -> UsdBridgeErrorCode;

    // Light export
    fn usd_bridge_write_light(
        layer: *mut UsdBridgeEditLayerRaw,
        path: *const std::ffi::c_char,
        light_type: UsdBridgeLightType,
        color: *const f32,
        intensity: f32,
        exposure: f32,
        transform: *const f32,
        angle: f32,
        radius: f32,
        width: f32,
        height: f32,
        length: f32,
        texture_path: *const std::ffi::c_char,
        shaping_cone_angle: f32,
        shaping_cone_softness: f32,
        shaping_focus: f32,
    ) -> UsdBridgeErrorCode;

    // Render settings export
    fn usd_bridge_write_render_settings(
        layer: *mut UsdBridgeEditLayerRaw,
        path: *const std::ffi::c_char,
        resolution_x: i32,
        resolution_y: i32,
        camera_path: *const std::ffi::c_char,
        pixel_aspect_ratio: f32,
    ) -> UsdBridgeErrorCode;

    // Payload
    fn usd_bridge_load_payload(
        stage: *mut UsdBridgeStageRaw,
        prim_path: *const std::ffi::c_char,
    ) -> UsdBridgeErrorCode;

    fn usd_bridge_unload_payload(
        stage: *mut UsdBridgeStageRaw,
        prim_path: *const std::ffi::c_char,
    ) -> UsdBridgeErrorCode;

    fn usd_bridge_edit_layer_add_payload(
        layer: *mut UsdBridgeEditLayerRaw,
        prim_path: *const std::ffi::c_char,
        asset_path: *const std::ffi::c_char,
        target_path: *const std::ffi::c_char,
    ) -> UsdBridgeErrorCode;

    // Variant query/selection
    fn usd_bridge_get_variant_set_count(
        stage: *const UsdBridgeStageRaw,
        prim_path: *const std::ffi::c_char,
        out_count: *mut usize,
    ) -> UsdBridgeErrorCode;

    fn usd_bridge_get_variant_set_name(
        stage: *const UsdBridgeStageRaw,
        prim_path: *const std::ffi::c_char,
        index: usize,
        out_name: *mut *const std::ffi::c_char,
    ) -> UsdBridgeErrorCode;

    fn usd_bridge_get_variant_count(
        stage: *const UsdBridgeStageRaw,
        prim_path: *const std::ffi::c_char,
        variant_set_name: *const std::ffi::c_char,
        out_count: *mut usize,
    ) -> UsdBridgeErrorCode;

    fn usd_bridge_get_variant_name(
        stage: *const UsdBridgeStageRaw,
        prim_path: *const std::ffi::c_char,
        variant_set_name: *const std::ffi::c_char,
        index: usize,
        out_name: *mut *const std::ffi::c_char,
    ) -> UsdBridgeErrorCode;

    fn usd_bridge_get_variant_selection(
        stage: *const UsdBridgeStageRaw,
        prim_path: *const std::ffi::c_char,
        variant_set_name: *const std::ffi::c_char,
        out_selection: *mut *const std::ffi::c_char,
    ) -> UsdBridgeErrorCode;

    fn usd_bridge_set_variant_selection(
        stage: *mut UsdBridgeStageRaw,
        prim_path: *const std::ffi::c_char,
        variant_set_name: *const std::ffi::c_char,
        variant_name: *const std::ffi::c_char,
    ) -> UsdBridgeErrorCode;

    // BasisCurves
    fn usd_bridge_get_curves_count(
        stage: *const UsdBridgeStageRaw,
        out_count: *mut usize,
    ) -> UsdBridgeErrorCode;

    fn usd_bridge_get_curves(
        stage: *const UsdBridgeStageRaw,
        index: usize,
        out_data: *mut UsdBridgeCurvesDataRaw,
    ) -> UsdBridgeErrorCode;

    // Skeleton
    fn usd_bridge_get_skeleton_count(
        stage: *const UsdBridgeStageRaw,
        out_count: *mut usize,
    ) -> UsdBridgeErrorCode;

    fn usd_bridge_get_skeleton(
        stage: *const UsdBridgeStageRaw,
        index: usize,
        out_data: *mut UsdBridgeSkeletonDataRaw,
    ) -> UsdBridgeErrorCode;

    fn usd_bridge_get_skin_binding(
        stage: *const UsdBridgeStageRaw,
        mesh_index: usize,
        out_data: *mut UsdBridgeSkinBindingDataRaw,
    ) -> UsdBridgeErrorCode;

    // Volumes
    fn usd_bridge_get_volume_count(
        stage: *const UsdBridgeStageRaw,
        out_count: *mut usize,
    ) -> UsdBridgeErrorCode;

    fn usd_bridge_get_volume(
        stage: *const UsdBridgeStageRaw,
        index: usize,
        out_data: *mut UsdBridgeVolumeDataRaw,
    ) -> UsdBridgeErrorCode;

    // Collection material binding handled by existing get_mesh_material_path
    // (ComputeBoundMaterial resolves both direct and collection-based bindings)
}

// ============================================================================
// Error Types
// ============================================================================

/// Errors from USD bridge operations.
#[derive(Error, Debug)]
pub enum UsdBridgeError {
    #[error("Null pointer passed to USD bridge")]
    NullPointer,

    #[error("USD file not found: {0}")]
    FileNotFound(String),

    #[error("Invalid USD stage handle")]
    InvalidStage,

    #[error("Invalid prim or index: {0}")]
    InvalidPrim(String),

    #[error("Out of memory")]
    OutOfMemory,

    #[error("USD bridge error: {0}")]
    Unknown(String),

    #[error("Path contains invalid UTF-8")]
    InvalidPath,
}

impl From<UsdBridgeErrorCode> for UsdBridgeError {
    fn from(code: UsdBridgeErrorCode) -> Self {
        match code {
            UsdBridgeErrorCode::Success => unreachable!("Success is not an error"),
            UsdBridgeErrorCode::NullPointer => UsdBridgeError::NullPointer,
            UsdBridgeErrorCode::FileNotFound => UsdBridgeError::FileNotFound(String::new()),
            UsdBridgeErrorCode::InvalidStage => UsdBridgeError::InvalidStage,
            UsdBridgeErrorCode::InvalidPrim => UsdBridgeError::InvalidPrim(String::new()),
            UsdBridgeErrorCode::OutOfMemory => UsdBridgeError::OutOfMemory,
            UsdBridgeErrorCode::Unknown => {
                let msg = unsafe {
                    let ptr = usd_bridge_error_message(code);
                    if ptr.is_null() {
                        "Unknown error".to_string()
                    } else {
                        CStr::from_ptr(ptr).to_string_lossy().into_owned()
                    }
                };
                UsdBridgeError::Unknown(msg)
            }
        }
    }
}

pub type UsdBridgeResult<T> = Result<T, UsdBridgeError>;

// ============================================================================
// Safe Rust Types
// ============================================================================

/// Subdivision scheme from UsdGeomMesh.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubdivisionScheme {
    /// No subdivision (polygonal mesh)
    None,
    /// Catmull-Clark subdivision
    CatmullClark,
    /// Loop subdivision (triangles only)
    Loop,
    /// Bilinear subdivision
    Bilinear,
}

/// Normals interpolation mode from USD.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NormalsInterpolation {
    /// One normal per vertex (shared across faces)
    Vertex,
    /// One normal per face-vertex (unique per face corner)
    FaceVarying,
    /// One normal per face
    Uniform,
    /// Single normal for entire mesh
    Constant,
}

/// Mesh purpose attribute from USD (UsdGeomImageable).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeshPurpose {
    /// Default purpose (always visible)
    Default,
    /// Render-quality geometry
    Render,
    /// Proxy/low-res geometry (used for viewport preview)
    Proxy,
    /// Guide geometry (helper visualization, usually hidden)
    Guide,
}

/// Mesh data extracted from USD.
#[derive(Clone, Debug)]
pub struct UsdMeshData {
    /// Prim path in the USD hierarchy
    pub path: String,

    /// Vertex positions
    pub vertices: Vec<Vec3>,

    /// Triangle indices
    pub indices: Vec<u32>,

    /// Vertex normals (optional)
    pub normals: Option<Vec<Vec3>>,

    /// UV coordinates (optional, from primvars:st)
    pub uvs: Option<Vec<[f32; 2]>>,

    /// Per-triangle material IDs (from GeomSubsets, optional)
    pub face_material_ids: Option<Vec<u32>>,

    /// World transform matrix
    pub transform: Mat4,

    /// Mesh purpose (default/render/proxy/guide)
    pub purpose: MeshPurpose,

    /// True if this mesh came from a native instance proxy
    pub is_instance_proxy: bool,

    /// Computed visibility (considering inherited visibility)
    pub visible: bool,

    /// Double-sided flag from UsdGeomMesh
    pub double_sided: bool,

    /// Subdivision scheme
    pub subdivision_scheme: SubdivisionScheme,

    /// Normals interpolation mode
    pub normals_interpolation: NormalsInterpolation,

    /// Display color (primvars:displayColor — fallback when no material bound)
    pub display_color: Option<Vec<Vec3>>,

    /// Display opacity (primvars:displayOpacity, default 1.0)
    pub display_opacity: f32,

    /// True if xformOpOrder contains !resetXformStack! (ignore parent transforms)
    pub resets_xform_stack: bool,

    /// Original face vertex counts (polygon topology, for subdivision surfaces)
    pub face_vertex_counts: Option<Vec<i32>>,

    /// Original face vertex indices (polygon topology, for subdivision surfaces)
    pub face_vertex_indices: Option<Vec<i32>>,

    /// Crease edge vertex indices (pairs)
    pub crease_indices: Option<Vec<i32>>,

    /// Crease chain lengths
    pub crease_lengths: Option<Vec<i32>>,

    /// Crease sharpnesses (one per chain)
    pub crease_sharpnesses: Option<Vec<f32>>,
}

/// Native instance data — references a prototype mesh with a unique transform.
#[derive(Clone, Debug)]
pub struct UsdNativeInstance {
    /// Index into the meshes array for prototype geometry
    pub proto_mesh_idx: usize,
    /// World transform
    pub transform: Mat4,
    /// Material override index (-1 = use prototype material)
    pub material_override_idx: i32,
}

/// Point instancer data extracted from USD.
#[derive(Clone, Debug)]
pub struct UsdInstancerData {
    /// Prim path in the USD hierarchy
    pub path: String,

    /// Paths to prototype prims
    pub prototype_paths: Vec<String>,

    /// Instance transforms (world space)
    pub transforms: Vec<Mat4>,

    /// Prototype index for each instance
    pub proto_indices: Vec<i32>,

    /// Velocities (vec3 per instance, for motion blur interpolation)
    pub velocities: Option<Vec<Vec3>>,

    /// Angular velocities (vec3 per instance, for motion blur)
    pub angular_velocities: Option<Vec<Vec3>>,

    /// IDs of invisible instances
    pub invisible_ids: Vec<i64>,
}

/// Prim info for scene hierarchy browsing.
#[derive(Clone, Debug)]
pub struct UsdPrimInfo {
    /// Prim path (e.g., "/World/Mesh")
    pub path: String,

    /// Type name (e.g., "Mesh", "Xform", "PointInstancer")
    pub type_name: String,

    /// Whether prim is active in composed scene
    pub is_active: bool,

    /// Whether prim has children
    pub has_children: bool,

    /// Number of direct children
    pub child_count: usize,

    /// Computed visibility (considering inherited visibility)
    pub visible: bool,

    /// Whether prim has a payload arc
    pub has_payload: bool,

    /// Whether payload is currently loaded
    pub is_loaded: bool,

    /// Number of variant sets
    pub variant_set_count: usize,

    /// Whether prim has inherit arcs
    pub has_inherits: bool,

    /// Whether prim has specializes arcs
    pub has_specializes: bool,
}

/// Material data extracted from USD (UsdPreviewSurface or MaterialX).
#[derive(Clone, Debug)]
pub struct UsdMaterialData {
    /// Material prim path (e.g., "/World/Looks/Material_0")
    pub path: String,

    /// Diffuse/albedo color (RGB, 0-1)
    pub diffuse_color: Vec3,

    /// Metallic factor (0=dielectric, 1=metal)
    pub metallic: f32,

    /// Roughness factor (0=smooth, 1=rough)
    pub roughness: f32,

    /// Specular factor
    pub specular: f32,

    /// Opacity (0=transparent, 1=opaque)
    pub opacity: f32,

    /// Emissive color (RGB)
    pub emissive_color: Vec3,

    /// Path to diffuse texture (if any)
    pub diffuse_texture: Option<String>,

    /// Path to roughness texture (if any)
    pub roughness_texture: Option<String>,

    /// Path to metallic texture (if any)
    pub metallic_texture: Option<String>,

    /// Path to normal map texture (if any)
    pub normal_texture: Option<String>,

    /// Path to emissive texture (if any)
    pub emissive_texture: Option<String>,

    /// True if material is from MaterialX, false for UsdPreviewSurface
    pub is_materialx: bool,
}

/// Light type extracted from USD.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsdLightType {
    /// Directional/distant light (like sun)
    Distant,
    /// Point/sphere light
    Sphere,
    /// Area/rect light
    Rect,
    /// Environment/dome light
    Dome,
    /// Cylinder light
    Cylinder,
    /// Disk light
    Disk,
}

impl From<UsdBridgeLightType> for UsdLightType {
    fn from(t: UsdBridgeLightType) -> Self {
        match t {
            UsdBridgeLightType::Distant => UsdLightType::Distant,
            UsdBridgeLightType::Sphere => UsdLightType::Sphere,
            UsdBridgeLightType::Rect => UsdLightType::Rect,
            UsdBridgeLightType::Dome => UsdLightType::Dome,
            UsdBridgeLightType::Cylinder => UsdLightType::Cylinder,
            UsdBridgeLightType::Disk => UsdLightType::Disk,
        }
    }
}

/// Shaping API data (spotlight cone, IES profiles).
#[derive(Clone, Debug, Default)]
pub struct UsdLightShaping {
    /// Cone angle in degrees (0 = no shaping)
    pub cone_angle: f32,
    /// Cone softness (0-1)
    pub cone_softness: f32,
    /// Focus (0 = uniform)
    pub focus: f32,
    /// IES profile path
    pub ies_file: Option<String>,
}

/// Primvar data type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrimvarType {
    Float,
    Float2,
    Float3,
    Int,
}

/// Primvar interpolation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrimvarInterpolation {
    Constant,
    Uniform,
    Vertex,
    FaceVarying,
}

/// Arbitrary primvar data extracted from USD.
#[derive(Clone, Debug)]
pub struct UsdPrimvarData {
    /// Primvar name
    pub name: String,
    /// Data type
    pub primvar_type: PrimvarType,
    /// Interpolation
    pub interpolation: PrimvarInterpolation,
    /// Float data (for Float/Float2/Float3 types)
    pub float_data: Vec<f32>,
    /// Int data (for Int type)
    pub int_data: Vec<i32>,
    /// Number of elements
    pub element_count: usize,
}

/// UsdGeomPoints data (point cloud / particles).
#[derive(Clone, Debug)]
pub struct UsdPointsData {
    /// Prim path
    pub path: String,
    /// Point positions
    pub positions: Vec<Vec3>,
    /// Per-point widths (optional)
    pub widths: Option<Vec<f32>>,
    /// Per-point normals (optional)
    pub normals: Option<Vec<Vec3>>,
    /// Per-point IDs (optional)
    pub ids: Option<Vec<i64>>,
    /// World transform
    pub transform: Mat4,
}

/// Curve type from USD BasisCurves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CurveType {
    Linear,
    Cubic,
}

/// Curve basis from USD BasisCurves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CurveBasis {
    Bezier,
    Bspline,
    CatmullRom,
}

/// Curve wrap mode from USD BasisCurves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CurveWrap {
    Nonperiodic,
    Periodic,
    Pinned,
}

/// BasisCurves data extracted from USD.
#[derive(Clone, Debug)]
pub struct UsdCurvesData {
    /// Prim path
    pub path: String,
    /// Control point positions
    pub points: Vec<Vec3>,
    /// Per-vertex or per-curve widths
    pub widths: Option<Vec<f32>>,
    /// Vertex counts per curve
    pub curve_vertex_counts: Vec<i32>,
    /// Curve type
    pub curve_type: CurveType,
    /// Curve basis (only meaningful for cubic)
    pub basis: CurveBasis,
    /// Wrap mode
    pub wrap: CurveWrap,
    /// World transform
    pub transform: Mat4,
}

/// Skeleton data extracted from USD (UsdSkelSkeleton).
#[derive(Clone, Debug)]
pub struct UsdSkeletonData {
    /// Prim path
    pub path: String,
    /// Joint paths (topology)
    pub joint_paths: Vec<String>,
    /// Bind transforms (one Mat4 per joint)
    pub bind_transforms: Vec<Mat4>,
    /// Rest transforms (one Mat4 per joint)
    pub rest_transforms: Vec<Mat4>,
}

/// Skin binding data for a skinned mesh.
#[derive(Clone, Debug)]
pub struct UsdSkinBindingData {
    /// Mesh prim path
    pub mesh_path: String,
    /// Skeleton prim path
    pub skeleton_path: String,
    /// Joint indices per vertex (flat, element_size per vertex)
    pub joint_indices: Vec<i32>,
    /// Joint weights per vertex (flat, same layout as indices)
    pub joint_weights: Vec<f32>,
    /// Number of influences per vertex
    pub element_size: usize,
    /// Geom bind transform
    pub geom_bind_transform: Mat4,
}

/// Volume data extracted from USD (UsdVol).
#[derive(Clone, Debug)]
pub struct UsdVolumeData {
    /// Prim path
    pub path: String,
    /// OpenVDB file path
    pub vdb_file_path: Option<String>,
    /// Field name within VDB (e.g., "density")
    pub field_name: Option<String>,
    /// World transform
    pub transform: Mat4,
}

/// USD prim specifier — how the prim opinion is authored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsdSpecifier {
    /// DefinePrim: creates a concrete prim with a type
    Define,
    /// OverridePrim: creates an override opinion (no type required)
    Over,
}

impl UsdSpecifier {
    /// All variants for UI iteration.
    pub const ALL: [Self; 2] = [Self::Define, Self::Over];
}

impl std::fmt::Display for UsdSpecifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Define => write!(f, "Define"),
            Self::Over => write!(f, "Over"),
        }
    }
}

/// USD model kind — used by asset pipelines (UsdModelAPI).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsdKind {
    /// No kind set
    None,
    /// Leaf-level renderable asset
    Component,
    /// Organizational group of assets
    Group,
    /// Top-level publishable asset
    Assembly,
    /// Part of a component (below component level)
    Subcomponent,
}

impl UsdKind {
    /// All variants for UI iteration.
    pub const ALL: [Self; 5] = [
        Self::None,
        Self::Component,
        Self::Group,
        Self::Assembly,
        Self::Subcomponent,
    ];
}

impl std::fmt::Display for UsdKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::None => write!(f, "None"),
            Self::Component => write!(f, "Component"),
            Self::Group => write!(f, "Group"),
            Self::Assembly => write!(f, "Assembly"),
            Self::Subcomponent => write!(f, "Subcomponent"),
        }
    }
}

/// USD prim type for scene assembly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsdPrimType {
    /// No type (typeless prim)
    None,
    /// Scope: organizational container (no transform)
    Scope,
    /// Xform: transformable container
    Xform,
}

impl UsdPrimType {
    /// All variants for UI iteration.
    pub const ALL: [Self; 3] = [Self::None, Self::Scope, Self::Xform];

    /// USD type name string for the C++ bridge.
    pub fn as_usd_type_name(&self) -> &str {
        match self {
            Self::None => "",
            Self::Scope => "Scope",
            Self::Xform => "Xform",
        }
    }
}

impl std::fmt::Display for UsdPrimType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::None => write!(f, "(none)"),
            Self::Scope => write!(f, "Scope"),
            Self::Xform => write!(f, "Xform"),
        }
    }
}

/// Camera lens/clipping properties from UsdGeomCamera.
#[derive(Clone, Debug)]
pub struct CameraProperties {
    /// Focal length in mm
    pub focal_length: f32,
    /// Vertical aperture in mm
    pub vertical_aperture: f32,
    /// Near clipping plane in scene units
    pub clip_near: f32,
    /// Far clipping plane in scene units
    pub clip_far: f32,
    /// Horizontal aperture in mm
    pub horizontal_aperture: f32,
}

impl CameraProperties {
    /// Compute vertical field of view in radians.
    ///
    /// `fov_y = 2 * atan(vertical_aperture / (2 * focal_length))`
    pub fn fov_y(&self) -> f32 {
        if self.focal_length <= 0.0 {
            return 45.0_f32.to_radians();
        }
        2.0 * (self.vertical_aperture / (2.0 * self.focal_length)).atan()
    }

    /// Compute aspect ratio from aperture dimensions.
    ///
    /// `aspect = horizontal_aperture / vertical_aperture`
    pub fn aspect_ratio(&self) -> f32 {
        if self.vertical_aperture <= 0.0 {
            return 16.0 / 9.0;
        }
        self.horizontal_aperture / self.vertical_aperture
    }
}

/// Light data extracted from USD (UsdLux).
#[derive(Clone, Debug)]
pub struct UsdLightData {
    /// Prim path in the USD hierarchy
    pub path: String,

    /// Light type
    pub light_type: UsdLightType,

    /// Light color (RGB, 0-1)
    pub color: Vec3,

    /// Combined intensity: intensity * 2^exposure
    pub intensity: f32,

    /// World transform matrix
    pub transform: Mat4,

    /// Distant light: angular diameter in degrees
    pub angle: f32,

    /// Sphere light: radius
    pub radius: f32,

    /// Rect light: width
    pub width: f32,

    /// Rect light: height
    pub height: f32,

    /// Dome light: texture path (if any)
    pub texture_path: Option<String>,

    /// Cylinder light: length
    pub length: f32,

    /// ShapingAPI data (spotlight cone, IES)
    pub shaping: UsdLightShaping,

    /// Light linking: include paths
    pub light_link_includes: Vec<String>,

    /// Light linking: exclude paths
    pub light_link_excludes: Vec<String>,
}

/// Timeline metadata extracted from USD stage.
#[derive(Clone, Debug)]
pub struct UsdTimelineData {
    /// Start time code (first frame)
    pub start_time_code: f64,

    /// End time code (last frame)
    pub end_time_code: f64,

    /// Frames per second
    pub frames_per_second: f64,

    /// Whether the stage has authored time range metadata
    pub has_authored_time_range: bool,
}

impl Default for UsdTimelineData {
    fn default() -> Self {
        Self {
            start_time_code: 0.0,
            end_time_code: 0.0,
            frames_per_second: 24.0,
            has_authored_time_range: false,
        }
    }
}

/// Up axis of a USD stage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpAxis {
    Y,
    Z,
}

impl std::fmt::Display for UpAxis {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UpAxis::Y => write!(f, "Y-up"),
            UpAxis::Z => write!(f, "Z-up"),
        }
    }
}

/// Stage-level metadata extracted from USD (metersPerUnit, upAxis).
#[derive(Clone, Debug)]
pub struct UsdStageMetadata {
    /// Scene scale: 1.0 = meters, 0.01 = centimeters, etc.
    pub meters_per_unit: f64,
    /// Up axis (Y or Z)
    pub up_axis: UpAxis,
    /// Time codes per second (default 24.0)
    pub time_codes_per_second: f64,
}

impl Default for UsdStageMetadata {
    fn default() -> Self {
        Self {
            meters_per_unit: 1.0,
            up_axis: UpAxis::Y,
            time_codes_per_second: 24.0,
        }
    }
}

impl std::fmt::Display for UsdStageMetadata {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let unit = match self.meters_per_unit {
            v if (v - 0.001).abs() < 1e-6 => "mm",
            v if (v - 0.01).abs() < 1e-6 => "cm",
            v if (v - 0.0254).abs() < 1e-6 => "in",
            v if (v - 0.3048).abs() < 1e-6 => "ft",
            v if (v - 1.0).abs() < 1e-6 => "m",
            _ => "custom",
        };
        write!(
            f,
            "{}, {}, tcps={}",
            self.up_axis, unit, self.time_codes_per_second
        )
    }
}

/// A single transform sample at a specific time.
#[derive(Clone, Debug)]
pub struct TransformSample {
    /// Time code for this sample
    pub time: f64,

    /// 4x4 transform matrix
    pub transform: Mat4,
}

/// Animated mesh data with time samples.
#[derive(Clone, Debug)]
pub struct UsdAnimatedMeshData {
    /// Mesh index this animation applies to
    pub mesh_index: usize,

    /// Transform samples (empty if static)
    pub xform_samples: Vec<TransformSample>,
}

/// Animated instancer data with per-instance transforms at multiple times.
#[derive(Clone, Debug)]
pub struct UsdAnimatedInstancerData {
    /// Instancer index this animation applies to
    pub instancer_index: usize,

    /// Time sample values
    pub time_samples: Vec<f64>,

    /// Number of instances
    pub instance_count: usize,

    /// Transforms per time sample: transforms[time_idx][instance_idx]
    pub transforms: Vec<Vec<Mat4>>,
}

// ============================================================================
// UsdStage - Safe Wrapper
// ============================================================================

/// A USD stage opened via the C++ bridge.
///
/// Automatically closes the stage when dropped.
pub struct UsdStage {
    raw: *mut UsdBridgeStageRaw,
}

// SAFETY: UsdStage is Send + Sync because:
// 1. All USD data is pre-cached at load time in usd_bridge_open_stage().
//    The C++ side populates caches (mesh vertices, normals, UVs, xforms)
//    during open and all subsequent reads go through these immutable caches.
// 2. All getter FFI functions take the stage pointer as const — no mutation
//    occurs through &self methods on the Rust side.
// 3. usd_bridge_get_mesh_vertices_at_time() uses thread_local storage for
//    its return buffer — each thread gets its own buffer, avoiding data races.
// 4. Rust immediately copies the data via to_vec() before the thread_local
//    buffer can be reused by a subsequent call on the same thread.
// 5. The underlying UsdStageRefPtr is read-only after caching; no USD
//    composition or layer mutations are performed through this type.
// 6. Rust's type system enforces that &UsdStage (shared ref) is the only
//    way to access the stage after construction — no &mut self methods exist.
//
// Usage pattern: Arc<UsdStage> is shared across threads in batch_render.rs
// for parallel bucket rendering, where each thread reads mesh data at
// potentially different animation times.
unsafe impl Send for UsdStage {}
unsafe impl Sync for UsdStage {}

impl UsdStage {
    /// Open a USD stage from a file path.
    ///
    /// Supports `.usda` (text), `.usdc` (binary), and `.usd` (auto-detect) formats.
    /// References are automatically resolved.
    pub fn open<P: AsRef<Path>>(path: P) -> UsdBridgeResult<Self> {
        // Convert to absolute path - USD C++ library may not handle relative paths well
        let abs_path = std::fs::canonicalize(path.as_ref())
            .map_err(|_| UsdBridgeError::FileNotFound(path.as_ref().display().to_string()))?;

        let path_str = abs_path.to_str().ok_or(UsdBridgeError::InvalidPath)?;

        // Handle Windows extended path prefixes from canonicalize():
        // - Local paths: \\?\C:\... -> C:\...
        // - UNC paths: \\?\UNC\server\share\... -> \\server\share\...
        let path_str = if let Some(unc_path) = path_str.strip_prefix(r"\\?\UNC\") {
            // UNC path - convert back to standard \\server\share format
            format!(r"\\{}", unc_path)
        } else if let Some(local_path) = path_str.strip_prefix(r"\\?\") {
            // Local extended path - just strip the prefix
            local_path.to_string()
        } else {
            path_str.to_string()
        };

        let c_path = CString::new(path_str.as_str()).map_err(|_| UsdBridgeError::InvalidPath)?;

        let mut raw: *mut UsdBridgeStageRaw = ptr::null_mut();

        let result = unsafe { usd_bridge_open_stage(c_path.as_ptr(), &mut raw) };

        if result != UsdBridgeErrorCode::Success {
            return Err(match result {
                UsdBridgeErrorCode::FileNotFound => {
                    UsdBridgeError::FileNotFound(path_str.to_string())
                }
                other => other.into(),
            });
        }

        Ok(Self { raw })
    }

    /// Get the number of mesh prims in the stage.
    pub fn mesh_count(&self) -> UsdBridgeResult<usize> {
        let mut count: usize = 0;
        let result = unsafe { usd_bridge_get_mesh_count(self.raw, &mut count) };

        if result != UsdBridgeErrorCode::Success {
            return Err(result.into());
        }

        Ok(count)
    }

    /// Get the number of point instancer prims in the stage.
    pub fn instancer_count(&self) -> UsdBridgeResult<usize> {
        let mut count: usize = 0;
        let result = unsafe { usd_bridge_get_instancer_count(self.raw, &mut count) };

        if result != UsdBridgeErrorCode::Success {
            return Err(result.into());
        }

        Ok(count)
    }

    /// Get mesh data by index.
    pub fn get_mesh(&self, index: usize) -> UsdBridgeResult<UsdMeshData> {
        let mut raw_data = UsdBridgeMeshDataRaw {
            path: ptr::null(),
            vertices: ptr::null(),
            vertex_count: 0,
            indices: ptr::null(),
            index_count: 0,
            normals: ptr::null(),
            normal_count: 0,
            uvs: ptr::null(),
            uv_count: 0,
            face_material_ids: ptr::null(),
            triangle_count: 0,
            transform: [0.0; 16],
            purpose: UsdBridgePurposeRaw::Default,
            is_instance_proxy: 0,
            visibility: 1,
            double_sided: 0,
            subdivision_scheme: ptr::null(),
            normals_interpolation: 0,
            display_color: ptr::null(),
            display_color_count: 0,
            display_opacity: 1.0,
            resets_xform_stack: 0,
            face_vertex_counts: ptr::null(),
            face_count: 0,
            face_vertex_indices: ptr::null(),
            face_vertex_index_count: 0,
            crease_indices: ptr::null(),
            crease_index_count: 0,
            crease_lengths: ptr::null(),
            crease_length_count: 0,
            crease_sharpnesses: ptr::null(),
            crease_sharpness_count: 0,
        };

        let result = unsafe { usd_bridge_get_mesh(self.raw, index, &mut raw_data) };

        if result != UsdBridgeErrorCode::Success {
            return Err(match result {
                UsdBridgeErrorCode::InvalidPrim => {
                    UsdBridgeError::InvalidPrim(format!("mesh index {}", index))
                }
                other => other.into(),
            });
        }

        // Convert to Rust types
        let path = unsafe {
            if raw_data.path.is_null() {
                String::new()
            } else {
                CStr::from_ptr(raw_data.path).to_string_lossy().into_owned()
            }
        };

        // Convert vertices (flat f32 array to Vec<Vec3>)
        let vertices = unsafe {
            if raw_data.vertices.is_null() || raw_data.vertex_count == 0 {
                Vec::new()
            } else {
                let slice =
                    std::slice::from_raw_parts(raw_data.vertices, raw_data.vertex_count * 3);
                slice
                    .chunks_exact(3)
                    .map(|chunk| Vec3::new(chunk[0], chunk[1], chunk[2]))
                    .collect()
            }
        };

        // Convert indices
        let indices = unsafe {
            if raw_data.indices.is_null() || raw_data.index_count == 0 {
                Vec::new()
            } else {
                std::slice::from_raw_parts(raw_data.indices, raw_data.index_count).to_vec()
            }
        };

        // Convert normals (optional)
        let normals = unsafe {
            if raw_data.normals.is_null() || raw_data.normal_count == 0 {
                None
            } else {
                let slice = std::slice::from_raw_parts(raw_data.normals, raw_data.normal_count * 3);
                Some(
                    slice
                        .chunks_exact(3)
                        .map(|chunk| Vec3::new(chunk[0], chunk[1], chunk[2]))
                        .collect(),
                )
            }
        };

        // Convert UVs (optional)
        let uvs = unsafe {
            if raw_data.uvs.is_null() || raw_data.uv_count == 0 {
                None
            } else {
                let slice = std::slice::from_raw_parts(raw_data.uvs, raw_data.uv_count * 2);
                Some(
                    slice
                        .chunks_exact(2)
                        .map(|chunk| [chunk[0], chunk[1]])
                        .collect(),
                )
            }
        };

        // Convert per-triangle material IDs (optional, from GeomSubsets)
        let face_material_ids = unsafe {
            if raw_data.face_material_ids.is_null() || raw_data.triangle_count == 0 {
                None
            } else {
                Some(
                    std::slice::from_raw_parts(raw_data.face_material_ids, raw_data.triangle_count)
                        .to_vec(),
                )
            }
        };

        // USD row-major f32[16] -> glam Mat4 via from_cols_array() (implicit transpose
        // from row-vector to column-vector convention)
        let transform = Mat4::from_cols_array(&raw_data.transform);

        let purpose = match raw_data.purpose {
            UsdBridgePurposeRaw::Render => MeshPurpose::Render,
            UsdBridgePurposeRaw::Proxy => MeshPurpose::Proxy,
            UsdBridgePurposeRaw::Guide => MeshPurpose::Guide,
            _ => MeshPurpose::Default,
        };

        let subdivision_scheme = unsafe {
            if raw_data.subdivision_scheme.is_null() {
                SubdivisionScheme::None
            } else {
                match CStr::from_ptr(raw_data.subdivision_scheme)
                    .to_str()
                    .unwrap_or("none")
                {
                    "catmullClark" => SubdivisionScheme::CatmullClark,
                    "loop" => SubdivisionScheme::Loop,
                    "bilinear" => SubdivisionScheme::Bilinear,
                    _ => SubdivisionScheme::None,
                }
            }
        };

        let normals_interpolation = match raw_data.normals_interpolation {
            1 => NormalsInterpolation::FaceVarying,
            2 => NormalsInterpolation::Uniform,
            3 => NormalsInterpolation::Constant,
            _ => NormalsInterpolation::Vertex,
        };

        Ok(UsdMeshData {
            path,
            vertices,
            indices,
            normals,
            uvs,
            face_material_ids,
            transform,
            purpose,
            is_instance_proxy: raw_data.is_instance_proxy != 0,
            visible: raw_data.visibility != 0,
            double_sided: raw_data.double_sided != 0,
            subdivision_scheme,
            normals_interpolation,
            display_color: unsafe {
                if raw_data.display_color.is_null() || raw_data.display_color_count == 0 {
                    None
                } else {
                    let slice = std::slice::from_raw_parts(
                        raw_data.display_color,
                        raw_data.display_color_count * 3,
                    );
                    Some(
                        slice
                            .chunks_exact(3)
                            .map(|c| Vec3::new(c[0], c[1], c[2]))
                            .collect(),
                    )
                }
            },
            display_opacity: raw_data.display_opacity,
            resets_xform_stack: raw_data.resets_xform_stack != 0,
            face_vertex_counts: unsafe {
                if raw_data.face_vertex_counts.is_null() || raw_data.face_count == 0 {
                    None
                } else {
                    Some(
                        std::slice::from_raw_parts(
                            raw_data.face_vertex_counts,
                            raw_data.face_count,
                        )
                        .to_vec(),
                    )
                }
            },
            face_vertex_indices: unsafe {
                if raw_data.face_vertex_indices.is_null() || raw_data.face_vertex_index_count == 0 {
                    None
                } else {
                    Some(
                        std::slice::from_raw_parts(
                            raw_data.face_vertex_indices,
                            raw_data.face_vertex_index_count,
                        )
                        .to_vec(),
                    )
                }
            },
            crease_indices: unsafe {
                if raw_data.crease_indices.is_null() || raw_data.crease_index_count == 0 {
                    None
                } else {
                    Some(
                        std::slice::from_raw_parts(
                            raw_data.crease_indices,
                            raw_data.crease_index_count,
                        )
                        .to_vec(),
                    )
                }
            },
            crease_lengths: unsafe {
                if raw_data.crease_lengths.is_null() || raw_data.crease_length_count == 0 {
                    None
                } else {
                    Some(
                        std::slice::from_raw_parts(
                            raw_data.crease_lengths,
                            raw_data.crease_length_count,
                        )
                        .to_vec(),
                    )
                }
            },
            crease_sharpnesses: unsafe {
                if raw_data.crease_sharpnesses.is_null() || raw_data.crease_sharpness_count == 0 {
                    None
                } else {
                    Some(
                        std::slice::from_raw_parts(
                            raw_data.crease_sharpnesses,
                            raw_data.crease_sharpness_count,
                        )
                        .to_vec(),
                    )
                }
            },
        })
    }

    /// Get instancer data by index.
    pub fn get_instancer(&self, index: usize) -> UsdBridgeResult<UsdInstancerData> {
        let mut raw_data = UsdBridgeInstancerDataRaw {
            path: ptr::null(),
            prototype_paths: ptr::null(),
            prototype_count: 0,
            transforms: ptr::null(),
            instance_count: 0,
            proto_indices: ptr::null(),
            velocities: ptr::null(),
            velocity_count: 0,
            angular_velocities: ptr::null(),
            angular_velocity_count: 0,
            invisible_ids: ptr::null(),
            invisible_id_count: 0,
        };

        let result = unsafe { usd_bridge_get_instancer(self.raw, index, &mut raw_data) };

        if result != UsdBridgeErrorCode::Success {
            return Err(match result {
                UsdBridgeErrorCode::InvalidPrim => {
                    UsdBridgeError::InvalidPrim(format!("instancer index {}", index))
                }
                other => other.into(),
            });
        }

        // Convert path
        let path = unsafe {
            if raw_data.path.is_null() {
                String::new()
            } else {
                CStr::from_ptr(raw_data.path).to_string_lossy().into_owned()
            }
        };

        // Convert prototype paths
        let prototype_paths = unsafe {
            if raw_data.prototype_paths.is_null() || raw_data.prototype_count == 0 {
                Vec::new()
            } else {
                let ptrs =
                    std::slice::from_raw_parts(raw_data.prototype_paths, raw_data.prototype_count);
                ptrs.iter()
                    .map(|&ptr| {
                        if ptr.is_null() {
                            String::new()
                        } else {
                            CStr::from_ptr(ptr).to_string_lossy().into_owned()
                        }
                    })
                    .collect()
            }
        };

        // Convert transforms (flat f32 array to Vec<Mat4>)
        let transforms = unsafe {
            if raw_data.transforms.is_null() || raw_data.instance_count == 0 {
                Vec::new()
            } else {
                let slice =
                    std::slice::from_raw_parts(raw_data.transforms, raw_data.instance_count * 16);
                slice
                    .chunks_exact(16)
                    .map(|chunk| {
                        let mut arr = [0.0f32; 16];
                        arr.copy_from_slice(chunk);
                        Mat4::from_cols_array(&arr)
                    })
                    .collect()
            }
        };

        // Convert proto indices
        let proto_indices = unsafe {
            if raw_data.proto_indices.is_null() || raw_data.instance_count == 0 {
                Vec::new()
            } else {
                std::slice::from_raw_parts(raw_data.proto_indices, raw_data.instance_count).to_vec()
            }
        };

        // Convert velocities (optional)
        let velocities = unsafe {
            if raw_data.velocities.is_null() || raw_data.velocity_count == 0 {
                None
            } else {
                let slice =
                    std::slice::from_raw_parts(raw_data.velocities, raw_data.velocity_count * 3);
                Some(
                    slice
                        .chunks_exact(3)
                        .map(|c| Vec3::new(c[0], c[1], c[2]))
                        .collect(),
                )
            }
        };

        // Convert angular velocities (optional)
        let angular_velocities = unsafe {
            if raw_data.angular_velocities.is_null() || raw_data.angular_velocity_count == 0 {
                None
            } else {
                let slice = std::slice::from_raw_parts(
                    raw_data.angular_velocities,
                    raw_data.angular_velocity_count * 3,
                );
                Some(
                    slice
                        .chunks_exact(3)
                        .map(|c| Vec3::new(c[0], c[1], c[2]))
                        .collect(),
                )
            }
        };

        // Convert invisible IDs
        let invisible_ids = unsafe {
            if raw_data.invisible_ids.is_null() || raw_data.invisible_id_count == 0 {
                Vec::new()
            } else {
                std::slice::from_raw_parts(raw_data.invisible_ids, raw_data.invisible_id_count)
                    .to_vec()
            }
        };

        Ok(UsdInstancerData {
            path,
            prototype_paths,
            transforms,
            proto_indices,
            velocities,
            angular_velocities,
            invisible_ids,
        })
    }

    /// Get all meshes in the stage.
    pub fn meshes(&self) -> UsdBridgeResult<Vec<UsdMeshData>> {
        let count = self.mesh_count()?;
        let mut meshes = Vec::with_capacity(count);
        for i in 0..count {
            meshes.push(self.get_mesh(i)?);
        }
        Ok(meshes)
    }

    /// Get all instancers in the stage.
    pub fn instancers(&self) -> UsdBridgeResult<Vec<UsdInstancerData>> {
        let count = self.instancer_count()?;
        let mut instancers = Vec::with_capacity(count);
        for i in 0..count {
            instancers.push(self.get_instancer(i)?);
        }
        Ok(instancers)
    }

    // ========================================================================
    // Native Instances (instanceable=true)
    // ========================================================================

    /// Get the number of native instances (from instanceable=true prims).
    pub fn native_instance_count(&self) -> UsdBridgeResult<usize> {
        let mut count: usize = 0;
        let result = unsafe { usd_bridge_get_native_instance_count(self.raw, &mut count) };
        if result != UsdBridgeErrorCode::Success {
            return Err(result.into());
        }
        Ok(count)
    }

    /// Get native instance data by index.
    pub fn get_native_instance(&self, index: usize) -> UsdBridgeResult<UsdNativeInstance> {
        let mut raw_data = UsdNativeInstanceDataRaw {
            proto_mesh_idx: 0,
            transform: [0.0; 16],
            material_override_idx: -1,
        };

        let result = unsafe { usd_bridge_get_native_instance(self.raw, index, &mut raw_data) };
        if result != UsdBridgeErrorCode::Success {
            return Err(match result {
                UsdBridgeErrorCode::InvalidPrim => {
                    UsdBridgeError::InvalidPrim(format!("native instance index {}", index))
                }
                other => other.into(),
            });
        }

        Ok(UsdNativeInstance {
            proto_mesh_idx: raw_data.proto_mesh_idx as usize,
            transform: Mat4::from_cols_array(&raw_data.transform),
            material_override_idx: raw_data.material_override_idx,
        })
    }

    /// Get all native instances.
    pub fn native_instances(&self) -> UsdBridgeResult<Vec<UsdNativeInstance>> {
        let count = self.native_instance_count()?;
        let mut instances = Vec::with_capacity(count);
        for i in 0..count {
            instances.push(self.get_native_instance(i)?);
        }
        Ok(instances)
    }

    /// Get the number of materials in the stage.
    pub fn material_count(&self) -> UsdBridgeResult<usize> {
        let mut count: usize = 0;
        let result = unsafe { usd_bridge_get_material_count(self.raw, &mut count) };

        if result != UsdBridgeErrorCode::Success {
            return Err(result.into());
        }

        Ok(count)
    }

    /// Get material data by index.
    pub fn get_material(&self, index: usize) -> UsdBridgeResult<UsdMaterialData> {
        let mut raw_data = UsdBridgeMaterialDataRaw {
            path: ptr::null(),
            diffuse_color: [0.5, 0.5, 0.5],
            metallic: 0.0,
            roughness: 0.5,
            specular: 0.5,
            opacity: 1.0,
            emissive_color: [0.0, 0.0, 0.0],
            diffuse_texture: ptr::null(),
            roughness_texture: ptr::null(),
            metallic_texture: ptr::null(),
            normal_texture: ptr::null(),
            emissive_texture: ptr::null(),
            is_materialx: 0,
        };

        let result = unsafe { usd_bridge_get_material(self.raw, index, &mut raw_data) };

        if result != UsdBridgeErrorCode::Success {
            return Err(match result {
                UsdBridgeErrorCode::InvalidPrim => {
                    UsdBridgeError::InvalidPrim(format!("material index {}", index))
                }
                other => other.into(),
            });
        }

        // Convert path
        let path = unsafe {
            if raw_data.path.is_null() {
                String::new()
            } else {
                CStr::from_ptr(raw_data.path).to_string_lossy().into_owned()
            }
        };

        // Helper to convert optional texture path
        let texture_path = |ptr: *const std::ffi::c_char| -> Option<String> {
            if ptr.is_null() {
                None
            } else {
                let s = unsafe { CStr::from_ptr(ptr).to_string_lossy().into_owned() };
                if s.is_empty() {
                    None
                } else {
                    Some(s)
                }
            }
        };

        Ok(UsdMaterialData {
            path,
            diffuse_color: Vec3::new(
                raw_data.diffuse_color[0],
                raw_data.diffuse_color[1],
                raw_data.diffuse_color[2],
            ),
            metallic: raw_data.metallic,
            roughness: raw_data.roughness,
            specular: raw_data.specular,
            opacity: raw_data.opacity,
            emissive_color: Vec3::new(
                raw_data.emissive_color[0],
                raw_data.emissive_color[1],
                raw_data.emissive_color[2],
            ),
            diffuse_texture: texture_path(raw_data.diffuse_texture),
            roughness_texture: texture_path(raw_data.roughness_texture),
            metallic_texture: texture_path(raw_data.metallic_texture),
            normal_texture: texture_path(raw_data.normal_texture),
            emissive_texture: texture_path(raw_data.emissive_texture),
            is_materialx: raw_data.is_materialx != 0,
        })
    }

    /// Get the material path bound to a mesh.
    pub fn get_mesh_material_path(&self, mesh_index: usize) -> UsdBridgeResult<Option<String>> {
        let mut path_ptr: *const std::ffi::c_char = ptr::null();
        let result =
            unsafe { usd_bridge_get_mesh_material_path(self.raw, mesh_index, &mut path_ptr) };

        if result != UsdBridgeErrorCode::Success {
            return Err(match result {
                UsdBridgeErrorCode::InvalidPrim => {
                    UsdBridgeError::InvalidPrim(format!("mesh index {}", mesh_index))
                }
                other => other.into(),
            });
        }

        if path_ptr.is_null() {
            return Ok(None);
        }

        let path = unsafe { CStr::from_ptr(path_ptr).to_string_lossy().into_owned() };
        if path.is_empty() {
            Ok(None)
        } else {
            Ok(Some(path))
        }
    }

    /// Get all materials in the stage.
    pub fn materials(&self) -> UsdBridgeResult<Vec<UsdMaterialData>> {
        let count = self.material_count()?;
        let mut materials = Vec::with_capacity(count);
        for i in 0..count {
            materials.push(self.get_material(i)?);
        }
        Ok(materials)
    }

    // ========================================================================
    // Light Data (UsdLux)
    // ========================================================================

    /// Get the number of lights in the stage.
    pub fn light_count(&self) -> UsdBridgeResult<usize> {
        let mut count: usize = 0;
        let result = unsafe { usd_bridge_get_light_count(self.raw, &mut count) };

        if result != UsdBridgeErrorCode::Success {
            return Err(result.into());
        }

        Ok(count)
    }

    /// Get light data by index.
    pub fn get_light(&self, index: usize) -> UsdBridgeResult<UsdLightData> {
        let mut raw_data = UsdBridgeLightDataRaw {
            path: ptr::null(),
            light_type: UsdBridgeLightType::Distant,
            color: [1.0, 1.0, 1.0],
            intensity: 1.0,
            exposure: 0.0,
            transform: [0.0; 16],
            angle: 0.0,
            radius: 0.0,
            width: 0.0,
            height: 0.0,
            texture_path: ptr::null(),
            length: 0.0,
            shaping_cone_angle: 0.0,
            shaping_cone_softness: 0.0,
            shaping_focus: 0.0,
            shaping_ies_file: ptr::null(),
            light_link_includes: ptr::null(),
            light_link_include_count: 0,
            light_link_excludes: ptr::null(),
            light_link_exclude_count: 0,
        };

        let result = unsafe { usd_bridge_get_light(self.raw, index, &mut raw_data) };

        if result != UsdBridgeErrorCode::Success {
            return Err(match result {
                UsdBridgeErrorCode::InvalidPrim => {
                    UsdBridgeError::InvalidPrim(format!("light index {}", index))
                }
                other => other.into(),
            });
        }

        // Convert path
        let path = unsafe {
            if raw_data.path.is_null() {
                String::new()
            } else {
                CStr::from_ptr(raw_data.path).to_string_lossy().into_owned()
            }
        };

        // Convert texture path
        let texture_path = if raw_data.texture_path.is_null() {
            None
        } else {
            let s = unsafe {
                CStr::from_ptr(raw_data.texture_path)
                    .to_string_lossy()
                    .into_owned()
            };
            if s.is_empty() {
                None
            } else {
                Some(s)
            }
        };

        // Combine intensity * 2^exposure
        let combined_intensity = raw_data.intensity * (2.0_f32).powf(raw_data.exposure);

        Ok(UsdLightData {
            path,
            light_type: raw_data.light_type.into(),
            color: Vec3::new(raw_data.color[0], raw_data.color[1], raw_data.color[2]),
            intensity: combined_intensity,
            transform: Mat4::from_cols_array(&raw_data.transform),
            angle: raw_data.angle,
            radius: raw_data.radius,
            width: raw_data.width,
            height: raw_data.height,
            texture_path,
            length: raw_data.length,
            shaping: UsdLightShaping {
                cone_angle: raw_data.shaping_cone_angle,
                cone_softness: raw_data.shaping_cone_softness,
                focus: raw_data.shaping_focus,
                ies_file: if raw_data.shaping_ies_file.is_null() {
                    None
                } else {
                    let s = unsafe {
                        CStr::from_ptr(raw_data.shaping_ies_file)
                            .to_string_lossy()
                            .into_owned()
                    };
                    if s.is_empty() {
                        None
                    } else {
                        Some(s)
                    }
                },
            },
            light_link_includes: unsafe {
                if raw_data.light_link_includes.is_null() || raw_data.light_link_include_count == 0
                {
                    Vec::new()
                } else {
                    let ptrs = std::slice::from_raw_parts(
                        raw_data.light_link_includes,
                        raw_data.light_link_include_count,
                    );
                    ptrs.iter()
                        .filter(|&&p| !p.is_null())
                        .map(|&p| CStr::from_ptr(p).to_string_lossy().into_owned())
                        .collect()
                }
            },
            light_link_excludes: unsafe {
                if raw_data.light_link_excludes.is_null() || raw_data.light_link_exclude_count == 0
                {
                    Vec::new()
                } else {
                    let ptrs = std::slice::from_raw_parts(
                        raw_data.light_link_excludes,
                        raw_data.light_link_exclude_count,
                    );
                    ptrs.iter()
                        .filter(|&&p| !p.is_null())
                        .map(|&p| CStr::from_ptr(p).to_string_lossy().into_owned())
                        .collect()
                }
            },
        })
    }

    /// Get all lights in the stage.
    pub fn lights(&self) -> UsdBridgeResult<Vec<UsdLightData>> {
        let count = self.light_count()?;
        let mut lights = Vec::with_capacity(count);
        for i in 0..count {
            lights.push(self.get_light(i)?);
        }
        Ok(lights)
    }

    // ========================================================================
    // UsdGeomPoints
    // ========================================================================

    /// Get the number of UsdGeomPoints prims.
    pub fn points_count(&self) -> UsdBridgeResult<usize> {
        let mut count: usize = 0;
        let result = unsafe { usd_bridge_get_points_count(self.raw, &mut count) };
        if result != UsdBridgeErrorCode::Success {
            return Err(result.into());
        }
        Ok(count)
    }

    /// Get points data by index.
    pub fn get_points(&self, index: usize) -> UsdBridgeResult<UsdPointsData> {
        let mut raw = UsdBridgePointsDataRaw {
            path: ptr::null(),
            positions: ptr::null(),
            point_count: 0,
            widths: ptr::null(),
            width_count: 0,
            normals: ptr::null(),
            normal_count: 0,
            ids: ptr::null(),
            id_count: 0,
            transform: [0.0; 16],
        };

        let result = unsafe { usd_bridge_get_points(self.raw, index, &mut raw) };
        if result != UsdBridgeErrorCode::Success {
            return Err(result.into());
        }

        let path = unsafe {
            if raw.path.is_null() {
                String::new()
            } else {
                CStr::from_ptr(raw.path).to_string_lossy().into_owned()
            }
        };

        let positions = unsafe {
            if raw.positions.is_null() || raw.point_count == 0 {
                Vec::new()
            } else {
                let s = std::slice::from_raw_parts(raw.positions, raw.point_count * 3);
                s.chunks_exact(3)
                    .map(|c| Vec3::new(c[0], c[1], c[2]))
                    .collect()
            }
        };

        let widths = unsafe {
            if raw.widths.is_null() || raw.width_count == 0 {
                None
            } else {
                Some(std::slice::from_raw_parts(raw.widths, raw.width_count).to_vec())
            }
        };

        let normals = unsafe {
            if raw.normals.is_null() || raw.normal_count == 0 {
                None
            } else {
                let s = std::slice::from_raw_parts(raw.normals, raw.normal_count * 3);
                Some(
                    s.chunks_exact(3)
                        .map(|c| Vec3::new(c[0], c[1], c[2]))
                        .collect(),
                )
            }
        };

        let ids = unsafe {
            if raw.ids.is_null() || raw.id_count == 0 {
                None
            } else {
                Some(std::slice::from_raw_parts(raw.ids, raw.id_count).to_vec())
            }
        };

        Ok(UsdPointsData {
            path,
            positions,
            widths,
            normals,
            ids,
            transform: Mat4::from_cols_array(&raw.transform),
        })
    }

    /// Get all UsdGeomPoints prims.
    pub fn points(&self) -> UsdBridgeResult<Vec<UsdPointsData>> {
        let count = self.points_count()?;
        let mut pts = Vec::with_capacity(count);
        for i in 0..count {
            pts.push(self.get_points(i)?);
        }
        Ok(pts)
    }

    // ========================================================================
    // BasisCurves
    // ========================================================================

    /// Get the number of BasisCurves prims.
    pub fn curves_count(&self) -> UsdBridgeResult<usize> {
        let mut count: usize = 0;
        let result = unsafe { usd_bridge_get_curves_count(self.raw, &mut count) };
        if result != UsdBridgeErrorCode::Success {
            return Err(result.into());
        }
        Ok(count)
    }

    /// Get curves data by index.
    pub fn get_curves(&self, index: usize) -> UsdBridgeResult<UsdCurvesData> {
        let mut raw = UsdBridgeCurvesDataRaw {
            path: ptr::null(),
            points: ptr::null(),
            point_count: 0,
            widths: ptr::null(),
            width_count: 0,
            curve_vertex_counts: ptr::null(),
            curve_count: 0,
            curve_type: UsdBridgeCurveTypeRaw::Linear,
            basis: UsdBridgeCurveBasisRaw::Bezier,
            wrap: UsdBridgeCurveWrapRaw::Nonperiodic,
            transform: [0.0; 16],
        };

        let result = unsafe { usd_bridge_get_curves(self.raw, index, &mut raw) };
        if result != UsdBridgeErrorCode::Success {
            return Err(result.into());
        }

        let path = unsafe {
            if raw.path.is_null() {
                String::new()
            } else {
                CStr::from_ptr(raw.path).to_string_lossy().into_owned()
            }
        };

        let points = unsafe {
            if raw.points.is_null() || raw.point_count == 0 {
                Vec::new()
            } else {
                let s = std::slice::from_raw_parts(raw.points, raw.point_count * 3);
                s.chunks_exact(3)
                    .map(|c| Vec3::new(c[0], c[1], c[2]))
                    .collect()
            }
        };

        let widths = unsafe {
            if raw.widths.is_null() || raw.width_count == 0 {
                None
            } else {
                Some(std::slice::from_raw_parts(raw.widths, raw.width_count).to_vec())
            }
        };

        let curve_vertex_counts = unsafe {
            if raw.curve_vertex_counts.is_null() || raw.curve_count == 0 {
                Vec::new()
            } else {
                std::slice::from_raw_parts(raw.curve_vertex_counts, raw.curve_count).to_vec()
            }
        };

        Ok(UsdCurvesData {
            path,
            points,
            widths,
            curve_vertex_counts,
            curve_type: match raw.curve_type {
                UsdBridgeCurveTypeRaw::Cubic => CurveType::Cubic,
                _ => CurveType::Linear,
            },
            basis: match raw.basis {
                UsdBridgeCurveBasisRaw::Bspline => CurveBasis::Bspline,
                UsdBridgeCurveBasisRaw::CatmullRom => CurveBasis::CatmullRom,
                _ => CurveBasis::Bezier,
            },
            wrap: match raw.wrap {
                UsdBridgeCurveWrapRaw::Periodic => CurveWrap::Periodic,
                UsdBridgeCurveWrapRaw::Pinned => CurveWrap::Pinned,
                _ => CurveWrap::Nonperiodic,
            },
            transform: Mat4::from_cols_array(&raw.transform),
        })
    }

    /// Get all BasisCurves prims.
    pub fn curves(&self) -> UsdBridgeResult<Vec<UsdCurvesData>> {
        let count = self.curves_count()?;
        let mut curves = Vec::with_capacity(count);
        for i in 0..count {
            curves.push(self.get_curves(i)?);
        }
        Ok(curves)
    }

    // ========================================================================
    // Skeleton
    // ========================================================================

    /// Get skeleton count.
    pub fn skeleton_count(&self) -> UsdBridgeResult<usize> {
        let mut count: usize = 0;
        let result = unsafe { usd_bridge_get_skeleton_count(self.raw, &mut count) };
        if result != UsdBridgeErrorCode::Success {
            return Err(result.into());
        }
        Ok(count)
    }

    /// Get skeleton data by index.
    pub fn get_skeleton(&self, index: usize) -> UsdBridgeResult<UsdSkeletonData> {
        let mut raw = UsdBridgeSkeletonDataRaw {
            path: ptr::null(),
            joint_paths: ptr::null(),
            joint_count: 0,
            bind_transforms: ptr::null(),
            rest_transforms: ptr::null(),
        };
        let result = unsafe { usd_bridge_get_skeleton(self.raw, index, &mut raw) };
        if result != UsdBridgeErrorCode::Success {
            return Err(result.into());
        }

        let path = unsafe {
            if raw.path.is_null() {
                String::new()
            } else {
                CStr::from_ptr(raw.path).to_string_lossy().into_owned()
            }
        };

        let joint_paths = unsafe {
            if raw.joint_paths.is_null() || raw.joint_count == 0 {
                Vec::new()
            } else {
                let ptrs = std::slice::from_raw_parts(raw.joint_paths, raw.joint_count);
                ptrs.iter()
                    .map(|&p| {
                        if p.is_null() {
                            String::new()
                        } else {
                            CStr::from_ptr(p).to_string_lossy().into_owned()
                        }
                    })
                    .collect()
            }
        };

        let bind_transforms = unsafe {
            if raw.bind_transforms.is_null() || raw.joint_count == 0 {
                Vec::new()
            } else {
                let s = std::slice::from_raw_parts(raw.bind_transforms, raw.joint_count * 16);
                s.chunks_exact(16)
                    .map(|c| {
                        let mut arr = [0.0f32; 16];
                        arr.copy_from_slice(c);
                        Mat4::from_cols_array(&arr)
                    })
                    .collect()
            }
        };

        let rest_transforms = unsafe {
            if raw.rest_transforms.is_null() || raw.joint_count == 0 {
                Vec::new()
            } else {
                let s = std::slice::from_raw_parts(raw.rest_transforms, raw.joint_count * 16);
                s.chunks_exact(16)
                    .map(|c| {
                        let mut arr = [0.0f32; 16];
                        arr.copy_from_slice(c);
                        Mat4::from_cols_array(&arr)
                    })
                    .collect()
            }
        };

        Ok(UsdSkeletonData {
            path,
            joint_paths,
            bind_transforms,
            rest_transforms,
        })
    }

    /// Get skin binding for a mesh (if it has UsdSkelBindingAPI).
    pub fn get_skin_binding(&self, mesh_index: usize) -> UsdBridgeResult<UsdSkinBindingData> {
        let mut raw = UsdBridgeSkinBindingDataRaw {
            mesh_path: ptr::null(),
            skeleton_path: ptr::null(),
            joint_indices: ptr::null(),
            joint_indices_count: 0,
            joint_weights: ptr::null(),
            joint_weights_count: 0,
            joint_indices_element_size: 0,
            geom_bind_transform: [0.0; 16],
        };
        let result = unsafe { usd_bridge_get_skin_binding(self.raw, mesh_index, &mut raw) };
        if result != UsdBridgeErrorCode::Success {
            return Err(result.into());
        }

        Ok(UsdSkinBindingData {
            mesh_path: unsafe {
                if raw.mesh_path.is_null() {
                    String::new()
                } else {
                    CStr::from_ptr(raw.mesh_path).to_string_lossy().into_owned()
                }
            },
            skeleton_path: unsafe {
                if raw.skeleton_path.is_null() {
                    String::new()
                } else {
                    CStr::from_ptr(raw.skeleton_path)
                        .to_string_lossy()
                        .into_owned()
                }
            },
            joint_indices: unsafe {
                if raw.joint_indices.is_null() || raw.joint_indices_count == 0 {
                    Vec::new()
                } else {
                    std::slice::from_raw_parts(raw.joint_indices, raw.joint_indices_count).to_vec()
                }
            },
            joint_weights: unsafe {
                if raw.joint_weights.is_null() || raw.joint_weights_count == 0 {
                    Vec::new()
                } else {
                    std::slice::from_raw_parts(raw.joint_weights, raw.joint_weights_count).to_vec()
                }
            },
            element_size: raw.joint_indices_element_size,
            geom_bind_transform: Mat4::from_cols_array(&raw.geom_bind_transform),
        })
    }

    // ========================================================================
    // Volumes
    // ========================================================================

    /// Get volume count.
    pub fn volume_count(&self) -> UsdBridgeResult<usize> {
        let mut count: usize = 0;
        let result = unsafe { usd_bridge_get_volume_count(self.raw, &mut count) };
        if result != UsdBridgeErrorCode::Success {
            return Err(result.into());
        }
        Ok(count)
    }

    /// Get volume data by index.
    pub fn get_volume(&self, index: usize) -> UsdBridgeResult<UsdVolumeData> {
        let mut raw = UsdBridgeVolumeDataRaw {
            path: ptr::null(),
            vdb_file_path: ptr::null(),
            field_name: ptr::null(),
            transform: [0.0; 16],
        };
        let result = unsafe { usd_bridge_get_volume(self.raw, index, &mut raw) };
        if result != UsdBridgeErrorCode::Success {
            return Err(result.into());
        }

        let to_opt_string = |p: *const std::ffi::c_char| -> Option<String> {
            if p.is_null() {
                None
            } else {
                let s = unsafe { CStr::from_ptr(p).to_string_lossy().into_owned() };
                if s.is_empty() {
                    None
                } else {
                    Some(s)
                }
            }
        };

        Ok(UsdVolumeData {
            path: unsafe {
                if raw.path.is_null() {
                    String::new()
                } else {
                    CStr::from_ptr(raw.path).to_string_lossy().into_owned()
                }
            },
            vdb_file_path: to_opt_string(raw.vdb_file_path),
            field_name: to_opt_string(raw.field_name),
            transform: Mat4::from_cols_array(&raw.transform),
        })
    }

    /// Get all volumes.
    pub fn volumes(&self) -> UsdBridgeResult<Vec<UsdVolumeData>> {
        let count = self.volume_count()?;
        let mut vols = Vec::with_capacity(count);
        for i in 0..count {
            vols.push(self.get_volume(i)?);
        }
        Ok(vols)
    }

    // ========================================================================
    // Primvar Query
    // ========================================================================

    /// Get user primvars for a mesh (excludes built-in st, normals, displayColor).
    pub fn get_mesh_primvars(&self, mesh_index: usize) -> UsdBridgeResult<Vec<UsdPrimvarData>> {
        let mut count: usize = 0;
        let result = unsafe { usd_bridge_get_mesh_primvar_count(self.raw, mesh_index, &mut count) };
        if result != UsdBridgeErrorCode::Success {
            return Err(result.into());
        }

        let mut primvars = Vec::with_capacity(count);
        for i in 0..count {
            let mut raw = UsdBridgePrimvarDataRaw {
                name: ptr::null(),
                primvar_type: UsdBridgePrimvarTypeRaw::Float,
                interpolation: UsdBridgePrimvarInterpolationRaw::Vertex,
                float_data: ptr::null(),
                int_data: ptr::null(),
                element_count: 0,
            };

            let result = unsafe { usd_bridge_get_mesh_primvar(self.raw, mesh_index, i, &mut raw) };
            if result != UsdBridgeErrorCode::Success {
                continue;
            }

            let name = unsafe {
                if raw.name.is_null() {
                    String::new()
                } else {
                    CStr::from_ptr(raw.name).to_string_lossy().into_owned()
                }
            };

            let primvar_type = match raw.primvar_type {
                UsdBridgePrimvarTypeRaw::Float => PrimvarType::Float,
                UsdBridgePrimvarTypeRaw::Float2 => PrimvarType::Float2,
                UsdBridgePrimvarTypeRaw::Float3 => PrimvarType::Float3,
                UsdBridgePrimvarTypeRaw::Int => PrimvarType::Int,
            };

            let interpolation = match raw.interpolation {
                UsdBridgePrimvarInterpolationRaw::Constant => PrimvarInterpolation::Constant,
                UsdBridgePrimvarInterpolationRaw::Uniform => PrimvarInterpolation::Uniform,
                UsdBridgePrimvarInterpolationRaw::Vertex => PrimvarInterpolation::Vertex,
                UsdBridgePrimvarInterpolationRaw::FaceVarying => PrimvarInterpolation::FaceVarying,
            };

            let float_count = match primvar_type {
                PrimvarType::Float => raw.element_count,
                PrimvarType::Float2 => raw.element_count * 2,
                PrimvarType::Float3 => raw.element_count * 3,
                PrimvarType::Int => 0,
            };

            let float_data = unsafe {
                if raw.float_data.is_null() || float_count == 0 {
                    Vec::new()
                } else {
                    std::slice::from_raw_parts(raw.float_data, float_count).to_vec()
                }
            };

            let int_data = unsafe {
                if raw.int_data.is_null() || raw.element_count == 0 {
                    Vec::new()
                } else if matches!(primvar_type, PrimvarType::Int) {
                    std::slice::from_raw_parts(raw.int_data, raw.element_count).to_vec()
                } else {
                    Vec::new()
                }
            };

            primvars.push(UsdPrimvarData {
                name,
                primvar_type,
                interpolation,
                float_data,
                int_data,
                element_count: raw.element_count,
            });
        }

        Ok(primvars)
    }

    // ========================================================================
    // Payload Load/Unload
    // ========================================================================

    /// Load a prim's payload content.
    pub fn load_payload(&self, prim_path: &str) -> UsdBridgeResult<()> {
        let c_path = CString::new(prim_path).map_err(|_| UsdBridgeError::InvalidPath)?;
        // Safety: load_payload invalidates caches in C++ (mutable through const pointer is ok
        // because the C++ side handles internal mutability via non-const stage member)
        let code = unsafe { usd_bridge_load_payload(self.raw as *mut _, c_path.as_ptr()) };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(())
    }

    /// Unload a prim's payload to free memory.
    pub fn unload_payload(&self, prim_path: &str) -> UsdBridgeResult<()> {
        let c_path = CString::new(prim_path).map_err(|_| UsdBridgeError::InvalidPath)?;
        let code = unsafe { usd_bridge_unload_payload(self.raw as *mut _, c_path.as_ptr()) };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(())
    }

    // ========================================================================
    // Variant Query / Selection
    // ========================================================================

    /// Get variant set names for a prim.
    pub fn get_variant_set_names(&self, prim_path: &str) -> UsdBridgeResult<Vec<String>> {
        let c_path = CString::new(prim_path).map_err(|_| UsdBridgeError::InvalidPath)?;
        let mut count: usize = 0;
        let code =
            unsafe { usd_bridge_get_variant_set_count(self.raw, c_path.as_ptr(), &mut count) };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        let mut names = Vec::with_capacity(count);
        for i in 0..count {
            let mut name_ptr: *const std::ffi::c_char = ptr::null();
            let code = unsafe {
                usd_bridge_get_variant_set_name(self.raw, c_path.as_ptr(), i, &mut name_ptr)
            };
            if code != UsdBridgeErrorCode::Success {
                continue;
            }
            if !name_ptr.is_null() {
                names.push(unsafe { CStr::from_ptr(name_ptr).to_string_lossy().into_owned() });
            }
        }
        Ok(names)
    }

    /// Get variant names within a variant set.
    pub fn get_variant_names(
        &self,
        prim_path: &str,
        variant_set: &str,
    ) -> UsdBridgeResult<Vec<String>> {
        let c_path = CString::new(prim_path).map_err(|_| UsdBridgeError::InvalidPath)?;
        let c_set = CString::new(variant_set).map_err(|_| UsdBridgeError::InvalidPath)?;
        let mut count: usize = 0;
        let code = unsafe {
            usd_bridge_get_variant_count(self.raw, c_path.as_ptr(), c_set.as_ptr(), &mut count)
        };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        let mut names = Vec::with_capacity(count);
        for i in 0..count {
            let mut name_ptr: *const std::ffi::c_char = ptr::null();
            let code = unsafe {
                usd_bridge_get_variant_name(
                    self.raw,
                    c_path.as_ptr(),
                    c_set.as_ptr(),
                    i,
                    &mut name_ptr,
                )
            };
            if code != UsdBridgeErrorCode::Success {
                continue;
            }
            if !name_ptr.is_null() {
                names.push(unsafe { CStr::from_ptr(name_ptr).to_string_lossy().into_owned() });
            }
        }
        Ok(names)
    }

    /// Get current variant selection for a variant set.
    pub fn get_variant_selection(
        &self,
        prim_path: &str,
        variant_set: &str,
    ) -> UsdBridgeResult<String> {
        let c_path = CString::new(prim_path).map_err(|_| UsdBridgeError::InvalidPath)?;
        let c_set = CString::new(variant_set).map_err(|_| UsdBridgeError::InvalidPath)?;
        let mut sel_ptr: *const std::ffi::c_char = ptr::null();
        let code = unsafe {
            usd_bridge_get_variant_selection(
                self.raw,
                c_path.as_ptr(),
                c_set.as_ptr(),
                &mut sel_ptr,
            )
        };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        if sel_ptr.is_null() {
            return Ok(String::new());
        }
        Ok(unsafe { CStr::from_ptr(sel_ptr).to_string_lossy().into_owned() })
    }

    /// Set variant selection (triggers re-composition, invalidates caches).
    pub fn set_variant_selection(
        &self,
        prim_path: &str,
        variant_set: &str,
        variant_name: &str,
    ) -> UsdBridgeResult<()> {
        let c_path = CString::new(prim_path).map_err(|_| UsdBridgeError::InvalidPath)?;
        let c_set = CString::new(variant_set).map_err(|_| UsdBridgeError::InvalidPath)?;
        let c_name = CString::new(variant_name).map_err(|_| UsdBridgeError::InvalidPath)?;
        let code = unsafe {
            usd_bridge_set_variant_selection(
                self.raw as *mut _,
                c_path.as_ptr(),
                c_set.as_ptr(),
                c_name.as_ptr(),
            )
        };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(())
    }

    /// Export the stage to a file.
    ///
    /// Format is determined by file extension: `.usda`, `.usdc`, or `.usd`.
    pub fn export<P: AsRef<Path>>(&self, path: P) -> UsdBridgeResult<()> {
        let path_str = path.as_ref().to_str().ok_or(UsdBridgeError::InvalidPath)?;
        let c_path = CString::new(path_str).map_err(|_| UsdBridgeError::InvalidPath)?;

        let result = unsafe { usd_bridge_export_stage(self.raw, c_path.as_ptr()) };

        if result != UsdBridgeErrorCode::Success {
            return Err(result.into());
        }

        Ok(())
    }

    // ========================================================================
    // Stage Metadata
    // ========================================================================

    /// Get stage metadata (metersPerUnit, upAxis).
    pub fn get_stage_metadata(&self) -> UsdBridgeResult<UsdStageMetadata> {
        let mut raw = UsdBridgeStageMetadataRaw {
            meters_per_unit: 1.0,
            up_axis: UsdBridgeUpAxisRaw::Y,
            time_codes_per_second: 24.0,
        };

        let result = unsafe { usd_bridge_get_stage_metadata(self.raw, &mut raw) };

        if result != UsdBridgeErrorCode::Success {
            return Err(result.into());
        }

        Ok(UsdStageMetadata {
            meters_per_unit: raw.meters_per_unit,
            up_axis: match raw.up_axis {
                UsdBridgeUpAxisRaw::Z => UpAxis::Z,
                _ => UpAxis::Y,
            },
            time_codes_per_second: raw.time_codes_per_second,
        })
    }

    // ========================================================================
    // Timeline / Animation
    // ========================================================================

    /// Get timeline metadata from the stage.
    ///
    /// Returns start/end time codes, FPS, and whether the stage has authored time range.
    pub fn get_timeline(&self) -> UsdBridgeResult<UsdTimelineData> {
        let mut raw_data = UsdBridgeTimelineDataRaw {
            start_time_code: 0.0,
            end_time_code: 0.0,
            frames_per_second: 24.0,
            has_authored_time_range: 0,
        };

        let result = unsafe { usd_bridge_get_timeline(self.raw, &mut raw_data) };

        if result != UsdBridgeErrorCode::Success {
            return Err(result.into());
        }

        Ok(UsdTimelineData {
            start_time_code: raw_data.start_time_code,
            end_time_code: raw_data.end_time_code,
            frames_per_second: raw_data.frames_per_second,
            has_authored_time_range: raw_data.has_authored_time_range != 0,
        })
    }

    /// Get animation data for a mesh by index.
    ///
    /// Returns transform samples if the mesh has animated transforms.
    pub fn get_mesh_animation(&self, mesh_index: usize) -> UsdBridgeResult<UsdAnimatedMeshData> {
        let mut raw_data = UsdBridgeAnimatedMeshDataRaw {
            mesh_index: 0,
            xform_samples: ptr::null(),
            xform_sample_count: 0,
        };

        let result = unsafe { usd_bridge_get_mesh_animation(self.raw, mesh_index, &mut raw_data) };

        if result != UsdBridgeErrorCode::Success {
            return Err(match result {
                UsdBridgeErrorCode::InvalidPrim => {
                    UsdBridgeError::InvalidPrim(format!("mesh index {}", mesh_index))
                }
                other => other.into(),
            });
        }

        let xform_samples = if raw_data.xform_samples.is_null() || raw_data.xform_sample_count == 0
        {
            Vec::new()
        } else {
            unsafe {
                std::slice::from_raw_parts(raw_data.xform_samples, raw_data.xform_sample_count)
                    .iter()
                    .map(|s| TransformSample {
                        time: s.time,
                        transform: Mat4::from_cols_array(&s.transform),
                    })
                    .collect()
            }
        };

        Ok(UsdAnimatedMeshData {
            mesh_index: raw_data.mesh_index,
            xform_samples,
        })
    }

    /// Get animation data for an instancer by index.
    ///
    /// Returns per-instance transforms at each time sample.
    pub fn get_instancer_animation(
        &self,
        instancer_index: usize,
    ) -> UsdBridgeResult<UsdAnimatedInstancerData> {
        let mut raw_data = UsdBridgeAnimatedInstancerDataRaw {
            instancer_index: 0,
            time_samples: ptr::null(),
            time_sample_count: 0,
            instance_count: 0,
            transforms: ptr::null(),
        };

        let result =
            unsafe { usd_bridge_get_instancer_animation(self.raw, instancer_index, &mut raw_data) };

        if result != UsdBridgeErrorCode::Success {
            return Err(match result {
                UsdBridgeErrorCode::InvalidPrim => {
                    UsdBridgeError::InvalidPrim(format!("instancer index {}", instancer_index))
                }
                other => other.into(),
            });
        }

        let time_samples = if raw_data.time_samples.is_null() || raw_data.time_sample_count == 0 {
            Vec::new()
        } else {
            unsafe {
                std::slice::from_raw_parts(raw_data.time_samples, raw_data.time_sample_count)
                    .to_vec()
            }
        };

        let transforms = if raw_data.transforms.is_null()
            || raw_data.time_sample_count == 0
            || raw_data.instance_count == 0
        {
            Vec::new()
        } else {
            let total_matrices = raw_data.time_sample_count * raw_data.instance_count;
            let flat_data =
                unsafe { std::slice::from_raw_parts(raw_data.transforms, total_matrices * 16) };

            // Reshape: [time_sample_count][instance_count] -> Vec<Vec<Mat4>>
            let mut result = Vec::with_capacity(raw_data.time_sample_count);
            for time_idx in 0..raw_data.time_sample_count {
                let mut instances = Vec::with_capacity(raw_data.instance_count);
                for inst_idx in 0..raw_data.instance_count {
                    let offset = (time_idx * raw_data.instance_count + inst_idx) * 16;
                    let mut arr = [0.0f32; 16];
                    arr.copy_from_slice(&flat_data[offset..offset + 16]);
                    instances.push(Mat4::from_cols_array(&arr));
                }
                result.push(instances);
            }
            result
        };

        Ok(UsdAnimatedInstancerData {
            instancer_index: raw_data.instancer_index,
            time_samples,
            instance_count: raw_data.instance_count,
            transforms,
        })
    }

    /// Get animated transform samples for a camera by path.
    ///
    /// Returns transform samples if the camera has animated transforms.
    pub fn get_camera_xform_samples(
        &self,
        camera_path: &str,
    ) -> UsdBridgeResult<Vec<TransformSample>> {
        let c_path = CString::new(camera_path).map_err(|_| UsdBridgeError::InvalidPath)?;
        let mut samples_ptr: *const UsdBridgeXformSampleRaw = ptr::null();
        let mut count: usize = 0;

        let result = unsafe {
            usd_bridge_get_camera_xform_samples(
                self.raw,
                c_path.as_ptr(),
                &mut samples_ptr,
                &mut count,
            )
        };

        if result != UsdBridgeErrorCode::Success {
            return Err(result.into());
        }

        if samples_ptr.is_null() || count == 0 {
            return Ok(Vec::new());
        }

        let samples = unsafe {
            std::slice::from_raw_parts(samples_ptr, count)
                .iter()
                .map(|s| TransformSample {
                    time: s.time,
                    transform: Mat4::from_cols_array(&s.transform),
                })
                .collect()
        };

        Ok(samples)
    }

    /// Get the number of cameras in the stage.
    pub fn camera_count(&self) -> UsdBridgeResult<usize> {
        let mut count: usize = 0;
        let result = unsafe { usd_bridge_get_camera_count(self.raw, &mut count) };
        if result != UsdBridgeErrorCode::Success {
            return Err(result.into());
        }
        Ok(count)
    }

    /// Get all camera paths in the stage.
    pub fn camera_paths(&self) -> UsdBridgeResult<Vec<String>> {
        let count = self.camera_count()?;
        let mut paths = Vec::with_capacity(count);

        for i in 0..count {
            let mut path_ptr: *const std::ffi::c_char = ptr::null();
            let result = unsafe { usd_bridge_get_camera_path(self.raw, i, &mut path_ptr) };
            if result != UsdBridgeErrorCode::Success {
                return Err(result.into());
            }
            if path_ptr.is_null() {
                continue;
            }
            let path_str = unsafe { CStr::from_ptr(path_ptr) }
                .to_str()
                .unwrap_or("")
                .to_string();
            paths.push(path_str);
        }

        Ok(paths)
    }

    /// Get camera transform at a specific time.
    ///
    /// Returns the interpolated world transform matrix for the camera at the given time.
    pub fn get_camera_xform_at_time(&self, camera_path: &str, time: f64) -> UsdBridgeResult<Mat4> {
        let c_path = CString::new(camera_path).map_err(|_| UsdBridgeError::InvalidPath)?;
        let mut transform = [0.0f32; 16];

        let result = unsafe {
            usd_bridge_get_camera_xform_at_time(
                self.raw,
                c_path.as_ptr(),
                time,
                transform.as_mut_ptr(),
            )
        };

        if result != UsdBridgeErrorCode::Success {
            return Err(result.into());
        }

        Ok(Mat4::from_cols_array(&transform))
    }

    /// Get camera lens and clipping properties at a specific time.
    pub fn get_camera_properties(
        &self,
        camera_path: &str,
        time: f64,
    ) -> UsdBridgeResult<CameraProperties> {
        let c_path = CString::new(camera_path).map_err(|_| UsdBridgeError::InvalidPath)?;
        let mut props = UsdBridgeCameraPropertiesRaw {
            focal_length: 0.0,
            vertical_aperture: 0.0,
            clip_near: 0.0,
            clip_far: 0.0,
            horizontal_aperture: 0.0,
        };

        let result = unsafe {
            usd_bridge_get_camera_properties(self.raw, c_path.as_ptr(), time, &mut props)
        };

        if result != UsdBridgeErrorCode::Success {
            return Err(result.into());
        }

        Ok(CameraProperties {
            focal_length: props.focal_length,
            vertical_aperture: props.vertical_aperture,
            clip_near: props.clip_near,
            clip_far: props.clip_far,
            horizontal_aperture: props.horizontal_aperture,
        })
    }

    // ========================================================================
    // Vertex Animation (Point Deformation)
    // ========================================================================

    /// Check if a mesh has animated vertices (point deformation).
    ///
    /// Returns the time samples if animated, or empty vec if static.
    pub fn get_mesh_vertex_animation_times(&self, mesh_index: usize) -> UsdBridgeResult<Vec<f64>> {
        let mut info = UsdBridgeVertexAnimationInfoRaw {
            has_animated_vertices: 0,
            time_sample_count: 0,
            time_samples: ptr::null(),
        };

        let result =
            unsafe { usd_bridge_get_mesh_vertex_animation_info(self.raw, mesh_index, &mut info) };

        if result != UsdBridgeErrorCode::Success {
            return Err(result.into());
        }

        if info.has_animated_vertices == 0 || info.time_samples.is_null() {
            return Ok(Vec::new());
        }

        let times = unsafe {
            std::slice::from_raw_parts(info.time_samples, info.time_sample_count).to_vec()
        };

        Ok(times)
    }

    /// Get mesh vertices at a specific time.
    ///
    /// For animated meshes, this queries the USD stage for interpolated vertices.
    /// For static meshes, returns the cached vertices.
    pub fn get_mesh_vertices_at_time(
        &self,
        mesh_index: usize,
        time: f64,
    ) -> UsdBridgeResult<Vec<f32>> {
        let mut vertices_ptr: *const f32 = ptr::null();
        let mut vertex_count: usize = 0;

        let result = unsafe {
            usd_bridge_get_mesh_vertices_at_time(
                self.raw,
                mesh_index,
                time,
                &mut vertices_ptr,
                &mut vertex_count,
            )
        };

        if result != UsdBridgeErrorCode::Success {
            return Err(result.into());
        }

        if vertices_ptr.is_null() || vertex_count == 0 {
            return Ok(Vec::new());
        }

        // Copy the vertices (C++ uses a temporary buffer that may be reused)
        let vertices =
            unsafe { std::slice::from_raw_parts(vertices_ptr, vertex_count * 3).to_vec() };

        Ok(vertices)
    }

    // ========================================================================
    // Prim Traversal (Scene Browser Support)
    // ========================================================================

    /// Get the total number of prims in the stage.
    pub fn prim_count(&self) -> UsdBridgeResult<usize> {
        let mut count: usize = 0;
        let result = unsafe { usd_bridge_get_prim_count(self.raw, &mut count) };

        if result != UsdBridgeErrorCode::Success {
            return Err(result.into());
        }

        Ok(count)
    }

    /// Get prim info by index (depth-first traversal order).
    pub fn get_prim_info(&self, index: usize) -> UsdBridgeResult<UsdPrimInfo> {
        let mut raw_info = UsdBridgePrimInfoRaw {
            path: ptr::null(),
            type_name: ptr::null(),
            is_active: 0,
            has_children: 0,
            child_count: 0,
            visibility: 1,
            has_payload: 0,
            is_loaded: 1,
            variant_set_count: 0,
            has_inherits: 0,
            has_specializes: 0,
        };

        let result = unsafe { usd_bridge_get_prim_info(self.raw, index, &mut raw_info) };

        if result != UsdBridgeErrorCode::Success {
            return Err(match result {
                UsdBridgeErrorCode::InvalidPrim => {
                    UsdBridgeError::InvalidPrim(format!("prim index {}", index))
                }
                other => other.into(),
            });
        }

        Self::convert_prim_info(&raw_info)
    }

    /// Get prim info by path.
    pub fn get_prim_info_by_path(&self, path: &str) -> UsdBridgeResult<UsdPrimInfo> {
        let c_path = CString::new(path).map_err(|_| UsdBridgeError::InvalidPath)?;
        let mut raw_info = UsdBridgePrimInfoRaw {
            path: ptr::null(),
            type_name: ptr::null(),
            is_active: 0,
            has_children: 0,
            child_count: 0,
            visibility: 1,
            has_payload: 0,
            is_loaded: 1,
            variant_set_count: 0,
            has_inherits: 0,
            has_specializes: 0,
        };

        let result =
            unsafe { usd_bridge_get_prim_info_by_path(self.raw, c_path.as_ptr(), &mut raw_info) };

        if result != UsdBridgeErrorCode::Success {
            return Err(match result {
                UsdBridgeErrorCode::InvalidPrim => {
                    UsdBridgeError::InvalidPrim(format!("prim path {}", path))
                }
                other => other.into(),
            });
        }

        Self::convert_prim_info(&raw_info)
    }

    /// Get root prim paths (direct children of pseudo-root).
    pub fn root_prim_paths(&self) -> UsdBridgeResult<Vec<String>> {
        let mut count: usize = 0;
        let result = unsafe { usd_bridge_get_root_prim_count(self.raw, &mut count) };

        if result != UsdBridgeErrorCode::Success {
            return Err(result.into());
        }

        let mut paths = Vec::with_capacity(count);
        for i in 0..count {
            let mut path_ptr: *const std::ffi::c_char = ptr::null();
            let result = unsafe { usd_bridge_get_root_prim_path(self.raw, i, &mut path_ptr) };

            if result != UsdBridgeErrorCode::Success {
                return Err(result.into());
            }

            let path = unsafe {
                if path_ptr.is_null() {
                    String::new()
                } else {
                    CStr::from_ptr(path_ptr).to_string_lossy().into_owned()
                }
            };
            paths.push(path);
        }

        Ok(paths)
    }

    /// Get child prim paths for a given parent path.
    ///
    /// Pass "/" or empty string for root prims.
    pub fn child_prim_paths(&self, parent_path: &str) -> UsdBridgeResult<Vec<String>> {
        let c_path = CString::new(parent_path).map_err(|_| UsdBridgeError::InvalidPath)?;

        let mut count: usize = 0;
        let result =
            unsafe { usd_bridge_get_children_count(self.raw, c_path.as_ptr(), &mut count) };

        if result != UsdBridgeErrorCode::Success {
            return Err(match result {
                UsdBridgeErrorCode::InvalidPrim => {
                    UsdBridgeError::InvalidPrim(format!("parent path {}", parent_path))
                }
                other => other.into(),
            });
        }

        let mut paths = Vec::with_capacity(count);
        for i in 0..count {
            let mut path_ptr: *const std::ffi::c_char = ptr::null();
            let result =
                unsafe { usd_bridge_get_child_path(self.raw, c_path.as_ptr(), i, &mut path_ptr) };

            if result != UsdBridgeErrorCode::Success {
                return Err(result.into());
            }

            let path = unsafe {
                if path_ptr.is_null() {
                    String::new()
                } else {
                    CStr::from_ptr(path_ptr).to_string_lossy().into_owned()
                }
            };
            paths.push(path);
        }

        Ok(paths)
    }

    /// Get all prims in the stage (depth-first order).
    pub fn all_prims(&self) -> UsdBridgeResult<Vec<UsdPrimInfo>> {
        let count = self.prim_count()?;
        let mut prims = Vec::with_capacity(count);
        for i in 0..count {
            prims.push(self.get_prim_info(i)?);
        }
        Ok(prims)
    }

    /// Helper to convert raw prim info to Rust type.
    fn convert_prim_info(raw: &UsdBridgePrimInfoRaw) -> UsdBridgeResult<UsdPrimInfo> {
        let path = unsafe {
            if raw.path.is_null() {
                String::new()
            } else {
                CStr::from_ptr(raw.path).to_string_lossy().into_owned()
            }
        };

        let type_name = unsafe {
            if raw.type_name.is_null() {
                String::new()
            } else {
                CStr::from_ptr(raw.type_name).to_string_lossy().into_owned()
            }
        };

        Ok(UsdPrimInfo {
            path,
            type_name,
            is_active: raw.is_active != 0,
            has_children: raw.has_children != 0,
            child_count: raw.child_count,
            visible: raw.visibility != 0,
            has_payload: raw.has_payload != 0,
            is_loaded: raw.is_loaded != 0,
            variant_set_count: raw.variant_set_count,
            has_inherits: raw.has_inherits != 0,
            has_specializes: raw.has_specializes != 0,
        })
    }
}

impl Drop for UsdStage {
    fn drop(&mut self) {
        if !self.raw.is_null() {
            unsafe {
                usd_bridge_close_stage(self.raw);
            }
        }
    }
}

// ============================================================================
// Edit Layer (for exporting transform overrides)
// ============================================================================

/// A writable USD stage for exporting edit opinions.
pub struct UsdEditLayer {
    raw: *mut UsdBridgeEditLayerRaw,
}

// SAFETY: UsdEditLayer is Send because:
// 1. The raw pointer is exclusively owned — only one UsdEditLayer instance
//    holds each C++ edit layer pointer (no Clone impl, no pointer aliasing).
// 2. All mutating FFI calls (define_prim, set_attribute, etc.) take &mut self,
//    so Rust's borrow checker guarantees exclusive access at compile time.
// 3. The C++ edit layer is self-contained — it does not reference shared
//    global state or other stage data that could cause races when moved
//    between threads.
// 4. Not Sync: concurrent &UsdEditLayer access from multiple threads is not
//    needed and not claimed safe.
unsafe impl Send for UsdEditLayer {}

impl UsdEditLayer {
    /// Create a new edit layer at the given output path.
    pub fn create(output_path: &str) -> UsdBridgeResult<Self> {
        let c_path = CString::new(output_path).map_err(|_| UsdBridgeError::InvalidPath)?;
        let mut raw: *mut UsdBridgeEditLayerRaw = std::ptr::null_mut();
        let code = unsafe { usd_bridge_create_edit_layer(c_path.as_ptr(), &mut raw) };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(Self { raw })
    }

    /// Write a transform opinion at the given prim path and time.
    ///
    /// Use `time = -1.0` for default (static) time.
    pub fn write_xform(
        &mut self,
        prim_path: &str,
        time: f64,
        matrix: &bif_math::Mat4,
    ) -> UsdBridgeResult<()> {
        let c_path = CString::new(prim_path).map_err(|_| UsdBridgeError::InvalidPath)?;
        let cols = matrix.to_cols_array();
        let code = unsafe {
            usd_bridge_write_xform_opinion(self.raw, c_path.as_ptr(), time, cols.as_ptr())
        };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(())
    }

    /// Add a sublayer to this edit layer (for composing over original USD).
    pub fn add_sublayer(&mut self, sublayer_path: &str) -> UsdBridgeResult<()> {
        let c_path = CString::new(sublayer_path).map_err(|_| UsdBridgeError::InvalidPath)?;
        let code = unsafe { usd_bridge_edit_layer_add_sublayer(self.raw, c_path.as_ptr()) };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(())
    }

    /// Add a reference on a prim.
    pub fn add_reference(
        &mut self,
        prim_path: &str,
        reference_file: &str,
        reference_prim_path: Option<&str>,
    ) -> UsdBridgeResult<()> {
        let c_prim = CString::new(prim_path).map_err(|_| UsdBridgeError::InvalidPath)?;
        let c_file = CString::new(reference_file).map_err(|_| UsdBridgeError::InvalidPath)?;
        let c_ref_prim = reference_prim_path
            .map(|p| CString::new(p).map_err(|_| UsdBridgeError::InvalidPath))
            .transpose()?;
        let ref_prim_ptr = c_ref_prim
            .as_ref()
            .map(|c| c.as_ptr())
            .unwrap_or(ptr::null());
        let code = unsafe {
            usd_bridge_edit_layer_add_reference(
                self.raw,
                c_prim.as_ptr(),
                c_file.as_ptr(),
                ref_prim_ptr,
            )
        };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(())
    }

    /// Set the default prim on the stage.
    pub fn set_default_prim(&mut self, prim_path: &str) -> UsdBridgeResult<()> {
        let c_path = CString::new(prim_path).map_err(|_| UsdBridgeError::InvalidPath)?;
        let code = unsafe { usd_bridge_edit_layer_set_default_prim(self.raw, c_path.as_ptr()) };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(())
    }

    /// Write a PointInstancer prim from a point cloud.
    pub fn write_point_instancer(
        &mut self,
        prim_path: &str,
        cloud: &crate::point_cloud::PointCloud,
        proto_paths: &[String],
    ) -> UsdBridgeResult<()> {
        let c_prim = CString::new(prim_path).map_err(|_| UsdBridgeError::InvalidPath)?;

        // Flatten positions to f32 array
        let positions: Vec<f32> = cloud
            .positions
            .iter()
            .flat_map(|p| [p.x, p.y, p.z])
            .collect();
        let count = cloud.positions.len();

        // Validate parallel arrays match positions length
        if let Some(ref orients) = cloud.attributes.orientations {
            if orients.len() != count {
                return Err(UsdBridgeError::InvalidPrim(format!(
                    "orientations len {} != positions len {}",
                    orients.len(),
                    count
                )));
            }
        }
        if let Some(ref s) = cloud.attributes.scales {
            if s.len() != count {
                return Err(UsdBridgeError::InvalidPrim(format!(
                    "scales len {} != positions len {}",
                    s.len(),
                    count
                )));
            }
        }
        if cloud.attributes.proto_indices.len() != count {
            return Err(UsdBridgeError::InvalidPrim(format!(
                "proto_indices len {} != positions len {}",
                cloud.attributes.proto_indices.len(),
                count
            )));
        }

        // Flatten orientations (wxyz) if available
        let orientations: Option<Vec<f32>> =
            cloud.attributes.orientations.as_ref().map(|orients| {
                orients
                    .iter()
                    .flat_map(|q| {
                        // glam Quat is (x, y, z, w) internally, USD wants (w, x, y, z)
                        [q.w, q.x, q.y, q.z]
                    })
                    .collect()
            });

        // Flatten scales if available
        let scales: Option<Vec<f32>> = cloud
            .attributes
            .scales
            .as_ref()
            .map(|s| s.iter().flat_map(|v| [v.x, v.y, v.z]).collect());

        // Proto indices (convert u32 -> i32, checked)
        let proto_indices: Vec<i32> = cloud
            .attributes
            .proto_indices
            .iter()
            .map(|&i| {
                i32::try_from(i).map_err(|_| {
                    UsdBridgeError::InvalidPrim(format!("proto_index {} overflows i32", i))
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        // Build C string array for prototype paths
        let c_proto_paths: Vec<CString> = proto_paths
            .iter()
            .map(|p| CString::new(p.as_str()).map_err(|_| UsdBridgeError::InvalidPath))
            .collect::<Result<Vec<_>, _>>()?;
        let c_proto_ptrs: Vec<*const std::ffi::c_char> =
            c_proto_paths.iter().map(|c| c.as_ptr()).collect();

        let code = unsafe {
            usd_bridge_write_point_instancer(
                self.raw,
                c_prim.as_ptr(),
                positions.as_ptr(),
                orientations
                    .as_ref()
                    .map(|o| o.as_ptr())
                    .unwrap_or(ptr::null()),
                scales.as_ref().map(|s| s.as_ptr()).unwrap_or(ptr::null()),
                proto_indices.as_ptr(),
                count,
                c_proto_ptrs.as_ptr(),
                c_proto_ptrs.len(),
            )
        };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(())
    }

    /// Write a UsdGeomMesh prim from a `Mesh`.
    pub fn write_mesh(&mut self, prim_path: &str, mesh: &crate::mesh::Mesh) -> UsdBridgeResult<()> {
        let c_path = CString::new(prim_path).map_err(|_| UsdBridgeError::InvalidPath)?;

        let points: Vec<f32> = mesh
            .positions
            .iter()
            .flat_map(|p| [p.x, p.y, p.z])
            .collect();

        let normals_flat: Vec<f32> = mesh
            .normals
            .as_ref()
            .map(|ns| ns.iter().flat_map(|n| [n.x, n.y, n.z]).collect())
            .unwrap_or_default();

        let uvs_flat: Vec<f32> = mesh
            .uvs
            .as_ref()
            .map(|uvs| uvs.iter().flat_map(|uv| [uv[0], uv[1]]).collect())
            .unwrap_or_default();

        let code = unsafe {
            usd_bridge_write_mesh(
                self.raw,
                c_path.as_ptr(),
                points.as_ptr(),
                mesh.positions.len(),
                mesh.indices.as_ptr(),
                mesh.indices.len(),
                if normals_flat.is_empty() {
                    ptr::null()
                } else {
                    normals_flat.as_ptr()
                },
                normals_flat.len() / 3,
                if uvs_flat.is_empty() {
                    ptr::null()
                } else {
                    uvs_flat.as_ptr()
                },
                uvs_flat.len() / 2,
            )
        };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(())
    }

    /// Define or override a prim at the given path.
    pub fn define_prim(
        &mut self,
        path: &str,
        prim_type: UsdPrimType,
        specifier: UsdSpecifier,
    ) -> UsdBridgeResult<()> {
        let c_path = CString::new(path).map_err(|_| UsdBridgeError::InvalidPath)?;
        let type_name = prim_type.as_usd_type_name();
        let c_type = CString::new(type_name).map_err(|_| UsdBridgeError::InvalidPath)?;
        let spec_raw = match specifier {
            UsdSpecifier::Define => UsdBridgeSpecifierRaw::Define,
            UsdSpecifier::Over => UsdBridgeSpecifierRaw::Over,
        };
        let code =
            unsafe { usd_bridge_define_prim(self.raw, c_path.as_ptr(), c_type.as_ptr(), spec_raw) };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(())
    }

    /// Set the model kind on a prim (must already exist).
    pub fn set_prim_kind(&mut self, path: &str, kind: UsdKind) -> UsdBridgeResult<()> {
        let c_path = CString::new(path).map_err(|_| UsdBridgeError::InvalidPath)?;
        let kind_raw = match kind {
            UsdKind::None => UsdBridgeKindRaw::None,
            UsdKind::Component => UsdBridgeKindRaw::Component,
            UsdKind::Group => UsdBridgeKindRaw::Group,
            UsdKind::Assembly => UsdBridgeKindRaw::Assembly,
            UsdKind::Subcomponent => UsdBridgeKindRaw::Subcomponent,
        };
        let code = unsafe { usd_bridge_set_prim_kind(self.raw, c_path.as_ptr(), kind_raw) };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(())
    }

    /// Write a material (UsdPreviewSurface + OpenPBR MaterialX).
    #[allow(clippy::too_many_arguments)]
    /// Add a payload arc on a prim.
    pub fn add_payload(
        &mut self,
        prim_path: &str,
        asset_path: &str,
        target_path: Option<&str>,
    ) -> UsdBridgeResult<()> {
        let c_prim = CString::new(prim_path).map_err(|_| UsdBridgeError::InvalidPath)?;
        let c_asset = CString::new(asset_path).map_err(|_| UsdBridgeError::InvalidPath)?;
        let c_target = target_path.and_then(|s| CString::new(s).ok());
        let code = unsafe {
            usd_bridge_edit_layer_add_payload(
                self.raw,
                c_prim.as_ptr(),
                c_asset.as_ptr(),
                c_target.as_ref().map_or(ptr::null(), |s| s.as_ptr()),
            )
        };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(())
    }

    pub fn write_material(
        &mut self,
        mat_path: &str,
        material: &crate::scene::Material,
    ) -> UsdBridgeResult<()> {
        let c_path = CString::new(mat_path).map_err(|_| UsdBridgeError::InvalidPath)?;
        let diffuse = [
            material.diffuse_color.x,
            material.diffuse_color.y,
            material.diffuse_color.z,
        ];
        let emissive = [
            material.emissive_color.x,
            material.emissive_color.y,
            material.emissive_color.z,
        ];

        let diffuse_tex = material
            .diffuse_texture
            .as_deref()
            .and_then(|s| CString::new(s.as_bytes()).ok());
        let roughness_tex = material
            .roughness_texture
            .as_deref()
            .and_then(|s| CString::new(s.as_bytes()).ok());
        let metallic_tex = material
            .metallic_texture
            .as_deref()
            .and_then(|s| CString::new(s.as_bytes()).ok());
        let normal_tex = material
            .normal_texture
            .as_deref()
            .and_then(|s| CString::new(s.as_bytes()).ok());
        let emissive_tex = material
            .emissive_texture
            .as_deref()
            .and_then(|s| CString::new(s.as_bytes()).ok());

        let code = unsafe {
            usd_bridge_write_material(
                self.raw,
                c_path.as_ptr(),
                diffuse.as_ptr(),
                material.metallic,
                material.roughness,
                material.specular,
                material.opacity,
                emissive.as_ptr(),
                diffuse_tex.as_ref().map_or(ptr::null(), |s| s.as_ptr()),
                roughness_tex.as_ref().map_or(ptr::null(), |s| s.as_ptr()),
                metallic_tex.as_ref().map_or(ptr::null(), |s| s.as_ptr()),
                normal_tex.as_ref().map_or(ptr::null(), |s| s.as_ptr()),
                emissive_tex.as_ref().map_or(ptr::null(), |s| s.as_ptr()),
            )
        };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(())
    }

    /// Bind a material to a prim.
    pub fn bind_material(&mut self, prim_path: &str, material_path: &str) -> UsdBridgeResult<()> {
        let c_prim = CString::new(prim_path).map_err(|_| UsdBridgeError::InvalidPath)?;
        let c_mat = CString::new(material_path).map_err(|_| UsdBridgeError::InvalidPath)?;
        let code = unsafe { usd_bridge_bind_material(self.raw, c_prim.as_ptr(), c_mat.as_ptr()) };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(())
    }

    /// Write visibility attribute on a prim.
    pub fn write_visibility(&mut self, prim_path: &str, visible: bool) -> UsdBridgeResult<()> {
        let c_path = CString::new(prim_path).map_err(|_| UsdBridgeError::InvalidPath)?;
        let code = unsafe {
            usd_bridge_write_visibility(self.raw, c_path.as_ptr(), if visible { 1 } else { 0 })
        };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(())
    }

    /// Set stage metadata (metersPerUnit, upAxis, timeCodesPerSecond).
    pub fn set_stage_metadata(&mut self, metadata: &UsdStageMetadata) -> UsdBridgeResult<()> {
        let up_axis = match metadata.up_axis {
            UpAxis::Z => 1,
            UpAxis::Y => 0,
        };
        let code = unsafe {
            usd_bridge_set_stage_metadata(
                self.raw,
                metadata.meters_per_unit,
                up_axis,
                metadata.time_codes_per_second,
            )
        };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(())
    }

    /// Write a UsdGeomCamera prim. Call multiple times at different `time` for animation.
    #[allow(clippy::too_many_arguments)]
    pub fn write_camera(
        &mut self,
        path: &str,
        props: &CameraProperties,
        time: f64,
        transform: &Mat4,
    ) -> UsdBridgeResult<()> {
        let c_path = CString::new(path).map_err(|_| UsdBridgeError::InvalidPath)?;
        let xform = transform.to_cols_array();
        let code = unsafe {
            usd_bridge_write_camera(
                self.raw,
                c_path.as_ptr(),
                props.focal_length,
                props.horizontal_aperture,
                props.vertical_aperture,
                props.clip_near,
                props.clip_far,
                time,
                xform.as_ptr(),
            )
        };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(())
    }

    /// Write a UsdLux light prim with type-specific properties.
    #[allow(clippy::too_many_arguments)]
    pub fn write_light(&mut self, light: &UsdLightData) -> UsdBridgeResult<()> {
        let c_path = CString::new(light.path.as_str()).map_err(|_| UsdBridgeError::InvalidPath)?;
        let color = [light.color.x, light.color.y, light.color.z];
        let xform = light.transform.to_cols_array();
        let light_type = match light.light_type {
            UsdLightType::Distant => UsdBridgeLightType::Distant,
            UsdLightType::Sphere => UsdBridgeLightType::Sphere,
            UsdLightType::Rect => UsdBridgeLightType::Rect,
            UsdLightType::Dome => UsdBridgeLightType::Dome,
            UsdLightType::Cylinder => UsdBridgeLightType::Cylinder,
            UsdLightType::Disk => UsdBridgeLightType::Disk,
        };
        let tex_path = light
            .texture_path
            .as_deref()
            .and_then(|s| CString::new(s.as_bytes()).ok());

        let code = unsafe {
            usd_bridge_write_light(
                self.raw,
                c_path.as_ptr(),
                light_type,
                color.as_ptr(),
                light.intensity,
                0.0, // exposure already baked into intensity on read
                xform.as_ptr(),
                light.angle,
                light.radius,
                light.width,
                light.height,
                light.length,
                tex_path.as_ref().map_or(ptr::null(), |s| s.as_ptr()),
                light.shaping.cone_angle,
                light.shaping.cone_softness,
                light.shaping.focus,
            )
        };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(())
    }

    /// Write UsdRenderSettings prim.
    pub fn write_render_settings(
        &mut self,
        path: &str,
        resolution: (u32, u32),
        camera_path: Option<&str>,
        pixel_aspect_ratio: f32,
    ) -> UsdBridgeResult<()> {
        let c_path = CString::new(path).map_err(|_| UsdBridgeError::InvalidPath)?;
        let c_cam = camera_path.and_then(|s| CString::new(s).ok());
        let code = unsafe {
            usd_bridge_write_render_settings(
                self.raw,
                c_path.as_ptr(),
                resolution.0 as i32,
                resolution.1 as i32,
                c_cam.as_ref().map_or(ptr::null(), |s| s.as_ptr()),
                pixel_aspect_ratio,
            )
        };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(())
    }

    /// Save the edit layer to disk, then free the handle.
    pub fn save(mut self) -> UsdBridgeResult<()> {
        let code = unsafe { usd_bridge_save_edit_layer(self.raw) };
        // Null out pointer so Drop won't double-free (C++ side frees the handle)
        self.raw = std::ptr::null_mut();
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(())
    }
}

impl Drop for UsdEditLayer {
    fn drop(&mut self) {
        if !self.raw.is_null() {
            // Discard unsaved changes, just free the handle
            unsafe {
                usd_bridge_free_edit_layer(self.raw);
            }
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fov_from_focal_length() {
        // USD default: 50mm focal, 24.89mm vertical aperture → ~27.9° vertical FOV
        let props = CameraProperties {
            focal_length: 50.0,
            vertical_aperture: 24.89,
            clip_near: 0.1,
            clip_far: 10000.0,
        };
        let fov_deg = props.fov_y().to_degrees();
        assert!(
            (fov_deg - 27.93).abs() < 0.1,
            "Expected ~27.93° FOV, got {fov_deg:.2}°"
        );
    }

    #[test]
    fn test_fov_wide_lens() {
        // 24mm wide lens → should be >50° FOV
        let props = CameraProperties {
            focal_length: 24.0,
            vertical_aperture: 24.89,
            clip_near: 1.0,
            clip_far: 100000.0,
        };
        let fov_deg = props.fov_y().to_degrees();
        assert!(
            fov_deg > 50.0,
            "24mm lens should give >50° FOV, got {fov_deg:.2}°"
        );
    }

    #[test]
    fn test_error_codes() {
        // Verify error conversion doesn't panic
        let _ = UsdBridgeError::from(UsdBridgeErrorCode::NullPointer);
        let _ = UsdBridgeError::from(UsdBridgeErrorCode::FileNotFound);
        let _ = UsdBridgeError::from(UsdBridgeErrorCode::InvalidStage);
    }

    /// Test loading USD file with relative references (Xform refs).
    /// lucy_100.usda has 100 Xform prims each referencing @./lucy_low.usda@
    #[test]
    fn test_load_relative_reference_usda() {
        let path = "../../assets/lucy_100.usda";
        let stage = match UsdStage::open(path) {
            Ok(s) => s,
            Err(e) => {
                // Skip test if USD library not available or file not found
                eprintln!("Skipping test - could not open stage: {e}");
                return;
            }
        };

        // Should have 100 meshes (one lucy_low mesh per Xform reference)
        let mesh_count = stage.mesh_count().expect("mesh_count failed");
        assert_eq!(
            mesh_count, 100,
            "Expected 100 meshes from lucy_100.usda, got {mesh_count}"
        );

        // Verify first mesh has vertices
        if mesh_count > 0 {
            let mesh = stage.get_mesh(0).expect("get_mesh(0) failed");
            assert!(
                !mesh.vertices.is_empty(),
                "First mesh should have vertices from referenced lucy_low.usda"
            );
            assert!(
                mesh.vertices.len() > 100,
                "First mesh should have substantial geometry"
            );
        }
    }

    /// Test loading PointInstancer with external prototype reference.
    /// lucy_100_fixed.usda has a PointInstancer with prototype referencing @./lucy_low.usda@
    #[test]
    fn test_load_pointinstancer_external_prototype() {
        let path = "../../assets/lucy_100_fixed.usda";
        let stage = match UsdStage::open(path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Skipping test - could not open stage: {e}");
                return;
            }
        };

        // Should have 1 instancer
        let instancer_count = stage.instancer_count().expect("instancer_count failed");
        assert_eq!(
            instancer_count, 1,
            "Expected 1 instancer from lucy_100_fixed.usda"
        );

        // Instancer should have 100 instances
        let instancer = stage.get_instancer(0).expect("get_instancer(0) failed");
        assert_eq!(
            instancer.transforms.len(),
            100,
            "Expected 100 instances, got {}",
            instancer.transforms.len()
        );
        assert_eq!(
            instancer.prototype_paths.len(),
            1,
            "Expected 1 prototype path"
        );

        assert!(
            !instancer.prototype_paths.is_empty(),
            "Instancer should have at least one prototype path"
        );

        // Should have 1 mesh (the lucy prototype)
        let mesh_count = stage.mesh_count().expect("mesh_count failed");
        assert!(
            mesh_count >= 1,
            "Expected at least 1 mesh (prototype), got {mesh_count}"
        );

        // Verify prototype mesh has vertices
        let mesh = stage.get_mesh(0).expect("get_mesh(0) failed");
        assert!(
            !mesh.vertices.is_empty(),
            "Prototype mesh should have vertices from referenced lucy_low.usda"
        );
    }
}
