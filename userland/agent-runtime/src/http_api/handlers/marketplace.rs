use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;

use crate::http_api::state::ApiState;

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

pub async fn marketplace_search_handler(
    State(state): State<ApiState>,
    Query(params): Query<MarketplaceSearchQuery>,
) -> impl IntoResponse {
    let registry = state.marketplace_registry.read().await;
    let results: Vec<serde_json::Value> = registry
        .search(&params.q)
        .iter()
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
        "total": results.len(),
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
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": format!("Invalid path: {}", e)})),
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

    // Run the blocking install_package off the async thread
    let registry = state.marketplace_registry.clone();
    let result = tokio::task::spawn_blocking(move || {
        let mut reg = registry.blocking_write();
        reg.install_package(&canonical)
            .map(|r| {
                serde_json::json!({
                    "status": "installed",
                    "name": r.name,
                    "version": r.version,
                    "install_dir": r.install_dir.to_string_lossy(),
                    "upgraded_from": r.upgraded_from,
                })
            })
            .map_err(|e| e.to_string())
    })
    .await;

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
