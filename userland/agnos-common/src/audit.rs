//! Audit logging types

use crate::{AgentId, UserId};
use serde::{Deserialize, Serialize};

/// Audit event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub sequence: u64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub event_type: AuditEventType,
    pub agent_id: Option<AgentId>,
    pub user_id: UserId,
    pub action: String,
    pub resource: String,
    pub result: AuditResult,
    pub details: serde_json::Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditEventType {
    AgentCreated,
    AgentTerminated,
    AgentAction,
    FileAccess,
    NetworkAccess,
    LlmInference,
    PermissionChange,
    ConfigChange,
    SecurityEvent,
    SystemEvent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditResult {
    Success,
    Failure,
    Denied,
}

/// Audit log entry with cryptographic chain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub event: AuditEvent,
    pub previous_hash: String,
    pub entry_hash: String,
    pub signature: String,
}

/// Audit log configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditConfig {
    pub enabled: bool,
    pub log_file: String,
    pub max_file_size: u64,
    pub max_files: u32,
    pub encrypt: bool,
    pub sign_entries: bool,
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            log_file: "/var/log/agnos/audit.log".into(),
            max_file_size: 100 * 1024 * 1024, // 100MB
            max_files: 10,
            encrypt: true,
            sign_entries: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audit_config_default() {
        let config = AuditConfig::default();
        assert!(config.enabled);
        assert_eq!(config.max_files, 10);
        assert!(config.encrypt);
        assert!(config.sign_entries);
    }

    #[test]
    fn test_audit_event_type_variants() {
        use crate::audit::AuditEventType;
        assert_eq!(AuditEventType::AgentCreated, AuditEventType::AgentCreated);
        assert_ne!(
            AuditEventType::AgentCreated,
            AuditEventType::AgentTerminated
        );
    }

    #[test]
    fn test_audit_result_variants() {
        use crate::audit::AuditResult;
        assert_eq!(AuditResult::Success, AuditResult::Success);
        assert_ne!(AuditResult::Success, AuditResult::Denied);
    }

    #[test]
    fn test_audit_entry() {
        let event = AuditEvent {
            sequence: 1,
            timestamp: chrono::Utc::now(),
            event_type: AuditEventType::AgentCreated,
            agent_id: None,
            user_id: UserId::new(),
            action: "create".to_string(),
            resource: "agent".to_string(),
            result: AuditResult::Success,
            details: serde_json::json!({"name": "test"}),
        };

        let entry = AuditEntry {
            event,
            previous_hash: "abc123".to_string(),
            entry_hash: "def456".to_string(),
            signature: "sig789".to_string(),
        };

        assert_eq!(entry.previous_hash, "abc123");
        assert_eq!(entry.entry_hash, "def456");
    }

    // --- New coverage tests (batch 2) ---

    #[test]
    fn test_audit_event_all_event_types() {
        let types = [
            AuditEventType::AgentCreated,
            AuditEventType::AgentTerminated,
            AuditEventType::AgentAction,
            AuditEventType::FileAccess,
            AuditEventType::NetworkAccess,
            AuditEventType::LlmInference,
            AuditEventType::PermissionChange,
            AuditEventType::ConfigChange,
            AuditEventType::SecurityEvent,
            AuditEventType::SystemEvent,
        ];
        // Ensure all 10 variants are distinct
        for (i, a) in types.iter().enumerate() {
            for (j, b) in types.iter().enumerate() {
                if i == j {
                    assert_eq!(a, b);
                } else {
                    assert_ne!(a, b);
                }
            }
        }
    }

    #[test]
    fn test_audit_result_all_variants() {
        let results = [AuditResult::Success, AuditResult::Failure, AuditResult::Denied];
        for (i, a) in results.iter().enumerate() {
            for (j, b) in results.iter().enumerate() {
                if i == j {
                    assert_eq!(a, b);
                } else {
                    assert_ne!(a, b);
                }
            }
        }
    }

    #[test]
    fn test_audit_event_serialization_roundtrip() {
        let event = AuditEvent {
            sequence: 42,
            timestamp: chrono::Utc::now(),
            event_type: AuditEventType::FileAccess,
            agent_id: Some(AgentId::new()),
            user_id: UserId::new(),
            action: "read".to_string(),
            resource: "/etc/passwd".to_string(),
            result: AuditResult::Denied,
            details: serde_json::json!({"reason": "sandbox violation"}),
        };
        let json = serde_json::to_string(&event).unwrap();
        let deserialized: AuditEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.sequence, 42);
        assert_eq!(deserialized.event_type, AuditEventType::FileAccess);
        assert_eq!(deserialized.result, AuditResult::Denied);
        assert_eq!(deserialized.action, "read");
        assert_eq!(deserialized.resource, "/etc/passwd");
    }

    #[test]
    fn test_audit_event_with_agent_id() {
        let agent_id = AgentId::new();
        let event = AuditEvent {
            sequence: 1,
            timestamp: chrono::Utc::now(),
            event_type: AuditEventType::AgentAction,
            agent_id: Some(agent_id),
            user_id: UserId::new(),
            action: "execute".to_string(),
            resource: "task-1".to_string(),
            result: AuditResult::Success,
            details: serde_json::json!(null),
        };
        assert!(event.agent_id.is_some());
    }

    #[test]
    fn test_audit_event_without_agent_id() {
        let event = AuditEvent {
            sequence: 1,
            timestamp: chrono::Utc::now(),
            event_type: AuditEventType::SystemEvent,
            agent_id: None,
            user_id: UserId::new(),
            action: "boot".to_string(),
            resource: "system".to_string(),
            result: AuditResult::Success,
            details: serde_json::json!({}),
        };
        assert!(event.agent_id.is_none());
    }

    #[test]
    fn test_audit_event_details_complex_json() {
        let details = serde_json::json!({
            "paths": ["/tmp/a", "/tmp/b"],
            "permissions": {"read": true, "write": false},
            "count": 42
        });
        let event = AuditEvent {
            sequence: 100,
            timestamp: chrono::Utc::now(),
            event_type: AuditEventType::PermissionChange,
            agent_id: None,
            user_id: UserId::new(),
            action: "chmod".to_string(),
            resource: "/tmp/a".to_string(),
            result: AuditResult::Success,
            details: details.clone(),
        };
        assert_eq!(event.details["count"], 42);
        assert_eq!(event.details["paths"][0], "/tmp/a");
    }

    #[test]
    fn test_audit_entry_serialization_roundtrip() {
        let event = AuditEvent {
            sequence: 5,
            timestamp: chrono::Utc::now(),
            event_type: AuditEventType::LlmInference,
            agent_id: None,
            user_id: UserId::new(),
            action: "infer".to_string(),
            resource: "gpt-4".to_string(),
            result: AuditResult::Success,
            details: serde_json::json!({"tokens": 1024}),
        };
        let entry = AuditEntry {
            event,
            previous_hash: "aaa".to_string(),
            entry_hash: "bbb".to_string(),
            signature: "ccc".to_string(),
        };
        let json = serde_json::to_string(&entry).unwrap();
        let deserialized: AuditEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.previous_hash, "aaa");
        assert_eq!(deserialized.entry_hash, "bbb");
        assert_eq!(deserialized.signature, "ccc");
        assert_eq!(deserialized.event.sequence, 5);
    }

    #[test]
    fn test_audit_config_custom_values() {
        let config = AuditConfig {
            enabled: false,
            log_file: "/custom/audit.log".into(),
            max_file_size: 50 * 1024 * 1024,
            max_files: 5,
            encrypt: false,
            sign_entries: false,
        };
        assert!(!config.enabled);
        assert_eq!(config.log_file, "/custom/audit.log");
        assert_eq!(config.max_file_size, 50 * 1024 * 1024);
        assert_eq!(config.max_files, 5);
        assert!(!config.encrypt);
        assert!(!config.sign_entries);
    }

    #[test]
    fn test_audit_config_default_log_file() {
        let config = AuditConfig::default();
        assert_eq!(config.log_file, "/var/log/agnos/audit.log");
    }

    #[test]
    fn test_audit_config_default_max_file_size() {
        let config = AuditConfig::default();
        assert_eq!(config.max_file_size, 100 * 1024 * 1024);
    }

    #[test]
    fn test_audit_config_serialization_roundtrip() {
        let config = AuditConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: AuditConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.enabled, config.enabled);
        assert_eq!(deserialized.log_file, config.log_file);
        assert_eq!(deserialized.max_file_size, config.max_file_size);
        assert_eq!(deserialized.max_files, config.max_files);
    }

    #[test]
    fn test_audit_config_clone() {
        let config = AuditConfig::default();
        let cloned = config.clone();
        assert_eq!(cloned.log_file, config.log_file);
        assert_eq!(cloned.max_files, config.max_files);
    }

    #[test]
    fn test_audit_config_debug() {
        let config = AuditConfig::default();
        let debug = format!("{:?}", config);
        assert!(debug.contains("AuditConfig"));
        assert!(debug.contains("enabled"));
    }

    #[test]
    fn test_audit_event_type_serialization_roundtrip() {
        let types = [
            AuditEventType::AgentCreated,
            AuditEventType::AgentTerminated,
            AuditEventType::AgentAction,
            AuditEventType::FileAccess,
            AuditEventType::NetworkAccess,
            AuditEventType::LlmInference,
            AuditEventType::PermissionChange,
            AuditEventType::ConfigChange,
            AuditEventType::SecurityEvent,
            AuditEventType::SystemEvent,
        ];
        for et in &types {
            let json = serde_json::to_string(et).unwrap();
            let deserialized: AuditEventType = serde_json::from_str(&json).unwrap();
            assert_eq!(*et, deserialized);
        }
    }

    #[test]
    fn test_audit_result_serialization_roundtrip() {
        let results = [AuditResult::Success, AuditResult::Failure, AuditResult::Denied];
        for r in &results {
            let json = serde_json::to_string(r).unwrap();
            let deserialized: AuditResult = serde_json::from_str(&json).unwrap();
            assert_eq!(*r, deserialized);
        }
    }

    #[test]
    fn test_audit_event_type_copy() {
        let a = AuditEventType::SecurityEvent;
        let b = a; // Copy
        assert_eq!(a, b);
    }

    #[test]
    fn test_audit_result_copy() {
        let a = AuditResult::Failure;
        let b = a; // Copy
        assert_eq!(a, b);
    }

    #[test]
    fn test_audit_event_clone() {
        let event = AuditEvent {
            sequence: 99,
            timestamp: chrono::Utc::now(),
            event_type: AuditEventType::ConfigChange,
            agent_id: Some(AgentId::new()),
            user_id: UserId::new(),
            action: "update".to_string(),
            resource: "config.toml".to_string(),
            result: AuditResult::Success,
            details: serde_json::json!({"key": "value"}),
        };
        let cloned = event.clone();
        assert_eq!(cloned.sequence, 99);
        assert_eq!(cloned.event_type, AuditEventType::ConfigChange);
        assert_eq!(cloned.action, "update");
    }

    #[test]
    fn test_audit_entry_clone() {
        let entry = AuditEntry {
            event: AuditEvent {
                sequence: 1,
                timestamp: chrono::Utc::now(),
                event_type: AuditEventType::AgentCreated,
                agent_id: None,
                user_id: UserId::new(),
                action: "create".to_string(),
                resource: "agent".to_string(),
                result: AuditResult::Success,
                details: serde_json::json!(null),
            },
            previous_hash: "prev".to_string(),
            entry_hash: "curr".to_string(),
            signature: "sig".to_string(),
        };
        let cloned = entry.clone();
        assert_eq!(cloned.previous_hash, "prev");
        assert_eq!(cloned.entry_hash, "curr");
    }

    #[test]
    fn test_audit_event_sequence_zero() {
        let event = AuditEvent {
            sequence: 0,
            timestamp: chrono::Utc::now(),
            event_type: AuditEventType::SystemEvent,
            agent_id: None,
            user_id: UserId::new(),
            action: "init".to_string(),
            resource: "system".to_string(),
            result: AuditResult::Success,
            details: serde_json::json!(null),
        };
        assert_eq!(event.sequence, 0);
    }

    #[test]
    fn test_audit_event_max_sequence() {
        let event = AuditEvent {
            sequence: u64::MAX,
            timestamp: chrono::Utc::now(),
            event_type: AuditEventType::SystemEvent,
            agent_id: None,
            user_id: UserId::new(),
            action: "test".to_string(),
            resource: "system".to_string(),
            result: AuditResult::Success,
            details: serde_json::json!(null),
        };
        assert_eq!(event.sequence, u64::MAX);
    }

    #[test]
    fn test_audit_event_empty_details() {
        let event = AuditEvent {
            sequence: 1,
            timestamp: chrono::Utc::now(),
            event_type: AuditEventType::NetworkAccess,
            agent_id: None,
            user_id: UserId::new(),
            action: "connect".to_string(),
            resource: "tcp://1.2.3.4:443".to_string(),
            result: AuditResult::Failure,
            details: serde_json::json!({}),
        };
        assert!(event.details.as_object().unwrap().is_empty());
    }

    #[test]
    fn test_audit_event_debug() {
        let event = AuditEvent {
            sequence: 1,
            timestamp: chrono::Utc::now(),
            event_type: AuditEventType::AgentCreated,
            agent_id: None,
            user_id: UserId::new(),
            action: "create".to_string(),
            resource: "agent".to_string(),
            result: AuditResult::Success,
            details: serde_json::json!(null),
        };
        let debug = format!("{:?}", event);
        assert!(debug.contains("AuditEvent"));
        assert!(debug.contains("AgentCreated"));
    }

    #[test]
    fn test_audit_event_type_debug() {
        let et = AuditEventType::LlmInference;
        let debug = format!("{:?}", et);
        assert_eq!(debug, "LlmInference");
    }

    #[test]
    fn test_audit_result_debug() {
        assert_eq!(format!("{:?}", AuditResult::Success), "Success");
        assert_eq!(format!("{:?}", AuditResult::Failure), "Failure");
        assert_eq!(format!("{:?}", AuditResult::Denied), "Denied");
    }
}
