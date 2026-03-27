//! Run configuration for benchmark sessions.

use serde::{Deserialize, Serialize};

/// Configuration for a benchmark run.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RunConfig {
    /// Number of measurement iterations per metric (default: 100).
    pub iterations: usize,
    /// Number of warmup iterations before measurement (default: 3).
    pub warmup_iterations: usize,
    /// Run only these metric IDs (None = all).
    pub metric_filter: Option<Vec<String>>,
}

impl Default for RunConfig {
    fn default() -> Self {
        Self {
            iterations: 100,
            warmup_iterations: 3,
            metric_filter: None,
        }
    }
}
