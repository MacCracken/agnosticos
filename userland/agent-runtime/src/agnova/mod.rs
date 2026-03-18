//! Agnova — OS Installer for AGNOS
//!
//! Handles disk partitioning, encryption, bootloader installation,
//! base system deployment, and first-boot configuration. Named from
//! AGNOS + Latin "nova" (new) — agnova creates new AGNOS installations.
//!
//! Submodules:
//! - **types**: All enums, structs, and config types
//! - **helpers**: Free-function utilities (fstab, hostname, machine-id, kernel params)
//! - **validation**: `validate_config` implementation on `AgnovaInstaller`
//! - **partitioning**: Disk partition, format, and encryption phase ops
//! - **rootfs**: Mount, install, bootloader, user, network, locale, security, first-boot, cleanup ops

pub mod types;

mod helpers;
mod partitioning;
mod rootfs;
mod validation;

#[cfg(test)]
mod tests;

pub use helpers::{
    default_kernel_params, generate_fstab, generate_hostname_config, generate_machine_id,
};
pub use types::{
    BootloaderConfig, BootloaderType, DiskLayout, Filesystem, FirewallDefault, InstallConfig,
    InstallError, InstallMode, InstallPhase, InstallProgress, InstallResult, IsoConfig,
    NetworkConfig, PackageSelection, PartitionFlag, PartitionSpec, PhaseOps, SecurityConfig,
    SystemOp, TrustEnforcementMode, UserConfig,
};

use chrono::Utc;
use tracing::{info, warn};

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
            luks_passphrase: None,
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
        use tracing::debug;
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
