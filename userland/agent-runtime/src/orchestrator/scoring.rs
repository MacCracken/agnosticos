//! Agent scoring — load-aware task assignment and capability matching.

use agnos_common::AgentConfig;
use anyhow::Result;
use tracing::info;
use uuid::Uuid;

use agnos_common::{AgentId, Message, MessageType};

use super::types::{Task, TaskRequirements};
use super::Orchestrator;

use std::collections::{HashMap, HashSet};

impl Orchestrator {
    /// Auto-assign a task to the most suitable agent using load-aware scoring.
    pub(crate) async fn auto_assign_task(&self, task: &Task) -> Result<()> {
        let available = self
            .registry
            .list_by_status(agnos_common::AgentStatus::Running);

        if available.is_empty() {
            tracing::warn!("No available agents to execute task {}", task.id);
            return Err(anyhow::anyhow!("No available agents"));
        }

        // Score each agent and pick the best
        let mut best_agent = &available[0];
        let mut best_score = f64::NEG_INFINITY;

        // Count tasks per agent for fair-share
        let state = self.state.read().await;
        let mut task_counts: HashMap<AgentId, usize> = HashMap::new();
        for t in state.running_tasks.values() {
            for agent_id in &t.target_agents {
                *task_counts.entry(*agent_id).or_insert(0) += 1;
            }
        }
        drop(state);

        for agent in &available {
            let config = self.registry.get_config(agent.id);
            let score = Self::score_agent(
                agent,
                config.as_ref(),
                &task.requirements,
                *task_counts.get(&agent.id).unwrap_or(&0),
            );
            if score > best_score {
                best_score = score;
                best_agent = agent;
            }
        }

        info!(
            "Auto-assigned task {} to agent {} ({}) with score {:.2}",
            task.id, best_agent.name, best_agent.id, best_score
        );

        let message = Message {
            id: Uuid::new_v4().to_string(),
            source: "orchestrator".to_string(),
            target: best_agent.name.clone(),
            message_type: MessageType::Command,
            payload: task.payload.clone(),
            timestamp: chrono::Utc::now(),
        };

        self.message_bus
            .send(message)
            .await
            .map_err(|_| anyhow::anyhow!("Failed to send message"))?;

        Ok(())
    }

    /// Convert a [`Permission`] variant to a lowercase static string,
    /// avoiding `format!("{:?}")` allocations on the hot scoring path.
    pub(crate) fn permission_to_str(p: agnos_common::Permission) -> &'static str {
        match p {
            agnos_common::Permission::FileRead => "fileread",
            agnos_common::Permission::FileWrite => "filewrite",
            agnos_common::Permission::NetworkAccess => "networkaccess",
            agnos_common::Permission::ProcessSpawn => "processspawn",
            agnos_common::Permission::LlmInference => "llminference",
            agnos_common::Permission::AuditRead => "auditread",
        }
    }

    /// Score an agent for a given task's requirements.
    ///
    /// Weights:
    /// - Memory headroom:  40%
    /// - CPU headroom:     30%
    /// - Capability match: 20%
    /// - Affinity bonus:   10%
    ///
    /// Fair-share: agents with fewer running tasks get a bonus.
    pub fn score_agent(
        agent: &crate::agent::AgentHandle,
        config: Option<&AgentConfig>,
        requirements: &TaskRequirements,
        running_task_count: usize,
    ) -> f64 {
        let mut score = 0.0;

        // --- Memory headroom (40%) ---
        let max_memory = config
            .map(|c| c.resource_limits.max_memory)
            .unwrap_or(1024 * 1024 * 1024); // 1GB default
        let used_memory = agent.resource_usage.memory_used;
        let available_memory = max_memory.saturating_sub(used_memory);

        if requirements.min_memory > 0 {
            if available_memory >= requirements.min_memory {
                // Ratio of available to max, capped at 1.0
                let ratio = (available_memory as f64) / (max_memory as f64);
                score += 0.4 * ratio;
            }
            // else: 0 points for memory — agent can't satisfy the requirement
        } else {
            // No memory requirement — full points based on headroom
            let ratio = (available_memory as f64) / (max_memory.max(1) as f64);
            score += 0.4 * ratio;
        }

        // --- CPU headroom (30%) ---
        let max_cpu_time = config
            .map(|c| c.resource_limits.max_cpu_time)
            .unwrap_or(3_600_000);
        let used_cpu = agent.resource_usage.cpu_time_used;
        let available_cpu = max_cpu_time.saturating_sub(used_cpu);
        let cpu_ratio = (available_cpu as f64) / (max_cpu_time.max(1) as f64);
        score += 0.3 * cpu_ratio;

        // --- Capability match (20%) ---
        if let Some(config) = config {
            if !requirements.required_capabilities.is_empty() {
                let agent_caps: HashSet<&'static str> = config
                    .permissions
                    .iter()
                    .map(|p| Self::permission_to_str(*p))
                    .collect();

                let matched = requirements
                    .required_capabilities
                    .iter()
                    .filter(|cap| agent_caps.contains(cap.to_lowercase().as_str()))
                    .count();

                let ratio = if requirements.required_capabilities.is_empty() {
                    1.0
                } else {
                    (matched as f64) / (requirements.required_capabilities.len() as f64)
                };
                score += 0.2 * ratio;
            } else {
                score += 0.2; // No requirements = full match
            }
        } else {
            score += 0.1; // No config = partial match
        }

        // --- Affinity bonus (10%) ---
        if let Some(ref preferred) = requirements.preferred_agent {
            if agent.name == *preferred {
                score += 0.1;
            }
        }

        // --- Fair-share bonus ---
        // Agents with fewer running tasks get a small bonus (up to 0.05)
        let fair_share_penalty = (running_task_count as f64) * 0.01;
        score -= fair_share_penalty.min(0.1);

        score
    }
}
