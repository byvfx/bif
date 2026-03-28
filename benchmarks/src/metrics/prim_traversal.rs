//! Metric: time to traverse all prims in a loaded USD stage.

use std::path::Path;
use std::time::Instant;

use bif_core::usd::cpp_bridge::UsdStage;

use super::{Measurement, MeasurementMeta, Metric, MetricError};

/// Measures prim traversal — iterates all prims via `prim_count()` + `get_prim_info()`.
/// Matches USD's custom "traverse stage" metric from `stageTraversalMetric.py`.
pub struct PrimTraversal;

impl Metric for PrimTraversal {
    fn name(&self) -> &str {
        "Prim Traversal"
    }

    fn id(&self) -> &str {
        "prim_traversal"
    }

    fn measure(&self, scene_path: &Path) -> Result<Measurement, MetricError> {
        // Setup (not timed): open stage + load payloads
        let stage = UsdStage::open(scene_path).map_err(|e| MetricError::Bridge(e.to_string()))?;
        stage
            .load_payloads()
            .map_err(|e| MetricError::Bridge(e.to_string()))?;

        let count = stage
            .prim_count()
            .map_err(|e| MetricError::Bridge(e.to_string()))?;

        // Timed: traverse all prims
        let start = Instant::now();
        for i in 0..count {
            let _ = stage.get_prim_info(i);
        }
        let duration = start.elapsed();

        Ok(Measurement {
            duration,
            metadata: Some(MeasurementMeta {
                prim_count: Some(count),
                ..Default::default()
            }),
        })
    }
}
