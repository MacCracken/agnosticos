//! Unified Marketplace Backend
//!
//! Single source of truth for the AGNOS agent/app marketplace. Combines
//! local registry, remote registry, publisher management, and package
//! lifecycle into a unified service that backs both the CLI and REST API.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use tracing::{info, warn};

// ---------------------------------------------------------------------------
// Publisher Management
// ---------------------------------------------------------------------------

/// A registered marketplace publisher.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Publisher {
    /// Unique publisher ID.
    pub id: String,
    /// Display name.
    pub display_name: String,
    /// Ed25519 public key ID (hex, first 8 bytes).
    pub key_id: String,
    /// Publisher email.
    pub email: String,
    /// Homepage URL.
    #[serde(default)]
    pub homepage: String,
    /// Whether the publisher is verified.
    pub verified: bool,
    /// Number of published packages.
    pub package_count: u32,
    /// Registration timestamp (Unix seconds).
    pub registered_at: u64,
    /// Publisher status.
    pub status: PublisherStatus,
}

/// Publisher account status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PublisherStatus {
    Active,
    Suspended,
    PendingVerification,
}

// ---------------------------------------------------------------------------
// Package Metadata
// ---------------------------------------------------------------------------

/// A package in the unified registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageEntry {
    /// Package name (lowercase, alphanumeric + hyphens).
    pub name: String,
    /// Latest version.
    pub latest_version: String,
    /// All published versions.
    pub versions: Vec<VersionEntry>,
    /// Publisher ID.
    pub publisher_id: String,
    /// Category.
    pub category: String,
    /// Tags.
    pub tags: Vec<String>,
    /// Short description.
    pub description: String,
    /// Total downloads.
    pub downloads: u64,
    /// Average rating (0.0–5.0).
    pub average_rating: f64,
    /// Number of ratings.
    pub rating_count: u32,
    /// Whether the package is featured.
    pub featured: bool,
    /// Creation timestamp.
    pub created_at: u64,
    /// Last update timestamp.
    pub updated_at: u64,
}

/// A specific version of a package.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionEntry {
    /// Semver version string.
    pub version: String,
    /// SHA-256 of the bundle.
    pub sha256: String,
    /// Bundle size in bytes.
    pub size_bytes: u64,
    /// Minimum AGNOS version required.
    pub min_agnos_version: String,
    /// Publication timestamp.
    pub published_at: u64,
    /// Whether this version has been yanked.
    pub yanked: bool,
    /// Changelog for this version.
    #[serde(default)]
    pub changelog: String,
    /// Download URL.
    pub download_url: String,
    /// Signature URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature_url: Option<String>,
}

// ---------------------------------------------------------------------------
// Unified Marketplace Backend
// ---------------------------------------------------------------------------

/// The unified marketplace backend service.
#[derive(Debug, Clone)]
pub struct MarketplaceBackend {
    /// Registered publishers: publisher_id → Publisher.
    publishers: HashMap<String, Publisher>,
    /// Package registry: package_name → PackageEntry.
    packages: HashMap<String, PackageEntry>,
    /// Featured package names (ordered).
    featured: Vec<String>,
    /// Maximum packages per publisher.
    max_packages_per_publisher: u32,
}

impl MarketplaceBackend {
    /// Create a new marketplace backend.
    pub fn new() -> Self {
        info!("Unified marketplace backend initialised");
        Self {
            publishers: HashMap::new(),
            packages: HashMap::new(),
            featured: Vec::new(),
            max_packages_per_publisher: 100,
        }
    }

    // --- Publisher management ---

    /// Register a new publisher.
    pub fn register_publisher(&mut self, publisher: Publisher) -> Result<(), MarketplaceError> {
        if self.publishers.contains_key(&publisher.id) {
            return Err(MarketplaceError::PublisherExists(publisher.id));
        }
        if publisher.display_name.is_empty() {
            return Err(MarketplaceError::ValidationError(
                "display_name is required".to_string(),
            ));
        }
        if publisher.key_id.is_empty() {
            return Err(MarketplaceError::ValidationError(
                "key_id is required".to_string(),
            ));
        }

        info!(publisher_id = %publisher.id, name = %publisher.display_name, "Registered publisher");
        self.publishers.insert(publisher.id.clone(), publisher);
        Ok(())
    }

    /// Get a publisher by ID.
    pub fn get_publisher(&self, publisher_id: &str) -> Option<&Publisher> {
        self.publishers.get(publisher_id)
    }

    /// Suspend a publisher (blocks new publishes, doesn't remove packages).
    pub fn suspend_publisher(&mut self, publisher_id: &str) -> Result<(), MarketplaceError> {
        let publisher = self
            .publishers
            .get_mut(publisher_id)
            .ok_or_else(|| MarketplaceError::PublisherNotFound(publisher_id.to_string()))?;
        publisher.status = PublisherStatus::Suspended;
        warn!(publisher_id, "Publisher suspended");
        Ok(())
    }

    /// Verify a publisher.
    pub fn verify_publisher(&mut self, publisher_id: &str) -> Result<(), MarketplaceError> {
        let publisher = self
            .publishers
            .get_mut(publisher_id)
            .ok_or_else(|| MarketplaceError::PublisherNotFound(publisher_id.to_string()))?;
        publisher.verified = true;
        publisher.status = PublisherStatus::Active;
        info!(publisher_id, "Publisher verified");
        Ok(())
    }

    /// List all publishers.
    pub fn list_publishers(&self) -> Vec<&Publisher> {
        let mut pubs: Vec<_> = self.publishers.values().collect();
        pubs.sort_by(|a, b| a.display_name.cmp(&b.display_name));
        pubs
    }

    /// Number of registered publishers.
    pub fn publisher_count(&self) -> usize {
        self.publishers.len()
    }

    // --- Package management ---

    /// Publish a new package version.
    pub fn publish(
        &mut self,
        publisher_id: &str,
        name: &str,
        version: VersionEntry,
    ) -> Result<(), MarketplaceError> {
        // Validate publisher.
        let publisher = self
            .publishers
            .get(publisher_id)
            .ok_or_else(|| MarketplaceError::PublisherNotFound(publisher_id.to_string()))?;

        if publisher.status == PublisherStatus::Suspended {
            return Err(MarketplaceError::PublisherSuspended(
                publisher_id.to_string(),
            ));
        }

        if let Some(entry) = self.packages.get(name) {
            // Existing package — verify same publisher.
            if entry.publisher_id != publisher_id {
                return Err(MarketplaceError::NotOwner {
                    package: name.to_string(),
                    publisher: publisher_id.to_string(),
                });
            }
            // Check for duplicate version.
            if entry.versions.iter().any(|v| v.version == version.version) {
                return Err(MarketplaceError::VersionExists {
                    package: name.to_string(),
                    version: version.version,
                });
            }
        } else {
            // Check publisher package limit.
            let pub_packages = self
                .packages
                .values()
                .filter(|p| p.publisher_id == publisher_id)
                .count() as u32;
            if pub_packages >= self.max_packages_per_publisher {
                return Err(MarketplaceError::LimitExceeded(format!(
                    "publisher has {} packages (max {})",
                    pub_packages, self.max_packages_per_publisher
                )));
            }
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let entry = self
            .packages
            .entry(name.to_string())
            .or_insert_with(|| PackageEntry {
                name: name.to_string(),
                latest_version: String::new(),
                versions: Vec::new(),
                publisher_id: publisher_id.to_string(),
                category: String::new(),
                tags: Vec::new(),
                description: String::new(),
                downloads: 0,
                average_rating: 0.0,
                rating_count: 0,
                featured: false,
                created_at: now,
                updated_at: now,
            });

        entry.latest_version = version.version.clone();
        entry.updated_at = now;
        entry.versions.push(version);

        // Update publisher package count.
        if let Some(pub_entry) = self.publishers.get_mut(publisher_id) {
            pub_entry.package_count = self
                .packages
                .values()
                .filter(|p| p.publisher_id == publisher_id)
                .count() as u32;
        }

        info!(package = %name, publisher = %publisher_id, "Package version published");
        Ok(())
    }

    /// Yank a specific version (soft-delete, still resolvable but not recommended).
    pub fn yank_version(&mut self, name: &str, version: &str) -> Result<(), MarketplaceError> {
        let entry = self
            .packages
            .get_mut(name)
            .ok_or_else(|| MarketplaceError::PackageNotFound(name.to_string()))?;

        let ver = entry
            .versions
            .iter_mut()
            .find(|v| v.version == version)
            .ok_or_else(|| MarketplaceError::VersionNotFound {
                package: name.to_string(),
                version: version.to_string(),
            })?;

        ver.yanked = true;
        warn!(package = %name, version, "Version yanked");
        Ok(())
    }

    /// Get a package by name.
    pub fn get_package(&self, name: &str) -> Option<&PackageEntry> {
        self.packages.get(name)
    }

    /// Search packages by query string (name or tag match).
    pub fn search(&self, query: &str, category: Option<&str>) -> Vec<&PackageEntry> {
        let query_lower = query.to_lowercase();
        let mut results: Vec<_> = self
            .packages
            .values()
            .filter(|p| {
                let name_match = p.name.to_lowercase().contains(&query_lower);
                let tag_match = p
                    .tags
                    .iter()
                    .any(|t| t.to_lowercase().contains(&query_lower));
                let desc_match = p.description.to_lowercase().contains(&query_lower);
                let text_match = name_match || tag_match || desc_match;

                let cat_match = category
                    .map(|c| p.category.eq_ignore_ascii_case(c))
                    .unwrap_or(true);

                text_match && cat_match
            })
            .collect();

        // Sort by downloads descending.
        results.sort_by(|a, b| b.downloads.cmp(&a.downloads));
        results
    }

    /// Get featured packages.
    pub fn featured_packages(&self) -> Vec<&PackageEntry> {
        self.featured
            .iter()
            .filter_map(|name| self.packages.get(name))
            .collect()
    }

    /// Set featured packages.
    pub fn set_featured(&mut self, names: Vec<String>) {
        self.featured = names;
    }

    /// Record a download for a package.
    pub fn record_download(&mut self, name: &str) {
        if let Some(entry) = self.packages.get_mut(name) {
            entry.downloads += 1;
        }
    }

    /// Update package metadata (description, category, tags).
    pub fn update_metadata(
        &mut self,
        name: &str,
        description: Option<&str>,
        category: Option<&str>,
        tags: Option<Vec<String>>,
    ) -> Result<(), MarketplaceError> {
        let entry = self
            .packages
            .get_mut(name)
            .ok_or_else(|| MarketplaceError::PackageNotFound(name.to_string()))?;

        if let Some(desc) = description {
            entry.description = desc.to_string();
        }
        if let Some(cat) = category {
            entry.category = cat.to_string();
        }
        if let Some(t) = tags {
            entry.tags = t;
        }
        entry.updated_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Ok(())
    }

    /// Submit a rating for a package.
    pub fn submit_rating(&mut self, name: &str, score: f64) -> Result<(), MarketplaceError> {
        if !(0.0..=5.0).contains(&score) {
            return Err(MarketplaceError::ValidationError(
                "rating must be between 0.0 and 5.0".to_string(),
            ));
        }
        let entry = self
            .packages
            .get_mut(name)
            .ok_or_else(|| MarketplaceError::PackageNotFound(name.to_string()))?;

        // Running average.
        let total = entry.average_rating * entry.rating_count as f64 + score;
        entry.rating_count += 1;
        entry.average_rating = total / entry.rating_count as f64;

        Ok(())
    }

    /// Number of packages.
    pub fn package_count(&self) -> usize {
        self.packages.len()
    }

    /// Get backend statistics.
    pub fn stats(&self) -> BackendStats {
        let total_downloads: u64 = self.packages.values().map(|p| p.downloads).sum();
        let total_versions: usize = self.packages.values().map(|p| p.versions.len()).sum();

        BackendStats {
            publishers: self.publishers.len(),
            packages: self.packages.len(),
            total_versions,
            total_downloads,
            featured: self.featured.len(),
            verified_publishers: self.publishers.values().filter(|p| p.verified).count(),
        }
    }
}

impl Default for MarketplaceBackend {
    fn default() -> Self {
        Self::new()
    }
}

/// Marketplace backend statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendStats {
    pub publishers: usize,
    pub packages: usize,
    pub total_versions: usize,
    pub total_downloads: u64,
    pub featured: usize,
    pub verified_publishers: usize,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Marketplace backend errors.
#[derive(Debug, Clone, PartialEq)]
pub enum MarketplaceError {
    PublisherExists(String),
    PublisherNotFound(String),
    PublisherSuspended(String),
    PackageNotFound(String),
    VersionExists { package: String, version: String },
    VersionNotFound { package: String, version: String },
    NotOwner { package: String, publisher: String },
    ValidationError(String),
    LimitExceeded(String),
}

impl std::fmt::Display for MarketplaceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PublisherExists(id) => write!(f, "publisher already exists: {}", id),
            Self::PublisherNotFound(id) => write!(f, "publisher not found: {}", id),
            Self::PublisherSuspended(id) => write!(f, "publisher suspended: {}", id),
            Self::PackageNotFound(n) => write!(f, "package not found: {}", n),
            Self::VersionExists { package, version } => {
                write!(f, "version {} already exists for {}", version, package)
            }
            Self::VersionNotFound { package, version } => {
                write!(f, "version {} not found for {}", version, package)
            }
            Self::NotOwner { package, publisher } => {
                write!(f, "{} is not the owner of {}", publisher, package)
            }
            Self::ValidationError(msg) => write!(f, "validation error: {}", msg),
            Self::LimitExceeded(msg) => write!(f, "limit exceeded: {}", msg),
        }
    }
}

impl std::error::Error for MarketplaceError {}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_publisher() -> Publisher {
        Publisher {
            id: "pub-001".to_string(),
            display_name: "AGNOS Team".to_string(),
            key_id: "deadbeef".to_string(),
            email: "team@agnos.org".to_string(),
            homepage: "https://agnos.org".to_string(),
            verified: true,
            package_count: 0,
            registered_at: 1710000000,
            status: PublisherStatus::Active,
        }
    }

    fn test_version() -> VersionEntry {
        VersionEntry {
            version: "1.0.0".to_string(),
            sha256: "abc123".to_string(),
            size_bytes: 1024,
            min_agnos_version: "2026.3.10".to_string(),
            published_at: 1710000000,
            yanked: false,
            changelog: "Initial release".to_string(),
            download_url: "https://registry.agnos.org/packages/test/1.0.0".to_string(),
            signature_url: None,
        }
    }

    fn test_backend() -> MarketplaceBackend {
        let mut backend = MarketplaceBackend::new();
        backend.register_publisher(test_publisher()).unwrap();
        backend
    }

    #[test]
    fn test_register_publisher() {
        let backend = test_backend();
        assert_eq!(backend.publisher_count(), 1);
        assert!(backend.get_publisher("pub-001").is_some());
        assert!(backend.get_publisher("unknown").is_none());
    }

    #[test]
    fn test_register_publisher_duplicate() {
        let mut backend = test_backend();
        assert!(matches!(
            backend.register_publisher(test_publisher()).unwrap_err(),
            MarketplaceError::PublisherExists(_)
        ));
    }

    #[test]
    fn test_register_publisher_missing_name() {
        let mut backend = MarketplaceBackend::new();
        let mut pub_ = test_publisher();
        pub_.id = "new".to_string();
        pub_.display_name = String::new();
        assert!(matches!(
            backend.register_publisher(pub_).unwrap_err(),
            MarketplaceError::ValidationError(_)
        ));
    }

    #[test]
    fn test_suspend_publisher() {
        let mut backend = test_backend();
        backend.suspend_publisher("pub-001").unwrap();
        assert_eq!(
            backend.get_publisher("pub-001").unwrap().status,
            PublisherStatus::Suspended
        );
    }

    #[test]
    fn test_verify_publisher() {
        let mut backend = MarketplaceBackend::new();
        let mut pub_ = test_publisher();
        pub_.verified = false;
        pub_.status = PublisherStatus::PendingVerification;
        backend.register_publisher(pub_).unwrap();
        backend.verify_publisher("pub-001").unwrap();
        let p = backend.get_publisher("pub-001").unwrap();
        assert!(p.verified);
        assert_eq!(p.status, PublisherStatus::Active);
    }

    #[test]
    fn test_list_publishers() {
        let mut backend = test_backend();
        let mut pub2 = test_publisher();
        pub2.id = "pub-002".to_string();
        pub2.display_name = "Another Publisher".to_string();
        backend.register_publisher(pub2).unwrap();
        let list = backend.list_publishers();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].display_name, "AGNOS Team");
        assert_eq!(list[1].display_name, "Another Publisher");
    }

    #[test]
    fn test_publish_package() {
        let mut backend = test_backend();
        backend
            .publish("pub-001", "my-agent", test_version())
            .unwrap();
        assert_eq!(backend.package_count(), 1);
        let pkg = backend.get_package("my-agent").unwrap();
        assert_eq!(pkg.latest_version, "1.0.0");
        assert_eq!(pkg.versions.len(), 1);
        assert_eq!(pkg.publisher_id, "pub-001");
    }

    #[test]
    fn test_publish_multiple_versions() {
        let mut backend = test_backend();
        backend
            .publish("pub-001", "my-agent", test_version())
            .unwrap();
        let mut v2 = test_version();
        v2.version = "1.1.0".to_string();
        backend.publish("pub-001", "my-agent", v2).unwrap();
        let pkg = backend.get_package("my-agent").unwrap();
        assert_eq!(pkg.latest_version, "1.1.0");
        assert_eq!(pkg.versions.len(), 2);
    }

    #[test]
    fn test_publish_duplicate_version() {
        let mut backend = test_backend();
        backend
            .publish("pub-001", "my-agent", test_version())
            .unwrap();
        assert!(matches!(
            backend
                .publish("pub-001", "my-agent", test_version())
                .unwrap_err(),
            MarketplaceError::VersionExists { .. }
        ));
    }

    #[test]
    fn test_publish_wrong_publisher() {
        let mut backend = test_backend();
        let mut pub2 = test_publisher();
        pub2.id = "pub-002".to_string();
        pub2.display_name = "Other".to_string();
        backend.register_publisher(pub2).unwrap();

        backend
            .publish("pub-001", "my-agent", test_version())
            .unwrap();

        let mut v2 = test_version();
        v2.version = "2.0.0".to_string();
        assert!(matches!(
            backend.publish("pub-002", "my-agent", v2).unwrap_err(),
            MarketplaceError::NotOwner { .. }
        ));
    }

    #[test]
    fn test_publish_suspended_publisher() {
        let mut backend = test_backend();
        backend.suspend_publisher("pub-001").unwrap();
        assert!(matches!(
            backend
                .publish("pub-001", "my-agent", test_version())
                .unwrap_err(),
            MarketplaceError::PublisherSuspended(_)
        ));
    }

    #[test]
    fn test_yank_version() {
        let mut backend = test_backend();
        backend
            .publish("pub-001", "my-agent", test_version())
            .unwrap();
        backend.yank_version("my-agent", "1.0.0").unwrap();
        let pkg = backend.get_package("my-agent").unwrap();
        assert!(pkg.versions[0].yanked);
    }

    #[test]
    fn test_yank_nonexistent_version() {
        let mut backend = test_backend();
        backend
            .publish("pub-001", "my-agent", test_version())
            .unwrap();
        assert!(matches!(
            backend.yank_version("my-agent", "9.9.9").unwrap_err(),
            MarketplaceError::VersionNotFound { .. }
        ));
    }

    #[test]
    fn test_search_by_name() {
        let mut backend = test_backend();
        backend
            .publish("pub-001", "code-reviewer", test_version())
            .unwrap();
        backend
            .update_metadata("code-reviewer", Some("AI code review"), None, None)
            .unwrap();

        let results = backend.search("code", None);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "code-reviewer");
    }

    #[test]
    fn test_search_by_tag() {
        let mut backend = test_backend();
        backend
            .publish("pub-001", "scanner", test_version())
            .unwrap();
        backend
            .update_metadata("scanner", None, None, Some(vec!["security".to_string()]))
            .unwrap();

        let results = backend.search("security", None);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_search_with_category() {
        let mut backend = test_backend();
        backend
            .publish("pub-001", "agent-a", test_version())
            .unwrap();
        backend
            .update_metadata("agent-a", Some("Agent A"), Some("utility"), None)
            .unwrap();
        backend
            .publish("pub-001", "agent-b", {
                let mut v = test_version();
                v.version = "1.0.0".to_string();
                v
            })
            .unwrap();
        backend
            .update_metadata("agent-b", Some("Agent B"), Some("security"), None)
            .unwrap();

        let results = backend.search("agent", Some("security"));
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "agent-b");
    }

    #[test]
    fn test_featured_packages() {
        let mut backend = test_backend();
        backend
            .publish("pub-001", "featured-app", test_version())
            .unwrap();
        backend.set_featured(vec!["featured-app".to_string()]);
        let featured = backend.featured_packages();
        assert_eq!(featured.len(), 1);
        assert_eq!(featured[0].name, "featured-app");
    }

    #[test]
    fn test_record_download() {
        let mut backend = test_backend();
        backend
            .publish("pub-001", "popular", test_version())
            .unwrap();
        for _ in 0..5 {
            backend.record_download("popular");
        }
        assert_eq!(backend.get_package("popular").unwrap().downloads, 5);
    }

    #[test]
    fn test_submit_rating() {
        let mut backend = test_backend();
        backend.publish("pub-001", "rated", test_version()).unwrap();
        backend.submit_rating("rated", 4.0).unwrap();
        backend.submit_rating("rated", 5.0).unwrap();
        let pkg = backend.get_package("rated").unwrap();
        assert_eq!(pkg.rating_count, 2);
        assert!((pkg.average_rating - 4.5).abs() < 0.01);
    }

    #[test]
    fn test_submit_rating_invalid() {
        let mut backend = test_backend();
        backend.publish("pub-001", "x", test_version()).unwrap();
        assert!(matches!(
            backend.submit_rating("x", 6.0).unwrap_err(),
            MarketplaceError::ValidationError(_)
        ));
        assert!(matches!(
            backend.submit_rating("x", -1.0).unwrap_err(),
            MarketplaceError::ValidationError(_)
        ));
    }

    #[test]
    fn test_stats() {
        let mut backend = test_backend();
        backend.publish("pub-001", "a", test_version()).unwrap();
        let mut v2 = test_version();
        v2.version = "1.1.0".to_string();
        backend.publish("pub-001", "a", v2).unwrap();
        backend.publish("pub-001", "b", test_version()).unwrap();
        backend.record_download("a");
        backend.record_download("a");
        backend.record_download("b");

        let stats = backend.stats();
        assert_eq!(stats.publishers, 1);
        assert_eq!(stats.packages, 2);
        assert_eq!(stats.total_versions, 3);
        assert_eq!(stats.total_downloads, 3);
        assert_eq!(stats.verified_publishers, 1);
    }

    #[test]
    fn test_marketplace_error_display() {
        assert_eq!(
            MarketplaceError::PublisherExists("x".to_string()).to_string(),
            "publisher already exists: x"
        );
        assert_eq!(
            MarketplaceError::PackageNotFound("y".to_string()).to_string(),
            "package not found: y"
        );
        let err = MarketplaceError::NotOwner {
            package: "pkg".to_string(),
            publisher: "bad-pub".to_string(),
        };
        assert!(err.to_string().contains("bad-pub"));
    }

    #[test]
    fn test_publisher_status_serialization() {
        let json = serde_json::to_string(&PublisherStatus::Suspended).unwrap();
        assert_eq!(json, "\"Suspended\"");
        let parsed: PublisherStatus = serde_json::from_str("\"PendingVerification\"").unwrap();
        assert_eq!(parsed, PublisherStatus::PendingVerification);
    }

    #[test]
    fn test_version_entry_serialization() {
        let v = test_version();
        let json = serde_json::to_string(&v).unwrap();
        let parsed: VersionEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.version, "1.0.0");
        assert_eq!(parsed.sha256, "abc123");
        assert!(!parsed.yanked);
    }

    #[test]
    fn test_backend_stats_serialization() {
        let stats = BackendStats {
            publishers: 3,
            packages: 15,
            total_versions: 42,
            total_downloads: 10000,
            featured: 2,
            verified_publishers: 2,
        };
        let json = serde_json::to_string(&stats).unwrap();
        let parsed: BackendStats = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.total_downloads, 10000);
    }

    #[test]
    fn test_update_metadata() {
        let mut backend = test_backend();
        backend.publish("pub-001", "app", test_version()).unwrap();
        backend
            .update_metadata(
                "app",
                Some("A great app"),
                Some("productivity"),
                Some(vec!["ai".to_string(), "tools".to_string()]),
            )
            .unwrap();
        let pkg = backend.get_package("app").unwrap();
        assert_eq!(pkg.description, "A great app");
        assert_eq!(pkg.category, "productivity");
        assert_eq!(pkg.tags, vec!["ai", "tools"]);
    }

    #[test]
    fn test_publisher_package_count_updated() {
        let mut backend = test_backend();
        backend.publish("pub-001", "a", test_version()).unwrap();
        backend.publish("pub-001", "b", test_version()).unwrap();
        let publisher = backend.get_publisher("pub-001").unwrap();
        assert_eq!(publisher.package_count, 2);
    }
}
