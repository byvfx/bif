//! Benchmark orchestrator: warmup → N iterations → statistics.

use std::path::Path;
use std::time::Duration;

use crate::config::RunConfig;
use crate::metrics::{Measurement, MeasurementMeta, Metric, MetricError};
use crate::stats::Stats;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Results for one metric across all iterations.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MetricResults {
    pub metric_id: String,
    pub metric_name: String,
    pub measurements: Vec<Duration>,
    pub stats: Stats,
    /// Metadata from the last iteration (prim count, mesh count, etc.).
    pub metadata: Option<MeasurementMeta>,
}

/// Results for one scene across all metrics.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SceneResults {
    pub scene_name: String,
    pub scene_path: String,
    pub target_name: String,
    pub timestamp: String,
    pub config: RunConfig,
    pub metrics: Vec<MetricResults>,
}

#[derive(Debug, Error)]
pub enum RunError {
    #[error("metric '{0}' failed: {1}")]
    MetricFailed(String, MetricError),
    #[error("scene not found: {0}")]
    SceneNotFound(String),
}

/// Run all metrics for one scene.
pub fn run_scene(
    scene_name: &str,
    scene_path: &Path,
    metrics: &[Box<dyn Metric>],
    config: &RunConfig,
    target_name: &str,
) -> Result<SceneResults, RunError> {
    if !scene_path.exists() {
        return Err(RunError::SceneNotFound(scene_path.display().to_string()));
    }

    let timestamp = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string();
    let mut metric_results = Vec::new();

    for metric in metrics {
        // Filter
        if let Some(ref filter) = config.metric_filter {
            if !filter.iter().any(|f| f == metric.id()) {
                continue;
            }
        }

        log::info!(
            "[{}] {} — warmup ({} iters)",
            scene_name,
            metric.name(),
            config.warmup_iterations
        );

        // Warmup
        for _ in 0..config.warmup_iterations {
            metric
                .warmup(scene_path)
                .map_err(|e| RunError::MetricFailed(metric.id().to_string(), e))?;
        }

        // Measure
        log::info!(
            "[{}] {} — measuring ({} iters)",
            scene_name,
            metric.name(),
            config.iterations
        );

        let mut durations = Vec::with_capacity(config.iterations);
        let mut last_measurement: Option<Measurement> = None;
        for i in 0..config.iterations {
            let m = metric
                .measure(scene_path)
                .map_err(|e| RunError::MetricFailed(metric.id().to_string(), e))?;
            durations.push(m.duration);

            if (i + 1) % 10 == 0 {
                log::debug!(
                    "  iter {}/{}: {:.4}s",
                    i + 1,
                    config.iterations,
                    m.duration.as_secs_f64()
                );
            }
            last_measurement = Some(m);
        }

        let stats = Stats::from_durations(&durations);
        log::info!("[{}] {} — {}", scene_name, metric.name(), stats);

        metric_results.push(MetricResults {
            metric_id: metric.id().to_string(),
            metric_name: metric.name().to_string(),
            measurements: durations,
            stats,
            metadata: last_measurement.and_then(|m| m.metadata),
        });
    }

    Ok(SceneResults {
        scene_name: scene_name.to_string(),
        scene_path: scene_path.display().to_string(),
        target_name: target_name.to_string(),
        timestamp,
        config: config.clone(),
        metrics: metric_results,
    })
}
