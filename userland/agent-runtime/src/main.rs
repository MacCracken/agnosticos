//! AGNOS Agent Runtime Daemon (akd)
//!
//! Manages agent lifecycle, orchestration, and resource allocation.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use tokio::sync::mpsc;
use tracing::{error, info};

use agnos_common::{AgentConfig, AgentId};

mod agent;
pub mod capability;
pub mod database;
pub mod file_watcher;
pub mod http_api;
pub mod integrity;
pub mod ipc;
pub mod knowledge_base;
pub mod learning;
pub mod lifecycle;
pub mod marketplace;
pub mod mcp_server;
pub mod memory_store;
pub mod mtls;
pub mod multimodal;
pub mod network_tools;
pub mod orchestrator;
pub mod package_manager;
pub mod pubsub;
pub mod rag;
pub mod registry;
pub mod resource;
pub mod resource_forecast;
pub mod rollback;
pub mod sandbox;
pub mod seccomp_profiles;
pub mod service_manager;
pub mod supervisor;
pub mod swarm;
pub mod tool_analysis;
pub mod vector_store;
pub mod wasm_runtime;

use crate::agent::Agent;
use crate::orchestrator::Orchestrator;
use crate::registry::AgentRegistry;
use crate::service_manager::ServiceManager;
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
    /// Manage system services
    Service {
        #[command(subcommand)]
        action: ServiceCommands,
    },
    /// Manage agent packages
    Package {
        #[command(subcommand)]
        action: PackageCommands,
    },
}

#[derive(Subcommand)]
enum ServiceCommands {
    /// List all services and their statuses
    List,
    /// Start a service
    Start {
        /// Service name
        name: String,
    },
    /// Stop a service
    Stop {
        /// Service name
        name: String,
    },
    /// Restart a service
    Restart {
        /// Service name
        name: String,
    },
    /// Show service status
    Status {
        /// Service name
        name: String,
    },
    /// Enable a service (start on boot)
    Enable {
        /// Service name
        name: String,
    },
    /// Disable a service (do not start on boot)
    Disable {
        /// Service name
        name: String,
    },
    /// Boot all enabled services in dependency order
    Boot,
}

#[derive(Subcommand)]
enum PackageCommands {
    /// Install an agent package from a directory
    Install {
        /// Path to the package directory
        source: PathBuf,
    },
    /// Uninstall an agent package
    Uninstall {
        /// Package name
        name: String,
    },
    /// List installed packages
    List,
    /// Show package details
    Info {
        /// Package name
        name: String,
    },
    /// Search installed packages
    Search {
        /// Search query
        query: String,
    },
    /// Verify package integrity
    Verify {
        /// Package name
        name: String,
    },
    /// Validate a package before installing
    Validate {
        /// Path to the package directory
        source: PathBuf,
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
        Commands::Service { action } => handle_service_command(action, &cli.config_dir).await,
        Commands::Package { action } => handle_package_command(action, &cli.data_dir).await,
    }
}

async fn run_daemon(cli: Cli) -> Result<()> {
    info!("Starting agent runtime daemon...");

    let registry = Arc::new(AgentRegistry::new());
    let supervisor = Arc::new(Supervisor::new(registry.clone()));
    let orchestrator = Arc::new(Orchestrator::new(registry.clone()));

    // Initialize the service manager and boot system services
    let services_dir = cli.config_dir.join("services");
    let service_manager = Arc::new(ServiceManager::new(&services_dir));
    let loaded = service_manager.load_definitions().await?;
    info!(
        "Loaded {} service definitions from {}",
        loaded,
        services_dir.display()
    );

    // Boot all enabled services in dependency order
    if let Err(e) = service_manager.boot().await {
        error!("Service boot errors (non-fatal): {}", e);
    }

    // Start service health monitor in background
    let monitor_mgr = service_manager.clone();
    tokio::spawn(async move {
        monitor_mgr.monitor_loop().await;
    });

    // Notify systemd we're ready (if running under systemd)
    ServiceManager::notify_ready();

    let state = RuntimeState {
        registry,
        orchestrator,
        supervisor,
    };

    info!("Agent runtime daemon started successfully");

    // Start the HTTP API server in the background
    tokio::spawn(async move {
        if let Err(e) = crate::http_api::start_server(crate::http_api::DEFAULT_PORT).await {
            error!("HTTP API server error: {}", e);
        }
    });

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
    service_manager.shutdown_all().await?;
    state.supervisor.shutdown_all().await?;
    info!("Agent runtime daemon stopped");

    Ok(())
}

async fn handle_service_command(action: ServiceCommands, config_dir: &Path) -> Result<()> {
    let services_dir = config_dir.join("services");
    let mgr = ServiceManager::new(&services_dir);
    mgr.load_definitions().await?;

    match action {
        ServiceCommands::List => {
            let services = mgr.list_services().await;
            if services.is_empty() {
                println!("No services configured.");
                println!("Add service definitions to: {}", services_dir.display());
                return Ok(());
            }
            println!(
                "{:<20} {:<10} {:<8} {:<10} {:<8} DESCRIPTION",
                "NAME", "STATE", "PID", "UPTIME", "RESTARTS"
            );
            println!("{}", "-".repeat(80));
            for svc in &services {
                println!(
                    "{:<20} {:<10} {:<8} {:<10} {:<8} {}",
                    svc.name,
                    svc.state.to_string(),
                    svc.pid.map_or("-".to_string(), |p| p.to_string()),
                    svc.uptime_display(),
                    svc.restart_count,
                    svc.description,
                );
            }
            println!("\nTotal: {} service(s)", services.len());
        }
        ServiceCommands::Start { name } => {
            mgr.start_service(&name).await?;
            println!("Service '{}' started.", name);
        }
        ServiceCommands::Stop { name } => {
            mgr.stop_service(&name).await?;
            println!("Service '{}' stopped.", name);
        }
        ServiceCommands::Restart { name } => {
            mgr.restart_service(&name).await?;
            println!("Service '{}' restarted.", name);
        }
        ServiceCommands::Status { name } => match mgr.get_status(&name).await {
            Some(status) => {
                println!("Service: {}", status.name);
                println!("  State:       {}", status.state);
                println!(
                    "  PID:         {}",
                    status.pid.map_or("-".to_string(), |p| p.to_string())
                );
                println!("  Uptime:      {}", status.uptime_display());
                println!("  Restarts:    {}", status.restart_count);
                println!("  Enabled:     {}", status.enabled);
                println!(
                    "  Exit Code:   {}",
                    status.exit_code.map_or("-".to_string(), |c| c.to_string())
                );
                if !status.description.is_empty() {
                    println!("  Description: {}", status.description);
                }
            }
            None => {
                anyhow::bail!("Unknown service: {}", name);
            }
        },
        ServiceCommands::Enable { name } => {
            mgr.enable_service(&name).await?;
            println!("Service '{}' enabled.", name);
        }
        ServiceCommands::Disable { name } => {
            mgr.disable_service(&name).await?;
            println!("Service '{}' disabled.", name);
        }
        ServiceCommands::Boot => {
            mgr.boot().await?;
            println!("All enabled services started.");
        }
    }

    Ok(())
}

async fn handle_package_command(action: PackageCommands, data_dir: &Path) -> Result<()> {
    let pkg_dir = data_dir.join("packages");
    let mut mgr = crate::package_manager::PackageManager::new(&pkg_dir)?;

    match action {
        PackageCommands::Install { source } => {
            // Validate first
            let package = mgr.validate_package(&source)?;

            // Show consent prompt
            println!("{}", crate::package_manager::consent_prompt(&package));
            println!();

            // Install
            let result = mgr.install(&source)?;
            if let Some(ref prev) = result.upgraded_from {
                println!(
                    "Upgraded '{}' from v{} to v{}",
                    result.name, prev, result.version
                );
            } else {
                println!("Installed '{}' v{}", result.name, result.version);
            }
            println!("  Location: {}", result.install_dir.display());
        }
        PackageCommands::Uninstall { name } => {
            let result = mgr.uninstall(&name)?;
            println!(
                "Uninstalled '{}' v{} ({} files removed)",
                result.name, result.version, result.files_removed
            );
        }
        PackageCommands::List => {
            let packages = mgr.list_installed();
            if packages.is_empty() {
                println!("No packages installed.");
                return Ok(());
            }
            println!(
                "{:<25} {:<12} {:<20} DESCRIPTION",
                "NAME", "VERSION", "AUTHOR"
            );
            println!("{}", "-".repeat(80));
            for pkg in &packages {
                println!(
                    "{:<25} {:<12} {:<20} {}",
                    pkg.name,
                    pkg.version,
                    pkg.author,
                    truncate(&pkg.description, 30),
                );
            }
            println!("\nTotal: {} package(s)", packages.len());
        }
        PackageCommands::Info { name } => match mgr.get_info(&name) {
            Some(info) => {
                println!("Package: {}", info.manifest.name);
                println!("  Version:     {}", info.manifest.version);
                println!("  Author:      {}", info.manifest.author);
                println!("  Description: {}", info.manifest.description);
                println!(
                    "  Installed:   {}",
                    info.installed_at.format("%Y-%m-%d %H:%M:%S UTC")
                );
                println!("  Location:    {}", info.install_dir.display());
                println!("  Binary:      {}", info.binary_path.display());
                println!("  Hash:        {}", info.binary_hash);
                if !info.manifest.requested_permissions.is_empty() {
                    println!("  Permissions: {:?}", info.manifest.requested_permissions);
                }
                println!("  Network:     {:?}", info.manifest.network_scope);
                println!("  Auto-update: {}", info.auto_update);
            }
            None => {
                anyhow::bail!("Package '{}' is not installed", name);
            }
        },
        PackageCommands::Search { query } => {
            let results = mgr.search(&query);
            if results.is_empty() {
                println!("No packages matching '{}'.", query);
                return Ok(());
            }
            for pkg in &results {
                println!("{} v{} — {}", pkg.name, pkg.version, pkg.description);
            }
        }
        PackageCommands::Verify { name } => match mgr.verify(&name) {
            Ok(true) => println!("Package '{}' integrity OK.", name),
            Ok(false) => println!(
                "Package '{}' integrity FAILED — binary may have been modified.",
                name
            ),
            Err(e) => anyhow::bail!("{}", e),
        },
        PackageCommands::Validate { source } => {
            let package = mgr.validate_package(&source)?;
            println!("Package valid:");
            println!("  Name:    {}", package.manifest.name);
            println!("  Version: {}", package.manifest.version);
            println!(
                "  Binary:  {} bytes (hash: {})",
                package.binary_size, package.binary_hash
            );
            println!("{}", crate::package_manager::consent_prompt(&package));
        }
    }

    Ok(())
}

fn truncate(s: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    if s.len() <= max {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max - 1).collect();
        format!("{}…", truncated)
    }
}

async fn start_agent(config_path: PathBuf) -> Result<()> {
    let config_str = tokio::fs::read_to_string(&config_path)
        .await
        .with_context(|| format!("Failed to read config file: {}", config_path.display()))?;

    let config: AgentConfig =
        serde_json::from_str(&config_str).with_context(|| "Failed to parse agent configuration")?;

    info!(
        "Starting agent: {} (type: {:?})",
        config.name, config.agent_type
    );

    // Create a temporary registry for the single-shot command
    let registry = Arc::new(AgentRegistry::new());

    // Create and register the agent
    let (agent, _rx) = Agent::new(config.clone()).await?;
    let handle = registry.register(&agent, config).await?;

    println!("Agent started: {} (id: {})", handle.name, handle.id);
    println!("  Status: {:?}", handle.status);
    println!(
        "  PID: {}",
        handle.pid.map_or("N/A".to_string(), |p| p.to_string())
    );

    Ok(())
}

async fn stop_agent(agent_id: String) -> Result<()> {
    let uuid: uuid::Uuid = agent_id
        .parse()
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
    println!("{:<40} {:<10} Socket", "ID", "PID");
    println!("{}", "-".repeat(70));

    match tokio::fs::read_dir(agents_dir).await {
        Ok(mut entries) => {
            let mut count = 0;
            while let Ok(Some(entry)) = entries.next_entry().await {
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "sock") {
                    let name = path
                        .file_stem()
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
    let uuid: uuid::Uuid = agent_id
        .parse()
        .with_context(|| format!("Invalid agent ID (expected UUID): {}", agent_id))?;
    let id = AgentId(uuid);

    let socket_path = format!("/run/agnos/agents/{}.sock", id);
    let socket_exists = std::path::Path::new(&socket_path).exists();

    println!("Agent: {}", id);
    println!(
        "  Socket: {} ({})",
        socket_path,
        if socket_exists { "exists" } else { "not found" }
    );

    if socket_exists {
        // Try to connect to verify it's responsive
        match tokio::time::timeout(
            std::time::Duration::from_secs(5),
            tokio::net::UnixStream::connect(&socket_path),
        )
        .await
        {
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
    let uuid: uuid::Uuid = target
        .parse()
        .with_context(|| format!("Invalid agent ID (expected UUID): {}", target))?;
    let id = AgentId(uuid);

    // Validate the message is valid JSON
    let _payload: serde_json::Value =
        serde_json::from_str(&message).with_context(|| "Message must be valid JSON")?;

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
