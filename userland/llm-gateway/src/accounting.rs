//! Token accounting for multi-agent LLM access

use std::collections::HashMap;

use agnos_common::{AgentId, TokenUsage};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
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
        let entry = agent_usage
            .entry(agent_id)
            .or_insert_with(TokenUsage::default);
        entry.prompt_tokens = entry.prompt_tokens.saturating_add(usage.prompt_tokens);
        entry.completion_tokens = entry
            .completion_tokens
            .saturating_add(usage.completion_tokens);
        entry.total_tokens = entry.total_tokens.saturating_add(usage.total_tokens);
        drop(agent_usage);

        // Update total usage
        let mut total = self.total_usage.write().await;
        total.prompt_tokens = total.prompt_tokens.saturating_add(usage.prompt_tokens);
        total.completion_tokens = total
            .completion_tokens
            .saturating_add(usage.completion_tokens);
        total.total_tokens = total.total_tokens.saturating_add(usage.total_tokens);

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

// ---------------------------------------------------------------------------
// Token Budget Pool Management
// ---------------------------------------------------------------------------

/// Summary of a single project's budget allocation and usage.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectBudget {
    pub name: String,
    pub allocated: u64,
    pub used: u64,
    pub remaining: u64,
    pub usage_percent: f64,
}

/// Snapshot of a budget pool's state.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetSummary {
    pub pool_name: String,
    pub total: u64,
    pub used: u64,
    pub period_remaining_seconds: i64,
    pub projects: Vec<ProjectBudget>,
}

/// A shared token budget pool that multiple projects can draw from.
///
/// Each project receives an allocation from the pool's total tokens.
/// Not yet wired into the HTTP API — will be exposed in cross-project integration.
#[allow(dead_code)]
/// Usage is tracked per-project and the pool resets after its configured period.
#[derive(Debug, Clone)]
pub struct BudgetPool {
    pub name: String,
    pub total_tokens: u64,
    pub used_tokens: u64,
    pub allocated: HashMap<String, u64>,
    used_per_project: HashMap<String, u64>,
    pub period_start: DateTime<Utc>,
    pub period_duration: Duration,
}

#[allow(dead_code)]
impl BudgetPool {
    /// Create a new budget pool with the given name, total token budget, and reset period.
    pub fn new(name: &str, total_tokens: u64, period: Duration) -> Self {
        Self {
            name: name.to_string(),
            total_tokens,
            used_tokens: 0,
            allocated: HashMap::new(),
            used_per_project: HashMap::new(),
            period_start: Utc::now(),
            period_duration: period,
        }
    }

    /// Reserve quota for a project. Fails if there are not enough unallocated tokens.
    pub fn allocate(&mut self, project: &str, tokens: u64) -> Result<(), String> {
        let total_allocated: u64 = self.allocated.values().sum();
        let available = self.total_tokens.saturating_sub(total_allocated);
        if tokens > available {
            return Err(format!(
                "Cannot allocate {} tokens for '{}': only {} available",
                tokens, project, available
            ));
        }
        *self.allocated.entry(project.to_string()).or_insert(0) += tokens;
        Ok(())
    }

    /// Consume tokens from a project's allocation. Fails if the project would exceed its quota.
    pub fn consume(&mut self, project: &str, tokens: u64) -> Result<(), String> {
        let allocation = self.allocated.get(project).copied().ok_or_else(|| {
            format!(
                "Project '{}' has no allocation in pool '{}'",
                project, self.name
            )
        })?;
        let used = self.used_per_project.get(project).copied().unwrap_or(0);
        if used + tokens > allocation {
            return Err(format!(
                "Project '{}' would exceed budget: used={}, requesting={}, allocated={}",
                project, used, tokens, allocation
            ));
        }
        *self
            .used_per_project
            .entry(project.to_string())
            .or_insert(0) += tokens;
        self.used_tokens += tokens;
        Ok(())
    }

    /// How many tokens remain in a project's allocation.
    pub fn remaining(&self, project: &str) -> Option<u64> {
        let allocated = self.allocated.get(project)?;
        let used = self.used_per_project.get(project).copied().unwrap_or(0);
        Some(allocated.saturating_sub(used))
    }

    /// Total unallocated tokens plus unused tokens across all projects.
    pub fn total_remaining(&self) -> u64 {
        self.total_tokens.saturating_sub(self.used_tokens)
    }

    /// Usage percentage (0.0–1.0) for a project relative to its allocation.
    pub fn usage_percent(&self, project: &str) -> Option<f64> {
        let allocated = *self.allocated.get(project)?;
        if allocated == 0 {
            return Some(0.0);
        }
        let used = self.used_per_project.get(project).copied().unwrap_or(0);
        Some(used as f64 / allocated as f64)
    }

    /// If the period has elapsed, reset all usage counters and start a new period.
    pub fn reset_if_expired(&mut self) {
        let now = Utc::now();
        let elapsed = now.signed_duration_since(self.period_start);
        if elapsed >= self.period_duration {
            self.used_tokens = 0;
            self.used_per_project.clear();
            self.period_start = now;
        }
    }

    /// Redistribute unused tokens from under-utilizing projects to over-utilizing ones,
    /// proportional to original allocation sizes.
    pub fn rebalance(&mut self) {
        if self.allocated.is_empty() {
            return;
        }

        // Find total surplus from under-utilizing projects
        let mut surplus: u64 = 0;
        let mut needy_projects: Vec<(String, u64)> = Vec::new();

        for (project, &allocation) in &self.allocated {
            let used = self.used_per_project.get(project).copied().unwrap_or(0);
            if used < allocation {
                // Under-utilizing: reclaim half the unused portion
                let unused = allocation - used;
                let reclaimable = unused / 2;
                surplus += reclaimable;
            } else if used >= allocation {
                // At or over allocation — they could use more
                needy_projects.push((project.clone(), allocation));
            }
        }

        if surplus == 0 || needy_projects.is_empty() {
            return;
        }

        // Distribute surplus proportionally to original allocation
        let total_needy_allocation: u64 = needy_projects.iter().map(|(_, a)| a).sum();
        if total_needy_allocation == 0 {
            return;
        }

        for (project, alloc) in &needy_projects {
            let share = (surplus as f64 * (*alloc as f64 / total_needy_allocation as f64)) as u64;
            if let Some(entry) = self.allocated.get_mut(project) {
                *entry += share;
            }
        }
    }

    /// Create a snapshot summary of the pool.
    pub fn summary(&self) -> BudgetSummary {
        let now = Utc::now();
        let elapsed = now.signed_duration_since(self.period_start);
        let remaining_secs = (self.period_duration - elapsed).num_seconds().max(0);

        let mut projects: Vec<ProjectBudget> = self
            .allocated
            .iter()
            .map(|(name, &allocated)| {
                let used = self.used_per_project.get(name).copied().unwrap_or(0);
                let remaining = allocated.saturating_sub(used);
                let usage_pct = if allocated > 0 {
                    used as f64 / allocated as f64
                } else {
                    0.0
                };
                ProjectBudget {
                    name: name.clone(),
                    allocated,
                    used,
                    remaining,
                    usage_percent: usage_pct,
                }
            })
            .collect();
        projects.sort_by(|a, b| a.name.cmp(&b.name));

        BudgetSummary {
            pool_name: self.name.clone(),
            total: self.total_tokens,
            used: self.used_tokens,
            period_remaining_seconds: remaining_secs,
            projects,
        }
    }
}

/// Manages multiple named budget pools.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct BudgetManager {
    pools: HashMap<String, BudgetPool>,
}

#[allow(dead_code)]
impl BudgetManager {
    /// Create a new empty budget manager.
    pub fn new() -> Self {
        Self {
            pools: HashMap::new(),
        }
    }

    /// Create a new named pool. Returns an error if the pool already exists.
    pub fn create_pool(
        &mut self,
        name: &str,
        total_tokens: u64,
        period: Duration,
    ) -> Result<(), String> {
        if self.pools.contains_key(name) {
            return Err(format!("Pool '{}' already exists", name));
        }
        self.pools.insert(
            name.to_string(),
            BudgetPool::new(name, total_tokens, period),
        );
        Ok(())
    }

    /// Get an immutable reference to a pool.
    pub fn get_pool(&self, name: &str) -> Option<&BudgetPool> {
        self.pools.get(name)
    }

    /// Get a mutable reference to a pool.
    pub fn get_pool_mut(&mut self, name: &str) -> Option<&mut BudgetPool> {
        self.pools.get_mut(name)
    }

    /// Delete a pool by name, returning it if it existed.
    pub fn delete_pool(&mut self, name: &str) -> Option<BudgetPool> {
        self.pools.remove(name)
    }

    /// Get references to all pools.
    pub fn all_pools(&self) -> Vec<&BudgetPool> {
        self.pools.values().collect()
    }
}

impl Default for BudgetManager {
    fn default() -> Self {
        Self::new()
    }
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

        accounting
            .record_usage(
                agent1,
                TokenUsage {
                    prompt_tokens: 10,
                    completion_tokens: 20,
                    total_tokens: 30,
                },
            )
            .await;
        accounting
            .record_usage(
                agent2,
                TokenUsage {
                    prompt_tokens: 15,
                    completion_tokens: 25,
                    total_tokens: 40,
                },
            )
            .await;

        let agents = accounting.list_agents().await;
        assert_eq!(agents.len(), 2);
    }

    #[tokio::test]
    async fn test_multiple_agents() {
        let accounting = TokenAccounting::new();

        let agent1 = AgentId::new();
        let agent2 = AgentId::new();

        accounting
            .record_usage(
                agent1,
                TokenUsage {
                    prompt_tokens: 100,
                    completion_tokens: 200,
                    total_tokens: 300,
                },
            )
            .await;
        accounting
            .record_usage(
                agent2,
                TokenUsage {
                    prompt_tokens: 50,
                    completion_tokens: 100,
                    total_tokens: 150,
                },
            )
            .await;

        let total = accounting.get_total_usage().await;
        assert_eq!(total.total_tokens, 450);
    }

    #[tokio::test]
    async fn test_stats() {
        let accounting = TokenAccounting::new();
        let agent1 = AgentId::new();
        let agent2 = AgentId::new();

        accounting
            .record_usage(
                agent1,
                TokenUsage {
                    prompt_tokens: 10,
                    completion_tokens: 20,
                    total_tokens: 30,
                },
            )
            .await;
        accounting
            .record_usage(
                agent2,
                TokenUsage {
                    prompt_tokens: 5,
                    completion_tokens: 15,
                    total_tokens: 20,
                },
            )
            .await;

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

        accounting
            .record_usage(
                agent,
                TokenUsage {
                    prompt_tokens: 10,
                    completion_tokens: 20,
                    total_tokens: 30,
                },
            )
            .await;
        accounting
            .record_usage(
                agent,
                TokenUsage {
                    prompt_tokens: 5,
                    completion_tokens: 10,
                    total_tokens: 15,
                },
            )
            .await;

        let usage = accounting.get_usage(agent).await.unwrap();
        assert_eq!(usage.prompt_tokens, 15);
        assert_eq!(usage.completion_tokens, 30);
        assert_eq!(usage.total_tokens, 45);
    }

    #[tokio::test]
    async fn test_zero_token_usage() {
        let accounting = TokenAccounting::new();
        let agent = AgentId::new();

        accounting
            .record_usage(
                agent,
                TokenUsage {
                    prompt_tokens: 0,
                    completion_tokens: 0,
                    total_tokens: 0,
                },
            )
            .await;

        let usage = accounting.get_usage(agent).await;
        assert!(usage.is_some(), "Zero-token usage should still be recorded");
        let u = usage.unwrap();
        assert_eq!(u.total_tokens, 0);
    }

    #[tokio::test]
    async fn test_reset_usage_does_not_affect_total() {
        let accounting = TokenAccounting::new();
        let agent = AgentId::new();

        accounting
            .record_usage(
                agent,
                TokenUsage {
                    prompt_tokens: 50,
                    completion_tokens: 50,
                    total_tokens: 100,
                },
            )
            .await;

        accounting.reset_usage(agent).await;

        // Agent is gone, but total should still reflect the recorded usage
        let total = accounting.get_total_usage().await;
        assert_eq!(total.total_tokens, 100);
    }

    #[tokio::test]
    async fn test_reset_all_clears_total_too() {
        let accounting = TokenAccounting::new();
        let agent = AgentId::new();

        accounting
            .record_usage(
                agent,
                TokenUsage {
                    prompt_tokens: 100,
                    completion_tokens: 200,
                    total_tokens: 300,
                },
            )
            .await;

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
            accounting
                .record_usage(
                    agent,
                    TokenUsage {
                        prompt_tokens: 1,
                        completion_tokens: 1,
                        total_tokens: 2,
                    },
                )
                .await;
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
                acct.record_usage(
                    aid,
                    TokenUsage {
                        prompt_tokens: 1,
                        completion_tokens: 2,
                        total_tokens: 3,
                    },
                )
                .await;
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

        accounting
            .record_usage(
                agent1,
                TokenUsage {
                    prompt_tokens: 1,
                    completion_tokens: 1,
                    total_tokens: 2,
                },
            )
            .await;
        accounting
            .record_usage(
                agent2,
                TokenUsage {
                    prompt_tokens: 1,
                    completion_tokens: 1,
                    total_tokens: 2,
                },
            )
            .await;
        accounting
            .record_usage(
                agent3,
                TokenUsage {
                    prompt_tokens: 1,
                    completion_tokens: 1,
                    total_tokens: 2,
                },
            )
            .await;

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

        accounting
            .record_usage(
                a1,
                TokenUsage {
                    prompt_tokens: 100,
                    completion_tokens: 50,
                    total_tokens: 150,
                },
            )
            .await;
        accounting
            .record_usage(
                a2,
                TokenUsage {
                    prompt_tokens: 200,
                    completion_tokens: 300,
                    total_tokens: 500,
                },
            )
            .await;

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
        accounting
            .record_usage(
                agent,
                TokenUsage {
                    prompt_tokens: u32::MAX / 2,
                    completion_tokens: u32::MAX / 2,
                    total_tokens: u32::MAX - 1,
                },
            )
            .await;
        let usage = accounting.get_usage(agent).await.unwrap();
        assert_eq!(usage.prompt_tokens, u32::MAX / 2);
        assert_eq!(usage.completion_tokens, u32::MAX / 2);
    }

    #[tokio::test]
    async fn test_record_usage_then_reset_then_record_again() {
        let accounting = TokenAccounting::new();
        let agent = AgentId::new();
        accounting
            .record_usage(
                agent,
                TokenUsage {
                    prompt_tokens: 100,
                    completion_tokens: 200,
                    total_tokens: 300,
                },
            )
            .await;
        accounting.reset_usage(agent).await;
        assert!(accounting.get_usage(agent).await.is_none());

        // Record again after reset
        accounting
            .record_usage(
                agent,
                TokenUsage {
                    prompt_tokens: 50,
                    completion_tokens: 50,
                    total_tokens: 100,
                },
            )
            .await;
        let usage = accounting.get_usage(agent).await.unwrap();
        assert_eq!(usage.prompt_tokens, 50);
        assert_eq!(usage.total_tokens, 100);
    }

    #[tokio::test]
    async fn test_reset_all_then_record() {
        let accounting = TokenAccounting::new();
        let a1 = AgentId::new();
        let a2 = AgentId::new();
        accounting
            .record_usage(
                a1,
                TokenUsage {
                    prompt_tokens: 10,
                    completion_tokens: 20,
                    total_tokens: 30,
                },
            )
            .await;
        accounting
            .record_usage(
                a2,
                TokenUsage {
                    prompt_tokens: 40,
                    completion_tokens: 50,
                    total_tokens: 90,
                },
            )
            .await;
        accounting.reset_all().await;

        // Record after reset
        accounting
            .record_usage(
                a1,
                TokenUsage {
                    prompt_tokens: 5,
                    completion_tokens: 5,
                    total_tokens: 10,
                },
            )
            .await;
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
        accounting
            .record_usage(
                agent,
                TokenUsage {
                    prompt_tokens: 1,
                    completion_tokens: 1,
                    total_tokens: 2,
                },
            )
            .await;
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
                acct.record_usage(
                    agent,
                    TokenUsage {
                        prompt_tokens: 10,
                        completion_tokens: 5,
                        total_tokens: 15,
                    },
                )
                .await;
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
        accounting
            .record_usage(
                agent,
                TokenUsage {
                    prompt_tokens: 100,
                    completion_tokens: 100,
                    total_tokens: 200,
                },
            )
            .await;

        let mut handles = vec![];
        // Concurrent records and resets
        for i in 0..10 {
            let acct = accounting.clone();
            let a = agent;
            handles.push(tokio::spawn(async move {
                if i % 2 == 0 {
                    acct.record_usage(
                        a,
                        TokenUsage {
                            prompt_tokens: 1,
                            completion_tokens: 1,
                            total_tokens: 2,
                        },
                    )
                    .await;
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
        assert!(
            total.total_tokens >= 200,
            "Total should at least have initial value"
        );
    }

    #[tokio::test]
    async fn test_stats_matches_sum_of_agents() {
        let accounting = TokenAccounting::new();
        let agents: Vec<AgentId> = (0..5).map(|_| AgentId::new()).collect();

        for (i, &agent) in agents.iter().enumerate() {
            let tokens = (i as u32 + 1) * 10;
            accounting
                .record_usage(
                    agent,
                    TokenUsage {
                        prompt_tokens: tokens,
                        completion_tokens: tokens * 2,
                        total_tokens: tokens * 3,
                    },
                )
                .await;
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
            accounting
                .record_usage(
                    agent,
                    TokenUsage {
                        prompt_tokens: 1,
                        completion_tokens: 2,
                        total_tokens: 3,
                    },
                )
                .await;
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
        accounting
            .record_usage(
                agent,
                TokenUsage {
                    prompt_tokens: 1,
                    completion_tokens: 1,
                    total_tokens: 2,
                },
            )
            .await;
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
            accounting
                .record_usage(
                    other,
                    TokenUsage {
                        prompt_tokens: 99,
                        completion_tokens: 99,
                        total_tokens: 198,
                    },
                )
                .await;
        }
        accounting
            .record_usage(
                target,
                TokenUsage {
                    prompt_tokens: 7,
                    completion_tokens: 13,
                    total_tokens: 20,
                },
            )
            .await;

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

        accounting
            .record_usage(
                a1,
                TokenUsage {
                    prompt_tokens: 10,
                    completion_tokens: 10,
                    total_tokens: 20,
                },
            )
            .await;
        accounting
            .record_usage(
                a2,
                TokenUsage {
                    prompt_tokens: 20,
                    completion_tokens: 20,
                    total_tokens: 40,
                },
            )
            .await;
        accounting
            .record_usage(
                a3,
                TokenUsage {
                    prompt_tokens: 30,
                    completion_tokens: 30,
                    total_tokens: 60,
                },
            )
            .await;

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

    // ------------------------------------------------------------------
    // Budget Pool Tests
    // ------------------------------------------------------------------

    fn hour() -> Duration {
        Duration::hours(1)
    }

    #[test]
    fn test_budget_pool_new() {
        let pool = BudgetPool::new("test", 100_000, hour());
        assert_eq!(pool.name, "test");
        assert_eq!(pool.total_tokens, 100_000);
        assert_eq!(pool.used_tokens, 0);
        assert!(pool.allocated.is_empty());
    }

    #[test]
    fn test_budget_pool_allocate() {
        let mut pool = BudgetPool::new("main", 10_000, hour());
        assert!(pool.allocate("AGNOSTIC", 5_000).is_ok());
        assert!(pool.allocate("SecureYeoman", 3_000).is_ok());
        assert_eq!(pool.allocated["AGNOSTIC"], 5_000);
        assert_eq!(pool.allocated["SecureYeoman"], 3_000);
    }

    #[test]
    fn test_budget_pool_allocate_exceeds() {
        let mut pool = BudgetPool::new("main", 10_000, hour());
        assert!(pool.allocate("AGNOSTIC", 7_000).is_ok());
        // Only 3000 left, requesting 5000
        let result = pool.allocate("SecureYeoman", 5_000);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("only 3000 available"));
    }

    #[test]
    fn test_budget_pool_consume() {
        let mut pool = BudgetPool::new("main", 10_000, hour());
        pool.allocate("AGNOSTIC", 5_000).unwrap();
        assert!(pool.consume("AGNOSTIC", 2_000).is_ok());
        assert_eq!(pool.used_tokens, 2_000);
        assert_eq!(pool.remaining("AGNOSTIC"), Some(3_000));
    }

    #[test]
    fn test_budget_pool_consume_exceeds_allocation() {
        let mut pool = BudgetPool::new("main", 10_000, hour());
        pool.allocate("AGNOSTIC", 1_000).unwrap();
        pool.consume("AGNOSTIC", 800).unwrap();
        let result = pool.consume("AGNOSTIC", 500);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("would exceed budget"));
    }

    #[test]
    fn test_budget_pool_consume_no_allocation() {
        let mut pool = BudgetPool::new("main", 10_000, hour());
        let result = pool.consume("unknown", 100);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("no allocation"));
    }

    #[test]
    fn test_budget_pool_remaining() {
        let mut pool = BudgetPool::new("main", 10_000, hour());
        pool.allocate("A", 5_000).unwrap();
        pool.consume("A", 1_500).unwrap();
        assert_eq!(pool.remaining("A"), Some(3_500));
        assert_eq!(pool.remaining("nonexistent"), None);
    }

    #[test]
    fn test_budget_pool_total_remaining() {
        let mut pool = BudgetPool::new("main", 10_000, hour());
        pool.allocate("A", 5_000).unwrap();
        pool.consume("A", 2_000).unwrap();
        // total_remaining = total - used = 10000 - 2000 = 8000
        assert_eq!(pool.total_remaining(), 8_000);
    }

    #[test]
    fn test_budget_pool_usage_percent() {
        let mut pool = BudgetPool::new("main", 10_000, hour());
        pool.allocate("A", 1_000).unwrap();
        pool.consume("A", 500).unwrap();
        let pct = pool.usage_percent("A").unwrap();
        assert!((pct - 0.5).abs() < 1e-9);
    }

    #[test]
    fn test_budget_pool_usage_percent_zero_allocation() {
        let mut pool = BudgetPool::new("main", 10_000, hour());
        pool.allocated.insert("empty".to_string(), 0);
        assert_eq!(pool.usage_percent("empty"), Some(0.0));
    }

    #[test]
    fn test_budget_pool_usage_percent_nonexistent() {
        let pool = BudgetPool::new("main", 10_000, hour());
        assert_eq!(pool.usage_percent("nope"), None);
    }

    #[test]
    fn test_budget_pool_reset_if_expired() {
        let mut pool = BudgetPool::new("main", 10_000, Duration::seconds(0));
        pool.allocate("A", 5_000).unwrap();
        pool.consume("A", 3_000).unwrap();
        assert_eq!(pool.used_tokens, 3_000);

        // Period of 0 seconds means it's always expired
        pool.reset_if_expired();
        assert_eq!(pool.used_tokens, 0);
        assert_eq!(pool.remaining("A"), Some(5_000)); // allocation preserved, usage cleared
    }

    #[test]
    fn test_budget_pool_rebalance() {
        let mut pool = BudgetPool::new("main", 10_000, hour());
        pool.allocate("under", 4_000).unwrap();
        pool.allocate("over", 4_000).unwrap();
        // "under" uses nothing, "over" uses all
        pool.consume("over", 4_000).unwrap();

        pool.rebalance();
        // "under" had 4000 unused, reclaim half = 2000 surplus
        // "over" is the only needy project, gets all 2000
        assert_eq!(pool.allocated["over"], 6_000);
    }

    #[test]
    fn test_budget_pool_rebalance_no_needy() {
        let mut pool = BudgetPool::new("main", 10_000, hour());
        pool.allocate("A", 5_000).unwrap();
        pool.allocate("B", 5_000).unwrap();
        // Both under-utilizing
        pool.consume("A", 1_000).unwrap();
        pool.consume("B", 1_000).unwrap();
        let a_before = pool.allocated["A"];
        pool.rebalance();
        // No needy projects, nothing should change
        assert_eq!(pool.allocated["A"], a_before);
    }

    #[test]
    fn test_budget_pool_summary() {
        let mut pool = BudgetPool::new("main", 10_000, hour());
        pool.allocate("AGNOSTIC", 6_000).unwrap();
        pool.allocate("SecureYeoman", 3_000).unwrap();
        pool.consume("AGNOSTIC", 2_000).unwrap();

        let summary = pool.summary();
        assert_eq!(summary.pool_name, "main");
        assert_eq!(summary.total, 10_000);
        assert_eq!(summary.used, 2_000);
        assert!(summary.period_remaining_seconds > 0);
        assert_eq!(summary.projects.len(), 2);

        // Projects are sorted by name
        assert_eq!(summary.projects[0].name, "AGNOSTIC");
        assert_eq!(summary.projects[0].used, 2_000);
        assert_eq!(summary.projects[0].remaining, 4_000);
        assert_eq!(summary.projects[1].name, "SecureYeoman");
        assert_eq!(summary.projects[1].used, 0);
    }

    #[test]
    fn test_budget_manager_new() {
        let mgr = BudgetManager::new();
        assert!(mgr.all_pools().is_empty());
    }

    #[test]
    fn test_budget_manager_create_and_get() {
        let mut mgr = BudgetManager::new();
        mgr.create_pool("prod", 50_000, hour()).unwrap();
        assert!(mgr.get_pool("prod").is_some());
        assert_eq!(mgr.get_pool("prod").unwrap().total_tokens, 50_000);
    }

    #[test]
    fn test_budget_manager_duplicate_pool() {
        let mut mgr = BudgetManager::new();
        mgr.create_pool("prod", 50_000, hour()).unwrap();
        let result = mgr.create_pool("prod", 100_000, hour());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("already exists"));
    }

    #[test]
    fn test_budget_manager_delete_pool() {
        let mut mgr = BudgetManager::new();
        mgr.create_pool("temp", 1_000, hour()).unwrap();
        let deleted = mgr.delete_pool("temp");
        assert!(deleted.is_some());
        assert_eq!(deleted.unwrap().name, "temp");
        assert!(mgr.get_pool("temp").is_none());
    }

    #[test]
    fn test_budget_manager_delete_nonexistent() {
        let mut mgr = BudgetManager::new();
        assert!(mgr.delete_pool("nope").is_none());
    }

    #[test]
    fn test_budget_manager_all_pools() {
        let mut mgr = BudgetManager::new();
        mgr.create_pool("a", 1_000, hour()).unwrap();
        mgr.create_pool("b", 2_000, hour()).unwrap();
        mgr.create_pool("c", 3_000, hour()).unwrap();
        assert_eq!(mgr.all_pools().len(), 3);
    }

    #[test]
    fn test_budget_manager_get_pool_mut() {
        let mut mgr = BudgetManager::new();
        mgr.create_pool("mutable", 10_000, hour()).unwrap();
        {
            let pool = mgr.get_pool_mut("mutable").unwrap();
            pool.allocate("proj", 5_000).unwrap();
            pool.consume("proj", 1_000).unwrap();
        }
        assert_eq!(mgr.get_pool("mutable").unwrap().used_tokens, 1_000);
    }

    #[test]
    fn test_budget_manager_default() {
        let mgr = BudgetManager::default();
        assert!(mgr.all_pools().is_empty());
    }

    #[test]
    fn test_budget_pool_multiple_consumes() {
        let mut pool = BudgetPool::new("main", 10_000, hour());
        pool.allocate("A", 5_000).unwrap();
        pool.consume("A", 1_000).unwrap();
        pool.consume("A", 1_000).unwrap();
        pool.consume("A", 1_000).unwrap();
        assert_eq!(pool.remaining("A"), Some(2_000));
        assert_eq!(pool.used_tokens, 3_000);
    }

    #[test]
    fn test_budget_pool_allocate_incremental() {
        let mut pool = BudgetPool::new("main", 10_000, hour());
        pool.allocate("A", 2_000).unwrap();
        pool.allocate("A", 1_000).unwrap(); // adds to existing
        assert_eq!(pool.allocated["A"], 3_000);
    }

    #[test]
    fn test_project_budget_serialize() {
        let pb = ProjectBudget {
            name: "test".to_string(),
            allocated: 1000,
            used: 500,
            remaining: 500,
            usage_percent: 0.5,
        };
        let json = serde_json::to_string(&pb).unwrap();
        assert!(json.contains("\"name\":\"test\""));
        assert!(json.contains("\"allocated\":1000"));
    }

    #[test]
    fn test_budget_summary_serialize() {
        let summary = BudgetSummary {
            pool_name: "pool".to_string(),
            total: 10000,
            used: 5000,
            period_remaining_seconds: 3600,
            projects: vec![],
        };
        let json = serde_json::to_string(&summary).unwrap();
        assert!(json.contains("\"pool_name\":\"pool\""));
    }

    #[tokio::test]
    async fn test_record_usage_saturates_instead_of_overflow() {
        let accounting = TokenAccounting::new();
        let agent_id = AgentId::new();

        // Record usage near u32::MAX
        let usage = TokenUsage {
            prompt_tokens: u32::MAX - 10,
            completion_tokens: u32::MAX - 10,
            total_tokens: u32::MAX - 10,
        };
        accounting.record_usage(agent_id, usage).await;

        // Record more — should saturate at u32::MAX, not wrap to 0
        let usage2 = TokenUsage {
            prompt_tokens: 100,
            completion_tokens: 100,
            total_tokens: 100,
        };
        accounting.record_usage(agent_id, usage2).await;

        let result = accounting.get_usage(agent_id).await.unwrap();
        assert_eq!(result.prompt_tokens, u32::MAX);
        assert_eq!(result.completion_tokens, u32::MAX);
        assert_eq!(result.total_tokens, u32::MAX);
    }
}
