//! Metric: time to open a USD stage (LoadNone — hierarchy only).

use std::path::Path;
use std::time::Instant;

use bif_core::usd::cpp_bridge::UsdStage;

use super::{Measurement, Metric, MetricError};

/// Measures `UsdStage::open()` duration (LoadNone policy).
pub struct StageOpen;

impl Metric for StageOpen {
    fn name(&self) -> &str {
        "Stage Open"
    }

    fn id(&self) -> &str {
        "stage_open"
    }

    fn measure(&self, scene_path: &Path) -> Result<Measurement, MetricError> {
        let start = Instant::now();
        let stage = UsdStage::open(scene_path).map_err(|e| MetricError::Bridge(e.to_string()))?;
        let duration = start.elapsed();
        // Explicit drop to avoid timing interference from deferred cleanup
        drop(stage);
        Ok(Measurement {
            duration,
            metadata: None,
        })
    }
}
