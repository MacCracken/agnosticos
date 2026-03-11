//! Recovery and restart logic for unhealthy agents.

use std::time::Duration;

use agnos_common::{AgentId, AgentStatus};
use anyhow::Result;
use tracing::{error, info, warn};

use agnos_sys::audit as sys_audit;

use super::Supervisor;

impl Supervisor {
    /// Handle an unhealthy agent with restart logic.
    ///
    /// Attempts to restart the agent with exponential backoff.
    /// After `MAX_RESTART_ATTEMPTS` failures, the agent is marked as permanently failed.
    pub(super) async fn handle_unhealthy_agent(&self, agent_id: AgentId) {
        const MAX_RESTART_ATTEMPTS: u32 = 5;

        warn!("Taking recovery action for unhealthy agent {}", agent_id);

        // Emit audit event for unhealthy agent
        if let Err(e) = sys_audit::agnos_audit_log_syscall(
            "agent_unhealthy",
            &format!("agent_id={}", agent_id),
            1,
        ) {
            error!("Audit log failed: {}", e);
        }

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
            if let Err(e) = self
                .registry
                .update_status(agent_id, AgentStatus::Failed)
                .await
            {
                error!("Failed to update agent {} status: {}", agent_id, e);
            }
            return;
        }

        let backoff = Self::calculate_restart_backoff(failure_count);
        info!(
            "Restarting agent {} (attempt {}/{}) after {:?} backoff",
            agent_id, failure_count, MAX_RESTART_ATTEMPTS, backoff
        );

        // Mark as restarting
        if let Err(e) = self
            .registry
            .update_status(agent_id, AgentStatus::Starting)
            .await
        {
            error!(
                "Failed to update agent {} status for restart: {}",
                agent_id, e
            );
            return;
        }

        tokio::time::sleep(backoff).await;

        match self.attempt_restart(agent_id).await {
            Ok(true) => {
                info!("Agent {} restarted successfully", agent_id);
                if let Err(e) = self
                    .registry
                    .update_status(agent_id, AgentStatus::Running)
                    .await
                {
                    error!(
                        "Failed to update agent {} status after restart: {}",
                        agent_id, e
                    );
                }
                // Reset health counters on successful restart
                let mut checks = self.health_checks.write().await;
                if let Some(health) = checks.get_mut(&agent_id) {
                    health.is_healthy = true;
                    health.consecutive_failures = 0;
                    health.consecutive_successes = 0;
                }
            }
            Ok(false) => {
                // No AgentControl registered — can't restart programmatically
                warn!(
                    "No AgentControl registered for agent {}, marking as failed",
                    agent_id
                );
                if let Err(e) = self
                    .registry
                    .update_status(agent_id, AgentStatus::Failed)
                    .await
                {
                    error!("Failed to update agent {} status: {}", agent_id, e);
                }
            }
            Err(e) => {
                error!("Failed to restart agent {}: {}", agent_id, e);
                if let Err(e) = self
                    .registry
                    .update_status(agent_id, AgentStatus::Failed)
                    .await
                {
                    error!("Failed to update agent {} status: {}", agent_id, e);
                }
            }
        }
    }

    /// Calculate exponential backoff duration for restart attempts.
    ///
    /// Backoff: 2^failures seconds, capped at 300s (5 min).
    pub(super) fn calculate_restart_backoff(consecutive_failures: u32) -> Duration {
        const BASE_BACKOFF_SECS: u64 = 2;
        const MAX_BACKOFF_SECS: u64 = 300;
        Duration::from_secs(
            BASE_BACKOFF_SECS
                .saturating_pow(consecutive_failures)
                .min(MAX_BACKOFF_SECS),
        )
    }

    /// Attempt to restart the agent via its AgentControl trait implementation.
    ///
    /// Returns `Ok(true)` on successful restart, `Ok(false)` if no AgentControl
    /// is registered for the agent, or `Err` if the restart itself failed.
    pub(super) async fn attempt_restart(&self, agent_id: AgentId) -> Result<bool> {
        let mut agents = self.running_agents.write().await;
        if let Some(agent) = agents.get_mut(&agent_id) {
            agent.restart().await?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}
