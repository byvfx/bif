//! USD (Universal Scene Description) support for BIF.
//!
//! This module provides loading of USD files (USDA, USD, USDC formats)
//! via the C++ USD library, converting them to BIF's scene graph representation.
//!
//! ## Supported USD Features (Milestone 13)
//!
//! - `UsdGeomMesh`: Triangle meshes with positions, normals, indices
//! - `UsdGeomPointInstancer`: Instanced geometry with transforms
//! - `Xform`: Transform hierarchies with xformOps
//! - **File references**: `@path/to/file.usda@</Prim>` syntax
//! - **Binary format**: `.usdc` files (via C++ bridge)
//! - **Auto-detect format**: `.usd` files
//!
//! ## Not Yet Supported
//!
//! - Materials and textures (`UsdShade`)
//! - Lights (`UsdLux`)
//! - Cameras (`UsdGeomCamera`)
//! - Animation / time samples
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
mod loader;
mod parser;
mod types;

pub use cpp_bridge::{UsdBridgeError, UsdInstancerData, UsdMeshData, UsdStage};
pub use loader::*;
pub use parser::*;
pub use types::*;
