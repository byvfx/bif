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
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::ffi_convert::{
    convert_attribute_opinions_ptr, convert_edit_target, convert_layer_offset,
    convert_layer_stack_ptr, convert_prim_stack_ptr, payload_policy_to_raw,
};
use super::ffi_raw::*;
use super::layer::{
    EditTarget, LayerOffset, LayerStack, OpinionSource, PayloadPolicy, PrimStackEntry,
};

// Raw FFI types and extern "C" block are in ffi_raw.rs

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
            UsdBridgeErrorCode::Success => {
                UsdBridgeError::Unknown("unexpected Success code".into())
            }
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

    /// Original vertex positions before UV seam splitting (for subdivision surfaces)
    pub vertices_orig: Option<Vec<Vec3>>,

    /// FaceVarying UV values for subdivision surfaces (pre-split, for Embree topology)
    pub facevarying_uvs: Option<Vec<[f32; 2]>>,

    /// FaceVarying UV indices for subdivision surfaces (per face-vertex)
    pub facevarying_uv_indices: Option<Vec<i32>>,
}

/// A USD prim attribute with name, type, and value.
#[derive(Clone, Debug)]
pub struct UsdAttributeData {
    /// Attribute name (e.g., "points", "primvars:st")
    pub name: String,
    /// USD type name (e.g., "point3f[]", "token", "float")
    pub type_name: String,
    /// Value as display string (scalars: actual value, arrays: "[count]")
    pub value: String,
    /// Whether this is a primvar (has interpolation)
    pub is_primvar: bool,
    /// Primvar interpolation ("constant", "uniform", "vertex", "faceVarying") or empty
    pub interpolation: String,
    /// Whether the attribute has an authored value
    pub is_authored: bool,
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
    /// Purpose (from scene hierarchy)
    pub purpose: MeshPurpose,
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

/// Material data extracted from USD, using OpenPBR naming.
#[derive(Clone, Debug)]
pub struct UsdMaterialData {
    /// Material prim path (e.g., "/World/Looks/Material_0")
    pub path: String,

    /// Base color / albedo (RGB, 0-1)
    pub base_color: Vec3,

    /// Metalness (0=dielectric, 1=metal)
    pub base_metalness: f32,

    /// Specular roughness (0=smooth, 1=rough)
    pub specular_roughness: f32,

    /// Specular weight (scales dielectric reflection)
    pub specular_weight: f32,

    /// Specular IOR (default 1.5)
    pub specular_ior: f32,

    /// Transmission weight (0=opaque, 1=fully transmissive glass)
    pub transmission_weight: f32,

    /// Geometry opacity (0=transparent, 1=opaque)
    pub geometry_opacity: f32,

    /// Emission color (RGB)
    pub emission_color: Vec3,

    /// Path to base color texture (if any)
    pub base_color_texture: Option<String>,

    /// Path to specular roughness texture (if any)
    pub specular_roughness_texture: Option<String>,

    /// Path to base metalness texture (if any)
    pub base_metalness_texture: Option<String>,

    /// Path to normal map texture (if any)
    pub normal_texture: Option<String>,

    /// Path to emission texture (if any)
    pub emission_texture: Option<String>,

    /// Path to geometry opacity texture (if any)
    pub geometry_opacity_texture: Option<String>,

    /// Path to displacement texture (if any)
    pub displacement_texture: Option<String>,

    /// Displacement scale factor
    pub displacement_scale: f32,

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
    /// Joint indices. When `is_rigid == false`, holds `vert_count * element_size`
    /// per-vertex influences. When `is_rigid == true`, holds a single block
    /// of `element_size` influences that apply to every vertex.
    pub joint_indices: Vec<i32>,
    /// Joint weights, parallel to `joint_indices` (same layout rules).
    pub joint_weights: Vec<f32>,
    /// Number of influences per vertex (or per mesh, when rigid).
    pub element_size: usize,
    /// Geom bind transform
    pub geom_bind_transform: Mat4,
    /// SkelRoot ancestor's world xform — use as the instance transform for
    /// skinned meshes so skel-local skinned vertices land in the right
    /// place even when the mesh prim sits under a non-identity sub-Xform.
    pub skel_root_world_xform: Mat4,
    /// True when `UsdSkelSkinningQuery::IsRigidlyDeformed()` was true: the
    /// mesh is bound to one joint via constant interpolation. The Rust loader
    /// uses this to construct a compact `SkinKind::Rigid` variant instead of
    /// broadcasting the single influence block to per-vertex layout.
    pub is_rigid: bool,
}

/// A single blend shape target (dense-expanded position/normal offsets).
#[derive(Clone, Debug)]
pub struct UsdBlendShapeTarget {
    /// Shape name token (e.g., "blink_L", "jawOpen")
    pub name: String,
    /// Dense position offsets — one `Vec3` per vertex, zero for unaffected verts.
    pub offsets: Vec<Vec3>,
    /// Dense normal offsets, or `None` if the BlendShape prim had no `normalOffsets`.
    pub normal_offsets: Option<Vec<Vec3>>,
}

/// Per-mesh blend shape binding: all targets that drive this mesh.
#[derive(Clone, Debug)]
pub struct UsdBlendShapeBinding {
    /// Mesh prim path this binding belongs to.
    pub mesh_path: String,
    /// Blend shape targets in the mesh's `skel:blendShapes` token order.
    pub targets: Vec<UsdBlendShapeTarget>,
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
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

// SAFETY: UsdStage is Send because:
// 1. The raw pointer is exclusively owned — only one UsdStage wraps a given
//    UsdBridgeStageRaw at a time, and Drop releases it.
// 2. Moving the UsdStage to another thread transfers ownership cleanly.
//
// UsdStage is NOT Sync because:
// - set_variant_selection() and load/unload_payload() mutate C++ state
//   through &self via raw pointer casts. Concurrent &UsdStage access from
//   multiple threads could cause data races in the C++ layer.
// - Use Arc<Mutex<UsdStage>> for shared access across threads. The Mutex
//   serializes all access, preventing concurrent C++ mutations.
unsafe impl Send for UsdStage {}

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
            vertices_orig: ptr::null(),
            vertex_count_orig: 0,
            facevarying_uvs: ptr::null(),
            facevarying_uv_count: 0,
            facevarying_uv_indices: ptr::null(),
            facevarying_uv_index_count: 0,
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

        // SAFETY: raw_data populated by FFI call above; pointers valid while stage is open
        Ok(unsafe { super::ffi_convert::convert_mesh(&raw_data) })
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

        // SAFETY: raw_data populated by FFI call above; pointers valid while stage is open
        Ok(unsafe { super::ffi_convert::convert_instancer(&raw_data) })
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
            purpose: 0,
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

        // SAFETY: raw_data populated by FFI call above; pointers valid while stage is open
        Ok(unsafe { super::ffi_convert::convert_native_instance(&raw_data) })
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
            transmission: 0.0,
            specular_ior: 1.5,
            emissive_color: [0.0, 0.0, 0.0],
            diffuse_texture: ptr::null(),
            roughness_texture: ptr::null(),
            metallic_texture: ptr::null(),
            normal_texture: ptr::null(),
            emissive_texture: ptr::null(),
            opacity_texture: ptr::null(),
            displacement_texture: ptr::null(),
            displacement_scale: 1.0,
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

        // SAFETY: raw_data populated by FFI call above; pointers valid while stage is open
        Ok(unsafe { super::ffi_convert::convert_material(&raw_data) })
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

        // SAFETY: raw_data populated by FFI call above; pointers valid while stage is open
        Ok(unsafe { super::ffi_convert::convert_light(&raw_data) })
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

        // SAFETY: raw populated by FFI call above; pointers valid while stage is open
        Ok(unsafe { super::ffi_convert::convert_points(&raw) })
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

        // SAFETY: raw populated by FFI call above; pointers valid while stage is open
        Ok(unsafe { super::ffi_convert::convert_curves(&raw) })
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

        // SAFETY: raw populated by FFI call above; pointers valid while stage is open
        Ok(unsafe { super::ffi_convert::convert_skeleton(&raw) })
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
            skel_root_world_xform: [
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
            ],
            is_rigid: 0,
        };
        let result = unsafe { usd_bridge_get_skin_binding(self.raw, mesh_index, &mut raw) };
        if result != UsdBridgeErrorCode::Success {
            return Err(result.into());
        }

        // SAFETY: raw populated by FFI call above; pointers valid while stage is open
        Ok(unsafe { super::ffi_convert::convert_skin_binding(&raw) })
    }

    /// Compute joint-skel-space transforms at a specific USD time code.
    ///
    /// Returns one `Mat4` per joint in the skeleton, in the skeleton's
    /// joint order (matches `UsdSkeletonData::joint_paths`). Phase 3 feeds
    /// these to `bif_core::skinning::compute_skin_matrices` to build the
    /// palette for CPU LBS.
    ///
    /// Cheap when called repeatedly at different times — the underlying
    /// `UsdSkelSkeletonQuery` is cached inside the C++ bridge's `UsdSkelCache`,
    /// so only the per-time topology walk + matrix compose runs each call.
    pub fn compute_skel_xforms(
        &self,
        skel_index: usize,
        time_code: f64,
    ) -> UsdBridgeResult<Vec<Mat4>> {
        let skel = self.get_skeleton(skel_index)?;
        let joint_count = skel.joint_paths.len();
        if joint_count == 0 {
            return Ok(Vec::new());
        }

        let mut buf: Vec<f32> = vec![0.0; joint_count * 16];
        let result = unsafe {
            usd_bridge_compute_skel_skin_xforms(
                self.raw,
                skel_index,
                time_code,
                buf.as_mut_ptr(),
                buf.len(),
            )
        };
        if result != UsdBridgeErrorCode::Success {
            return Err(result.into());
        }

        let mut out = Vec::with_capacity(joint_count);
        for i in 0..joint_count {
            let mut arr = [0.0f32; 16];
            arr.copy_from_slice(&buf[i * 16..(i + 1) * 16]);
            out.push(Mat4::from_cols_array(&arr));
        }
        Ok(out)
    }

    // ========================================================================
    // Blend Shapes
    // ========================================================================

    /// Get the number of meshes with blend shape bindings.
    pub fn blend_shape_binding_count(&self) -> UsdBridgeResult<usize> {
        let mut count: usize = 0;
        let result = unsafe { usd_bridge_get_blend_shape_binding_count(self.raw, &mut count) };
        if result != UsdBridgeErrorCode::Success {
            return Err(result.into());
        }
        Ok(count)
    }

    /// Get blend shape binding data by index.
    pub fn get_blend_shape_binding(&self, index: usize) -> UsdBridgeResult<UsdBlendShapeBinding> {
        let mut raw = UsdBridgeBlendShapeBindingDataRaw {
            targets: ptr::null(),
            target_count: 0,
            mesh_prim_path: ptr::null(),
        };
        let result = unsafe { usd_bridge_get_blend_shape_binding(self.raw, index, &mut raw) };
        if result != UsdBridgeErrorCode::Success {
            return Err(result.into());
        }

        // SAFETY: raw populated by FFI call above; pointers valid while stage is open
        Ok(unsafe { super::ffi_convert::convert_blend_shape_binding(&raw) })
    }

    /// Compute blend shape weights at a specific USD time code.
    ///
    /// Returns one weight per target in the binding's target order (matches
    /// `UsdBlendShapeBinding::targets`). Shapes not driven by the animation
    /// get weight 0.
    pub fn compute_blend_shape_weights(
        &self,
        binding_index: usize,
        time_code: f64,
        target_count: usize,
    ) -> UsdBridgeResult<Vec<f32>> {
        if target_count == 0 {
            return Ok(Vec::new());
        }

        let mut buf: Vec<f32> = vec![0.0; target_count];
        let result = unsafe {
            usd_bridge_compute_blend_shape_weights(
                self.raw,
                binding_index,
                time_code,
                buf.as_mut_ptr(),
                buf.len(),
            )
        };
        if result != UsdBridgeErrorCode::Success {
            return Err(result.into());
        }
        Ok(buf)
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

        // SAFETY: raw populated by FFI call above; pointers valid while stage is open
        Ok(unsafe { super::ffi_convert::convert_volume(&raw) })
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

            // SAFETY: raw populated by FFI call above; pointers valid while stage is open
            primvars.push(unsafe { super::ffi_convert::convert_primvar(&raw) });
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
    ///
    /// # Safety note
    /// C++ returns pointers to thread-local strings — we copy immediately via
    /// `to_string_lossy().into_owned()`. Never store the raw pointer across calls.
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
        layer_identifier: &str,
    ) -> UsdBridgeResult<()> {
        let c_path = CString::new(prim_path).map_err(|_| UsdBridgeError::InvalidPath)?;
        let c_set = CString::new(variant_set).map_err(|_| UsdBridgeError::InvalidPath)?;
        let c_name = CString::new(variant_name).map_err(|_| UsdBridgeError::InvalidPath)?;
        let c_layer = CString::new(layer_identifier).map_err(|_| UsdBridgeError::InvalidPath)?;
        let code = unsafe {
            usd_bridge_set_variant_selection(
                self.raw as *mut _,
                c_path.as_ptr(),
                c_set.as_ptr(),
                c_name.as_ptr(),
                c_layer.as_ptr(),
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

        // SAFETY: no pointers in stage metadata raw struct
        Ok(super::ffi_convert::convert_stage_metadata(&raw))
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

        // SAFETY: no pointers in timeline raw struct
        Ok(super::ffi_convert::convert_timeline(&raw_data))
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

        // SAFETY: raw_data populated by FFI call above; pointers valid while stage is open
        Ok(unsafe { super::ffi_convert::convert_mesh_animation(&raw_data) })
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

        // SAFETY: raw_data populated by FFI call above; pointers valid while stage is open
        Ok(unsafe { super::ffi_convert::convert_instancer_animation(&raw_data) })
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

        // SAFETY: no pointers in camera properties raw struct
        Ok(super::ffi_convert::convert_camera_properties(&props))
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

    /// Return paths of all UsdGeomCamera prims in the stage.
    pub fn list_camera_prims(&self) -> UsdBridgeResult<Vec<String>> {
        Ok(self
            .all_prims()?
            .into_iter()
            .filter(|p| p.type_name == "Camera")
            .map(|p| p.path)
            .collect())
    }

    /// Helper to convert raw prim info to Rust type.
    fn convert_prim_info(raw: &UsdBridgePrimInfoRaw) -> UsdBridgeResult<UsdPrimInfo> {
        // SAFETY: raw populated by FFI call in caller; pointers valid while stage is open
        Ok(unsafe { super::ffi_convert::convert_prim_info(raw) })
    }

    /// Get all attributes for a prim by path.
    pub fn get_prim_attributes(&self, prim_path: &str) -> UsdBridgeResult<Vec<UsdAttributeData>> {
        let c_path = std::ffi::CString::new(prim_path)
            .map_err(|_| UsdBridgeError::InvalidPrim("invalid path".to_string()))?;

        let mut raw_ptr: *mut UsdBridgeAttributeDataRaw = std::ptr::null_mut();
        let mut count: usize = 0;

        let result = unsafe {
            usd_bridge_get_prim_attributes(self.raw, c_path.as_ptr(), &mut raw_ptr, &mut count)
        };

        if result != UsdBridgeErrorCode::Success {
            return Err(result.into());
        }

        if raw_ptr.is_null() || count == 0 {
            return Ok(Vec::new());
        }

        let attributes = unsafe {
            let raw_slice = std::slice::from_raw_parts(raw_ptr, count);
            let attrs: Vec<UsdAttributeData> = raw_slice
                .iter()
                .map(|raw| UsdAttributeData {
                    name: super::ffi_convert::c_str_to_string(raw.name),
                    type_name: super::ffi_convert::c_str_to_string(raw.type_name),
                    value: super::ffi_convert::c_str_to_string(raw.value_str),
                    is_primvar: raw.is_primvar != 0,
                    interpolation: super::ffi_convert::c_str_to_string(raw.interpolation),
                    is_authored: raw.is_authored != 0,
                })
                .collect();
            usd_bridge_free_prim_attributes(raw_ptr, count);
            attrs
        };

        Ok(attributes)
    }
}

impl UsdStage {
    /// Free bulk mesh geometry cache (normals, UVs, subdivision data) after Rust
    /// has copied it. Keeps vertices/indices/paths for animation queries.
    pub fn free_mesh_geometry_cache(&self) {
        if !self.raw.is_null() {
            unsafe { usd_bridge_free_mesh_geometry(self.raw) }
        }
    }

    /// Load all payloads and cache mesh/material/animation data.
    /// Stage opens with LoadNone (hierarchy only); call this to populate geometry.
    /// Returns prim count.
    pub fn load_payloads(&self) -> UsdBridgeResult<usize> {
        let mut prim_count: usize = 0;
        let result = unsafe { usd_bridge_load_payloads(self.raw, &mut prim_count) };
        if result != UsdBridgeErrorCode::Success {
            return Err(result.into());
        }
        Ok(prim_count)
    }
}

// ============================================================================
// Layer-Aware Stage (v0.14.0)
// ============================================================================
//
// Read-only inspection of the stage's layer stack, prim stacks, and
// per-attribute opinion sources, plus layer muting and payload-policy-aware
// stage opening. No write paths — editing lands in v0.16.

impl UsdStage {
    /// Open a stage with an explicit payload-loading policy.
    ///
    /// `LoadAll` opens and resolves every payload eagerly (default USD
    /// behavior). `LoadNone` opens hierarchy only; payloads load lazily via
    /// [`load_payload`](Self::load_payload) or [`load_payloads`](Self::load_payloads).
    pub fn open_with_policy<P: AsRef<Path>>(
        path: P,
        policy: PayloadPolicy,
    ) -> UsdBridgeResult<Self> {
        let abs_path = std::fs::canonicalize(path.as_ref())
            .map_err(|_| UsdBridgeError::FileNotFound(path.as_ref().display().to_string()))?;

        let path_str = abs_path.to_str().ok_or(UsdBridgeError::InvalidPath)?;

        // Mirror UsdStage::open's Windows extended-path handling.
        let path_str = if let Some(unc_path) = path_str.strip_prefix(r"\\?\UNC\") {
            format!(r"\\{}", unc_path)
        } else if let Some(local_path) = path_str.strip_prefix(r"\\?\") {
            local_path.to_string()
        } else {
            path_str.to_string()
        };

        let c_path = CString::new(path_str.as_str()).map_err(|_| UsdBridgeError::InvalidPath)?;

        let mut raw: *mut UsdBridgeStageRaw = ptr::null_mut();
        let code = unsafe {
            usd_bridge_open_stage_with_policy(
                c_path.as_ptr(),
                payload_policy_to_raw(policy),
                &mut raw,
            )
        };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        if raw.is_null() {
            return Err(UsdBridgeError::InvalidStage);
        }
        Ok(Self { raw })
    }

    /// Get the stage's sublayer stack (root + recursive sublayers).
    ///
    /// Read-only snapshot. If sublayers change (mute/unmute, reload), call
    /// this again to refresh.
    pub fn get_layer_stack(&self) -> UsdBridgeResult<LayerStack> {
        let mut raw_stack: *mut UsdBridgeLayerStackRaw = ptr::null_mut();
        let code = unsafe { usd_bridge_stage_get_layer_stack(self.raw, &mut raw_stack) };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        let stack = unsafe { convert_layer_stack_ptr(raw_stack) };
        unsafe { usd_bridge_layer_stack_free(raw_stack) };
        Ok(stack)
    }

    /// Get the current edit target — the layer where new opinions would be
    /// authored. Informational in v0.14.0 (read-only release).
    pub fn get_edit_target(&self) -> UsdBridgeResult<EditTarget> {
        let mut raw_target = UsdBridgeEditTargetRaw {
            layer_identifier: ptr::null(),
        };
        let code = unsafe { usd_bridge_stage_get_edit_target(self.raw, &mut raw_target) };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        let target = unsafe { convert_edit_target(&raw_target) };
        unsafe { usd_bridge_edit_target_free(&mut raw_target) };
        Ok(target)
    }

    /// Mute or unmute a layer by its authored identifier. Triggers stage
    /// recomposition — any cached prim data should be refreshed.
    pub fn set_layer_muted(&self, identifier: &str, muted: bool) -> UsdBridgeResult<()> {
        let c_id = CString::new(identifier).map_err(|_| UsdBridgeError::InvalidPath)?;
        let code = unsafe {
            usd_bridge_stage_mute_layer(
                self.raw as *mut _,
                c_id.as_ptr(),
                if muted { 1 } else { 0 },
            )
        };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(())
    }

    pub fn save_layer(&self, identifier: &str) -> UsdBridgeResult<()> {
        let c_id = CString::new(identifier).map_err(|_| UsdBridgeError::InvalidPath)?;
        let code = unsafe { usd_bridge_layer_save(self.raw, c_id.as_ptr()) };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(())
    }

    pub fn layer_permission_to_edit(&self, identifier: &str) -> UsdBridgeResult<bool> {
        let c_id = CString::new(identifier).map_err(|_| UsdBridgeError::InvalidPath)?;
        let mut can_edit = 0;
        let code =
            unsafe { usd_bridge_layer_permission_to_edit(self.raw, c_id.as_ptr(), &mut can_edit) };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(can_edit != 0)
    }

    pub fn export_layer_as_string(&self, identifier: &str) -> UsdBridgeResult<String> {
        let c_id = CString::new(identifier).map_err(|_| UsdBridgeError::InvalidPath)?;
        let mut text_ptr: *const std::ffi::c_char = ptr::null();
        let code =
            unsafe { usd_bridge_layer_export_as_string(self.raw, c_id.as_ptr(), &mut text_ptr) };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        if text_ptr.is_null() {
            return Ok(String::new());
        }
        Ok(unsafe { CStr::from_ptr(text_ptr).to_string_lossy().into_owned() })
    }

    pub fn import_layer_from_string(&self, identifier: &str, text: &str) -> UsdBridgeResult<()> {
        let c_id = CString::new(identifier).map_err(|_| UsdBridgeError::InvalidPath)?;
        let c_text = CString::new(text).map_err(|_| UsdBridgeError::InvalidPath)?;
        let code = unsafe {
            usd_bridge_layer_import_from_string(self.raw, c_id.as_ptr(), c_text.as_ptr())
        };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(())
    }

    pub fn parse_usda(text: &str) -> UsdBridgeResult<()> {
        let c_text = CString::new(text).map_err(|_| UsdBridgeError::InvalidPath)?;
        let code = unsafe { usd_bridge_parse_usda(c_text.as_ptr()) };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(())
    }

    pub fn layer_get_attr_value(
        &self,
        identifier: &str,
        prim_path: &str,
        attr_name: &str,
    ) -> UsdBridgeResult<Option<String>> {
        let c_id = CString::new(identifier).map_err(|_| UsdBridgeError::InvalidPath)?;
        let c_path = CString::new(prim_path).map_err(|_| UsdBridgeError::InvalidPath)?;
        let c_attr = CString::new(attr_name).map_err(|_| UsdBridgeError::InvalidPath)?;
        let mut value_ptr: *const std::ffi::c_char = ptr::null();
        let code = unsafe {
            usd_bridge_layer_get_attr_value(
                self.raw,
                c_id.as_ptr(),
                c_path.as_ptr(),
                c_attr.as_ptr(),
                &mut value_ptr,
            )
        };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        if value_ptr.is_null() {
            return Ok(None);
        }
        Ok(Some(unsafe {
            CStr::from_ptr(value_ptr).to_string_lossy().into_owned()
        }))
    }

    pub fn write_layer_xform(
        &self,
        identifier: &str,
        prim_path: &str,
        time: f64,
        matrix_16: &[f32; 16],
    ) -> UsdBridgeResult<()> {
        let c_id = CString::new(identifier).map_err(|_| UsdBridgeError::InvalidPath)?;
        let c_path = CString::new(prim_path).map_err(|_| UsdBridgeError::InvalidPath)?;
        let code = unsafe {
            usd_bridge_layer_write_xform(
                self.raw as *mut _,
                c_id.as_ptr(),
                c_path.as_ptr(),
                time,
                matrix_16.as_ptr(),
            )
        };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(())
    }

    pub fn write_layer_visibility(
        &self,
        identifier: &str,
        prim_path: &str,
        visible: bool,
    ) -> UsdBridgeResult<()> {
        let c_id = CString::new(identifier).map_err(|_| UsdBridgeError::InvalidPath)?;
        let c_path = CString::new(prim_path).map_err(|_| UsdBridgeError::InvalidPath)?;
        let code = unsafe {
            usd_bridge_layer_write_visibility(
                self.raw as *mut _,
                c_id.as_ptr(),
                c_path.as_ptr(),
                if visible { 1 } else { 0 },
            )
        };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(())
    }

    pub fn bind_layer_material(
        &self,
        identifier: &str,
        prim_path: &str,
        material_path: &str,
    ) -> UsdBridgeResult<()> {
        let c_id = CString::new(identifier).map_err(|_| UsdBridgeError::InvalidPath)?;
        let c_prim = CString::new(prim_path).map_err(|_| UsdBridgeError::InvalidPath)?;
        let c_mat = CString::new(material_path).map_err(|_| UsdBridgeError::InvalidPath)?;
        let code = unsafe {
            usd_bridge_layer_bind_material(
                self.raw as *mut _,
                c_id.as_ptr(),
                c_prim.as_ptr(),
                c_mat.as_ptr(),
            )
        };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(())
    }

    pub fn set_layer_shader_input(
        &self,
        identifier: &str,
        shader_path: &str,
        input_name: &str,
        value_type: &str,
        value: &str,
    ) -> UsdBridgeResult<()> {
        let c_id = CString::new(identifier).map_err(|_| UsdBridgeError::InvalidPath)?;
        let c_shader = CString::new(shader_path).map_err(|_| UsdBridgeError::InvalidPath)?;
        let c_input = CString::new(input_name).map_err(|_| UsdBridgeError::InvalidPath)?;
        let c_type = CString::new(value_type).map_err(|_| UsdBridgeError::InvalidPath)?;
        let c_value = CString::new(value).map_err(|_| UsdBridgeError::InvalidPath)?;
        let code = unsafe {
            usd_bridge_layer_set_shader_input(
                self.raw as *mut _,
                c_id.as_ptr(),
                c_shader.as_ptr(),
                c_input.as_ptr(),
                c_type.as_ptr(),
                c_value.as_ptr(),
            )
        };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(())
    }

    #[cfg(test)]
    pub fn set_layer_permission_to_edit(
        &self,
        identifier: &str,
        permission_to_edit: bool,
    ) -> UsdBridgeResult<()> {
        let c_id = CString::new(identifier).map_err(|_| UsdBridgeError::InvalidPath)?;
        let code = unsafe {
            usd_bridge_layer_set_permission_to_edit(
                self.raw,
                c_id.as_ptr(),
                if permission_to_edit { 1 } else { 0 },
            )
        };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(())
    }

    /// Get a layer's time offset + scale as authored on the root layer's
    /// sublayer reference list. Returns identity `(0.0, 1.0)` if the layer
    /// isn't a direct sublayer of the root.
    pub fn get_layer_offset(&self, identifier: &str) -> UsdBridgeResult<LayerOffset> {
        let c_id = CString::new(identifier).map_err(|_| UsdBridgeError::InvalidPath)?;
        let mut raw_offset = UsdBridgeLayerOffsetRaw {
            offset: 0.0,
            scale: 1.0,
        };
        let code = unsafe { usd_bridge_layer_get_offset(self.raw, c_id.as_ptr(), &mut raw_offset) };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(convert_layer_offset(&raw_offset))
    }

    /// Get the full prim stack — every layer that authors an opinion on the
    /// prim, ordered strongest-first. Returns empty `Vec` if the prim has no
    /// authored opinions (shouldn't happen for a composed prim).
    pub fn get_prim_stack(&self, prim_path: &str) -> UsdBridgeResult<Vec<PrimStackEntry>> {
        let c_path = CString::new(prim_path).map_err(|_| UsdBridgeError::InvalidPath)?;
        let mut raw_stack: *mut UsdBridgePrimStackRaw = ptr::null_mut();
        let code =
            unsafe { usd_bridge_prim_get_prim_stack(self.raw, c_path.as_ptr(), &mut raw_stack) };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        let entries = unsafe { convert_prim_stack_ptr(raw_stack) };
        unsafe { usd_bridge_prim_stack_free(raw_stack) };
        Ok(entries)
    }

    /// Get the opinion stack for a single attribute — one entry per layer
    /// contributing an opinion. The entry with `is_winning = true` is the
    /// value the composed stage sees.
    pub fn get_attribute_opinions(
        &self,
        prim_path: &str,
        attr_name: &str,
    ) -> UsdBridgeResult<Vec<OpinionSource>> {
        let c_path = CString::new(prim_path).map_err(|_| UsdBridgeError::InvalidPath)?;
        let c_attr = CString::new(attr_name).map_err(|_| UsdBridgeError::InvalidPath)?;
        let mut raw_opinions: *mut UsdBridgeAttributeOpinionsRaw = ptr::null_mut();
        let code = unsafe {
            usd_bridge_attr_get_opinion_sources(
                self.raw,
                c_path.as_ptr(),
                c_attr.as_ptr(),
                &mut raw_opinions,
            )
        };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        let sources = unsafe { convert_attribute_opinions_ptr(raw_opinions) };
        unsafe { usd_bridge_opinions_free(raw_opinions) };
        Ok(sources)
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
            material.base_color.x,
            material.base_color.y,
            material.base_color.z,
        ];
        let emissive = [
            material.emission_color.x,
            material.emission_color.y,
            material.emission_color.z,
        ];

        let diffuse_tex = material
            .base_color_texture
            .as_deref()
            .and_then(|s| CString::new(s.as_bytes()).ok());
        let roughness_tex = material
            .specular_roughness_texture
            .as_deref()
            .and_then(|s| CString::new(s.as_bytes()).ok());
        let metallic_tex = material
            .base_metalness_texture
            .as_deref()
            .and_then(|s| CString::new(s.as_bytes()).ok());
        let normal_tex = material
            .normal_texture
            .as_deref()
            .and_then(|s| CString::new(s.as_bytes()).ok());
        let emissive_tex = material
            .emission_texture
            .as_deref()
            .and_then(|s| CString::new(s.as_bytes()).ok());

        let code = unsafe {
            usd_bridge_write_material(
                self.raw,
                c_path.as_ptr(),
                diffuse.as_ptr(),
                material.base_metalness,
                material.specular_roughness,
                material.specular_weight,
                material.geometry_opacity,
                emissive.as_ptr(),
                diffuse_tex.as_ref().map_or(ptr::null(), |s| s.as_ptr()),
                roughness_tex.as_ref().map_or(ptr::null(), |s| s.as_ptr()),
                metallic_tex.as_ref().map_or(ptr::null(), |s| s.as_ptr()),
                normal_tex.as_ref().map_or(ptr::null(), |s| s.as_ptr()),
                emissive_tex.as_ref().map_or(ptr::null(), |s| s.as_ptr()),
                material.specular_ior,
                material.transmission_weight,
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

    /// Write a GeomSubset child prim for per-face material assignment.
    pub fn write_geom_subset(
        &mut self,
        mesh_path: &str,
        subset_name: &str,
        face_indices: &[i32],
        material_path: Option<&str>,
    ) -> UsdBridgeResult<()> {
        let c_mesh = CString::new(mesh_path).map_err(|_| UsdBridgeError::InvalidPath)?;
        let c_name = CString::new(subset_name).map_err(|_| UsdBridgeError::InvalidPath)?;
        let c_mat = material_path.and_then(|s| CString::new(s).ok());
        let code = unsafe {
            usd_bridge_write_geom_subset(
                self.raw,
                c_mesh.as_ptr(),
                c_name.as_ptr(),
                face_indices.as_ptr(),
                face_indices.len(),
                c_mat.as_ref().map_or(ptr::null(), |s| s.as_ptr()),
            )
        };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(())
    }

    /// Write invisibleIds attribute on a PointInstancer prim.
    pub fn write_invisible_ids(
        &mut self,
        instancer_path: &str,
        ids: &[i64],
    ) -> UsdBridgeResult<()> {
        if ids.is_empty() {
            return Ok(());
        }
        let c_path = CString::new(instancer_path).map_err(|_| UsdBridgeError::InvalidPath)?;
        let code = unsafe {
            usd_bridge_write_invisible_ids(self.raw, c_path.as_ptr(), ids.as_ptr(), ids.len())
        };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        Ok(())
    }

    /// Save the edit layer to disk. Drop frees the C++ handle.
    pub fn save(self) -> UsdBridgeResult<()> {
        let code = unsafe { usd_bridge_save_edit_layer(self.raw) };
        if code != UsdBridgeErrorCode::Success {
            return Err(code.into());
        }
        // Drop runs after this, calling usd_bridge_free_edit_layer
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
            horizontal_aperture: 36.0,
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
            horizontal_aperture: 36.0,
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

    fn temp_usda_path(prefix: &str) -> std::path::PathBuf {
        let id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "bif_bridge_{prefix}_{}_{}.usda",
            std::process::id(),
            id
        ))
    }

    #[test]
    fn parse_usda_rejects_garbage() {
        assert!(UsdStage::parse_usda("not valid usda").is_err());
    }

    #[test]
    fn parse_usda_accepts_minimal() {
        UsdStage::parse_usda(
            r#"#usda 1.0

def Xform "World"
{
}

"#,
        )
        .expect("parse minimal usda");
    }

    #[test]
    fn direct_ffi_save_roundtrip_persists_working_layer() {
        let dir = temp_usda_path("roundtrip_dir");
        let dir = dir.with_extension("");
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let root = dir.join("root.usda");
        let working = dir.join("working.usda");
        let asset = dir.join("asset.usda");
        std::fs::write(&working, "#usda 1.0\n\n").expect("write working");
        std::fs::write(
            &asset,
            r#"#usda 1.0

def Xform "World"
{
    def Xform "Cube"
    {
    }
}

"#,
        )
        .expect("write asset");
        std::fs::write(
            &root,
            r#"#usda 1.0
(
    subLayers = [
        @working.usda@,
        @asset.usda@
    ]
)

"#,
        )
        .expect("write root");

        let stage = UsdStage::open(&root).expect("open root");
        let working_id = stage
            .get_layer_stack()
            .expect("layer stack")
            .layers
            .into_iter()
            .find(|l| l.identifier.ends_with("working.usda"))
            .map(|l| l.identifier)
            .expect("working layer");
        stage
            .write_layer_visibility(&working_id, "/World/Cube", false)
            .expect("write visibility");
        stage.save_layer(&working_id).expect("save working");

        let reopened = UsdStage::open(&root).expect("reopen root");
        let text = reopened
            .export_layer_as_string(&working_id)
            .expect("export working");
        assert!(text.contains("invisible"));

        let _ = std::fs::remove_dir_all(&dir);
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
        stage.load_payloads().expect("load_payloads failed");

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
        stage.load_payloads().expect("load_payloads failed");

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

    /// v0.13.5 Phase 0: load skinned fixture via UsdSkelCache path.
    /// two_bone_arm.usda defines a 2-joint skeleton and an 8-vertex box with
    /// per-vertex joint weights (bottom 4 to joint 0, top 4 to joint 1).
    #[test]
    fn test_load_two_bone_arm_skel() {
        let path = "../../test_assets/skel/two_bone_arm.usda";
        let stage = match UsdStage::open(path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Skipping test - could not open stage: {e}");
                return;
            }
        };

        // One skeleton
        let skel_count = stage.skeleton_count().expect("skeleton_count failed");
        assert_eq!(skel_count, 1, "expected 1 skeleton, got {skel_count}");

        let skel = stage.get_skeleton(0).expect("get_skeleton(0) failed");
        assert_eq!(skel.path, "/Root/Character/Skel");
        assert_eq!(
            skel.joint_paths,
            vec!["Root".to_string(), "Root/Bend".to_string()]
        );
        assert_eq!(skel.bind_transforms.len(), 2);
        assert_eq!(skel.rest_transforms.len(), 2);

        // Joint 0 bind = identity (world-space)
        let id = Mat4::IDENTITY;
        let b0 = skel.bind_transforms[0];
        assert!(
            (b0.to_cols_array()[0] - id.to_cols_array()[0]).abs() < 1e-5,
            "joint 0 bind should be identity"
        );
        // Joint 1 bind = translate(0, 1, 0). Row-major translation column = index 13
        // in column-major layout (translation lives in .w_axis components).
        let b1_translation = skel.bind_transforms[1].w_axis;
        assert!(
            (b1_translation.y - 1.0).abs() < 1e-5,
            "joint 1 bind translation.y should be 1.0, got {b1_translation:?}"
        );

        // One mesh, with skin binding
        let mesh_count = stage.mesh_count().expect("mesh_count failed");
        assert_eq!(mesh_count, 1, "expected 1 mesh, got {mesh_count}");

        let skin = stage
            .get_skin_binding(0)
            .expect("get_skin_binding(0) failed");
        assert_eq!(skin.mesh_path, "/Root/Character/Box");
        assert_eq!(skin.skeleton_path, "/Root/Character/Skel");
        assert_eq!(skin.element_size, 1, "1 influence per vertex");
        assert_eq!(skin.joint_indices.len(), 8, "8 vertices * 1 influence");
        assert_eq!(skin.joint_weights.len(), 8);
        // Bottom 4 verts bound to joint 0
        assert_eq!(&skin.joint_indices[..4], &[0, 0, 0, 0]);
        // Top 4 verts bound to joint 1
        assert_eq!(&skin.joint_indices[4..], &[1, 1, 1, 1]);
        // All weights 1.0
        for w in &skin.joint_weights {
            assert!((w - 1.0).abs() < 1e-5);
        }
    }

    /// v0.13.5 Phase 3: evaluate skel xforms at a time code via UsdSkelSkeletonQuery.
    /// Fixture has no authored animation — eval at any time should return bind pose.
    #[test]
    fn test_compute_skel_xforms_at_time() {
        let path = "../../test_assets/skel/two_bone_arm.usda";
        let stage = match UsdStage::open(path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Skipping test - could not open stage: {e}");
                return;
            }
        };

        // At default time
        let xforms = stage
            .compute_skel_xforms(0, 0.0)
            .expect("compute_skel_xforms failed");
        assert_eq!(xforms.len(), 2, "expected 2 joint xforms");

        // Joint 0 (Root) = identity
        let j0 = xforms[0];
        assert!(
            (j0 - Mat4::IDENTITY)
                .to_cols_array()
                .iter()
                .all(|&x| x.abs() < 1e-5),
            "joint 0 should be identity at default time"
        );
        // Joint 1 (Bend) = translate(0, 1, 0)
        assert!(
            (xforms[1].w_axis.y - 1.0).abs() < 1e-5,
            "joint 1 y-translation should be 1.0"
        );

        // Eval at a different time (no animation → same result)
        let xforms_t = stage
            .compute_skel_xforms(0, 42.0)
            .expect("compute_skel_xforms at t=42 failed");
        for (a, b) in xforms.iter().zip(xforms_t.iter()) {
            assert!(
                (*a - *b).to_cols_array().iter().all(|&x| x.abs() < 1e-5),
                "static fixture must return identical xforms at all times"
            );
        }

        // Full round-trip: compute_skel_xforms → compute_skin_matrices → skin_positions
        // Identity palette → vertices unchanged (Phase 2 invariant via Phase 3 path).
        let skin_data = stage.get_skin_binding(0).expect("get_skin_binding failed");
        let skel_data = stage.get_skeleton(0).expect("get_skeleton failed");
        let inv_binds: Vec<Mat4> = skel_data
            .bind_transforms
            .iter()
            .map(|m| m.inverse())
            .collect();
        let binding = crate::mesh::SkinBinding {
            skeleton_path: skin_data.skeleton_path,
            kind: crate::mesh::SkinKind::PerVertex {
                joint_indices: skin_data
                    .joint_indices
                    .iter()
                    .map(|&i| if i < 0 { u32::MAX } else { i as u32 })
                    .collect(),
                joint_weights: skin_data.joint_weights,
                element_size: skin_data.element_size,
            },
            geom_bind_transform: skin_data.geom_bind_transform,
            inv_bind_matrices: inv_binds,
        };
        let palette = crate::skinning::compute_skin_matrices(&binding, &xforms);
        let bind_positions = vec![
            bif_math::Vec3::new(-0.5, 0.0, -0.5),
            bif_math::Vec3::new(0.5, 0.0, -0.5),
            bif_math::Vec3::new(0.5, 0.0, 0.5),
            bif_math::Vec3::new(-0.5, 0.0, 0.5),
            bif_math::Vec3::new(-0.5, 2.0, -0.5),
            bif_math::Vec3::new(0.5, 2.0, -0.5),
            bif_math::Vec3::new(0.5, 2.0, 0.5),
            bif_math::Vec3::new(-0.5, 2.0, 0.5),
        ];
        let mut out = vec![bif_math::Vec3::ZERO; 8];
        crate::skinning::skin_positions(&binding, &bind_positions, &palette, &mut out);
        for (got, want) in out.iter().zip(bind_positions.iter()) {
            assert!(
                (*got - *want).length() < 1e-5,
                "round-trip failed: expected {want:?}, got {got:?}"
            );
        }
    }

    /// v0.13.5 Phase 3: smoke-test time-varying animation eval against a real
    /// UsdSkel character with authored joint animation (Pixar's HumanFemale).
    /// Skipped silently when the asset is not present.
    #[test]
    fn test_compute_skel_xforms_animated_character() {
        let path = "../../assets/UsdSkelExamples/HumanFemale/HumanFemale.walk.usd";
        let stage = match UsdStage::open(path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Skipping test - HumanFemale asset unavailable: {e}");
                return;
            }
        };

        let skel_count = match stage.skeleton_count() {
            Ok(n) => n,
            Err(e) => {
                eprintln!("Skipping - skeleton_count failed: {e}");
                return;
            }
        };
        if skel_count == 0 {
            eprintln!("Skipping - no skeletons found in HumanFemale.walk.usd");
            return;
        }

        // Query within the authored time range. USD clamps out-of-range queries
        // to the nearest keyframe, so picking 0.0 and 20.0 (outside HumanFemale's
        // 101-129 range) would return identical results. Use the stage timeline.
        let timeline = stage.get_timeline().expect("get_timeline failed");
        let start = timeline.start_time_code;
        let end = timeline.end_time_code;
        assert!(
            end > start,
            "expected authored time range, got start={start} end={end}"
        );
        let mid = start + (end - start) * 0.5;

        let x_start = stage
            .compute_skel_xforms(0, start)
            .expect("eval @ start failed");
        let x_mid = stage
            .compute_skel_xforms(0, mid)
            .expect("eval @ mid failed");
        assert_eq!(
            x_start.len(),
            x_mid.len(),
            "joint count must match across time"
        );
        assert!(x_start.len() > 1, "HumanFemale should have >1 joint");

        let mut max_delta: f32 = 0.0;
        for (a, b) in x_start.iter().zip(x_mid.iter()) {
            let delta: f32 = (*a - *b)
                .to_cols_array()
                .iter()
                .map(|x| x.abs())
                .fold(0.0f32, f32::max);
            if delta > max_delta {
                max_delta = delta;
            }
        }
        assert!(
            max_delta > 1e-3,
            "expected joint motion between frame {start} and {mid}; max delta {max_delta}"
        );
        eprintln!("HumanFemale joint animation max delta over [{start}, {mid}]: {max_delta}");
    }

    // ------------------------------------------------------------------
    // v0.14.0 Layer-aware stage integration
    // ------------------------------------------------------------------

    /// Path to the multi-layer fixture. `test_assets/layers/root.usda`
    /// pulls in shot.usda + anim.usda as sublayers; `/Hero.xformOp:translate`
    /// carries a three-layer opinion stack (root → shot → anim).
    /// Only shot + anim author opinions on `/Hero/Geo` — root is
    /// opinion-free so muting shot reveals anim's values visibly in
    /// the viewport (USD forbids muting the root layer itself).
    const LAYERS_ROOT_FIXTURE: &str = "../../test_assets/layers/root.usda";
    const LAYERS_DEMO_ATTR_PATH: &str = "/Hero/Geo";
    const LAYERS_DEMO_ATTR_NAME: &str = "xformOp:translate";

    #[test]
    fn test_get_layer_stack_on_multilayer_fixture() {
        let stage = UsdStage::open(LAYERS_ROOT_FIXTURE).expect("open fixture");
        let stack = stage.get_layer_stack().expect("layer stack");

        assert!(
            stack.layers.len() >= 3,
            "expected at least 3 layers (root + 2 sublayers), got {}",
            stack.layers.len()
        );
        assert_eq!(stack.root_index, 0, "root layer should be first entry");
        assert_eq!(stack.layers[0].depth, 0, "root layer must have depth 0");
        assert!(
            stack.layers[1..]
                .iter()
                .all(|l| l.depth >= 1 && l.parent_index.is_some()),
            "sublayers must have depth >= 1 and a parent_index"
        );
        assert!(
            stack.layers.iter().all(|l| !l.is_muted),
            "no layer should be muted at load time"
        );
    }

    #[test]
    fn test_get_layer_stack_reports_permission_to_edit() {
        let stage = UsdStage::open(LAYERS_ROOT_FIXTURE).expect("open fixture");
        let shot_id = stage
            .get_layer_stack()
            .expect("layer stack")
            .layers
            .iter()
            .find(|l| l.identifier.ends_with("shot.usda"))
            .map(|l| l.identifier.clone())
            .expect("shot.usda in stack");

        stage
            .set_layer_permission_to_edit(&shot_id, false)
            .expect("disable shot permission");

        let stack = stage
            .get_layer_stack()
            .expect("layer stack after permission edit");

        let shot = stack
            .layers
            .iter()
            .find(|l| l.identifier.ends_with("shot.usda"))
            .expect("shot.usda in stack");
        let anim = stack
            .layers
            .iter()
            .find(|l| l.identifier.ends_with("anim.usda"))
            .expect("anim.usda in stack");

        assert!(
            !shot.permission_to_edit,
            "read-only shot.usda should report permission_to_edit=false"
        );
        assert!(
            anim.permission_to_edit,
            "writable anim.usda should remain editable"
        );

        stage
            .set_layer_permission_to_edit(&shot_id, true)
            .expect("restore shot permission");
    }

    #[test]
    fn test_attribute_opinions_winning_layer() {
        let stage = UsdStage::open(LAYERS_ROOT_FIXTURE).expect("open fixture");

        let opinions = stage
            .get_attribute_opinions(LAYERS_DEMO_ATTR_PATH, LAYERS_DEMO_ATTR_NAME)
            .expect("opinions query");

        // Root authors no prim opinions; translate has two contributing
        // layers (shot wins, anim provides the fallback).
        assert_eq!(
            opinions.len(),
            2,
            "expected 2 opinions on {LAYERS_DEMO_ATTR_PATH}.{LAYERS_DEMO_ATTR_NAME}, got {}",
            opinions.len()
        );
        assert!(opinions[0].is_winning, "first entry must be winning");
        assert!(!opinions[1].is_winning, "second entry must not be winning");
        assert!(
            opinions[0].layer_identifier.ends_with("shot.usda"),
            "shot.usda should author the strongest opinion, got {}",
            opinions[0].layer_identifier
        );
        assert!(
            opinions[1].layer_identifier.ends_with("anim.usda"),
            "anim.usda should author the weakest opinion, got {}",
            opinions[1].layer_identifier
        );
        // TfStringify on GfVec3f gives "(3, 0, 0)" — only the dominant
        // component is stable across USD versions.
        assert!(
            opinions[0].value_display.contains('3'),
            "winning value should reflect shot's (3, 0, 0), got {}",
            opinions[0].value_display
        );
    }

    #[test]
    fn test_mute_layer_recomposes_opinions() {
        let stage = UsdStage::open(LAYERS_ROOT_FIXTURE).expect("open fixture");

        // USD forbids muting the root layer. Mute the strong sublayer
        // `shot.usda` — the remaining opinion stack shrinks to a single
        // entry from anim.usda, which is exactly the visible effect we
        // want in the viewport (cube snaps back to origin + turns white).
        let stack = stage.get_layer_stack().expect("layer stack");
        let shot_id = stack
            .layers
            .iter()
            .find(|l| l.identifier.ends_with("shot.usda"))
            .map(|l| l.identifier.clone())
            .expect("shot.usda in stack");

        stage
            .set_layer_muted(&shot_id, true)
            .expect("mute shot succeeds");

        let opinions_after = stage
            .get_attribute_opinions(LAYERS_DEMO_ATTR_PATH, LAYERS_DEMO_ATTR_NAME)
            .expect("opinions query after mute");
        assert_eq!(
            opinions_after.len(),
            1,
            "expected 1 opinion after muting shot.usda, got {}",
            opinions_after.len()
        );
        assert!(
            opinions_after[0].layer_identifier.ends_with("anim.usda"),
            "anim.usda should be the sole remaining opinion, got {}",
            opinions_after[0].layer_identifier
        );

        // Unmute → back to 2 opinions with shot winning.
        stage
            .set_layer_muted(&shot_id, false)
            .expect("unmute shot succeeds");
        let opinions_restored = stage
            .get_attribute_opinions(LAYERS_DEMO_ATTR_PATH, LAYERS_DEMO_ATTR_NAME)
            .expect("opinions query after unmute");
        assert_eq!(
            opinions_restored.len(),
            2,
            "expected 2 opinions after unmuting shot, got {}",
            opinions_restored.len()
        );
        assert!(
            opinions_restored[0].layer_identifier.ends_with("shot.usda"),
            "shot.usda should win again after unmute, got {}",
            opinions_restored[0].layer_identifier
        );
    }

    #[test]
    fn test_prim_stack_lists_all_layer_opinions() {
        let stage = UsdStage::open(LAYERS_ROOT_FIXTURE).expect("open fixture");
        // /Hero/Geo is authored by anim (as a `def Cube`) and overridden
        // by shot. root has no opinions on it — two specs total.
        let prim_stack = stage
            .get_prim_stack(LAYERS_DEMO_ATTR_PATH)
            .expect("prim stack query");

        assert_eq!(
            prim_stack.len(),
            2,
            "expected 2 prim specs for {LAYERS_DEMO_ATTR_PATH}, got {}",
            prim_stack.len()
        );
        // Strongest → weakest: shot (over), anim (def Cube).
        assert!(prim_stack[0].layer_identifier.ends_with("shot.usda"));
        assert!(prim_stack[1].layer_identifier.ends_with("anim.usda"));
        // Only anim.usda carries the `def` specifier.
        assert_eq!(
            prim_stack[1].specifier,
            crate::usd::layer::PrimSpecifier::Def,
            "anim.usda defines {LAYERS_DEMO_ATTR_PATH} as Cube; specifier must be Def"
        );
    }
}
