//! Disk partitioning, formatting, and encryption operations for agnova.

use super::types::{Filesystem, InstallPhase, PartitionFlag, PhaseOps, SystemOp};
use super::AgnovaInstaller;

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
                stdin: None,
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
                stdin: None,
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
                stdin: None,
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
                    stdin: None,
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
                stdin: None,
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
                    "--batch-mode".into(),
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
                    "--key-file=-".into(),
                    part_dev.clone(),
                ],
                description: "Format LUKS2 encrypted volume".into(),
                fatal: true,
                stdin: disk.luks_passphrase.clone(),
            });

            ops.push(SystemOp::Command {
                binary: "cryptsetup".into(),
                args: vec![
                    "open".into(),
                    "--key-file=-".into(),
                    part_dev,
                    "agnos-root".into(),
                ],
                description: "Open LUKS volume as agnos-root".into(),
                fatal: true,
                stdin: disk.luks_passphrase.clone(),
            });
        }

        PhaseOps {
            phase: InstallPhase::SetupEncryption,
            description: "Setup disk encryption".into(),
            operations: ops,
        }
    }

    /// Helper: generate the partition device path for partition index `i`.
    pub(crate) fn partition_device(device: &str, i: usize) -> String {
        if device.contains("nvme") || device.contains("mmcblk") {
            format!("{}p{}", device, i + 1)
        } else {
            format!("{}{}", device, i + 1)
        }
    }
}
