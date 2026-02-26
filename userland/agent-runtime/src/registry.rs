//! Agent registry for discovery and management

use std::collections::HashMap;

use anyhow::Result;
use dashmap::DashMap;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

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
            self.by_capability
                .entry(cap)
                .or_insert_with(Vec::new)
                .push(id);
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
        self.agents
            .iter()
            .map(|a| a.handle.clone())
            .collect()
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
}
