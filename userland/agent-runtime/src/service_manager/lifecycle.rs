//! Internal service lifecycle helpers: spawning, stopping, dependency levels.

use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use tokio::process::Command;
use tokio::sync::RwLock;
use tracing::{debug, error, info};

use super::types::{ServiceDefinition, ServiceRuntime, ServiceState, ServiceType};

/// Load a single service TOML file.
pub(crate) async fn load_service_file(path: &Path) -> Result<ServiceDefinition> {
    let content = tokio::fs::read_to_string(path)
        .await
        .with_context(|| format!("Failed to read {}", path.display()))?;
    let def: ServiceDefinition =
        toml::from_str(&content).with_context(|| format!("Failed to parse {}", path.display()))?;
    Ok(def)
}

/// Start a service (internal, lock-free on entry — acquires its own locks).
pub(crate) async fn start_service_inner(
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
pub(crate) fn dependency_levels(
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
