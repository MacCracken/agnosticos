//! MQTT Bridge — Translates ESP32 MCU heartbeats to the edge fleet model.
//!
//! Subscribes to MQTT topics published by ESP32 agents (and other MCU nodes)
//! and translates incoming JSON messages into [`EdgeNode`] heartbeats that
//! feed into the existing [`EdgeFleetManager`] registry.
//!
//! Topic schema (from `recipes/edge/esp32-agent.toml`):
//!   - `agnos/{node_id}/heartbeat`        — periodic liveness + capabilities
//!   - `agnos/{node_id}/telemetry`        — sensor readings batch
//!   - `agnos/{node_id}/status`           — lifecycle events (boot, sleep, ota)
//!   - `agnos/{node_id}/inference/result` — TinyML inference results (E4)
//!   - `agnos/{node_id}/inference/status` — TinyML model load state (E4)
//!   - `agnos/{node_id}/camera/frame`     — JPEG frame as base64 (ESP32-CAM, E3)
//!   - `agnos/{node_id}/camera/motion`    — motion detection event (ESP32-CAM, E3)
//!
//! The bridge auto-registers unknown nodes on first heartbeat and updates
//! existing nodes on subsequent messages. Reconnection is handled by
//! rumqttc's built-in retry loop.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, warn};

use super::fleet::EdgeFleetManager;
use super::types::{EdgeCapabilities, EdgeNodeStatus};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for the MQTT bridge connecting MCU agents to daimon.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MqttBridgeConfig {
    /// MQTT broker hostname or IP address.
    pub broker_host: String,
    /// MQTT broker port (typically 1883 for plain, 8883 for TLS).
    pub broker_port: u16,
    /// Topic prefix. The bridge subscribes to `{prefix}/+/heartbeat` etc.
    /// Default: `"agnos"`.
    pub topic_prefix: String,
    /// Client ID used when connecting to the broker.
    pub client_id: String,
    /// Keep-alive interval in seconds.
    pub keepalive_secs: u64,
    /// Whether to use TLS (mbedtls on ESP32 side, rustls on daimon side).
    pub use_tls: bool,
    /// Optional path to CA certificate for TLS verification.
    pub ca_cert_path: Option<String>,
    /// Optional username for MQTT authentication.
    pub username: Option<String>,
    /// Optional password for MQTT authentication.
    pub password: Option<String>,
    /// Maximum number of MCU nodes the bridge will track. Prevents unbounded
    /// growth from rogue publishers.
    pub max_mcu_nodes: usize,
}

impl Default for MqttBridgeConfig {
    fn default() -> Self {
        Self {
            broker_host: "localhost".into(),
            broker_port: 1883,
            topic_prefix: "agnos".into(),
            client_id: "daimon-mqtt-bridge".into(),
            keepalive_secs: 30,
            use_tls: false,
            ca_cert_path: None,
            username: None,
            password: None,
            max_mcu_nodes: 500,
        }
    }
}

// ---------------------------------------------------------------------------
// Inbound MQTT payloads (ESP32 agent JSON format)
// ---------------------------------------------------------------------------

/// Heartbeat payload published by an ESP32 agent to `agnos/{node_id}/heartbeat`.
#[derive(Debug, Clone, Deserialize)]
pub struct McuHeartbeat {
    /// CPU architecture: `"xtensa"` (ESP32-S3) or `"riscv32"` (ESP32-C3).
    pub arch: String,
    /// Available memory in KB (SRAM or SRAM + PSRAM).
    pub memory_kb: u64,
    /// Uptime in seconds since last boot.
    pub uptime_secs: u64,
    /// WiFi RSSI in dBm (negative value, e.g. -42).
    #[serde(default)]
    pub wifi_rssi: Option<i32>,
    /// Battery percentage (0-100), if battery-powered.
    #[serde(default)]
    pub battery_pct: Option<u8>,
    /// List of active sensor types.
    #[serde(default)]
    pub sensors: Vec<String>,
    /// Firmware version string.
    #[serde(default)]
    pub firmware_version: Option<String>,
    /// Node name / hostname (optional, falls back to node_id).
    #[serde(default)]
    pub name: Option<String>,
}

/// Telemetry payload published by an ESP32 agent to `agnos/{node_id}/telemetry`.
#[derive(Debug, Clone, Deserialize)]
pub struct McuTelemetry {
    /// Sensor readings as key-value pairs.
    /// Keys are sensor names (e.g. "temperature", "humidity").
    /// Values are the reading as f64.
    #[serde(default)]
    pub readings: std::collections::HashMap<String, f64>,
    /// Timestamp from the MCU (seconds since boot, or epoch if NTP synced).
    #[serde(default)]
    pub timestamp_secs: Option<u64>,
}

/// Status payload published to `agnos/{node_id}/status`.
#[derive(Debug, Clone, Deserialize)]
pub struct McuStatus {
    /// Status event type.
    pub event: McuStatusEvent,
    /// Optional human-readable message.
    #[serde(default)]
    pub message: Option<String>,
}

/// MCU lifecycle events.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum McuStatusEvent {
    Boot,
    Sleep,
    Wake,
    OtaStart,
    OtaComplete,
    OtaFailed,
    Error,
}

/// TinyML inference result published by an ESP32-S3 agent to
/// `agnos/{node_id}/inference/result`.
///
/// Sent each time the on-device model produces a classification above the
/// configured confidence threshold. The ESP32-S3 vector extensions (Xtensa
/// HiFi DSP) accelerate int8 quantized inference ~3-5x over scalar.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McuInferenceResult {
    /// Name of the loaded TinyML model (e.g. "kws_micro_speech").
    pub model_name: String,
    /// Predicted label (e.g. "yes", "no", "wave", "anomaly_high").
    pub label: String,
    /// Confidence score in 0.0-1.0 range.
    pub confidence: f64,
    /// Inference latency on the MCU in milliseconds.
    pub latency_ms: u32,
    /// Input modality: "audio" (I2S mic / KWS), "imu" (MPU6050 / gesture),
    /// "image" (ESP32-CAM / visual classification).
    pub input_type: McuInferenceInputType,
    /// Optional timestamp (seconds since boot, or epoch if NTP synced).
    #[serde(default)]
    pub timestamp_secs: Option<u64>,
}

/// TinyML inference status published by an ESP32-S3 agent to
/// `agnos/{node_id}/inference/status`.
///
/// Sent periodically (configurable) or on model load/unload events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McuInferenceStatus {
    /// Whether a model is currently loaded and ready for inference.
    pub model_loaded: bool,
    /// Name of the loaded model (empty if none loaded).
    pub model_name: String,
    /// Memory consumed by the model + arena in bytes.
    pub memory_used_bytes: u64,
    /// Total number of inferences since last boot.
    pub inference_count: u64,
    /// Model type: "kws", "gesture", or "anomaly".
    #[serde(default)]
    pub model_type: Option<String>,
    /// Average inference latency in milliseconds (rolling window).
    #[serde(default)]
    pub avg_latency_ms: Option<u32>,
}

/// Input modality for TinyML inference on the MCU.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum McuInferenceInputType {
    /// Audio input from I2S microphone (keyword spotting).
    Audio,
    /// IMU / accelerometer input (gesture recognition).
    Imu,
    /// Camera image input (visual classification).
    Image,
}

/// Camera frame payload published by an ESP32-CAM agent to
/// `agnos/{node_id}/camera/frame`.
///
/// Contains a JPEG-encoded image as base64 string. Frames are captured either
/// on a timed interval or triggered by motion detection. The bridge stores
/// each frame as a capture event in the fleet node's history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McuCameraFrame {
    /// Base64-encoded JPEG image data.
    pub data_base64: String,
    /// Image width in pixels.
    pub width: u32,
    /// Image height in pixels.
    pub height: u32,
    /// JPEG quality setting used (1-63).
    pub jpeg_quality: u8,
    /// What triggered this capture: "motion", "interval", or "manual".
    #[serde(default = "default_camera_trigger")]
    pub trigger: String,
    /// Optional timestamp (seconds since boot, or epoch if NTP synced).
    #[serde(default)]
    pub timestamp_secs: Option<u64>,
}

fn default_camera_trigger() -> String {
    "manual".to_string()
}

/// Motion detection event published by an ESP32-CAM agent to
/// `agnos/{node_id}/camera/motion`.
///
/// Sent when the on-device frame-differencing algorithm or an external PIR
/// sensor detects motion. May optionally include a snapshot frame if the
/// camera captured one at the time of detection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McuMotionEvent {
    /// Whether motion was detected (always true when published, but included
    /// for schema completeness and potential "motion_cleared" events).
    pub detected: bool,
    /// Motion intensity as a percentage (0.0-100.0). Higher values indicate
    /// more pixels changed between frames.
    #[serde(default)]
    pub intensity: Option<f64>,
    /// Detection source: "frame_diff" (software), "pir" (GPIO sensor), or "both".
    #[serde(default = "default_motion_source")]
    pub source: String,
    /// Optional base64-encoded JPEG snapshot taken at the moment of detection.
    /// Only included if the ESP32-CAM is configured to attach a frame to
    /// motion events (increases payload size).
    #[serde(default)]
    pub snapshot_base64: Option<String>,
    /// Snapshot dimensions (only present if snapshot_base64 is set).
    #[serde(default)]
    pub snapshot_width: Option<u32>,
    #[serde(default)]
    pub snapshot_height: Option<u32>,
    /// Optional timestamp (seconds since boot, or epoch if NTP synced).
    #[serde(default)]
    pub timestamp_secs: Option<u64>,
}

fn default_motion_source() -> String {
    "frame_diff".to_string()
}

/// A stored camera capture event, created when a frame or motion snapshot
/// is received from an ESP32-CAM node via MQTT.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CameraCaptureEvent {
    /// Unique ID for this capture event.
    pub id: String,
    /// MCU node_id that produced this capture.
    pub mcu_node_id: String,
    /// Fleet registry node_id (if registered).
    pub fleet_id: Option<String>,
    /// Base64-encoded JPEG image data.
    pub data_base64: String,
    /// Image width in pixels.
    pub width: u32,
    /// Image height in pixels.
    pub height: u32,
    /// What triggered this capture.
    pub trigger: String,
    /// When this event was received by the bridge.
    pub received_at: chrono::DateTime<Utc>,
    /// Motion intensity (only for motion-triggered captures).
    pub motion_intensity: Option<f64>,
}

// ---------------------------------------------------------------------------
// Bridge
// ---------------------------------------------------------------------------

/// MQTT bridge that subscribes to MCU agent topics and translates messages
/// into edge fleet operations.
///
/// Holds a shared reference to the [`EdgeFleetManager`] so heartbeats from
/// MQTT and HTTP coexist in the same registry.
pub struct MqttBridge {
    config: MqttBridgeConfig,
    fleet: Arc<Mutex<EdgeFleetManager>>,
    /// Maps MQTT node_id (from topic) to fleet registry node_id (UUID).
    /// MCU nodes use short identifiers (e.g. MAC-derived), while the fleet
    /// registry assigns UUIDs on registration.
    node_id_map: Arc<Mutex<std::collections::HashMap<String, String>>>,
    /// Camera capture events received from ESP32-CAM nodes. Stored in a
    /// ring buffer (newest entries replace oldest when capacity is reached).
    /// Each entry contains the base64-encoded JPEG and metadata.
    camera_captures: Arc<Mutex<Vec<CameraCaptureEvent>>>,
}

/// Maximum number of camera capture events retained in memory.
/// Older events are discarded when this limit is reached.
const MAX_CAMERA_CAPTURES: usize = 200;

impl MqttBridge {
    /// Create a new MQTT bridge.
    pub fn new(config: MqttBridgeConfig, fleet: Arc<Mutex<EdgeFleetManager>>) -> Self {
        Self {
            config,
            fleet,
            node_id_map: Arc::new(Mutex::new(std::collections::HashMap::new())),
            camera_captures: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Start the bridge. Connects to the MQTT broker, subscribes to the
    /// heartbeat/telemetry/status topics, and processes messages in a loop.
    ///
    /// This method runs until the connection is permanently lost or the
    /// task is cancelled. rumqttc handles reconnection automatically.
    pub async fn start(&self) -> Result<(), MqttBridgeError> {
        let mut mqtt_options = rumqttc::MqttOptions::new(
            &self.config.client_id,
            &self.config.broker_host,
            self.config.broker_port,
        );
        mqtt_options.set_keep_alive(Duration::from_secs(self.config.keepalive_secs));
        mqtt_options.set_clean_session(true);

        // Authentication
        if let (Some(user), Some(pass)) = (&self.config.username, &self.config.password) {
            mqtt_options.set_credentials(user.clone(), pass.clone());
        }

        // TLS configuration
        if self.config.use_tls {
            if let Some(ca_path) = &self.config.ca_cert_path {
                let ca_bytes = tokio::fs::read(ca_path).await.map_err(|e| {
                    MqttBridgeError::ConfigError(format!("failed to read CA cert: {}", e))
                })?;
                let transport = rumqttc::Transport::Tls(rumqttc::TlsConfiguration::Simple {
                    ca: ca_bytes,
                    alpn: None,
                    client_auth: None,
                });
                mqtt_options.set_transport(transport);
            } else {
                return Err(MqttBridgeError::ConfigError(
                    "TLS enabled but no ca_cert_path provided".into(),
                ));
            }
        }

        let (client, mut eventloop) = rumqttc::AsyncClient::new(mqtt_options, 64);

        // Subscribe to MCU topics using wildcard
        let heartbeat_topic = format!("{}/+/heartbeat", self.config.topic_prefix);
        let telemetry_topic = format!("{}/+/telemetry", self.config.topic_prefix);
        let status_topic = format!("{}/+/status", self.config.topic_prefix);

        client
            .subscribe(&heartbeat_topic, rumqttc::QoS::AtLeastOnce)
            .await
            .map_err(|e| MqttBridgeError::SubscribeError(e.to_string()))?;
        client
            .subscribe(&telemetry_topic, rumqttc::QoS::AtLeastOnce)
            .await
            .map_err(|e| MqttBridgeError::SubscribeError(e.to_string()))?;
        client
            .subscribe(&status_topic, rumqttc::QoS::AtLeastOnce)
            .await
            .map_err(|e| MqttBridgeError::SubscribeError(e.to_string()))?;

        // TinyML inference topics (E4)
        let inference_result_topic = format!("{}/+/inference/result", self.config.topic_prefix);
        let inference_status_topic = format!("{}/+/inference/status", self.config.topic_prefix);

        client
            .subscribe(&inference_result_topic, rumqttc::QoS::AtLeastOnce)
            .await
            .map_err(|e| MqttBridgeError::SubscribeError(e.to_string()))?;
        client
            .subscribe(&inference_status_topic, rumqttc::QoS::AtLeastOnce)
            .await
            .map_err(|e| MqttBridgeError::SubscribeError(e.to_string()))?;

        // ESP32-CAM camera topics (E3)
        let camera_frame_topic = format!("{}/+/camera/frame", self.config.topic_prefix);
        let camera_motion_topic = format!("{}/+/camera/motion", self.config.topic_prefix);

        client
            .subscribe(&camera_frame_topic, rumqttc::QoS::AtLeastOnce)
            .await
            .map_err(|e| MqttBridgeError::SubscribeError(e.to_string()))?;
        client
            .subscribe(&camera_motion_topic, rumqttc::QoS::AtLeastOnce)
            .await
            .map_err(|e| MqttBridgeError::SubscribeError(e.to_string()))?;

        info!(
            broker = %self.config.broker_host,
            port = %self.config.broker_port,
            prefix = %self.config.topic_prefix,
            "MQTT bridge started, subscribed to MCU topics"
        );

        // Event loop — process incoming messages
        loop {
            match eventloop.poll().await {
                Ok(event) => {
                    if let rumqttc::Event::Incoming(rumqttc::Packet::Publish(publish)) = event {
                        self.handle_publish(&publish);
                    }
                }
                Err(e) => {
                    warn!(error = %e, "MQTT connection error, rumqttc will retry");
                    // rumqttc retries automatically; we just log and continue.
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        }
    }

    /// Route an incoming MQTT publish to the appropriate handler based on
    /// the topic suffix.
    fn handle_publish(&self, publish: &rumqttc::Publish) {
        let topic = &publish.topic;
        let prefix = &self.config.topic_prefix;

        // Parse topic: "{prefix}/{node_id}/{message_type}"
        let Some(rest) = topic.strip_prefix(&format!("{}/", prefix)) else {
            debug!(topic = %topic, "Ignoring message with unexpected prefix");
            return;
        };

        let parts: Vec<&str> = rest.splitn(2, '/').collect();
        if parts.len() != 2 {
            debug!(topic = %topic, "Ignoring malformed topic");
            return;
        }

        let mcu_node_id = parts[0];
        let message_type = parts[1];

        // Validate node_id length to prevent abuse
        if mcu_node_id.is_empty() || mcu_node_id.len() > 64 {
            warn!(topic = %topic, "Ignoring message with invalid node_id length");
            return;
        }

        match message_type {
            "heartbeat" => self.handle_heartbeat(mcu_node_id, &publish.payload),
            "telemetry" => self.handle_telemetry(mcu_node_id, &publish.payload),
            "status" => self.handle_status(mcu_node_id, &publish.payload),
            "inference/result" => {
                self.handle_inference_result(mcu_node_id, &publish.payload);
            }
            "inference/status" => {
                self.handle_inference_status(mcu_node_id, &publish.payload);
            }
            "camera/frame" => {
                self.handle_camera_frame(mcu_node_id, &publish.payload);
            }
            "camera/motion" => {
                self.handle_camera_motion(mcu_node_id, &publish.payload);
            }
            other => {
                debug!(topic = %topic, msg_type = %other, "Ignoring unknown message type");
            }
        }
    }

    /// Process an MCU heartbeat: auto-register if new, update if existing.
    fn handle_heartbeat(&self, mcu_node_id: &str, payload: &[u8]) {
        let heartbeat: McuHeartbeat = match serde_json::from_slice(payload) {
            Ok(hb) => hb,
            Err(e) => {
                warn!(
                    node = %mcu_node_id,
                    error = %e,
                    "Failed to parse MCU heartbeat JSON"
                );
                return;
            }
        };

        let mut fleet = match self.fleet.lock() {
            Ok(f) => f,
            Err(e) => {
                error!(error = %e, "Fleet lock poisoned");
                return;
            }
        };

        let mut id_map = match self.node_id_map.lock() {
            Ok(m) => m,
            Err(e) => {
                error!(error = %e, "Node ID map lock poisoned");
                return;
            }
        };

        if let Some(fleet_id) = id_map.get(mcu_node_id) {
            // Existing node — send heartbeat
            let fleet_id = fleet_id.clone();
            if let Err(e) = fleet.heartbeat(
                &fleet_id, 0, // MCUs don't run tasks
                0, None, None, None, None,
            ) {
                warn!(
                    mcu_id = %mcu_node_id,
                    fleet_id = %fleet_id,
                    error = %e,
                    "Failed to process MCU heartbeat"
                );
            } else {
                debug!(mcu_id = %mcu_node_id, fleet_id = %fleet_id, "MCU heartbeat processed");

                // Update capabilities if sensors or WiFi changed
                let mut tags = heartbeat.sensors.clone();
                tags.push("esp32".into());
                tags.push("mqtt".into());
                if heartbeat.battery_pct.is_some() {
                    tags.push("battery".into());
                }

                let capabilities = EdgeCapabilities {
                    arch: heartbeat.arch.clone(),
                    cpu_cores: 1,
                    memory_mb: heartbeat.memory_kb / 1024,
                    disk_mb: 0,
                    has_gpu: false,
                    gpu_memory_mb: None,
                    gpu_compute_capability: None,
                    network_quality: wifi_rssi_to_quality(heartbeat.wifi_rssi),
                    location: None,
                    tags,
                };

                let _ = fleet.update_capabilities(&fleet_id, capabilities);
            }
        } else {
            // New node — check limit then auto-register
            if id_map.len() >= self.config.max_mcu_nodes {
                warn!(
                    mcu_id = %mcu_node_id,
                    max = self.config.max_mcu_nodes,
                    "MCU node limit reached, rejecting registration"
                );
                return;
            }

            let node_name = heartbeat
                .name
                .clone()
                .unwrap_or_else(|| format!("mcu-{}", mcu_node_id));

            let mut tags = heartbeat.sensors.clone();
            tags.push("esp32".into());
            tags.push("mqtt".into());
            tags.push("mcu".into());
            tags.push("no_std".into());
            if heartbeat.battery_pct.is_some() {
                tags.push("battery".into());
            }

            let capabilities = EdgeCapabilities {
                arch: heartbeat.arch.clone(),
                cpu_cores: 1,
                memory_mb: heartbeat.memory_kb / 1024,
                disk_mb: 0,
                has_gpu: false,
                gpu_memory_mb: None,
                gpu_compute_capability: None,
                network_quality: wifi_rssi_to_quality(heartbeat.wifi_rssi),
                location: None,
                tags,
            };

            let firmware = heartbeat
                .firmware_version
                .clone()
                .unwrap_or_else(|| "unknown".into());

            match fleet.register_node(
                node_name.clone(),
                capabilities,
                "esp32-agent".into(),
                firmware,
                "n/a".into(), // MCUs don't run AGNOS
                "mqtt".into(),
            ) {
                Ok(fleet_id) => {
                    info!(
                        mcu_id = %mcu_node_id,
                        fleet_id = %fleet_id,
                        name = %node_name,
                        arch = %heartbeat.arch,
                        "Auto-registered MCU node via MQTT"
                    );
                    id_map.insert(mcu_node_id.to_string(), fleet_id);
                }
                Err(e) => {
                    warn!(
                        mcu_id = %mcu_node_id,
                        error = %e,
                        "Failed to auto-register MCU node"
                    );
                }
            }
        }
    }

    /// Process MCU telemetry. Logs the readings; telemetry storage is handled
    /// by the existing daimon anomaly/metrics pipeline.
    fn handle_telemetry(&self, mcu_node_id: &str, payload: &[u8]) {
        let telemetry: McuTelemetry = match serde_json::from_slice(payload) {
            Ok(t) => t,
            Err(e) => {
                warn!(
                    node = %mcu_node_id,
                    error = %e,
                    "Failed to parse MCU telemetry JSON"
                );
                return;
            }
        };

        let id_map = match self.node_id_map.lock() {
            Ok(m) => m,
            Err(_) => return,
        };

        let fleet_id = id_map
            .get(mcu_node_id)
            .map(|s| s.as_str())
            .unwrap_or("unregistered");

        debug!(
            mcu_id = %mcu_node_id,
            fleet_id = %fleet_id,
            readings = ?telemetry.readings,
            "MCU telemetry received"
        );
    }

    /// Process MCU status events (boot, sleep, OTA, errors).
    fn handle_status(&self, mcu_node_id: &str, payload: &[u8]) {
        let status: McuStatus = match serde_json::from_slice(payload) {
            Ok(s) => s,
            Err(e) => {
                warn!(
                    node = %mcu_node_id,
                    error = %e,
                    "Failed to parse MCU status JSON"
                );
                return;
            }
        };

        let id_map = match self.node_id_map.lock() {
            Ok(m) => m,
            Err(_) => return,
        };

        let Some(fleet_id) = id_map.get(mcu_node_id) else {
            debug!(mcu_id = %mcu_node_id, "Status from unregistered MCU, ignoring");
            return;
        };
        let fleet_id = fleet_id.clone();
        drop(id_map);

        let mut fleet = match self.fleet.lock() {
            Ok(f) => f,
            Err(_) => return,
        };

        match status.event {
            McuStatusEvent::OtaStart => {
                info!(mcu_id = %mcu_node_id, fleet_id = %fleet_id, "MCU OTA update started");
                if let Some(node) = fleet.nodes.get_mut(&fleet_id) {
                    node.status = EdgeNodeStatus::Updating;
                }
            }
            McuStatusEvent::OtaComplete => {
                info!(mcu_id = %mcu_node_id, fleet_id = %fleet_id, "MCU OTA update complete");
                if let Some(node) = fleet.nodes.get_mut(&fleet_id) {
                    node.status = EdgeNodeStatus::Online;
                    node.last_heartbeat = Utc::now();
                }
            }
            McuStatusEvent::OtaFailed => {
                warn!(
                    mcu_id = %mcu_node_id,
                    fleet_id = %fleet_id,
                    message = ?status.message,
                    "MCU OTA update failed"
                );
                if let Some(node) = fleet.nodes.get_mut(&fleet_id) {
                    node.status = EdgeNodeStatus::Online;
                }
            }
            McuStatusEvent::Sleep => {
                info!(mcu_id = %mcu_node_id, fleet_id = %fleet_id, "MCU entering deep sleep");
                // Don't mark offline — the node will wake and heartbeat again.
                // The watchdog timer will handle actual offline detection.
            }
            McuStatusEvent::Boot | McuStatusEvent::Wake => {
                info!(
                    mcu_id = %mcu_node_id,
                    fleet_id = %fleet_id,
                    event = ?status.event,
                    "MCU lifecycle event"
                );
                if let Some(node) = fleet.nodes.get_mut(&fleet_id) {
                    node.status = EdgeNodeStatus::Online;
                    node.last_heartbeat = Utc::now();
                }
            }
            McuStatusEvent::Error => {
                warn!(
                    mcu_id = %mcu_node_id,
                    fleet_id = %fleet_id,
                    message = ?status.message,
                    "MCU reported error"
                );
            }
        }
    }

    /// Process a TinyML inference result from an ESP32-S3 node.
    ///
    /// Logs the result and updates the fleet node's tags with the "tinyml"
    /// marker so the dashboard can identify inference-capable nodes. High-
    /// confidence results are logged at info level for aggregation by the
    /// daimon metrics pipeline.
    fn handle_inference_result(&self, mcu_node_id: &str, payload: &[u8]) {
        let result: McuInferenceResult = match serde_json::from_slice(payload) {
            Ok(r) => r,
            Err(e) => {
                warn!(
                    node = %mcu_node_id,
                    error = %e,
                    "Failed to parse MCU inference result JSON"
                );
                return;
            }
        };

        let id_map = match self.node_id_map.lock() {
            Ok(m) => m,
            Err(_) => return,
        };

        let fleet_id = id_map
            .get(mcu_node_id)
            .map(|s| s.as_str())
            .unwrap_or("unregistered");

        info!(
            mcu_id = %mcu_node_id,
            fleet_id = %fleet_id,
            model = %result.model_name,
            label = %result.label,
            confidence = %result.confidence,
            latency_ms = %result.latency_ms,
            input_type = ?result.input_type,
            "MCU TinyML inference result"
        );

        // Tag the fleet node as tinyml-capable if registered
        if fleet_id != "unregistered" {
            let fleet_id_owned = fleet_id.to_string();
            drop(id_map);

            if let Ok(mut fleet) = self.fleet.lock() {
                if let Some(node) = fleet.nodes.get_mut(&fleet_id_owned) {
                    if !node.capabilities.tags.contains(&"tinyml".to_string()) {
                        node.capabilities.tags.push("tinyml".into());
                    }
                    // Add the specific model type tag (e.g. "tinyml:kws")
                    let model_tag = format!("tinyml:{}", result.model_name);
                    if !node.capabilities.tags.contains(&model_tag) {
                        node.capabilities.tags.push(model_tag);
                    }
                }
            }
        }
    }

    /// Process a TinyML inference status report from an ESP32-S3 node.
    ///
    /// Tracks model load state, memory usage, and inference throughput.
    /// Forwarded to daimon metrics for dashboard display.
    fn handle_inference_status(&self, mcu_node_id: &str, payload: &[u8]) {
        let status: McuInferenceStatus = match serde_json::from_slice(payload) {
            Ok(s) => s,
            Err(e) => {
                warn!(
                    node = %mcu_node_id,
                    error = %e,
                    "Failed to parse MCU inference status JSON"
                );
                return;
            }
        };

        let id_map = match self.node_id_map.lock() {
            Ok(m) => m,
            Err(_) => return,
        };

        let fleet_id = id_map
            .get(mcu_node_id)
            .map(|s| s.as_str())
            .unwrap_or("unregistered");

        if status.model_loaded {
            info!(
                mcu_id = %mcu_node_id,
                fleet_id = %fleet_id,
                model = %status.model_name,
                model_type = ?status.model_type,
                memory_bytes = %status.memory_used_bytes,
                inferences = %status.inference_count,
                avg_latency_ms = ?status.avg_latency_ms,
                "MCU TinyML model status: loaded"
            );
        } else {
            debug!(
                mcu_id = %mcu_node_id,
                fleet_id = %fleet_id,
                "MCU TinyML model status: no model loaded"
            );
        }
    }

    /// Process a camera frame from an ESP32-CAM node.
    ///
    /// Stores the JPEG frame as a [`CameraCaptureEvent`] in the bridge's
    /// in-memory capture buffer. The frame can later be forwarded to daimon's
    /// screen capture API (POST /v1/screen/capture) or queried directly.
    ///
    /// Frames larger than 1 MB (base64) are rejected to prevent memory abuse
    /// from rogue publishers.
    fn handle_camera_frame(&self, mcu_node_id: &str, payload: &[u8]) {
        let frame: McuCameraFrame = match serde_json::from_slice(payload) {
            Ok(f) => f,
            Err(e) => {
                warn!(
                    node = %mcu_node_id,
                    error = %e,
                    "Failed to parse MCU camera frame JSON"
                );
                return;
            }
        };

        // Reject oversized frames (1 MB base64 ~ 750 KB raw JPEG)
        if frame.data_base64.len() > 1_048_576 {
            warn!(
                node = %mcu_node_id,
                size = frame.data_base64.len(),
                "Camera frame too large, rejecting (max 1 MB base64)"
            );
            return;
        }

        let id_map = match self.node_id_map.lock() {
            Ok(m) => m,
            Err(_) => return,
        };

        let fleet_id = id_map.get(mcu_node_id).cloned();
        let fleet_id_str = fleet_id.as_deref().unwrap_or("unregistered");

        info!(
            mcu_id = %mcu_node_id,
            fleet_id = %fleet_id_str,
            width = frame.width,
            height = frame.height,
            quality = frame.jpeg_quality,
            trigger = %frame.trigger,
            data_size = frame.data_base64.len(),
            "ESP32-CAM frame received"
        );

        // Tag the fleet node as camera-capable if registered
        if let Some(ref fid) = fleet_id {
            let fid = fid.clone();
            drop(id_map);

            if let Ok(mut fleet) = self.fleet.lock() {
                if let Some(node) = fleet.nodes.get_mut(&fid) {
                    if !node.capabilities.tags.contains(&"camera".to_string()) {
                        node.capabilities.tags.push("camera".into());
                    }
                }
            }
        } else {
            drop(id_map);
        }

        // Store as capture event
        let event = CameraCaptureEvent {
            id: uuid::Uuid::new_v4().to_string(),
            mcu_node_id: mcu_node_id.to_string(),
            fleet_id: fleet_id.clone(),
            data_base64: frame.data_base64,
            width: frame.width,
            height: frame.height,
            trigger: frame.trigger,
            received_at: Utc::now(),
            motion_intensity: None,
        };

        if let Ok(mut captures) = self.camera_captures.lock() {
            if captures.len() >= MAX_CAMERA_CAPTURES {
                captures.remove(0);
            }
            captures.push(event);
        }
    }

    /// Process a motion detection event from an ESP32-CAM node.
    ///
    /// Logs the motion event and, if a snapshot is attached, stores it as a
    /// capture event. Motion events without snapshots are still logged for
    /// the daimon anomaly pipeline to pick up.
    fn handle_camera_motion(&self, mcu_node_id: &str, payload: &[u8]) {
        let motion: McuMotionEvent = match serde_json::from_slice(payload) {
            Ok(m) => m,
            Err(e) => {
                warn!(
                    node = %mcu_node_id,
                    error = %e,
                    "Failed to parse MCU motion event JSON"
                );
                return;
            }
        };

        let id_map = match self.node_id_map.lock() {
            Ok(m) => m,
            Err(_) => return,
        };

        let fleet_id = id_map.get(mcu_node_id).cloned();
        let fleet_id_str = fleet_id.as_deref().unwrap_or("unregistered");

        info!(
            mcu_id = %mcu_node_id,
            fleet_id = %fleet_id_str,
            detected = motion.detected,
            intensity = ?motion.intensity,
            source = %motion.source,
            has_snapshot = motion.snapshot_base64.is_some(),
            "ESP32-CAM motion event"
        );

        // Tag the fleet node as motion-capable
        if let Some(ref fid) = fleet_id {
            let fid = fid.clone();
            drop(id_map);

            if let Ok(mut fleet) = self.fleet.lock() {
                if let Some(node) = fleet.nodes.get_mut(&fid) {
                    for tag in ["camera", "motion_detect"] {
                        if !node.capabilities.tags.contains(&tag.to_string()) {
                            node.capabilities.tags.push(tag.into());
                        }
                    }
                }
            }
        } else {
            drop(id_map);
        }

        // If a snapshot is attached, store it as a capture event
        if let Some(snapshot_data) = motion.snapshot_base64 {
            // Reject oversized snapshots
            if snapshot_data.len() > 1_048_576 {
                warn!(
                    node = %mcu_node_id,
                    size = snapshot_data.len(),
                    "Motion snapshot too large, skipping storage"
                );
                return;
            }

            let event = CameraCaptureEvent {
                id: uuid::Uuid::new_v4().to_string(),
                mcu_node_id: mcu_node_id.to_string(),
                fleet_id: fleet_id.clone(),
                data_base64: snapshot_data,
                width: motion.snapshot_width.unwrap_or(0),
                height: motion.snapshot_height.unwrap_or(0),
                trigger: "motion".to_string(),
                received_at: Utc::now(),
                motion_intensity: motion.intensity,
            };

            if let Ok(mut captures) = self.camera_captures.lock() {
                if captures.len() >= MAX_CAMERA_CAPTURES {
                    captures.remove(0);
                }
                captures.push(event);
            }
        }
    }

    /// Get the fleet node ID for a given MCU node_id, if registered.
    pub fn get_fleet_id(&self, mcu_node_id: &str) -> Option<String> {
        self.node_id_map
            .lock()
            .ok()
            .and_then(|m| m.get(mcu_node_id).cloned())
    }

    /// Number of MCU nodes currently tracked by the bridge.
    pub fn tracked_node_count(&self) -> usize {
        self.node_id_map.lock().map(|m| m.len()).unwrap_or(0)
    }

    /// Get all stored camera capture events.
    pub fn camera_captures(&self) -> Vec<CameraCaptureEvent> {
        self.camera_captures
            .lock()
            .map(|c| c.clone())
            .unwrap_or_default()
    }

    /// Get camera captures for a specific MCU node.
    pub fn camera_captures_for_node(&self, mcu_node_id: &str) -> Vec<CameraCaptureEvent> {
        self.camera_captures
            .lock()
            .map(|c| {
                c.iter()
                    .filter(|e| e.mcu_node_id == mcu_node_id)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Convert WiFi RSSI (dBm) to a 0.0-1.0 network quality score.
///
/// Rough mapping:
///   -30 dBm or better  = 1.0 (excellent)
///   -50 dBm            = 0.8
///   -70 dBm            = 0.5
///   -90 dBm or worse   = 0.1 (very poor)
fn wifi_rssi_to_quality(rssi: Option<i32>) -> f64 {
    match rssi {
        Some(r) if r >= -30 => 1.0,
        Some(r) if r >= -50 => 0.8,
        Some(r) if r >= -60 => 0.7,
        Some(r) if r >= -70 => 0.5,
        Some(r) if r >= -80 => 0.3,
        Some(r) if r >= -90 => 0.1,
        Some(_) => 0.05,
        None => 0.5, // Unknown, assume moderate
    }
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors from the MQTT bridge.
#[derive(Debug)]
pub enum MqttBridgeError {
    /// Configuration is invalid.
    ConfigError(String),
    /// Failed to subscribe to MQTT topics.
    SubscribeError(String),
    /// Connection to the MQTT broker failed permanently.
    ConnectionError(String),
}

impl std::fmt::Display for MqttBridgeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ConfigError(msg) => write!(f, "MQTT bridge config error: {}", msg),
            Self::SubscribeError(msg) => write!(f, "MQTT subscribe error: {}", msg),
            Self::ConnectionError(msg) => write!(f, "MQTT connection error: {}", msg),
        }
    }
}

impl std::error::Error for MqttBridgeError {}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edge::types::EdgeFleetConfig;

    fn make_bridge() -> MqttBridge {
        let config = MqttBridgeConfig::default();
        let fleet = Arc::new(Mutex::new(
            EdgeFleetManager::new(EdgeFleetConfig::default()),
        ));
        MqttBridge::new(config, fleet)
    }

    fn make_heartbeat_payload(arch: &str, memory_kb: u64, rssi: Option<i32>) -> Vec<u8> {
        let hb = serde_json::json!({
            "arch": arch,
            "memory_kb": memory_kb,
            "uptime_secs": 3600,
            "wifi_rssi": rssi,
            "battery_pct": 85,
            "sensors": ["temperature", "humidity"],
            "firmware_version": "2026.3.18",
            "name": "sensor-kitchen"
        });
        serde_json::to_vec(&hb).unwrap()
    }

    fn make_publish(topic: &str, payload: Vec<u8>) -> rumqttc::Publish {
        let mut publish = rumqttc::Publish::new(topic, rumqttc::QoS::AtLeastOnce, payload);
        publish.dup = false;
        publish.retain = false;
        publish
    }

    #[test]
    fn test_heartbeat_auto_registers_new_node() {
        let bridge = make_bridge();
        let payload = make_heartbeat_payload("xtensa", 8704, Some(-42));
        let publish = make_publish("agnos/esp32-001/heartbeat", payload);

        bridge.handle_publish(&publish);

        // Node should be registered in fleet
        let fleet = bridge.fleet.lock().unwrap();
        assert_eq!(fleet.nodes.len(), 1);

        let node = fleet.nodes.values().next().unwrap();
        assert_eq!(node.name, "sensor-kitchen");
        assert_eq!(node.agent_binary, "esp32-agent");
        assert_eq!(node.agent_version, "2026.3.18");
        assert_eq!(node.capabilities.arch, "xtensa");
        assert_eq!(node.capabilities.memory_mb, 8); // 8704 KB / 1024
        assert!(!node.capabilities.has_gpu);
        assert!(node.capabilities.tags.contains(&"esp32".to_string()));
        assert!(node.capabilities.tags.contains(&"mqtt".to_string()));
        assert!(node.capabilities.tags.contains(&"mcu".to_string()));
        assert!(node.capabilities.tags.contains(&"temperature".to_string()));
        assert_eq!(node.status, EdgeNodeStatus::Online);

        // MCU node_id should be mapped
        assert!(bridge.get_fleet_id("esp32-001").is_some());
        assert_eq!(bridge.tracked_node_count(), 1);
    }

    #[test]
    fn test_subsequent_heartbeat_updates_existing_node() {
        let bridge = make_bridge();
        let payload = make_heartbeat_payload("xtensa", 512, Some(-50));
        let publish = make_publish("agnos/esp32-002/heartbeat", payload);

        // First heartbeat — registers
        bridge.handle_publish(&publish);
        let fleet_id = bridge.get_fleet_id("esp32-002").unwrap();

        // Verify initial heartbeat timestamp
        let ts1 = {
            let fleet = bridge.fleet.lock().unwrap();
            fleet.nodes[&fleet_id].last_heartbeat
        };

        // Second heartbeat — updates
        std::thread::sleep(std::time::Duration::from_millis(10));
        let payload2 = make_heartbeat_payload("xtensa", 512, Some(-60));
        let publish2 = make_publish("agnos/esp32-002/heartbeat", payload2);
        bridge.handle_publish(&publish2);

        let fleet = bridge.fleet.lock().unwrap();
        assert_eq!(fleet.nodes.len(), 1); // Still 1 node, not 2
        assert!(fleet.nodes[&fleet_id].last_heartbeat >= ts1);
    }

    #[test]
    fn test_riscv32_esp32c3_heartbeat() {
        let bridge = make_bridge();
        let hb = serde_json::json!({
            "arch": "riscv32",
            "memory_kb": 400,
            "uptime_secs": 120,
            "sensors": ["temperature"],
        });
        let publish = make_publish("agnos/c3-node/heartbeat", serde_json::to_vec(&hb).unwrap());

        bridge.handle_publish(&publish);

        let fleet = bridge.fleet.lock().unwrap();
        let node = fleet.nodes.values().next().unwrap();
        assert_eq!(node.capabilities.arch, "riscv32");
        assert_eq!(node.name, "mcu-c3-node"); // Fallback name
    }

    #[test]
    fn test_telemetry_message_parsed() {
        let bridge = make_bridge();

        // Register first via heartbeat
        let hb_payload = make_heartbeat_payload("xtensa", 512, None);
        bridge.handle_publish(&make_publish("agnos/esp32-t/heartbeat", hb_payload));

        // Send telemetry
        let telemetry = serde_json::json!({
            "readings": {
                "temperature": 22.5,
                "humidity": 65.0,
                "pressure": 1013.25
            },
            "timestamp_secs": 1710720000
        });
        let publish = make_publish(
            "agnos/esp32-t/telemetry",
            serde_json::to_vec(&telemetry).unwrap(),
        );

        // Should not panic
        bridge.handle_publish(&publish);
    }

    #[test]
    fn test_status_ota_lifecycle() {
        let bridge = make_bridge();

        // Register via heartbeat
        let hb = make_heartbeat_payload("xtensa", 512, Some(-40));
        bridge.handle_publish(&make_publish("agnos/esp32-ota/heartbeat", hb));
        let fleet_id = bridge.get_fleet_id("esp32-ota").unwrap();

        // OTA start -> Updating
        let ota_start = serde_json::json!({ "event": "ota_start" });
        bridge.handle_publish(&make_publish(
            "agnos/esp32-ota/status",
            serde_json::to_vec(&ota_start).unwrap(),
        ));
        {
            let fleet = bridge.fleet.lock().unwrap();
            assert_eq!(fleet.nodes[&fleet_id].status, EdgeNodeStatus::Updating);
        }

        // OTA complete -> Online
        let ota_done = serde_json::json!({ "event": "ota_complete" });
        bridge.handle_publish(&make_publish(
            "agnos/esp32-ota/status",
            serde_json::to_vec(&ota_done).unwrap(),
        ));
        {
            let fleet = bridge.fleet.lock().unwrap();
            assert_eq!(fleet.nodes[&fleet_id].status, EdgeNodeStatus::Online);
        }
    }

    #[test]
    fn test_status_sleep_does_not_mark_offline() {
        let bridge = make_bridge();

        let hb = make_heartbeat_payload("riscv32", 400, Some(-55));
        bridge.handle_publish(&make_publish("agnos/esp32-sleep/heartbeat", hb));
        let fleet_id = bridge.get_fleet_id("esp32-sleep").unwrap();

        let sleep_msg = serde_json::json!({ "event": "sleep" });
        bridge.handle_publish(&make_publish(
            "agnos/esp32-sleep/status",
            serde_json::to_vec(&sleep_msg).unwrap(),
        ));

        let fleet = bridge.fleet.lock().unwrap();
        // Should still be Online, not Offline
        assert_eq!(fleet.nodes[&fleet_id].status, EdgeNodeStatus::Online);
    }

    #[test]
    fn test_malformed_json_does_not_panic() {
        let bridge = make_bridge();

        // Invalid JSON
        let publish = make_publish("agnos/bad/heartbeat", b"not json".to_vec());
        bridge.handle_publish(&publish);

        // Empty payload
        let publish2 = make_publish("agnos/bad2/heartbeat", vec![]);
        bridge.handle_publish(&publish2);

        // Valid JSON but wrong schema
        let publish3 = make_publish(
            "agnos/bad3/heartbeat",
            serde_json::to_vec(&serde_json::json!({"foo": "bar"})).unwrap(),
        );
        bridge.handle_publish(&publish3);

        // Fleet should be empty — none registered
        let fleet = bridge.fleet.lock().unwrap();
        assert_eq!(fleet.nodes.len(), 0);
    }

    #[test]
    fn test_ignores_wrong_prefix() {
        let bridge = make_bridge();
        let payload = make_heartbeat_payload("xtensa", 512, None);
        let publish = make_publish("other/esp32/heartbeat", payload);

        bridge.handle_publish(&publish);

        let fleet = bridge.fleet.lock().unwrap();
        assert_eq!(fleet.nodes.len(), 0);
    }

    #[test]
    fn test_ignores_invalid_node_id() {
        let bridge = make_bridge();
        let payload = make_heartbeat_payload("xtensa", 512, None);

        // Empty node_id
        let publish = make_publish("agnos//heartbeat", payload.clone());
        bridge.handle_publish(&publish);

        // Oversized node_id (65 chars)
        let long_id = "a".repeat(65);
        let topic = format!("agnos/{}/heartbeat", long_id);
        let publish2 = make_publish(&topic, payload);
        bridge.handle_publish(&publish2);

        let fleet = bridge.fleet.lock().unwrap();
        assert_eq!(fleet.nodes.len(), 0);
    }

    #[test]
    fn test_max_mcu_nodes_limit() {
        let config = MqttBridgeConfig {
            max_mcu_nodes: 2,
            ..Default::default()
        };
        let fleet = Arc::new(Mutex::new(EdgeFleetManager::new(EdgeFleetConfig {
            max_nodes: 1000,
            ..Default::default()
        })));
        let bridge = MqttBridge::new(config, fleet);

        // Register 2 nodes (at limit)
        for i in 0..2 {
            let hb = serde_json::json!({
                "arch": "xtensa",
                "memory_kb": 512,
                "uptime_secs": 100,
            });
            let publish = make_publish(
                &format!("agnos/node-{}/heartbeat", i),
                serde_json::to_vec(&hb).unwrap(),
            );
            bridge.handle_publish(&publish);
        }
        assert_eq!(bridge.tracked_node_count(), 2);

        // Third node should be rejected
        let hb = serde_json::json!({
            "arch": "xtensa",
            "memory_kb": 512,
            "uptime_secs": 100,
        });
        let publish = make_publish(
            "agnos/node-rejected/heartbeat",
            serde_json::to_vec(&hb).unwrap(),
        );
        bridge.handle_publish(&publish);
        assert_eq!(bridge.tracked_node_count(), 2);
    }

    #[test]
    fn test_wifi_rssi_to_quality() {
        assert_eq!(wifi_rssi_to_quality(Some(-25)), 1.0);
        assert_eq!(wifi_rssi_to_quality(Some(-30)), 1.0);
        assert_eq!(wifi_rssi_to_quality(Some(-42)), 0.8);
        assert_eq!(wifi_rssi_to_quality(Some(-55)), 0.7);
        assert_eq!(wifi_rssi_to_quality(Some(-65)), 0.5);
        assert_eq!(wifi_rssi_to_quality(Some(-75)), 0.3);
        assert_eq!(wifi_rssi_to_quality(Some(-85)), 0.1);
        assert_eq!(wifi_rssi_to_quality(Some(-95)), 0.05);
        assert_eq!(wifi_rssi_to_quality(None), 0.5);
    }

    #[test]
    fn test_config_defaults() {
        let config = MqttBridgeConfig::default();
        assert_eq!(config.broker_host, "localhost");
        assert_eq!(config.broker_port, 1883);
        assert_eq!(config.topic_prefix, "agnos");
        assert_eq!(config.client_id, "daimon-mqtt-bridge");
        assert!(!config.use_tls);
        assert_eq!(config.max_mcu_nodes, 500);
    }

    #[test]
    fn test_multiple_nodes_coexist() {
        let bridge = make_bridge();

        // Register 3 different MCU nodes
        for (id, arch) in [
            ("s3-01", "xtensa"),
            ("s3-02", "xtensa"),
            ("c3-01", "riscv32"),
        ] {
            let hb = serde_json::json!({
                "arch": arch,
                "memory_kb": 512,
                "uptime_secs": 100,
                "sensors": ["temperature"],
            });
            let publish = make_publish(
                &format!("agnos/{}/heartbeat", id),
                serde_json::to_vec(&hb).unwrap(),
            );
            bridge.handle_publish(&publish);
        }

        let fleet = bridge.fleet.lock().unwrap();
        assert_eq!(fleet.nodes.len(), 3);
        assert_eq!(bridge.tracked_node_count(), 3);

        // Verify each has a unique fleet ID
        let ids: std::collections::HashSet<_> = fleet.nodes.keys().collect();
        assert_eq!(ids.len(), 3);
    }

    // -----------------------------------------------------------------------
    // TinyML inference tests (E4)
    // -----------------------------------------------------------------------

    #[test]
    fn test_inference_result_parsed_and_tags_fleet_node() {
        let bridge = make_bridge();

        // Register via heartbeat first
        let hb = make_heartbeat_payload("xtensa", 8704, Some(-40));
        bridge.handle_publish(&make_publish("agnos/s3-ml/heartbeat", hb));
        let fleet_id = bridge.get_fleet_id("s3-ml").unwrap();

        // Send inference result
        let result = serde_json::json!({
            "model_name": "kws_micro_speech",
            "label": "yes",
            "confidence": 0.92,
            "latency_ms": 18,
            "input_type": "audio",
            "timestamp_secs": 1710720000
        });
        let publish = make_publish(
            "agnos/s3-ml/inference/result",
            serde_json::to_vec(&result).unwrap(),
        );
        bridge.handle_publish(&publish);

        // Fleet node should be tagged with "tinyml" and "tinyml:kws_micro_speech"
        let fleet = bridge.fleet.lock().unwrap();
        let node = &fleet.nodes[&fleet_id];
        assert!(node.capabilities.tags.contains(&"tinyml".to_string()));
        assert!(node
            .capabilities
            .tags
            .contains(&"tinyml:kws_micro_speech".to_string()));
    }

    #[test]
    fn test_inference_result_no_duplicate_tags() {
        let bridge = make_bridge();

        // Register
        let hb = make_heartbeat_payload("xtensa", 8704, Some(-45));
        bridge.handle_publish(&make_publish("agnos/s3-dup/heartbeat", hb));
        let fleet_id = bridge.get_fleet_id("s3-dup").unwrap();

        // Send same inference result twice
        let result = serde_json::json!({
            "model_name": "gesture_wave",
            "label": "wave",
            "confidence": 0.85,
            "latency_ms": 22,
            "input_type": "imu"
        });
        let payload = serde_json::to_vec(&result).unwrap();
        bridge.handle_publish(&make_publish(
            "agnos/s3-dup/inference/result",
            payload.clone(),
        ));
        bridge.handle_publish(&make_publish("agnos/s3-dup/inference/result", payload));

        // Tags should not be duplicated
        let fleet = bridge.fleet.lock().unwrap();
        let tags = &fleet.nodes[&fleet_id].capabilities.tags;
        let tinyml_count = tags.iter().filter(|t| *t == "tinyml").count();
        assert_eq!(tinyml_count, 1);
    }

    #[test]
    fn test_inference_result_from_unregistered_node_does_not_panic() {
        let bridge = make_bridge();

        // No heartbeat — node is unregistered
        let result = serde_json::json!({
            "model_name": "anomaly_sensor",
            "label": "anomaly_high",
            "confidence": 0.78,
            "latency_ms": 12,
            "input_type": "imu"
        });
        let publish = make_publish(
            "agnos/unknown-ml/inference/result",
            serde_json::to_vec(&result).unwrap(),
        );

        // Should not panic
        bridge.handle_publish(&publish);
    }

    #[test]
    fn test_inference_status_parsed() {
        let bridge = make_bridge();

        // Register via heartbeat
        let hb = make_heartbeat_payload("xtensa", 8704, Some(-38));
        bridge.handle_publish(&make_publish("agnos/s3-status/heartbeat", hb));

        // Send inference status (model loaded)
        let status = serde_json::json!({
            "model_loaded": true,
            "model_name": "kws_micro_speech",
            "memory_used_bytes": 98304,
            "inference_count": 1500,
            "model_type": "kws",
            "avg_latency_ms": 19
        });
        let publish = make_publish(
            "agnos/s3-status/inference/status",
            serde_json::to_vec(&status).unwrap(),
        );

        // Should not panic
        bridge.handle_publish(&publish);
    }

    #[test]
    fn test_inference_status_no_model_loaded() {
        let bridge = make_bridge();

        let hb = make_heartbeat_payload("xtensa", 512, None);
        bridge.handle_publish(&make_publish("agnos/s3-noml/heartbeat", hb));

        // Send status with no model loaded
        let status = serde_json::json!({
            "model_loaded": false,
            "model_name": "",
            "memory_used_bytes": 0,
            "inference_count": 0
        });
        let publish = make_publish(
            "agnos/s3-noml/inference/status",
            serde_json::to_vec(&status).unwrap(),
        );
        bridge.handle_publish(&publish);
    }

    #[test]
    fn test_inference_malformed_json_does_not_panic() {
        let bridge = make_bridge();

        // Malformed inference result
        let publish = make_publish("agnos/bad-ml/inference/result", b"not json".to_vec());
        bridge.handle_publish(&publish);

        // Malformed inference status
        let publish2 = make_publish("agnos/bad-ml/inference/status", b"{}".to_vec());
        bridge.handle_publish(&publish2);
    }

    #[test]
    fn test_inference_result_all_input_types() {
        let bridge = make_bridge();

        let hb = make_heartbeat_payload("xtensa", 8704, Some(-35));
        bridge.handle_publish(&make_publish("agnos/s3-inputs/heartbeat", hb));

        for (input, label) in [("audio", "yes"), ("imu", "wave"), ("image", "person")] {
            let result = serde_json::json!({
                "model_name": format!("model_{}", input),
                "label": label,
                "confidence": 0.9,
                "latency_ms": 15,
                "input_type": input
            });
            let publish = make_publish(
                "agnos/s3-inputs/inference/result",
                serde_json::to_vec(&result).unwrap(),
            );
            bridge.handle_publish(&publish);
        }

        // All three model tags should be present
        let fleet = bridge.fleet.lock().unwrap();
        let fleet_id = bridge.get_fleet_id("s3-inputs").unwrap();
        let tags = &fleet.nodes[&fleet_id].capabilities.tags;
        assert!(tags.contains(&"tinyml:model_audio".to_string()));
        assert!(tags.contains(&"tinyml:model_imu".to_string()));
        assert!(tags.contains(&"tinyml:model_image".to_string()));
    }

    #[test]
    fn test_inference_result_deserialization() {
        let json = serde_json::json!({
            "model_name": "kws_micro_speech",
            "label": "yes",
            "confidence": 0.95,
            "latency_ms": 16,
            "input_type": "audio",
            "timestamp_secs": 1710720000
        });
        let result: McuInferenceResult = serde_json::from_value(json).unwrap();
        assert_eq!(result.model_name, "kws_micro_speech");
        assert_eq!(result.label, "yes");
        assert!((result.confidence - 0.95).abs() < f64::EPSILON);
        assert_eq!(result.latency_ms, 16);
        assert_eq!(result.input_type, McuInferenceInputType::Audio);
        assert_eq!(result.timestamp_secs, Some(1710720000));
    }

    #[test]
    fn test_inference_status_deserialization() {
        let json = serde_json::json!({
            "model_loaded": true,
            "model_name": "gesture_wave",
            "memory_used_bytes": 65536,
            "inference_count": 42,
            "model_type": "gesture",
            "avg_latency_ms": 21
        });
        let status: McuInferenceStatus = serde_json::from_value(json).unwrap();
        assert!(status.model_loaded);
        assert_eq!(status.model_name, "gesture_wave");
        assert_eq!(status.memory_used_bytes, 65536);
        assert_eq!(status.inference_count, 42);
        assert_eq!(status.model_type.as_deref(), Some("gesture"));
        assert_eq!(status.avg_latency_ms, Some(21));
    }

    #[test]
    fn test_inference_input_type_deserialization() {
        for (s, expected) in [
            ("\"audio\"", McuInferenceInputType::Audio),
            ("\"imu\"", McuInferenceInputType::Imu),
            ("\"image\"", McuInferenceInputType::Image),
        ] {
            let parsed: McuInferenceInputType = serde_json::from_str(s).unwrap();
            assert_eq!(parsed, expected);
        }
    }

    // -----------------------------------------------------------------------
    // ESP32-CAM camera tests (E3)
    // -----------------------------------------------------------------------

    fn make_camera_frame_payload(width: u32, height: u32, trigger: &str) -> Vec<u8> {
        let frame = serde_json::json!({
            "data_base64": "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJ",
            "width": width,
            "height": height,
            "jpeg_quality": 20,
            "trigger": trigger,
            "timestamp_secs": 1710720000
        });
        serde_json::to_vec(&frame).unwrap()
    }

    fn make_motion_event_payload(intensity: f64, source: &str, with_snapshot: bool) -> Vec<u8> {
        let mut event = serde_json::json!({
            "detected": true,
            "intensity": intensity,
            "source": source,
            "timestamp_secs": 1710720000
        });
        if with_snapshot {
            event["snapshot_base64"] = serde_json::Value::String("iVBORw0KGgoAAAANSUhEUg==".into());
            event["snapshot_width"] = serde_json::json!(640);
            event["snapshot_height"] = serde_json::json!(480);
        }
        serde_json::to_vec(&event).unwrap()
    }

    #[test]
    fn test_camera_frame_stored_as_capture_event() {
        let bridge = make_bridge();

        // Register via heartbeat
        let hb = make_heartbeat_payload("xtensa", 8704, Some(-40));
        bridge.handle_publish(&make_publish("agnos/cam-01/heartbeat", hb));
        let fleet_id = bridge.get_fleet_id("cam-01").unwrap();

        // Send camera frame
        let payload = make_camera_frame_payload(640, 480, "motion");
        bridge.handle_publish(&make_publish("agnos/cam-01/camera/frame", payload));

        // Capture should be stored
        let captures = bridge.camera_captures();
        assert_eq!(captures.len(), 1);
        assert_eq!(captures[0].mcu_node_id, "cam-01");
        assert_eq!(captures[0].fleet_id, Some(fleet_id.clone()));
        assert_eq!(captures[0].width, 640);
        assert_eq!(captures[0].height, 480);
        assert_eq!(captures[0].trigger, "motion");
        assert!(!captures[0].data_base64.is_empty());

        // Fleet node should be tagged with "camera"
        let fleet = bridge.fleet.lock().unwrap();
        let node = &fleet.nodes[&fleet_id];
        assert!(node.capabilities.tags.contains(&"camera".to_string()));
    }

    #[test]
    fn test_camera_frame_from_unregistered_node() {
        let bridge = make_bridge();

        // No heartbeat — node is unregistered
        let payload = make_camera_frame_payload(320, 240, "interval");
        bridge.handle_publish(&make_publish("agnos/cam-unreg/camera/frame", payload));

        // Capture should still be stored (fleet_id = None)
        let captures = bridge.camera_captures();
        assert_eq!(captures.len(), 1);
        assert_eq!(captures[0].mcu_node_id, "cam-unreg");
        assert!(captures[0].fleet_id.is_none());
    }

    #[test]
    fn test_camera_frame_oversized_rejected() {
        let bridge = make_bridge();

        // Create a frame with > 1 MB base64 data
        let large_data = "A".repeat(1_100_000);
        let frame = serde_json::json!({
            "data_base64": large_data,
            "width": 1280,
            "height": 1024,
            "jpeg_quality": 63,
            "trigger": "manual"
        });
        let payload = serde_json::to_vec(&frame).unwrap();
        bridge.handle_publish(&make_publish("agnos/cam-big/camera/frame", payload));

        // Should be rejected — no capture stored
        let captures = bridge.camera_captures();
        assert_eq!(captures.len(), 0);
    }

    #[test]
    fn test_camera_motion_event_logged() {
        let bridge = make_bridge();

        // Register via heartbeat
        let hb = make_heartbeat_payload("xtensa", 8704, Some(-45));
        bridge.handle_publish(&make_publish("agnos/cam-motion/heartbeat", hb));
        let fleet_id = bridge.get_fleet_id("cam-motion").unwrap();

        // Send motion event without snapshot
        let payload = make_motion_event_payload(72.5, "frame_diff", false);
        bridge.handle_publish(&make_publish("agnos/cam-motion/camera/motion", payload));

        // No capture stored (no snapshot attached)
        let captures = bridge.camera_captures();
        assert_eq!(captures.len(), 0);

        // Fleet node should be tagged with "camera" and "motion_detect"
        let fleet = bridge.fleet.lock().unwrap();
        let node = &fleet.nodes[&fleet_id];
        assert!(node.capabilities.tags.contains(&"camera".to_string()));
        assert!(node
            .capabilities
            .tags
            .contains(&"motion_detect".to_string()));
    }

    #[test]
    fn test_camera_motion_with_snapshot_stores_capture() {
        let bridge = make_bridge();

        // Register via heartbeat
        let hb = make_heartbeat_payload("xtensa", 8704, Some(-42));
        bridge.handle_publish(&make_publish("agnos/cam-snap/heartbeat", hb));

        // Send motion event with snapshot
        let payload = make_motion_event_payload(85.0, "pir", true);
        bridge.handle_publish(&make_publish("agnos/cam-snap/camera/motion", payload));

        // Capture should be stored from snapshot
        let captures = bridge.camera_captures();
        assert_eq!(captures.len(), 1);
        assert_eq!(captures[0].trigger, "motion");
        assert_eq!(captures[0].width, 640);
        assert_eq!(captures[0].height, 480);
        assert!((captures[0].motion_intensity.unwrap() - 85.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_camera_captures_per_node_filter() {
        let bridge = make_bridge();

        // Send frames from two different nodes
        let payload1 = make_camera_frame_payload(640, 480, "motion");
        let payload2 = make_camera_frame_payload(320, 240, "interval");
        bridge.handle_publish(&make_publish("agnos/cam-a/camera/frame", payload1));
        bridge.handle_publish(&make_publish("agnos/cam-b/camera/frame", payload2));

        // Total captures = 2
        assert_eq!(bridge.camera_captures().len(), 2);

        // Per-node filter
        let a_captures = bridge.camera_captures_for_node("cam-a");
        assert_eq!(a_captures.len(), 1);
        assert_eq!(a_captures[0].width, 640);

        let b_captures = bridge.camera_captures_for_node("cam-b");
        assert_eq!(b_captures.len(), 1);
        assert_eq!(b_captures[0].width, 320);
    }

    #[test]
    fn test_camera_captures_ring_buffer() {
        let bridge = make_bridge();

        // Fill past the MAX_CAMERA_CAPTURES limit
        for i in 0..(MAX_CAMERA_CAPTURES + 10) {
            let frame = serde_json::json!({
                "data_base64": format!("frame_{}", i),
                "width": 160,
                "height": 120,
                "jpeg_quality": 10,
                "trigger": "interval"
            });
            let payload = serde_json::to_vec(&frame).unwrap();
            bridge.handle_publish(&make_publish(
                &format!("agnos/cam-ring/camera/frame"),
                payload,
            ));
        }

        let captures = bridge.camera_captures();
        assert_eq!(captures.len(), MAX_CAMERA_CAPTURES);

        // Oldest frames should have been evicted; newest should be present
        let last = &captures[captures.len() - 1];
        assert_eq!(
            last.data_base64,
            format!("frame_{}", MAX_CAMERA_CAPTURES + 9)
        );
    }

    #[test]
    fn test_camera_frame_malformed_json_does_not_panic() {
        let bridge = make_bridge();

        // Invalid JSON
        bridge.handle_publish(&make_publish(
            "agnos/cam-bad/camera/frame",
            b"not json".to_vec(),
        ));

        // Valid JSON but wrong schema
        bridge.handle_publish(&make_publish(
            "agnos/cam-bad/camera/frame",
            serde_json::to_vec(&serde_json::json!({"foo": "bar"})).unwrap(),
        ));

        // Empty payload
        bridge.handle_publish(&make_publish("agnos/cam-bad/camera/frame", vec![]));

        assert_eq!(bridge.camera_captures().len(), 0);
    }

    #[test]
    fn test_camera_motion_malformed_json_does_not_panic() {
        let bridge = make_bridge();

        bridge.handle_publish(&make_publish(
            "agnos/cam-bad/camera/motion",
            b"not json".to_vec(),
        ));

        bridge.handle_publish(&make_publish(
            "agnos/cam-bad/camera/motion",
            serde_json::to_vec(&serde_json::json!({"wrong": true})).unwrap(),
        ));

        assert_eq!(bridge.camera_captures().len(), 0);
    }

    #[test]
    fn test_camera_frame_deserialization() {
        let json = serde_json::json!({
            "data_base64": "dGVzdA==",
            "width": 800,
            "height": 600,
            "jpeg_quality": 35,
            "trigger": "manual",
            "timestamp_secs": 1710720000
        });
        let frame: McuCameraFrame = serde_json::from_value(json).unwrap();
        assert_eq!(frame.data_base64, "dGVzdA==");
        assert_eq!(frame.width, 800);
        assert_eq!(frame.height, 600);
        assert_eq!(frame.jpeg_quality, 35);
        assert_eq!(frame.trigger, "manual");
        assert_eq!(frame.timestamp_secs, Some(1710720000));
    }

    #[test]
    fn test_camera_frame_default_trigger() {
        let json = serde_json::json!({
            "data_base64": "dGVzdA==",
            "width": 640,
            "height": 480,
            "jpeg_quality": 20
        });
        let frame: McuCameraFrame = serde_json::from_value(json).unwrap();
        assert_eq!(frame.trigger, "manual");
    }

    #[test]
    fn test_motion_event_deserialization() {
        let json = serde_json::json!({
            "detected": true,
            "intensity": 45.5,
            "source": "both",
            "snapshot_base64": "c25hcA==",
            "snapshot_width": 320,
            "snapshot_height": 240,
            "timestamp_secs": 1710720000
        });
        let event: McuMotionEvent = serde_json::from_value(json).unwrap();
        assert!(event.detected);
        assert!((event.intensity.unwrap() - 45.5).abs() < f64::EPSILON);
        assert_eq!(event.source, "both");
        assert_eq!(event.snapshot_base64.as_deref(), Some("c25hcA=="));
        assert_eq!(event.snapshot_width, Some(320));
        assert_eq!(event.snapshot_height, Some(240));
    }

    #[test]
    fn test_motion_event_default_source() {
        let json = serde_json::json!({
            "detected": true
        });
        let event: McuMotionEvent = serde_json::from_value(json).unwrap();
        assert_eq!(event.source, "frame_diff");
        assert!(event.snapshot_base64.is_none());
        assert!(event.intensity.is_none());
    }

    #[test]
    fn test_camera_no_duplicate_tags() {
        let bridge = make_bridge();

        // Register via heartbeat
        let hb = make_heartbeat_payload("xtensa", 8704, Some(-40));
        bridge.handle_publish(&make_publish("agnos/cam-dup/heartbeat", hb));
        let fleet_id = bridge.get_fleet_id("cam-dup").unwrap();

        // Send two frames — "camera" tag should not be duplicated
        for _ in 0..2 {
            let payload = make_camera_frame_payload(640, 480, "interval");
            bridge.handle_publish(&make_publish("agnos/cam-dup/camera/frame", payload));
        }

        let fleet = bridge.fleet.lock().unwrap();
        let tags = &fleet.nodes[&fleet_id].capabilities.tags;
        let camera_count = tags.iter().filter(|t| *t == "camera").count();
        assert_eq!(camera_count, 1);
    }
}
