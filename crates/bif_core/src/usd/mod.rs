//! USD (Universal Scene Description) support for BIF.
//!
//! This module provides parsing and loading of USDA (ASCII) files,
//! converting them to BIF's scene graph representation.
//!
//! ## Supported USD Features (Milestone 9)
//!
//! - `UsdGeomMesh`: Triangle meshes with positions, normals, indices
//! - `UsdGeomPointInstancer`: Instanced geometry with transforms
//! - `Xform`: Transform hierarchies with xformOps
//!
//! ## Not Yet Supported
//!
//! - Binary `.usdc` format
//! - Materials and textures (`UsdShade`)
//! - Lights (`UsdLux`)
//! - Cameras (`UsdGeomCamera`)
//! - Animation / time samples
//! - References and payloads (Milestone 10)
//! - Variants and composition arcs
//!
//! # Example
//!
//! ```ignore
//! use bif_core::usd::load_usda;
//!
//! let scene = load_usda("path/to/scene.usda")?;
//! println!("Loaded {} prototypes, {} instances",
//!     scene.prototype_count(),
//!     scene.instance_count());
//! ```

// TODO: Consider nom/pest for robustness if grammar complexity grows

mod types;
mod parser;
mod loader;

pub use types::*;
pub use parser::*;
pub use loader::*;
