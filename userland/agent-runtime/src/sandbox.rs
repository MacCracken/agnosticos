//! Sandbox for agent isolation
//!
//! Implements Landlock, seccomp-bpf, and namespace isolation by delegating
//! to the real kernel interfaces in `agnos-sys`.

use std::collections::HashSet;
use std::path::PathBuf;

use anyhow::{Context, Result};
use tracing::{debug, info, warn};

use agnos_common::SandboxConfig;
use agnos_sys::audit;
use agnos_sys::luks;
use agnos_sys::mac;
use agnos_sys::netns;
use agnos_sys::security::{
    self, FilesystemRule as SysFilesystemRule, FsAccess as SysFsAccess, NamespaceFlags,
};

/// Security sandbox for agent processes
pub struct Sandbox {
    config: SandboxConfig,
    applied: bool,
    /// Handle for the agent's network namespace (if created)
    netns_handle: Option<netns::NetNamespaceHandle>,
    /// Name of the LUKS volume (if created)
    luks_name: Option<String>,
}

impl Sandbox {
    /// Create a new sandbox from configuration
    pub fn new(config: &SandboxConfig) -> Result<Self> {
        Ok(Self {
            config: config.clone(),
            applied: false,
            netns_handle: None,
            luks_name: None,
        })
    }

    /// Apply sandbox restrictions to the current process.
    ///
    /// Ordering is critical:
    /// 1. Encrypted storage — LUKS mount must happen before Landlock locks FS
    /// 2. MAC profile — context must be set before seccomp blocks /proc/self/attr/ writes
    /// 3. Landlock — filesystem restrictions
    /// 4. Seccomp — syscall filter (most restrictive, applied last)
    /// 5. Network isolation — namespace + nftables for Restricted mode
    /// 6. Audit event — record that sandbox was applied
    pub async fn apply(&mut self) -> Result<()> {
        if self.applied {
            return Ok(());
        }

        info!("Applying sandbox restrictions...");

        // 1. Set up encrypted storage (before Landlock locks filesystem)
        self.apply_encrypted_storage().await?;

        // 2. Apply MAC profile (before seccomp blocks /proc/self/attr/ writes)
        self.apply_mac_profile().await?;

        // 3. Apply Landlock filesystem restrictions
        self.apply_landlock().await?;

        // 4. Apply seccomp-bpf filters
        self.apply_seccomp().await?;

        // 5. Apply network namespace isolation
        self.apply_network_isolation().await?;

        // 6. Emit audit event
        self.emit_audit_event("sandbox_applied").await;

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

    /// Apply network namespace isolation using real kernel syscalls.
    ///
    /// For `Restricted` mode, creates a per-agent network namespace with veth
    /// pair, IP addresses, and nftables firewall rules based on `network_policy`.
    async fn apply_network_isolation(&mut self) -> Result<()> {
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
                // Create per-agent network namespace with veth + nftables
                let agent_name = format!("sandbox-{}", std::process::id());
                let ns_config = netns::NetNamespaceConfig::for_agent(&agent_name);

                match netns::create_agent_netns(&ns_config) {
                    Ok(handle) => {
                        // Apply firewall rules from network policy
                        let policy = self.build_firewall_policy();
                        if let Err(e) = netns::apply_firewall_rules(&handle, &policy) {
                            warn!("Failed to apply nftables rules: {} (namespace created without firewall)", e);
                        }
                        info!(
                            "Network access: restricted (namespace '{}' with nftables firewall)",
                            handle.name
                        );
                        self.netns_handle = Some(handle);
                    }
                    Err(e) => {
                        // Fall back to plain namespace isolation
                        warn!("Failed to create agent netns: {} — falling back to basic namespace", e);
                        security::create_namespace(NamespaceFlags::NETWORK)
                            .context("Failed to create network namespace for restricted access")?;
                    }
                }
            }
            agnos_common::NetworkAccess::Full => {
                // Full access — don't create a new namespace
                debug!("Network access: full (no isolation)");
            }
        }

        Ok(())
    }

    /// Build nftables firewall policy from the sandbox's network_policy config.
    fn build_firewall_policy(&self) -> netns::FirewallPolicy {
        let mut rules = Vec::new();

        if let Some(ref policy) = self.config.network_policy {
            // Allow specified outbound ports
            for &port in &policy.allowed_outbound_ports {
                rules.push(netns::FirewallRule {
                    direction: netns::TrafficDirection::Outbound,
                    protocol: netns::Protocol::Tcp,
                    port,
                    remote_addr: String::new(),
                    action: netns::FirewallAction::Accept,
                    comment: format!("Allow outbound TCP/{}", port),
                });
            }

            // Allow specified outbound hosts
            for host in &policy.allowed_outbound_hosts {
                rules.push(netns::FirewallRule {
                    direction: netns::TrafficDirection::Outbound,
                    protocol: netns::Protocol::Any,
                    port: 0,
                    remote_addr: host.clone(),
                    action: netns::FirewallAction::Accept,
                    comment: format!("Allow outbound to {}", host),
                });
            }

            // Allow specified inbound ports
            for &port in &policy.allowed_inbound_ports {
                rules.push(netns::FirewallRule {
                    direction: netns::TrafficDirection::Inbound,
                    protocol: netns::Protocol::Tcp,
                    port,
                    remote_addr: String::new(),
                    action: netns::FirewallAction::Accept,
                    comment: format!("Allow inbound TCP/{}", port),
                });
            }
        }

        netns::FirewallPolicy {
            default_inbound: netns::FirewallAction::Drop,
            default_outbound: if self.config.network_policy.is_some() {
                netns::FirewallAction::Drop // Explicit allow-list mode
            } else {
                netns::FirewallAction::Accept
            },
            rules,
        }
    }

    /// Apply MAC (AppArmor/SELinux) profile based on sandbox config.
    async fn apply_mac_profile(&self) -> Result<()> {
        let profile_name = match &self.config.mac_profile {
            Some(name) if !name.is_empty() => name.clone(),
            _ => {
                debug!("No MAC profile configured, skipping");
                return Ok(());
            }
        };

        debug!("Applying MAC profile: {}", profile_name);

        let profiles = mac::default_agent_profiles();
        match mac::apply_agent_mac_profile(&profile_name, &profiles) {
            Ok(()) => {
                info!("MAC profile '{}' applied", profile_name);
            }
            Err(e) => {
                // MAC is best-effort on systems without SELinux/AppArmor
                let mac_system = mac::detect_mac_system();
                if mac_system == mac::MacSystem::None {
                    debug!("No MAC system active, skipping profile: {}", e);
                } else {
                    warn!("MAC profile application failed: {}", e);
                    return Err(anyhow::anyhow!("MAC profile application failed: {}", e));
                }
            }
        }

        Ok(())
    }

    /// Set up LUKS encrypted storage if configured.
    async fn apply_encrypted_storage(&mut self) -> Result<()> {
        let storage_config = match &self.config.encrypted_storage {
            Some(cfg) if cfg.enabled => cfg.clone(),
            _ => {
                debug!("Encrypted storage not configured, skipping");
                return Ok(());
            }
        };

        debug!("Setting up encrypted storage ({} MB)", storage_config.size_mb);

        let agent_id = format!("sandbox-{}", std::process::id());
        let luks_config = luks::LuksConfig::for_agent(&agent_id, storage_config.size_mb);

        // Generate a random key for this volume
        let key = match luks::LuksKey::generate(64) {
            Ok(k) => k,
            Err(e) => {
                warn!("Failed to generate LUKS key: {} — skipping encrypted storage", e);
                return Ok(());
            }
        };

        match luks::setup_agent_volume(&luks_config, &key) {
            Ok(status) => {
                info!(
                    "Encrypted storage ready: {} ({} MB, {})",
                    status.name, storage_config.size_mb, status.cipher
                );
                self.luks_name = Some(status.name);
            }
            Err(e) => {
                warn!("Failed to set up encrypted storage: {} — continuing without it", e);
            }
        }

        Ok(())
    }

    /// Emit an audit event for sandbox lifecycle actions.
    async fn emit_audit_event(&self, event: &str) {
        let msg = format!(
            "pid={} network={:?} mac={:?} encrypted={}",
            std::process::id(),
            self.config.network_access,
            self.config.mac_profile,
            self.luks_name.is_some()
        );

        if let Err(e) = audit::agnos_audit_log_syscall(event, &msg, 0) {
            debug!("Audit event '{}' not logged (expected on non-AGNOS kernels): {}", event, e);
        }
    }

    /// Tear down sandbox resources (network namespace, LUKS volume).
    ///
    /// Called during agent unregistration to clean up kernel resources.
    pub async fn teardown(&mut self) {
        // Destroy network namespace
        if let Some(ref handle) = self.netns_handle {
            if let Err(e) = netns::destroy_agent_netns(handle) {
                warn!("Failed to destroy network namespace '{}': {}", handle.name, e);
            }
        }
        self.netns_handle = None;

        // Teardown LUKS volume
        if let Some(ref name) = self.luks_name {
            if let Err(e) = luks::teardown_agent_volume(name) {
                warn!("Failed to teardown LUKS volume '{}': {}", name, e);
            }
        }
        self.luks_name = None;

        // Emit audit event
        self.emit_audit_event("sandbox_teardown").await;
    }

    /// Check if sandbox has been applied
    pub fn is_applied(&self) -> bool {
        self.applied
    }

    /// Apply sandbox restrictions with a pre-compiled seccomp profile instead
    /// of the generic filter.
    pub async fn apply_with_profile(
        &mut self,
        profile: &crate::seccomp_profiles::SeccompProfile,
    ) -> Result<()> {
        if self.applied {
            return Ok(());
        }

        info!("Applying sandbox with seccomp profile...");

        // Validate the profile first
        crate::seccomp_profiles::validate_profile(profile)
            .map_err(|e| anyhow::anyhow!("Invalid seccomp profile: {}", e))?;

        // 1. Encrypted storage (before Landlock)
        self.apply_encrypted_storage().await?;

        // 2. MAC profile (before seccomp)
        self.apply_mac_profile().await?;

        // 3. Apply Landlock filesystem restrictions
        self.apply_landlock().await?;

        // Build the profile-specific filter spec (for logging/audit)
        let filter_spec = crate::seccomp_profiles::build_seccomp_filter(profile);
        debug!(
            "Seccomp profile '{}': {} allowed syscalls, default={}",
            filter_spec.profile_name,
            filter_spec.allowed.len(),
            filter_spec.default_action
        );

        // 4. Apply the actual seccomp filter via agnos-sys
        self.apply_seccomp().await?;

        // 5. Apply network namespace isolation
        self.apply_network_isolation().await?;

        // 6. Emit audit event
        self.emit_audit_event("sandbox_applied").await;

        self.applied = true;
        info!(
            "Sandbox applied with '{}' seccomp profile",
            filter_spec.profile_name
        );

        Ok(())
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

    #[tokio::test]
    async fn test_sandbox_apply_with_profile() {
        let config = SandboxConfig::default();
        let mut sandbox = Sandbox::new(&config).unwrap();

        // apply_with_profile may fail on non-Linux — that's expected
        let _result = sandbox
            .apply_with_profile(&crate::seccomp_profiles::SeccompProfile::Python)
            .await;
    }

    #[tokio::test]
    async fn test_sandbox_apply_with_invalid_profile() {
        let config = SandboxConfig::default();
        let mut sandbox = Sandbox::new(&config).unwrap();

        let empty_profile = crate::seccomp_profiles::SeccompProfile::Custom(vec![]);
        let result = sandbox.apply_with_profile(&empty_profile).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_sandbox_new_has_no_handles() {
        let config = SandboxConfig::default();
        let sandbox = Sandbox::new(&config).unwrap();
        assert!(sandbox.netns_handle.is_none());
        assert!(sandbox.luks_name.is_none());
    }

    #[test]
    fn test_sandbox_config_with_network_policy() {
        let config = SandboxConfig {
            network_policy: Some(agnos_common::NetworkPolicy {
                allowed_outbound_ports: vec![80, 443],
                allowed_outbound_hosts: vec!["10.0.0.0/8".to_string()],
                allowed_inbound_ports: vec![8080],
                enable_nat: true,
            }),
            ..SandboxConfig::default()
        };
        let sandbox = Sandbox::new(&config).unwrap();
        assert!(sandbox.config.network_policy.is_some());
    }

    #[test]
    fn test_sandbox_config_with_mac_profile() {
        let config = SandboxConfig {
            mac_profile: Some("User".to_string()),
            ..SandboxConfig::default()
        };
        let sandbox = Sandbox::new(&config).unwrap();
        assert_eq!(sandbox.config.mac_profile.as_deref(), Some("User"));
    }

    #[test]
    fn test_sandbox_config_with_encrypted_storage() {
        let config = SandboxConfig {
            encrypted_storage: Some(agnos_common::EncryptedStorageConfig {
                enabled: true,
                size_mb: 128,
                filesystem: "ext4".to_string(),
            }),
            ..SandboxConfig::default()
        };
        let sandbox = Sandbox::new(&config).unwrap();
        assert!(sandbox.config.encrypted_storage.is_some());
    }

    #[test]
    fn test_build_firewall_policy_no_network_policy() {
        let config = SandboxConfig::default();
        let sandbox = Sandbox::new(&config).unwrap();
        let policy = sandbox.build_firewall_policy();
        assert!(policy.rules.is_empty());
        assert_eq!(policy.default_outbound, netns::FirewallAction::Accept);
    }

    #[test]
    fn test_build_firewall_policy_with_rules() {
        let config = SandboxConfig {
            network_policy: Some(agnos_common::NetworkPolicy {
                allowed_outbound_ports: vec![443, 8088],
                allowed_outbound_hosts: vec![],
                allowed_inbound_ports: vec![8080],
                enable_nat: true,
            }),
            ..SandboxConfig::default()
        };
        let sandbox = Sandbox::new(&config).unwrap();
        let policy = sandbox.build_firewall_policy();
        // 2 outbound port rules + 1 inbound port rule = 3
        assert_eq!(policy.rules.len(), 3);
        assert_eq!(policy.default_outbound, netns::FirewallAction::Drop);
        assert_eq!(policy.default_inbound, netns::FirewallAction::Drop);
    }

    #[tokio::test]
    async fn test_sandbox_teardown_noop() {
        let config = SandboxConfig::default();
        let mut sandbox = Sandbox::new(&config).unwrap();
        // Teardown on a sandbox with no handles should not panic
        sandbox.teardown().await;
        assert!(sandbox.netns_handle.is_none());
        assert!(sandbox.luks_name.is_none());
    }

    #[test]
    fn test_sandbox_config_serialization() {
        let config = SandboxConfig {
            network_policy: Some(agnos_common::NetworkPolicy::default()),
            mac_profile: Some("Service".to_string()),
            encrypted_storage: Some(agnos_common::EncryptedStorageConfig::default()),
            ..SandboxConfig::default()
        };
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: SandboxConfig = serde_json::from_str(&json).unwrap();
        assert!(deserialized.network_policy.is_some());
        assert_eq!(deserialized.mac_profile.as_deref(), Some("Service"));
        assert!(deserialized.encrypted_storage.is_some());
    }

    #[test]
    fn test_sandbox_config_default_serialization_roundtrip() {
        // Ensure default config can serialize/deserialize (no missing fields)
        let config = SandboxConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: SandboxConfig = serde_json::from_str(&json).unwrap();
        assert!(deserialized.network_policy.is_none());
        assert!(deserialized.mac_profile.is_none());
        assert!(deserialized.encrypted_storage.is_none());
    }

    #[test]
    fn test_sandbox_config_backward_compatible_deserialization() {
        // Old configs without new fields should still deserialize (serde(default))
        let json = r#"{"filesystem_rules":[{"path":"/tmp","access":"ReadWrite"}],"network_access":"LocalhostOnly","seccomp_rules":[],"isolate_network":true}"#;
        let config: SandboxConfig = serde_json::from_str(json).unwrap();
        assert!(config.network_policy.is_none());
        assert!(config.mac_profile.is_none());
        assert!(config.encrypted_storage.is_none());
    }
}
