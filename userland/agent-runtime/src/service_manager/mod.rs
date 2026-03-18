//! AGNOS Init / Service Manager
//!
//! Provides PID-1-style service supervision with dependency ordering,
//! parallel startup, health monitoring, and sd_notify integration.
//! Services are defined as TOML files in `/etc/agnos/services/`.
//!
//! Submodules:
//! - **types**: All types, enums, and data structures
//! - **lifecycle**: Service spawning, stopping, and dependency level helpers
//! - **health**: Health monitoring, cron scheduling, and task scheduler
//! - **tests**: Unit tests

pub mod health;
pub mod types;

mod lifecycle;

#[cfg(test)]
mod tests;

// Re-export all public types so external consumers see the same flat API
// as they did when this was a single service_manager.rs file.
pub use health::{CronSchedule, ScheduledTask, TaskScheduler};
pub use types::{
    FleetConfig, ReconciliationPlan, RestartPolicy, ServiceDefinition, ServiceResources,
    ServiceState, ServiceStatus, ServiceType,
};

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

use std::sync::Arc;

use lifecycle::{dependency_levels, load_service_file, start_service_inner};
use types::ServiceRuntime;

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

    let mut queue: std::collections::VecDeque<&str> = in_degree
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
    pub(crate) services: Arc<RwLock<HashMap<String, ServiceRuntime>>>,
    pub(crate) definitions: Arc<RwLock<HashMap<String, ServiceDefinition>>>,
    config_dir: PathBuf,
    /// H20: Tracks the order in which services were actually started,
    /// used as the authoritative source for reverse-order shutdown.
    start_order: Arc<RwLock<Vec<String>>>,
}

impl ServiceManager {
    /// Create a new service manager that loads definitions from `config_dir`.
    pub fn new(config_dir: impl Into<PathBuf>) -> Self {
        Self {
            services: Arc::new(RwLock::new(HashMap::new())),
            definitions: Arc::new(RwLock::new(HashMap::new())),
            config_dir: config_dir.into(),
            start_order: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Load all `.toml` service definitions from the config directory.
    pub async fn load_definitions(&self) -> Result<usize> {
        let dir = &self.config_dir;
        if !dir.is_dir() {
            info!(
                "Service config dir {} does not exist, creating it",
                dir.display()
            );
            tokio::fs::create_dir_all(dir)
                .await
                .with_context(|| format!("Failed to create {}", dir.display()))?;
            return Ok(0);
        }

        let mut count = 0;
        let mut entries = tokio::fs::read_dir(dir)
            .await
            .with_context(|| format!("Failed to read {}", dir.display()))?;

        // Load all definitions first without holding locks
        let mut loaded = Vec::new();
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().map_or(true, |e| e != "toml") {
                continue;
            }

            match load_service_file(&path).await {
                Ok(def) => {
                    info!("Loaded service definition: {}", def.name);
                    loaded.push(def);
                }
                Err(e) => {
                    warn!("Failed to load {}: {}", path.display(), e);
                }
            }
        }

        // Now acquire locks briefly to insert
        let mut defs = self.definitions.write().await;
        let mut svcs = self.services.write().await;

        for def in loaded {
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
        info!("Boot order: {}", order.join(" -> "));

        // H20: Record the topological start order for deterministic shutdown
        {
            let mut start_order = self.start_order.write().await;
            start_order.clear();
            start_order.extend(order.iter().cloned());
        }

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

        start_service_inner(&self.services, &self.definitions, name).await?;

        // H20: Record start order for deterministic reverse shutdown
        {
            let mut order = self.start_order.write().await;
            if !order.contains(&name.to_string()) {
                order.push(name.to_string());
            }
        }

        Ok(())
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
                    if let Err(e) = nix::sys::signal::kill(
                        nix::unistd::Pid::from_raw(pid as i32),
                        nix::sys::signal::Signal::SIGTERM,
                    ) {
                        warn!(
                            "Failed to send SIGTERM to service {} (pid {}): {}",
                            name, pid, e
                        );
                    }
                }
            }

            // Wait up to 10 seconds for graceful shutdown
            let timeout_result = tokio::time::timeout(Duration::from_secs(10), child.wait()).await;

            match timeout_result {
                Ok(Ok(status)) => {
                    info!("Service {} exited with {}", name, status);
                    svc.exit_code = status.code();
                }
                _ => {
                    warn!("Service {} did not exit gracefully, sending SIGKILL", name);
                    if let Err(e) = child.kill().await {
                        warn!("Failed to SIGKILL service {}: {}", name, e);
                    }
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
    ///
    /// H20: Uses the recorded start order (from boot) reversed, ensuring
    /// dependents are stopped before their dependencies. Falls back to
    /// topological sort of definitions if no start order was recorded.
    pub async fn shutdown_all(&self) -> Result<()> {
        info!("AGNOS service manager: shutting down all services");

        // H20: Prefer recorded start order (reversed) for deterministic shutdown
        let reversed = {
            let start_order = self.start_order.read().await;
            if !start_order.is_empty() {
                let mut rev = start_order.clone();
                rev.reverse();
                info!("Shutdown order (reverse of start): {}", rev.join(" -> "));
                rev
            } else {
                // Fallback to topological sort of definitions
                let defs = self.definitions.read().await;
                let all: HashMap<String, ServiceDefinition> = defs.clone();
                drop(defs);

                if all.is_empty() {
                    return Ok(());
                }

                let order = topological_sort(&all).unwrap_or_default();
                let rev: Vec<_> = order.into_iter().rev().collect();
                info!("Shutdown order (topo fallback): {}", rev.join(" -> "));
                rev
            }
        };

        for name in &reversed {
            if let Err(e) = self.stop_service(name).await {
                warn!("Error stopping {}: {}", name, e);
            }
        }

        info!("AGNOS service manager: all services stopped");
        Ok(())
    }

    /// Returns the recorded start order (for testing/inspection).
    pub async fn get_start_order(&self) -> Vec<String> {
        self.start_order.read().await.clone()
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
        use types::{RestartPolicy, ServiceType};

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

            info!("Restarting service {} in {}s (attempt {})", name, delay, {
                let svcs = self.services.read().await;
                svcs.get(&name).map_or(0, |s| s.restart_count + 1)
            });

            tokio::time::sleep(Duration::from_secs(delay)).await;

            if let Err(e) = start_service_inner(&self.services, &self.definitions, &name).await {
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
                    if let Err(e) = sock.send_to(b"READY=1", &socket_path) {
                        warn!("Failed to send sd_notify READY=1: {}", e);
                    }
                }
            }
            info!("Sent sd_notify READY=1 to {}", socket_path);
        }
    }
}

// Pull in the with_context extension used in load_definitions
use anyhow::Context;
