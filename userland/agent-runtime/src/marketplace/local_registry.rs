//! Local Package Registry
//!
//! File-backed index of installed marketplace packages. Handles extraction
//! of `.agnos-agent` tarballs, signature verification, and package lifecycle.

use std::collections::HashMap;
use std::io::Read as IoRead;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
// sha2 used transitively via trust::hash_data
use tracing::{debug, info, warn};

use super::transparency::TransparencyLog;
use super::trust::{self, PublisherKeyring};
use super::MarketplaceManifest;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Default marketplace storage directory.
pub const DEFAULT_MARKETPLACE_DIR: &str = "/var/lib/agnos/marketplace";

/// Index filename within the marketplace directory.
pub const INDEX_FILENAME: &str = "index.json";

/// Maximum package size (100 MB).
pub const MAX_PACKAGE_SIZE: u64 = 100 * 1024 * 1024;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Record of a locally installed marketplace package.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledMarketplacePackage {
    /// The marketplace manifest.
    pub manifest: MarketplaceManifest,
    /// When installed.
    pub installed_at: DateTime<Utc>,
    /// Installation directory.
    pub install_dir: PathBuf,
    /// SHA-256 hash of the package tarball.
    pub package_hash: String,
    /// Whether auto-update is enabled.
    pub auto_update: bool,
    /// Size of the installed package in bytes.
    pub installed_size: u64,
}

impl InstalledMarketplacePackage {
    /// Package name shorthand.
    pub fn name(&self) -> &str {
        &self.manifest.agent.name
    }

    /// Package version shorthand.
    pub fn version(&self) -> &str {
        &self.manifest.agent.version
    }

    /// Publisher name shorthand.
    pub fn publisher(&self) -> &str {
        &self.manifest.publisher.name
    }
}

/// Result of installing a package.
#[derive(Debug, Clone)]
pub struct MarketplaceInstallResult {
    pub name: String,
    pub version: String,
    pub install_dir: PathBuf,
    pub upgraded_from: Option<String>,
}

/// Result of uninstalling a package.
#[derive(Debug, Clone)]
pub struct MarketplaceUninstallResult {
    pub name: String,
    pub version: String,
    pub files_removed: usize,
}

// ---------------------------------------------------------------------------
// Local registry
// ---------------------------------------------------------------------------

/// File-backed registry of installed marketplace packages.
pub struct LocalRegistry {
    /// Root directory for marketplace data.
    root_dir: PathBuf,
    /// Installed packages index (name → record).
    index: HashMap<String, InstalledMarketplacePackage>,
    /// Publisher keyring for signature verification.
    keyring: PublisherKeyring,
    /// Transparency log.
    transparency_log: TransparencyLog,
    /// Storage quota in bytes (0 = unlimited).
    storage_quota: u64,
}

impl LocalRegistry {
    /// Create a new local registry backed by the given directory.
    pub fn new(root_dir: &Path) -> Result<Self> {
        let keys_dir = root_dir.join("keys");
        let mut registry = Self {
            root_dir: root_dir.to_path_buf(),
            index: HashMap::new(),
            keyring: PublisherKeyring::new(&keys_dir),
            transparency_log: TransparencyLog::new(),
            storage_quota: 0,
        };
        registry.load_index()?;
        Ok(registry)
    }

    /// Set storage quota (bytes). 0 = unlimited.
    pub fn set_storage_quota(&mut self, quota: u64) {
        self.storage_quota = quota;
    }

    /// Load the package index from disk.
    fn load_index(&mut self) -> Result<()> {
        let index_path = self.root_dir.join(INDEX_FILENAME);
        if !index_path.exists() {
            return Ok(());
        }

        let content = std::fs::read_to_string(&index_path)
            .with_context(|| format!("Failed to read index at {}", index_path.display()))?;

        self.index =
            serde_json::from_str(&content).with_context(|| "Failed to parse marketplace index")?;

        debug!(
            "Loaded {} marketplace packages from index",
            self.index.len()
        );
        Ok(())
    }

    /// Save the package index to disk.
    fn save_index(&self) -> Result<()> {
        let index_path = self.root_dir.join(INDEX_FILENAME);

        // Ensure directory exists
        if let Some(parent) = index_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory {}", parent.display()))?;
        }

        let content = serde_json::to_string_pretty(&self.index)
            .context("Failed to serialize marketplace index")?;

        std::fs::write(&index_path, content)
            .with_context(|| format!("Failed to write index to {}", index_path.display()))?;

        Ok(())
    }

    /// Install a package from a `.agnos-agent` tarball.
    ///
    /// If `keyring` is `Some`, the package signature is verified against the
    /// publisher key before extraction. Pass `None` to skip verification
    /// (dev / unsigned-package mode).
    pub fn install_package(
        &mut self,
        tarball_path: &Path,
        keyring: Option<&PublisherKeyring>,
    ) -> Result<MarketplaceInstallResult> {
        // Check file size
        let metadata = std::fs::metadata(tarball_path)
            .with_context(|| format!("Failed to stat {}", tarball_path.display()))?;

        if metadata.len() > MAX_PACKAGE_SIZE {
            anyhow::bail!(
                "Package exceeds maximum size ({} bytes > {} bytes)",
                metadata.len(),
                MAX_PACKAGE_SIZE
            );
        }

        // Check storage quota
        if self.storage_quota > 0 {
            let used = self.total_installed_size();
            if used + metadata.len() > self.storage_quota {
                anyhow::bail!(
                    "Storage quota exceeded ({} + {} > {})",
                    used,
                    metadata.len(),
                    self.storage_quota
                );
            }
        }

        // Compute hash
        let tarball_data = std::fs::read(tarball_path)
            .with_context(|| format!("Failed to read {}", tarball_path.display()))?;
        let package_hash = trust::hash_data(&tarball_data);

        // Extract and parse manifest
        let manifest = extract_manifest_from_tarball(&tarball_data)?;

        // Verify signature if a keyring was provided
        if let Some(kr) = keyring {
            let key_id = &manifest.publisher.key_id;
            let kv = kr.get_current_key(key_id).ok_or_else(|| {
                anyhow::anyhow!(
                    "No trusted key found for publisher key_id '{}' — \
                         package signature cannot be verified",
                    key_id
                )
            })?;
            let verifying_key = kv
                .verifying_key()
                .context("Failed to decode publisher verifying key from keyring")?;

            // Signature is expected as a `.sig` sidecar file next to the tarball
            let sig_path = tarball_path.with_extension("sig");
            let sig_bytes = std::fs::read(&sig_path).with_context(|| {
                format!(
                    "Signature file not found at {} — cannot verify package",
                    sig_path.display()
                )
            })?;

            trust::verify_signature(&tarball_data, &sig_bytes, &verifying_key)
                .context("Package signature verification failed")?;

            info!(
                "Verified signature for package {} (key_id={})",
                manifest.agent.name, key_id
            );
        } else {
            debug!(
                "Skipping signature verification for {} (no keyring provided)",
                manifest.agent.name
            );
        }

        // Validate manifest
        let errors = manifest.validate();
        if !errors.is_empty() {
            anyhow::bail!("Invalid manifest: {}", errors.join(", "));
        }

        let name = manifest.agent.name.clone();
        let version = manifest.agent.version.clone();

        // Check for upgrade
        let upgraded_from = self.index.get(&name).map(|p| p.version().to_string());

        // Install directory
        let install_dir = self.root_dir.join("packages").join(&name);
        std::fs::create_dir_all(&install_dir)
            .with_context(|| format!("Failed to create install dir {}", install_dir.display()))?;

        // Extract tarball contents
        let extracted_size = extract_tarball(&tarball_data, &install_dir)?;

        // Record in index
        let record = InstalledMarketplacePackage {
            manifest,
            installed_at: Utc::now(),
            install_dir: install_dir.clone(),
            package_hash,
            auto_update: false,
            installed_size: extracted_size,
        };

        self.index.insert(name.clone(), record);
        self.save_index()?;

        info!("Installed marketplace package {} v{}", name, version);

        Ok(MarketplaceInstallResult {
            name,
            version,
            install_dir,
            upgraded_from,
        })
    }

    /// Uninstall a package by name.
    pub fn uninstall_package(&mut self, name: &str) -> Result<MarketplaceUninstallResult> {
        let record = self
            .index
            .remove(name)
            .ok_or_else(|| anyhow::anyhow!("Package '{}' is not installed", name))?;

        let version = record.version().to_string();
        let mut files_removed = 0;

        // Remove install directory
        if record.install_dir.exists() {
            files_removed = count_files(&record.install_dir);
            std::fs::remove_dir_all(&record.install_dir).with_context(|| {
                format!(
                    "Failed to remove install dir {}",
                    record.install_dir.display()
                )
            })?;
        }

        self.save_index()?;

        info!("Uninstalled marketplace package {} v{}", name, version);

        Ok(MarketplaceUninstallResult {
            name: name.to_string(),
            version,
            files_removed,
        })
    }

    /// Get info about an installed package.
    pub fn get_package(&self, name: &str) -> Option<&InstalledMarketplacePackage> {
        self.index.get(name)
    }

    /// List all installed packages.
    pub fn list_installed(&self) -> Vec<&InstalledMarketplacePackage> {
        let mut packages: Vec<_> = self.index.values().collect();
        packages.sort_by_key(|p| p.name());
        packages
    }

    /// Search installed packages by query (matches name or description).
    pub fn search(&self, query: &str) -> Vec<&InstalledMarketplacePackage> {
        let q = query.to_lowercase();
        self.index
            .values()
            .filter(|p| {
                p.name().to_lowercase().contains(&q)
                    || p.manifest.agent.description.to_lowercase().contains(&q)
                    || p.manifest
                        .tags
                        .iter()
                        .any(|t| t.to_lowercase().contains(&q))
            })
            .collect()
    }

    /// Total size of all installed packages.
    pub fn total_installed_size(&self) -> u64 {
        self.index.values().map(|p| p.installed_size).sum()
    }

    /// Number of installed packages.
    pub fn len(&self) -> usize {
        self.index.len()
    }

    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    /// Access the transparency log.
    pub fn transparency_log(&self) -> &TransparencyLog {
        &self.transparency_log
    }

    /// Access the keyring.
    pub fn keyring(&self) -> &PublisherKeyring {
        &self.keyring
    }

    /// Mutable access to the keyring (for loading keys).
    pub fn keyring_mut(&mut self) -> &mut PublisherKeyring {
        &mut self.keyring
    }
}

// ---------------------------------------------------------------------------
// Tarball helpers
// ---------------------------------------------------------------------------

/// Extract the `manifest.json` from a gzipped tarball.
fn extract_manifest_from_tarball(data: &[u8]) -> Result<MarketplaceManifest> {
    let decoder = flate2::read::GzDecoder::new(data);
    let mut archive = tar::Archive::new(decoder);

    for entry in archive
        .entries()
        .context("Failed to read tarball entries")?
    {
        let mut entry = entry.context("Failed to read tarball entry")?;
        let path = entry
            .path()
            .context("Failed to get entry path")?
            .to_path_buf();

        if path.file_name().and_then(|f| f.to_str()) == Some("manifest.json") {
            let mut content = String::new();
            entry
                .read_to_string(&mut content)
                .context("Failed to read manifest.json")?;
            let manifest: MarketplaceManifest =
                serde_json::from_str(&content).context("Failed to parse manifest.json")?;
            return Ok(manifest);
        }
    }

    Err(anyhow::anyhow!("Package does not contain a manifest.json"))
}

/// Extract a gzipped tarball to a directory. Returns total extracted size.
fn extract_tarball(data: &[u8], dest: &Path) -> Result<u64> {
    let decoder = flate2::read::GzDecoder::new(data);
    let mut archive = tar::Archive::new(decoder);
    let mut total_size = 0u64;

    for entry in archive
        .entries()
        .context("Failed to read tarball entries")?
    {
        let mut entry = entry.context("Failed to read tarball entry")?;
        let path = entry
            .path()
            .context("Failed to get entry path")?
            .to_path_buf();

        // Security: prevent path traversal
        if path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            warn!(
                "Skipping tarball entry with path traversal: {}",
                path.display()
            );
            continue;
        }

        let dest_path = dest.join(&path);
        if let Some(parent) = dest_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        total_size += entry.size();
        entry.unpack(&dest_path).with_context(|| {
            format!(
                "Failed to extract {} to {}",
                path.display(),
                dest_path.display()
            )
        })?;
    }

    Ok(total_size)
}

/// Count files recursively in a directory.
fn count_files(dir: &Path) -> usize {
    let mut count = 0;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                count += count_files(&path);
            } else {
                count += 1;
            }
        }
    }
    count
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::marketplace::{MarketplaceCategory, PublisherInfo};
    use agnos_common::AgentManifest;

    fn create_test_tarball(manifest: &MarketplaceManifest) -> Vec<u8> {
        let mut builder = tar::Builder::new(Vec::new());

        // Add manifest.json
        let manifest_json = serde_json::to_string_pretty(manifest).unwrap();
        let manifest_bytes = manifest_json.as_bytes();
        let mut header = tar::Header::new_gnu();
        header.set_size(manifest_bytes.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder
            .append_data(&mut header, "manifest.json", manifest_bytes)
            .unwrap();

        // Add a dummy binary
        let binary = b"#!/bin/sh\necho hello\n";
        let mut header = tar::Header::new_gnu();
        header.set_size(binary.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        builder
            .append_data(&mut header, "bin/test-agent", &binary[..])
            .unwrap();

        let tar_data = builder.into_inner().unwrap();

        // Gzip it
        use flate2::write::GzEncoder;
        use std::io::Write;
        let mut encoder = GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(&tar_data).unwrap();
        encoder.finish().unwrap()
    }

    fn sample_manifest() -> MarketplaceManifest {
        MarketplaceManifest {
            agent: AgentManifest {
                name: "test-pkg".to_string(),
                description: "Test package".to_string(),
                version: "1.0.0".to_string(),
                ..Default::default()
            },
            publisher: PublisherInfo {
                name: "Test Publisher".to_string(),
                key_id: "abc12345".to_string(),
                homepage: String::new(),
            },
            category: MarketplaceCategory::Utility,
            runtime: "native".to_string(),
            screenshots: vec![],
            changelog: String::new(),
            min_agnos_version: String::new(),
            dependencies: HashMap::new(),
            tags: vec!["test".to_string()],
        }
    }

    #[test]
    fn test_local_registry_new_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let registry = LocalRegistry::new(dir.path()).unwrap();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn test_install_and_list() {
        let dir = tempfile::tempdir().unwrap();
        let mut registry = LocalRegistry::new(dir.path()).unwrap();

        let manifest = sample_manifest();
        let tarball = create_test_tarball(&manifest);
        let tarball_path = dir.path().join("test-pkg.agnos-agent");
        std::fs::write(&tarball_path, &tarball).unwrap();

        let result = registry.install_package(&tarball_path, None).unwrap();
        assert_eq!(result.name, "test-pkg");
        assert_eq!(result.version, "1.0.0");
        assert!(result.upgraded_from.is_none());

        assert_eq!(registry.len(), 1);
        let listed = registry.list_installed();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name(), "test-pkg");
    }

    #[test]
    fn test_install_and_get() {
        let dir = tempfile::tempdir().unwrap();
        let mut registry = LocalRegistry::new(dir.path()).unwrap();

        let manifest = sample_manifest();
        let tarball = create_test_tarball(&manifest);
        let tarball_path = dir.path().join("test.agnos-agent");
        std::fs::write(&tarball_path, &tarball).unwrap();

        registry.install_package(&tarball_path, None).unwrap();

        let pkg = registry.get_package("test-pkg").unwrap();
        assert_eq!(pkg.version(), "1.0.0");
        assert_eq!(pkg.publisher(), "Test Publisher");
    }

    #[test]
    fn test_install_upgrade() {
        let dir = tempfile::tempdir().unwrap();
        let mut registry = LocalRegistry::new(dir.path()).unwrap();

        let mut manifest = sample_manifest();
        let tarball = create_test_tarball(&manifest);
        let tarball_path = dir.path().join("v1.agnos-agent");
        std::fs::write(&tarball_path, &tarball).unwrap();
        registry.install_package(&tarball_path, None).unwrap();

        // Upgrade
        manifest.agent.version = "2.0.0".to_string();
        let tarball2 = create_test_tarball(&manifest);
        let tarball_path2 = dir.path().join("v2.agnos-agent");
        std::fs::write(&tarball_path2, &tarball2).unwrap();

        let result = registry.install_package(&tarball_path2, None).unwrap();
        assert_eq!(result.upgraded_from, Some("1.0.0".to_string()));
        assert_eq!(result.version, "2.0.0");
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn test_uninstall() {
        let dir = tempfile::tempdir().unwrap();
        let mut registry = LocalRegistry::new(dir.path()).unwrap();

        let manifest = sample_manifest();
        let tarball = create_test_tarball(&manifest);
        let tarball_path = dir.path().join("test.agnos-agent");
        std::fs::write(&tarball_path, &tarball).unwrap();
        registry.install_package(&tarball_path, None).unwrap();

        let result = registry.uninstall_package("test-pkg").unwrap();
        assert_eq!(result.name, "test-pkg");
        assert_eq!(result.version, "1.0.0");
        assert!(registry.is_empty());
    }

    #[test]
    fn test_uninstall_nonexistent() {
        let dir = tempfile::tempdir().unwrap();
        let mut registry = LocalRegistry::new(dir.path()).unwrap();
        assert!(registry.uninstall_package("nonexistent").is_err());
    }

    #[test]
    fn test_search() {
        let dir = tempfile::tempdir().unwrap();
        let mut registry = LocalRegistry::new(dir.path()).unwrap();

        let manifest = sample_manifest();
        let tarball = create_test_tarball(&manifest);
        let tarball_path = dir.path().join("test.agnos-agent");
        std::fs::write(&tarball_path, &tarball).unwrap();
        registry.install_package(&tarball_path, None).unwrap();

        // Search by name
        assert_eq!(registry.search("test").len(), 1);
        // Search by tag
        assert_eq!(registry.search("test").len(), 1);
        // No match
        assert_eq!(registry.search("zzz").len(), 0);
    }

    #[test]
    fn test_storage_quota() {
        let dir = tempfile::tempdir().unwrap();
        let mut registry = LocalRegistry::new(dir.path()).unwrap();
        registry.set_storage_quota(1); // 1 byte quota

        let manifest = sample_manifest();
        let tarball = create_test_tarball(&manifest);
        let tarball_path = dir.path().join("test.agnos-agent");
        std::fs::write(&tarball_path, &tarball).unwrap();

        let result = registry.install_package(&tarball_path, None);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("quota"));
    }

    #[test]
    fn test_installed_package_accessors() {
        let pkg = InstalledMarketplacePackage {
            manifest: sample_manifest(),
            installed_at: Utc::now(),
            install_dir: PathBuf::from("/tmp/test"),
            package_hash: "abc".to_string(),
            auto_update: false,
            installed_size: 1024,
        };
        assert_eq!(pkg.name(), "test-pkg");
        assert_eq!(pkg.version(), "1.0.0");
        assert_eq!(pkg.publisher(), "Test Publisher");
    }

    #[test]
    fn test_total_installed_size() {
        let dir = tempfile::tempdir().unwrap();
        let mut registry = LocalRegistry::new(dir.path()).unwrap();

        let manifest = sample_manifest();
        let tarball = create_test_tarball(&manifest);
        let tarball_path = dir.path().join("test.agnos-agent");
        std::fs::write(&tarball_path, &tarball).unwrap();
        registry.install_package(&tarball_path, None).unwrap();

        assert!(registry.total_installed_size() > 0);
    }

    #[test]
    fn test_persistence() {
        let dir = tempfile::tempdir().unwrap();

        // Install a package
        {
            let mut registry = LocalRegistry::new(dir.path()).unwrap();
            let manifest = sample_manifest();
            let tarball = create_test_tarball(&manifest);
            let tarball_path = dir.path().join("test.agnos-agent");
            std::fs::write(&tarball_path, &tarball).unwrap();
            registry.install_package(&tarball_path, None).unwrap();
        }

        // Reload and verify
        {
            let registry = LocalRegistry::new(dir.path()).unwrap();
            assert_eq!(registry.len(), 1);
            assert!(registry.get_package("test-pkg").is_some());
        }
    }

    #[test]
    fn test_invalid_manifest_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let mut registry = LocalRegistry::new(dir.path()).unwrap();

        let mut manifest = sample_manifest();
        manifest.agent.name = String::new(); // Invalid: empty name
        let tarball = create_test_tarball(&manifest);
        let tarball_path = dir.path().join("bad.agnos-agent");
        std::fs::write(&tarball_path, &tarball).unwrap();

        assert!(registry.install_package(&tarball_path, None).is_err());
    }

    #[test]
    fn test_count_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "a").unwrap();
        std::fs::write(dir.path().join("b.txt"), "b").unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("sub/c.txt"), "c").unwrap();
        assert_eq!(count_files(dir.path()), 3);
    }

    #[test]
    fn test_extract_manifest_from_tarball() {
        let manifest = sample_manifest();
        let tarball = create_test_tarball(&manifest);
        let extracted = extract_manifest_from_tarball(&tarball).unwrap();
        assert_eq!(extracted.agent.name, "test-pkg");
    }

    // -----------------------------------------------------------------------
    // Signature verification integration tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_install_with_valid_signature() {
        let dir = tempfile::tempdir().unwrap();
        let mut registry = LocalRegistry::new(dir.path()).unwrap();

        // Generate a keypair and build a keyring
        let (sk, vk, key_id) = trust::generate_keypair();
        let keys_dir = tempfile::tempdir().unwrap();
        let mut keyring = trust::PublisherKeyring::new(keys_dir.path());
        keyring.add_key(trust::KeyVersion {
            key_id: key_id.clone(),
            valid_from: chrono::Utc::now() - chrono::Duration::hours(1),
            valid_until: None,
            public_key_hex: trust::key_id_from_verifying_key(&vk)
                .chars()
                .collect::<String>(), // this is just the first 8 bytes; we need full key
        });
        // Actually add the full public key hex
        let mut keyring = trust::PublisherKeyring::new(keys_dir.path());
        let full_hex: String = vk.to_bytes().iter().map(|b| format!("{:02x}", b)).collect();
        keyring.add_key(trust::KeyVersion {
            key_id: key_id.clone(),
            valid_from: chrono::Utc::now() - chrono::Duration::hours(1),
            valid_until: None,
            public_key_hex: full_hex,
        });

        // Create manifest with matching key_id
        let mut manifest = sample_manifest();
        manifest.publisher.key_id = key_id;

        let tarball = create_test_tarball(&manifest);
        let tarball_path = dir.path().join("signed.agnos-agent");
        std::fs::write(&tarball_path, &tarball).unwrap();

        // Create signature sidecar
        let sig = trust::sign_data(&tarball, &sk);
        let sig_path = tarball_path.with_extension("sig");
        std::fs::write(&sig_path, &sig).unwrap();

        let result = registry.install_package(&tarball_path, Some(&keyring));
        assert!(
            result.is_ok(),
            "Install with valid signature should succeed"
        );
        assert_eq!(result.unwrap().name, "test-pkg");
    }

    #[test]
    fn test_install_with_invalid_signature() {
        let dir = tempfile::tempdir().unwrap();
        let mut registry = LocalRegistry::new(dir.path()).unwrap();

        let (sk, _vk, key_id) = trust::generate_keypair();
        let (_sk2, vk2, _key_id2) = trust::generate_keypair();

        // Keyring has a different key than the one that signed
        let keys_dir = tempfile::tempdir().unwrap();
        let mut keyring = trust::PublisherKeyring::new(keys_dir.path());
        let full_hex: String = vk2
            .to_bytes()
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect();
        keyring.add_key(trust::KeyVersion {
            key_id: key_id.clone(),
            valid_from: chrono::Utc::now() - chrono::Duration::hours(1),
            valid_until: None,
            public_key_hex: full_hex,
        });

        let mut manifest = sample_manifest();
        manifest.publisher.key_id = key_id;
        let tarball = create_test_tarball(&manifest);
        let tarball_path = dir.path().join("badsig.agnos-agent");
        std::fs::write(&tarball_path, &tarball).unwrap();

        // Sign with sk but keyring has vk2
        let sig = trust::sign_data(&tarball, &sk);
        let sig_path = tarball_path.with_extension("sig");
        std::fs::write(&sig_path, &sig).unwrap();

        let result = registry.install_package(&tarball_path, Some(&keyring));
        assert!(
            result.is_err(),
            "Install with invalid signature should fail"
        );
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("verification failed"),
            "Error should mention verification failure"
        );
    }

    #[test]
    fn test_install_with_missing_signature_file() {
        let dir = tempfile::tempdir().unwrap();
        let mut registry = LocalRegistry::new(dir.path()).unwrap();

        let (_sk, vk, key_id) = trust::generate_keypair();
        let keys_dir = tempfile::tempdir().unwrap();
        let mut keyring = trust::PublisherKeyring::new(keys_dir.path());
        let full_hex: String = vk.to_bytes().iter().map(|b| format!("{:02x}", b)).collect();
        keyring.add_key(trust::KeyVersion {
            key_id: key_id.clone(),
            valid_from: chrono::Utc::now() - chrono::Duration::hours(1),
            valid_until: None,
            public_key_hex: full_hex,
        });

        let mut manifest = sample_manifest();
        manifest.publisher.key_id = key_id;
        let tarball = create_test_tarball(&manifest);
        let tarball_path = dir.path().join("nosig.agnos-agent");
        std::fs::write(&tarball_path, &tarball).unwrap();
        // No .sig file written

        let result = registry.install_package(&tarball_path, Some(&keyring));
        assert!(
            result.is_err(),
            "Install without signature file should fail"
        );
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Signature file not found"),
            "Error should mention missing signature file"
        );
    }

    #[test]
    fn test_install_with_unknown_key_id() {
        let dir = tempfile::tempdir().unwrap();
        let mut registry = LocalRegistry::new(dir.path()).unwrap();

        // Empty keyring — no keys loaded
        let keys_dir = tempfile::tempdir().unwrap();
        let keyring = trust::PublisherKeyring::new(keys_dir.path());

        let manifest = sample_manifest(); // key_id = "abc12345"
        let tarball = create_test_tarball(&manifest);
        let tarball_path = dir.path().join("unknown.agnos-agent");
        std::fs::write(&tarball_path, &tarball).unwrap();

        let result = registry.install_package(&tarball_path, Some(&keyring));
        assert!(result.is_err(), "Install with unknown key_id should fail");
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("No trusted key found"),
            "Error should mention missing key"
        );
    }

    #[test]
    fn test_install_without_keyring_skips_verification() {
        // This is the dev-mode path: no keyring = no verification
        let dir = tempfile::tempdir().unwrap();
        let mut registry = LocalRegistry::new(dir.path()).unwrap();

        let manifest = sample_manifest();
        let tarball = create_test_tarball(&manifest);
        let tarball_path = dir.path().join("devmode.agnos-agent");
        std::fs::write(&tarball_path, &tarball).unwrap();

        let result = registry.install_package(&tarball_path, None);
        assert!(
            result.is_ok(),
            "Install without keyring should succeed (dev mode)"
        );
    }
}
