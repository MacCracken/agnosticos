//! AGNOS Init / Service Manager
//!
//! Provides PID-1-style service supervision with dependency ordering,
//! parallel startup, health monitoring, and sd_notify integration.
//! Services are defined as TOML files in `/etc/agnos/services/`.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tokio::process::{Child, Command};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

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

fn default_max_restarts() -> u32 {
    5
}
fn default_restart_delay() -> u64 {
    1
}
fn default_readiness_timeout() -> u64 {
    30
}
fn default_true() -> bool {
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
struct ServiceRuntime {
    definition: ServiceDefinition,
    state: ServiceState,
    child: Option<Child>,
    pid: Option<u32>,
    restart_count: u32,
    started_at: Option<Instant>,
    exit_code: Option<i32>,
}

// ---------------------------------------------------------------------------
// Dependency resolution
// ---------------------------------------------------------------------------

/// Topologically sort service names so that dependencies come first.
/// Returns `Err` if there is a cycle.
pub fn topological_sort(services: &HashMap<String, ServiceDefinition>) -> Result<Vec<String>> {
    // Build adjacency: for each service, after + wants are its incoming edges.
    let mut in_degree: HashMap<&str, usize> = HashMap::new();
    let mut dependents: HashMap<&str, Vec<&str>> = HashMap::new();

    for (name, def) in services {
        in_degree.entry(name.as_str()).or_insert(0);
        for dep in def.after.iter().chain(def.wants.iter()) {
            if services.contains_key(dep.as_str()) {
                dependents
                    .entry(dep.as_str())
                    .or_default()
                    .push(name.as_str());
                *in_degree.entry(name.as_str()).or_insert(0) += 1;
            }
        }
    }

    let mut queue: VecDeque<&str> = in_degree
        .iter()
        .filter(|(_, &deg)| deg == 0)
        .map(|(&name, _)| name)
        .collect();

    let mut order = Vec::with_capacity(services.len());

    while let Some(name) = queue.pop_front() {
        order.push(name.to_string());
        if let Some(deps) = dependents.get(name) {
            for &dep in deps {
                if let Some(deg) = in_degree.get_mut(dep) {
                    *deg -= 1;
                    if *deg == 0 {
                        queue.push_back(dep);
                    }
                }
            }
        }
    }

    if order.len() != services.len() {
        let remaining: Vec<_> = services
            .keys()
            .filter(|k| !order.contains(k))
            .cloned()
            .collect();
        anyhow::bail!(
            "Dependency cycle detected among services: {}",
            remaining.join(", ")
        );
    }

    Ok(order)
}

// ---------------------------------------------------------------------------
// ServiceManager
// ---------------------------------------------------------------------------

/// The AGNOS service manager — manages lifecycle of system services with
/// dependency ordering, restart policies, and health monitoring.
pub struct ServiceManager {
    services: Arc<RwLock<HashMap<String, ServiceRuntime>>>,
    definitions: Arc<RwLock<HashMap<String, ServiceDefinition>>>,
    config_dir: PathBuf,
}

impl ServiceManager {
    /// Create a new service manager that loads definitions from `config_dir`.
    pub fn new(config_dir: impl Into<PathBuf>) -> Self {
        Self {
            services: Arc::new(RwLock::new(HashMap::new())),
            definitions: Arc::new(RwLock::new(HashMap::new())),
            config_dir: config_dir.into(),
        }
    }

    /// Load all `.toml` service definitions from the config directory.
    pub async fn load_definitions(&self) -> Result<usize> {
        let dir = &self.config_dir;
        if !dir.is_dir() {
            info!("Service config dir {} does not exist, creating it", dir.display());
            tokio::fs::create_dir_all(dir)
                .await
                .with_context(|| format!("Failed to create {}", dir.display()))?;
            return Ok(0);
        }

        let mut count = 0;
        let mut entries = tokio::fs::read_dir(dir)
            .await
            .with_context(|| format!("Failed to read {}", dir.display()))?;

        let mut defs = self.definitions.write().await;
        let mut svcs = self.services.write().await;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().map_or(true, |e| e != "toml") {
                continue;
            }

            match load_service_file(&path).await {
                Ok(def) => {
                    info!("Loaded service definition: {}", def.name);
                    let name = def.name.clone();
                    svcs.entry(name.clone()).or_insert_with(|| ServiceRuntime {
                        definition: def.clone(),
                        state: ServiceState::Stopped,
                        child: None,
                        pid: None,
                        restart_count: 0,
                        started_at: None,
                        exit_code: None,
                    });
                    defs.insert(name, def);
                    count += 1;
                }
                Err(e) => {
                    warn!("Failed to load {}: {}", path.display(), e);
                }
            }
        }

        Ok(count)
    }

    /// Register a service definition programmatically (used by agent-runtime
    /// to register its own built-in services).
    pub async fn register(&self, def: ServiceDefinition) {
        let name = def.name.clone();
        let mut defs = self.definitions.write().await;
        let mut svcs = self.services.write().await;
        svcs.entry(name.clone()).or_insert_with(|| ServiceRuntime {
            definition: def.clone(),
            state: ServiceState::Stopped,
            child: None,
            pid: None,
            restart_count: 0,
            started_at: None,
            exit_code: None,
        });
        defs.insert(name, def);
    }

    /// Boot all enabled services in dependency order.
    /// Services at the same dependency level are started in parallel.
    pub async fn boot(&self) -> Result<()> {
        info!("AGNOS service manager: starting boot sequence");

        let defs = self.definitions.read().await;
        let enabled: HashMap<String, ServiceDefinition> = defs
            .iter()
            .filter(|(_, d)| d.enabled)
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        drop(defs);

        if enabled.is_empty() {
            info!("No enabled services to start");
            return Ok(());
        }

        let order = topological_sort(&enabled)?;
        info!(
            "Boot order: {}",
            order.join(" -> ")
        );

        // Group by dependency level for parallel startup
        let levels = dependency_levels(&enabled, &order);

        for (level_idx, level) in levels.iter().enumerate() {
            info!(
                "Starting level {} services: {}",
                level_idx,
                level.join(", ")
            );

            let mut handles = Vec::new();
            for name in level {
                let name = name.clone();
                let mgr_services = self.services.clone();
                let mgr_defs = self.definitions.clone();
                handles.push(tokio::spawn(async move {
                    start_service_inner(&mgr_services, &mgr_defs, &name).await
                }));
            }

            for handle in handles {
                if let Err(e) = handle.await? {
                    error!("Service start error: {}", e);
                    // Continue booting other services — don't fail the whole boot
                }
            }
        }

        info!("AGNOS service manager: boot sequence complete");
        Ok(())
    }

    /// Start a single service by name.
    pub async fn start_service(&self, name: &str) -> Result<()> {
        // Check it exists
        {
            let defs = self.definitions.read().await;
            if !defs.contains_key(name) {
                anyhow::bail!("Unknown service: {}", name);
            }
        }

        // Start dependencies first
        let deps = {
            let defs = self.definitions.read().await;
            let def = &defs[name];
            def.after.clone()
        };

        for dep in &deps {
            let state = {
                let svcs = self.services.read().await;
                svcs.get(dep).map(|s| s.state)
            };
            match state {
                Some(ServiceState::Running | ServiceState::Exited) => {}
                Some(_) | None => {
                    info!("Starting dependency {} for {}", dep, name);
                    // Box::pin to avoid recursive async issue
                    Box::pin(self.start_service(dep)).await?;
                }
            }
        }

        start_service_inner(&self.services, &self.definitions, name).await
    }

    /// Stop a single service by name.
    pub async fn stop_service(&self, name: &str) -> Result<()> {
        let mut svcs = self.services.write().await;
        let svc = svcs
            .get_mut(name)
            .ok_or_else(|| anyhow::anyhow!("Unknown service: {}", name))?;

        if svc.state == ServiceState::Stopped || svc.state == ServiceState::Exited {
            info!("Service {} is already stopped", name);
            return Ok(());
        }

        info!("Stopping service: {}", name);
        svc.state = ServiceState::Stopping;

        if let Some(ref mut child) = svc.child {
            // Send SIGTERM first
            if let Some(pid) = svc.pid {
                #[cfg(unix)]
                {
                    let _ = nix::sys::signal::kill(
                        nix::unistd::Pid::from_raw(pid as i32),
                        nix::sys::signal::Signal::SIGTERM,
                    );
                }
            }

            // Wait up to 10 seconds for graceful shutdown
            let timeout_result = tokio::time::timeout(
                Duration::from_secs(10),
                child.wait(),
            )
            .await;

            match timeout_result {
                Ok(Ok(status)) => {
                    info!("Service {} exited with {}", name, status);
                    svc.exit_code = status.code();
                }
                _ => {
                    warn!("Service {} did not exit gracefully, sending SIGKILL", name);
                    let _ = child.kill().await;
                }
            }
        }

        svc.state = ServiceState::Stopped;
        svc.child = None;
        svc.pid = None;
        info!("Service {} stopped", name);
        Ok(())
    }

    /// Shutdown all services in reverse dependency order.
    pub async fn shutdown_all(&self) -> Result<()> {
        info!("AGNOS service manager: shutting down all services");

        let defs = self.definitions.read().await;
        let all: HashMap<String, ServiceDefinition> = defs.clone();
        drop(defs);

        if all.is_empty() {
            return Ok(());
        }

        let order = topological_sort(&all).unwrap_or_default();
        // Reverse: stop dependents before their dependencies
        let reversed: Vec<_> = order.into_iter().rev().collect();

        for name in &reversed {
            if let Err(e) = self.stop_service(name).await {
                warn!("Error stopping {}: {}", name, e);
            }
        }

        info!("AGNOS service manager: all services stopped");
        Ok(())
    }

    /// Restart a service (stop then start).
    pub async fn restart_service(&self, name: &str) -> Result<()> {
        self.stop_service(name).await?;
        self.start_service(name).await
    }

    /// Enable a service (will start on next boot).
    pub async fn enable_service(&self, name: &str) -> Result<()> {
        let mut defs = self.definitions.write().await;
        let def = defs
            .get_mut(name)
            .ok_or_else(|| anyhow::anyhow!("Unknown service: {}", name))?;
        def.enabled = true;
        // Persist to disk
        let path = self.config_dir.join(format!("{}.toml", name));
        let content = toml::to_string_pretty(def)?;
        tokio::fs::write(&path, content).await?;
        info!("Service {} enabled", name);
        Ok(())
    }

    /// Disable a service (will not start on next boot).
    pub async fn disable_service(&self, name: &str) -> Result<()> {
        let mut defs = self.definitions.write().await;
        let def = defs
            .get_mut(name)
            .ok_or_else(|| anyhow::anyhow!("Unknown service: {}", name))?;
        def.enabled = false;
        let path = self.config_dir.join(format!("{}.toml", name));
        let content = toml::to_string_pretty(def)?;
        tokio::fs::write(&path, content).await?;
        info!("Service {} disabled", name);
        Ok(())
    }

    /// Get the status of a single service.
    pub async fn get_status(&self, name: &str) -> Option<ServiceStatus> {
        let svcs = self.services.read().await;
        let svc = svcs.get(name)?;
        Some(ServiceStatus {
            name: name.to_string(),
            state: svc.state,
            pid: svc.pid,
            restart_count: svc.restart_count,
            uptime: svc.started_at.map(|t| t.elapsed()),
            exit_code: svc.exit_code,
            enabled: svc.definition.enabled,
            description: svc.definition.description.clone(),
        })
    }

    /// List all services and their statuses.
    pub async fn list_services(&self) -> Vec<ServiceStatus> {
        let svcs = self.services.read().await;
        svcs.iter()
            .map(|(name, svc)| ServiceStatus {
                name: name.clone(),
                state: svc.state,
                pid: svc.pid,
                restart_count: svc.restart_count,
                uptime: svc.started_at.map(|t| t.elapsed()),
                exit_code: svc.exit_code,
                enabled: svc.definition.enabled,
                description: svc.definition.description.clone(),
            })
            .collect()
    }

    /// Background loop that monitors running services and restarts them
    /// according to their restart policy.
    pub async fn monitor_loop(&self) {
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        loop {
            interval.tick().await;
            self.check_services().await;
        }
    }

    async fn check_services(&self) {
        let mut svcs = self.services.write().await;

        let mut to_restart: Vec<String> = Vec::new();

        for (name, svc) in svcs.iter_mut() {
            if svc.state != ServiceState::Running {
                continue;
            }

            // Check if child has exited
            if let Some(ref mut child) = svc.child {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        let code = status.code().unwrap_or(-1);
                        info!("Service {} exited with code {}", name, code);
                        svc.exit_code = Some(code);
                        svc.pid = None;
                        svc.child = None;

                        if svc.definition.service_type == ServiceType::Oneshot && code == 0 {
                            svc.state = ServiceState::Exited;
                            continue;
                        }

                        let should_restart = match svc.definition.restart {
                            RestartPolicy::Always => true,
                            RestartPolicy::OnFailure => code != 0,
                            RestartPolicy::No => false,
                        };

                        if should_restart && svc.restart_count < svc.definition.max_restarts {
                            svc.state = ServiceState::Stopped;
                            to_restart.push(name.clone());
                        } else {
                            svc.state = if code == 0 {
                                ServiceState::Exited
                            } else {
                                ServiceState::Failed
                            };
                            if svc.restart_count >= svc.definition.max_restarts {
                                error!(
                                    "Service {} exceeded max restarts ({}), marking failed",
                                    name, svc.definition.max_restarts
                                );
                            }
                        }
                    }
                    Ok(None) => {} // still running
                    Err(e) => {
                        warn!("Failed to check service {} status: {}", name, e);
                    }
                }
            }
        }

        drop(svcs);

        // Restart services outside the write lock
        for name in to_restart {
            let delay = {
                let svcs = self.services.read().await;
                if let Some(svc) = svcs.get(&name) {
                    // Exponential backoff capped at 60s
                    let base = svc.definition.restart_delay_secs;
                    let backoff = base * 2u64.pow(svc.restart_count.min(5));
                    backoff.min(60)
                } else {
                    1
                }
            };

            info!(
                "Restarting service {} in {}s (attempt {})",
                name,
                delay,
                {
                    let svcs = self.services.read().await;
                    svcs.get(&name).map_or(0, |s| s.restart_count + 1)
                }
            );

            tokio::time::sleep(Duration::from_secs(delay)).await;

            if let Err(e) =
                start_service_inner(&self.services, &self.definitions, &name).await
            {
                error!("Failed to restart service {}: {}", name, e);
                let mut svcs = self.services.write().await;
                if let Some(svc) = svcs.get_mut(&name) {
                    svc.state = ServiceState::Failed;
                }
            } else {
                let mut svcs = self.services.write().await;
                if let Some(svc) = svcs.get_mut(&name) {
                    svc.restart_count += 1;
                }
            }
        }
    }

    /// Send sd_notify-compatible readiness notification.
    /// In a real PID 1 scenario this would write to NOTIFY_SOCKET.
    pub fn notify_ready() {
        if let Ok(socket_path) = std::env::var("NOTIFY_SOCKET") {
            #[cfg(unix)]
            {
                use std::os::unix::net::UnixDatagram;
                if let Ok(sock) = UnixDatagram::unbound() {
                    let _ = sock.send_to(b"READY=1", &socket_path);
                }
            }
            info!("Sent sd_notify READY=1 to {}", socket_path);
        }
    }
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
// Internal helpers
// ---------------------------------------------------------------------------

/// Load a single service TOML file.
async fn load_service_file(path: &Path) -> Result<ServiceDefinition> {
    let content = tokio::fs::read_to_string(path)
        .await
        .with_context(|| format!("Failed to read {}", path.display()))?;
    let def: ServiceDefinition =
        toml::from_str(&content).with_context(|| format!("Failed to parse {}", path.display()))?;
    Ok(def)
}

/// Start a service (internal, lock-free on entry — acquires its own locks).
async fn start_service_inner(
    services: &RwLock<HashMap<String, ServiceRuntime>>,
    definitions: &RwLock<HashMap<String, ServiceDefinition>>,
    name: &str,
) -> Result<()> {
    let def = {
        let defs = definitions.read().await;
        defs.get(name)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Unknown service: {}", name))?
    };

    // Check if already running
    {
        let svcs = services.read().await;
        if let Some(svc) = svcs.get(name) {
            if svc.state == ServiceState::Running {
                debug!("Service {} is already running", name);
                return Ok(());
            }
        }
    }

    info!(
        "Starting service: {} ({})",
        name,
        if def.description.is_empty() {
            &def.exec_start
        } else {
            &def.description
        }
    );

    // Mark as starting
    {
        let mut svcs = services.write().await;
        if let Some(svc) = svcs.get_mut(name) {
            svc.state = ServiceState::Starting;
        }
    }

    // Build command
    let mut cmd = Command::new(&def.exec_start);
    cmd.args(&def.args);
    cmd.stdout(Stdio::null());
    cmd.stderr(Stdio::null());

    if !def.working_directory.is_empty() {
        cmd.current_dir(&def.working_directory);
    }

    for env_var in &def.environment {
        if let Some((key, val)) = env_var.split_once('=') {
            cmd.env(key, val);
        }
    }

    #[cfg(unix)]
    if !def.user.is_empty() {
        // Look up UID from username
        if let Ok(output) = std::process::Command::new("id")
            .args(["-u", &def.user])
            .output()
        {
            if let Ok(uid_str) = std::str::from_utf8(&output.stdout) {
                if let Ok(uid) = uid_str.trim().parse::<u32>() {
                    cmd.uid(uid);
                }
            }
        }
    }

    #[cfg(unix)]
    if !def.group.is_empty() {
        if let Ok(output) = std::process::Command::new("id")
            .args(["-g", &def.group])
            .output()
        {
            if let Ok(gid_str) = std::str::from_utf8(&output.stdout) {
                if let Ok(gid) = gid_str.trim().parse::<u32>() {
                    cmd.gid(gid);
                }
            }
        }
    }

    // Spawn the process
    let mut child = cmd
        .spawn()
        .with_context(|| format!("Failed to spawn service {}: {}", name, def.exec_start))?;

    let pid = child.id();

    // For oneshot services, wait for exit
    if def.service_type == ServiceType::Oneshot {
        let status = tokio::time::timeout(
            Duration::from_secs(def.readiness_timeout_secs),
            child.wait(),
        )
        .await
        .with_context(|| format!("Service {} oneshot timed out", name))?
        .with_context(|| format!("Service {} wait failed", name))?;

        let code = status.code().unwrap_or(-1);
        let mut svcs = services.write().await;
        if let Some(svc) = svcs.get_mut(name) {
            svc.exit_code = Some(code);
            if status.success() {
                svc.state = ServiceState::Exited;
                info!("Service {} (oneshot) completed successfully", name);
            } else {
                svc.state = ServiceState::Failed;
                error!("Service {} (oneshot) failed with code {}", name, code);
            }
        }
        return if status.success() {
            Ok(())
        } else {
            anyhow::bail!("Service {} failed with exit code {}", name, code)
        };
    }

    // For simple/notify services
    let mut svcs = services.write().await;
    if let Some(svc) = svcs.get_mut(name) {
        svc.pid = pid;
        svc.child = Some(child);
        svc.started_at = Some(Instant::now());
        svc.state = ServiceState::Running;
    }

    info!(
        "Service {} started (pid: {})",
        name,
        pid.map_or("unknown".to_string(), |p| p.to_string())
    );

    Ok(())
}

/// Group services by dependency level for parallel startup.
/// Returns a vec of vecs — each inner vec can be started concurrently.
fn dependency_levels(
    services: &HashMap<String, ServiceDefinition>,
    order: &[String],
) -> Vec<Vec<String>> {
    let mut levels: Vec<Vec<String>> = Vec::new();
    let mut assigned: HashMap<&str, usize> = HashMap::new();

    for name in order {
        let def = &services[name];
        let deps: Vec<&str> = def
            .after
            .iter()
            .chain(def.wants.iter())
            .filter(|d| services.contains_key(d.as_str()))
            .map(|d| d.as_str())
            .collect();

        let level = if deps.is_empty() {
            0
        } else {
            deps.iter()
                .filter_map(|d| assigned.get(d))
                .max()
                .map_or(0, |l| l + 1)
        };

        assigned.insert(name.as_str(), level);

        while levels.len() <= level {
            levels.push(Vec::new());
        }
        levels[level].push(name.clone());
    }

    levels
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_def(name: &str, after: &[&str]) -> ServiceDefinition {
        ServiceDefinition {
            name: name.to_string(),
            exec_start: format!("/usr/bin/{}", name),
            args: vec![],
            environment: vec![],
            after: after.iter().map(|s| s.to_string()).collect(),
            wants: vec![],
            restart: RestartPolicy::Always,
            max_restarts: 5,
            restart_delay_secs: 1,
            user: String::new(),
            group: String::new(),
            working_directory: String::new(),
            service_type: ServiceType::Simple,
            readiness_timeout_secs: 30,
            resources: ServiceResources::default(),
            enabled: true,
            description: String::new(),
        }
    }

    #[test]
    fn test_topological_sort_basic() {
        let mut services = HashMap::new();
        services.insert("c".into(), make_def("c", &["b"]));
        services.insert("b".into(), make_def("b", &["a"]));
        services.insert("a".into(), make_def("a", &[]));

        let order = topological_sort(&services).unwrap();
        assert_eq!(order, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_topological_sort_parallel_roots() {
        let mut services = HashMap::new();
        services.insert("audit".into(), make_def("audit", &[]));
        services.insert("network".into(), make_def("network", &[]));
        services.insert("runtime".into(), make_def("runtime", &["audit", "network"]));

        let order = topological_sort(&services).unwrap();
        // audit and network should come before runtime
        let runtime_pos = order.iter().position(|s| s == "runtime").unwrap();
        let audit_pos = order.iter().position(|s| s == "audit").unwrap();
        let network_pos = order.iter().position(|s| s == "network").unwrap();
        assert!(audit_pos < runtime_pos);
        assert!(network_pos < runtime_pos);
    }

    #[test]
    fn test_topological_sort_cycle_detection() {
        let mut services = HashMap::new();
        services.insert("a".into(), make_def("a", &["c"]));
        services.insert("b".into(), make_def("b", &["a"]));
        services.insert("c".into(), make_def("c", &["b"]));

        let result = topological_sort(&services);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cycle"));
    }

    #[test]
    fn test_topological_sort_ignores_unknown_deps() {
        let mut services = HashMap::new();
        services.insert("a".into(), make_def("a", &["nonexistent"]));

        let order = topological_sort(&services).unwrap();
        assert_eq!(order, vec!["a"]);
    }

    #[test]
    fn test_dependency_levels_basic() {
        let mut services = HashMap::new();
        services.insert("a".into(), make_def("a", &[]));
        services.insert("b".into(), make_def("b", &["a"]));
        services.insert("c".into(), make_def("c", &["b"]));

        let order = topological_sort(&services).unwrap();
        let levels = dependency_levels(&services, &order);

        assert_eq!(levels.len(), 3);
        assert_eq!(levels[0], vec!["a"]);
        assert_eq!(levels[1], vec!["b"]);
        assert_eq!(levels[2], vec!["c"]);
    }

    #[test]
    fn test_dependency_levels_parallel() {
        let mut services = HashMap::new();
        services.insert("audit".into(), make_def("audit", &[]));
        services.insert("network".into(), make_def("network", &[]));
        services.insert("runtime".into(), make_def("runtime", &["audit", "network"]));

        let order = topological_sort(&services).unwrap();
        let levels = dependency_levels(&services, &order);

        assert_eq!(levels.len(), 2);
        // Level 0 has audit and network (parallel)
        assert_eq!(levels[0].len(), 2);
        assert!(levels[0].contains(&"audit".to_string()));
        assert!(levels[0].contains(&"network".to_string()));
        // Level 1 has runtime
        assert_eq!(levels[1], vec!["runtime"]);
    }

    #[test]
    fn test_dependency_levels_diamond() {
        let mut services = HashMap::new();
        services.insert("base".into(), make_def("base", &[]));
        services.insert("left".into(), make_def("left", &["base"]));
        services.insert("right".into(), make_def("right", &["base"]));
        services.insert("top".into(), make_def("top", &["left", "right"]));

        let order = topological_sort(&services).unwrap();
        let levels = dependency_levels(&services, &order);

        assert_eq!(levels.len(), 3);
        assert_eq!(levels[0], vec!["base"]);
        assert_eq!(levels[1].len(), 2); // left and right in parallel
        assert_eq!(levels[2], vec!["top"]);
    }

    #[test]
    fn test_service_status_uptime_display() {
        let status = ServiceStatus {
            name: "test".to_string(),
            state: ServiceState::Running,
            pid: Some(1234),
            restart_count: 0,
            uptime: Some(Duration::from_secs(3661)),
            exit_code: None,
            enabled: true,
            description: "test service".to_string(),
        };
        assert_eq!(status.uptime_display(), "1h 1m");
    }

    #[test]
    fn test_service_status_uptime_none() {
        let status = ServiceStatus {
            name: "test".to_string(),
            state: ServiceState::Stopped,
            pid: None,
            restart_count: 0,
            uptime: None,
            exit_code: None,
            enabled: true,
            description: String::new(),
        };
        assert_eq!(status.uptime_display(), "-");
    }

    #[test]
    fn test_restart_policy_default() {
        let policy = RestartPolicy::default();
        assert_eq!(policy, RestartPolicy::Always);
    }

    #[test]
    fn test_service_type_default() {
        let stype = ServiceType::default();
        assert_eq!(stype, ServiceType::Simple);
    }

    #[test]
    fn test_service_state_display() {
        assert_eq!(ServiceState::Running.to_string(), "running");
        assert_eq!(ServiceState::Failed.to_string(), "failed");
        assert_eq!(ServiceState::Starting.to_string(), "starting");
        assert_eq!(ServiceState::Stopped.to_string(), "stopped");
        assert_eq!(ServiceState::Stopping.to_string(), "stopping");
        assert_eq!(ServiceState::Exited.to_string(), "exited");
    }

    #[test]
    fn test_service_resources_default() {
        let res = ServiceResources::default();
        assert_eq!(res.memory_max, 0);
        assert_eq!(res.cpu_quota_percent, 0);
        assert_eq!(res.tasks_max, 0);
    }

    #[tokio::test]
    async fn test_service_manager_new() {
        let mgr = ServiceManager::new("/tmp/agnos-test-services");
        let list = mgr.list_services().await;
        assert!(list.is_empty());
    }

    #[tokio::test]
    async fn test_service_manager_register() {
        let mgr = ServiceManager::new("/tmp/agnos-test-services");
        mgr.register(make_def("test-svc", &[])).await;

        let list = mgr.list_services().await;
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "test-svc");
        assert_eq!(list[0].state, ServiceState::Stopped);
    }

    #[tokio::test]
    async fn test_service_manager_get_status() {
        let mgr = ServiceManager::new("/tmp/agnos-test-services");
        mgr.register(make_def("foo", &[])).await;

        let status = mgr.get_status("foo").await;
        assert!(status.is_some());
        let status = status.unwrap();
        assert_eq!(status.name, "foo");
        assert!(status.enabled);

        let missing = mgr.get_status("bar").await;
        assert!(missing.is_none());
    }

    #[tokio::test]
    async fn test_service_manager_stop_already_stopped() {
        let mgr = ServiceManager::new("/tmp/agnos-test-services");
        mgr.register(make_def("stopped-svc", &[])).await;

        // Stopping an already-stopped service should succeed
        let result = mgr.stop_service("stopped-svc").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_service_manager_stop_unknown() {
        let mgr = ServiceManager::new("/tmp/agnos-test-services");
        let result = mgr.stop_service("nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_service_manager_start_unknown() {
        let mgr = ServiceManager::new("/tmp/agnos-test-services");
        let result = mgr.start_service("nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_service_manager_load_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let mgr = ServiceManager::new(dir.path());
        let count = mgr.load_definitions().await.unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_service_manager_load_toml() {
        let dir = tempfile::tempdir().unwrap();
        let toml_content = r#"
name = "test-service"
exec_start = "/bin/true"
description = "A test service"
"#;
        tokio::fs::write(dir.path().join("test-service.toml"), toml_content)
            .await
            .unwrap();

        let mgr = ServiceManager::new(dir.path());
        let count = mgr.load_definitions().await.unwrap();
        assert_eq!(count, 1);

        let status = mgr.get_status("test-service").await.unwrap();
        assert_eq!(status.description, "A test service");
    }

    #[tokio::test]
    async fn test_service_manager_start_real_process() {
        let mgr = ServiceManager::new("/tmp/agnos-test-services-start");
        mgr.register(ServiceDefinition {
            name: "sleeper".to_string(),
            exec_start: "/bin/sleep".to_string(),
            args: vec!["60".to_string()],
            service_type: ServiceType::Simple,
            restart: RestartPolicy::No,
            ..make_def("sleeper", &[])
        })
        .await;

        let result = mgr.start_service("sleeper").await;
        assert!(result.is_ok());

        let status = mgr.get_status("sleeper").await.unwrap();
        assert_eq!(status.state, ServiceState::Running);
        assert!(status.pid.is_some());

        // Clean up
        mgr.stop_service("sleeper").await.unwrap();
        let status = mgr.get_status("sleeper").await.unwrap();
        assert_eq!(status.state, ServiceState::Stopped);
    }

    #[tokio::test]
    async fn test_service_manager_oneshot() {
        let mgr = ServiceManager::new("/tmp/agnos-test-services-oneshot");
        mgr.register(ServiceDefinition {
            name: "init-dirs".to_string(),
            exec_start: "/bin/true".to_string(),
            service_type: ServiceType::Oneshot,
            restart: RestartPolicy::No,
            ..make_def("init-dirs", &[])
        })
        .await;

        let result = mgr.start_service("init-dirs").await;
        assert!(result.is_ok());

        let status = mgr.get_status("init-dirs").await.unwrap();
        assert_eq!(status.state, ServiceState::Exited);
    }

    #[tokio::test]
    async fn test_service_manager_oneshot_failure() {
        let mgr = ServiceManager::new("/tmp/agnos-test-services-oneshot-fail");
        mgr.register(ServiceDefinition {
            name: "bad-init".to_string(),
            exec_start: "/bin/false".to_string(),
            service_type: ServiceType::Oneshot,
            restart: RestartPolicy::No,
            ..make_def("bad-init", &[])
        })
        .await;

        let result = mgr.start_service("bad-init").await;
        assert!(result.is_err());

        let status = mgr.get_status("bad-init").await.unwrap();
        assert_eq!(status.state, ServiceState::Failed);
    }

    #[tokio::test]
    async fn test_service_manager_boot_order() {
        let mgr = ServiceManager::new("/tmp/agnos-test-boot");
        mgr.register(ServiceDefinition {
            name: "base".to_string(),
            exec_start: "/bin/sleep".to_string(),
            args: vec!["60".to_string()],
            service_type: ServiceType::Simple,
            restart: RestartPolicy::No,
            ..make_def("base", &[])
        })
        .await;

        mgr.register(ServiceDefinition {
            name: "dep".to_string(),
            exec_start: "/bin/sleep".to_string(),
            args: vec!["60".to_string()],
            service_type: ServiceType::Simple,
            restart: RestartPolicy::No,
            after: vec!["base".to_string()],
            ..make_def("dep", &[])
        })
        .await;

        let result = mgr.boot().await;
        assert!(result.is_ok());

        let base_status = mgr.get_status("base").await.unwrap();
        let dep_status = mgr.get_status("dep").await.unwrap();
        assert_eq!(base_status.state, ServiceState::Running);
        assert_eq!(dep_status.state, ServiceState::Running);

        // Shutdown in reverse order
        mgr.shutdown_all().await.unwrap();

        let base_status = mgr.get_status("base").await.unwrap();
        let dep_status = mgr.get_status("dep").await.unwrap();
        assert_eq!(base_status.state, ServiceState::Stopped);
        assert_eq!(dep_status.state, ServiceState::Stopped);
    }

    #[tokio::test]
    async fn test_service_manager_restart() {
        let mgr = ServiceManager::new("/tmp/agnos-test-restart");
        mgr.register(ServiceDefinition {
            name: "restartable".to_string(),
            exec_start: "/bin/sleep".to_string(),
            args: vec!["60".to_string()],
            service_type: ServiceType::Simple,
            restart: RestartPolicy::No,
            ..make_def("restartable", &[])
        })
        .await;

        mgr.start_service("restartable").await.unwrap();
        let pid1 = mgr.get_status("restartable").await.unwrap().pid;

        mgr.restart_service("restartable").await.unwrap();
        let pid2 = mgr.get_status("restartable").await.unwrap().pid;

        // Should have a new PID
        assert_ne!(pid1, pid2);

        mgr.stop_service("restartable").await.unwrap();
    }

    #[tokio::test]
    async fn test_service_manager_dependency_auto_start() {
        let mgr = ServiceManager::new("/tmp/agnos-test-dep-auto");
        mgr.register(ServiceDefinition {
            name: "dep-base".to_string(),
            exec_start: "/bin/sleep".to_string(),
            args: vec!["60".to_string()],
            service_type: ServiceType::Simple,
            restart: RestartPolicy::No,
            ..make_def("dep-base", &[])
        })
        .await;

        mgr.register(ServiceDefinition {
            name: "dep-child".to_string(),
            exec_start: "/bin/sleep".to_string(),
            args: vec!["60".to_string()],
            service_type: ServiceType::Simple,
            restart: RestartPolicy::No,
            after: vec!["dep-base".to_string()],
            ..make_def("dep-child", &[])
        })
        .await;

        // Starting dep-child should auto-start dep-base
        mgr.start_service("dep-child").await.unwrap();

        let base = mgr.get_status("dep-base").await.unwrap();
        let child = mgr.get_status("dep-child").await.unwrap();
        assert_eq!(base.state, ServiceState::Running);
        assert_eq!(child.state, ServiceState::Running);

        mgr.shutdown_all().await.unwrap();
    }

    #[test]
    fn test_service_definition_toml_roundtrip() {
        let def = make_def("test", &["dep1", "dep2"]);
        let toml_str = toml::to_string_pretty(&def).unwrap();
        let parsed: ServiceDefinition = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.name, "test");
        assert_eq!(parsed.after, vec!["dep1", "dep2"]);
        assert_eq!(parsed.restart, RestartPolicy::Always);
    }

    #[test]
    fn test_service_definition_minimal_toml() {
        let toml_str = r#"
name = "minimal"
exec_start = "/bin/true"
"#;
        let def: ServiceDefinition = toml::from_str(toml_str).unwrap();
        assert_eq!(def.name, "minimal");
        assert!(def.after.is_empty());
        assert_eq!(def.restart, RestartPolicy::Always);
        assert_eq!(def.service_type, ServiceType::Simple);
        assert!(def.enabled);
    }
}
