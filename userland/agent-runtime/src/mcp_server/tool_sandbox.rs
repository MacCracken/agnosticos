//! MCP tool-level sandboxing.
//!
//! Extends AGNOS's Landlock/seccomp per-agent sandboxing to MCP tools.
//! Each agent can be granted access to specific tool prefixes via its
//! sandbox profile. Tool calls are checked against the agent's allowed
//! tools before dispatch.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Per-agent tool permission set.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentToolPermissions {
    /// Agent ID.
    pub agent_id: String,
    /// Allowed tool name prefixes (e.g. ["agnos_health", "phylax_"]).
    /// Empty = all tools allowed (default for backward compat).
    pub allowed_prefixes: Vec<String>,
    /// Explicitly denied tool names (overrides allowed_prefixes).
    pub denied_tools: Vec<String>,
    /// Maximum tool calls per minute for this agent.
    pub rate_limit: u32,
    /// Bridge profile this agent is restricted to (if any).
    pub bridge_profile: Option<String>,
}

impl AgentToolPermissions {
    /// Check if the agent is allowed to call the given tool.
    pub fn is_allowed(&self, tool_name: &str) -> bool {
        // Explicit deny always wins
        if self.denied_tools.iter().any(|d| d == tool_name) {
            return false;
        }
        // Empty allowed_prefixes = allow all (backward compat)
        if self.allowed_prefixes.is_empty() {
            return true;
        }
        // Check prefix match
        self.allowed_prefixes
            .iter()
            .any(|prefix| tool_name.starts_with(prefix))
    }
}

/// Predefined tool profiles for edge devices.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EdgeToolProfile {
    pub name: String,
    pub description: String,
    /// Allowed tool prefixes for this profile.
    pub allowed_prefixes: Vec<String>,
    /// Minimum memory (MB) the edge device needs for this profile.
    pub min_memory_mb: u64,
    /// Maximum concurrent tool calls.
    pub max_concurrent: u32,
    /// Tool call timeout in seconds.
    pub timeout_secs: u64,
}

/// Predefined edge tool profiles.
pub fn default_edge_profiles() -> Vec<EdgeToolProfile> {
    vec![
        EdgeToolProfile {
            name: "sensor".into(),
            description: "Minimal profile for telemetry and health monitoring".into(),
            allowed_prefixes: vec![
                "agnos_health".into(),
                "agnos_agents".into(),
                "agnos_edge_".into(),
                "edge_".into(),
                "system_".into(),
            ],
            min_memory_mb: 32,
            max_concurrent: 2,
            timeout_secs: 30,
        },
        EdgeToolProfile {
            name: "security".into(),
            description: "Security-focused profile for network monitoring and threat detection".into(),
            allowed_prefixes: vec![
                "agnos_health".into(),
                "agnos_agents".into(),
                "phylax_".into(),
                "network_".into(),
                "sec_".into(),
                "dlp_".into(),
                "twingate_".into(),
                "agnos_audit_".into(),
            ],
            min_memory_mb: 64,
            max_concurrent: 4,
            timeout_secs: 60,
        },
        EdgeToolProfile {
            name: "devops".into(),
            description: "DevOps profile for CI/CD and container management".into(),
            allowed_prefixes: vec![
                "agnos_health".into(),
                "agnos_agents".into(),
                "docker_".into(),
                "gha_".into(),
                "jenkins_".into(),
                "gitlab_".into(),
                "git_".into(),
                "terminal_".into(),
            ],
            min_memory_mb: 128,
            max_concurrent: 4,
            timeout_secs: 120,
        },
        EdgeToolProfile {
            name: "full".into(),
            description: "Full access to all tools (requires adequate resources)".into(),
            allowed_prefixes: vec![], // Empty = allow all
            min_memory_mb: 256,
            max_concurrent: 8,
            timeout_secs: 300,
        },
    ]
}

/// Store for agent tool permissions, indexed by agent ID.
#[derive(Debug, Clone)]
pub struct ToolSandboxStore {
    permissions: Arc<RwLock<HashMap<String, AgentToolPermissions>>>,
    edge_profiles: Arc<RwLock<Vec<EdgeToolProfile>>>,
}

impl ToolSandboxStore {
    pub fn new() -> Self {
        Self {
            permissions: Arc::new(RwLock::new(HashMap::new())),
            edge_profiles: Arc::new(RwLock::new(default_edge_profiles())),
        }
    }

    /// Set permissions for an agent.
    pub async fn set_permissions(&self, perms: AgentToolPermissions) {
        let mut map = self.permissions.write().await;
        map.insert(perms.agent_id.clone(), perms);
    }

    /// Remove permissions for an agent (reverts to allow-all).
    pub async fn remove_permissions(&self, agent_id: &str) {
        let mut map = self.permissions.write().await;
        map.remove(agent_id);
    }

    /// Check if an agent is allowed to call a tool.
    /// Returns Ok(()) if allowed, Err(reason) if denied.
    pub async fn check_access(&self, agent_id: &str, tool_name: &str) -> Result<(), String> {
        let map = self.permissions.read().await;
        match map.get(agent_id) {
            None => Ok(()), // No permissions set = allow all (backward compat)
            Some(perms) => {
                if perms.is_allowed(tool_name) {
                    Ok(())
                } else {
                    Err(format!(
                        "Agent '{}' is not permitted to call tool '{}'. Allowed prefixes: {:?}",
                        agent_id, tool_name, perms.allowed_prefixes
                    ))
                }
            }
        }
    }

    /// Get all permissions (for inspection/debugging).
    pub async fn list_permissions(&self) -> Vec<AgentToolPermissions> {
        let map = self.permissions.read().await;
        map.values().cloned().collect()
    }

    /// Get edge profiles.
    pub async fn list_edge_profiles(&self) -> Vec<EdgeToolProfile> {
        let profiles = self.edge_profiles.read().await;
        profiles.clone()
    }

    /// Find an edge profile by name.
    pub async fn get_edge_profile(&self, name: &str) -> Option<EdgeToolProfile> {
        let profiles = self.edge_profiles.read().await;
        profiles.iter().find(|p| p.name == name).cloned()
    }

    /// Apply an edge profile to an agent — sets the agent's tool permissions
    /// based on the profile's allowed prefixes.
    pub async fn apply_edge_profile(
        &self,
        agent_id: &str,
        profile_name: &str,
    ) -> Result<AgentToolPermissions, String> {
        let profile = self
            .get_edge_profile(profile_name)
            .await
            .ok_or_else(|| format!("Edge profile '{}' not found", profile_name))?;

        let perms = AgentToolPermissions {
            agent_id: agent_id.to_string(),
            allowed_prefixes: profile.allowed_prefixes.clone(),
            denied_tools: vec![],
            rate_limit: profile.max_concurrent * 15, // 15 calls per concurrent slot per minute
            bridge_profile: Some(profile_name.to_string()),
        };

        self.set_permissions(perms.clone()).await;
        Ok(perms)
    }
}

impl Default for ToolSandboxStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allow_all_when_empty_prefixes() {
        let perms = AgentToolPermissions {
            agent_id: "agent-1".into(),
            allowed_prefixes: vec![],
            denied_tools: vec![],
            rate_limit: 30,
            bridge_profile: None,
        };
        assert!(perms.is_allowed("anything_goes"));
        assert!(perms.is_allowed("phylax_scan"));
    }

    #[test]
    fn test_prefix_filtering() {
        let perms = AgentToolPermissions {
            agent_id: "agent-1".into(),
            allowed_prefixes: vec!["agnos_health".into(), "phylax_".into()],
            denied_tools: vec![],
            rate_limit: 30,
            bridge_profile: None,
        };
        assert!(perms.is_allowed("agnos_health"));
        assert!(perms.is_allowed("phylax_scan"));
        assert!(perms.is_allowed("phylax_rules"));
        assert!(!perms.is_allowed("docker_ps"));
        assert!(!perms.is_allowed("agnos_memory_get"));
    }

    #[test]
    fn test_deny_overrides_allow() {
        let perms = AgentToolPermissions {
            agent_id: "agent-1".into(),
            allowed_prefixes: vec!["phylax_".into()],
            denied_tools: vec!["phylax_scan".into()],
            rate_limit: 30,
            bridge_profile: None,
        };
        assert!(!perms.is_allowed("phylax_scan"));
        assert!(perms.is_allowed("phylax_rules"));
    }

    #[tokio::test]
    async fn test_store_set_and_check() {
        let store = ToolSandboxStore::new();
        store
            .set_permissions(AgentToolPermissions {
                agent_id: "agent-1".into(),
                allowed_prefixes: vec!["phylax_".into()],
                denied_tools: vec![],
                rate_limit: 30,
                bridge_profile: None,
            })
            .await;

        assert!(store.check_access("agent-1", "phylax_scan").await.is_ok());
        assert!(store.check_access("agent-1", "docker_ps").await.is_err());
        // Unknown agent = allow all
        assert!(store.check_access("unknown", "docker_ps").await.is_ok());
    }

    #[tokio::test]
    async fn test_remove_permissions() {
        let store = ToolSandboxStore::new();
        store
            .set_permissions(AgentToolPermissions {
                agent_id: "agent-1".into(),
                allowed_prefixes: vec!["phylax_".into()],
                denied_tools: vec![],
                rate_limit: 30,
                bridge_profile: None,
            })
            .await;

        assert!(store.check_access("agent-1", "docker_ps").await.is_err());
        store.remove_permissions("agent-1").await;
        assert!(store.check_access("agent-1", "docker_ps").await.is_ok());
    }

    #[tokio::test]
    async fn test_edge_profiles_exist() {
        let store = ToolSandboxStore::new();
        let profiles = store.list_edge_profiles().await;
        assert!(profiles.len() >= 4);
        assert!(profiles.iter().any(|p| p.name == "sensor"));
        assert!(profiles.iter().any(|p| p.name == "security"));
        assert!(profiles.iter().any(|p| p.name == "devops"));
        assert!(profiles.iter().any(|p| p.name == "full"));
    }

    #[tokio::test]
    async fn test_apply_edge_profile() {
        let store = ToolSandboxStore::new();
        let perms = store.apply_edge_profile("edge-agent-1", "sensor").await.unwrap();
        assert_eq!(perms.bridge_profile, Some("sensor".to_string()));
        assert!(perms.allowed_prefixes.contains(&"edge_".to_string()));
        assert!(!perms.allowed_prefixes.contains(&"docker_".to_string()));

        // Verify it's enforced
        assert!(store.check_access("edge-agent-1", "edge_list").await.is_ok());
        assert!(store.check_access("edge-agent-1", "docker_ps").await.is_err());
    }

    #[tokio::test]
    async fn test_apply_full_profile() {
        let store = ToolSandboxStore::new();
        let perms = store.apply_edge_profile("edge-agent-1", "full").await.unwrap();
        assert!(perms.allowed_prefixes.is_empty()); // empty = allow all
        assert!(store.check_access("edge-agent-1", "anything").await.is_ok());
    }

    #[tokio::test]
    async fn test_unknown_profile_error() {
        let store = ToolSandboxStore::new();
        let result = store.apply_edge_profile("agent", "nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_sensor_profile_constraints() {
        let store = ToolSandboxStore::new();
        let profile = store.get_edge_profile("sensor").await.unwrap();
        assert_eq!(profile.min_memory_mb, 32);
        assert_eq!(profile.max_concurrent, 2);
        assert_eq!(profile.timeout_secs, 30);
    }
}
