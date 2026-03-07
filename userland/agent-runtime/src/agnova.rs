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
        Self::ALL.iter().position(|&p| p == self).expect("phase must be in ALL")
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
        } else if part.filesystem == Filesystem::Vfat {
            format!("LABEL={}", part.label)
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
    let mut params = vec![
        "quiet".to_string(),
        "loglevel=3".to_string(),
    ];

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
        if dev.contains("..") || dev.contains(' ') || dev.contains(';')
            || dev.contains('|') || dev.contains('&') || dev.contains('`')
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
        if !uname.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-') {
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
        if !hostname.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
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

        debug!("agnova: config validation passed with {} warning(s)", warnings.len());
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
            "linux-kernel", "linux-firmware", "agnos-init", "agnos-sys",
            "agnos-common", "agnoshi", "shakti", "daimon", "hoosh",
            "systemd", "dbus", "networkmanager", "nftables", "openssh",
            "coreutils", "util-linux", "bash", "zsh", "curl", "wget",
            "ca-certificates", "gnupg", "tar", "gzip", "xz", "bzip2",
            "iproute2", "iputils", "less", "nano", "man-pages",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        let (mode_packages, size) = match mode {
            InstallMode::Server => {
                let pkgs: Vec<String> = [
                    "hoosh-server", "daimon-server", "ark", "nous",
                    "prometheus-node-exporter", "fail2ban", "tmux",
                ]
                .iter()
                .map(|s| s.to_string())
                .collect();
                (pkgs, 2400)
            }
            InstallMode::Desktop => {
                let pkgs: Vec<String> = [
                    "aethersafha", "pipewire", "wireplumber", "mesa",
                    "vulkan-loader", "fonts-noto", "fonts-jetbrains-mono",
                    "ark", "nous", "hoosh-server", "daimon-server",
                    "xdg-utils", "nautilus", "evince", "firefox",
                ]
                .iter()
                .map(|s| s.to_string())
                .collect();
                (pkgs, 4800)
            }
            InstallMode::Minimal => {
                (Vec::new(), 800)
            }
            InstallMode::Custom => {
                (Vec::new(), 1200)
            }
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
        if self.errors.iter().any(|e| e.phase == current && !e.recoverable) {
            warn!("agnova: cannot advance past non-recoverable failure at {}", current);
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
        assert_eq!(
            InstallPhase::Complete.to_string(),
            "Installation complete"
        );
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
}
