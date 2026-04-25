//! USD (Universal Scene Description) support for BIF.
//!
//! This module provides loading of USD files (USDA, USD, USDC formats)
//! via the C++ USD library, converting them to BIF's scene graph representation.
//!
//! ## Supported USD Features
//!
//! - `UsdGeomMesh`: Triangle meshes with positions, normals, indices
//! - `UsdGeomPointInstancer`: Instanced geometry with transforms
//! - `Xform`: Transform hierarchies with xformOps
//! - **File references**: `@path/to/file.usda@</Prim>` syntax
//! - **Binary format**: `.usdc` files (via C++ bridge)
//! - **Auto-detect format**: `.usd` files
//! - **Materials** (`UsdShade`): PBR materials with textures
//! - **Lights** (`UsdLux`): Distant, Sphere, Rect, Dome lights
//! - **Animation**: Time samples for transforms and geometry
//!
//! ## Not Yet Supported
//!
//! - Cameras (`UsdGeomCamera`)
//! - Payloads and variants
//!
//! # Example
//!
//! ```ignore
//! use bif_core::usd::load_usd;
//!
//! // Works with .usda, .usd, or .usdc files
//! let scene = load_usd("path/to/scene.usdc")?;
//! println!("Loaded {} prototypes, {} instances",
//!     scene.prototype_count(),
//!     scene.instance_count());
//! ```

pub mod cpp_bridge;
pub mod displacement;
pub mod export;
pub(crate) mod ffi_convert;
pub(crate) mod ffi_raw;
pub mod layer;
mod loader;
mod types;
pub mod validate;

pub use cpp_bridge::{
    CameraProperties, CurveBasis, CurveType, CurveWrap, MeshPurpose, NormalsInterpolation,
    PrimvarInterpolation, PrimvarType, SubdivisionScheme, TransformSample,
    UsdAnimatedInstancerData, UsdAnimatedMeshData, UsdAttributeData, UsdBridgeError, UsdCurvesData,
    UsdEditLayer, UsdInstancerData, UsdKind, UsdLightData, UsdLightShaping, UsdLightType,
    UsdMeshData, UsdNativeInstance, UsdPointsData, UsdPrimType, UsdPrimvarData, UsdSkeletonData,
    UsdSkinBindingData, UsdSpecifier, UsdStage, UsdTimelineData, UsdVolumeData,
};
pub use export::{AuthoredPrim, ExportConfig, ExportResult};
pub use layer::{
    EditTarget, LayerInfo, LayerOffset, LayerStack, OpinionSource, PayloadPolicy, PrimSpecifier,
    PrimStackEntry,
};
pub use loader::{
    load_usd, load_usd_with_stage, load_usd_with_stage_muted, load_usd_with_stage_policy_muted,
    load_usda, load_usda_from_string, LoadError, LoadResult,
};
pub use types::{
    compose_xform_ops, UsdMesh, UsdPointInstancer, UsdPrim, UsdReference, UsdXform, XformOp,
};
