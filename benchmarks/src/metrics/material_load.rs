//! Metric: time to extract material data from a loaded USD stage.

use std::path::Path;
use std::time::Instant;

use bif_core::usd::cpp_bridge::UsdStage;

use super::{Measurement, MeasurementMeta, Metric, MetricError};

/// Measures `stage.materials()` duration — PBR material extraction.
pub struct MaterialLoad;

impl Metric for MaterialLoad {
    fn name(&self) -> &str {
        "Material Load"
    }

    fn id(&self) -> &str {
        "material_load"
    }

    fn measure(&self, scene_path: &Path) -> Result<Measurement, MetricError> {
        // Setup (not timed): open stage + load payloads
        let stage = UsdStage::open(scene_path).map_err(|e| MetricError::Bridge(e.to_string()))?;
        stage
            .load_payloads()
            .map_err(|e| MetricError::Bridge(e.to_string()))?;

        // Timed: extract all material data
        let start = Instant::now();
        let materials = stage
            .materials()
            .map_err(|e| MetricError::Bridge(e.to_string()))?;
        let duration = start.elapsed();

        Ok(Measurement {
            duration,
            metadata: Some(MeasurementMeta {
                mesh_count: Some(materials.len()),
                ..Default::default()
            }),
        })
    }
}
