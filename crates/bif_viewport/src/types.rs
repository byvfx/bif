//! Shared type definitions for the viewport crate.
//!
//! Contains enums, structs, and messages that are used across multiple modules
//! but are not part of the Renderer struct itself.

use std::sync::Arc;

use bif_math::Mat4;

use crate::batch_render;
use crate::texture_loader;

/// Which USD purpose geometry to display in the viewport.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PurposeMode {
    /// Show Default + Render purpose geometry (full detail).
    Render,
    /// Show Default + Proxy purpose geometry (low-res preview).
    Proxy,
    /// Show all purposes (Default + Render + Proxy + Guide).
    All,
    /// Show Default + Guide purpose geometry (helper viz).
    Guide,
}

impl PurposeMode {
    /// Whether the given purpose is visible under this mode.
    pub fn includes(self, purpose: bif_core::Purpose) -> bool {
        use bif_core::Purpose;
        match purpose {
            Purpose::Default => true, // Default is always visible
            Purpose::Render => matches!(self, Self::Render | Self::All),
            Purpose::Proxy => matches!(self, Self::Proxy | Self::All),
            Purpose::Guide => matches!(self, Self::Guide | Self::All),
        }
    }
}

/// Viewport shading mode — controls how surfaces are colored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ShadingMode {
    /// Full material with textures (default)
    Textured,
    /// Display color only (primvars:displayColor, ignores textures)
    DisplayColor,
}

impl ShadingMode {
    /// GPU uniform value (matches shader constants).
    pub fn as_u32(self) -> u32 {
        match self {
            Self::Textured => 0,
            Self::DisplayColor => 1,
        }
    }
}

/// Framework-agnostic display settings — UI layer reads/writes these.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DisplaySettings {
    /// Which purpose geometry to show (Render or Proxy).
    pub purpose_mode: PurposeMode,
    /// Whether the built-in box-LOD system is enabled.
    pub lod_enabled: bool,
    /// Viewport shading mode (textured vs display color).
    pub shading_mode: ShadingMode,
}

impl Default for DisplaySettings {
    fn default() -> Self {
        Self {
            purpose_mode: PurposeMode::Render,
            lod_enabled: true,
            shading_mode: ShadingMode::Textured,
        }
    }
}

/// Status of an asynchronous USD load operation.
#[derive(Debug, Clone)]
pub enum UsdLoadStatus {
    /// No load in progress.
    Idle,
    /// Loading in progress with granular progress info.
    Loading(UsdLoadProgress),
    /// Load completed — scene + stage ready for GPU finalization.
    Ready,
    /// Load failed with error message.
    Error(String),
}

/// Granular progress stages for USD loading.
#[derive(Debug, Clone)]
pub enum UsdLoadProgress {
    /// Opening the USD stage via C++ bridge.
    OpeningStage,
    /// Extracting mesh geometry.
    ExtractingMeshes { current: usize, total: usize },
    /// Extracting materials.
    ExtractingMaterials { current: usize, total: usize },
    /// Extracting lights.
    ExtractingLights,
    /// Building BIF scene graph.
    BuildingScene,
    /// Finalizing (creating GPU buffers).
    Finalizing,
}

impl std::fmt::Display for UsdLoadProgress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OpeningStage => write!(f, "Opening USD stage..."),
            Self::ExtractingMeshes { current, total } => {
                write!(f, "Extracting meshes ({current}/{total})...")
            }
            Self::ExtractingMaterials { current, total } => {
                write!(f, "Extracting materials ({current}/{total})...")
            }
            Self::ExtractingLights => write!(f, "Extracting lights..."),
            Self::BuildingScene => write!(f, "Building scene..."),
            Self::Finalizing => write!(f, "Creating GPU buffers..."),
        }
    }
}

impl std::fmt::Display for UsdLoadStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Idle => write!(f, ""),
            Self::Loading(p) => write!(f, "{p}"),
            Self::Ready => write!(f, "Ready"),
            Self::Error(e) => write!(f, "Error: {e}"),
        }
    }
}

/// Message sent from the background USD load thread.
pub(crate) enum UsdLoadMessage {
    /// Progress update.
    Progress(UsdLoadProgress),
    /// Load completed with scene + stage.
    Complete {
        scene: Box<bif_core::Scene>,
        stage: bif_core::usd::UsdStage,
        path: std::path::PathBuf,
    },
    /// Load failed.
    Failed(String),
}

/// Async channel receivers and status for background operations.
pub(crate) struct AsyncChannels {
    /// Receiver for messages from the background USD load thread.
    pub usd_load_receiver: Option<std::sync::mpsc::Receiver<UsdLoadMessage>>,
    /// Current status of the async USD load (for UI display).
    pub usd_load_status: UsdLoadStatus,
    /// Receiver for textures loaded on background thread.
    pub texture_load_receiver:
        Option<std::sync::mpsc::Receiver<texture_loader::TextureLoadMessage>>,
    /// Receiver for batch render messages.
    pub batch_receiver: Option<std::sync::mpsc::Receiver<batch_render::BatchMessage>>,
    /// Cancel flag for batch render.
    pub batch_cancel_flag: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    /// Receiver for background material pre-warm thread (materials + texture cache).
    pub ivar_materials_receiver: Option<
        std::sync::mpsc::Receiver<(
            Vec<Arc<bif_renderer::OpenPbrSurface>>,
            bif_core::texture::TextureCache,
        )>,
    >,
}

impl Default for AsyncChannels {
    fn default() -> Self {
        Self {
            usd_load_receiver: None,
            usd_load_status: UsdLoadStatus::Idle,
            texture_load_receiver: None,
            batch_receiver: None,
            batch_cancel_flag: None,
            ivar_materials_receiver: None,
        }
    }
}

/// UI panel dimensions (for viewport-safe overlays).
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct UiLayout {
    pub left_panel_width: f32,
    pub right_panel_width: f32,
    pub top_panel_height: f32,
    pub bottom_panel_height: f32,
}

/// Parallel arrays of per-instance data (transforms, materials, prototype IDs, prim paths).
/// Named `SceneInstances` to avoid confusion with `gpu_types::InstanceData`.
#[derive(Default)]
pub struct SceneInstances {
    /// Base transforms (from scene load) — used for re-evaluation.
    pub transforms: Vec<Mat4>,
    /// Current transforms (after animation evaluation) — used for rendering.
    pub current: Vec<Mat4>,
    /// Material ID per instance.
    pub material_ids: Vec<u32>,
    /// Prototype ID per instance (for multi-draw rebuild).
    pub prototype_ids: Vec<usize>,
    /// Prim path per instance (for USD export).
    pub prim_paths: Vec<String>,
    /// USD purpose per instance (for viewport purpose filtering).
    pub purposes: Vec<bif_core::Purpose>,
}
