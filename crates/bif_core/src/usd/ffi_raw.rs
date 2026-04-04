//! Raw FFI declarations for the USD C++ bridge. All types and functions are pub(crate).

#![allow(dead_code)]

use std::ffi::c_char;

/// Opaque stage handle (matches C struct)
#[repr(C)]
pub(crate) struct UsdBridgeStageRaw {
    _private: [u8; 0],
}

/// Opaque edit layer handle (matches C struct)
#[repr(C)]
pub(crate) struct UsdBridgeEditLayerRaw {
    _private: [u8; 0],
}

/// Error codes from C API
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum UsdBridgeErrorCode {
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
pub(crate) enum UsdBridgePurposeRaw {
    Default = 0,
    Render = 1,
    Proxy = 2,
    Guide = 3,
}

/// Mesh data from C API
#[repr(C)]
pub(crate) struct UsdBridgeMeshDataRaw {
    pub(crate) path: *const c_char,
    pub(crate) vertices: *const f32,
    pub(crate) vertex_count: usize,
    pub(crate) indices: *const u32,
    pub(crate) index_count: usize,
    pub(crate) normals: *const f32,
    pub(crate) normal_count: usize,
    pub(crate) uvs: *const f32,
    pub(crate) uv_count: usize,
    pub(crate) face_material_ids: *const u32,
    pub(crate) triangle_count: usize,
    pub(crate) transform: [f32; 16],
    pub(crate) purpose: UsdBridgePurposeRaw,
    pub(crate) is_instance_proxy: i32,
    pub(crate) visibility: i32,
    pub(crate) double_sided: i32,
    pub(crate) subdivision_scheme: *const c_char,
    pub(crate) normals_interpolation: i32,
    pub(crate) display_color: *const f32,
    pub(crate) display_color_count: usize,
    pub(crate) display_opacity: f32,
    pub(crate) resets_xform_stack: i32,
    pub(crate) face_vertex_counts: *const i32,
    pub(crate) face_count: usize,
    pub(crate) face_vertex_indices: *const i32,
    pub(crate) face_vertex_index_count: usize,
    pub(crate) crease_indices: *const i32,
    pub(crate) crease_index_count: usize,
    pub(crate) crease_lengths: *const i32,
    pub(crate) crease_length_count: usize,
    pub(crate) crease_sharpnesses: *const f32,
    pub(crate) crease_sharpness_count: usize,
    pub(crate) vertices_orig: *const f32,
    pub(crate) vertex_count_orig: usize,
}

/// Prim attribute data from C API (returned by usd_bridge_get_prim_attributes)
#[repr(C)]
pub(crate) struct UsdBridgeAttributeDataRaw {
    pub(crate) name: *const c_char,
    pub(crate) type_name: *const c_char,
    pub(crate) value_str: *const c_char,
    pub(crate) is_primvar: i32,
    pub(crate) interpolation: *const c_char,
    pub(crate) is_authored: i32,
}

/// Native instance data from C API
#[repr(C)]
pub(crate) struct UsdNativeInstanceDataRaw {
    pub(crate) proto_mesh_idx: i32,
    pub(crate) transform: [f32; 16],
    pub(crate) material_override_idx: i32,
    pub(crate) purpose: i32,
}

/// Instancer data from C API
#[repr(C)]
pub(crate) struct UsdBridgeInstancerDataRaw {
    pub(crate) path: *const c_char,
    pub(crate) prototype_paths: *const *const c_char,
    pub(crate) prototype_count: usize,
    pub(crate) transforms: *const f32,
    pub(crate) instance_count: usize,
    pub(crate) proto_indices: *const i32,
    pub(crate) velocities: *const f32,
    pub(crate) velocity_count: usize,
    pub(crate) angular_velocities: *const f32,
    pub(crate) angular_velocity_count: usize,
    pub(crate) invisible_ids: *const i64,
    pub(crate) invisible_id_count: usize,
}

/// Prim info from C API (for scene browser)
#[repr(C)]
pub(crate) struct UsdBridgePrimInfoRaw {
    pub(crate) path: *const c_char,
    pub(crate) type_name: *const c_char,
    pub(crate) is_active: i32,
    pub(crate) has_children: i32,
    pub(crate) child_count: usize,
    pub(crate) visibility: i32,
    pub(crate) has_payload: i32,
    pub(crate) is_loaded: i32,
    pub(crate) variant_set_count: usize,
    pub(crate) has_inherits: i32,
    pub(crate) has_specializes: i32,
}

/// Timeline data from C API
#[repr(C)]
pub(crate) struct UsdBridgeTimelineDataRaw {
    pub(crate) start_time_code: f64,
    pub(crate) end_time_code: f64,
    pub(crate) frames_per_second: f64,
    pub(crate) has_authored_time_range: i32,
}

/// A single transform sample at a specific time
#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct UsdBridgeXformSampleRaw {
    pub(crate) time: f64,
    pub(crate) transform: [f32; 16],
}

/// Animated mesh data from C API
#[repr(C)]
pub(crate) struct UsdBridgeAnimatedMeshDataRaw {
    pub(crate) mesh_index: usize,
    pub(crate) xform_samples: *const UsdBridgeXformSampleRaw,
    pub(crate) xform_sample_count: usize,
}

/// Animated instancer data from C API
#[repr(C)]
pub(crate) struct UsdBridgeAnimatedInstancerDataRaw {
    pub(crate) instancer_index: usize,
    pub(crate) time_samples: *const f64,
    pub(crate) time_sample_count: usize,
    pub(crate) instance_count: usize,
    pub(crate) transforms: *const f32,
}

/// Vertex animation info from C API
#[repr(C)]
pub(crate) struct UsdBridgeVertexAnimationInfoRaw {
    pub(crate) has_animated_vertices: i32,
    pub(crate) time_sample_count: usize,
    pub(crate) time_samples: *const f64,
}

/// Camera properties from C API
#[repr(C)]
pub(crate) struct UsdBridgeCameraPropertiesRaw {
    pub(crate) focal_length: f32,
    pub(crate) vertical_aperture: f32,
    pub(crate) clip_near: f32,
    pub(crate) clip_far: f32,
    pub(crate) horizontal_aperture: f32,
}

/// Light type enumeration from C API
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UsdBridgeLightType {
    Distant = 0,
    Sphere = 1,
    Rect = 2,
    Dome = 3,
    Cylinder = 4,
    Disk = 5,
}

/// Light data from C API
#[repr(C)]
pub(crate) struct UsdBridgeLightDataRaw {
    pub(crate) path: *const c_char,
    pub(crate) light_type: UsdBridgeLightType,
    pub(crate) color: [f32; 3],
    pub(crate) intensity: f32,
    pub(crate) exposure: f32,
    pub(crate) transform: [f32; 16],
    pub(crate) angle: f32,
    pub(crate) radius: f32,
    pub(crate) width: f32,
    pub(crate) height: f32,
    pub(crate) texture_path: *const c_char,
    pub(crate) length: f32,
    pub(crate) shaping_cone_angle: f32,
    pub(crate) shaping_cone_softness: f32,
    pub(crate) shaping_focus: f32,
    pub(crate) shaping_ies_file: *const c_char,
    pub(crate) light_link_includes: *const *const c_char,
    pub(crate) light_link_include_count: usize,
    pub(crate) light_link_excludes: *const *const c_char,
    pub(crate) light_link_exclude_count: usize,
}

/// Skeleton data from C API
#[repr(C)]
pub(crate) struct UsdBridgeSkeletonDataRaw {
    pub(crate) path: *const c_char,
    pub(crate) joint_paths: *const *const c_char,
    pub(crate) joint_count: usize,
    pub(crate) bind_transforms: *const f32,
    pub(crate) rest_transforms: *const f32,
}

/// Skin binding data from C API
#[repr(C)]
pub(crate) struct UsdBridgeSkinBindingDataRaw {
    pub(crate) mesh_path: *const c_char,
    pub(crate) skeleton_path: *const c_char,
    pub(crate) joint_indices: *const i32,
    pub(crate) joint_indices_count: usize,
    pub(crate) joint_weights: *const f32,
    pub(crate) joint_weights_count: usize,
    pub(crate) joint_indices_element_size: usize,
    pub(crate) geom_bind_transform: [f32; 16],
}

/// Volume data from C API
#[repr(C)]
pub(crate) struct UsdBridgeVolumeDataRaw {
    pub(crate) path: *const c_char,
    pub(crate) vdb_file_path: *const c_char,
    pub(crate) field_name: *const c_char,
    pub(crate) transform: [f32; 16],
}

/// Points data from C API (UsdGeomPoints)
#[repr(C)]
pub(crate) struct UsdBridgePointsDataRaw {
    pub(crate) path: *const c_char,
    pub(crate) positions: *const f32,
    pub(crate) point_count: usize,
    pub(crate) widths: *const f32,
    pub(crate) width_count: usize,
    pub(crate) normals: *const f32,
    pub(crate) normal_count: usize,
    pub(crate) ids: *const i64,
    pub(crate) id_count: usize,
    pub(crate) transform: [f32; 16],
}

/// Primvar data type from C API
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum UsdBridgePrimvarTypeRaw {
    Float = 0,
    Float2 = 1,
    Float3 = 2,
    Int = 3,
}

/// Primvar interpolation from C API
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum UsdBridgePrimvarInterpolationRaw {
    Constant = 0,
    Uniform = 1,
    Vertex = 2,
    FaceVarying = 3,
}

/// Primvar data from C API
#[repr(C)]
pub(crate) struct UsdBridgePrimvarDataRaw {
    pub(crate) name: *const c_char,
    pub(crate) primvar_type: UsdBridgePrimvarTypeRaw,
    pub(crate) interpolation: UsdBridgePrimvarInterpolationRaw,
    pub(crate) float_data: *const f32,
    pub(crate) int_data: *const i32,
    pub(crate) element_count: usize,
}

/// Curve type from C API
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum UsdBridgeCurveTypeRaw {
    Linear = 0,
    Cubic = 1,
}

/// Curve basis from C API
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum UsdBridgeCurveBasisRaw {
    Bezier = 0,
    Bspline = 1,
    CatmullRom = 2,
}

/// Curve wrap from C API
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum UsdBridgeCurveWrapRaw {
    Nonperiodic = 0,
    Periodic = 1,
    Pinned = 2,
}

/// BasisCurves data from C API
#[repr(C)]
pub(crate) struct UsdBridgeCurvesDataRaw {
    pub(crate) path: *const c_char,
    pub(crate) points: *const f32,
    pub(crate) point_count: usize,
    pub(crate) widths: *const f32,
    pub(crate) width_count: usize,
    pub(crate) curve_vertex_counts: *const i32,
    pub(crate) curve_count: usize,
    pub(crate) curve_type: UsdBridgeCurveTypeRaw,
    pub(crate) basis: UsdBridgeCurveBasisRaw,
    pub(crate) wrap: UsdBridgeCurveWrapRaw,
    pub(crate) transform: [f32; 16],
}

/// Up axis value from C API (populated by FFI)
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum UsdBridgeUpAxisRaw {
    Y = 0,
    Z = 1,
}

/// Stage metadata from C API
#[repr(C)]
pub(crate) struct UsdBridgeStageMetadataRaw {
    pub(crate) meters_per_unit: f64,
    pub(crate) up_axis: UsdBridgeUpAxisRaw,
    pub(crate) time_codes_per_second: f64,
}

/// Material data from C API (UsdPreviewSurface or MaterialX)
#[repr(C)]
pub(crate) struct UsdBridgeMaterialDataRaw {
    pub(crate) path: *const c_char,
    pub(crate) diffuse_color: [f32; 3],
    pub(crate) metallic: f32,
    pub(crate) roughness: f32,
    pub(crate) specular: f32,
    pub(crate) opacity: f32,
    pub(crate) transmission: f32,
    pub(crate) specular_ior: f32,
    pub(crate) emissive_color: [f32; 3],
    pub(crate) diffuse_texture: *const c_char,
    pub(crate) roughness_texture: *const c_char,
    pub(crate) metallic_texture: *const c_char,
    pub(crate) normal_texture: *const c_char,
    pub(crate) emissive_texture: *const c_char,
    pub(crate) opacity_texture: *const c_char,
    pub(crate) is_materialx: i32,
}

/// Prim specifier from C API
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UsdBridgeSpecifierRaw {
    Define = 0,
    Over = 1,
}

/// Model kind from C API
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UsdBridgeKindRaw {
    None = 0,
    Component = 1,
    Group = 2,
    Assembly = 3,
    Subcomponent = 4,
}

#[link(name = "usd_bridge")]
extern "C" {
    pub(crate) fn usd_bridge_error_message(error: UsdBridgeErrorCode) -> *const c_char;

    pub(crate) fn usd_bridge_open_stage(
        path: *const c_char,
        out_stage: *mut *mut UsdBridgeStageRaw,
    ) -> UsdBridgeErrorCode;

    pub(crate) fn usd_bridge_close_stage(stage: *mut UsdBridgeStageRaw);
    pub(crate) fn usd_bridge_free_mesh_geometry(stage: *mut UsdBridgeStageRaw);
    pub(crate) fn usd_bridge_load_payloads(
        stage: *mut UsdBridgeStageRaw,
        out_prim_count: *mut usize,
    ) -> UsdBridgeErrorCode;

    pub(crate) fn usd_bridge_get_mesh_count(
        stage: *const UsdBridgeStageRaw,
        out_count: *mut usize,
    ) -> UsdBridgeErrorCode;

    pub(crate) fn usd_bridge_get_instancer_count(
        stage: *const UsdBridgeStageRaw,
        out_count: *mut usize,
    ) -> UsdBridgeErrorCode;

    pub(crate) fn usd_bridge_get_mesh(
        stage: *const UsdBridgeStageRaw,
        index: usize,
        out_data: *mut UsdBridgeMeshDataRaw,
    ) -> UsdBridgeErrorCode;

    pub(crate) fn usd_bridge_get_instancer(
        stage: *const UsdBridgeStageRaw,
        index: usize,
        out_data: *mut UsdBridgeInstancerDataRaw,
    ) -> UsdBridgeErrorCode;

    pub(crate) fn usd_bridge_get_native_instance_count(
        stage: *const UsdBridgeStageRaw,
        out_count: *mut usize,
    ) -> UsdBridgeErrorCode;

    pub(crate) fn usd_bridge_get_native_instance(
        stage: *const UsdBridgeStageRaw,
        index: usize,
        out_data: *mut UsdNativeInstanceDataRaw,
    ) -> UsdBridgeErrorCode;

    pub(crate) fn usd_bridge_export_stage(
        stage: *const UsdBridgeStageRaw,
        path: *const c_char,
    ) -> UsdBridgeErrorCode;

    // Prim traversal APIs (for scene browser)
    pub(crate) fn usd_bridge_get_prim_count(
        stage: *const UsdBridgeStageRaw,
        out_count: *mut usize,
    ) -> UsdBridgeErrorCode;

    pub(crate) fn usd_bridge_get_prim_info(
        stage: *const UsdBridgeStageRaw,
        index: usize,
        out_info: *mut UsdBridgePrimInfoRaw,
    ) -> UsdBridgeErrorCode;

    pub(crate) fn usd_bridge_get_root_prim_count(
        stage: *const UsdBridgeStageRaw,
        out_count: *mut usize,
    ) -> UsdBridgeErrorCode;

    pub(crate) fn usd_bridge_get_root_prim_path(
        stage: *const UsdBridgeStageRaw,
        index: usize,
        out_path: *mut *const c_char,
    ) -> UsdBridgeErrorCode;

    pub(crate) fn usd_bridge_get_children_count(
        stage: *const UsdBridgeStageRaw,
        parent_path: *const c_char,
        out_count: *mut usize,
    ) -> UsdBridgeErrorCode;

    pub(crate) fn usd_bridge_get_child_path(
        stage: *const UsdBridgeStageRaw,
        parent_path: *const c_char,
        index: usize,
        out_path: *mut *const c_char,
    ) -> UsdBridgeErrorCode;

    pub(crate) fn usd_bridge_get_prim_info_by_path(
        stage: *const UsdBridgeStageRaw,
        path: *const c_char,
        out_info: *mut UsdBridgePrimInfoRaw,
    ) -> UsdBridgeErrorCode;

    // Material APIs
    pub(crate) fn usd_bridge_get_material_count(
        stage: *const UsdBridgeStageRaw,
        out_count: *mut usize,
    ) -> UsdBridgeErrorCode;

    pub(crate) fn usd_bridge_get_material(
        stage: *const UsdBridgeStageRaw,
        index: usize,
        out_data: *mut UsdBridgeMaterialDataRaw,
    ) -> UsdBridgeErrorCode;

    pub(crate) fn usd_bridge_get_mesh_material_path(
        stage: *const UsdBridgeStageRaw,
        mesh_index: usize,
        out_path: *mut *const c_char,
    ) -> UsdBridgeErrorCode;

    // Timeline APIs
    pub(crate) fn usd_bridge_get_timeline(
        stage: *const UsdBridgeStageRaw,
        out_data: *mut UsdBridgeTimelineDataRaw,
    ) -> UsdBridgeErrorCode;

    // Stage metadata APIs
    pub(crate) fn usd_bridge_get_stage_metadata(
        stage: *const UsdBridgeStageRaw,
        out_data: *mut UsdBridgeStageMetadataRaw,
    ) -> UsdBridgeErrorCode;

    // Animation APIs
    pub(crate) fn usd_bridge_get_mesh_animation(
        stage: *const UsdBridgeStageRaw,
        mesh_index: usize,
        out_data: *mut UsdBridgeAnimatedMeshDataRaw,
    ) -> UsdBridgeErrorCode;

    pub(crate) fn usd_bridge_get_instancer_animation(
        stage: *const UsdBridgeStageRaw,
        instancer_index: usize,
        out_data: *mut UsdBridgeAnimatedInstancerDataRaw,
    ) -> UsdBridgeErrorCode;

    pub(crate) fn usd_bridge_get_camera_xform_samples(
        stage: *const UsdBridgeStageRaw,
        camera_path: *const c_char,
        out_samples: *mut *const UsdBridgeXformSampleRaw,
        out_count: *mut usize,
    ) -> UsdBridgeErrorCode;

    pub(crate) fn usd_bridge_get_camera_count(
        stage: *const UsdBridgeStageRaw,
        out_count: *mut usize,
    ) -> UsdBridgeErrorCode;

    pub(crate) fn usd_bridge_get_camera_path(
        stage: *const UsdBridgeStageRaw,
        index: usize,
        out_path: *mut *const c_char,
    ) -> UsdBridgeErrorCode;

    pub(crate) fn usd_bridge_get_camera_xform_at_time(
        stage: *const UsdBridgeStageRaw,
        camera_path: *const c_char,
        time: f64,
        out_transform: *mut f32,
    ) -> UsdBridgeErrorCode;

    pub(crate) fn usd_bridge_get_camera_properties(
        stage: *const UsdBridgeStageRaw,
        camera_path: *const c_char,
        time: f64,
        out_props: *mut UsdBridgeCameraPropertiesRaw,
    ) -> UsdBridgeErrorCode;

    pub(crate) fn usd_bridge_get_mesh_vertex_animation_info(
        stage: *const UsdBridgeStageRaw,
        mesh_index: usize,
        out_info: *mut UsdBridgeVertexAnimationInfoRaw,
    ) -> UsdBridgeErrorCode;

    pub(crate) fn usd_bridge_get_mesh_vertices_at_time(
        stage: *const UsdBridgeStageRaw,
        mesh_index: usize,
        time: f64,
        out_vertices: *mut *const f32,
        out_vertex_count: *mut usize,
    ) -> UsdBridgeErrorCode;

    // Light APIs
    pub(crate) fn usd_bridge_get_light_count(
        stage: *const UsdBridgeStageRaw,
        out_count: *mut usize,
    ) -> UsdBridgeErrorCode;

    pub(crate) fn usd_bridge_get_light(
        stage: *const UsdBridgeStageRaw,
        index: usize,
        out_data: *mut UsdBridgeLightDataRaw,
    ) -> UsdBridgeErrorCode;

    // Points APIs (UsdGeomPoints)
    pub(crate) fn usd_bridge_get_points_count(
        stage: *const UsdBridgeStageRaw,
        out_count: *mut usize,
    ) -> UsdBridgeErrorCode;

    pub(crate) fn usd_bridge_get_points(
        stage: *const UsdBridgeStageRaw,
        index: usize,
        out_data: *mut UsdBridgePointsDataRaw,
    ) -> UsdBridgeErrorCode;

    // Primvar query APIs
    pub(crate) fn usd_bridge_get_mesh_primvar_count(
        stage: *const UsdBridgeStageRaw,
        mesh_index: usize,
        out_count: *mut usize,
    ) -> UsdBridgeErrorCode;

    pub(crate) fn usd_bridge_get_mesh_primvar(
        stage: *const UsdBridgeStageRaw,
        mesh_index: usize,
        primvar_index: usize,
        out_data: *mut UsdBridgePrimvarDataRaw,
    ) -> UsdBridgeErrorCode;

    // Edit layer export
    pub(crate) fn usd_bridge_create_edit_layer(
        output_path: *const c_char,
        out_layer: *mut *mut UsdBridgeEditLayerRaw,
    ) -> UsdBridgeErrorCode;

    pub(crate) fn usd_bridge_write_xform_opinion(
        layer: *mut UsdBridgeEditLayerRaw,
        prim_path: *const c_char,
        time: f64,
        matrix_16: *const f32,
    ) -> UsdBridgeErrorCode;

    pub(crate) fn usd_bridge_save_edit_layer(
        layer: *mut UsdBridgeEditLayerRaw,
    ) -> UsdBridgeErrorCode;

    pub(crate) fn usd_bridge_free_edit_layer(layer: *mut UsdBridgeEditLayerRaw);

    pub(crate) fn usd_bridge_edit_layer_add_sublayer(
        layer: *mut UsdBridgeEditLayerRaw,
        sublayer_path: *const c_char,
    ) -> UsdBridgeErrorCode;

    pub(crate) fn usd_bridge_edit_layer_add_reference(
        layer: *mut UsdBridgeEditLayerRaw,
        prim_path: *const c_char,
        reference_file: *const c_char,
        reference_prim_path: *const c_char,
    ) -> UsdBridgeErrorCode;

    pub(crate) fn usd_bridge_edit_layer_set_default_prim(
        layer: *mut UsdBridgeEditLayerRaw,
        prim_path: *const c_char,
    ) -> UsdBridgeErrorCode;

    pub(crate) fn usd_bridge_write_point_instancer(
        layer: *mut UsdBridgeEditLayerRaw,
        prim_path: *const c_char,
        positions: *const f32,
        orientations: *const f32,
        scales: *const f32,
        proto_indices: *const i32,
        count: usize,
        prototype_paths: *const *const c_char,
        prototype_count: usize,
    ) -> UsdBridgeErrorCode;

    // Mesh authoring
    pub(crate) fn usd_bridge_write_mesh(
        layer: *mut UsdBridgeEditLayerRaw,
        prim_path: *const c_char,
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
    pub(crate) fn usd_bridge_define_prim(
        layer: *mut UsdBridgeEditLayerRaw,
        prim_path: *const c_char,
        type_name: *const c_char,
        specifier: UsdBridgeSpecifierRaw,
    ) -> UsdBridgeErrorCode;

    pub(crate) fn usd_bridge_set_prim_kind(
        layer: *mut UsdBridgeEditLayerRaw,
        prim_path: *const c_char,
        kind: UsdBridgeKindRaw,
    ) -> UsdBridgeErrorCode;

    // Material export (UsdPreviewSurface + OpenPBR MaterialX dual output)
    pub(crate) fn usd_bridge_write_material(
        layer: *mut UsdBridgeEditLayerRaw,
        mat_path: *const c_char,
        diffuse_color: *const f32,
        metallic: f32,
        roughness: f32,
        specular: f32,
        opacity: f32,
        emissive_color: *const f32,
        diffuse_tex: *const c_char,
        roughness_tex: *const c_char,
        metallic_tex: *const c_char,
        normal_tex: *const c_char,
        emissive_tex: *const c_char,
        specular_ior: f32,
        transmission_weight: f32,
    ) -> UsdBridgeErrorCode;

    pub(crate) fn usd_bridge_bind_material(
        layer: *mut UsdBridgeEditLayerRaw,
        prim_path: *const c_char,
        material_path: *const c_char,
    ) -> UsdBridgeErrorCode;

    pub(crate) fn usd_bridge_write_visibility(
        layer: *mut UsdBridgeEditLayerRaw,
        prim_path: *const c_char,
        visible: i32,
    ) -> UsdBridgeErrorCode;

    pub(crate) fn usd_bridge_set_stage_metadata(
        layer: *mut UsdBridgeEditLayerRaw,
        meters_per_unit: f64,
        up_axis: i32,
        time_codes_per_second: f64,
    ) -> UsdBridgeErrorCode;

    // Camera export
    pub(crate) fn usd_bridge_write_camera(
        layer: *mut UsdBridgeEditLayerRaw,
        path: *const c_char,
        focal_length: f32,
        h_aperture: f32,
        v_aperture: f32,
        clip_near: f32,
        clip_far: f32,
        time: f64,
        transform: *const f32,
    ) -> UsdBridgeErrorCode;

    // Light export
    pub(crate) fn usd_bridge_write_light(
        layer: *mut UsdBridgeEditLayerRaw,
        path: *const c_char,
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
        texture_path: *const c_char,
        shaping_cone_angle: f32,
        shaping_cone_softness: f32,
        shaping_focus: f32,
    ) -> UsdBridgeErrorCode;

    // Render settings export
    pub(crate) fn usd_bridge_write_render_settings(
        layer: *mut UsdBridgeEditLayerRaw,
        path: *const c_char,
        resolution_x: i32,
        resolution_y: i32,
        camera_path: *const c_char,
        pixel_aspect_ratio: f32,
    ) -> UsdBridgeErrorCode;

    // Payload
    pub(crate) fn usd_bridge_load_payload(
        stage: *mut UsdBridgeStageRaw,
        prim_path: *const c_char,
    ) -> UsdBridgeErrorCode;

    pub(crate) fn usd_bridge_unload_payload(
        stage: *mut UsdBridgeStageRaw,
        prim_path: *const c_char,
    ) -> UsdBridgeErrorCode;

    pub(crate) fn usd_bridge_edit_layer_add_payload(
        layer: *mut UsdBridgeEditLayerRaw,
        prim_path: *const c_char,
        asset_path: *const c_char,
        target_path: *const c_char,
    ) -> UsdBridgeErrorCode;

    // Variant query/selection
    pub(crate) fn usd_bridge_get_variant_set_count(
        stage: *const UsdBridgeStageRaw,
        prim_path: *const c_char,
        out_count: *mut usize,
    ) -> UsdBridgeErrorCode;

    pub(crate) fn usd_bridge_get_variant_set_name(
        stage: *const UsdBridgeStageRaw,
        prim_path: *const c_char,
        index: usize,
        out_name: *mut *const c_char,
    ) -> UsdBridgeErrorCode;

    pub(crate) fn usd_bridge_get_variant_count(
        stage: *const UsdBridgeStageRaw,
        prim_path: *const c_char,
        variant_set_name: *const c_char,
        out_count: *mut usize,
    ) -> UsdBridgeErrorCode;

    pub(crate) fn usd_bridge_get_variant_name(
        stage: *const UsdBridgeStageRaw,
        prim_path: *const c_char,
        variant_set_name: *const c_char,
        index: usize,
        out_name: *mut *const c_char,
    ) -> UsdBridgeErrorCode;

    pub(crate) fn usd_bridge_get_variant_selection(
        stage: *const UsdBridgeStageRaw,
        prim_path: *const c_char,
        variant_set_name: *const c_char,
        out_selection: *mut *const c_char,
    ) -> UsdBridgeErrorCode;

    pub(crate) fn usd_bridge_set_variant_selection(
        stage: *mut UsdBridgeStageRaw,
        prim_path: *const c_char,
        variant_set_name: *const c_char,
        variant_name: *const c_char,
    ) -> UsdBridgeErrorCode;

    // BasisCurves
    pub(crate) fn usd_bridge_get_curves_count(
        stage: *const UsdBridgeStageRaw,
        out_count: *mut usize,
    ) -> UsdBridgeErrorCode;

    pub(crate) fn usd_bridge_get_curves(
        stage: *const UsdBridgeStageRaw,
        index: usize,
        out_data: *mut UsdBridgeCurvesDataRaw,
    ) -> UsdBridgeErrorCode;

    // Skeleton
    pub(crate) fn usd_bridge_get_skeleton_count(
        stage: *const UsdBridgeStageRaw,
        out_count: *mut usize,
    ) -> UsdBridgeErrorCode;

    pub(crate) fn usd_bridge_get_skeleton(
        stage: *const UsdBridgeStageRaw,
        index: usize,
        out_data: *mut UsdBridgeSkeletonDataRaw,
    ) -> UsdBridgeErrorCode;

    pub(crate) fn usd_bridge_get_skin_binding(
        stage: *const UsdBridgeStageRaw,
        mesh_index: usize,
        out_data: *mut UsdBridgeSkinBindingDataRaw,
    ) -> UsdBridgeErrorCode;

    // Volumes
    pub(crate) fn usd_bridge_get_volume_count(
        stage: *const UsdBridgeStageRaw,
        out_count: *mut usize,
    ) -> UsdBridgeErrorCode;

    pub(crate) fn usd_bridge_get_volume(
        stage: *const UsdBridgeStageRaw,
        index: usize,
        out_data: *mut UsdBridgeVolumeDataRaw,
    ) -> UsdBridgeErrorCode;

    // Collection material binding handled by existing get_mesh_material_path
    // (ComputeBoundMaterial resolves both direct and collection-based bindings)

    // GeomSubset export
    pub(crate) fn usd_bridge_write_geom_subset(
        layer: *mut UsdBridgeEditLayerRaw,
        mesh_path: *const c_char,
        subset_name: *const c_char,
        face_indices: *const i32,
        face_count: usize,
        material_path: *const c_char,
    ) -> UsdBridgeErrorCode;

    // PointInstancer invisibleIds export
    pub(crate) fn usd_bridge_write_invisible_ids(
        layer: *mut UsdBridgeEditLayerRaw,
        instancer_path: *const c_char,
        ids: *const i64,
        count: usize,
    ) -> UsdBridgeErrorCode;

    pub(crate) fn usd_bridge_get_prim_attributes(
        stage: *const UsdBridgeStageRaw,
        prim_path: *const c_char,
        out_attributes: *mut *mut UsdBridgeAttributeDataRaw,
        out_count: *mut usize,
    ) -> UsdBridgeErrorCode;

    pub(crate) fn usd_bridge_free_prim_attributes(
        attributes: *mut UsdBridgeAttributeDataRaw,
        count: usize,
    );
}
