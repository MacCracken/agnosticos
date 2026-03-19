//! MQTT Bridge — Translates ESP32 MCU heartbeats to the edge fleet model.
//!
//! Subscribes to MQTT topics published by ESP32 agents (and other MCU nodes)
//! and translates incoming JSON messages into [`EdgeNode`] heartbeats that
//! feed into the existing [`EdgeFleetManager`] registry.
//!
//! Topic schema (from `recipes/edge/esp32-agent.toml`):
//!   - `agnos/{node_id}/heartbeat` — periodic liveness + capabilities
//!   - `agnos/{node_id}/telemetry` — sensor readings batch
//!   - `agnos/{node_id}/status`    — lifecycle events (boot, sleep, ota)
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
}

impl MqttBridge {
    /// Create a new MQTT bridge.
    pub fn new(config: MqttBridgeConfig, fleet: Arc<Mutex<EdgeFleetManager>>) -> Self {
        Self {
            config,
            fleet,
            node_id_map: Arc::new(Mutex::new(std::collections::HashMap::new())),
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
}
