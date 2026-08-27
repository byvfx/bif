//! Pure Rust conversion functions for FFI raw types to safe domain types.
//!
//! These functions extract the conversion logic from `UsdStage::get_*` methods
//! so it can be tested without C++ DLLs or USD environment setup. Each function
//! takes a reference to a raw FFI struct and returns the corresponding safe type.
//!
//! # Safety
//!
//! All conversion functions are `unsafe` because they dereference raw pointers
//! inside the FFI structs. Callers must ensure the raw data's pointers are valid
//! and point to arrays of the specified counts.

// Called from cpp_bridge.rs to convert raw FFI structs to safe domain types.

use std::ffi::CStr;
use std::path::PathBuf;

use bif_math::{Mat4, Vec3};

use super::ffi::{
    CameraProperties, CurveBasis, CurveType, CurveWrap, MeshPurpose, NormalsInterpolation,
    PrimvarInterpolation, PrimvarType, SubdivisionScheme, TransformSample, UpAxis,
    UsdAnimatedInstancerData, UsdAnimatedMeshData, UsdBlendShapeBinding, UsdBlendShapeTarget,
    UsdBridgeResult, UsdCurvesData, UsdInstancerData, UsdLightData, UsdLightShaping,
    UsdMaterialData, UsdMeshData, UsdNativeInstance, UsdPointsData, UsdPrimInfo, UsdPrimvarData,
    UsdSkeletonData, UsdSkinBindingData, UsdStageMetadata, UsdTimelineData, UsdVolumeData,
};
use super::ffi_guard::{checked_alloc_count, checked_mul_count};

use super::ffi_raw::{
    UsdBridgeAnimatedInstancerDataRaw, UsdBridgeAnimatedMeshDataRaw, UsdBridgeAttributeOpinionsRaw,
    UsdBridgeBlendShapeBindingDataRaw, UsdBridgeCameraPropertiesRaw, UsdBridgeCurveBasisRaw,
    UsdBridgeCurveTypeRaw, UsdBridgeCurveWrapRaw, UsdBridgeCurvesDataRaw, UsdBridgeEditTargetRaw,
    UsdBridgeInstancerDataRaw, UsdBridgeLayerInfoRaw, UsdBridgeLayerOffsetRaw,
    UsdBridgeLayerStackRaw, UsdBridgeLightDataRaw, UsdBridgeMaterialDataRaw, UsdBridgeMeshDataRaw,
    UsdBridgeOpinionSourceRaw, UsdBridgePayloadPolicyRaw, UsdBridgePointsDataRaw,
    UsdBridgePrimInfoRaw, UsdBridgePrimSpecRaw, UsdBridgePrimStackRaw, UsdBridgePrimvarDataRaw,
    UsdBridgePrimvarInterpolationRaw, UsdBridgePrimvarTypeRaw, UsdBridgePurposeRaw,
    UsdBridgeSkeletonDataRaw, UsdBridgeSkinBindingDataRaw, UsdBridgeStageMetadataRaw,
    UsdBridgeTimelineDataRaw, UsdBridgeUpAxisRaw, UsdBridgeVolumeDataRaw, UsdNativeInstanceDataRaw,
};

use super::layer::{
    EditTarget, LayerInfo, LayerOffset, LayerStack, OpinionSource, PayloadPolicy, PrimSpecifier,
    PrimStackEntry,
};

// ============================================================================
// Helpers
// ============================================================================

/// Convert a nullable C string pointer to an owned `String`.
///
/// Returns empty string if the pointer is null.
///
/// # Safety
///
/// `ptr` must be null or point to a valid NUL-terminated C string.
pub(crate) unsafe fn c_str_to_string(ptr: *const std::ffi::c_char) -> String {
    if ptr.is_null() {
        String::new()
    } else {
        CStr::from_ptr(ptr).to_string_lossy().into_owned()
    }
}

/// Convert a nullable C string pointer to `Option<String>`.
///
/// Returns `None` if the pointer is null or the string is empty.
///
/// # Safety
///
/// `ptr` must be null or point to a valid NUL-terminated C string.
pub(crate) unsafe fn c_str_to_opt_string(ptr: *const std::ffi::c_char) -> Option<String> {
    if ptr.is_null() {
        None
    } else {
        let s = CStr::from_ptr(ptr).to_string_lossy().into_owned();
        if s.is_empty() {
            None
        } else {
            Some(s)
        }
    }
}

/// Convert a nullable f32 pointer + count to `Vec<Vec3>`.
///
/// Interprets the data as tightly-packed `[x, y, z, x, y, z, ...]`.
///
/// # Safety
///
/// `ptr` must be null or point to at least `count * 3` valid f32 values.
pub(crate) unsafe fn f32_ptr_to_vec3s(ptr: *const f32, count: usize) -> Vec<Vec3> {
    if ptr.is_null() || count == 0 {
        Vec::new()
    } else {
        let Some(n) = super::ffi_guard::safe_mul_count::<f32>(count, 3, "f32_ptr_to_vec3s") else {
            return Vec::new();
        };
        let slice = std::slice::from_raw_parts(ptr, n);
        slice
            .as_chunks::<3>()
            .0
            .iter()
            .map(|c| Vec3::new(c[0], c[1], c[2]))
            .collect()
    }
}

/// Convert a nullable f32 pointer + count to `Option<Vec<Vec3>>`.
///
/// # Safety
///
/// `ptr` must be null or point to at least `count * 3` valid f32 values.
pub(crate) unsafe fn f32_ptr_to_opt_vec3s(ptr: *const f32, count: usize) -> Option<Vec<Vec3>> {
    if ptr.is_null() || count == 0 {
        None
    } else {
        Some(f32_ptr_to_vec3s(ptr, count))
    }
}

/// Convert a flat f32[16] array to `Mat4`.
///
/// Uses `Mat4::from_cols_array` which matches the USD row-major to glam
/// column-major convention (implicit transpose from row-vector to
/// column-vector convention).
pub(crate) fn f32x16_to_mat4(arr: &[f32; 16]) -> Mat4 {
    Mat4::from_cols_array(arr)
}

/// Convert a flat f32 pointer + count of matrices to `Vec<Mat4>`.
///
/// # Safety
///
/// `ptr` must be null or point to at least `count * 16` valid f32 values.
pub(crate) unsafe fn f32_ptr_to_mat4s(ptr: *const f32, count: usize) -> Vec<Mat4> {
    if ptr.is_null() || count == 0 {
        Vec::new()
    } else {
        let Some(n) = super::ffi_guard::safe_mul_count::<f32>(count, 16, "f32_ptr_to_mat4s") else {
            return Vec::new();
        };
        let slice = std::slice::from_raw_parts(ptr, n);
        // `as_chunks` hands back `&[f32; 16]` directly, so the intermediate
        // copy `chunks_exact` needed to produce a fixed-size array is gone.
        slice
            .as_chunks::<16>()
            .0
            .iter()
            .map(Mat4::from_cols_array)
            .collect()
    }
}

/// Convert a `*const *const c_char` + count to `Vec<String>`.
///
/// # Safety
///
/// `ptr` must be null or point to `count` valid `*const c_char` pointers,
/// each of which must be null or point to a valid NUL-terminated C string.
pub(crate) unsafe fn c_str_array_to_strings(
    ptr: *const *const std::ffi::c_char,
    count: usize,
) -> Vec<String> {
    if ptr.is_null() || count == 0 {
        Vec::new()
    } else {
        let ptrs = std::slice::from_raw_parts(ptr, count);
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
}

/// Convert a `*const *const c_char` + count to `Vec<String>`, filtering nulls.
///
/// # Safety
///
/// Same as `c_str_array_to_strings`.
pub(crate) unsafe fn c_str_array_to_strings_filtered(
    ptr: *const *const std::ffi::c_char,
    count: usize,
) -> Vec<String> {
    if ptr.is_null() || count == 0 {
        Vec::new()
    } else {
        let ptrs = std::slice::from_raw_parts(ptr, count);
        ptrs.iter()
            .filter(|&&p| !p.is_null())
            .map(|&p| CStr::from_ptr(p).to_string_lossy().into_owned())
            .collect()
    }
}

/// Convert `UsdBridgePurposeRaw` to `MeshPurpose`.
pub(crate) fn convert_purpose_raw(purpose: UsdBridgePurposeRaw) -> MeshPurpose {
    match purpose {
        UsdBridgePurposeRaw::Render => MeshPurpose::Render,
        UsdBridgePurposeRaw::Proxy => MeshPurpose::Proxy,
        UsdBridgePurposeRaw::Guide => MeshPurpose::Guide,
        _ => MeshPurpose::Default,
    }
}

/// Convert an integer purpose value to `MeshPurpose`.
pub(crate) fn convert_purpose_int(purpose: i32) -> MeshPurpose {
    match purpose {
        1 => MeshPurpose::Render,
        2 => MeshPurpose::Proxy,
        3 => MeshPurpose::Guide,
        _ => MeshPurpose::Default,
    }
}

// ============================================================================
// Conversion Functions
// ============================================================================

/// Convert raw mesh data from FFI to safe `UsdMeshData`.
///
/// # Safety
///
/// All pointers in `raw` must be valid and point to arrays of the specified
/// counts, or be null. String pointers must be NUL-terminated.
pub(crate) unsafe fn convert_mesh(raw: &UsdBridgeMeshDataRaw) -> UsdMeshData {
    let path = c_str_to_string(raw.path);

    let vertices = f32_ptr_to_vec3s(raw.vertices, raw.vertex_count);

    let indices = if raw.indices.is_null() || raw.index_count == 0 {
        Vec::new()
    } else {
        std::slice::from_raw_parts(raw.indices, raw.index_count).to_vec()
    };

    let normals = f32_ptr_to_opt_vec3s(raw.normals, raw.normal_count);

    let uvs = if raw.uvs.is_null() || raw.uv_count == 0 {
        None
    } else if let Some(n) = super::ffi_guard::safe_mul_count::<f32>(raw.uv_count, 2, "mesh.uvs") {
        let slice = std::slice::from_raw_parts(raw.uvs, n);
        // `as_chunks::<2>().0` is already `&[[f32; 2]]` — the element type we
        // want — so the per-chunk rebuild collapses into a copy.
        Some(slice.as_chunks::<2>().0.to_vec())
    } else {
        None
    };

    let face_material_ids = if raw.face_material_ids.is_null() || raw.triangle_count == 0 {
        None
    } else {
        Some(std::slice::from_raw_parts(raw.face_material_ids, raw.triangle_count).to_vec())
    };

    let transform = f32x16_to_mat4(&raw.transform);

    let purpose = convert_purpose_raw(raw.purpose);

    let subdivision_scheme = if raw.subdivision_scheme.is_null() {
        SubdivisionScheme::None
    } else {
        match CStr::from_ptr(raw.subdivision_scheme)
            .to_str()
            .unwrap_or("none")
        {
            "catmullClark" => SubdivisionScheme::CatmullClark,
            "loop" => SubdivisionScheme::Loop,
            "bilinear" => SubdivisionScheme::Bilinear,
            _ => SubdivisionScheme::None,
        }
    };

    let normals_interpolation = match raw.normals_interpolation {
        1 => NormalsInterpolation::FaceVarying,
        2 => NormalsInterpolation::Uniform,
        3 => NormalsInterpolation::Constant,
        _ => NormalsInterpolation::Vertex,
    };

    let display_color = if raw.display_color.is_null() || raw.display_color_count == 0 {
        None
    } else if let Some(n) =
        super::ffi_guard::safe_mul_count::<f32>(raw.display_color_count, 3, "mesh.display_color")
    {
        let slice = std::slice::from_raw_parts(raw.display_color, n);
        Some(
            slice
                .as_chunks::<3>()
                .0
                .iter()
                .map(|c| Vec3::new(c[0], c[1], c[2]))
                .collect(),
        )
    } else {
        None
    };

    let face_vertex_counts = if raw.face_vertex_counts.is_null() || raw.face_count == 0 {
        None
    } else {
        Some(std::slice::from_raw_parts(raw.face_vertex_counts, raw.face_count).to_vec())
    };

    let face_vertex_indices =
        if raw.face_vertex_indices.is_null() || raw.face_vertex_index_count == 0 {
            None
        } else {
            Some(
                std::slice::from_raw_parts(raw.face_vertex_indices, raw.face_vertex_index_count)
                    .to_vec(),
            )
        };

    let crease_indices = if raw.crease_indices.is_null() || raw.crease_index_count == 0 {
        None
    } else {
        Some(std::slice::from_raw_parts(raw.crease_indices, raw.crease_index_count).to_vec())
    };

    let crease_lengths = if raw.crease_lengths.is_null() || raw.crease_length_count == 0 {
        None
    } else {
        Some(std::slice::from_raw_parts(raw.crease_lengths, raw.crease_length_count).to_vec())
    };

    let crease_sharpnesses = if raw.crease_sharpnesses.is_null() || raw.crease_sharpness_count == 0
    {
        None
    } else {
        Some(
            std::slice::from_raw_parts(raw.crease_sharpnesses, raw.crease_sharpness_count).to_vec(),
        )
    };

    let vertices_orig = f32_ptr_to_opt_vec3s(raw.vertices_orig, raw.vertex_count_orig);

    // FaceVarying UV data for subdivision surfaces
    let facevarying_uvs = if !raw.facevarying_uvs.is_null() && raw.facevarying_uv_count > 0 {
        let float_count = raw
            .facevarying_uv_count
            .checked_mul(2)
            .expect("facevarying_uv_count overflow");
        let slice = unsafe { std::slice::from_raw_parts(raw.facevarying_uvs, float_count) };
        Some(slice.as_chunks::<2>().0.to_vec())
    } else {
        None
    };

    let facevarying_uv_indices = if !raw.facevarying_uv_indices.is_null()
        && raw.facevarying_uv_index_count > 0
    {
        let slice = unsafe {
            std::slice::from_raw_parts(raw.facevarying_uv_indices, raw.facevarying_uv_index_count)
        };
        Some(slice.to_vec())
    } else {
        None
    };

    UsdMeshData {
        path,
        vertices,
        indices,
        normals,
        uvs,
        face_material_ids,
        transform,
        purpose,
        is_instance_proxy: raw.is_instance_proxy != 0,
        visible: raw.visibility != 0,
        double_sided: raw.double_sided != 0,
        subdivision_scheme,
        normals_interpolation,
        display_color,
        display_opacity: raw.display_opacity,
        resets_xform_stack: raw.resets_xform_stack != 0,
        face_vertex_counts,
        face_vertex_indices,
        crease_indices,
        crease_lengths,
        crease_sharpnesses,
        vertices_orig,
        facevarying_uvs,
        facevarying_uv_indices,
    }
}

/// Convert raw instancer data from FFI to safe `UsdInstancerData`.
///
/// # Safety
///
/// All pointers in `raw` must be valid and point to arrays of the specified
/// counts, or be null.
pub(crate) unsafe fn convert_instancer(raw: &UsdBridgeInstancerDataRaw) -> UsdInstancerData {
    let path = c_str_to_string(raw.path);
    let prototype_paths = c_str_array_to_strings(raw.prototype_paths, raw.prototype_count);
    let transforms = f32_ptr_to_mat4s(raw.transforms, raw.instance_count);

    let proto_indices = if raw.proto_indices.is_null() || raw.instance_count == 0 {
        Vec::new()
    } else {
        std::slice::from_raw_parts(raw.proto_indices, raw.instance_count).to_vec()
    };

    let velocities = f32_ptr_to_opt_vec3s(raw.velocities, raw.velocity_count);
    let angular_velocities =
        f32_ptr_to_opt_vec3s(raw.angular_velocities, raw.angular_velocity_count);

    let invisible_ids = if raw.invisible_ids.is_null() || raw.invisible_id_count == 0 {
        Vec::new()
    } else {
        std::slice::from_raw_parts(raw.invisible_ids, raw.invisible_id_count).to_vec()
    };

    UsdInstancerData {
        path,
        prototype_paths,
        transforms,
        proto_indices,
        velocities,
        angular_velocities,
        invisible_ids,
    }
}

/// Convert raw native instance data from FFI to safe `UsdNativeInstance`.
///
/// # Safety
///
/// The `raw` struct must contain valid data.
pub(crate) unsafe fn convert_native_instance(raw: &UsdNativeInstanceDataRaw) -> UsdNativeInstance {
    UsdNativeInstance {
        proto_mesh_idx: raw.proto_mesh_idx as usize,
        transform: f32x16_to_mat4(&raw.transform),
        material_override_idx: raw.material_override_idx,
        purpose: convert_purpose_int(raw.purpose),
    }
}

/// Convert raw material data from FFI to safe `UsdMaterialData`.
///
/// # Safety
///
/// All string pointers in `raw` must be null or point to valid NUL-terminated
/// C strings.
pub(crate) unsafe fn convert_material(raw: &UsdBridgeMaterialDataRaw) -> UsdMaterialData {
    let path = c_str_to_string(raw.path);

    UsdMaterialData {
        path,
        base_color: Vec3::new(
            raw.diffuse_color[0],
            raw.diffuse_color[1],
            raw.diffuse_color[2],
        ),
        base_metalness: raw.metallic,
        specular_roughness: raw.roughness,
        specular_weight: raw.specular,
        specular_ior: raw.specular_ior,
        transmission_weight: raw.transmission,
        geometry_opacity: raw.opacity,
        emission_color: Vec3::new(
            raw.emissive_color[0],
            raw.emissive_color[1],
            raw.emissive_color[2],
        ),
        base_color_texture: c_str_to_opt_string(raw.diffuse_texture),
        specular_roughness_texture: c_str_to_opt_string(raw.roughness_texture),
        base_metalness_texture: c_str_to_opt_string(raw.metallic_texture),
        normal_texture: c_str_to_opt_string(raw.normal_texture),
        emission_texture: c_str_to_opt_string(raw.emissive_texture),
        geometry_opacity_texture: c_str_to_opt_string(raw.opacity_texture),
        displacement_texture: c_str_to_opt_string(raw.displacement_texture),
        displacement_scale: raw.displacement_scale,
        is_materialx: raw.is_materialx != 0,
    }
}

/// Convert raw light data from FFI to safe `UsdLightData`.
///
/// Combines `intensity * 2^exposure` into a single intensity value.
///
/// # Safety
///
/// All pointers in `raw` must be valid and point to arrays of the specified
/// counts, or be null.
pub(crate) unsafe fn convert_light(raw: &UsdBridgeLightDataRaw) -> UsdLightData {
    let path = c_str_to_string(raw.path);
    let texture_path = c_str_to_opt_string(raw.texture_path);
    let combined_intensity = raw.intensity * (2.0_f32).powf(raw.exposure);

    UsdLightData {
        path,
        light_type: raw.light_type.into(),
        color: Vec3::new(raw.color[0], raw.color[1], raw.color[2]),
        intensity: combined_intensity,
        transform: f32x16_to_mat4(&raw.transform),
        angle: raw.angle,
        radius: raw.radius,
        width: raw.width,
        height: raw.height,
        texture_path,
        length: raw.length,
        shaping: UsdLightShaping {
            cone_angle: raw.shaping_cone_angle,
            cone_softness: raw.shaping_cone_softness,
            focus: raw.shaping_focus,
            ies_file: c_str_to_opt_string(raw.shaping_ies_file),
        },
        light_link_includes: c_str_array_to_strings_filtered(
            raw.light_link_includes,
            raw.light_link_include_count,
        ),
        light_link_excludes: c_str_array_to_strings_filtered(
            raw.light_link_excludes,
            raw.light_link_exclude_count,
        ),
    }
}

/// Convert raw prim info from FFI to safe `UsdPrimInfo`.
///
/// # Safety
///
/// String pointers in `raw` must be null or point to valid NUL-terminated
/// C strings.
pub(crate) unsafe fn convert_prim_info(raw: &UsdBridgePrimInfoRaw) -> UsdPrimInfo {
    UsdPrimInfo {
        path: c_str_to_string(raw.path),
        type_name: c_str_to_string(raw.type_name),
        is_active: raw.is_active != 0,
        has_children: raw.has_children != 0,
        child_count: raw.child_count,
        visible: raw.visibility != 0,
        has_payload: raw.has_payload != 0,
        is_loaded: raw.is_loaded != 0,
        variant_set_count: raw.variant_set_count,
        has_inherits: raw.has_inherits != 0,
        has_specializes: raw.has_specializes != 0,
    }
}

/// Convert raw timeline data from FFI to safe `UsdTimelineData`.
pub(crate) fn convert_timeline(raw: &UsdBridgeTimelineDataRaw) -> UsdTimelineData {
    UsdTimelineData {
        start_time_code: raw.start_time_code,
        end_time_code: raw.end_time_code,
        frames_per_second: raw.frames_per_second,
        has_authored_time_range: raw.has_authored_time_range != 0,
    }
}

/// Convert raw points data from FFI to safe `UsdPointsData`.
///
/// # Safety
///
/// All pointers in `raw` must be valid and point to arrays of the specified
/// counts, or be null.
pub(crate) unsafe fn convert_points(raw: &UsdBridgePointsDataRaw) -> UsdPointsData {
    let path = c_str_to_string(raw.path);
    let positions = f32_ptr_to_vec3s(raw.positions, raw.point_count);

    let widths = if raw.widths.is_null() || raw.width_count == 0 {
        None
    } else {
        Some(std::slice::from_raw_parts(raw.widths, raw.width_count).to_vec())
    };

    let normals = f32_ptr_to_opt_vec3s(raw.normals, raw.normal_count);

    let ids = if raw.ids.is_null() || raw.id_count == 0 {
        None
    } else {
        Some(std::slice::from_raw_parts(raw.ids, raw.id_count).to_vec())
    };

    UsdPointsData {
        path,
        positions,
        widths,
        normals,
        ids,
        transform: f32x16_to_mat4(&raw.transform),
    }
}

/// Convert raw curves data from FFI to safe `UsdCurvesData`.
///
/// # Safety
///
/// All pointers in `raw` must be valid and point to arrays of the specified
/// counts, or be null.
pub(crate) unsafe fn convert_curves(raw: &UsdBridgeCurvesDataRaw) -> UsdCurvesData {
    let path = c_str_to_string(raw.path);
    let points = f32_ptr_to_vec3s(raw.points, raw.point_count);

    let widths = if raw.widths.is_null() || raw.width_count == 0 {
        None
    } else {
        Some(std::slice::from_raw_parts(raw.widths, raw.width_count).to_vec())
    };

    let curve_vertex_counts = if raw.curve_vertex_counts.is_null() || raw.curve_count == 0 {
        Vec::new()
    } else {
        std::slice::from_raw_parts(raw.curve_vertex_counts, raw.curve_count).to_vec()
    };

    UsdCurvesData {
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
        transform: f32x16_to_mat4(&raw.transform),
    }
}

/// Convert raw skeleton data from FFI to safe `UsdSkeletonData`.
///
/// # Safety
///
/// All pointers in `raw` must be valid and point to arrays of the specified
/// counts, or be null.
pub(crate) unsafe fn convert_skeleton(raw: &UsdBridgeSkeletonDataRaw) -> UsdSkeletonData {
    let path = c_str_to_string(raw.path);
    let joint_paths = c_str_array_to_strings(raw.joint_paths, raw.joint_count);
    let bind_transforms = f32_ptr_to_mat4s(raw.bind_transforms, raw.joint_count);
    let rest_transforms = f32_ptr_to_mat4s(raw.rest_transforms, raw.joint_count);

    UsdSkeletonData {
        path,
        joint_paths,
        bind_transforms,
        rest_transforms,
    }
}

/// Convert raw skin binding data from FFI to safe `UsdSkinBindingData`.
///
/// # Safety
///
/// All pointers in `raw` must be valid and point to arrays of the specified
/// counts, or be null.
pub(crate) unsafe fn convert_skin_binding(raw: &UsdBridgeSkinBindingDataRaw) -> UsdSkinBindingData {
    let mesh_path = c_str_to_string(raw.mesh_path);
    let skeleton_path = c_str_to_string(raw.skeleton_path);

    let joint_indices = if raw.joint_indices.is_null() || raw.joint_indices_count == 0 {
        Vec::new()
    } else {
        std::slice::from_raw_parts(raw.joint_indices, raw.joint_indices_count).to_vec()
    };

    let joint_weights = if raw.joint_weights.is_null() || raw.joint_weights_count == 0 {
        Vec::new()
    } else {
        std::slice::from_raw_parts(raw.joint_weights, raw.joint_weights_count).to_vec()
    };

    UsdSkinBindingData {
        mesh_path,
        skeleton_path,
        joint_indices,
        joint_weights,
        element_size: raw.joint_indices_element_size,
        geom_bind_transform: f32x16_to_mat4(&raw.geom_bind_transform),
        skel_root_world_xform: f32x16_to_mat4(&raw.skel_root_world_xform),
        is_rigid: raw.is_rigid != 0,
    }
}

/// Convert raw blend shape binding data from FFI to safe `UsdBlendShapeBinding`.
///
/// # Safety
///
/// All pointers in `raw` (and in each target) must be valid and point to arrays
/// of the specified counts, or be null.
pub(crate) unsafe fn convert_blend_shape_binding(
    raw: &UsdBridgeBlendShapeBindingDataRaw,
) -> UsdBlendShapeBinding {
    let mesh_path = c_str_to_string(raw.mesh_prim_path);

    let target_count = raw.target_count as usize;
    let mut targets = Vec::with_capacity(target_count);

    if !raw.targets.is_null() && target_count > 0 {
        let raw_targets = std::slice::from_raw_parts(raw.targets, target_count);
        for rt in raw_targets {
            let name = c_str_to_string(rt.name);
            let vert_count = rt.vert_count as usize;

            let offsets = if rt.offsets_xyz.is_null() || vert_count == 0 {
                Vec::new()
            } else if let Some(n) =
                super::ffi_guard::safe_mul_count::<f32>(vert_count, 3, "blendshape.offsets")
            {
                let floats = std::slice::from_raw_parts(rt.offsets_xyz, n);
                floats
                    .as_chunks::<3>()
                    .0
                    .iter()
                    .map(|c| Vec3::new(c[0], c[1], c[2]))
                    .collect()
            } else {
                Vec::new()
            };

            let normal_offsets =
                if rt.has_normals != 0 && !rt.normal_offsets_xyz.is_null() && vert_count > 0 {
                    if let Some(n) = super::ffi_guard::safe_mul_count::<f32>(
                        vert_count,
                        3,
                        "blendshape.normal_offsets",
                    ) {
                        let floats = std::slice::from_raw_parts(rt.normal_offsets_xyz, n);
                        Some(
                            floats
                                .as_chunks::<3>()
                                .0
                                .iter()
                                .map(|c| Vec3::new(c[0], c[1], c[2]))
                                .collect(),
                        )
                    } else {
                        None
                    }
                } else {
                    None
                };

            targets.push(UsdBlendShapeTarget {
                name,
                offsets,
                normal_offsets,
            });
        }
    }

    UsdBlendShapeBinding { mesh_path, targets }
}

/// Convert raw volume data from FFI to safe `UsdVolumeData`.
///
/// # Safety
///
/// All string pointers in `raw` must be null or point to valid NUL-terminated
/// C strings.
pub(crate) unsafe fn convert_volume(raw: &UsdBridgeVolumeDataRaw) -> UsdVolumeData {
    UsdVolumeData {
        path: c_str_to_string(raw.path),
        vdb_file_path: c_str_to_opt_string(raw.vdb_file_path),
        field_name: c_str_to_opt_string(raw.field_name),
        transform: f32x16_to_mat4(&raw.transform),
    }
}

/// Convert raw stage metadata from FFI to safe `UsdStageMetadata`.
pub(crate) fn convert_stage_metadata(raw: &UsdBridgeStageMetadataRaw) -> UsdStageMetadata {
    UsdStageMetadata {
        meters_per_unit: raw.meters_per_unit,
        up_axis: match raw.up_axis {
            UsdBridgeUpAxisRaw::Z => UpAxis::Z,
            _ => UpAxis::Y,
        },
        time_codes_per_second: raw.time_codes_per_second,
    }
}

/// Convert raw camera properties from FFI to safe `CameraProperties`.
pub(crate) fn convert_camera_properties(raw: &UsdBridgeCameraPropertiesRaw) -> CameraProperties {
    CameraProperties {
        focal_length: raw.focal_length,
        vertical_aperture: raw.vertical_aperture,
        clip_near: raw.clip_near,
        clip_far: raw.clip_far,
        horizontal_aperture: raw.horizontal_aperture,
    }
}

/// Convert raw mesh animation data from FFI to safe `UsdAnimatedMeshData`.
///
/// # Safety
///
/// `raw.xform_samples` must be null or point to `raw.xform_sample_count`
/// valid `UsdBridgeXformSampleRaw` values.
pub(crate) unsafe fn convert_mesh_animation(
    raw: &UsdBridgeAnimatedMeshDataRaw,
) -> UsdAnimatedMeshData {
    let xform_samples = if raw.xform_samples.is_null() || raw.xform_sample_count == 0 {
        Vec::new()
    } else {
        std::slice::from_raw_parts(raw.xform_samples, raw.xform_sample_count)
            .iter()
            .map(|s| TransformSample {
                time: s.time,
                transform: f32x16_to_mat4(&s.transform),
            })
            .collect()
    };

    UsdAnimatedMeshData {
        mesh_index: raw.mesh_index,
        xform_samples,
    }
}

/// Convert raw instancer animation data from FFI to safe `UsdAnimatedInstancerData`.
///
/// The transforms are reshaped from a flat array into
/// `transforms[time_idx][instance_idx]`.
///
/// # Safety
///
/// `raw.time_samples` must be null or point to `raw.time_sample_count` valid
/// f64 values. `raw.transforms` must be null or point to
/// `time_sample_count * instance_count * 16` valid f32 values.
pub(crate) unsafe fn convert_instancer_animation(
    raw: &UsdBridgeAnimatedInstancerDataRaw,
) -> UsdBridgeResult<UsdAnimatedInstancerData> {
    let ctx = format!(
        "instancer_animation[idx={}] time_samples={} instances={}",
        raw.instancer_index, raw.time_sample_count, raw.instance_count
    );

    let time_samples = if raw.time_samples.is_null() || raw.time_sample_count == 0 {
        Vec::new()
    } else {
        let n = checked_alloc_count::<f64>(raw.time_sample_count, &ctx)?;
        std::slice::from_raw_parts(raw.time_samples, n).to_vec()
    };

    let transforms =
        if raw.transforms.is_null() || raw.time_sample_count == 0 || raw.instance_count == 0 {
            Vec::new()
        } else {
            // Guard before touching the raw pointer — bad counts here would
            // otherwise feed `Vec::with_capacity` (and the global allocator)
            // and abort the process with STATUS_STACK_BUFFER_OVERRUN.
            let total_matrices =
                checked_mul_count(raw.time_sample_count, raw.instance_count, &ctx)?;
            let _ = checked_alloc_count::<Mat4>(total_matrices, &ctx)?;
            let flat_floats = checked_mul_count(total_matrices, 16, &ctx)?;
            let _ = checked_alloc_count::<f32>(flat_floats, &ctx)?;
            // Allocating the nested `Vec<Vec<Mat4>>` shell costs `time_sample_count`
            // `Vec<Mat4>` headers in the outer + `instance_count` Mat4 each row.
            let _ = checked_alloc_count::<Vec<Mat4>>(raw.time_sample_count, &ctx)?;

            let flat_data = std::slice::from_raw_parts(raw.transforms, flat_floats);

            let mut result = Vec::with_capacity(raw.time_sample_count);
            for time_idx in 0..raw.time_sample_count {
                let mut instances = Vec::with_capacity(raw.instance_count);
                for inst_idx in 0..raw.instance_count {
                    let offset = (time_idx * raw.instance_count + inst_idx) * 16;
                    let mut arr = [0.0f32; 16];
                    arr.copy_from_slice(&flat_data[offset..offset + 16]);
                    instances.push(Mat4::from_cols_array(&arr));
                }
                result.push(instances);
            }
            result
        };

    Ok(UsdAnimatedInstancerData {
        instancer_index: raw.instancer_index,
        time_samples,
        instance_count: raw.instance_count,
        transforms,
    })
}

/// Convert raw primvar data from FFI to safe `UsdPrimvarData`.
///
/// # Safety
///
/// `raw.float_data` and `raw.int_data` pointers must be valid for the
/// element counts implied by `raw.primvar_type` and `raw.element_count`.
pub(crate) unsafe fn convert_primvar(raw: &UsdBridgePrimvarDataRaw) -> UsdPrimvarData {
    let name = c_str_to_string(raw.name);

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

    let stride = match primvar_type {
        PrimvarType::Float => 1,
        PrimvarType::Float2 => 2,
        PrimvarType::Float3 => 3,
        PrimvarType::Int => 0,
    };
    let float_count = if stride == 0 {
        0
    } else {
        super::ffi_guard::safe_mul_count::<f32>(raw.element_count, stride, "primvar.float_data")
            .unwrap_or(0)
    };

    let float_data = if raw.float_data.is_null() || float_count == 0 {
        Vec::new()
    } else {
        std::slice::from_raw_parts(raw.float_data, float_count).to_vec()
    };

    let int_data = if raw.int_data.is_null()
        || raw.element_count == 0
        || !matches!(primvar_type, PrimvarType::Int)
    {
        Vec::new()
    } else {
        std::slice::from_raw_parts(raw.int_data, raw.element_count).to_vec()
    };

    UsdPrimvarData {
        name,
        primvar_type,
        interpolation,
        float_data,
        int_data,
        element_count: raw.element_count,
    }
}

// ============================================================================
// Layer-Aware Stage (v0.14.0)
// ============================================================================

/// Convert one raw layer info struct to safe `LayerInfo`.
///
/// # Safety
///
/// All string pointers in `raw` must be valid NUL-terminated C strings or null.
pub(crate) unsafe fn convert_layer_info(raw: &UsdBridgeLayerInfoRaw) -> LayerInfo {
    LayerInfo {
        identifier: c_str_to_string(raw.identifier),
        display_name: c_str_to_string(raw.display_name),
        real_path: PathBuf::from(c_str_to_string(raw.real_path)),
        is_anonymous: raw.is_anonymous != 0,
        is_dirty: raw.is_dirty != 0,
        is_muted: raw.is_muted != 0,
        permission_to_edit: raw.permission_to_edit != 0,
        offset: LayerOffset {
            offset: raw.time_offset,
            scale: raw.time_scale,
        },
        parent_index: if raw.parent_index < 0 {
            None
        } else {
            Some(raw.parent_index as usize)
        },
        depth: raw.depth,
    }
}

/// Convert a raw layer stack pointer to a safe `LayerStack`.
///
/// Returns an empty stack if `ptr` is null or `count == 0`.
///
/// # Safety
///
/// `ptr` must be null or point to a valid `UsdBridgeLayerStackRaw` whose
/// `layers` pointer + `count` describe a readable slice.
pub(crate) unsafe fn convert_layer_stack_ptr(ptr: *const UsdBridgeLayerStackRaw) -> LayerStack {
    if ptr.is_null() {
        return LayerStack {
            layers: Vec::new(),
            root_index: 0,
        };
    }
    let raw = &*ptr;
    let layers = if raw.layers.is_null() || raw.count == 0 {
        Vec::new()
    } else {
        std::slice::from_raw_parts(raw.layers, raw.count)
            .iter()
            .map(|r| convert_layer_info(r))
            .collect()
    };
    LayerStack {
        layers,
        root_index: raw.root_index,
    }
}

/// Convert one raw prim spec to a safe `PrimStackEntry`.
///
/// # Safety
///
/// All string pointers in `raw` must be valid NUL-terminated C strings or null.
pub(crate) unsafe fn convert_prim_spec(raw: &UsdBridgePrimSpecRaw) -> PrimStackEntry {
    PrimStackEntry {
        layer_identifier: c_str_to_string(raw.layer_identifier),
        path: c_str_to_string(raw.path),
        specifier: PrimSpecifier::from_u8(raw.specifier),
        has_authored_opinions: raw.has_authored_opinions != 0,
    }
}

/// Convert a raw prim stack pointer to a safe `Vec<PrimStackEntry>`.
///
/// # Safety
///
/// `ptr` must be null or point to a valid `UsdBridgePrimStackRaw`.
pub(crate) unsafe fn convert_prim_stack_ptr(
    ptr: *const UsdBridgePrimStackRaw,
) -> Vec<PrimStackEntry> {
    if ptr.is_null() {
        return Vec::new();
    }
    let raw = &*ptr;
    if raw.specs.is_null() || raw.count == 0 {
        return Vec::new();
    }
    std::slice::from_raw_parts(raw.specs, raw.count)
        .iter()
        .map(|r| convert_prim_spec(r))
        .collect()
}

/// Convert one raw opinion source to a safe `OpinionSource`.
///
/// # Safety
///
/// All string pointers in `raw` must be valid NUL-terminated C strings or null.
pub(crate) unsafe fn convert_opinion_source(
    raw: &UsdBridgeOpinionSourceRaw,
    is_winning: bool,
) -> OpinionSource {
    OpinionSource {
        layer_identifier: c_str_to_string(raw.layer_identifier),
        value_display: c_str_to_string(raw.value_display),
        value_type: c_str_to_string(raw.value_type_token),
        is_winning,
    }
}

/// Convert a raw attribute opinions pointer to a safe `Vec<OpinionSource>`.
/// The entry at `winning_index` gets `is_winning = true`.
///
/// # Safety
///
/// `ptr` must be null or point to a valid `UsdBridgeAttributeOpinionsRaw`.
pub(crate) unsafe fn convert_attribute_opinions_ptr(
    ptr: *const UsdBridgeAttributeOpinionsRaw,
) -> Vec<OpinionSource> {
    if ptr.is_null() {
        return Vec::new();
    }
    let raw = &*ptr;
    if raw.sources.is_null() || raw.count == 0 {
        return Vec::new();
    }
    let winning = raw.winning_index;
    std::slice::from_raw_parts(raw.sources, raw.count)
        .iter()
        .enumerate()
        .map(|(i, r)| convert_opinion_source(r, i == winning))
        .collect()
}

/// Convert a raw edit target struct to safe `EditTarget`.
///
/// # Safety
///
/// `raw.layer_identifier` must be valid NUL-terminated or null.
pub(crate) unsafe fn convert_edit_target(raw: &UsdBridgeEditTargetRaw) -> EditTarget {
    EditTarget {
        layer_identifier: c_str_to_string(raw.layer_identifier),
    }
}

/// Convert a raw layer offset to safe `LayerOffset` (plain struct copy).
pub(crate) fn convert_layer_offset(raw: &UsdBridgeLayerOffsetRaw) -> LayerOffset {
    LayerOffset {
        offset: raw.offset,
        scale: raw.scale,
    }
}

/// Map safe `PayloadPolicy` to its FFI raw enum.
pub(crate) fn payload_policy_to_raw(p: PayloadPolicy) -> UsdBridgePayloadPolicyRaw {
    match p {
        PayloadPolicy::LoadAll => UsdBridgePayloadPolicyRaw::LoadAll,
        PayloadPolicy::LoadNone => UsdBridgePayloadPolicyRaw::LoadNone,
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::super::cpp_bridge::UsdLightType;
    use super::super::ffi_raw::UsdBridgeLightType;
    use super::*;
    use std::ffi::CString;
    use std::ptr;

    // ---- Mesh tests ----

    #[test]
    fn test_convert_mesh_basic() {
        let verts = vec![1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0];
        let indices = vec![0u32, 1, 2];
        let path = CString::new("/World/Mesh").unwrap();
        let raw = UsdBridgeMeshDataRaw {
            path: path.as_ptr(),
            vertices: verts.as_ptr(),
            vertex_count: 2,
            indices: indices.as_ptr(),
            index_count: 3,
            normals: ptr::null(),
            normal_count: 0,
            uvs: ptr::null(),
            uv_count: 0,
            face_material_ids: ptr::null(),
            triangle_count: 0,
            transform: [
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
            ],
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
            vertices_orig: ptr::null(),
            vertex_count_orig: 0,
            facevarying_uvs: ptr::null(),
            facevarying_uv_count: 0,
            facevarying_uv_indices: ptr::null(),
            facevarying_uv_index_count: 0,
        };

        let result = unsafe { convert_mesh(&raw) };

        assert_eq!(result.path, "/World/Mesh");
        assert_eq!(result.vertices.len(), 2);
        assert_eq!(result.vertices[0], Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(result.vertices[1], Vec3::new(4.0, 5.0, 6.0));
        assert_eq!(result.indices, vec![0, 1, 2]);
        assert!(result.normals.is_none());
        assert!(result.uvs.is_none());
        assert!(result.face_material_ids.is_none());
        assert_eq!(result.purpose, MeshPurpose::Default);
        assert!(result.visible);
        assert!(!result.double_sided);
        assert!(!result.is_instance_proxy);
        assert_eq!(result.subdivision_scheme, SubdivisionScheme::None);
        assert_eq!(result.normals_interpolation, NormalsInterpolation::Vertex);
        assert_eq!(result.display_opacity, 1.0);
        assert!(!result.resets_xform_stack);
    }

    #[test]
    fn test_convert_mesh_empty() {
        let path = CString::new("").unwrap();
        let raw = UsdBridgeMeshDataRaw {
            path: path.as_ptr(),
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
            visibility: 0,
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
            vertices_orig: ptr::null(),
            vertex_count_orig: 0,
            facevarying_uvs: ptr::null(),
            facevarying_uv_count: 0,
            facevarying_uv_indices: ptr::null(),
            facevarying_uv_index_count: 0,
        };

        let result = unsafe { convert_mesh(&raw) };

        assert!(result.path.is_empty());
        assert!(result.vertices.is_empty());
        assert!(result.indices.is_empty());
        assert!(!result.visible);
    }

    #[test]
    fn test_convert_mesh_with_normals_uvs() {
        let verts = vec![0.0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
        let indices = vec![0u32, 1, 2];
        let normals = vec![0.0f32, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0];
        let uvs = vec![0.0f32, 0.0, 1.0, 0.0, 0.5, 1.0];
        let path = CString::new("/World/TriMesh").unwrap();
        let subdiv = CString::new("catmullClark").unwrap();

        let raw = UsdBridgeMeshDataRaw {
            path: path.as_ptr(),
            vertices: verts.as_ptr(),
            vertex_count: 3,
            indices: indices.as_ptr(),
            index_count: 3,
            normals: normals.as_ptr(),
            normal_count: 3,
            uvs: uvs.as_ptr(),
            uv_count: 3,
            face_material_ids: ptr::null(),
            triangle_count: 0,
            transform: [
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
            ],
            purpose: UsdBridgePurposeRaw::Render,
            is_instance_proxy: 1,
            visibility: 1,
            double_sided: 1,
            subdivision_scheme: subdiv.as_ptr(),
            normals_interpolation: 1,
            display_color: ptr::null(),
            display_color_count: 0,
            display_opacity: 0.8,
            resets_xform_stack: 1,
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
            vertices_orig: ptr::null(),
            vertex_count_orig: 0,
            facevarying_uvs: ptr::null(),
            facevarying_uv_count: 0,
            facevarying_uv_indices: ptr::null(),
            facevarying_uv_index_count: 0,
        };

        let result = unsafe { convert_mesh(&raw) };

        assert_eq!(result.path, "/World/TriMesh");
        assert_eq!(result.vertices.len(), 3);
        let norms = result.normals.unwrap();
        assert_eq!(norms.len(), 3);
        assert_eq!(norms[0], Vec3::new(0.0, 0.0, 1.0));
        let uv_data = result.uvs.unwrap();
        assert_eq!(uv_data.len(), 3);
        assert_eq!(uv_data[2], [0.5, 1.0]);
        assert_eq!(result.purpose, MeshPurpose::Render);
        assert!(result.is_instance_proxy);
        assert!(result.double_sided);
        assert_eq!(result.subdivision_scheme, SubdivisionScheme::CatmullClark);
        assert_eq!(
            result.normals_interpolation,
            NormalsInterpolation::FaceVarying
        );
        assert_eq!(result.display_opacity, 0.8);
        assert!(result.resets_xform_stack);
    }

    #[test]
    fn test_convert_mesh_with_display_color_and_creases() {
        let verts = vec![0.0f32; 9];
        let indices = vec![0u32, 1, 2];
        let display_color = vec![1.0f32, 0.0, 0.0, 0.0, 1.0, 0.0];
        let crease_idx = vec![0i32, 1, 2, 3];
        let crease_len = vec![2i32, 2];
        let crease_sharp = vec![5.0f32, 3.0];
        let fvc = vec![3i32];
        let fvi = vec![0i32, 1, 2];
        let path = CString::new("/creased").unwrap();

        let raw = UsdBridgeMeshDataRaw {
            path: path.as_ptr(),
            vertices: verts.as_ptr(),
            vertex_count: 3,
            indices: indices.as_ptr(),
            index_count: 3,
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
            display_color: display_color.as_ptr(),
            display_color_count: 2,
            display_opacity: 1.0,
            resets_xform_stack: 0,
            face_vertex_counts: fvc.as_ptr(),
            face_count: 1,
            face_vertex_indices: fvi.as_ptr(),
            face_vertex_index_count: 3,
            crease_indices: crease_idx.as_ptr(),
            crease_index_count: 4,
            crease_lengths: crease_len.as_ptr(),
            crease_length_count: 2,
            crease_sharpnesses: crease_sharp.as_ptr(),
            crease_sharpness_count: 2,
            vertices_orig: ptr::null(),
            vertex_count_orig: 0,
            facevarying_uvs: ptr::null(),
            facevarying_uv_count: 0,
            facevarying_uv_indices: ptr::null(),
            facevarying_uv_index_count: 0,
        };

        let result = unsafe { convert_mesh(&raw) };

        let dc = result.display_color.unwrap();
        assert_eq!(dc.len(), 2);
        assert_eq!(dc[0], Vec3::new(1.0, 0.0, 0.0));
        assert_eq!(dc[1], Vec3::new(0.0, 1.0, 0.0));
        assert_eq!(result.face_vertex_counts.unwrap(), vec![3]);
        assert_eq!(result.face_vertex_indices.unwrap(), vec![0, 1, 2]);
        assert_eq!(result.crease_indices.unwrap(), vec![0, 1, 2, 3]);
        assert_eq!(result.crease_lengths.unwrap(), vec![2, 2]);
        assert_eq!(result.crease_sharpnesses.unwrap(), vec![5.0, 3.0]);
    }

    #[test]
    fn test_convert_mesh_face_material_ids() {
        let verts = vec![0.0f32; 9];
        let indices = vec![0u32, 1, 2];
        let mat_ids = vec![0u32, 1, 2];
        let path = CString::new("/mat_ids").unwrap();

        let raw = UsdBridgeMeshDataRaw {
            path: path.as_ptr(),
            vertices: verts.as_ptr(),
            vertex_count: 3,
            indices: indices.as_ptr(),
            index_count: 3,
            normals: ptr::null(),
            normal_count: 0,
            uvs: ptr::null(),
            uv_count: 0,
            face_material_ids: mat_ids.as_ptr(),
            triangle_count: 3,
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
            vertices_orig: ptr::null(),
            vertex_count_orig: 0,
            facevarying_uvs: ptr::null(),
            facevarying_uv_count: 0,
            facevarying_uv_indices: ptr::null(),
            facevarying_uv_index_count: 0,
        };

        let result = unsafe { convert_mesh(&raw) };
        assert_eq!(result.face_material_ids.unwrap(), vec![0, 1, 2]);
    }

    #[test]
    fn test_convert_mesh_subdivision_schemes() {
        let path = CString::new("/subdiv").unwrap();
        let verts = vec![0.0f32; 3];

        // Test loop
        let loop_scheme = CString::new("loop").unwrap();
        let raw = UsdBridgeMeshDataRaw {
            path: path.as_ptr(),
            vertices: verts.as_ptr(),
            vertex_count: 1,
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
            subdivision_scheme: loop_scheme.as_ptr(),
            normals_interpolation: 2,
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
            vertices_orig: ptr::null(),
            vertex_count_orig: 0,
            facevarying_uvs: ptr::null(),
            facevarying_uv_count: 0,
            facevarying_uv_indices: ptr::null(),
            facevarying_uv_index_count: 0,
        };
        let result = unsafe { convert_mesh(&raw) };
        assert_eq!(result.subdivision_scheme, SubdivisionScheme::Loop);
        assert_eq!(result.normals_interpolation, NormalsInterpolation::Uniform);

        // Test bilinear
        let bilinear_scheme = CString::new("bilinear").unwrap();
        let raw2 = UsdBridgeMeshDataRaw {
            subdivision_scheme: bilinear_scheme.as_ptr(),
            normals_interpolation: 3,
            ..raw
        };
        let result2 = unsafe { convert_mesh(&raw2) };
        assert_eq!(result2.subdivision_scheme, SubdivisionScheme::Bilinear);
        assert_eq!(
            result2.normals_interpolation,
            NormalsInterpolation::Constant
        );
    }

    // ---- Instancer tests ----

    #[test]
    fn test_convert_instancer_basic() {
        let path = CString::new("/World/Instancer").unwrap();
        let proto1 = CString::new("/World/Proto1").unwrap();
        let proto2 = CString::new("/World/Proto2").unwrap();
        let proto_ptrs = vec![proto1.as_ptr(), proto2.as_ptr()];
        // 2 instances: identity transforms
        let transforms = vec![
            1.0f32, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 2.0,
            0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ];
        let proto_indices = vec![0i32, 1];

        let raw = UsdBridgeInstancerDataRaw {
            path: path.as_ptr(),
            prototype_paths: proto_ptrs.as_ptr(),
            prototype_count: 2,
            transforms: transforms.as_ptr(),
            instance_count: 2,
            proto_indices: proto_indices.as_ptr(),
            velocities: ptr::null(),
            velocity_count: 0,
            angular_velocities: ptr::null(),
            angular_velocity_count: 0,
            invisible_ids: ptr::null(),
            invisible_id_count: 0,
        };

        let result = unsafe { convert_instancer(&raw) };

        assert_eq!(result.path, "/World/Instancer");
        assert_eq!(result.prototype_paths.len(), 2);
        assert_eq!(result.prototype_paths[0], "/World/Proto1");
        assert_eq!(result.prototype_paths[1], "/World/Proto2");
        assert_eq!(result.transforms.len(), 2);
        assert_eq!(result.proto_indices, vec![0, 1]);
        assert!(result.velocities.is_none());
        assert!(result.angular_velocities.is_none());
        assert!(result.invisible_ids.is_empty());
    }

    #[test]
    fn test_convert_instancer_with_velocities() {
        let path = CString::new("/inst").unwrap();
        let transforms = vec![0.0f32; 16];
        let proto_indices = vec![0i32];
        let velocities = vec![1.0f32, 2.0, 3.0];
        let angular_velocities = vec![0.1f32, 0.2, 0.3];
        let invisible_ids = vec![5i64, 10];

        let raw = UsdBridgeInstancerDataRaw {
            path: path.as_ptr(),
            prototype_paths: ptr::null(),
            prototype_count: 0,
            transforms: transforms.as_ptr(),
            instance_count: 1,
            proto_indices: proto_indices.as_ptr(),
            velocities: velocities.as_ptr(),
            velocity_count: 1,
            angular_velocities: angular_velocities.as_ptr(),
            angular_velocity_count: 1,
            invisible_ids: invisible_ids.as_ptr(),
            invisible_id_count: 2,
        };

        let result = unsafe { convert_instancer(&raw) };

        let v = result.velocities.unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0], Vec3::new(1.0, 2.0, 3.0));
        let av = result.angular_velocities.unwrap();
        assert_eq!(av[0], Vec3::new(0.1, 0.2, 0.3));
        assert_eq!(result.invisible_ids, vec![5, 10]);
    }

    // ---- Native instance test ----

    #[test]
    fn test_convert_native_instance() {
        let raw = UsdNativeInstanceDataRaw {
            proto_mesh_idx: 3,
            transform: [
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 5.0, 6.0, 7.0, 1.0,
            ],
            material_override_idx: 2,
            purpose: 1,
        };

        let result = unsafe { convert_native_instance(&raw) };

        assert_eq!(result.proto_mesh_idx, 3);
        assert_eq!(result.material_override_idx, 2);
        assert_eq!(result.purpose, MeshPurpose::Render);
    }

    #[test]
    fn test_convert_native_instance_no_override() {
        let raw = UsdNativeInstanceDataRaw {
            proto_mesh_idx: 0,
            transform: [0.0; 16],
            material_override_idx: -1,
            purpose: 0,
        };

        let result = unsafe { convert_native_instance(&raw) };

        assert_eq!(result.proto_mesh_idx, 0);
        assert_eq!(result.material_override_idx, -1);
        assert_eq!(result.purpose, MeshPurpose::Default);
    }

    // ---- Material tests ----

    #[test]
    fn test_convert_material_basic() {
        let path = CString::new("/World/Looks/Metal").unwrap();

        let raw = UsdBridgeMaterialDataRaw {
            path: path.as_ptr(),
            diffuse_color: [0.8, 0.7, 0.6],
            metallic: 1.0,
            roughness: 0.3,
            specular: 0.5,
            opacity: 1.0,
            transmission: 0.0,
            specular_ior: 1.5,
            emissive_color: [0.0, 0.0, 0.0],
            diffuse_texture: ptr::null(),
            roughness_texture: ptr::null(),
            metallic_texture: ptr::null(),
            normal_texture: ptr::null(),
            emissive_texture: ptr::null(),
            opacity_texture: ptr::null(),
            is_materialx: 0,
            displacement_texture: ptr::null(),
            displacement_scale: 1.0,
        };

        let result = unsafe { convert_material(&raw) };

        assert_eq!(result.path, "/World/Looks/Metal");
        assert_eq!(result.base_color, Vec3::new(0.8, 0.7, 0.6));
        assert_eq!(result.base_metalness, 1.0);
        assert_eq!(result.specular_roughness, 0.3);
        assert_eq!(result.specular_weight, 0.5);
        assert_eq!(result.specular_ior, 1.5);
        assert_eq!(result.transmission_weight, 0.0);
        assert_eq!(result.geometry_opacity, 1.0);
        assert!(result.base_color_texture.is_none());
        assert!(!result.is_materialx);
    }

    #[test]
    fn test_convert_material_with_textures() {
        let path = CString::new("/Looks/Textured").unwrap();
        let diff_tex = CString::new("/textures/albedo.png").unwrap();
        let rough_tex = CString::new("/textures/roughness.png").unwrap();
        let metal_tex = CString::new("/textures/metallic.png").unwrap();
        let norm_tex = CString::new("/textures/normal.png").unwrap();
        let emis_tex = CString::new("/textures/emissive.png").unwrap();
        let opac_tex = CString::new("/textures/opacity.png").unwrap();

        let raw = UsdBridgeMaterialDataRaw {
            path: path.as_ptr(),
            diffuse_color: [1.0, 1.0, 1.0],
            metallic: 0.0,
            roughness: 0.5,
            specular: 0.5,
            opacity: 1.0,
            transmission: 0.1,
            specular_ior: 1.45,
            emissive_color: [1.0, 0.5, 0.0],
            diffuse_texture: diff_tex.as_ptr(),
            roughness_texture: rough_tex.as_ptr(),
            metallic_texture: metal_tex.as_ptr(),
            normal_texture: norm_tex.as_ptr(),
            emissive_texture: emis_tex.as_ptr(),
            opacity_texture: opac_tex.as_ptr(),
            is_materialx: 1,
            displacement_texture: ptr::null(),
            displacement_scale: 1.0,
        };

        let result = unsafe { convert_material(&raw) };

        assert_eq!(result.base_color_texture.unwrap(), "/textures/albedo.png");
        assert_eq!(
            result.specular_roughness_texture.unwrap(),
            "/textures/roughness.png"
        );
        assert_eq!(
            result.base_metalness_texture.unwrap(),
            "/textures/metallic.png"
        );
        assert_eq!(result.normal_texture.unwrap(), "/textures/normal.png");
        assert_eq!(result.emission_texture.unwrap(), "/textures/emissive.png");
        assert_eq!(
            result.geometry_opacity_texture.unwrap(),
            "/textures/opacity.png"
        );
        assert!(result.is_materialx);
        assert_eq!(result.transmission_weight, 0.1);
        assert_eq!(result.specular_ior, 1.45);
        assert_eq!(result.emission_color, Vec3::new(1.0, 0.5, 0.0));
    }

    #[test]
    fn test_convert_material_empty_texture_is_none() {
        let path = CString::new("/mat").unwrap();
        let empty_tex = CString::new("").unwrap();

        let raw = UsdBridgeMaterialDataRaw {
            path: path.as_ptr(),
            diffuse_color: [0.5, 0.5, 0.5],
            metallic: 0.0,
            roughness: 0.5,
            specular: 0.5,
            opacity: 1.0,
            transmission: 0.0,
            specular_ior: 1.5,
            emissive_color: [0.0, 0.0, 0.0],
            diffuse_texture: empty_tex.as_ptr(),
            roughness_texture: ptr::null(),
            metallic_texture: ptr::null(),
            normal_texture: ptr::null(),
            emissive_texture: ptr::null(),
            opacity_texture: ptr::null(),
            is_materialx: 0,
            displacement_texture: ptr::null(),
            displacement_scale: 1.0,
        };

        let result = unsafe { convert_material(&raw) };
        // Empty string should map to None
        assert!(result.base_color_texture.is_none());
    }

    // ---- Light tests ----

    #[test]
    fn test_convert_light_distant() {
        let path = CString::new("/World/Sun").unwrap();

        let raw = UsdBridgeLightDataRaw {
            path: path.as_ptr(),
            light_type: UsdBridgeLightType::Distant,
            color: [1.0, 0.95, 0.9],
            intensity: 500.0,
            exposure: 0.0,
            transform: [
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
            ],
            angle: 0.53,
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

        let result = unsafe { convert_light(&raw) };

        assert_eq!(result.path, "/World/Sun");
        assert_eq!(result.light_type, UsdLightType::Distant);
        assert_eq!(result.color, Vec3::new(1.0, 0.95, 0.9));
        assert_eq!(result.intensity, 500.0);
        assert_eq!(result.angle, 0.53);
        assert!(result.texture_path.is_none());
        assert!(result.light_link_includes.is_empty());
        assert!(result.light_link_excludes.is_empty());
    }

    #[test]
    fn test_convert_light_dome_with_texture() {
        let path = CString::new("/World/DomeLight").unwrap();
        let tex = CString::new("/textures/env.hdr").unwrap();

        let raw = UsdBridgeLightDataRaw {
            path: path.as_ptr(),
            light_type: UsdBridgeLightType::Dome,
            color: [1.0, 1.0, 1.0],
            intensity: 1.0,
            exposure: 2.0,
            transform: [0.0; 16],
            angle: 0.0,
            radius: 0.0,
            width: 0.0,
            height: 0.0,
            texture_path: tex.as_ptr(),
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

        let result = unsafe { convert_light(&raw) };

        assert_eq!(result.texture_path.unwrap(), "/textures/env.hdr");
        // intensity * 2^exposure = 1.0 * 4.0 = 4.0
        assert_eq!(result.intensity, 4.0);
    }

    #[test]
    fn test_convert_light_with_shaping_and_linking() {
        let path = CString::new("/spotlight").unwrap();
        let ies = CString::new("/profiles/spot.ies").unwrap();
        let inc1 = CString::new("/World/Geo").unwrap();
        let exc1 = CString::new("/World/Background").unwrap();
        let includes = vec![inc1.as_ptr()];
        let excludes = vec![exc1.as_ptr()];

        let raw = UsdBridgeLightDataRaw {
            path: path.as_ptr(),
            light_type: UsdBridgeLightType::Sphere,
            color: [1.0, 1.0, 1.0],
            intensity: 100.0,
            exposure: 0.0,
            transform: [0.0; 16],
            angle: 0.0,
            radius: 0.5,
            width: 0.0,
            height: 0.0,
            texture_path: ptr::null(),
            length: 0.0,
            shaping_cone_angle: 45.0,
            shaping_cone_softness: 0.25,
            shaping_focus: 1.0,
            shaping_ies_file: ies.as_ptr(),
            light_link_includes: includes.as_ptr(),
            light_link_include_count: 1,
            light_link_excludes: excludes.as_ptr(),
            light_link_exclude_count: 1,
        };

        let result = unsafe { convert_light(&raw) };

        assert_eq!(result.shaping.cone_angle, 45.0);
        assert_eq!(result.shaping.cone_softness, 0.25);
        assert_eq!(result.shaping.focus, 1.0);
        assert_eq!(result.shaping.ies_file.unwrap(), "/profiles/spot.ies");
        assert_eq!(result.light_link_includes, vec!["/World/Geo"]);
        assert_eq!(result.light_link_excludes, vec!["/World/Background"]);
    }

    // ---- Prim info test ----

    #[test]
    fn test_convert_prim_info() {
        let path = CString::new("/World/Xform").unwrap();
        let type_name = CString::new("Xform").unwrap();

        let raw = UsdBridgePrimInfoRaw {
            path: path.as_ptr(),
            type_name: type_name.as_ptr(),
            is_active: 1,
            has_children: 1,
            child_count: 3,
            visibility: 1,
            has_payload: 0,
            is_loaded: 1,
            variant_set_count: 2,
            has_inherits: 0,
            has_specializes: 1,
        };

        let result = unsafe { convert_prim_info(&raw) };

        assert_eq!(result.path, "/World/Xform");
        assert_eq!(result.type_name, "Xform");
        assert!(result.is_active);
        assert!(result.has_children);
        assert_eq!(result.child_count, 3);
        assert!(result.visible);
        assert!(!result.has_payload);
        assert!(result.is_loaded);
        assert_eq!(result.variant_set_count, 2);
        assert!(!result.has_inherits);
        assert!(result.has_specializes);
    }

    // ---- Timeline test ----

    #[test]
    fn test_convert_timeline() {
        let raw = UsdBridgeTimelineDataRaw {
            start_time_code: 1.0,
            end_time_code: 100.0,
            frames_per_second: 30.0,
            has_authored_time_range: 1,
        };

        let result = convert_timeline(&raw);

        assert_eq!(result.start_time_code, 1.0);
        assert_eq!(result.end_time_code, 100.0);
        assert_eq!(result.frames_per_second, 30.0);
        assert!(result.has_authored_time_range);
    }

    #[test]
    fn test_convert_timeline_no_authored_range() {
        let raw = UsdBridgeTimelineDataRaw {
            start_time_code: 0.0,
            end_time_code: 0.0,
            frames_per_second: 24.0,
            has_authored_time_range: 0,
        };

        let result = convert_timeline(&raw);
        assert!(!result.has_authored_time_range);
    }

    // ---- Stage metadata test ----

    #[test]
    fn test_convert_stage_metadata() {
        let raw = UsdBridgeStageMetadataRaw {
            meters_per_unit: 0.01,
            up_axis: UsdBridgeUpAxisRaw::Z,
            time_codes_per_second: 30.0,
        };

        let result = convert_stage_metadata(&raw);

        assert_eq!(result.meters_per_unit, 0.01);
        assert_eq!(result.up_axis, UpAxis::Z);
        assert_eq!(result.time_codes_per_second, 30.0);
    }

    #[test]
    fn test_convert_stage_metadata_y_up() {
        let raw = UsdBridgeStageMetadataRaw {
            meters_per_unit: 1.0,
            up_axis: UsdBridgeUpAxisRaw::Y,
            time_codes_per_second: 24.0,
        };

        let result = convert_stage_metadata(&raw);
        assert_eq!(result.up_axis, UpAxis::Y);
    }

    // ---- Camera properties test ----

    #[test]
    fn test_convert_camera_properties() {
        let raw = UsdBridgeCameraPropertiesRaw {
            focal_length: 50.0,
            vertical_aperture: 15.2908,
            clip_near: 0.1,
            clip_far: 10000.0,
            horizontal_aperture: 20.955,
        };

        let result = convert_camera_properties(&raw);

        assert_eq!(result.focal_length, 50.0);
        assert_eq!(result.vertical_aperture, 15.2908);
        assert_eq!(result.clip_near, 0.1);
        assert_eq!(result.clip_far, 10000.0);
        assert_eq!(result.horizontal_aperture, 20.955);
    }

    // ---- Points test ----

    #[test]
    fn test_convert_points() {
        let path = CString::new("/particles").unwrap();
        let positions = vec![1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0];
        let widths = vec![0.1f32, 0.2];
        let ids = vec![100i64, 200];

        let raw = UsdBridgePointsDataRaw {
            path: path.as_ptr(),
            positions: positions.as_ptr(),
            point_count: 2,
            widths: widths.as_ptr(),
            width_count: 2,
            normals: ptr::null(),
            normal_count: 0,
            ids: ids.as_ptr(),
            id_count: 2,
            transform: [
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
            ],
        };

        let result = unsafe { convert_points(&raw) };

        assert_eq!(result.path, "/particles");
        assert_eq!(result.positions.len(), 2);
        assert_eq!(result.positions[0], Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(result.widths.unwrap(), vec![0.1, 0.2]);
        assert!(result.normals.is_none());
        assert_eq!(result.ids.unwrap(), vec![100, 200]);
    }

    // ---- Curves test ----

    #[test]
    fn test_convert_curves() {
        let path = CString::new("/hair").unwrap();
        let points = vec![0.0f32, 0.0, 0.0, 1.0, 1.0, 0.0, 2.0, 0.0, 0.0];
        let widths = vec![0.01f32, 0.005, 0.001];
        let counts = vec![3i32];

        let raw = UsdBridgeCurvesDataRaw {
            path: path.as_ptr(),
            points: points.as_ptr(),
            point_count: 3,
            widths: widths.as_ptr(),
            width_count: 3,
            curve_vertex_counts: counts.as_ptr(),
            curve_count: 1,
            curve_type: UsdBridgeCurveTypeRaw::Cubic,
            basis: UsdBridgeCurveBasisRaw::CatmullRom,
            wrap: UsdBridgeCurveWrapRaw::Nonperiodic,
            transform: [0.0; 16],
        };

        let result = unsafe { convert_curves(&raw) };

        assert_eq!(result.path, "/hair");
        assert_eq!(result.points.len(), 3);
        assert_eq!(result.widths.unwrap(), vec![0.01, 0.005, 0.001]);
        assert_eq!(result.curve_vertex_counts, vec![3]);
        assert_eq!(result.curve_type, CurveType::Cubic);
        assert_eq!(result.basis, CurveBasis::CatmullRom);
        assert_eq!(result.wrap, CurveWrap::Nonperiodic);
    }

    // ---- Skeleton test ----

    #[test]
    fn test_convert_skeleton() {
        let path = CString::new("/Skel").unwrap();
        let j1 = CString::new("Root").unwrap();
        let j2 = CString::new("Root/Spine").unwrap();
        let joint_ptrs = vec![j1.as_ptr(), j2.as_ptr()];
        // 2 identity Mat4s = 32 floats
        let bind_xforms = vec![
            1.0f32, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0,
            0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ];
        let rest_xforms = bind_xforms.clone();

        let raw = UsdBridgeSkeletonDataRaw {
            path: path.as_ptr(),
            joint_paths: joint_ptrs.as_ptr(),
            joint_count: 2,
            bind_transforms: bind_xforms.as_ptr(),
            rest_transforms: rest_xforms.as_ptr(),
        };

        let result = unsafe { convert_skeleton(&raw) };

        assert_eq!(result.path, "/Skel");
        assert_eq!(result.joint_paths, vec!["Root", "Root/Spine"]);
        assert_eq!(result.bind_transforms.len(), 2);
        assert_eq!(result.rest_transforms.len(), 2);
    }

    // ---- Skin binding test ----

    #[test]
    fn test_convert_skin_binding() {
        let mesh_path = CString::new("/Mesh").unwrap();
        let skel_path = CString::new("/Skel").unwrap();
        let joint_idx = vec![0i32, 1, 0, 1];
        let joint_wts = vec![0.8f32, 0.2, 0.3, 0.7];

        let raw = UsdBridgeSkinBindingDataRaw {
            mesh_path: mesh_path.as_ptr(),
            skeleton_path: skel_path.as_ptr(),
            joint_indices: joint_idx.as_ptr(),
            joint_indices_count: 4,
            joint_weights: joint_wts.as_ptr(),
            joint_weights_count: 4,
            joint_indices_element_size: 2,
            geom_bind_transform: [
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
            ],
            skel_root_world_xform: [
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
            ],
            is_rigid: 0,
        };

        let result = unsafe { convert_skin_binding(&raw) };

        assert_eq!(result.mesh_path, "/Mesh");
        assert_eq!(result.skeleton_path, "/Skel");
        assert_eq!(result.joint_indices, vec![0, 1, 0, 1]);
        assert_eq!(result.joint_weights, vec![0.8, 0.2, 0.3, 0.7]);
        assert_eq!(result.element_size, 2);
    }

    // ---- Volume test ----

    #[test]
    fn test_convert_volume() {
        let path = CString::new("/World/Smoke").unwrap();
        let vdb = CString::new("/volumes/smoke.vdb").unwrap();
        let field = CString::new("density").unwrap();

        let raw = UsdBridgeVolumeDataRaw {
            path: path.as_ptr(),
            vdb_file_path: vdb.as_ptr(),
            field_name: field.as_ptr(),
            transform: [0.0; 16],
        };

        let result = unsafe { convert_volume(&raw) };

        assert_eq!(result.path, "/World/Smoke");
        assert_eq!(result.vdb_file_path.unwrap(), "/volumes/smoke.vdb");
        assert_eq!(result.field_name.unwrap(), "density");
    }

    #[test]
    fn test_convert_volume_no_vdb() {
        let path = CString::new("/World/Empty").unwrap();

        let raw = UsdBridgeVolumeDataRaw {
            path: path.as_ptr(),
            vdb_file_path: ptr::null(),
            field_name: ptr::null(),
            transform: [0.0; 16],
        };

        let result = unsafe { convert_volume(&raw) };

        assert!(result.vdb_file_path.is_none());
        assert!(result.field_name.is_none());
    }

    // ---- Primvar test ----

    #[test]
    fn test_convert_primvar_float3() {
        let name = CString::new("Cd").unwrap();
        let float_data = vec![1.0f32, 0.0, 0.0, 0.0, 1.0, 0.0];

        let raw = UsdBridgePrimvarDataRaw {
            name: name.as_ptr(),
            primvar_type: UsdBridgePrimvarTypeRaw::Float3,
            interpolation: UsdBridgePrimvarInterpolationRaw::Vertex,
            float_data: float_data.as_ptr(),
            int_data: ptr::null(),
            element_count: 2,
        };

        let result = unsafe { convert_primvar(&raw) };

        assert_eq!(result.name, "Cd");
        assert_eq!(result.primvar_type, PrimvarType::Float3);
        assert_eq!(result.interpolation, PrimvarInterpolation::Vertex);
        assert_eq!(result.float_data, vec![1.0, 0.0, 0.0, 0.0, 1.0, 0.0]);
        assert!(result.int_data.is_empty());
        assert_eq!(result.element_count, 2);
    }

    #[test]
    fn test_convert_primvar_int() {
        let name = CString::new("materialIndex").unwrap();
        let int_data = vec![0i32, 1, 2, 0];

        let raw = UsdBridgePrimvarDataRaw {
            name: name.as_ptr(),
            primvar_type: UsdBridgePrimvarTypeRaw::Int,
            interpolation: UsdBridgePrimvarInterpolationRaw::Uniform,
            float_data: ptr::null(),
            int_data: int_data.as_ptr(),
            element_count: 4,
        };

        let result = unsafe { convert_primvar(&raw) };

        assert_eq!(result.name, "materialIndex");
        assert_eq!(result.primvar_type, PrimvarType::Int);
        assert_eq!(result.interpolation, PrimvarInterpolation::Uniform);
        assert!(result.float_data.is_empty());
        assert_eq!(result.int_data, vec![0, 1, 2, 0]);
    }

    // ---- Mesh animation test ----

    #[test]
    fn test_convert_mesh_animation() {
        use super::super::ffi_raw::UsdBridgeXformSampleRaw;

        let samples = vec![
            UsdBridgeXformSampleRaw {
                time: 1.0,
                transform: [
                    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
                ],
            },
            UsdBridgeXformSampleRaw {
                time: 24.0,
                transform: [
                    2.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0, 1.0,
                ],
            },
        ];

        let raw = UsdBridgeAnimatedMeshDataRaw {
            mesh_index: 5,
            xform_samples: samples.as_ptr(),
            xform_sample_count: 2,
        };

        let result = unsafe { convert_mesh_animation(&raw) };

        assert_eq!(result.mesh_index, 5);
        assert_eq!(result.xform_samples.len(), 2);
        assert_eq!(result.xform_samples[0].time, 1.0);
        assert_eq!(result.xform_samples[1].time, 24.0);
    }

    #[test]
    fn test_convert_mesh_animation_empty() {
        let raw = UsdBridgeAnimatedMeshDataRaw {
            mesh_index: 0,
            xform_samples: ptr::null(),
            xform_sample_count: 0,
        };

        let result = unsafe { convert_mesh_animation(&raw) };
        assert!(result.xform_samples.is_empty());
    }

    // ---- Instancer animation test ----

    #[test]
    fn test_convert_instancer_animation() {
        let time_samples = vec![1.0f64, 24.0];
        // 2 time samples * 2 instances * 16 floats = 64 floats
        let mut transforms = vec![0.0f32; 64];
        // Set first instance at t=0 to identity
        transforms[0] = 1.0;
        transforms[5] = 1.0;
        transforms[10] = 1.0;
        transforms[15] = 1.0;
        // Set second instance at t=0 to identity
        transforms[16] = 1.0;
        transforms[21] = 1.0;
        transforms[26] = 1.0;
        transforms[31] = 1.0;

        let raw = UsdBridgeAnimatedInstancerDataRaw {
            instancer_index: 3,
            time_samples: time_samples.as_ptr(),
            time_sample_count: 2,
            instance_count: 2,
            transforms: transforms.as_ptr(),
        };

        let result = unsafe { convert_instancer_animation(&raw) }.unwrap();

        assert_eq!(result.instancer_index, 3);
        assert_eq!(result.time_samples, vec![1.0, 24.0]);
        assert_eq!(result.instance_count, 2);
        assert_eq!(result.transforms.len(), 2); // 2 time samples
        assert_eq!(result.transforms[0].len(), 2); // 2 instances per time
    }

    #[test]
    fn test_convert_instancer_animation_empty() {
        let raw = UsdBridgeAnimatedInstancerDataRaw {
            instancer_index: 0,
            time_samples: ptr::null(),
            time_sample_count: 0,
            instance_count: 0,
            transforms: ptr::null(),
        };

        let result = unsafe { convert_instancer_animation(&raw) }.unwrap();
        assert!(result.time_samples.is_empty());
        assert!(result.transforms.is_empty());
    }

    // ---- Regression: rt_010_base.usda 24 GiB OOM ----

    #[test]
    fn test_convert_instancer_animation_rejects_oversized() {
        // Replicates the production shot that OOM-aborted bif_viewer:
        // 16_000 timecodes × 25_000 instances × sizeof(Mat4) ≈ 25 GiB.
        // Should return AllocTooLarge instead of panicking on Vec::with_capacity.
        let raw = UsdBridgeAnimatedInstancerDataRaw {
            instancer_index: 7,
            // Non-null pointer is fine — the guard fires before we deref it.
            time_samples: ptr::null(),
            time_sample_count: 16_000,
            instance_count: 25_000,
            transforms: 0x1 as *const f32, // sentinel; guard runs first
        };

        let result = unsafe { convert_instancer_animation(&raw) };
        match result {
            Err(super::super::cpp_bridge::UsdBridgeError::AllocTooLarge {
                requested_bytes,
                max_bytes,
                ..
            }) => {
                assert!(requested_bytes > max_bytes);
            }
            other => panic!("expected AllocTooLarge, got {other:?}"),
        }
    }

    // ---- Helper function tests ----

    #[test]
    fn test_c_str_to_string_null() {
        let result = unsafe { c_str_to_string(ptr::null()) };
        assert!(result.is_empty());
    }

    #[test]
    fn test_c_str_to_string_valid() {
        let s = CString::new("hello").unwrap();
        let result = unsafe { c_str_to_string(s.as_ptr()) };
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_c_str_to_opt_string_null() {
        let result = unsafe { c_str_to_opt_string(ptr::null()) };
        assert!(result.is_none());
    }

    #[test]
    fn test_c_str_to_opt_string_empty() {
        let s = CString::new("").unwrap();
        let result = unsafe { c_str_to_opt_string(s.as_ptr()) };
        assert!(result.is_none());
    }

    #[test]
    fn test_c_str_to_opt_string_valid() {
        let s = CString::new("texture.png").unwrap();
        let result = unsafe { c_str_to_opt_string(s.as_ptr()) };
        assert_eq!(result.unwrap(), "texture.png");
    }

    #[test]
    fn test_f32_ptr_to_vec3s_null() {
        let result = unsafe { f32_ptr_to_vec3s(ptr::null(), 10) };
        assert!(result.is_empty());
    }

    #[test]
    fn test_f32_ptr_to_vec3s_zero_count() {
        let data = vec![1.0f32, 2.0, 3.0];
        let result = unsafe { f32_ptr_to_vec3s(data.as_ptr(), 0) };
        assert!(result.is_empty());
    }

    #[test]
    fn test_f32x16_to_mat4_identity() {
        let arr = [
            1.0f32, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ];
        let result = f32x16_to_mat4(&arr);
        assert_eq!(result, Mat4::IDENTITY);
    }

    #[test]
    fn test_f32_ptr_to_mat4s() {
        let data = vec![
            1.0f32, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ];
        let result = unsafe { f32_ptr_to_mat4s(data.as_ptr(), 1) };
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], Mat4::IDENTITY);
    }

    #[test]
    fn test_convert_purpose_raw_all_variants() {
        assert_eq!(
            convert_purpose_raw(UsdBridgePurposeRaw::Default),
            MeshPurpose::Default
        );
        assert_eq!(
            convert_purpose_raw(UsdBridgePurposeRaw::Render),
            MeshPurpose::Render
        );
        assert_eq!(
            convert_purpose_raw(UsdBridgePurposeRaw::Proxy),
            MeshPurpose::Proxy
        );
        assert_eq!(
            convert_purpose_raw(UsdBridgePurposeRaw::Guide),
            MeshPurpose::Guide
        );
    }

    // ---- Layer-Aware Stage tests (v0.14.0) ----

    fn make_layer_info_raw(
        identifier: &CString,
        display: &CString,
        real_path: &CString,
        parent_index: i32,
        depth: u8,
        muted: bool,
        permission_to_edit: bool,
    ) -> UsdBridgeLayerInfoRaw {
        UsdBridgeLayerInfoRaw {
            identifier: identifier.as_ptr(),
            display_name: display.as_ptr(),
            real_path: real_path.as_ptr(),
            is_anonymous: 0,
            is_dirty: 0,
            is_muted: if muted { 1 } else { 0 },
            permission_to_edit: if permission_to_edit { 1 } else { 0 },
            time_offset: 0.0,
            time_scale: 1.0,
            parent_index,
            depth,
        }
    }

    #[test]
    fn test_convert_layer_info_root() {
        let ident = CString::new("root.usd").unwrap();
        let disp = CString::new("root.usd").unwrap();
        let real = CString::new("/abs/root.usd").unwrap();
        let raw = make_layer_info_raw(&ident, &disp, &real, -1, 0, false, true);
        let info = unsafe { convert_layer_info(&raw) };
        assert_eq!(info.identifier, "root.usd");
        assert_eq!(info.parent_index, None);
        assert_eq!(info.depth, 0);
        assert!(info.offset.is_identity());
        assert!(!info.is_muted);
        assert!(info.permission_to_edit);
    }

    #[test]
    fn test_convert_layer_info_sublayer_with_parent() {
        let ident = CString::new("anim.usd").unwrap();
        let disp = CString::new("anim.usd").unwrap();
        let real = CString::new("/abs/anim.usd").unwrap();
        let raw = make_layer_info_raw(&ident, &disp, &real, 0, 1, true, false);
        let info = unsafe { convert_layer_info(&raw) };
        assert_eq!(info.parent_index, Some(0));
        assert_eq!(info.depth, 1);
        assert!(info.is_muted);
        assert!(!info.permission_to_edit);
    }

    #[test]
    fn test_convert_layer_stack_null_returns_empty() {
        let stack = unsafe { convert_layer_stack_ptr(ptr::null()) };
        assert!(stack.layers.is_empty());
        assert_eq!(stack.root_index, 0);
    }

    #[test]
    fn test_convert_layer_stack_two_layers() {
        let root_id = CString::new("root.usd").unwrap();
        let root_disp = CString::new("root.usd").unwrap();
        let root_path = CString::new("/abs/root.usd").unwrap();
        let sub_id = CString::new("anim.usd").unwrap();
        let sub_disp = CString::new("anim.usd").unwrap();
        let sub_path = CString::new("/abs/anim.usd").unwrap();
        let mut layers = vec![
            make_layer_info_raw(&root_id, &root_disp, &root_path, -1, 0, false, true),
            make_layer_info_raw(&sub_id, &sub_disp, &sub_path, 0, 1, false, true),
        ];
        let raw = UsdBridgeLayerStackRaw {
            layers: layers.as_mut_ptr(),
            count: layers.len(),
            root_index: 0,
        };
        let stack = unsafe { convert_layer_stack_ptr(&raw) };
        assert_eq!(stack.layers.len(), 2);
        assert_eq!(stack.layers[0].identifier, "root.usd");
        assert_eq!(stack.layers[0].parent_index, None);
        assert_eq!(stack.layers[1].identifier, "anim.usd");
        assert_eq!(stack.layers[1].parent_index, Some(0));

        // LayerStack::children_of should find the sublayer
        let kids: Vec<_> = stack.children_of(0).collect();
        assert_eq!(kids.len(), 1);
        assert_eq!(kids[0].1.identifier, "anim.usd");
    }

    #[test]
    fn test_prim_specifier_from_u8_maps_correctly() {
        assert_eq!(PrimSpecifier::from_u8(0), PrimSpecifier::Def);
        assert_eq!(PrimSpecifier::from_u8(1), PrimSpecifier::Over);
        assert_eq!(PrimSpecifier::from_u8(2), PrimSpecifier::Class);
        // Unknown values fall back to Over (the most permissive).
        assert_eq!(PrimSpecifier::from_u8(99), PrimSpecifier::Over);
    }

    #[test]
    fn test_convert_prim_spec_all_specifiers() {
        let layer = CString::new("shot.usd").unwrap();
        let path = CString::new("/World/Hero").unwrap();
        for (raw_val, expected) in [
            (0u8, PrimSpecifier::Def),
            (1u8, PrimSpecifier::Over),
            (2u8, PrimSpecifier::Class),
        ] {
            let raw = UsdBridgePrimSpecRaw {
                layer_identifier: layer.as_ptr(),
                path: path.as_ptr(),
                specifier: raw_val,
                has_authored_opinions: 1,
            };
            let entry = unsafe { convert_prim_spec(&raw) };
            assert_eq!(entry.specifier, expected);
            assert_eq!(entry.layer_identifier, "shot.usd");
            assert!(entry.has_authored_opinions);
        }
    }

    #[test]
    fn test_convert_opinion_source_winning_flag() {
        let layer = CString::new("shot.usd").unwrap();
        let value = CString::new("(1.0, 2.0, 3.0)").unwrap();
        let vtype = CString::new("float3").unwrap();
        let raw = UsdBridgeOpinionSourceRaw {
            layer_identifier: layer.as_ptr(),
            value_display: value.as_ptr(),
            value_type_token: vtype.as_ptr(),
        };
        let winning = unsafe { convert_opinion_source(&raw, true) };
        assert!(winning.is_winning);
        assert_eq!(winning.value_display, "(1.0, 2.0, 3.0)");
        assert_eq!(winning.value_type, "float3");

        let weak = unsafe { convert_opinion_source(&raw, false) };
        assert!(!weak.is_winning);
    }

    #[test]
    fn test_convert_attribute_opinions_marks_winning() {
        let l1 = CString::new("shot.usd").unwrap();
        let l2 = CString::new("anim.usd").unwrap();
        let v1 = CString::new("5.0").unwrap();
        let v2 = CString::new("3.0").unwrap();
        let t = CString::new("float").unwrap();
        let mut sources = vec![
            UsdBridgeOpinionSourceRaw {
                layer_identifier: l1.as_ptr(),
                value_display: v1.as_ptr(),
                value_type_token: t.as_ptr(),
            },
            UsdBridgeOpinionSourceRaw {
                layer_identifier: l2.as_ptr(),
                value_display: v2.as_ptr(),
                value_type_token: t.as_ptr(),
            },
        ];
        let raw = UsdBridgeAttributeOpinionsRaw {
            sources: sources.as_mut_ptr(),
            count: sources.len(),
            winning_index: 0,
        };
        let out = unsafe { convert_attribute_opinions_ptr(&raw) };
        assert_eq!(out.len(), 2);
        assert!(out[0].is_winning);
        assert!(!out[1].is_winning);
        assert_eq!(out[0].layer_identifier, "shot.usd");
        assert_eq!(out[1].layer_identifier, "anim.usd");
    }

    #[test]
    fn test_convert_layer_offset_identity() {
        let raw = UsdBridgeLayerOffsetRaw {
            offset: 0.0,
            scale: 1.0,
        };
        let off = convert_layer_offset(&raw);
        assert!(off.is_identity());
    }

    #[test]
    fn test_convert_layer_offset_non_identity() {
        let raw = UsdBridgeLayerOffsetRaw {
            offset: 24.0,
            scale: 0.5,
        };
        let off = convert_layer_offset(&raw);
        assert!(!off.is_identity());
        assert_eq!(off.offset, 24.0);
        assert_eq!(off.scale, 0.5);
    }

    #[test]
    fn test_payload_policy_to_raw() {
        assert_eq!(
            payload_policy_to_raw(PayloadPolicy::LoadAll),
            UsdBridgePayloadPolicyRaw::LoadAll
        );
        assert_eq!(
            payload_policy_to_raw(PayloadPolicy::LoadNone),
            UsdBridgePayloadPolicyRaw::LoadNone
        );
    }

    #[test]
    fn test_convert_edit_target_with_identifier() {
        let id = CString::new("shot.usd").unwrap();
        let raw = UsdBridgeEditTargetRaw {
            layer_identifier: id.as_ptr(),
        };
        let target = unsafe { convert_edit_target(&raw) };
        assert_eq!(target.layer_identifier, "shot.usd");
    }

    #[test]
    fn test_convert_edit_target_empty() {
        let empty = CString::new("").unwrap();
        let raw = UsdBridgeEditTargetRaw {
            layer_identifier: empty.as_ptr(),
        };
        let target = unsafe { convert_edit_target(&raw) };
        assert_eq!(target.layer_identifier, "");
    }
}
