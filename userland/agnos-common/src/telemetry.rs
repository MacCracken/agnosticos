//! Telemetry and crash reporting system
//!
//! Provides opt-in metrics collection and crash reporting for AGNOS.
//! All telemetry is anonymous and requires explicit user consent.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Telemetry configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryConfig {
    /// Whether telemetry is enabled
    pub enabled: bool,
    /// Whether crash reporting is enabled
    pub crash_reporting: bool,
    /// Whether metrics collection is enabled
    pub metrics_enabled: bool,
    /// Unique anonymous instance ID
    pub instance_id: String,
    /// Telemetry endpoint URL
    pub endpoint_url: String,
    /// Sampling rate (0.0 - 1.0)
    pub sampling_rate: f32,
    /// Metrics flush interval in seconds
    pub flush_interval_secs: u64,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            enabled: false, // Opt-in - disabled by default
            crash_reporting: false,
            metrics_enabled: false,
            instance_id: generate_instance_id(),
            endpoint_url: "https://telemetry.agnos.org/v1".to_string(),
            sampling_rate: 1.0,
            flush_interval_secs: 300, // 5 minutes
        }
    }
}

/// Generate anonymous instance ID
fn generate_instance_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Crash report data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrashReport {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub instance_id: String,
    pub version: String,
    pub component: String,
    pub error_message: String,
    pub stack_trace: Option<String>,
    pub system_info: SystemInfo,
}

/// System information for crash reports
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfo {
    pub os_type: String,
    pub os_version: String,
    pub architecture: String,
    pub cpu_count: usize,
    pub memory_mb: u64,
    pub kernel_version: String,
}

/// Telemetry event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryEvent {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub instance_id: String,
    pub event_type: EventType,
    pub category: String,
    pub name: String,
    pub value: f64,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventType {
    Counter,
    Gauge,
    Histogram,
    Timing,
}

/// Telemetry session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetrySession {
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub instance_id: String,
    pub version: String,
    pub events_sent: u64,
    pub events_dropped: u64,
}

/// Maximum number of telemetry events kept in memory.
const MAX_EVENTS: usize = 1000;
/// Maximum number of crash reports kept in memory.
const MAX_CRASH_REPORTS: usize = 10;

/// Main telemetry collector
pub struct TelemetryCollector {
    config: TelemetryConfig,
    /// Cached instance_id for cheap cloning in hot path
    instance_id: Arc<str>,
    session: Arc<RwLock<TelemetrySession>>,
    events: Arc<RwLock<VecDeque<TelemetryEvent>>>,
    crash_reports: Arc<RwLock<VecDeque<CrashReport>>>,
}

impl TelemetryCollector {
    /// Create new telemetry collector
    pub fn new(config: TelemetryConfig) -> Self {
        let session = TelemetrySession {
            started_at: chrono::Utc::now(),
            instance_id: config.instance_id.clone(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            events_sent: 0,
            events_dropped: 0,
        };

        let instance_id: Arc<str> = config.instance_id.as_str().into();
        Self {
            config,
            instance_id,
            session: Arc::new(RwLock::new(session)),
            events: Arc::new(RwLock::new(VecDeque::new())),
            crash_reports: Arc::new(RwLock::new(VecDeque::new())),
        }
    }

    /// Check if telemetry is enabled
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Record a telemetry event
    pub async fn record_event(
        &self,
        category: &str,
        name: &str,
        value: f64,
        event_type: EventType,
    ) {
        if !self.config.enabled || !self.config.metrics_enabled {
            return;
        }

        // Sampling
        if self.config.sampling_rate < 1.0 {
            let random_val = rand::random::<f32>();
            if random_val > self.config.sampling_rate {
                return;
            }
        }

        let event = TelemetryEvent {
            timestamp: chrono::Utc::now(),
            instance_id: (*self.instance_id).to_string(),
            event_type,
            category: category.to_string(),
            name: name.to_string(),
            value,
            metadata: HashMap::new(),
        };

        let mut events = self.events.write().await;
        events.push_back(event);

        // Limit in-memory events — O(1) with VecDeque
        if events.len() > MAX_EVENTS {
            events.pop_front();
        }
    }

    /// Record a counter event
    pub async fn record_counter(&self, category: &str, name: &str, value: f64) {
        self.record_event(category, name, value, EventType::Counter)
            .await;
    }

    /// Record a gauge event
    pub async fn record_gauge(&self, category: &str, name: &str, value: f64) {
        self.record_event(category, name, value, EventType::Gauge)
            .await;
    }

    /// Record a timing event
    pub async fn record_timing(&self, category: &str, name: &str, milliseconds: f64) {
        self.record_event(category, name, milliseconds, EventType::Timing)
            .await;
    }

    /// Submit a crash report
    pub async fn submit_crash(&self, component: &str, error: &str, stack_trace: Option<&str>) {
        if !self.config.enabled || !self.config.crash_reporting {
            return;
        }

        let system_info = SystemInfo {
            os_type: std::env::consts::OS.to_string(),
            os_version: Self::read_os_version(),
            architecture: std::env::consts::ARCH.to_string(),
            cpu_count: num_cpus::get(),
            memory_mb: Self::read_memory_mb(),
            kernel_version: Self::read_kernel_version(),
        };

        let report = CrashReport {
            timestamp: chrono::Utc::now(),
            instance_id: self.config.instance_id.clone(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            component: component.to_string(),
            error_message: error.to_string(),
            stack_trace: stack_trace.map(|s| s.to_string()),
            system_info,
        };

        let mut reports = self.crash_reports.write().await;
        reports.push_back(report);

        // Keep only last crash reports in memory — O(1) with VecDeque
        if reports.len() > MAX_CRASH_REPORTS {
            reports.pop_front();
        }
    }

    /// Flush collected telemetry to endpoint
    pub async fn flush(&self) -> Result<(), TelemetryError> {
        if !self.config.enabled {
            return Ok(());
        }

        // Drain all queued events
        let events_to_send: Vec<_> = {
            let mut events = self.events.write().await;
            events.drain(..).collect()
        };

        if events_to_send.is_empty() {
            return Ok(());
        }

        // Send to endpoint (shared client avoids per-flush connection overhead)
        static TELEMETRY_CLIENT: once_cell::sync::Lazy<reqwest::Client> =
            once_cell::sync::Lazy::new(|| {
                reqwest::Client::builder()
                    .timeout(std::time::Duration::from_secs(30))
                    .pool_max_idle_per_host(2)
                    .build()
                    .unwrap_or_else(|e| {
                        tracing::error!("Failed to build telemetry client: {}, using default", e);
                        reqwest::Client::new()
                    })
            });
        let payload = serde_json::json!({
            "instance_id": self.config.instance_id,
            "events": events_to_send,
        });

        match TELEMETRY_CLIENT
            .post(&self.config.endpoint_url)
            .json(&payload)
            .send()
            .await
        {
            Ok(response) => {
                if response.status().is_success() {
                    let mut session = self.session.write().await;
                    session.events_sent += events_to_send.len() as u64;
                    Ok(())
                } else {
                    Err(TelemetryError::EndpointError(format!(
                        "HTTP {}",
                        response.status()
                    )))
                }
            }
            Err(e) => Err(TelemetryError::NetworkError(e.to_string())),
        }
    }

    /// Get current session statistics
    pub async fn get_stats(&self) -> TelemetrySession {
        self.session.read().await.clone()
    }

    /// Read OS version from /etc/os-release (Linux) or fallback
    fn read_os_version() -> String {
        std::fs::read_to_string("/etc/os-release")
            .ok()
            .and_then(|content| {
                content
                    .lines()
                    .find(|l| l.starts_with("PRETTY_NAME="))
                    .map(|l| {
                        l.trim_start_matches("PRETTY_NAME=")
                            .trim_matches('"')
                            .to_string()
                    })
            })
            .unwrap_or_else(|| std::env::consts::OS.to_string())
    }

    /// Read total physical memory from /proc/meminfo (Linux)
    fn read_memory_mb() -> u64 {
        std::fs::read_to_string("/proc/meminfo")
            .ok()
            .and_then(|content| {
                content
                    .lines()
                    .find(|l| l.starts_with("MemTotal:"))
                    .and_then(|l| {
                        l.split_whitespace()
                            .nth(1)
                            .and_then(|kb| kb.parse::<u64>().ok())
                            .map(|kb| kb / 1024) // Convert kB to MB
                    })
            })
            .unwrap_or(0)
    }

    /// Read kernel version from /proc/version or uname
    fn read_kernel_version() -> String {
        std::fs::read_to_string("/proc/version")
            .ok()
            .and_then(|content| {
                // /proc/version format: "Linux version X.Y.Z ..."
                content.split_whitespace().nth(2).map(|s| s.to_string())
            })
            .unwrap_or_else(|| "unknown".to_string())
    }
}

/// Telemetry errors
#[derive(Debug, thiserror::Error)]
pub enum TelemetryError {
    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Endpoint error: {0}")]
    EndpointError(String),

    #[error("Serialization error: {0}")]
    Serialization(String),
}

/// Global telemetry instance (optional)
static GLOBAL_TELEMETRY: once_cell::sync::OnceCell<TelemetryCollector> =
    once_cell::sync::OnceCell::new();

/// Initialize global telemetry
pub fn init_telemetry(config: TelemetryConfig) {
    let _ = GLOBAL_TELEMETRY.set(TelemetryCollector::new(config));
}

/// Get global telemetry instance
pub fn global_telemetry() -> Option<&'static TelemetryCollector> {
    GLOBAL_TELEMETRY.get()
}

/// Convenience macro for recording events
#[macro_export]
macro_rules! telemetry_counter {
    ($category:expr, $name:expr, $value:expr) => {
        if let Some(telemetry) = $crate::telemetry::global_telemetry() {
            tokio::spawn(async move {
                telemetry.record_counter($category, $name, $value).await;
            });
        }
    };
}

/// Convenience macro for recording timing
#[macro_export]
macro_rules! telemetry_timing {
    ($category:expr, $name:expr, $duration:expr) => {
        if let Some(telemetry) = $crate::telemetry::global_telemetry() {
            tokio::spawn(async move {
                telemetry.record_timing($category, $name, $duration).await;
            });
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_telemetry_config_default() {
        let config = TelemetryConfig::default();
        assert!(!config.enabled); // Should be opt-in (disabled by default)
        assert!(!config.crash_reporting);
        assert!(!config.metrics_enabled);
        assert!(!config.instance_id.is_empty());
        assert_eq!(config.sampling_rate, 1.0);
    }

    #[test]
    fn test_telemetry_config_custom() {
        let config = TelemetryConfig {
            enabled: true,
            crash_reporting: true,
            metrics_enabled: true,
            instance_id: "test-id".to_string(),
            endpoint_url: "https://test.example.com".to_string(),
            sampling_rate: 0.5,
            flush_interval_secs: 60,
        };

        assert!(config.enabled);
        assert!(config.crash_reporting);
        assert!(config.metrics_enabled);
        assert_eq!(config.instance_id, "test-id");
    }

    #[test]
    fn test_system_info_creation() {
        let info = SystemInfo {
            os_type: "linux".to_string(),
            os_version: "6.1.0".to_string(),
            architecture: "x86_64".to_string(),
            cpu_count: 8,
            memory_mb: 16384,
            kernel_version: "6.1.0-agnos".to_string(),
        };

        assert_eq!(info.os_type, "linux");
        assert_eq!(info.cpu_count, 8);
        assert_eq!(info.memory_mb, 16384);
    }

    #[test]
    fn test_event_type_variants() {
        assert!(matches!(EventType::Counter, EventType::Counter));
        assert!(matches!(EventType::Gauge, EventType::Gauge));
        assert!(matches!(EventType::Histogram, EventType::Histogram));
        assert!(matches!(EventType::Timing, EventType::Timing));
    }

    #[tokio::test]
    async fn test_telemetry_collector_disabled() {
        let config = TelemetryConfig::default();
        let collector = TelemetryCollector::new(config);

        assert!(!collector.is_enabled());

        // Should not panic when recording events while disabled
        collector.record_counter("test", "counter", 1.0).await;
    }

    #[tokio::test]
    async fn test_telemetry_collector_enabled() {
        let config = TelemetryConfig {
            enabled: true,
            metrics_enabled: true,
            ..Default::default()
        };
        let collector = TelemetryCollector::new(config);

        assert!(collector.is_enabled());

        // Record some events
        collector.record_counter("test", "event1", 1.0).await;
        collector.record_gauge("test", "event2", 42.0).await;
        collector.record_timing("test", "event3", 100.0).await;
    }

    #[tokio::test]
    async fn test_crash_reporting_disabled() {
        let config = TelemetryConfig::default();
        let collector = TelemetryCollector::new(config);

        // Should not panic when crash reporting is disabled
        collector
            .submit_crash("test-component", "test error", None)
            .await;
    }

    #[tokio::test]
    async fn test_telemetry_stats() {
        let config = TelemetryConfig {
            enabled: true,
            metrics_enabled: true,
            ..Default::default()
        };
        let collector = TelemetryCollector::new(config);

        let stats = collector.get_stats().await;
        assert!(!stats.instance_id.is_empty());
        assert_eq!(stats.events_sent, 0);
    }

    #[test]
    fn test_generate_instance_id() {
        let id1 = generate_instance_id();
        let id2 = generate_instance_id();

        // IDs should be unique
        assert_ne!(id1, id2);

        // Should be valid UUID format
        assert_eq!(id1.len(), 36);
        assert!(id1.contains('-'));
    }

    #[test]
    fn test_telemetry_session_serialization() {
        let session = TelemetrySession {
            started_at: chrono::Utc::now(),
            instance_id: "test-id".to_string(),
            version: "1.0.0".to_string(),
            events_sent: 100,
            events_dropped: 5,
        };

        let json = serde_json::to_string(&session).expect("Failed to serialize");
        assert!(json.contains("test-id"));
        assert!(json.contains("1.0.0"));
    }

    // --- Additional telemetry.rs coverage tests ---

    #[tokio::test]
    async fn test_collector_construction_sets_session() {
        let config = TelemetryConfig {
            enabled: true,
            metrics_enabled: true,
            instance_id: "construct-test".to_string(),
            ..Default::default()
        };
        let collector = TelemetryCollector::new(config);
        let stats = collector.get_stats().await;
        assert_eq!(stats.instance_id, "construct-test");
        assert_eq!(stats.events_sent, 0);
        assert_eq!(stats.events_dropped, 0);
    }

    #[tokio::test]
    async fn test_record_event_disabled_telemetry() {
        let config = TelemetryConfig {
            enabled: false,
            metrics_enabled: true,
            ..Default::default()
        };
        let collector = TelemetryCollector::new(config);
        // Recording should silently skip when disabled
        collector
            .record_event("cat", "name", 1.0, EventType::Counter)
            .await;
        let events = collector.events.read().await;
        assert!(events.is_empty());
    }

    #[tokio::test]
    async fn test_record_event_disabled_metrics() {
        let config = TelemetryConfig {
            enabled: true,
            metrics_enabled: false,
            ..Default::default()
        };
        let collector = TelemetryCollector::new(config);
        collector
            .record_event("cat", "name", 1.0, EventType::Counter)
            .await;
        let events = collector.events.read().await;
        assert!(events.is_empty());
    }

    #[tokio::test]
    async fn test_record_event_stores_correctly() {
        let config = TelemetryConfig {
            enabled: true,
            metrics_enabled: true,
            instance_id: "event-store-test".to_string(),
            ..Default::default()
        };
        let collector = TelemetryCollector::new(config);
        collector
            .record_event("perf", "latency", 42.5, EventType::Gauge)
            .await;

        let events = collector.events.read().await;
        assert_eq!(events.len(), 1);
        let event = &events[0];
        assert_eq!(event.category, "perf");
        assert_eq!(event.name, "latency");
        assert_eq!(event.value, 42.5);
        assert_eq!(event.event_type, EventType::Gauge);
        assert_eq!(event.instance_id, "event-store-test");
    }

    #[tokio::test]
    async fn test_record_counter_stores_counter_type() {
        let config = TelemetryConfig {
            enabled: true,
            metrics_enabled: true,
            ..Default::default()
        };
        let collector = TelemetryCollector::new(config);
        collector.record_counter("http", "requests", 1.0).await;

        let events = collector.events.read().await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, EventType::Counter);
    }

    #[tokio::test]
    async fn test_record_gauge_stores_gauge_type() {
        let config = TelemetryConfig {
            enabled: true,
            metrics_enabled: true,
            ..Default::default()
        };
        let collector = TelemetryCollector::new(config);
        collector.record_gauge("system", "cpu_pct", 85.0).await;

        let events = collector.events.read().await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, EventType::Gauge);
        assert_eq!(events[0].value, 85.0);
    }

    #[tokio::test]
    async fn test_record_timing_stores_timing_type() {
        let config = TelemetryConfig {
            enabled: true,
            metrics_enabled: true,
            ..Default::default()
        };
        let collector = TelemetryCollector::new(config);
        collector.record_timing("db", "query_ms", 123.456).await;

        let events = collector.events.read().await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, EventType::Timing);
        assert_eq!(events[0].value, 123.456);
    }

    #[tokio::test]
    async fn test_vecdeque_eviction_at_max_events() {
        let config = TelemetryConfig {
            enabled: true,
            metrics_enabled: true,
            sampling_rate: 1.0,
            ..Default::default()
        };
        let collector = TelemetryCollector::new(config);

        // Fill beyond MAX_EVENTS (1000)
        for i in 0..1005 {
            collector.record_counter("test", "counter", i as f64).await;
        }

        let events = collector.events.read().await;
        // Should be capped at MAX_EVENTS
        assert_eq!(events.len(), MAX_EVENTS);
        // The oldest events (0..5) should have been evicted; first remaining = 5.0
        assert_eq!(events[0].value, 5.0);
    }

    #[tokio::test]
    async fn test_submit_crash_disabled() {
        let config = TelemetryConfig {
            enabled: false,
            crash_reporting: true,
            ..Default::default()
        };
        let collector = TelemetryCollector::new(config);
        collector
            .submit_crash("comp", "error msg", Some("trace"))
            .await;
        let reports = collector.crash_reports.read().await;
        assert!(reports.is_empty());
    }

    #[tokio::test]
    async fn test_submit_crash_reporting_disabled() {
        let config = TelemetryConfig {
            enabled: true,
            crash_reporting: false,
            ..Default::default()
        };
        let collector = TelemetryCollector::new(config);
        collector.submit_crash("comp", "error msg", None).await;
        let reports = collector.crash_reports.read().await;
        assert!(reports.is_empty());
    }

    #[tokio::test]
    async fn test_submit_crash_enabled() {
        let config = TelemetryConfig {
            enabled: true,
            crash_reporting: true,
            instance_id: "crash-test".to_string(),
            ..Default::default()
        };
        let collector = TelemetryCollector::new(config);
        collector
            .submit_crash("agent-runtime", "panic at the disco", Some("line 42"))
            .await;

        let reports = collector.crash_reports.read().await;
        assert_eq!(reports.len(), 1);
        let report = &reports[0];
        assert_eq!(report.component, "agent-runtime");
        assert_eq!(report.error_message, "panic at the disco");
        assert_eq!(report.stack_trace.as_deref(), Some("line 42"));
        assert_eq!(report.instance_id, "crash-test");
    }

    #[tokio::test]
    async fn test_submit_crash_no_stack_trace() {
        let config = TelemetryConfig {
            enabled: true,
            crash_reporting: true,
            ..Default::default()
        };
        let collector = TelemetryCollector::new(config);
        collector
            .submit_crash("llm-gateway", "connection lost", None)
            .await;

        let reports = collector.crash_reports.read().await;
        assert_eq!(reports.len(), 1);
        assert!(reports[0].stack_trace.is_none());
    }

    #[tokio::test]
    async fn test_crash_report_eviction_at_max() {
        let config = TelemetryConfig {
            enabled: true,
            crash_reporting: true,
            ..Default::default()
        };
        let collector = TelemetryCollector::new(config);

        // Submit more than MAX_CRASH_REPORTS (10)
        for i in 0..15 {
            collector
                .submit_crash("comp", &format!("error {}", i), None)
                .await;
        }

        let reports = collector.crash_reports.read().await;
        assert_eq!(reports.len(), MAX_CRASH_REPORTS);
        // Oldest should be evicted; first remaining should be "error 5"
        assert_eq!(reports[0].error_message, "error 5");
    }

    #[tokio::test]
    async fn test_flush_disabled() {
        let config = TelemetryConfig::default(); // disabled
        let collector = TelemetryCollector::new(config);
        let result = collector.flush().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_flush_empty_events() {
        let config = TelemetryConfig {
            enabled: true,
            metrics_enabled: true,
            ..Default::default()
        };
        let collector = TelemetryCollector::new(config);
        // No events recorded — flush should succeed (no-op)
        let result = collector.flush().await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_read_os_version_returns_string() {
        // Pure function — just verify it returns something non-panicking
        let version = TelemetryCollector::read_os_version();
        assert!(!version.is_empty());
    }

    #[test]
    fn test_read_memory_mb_returns_value() {
        let mem = TelemetryCollector::read_memory_mb();
        // On Linux this reads /proc/meminfo, on other platforms returns 0
        // Just verify no panic
        let _ = mem;
    }

    #[test]
    fn test_read_kernel_version_returns_string() {
        let version = TelemetryCollector::read_kernel_version();
        assert!(!version.is_empty());
    }

    #[test]
    fn test_telemetry_config_serialization_roundtrip() {
        let config = TelemetryConfig {
            enabled: true,
            crash_reporting: true,
            metrics_enabled: true,
            instance_id: "serial-test".to_string(),
            endpoint_url: "https://test.example.com/v1".to_string(),
            sampling_rate: 0.75,
            flush_interval_secs: 120,
        };
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: TelemetryConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.instance_id, "serial-test");
        assert_eq!(deserialized.sampling_rate, 0.75);
        assert_eq!(deserialized.flush_interval_secs, 120);
    }

    #[test]
    fn test_crash_report_serialization() {
        let report = CrashReport {
            timestamp: chrono::Utc::now(),
            instance_id: "crash-serial".to_string(),
            version: "0.1.0".to_string(),
            component: "desktop".to_string(),
            error_message: "segfault".to_string(),
            stack_trace: Some("at main.rs:10".to_string()),
            system_info: SystemInfo {
                os_type: "linux".to_string(),
                os_version: "Arch".to_string(),
                architecture: "x86_64".to_string(),
                cpu_count: 4,
                memory_mb: 8192,
                kernel_version: "6.6.0".to_string(),
            },
        };
        let json = serde_json::to_string(&report).unwrap();
        let deserialized: CrashReport = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.component, "desktop");
        assert_eq!(deserialized.system_info.cpu_count, 4);
    }

    #[test]
    fn test_telemetry_event_serialization() {
        let event = TelemetryEvent {
            timestamp: chrono::Utc::now(),
            instance_id: "evt-serial".to_string(),
            event_type: EventType::Histogram,
            category: "perf".to_string(),
            name: "request_size".to_string(),
            value: 1024.0,
            metadata: HashMap::from([("unit".to_string(), "bytes".to_string())]),
        };
        let json = serde_json::to_string(&event).unwrap();
        let deserialized: TelemetryEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.event_type, EventType::Histogram);
        assert_eq!(deserialized.value, 1024.0);
        assert_eq!(deserialized.metadata.get("unit").unwrap(), "bytes");
    }

    #[test]
    fn test_event_type_equality() {
        assert_eq!(EventType::Counter, EventType::Counter);
        assert_ne!(EventType::Counter, EventType::Gauge);
        assert_ne!(EventType::Histogram, EventType::Timing);
    }

    #[test]
    fn test_system_info_serialization_roundtrip() {
        let info = SystemInfo {
            os_type: "linux".to_string(),
            os_version: "AGNOS 0.1".to_string(),
            architecture: "aarch64".to_string(),
            cpu_count: 16,
            memory_mb: 32768,
            kernel_version: "6.6.0-agnos".to_string(),
        };
        let json = serde_json::to_string(&info).unwrap();
        let deserialized: SystemInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.architecture, "aarch64");
        assert_eq!(deserialized.memory_mb, 32768);
    }

    #[test]
    fn test_telemetry_error_display() {
        let net_err = TelemetryError::NetworkError("timeout".to_string());
        assert!(net_err.to_string().contains("timeout"));

        let ep_err = TelemetryError::EndpointError("HTTP 500".to_string());
        assert!(ep_err.to_string().contains("HTTP 500"));

        let ser_err = TelemetryError::Serialization("bad json".to_string());
        assert!(ser_err.to_string().contains("bad json"));
    }

    #[tokio::test]
    async fn test_is_enabled_reflects_config() {
        let enabled_config = TelemetryConfig {
            enabled: true,
            ..Default::default()
        };
        let disabled_config = TelemetryConfig {
            enabled: false,
            ..Default::default()
        };
        let c1 = TelemetryCollector::new(enabled_config);
        let c2 = TelemetryCollector::new(disabled_config);
        assert!(c1.is_enabled());
        assert!(!c2.is_enabled());
    }

    #[tokio::test]
    async fn test_multiple_event_types_interleaved() {
        let config = TelemetryConfig {
            enabled: true,
            metrics_enabled: true,
            ..Default::default()
        };
        let collector = TelemetryCollector::new(config);

        collector.record_counter("a", "c1", 1.0).await;
        collector.record_gauge("a", "g1", 2.0).await;
        collector.record_timing("a", "t1", 3.0).await;
        collector
            .record_event("a", "h1", 4.0, EventType::Histogram)
            .await;

        let events = collector.events.read().await;
        assert_eq!(events.len(), 4);
        assert_eq!(events[0].event_type, EventType::Counter);
        assert_eq!(events[1].event_type, EventType::Gauge);
        assert_eq!(events[2].event_type, EventType::Timing);
        assert_eq!(events[3].event_type, EventType::Histogram);
    }

    // --- New coverage tests (batch 2) ---

    #[test]
    fn test_telemetry_config_default_endpoint() {
        let config = TelemetryConfig::default();
        assert_eq!(config.endpoint_url, "https://telemetry.agnos.org/v1");
    }

    #[test]
    fn test_telemetry_config_default_flush_interval() {
        let config = TelemetryConfig::default();
        assert_eq!(config.flush_interval_secs, 300);
    }

    #[test]
    fn test_telemetry_config_instance_id_is_uuid() {
        let config = TelemetryConfig::default();
        // UUID v4 format: 8-4-4-4-12
        let parts: Vec<&str> = config.instance_id.split('-').collect();
        assert_eq!(parts.len(), 5);
        assert_eq!(parts[0].len(), 8);
        assert_eq!(parts[1].len(), 4);
        assert_eq!(parts[2].len(), 4);
        assert_eq!(parts[3].len(), 4);
        assert_eq!(parts[4].len(), 12);
    }

    #[test]
    fn test_generate_instance_id_is_valid_uuid() {
        let id = generate_instance_id();
        let parsed = uuid::Uuid::parse_str(&id);
        assert!(parsed.is_ok(), "Generated ID should be valid UUID: {}", id);
    }

    #[test]
    fn test_telemetry_config_clone() {
        let config = TelemetryConfig {
            enabled: true,
            crash_reporting: true,
            metrics_enabled: true,
            instance_id: "clone-me".to_string(),
            endpoint_url: "https://example.com".to_string(),
            sampling_rate: 0.42,
            flush_interval_secs: 10,
        };
        let cloned = config.clone();
        assert_eq!(cloned.instance_id, "clone-me");
        assert_eq!(cloned.sampling_rate, 0.42);
        assert_eq!(cloned.flush_interval_secs, 10);
    }

    #[test]
    fn test_telemetry_config_debug() {
        let config = TelemetryConfig::default();
        let debug = format!("{:?}", config);
        assert!(debug.contains("TelemetryConfig"));
        assert!(debug.contains("enabled"));
    }

    #[test]
    fn test_event_type_serialization_roundtrip() {
        let types = [
            EventType::Counter,
            EventType::Gauge,
            EventType::Histogram,
            EventType::Timing,
        ];
        for et in &types {
            let json = serde_json::to_string(et).unwrap();
            let deserialized: EventType = serde_json::from_str(&json).unwrap();
            assert_eq!(*et, deserialized);
        }
    }

    #[test]
    fn test_event_type_copy() {
        let a = EventType::Counter;
        let b = a; // Copy, not move
        assert_eq!(a, b);
    }

    #[test]
    fn test_system_info_clone() {
        let info = SystemInfo {
            os_type: "linux".to_string(),
            os_version: "test".to_string(),
            architecture: "x86_64".to_string(),
            cpu_count: 4,
            memory_mb: 8192,
            kernel_version: "6.6".to_string(),
        };
        let cloned = info.clone();
        assert_eq!(cloned.os_type, info.os_type);
        assert_eq!(cloned.cpu_count, info.cpu_count);
    }

    #[test]
    fn test_system_info_debug() {
        let info = SystemInfo {
            os_type: "linux".to_string(),
            os_version: "test".to_string(),
            architecture: "x86_64".to_string(),
            cpu_count: 4,
            memory_mb: 8192,
            kernel_version: "6.6".to_string(),
        };
        let debug = format!("{:?}", info);
        assert!(debug.contains("linux"));
        assert!(debug.contains("8192"));
    }

    #[test]
    fn test_crash_report_clone() {
        let report = CrashReport {
            timestamp: chrono::Utc::now(),
            instance_id: "clone-test".to_string(),
            version: "0.1.0".to_string(),
            component: "test".to_string(),
            error_message: "err".to_string(),
            stack_trace: None,
            system_info: SystemInfo {
                os_type: "linux".to_string(),
                os_version: "v".to_string(),
                architecture: "x86_64".to_string(),
                cpu_count: 1,
                memory_mb: 1024,
                kernel_version: "6.0".to_string(),
            },
        };
        let cloned = report.clone();
        assert_eq!(cloned.instance_id, "clone-test");
        assert!(cloned.stack_trace.is_none());
    }

    #[test]
    fn test_telemetry_session_clone() {
        let session = TelemetrySession {
            started_at: chrono::Utc::now(),
            instance_id: "sess-clone".to_string(),
            version: "0.1.0".to_string(),
            events_sent: 42,
            events_dropped: 3,
        };
        let cloned = session.clone();
        assert_eq!(cloned.events_sent, 42);
        assert_eq!(cloned.events_dropped, 3);
    }

    #[test]
    fn test_telemetry_session_debug() {
        let session = TelemetrySession {
            started_at: chrono::Utc::now(),
            instance_id: "dbg".to_string(),
            version: "1.0.0".to_string(),
            events_sent: 0,
            events_dropped: 0,
        };
        let debug = format!("{:?}", session);
        assert!(debug.contains("dbg"));
    }

    #[tokio::test]
    async fn test_record_event_with_zero_sampling_rate() {
        let config = TelemetryConfig {
            enabled: true,
            metrics_enabled: true,
            sampling_rate: 0.0,
            ..Default::default()
        };
        let collector = TelemetryCollector::new(config);
        // With 0.0 sampling rate, random_val > 0.0 is always true, so events are dropped
        for _ in 0..100 {
            collector.record_counter("cat", "name", 1.0).await;
        }
        let events = collector.events.read().await;
        assert!(events.is_empty(), "No events should pass 0.0 sampling rate");
    }

    #[tokio::test]
    async fn test_record_event_with_full_sampling_rate() {
        let config = TelemetryConfig {
            enabled: true,
            metrics_enabled: true,
            sampling_rate: 1.0,
            ..Default::default()
        };
        let collector = TelemetryCollector::new(config);
        collector.record_counter("cat", "name", 1.0).await;
        let events = collector.events.read().await;
        assert_eq!(events.len(), 1, "All events should pass 1.0 sampling rate");
    }

    #[tokio::test]
    async fn test_submit_crash_system_info_populated() {
        let config = TelemetryConfig {
            enabled: true,
            crash_reporting: true,
            ..Default::default()
        };
        let collector = TelemetryCollector::new(config);
        collector.submit_crash("test-comp", "test-err", None).await;

        let reports = collector.crash_reports.read().await;
        assert_eq!(reports.len(), 1);
        let info = &reports[0].system_info;
        assert!(!info.os_type.is_empty());
        assert!(!info.architecture.is_empty());
        assert!(info.cpu_count > 0);
    }

    #[tokio::test]
    async fn test_get_stats_version_is_package_version() {
        let config = TelemetryConfig::default();
        let collector = TelemetryCollector::new(config);
        let stats = collector.get_stats().await;
        assert_eq!(stats.version, env!("CARGO_PKG_VERSION"));
    }

    #[tokio::test]
    async fn test_flush_drains_events() {
        let config = TelemetryConfig {
            enabled: true,
            metrics_enabled: true,
            // Use an unreachable endpoint so flush will fail but still drain
            endpoint_url: "http://127.0.0.1:1/v1".to_string(),
            ..Default::default()
        };
        let collector = TelemetryCollector::new(config);
        collector.record_counter("cat", "name", 1.0).await;
        assert_eq!(collector.events.read().await.len(), 1);

        // Flush will fail (no server) but should drain events
        let _ = collector.flush().await;
        assert_eq!(collector.events.read().await.len(), 0);
    }

    #[test]
    fn test_telemetry_error_variants_are_error_trait() {
        let err: Box<dyn std::error::Error> =
            Box::new(TelemetryError::NetworkError("test".to_string()));
        assert!(err.to_string().contains("test"));
    }

    #[test]
    fn test_telemetry_event_metadata_empty_default() {
        let event = TelemetryEvent {
            timestamp: chrono::Utc::now(),
            instance_id: "test".to_string(),
            event_type: EventType::Counter,
            category: "cat".to_string(),
            name: "name".to_string(),
            value: 0.0,
            metadata: HashMap::new(),
        };
        assert!(event.metadata.is_empty());
    }

    #[test]
    fn test_telemetry_event_with_metadata() {
        let mut metadata = HashMap::new();
        metadata.insert("host".to_string(), "node-1".to_string());
        metadata.insert("region".to_string(), "us-west".to_string());
        let event = TelemetryEvent {
            timestamp: chrono::Utc::now(),
            instance_id: "test".to_string(),
            event_type: EventType::Gauge,
            category: "infra".to_string(),
            name: "disk_pct".to_string(),
            value: 72.5,
            metadata,
        };
        assert_eq!(event.metadata.len(), 2);
        assert_eq!(event.metadata.get("host").unwrap(), "node-1");
    }

    #[test]
    fn test_read_memory_mb_on_linux() {
        let mem = TelemetryCollector::read_memory_mb();
        // On Linux with /proc/meminfo, should be > 0
        #[cfg(target_os = "linux")]
        assert!(mem > 0, "Memory should be > 0 on Linux, got {}", mem);
        let _ = mem;
    }

    #[test]
    fn test_read_kernel_version_on_linux() {
        let version = TelemetryCollector::read_kernel_version();
        #[cfg(target_os = "linux")]
        {
            assert_ne!(version, "unknown");
            // Kernel version usually starts with a digit
            assert!(
                version.chars().next().unwrap().is_ascii_digit(),
                "Kernel version should start with digit: {}",
                version
            );
        }
        let _ = version;
    }

    #[test]
    fn test_telemetry_error_debug() {
        let err = TelemetryError::NetworkError("conn refused".to_string());
        let debug = format!("{:?}", err);
        assert!(debug.contains("NetworkError"));
        assert!(debug.contains("conn refused"));
    }
}

// ===========================================================================
// OpenTelemetry Trace Export + Distributed Tracing
// ===========================================================================

/// A 128-bit trace identifier, displayed as a 32-character hex string.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TraceId(pub [u8; 16]);

impl TraceId {
    /// Generate a new random TraceId.
    pub fn generate() -> Self {
        let mut bytes = [0u8; 16];
        for b in &mut bytes {
            *b = rand::random();
        }
        Self(bytes)
    }
}

impl std::fmt::Display for TraceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for b in &self.0 {
            write!(f, "{:02x}", b)?;
        }
        Ok(())
    }
}

/// A 64-bit span identifier, displayed as a 16-character hex string.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SpanId(pub [u8; 8]);

impl SpanId {
    /// Generate a new random SpanId.
    pub fn generate() -> Self {
        let mut bytes = [0u8; 8];
        for b in &mut bytes {
            *b = rand::random();
        }
        Self(bytes)
    }
}

impl std::fmt::Display for SpanId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for b in &self.0 {
            write!(f, "{:02x}", b)?;
        }
        Ok(())
    }
}

/// Status of a completed span.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpanStatus {
    Ok,
    Error(String),
    Unset,
}

/// A single span in a distributed trace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Span {
    pub trace_id: TraceId,
    pub span_id: SpanId,
    pub parent_span_id: Option<SpanId>,
    pub operation_name: String,
    pub service_name: String,
    pub start_time: chrono::DateTime<chrono::Utc>,
    pub end_time: Option<chrono::DateTime<chrono::Utc>>,
    pub status: SpanStatus,
    pub attributes: HashMap<String, String>,
}

impl Span {
    /// Mark this span as completed with the given status.
    pub fn finish(&mut self, status: SpanStatus) {
        self.end_time = Some(chrono::Utc::now());
        self.status = status;
    }
}

/// Context for propagating trace information across service boundaries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceContext {
    pub trace_id: TraceId,
    pub span_id: SpanId,
    pub baggage: HashMap<String, String>,
}

impl TraceContext {
    /// Start a new root trace for the given service.
    pub fn new_root(service: &str) -> Self {
        let _ = service; // service name is recorded on individual spans
        Self {
            trace_id: TraceId::generate(),
            span_id: SpanId::generate(),
            baggage: HashMap::new(),
        }
    }

    /// Create a child span under this context.
    pub fn child_span(&self, operation: &str, service: &str) -> Span {
        Span {
            trace_id: self.trace_id.clone(),
            span_id: SpanId::generate(),
            parent_span_id: Some(self.span_id.clone()),
            operation_name: operation.to_string(),
            service_name: service.to_string(),
            start_time: chrono::Utc::now(),
            end_time: None,
            status: SpanStatus::Unset,
            attributes: HashMap::new(),
        }
    }

    /// Inject W3C Trace Context headers (`traceparent`, `tracestate`) into a header map.
    pub fn inject_headers(&self) -> HashMap<String, String> {
        let mut headers = HashMap::new();
        // W3C traceparent: version-trace_id-parent_id-flags
        let traceparent = format!("00-{}-{}-01", self.trace_id, self.span_id);
        headers.insert("traceparent".to_string(), traceparent);

        if !self.baggage.is_empty() {
            let state_parts: Vec<String> = self
                .baggage
                .iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect();
            headers.insert("tracestate".to_string(), state_parts.join(","));
        }
        headers
    }

    /// Extract a `TraceContext` from W3C Trace Context headers.
    pub fn extract_headers(headers: &HashMap<String, String>) -> Option<Self> {
        let traceparent = headers.get("traceparent")?;
        let parts: Vec<&str> = traceparent.split('-').collect();
        if parts.len() != 4 {
            return None;
        }

        let trace_hex = parts[1];
        let span_hex = parts[2];

        if trace_hex.len() != 32 || span_hex.len() != 16 {
            return None;
        }

        let trace_bytes = hex_to_bytes_16(trace_hex)?;
        let span_bytes = hex_to_bytes_8(span_hex)?;

        let mut baggage = HashMap::new();
        if let Some(state) = headers.get("tracestate") {
            for pair in state.split(',') {
                let pair = pair.trim();
                if let Some((k, v)) = pair.split_once('=') {
                    baggage.insert(k.to_string(), v.to_string());
                }
            }
        }

        Some(Self {
            trace_id: TraceId(trace_bytes),
            span_id: SpanId(span_bytes),
            baggage,
        })
    }
}

/// Parse a 32-character hex string into [u8; 16].
fn hex_to_bytes_16(hex: &str) -> Option<[u8; 16]> {
    if hex.len() != 32 {
        return None;
    }
    let mut bytes = [0u8; 16];
    for i in 0..16 {
        bytes[i] = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(bytes)
}

/// Parse a 16-character hex string into [u8; 8].
fn hex_to_bytes_8(hex: &str) -> Option<[u8; 8]> {
    if hex.len() != 16 {
        return None;
    }
    let mut bytes = [0u8; 8];
    for i in 0..8 {
        bytes[i] = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(bytes)
}

/// Collects completed spans and exports them as JSON batches.
pub struct SpanCollector {
    spans: std::sync::Mutex<Vec<Span>>,
}

impl SpanCollector {
    /// Create a new empty collector.
    pub fn new() -> Self {
        Self {
            spans: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// Record a completed span.
    pub fn record(&self, span: Span) {
        if let Ok(mut spans) = self.spans.lock() {
            spans.push(span);
        }
    }

    /// Drain and return all collected spans.
    pub fn export_batch(&self) -> Vec<Span> {
        match self.spans.lock() {
            Ok(mut spans) => spans.drain(..).collect(),
            Err(_) => Vec::new(),
        }
    }

    /// Export collected spans as an OTLP-like JSON string, then drain.
    pub fn export_json(&self) -> String {
        let batch = self.export_batch();
        let resource_spans: Vec<serde_json::Value> = batch
            .iter()
            .map(|s| {
                serde_json::json!({
                    "traceId": s.trace_id.to_string(),
                    "spanId": s.span_id.to_string(),
                    "parentSpanId": s.parent_span_id.as_ref().map(|p| p.to_string()),
                    "operationName": s.operation_name,
                    "serviceName": s.service_name,
                    "startTimeUnixNano": s.start_time.timestamp_nanos_opt().unwrap_or(0),
                    "endTimeUnixNano": s.end_time.and_then(|e| e.timestamp_nanos_opt()),
                    "status": match &s.status {
                        SpanStatus::Ok => serde_json::json!({"code": "OK"}),
                        SpanStatus::Error(msg) => serde_json::json!({"code": "ERROR", "message": msg}),
                        SpanStatus::Unset => serde_json::json!({"code": "UNSET"}),
                    },
                    "attributes": s.attributes,
                })
            })
            .collect();

        serde_json::json!({
            "resourceSpans": resource_spans
        })
        .to_string()
    }
}

impl Default for SpanCollector {
    fn default() -> Self {
        Self::new()
    }
}

// ===========================================================================
// Distributed Tracing Tests
// ===========================================================================

#[cfg(test)]
mod tracing_tests {
    use super::*;

    #[test]
    fn test_trace_id_generate_nonzero() {
        let id = TraceId::generate();
        // Extremely unlikely to be all-zero
        assert!(id.0.iter().any(|&b| b != 0));
    }

    #[test]
    fn test_trace_id_display_hex_length() {
        let id = TraceId([0xab; 16]);
        let hex = id.to_string();
        assert_eq!(hex.len(), 32);
        assert_eq!(hex, "abababababababababababababababab");
    }

    #[test]
    fn test_trace_id_display_leading_zeros() {
        let mut bytes = [0u8; 16];
        bytes[15] = 1;
        let id = TraceId(bytes);
        let hex = id.to_string();
        assert_eq!(hex, "00000000000000000000000000000001");
    }

    #[test]
    fn test_span_id_generate_nonzero() {
        let id = SpanId::generate();
        assert!(id.0.iter().any(|&b| b != 0));
    }

    #[test]
    fn test_span_id_display_hex_length() {
        let id = SpanId([0xcd; 8]);
        let hex = id.to_string();
        assert_eq!(hex.len(), 16);
        assert_eq!(hex, "cdcdcdcdcdcdcdcd");
    }

    #[test]
    fn test_trace_context_new_root() {
        let ctx = TraceContext::new_root("my-service");
        assert!(ctx.trace_id.0.iter().any(|&b| b != 0));
        assert!(ctx.span_id.0.iter().any(|&b| b != 0));
        assert!(ctx.baggage.is_empty());
    }

    #[test]
    fn test_child_span_inherits_trace_id() {
        let ctx = TraceContext::new_root("parent-svc");
        let child = ctx.child_span("do-work", "child-svc");
        assert_eq!(child.trace_id, ctx.trace_id);
        assert_eq!(child.parent_span_id, Some(ctx.span_id.clone()));
        assert_eq!(child.operation_name, "do-work");
        assert_eq!(child.service_name, "child-svc");
        assert!(child.end_time.is_none());
        assert_eq!(child.status, SpanStatus::Unset);
    }

    #[test]
    fn test_child_span_has_unique_span_id() {
        let ctx = TraceContext::new_root("svc");
        let c1 = ctx.child_span("op1", "svc");
        let c2 = ctx.child_span("op2", "svc");
        assert_ne!(c1.span_id, c2.span_id);
    }

    #[test]
    fn test_span_finish() {
        let ctx = TraceContext::new_root("svc");
        let mut span = ctx.child_span("op", "svc");
        assert!(span.end_time.is_none());
        span.finish(SpanStatus::Ok);
        assert!(span.end_time.is_some());
        assert_eq!(span.status, SpanStatus::Ok);
    }

    #[test]
    fn test_span_finish_with_error() {
        let ctx = TraceContext::new_root("svc");
        let mut span = ctx.child_span("op", "svc");
        span.finish(SpanStatus::Error("boom".to_string()));
        assert_eq!(span.status, SpanStatus::Error("boom".to_string()));
    }

    #[test]
    fn test_inject_headers_format() {
        let ctx = TraceContext {
            trace_id: TraceId([0xaa; 16]),
            span_id: SpanId([0xbb; 8]),
            baggage: HashMap::new(),
        };
        let headers = ctx.inject_headers();
        let tp = headers.get("traceparent").unwrap();
        assert_eq!(
            tp,
            "00-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-bbbbbbbbbbbbbbbb-01"
        );
        assert!(!headers.contains_key("tracestate"));
    }

    #[test]
    fn test_inject_headers_with_baggage() {
        let mut baggage = HashMap::new();
        baggage.insert("tenant".to_string(), "acme".to_string());
        let ctx = TraceContext {
            trace_id: TraceId([0x01; 16]),
            span_id: SpanId([0x02; 8]),
            baggage,
        };
        let headers = ctx.inject_headers();
        assert!(headers.contains_key("tracestate"));
        assert!(headers["tracestate"].contains("tenant=acme"));
    }

    #[test]
    fn test_extract_headers_roundtrip() {
        let original = TraceContext {
            trace_id: TraceId([0xde; 16]),
            span_id: SpanId([0xad; 8]),
            baggage: {
                let mut m = HashMap::new();
                m.insert("key".to_string(), "val".to_string());
                m
            },
        };
        let headers = original.inject_headers();
        let extracted = TraceContext::extract_headers(&headers).unwrap();
        assert_eq!(extracted.trace_id, original.trace_id);
        assert_eq!(extracted.span_id, original.span_id);
        assert_eq!(extracted.baggage.get("key").unwrap(), "val");
    }

    #[test]
    fn test_extract_headers_invalid_format() {
        let mut headers = HashMap::new();
        headers.insert("traceparent".to_string(), "invalid".to_string());
        assert!(TraceContext::extract_headers(&headers).is_none());
    }

    #[test]
    fn test_extract_headers_missing() {
        let headers = HashMap::new();
        assert!(TraceContext::extract_headers(&headers).is_none());
    }

    #[test]
    fn test_extract_headers_wrong_length() {
        let mut headers = HashMap::new();
        headers.insert("traceparent".to_string(), "00-abcd-ef01-01".to_string());
        assert!(TraceContext::extract_headers(&headers).is_none());
    }

    #[test]
    fn test_extract_headers_invalid_hex() {
        let mut headers = HashMap::new();
        // 32 chars but not valid hex
        headers.insert(
            "traceparent".to_string(),
            "00-zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz-0000000000000001-01".to_string(),
        );
        assert!(TraceContext::extract_headers(&headers).is_none());
    }

    #[test]
    fn test_span_collector_record_and_export() {
        let collector = SpanCollector::new();
        let ctx = TraceContext::new_root("svc");
        let mut span = ctx.child_span("op", "svc");
        span.finish(SpanStatus::Ok);
        collector.record(span);

        let batch = collector.export_batch();
        assert_eq!(batch.len(), 1);
        assert_eq!(batch[0].operation_name, "op");

        // After export, should be empty
        let batch2 = collector.export_batch();
        assert!(batch2.is_empty());
    }

    #[test]
    fn test_span_collector_export_json() {
        let collector = SpanCollector::new();
        let ctx = TraceContext::new_root("json-svc");
        let mut span = ctx.child_span("json-op", "json-svc");
        span.finish(SpanStatus::Ok);
        collector.record(span);

        let json = collector.export_json();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed["resourceSpans"].is_array());
        assert_eq!(parsed["resourceSpans"].as_array().unwrap().len(), 1);
        assert_eq!(parsed["resourceSpans"][0]["operationName"], "json-op");
        assert_eq!(parsed["resourceSpans"][0]["status"]["code"], "OK");
    }

    #[test]
    fn test_span_collector_export_json_empty() {
        let collector = SpanCollector::new();
        let json = collector.export_json();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["resourceSpans"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn test_span_collector_multiple_spans() {
        let collector = SpanCollector::new();
        let ctx = TraceContext::new_root("svc");
        for i in 0..5 {
            let mut span = ctx.child_span(&format!("op-{}", i), "svc");
            span.finish(SpanStatus::Ok);
            collector.record(span);
        }
        let batch = collector.export_batch();
        assert_eq!(batch.len(), 5);
    }

    #[test]
    fn test_span_status_variants() {
        assert_eq!(SpanStatus::Ok, SpanStatus::Ok);
        assert_ne!(SpanStatus::Ok, SpanStatus::Unset);
        assert_ne!(SpanStatus::Ok, SpanStatus::Error("x".to_string()));
    }

    #[test]
    fn test_span_attributes() {
        let ctx = TraceContext::new_root("svc");
        let mut span = ctx.child_span("op", "svc");
        span.attributes
            .insert("http.method".to_string(), "GET".to_string());
        span.attributes
            .insert("http.url".to_string(), "/api/v1".to_string());
        assert_eq!(span.attributes.len(), 2);
        assert_eq!(span.attributes["http.method"], "GET");
    }

    #[test]
    fn test_hex_to_bytes_16_valid() {
        let result = hex_to_bytes_16("0123456789abcdef0123456789abcdef");
        assert!(result.is_some());
        let bytes = result.unwrap();
        assert_eq!(bytes[0], 0x01);
        assert_eq!(bytes[7], 0xef);
    }

    #[test]
    fn test_hex_to_bytes_16_invalid_length() {
        assert!(hex_to_bytes_16("abcd").is_none());
    }

    #[test]
    fn test_hex_to_bytes_8_valid() {
        let result = hex_to_bytes_8("0123456789abcdef");
        assert!(result.is_some());
        let bytes = result.unwrap();
        assert_eq!(bytes[0], 0x01);
        assert_eq!(bytes[7], 0xef);
    }

    #[test]
    fn test_hex_to_bytes_8_invalid_length() {
        assert!(hex_to_bytes_8("abcd").is_none());
    }

    #[test]
    fn test_trace_id_serialization_roundtrip() {
        let id = TraceId([0x42; 16]);
        let json = serde_json::to_string(&id).unwrap();
        let deser: TraceId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, deser);
    }

    #[test]
    fn test_span_id_serialization_roundtrip() {
        let id = SpanId([0x99; 8]);
        let json = serde_json::to_string(&id).unwrap();
        let deser: SpanId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, deser);
    }

    #[test]
    fn test_span_serialization() {
        let ctx = TraceContext::new_root("svc");
        let mut span = ctx.child_span("op", "svc");
        span.finish(SpanStatus::Ok);
        let json = serde_json::to_string(&span).unwrap();
        let deser: Span = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.operation_name, "op");
        assert_eq!(deser.status, SpanStatus::Ok);
    }

    #[test]
    fn test_span_collector_default() {
        let collector = SpanCollector::default();
        assert!(collector.export_batch().is_empty());
    }

    #[test]
    fn test_export_json_error_status() {
        let collector = SpanCollector::new();
        let ctx = TraceContext::new_root("svc");
        let mut span = ctx.child_span("fail-op", "svc");
        span.finish(SpanStatus::Error("timeout".to_string()));
        collector.record(span);

        let json = collector.export_json();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["resourceSpans"][0]["status"]["code"], "ERROR");
        assert_eq!(parsed["resourceSpans"][0]["status"]["message"], "timeout");
    }
}
