//! Report generation — terminal tables, YAML, CSV.

pub mod terminal;
pub mod yaml;

use crate::runner::SceneResults;

/// Output format selector.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OutputFormat {
    Terminal,
    Yaml,
    Csv,
}

impl OutputFormat {
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "yaml" | "yml" => Self::Yaml,
            "csv" => Self::Csv,
            _ => Self::Terminal,
        }
    }
}

/// A benchmark report containing results from one or more scenes.
pub struct Report {
    pub results: Vec<SceneResults>,
}

impl Report {
    pub fn new(results: Vec<SceneResults>) -> Self {
        Self { results }
    }

    pub fn render(&self, format: &OutputFormat) -> String {
        match format {
            OutputFormat::Terminal => terminal::render(&self.results),
            OutputFormat::Yaml => yaml::render(&self.results),
            OutputFormat::Csv => csv_render(&self.results),
        }
    }
}

fn csv_render(results: &[SceneResults]) -> String {
    let mut out = String::from("scene,metric,min_s,max_s,mean_s,median_s,stddev_ms,p95_s,count\n");
    for scene in results {
        for m in &scene.metrics {
            out.push_str(&format!(
                "{},{},{:.6},{:.6},{:.6},{:.6},{:.2},{:.6},{}\n",
                scene.scene_name,
                m.metric_id,
                m.stats.min.as_secs_f64(),
                m.stats.max.as_secs_f64(),
                m.stats.mean.as_secs_f64(),
                m.stats.median.as_secs_f64(),
                m.stats.stddev_ms,
                m.stats.p95.as_secs_f64(),
                m.stats.count,
            ));
        }
    }
    out
}
