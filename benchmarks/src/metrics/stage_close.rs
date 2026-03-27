//! Metric: time to close/drop a fully-loaded USD stage.

use std::path::Path;
use std::time::Instant;

use bif_core::usd::cpp_bridge::UsdStage;

use super::{Measurement, Metric, MetricError};

/// Measures the time to close a stage (via Drop → `usd_bridge_close_stage`).
pub struct StageClose;

impl Metric for StageClose {
    fn name(&self) -> &str {
        "Stage Close"
    }

    fn id(&self) -> &str {
        "stage_close"
    }

    fn measure(&self, scene_path: &Path) -> Result<Measurement, MetricError> {
        // Setup (not timed): open and load
        let stage = UsdStage::open(scene_path).map_err(|e| MetricError::Bridge(e.to_string()))?;
        stage
            .load_payloads()
            .map_err(|e| MetricError::Bridge(e.to_string()))?;

        // Timed: drop triggers usd_bridge_close_stage via FFI.
        // NOTE: timing depends on UsdStage::Drop calling usd_bridge_close_stage.
        // If the Drop impl changes to a manual .close() method, update this metric.
        let start = Instant::now();
        drop(stage);
        let duration = start.elapsed();

        Ok(Measurement {
            duration,
            metadata: None,
        })
    }
}
