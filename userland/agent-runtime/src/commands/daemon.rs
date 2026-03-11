//! Daemon startup and main loop.

use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use crate::cli::Cli;
use crate::daemon_config::DaemonConfig;
use crate::health::check_dependencies_healthy;
use crate::orchestrator::Orchestrator;
use crate::registry::AgentRegistry;
use crate::service_manager::ServiceManager;
use crate::supervisor::Supervisor;

/// Runtime state shared across all components
#[derive(Clone)]
pub struct RuntimeState {
    pub registry: Arc<AgentRegistry>,
    pub orchestrator: Arc<Orchestrator>,
    pub supervisor: Arc<Supervisor>,
}

pub async fn run_daemon(cli: Cli) -> Result<()> {
    info!("Starting agent runtime daemon...");

    // H19: Validate configuration before proceeding
    let config = DaemonConfig::default();
    config
        .validate()
        .with_context(|| "Daemon configuration validation failed")?;
    info!(
        "Configuration validated: port={}, shutdown_timeout={}s",
        config.api_port, config.shutdown_timeout_secs
    );

    // H21: Clean up stale socket files from previous abnormal terminations
    let ipc_dir = std::path::Path::new("/run/agnos/agents");
    crate::ipc::cleanup_stale_sockets_in_dir(ipc_dir).await;

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

    // H16: Verify critical dependencies are healthy before declaring ready
    if let Err(e) = check_dependencies_healthy(&config, &cli.data_dir).await {
        error!("Dependency health check failed: {}", e);
        warn!("Proceeding despite health check failure");
    }

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
