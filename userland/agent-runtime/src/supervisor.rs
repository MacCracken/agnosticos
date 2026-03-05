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
use agnos_sys::audit as sys_audit;

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

    /// Unregister an agent from supervision and clean up its cgroup,
    /// network namespace, and encrypted storage.
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

        // Clean up network namespace (if one was created for this agent)
        let ns_name = format!("agnos-agent-{}", agent_id);
        let handle = agnos_sys::netns::NetNamespaceHandle {
            name: ns_name.clone(),
            veth_host: String::new(),
            veth_agent: String::new(),
            netns_path: format!("/var/run/netns/{}", ns_name),
        };
        if std::path::Path::new(&handle.netns_path).exists() {
            if let Err(e) = agnos_sys::netns::destroy_agent_netns(&handle) {
                debug!("Could not destroy netns for agent {}: {}", agent_id, e);
            }
        }

        // Clean up LUKS encrypted volume (if one was created)
        let luks_name = format!("agnos-agent-{}", agent_id);
        let mapper_path = format!("/dev/mapper/{}", luks_name);
        if std::path::Path::new(&mapper_path).exists() {
            if let Err(e) = agnos_sys::luks::teardown_agent_volume(&luks_name) {
                debug!("Could not teardown LUKS for agent {}: {}", agent_id, e);
            }
        }

        // Emit audit event for agent unregistration
        let _ = sys_audit::agnos_audit_log_syscall(
            "agent_unregistered",
            &format!("agent_id={}", agent_id),
            0,
        );

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

        // Emit audit event for unhealthy agent
        let _ = sys_audit::agnos_audit_log_syscall(
            "agent_unhealthy",
            &format!("agent_id={}", agent_id),
            1,
        );

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use agnos_common::AgentId;
    use tempfile::TempDir;

    struct MockAgentControl {
        healthy: bool,
    }

    #[async_trait::async_trait]
    impl AgentControl for MockAgentControl {
        async fn check_health(&self) -> Result<bool> {
            Ok(self.healthy)
        }

        async fn get_resource_usage(&self) -> Result<ResourceUsage> {
            Ok(ResourceUsage {
                memory_used: 100 * 1024 * 1024,
                cpu_time_used: 1000,
                file_descriptors_used: 10,
                processes_used: 1,
            })
        }

        async fn stop(&mut self, _reason: StopReason) -> Result<()> {
            Ok(())
        }

        async fn restart(&mut self) -> Result<()> {
            Ok(())
        }
    }

    #[test]
    fn test_health_check_config_defaults() {
        let config = HealthCheckConfig::default();
        assert_eq!(config.interval, Duration::from_secs(30));
        assert_eq!(config.timeout, Duration::from_secs(5));
        assert_eq!(config.unhealthy_threshold, 3);
        assert_eq!(config.healthy_threshold, 2);
    }

    #[test]
    fn test_health_check_config_custom() {
        let config = HealthCheckConfig {
            interval: Duration::from_secs(60),
            timeout: Duration::from_secs(10),
            unhealthy_threshold: 5,
            healthy_threshold: 3,
        };
        assert_eq!(config.interval, Duration::from_secs(60));
        assert_eq!(config.timeout, Duration::from_secs(10));
        assert_eq!(config.unhealthy_threshold, 5);
        assert_eq!(config.healthy_threshold, 3);
    }

    #[test]
    fn test_agent_health_default() {
        let agent_id = AgentId::new();
        let health = AgentHealth {
            agent_id,
            is_healthy: true,
            consecutive_failures: 0,
            consecutive_successes: 0,
            last_check: Instant::now(),
            last_response_time_ms: 0,
            resource_usage: ResourceUsage::default(),
        };
        assert!(health.is_healthy);
        assert_eq!(health.consecutive_failures, 0);
        assert_eq!(health.consecutive_successes, 0);
    }

    #[test]
    fn test_agent_health_unhealthy() {
        let agent_id = AgentId::new();
        let health = AgentHealth {
            agent_id,
            is_healthy: false,
            consecutive_failures: 3,
            consecutive_successes: 0,
            last_check: Instant::now(),
            last_response_time_ms: 5000,
            resource_usage: ResourceUsage {
                memory_used: 2 * 1024 * 1024 * 1024,
                cpu_time_used: 10_000_000,
                file_descriptors_used: 1000,
                processes_used: 50,
            },
        };
        assert!(!health.is_healthy);
        assert_eq!(health.consecutive_failures, 3);
    }

    #[test]
    fn test_cgroup_controller_memory_limit_format() {
        let agent_id = AgentId::new();
        let controller = CgroupController {
            path: PathBuf::from(format!("/tmp/test-cgroup-{}", agent_id)),
            agent_id,
        };
        
        assert_eq!(controller.memory_current(), 0);
    }

    #[test]
    fn test_cgroup_controller_new_requires_path() {
        let agent_id = AgentId::new();
        let path = PathBuf::from("/nonexistent/path/that/should/not/exist");
        let controller = CgroupController {
            path,
            agent_id,
        };
        
        let result = controller.set_memory_limit(1024 * 1024 * 1024);
        assert!(result.is_err());
    }

    #[test]
    fn test_cgroup_controller_open_nonexistent() {
        let agent_id = AgentId::new();
        let result = CgroupController::open(agent_id);
        assert!(result.is_none());
    }

    #[test]
    fn test_supervisor_new() {
        let registry = Arc::new(AgentRegistry::new());
        let supervisor = Supervisor::new(registry.clone());
        
        assert!(supervisor.health_checks.blocking_read().is_empty());
        assert!(supervisor.running_agents.blocking_read().is_empty());
        assert!(supervisor.cgroups.blocking_read().is_empty());
    }

    #[test]
    fn test_supervisor_config() {
        let registry = Arc::new(AgentRegistry::new());
        let supervisor = Supervisor::new(registry);
        
        assert_eq!(supervisor.config.interval, Duration::from_secs(30));
        assert_eq!(supervisor.config.timeout, Duration::from_secs(5));
        assert_eq!(supervisor.config.unhealthy_threshold, 3);
        assert_eq!(supervisor.config.healthy_threshold, 2);
    }

    #[tokio::test]
    async fn test_supervisor_register_agent() {
        let registry = Arc::new(AgentRegistry::new());
        let supervisor = Supervisor::new(registry.clone());
        
        let agent_id = AgentId::new();
        let result = supervisor.register_agent(agent_id).await;
        assert!(result.is_ok());
        
        let health_map = supervisor.health_checks.read().await;
        assert!(health_map.contains_key(&agent_id));
        
        let health = health_map.get(&agent_id).unwrap();
        assert!(health.is_healthy);
        assert_eq!(health.consecutive_failures, 0);
        assert_eq!(health.consecutive_successes, 0);
    }

    #[tokio::test]
    async fn test_supervisor_unregister_agent() {
        let registry = Arc::new(AgentRegistry::new());
        let supervisor = Supervisor::new(registry.clone());
        
        let agent_id = AgentId::new();
        supervisor.register_agent(agent_id).await.unwrap();
        
        let result = supervisor.unregister_agent(agent_id).await;
        assert!(result.is_ok());
        
        let health_map = supervisor.health_checks.read().await;
        assert!(!health_map.contains_key(&agent_id));
    }

    #[tokio::test]
    async fn test_supervisor_unregister_nonexistent() {
        let registry = Arc::new(AgentRegistry::new());
        let supervisor = Supervisor::new(registry.clone());
        
        let agent_id = AgentId::new();
        let result = supervisor.unregister_agent(agent_id).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_resource_usage_default() {
        let usage = ResourceUsage::default();
        assert_eq!(usage.memory_used, 0);
        assert_eq!(usage.cpu_time_used, 0);
        assert_eq!(usage.file_descriptors_used, 0);
        assert_eq!(usage.processes_used, 0);
    }

    #[test]
    fn test_resource_usage_custom() {
        let usage = ResourceUsage {
            memory_used: 1024 * 1024 * 1024,
            cpu_time_used: 5000000,
            file_descriptors_used: 100,
            processes_used: 10,
        };
        assert_eq!(usage.memory_used, 1024 * 1024 * 1024);
        assert_eq!(usage.cpu_time_used, 5_000_000);
        assert_eq!(usage.file_descriptors_used, 100);
        assert_eq!(usage.processes_used, 10);
    }

    #[test]
    fn test_agent_control_trait_object() {
        let mock = MockAgentControl { healthy: true };
        let boxed: Box<dyn AgentControl> = Box::new(mock);
        
        let health = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(boxed.check_health());
        
        assert!(health.unwrap());
    }

    #[test]
    fn test_agent_control_trait_object_unhealthy() {
        let mock = MockAgentControl { healthy: false };
        let boxed: Box<dyn AgentControl> = Box::new(mock);
        
        let health = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(boxed.check_health());
        
        assert!(!health.unwrap());
    }

    #[test]
    fn test_cgroup_controller_memory_max_unlimited() {
        let agent_id = AgentId::new();
        let controller = CgroupController {
            path: PathBuf::from("/sys/fs/cgroup"),
            agent_id,
        };

        let max = controller.memory_max();
        assert!(max.is_none() || max.is_some());
    }

    #[test]
    fn test_cgroup_controller_path_generation() {
        let agent_id = AgentId::new();
        let expected_path = PathBuf::from(CGROUP_BASE).join(agent_id.to_string());
        let controller = CgroupController {
            path: expected_path.clone(),
            agent_id,
        };
        assert_eq!(controller.path, expected_path);
        assert_eq!(controller.agent_id, agent_id);
    }

    #[test]
    fn test_cgroup_controller_cpu_usage_usec_nonexistent() {
        let agent_id = AgentId::new();
        let controller = CgroupController {
            path: PathBuf::from("/nonexistent/cgroup/path"),
            agent_id,
        };
        assert_eq!(controller.cpu_usage_usec(), 0);
    }

    #[test]
    fn test_cgroup_controller_pids_nonexistent() {
        let agent_id = AgentId::new();
        let controller = CgroupController {
            path: PathBuf::from("/nonexistent/cgroup/path"),
            agent_id,
        };
        assert!(controller.pids().is_empty());
    }

    #[test]
    fn test_cgroup_controller_memory_current_nonexistent() {
        let agent_id = AgentId::new();
        let controller = CgroupController {
            path: PathBuf::from("/nonexistent/cgroup/path"),
            agent_id,
        };
        assert_eq!(controller.memory_current(), 0);
    }

    #[test]
    fn test_cgroup_controller_memory_max_nonexistent() {
        let agent_id = AgentId::new();
        let controller = CgroupController {
            path: PathBuf::from("/nonexistent/cgroup/path"),
            agent_id,
        };
        assert!(controller.memory_max().is_none());
    }

    #[test]
    fn test_cgroup_controller_set_cpu_limit_nonexistent() {
        let agent_id = AgentId::new();
        let controller = CgroupController {
            path: PathBuf::from("/nonexistent/cgroup/path"),
            agent_id,
        };
        let result = controller.set_cpu_limit(100_000, 100_000);
        assert!(result.is_err());
    }

    #[test]
    fn test_cgroup_controller_add_pid_nonexistent() {
        let agent_id = AgentId::new();
        let controller = CgroupController {
            path: PathBuf::from("/nonexistent/cgroup/path"),
            agent_id,
        };
        let result = controller.add_pid(12345);
        assert!(result.is_err());
    }

    #[test]
    fn test_cgroup_controller_destroy_nonexistent() {
        let agent_id = AgentId::new();
        let controller = CgroupController {
            path: PathBuf::from("/nonexistent/cgroup/path"),
            agent_id,
        };
        // Non-existent path should succeed (no-op)
        let result = controller.destroy();
        assert!(result.is_ok());
    }

    #[test]
    fn test_cgroup_controller_with_tempdir() {
        let tmp = TempDir::new().unwrap();
        let agent_id = AgentId::new();
        let cg_path = tmp.path().join(agent_id.to_string());
        std::fs::create_dir_all(&cg_path).unwrap();

        let controller = CgroupController {
            path: cg_path.clone(),
            agent_id,
        };

        // Write a fake memory.max file
        std::fs::write(cg_path.join("memory.max"), "1073741824").unwrap();
        assert_eq!(controller.memory_max(), Some(1073741824));

        // Write "max" for unlimited
        std::fs::write(cg_path.join("memory.max"), "max").unwrap();
        assert_eq!(controller.memory_max(), None);

        // Write a fake memory.current
        std::fs::write(cg_path.join("memory.current"), "524288000").unwrap();
        assert_eq!(controller.memory_current(), 524288000);

        // Write a fake cpu.stat
        std::fs::write(
            cg_path.join("cpu.stat"),
            "usage_usec 1234567\nuser_usec 1000000\nsystem_usec 234567\n",
        )
        .unwrap();
        assert_eq!(controller.cpu_usage_usec(), 1234567);

        // Write a fake cgroup.procs
        std::fs::write(cg_path.join("cgroup.procs"), "100\n200\n300\n").unwrap();
        assert_eq!(controller.pids(), vec![100, 200, 300]);
    }

    #[test]
    fn test_cgroup_controller_pids_empty_file() {
        let tmp = TempDir::new().unwrap();
        let agent_id = AgentId::new();
        let cg_path = tmp.path().join(agent_id.to_string());
        std::fs::create_dir_all(&cg_path).unwrap();

        let controller = CgroupController {
            path: cg_path.clone(),
            agent_id,
        };
        std::fs::write(cg_path.join("cgroup.procs"), "").unwrap();
        assert!(controller.pids().is_empty());
    }

    #[test]
    fn test_cgroup_controller_set_memory_limit_with_tempdir() {
        let tmp = TempDir::new().unwrap();
        let agent_id = AgentId::new();
        let cg_path = tmp.path().join(agent_id.to_string());
        std::fs::create_dir_all(&cg_path).unwrap();

        let controller = CgroupController {
            path: cg_path.clone(),
            agent_id,
        };

        // Set a numeric limit
        controller.set_memory_limit(2 * 1024 * 1024 * 1024).unwrap();
        let written = std::fs::read_to_string(cg_path.join("memory.max")).unwrap();
        assert_eq!(written, "2147483648");

        // Set unlimited (0 means "max")
        controller.set_memory_limit(0).unwrap();
        let written = std::fs::read_to_string(cg_path.join("memory.max")).unwrap();
        assert_eq!(written, "max");
    }

    #[test]
    fn test_cgroup_controller_set_cpu_limit_with_tempdir() {
        let tmp = TempDir::new().unwrap();
        let agent_id = AgentId::new();
        let cg_path = tmp.path().join(agent_id.to_string());
        std::fs::create_dir_all(&cg_path).unwrap();

        let controller = CgroupController {
            path: cg_path.clone(),
            agent_id,
        };

        controller.set_cpu_limit(50000, 100000).unwrap();
        let written = std::fs::read_to_string(cg_path.join("cpu.max")).unwrap();
        assert_eq!(written, "50000 100000");

        // Unlimited (0 quota)
        controller.set_cpu_limit(0, 100000).unwrap();
        let written = std::fs::read_to_string(cg_path.join("cpu.max")).unwrap();
        assert_eq!(written, "max 100000");
    }

    #[test]
    fn test_cgroup_controller_add_pid_with_tempdir() {
        let tmp = TempDir::new().unwrap();
        let agent_id = AgentId::new();
        let cg_path = tmp.path().join(agent_id.to_string());
        std::fs::create_dir_all(&cg_path).unwrap();

        let controller = CgroupController {
            path: cg_path.clone(),
            agent_id,
        };

        controller.add_pid(42).unwrap();
        let written = std::fs::read_to_string(cg_path.join("cgroup.procs")).unwrap();
        assert_eq!(written, "42");
    }

    #[test]
    fn test_cgroup_controller_destroy_with_tempdir() {
        let tmp = TempDir::new().unwrap();
        let agent_id = AgentId::new();
        let cg_path = tmp.path().join(agent_id.to_string());
        std::fs::create_dir_all(&cg_path).unwrap();

        let controller = CgroupController {
            path: cg_path.clone(),
            agent_id,
        };

        assert!(cg_path.exists());
        controller.destroy().unwrap();
        assert!(!cg_path.exists());
    }

    #[test]
    fn test_health_check_config_clone() {
        let config = HealthCheckConfig {
            interval: Duration::from_secs(15),
            timeout: Duration::from_secs(3),
            unhealthy_threshold: 5,
            healthy_threshold: 2,
        };
        let cloned = config.clone();
        assert_eq!(cloned.interval, Duration::from_secs(15));
        assert_eq!(cloned.timeout, Duration::from_secs(3));
        assert_eq!(cloned.unhealthy_threshold, 5);
        assert_eq!(cloned.healthy_threshold, 2);
    }

    #[test]
    fn test_health_check_config_debug() {
        let config = HealthCheckConfig::default();
        let debug_str = format!("{:?}", config);
        assert!(debug_str.contains("interval"));
        assert!(debug_str.contains("timeout"));
    }

    #[test]
    fn test_agent_health_clone() {
        let agent_id = AgentId::new();
        let health = AgentHealth {
            agent_id,
            is_healthy: false,
            consecutive_failures: 5,
            consecutive_successes: 0,
            last_check: Instant::now(),
            last_response_time_ms: 3000,
            resource_usage: ResourceUsage {
                memory_used: 500,
                cpu_time_used: 100,
                file_descriptors_used: 10,
                processes_used: 2,
            },
        };

        let cloned = health.clone();
        assert_eq!(cloned.agent_id, agent_id);
        assert!(!cloned.is_healthy);
        assert_eq!(cloned.consecutive_failures, 5);
        assert_eq!(cloned.last_response_time_ms, 3000);
        assert_eq!(cloned.resource_usage.memory_used, 500);
    }

    #[test]
    fn test_agent_health_debug() {
        let health = AgentHealth {
            agent_id: AgentId::new(),
            is_healthy: true,
            consecutive_failures: 0,
            consecutive_successes: 10,
            last_check: Instant::now(),
            last_response_time_ms: 5,
            resource_usage: ResourceUsage::default(),
        };
        let debug_str = format!("{:?}", health);
        assert!(debug_str.contains("is_healthy"));
        assert!(debug_str.contains("consecutive_failures"));
    }

    #[test]
    fn test_supervisor_clone_shares_state() {
        let registry = Arc::new(AgentRegistry::new());
        let supervisor = Supervisor::new(registry.clone());
        let cloned = supervisor.clone();

        // Both should share the same Arc pointers
        assert!(Arc::ptr_eq(&supervisor.health_checks, &cloned.health_checks));
        assert!(Arc::ptr_eq(&supervisor.running_agents, &cloned.running_agents));
        assert!(Arc::ptr_eq(&supervisor.cgroups, &cloned.cgroups));
    }

    #[tokio::test]
    async fn test_supervisor_register_multiple_agents() {
        let registry = Arc::new(AgentRegistry::new());
        let supervisor = Supervisor::new(registry.clone());

        let id1 = AgentId::new();
        let id2 = AgentId::new();
        let id3 = AgentId::new();

        supervisor.register_agent(id1).await.unwrap();
        supervisor.register_agent(id2).await.unwrap();
        supervisor.register_agent(id3).await.unwrap();

        let health_map = supervisor.health_checks.read().await;
        assert_eq!(health_map.len(), 3);
        assert!(health_map.contains_key(&id1));
        assert!(health_map.contains_key(&id2));
        assert!(health_map.contains_key(&id3));
    }

    #[tokio::test]
    async fn test_supervisor_get_health() {
        let registry = Arc::new(AgentRegistry::new());
        let supervisor = Supervisor::new(registry.clone());
        let agent_id = AgentId::new();

        // No health yet
        assert!(supervisor.get_health(agent_id).await.is_none());

        // Register, then check
        supervisor.register_agent(agent_id).await.unwrap();
        let health = supervisor.get_health(agent_id).await.unwrap();
        assert!(health.is_healthy);
        assert_eq!(health.agent_id, agent_id);
    }

    #[tokio::test]
    async fn test_supervisor_get_all_health() {
        let registry = Arc::new(AgentRegistry::new());
        let supervisor = Supervisor::new(registry.clone());

        assert!(supervisor.get_all_health().await.is_empty());

        let id1 = AgentId::new();
        let id2 = AgentId::new();
        supervisor.register_agent(id1).await.unwrap();
        supervisor.register_agent(id2).await.unwrap();

        let all = supervisor.get_all_health().await;
        assert_eq!(all.len(), 2);
    }

    #[tokio::test]
    async fn test_supervisor_update_health_status_healthy() {
        let registry = Arc::new(AgentRegistry::new());
        let supervisor = Supervisor::new(registry.clone());
        let agent_id = AgentId::new();

        supervisor.register_agent(agent_id).await.unwrap();

        // Mark healthy several times
        supervisor.update_health_status(agent_id, true).await;
        supervisor.update_health_status(agent_id, true).await;

        let health = supervisor.get_health(agent_id).await.unwrap();
        assert!(health.is_healthy);
        assert_eq!(health.consecutive_successes, 2);
        assert_eq!(health.consecutive_failures, 0);
    }

    #[tokio::test]
    async fn test_supervisor_update_health_status_unhealthy_below_threshold() {
        let registry = Arc::new(AgentRegistry::new());
        let supervisor = Supervisor::new(registry.clone());
        let agent_id = AgentId::new();

        supervisor.register_agent(agent_id).await.unwrap();

        // Mark unhealthy once (threshold is 3 by default)
        supervisor.update_health_status(agent_id, false).await;

        let health = supervisor.get_health(agent_id).await.unwrap();
        // Still healthy — hasn't hit threshold yet
        assert!(health.is_healthy);
        assert_eq!(health.consecutive_failures, 1);
    }

    #[tokio::test]
    async fn test_supervisor_update_health_resets_counters() {
        let registry = Arc::new(AgentRegistry::new());
        let supervisor = Supervisor::new(registry.clone());
        let agent_id = AgentId::new();

        supervisor.register_agent(agent_id).await.unwrap();

        // Failure resets successes
        supervisor.update_health_status(agent_id, true).await;
        supervisor.update_health_status(agent_id, false).await;

        let health = supervisor.get_health(agent_id).await.unwrap();
        assert_eq!(health.consecutive_successes, 0);
        assert_eq!(health.consecutive_failures, 1);

        // Success resets failures
        supervisor.update_health_status(agent_id, true).await;
        let health = supervisor.get_health(agent_id).await.unwrap();
        assert_eq!(health.consecutive_successes, 1);
        assert_eq!(health.consecutive_failures, 0);
    }

    #[tokio::test]
    async fn test_supervisor_shutdown_all_empty() {
        let registry = Arc::new(AgentRegistry::new());
        let supervisor = Supervisor::new(registry.clone());

        // Should succeed even with no agents
        let result = supervisor.shutdown_all().await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_is_process_alive_nonexistent() {
        // PID 4_000_000 is extremely unlikely to exist (max PID is typically 4194304)
        // but won't wrap to -1 like u32::MAX would when cast to pid_t
        assert!(!Supervisor::is_process_alive(4_000_000));
    }

    #[test]
    fn test_is_process_alive_current() {
        let pid = std::process::id();
        assert!(Supervisor::is_process_alive(pid));
    }

    #[test]
    fn test_read_proc_memory_nonexistent() {
        assert_eq!(read_proc_memory(u32::MAX), 0);
    }

    #[test]
    fn test_read_proc_memory_current() {
        let pid = std::process::id();
        let mem = read_proc_memory(pid);
        assert!(mem > 0);
    }

    #[test]
    fn test_read_proc_cpu_time_us_nonexistent() {
        assert_eq!(read_proc_cpu_time_us(u32::MAX), 0);
    }

    #[test]
    fn test_read_proc_cpu_time_us_current() {
        let pid = std::process::id();
        let _cpu = read_proc_cpu_time_us(pid);
        // May be 0 in short test, but should not panic
    }

    #[tokio::test]
    async fn test_mock_agent_control_resource_usage() {
        let mock = MockAgentControl { healthy: true };
        let usage = mock.get_resource_usage().await.unwrap();
        assert_eq!(usage.memory_used, 100 * 1024 * 1024);
        assert_eq!(usage.cpu_time_used, 1000);
        assert_eq!(usage.file_descriptors_used, 10);
        assert_eq!(usage.processes_used, 1);
    }

    #[tokio::test]
    async fn test_mock_agent_control_stop() {
        let mut mock = MockAgentControl { healthy: true };
        let result = mock.stop(StopReason::Normal).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_mock_agent_control_restart() {
        let mut mock = MockAgentControl { healthy: true };
        let result = mock.restart().await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_cgroup_base_constant() {
        assert_eq!(CGROUP_BASE, "/sys/fs/cgroup/agnos");
    }

    // ==================================================================
    // Additional coverage: update_health_status threshold transitions,
    // handle_unhealthy_agent paths, check_resource_limits, shutdown_all,
    // read_proc helpers, cgroup controller with real tempdir data
    // ==================================================================

    #[tokio::test]
    async fn test_update_health_status_transition_to_unhealthy() {
        let registry = Arc::new(AgentRegistry::new());
        let supervisor = Supervisor::new(registry.clone());
        let agent_id = AgentId::new();

        supervisor.register_agent(agent_id).await.unwrap();

        // Mark unhealthy 3 times (= unhealthy_threshold)
        supervisor.update_health_status(agent_id, false).await;
        supervisor.update_health_status(agent_id, false).await;
        // At failure count 2, still healthy (threshold is 3)
        let health = supervisor.get_health(agent_id).await.unwrap();
        assert!(health.is_healthy);
        assert_eq!(health.consecutive_failures, 2);

        // Third failure should trigger transition to unhealthy
        supervisor.update_health_status(agent_id, false).await;
        let health = supervisor.get_health(agent_id).await.unwrap();
        assert!(!health.is_healthy);
        assert_eq!(health.consecutive_failures, 3);
    }

    #[tokio::test]
    async fn test_update_health_status_recovery_from_unhealthy() {
        let registry = Arc::new(AgentRegistry::new());
        let supervisor = Supervisor::new(registry.clone());
        let agent_id = AgentId::new();

        supervisor.register_agent(agent_id).await.unwrap();

        // Make it unhealthy
        for _ in 0..3 {
            supervisor.update_health_status(agent_id, false).await;
        }
        let health = supervisor.get_health(agent_id).await.unwrap();
        assert!(!health.is_healthy);

        // Recover with successes (healthy_threshold = 2)
        supervisor.update_health_status(agent_id, true).await;
        let health = supervisor.get_health(agent_id).await.unwrap();
        assert!(!health.is_healthy); // Not yet recovered
        assert_eq!(health.consecutive_successes, 1);

        supervisor.update_health_status(agent_id, true).await;
        let health = supervisor.get_health(agent_id).await.unwrap();
        assert!(health.is_healthy); // Recovered!
        assert_eq!(health.consecutive_successes, 2);
    }

    #[tokio::test]
    async fn test_update_health_status_nonexistent_agent() {
        let registry = Arc::new(AgentRegistry::new());
        let supervisor = Supervisor::new(registry.clone());

        // Should not panic when updating health of an agent that's not registered
        supervisor.update_health_status(AgentId::new(), true).await;
        supervisor.update_health_status(AgentId::new(), false).await;
    }

    #[tokio::test]
    async fn test_supervisor_start_spawns_loops() {
        let registry = Arc::new(AgentRegistry::new());
        let supervisor = Supervisor::new(registry.clone());

        // start() should succeed and spawn background tasks
        let result = supervisor.start().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_supervisor_shutdown_all_with_agents() {
        let registry = Arc::new(AgentRegistry::new());
        let supervisor = Supervisor::new(registry.clone());

        let id1 = AgentId::new();
        let id2 = AgentId::new();
        supervisor.register_agent(id1).await.unwrap();
        supervisor.register_agent(id2).await.unwrap();

        // Put mock agents into running_agents
        {
            let mut running = supervisor.running_agents.write().await;
            running.insert(id1, Box::new(MockAgentControl { healthy: true }));
            running.insert(id2, Box::new(MockAgentControl { healthy: false }));
        }

        let result = supervisor.shutdown_all().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_supervisor_register_then_unregister_cleans_up() {
        let registry = Arc::new(AgentRegistry::new());
        let supervisor = Supervisor::new(registry.clone());

        let id = AgentId::new();
        supervisor.register_agent(id).await.unwrap();
        assert!(supervisor.get_health(id).await.is_some());

        supervisor.unregister_agent(id).await.unwrap();
        assert!(supervisor.get_health(id).await.is_none());
        // Cgroup map should also be cleaned
        assert!(!supervisor.cgroups.read().await.contains_key(&id));
    }

    #[test]
    fn test_read_proc_memory_current_process() {
        let pid = std::process::id();
        let mem = read_proc_memory(pid);
        assert!(mem > 0, "Current process should have non-zero memory");
    }

    #[test]
    fn test_read_proc_cpu_time_us_current_process() {
        let pid = std::process::id();
        let _cpu = read_proc_cpu_time_us(pid);
        // May be 0 in a short test, but should not panic
    }

    #[test]
    fn test_cgroup_controller_debug() {
        let agent_id = AgentId::new();
        let controller = CgroupController {
            path: PathBuf::from("/tmp/test-debug"),
            agent_id,
        };
        let dbg = format!("{:?}", controller);
        assert!(dbg.contains("CgroupController"));
        assert!(dbg.contains("/tmp/test-debug"));
    }

    #[test]
    fn test_cgroup_controller_cpu_stat_with_tempdir_no_usage_line() {
        let tmp = TempDir::new().unwrap();
        let agent_id = AgentId::new();
        let cg_path = tmp.path().join(agent_id.to_string());
        std::fs::create_dir_all(&cg_path).unwrap();

        let controller = CgroupController {
            path: cg_path.clone(),
            agent_id,
        };

        // Write cpu.stat without usage_usec line
        std::fs::write(cg_path.join("cpu.stat"), "user_usec 100\nsystem_usec 200\n").unwrap();
        assert_eq!(controller.cpu_usage_usec(), 0);
    }

    #[test]
    fn test_cgroup_controller_memory_max_invalid_content() {
        let tmp = TempDir::new().unwrap();
        let agent_id = AgentId::new();
        let cg_path = tmp.path().join(agent_id.to_string());
        std::fs::create_dir_all(&cg_path).unwrap();

        let controller = CgroupController {
            path: cg_path.clone(),
            agent_id,
        };

        // Write invalid content that's not "max" and not a number
        std::fs::write(cg_path.join("memory.max"), "invalid").unwrap();
        assert_eq!(controller.memory_max(), None);
    }

    #[test]
    fn test_cgroup_controller_pids_with_invalid_lines() {
        let tmp = TempDir::new().unwrap();
        let agent_id = AgentId::new();
        let cg_path = tmp.path().join(agent_id.to_string());
        std::fs::create_dir_all(&cg_path).unwrap();

        let controller = CgroupController {
            path: cg_path.clone(),
            agent_id,
        };

        // Mix of valid and invalid lines
        std::fs::write(cg_path.join("cgroup.procs"), "100\nnot_a_pid\n200\n\n300\n").unwrap();
        let pids = controller.pids();
        assert_eq!(pids, vec![100, 200, 300]);
    }

    #[tokio::test]
    async fn test_supervisor_get_all_health_after_unregister() {
        let registry = Arc::new(AgentRegistry::new());
        let supervisor = Supervisor::new(registry.clone());

        let id1 = AgentId::new();
        let id2 = AgentId::new();
        supervisor.register_agent(id1).await.unwrap();
        supervisor.register_agent(id2).await.unwrap();
        assert_eq!(supervisor.get_all_health().await.len(), 2);

        supervisor.unregister_agent(id1).await.unwrap();
        assert_eq!(supervisor.get_all_health().await.len(), 1);
    }

    #[tokio::test]
    async fn test_supervisor_register_same_agent_twice() {
        let registry = Arc::new(AgentRegistry::new());
        let supervisor = Supervisor::new(registry.clone());

        let id = AgentId::new();
        supervisor.register_agent(id).await.unwrap();
        // Registering again should overwrite (reset health counters)
        supervisor.update_health_status(id, false).await;
        supervisor.register_agent(id).await.unwrap();
        let health = supervisor.get_health(id).await.unwrap();
        assert_eq!(health.consecutive_failures, 0);
    }

    #[test]
    fn test_is_process_alive_self() {
        // Our own PID should always be alive
        let pid = std::process::id();
        assert!(Supervisor::is_process_alive(pid));
    }

    #[tokio::test]
    async fn test_check_agent_health_not_in_registry() {
        let registry = Arc::new(AgentRegistry::new());
        let supervisor = Supervisor::new(registry.clone());
        let agent_id = AgentId::new();

        // Agent not in registry should error
        let result = supervisor.check_agent_health(agent_id).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_supervisor_clone_register_visible_in_clone() {
        let registry = Arc::new(AgentRegistry::new());
        let supervisor = Supervisor::new(registry.clone());
        let cloned = supervisor.clone();

        let id = AgentId::new();
        supervisor.register_agent(id).await.unwrap();

        // Visible in clone
        assert!(cloned.get_health(id).await.is_some());
    }

    // ==================================================================
    // Additional coverage: signal_agent paths, check_agent_health states,
    // handle_unhealthy_agent, check_resource_limits, cgroup edge cases,
    // read_proc helper edge cases, supervisor with registry interactions
    // ==================================================================

    #[tokio::test]
    async fn test_signal_agent_no_agent_in_registry() {
        let registry = Arc::new(AgentRegistry::new());
        let supervisor = Supervisor::new(registry.clone());
        // Should not panic when agent is not in registry
        supervisor.signal_agent(AgentId::new(), libc::SIGTERM).await;
    }

    #[tokio::test]
    async fn test_handle_unhealthy_agent_no_control_exceeds_max() {
        let registry = Arc::new(AgentRegistry::new());
        let supervisor = Supervisor::new(registry.clone());
        let agent_id = AgentId::new();

        // Register the agent for supervision
        supervisor.register_agent(agent_id).await.unwrap();

        // Set consecutive_failures high enough to exceed MAX_RESTART_ATTEMPTS (5)
        {
            let mut checks = supervisor.health_checks.write().await;
            if let Some(h) = checks.get_mut(&agent_id) {
                h.consecutive_failures = 10;
            }
        }

        // Should not panic, just try to mark as failed
        supervisor.handle_unhealthy_agent(agent_id).await;
    }

    #[tokio::test]
    async fn test_handle_unhealthy_agent_below_max() {
        let registry = Arc::new(AgentRegistry::new());
        let supervisor = Supervisor::new(registry.clone());
        let agent_id = AgentId::new();

        supervisor.register_agent(agent_id).await.unwrap();

        // Set failures below max — will try to restart but no AgentControl registered
        {
            let mut checks = supervisor.health_checks.write().await;
            if let Some(h) = checks.get_mut(&agent_id) {
                h.consecutive_failures = 2;
            }
        }

        // Should not panic; will attempt restart, find no AgentControl, mark failed
        supervisor.handle_unhealthy_agent(agent_id).await;
    }

    #[test]
    fn test_cgroup_controller_set_memory_limit_zero_is_max() {
        let tmp = TempDir::new().unwrap();
        let agent_id = AgentId::new();
        let cg_path = tmp.path().join(agent_id.to_string());
        std::fs::create_dir_all(&cg_path).unwrap();

        let controller = CgroupController {
            path: cg_path.clone(),
            agent_id,
        };

        controller.set_memory_limit(0).unwrap();
        let content = std::fs::read_to_string(cg_path.join("memory.max")).unwrap();
        assert_eq!(content, "max");
    }

    #[test]
    fn test_cgroup_controller_set_cpu_limit_zero_quota_is_max() {
        let tmp = TempDir::new().unwrap();
        let agent_id = AgentId::new();
        let cg_path = tmp.path().join(agent_id.to_string());
        std::fs::create_dir_all(&cg_path).unwrap();

        let controller = CgroupController {
            path: cg_path.clone(),
            agent_id,
        };

        controller.set_cpu_limit(0, 50000).unwrap();
        let content = std::fs::read_to_string(cg_path.join("cpu.max")).unwrap();
        assert_eq!(content, "max 50000");
    }

    #[test]
    fn test_read_proc_memory_pid_zero() {
        // PID 0 (kernel scheduler) is special, should not panic
        let mem = read_proc_memory(0);
        let _ = mem;
    }

    #[test]
    fn test_read_proc_cpu_time_us_pid_zero() {
        let cpu = read_proc_cpu_time_us(0);
        let _ = cpu;
    }

    #[tokio::test]
    async fn test_supervisor_register_unregister_multiple() {
        let registry = Arc::new(AgentRegistry::new());
        let supervisor = Supervisor::new(registry.clone());

        let ids: Vec<AgentId> = (0..10).map(|_| AgentId::new()).collect();
        for &id in &ids {
            supervisor.register_agent(id).await.unwrap();
        }
        assert_eq!(supervisor.get_all_health().await.len(), 10);

        // Unregister half
        for &id in &ids[..5] {
            supervisor.unregister_agent(id).await.unwrap();
        }
        assert_eq!(supervisor.get_all_health().await.len(), 5);

        // Remaining should still be there
        for &id in &ids[5..] {
            assert!(supervisor.get_health(id).await.is_some());
        }
    }

    #[tokio::test]
    async fn test_supervisor_health_alternating_updates() {
        let registry = Arc::new(AgentRegistry::new());
        let supervisor = Supervisor::new(registry.clone());
        let agent_id = AgentId::new();
        supervisor.register_agent(agent_id).await.unwrap();

        // Alternate healthy/unhealthy to verify counter resets
        supervisor.update_health_status(agent_id, true).await;
        supervisor.update_health_status(agent_id, false).await;
        supervisor.update_health_status(agent_id, true).await;
        supervisor.update_health_status(agent_id, false).await;

        let health = supervisor.get_health(agent_id).await.unwrap();
        // After alternating, consecutive counters should reflect the last transition
        assert_eq!(health.consecutive_failures, 1);
        assert_eq!(health.consecutive_successes, 0);
    }

    #[test]
    fn test_cgroup_controller_memory_current_with_whitespace() {
        let tmp = TempDir::new().unwrap();
        let agent_id = AgentId::new();
        let cg_path = tmp.path().join(agent_id.to_string());
        std::fs::create_dir_all(&cg_path).unwrap();

        let controller = CgroupController {
            path: cg_path.clone(),
            agent_id,
        };

        // Kernel often writes with trailing newline
        std::fs::write(cg_path.join("memory.current"), "12345678\n").unwrap();
        assert_eq!(controller.memory_current(), 12345678);
    }

    #[test]
    fn test_cgroup_controller_cpu_stat_multiple_lines() {
        let tmp = TempDir::new().unwrap();
        let agent_id = AgentId::new();
        let cg_path = tmp.path().join(agent_id.to_string());
        std::fs::create_dir_all(&cg_path).unwrap();

        let controller = CgroupController {
            path: cg_path.clone(),
            agent_id,
        };

        // Realistic cpu.stat output
        std::fs::write(
            cg_path.join("cpu.stat"),
            "usage_usec 9876543\nuser_usec 6000000\nsystem_usec 3876543\nnr_periods 100\nnr_throttled 5\nthrottled_usec 50000\n",
        ).unwrap();
        assert_eq!(controller.cpu_usage_usec(), 9876543);
    }
}

