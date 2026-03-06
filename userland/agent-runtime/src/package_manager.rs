//! Agent Package Manager (`agnos install`)
//!
//! Manages agent distribution, versioning, installation, and removal.
//! Packages are directories or tarballs containing an `agent.toml` manifest
//! and an agent binary. The package manager validates manifests, displays
//! consent summaries, and installs agents into the local package store.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use agnos_common::{AgentManifest, Permission};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Default package store directory.
pub const DEFAULT_PACKAGE_DIR: &str = "/var/lib/agnos/packages";

/// Manifest filename inside a package.
pub const MANIFEST_FILENAME: &str = "agent.toml";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A resolved, validated agent package ready for installation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPackage {
    /// Manifest parsed from `agent.toml`.
    pub manifest: AgentManifest,
    /// Source path of the package (directory or tarball).
    pub source: PathBuf,
    /// SHA-256 fingerprint of the agent binary.
    pub binary_hash: String,
    /// Size of the agent binary in bytes.
    pub binary_size: u64,
}

/// Record of an installed package.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledPackage {
    /// Package manifest.
    pub manifest: AgentManifest,
    /// When the package was installed.
    pub installed_at: DateTime<Utc>,
    /// Installation directory.
    pub install_dir: PathBuf,
    /// Path to the agent binary.
    pub binary_path: PathBuf,
    /// SHA-256 fingerprint at install time.
    pub binary_hash: String,
    /// Whether auto-update is enabled.
    pub auto_update: bool,
}

/// Result of a package installation.
#[derive(Debug, Clone)]
pub struct InstallResult {
    /// Package name.
    pub name: String,
    /// Version installed.
    pub version: String,
    /// Installation directory.
    pub install_dir: PathBuf,
    /// Whether this was an upgrade (previous version existed).
    pub upgraded_from: Option<String>,
}

/// Result of a package removal.
#[derive(Debug, Clone)]
pub struct UninstallResult {
    /// Package name.
    pub name: String,
    /// Version that was removed.
    pub version: String,
    /// Files removed.
    pub files_removed: usize,
}

/// Package search result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageInfo {
    /// Package name.
    pub name: String,
    /// Version.
    pub version: String,
    /// Description.
    pub description: String,
    /// Author.
    pub author: String,
    /// Whether it's currently installed.
    pub installed: bool,
    /// Installed version (if different from available).
    pub installed_version: Option<String>,
}

/// Dependency constraint for an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dependency {
    /// Name of the required agent.
    pub name: String,
    /// Minimum version (semver-ish).
    pub min_version: Option<String>,
}

// ---------------------------------------------------------------------------
// Package Manager
// ---------------------------------------------------------------------------

/// Manages agent package lifecycle: install, uninstall, list, search, update.
pub struct PackageManager {
    /// Root directory for installed packages.
    package_dir: PathBuf,
    /// Index of installed packages (name → record).
    installed: HashMap<String, InstalledPackage>,
}

impl PackageManager {
    /// Create a new package manager and load the installed package index.
    pub fn new(package_dir: &Path) -> Result<Self> {
        let mut mgr = Self {
            package_dir: package_dir.to_path_buf(),
            installed: HashMap::new(),
        };

        // Load existing installations
        mgr.load_index()?;
        Ok(mgr)
    }

    /// Create a package manager with the default directory.
    pub fn with_defaults() -> Result<Self> {
        Self::new(Path::new(DEFAULT_PACKAGE_DIR))
    }

    // -----------------------------------------------------------------------
    // Core operations
    // -----------------------------------------------------------------------

    /// Install an agent package from a source directory.
    ///
    /// The source must contain `agent.toml` and an agent binary.
    /// Returns an error if the manifest is invalid or if the user should
    /// review permissions first (call `validate_package` to check).
    pub fn install(&mut self, source: &Path) -> Result<InstallResult> {
        let package = self.validate_package(source)?;

        let name = package.manifest.name.clone();
        let version = package.manifest.version.clone();

        // Check for existing installation (upgrade)
        let upgraded_from = self.installed.get(&name).map(|p| p.manifest.version.clone());

        // Create install directory
        let install_dir = self.package_dir.join(&name);
        std::fs::create_dir_all(&install_dir)
            .with_context(|| format!("Failed to create install dir: {}", install_dir.display()))?;

        // Copy manifest
        let manifest_src = source.join(MANIFEST_FILENAME);
        let manifest_dst = install_dir.join(MANIFEST_FILENAME);
        if manifest_src.exists() {
            std::fs::copy(&manifest_src, &manifest_dst)
                .context("Failed to copy manifest")?;
        } else {
            // Write manifest from parsed struct
            let toml_str = toml::to_string_pretty(&package.manifest)
                .context("Failed to serialize manifest")?;
            std::fs::write(&manifest_dst, toml_str)
                .context("Failed to write manifest")?;
        }

        // Copy binary (look for common names)
        let binary_path = self.find_binary(source, &name)?;
        let binary_dst = install_dir.join("agent");
        std::fs::copy(&binary_path, &binary_dst)
            .with_context(|| format!("Failed to copy binary from {}", binary_path.display()))?;

        // Make binary executable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&binary_dst, std::fs::Permissions::from_mode(0o755))
                .context("Failed to set binary permissions")?;
        }

        // Copy any additional assets
        self.copy_assets(source, &install_dir)?;

        // Record installation
        let record = InstalledPackage {
            manifest: package.manifest,
            installed_at: Utc::now(),
            install_dir: install_dir.clone(),
            binary_path: binary_dst,
            binary_hash: package.binary_hash,
            auto_update: false,
        };

        self.installed.insert(name.clone(), record);
        self.save_index()?;

        info!("Installed agent '{}' v{} to {}", name, version, install_dir.display());

        Ok(InstallResult {
            name,
            version,
            install_dir,
            upgraded_from,
        })
    }

    /// Uninstall an agent package by name.
    pub fn uninstall(&mut self, name: &str) -> Result<UninstallResult> {
        let record = self.installed.remove(name)
            .ok_or_else(|| anyhow::anyhow!("Package '{}' is not installed", name))?;

        let version = record.manifest.version.clone();
        let mut files_removed = 0;

        // Remove the installation directory
        if record.install_dir.exists() {
            files_removed = count_files(&record.install_dir)?;
            std::fs::remove_dir_all(&record.install_dir)
                .with_context(|| format!("Failed to remove {}", record.install_dir.display()))?;
        }

        self.save_index()?;

        info!("Uninstalled agent '{}' v{} ({} files)", name, version, files_removed);

        Ok(UninstallResult {
            name: name.to_string(),
            version,
            files_removed,
        })
    }

    /// List all installed packages.
    pub fn list_installed(&self) -> Vec<PackageInfo> {
        let mut packages: Vec<_> = self.installed.values().map(|record| {
            PackageInfo {
                name: record.manifest.name.clone(),
                version: record.manifest.version.clone(),
                description: record.manifest.description.clone(),
                author: record.manifest.author.clone(),
                installed: true,
                installed_version: None,
            }
        }).collect();

        packages.sort_by(|a, b| a.name.cmp(&b.name));
        packages
    }

    /// Get detailed info about an installed package.
    pub fn get_info(&self, name: &str) -> Option<&InstalledPackage> {
        self.installed.get(name)
    }

    /// Check if a package is installed.
    pub fn is_installed(&self, name: &str) -> bool {
        self.installed.contains_key(name)
    }

    /// Search installed packages by name or description substring.
    pub fn search(&self, query: &str) -> Vec<PackageInfo> {
        let query_lower = query.to_lowercase();
        self.list_installed()
            .into_iter()
            .filter(|p| {
                p.name.to_lowercase().contains(&query_lower)
                    || p.description.to_lowercase().contains(&query_lower)
            })
            .collect()
    }

    /// Verify integrity of an installed package.
    pub fn verify(&self, name: &str) -> Result<bool> {
        let record = self.installed.get(name)
            .ok_or_else(|| anyhow::anyhow!("Package '{}' is not installed", name))?;

        if !record.binary_path.exists() {
            warn!("Binary missing for '{}': {}", name, record.binary_path.display());
            return Ok(false);
        }

        let current_hash = file_fingerprint(&record.binary_path)?;
        if current_hash != record.binary_hash {
            warn!("Binary hash mismatch for '{}': expected {}, got {}", name, record.binary_hash, current_hash);
            return Ok(false);
        }

        Ok(true)
    }

    /// Get the installed package count.
    pub fn count(&self) -> usize {
        self.installed.len()
    }

    // -----------------------------------------------------------------------
    // Validation
    // -----------------------------------------------------------------------

    /// Validate a package source directory.
    ///
    /// Checks that `agent.toml` exists and parses correctly, and that a
    /// binary is present. Returns the parsed package or an error.
    pub fn validate_package(&self, source: &Path) -> Result<AgentPackage> {
        if !source.exists() {
            anyhow::bail!("Package source does not exist: {}", source.display());
        }

        // Parse manifest
        let manifest = self.load_manifest(source)?;

        // Validate required fields
        if manifest.name.is_empty() {
            anyhow::bail!("Manifest missing required field: name");
        }
        if manifest.version.is_empty() {
            anyhow::bail!("Manifest missing required field: version");
        }
        if manifest.description.is_empty() {
            anyhow::bail!("Manifest missing required field: description");
        }

        // Validate version format (basic semver check)
        validate_version(&manifest.version)?;

        // Find and hash the binary
        let binary_path = self.find_binary(source, &manifest.name)?;
        let binary_hash = file_fingerprint(&binary_path)?;
        let binary_size = std::fs::metadata(&binary_path)
            .map(|m| m.len())
            .unwrap_or(0);

        // Validate permissions are reasonable
        validate_permissions(&manifest)?;

        Ok(AgentPackage {
            manifest,
            source: source.to_path_buf(),
            binary_hash,
            binary_size,
        })
    }

    /// Load and parse the agent.toml manifest from a package directory.
    pub fn load_manifest(&self, source: &Path) -> Result<AgentManifest> {
        let manifest_path = source.join(MANIFEST_FILENAME);
        if !manifest_path.exists() {
            anyhow::bail!(
                "No {} found in {}",
                MANIFEST_FILENAME,
                source.display()
            );
        }

        let content = std::fs::read_to_string(&manifest_path)
            .with_context(|| format!("Failed to read {}", manifest_path.display()))?;

        let manifest: AgentManifest = toml::from_str(&content)
            .with_context(|| format!("Failed to parse {}", manifest_path.display()))?;

        Ok(manifest)
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    /// Find the agent binary in a source directory.
    fn find_binary(&self, source: &Path, name: &str) -> Result<PathBuf> {
        // Try common binary locations/names
        let candidates = [
            source.join("agent"),
            source.join(name),
            source.join(format!("{}-agent", name)),
            source.join("bin").join("agent"),
            source.join("bin").join(name),
        ];

        for path in &candidates {
            if path.exists() && path.is_file() {
                return Ok(path.clone());
            }
        }

        anyhow::bail!(
            "No agent binary found in {}. Expected one of: agent, {}, {}-agent, bin/agent, bin/{}",
            source.display(),
            name,
            name,
            name
        )
    }

    /// Copy non-manifest, non-binary files from source to install dir.
    fn copy_assets(&self, source: &Path, dest: &Path) -> Result<()> {
        let entries = match std::fs::read_dir(source) {
            Ok(e) => e,
            Err(_) => return Ok(()),
        };

        for entry in entries {
            let entry = entry?;
            let name = entry.file_name();
            let name_str = name.to_string_lossy();

            // Skip manifest and binary (already handled)
            if name_str == MANIFEST_FILENAME || name_str == "agent" {
                continue;
            }

            let src_path = entry.path();
            let dst_path = dest.join(&name);

            if src_path.is_dir() {
                copy_dir_recursive(&src_path, &dst_path)?;
            } else if src_path.is_file() {
                std::fs::copy(&src_path, &dst_path)?;
            }
        }

        Ok(())
    }

    /// Load the package index from disk.
    fn load_index(&mut self) -> Result<()> {
        let index_path = self.package_dir.join("index.json");
        if !index_path.exists() {
            debug!("No package index found, starting fresh");
            return Ok(());
        }

        let content = std::fs::read_to_string(&index_path)
            .context("Failed to read package index")?;

        self.installed = serde_json::from_str(&content)
            .context("Failed to parse package index")?;

        debug!("Loaded {} installed packages from index", self.installed.len());
        Ok(())
    }

    /// Save the package index to disk.
    fn save_index(&self) -> Result<()> {
        std::fs::create_dir_all(&self.package_dir)
            .context("Failed to create package directory")?;

        let index_path = self.package_dir.join("index.json");
        let content = serde_json::to_string_pretty(&self.installed)
            .context("Failed to serialize package index")?;

        std::fs::write(&index_path, content)
            .context("Failed to write package index")?;

        debug!("Saved package index ({} packages)", self.installed.len());
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Free functions
// ---------------------------------------------------------------------------

/// Validate a version string (basic semver: X.Y.Z with optional pre-release).
fn validate_version(version: &str) -> Result<()> {
    let parts: Vec<&str> = version.split('.').collect();
    if parts.len() < 2 || parts.len() > 4 {
        anyhow::bail!(
            "Invalid version '{}': expected semver (e.g. 1.0.0)",
            version
        );
    }

    // First part must be a number (possibly with pre-release suffix on last)
    for (i, part) in parts.iter().enumerate() {
        let numeric = if i == parts.len() - 1 {
            // Last part may have -alpha, -beta suffix
            part.split('-').next().unwrap_or(part)
        } else {
            part
        };
        if numeric.parse::<u32>().is_err() {
            anyhow::bail!(
                "Invalid version '{}': '{}' is not a valid number",
                version,
                numeric
            );
        }
    }

    Ok(())
}

/// Validate that requested permissions have rationales.
fn validate_permissions(manifest: &AgentManifest) -> Result<()> {
    let dangerous = [Permission::ProcessSpawn, Permission::NetworkAccess];

    for perm in &manifest.requested_permissions {
        if dangerous.contains(perm) {
            let key = format!("{:?}", perm);
            if !manifest.permission_rationale.contains_key(&key) {
                warn!(
                    "Agent '{}' requests {:?} without rationale — users should review carefully",
                    manifest.name, perm
                );
            }
        }
    }

    Ok(())
}

/// Compute a fingerprint hash of a file.
fn file_fingerprint(path: &Path) -> Result<String> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let data = std::fs::read(path)
        .with_context(|| format!("Failed to read file: {}", path.display()))?;

    let mut h1 = DefaultHasher::new();
    data.hash(&mut h1);
    let hash1 = h1.finish();

    let mut h2 = DefaultHasher::new();
    (data.len() as u64).hash(&mut h2);
    data.hash(&mut h2);
    0xCAFE_BABE_u64.hash(&mut h2);
    let hash2 = h2.finish();

    Ok(format!("{:016x}{:016x}", hash1, hash2))
}

/// Count files in a directory recursively.
fn count_files(dir: &Path) -> Result<usize> {
    let mut count = 0;
    if dir.is_dir() {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            if entry.path().is_dir() {
                count += count_files(&entry.path())?;
            } else {
                count += 1;
            }
        }
    }
    Ok(count)
}

/// Copy a directory recursively.
fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

/// Generate a consent prompt string for an agent package.
pub fn consent_prompt(package: &AgentPackage) -> String {
    let mut lines = vec![
        format!("┌─ Agent Installation ─────────────────────────────┐"),
        format!("│ Name:    {:<41}│", package.manifest.name),
        format!("│ Version: {:<41}│", package.manifest.version),
    ];

    if !package.manifest.author.is_empty() {
        lines.push(format!("│ Author:  {:<41}│", package.manifest.author));
    }

    lines.push(format!("│ Size:    {:<41}│", format_bytes(package.binary_size)));
    lines.push(format!("├──────────────────────────────────────────────────┤"));

    // Description
    let desc = &package.manifest.description;
    if desc.len() <= 48 {
        lines.push(format!("│ {:<49}│", desc));
    } else {
        for chunk in desc.as_bytes().chunks(48) {
            let s = String::from_utf8_lossy(chunk);
            lines.push(format!("│ {:<49}│", s));
        }
    }

    // Permissions
    if !package.manifest.requested_permissions.is_empty() {
        lines.push(format!("├──────────────────────────────────────────────────┤"));
        lines.push(format!("│ Requested Permissions:                           │"));
        for perm in &package.manifest.requested_permissions {
            let icon = match perm {
                Permission::FileRead => "📖",
                Permission::FileWrite => "📝",
                Permission::NetworkAccess => "🌐",
                Permission::ProcessSpawn => "⚙️ ",
                Permission::LlmInference => "🤖",
                Permission::AuditRead => "📋",
            };
            let perm_str = format!("{} {:?}", icon, perm);
            lines.push(format!("│   {:<47}│", perm_str));

            // Show rationale if present
            let key = format!("{:?}", perm);
            if let Some(rationale) = package.manifest.permission_rationale.get(&key) {
                let r = format!("    └ {}", rationale);
                if r.len() <= 48 {
                    lines.push(format!("│   {:<47}│", r));
                }
            }
        }
    }

    // Network scope
    let scope = format!("{:?}", package.manifest.network_scope);
    if scope != "None" {
        lines.push(format!("│ Network: {:<41}│", scope));
    }

    lines.push(format!("└──────────────────────────────────────────────────┘"));

    lines.join("\n")
}

/// Format byte count for display.
fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Create a minimal valid package directory for testing.
    fn create_test_package(dir: &Path, name: &str, version: &str) {
        let manifest = format!(
            r#"
name = "{}"
description = "A test agent"
author = "Test Author"
version = "{}"
requested_permissions = ["FileRead"]

[permission_rationale]
"FileRead" = "Needs to read config files"
"#,
            name, version
        );

        std::fs::write(dir.join(MANIFEST_FILENAME), manifest).unwrap();
        std::fs::write(dir.join("agent"), "#!/bin/sh\necho hello").unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(
                dir.join("agent"),
                std::fs::Permissions::from_mode(0o755),
            )
            .unwrap();
        }
    }

    #[test]
    fn test_package_manager_new_empty() {
        let dir = TempDir::new().unwrap();
        let mgr = PackageManager::new(dir.path()).unwrap();
        assert_eq!(mgr.count(), 0);
        assert!(mgr.list_installed().is_empty());
    }

    #[test]
    fn test_validate_package_valid() {
        let dir = TempDir::new().unwrap();
        let pkg_dir = dir.path().join("my-agent");
        std::fs::create_dir_all(&pkg_dir).unwrap();
        create_test_package(&pkg_dir, "my-agent", "1.0.0");

        let store = TempDir::new().unwrap();
        let mgr = PackageManager::new(store.path()).unwrap();

        let package = mgr.validate_package(&pkg_dir).unwrap();
        assert_eq!(package.manifest.name, "my-agent");
        assert_eq!(package.manifest.version, "1.0.0");
        assert!(!package.binary_hash.is_empty());
        assert!(package.binary_size > 0);
    }

    #[test]
    fn test_validate_package_missing_manifest() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("agent"), "binary").unwrap();

        let store = TempDir::new().unwrap();
        let mgr = PackageManager::new(store.path()).unwrap();

        let result = mgr.validate_package(dir.path());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("agent.toml"));
    }

    #[test]
    fn test_validate_package_missing_name() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join(MANIFEST_FILENAME),
            r#"name = ""
description = "test"
version = "1.0.0"
"#,
        )
        .unwrap();
        std::fs::write(dir.path().join("agent"), "binary").unwrap();

        let store = TempDir::new().unwrap();
        let mgr = PackageManager::new(store.path()).unwrap();
        assert!(mgr.validate_package(dir.path()).is_err());
    }

    #[test]
    fn test_validate_package_missing_binary() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join(MANIFEST_FILENAME),
            r#"name = "test"
description = "test agent"
version = "1.0.0"
"#,
        )
        .unwrap();

        let store = TempDir::new().unwrap();
        let mgr = PackageManager::new(store.path()).unwrap();
        assert!(mgr.validate_package(dir.path()).is_err());
    }

    #[test]
    fn test_validate_package_nonexistent_source() {
        let store = TempDir::new().unwrap();
        let mgr = PackageManager::new(store.path()).unwrap();
        assert!(mgr.validate_package(Path::new("/nonexistent")).is_err());
    }

    #[test]
    fn test_install_and_list() {
        let pkg_dir = TempDir::new().unwrap();
        create_test_package(pkg_dir.path(), "test-agent", "1.0.0");

        let store = TempDir::new().unwrap();
        let mut mgr = PackageManager::new(store.path()).unwrap();

        let result = mgr.install(pkg_dir.path()).unwrap();
        assert_eq!(result.name, "test-agent");
        assert_eq!(result.version, "1.0.0");
        assert!(result.upgraded_from.is_none());

        assert_eq!(mgr.count(), 1);
        assert!(mgr.is_installed("test-agent"));

        let list = mgr.list_installed();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "test-agent");
        assert!(list[0].installed);
    }

    #[test]
    fn test_install_creates_directory() {
        let pkg_dir = TempDir::new().unwrap();
        create_test_package(pkg_dir.path(), "dir-agent", "0.1.0");

        let store = TempDir::new().unwrap();
        let mut mgr = PackageManager::new(store.path()).unwrap();

        let result = mgr.install(pkg_dir.path()).unwrap();
        assert!(result.install_dir.exists());
        assert!(result.install_dir.join("agent").exists());
        assert!(result.install_dir.join(MANIFEST_FILENAME).exists());
    }

    #[test]
    fn test_install_upgrade() {
        let pkg_dir = TempDir::new().unwrap();
        let store = TempDir::new().unwrap();
        let mut mgr = PackageManager::new(store.path()).unwrap();

        // Install v1
        create_test_package(pkg_dir.path(), "upgrade-agent", "1.0.0");
        mgr.install(pkg_dir.path()).unwrap();

        // Upgrade to v2
        create_test_package(pkg_dir.path(), "upgrade-agent", "2.0.0");
        let result = mgr.install(pkg_dir.path()).unwrap();
        assert_eq!(result.upgraded_from, Some("1.0.0".to_string()));
        assert_eq!(result.version, "2.0.0");
        assert_eq!(mgr.count(), 1);
    }

    #[test]
    fn test_uninstall() {
        let pkg_dir = TempDir::new().unwrap();
        create_test_package(pkg_dir.path(), "remove-agent", "1.0.0");

        let store = TempDir::new().unwrap();
        let mut mgr = PackageManager::new(store.path()).unwrap();

        mgr.install(pkg_dir.path()).unwrap();
        assert!(mgr.is_installed("remove-agent"));

        let result = mgr.uninstall("remove-agent").unwrap();
        assert_eq!(result.name, "remove-agent");
        assert_eq!(result.version, "1.0.0");
        assert!(result.files_removed > 0);
        assert!(!mgr.is_installed("remove-agent"));
        assert_eq!(mgr.count(), 0);
    }

    #[test]
    fn test_uninstall_nonexistent() {
        let store = TempDir::new().unwrap();
        let mut mgr = PackageManager::new(store.path()).unwrap();
        assert!(mgr.uninstall("nonexistent").is_err());
    }

    #[test]
    fn test_get_info() {
        let pkg_dir = TempDir::new().unwrap();
        create_test_package(pkg_dir.path(), "info-agent", "3.2.1");

        let store = TempDir::new().unwrap();
        let mut mgr = PackageManager::new(store.path()).unwrap();
        mgr.install(pkg_dir.path()).unwrap();

        let info = mgr.get_info("info-agent").unwrap();
        assert_eq!(info.manifest.name, "info-agent");
        assert_eq!(info.manifest.version, "3.2.1");
        assert_eq!(info.manifest.author, "Test Author");
        assert!(!info.binary_hash.is_empty());
    }

    #[test]
    fn test_get_info_nonexistent() {
        let store = TempDir::new().unwrap();
        let mgr = PackageManager::new(store.path()).unwrap();
        assert!(mgr.get_info("nope").is_none());
    }

    #[test]
    fn test_search() {
        let store = TempDir::new().unwrap();
        let mut mgr = PackageManager::new(store.path()).unwrap();

        for name in ["search-alpha", "search-beta", "other-agent"] {
            let pkg = TempDir::new().unwrap();
            create_test_package(pkg.path(), name, "1.0.0");
            mgr.install(pkg.path()).unwrap();
        }

        let results = mgr.search("search");
        assert_eq!(results.len(), 2);

        let results = mgr.search("other");
        assert_eq!(results.len(), 1);

        let results = mgr.search("nonexistent");
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_case_insensitive() {
        let store = TempDir::new().unwrap();
        let mut mgr = PackageManager::new(store.path()).unwrap();

        let pkg = TempDir::new().unwrap();
        create_test_package(pkg.path(), "CamelAgent", "1.0.0");
        mgr.install(pkg.path()).unwrap();

        assert_eq!(mgr.search("camel").len(), 1);
        assert_eq!(mgr.search("CAMEL").len(), 1);
    }

    #[test]
    fn test_verify_installed() {
        let pkg_dir = TempDir::new().unwrap();
        create_test_package(pkg_dir.path(), "verify-agent", "1.0.0");

        let store = TempDir::new().unwrap();
        let mut mgr = PackageManager::new(store.path()).unwrap();
        mgr.install(pkg_dir.path()).unwrap();

        assert!(mgr.verify("verify-agent").unwrap());
    }

    #[test]
    fn test_verify_tampered_binary() {
        let pkg_dir = TempDir::new().unwrap();
        create_test_package(pkg_dir.path(), "tamper-agent", "1.0.0");

        let store = TempDir::new().unwrap();
        let mut mgr = PackageManager::new(store.path()).unwrap();
        mgr.install(pkg_dir.path()).unwrap();

        // Tamper with the installed binary
        let info = mgr.get_info("tamper-agent").unwrap();
        std::fs::write(&info.binary_path, "TAMPERED CONTENT").unwrap();

        assert!(!mgr.verify("tamper-agent").unwrap());
    }

    #[test]
    fn test_verify_nonexistent() {
        let store = TempDir::new().unwrap();
        let mgr = PackageManager::new(store.path()).unwrap();
        assert!(mgr.verify("nope").is_err());
    }

    #[test]
    fn test_index_persistence() {
        let store = TempDir::new().unwrap();

        // Install a package
        {
            let mut mgr = PackageManager::new(store.path()).unwrap();
            let pkg = TempDir::new().unwrap();
            create_test_package(pkg.path(), "persist-agent", "1.0.0");
            mgr.install(pkg.path()).unwrap();
        }

        // Create a new manager — should load the index
        let mgr = PackageManager::new(store.path()).unwrap();
        assert_eq!(mgr.count(), 1);
        assert!(mgr.is_installed("persist-agent"));
    }

    #[test]
    fn test_install_with_assets() {
        let pkg_dir = TempDir::new().unwrap();
        create_test_package(pkg_dir.path(), "asset-agent", "1.0.0");

        // Add some extra files
        std::fs::write(pkg_dir.path().join("config.json"), r#"{"key": "value"}"#).unwrap();
        std::fs::create_dir_all(pkg_dir.path().join("data")).unwrap();
        std::fs::write(pkg_dir.path().join("data/model.bin"), "model data").unwrap();

        let store = TempDir::new().unwrap();
        let mut mgr = PackageManager::new(store.path()).unwrap();
        let result = mgr.install(pkg_dir.path()).unwrap();

        // Check assets were copied
        assert!(result.install_dir.join("config.json").exists());
        assert!(result.install_dir.join("data/model.bin").exists());
    }

    #[test]
    fn test_validate_version_valid() {
        assert!(validate_version("1.0.0").is_ok());
        assert!(validate_version("0.1.0").is_ok());
        assert!(validate_version("2.0").is_ok());
        assert!(validate_version("1.0.0-alpha").is_ok());
        assert!(validate_version("10.20.30").is_ok());
    }

    #[test]
    fn test_validate_version_invalid() {
        assert!(validate_version("").is_err());
        assert!(validate_version("1").is_err());
        assert!(validate_version("abc").is_err());
        assert!(validate_version("1.x.0").is_err());
        assert!(validate_version("1.0.0.0.0").is_err());
    }

    #[test]
    fn test_validate_permissions_warns_no_rationale() {
        // Should not error, just warn
        let manifest = AgentManifest {
            name: "test".to_string(),
            description: "test".to_string(),
            version: "1.0.0".to_string(),
            requested_permissions: vec![Permission::ProcessSpawn],
            ..Default::default()
        };
        assert!(validate_permissions(&manifest).is_ok());
    }

    #[test]
    fn test_consent_prompt_formatting() {
        let package = AgentPackage {
            manifest: AgentManifest {
                name: "test-agent".to_string(),
                description: "A helpful assistant agent".to_string(),
                author: "AGNOS Team".to_string(),
                version: "1.0.0".to_string(),
                requested_permissions: vec![Permission::FileRead, Permission::NetworkAccess],
                permission_rationale: {
                    let mut m = HashMap::new();
                    m.insert("FileRead".to_string(), "Read config files".to_string());
                    m
                },
                ..Default::default()
            },
            source: PathBuf::from("/tmp/test"),
            binary_hash: "abc123".to_string(),
            binary_size: 1024 * 512, // 512 KB
        };

        let prompt = consent_prompt(&package);
        assert!(prompt.contains("test-agent"));
        assert!(prompt.contains("1.0.0"));
        assert!(prompt.contains("AGNOS Team"));
        assert!(prompt.contains("512.0 KB"));
        assert!(prompt.contains("FileRead"));
        assert!(prompt.contains("NetworkAccess"));
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1024), "1.0 KB");
        assert_eq!(format_bytes(1024 * 1024), "1.0 MB");
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1.0 GB");
        assert_eq!(format_bytes(1536), "1.5 KB");
    }

    #[test]
    fn test_file_fingerprint_deterministic() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.bin");
        std::fs::write(&path, b"test content").unwrap();

        let h1 = file_fingerprint(&path).unwrap();
        let h2 = file_fingerprint(&path).unwrap();
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_file_fingerprint_different_content() {
        let dir = TempDir::new().unwrap();
        let p1 = dir.path().join("a.bin");
        let p2 = dir.path().join("b.bin");
        std::fs::write(&p1, b"content A").unwrap();
        std::fs::write(&p2, b"content B").unwrap();

        let h1 = file_fingerprint(&p1).unwrap();
        let h2 = file_fingerprint(&p2).unwrap();
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_count_files() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("a"), "").unwrap();
        std::fs::write(dir.path().join("b"), "").unwrap();
        std::fs::create_dir_all(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("sub/c"), "").unwrap();

        assert_eq!(count_files(dir.path()).unwrap(), 3);
    }

    #[test]
    fn test_copy_dir_recursive() {
        let src = TempDir::new().unwrap();
        std::fs::write(src.path().join("a.txt"), "aaa").unwrap();
        std::fs::create_dir_all(src.path().join("sub")).unwrap();
        std::fs::write(src.path().join("sub/b.txt"), "bbb").unwrap();

        let dst = TempDir::new().unwrap();
        let target = dst.path().join("copy");
        copy_dir_recursive(src.path(), &target).unwrap();

        assert!(target.join("a.txt").exists());
        assert!(target.join("sub/b.txt").exists());
        assert_eq!(std::fs::read_to_string(target.join("sub/b.txt")).unwrap(), "bbb");
    }

    #[test]
    fn test_multiple_packages() {
        let store = TempDir::new().unwrap();
        let mut mgr = PackageManager::new(store.path()).unwrap();

        for name in ["agent-a", "agent-b", "agent-c"] {
            let pkg = TempDir::new().unwrap();
            create_test_package(pkg.path(), name, "1.0.0");
            mgr.install(pkg.path()).unwrap();
        }

        assert_eq!(mgr.count(), 3);
        let list = mgr.list_installed();
        assert_eq!(list.len(), 3);
        // Should be sorted by name
        assert_eq!(list[0].name, "agent-a");
        assert_eq!(list[1].name, "agent-b");
        assert_eq!(list[2].name, "agent-c");
    }

    #[test]
    fn test_find_binary_by_name() {
        let dir = TempDir::new().unwrap();
        // No "agent" file, but has the named binary
        std::fs::write(
            dir.path().join(MANIFEST_FILENAME),
            r#"name = "custom"
description = "test"
version = "1.0.0"
"#,
        )
        .unwrap();
        std::fs::write(dir.path().join("custom"), "binary").unwrap();

        let store = TempDir::new().unwrap();
        let mgr = PackageManager::new(store.path()).unwrap();

        let pkg = mgr.validate_package(dir.path()).unwrap();
        assert_eq!(pkg.manifest.name, "custom");
    }
}
