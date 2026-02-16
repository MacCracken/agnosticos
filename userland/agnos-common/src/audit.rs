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
}
