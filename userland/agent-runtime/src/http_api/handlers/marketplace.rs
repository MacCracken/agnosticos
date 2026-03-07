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
    let mut registry = state.marketplace_registry.write().await;
    let path = std::path::Path::new(&req.path);

    match registry.install_package(path) {
        Ok(result) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "installed",
                "name": result.name,
                "version": result.version,
                "install_dir": result.install_dir.to_string_lossy(),
                "upgraded_from": result.upgraded_from,
            })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": e.to_string()})),
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
