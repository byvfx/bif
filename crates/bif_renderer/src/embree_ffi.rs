//! Shared Embree 4 FFI declarations.
//!
//! Single source of truth for Embree types and extern functions.
//! Used by both `embree.rs` (Ivar raytracer) and `pick_scene.rs` (viewport picking).

#![allow(non_camel_case_types, dead_code)]

// ============================================================================
// Opaque handle types
// ============================================================================

pub type RTCDevice = *mut std::ffi::c_void;
pub type RTCScene = *mut std::ffi::c_void;
pub type RTCGeometry = *mut std::ffi::c_void;
pub type RTCBuffer = *mut std::ffi::c_void;

// ============================================================================
// Enums
// ============================================================================

#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum RTCGeometryType {
    Triangle = 0,
    Subdivision = 8,
    Instance = 121,
}

#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum RTCBufferType {
    Index = 0,
    Vertex = 1,
    VertexAttribute = 2,
    Face = 16,
    EdgeCreaseIndex = 18,
    EdgeCreaseWeight = 19,
}

#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum RTCFormat {
    Undefined = 0,
    UInt = 0x5001,
    UInt3 = 0x5003,
    Float = 0x9001,
    Float2 = 0x9002,
    Float3 = 0x9003,
    Float4x4ColumnMajor = 0x9244,
}

#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum RTCSubdivisionMode {
    NoBoundary = 0,
    SmoothBoundary = 1,
    PinCorners = 2,
    PinBoundary = 3,
    PinAll = 4,
}

#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum RTCSceneFlags {
    None = 0,
    Robust = 1,
}

#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum RTCBuildQuality {
    Low = 0,
    Medium = 1,
    High = 2,
}

// ============================================================================
// Structs
// ============================================================================

pub const RTC_INVALID_GEOMETRY_ID: u32 = 0xFFFFFFFF;

#[repr(C, align(16))]
#[derive(Debug, Copy, Clone)]
pub struct RTCRay {
    pub org_x: f32,
    pub org_y: f32,
    pub org_z: f32,
    pub tnear: f32,
    pub dir_x: f32,
    pub dir_y: f32,
    pub dir_z: f32,
    pub time: f32,
    pub tfar: f32,
    pub mask: u32,
    pub id: u32,
    pub flags: u32,
}

#[repr(C, align(16))]
#[derive(Debug, Copy, Clone)]
pub struct RTCHit {
    pub ng_x: f32,
    pub ng_y: f32,
    pub ng_z: f32,
    pub u: f32,
    pub v: f32,
    pub prim_id: u32,
    pub geom_id: u32,
    pub inst_id: [u32; 1],
}

#[repr(C, align(16))]
#[derive(Debug, Copy, Clone)]
pub struct RTCRayHit {
    pub ray: RTCRay,
    pub hit: RTCHit,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct RTCBounds {
    pub lower_x: f32,
    pub lower_y: f32,
    pub lower_z: f32,
    pub align0: f32,
    pub upper_x: f32,
    pub upper_y: f32,
    pub upper_z: f32,
    pub align1: f32,
}

impl Default for RTCBounds {
    fn default() -> Self {
        Self {
            lower_x: f32::MAX,
            lower_y: f32::MAX,
            lower_z: f32::MAX,
            align0: 0.0,
            upper_x: f32::MIN,
            upper_y: f32::MIN,
            upper_z: f32::MIN,
            align1: 0.0,
        }
    }
}

// ============================================================================
// Extern functions
// ============================================================================

#[link(name = "embree4")]
extern "C" {
    pub fn rtcNewDevice(config: *const std::ffi::c_char) -> RTCDevice;
    pub fn rtcReleaseDevice(device: RTCDevice);
    pub fn rtcGetDeviceError(device: RTCDevice) -> i32;

    pub fn rtcNewScene(device: RTCDevice) -> RTCScene;
    pub fn rtcReleaseScene(scene: RTCScene);
    pub fn rtcCommitScene(scene: RTCScene);
    pub fn rtcGetSceneBounds(scene: RTCScene, bounds: *mut RTCBounds);

    pub fn rtcNewGeometry(device: RTCDevice, geom_type: RTCGeometryType) -> RTCGeometry;
    pub fn rtcReleaseGeometry(geom: RTCGeometry);
    pub fn rtcCommitGeometry(geom: RTCGeometry);
    pub fn rtcAttachGeometry(scene: RTCScene, geom: RTCGeometry) -> u32;
    pub fn rtcAttachGeometryByID(scene: RTCScene, geom: RTCGeometry, id: u32);
    pub fn rtcGetGeometry(scene: RTCScene, geom_id: u32) -> RTCGeometry;

    pub fn rtcSetSharedGeometryBuffer(
        geom: RTCGeometry,
        buffer_type: u32,
        slot: u32,
        format: u32,
        ptr: *const std::ffi::c_void,
        byte_offset: usize,
        byte_stride: usize,
        item_count: usize,
    );

    pub fn rtcSetGeometryInstancedScene(geom: RTCGeometry, scene: RTCScene);
    pub fn rtcSetGeometryTransform(geom: RTCGeometry, time_step: u32, format: u32, xfm: *const f32);
    pub fn rtcSetGeometryVertexAttributeCount(geom: RTCGeometry, vertex_attribute_count: u32);
    pub fn rtcSetGeometrySubdivisionMode(
        geom: RTCGeometry,
        topology_id: u32,
        mode: RTCSubdivisionMode,
    );

    pub fn rtcSetGeometryTessellationRate(geom: RTCGeometry, rate: f32);
    pub fn rtcSetGeometryTopologyCount(geom: RTCGeometry, topology_count: u32);
    pub fn rtcSetGeometryVertexAttributeTopology(
        geom: RTCGeometry,
        vertex_attribute_id: u32,
        topology_id: u32,
    );

    pub fn rtcIntersect1(scene: RTCScene, rayhit: *mut RTCRayHit, args: *const std::ffi::c_void);

    /// Interpolate vertex data at (u,v) on a primitive (supports subdivision surfaces).
    pub fn rtcInterpolate(args: *const RTCInterpolateArguments);
}

/// Arguments for rtcInterpolate.
#[repr(C)]
pub struct RTCInterpolateArguments {
    pub geometry: RTCGeometry,
    pub prim_id: u32,
    pub u: f32,
    pub v: f32,
    pub buffer_type: u32,
    pub buffer_slot: u32,
    pub p: *mut f32,
    pub dp_du: *mut f32,
    pub dp_dv: *mut f32,
    pub ddp_dudu: *mut f32,
    pub ddp_dvdv: *mut f32,
    pub ddp_dudv: *mut f32,
    pub value_count: u32,
}
