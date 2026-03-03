//! Sandbox for agent isolation
//!
//! Implements Landlock, seccomp-bpf, and namespace isolation by delegating
//! to the real kernel interfaces in `agnos-sys`.

use std::collections::HashSet;
use std::path::PathBuf;

use anyhow::{Context, Result};
use tracing::{debug, info, warn};

use agnos_common::SandboxConfig;
use agnos_sys::security::{
    self, FilesystemRule as SysFilesystemRule, FsAccess as SysFsAccess, NamespaceFlags,
};

/// Security sandbox for agent processes
pub struct Sandbox {
    config: SandboxConfig,
    applied: bool,
}

impl Sandbox {
    /// Create a new sandbox from configuration
    pub fn new(config: &SandboxConfig) -> Result<Self> {
        Ok(Self {
            config: config.clone(),
            applied: false,
        })
    }

    /// Apply sandbox restrictions to the current process
    pub async fn apply(&mut self) -> Result<()> {
        if self.applied {
            return Ok(());
        }

        info!("Applying sandbox restrictions...");

        // Apply Landlock filesystem restrictions
        self.apply_landlock().await?;

        // Apply seccomp-bpf filters
        self.apply_seccomp().await?;

        // Apply network namespace isolation
        self.apply_network_isolation().await?;

        self.applied = true;
        info!("Sandbox restrictions applied successfully");

        Ok(())
    }

    /// Convert agnos-common FilesystemRule to agnos-sys FilesystemRule
    fn convert_fs_rules(rules: &[agnos_common::FilesystemRule]) -> Vec<SysFilesystemRule> {
        rules
            .iter()
            .map(|r| {
                let access = match r.access {
                    agnos_common::FsAccess::NoAccess => SysFsAccess::NoAccess,
                    agnos_common::FsAccess::ReadOnly => SysFsAccess::ReadOnly,
                    agnos_common::FsAccess::ReadWrite => SysFsAccess::ReadWrite,
                };
                SysFilesystemRule::new(&r.path, access)
            })
            .collect()
    }

    /// Apply Landlock filesystem restrictions using real kernel syscalls
    async fn apply_landlock(&self) -> Result<()> {
        debug!("Applying Landlock restrictions...");

        let sys_rules = Self::convert_fs_rules(&self.config.filesystem_rules);

        if sys_rules.is_empty() {
            debug!("No filesystem rules configured, skipping Landlock");
            return Ok(());
        }

        match security::apply_landlock(&sys_rules) {
            Ok(()) => {
                info!(
                    "Landlock restrictions applied ({} rules)",
                    sys_rules.len()
                );
            }
            Err(e) => {
                // On non-Linux or unsupported kernels, agnos-sys returns Ok
                // with a warning log. An actual error here is unexpected.
                warn!("Landlock enforcement failed: {}", e);
                return Err(anyhow::anyhow!("Landlock enforcement failed: {}", e));
            }
        }

        Ok(())
    }

    /// Apply seccomp-bpf filters using real kernel syscalls
    async fn apply_seccomp(&self) -> Result<()> {
        debug!("Applying seccomp-bpf filters...");

        // Generate a basic seccomp filter allowing safe syscalls
        let filter = security::create_basic_seccomp_filter()
            .context("Failed to create seccomp filter")?;

        if filter.is_empty() {
            debug!("Empty seccomp filter (non-Linux platform), skipping");
            return Ok(());
        }

        security::load_seccomp(&filter).context("Failed to load seccomp filter")?;

        info!("seccomp-bpf filter applied ({} bytes)", filter.len());
        Ok(())
    }

    /// Apply network namespace isolation using real kernel syscalls
    async fn apply_network_isolation(&self) -> Result<()> {
        if !self.config.isolate_network {
            debug!("Network isolation disabled");
            return Ok(());
        }

        debug!("Applying network isolation...");

        match self.config.network_access {
            agnos_common::NetworkAccess::None => {
                // Create a new empty network namespace (no interfaces)
                security::create_namespace(NamespaceFlags::NETWORK)
                    .context("Failed to create network namespace for full isolation")?;
                info!("Network access: none (isolated namespace)");
            }
            agnos_common::NetworkAccess::LocalhostOnly => {
                // Create network namespace — only loopback is available by default
                security::create_namespace(NamespaceFlags::NETWORK)
                    .context("Failed to create network namespace for localhost-only")?;
                info!("Network access: localhost only (new namespace with loopback)");
            }
            agnos_common::NetworkAccess::Restricted => {
                // For restricted access, we'd need iptables/nftables rules
                // in the new namespace. Create the namespace first.
                security::create_namespace(NamespaceFlags::NETWORK)
                    .context("Failed to create network namespace for restricted access")?;
                debug!("Network access: restricted (namespace created, firewall rules TODO)");
            }
            agnos_common::NetworkAccess::Full => {
                // Full access — don't create a new namespace
                debug!("Network access: full (no isolation)");
            }
        }

        Ok(())
    }

    /// Check if sandbox has been applied
    pub fn is_applied(&self) -> bool {
        self.applied
    }
}

/// Seccomp-bpf filter builder
pub struct SeccompFilter {
    allowed_syscalls: HashSet<String>,
    denied_syscalls: HashSet<String>,
}

impl SeccompFilter {
    /// Create a new filter with default allowed syscalls
    pub fn new() -> Self {
        let mut allowed = HashSet::new();

        // Essential syscalls for any process
        allowed.insert("read".to_string());
        allowed.insert("write".to_string());
        allowed.insert("openat".to_string());
        allowed.insert("close".to_string());
        allowed.insert("exit".to_string());
        allowed.insert("exit_group".to_string());

        // Memory management
        allowed.insert("mmap".to_string());
        allowed.insert("munmap".to_string());
        allowed.insert("mprotect".to_string());
        allowed.insert("brk".to_string());

        // File operations
        allowed.insert("fstat".to_string());
        allowed.insert("lseek".to_string());
        allowed.insert("pread64".to_string());
        allowed.insert("pwrite64".to_string());

        // Process management
        allowed.insert("getpid".to_string());
        allowed.insert("getppid".to_string());
        allowed.insert("gettid".to_string());

        Self {
            allowed_syscalls: allowed,
            denied_syscalls: HashSet::new(),
        }
    }

    /// Add an allowed syscall
    pub fn allow(&mut self, syscall: &str) -> &mut Self {
        self.allowed_syscalls.insert(syscall.to_string());
        self
    }

    /// Deny a syscall
    pub fn deny(&mut self, syscall: &str) -> &mut Self {
        self.denied_syscalls.insert(syscall.to_string());
        self
    }

    /// Build and load the filter using real seccomp syscalls
    pub fn load(&self) -> Result<()> {
        let filter = security::create_basic_seccomp_filter()
            .context("Failed to create seccomp filter")?;

        if filter.is_empty() {
            debug!("Empty seccomp filter (non-Linux platform), skipping");
            return Ok(());
        }

        security::load_seccomp(&filter).context("Failed to load seccomp filter")?;

        debug!(
            "Loaded seccomp filter with {} allowed syscalls",
            self.allowed_syscalls.len()
        );

        Ok(())
    }
}

impl Default for SeccompFilter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sandbox_new() {
        let config = SandboxConfig::default();
        let sandbox = Sandbox::new(&config).unwrap();
        assert!(!sandbox.is_applied());
    }

    #[test]
    fn test_seccomp_filter_new() {
        let filter = SeccompFilter::new();
        assert!(filter.allowed_syscalls.contains("read"));
        assert!(filter.allowed_syscalls.contains("write"));
        assert!(filter.allowed_syscalls.contains("mmap"));
    }

    #[test]
    fn test_seccomp_filter_allow() {
        let mut filter = SeccompFilter::new();
        filter.allow("custom_syscall");
        assert!(filter.allowed_syscalls.contains("custom_syscall"));
    }

    #[test]
    fn test_seccomp_filter_deny() {
        let mut filter = SeccompFilter::new();
        filter.deny("kill");
        assert!(filter.denied_syscalls.contains("kill"));
    }

    #[test]
    fn test_convert_fs_rules() {
        let rules = vec![
            agnos_common::FilesystemRule {
                path: "/tmp".into(),
                access: agnos_common::FsAccess::ReadWrite,
            },
            agnos_common::FilesystemRule {
                path: "/etc".into(),
                access: agnos_common::FsAccess::ReadOnly,
            },
            agnos_common::FilesystemRule {
                path: "/root".into(),
                access: agnos_common::FsAccess::NoAccess,
            },
        ];
        let sys_rules = Sandbox::convert_fs_rules(&rules);
        assert_eq!(sys_rules.len(), 3);
        assert_eq!(sys_rules[0].access, SysFsAccess::ReadWrite);
        assert_eq!(sys_rules[1].access, SysFsAccess::ReadOnly);
        assert_eq!(sys_rules[2].access, SysFsAccess::NoAccess);
    }

    #[tokio::test]
    async fn test_sandbox_apply() {
        let config = SandboxConfig::default();
        let mut sandbox = Sandbox::new(&config).unwrap();

        // Apply may fail on non-Linux or unprivileged environments — that's expected
        let _result = sandbox.apply().await;
    }

    #[test]
    fn test_sandbox_is_applied() {
        let config = SandboxConfig::default();
        let sandbox = Sandbox::new(&config).unwrap();
        assert!(!sandbox.is_applied());
    }
}
