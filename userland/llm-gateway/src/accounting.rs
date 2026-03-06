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
        debug!(agent_id = ?agent_id, "Reset token usage for agent");
    }

    /// Reset all usage data
    #[allow(dead_code)]
    pub async fn reset_all(&self) {
        let mut agent_usage = self.agent_usage.write().await;
        agent_usage.clear();
        
        let mut total = self.total_usage.write().await;
        *total = TokenUsage::default();
        
        debug!("Reset all token usage");
    }

    /// Evict dead agents from the usage map.
    /// Call periodically or when an agent is unregistered.
    #[allow(dead_code)]
    pub async fn evict_agents(&self, live_agents: &[AgentId]) {
        let live_set: std::collections::HashSet<_> = live_agents.iter().collect();
        let mut agent_usage = self.agent_usage.write().await;
        let before = agent_usage.len();
        agent_usage.retain(|id, _| live_set.contains(id));
        let evicted = before - agent_usage.len();
        if evicted > 0 {
            debug!("Evicted {} dead agent(s) from token accounting", evicted);
        }
    }

    /// List all agents with usage
    #[allow(dead_code)]
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

    #[tokio::test]
    async fn test_zero_token_usage() {
        let accounting = TokenAccounting::new();
        let agent = AgentId::new();

        accounting.record_usage(agent, TokenUsage {
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
        }).await;

        let usage = accounting.get_usage(agent).await;
        assert!(usage.is_some(), "Zero-token usage should still be recorded");
        let u = usage.unwrap();
        assert_eq!(u.total_tokens, 0);
    }

    #[tokio::test]
    async fn test_reset_usage_does_not_affect_total() {
        let accounting = TokenAccounting::new();
        let agent = AgentId::new();

        accounting.record_usage(agent, TokenUsage {
            prompt_tokens: 50, completion_tokens: 50, total_tokens: 100,
        }).await;

        accounting.reset_usage(agent).await;

        // Agent is gone, but total should still reflect the recorded usage
        let total = accounting.get_total_usage().await;
        assert_eq!(total.total_tokens, 100);
    }

    #[tokio::test]
    async fn test_reset_all_clears_total_too() {
        let accounting = TokenAccounting::new();
        let agent = AgentId::new();

        accounting.record_usage(agent, TokenUsage {
            prompt_tokens: 100, completion_tokens: 200, total_tokens: 300,
        }).await;

        accounting.reset_all().await;

        let total = accounting.get_total_usage().await;
        assert_eq!(total.total_tokens, 0);
        assert_eq!(total.prompt_tokens, 0);
        assert_eq!(total.completion_tokens, 0);
    }

    #[tokio::test]
    async fn test_reset_nonexistent_agent_no_panic() {
        let accounting = TokenAccounting::new();
        let agent = AgentId::new();
        // Should not panic or error
        accounting.reset_usage(agent).await;
        assert!(accounting.get_usage(agent).await.is_none());
    }

    #[tokio::test]
    async fn test_many_agents_stats() {
        let accounting = TokenAccounting::new();

        for _ in 0..100 {
            let agent = AgentId::new();
            accounting.record_usage(agent, TokenUsage {
                prompt_tokens: 1, completion_tokens: 1, total_tokens: 2,
            }).await;
        }

        let stats = accounting.stats().await;
        assert_eq!(stats.total_agents, 100);
        assert_eq!(stats.total_prompt_tokens, 100);
        assert_eq!(stats.total_completion_tokens, 100);
        assert_eq!(stats.total_tokens, 200);
    }

    #[tokio::test]
    async fn test_concurrent_record_usage() {
        let accounting = std::sync::Arc::new(TokenAccounting::new());
        let agent = AgentId::new();

        let mut handles = vec![];
        for _ in 0..10 {
            let acct = accounting.clone();
            let aid = agent;
            handles.push(tokio::spawn(async move {
                acct.record_usage(aid, TokenUsage {
                    prompt_tokens: 1, completion_tokens: 2, total_tokens: 3,
                }).await;
            }));
        }

        for handle in handles {
            handle.await.unwrap();
        }

        let usage = accounting.get_usage(agent).await.unwrap();
        assert_eq!(usage.prompt_tokens, 10);
        assert_eq!(usage.completion_tokens, 20);
        assert_eq!(usage.total_tokens, 30);
    }

    #[tokio::test]
    async fn test_list_agents_after_reset_one() {
        let accounting = TokenAccounting::new();
        let agent1 = AgentId::new();
        let agent2 = AgentId::new();
        let agent3 = AgentId::new();

        accounting.record_usage(agent1, TokenUsage { prompt_tokens: 1, completion_tokens: 1, total_tokens: 2 }).await;
        accounting.record_usage(agent2, TokenUsage { prompt_tokens: 1, completion_tokens: 1, total_tokens: 2 }).await;
        accounting.record_usage(agent3, TokenUsage { prompt_tokens: 1, completion_tokens: 1, total_tokens: 2 }).await;

        accounting.reset_usage(agent2).await;

        let agents = accounting.list_agents().await;
        assert_eq!(agents.len(), 2);
        let ids: Vec<AgentId> = agents.iter().map(|(id, _)| *id).collect();
        assert!(!ids.contains(&agent2));
        assert!(ids.contains(&agent1));
        assert!(ids.contains(&agent3));
    }

    #[tokio::test]
    async fn test_stats_prompt_completion_breakdown() {
        let accounting = TokenAccounting::new();
        let a1 = AgentId::new();
        let a2 = AgentId::new();

        accounting.record_usage(a1, TokenUsage {
            prompt_tokens: 100, completion_tokens: 50, total_tokens: 150,
        }).await;
        accounting.record_usage(a2, TokenUsage {
            prompt_tokens: 200, completion_tokens: 300, total_tokens: 500,
        }).await;

        let stats = accounting.stats().await;
        assert_eq!(stats.total_prompt_tokens, 300);
        assert_eq!(stats.total_completion_tokens, 350);
        assert_eq!(stats.total_tokens, 650);
    }

    #[test]
    fn test_accounting_stats_debug() {
        let stats = AccountingStats {
            total_agents: 5,
            total_prompt_tokens: 1000,
            total_completion_tokens: 2000,
            total_tokens: 3000,
        };
        let dbg = format!("{:?}", stats);
        assert!(dbg.contains("5"));
        assert!(dbg.contains("3000"));
    }

    #[test]
    fn test_accounting_stats_clone() {
        let stats = AccountingStats {
            total_agents: 3,
            total_prompt_tokens: 10,
            total_completion_tokens: 20,
            total_tokens: 30,
        };
        let cloned = stats;
        assert_eq!(cloned.total_agents, 3);
        assert_eq!(cloned.total_tokens, 30);
    }

    // ------------------------------------------------------------------
    // Additional accounting tests: edge cases, concurrent operations,
    // large values, multiple reset cycles, list ordering
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_record_usage_large_values() {
        let accounting = TokenAccounting::new();
        let agent = AgentId::new();
        accounting.record_usage(agent, TokenUsage {
            prompt_tokens: u32::MAX / 2,
            completion_tokens: u32::MAX / 2,
            total_tokens: u32::MAX - 1,
        }).await;
        let usage = accounting.get_usage(agent).await.unwrap();
        assert_eq!(usage.prompt_tokens, u32::MAX / 2);
        assert_eq!(usage.completion_tokens, u32::MAX / 2);
    }

    #[tokio::test]
    async fn test_record_usage_then_reset_then_record_again() {
        let accounting = TokenAccounting::new();
        let agent = AgentId::new();
        accounting.record_usage(agent, TokenUsage {
            prompt_tokens: 100, completion_tokens: 200, total_tokens: 300,
        }).await;
        accounting.reset_usage(agent).await;
        assert!(accounting.get_usage(agent).await.is_none());

        // Record again after reset
        accounting.record_usage(agent, TokenUsage {
            prompt_tokens: 50, completion_tokens: 50, total_tokens: 100,
        }).await;
        let usage = accounting.get_usage(agent).await.unwrap();
        assert_eq!(usage.prompt_tokens, 50);
        assert_eq!(usage.total_tokens, 100);
    }

    #[tokio::test]
    async fn test_reset_all_then_record() {
        let accounting = TokenAccounting::new();
        let a1 = AgentId::new();
        let a2 = AgentId::new();
        accounting.record_usage(a1, TokenUsage {
            prompt_tokens: 10, completion_tokens: 20, total_tokens: 30,
        }).await;
        accounting.record_usage(a2, TokenUsage {
            prompt_tokens: 40, completion_tokens: 50, total_tokens: 90,
        }).await;
        accounting.reset_all().await;

        // Record after reset
        accounting.record_usage(a1, TokenUsage {
            prompt_tokens: 5, completion_tokens: 5, total_tokens: 10,
        }).await;
        let stats = accounting.stats().await;
        assert_eq!(stats.total_agents, 1);
        assert_eq!(stats.total_tokens, 10);
        // a2 should not exist
        assert!(accounting.get_usage(a2).await.is_none());
    }

    #[tokio::test]
    async fn test_list_agents_empty() {
        let accounting = TokenAccounting::new();
        let agents = accounting.list_agents().await;
        assert!(agents.is_empty());
    }

    #[tokio::test]
    async fn test_list_agents_after_reset_all() {
        let accounting = TokenAccounting::new();
        let agent = AgentId::new();
        accounting.record_usage(agent, TokenUsage {
            prompt_tokens: 1, completion_tokens: 1, total_tokens: 2,
        }).await;
        accounting.reset_all().await;
        assert!(accounting.list_agents().await.is_empty());
    }

    #[tokio::test]
    async fn test_concurrent_record_different_agents() {
        let accounting = std::sync::Arc::new(TokenAccounting::new());
        let mut handles = vec![];
        let agents: Vec<AgentId> = (0..20).map(|_| AgentId::new()).collect();

        for &agent in &agents {
            let acct = accounting.clone();
            handles.push(tokio::spawn(async move {
                acct.record_usage(agent, TokenUsage {
                    prompt_tokens: 10, completion_tokens: 5, total_tokens: 15,
                }).await;
            }));
        }

        for handle in handles {
            handle.await.unwrap();
        }

        let stats = accounting.stats().await;
        assert_eq!(stats.total_agents, 20);
        assert_eq!(stats.total_prompt_tokens, 200);
        assert_eq!(stats.total_completion_tokens, 100);
        assert_eq!(stats.total_tokens, 300);
    }

    #[tokio::test]
    async fn test_concurrent_reset_and_record() {
        let accounting = std::sync::Arc::new(TokenAccounting::new());
        let agent = AgentId::new();

        // Pre-populate
        accounting.record_usage(agent, TokenUsage {
            prompt_tokens: 100, completion_tokens: 100, total_tokens: 200,
        }).await;

        let mut handles = vec![];
        // Concurrent records and resets
        for i in 0..10 {
            let acct = accounting.clone();
            let a = agent;
            handles.push(tokio::spawn(async move {
                if i % 2 == 0 {
                    acct.record_usage(a, TokenUsage {
                        prompt_tokens: 1, completion_tokens: 1, total_tokens: 2,
                    }).await;
                } else {
                    acct.reset_usage(a).await;
                }
            }));
        }

        for handle in handles {
            handle.await.unwrap();
        }

        // Should not panic; state may vary but total should be consistent
        let total = accounting.get_total_usage().await;
        assert!(total.total_tokens >= 200, "Total should at least have initial value");
    }

    #[tokio::test]
    async fn test_stats_matches_sum_of_agents() {
        let accounting = TokenAccounting::new();
        let agents: Vec<AgentId> = (0..5).map(|_| AgentId::new()).collect();

        for (i, &agent) in agents.iter().enumerate() {
            let tokens = (i as u32 + 1) * 10;
            accounting.record_usage(agent, TokenUsage {
                prompt_tokens: tokens,
                completion_tokens: tokens * 2,
                total_tokens: tokens * 3,
            }).await;
        }

        let stats = accounting.stats().await;
        assert_eq!(stats.total_agents, 5);
        // sum of prompt: 10+20+30+40+50 = 150
        assert_eq!(stats.total_prompt_tokens, 150);
        // sum of completion: 20+40+60+80+100 = 300
        assert_eq!(stats.total_completion_tokens, 300);
        // sum of total: 30+60+90+120+150 = 450
        assert_eq!(stats.total_tokens, 450);
    }

    #[tokio::test]
    async fn test_multiple_record_same_agent_accumulates_correctly() {
        let accounting = TokenAccounting::new();
        let agent = AgentId::new();

        for _ in 0..100 {
            accounting.record_usage(agent, TokenUsage {
                prompt_tokens: 1, completion_tokens: 2, total_tokens: 3,
            }).await;
        }

        let usage = accounting.get_usage(agent).await.unwrap();
        assert_eq!(usage.prompt_tokens, 100);
        assert_eq!(usage.completion_tokens, 200);
        assert_eq!(usage.total_tokens, 300);

        let total = accounting.get_total_usage().await;
        assert_eq!(total.total_tokens, 300);
    }

    #[tokio::test]
    async fn test_reset_usage_returns_cleanly() {
        let accounting = TokenAccounting::new();
        let agent = AgentId::new();
        accounting.record_usage(agent, TokenUsage {
            prompt_tokens: 1, completion_tokens: 1, total_tokens: 2,
        }).await;
        // reset_usage returns nothing; just ensure no panic
        accounting.reset_usage(agent).await;
        accounting.reset_usage(agent).await; // Double reset
        assert!(accounting.get_usage(agent).await.is_none());
    }

    #[tokio::test]
    async fn test_reset_all_twice_no_panic() {
        let accounting = TokenAccounting::new();
        accounting.reset_all().await;
        accounting.reset_all().await;
        let stats = accounting.stats().await;
        assert_eq!(stats.total_agents, 0);
        assert_eq!(stats.total_tokens, 0);
    }

    #[test]
    fn test_accounting_stats_copy() {
        let stats = AccountingStats {
            total_agents: 1,
            total_prompt_tokens: 2,
            total_completion_tokens: 3,
            total_tokens: 5,
        };
        let copied = stats; // Copy
        let copied2 = stats; // Copy again, original still valid
        assert_eq!(copied.total_agents, copied2.total_agents);
        assert_eq!(stats.total_tokens, 5);
    }

    #[tokio::test]
    async fn test_get_usage_specific_agent_among_many() {
        let accounting = TokenAccounting::new();
        let target = AgentId::new();
        let others: Vec<AgentId> = (0..10).map(|_| AgentId::new()).collect();

        for &other in &others {
            accounting.record_usage(other, TokenUsage {
                prompt_tokens: 99, completion_tokens: 99, total_tokens: 198,
            }).await;
        }
        accounting.record_usage(target, TokenUsage {
            prompt_tokens: 7, completion_tokens: 13, total_tokens: 20,
        }).await;

        let usage = accounting.get_usage(target).await.unwrap();
        assert_eq!(usage.prompt_tokens, 7);
        assert_eq!(usage.completion_tokens, 13);
        assert_eq!(usage.total_tokens, 20);
    }

    #[tokio::test]
    async fn test_total_usage_after_partial_resets() {
        let accounting = TokenAccounting::new();
        let a1 = AgentId::new();
        let a2 = AgentId::new();
        let a3 = AgentId::new();

        accounting.record_usage(a1, TokenUsage { prompt_tokens: 10, completion_tokens: 10, total_tokens: 20 }).await;
        accounting.record_usage(a2, TokenUsage { prompt_tokens: 20, completion_tokens: 20, total_tokens: 40 }).await;
        accounting.record_usage(a3, TokenUsage { prompt_tokens: 30, completion_tokens: 30, total_tokens: 60 }).await;

        // Reset a2
        accounting.reset_usage(a2).await;

        // Total should still reflect all recorded usage (reset_usage doesn't subtract from total)
        let total = accounting.get_total_usage().await;
        assert_eq!(total.total_tokens, 120);

        // But list_agents should only have 2
        let agents = accounting.list_agents().await;
        assert_eq!(agents.len(), 2);
    }

    #[tokio::test]
    async fn test_default_token_usage_is_zero() {
        let usage = TokenUsage::default();
        assert_eq!(usage.prompt_tokens, 0);
        assert_eq!(usage.completion_tokens, 0);
        assert_eq!(usage.total_tokens, 0);
    }

    #[test]
    fn test_accounting_stats_all_fields_zero() {
        let stats = AccountingStats {
            total_agents: 0,
            total_prompt_tokens: 0,
            total_completion_tokens: 0,
            total_tokens: 0,
        };
        assert_eq!(stats.total_agents, 0);
        assert_eq!(stats.total_prompt_tokens, 0);
        assert_eq!(stats.total_completion_tokens, 0);
        assert_eq!(stats.total_tokens, 0);
    }
}
