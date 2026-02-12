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
