//! Statistical aggregation for benchmark measurements.

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Aggregated statistics from a series of duration measurements.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Stats {
    pub min: Duration,
    pub max: Duration,
    pub mean: Duration,
    pub median: Duration,
    pub stddev_ms: f64,
    pub p95: Duration,
    pub count: usize,
}

impl Stats {
    /// Compute statistics from a slice of durations.
    ///
    /// # Panics
    ///
    /// Panics if `durations` is empty.
    pub fn from_durations(durations: &[Duration]) -> Self {
        assert!(
            !durations.is_empty(),
            "cannot compute stats from empty slice"
        );

        let mut sorted: Vec<Duration> = durations.to_vec();
        sorted.sort();

        let count = sorted.len();
        let min = sorted[0];
        let max = sorted[count - 1];

        let total_ns: u128 = sorted.iter().map(|d| d.as_nanos()).sum();
        let mean_ns = total_ns / count as u128;
        let mean = Duration::from_nanos(u64::try_from(mean_ns).expect("mean duration overflow"));

        #[allow(clippy::manual_is_multiple_of)]
        let median = if count % 2 == 0 {
            let a = sorted[count / 2 - 1].as_nanos();
            let b = sorted[count / 2].as_nanos();
            Duration::from_nanos(u64::try_from((a + b) / 2).expect("median duration overflow"))
        } else {
            sorted[count / 2]
        };

        // Nearest-rank percentile method
        let p95_idx = ((count as f64) * 0.95).ceil() as usize - 1;
        let p95 = sorted[p95_idx.min(count - 1)];

        // Sample standard deviation (N-1 denominator) in milliseconds.
        // Falls back to population stddev for N=1.
        let mean_ms = mean.as_secs_f64() * 1000.0;
        let denom = if count > 1 { count - 1 } else { 1 };
        let variance: f64 = sorted
            .iter()
            .map(|d| {
                let ms = d.as_secs_f64() * 1000.0;
                (ms - mean_ms).powi(2)
            })
            .sum::<f64>()
            / denom as f64;
        let stddev_ms = variance.sqrt();

        Self {
            min,
            max,
            mean,
            median,
            stddev_ms,
            p95,
            count,
        }
    }
}

impl std::fmt::Display for Stats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "min={:.4}s max={:.4}s mean={:.4}s median={:.4}s stddev={:.2}ms (n={})",
            self.min.as_secs_f64(),
            self.max.as_secs_f64(),
            self.mean.as_secs_f64(),
            self.median.as_secs_f64(),
            self.stddev_ms,
            self.count,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stats_single() {
        let d = vec![Duration::from_millis(100)];
        let s = Stats::from_durations(&d);
        assert_eq!(s.count, 1);
        assert_eq!(s.min, s.max);
        assert_eq!(s.min, s.mean);
    }

    #[test]
    fn test_stats_basic() {
        let d: Vec<Duration> = (1..=10).map(|i| Duration::from_millis(i * 10)).collect();
        let s = Stats::from_durations(&d);
        assert_eq!(s.count, 10);
        assert_eq!(s.min, Duration::from_millis(10));
        assert_eq!(s.max, Duration::from_millis(100));
        // mean of 10..100 step 10 = 55ms
        assert_eq!(s.mean, Duration::from_millis(55));
        // median of 10,20,30,40,50,60,70,80,90,100 = avg(50,60) = 55
        assert_eq!(s.median, Duration::from_millis(55));
    }

    #[test]
    fn test_stats_p95() {
        let d: Vec<Duration> = (1..=100).map(|i| Duration::from_millis(i)).collect();
        let s = Stats::from_durations(&d);
        assert_eq!(s.p95, Duration::from_millis(95));
    }

    #[test]
    fn test_stats_odd_count() {
        let d = vec![
            Duration::from_millis(10),
            Duration::from_millis(20),
            Duration::from_millis(30),
        ];
        let s = Stats::from_durations(&d);
        assert_eq!(s.median, Duration::from_millis(20));
    }

    #[test]
    #[should_panic(expected = "cannot compute stats from empty slice")]
    fn test_stats_empty_panics() {
        Stats::from_durations(&[]);
    }
}
