//! Resource monitoring loop and limit enforcement.

use std::time::{Duration, Instant};

use agnos_common::{AgentId, ResourceUsage};
use anyhow::Result;
use tokio::time::interval;
use tracing::{debug, error, info, warn};

use agnos_sys::audit as sys_audit;

use super::cgroup::CgroupController;
use super::proc_utils::{read_proc_cpu_time_us, read_proc_memory};
use super::resource_quota::ResourceQuota;
use super::Supervisor;

impl Supervisor {
    /// Resource monitoring loop
    pub(super) async fn resource_monitor_loop(&self) {
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

    /// Check if an agent is exceeding resource limits using quota thresholds.
    ///
    /// On Linux with cgroups v2, memory limits are enforced by the kernel OOM
    /// killer automatically (memory.max).  This function reads the actual usage
    /// from the cgroup counters and updates the registry.  It then delegates to
    /// `check_memory_limits()` and `check_cpu_limits()` for threshold enforcement.
    ///
    /// If cgroups are unavailable, we fall back to `/proc/{pid}/` reads.
    pub(super) async fn check_resource_limits(&self, agent_id: AgentId) -> Result<()> {
        let agent = self
            .registry
            .get(agent_id)
            .ok_or_else(|| anyhow::anyhow!("Agent {} not found", agent_id))?;

        let (mem_used, cpu_used_us) = if let Some(cg) = CgroupController::open(agent_id) {
            (cg.memory_current(), cg.cpu_usage_usec())
        } else if let Some(pid) = agent.pid {
            // Fallback: read from /proc
            let mem = read_proc_memory(pid);
            let cpu = read_proc_cpu_time_us(pid);
            (mem, cpu)
        } else {
            (0, 0)
        };

        // Convert CPU microseconds to milliseconds for ResourceUsage
        let cpu_used_ms = cpu_used_us / 1000;

        // Update registry with real usage
        let usage = ResourceUsage {
            memory_used: mem_used,
            cpu_time_used: cpu_used_ms,
            ..ResourceUsage::default()
        };
        if let Err(e) = self.registry.update_resource_usage(agent_id, usage).await {
            debug!(agent_id = %agent_id, error = %e, "Failed to update resource usage in registry");
        }

        // Get the quota for this agent (fall back to defaults if missing)
        let quota = {
            let quotas = self.quotas.read().await;
            quotas.get(&agent_id).cloned().unwrap_or_default()
        };

        // Check memory thresholds
        self.check_memory_limits(agent_id, mem_used, &quota).await;

        // Check CPU rate and total time thresholds
        self.check_cpu_limits(agent_id, cpu_used_us, cpu_used_ms, &quota)
            .await;

        Ok(())
    }

    /// Check memory usage against quota thresholds and take action.
    ///
    /// - **memory_kill_pct** (default 95%): SIGKILL the agent + audit event
    /// - **memory_warn_pct** (default 80%): emit warning + audit event
    pub(super) async fn check_memory_limits(
        &self,
        agent_id: AgentId,
        memory_current: u64,
        quota: &ResourceQuota,
    ) {
        if quota.memory_limit == 0 {
            return;
        }

        let mem_pct = (memory_current as f64 / quota.memory_limit as f64) * 100.0;

        if mem_pct >= quota.memory_kill_pct {
            error!(
                "Agent {} EXCEEDED memory kill threshold ({:.1}% >= {:.1}%): {} / {} bytes — sending SIGKILL",
                agent_id, mem_pct, quota.memory_kill_pct, memory_current, quota.memory_limit
            );
            if let Err(e) = sys_audit::agnos_audit_log_syscall(
                "agent_memory_kill",
                &format!(
                    "agent_id={} memory_used={} memory_limit={} pct={:.1} threshold={:.1}",
                    agent_id, memory_current, quota.memory_limit, mem_pct, quota.memory_kill_pct
                ),
                1,
            ) {
                error!("Audit log failed: {}", e);
            }
            self.signal_agent(agent_id, libc::SIGKILL).await;
        } else if mem_pct >= quota.memory_warn_pct {
            warn!(
                "Agent {} approaching memory limit ({:.1}% >= {:.1}%): {} / {} bytes",
                agent_id, mem_pct, quota.memory_warn_pct, memory_current, quota.memory_limit
            );
            if let Err(e) = sys_audit::agnos_audit_log_syscall(
                "agent_memory_warning",
                &format!(
                    "agent_id={} memory_used={} memory_limit={} pct={:.1} threshold={:.1}",
                    agent_id, memory_current, quota.memory_limit, mem_pct, quota.memory_warn_pct
                ),
                0,
            ) {
                error!("Audit log failed: {}", e);
            }
        }
    }

    /// Check CPU usage rate and total CPU time against quota thresholds.
    ///
    /// - **cpu_throttle_pct** (default 90%): emit CPU throttle warning + audit event
    /// - **cpu_time_limit**: SIGKILL the agent if total CPU time exceeded + audit event
    pub(super) async fn check_cpu_limits(
        &self,
        agent_id: AgentId,
        cpu_usage_us: u64,
        cpu_used_ms: u64,
        quota: &ResourceQuota,
    ) {
        // --- CPU usage rate enforcement ---
        // Calculate CPU usage rate by comparing with previous reading.
        // Rate = (delta_usage_usec / delta_time_usec) * 100 → percentage of one core.
        let now = Instant::now();
        let prev_reading = {
            let readings = self.last_cpu_readings.read().await;
            readings.get(&agent_id).copied()
        };

        if let Some((prev_time, prev_usec)) = prev_reading {
            let elapsed = now.duration_since(prev_time);
            let elapsed_us = elapsed.as_micros() as u64;
            if elapsed_us > 0 && cpu_usage_us >= prev_usec {
                let delta_cpu_us = cpu_usage_us - prev_usec;
                let cpu_rate_pct = (delta_cpu_us as f64 / elapsed_us as f64) * 100.0;

                if cpu_rate_pct >= quota.cpu_throttle_pct {
                    warn!(
                        "Agent {} CPU usage rate {:.1}% >= throttle threshold {:.1}%",
                        agent_id, cpu_rate_pct, quota.cpu_throttle_pct
                    );
                    if let Err(e) = sys_audit::agnos_audit_log_syscall(
                        "agent_cpu_throttle_warning",
                        &format!(
                            "agent_id={} cpu_rate_pct={:.1} threshold={:.1}",
                            agent_id, cpu_rate_pct, quota.cpu_throttle_pct
                        ),
                        0,
                    ) {
                        error!("Audit log failed: {}", e);
                    }
                }
            }
        }

        // Store current reading for next interval
        self.last_cpu_readings
            .write()
            .await
            .insert(agent_id, (now, cpu_usage_us));

        // --- CPU total time enforcement ---
        if quota.cpu_time_limit > 0 && cpu_used_ms > quota.cpu_time_limit {
            error!(
                "Agent {} EXCEEDED CPU time limit: {} > {} ms — sending SIGKILL",
                agent_id, cpu_used_ms, quota.cpu_time_limit
            );
            if let Err(e) = sys_audit::agnos_audit_log_syscall(
                "agent_cpu_time_kill",
                &format!(
                    "agent_id={} cpu_used_ms={} cpu_limit_ms={}",
                    agent_id, cpu_used_ms, quota.cpu_time_limit
                ),
                1,
            ) {
                error!("Audit log failed: {}", e);
            }
            self.signal_agent(agent_id, libc::SIGKILL).await;
        }
    }

    /// Send a signal to the agent's process.
    pub(super) async fn signal_agent(&self, agent_id: AgentId, signal: i32) {
        if let Some(agent) = self.registry.get(agent_id) {
            if let Some(pid) = agent.pid {
                #[cfg(target_os = "linux")]
                {
                    let ret = unsafe { libc::kill(pid as libc::pid_t, signal) };
                    if ret == 0 {
                        info!("Sent signal {} to agent {} (pid {})", signal, agent_id, pid);
                    } else {
                        error!(
                            "Failed to send signal {} to agent {} (pid {})",
                            signal, agent_id, pid
                        );
                    }
                }
                #[cfg(not(target_os = "linux"))]
                {
                    let _ = (pid, signal);
                    warn!("Signal sending not supported on this platform");
                }
            }
        }
    }
}
