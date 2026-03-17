//! Agent scoring — load-aware task assignment, capability matching, and GPU awareness.
//!
//! # Scoring Weights
//!
//! When a task requires GPU (`gpu_required = true`):
//! - Memory headroom: 35%
//! - CPU headroom:    25%
//! - GPU headroom:    15%
//! - Capability match: 15%
//! - Affinity bonus:   10%
//!
//! When no GPU is required (default):
//! - Memory headroom: 40%
//! - CPU headroom:    30%
//! - Capability match: 20%
//! - Affinity bonus:   10%
//!
//! # GPU Scoring
//!
//! The GPU score evaluates available GPUs against the task's requirements:
//! - `min_gpu_memory`: minimum VRAM the GPU must have available
//! - `required_compute_capability`: minimum compute capability string (e.g., "8.0")
//!
//! The best-matching GPU's VRAM headroom ratio (available/total) is used as the score.
//! If no GPU can satisfy the requirements, the GPU component scores 0.0.

use agnos_common::AgentConfig;
use anyhow::Result;
use tracing::{debug, info};
use uuid::Uuid;

use agnos_common::{AgentId, Message, MessageType};

use super::types::{Task, TaskRequirements};
use super::Orchestrator;

use std::collections::{HashMap, HashSet};
use std::sync::atomic::Ordering;

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

        // Snapshot GPU state for scoring (if resource manager attached)
        let gpu_snapshot = if let Some(ref rm) = self.resource_manager {
            rm.list_gpus().await
        } else {
            Vec::new()
        };

        // Count tasks per agent for fair-share
        let state = self.state.read().await;
        let mut task_counts: HashMap<AgentId, usize> = HashMap::new();
        for t in state.running_tasks.values() {
            for agent_id in &t.target_agents {
                *task_counts.entry(*agent_id).or_insert(0) += 1;
            }
        }
        drop(state);

        // Score each agent and pick the best
        let mut best_agent = &available[0];
        let mut best_score = f64::NEG_INFINITY;

        for agent in &available {
            let config = self.registry.get_config(agent.id);
            let score = Self::score_agent(
                agent,
                config.as_ref(),
                &task.requirements,
                *task_counts.get(&agent.id).unwrap_or(&0),
                &gpu_snapshot,
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

        // Allocate GPU if required and resource manager is available
        if task.requirements.gpu_required {
            if let Some(ref rm) = self.resource_manager {
                match rm
                    .allocate_gpu(best_agent.id, task.requirements.min_gpu_memory)
                    .await
                {
                    Ok(gpu_ids) => {
                        debug!(
                            "Allocated GPU(s) {:?} for task {} on agent {}",
                            gpu_ids, task.id, best_agent.id
                        );
                    }
                    Err(e) => {
                        tracing::warn!(
                            "GPU allocation failed for task {} on agent {}: {}",
                            task.id,
                            best_agent.id,
                            e
                        );
                        // Continue anyway — the agent may have its own GPU access
                    }
                }
            }
        }

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
    /// - Memory headroom:  35%
    /// - CPU headroom:     25%
    /// - GPU headroom:     15% (only when task requires GPU; redistributed otherwise)
    /// - Capability match: 15%
    /// - Affinity bonus:   10%
    ///
    /// Fair-share: agents with fewer running tasks get a bonus (up to -0.1).
    pub fn score_agent(
        agent: &crate::agent::AgentHandle,
        config: Option<&AgentConfig>,
        requirements: &TaskRequirements,
        running_task_count: usize,
        gpu_snapshot: &[crate::resource::GpuDevice],
    ) -> f64 {
        let mut score = 0.0;

        // Determine weights based on whether GPU is required.
        // When no GPU is needed, redistribute GPU weight to memory and CPU.
        let (w_mem, w_cpu, w_gpu, w_cap) = if requirements.gpu_required {
            (0.35, 0.25, 0.15, 0.15)
        } else {
            (0.40, 0.30, 0.0, 0.20)
        };

        // --- Memory headroom ---
        let max_memory = config
            .map(|c| c.resource_limits.max_memory)
            .unwrap_or(1024 * 1024 * 1024); // 1GB default
        let used_memory = agent.resource_usage.memory_used;
        let available_memory = max_memory.saturating_sub(used_memory);

        if requirements.min_memory > 0 {
            if available_memory >= requirements.min_memory {
                let ratio = (available_memory as f64) / (max_memory as f64);
                score += w_mem * ratio;
            }
            // else: 0 points for memory — agent can't satisfy the requirement
        } else {
            let ratio = (available_memory as f64) / (max_memory.max(1) as f64);
            score += w_mem * ratio;
        }

        // --- CPU headroom ---
        let max_cpu_time = config
            .map(|c| c.resource_limits.max_cpu_time)
            .unwrap_or(3_600_000);
        let used_cpu = agent.resource_usage.cpu_time_used;
        let available_cpu = max_cpu_time.saturating_sub(used_cpu);
        let cpu_ratio = (available_cpu as f64) / (max_cpu_time.max(1) as f64);
        score += w_cpu * cpu_ratio;

        // --- GPU headroom (only when GPU required) ---
        if requirements.gpu_required && w_gpu > 0.0 {
            score += w_gpu * Self::score_gpu(gpu_snapshot, requirements);
        }

        // --- Capability match ---
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
                score += w_cap * ratio;
            } else {
                score += w_cap; // No requirements = full match
            }
        } else {
            score += w_cap * 0.5; // No config = partial match
        }

        // --- Affinity bonus (10%) ---
        if let Some(ref preferred) = requirements.preferred_agent {
            if agent.name == *preferred {
                score += 0.1;
            }
        }

        // --- Fair-share penalty ---
        let fair_share_penalty = (running_task_count as f64) * 0.01;
        score -= fair_share_penalty.min(0.1);

        score
    }

    /// Score GPU availability for a task's requirements.
    ///
    /// Returns 0.0–1.0:
    /// - 0.0 if no GPU can satisfy the requirement
    /// - Higher scores for GPUs with more available VRAM headroom
    pub(crate) fn score_gpu(
        gpu_snapshot: &[crate::resource::GpuDevice],
        requirements: &TaskRequirements,
    ) -> f64 {
        if gpu_snapshot.is_empty() {
            return 0.0;
        }

        let min_vram = requirements.min_gpu_memory;
        let mut best_ratio = 0.0_f64;

        for gpu in gpu_snapshot {
            let available = gpu.available_memory.load(Ordering::Relaxed);

            // Check compute capability if required
            if let Some(ref required_cc) = requirements.required_compute_capability {
                match &gpu.compute_capability {
                    Some(cc) if cc >= required_cc => {}
                    _ => continue, // Skip GPU — doesn't meet compute capability
                }
            }

            if min_vram > 0 {
                if available >= min_vram {
                    let ratio = (available as f64) / (gpu.total_memory.max(1) as f64);
                    best_ratio = best_ratio.max(ratio);
                }
            } else {
                // No VRAM requirement — score based on headroom
                let ratio = (available as f64) / (gpu.total_memory.max(1) as f64);
                best_ratio = best_ratio.max(ratio);
            }
        }

        best_ratio
    }
}
