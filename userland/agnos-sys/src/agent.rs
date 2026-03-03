//! Agent SDK for building AGNOS agents
//!
//! Provides a high-level API for agent development with automatic
//! registration, resource management, and communication.

use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, info, warn};

use agnos_common::{
    AgentConfig, AgentId, AgentStatus, Message, MessageType, ResourceUsage,
};

/// Agent context passed to all agent implementations
pub struct AgentContext {
    pub id: AgentId,
    pub config: AgentConfig,
    pub status: RwLock<AgentStatus>,
    message_tx: mpsc::Sender<Message>,
}

impl AgentContext {
    /// Create a new agent context
    pub fn new(config: AgentConfig) -> (Self, mpsc::Receiver<Message>) {
        let id = AgentId::new();
        let (message_tx, message_rx) = mpsc::channel(100);
        
        let ctx = Self {
            id,
            config,
            status: RwLock::new(AgentStatus::Starting),
            message_tx,
        };
        
        (ctx, message_rx)
    }

    /// Send a message to another agent
    pub async fn send_message(&self, target: &str, payload: agnos_common::serde_json::Value) -> Result<()> {
        let message = Message {
            id: uuid::Uuid::new_v4().to_string(),
            source: self.config.name.clone(),
            target: target.to_string(),
            message_type: MessageType::Command,
            payload,
            timestamp: chrono::Utc::now(),
        };
        
        self.message_tx.send(message).await
            .map_err(|_| anyhow::anyhow!("Failed to send message"))?;
        
        Ok(())
    }

    /// Get current agent status
    pub async fn status(&self) -> AgentStatus {
        *self.status.read().await
    }

    /// Update agent status
    pub async fn set_status(&self, status: AgentStatus) {
        let mut s = self.status.write().await;
        *s = status;
        debug!("Agent {} status changed to {:?}", self.id, status);
    }
}

/// Trait that all AGNOS agents must implement
#[async_trait::async_trait]
pub trait Agent: Send + Sync {
    /// Initialize the agent
    async fn init(&mut self, ctx: &AgentContext) -> Result<()>;
    
    /// Main agent loop
    async fn run(&mut self, ctx: &AgentContext) -> Result<()>;
    
    /// Handle incoming messages
    async fn handle_message(&mut self, ctx: &AgentContext, message: Message) -> Result<()>;
    
    /// Cleanup before shutdown
    async fn shutdown(&mut self, ctx: &AgentContext) -> Result<()>;
}

/// Agent runtime for executing agents
pub struct AgentRuntime {
    ctx: Arc<AgentContext>,
    message_rx: Option<mpsc::Receiver<Message>>,
}

impl AgentRuntime {
    /// Create a new agent runtime
    pub fn new(config: AgentConfig) -> Self {
        let (ctx, message_rx) = AgentContext::new(config);
        let ctx = Arc::new(ctx);
        
        Self {
            ctx,
            message_rx: Some(message_rx),
        }
    }

    /// Run an agent implementation
    pub async fn run<A: Agent>(mut self, mut agent: A) -> Result<()> {
        info!("Starting agent runtime for {}", self.ctx.config.name);
        
        // Initialize the agent
        agent.init(&self.ctx).await
            .with_context(|| "Agent initialization failed")?;
        
        self.ctx.set_status(AgentStatus::Running).await;
        
        info!("Agent {} is running", self.ctx.config.name);
        
        // Get the message receiver if available
        let message_rx = self.message_rx.take();
        
        // Run the main agent loop with message handling
        let agent_result = self.run_message_loop(&mut agent, message_rx).await;
        
        // Cleanup
        self.ctx.set_status(AgentStatus::Stopping).await;
        agent.shutdown(&self.ctx).await?;
        self.ctx.set_status(AgentStatus::Stopped).await;
        
        info!("Agent {} stopped", self.ctx.config.name);
        
        agent_result
    }
    
    async fn run_message_loop<A: Agent>(
        &self,
        agent: &mut A,
        mut message_rx: Option<mpsc::Receiver<Message>>,
    ) -> Result<()> {
        let agent_name = self.ctx.config.name.clone();
        
        loop {
            tokio::select! {
                // Handle incoming messages
                Some(message) = async {
                    if let Some(rx) = message_rx.as_mut() {
                        rx.recv().await
                    } else {
                        None
                    }
                } => {
                    debug!("Agent {} received message: {}", agent_name, message.id);
                    
                    if let Err(e) = agent.handle_message(&self.ctx, message).await {
                        warn!("Error handling message: {}", e);
                    }
                }
                // Run the agent's main loop
                result = agent.run(&self.ctx) => {
                    result?;
                    // Agent run() returned, which means the agent wants to stop
                    break;
                }
            }
        }
        
        Ok(())
    }
}

/// Helper functions for agents
pub mod helpers {
    use super::*;
    use std::time::Duration;

    use sha2::{Digest, Sha256};

    pub const LLM_GATEWAY_ADDR: &str = "http://localhost:8088";
    const LLM_GATEWAY_TIMEOUT: Duration = Duration::from_secs(60);
    const AUDIT_LOG_PATH: &str = "/var/log/agnos/audit.log";
    const AUDIT_LOG_DIR: &str = "/var/log/agnos";

    /// Shared HTTP client — reuses connection pool across all helper calls.
    fn shared_client() -> &'static reqwest::Client {
        static CLIENT: once_cell::sync::Lazy<reqwest::Client> =
            once_cell::sync::Lazy::new(|| {
                reqwest::Client::builder()
                    .timeout(LLM_GATEWAY_TIMEOUT)
                    .pool_max_idle_per_host(4)
                    .build()
                    .unwrap_or_else(|e| {
                        tracing::error!("Failed to build reqwest client: {}, using default", e);
                        reqwest::Client::new()
                    })
            });
        &CLIENT
    }

    /// Request LLM inference through the gateway
    pub async fn llm_inference(prompt: &str, model: Option<&str>) -> Result<String> {
        let client = shared_client();
        
        let request_body = serde_json::json!({
            "model": model.unwrap_or("default"),
            "messages": [
                {"role": "user", "content": prompt}
            ],
            "max_tokens": 1024,
            "temperature": 0.7
        });
        
        let response = client
            .post(format!("{}/v1/chat/completions", LLM_GATEWAY_ADDR))
            .json(&request_body)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("LLM gateway request failed: {}", e))?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("LLM gateway error: {}", response.status()));
        }

        let response_body: serde_json::Value = response.json().await
            .map_err(|e| anyhow::anyhow!("Failed to parse LLM response: {}", e))?;

        let content = response_body["choices"]
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(|c| c["message"]["content"].as_str())
            .unwrap_or("")
            .to_string();

        debug!("LLM inference completed: {} chars", content.len());
        Ok(content)
    }

    /// Request LLM inference with full options
    pub async fn llm_inference_with_options(
        prompt: &str,
        model: Option<&str>,
        temperature: Option<f32>,
        max_tokens: Option<u32>,
    ) -> Result<String> {
        let client = shared_client();
        
        let mut request_body = serde_json::json!({
            "model": model.unwrap_or("default"),
            "messages": [
                {"role": "user", "content": prompt}
            ]
        });
        
        if let Some(temp) = temperature {
            request_body["temperature"] = serde_json::json!(temp);
        }
        if let Some(tokens) = max_tokens {
            request_body["max_tokens"] = serde_json::json!(tokens);
        }
        
        let response = client
            .post(format!("{}/v1/chat/completions", LLM_GATEWAY_ADDR))
            .json(&request_body)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("LLM gateway request failed: {}", e))?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("LLM gateway error: {}", response.status()));
        }

        let response_body: serde_json::Value = response.json().await
            .map_err(|e| anyhow::anyhow!("Failed to parse LLM response: {}", e))?;

        let content = response_body["choices"]
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(|c| c["message"]["content"].as_str())
            .unwrap_or("")
            .to_string();

        debug!("LLM inference completed: {} chars", content.len());
        Ok(content)
    }

    /// Check if LLM gateway is available
    pub async fn llm_gateway_health() -> Result<bool> {
        let client = shared_client();

        match client
            .get(format!("{}/v1/health", LLM_GATEWAY_ADDR))
            .timeout(Duration::from_secs(5))
            .send()
            .await
        {
            Ok(response) => Ok(response.status().is_success()),
            Err(_) => Ok(false),
        }
    }
    
    /// List available models from gateway
    pub async fn llm_list_models() -> Result<Vec<String>> {
        let client = shared_client();

        let response = client
            .get(format!("{}/v1/models", LLM_GATEWAY_ADDR))
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("LLM gateway request failed: {}", e))?;
        
        if !response.status().is_success() {
            return Err(anyhow::anyhow!("LLM gateway error: {}", response.status()));
        }
        
        let response_body: serde_json::Value = response.json().await
            .map_err(|e| anyhow::anyhow!("Failed to parse LLM response: {}", e))?;
        
        let models: Vec<String> = response_body["data"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|m| m["id"].as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        
        Ok(models)
    }
    
    /// Log an audit event to `/var/log/agnos/audit.log` with cryptographic hash chain.
    ///
    /// Each entry is a JSON line containing the event data plus a SHA-256 hash
    /// that chains to the previous entry, providing tamper evidence.  The file
    /// is created (with directory) if it doesn't exist.  Writes are appended
    /// atomically with a file lock.
    pub async fn audit_log(event_type: &str, details: agnos_common::serde_json::Value) -> Result<()> {
        debug!("Audit log: {} - {:?}", event_type, details);

        // Build the log entry
        let timestamp = chrono::Utc::now().to_rfc3339();
        let previous_hash = read_last_hash().unwrap_or_else(|| "genesis".to_string());

        // Hash = SHA-256(previous_hash || timestamp || event_type || details)
        let mut hasher = Sha256::new();
        hasher.update(previous_hash.as_bytes());
        hasher.update(timestamp.as_bytes());
        hasher.update(event_type.as_bytes());
        hasher.update(details.to_string().as_bytes());
        let hash = format!("{:x}", hasher.finalize());

        let entry = serde_json::json!({
            "timestamp": timestamp,
            "event_type": event_type,
            "details": details,
            "previous_hash": previous_hash,
            "hash": hash,
        });

        // Ensure the directory exists
        if let Err(e) = std::fs::create_dir_all(AUDIT_LOG_DIR) {
            warn!("Could not create audit log directory {}: {}", AUDIT_LOG_DIR, e);
            // Still log to debug as fallback
            return Ok(());
        }

        // Append atomically with exclusive file lock
        use std::io::Write;
        match std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(AUDIT_LOG_PATH)
        {
            Ok(mut file) => {
                // Use advisory lock for concurrent writers
                #[cfg(unix)]
                {
                    use std::os::unix::io::AsRawFd;
                    let fd = file.as_raw_fd();
                    unsafe {
                        libc::flock(fd, libc::LOCK_EX);
                    }
                }

                let line = format!("{}\n", entry);
                if let Err(e) = file.write_all(line.as_bytes()) {
                    warn!("Failed to write audit log: {}", e);
                }

                #[cfg(unix)]
                {
                    use std::os::unix::io::AsRawFd;
                    let fd = file.as_raw_fd();
                    unsafe {
                        libc::flock(fd, libc::LOCK_UN);
                    }
                }
            }
            Err(e) => {
                warn!("Could not open audit log {}: {}", AUDIT_LOG_PATH, e);
            }
        }

        Ok(())
    }

    /// Read the hash from the last line of the audit log (for chaining).
    fn read_last_hash() -> Option<String> {
        let contents = std::fs::read_to_string(AUDIT_LOG_PATH).ok()?;
        let last_line = contents.lines().rev().find(|l| !l.trim().is_empty())?;
        let entry: serde_json::Value = serde_json::from_str(last_line).ok()?;
        entry["hash"].as_str().map(String::from)
    }

    /// Check resource usage for the current process by reading from `/proc/self/`.
    pub async fn check_resources() -> ResourceUsage {
        let pid = std::process::id();

        let memory_used = read_vm_rss(pid);
        let cpu_time_used = read_cpu_time_ms(pid);
        let file_descriptors_used = count_fds(pid);
        let processes_used = count_threads(pid);

        ResourceUsage {
            memory_used,
            cpu_time_used,
            file_descriptors_used,
            processes_used,
        }
    }

    fn read_vm_rss(pid: u32) -> u64 {
        let path = format!("/proc/{}/status", pid);
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|contents| {
                for line in contents.lines() {
                    if let Some(val) = line.strip_prefix("VmRSS:") {
                        let kb: u64 = val.trim().split_whitespace().next()?.parse().ok()?;
                        return Some(kb * 1024);
                    }
                }
                None
            })
            .unwrap_or(0)
    }

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
                    Some(ticks * 10)
                }
            })
            .unwrap_or(0)
    }

    fn count_fds(pid: u32) -> u32 {
        let path = format!("/proc/{}/fd", pid);
        std::fs::read_dir(&path)
            .map(|entries| entries.count() as u32)
            .unwrap_or(0)
    }

    fn count_threads(pid: u32) -> u32 {
        let path = format!("/proc/{}/task", pid);
        std::fs::read_dir(&path)
            .map(|entries| entries.count() as u32)
            .unwrap_or(1)
    }
}

/// Macros for agent development
#[macro_export]
macro_rules! agent_main {
    ($agent_type:ty) => {
        #[tokio::main]
        async fn main() -> anyhow::Result<()> {
            use agnos_sys::agent::{AgentRuntime};
            
            tracing_subscriber::fmt::init();
            
            // Load configuration from environment or defaults
            let config = agnos_common::AgentConfig {
                name: env!("CARGO_PKG_NAME").to_string(),
                agent_type: agnos_common::AgentType::Service,
                ..Default::default()
            };
            
            let runtime = AgentRuntime::new(config);
            let agent = <$agent_type>::new()?;
            
            runtime.run(agent).await
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;
    use agnos_common::{AgentConfig, AgentId, AgentStatus, Message, MessageType};
    
    #[test]
    fn test_agent_context_new() {
        let config = AgentConfig::default();
        let (ctx, rx) = AgentContext::new(config);
        
        assert_eq!(*ctx.status.blocking_read(), AgentStatus::Starting);
        // Receiver is returned separately, not stored in context
        drop(rx);
    }
    
    #[test]
    fn test_agent_runtime_new() {
        let config = AgentConfig::default();
        let _runtime = AgentRuntime::new(config);
        // Runtime should be created without panicking
    }
    
    #[tokio::test]
    async fn test_agent_context_send_message() {
        let config = AgentConfig::default();
        let (ctx, rx) = AgentContext::new(config);
        
        // Drop the receiver so the send will fail
        drop(rx);
        
        let result = ctx.send_message("target", serde_json::json!({"test": true})).await;
        // This should fail because the receiver has been dropped
        assert!(result.is_err());
    }
    
    #[tokio::test]
    async fn test_agent_context_status() {
        let config = AgentConfig::default();
        let (ctx, _rx) = AgentContext::new(config);
        
        ctx.set_status(AgentStatus::Running).await;
        assert_eq!(ctx.status().await, AgentStatus::Running);
        
        ctx.set_status(AgentStatus::Stopped).await;
        assert_eq!(ctx.status().await, AgentStatus::Stopped);
    }
    
    #[tokio::test]
    async fn test_llm_gateway_constants() {
        // Verify the gateway address and port are correct
        assert_eq!(helpers::LLM_GATEWAY_ADDR, "http://localhost:8088");
    }
}
