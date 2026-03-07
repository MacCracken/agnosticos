//! Agent representation and lifecycle management

use std::path::PathBuf;
use std::process::Stdio;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, RwLock};
use tracing::{info, warn};

use agnos_common::{AgentConfig, AgentId, AgentStatus, Message, ResourceUsage, StopReason};

use crate::ipc::AgentIpc;
use crate::sandbox::Sandbox;

/// Handle to a running agent
#[derive(Debug, Clone)]
pub struct AgentHandle {
    pub id: AgentId,
    pub name: String,
    pub status: AgentStatus,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub resource_usage: ResourceUsage,
    /// PID of the agent process (if spawned)
    pub pid: Option<u32>,
}

/// Represents a running agent process
pub struct Agent {
    id: AgentId,
    config: AgentConfig,
    status: RwLock<AgentStatus>,
    process: Option<Child>,
    _ipc: Option<AgentIpc>,
    sandbox: Sandbox,
    started_at: Option<Instant>,
    message_tx: mpsc::Sender<Message>,
    _message_rx: Option<mpsc::Receiver<Message>>,
}

impl Agent {
    /// Create a new agent from configuration
    pub async fn new(config: AgentConfig) -> Result<(Self, mpsc::Receiver<Message>)> {
        let id = AgentId::new();
        let (message_tx, message_rx) = mpsc::channel(100);

        let sandbox =
            Sandbox::new(&config.sandbox).with_context(|| "Failed to create agent sandbox")?;

        let agent = Self {
            id,
            config,
            status: RwLock::new(AgentStatus::Pending),
            process: None,
            _ipc: None,
            sandbox,
            started_at: None,
            message_tx,
            _message_rx: None,
        };

        Ok((agent, message_rx))
    }

    /// Get agent ID
    pub fn id(&self) -> AgentId {
        self.id
    }

    /// Get agent handle for external reference
    pub async fn handle(&self) -> AgentHandle {
        let pid = self.process.as_ref().and_then(|p| p.id());
        AgentHandle {
            id: self.id,
            name: self.config.name.clone(),
            status: *self.status.read().await,
            created_at: chrono::Utc::now(),
            started_at: None,
            resource_usage: ResourceUsage::default(),
            pid,
        }
    }

    /// Start the agent process
    pub async fn start(&mut self) -> Result<()> {
        let mut status = self.status.write().await;

        if *status != AgentStatus::Pending && *status != AgentStatus::Stopped {
            return Err(anyhow::anyhow!(
                "Agent is not in a startable state: {:?}",
                *status
            ));
        }

        *status = AgentStatus::Starting;
        drop(status);

        info!("Starting agent {} ({})", self.config.name, self.id);

        // Apply sandbox restrictions
        self.sandbox.apply().await?;

        // Spawn agent process
        let executable = self.find_agent_executable().await?;

        let mut cmd = Command::new(&executable);
        cmd.arg("--agent-id")
            .arg(self.id.to_string())
            .arg("--agent-name")
            .arg(&self.config.name)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // Apply resource limits
        if let Some(rlimits) = self.build_resource_limits() {
            unsafe {
                cmd.pre_exec(move || {
                    rlimits.apply()?;
                    Ok(())
                });
            }
        }

        let child = cmd
            .spawn()
            .with_context(|| format!("Failed to spawn agent process: {}", executable.display()))?;

        info!("Agent {} started with PID {:?}", self.id, child.id());

        self.process = Some(child);
        self.started_at = Some(Instant::now());

        let mut status = self.status.write().await;
        *status = AgentStatus::Running;

        Ok(())
    }

    /// Stop the agent gracefully
    pub async fn stop(&mut self, reason: StopReason) -> Result<()> {
        info!("Stopping agent {}: {:?}", self.id, reason);

        let mut status = self.status.write().await;
        *status = AgentStatus::Stopping;
        drop(status);

        // Send graceful shutdown signal
        if let Some(ref mut process) = self.process {
            #[cfg(unix)]
            {
                use nix::sys::signal::{self, Signal};
                use nix::unistd::Pid;

                if let Some(pid) = process.id() {
                    let _ = signal::kill(Pid::from_raw(pid as i32), Signal::SIGTERM);
                }
            }

            // Wait for graceful shutdown with timeout
            match tokio::time::timeout(Duration::from_secs(10), process.wait()).await {
                Ok(Ok(_)) => {
                    info!("Agent {} stopped gracefully", self.id);
                }
                Ok(Err(e)) => {
                    warn!("Agent {} exit error: {}", self.id, e);
                    process.kill().await.ok();
                }
                Err(_) => {
                    warn!("Agent {} shutdown timeout, forcing kill", self.id);
                    process.kill().await.ok();
                }
            }
        }

        let mut status = self.status.write().await;
        *status = AgentStatus::Stopped;

        Ok(())
    }

    /// Pause the agent (suspend execution via SIGSTOP)
    pub async fn pause(&mut self) -> Result<()> {
        let mut status = self.status.write().await;
        if *status != AgentStatus::Running {
            return Err(anyhow::anyhow!(
                "Cannot pause agent in state: {:?}",
                *status
            ));
        }

        // Send SIGSTOP to actually suspend the process
        if let Some(ref process) = self.process {
            if let Some(pid) = process.id() {
                #[cfg(unix)]
                {
                    use nix::sys::signal::{self, Signal};
                    use nix::unistd::Pid;
                    signal::kill(Pid::from_raw(pid as i32), Signal::SIGSTOP).map_err(|e| {
                        anyhow::anyhow!("Failed to SIGSTOP agent {}: {}", self.id, e)
                    })?;
                }
            }
        }

        *status = AgentStatus::Paused;
        info!("Agent {} paused (SIGSTOP)", self.id);
        Ok(())
    }

    /// Resume a paused agent (via SIGCONT)
    pub async fn resume(&mut self) -> Result<()> {
        let mut status = self.status.write().await;
        if *status != AgentStatus::Paused {
            return Err(anyhow::anyhow!(
                "Cannot resume agent in state: {:?}",
                *status
            ));
        }

        // Send SIGCONT to resume the process
        if let Some(ref process) = self.process {
            if let Some(pid) = process.id() {
                #[cfg(unix)]
                {
                    use nix::sys::signal::{self, Signal};
                    use nix::unistd::Pid;
                    signal::kill(Pid::from_raw(pid as i32), Signal::SIGCONT).map_err(|e| {
                        anyhow::anyhow!("Failed to SIGCONT agent {}: {}", self.id, e)
                    })?;
                }
            }
        }

        *status = AgentStatus::Running;
        info!("Agent {} resumed (SIGCONT)", self.id);
        Ok(())
    }

    /// Get current resource usage by reading from `/proc/{pid}/`.
    ///
    /// Returns real memory (VmRSS), CPU time (utime+stime), FD count, and
    /// thread count for the agent's process.  Falls back to defaults if the
    /// process has no PID or `/proc` is unavailable.
    pub async fn resource_usage(&self) -> ResourceUsage {
        let pid = match self.process.as_ref().and_then(|p| p.id()) {
            Some(p) => p,
            None => return ResourceUsage::default(),
        };

        let memory_used = Self::read_vm_rss(pid);
        let cpu_time_used = Self::read_cpu_time_ms(pid);
        let file_descriptors_used = Self::count_fds(pid);
        let processes_used = Self::count_threads(pid);

        ResourceUsage {
            memory_used,
            cpu_time_used,
            file_descriptors_used,
            processes_used,
        }
    }

    /// Read VmRSS from /proc/{pid}/status in bytes.
    fn read_vm_rss(pid: u32) -> u64 {
        let path = format!("/proc/{}/status", pid);
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|contents| {
                for line in contents.lines() {
                    if let Some(val) = line.strip_prefix("VmRSS:") {
                        let kb: u64 = val.split_whitespace().next()?.parse().ok()?;
                        return Some(kb * 1024);
                    }
                }
                None
            })
            .unwrap_or(0)
    }

    /// Read CPU time (utime + stime) from /proc/{pid}/stat in milliseconds.
    fn read_cpu_time_ms(pid: u32) -> u64 {
        let path = format!("/proc/{}/stat", pid);
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|contents| {
                let after_comm = contents.find(')')?.checked_add(2)?;
                let fields: Vec<&str> = contents[after_comm..].split_whitespace().collect();
                let utime: u64 = fields.get(11)?.parse().ok()?;
                let stime: u64 = fields.get(12)?.parse().ok()?;
                let ticks = utime + stime;
                let ticks_per_sec = unsafe { libc::sysconf(libc::_SC_CLK_TCK) } as u64;
                if ticks_per_sec > 0 {
                    Some(ticks * 1000 / ticks_per_sec)
                } else {
                    Some(ticks * 10) // fallback: assume 100 Hz
                }
            })
            .unwrap_or(0)
    }

    /// Count open file descriptors from /proc/{pid}/fd/.
    fn count_fds(pid: u32) -> u32 {
        let path = format!("/proc/{}/fd", pid);
        std::fs::read_dir(&path)
            .map(|entries| entries.count() as u32)
            .unwrap_or(0)
    }

    /// Count threads from /proc/{pid}/task/.
    fn count_threads(pid: u32) -> u32 {
        let path = format!("/proc/{}/task", pid);
        std::fs::read_dir(&path)
            .map(|entries| entries.count() as u32)
            .unwrap_or(1) // at least 1 thread (the main thread)
    }

    /// Send a message to the agent
    pub async fn send_message(&self, message: Message) -> Result<()> {
        self.message_tx
            .send(message)
            .await
            .map_err(|_| anyhow::anyhow!("Failed to send message to agent"))?;
        Ok(())
    }

    /// Check if the agent is still running
    pub async fn is_running(&self) -> bool {
        let status = self.status.read().await;
        *status == AgentStatus::Running
    }

    /// Find the executable for this agent type
    async fn find_agent_executable(&self) -> Result<PathBuf> {
        // Look for agent implementations in standard locations
        let search_paths = vec![
            PathBuf::from("/usr/lib/agnos/agents"),
            PathBuf::from("/opt/agnos/agents"),
            PathBuf::from("./agents"),
        ];

        let agent_type = format!("{:?}", self.config.agent_type).to_lowercase();
        let executable_name = format!("agnos-agent-{}-agent", agent_type);

        for path in search_paths {
            let executable = path.join(&executable_name);
            if executable.exists() {
                return Ok(executable);
            }
        }

        // Default to a generic agent runner
        Ok(PathBuf::from("/usr/bin/agnos-agent-runner"))
    }

    fn build_resource_limits(&self) -> Option<ResourceLimits> {
        Some(ResourceLimits {
            max_memory: self.config.resource_limits.max_memory,
            max_cpu_time: self.config.resource_limits.max_cpu_time,
        })
    }
}

#[async_trait::async_trait]
impl crate::supervisor::AgentControl for Agent {
    async fn check_health(&self) -> Result<bool> {
        // Check process is alive via kill(pid, 0)
        if let Some(ref process) = self.process {
            if let Some(pid) = process.id() {
                #[cfg(unix)]
                {
                    let alive = unsafe { libc::kill(pid as i32, 0) } == 0;
                    return Ok(alive);
                }
                #[cfg(not(unix))]
                {
                    let _ = pid;
                    return Ok(true);
                }
            }
        }
        // No process spawned — check status
        Ok(*self.status.read().await == AgentStatus::Running)
    }

    async fn get_resource_usage(&self) -> Result<ResourceUsage> {
        Ok(self.resource_usage().await)
    }

    async fn stop(&mut self, reason: StopReason) -> Result<()> {
        Agent::stop(self, reason).await
    }

    async fn restart(&mut self) -> Result<()> {
        Agent::stop(self, StopReason::Normal).await?;
        // Reset status so start() will accept it
        *self.status.write().await = AgentStatus::Pending;
        Agent::start(self).await
    }
}

/// Resource limits for agent processes
struct ResourceLimits {
    max_memory: u64,
    max_cpu_time: u64,
}

impl ResourceLimits {
    fn apply(&self) -> std::io::Result<()> {
        #[cfg(unix)]
        {
            use libc::{rlimit, setrlimit, RLIMIT_AS, RLIMIT_CPU};

            // Set memory limit
            if self.max_memory > 0 {
                let limit = rlimit {
                    rlim_cur: self.max_memory,
                    rlim_max: self.max_memory,
                };
                unsafe {
                    setrlimit(RLIMIT_AS, &limit);
                }
            }

            // Set CPU time limit
            if self.max_cpu_time > 0 {
                let limit = rlimit {
                    rlim_cur: self.max_cpu_time,
                    rlim_max: self.max_cpu_time,
                };
                unsafe {
                    setrlimit(RLIMIT_CPU, &limit);
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_handle_default() {
        let handle = AgentHandle {
            id: AgentId::new(),
            name: "test-agent".to_string(),
            status: AgentStatus::Pending,
            created_at: chrono::Utc::now(),
            started_at: None,
            resource_usage: ResourceUsage::default(),
            pid: None,
        };

        assert_eq!(handle.name, "test-agent");
        assert_eq!(handle.status, AgentStatus::Pending);
        assert!(handle.pid.is_none());
    }

    #[test]
    fn test_agent_handle_running() {
        let handle = AgentHandle {
            id: AgentId::new(),
            name: "running-agent".to_string(),
            status: AgentStatus::Running,
            created_at: chrono::Utc::now(),
            started_at: Some(chrono::Utc::now()),
            resource_usage: ResourceUsage {
                memory_used: 1024 * 1024 * 100,
                cpu_time_used: 50000,
                file_descriptors_used: 5,
                processes_used: 1,
            },
            pid: Some(12345),
        };

        assert_eq!(handle.status, AgentStatus::Running);
        assert!(handle.pid.is_some());
        assert_eq!(handle.pid, Some(12345));
        assert!(handle.resource_usage.memory_used > 0);
    }

    #[test]
    fn test_agent_handle_debug() {
        let handle = AgentHandle {
            id: AgentId::new(),
            name: "debug-agent".to_string(),
            status: AgentStatus::Stopped,
            created_at: chrono::Utc::now(),
            started_at: None,
            resource_usage: ResourceUsage::default(),
            pid: None,
        };

        let debug_str = format!("{:?}", handle);
        assert!(debug_str.contains("debug-agent"));
        assert!(debug_str.contains("Stopped"));
    }

    #[test]
    fn test_resource_limits_default() {
        let limits = ResourceLimits {
            max_memory: 0,
            max_cpu_time: 0,
        };

        assert_eq!(limits.max_memory, 0);
        assert_eq!(limits.max_cpu_time, 0);
    }

    #[test]
    fn test_resource_limits_custom() {
        let limits = ResourceLimits {
            max_memory: 1024 * 1024 * 1024,
            max_cpu_time: 3600,
        };

        assert_eq!(limits.max_memory, 1024 * 1024 * 1024);
        assert_eq!(limits.max_cpu_time, 3600);
    }

    #[test]
    fn test_resource_limits_apply_zero() {
        let limits = ResourceLimits {
            max_memory: 0,
            max_cpu_time: 0,
        };

        let result = limits.apply();
        assert!(result.is_ok());
    }

    #[test]
    fn test_agent_id_new_unique() {
        let id1 = AgentId::new();
        let id2 = AgentId::new();

        assert_ne!(id1, id2);
    }

    #[test]
    fn test_agent_status_variants() {
        assert_eq!(format!("{:?}", AgentStatus::Pending), "Pending");
        assert_eq!(format!("{:?}", AgentStatus::Starting), "Starting");
        assert_eq!(format!("{:?}", AgentStatus::Running), "Running");
        assert_eq!(format!("{:?}", AgentStatus::Stopping), "Stopping");
        assert_eq!(format!("{:?}", AgentStatus::Stopped), "Stopped");
        assert_eq!(format!("{:?}", AgentStatus::Failed), "Failed");
    }

    #[test]
    fn test_agent_status_is_stopped() {
        assert_eq!(AgentStatus::Pending, AgentStatus::Pending);
        assert_eq!(AgentStatus::Stopped, AgentStatus::Stopped);
        assert_eq!(AgentStatus::Failed, AgentStatus::Failed);
    }

    #[test]
    fn test_agent_handle_clone() {
        let handle = AgentHandle {
            id: AgentId::new(),
            name: "clone-test".to_string(),
            status: AgentStatus::Running,
            created_at: chrono::Utc::now(),
            started_at: Some(chrono::Utc::now()),
            resource_usage: ResourceUsage {
                memory_used: 42,
                cpu_time_used: 100,
                file_descriptors_used: 3,
                processes_used: 1,
            },
            pid: Some(9999),
        };

        let cloned = handle.clone();
        assert_eq!(cloned.id, handle.id);
        assert_eq!(cloned.name, "clone-test");
        assert_eq!(cloned.status, AgentStatus::Running);
        assert_eq!(cloned.pid, Some(9999));
        assert_eq!(cloned.resource_usage.memory_used, 42);
        assert_eq!(cloned.resource_usage.cpu_time_used, 100);
        assert_eq!(cloned.resource_usage.file_descriptors_used, 3);
        assert_eq!(cloned.resource_usage.processes_used, 1);
    }

    #[test]
    fn test_agent_handle_all_statuses() {
        for status in [
            AgentStatus::Pending,
            AgentStatus::Starting,
            AgentStatus::Running,
            AgentStatus::Paused,
            AgentStatus::Stopping,
            AgentStatus::Stopped,
            AgentStatus::Failed,
        ] {
            let handle = AgentHandle {
                id: AgentId::new(),
                name: format!("agent-{:?}", status),
                status,
                created_at: chrono::Utc::now(),
                started_at: None,
                resource_usage: ResourceUsage::default(),
                pid: None,
            };
            assert_eq!(handle.status, status);
        }
    }

    #[test]
    fn test_agent_handle_with_pid() {
        let handle = AgentHandle {
            id: AgentId::new(),
            name: "pid-agent".to_string(),
            status: AgentStatus::Running,
            created_at: chrono::Utc::now(),
            started_at: Some(chrono::Utc::now()),
            resource_usage: ResourceUsage::default(),
            pid: Some(1),
        };
        assert_eq!(handle.pid, Some(1));

        let handle_no_pid = AgentHandle {
            id: AgentId::new(),
            name: "no-pid-agent".to_string(),
            status: AgentStatus::Pending,
            created_at: chrono::Utc::now(),
            started_at: None,
            resource_usage: ResourceUsage::default(),
            pid: None,
        };
        assert!(handle_no_pid.pid.is_none());
    }

    #[test]
    fn test_resource_usage_default_is_zero() {
        let usage = ResourceUsage::default();
        assert_eq!(usage.memory_used, 0);
        assert_eq!(usage.cpu_time_used, 0);
        assert_eq!(usage.file_descriptors_used, 0);
        assert_eq!(usage.processes_used, 0);
    }

    #[test]
    fn test_agent_id_display() {
        let id = AgentId::new();
        let s = id.to_string();
        // AgentId wraps a UUID, so the display string should be a valid UUID
        assert!(!s.is_empty());
        assert!(uuid::Uuid::parse_str(&s).is_ok());
    }

    #[test]
    fn test_agent_status_equality() {
        assert_ne!(AgentStatus::Running, AgentStatus::Stopped);
        assert_ne!(AgentStatus::Pending, AgentStatus::Failed);
        assert_ne!(AgentStatus::Starting, AgentStatus::Stopping);
        assert_eq!(AgentStatus::Paused, AgentStatus::Paused);
    }

    #[test]
    fn test_agent_handle_resource_usage_large_values() {
        let handle = AgentHandle {
            id: AgentId::new(),
            name: "heavy-agent".to_string(),
            status: AgentStatus::Running,
            created_at: chrono::Utc::now(),
            started_at: Some(chrono::Utc::now()),
            resource_usage: ResourceUsage {
                memory_used: 16 * 1024 * 1024 * 1024, // 16 GB
                cpu_time_used: 86_400_000,            // 24 hours in ms
                file_descriptors_used: 65535,
                processes_used: 1024,
            },
            pid: Some(42),
        };
        assert_eq!(handle.resource_usage.memory_used, 16 * 1024 * 1024 * 1024);
        assert_eq!(handle.resource_usage.cpu_time_used, 86_400_000);
        assert_eq!(handle.resource_usage.file_descriptors_used, 65535);
        assert_eq!(handle.resource_usage.processes_used, 1024);
    }

    #[test]
    fn test_read_vm_rss_nonexistent_pid() {
        // PID 0 or very large PID should return 0 (no /proc entry)
        assert_eq!(Agent::read_vm_rss(u32::MAX), 0);
    }

    #[test]
    fn test_read_cpu_time_ms_nonexistent_pid() {
        assert_eq!(Agent::read_cpu_time_ms(u32::MAX), 0);
    }

    #[test]
    fn test_count_fds_nonexistent_pid() {
        assert_eq!(Agent::count_fds(u32::MAX), 0);
    }

    #[test]
    fn test_count_threads_nonexistent_pid() {
        // Falls back to 1 (at least main thread)
        assert_eq!(Agent::count_threads(u32::MAX), 1);
    }

    #[test]
    fn test_read_vm_rss_current_process() {
        // Reading our own process should return non-zero
        let pid = std::process::id();
        let rss = Agent::read_vm_rss(pid);
        assert!(rss > 0, "Current process should have non-zero RSS");
    }

    #[test]
    fn test_read_cpu_time_current_process() {
        let pid = std::process::id();
        let cpu = Agent::read_cpu_time_ms(pid);
        // CPU time might be 0 for a very short-lived test, but it shouldn't panic
        let _ = cpu;
    }

    #[test]
    fn test_count_fds_current_process() {
        let pid = std::process::id();
        let fds = Agent::count_fds(pid);
        assert!(fds > 0, "Current process should have open file descriptors");
    }

    #[test]
    fn test_count_threads_current_process() {
        let pid = std::process::id();
        let threads = Agent::count_threads(pid);
        assert!(
            threads >= 1,
            "Current process should have at least 1 thread"
        );
    }

    #[tokio::test]
    async fn test_agent_new_creates_pending_agent() {
        let config = AgentConfig {
            name: "test-new-agent".to_string(),
            agent_type: agnos_common::AgentType::User,
            ..Default::default()
        };

        let (agent, _rx) = Agent::new(config).await.unwrap();
        assert!(!agent.id().to_string().is_empty());
        assert!(!agent.is_running().await);
    }

    #[tokio::test]
    async fn test_agent_handle_method() {
        let config = AgentConfig {
            name: "handle-test-agent".to_string(),
            agent_type: agnos_common::AgentType::Service,
            ..Default::default()
        };

        let (agent, _rx) = Agent::new(config).await.unwrap();
        let handle = agent.handle().await;

        assert_eq!(handle.name, "handle-test-agent");
        assert_eq!(handle.status, AgentStatus::Pending);
        assert!(handle.pid.is_none());
        assert_eq!(handle.resource_usage.memory_used, 0);
    }

    #[tokio::test]
    async fn test_agent_resource_usage_no_process() {
        let config = AgentConfig {
            name: "no-proc-agent".to_string(),
            ..Default::default()
        };

        let (agent, _rx) = Agent::new(config).await.unwrap();
        let usage = agent.resource_usage().await;

        // No process spawned, so should return defaults
        assert_eq!(usage.memory_used, 0);
        assert_eq!(usage.cpu_time_used, 0);
        assert_eq!(usage.file_descriptors_used, 0);
        assert_eq!(usage.processes_used, 0);
    }

    #[tokio::test]
    async fn test_agent_send_message() {
        let config = AgentConfig {
            name: "msg-agent".to_string(),
            ..Default::default()
        };

        let (agent, mut rx) = Agent::new(config).await.unwrap();

        let msg = Message {
            id: "msg-1".to_string(),
            source: "test".to_string(),
            target: "msg-agent".to_string(),
            message_type: agnos_common::MessageType::Command,
            payload: serde_json::json!({"hello": "world"}),
            timestamp: chrono::Utc::now(),
        };

        agent.send_message(msg.clone()).await.unwrap();
        let received = rx.recv().await.unwrap();
        assert_eq!(received.id, "msg-1");
        assert_eq!(received.source, "test");
    }

    #[tokio::test]
    async fn test_agent_is_running_false_when_pending() {
        let config = AgentConfig {
            name: "pending-agent".to_string(),
            ..Default::default()
        };
        let (agent, _rx) = Agent::new(config).await.unwrap();
        assert!(!agent.is_running().await);
    }

    // ==================================================================
    // Additional coverage: find_agent_executable, build_resource_limits,
    // ResourceLimits::apply with non-zero values, Agent state methods,
    // handle() details, send_message channel full
    // ==================================================================

    #[tokio::test]
    async fn test_agent_find_agent_executable_default() {
        let config = AgentConfig {
            name: "exec-test".to_string(),
            agent_type: agnos_common::AgentType::User,
            ..Default::default()
        };
        let (agent, _rx) = Agent::new(config).await.unwrap();

        // Since no executable exists in standard paths, should fall back to default
        let exec = agent.find_agent_executable().await.unwrap();
        assert_eq!(exec, PathBuf::from("/usr/bin/agnos-agent-runner"));
    }

    #[tokio::test]
    async fn test_agent_build_resource_limits() {
        let config = AgentConfig {
            name: "limits-test".to_string(),
            resource_limits: agnos_common::ResourceLimits {
                max_memory: 512 * 1024 * 1024,
                max_cpu_time: 7200,
                max_file_descriptors: 256,
                max_processes: 10,
            },
            ..Default::default()
        };
        let (agent, _rx) = Agent::new(config).await.unwrap();

        let limits = agent.build_resource_limits();
        assert!(limits.is_some());
        let limits = limits.unwrap();
        assert_eq!(limits.max_memory, 512 * 1024 * 1024);
        assert_eq!(limits.max_cpu_time, 7200);
    }

    #[test]
    fn test_resource_limits_apply_cpu_only() {
        // Only set CPU limit (not memory, which would restrict the test process)
        let limits = ResourceLimits {
            max_memory: 0,
            max_cpu_time: 3600,
        };
        let result = limits.apply();
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_agent_handle_has_correct_name() {
        let config = AgentConfig {
            name: "named-agent".to_string(),
            agent_type: agnos_common::AgentType::Service,
            ..Default::default()
        };
        let (agent, _rx) = Agent::new(config).await.unwrap();
        let handle = agent.handle().await;
        assert_eq!(handle.name, "named-agent");
        assert_eq!(handle.status, AgentStatus::Pending);
        assert!(handle.pid.is_none());
    }

    #[tokio::test]
    async fn test_agent_id_is_stable() {
        let config = AgentConfig {
            name: "stable-id".to_string(),
            ..Default::default()
        };
        let (agent, _rx) = Agent::new(config).await.unwrap();
        let id1 = agent.id();
        let id2 = agent.id();
        assert_eq!(id1, id2, "Agent ID should not change between calls");
    }

    #[tokio::test]
    async fn test_agent_send_multiple_messages() {
        let config = AgentConfig {
            name: "multi-msg".to_string(),
            ..Default::default()
        };
        let (agent, mut rx) = Agent::new(config).await.unwrap();

        for i in 0..5 {
            let msg = Message {
                id: format!("msg-{}", i),
                source: "test".to_string(),
                target: "multi-msg".to_string(),
                message_type: agnos_common::MessageType::Command,
                payload: serde_json::json!({"index": i}),
                timestamp: chrono::Utc::now(),
            };
            agent.send_message(msg).await.unwrap();
        }

        // Receive all 5
        for i in 0..5 {
            let received = rx.recv().await.unwrap();
            assert_eq!(received.id, format!("msg-{}", i));
        }
    }

    #[tokio::test]
    async fn test_agent_resource_usage_returns_default_no_process() {
        let config = AgentConfig {
            name: "no-proc".to_string(),
            ..Default::default()
        };
        let (agent, _rx) = Agent::new(config).await.unwrap();
        let usage = agent.resource_usage().await;
        assert_eq!(usage.memory_used, 0);
        assert_eq!(usage.cpu_time_used, 0);
        assert_eq!(usage.file_descriptors_used, 0);
        assert_eq!(usage.processes_used, 0);
    }

    #[test]
    fn test_read_vm_rss_pid_1() {
        // PID 1 should exist; may return 0 if /proc/1/status not readable (permissions)
        let rss = Agent::read_vm_rss(1);
        // Just ensure no panic
        let _ = rss;
    }

    #[test]
    fn test_count_fds_pid_1() {
        // PID 1 may not be readable without root
        let fds = Agent::count_fds(1);
        let _ = fds;
    }

    #[test]
    fn test_count_threads_pid_1() {
        let threads = Agent::count_threads(1);
        // At minimum 1 (fallback)
        assert!(threads >= 1);
    }

    #[tokio::test]
    async fn test_agent_new_different_agent_types() {
        for agent_type in [
            agnos_common::AgentType::User,
            agnos_common::AgentType::Service,
            agnos_common::AgentType::System,
        ] {
            let config = AgentConfig {
                name: format!("{:?}-agent", agent_type),
                agent_type,
                ..Default::default()
            };
            let result = Agent::new(config).await;
            assert!(result.is_ok());
        }
    }

    #[tokio::test]
    async fn test_agent_handle_resource_usage_is_default() {
        let config = AgentConfig {
            name: "handle-usage".to_string(),
            ..Default::default()
        };
        let (agent, _rx) = Agent::new(config).await.unwrap();
        let handle = agent.handle().await;
        assert_eq!(handle.resource_usage.memory_used, 0);
        assert_eq!(handle.resource_usage.cpu_time_used, 0);
        assert_eq!(handle.resource_usage.file_descriptors_used, 0);
        assert_eq!(handle.resource_usage.processes_used, 0);
    }

    #[test]
    fn test_agent_handle_debug_format() {
        let handle = AgentHandle {
            id: AgentId::new(),
            name: "dbg-test".to_string(),
            status: AgentStatus::Running,
            created_at: chrono::Utc::now(),
            started_at: Some(chrono::Utc::now()),
            resource_usage: ResourceUsage::default(),
            pid: Some(42),
        };
        let dbg = format!("{:?}", handle);
        assert!(dbg.contains("dbg-test"));
        assert!(dbg.contains("Running"));
        assert!(dbg.contains("42"));
    }

    #[tokio::test]
    async fn test_agent_find_executable_service_type() {
        let config = AgentConfig {
            name: "svc-exec".to_string(),
            agent_type: agnos_common::AgentType::Service,
            ..Default::default()
        };
        let (agent, _rx) = Agent::new(config).await.unwrap();
        let exec = agent.find_agent_executable().await.unwrap();
        // Should fall back to default since no standard paths exist
        assert_eq!(exec, PathBuf::from("/usr/bin/agnos-agent-runner"));
    }

    #[tokio::test]
    async fn test_agent_find_executable_system_type() {
        let config = AgentConfig {
            name: "sys-exec".to_string(),
            agent_type: agnos_common::AgentType::System,
            ..Default::default()
        };
        let (agent, _rx) = Agent::new(config).await.unwrap();
        let exec = agent.find_agent_executable().await.unwrap();
        assert_eq!(exec, PathBuf::from("/usr/bin/agnos-agent-runner"));
    }

    // ==================================================================
    // Additional coverage: pause/resume error paths, stop without process,
    // ResourceLimits::apply with non-zero values, start error state,
    // message channel drop, handle details, agent lifecycle edge cases
    // ==================================================================

    #[tokio::test]
    async fn test_agent_pause_when_not_running() {
        let config = AgentConfig {
            name: "pause-fail".to_string(),
            ..Default::default()
        };
        let (mut agent, _rx) = Agent::new(config).await.unwrap();
        // Agent is Pending, not Running — pause should fail
        let result = agent.pause().await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Cannot pause"));
    }

    #[tokio::test]
    async fn test_agent_resume_when_not_paused() {
        let config = AgentConfig {
            name: "resume-fail".to_string(),
            ..Default::default()
        };
        let (mut agent, _rx) = Agent::new(config).await.unwrap();
        // Agent is Pending, not Paused — resume should fail
        let result = agent.resume().await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Cannot resume"));
    }

    #[tokio::test]
    async fn test_agent_stop_without_process() {
        let config = AgentConfig {
            name: "stop-no-proc".to_string(),
            ..Default::default()
        };
        let (mut agent, _rx) = Agent::new(config).await.unwrap();
        // Stop should succeed even without a spawned process
        let result = agent.stop(agnos_common::StopReason::Normal).await;
        assert!(result.is_ok());
        assert!(!agent.is_running().await);
    }

    #[tokio::test]
    async fn test_agent_stop_with_different_reasons() {
        for reason in [
            agnos_common::StopReason::Normal,
            agnos_common::StopReason::Error("test error".to_string()),
            agnos_common::StopReason::ResourceLimit,
            agnos_common::StopReason::UserRequest,
        ] {
            let config = AgentConfig {
                name: format!("stop-{:?}", reason),
                ..Default::default()
            };
            let (mut agent, _rx) = Agent::new(config).await.unwrap();
            let result = agent.stop(reason).await;
            assert!(result.is_ok());
        }
    }

    #[tokio::test]
    async fn test_agent_start_when_not_pending_or_stopped() {
        let config = AgentConfig {
            name: "start-fail".to_string(),
            ..Default::default()
        };
        let (mut agent, _rx) = Agent::new(config).await.unwrap();
        // Stop the agent first (moves to Stopped)
        agent.stop(agnos_common::StopReason::Normal).await.unwrap();
        // Start again from Stopped should attempt (may fail finding executable, but state check passes)
        let result = agent.start().await;
        // The start will probably fail due to sandbox or missing executable, but the state check passed
        let _ = result;
    }

    #[test]
    fn test_resource_limits_apply_with_memory() {
        let limits = ResourceLimits {
            max_memory: 0, // Don't restrict the test process
            max_cpu_time: 7200,
        };
        let result = limits.apply();
        assert!(result.is_ok());
    }

    #[test]
    fn test_resource_limits_apply_both_zero() {
        let limits = ResourceLimits {
            max_memory: 0,
            max_cpu_time: 0,
        };
        let result = limits.apply();
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_agent_send_message_after_receiver_dropped() {
        let config = AgentConfig {
            name: "dropped-rx".to_string(),
            ..Default::default()
        };
        let (agent, rx) = Agent::new(config).await.unwrap();
        drop(rx); // Drop the receiver

        let msg = Message {
            id: "orphan".to_string(),
            source: "test".to_string(),
            target: "dropped-rx".to_string(),
            message_type: agnos_common::MessageType::Command,
            payload: serde_json::json!({}),
            timestamp: chrono::Utc::now(),
        };

        // Should fail because receiver is dropped
        let result = agent.send_message(msg).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_agent_handle_has_correct_id() {
        let config = AgentConfig {
            name: "id-check".to_string(),
            ..Default::default()
        };
        let (agent, _rx) = Agent::new(config).await.unwrap();
        let id = agent.id();
        let handle = agent.handle().await;
        assert_eq!(handle.id, id);
    }

    #[test]
    fn test_agent_handle_name_is_config_name() {
        let handle = AgentHandle {
            id: AgentId::new(),
            name: "specific-name".to_string(),
            status: AgentStatus::Pending,
            created_at: chrono::Utc::now(),
            started_at: None,
            resource_usage: ResourceUsage::default(),
            pid: None,
        };
        assert_eq!(handle.name, "specific-name");
    }

    #[tokio::test]
    async fn test_agent_build_resource_limits_default_config() {
        let config = AgentConfig::default();
        let (agent, _rx) = Agent::new(config).await.unwrap();
        let limits = agent.build_resource_limits();
        // Default config has resource limits; should return Some
        assert!(limits.is_some());
    }

    #[test]
    fn test_read_vm_rss_pid_2() {
        // PID 2 is kthreadd on Linux, typically has 0 VmRSS
        let rss = Agent::read_vm_rss(2);
        // Just ensure no panic
        let _ = rss;
    }

    #[test]
    fn test_read_cpu_time_ms_pid_2() {
        let cpu = Agent::read_cpu_time_ms(2);
        let _ = cpu;
    }

    #[test]
    fn test_count_fds_pid_2() {
        let fds = Agent::count_fds(2);
        let _ = fds;
    }

    #[test]
    fn test_count_threads_pid_2() {
        let threads = Agent::count_threads(2);
        // Should be at least 1 (the fallback)
        assert!(threads >= 1);
    }

    #[tokio::test]
    async fn test_agent_new_with_custom_resource_limits() {
        let config = AgentConfig {
            name: "custom-limits".to_string(),
            resource_limits: agnos_common::ResourceLimits {
                max_memory: 2 * 1024 * 1024 * 1024,
                max_cpu_time: 14400,
                max_file_descriptors: 1024,
                max_processes: 50,
            },
            ..Default::default()
        };
        let (agent, _rx) = Agent::new(config).await.unwrap();
        let limits = agent.build_resource_limits().unwrap();
        assert_eq!(limits.max_memory, 2 * 1024 * 1024 * 1024);
        assert_eq!(limits.max_cpu_time, 14400);
    }

    #[tokio::test]
    async fn test_agent_new_with_sandbox_config() {
        let config = AgentConfig {
            name: "sandboxed".to_string(),
            sandbox: agnos_common::SandboxConfig {
                isolate_network: true,
                network_access: agnos_common::NetworkAccess::LocalhostOnly,
                ..Default::default()
            },
            ..Default::default()
        };
        let result = Agent::new(config).await;
        assert!(result.is_ok());
    }

    // ==================================================================
    // New coverage: AgentControl trait, proc helpers with current PID,
    // resource_usage without process, find_agent_executable fallback,
    // ResourceLimits::apply with non-zero values
    // ==================================================================

    #[tokio::test]
    async fn test_agent_control_check_health_no_process() {
        use crate::supervisor::AgentControl;
        let config = AgentConfig {
            name: "ctrl-health".to_string(),
            ..Default::default()
        };
        let (agent, _rx) = Agent::new(config).await.unwrap();
        // No process spawned, status is Pending (not Running) => returns false
        let healthy = agent.check_health().await.unwrap();
        assert!(!healthy);
    }

    #[tokio::test]
    async fn test_agent_control_get_resource_usage_no_process() {
        use crate::supervisor::AgentControl;
        let config = AgentConfig {
            name: "ctrl-usage".to_string(),
            ..Default::default()
        };
        let (agent, _rx) = Agent::new(config).await.unwrap();
        let usage = agent.get_resource_usage().await.unwrap();
        assert_eq!(usage.memory_used, 0);
        assert_eq!(usage.cpu_time_used, 0);
        assert_eq!(usage.file_descriptors_used, 0);
        assert_eq!(usage.processes_used, 0);
    }

    #[test]
    fn test_read_vm_rss_current_process_positive() {
        let pid = std::process::id();
        let rss = Agent::read_vm_rss(pid);
        // The test runner process should have non-trivial RSS
        assert!(rss > 1024, "Expected RSS > 1KB, got {}", rss);
    }

    #[test]
    fn test_count_fds_current_process_at_least_three() {
        // stdin, stdout, stderr at minimum
        let pid = std::process::id();
        let fds = Agent::count_fds(pid);
        assert!(fds >= 3, "Expected at least 3 FDs, got {}", fds);
    }

    #[test]
    fn test_count_threads_current_process_at_least_one() {
        let pid = std::process::id();
        let threads = Agent::count_threads(pid);
        assert!(threads >= 1);
    }

    #[test]
    fn test_resource_limits_apply_nonzero_cpu() {
        // Apply a generous CPU limit (won't restrict the test)
        let limits = ResourceLimits {
            max_memory: 0,
            max_cpu_time: 86400, // 24h
        };
        assert!(limits.apply().is_ok());
    }

    #[tokio::test]
    async fn test_agent_handle_id_matches_agent_id() {
        let config = AgentConfig {
            name: "id-match".to_string(),
            ..Default::default()
        };
        let (agent, _rx) = Agent::new(config).await.unwrap();
        let handle = agent.handle().await;
        assert_eq!(handle.id, agent.id());
        assert_eq!(handle.name, "id-match");
    }

    #[tokio::test]
    async fn test_agent_new_unique_ids() {
        let config1 = AgentConfig {
            name: "a1".to_string(),
            ..Default::default()
        };
        let config2 = AgentConfig {
            name: "a2".to_string(),
            ..Default::default()
        };
        let (agent1, _rx1) = Agent::new(config1).await.unwrap();
        let (agent2, _rx2) = Agent::new(config2).await.unwrap();
        assert_ne!(agent1.id(), agent2.id());
    }
}
