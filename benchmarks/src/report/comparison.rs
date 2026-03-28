//! Compare two benchmark result sets and report regressions/improvements.

use std::path::Path;

use tabled::{Table, Tabled};

use crate::runner::SceneResults;

#[derive(Tabled)]
struct CompareRow {
    #[tabled(rename = "Scene")]
    scene: String,
    #[tabled(rename = "Metric")]
    metric: String,
    #[tabled(rename = "Baseline (s)")]
    baseline: String,
    #[tabled(rename = "Current (s)")]
    current: String,
    #[tabled(rename = "Delta (ms)")]
    delta_ms: String,
    #[tabled(rename = "Delta (%)")]
    delta_pct: String,
    #[tabled(rename = "Status")]
    status: String,
}

/// Load a YAML results file into SceneResults.
pub fn load_results(path: &Path) -> Result<Vec<SceneResults>, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Cannot read {}: {e}", path.display()))?;

    // The YAML file has a wrapper with target/timestamp/scenes.
    // Parse the scenes array from it.
    let value: serde_yml::Value =
        serde_yml::from_str(&content).map_err(|e| format!("YAML parse error: {e}"))?;

    let scenes = value.get("scenes").ok_or("No 'scenes' key in YAML")?;

    let results: Vec<SceneResults> =
        serde_yml::from_value(scenes.clone()).map_err(|e| format!("Cannot parse scenes: {e}"))?;

    Ok(results)
}

/// Compare baseline vs current results and render a terminal table.
pub fn compare(baseline_path: &Path, current_path: &Path) -> Result<String, String> {
    let baseline = load_results(baseline_path)?;
    let current = load_results(current_path)?;

    let mut rows = Vec::new();

    for cur_scene in &current {
        // Find matching baseline scene by name
        let base_scene = baseline
            .iter()
            .find(|b| b.scene_name == cur_scene.scene_name);

        for cur_metric in &cur_scene.metrics {
            let base_mean = base_scene
                .and_then(|bs| {
                    bs.metrics
                        .iter()
                        .find(|m| m.metric_id == cur_metric.metric_id)
                })
                .map(|m| m.stats.mean.as_secs_f64());

            let cur_mean = cur_metric.stats.mean.as_secs_f64();

            let (delta_ms, delta_pct, status) = if let Some(base) = base_mean {
                let delta = cur_mean - base;
                let delta_ms_val = delta * 1000.0;
                let pct = if base > 0.0 {
                    (delta / base) * 100.0
                } else {
                    0.0
                };
                let status = if pct < -5.0 {
                    "FASTER"
                } else if pct > 5.0 {
                    "SLOWER"
                } else {
                    "~same"
                };
                (
                    format!("{delta_ms_val:+.2}"),
                    format!("{pct:+.1}%"),
                    status.to_string(),
                )
            } else {
                ("N/A".into(), "N/A".into(), "new".into())
            };

            rows.push(CompareRow {
                scene: cur_scene.scene_name.clone(),
                metric: cur_metric.metric_name.clone(),
                baseline: base_mean.map_or("N/A".into(), |v| format!("{v:.6}")),
                current: format!("{cur_mean:.6}"),
                delta_ms,
                delta_pct,
                status,
            });
        }
    }

    if rows.is_empty() {
        return Ok("No matching metrics to compare.".to_string());
    }

    Ok(Table::new(rows).to_string())
}
