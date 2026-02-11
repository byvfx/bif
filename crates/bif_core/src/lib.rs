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

pub mod hdr;
pub mod ibl;
pub mod mesh;
pub mod primitives;
pub mod scene;
pub mod texture;
pub mod undo;
pub mod usd;

#[cfg(feature = "oiio")]
pub mod oiio;

// Re-export commonly used types
pub use mesh::Mesh;
pub use primitives::PrimitiveKind;
pub use scene::{
    AnimatedTransform, Instance, Light, Material, Prototype, Scene, SceneCamera, TimelineInfo,
    Transform, TransformKeyframe,
};
pub use texture::{Texture, TextureCache, TextureError, TextureResult};
pub use undo::{
    CreatePrimitiveCommand, DeletePrimitiveCommand, EditState, KeyframeCommand, SceneOp,
    TransformCommand, UndoCommand, UndoStack,
};
pub use usd::{load_usd, load_usda, load_usda_from_string};

#[cfg(feature = "oiio")]
pub use oiio::{
    get_tx_path, load_texture_with_mips, make_tx, tx_is_valid, MipLevel, OiioError, OiioResult,
    OiioTexture, TxOptions,
};
