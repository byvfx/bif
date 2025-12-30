//! BIF Core - Scene graph and USD support for VFX rendering.
//!
//! This crate provides:
//!
//! - **Scene graph types**: `Scene`, `Prototype`, `Instance`, `Mesh`
//! - **USD support**: USDA file parsing and scene loading
//!
//! # Example
//!
//! ```ignore
//! use bif_core::usd::load_usda;
//! use bif_core::scene::Scene;
//!
//! // Load a USD scene
//! let scene = load_usda("scene.usda")?;
//! println!("Loaded {} prototypes, {} instances",
//!     scene.prototype_count(),
//!     scene.instance_count());
//! ```

pub mod mesh;
pub mod scene;
pub mod usd;

// Re-export commonly used types
pub use mesh::Mesh;
pub use scene::{Instance, Material, Prototype, Scene, Transform};
pub use usd::{load_usda, load_usda_from_string};
