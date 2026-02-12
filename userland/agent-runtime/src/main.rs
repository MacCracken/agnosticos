//! AGNOS Agent Runtime Daemon (akd)
//!
//! Manages agent lifecycle, orchestration, and resource allocation.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};

use agnos_common::{
    AgentConfig, AgentId, AgentStatus, AgentType, Message, MessageType, ResourceUsage,
};

mod agent;
mod ipc;
mod orchestrator;
mod registry;
mod resource;
mod sandbox;
mod supervisor;

use crate::agent::{Agent, AgentHandle};
use crate::orchestrator::Orchestrator;
use crate::registry::AgentRegistry;
use crate::supervisor::Supervisor;

#[derive(Parser)]
#[command(name = "agent-runtime")]
#[command(about = "AGNOS Agent Runtime Daemon")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    #[arg(short, long, default_value = "/etc/agnos/agent-runtime")]
    config_dir: PathBuf,

    #[arg(short, long, default_value = "/var/lib/agnos/agents")]
    data_dir: PathBuf,
}

#[derive(Subcommand)]
enum Commands {
    /// Run the agent runtime daemon
    Daemon,
    /// Start a new agent
    Start {
        /// Path to agent configuration file
        #[arg(short, long)]
        config: PathBuf,
    },
    /// Stop an agent
    Stop {
        /// Agent ID
        agent_id: String,
    },
    /// List running agents
    List,
    /// Get agent status
    Status {
        /// Agent ID
        agent_id: String,
    },
    /// Send a message to an agent
    Send {
        /// Target agent ID
        target: String,
        /// Message payload (JSON)
        message: String,
    },
}

/// Runtime state shared across all components
#[derive(Clone)]
pub struct RuntimeState {
    pub registry: Arc<AgentRegistry>,
    pub orchestrator: Arc<Orchestrator>,
    pub supervisor: Arc<Supervisor>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    info!("AGNOS Agent Runtime Daemon v{}", env!("CARGO_PKG_VERSION"));

    let cli = Cli::parse();

    match cli.command {
        Commands::Daemon => run_daemon(cli).await,
        Commands::Start { config } => start_agent(config).await,
        Commands::Stop { agent_id } => stop_agent(agent_id).await,
        Commands::List => list_agents().await,
        Commands::Status { agent_id } => get_status(agent_id).await,
        Commands::Send { target, message } => send_message(target, message).await,
    }
}

async fn run_daemon(cli: Cli) -> Result<()> {
    info!("Starting agent runtime daemon...");

    let registry = Arc::new(AgentRegistry::new());
    let supervisor = Arc::new(Supervisor::new(registry.clone()));
    let orchestrator = Arc::new(Orchestrator::new(registry.clone()));

    let state = RuntimeState {
        registry,
        orchestrator,
        supervisor,
    };

    info!("Agent runtime daemon started successfully");

    let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            info!("Received shutdown signal");
        }
        _ = shutdown_rx.recv() => {
            info!("Received internal shutdown signal");
        }
    }

    info!("Shutting down agent runtime daemon...");
    state.supervisor.shutdown_all().await?;
    info!("Agent runtime daemon stopped");

    Ok(())
}

async fn start_agent(config_path: PathBuf) -> Result<()> {
    let config_str = tokio::fs::read_to_string(&config_path)
        .await
        .with_context(|| format!("Failed to read config file: {}", config_path.display()))?;

    let config: AgentConfig = serde_json::from_str(&config_str)
        .with_context(|| "Failed to parse agent configuration")?;

    println!("Starting agent: {}", config.name);

    Ok(())
}

async fn stop_agent(agent_id: String) -> Result<()> {
    println!("Stopping agent: {}", agent_id);
    Ok(())
}

async fn list_agents() -> Result<()> {
    println!("Running agents:");
    Ok(())
}

async fn get_status(agent_id: String) -> Result<()> {
    println!("Agent status for: {}", agent_id);
    Ok(())
}

async fn send_message(target: String, message: String) -> Result<()> {
    println!("Sending message to {}: {}", target, message);
    Ok(())
}
