//! Agent Supervisor
//!
//! Monitors agent health, enforces resource limits, and handles failures.
//! Resource enforcement uses cgroups v2 on Linux for hard memory/CPU limits.

pub mod output_capture;
pub mod circuit_breaker;
pub mod resource_quota;

mod cgroup;
mod health_check;
mod resource_monitor;
mod recovery;
mod proc_utils;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use agnos_common::{AgentId, AgentStatus, ResourceUsage, StopReason};
use agnos_sys::audit as sys_audit;

use crate::registry::AgentRegistry;

use self::cgroup::CgroupController;

// Re-export all public types
pub use self::output_capture::{OutputCapture, OutputLine, OutputStream};
pub use self::circuit_breaker::{CircuitBreaker, CircuitBreakerConfig, CircuitState};
pub use self::resource_quota::{AgentHealth, HealthCheckConfig, ResourceQuota};

/// Supervisor for monitoring and managing agents.
///
/// All mutable state is wrapped in `Arc<RwLock<...>>` so that clones share
/// the same state — critical for background tasks like health_check_loop.
#[derive(Clone)]
pub struct Supervisor {
    registry: Arc<AgentRegistry>,
    health_checks: Arc<RwLock<HashMap<AgentId, AgentHealth>>>,
    config: HealthCheckConfig,
    running_agents: Arc<RwLock<HashMap<AgentId, Box<dyn AgentControl>>>>,
    /// Tracks which agents have active cgroup controllers
    cgroups: Arc<RwLock<HashMap<AgentId, ()>>>,
    /// Per-agent resource quotas (configurable thresholds for enforcement)
    quotas: Arc<RwLock<HashMap<AgentId, ResourceQuota>>>,
    /// Previous CPU usage readings for rate calculation (agent_id → (timestamp, usage_usec))
    last_cpu_readings: Arc<RwLock<HashMap<AgentId, (Instant, u64)>>>,
}

/// Trait for controlling agent processes
#[async_trait::async_trait]
pub trait AgentControl: Send + Sync {
    async fn check_health(&self) -> Result<bool>;
    async fn get_resource_usage(&self) -> Result<ResourceUsage>;
    async fn stop(&mut self, reason: StopReason) -> Result<()>;
    async fn restart(&mut self) -> Result<()>;
}

impl Supervisor {
    /// Create a new supervisor
    pub fn new(registry: Arc<AgentRegistry>) -> Self {
        Self {
            registry,
            health_checks: Arc::new(RwLock::new(HashMap::new())),
            config: HealthCheckConfig::default(),
            running_agents: Arc::new(RwLock::new(HashMap::new())),
            cgroups: Arc::new(RwLock::new(HashMap::new())),
            quotas: Arc::new(RwLock::new(HashMap::new())),
            last_cpu_readings: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Start the supervisor
    pub async fn start(&self) -> Result<()> {
        info!("Starting agent supervisor...");

        // Start health check loop
        let supervisor_clone = self.clone();
        tokio::spawn(async move {
            supervisor_clone.health_check_loop().await;
        });

        // Start resource monitoring loop
        let supervisor_clone = self.clone();
        tokio::spawn(async move {
            supervisor_clone.resource_monitor_loop().await;
        });

        info!("Agent supervisor started");
        Ok(())
    }

    /// Register an agent for supervision.
    ///
    /// If the agent has a PID and resource limits configured, a cgroup v2
    /// controller is created and the process is placed inside it.
    pub async fn register_agent(&self, agent_id: AgentId) -> Result<()> {
        let health = AgentHealth {
            agent_id,
            is_healthy: true,
            consecutive_failures: 0,
            consecutive_successes: 0,
            last_check: Instant::now(),
            last_response_time_ms: 0,
            resource_usage: ResourceUsage::default(),
        };

        self.health_checks.write().await.insert(agent_id, health);

        // Set up resource quota from agent config (if available in registry)
        if let Some(config) = self.registry.get_config(agent_id) {
            let quota = ResourceQuota::from_limits(
                config.resource_limits.max_memory,
                config.resource_limits.max_cpu_time,
            );
            self.quotas.write().await.insert(agent_id, quota);
        } else {
            // No config available — use default quota (no limits enforced)
            self.quotas
                .write()
                .await
                .insert(agent_id, ResourceQuota::default());
        }

        // Attempt to set up cgroups enforcement for this agent
        if let Err(e) = self.setup_cgroup(agent_id).await {
            // cgroups are best-effort — log but don't fail registration
            warn!(
                "Could not set up cgroup for agent {}: {} (resource enforcement unavailable)",
                agent_id, e
            );
        }

        info!("Registered agent {} for supervision", agent_id);
        Ok(())
    }

    /// Create a cgroup for the agent and apply resource limits from its config.
    async fn setup_cgroup(&self, agent_id: AgentId) -> Result<()> {
        let agent = self
            .registry
            .get(agent_id)
            .ok_or_else(|| anyhow::anyhow!("Agent {} not in registry", agent_id))?;
        let config = self
            .registry
            .get_config(agent_id)
            .ok_or_else(|| anyhow::anyhow!("Config not found for agent {}", agent_id))?;

        let max_memory = config.resource_limits.max_memory;
        let max_cpu_time = config.resource_limits.max_cpu_time;
        let pid = agent.pid;

        // CGroup filesystem ops are blocking — run on a blocking thread with timeout
        let cgroup_result = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            tokio::task::spawn_blocking(move || -> Result<()> {
                let cg = CgroupController::new(agent_id)?;

                // Apply memory limit
                if max_memory > 0 {
                    cg.set_memory_limit(max_memory)?;
                }

                // Apply CPU limit
                if max_cpu_time > 0 {
                    let period_us: u64 = 100_000; // 100 ms
                    let quota_us = period_us; // 1 core equivalent by default
                    cg.set_cpu_limit(quota_us, period_us)?;
                }

                // Place the agent's process in the cgroup
                if let Some(pid) = pid {
                    cg.add_pid(pid)?;
                }
                Ok(())
            }),
        )
        .await
        .map_err(|_| anyhow::anyhow!("cgroup setup timed out for agent {}", agent_id))?
        .map_err(|e| anyhow::anyhow!("cgroup task failed: {}", e))?;

        cgroup_result?;

        self.cgroups.write().await.insert(agent_id, ());
        Ok(())
    }

    /// Unregister an agent from supervision and clean up its cgroup,
    /// network namespace, and encrypted storage.
    pub async fn unregister_agent(&self, agent_id: AgentId) -> Result<()> {
        self.health_checks.write().await.remove(&agent_id);
        self.running_agents.write().await.remove(&agent_id);
        self.quotas.write().await.remove(&agent_id);
        self.last_cpu_readings.write().await.remove(&agent_id);

        // Clean up cgroup
        if self.cgroups.write().await.remove(&agent_id).is_some() {
            if let Some(cg) = CgroupController::open(agent_id) {
                if let Err(e) = cg.destroy() {
                    debug!("Could not destroy cgroup for agent {}: {}", agent_id, e);
                }
            }
        }

        // Clean up network namespace (if one was created for this agent)
        let ns_name = format!("agnos-agent-{}", agent_id);
        let handle = agnos_sys::netns::NetNamespaceHandle {
            name: ns_name.clone(),
            veth_host: String::new(),
            veth_agent: String::new(),
            netns_path: format!("/var/run/netns/{}", ns_name),
        };
        // No exists() check — let destroy report errors to avoid TOCTOU
        if let Err(e) = agnos_sys::netns::destroy_agent_netns(&handle) {
            debug!("Could not destroy netns for agent {}: {}", agent_id, e);
        }

        // Clean up LUKS encrypted volume (if one was created)
        let luks_name = format!("agnos-agent-{}", agent_id);
        // No exists() check — let teardown report errors to avoid TOCTOU
        if let Err(e) = agnos_sys::luks::teardown_agent_volume(&luks_name) {
            debug!("Could not teardown LUKS for agent {}: {}", agent_id, e);
        }

        // Emit audit event for agent unregistration
        if let Err(e) = sys_audit::agnos_audit_log_syscall(
            "agent_unregistered",
            &format!("agent_id={}", agent_id),
            0,
        ) {
            error!("Audit log failed: {}", e);
        }

        info!("Unregistered agent {} from supervision", agent_id);
        Ok(())
    }

    /// Shutdown all supervised agents
    pub async fn shutdown_all(&self) -> Result<()> {
        info!("Shutting down all supervised agents...");

        let agents: Vec<_> = self.running_agents.write().await.keys().copied().collect();

        for agent_id in agents {
            info!("Stopping agent {}", agent_id);
            // Send stop signal via registry
            if let Err(e) = self
                .registry
                .update_status(agent_id, AgentStatus::Stopping)
                .await
            {
                warn!("Failed to update agent {} status: {}", agent_id, e);
            }
        }

        info!("All agents shutdown complete");
        Ok(())
    }

    /// Get health status for an agent
    pub async fn get_health(&self, agent_id: AgentId) -> Option<AgentHealth> {
        self.health_checks.read().await.get(&agent_id).cloned()
    }

    /// Get all health statuses
    pub async fn get_all_health(&self) -> Vec<AgentHealth> {
        self.health_checks.read().await.values().cloned().collect()
    }

    /// Set the resource quota for a specific agent.
    ///
    /// This allows runtime tuning of the warning/kill thresholds without
    /// re-registering the agent.
    pub async fn set_quota(&self, agent_id: AgentId, quota: ResourceQuota) {
        self.quotas.write().await.insert(agent_id, quota);
    }

    /// Get the resource quota for a specific agent.
    pub async fn get_quota(&self, agent_id: AgentId) -> Option<ResourceQuota> {
        self.quotas.read().await.get(&agent_id).cloned()
    }
}
