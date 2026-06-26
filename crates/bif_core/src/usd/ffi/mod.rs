//! USD C++ Bridge — all types and re-exports for the split submodule tree.

// ── Sub-module declarations ──────────────────────────────────────────────────
pub(super) mod instance;
pub(super) mod layer;
pub(super) mod material;
pub(super) mod mesh;
pub(super) mod prim;
pub(super) mod stage;
pub(super) mod variant;
pub(super) mod xform;

// ── Re-exported imports (sub-files use `use super::*;` to access these) ─────
pub(super) use std::ffi::{CStr, CString};
pub(super) use std::path::Path;
pub(super) use std::ptr;

pub(super) use bif_math::{Mat4, Vec3};

// Re-export the ffi_convert and ffi_guard modules themselves so sub-files can
// use `super::ffi_convert::some_fn(...)` paths.
pub(super) use super::ffi_convert;
pub(super) use super::ffi_guard;
// Also re-export the specific functions used via `use super::*` in stage.rs
pub(super) use super::ffi_convert::{
    convert_attribute_opinions_ptr, convert_edit_target, convert_layer_offset,
    convert_layer_stack_ptr, convert_prim_stack_ptr, payload_policy_to_raw,
};
pub(super) use super::ffi_raw::*;
pub(super) use super::layer::{
    EditTarget, LayerOffset, LayerStack, OpinionSource, PayloadPolicy, PrimStackEntry,
};

// ── Private imports (only needed in mod.rs) ──────────────────────────────────
use serde::{Deserialize, Serialize};
use thiserror::Error;

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

    #[error("Save failed: {0}")]
    SaveFailed(String),

    #[error("Path contains invalid UTF-8")]
    InvalidPath,

    #[error(
        "Refused {requested_bytes}-byte allocation ({context}) exceeds {max_bytes}-byte safety cap"
    )]
    AllocTooLarge {
        context: String,
        requested_bytes: usize,
        max_bytes: usize,
    },
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

pub(super) fn cstr(s: &str) -> UsdBridgeResult<CString> {
    CString::new(s).map_err(|_| UsdBridgeError::InvalidPath)
}

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

/// A USD relationship with name and resolved (composed) target paths.
#[derive(Clone, Debug)]
pub struct UsdRelationshipData {
    /// Relationship name (e.g., "material:binding", "proxyPrim")
    pub name: String,
    /// Resolved target prim/property paths (composed across layers)
    pub targets: Vec<String>,
    /// Whether the relationship has any authored targets on any layer
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

/// One shader input row from `UsdStage::get_bound_material_inputs`.
/// Used by the Material Sheet to populate per-input editors. C4b-1.
#[derive(Clone, Debug)]
pub struct BoundMaterialInput {
    pub name: String,
    pub type_name: String,
    pub value: String,
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
    pub(super) raw: *mut UsdBridgeStageRaw,
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

impl Drop for UsdStage {
    fn drop(&mut self) {
        if !self.raw.is_null() {
            unsafe {
                usd_bridge_close_stage(self.raw);
            }
        }
    }
}

/// Authored info for one UsdCollectionAPI instance applied to a prim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsdCollectionInfo {
    pub name: String,
    pub includes: Vec<String>,
    pub excludes: Vec<String>,
    /// "expandPrims" | "expandPrimsAndProperties" | "explicitOnly"
    pub expansion_rule: String,
    pub include_root: bool,
}

// ============================================================================
// Edit Layer (for exporting transform overrides)
// ============================================================================

/// A writable USD stage for exporting edit opinions.
pub struct UsdEditLayer {
    pub(super) raw: *mut UsdBridgeEditLayerRaw,
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
            .write_layer_visibility(&working_id, "/World/Cube", false, None)
            .expect("write visibility");
        stage.save_layer(&working_id).expect("save working");

        let reopened = UsdStage::open(&root).expect("reopen root");
        let text = reopened
            .export_layer_as_string(&working_id)
            .expect("export working");
        assert!(text.contains("invisible"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_layer_xform_preserves_translate_op_type() {
        let dir = temp_usda_path("translate_xform_dir");
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
        float3 xformOp:translate = (0, 0, 0)
        uniform token[] xformOpOrder = ["xformOp:translate"]
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
        let matrix = Mat4::from_translation(Vec3::new(4.0, 5.0, 6.0)).to_cols_array();

        stage
            .write_layer_xform(&working_id, "/World/Cube", -1.0, &matrix)
            .expect("write translate-backed xform");

        let value = stage
            .layer_get_attr_value(&working_id, "/World/Cube", "xformOp:translate")
            .expect("get authored translate")
            .expect("authored translate value");
        assert!(
            value.contains('4') && value.contains('5') && value.contains('6'),
            "translate op should receive vector value from matrix translation, got {value}"
        );
        assert!(
            stage
                .layer_get_attr_value(&working_id, "/World/Cube", "xformOp:transform")
                .expect("get authored transform")
                .is_none(),
            "translate-backed prim should not receive matrix transform op"
        );

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
    fn save_with_locked_layer_returns_message() {
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

        let err = stage
            .save_layer(&shot_id)
            .expect_err("locked layer save fails");

        stage
            .set_layer_permission_to_edit(&shot_id, true)
            .expect("restore shot permission");

        match err {
            UsdBridgeError::SaveFailed(message) => {
                assert!(
                    message.contains("not editable") && message.contains("shot.usda"),
                    "expected concrete save failure, got {message:?}"
                );
            }
            other => panic!("expected SaveFailed, got {other:?}"),
        }
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

    const RELS_FIXTURE: &str = "../../test_assets/relationships.usda";

    #[test]
    fn test_get_prim_relationships_lists_authored() {
        let stage = UsdStage::open(RELS_FIXTURE).expect("open relationships fixture");

        let rels = stage
            .get_prim_relationships("/World/Geo")
            .expect("relationships query");

        let binding = rels
            .iter()
            .find(|r| r.name == "material:binding")
            .expect("material:binding present on /World/Geo");
        assert!(binding.is_authored, "material:binding should be authored");
        assert_eq!(binding.targets, vec!["/World/Mat".to_string()]);

        let proxy = rels
            .iter()
            .find(|r| r.name == "proxyPrim")
            .expect("proxyPrim present on /World/Geo");
        assert_eq!(proxy.targets, vec!["/World/Proxy".to_string()]);
    }

    #[test]
    fn test_get_relationship_opinions_returns_single_layer() {
        let stage = UsdStage::open(RELS_FIXTURE).expect("open relationships fixture");

        let opinions = stage
            .get_relationship_opinions("/World/Geo", "material:binding")
            .expect("relationship opinions query");

        assert_eq!(opinions.len(), 1, "single authoring layer expected");
        assert!(opinions[0].is_winning);
        assert!(
            opinions[0].layer_identifier.ends_with("relationships.usda"),
            "opinion should come from relationships.usda, got {}",
            opinions[0].layer_identifier
        );
        assert!(
            opinions[0].value_display.contains("/World/Mat"),
            "value_display should list /World/Mat target, got {}",
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

    const COLLECTIONS_FIXTURE: &str = "../../test_assets/collections.usda";

    #[test]
    fn test_list_collections_returns_applied_names() {
        let stage = UsdStage::open(COLLECTIONS_FIXTURE).expect("open collections fixture");
        let mut names = stage.list_collections("/World").expect("list collections");
        names.sort();
        assert_eq!(names, vec!["lights".to_string(), "skinMeshes".to_string()]);
    }

    #[test]
    fn test_get_collection_info_reads_authored_targets() {
        let stage = UsdStage::open(COLLECTIONS_FIXTURE).expect("open collections fixture");
        let info = stage
            .get_collection_info("/World", "skinMeshes")
            .expect("collection info");
        assert_eq!(info.name, "skinMeshes");
        assert_eq!(info.includes, vec!["/World/Hero/Body".to_string()]);
        assert_eq!(info.excludes, vec!["/World/Hero/Body/Eyes".to_string()]);
        assert_eq!(info.expansion_rule, "expandPrims");
    }

    #[test]
    fn test_compute_collection_members_expands_includes() {
        let stage = UsdStage::open(COLLECTIONS_FIXTURE).expect("open collections fixture");
        let members = stage
            .compute_collection_members("/World", "skinMeshes")
            .expect("compute members");
        // /World/Hero/Body is included, but /World/Hero/Body/Eyes is excluded
        assert!(members.contains(&"/World/Hero/Body".to_string()));
        assert!(!members.contains(&"/World/Hero/Body/Eyes".to_string()));
    }

    #[test]
    fn test_collection_add_and_remove_target_roundtrip() {
        let stage = UsdStage::open(COLLECTIONS_FIXTURE).expect("open collections fixture");
        stage
            .collection_add_target("/World", "skinMeshes", "/World/Hero", true)
            .expect("add include");
        let info = stage.get_collection_info("/World", "skinMeshes").unwrap();
        assert!(info.includes.contains(&"/World/Hero".to_string()));

        stage
            .collection_remove_target("/World", "skinMeshes", "/World/Hero", true)
            .expect("remove include");
        let info = stage.get_collection_info("/World", "skinMeshes").unwrap();
        assert!(!info.includes.contains(&"/World/Hero".to_string()));
    }

    #[test]
    fn test_collection_set_expansion_rule() {
        let stage = UsdStage::open(COLLECTIONS_FIXTURE).expect("open collections fixture");
        stage
            .collection_set_expansion_rule("/World", "skinMeshes", "explicitOnly")
            .expect("set rule");
        let info = stage.get_collection_info("/World", "skinMeshes").unwrap();
        assert_eq!(info.expansion_rule, "explicitOnly");
    }

    #[test]
    fn test_apply_collection_creates_new() {
        let stage = UsdStage::open(COLLECTIONS_FIXTURE).expect("open collections fixture");
        stage
            .apply_collection("/World", "extras")
            .expect("apply new collection");
        let names = stage.list_collections("/World").unwrap();
        assert!(names.contains(&"extras".to_string()));
    }
}
