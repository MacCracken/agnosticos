//! AGNOS Common Library
//!
//! Shared types, traits, and utilities used across all AGNOS components.

pub mod agent;
pub mod audit;
pub mod error;
pub mod llm;
pub mod secrets;
pub mod security;
pub mod telemetry;
pub mod types;

#[cfg(test)]
mod security_tests;

pub use agent::{AgentEvent, AgentInfo, AgentStats, StopReason};
pub use error::{AgnosError, Result};
pub use llm::*;
pub use security::{Capability, SecurityContext, SecurityPolicy, PolicyEffect};
pub use telemetry::{TelemetryConfig, TelemetryCollector, CrashReport, EventType};
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

/// Per-agent network firewall policy for restricted network access.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkPolicy {
    /// Outbound ports the agent is allowed to connect to
    pub allowed_outbound_ports: Vec<u16>,
    /// Outbound hosts/CIDRs the agent is allowed to connect to
    pub allowed_outbound_hosts: Vec<String>,
    /// Inbound ports the agent is allowed to receive on
    pub allowed_inbound_ports: Vec<u16>,
    /// Whether to enable NAT for outbound traffic
    pub enable_nat: bool,
}

impl Default for NetworkPolicy {
    fn default() -> Self {
        Self {
            allowed_outbound_ports: vec![80, 443],
            allowed_outbound_hosts: Vec::new(),
            allowed_inbound_ports: Vec::new(),
            enable_nat: true,
        }
    }
}

/// Per-agent encrypted storage configuration (LUKS2).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedStorageConfig {
    /// Whether encrypted storage is enabled for this agent
    pub enabled: bool,
    /// Size of the encrypted volume in megabytes
    pub size_mb: u64,
    /// Filesystem to use (ext4, xfs, btrfs)
    pub filesystem: String,
}

impl Default for EncryptedStorageConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            size_mb: 256,
            filesystem: "ext4".to_string(),
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
    /// Per-agent network firewall policy (for NetworkAccess::Restricted)
    #[serde(default)]
    pub network_policy: Option<NetworkPolicy>,
    /// MAC profile name to apply (auto-detects SELinux/AppArmor)
    #[serde(default)]
    pub mac_profile: Option<String>,
    /// Encrypted storage configuration
    #[serde(default)]
    pub encrypted_storage: Option<EncryptedStorageConfig>,
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
            network_policy: None,
            mac_profile: None,
            encrypted_storage: None,
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

    #[test]
    fn test_user_id_generation() {
        let id1 = UserId::new();
        let id2 = UserId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_user_id_default() {
        let id = UserId::default();
        let _ = id.0;
    }

    #[test]
    fn test_agent_id_display() {
        let id = AgentId::new();
        let display = format!("{}", id);
        assert!(!display.is_empty());
        assert_eq!(display.len(), 36); // UUID format
    }

    #[test]
    fn test_agent_id_default() {
        let id = AgentId::default();
        let _ = id.0;
    }

    #[test]
    fn test_network_policy_default() {
        let policy = NetworkPolicy::default();
        assert_eq!(policy.allowed_outbound_ports, vec![80, 443]);
        assert!(policy.allowed_outbound_hosts.is_empty());
        assert!(policy.allowed_inbound_ports.is_empty());
        assert!(policy.enable_nat);
    }

    #[test]
    fn test_encrypted_storage_config_default() {
        let config = EncryptedStorageConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.size_mb, 256);
        assert_eq!(config.filesystem, "ext4");
    }

    #[test]
    fn test_sandbox_config_default() {
        let config = SandboxConfig::default();
        assert_eq!(config.filesystem_rules.len(), 1);
        assert_eq!(config.network_access, NetworkAccess::LocalhostOnly);
        assert!(config.seccomp_rules.is_empty());
        assert!(config.isolate_network);
        assert!(config.network_policy.is_none());
        assert!(config.mac_profile.is_none());
        assert!(config.encrypted_storage.is_none());
    }

    #[test]
    fn test_agent_config_default() {
        let config = AgentConfig::default();
        assert!(config.name.is_empty());
        assert_eq!(config.agent_type, AgentType::User);
        assert!(config.permissions.is_empty());
    }

    #[test]
    fn test_agent_type_default() {
        assert_eq!(AgentType::default(), AgentType::User);
    }

    #[test]
    fn test_resource_usage_default() {
        let usage = ResourceUsage::default();
        assert_eq!(usage.memory_used, 0);
        assert_eq!(usage.cpu_time_used, 0);
        assert_eq!(usage.file_descriptors_used, 0);
        assert_eq!(usage.processes_used, 0);
    }

    #[test]
    fn test_fs_access_variants() {
        assert_ne!(FsAccess::NoAccess, FsAccess::ReadOnly);
        assert_ne!(FsAccess::ReadOnly, FsAccess::ReadWrite);
    }

    #[test]
    fn test_network_access_variants() {
        assert_ne!(NetworkAccess::None, NetworkAccess::Full);
        assert_ne!(NetworkAccess::LocalhostOnly, NetworkAccess::Restricted);
    }

    #[test]
    fn test_seccomp_action_variants() {
        assert_ne!(SeccompAction::Allow, SeccompAction::Deny);
        assert_ne!(SeccompAction::Deny, SeccompAction::Trap);
    }

    #[test]
    fn test_permission_variants() {
        assert_ne!(Permission::FileRead, Permission::FileWrite);
        assert_ne!(Permission::NetworkAccess, Permission::ProcessSpawn);
    }

    #[test]
    fn test_agent_status_variants() {
        assert_ne!(AgentStatus::Pending, AgentStatus::Running);
        assert_ne!(AgentStatus::Running, AgentStatus::Stopped);
        assert_ne!(AgentStatus::Failed, AgentStatus::Paused);
    }

    #[test]
    fn test_sandbox_config_serialization() {
        let config = SandboxConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: SandboxConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.filesystem_rules.len(), 1);
        assert_eq!(deserialized.network_access, NetworkAccess::LocalhostOnly);
    }

    #[test]
    fn test_network_policy_serialization() {
        let policy = NetworkPolicy {
            allowed_outbound_ports: vec![8080],
            allowed_outbound_hosts: vec!["example.com".to_string()],
            allowed_inbound_ports: vec![3000],
            enable_nat: false,
        };
        let json = serde_json::to_string(&policy).unwrap();
        let deserialized: NetworkPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.allowed_outbound_ports, vec![8080]);
        assert!(!deserialized.enable_nat);
    }

    #[test]
    fn test_agent_config_serialization() {
        let config = AgentConfig {
            name: "test-agent".to_string(),
            agent_type: AgentType::Service,
            resource_limits: ResourceLimits::default(),
            sandbox: SandboxConfig::default(),
            permissions: vec![Permission::FileRead, Permission::LlmInference],
            metadata: serde_json::json!({"version": "1.0"}),
        };
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("test-agent"));
        assert!(json.contains("Service"));
    }

    #[test]
    fn test_agent_id_serde_roundtrip() {
        let id = AgentId::new();
        let json = serde_json::to_string(&id).unwrap();
        let deserialized: AgentId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, deserialized);
    }

    #[test]
    fn test_agent_id_hash_equality() {
        use std::collections::HashSet;
        let id = AgentId::new();
        let mut set = HashSet::new();
        set.insert(id);
        set.insert(id); // duplicate
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn test_agent_id_copy_semantics() {
        let id = AgentId::new();
        let id2 = id; // Copy
        assert_eq!(id, id2);
    }

    #[test]
    fn test_user_id_serde_roundtrip() {
        let id = UserId::new();
        let json = serde_json::to_string(&id).unwrap();
        let deserialized: UserId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, deserialized);
    }

    #[test]
    fn test_resource_limits_serde_roundtrip() {
        let limits = ResourceLimits {
            max_memory: 512 * 1024 * 1024,
            max_cpu_time: 60_000,
            max_file_descriptors: 256,
            max_processes: 16,
        };
        let json = serde_json::to_string(&limits).unwrap();
        let deserialized: ResourceLimits = serde_json::from_str(&json).unwrap();
        assert_eq!(limits, deserialized);
    }

    #[test]
    fn test_agent_status_serde_roundtrip() {
        let statuses = [
            AgentStatus::Pending,
            AgentStatus::Starting,
            AgentStatus::Running,
            AgentStatus::Paused,
            AgentStatus::Stopping,
            AgentStatus::Stopped,
            AgentStatus::Failed,
        ];
        for status in &statuses {
            let json = serde_json::to_string(status).unwrap();
            let deserialized: AgentStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(*status, deserialized);
        }
    }

    #[test]
    fn test_resource_usage_serde_roundtrip() {
        let usage = ResourceUsage {
            memory_used: 1024,
            cpu_time_used: 500,
            file_descriptors_used: 10,
            processes_used: 3,
        };
        let json = serde_json::to_string(&usage).unwrap();
        let deserialized: ResourceUsage = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.memory_used, 1024);
        assert_eq!(deserialized.cpu_time_used, 500);
    }

    #[test]
    fn test_agent_type_serde_all_variants() {
        let types = [AgentType::System, AgentType::User, AgentType::Service];
        for t in &types {
            let json = serde_json::to_string(t).unwrap();
            let deserialized: AgentType = serde_json::from_str(&json).unwrap();
            assert_eq!(*t, deserialized);
        }
    }

    #[test]
    fn test_permission_serde_all_variants() {
        let perms = [
            Permission::FileRead,
            Permission::FileWrite,
            Permission::NetworkAccess,
            Permission::ProcessSpawn,
            Permission::LlmInference,
            Permission::AuditRead,
        ];
        for p in &perms {
            let json = serde_json::to_string(p).unwrap();
            let deserialized: Permission = serde_json::from_str(&json).unwrap();
            assert_eq!(*p, deserialized);
        }
    }

    #[test]
    fn test_sandbox_config_with_all_optional_fields() {
        let config = SandboxConfig {
            filesystem_rules: vec![],
            network_access: NetworkAccess::Restricted,
            seccomp_rules: vec![SeccompRule {
                syscall: "write".to_string(),
                action: SeccompAction::Allow,
            }],
            isolate_network: false,
            network_policy: Some(NetworkPolicy::default()),
            mac_profile: Some("agnos-agent".to_string()),
            encrypted_storage: Some(EncryptedStorageConfig {
                enabled: true,
                size_mb: 512,
                filesystem: "btrfs".to_string(),
            }),
        };
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: SandboxConfig = serde_json::from_str(&json).unwrap();
        assert!(deserialized.network_policy.is_some());
        assert_eq!(deserialized.mac_profile.as_deref(), Some("agnos-agent"));
        assert!(deserialized.encrypted_storage.unwrap().enabled);
    }

    #[test]
    fn test_sandbox_config_json_missing_optional_fields_uses_defaults() {
        let json = r#"{"filesystem_rules":[],"network_access":"Full","seccomp_rules":[],"isolate_network":false}"#;
        let config: SandboxConfig = serde_json::from_str(json).unwrap();
        assert!(config.network_policy.is_none());
        assert!(config.mac_profile.is_none());
        assert!(config.encrypted_storage.is_none());
    }

    #[test]
    fn test_encrypted_storage_config_serde_roundtrip() {
        let config = EncryptedStorageConfig {
            enabled: true,
            size_mb: 1024,
            filesystem: "xfs".to_string(),
        };
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: EncryptedStorageConfig = serde_json::from_str(&json).unwrap();
        assert!(deserialized.enabled);
        assert_eq!(deserialized.size_mb, 1024);
        assert_eq!(deserialized.filesystem, "xfs");
    }

    #[test]
    fn test_agent_config_deserialization_roundtrip() {
        let config = AgentConfig {
            name: "roundtrip-agent".to_string(),
            agent_type: AgentType::System,
            resource_limits: ResourceLimits::default(),
            sandbox: SandboxConfig::default(),
            permissions: vec![Permission::FileRead, Permission::FileWrite, Permission::ProcessSpawn],
            metadata: serde_json::json!({"key": "value", "count": 42}),
        };
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: AgentConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "roundtrip-agent");
        assert_eq!(deserialized.agent_type, AgentType::System);
        assert_eq!(deserialized.permissions.len(), 3);
        assert_eq!(deserialized.metadata["count"], 42);
    }

    #[test]
    fn test_resource_usage_clone() {
        let usage = ResourceUsage {
            memory_used: 999,
            cpu_time_used: 888,
            file_descriptors_used: 77,
            processes_used: 6,
        };
        let cloned = usage;
        assert_eq!(cloned.memory_used, 999);
        assert_eq!(cloned.processes_used, 6);
    }

    #[test]
    fn test_agent_status_debug() {
        let status = AgentStatus::Running;
        let debug_str = format!("{:?}", status);
        assert_eq!(debug_str, "Running");
    }
}
