//! GPU status HUD widget for the AGNOS desktop environment.
//!
//! Displays per-device VRAM usage bars, compute utilization, and temperature
//! by polling the `agnos_gpu_status` MCP tool via daimon's MCP call API at
//! `http://localhost:8090/v1/mcp/tools/call`.

use std::sync::{Arc, RwLock};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

/// Severity band used to colour-code a metric in the HUD.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MetricBand {
    /// Value is in the normal/safe range (green).
    Normal,
    /// Value is elevated and warrants attention (amber).
    Warning,
    /// Value is critical; action may be required (red).
    Critical,
}

impl MetricBand {
    /// Classify a 0–100 utilisation percentage.
    pub fn from_utilization(pct: f32) -> Self {
        if pct >= 95.0 {
            Self::Critical
        } else if pct >= 80.0 {
            Self::Warning
        } else {
            Self::Normal
        }
    }

    /// Classify a temperature in degrees Celsius.
    pub fn from_temperature(deg_c: f32) -> Self {
        if deg_c >= 90.0 {
            Self::Critical
        } else if deg_c >= 75.0 {
            Self::Warning
        } else {
            Self::Normal
        }
    }
}

/// Per-device GPU snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuDeviceState {
    /// Vendor-assigned device index (0-based).
    pub device_index: u32,
    /// Human-readable device name (e.g. "NVIDIA GeForce RTX 4090").
    pub name: String,
    /// VRAM used in MiB.
    pub vram_used_mib: u64,
    /// Total VRAM in MiB.
    pub vram_total_mib: u64,
    /// VRAM utilisation as a fraction 0.0–1.0 (derived from used/total).
    pub vram_fraction: f32,
    /// GPU compute / shader utilisation as a percentage 0–100.
    pub compute_utilization_pct: f32,
    /// Core temperature in degrees Celsius, if available.
    pub temperature_c: Option<f32>,
    /// Compute capability string (e.g. `"8.9"` for Ada Lovelace).
    pub compute_capability: Option<String>,
    /// Severity band for VRAM utilisation.
    pub vram_band: MetricBand,
    /// Severity band for compute utilisation.
    pub compute_band: MetricBand,
    /// Severity band for temperature (Normal when no sensor data).
    pub temp_band: MetricBand,
    /// Timestamp when this snapshot was recorded.
    pub snapshot_at: DateTime<Utc>,
}

impl GpuDeviceState {
    /// Construct a device state entry and derive the severity bands.
    pub fn new(
        device_index: u32,
        name: String,
        vram_used_mib: u64,
        vram_total_mib: u64,
        compute_utilization_pct: f32,
        temperature_c: Option<f32>,
        compute_capability: Option<String>,
    ) -> Self {
        let vram_fraction = if vram_total_mib == 0 {
            0.0
        } else {
            vram_used_mib as f32 / vram_total_mib as f32
        };

        let vram_band = MetricBand::from_utilization(vram_fraction * 100.0);
        let compute_band = MetricBand::from_utilization(compute_utilization_pct);
        let temp_band = temperature_c
            .map(MetricBand::from_temperature)
            .unwrap_or(MetricBand::Normal);

        Self {
            device_index,
            name,
            vram_used_mib,
            vram_total_mib,
            vram_fraction,
            compute_utilization_pct,
            temperature_c,
            compute_capability,
            vram_band,
            compute_band,
            temp_band,
            snapshot_at: Utc::now(),
        }
    }
}

/// Render output produced by [`GpuStatusWidget::render`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuStatusRenderData {
    /// Per-device snapshots, ordered by `device_index`.
    pub devices: Vec<GpuDeviceState>,
    /// Number of devices currently in a Warning or Critical VRAM state.
    pub devices_under_pressure: usize,
    /// Highest single-device VRAM utilisation fraction across all devices.
    pub peak_vram_fraction: f32,
    /// Highest single-device temperature across all devices, if any sensor data available.
    pub peak_temperature_c: Option<f32>,
    /// Whether the last `update()` call succeeded.
    pub last_fetch_ok: bool,
    /// Timestamp of the most recent successful fetch.
    pub last_fetch_at: Option<DateTime<Utc>>,
}

/// HUD widget that displays per-GPU VRAM, utilisation, and temperature.
///
/// # Example
/// ```rust,no_run
/// use desktop_environment::hud::gpu_status::GpuStatusWidget;
/// use std::time::Duration;
///
/// let widget = GpuStatusWidget::new();
/// let handle = widget.start_polling(Duration::from_secs(3));
/// let render = widget.render();
/// for dev in &render.devices {
///     println!(
///         "{}: VRAM {}/{} MiB  util {:.0}%  temp {:?}°C",
///         dev.name, dev.vram_used_mib, dev.vram_total_mib,
///         dev.compute_utilization_pct, dev.temperature_c
///     );
/// }
/// ```
#[derive(Debug, Clone)]
pub struct GpuStatusWidget {
    devices: Arc<RwLock<Vec<GpuDeviceState>>>,
    last_fetch_ok: Arc<RwLock<bool>>,
    last_fetch_at: Arc<RwLock<Option<DateTime<Utc>>>>,
}

impl Default for GpuStatusWidget {
    fn default() -> Self {
        Self::new()
    }
}

impl GpuStatusWidget {
    /// Create a new widget with no device data.
    pub fn new() -> Self {
        Self {
            devices: Arc::new(RwLock::new(Vec::new())),
            last_fetch_ok: Arc::new(RwLock::new(false)),
            last_fetch_at: Arc::new(RwLock::new(None)),
        }
    }

    /// Return display-ready data for the compositor.
    pub fn render(&self) -> GpuStatusRenderData {
        let devices = self
            .devices
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let last_fetch_ok = *self.last_fetch_ok.read().unwrap_or_else(|e| e.into_inner());
        let last_fetch_at = *self.last_fetch_at.read().unwrap_or_else(|e| e.into_inner());

        let devices_under_pressure = devices
            .iter()
            .filter(|d| matches!(d.vram_band, MetricBand::Warning | MetricBand::Critical))
            .count();

        let peak_vram_fraction = devices
            .iter()
            .map(|d| d.vram_fraction)
            .fold(0.0_f32, f32::max);

        let peak_temperature_c = devices
            .iter()
            .filter_map(|d| d.temperature_c)
            .reduce(f32::max);

        GpuStatusRenderData {
            devices,
            devices_under_pressure,
            peak_vram_fraction,
            peak_temperature_c,
            last_fetch_ok,
            last_fetch_at,
        }
    }

    /// Fetch fresh GPU data from the daimon MCP endpoint and update state.
    ///
    /// Calls `POST /v1/mcp/tools/call` with tool `agnos_gpu_status`.
    pub async fn update(&self) {
        match Self::fetch_gpu_status().await {
            Ok(devs) => {
                let mut devices = self.devices.write().unwrap_or_else(|e| e.into_inner());
                *devices = devs;
                *self
                    .last_fetch_ok
                    .write()
                    .unwrap_or_else(|e| e.into_inner()) = true;
                *self
                    .last_fetch_at
                    .write()
                    .unwrap_or_else(|e| e.into_inner()) = Some(Utc::now());
                debug!("GPU status HUD: {} device(s) loaded", devices.len());
            }
            Err(e) => {
                warn!("GPU status HUD update failed: {}", e);
                *self
                    .last_fetch_ok
                    .write()
                    .unwrap_or_else(|e| e.into_inner()) = false;
            }
        }
    }

    /// Spawn a background task that calls [`update`](Self::update) on `interval`.
    pub fn start_polling(&self, interval: std::time::Duration) -> tokio::task::JoinHandle<()> {
        let widget = self.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                ticker.tick().await;
                widget.update().await;
            }
        })
    }

    /// Directly overwrite device state (useful for tests or manual injection).
    pub fn set_devices(&self, devs: Vec<GpuDeviceState>) {
        let mut devices = self.devices.write().unwrap_or_else(|e| e.into_inner());
        *devices = devs;
        *self
            .last_fetch_ok
            .write()
            .unwrap_or_else(|e| e.into_inner()) = true;
        *self
            .last_fetch_at
            .write()
            .unwrap_or_else(|e| e.into_inner()) = Some(Utc::now());
    }

    // --- private helpers ---

    async fn fetch_gpu_status(
    ) -> Result<Vec<GpuDeviceState>, Box<dyn std::error::Error + Send + Sync>> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()?;

        let body = serde_json::json!({
            "name": "agnos_gpu_status",
            "arguments": {}
        });

        let resp = client
            .post("http://localhost:8090/v1/mcp/tools/call")
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(format!("MCP call returned {}", resp.status()).into());
        }

        let json: serde_json::Value = resp.json().await?;
        Self::parse_gpu_status(&json)
    }

    fn parse_gpu_status(
        json: &serde_json::Value,
    ) -> Result<Vec<GpuDeviceState>, Box<dyn std::error::Error + Send + Sync>> {
        // Unwrap MCP envelope: result.content[0].text may be a JSON string.
        let inner = json
            .pointer("/result/content/0/text")
            .and_then(|v| v.as_str())
            .map(|s| {
                serde_json::from_str::<serde_json::Value>(s).unwrap_or(serde_json::Value::Null)
            })
            .unwrap_or_else(|| json.clone());

        // Accept either `{ "devices": [...] }` or a bare array.
        let arr = inner
            .get("devices")
            .or_else(|| if inner.is_array() { Some(&inner) } else { None })
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        let mut devices: Vec<GpuDeviceState> = arr
            .iter()
            .enumerate()
            .map(|(fallback_idx, d)| {
                let device_index = d["device_index"].as_u64().unwrap_or(fallback_idx as u64) as u32;
                let name = d["name"].as_str().unwrap_or("GPU").to_string();
                let vram_used_mib = d["vram_used_mib"]
                    .as_u64()
                    .or_else(|| d["vram_used"].as_u64())
                    .unwrap_or(0);
                let vram_total_mib = d["vram_total_mib"]
                    .as_u64()
                    .or_else(|| d["vram_total"].as_u64())
                    .unwrap_or(0);
                let compute_utilization_pct = d["compute_utilization_pct"]
                    .as_f64()
                    .or_else(|| d["utilization"].as_f64())
                    .unwrap_or(0.0) as f32;
                let temperature_c = d["temperature_c"]
                    .as_f64()
                    .or_else(|| d["temperature"].as_f64())
                    .map(|t| t as f32);
                let compute_capability = d["compute_capability"].as_str().map(str::to_string);

                GpuDeviceState::new(
                    device_index,
                    name,
                    vram_used_mib,
                    vram_total_mib,
                    compute_utilization_pct,
                    temperature_c,
                    compute_capability,
                )
            })
            .collect();

        devices.sort_by_key(|d| d.device_index);
        Ok(devices)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn two_device_json() -> serde_json::Value {
        serde_json::json!({
            "devices": [
                {
                    "device_index": 0,
                    "name": "NVIDIA RTX 4090",
                    "vram_used_mib": 8192,
                    "vram_total_mib": 24576,
                    "compute_utilization_pct": 65.0,
                    "temperature_c": 72.0,
                    "compute_capability": "8.9"
                },
                {
                    "device_index": 1,
                    "name": "NVIDIA RTX 3080",
                    "vram_used_mib": 9500,
                    "vram_total_mib": 10240,
                    "compute_utilization_pct": 88.0,
                    "temperature_c": 83.0,
                    "compute_capability": "8.6"
                }
            ]
        })
    }

    #[test]
    fn test_parse_gpu_status() {
        let json = two_device_json();
        let devs = GpuStatusWidget::parse_gpu_status(&json).unwrap();
        assert_eq!(devs.len(), 2);

        let d0 = &devs[0];
        assert_eq!(d0.device_index, 0);
        assert_eq!(d0.vram_used_mib, 8192);
        assert_eq!(d0.vram_total_mib, 24576);
        assert!((d0.vram_fraction - 8192.0 / 24576.0).abs() < 1e-4);
        assert_eq!(d0.compute_capability, Some("8.9".to_string()));
        assert_eq!(d0.vram_band, MetricBand::Normal);
        assert_eq!(d0.compute_band, MetricBand::Normal);
        assert_eq!(d0.temp_band, MetricBand::Normal);

        let d1 = &devs[1];
        // 9500/10240 ≈ 92.8% → Warning
        assert_eq!(d1.vram_band, MetricBand::Warning);
        // 88% utilization → Warning
        assert_eq!(d1.compute_band, MetricBand::Warning);
        // 83°C → Warning
        assert_eq!(d1.temp_band, MetricBand::Warning);
    }

    #[test]
    fn test_render_empty() {
        let widget = GpuStatusWidget::new();
        let data = widget.render();
        assert!(data.devices.is_empty());
        assert_eq!(data.devices_under_pressure, 0);
        assert_eq!(data.peak_vram_fraction, 0.0);
        assert!(data.peak_temperature_c.is_none());
        assert!(!data.last_fetch_ok);
    }

    #[test]
    fn test_render_with_devices() {
        let widget = GpuStatusWidget::new();
        let devs = GpuStatusWidget::parse_gpu_status(&two_device_json()).unwrap();
        widget.set_devices(devs);

        let data = widget.render();
        assert_eq!(data.devices.len(), 2);
        // Only device 1 is under VRAM pressure (Warning)
        assert_eq!(data.devices_under_pressure, 1);
        assert!(data.peak_vram_fraction > 0.5);
        assert_eq!(data.peak_temperature_c, Some(83.0));
        assert!(data.last_fetch_ok);
        assert!(data.last_fetch_at.is_some());
    }

    #[test]
    fn test_metric_band_utilization() {
        assert_eq!(MetricBand::from_utilization(50.0), MetricBand::Normal);
        assert_eq!(MetricBand::from_utilization(80.0), MetricBand::Warning);
        assert_eq!(MetricBand::from_utilization(94.9), MetricBand::Warning);
        assert_eq!(MetricBand::from_utilization(95.0), MetricBand::Critical);
        assert_eq!(MetricBand::from_utilization(100.0), MetricBand::Critical);
    }

    #[test]
    fn test_metric_band_temperature() {
        assert_eq!(MetricBand::from_temperature(60.0), MetricBand::Normal);
        assert_eq!(MetricBand::from_temperature(75.0), MetricBand::Warning);
        assert_eq!(MetricBand::from_temperature(89.9), MetricBand::Warning);
        assert_eq!(MetricBand::from_temperature(90.0), MetricBand::Critical);
        assert_eq!(MetricBand::from_temperature(100.0), MetricBand::Critical);
    }

    #[test]
    fn test_gpu_device_state_vram_zero_total() {
        // Should not divide by zero.
        let dev = GpuDeviceState::new(0, "ghost".to_string(), 0, 0, 0.0, None, None);
        assert_eq!(dev.vram_fraction, 0.0);
        assert_eq!(dev.vram_band, MetricBand::Normal);
    }

    #[test]
    fn test_parse_mcp_wrapped_response() {
        let inner = serde_json::json!({
            "devices": [
                {
                    "device_index": 0,
                    "name": "Test GPU",
                    "vram_used_mib": 4096,
                    "vram_total_mib": 8192,
                    "compute_utilization_pct": 30.0,
                    "temperature_c": 55.0
                }
            ]
        });
        let wrapped = serde_json::json!({
            "result": {
                "content": [{ "text": inner.to_string() }]
            }
        });
        let devs = GpuStatusWidget::parse_gpu_status(&wrapped).unwrap();
        assert_eq!(devs.len(), 1);
        assert_eq!(devs[0].name, "Test GPU");
    }

    #[test]
    fn test_devices_sorted_by_index() {
        let json = serde_json::json!({
            "devices": [
                { "device_index": 3, "name": "D3", "vram_used_mib": 0, "vram_total_mib": 8192, "compute_utilization_pct": 0.0 },
                { "device_index": 1, "name": "D1", "vram_used_mib": 0, "vram_total_mib": 8192, "compute_utilization_pct": 0.0 },
                { "device_index": 0, "name": "D0", "vram_used_mib": 0, "vram_total_mib": 8192, "compute_utilization_pct": 0.0 }
            ]
        });
        let devs = GpuStatusWidget::parse_gpu_status(&json).unwrap();
        assert_eq!(devs[0].device_index, 0);
        assert_eq!(devs[1].device_index, 1);
        assert_eq!(devs[2].device_index, 3);
    }
}
