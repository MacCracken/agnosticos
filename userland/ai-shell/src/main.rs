//! AGNOS AI Shell (agnsh)
//!
//! A natural language shell interface with built-in human oversight.
//! Never exposes root to AI - all privileged operations require human approval.

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use tracing::{info, warn};

mod approval;
pub mod audit;
pub mod commands;
pub mod config;
pub mod history;
pub mod interpreter;
pub mod llm;
pub mod mode;
pub mod output;
pub mod permissions;
pub mod prompt;
pub mod sandbox;
pub mod security;
pub mod session;
pub mod ui;

use approval::ApprovalManager;
use config::ShellConfig;
use mode::{Mode, ModeManager};
use security::SecurityContext;
use session::Session;

#[derive(Parser, Debug)]
#[command(name = "agnsh")]
#[command(about = "AGNOS AI Shell - Natural language interface with human oversight")]
#[command(version)]
struct Args {
    /// Start in AI mode
    #[arg(short, long)]
    ai: bool,
    
    /// Start in human mode (default)
    #[arg(short, long)]
    human: bool,
    
    /// Execute command and exit
    #[arg(short, long)]
    command: Option<String>,
    
    /// Configuration file path
    #[arg(short, long)]
    config: Option<PathBuf>,
    
    /// Disable AI assistance
    #[arg(long)]
    no_ai: bool,
    
    /// Require approval for all AI actions
    #[arg(long)]
    strict: bool,
    
    /// Start as restricted user (no privilege escalation)
    #[arg(long)]
    restricted: bool,
    
    /// Use starship-style prompt
    #[arg(long)]
    starship: bool,
    
    /// Custom prompt format (starship-style)
    #[arg(long)]
    prompt_format: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();
    
    let args = Args::parse();
    
    // Load configuration
    let config = if let Some(config_path) = args.config {
        ShellConfig::from_file(config_path).await?
    } else {
        ShellConfig::default()
    };
    
    // Initialize security context
    let security = SecurityContext::new(args.restricted)?;
    
    info!("Starting AGNOS AI Shell v{}", env!("CARGO_PKG_VERSION"));
    info!("User: {}, Restricted: {}", security.username(), security.is_restricted());
    
    // Determine initial mode
    let initial_mode = if args.ai {
        Mode::AiAutonomous
    } else if args.human {
        Mode::Human
    } else {
        config.default_mode.clone()
    };
    
    // Create session
    let mut session = Session::new(config, security, initial_mode).await?;
    
    // Handle one-shot command execution
    if let Some(cmd) = args.command {
        session.execute_one_shot(cmd).await?;
        return Ok(());
    }
    
    // Run interactive shell
    session.run_interactive().await?;
    
    Ok(())
}
