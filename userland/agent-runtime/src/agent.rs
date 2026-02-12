//! Agent representation and lifecycle management

use std::path::PathBuf;
use std::process::Stdio;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, oneshot, RwLock};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use agnos_common::{
    AgentConfig, AgentId, AgentStatus, AgentType, Message, ResourceUsage, StopReason,
};

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
}

/// Represents a running agent process
pub struct Agent {
    id: AgentId,
    config: AgentConfig,
    status: RwLock<AgentStatus>,
    process: Option<Child>,
    ipc: Option<AgentIpc>,
    sandbox: Sandbox,
    started_at: Option<Instant>,
    message_tx: mpsc::Sender<Message>,
    message_rx: Option<mpsc::Receiver<Message>>,
}

impl Agent {
    /// Create a new agent from configuration
    pub async fn new(config: AgentConfig) -> Result<(Self, mpsc::Receiver<Message>)> {
        let id = AgentId::new();
        let (message_tx, message_rx) = mpsc::channel(100);

        let sandbox = Sandbox::new(&config.sandbox)
            .with_context(|| "Failed to create agent sandbox")?;

        let agent = Self {
            id,
            config,
            status: RwLock::new(AgentStatus::Pending),
            process: None,
            ipc: None,
            sandbox,
            started_at: None,
            message_tx,
            message_rx: None,
        };

        Ok((agent, message_rx))
    }

    /// Get agent ID
    pub fn id(&self) -> AgentId {
        self.id
    }

    /// Get agent handle for external reference
    pub async fn handle(&self) -> AgentHandle {
        AgentHandle {
            id: self.id,
            name: self.config.name.clone(),
            status: *self.status.read().await,
            created_at: chrono::Utc::now(),
            started_at: None,
            resource_usage: ResourceUsage::default(),
        }
    }

    /// Start the agent process
    pub async fn start(&mut self) -> Result<()> {
        let mut status = self.status.write().await;
        
        if *status != AgentStatus::Pending && *status != AgentStatus::Stopped {
            return Err(anyhow::anyhow!("Agent is not in a startable state: {:?}", *status));
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

        let mut child = cmd.spawn()
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

    /// Pause the agent (suspend execution)
    pub async fn pause(&mut self) -> Result<()> {
        let mut status = self.status.write().await;
        if *status != AgentStatus::Running {
            return Err(anyhow::anyhow!("Cannot pause agent in state: {:?}", *status));
        }

        *status = AgentStatus::Paused;
        info!("Agent {} paused", self.id);
        
        // TODO: Implement actual process suspension
        
        Ok(())
    }

    /// Resume a paused agent
    pub async fn resume(&mut self) -> Result<()> {
        let mut status = self.status.write().await;
        if *status != AgentStatus::Paused {
            return Err(anyhow::anyhow!("Cannot resume agent in state: {:?}", *status));
        }

        *status = AgentStatus::Running;
        info!("Agent {} resumed", self.id);
        
        // TODO: Implement actual process resumption
        
        Ok(())
    }

    /// Get current resource usage
    pub async fn resource_usage(&self) -> ResourceUsage {
        // TODO: Implement actual resource monitoring
        ResourceUsage::default()
    }

    /// Send a message to the agent
    pub async fn send_message(&self, message: Message) -> Result<()> {
        self.message_tx.send(message).await
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
