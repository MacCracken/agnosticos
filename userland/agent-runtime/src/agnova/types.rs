//! Core types, enums, and structs for agnova.

use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

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
    /// LUKS passphrase (piped via stdin to cryptsetup).
    #[serde(default, skip_serializing)]
    pub luks_passphrase: Option<String>,
}

impl Default for DiskLayout {
    fn default() -> Self {
        Self {
            target_device: String::new(),
            partitions: Vec::new(),
            use_gpt: true,
            encrypt: false,
            luks_passphrase: None,
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
    pub(crate) fn new() -> Self {
        Self {
            current_phase: InstallPhase::ValidateConfig,
            phase_progress: 0.0,
            overall_progress: 0.0,
            message: "Preparing installation".to_string(),
            started_at: chrono::Utc::now(),
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
// SystemOp / PhaseOps
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
        /// Optional data to pipe to the command's stdin.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stdin: Option<String>,
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

// ---------------------------------------------------------------------------
// IsoConfig
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
            stdin: None,
        }
    }
}
