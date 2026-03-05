//! Token accounting for multi-agent LLM access

use std::collections::HashMap;

use agnos_common::{AgentId, TokenUsage};
use tokio::sync::RwLock;
use tracing::debug;

/// Token accounting system for tracking usage per agent
pub struct TokenAccounting {
    /// Usage per agent
    agent_usage: RwLock<HashMap<AgentId, TokenUsage>>,
    /// Total usage across all agents
    total_usage: RwLock<TokenUsage>,
}

impl TokenAccounting {
    /// Create a new token accounting system
    pub fn new() -> Self {
        Self {
            agent_usage: RwLock::new(HashMap::new()),
            total_usage: RwLock::new(TokenUsage::default()),
        }
    }

    /// Record token usage for an agent
    pub async fn record_usage(&self, agent_id: AgentId, usage: TokenUsage) {
        // Update agent-specific usage
        let mut agent_usage = self.agent_usage.write().await;
        let entry = agent_usage.entry(agent_id).or_insert_with(TokenUsage::default);
        entry.prompt_tokens += usage.prompt_tokens;
        entry.completion_tokens += usage.completion_tokens;
        entry.total_tokens += usage.total_tokens;
        drop(agent_usage);

        // Update total usage
        let mut total = self.total_usage.write().await;
        total.prompt_tokens += usage.prompt_tokens;
        total.completion_tokens += usage.completion_tokens;
        total.total_tokens += usage.total_tokens;

        debug!(
            "Recorded usage for agent {}: {} tokens (total: {})",
            agent_id, usage.total_tokens, total.total_tokens
        );
    }

    /// Get token usage for a specific agent
    pub async fn get_usage(&self, agent_id: AgentId) -> Option<TokenUsage> {
        self.agent_usage.read().await.get(&agent_id).copied()
    }

    /// Get total usage across all agents
    pub async fn get_total_usage(&self) -> TokenUsage {
        *self.total_usage.read().await
    }

    /// Reset usage for a specific agent
    pub async fn reset_usage(&self, agent_id: AgentId) {
        let mut agent_usage = self.agent_usage.write().await;
        agent_usage.remove(&agent_id);
        debug!("Reset token usage for agent {}", agent_id);
    }

    /// Reset all usage data
    pub async fn reset_all(&self) {
        let mut agent_usage = self.agent_usage.write().await;
        agent_usage.clear();
        
        let mut total = self.total_usage.write().await;
        *total = TokenUsage::default();
        
        debug!("Reset all token usage");
    }

    /// List all agents with usage
    pub async fn list_agents(&self) -> Vec<(AgentId, TokenUsage)> {
        self.agent_usage
            .read()
            .await
            .iter()
            .map(|(k, v)| (*k, *v))
            .collect()
    }

    /// Get usage statistics
    pub async fn stats(&self) -> AccountingStats {
        let agents = self.agent_usage.read().await;
        let total = self.total_usage.read().await;
        
        AccountingStats {
            total_agents: agents.len(),
            total_prompt_tokens: total.prompt_tokens,
            total_completion_tokens: total.completion_tokens,
            total_tokens: total.total_tokens,
        }
    }
}

impl Default for TokenAccounting {
    fn default() -> Self {
        Self::new()
    }
}

/// Accounting statistics
#[derive(Debug, Clone, Copy)]
pub struct AccountingStats {
    pub total_agents: usize,
    pub total_prompt_tokens: u32,
    pub total_completion_tokens: u32,
    pub total_tokens: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use agnos_common::{AgentId, TokenUsage};

    #[tokio::test]
    async fn test_token_accounting_new() {
        let accounting = TokenAccounting::new();
        let stats = accounting.stats().await;
        assert_eq!(stats.total_agents, 0);
        assert_eq!(stats.total_tokens, 0);
    }

    #[tokio::test]
    async fn test_record_usage() {
        let accounting = TokenAccounting::new();
        let agent_id = AgentId::new();
        
        let usage = TokenUsage {
            prompt_tokens: 100,
            completion_tokens: 200,
            total_tokens: 300,
        };
        
        accounting.record_usage(agent_id, usage).await;
        
        let agent_usage = accounting.get_usage(agent_id).await;
        assert!(agent_usage.is_some());
        assert_eq!(agent_usage.unwrap().total_tokens, 300);
    }

    #[tokio::test]
    async fn test_get_total_usage() {
        let accounting = TokenAccounting::new();
        let agent_id = AgentId::new();
        
        let usage = TokenUsage {
            prompt_tokens: 50,
            completion_tokens: 100,
            total_tokens: 150,
        };
        
        accounting.record_usage(agent_id, usage).await;
        
        let total = accounting.get_total_usage().await;
        assert_eq!(total.total_tokens, 150);
    }

    #[tokio::test]
    async fn test_reset_usage() {
        let accounting = TokenAccounting::new();
        let agent_id = AgentId::new();
        
        let usage = TokenUsage {
            prompt_tokens: 10,
            completion_tokens: 20,
            total_tokens: 30,
        };
        
        accounting.record_usage(agent_id, usage).await;
        accounting.reset_usage(agent_id).await;
        
        let agent_usage = accounting.get_usage(agent_id).await;
        assert!(agent_usage.is_none());
    }

    #[tokio::test]
    async fn test_reset_all() {
        let accounting = TokenAccounting::new();
        let agent_id = AgentId::new();
        
        let usage = TokenUsage {
            prompt_tokens: 10,
            completion_tokens: 20,
            total_tokens: 30,
        };
        
        accounting.record_usage(agent_id, usage).await;
        accounting.reset_all().await;
        
        let stats = accounting.stats().await;
        assert_eq!(stats.total_agents, 0);
        assert_eq!(stats.total_tokens, 0);
    }

    #[tokio::test]
    async fn test_list_agents() {
        let accounting = TokenAccounting::new();
        
        let agent1 = AgentId::new();
        let agent2 = AgentId::new();
        
        accounting.record_usage(agent1, TokenUsage { prompt_tokens: 10, completion_tokens: 20, total_tokens: 30 }).await;
        accounting.record_usage(agent2, TokenUsage { prompt_tokens: 15, completion_tokens: 25, total_tokens: 40 }).await;
        
        let agents = accounting.list_agents().await;
        assert_eq!(agents.len(), 2);
    }

    #[tokio::test]
    async fn test_multiple_agents() {
        let accounting = TokenAccounting::new();

        let agent1 = AgentId::new();
        let agent2 = AgentId::new();

        accounting.record_usage(agent1, TokenUsage { prompt_tokens: 100, completion_tokens: 200, total_tokens: 300 }).await;
        accounting.record_usage(agent2, TokenUsage { prompt_tokens: 50, completion_tokens: 100, total_tokens: 150 }).await;

        let total = accounting.get_total_usage().await;
        assert_eq!(total.total_tokens, 450);
    }

    #[tokio::test]
    async fn test_stats() {
        let accounting = TokenAccounting::new();
        let agent1 = AgentId::new();
        let agent2 = AgentId::new();

        accounting.record_usage(agent1, TokenUsage { prompt_tokens: 10, completion_tokens: 20, total_tokens: 30 }).await;
        accounting.record_usage(agent2, TokenUsage { prompt_tokens: 5, completion_tokens: 15, total_tokens: 20 }).await;

        let stats = accounting.stats().await;
        assert_eq!(stats.total_agents, 2);
        assert_eq!(stats.total_prompt_tokens, 15);
        assert_eq!(stats.total_completion_tokens, 35);
        assert_eq!(stats.total_tokens, 50);
    }

    #[tokio::test]
    async fn test_default() {
        let accounting = TokenAccounting::default();
        let stats = accounting.stats().await;
        assert_eq!(stats.total_agents, 0);
    }

    #[tokio::test]
    async fn test_get_usage_nonexistent() {
        let accounting = TokenAccounting::new();
        let usage = accounting.get_usage(AgentId::new()).await;
        assert!(usage.is_none());
    }

    #[tokio::test]
    async fn test_accumulate_usage_same_agent() {
        let accounting = TokenAccounting::new();
        let agent = AgentId::new();

        accounting.record_usage(agent, TokenUsage { prompt_tokens: 10, completion_tokens: 20, total_tokens: 30 }).await;
        accounting.record_usage(agent, TokenUsage { prompt_tokens: 5, completion_tokens: 10, total_tokens: 15 }).await;

        let usage = accounting.get_usage(agent).await.unwrap();
        assert_eq!(usage.prompt_tokens, 15);
        assert_eq!(usage.completion_tokens, 30);
        assert_eq!(usage.total_tokens, 45);
    }
}
