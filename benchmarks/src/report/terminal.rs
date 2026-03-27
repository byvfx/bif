//! Terminal table output using the `tabled` crate.

use tabled::{Table, Tabled};

use crate::runner::SceneResults;

#[derive(Tabled)]
struct MetricRow {
    #[tabled(rename = "Scene")]
    scene: String,
    #[tabled(rename = "Metric")]
    metric: String,
    #[tabled(rename = "Min (s)")]
    min: String,
    #[tabled(rename = "Max (s)")]
    max: String,
    #[tabled(rename = "Mean (s)")]
    mean: String,
    #[tabled(rename = "Median (s)")]
    median: String,
    #[tabled(rename = "Stddev (ms)")]
    stddev: String,
    #[tabled(rename = "P95 (s)")]
    p95: String,
    #[tabled(rename = "N")]
    count: String,
}

pub fn render(results: &[SceneResults]) -> String {
    let rows: Vec<MetricRow> = results
        .iter()
        .flat_map(|scene| {
            scene.metrics.iter().map(move |m| MetricRow {
                scene: scene.scene_name.clone(),
                metric: m.metric_name.clone(),
                min: format!("{:.6}", m.stats.min.as_secs_f64()),
                max: format!("{:.6}", m.stats.max.as_secs_f64()),
                mean: format!("{:.6}", m.stats.mean.as_secs_f64()),
                median: format!("{:.6}", m.stats.median.as_secs_f64()),
                stddev: format!("{:.2}", m.stats.stddev_ms),
                p95: format!("{:.6}", m.stats.p95.as_secs_f64()),
                count: m.stats.count.to_string(),
            })
        })
        .collect();

    if rows.is_empty() {
        return "No results to display.".to_string();
    }

    Table::new(rows).to_string()
}
