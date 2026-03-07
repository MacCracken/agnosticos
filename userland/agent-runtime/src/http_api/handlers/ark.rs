use std::collections::HashMap;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::http_api::state::ApiState;

// ---------------------------------------------------------------------------
// Ark request types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArkInstallRequest {
    pub packages: Vec<String>,
    #[serde(default)]
    pub force: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArkRemoveRequest {
    pub packages: Vec<String>,
    #[serde(default)]
    pub purge: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArkUpgradeRequest {
    pub packages: Option<Vec<String>>,
}

// ---------------------------------------------------------------------------
// Ark handlers
// ---------------------------------------------------------------------------

pub async fn ark_install_handler(
    State(_state): State<ApiState>,
    Json(req): Json<ArkInstallRequest>,
) -> impl IntoResponse {
    if req.packages.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "No packages specified"})),
        )
            .into_response();
    }
    let steps: Vec<serde_json::Value> = req
        .packages
        .iter()
        .map(|p| serde_json::json!({"action": "install", "package": p, "source": "auto"}))
        .collect();
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "planned",
            "steps": steps,
            "message": format!("Planned installation of {} package(s)", req.packages.len()),
            "force": req.force,
        })),
    )
        .into_response()
}

pub async fn ark_remove_handler(
    State(_state): State<ApiState>,
    Json(req): Json<ArkRemoveRequest>,
) -> impl IntoResponse {
    if req.packages.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "No packages specified"})),
        )
            .into_response();
    }
    let steps: Vec<serde_json::Value> = req
        .packages
        .iter()
        .map(|p| serde_json::json!({"action": "remove", "package": p, "purge": req.purge}))
        .collect();
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "planned",
            "steps": steps,
            "message": format!("Planned removal of {} package(s)", req.packages.len()),
        })),
    )
        .into_response()
}

pub async fn ark_search_handler(
    State(_state): State<ApiState>,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let query = params.get("q").cloned().unwrap_or_default();
    if query.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Missing query parameter 'q'"})),
        )
            .into_response();
    }
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "query": query,
            "results": [],
            "sources_searched": ["marketplace", "system"],
            "total": 0,
        })),
    )
        .into_response()
}

pub async fn ark_info_handler(
    State(_state): State<ApiState>,
    Path(package): Path<String>,
) -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "name": package,
            "status": "not_installed",
            "available_sources": ["system", "marketplace"],
            "versions": {},
        })),
    )
}

pub async fn ark_update_handler(State(_state): State<ApiState>) -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "planned",
            "steps": [{"action": "refresh_index", "sources": ["system", "marketplace", "flutter"]}],
            "message": "Planned index refresh across all sources",
        })),
    )
}

pub async fn ark_upgrade_handler(
    State(_state): State<ApiState>,
    Json(req): Json<ArkUpgradeRequest>,
) -> impl IntoResponse {
    let msg = match &req.packages {
        Some(pkgs) => format!("Planned upgrade of {} specific package(s)", pkgs.len()),
        None => "Planned upgrade of all outdated packages".to_string(),
    };
    let steps: Vec<serde_json::Value> = match &req.packages {
        Some(pkgs) => pkgs
            .iter()
            .map(|p| serde_json::json!({"action": "upgrade", "package": p}))
            .collect(),
        None => vec![serde_json::json!({"action": "upgrade_all"})],
    };
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "planned",
            "steps": steps,
            "message": msg,
        })),
    )
}

pub async fn ark_status_handler() -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "version": env!("CARGO_PKG_VERSION"),
            "sources": ["system", "marketplace", "flutter"],
            "resolver": "nous",
        })),
    )
}
