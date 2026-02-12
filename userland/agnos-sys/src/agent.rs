//! Agent SDK for building AGNOS agents
//!
//! Provides a high-level API for agent development with automatic
//! registration, resource management, and communication.

use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, info};

use agnos_common::{
    AgentConfig, AgentId, AgentStatus, Message, MessageType, ResourceUsage,
};

/// Agent context passed to all agent implementations
pub struct AgentContext {
    pub id: AgentId,
    pub config: AgentConfig,
    pub status: RwLock<AgentStatus>,
    message_tx: mpsc::Sender<Message>,
    message_rx: Option<mpsc::Receiver<Message>>,
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
            message_rx: None,
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
    pub fn new(config: AgentConfig) -> (Self, mpsc::Receiver<Message>) {
        let (ctx, message_rx) = AgentContext::new(config);
        let ctx = Arc::new(ctx);
        
        let runtime = Self {
            ctx,
            message_rx: None,
        };
        
        (runtime, message_rx)
    }

    /// Run an agent implementation
    pub async fn run<A: Agent>(self, mut agent: A) -> Result<()> {
        info!("Starting agent runtime for {}", self.ctx.config.name);
        
        // Initialize the agent
        agent.init(&self.ctx).await
            .with_context(|| "Agent initialization failed")?;
        
        self.ctx.set_status(AgentStatus::Running).await;
        
        info!("Agent {} is running", self.ctx.config.name);
        
        // TODO: Implement message loop and agent lifecycle
        
        // Run the agent
        agent.run(&self.ctx).await?;
        
        // Cleanup
        self.ctx.set_status(AgentStatus::Stopping).await;
        agent.shutdown(&self.ctx).await?;
        self.ctx.set_status(AgentStatus::Stopped).await;
        
        info!("Agent {} stopped", self.ctx.config.name);
        
        Ok(())
    }
}

/// Helper functions for agents
pub mod helpers {
    use super::*;
    
    /// Request LLM inference through the gateway
    pub async fn llm_inference(prompt: &str, model: Option<&str>) -> Result<String> {
        // TODO: Implement actual LLM gateway communication
        debug!("LLM inference request: prompt={} chars, model={:?}", 
               prompt.len(), model);
        Ok("LLM response placeholder".to_string())
    }
    
    /// Log an audit event
    pub async fn audit_log(event_type: &str, details: agnos_common::serde_json::Value) -> Result<()> {
        debug!("Audit log: {} - {:?}", event_type, details);
        Ok(())
    }
    
    /// Check resource usage
    pub async fn check_resources() -> ResourceUsage {
        // TODO: Implement resource checking
        ResourceUsage::default()
    }
}

/// Macros for agent development
#[macro_export]
macro_rules! agent_main {
    ($agent_type:ty) => {
        #[tokio::main]
        async fn main() -> anyhow::Result<()> {
            use agnos_sys::agent::{AgentContext, AgentRuntime};
            
            tracing_subscriber::fmt::init();
            
            // Load configuration from environment or defaults
            let config = agnos_common::AgentConfig {
                name: env!("CARGO_PKG_NAME").to_string(),
                agent_type: agnos_common::AgentType::Service,
                ..Default::default()
            };
            
            let (runtime, _message_rx) = AgentRuntime::new(config);
            let agent = <$agent_type>::new()?;
            
            runtime.run(agent).await
        }
    };
}
