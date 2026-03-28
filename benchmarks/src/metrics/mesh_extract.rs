//! Metric: time to extract mesh data from a loaded USD stage.

use std::path::Path;
use std::time::Instant;

use bif_core::usd::cpp_bridge::UsdStage;

use super::{Measurement, MeasurementMeta, Metric, MetricError};

/// Measures `stage.meshes()` duration — vertex/index/normal/UV extraction.
pub struct MeshExtract;

impl Metric for MeshExtract {
    fn name(&self) -> &str {
        "Mesh Extract"
    }

    fn id(&self) -> &str {
        "mesh_extract"
    }

    fn measure(&self, scene_path: &Path) -> Result<Measurement, MetricError> {
        // Setup (not timed): open stage + load payloads
        let stage = UsdStage::open(scene_path).map_err(|e| MetricError::Bridge(e.to_string()))?;
        stage
            .load_payloads()
            .map_err(|e| MetricError::Bridge(e.to_string()))?;

        // Timed: extract all mesh data
        let start = Instant::now();
        let meshes = stage
            .meshes()
            .map_err(|e| MetricError::Bridge(e.to_string()))?;
        let duration = start.elapsed();

        Ok(Measurement {
            duration,
            metadata: Some(MeasurementMeta {
                mesh_count: Some(meshes.len()),
                ..Default::default()
            }),
        })
    }
}
