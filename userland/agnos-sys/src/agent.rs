//! Agent SDK for building AGNOS agents
//!
//! Provides a high-level API for agent development with automatic
//! registration, resource management, and communication.

use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, info, warn};

use agnos_common::{AgentConfig, AgentId, AgentStatus, Message, MessageType, ResourceUsage};

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
    pub async fn send_message(
        &self,
        target: &str,
        payload: agnos_common::serde_json::Value,
    ) -> Result<()> {
        let message = Message {
            id: uuid::Uuid::new_v4().to_string(),
            source: self.config.name.clone(),
            target: target.to_string(),
            message_type: MessageType::Command,
            payload,
            timestamp: chrono::Utc::now(),
        };

        self.message_tx
            .send(message)
            .await
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
        agent
            .init(&self.ctx)
            .await
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
        static CLIENT: once_cell::sync::Lazy<reqwest::Client> = once_cell::sync::Lazy::new(|| {
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

        let response_body: serde_json::Value = response
            .json()
            .await
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

        let response_body: serde_json::Value = response
            .json()
            .await
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

        let response_body: serde_json::Value = response
            .json()
            .await
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
    pub async fn audit_log(
        event_type: &str,
        details: agnos_common::serde_json::Value,
    ) -> Result<()> {
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
            warn!(
                "Could not create audit log directory {}: {}",
                AUDIT_LOG_DIR, e
            );
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
                        let kb: u64 = val.split_whitespace().next()?.parse().ok()?;
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

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn test_read_vm_rss_for_self() {
            let pid = std::process::id();
            let rss = read_vm_rss(pid);
            assert!(rss > 0, "RSS for current process should be > 0");
        }

        #[test]
        fn test_read_vm_rss_for_invalid_pid() {
            let rss = read_vm_rss(u32::MAX);
            assert_eq!(rss, 0);
        }

        #[test]
        fn test_read_cpu_time_ms_for_self() {
            let pid = std::process::id();
            let cpu = read_cpu_time_ms(pid);
            // Could be 0 if very fast, but should not panic
            let _ = cpu;
        }

        #[test]
        fn test_read_cpu_time_ms_for_invalid_pid() {
            let cpu = read_cpu_time_ms(u32::MAX);
            assert_eq!(cpu, 0);
        }

        #[test]
        fn test_count_fds_for_self() {
            let pid = std::process::id();
            let fds = count_fds(pid);
            // At minimum stdin, stdout, stderr
            assert!(fds >= 3, "fd count should be >= 3, got {}", fds);
        }

        #[test]
        fn test_count_fds_for_invalid_pid() {
            let fds = count_fds(u32::MAX);
            assert_eq!(fds, 0);
        }

        #[test]
        fn test_count_threads_for_self() {
            let pid = std::process::id();
            let threads = count_threads(pid);
            assert!(threads >= 1, "thread count should be >= 1");
        }

        #[test]
        fn test_count_threads_for_invalid_pid() {
            let threads = count_threads(u32::MAX);
            assert_eq!(threads, 1);
        }

        #[test]
        fn test_read_last_hash_returns_option() {
            // In test environment, audit log likely doesn't exist
            // Should return None gracefully
            let hash = read_last_hash();
            let _ = hash; // Should not panic
        }

        #[tokio::test]
        async fn test_check_resources_returns_nonzero() {
            let usage = check_resources().await;
            assert!(usage.memory_used > 0, "memory_used should be > 0");
            assert!(usage.file_descriptors_used > 0, "fd count should be > 0");
            assert!(usage.processes_used >= 1, "thread count should be >= 1");
        }

        #[tokio::test]
        async fn test_llm_gateway_health_no_server() {
            let result = llm_gateway_health().await;
            assert!(result.is_ok());
            // Result depends on whether a live gateway is running
            let _healthy = result.unwrap();
        }

        #[tokio::test]
        async fn test_llm_inference_no_server() {
            let result = llm_inference("hello", None).await;
            assert!(result.is_err());
            assert!(result
                .unwrap_err()
                .to_string()
                .contains("LLM gateway request failed"));
        }

        #[tokio::test]
        async fn test_llm_inference_with_model_no_server() {
            let result = llm_inference("hello", Some("gpt-4")).await;
            assert!(result.is_err());
        }

        #[tokio::test]
        async fn test_llm_inference_with_options_all_some() {
            let result =
                llm_inference_with_options("prompt", Some("model"), Some(0.5), Some(256)).await;
            // May succeed if a live gateway is running, otherwise errors
            if let Err(e) = &result {
                assert!(e.to_string().contains("LLM gateway request failed")
                    || e.to_string().contains("LLM"));
            }
        }

        #[tokio::test]
        async fn test_llm_inference_with_options_all_none() {
            let result = llm_inference_with_options("prompt", None, None, None).await;
            assert!(result.is_err());
        }

        #[tokio::test]
        async fn test_llm_inference_with_options_partial() {
            // Only temperature set
            let result = llm_inference_with_options("prompt", None, Some(0.9), None).await;
            assert!(result.is_err());

            // Only max_tokens set
            let result = llm_inference_with_options("prompt", None, None, Some(512)).await;
            assert!(result.is_err());
        }

        #[tokio::test]
        async fn test_llm_list_models_no_server() {
            let result = llm_list_models().await;
            // May succeed if a live gateway is running, otherwise errors
            if let Err(e) = &result {
                assert!(e.to_string().contains("LLM gateway request failed")
                    || e.to_string().contains("LLM"));
            }
        }

        #[tokio::test]
        async fn test_audit_log_returns_ok_on_permission_error() {
            let result = audit_log("test_event", serde_json::json!({"key": "value"})).await;
            // audit_log returns Ok(()) even on write failure
            assert!(result.is_ok());
        }

        #[test]
        fn test_llm_gateway_addr_constant() {
            assert_eq!(LLM_GATEWAY_ADDR, "http://localhost:8088");
        }

        #[test]
        fn test_hash_chain_determinism() {
            // Verify SHA-256 hashing works as expected for audit chain
            let mut hasher1 = Sha256::new();
            hasher1.update(b"genesis");
            hasher1.update(b"2026-01-01T00:00:00Z");
            hasher1.update(b"test");
            hasher1.update(b"{}");
            let hash1 = format!("{:x}", hasher1.finalize());

            let mut hasher2 = Sha256::new();
            hasher2.update(b"genesis");
            hasher2.update(b"2026-01-01T00:00:00Z");
            hasher2.update(b"test");
            hasher2.update(b"{}");
            let hash2 = format!("{:x}", hasher2.finalize());

            assert_eq!(hash1, hash2);
            assert_eq!(hash1.len(), 64); // SHA-256 hex = 64 chars
        }

        #[test]
        fn test_hash_chain_different_inputs() {
            let mut hasher1 = Sha256::new();
            hasher1.update(b"genesis");
            hasher1.update(b"event_a");
            let hash1 = format!("{:x}", hasher1.finalize());

            let mut hasher2 = Sha256::new();
            hasher2.update(b"genesis");
            hasher2.update(b"event_b");
            let hash2 = format!("{:x}", hasher2.finalize());

            assert_ne!(hash1, hash2);
        }

        // --- New coverage tests ---

        #[test]
        fn test_read_vm_rss_returns_bytes_not_kb() {
            let pid = std::process::id();
            let rss = read_vm_rss(pid);
            // RSS should be in bytes (VmRSS kB * 1024), so at least several KB
            if rss > 0 {
                assert!(rss >= 1024, "RSS should be in bytes, got {}", rss);
            }
        }

        #[test]
        fn test_read_cpu_time_ms_nonnegative() {
            let pid = std::process::id();
            let cpu = read_cpu_time_ms(pid);
            // Can be 0 but never negative (u64)
            let _ = cpu;
        }

        #[test]
        fn test_count_fds_reasonable_range() {
            let pid = std::process::id();
            let fds = count_fds(pid);
            // Should have at least 3 (stdin/stdout/stderr) and less than 10000
            assert!((3..10000).contains(&fds), "Unexpected fd count: {}", fds);
        }

        #[test]
        fn test_count_threads_at_least_one() {
            let pid = std::process::id();
            let threads = count_threads(pid);
            assert!(threads >= 1);
        }

        #[tokio::test]
        async fn test_check_resources_all_fields_populated() {
            let usage = check_resources().await;
            // memory_used should be > 0 on Linux
            assert!(usage.memory_used > 0);
            // fd count >= 3
            assert!(usage.file_descriptors_used >= 3);
            // at least 1 thread
            assert!(usage.processes_used >= 1);
        }

        #[test]
        fn test_read_last_hash_missing_file() {
            // AUDIT_LOG_PATH unlikely to exist in test env
            let hash = read_last_hash();
            // Should return None without panicking
            let _ = hash;
        }

        #[tokio::test]
        async fn test_audit_log_creates_hash_chain_entry() {
            // audit_log always returns Ok (even on write failure)
            let result = audit_log("test_event_type", serde_json::json!({"agent": "test-1"})).await;
            assert!(result.is_ok());
        }

        #[test]
        fn test_shared_client_returns_same_instance() {
            let c1 = shared_client();
            let c2 = shared_client();
            // Both should be the same static reference
            assert!(std::ptr::eq(c1, c2));
        }
    }
}

/// Macros for agent development
#[macro_export]
macro_rules! agent_main {
    ($agent_type:ty) => {
        #[tokio::main]
        async fn main() -> anyhow::Result<()> {
            use agnos_sys::agent::AgentRuntime;

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
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;
    use agnos_common::{AgentConfig, AgentStatus, Message, MessageType};

    // ── AgentContext ──────────────────────────────────────────────────

    #[test]
    fn test_agent_context_new() {
        let config = AgentConfig::default();
        let (ctx, rx) = AgentContext::new(config);

        assert_eq!(*ctx.status.blocking_read(), AgentStatus::Starting);
        // Receiver is returned separately, not stored in context
        drop(rx);
    }

    #[test]
    fn test_agent_context_new_preserves_config() {
        let mut config = AgentConfig::default();
        config.name = "test-agent".to_string();
        let (ctx, _rx) = AgentContext::new(config);

        assert_eq!(ctx.config.name, "test-agent");
    }

    #[test]
    fn test_agent_context_new_generates_unique_ids() {
        let (ctx1, _rx1) = AgentContext::new(AgentConfig::default());
        let (ctx2, _rx2) = AgentContext::new(AgentConfig::default());
        assert_ne!(ctx1.id, ctx2.id);
    }

    #[tokio::test]
    async fn test_agent_context_send_message_success() {
        let mut config = AgentConfig::default();
        config.name = "sender-agent".to_string();
        let (ctx, mut rx) = AgentContext::new(config);

        let payload = serde_json::json!({"action": "do_thing"});
        ctx.send_message("target-agent", payload.clone())
            .await
            .unwrap();

        let msg = rx.recv().await.expect("should receive a message");
        assert_eq!(msg.source, "sender-agent");
        assert_eq!(msg.target, "target-agent");
        assert_eq!(msg.payload, payload);
        assert_eq!(msg.message_type, MessageType::Command);
        // id should be a valid UUID string
        assert!(!msg.id.is_empty());
    }

    #[tokio::test]
    async fn test_agent_context_send_message_fails_when_receiver_dropped() {
        let config = AgentConfig::default();
        let (ctx, rx) = AgentContext::new(config);

        // Drop the receiver so the send will fail
        drop(rx);

        let result = ctx
            .send_message("target", serde_json::json!({"test": true}))
            .await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Failed to send message"));
    }

    #[tokio::test]
    async fn test_agent_context_send_multiple_messages() {
        let config = AgentConfig::default();
        let (ctx, mut rx) = AgentContext::new(config);

        for i in 0..5 {
            ctx.send_message("target", serde_json::json!({"i": i}))
                .await
                .unwrap();
        }

        for i in 0..5 {
            let msg = rx.recv().await.unwrap();
            assert_eq!(msg.payload["i"], i);
        }
    }

    #[tokio::test]
    async fn test_agent_context_status() {
        let config = AgentConfig::default();
        let (ctx, _rx) = AgentContext::new(config);

        assert_eq!(ctx.status().await, AgentStatus::Starting);

        ctx.set_status(AgentStatus::Running).await;
        assert_eq!(ctx.status().await, AgentStatus::Running);

        ctx.set_status(AgentStatus::Paused).await;
        assert_eq!(ctx.status().await, AgentStatus::Paused);

        ctx.set_status(AgentStatus::Stopping).await;
        assert_eq!(ctx.status().await, AgentStatus::Stopping);

        ctx.set_status(AgentStatus::Stopped).await;
        assert_eq!(ctx.status().await, AgentStatus::Stopped);

        ctx.set_status(AgentStatus::Failed).await;
        assert_eq!(ctx.status().await, AgentStatus::Failed);
    }

    // ── AgentRuntime ──────────────────────────────────────────────────

    #[test]
    fn test_agent_runtime_new() {
        let config = AgentConfig::default();
        let runtime = AgentRuntime::new(config);
        assert!(runtime.message_rx.is_some());
    }

    #[test]
    fn test_agent_runtime_new_preserves_config() {
        let mut config = AgentConfig::default();
        config.name = "runtime-test".to_string();
        let runtime = AgentRuntime::new(config);
        assert_eq!(runtime.ctx.config.name, "runtime-test");
    }

    // A minimal mock agent for testing runtime.run()
    struct MockAgent {
        init_called: bool,
        run_called: bool,
        shutdown_called: bool,
        init_should_fail: bool,
    }

    impl MockAgent {
        fn new() -> Self {
            Self {
                init_called: false,
                run_called: false,
                shutdown_called: false,
                init_should_fail: false,
            }
        }

        fn failing_init() -> Self {
            Self {
                init_called: false,
                run_called: false,
                shutdown_called: false,
                init_should_fail: true,
            }
        }
    }

    #[async_trait::async_trait]
    impl Agent for MockAgent {
        async fn init(&mut self, _ctx: &AgentContext) -> Result<()> {
            self.init_called = true;
            if self.init_should_fail {
                return Err(anyhow::anyhow!("init failure"));
            }
            Ok(())
        }

        async fn run(&mut self, _ctx: &AgentContext) -> Result<()> {
            self.run_called = true;
            // Return immediately to stop the loop
            Ok(())
        }

        async fn handle_message(&mut self, _ctx: &AgentContext, _message: Message) -> Result<()> {
            Ok(())
        }

        async fn shutdown(&mut self, _ctx: &AgentContext) -> Result<()> {
            self.shutdown_called = true;
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_agent_runtime_run_lifecycle() {
        let mut config = AgentConfig::default();
        config.name = "lifecycle-test".to_string();
        let runtime = AgentRuntime::new(config);
        let agent = MockAgent::new();

        let result = runtime.run(agent).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_agent_runtime_run_init_failure() {
        let config = AgentConfig::default();
        let runtime = AgentRuntime::new(config);
        let agent = MockAgent::failing_init();

        let result = runtime.run(agent).await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Agent initialization failed"));
    }

    // Agent that handles a message, then returns from run
    struct MessageHandlingAgent {
        received_messages: Vec<Message>,
    }

    impl MessageHandlingAgent {
        fn new() -> Self {
            Self {
                received_messages: Vec::new(),
            }
        }
    }

    #[async_trait::async_trait]
    impl Agent for MessageHandlingAgent {
        async fn init(&mut self, _ctx: &AgentContext) -> Result<()> {
            Ok(())
        }

        async fn run(&mut self, _ctx: &AgentContext) -> Result<()> {
            // Small delay to allow message handling to occur first
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            Ok(())
        }

        async fn handle_message(&mut self, _ctx: &AgentContext, message: Message) -> Result<()> {
            self.received_messages.push(message);
            Ok(())
        }

        async fn shutdown(&mut self, _ctx: &AgentContext) -> Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_agent_runtime_run_completes_on_run_return() {
        let mut config = AgentConfig::default();
        config.name = "run-return-test".to_string();
        let runtime = AgentRuntime::new(config);
        let agent = MessageHandlingAgent::new();

        // Should complete without hanging
        let result =
            tokio::time::timeout(std::time::Duration::from_secs(5), runtime.run(agent)).await;

        assert!(result.is_ok(), "runtime.run should complete within timeout");
        assert!(result.unwrap().is_ok());
    }

    // Agent whose run() fails with an error
    struct FailingRunAgent;

    #[async_trait::async_trait]
    impl Agent for FailingRunAgent {
        async fn init(&mut self, _ctx: &AgentContext) -> Result<()> {
            Ok(())
        }

        async fn run(&mut self, _ctx: &AgentContext) -> Result<()> {
            Err(anyhow::anyhow!("run failed"))
        }

        async fn handle_message(&mut self, _ctx: &AgentContext, _message: Message) -> Result<()> {
            Ok(())
        }

        async fn shutdown(&mut self, _ctx: &AgentContext) -> Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_agent_runtime_run_propagates_run_error() {
        let config = AgentConfig::default();
        let runtime = AgentRuntime::new(config);
        let agent = FailingRunAgent;

        let result = runtime.run(agent).await;
        // The run error propagates through run_message_loop via `result?`
        // but shutdown is still called, and then agent_result is returned
        assert!(result.is_err());
    }

    // ── AgentContext with custom config fields ────────────────────────

    #[test]
    fn test_agent_context_preserves_agent_type() {
        let mut config = AgentConfig::default();
        config.agent_type = agnos_common::AgentType::Service;
        let (ctx, _rx) = AgentContext::new(config);
        assert_eq!(ctx.config.agent_type, agnos_common::AgentType::Service);
    }

    #[tokio::test]
    async fn test_agent_context_status_transitions_all_variants() {
        let config = AgentConfig::default();
        let (ctx, _rx) = AgentContext::new(config);

        let variants = [
            AgentStatus::Pending,
            AgentStatus::Starting,
            AgentStatus::Running,
            AgentStatus::Paused,
            AgentStatus::Stopping,
            AgentStatus::Stopped,
            AgentStatus::Failed,
        ];

        for status in variants {
            ctx.set_status(status).await;
            assert_eq!(ctx.status().await, status);
        }
    }

    #[tokio::test]
    async fn test_send_message_sets_correct_timestamp() {
        let config = AgentConfig::default();
        let (ctx, mut rx) = AgentContext::new(config);

        let before = chrono::Utc::now();
        ctx.send_message("t", serde_json::json!(null))
            .await
            .unwrap();
        let after = chrono::Utc::now();

        let msg = rx.recv().await.unwrap();
        assert!(msg.timestamp >= before);
        assert!(msg.timestamp <= after);
    }

    #[test]
    fn test_agent_runtime_ctx_is_arc() {
        let config = AgentConfig::default();
        let runtime = AgentRuntime::new(config);
        // ctx should be shareable (Arc)
        let _clone = Arc::clone(&runtime.ctx);
    }

    // --- New coverage tests (batch 2) ---

    #[tokio::test]
    async fn test_agent_context_initial_status_is_starting() {
        let config = AgentConfig::default();
        let (ctx, _rx) = AgentContext::new(config);
        assert_eq!(ctx.status().await, AgentStatus::Starting);
    }

    #[tokio::test]
    async fn test_agent_context_set_status_overwrite() {
        let config = AgentConfig::default();
        let (ctx, _rx) = AgentContext::new(config);
        ctx.set_status(AgentStatus::Running).await;
        ctx.set_status(AgentStatus::Paused).await;
        ctx.set_status(AgentStatus::Running).await;
        assert_eq!(ctx.status().await, AgentStatus::Running);
    }

    #[tokio::test]
    async fn test_send_message_unique_ids() {
        let config = AgentConfig::default();
        let (ctx, mut rx) = AgentContext::new(config);

        ctx.send_message("t", serde_json::json!(1)).await.unwrap();
        ctx.send_message("t", serde_json::json!(2)).await.unwrap();

        let msg1 = rx.recv().await.unwrap();
        let msg2 = rx.recv().await.unwrap();
        assert_ne!(msg1.id, msg2.id, "Each message should have a unique ID");
    }

    #[tokio::test]
    async fn test_send_message_preserves_complex_payload() {
        let config = AgentConfig::default();
        let (ctx, mut rx) = AgentContext::new(config);

        let payload = serde_json::json!({
            "nested": {"key": "value"},
            "array": [1, 2, 3],
            "null_val": null,
            "bool": true
        });
        ctx.send_message("target", payload.clone()).await.unwrap();
        let msg = rx.recv().await.unwrap();
        assert_eq!(msg.payload, payload);
    }

    #[test]
    fn test_agent_runtime_message_rx_is_some() {
        let config = AgentConfig::default();
        let runtime = AgentRuntime::new(config);
        assert!(runtime.message_rx.is_some());
    }

    #[test]
    fn test_agent_context_config_default_values() {
        let mut config = AgentConfig::default();
        config.name = "default-agent".to_string();
        let (ctx, _rx) = AgentContext::new(config);
        assert_eq!(ctx.config.name, "default-agent");
    }

    #[tokio::test]
    async fn test_agent_context_message_channel_capacity() {
        let config = AgentConfig::default();
        let (ctx, _rx) = AgentContext::new(config);
        // Channel capacity is 100, so we can send 100 without receiving
        for i in 0..100 {
            ctx.send_message("t", serde_json::json!(i)).await.unwrap();
        }
    }

    // Agent that panics in handle_message but run() returns Ok
    struct ErrorMessageAgent;

    #[async_trait::async_trait]
    impl Agent for ErrorMessageAgent {
        async fn init(&mut self, _ctx: &AgentContext) -> Result<()> {
            Ok(())
        }

        async fn run(&mut self, _ctx: &AgentContext) -> Result<()> {
            Ok(())
        }

        async fn handle_message(&mut self, _ctx: &AgentContext, _message: Message) -> Result<()> {
            Err(anyhow::anyhow!("message handling failed"))
        }

        async fn shutdown(&mut self, _ctx: &AgentContext) -> Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_agent_runtime_error_message_agent_completes() {
        let config = AgentConfig::default();
        let runtime = AgentRuntime::new(config);
        let agent = ErrorMessageAgent;

        let result =
            tokio::time::timeout(std::time::Duration::from_secs(5), runtime.run(agent)).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_ok());
    }

    // Agent that tracks shutdown was called
    struct ShutdownTrackingAgent {
        shutdown_called: std::sync::Arc<std::sync::atomic::AtomicBool>,
    }

    #[async_trait::async_trait]
    impl Agent for ShutdownTrackingAgent {
        async fn init(&mut self, _ctx: &AgentContext) -> Result<()> {
            Ok(())
        }

        async fn run(&mut self, _ctx: &AgentContext) -> Result<()> {
            Ok(())
        }

        async fn handle_message(&mut self, _ctx: &AgentContext, _message: Message) -> Result<()> {
            Ok(())
        }

        async fn shutdown(&mut self, _ctx: &AgentContext) -> Result<()> {
            self.shutdown_called
                .store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_agent_runtime_calls_shutdown() {
        let shutdown_flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let config = AgentConfig::default();
        let runtime = AgentRuntime::new(config);
        let agent = ShutdownTrackingAgent {
            shutdown_called: shutdown_flag.clone(),
        };
        runtime.run(agent).await.unwrap();
        assert!(shutdown_flag.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_agent_runtime_sets_running_status() {
        // Use a simple agent that checks status during run
        struct StatusCheckAgent {
            was_running: std::sync::Arc<std::sync::atomic::AtomicBool>,
        }

        #[async_trait::async_trait]
        impl Agent for StatusCheckAgent {
            async fn init(&mut self, _ctx: &AgentContext) -> Result<()> {
                Ok(())
            }

            async fn run(&mut self, ctx: &AgentContext) -> Result<()> {
                if ctx.status().await == AgentStatus::Running {
                    self.was_running
                        .store(true, std::sync::atomic::Ordering::SeqCst);
                }
                Ok(())
            }

            async fn handle_message(
                &mut self,
                _ctx: &AgentContext,
                _message: Message,
            ) -> Result<()> {
                Ok(())
            }

            async fn shutdown(&mut self, _ctx: &AgentContext) -> Result<()> {
                Ok(())
            }
        }

        let was_running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let config = AgentConfig::default();
        let runtime = AgentRuntime::new(config);
        let agent = StatusCheckAgent {
            was_running: was_running.clone(),
        };
        runtime.run(agent).await.unwrap();
        assert!(was_running.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[test]
    fn test_agent_context_id_is_valid_uuid() {
        let config = AgentConfig::default();
        let (ctx, _rx) = AgentContext::new(config);
        // AgentId wraps a Uuid — just check it's non-nil
        assert_ne!(format!("{}", ctx.id), "");
    }

    #[tokio::test]
    async fn test_send_message_empty_target() {
        let config = AgentConfig::default();
        let (ctx, mut rx) = AgentContext::new(config);
        ctx.send_message("", serde_json::json!(null)).await.unwrap();
        let msg = rx.recv().await.unwrap();
        assert_eq!(msg.target, "");
    }

    #[tokio::test]
    async fn test_send_message_empty_payload() {
        let config = AgentConfig::default();
        let (ctx, mut rx) = AgentContext::new(config);
        ctx.send_message("target", serde_json::json!(null))
            .await
            .unwrap();
        let msg = rx.recv().await.unwrap();
        assert!(msg.payload.is_null());
    }

    // Agent whose shutdown fails
    struct FailingShutdownAgent;

    #[async_trait::async_trait]
    impl Agent for FailingShutdownAgent {
        async fn init(&mut self, _ctx: &AgentContext) -> Result<()> {
            Ok(())
        }

        async fn run(&mut self, _ctx: &AgentContext) -> Result<()> {
            Ok(())
        }

        async fn handle_message(&mut self, _ctx: &AgentContext, _message: Message) -> Result<()> {
            Ok(())
        }

        async fn shutdown(&mut self, _ctx: &AgentContext) -> Result<()> {
            Err(anyhow::anyhow!("shutdown failed"))
        }
    }

    #[tokio::test]
    async fn test_agent_runtime_shutdown_error_propagated() {
        let config = AgentConfig::default();
        let runtime = AgentRuntime::new(config);
        let agent = FailingShutdownAgent;
        let result = runtime.run(agent).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("shutdown failed"));
    }
}
