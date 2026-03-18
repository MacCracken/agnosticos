use super::helpers::{
    default_kernel_params, generate_fstab, generate_hostname_config, generate_machine_id,
};
use super::types::{
    BootloaderConfig, BootloaderType, DiskLayout, Filesystem, FirewallDefault, InstallConfig,
    InstallError, InstallMode, InstallPhase, InstallProgress, NetworkConfig, PackageSelection,
    PartitionFlag, PartitionSpec, SecurityConfig, SystemOp, TrustEnforcementMode, UserConfig,
};
use super::AgnovaInstaller;

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
    let flags = [
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
    let first_mount = ops
        .operations
        .iter()
        .find(|op| matches!(op, SystemOp::Mount { .. }));
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
    let has_tar = ops
        .operations
        .iter()
        .any(|op| matches!(op, SystemOp::Command { binary, .. } if binary == "tar"));
    assert!(has_tar, "expected tar extraction");
}

#[test]
fn install_packages_ops_uses_ark() {
    let config = test_config();
    let installer = AgnovaInstaller::new(config);
    let ops = installer.plan_install_packages_ops("/mnt");
    assert_eq!(ops.phase, InstallPhase::InstallPackages);
    let has_ark = ops
        .operations
        .iter()
        .any(|op| matches!(op, SystemOp::Command { binary, .. } if binary == "ark"));
    assert!(has_ark, "expected ark install command");
}

#[test]
fn security_ops_writes_nft_and_sysctl() {
    let config = test_config();
    let installer = AgnovaInstaller::new(config);
    let ops = installer.plan_security_ops("/mnt");
    assert_eq!(ops.phase, InstallPhase::SetupSecurity);
    let has_nft = ops
        .operations
        .iter()
        .any(|op| matches!(op, SystemOp::WriteFile { path, .. } if path.contains("nftables")));
    let has_sysctl = ops
        .operations
        .iter()
        .any(|op| matches!(op, SystemOp::WriteFile { path, .. } if path.contains("sysctl")));
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
    let has_chroot = ops
        .operations
        .iter()
        .any(|op| matches!(op, SystemOp::Command { binary, .. } if binary == "chroot"));
    assert!(has_chroot, "expected chroot service enable");
    // Desktop mode should enable compositor
    let has_compositor = ops.operations.iter().any(|op| {
        if let SystemOp::Command { args, .. } = op {
            args.iter().any(|a| a == "aethersafha")
        } else {
            false
        }
    });
    assert!(
        has_compositor,
        "expected compositor enable for Desktop mode"
    );
}

#[test]
fn cleanup_ops_unmounts_and_syncs() {
    let config = test_config();
    let installer = AgnovaInstaller::new(config);
    let ops = installer.plan_cleanup_ops("/mnt");
    assert_eq!(ops.phase, InstallPhase::Cleanup);
    let has_sync = ops
        .operations
        .iter()
        .any(|op| matches!(op, SystemOp::Command { binary, .. } if binary == "sync"));
    let has_unmount = ops
        .operations
        .iter()
        .any(|op| matches!(op, SystemOp::Unmount { .. }));
    assert!(has_sync, "expected sync");
    assert!(has_unmount, "expected unmount");
}

#[test]
fn partition_device_helper_sda() {
    assert_eq!(
        AgnovaInstaller::partition_device("/dev/sda", 0),
        "/dev/sda1"
    );
    assert_eq!(
        AgnovaInstaller::partition_device("/dev/sda", 2),
        "/dev/sda3"
    );
}

#[test]
fn partition_device_helper_nvme() {
    assert_eq!(
        AgnovaInstaller::partition_device("/dev/nvme0n1", 0),
        "/dev/nvme0n1p1"
    );
    assert_eq!(
        AgnovaInstaller::partition_device("/dev/mmcblk0", 1),
        "/dev/mmcblk0p2"
    );
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
    let has_loader_conf = ops
        .operations
        .iter()
        .any(|op| matches!(op, SystemOp::WriteFile { path, .. } if path.contains("loader.conf")));
    let has_entry = ops
        .operations
        .iter()
        .any(|op| matches!(op, SystemOp::WriteFile { path, .. } if path.contains("agnos.conf")));
    let has_rescue = ops.operations.iter().any(
        |op| matches!(op, SystemOp::WriteFile { path, .. } if path.contains("agnos-rescue.conf")),
    );
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
            if path.contains("grub.cfg") {
                Some(content.as_str())
            } else {
                None
            }
        } else {
            None
        }
    });
    let cfg = grub_cfg.expect("grub.cfg should exist");
    assert!(
        cfg.contains("vmlinuz-"),
        "should reference kernel by version"
    );
    assert!(
        cfg.contains("initramfs-"),
        "should reference initramfs by version"
    );
    // Should NOT have bare "6.6.72" hardcoded without the installer method
    assert!(cfg.contains(installer.kernel_version()));
}

// -----------------------------------------------------------------------
// ISO config tests
// -----------------------------------------------------------------------

#[test]
fn iso_config_defaults() {
    use super::types::IsoConfig;
    let iso = IsoConfig::default();
    assert!(iso.uefi);
    assert!(!iso.bios);
    assert_eq!(iso.compression, "zstd");
    assert_eq!(iso.volume_label, "AGNOS");
}

#[test]
fn iso_build_command_uefi() {
    use super::types::IsoConfig;
    let iso = IsoConfig::default();
    let op = iso.build_command();
    if let SystemOp::Command { binary, args, .. } = op {
        assert_eq!(binary, "xorriso");
        assert!(args.contains(&"-no-emul-boot".to_string()));
        assert!(args.contains(&"efi.img".to_string()) || args.iter().any(|a| a.contains("efi")));
    } else {
        panic!("expected Command");
    }
}

#[test]
fn iso_build_command_bios() {
    use super::types::IsoConfig;
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
        stdin: None,
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

// -- LUKS stdin passphrase piping --

#[test]
fn luks_format_uses_stdin_passphrase() {
    let mut config = test_config();
    config.disk.encrypt = true;
    config.disk.luks_passphrase = Some("hunter2".to_string());
    let installer = AgnovaInstaller::new(config);
    let ops = installer.plan_encryption_ops();
    assert!(ops.operations.len() >= 2);

    // luksFormat command
    if let SystemOp::Command {
        args,
        stdin,
        binary,
        ..
    } = &ops.operations[0]
    {
        assert_eq!(binary, "cryptsetup");
        assert!(args.contains(&"--batch-mode".to_string()));
        assert!(args.contains(&"--key-file=-".to_string()));
        assert_eq!(stdin.as_deref(), Some("hunter2"));
    } else {
        panic!("expected Command for luksFormat");
    }

    // open command
    if let SystemOp::Command {
        args,
        stdin,
        binary,
        ..
    } = &ops.operations[1]
    {
        assert_eq!(binary, "cryptsetup");
        assert!(args.contains(&"open".to_string()));
        assert!(args.contains(&"--key-file=-".to_string()));
        assert_eq!(stdin.as_deref(), Some("hunter2"));
    } else {
        panic!("expected Command for open");
    }
}

// -- MBR partition count validation --

#[test]
fn validate_rejects_mbr_with_more_than_four_partitions() {
    let mut config = test_config();
    config.disk.use_gpt = false;
    // Create 5 partitions (exceeds MBR limit)
    config.disk.partitions = (1..=5)
        .map(|i| PartitionSpec {
            label: format!("part{}", i),
            mount_point: if i == 5 {
                "/".to_string()
            } else {
                format!("/mnt/{}", i)
            },
            filesystem: Filesystem::Ext4,
            size_mb: if i == 5 { None } else { Some(1024) },
            flags: Vec::new(),
        })
        .collect();
    let installer = AgnovaInstaller::new(config);
    let err = installer.validate_config().unwrap_err();
    assert!(err.to_string().contains("MBR partition table limited to 4"));
}

#[test]
fn validate_accepts_mbr_with_four_partitions() {
    let mut config = test_config();
    config.disk.use_gpt = false;
    config.disk.partitions = (1..=4)
        .map(|i| PartitionSpec {
            label: format!("part{}", i),
            mount_point: if i == 4 {
                "/".to_string()
            } else {
                format!("/mnt/{}", i)
            },
            filesystem: Filesystem::Ext4,
            size_mb: if i == 4 { None } else { Some(1024) },
            flags: Vec::new(),
        })
        .collect();
    let installer = AgnovaInstaller::new(config);
    assert!(installer.validate_config().is_ok());
}

// -- Static IP network configuration --

#[test]
fn plan_network_ops_static_ip_config() {
    let mut config = test_config();
    config.network.use_dhcp = false;
    config.network.static_ip = Some("192.168.1.100/24".to_string());
    config.network.gateway = Some("192.168.1.1".to_string());
    config.network.dns = vec!["1.1.1.1".to_string(), "8.8.8.8".to_string()];
    let installer = AgnovaInstaller::new(config);
    let ops = installer.plan_network_ops("/mnt/target");

    // Should have: hostname, hosts, resolv.conf, mkdir networkd, 10-static.network
    let network_file = ops.operations.iter().find(
        |op| matches!(op, SystemOp::WriteFile { path, .. } if path.contains("10-static.network")),
    );
    assert!(network_file.is_some(), "should generate 10-static.network");

    if let SystemOp::WriteFile { content, path, .. } = network_file.unwrap() {
        assert!(path.ends_with("/etc/systemd/network/10-static.network"));
        assert!(content.contains("[Match]"));
        assert!(content.contains("Name=eth0"));
        assert!(content.contains("Address=192.168.1.100/24"));
        assert!(content.contains("Gateway=192.168.1.1"));
        assert!(content.contains("DNS=1.1.1.1"));
        assert!(content.contains("DNS=8.8.8.8"));
    }
}

#[test]
fn plan_network_ops_dhcp_no_static_file() {
    let config = test_config(); // use_dhcp=true by default
    let installer = AgnovaInstaller::new(config);
    let ops = installer.plan_network_ops("/mnt/target");

    let has_static = ops.operations.iter().any(
        |op| matches!(op, SystemOp::WriteFile { path, .. } if path.contains("10-static.network")),
    );
    assert!(
        !has_static,
        "DHCP mode should not generate static network config"
    );
}
