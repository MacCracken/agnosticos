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
