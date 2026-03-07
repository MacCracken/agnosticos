//! Agent registry for discovery and management

use anyhow::Result;
use dashmap::DashMap;
use tokio::sync::RwLock;
use tracing::{debug, info};

use agnos_common::{AgentConfig, AgentId, AgentStatus, ResourceUsage};

use crate::agent::{Agent, AgentHandle};

/// Central registry for all agents
pub struct AgentRegistry {
    /// All registered agents by ID
    agents: DashMap<AgentId, RegisteredAgent>,
    /// Agents indexed by name for lookup
    by_name: DashMap<String, AgentId>,
    /// Agents indexed by capability
    by_capability: DashMap<String, Vec<AgentId>>,
    /// Agent statistics
    stats: RwLock<RegistryStats>,
}

/// Internal representation of a registered agent
struct RegisteredAgent {
    handle: AgentHandle,
    config: AgentConfig,
    capabilities: Vec<String>,
}

/// Registry statistics
#[derive(Debug, Default, Clone)]
pub struct RegistryStats {
    pub total_registered: u64,
    pub total_started: u64,
    pub total_stopped: u64,
    pub total_failed: u64,
}

impl AgentRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            agents: DashMap::new(),
            by_name: DashMap::new(),
            by_capability: DashMap::new(),
            stats: RwLock::new(RegistryStats::default()),
        }
    }

    /// Register a new agent
    pub async fn register(&self, agent: &Agent, config: AgentConfig) -> Result<AgentHandle> {
        let handle = agent.handle().await;
        let id = handle.id;
        let name = handle.name.clone();

        // Check for duplicate names
        if self.by_name.contains_key(&name) {
            return Err(anyhow::anyhow!("Agent with name '{}' already exists", name));
        }

        let capabilities = Self::extract_capabilities(&config);

        let registered = RegisteredAgent {
            handle: handle.clone(),
            config,
            capabilities: capabilities.clone(),
        };

        // Insert into primary storage
        self.agents.insert(id, registered);
        self.by_name.insert(name.clone(), id);

        // Index by capabilities
        for cap in capabilities {
            self.by_capability.entry(cap).or_default().push(id);
        }

        let mut stats = self.stats.write().await;
        stats.total_registered += 1;

        info!("Registered agent {} ({})", name, id);
        debug!("Registry now contains {} agents", self.agents.len());

        Ok(handle)
    }

    /// Unregister an agent
    pub async fn unregister(&self, id: AgentId) -> Result<()> {
        if let Some((_, agent)) = self.agents.remove(&id) {
            self.by_name.remove(&agent.handle.name);

            // Remove from capability indices
            for cap in &agent.capabilities {
                if let Some(mut agents) = self.by_capability.get_mut(cap) {
                    agents.retain(|&agent_id| agent_id != id);
                }
            }

            info!("Unregistered agent {} ({})", agent.handle.name, id);
        }

        Ok(())
    }

    /// Get an agent by ID
    pub fn get(&self, id: AgentId) -> Option<AgentHandle> {
        self.agents.get(&id).map(|a| a.handle.clone())
    }

    /// Get an agent by name
    pub fn get_by_name(&self, name: &str) -> Option<AgentHandle> {
        self.by_name
            .get(name)
            .and_then(|id| self.agents.get(&*id))
            .map(|a| a.handle.clone())
    }

    /// Find agents by capability
    pub fn find_by_capability(&self, capability: &str) -> Vec<AgentHandle> {
        self.by_capability
            .get(capability)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.agents.get(id).map(|a| a.handle.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// List all agents
    pub fn list_all(&self) -> Vec<AgentHandle> {
        self.agents.iter().map(|a| a.handle.clone()).collect()
    }

    /// List agents by status
    pub fn list_by_status(&self, status: AgentStatus) -> Vec<AgentHandle> {
        self.agents
            .iter()
            .filter(|a| a.handle.status == status)
            .map(|a| a.handle.clone())
            .collect()
    }

    /// Update agent status
    pub async fn update_status(&self, id: AgentId, status: AgentStatus) -> Result<()> {
        if let Some(mut agent) = self.agents.get_mut(&id) {
            agent.handle.status = status;
            debug!("Updated agent {} status to {:?}", id, status);
        } else {
            return Err(anyhow::anyhow!("Agent {} not found", id));
        }

        Ok(())
    }

    /// Update agent resource usage
    pub async fn update_resource_usage(&self, id: AgentId, usage: ResourceUsage) -> Result<()> {
        if let Some(mut agent) = self.agents.get_mut(&id) {
            agent.handle.resource_usage = usage;
        }
        Ok(())
    }

    /// Get agent configuration
    pub fn get_config(&self, id: AgentId) -> Option<AgentConfig> {
        self.agents.get(&id).map(|a| a.config.clone())
    }

    /// Get registry statistics
    pub async fn stats(&self) -> RegistryStats {
        self.stats.read().await.clone()
    }

    /// Check if an agent exists
    pub fn contains(&self, id: AgentId) -> bool {
        self.agents.contains_key(&id)
    }

    /// Get total number of registered agents
    pub fn len(&self) -> usize {
        self.agents.len()
    }

    /// Check if registry is empty
    pub fn is_empty(&self) -> bool {
        self.agents.is_empty()
    }

    /// Extract capabilities from agent configuration
    fn extract_capabilities(config: &AgentConfig) -> Vec<String> {
        let mut caps = vec![format!("type:{:?}", config.agent_type).to_lowercase()];

        // Add capability based on permissions
        for perm in &config.permissions {
            caps.push(format!("perm:{:?}", perm).to_lowercase());
        }

        caps
    }
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_new() {
        let registry = AgentRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn test_registry_contains() {
        let registry = AgentRegistry::new();
        let id = AgentId::new();
        assert!(!registry.contains(id));
    }

    #[test]
    fn test_registry_stats_default() {
        let stats = RegistryStats::default();
        assert_eq!(stats.total_registered, 0);
        assert_eq!(stats.total_started, 0);
        assert_eq!(stats.total_stopped, 0);
    }

    #[test]
    fn test_registry_stats_clone() {
        let stats = RegistryStats {
            total_registered: 10,
            total_started: 5,
            total_stopped: 3,
            total_failed: 2,
        };
        let cloned = stats.clone();
        assert_eq!(cloned.total_registered, 10);
        assert_eq!(cloned.total_started, 5);
    }

    #[tokio::test]
    async fn test_registry_stats_async() {
        let registry = AgentRegistry::new();
        let stats = registry.stats().await;
        assert_eq!(stats.total_registered, 0);
    }

    #[test]
    fn test_extract_capabilities() {
        let config = AgentConfig {
            name: "test".to_string(),
            agent_type: agnos_common::AgentType::Service,
            permissions: vec![
                agnos_common::Permission::FileRead,
                agnos_common::Permission::NetworkAccess,
            ],
            ..Default::default()
        };

        let caps = AgentRegistry::extract_capabilities(&config);
        assert!(!caps.is_empty());
    }

    #[test]
    fn test_extract_capabilities_all_types() {
        let config = AgentConfig {
            name: "cap-test".to_string(),
            agent_type: agnos_common::AgentType::System,
            permissions: vec![
                agnos_common::Permission::FileRead,
                agnos_common::Permission::FileWrite,
                agnos_common::Permission::NetworkAccess,
                agnos_common::Permission::ProcessSpawn,
                agnos_common::Permission::LlmInference,
                agnos_common::Permission::AuditRead,
            ],
            ..Default::default()
        };

        let caps = AgentRegistry::extract_capabilities(&config);
        // 1 type cap + 6 permission caps = 7
        assert_eq!(caps.len(), 7);
        assert!(caps[0].starts_with("type:"));
        for cap in &caps[1..] {
            assert!(cap.starts_with("perm:"));
        }
    }

    #[test]
    fn test_extract_capabilities_no_permissions() {
        let config = AgentConfig {
            name: "minimal".to_string(),
            agent_type: agnos_common::AgentType::User,
            permissions: vec![],
            ..Default::default()
        };

        let caps = AgentRegistry::extract_capabilities(&config);
        assert_eq!(caps.len(), 1); // Only type cap
        assert!(caps[0].contains("user"));
    }

    #[test]
    fn test_registry_default() {
        let registry = AgentRegistry::default();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn test_registry_get_nonexistent() {
        let registry = AgentRegistry::new();
        assert!(registry.get(AgentId::new()).is_none());
    }

    #[test]
    fn test_registry_get_by_name_nonexistent() {
        let registry = AgentRegistry::new();
        assert!(registry.get_by_name("nonexistent").is_none());
    }

    #[test]
    fn test_registry_find_by_capability_empty() {
        let registry = AgentRegistry::new();
        let found = registry.find_by_capability("gpu");
        assert!(found.is_empty());
    }

    #[test]
    fn test_registry_list_all_empty() {
        let registry = AgentRegistry::new();
        assert!(registry.list_all().is_empty());
    }

    #[test]
    fn test_registry_list_by_status_empty() {
        let registry = AgentRegistry::new();
        assert!(registry.list_by_status(AgentStatus::Running).is_empty());
    }

    #[test]
    fn test_registry_get_config_nonexistent() {
        let registry = AgentRegistry::new();
        assert!(registry.get_config(AgentId::new()).is_none());
    }

    #[test]
    fn test_registry_stats_debug() {
        let stats = RegistryStats {
            total_registered: 10,
            total_started: 8,
            total_stopped: 2,
            total_failed: 1,
        };
        let debug = format!("{:?}", stats);
        assert!(debug.contains("total_registered"));
        assert!(debug.contains("10"));
        assert!(debug.contains("total_failed"));
        assert!(debug.contains("1"));
    }

    #[test]
    fn test_registry_stats_default_all_zero() {
        let stats = RegistryStats::default();
        assert_eq!(stats.total_registered, 0);
        assert_eq!(stats.total_started, 0);
        assert_eq!(stats.total_stopped, 0);
        assert_eq!(stats.total_failed, 0);
    }

    #[tokio::test]
    async fn test_registry_register_and_get() {
        let registry = AgentRegistry::new();

        let config = AgentConfig {
            name: "test-agent".to_string(),
            agent_type: agnos_common::AgentType::Service,
            permissions: vec![agnos_common::Permission::FileRead],
            ..Default::default()
        };

        let (agent, _rx) = Agent::new(config.clone()).await.unwrap();
        let handle = registry.register(&agent, config).await.unwrap();

        assert_eq!(handle.name, "test-agent");
        assert_eq!(handle.status, AgentStatus::Pending);
        assert_eq!(registry.len(), 1);
        assert!(!registry.is_empty());

        // Get by ID
        let found = registry.get(handle.id).unwrap();
        assert_eq!(found.name, "test-agent");

        // Get by name
        let found = registry.get_by_name("test-agent").unwrap();
        assert_eq!(found.id, handle.id);

        // Contains
        assert!(registry.contains(handle.id));
    }

    #[tokio::test]
    async fn test_registry_register_duplicate_name() {
        let registry = AgentRegistry::new();

        let config1 = AgentConfig {
            name: "dup-agent".to_string(),
            ..Default::default()
        };
        let config2 = AgentConfig {
            name: "dup-agent".to_string(),
            ..Default::default()
        };

        let (agent1, _rx1) = Agent::new(config1.clone()).await.unwrap();
        registry.register(&agent1, config1).await.unwrap();

        let (agent2, _rx2) = Agent::new(config2.clone()).await.unwrap();
        let result = registry.register(&agent2, config2).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already exists"));
    }

    #[tokio::test]
    async fn test_registry_unregister() {
        let registry = AgentRegistry::new();

        let config = AgentConfig {
            name: "unreg-agent".to_string(),
            ..Default::default()
        };

        let (agent, _rx) = Agent::new(config.clone()).await.unwrap();
        let handle = registry.register(&agent, config).await.unwrap();

        assert_eq!(registry.len(), 1);
        registry.unregister(handle.id).await.unwrap();
        assert_eq!(registry.len(), 0);
        assert!(registry.get(handle.id).is_none());
        assert!(registry.get_by_name("unreg-agent").is_none());
    }

    #[tokio::test]
    async fn test_registry_unregister_nonexistent() {
        let registry = AgentRegistry::new();
        // Should succeed (no-op)
        let result = registry.unregister(AgentId::new()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_registry_update_status() {
        let registry = AgentRegistry::new();

        let config = AgentConfig {
            name: "status-agent".to_string(),
            ..Default::default()
        };

        let (agent, _rx) = Agent::new(config.clone()).await.unwrap();
        let handle = registry.register(&agent, config).await.unwrap();

        registry
            .update_status(handle.id, AgentStatus::Running)
            .await
            .unwrap();
        let updated = registry.get(handle.id).unwrap();
        assert_eq!(updated.status, AgentStatus::Running);
    }

    #[tokio::test]
    async fn test_registry_update_status_nonexistent() {
        let registry = AgentRegistry::new();
        let result = registry
            .update_status(AgentId::new(), AgentStatus::Running)
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_registry_update_resource_usage() {
        let registry = AgentRegistry::new();

        let config = AgentConfig {
            name: "resource-agent".to_string(),
            ..Default::default()
        };

        let (agent, _rx) = Agent::new(config.clone()).await.unwrap();
        let handle = registry.register(&agent, config).await.unwrap();

        let usage = ResourceUsage {
            memory_used: 1024,
            cpu_time_used: 500,
            file_descriptors_used: 5,
            processes_used: 1,
        };
        registry
            .update_resource_usage(handle.id, usage)
            .await
            .unwrap();

        let updated = registry.get(handle.id).unwrap();
        assert_eq!(updated.resource_usage.memory_used, 1024);
        assert_eq!(updated.resource_usage.cpu_time_used, 500);
    }

    #[tokio::test]
    async fn test_registry_list_all() {
        let registry = AgentRegistry::new();

        let config1 = AgentConfig {
            name: "agent-a".to_string(),
            ..Default::default()
        };
        let config2 = AgentConfig {
            name: "agent-b".to_string(),
            ..Default::default()
        };

        let (a1, _rx1) = Agent::new(config1.clone()).await.unwrap();
        let (a2, _rx2) = Agent::new(config2.clone()).await.unwrap();
        registry.register(&a1, config1).await.unwrap();
        registry.register(&a2, config2).await.unwrap();

        let all = registry.list_all();
        assert_eq!(all.len(), 2);
    }

    #[tokio::test]
    async fn test_registry_list_by_status() {
        let registry = AgentRegistry::new();

        let config1 = AgentConfig {
            name: "running-agent".to_string(),
            ..Default::default()
        };
        let config2 = AgentConfig {
            name: "stopped-agent".to_string(),
            ..Default::default()
        };

        let (a1, _rx1) = Agent::new(config1.clone()).await.unwrap();
        let (a2, _rx2) = Agent::new(config2.clone()).await.unwrap();
        let h1 = registry.register(&a1, config1).await.unwrap();
        let h2 = registry.register(&a2, config2).await.unwrap();

        registry
            .update_status(h1.id, AgentStatus::Running)
            .await
            .unwrap();
        registry
            .update_status(h2.id, AgentStatus::Stopped)
            .await
            .unwrap();

        let running = registry.list_by_status(AgentStatus::Running);
        assert_eq!(running.len(), 1);
        assert_eq!(running[0].name, "running-agent");

        let stopped = registry.list_by_status(AgentStatus::Stopped);
        assert_eq!(stopped.len(), 1);
        assert_eq!(stopped[0].name, "stopped-agent");
    }

    #[tokio::test]
    async fn test_registry_find_by_capability() {
        let registry = AgentRegistry::new();

        let config = AgentConfig {
            name: "capable-agent".to_string(),
            agent_type: agnos_common::AgentType::Service,
            permissions: vec![agnos_common::Permission::NetworkAccess],
            ..Default::default()
        };

        let (agent, _rx) = Agent::new(config.clone()).await.unwrap();
        registry.register(&agent, config).await.unwrap();

        // The capability is stored as "perm:networkaccess" (lowercase debug format)
        let found = registry.find_by_capability("perm:networkaccess");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "capable-agent");

        // Non-matching capability
        let not_found = registry.find_by_capability("perm:filewrite");
        assert!(not_found.is_empty());
    }

    #[tokio::test]
    async fn test_registry_get_config() {
        let registry = AgentRegistry::new();

        let config = AgentConfig {
            name: "config-agent".to_string(),
            agent_type: agnos_common::AgentType::System,
            ..Default::default()
        };

        let (agent, _rx) = Agent::new(config.clone()).await.unwrap();
        let handle = registry.register(&agent, config).await.unwrap();

        let retrieved_config = registry.get_config(handle.id).unwrap();
        assert_eq!(retrieved_config.name, "config-agent");
        assert!(matches!(
            retrieved_config.agent_type,
            agnos_common::AgentType::System
        ));
    }

    #[tokio::test]
    async fn test_registry_stats_after_register() {
        let registry = AgentRegistry::new();

        let config = AgentConfig {
            name: "stats-agent".to_string(),
            ..Default::default()
        };

        let (agent, _rx) = Agent::new(config.clone()).await.unwrap();
        registry.register(&agent, config).await.unwrap();

        let stats = registry.stats().await;
        assert_eq!(stats.total_registered, 1);
    }

    // ==================================================================
    // New coverage: concurrent register/unregister, capability indexing,
    // update_resource_usage for nonexistent, multiple agents by capability
    // ==================================================================

    #[tokio::test]
    async fn test_registry_update_resource_usage_nonexistent() {
        let registry = AgentRegistry::new();
        let usage = ResourceUsage {
            memory_used: 100,
            cpu_time_used: 50,
            file_descriptors_used: 2,
            processes_used: 1,
        };
        // Should not error — just a no-op
        let result = registry.update_resource_usage(AgentId::new(), usage).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_registry_multiple_agents_same_capability() {
        let registry = AgentRegistry::new();

        for name in ["cap-a1", "cap-a2", "cap-a3"] {
            let config = AgentConfig {
                name: name.to_string(),
                agent_type: agnos_common::AgentType::Service,
                permissions: vec![agnos_common::Permission::NetworkAccess],
                ..Default::default()
            };
            let (agent, _rx) = Agent::new(config.clone()).await.unwrap();
            registry.register(&agent, config).await.unwrap();
        }

        let found = registry.find_by_capability("perm:networkaccess");
        assert_eq!(found.len(), 3);
    }

    #[tokio::test]
    async fn test_registry_unregister_removes_capabilities() {
        let registry = AgentRegistry::new();

        let config = AgentConfig {
            name: "cap-remove".to_string(),
            agent_type: agnos_common::AgentType::Service,
            permissions: vec![agnos_common::Permission::FileRead],
            ..Default::default()
        };
        let (agent, _rx) = Agent::new(config.clone()).await.unwrap();
        let handle = registry.register(&agent, config).await.unwrap();

        let before = registry.find_by_capability("perm:fileread");
        assert_eq!(before.len(), 1);

        registry.unregister(handle.id).await.unwrap();

        let after = registry.find_by_capability("perm:fileread");
        assert!(after.is_empty());
    }

    #[tokio::test]
    async fn test_registry_stats_increments() {
        let registry = AgentRegistry::new();

        for i in 0..3 {
            let config = AgentConfig {
                name: format!("stat-agent-{}", i),
                ..Default::default()
            };
            let (agent, _rx) = Agent::new(config.clone()).await.unwrap();
            registry.register(&agent, config).await.unwrap();
        }

        let stats = registry.stats().await;
        assert_eq!(stats.total_registered, 3);
    }

    #[tokio::test]
    async fn test_registry_concurrent_register() {
        let registry = std::sync::Arc::new(AgentRegistry::new());
        let mut handles = Vec::new();

        for i in 0..5 {
            let reg = registry.clone();
            let handle = tokio::spawn(async move {
                let config = AgentConfig {
                    name: format!("concurrent-{}", i),
                    ..Default::default()
                };
                let (agent, _rx) = Agent::new(config.clone()).await.unwrap();
                reg.register(&agent, config).await
            });
            handles.push(handle);
        }

        let mut success_count = 0;
        for handle in handles {
            if handle.await.unwrap().is_ok() {
                success_count += 1;
            }
        }
        assert_eq!(success_count, 5);
        assert_eq!(registry.len(), 5);
    }

    #[test]
    fn test_extract_capabilities_service_with_file_perms() {
        let config = AgentConfig {
            name: "svc".to_string(),
            agent_type: agnos_common::AgentType::Service,
            permissions: vec![
                agnos_common::Permission::FileRead,
                agnos_common::Permission::FileWrite,
            ],
            ..Default::default()
        };
        let caps = AgentRegistry::extract_capabilities(&config);
        assert_eq!(caps.len(), 3); // type + 2 perms
        assert!(caps[0].contains("service"));
        assert!(caps.iter().any(|c| c.contains("fileread")));
        assert!(caps.iter().any(|c| c.contains("filewrite")));
    }
}
