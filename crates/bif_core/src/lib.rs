//! BIF Core - Scene graph and USD support for VFX rendering.
//!
//! This crate provides:
//!
//! - **Scene graph types**: `Scene`, `Prototype`, `Instance`, `Mesh`
//! - **USD support**: All USD formats via C++ bridge (USDA, USD, USDC)
//!
//! # Example
//!
//! ```ignore
//! use bif_core::usd::load_usd;
//! use bif_core::scene::Scene;
//!
//! // Load any USD format (usda, usd, usdc)
//! let scene = load_usd("scene.usdc")?;
//! println!("Loaded {} prototypes, {} instances",
//!     scene.prototype_count(),
//!     scene.instance_count());
//! ```

pub mod mesh;
pub mod scene;
pub mod texture;
pub mod usd;

// Re-export commonly used types
pub use mesh::Mesh;
pub use scene::{Instance, Material, Prototype, Scene, Transform};
pub use texture::{Texture, TextureCache, TextureError, TextureResult};
pub use usd::{load_usd, load_usda, load_usda_from_string};
