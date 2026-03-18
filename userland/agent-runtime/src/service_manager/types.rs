//! Types, enums, and data structures for the service manager.

use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tokio::process::Child;

// ---------------------------------------------------------------------------
// Service definition (parsed from TOML)
// ---------------------------------------------------------------------------

/// A service definition loaded from `/etc/agnos/services/<name>.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceDefinition {
    /// Human-readable service name (must be unique).
    pub name: String,

    /// Absolute path to the executable.
    pub exec_start: String,

    /// Arguments passed to the executable.
    #[serde(default)]
    pub args: Vec<String>,

    /// Environment variables (`KEY=VALUE`).
    #[serde(default)]
    pub environment: Vec<String>,

    /// Service names that must be running before this service starts.
    #[serde(default)]
    pub after: Vec<String>,

    /// Service names that should (soft) start before this one but are not required.
    #[serde(default)]
    pub wants: Vec<String>,

    /// Restart policy.
    #[serde(default)]
    pub restart: RestartPolicy,

    /// Maximum number of restart attempts before giving up.
    #[serde(default = "default_max_restarts")]
    pub max_restarts: u32,

    /// Seconds to wait between restarts (doubles on each consecutive failure, capped at 60s).
    #[serde(default = "default_restart_delay")]
    pub restart_delay_secs: u64,

    /// Unix user to run as (empty = inherit).
    #[serde(default)]
    pub user: String,

    /// Unix group to run as (empty = inherit).
    #[serde(default)]
    pub group: String,

    /// Working directory.
    #[serde(default)]
    pub working_directory: String,

    /// Service type (analogous to systemd Type=).
    #[serde(default)]
    pub service_type: ServiceType,

    /// Readiness timeout — how long to wait for sd_notify READY=1.
    #[serde(default = "default_readiness_timeout")]
    pub readiness_timeout_secs: u64,

    /// Resource limits.
    #[serde(default)]
    pub resources: ServiceResources,

    /// Whether the service is enabled (started at boot).
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Optional description.
    #[serde(default)]
    pub description: String,
}

pub fn default_max_restarts() -> u32 {
    5
}
pub fn default_restart_delay() -> u64 {
    1
}
pub fn default_readiness_timeout() -> u64 {
    30
}
pub fn default_true() -> bool {
    true
}

/// Restart policy for a service.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum RestartPolicy {
    /// Never restart.
    No,
    /// Restart on any exit.
    #[default]
    Always,
    /// Restart only on non-zero exit.
    OnFailure,
}

/// Service type (how readiness is determined).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ServiceType {
    /// The service is ready as soon as the process is forked.
    #[default]
    Simple,
    /// The service will send `READY=1` via sd_notify when ready.
    Notify,
    /// The service runs, exits, and is considered "active" while exit code is 0.
    Oneshot,
}

/// Per-service resource limits.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ServiceResources {
    /// Memory limit in bytes (0 = unlimited).
    #[serde(default)]
    pub memory_max: u64,
    /// CPU quota as percentage of one core (0 = unlimited).
    #[serde(default)]
    pub cpu_quota_percent: u32,
    /// Max number of tasks/threads (0 = unlimited).
    #[serde(default)]
    pub tasks_max: u32,
}

// ---------------------------------------------------------------------------
// Service runtime state
// ---------------------------------------------------------------------------

/// Current state of a managed service.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServiceState {
    Stopped,
    Starting,
    Running,
    Stopping,
    Failed,
    /// Oneshot completed successfully.
    Exited,
}

impl std::fmt::Display for ServiceState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Stopped => write!(f, "stopped"),
            Self::Starting => write!(f, "starting"),
            Self::Running => write!(f, "running"),
            Self::Stopping => write!(f, "stopping"),
            Self::Failed => write!(f, "failed"),
            Self::Exited => write!(f, "exited"),
        }
    }
}

/// Runtime information for one service.
pub(crate) struct ServiceRuntime {
    pub(crate) definition: ServiceDefinition,
    pub(crate) state: ServiceState,
    pub(crate) child: Option<Child>,
    pub(crate) pid: Option<u32>,
    pub(crate) restart_count: u32,
    pub(crate) started_at: Option<Instant>,
    pub(crate) exit_code: Option<i32>,
}

/// Publicly visible service status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceStatus {
    pub name: String,
    pub state: ServiceState,
    pub pid: Option<u32>,
    pub restart_count: u32,
    #[serde(skip)]
    pub uptime: Option<Duration>,
    pub exit_code: Option<i32>,
    pub enabled: bool,
    pub description: String,
}

impl ServiceStatus {
    /// Human-readable uptime string.
    pub fn uptime_display(&self) -> String {
        match self.uptime {
            Some(d) => {
                let secs = d.as_secs();
                if secs < 60 {
                    format!("{}s", secs)
                } else if secs < 3600 {
                    format!("{}m {}s", secs / 60, secs % 60)
                } else {
                    format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
                }
            }
            None => "-".to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// Fleet configuration
// ---------------------------------------------------------------------------

/// Declarative fleet configuration: defines the desired agent fleet state.
/// Loaded from a TOML file (e.g., `/etc/agnos/fleet.toml`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FleetConfig {
    /// Desired services to run
    #[serde(default)]
    pub services: Vec<ServiceDefinition>,
}

impl FleetConfig {
    /// Load fleet config from a TOML file
    pub async fn from_file(path: &Path) -> Result<Self> {
        let content = tokio::fs::read_to_string(path).await.context(format!(
            "Failed to read fleet config from {}",
            path.display()
        ))?;
        let config: FleetConfig =
            toml::from_str(&content).context("Failed to parse fleet config TOML")?;
        Ok(config)
    }

    /// Compute the reconciliation plan: which services to start, stop, or update
    pub fn reconcile(&self, running: &[String]) -> ReconciliationPlan {
        let desired: std::collections::HashSet<String> = self
            .services
            .iter()
            .filter(|s| s.enabled)
            .map(|s| s.name.clone())
            .collect();
        let current: std::collections::HashSet<String> = running.iter().cloned().collect();

        let to_start: Vec<String> = desired.difference(&current).cloned().collect();
        let to_stop: Vec<String> = current.difference(&desired).cloned().collect();
        let unchanged: Vec<String> = desired.intersection(&current).cloned().collect();

        ReconciliationPlan {
            to_start,
            to_stop,
            unchanged,
        }
    }
}

/// Plan for reconciling desired vs actual fleet state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconciliationPlan {
    pub to_start: Vec<String>,
    pub to_stop: Vec<String>,
    pub unchanged: Vec<String>,
}

impl ReconciliationPlan {
    /// Whether any changes are needed
    pub fn has_changes(&self) -> bool {
        !self.to_start.is_empty() || !self.to_stop.is_empty()
    }

    /// Human-readable summary
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        if !self.to_start.is_empty() {
            parts.push(format!("start: {}", self.to_start.join(", ")));
        }
        if !self.to_stop.is_empty() {
            parts.push(format!("stop: {}", self.to_stop.join(", ")));
        }
        if !self.unchanged.is_empty() {
            parts.push(format!("unchanged: {}", self.unchanged.join(", ")));
        }
        if parts.is_empty() {
            "No changes needed".to_string()
        } else {
            parts.join(" | ")
        }
    }
}
