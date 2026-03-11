//! CLI argument parsing for the agent runtime daemon.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "agent-runtime")]
#[command(about = "AGNOS Agent Runtime Daemon")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    #[arg(short, long, default_value = "/etc/agnos/agent-runtime")]
    pub config_dir: PathBuf,

    #[arg(short, long, default_value = "/var/lib/agnos/agents")]
    pub data_dir: PathBuf,
}

#[derive(Subcommand)]
pub enum Commands {
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
pub enum ServiceCommands {
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
pub enum PackageCommands {
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
