//! Argonaut type definitions — enums, structs, and configuration types.

use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

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
    pub(crate) fn order(self) -> u8 {
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
// Health check execution types
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
    pub(crate) failures: HashMap<String, u32>,
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

// ---------------------------------------------------------------------------
// Crash action
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Safe command (injection-resistant)
// ---------------------------------------------------------------------------

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
