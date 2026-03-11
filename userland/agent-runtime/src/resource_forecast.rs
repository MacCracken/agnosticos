//! Resource Usage Forecasting
//!
//! Predicts future memory and CPU needs from trailing usage samples using
//! linear regression over a sliding window.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A single resource usage sample at a point in time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceSample {
    pub timestamp: DateTime<Utc>,
    pub cpu_percent: f64,
    pub memory_bytes: u64,
    pub fd_count: u32,
}

/// Supported resource metric types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MetricKind {
    Cpu,
    Memory,
}

impl MetricKind {
    /// Threshold for "stable" trend detection.
    pub fn stability_threshold(self) -> f64 {
        match self {
            MetricKind::Cpu => 0.001,    // % per second
            MetricKind::Memory => 100.0, // bytes per second
        }
    }
}

impl std::fmt::Display for MetricKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MetricKind::Cpu => write!(f, "cpu"),
            MetricKind::Memory => write!(f, "memory"),
        }
    }
}

impl std::str::FromStr for MetricKind {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "cpu" => Ok(Self::Cpu),
            "memory" => Ok(Self::Memory),
            other => Err(format!("unknown metric: {other}")),
        }
    }
}

/// Trend direction derived from the slope of a linear regression.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Trend {
    Rising,
    Stable,
    Falling,
}

/// An alert indicating a resource metric is predicted to exceed its limit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceAlert {
    pub metric: String,
    pub current: f64,
    pub predicted: f64,
    pub limit: f64,
    pub time_to_breach_minutes: Option<u64>,
}

/// Sliding window of resource samples with forecasting capabilities.
pub struct ForecastWindow {
    max_samples: usize,
    samples: Vec<ResourceSample>,
}

impl ForecastWindow {
    /// Create a new forecast window with the given maximum capacity.
    pub fn new(max_samples: usize) -> Self {
        Self {
            max_samples: max_samples.max(1),
            samples: Vec::new(),
        }
    }

    /// Add a sample to the window, evicting the oldest if at capacity.
    pub fn add_sample(&mut self, sample: ResourceSample) {
        if self.samples.len() >= self.max_samples {
            self.samples.remove(0);
        }
        self.samples.push(sample);
    }

    /// Forecast CPU percentage at `horizon_minutes` into the future.
    /// Returns `None` if fewer than 2 samples are available.
    pub fn forecast_cpu(&self, horizon_minutes: u64) -> Option<f64> {
        if self.samples.len() < 2 {
            return None;
        }
        let points = self.cpu_time_points();
        let (slope, intercept) = linear_regression(&points);
        let last_t = points.last().map(|(t, _)| *t).unwrap_or(0.0);
        let future_t = last_t + (horizon_minutes as f64) * 60.0;
        Some(slope * future_t + intercept)
    }

    /// Forecast memory bytes at `horizon_minutes` into the future.
    /// Returns `None` if fewer than 2 samples are available.
    pub fn forecast_memory(&self, horizon_minutes: u64) -> Option<u64> {
        if self.samples.len() < 2 {
            return None;
        }
        let points = self.memory_time_points();
        let (slope, intercept) = linear_regression(&points);
        let last_t = points.last().map(|(t, _)| *t).unwrap_or(0.0);
        let future_t = last_t + (horizon_minutes as f64) * 60.0;
        let predicted = slope * future_t + intercept;
        Some(predicted.max(0.0) as u64)
    }

    /// Determine the trend for a given metric.
    pub fn trend(&self, metric: MetricKind) -> Trend {
        if self.samples.len() < 2 {
            return Trend::Stable;
        }
        let points = match metric {
            MetricKind::Cpu => self.cpu_time_points(),
            MetricKind::Memory => self.memory_time_points(),
        };
        let (slope, _) = linear_regression(&points);
        let threshold = metric.stability_threshold();

        if slope > threshold {
            Trend::Rising
        } else if slope < -threshold {
            Trend::Falling
        } else {
            Trend::Stable
        }
    }

    /// Check if CPU or memory will exceed the given limits within
    /// `within_minutes`. Returns alerts for each metric that will breach.
    pub fn will_exceed(
        &self,
        cpu_limit: f64,
        memory_limit: u64,
        within_minutes: u64,
    ) -> Vec<ResourceAlert> {
        let mut alerts = Vec::new();

        if let Some(predicted_cpu) = self.forecast_cpu(within_minutes) {
            let current_cpu = self.samples.last().map(|s| s.cpu_percent).unwrap_or(0.0);
            if predicted_cpu > cpu_limit {
                let ttb = self.time_to_breach_cpu(cpu_limit);
                alerts.push(ResourceAlert {
                    metric: "cpu".to_string(),
                    current: current_cpu,
                    predicted: predicted_cpu,
                    limit: cpu_limit,
                    time_to_breach_minutes: ttb,
                });
            }
        }

        if let Some(predicted_mem) = self.forecast_memory(within_minutes) {
            let current_mem = self
                .samples
                .last()
                .map(|s| s.memory_bytes as f64)
                .unwrap_or(0.0);
            if predicted_mem > memory_limit {
                let ttb = self.time_to_breach_memory(memory_limit);
                alerts.push(ResourceAlert {
                    metric: "memory".to_string(),
                    current: current_mem,
                    predicted: predicted_mem as f64,
                    limit: memory_limit as f64,
                    time_to_breach_minutes: ttb,
                });
            }
        }

        alerts
    }

    // -- private helpers --

    fn cpu_time_points(&self) -> Vec<(f64, f64)> {
        self.time_points(|s| s.cpu_percent)
    }

    fn memory_time_points(&self) -> Vec<(f64, f64)> {
        self.time_points(|s| s.memory_bytes as f64)
    }

    fn time_points(&self, value_fn: impl Fn(&ResourceSample) -> f64) -> Vec<(f64, f64)> {
        if self.samples.is_empty() {
            return Vec::new();
        }
        let base = self.samples[0].timestamp;
        self.samples
            .iter()
            .map(|s| {
                let t = (s.timestamp - base).num_seconds() as f64;
                (t, value_fn(s))
            })
            .collect()
    }

    fn time_to_breach_cpu(&self, limit: f64) -> Option<u64> {
        if self.samples.len() < 2 {
            return None;
        }
        let points = self.cpu_time_points();
        let (slope, intercept) = linear_regression(&points);
        if slope <= 0.0 {
            return None;
        }
        let last_t = points.last().map(|(t, _)| *t).unwrap_or(0.0);
        let breach_t = (limit - intercept) / slope;
        let delta = breach_t - last_t;
        if delta > 0.0 {
            Some((delta / 60.0).ceil() as u64)
        } else {
            Some(0)
        }
    }

    fn time_to_breach_memory(&self, limit: u64) -> Option<u64> {
        if self.samples.len() < 2 {
            return None;
        }
        let points = self.memory_time_points();
        let (slope, intercept) = linear_regression(&points);
        if slope <= 0.0 {
            return None;
        }
        let last_t = points.last().map(|(t, _)| *t).unwrap_or(0.0);
        let breach_t = (limit as f64 - intercept) / slope;
        let delta = breach_t - last_t;
        if delta > 0.0 {
            Some((delta / 60.0).ceil() as u64)
        } else {
            Some(0)
        }
    }
}

/// Simple linear regression on (x, y) pairs.
/// Returns `(slope, intercept)`. Returns `(0.0, 0.0)` for empty input.
pub fn linear_regression(points: &[(f64, f64)]) -> (f64, f64) {
    let n = points.len() as f64;
    if n < 1.0 {
        return (0.0, 0.0);
    }
    if n < 2.0 {
        return (0.0, points[0].1);
    }

    let sum_x: f64 = points.iter().map(|(x, _)| x).sum();
    let sum_y: f64 = points.iter().map(|(_, y)| y).sum();
    let sum_xy: f64 = points.iter().map(|(x, y)| x * y).sum();
    let sum_x2: f64 = points.iter().map(|(x, _)| x * x).sum();

    let denom = n * sum_x2 - sum_x * sum_x;
    if denom.abs() < f64::EPSILON {
        return (0.0, sum_y / n);
    }

    let slope = (n * sum_xy - sum_x * sum_y) / denom;
    let intercept = (sum_y - slope * sum_x) / n;

    (slope, intercept)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn make_sample(offset_secs: i64, cpu: f64, mem: u64) -> ResourceSample {
        let base = Utc::now() - Duration::seconds(300);
        ResourceSample {
            timestamp: base + Duration::seconds(offset_secs),
            cpu_percent: cpu,
            memory_bytes: mem,
            fd_count: 10,
        }
    }

    // --- linear_regression tests ---

    #[test]
    fn test_linear_regression_empty() {
        let (slope, intercept) = linear_regression(&[]);
        assert_eq!(slope, 0.0);
        assert_eq!(intercept, 0.0);
    }

    #[test]
    fn test_linear_regression_single_point() {
        let (slope, intercept) = linear_regression(&[(1.0, 5.0)]);
        assert_eq!(slope, 0.0);
        assert_eq!(intercept, 5.0);
    }

    #[test]
    fn test_linear_regression_perfect_line() {
        // y = 2x + 1
        let points = vec![(0.0, 1.0), (1.0, 3.0), (2.0, 5.0), (3.0, 7.0)];
        let (slope, intercept) = linear_regression(&points);
        assert!((slope - 2.0).abs() < 1e-9);
        assert!((intercept - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_linear_regression_negative_slope() {
        // y = -1x + 10
        let points = vec![(0.0, 10.0), (5.0, 5.0), (10.0, 0.0)];
        let (slope, intercept) = linear_regression(&points);
        assert!((slope - (-1.0)).abs() < 1e-9);
        assert!((intercept - 10.0).abs() < 1e-9);
    }

    #[test]
    fn test_linear_regression_flat() {
        let points = vec![(0.0, 5.0), (1.0, 5.0), (2.0, 5.0)];
        let (slope, intercept) = linear_regression(&points);
        assert!(slope.abs() < 1e-9);
        assert!((intercept - 5.0).abs() < 1e-9);
    }

    #[test]
    fn test_linear_regression_same_x() {
        // All x the same — degenerate case
        let points = vec![(1.0, 2.0), (1.0, 4.0), (1.0, 6.0)];
        let (slope, intercept) = linear_regression(&points);
        assert_eq!(slope, 0.0);
        assert!((intercept - 4.0).abs() < 1e-9); // average y
    }

    // --- ForecastWindow tests ---

    #[test]
    fn test_forecast_window_new() {
        let fw = ForecastWindow::new(100);
        assert_eq!(fw.max_samples, 100);
        assert!(fw.samples.is_empty());
    }

    #[test]
    fn test_forecast_window_add_sample_eviction() {
        let mut fw = ForecastWindow::new(3);
        for i in 0..5 {
            fw.add_sample(make_sample(i * 60, 10.0, 1000));
        }
        assert_eq!(fw.samples.len(), 3);
    }

    #[test]
    fn test_forecast_cpu_insufficient_data() {
        let mut fw = ForecastWindow::new(10);
        fw.add_sample(make_sample(0, 50.0, 1000));
        assert!(fw.forecast_cpu(10).is_none());
    }

    #[test]
    fn test_forecast_cpu_rising() {
        let mut fw = ForecastWindow::new(100);
        // CPU rising: 10%, 20%, 30% at 0, 60, 120 seconds
        fw.add_sample(make_sample(0, 10.0, 1000));
        fw.add_sample(make_sample(60, 20.0, 1000));
        fw.add_sample(make_sample(120, 30.0, 1000));

        let predicted = fw.forecast_cpu(1).unwrap(); // 1 minute ahead
                                                     // Slope is ~0.1667 %/s, so at 180s: ~40%
        assert!(predicted > 30.0, "predicted {} should be > 30", predicted);
    }

    #[test]
    fn test_forecast_memory_rising() {
        let mut fw = ForecastWindow::new(100);
        fw.add_sample(make_sample(0, 10.0, 1_000_000));
        fw.add_sample(make_sample(60, 10.0, 2_000_000));
        fw.add_sample(make_sample(120, 10.0, 3_000_000));

        let predicted = fw.forecast_memory(1).unwrap();
        assert!(
            predicted > 3_000_000,
            "predicted {} should be > 3M",
            predicted
        );
    }

    #[test]
    fn test_forecast_memory_insufficient_data() {
        let fw = ForecastWindow::new(10);
        assert!(fw.forecast_memory(10).is_none());
    }

    #[test]
    fn test_trend_rising() {
        let mut fw = ForecastWindow::new(100);
        fw.add_sample(make_sample(0, 10.0, 1000));
        fw.add_sample(make_sample(60, 30.0, 1000));
        fw.add_sample(make_sample(120, 50.0, 1000));
        assert_eq!(fw.trend(MetricKind::Cpu), Trend::Rising);
    }

    #[test]
    fn test_trend_falling() {
        let mut fw = ForecastWindow::new(100);
        fw.add_sample(make_sample(0, 80.0, 1000));
        fw.add_sample(make_sample(60, 50.0, 1000));
        fw.add_sample(make_sample(120, 20.0, 1000));
        assert_eq!(fw.trend(MetricKind::Cpu), Trend::Falling);
    }

    #[test]
    fn test_trend_stable() {
        let mut fw = ForecastWindow::new(100);
        fw.add_sample(make_sample(0, 50.0, 1000));
        fw.add_sample(make_sample(60, 50.0, 1000));
        fw.add_sample(make_sample(120, 50.0, 1000));
        assert_eq!(fw.trend(MetricKind::Cpu), Trend::Stable);
    }

    #[test]
    fn test_trend_insufficient_data() {
        let mut fw = ForecastWindow::new(100);
        fw.add_sample(make_sample(0, 50.0, 1000));
        assert_eq!(fw.trend(MetricKind::Cpu), Trend::Stable);
    }

    #[test]
    fn test_trend_memory_rising() {
        let mut fw = ForecastWindow::new(100);
        fw.add_sample(make_sample(0, 10.0, 100_000));
        fw.add_sample(make_sample(60, 10.0, 500_000));
        fw.add_sample(make_sample(120, 10.0, 900_000));
        assert_eq!(fw.trend(MetricKind::Memory), Trend::Rising);
    }

    #[test]
    fn test_will_exceed_no_breach() {
        let mut fw = ForecastWindow::new(100);
        fw.add_sample(make_sample(0, 10.0, 1000));
        fw.add_sample(make_sample(60, 10.0, 1000));
        fw.add_sample(make_sample(120, 10.0, 1000));

        let alerts = fw.will_exceed(90.0, 1_000_000_000, 60);
        assert!(alerts.is_empty());
    }

    #[test]
    fn test_will_exceed_cpu_breach() {
        let mut fw = ForecastWindow::new(100);
        fw.add_sample(make_sample(0, 50.0, 1000));
        fw.add_sample(make_sample(60, 70.0, 1000));
        fw.add_sample(make_sample(120, 90.0, 1000));

        let alerts = fw.will_exceed(95.0, u64::MAX, 10);
        assert!(!alerts.is_empty());
        assert_eq!(alerts[0].metric, "cpu");
        assert!(alerts[0].predicted > 95.0);
    }

    #[test]
    fn test_will_exceed_memory_breach() {
        let mut fw = ForecastWindow::new(100);
        fw.add_sample(make_sample(0, 10.0, 500_000_000));
        fw.add_sample(make_sample(60, 10.0, 700_000_000));
        fw.add_sample(make_sample(120, 10.0, 900_000_000));

        let alerts = fw.will_exceed(100.0, 1_000_000_000, 10);
        assert!(alerts.iter().any(|a| a.metric == "memory"));
    }

    #[test]
    fn test_will_exceed_insufficient_data() {
        let fw = ForecastWindow::new(100);
        let alerts = fw.will_exceed(90.0, 1_000_000_000, 60);
        assert!(alerts.is_empty());
    }

    #[test]
    fn test_resource_alert_has_time_to_breach() {
        let mut fw = ForecastWindow::new(100);
        fw.add_sample(make_sample(0, 50.0, 1000));
        fw.add_sample(make_sample(60, 70.0, 1000));
        fw.add_sample(make_sample(120, 90.0, 1000));

        let alerts = fw.will_exceed(95.0, u64::MAX, 10);
        if !alerts.is_empty() {
            assert!(alerts[0].time_to_breach_minutes.is_some());
        }
    }

    #[test]
    fn test_resource_sample_serialization() {
        let sample = make_sample(0, 42.5, 12345);
        let json = serde_json::to_string(&sample).unwrap();
        let deser: ResourceSample = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.cpu_percent, 42.5);
        assert_eq!(deser.memory_bytes, 12345);
    }

    #[test]
    fn test_trend_serialization() {
        for t in [Trend::Rising, Trend::Stable, Trend::Falling] {
            let json = serde_json::to_string(&t).unwrap();
            let deser: Trend = serde_json::from_str(&json).unwrap();
            assert_eq!(t, deser);
        }
    }

    #[test]
    fn test_forecast_window_min_capacity() {
        let fw = ForecastWindow::new(0);
        assert_eq!(fw.max_samples, 1);
    }

    #[test]
    fn test_forecast_memory_non_negative() {
        let mut fw = ForecastWindow::new(100);
        // Sharply falling memory — prediction should clamp to 0
        fw.add_sample(make_sample(0, 10.0, 1_000_000));
        fw.add_sample(make_sample(60, 10.0, 500_000));
        fw.add_sample(make_sample(120, 10.0, 100));

        let predicted = fw.forecast_memory(100).unwrap();
        // Should not underflow (returns u64)
        let _ = predicted; // Just ensure no panic
    }
}
