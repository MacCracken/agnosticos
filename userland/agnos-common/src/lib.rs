//! AGNOS Common Library
//!
//! Shared types, traits, and utilities used across all AGNOS components.

pub mod agent;
pub mod audit;
pub mod error;
pub mod llm;
pub mod security;
pub mod types;

#[cfg(test)]
mod security_tests;

pub use agent::{AgentEvent, AgentInfo, AgentStats, StopReason};
pub use error::{AgnosError, Result};
pub use llm::*;
pub use types::*;

// Re-export commonly used crates
pub use serde_json;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unique identifier for an agent
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AgentId(pub Uuid);

impl AgentId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for AgentId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for AgentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Unique identifier for a user
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UserId(pub Uuid);

impl UserId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for UserId {
    fn default() -> Self {
        Self::new()
    }
}

/// Resource limits for agents
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ResourceLimits {
    /// Maximum memory in bytes
    pub max_memory: u64,
    /// Maximum CPU time in milliseconds
    pub max_cpu_time: u64,
    /// Maximum number of file descriptors
    pub max_file_descriptors: u32,
    /// Maximum number of processes
    pub max_processes: u32,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_memory: 1024 * 1024 * 1024, // 1GB
            max_cpu_time: 3600 * 1000,      // 1 hour
            max_file_descriptors: 1024,
            max_processes: 64,
        }
    }
}

/// Sandbox configuration for agents
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    /// Filesystem access rules
    pub filesystem_rules: Vec<FilesystemRule>,
    /// Network access configuration
    pub network_access: NetworkAccess,
    /// seccomp filter rules
    pub seccomp_rules: Vec<SeccompRule>,
    /// Whether to create a new network namespace
    pub isolate_network: bool,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            filesystem_rules: vec![FilesystemRule {
                path: "/tmp".into(),
                access: FsAccess::ReadWrite,
            }],
            network_access: NetworkAccess::LocalhostOnly,
            seccomp_rules: vec![],
            isolate_network: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilesystemRule {
    pub path: std::path::PathBuf,
    pub access: FsAccess,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FsAccess {
    NoAccess,
    ReadOnly,
    ReadWrite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NetworkAccess {
    None,
    LocalhostOnly,
    Restricted,
    Full,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeccompRule {
    pub syscall: String,
    pub action: SeccompAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SeccompAction {
    Allow,
    Deny,
    Trap,
}

/// Agent configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub name: String,
    pub agent_type: AgentType,
    pub resource_limits: ResourceLimits,
    pub sandbox: SandboxConfig,
    pub permissions: Vec<Permission>,
    pub metadata: serde_json::Value,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            agent_type: AgentType::User,
            resource_limits: ResourceLimits::default(),
            sandbox: SandboxConfig::default(),
            permissions: Vec::new(),
            metadata: serde_json::Value::Null,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentType {
    System,
    User,
    Service,
}

impl Default for AgentType {
    fn default() -> Self {
        Self::User
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Permission {
    FileRead,
    FileWrite,
    NetworkAccess,
    ProcessSpawn,
    LlmInference,
    AuditRead,
}

/// Agent status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentStatus {
    Pending,
    Starting,
    Running,
    Paused,
    Stopping,
    Stopped,
    Failed,
}

/// Current resource usage
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct ResourceUsage {
    pub memory_used: u64,
    pub cpu_time_used: u64,
    pub file_descriptors_used: u32,
    pub processes_used: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_id_generation() {
        let id1 = AgentId::new();
        let id2 = AgentId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_resource_limits_default() {
        let limits = ResourceLimits::default();
        assert_eq!(limits.max_memory, 1024 * 1024 * 1024);
        assert_eq!(limits.max_file_descriptors, 1024);
    }

    #[test]
    fn test_version_display() {
        let v = Version {
            major: 1,
            minor: 2,
            patch: 3,
            prerelease: Some("alpha".to_string()),
            build: Some("build123".to_string()),
        };
        assert_eq!(v.to_string(), "1.2.3-alpha+build123");
    }

    #[test]
    fn test_version_default() {
        let v = Version::default();
        assert_eq!(v.to_string(), "0.1.0");
    }

    #[test]
    fn test_version_without_optional() {
        let v = Version {
            major: 2,
            minor: 0,
            patch: 0,
            prerelease: None,
            build: None,
        };
        assert_eq!(v.to_string(), "2.0.0");
    }

    #[test]
    fn test_capabilities_default() {
        let caps = Capabilities::default();
        assert!(!caps.llm_support);
        assert!(caps.virtualization);
    }

    #[test]
    fn test_message_type_variants() {
        use crate::types::MessageType;
        assert_eq!(MessageType::Command, MessageType::Command);
        assert_eq!(MessageType::Response, MessageType::Response);
        assert_ne!(MessageType::Command, MessageType::Event);
    }

    #[test]
    fn test_system_status_variants() {
        use crate::types::SystemStatus;
        assert_eq!(SystemStatus::Healthy, SystemStatus::Healthy);
        assert_ne!(SystemStatus::Healthy, SystemStatus::Critical);
    }

    #[test]
    fn test_component_config() {
        use std::collections::HashMap;
        let mut settings = HashMap::new();
        settings.insert("port".to_string(), serde_json::json!(8080));
        let config = ComponentConfig {
            name: "test".to_string(),
            enabled: true,
            settings,
        };
        assert_eq!(config.name, "test");
        assert!(config.enabled);
    }

    #[test]
    fn test_agnos_error_retriable() {
        use crate::error::AgnosError;

        let timeout = AgnosError::Timeout;
        assert!(timeout.is_retriable());

        let not_found = AgnosError::AgentNotFound("test".to_string());
        assert!(!not_found.is_retriable());

        let permission = AgnosError::PermissionDenied("test".to_string());
        assert!(!permission.is_retriable());
    }
}
