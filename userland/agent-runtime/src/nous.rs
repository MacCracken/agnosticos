//! Nous Resolver Daemon
//!
//! The intelligence layer that determines where to get packages from.
//! Given a package name, nous figures out which source to use (system apt,
//! marketplace agents, or Flutter desktop apps) and returns a resolution plan.
//! It does NOT execute installs itself — that is `ark`'s job.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use crate::marketplace::local_registry::LocalRegistry;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Where a package comes from.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PackageSource {
    /// System package via apt/dpkg.
    System,
    /// AGNOS marketplace agent package.
    Marketplace,
    /// Flutter desktop app via agpkg.
    FlutterApp,
    /// Community-submitted recipe built via takumi (like AUR).
    Community,
    /// Unknown — not found in any source.
    Unknown,
}

impl std::fmt::Display for PackageSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::System => write!(f, "system"),
            Self::Marketplace => write!(f, "marketplace"),
            Self::FlutterApp => write!(f, "flutter-app"),
            Self::Community => write!(f, "community"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

/// A resolved package with source and metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedPackage {
    /// Package name.
    pub name: String,
    /// Package version.
    pub version: String,
    /// Source of the package.
    pub source: PackageSource,
    /// Human-readable description.
    pub description: String,
    /// Size in bytes (if known).
    pub size_bytes: Option<u64>,
    /// Dependency names.
    pub dependencies: Vec<String>,
    /// Whether the package is signed/verified.
    pub trusted: bool,
}

/// Resolution strategy — how nous decides where to look.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ResolutionStrategy {
    /// Check marketplace first, then system (default).
    #[default]
    MarketplaceFirst,
    /// Check system first, then marketplace.
    SystemFirst,
    /// Only check a specific source.
    OnlySource(PackageSource),
    /// Check all sources, return all matches.
    SearchAll,
}

/// A unified search result across all sources.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedSearchResult {
    /// All matching packages.
    pub results: Vec<ResolvedPackage>,
    /// Which sources were searched.
    pub sources_searched: Vec<PackageSource>,
    /// Total number of matches.
    pub total_matches: usize,
}

/// Installed package info (unified view across all sources).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledPackage {
    /// Package name.
    pub name: String,
    /// Installed version.
    pub version: String,
    /// Source of the package.
    pub source: PackageSource,
    /// When the package was installed.
    pub install_date: DateTime<Utc>,
    /// Size in bytes (if known).
    pub size_bytes: Option<u64>,
    /// Whether this was auto-installed as a dependency.
    pub auto_installed: bool,
}

/// Update available for an installed package.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvailableUpdate {
    /// Package name.
    pub name: String,
    /// Source of the package.
    pub source: PackageSource,
    /// Currently installed version.
    pub installed_version: String,
    /// Available version.
    pub available_version: String,
    /// Changelog for the new version.
    pub changelog: Option<String>,
}

// ---------------------------------------------------------------------------
// SystemPackageDb
// ---------------------------------------------------------------------------

/// Wrapper around apt/dpkg for querying system packages.
pub struct SystemPackageDb {
    /// Whether apt is available on this system.
    apt_available: bool,
}

impl SystemPackageDb {
    /// Create a new `SystemPackageDb`, checking whether apt tools exist on PATH.
    pub fn new() -> Self {
        let apt_available = which_exists("apt-cache") && which_exists("dpkg-query");
        if !apt_available {
            debug!("apt/dpkg not found on PATH; system package queries will return empty results");
        }
        Self { apt_available }
    }

    /// Whether apt/dpkg tools are available.
    pub fn is_available(&self) -> bool {
        self.apt_available
    }

    /// Search system packages using `apt-cache search`.
    pub fn search(&self, query: &str) -> Result<Vec<ResolvedPackage>> {
        if !self.apt_available {
            return Ok(Vec::new());
        }

        let output = std::process::Command::new("apt-cache")
            .arg("search")
            .arg("--names-only")
            .arg(query)
            .output()
            .context("Failed to run apt-cache search")?;

        if !output.status.success() {
            warn!(
                "apt-cache search failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            return Ok(Vec::new());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let results = stdout
            .lines()
            .filter(|line| !line.trim().is_empty())
            .filter_map(|line| {
                // Format: "package-name - description"
                let mut parts = line.splitn(2, " - ");
                let name = parts.next()?.trim().to_string();
                let description = parts.next().unwrap_or("").trim().to_string();
                Some(ResolvedPackage {
                    name,
                    version: String::new(), // apt-cache search doesn't include version
                    source: PackageSource::System,
                    description,
                    size_bytes: None,
                    dependencies: Vec::new(),
                    trusted: true, // system packages are trusted
                })
            })
            .collect();

        Ok(results)
    }

    /// Check if a system package is installed via `dpkg-query -W`.
    pub fn is_installed(&self, name: &str) -> Result<bool> {
        if !self.apt_available {
            return Ok(false);
        }

        let output = std::process::Command::new("dpkg-query")
            .arg("-W")
            .arg("-f")
            .arg("${Status}")
            .arg(name)
            .output()
            .context("Failed to run dpkg-query")?;

        if !output.status.success() {
            return Ok(false);
        }

        let status = String::from_utf8_lossy(&output.stdout);
        Ok(status.contains("install ok installed"))
    }

    /// Get info about an installed system package.
    pub fn get_installed(&self, name: &str) -> Result<Option<InstalledPackage>> {
        if !self.apt_available {
            return Ok(None);
        }

        let output = std::process::Command::new("dpkg-query")
            .arg("-W")
            .arg("-f")
            .arg("${Package}\t${Version}\t${Installed-Size}\n")
            .arg(name)
            .output()
            .context("Failed to run dpkg-query")?;

        if !output.status.success() {
            return Ok(None);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let line = match stdout.lines().next() {
            Some(l) => l,
            None => return Ok(None),
        };

        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() < 3 {
            return Ok(None);
        }

        let size_kb: u64 = fields[2].trim().parse().unwrap_or(0);

        Ok(Some(InstalledPackage {
            name: fields[0].to_string(),
            version: fields[1].to_string(),
            source: PackageSource::System,
            install_date: Utc::now(), // dpkg doesn't track install date precisely
            size_bytes: Some(size_kb * 1024),
            auto_installed: false,
        }))
    }

    /// List all installed system packages.
    pub fn list_installed(&self) -> Result<Vec<InstalledPackage>> {
        if !self.apt_available {
            return Ok(Vec::new());
        }

        let output = std::process::Command::new("dpkg-query")
            .arg("-W")
            .arg("-f")
            .arg("${Package}\t${Version}\t${Installed-Size}\t${Status}\n")
            .output()
            .context("Failed to run dpkg-query")?;

        if !output.status.success() {
            warn!(
                "dpkg-query failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            return Ok(Vec::new());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let packages = stdout
            .lines()
            .filter(|line| !line.trim().is_empty())
            .filter_map(|line| {
                let fields: Vec<&str> = line.split('\t').collect();
                if fields.len() < 4 {
                    return None;
                }
                // Only include fully installed packages
                if !fields[3].contains("install ok installed") {
                    return None;
                }
                let size_kb: u64 = fields[2].trim().parse().unwrap_or(0);
                Some(InstalledPackage {
                    name: fields[0].to_string(),
                    version: fields[1].to_string(),
                    source: PackageSource::System,
                    install_date: Utc::now(),
                    size_bytes: Some(size_kb * 1024),
                    auto_installed: false,
                })
            })
            .collect();

        Ok(packages)
    }

    /// Check for available system package updates via `apt list --upgradable`.
    pub fn check_updates(&self) -> Result<Vec<AvailableUpdate>> {
        if !self.apt_available {
            return Ok(Vec::new());
        }

        let output = std::process::Command::new("apt")
            .arg("list")
            .arg("--upgradable")
            .output()
            .context("Failed to run apt list --upgradable")?;

        if !output.status.success() {
            warn!(
                "apt list --upgradable failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            return Ok(Vec::new());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let updates = stdout
            .lines()
            .filter(|line| !line.starts_with("Listing"))
            .filter(|line| line.contains("[upgradable"))
            .filter_map(|line| {
                // Format: "package/suite version arch [upgradable from: old_version]"
                let name = line.split('/').next()?.to_string();
                let rest = line.split(' ').collect::<Vec<_>>();
                let available_version = rest.get(1).unwrap_or(&"").to_string();
                // Extract old version from "[upgradable from: X]"
                let installed_version = line
                    .split("from: ")
                    .nth(1)
                    .and_then(|s| s.strip_suffix(']'))
                    .unwrap_or("")
                    .to_string();

                Some(AvailableUpdate {
                    name,
                    source: PackageSource::System,
                    installed_version,
                    available_version,
                    changelog: None,
                })
            })
            .collect();

        Ok(updates)
    }

    /// Get detailed package info via `apt-cache show`.
    pub fn get_package_info(&self, name: &str) -> Result<Option<ResolvedPackage>> {
        if !self.apt_available {
            return Ok(None);
        }

        let output = std::process::Command::new("apt-cache")
            .arg("show")
            .arg(name)
            .output()
            .context("Failed to run apt-cache show")?;

        if !output.status.success() {
            return Ok(None);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);

        let mut version = String::new();
        let mut description = String::new();
        let mut size_bytes = None;
        let mut dependencies = Vec::new();

        for line in stdout.lines() {
            if let Some(v) = line.strip_prefix("Version: ") {
                version = v.trim().to_string();
            } else if let Some(d) = line.strip_prefix("Description: ") {
                description = d.trim().to_string();
            } else if let Some(s) = line.strip_prefix("Size: ") {
                size_bytes = s.trim().parse::<u64>().ok();
            } else if let Some(deps) = line.strip_prefix("Depends: ") {
                dependencies = deps
                    .split(',')
                    .map(|d| {
                        // Strip version constraints: "foo (>= 1.0)" -> "foo"
                        d.split_whitespace().next().unwrap_or("").to_string()
                    })
                    .filter(|d| !d.is_empty())
                    .collect();
            }
        }

        if version.is_empty() {
            return Ok(None);
        }

        Ok(Some(ResolvedPackage {
            name: name.to_string(),
            version,
            source: PackageSource::System,
            description,
            size_bytes,
            dependencies,
            trusted: true,
        }))
    }
}

impl Default for SystemPackageDb {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// NousResolver
// ---------------------------------------------------------------------------

/// The nous resolver — given a package name, determines the source and
/// returns a resolution plan.
pub struct NousResolver {
    /// Resolution strategy.
    strategy: ResolutionStrategy,
    /// Root directory for marketplace packages.
    marketplace_dir: PathBuf,
    /// Cache directory for resolver metadata.
    #[allow(dead_code)]
    cache_dir: PathBuf,
    /// System package database (apt/dpkg wrapper).
    system_package_db: SystemPackageDb,
}

impl NousResolver {
    /// Create a new resolver with default strategy (MarketplaceFirst).
    pub fn new(marketplace_dir: &Path, cache_dir: &Path) -> Self {
        Self {
            strategy: ResolutionStrategy::default(),
            marketplace_dir: marketplace_dir.to_path_buf(),
            cache_dir: cache_dir.to_path_buf(),
            system_package_db: SystemPackageDb::new(),
        }
    }

    /// Set the resolution strategy (builder pattern).
    pub fn with_strategy(mut self, strategy: ResolutionStrategy) -> Self {
        self.strategy = strategy;
        self
    }

    /// Get the current resolution strategy.
    pub fn strategy(&self) -> &ResolutionStrategy {
        &self.strategy
    }

    /// Resolve a single package name using the configured strategy.
    pub fn resolve(&self, name: &str) -> Result<Option<ResolvedPackage>> {
        debug!(name, ?self.strategy, "Resolving package");

        match &self.strategy {
            ResolutionStrategy::MarketplaceFirst => {
                if let Some(pkg) = self.resolve_from_marketplace(name)? {
                    return Ok(Some(pkg));
                }
                if let Some(pkg) = self.resolve_from_flutter(name)? {
                    return Ok(Some(pkg));
                }
                self.resolve_from_system(name)
            }
            ResolutionStrategy::SystemFirst => {
                if let Some(pkg) = self.resolve_from_system(name)? {
                    return Ok(Some(pkg));
                }
                if let Some(pkg) = self.resolve_from_marketplace(name)? {
                    return Ok(Some(pkg));
                }
                self.resolve_from_flutter(name)
            }
            ResolutionStrategy::OnlySource(source) => match source {
                PackageSource::System => self.resolve_from_system(name),
                PackageSource::Marketplace | PackageSource::Community => {
                    self.resolve_from_marketplace(name)
                }
                PackageSource::FlutterApp => self.resolve_from_flutter(name),
                PackageSource::Unknown => Ok(None),
            },
            ResolutionStrategy::SearchAll => {
                // Return the first match found in priority order
                if let Some(pkg) = self.resolve_from_marketplace(name)? {
                    return Ok(Some(pkg));
                }
                if let Some(pkg) = self.resolve_from_flutter(name)? {
                    return Ok(Some(pkg));
                }
                self.resolve_from_system(name)
            }
        }
    }

    /// Search ALL sources, merge results, deduplicate by name
    /// (marketplace takes priority).
    pub fn search(&self, query: &str) -> Result<UnifiedSearchResult> {
        info!(query, "Searching all package sources");

        let mut results: Vec<ResolvedPackage> = Vec::new();
        let mut sources_searched = Vec::new();
        let mut seen_names = std::collections::HashSet::new();

        // 1. Marketplace (highest priority for dedup)
        if let Ok(registry) = LocalRegistry::new(&self.marketplace_dir) {
            sources_searched.push(PackageSource::Marketplace);
            for pkg in registry.search(query) {
                let resolved = ResolvedPackage {
                    name: pkg.manifest.agent.name.clone(),
                    version: pkg.manifest.agent.version.clone(),
                    source: PackageSource::Marketplace,
                    description: pkg.manifest.agent.description.clone(),
                    size_bytes: Some(pkg.installed_size),
                    dependencies: pkg.manifest.dependencies.keys().cloned().collect(),
                    trusted: true, // marketplace packages are signed
                };
                seen_names.insert(resolved.name.clone());
                results.push(resolved);
            }
        }

        // 2. System packages
        sources_searched.push(PackageSource::System);
        for pkg in self.system_package_db.search(query)? {
            if !seen_names.contains(&pkg.name) {
                seen_names.insert(pkg.name.clone());
                results.push(pkg);
            }
        }

        let total_matches = results.len();

        Ok(UnifiedSearchResult {
            results,
            sources_searched,
            total_matches,
        })
    }

    /// Unified list of installed packages from all sources.
    pub fn list_installed(&self) -> Result<Vec<InstalledPackage>> {
        let mut packages = Vec::new();

        // Marketplace packages
        if let Ok(registry) = LocalRegistry::new(&self.marketplace_dir) {
            for pkg in registry.list_installed() {
                packages.push(InstalledPackage {
                    name: pkg.manifest.agent.name.clone(),
                    version: pkg.manifest.agent.version.clone(),
                    source: PackageSource::Marketplace,
                    install_date: pkg.installed_at,
                    size_bytes: Some(pkg.installed_size),
                    auto_installed: false,
                });
            }
        }

        // System packages
        for pkg in self.system_package_db.list_installed()? {
            packages.push(pkg);
        }

        Ok(packages)
    }

    /// Check all sources for available updates.
    pub fn check_updates(&self) -> Result<Vec<AvailableUpdate>> {
        let mut updates = Vec::new();

        // System updates
        updates.extend(self.system_package_db.check_updates()?);

        // Marketplace updates would require checking the remote registry,
        // which is not done here (would be async). We only report system
        // updates in the synchronous path.
        debug!(count = updates.len(), "Found available updates");

        Ok(updates)
    }

    /// Heuristic source detection based on package name format.
    ///
    /// - Contains `/` => Marketplace (publisher/name format)
    /// - Ends with `.flutter` or reverse-domain notation (e.g., `com.example.app`) => FlutterApp
    /// - Otherwise => Unknown (needs resolution)
    pub fn detect_source(name: &str) -> PackageSource {
        if name.is_empty() {
            return PackageSource::Unknown;
        }

        // publisher/name format => Marketplace
        if name.contains('/') {
            return PackageSource::Marketplace;
        }

        // Flutter app patterns: reverse domain notation or .flutter suffix
        if name.ends_with(".flutter") {
            return PackageSource::FlutterApp;
        }

        // Reverse domain notation: at least 2 dots, starts with common TLD prefix
        let dots = name.chars().filter(|&c| c == '.').count();
        if dots >= 2 {
            let first_segment = name.split('.').next().unwrap_or("");
            if matches!(
                first_segment,
                "com" | "org" | "net" | "io" | "dev" | "app" | "me"
            ) {
                return PackageSource::FlutterApp;
            }
        }

        PackageSource::Unknown
    }

    /// Check if a package is installed from the marketplace registry.
    pub fn is_marketplace_package(&self, name: &str) -> bool {
        if let Ok(registry) = LocalRegistry::new(&self.marketplace_dir) {
            registry.get_package(name).is_some()
        } else {
            false
        }
    }

    /// Check if a package is installed as a system package.
    pub fn is_system_package(&self, name: &str) -> bool {
        self.system_package_db.is_installed(name).unwrap_or(false)
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    /// Try to resolve a package from the local marketplace registry.
    fn resolve_from_marketplace(&self, name: &str) -> Result<Option<ResolvedPackage>> {
        let registry = match LocalRegistry::new(&self.marketplace_dir) {
            Ok(r) => r,
            Err(e) => {
                debug!("Could not open marketplace registry: {}", e);
                return Ok(None);
            }
        };

        if let Some(pkg) = registry.get_package(name) {
            return Ok(Some(ResolvedPackage {
                name: pkg.manifest.agent.name.clone(),
                version: pkg.manifest.agent.version.clone(),
                source: PackageSource::Marketplace,
                description: pkg.manifest.agent.description.clone(),
                size_bytes: Some(pkg.installed_size),
                dependencies: pkg.manifest.dependencies.keys().cloned().collect(),
                trusted: true,
            }));
        }

        Ok(None)
    }

    /// Try to resolve a package from the system package database.
    fn resolve_from_system(&self, name: &str) -> Result<Option<ResolvedPackage>> {
        self.system_package_db.get_package_info(name)
    }

    /// Try to resolve a package as a Flutter app.
    ///
    /// Currently checks the marketplace registry for packages with
    /// `runtime: "flutter"` in their manifest.
    fn resolve_from_flutter(&self, name: &str) -> Result<Option<ResolvedPackage>> {
        let registry = match LocalRegistry::new(&self.marketplace_dir) {
            Ok(r) => r,
            Err(_) => return Ok(None),
        };

        if let Some(pkg) = registry.get_package(name) {
            if pkg.manifest.runtime == "flutter" {
                return Ok(Some(ResolvedPackage {
                    name: pkg.manifest.agent.name.clone(),
                    version: pkg.manifest.agent.version.clone(),
                    source: PackageSource::FlutterApp,
                    description: pkg.manifest.agent.description.clone(),
                    size_bytes: Some(pkg.installed_size),
                    dependencies: pkg.manifest.dependencies.keys().cloned().collect(),
                    trusted: true,
                }));
            }
        }

        Ok(None)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Check if an executable exists on PATH.
fn which_exists(name: &str) -> bool {
    std::process::Command::new("which")
        .arg(name)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    use crate::marketplace::{MarketplaceCategory, MarketplaceManifest, PublisherInfo};
    use agnos_common::AgentManifest;

    /// Helper: create a test tarball for a marketplace package.
    fn create_test_tarball(manifest: &MarketplaceManifest) -> Vec<u8> {
        use flate2::write::GzEncoder;
        use std::io::Write;

        let mut builder = tar::Builder::new(Vec::new());

        let manifest_json = serde_json::to_string_pretty(manifest).unwrap();
        let manifest_bytes = manifest_json.as_bytes();
        let mut header = tar::Header::new_gnu();
        header.set_size(manifest_bytes.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder
            .append_data(&mut header, "manifest.json", manifest_bytes)
            .unwrap();

        let binary = b"#!/bin/sh\necho hello\n";
        let mut header = tar::Header::new_gnu();
        header.set_size(binary.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        builder
            .append_data(&mut header, "bin/agent", &binary[..])
            .unwrap();

        let tar_data = builder.into_inner().unwrap();
        let mut encoder = GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(&tar_data).unwrap();
        encoder.finish().unwrap()
    }

    fn sample_manifest(name: &str) -> MarketplaceManifest {
        MarketplaceManifest {
            agent: AgentManifest {
                name: name.to_string(),
                description: format!("Test package {}", name),
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

    #[allow(dead_code)]
    fn sample_flutter_manifest(name: &str) -> MarketplaceManifest {
        let mut m = sample_manifest(name);
        m.runtime = "flutter".to_string();
        m.category = MarketplaceCategory::DesktopApp;
        m
    }

    /// Install a test package into a LocalRegistry temp dir.
    fn install_test_package(dir: &Path, manifest: &MarketplaceManifest) {
        let mut registry = LocalRegistry::new(dir).unwrap();
        let tarball = create_test_tarball(manifest);
        let tarball_path = dir.join(format!("{}.agnos-agent", manifest.agent.name));
        std::fs::write(&tarball_path, &tarball).unwrap();
        registry.install_package(&tarball_path, None).unwrap();
    }

    // -----------------------------------------------------------------------
    // Test 1: SystemPackageDb::new()
    // -----------------------------------------------------------------------

    #[test]
    fn test_system_package_db_new() {
        let db = SystemPackageDb::new();
        // Just verify it constructs without panic; apt_available depends on host
        let _ = db.is_available();
    }

    // -----------------------------------------------------------------------
    // Test 2: detect_source with various package names
    // -----------------------------------------------------------------------

    #[test]
    fn test_detect_source_marketplace() {
        assert_eq!(
            NousResolver::detect_source("acme/scanner"),
            PackageSource::Marketplace
        );
        assert_eq!(
            NousResolver::detect_source("publisher/agent-name"),
            PackageSource::Marketplace
        );
    }

    #[test]
    fn test_detect_source_flutter() {
        assert_eq!(
            NousResolver::detect_source("com.example.app"),
            PackageSource::FlutterApp
        );
        assert_eq!(
            NousResolver::detect_source("myapp.flutter"),
            PackageSource::FlutterApp
        );
        assert_eq!(
            NousResolver::detect_source("org.agnos.files"),
            PackageSource::FlutterApp
        );
    }

    #[test]
    fn test_detect_source_unknown() {
        assert_eq!(NousResolver::detect_source("nginx"), PackageSource::Unknown);
        assert_eq!(NousResolver::detect_source("curl"), PackageSource::Unknown);
    }

    // -----------------------------------------------------------------------
    // Test 3: NousResolver::new() construction
    // -----------------------------------------------------------------------

    #[test]
    fn test_nous_resolver_new() {
        let dir = tempfile::tempdir().unwrap();
        let cache = tempfile::tempdir().unwrap();
        let resolver = NousResolver::new(dir.path(), cache.path());
        assert_eq!(*resolver.strategy(), ResolutionStrategy::MarketplaceFirst);
    }

    // -----------------------------------------------------------------------
    // Test 4: with_strategy builder
    // -----------------------------------------------------------------------

    #[test]
    fn test_with_strategy() {
        let dir = tempfile::tempdir().unwrap();
        let cache = tempfile::tempdir().unwrap();
        let resolver = NousResolver::new(dir.path(), cache.path())
            .with_strategy(ResolutionStrategy::SystemFirst);
        assert_eq!(*resolver.strategy(), ResolutionStrategy::SystemFirst);
    }

    // -----------------------------------------------------------------------
    // Test 5: resolve() with marketplace package
    // -----------------------------------------------------------------------

    #[test]
    fn test_resolve_marketplace_package() {
        let dir = tempfile::tempdir().unwrap();
        let cache = tempfile::tempdir().unwrap();

        let manifest = sample_manifest("test-scanner");
        install_test_package(dir.path(), &manifest);

        let resolver = NousResolver::new(dir.path(), cache.path());
        let result = resolver.resolve("test-scanner").unwrap();
        assert!(result.is_some());
        let pkg = result.unwrap();
        assert_eq!(pkg.name, "test-scanner");
        assert_eq!(pkg.source, PackageSource::Marketplace);
        assert!(pkg.trusted);
    }

    // -----------------------------------------------------------------------
    // Test 6: resolve() unknown package returns None when system unavailable
    // -----------------------------------------------------------------------

    #[test]
    fn test_resolve_unknown_package() {
        let dir = tempfile::tempdir().unwrap();
        let cache = tempfile::tempdir().unwrap();

        // Empty marketplace dir, so nothing to find there
        let resolver = NousResolver::new(dir.path(), cache.path())
            .with_strategy(ResolutionStrategy::OnlySource(PackageSource::Marketplace));
        let result = resolver.resolve("nonexistent-package-xyz").unwrap();
        assert!(result.is_none());
    }

    // -----------------------------------------------------------------------
    // Test 7: search() merges results from marketplace
    // -----------------------------------------------------------------------

    #[test]
    fn test_search_merges_marketplace() {
        let dir = tempfile::tempdir().unwrap();
        let cache = tempfile::tempdir().unwrap();

        let manifest = sample_manifest("test-tool");
        install_test_package(dir.path(), &manifest);

        let resolver = NousResolver::new(dir.path(), cache.path());
        let result = resolver.search("test").unwrap();
        assert!(result.total_matches >= 1);
        assert!(result
            .results
            .iter()
            .any(|p| p.name == "test-tool" && p.source == PackageSource::Marketplace));
    }

    // -----------------------------------------------------------------------
    // Test 8: list_installed() returns marketplace packages
    // -----------------------------------------------------------------------

    #[test]
    fn test_list_installed_marketplace() {
        let dir = tempfile::tempdir().unwrap();
        let cache = tempfile::tempdir().unwrap();

        let manifest = sample_manifest("installed-agent");
        install_test_package(dir.path(), &manifest);

        let resolver = NousResolver::new(dir.path(), cache.path());
        let installed = resolver.list_installed().unwrap();
        assert!(installed
            .iter()
            .any(|p| p.name == "installed-agent" && p.source == PackageSource::Marketplace));
    }

    // -----------------------------------------------------------------------
    // Test 9: ResolvedPackage serialization roundtrip
    // -----------------------------------------------------------------------

    #[test]
    fn test_resolved_package_serialization() {
        let pkg = ResolvedPackage {
            name: "test-pkg".to_string(),
            version: "1.2.3".to_string(),
            source: PackageSource::System,
            description: "A test package".to_string(),
            size_bytes: Some(4096),
            dependencies: vec!["libc".to_string()],
            trusted: true,
        };
        let json = serde_json::to_string(&pkg).unwrap();
        let parsed: ResolvedPackage = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "test-pkg");
        assert_eq!(parsed.version, "1.2.3");
        assert_eq!(parsed.source, PackageSource::System);
        assert_eq!(parsed.size_bytes, Some(4096));
        assert_eq!(parsed.dependencies, vec!["libc"]);
        assert!(parsed.trusted);
    }

    // -----------------------------------------------------------------------
    // Test 10: InstalledPackage serialization roundtrip
    // -----------------------------------------------------------------------

    #[test]
    fn test_installed_package_serialization() {
        let pkg = InstalledPackage {
            name: "my-agent".to_string(),
            version: "2.0.0".to_string(),
            source: PackageSource::Marketplace,
            install_date: Utc::now(),
            size_bytes: Some(8192),
            auto_installed: false,
        };
        let json = serde_json::to_string(&pkg).unwrap();
        let parsed: InstalledPackage = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "my-agent");
        assert_eq!(parsed.source, PackageSource::Marketplace);
        assert!(!parsed.auto_installed);
    }

    // -----------------------------------------------------------------------
    // Test 11: AvailableUpdate serialization
    // -----------------------------------------------------------------------

    #[test]
    fn test_available_update_serialization() {
        let update = AvailableUpdate {
            name: "openssl".to_string(),
            source: PackageSource::System,
            installed_version: "3.0.0".to_string(),
            available_version: "3.1.0".to_string(),
            changelog: Some("Security fixes".to_string()),
        };
        let json = serde_json::to_string(&update).unwrap();
        let parsed: AvailableUpdate = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "openssl");
        assert_eq!(parsed.installed_version, "3.0.0");
        assert_eq!(parsed.available_version, "3.1.0");
        assert_eq!(parsed.changelog, Some("Security fixes".to_string()));
    }

    // -----------------------------------------------------------------------
    // Test 12: PackageSource Display impl
    // -----------------------------------------------------------------------

    #[test]
    fn test_package_source_display() {
        assert_eq!(PackageSource::System.to_string(), "system");
        assert_eq!(PackageSource::Marketplace.to_string(), "marketplace");
        assert_eq!(PackageSource::FlutterApp.to_string(), "flutter-app");
        assert_eq!(PackageSource::Unknown.to_string(), "unknown");
    }

    // -----------------------------------------------------------------------
    // Test 13: ResolutionStrategy default is MarketplaceFirst
    // -----------------------------------------------------------------------

    #[test]
    fn test_resolution_strategy_default() {
        let strategy = ResolutionStrategy::default();
        assert_eq!(strategy, ResolutionStrategy::MarketplaceFirst);
    }

    // -----------------------------------------------------------------------
    // Test 14: UnifiedSearchResult with empty results
    // -----------------------------------------------------------------------

    #[test]
    fn test_unified_search_result_empty() {
        let result = UnifiedSearchResult {
            results: Vec::new(),
            sources_searched: vec![PackageSource::System, PackageSource::Marketplace],
            total_matches: 0,
        };
        assert!(result.results.is_empty());
        assert_eq!(result.sources_searched.len(), 2);
        assert_eq!(result.total_matches, 0);
    }

    // -----------------------------------------------------------------------
    // Test 15: detect_source edge cases
    // -----------------------------------------------------------------------

    #[test]
    fn test_detect_source_edge_cases() {
        // Empty string
        assert_eq!(NousResolver::detect_source(""), PackageSource::Unknown);
        // Multiple slashes (still marketplace)
        assert_eq!(
            NousResolver::detect_source("a/b/c"),
            PackageSource::Marketplace
        );
        // Single dot, not enough for flutter
        assert_eq!(
            NousResolver::detect_source("foo.bar"),
            PackageSource::Unknown
        );
        // Dots with non-TLD prefix
        assert_eq!(
            NousResolver::detect_source("my.custom.thing"),
            PackageSource::Unknown
        );
        // Valid reverse domain
        assert_eq!(
            NousResolver::detect_source("io.flutter.demo"),
            PackageSource::FlutterApp
        );
    }

    // -----------------------------------------------------------------------
    // Test 16: is_marketplace_package with pre-populated registry
    // -----------------------------------------------------------------------

    #[test]
    fn test_is_marketplace_package() {
        let dir = tempfile::tempdir().unwrap();
        let cache = tempfile::tempdir().unwrap();

        let manifest = sample_manifest("my-agent");
        install_test_package(dir.path(), &manifest);

        let resolver = NousResolver::new(dir.path(), cache.path());
        assert!(resolver.is_marketplace_package("my-agent"));
        assert!(!resolver.is_marketplace_package("not-installed"));
    }

    // -----------------------------------------------------------------------
    // Test 17: check_updates with empty installed list
    // -----------------------------------------------------------------------

    #[test]
    fn test_check_updates_empty() {
        let dir = tempfile::tempdir().unwrap();
        let cache = tempfile::tempdir().unwrap();

        let resolver = NousResolver::new(dir.path(), cache.path());
        // Should not error even with no packages
        let updates = resolver.check_updates().unwrap();
        // Can't assert exact count (depends on host), just that it doesn't panic
        let _ = updates.len();
    }

    // -----------------------------------------------------------------------
    // Test 18: OnlySource(System) skips marketplace
    // -----------------------------------------------------------------------

    #[test]
    fn test_only_source_system_skips_marketplace() {
        let dir = tempfile::tempdir().unwrap();
        let cache = tempfile::tempdir().unwrap();

        let manifest = sample_manifest("marketplace-only-pkg");
        install_test_package(dir.path(), &manifest);

        let resolver = NousResolver::new(dir.path(), cache.path())
            .with_strategy(ResolutionStrategy::OnlySource(PackageSource::System));

        // This package exists only in marketplace, but strategy is System-only
        let result = resolver.resolve("marketplace-only-pkg").unwrap();
        // On systems without apt, this should be None since System source won't find it
        // On systems with apt, it also won't be a system package
        assert!(result.is_none());
    }

    // -----------------------------------------------------------------------
    // Test 19: OnlySource(Marketplace) skips system
    // -----------------------------------------------------------------------

    #[test]
    fn test_only_source_marketplace_finds_marketplace() {
        let dir = tempfile::tempdir().unwrap();
        let cache = tempfile::tempdir().unwrap();

        let manifest = sample_manifest("market-pkg");
        install_test_package(dir.path(), &manifest);

        let resolver = NousResolver::new(dir.path(), cache.path())
            .with_strategy(ResolutionStrategy::OnlySource(PackageSource::Marketplace));

        let result = resolver.resolve("market-pkg").unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().source, PackageSource::Marketplace);
    }

    // -----------------------------------------------------------------------
    // Test 20: SystemPackageDb methods return empty when apt unavailable
    // -----------------------------------------------------------------------

    #[test]
    fn test_system_package_db_empty_when_unavailable() {
        let db = SystemPackageDb {
            apt_available: false,
        };
        assert!(!db.is_available());
        assert!(db.search("anything").unwrap().is_empty());
        assert!(!db.is_installed("anything").unwrap());
        assert!(db.get_installed("anything").unwrap().is_none());
        assert!(db.list_installed().unwrap().is_empty());
        assert!(db.check_updates().unwrap().is_empty());
        assert!(db.get_package_info("anything").unwrap().is_none());
    }

    // -----------------------------------------------------------------------
    // Coverage improvement: Resolution strategies & flutter source
    // -----------------------------------------------------------------------

    #[test]
    fn test_resolve_system_first_strategy() {
        let dir = tempfile::tempdir().unwrap();
        let cache = tempfile::tempdir().unwrap();

        let manifest = sample_manifest("sys-first-pkg");
        install_test_package(dir.path(), &manifest);

        let resolver = NousResolver::new(dir.path(), cache.path())
            .with_strategy(ResolutionStrategy::SystemFirst);

        // Package exists in marketplace but not system; SystemFirst should
        // fall through to marketplace
        let result = resolver.resolve("sys-first-pkg").unwrap();
        assert!(result.is_some());
        let pkg = result.unwrap();
        assert_eq!(pkg.name, "sys-first-pkg");
        assert_eq!(pkg.source, PackageSource::Marketplace);
    }

    #[test]
    fn test_resolve_search_all_strategy() {
        let dir = tempfile::tempdir().unwrap();
        let cache = tempfile::tempdir().unwrap();

        let manifest = sample_manifest("search-all-pkg");
        install_test_package(dir.path(), &manifest);

        let resolver = NousResolver::new(dir.path(), cache.path())
            .with_strategy(ResolutionStrategy::SearchAll);

        let result = resolver.resolve("search-all-pkg").unwrap();
        assert!(result.is_some());
        let pkg = result.unwrap();
        assert_eq!(pkg.name, "search-all-pkg");
    }

    #[test]
    fn test_resolve_search_all_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let cache = tempfile::tempdir().unwrap();

        let resolver = NousResolver::new(dir.path(), cache.path())
            .with_strategy(ResolutionStrategy::SearchAll);

        let result = resolver.resolve("nonexistent-pkg").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_resolve_only_source_flutter_app() {
        let dir = tempfile::tempdir().unwrap();
        let cache = tempfile::tempdir().unwrap();

        let manifest = sample_flutter_manifest("flutter-pkg");
        install_test_package(dir.path(), &manifest);

        let resolver = NousResolver::new(dir.path(), cache.path())
            .with_strategy(ResolutionStrategy::OnlySource(PackageSource::FlutterApp));

        let result = resolver.resolve("flutter-pkg").unwrap();
        assert!(result.is_some());
        let pkg = result.unwrap();
        assert_eq!(pkg.name, "flutter-pkg");
        assert_eq!(pkg.source, PackageSource::FlutterApp);
    }

    #[test]
    fn test_resolve_only_source_unknown_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let cache = tempfile::tempdir().unwrap();

        let resolver = NousResolver::new(dir.path(), cache.path())
            .with_strategy(ResolutionStrategy::OnlySource(PackageSource::Unknown));

        let result = resolver.resolve("any-pkg").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_search_empty_query_returns_all() {
        let dir = tempfile::tempdir().unwrap();
        let cache = tempfile::tempdir().unwrap();

        let manifest = sample_manifest("searchable-pkg");
        install_test_package(dir.path(), &manifest);

        let resolver = NousResolver::new(dir.path(), cache.path());
        let result = resolver.search("searchable").unwrap();
        assert!(!result.results.is_empty());
        assert!(result.total_matches > 0);
    }

    #[test]
    fn test_search_no_matches() {
        let dir = tempfile::tempdir().unwrap();
        let cache = tempfile::tempdir().unwrap();

        let resolver = NousResolver::new(dir.path(), cache.path());
        let result = resolver.search("nonexistent-xyz-123").unwrap();
        // Marketplace should be empty; system results depend on environment
        assert!(!result.sources_searched.is_empty());
    }

    #[test]
    fn test_list_installed_with_packages() {
        let dir = tempfile::tempdir().unwrap();
        let cache = tempfile::tempdir().unwrap();

        let manifest = sample_manifest("list-pkg-a");
        install_test_package(dir.path(), &manifest);

        let resolver = NousResolver::new(dir.path(), cache.path());
        let installed = resolver.list_installed().unwrap();
        assert!(installed.iter().any(|p| p.name == "list-pkg-a"));
    }

    #[test]
    fn test_is_system_package_false_for_marketplace() {
        let dir = tempfile::tempdir().unwrap();
        let cache = tempfile::tempdir().unwrap();

        let manifest = sample_manifest("market-only");
        install_test_package(dir.path(), &manifest);

        let resolver = NousResolver::new(dir.path(), cache.path());
        // market-only is a marketplace package, not a system package
        assert!(!resolver.is_system_package("market-only"));
    }
}
