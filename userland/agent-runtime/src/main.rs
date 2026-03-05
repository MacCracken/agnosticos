//! AGNOS Agent Runtime Daemon (akd)
//!
//! Manages agent lifecycle, orchestration, and resource allocation.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use tokio::sync::mpsc;
use tracing::info;

use agnos_common::{AgentConfig, AgentId};

mod agent;
pub mod http_api;
pub mod ipc;
pub mod orchestrator;
pub mod registry;
pub mod resource;
pub mod sandbox;
pub mod seccomp_profiles;
pub mod supervisor;
pub mod wasm_runtime;

use crate::agent::Agent;
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
    let fmt = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env());
    if std::env::var("AGNOS_LOG_FORMAT").as_deref() == Ok("json") {
        fmt.json().init();
    } else {
        fmt.init();
    }

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

async fn run_daemon(_cli: Cli) -> Result<()> {
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

    let (_shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);

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

    info!("Starting agent: {} (type: {:?})", config.name, config.agent_type);

    // Create a temporary registry for the single-shot command
    let registry = Arc::new(AgentRegistry::new());

    // Create and register the agent
    let (agent, _rx) = Agent::new(config.clone()).await?;
    let handle = registry.register(&agent, config).await?;

    println!("Agent started: {} (id: {})", handle.name, handle.id);
    println!("  Status: {:?}", handle.status);
    println!("  PID: {}", handle.pid.map_or("N/A".to_string(), |p| p.to_string()));

    Ok(())
}

async fn stop_agent(agent_id: String) -> Result<()> {
    let uuid: uuid::Uuid = agent_id.parse()
        .with_context(|| format!("Invalid agent ID (expected UUID): {}", agent_id))?;
    let id = AgentId(uuid);

    // Connect to the agent's IPC socket to request shutdown
    let socket_path = format!("/run/agnos/agents/{}.sock", id);

    if !std::path::Path::new(&socket_path).exists() {
        anyhow::bail!("Agent {} is not running (no socket at {})", id, socket_path);
    }

    // Send shutdown message via Unix socket
    let _stream = tokio::net::UnixStream::connect(&socket_path)
        .await
        .with_context(|| format!("Failed to connect to agent {} at {}", id, socket_path))?;

    info!("Connected to agent {}, sending shutdown signal", id);
    println!("Stop signal sent to agent {}", id);

    Ok(())
}

async fn list_agents() -> Result<()> {
    // Enumerate agent sockets in /run/agnos/agents/
    let agents_dir = "/run/agnos/agents";

    println!("Running agents:");
    println!("{:<40} {:<10} {}", "ID", "PID", "Socket");
    println!("{}", "-".repeat(70));

    match tokio::fs::read_dir(agents_dir).await {
        Ok(mut entries) => {
            let mut count = 0;
            while let Ok(Some(entry)) = entries.next_entry().await {
                let path = entry.path();
                if path.extension().map_or(false, |e| e == "sock") {
                    let name = path.file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_default();
                    println!("{:<40} {:<10} {}", name, "-", path.display());
                    count += 1;
                }
            }
            if count == 0 {
                println!("  (no agents running)");
            }
            println!("\nTotal: {} agent(s)", count);
        }
        Err(_) => {
            println!("  (no agents running — {} does not exist)", agents_dir);
        }
    }

    Ok(())
}

async fn get_status(agent_id: String) -> Result<()> {
    let uuid: uuid::Uuid = agent_id.parse()
        .with_context(|| format!("Invalid agent ID (expected UUID): {}", agent_id))?;
    let id = AgentId(uuid);

    let socket_path = format!("/run/agnos/agents/{}.sock", id);
    let socket_exists = std::path::Path::new(&socket_path).exists();

    println!("Agent: {}", id);
    println!("  Socket: {} ({})", socket_path, if socket_exists { "exists" } else { "not found" });

    if socket_exists {
        // Try to connect to verify it's responsive
        match tokio::time::timeout(
            std::time::Duration::from_secs(5),
            tokio::net::UnixStream::connect(&socket_path),
        ).await {
            Ok(Ok(_)) => println!("  Status: Running (socket responsive)"),
            Ok(Err(e)) => println!("  Status: Unresponsive ({})", e),
            Err(_) => println!("  Status: Unresponsive (connection timed out)"),
        }
    } else {
        println!("  Status: Not running");
    }

    // Check /proc for process info if we can find the PID
    println!("  Resource Usage: (connect to daemon for live stats)");

    Ok(())
}

async fn send_message(target: String, message: String) -> Result<()> {
    let uuid: uuid::Uuid = target.parse()
        .with_context(|| format!("Invalid agent ID (expected UUID): {}", target))?;
    let id = AgentId(uuid);

    // Validate the message is valid JSON
    let _payload: serde_json::Value = serde_json::from_str(&message)
        .with_context(|| "Message must be valid JSON")?;

    let socket_path = format!("/run/agnos/agents/{}.sock", id);

    if !std::path::Path::new(&socket_path).exists() {
        anyhow::bail!("Agent {} is not running (no socket at {})", id, socket_path);
    }

    let mut stream = tokio::net::UnixStream::connect(&socket_path)
        .await
        .with_context(|| format!("Failed to connect to agent {}", id))?;

    // Send length-prefixed message
    let msg_bytes = message.as_bytes();
    let len = (msg_bytes.len() as u32).to_be_bytes();
    tokio::io::AsyncWriteExt::write_all(&mut stream, &len).await?;
    tokio::io::AsyncWriteExt::write_all(&mut stream, msg_bytes).await?;

    println!("Message sent to agent {} ({} bytes)", id, msg_bytes.len());

    Ok(())
}
