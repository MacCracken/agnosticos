use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;
use tracing::{info, warn};

use crate::http_api::state::ApiState;
use crate::marketplace::remote_client::RegistryClient;

// ---------------------------------------------------------------------------
// Marketplace types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct MarketplaceSearchQuery {
    #[serde(default)]
    pub q: String,
}

#[derive(Debug, Deserialize)]
pub struct MarketplaceInstallRequest {
    pub path: String,
}

#[derive(Debug, Deserialize)]
pub struct RemoteSearchQuery {
    #[serde(default)]
    pub q: String,
    pub category: Option<String>,
    #[serde(default = "default_page")]
    pub page: u32,
}

fn default_page() -> u32 {
    1
}

#[derive(Debug, Deserialize)]
pub struct RemoteInstallRequest {
    pub name: String,
    pub version: String,
}

// ---------------------------------------------------------------------------
// Marketplace handlers
// ---------------------------------------------------------------------------

pub async fn marketplace_installed_handler(State(state): State<ApiState>) -> impl IntoResponse {
    let registry = state.marketplace_registry.read().await;
    let packages: Vec<serde_json::Value> = registry
        .list_installed()
        .iter()
        .map(|p| {
            serde_json::json!({
                "name": p.name(),
                "version": p.version(),
                "publisher": p.publisher(),
                "category": format!("{}", p.manifest.category),
                "installed_at": p.installed_at.to_rfc3339(),
                "installed_size": p.installed_size,
            })
        })
        .collect();
    Json(serde_json::json!({
        "packages": packages,
        "total": packages.len(),
    }))
}

/// Maximum search results returned per query.
const MARKETPLACE_SEARCH_MAX_RESULTS: usize = 100;

pub async fn marketplace_search_handler(
    State(state): State<ApiState>,
    Query(params): Query<MarketplaceSearchQuery>,
) -> impl IntoResponse {
    let registry = state.marketplace_registry.read().await;
    let all_results = registry.search(&params.q);
    let total = all_results.len();
    let results: Vec<serde_json::Value> = all_results
        .iter()
        .take(MARKETPLACE_SEARCH_MAX_RESULTS)
        .map(|p| {
            serde_json::json!({
                "name": p.name(),
                "version": p.version(),
                "publisher": p.publisher(),
                "description": p.manifest.agent.description,
                "category": format!("{}", p.manifest.category),
            })
        })
        .collect();
    Json(serde_json::json!({
        "results": results,
        "total": total,
        "returned": results.len(),
        "query": params.q,
    }))
}

pub async fn marketplace_install_handler(
    State(state): State<ApiState>,
    Json(req): Json<MarketplaceInstallRequest>,
) -> impl IntoResponse {
    // Path traversal protection: canonicalize and restrict to allowed directories
    let raw_path = std::path::Path::new(&req.path);
    let canonical = match raw_path.canonicalize() {
        Ok(p) => p,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "Invalid or inaccessible path"})),
            )
                .into_response();
        }
    };
    let allowed_prefixes = ["/var/agnos/", "/tmp/agnos/"];
    if !allowed_prefixes
        .iter()
        .any(|prefix| canonical.starts_with(prefix))
    {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": "Path not in allowed install directories",
                "allowed": allowed_prefixes,
            })),
        )
            .into_response();
    }

    // Staged install with transaction isolation (H22):
    // copy -> verify -> install -> commit/rollback
    let registry = state.marketplace_registry.clone();
    let result =
        tokio::task::spawn_blocking(move || staged_install(&registry, &canonical, None)).await;

    match result {
        Ok(Ok(json)) => (StatusCode::OK, Json(json)).into_response(),
        Ok(Err(e)) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": e})),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Install task failed: {}", e)})),
        )
            .into_response(),
    }
}

pub async fn marketplace_uninstall_handler(
    State(state): State<ApiState>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let mut registry = state.marketplace_registry.write().await;
    match registry.uninstall_package(&name) {
        Ok(result) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "uninstalled",
                "name": result.name,
                "version": result.version,
                "files_removed": result.files_removed,
            })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

pub async fn marketplace_info_handler(
    State(state): State<ApiState>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let registry = state.marketplace_registry.read().await;
    match registry.get_package(&name) {
        Some(pkg) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "name": pkg.name(),
                "version": pkg.version(),
                "publisher": pkg.publisher(),
                "description": pkg.manifest.agent.description,
                "category": format!("{}", pkg.manifest.category),
                "runtime": pkg.manifest.runtime,
                "installed_at": pkg.installed_at.to_rfc3339(),
                "installed_size": pkg.installed_size,
                "auto_update": pkg.auto_update,
                "package_hash": pkg.package_hash,
                "tags": pkg.manifest.tags,
                "dependencies": pkg.manifest.dependencies,
            })),
        )
            .into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": format!("Package '{}' not found", name)})),
        )
            .into_response(),
    }
}

// ---------------------------------------------------------------------------
// Remote marketplace handlers
// ---------------------------------------------------------------------------

/// Helper to build a `RegistryClient` from environment or defaults.
fn build_registry_client() -> Result<RegistryClient, String> {
    let base_url = std::env::var("AGNOS_REGISTRY_URL")
        .unwrap_or_else(|_| crate::marketplace::remote_client::DEFAULT_REGISTRY_URL.to_string());
    let cache_dir = std::env::var("AGNOS_MARKETPLACE_CACHE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("/run/agnos/marketplace-cache"));
    RegistryClient::new(&base_url, &cache_dir).map_err(|e| e.to_string())
}

/// `GET /v1/marketplace/remote/search?q=...&category=...&page=N`
///
/// Proxy search requests to the remote marketplace registry.
pub async fn marketplace_remote_search_handler(
    Query(params): Query<RemoteSearchQuery>,
) -> impl IntoResponse {
    let client = match build_registry_client() {
        Ok(c) => c,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("Failed to create registry client: {}", e)})),
            )
                .into_response();
        }
    };

    match client
        .search(&params.q, params.category.as_deref(), params.page)
        .await
    {
        Ok(results) => {
            (StatusCode::OK, Json(serde_json::to_value(results).unwrap())).into_response()
        }
        Err(e) => (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({"error": format!("Remote search failed: {}", e)})),
        )
            .into_response(),
    }
}

/// `GET /v1/marketplace/remote/:name`
///
/// Fetch the manifest for a remote package. The version defaults to "latest".
pub async fn marketplace_remote_info_handler(Path(name): Path<String>) -> impl IntoResponse {
    let client = match build_registry_client() {
        Ok(c) => c,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("Failed to create registry client: {}", e)})),
            )
                .into_response();
        }
    };

    match client.fetch_manifest(&name, "latest").await {
        Ok(manifest) => (
            StatusCode::OK,
            Json(serde_json::to_value(manifest).unwrap()),
        )
            .into_response(),
        Err(e) => (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({"error": format!("Failed to fetch manifest: {}", e)})),
        )
            .into_response(),
    }
}

/// `POST /v1/marketplace/remote/install`
///
/// Download a package from the remote registry, verify its signature against
/// the local keyring, then install it locally.
pub async fn marketplace_remote_install_handler(
    State(state): State<ApiState>,
    Json(req): Json<RemoteInstallRequest>,
) -> impl IntoResponse {
    let client = match build_registry_client() {
        Ok(c) => c,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("Failed to create registry client: {}", e)})),
            )
                .into_response();
        }
    };

    // Download the package tarball from the remote registry
    let tarball_path = match client.download_package(&req.name, &req.version).await {
        Ok(p) => p,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({
                    "error": format!("Failed to download package: {}", e)
                })),
            )
                .into_response();
        }
    };

    // Staged install with transaction isolation (H22).
    let registry = state.marketplace_registry.clone();
    let result =
        tokio::task::spawn_blocking(move || staged_install(&registry, &tarball_path, None)).await;

    match result {
        Ok(Ok(json)) => (StatusCode::OK, Json(json)).into_response(),
        Ok(Err(e)) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": e})),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Install task failed: {}", e)})),
        )
            .into_response(),
    }
}

/// Perform a staged install with cleanup on failure (H22).
pub(crate) fn staged_install(
    registry: &std::sync::Arc<
        tokio::sync::RwLock<crate::marketplace::local_registry::LocalRegistry>,
    >,
    tarball_path: &std::path::Path,
    keyring: Option<&crate::marketplace::trust::PublisherKeyring>,
) -> Result<serde_json::Value, String> {
    let staging_dir = std::path::PathBuf::from(format!(
        "/run/agnos/marketplace-stage-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&staging_dir)
        .map_err(|e| format!("Failed to create staging dir: {}", e))?;
    let staged_tarball = staging_dir.join(
        tarball_path
            .file_name()
            .unwrap_or_else(|| std::ffi::OsStr::new("package.tar")),
    );
    std::fs::copy(tarball_path, &staged_tarball).map_err(|e| {
        let _ = std::fs::remove_dir_all(&staging_dir);
        format!("Failed to stage tarball: {}", e)
    })?;
    info!(staging_dir = %staging_dir.display(), "Marketplace install staged");
    let meta = std::fs::metadata(&staged_tarball).map_err(|e| {
        let _ = std::fs::remove_dir_all(&staging_dir);
        format!("Staged tarball metadata error: {}", e)
    })?;
    if meta.len() == 0 {
        let _ = std::fs::remove_dir_all(&staging_dir);
        return Err("Staged tarball is empty".to_string());
    }
    let install_result = {
        let mut reg = registry.blocking_write();
        reg.install_package(&staged_tarball, keyring)
    };
    match install_result {
        Ok(r) => {
            let _ = std::fs::remove_dir_all(&staging_dir);
            info!(name = %r.name, version = %r.version, "Marketplace install committed");
            Ok(serde_json::json!({
                "status": "installed",
                "name": r.name,
                "version": r.version,
                "install_dir": r.install_dir.to_string_lossy(),
                "upgraded_from": r.upgraded_from,
            }))
        }
        Err(e) => {
            warn!(error = %e, "Marketplace install failed -- rolling back");
            let _ = std::fs::remove_dir_all(&staging_dir);
            if let Ok(data) = std::fs::read(tarball_path) {
                if let Ok(manifest) =
                    crate::marketplace::local_registry::extract_manifest_from_tarball(&data)
                {
                    let reg = registry.blocking_read();
                    let partial_dir = reg.packages_dir().join(&manifest.agent.name);
                    if partial_dir.exists() {
                        let _ = std::fs::remove_dir_all(&partial_dir);
                        info!(dir = %partial_dir.display(), "Removed partially installed package directory");
                    }
                }
            }
            Err(format!("Install failed (rolled back): {}", e))
        }
    }
}
