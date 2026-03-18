//! Configuration validation for agnova.

use anyhow::{bail, Result};
use tracing::debug;

use super::types::{FirewallDefault, TrustEnforcementMode};
use super::AgnovaInstaller;

impl AgnovaInstaller {
    /// Validate the installation configuration. Returns a list of warnings
    /// (non-fatal). Errors are returned via `Result::Err`.
    pub fn validate_config(&self) -> Result<Vec<String>> {
        let mut warnings = Vec::new();

        // Target device must be set
        if self.config.disk.target_device.is_empty() {
            bail!("target device is not set");
        }

        // HIGH 2: Validate device path
        let dev = &self.config.disk.target_device;
        if !dev.starts_with("/dev/") {
            bail!("target device must start with /dev/");
        }
        if dev.contains("..")
            || dev.contains(' ')
            || dev.contains(';')
            || dev.contains('|')
            || dev.contains('&')
            || dev.contains('`')
            || dev.contains('\n')
        {
            bail!("target device path contains invalid characters");
        }
        // After "/dev/" the rest should be alphanumeric, slashes, or hyphens
        let dev_suffix = &dev[5..];
        if dev_suffix.is_empty()
            || !dev_suffix
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '/' || c == '-' || c == '_')
        {
            bail!("target device path contains invalid characters after /dev/");
        }

        // Must have at least one partition
        if self.config.disk.partitions.is_empty() {
            bail!("no partitions defined");
        }

        // Username is required
        if self.config.user.username.is_empty() {
            bail!("username is empty");
        }

        // MEDIUM 2: Validate username
        let uname = &self.config.user.username;
        if uname == "root" {
            bail!("username cannot be 'root'");
        }
        if uname.len() > 32 {
            bail!("username must be 1-32 characters");
        }
        if !uname.starts_with(|c: char| c.is_ascii_lowercase()) {
            bail!("username must start with a lowercase letter");
        }
        if !uname
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
        {
            bail!("username contains invalid characters (allowed: a-z, 0-9, _, -)");
        }

        // Hostname should be set
        if self.config.network.hostname.is_empty() {
            bail!("hostname is empty");
        }

        // MEDIUM 1: Validate hostname
        let hostname = &self.config.network.hostname;
        if hostname.len() > 63 {
            bail!("hostname must be 1-63 characters");
        }
        if hostname.starts_with('-') || hostname.ends_with('-') {
            bail!("hostname must not start or end with a hyphen");
        }
        if hostname.starts_with('.') {
            bail!("hostname must not start with a dot");
        }
        if !hostname
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
        {
            bail!("hostname contains invalid characters (allowed: alphanumeric and hyphens)");
        }

        // MEDIUM 6: Static network requires static_ip
        if !self.config.network.use_dhcp && self.config.network.static_ip.is_none() {
            bail!("static network configuration requires a static_ip");
        }

        // CRITICAL 1: Validate kernel params
        let dangerous_substrings = ["init=", "rd.break", "single", "rescue", "break="];
        let dangerous_chars = ['|', ';', '&', '`', '\n'];
        for param in &self.config.bootloader.kernel_params {
            for substr in &dangerous_substrings {
                if param.contains(substr) {
                    bail!("dangerous kernel parameter detected: '{}'", param);
                }
            }
            for ch in &dangerous_chars {
                if param.contains(*ch) {
                    bail!("kernel parameter contains dangerous character: '{}'", param);
                }
            }
        }

        // Validate partition labels (used as args to parted/mkfs, must be safe)
        for (i, part) in self.config.disk.partitions.iter().enumerate() {
            if part.label.is_empty() {
                bail!("partition {} has an empty label", i + 1);
            }
            if part.label.len() > 36 {
                bail!("partition {} label is too long (max 36 chars)", i + 1);
            }
            if !part
                .label
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            {
                bail!(
                    "partition {} label '{}' contains invalid characters (allowed: a-z, A-Z, 0-9, -, _)",
                    i + 1,
                    part.label
                );
            }
        }

        // Only the last partition may use size_mb = None (fill remaining)
        for (i, part) in self.config.disk.partitions.iter().enumerate() {
            if part.size_mb.is_none() && i + 1 < self.config.disk.partitions.len() {
                bail!(
                    "partition {} ('{}') has no size (fill remaining) but is not the last partition",
                    i + 1,
                    part.label
                );
            }
        }

        // MBR partition table supports at most 4 primary partitions
        if !self.config.disk.use_gpt && self.config.disk.partitions.len() > 4 {
            bail!(
                "MBR partition table limited to 4 primary partitions, {} configured",
                self.config.disk.partitions.len()
            );
        }

        // Validate full_name if provided (used as -c arg to useradd)
        if let Some(ref full_name) = self.config.user.full_name {
            if full_name.len() > 256 {
                bail!("user full_name is too long (max 256 chars)");
            }
            // Disallow shell metacharacters and colons (passwd field separator)
            if full_name
                .chars()
                .any(|c| matches!(c, ':' | ';' | '|' | '&' | '`' | '\n' | '\0'))
            {
                bail!("user full_name contains invalid characters");
            }
        }

        // Validate group names
        for group in &self.config.user.groups {
            if group.is_empty() || group.len() > 32 {
                bail!("group name must be 1-32 characters");
            }
            if !group
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
            {
                bail!(
                    "group name '{}' contains invalid characters (allowed: a-z, 0-9, _, -)",
                    group
                );
            }
        }

        // Check for a root partition
        let has_root = self
            .config
            .disk
            .partitions
            .iter()
            .any(|p| p.mount_point == "/");
        if !has_root {
            bail!("no root partition (mount_point = \"/\") defined");
        }

        // Warn if LUKS is requested in config but disk layout says no
        if self.config.security.enable_luks && !self.config.disk.encrypt {
            warnings.push("security.enable_luks is true but disk.encrypt is false".to_string());
        }

        // Warn about permissive trust enforcement
        if self.config.security.trust_enforcement == TrustEnforcementMode::Permissive {
            warnings.push("trust enforcement is set to permissive".to_string());
        }

        // Warn about allow-all firewall
        if self.config.security.firewall_default == FirewallDefault::Allow {
            warnings.push("firewall default policy is allow — not recommended".to_string());
        }

        debug!(
            "agnova: config validation passed with {} warning(s)",
            warnings.len()
        );
        Ok(warnings)
    }
}
