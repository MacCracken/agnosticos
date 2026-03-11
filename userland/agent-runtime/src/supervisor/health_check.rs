//! Health check monitoring for supervised agents.

use agnos_common::{AgentId, AgentStatus};
use anyhow::Result;
use tokio::time::interval;
use tracing::{debug, error, info, warn};

use super::Supervisor;

impl Supervisor {
    /// Health check monitoring loop
    pub(super) async fn health_check_loop(&self) {
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

    /// Check the health of a specific agent.
    ///
    /// Verifies that the agent's process is alive (via /proc on Linux or
    /// `kill(pid, 0)`) and that it hasn't exceeded its health check timeout.
    pub(super) async fn check_agent_health(&self, agent_id: AgentId) -> Result<bool> {
        let agent = self
            .registry
            .get(agent_id)
            .ok_or_else(|| anyhow::anyhow!("Agent {} not found in registry", agent_id))?;

        // Only check running agents
        if agent.status != AgentStatus::Running {
            return Ok(true);
        }

        // Check if the agent's process is still alive
        if let Some(pid) = agent.pid {
            let alive = Self::is_process_alive(pid);
            if !alive {
                warn!(
                    "Agent {} (pid {}) process is no longer alive",
                    agent_id, pid
                );
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
                        debug!(
                            "Agent {} health check passed (process alive + socket responsive)",
                            agent_id
                        );
                        return Ok(true);
                    }
                    Ok(Err(e)) => {
                        debug!("Agent {} socket connect failed: {} (process alive, socket unresponsive)", agent_id, e);
                        // Process is alive but socket isn't responding — might be starting up
                        return Ok(true);
                    }
                    Err(_) => {
                        warn!(
                            "Agent {} health check timed out after {:?}",
                            agent_id, self.config.timeout
                        );
                        return Ok(false);
                    }
                }
            }

            // No socket but process is alive — agent may not use IPC
            debug!(
                "Agent {} health check passed (process alive, no socket)",
                agent_id
            );
            return Ok(true);
        }

        // No PID tracked — can't verify, assume healthy
        debug!("Agent {} has no PID tracked, assuming healthy", agent_id);
        Ok(true)
    }

    /// Check if a process is alive using kill(pid, 0).
    pub(super) fn is_process_alive(pid: u32) -> bool {
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
    pub(super) async fn update_health_status(&self, agent_id: AgentId, healthy: bool) {
        let mut checks = self.health_checks.write().await;

        if let Some(health) = checks.get_mut(&agent_id) {
            health.last_check = std::time::Instant::now();

            if healthy {
                health.consecutive_successes += 1;
                health.consecutive_failures = 0;

                if health.consecutive_successes >= self.config.healthy_threshold
                    && !health.is_healthy
                {
                    info!("Agent {} is now healthy", agent_id);
                    health.is_healthy = true;
                }
            } else {
                health.consecutive_failures += 1;
                health.consecutive_successes = 0;

                if health.consecutive_failures >= self.config.unhealthy_threshold
                    && health.is_healthy
                {
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
