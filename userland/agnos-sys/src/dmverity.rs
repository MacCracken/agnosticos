//! dm-verity Rootfs Integrity Interface
//!
//! Userland wrappers for dm-verity — verifies read-only rootfs integrity at the
//! block level. Shells out to `veritysetup` (part of the cryptsetup package).
//!
//! On non-Linux platforms, all operations return `SysError::NotSupported`.

use crate::error::{Result, SysError};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Hash algorithm for dm-verity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VerityHashAlgorithm {
    Sha256,
    Sha512,
}

impl VerityHashAlgorithm {
    /// Return the algorithm name as used by `veritysetup`.
    pub fn as_str(&self) -> &str {
        match self {
            VerityHashAlgorithm::Sha256 => "sha256",
            VerityHashAlgorithm::Sha512 => "sha512",
        }
    }

    /// Expected hex length of the root hash for this algorithm.
    pub fn hash_hex_len(&self) -> usize {
        match self {
            VerityHashAlgorithm::Sha256 => 64,
            VerityHashAlgorithm::Sha512 => 128,
        }
    }
}

impl std::fmt::Display for VerityHashAlgorithm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Configuration for a dm-verity volume.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerityConfig {
    /// Name for the dm-verity device mapping
    pub name: String,
    /// Path to the data (read-only) device/image
    pub data_device: PathBuf,
    /// Path to the hash device/image
    pub hash_device: PathBuf,
    /// Data block size (typically 4096)
    pub data_block_size: u32,
    /// Hash block size (typically 4096)
    pub hash_block_size: u32,
    /// Hash algorithm
    pub hash_algorithm: VerityHashAlgorithm,
    /// Root hash (hex string) — the trust anchor
    pub root_hash: String,
    /// Optional salt (hex string)
    pub salt: Option<String>,
}

impl VerityConfig {
    /// Validate the configuration.
    pub fn validate(&self) -> Result<()> {
        if self.name.is_empty() {
            return Err(SysError::InvalidArgument("Verity name cannot be empty".into()));
        }
        if self.name.len() > 128 {
            return Err(SysError::InvalidArgument("Verity name too long (max 128)".into()));
        }
        if !self.name.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
            return Err(SysError::InvalidArgument(format!(
                "Verity name contains invalid characters: {}",
                self.name
            )));
        }
        if self.data_block_size == 0 || (self.data_block_size & (self.data_block_size - 1)) != 0 {
            return Err(SysError::InvalidArgument(format!(
                "Data block size must be a power of 2: {}",
                self.data_block_size
            )));
        }
        if self.hash_block_size == 0 || (self.hash_block_size & (self.hash_block_size - 1)) != 0 {
            return Err(SysError::InvalidArgument(format!(
                "Hash block size must be a power of 2: {}",
                self.hash_block_size
            )));
        }
        validate_root_hash(&self.root_hash, self.hash_algorithm)?;
        if let Some(ref salt) = self.salt {
            validate_hex_string(salt, "salt")?;
        }
        Ok(())
    }
}

/// Status of a dm-verity volume.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerityStatus {
    /// Device mapping name
    pub name: String,
    /// Whether the verity mapping is active
    pub is_active: bool,
    /// Whether verification is passing
    pub is_verified: bool,
    /// Whether corruption has been detected
    pub corruption_detected: bool,
    /// The root hash in use
    pub root_hash: String,
}

/// Validate that a root hash is well-formed hex of the correct length.
pub fn validate_root_hash(hash: &str, algorithm: VerityHashAlgorithm) -> Result<()> {
    if hash.is_empty() {
        return Err(SysError::InvalidArgument("Root hash cannot be empty".into()));
    }

    let expected_len = algorithm.hash_hex_len();
    if hash.len() != expected_len {
        return Err(SysError::InvalidArgument(format!(
            "Root hash length {} does not match {} expected length {}",
            hash.len(),
            algorithm,
            expected_len
        )));
    }

    if !hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(SysError::InvalidArgument(
            "Root hash contains non-hex characters".into(),
        ));
    }

    Ok(())
}

/// Validate a hex string.
fn validate_hex_string(s: &str, name: &str) -> Result<()> {
    if !s.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(SysError::InvalidArgument(format!(
            "{} contains non-hex characters",
            name
        )));
    }
    Ok(())
}

/// Format a data device for dm-verity, generating the hash tree.
///
/// Runs `veritysetup format` and returns the computed root hash.
pub fn verity_format(
    data_device: &Path,
    hash_device: &Path,
    algorithm: VerityHashAlgorithm,
    salt: Option<&str>,
) -> Result<String> {
    #[cfg(target_os = "linux")]
    {
        if !data_device.exists() {
            return Err(SysError::InvalidArgument(format!(
                "Data device not found: {}",
                data_device.display()
            )));
        }

        let mut args = vec![
            "format".to_string(),
            data_device.to_string_lossy().to_string(),
            hash_device.to_string_lossy().to_string(),
            "--hash".to_string(),
            algorithm.as_str().to_string(),
        ];

        if let Some(salt) = salt {
            validate_hex_string(salt, "salt")?;
            args.push("--salt".to_string());
            args.push(salt.to_string());
        }

        let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let output = run_veritysetup(&args_ref)?;

        // Parse root hash from output: "Root hash: <hex>"
        let root_hash = output
            .lines()
            .find(|line| line.starts_with("Root hash:"))
            .and_then(|line| line.strip_prefix("Root hash:"))
            .map(|h| h.trim().to_string())
            .ok_or_else(|| SysError::Unknown("Could not parse root hash from veritysetup output".into()))?;

        tracing::info!(
            "Formatted dm-verity: data={}, hash={}, algo={}, root_hash={}",
            data_device.display(),
            hash_device.display(),
            algorithm,
            &root_hash[..16.min(root_hash.len())]
        );

        Ok(root_hash)
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = (data_device, hash_device, algorithm, salt);
        Err(SysError::NotSupported)
    }
}

/// Open (activate) a dm-verity volume.
///
/// Creates a read-only device mapping at `/dev/mapper/{name}` that verifies
/// every block read against the hash tree.
pub fn verity_open(config: &VerityConfig) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        config.validate()?;

        let data_dev = config.data_device.to_string_lossy().to_string();
        let hash_dev = config.hash_device.to_string_lossy().to_string();
        let hash_algo = config.hash_algorithm.as_str().to_string();

        let mut args: Vec<String> = vec![
            "open".to_string(),
            "--hash".to_string(),
            hash_algo,
            data_dev,
            hash_dev,
            config.name.clone(),
            config.root_hash.clone(),
        ];

        if let Some(ref salt) = config.salt {
            args.push("--salt".to_string());
            args.push(salt.clone());
        }

        let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        run_veritysetup_checked(&args_ref)?;

        tracing::info!("Opened dm-verity volume '{}'", config.name);
        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = config;
        Err(SysError::NotSupported)
    }
}

/// Close (deactivate) a dm-verity volume.
pub fn verity_close(name: &str) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        if name.is_empty() {
            return Err(SysError::InvalidArgument("Verity name cannot be empty".into()));
        }

        run_veritysetup_checked(&["close", name])?;
        tracing::info!("Closed dm-verity volume '{}'", name);
        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = name;
        Err(SysError::NotSupported)
    }
}

/// Query the status of a dm-verity volume.
pub fn verity_status(name: &str) -> Result<VerityStatus> {
    #[cfg(target_os = "linux")]
    {
        if name.is_empty() {
            return Err(SysError::InvalidArgument("Verity name cannot be empty".into()));
        }

        let output = run_veritysetup(&["status", name]);

        match output {
            Ok(text) => {
                let is_active = text.contains("type:");
                let corruption_detected = text.contains("corrupted");

                // Try to extract root hash
                let root_hash = text
                    .lines()
                    .find(|l| l.trim().starts_with("root hash:"))
                    .and_then(|l| l.split(':').nth(1))
                    .map(|h| h.trim().to_string())
                    .unwrap_or_default();

                Ok(VerityStatus {
                    name: name.to_string(),
                    is_active,
                    is_verified: is_active && !corruption_detected,
                    corruption_detected,
                    root_hash,
                })
            }
            Err(_) => Ok(VerityStatus {
                name: name.to_string(),
                is_active: false,
                is_verified: false,
                corruption_detected: false,
                root_hash: String::new(),
            }),
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = name;
        Err(SysError::NotSupported)
    }
}

/// Verify a dm-verity volume without activating it.
///
/// Returns `true` if the data matches the hash tree and root hash.
pub fn verity_verify(
    data_device: &Path,
    hash_device: &Path,
    root_hash: &str,
) -> Result<bool> {
    #[cfg(target_os = "linux")]
    {
        if !data_device.exists() {
            return Err(SysError::InvalidArgument(format!(
                "Data device not found: {}",
                data_device.display()
            )));
        }
        if !hash_device.exists() {
            return Err(SysError::InvalidArgument(format!(
                "Hash device not found: {}",
                hash_device.display()
            )));
        }
        if root_hash.is_empty() || !root_hash.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(SysError::InvalidArgument("Invalid root hash".into()));
        }

        let data_str = data_device.to_string_lossy();
        let hash_str = hash_device.to_string_lossy();
        let result = run_veritysetup(&["verify", &data_str, &hash_str, root_hash]);

        match result {
            Ok(_) => {
                tracing::info!("dm-verity verification PASSED for {}", data_device.display());
                Ok(true)
            }
            Err(_) => {
                tracing::warn!("dm-verity verification FAILED for {}", data_device.display());
                Ok(false)
            }
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = (data_device, hash_device, root_hash);
        Err(SysError::NotSupported)
    }
}

/// Check if dm-verity is supported on this system.
///
/// Checks for both the kernel module and the `veritysetup` tool.
pub fn verity_supported() -> bool {
    #[cfg(target_os = "linux")]
    {
        let module_loaded = Path::new("/sys/module/dm_verity").exists()
            || std::fs::read_to_string("/proc/modules")
                .map(|s| s.contains("dm_verity"))
                .unwrap_or(false);

        let tool_available = std::process::Command::new("veritysetup")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        module_loaded || tool_available
    }

    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}

/// Read a stored root hash from a file (e.g., `/etc/agnos/verity-root-hash`).
///
/// Validates that the content is a well-formed hex string.
pub fn read_stored_root_hash(path: &Path) -> Result<String> {
    if !path.exists() {
        return Err(SysError::InvalidArgument(format!(
            "Root hash file not found: {}",
            path.display()
        )));
    }

    let hash = std::fs::read_to_string(path)
        .map_err(|e| SysError::Unknown(format!("Failed to read {}: {}", path.display(), e)))?;
    let hash = hash.trim().to_string();

    if hash.is_empty() {
        return Err(SysError::InvalidArgument("Root hash file is empty".into()));
    }
    if !hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(SysError::InvalidArgument(
            "Root hash file contains non-hex characters".into(),
        ));
    }

    Ok(hash)
}

// --- Internal helpers ---

/// Run `veritysetup` and return stdout.
#[cfg(target_os = "linux")]
fn run_veritysetup(args: &[&str]) -> Result<String> {
    let output = std::process::Command::new("veritysetup")
        .args(args)
        .output()
        .map_err(|e| SysError::Unknown(format!("Failed to run veritysetup: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(SysError::Unknown(format!(
            "veritysetup {} failed: {}",
            args.join(" "),
            stderr.trim()
        )));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Run `veritysetup` and check for success.
#[cfg(target_os = "linux")]
fn run_veritysetup_checked(args: &[&str]) -> Result<()> {
    let _ = run_veritysetup(args)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verity_hash_algorithm_as_str() {
        assert_eq!(VerityHashAlgorithm::Sha256.as_str(), "sha256");
        assert_eq!(VerityHashAlgorithm::Sha512.as_str(), "sha512");
    }

    #[test]
    fn test_verity_hash_algorithm_hex_len() {
        assert_eq!(VerityHashAlgorithm::Sha256.hash_hex_len(), 64);
        assert_eq!(VerityHashAlgorithm::Sha512.hash_hex_len(), 128);
    }

    #[test]
    fn test_validate_root_hash_sha256_ok() {
        let hash = "a".repeat(64);
        assert!(validate_root_hash(&hash, VerityHashAlgorithm::Sha256).is_ok());
    }

    #[test]
    fn test_validate_root_hash_sha512_ok() {
        let hash = "b".repeat(128);
        assert!(validate_root_hash(&hash, VerityHashAlgorithm::Sha512).is_ok());
    }

    #[test]
    fn test_validate_root_hash_empty() {
        assert!(validate_root_hash("", VerityHashAlgorithm::Sha256).is_err());
    }

    #[test]
    fn test_validate_root_hash_wrong_length() {
        let hash = "a".repeat(32); // Too short for SHA-256
        assert!(validate_root_hash(&hash, VerityHashAlgorithm::Sha256).is_err());
    }

    #[test]
    fn test_validate_root_hash_non_hex() {
        let hash = "g".repeat(64); // 'g' is not hex
        assert!(validate_root_hash(&hash, VerityHashAlgorithm::Sha256).is_err());
    }

    #[test]
    fn test_verity_config_validate_ok() {
        let config = VerityConfig {
            name: "test-verity".to_string(),
            data_device: PathBuf::from("/dev/sda1"),
            hash_device: PathBuf::from("/dev/sda2"),
            data_block_size: 4096,
            hash_block_size: 4096,
            hash_algorithm: VerityHashAlgorithm::Sha256,
            root_hash: "a".repeat(64),
            salt: None,
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_verity_config_validate_empty_name() {
        let config = VerityConfig {
            name: String::new(),
            data_device: PathBuf::from("/dev/sda1"),
            hash_device: PathBuf::from("/dev/sda2"),
            data_block_size: 4096,
            hash_block_size: 4096,
            hash_algorithm: VerityHashAlgorithm::Sha256,
            root_hash: "a".repeat(64),
            salt: None,
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_verity_config_validate_bad_block_size() {
        let config = VerityConfig {
            name: "test".to_string(),
            data_device: PathBuf::from("/dev/sda1"),
            hash_device: PathBuf::from("/dev/sda2"),
            data_block_size: 1000, // Not a power of 2
            hash_block_size: 4096,
            hash_algorithm: VerityHashAlgorithm::Sha256,
            root_hash: "a".repeat(64),
            salt: None,
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_verity_config_validate_with_salt() {
        let config = VerityConfig {
            name: "test".to_string(),
            data_device: PathBuf::from("/dev/sda1"),
            hash_device: PathBuf::from("/dev/sda2"),
            data_block_size: 4096,
            hash_block_size: 4096,
            hash_algorithm: VerityHashAlgorithm::Sha256,
            root_hash: "a".repeat(64),
            salt: Some("deadbeef".to_string()),
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_verity_config_validate_bad_salt() {
        let config = VerityConfig {
            name: "test".to_string(),
            data_device: PathBuf::from("/dev/sda1"),
            hash_device: PathBuf::from("/dev/sda2"),
            data_block_size: 4096,
            hash_block_size: 4096,
            hash_algorithm: VerityHashAlgorithm::Sha256,
            root_hash: "a".repeat(64),
            salt: Some("not-hex!".to_string()),
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_verity_status_serialization() {
        let status = VerityStatus {
            name: "test-vol".to_string(),
            is_active: true,
            is_verified: true,
            corruption_detected: false,
            root_hash: "a".repeat(64),
        };
        let json = serde_json::to_string(&status).unwrap();
        let deserialized: VerityStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "test-vol");
        assert!(deserialized.is_active);
        assert!(!deserialized.corruption_detected);
    }

    #[test]
    fn test_read_stored_root_hash_ok() {
        let dir = std::env::temp_dir().join("agnos_verity_test");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("root-hash");
        let hash = "a".repeat(64);
        std::fs::write(&path, &hash).unwrap();

        let result = read_stored_root_hash(&path).unwrap();
        assert_eq!(result, hash);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_read_stored_root_hash_not_found() {
        let path = Path::new("/tmp/nonexistent_verity_hash_test");
        assert!(read_stored_root_hash(path).is_err());
    }

    #[test]
    fn test_read_stored_root_hash_invalid() {
        let dir = std::env::temp_dir().join("agnos_verity_test_invalid");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("root-hash-bad");
        std::fs::write(&path, "not-hex-data!").unwrap();

        assert!(read_stored_root_hash(&path).is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_verity_supported() {
        // Just verify it doesn't crash
        let _supported = verity_supported();
    }

    #[test]
    #[ignore = "Requires root and veritysetup"]
    fn test_verity_format_and_verify() {
        // Would create test images, format, and verify
    }
}
