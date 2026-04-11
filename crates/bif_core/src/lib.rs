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
pub mod point_cloud;
pub mod primitives;
pub mod scatter;
pub mod scene;
pub mod scene_query;
pub mod skinning;
pub mod texture;
pub mod undo;
pub mod usd;

#[cfg(feature = "oiio")]
pub mod oiio;

// Re-export commonly used types
pub use mesh::{Mesh, SkinBinding, SkinKind};
pub use point_cloud::{DistributionMethod, PointAttributes, PointCloud};
pub use primitives::PrimitiveKind;
pub use scatter::PointSource;
pub use scene::{
    AnimatedTransform, Instance, Light, Material, Prototype, Purpose, Scene, SceneCamera,
    TimelineInfo, Transform, TransformKeyframe,
};
pub use scene_query::SceneQuery;
pub use texture::{Texture, TextureCache, TextureError, TextureResult};
pub use undo::{
    CreatePrimitiveCommand, DeletePrimitiveCommand, EditState, KeyframeCommand, ScatterCommand,
    SceneOp, TransformCommand, UndoCommand, UndoStack,
};
pub use usd::{
    load_usd, load_usda, load_usda_from_string, AuthoredPrim, ExportConfig, ExportResult,
};

#[cfg(feature = "oiio")]
pub use oiio::{
    get_tx_path, load_texture_with_mips, make_tx, tx_is_valid, MipLevel, OiioError, OiioResult,
    OiioTexture, TxOptions,
};
