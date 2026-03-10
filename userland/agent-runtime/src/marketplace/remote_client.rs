//! Marketplace Remote Client
//!
//! HTTP client for communicating with a remote marketplace registry.
//! Supports searching, fetching manifests, downloading packages, and
//! checking for updates.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use super::local_registry::InstalledMarketplacePackage;
use super::MarketplaceManifest;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Default remote registry URL.
pub const DEFAULT_REGISTRY_URL: &str = "https://registry.agnos.org";

/// Request timeout.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Download timeout (longer for large packages).
pub const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(300);

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Search results from the remote registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResults {
    /// Matching packages.
    pub packages: Vec<SearchResult>,
    /// Total number of matches (for pagination).
    pub total: u64,
    /// Current page.
    pub page: u32,
    /// Items per page.
    pub per_page: u32,
}

/// A single search result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// Fully-qualified name (publisher/name).
    pub qualified_name: String,
    /// Latest version.
    pub latest_version: String,
    /// Short description.
    pub description: String,
    /// Download count.
    pub downloads: u64,
    /// Category.
    pub category: String,
    /// Publisher name.
    pub publisher: String,
}

/// Response from publishing a package.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishResponse {
    /// Published package name.
    pub name: String,
    /// Published version.
    pub version: String,
    /// Registry-assigned download URL.
    pub download_url: String,
    /// Whether this version replaced an existing one.
    pub replaced: bool,
}

/// An available update for an installed package.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateAvailable {
    /// Package name.
    pub name: String,
    /// Currently installed version.
    pub installed_version: String,
    /// Available version.
    pub available_version: String,
    /// Changelog for the new version.
    pub changelog: String,
}

// ---------------------------------------------------------------------------
// Remote client
// ---------------------------------------------------------------------------

/// HTTP client for the remote marketplace registry.
pub struct RegistryClient {
    /// Base URL of the registry.
    base_url: String,
    /// HTTP client.
    client: reqwest::Client,
    /// Local cache directory for downloaded manifests.
    cache_dir: PathBuf,
    /// Whether to operate in offline mode.
    offline: bool,
}

impl RegistryClient {
    /// Create a new registry client.
    pub fn new(base_url: &str, cache_dir: &Path) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .user_agent(format!("agnos-marketplace/{}", env!("CARGO_PKG_VERSION")))
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            client,
            cache_dir: cache_dir.to_path_buf(),
            offline: false,
        })
    }

    /// Enable offline mode (use cached data only).
    pub fn set_offline(&mut self, offline: bool) {
        self.offline = offline;
    }

    /// Whether operating in offline mode.
    pub fn is_offline(&self) -> bool {
        self.offline
    }

    /// Base URL of the registry.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Search for packages.
    pub async fn search(
        &self,
        query: &str,
        category: Option<&str>,
        page: u32,
    ) -> Result<SearchResults> {
        if self.offline {
            return self.cached_search(query);
        }

        let mut url = format!("{}/v1/packages/search?q={}", self.base_url, query);
        if let Some(cat) = category {
            url.push_str(&format!("&category={}", cat));
        }
        url.push_str(&format!("&page={}", page));

        debug!("Searching registry: {}", url);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to reach registry")?;

        if !response.status().is_success() {
            anyhow::bail!("Registry search failed: HTTP {}", response.status());
        }

        let results: SearchResults = response
            .json()
            .await
            .context("Failed to parse search results")?;

        // Cache results
        self.cache_search_results(query, &results)?;

        Ok(results)
    }

    /// Fetch manifest for a specific package version.
    pub async fn fetch_manifest(&self, name: &str, version: &str) -> Result<MarketplaceManifest> {
        if self.offline {
            return self.cached_manifest(name, version);
        }

        let url = format!("{}/v1/packages/{}/{}", self.base_url, name, version);

        debug!("Fetching manifest: {}", url);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to reach registry")?;

        if !response.status().is_success() {
            anyhow::bail!(
                "Failed to fetch manifest for {} v{}: HTTP {}",
                name,
                version,
                response.status()
            );
        }

        let manifest: MarketplaceManifest =
            response.json().await.context("Failed to parse manifest")?;

        // Cache manifest
        self.cache_manifest(name, version, &manifest)?;

        Ok(manifest)
    }

    /// Download a package tarball.
    pub async fn download_package(&self, name: &str, version: &str) -> Result<PathBuf> {
        if self.offline {
            anyhow::bail!("Cannot download in offline mode");
        }

        let url = format!(
            "{}/v1/packages/{}/{}/download",
            self.base_url, name, version
        );

        info!("Downloading package {} v{}", name, version);

        let response = self
            .client
            .get(&url)
            .timeout(DOWNLOAD_TIMEOUT)
            .send()
            .await
            .context("Failed to download package")?;

        if !response.status().is_success() {
            anyhow::bail!(
                "Failed to download {} v{}: HTTP {}",
                name,
                version,
                response.status()
            );
        }

        // Save to cache
        let download_dir = self.cache_dir.join("downloads");
        std::fs::create_dir_all(&download_dir)?;
        let dest = download_dir.join(format!("{}-{}.agnos-agent", name, version));

        let bytes = response
            .bytes()
            .await
            .context("Failed to read download body")?;

        std::fs::write(&dest, &bytes)
            .with_context(|| format!("Failed to write package to {}", dest.display()))?;

        info!(
            "Downloaded {} v{} ({} bytes) to {}",
            name,
            version,
            bytes.len(),
            dest.display()
        );

        Ok(dest)
    }

    /// Publish a package to the remote registry.
    ///
    /// Uploads the `.agnos-agent` bundle and optional `.sig` sidecar file.
    /// Requires a valid API token.
    pub async fn publish(
        &self,
        bundle_path: &Path,
        api_token: &str,
    ) -> Result<PublishResponse> {
        if self.offline {
            anyhow::bail!("Cannot publish in offline mode");
        }

        let bundle_bytes = std::fs::read(bundle_path)
            .with_context(|| format!("Failed to read bundle: {}", bundle_path.display()))?;

        let sha256 = {
            use sha2::{Digest, Sha256};
            let hash = Sha256::digest(&bundle_bytes);
            hash.iter().map(|b| format!("{:02x}", b)).collect::<String>()
        };

        let bundle_name = bundle_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let url = format!("{}/v1/packages/publish", self.base_url);

        info!(
            bundle = %bundle_name,
            size = bundle_bytes.len(),
            sha256 = %sha256,
            "Publishing package to registry"
        );

        // Check for signature sidecar
        let sig_path = bundle_path.with_extension("agnos-agent.sig");
        let signature = if sig_path.exists() {
            let sig_content = std::fs::read_to_string(&sig_path)?;
            info!("Including signature sidecar");
            Some(sig_content)
        } else {
            None
        };

        // Upload as JSON with base64-encoded bundle
        let payload = serde_json::json!({
            "bundle_name": bundle_name,
            "bundle_sha256": sha256,
            "bundle_size": bundle_bytes.len(),
            "signature": signature,
        });

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", api_token))
            .header("X-Package-SHA256", &sha256)
            .header("X-Package-Name", &bundle_name)
            .header("Content-Type", "application/octet-stream")
            .header("X-Package-Metadata", serde_json::to_string(&payload).unwrap_or_default())
            .body(bundle_bytes)
            .timeout(DOWNLOAD_TIMEOUT)
            .send()
            .await
            .context("Failed to reach registry for publish")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Publish failed (HTTP {}): {}", status, body);
        }

        let publish_resp: PublishResponse = response
            .json()
            .await
            .context("Failed to parse publish response")?;

        info!(
            name = %publish_resp.name,
            version = %publish_resp.version,
            "Package published successfully"
        );

        Ok(publish_resp)
    }

    /// Check for available updates given a list of installed packages.
    pub async fn check_updates(
        &self,
        installed: &[&InstalledMarketplacePackage],
    ) -> Result<Vec<UpdateAvailable>> {
        if self.offline {
            return Ok(Vec::new());
        }

        let mut updates = Vec::new();

        for pkg in installed {
            let url = format!("{}/v1/packages/{}/latest", self.base_url, pkg.name());

            match self.client.get(&url).send().await {
                Ok(response) if response.status().is_success() => {
                    if let Ok(manifest) = response.json::<MarketplaceManifest>().await {
                        if manifest.agent.version != pkg.version() {
                            updates.push(UpdateAvailable {
                                name: pkg.name().to_string(),
                                installed_version: pkg.version().to_string(),
                                available_version: manifest.agent.version,
                                changelog: manifest.changelog,
                            });
                        }
                    }
                }
                Ok(response) => {
                    debug!(
                        "Update check for {} returned HTTP {}",
                        pkg.name(),
                        response.status()
                    );
                }
                Err(e) => {
                    warn!("Failed to check updates for {}: {}", pkg.name(), e);
                }
            }
        }

        Ok(updates)
    }

    // -----------------------------------------------------------------------
    // Cache helpers
    // -----------------------------------------------------------------------

    fn cache_search_results(&self, query: &str, results: &SearchResults) -> Result<()> {
        let cache_dir = self.cache_dir.join("search");
        std::fs::create_dir_all(&cache_dir)?;
        let path = cache_dir.join(format!("{}.json", sanitize_filename(query)));
        let json = serde_json::to_string(results)?;
        std::fs::write(&path, json)?;
        Ok(())
    }

    fn cached_search(&self, query: &str) -> Result<SearchResults> {
        let path = self
            .cache_dir
            .join("search")
            .join(format!("{}.json", sanitize_filename(query)));
        if !path.exists() {
            anyhow::bail!("No cached search results for '{}' (offline mode)", query);
        }
        let content = std::fs::read_to_string(&path)?;
        Ok(serde_json::from_str(&content)?)
    }

    fn cache_manifest(
        &self,
        name: &str,
        version: &str,
        manifest: &MarketplaceManifest,
    ) -> Result<()> {
        let cache_dir = self.cache_dir.join("manifests").join(name);
        std::fs::create_dir_all(&cache_dir)?;
        let path = cache_dir.join(format!("{}.json", version));
        let json = serde_json::to_string(manifest)?;
        std::fs::write(&path, json)?;
        Ok(())
    }

    fn cached_manifest(&self, name: &str, version: &str) -> Result<MarketplaceManifest> {
        let path = self
            .cache_dir
            .join("manifests")
            .join(name)
            .join(format!("{}.json", version));
        if !path.exists() {
            anyhow::bail!(
                "No cached manifest for {} v{} (offline mode)",
                name,
                version
            );
        }
        let content = std::fs::read_to_string(&path)?;
        Ok(serde_json::from_str(&content)?)
    }
}

/// Sanitize a string for use as a filename.
fn sanitize_filename(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_client_creation() {
        let dir = tempfile::tempdir().unwrap();
        let client = RegistryClient::new("https://registry.agnos.org", dir.path()).unwrap();
        assert_eq!(client.base_url(), "https://registry.agnos.org");
        assert!(!client.is_offline());
    }

    #[test]
    fn test_registry_client_trailing_slash() {
        let dir = tempfile::tempdir().unwrap();
        let client = RegistryClient::new("https://registry.agnos.org/", dir.path()).unwrap();
        assert_eq!(client.base_url(), "https://registry.agnos.org");
    }

    #[test]
    fn test_set_offline() {
        let dir = tempfile::tempdir().unwrap();
        let mut client = RegistryClient::new(DEFAULT_REGISTRY_URL, dir.path()).unwrap();
        assert!(!client.is_offline());
        client.set_offline(true);
        assert!(client.is_offline());
    }

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("hello world"), "hello_world");
        assert_eq!(sanitize_filename("test@1.0"), "test_1_0");
        assert_eq!(sanitize_filename("a-b_c"), "a-b_c");
        assert_eq!(sanitize_filename(""), "");
    }

    #[test]
    fn test_search_results_serialization() {
        let results = SearchResults {
            packages: vec![SearchResult {
                qualified_name: "acme/scanner".to_string(),
                latest_version: "1.0.0".to_string(),
                description: "A scanner".to_string(),
                downloads: 1000,
                category: "security".to_string(),
                publisher: "Acme Corp".to_string(),
            }],
            total: 1,
            page: 1,
            per_page: 20,
        };
        let json = serde_json::to_string(&results).unwrap();
        let parsed: SearchResults = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.packages.len(), 1);
        assert_eq!(parsed.packages[0].qualified_name, "acme/scanner");
        assert_eq!(parsed.total, 1);
    }

    #[test]
    fn test_update_available_serialization() {
        let update = UpdateAvailable {
            name: "test-pkg".to_string(),
            installed_version: "1.0.0".to_string(),
            available_version: "2.0.0".to_string(),
            changelog: "New features".to_string(),
        };
        let json = serde_json::to_string(&update).unwrap();
        let parsed: UpdateAvailable = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "test-pkg");
        assert_eq!(parsed.available_version, "2.0.0");
    }

    #[tokio::test]
    async fn test_offline_search_no_cache() {
        let dir = tempfile::tempdir().unwrap();
        let mut client = RegistryClient::new(DEFAULT_REGISTRY_URL, dir.path()).unwrap();
        client.set_offline(true);
        let result = client.search("test", None, 1).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_offline_manifest_no_cache() {
        let dir = tempfile::tempdir().unwrap();
        let mut client = RegistryClient::new(DEFAULT_REGISTRY_URL, dir.path()).unwrap();
        client.set_offline(true);
        let result = client.fetch_manifest("test", "1.0.0").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_offline_download_blocked() {
        let dir = tempfile::tempdir().unwrap();
        let mut client = RegistryClient::new(DEFAULT_REGISTRY_URL, dir.path()).unwrap();
        client.set_offline(true);
        let result = client.download_package("test", "1.0.0").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("offline"));
    }

    #[tokio::test]
    async fn test_offline_updates_empty() {
        let dir = tempfile::tempdir().unwrap();
        let mut client = RegistryClient::new(DEFAULT_REGISTRY_URL, dir.path()).unwrap();
        client.set_offline(true);
        let updates = client.check_updates(&[]).await.unwrap();
        assert!(updates.is_empty());
    }

    #[test]
    fn test_cache_and_retrieve_search() {
        let dir = tempfile::tempdir().unwrap();
        let client = RegistryClient::new(DEFAULT_REGISTRY_URL, dir.path()).unwrap();

        let results = SearchResults {
            packages: vec![],
            total: 0,
            page: 1,
            per_page: 20,
        };
        client.cache_search_results("test query", &results).unwrap();

        let cached = client.cached_search("test query").unwrap();
        assert_eq!(cached.total, 0);
    }

    #[test]
    fn test_cache_and_retrieve_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let client = RegistryClient::new(DEFAULT_REGISTRY_URL, dir.path()).unwrap();

        let manifest = MarketplaceManifest {
            agent: agnos_common::AgentManifest {
                name: "test-pkg".to_string(),
                description: "A test".to_string(),
                version: "1.0.0".to_string(),
                ..Default::default()
            },
            publisher: crate::marketplace::PublisherInfo {
                name: "Tester".to_string(),
                key_id: "abc".to_string(),
                homepage: String::new(),
            },
            category: crate::marketplace::MarketplaceCategory::Utility,
            runtime: String::new(),
            screenshots: vec![],
            changelog: String::new(),
            min_agnos_version: String::new(),
            dependencies: std::collections::HashMap::new(),
            tags: vec![],
        };
        client
            .cache_manifest("test-pkg", "1.0.0", &manifest)
            .unwrap();

        let cached = client.cached_manifest("test-pkg", "1.0.0").unwrap();
        assert_eq!(cached.agent.name, "test-pkg");
    }

    #[test]
    fn test_default_constants() {
        assert!(DEFAULT_REGISTRY_URL.starts_with("https://"));
        assert!(REQUEST_TIMEOUT.as_secs() > 0);
        assert!(DOWNLOAD_TIMEOUT > REQUEST_TIMEOUT);
    }

    #[tokio::test]
    async fn test_offline_publish_blocked() {
        let dir = tempfile::tempdir().unwrap();
        let mut client = RegistryClient::new(DEFAULT_REGISTRY_URL, dir.path()).unwrap();
        client.set_offline(true);

        let fake_bundle = dir.path().join("test.agnos-agent");
        std::fs::write(&fake_bundle, b"fake").unwrap();

        let result = client.publish(&fake_bundle, "token").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("offline"));
    }

    #[test]
    fn test_publish_response_serialization() {
        let resp = PublishResponse {
            name: "test-app".to_string(),
            version: "1.0.0".to_string(),
            download_url: "https://registry.agnos.org/v1/packages/test-app/1.0.0/download".to_string(),
            replaced: false,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let parsed: PublishResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "test-app");
        assert!(!parsed.replaced);
    }
}
