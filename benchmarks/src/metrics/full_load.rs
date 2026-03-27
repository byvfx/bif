//! Metric: end-to-end USD scene load via `load_usd()`.

use std::path::Path;
use std::time::Instant;

use bif_core::usd::load_usd;

use super::{Measurement, MeasurementMeta, Metric, MetricError};

/// Measures the full `load_usd()` pipeline: stage open → payloads →
/// mesh extraction → material load → scene construction.
pub struct FullLoad;

impl Metric for FullLoad {
    fn name(&self) -> &str {
        "Full Load"
    }

    fn id(&self) -> &str {
        "full_load"
    }

    fn measure(&self, scene_path: &Path) -> Result<Measurement, MetricError> {
        let start = Instant::now();
        let scene = load_usd(scene_path).map_err(|e| MetricError::Bridge(e.to_string()))?;
        let duration = start.elapsed();

        Ok(Measurement {
            duration,
            metadata: Some(MeasurementMeta {
                mesh_count: Some(scene.prototype_count()),
                instance_count: Some(scene.instance_count()),
                ..Default::default()
            }),
        })
    }
}
