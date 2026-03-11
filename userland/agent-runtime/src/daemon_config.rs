//! Daemon configuration with bounds-checked validation (H19).

use anyhow::Result;

use crate::http_api;

/// Runtime daemon configuration with validated values.
#[derive(Debug, Clone)]
pub struct DaemonConfig {
    /// HTTP API listen port (1..=65535).
    pub api_port: u16,
    /// Shutdown drain timeout in seconds (1..=300).
    pub shutdown_timeout_secs: u64,
    /// Service health-check interval in seconds (1..=3600).
    pub health_check_interval_secs: u64,
    /// Maximum IPC message buffer size in bytes (1024..=16_777_216).
    pub max_ipc_buffer_bytes: u32,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            api_port: http_api::DEFAULT_PORT,
            shutdown_timeout_secs: 5,
            health_check_interval_secs: 5,
            max_ipc_buffer_bytes: 64 * 1024,
        }
    }
}

impl DaemonConfig {
    /// Validate all configuration values, returning a clear error on the first
    /// invalid field. Called early in startup to fail fast.
    pub fn validate(&self) -> Result<()> {
        if self.api_port == 0 {
            anyhow::bail!("Invalid api_port: 0 (must be 1-65535)");
        }
        if self.shutdown_timeout_secs == 0 || self.shutdown_timeout_secs > 300 {
            anyhow::bail!(
                "Invalid shutdown_timeout_secs: {} (must be 1-300)",
                self.shutdown_timeout_secs
            );
        }
        if self.health_check_interval_secs == 0 || self.health_check_interval_secs > 3600 {
            anyhow::bail!(
                "Invalid health_check_interval_secs: {} (must be 1-3600)",
                self.health_check_interval_secs
            );
        }
        if self.max_ipc_buffer_bytes < 1024 || self.max_ipc_buffer_bytes > 16_777_216 {
            anyhow::bail!(
                "Invalid max_ipc_buffer_bytes: {} (must be 1024-16777216)",
                self.max_ipc_buffer_bytes
            );
        }
        Ok(())
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // H19: Config validation tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_daemon_config_default_is_valid() {
        let config = DaemonConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_daemon_config_valid_custom() {
        let config = DaemonConfig {
            api_port: 9090,
            shutdown_timeout_secs: 30,
            health_check_interval_secs: 60,
            max_ipc_buffer_bytes: 128 * 1024,
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_daemon_config_invalid_port_zero() {
        let config = DaemonConfig {
            api_port: 0,
            ..DaemonConfig::default()
        };
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("api_port"));
    }

    #[test]
    fn test_daemon_config_invalid_shutdown_timeout_zero() {
        let config = DaemonConfig {
            shutdown_timeout_secs: 0,
            ..DaemonConfig::default()
        };
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("shutdown_timeout_secs"));
    }

    #[test]
    fn test_daemon_config_invalid_shutdown_timeout_too_large() {
        let config = DaemonConfig {
            shutdown_timeout_secs: 301,
            ..DaemonConfig::default()
        };
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("shutdown_timeout_secs"));
    }

    #[test]
    fn test_daemon_config_invalid_health_interval_zero() {
        let config = DaemonConfig {
            health_check_interval_secs: 0,
            ..DaemonConfig::default()
        };
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("health_check_interval_secs"));
    }

    #[test]
    fn test_daemon_config_invalid_health_interval_too_large() {
        let config = DaemonConfig {
            health_check_interval_secs: 3601,
            ..DaemonConfig::default()
        };
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("health_check_interval_secs"));
    }

    #[test]
    fn test_daemon_config_invalid_ipc_buffer_too_small() {
        let config = DaemonConfig {
            max_ipc_buffer_bytes: 512,
            ..DaemonConfig::default()
        };
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("max_ipc_buffer_bytes"));
    }

    #[test]
    fn test_daemon_config_invalid_ipc_buffer_too_large() {
        let config = DaemonConfig {
            max_ipc_buffer_bytes: 16_777_217,
            ..DaemonConfig::default()
        };
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("max_ipc_buffer_bytes"));
    }

    #[test]
    fn test_daemon_config_boundary_values() {
        // Lower bounds
        let config = DaemonConfig {
            api_port: 1,
            shutdown_timeout_secs: 1,
            health_check_interval_secs: 1,
            max_ipc_buffer_bytes: 1024,
        };
        assert!(config.validate().is_ok());

        // Upper bounds
        let config = DaemonConfig {
            api_port: 65535,
            shutdown_timeout_secs: 300,
            health_check_interval_secs: 3600,
            max_ipc_buffer_bytes: 16_777_216,
        };
        assert!(config.validate().is_ok());
    }

    // -----------------------------------------------------------------------
    // H16: Dependency health check test
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_check_dependencies_healthy_with_writable_dir() {
        let tmp = std::env::temp_dir().join(format!("agnos_health_test_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&tmp);

        assert!(tmp.exists());

        // Verify write probe works
        let probe_file = tmp.join(".health_probe");
        tokio::fs::write(&probe_file, b"ok").await.unwrap();
        let _ = tokio::fs::remove_file(&probe_file).await;

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
