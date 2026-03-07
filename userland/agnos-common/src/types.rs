//! Common types used throughout AGNOS

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Version information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    pub prerelease: Option<String>,
    pub build: Option<String>,
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if let Some(pre) = &self.prerelease {
            write!(f, "-{}", pre)?;
        }
        if let Some(build) = &self.build {
            write!(f, "+{}", build)?;
        }
        Ok(())
    }
}

impl Default for Version {
    fn default() -> Self {
        Self {
            major: 2026,
            minor: 3,
            patch: 7,
            prerelease: None,
            build: None,
        }
    }
}

/// System capabilities
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    pub llm_support: bool,
    pub gpu_acceleration: bool,
    pub secure_boot: bool,
    pub virtualization: bool,
}

impl Default for Capabilities {
    fn default() -> Self {
        Self {
            llm_support: false,
            gpu_acceleration: false,
            secure_boot: false,
            virtualization: true,
        }
    }
}

/// Message for inter-agent communication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub source: String,
    pub target: String,
    pub message_type: MessageType,
    pub payload: serde_json::Value,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageType {
    Command,
    Response,
    Event,
    Error,
    Heartbeat,
}

/// System health information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemHealth {
    pub uptime_seconds: u64,
    pub cpu_usage_percent: f32,
    pub memory_usage_percent: f32,
    pub disk_usage_percent: f32,
    pub active_agents: u32,
    pub pending_tasks: u32,
    pub status: SystemStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SystemStatus {
    Healthy,
    Degraded,
    Critical,
    Maintenance,
}

/// Configuration for AGNOS components
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentConfig {
    pub name: String,
    pub enabled: bool,
    pub settings: HashMap<String, serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_default() {
        let version = Version::default();
        assert_eq!(version.major, 2026);
        assert_eq!(version.minor, 3);
        assert_eq!(version.patch, 7);
        assert!(version.prerelease.is_none());
        assert!(version.build.is_none());
    }

    #[test]
    fn test_version_display() {
        let version = Version {
            major: 1,
            minor: 2,
            patch: 3,
            prerelease: None,
            build: None,
        };
        assert_eq!(version.to_string(), "1.2.3");
    }

    #[test]
    fn test_version_display_with_prerelease() {
        let version = Version {
            major: 1,
            minor: 0,
            patch: 0,
            prerelease: Some("alpha".to_string()),
            build: None,
        };
        assert_eq!(version.to_string(), "1.0.0-alpha");
    }

    #[test]
    fn test_version_display_with_build() {
        let version = Version {
            major: 2,
            minor: 1,
            patch: 0,
            prerelease: None,
            build: Some("abc123".to_string()),
        };
        assert_eq!(version.to_string(), "2.1.0+abc123");
    }

    #[test]
    fn test_version_display_full() {
        let version = Version {
            major: 1,
            minor: 0,
            patch: 0,
            prerelease: Some("beta.1".to_string()),
            build: Some("20240101".to_string()),
        };
        assert_eq!(version.to_string(), "1.0.0-beta.1+20240101");
    }

    #[test]
    fn test_capabilities_default() {
        let caps = Capabilities::default();
        assert!(!caps.llm_support);
        assert!(!caps.gpu_acceleration);
        assert!(!caps.secure_boot);
        assert!(caps.virtualization);
    }

    #[test]
    fn test_capabilities_custom() {
        let caps = Capabilities {
            llm_support: true,
            gpu_acceleration: true,
            secure_boot: true,
            virtualization: false,
        };
        assert!(caps.llm_support);
        assert!(caps.gpu_acceleration);
        assert!(caps.secure_boot);
        assert!(!caps.virtualization);
    }

    #[test]
    fn test_message_creation() {
        let msg = Message {
            id: "msg-123".to_string(),
            source: "agent-a".to_string(),
            target: "agent-b".to_string(),
            message_type: MessageType::Command,
            payload: serde_json::json!({"action": "test"}),
            timestamp: chrono::Utc::now(),
        };
        assert_eq!(msg.id, "msg-123");
        assert_eq!(msg.source, "agent-a");
        assert_eq!(msg.target, "agent-b");
        assert!(matches!(msg.message_type, MessageType::Command));
    }

    #[test]
    fn test_message_type_variants() {
        assert!(matches!(MessageType::Command, MessageType::Command));
        assert!(matches!(MessageType::Response, MessageType::Response));
        assert!(matches!(MessageType::Event, MessageType::Event));
        assert!(matches!(MessageType::Error, MessageType::Error));
        assert!(matches!(MessageType::Heartbeat, MessageType::Heartbeat));
    }

    #[test]
    fn test_system_health() {
        let health = SystemHealth {
            uptime_seconds: 3600,
            cpu_usage_percent: 25.5,
            memory_usage_percent: 60.0,
            disk_usage_percent: 45.0,
            active_agents: 5,
            pending_tasks: 2,
            status: SystemStatus::Healthy,
        };
        assert_eq!(health.uptime_seconds, 3600);
        assert_eq!(health.cpu_usage_percent, 25.5);
        assert_eq!(health.active_agents, 5);
        assert!(matches!(health.status, SystemStatus::Healthy));
    }

    #[test]
    fn test_system_status_variants() {
        assert!(matches!(SystemStatus::Healthy, SystemStatus::Healthy));
        assert!(matches!(SystemStatus::Degraded, SystemStatus::Degraded));
        assert!(matches!(SystemStatus::Critical, SystemStatus::Critical));
        assert!(matches!(
            SystemStatus::Maintenance,
            SystemStatus::Maintenance
        ));
    }

    #[test]
    fn test_component_config() {
        let mut settings = HashMap::new();
        settings.insert("key".to_string(), serde_json::json!("value"));

        let config = ComponentConfig {
            name: "test-component".to_string(),
            enabled: true,
            settings,
        };
        assert_eq!(config.name, "test-component");
        assert!(config.enabled);
        assert!(config.settings.contains_key("key"));
    }

    #[test]
    fn test_component_config_disabled() {
        let config = ComponentConfig {
            name: "disabled-component".to_string(),
            enabled: false,
            settings: HashMap::new(),
        };
        assert!(!config.enabled);
    }

    #[test]
    fn test_message_json_serialization() {
        let msg = Message {
            id: "msg-456".to_string(),
            source: "src".to_string(),
            target: "dst".to_string(),
            message_type: MessageType::Response,
            payload: serde_json::json!({"result": true}),
            timestamp: chrono::Utc::now(),
        };

        let json = serde_json::to_string(&msg).expect("Failed to serialize");
        assert!(json.contains("msg-456"));
        assert!(json.contains("Response"));
    }
}
