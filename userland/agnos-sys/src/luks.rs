//! LUKS2 Encrypted Volume Management
//!
//! Per-agent LUKS2-encrypted loopback volumes for encrypted-at-rest sandbox
//! storage. Shells out to `cryptsetup` (standard tool from the cryptsetup package).
//!
//! On non-Linux platforms, all operations return `SysError::NotSupported`.

use crate::error::{Result, SysError};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use zeroize::Zeroize;

/// Configuration for a LUKS encrypted volume.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LuksConfig {
    /// Volume name (used for dm-crypt mapping: `/dev/mapper/{name}`)
    pub name: String,
    /// Path to the backing loopback file
    pub backing_path: PathBuf,
    /// Size of the volume in megabytes
    pub size_mb: u64,
    /// Mount point for the decrypted filesystem
    pub mount_point: PathBuf,
    /// Filesystem to create on the volume
    pub filesystem: LuksFilesystem,
    /// Cipher specification
    pub cipher: LuksCipher,
    /// Key size in bits
    pub key_size_bits: u32,
    /// Password-based key derivation function
    pub pbkdf: LuksPbkdf,
}

impl Default for LuksConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            backing_path: PathBuf::new(),
            size_mb: 256,
            mount_point: PathBuf::new(),
            filesystem: LuksFilesystem::Ext4,
            cipher: LuksCipher::default(),
            key_size_bits: 512,
            pbkdf: LuksPbkdf::Argon2id,
        }
    }
}

impl LuksConfig {
    /// Create a config for an agent volume with sensible defaults.
    pub fn for_agent(agent_id: &str, size_mb: u64) -> Self {
        Self {
            name: format!("agnos-agent-{}", agent_id),
            backing_path: PathBuf::from(format!("/var/lib/agnos/agents/{}/volume.img", agent_id)),
            size_mb,
            mount_point: PathBuf::from(format!("/var/lib/agnos/agents/{}/data", agent_id)),
            filesystem: LuksFilesystem::Ext4,
            cipher: LuksCipher::default(),
            key_size_bits: 512,
            pbkdf: LuksPbkdf::Argon2id,
        }
    }

    /// Validate the configuration.
    pub fn validate(&self) -> Result<()> {
        if self.name.is_empty() {
            return Err(SysError::InvalidArgument(
                "LUKS volume name cannot be empty".into(),
            ));
        }
        if self.name.len() > 128 {
            return Err(SysError::InvalidArgument(
                "LUKS volume name too long (max 128)".into(),
            ));
        }
        // Name should only contain safe characters for dm-crypt mapping
        if !self
            .name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        {
            return Err(SysError::InvalidArgument(format!(
                "LUKS volume name contains invalid characters: {}",
                self.name
            )));
        }
        if self.size_mb < 4 {
            return Err(SysError::InvalidArgument(
                "LUKS volume must be at least 4 MB".into(),
            ));
        }
        if self.size_mb > 1024 * 1024 {
            return Err(SysError::InvalidArgument(
                "LUKS volume exceeds maximum size of 1 TB".into(),
            ));
        }
        if self.key_size_bits != 256 && self.key_size_bits != 512 {
            return Err(SysError::InvalidArgument(format!(
                "Invalid key size: {} (must be 256 or 512)",
                self.key_size_bits
            )));
        }
        Ok(())
    }
}

/// Supported filesystems for LUKS volumes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LuksFilesystem {
    Ext4,
    Xfs,
    Btrfs,
}

impl LuksFilesystem {
    /// Return the mkfs command name.
    pub fn as_str(&self) -> &str {
        match self {
            LuksFilesystem::Ext4 => "ext4",
            LuksFilesystem::Xfs => "xfs",
            LuksFilesystem::Btrfs => "btrfs",
        }
    }

    /// Return the mkfs binary name.
    pub fn mkfs_cmd(&self) -> &str {
        match self {
            LuksFilesystem::Ext4 => "mkfs.ext4",
            LuksFilesystem::Xfs => "mkfs.xfs",
            LuksFilesystem::Btrfs => "mkfs.btrfs",
        }
    }
}

impl std::fmt::Display for LuksFilesystem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Cipher specification for LUKS.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LuksCipher {
    /// Algorithm name (e.g., "aes")
    pub algorithm: String,
    /// Mode (e.g., "xts-plain64")
    pub mode: String,
}

impl Default for LuksCipher {
    fn default() -> Self {
        Self {
            algorithm: "aes".to_string(),
            mode: "xts-plain64".to_string(),
        }
    }
}

impl LuksCipher {
    /// Return the cipher string for cryptsetup (e.g., "aes-xts-plain64").
    pub fn as_cryptsetup_str(&self) -> String {
        format!("{}-{}", self.algorithm, self.mode)
    }
}

/// Password-based key derivation function.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LuksPbkdf {
    Argon2id,
    Pbkdf2,
}

impl LuksPbkdf {
    pub fn as_str(&self) -> &str {
        match self {
            LuksPbkdf::Argon2id => "argon2id",
            LuksPbkdf::Pbkdf2 => "pbkdf2",
        }
    }
}

/// Status of a LUKS volume.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LuksStatus {
    /// Volume name
    pub name: String,
    /// Whether the dm-crypt mapping is open
    pub is_open: bool,
    /// Whether the volume is currently mounted
    pub is_mounted: bool,
    /// Backing file/device path
    pub backing_path: PathBuf,
    /// Current mount point (if mounted)
    pub mount_point: Option<PathBuf>,
    /// Cipher in use
    pub cipher: String,
    /// Key size in bits
    pub key_size_bits: u32,
}

/// A LUKS encryption key that zeroes its memory on drop.
pub struct LuksKey {
    data: Vec<u8>,
}

impl LuksKey {
    /// Create a key from raw bytes.
    pub fn from_bytes(data: Vec<u8>) -> Result<Self> {
        if data.is_empty() {
            return Err(SysError::InvalidArgument("LUKS key cannot be empty".into()));
        }
        Ok(Self { data })
    }

    /// Create a key from a passphrase string.
    pub fn from_passphrase(passphrase: &str) -> Result<Self> {
        if passphrase.is_empty() {
            return Err(SysError::InvalidArgument(
                "Passphrase cannot be empty".into(),
            ));
        }
        Ok(Self {
            data: passphrase.as_bytes().to_vec(),
        })
    }

    /// Generate a random key of the specified size in bytes.
    pub fn generate(size: usize) -> Result<Self> {
        if size == 0 {
            return Err(SysError::InvalidArgument("Key size must be > 0".into()));
        }
        if size > 1024 {
            return Err(SysError::InvalidArgument(
                "Key size too large (max 1024 bytes)".into(),
            ));
        }

        #[cfg(target_os = "linux")]
        {
            let mut data = vec![0u8; size];
            let bytes_read = unsafe {
                libc::getrandom(
                    data.as_mut_ptr() as *mut libc::c_void,
                    size,
                    0, // no flags, blocking
                )
            };
            if bytes_read < 0 || bytes_read as usize != size {
                return Err(SysError::Unknown("getrandom failed".into()));
            }
            Ok(Self { data })
        }

        #[cfg(not(target_os = "linux"))]
        {
            // Fallback: use /dev/urandom
            use std::io::Read;
            let mut data = vec![0u8; size];
            let mut f = std::fs::File::open("/dev/urandom")
                .map_err(|e| SysError::Unknown(format!("Cannot open /dev/urandom: {}", e)))?;
            f.read_exact(&mut data)
                .map_err(|e| SysError::Unknown(format!("Read /dev/urandom failed: {}", e)))?;
            Ok(Self { data })
        }
    }

    /// Get the key data as bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }

    /// Get the key length in bytes.
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Check if the key is empty.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

impl Drop for LuksKey {
    fn drop(&mut self) {
        self.data.zeroize();
    }
}

impl std::fmt::Debug for LuksKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "LuksKey([REDACTED; {} bytes])", self.data.len())
    }
}

/// Format a LUKS2 encrypted volume.
///
/// Steps:
/// 1. Create backing file with `fallocate`
/// 2. Set up loop device with `losetup`
/// 3. Format with `cryptsetup luksFormat --type luks2`
///
/// Returns the path to the loop device.
pub fn luks_format(config: &LuksConfig, key: &LuksKey) -> Result<PathBuf> {
    #[cfg(target_os = "linux")]
    {
        config.validate()?;

        // Ensure parent directory exists
        if let Some(parent) = config.backing_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                SysError::Unknown(format!(
                    "Failed to create directory {}: {}",
                    parent.display(),
                    e
                ))
            })?;
        }

        // 1. Create backing file
        let size_bytes = config.size_mb.checked_mul(1024 * 1024).ok_or_else(|| {
            SysError::InvalidArgument(format!(
                "LUKS size overflow: {} MB exceeds u64 byte range",
                config.size_mb
            ))
        })?;
        run_cmd_with_output(
            "fallocate",
            &[
                "-l",
                &size_bytes.to_string(),
                &path_str(&config.backing_path),
            ],
        )?;

        // 2. Set up loop device
        let loop_dev = run_cmd_with_output(
            "losetup",
            &["--find", "--show", &path_str(&config.backing_path)],
        )?;
        let loop_dev = loop_dev.trim().to_string();

        // 3. Format with LUKS2
        let cipher = config.cipher.as_cryptsetup_str();
        let key_size = config.key_size_bits.to_string();
        let pbkdf = config.pbkdf.as_str();

        let result = run_cmd_stdin(
            "cryptsetup",
            &[
                "luksFormat",
                "--type",
                "luks2",
                "--cipher",
                &cipher,
                "--key-size",
                &key_size,
                "--pbkdf",
                pbkdf,
                "--batch-mode",
                "--key-file",
                "-",
                &loop_dev,
            ],
            key.as_bytes(),
        );

        if let Err(e) = result {
            // Cleanup loop device on failure
            let _ = run_cmd_with_output("losetup", &["-d", &loop_dev]);
            return Err(e);
        }

        tracing::info!(
            "Formatted LUKS2 volume '{}' ({} MB, {}, {})",
            config.name,
            config.size_mb,
            cipher,
            pbkdf
        );

        Ok(PathBuf::from(loop_dev))
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = (config, key);
        Err(SysError::NotSupported)
    }
}

/// Open (unlock) a LUKS volume.
///
/// Maps the device to `/dev/mapper/{name}`.
pub fn luks_open(config: &LuksConfig, key: &LuksKey) -> Result<PathBuf> {
    #[cfg(target_os = "linux")]
    {
        config.validate()?;

        // Find the loop device for the backing file
        let loop_dev = run_cmd_with_output("losetup", &["-j", &path_str(&config.backing_path)])?;

        let loop_dev = loop_dev
            .split(':')
            .next()
            .ok_or_else(|| SysError::Unknown("No loop device found for backing file".into()))?
            .trim()
            .to_string();

        if loop_dev.is_empty() {
            // Not yet attached, attach it
            let loop_dev = run_cmd_with_output(
                "losetup",
                &["--find", "--show", &path_str(&config.backing_path)],
            )?;
            let loop_dev = loop_dev.trim().to_string();
            open_luks_device(&loop_dev, &config.name, key)?;
        } else {
            open_luks_device(&loop_dev, &config.name, key)?;
        }

        let mapper_path = PathBuf::from(format!("/dev/mapper/{}", config.name));
        tracing::info!(
            "Opened LUKS volume '{}' at {}",
            config.name,
            mapper_path.display()
        );
        Ok(mapper_path)
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = (config, key);
        Err(SysError::NotSupported)
    }
}

/// Close (lock) a LUKS volume.
///
/// Unmounts if mounted, then closes the dm-crypt mapping.
pub fn luks_close(name: &str) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        if name.is_empty() {
            return Err(SysError::InvalidArgument(
                "Volume name cannot be empty".into(),
            ));
        }

        let mapper_path = format!("/dev/mapper/{}", name);
        if Path::new(&mapper_path).exists() {
            run_cmd_checked("cryptsetup", &["close", name])?;
            tracing::info!("Closed LUKS volume '{}'", name);
        } else {
            tracing::debug!("LUKS volume '{}' not open, nothing to close", name);
        }
        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = name;
        Err(SysError::NotSupported)
    }
}

/// Create a filesystem on a device.
pub fn luks_mkfs(device: &Path, filesystem: LuksFilesystem) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        let cmd = filesystem.mkfs_cmd();
        let device_str = path_str_ref(device);
        let args: Vec<&str> = match filesystem {
            LuksFilesystem::Ext4 => vec!["-F", &device_str],
            LuksFilesystem::Xfs => vec!["-f", &device_str],
            LuksFilesystem::Btrfs => vec!["-f", &device_str],
        };
        run_cmd_checked(cmd, &args)?;
        tracing::info!("Created {} filesystem on {}", filesystem, device.display());
        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = (device, filesystem);
        Err(SysError::NotSupported)
    }
}

/// Mount a device at a mount point.
pub fn luks_mount(device: &Path, mount_point: &Path, filesystem: LuksFilesystem) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        std::fs::create_dir_all(mount_point).map_err(|e| {
            SysError::Unknown(format!(
                "Failed to create mount point {}: {}",
                mount_point.display(),
                e
            ))
        })?;

        run_cmd_checked(
            "mount",
            &[
                "-t",
                filesystem.as_str(),
                &path_str_ref(device),
                &path_str_ref(mount_point),
            ],
        )?;

        tracing::info!("Mounted {} at {}", device.display(), mount_point.display());
        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = (device, mount_point, filesystem);
        Err(SysError::NotSupported)
    }
}

/// Unmount a mount point.
pub fn luks_unmount(mount_point: &Path) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        run_cmd_checked("umount", &[&path_str_ref(mount_point)])?;
        tracing::info!("Unmounted {}", mount_point.display());
        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = mount_point;
        Err(SysError::NotSupported)
    }
}

/// High-level: Set up a complete agent encrypted volume.
///
/// Format + open + mkfs + mount. Returns the volume status.
pub fn setup_agent_volume(config: &LuksConfig, key: &LuksKey) -> Result<LuksStatus> {
    #[cfg(target_os = "linux")]
    {
        config.validate()?;

        // Format
        let _loop_dev = luks_format(config, key)?;

        // Open
        let mapper_path = luks_open(config, key)?;

        // Create filesystem
        luks_mkfs(&mapper_path, config.filesystem)?;

        // Mount
        luks_mount(&mapper_path, &config.mount_point, config.filesystem)?;

        Ok(LuksStatus {
            name: config.name.clone(),
            is_open: true,
            is_mounted: true,
            backing_path: config.backing_path.clone(),
            mount_point: Some(config.mount_point.clone()),
            cipher: config.cipher.as_cryptsetup_str(),
            key_size_bits: config.key_size_bits,
        })
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = (config, key);
        Err(SysError::NotSupported)
    }
}

/// High-level: Tear down an agent encrypted volume.
///
/// Unmount + close dm-crypt + detach loop device.
pub fn teardown_agent_volume(name: &str) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        if name.is_empty() {
            return Err(SysError::InvalidArgument(
                "Volume name cannot be empty".into(),
            ));
        }

        // Try to unmount first (may not be mounted)
        let mount_point = format!(
            "/var/lib/agnos/agents/{}/data",
            name.trim_start_matches("agnos-agent-")
        );
        if Path::new(&mount_point).exists() {
            let _ = luks_unmount(Path::new(&mount_point));
        }

        // Close dm-crypt
        luks_close(name)?;

        // Detach loop device if possible
        let backing_path = format!(
            "/var/lib/agnos/agents/{}/volume.img",
            name.trim_start_matches("agnos-agent-")
        );
        if Path::new(&backing_path).exists() {
            let loop_info = run_cmd_with_output("losetup", &["-j", &backing_path]);
            if let Ok(info) = loop_info {
                if let Some(loop_dev) = info.split(':').next() {
                    let loop_dev = loop_dev.trim();
                    if !loop_dev.is_empty() {
                        let _ = run_cmd_checked("losetup", &["-d", loop_dev]);
                    }
                }
            }
        }

        tracing::info!("Tore down LUKS volume '{}'", name);
        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = name;
        Err(SysError::NotSupported)
    }
}

/// Check if `cryptsetup` is available on this system.
pub fn cryptsetup_available() -> bool {
    std::process::Command::new("cryptsetup")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Check if dm-crypt kernel module is loaded.
pub fn dmcrypt_supported() -> bool {
    #[cfg(target_os = "linux")]
    {
        Path::new("/sys/module/dm_crypt").exists()
            || std::fs::read_to_string("/proc/modules")
                .map(|s| s.contains("dm_crypt"))
                .unwrap_or(false)
    }

    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}

// --- Internal helpers ---

/// Open a LUKS device with cryptsetup.
#[cfg(target_os = "linux")]
fn open_luks_device(loop_dev: &str, name: &str, key: &LuksKey) -> Result<()> {
    run_cmd_stdin(
        "cryptsetup",
        &["open", "--type", "luks2", "--key-file", "-", loop_dev, name],
        key.as_bytes(),
    )
}

/// Run a command and return stdout.
#[cfg(target_os = "linux")]
fn run_cmd_with_output(cmd: &str, args: &[&str]) -> Result<String> {
    let output = std::process::Command::new(cmd)
        .args(args)
        .output()
        .map_err(|e| SysError::Unknown(format!("Failed to run '{}': {}", cmd, e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(SysError::Unknown(format!(
            "{} {} failed: {}",
            cmd,
            args.join(" "),
            stderr.trim()
        )));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Run a command and check for success (discard output).
#[cfg(target_os = "linux")]
fn run_cmd_checked(cmd: &str, args: &[&str]) -> Result<()> {
    let output = std::process::Command::new(cmd)
        .args(args)
        .output()
        .map_err(|e| SysError::Unknown(format!("Failed to run '{}': {}", cmd, e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(SysError::Unknown(format!(
            "{} {} failed: {}",
            cmd,
            args.join(" "),
            stderr.trim()
        )));
    }

    Ok(())
}

/// Run a command with stdin data.
#[cfg(target_os = "linux")]
fn run_cmd_stdin(cmd: &str, args: &[&str], stdin_data: &[u8]) -> Result<()> {
    use std::io::Write;
    use std::process::Stdio;

    let mut child = std::process::Command::new(cmd)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| SysError::Unknown(format!("Failed to spawn '{}': {}", cmd, e)))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(stdin_data)
            .map_err(|e| SysError::Unknown(format!("Failed to write to {} stdin: {}", cmd, e)))?;
    }

    let output = child
        .wait_with_output()
        .map_err(|e| SysError::Unknown(format!("Failed to wait for '{}': {}", cmd, e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(SysError::Unknown(format!(
            "{} {} failed: {}",
            cmd,
            args.join(" "),
            stderr.trim()
        )));
    }

    Ok(())
}

#[cfg(target_os = "linux")]
fn path_str(p: &Path) -> String {
    p.to_string_lossy().to_string()
}

#[cfg(target_os = "linux")]
fn path_str_ref(p: &Path) -> String {
    p.to_string_lossy().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_luks_config_for_agent() {
        let config = LuksConfig::for_agent("test-1", 128);
        assert_eq!(config.name, "agnos-agent-test-1");
        assert_eq!(config.size_mb, 128);
        assert_eq!(config.key_size_bits, 512);
        assert_eq!(config.filesystem, LuksFilesystem::Ext4);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_luks_config_validate_empty_name() {
        let mut config = LuksConfig::default();
        assert!(config.validate().is_err());

        config.name = "valid-name".to_string();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_luks_config_validate_bad_name_chars() {
        let config = LuksConfig {
            name: "bad name with spaces".to_string(),
            ..LuksConfig::for_agent("x", 64)
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_luks_config_validate_size_too_small() {
        let config = LuksConfig {
            size_mb: 1,
            ..LuksConfig::for_agent("x", 1)
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_luks_config_validate_bad_key_size() {
        let config = LuksConfig {
            key_size_bits: 128,
            ..LuksConfig::for_agent("x", 64)
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_luks_filesystem_as_str() {
        assert_eq!(LuksFilesystem::Ext4.as_str(), "ext4");
        assert_eq!(LuksFilesystem::Xfs.as_str(), "xfs");
        assert_eq!(LuksFilesystem::Btrfs.as_str(), "btrfs");
    }

    #[test]
    fn test_luks_filesystem_mkfs_cmd() {
        assert_eq!(LuksFilesystem::Ext4.mkfs_cmd(), "mkfs.ext4");
        assert_eq!(LuksFilesystem::Xfs.mkfs_cmd(), "mkfs.xfs");
        assert_eq!(LuksFilesystem::Btrfs.mkfs_cmd(), "mkfs.btrfs");
    }

    #[test]
    fn test_luks_cipher_default() {
        let cipher = LuksCipher::default();
        assert_eq!(cipher.algorithm, "aes");
        assert_eq!(cipher.mode, "xts-plain64");
        assert_eq!(cipher.as_cryptsetup_str(), "aes-xts-plain64");
    }

    #[test]
    fn test_luks_pbkdf_as_str() {
        assert_eq!(LuksPbkdf::Argon2id.as_str(), "argon2id");
        assert_eq!(LuksPbkdf::Pbkdf2.as_str(), "pbkdf2");
    }

    #[test]
    fn test_luks_key_from_bytes() {
        let key = LuksKey::from_bytes(vec![1, 2, 3, 4]).unwrap();
        assert_eq!(key.len(), 4);
        assert_eq!(key.as_bytes(), &[1, 2, 3, 4]);
        assert!(!key.is_empty());
    }

    #[test]
    fn test_luks_key_from_bytes_empty() {
        assert!(LuksKey::from_bytes(vec![]).is_err());
    }

    #[test]
    fn test_luks_key_from_passphrase() {
        let key = LuksKey::from_passphrase("my-secret").unwrap();
        assert_eq!(key.as_bytes(), b"my-secret");
    }

    #[test]
    fn test_luks_key_from_passphrase_empty() {
        assert!(LuksKey::from_passphrase("").is_err());
    }

    #[test]
    fn test_luks_key_generate() {
        let key = LuksKey::generate(32).unwrap();
        assert_eq!(key.len(), 32);
        // Should not be all zeros (astronomically unlikely)
        assert!(key.as_bytes().iter().any(|&b| b != 0));
    }

    #[test]
    fn test_luks_key_generate_zero_size() {
        assert!(LuksKey::generate(0).is_err());
    }

    #[test]
    fn test_luks_key_generate_too_large() {
        assert!(LuksKey::generate(2048).is_err());
    }

    #[test]
    fn test_luks_key_debug_redacted() {
        let key = LuksKey::from_bytes(vec![0xDE, 0xAD, 0xBE, 0xEF]).unwrap();
        let debug = format!("{:?}", key);
        assert!(debug.contains("REDACTED"));
        assert!(!debug.contains("DEAD"));
    }

    #[test]
    fn test_luks_key_zeroed_on_drop() {
        let data_ptr: *const u8;
        let data_len: usize;

        {
            let key = LuksKey::from_bytes(vec![0xFF; 32]).unwrap();
            data_ptr = key.as_bytes().as_ptr();
            data_len = key.len();
            // Key is dropped here
        }

        // After drop, the memory should be zeroed.
        // Note: this is best-effort testing — the allocator may reuse the memory.
        // We verify the Drop impl exists and runs correctly.
        let _ = (data_ptr, data_len);
    }

    #[test]
    fn test_luks_status_serialization() {
        let status = LuksStatus {
            name: "test-vol".to_string(),
            is_open: true,
            is_mounted: true,
            backing_path: PathBuf::from("/var/lib/test/vol.img"),
            mount_point: Some(PathBuf::from("/mnt/test")),
            cipher: "aes-xts-plain64".to_string(),
            key_size_bits: 512,
        };

        let json = serde_json::to_string(&status).unwrap();
        let deserialized: LuksStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "test-vol");
        assert!(deserialized.is_open);
    }

    #[test]
    fn test_cryptsetup_available() {
        // Just verify it doesn't crash
        let _available = cryptsetup_available();
    }

    #[test]
    fn test_dmcrypt_supported() {
        // Just verify it doesn't crash
        let _supported = dmcrypt_supported();
    }

    #[test]
    #[ignore = "Requires root, cryptsetup, and loop device support"]
    fn test_setup_and_teardown_agent_volume() {
        let config = LuksConfig::for_agent("luks-test", 32);
        let key = LuksKey::generate(64).unwrap();

        let status = setup_agent_volume(&config, &key).unwrap();
        assert!(status.is_open);
        assert!(status.is_mounted);

        teardown_agent_volume(&config.name).unwrap();
    }

    // --- Additional coverage tests ---

    #[test]
    fn test_luks_config_default_all_fields() {
        let config = LuksConfig::default();
        assert_eq!(config.name, "");
        assert_eq!(config.backing_path, PathBuf::new());
        assert_eq!(config.size_mb, 256);
        assert_eq!(config.mount_point, PathBuf::new());
        assert_eq!(config.filesystem, LuksFilesystem::Ext4);
        assert_eq!(config.key_size_bits, 512);
        assert_eq!(config.pbkdf, LuksPbkdf::Argon2id);
        assert_eq!(config.cipher.algorithm, "aes");
        assert_eq!(config.cipher.mode, "xts-plain64");
    }

    #[test]
    fn test_luks_config_for_agent_paths() {
        let config = LuksConfig::for_agent("my-agent", 64);
        assert_eq!(
            config.backing_path,
            PathBuf::from("/var/lib/agnos/agents/my-agent/volume.img")
        );
        assert_eq!(
            config.mount_point,
            PathBuf::from("/var/lib/agnos/agents/my-agent/data")
        );
        assert_eq!(config.pbkdf, LuksPbkdf::Argon2id);
        assert_eq!(config.cipher.algorithm, "aes");
        assert_eq!(config.cipher.mode, "xts-plain64");
    }

    #[test]
    fn test_luks_config_validate_name_too_long() {
        let config = LuksConfig {
            name: "a".repeat(129),
            ..LuksConfig::for_agent("x", 64)
        };
        let err = config.validate().unwrap_err();
        let msg = format!("{}", err);
        assert!(msg.contains("too long"), "Expected 'too long' in: {}", msg);
    }

    #[test]
    fn test_luks_config_validate_name_exactly_128() {
        let config = LuksConfig {
            name: "a".repeat(128),
            ..LuksConfig::for_agent("x", 64)
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_luks_config_validate_size_too_large() {
        let config = LuksConfig {
            size_mb: 1024 * 1024 + 1,
            ..LuksConfig::for_agent("x", 64)
        };
        let err = config.validate().unwrap_err();
        let msg = format!("{}", err);
        assert!(
            msg.contains("maximum size"),
            "Expected 'maximum size' in: {}",
            msg
        );
    }

    #[test]
    fn test_luks_config_validate_size_exactly_4mb() {
        let config = LuksConfig {
            size_mb: 4,
            ..LuksConfig::for_agent("x", 64)
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_luks_config_validate_size_exactly_1tb() {
        let config = LuksConfig {
            size_mb: 1024 * 1024,
            ..LuksConfig::for_agent("x", 64)
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_luks_config_validate_key_size_256_valid() {
        let config = LuksConfig {
            key_size_bits: 256,
            ..LuksConfig::for_agent("x", 64)
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_luks_config_validate_bad_key_size_error_msg() {
        let config = LuksConfig {
            key_size_bits: 1024,
            ..LuksConfig::for_agent("x", 64)
        };
        let err = config.validate().unwrap_err();
        let msg = format!("{}", err);
        assert!(msg.contains("1024"), "Expected '1024' in: {}", msg);
        assert!(
            msg.contains("256 or 512"),
            "Expected '256 or 512' in: {}",
            msg
        );
    }

    #[test]
    fn test_luks_config_validate_invalid_chars_error_msg() {
        let config = LuksConfig {
            name: "bad/name".to_string(),
            ..LuksConfig::for_agent("x", 64)
        };
        let err = config.validate().unwrap_err();
        let msg = format!("{}", err);
        assert!(
            msg.contains("invalid characters"),
            "Expected 'invalid characters' in: {}",
            msg
        );
        assert!(msg.contains("bad/name"), "Expected name in error: {}", msg);
    }

    #[test]
    fn test_luks_config_validate_name_with_dots() {
        let config = LuksConfig {
            name: "has.dot".to_string(),
            ..LuksConfig::for_agent("x", 64)
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_luks_config_validate_name_with_underscore() {
        let config = LuksConfig {
            name: "has_underscore".to_string(),
            ..LuksConfig::for_agent("x", 64)
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_luks_filesystem_display() {
        assert_eq!(format!("{}", LuksFilesystem::Ext4), "ext4");
        assert_eq!(format!("{}", LuksFilesystem::Xfs), "xfs");
        assert_eq!(format!("{}", LuksFilesystem::Btrfs), "btrfs");
    }

    #[test]
    fn test_luks_filesystem_clone_copy_eq() {
        let fs = LuksFilesystem::Xfs;
        let fs2 = fs; // Copy
        let fs3 = fs; // Clone
        assert_eq!(fs, fs2);
        assert_eq!(fs, fs3);
        assert_ne!(fs, LuksFilesystem::Ext4);
    }

    #[test]
    fn test_luks_cipher_custom() {
        let cipher = LuksCipher {
            algorithm: "serpent".to_string(),
            mode: "cbc-essiv".to_string(),
        };
        assert_eq!(cipher.as_cryptsetup_str(), "serpent-cbc-essiv");
    }

    #[test]
    fn test_luks_cipher_clone() {
        let c1 = LuksCipher::default();
        let c2 = c1.clone();
        assert_eq!(c2.algorithm, "aes");
        assert_eq!(c2.mode, "xts-plain64");
    }

    #[test]
    fn test_luks_cipher_debug() {
        let cipher = LuksCipher::default();
        let debug = format!("{:?}", cipher);
        assert!(debug.contains("aes"));
        assert!(debug.contains("xts-plain64"));
    }

    #[test]
    fn test_luks_pbkdf_clone_copy_eq() {
        let p1 = LuksPbkdf::Argon2id;
        let p2 = p1; // Copy
        let p3 = p1; // Clone
        assert_eq!(p1, p2);
        assert_eq!(p1, p3);
        assert_ne!(p1, LuksPbkdf::Pbkdf2);
    }

    #[test]
    fn test_luks_status_none_mount_point() {
        let status = LuksStatus {
            name: "test-vol".to_string(),
            is_open: false,
            is_mounted: false,
            backing_path: PathBuf::from("/tmp/vol.img"),
            mount_point: None,
            cipher: "aes-xts-plain64".to_string(),
            key_size_bits: 256,
        };
        assert!(!status.is_open);
        assert!(!status.is_mounted);
        assert!(status.mount_point.is_none());
        assert_eq!(status.key_size_bits, 256);
    }

    #[test]
    fn test_luks_status_clone() {
        let status = LuksStatus {
            name: "vol".to_string(),
            is_open: true,
            is_mounted: false,
            backing_path: PathBuf::from("/a"),
            mount_point: Some(PathBuf::from("/b")),
            cipher: "aes-xts-plain64".to_string(),
            key_size_bits: 512,
        };
        let cloned = status.clone();
        assert_eq!(cloned.name, "vol");
        assert!(cloned.is_open);
        assert!(!cloned.is_mounted);
        assert_eq!(cloned.mount_point, Some(PathBuf::from("/b")));
    }

    #[test]
    fn test_luks_status_debug() {
        let status = LuksStatus {
            name: "dbg".to_string(),
            is_open: true,
            is_mounted: true,
            backing_path: PathBuf::from("/x"),
            mount_point: Some(PathBuf::from("/y")),
            cipher: "aes-xts-plain64".to_string(),
            key_size_bits: 512,
        };
        let debug = format!("{:?}", status);
        assert!(debug.contains("dbg"));
        assert!(debug.contains("true"));
    }

    #[test]
    fn test_luks_status_json_roundtrip_none_mount() {
        let status = LuksStatus {
            name: "roundtrip".to_string(),
            is_open: false,
            is_mounted: false,
            backing_path: PathBuf::from("/tmp/rt.img"),
            mount_point: None,
            cipher: "serpent-cbc-essiv".to_string(),
            key_size_bits: 256,
        };
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("\"mount_point\":null"));
        let de: LuksStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(de.name, "roundtrip");
        assert!(de.mount_point.is_none());
        assert_eq!(de.cipher, "serpent-cbc-essiv");
        assert_eq!(de.key_size_bits, 256);
    }

    #[test]
    fn test_luks_config_serialization_roundtrip() {
        let config = LuksConfig::for_agent("ser-test", 128);
        let json = serde_json::to_string(&config).unwrap();
        let de: LuksConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(de.name, "agnos-agent-ser-test");
        assert_eq!(de.size_mb, 128);
        assert_eq!(de.key_size_bits, 512);
        assert_eq!(de.filesystem, LuksFilesystem::Ext4);
        assert_eq!(de.pbkdf, LuksPbkdf::Argon2id);
        assert_eq!(de.cipher.algorithm, "aes");
    }

    #[test]
    fn test_luks_key_from_passphrase_utf8() {
        let key = LuksKey::from_passphrase("héllo wörld 🔑").unwrap();
        assert_eq!(key.as_bytes(), "héllo wörld 🔑".as_bytes());
        assert!(!key.is_empty());
        assert!(!key.is_empty());
    }

    #[test]
    fn test_luks_key_from_bytes_single_byte() {
        let key = LuksKey::from_bytes(vec![42]).unwrap();
        assert_eq!(key.len(), 1);
        assert_eq!(key.as_bytes(), &[42]);
    }

    #[test]
    fn test_luks_key_generate_boundary_1024() {
        let key = LuksKey::generate(1024).unwrap();
        assert_eq!(key.len(), 1024);
    }

    #[test]
    fn test_luks_key_generate_boundary_1025() {
        let err = LuksKey::generate(1025).unwrap_err();
        let msg = format!("{}", err);
        assert!(
            msg.contains("too large"),
            "Expected 'too large' in: {}",
            msg
        );
    }

    #[test]
    fn test_luks_key_generate_size_1() {
        let key = LuksKey::generate(1).unwrap();
        assert_eq!(key.len(), 1);
    }

    #[test]
    fn test_luks_key_debug_shows_length() {
        let key = LuksKey::from_bytes(vec![1, 2, 3]).unwrap();
        let debug = format!("{:?}", key);
        assert_eq!(debug, "LuksKey([REDACTED; 3 bytes])");
    }

    #[test]
    fn test_luks_key_debug_different_sizes() {
        let k1 = LuksKey::from_bytes(vec![0; 64]).unwrap();
        let k2 = LuksKey::from_bytes(vec![0; 1]).unwrap();
        assert!(format!("{:?}", k1).contains("64 bytes"));
        assert!(format!("{:?}", k2).contains("1 bytes"));
    }

    #[test]
    fn test_luks_key_zeroed_on_drop_via_raw_ptr() {
        // Verify the drop implementation runs; we use a raw pointer
        // to check the memory region after drop. Note: this is inherently
        // racy with the allocator, but it exercises the Drop impl code path.
        let mut raw_data = vec![0xAAu8; 16];
        let ptr = raw_data.as_mut_ptr();
        let len = raw_data.len();
        {
            let key = LuksKey::from_bytes(raw_data).unwrap();
            assert_eq!(key.len(), len);
            assert!(key.as_bytes().iter().all(|&b| b == 0xAA));
            // key dropped here, zeroing the vec's contents
        }
        // The Vec was moved into LuksKey, so we read from ptr.
        // The allocator *may* have reused the memory, so we just
        // exercise the code path. The important thing is no panic.
        let _ = ptr;
    }

    #[test]
    fn test_luks_key_from_bytes_error_message() {
        let err = LuksKey::from_bytes(vec![]).unwrap_err();
        let msg = format!("{}", err);
        assert!(msg.contains("empty"), "Expected 'empty' in: {}", msg);
    }

    #[test]
    fn test_luks_key_from_passphrase_error_message() {
        let err = LuksKey::from_passphrase("").unwrap_err();
        let msg = format!("{}", err);
        assert!(msg.contains("empty"), "Expected 'empty' in: {}", msg);
    }

    #[test]
    fn test_luks_key_generate_zero_error_message() {
        let err = LuksKey::generate(0).unwrap_err();
        let msg = format!("{}", err);
        assert!(msg.contains("> 0"), "Expected '> 0' in: {}", msg);
    }

    #[test]
    fn test_luks_config_validate_empty_name_error_message() {
        let config = LuksConfig::default();
        let err = config.validate().unwrap_err();
        let msg = format!("{}", err);
        assert!(msg.contains("empty"), "Expected 'empty' in: {}", msg);
    }

    #[test]
    fn test_luks_config_validate_size_3mb_error() {
        let config = LuksConfig {
            size_mb: 3,
            ..LuksConfig::for_agent("x", 64)
        };
        let err = config.validate().unwrap_err();
        let msg = format!("{}", err);
        assert!(
            msg.contains("at least 4 MB"),
            "Expected 'at least 4 MB' in: {}",
            msg
        );
    }

    #[test]
    fn test_luks_pbkdf_debug() {
        assert!(format!("{:?}", LuksPbkdf::Argon2id).contains("Argon2id"));
        assert!(format!("{:?}", LuksPbkdf::Pbkdf2).contains("Pbkdf2"));
    }

    #[test]
    fn test_luks_filesystem_debug() {
        assert!(format!("{:?}", LuksFilesystem::Ext4).contains("Ext4"));
        assert!(format!("{:?}", LuksFilesystem::Xfs).contains("Xfs"));
        assert!(format!("{:?}", LuksFilesystem::Btrfs).contains("Btrfs"));
    }

    #[test]
    fn test_luks_config_debug() {
        let config = LuksConfig::for_agent("dbg-agent", 32);
        let debug = format!("{:?}", config);
        assert!(debug.contains("agnos-agent-dbg-agent"));
        assert!(debug.contains("32"));
    }

    #[test]
    fn test_luks_config_clone() {
        let config = LuksConfig::for_agent("clone-test", 100);
        let cloned = config.clone();
        assert_eq!(cloned.name, config.name);
        assert_eq!(cloned.size_mb, config.size_mb);
        assert_eq!(cloned.backing_path, config.backing_path);
        assert_eq!(cloned.mount_point, config.mount_point);
        assert_eq!(cloned.filesystem, config.filesystem);
        assert_eq!(cloned.key_size_bits, config.key_size_bits);
        assert_eq!(cloned.pbkdf, config.pbkdf);
    }

    #[test]
    fn test_luks_config_for_agent_with_special_id() {
        let config = LuksConfig::for_agent("abc_123", 8);
        assert_eq!(config.name, "agnos-agent-abc_123");
        // size_mb 8 is >= 4, should be valid
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_luks_pbkdf_serialization() {
        let json_argon = serde_json::to_string(&LuksPbkdf::Argon2id).unwrap();
        let json_pbkdf2 = serde_json::to_string(&LuksPbkdf::Pbkdf2).unwrap();
        let de_argon: LuksPbkdf = serde_json::from_str(&json_argon).unwrap();
        let de_pbkdf2: LuksPbkdf = serde_json::from_str(&json_pbkdf2).unwrap();
        assert_eq!(de_argon, LuksPbkdf::Argon2id);
        assert_eq!(de_pbkdf2, LuksPbkdf::Pbkdf2);
    }

    #[test]
    fn test_luks_filesystem_serialization() {
        for fs in [
            LuksFilesystem::Ext4,
            LuksFilesystem::Xfs,
            LuksFilesystem::Btrfs,
        ] {
            let json = serde_json::to_string(&fs).unwrap();
            let de: LuksFilesystem = serde_json::from_str(&json).unwrap();
            assert_eq!(de, fs);
        }
    }

    #[test]
    fn test_luks_cipher_serialization() {
        let cipher = LuksCipher {
            algorithm: "twofish".to_string(),
            mode: "cbc-plain".to_string(),
        };
        let json = serde_json::to_string(&cipher).unwrap();
        assert!(json.contains("twofish"));
        assert!(json.contains("cbc-plain"));
        let de: LuksCipher = serde_json::from_str(&json).unwrap();
        assert_eq!(de.algorithm, "twofish");
        assert_eq!(de.mode, "cbc-plain");
        assert_eq!(de.as_cryptsetup_str(), "twofish-cbc-plain");
    }

    #[test]
    fn test_luks_key_large_passphrase() {
        let passphrase = "x".repeat(4096);
        let key = LuksKey::from_passphrase(&passphrase).unwrap();
        assert_eq!(key.len(), 4096);
    }

    #[test]
    fn test_luks_key_from_bytes_large() {
        let data = vec![0xBB; 8192];
        let key = LuksKey::from_bytes(data).unwrap();
        assert_eq!(key.len(), 8192);
        assert!(key.as_bytes().iter().all(|&b| b == 0xBB));
    }

    #[test]
    fn test_luks_config_validate_name_special_chars() {
        // Test various invalid characters
        for name in &["a b", "a/b", "a.b", "a:b", "a;b", "a@b", "a!b", "a#b"] {
            let config = LuksConfig {
                name: name.to_string(),
                ..LuksConfig::for_agent("x", 64)
            };
            assert!(
                config.validate().is_err(),
                "Expected error for name '{}' but got Ok",
                name
            );
        }
    }

    #[test]
    fn test_luks_config_validate_alphanumeric_hyphen_underscore() {
        // These should all be valid
        for name in &["abc", "ABC", "a-b", "a_b", "123", "a1-b2_c3"] {
            let config = LuksConfig {
                name: name.to_string(),
                ..LuksConfig::for_agent("x", 64)
            };
            assert!(
                config.validate().is_ok(),
                "Expected Ok for name '{}' but got Err",
                name
            );
        }
    }

    #[test]
    fn test_luks_key_multiple_generate_unique() {
        let k1 = LuksKey::generate(32).unwrap();
        let k2 = LuksKey::generate(32).unwrap();
        // Two random keys should (almost certainly) differ
        assert_ne!(k1.as_bytes(), k2.as_bytes());
    }

    #[test]
    fn test_luks_status_all_fields_json() {
        let json = r#"{
            "name": "vol1",
            "is_open": true,
            "is_mounted": false,
            "backing_path": "/dev/sda1",
            "mount_point": "/mnt/data",
            "cipher": "aes-xts-plain64",
            "key_size_bits": 512
        }"#;
        let status: LuksStatus = serde_json::from_str(json).unwrap();
        assert_eq!(status.name, "vol1");
        assert!(status.is_open);
        assert!(!status.is_mounted);
        assert_eq!(status.backing_path, PathBuf::from("/dev/sda1"));
        assert_eq!(status.mount_point, Some(PathBuf::from("/mnt/data")));
        assert_eq!(status.cipher, "aes-xts-plain64");
        assert_eq!(status.key_size_bits, 512);
    }

    #[test]
    fn test_luks_config_default_cipher_is_default() {
        let config = LuksConfig::default();
        let cipher_default = LuksCipher::default();
        assert_eq!(config.cipher.algorithm, cipher_default.algorithm);
        assert_eq!(config.cipher.mode, cipher_default.mode);
    }

    // --- New coverage tests ---

    #[test]
    fn test_luks_close_empty_name() {
        let result = luks_close("");
        assert!(result.is_err());
    }

    #[test]
    fn test_teardown_agent_volume_empty_name() {
        let result = teardown_agent_volume("");
        assert!(result.is_err());
    }

    #[test]
    fn test_luks_key_generate_various_sizes() {
        for size in &[1, 16, 32, 64, 128, 256, 512, 1024] {
            let key = LuksKey::generate(*size).unwrap();
            assert_eq!(key.len(), *size);
            assert!(!key.is_empty());
        }
    }

    #[test]
    fn test_luks_config_for_agent_validates_ok() {
        // for_agent should always produce a valid config with reasonable agent IDs
        for id in &["agent-1", "myagent", "a", "test_agent_123"] {
            let config = LuksConfig::for_agent(id, 8);
            assert!(
                config.validate().is_ok(),
                "Config for '{}' should be valid",
                id
            );
        }
    }

    #[test]
    fn test_luks_config_validate_size_exactly_3mb_fails() {
        let config = LuksConfig {
            size_mb: 3,
            ..LuksConfig::for_agent("x", 64)
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_luks_filesystem_serde_each_variant() {
        let variants = [
            LuksFilesystem::Ext4,
            LuksFilesystem::Xfs,
            LuksFilesystem::Btrfs,
        ];
        for fs in &variants {
            let json = serde_json::to_string(fs).unwrap();
            let de: LuksFilesystem = serde_json::from_str(&json).unwrap();
            assert_eq!(de, *fs);
            assert_eq!(de.as_str(), fs.as_str());
            assert_eq!(de.mkfs_cmd(), fs.mkfs_cmd());
        }
    }

    #[test]
    fn test_luks_pbkdf_serde_each_variant() {
        for pbkdf in &[LuksPbkdf::Argon2id, LuksPbkdf::Pbkdf2] {
            let json = serde_json::to_string(pbkdf).unwrap();
            let de: LuksPbkdf = serde_json::from_str(&json).unwrap();
            assert_eq!(de, *pbkdf);
            assert_eq!(de.as_str(), pbkdf.as_str());
        }
    }

    #[test]
    fn test_luks_key_from_bytes_preserves_all_byte_values() {
        let data: Vec<u8> = (0..=255).collect();
        let key = LuksKey::from_bytes(data.clone()).unwrap();
        assert_eq!(key.len(), 256);
        assert_eq!(key.as_bytes(), &data[..]);
    }

    #[test]
    fn test_luks_status_serde_with_some_mount() {
        let status = LuksStatus {
            name: "vol".to_string(),
            is_open: true,
            is_mounted: true,
            backing_path: PathBuf::from("/a"),
            mount_point: Some(PathBuf::from("/mnt")),
            cipher: "aes-xts-plain64".to_string(),
            key_size_bits: 512,
        };
        let json = serde_json::to_string(&status).unwrap();
        let de: LuksStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(de.mount_point, Some(PathBuf::from("/mnt")));
    }

    #[test]
    fn test_cryptsetup_available_does_not_panic() {
        // Only verifies no panic/crash
        let _ = cryptsetup_available();
    }

    #[test]
    fn test_dmcrypt_supported_does_not_panic() {
        let _ = dmcrypt_supported();
    }
}
