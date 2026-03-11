//! AGNOS Agent Runtime Daemon (akd)
//!
//! Manages agent lifecycle, orchestration, and resource allocation.

use anyhow::Result;
use clap::Parser;
use tracing::info;

use agent_runtime::cli::{Cli, Commands};
use agent_runtime::commands::agent::{
    get_status, list_agents, send_message, start_agent, stop_agent,
};
use agent_runtime::commands::daemon::run_daemon;
use agent_runtime::commands::package::handle_package_command;
use agent_runtime::commands::service::handle_service_command;

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
