//! FUSE (Filesystem in Userspace) Management
//!
//! Provides mount/unmount/status operations for FUSE filesystems, including
//! per-agent overlayfs sandboxing. Shells out to `fusermount` / `mount` (standard
//! Linux tools).
//!
//! On non-Linux platforms, all operations return `SysError::NotSupported`.

use crate::error::{Result, SysError};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// A mounted FUSE filesystem.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FuseMount {
    /// Where the filesystem is mounted
    pub mountpoint: PathBuf,
    /// Filesystem type (e.g., `fuse.sshfs`, `fuse.s3fs`)
    pub fstype: String,
    /// Source device or remote path
    pub source: String,
    /// Mount options
    pub options: Vec<String>,
    /// PID of the FUSE daemon process, if known
    pub pid: Option<u32>,
}

/// Options controlling FUSE mount behaviour.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FuseMountOptions {
    /// Allow other users to access the mount
    pub allow_other: bool,
    /// Allow root to access the mount
    pub allow_root: bool,
    /// Enable kernel-level permission checks
    pub default_permissions: bool,
    /// Maximum read size in bytes
    pub max_read: Option<u32>,
    /// Maximum write size in bytes
    pub max_write: Option<u32>,
    /// Allow mounting over a non-empty directory
    pub nonempty: bool,
    /// UID to report for files
    pub uid: Option<u32>,
    /// GID to report for files
    pub gid: Option<u32>,
}

impl Default for FuseMountOptions {
    fn default() -> Self {
        Self {
            allow_other: false,
            allow_root: false,
            default_permissions: true,
            max_read: None,
            max_write: None,
            nonempty: false,
            uid: None,
            gid: None,
        }
    }
}

/// Well-known FUSE filesystem types.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FuseFilesystem {
    /// SSH filesystem
    Sshfs,
    /// Amazon S3 filesystem
    S3fs,
    /// Rclone-based cloud filesystem
    Rclone,
    /// Overlay filesystem (union mount)
    OverlayFs,
    /// Bind filesystem (permission remapping)
    BindFs,
    /// Any other FUSE filesystem
    Custom(String),
}

impl FuseFilesystem {
    /// Return the binary name used to mount this filesystem.
    pub fn binary_name(&self) -> &str {
        match self {
            FuseFilesystem::Sshfs => "sshfs",
            FuseFilesystem::S3fs => "s3fs",
            FuseFilesystem::Rclone => "rclone",
            FuseFilesystem::OverlayFs => "fuse-overlayfs",
            FuseFilesystem::BindFs => "bindfs",
            FuseFilesystem::Custom(name) => name.as_str(),
        }
    }

    /// Return the `fuse.` type string as it appears in `/proc/mounts`.
    pub fn fstype_str(&self) -> String {
        match self {
            FuseFilesystem::Sshfs => "fuse.sshfs".to_string(),
            FuseFilesystem::S3fs => "fuse.s3fs".to_string(),
            FuseFilesystem::Rclone => "fuse.rclone".to_string(),
            FuseFilesystem::OverlayFs => "fuse.fuse-overlayfs".to_string(),
            FuseFilesystem::BindFs => "fuse.bindfs".to_string(),
            FuseFilesystem::Custom(name) => format!("fuse.{}", name),
        }
    }
}

impl std::fmt::Display for FuseFilesystem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FuseFilesystem::Sshfs => write!(f, "sshfs"),
            FuseFilesystem::S3fs => write!(f, "s3fs"),
            FuseFilesystem::Rclone => write!(f, "rclone"),
            FuseFilesystem::OverlayFs => write!(f, "fuse-overlayfs"),
            FuseFilesystem::BindFs => write!(f, "bindfs"),
            FuseFilesystem::Custom(name) => write!(f, "{}", name),
        }
    }
}

/// Runtime status of a FUSE mount.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FuseStatus {
    /// Whether the filesystem is currently mounted
    pub is_mounted: bool,
    /// Mount point path
    pub mountpoint: PathBuf,
    /// PID of the FUSE daemon, if known
    pub pid: Option<u32>,
    /// Filesystem type
    pub filesystem: String,
    /// Time since the mount was established
    pub uptime: Option<Duration>,
}

/// Per-agent FUSE configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentFuseConfig {
    /// Agent identifier
    pub agent_id: String,
    /// FUSE filesystem type to mount
    pub filesystem: FuseFilesystem,
    /// Source (remote path, bucket name, etc.)
    pub source: String,
    /// Where to mount the filesystem
    pub mountpoint: PathBuf,
    /// Mount options
    pub options: FuseMountOptions,
    /// Mount as read-only
    pub read_only: bool,
}

// ---------------------------------------------------------------------------
// Pure functions
// ---------------------------------------------------------------------------

/// Parse the content of `/proc/mounts` and return all FUSE entries.
///
/// This is a pure function suitable for unit testing.
pub fn parse_proc_mounts(content: &str) -> Vec<FuseMount> {
    content
        .lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 4 {
                return None;
            }
            let fstype = parts[2];
            if !fstype.starts_with("fuse") {
                return None;
            }
            let options: Vec<String> = parts[3].split(',').map(|s| s.to_string()).collect();
            Some(FuseMount {
                source: parts[0].to_string(),
                mountpoint: PathBuf::from(parts[1]),
                fstype: fstype.to_string(),
                options,
                pid: None,
            })
        })
        .collect()
}

/// Render [`FuseMountOptions`] into a comma-separated `-o` option string.
///
/// This is a pure function suitable for unit testing.
pub fn render_mount_options(opts: &FuseMountOptions) -> String {
    let mut parts: Vec<String> = Vec::new();

    if opts.allow_other {
        parts.push("allow_other".to_string());
    }
    if opts.allow_root {
        parts.push("allow_root".to_string());
    }
    if opts.default_permissions {
        parts.push("default_permissions".to_string());
    }
    if let Some(max_read) = opts.max_read {
        parts.push(format!("max_read={}", max_read));
    }
    if let Some(max_write) = opts.max_write {
        parts.push(format!("max_write={}", max_write));
    }
    if opts.nonempty {
        parts.push("nonempty".to_string());
    }
    if let Some(uid) = opts.uid {
        parts.push(format!("uid={}", uid));
    }
    if let Some(gid) = opts.gid {
        parts.push(format!("gid={}", gid));
    }

    parts.join(",")
}

/// Validate that a path is suitable as a FUSE mountpoint.
///
/// Checks: exists, is a directory, is empty, and is not the filesystem root.
pub fn validate_mountpoint(path: &Path) -> Result<()> {
    if path == Path::new("/") {
        return Err(SysError::InvalidArgument(
            "Cannot use filesystem root as mountpoint".into(),
        ));
    }

    if !path.exists() {
        return Err(SysError::InvalidArgument(format!(
            "Mountpoint does not exist: {}",
            path.display()
        )));
    }

    if !path.is_dir() {
        return Err(SysError::InvalidArgument(format!(
            "Mountpoint is not a directory: {}",
            path.display()
        )));
    }

    let entries = std::fs::read_dir(path).map_err(|e| {
        SysError::Unknown(format!(
            "Cannot read mountpoint directory {}: {}",
            path.display(),
            e
        ))
    })?;
    if entries.count() > 0 {
        return Err(SysError::InvalidArgument(format!(
            "Mountpoint is not empty: {}",
            path.display()
        )));
    }

    Ok(())
}

/// Check whether FUSE is available on this system by testing for `/dev/fuse`.
pub fn is_fuse_available() -> bool {
    #[cfg(target_os = "linux")]
    {
        Path::new("/dev/fuse").exists()
    }

    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}

// ---------------------------------------------------------------------------
// System-interacting functions
// ---------------------------------------------------------------------------

/// List all currently mounted FUSE filesystems by reading `/proc/mounts`.
pub fn list_fuse_mounts() -> Result<Vec<FuseMount>> {
    #[cfg(target_os = "linux")]
    {
        let content = std::fs::read_to_string("/proc/mounts")
            .map_err(|e| SysError::Unknown(format!("Failed to read /proc/mounts: {}", e)))?;
        Ok(parse_proc_mounts(&content))
    }

    #[cfg(not(target_os = "linux"))]
    {
        Err(SysError::NotSupported)
    }
}

/// Mount a FUSE filesystem for an agent.
///
/// Constructs the appropriate command for the configured [`FuseFilesystem`] type,
/// applies [`FuseMountOptions`], and spawns the FUSE daemon.
pub fn mount_fuse(config: &AgentFuseConfig) -> Result<FuseMount> {
    #[cfg(target_os = "linux")]
    {
        if config.agent_id.is_empty() {
            return Err(SysError::InvalidArgument("Agent ID cannot be empty".into()));
        }
        if config.source.is_empty() {
            return Err(SysError::InvalidArgument("Source cannot be empty".into()));
        }

        validate_mountpoint(&config.mountpoint)?;

        let opts_str = render_mount_options(&config.options);
        let mut opt_parts: Vec<String> = Vec::new();
        if !opts_str.is_empty() {
            opt_parts.push(opts_str);
        }
        if config.read_only {
            opt_parts.push("ro".to_string());
        }
        let full_opts = opt_parts.join(",");

        let binary = config.filesystem.binary_name();
        let mountpoint_str = config.mountpoint.to_string_lossy().to_string();

        let mut cmd = std::process::Command::new(binary);

        // Each filesystem has its own argument convention
        match &config.filesystem {
            FuseFilesystem::Rclone => {
                cmd.args(["mount", &config.source, &mountpoint_str]);
                if !full_opts.is_empty() {
                    cmd.args(["--fuse-flag", &full_opts]);
                }
                cmd.arg("--daemon");
            }
            _ => {
                cmd.arg(&config.source);
                cmd.arg(&mountpoint_str);
                if !full_opts.is_empty() {
                    cmd.args(["-o", &full_opts]);
                }
            }
        }

        let output = cmd
            .output()
            .map_err(|e| SysError::Unknown(format!("Failed to run {}: {}", binary, e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(SysError::Unknown(format!(
                "{} mount failed: {}",
                binary,
                stderr.trim()
            )));
        }

        tracing::info!(
            agent_id = %config.agent_id,
            filesystem = %config.filesystem,
            mountpoint = %config.mountpoint.display(),
            "Mounted FUSE filesystem for agent"
        );

        Ok(FuseMount {
            mountpoint: config.mountpoint.clone(),
            fstype: config.filesystem.fstype_str(),
            source: config.source.clone(),
            options: if full_opts.is_empty() {
                vec![]
            } else {
                full_opts.split(',').map(|s| s.to_string()).collect()
            },
            pid: None,
        })
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = config;
        Err(SysError::NotSupported)
    }
}

/// Unmount a FUSE filesystem.
///
/// Attempts `fusermount -u` first; falls back to `umount` on failure.
pub fn unmount_fuse(mountpoint: &Path) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        if !mountpoint.exists() {
            return Err(SysError::InvalidArgument(format!(
                "Mountpoint does not exist: {}",
                mountpoint.display()
            )));
        }

        let mp = mountpoint.to_string_lossy().to_string();

        // Try fusermount first (unprivileged)
        let result = std::process::Command::new("fusermount")
            .args(["-u", &mp])
            .output();

        match result {
            Ok(output) if output.status.success() => {
                tracing::info!("Unmounted FUSE filesystem at {}", mountpoint.display());
                return Ok(());
            }
            _ => {}
        }

        // Fall back to umount
        let output = std::process::Command::new("umount")
            .arg(&mp)
            .output()
            .map_err(|e| SysError::Unknown(format!("Failed to run umount: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(SysError::Unknown(format!(
                "Failed to unmount {}: {}",
                mountpoint.display(),
                stderr.trim()
            )));
        }

        tracing::info!(
            "Unmounted FUSE filesystem at {} (via umount)",
            mountpoint.display()
        );
        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = mountpoint;
        Err(SysError::NotSupported)
    }
}

/// Get the status of a FUSE mount at the given mountpoint.
pub fn get_fuse_status(mountpoint: &Path) -> Result<FuseStatus> {
    #[cfg(target_os = "linux")]
    {
        let content = std::fs::read_to_string("/proc/mounts")
            .map_err(|e| SysError::Unknown(format!("Failed to read /proc/mounts: {}", e)))?;
        let mounts = parse_proc_mounts(&content);

        let canonical = mountpoint
            .canonicalize()
            .unwrap_or_else(|_| mountpoint.to_path_buf());

        for mount in &mounts {
            let mount_canonical = mount
                .mountpoint
                .canonicalize()
                .unwrap_or_else(|_| mount.mountpoint.clone());
            if mount_canonical == canonical {
                return Ok(FuseStatus {
                    is_mounted: true,
                    mountpoint: mount.mountpoint.clone(),
                    pid: mount.pid,
                    filesystem: mount.fstype.clone(),
                    uptime: None, // uptime requires additional tracking
                });
            }
        }

        Ok(FuseStatus {
            is_mounted: false,
            mountpoint: mountpoint.to_path_buf(),
            pid: None,
            filesystem: String::new(),
            uptime: None,
        })
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = mountpoint;
        Err(SysError::NotSupported)
    }
}

/// Set up a FUSE overlayfs for agent sandboxing.
///
/// Creates a union mount with the given lower (read-only), work, and mount
/// directories. The upper directory is automatically derived as `work_dir/upper`.
pub fn setup_agent_overlay(
    agent_id: &str,
    lower_dir: &Path,
    work_dir: &Path,
    mount_dir: &Path,
) -> Result<FuseMount> {
    #[cfg(target_os = "linux")]
    {
        if agent_id.is_empty() {
            return Err(SysError::InvalidArgument("Agent ID cannot be empty".into()));
        }
        if !lower_dir.exists() {
            return Err(SysError::InvalidArgument(format!(
                "Lower directory does not exist: {}",
                lower_dir.display()
            )));
        }
        if !work_dir.exists() {
            return Err(SysError::InvalidArgument(format!(
                "Work directory does not exist: {}",
                work_dir.display()
            )));
        }

        // Create upper directory inside work_dir
        let upper_dir = work_dir.join("upper");
        std::fs::create_dir_all(&upper_dir).map_err(|e| {
            SysError::Unknown(format!(
                "Failed to create upper directory {}: {}",
                upper_dir.display(),
                e
            ))
        })?;

        let opts = format!(
            "lowerdir={},upperdir={},workdir={}",
            lower_dir.display(),
            upper_dir.display(),
            work_dir.display()
        );
        let mount_str = mount_dir.to_string_lossy().to_string();

        let output = std::process::Command::new("fuse-overlayfs")
            .args(["-o", &opts, &mount_str])
            .output()
            .map_err(|e| SysError::Unknown(format!("Failed to run fuse-overlayfs: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(SysError::Unknown(format!(
                "fuse-overlayfs failed for agent '{}': {}",
                agent_id,
                stderr.trim()
            )));
        }

        tracing::info!(
            agent_id = %agent_id,
            lower = %lower_dir.display(),
            mount = %mount_dir.display(),
            "Set up overlay filesystem for agent"
        );

        Ok(FuseMount {
            mountpoint: mount_dir.to_path_buf(),
            fstype: "fuse.fuse-overlayfs".to_string(),
            source: format!("overlay:{}", agent_id),
            options: opts.split(',').map(|s| s.to_string()).collect(),
            pid: None,
        })
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = (agent_id, lower_dir, work_dir, mount_dir);
        Err(SysError::NotSupported)
    }
}

/// Unmount all FUSE filesystems belonging to an agent.
///
/// Scans `/proc/mounts` for mountpoints under `/run/agnos/agents/{agent_id}/`
/// and unmounts each one.
pub fn cleanup_agent_mounts(agent_id: &str) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        if agent_id.is_empty() {
            return Err(SysError::InvalidArgument("Agent ID cannot be empty".into()));
        }

        let prefix = format!("/run/agnos/agents/{}/", agent_id);
        let content = std::fs::read_to_string("/proc/mounts")
            .map_err(|e| SysError::Unknown(format!("Failed to read /proc/mounts: {}", e)))?;
        let mounts = parse_proc_mounts(&content);

        let mut errors: Vec<String> = Vec::new();
        // Unmount in reverse order (nested mounts first)
        let agent_mounts: Vec<&FuseMount> = mounts
            .iter()
            .filter(|m| m.mountpoint.to_string_lossy().starts_with(&prefix))
            .collect();

        for mount in agent_mounts.iter().rev() {
            if let Err(e) = unmount_fuse(&mount.mountpoint) {
                errors.push(format!("{}: {}", mount.mountpoint.display(), e));
            }
        }

        if !errors.is_empty() {
            return Err(SysError::Unknown(format!(
                "Failed to unmount some agent filesystems: {}",
                errors.join("; ")
            )));
        }

        tracing::info!(agent_id = %agent_id, "Cleaned up all FUSE mounts for agent");
        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = agent_id;
        Err(SysError::NotSupported)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- parse_proc_mounts ---

    #[test]
    fn test_parse_proc_mounts_empty() {
        let mounts = parse_proc_mounts("");
        assert!(mounts.is_empty());
    }

    #[test]
    fn test_parse_proc_mounts_no_fuse() {
        let content = "\
/dev/sda1 / ext4 rw,relatime 0 0
tmpfs /tmp tmpfs rw,nosuid,nodev 0 0
proc /proc proc rw,nosuid,nodev,noexec 0 0";
        let mounts = parse_proc_mounts(content);
        assert!(mounts.is_empty());
    }

    #[test]
    fn test_parse_proc_mounts_single_fuse() {
        let content = "\
/dev/sda1 / ext4 rw,relatime 0 0
sshfs#user@host:/path /mnt/remote fuse.sshfs rw,nosuid,nodev,user_id=1000 0 0";
        let mounts = parse_proc_mounts(content);
        assert_eq!(mounts.len(), 1);
        assert_eq!(mounts[0].fstype, "fuse.sshfs");
        assert_eq!(mounts[0].mountpoint, PathBuf::from("/mnt/remote"));
        assert_eq!(mounts[0].source, "sshfs#user@host:/path");
        assert!(mounts[0].options.contains(&"rw".to_string()));
        assert!(mounts[0].pid.is_none());
    }

    #[test]
    fn test_parse_proc_mounts_multiple_fuse() {
        let content = "\
/dev/sda1 / ext4 rw 0 0
s3fs /mnt/bucket fuse.s3fs rw,nosuid 0 0
rclone:drive /mnt/drive fuse.rclone rw,user_id=1000 0 0
fuse-overlayfs /mnt/overlay fuse.fuse-overlayfs rw,lowerdir=/lower 0 0";
        let mounts = parse_proc_mounts(content);
        assert_eq!(mounts.len(), 3);
        assert_eq!(mounts[0].fstype, "fuse.s3fs");
        assert_eq!(mounts[1].fstype, "fuse.rclone");
        assert_eq!(mounts[2].fstype, "fuse.fuse-overlayfs");
    }

    #[test]
    fn test_parse_proc_mounts_bare_fuse_type() {
        // Some mounts show just "fuse" without a subtype
        let content = "custom /mnt/custom fuse rw,user_id=1000 0 0";
        let mounts = parse_proc_mounts(content);
        assert_eq!(mounts.len(), 1);
        assert_eq!(mounts[0].fstype, "fuse");
    }

    #[test]
    fn test_parse_proc_mounts_malformed_lines() {
        let content = "\
short line
/dev/sda1 / ext4 rw 0 0
only two fields
sshfs#host /mnt/r fuse.sshfs rw 0 0";
        let mounts = parse_proc_mounts(content);
        assert_eq!(mounts.len(), 1);
        assert_eq!(mounts[0].fstype, "fuse.sshfs");
    }

    #[test]
    fn test_parse_proc_mounts_options_split() {
        let content = "src /mnt fuse.test rw,nosuid,nodev,allow_other,max_read=131072 0 0";
        let mounts = parse_proc_mounts(content);
        assert_eq!(mounts.len(), 1);
        assert_eq!(mounts[0].options.len(), 5);
        assert!(mounts[0].options.contains(&"allow_other".to_string()));
        assert!(mounts[0].options.contains(&"max_read=131072".to_string()));
    }

    // --- render_mount_options ---

    #[test]
    fn test_render_mount_options_defaults() {
        let opts = FuseMountOptions::default();
        let rendered = render_mount_options(&opts);
        assert_eq!(rendered, "default_permissions");
    }

    #[test]
    fn test_render_mount_options_empty() {
        let opts = FuseMountOptions {
            allow_other: false,
            allow_root: false,
            default_permissions: false,
            max_read: None,
            max_write: None,
            nonempty: false,
            uid: None,
            gid: None,
        };
        let rendered = render_mount_options(&opts);
        assert!(rendered.is_empty());
    }

    #[test]
    fn test_render_mount_options_all_set() {
        let opts = FuseMountOptions {
            allow_other: true,
            allow_root: true,
            default_permissions: true,
            max_read: Some(131072),
            max_write: Some(65536),
            nonempty: true,
            uid: Some(1000),
            gid: Some(1000),
        };
        let rendered = render_mount_options(&opts);
        assert!(rendered.contains("allow_other"));
        assert!(rendered.contains("allow_root"));
        assert!(rendered.contains("default_permissions"));
        assert!(rendered.contains("max_read=131072"));
        assert!(rendered.contains("max_write=65536"));
        assert!(rendered.contains("nonempty"));
        assert!(rendered.contains("uid=1000"));
        assert!(rendered.contains("gid=1000"));
        // Verify comma separation
        assert_eq!(rendered.matches(',').count(), 7);
    }

    #[test]
    fn test_render_mount_options_partial() {
        let opts = FuseMountOptions {
            allow_other: true,
            allow_root: false,
            default_permissions: false,
            max_read: Some(4096),
            max_write: None,
            nonempty: false,
            uid: None,
            gid: Some(500),
        };
        let rendered = render_mount_options(&opts);
        assert_eq!(rendered, "allow_other,max_read=4096,gid=500");
    }

    // --- validate_mountpoint ---

    #[test]
    fn test_validate_mountpoint_root_rejected() {
        let err = validate_mountpoint(Path::new("/")).unwrap_err();
        assert!(err.to_string().contains("root"));
    }

    #[test]
    fn test_validate_mountpoint_nonexistent() {
        let err =
            validate_mountpoint(Path::new("/tmp/agnos_fuse_test_nonexistent_12345")).unwrap_err();
        assert!(err.to_string().contains("does not exist"));
    }

    #[test]
    fn test_validate_mountpoint_not_a_directory() {
        let dir = std::env::temp_dir().join("agnos_fuse_test_file");
        std::fs::write(&dir, "not a dir").unwrap();
        let err = validate_mountpoint(&dir).unwrap_err();
        assert!(err.to_string().contains("not a directory"));
        let _ = std::fs::remove_file(&dir);
    }

    #[test]
    fn test_validate_mountpoint_not_empty() {
        let dir = std::env::temp_dir().join("agnos_fuse_test_notempty");
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(dir.join("file.txt"), "data").unwrap();
        let err = validate_mountpoint(&dir).unwrap_err();
        assert!(err.to_string().contains("not empty"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_validate_mountpoint_ok() {
        let dir = std::env::temp_dir().join("agnos_fuse_test_ok");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(validate_mountpoint(&dir).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    // --- FuseFilesystem ---

    #[test]
    fn test_fuse_filesystem_binary_names() {
        assert_eq!(FuseFilesystem::Sshfs.binary_name(), "sshfs");
        assert_eq!(FuseFilesystem::S3fs.binary_name(), "s3fs");
        assert_eq!(FuseFilesystem::Rclone.binary_name(), "rclone");
        assert_eq!(FuseFilesystem::OverlayFs.binary_name(), "fuse-overlayfs");
        assert_eq!(FuseFilesystem::BindFs.binary_name(), "bindfs");
        assert_eq!(
            FuseFilesystem::Custom("ntfs-3g".to_string()).binary_name(),
            "ntfs-3g"
        );
    }

    #[test]
    fn test_fuse_filesystem_fstype_str() {
        assert_eq!(FuseFilesystem::Sshfs.fstype_str(), "fuse.sshfs");
        assert_eq!(FuseFilesystem::S3fs.fstype_str(), "fuse.s3fs");
        assert_eq!(
            FuseFilesystem::OverlayFs.fstype_str(),
            "fuse.fuse-overlayfs"
        );
        assert_eq!(
            FuseFilesystem::Custom("myfs".to_string()).fstype_str(),
            "fuse.myfs"
        );
    }

    #[test]
    fn test_fuse_filesystem_display() {
        assert_eq!(format!("{}", FuseFilesystem::Sshfs), "sshfs");
        assert_eq!(format!("{}", FuseFilesystem::Rclone), "rclone");
        assert_eq!(
            format!("{}", FuseFilesystem::Custom("zfs-fuse".to_string())),
            "zfs-fuse"
        );
    }

    #[test]
    fn test_fuse_filesystem_serde_roundtrip() {
        let variants = vec![
            FuseFilesystem::Sshfs,
            FuseFilesystem::S3fs,
            FuseFilesystem::Rclone,
            FuseFilesystem::OverlayFs,
            FuseFilesystem::BindFs,
            FuseFilesystem::Custom("ntfs-3g".to_string()),
        ];
        for fs in &variants {
            let json = serde_json::to_string(fs).unwrap();
            let back: FuseFilesystem = serde_json::from_str(&json).unwrap();
            assert_eq!(*fs, back);
        }
    }

    #[test]
    fn test_fuse_filesystem_eq() {
        assert_eq!(FuseFilesystem::Sshfs, FuseFilesystem::Sshfs);
        assert_ne!(FuseFilesystem::Sshfs, FuseFilesystem::S3fs);
        assert_eq!(
            FuseFilesystem::Custom("x".to_string()),
            FuseFilesystem::Custom("x".to_string())
        );
        assert_ne!(
            FuseFilesystem::Custom("x".to_string()),
            FuseFilesystem::Custom("y".to_string())
        );
    }

    // --- Config serialization ---

    #[test]
    fn test_agent_fuse_config_serde_roundtrip() {
        let config = AgentFuseConfig {
            agent_id: "agent-42".to_string(),
            filesystem: FuseFilesystem::Sshfs,
            source: "user@host:/data".to_string(),
            mountpoint: PathBuf::from("/run/agnos/agents/agent-42/data"),
            options: FuseMountOptions::default(),
            read_only: true,
        };
        let json = serde_json::to_string(&config).unwrap();
        let back: AgentFuseConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.agent_id, "agent-42");
        assert_eq!(back.filesystem, FuseFilesystem::Sshfs);
        assert!(back.read_only);
        assert!(back.options.default_permissions);
    }

    #[test]
    fn test_fuse_mount_serde_roundtrip() {
        let mount = FuseMount {
            mountpoint: PathBuf::from("/mnt/test"),
            fstype: "fuse.sshfs".to_string(),
            source: "host:/path".to_string(),
            options: vec!["rw".to_string(), "nosuid".to_string()],
            pid: Some(12345),
        };
        let json = serde_json::to_string(&mount).unwrap();
        let back: FuseMount = serde_json::from_str(&json).unwrap();
        assert_eq!(back.mountpoint, PathBuf::from("/mnt/test"));
        assert_eq!(back.pid, Some(12345));
        assert_eq!(back.options.len(), 2);
    }

    #[test]
    fn test_fuse_status_serde_roundtrip() {
        let status = FuseStatus {
            is_mounted: true,
            mountpoint: PathBuf::from("/mnt/fuse"),
            pid: Some(9999),
            filesystem: "fuse.s3fs".to_string(),
            uptime: Some(Duration::from_secs(3600)),
        };
        let json = serde_json::to_string(&status).unwrap();
        let back: FuseStatus = serde_json::from_str(&json).unwrap();
        assert!(back.is_mounted);
        assert_eq!(back.pid, Some(9999));
        assert_eq!(back.filesystem, "fuse.s3fs");
    }

    // --- FuseMountOptions ---

    #[test]
    fn test_fuse_mount_options_default() {
        let opts = FuseMountOptions::default();
        assert!(!opts.allow_other);
        assert!(!opts.allow_root);
        assert!(opts.default_permissions);
        assert!(opts.max_read.is_none());
        assert!(opts.max_write.is_none());
        assert!(!opts.nonempty);
        assert!(opts.uid.is_none());
        assert!(opts.gid.is_none());
    }

    #[test]
    fn test_fuse_mount_options_serde_roundtrip() {
        let opts = FuseMountOptions {
            allow_other: true,
            allow_root: false,
            default_permissions: true,
            max_read: Some(65536),
            max_write: None,
            nonempty: true,
            uid: Some(0),
            gid: Some(0),
        };
        let json = serde_json::to_string(&opts).unwrap();
        let back: FuseMountOptions = serde_json::from_str(&json).unwrap();
        assert!(back.allow_other);
        assert!(!back.allow_root);
        assert_eq!(back.max_read, Some(65536));
        assert!(back.max_write.is_none());
        assert!(back.nonempty);
        assert_eq!(back.uid, Some(0));
    }

    // --- is_fuse_available ---

    #[test]
    fn test_is_fuse_available_does_not_panic() {
        // Smoke test: just make sure the function runs without crashing
        let _available = is_fuse_available();
    }

    // --- Edge cases ---

    #[test]
    fn test_parse_proc_mounts_trailing_newline() {
        let content = "src /mnt fuse.test rw 0 0\n";
        let mounts = parse_proc_mounts(content);
        assert_eq!(mounts.len(), 1);
    }

    #[test]
    fn test_parse_proc_mounts_extra_whitespace() {
        let content = "src   /mnt   fuse.test   rw,nodev   0 0";
        let mounts = parse_proc_mounts(content);
        assert_eq!(mounts.len(), 1);
        assert_eq!(mounts[0].source, "src");
        assert_eq!(mounts[0].mountpoint, PathBuf::from("/mnt"));
    }

    #[test]
    fn test_fuse_filesystem_clone_debug() {
        let fs = FuseFilesystem::Custom("myfs".to_string());
        let cloned = fs.clone();
        assert_eq!(fs, cloned);
        let dbg = format!("{:?}", fs);
        assert!(dbg.contains("Custom"));
        assert!(dbg.contains("myfs"));
    }

    #[test]
    fn test_fuse_mount_clone() {
        let mount = FuseMount {
            mountpoint: PathBuf::from("/mnt"),
            fstype: "fuse.test".to_string(),
            source: "src".to_string(),
            options: vec!["rw".to_string()],
            pid: Some(1),
        };
        let cloned = mount.clone();
        assert_eq!(cloned.mountpoint, mount.mountpoint);
        assert_eq!(cloned.pid, mount.pid);
    }

    #[test]
    fn test_fuse_status_not_mounted() {
        let status = FuseStatus {
            is_mounted: false,
            mountpoint: PathBuf::from("/mnt/gone"),
            pid: None,
            filesystem: String::new(),
            uptime: None,
        };
        assert!(!status.is_mounted);
        assert!(status.pid.is_none());
        assert!(status.uptime.is_none());
    }
}
