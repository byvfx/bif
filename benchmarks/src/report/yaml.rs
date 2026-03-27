//! YAML output matching USD's `usdmeasureperformance.py` format.

use serde::Serialize;

use crate::metrics::MeasurementMeta;
use crate::runner::SceneResults;

#[derive(Serialize)]
struct YamlReport<'a> {
    target: &'a str,
    timestamp: &'a str,
    scenes: Vec<YamlScene<'a>>,
}

#[derive(Serialize)]
struct YamlScene<'a> {
    name: &'a str,
    path: &'a str,
    iterations: usize,
    metrics: Vec<YamlMetric<'a>>,
}

#[derive(Serialize)]
struct YamlMetric<'a> {
    id: &'a str,
    name: &'a str,
    min_s: f64,
    max_s: f64,
    mean_s: f64,
    median_s: f64,
    stddev_ms: f64,
    p95_s: f64,
    count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    metadata: Option<&'a MeasurementMeta>,
}

pub fn render(results: &[SceneResults]) -> String {
    if results.is_empty() {
        return "# No results\n".to_string();
    }

    let target = &results[0].target_name;
    let timestamp = &results[0].timestamp;

    let scenes: Vec<YamlScene> = results
        .iter()
        .map(|scene| YamlScene {
            name: &scene.scene_name,
            path: &scene.scene_path,
            iterations: scene.config.iterations,
            metrics: scene
                .metrics
                .iter()
                .map(|m| YamlMetric {
                    id: &m.metric_id,
                    name: &m.metric_name,
                    min_s: m.stats.min.as_secs_f64(),
                    max_s: m.stats.max.as_secs_f64(),
                    mean_s: m.stats.mean.as_secs_f64(),
                    median_s: m.stats.median.as_secs_f64(),
                    stddev_ms: m.stats.stddev_ms,
                    p95_s: m.stats.p95.as_secs_f64(),
                    count: m.stats.count,
                    metadata: m.metadata.as_ref(),
                })
                .collect(),
        })
        .collect();

    let report = YamlReport {
        target,
        timestamp,
        scenes,
    };

    serde_yml::to_string(&report).unwrap_or_else(|e| format!("# YAML error: {e}\n"))
}
