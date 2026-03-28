//! Metric trait and registry.

pub mod full_load;
pub mod material_load;
pub mod mesh_extract;
pub mod payload_load;
pub mod prim_traversal;
pub mod stage_close;
pub mod stage_open;

use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// A single measurement result from one iteration.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Measurement {
    pub duration: Duration,
    pub metadata: Option<MeasurementMeta>,
}

/// Optional metadata captured alongside timing.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MeasurementMeta {
    pub prim_count: Option<usize>,
    pub mesh_count: Option<usize>,
    pub instance_count: Option<usize>,
}

/// Errors from metric measurement.
#[derive(Debug, Error)]
pub enum MetricError {
    #[error("USD bridge: {0}")]
    Bridge(String),
    #[error("IO: {0}")]
    Io(#[from] std::io::Error),
    #[error("subprocess failed: {0}")]
    Subprocess(String),
}

/// A composable performance metric.
///
/// Each implementation measures one isolated phase of USD processing.
/// Implementations must be stateless — all state flows through the scene path.
pub trait Metric: Send + Sync {
    /// Human-readable name (e.g., "Stage Open").
    fn name(&self) -> &str;

    /// Short identifier for keys (e.g., "stage_open").
    fn id(&self) -> &str;

    /// Run one measurement iteration.
    fn measure(&self, scene_path: &Path) -> Result<Measurement, MetricError>;

    /// Warmup before the iteration loop. Default: one measure() call discarded.
    fn warmup(&self, scene_path: &Path) -> Result<(), MetricError> {
        let _ = self.measure(scene_path)?;
        Ok(())
    }
}

/// All available BIF-native metrics.
pub fn all_bif_metrics() -> Vec<Box<dyn Metric>> {
    vec![
        Box::new(stage_open::StageOpen),
        Box::new(payload_load::PayloadLoad),
        Box::new(mesh_extract::MeshExtract),
        Box::new(material_load::MaterialLoad),
        Box::new(prim_traversal::PrimTraversal),
        Box::new(stage_close::StageClose),
        Box::new(full_load::FullLoad),
    ]
}
