//! Example File Manager Agent
//!
//! Demonstrates a simple agent that manages file operations.

use anyhow::Result;

use agnos_examples::agent::{Agent, AgentContext, Message};
use agnostik::{AgentStatus, MessageType};

pub struct FileManagerAgent {
    root_path: std::path::PathBuf,
}

impl FileManagerAgent {
    pub fn new() -> Result<Self> {
        Ok(Self {
            root_path: std::path::PathBuf::from("/tmp/file-manager"),
        })
    }
}

#[async_trait::async_trait]
impl Agent for FileManagerAgent {
    async fn init(&mut self, ctx: &AgentContext) -> Result<()> {
        tracing::info!("FileManagerAgent {} initializing", ctx.id);

        // Create root directory if it doesn't exist
        tokio::fs::create_dir_all(&self.root_path).await?;

        Ok(())
    }

    async fn run(&mut self, ctx: &AgentContext) -> Result<()> {
        tracing::info!("FileManagerAgent {} running", ctx.id);

        // Main agent loop
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

            // Check if we should shutdown
            if ctx.status().await == AgentStatus::Stopping {
                break;
            }
        }

        Ok(())
    }

    async fn handle_message(&mut self, _ctx: &AgentContext, message: Message) -> Result<()> {
        tracing::debug!("Received message: {:?}", message);

        // Handle file operation commands
        if message.message_type == MessageType::Command {
            // Parse and execute command
        }

        Ok(())
    }

    async fn shutdown(&mut self, ctx: &AgentContext) -> Result<()> {
        tracing::info!("FileManagerAgent {} shutting down", ctx.id);
        Ok(())
    }
}

// Entry point
agnos_examples::agent_main!(FileManagerAgent);
