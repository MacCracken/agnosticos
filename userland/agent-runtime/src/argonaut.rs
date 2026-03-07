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
}

impl fmt::Display for BootMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Server => write!(f, "server"),
            Self::Desktop => write!(f, "desktop"),
            Self::Minimal => write!(f, "minimal"),
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
    StartAgentRuntime,
    StartLlmGateway,
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
            Self::StartAgentRuntime => 4,
            Self::StartLlmGateway => 5,
            Self::StartCompositor => 6,
            Self::StartShell => 7,
            Self::BootComplete => 8,
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
            Self::StartAgentRuntime => write!(f, "start-agent-runtime"),
            Self::StartLlmGateway => write!(f, "start-llm-gateway"),
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

    /// Return the default AGNOS services for a boot mode.
    pub fn default_services(mode: BootMode) -> Vec<ServiceDefinition> {
        let mut services = Vec::new();

        // agent-runtime is always present.
        services.push(ServiceDefinition {
            name: "agent-runtime".into(),
            description: "Daimon agent orchestrator".into(),
            binary_path: PathBuf::from("/usr/lib/agnos/agent_runtime"),
            args: vec!["--port".into(), "8090".into()],
            environment: HashMap::new(),
            depends_on: vec![],
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
            let svc = self.services.get_mut(name).unwrap();
            debug!(service = %name, from = %svc.state, to = %state, "state transition");
            svc.state = state;
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

    // --- BootStage ordering ---

    #[test]
    fn boot_stage_ordering() {
        assert!(BootStage::MountFilesystems < BootStage::StartDeviceManager);
        assert!(BootStage::StartDeviceManager < BootStage::VerifyRootfs);
        assert!(BootStage::VerifyRootfs < BootStage::StartSecurity);
        assert!(BootStage::StartSecurity < BootStage::StartAgentRuntime);
        assert!(BootStage::StartAgentRuntime < BootStage::StartLlmGateway);
        assert!(BootStage::StartLlmGateway < BootStage::StartCompositor);
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
        // 6 (minimal) + LlmGateway = 7
        let steps = ArgonautInit::build_boot_sequence(BootMode::Server);
        assert_eq!(steps.len(), 7);
    }

    #[test]
    fn boot_sequence_step_count_desktop() {
        // 7 (server) + Compositor + Shell = 9
        let steps = ArgonautInit::build_boot_sequence(BootMode::Desktop);
        assert_eq!(steps.len(), 9);
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
        assert_eq!(svcs.len(), 2);
        let names: Vec<&str> = svcs.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"agent-runtime"));
        assert!(names.contains(&"llm-gateway"));
    }

    #[test]
    fn default_services_desktop() {
        let svcs = ArgonautInit::default_services(BootMode::Desktop);
        assert_eq!(svcs.len(), 4);
        let names: Vec<&str> = svcs.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"agent-runtime"));
        assert!(names.contains(&"llm-gateway"));
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
        assert_eq!(desktop_svcs.len(), 4);
    }

    // --- Stats ---

    #[test]
    fn stats_accuracy() {
        let mut init = ArgonautInit::new(server_config());
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
        assert_eq!(s.services_running, 1);
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
}
