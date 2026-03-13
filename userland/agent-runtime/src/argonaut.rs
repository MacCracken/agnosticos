//! Argonaut — Init System for AGNOS
//!
//! Minimal init system that boots AGNOS in under 3 seconds. Manages
//! service startup ordering, health checks, and shutdown sequences.
//! Named after the Greek Argonauts who sailed the Argo — one letter
//! off from AGNOS.
//!
//! This module defines the shared types and boot orchestration logic
//! that agent-runtime uses. The actual PID 1 binary will live in a
//! separate crate; this module provides the brain.

use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;

use anyhow::{bail, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

// ---------------------------------------------------------------------------
// Boot mode
// ---------------------------------------------------------------------------

/// Which mode to boot into. Determines which services and boot stages
/// are executed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BootMode {
    /// Headless server: agent-runtime + llm-gateway, no compositor.
    Server,
    /// Full desktop: agent-runtime + llm-gateway + compositor + shell.
    Desktop,
    /// Bare minimum: agent-runtime only (container/embedded use).
    Minimal,
    /// Edge: constrained hardware (RPi, NUC, IoT). Boots daimon + edge agent
    /// only. No compositor, no shell, no LLM gateway. Read-only rootfs,
    /// dm-verity enforced, LUKS enabled, minimal seccomp profile.
    /// Target: <256 MB disk, <128 MB RAM, <3s boot.
    Edge,
}

impl fmt::Display for BootMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Server => write!(f, "server"),
            Self::Desktop => write!(f, "desktop"),
            Self::Minimal => write!(f, "minimal"),
            Self::Edge => write!(f, "edge"),
        }
    }
}

// ---------------------------------------------------------------------------
// Edge boot configuration (Phase 14D)
// ---------------------------------------------------------------------------

/// Security and performance configuration for Edge boot mode.
///
/// Controls LUKS full-disk encryption, TPM attestation requirements,
/// read-only rootfs enforcement, and maximum allowed boot time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeBootConfig {
    /// Whether the rootfs should be mounted read-only (dm-verity enforced).
    pub readonly_rootfs: bool,
    /// Whether LUKS full-disk encryption is enabled for the data partition.
    pub luks_enabled: bool,
    /// Whether TPM 2.0 attestation is required during boot.
    pub tpm_attestation: bool,
    /// Maximum boot time in milliseconds before the watchdog triggers.
    pub max_boot_time_ms: u64,
}

impl Default for EdgeBootConfig {
    fn default() -> Self {
        Self {
            readonly_rootfs: true,
            luks_enabled: true,
            tpm_attestation: false,
            max_boot_time_ms: 3000,
        }
    }
}

// ---------------------------------------------------------------------------
// Boot stages
// ---------------------------------------------------------------------------

/// Ordered boot stages. The init system walks through these in order,
/// skipping stages that are not relevant to the current [`BootMode`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BootStage {
    MountFilesystems,
    StartDeviceManager,
    VerifyRootfs,
    StartSecurity,
    StartDatabaseServices,
    StartAgentRuntime,
    StartLlmGateway,
    StartModelServices,
    StartCompositor,
    StartShell,
    BootComplete,
}

impl BootStage {
    /// Numeric order for sorting (lower = earlier).
    fn order(self) -> u8 {
        match self {
            Self::MountFilesystems => 0,
            Self::StartDeviceManager => 1,
            Self::VerifyRootfs => 2,
            Self::StartSecurity => 3,
            Self::StartDatabaseServices => 4,
            Self::StartAgentRuntime => 5,
            Self::StartLlmGateway => 6,
            Self::StartModelServices => 7,
            Self::StartCompositor => 8,
            Self::StartShell => 9,
            Self::BootComplete => 10,
        }
    }
}

impl fmt::Display for BootStage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MountFilesystems => write!(f, "mount-filesystems"),
            Self::StartDeviceManager => write!(f, "start-device-manager"),
            Self::VerifyRootfs => write!(f, "verify-rootfs"),
            Self::StartSecurity => write!(f, "start-security"),
            Self::StartDatabaseServices => write!(f, "start-database-services"),
            Self::StartAgentRuntime => write!(f, "start-agent-runtime"),
            Self::StartLlmGateway => write!(f, "start-llm-gateway"),
            Self::StartModelServices => write!(f, "start-model-services"),
            Self::StartCompositor => write!(f, "start-compositor"),
            Self::StartShell => write!(f, "start-shell"),
            Self::BootComplete => write!(f, "boot-complete"),
        }
    }
}

impl PartialOrd for BootStage {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for BootStage {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.order().cmp(&other.order())
    }
}

// ---------------------------------------------------------------------------
// Boot step status
// ---------------------------------------------------------------------------

/// Status of an individual boot step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BootStepStatus {
    Pending,
    Running,
    Complete,
    Failed,
    Skipped,
}

impl fmt::Display for BootStepStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Running => write!(f, "running"),
            Self::Complete => write!(f, "complete"),
            Self::Failed => write!(f, "failed"),
            Self::Skipped => write!(f, "skipped"),
        }
    }
}

// ---------------------------------------------------------------------------
// Boot step
// ---------------------------------------------------------------------------

/// A single step in the boot sequence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootStep {
    /// Which stage this step represents.
    pub stage: BootStage,
    /// Human-readable description of the step.
    pub description: String,
    /// If true, failure aborts the entire boot.
    pub required: bool,
    /// Maximum time this step may take (milliseconds).
    pub timeout_ms: u64,
    /// Current status.
    pub status: BootStepStatus,
    /// When the step started executing.
    pub started_at: Option<DateTime<Utc>>,
    /// When the step finished (success or failure).
    pub completed_at: Option<DateTime<Utc>>,
    /// Error message, if any.
    pub error: Option<String>,
}

// ---------------------------------------------------------------------------
// Restart policy
// ---------------------------------------------------------------------------

/// How the init system should handle a service that exits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RestartPolicy {
    /// Always restart, regardless of exit code.
    Always,
    /// Restart only on non-zero exit.
    OnFailure,
    /// Never restart; the service is one-shot.
    Never,
}

impl fmt::Display for RestartPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Always => write!(f, "always"),
            Self::OnFailure => write!(f, "on-failure"),
            Self::Never => write!(f, "never"),
        }
    }
}

// ---------------------------------------------------------------------------
// Health / ready checks
// ---------------------------------------------------------------------------

/// Method used to check health or readiness.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthCheckType {
    /// HTTP GET against a URL — expects 2xx.
    HttpGet(String),
    /// TCP connect to host:port.
    TcpConnect(String, u16),
    /// Run a shell command — 0 exit = healthy.
    Command(String),
    /// Simply check if the PID is still alive.
    ProcessAlive,
}

/// Periodic health check for a running service.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheck {
    pub check_type: HealthCheckType,
    /// Milliseconds between checks.
    pub interval_ms: u64,
    /// Per-check timeout (milliseconds).
    pub timeout_ms: u64,
    /// How many consecutive failures before declaring unhealthy.
    pub retries: u32,
}

/// One-shot readiness check executed at startup. The service is not
/// considered "running" until the ready check passes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadyCheck {
    pub check_type: HealthCheckType,
    /// Maximum time to wait for readiness (milliseconds).
    pub timeout_ms: u64,
    /// Number of retries before giving up.
    pub retries: u32,
    /// Delay between retry attempts (milliseconds).
    pub retry_delay_ms: u64,
}

// ---------------------------------------------------------------------------
// Service types
// ---------------------------------------------------------------------------

/// Static definition of a service managed by argonaut.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceDefinition {
    /// Unique service name (e.g. "agent-runtime").
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// Path to the executable binary.
    pub binary_path: PathBuf,
    /// Command-line arguments.
    pub args: Vec<String>,
    /// Environment variables.
    pub environment: HashMap<String, String>,
    /// Names of services that must be running before this one starts.
    pub depends_on: Vec<String>,
    /// Boot modes that require this service.
    pub required_for_modes: Vec<BootMode>,
    /// What to do when the service exits.
    pub restart_policy: RestartPolicy,
    /// Optional periodic health check.
    pub health_check: Option<HealthCheck>,
    /// Optional one-shot startup readiness check.
    pub ready_check: Option<ReadyCheck>,
}

/// Runtime state of a service.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ServiceState {
    Stopped,
    Starting,
    Running,
    Stopping,
    Failed(String),
    Restarting,
}

impl ServiceState {
    /// Check whether a transition from `self` to `to` is valid.
    pub fn valid_transition(&self, to: &ServiceState) -> bool {
        // Same state is always a no-op.
        if self == to {
            return true;
        }
        match self {
            ServiceState::Stopped => matches!(to, ServiceState::Starting),
            ServiceState::Starting => {
                matches!(to, ServiceState::Running | ServiceState::Failed(_))
            }
            ServiceState::Running => {
                matches!(to, ServiceState::Stopping | ServiceState::Failed(_))
            }
            ServiceState::Stopping => {
                matches!(to, ServiceState::Stopped | ServiceState::Failed(_))
            }
            ServiceState::Failed(_) => {
                matches!(to, ServiceState::Starting | ServiceState::Stopped)
            }
            ServiceState::Restarting => matches!(to, ServiceState::Starting),
        }
    }
}

impl fmt::Display for ServiceState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Stopped => write!(f, "stopped"),
            Self::Starting => write!(f, "starting"),
            Self::Running => write!(f, "running"),
            Self::Stopping => write!(f, "stopping"),
            Self::Failed(msg) => write!(f, "failed: {msg}"),
            Self::Restarting => write!(f, "restarting"),
        }
    }
}

/// A service with its runtime state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagedService {
    pub definition: ServiceDefinition,
    pub state: ServiceState,
    pub pid: Option<u32>,
    pub started_at: Option<DateTime<Utc>>,
    pub restart_count: u32,
    pub last_health_check: Option<DateTime<Utc>>,
}

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Top-level configuration for the argonaut init system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArgonautConfig {
    /// Which boot mode to use.
    pub boot_mode: BootMode,
    /// Service definitions to manage.
    pub services: Vec<ServiceDefinition>,
    /// Total boot timeout in milliseconds (default 30 000).
    pub boot_timeout_ms: u64,
    /// Graceful shutdown timeout in milliseconds (default 10 000).
    pub shutdown_timeout_ms: u64,
    /// Whether to log to the console (useful for early boot).
    pub log_to_console: bool,
    /// Whether to run dm-verity rootfs verification at boot.
    pub verify_on_boot: bool,
}

impl Default for ArgonautConfig {
    fn default() -> Self {
        Self {
            boot_mode: BootMode::Desktop,
            services: Vec::new(),
            boot_timeout_ms: 30_000,
            shutdown_timeout_ms: 10_000,
            log_to_console: true,
            verify_on_boot: true,
        }
    }
}

// ---------------------------------------------------------------------------
// Stats
// ---------------------------------------------------------------------------

/// Snapshot of init-system statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArgonautStats {
    pub boot_mode: BootMode,
    pub boot_duration_ms: Option<u64>,
    pub services_running: usize,
    pub services_failed: usize,
    pub total_restarts: u32,
    pub boot_complete: bool,
}

// ---------------------------------------------------------------------------
// ArgonautInit — main orchestrator
// ---------------------------------------------------------------------------

/// The main init-system orchestrator. Holds the boot sequence and
/// all managed services.
pub struct ArgonautInit {
    pub config: ArgonautConfig,
    pub boot_sequence: Vec<BootStep>,
    pub services: HashMap<String, ManagedService>,
    pub boot_started: Option<DateTime<Utc>>,
    pub boot_completed: Option<DateTime<Utc>>,
}

impl ArgonautInit {
    /// Create a new init system from the given configuration. Builds
    /// the boot sequence and registers default services for the mode.
    pub fn new(config: ArgonautConfig) -> Self {
        let boot_sequence = Self::build_boot_sequence(config.boot_mode);
        let default_svc = Self::default_services(config.boot_mode);
        let mut services = HashMap::new();
        for svc in default_svc {
            let managed = ManagedService {
                definition: svc.clone(),
                state: ServiceState::Stopped,
                pid: None,
                started_at: None,
                restart_count: 0,
                last_health_check: None,
            };
            services.insert(svc.name.clone(), managed);
        }
        // Also register any extra services from config.
        for svc in &config.services {
            if !services.contains_key(&svc.name) {
                let managed = ManagedService {
                    definition: svc.clone(),
                    state: ServiceState::Stopped,
                    pid: None,
                    started_at: None,
                    restart_count: 0,
                    last_health_check: None,
                };
                services.insert(svc.name.clone(), managed);
            }
        }
        info!(mode = %config.boot_mode, steps = boot_sequence.len(), services = services.len(), "argonaut initialized");
        Self {
            config,
            boot_sequence,
            services,
            boot_started: None,
            boot_completed: None,
        }
    }

    /// Build the ordered boot sequence for a given mode.
    #[allow(clippy::vec_init_then_push)]
    pub fn build_boot_sequence(mode: BootMode) -> Vec<BootStep> {
        let mut steps = Vec::new();

        // Common early stages (all modes).
        steps.push(BootStep {
            stage: BootStage::MountFilesystems,
            description: "Mount essential filesystems (proc, sys, dev, tmp)".into(),
            required: true,
            timeout_ms: 2000,
            status: BootStepStatus::Pending,
            started_at: None,
            completed_at: None,
            error: None,
        });
        steps.push(BootStep {
            stage: BootStage::StartDeviceManager,
            description: "Start udev device manager".into(),
            required: true,
            timeout_ms: 3000,
            status: BootStepStatus::Pending,
            started_at: None,
            completed_at: None,
            error: None,
        });
        steps.push(BootStep {
            stage: BootStage::VerifyRootfs,
            description: "Verify rootfs integrity via dm-verity".into(),
            required: true,
            timeout_ms: 5000,
            status: BootStepStatus::Pending,
            started_at: None,
            completed_at: None,
            error: None,
        });
        steps.push(BootStep {
            stage: BootStage::StartSecurity,
            description: "Initialize Landlock, seccomp, and MAC policies".into(),
            required: true,
            timeout_ms: 2000,
            status: BootStepStatus::Pending,
            started_at: None,
            completed_at: None,
            error: None,
        });
        // Edge mode: enforce dm-verity, TPM attestation, then straight to
        // agent-runtime. No databases, no LLM gateway, no compositor/shell.
        if mode == BootMode::Edge {
            steps.push(BootStep {
                stage: BootStage::StartAgentRuntime,
                description: "Start daimon (agent-runtime) in edge mode on port 8090".into(),
                required: true,
                timeout_ms: 3000,
                status: BootStepStatus::Pending,
                started_at: None,
                completed_at: None,
                error: None,
            });
            steps.push(BootStep {
                stage: BootStage::BootComplete,
                description: "Edge boot complete — agent ready".into(),
                required: true,
                timeout_ms: 500,
                status: BootStepStatus::Pending,
                started_at: None,
                completed_at: None,
                error: None,
            });
            return steps;
        }

        // Server and Desktop get database services.
        if mode == BootMode::Server || mode == BootMode::Desktop {
            steps.push(BootStep {
                stage: BootStage::StartDatabaseServices,
                description: "Start PostgreSQL and Redis database services".into(),
                required: true,
                timeout_ms: 15_000,
                status: BootStepStatus::Pending,
                started_at: None,
                completed_at: None,
                error: None,
            });
        }

        steps.push(BootStep {
            stage: BootStage::StartAgentRuntime,
            description: "Start daimon (agent-runtime) on port 8090".into(),
            required: true,
            timeout_ms: 5000,
            status: BootStepStatus::Pending,
            started_at: None,
            completed_at: None,
            error: None,
        });

        // Server and Desktop get llm-gateway.
        if mode == BootMode::Server || mode == BootMode::Desktop {
            steps.push(BootStep {
                stage: BootStage::StartLlmGateway,
                description: "Start hoosh (llm-gateway) on port 8088".into(),
                required: false,
                timeout_ms: 5000,
                status: BootStepStatus::Pending,
                started_at: None,
                completed_at: None,
                error: None,
            });
            steps.push(BootStep {
                stage: BootStage::StartModelServices,
                description: "Start Synapse LLM model manager".into(),
                required: false,
                timeout_ms: 15_000,
                status: BootStepStatus::Pending,
                started_at: None,
                completed_at: None,
                error: None,
            });
        }

        // Desktop-only stages.
        if mode == BootMode::Desktop {
            steps.push(BootStep {
                stage: BootStage::StartCompositor,
                description: "Start aethersafha (Wayland compositor)".into(),
                required: true,
                timeout_ms: 5000,
                status: BootStepStatus::Pending,
                started_at: None,
                completed_at: None,
                error: None,
            });
            steps.push(BootStep {
                stage: BootStage::StartShell,
                description: "Start agnoshi (AI shell)".into(),
                required: false,
                timeout_ms: 3000,
                status: BootStepStatus::Pending,
                started_at: None,
                completed_at: None,
                error: None,
            });
        }

        // Final stage.
        steps.push(BootStep {
            stage: BootStage::BootComplete,
            description: "All boot stages finished".into(),
            required: true,
            timeout_ms: 1000,
            status: BootStepStatus::Pending,
            started_at: None,
            completed_at: None,
            error: None,
        });

        steps
    }

    /// Return the PostgreSQL and Redis database service definitions.
    pub fn database_services() -> Vec<ServiceDefinition> {
        vec![
            ServiceDefinition {
                name: "postgres".into(),
                description: "PostgreSQL 17 database server".into(),
                binary_path: PathBuf::from("/usr/lib/postgresql/17/bin/postgres"),
                args: vec![
                    "-D".into(),
                    "/var/lib/postgresql/data".into(),
                    "-c".into(),
                    "config_file=/etc/postgresql/postgresql.conf.agnos".into(),
                ],
                environment: {
                    let mut env = HashMap::new();
                    env.insert("PGDATA".into(), "/var/lib/postgresql/data".into());
                    env
                },
                depends_on: vec![],
                required_for_modes: vec![BootMode::Server, BootMode::Desktop],
                restart_policy: RestartPolicy::OnFailure,
                health_check: Some(HealthCheck {
                    check_type: HealthCheckType::TcpConnect("127.0.0.1".into(), 5432),
                    interval_ms: 15_000,
                    timeout_ms: 2000,
                    retries: 3,
                }),
                ready_check: Some(ReadyCheck {
                    check_type: HealthCheckType::TcpConnect("127.0.0.1".into(), 5432),
                    timeout_ms: 10_000,
                    retries: 15,
                    retry_delay_ms: 500,
                }),
            },
            ServiceDefinition {
                name: "redis".into(),
                description: "Redis 7 in-memory cache".into(),
                binary_path: PathBuf::from("/usr/bin/redis-server"),
                args: vec!["/etc/redis/redis.conf".into()],
                environment: HashMap::new(),
                depends_on: vec![],
                required_for_modes: vec![BootMode::Server, BootMode::Desktop],
                restart_policy: RestartPolicy::Always,
                health_check: Some(HealthCheck {
                    check_type: HealthCheckType::TcpConnect("127.0.0.1".into(), 6379),
                    interval_ms: 10_000,
                    timeout_ms: 1000,
                    retries: 3,
                }),
                ready_check: Some(ReadyCheck {
                    check_type: HealthCheckType::TcpConnect("127.0.0.1".into(), 6379),
                    timeout_ms: 5000,
                    retries: 10,
                    retry_delay_ms: 200,
                }),
            },
        ]
    }

    /// Return the Synapse LLM management and training service definition.
    pub fn synapse_service() -> ServiceDefinition {
        ServiceDefinition {
            name: "synapse".into(),
            description: "Synapse LLM management and training service".into(),
            binary_path: PathBuf::from("/usr/lib/synapse/bin/synapse"),
            args: vec![
                "serve".into(),
                "--config".into(),
                "/etc/synapse/synapse.toml".into(),
            ],
            environment: {
                let mut env = HashMap::new();
                env.insert("SYNAPSE_DATA_DIR".into(), "/var/lib/synapse".into());
                env.insert("SYNAPSE_MODEL_DIR".into(), "/var/lib/synapse/models".into());
                env
            },
            depends_on: vec!["agent-runtime".into(), "llm-gateway".into()],
            required_for_modes: vec![BootMode::Server, BootMode::Desktop],
            restart_policy: RestartPolicy::OnFailure,
            health_check: Some(HealthCheck {
                check_type: HealthCheckType::HttpGet("http://127.0.0.1:8080/health".into()),
                interval_ms: 15_000,
                timeout_ms: 3000,
                retries: 3,
            }),
            ready_check: Some(ReadyCheck {
                check_type: HealthCheckType::TcpConnect("127.0.0.1".into(), 8080),
                timeout_ms: 10_000,
                retries: 15,
                retry_delay_ms: 500,
            }),
        }
    }

    /// Return a `ServiceDefinition` for the Shruti DAW.
    ///
    /// Shruti is **not** auto-started — users opt-in by adding the
    /// definition to `ArgonautConfig.services` or enabling it at
    /// runtime via `enable_optional_service("shruti")`.
    pub fn shruti_service() -> ServiceDefinition {
        ServiceDefinition {
            name: "shruti".into(),
            description: "Shruti digital audio workstation".into(),
            binary_path: PathBuf::from("/usr/local/bin/shruti"),
            args: vec![],
            environment: {
                let mut env = HashMap::new();
                env.insert(
                    "SHRUTI_DATA_DIR".into(),
                    "/home/${USER}/.local/share/shruti".into(),
                );
                env.insert("PIPEWIRE_RUNTIME_DIR".into(), "/run/user/1000".into());
                env
            },
            depends_on: vec!["agent-runtime".into(), "aethersafha".into()],
            required_for_modes: vec![], // never auto-started
            restart_policy: RestartPolicy::OnFailure,
            health_check: Some(HealthCheck {
                check_type: HealthCheckType::ProcessAlive,
                interval_ms: 10_000,
                timeout_ms: 1000,
                retries: 3,
            }),
            ready_check: None,
        }
    }

    /// Return the optional service catalogue. These services are not
    /// started by default but can be enabled by the user.
    pub fn optional_services() -> Vec<ServiceDefinition> {
        vec![Self::shruti_service()]
    }

    /// Look up an optional service by name.
    pub fn optional_service(name: &str) -> Option<ServiceDefinition> {
        Self::optional_services()
            .into_iter()
            .find(|s| s.name == name)
    }

    /// Enable an optional service at runtime by inserting its
    /// definition into the managed service set. Returns `true` if the
    /// service was newly inserted, `false` if it was already present.
    pub fn enable_optional_service(&mut self, name: &str) -> bool {
        if self.services.contains_key(name) {
            return false;
        }
        if let Some(def) = Self::optional_service(name) {
            let managed = ManagedService {
                definition: def.clone(),
                state: ServiceState::Stopped,
                pid: None,
                started_at: None,
                restart_count: 0,
                last_health_check: None,
            };
            self.services.insert(def.name.clone(), managed);
            info!(service = name, "enabled optional service");
            true
        } else {
            warn!(service = name, "unknown optional service");
            false
        }
    }

    /// Return the default AGNOS services for a boot mode.
    pub fn default_services(mode: BootMode) -> Vec<ServiceDefinition> {
        let mut services = Vec::new();

        // Edge mode: agent-runtime only, no database dependencies.
        if mode == BootMode::Edge {
            services.push(ServiceDefinition {
                name: "agent-runtime".into(),
                description: "Daimon agent orchestrator (edge mode)".into(),
                binary_path: PathBuf::from("/usr/lib/agnos/agent_runtime"),
                args: vec![
                    "--port".into(),
                    "8090".into(),
                    "--mode".into(),
                    "edge".into(),
                ],
                environment: {
                    let mut env = HashMap::new();
                    env.insert("AGNOS_EDGE_MODE".into(), "1".into());
                    env.insert("AGNOS_READONLY_ROOTFS".into(), "1".into());
                    env.insert("AGNOS_EDGE_LUKS".into(), "1".into());
                    env
                },
                depends_on: vec![],
                required_for_modes: vec![BootMode::Edge],
                restart_policy: RestartPolicy::Always,
                health_check: Some(HealthCheck {
                    check_type: HealthCheckType::HttpGet("http://127.0.0.1:8090/v1/health".into()),
                    interval_ms: 10_000,
                    timeout_ms: 2000,
                    retries: 3,
                }),
                ready_check: Some(ReadyCheck {
                    check_type: HealthCheckType::TcpConnect("127.0.0.1".into(), 8090),
                    timeout_ms: 3000,
                    retries: 5,
                    retry_delay_ms: 200,
                }),
            });
            return services;
        }

        // Database services for Server and Desktop modes.
        if mode == BootMode::Server || mode == BootMode::Desktop {
            services.extend(Self::database_services());
        }

        // agent-runtime is always present.
        // In Server/Desktop modes it depends on database services.
        let db_deps = if mode == BootMode::Server || mode == BootMode::Desktop {
            vec!["postgres".into(), "redis".into()]
        } else {
            vec![]
        };
        services.push(ServiceDefinition {
            name: "agent-runtime".into(),
            description: "Daimon agent orchestrator".into(),
            binary_path: PathBuf::from("/usr/lib/agnos/agent_runtime"),
            args: vec!["--port".into(), "8090".into()],
            environment: HashMap::new(),
            depends_on: db_deps,
            required_for_modes: vec![BootMode::Minimal, BootMode::Server, BootMode::Desktop],
            restart_policy: RestartPolicy::Always,
            health_check: Some(HealthCheck {
                check_type: HealthCheckType::HttpGet("http://127.0.0.1:8090/v1/health".into()),
                interval_ms: 10_000,
                timeout_ms: 2000,
                retries: 3,
            }),
            ready_check: Some(ReadyCheck {
                check_type: HealthCheckType::TcpConnect("127.0.0.1".into(), 8090),
                timeout_ms: 5000,
                retries: 10,
                retry_delay_ms: 200,
            }),
        });

        if mode == BootMode::Server || mode == BootMode::Desktop {
            services.push(ServiceDefinition {
                name: "llm-gateway".into(),
                description: "Hoosh LLM inference gateway".into(),
                binary_path: PathBuf::from("/usr/lib/agnos/llm_gateway"),
                args: vec!["--port".into(), "8088".into()],
                environment: HashMap::new(),
                depends_on: vec!["agent-runtime".into()],
                required_for_modes: vec![BootMode::Server, BootMode::Desktop],
                restart_policy: RestartPolicy::OnFailure,
                health_check: Some(HealthCheck {
                    check_type: HealthCheckType::HttpGet("http://127.0.0.1:8088/health".into()),
                    interval_ms: 15_000,
                    timeout_ms: 2000,
                    retries: 3,
                }),
                ready_check: Some(ReadyCheck {
                    check_type: HealthCheckType::TcpConnect("127.0.0.1".into(), 8088),
                    timeout_ms: 5000,
                    retries: 10,
                    retry_delay_ms: 200,
                }),
            });
            services.push(Self::synapse_service());
        }

        if mode == BootMode::Desktop {
            services.push(ServiceDefinition {
                name: "aethersafha".into(),
                description: "Wayland compositor".into(),
                binary_path: PathBuf::from("/usr/lib/agnos/aethersafha"),
                args: vec![],
                environment: {
                    let mut env = HashMap::new();
                    env.insert("XDG_SESSION_TYPE".into(), "wayland".into());
                    env
                },
                depends_on: vec!["agent-runtime".into()],
                required_for_modes: vec![BootMode::Desktop],
                restart_policy: RestartPolicy::Always,
                health_check: Some(HealthCheck {
                    check_type: HealthCheckType::ProcessAlive,
                    interval_ms: 5000,
                    timeout_ms: 1000,
                    retries: 2,
                }),
                ready_check: None,
            });

            services.push(ServiceDefinition {
                name: "agnoshi".into(),
                description: "AI terminal shell".into(),
                binary_path: PathBuf::from("/usr/lib/agnos/agnoshi"),
                args: vec![],
                environment: HashMap::new(),
                depends_on: vec!["agent-runtime".into(), "aethersafha".into()],
                required_for_modes: vec![BootMode::Desktop],
                restart_policy: RestartPolicy::OnFailure,
                health_check: Some(HealthCheck {
                    check_type: HealthCheckType::ProcessAlive,
                    interval_ms: 10_000,
                    timeout_ms: 1000,
                    retries: 3,
                }),
                ready_check: None,
            });
        }

        services
    }

    /// Topological sort of service names by `depends_on`. Returns an
    /// ordered list of service names such that every service appears
    /// after its dependencies. Detects cycles and returns an error.
    pub fn resolve_service_order(services: &[ServiceDefinition]) -> Result<Vec<String>> {
        let name_set: HashMap<&str, &ServiceDefinition> =
            services.iter().map(|s| (s.name.as_str(), s)).collect();

        // Kahn's algorithm.
        let mut in_degree: HashMap<&str, usize> = HashMap::new();
        let mut dependents: HashMap<&str, Vec<&str>> = HashMap::new();

        for svc in services {
            in_degree.entry(svc.name.as_str()).or_insert(0);
            for dep in &svc.depends_on {
                if !name_set.contains_key(dep.as_str()) {
                    bail!(
                        "service '{}' depends on '{}' which is not defined",
                        svc.name,
                        dep
                    );
                }
                *in_degree.entry(svc.name.as_str()).or_insert(0) += 1;
                dependents
                    .entry(dep.as_str())
                    .or_default()
                    .push(svc.name.as_str());
            }
        }

        let mut queue: std::collections::BinaryHeap<std::cmp::Reverse<&str>> = in_degree
            .iter()
            .filter(|(_, &deg)| deg == 0)
            .map(|(&name, _)| std::cmp::Reverse(name))
            .collect();

        let mut ordered: Vec<String> = Vec::new();

        while let Some(std::cmp::Reverse(current)) = queue.pop() {
            ordered.push(current.to_string());
            if let Some(deps) = dependents.get(current) {
                for &dep in deps {
                    if let Some(deg) = in_degree.get_mut(dep) {
                        *deg = deg.saturating_sub(1);
                        if *deg == 0 {
                            queue.push(std::cmp::Reverse(dep));
                        }
                    }
                }
            }
        }

        if ordered.len() != services.len() {
            bail!(
                "cycle detected in service dependencies — resolved {} of {} services",
                ordered.len(),
                services.len()
            );
        }

        debug!(order = ?ordered, "resolved service start order");
        Ok(ordered)
    }

    /// Register a new service definition. If a service with the same
    /// name already exists, the definition is updated but runtime state
    /// (state, pid, restart_count, etc.) is preserved.
    pub fn register_service(&mut self, definition: ServiceDefinition) {
        let name = definition.name.clone();
        if let Some(existing) = self.services.get_mut(&name) {
            warn!(service = %name, "service already registered — updating definition, preserving state");
            existing.definition = definition;
        } else {
            let managed = ManagedService {
                definition,
                state: ServiceState::Stopped,
                pid: None,
                started_at: None,
                restart_count: 0,
                last_health_check: None,
            };
            info!(service = %name, "registered service");
            self.services.insert(name, managed);
        }
    }

    /// Look up a managed service by name.
    pub fn get_service(&self, name: &str) -> Option<&ManagedService> {
        self.services.get(name)
    }

    /// Get the current state of a service.
    pub fn get_service_state(&self, name: &str) -> Option<&ServiceState> {
        self.services.get(name).map(|s| &s.state)
    }

    /// Transition a service to a new state. Returns `true` if the
    /// service exists and the transition is valid.
    ///
    /// Invalid transitions are rejected with a warning log and `false`
    /// return value. When transitioning to `Starting`, all services
    /// listed in `depends_on` must already be `Running`.
    pub fn set_service_state(&mut self, name: &str, state: ServiceState) -> bool {
        // First, validate the transition and dependency constraints
        // without holding a mutable borrow.
        let validation = {
            if let Some(svc) = self.services.get(name) {
                if !svc.state.valid_transition(&state) {
                    warn!(
                        service = %name,
                        from = %svc.state,
                        to = %state,
                        "invalid state transition"
                    );
                    return false;
                }
                // When transitioning to Starting, check that all
                // dependencies are Running.
                if matches!(state, ServiceState::Starting) {
                    let mut unmet = Vec::new();
                    for dep in &svc.definition.depends_on {
                        let dep_running = self
                            .services
                            .get(dep.as_str())
                            .map(|d| d.state == ServiceState::Running)
                            .unwrap_or(false);
                        if !dep_running {
                            unmet.push(dep.clone());
                        }
                    }
                    if !unmet.is_empty() {
                        warn!(
                            service = %name,
                            unmet_deps = ?unmet,
                            "cannot start: dependencies not running"
                        );
                        return false;
                    }
                }
                true
            } else {
                warn!(service = %name, "set_service_state: unknown service");
                return false;
            }
        };

        if validation {
            if let Some(svc) = self.services.get_mut(name) {
                debug!(service = %name, from = %svc.state, to = %state, "state transition");
                svc.state = state;
            }
        }
        validation
    }

    /// Return service definitions that are required for the given mode.
    pub fn services_for_mode(&self, mode: &BootMode) -> Vec<&ServiceDefinition> {
        self.services
            .values()
            .filter(|s| s.definition.required_for_modes.contains(mode))
            .map(|s| &s.definition)
            .collect()
    }

    /// Mark a boot stage as complete. Returns `true` if the stage was
    /// found and updated.
    pub fn mark_step_complete(&mut self, stage: BootStage) -> bool {
        if let Some(step) = self.boot_sequence.iter_mut().find(|s| s.stage == stage) {
            // Ensure started_at is populated.
            if step.started_at.is_none() {
                step.started_at = Some(Utc::now());
            }
            step.status = BootStepStatus::Complete;
            step.completed_at = Some(Utc::now());
            info!(stage = %stage, "boot step complete");

            // Set boot_started on the first step completion if not yet set.
            if self.boot_started.is_none() {
                self.boot_started = Some(Utc::now());
            }

            // If this was the BootComplete stage, mark the overall boot time.
            if stage == BootStage::BootComplete {
                self.boot_completed = Some(Utc::now());
            }
            true
        } else {
            false
        }
    }

    /// Mark a boot stage as failed. Returns `true` if the stage was
    /// found and updated.
    pub fn mark_step_failed(&mut self, stage: BootStage, error: String) -> bool {
        if let Some(step) = self.boot_sequence.iter_mut().find(|s| s.stage == stage) {
            warn!(stage = %stage, error = %error, "boot step failed");
            // Ensure started_at is populated.
            if step.started_at.is_none() {
                step.started_at = Some(Utc::now());
            }
            step.status = BootStepStatus::Failed;
            step.completed_at = Some(Utc::now());
            step.error = Some(error);

            // Set boot_started on the first step if not yet set.
            if self.boot_started.is_none() {
                self.boot_started = Some(Utc::now());
            }
            true
        } else {
            false
        }
    }

    /// The first boot step that is not yet complete or failed.
    pub fn current_stage(&self) -> Option<&BootStep> {
        self.boot_sequence.iter().find(|s| {
            s.status != BootStepStatus::Complete
                && s.status != BootStepStatus::Failed
                && s.status != BootStepStatus::Skipped
        })
    }

    /// Whether every boot step has completed (or been skipped).
    pub fn is_boot_complete(&self) -> bool {
        self.boot_sequence.iter().all(|s| {
            s.status == BootStepStatus::Complete
                || s.status == BootStepStatus::Skipped
                || (!s.required && s.status == BootStepStatus::Failed)
        })
    }

    /// Total boot duration in milliseconds, if boot has completed.
    pub fn boot_duration_ms(&self) -> Option<u64> {
        match (self.boot_started, self.boot_completed) {
            (Some(start), Some(end)) => {
                let dur = end.signed_duration_since(start);
                Some(dur.num_milliseconds().max(0) as u64)
            }
            _ => None,
        }
    }

    /// All boot steps that have failed.
    pub fn failed_steps(&self) -> Vec<&BootStep> {
        self.boot_sequence
            .iter()
            .filter(|s| s.status == BootStepStatus::Failed)
            .collect()
    }

    /// Return service names in shutdown order (reverse of startup order).
    /// Returns an error if dependency resolution fails (e.g. cycles).
    pub fn shutdown_order(&self) -> Result<Vec<String>> {
        let definitions: Vec<ServiceDefinition> = self
            .services
            .values()
            .map(|s| s.definition.clone())
            .collect();
        let mut order = Self::resolve_service_order(&definitions)?;
        order.reverse();
        Ok(order)
    }

    /// Collect current statistics.
    pub fn stats(&self) -> ArgonautStats {
        let services_running = self
            .services
            .values()
            .filter(|s| s.state == ServiceState::Running)
            .count();
        let services_failed = self
            .services
            .values()
            .filter(|s| matches!(s.state, ServiceState::Failed(_)))
            .count();
        let total_restarts: u32 = self.services.values().map(|s| s.restart_count).sum();

        ArgonautStats {
            boot_mode: self.config.boot_mode,
            boot_duration_ms: self.boot_duration_ms(),
            services_running,
            services_failed,
            total_restarts,
            boot_complete: self.is_boot_complete(),
        }
    }
}

// ---------------------------------------------------------------------------
// Runlevel — runtime boot mode switching
// ---------------------------------------------------------------------------

/// Runlevel represents a system operational state, analogous to SysV runlevels
/// but mapped to AGNOS boot modes. Supports runtime switching.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Runlevel {
    /// Emergency: single-user, no services, drop to agnoshi shell.
    Emergency,
    /// Rescue: basic services only, network down, for recovery.
    Rescue,
    /// Console: multi-user text mode (equivalent to BootMode::Server).
    Console,
    /// Graphical: full desktop (equivalent to BootMode::Desktop).
    Graphical,
    /// Container: minimal services for container/embedded use.
    Container,
    /// Edge: constrained IoT/edge device, agent-runtime only.
    Edge,
}

impl Runlevel {
    /// Map a runlevel to the services that should be running.
    pub fn to_boot_mode(self) -> Option<BootMode> {
        match self {
            Self::Emergency | Self::Rescue => None,
            Self::Console => Some(BootMode::Server),
            Self::Graphical => Some(BootMode::Desktop),
            Self::Container => Some(BootMode::Minimal),
            Self::Edge => Some(BootMode::Edge),
        }
    }

    /// Map a boot mode to the corresponding runlevel.
    pub fn from_boot_mode(mode: BootMode) -> Self {
        match mode {
            BootMode::Server => Self::Console,
            BootMode::Desktop => Self::Graphical,
            BootMode::Minimal => Self::Container,
            BootMode::Edge => Self::Edge,
        }
    }

    /// Numeric level for display (compatible with SysV conventions).
    pub fn level(self) -> u8 {
        match self {
            Self::Emergency => 0,
            Self::Rescue => 1,
            Self::Console => 3,
            Self::Graphical => 5,
            Self::Container => 7,
            Self::Edge => 8,
        }
    }
}

impl fmt::Display for Runlevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Emergency => write!(f, "emergency"),
            Self::Rescue => write!(f, "rescue"),
            Self::Console => write!(f, "console"),
            Self::Graphical => write!(f, "graphical"),
            Self::Container => write!(f, "container"),
            Self::Edge => write!(f, "edge"),
        }
    }
}

// ---------------------------------------------------------------------------
// Service target — grouping services for coordinated lifecycle
// ---------------------------------------------------------------------------

/// A target groups related services that should start/stop together.
/// Analogous to systemd targets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceTarget {
    pub name: String,
    pub description: String,
    /// Services that MUST be running for this target to be met.
    pub requires: Vec<String>,
    /// Services that SHOULD be running but aren't fatal if missing.
    pub wants: Vec<String>,
    /// Runlevels where this target is active.
    pub active_in: Vec<Runlevel>,
}

impl ServiceTarget {
    /// Predefined targets for AGNOS.
    pub fn defaults() -> Vec<Self> {
        vec![
            Self {
                name: "basic".into(),
                description: "Basic system services".into(),
                requires: vec!["eudev".into(), "dbus".into(), "syslogd".into()],
                wants: vec![],
                active_in: vec![
                    Runlevel::Rescue,
                    Runlevel::Console,
                    Runlevel::Graphical,
                    Runlevel::Container,
                ],
            },
            Self {
                name: "network".into(),
                description: "Network connectivity".into(),
                requires: vec!["networkmanager".into()],
                wants: vec!["nftables".into(), "openssh".into()],
                active_in: vec![Runlevel::Console, Runlevel::Graphical],
            },
            Self {
                name: "agnos-core".into(),
                description: "AGNOS agent runtime and LLM gateway".into(),
                requires: vec!["daimon".into()],
                wants: vec!["hoosh".into(), "aegis".into()],
                active_in: vec![Runlevel::Console, Runlevel::Graphical, Runlevel::Container],
            },
            Self {
                name: "graphical".into(),
                description: "Desktop environment".into(),
                requires: vec!["aethersafha".into()],
                wants: vec!["pipewire".into(), "agnoshi".into()],
                active_in: vec![Runlevel::Graphical],
            },
            Self {
                name: "edge".into(),
                description: "Edge device — minimal agent runtime".into(),
                requires: vec!["daimon".into()],
                wants: vec!["aegis".into()],
                active_in: vec![Runlevel::Edge],
            },
        ]
    }

    /// Check if this target is active in the given runlevel.
    pub fn is_active_in(&self, runlevel: Runlevel) -> bool {
        self.active_in.contains(&runlevel)
    }

    /// All services needed for this target (requires + wants).
    pub fn all_services(&self) -> Vec<&str> {
        self.requires
            .iter()
            .chain(self.wants.iter())
            .map(|s| s.as_str())
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Process execution types
// ---------------------------------------------------------------------------

/// Describes how a service process exited.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ExitStatus {
    /// Exited normally with the given code (0 = success).
    Code(i32),
    /// Killed by a signal (e.g. SIGTERM=15, SIGKILL=9).
    Signal(i32),
    /// Process hasn't exited yet.
    Running,
    /// Never started.
    NotStarted,
}

impl fmt::Display for ExitStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Code(c) => write!(f, "exit({})", c),
            Self::Signal(s) => write!(f, "signal({})", s),
            Self::Running => write!(f, "running"),
            Self::NotStarted => write!(f, "not-started"),
        }
    }
}

/// A service lifecycle event recorded in the audit log.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceEvent {
    pub timestamp: DateTime<Utc>,
    pub service: String,
    pub event_type: ServiceEventType,
    pub details: Option<String>,
}

/// Types of service lifecycle events.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ServiceEventType {
    Starting,
    Started { pid: u32 },
    HealthCheckPassed,
    HealthCheckFailed { consecutive: u32 },
    ReadyCheckPassed,
    ReadyCheckFailed,
    Stopping,
    Stopped { exit_status: ExitStatus },
    Restarting { restart_count: u32 },
    DependencyWaiting { dependency: String },
    DependencyMet { dependency: String },
    TimeoutKilled,
    CrashDetected { exit_status: ExitStatus },
}

impl fmt::Display for ServiceEventType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Starting => write!(f, "starting"),
            Self::Started { pid } => write!(f, "started(pid={})", pid),
            Self::HealthCheckPassed => write!(f, "health-ok"),
            Self::HealthCheckFailed { consecutive } => {
                write!(f, "health-fail({}x)", consecutive)
            }
            Self::ReadyCheckPassed => write!(f, "ready"),
            Self::ReadyCheckFailed => write!(f, "not-ready"),
            Self::Stopping => write!(f, "stopping"),
            Self::Stopped { exit_status } => write!(f, "stopped({})", exit_status),
            Self::Restarting { restart_count } => write!(f, "restarting(#{})", restart_count),
            Self::DependencyWaiting { dependency } => write!(f, "waiting({})", dependency),
            Self::DependencyMet { dependency } => write!(f, "dep-met({})", dependency),
            Self::TimeoutKilled => write!(f, "timeout-killed"),
            Self::CrashDetected { exit_status } => write!(f, "crash({})", exit_status),
        }
    }
}

// ---------------------------------------------------------------------------
// Shutdown orchestration
// ---------------------------------------------------------------------------

/// Shutdown type selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShutdownType {
    /// Clean shutdown and power off.
    Poweroff,
    /// Clean shutdown and reboot.
    Reboot,
    /// Halt the CPU without powering off.
    Halt,
    /// Kexec into a new kernel (fast reboot).
    Kexec,
}

impl fmt::Display for ShutdownType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Poweroff => write!(f, "poweroff"),
            Self::Reboot => write!(f, "reboot"),
            Self::Halt => write!(f, "halt"),
            Self::Kexec => write!(f, "kexec"),
        }
    }
}

/// A shutdown plan describing the ordered steps to cleanly shut down.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShutdownPlan {
    pub shutdown_type: ShutdownType,
    pub steps: Vec<ShutdownStep>,
    pub timeout_ms: u64,
    pub wall_message: Option<String>,
}

/// An individual shutdown step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShutdownStep {
    pub description: String,
    pub action: ShutdownAction,
    pub timeout_ms: u64,
    pub status: ShutdownStepStatus,
}

/// Actions performed during shutdown.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ShutdownAction {
    /// Broadcast wall message to all terminals.
    WallMessage(String),
    /// Signal agents to save state and disconnect.
    NotifyAgents,
    /// Send SIGTERM to a service, wait for graceful exit.
    StopService { name: String, signal: i32 },
    /// Force kill a service that didn't stop gracefully.
    ForceKillService { name: String },
    /// Flush filesystem buffers.
    SyncFilesystems,
    /// Unmount all filesystems.
    UnmountFilesystems,
    /// Deactivate swap.
    SwapOff,
    /// Deactivate LUKS volumes.
    CloseLuks,
    /// Final kernel call (reboot/poweroff/halt).
    KernelAction(ShutdownType),
}

/// Status of a shutdown step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ShutdownStepStatus {
    Pending,
    InProgress,
    Complete,
    Failed(String),
    Skipped,
}

impl fmt::Display for ShutdownStepStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::InProgress => write!(f, "in-progress"),
            Self::Complete => write!(f, "complete"),
            Self::Failed(e) => write!(f, "failed: {}", e),
            Self::Skipped => write!(f, "skipped"),
        }
    }
}

impl ArgonautInit {
    /// Build a shutdown plan for the given shutdown type.
    /// Services are stopped in reverse dependency order.
    pub fn plan_shutdown(&self, shutdown_type: ShutdownType) -> Result<ShutdownPlan> {
        let service_order = self.shutdown_order()?;
        let mut steps = Vec::new();

        // Step 1: Wall message
        let wall_msg = format!(
            "AGNOS system {} in {} seconds",
            shutdown_type,
            self.config.shutdown_timeout_ms / 1000
        );
        steps.push(ShutdownStep {
            description: "Broadcast shutdown warning".into(),
            action: ShutdownAction::WallMessage(wall_msg.clone()),
            timeout_ms: 1000,
            status: ShutdownStepStatus::Pending,
        });

        // Step 2: Notify agents to save state
        steps.push(ShutdownStep {
            description: "Notify agents to save state".into(),
            action: ShutdownAction::NotifyAgents,
            timeout_ms: 5000,
            status: ShutdownStepStatus::Pending,
        });

        // Step 3: Stop services in reverse dependency order
        for svc_name in &service_order {
            if let Some(svc) = self.services.get(svc_name) {
                if svc.state == ServiceState::Running || svc.state == ServiceState::Starting {
                    steps.push(ShutdownStep {
                        description: format!("Stop service: {}", svc_name),
                        action: ShutdownAction::StopService {
                            name: svc_name.clone(),
                            signal: 15, // SIGTERM
                        },
                        timeout_ms: 5000,
                        status: ShutdownStepStatus::Pending,
                    });
                }
            }
        }

        // Step 4: Sync filesystems
        steps.push(ShutdownStep {
            description: "Sync filesystem buffers".into(),
            action: ShutdownAction::SyncFilesystems,
            timeout_ms: 3000,
            status: ShutdownStepStatus::Pending,
        });

        // Step 5: Unmount
        steps.push(ShutdownStep {
            description: "Unmount filesystems".into(),
            action: ShutdownAction::UnmountFilesystems,
            timeout_ms: 5000,
            status: ShutdownStepStatus::Pending,
        });

        // Step 6: Deactivate swap
        steps.push(ShutdownStep {
            description: "Deactivate swap".into(),
            action: ShutdownAction::SwapOff,
            timeout_ms: 2000,
            status: ShutdownStepStatus::Pending,
        });

        // Step 7: Close LUKS volumes
        steps.push(ShutdownStep {
            description: "Close encrypted volumes".into(),
            action: ShutdownAction::CloseLuks,
            timeout_ms: 3000,
            status: ShutdownStepStatus::Pending,
        });

        // Step 8: Final kernel action
        steps.push(ShutdownStep {
            description: format!("Execute {}", shutdown_type),
            action: ShutdownAction::KernelAction(shutdown_type),
            timeout_ms: 1000,
            status: ShutdownStepStatus::Pending,
        });

        Ok(ShutdownPlan {
            shutdown_type,
            steps,
            timeout_ms: self.config.shutdown_timeout_ms,
            wall_message: Some(wall_msg),
        })
    }

    /// Compute which services need to start/stop when switching runlevels.
    pub fn plan_runlevel_switch(
        &self,
        target: Runlevel,
        targets: &[ServiceTarget],
    ) -> RunlevelSwitchPlan {
        let mut services_to_start = Vec::new();
        let mut services_to_stop = Vec::new();

        // Determine which services should be running at the target runlevel
        let mut desired: std::collections::HashSet<String> = std::collections::HashSet::new();
        for tgt in targets {
            if tgt.is_active_in(target) {
                for svc in &tgt.requires {
                    desired.insert(svc.clone());
                }
                for svc in &tgt.wants {
                    desired.insert(svc.clone());
                }
            }
        }

        // Services that need to start (desired but not running)
        for svc_name in &desired {
            if let Some(svc) = self.services.get(svc_name) {
                if svc.state != ServiceState::Running && svc.state != ServiceState::Starting {
                    services_to_start.push(svc_name.clone());
                }
            } else {
                // Service not registered, still mark it as needed
                services_to_start.push(svc_name.clone());
            }
        }

        // Services that need to stop (running but not desired)
        for (name, svc) in &self.services {
            if (svc.state == ServiceState::Running || svc.state == ServiceState::Starting)
                && !desired.contains(name)
            {
                services_to_stop.push(name.clone());
            }
        }

        // Emergency/rescue: stop everything except basic shell
        if target == Runlevel::Emergency {
            services_to_stop.clear();
            services_to_start.clear();
            for (name, svc) in &self.services {
                if svc.state == ServiceState::Running || svc.state == ServiceState::Starting {
                    services_to_stop.push(name.clone());
                }
            }
        }

        RunlevelSwitchPlan {
            from: Runlevel::from_boot_mode(self.config.boot_mode),
            to: target,
            services_to_start,
            services_to_stop,
            drop_to_shell: target == Runlevel::Emergency || target == Runlevel::Rescue,
        }
    }
}

/// Plan for switching between runlevels at runtime.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunlevelSwitchPlan {
    pub from: Runlevel,
    pub to: Runlevel,
    pub services_to_start: Vec<String>,
    pub services_to_stop: Vec<String>,
    pub drop_to_shell: bool,
}

// ---------------------------------------------------------------------------
// Health check execution logic
// ---------------------------------------------------------------------------

/// Result of executing a single health check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckResult {
    pub service: String,
    pub check_type: String,
    pub passed: bool,
    pub latency_ms: u64,
    pub message: Option<String>,
    pub checked_at: DateTime<Utc>,
}

/// Tracks consecutive health check failures for a service.
#[derive(Debug, Clone, Default)]
pub struct HealthTracker {
    /// Per-service consecutive failure count.
    failures: HashMap<String, u32>,
}

impl HealthTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a health check result. Returns true if the service should
    /// be restarted (consecutive failures >= threshold).
    pub fn record(&mut self, service: &str, passed: bool, threshold: u32) -> bool {
        if passed {
            self.failures.remove(service);
            false
        } else {
            let count = self.failures.entry(service.to_string()).or_insert(0);
            *count += 1;
            *count >= threshold
        }
    }

    /// Get current consecutive failure count for a service.
    pub fn failure_count(&self, service: &str) -> u32 {
        self.failures.get(service).copied().unwrap_or(0)
    }

    /// Reset tracking for a service (e.g. after restart).
    pub fn reset(&mut self, service: &str) {
        self.failures.remove(service);
    }
}

// ---------------------------------------------------------------------------
// Process spawn specification
// ---------------------------------------------------------------------------

/// Everything needed to spawn a service process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessSpec {
    pub binary: PathBuf,
    pub args: Vec<String>,
    pub environment: HashMap<String, String>,
    pub working_dir: Option<PathBuf>,
    pub stdout_log: Option<PathBuf>,
    pub stderr_log: Option<PathBuf>,
    pub uid: Option<u32>,
    pub gid: Option<u32>,
}

impl ProcessSpec {
    /// Build a ProcessSpec from a ServiceDefinition.
    pub fn from_service(def: &ServiceDefinition) -> Self {
        Self {
            binary: def.binary_path.clone(),
            args: def.args.clone(),
            environment: def.environment.clone(),
            working_dir: None,
            stdout_log: Some(PathBuf::from(format!(
                "/var/log/agnos/services/{}.log",
                def.name
            ))),
            stderr_log: Some(PathBuf::from(format!(
                "/var/log/agnos/services/{}.err",
                def.name
            ))),
            uid: None,
            gid: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Emergency shell
// ---------------------------------------------------------------------------

/// Configuration for the emergency recovery shell.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmergencyShellConfig {
    /// Path to the shell binary.
    pub shell_path: PathBuf,
    /// Environment variables for the emergency shell.
    pub environment: HashMap<String, String>,
    /// Message displayed before dropping to the shell.
    pub banner: String,
    /// Whether to require root password before granting access.
    pub require_auth: bool,
}

impl Default for EmergencyShellConfig {
    fn default() -> Self {
        let mut env = HashMap::new();
        env.insert("HOME".into(), "/root".into());
        env.insert("TERM".into(), "linux".into());
        env.insert("PATH".into(), "/usr/sbin:/usr/bin:/sbin:/bin".into());
        env.insert("SHELL".into(), "/usr/bin/agnoshi".into());

        Self {
            shell_path: PathBuf::from("/usr/bin/agnoshi"),
            environment: env,
            banner: concat!(
                "\n",
                "========================================\n",
                "  AGNOS Emergency Shell\n",
                "========================================\n",
                "\n",
                "The system has entered emergency mode.\n",
                "Use 'argonaut status' to check service state.\n",
                "Use 'argonaut start <service>' to start services.\n",
                "Use 'exit' to continue boot or 'reboot' to restart.\n",
                "\n",
            )
            .into(),
            require_auth: false,
        }
    }
}

impl ArgonautInit {
    /// Determine whether the system should drop to an emergency shell.
    /// Called after a critical boot step failure.
    pub fn should_drop_to_emergency(&self) -> bool {
        self.failed_steps()
            .iter()
            .any(|step| step.required && step.status == BootStepStatus::Failed)
    }

    /// Get the emergency shell configuration.
    pub fn emergency_shell_config(&self) -> EmergencyShellConfig {
        EmergencyShellConfig::default()
    }

    /// Record a service event in the audit log.
    pub fn record_event(&self, service: &str, event_type: ServiceEventType) -> ServiceEvent {
        let event = ServiceEvent {
            timestamp: Utc::now(),
            service: service.to_string(),
            event_type: event_type.clone(),
            details: None,
        };
        info!(
            service = service,
            event = %event_type,
            "service event"
        );
        event
    }

    /// Build a complete boot execution plan: resolve service order,
    /// create ProcessSpecs, and return the ordered list.
    pub fn boot_execution_plan(&self) -> Result<Vec<(String, ProcessSpec)>> {
        let definitions: Vec<ServiceDefinition> = self
            .services
            .values()
            .map(|s| s.definition.clone())
            .collect();
        let order = Self::resolve_service_order(&definitions)?;

        let plan: Vec<(String, ProcessSpec)> = order
            .into_iter()
            .filter_map(|name| {
                self.services.get(&name).map(|svc| {
                    let spec = ProcessSpec::from_service(&svc.definition);
                    (name, spec)
                })
            })
            .collect();

        Ok(plan)
    }

    /// Determine what action to take when a service crashes.
    pub fn on_service_crash(&self, service_name: &str, exit_status: &ExitStatus) -> CrashAction {
        let svc = match self.services.get(service_name) {
            Some(s) => s,
            None => return CrashAction::Ignore,
        };

        match svc.definition.restart_policy {
            RestartPolicy::Always => {
                if svc.restart_count >= 5 {
                    warn!(
                        service = service_name,
                        restarts = svc.restart_count,
                        "service exceeded restart limit"
                    );
                    CrashAction::GiveUp {
                        reason: format!("exceeded restart limit ({} restarts)", svc.restart_count),
                    }
                } else {
                    CrashAction::Restart {
                        delay_ms: backoff_delay(svc.restart_count),
                    }
                }
            }
            RestartPolicy::OnFailure => {
                if *exit_status == ExitStatus::Code(0) {
                    CrashAction::Ignore
                } else if svc.restart_count >= 5 {
                    CrashAction::GiveUp {
                        reason: format!(
                            "exceeded restart limit after failures ({} restarts)",
                            svc.restart_count
                        ),
                    }
                } else {
                    CrashAction::Restart {
                        delay_ms: backoff_delay(svc.restart_count),
                    }
                }
            }
            RestartPolicy::Never => CrashAction::Ignore,
        }
    }
}

/// Action to take when a service crashes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CrashAction {
    /// Restart the service after the given delay.
    Restart { delay_ms: u64 },
    /// Don't restart; service exited normally or policy is Never.
    Ignore,
    /// Stop trying to restart; too many failures.
    GiveUp { reason: String },
}

/// Exponential backoff delay for service restarts.
/// 1s, 2s, 4s, 8s, 16s (capped at 30s).
fn backoff_delay(restart_count: u32) -> u64 {
    let base: u64 = 1000;
    let delay = base * 2u64.saturating_pow(restart_count);
    delay.min(30_000)
}

// ---------------------------------------------------------------------------
// Read-only rootfs with dm-verity (Phase 14A-3/4)
// ---------------------------------------------------------------------------

/// Generate mount commands to configure a read-only root filesystem with
/// writable tmpfs overlays for directories that require writes at runtime.
///
/// This is used during Edge boot to lock down the root partition, ensuring
/// integrity while still allowing ephemeral writes to `/tmp`, `/var/run`,
/// `/var/log`, and `/var/tmp`.
pub fn configure_readonly_rootfs() -> Vec<String> {
    vec![
        "mount -o remount,ro /".to_string(),
        "mount -t tmpfs tmpfs /tmp -o size=64M,noexec,nosuid".to_string(),
        "mount -t tmpfs tmpfs /var/run -o size=16M,nosuid".to_string(),
        "mount -t tmpfs tmpfs /var/log -o size=32M,nosuid".to_string(),
        "mount -t tmpfs tmpfs /var/tmp -o size=16M,noexec,nosuid".to_string(),
    ]
}

/// Generate dm-verity verification and activation commands for a root device.
///
/// Validates the integrity of a root filesystem partition using dm-verity by
/// checking the provided SHA-256 root hash against the hash device, then
/// activating a read-only verified mapping at `/dev/mapper/verified-root`.
///
/// # Errors
///
/// Returns an error if any parameter is empty or the root hash is not exactly
/// 64 hex characters (SHA-256 digest length).
/// A structured command representation to avoid shell injection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SafeCommand {
    pub binary: String,
    pub args: Vec<String>,
}

impl SafeCommand {
    /// Format as a display string (for logging only — NOT for shell execution).
    pub fn display(&self) -> String {
        let mut parts = vec![self.binary.clone()];
        parts.extend(self.args.iter().cloned());
        parts.join(" ")
    }
}

/// Validate a device path to prevent injection.
fn validate_device_path(path: &str) -> Result<(), String> {
    if path.is_empty() {
        return Err("device path cannot be empty".to_string());
    }
    if !path.starts_with("/dev/") {
        return Err(format!(
            "device path must start with /dev/, got: {}",
            &path[..path.len().min(20)]
        ));
    }
    if !path
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '-' | '_' | '.'))
    {
        return Err("device path contains invalid characters".to_string());
    }
    Ok(())
}

pub fn verify_rootfs_integrity(
    root_device: &str,
    hash_device: &str,
    root_hash: &str,
) -> Result<Vec<SafeCommand>, String> {
    if root_device.is_empty() || hash_device.is_empty() || root_hash.is_empty() {
        return Err("dm-verity parameters cannot be empty".to_string());
    }
    if root_hash.len() != 64 {
        return Err("Root hash must be 64 hex characters (SHA-256)".to_string());
    }
    if !root_hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("Root hash must contain only hex characters (0-9, a-f)".to_string());
    }
    validate_device_path(root_device)?;
    validate_device_path(hash_device)?;

    Ok(vec![
        SafeCommand {
            binary: "veritysetup".to_string(),
            args: vec![
                "verify".to_string(),
                root_device.to_string(),
                hash_device.to_string(),
                root_hash.to_string(),
            ],
        },
        SafeCommand {
            binary: "veritysetup".to_string(),
            args: vec![
                "open".to_string(),
                root_device.to_string(),
                "verified-root".to_string(),
                hash_device.to_string(),
                root_hash.to_string(),
            ],
        },
        SafeCommand {
            binary: "mount".to_string(),
            args: vec![
                "-o".to_string(),
                "ro".to_string(),
                "/dev/mapper/verified-root".to_string(),
                "/mnt/root".to_string(),
            ],
        },
    ])
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // --- helpers ---

    fn minimal_config() -> ArgonautConfig {
        ArgonautConfig {
            boot_mode: BootMode::Minimal,
            ..Default::default()
        }
    }

    fn server_config() -> ArgonautConfig {
        ArgonautConfig {
            boot_mode: BootMode::Server,
            ..Default::default()
        }
    }

    fn desktop_config() -> ArgonautConfig {
        ArgonautConfig {
            boot_mode: BootMode::Desktop,
            ..Default::default()
        }
    }

    fn dummy_service(name: &str, deps: Vec<&str>) -> ServiceDefinition {
        ServiceDefinition {
            name: name.into(),
            description: format!("test service {name}"),
            binary_path: PathBuf::from(format!("/usr/bin/{name}")),
            args: vec![],
            environment: HashMap::new(),
            depends_on: deps.into_iter().map(String::from).collect(),
            required_for_modes: vec![BootMode::Minimal],
            restart_policy: RestartPolicy::Never,
            health_check: None,
            ready_check: None,
        }
    }

    // --- BootMode ---

    #[test]
    fn boot_mode_display_server() {
        assert_eq!(BootMode::Server.to_string(), "server");
    }

    #[test]
    fn boot_mode_display_desktop() {
        assert_eq!(BootMode::Desktop.to_string(), "desktop");
    }

    #[test]
    fn boot_mode_display_minimal() {
        assert_eq!(BootMode::Minimal.to_string(), "minimal");
    }

    #[test]
    fn boot_mode_display_edge() {
        assert_eq!(BootMode::Edge.to_string(), "edge");
    }

    // --- BootStage ordering ---

    #[test]
    fn boot_stage_ordering() {
        assert!(BootStage::MountFilesystems < BootStage::StartDeviceManager);
        assert!(BootStage::StartDeviceManager < BootStage::VerifyRootfs);
        assert!(BootStage::VerifyRootfs < BootStage::StartSecurity);
        assert!(BootStage::StartSecurity < BootStage::StartDatabaseServices);
        assert!(BootStage::StartDatabaseServices < BootStage::StartAgentRuntime);
        assert!(BootStage::StartAgentRuntime < BootStage::StartLlmGateway);
        assert!(BootStage::StartLlmGateway < BootStage::StartModelServices);
        assert!(BootStage::StartModelServices < BootStage::StartCompositor);
        assert!(BootStage::StartCompositor < BootStage::StartShell);
        assert!(BootStage::StartShell < BootStage::BootComplete);
    }

    #[test]
    fn boot_stage_display() {
        assert_eq!(BootStage::MountFilesystems.to_string(), "mount-filesystems");
        assert_eq!(BootStage::BootComplete.to_string(), "boot-complete");
    }

    // --- BootStepStatus ---

    #[test]
    fn boot_step_status_variants() {
        assert_eq!(BootStepStatus::Pending.to_string(), "pending");
        assert_eq!(BootStepStatus::Running.to_string(), "running");
        assert_eq!(BootStepStatus::Complete.to_string(), "complete");
        assert_eq!(BootStepStatus::Failed.to_string(), "failed");
        assert_eq!(BootStepStatus::Skipped.to_string(), "skipped");
    }

    // --- RestartPolicy ---

    #[test]
    fn restart_policy_display() {
        assert_eq!(RestartPolicy::Always.to_string(), "always");
        assert_eq!(RestartPolicy::OnFailure.to_string(), "on-failure");
        assert_eq!(RestartPolicy::Never.to_string(), "never");
    }

    // --- HealthCheckType ---

    #[test]
    fn health_check_type_variants() {
        let http = HealthCheckType::HttpGet("http://localhost/health".into());
        let tcp = HealthCheckType::TcpConnect("127.0.0.1".into(), 8080);
        let cmd = HealthCheckType::Command("systemctl is-active foo".into());
        let alive = HealthCheckType::ProcessAlive;

        assert!(matches!(http, HealthCheckType::HttpGet(_)));
        assert!(matches!(tcp, HealthCheckType::TcpConnect(_, 8080)));
        assert!(matches!(cmd, HealthCheckType::Command(_)));
        assert!(matches!(alive, HealthCheckType::ProcessAlive));
    }

    // --- Boot sequence per mode ---

    #[test]
    fn boot_sequence_minimal_no_compositor() {
        let steps = ArgonautInit::build_boot_sequence(BootMode::Minimal);
        let stages: Vec<BootStage> = steps.iter().map(|s| s.stage).collect();
        assert!(!stages.contains(&BootStage::StartCompositor));
        assert!(!stages.contains(&BootStage::StartShell));
        assert!(!stages.contains(&BootStage::StartLlmGateway));
        assert!(stages.contains(&BootStage::StartAgentRuntime));
        assert!(stages.contains(&BootStage::BootComplete));
    }

    #[test]
    fn boot_sequence_server_no_compositor() {
        let steps = ArgonautInit::build_boot_sequence(BootMode::Server);
        let stages: Vec<BootStage> = steps.iter().map(|s| s.stage).collect();
        assert!(!stages.contains(&BootStage::StartCompositor));
        assert!(!stages.contains(&BootStage::StartShell));
        assert!(stages.contains(&BootStage::StartLlmGateway));
        assert!(stages.contains(&BootStage::StartAgentRuntime));
    }

    #[test]
    fn boot_sequence_desktop_all_stages() {
        let steps = ArgonautInit::build_boot_sequence(BootMode::Desktop);
        let stages: Vec<BootStage> = steps.iter().map(|s| s.stage).collect();
        assert!(stages.contains(&BootStage::StartCompositor));
        assert!(stages.contains(&BootStage::StartShell));
        assert!(stages.contains(&BootStage::StartLlmGateway));
        assert!(stages.contains(&BootStage::StartAgentRuntime));
        assert!(stages.contains(&BootStage::BootComplete));
    }

    #[test]
    fn boot_sequence_step_count_minimal() {
        // MountFS, DevMgr, Verify, Security, AgentRuntime, BootComplete = 6
        let steps = ArgonautInit::build_boot_sequence(BootMode::Minimal);
        assert_eq!(steps.len(), 6);
    }

    #[test]
    fn boot_sequence_step_count_server() {
        // 6 (minimal) + DatabaseServices + LlmGateway + ModelServices = 9
        let steps = ArgonautInit::build_boot_sequence(BootMode::Server);
        assert_eq!(steps.len(), 9);
    }

    #[test]
    fn boot_sequence_step_count_desktop() {
        // 9 (server) + Compositor + Shell = 11
        let steps = ArgonautInit::build_boot_sequence(BootMode::Desktop);
        assert_eq!(steps.len(), 11);
    }

    // --- Default services ---

    #[test]
    fn default_services_minimal() {
        let svcs = ArgonautInit::default_services(BootMode::Minimal);
        assert_eq!(svcs.len(), 1);
        assert_eq!(svcs[0].name, "agent-runtime");
    }

    #[test]
    fn default_services_server() {
        let svcs = ArgonautInit::default_services(BootMode::Server);
        assert_eq!(svcs.len(), 5);
        let names: Vec<&str> = svcs.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"agent-runtime"));
        assert!(names.contains(&"llm-gateway"));
    }

    #[test]
    fn default_services_desktop() {
        let svcs = ArgonautInit::default_services(BootMode::Desktop);
        assert_eq!(svcs.len(), 7);
        let names: Vec<&str> = svcs.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"agent-runtime"));
        assert!(names.contains(&"llm-gateway"));
        assert!(names.contains(&"synapse"));
        assert!(names.contains(&"aethersafha"));
        assert!(names.contains(&"agnoshi"));
    }

    // --- Service order resolution ---

    #[test]
    fn resolve_service_order_simple_chain() {
        let services = vec![
            dummy_service("c", vec!["b"]),
            dummy_service("b", vec!["a"]),
            dummy_service("a", vec![]),
        ];
        let order = ArgonautInit::resolve_service_order(&services).unwrap();
        assert_eq!(order, vec!["a", "b", "c"]);
    }

    #[test]
    fn resolve_service_order_independent() {
        let services = vec![
            dummy_service("alpha", vec![]),
            dummy_service("beta", vec![]),
            dummy_service("gamma", vec![]),
        ];
        let order = ArgonautInit::resolve_service_order(&services).unwrap();
        assert_eq!(order.len(), 3);
        // All independent — any valid topological order contains all three.
        assert!(order.contains(&"alpha".to_string()));
        assert!(order.contains(&"beta".to_string()));
        assert!(order.contains(&"gamma".to_string()));
    }

    #[test]
    fn resolve_service_order_cycle_detection() {
        let services = vec![dummy_service("a", vec!["b"]), dummy_service("b", vec!["a"])];
        let result = ArgonautInit::resolve_service_order(&services);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("cycle detected"));
    }

    // --- Register and get service ---

    #[test]
    fn register_and_get_service() {
        let mut init = ArgonautInit::new(minimal_config());
        let svc = dummy_service("my-service", vec![]);
        init.register_service(svc);
        let got = init.get_service("my-service");
        assert!(got.is_some());
        assert_eq!(got.unwrap().definition.name, "my-service");
        assert_eq!(got.unwrap().state, ServiceState::Stopped);
    }

    #[test]
    fn get_service_not_found() {
        let init = ArgonautInit::new(minimal_config());
        assert!(init.get_service("nonexistent").is_none());
    }

    // --- Service state transitions ---

    #[test]
    fn set_service_state_valid_transitions() {
        let mut init = ArgonautInit::new(minimal_config());
        // Stopped → Starting (agent-runtime has no deps)
        assert!(init.set_service_state("agent-runtime", ServiceState::Starting));
        assert_eq!(
            init.get_service_state("agent-runtime"),
            Some(&ServiceState::Starting)
        );
        // Starting → Running
        assert!(init.set_service_state("agent-runtime", ServiceState::Running));
        assert_eq!(
            init.get_service_state("agent-runtime"),
            Some(&ServiceState::Running)
        );
    }

    #[test]
    fn set_service_state_unknown_service() {
        let mut init = ArgonautInit::new(minimal_config());
        assert!(!init.set_service_state("nonexistent", ServiceState::Running));
    }

    // --- Boot step marking ---

    #[test]
    fn mark_step_complete() {
        let mut init = ArgonautInit::new(minimal_config());
        assert!(init.mark_step_complete(BootStage::MountFilesystems));
        let step = init
            .boot_sequence
            .iter()
            .find(|s| s.stage == BootStage::MountFilesystems)
            .unwrap();
        assert_eq!(step.status, BootStepStatus::Complete);
        assert!(step.completed_at.is_some());
    }

    #[test]
    fn mark_step_failed() {
        let mut init = ArgonautInit::new(minimal_config());
        assert!(init.mark_step_failed(BootStage::VerifyRootfs, "dm-verity mismatch".into()));
        let step = init
            .boot_sequence
            .iter()
            .find(|s| s.stage == BootStage::VerifyRootfs)
            .unwrap();
        assert_eq!(step.status, BootStepStatus::Failed);
        assert_eq!(step.error.as_deref(), Some("dm-verity mismatch"));
    }

    #[test]
    fn mark_step_nonexistent() {
        let mut init = ArgonautInit::new(minimal_config());
        // Minimal mode has no compositor stage.
        assert!(!init.mark_step_complete(BootStage::StartCompositor));
    }

    // --- Current stage ---

    #[test]
    fn current_stage_returns_first_pending() {
        let init = ArgonautInit::new(minimal_config());
        let current = init.current_stage().unwrap();
        assert_eq!(current.stage, BootStage::MountFilesystems);
    }

    #[test]
    fn current_stage_skips_complete() {
        let mut init = ArgonautInit::new(minimal_config());
        init.mark_step_complete(BootStage::MountFilesystems);
        let current = init.current_stage().unwrap();
        assert_eq!(current.stage, BootStage::StartDeviceManager);
    }

    // --- Boot complete ---

    #[test]
    fn is_boot_complete_all_complete() {
        let mut init = ArgonautInit::new(minimal_config());
        for step in &mut init.boot_sequence {
            step.status = BootStepStatus::Complete;
        }
        assert!(init.is_boot_complete());
    }

    #[test]
    fn is_boot_complete_required_failed() {
        let mut init = ArgonautInit::new(minimal_config());
        for step in &mut init.boot_sequence {
            if step.required {
                step.status = BootStepStatus::Failed;
            } else {
                step.status = BootStepStatus::Complete;
            }
        }
        // Required steps failed — boot is NOT complete.
        assert!(!init.is_boot_complete());
    }

    #[test]
    fn is_boot_complete_optional_failed_ok() {
        let mut init = ArgonautInit::new(desktop_config());
        for step in &mut init.boot_sequence {
            if step.required {
                step.status = BootStepStatus::Complete;
            } else {
                step.status = BootStepStatus::Failed;
            }
        }
        // Non-required failures are tolerated.
        assert!(init.is_boot_complete());
    }

    // --- Boot duration ---

    #[test]
    fn boot_duration_ms_calculation() {
        let mut init = ArgonautInit::new(minimal_config());
        let start = Utc::now();
        init.boot_started = Some(start);
        init.boot_completed = Some(start + chrono::Duration::milliseconds(1234));
        assert_eq!(init.boot_duration_ms(), Some(1234));
    }

    #[test]
    fn boot_duration_ms_not_complete() {
        let init = ArgonautInit::new(minimal_config());
        assert_eq!(init.boot_duration_ms(), None);
    }

    // --- Failed steps ---

    #[test]
    fn failed_steps_list() {
        let mut init = ArgonautInit::new(minimal_config());
        init.mark_step_failed(BootStage::VerifyRootfs, "bad hash".into());
        init.mark_step_failed(BootStage::StartSecurity, "seccomp err".into());
        let failed = init.failed_steps();
        assert_eq!(failed.len(), 2);
        let stages: Vec<BootStage> = failed.iter().map(|s| s.stage).collect();
        assert!(stages.contains(&BootStage::VerifyRootfs));
        assert!(stages.contains(&BootStage::StartSecurity));
    }

    // --- Shutdown order ---

    #[test]
    fn shutdown_order_is_reverse_of_startup() {
        let init = ArgonautInit::new(desktop_config());
        let definitions: Vec<ServiceDefinition> = init
            .services
            .values()
            .map(|s| s.definition.clone())
            .collect();
        let startup = ArgonautInit::resolve_service_order(&definitions).unwrap();
        let shutdown = init.shutdown_order().unwrap();
        let reversed_startup: Vec<String> = startup.into_iter().rev().collect();
        assert_eq!(shutdown, reversed_startup);
    }

    // --- ArgonautConfig defaults ---

    #[test]
    fn config_defaults() {
        let cfg = ArgonautConfig::default();
        assert_eq!(cfg.boot_mode, BootMode::Desktop);
        assert_eq!(cfg.boot_timeout_ms, 30_000);
        assert_eq!(cfg.shutdown_timeout_ms, 10_000);
        assert!(cfg.log_to_console);
        assert!(cfg.verify_on_boot);
        assert!(cfg.services.is_empty());
    }

    // --- ServiceDefinition with checks ---

    #[test]
    fn service_definition_with_health_check() {
        let svcs = ArgonautInit::default_services(BootMode::Minimal);
        let agent_rt = &svcs[0];
        assert!(agent_rt.health_check.is_some());
        let hc = agent_rt.health_check.as_ref().unwrap();
        assert!(matches!(hc.check_type, HealthCheckType::HttpGet(_)));
        assert_eq!(hc.retries, 3);
    }

    #[test]
    fn service_definition_with_ready_check() {
        let svcs = ArgonautInit::default_services(BootMode::Minimal);
        let agent_rt = &svcs[0];
        assert!(agent_rt.ready_check.is_some());
        let rc = agent_rt.ready_check.as_ref().unwrap();
        assert!(matches!(
            rc.check_type,
            HealthCheckType::TcpConnect(_, 8090)
        ));
        assert_eq!(rc.retry_delay_ms, 200);
    }

    // --- ManagedService initial state ---

    #[test]
    fn managed_service_initial_state() {
        let init = ArgonautInit::new(minimal_config());
        let svc = init.get_service("agent-runtime").unwrap();
        assert_eq!(svc.state, ServiceState::Stopped);
        assert!(svc.pid.is_none());
        assert!(svc.started_at.is_none());
        assert_eq!(svc.restart_count, 0);
        assert!(svc.last_health_check.is_none());
    }

    // --- services_for_mode ---

    #[test]
    fn services_for_mode_filtering() {
        let init = ArgonautInit::new(desktop_config());
        let minimal_svcs = init.services_for_mode(&BootMode::Minimal);
        // Only agent-runtime is required for Minimal.
        assert_eq!(minimal_svcs.len(), 1);
        assert_eq!(minimal_svcs[0].name, "agent-runtime");
    }

    #[test]
    fn services_for_mode_desktop() {
        let init = ArgonautInit::new(desktop_config());
        let desktop_svcs = init.services_for_mode(&BootMode::Desktop);
        assert_eq!(desktop_svcs.len(), 7);
    }

    // --- Stats ---

    #[test]
    fn stats_accuracy() {
        let mut init = ArgonautInit::new(server_config());
        // Start database services first (agent-runtime depends on them in server mode)
        assert!(init.set_service_state("postgres", ServiceState::Starting));
        assert!(init.set_service_state("postgres", ServiceState::Running));
        assert!(init.set_service_state("redis", ServiceState::Starting));
        assert!(init.set_service_state("redis", ServiceState::Running));
        // Valid transition path: Stopped → Starting → Running
        assert!(init.set_service_state("agent-runtime", ServiceState::Starting));
        assert!(init.set_service_state("agent-runtime", ServiceState::Running));
        // llm-gateway depends on agent-runtime which is now Running
        assert!(init.set_service_state("llm-gateway", ServiceState::Starting));
        assert!(init.set_service_state("llm-gateway", ServiceState::Failed("crash".into()),));
        if let Some(svc) = init.services.get_mut("agent-runtime") {
            svc.restart_count = 3;
        }
        let s = init.stats();
        assert_eq!(s.boot_mode, BootMode::Server);
        assert_eq!(s.services_running, 3);
        assert_eq!(s.services_failed, 1);
        assert_eq!(s.total_restarts, 3);
        assert!(!s.boot_complete);
    }

    #[test]
    fn stats_empty_init() {
        let init = ArgonautInit::new(minimal_config());
        let s = init.stats();
        assert_eq!(s.boot_mode, BootMode::Minimal);
        assert_eq!(s.services_running, 0);
        assert_eq!(s.services_failed, 0);
        assert_eq!(s.total_restarts, 0);
        assert!(!s.boot_complete);
        assert!(s.boot_duration_ms.is_none());
    }

    // --- Boot step timeout values ---

    #[test]
    fn boot_step_timeout_values() {
        let steps = ArgonautInit::build_boot_sequence(BootMode::Desktop);
        let fs_step = steps
            .iter()
            .find(|s| s.stage == BootStage::MountFilesystems)
            .unwrap();
        assert_eq!(fs_step.timeout_ms, 2000);
        let verify_step = steps
            .iter()
            .find(|s| s.stage == BootStage::VerifyRootfs)
            .unwrap();
        assert_eq!(verify_step.timeout_ms, 5000);
        let complete_step = steps
            .iter()
            .find(|s| s.stage == BootStage::BootComplete)
            .unwrap();
        assert_eq!(complete_step.timeout_ms, 1000);
    }

    // --- Depends-on resolution for default desktop ---

    #[test]
    fn service_depends_on_resolution_desktop() {
        let svcs = ArgonautInit::default_services(BootMode::Desktop);
        let order = ArgonautInit::resolve_service_order(&svcs).unwrap();
        let rt_pos = order.iter().position(|n| n == "agent-runtime").unwrap();
        let gw_pos = order.iter().position(|n| n == "llm-gateway").unwrap();
        let comp_pos = order.iter().position(|n| n == "aethersafha").unwrap();
        let shell_pos = order.iter().position(|n| n == "agnoshi").unwrap();
        // agent-runtime before everything.
        assert!(rt_pos < gw_pos);
        assert!(rt_pos < comp_pos);
        assert!(rt_pos < shell_pos);
        // aethersafha before agnoshi (shell depends on compositor).
        assert!(comp_pos < shell_pos);
    }

    // --- Audit fix tests ---

    #[test]
    fn invalid_state_transition_stopped_to_running() {
        let mut init = ArgonautInit::new(minimal_config());
        // Stopped → Running is not valid (must go through Starting)
        assert!(!init.set_service_state("agent-runtime", ServiceState::Running));
        // State should remain Stopped
        assert_eq!(
            init.get_service_state("agent-runtime"),
            Some(&ServiceState::Stopped)
        );
    }

    #[test]
    fn valid_state_transition_full_lifecycle() {
        let mut init = ArgonautInit::new(minimal_config());
        // Stopped → Starting → Running → Stopping → Stopped
        assert!(init.set_service_state("agent-runtime", ServiceState::Starting));
        assert!(init.set_service_state("agent-runtime", ServiceState::Running));
        assert!(init.set_service_state("agent-runtime", ServiceState::Stopping));
        assert!(init.set_service_state("agent-runtime", ServiceState::Stopped));
        // Failed → Starting (restart), Failed → Stopped
        assert!(init.set_service_state("agent-runtime", ServiceState::Starting));
        assert!(init.set_service_state("agent-runtime", ServiceState::Failed("err".into())));
        assert!(init.set_service_state("agent-runtime", ServiceState::Starting));
        assert!(init.set_service_state("agent-runtime", ServiceState::Failed("err2".into())));
        assert!(init.set_service_state("agent-runtime", ServiceState::Stopped));
    }

    #[test]
    fn starting_blocked_when_dependency_not_running() {
        let mut init = ArgonautInit::new(server_config());
        // llm-gateway depends on agent-runtime. agent-runtime is Stopped.
        assert!(!init.set_service_state("llm-gateway", ServiceState::Starting));
        // Start database services first (agent-runtime depends on them in server mode)
        assert!(init.set_service_state("postgres", ServiceState::Starting));
        assert!(init.set_service_state("postgres", ServiceState::Running));
        assert!(init.set_service_state("redis", ServiceState::Starting));
        assert!(init.set_service_state("redis", ServiceState::Running));
        // Start agent-runtime but leave it in Starting (not Running).
        assert!(init.set_service_state("agent-runtime", ServiceState::Starting));
        assert!(!init.set_service_state("llm-gateway", ServiceState::Starting));
        // Now make agent-runtime Running.
        assert!(init.set_service_state("agent-runtime", ServiceState::Running));
        assert!(init.set_service_state("llm-gateway", ServiceState::Starting));
    }

    #[test]
    fn register_service_overwrites_definition_preserves_state() {
        let mut init = ArgonautInit::new(minimal_config());
        let svc = dummy_service("my-svc", vec![]);
        init.register_service(svc);
        // Transition to Starting
        assert!(init.set_service_state("my-svc", ServiceState::Starting));
        assert!(init.set_service_state("my-svc", ServiceState::Running));
        if let Some(s) = init.services.get_mut("my-svc") {
            s.restart_count = 5;
        }
        // Re-register with updated description
        let mut svc2 = dummy_service("my-svc", vec![]);
        svc2.description = "updated description".into();
        init.register_service(svc2);
        let got = init.get_service("my-svc").unwrap();
        // Definition updated
        assert_eq!(got.definition.description, "updated description");
        // State preserved
        assert_eq!(got.state, ServiceState::Running);
        assert_eq!(got.restart_count, 5);
    }

    #[test]
    fn boot_started_set_after_first_step_completes() {
        let mut init = ArgonautInit::new(minimal_config());
        assert!(init.boot_started.is_none());
        init.mark_step_complete(BootStage::MountFilesystems);
        assert!(init.boot_started.is_some());
    }

    #[test]
    fn boot_started_set_after_first_step_fails() {
        let mut init = ArgonautInit::new(minimal_config());
        assert!(init.boot_started.is_none());
        init.mark_step_failed(BootStage::MountFilesystems, "fail".into());
        assert!(init.boot_started.is_some());
    }

    #[test]
    fn started_at_populated_on_mark_step_complete() {
        let mut init = ArgonautInit::new(minimal_config());
        init.mark_step_complete(BootStage::MountFilesystems);
        let step = init
            .boot_sequence
            .iter()
            .find(|s| s.stage == BootStage::MountFilesystems)
            .unwrap();
        assert!(step.started_at.is_some());
        assert!(step.completed_at.is_some());
        // started_at should be <= completed_at
        assert!(step.started_at.unwrap() <= step.completed_at.unwrap());
    }

    #[test]
    fn started_at_populated_on_mark_step_failed() {
        let mut init = ArgonautInit::new(minimal_config());
        init.mark_step_failed(BootStage::MountFilesystems, "oops".into());
        let step = init
            .boot_sequence
            .iter()
            .find(|s| s.stage == BootStage::MountFilesystems)
            .unwrap();
        assert!(step.started_at.is_some());
    }

    #[test]
    fn missing_dependency_returns_error() {
        let services = vec![dummy_service("a", vec!["nonexistent"])];
        let result = ArgonautInit::resolve_service_order(&services);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("depends on"));
        assert!(err.contains("nonexistent"));
        assert!(err.contains("not defined"));
    }

    #[test]
    fn shutdown_order_returns_error_on_cycle() {
        let mut init = ArgonautInit::new(minimal_config());
        // Create a cycle: x depends on y, y depends on x
        let svc_x = dummy_service("x", vec!["y"]);
        let svc_y = dummy_service("y", vec!["x"]);
        init.services.clear();
        init.services.insert(
            "x".into(),
            ManagedService {
                definition: svc_x,
                state: ServiceState::Stopped,
                pid: None,
                started_at: None,
                restart_count: 0,
                last_health_check: None,
            },
        );
        init.services.insert(
            "y".into(),
            ManagedService {
                definition: svc_y,
                state: ServiceState::Stopped,
                pid: None,
                started_at: None,
                restart_count: 0,
                last_health_check: None,
            },
        );
        let result = init.shutdown_order();
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("cycle detected"));
    }

    // --- Database services ---

    #[test]
    fn database_services_returns_postgres_and_redis() {
        let svcs = ArgonautInit::database_services();
        assert_eq!(svcs.len(), 2);
        let names: Vec<&str> = svcs.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"postgres"));
        assert!(names.contains(&"redis"));
    }

    #[test]
    fn database_services_health_checks() {
        let svcs = ArgonautInit::database_services();
        for svc in &svcs {
            assert!(svc.health_check.is_some());
            assert!(svc.ready_check.is_some());
        }
    }

    #[test]
    fn database_services_restart_policies() {
        let svcs = ArgonautInit::database_services();
        let pg = svcs.iter().find(|s| s.name == "postgres").unwrap();
        let redis = svcs.iter().find(|s| s.name == "redis").unwrap();
        assert_eq!(pg.restart_policy, RestartPolicy::OnFailure);
        assert_eq!(redis.restart_policy, RestartPolicy::Always);
    }

    #[test]
    fn database_services_modes() {
        let svcs = ArgonautInit::database_services();
        for svc in &svcs {
            assert!(svc.required_for_modes.contains(&BootMode::Server));
            assert!(svc.required_for_modes.contains(&BootMode::Desktop));
            assert!(!svc.required_for_modes.contains(&BootMode::Minimal));
        }
    }

    #[test]
    fn default_services_server_includes_databases() {
        let svcs = ArgonautInit::default_services(BootMode::Server);
        let names: Vec<&str> = svcs.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"postgres"));
        assert!(names.contains(&"redis"));
    }

    #[test]
    fn default_services_minimal_excludes_databases() {
        let svcs = ArgonautInit::default_services(BootMode::Minimal);
        let names: Vec<&str> = svcs.iter().map(|s| s.name.as_str()).collect();
        assert!(!names.contains(&"postgres"));
        assert!(!names.contains(&"redis"));
    }

    #[test]
    fn agent_runtime_depends_on_databases_in_server_mode() {
        let svcs = ArgonautInit::default_services(BootMode::Server);
        let rt = svcs.iter().find(|s| s.name == "agent-runtime").unwrap();
        assert!(rt.depends_on.contains(&"postgres".to_string()));
        assert!(rt.depends_on.contains(&"redis".to_string()));
    }

    #[test]
    fn agent_runtime_no_db_deps_in_minimal_mode() {
        let svcs = ArgonautInit::default_services(BootMode::Minimal);
        let rt = svcs.iter().find(|s| s.name == "agent-runtime").unwrap();
        assert!(rt.depends_on.is_empty());
    }

    #[test]
    fn boot_stage_database_ordering() {
        assert!(BootStage::StartSecurity < BootStage::StartDatabaseServices);
        assert!(BootStage::StartDatabaseServices < BootStage::StartAgentRuntime);
    }

    #[test]
    fn boot_stage_database_display() {
        assert_eq!(
            BootStage::StartDatabaseServices.to_string(),
            "start-database-services"
        );
    }

    #[test]
    fn boot_sequence_server_includes_database_stage() {
        let steps = ArgonautInit::build_boot_sequence(BootMode::Server);
        let stages: Vec<BootStage> = steps.iter().map(|s| s.stage).collect();
        assert!(stages.contains(&BootStage::StartDatabaseServices));
    }

    #[test]
    fn boot_sequence_minimal_excludes_database_stage() {
        let steps = ArgonautInit::build_boot_sequence(BootMode::Minimal);
        let stages: Vec<BootStage> = steps.iter().map(|s| s.stage).collect();
        assert!(!stages.contains(&BootStage::StartDatabaseServices));
    }

    // --- Synapse service ---

    #[test]
    fn synapse_service_definition() {
        let svc = ArgonautInit::synapse_service();
        assert_eq!(svc.name, "synapse");
        assert!(svc.depends_on.contains(&"agent-runtime".to_string()));
        assert!(svc.depends_on.contains(&"llm-gateway".to_string()));
        assert!(svc.health_check.is_some());
        assert!(svc.ready_check.is_some());
    }

    #[test]
    fn server_mode_includes_synapse() {
        let services = ArgonautInit::default_services(BootMode::Server);
        assert!(services.iter().any(|s| s.name == "synapse"));
    }

    #[test]
    fn desktop_mode_includes_synapse() {
        let services = ArgonautInit::default_services(BootMode::Desktop);
        assert!(services.iter().any(|s| s.name == "synapse"));
    }

    #[test]
    fn minimal_mode_excludes_synapse() {
        let services = ArgonautInit::default_services(BootMode::Minimal);
        assert!(!services.iter().any(|s| s.name == "synapse"));
    }

    #[test]
    fn synapse_starts_after_llm_gateway() {
        let services = ArgonautInit::default_services(BootMode::Server);
        let order = ArgonautInit::resolve_service_order(&services).unwrap();
        let gw_pos = order.iter().position(|s| s == "llm-gateway").unwrap();
        let syn_pos = order.iter().position(|s| s == "synapse").unwrap();
        assert!(syn_pos > gw_pos);
    }

    #[test]
    fn boot_sequence_includes_model_services_for_server() {
        let steps = ArgonautInit::build_boot_sequence(BootMode::Server);
        assert!(steps
            .iter()
            .any(|s| s.stage == BootStage::StartModelServices));
    }

    #[test]
    fn boot_sequence_excludes_model_services_for_minimal() {
        let steps = ArgonautInit::build_boot_sequence(BootMode::Minimal);
        assert!(!steps
            .iter()
            .any(|s| s.stage == BootStage::StartModelServices));
    }

    #[test]
    fn model_services_stage_after_llm_gateway() {
        assert!(BootStage::StartModelServices > BootStage::StartLlmGateway);
        assert!(BootStage::StartModelServices < BootStage::StartCompositor);
    }

    #[test]
    fn server_service_count_with_synapse() {
        let services = ArgonautInit::default_services(BootMode::Server);
        // postgres, redis, agent-runtime, llm-gateway, synapse = 5
        assert_eq!(services.len(), 5);
    }

    #[test]
    fn desktop_service_count_with_synapse() {
        let services = ArgonautInit::default_services(BootMode::Desktop);
        // postgres, redis, agent-runtime, llm-gateway, synapse, aethersafha, agnoshi = 7
        assert_eq!(services.len(), 7);
    }

    // -----------------------------------------------------------------------
    // Phase 12A: Runlevel tests
    // -----------------------------------------------------------------------

    #[test]
    fn runlevel_to_boot_mode() {
        assert_eq!(Runlevel::Console.to_boot_mode(), Some(BootMode::Server));
        assert_eq!(Runlevel::Graphical.to_boot_mode(), Some(BootMode::Desktop));
        assert_eq!(Runlevel::Container.to_boot_mode(), Some(BootMode::Minimal));
        assert_eq!(Runlevel::Emergency.to_boot_mode(), None);
        assert_eq!(Runlevel::Rescue.to_boot_mode(), None);
    }

    #[test]
    fn runlevel_from_boot_mode() {
        assert_eq!(
            Runlevel::from_boot_mode(BootMode::Server),
            Runlevel::Console
        );
        assert_eq!(
            Runlevel::from_boot_mode(BootMode::Desktop),
            Runlevel::Graphical
        );
        assert_eq!(
            Runlevel::from_boot_mode(BootMode::Minimal),
            Runlevel::Container
        );
    }

    #[test]
    fn runlevel_levels() {
        assert_eq!(Runlevel::Emergency.level(), 0);
        assert_eq!(Runlevel::Rescue.level(), 1);
        assert_eq!(Runlevel::Console.level(), 3);
        assert_eq!(Runlevel::Graphical.level(), 5);
        assert_eq!(Runlevel::Container.level(), 7);
    }

    #[test]
    fn runlevel_display() {
        assert_eq!(format!("{}", Runlevel::Emergency), "emergency");
        assert_eq!(format!("{}", Runlevel::Rescue), "rescue");
        assert_eq!(format!("{}", Runlevel::Console), "console");
        assert_eq!(format!("{}", Runlevel::Graphical), "graphical");
        assert_eq!(format!("{}", Runlevel::Container), "container");
    }

    // -----------------------------------------------------------------------
    // Service target tests
    // -----------------------------------------------------------------------

    #[test]
    fn default_targets_exist() {
        let targets = ServiceTarget::defaults();
        assert_eq!(targets.len(), 5);
        let names: Vec<&str> = targets.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"basic"));
        assert!(names.contains(&"network"));
        assert!(names.contains(&"agnos-core"));
        assert!(names.contains(&"graphical"));
        assert!(names.contains(&"edge"));
    }

    #[test]
    fn target_active_in_runlevel() {
        let targets = ServiceTarget::defaults();
        let basic = targets.iter().find(|t| t.name == "basic").unwrap();
        assert!(basic.is_active_in(Runlevel::Console));
        assert!(basic.is_active_in(Runlevel::Graphical));
        assert!(basic.is_active_in(Runlevel::Rescue));
        assert!(!basic.is_active_in(Runlevel::Emergency));

        let graphical = targets.iter().find(|t| t.name == "graphical").unwrap();
        assert!(graphical.is_active_in(Runlevel::Graphical));
        assert!(!graphical.is_active_in(Runlevel::Console));
    }

    #[test]
    fn target_all_services() {
        let targets = ServiceTarget::defaults();
        let network = targets.iter().find(|t| t.name == "network").unwrap();
        let svcs = network.all_services();
        assert!(svcs.contains(&"networkmanager"));
        assert!(svcs.contains(&"nftables"));
        assert!(svcs.contains(&"openssh"));
    }

    // -----------------------------------------------------------------------
    // Shutdown plan tests
    // -----------------------------------------------------------------------

    #[test]
    fn shutdown_plan_poweroff() {
        let config = ArgonautConfig::default();
        let init = ArgonautInit::new(config);
        let plan = init.plan_shutdown(ShutdownType::Poweroff).unwrap();
        assert_eq!(plan.shutdown_type, ShutdownType::Poweroff);
        assert!(!plan.steps.is_empty());
        // Last step should be KernelAction
        let last = plan.steps.last().unwrap();
        assert_eq!(
            last.action,
            ShutdownAction::KernelAction(ShutdownType::Poweroff)
        );
    }

    #[test]
    fn shutdown_plan_reboot() {
        let config = ArgonautConfig::default();
        let init = ArgonautInit::new(config);
        let plan = init.plan_shutdown(ShutdownType::Reboot).unwrap();
        assert_eq!(plan.shutdown_type, ShutdownType::Reboot);
        let last = plan.steps.last().unwrap();
        assert_eq!(
            last.action,
            ShutdownAction::KernelAction(ShutdownType::Reboot)
        );
    }

    #[test]
    fn shutdown_plan_includes_sync() {
        let config = ArgonautConfig::default();
        let init = ArgonautInit::new(config);
        let plan = init.plan_shutdown(ShutdownType::Poweroff).unwrap();
        assert!(plan
            .steps
            .iter()
            .any(|s| s.action == ShutdownAction::SyncFilesystems));
    }

    #[test]
    fn shutdown_plan_includes_unmount() {
        let config = ArgonautConfig::default();
        let init = ArgonautInit::new(config);
        let plan = init.plan_shutdown(ShutdownType::Halt).unwrap();
        assert!(plan
            .steps
            .iter()
            .any(|s| s.action == ShutdownAction::UnmountFilesystems));
    }

    #[test]
    fn shutdown_plan_has_wall_message() {
        let config = ArgonautConfig::default();
        let init = ArgonautInit::new(config);
        let plan = init.plan_shutdown(ShutdownType::Reboot).unwrap();
        assert!(plan.wall_message.is_some());
        assert!(plan.wall_message.unwrap().contains("reboot"));
    }

    #[test]
    fn shutdown_plan_stops_running_services() {
        let config = ArgonautConfig {
            boot_mode: BootMode::Minimal,
            ..ArgonautConfig::default()
        };
        let mut init = ArgonautInit::new(config);
        // Minimal has only agent-runtime with no deps
        init.set_service_state("agent-runtime", ServiceState::Starting);
        init.set_service_state("agent-runtime", ServiceState::Running);

        let plan = init.plan_shutdown(ShutdownType::Poweroff).unwrap();
        let stop_steps: Vec<_> = plan
            .steps
            .iter()
            .filter(|s| matches!(s.action, ShutdownAction::StopService { .. }))
            .collect();
        assert!(!stop_steps.is_empty());
    }

    #[test]
    fn shutdown_plan_includes_luks_close() {
        let config = ArgonautConfig::default();
        let init = ArgonautInit::new(config);
        let plan = init.plan_shutdown(ShutdownType::Poweroff).unwrap();
        assert!(plan
            .steps
            .iter()
            .any(|s| s.action == ShutdownAction::CloseLuks));
    }

    #[test]
    fn shutdown_type_display() {
        assert_eq!(format!("{}", ShutdownType::Poweroff), "poweroff");
        assert_eq!(format!("{}", ShutdownType::Reboot), "reboot");
        assert_eq!(format!("{}", ShutdownType::Halt), "halt");
        assert_eq!(format!("{}", ShutdownType::Kexec), "kexec");
    }

    #[test]
    fn shutdown_step_status_display() {
        assert_eq!(format!("{}", ShutdownStepStatus::Pending), "pending");
        assert_eq!(
            format!("{}", ShutdownStepStatus::Failed("disk busy".into())),
            "failed: disk busy"
        );
    }

    // -----------------------------------------------------------------------
    // Runlevel switch plan tests
    // -----------------------------------------------------------------------

    #[test]
    fn runlevel_switch_console_to_graphical() {
        let config = ArgonautConfig {
            boot_mode: BootMode::Server,
            ..ArgonautConfig::default()
        };
        let init = ArgonautInit::new(config);
        let targets = ServiceTarget::defaults();
        let plan = init.plan_runlevel_switch(Runlevel::Graphical, &targets);
        assert_eq!(plan.from, Runlevel::Console);
        assert_eq!(plan.to, Runlevel::Graphical);
        // Should want to start graphical services
        assert!(plan.services_to_start.contains(&"aethersafha".to_string()));
        assert!(!plan.drop_to_shell);
    }

    #[test]
    fn runlevel_switch_to_emergency_stops_all() {
        let config = ArgonautConfig {
            boot_mode: BootMode::Minimal,
            ..ArgonautConfig::default()
        };
        let mut init = ArgonautInit::new(config);
        // Minimal has agent-runtime with no deps, so state transition works
        init.set_service_state("agent-runtime", ServiceState::Starting);
        init.set_service_state("agent-runtime", ServiceState::Running);

        let targets = ServiceTarget::defaults();
        let plan = init.plan_runlevel_switch(Runlevel::Emergency, &targets);
        assert!(plan.drop_to_shell);
        assert!(plan.services_to_start.is_empty());
        // Should stop running services
        assert!(plan.services_to_stop.contains(&"agent-runtime".to_string()));
    }

    #[test]
    fn runlevel_switch_rescue_drops_to_shell() {
        let config = ArgonautConfig::default();
        let init = ArgonautInit::new(config);
        let targets = ServiceTarget::defaults();
        let plan = init.plan_runlevel_switch(Runlevel::Rescue, &targets);
        assert!(plan.drop_to_shell);
    }

    // -----------------------------------------------------------------------
    // Health tracker tests
    // -----------------------------------------------------------------------

    #[test]
    fn health_tracker_records_pass() {
        let mut tracker = HealthTracker::new();
        let should_restart = tracker.record("svc1", true, 3);
        assert!(!should_restart);
        assert_eq!(tracker.failure_count("svc1"), 0);
    }

    #[test]
    fn health_tracker_records_failures() {
        let mut tracker = HealthTracker::new();
        assert!(!tracker.record("svc1", false, 3));
        assert_eq!(tracker.failure_count("svc1"), 1);
        assert!(!tracker.record("svc1", false, 3));
        assert_eq!(tracker.failure_count("svc1"), 2);
        // Third failure triggers restart
        assert!(tracker.record("svc1", false, 3));
        assert_eq!(tracker.failure_count("svc1"), 3);
    }

    #[test]
    fn health_tracker_resets_on_pass() {
        let mut tracker = HealthTracker::new();
        tracker.record("svc1", false, 3);
        tracker.record("svc1", false, 3);
        // Pass resets counter
        tracker.record("svc1", true, 3);
        assert_eq!(tracker.failure_count("svc1"), 0);
    }

    #[test]
    fn health_tracker_reset_manual() {
        let mut tracker = HealthTracker::new();
        tracker.record("svc1", false, 3);
        tracker.record("svc1", false, 3);
        tracker.reset("svc1");
        assert_eq!(tracker.failure_count("svc1"), 0);
    }

    #[test]
    fn health_tracker_independent_services() {
        let mut tracker = HealthTracker::new();
        tracker.record("svc1", false, 2);
        tracker.record("svc2", false, 2);
        assert_eq!(tracker.failure_count("svc1"), 1);
        assert_eq!(tracker.failure_count("svc2"), 1);
        // Only svc1 reaches threshold
        assert!(tracker.record("svc1", false, 2));
        assert!(!tracker.record("svc2", true, 2));
    }

    // -----------------------------------------------------------------------
    // Exit status and event tests
    // -----------------------------------------------------------------------

    #[test]
    fn exit_status_display() {
        assert_eq!(format!("{}", ExitStatus::Code(0)), "exit(0)");
        assert_eq!(format!("{}", ExitStatus::Code(1)), "exit(1)");
        assert_eq!(format!("{}", ExitStatus::Signal(9)), "signal(9)");
        assert_eq!(format!("{}", ExitStatus::Signal(15)), "signal(15)");
        assert_eq!(format!("{}", ExitStatus::Running), "running");
        assert_eq!(format!("{}", ExitStatus::NotStarted), "not-started");
    }

    #[test]
    fn service_event_type_display() {
        assert_eq!(format!("{}", ServiceEventType::Starting), "starting");
        assert_eq!(
            format!("{}", ServiceEventType::Started { pid: 42 }),
            "started(pid=42)"
        );
        assert_eq!(
            format!("{}", ServiceEventType::HealthCheckFailed { consecutive: 3 }),
            "health-fail(3x)"
        );
        assert_eq!(
            format!("{}", ServiceEventType::TimeoutKilled),
            "timeout-killed"
        );
        assert_eq!(
            format!(
                "{}",
                ServiceEventType::CrashDetected {
                    exit_status: ExitStatus::Signal(11)
                }
            ),
            "crash(signal(11))"
        );
    }

    #[test]
    fn record_event_creates_event() {
        let config = ArgonautConfig::default();
        let init = ArgonautInit::new(config);
        let event = init.record_event("daimon", ServiceEventType::Starting);
        assert_eq!(event.service, "daimon");
        assert_eq!(event.event_type, ServiceEventType::Starting);
    }

    // -----------------------------------------------------------------------
    // Process spec tests
    // -----------------------------------------------------------------------

    #[test]
    fn process_spec_from_service() {
        let def = ServiceDefinition {
            name: "test-svc".into(),
            description: "test".into(),
            binary_path: PathBuf::from("/usr/bin/test"),
            args: vec!["--flag".into()],
            environment: HashMap::new(),
            depends_on: vec![],
            required_for_modes: vec![BootMode::Server],
            restart_policy: RestartPolicy::Always,
            health_check: None,
            ready_check: None,
        };
        let spec = ProcessSpec::from_service(&def);
        assert_eq!(spec.binary, PathBuf::from("/usr/bin/test"));
        assert_eq!(spec.args, vec!["--flag"]);
        assert!(spec
            .stdout_log
            .unwrap()
            .to_str()
            .unwrap()
            .contains("test-svc"));
        assert!(spec
            .stderr_log
            .unwrap()
            .to_str()
            .unwrap()
            .contains("test-svc"));
    }

    // -----------------------------------------------------------------------
    // Emergency shell tests
    // -----------------------------------------------------------------------

    #[test]
    fn emergency_shell_default_config() {
        let config = EmergencyShellConfig::default();
        assert_eq!(config.shell_path, PathBuf::from("/usr/bin/agnoshi"));
        assert!(!config.require_auth);
        assert!(config.banner.contains("Emergency"));
        assert_eq!(config.environment.get("SHELL").unwrap(), "/usr/bin/agnoshi");
    }

    #[test]
    fn should_drop_to_emergency_on_required_failure() {
        let config = ArgonautConfig {
            boot_mode: BootMode::Server,
            ..ArgonautConfig::default()
        };
        let mut init = ArgonautInit::new(config);
        // MountFilesystems is required — fail it
        init.mark_step_failed(BootStage::MountFilesystems, "disk error".into());
        assert!(init.should_drop_to_emergency());
    }

    #[test]
    fn no_emergency_without_required_failure() {
        let config = ArgonautConfig::default();
        let init = ArgonautInit::new(config);
        assert!(!init.should_drop_to_emergency());
    }

    // -----------------------------------------------------------------------
    // Boot execution plan tests
    // -----------------------------------------------------------------------

    #[test]
    fn boot_execution_plan_ordered() {
        let config = ArgonautConfig {
            boot_mode: BootMode::Minimal,
            ..ArgonautConfig::default()
        };
        let init = ArgonautInit::new(config);
        let plan = init.boot_execution_plan().unwrap();
        assert!(!plan.is_empty());
        // First service should be agent-runtime (only service in minimal)
        assert_eq!(plan[0].0, "agent-runtime");
    }

    #[test]
    fn boot_execution_plan_server() {
        let config = ArgonautConfig {
            boot_mode: BootMode::Server,
            ..ArgonautConfig::default()
        };
        let init = ArgonautInit::new(config);
        let plan = init.boot_execution_plan().unwrap();
        let names: Vec<&str> = plan.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains(&"agent-runtime"));
        assert!(names.contains(&"llm-gateway"));
        // agent-runtime should come after postgres/redis (dependencies)
        let pg_idx = names.iter().position(|n| *n == "postgres");
        let ar_idx = names.iter().position(|n| *n == "agent-runtime");
        if let (Some(pg), Some(ar)) = (pg_idx, ar_idx) {
            assert!(pg < ar, "postgres should start before agent-runtime");
        }
    }

    // -----------------------------------------------------------------------
    // Crash action tests
    // -----------------------------------------------------------------------

    #[test]
    fn crash_action_always_restarts() {
        let config = ArgonautConfig {
            boot_mode: BootMode::Minimal,
            ..ArgonautConfig::default()
        };
        let init = ArgonautInit::new(config);
        let action = init.on_service_crash("agent-runtime", &ExitStatus::Code(1));
        assert!(matches!(action, CrashAction::Restart { .. }));
    }

    #[test]
    fn crash_action_on_failure_ignores_clean_exit() {
        let config = ArgonautConfig {
            boot_mode: BootMode::Server,
            ..ArgonautConfig::default()
        };
        let init = ArgonautInit::new(config);
        // postgres has OnFailure restart policy
        let action = init.on_service_crash("postgres", &ExitStatus::Code(0));
        assert_eq!(action, CrashAction::Ignore);
    }

    #[test]
    fn crash_action_on_failure_restarts_on_error() {
        let config = ArgonautConfig {
            boot_mode: BootMode::Server,
            ..ArgonautConfig::default()
        };
        let init = ArgonautInit::new(config);
        let action = init.on_service_crash("postgres", &ExitStatus::Code(1));
        assert!(matches!(action, CrashAction::Restart { .. }));
    }

    #[test]
    fn crash_action_unknown_service() {
        let config = ArgonautConfig::default();
        let init = ArgonautInit::new(config);
        let action = init.on_service_crash("nonexistent", &ExitStatus::Code(1));
        assert_eq!(action, CrashAction::Ignore);
    }

    #[test]
    fn crash_action_gives_up_after_limit() {
        let config = ArgonautConfig {
            boot_mode: BootMode::Minimal,
            ..ArgonautConfig::default()
        };
        let mut init = ArgonautInit::new(config);
        // Simulate 5 restarts
        if let Some(svc) = init.services.get_mut("agent-runtime") {
            svc.restart_count = 5;
        }
        let action = init.on_service_crash("agent-runtime", &ExitStatus::Signal(11));
        assert!(matches!(action, CrashAction::GiveUp { .. }));
    }

    #[test]
    fn backoff_delay_exponential() {
        assert_eq!(backoff_delay(0), 1000);
        assert_eq!(backoff_delay(1), 2000);
        assert_eq!(backoff_delay(2), 4000);
        assert_eq!(backoff_delay(3), 8000);
        assert_eq!(backoff_delay(4), 16000);
        // Capped at 30s
        assert_eq!(backoff_delay(5), 30000);
        assert_eq!(backoff_delay(10), 30000);
    }

    // -----------------------------------------------------------------------
    // Phase 14A: Edge boot mode tests
    // -----------------------------------------------------------------------

    fn edge_config() -> ArgonautConfig {
        ArgonautConfig {
            boot_mode: BootMode::Edge,
            ..Default::default()
        }
    }

    #[test]
    fn boot_sequence_edge_minimal_stages() {
        let steps = ArgonautInit::build_boot_sequence(BootMode::Edge);
        let stages: Vec<BootStage> = steps.iter().map(|s| s.stage).collect();
        // Edge gets: MountFS, DevMgr, Verify, Security, AgentRuntime, BootComplete = 6
        assert_eq!(steps.len(), 6);
        assert!(stages.contains(&BootStage::MountFilesystems));
        assert!(stages.contains(&BootStage::VerifyRootfs));
        assert!(stages.contains(&BootStage::StartAgentRuntime));
        assert!(stages.contains(&BootStage::BootComplete));
        // Must NOT have these:
        assert!(!stages.contains(&BootStage::StartDatabaseServices));
        assert!(!stages.contains(&BootStage::StartLlmGateway));
        assert!(!stages.contains(&BootStage::StartModelServices));
        assert!(!stages.contains(&BootStage::StartCompositor));
        assert!(!stages.contains(&BootStage::StartShell));
    }

    #[test]
    fn boot_sequence_edge_fast_timeouts() {
        let steps = ArgonautInit::build_boot_sequence(BootMode::Edge);
        let rt_step = steps
            .iter()
            .find(|s| s.stage == BootStage::StartAgentRuntime)
            .unwrap();
        // Edge agent-runtime timeout should be 3s (tight for fast boot)
        assert_eq!(rt_step.timeout_ms, 3000);
        let complete = steps
            .iter()
            .find(|s| s.stage == BootStage::BootComplete)
            .unwrap();
        assert_eq!(complete.timeout_ms, 500);
    }

    #[test]
    fn default_services_edge() {
        let svcs = ArgonautInit::default_services(BootMode::Edge);
        assert_eq!(svcs.len(), 1);
        assert_eq!(svcs[0].name, "agent-runtime");
        assert!(svcs[0].depends_on.is_empty());
        assert!(svcs[0].required_for_modes.contains(&BootMode::Edge));
        // Edge mode env vars
        assert_eq!(svcs[0].environment.get("AGNOS_EDGE_MODE").unwrap(), "1");
        assert_eq!(
            svcs[0].environment.get("AGNOS_READONLY_ROOTFS").unwrap(),
            "1"
        );
    }

    #[test]
    fn edge_services_no_databases() {
        let svcs = ArgonautInit::default_services(BootMode::Edge);
        let names: Vec<&str> = svcs.iter().map(|s| s.name.as_str()).collect();
        assert!(!names.contains(&"postgres"));
        assert!(!names.contains(&"redis"));
        assert!(!names.contains(&"llm-gateway"));
        assert!(!names.contains(&"synapse"));
        assert!(!names.contains(&"aethersafha"));
        assert!(!names.contains(&"agnoshi"));
    }

    #[test]
    fn edge_init_creates_single_service() {
        let init = ArgonautInit::new(edge_config());
        assert_eq!(init.services.len(), 1);
        assert!(init.services.contains_key("agent-runtime"));
    }

    #[test]
    fn edge_boot_can_complete() {
        let mut init = ArgonautInit::new(edge_config());
        for step in &mut init.boot_sequence {
            step.status = BootStepStatus::Complete;
        }
        assert!(init.is_boot_complete());
    }

    #[test]
    fn edge_shutdown_plan() {
        let init = ArgonautInit::new(edge_config());
        let plan = init.plan_shutdown(ShutdownType::Reboot).unwrap();
        assert_eq!(plan.shutdown_type, ShutdownType::Reboot);
        assert!(plan
            .steps
            .iter()
            .any(|s| s.action == ShutdownAction::CloseLuks));
    }

    #[test]
    fn runlevel_edge_mapping() {
        assert_eq!(Runlevel::Edge.to_boot_mode(), Some(BootMode::Edge));
        assert_eq!(Runlevel::from_boot_mode(BootMode::Edge), Runlevel::Edge);
        assert_eq!(Runlevel::Edge.level(), 8);
        assert_eq!(format!("{}", Runlevel::Edge), "edge");
    }

    #[test]
    fn edge_target_active_in_edge_runlevel() {
        let targets = ServiceTarget::defaults();
        let edge = targets.iter().find(|t| t.name == "edge").unwrap();
        assert!(edge.is_active_in(Runlevel::Edge));
        assert!(!edge.is_active_in(Runlevel::Console));
        assert!(!edge.is_active_in(Runlevel::Graphical));
        assert!(edge.requires.contains(&"daimon".to_string()));
        assert!(edge.wants.contains(&"aegis".to_string()));
    }

    #[test]
    fn edge_service_state_lifecycle() {
        let mut init = ArgonautInit::new(edge_config());
        assert!(init.set_service_state("agent-runtime", ServiceState::Starting));
        assert!(init.set_service_state("agent-runtime", ServiceState::Running));
        let stats = init.stats();
        assert_eq!(stats.boot_mode, BootMode::Edge);
        assert_eq!(stats.services_running, 1);
    }

    // --- Read-only rootfs / dm-verity (Phase 14A-3/4) ---

    #[test]
    fn readonly_rootfs_returns_five_commands() {
        let cmds = configure_readonly_rootfs();
        assert_eq!(cmds.len(), 5);
    }

    #[test]
    fn readonly_rootfs_remounts_root_ro() {
        let cmds = configure_readonly_rootfs();
        assert_eq!(cmds[0], "mount -o remount,ro /");
    }

    #[test]
    fn readonly_rootfs_tmpfs_noexec() {
        let cmds = configure_readonly_rootfs();
        // /tmp and /var/tmp should have noexec
        assert!(cmds[1].contains("noexec"));
        assert!(cmds[4].contains("noexec"));
        // /var/run and /var/log should NOT have noexec
        assert!(!cmds[2].contains("noexec"));
        assert!(!cmds[3].contains("noexec"));
    }

    #[test]
    fn verify_rootfs_integrity_success() {
        let hash = "a".repeat(64);
        let result = verify_rootfs_integrity("/dev/sda1", "/dev/sda2", &hash);
        assert!(result.is_ok());
        let cmds = result.unwrap();
        assert_eq!(cmds.len(), 3);
        assert_eq!(cmds[0].binary, "veritysetup");
        assert_eq!(cmds[0].args[0], "verify");
        assert_eq!(cmds[1].binary, "veritysetup");
        assert_eq!(cmds[1].args[0], "open");
        assert_eq!(cmds[2].binary, "mount");
        assert!(cmds[2]
            .args
            .contains(&"/dev/mapper/verified-root".to_string()));
    }

    #[test]
    fn verify_rootfs_integrity_empty_params() {
        let hash = "a".repeat(64);
        assert!(verify_rootfs_integrity("", "/dev/sda2", &hash).is_err());
        assert!(verify_rootfs_integrity("/dev/sda1", "", &hash).is_err());
        assert!(verify_rootfs_integrity("/dev/sda1", "/dev/sda2", "").is_err());
    }

    #[test]
    fn verify_rootfs_integrity_bad_hash_length() {
        let result = verify_rootfs_integrity("/dev/sda1", "/dev/sda2", "abcdef");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("64 hex characters"));

        let long_hash = "a".repeat(128);
        let result = verify_rootfs_integrity("/dev/sda1", "/dev/sda2", &long_hash);
        assert!(result.is_err());
    }

    #[test]
    fn verify_rootfs_integrity_bad_hash_chars() {
        // 64 chars but non-hex
        let hash = "g".repeat(64);
        let result = verify_rootfs_integrity("/dev/sda1", "/dev/sda2", &hash);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("hex characters"));
    }

    #[test]
    fn verify_rootfs_integrity_bad_device_path() {
        let hash = "a".repeat(64);
        // Path without /dev/ prefix
        let result = verify_rootfs_integrity("/tmp/sda1", "/dev/sda2", &hash);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("/dev/"));

        // Path with shell metacharacters
        let result = verify_rootfs_integrity("/dev/sda1; rm -rf /", "/dev/sda2", &hash);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("invalid characters"));
    }

    #[test]
    fn verify_rootfs_integrity_commands_contain_devices() {
        let hash = "b".repeat(64);
        let cmds = verify_rootfs_integrity("/dev/vda1", "/dev/vda2", &hash).unwrap();
        assert!(cmds[0].args.contains(&"/dev/vda1".to_string()));
        assert!(cmds[0].args.contains(&"/dev/vda2".to_string()));
        assert!(cmds[0].args.contains(&hash));
        assert!(cmds[1].args.contains(&"/dev/vda1".to_string()));
        assert!(cmds[1].args.contains(&"verified-root".to_string()));
    }

    #[test]
    fn verify_rootfs_integrity_mount_is_readonly() {
        let hash = "c".repeat(64);
        let cmds = verify_rootfs_integrity("/dev/sda1", "/dev/sda2", &hash).unwrap();
        assert!(cmds[2].args.contains(&"ro".to_string()));
    }

    #[test]
    fn safe_command_display() {
        let cmd = SafeCommand {
            binary: "mount".to_string(),
            args: vec!["-o".to_string(), "ro".to_string(), "/dev/sda1".to_string()],
        };
        assert_eq!(cmd.display(), "mount -o ro /dev/sda1");
    }

    // -----------------------------------------------------------------------
    // Phase 14D: Edge Security tests
    // -----------------------------------------------------------------------

    #[test]
    fn edge_boot_config_defaults() {
        let cfg = EdgeBootConfig::default();
        assert!(cfg.readonly_rootfs);
        assert!(cfg.luks_enabled);
        assert!(!cfg.tpm_attestation);
        assert_eq!(cfg.max_boot_time_ms, 3000);
    }

    #[test]
    fn edge_boot_config_custom() {
        let cfg = EdgeBootConfig {
            readonly_rootfs: false,
            luks_enabled: false,
            tpm_attestation: true,
            max_boot_time_ms: 5000,
        };
        assert!(!cfg.readonly_rootfs);
        assert!(!cfg.luks_enabled);
        assert!(cfg.tpm_attestation);
        assert_eq!(cfg.max_boot_time_ms, 5000);
    }

    #[test]
    fn edge_boot_config_serde_roundtrip() {
        let cfg = EdgeBootConfig::default();
        let json = serde_json::to_string(&cfg).unwrap();
        let deserialized: EdgeBootConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.luks_enabled, cfg.luks_enabled);
        assert_eq!(deserialized.readonly_rootfs, cfg.readonly_rootfs);
        assert_eq!(deserialized.tpm_attestation, cfg.tpm_attestation);
        assert_eq!(deserialized.max_boot_time_ms, cfg.max_boot_time_ms);
    }

    #[test]
    fn default_services_edge_has_luks_env() {
        let svcs = ArgonautInit::default_services(BootMode::Edge);
        assert_eq!(svcs.len(), 1);
        assert_eq!(svcs[0].environment.get("AGNOS_EDGE_LUKS").unwrap(), "1");
    }

    // -----------------------------------------------------------------------
    // Shruti optional service
    // -----------------------------------------------------------------------

    #[test]
    fn shruti_service_definition() {
        let svc = ArgonautInit::shruti_service();
        assert_eq!(svc.name, "shruti");
        assert_eq!(svc.binary_path, PathBuf::from("/usr/local/bin/shruti"));
        assert!(svc.depends_on.contains(&"agent-runtime".into()));
        assert!(svc.depends_on.contains(&"aethersafha".into()));
        assert!(
            svc.required_for_modes.is_empty(),
            "shruti must not auto-start"
        );
        assert_eq!(svc.restart_policy, RestartPolicy::OnFailure);
        assert!(svc.health_check.is_some());
        assert!(svc.ready_check.is_none());
    }

    #[test]
    fn shruti_not_in_default_services() {
        for mode in [
            BootMode::Desktop,
            BootMode::Server,
            BootMode::Minimal,
            BootMode::Edge,
        ] {
            let svcs = ArgonautInit::default_services(mode);
            assert!(
                !svcs.iter().any(|s| s.name == "shruti"),
                "shruti should not appear in default services for {:?}",
                mode,
            );
        }
    }

    #[test]
    fn shruti_optional_service_lookup() {
        assert!(ArgonautInit::optional_service("shruti").is_some());
        assert!(ArgonautInit::optional_service("nonexistent").is_none());
    }

    #[test]
    fn enable_optional_shruti_service() {
        let config = ArgonautConfig {
            boot_mode: BootMode::Desktop,
            ..Default::default()
        };
        let mut init = ArgonautInit::new(config);
        assert!(!init.services.contains_key("shruti"));

        let added = init.enable_optional_service("shruti");
        assert!(added);
        assert!(init.services.contains_key("shruti"));
        assert_eq!(init.services["shruti"].state, ServiceState::Stopped);

        // Second call is a no-op
        let added_again = init.enable_optional_service("shruti");
        assert!(!added_again);
    }

    #[test]
    fn shruti_user_config_service() {
        let config = ArgonautConfig {
            boot_mode: BootMode::Desktop,
            services: vec![ArgonautInit::shruti_service()],
            ..Default::default()
        };
        let init = ArgonautInit::new(config);
        assert!(init.services.contains_key("shruti"));
    }
}
