//! Lightweight dependency health checks before systemd ready notification (H16).

use std::path::Path;

use anyhow::{Context, Result};
use tracing::info;

use crate::daemon_config::DaemonConfig;

/// Perform lightweight health checks on critical dependencies before
/// declaring the daemon ready.
pub async fn check_dependencies_healthy(config: &DaemonConfig, data_dir: &Path) -> Result<()> {
    // 1. Verify the HTTP API port is available (bindable)
    let addr: std::net::SocketAddr = ([127, 0, 0, 1], config.api_port).into();
    let probe = tokio::net::TcpListener::bind(addr).await;
    match probe {
        Ok(_listener) => {
            info!("Health check: port {} is available", config.api_port);
        }
        Err(e) => {
            anyhow::bail!(
                "Health check failed: port {} is not bindable: {}",
                config.api_port,
                e
            );
        }
    }

    // 2. Verify the data directory is accessible
    if !data_dir.exists() {
        tokio::fs::create_dir_all(data_dir).await.with_context(|| {
            format!(
                "Health check failed: cannot create data dir {}",
                data_dir.display()
            )
        })?;
    }
    let probe_file = data_dir.join(".health_probe");
    tokio::fs::write(&probe_file, b"ok")
        .await
        .with_context(|| {
            format!(
                "Health check failed: data dir {} is not writable",
                data_dir.display()
            )
        })?;
    let _ = tokio::fs::remove_file(&probe_file).await;

    // 3. Verify the IPC socket directory is accessible
    let ipc_dir = Path::new("/run/agnos/agents");
    if !ipc_dir.exists() {
        tokio::fs::create_dir_all(ipc_dir).await.with_context(|| {
            format!(
                "Health check failed: cannot create IPC dir {}",
                ipc_dir.display()
            )
        })?;
    }

    info!("All dependency health checks passed");
    Ok(())
}
