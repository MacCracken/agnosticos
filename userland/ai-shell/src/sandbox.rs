//! Sandboxing utilities
//!
//! Provides Landlock and seccomp integration for command isolation,
//! delegating to the real kernel interfaces in `agnos-sys`.

use anyhow::{Context, Result};
use tracing::{debug, info, warn};

use agnos_sys::security::{
    self, FilesystemRule, FsAccess, NamespaceFlags,
};

/// Sandboxed execution context
pub struct Sandbox {
    enabled: bool,
}

impl Sandbox {
    pub fn new(enabled: bool) -> Self {
        Self { enabled }
    }

    /// Apply Landlock restrictions for shell command execution.
    ///
    /// Grants read-only access to system paths and read-write to /tmp.
    /// The calling process is restricted after this call.
    pub fn apply_landlock(&self) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }

        let rules = vec![
            FilesystemRule::read_only("/usr"),
            FilesystemRule::read_only("/lib"),
            FilesystemRule::read_only("/lib64"),
            FilesystemRule::read_only("/bin"),
            FilesystemRule::read_only("/sbin"),
            FilesystemRule::read_only("/etc"),
            FilesystemRule::read_write("/tmp"),
            FilesystemRule::read_write("/var/tmp"),
        ];

        match security::apply_landlock(&rules) {
            Ok(()) => {
                info!("Shell Landlock restrictions applied ({} rules)", rules.len());
            }
            Err(e) => {
                warn!("Landlock enforcement failed (may not be supported): {}", e);
                // Don't fail hard — graceful degradation on unsupported kernels
            }
        }

        Ok(())
    }

    /// Apply seccomp filters for shell command execution.
    ///
    /// Loads a basic seccomp-BPF filter that allows safe syscalls
    /// and kills the process on dangerous ones.
    pub fn apply_seccomp(&self) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }

        let filter = security::create_basic_seccomp_filter()
            .context("Failed to create seccomp filter")?;

        if filter.is_empty() {
            debug!("Empty seccomp filter (non-Linux platform), skipping");
            return Ok(());
        }

        match security::load_seccomp(&filter) {
            Ok(()) => {
                info!("Shell seccomp filter applied ({} bytes)", filter.len());
            }
            Err(e) => {
                warn!("Seccomp enforcement failed (may not be supported): {}", e);
                // Don't fail hard — graceful degradation
            }
        }

        Ok(())
    }
}

impl Default for Sandbox {
    fn default() -> Self {
        Self::new(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sandbox_default_enabled() {
        let sandbox = Sandbox::default();
        assert!(sandbox.enabled);
    }

    #[test]
    fn test_sandbox_new_disabled() {
        let sandbox = Sandbox::new(false);
        assert!(!sandbox.enabled);
    }

    #[test]
    fn test_sandbox_apply_landlock_disabled() {
        let sandbox = Sandbox::new(false);
        assert!(sandbox.apply_landlock().is_ok());
    }

    #[test]
    fn test_sandbox_apply_seccomp_disabled() {
        let sandbox = Sandbox::new(false);
        assert!(sandbox.apply_seccomp().is_ok());
    }

    #[test]
    fn test_sandbox_apply_landlock_enabled() {
        let sandbox = Sandbox::new(true);
        // On non-Linux or unprivileged, this degrades gracefully
        assert!(sandbox.apply_landlock().is_ok());
    }

    #[test]
    fn test_sandbox_apply_seccomp_enabled() {
        let sandbox = Sandbox::new(true);
        // On non-Linux or unprivileged, this degrades gracefully
        assert!(sandbox.apply_seccomp().is_ok());
    }
}
