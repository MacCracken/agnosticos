//! Lightweight Agent SDK for AGNOS examples.
//!
//! Extracted from the former `agnos-sys::agent` module. Provides just enough
//! runtime scaffolding (context, message loop, trait) to run example agents.

use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, info, warn};

use agnostik::{AgentConfig, AgentId, AgentStatus, MessageType};

/// Legacy IPC message for agent-to-agent communication.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Message {
    pub id: String,
    pub source: String,
    pub target: String,
    pub message_type: MessageType,
    pub payload: serde_json::Value,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Agent context passed to all agent implementations.
pub struct AgentContext {
    pub id: AgentId,
    pub config: AgentConfig,
    pub status: RwLock<AgentStatus>,
    message_tx: mpsc::Sender<Message>,
}

impl AgentContext {
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

    pub async fn send_message(&self, target: &str, payload: serde_json::Value) -> Result<()> {
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

    pub async fn status(&self) -> AgentStatus {
        *self.status.read().await
    }

    pub async fn set_status(&self, status: AgentStatus) {
        let mut s = self.status.write().await;
        *s = status;
        debug!("Agent {} status changed to {:?}", self.id, status);
    }
}

/// Trait that all AGNOS agents must implement.
#[async_trait::async_trait]
pub trait Agent: Send + Sync {
    async fn init(&mut self, ctx: &AgentContext) -> Result<()>;
    async fn run(&mut self, ctx: &AgentContext) -> Result<()>;
    async fn handle_message(&mut self, ctx: &AgentContext, message: Message) -> Result<()>;
    async fn shutdown(&mut self, ctx: &AgentContext) -> Result<()>;
}

/// Agent runtime for executing agents.
pub struct AgentRuntime {
    ctx: Arc<AgentContext>,
    message_rx: Option<mpsc::Receiver<Message>>,
}

impl AgentRuntime {
    pub fn new(config: AgentConfig) -> Self {
        let (ctx, message_rx) = AgentContext::new(config);
        let ctx = Arc::new(ctx);

        Self {
            ctx,
            message_rx: Some(message_rx),
        }
    }

    pub async fn run<A: Agent>(mut self, mut agent: A) -> Result<()> {
        info!("Starting agent runtime for {}", self.ctx.config.name);

        agent
            .init(&self.ctx)
            .await
            .with_context(|| "Agent initialization failed")?;

        self.ctx.set_status(AgentStatus::Running).await;
        info!("Agent {} is running", self.ctx.config.name);

        let message_rx = self.message_rx.take();
        let agent_result = self.run_message_loop(&mut agent, message_rx).await;

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
                result = agent.run(&self.ctx) => {
                    result?;
                    break;
                }
            }
        }

        Ok(())
    }
}

/// Macro for agent entry points.
#[macro_export]
macro_rules! agent_main {
    ($agent_type:ty) => {
        #[tokio::main]
        async fn main() -> anyhow::Result<()> {
            use agnos_examples::agent::AgentRuntime;

            tracing_subscriber::fmt::init();

            let config = agnostik::AgentConfig {
                name: env!("CARGO_PKG_NAME").to_string(),
                agent_type: agnostik::AgentType::Service,
                ..Default::default()
            };

            let runtime = AgentRuntime::new(config);
            let agent = <$agent_type>::new()?;

            runtime.run(agent).await
        }
    };
}
