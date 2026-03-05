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

    // --- Additional coverage tests ---

    #[test]
    fn test_verity_hash_algorithm_display() {
        assert_eq!(format!("{}", VerityHashAlgorithm::Sha256), "sha256");
        assert_eq!(format!("{}", VerityHashAlgorithm::Sha512), "sha512");
    }

    #[test]
    fn test_verity_hash_algorithm_serde_roundtrip() {
        for alg in &[VerityHashAlgorithm::Sha256, VerityHashAlgorithm::Sha512] {
            let json = serde_json::to_string(alg).unwrap();
            let back: VerityHashAlgorithm = serde_json::from_str(&json).unwrap();
            assert_eq!(*alg, back);
        }
    }

    #[test]
    fn test_verity_hash_algorithm_clone_copy_eq() {
        let a = VerityHashAlgorithm::Sha256;
        let b = a; // Copy
        let c = a.clone(); // Clone
        assert_eq!(a, b);
        assert_eq!(a, c);
        assert_ne!(VerityHashAlgorithm::Sha256, VerityHashAlgorithm::Sha512);
    }

    #[test]
    fn test_verity_hash_algorithm_debug() {
        assert_eq!(format!("{:?}", VerityHashAlgorithm::Sha256), "Sha256");
        assert_eq!(format!("{:?}", VerityHashAlgorithm::Sha512), "Sha512");
    }

    #[test]
    fn test_verity_config_validate_name_too_long() {
        let config = VerityConfig {
            name: "a".repeat(129),
            data_device: PathBuf::from("/dev/sda1"),
            hash_device: PathBuf::from("/dev/sda2"),
            data_block_size: 4096,
            hash_block_size: 4096,
            hash_algorithm: VerityHashAlgorithm::Sha256,
            root_hash: "a".repeat(64),
            salt: None,
        };
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("too long"));
    }

    #[test]
    fn test_verity_config_validate_name_exactly_128() {
        let config = VerityConfig {
            name: "a".repeat(128),
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
    fn test_verity_config_validate_name_invalid_chars() {
        let config = VerityConfig {
            name: "bad name!".to_string(),
            data_device: PathBuf::from("/dev/sda1"),
            hash_device: PathBuf::from("/dev/sda2"),
            data_block_size: 4096,
            hash_block_size: 4096,
            hash_algorithm: VerityHashAlgorithm::Sha256,
            root_hash: "a".repeat(64),
            salt: None,
        };
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("invalid characters"));
    }

    #[test]
    fn test_verity_config_validate_name_with_slash() {
        let config = VerityConfig {
            name: "bad/name".to_string(),
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
    fn test_verity_config_validate_name_with_dash_and_underscore() {
        let config = VerityConfig {
            name: "my-verity_vol01".to_string(),
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
    fn test_verity_config_validate_data_block_size_zero() {
        let config = VerityConfig {
            name: "test".to_string(),
            data_device: PathBuf::from("/dev/sda1"),
            hash_device: PathBuf::from("/dev/sda2"),
            data_block_size: 0,
            hash_block_size: 4096,
            hash_algorithm: VerityHashAlgorithm::Sha256,
            root_hash: "a".repeat(64),
            salt: None,
        };
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("power of 2"));
    }

    #[test]
    fn test_verity_config_validate_hash_block_size_not_pow2() {
        let config = VerityConfig {
            name: "test".to_string(),
            data_device: PathBuf::from("/dev/sda1"),
            hash_device: PathBuf::from("/dev/sda2"),
            data_block_size: 4096,
            hash_block_size: 3000,
            hash_algorithm: VerityHashAlgorithm::Sha256,
            root_hash: "a".repeat(64),
            salt: None,
        };
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("Hash block size"));
    }

    #[test]
    fn test_verity_config_validate_block_sizes_pow2() {
        // Test various valid power-of-2 block sizes
        for size in &[512, 1024, 2048, 4096, 8192] {
            let config = VerityConfig {
                name: "test".to_string(),
                data_device: PathBuf::from("/dev/sda1"),
                hash_device: PathBuf::from("/dev/sda2"),
                data_block_size: *size,
                hash_block_size: *size,
                hash_algorithm: VerityHashAlgorithm::Sha256,
                root_hash: "a".repeat(64),
                salt: None,
            };
            assert!(config.validate().is_ok(), "Block size {} should be valid", size);
        }
    }

    #[test]
    fn test_verity_config_validate_sha512_hash() {
        let config = VerityConfig {
            name: "test".to_string(),
            data_device: PathBuf::from("/dev/sda1"),
            hash_device: PathBuf::from("/dev/sda2"),
            data_block_size: 4096,
            hash_block_size: 4096,
            hash_algorithm: VerityHashAlgorithm::Sha512,
            root_hash: "b".repeat(128),
            salt: None,
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_verity_config_validate_sha512_wrong_hash_len() {
        let config = VerityConfig {
            name: "test".to_string(),
            data_device: PathBuf::from("/dev/sda1"),
            hash_device: PathBuf::from("/dev/sda2"),
            data_block_size: 4096,
            hash_block_size: 4096,
            hash_algorithm: VerityHashAlgorithm::Sha512,
            root_hash: "b".repeat(64), // SHA-256 length, not SHA-512
            salt: None,
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_verity_config_clone() {
        let config = VerityConfig {
            name: "test-clone".to_string(),
            data_device: PathBuf::from("/dev/sda1"),
            hash_device: PathBuf::from("/dev/sda2"),
            data_block_size: 4096,
            hash_block_size: 4096,
            hash_algorithm: VerityHashAlgorithm::Sha256,
            root_hash: "a".repeat(64),
            salt: Some("beef".to_string()),
        };
        let cloned = config.clone();
        assert_eq!(cloned.name, config.name);
        assert_eq!(cloned.data_device, config.data_device);
        assert_eq!(cloned.hash_device, config.hash_device);
        assert_eq!(cloned.data_block_size, config.data_block_size);
        assert_eq!(cloned.hash_block_size, config.hash_block_size);
        assert_eq!(cloned.hash_algorithm, config.hash_algorithm);
        assert_eq!(cloned.root_hash, config.root_hash);
        assert_eq!(cloned.salt, config.salt);
    }

    #[test]
    fn test_verity_config_debug() {
        let config = VerityConfig {
            name: "dbg-test".to_string(),
            data_device: PathBuf::from("/dev/sda1"),
            hash_device: PathBuf::from("/dev/sda2"),
            data_block_size: 4096,
            hash_block_size: 4096,
            hash_algorithm: VerityHashAlgorithm::Sha256,
            root_hash: "a".repeat(64),
            salt: None,
        };
        let dbg = format!("{:?}", config);
        assert!(dbg.contains("VerityConfig"));
        assert!(dbg.contains("dbg-test"));
    }

    #[test]
    fn test_verity_config_serialization_roundtrip() {
        let config = VerityConfig {
            name: "serde-test".to_string(),
            data_device: PathBuf::from("/dev/loop0"),
            hash_device: PathBuf::from("/dev/loop1"),
            data_block_size: 4096,
            hash_block_size: 4096,
            hash_algorithm: VerityHashAlgorithm::Sha512,
            root_hash: "c".repeat(128),
            salt: Some("0123456789abcdef".to_string()),
        };
        let json = serde_json::to_string(&config).unwrap();
        let back: VerityConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "serde-test");
        assert_eq!(back.data_device, PathBuf::from("/dev/loop0"));
        assert_eq!(back.hash_device, PathBuf::from("/dev/loop1"));
        assert_eq!(back.data_block_size, 4096);
        assert_eq!(back.hash_algorithm, VerityHashAlgorithm::Sha512);
        assert_eq!(back.root_hash, "c".repeat(128));
        assert_eq!(back.salt, Some("0123456789abcdef".to_string()));
    }

    #[test]
    fn test_verity_status_clone() {
        let status = VerityStatus {
            name: "vol1".to_string(),
            is_active: true,
            is_verified: true,
            corruption_detected: false,
            root_hash: "f".repeat(64),
        };
        let cloned = status.clone();
        assert_eq!(cloned.name, status.name);
        assert_eq!(cloned.is_active, status.is_active);
        assert_eq!(cloned.is_verified, status.is_verified);
        assert_eq!(cloned.corruption_detected, status.corruption_detected);
        assert_eq!(cloned.root_hash, status.root_hash);
    }

    #[test]
    fn test_verity_status_debug() {
        let status = VerityStatus {
            name: "vol1".to_string(),
            is_active: false,
            is_verified: false,
            corruption_detected: true,
            root_hash: String::new(),
        };
        let dbg = format!("{:?}", status);
        assert!(dbg.contains("VerityStatus"));
        assert!(dbg.contains("vol1"));
        assert!(dbg.contains("true")); // corruption_detected
    }

    #[test]
    fn test_verity_status_serialization_all_states() {
        // Active and verified
        let s1 = VerityStatus {
            name: "active".to_string(),
            is_active: true,
            is_verified: true,
            corruption_detected: false,
            root_hash: "a".repeat(64),
        };
        let json1 = serde_json::to_string(&s1).unwrap();
        let back1: VerityStatus = serde_json::from_str(&json1).unwrap();
        assert!(back1.is_active);
        assert!(back1.is_verified);

        // Inactive
        let s2 = VerityStatus {
            name: "inactive".to_string(),
            is_active: false,
            is_verified: false,
            corruption_detected: false,
            root_hash: String::new(),
        };
        let json2 = serde_json::to_string(&s2).unwrap();
        let back2: VerityStatus = serde_json::from_str(&json2).unwrap();
        assert!(!back2.is_active);

        // Corrupted
        let s3 = VerityStatus {
            name: "corrupted".to_string(),
            is_active: true,
            is_verified: false,
            corruption_detected: true,
            root_hash: "d".repeat(64),
        };
        let json3 = serde_json::to_string(&s3).unwrap();
        let back3: VerityStatus = serde_json::from_str(&json3).unwrap();
        assert!(back3.corruption_detected);
        assert!(!back3.is_verified);
    }

    #[test]
    fn test_validate_root_hash_uppercase_hex() {
        // Uppercase hex should be valid
        let hash = "ABCDEF0123456789".repeat(4); // 64 chars
        assert!(validate_root_hash(&hash, VerityHashAlgorithm::Sha256).is_ok());
    }

    #[test]
    fn test_validate_root_hash_mixed_case_hex() {
        let hash = "aAbBcCdDeEfF0123456789aAbBcCdDeEfF0123456789aAbBcCdDeEfF01234567";
        assert_eq!(hash.len(), 64);
        assert!(validate_root_hash(hash, VerityHashAlgorithm::Sha256).is_ok());
    }

    #[test]
    fn test_validate_root_hash_sha512_too_long() {
        let hash = "a".repeat(129);
        assert!(validate_root_hash(&hash, VerityHashAlgorithm::Sha512).is_err());
    }

    #[test]
    fn test_validate_root_hash_sha256_too_long() {
        let hash = "a".repeat(65);
        assert!(validate_root_hash(&hash, VerityHashAlgorithm::Sha256).is_err());
    }

    #[test]
    fn test_validate_root_hash_non_hex_mixed() {
        // Valid length but contains non-hex chars
        let mut hash = "a".repeat(63);
        hash.push('z');
        assert!(validate_root_hash(&hash, VerityHashAlgorithm::Sha256).is_err());
    }

    #[test]
    fn test_validate_root_hash_error_messages() {
        let err = validate_root_hash("", VerityHashAlgorithm::Sha256).unwrap_err();
        assert!(err.to_string().contains("empty"));

        let err = validate_root_hash(&"a".repeat(32), VerityHashAlgorithm::Sha256).unwrap_err();
        assert!(err.to_string().contains("length"));

        let err = validate_root_hash(&"z".repeat(64), VerityHashAlgorithm::Sha256).unwrap_err();
        assert!(err.to_string().contains("non-hex"));
    }

    #[test]
    fn test_validate_hex_string_valid() {
        assert!(validate_hex_string("0123456789abcdefABCDEF", "test").is_ok());
    }

    #[test]
    fn test_validate_hex_string_empty() {
        // Empty string should be valid (all chars are hex vacuously)
        assert!(validate_hex_string("", "test").is_ok());
    }

    #[test]
    fn test_validate_hex_string_invalid() {
        let err = validate_hex_string("not-hex!", "mysalt").unwrap_err();
        assert!(err.to_string().contains("mysalt"));
        assert!(err.to_string().contains("non-hex"));
    }

    #[test]
    fn test_read_stored_root_hash_empty_file() {
        let dir = std::env::temp_dir().join("agnos_verity_test_empty");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("empty-hash");
        std::fs::write(&path, "").unwrap();

        let err = read_stored_root_hash(&path).unwrap_err();
        assert!(err.to_string().contains("empty"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_read_stored_root_hash_with_whitespace() {
        let dir = std::env::temp_dir().join("agnos_verity_test_ws");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("hash-ws");
        let hash = "a".repeat(64);
        // Write with trailing newline and spaces
        std::fs::write(&path, format!("  {}  \n", hash)).unwrap();

        let result = read_stored_root_hash(&path).unwrap();
        assert_eq!(result, hash);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_verity_close_empty_name() {
        let result = verity_close("");
        assert!(result.is_err());
    }

    #[test]
    fn test_verity_status_empty_name() {
        let result = verity_status("");
        assert!(result.is_err());
    }

    #[test]
    fn test_verity_config_validate_non_hex_root_hash() {
        let config = VerityConfig {
            name: "test".to_string(),
            data_device: PathBuf::from("/dev/sda1"),
            hash_device: PathBuf::from("/dev/sda2"),
            data_block_size: 4096,
            hash_block_size: 4096,
            hash_algorithm: VerityHashAlgorithm::Sha256,
            root_hash: "z".repeat(64),
            salt: None,
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_verity_config_validate_empty_root_hash() {
        let config = VerityConfig {
            name: "test".to_string(),
            data_device: PathBuf::from("/dev/sda1"),
            hash_device: PathBuf::from("/dev/sda2"),
            data_block_size: 4096,
            hash_block_size: 4096,
            hash_algorithm: VerityHashAlgorithm::Sha256,
            root_hash: String::new(),
            salt: None,
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_verity_config_serialization_no_salt() {
        let config = VerityConfig {
            name: "no-salt".to_string(),
            data_device: PathBuf::from("/dev/sda1"),
            hash_device: PathBuf::from("/dev/sda2"),
            data_block_size: 4096,
            hash_block_size: 4096,
            hash_algorithm: VerityHashAlgorithm::Sha256,
            root_hash: "a".repeat(64),
            salt: None,
        };
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("\"salt\":null"));
        let back: VerityConfig = serde_json::from_str(&json).unwrap();
        assert!(back.salt.is_none());
    }

    // --- New coverage tests ---

    #[test]
    fn test_read_stored_root_hash_whitespace_only_file() {
        let dir = std::env::temp_dir().join("agnos_verity_test_ws_only");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("ws-only-hash");
        std::fs::write(&path, "   \n  \t  \n").unwrap();

        let err = read_stored_root_hash(&path).unwrap_err();
        assert!(err.to_string().contains("empty"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_verity_config_validate_block_size_1() {
        let config = VerityConfig {
            name: "test".to_string(),
            data_device: PathBuf::from("/dev/sda1"),
            hash_device: PathBuf::from("/dev/sda2"),
            data_block_size: 1,
            hash_block_size: 4096,
            hash_algorithm: VerityHashAlgorithm::Sha256,
            root_hash: "a".repeat(64),
            salt: None,
        };
        // 1 is a power of 2
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_verity_config_validate_block_size_2() {
        let config = VerityConfig {
            name: "test".to_string(),
            data_device: PathBuf::from("/dev/sda1"),
            hash_device: PathBuf::from("/dev/sda2"),
            data_block_size: 2,
            hash_block_size: 2,
            hash_algorithm: VerityHashAlgorithm::Sha256,
            root_hash: "a".repeat(64),
            salt: None,
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_verity_config_validate_hash_block_size_zero() {
        let config = VerityConfig {
            name: "test".to_string(),
            data_device: PathBuf::from("/dev/sda1"),
            hash_device: PathBuf::from("/dev/sda2"),
            data_block_size: 4096,
            hash_block_size: 0,
            hash_algorithm: VerityHashAlgorithm::Sha256,
            root_hash: "a".repeat(64),
            salt: None,
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_root_hash_sha256_all_digits() {
        let hash = "0123456789012345678901234567890123456789012345678901234567890123";
        assert_eq!(hash.len(), 64);
        assert!(validate_root_hash(hash, VerityHashAlgorithm::Sha256).is_ok());
    }

    #[test]
    fn test_validate_root_hash_sha512_correct_length() {
        let hash = "a".repeat(128);
        assert!(validate_root_hash(&hash, VerityHashAlgorithm::Sha512).is_ok());
    }

    #[test]
    fn test_validate_root_hash_sha512_wrong_length_64() {
        let hash = "a".repeat(64);
        let err = validate_root_hash(&hash, VerityHashAlgorithm::Sha512).unwrap_err();
        assert!(err.to_string().contains("length"));
        assert!(err.to_string().contains("128"));
    }

    #[test]
    fn test_verity_config_validate_salt_empty_string() {
        let config = VerityConfig {
            name: "test".to_string(),
            data_device: PathBuf::from("/dev/sda1"),
            hash_device: PathBuf::from("/dev/sda2"),
            data_block_size: 4096,
            hash_block_size: 4096,
            hash_algorithm: VerityHashAlgorithm::Sha256,
            root_hash: "a".repeat(64),
            salt: Some(String::new()),
        };
        // Empty hex string is vacuously valid
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_verity_status_default_values() {
        let status = VerityStatus {
            name: String::new(),
            is_active: false,
            is_verified: false,
            corruption_detected: false,
            root_hash: String::new(),
        };
        assert!(!status.is_active);
        assert!(!status.is_verified);
        assert!(!status.corruption_detected);
        assert!(status.root_hash.is_empty());
    }
}
