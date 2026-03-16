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
}

/// Prim info from C API (for scene browser)
#[repr(C)]
struct UsdBridgePrimInfoRaw {
    path: *const std::ffi::c_char,
    type_name: *const std::ffi::c_char,
    is_active: i32,
    has_children: i32,
    child_count: usize,
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
}

/// Light type enumeration from C API
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsdBridgeLightType {
    Distant = 0,
    Sphere = 1,
    Rect = 2,
    Dome = 3,
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
}

impl From<UsdBridgeLightType> for UsdLightType {
    fn from(t: UsdBridgeLightType) -> Self {
        match t {
            UsdBridgeLightType::Distant => UsdLightType::Distant,
            UsdBridgeLightType::Sphere => UsdLightType::Sphere,
            UsdBridgeLightType::Rect => UsdLightType::Rect,
            UsdBridgeLightType::Dome => UsdLightType::Dome,
        }
    }
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
}

impl Default for UsdStageMetadata {
    fn default() -> Self {
        Self {
            meters_per_unit: 1.0,
            up_axis: UpAxis::Y,
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
        write!(f, "{}, {}", self.up_axis, unit)
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

        Ok(UsdInstancerData {
            path,
            prototype_paths,
            transforms,
            proto_indices,
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
