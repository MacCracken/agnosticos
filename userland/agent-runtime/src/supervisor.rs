//! Agent Supervisor
//!
//! Monitors agent health, enforces resource limits, and handles failures.
//! Resource enforcement uses cgroups v2 on Linux for hard memory/CPU limits.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use tokio::sync::RwLock;
use tokio::time::interval;
use tracing::{debug, error, info, warn};

use agnos_common::{AgentEvent, AgentId, AgentStatus, ResourceUsage, StopReason};

use crate::agent::Agent;
use crate::registry::AgentRegistry;

/// Base path for AGNOS cgroups v2 hierarchy
const CGROUP_BASE: &str = "/sys/fs/cgroup/agnos";

/// Manages cgroups v2 resource enforcement for a single agent
#[derive(Debug)]
struct CgroupController {
    path: PathBuf,
    agent_id: AgentId,
}

impl CgroupController {
    /// Create (or ensure existence of) a cgroup for the given agent.
    fn new(agent_id: AgentId) -> Result<Self> {
        let path = PathBuf::from(CGROUP_BASE).join(agent_id.to_string());
        std::fs::create_dir_all(&path)
            .map_err(|e| anyhow::anyhow!("Failed to create cgroup dir {}: {}", path.display(), e))?;
        Ok(Self { path, agent_id })
    }

    /// Try to open an existing cgroup without creating it.
    fn open(agent_id: AgentId) -> Option<Self> {
        let path = PathBuf::from(CGROUP_BASE).join(agent_id.to_string());
        if path.is_dir() {
            Some(Self { path, agent_id })
        } else {
            None
        }
    }

    /// Set the hard memory limit (memory.max) in bytes.  0 means "max" (unlimited).
    fn set_memory_limit(&self, bytes: u64) -> Result<()> {
        let value = if bytes == 0 {
            "max".to_string()
        } else {
            bytes.to_string()
        };
        std::fs::write(self.path.join("memory.max"), &value)
            .map_err(|e| anyhow::anyhow!("cgroup memory.max write: {}", e))?;
        debug!("Agent {} cgroup memory.max set to {}", self.agent_id, value);
        Ok(())
    }

    /// Set the CPU bandwidth limit (cpu.max).
    /// `quota_us` is the allowed microseconds per `period_us` (default 100 000 µs = 100 ms).
    /// Setting quota_us to 0 means "max" (unlimited).
    fn set_cpu_limit(&self, quota_us: u64, period_us: u64) -> Result<()> {
        let value = if quota_us == 0 {
            format!("max {}", period_us)
        } else {
            format!("{} {}", quota_us, period_us)
        };
        std::fs::write(self.path.join("cpu.max"), &value)
            .map_err(|e| anyhow::anyhow!("cgroup cpu.max write: {}", e))?;
        debug!("Agent {} cgroup cpu.max set to {}", self.agent_id, value);
        Ok(())
    }

    /// Add a process to this cgroup.
    fn add_pid(&self, pid: u32) -> Result<()> {
        std::fs::write(self.path.join("cgroup.procs"), pid.to_string())
            .map_err(|e| anyhow::anyhow!("cgroup add pid {}: {}", pid, e))?;
        info!("Agent {} added pid {} to cgroup", self.agent_id, pid);
        Ok(())
    }

    /// Read current memory usage from memory.current (bytes).
    fn memory_current(&self) -> u64 {
        std::fs::read_to_string(self.path.join("memory.current"))
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0)
    }

    /// Read the configured memory limit from memory.max (bytes).
    fn memory_max(&self) -> Option<u64> {
        let s = std::fs::read_to_string(self.path.join("memory.max")).ok()?;
        let trimmed = s.trim();
        if trimmed == "max" {
            None // unlimited
        } else {
            trimmed.parse().ok()
        }
    }

    /// Read CPU usage from cpu.stat (usage_usec field).
    fn cpu_usage_usec(&self) -> u64 {
        std::fs::read_to_string(self.path.join("cpu.stat"))
            .ok()
            .and_then(|contents| {
                for line in contents.lines() {
                    if let Some(val) = line.strip_prefix("usage_usec ") {
                        return val.trim().parse().ok();
                    }
                }
                None
            })
            .unwrap_or(0)
    }

    /// Read the set of PIDs in this cgroup.
    fn pids(&self) -> Vec<u32> {
        std::fs::read_to_string(self.path.join("cgroup.procs"))
            .ok()
            .map(|s| {
                s.lines()
                    .filter_map(|l| l.trim().parse().ok())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Remove the cgroup directory (must be empty of processes first).
    fn destroy(&self) -> Result<()> {
        if self.path.is_dir() {
            std::fs::remove_dir(&self.path)
                .map_err(|e| anyhow::anyhow!("cgroup destroy {}: {}", self.path.display(), e))?;
            debug!("Destroyed cgroup for agent {}", self.agent_id);
        }
        Ok(())
    }
}

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
    /// Tracks which agents have active cgroup controllers
    cgroups: Arc<RwLock<HashMap<AgentId, ()>>>,
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

        // Attempt to set up cgroups enforcement for this agent
        if let Err(e) = self.setup_cgroup(agent_id).await {
            // cgroups are best-effort — log but don't fail registration
            warn!("Could not set up cgroup for agent {}: {} (resource enforcement unavailable)", agent_id, e);
        }

        info!("Registered agent {} for supervision", agent_id);
        Ok(())
    }

    /// Create a cgroup for the agent and apply resource limits from its config.
    async fn setup_cgroup(&self, agent_id: AgentId) -> Result<()> {
        let agent = self.registry.get(agent_id)
            .ok_or_else(|| anyhow::anyhow!("Agent {} not in registry", agent_id))?;
        let config = self.registry.get_config(agent_id)
            .ok_or_else(|| anyhow::anyhow!("Config not found for agent {}", agent_id))?;

        let cg = CgroupController::new(agent_id)?;

        // Apply memory limit
        if config.resource_limits.max_memory > 0 {
            cg.set_memory_limit(config.resource_limits.max_memory)?;
        }

        // Apply CPU limit: convert max_cpu_time (ms total) to a bandwidth quota.
        // We use 100ms period and grant proportional quota based on limit.
        // A max_cpu_time of 0 means unlimited.
        if config.resource_limits.max_cpu_time > 0 {
            // Allow full CPU for the period — the hard limit is enforced by the
            // kernel OOM / cpu throttle.  We set quota = period (one full core).
            let period_us: u64 = 100_000; // 100 ms
            let quota_us = period_us; // 1 core equivalent by default
            cg.set_cpu_limit(quota_us, period_us)?;
        }

        // Place the agent's process in the cgroup
        if let Some(pid) = agent.pid {
            cg.add_pid(pid)?;
        }

        self.cgroups.write().await.insert(agent_id, ());
        Ok(())
    }

    /// Unregister an agent from supervision and clean up its cgroup.
    pub async fn unregister_agent(&self, agent_id: AgentId) -> Result<()> {
        self.health_checks.write().await.remove(&agent_id);
        self.running_agents.write().await.remove(&agent_id);

        // Clean up cgroup
        if self.cgroups.write().await.remove(&agent_id).is_some() {
            if let Some(cg) = CgroupController::open(agent_id) {
                if let Err(e) = cg.destroy() {
                    debug!("Could not destroy cgroup for agent {}: {}", agent_id, e);
                }
            }
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

    /// Check if an agent is exceeding resource limits.
    ///
    /// On Linux with cgroups v2, memory limits are enforced by the kernel OOM
    /// killer automatically (memory.max).  This function reads the actual usage
    /// from the cgroup counters and updates the registry.  If limits are
    /// exceeded beyond a soft threshold (90%), we issue a SIGTERM to give the
    /// agent a chance to clean up.  If cgroups are unavailable, we fall back
    /// to `/proc/{pid}/` reads.
    async fn check_resource_limits(&self, agent_id: AgentId) -> Result<()> {
        let agent = self.registry.get(agent_id)
            .ok_or_else(|| anyhow::anyhow!("Agent {} not found", agent_id))?;
        let config = self.registry.get_config(agent_id)
            .ok_or_else(|| anyhow::anyhow!("Config not found for agent {}", agent_id))?;

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
        let _ = self.registry.update_resource_usage(agent_id, usage).await;

        // Enforce memory: warn at 90%, SIGTERM at limit
        if config.resource_limits.max_memory > 0 {
            let limit = config.resource_limits.max_memory;
            if mem_used > limit {
                warn!(
                    "Agent {} EXCEEDED memory limit: {} > {} bytes — sending SIGTERM",
                    agent_id, mem_used, limit
                );
                self.signal_agent(agent_id, libc::SIGTERM).await;
            } else if mem_used > limit * 9 / 10 {
                warn!(
                    "Agent {} approaching memory limit: {} / {} bytes (>90%)",
                    agent_id, mem_used, limit
                );
            }
        }

        // Enforce CPU time: warn at 90%, SIGTERM at limit
        if config.resource_limits.max_cpu_time > 0 {
            let limit = config.resource_limits.max_cpu_time;
            if cpu_used_ms > limit {
                warn!(
                    "Agent {} EXCEEDED CPU time limit: {} > {} ms — sending SIGTERM",
                    agent_id, cpu_used_ms, limit
                );
                self.signal_agent(agent_id, libc::SIGTERM).await;
            } else if cpu_used_ms > limit * 9 / 10 {
                warn!(
                    "Agent {} approaching CPU time limit: {} / {} ms (>90%)",
                    agent_id, cpu_used_ms, limit
                );
            }
        }

        Ok(())
    }

    /// Send a signal to the agent's process.
    async fn signal_agent(&self, agent_id: AgentId, signal: i32) {
        if let Some(agent) = self.registry.get(agent_id) {
            if let Some(pid) = agent.pid {
                #[cfg(target_os = "linux")]
                {
                    let ret = unsafe { libc::kill(pid as libc::pid_t, signal) };
                    if ret == 0 {
                        info!("Sent signal {} to agent {} (pid {})", signal, agent_id, pid);
                    } else {
                        error!("Failed to send signal {} to agent {} (pid {})", signal, agent_id, pid);
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

    /// Get health status for an agent
    pub async fn get_health(&self, agent_id: AgentId) -> Option<AgentHealth> {
        self.health_checks.read().await.get(&agent_id).cloned()
    }

    /// Get all health statuses
    pub async fn get_all_health(&self) -> Vec<AgentHealth> {
        self.health_checks.read().await.values().cloned().collect()
    }
}

/// Read VmRSS (resident set size) from `/proc/{pid}/status` in bytes.
fn read_proc_memory(pid: u32) -> u64 {
    let path = format!("/proc/{}/status", pid);
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|contents| {
            for line in contents.lines() {
                if let Some(val) = line.strip_prefix("VmRSS:") {
                    // Value is in kB, e.g. "   12345 kB"
                    let kb: u64 = val.trim().split_whitespace().next()?.parse().ok()?;
                    return Some(kb * 1024);
                }
            }
            None
        })
        .unwrap_or(0)
}

/// Read CPU time (utime + stime) from `/proc/{pid}/stat` and convert to microseconds.
fn read_proc_cpu_time_us(pid: u32) -> u64 {
    let path = format!("/proc/{}/stat", pid);
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|contents| {
            // Fields in /proc/pid/stat are space-separated.
            // Field 14 = utime (user ticks), field 15 = stime (kernel ticks).
            // The comm field (2) can contain spaces/parens, so find the closing ')' first.
            let after_comm = contents.find(')')?.checked_add(2)?;
            let fields: Vec<&str> = contents[after_comm..].split_whitespace().collect();
            // After comm, fields are 0-indexed from field 3 of the original format
            // utime = field 14 → index 11, stime = field 15 → index 12
            let utime: u64 = fields.get(11)?.parse().ok()?;
            let stime: u64 = fields.get(12)?.parse().ok()?;
            let ticks = utime + stime;
            // Convert clock ticks to microseconds (typically 100 ticks/sec on Linux)
            let ticks_per_sec = unsafe { libc::sysconf(libc::_SC_CLK_TCK) } as u64;
            if ticks_per_sec > 0 {
                Some(ticks * 1_000_000 / ticks_per_sec)
            } else {
                Some(ticks * 10_000) // fallback: assume 100 Hz
            }
        })
        .unwrap_or(0)
}

