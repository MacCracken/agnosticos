//! Agent Supervisor
//!
//! Monitors agent health, enforces resource limits, and handles failures.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use tokio::sync::RwLock;
use tokio::time::interval;
use tracing::{debug, error, info, warn};

use agnos_common::{AgentEvent, AgentId, AgentStatus, ResourceUsage, StopReason};

use crate::agent::Agent;
use crate::registry::AgentRegistry;

/// Health check configuration
#[derive(Debug, Clone)]
pub struct HealthCheckConfig {
    pub interval: Duration,
    pub timeout: Duration,
    pub unhealthy_threshold: u32,
    pub healthy_threshold: u32,
}

impl Default for HealthCheckConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(30),
            timeout: Duration::from_secs(5),
            unhealthy_threshold: 3,
            healthy_threshold: 2,
        }
    }
}

/// Agent health status
#[derive(Debug, Clone)]
pub struct AgentHealth {
    pub agent_id: AgentId,
    pub is_healthy: bool,
    pub consecutive_failures: u32,
    pub consecutive_successes: u32,
    pub last_check: Instant,
    pub last_response_time_ms: u64,
    pub resource_usage: ResourceUsage,
}

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

    /// Register an agent for supervision
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
        info!("Registered agent {} for supervision", agent_id);

        Ok(())
    }

    /// Unregister an agent from supervision
    pub async fn unregister_agent(&self, agent_id: AgentId) -> Result<()> {
        self.health_checks.write().await.remove(&agent_id);
        self.running_agents.write().await.remove(&agent_id);
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
            if let Err(e) = self.registry.update_status(agent_id, AgentStatus::Stopping).await {
                warn!("Failed to update agent {} status: {}", agent_id, e);
            }
        }

        info!("All agents shutdown complete");
        Ok(())
    }

    /// Health check monitoring loop
    async fn health_check_loop(&self) {
        let mut interval = interval(self.config.interval);

        loop {
            interval.tick().await;

            let agents: Vec<_> = self.health_checks.read().await.keys().copied().collect();

            for agent_id in agents {
                match self.check_agent_health(agent_id).await {
                    Ok(healthy) => {
                        self.update_health_status(agent_id, healthy).await;
                    }
                    Err(e) => {
                        error!("Health check failed for agent {}: {}", agent_id, e);
                        self.update_health_status(agent_id, false).await;
                    }
                }
            }
        }
    }

    /// Resource monitoring loop
    async fn resource_monitor_loop(&self) {
        let mut interval = interval(Duration::from_secs(10));

        loop {
            interval.tick().await;

            let agents: Vec<_> = self.running_agents.read().await.keys().copied().collect();

            for agent_id in agents {
                if let Err(e) = self.check_resource_limits(agent_id).await {
                    error!("Resource check failed for agent {}: {}", agent_id, e);
                }
            }
        }
    }

    /// Check the health of a specific agent.
    ///
    /// Verifies that the agent's process is alive (via /proc on Linux or
    /// `kill(pid, 0)`) and that it hasn't exceeded its health check timeout.
    async fn check_agent_health(&self, agent_id: AgentId) -> Result<bool> {
        let agent = self.registry.get(agent_id)
            .ok_or_else(|| anyhow::anyhow!("Agent {} not found in registry", agent_id))?;

        // Only check running agents
        if agent.status != AgentStatus::Running {
            return Ok(true);
        }

        // Check if the agent's process is still alive
        if let Some(pid) = agent.pid {
            let alive = Self::is_process_alive(pid);
            if !alive {
                warn!("Agent {} (pid {}) process is no longer alive", agent_id, pid);
                return Ok(false);
            }

            // Check IPC socket existence as a secondary liveness signal
            let socket_path = format!("/run/agnos/agents/{}.sock", agent_id);
            if std::path::Path::new(&socket_path).exists() {
                // Try a non-blocking connect to verify socket is accepting
                match tokio::time::timeout(
                    self.config.timeout,
                    tokio::net::UnixStream::connect(&socket_path),
                )
                .await
                {
                    Ok(Ok(_stream)) => {
                        debug!("Agent {} health check passed (process alive + socket responsive)", agent_id);
                        return Ok(true);
                    }
                    Ok(Err(e)) => {
                        debug!("Agent {} socket connect failed: {} (process alive, socket unresponsive)", agent_id, e);
                        // Process is alive but socket isn't responding — might be starting up
                        return Ok(true);
                    }
                    Err(_) => {
                        warn!("Agent {} health check timed out after {:?}", agent_id, self.config.timeout);
                        return Ok(false);
                    }
                }
            }

            // No socket but process is alive — agent may not use IPC
            debug!("Agent {} health check passed (process alive, no socket)", agent_id);
            return Ok(true);
        }

        // No PID tracked — can't verify, assume healthy
        debug!("Agent {} has no PID tracked, assuming healthy", agent_id);
        Ok(true)
    }

    /// Check if a process is alive using kill(pid, 0).
    fn is_process_alive(pid: u32) -> bool {
        #[cfg(target_os = "linux")]
        {
            // kill(pid, 0) checks if the process exists without sending a signal
            unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = pid;
            true // Can't check on non-Linux
        }
    }

    /// Update health status based on check result
    async fn update_health_status(&self, agent_id: AgentId, healthy: bool) {
        let mut checks = self.health_checks.write().await;
        
        if let Some(health) = checks.get_mut(&agent_id) {
            health.last_check = Instant::now();

            if healthy {
                health.consecutive_successes += 1;
                health.consecutive_failures = 0;

                if health.consecutive_successes >= self.config.healthy_threshold {
                    if !health.is_healthy {
                        info!("Agent {} is now healthy", agent_id);
                        health.is_healthy = true;
                    }
                }
            } else {
                health.consecutive_failures += 1;
                health.consecutive_successes = 0;

                if health.consecutive_failures >= self.config.unhealthy_threshold {
                    if health.is_healthy {
                        warn!("Agent {} is now unhealthy", agent_id);
                        health.is_healthy = false;
                        
                        // Trigger recovery action
                        drop(checks);
                        self.handle_unhealthy_agent(agent_id).await;
                    }
                }
            }
        }
    }

    /// Handle an unhealthy agent with restart logic.
    ///
    /// Attempts to restart the agent with exponential backoff.
    /// After `MAX_RESTART_ATTEMPTS` failures, the agent is marked as permanently failed.
    async fn handle_unhealthy_agent(&self, agent_id: AgentId) {
        const MAX_RESTART_ATTEMPTS: u32 = 5;
        const BASE_BACKOFF_SECS: u64 = 2;

        warn!("Taking recovery action for unhealthy agent {}", agent_id);

        // Get current failure count
        let failure_count = {
            let checks = self.health_checks.read().await;
            checks.get(&agent_id).map_or(0, |h| h.consecutive_failures)
        };

        if failure_count > MAX_RESTART_ATTEMPTS {
            error!(
                "Agent {} has exceeded max restart attempts ({}), marking as permanently failed",
                agent_id, MAX_RESTART_ATTEMPTS
            );
            if let Err(e) = self.registry.update_status(agent_id, AgentStatus::Failed).await {
                error!("Failed to update agent {} status: {}", agent_id, e);
            }
            return;
        }

        // Exponential backoff: 2s, 4s, 8s, 16s, 32s
        let backoff = Duration::from_secs(BASE_BACKOFF_SECS.saturating_pow(failure_count));
        info!(
            "Restarting agent {} (attempt {}/{}) after {:?} backoff",
            agent_id, failure_count, MAX_RESTART_ATTEMPTS, backoff
        );

        // Mark as restarting
        if let Err(e) = self.registry.update_status(agent_id, AgentStatus::Starting).await {
            error!("Failed to update agent {} status for restart: {}", agent_id, e);
            return;
        }

        tokio::time::sleep(backoff).await;

        // Attempt restart via the AgentControl trait if available
        let restart_result = {
            let mut agents = self.running_agents.write().await;
            if let Some(agent) = agents.get_mut(&agent_id) {
                Some(agent.restart().await)
            } else {
                None
            }
        };

        match restart_result {
            Some(Ok(())) => {
                info!("Agent {} restarted successfully", agent_id);
                if let Err(e) = self.registry.update_status(agent_id, AgentStatus::Running).await {
                    error!("Failed to update agent {} status after restart: {}", agent_id, e);
                }
                // Reset health counters on successful restart
                let mut checks = self.health_checks.write().await;
                if let Some(health) = checks.get_mut(&agent_id) {
                    health.is_healthy = true;
                    health.consecutive_failures = 0;
                    health.consecutive_successes = 0;
                }
            }
            Some(Err(e)) => {
                error!("Failed to restart agent {}: {}", agent_id, e);
                if let Err(e) = self.registry.update_status(agent_id, AgentStatus::Failed).await {
                    error!("Failed to update agent {} status: {}", agent_id, e);
                }
            }
            None => {
                // No AgentControl registered — can't restart programmatically
                warn!(
                    "No AgentControl registered for agent {}, marking as failed",
                    agent_id
                );
                if let Err(e) = self.registry.update_status(agent_id, AgentStatus::Failed).await {
                    error!("Failed to update agent {} status: {}", agent_id, e);
                }
            }
        }
    }

    /// Check if an agent is exceeding resource limits
    async fn check_resource_limits(&self, agent_id: AgentId) -> Result<()> {
        let agent = self.registry.get(agent_id)
            .ok_or_else(|| anyhow::anyhow!("Agent {} not found", agent_id))?;

        let config = self.registry.get_config(agent_id)
            .ok_or_else(|| anyhow::anyhow!("Config not found for agent {}", agent_id))?;

        // Check memory limit
        if agent.resource_usage.memory_used > config.resource_limits.max_memory {
            warn!(
                "Agent {} exceeded memory limit: {} > {}",
                agent_id, agent.resource_usage.memory_used, config.resource_limits.max_memory
            );
            
            // TODO: Implement memory limit enforcement
        }

        // Check CPU time limit
        if agent.resource_usage.cpu_time_used > config.resource_limits.max_cpu_time {
            warn!(
                "Agent {} exceeded CPU time limit: {} > {}",
                agent_id, agent.resource_usage.cpu_time_used, config.resource_limits.max_cpu_time
            );
            
            // TODO: Implement CPU limit enforcement
        }

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
}

