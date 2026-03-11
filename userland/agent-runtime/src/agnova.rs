//! Agnova — OS Installer for AGNOS
//!
//! Handles disk partitioning, encryption, bootloader installation,
//! base system deployment, and first-boot configuration. Named from
//! AGNOS + Latin "nova" (new) — agnova creates new AGNOS installations.

use std::fmt;

use anyhow::{bail, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// InstallMode
// ---------------------------------------------------------------------------

/// The installation profile, which determines the default package set and
/// system configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum InstallMode {
    /// Headless server with agent-runtime, LLM gateway, SSH.
    Server,
    /// Full desktop with Wayland compositor, AI shell, desktop environment.
    Desktop,
    /// Bare-minimum boot: kernel, init, agnoshi shell.
    Minimal,
    /// User-defined package selection.
    Custom,
}

impl fmt::Display for InstallMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Server => write!(f, "Server"),
            Self::Desktop => write!(f, "Desktop"),
            Self::Minimal => write!(f, "Minimal"),
            Self::Custom => write!(f, "Custom"),
        }
    }
}

// ---------------------------------------------------------------------------
// Filesystem
// ---------------------------------------------------------------------------

/// Supported filesystem types for partitions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Filesystem {
    Ext4,
    Btrfs,
    Xfs,
    Vfat,
    Swap,
}

impl fmt::Display for Filesystem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ext4 => write!(f, "ext4"),
            Self::Btrfs => write!(f, "btrfs"),
            Self::Xfs => write!(f, "xfs"),
            Self::Vfat => write!(f, "vfat"),
            Self::Swap => write!(f, "swap"),
        }
    }
}

// ---------------------------------------------------------------------------
// PartitionFlag
// ---------------------------------------------------------------------------

/// Flags that can be set on a partition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PartitionFlag {
    Boot,
    Esp,
    Lvm,
    Raid,
}

// ---------------------------------------------------------------------------
// PartitionSpec
// ---------------------------------------------------------------------------

/// Specification for a single partition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionSpec {
    pub label: String,
    pub mount_point: String,
    pub filesystem: Filesystem,
    /// Size in megabytes. `None` means "fill remaining disk space".
    pub size_mb: Option<u64>,
    pub flags: Vec<PartitionFlag>,
}

// ---------------------------------------------------------------------------
// DiskLayout
// ---------------------------------------------------------------------------

/// Complete disk layout for the installation target.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskLayout {
    /// Block device path, e.g. "/dev/sda" or "/dev/nvme0n1".
    pub target_device: String,
    pub partitions: Vec<PartitionSpec>,
    /// Use GPT partition table (default true; false = MBR).
    pub use_gpt: bool,
    /// Encrypt the root partition with LUKS2.
    pub encrypt: bool,
}

impl Default for DiskLayout {
    fn default() -> Self {
        Self {
            target_device: String::new(),
            partitions: Vec::new(),
            use_gpt: true,
            encrypt: false,
        }
    }
}

// ---------------------------------------------------------------------------
// BootloaderType / BootloaderConfig
// ---------------------------------------------------------------------------

/// Supported bootloaders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BootloaderType {
    SystemdBoot,
    Grub2,
}

impl fmt::Display for BootloaderType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SystemdBoot => write!(f, "systemd-boot"),
            Self::Grub2 => write!(f, "GRUB 2"),
        }
    }
}

/// Bootloader configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootloaderConfig {
    pub bootloader_type: BootloaderType,
    pub timeout_secs: u32,
    pub default_entry: String,
    pub kernel_params: Vec<String>,
}

impl Default for BootloaderConfig {
    fn default() -> Self {
        Self {
            bootloader_type: BootloaderType::SystemdBoot,
            timeout_secs: 5,
            default_entry: "agnos".to_string(),
            kernel_params: vec!["quiet".to_string()],
        }
    }
}

// ---------------------------------------------------------------------------
// NetworkConfig
// ---------------------------------------------------------------------------

/// Network configuration for the installed system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub hostname: String,
    pub use_dhcp: bool,
    pub static_ip: Option<String>,
    pub gateway: Option<String>,
    pub dns: Vec<String>,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            hostname: "agnos".to_string(),
            use_dhcp: true,
            static_ip: None,
            gateway: None,
            dns: vec!["1.1.1.1".to_string(), "8.8.8.8".to_string()],
        }
    }
}

// ---------------------------------------------------------------------------
// UserConfig
// ---------------------------------------------------------------------------

/// Initial user account configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserConfig {
    pub username: String,
    pub full_name: Option<String>,
    /// Login shell — defaults to agnoshi.
    pub shell: String,
    pub groups: Vec<String>,
    pub ssh_keys: Vec<String>,
    pub enable_sudo: bool,
}

impl Default for UserConfig {
    fn default() -> Self {
        Self {
            username: String::new(),
            full_name: None,
            shell: "/usr/bin/agnoshi".to_string(),
            groups: vec!["wheel".to_string(), "agents".to_string()],
            ssh_keys: Vec::new(),
            enable_sudo: true,
        }
    }
}

// ---------------------------------------------------------------------------
// SecurityConfig
// ---------------------------------------------------------------------------

/// Trust enforcement mode for the installed system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TrustEnforcementMode {
    Strict,
    Permissive,
    AuditOnly,
}

impl fmt::Display for TrustEnforcementMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Strict => write!(f, "strict"),
            Self::Permissive => write!(f, "permissive"),
            Self::AuditOnly => write!(f, "audit-only"),
        }
    }
}

/// Firewall default policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FirewallDefault {
    Deny,
    Allow,
}

impl fmt::Display for FirewallDefault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Deny => write!(f, "deny"),
            Self::Allow => write!(f, "allow"),
        }
    }
}

/// Security hardening options for the installed system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    pub enable_luks: bool,
    pub enable_secureboot: bool,
    pub enable_tpm: bool,
    pub enable_dmverity: bool,
    pub trust_enforcement: TrustEnforcementMode,
    pub firewall_default: FirewallDefault,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            enable_luks: true,
            enable_secureboot: true,
            enable_tpm: true,
            enable_dmverity: true,
            trust_enforcement: TrustEnforcementMode::Strict,
            firewall_default: FirewallDefault::Deny,
        }
    }
}

// ---------------------------------------------------------------------------
// PackageSelection
// ---------------------------------------------------------------------------

/// Which .ark packages to install.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageSelection {
    /// Always installed regardless of mode.
    pub base_packages: Vec<String>,
    /// Added based on InstallMode.
    pub mode_packages: Vec<String>,
    /// User-selected additional packages.
    pub extra_packages: Vec<String>,
    /// Estimated total disk usage in MB.
    pub total_size_mb: u64,
}

impl PackageSelection {
    /// Total number of packages across all lists.
    pub fn total_count(&self) -> usize {
        self.base_packages.len() + self.mode_packages.len() + self.extra_packages.len()
    }
}

// ---------------------------------------------------------------------------
// InstallConfig
// ---------------------------------------------------------------------------

/// Complete installation configuration — everything needed to install AGNOS.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallConfig {
    pub mode: InstallMode,
    pub disk: DiskLayout,
    pub bootloader: BootloaderConfig,
    pub network: NetworkConfig,
    pub user: UserConfig,
    pub security: SecurityConfig,
    pub packages: PackageSelection,
    pub locale: String,
    pub timezone: String,
}

impl Default for InstallConfig {
    fn default() -> Self {
        Self {
            mode: InstallMode::Desktop,
            disk: DiskLayout::default(),
            bootloader: BootloaderConfig::default(),
            network: NetworkConfig::default(),
            user: UserConfig::default(),
            security: SecurityConfig::default(),
            packages: PackageSelection {
                base_packages: Vec::new(),
                mode_packages: Vec::new(),
                extra_packages: Vec::new(),
                total_size_mb: 0,
            },
            locale: "en_US.UTF-8".to_string(),
            timezone: "UTC".to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// InstallPhase
// ---------------------------------------------------------------------------

/// Ordered phases of the installation process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum InstallPhase {
    ValidateConfig,
    PartitionDisk,
    FormatFilesystems,
    SetupEncryption,
    MountFilesystems,
    InstallBase,
    InstallPackages,
    ConfigureSystem,
    InstallBootloader,
    CreateUser,
    SetupSecurity,
    FirstBootSetup,
    Cleanup,
    Complete,
}

impl InstallPhase {
    /// All phases in execution order.
    pub const ALL: &'static [InstallPhase] = &[
        Self::ValidateConfig,
        Self::PartitionDisk,
        Self::SetupEncryption,
        Self::FormatFilesystems,
        Self::MountFilesystems,
        Self::InstallBase,
        Self::InstallPackages,
        Self::ConfigureSystem,
        Self::InstallBootloader,
        Self::CreateUser,
        Self::SetupSecurity,
        Self::FirstBootSetup,
        Self::Cleanup,
        Self::Complete,
    ];

    /// Zero-based index in the phase sequence.
    pub fn index(self) -> usize {
        Self::ALL
            .iter()
            .position(|&p| p == self)
            .expect("phase must be in ALL")
    }

    /// Next phase, or `None` if this is `Complete`.
    pub fn next(self) -> Option<InstallPhase> {
        let idx = self.index();
        Self::ALL.get(idx + 1).copied()
    }
}

impl fmt::Display for InstallPhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ValidateConfig => write!(f, "Validating configuration"),
            Self::PartitionDisk => write!(f, "Partitioning disk"),
            Self::FormatFilesystems => write!(f, "Formatting filesystems"),
            Self::SetupEncryption => write!(f, "Setting up encryption"),
            Self::MountFilesystems => write!(f, "Mounting filesystems"),
            Self::InstallBase => write!(f, "Installing base system"),
            Self::InstallPackages => write!(f, "Installing packages"),
            Self::ConfigureSystem => write!(f, "Configuring system"),
            Self::InstallBootloader => write!(f, "Installing bootloader"),
            Self::CreateUser => write!(f, "Creating user account"),
            Self::SetupSecurity => write!(f, "Setting up security"),
            Self::FirstBootSetup => write!(f, "Preparing first boot"),
            Self::Cleanup => write!(f, "Cleaning up"),
            Self::Complete => write!(f, "Installation complete"),
        }
    }
}

// ---------------------------------------------------------------------------
// InstallProgress
// ---------------------------------------------------------------------------

/// Live progress information for the running installation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallProgress {
    pub current_phase: InstallPhase,
    /// Progress within the current phase (0.0 – 1.0).
    pub phase_progress: f32,
    /// Overall progress across all phases (0.0 – 1.0).
    pub overall_progress: f32,
    pub message: String,
    pub started_at: DateTime<Utc>,
    pub estimated_remaining_secs: Option<u64>,
}

impl InstallProgress {
    fn new() -> Self {
        Self {
            current_phase: InstallPhase::ValidateConfig,
            phase_progress: 0.0,
            overall_progress: 0.0,
            message: "Preparing installation".to_string(),
            started_at: Utc::now(),
            estimated_remaining_secs: None,
        }
    }
}

// ---------------------------------------------------------------------------
// InstallError
// ---------------------------------------------------------------------------

/// An error that occurred during a specific installation phase.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallError {
    pub phase: InstallPhase,
    pub message: String,
    /// Whether the installation can continue past this error.
    pub recoverable: bool,
}

// ---------------------------------------------------------------------------
// InstallResult
// ---------------------------------------------------------------------------

/// Summary returned when the installation finishes (or fails).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallResult {
    pub success: bool,
    pub phases_completed: Vec<InstallPhase>,
    pub errors: Vec<InstallError>,
    pub duration_secs: u64,
    pub installed_packages: usize,
    pub disk_used_mb: u64,
}

// ---------------------------------------------------------------------------
// First-boot helpers (free functions)
// ---------------------------------------------------------------------------

/// Generate a random machine-id suitable for `/etc/machine-id`.
pub fn generate_machine_id() -> String {
    Uuid::new_v4().to_string().replace('-', "")
}

/// Generate `/etc/hostname` content.
pub fn generate_hostname_config(hostname: &str) -> String {
    format!("{}\n", hostname)
}

/// Generate a basic `/etc/fstab` from the partition specifications.
pub fn generate_fstab(partitions: &[PartitionSpec], encrypt: bool) -> String {
    let mut lines = vec![
        "# /etc/fstab — generated by agnova".to_string(),
        "# <device>  <mount>  <type>  <options>  <dump>  <pass>".to_string(),
    ];

    for part in partitions {
        let device = if encrypt && part.mount_point == "/" {
            "/dev/mapper/agnos-root".to_string()
        } else {
            format!("LABEL={}", part.label)
        };

        let options = if part.filesystem == Filesystem::Swap {
            "sw"
        } else if part.mount_point == "/" {
            "defaults,errors=remount-ro"
        } else {
            "defaults"
        };

        let pass = if part.mount_point == "/" {
            "1"
        } else if part.filesystem == Filesystem::Swap || part.filesystem == Filesystem::Vfat {
            "0"
        } else {
            "2"
        };

        let mount = if part.filesystem == Filesystem::Swap {
            "none"
        } else {
            &part.mount_point
        };

        lines.push(format!(
            "{}  {}  {}  {}  0  {}",
            device, mount, part.filesystem, options, pass
        ));
    }

    lines.join("\n") + "\n"
}

/// Default kernel command-line parameters based on security settings.
pub fn default_kernel_params(security: &SecurityConfig) -> Vec<String> {
    let mut params = vec!["quiet".to_string(), "loglevel=3".to_string()];

    if security.enable_luks {
        params.push("rd.luks=1".to_string());
    }
    if security.enable_secureboot {
        params.push("lockdown=integrity".to_string());
    }
    if security.enable_tpm {
        params.push("tpm_tis.interrupts=0".to_string());
    }
    if security.enable_dmverity {
        params.push("dm_verity.verify=1".to_string());
    }

    // LSM stack
    params.push("lsm=landlock,lockdown,yama,apparmor,bpf".to_string());

    params
}

// ---------------------------------------------------------------------------
// AgnovaInstaller
// ---------------------------------------------------------------------------

/// Main installer orchestrator. Tracks configuration, progress, and phase
/// transitions for a complete AGNOS installation.
pub struct AgnovaInstaller {
    pub config: InstallConfig,
    pub progress: InstallProgress,
    pub log: Vec<String>,
    pub completed_phases: Vec<InstallPhase>,
    pub errors: Vec<InstallError>,
}

impl AgnovaInstaller {
    /// Create a new installer with the given configuration.
    pub fn new(config: InstallConfig) -> Self {
        info!("agnova: creating installer for mode={}", config.mode);
        Self {
            config,
            progress: InstallProgress::new(),
            log: Vec::new(),
            completed_phases: Vec::new(),
            errors: Vec::new(),
        }
    }

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

    /// Produce the standard disk layout: 512 MB ESP + remaining space for root.
    pub fn default_disk_layout(device: &str, encrypt: bool) -> DiskLayout {
        DiskLayout {
            target_device: device.to_string(),
            partitions: vec![
                PartitionSpec {
                    label: "ESP".to_string(),
                    mount_point: "/boot/efi".to_string(),
                    filesystem: Filesystem::Vfat,
                    size_mb: Some(512),
                    flags: vec![PartitionFlag::Boot, PartitionFlag::Esp],
                },
                PartitionSpec {
                    label: "agnos-root".to_string(),
                    mount_point: "/".to_string(),
                    filesystem: Filesystem::Ext4,
                    size_mb: None, // fill remaining
                    flags: Vec::new(),
                },
            ],
            use_gpt: true,
            encrypt,
        }
    }

    /// Default package selection for a given installation mode.
    pub fn default_packages(mode: &InstallMode) -> PackageSelection {
        let base_packages: Vec<String> = [
            "linux-kernel",
            "linux-firmware",
            "agnos-init",
            "agnos-sys",
            "agnos-common",
            "agnoshi",
            "shakti",
            "daimon",
            "hoosh",
            "systemd",
            "dbus",
            "networkmanager",
            "nftables",
            "openssh",
            "coreutils",
            "util-linux",
            "bash",
            "zsh",
            "curl",
            "wget",
            "ca-certificates",
            "gnupg",
            "tar",
            "gzip",
            "xz",
            "bzip2",
            "iproute2",
            "iputils",
            "less",
            "nano",
            "man-pages",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        let (mode_packages, size) = match mode {
            InstallMode::Server => {
                let pkgs: Vec<String> = [
                    "hoosh-server",
                    "daimon-server",
                    "ark",
                    "nous",
                    "prometheus-node-exporter",
                    "fail2ban",
                    "tmux",
                ]
                .iter()
                .map(|s| s.to_string())
                .collect();
                (pkgs, 2400)
            }
            InstallMode::Desktop => {
                let pkgs: Vec<String> = [
                    "aethersafha",
                    "pipewire",
                    "wireplumber",
                    "mesa",
                    "vulkan-loader",
                    "fonts-noto",
                    "fonts-jetbrains-mono",
                    "ark",
                    "nous",
                    "hoosh-server",
                    "daimon-server",
                    "xdg-utils",
                    "nautilus",
                    "evince",
                    "firefox",
                ]
                .iter()
                .map(|s| s.to_string())
                .collect();
                (pkgs, 4800)
            }
            InstallMode::Minimal => (Vec::new(), 800),
            InstallMode::Custom => (Vec::new(), 1200),
        };

        PackageSelection {
            base_packages,
            mode_packages,
            extra_packages: Vec::new(),
            total_size_mb: size,
        }
    }

    /// Total number of installation phases.
    pub fn phase_count() -> usize {
        InstallPhase::ALL.len()
    }

    /// The current installation phase.
    pub fn current_phase(&self) -> &InstallPhase {
        &self.progress.current_phase
    }

    /// Current progress snapshot.
    pub fn progress(&self) -> &InstallProgress {
        &self.progress
    }

    /// Advance to the next phase. Returns `true` if there is a next phase,
    /// `false` if the installation is already complete.
    pub fn advance_phase(&mut self) -> bool {
        let current = self.progress.current_phase;

        // Block advancement if the current phase has a non-recoverable error
        if self
            .errors
            .iter()
            .any(|e| e.phase == current && !e.recoverable)
        {
            warn!(
                "agnova: cannot advance past non-recoverable failure at {}",
                current
            );
            return false;
        }

        if let Some(next) = current.next() {
            self.completed_phases.push(current);
            self.progress.current_phase = next;
            let idx = next.index() as f32;
            let total = InstallPhase::ALL.len() as f32;
            self.progress.overall_progress = idx / total;
            self.progress.phase_progress = 0.0;
            self.progress.message = format!("{}", next);
            info!("agnova: phase {} -> {}", current, next);
            true
        } else {
            // Already at Complete
            false
        }
    }

    /// Record a failure at the current phase.
    pub fn fail_phase(&mut self, error: String) {
        let phase = self.progress.current_phase;
        warn!("agnova: phase {} failed: {}", phase, error);
        self.errors.push(InstallError {
            phase,
            message: error.clone(),
            recoverable: !matches!(
                phase,
                InstallPhase::PartitionDisk
                    | InstallPhase::SetupEncryption
                    | InstallPhase::FormatFilesystems
                    | InstallPhase::InstallBase
                    | InstallPhase::InstallBootloader
            ),
        });
        self.log.push(format!("ERROR [{}]: {}", phase, error));
    }

    /// Whether the installation has reached the Complete phase.
    pub fn is_complete(&self) -> bool {
        self.progress.current_phase == InstallPhase::Complete
    }

    /// Build the final installation result.
    pub fn result(&self) -> InstallResult {
        let elapsed = Utc::now()
            .signed_duration_since(self.progress.started_at)
            .num_seconds()
            .unsigned_abs();

        InstallResult {
            success: self.errors.is_empty() && self.is_complete(),
            phases_completed: self.completed_phases.clone(),
            errors: self.errors.clone(),
            duration_secs: elapsed,
            installed_packages: self.config.packages.total_count(),
            disk_used_mb: self.config.packages.total_size_mb,
        }
    }

    /// Append a message to the install log.
    pub fn log_message(&mut self, msg: String) {
        debug!("agnova: {}", msg);
        self.log.push(msg);
    }

    /// Read-only access to the log.
    pub fn get_log(&self) -> &[String] {
        &self.log
    }

    /// Rough estimate of total installation time in seconds.
    pub fn estimate_install_time(mode: &InstallMode) -> u64 {
        match mode {
            InstallMode::Minimal => 120,
            InstallMode::Server => 300,
            InstallMode::Desktop => 480,
            InstallMode::Custom => 360,
        }
    }

    /// Generate the kernel command line string for the bootloader entry.
    pub fn kernel_cmdline(config: &InstallConfig) -> String {
        let mut params = config.bootloader.kernel_params.clone();

        // Merge security-derived params, avoiding duplicates
        for p in default_kernel_params(&config.security) {
            if !params.contains(&p) {
                params.push(p);
            }
        }

        // Root device
        if config.disk.encrypt {
            params.push("root=/dev/mapper/agnos-root".to_string());
        } else {
            params.push("root=LABEL=agnos-root".to_string());
        }

        params.join(" ")
    }
}

// ---------------------------------------------------------------------------
// Phase execution — system operation descriptors
// ---------------------------------------------------------------------------

/// A concrete system operation to execute during installation.
/// These are descriptors — the actual execution happens in the installer
/// binary which calls out to system tools.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SystemOp {
    /// Run a shell command with the given args.
    Command {
        binary: String,
        args: Vec<String>,
        description: String,
        /// If true, failure aborts the installation.
        fatal: bool,
    },
    /// Write content to a file.
    WriteFile {
        path: String,
        content: String,
        mode: u32,
        owner: Option<String>,
    },
    /// Create a directory.
    MakeDir {
        path: String,
        mode: u32,
        parents: bool,
    },
    /// Create a symlink.
    Symlink { target: String, link: String },
    /// Mount a filesystem.
    Mount {
        device: String,
        mount_point: String,
        fs_type: String,
        options: Vec<String>,
    },
    /// Unmount a filesystem.
    Unmount { mount_point: String },
}

impl fmt::Display for SystemOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Command {
                binary,
                args,
                description,
                ..
            } => write!(f, "{}: {} {}", description, binary, args.join(" ")),
            Self::WriteFile { path, .. } => write!(f, "write {}", path),
            Self::MakeDir { path, .. } => write!(f, "mkdir {}", path),
            Self::Symlink { target, link } => write!(f, "symlink {} -> {}", link, target),
            Self::Mount {
                device,
                mount_point,
                ..
            } => write!(f, "mount {} on {}", device, mount_point),
            Self::Unmount { mount_point } => write!(f, "umount {}", mount_point),
        }
    }
}

/// A phase execution plan: ordered list of system operations for one phase.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseOps {
    pub phase: InstallPhase,
    pub description: String,
    pub operations: Vec<SystemOp>,
}

impl AgnovaInstaller {
    /// Generate the operations needed for disk partitioning.
    pub fn plan_partition_ops(&self) -> PhaseOps {
        let disk = &self.config.disk;
        let device = &disk.target_device;
        let mut ops = Vec::new();

        // Create GPT partition table
        if disk.use_gpt {
            ops.push(SystemOp::Command {
                binary: "parted".into(),
                args: vec!["-s".into(), device.clone(), "mklabel".into(), "gpt".into()],
                description: "Create GPT partition table".into(),
                fatal: true,
            });
        } else {
            ops.push(SystemOp::Command {
                binary: "parted".into(),
                args: vec![
                    "-s".into(),
                    device.clone(),
                    "mklabel".into(),
                    "msdos".into(),
                ],
                description: "Create MBR partition table".into(),
                fatal: true,
            });
        }

        // Create partitions
        let mut start_mb: u64 = 1; // Start at 1 MiB (alignment)
        for (i, part) in disk.partitions.iter().enumerate() {
            let end = if let Some(size) = part.size_mb {
                format!("{}MiB", start_mb + size)
            } else {
                "100%".into()
            };
            let fs_type = match part.filesystem {
                Filesystem::Vfat => "fat32",
                Filesystem::Swap => "linux-swap",
                _ => "ext4",
            };

            ops.push(SystemOp::Command {
                binary: "parted".into(),
                args: vec![
                    "-s".into(),
                    device.clone(),
                    "mkpart".into(),
                    part.label.clone(),
                    fs_type.into(),
                    format!("{}MiB", start_mb),
                    end.clone(),
                ],
                description: format!("Create partition {} ({})", i + 1, part.label),
                fatal: true,
            });

            // Set flags
            for flag in &part.flags {
                let flag_name = match flag {
                    PartitionFlag::Boot => "boot",
                    PartitionFlag::Esp => "esp",
                    PartitionFlag::Lvm => "lvm",
                    PartitionFlag::Raid => "raid",
                };
                ops.push(SystemOp::Command {
                    binary: "parted".into(),
                    args: vec![
                        "-s".into(),
                        device.clone(),
                        "set".into(),
                        format!("{}", i + 1),
                        flag_name.into(),
                        "on".into(),
                    ],
                    description: format!("Set {} flag on partition {}", flag_name, i + 1),
                    fatal: true,
                });
            }

            if let Some(size) = part.size_mb {
                start_mb += size;
            }
        }

        PhaseOps {
            phase: InstallPhase::PartitionDisk,
            description: format!("Partition {}", device),
            operations: ops,
        }
    }

    /// Generate the operations needed for filesystem formatting.
    pub fn plan_format_ops(&self) -> PhaseOps {
        let disk = &self.config.disk;
        let device = &disk.target_device;
        let mut ops = Vec::new();

        for (i, part) in disk.partitions.iter().enumerate() {
            let part_dev = Self::partition_device(device, i);

            let mkfs_cmd = match part.filesystem {
                Filesystem::Ext4 => vec![
                    "mkfs.ext4".into(),
                    "-L".into(),
                    part.label.clone(),
                    part_dev.clone(),
                ],
                Filesystem::Btrfs => vec![
                    "mkfs.btrfs".into(),
                    "-L".into(),
                    part.label.clone(),
                    "-f".into(),
                    part_dev.clone(),
                ],
                Filesystem::Xfs => vec![
                    "mkfs.xfs".into(),
                    "-L".into(),
                    part.label.clone(),
                    part_dev.clone(),
                ],
                Filesystem::Vfat => vec![
                    "mkfs.vfat".into(),
                    "-F".into(),
                    "32".into(),
                    "-n".into(),
                    part.label.clone(),
                    part_dev.clone(),
                ],
                Filesystem::Swap => vec![
                    "mkswap".into(),
                    "-L".into(),
                    part.label.clone(),
                    part_dev.clone(),
                ],
            };

            ops.push(SystemOp::Command {
                binary: mkfs_cmd[0].clone(),
                args: mkfs_cmd[1..].to_vec(),
                description: format!(
                    "Format {} as {} ({})",
                    part_dev, part.filesystem, part.label
                ),
                fatal: true,
            });
        }

        PhaseOps {
            phase: InstallPhase::FormatFilesystems,
            description: "Format filesystems".into(),
            operations: ops,
        }
    }

    /// Generate the operations needed for LUKS encryption setup.
    pub fn plan_encryption_ops(&self) -> PhaseOps {
        let disk = &self.config.disk;
        let device = &disk.target_device;
        let mut ops = Vec::new();

        if disk.encrypt {
            if disk.partitions.is_empty() {
                return PhaseOps {
                    phase: InstallPhase::SetupEncryption,
                    description: "Setup disk encryption (no partitions)".into(),
                    operations: vec![],
                };
            }

            // Find the root partition (largest or no size_mb)
            let root_idx = disk
                .partitions
                .iter()
                .position(|p| p.mount_point == "/")
                .unwrap_or(disk.partitions.len() - 1);

            let part_dev = Self::partition_device(device, root_idx);

            ops.push(SystemOp::Command {
                binary: "cryptsetup".into(),
                args: vec![
                    "luksFormat".into(),
                    "--type".into(),
                    "luks2".into(),
                    "--cipher".into(),
                    "aes-xts-plain64".into(),
                    "--key-size".into(),
                    "512".into(),
                    "--hash".into(),
                    "sha512".into(),
                    "--iter-time".into(),
                    "5000".into(),
                    part_dev.clone(),
                ],
                description: "Format LUKS2 encrypted volume".into(),
                fatal: true,
            });

            ops.push(SystemOp::Command {
                binary: "cryptsetup".into(),
                args: vec!["open".into(), part_dev, "agnos-root".into()],
                description: "Open LUKS volume as agnos-root".into(),
                fatal: true,
            });
        }

        PhaseOps {
            phase: InstallPhase::SetupEncryption,
            description: "Setup disk encryption".into(),
            operations: ops,
        }
    }

    /// Kernel version string used in boot entries. Derived from the kernel
    /// recipe or overridden via `InstallConfig`. Centralised here so boot
    /// entries stay in sync with the installed kernel.
    fn kernel_version(&self) -> &str {
        // Future: read from config.kernel_version once the field exists.
        // For now use the version from the LFS kernel recipe.
        "6.6.72-agnos"
    }

    /// Detect whether the *running* system booted via UEFI by probing
    /// `/sys/firmware/efi`. This is a pure check — no side-effects.
    pub fn is_uefi_system() -> bool {
        std::path::Path::new("/sys/firmware/efi").exists()
    }

    /// Generate the operations needed for bootloader installation.
    ///
    /// Supports both UEFI and BIOS (MBR) for GRUB2, and generates the
    /// required entry config files for systemd-boot.
    pub fn plan_bootloader_ops(&self, target_root: &str) -> PhaseOps {
        let boot = &self.config.bootloader;
        let kver = self.kernel_version();
        let kernel_cmdline = Self::kernel_cmdline(&self.config);
        let mut ops = Vec::new();

        let uefi = Self::is_uefi_system();

        match boot.bootloader_type {
            BootloaderType::Grub2 => {
                if uefi {
                    ops.push(SystemOp::Command {
                        binary: "grub-install".into(),
                        args: vec![
                            "--target=x86_64-efi".into(),
                            format!("--efi-directory={}/boot/efi", target_root),
                            format!("--boot-directory={}/boot", target_root),
                            "--bootloader-id=AGNOS".into(),
                        ],
                        description: "Install GRUB EFI bootloader".into(),
                        fatal: true,
                    });
                } else {
                    // BIOS / MBR install — write to the disk MBR
                    ops.push(SystemOp::Command {
                        binary: "grub-install".into(),
                        args: vec![
                            "--target=i386-pc".into(),
                            format!("--boot-directory={}/boot", target_root),
                            self.config.disk.target_device.clone(),
                        ],
                        description: "Install GRUB BIOS bootloader".into(),
                        fatal: true,
                    });
                }

                // Generate grub.cfg (uses kver variable, not hardcoded)
                let grub_cfg = format!(
                    concat!(
                        "# AGNOS GRUB configuration\n",
                        "set default={}\n",
                        "set timeout={}\n",
                        "\n",
                        "menuentry \"AGNOS\" {{\n",
                        "    linux /vmlinuz-{} {}\n",
                        "    initrd /initramfs-{}.img\n",
                        "}}\n",
                        "\n",
                        "menuentry \"AGNOS (rescue)\" {{\n",
                        "    linux /vmlinuz-{} {} single\n",
                        "    initrd /initramfs-{}.img\n",
                        "}}\n",
                    ),
                    boot.default_entry, boot.timeout_secs,
                    kver, kernel_cmdline, kver,
                    kver, kernel_cmdline, kver,
                );

                ops.push(SystemOp::MakeDir {
                    path: format!("{}/boot/grub", target_root),
                    mode: 0o755,
                    parents: true,
                });

                ops.push(SystemOp::WriteFile {
                    path: format!("{}/boot/grub/grub.cfg", target_root),
                    content: grub_cfg,
                    mode: 0o644,
                    owner: Some("root:root".into()),
                });
            }
            BootloaderType::SystemdBoot => {
                ops.push(SystemOp::Command {
                    binary: "bootctl".into(),
                    args: vec![
                        format!("--esp-path={}/boot/efi", target_root),
                        "install".into(),
                    ],
                    description: "Install systemd-boot".into(),
                    fatal: true,
                });

                // Loader config
                ops.push(SystemOp::MakeDir {
                    path: format!("{}/boot/efi/loader", target_root),
                    mode: 0o755,
                    parents: true,
                });
                ops.push(SystemOp::WriteFile {
                    path: format!("{}/boot/efi/loader/loader.conf", target_root),
                    content: format!(
                        "default agnos.conf\ntimeout {}\neditor no\n",
                        boot.timeout_secs
                    ),
                    mode: 0o644,
                    owner: Some("root:root".into()),
                });

                // Boot entry
                ops.push(SystemOp::MakeDir {
                    path: format!("{}/boot/efi/loader/entries", target_root),
                    mode: 0o755,
                    parents: true,
                });
                ops.push(SystemOp::WriteFile {
                    path: format!("{}/boot/efi/loader/entries/agnos.conf", target_root),
                    content: format!(
                        "title   AGNOS\nlinux   /vmlinuz-{}\ninitrd  /initramfs-{}.img\noptions {}\n",
                        kver, kver, kernel_cmdline
                    ),
                    mode: 0o644,
                    owner: Some("root:root".into()),
                });
                ops.push(SystemOp::WriteFile {
                    path: format!("{}/boot/efi/loader/entries/agnos-rescue.conf", target_root),
                    content: format!(
                        "title   AGNOS (rescue)\nlinux   /vmlinuz-{}\ninitrd  /initramfs-{}.img\noptions {} single\n",
                        kver, kver, kernel_cmdline
                    ),
                    mode: 0o644,
                    owner: Some("root:root".into()),
                });
            }
        }

        PhaseOps {
            phase: InstallPhase::InstallBootloader,
            description: "Install bootloader".into(),
            operations: ops,
        }
    }

    /// Generate the operations needed for user creation.
    pub fn plan_user_ops(&self, target_root: &str) -> PhaseOps {
        let user = &self.config.user;
        let mut ops = Vec::new();

        // Create the user
        let mut useradd_args = vec![
            "--root".into(),
            target_root.to_string(),
            "-m".into(),
            "-s".into(),
            user.shell.clone(),
        ];
        if let Some(ref full_name) = user.full_name {
            useradd_args.push("-c".into());
            useradd_args.push(full_name.clone());
        }
        if !user.groups.is_empty() {
            useradd_args.push("-G".into());
            useradd_args.push(user.groups.join(","));
        }
        useradd_args.push(user.username.clone());

        ops.push(SystemOp::Command {
            binary: "useradd".into(),
            args: useradd_args,
            description: format!("Create user '{}'", user.username),
            fatal: true,
        });

        // SSH keys
        if !user.ssh_keys.is_empty() {
            let ssh_dir = format!("{}/home/{}/.ssh", target_root, user.username);
            ops.push(SystemOp::MakeDir {
                path: ssh_dir.clone(),
                mode: 0o700,
                parents: true,
            });

            let auth_keys = user.ssh_keys.join("\n") + "\n";
            ops.push(SystemOp::WriteFile {
                path: format!("{}/authorized_keys", ssh_dir),
                content: auth_keys,
                mode: 0o600,
                owner: Some(format!("{}:{}", user.username, user.username)),
            });
        }

        // Sudo access
        if user.enable_sudo {
            ops.push(SystemOp::Command {
                binary: "usermod".into(),
                args: vec![
                    "--root".into(),
                    target_root.to_string(),
                    "-aG".into(),
                    "wheel".into(),
                    user.username.clone(),
                ],
                description: format!("Add '{}' to wheel group", user.username),
                fatal: false,
            });
        }

        PhaseOps {
            phase: InstallPhase::CreateUser,
            description: format!("Create user {}", user.username),
            operations: ops,
        }
    }

    /// Generate the operations needed for network configuration.
    pub fn plan_network_ops(&self, target_root: &str) -> PhaseOps {
        let net = &self.config.network;
        let mut ops = Vec::new();

        // /etc/hostname
        ops.push(SystemOp::WriteFile {
            path: format!("{}/etc/hostname", target_root),
            content: generate_hostname_config(&net.hostname),
            mode: 0o644,
            owner: None,
        });

        // /etc/hosts
        let hosts = format!(
            "127.0.0.1   localhost\n::1         localhost\n127.0.1.1   {}\n",
            net.hostname
        );
        ops.push(SystemOp::WriteFile {
            path: format!("{}/etc/hosts", target_root),
            content: hosts,
            mode: 0o644,
            owner: None,
        });

        // DNS resolv.conf
        if !net.dns.is_empty() {
            let resolv = net
                .dns
                .iter()
                .map(|d| format!("nameserver {}", d))
                .collect::<Vec<_>>()
                .join("\n")
                + "\n";
            ops.push(SystemOp::WriteFile {
                path: format!("{}/etc/resolv.conf", target_root),
                content: resolv,
                mode: 0o644,
                owner: None,
            });
        }

        PhaseOps {
            phase: InstallPhase::ConfigureSystem,
            description: "Configure network".into(),
            operations: ops,
        }
    }

    /// Generate the operations for locale and timezone setup.
    pub fn plan_locale_ops(&self, target_root: &str) -> PhaseOps {
        let mut ops = Vec::new();

        // /etc/locale.conf
        ops.push(SystemOp::WriteFile {
            path: format!("{}/etc/locale.conf", target_root),
            content: format!("LANG={}\n", self.config.locale),
            mode: 0o644,
            owner: None,
        });

        // Timezone symlink
        ops.push(SystemOp::Symlink {
            target: format!("/usr/share/zoneinfo/{}", self.config.timezone),
            link: format!("{}/etc/localtime", target_root),
        });

        // /etc/machine-id
        ops.push(SystemOp::WriteFile {
            path: format!("{}/etc/machine-id", target_root),
            content: format!("{}\n", generate_machine_id()),
            mode: 0o444,
            owner: None,
        });

        // /etc/fstab
        ops.push(SystemOp::WriteFile {
            path: format!("{}/etc/fstab", target_root),
            content: generate_fstab(&self.config.disk.partitions, self.config.disk.encrypt),
            mode: 0o644,
            owner: None,
        });

        PhaseOps {
            phase: InstallPhase::ConfigureSystem,
            description: "Configure locale and timezone".into(),
            operations: ops,
        }
    }

    /// Generate the operations needed to mount partitions at the target root.
    pub fn plan_mount_ops(&self, target_root: &str) -> PhaseOps {
        let disk = &self.config.disk;
        let device = &disk.target_device;
        let mut ops = Vec::new();

        // Sort partitions: mount "/" first, then others by mount-point depth
        let mut sorted: Vec<(usize, &PartitionSpec)> =
            disk.partitions.iter().enumerate().collect();
        sorted.sort_by_key(|(_, p)| {
            if p.mount_point == "/" { 0 } else { p.mount_point.matches('/').count() }
        });

        for (i, part) in &sorted {
            if part.filesystem == Filesystem::Swap {
                // Activate swap rather than mounting
                let part_dev = Self::partition_device(device, *i);
                ops.push(SystemOp::Command {
                    binary: "swapon".into(),
                    args: vec![part_dev],
                    description: format!("Activate swap ({})", part.label),
                    fatal: false,
                });
                continue;
            }

            let mount_path = format!("{}{}", target_root, part.mount_point);
            ops.push(SystemOp::MakeDir {
                path: mount_path.clone(),
                mode: 0o755,
                parents: true,
            });

            let part_dev = if disk.encrypt && part.mount_point == "/" {
                "/dev/mapper/agnos-root".to_string()
            } else {
                Self::partition_device(device, *i)
            };

            ops.push(SystemOp::Mount {
                device: part_dev,
                mount_point: mount_path,
                fs_type: format!("{}", part.filesystem),
                options: vec!["defaults".into()],
            });
        }

        PhaseOps {
            phase: InstallPhase::MountFilesystems,
            description: "Mount target filesystems".into(),
            operations: ops,
        }
    }

    /// Generate the operations to deploy the AGNOS base system to the
    /// target root. Unpacks the base tarball and creates the required
    /// directory structure.
    pub fn plan_install_base_ops(&self, target_root: &str) -> PhaseOps {
        let mut ops = Vec::new();

        // Create required directory hierarchy
        for dir in &[
            "bin", "sbin", "lib", "lib64", "usr/bin", "usr/sbin", "usr/lib",
            "etc", "var/log", "var/lib/agnos/ark/installed", "tmp", "proc",
            "sys", "dev", "run", "home", "root", "boot",
        ] {
            ops.push(SystemOp::MakeDir {
                path: format!("{}/{}", target_root, dir),
                mode: if *dir == "tmp" { 0o1777 } else { 0o755 },
                parents: true,
            });
        }

        // Unpack base system tarball (built by takumi)
        ops.push(SystemOp::Command {
            binary: "tar".into(),
            args: vec![
                "-xf".into(),
                "/run/agnos/installer/base-system.tar.zst".into(),
                "--zstd".into(),
                "-C".into(),
                target_root.to_string(),
            ],
            description: "Extract base system tarball".into(),
            fatal: true,
        });

        // Alternatively, install base via ark if packages are available
        ops.push(SystemOp::Command {
            binary: "ark-install.sh".into(),
            args: vec![
                "--root".into(),
                target_root.to_string(),
                "--packages".into(),
                "/run/agnos/installer/packages/".into(),
            ],
            description: "Install base .ark packages (fallback)".into(),
            fatal: false, // non-fatal: either tarball OR ark method succeeds
        });

        PhaseOps {
            phase: InstallPhase::InstallBase,
            description: "Deploy AGNOS base system".into(),
            operations: ops,
        }
    }

    /// Generate the operations to install mode-specific packages into
    /// the target root using ark.
    pub fn plan_install_packages_ops(&self, target_root: &str) -> PhaseOps {
        let packages = Self::default_packages(&self.config.mode);
        let mut ops = Vec::new();

        // Install mode-specific packages via ark
        if !packages.mode_packages.is_empty() {
            ops.push(SystemOp::Command {
                binary: "ark".into(),
                args: {
                    let mut a = vec![
                        "install".into(),
                        "--root".into(),
                        target_root.to_string(),
                        "--no-confirm".into(),
                    ];
                    a.extend(packages.mode_packages.iter().cloned());
                    a
                },
                description: format!("Install {} mode packages", self.config.mode),
                fatal: true,
            });
        }

        // Custom packages if specified
        if !packages.extra_packages.is_empty() {
            ops.push(SystemOp::Command {
                binary: "ark".into(),
                args: {
                    let mut a = vec![
                        "install".into(),
                        "--root".into(),
                        target_root.to_string(),
                        "--no-confirm".into(),
                    ];
                    a.extend(packages.extra_packages.iter().cloned());
                    a
                },
                description: "Install extra user-selected packages".into(),
                fatal: false,
            });
        }

        PhaseOps {
            phase: InstallPhase::InstallPackages,
            description: format!("Install packages for {} mode", self.config.mode),
            operations: ops,
        }
    }

    /// Generate security hardening operations for the target system.
    pub fn plan_security_ops(&self, target_root: &str) -> PhaseOps {
        let sec = &self.config.security;
        let mut ops = Vec::new();

        // Enable firewall defaults
        if sec.firewall_default == FirewallDefault::Deny {
            let nft_rules = concat!(
                "#!/usr/sbin/nft -f\n",
                "flush ruleset\n",
                "table inet filter {\n",
                "    chain input {\n",
                "        type filter hook input priority 0; policy drop;\n",
                "        iif lo accept\n",
                "        ct state established,related accept\n",
                "        tcp dport 22 accept comment \"SSH\"\n",
                "        icmp type echo-request accept\n",
                "    }\n",
                "    chain forward {\n",
                "        type filter hook forward priority 0; policy drop;\n",
                "    }\n",
                "    chain output {\n",
                "        type filter hook output priority 0; policy accept;\n",
                "    }\n",
                "}\n",
            );
            ops.push(SystemOp::WriteFile {
                path: format!("{}/etc/nftables.conf", target_root),
                content: nft_rules.to_string(),
                mode: 0o600,
                owner: Some("root:root".into()),
            });
        }

        // IMA policy — tied to dm-verity/integrity enforcement
        if sec.enable_dmverity {
            ops.push(SystemOp::MakeDir {
                path: format!("{}/etc/ima", target_root),
                mode: 0o700,
                parents: true,
            });
            ops.push(SystemOp::WriteFile {
                path: format!("{}/etc/ima/policy.conf", target_root),
                content: "measure func=BPRM_CHECK\nmeasure func=FILE_MMAP mask=MAY_EXEC\n"
                    .to_string(),
                mode: 0o600,
                owner: Some("root:root".into()),
            });
        }

        // Sysctl hardening
        let sysctl_content = concat!(
            "# AGNOS security defaults\n",
            "kernel.kptr_restrict=2\n",
            "kernel.dmesg_restrict=1\n",
            "kernel.perf_event_paranoid=3\n",
            "net.ipv4.conf.all.rp_filter=1\n",
            "net.ipv4.conf.all.send_redirects=0\n",
            "net.ipv4.conf.all.accept_redirects=0\n",
            "net.ipv6.conf.all.accept_redirects=0\n",
        );
        ops.push(SystemOp::WriteFile {
            path: format!("{}/etc/sysctl.d/99-agnos-hardening.conf", target_root),
            content: sysctl_content.to_string(),
            mode: 0o644,
            owner: Some("root:root".into()),
        });

        PhaseOps {
            phase: InstallPhase::SetupSecurity,
            description: "Configure security hardening".into(),
            operations: ops,
        }
    }

    /// Generate first-boot setup operations (argonaut services, agent config).
    pub fn plan_first_boot_ops(&self, target_root: &str) -> PhaseOps {
        let mut ops = Vec::new();

        // Enable core argonaut services
        let services = match self.config.mode {
            InstallMode::Server | InstallMode::Desktop => vec![
                "daimon", "hoosh", "aegis", "nftables", "sshd", "networkmanager",
            ],
            InstallMode::Minimal => vec!["nftables", "sshd", "networkmanager"],
            InstallMode::Custom => vec!["nftables", "sshd", "networkmanager"],
        };

        for svc in services {
            ops.push(SystemOp::Command {
                binary: "chroot".into(),
                args: vec![
                    target_root.to_string(),
                    "argonaut".into(),
                    "enable".into(),
                    svc.into(),
                ],
                description: format!("Enable {} service", svc),
                fatal: false,
            });
        }

        // Desktop-specific: enable compositor
        if self.config.mode == InstallMode::Desktop {
            ops.push(SystemOp::Command {
                binary: "chroot".into(),
                args: vec![
                    target_root.to_string(),
                    "argonaut".into(),
                    "enable".into(),
                    "aethersafha".into(),
                ],
                description: "Enable desktop compositor".into(),
                fatal: false,
            });
        }

        // First-boot marker file — argonaut checks this to run post-install
        ops.push(SystemOp::WriteFile {
            path: format!("{}/etc/agnos/first-boot", target_root),
            content: "1\n".to_string(),
            mode: 0o644,
            owner: Some("root:root".into()),
        });

        PhaseOps {
            phase: InstallPhase::FirstBootSetup,
            description: "Configure first-boot services".into(),
            operations: ops,
        }
    }

    /// Generate cleanup operations (unmount, sync, remove temp files).
    pub fn plan_cleanup_ops(&self, target_root: &str) -> PhaseOps {
        let disk = &self.config.disk;
        let device = &disk.target_device;
        let mut ops = Vec::new();

        // Sync to flush writes
        ops.push(SystemOp::Command {
            binary: "sync".into(),
            args: vec![],
            description: "Flush filesystem buffers".into(),
            fatal: false,
        });

        // Unmount in reverse depth order (deepest first)
        let mut mounts: Vec<&PartitionSpec> = disk
            .partitions
            .iter()
            .filter(|p| p.filesystem != Filesystem::Swap)
            .collect();
        mounts.sort_by(|a, b| {
            b.mount_point
                .matches('/')
                .count()
                .cmp(&a.mount_point.matches('/').count())
        });

        for part in mounts {
            ops.push(SystemOp::Unmount {
                mount_point: format!("{}{}", target_root, part.mount_point),
            });
        }

        // Deactivate swap
        for (i, part) in disk.partitions.iter().enumerate() {
            if part.filesystem == Filesystem::Swap {
                ops.push(SystemOp::Command {
                    binary: "swapoff".into(),
                    args: vec![Self::partition_device(device, i)],
                    description: "Deactivate swap".into(),
                    fatal: false,
                });
            }
        }

        // Close LUKS if encrypted
        if disk.encrypt {
            ops.push(SystemOp::Command {
                binary: "cryptsetup".into(),
                args: vec!["close".into(), "agnos-root".into()],
                description: "Close LUKS volume".into(),
                fatal: false,
            });
        }

        PhaseOps {
            phase: InstallPhase::Cleanup,
            description: "Cleanup and unmount".into(),
            operations: ops,
        }
    }

    /// Helper: generate the partition device path for partition index `i`.
    fn partition_device(device: &str, i: usize) -> String {
        if device.contains("nvme") || device.contains("mmcblk") {
            format!("{}p{}", device, i + 1)
        } else {
            format!("{}{}", device, i + 1)
        }
    }

    /// Generate the complete ordered list of phase operations for the
    /// entire installation. This is the full execution plan.
    pub fn full_execution_plan(&self, target_root: &str) -> Vec<PhaseOps> {
        vec![
            self.plan_partition_ops(),
            self.plan_encryption_ops(),
            self.plan_format_ops(),
            self.plan_mount_ops(target_root),
            self.plan_install_base_ops(target_root),
            self.plan_install_packages_ops(target_root),
            self.plan_bootloader_ops(target_root),
            self.plan_user_ops(target_root),
            self.plan_network_ops(target_root),
            self.plan_locale_ops(target_root),
            self.plan_security_ops(target_root),
            self.plan_first_boot_ops(target_root),
            self.plan_cleanup_ops(target_root),
        ]
    }

    /// Count total system operations across all phases.
    pub fn total_ops_count(&self, target_root: &str) -> usize {
        self.full_execution_plan(target_root)
            .iter()
            .map(|p| p.operations.len())
            .sum()
    }
}

// ---------------------------------------------------------------------------
// ISO generation descriptor
// ---------------------------------------------------------------------------

/// Configuration for generating a bootable installation ISO.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IsoConfig {
    pub output_path: String,
    pub volume_label: String,
    /// Path to the root filesystem tree to pack into the ISO.
    pub root_tree: String,
    /// Whether to include UEFI boot support.
    pub uefi: bool,
    /// Whether to include legacy BIOS boot support.
    pub bios: bool,
    /// Compression for the squashfs image.
    pub compression: String,
}

impl Default for IsoConfig {
    fn default() -> Self {
        Self {
            output_path: "agnos-install.iso".into(),
            volume_label: "AGNOS".into(),
            root_tree: "/tmp/agnos-iso-tree".into(),
            uefi: true,
            bios: false,
            compression: "zstd".into(),
        }
    }
}

impl IsoConfig {
    /// Generate the xorriso command to create the ISO.
    pub fn build_command(&self) -> SystemOp {
        let mut args = vec![
            "-as".into(),
            "mkisofs".into(),
            "-o".into(),
            self.output_path.clone(),
            "-V".into(),
            self.volume_label.clone(),
            "-J".into(),
            "-R".into(),
        ];

        if self.uefi {
            args.extend_from_slice(&[
                "-e".into(),
                "boot/efi.img".into(),
                "-no-emul-boot".into(),
                "-isohybrid-gpt-basdat".into(),
            ]);
        }

        if self.bios {
            args.extend_from_slice(&[
                "-b".into(),
                "boot/grub/bios.img".into(),
                "-no-emul-boot".into(),
                "-boot-load-size".into(),
                "4".into(),
                "-boot-info-table".into(),
            ]);
        }

        args.push(self.root_tree.clone());

        SystemOp::Command {
            binary: "xorriso".into(),
            args,
            description: format!("Generate ISO: {}", self.output_path),
            fatal: true,
        }
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -- helpers --

    fn test_config() -> InstallConfig {
        InstallConfig {
            mode: InstallMode::Desktop,
            disk: AgnovaInstaller::default_disk_layout("/dev/sda", false),
            bootloader: BootloaderConfig::default(),
            network: NetworkConfig::default(),
            user: UserConfig {
                username: "testuser".to_string(),
                ..UserConfig::default()
            },
            security: SecurityConfig::default(),
            packages: AgnovaInstaller::default_packages(&InstallMode::Desktop),
            locale: "en_US.UTF-8".to_string(),
            timezone: "UTC".to_string(),
        }
    }

    // -- Display tests --

    #[test]
    fn install_mode_display() {
        assert_eq!(InstallMode::Server.to_string(), "Server");
        assert_eq!(InstallMode::Desktop.to_string(), "Desktop");
        assert_eq!(InstallMode::Minimal.to_string(), "Minimal");
        assert_eq!(InstallMode::Custom.to_string(), "Custom");
    }

    #[test]
    fn filesystem_display() {
        assert_eq!(Filesystem::Ext4.to_string(), "ext4");
        assert_eq!(Filesystem::Btrfs.to_string(), "btrfs");
        assert_eq!(Filesystem::Xfs.to_string(), "xfs");
        assert_eq!(Filesystem::Vfat.to_string(), "vfat");
        assert_eq!(Filesystem::Swap.to_string(), "swap");
    }

    #[test]
    fn bootloader_type_display() {
        assert_eq!(BootloaderType::SystemdBoot.to_string(), "systemd-boot");
        assert_eq!(BootloaderType::Grub2.to_string(), "GRUB 2");
    }

    #[test]
    fn partition_flag_variants() {
        let flags = vec![
            PartitionFlag::Boot,
            PartitionFlag::Esp,
            PartitionFlag::Lvm,
            PartitionFlag::Raid,
        ];
        assert_eq!(flags.len(), 4);
        assert_ne!(PartitionFlag::Boot, PartitionFlag::Esp);
    }

    // -- InstallPhase --

    #[test]
    fn install_phase_ordering() {
        assert!(InstallPhase::ValidateConfig.index() < InstallPhase::PartitionDisk.index());
        assert!(InstallPhase::InstallBase.index() < InstallPhase::InstallPackages.index());
        assert!(InstallPhase::Cleanup.index() < InstallPhase::Complete.index());
        assert_eq!(InstallPhase::Complete.index(), InstallPhase::ALL.len() - 1);
    }

    #[test]
    fn install_phase_display() {
        assert_eq!(
            InstallPhase::ValidateConfig.to_string(),
            "Validating configuration"
        );
        assert_eq!(InstallPhase::Complete.to_string(), "Installation complete");
    }

    #[test]
    fn install_phase_next() {
        assert_eq!(
            InstallPhase::ValidateConfig.next(),
            Some(InstallPhase::PartitionDisk)
        );
        assert_eq!(InstallPhase::Complete.next(), None);
    }

    // -- DiskLayout --

    #[test]
    fn default_disk_layout_structure() {
        let layout = AgnovaInstaller::default_disk_layout("/dev/sda", false);
        assert_eq!(layout.target_device, "/dev/sda");
        assert!(layout.use_gpt);
        assert!(!layout.encrypt);
        assert_eq!(layout.partitions.len(), 2);
        assert_eq!(layout.partitions[0].label, "ESP");
        assert_eq!(layout.partitions[0].size_mb, Some(512));
        assert_eq!(layout.partitions[1].mount_point, "/");
        assert_eq!(layout.partitions[1].size_mb, None);
    }

    #[test]
    fn default_disk_layout_with_encryption() {
        let layout = AgnovaInstaller::default_disk_layout("/dev/nvme0n1", true);
        assert!(layout.encrypt);
        assert_eq!(layout.target_device, "/dev/nvme0n1");
    }

    #[test]
    fn disk_layout_gpt_default() {
        let layout = DiskLayout::default();
        assert!(layout.use_gpt);
    }

    // -- PackageSelection --

    #[test]
    fn default_packages_server() {
        let pkgs = AgnovaInstaller::default_packages(&InstallMode::Server);
        assert!(!pkgs.base_packages.is_empty());
        assert!(!pkgs.mode_packages.is_empty());
        assert!(pkgs.base_packages.contains(&"linux-kernel".to_string()));
        assert!(pkgs.mode_packages.contains(&"daimon-server".to_string()));
    }

    #[test]
    fn default_packages_desktop_more_than_server() {
        let desktop = AgnovaInstaller::default_packages(&InstallMode::Desktop);
        let server = AgnovaInstaller::default_packages(&InstallMode::Server);
        assert!(desktop.mode_packages.len() > server.mode_packages.len());
        assert!(desktop.total_size_mb > server.total_size_mb);
    }

    #[test]
    fn default_packages_minimal_fewest() {
        let minimal = AgnovaInstaller::default_packages(&InstallMode::Minimal);
        assert!(minimal.mode_packages.is_empty());
        assert!(minimal.total_size_mb < 1000);
    }

    #[test]
    fn package_selection_total_count() {
        let pkgs = PackageSelection {
            base_packages: vec!["a".into(), "b".into()],
            mode_packages: vec!["c".into()],
            extra_packages: vec!["d".into(), "e".into(), "f".into()],
            total_size_mb: 100,
        };
        assert_eq!(pkgs.total_count(), 6);
    }

    // -- validate_config --

    #[test]
    fn validate_config_valid() {
        let installer = AgnovaInstaller::new(test_config());
        let result = installer.validate_config();
        assert!(result.is_ok());
    }

    #[test]
    fn validate_config_missing_device() {
        let mut config = test_config();
        config.disk.target_device = String::new();
        let installer = AgnovaInstaller::new(config);
        assert!(installer.validate_config().is_err());
    }

    #[test]
    fn validate_config_empty_username() {
        let mut config = test_config();
        config.user.username = String::new();
        let installer = AgnovaInstaller::new(config);
        assert!(installer.validate_config().is_err());
    }

    #[test]
    fn validate_config_no_partitions() {
        let mut config = test_config();
        config.disk.partitions.clear();
        let installer = AgnovaInstaller::new(config);
        assert!(installer.validate_config().is_err());
    }

    #[test]
    fn validate_config_warns_luks_mismatch() {
        let mut config = test_config();
        config.security.enable_luks = true;
        config.disk.encrypt = false;
        let installer = AgnovaInstaller::new(config);
        let warnings = installer.validate_config().unwrap();
        assert!(warnings.iter().any(|w| w.contains("enable_luks")));
    }

    // -- Phase advancement --

    #[test]
    fn phase_advancement_sequence() {
        let mut installer = AgnovaInstaller::new(test_config());
        assert_eq!(*installer.current_phase(), InstallPhase::ValidateConfig);
        assert!(installer.advance_phase());
        assert_eq!(*installer.current_phase(), InstallPhase::PartitionDisk);
    }

    #[test]
    fn phase_advancement_past_complete_returns_false() {
        let mut installer = AgnovaInstaller::new(test_config());
        // Walk all the way to Complete
        while installer.advance_phase() {}
        assert!(installer.is_complete());
        // Trying again returns false
        assert!(!installer.advance_phase());
    }

    #[test]
    fn fail_phase_records_error() {
        let mut installer = AgnovaInstaller::new(test_config());
        installer.fail_phase("disk I/O error".to_string());
        assert_eq!(installer.errors.len(), 1);
        assert_eq!(installer.errors[0].phase, InstallPhase::ValidateConfig);
        assert_eq!(installer.errors[0].message, "disk I/O error");
    }

    // -- result --

    #[test]
    fn result_after_completion() {
        let mut installer = AgnovaInstaller::new(test_config());
        while installer.advance_phase() {}
        let result = installer.result();
        assert!(result.success);
        assert!(result.errors.is_empty());
        assert!(!result.phases_completed.is_empty());
    }

    #[test]
    fn result_with_errors() {
        let mut installer = AgnovaInstaller::new(test_config());
        installer.fail_phase("something broke".to_string());
        let result = installer.result();
        assert!(!result.success);
        assert_eq!(result.errors.len(), 1);
    }

    // -- First-boot helpers --

    #[test]
    fn generate_machine_id_format() {
        let id = generate_machine_id();
        // machine-id is 32 hex chars (UUID without dashes)
        assert_eq!(id.len(), 32);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn generate_hostname_config_content() {
        let content = generate_hostname_config("myhost");
        assert_eq!(content, "myhost\n");
    }

    #[test]
    fn generate_fstab_basic() {
        let partitions = vec![
            PartitionSpec {
                label: "ESP".to_string(),
                mount_point: "/boot/efi".to_string(),
                filesystem: Filesystem::Vfat,
                size_mb: Some(512),
                flags: vec![PartitionFlag::Esp],
            },
            PartitionSpec {
                label: "agnos-root".to_string(),
                mount_point: "/".to_string(),
                filesystem: Filesystem::Ext4,
                size_mb: None,
                flags: Vec::new(),
            },
        ];
        let fstab = generate_fstab(&partitions, false);
        assert!(fstab.contains("LABEL=ESP"));
        assert!(fstab.contains("LABEL=agnos-root"));
        assert!(fstab.contains("vfat"));
        assert!(fstab.contains("ext4"));
    }

    #[test]
    fn generate_fstab_with_encryption() {
        let partitions = vec![PartitionSpec {
            label: "agnos-root".to_string(),
            mount_point: "/".to_string(),
            filesystem: Filesystem::Ext4,
            size_mb: None,
            flags: Vec::new(),
        }];
        let fstab = generate_fstab(&partitions, true);
        assert!(fstab.contains("/dev/mapper/agnos-root"));
    }

    // -- Kernel params --

    #[test]
    fn default_kernel_params_full_security() {
        let sec = SecurityConfig::default();
        let params = default_kernel_params(&sec);
        assert!(params.contains(&"rd.luks=1".to_string()));
        assert!(params.contains(&"lockdown=integrity".to_string()));
        assert!(params.contains(&"tpm_tis.interrupts=0".to_string()));
        assert!(params.contains(&"dm_verity.verify=1".to_string()));
        assert!(params.iter().any(|p| p.starts_with("lsm=")));
    }

    #[test]
    fn default_kernel_params_no_security() {
        let sec = SecurityConfig {
            enable_luks: false,
            enable_secureboot: false,
            enable_tpm: false,
            enable_dmverity: false,
            trust_enforcement: TrustEnforcementMode::AuditOnly,
            firewall_default: FirewallDefault::Deny,
        };
        let params = default_kernel_params(&sec);
        assert!(!params.contains(&"rd.luks=1".to_string()));
        assert!(!params.contains(&"lockdown=integrity".to_string()));
        // Base params still present
        assert!(params.contains(&"quiet".to_string()));
    }

    #[test]
    fn kernel_cmdline_generation() {
        let config = test_config();
        let cmdline = AgnovaInstaller::kernel_cmdline(&config);
        assert!(cmdline.contains("quiet"));
        assert!(cmdline.contains("root=LABEL=agnos-root"));
    }

    #[test]
    fn kernel_cmdline_encrypted() {
        let mut config = test_config();
        config.disk.encrypt = true;
        let cmdline = AgnovaInstaller::kernel_cmdline(&config);
        assert!(cmdline.contains("root=/dev/mapper/agnos-root"));
    }

    // -- estimate_install_time --

    #[test]
    fn estimate_install_time_varies_by_mode() {
        let minimal = AgnovaInstaller::estimate_install_time(&InstallMode::Minimal);
        let desktop = AgnovaInstaller::estimate_install_time(&InstallMode::Desktop);
        assert!(desktop > minimal);
    }

    // -- InstallProgress --

    #[test]
    fn install_progress_initial_state() {
        let p = InstallProgress::new();
        assert_eq!(p.current_phase, InstallPhase::ValidateConfig);
        assert_eq!(p.phase_progress, 0.0);
        assert_eq!(p.overall_progress, 0.0);
        assert!(p.estimated_remaining_secs.is_none());
    }

    // -- UserConfig --

    #[test]
    fn user_config_defaults() {
        let u = UserConfig::default();
        assert_eq!(u.shell, "/usr/bin/agnoshi");
        assert!(u.enable_sudo);
        assert!(u.groups.contains(&"wheel".to_string()));
    }

    // -- NetworkConfig --

    #[test]
    fn network_config_dhcp() {
        let n = NetworkConfig::default();
        assert!(n.use_dhcp);
        assert!(n.static_ip.is_none());
        assert!(!n.dns.is_empty());
    }

    #[test]
    fn network_config_static_ip() {
        let n = NetworkConfig {
            hostname: "server1".to_string(),
            use_dhcp: false,
            static_ip: Some("192.168.1.100/24".to_string()),
            gateway: Some("192.168.1.1".to_string()),
            dns: vec!["192.168.1.1".to_string()],
        };
        assert!(!n.use_dhcp);
        assert!(n.static_ip.is_some());
        assert!(n.gateway.is_some());
    }

    // -- SecurityConfig --

    #[test]
    fn security_config_full_lockdown() {
        let s = SecurityConfig::default();
        assert!(s.enable_luks);
        assert!(s.enable_secureboot);
        assert!(s.enable_tpm);
        assert!(s.enable_dmverity);
        assert_eq!(s.trust_enforcement, TrustEnforcementMode::Strict);
        assert_eq!(s.firewall_default, FirewallDefault::Deny);
    }

    // -- InstallError --

    #[test]
    fn install_error_recoverable_flag() {
        let e = InstallError {
            phase: InstallPhase::ConfigureSystem,
            message: "locale not found".to_string(),
            recoverable: true,
        };
        assert!(e.recoverable);

        let e2 = InstallError {
            phase: InstallPhase::PartitionDisk,
            message: "disk not found".to_string(),
            recoverable: false,
        };
        assert!(!e2.recoverable);
    }

    // -- Log --

    #[test]
    fn log_messages_recorded() {
        let mut installer = AgnovaInstaller::new(test_config());
        installer.log_message("step 1 done".to_string());
        installer.log_message("step 2 done".to_string());
        assert_eq!(installer.get_log().len(), 2);
        assert_eq!(installer.get_log()[0], "step 1 done");
    }

    // -- Phase count --

    #[test]
    fn phase_count_matches_all() {
        assert_eq!(AgnovaInstaller::phase_count(), 14);
        assert_eq!(AgnovaInstaller::phase_count(), InstallPhase::ALL.len());
    }

    // -- CRITICAL 1: Kernel param validation --

    #[test]
    fn validate_rejects_dangerous_kernel_param_init() {
        let mut config = test_config();
        config.bootloader.kernel_params = vec!["init=/bin/sh".to_string()];
        let installer = AgnovaInstaller::new(config);
        let err = installer.validate_config().unwrap_err();
        assert!(err.to_string().contains("dangerous kernel parameter"));
    }

    #[test]
    fn validate_accepts_safe_kernel_params() {
        let mut config = test_config();
        config.bootloader.kernel_params = vec!["quiet".to_string(), "loglevel=3".to_string()];
        let installer = AgnovaInstaller::new(config);
        assert!(installer.validate_config().is_ok());
    }

    // -- HIGH 2: Device path validation --

    #[test]
    fn validate_rejects_device_path_with_dotdot() {
        let mut config = test_config();
        config.disk.target_device = "/dev/../etc/passwd".to_string();
        let installer = AgnovaInstaller::new(config);
        assert!(installer.validate_config().is_err());
    }

    #[test]
    fn validate_rejects_device_path_with_semicolon() {
        let mut config = test_config();
        config.disk.target_device = "/dev/sda;rm -rf /".to_string();
        let installer = AgnovaInstaller::new(config);
        assert!(installer.validate_config().is_err());
    }

    #[test]
    fn validate_accepts_valid_device_path() {
        let config = test_config(); // uses /dev/sda
        let installer = AgnovaInstaller::new(config);
        assert!(installer.validate_config().is_ok());
    }

    // -- MEDIUM 1: Hostname validation --

    #[test]
    fn validate_rejects_hostname_with_spaces() {
        let mut config = test_config();
        config.network.hostname = "my host".to_string();
        let installer = AgnovaInstaller::new(config);
        assert!(installer.validate_config().is_err());
    }

    #[test]
    fn validate_accepts_valid_hostname() {
        let mut config = test_config();
        config.network.hostname = "my-server-01".to_string();
        let installer = AgnovaInstaller::new(config);
        assert!(installer.validate_config().is_ok());
    }

    // -- MEDIUM 2: Username validation --

    #[test]
    fn validate_rejects_username_root() {
        let mut config = test_config();
        config.user.username = "root".to_string();
        let installer = AgnovaInstaller::new(config);
        let err = installer.validate_config().unwrap_err();
        assert!(err.to_string().contains("root"));
    }

    #[test]
    fn validate_rejects_username_with_special_chars() {
        let mut config = test_config();
        config.user.username = "user!name".to_string();
        let installer = AgnovaInstaller::new(config);
        assert!(installer.validate_config().is_err());
    }

    // -- Phase ordering: SetupEncryption before FormatFilesystems --

    #[test]
    fn phase_ordering_encryption_before_format() {
        assert!(
            InstallPhase::SetupEncryption.index() < InstallPhase::FormatFilesystems.index(),
            "SetupEncryption must come before FormatFilesystems"
        );
    }

    // -- MEDIUM 5: advance_phase blocked after non-recoverable failure --

    #[test]
    fn advance_phase_blocked_after_non_recoverable_failure() {
        let mut installer = AgnovaInstaller::new(test_config());
        // Advance to PartitionDisk
        installer.advance_phase();
        assert_eq!(*installer.current_phase(), InstallPhase::PartitionDisk);
        // Fail with non-recoverable error
        installer.fail_phase("disk not found".to_string());
        assert!(!installer.errors.last().unwrap().recoverable);
        // Should not be able to advance
        assert!(!installer.advance_phase());
        assert_eq!(*installer.current_phase(), InstallPhase::PartitionDisk);
    }

    // -- MEDIUM 6: DHCP false without static IP --

    #[test]
    fn validate_rejects_no_dhcp_without_static_ip() {
        let mut config = test_config();
        config.network.use_dhcp = false;
        config.network.static_ip = None;
        let installer = AgnovaInstaller::new(config);
        let err = installer.validate_config().unwrap_err();
        assert!(err.to_string().contains("static_ip"));
    }

    // -- MEDIUM 3: Enum Display tests --

    #[test]
    fn trust_enforcement_mode_display() {
        assert_eq!(TrustEnforcementMode::Strict.to_string(), "strict");
        assert_eq!(TrustEnforcementMode::Permissive.to_string(), "permissive");
        assert_eq!(TrustEnforcementMode::AuditOnly.to_string(), "audit-only");
    }

    #[test]
    fn firewall_default_display() {
        assert_eq!(FirewallDefault::Deny.to_string(), "deny");
        assert_eq!(FirewallDefault::Allow.to_string(), "allow");
    }

    // -- Audit Round 2: Input validation tests --

    #[test]
    fn validate_rejects_partition_label_with_spaces() {
        let mut config = test_config();
        config.disk.partitions[0].label = "my label".to_string();
        let installer = AgnovaInstaller::new(config);
        let err = installer.validate_config().unwrap_err();
        assert!(err.to_string().contains("invalid characters"));
    }

    #[test]
    fn validate_rejects_partition_label_with_shell_chars() {
        let mut config = test_config();
        config.disk.partitions[0].label = "root;rm".to_string();
        let installer = AgnovaInstaller::new(config);
        assert!(installer.validate_config().is_err());
    }

    #[test]
    fn validate_rejects_empty_partition_label() {
        let mut config = test_config();
        config.disk.partitions[0].label = String::new();
        let installer = AgnovaInstaller::new(config);
        let err = installer.validate_config().unwrap_err();
        assert!(err.to_string().contains("empty label"));
    }

    #[test]
    fn validate_rejects_fill_remaining_not_last() {
        let mut config = test_config();
        // First partition has no size (fill remaining), but there's a second partition
        config.disk.partitions[0].size_mb = None;
        config.disk.partitions.push(PartitionSpec {
            label: "extra".to_string(),
            mount_point: "/data".to_string(),
            filesystem: Filesystem::Ext4,
            size_mb: Some(1024),
            flags: vec![],
        });
        let installer = AgnovaInstaller::new(config);
        let err = installer.validate_config().unwrap_err();
        assert!(err.to_string().contains("not the last partition"));
    }

    #[test]
    fn validate_rejects_full_name_with_colon() {
        let mut config = test_config();
        config.user.full_name = Some("user:name".to_string());
        let installer = AgnovaInstaller::new(config);
        let err = installer.validate_config().unwrap_err();
        assert!(err.to_string().contains("full_name"));
    }

    #[test]
    fn validate_rejects_group_with_special_chars() {
        let mut config = test_config();
        config.user.groups = vec!["valid".to_string(), "bad group!".to_string()];
        let installer = AgnovaInstaller::new(config);
        assert!(installer.validate_config().is_err());
    }

    #[test]
    fn validate_accepts_valid_partition_labels() {
        let config = test_config();
        let installer = AgnovaInstaller::new(config);
        // Default test_config should pass
        assert!(installer.validate_config().is_ok());
    }

    #[test]
    fn encryption_ops_empty_partitions_no_panic() {
        let mut config = test_config();
        config.disk.encrypt = true;
        config.disk.partitions.clear();
        let installer = AgnovaInstaller::new(config);
        let ops = installer.plan_encryption_ops();
        assert!(ops.operations.is_empty());
    }

    // -----------------------------------------------------------------------
    // Phase 12C: Partition ops tests
    // -----------------------------------------------------------------------

    #[test]
    fn partition_ops_creates_gpt() {
        let config = test_config();
        let installer = AgnovaInstaller::new(config);
        let ops = installer.plan_partition_ops();
        assert_eq!(ops.phase, InstallPhase::PartitionDisk);
        // First op should create GPT
        let first = &ops.operations[0];
        if let SystemOp::Command { args, .. } = first {
            assert!(args.contains(&"gpt".to_string()));
        } else {
            panic!("expected Command op");
        }
    }

    #[test]
    fn partition_ops_creates_partitions() {
        let config = test_config();
        let installer = AgnovaInstaller::new(config);
        let ops = installer.plan_partition_ops();
        // Should have: mklabel + 2 mkpart + flag ops
        assert!(ops.operations.len() >= 3);
    }

    #[test]
    fn partition_ops_sets_esp_flag() {
        let config = test_config();
        let installer = AgnovaInstaller::new(config);
        let ops = installer.plan_partition_ops();
        let has_esp = ops.operations.iter().any(|op| {
            if let SystemOp::Command { args, .. } = op {
                args.contains(&"esp".to_string())
            } else {
                false
            }
        });
        assert!(has_esp);
    }

    // -----------------------------------------------------------------------
    // Format ops tests
    // -----------------------------------------------------------------------

    #[test]
    fn format_ops_creates_filesystems() {
        let config = test_config();
        let installer = AgnovaInstaller::new(config);
        let ops = installer.plan_format_ops();
        assert_eq!(ops.phase, InstallPhase::FormatFilesystems);
        // Should have one mkfs per partition
        assert_eq!(ops.operations.len(), 2);
    }

    #[test]
    fn format_ops_uses_correct_mkfs() {
        let config = test_config();
        let installer = AgnovaInstaller::new(config);
        let ops = installer.plan_format_ops();
        // First partition is vfat (ESP)
        if let SystemOp::Command { binary, .. } = &ops.operations[0] {
            assert_eq!(binary, "mkfs.vfat");
        }
        // Second partition is ext4 (root)
        if let SystemOp::Command { binary, .. } = &ops.operations[1] {
            assert_eq!(binary, "mkfs.ext4");
        }
    }

    // -----------------------------------------------------------------------
    // Encryption ops tests
    // -----------------------------------------------------------------------

    #[test]
    fn encryption_ops_empty_when_disabled() {
        let config = test_config();
        let installer = AgnovaInstaller::new(config);
        let ops = installer.plan_encryption_ops();
        assert!(ops.operations.is_empty());
    }

    #[test]
    fn encryption_ops_luks_when_enabled() {
        let mut config = test_config();
        config.disk.encrypt = true;
        let installer = AgnovaInstaller::new(config);
        let ops = installer.plan_encryption_ops();
        assert_eq!(ops.operations.len(), 2);
        // Should have luksFormat and open
        if let SystemOp::Command { binary, args, .. } = &ops.operations[0] {
            assert_eq!(binary, "cryptsetup");
            assert!(args.contains(&"luksFormat".to_string()));
        }
        if let SystemOp::Command { binary, args, .. } = &ops.operations[1] {
            assert_eq!(binary, "cryptsetup");
            assert!(args.contains(&"open".to_string()));
        }
    }

    // -----------------------------------------------------------------------
    // Bootloader ops tests
    // -----------------------------------------------------------------------

    #[test]
    fn bootloader_ops_grub() {
        let mut config = test_config();
        config.bootloader.bootloader_type = BootloaderType::Grub2;
        let installer = AgnovaInstaller::new(config);
        let ops = installer.plan_bootloader_ops("/mnt");
        assert_eq!(ops.phase, InstallPhase::InstallBootloader);
        // grub-install + mkdir + grub.cfg write
        assert_eq!(ops.operations.len(), 3);
        if let SystemOp::Command { binary, .. } = &ops.operations[0] {
            assert_eq!(binary, "grub-install");
        }
        if let SystemOp::WriteFile { path, content, .. } = &ops.operations[2] {
            assert!(path.contains("grub.cfg"));
            assert!(content.contains("AGNOS"));
            assert!(content.contains("rescue"));
        }
    }

    #[test]
    fn bootloader_ops_systemd_boot() {
        let mut config = test_config();
        config.bootloader.bootloader_type = BootloaderType::SystemdBoot;
        let installer = AgnovaInstaller::new(config);
        let ops = installer.plan_bootloader_ops("/mnt");
        if let SystemOp::Command { binary, .. } = &ops.operations[0] {
            assert_eq!(binary, "bootctl");
        }
    }

    // -----------------------------------------------------------------------
    // User creation ops tests
    // -----------------------------------------------------------------------

    #[test]
    fn user_ops_creates_user() {
        let config = test_config();
        let installer = AgnovaInstaller::new(config);
        let ops = installer.plan_user_ops("/mnt");
        assert_eq!(ops.phase, InstallPhase::CreateUser);
        if let SystemOp::Command { binary, args, .. } = &ops.operations[0] {
            assert_eq!(binary, "useradd");
            assert!(args.contains(&"testuser".to_string()));
        }
    }

    #[test]
    fn user_ops_installs_ssh_keys() {
        let mut config = test_config();
        config.user.ssh_keys = vec!["ssh-ed25519 AAAA... user@host".into()];
        let installer = AgnovaInstaller::new(config);
        let ops = installer.plan_user_ops("/mnt");
        // Should have: useradd + mkdir .ssh + write authorized_keys
        assert!(ops.operations.len() >= 3);
        let has_auth_keys = ops.operations.iter().any(|op| {
            if let SystemOp::WriteFile { path, .. } = op {
                path.contains("authorized_keys")
            } else {
                false
            }
        });
        assert!(has_auth_keys);
    }

    #[test]
    fn user_ops_adds_sudo() {
        let mut config = test_config();
        config.user.enable_sudo = true;
        let installer = AgnovaInstaller::new(config);
        let ops = installer.plan_user_ops("/mnt");
        let has_wheel = ops.operations.iter().any(|op| {
            if let SystemOp::Command { args, .. } = op {
                args.contains(&"wheel".to_string())
            } else {
                false
            }
        });
        assert!(has_wheel);
    }

    // -----------------------------------------------------------------------
    // Network ops tests
    // -----------------------------------------------------------------------

    #[test]
    fn network_ops_creates_hostname() {
        let config = test_config();
        let installer = AgnovaInstaller::new(config);
        let ops = installer.plan_network_ops("/mnt");
        let has_hostname = ops.operations.iter().any(|op| {
            if let SystemOp::WriteFile { path, content, .. } = op {
                path.contains("hostname") && content.contains("agnos")
            } else {
                false
            }
        });
        assert!(has_hostname);
    }

    #[test]
    fn network_ops_creates_hosts() {
        let config = test_config();
        let installer = AgnovaInstaller::new(config);
        let ops = installer.plan_network_ops("/mnt");
        let has_hosts = ops.operations.iter().any(|op| {
            if let SystemOp::WriteFile { path, content, .. } = op {
                path.contains("/etc/hosts") && content.contains("localhost")
            } else {
                false
            }
        });
        assert!(has_hosts);
    }

    #[test]
    fn network_ops_creates_resolv_conf() {
        let mut config = test_config();
        config.network.dns = vec!["8.8.8.8".into(), "1.1.1.1".into()];
        let installer = AgnovaInstaller::new(config);
        let ops = installer.plan_network_ops("/mnt");
        let has_resolv = ops.operations.iter().any(|op| {
            if let SystemOp::WriteFile { path, content, .. } = op {
                path.contains("resolv.conf") && content.contains("8.8.8.8")
            } else {
                false
            }
        });
        assert!(has_resolv);
    }

    // -----------------------------------------------------------------------
    // Locale ops tests
    // -----------------------------------------------------------------------

    #[test]
    fn locale_ops_creates_locale_conf() {
        let config = test_config();
        let installer = AgnovaInstaller::new(config);
        let ops = installer.plan_locale_ops("/mnt");
        let has_locale = ops.operations.iter().any(|op| {
            if let SystemOp::WriteFile { path, content, .. } = op {
                path.contains("locale.conf") && content.contains("en_US.UTF-8")
            } else {
                false
            }
        });
        assert!(has_locale);
    }

    #[test]
    fn locale_ops_creates_timezone_symlink() {
        let config = test_config();
        let installer = AgnovaInstaller::new(config);
        let ops = installer.plan_locale_ops("/mnt");
        let has_tz = ops.operations.iter().any(|op| {
            if let SystemOp::Symlink { target, link } = op {
                target.contains("zoneinfo") && link.contains("localtime")
            } else {
                false
            }
        });
        assert!(has_tz);
    }

    #[test]
    fn locale_ops_creates_fstab() {
        let config = test_config();
        let installer = AgnovaInstaller::new(config);
        let ops = installer.plan_locale_ops("/mnt");
        let has_fstab = ops.operations.iter().any(|op| {
            if let SystemOp::WriteFile { path, .. } = op {
                path.contains("fstab")
            } else {
                false
            }
        });
        assert!(has_fstab);
    }

    #[test]
    fn locale_ops_creates_machine_id() {
        let config = test_config();
        let installer = AgnovaInstaller::new(config);
        let ops = installer.plan_locale_ops("/mnt");
        let has_machine_id = ops.operations.iter().any(|op| {
            if let SystemOp::WriteFile { path, .. } = op {
                path.contains("machine-id")
            } else {
                false
            }
        });
        assert!(has_machine_id);
    }

    // -----------------------------------------------------------------------
    // Full execution plan tests
    // -----------------------------------------------------------------------

    #[test]
    fn full_execution_plan_has_all_phases() {
        let config = test_config();
        let installer = AgnovaInstaller::new(config);
        let plan = installer.full_execution_plan("/mnt");
        assert_eq!(plan.len(), 13);
    }

    #[test]
    fn total_ops_count_nonzero() {
        let config = test_config();
        let installer = AgnovaInstaller::new(config);
        let count = installer.total_ops_count("/mnt");
        assert!(count > 10, "expected >10 ops, got {}", count);
    }

    // -----------------------------------------------------------------------
    // New phase handler tests
    // -----------------------------------------------------------------------

    #[test]
    fn mount_ops_mounts_root_first() {
        let config = test_config();
        let installer = AgnovaInstaller::new(config);
        let ops = installer.plan_mount_ops("/mnt");
        assert_eq!(ops.phase, InstallPhase::MountFilesystems);
        assert!(!ops.operations.is_empty());
        // First real mount should be root
        let first_mount = ops.operations.iter().find(|op| matches!(op, SystemOp::Mount { .. }));
        if let Some(SystemOp::Mount { mount_point, .. }) = first_mount {
            assert_eq!(mount_point, "/mnt/");
        } else {
            panic!("expected a mount op for root");
        }
    }

    #[test]
    fn install_base_ops_creates_dirs_and_extracts() {
        let config = test_config();
        let installer = AgnovaInstaller::new(config);
        let ops = installer.plan_install_base_ops("/mnt");
        assert_eq!(ops.phase, InstallPhase::InstallBase);
        // Should have directory creation + tar extraction + ark fallback
        assert!(ops.operations.len() >= 3);
        let has_tar = ops.operations.iter().any(|op| {
            matches!(op, SystemOp::Command { binary, .. } if binary == "tar")
        });
        assert!(has_tar, "expected tar extraction");
    }

    #[test]
    fn install_packages_ops_uses_ark() {
        let config = test_config();
        let installer = AgnovaInstaller::new(config);
        let ops = installer.plan_install_packages_ops("/mnt");
        assert_eq!(ops.phase, InstallPhase::InstallPackages);
        let has_ark = ops.operations.iter().any(|op| {
            matches!(op, SystemOp::Command { binary, .. } if binary == "ark")
        });
        assert!(has_ark, "expected ark install command");
    }

    #[test]
    fn security_ops_writes_nft_and_sysctl() {
        let config = test_config();
        let installer = AgnovaInstaller::new(config);
        let ops = installer.plan_security_ops("/mnt");
        assert_eq!(ops.phase, InstallPhase::SetupSecurity);
        let has_nft = ops.operations.iter().any(|op| {
            matches!(op, SystemOp::WriteFile { path, .. } if path.contains("nftables"))
        });
        let has_sysctl = ops.operations.iter().any(|op| {
            matches!(op, SystemOp::WriteFile { path, .. } if path.contains("sysctl"))
        });
        assert!(has_nft, "expected nftables config");
        assert!(has_sysctl, "expected sysctl hardening");
    }

    #[test]
    fn first_boot_ops_enables_services() {
        let mut config = test_config();
        config.mode = InstallMode::Desktop;
        let installer = AgnovaInstaller::new(config);
        let ops = installer.plan_first_boot_ops("/mnt");
        assert_eq!(ops.phase, InstallPhase::FirstBootSetup);
        let has_chroot = ops.operations.iter().any(|op| {
            matches!(op, SystemOp::Command { binary, .. } if binary == "chroot")
        });
        assert!(has_chroot, "expected chroot service enable");
        // Desktop mode should enable compositor
        let has_compositor = ops.operations.iter().any(|op| {
            if let SystemOp::Command { args, .. } = op {
                args.iter().any(|a| a == "aethersafha")
            } else {
                false
            }
        });
        assert!(has_compositor, "expected compositor enable for Desktop mode");
    }

    #[test]
    fn cleanup_ops_unmounts_and_syncs() {
        let config = test_config();
        let installer = AgnovaInstaller::new(config);
        let ops = installer.plan_cleanup_ops("/mnt");
        assert_eq!(ops.phase, InstallPhase::Cleanup);
        let has_sync = ops.operations.iter().any(|op| {
            matches!(op, SystemOp::Command { binary, .. } if binary == "sync")
        });
        let has_unmount = ops.operations.iter().any(|op| matches!(op, SystemOp::Unmount { .. }));
        assert!(has_sync, "expected sync");
        assert!(has_unmount, "expected unmount");
    }

    #[test]
    fn partition_device_helper_sda() {
        assert_eq!(AgnovaInstaller::partition_device("/dev/sda", 0), "/dev/sda1");
        assert_eq!(AgnovaInstaller::partition_device("/dev/sda", 2), "/dev/sda3");
    }

    #[test]
    fn partition_device_helper_nvme() {
        assert_eq!(AgnovaInstaller::partition_device("/dev/nvme0n1", 0), "/dev/nvme0n1p1");
        assert_eq!(AgnovaInstaller::partition_device("/dev/mmcblk0", 1), "/dev/mmcblk0p2");
    }

    #[test]
    fn bootloader_grub_bios_mode() {
        // Simulate non-UEFI: is_uefi_system() checks /sys/firmware/efi
        // which won't exist in test environment, so grub should use i386-pc
        let mut config = test_config();
        config.bootloader.bootloader_type = BootloaderType::Grub2;
        let installer = AgnovaInstaller::new(config);
        let ops = installer.plan_bootloader_ops("/mnt");
        if let SystemOp::Command { args, .. } = &ops.operations[0] {
            // In CI/test env without /sys/firmware/efi, should be BIOS
            let target = args.iter().find(|a| a.starts_with("--target="));
            assert!(target.is_some());
        }
    }

    #[test]
    fn bootloader_systemd_boot_has_entry_files() {
        let mut config = test_config();
        config.bootloader.bootloader_type = BootloaderType::SystemdBoot;
        let installer = AgnovaInstaller::new(config);
        let ops = installer.plan_bootloader_ops("/mnt");
        let has_loader_conf = ops.operations.iter().any(|op| {
            matches!(op, SystemOp::WriteFile { path, .. } if path.contains("loader.conf"))
        });
        let has_entry = ops.operations.iter().any(|op| {
            matches!(op, SystemOp::WriteFile { path, .. } if path.contains("agnos.conf"))
        });
        let has_rescue = ops.operations.iter().any(|op| {
            matches!(op, SystemOp::WriteFile { path, .. } if path.contains("agnos-rescue.conf"))
        });
        assert!(has_loader_conf, "expected loader.conf");
        assert!(has_entry, "expected boot entry agnos.conf");
        assert!(has_rescue, "expected rescue entry agnos-rescue.conf");
    }

    #[test]
    fn grub_cfg_uses_kernel_version() {
        let mut config = test_config();
        config.bootloader.bootloader_type = BootloaderType::Grub2;
        let installer = AgnovaInstaller::new(config);
        let ops = installer.plan_bootloader_ops("/mnt");
        let grub_cfg = ops.operations.iter().find_map(|op| {
            if let SystemOp::WriteFile { path, content, .. } = op {
                if path.contains("grub.cfg") { Some(content.as_str()) } else { None }
            } else {
                None
            }
        });
        let cfg = grub_cfg.expect("grub.cfg should exist");
        assert!(cfg.contains("vmlinuz-"), "should reference kernel by version");
        assert!(cfg.contains("initramfs-"), "should reference initramfs by version");
        // Should NOT have bare "6.6.72" hardcoded without the installer method
        assert!(cfg.contains(installer.kernel_version()));
    }

    // -----------------------------------------------------------------------
    // ISO config tests
    // -----------------------------------------------------------------------

    #[test]
    fn iso_config_defaults() {
        let iso = IsoConfig::default();
        assert!(iso.uefi);
        assert!(!iso.bios);
        assert_eq!(iso.compression, "zstd");
        assert_eq!(iso.volume_label, "AGNOS");
    }

    #[test]
    fn iso_build_command_uefi() {
        let iso = IsoConfig::default();
        let op = iso.build_command();
        if let SystemOp::Command { binary, args, .. } = op {
            assert_eq!(binary, "xorriso");
            assert!(args.contains(&"-no-emul-boot".to_string()));
            assert!(
                args.contains(&"efi.img".to_string()) || args.iter().any(|a| a.contains("efi"))
            );
        } else {
            panic!("expected Command");
        }
    }

    #[test]
    fn iso_build_command_bios() {
        let mut iso = IsoConfig::default();
        iso.bios = true;
        iso.uefi = false;
        let op = iso.build_command();
        if let SystemOp::Command { args, .. } = op {
            assert!(args.iter().any(|a| a.contains("bios")));
        } else {
            panic!("expected Command");
        }
    }

    #[test]
    fn system_op_display() {
        let op = SystemOp::Command {
            binary: "parted".into(),
            args: vec!["-s".into(), "/dev/sda".into()],
            description: "partition disk".into(),
            fatal: true,
        };
        let s = format!("{}", op);
        assert!(s.contains("partition disk"));
        assert!(s.contains("parted"));

        let op = SystemOp::WriteFile {
            path: "/etc/hostname".into(),
            content: "agnos\n".into(),
            mode: 0o644,
            owner: None,
        };
        assert_eq!(format!("{}", op), "write /etc/hostname");

        let op = SystemOp::Symlink {
            target: "/usr/share/zoneinfo/UTC".into(),
            link: "/etc/localtime".into(),
        };
        assert!(format!("{}", op).contains("symlink"));
    }
}
