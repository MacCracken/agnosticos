//! Root filesystem creation, package installation, and system configuration
//! operations for agnova.

use super::helpers::{generate_fstab, generate_hostname_config, generate_machine_id};
use super::types::{
    BootloaderType, Filesystem, FirewallDefault, InstallMode, InstallPhase, PhaseOps, SystemOp,
};
use super::AgnovaInstaller;

impl AgnovaInstaller {
    /// Kernel version string used in boot entries. Derived from the kernel
    /// recipe or overridden via `InstallConfig`. Centralised here so boot
    /// entries stay in sync with the installed kernel.
    pub(crate) fn kernel_version(&self) -> &str {
        // Future: read from config.kernel_version once the field exists.
        // For now use the version from the LFS kernel recipe.
        "6.6.130-agnos"
    }

    /// Detect whether the *running* system booted via UEFI by probing
    /// `/sys/firmware/efi`. This is a pure check — no side-effects.
    pub fn is_uefi_system() -> bool {
        std::path::Path::new("/sys/firmware/efi").exists()
    }

    /// Generate the operations needed to mount partitions at the target root.
    pub fn plan_mount_ops(&self, target_root: &str) -> PhaseOps {
        let disk = &self.config.disk;
        let device = &disk.target_device;
        let mut ops = Vec::new();

        // Sort partitions: mount "/" first, then others by mount-point depth
        let mut sorted: Vec<(usize, &super::types::PartitionSpec)> =
            disk.partitions.iter().enumerate().collect();
        sorted.sort_by_key(|(_, p)| {
            if p.mount_point == "/" {
                0
            } else {
                p.mount_point.matches('/').count()
            }
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
                    stdin: None,
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
            "bin",
            "sbin",
            "lib",
            "lib64",
            "usr/bin",
            "usr/sbin",
            "usr/lib",
            "etc",
            "var/log",
            "var/lib/agnos/ark/installed",
            "tmp",
            "proc",
            "sys",
            "dev",
            "run",
            "home",
            "root",
            "boot",
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
            stdin: None,
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
            stdin: None,
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
                stdin: None,
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
                stdin: None,
            });
        }

        PhaseOps {
            phase: InstallPhase::InstallPackages,
            description: format!("Install packages for {} mode", self.config.mode),
            operations: ops,
        }
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
                        stdin: None,
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
                        stdin: None,
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
                    boot.default_entry,
                    boot.timeout_secs,
                    kver,
                    kernel_cmdline,
                    kver,
                    kver,
                    kernel_cmdline,
                    kver,
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
                    stdin: None,
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
            stdin: None,
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
                stdin: None,
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

        // Static IP configuration via systemd-networkd
        if !net.use_dhcp {
            if let Some(ref static_ip) = net.static_ip {
                let mut networkd = String::new();
                networkd.push_str("[Match]\nName=eth0\n\n[Network]\n");
                networkd.push_str(&format!("Address={}\n", static_ip));
                if let Some(ref gw) = net.gateway {
                    networkd.push_str(&format!("Gateway={}\n", gw));
                }
                for dns in &net.dns {
                    networkd.push_str(&format!("DNS={}\n", dns));
                }

                // Ensure the networkd config directory exists
                ops.push(SystemOp::MakeDir {
                    path: format!("{}/etc/systemd/network", target_root),
                    mode: 0o755,
                    parents: true,
                });

                ops.push(SystemOp::WriteFile {
                    path: format!("{}/etc/systemd/network/10-static.network", target_root),
                    content: networkd,
                    mode: 0o644,
                    owner: None,
                });
            }
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
                "daimon",
                "hoosh",
                "aegis",
                "nftables",
                "sshd",
                "networkmanager",
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
                stdin: None,
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
                stdin: None,
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
            stdin: None,
        });

        // Unmount in reverse depth order (deepest first)
        let mut mounts: Vec<&super::types::PartitionSpec> = disk
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
                    stdin: None,
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
                stdin: None,
            });
        }

        PhaseOps {
            phase: InstallPhase::Cleanup,
            description: "Cleanup and unmount".into(),
            operations: ops,
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
