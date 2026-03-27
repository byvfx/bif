//! Metric: time to load payloads after stage open.

use std::path::Path;
use std::time::Instant;

use bif_core::usd::cpp_bridge::UsdStage;

use super::{Measurement, MeasurementMeta, Metric, MetricError};

/// Measures `stage.load_payloads()` duration (geometry + material caching).
pub struct PayloadLoad;

impl Metric for PayloadLoad {
    fn name(&self) -> &str {
        "Payload Load"
    }

    fn id(&self) -> &str {
        "payload_load"
    }

    fn measure(&self, scene_path: &Path) -> Result<Measurement, MetricError> {
        // Setup (not timed): open stage
        let stage = UsdStage::open(scene_path).map_err(|e| MetricError::Bridge(e.to_string()))?;

        // Timed: load payloads
        let start = Instant::now();
        let prim_count = stage
            .load_payloads()
            .map_err(|e| MetricError::Bridge(e.to_string()))?;
        let duration = start.elapsed();

        Ok(Measurement {
            duration,
            metadata: Some(MeasurementMeta {
                prim_count: Some(prim_count),
                ..Default::default()
            }),
        })
    }
}
