use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;
use uuid::Uuid;

use crate::database::{AgentDatabaseRequirements, DatabaseManager};
use crate::http_api::state::ApiState;

// ---------------------------------------------------------------------------
// Request types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ProvisionDatabaseRequest {
    #[serde(default)]
    pub postgres: bool,
    #[serde(default)]
    pub redis: bool,
    #[serde(default)]
    pub schema: Option<String>,
    #[serde(default)]
    pub storage_quota: Option<u64>,
    #[serde(default)]
    pub extensions: Vec<String>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn parse_agent_id(id: &str) -> Result<agnos_common::AgentId, StatusCode> {
    let uuid: Uuid = id.parse().map_err(|_| StatusCode::BAD_REQUEST)?;
    Ok(agnos_common::AgentId(uuid))
}

fn bad_id_response() -> axum::response::Response {
    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({"error": "Invalid agent ID", "code": 400})),
    )
        .into_response()
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// `POST /v1/agents/:id/database` — Provision database resources for an agent.
pub async fn database_provision_handler(
    State(state): State<ApiState>,
    Path(id): Path<String>,
    Json(req): Json<ProvisionDatabaseRequest>,
) -> axum::response::Response {
    let agent_id = match parse_agent_id(&id) {
        Ok(id) => id,
        Err(_) => return bad_id_response(),
    };

    let requirements = AgentDatabaseRequirements {
        postgres: req.postgres,
        redis: req.redis,
        schema: req.schema,
        storage_quota: req.storage_quota,
        extensions: req.extensions,
    };

    let mut mgr: tokio::sync::RwLockWriteGuard<'_, DatabaseManager> =
        state.database_manager.write().await;
    match mgr.provision(agent_id, &requirements) {
        Ok(info) => {
            let sql = mgr.provision_sql(&agent_id).unwrap_or_default();
            (
                StatusCode::CREATED,
                Json(serde_json::json!({
                    "status": "provisioned",
                    "database": info,
                    "provision_sql": sql,
                })),
            )
                .into_response()
        }
        Err(e) => {
            let msg = format!("{e}");
            let status = if msg.contains("already provisioned") {
                StatusCode::CONFLICT
            } else if msg.contains("Maximum") {
                StatusCode::SERVICE_UNAVAILABLE
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            (
                status,
                Json(serde_json::json!({"error": msg, "code": status.as_u16()})),
            )
                .into_response()
        }
    }
}

/// `DELETE /v1/agents/:id/database` — Deprovision database resources for an agent.
pub async fn database_deprovision_handler(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> axum::response::Response {
    let agent_id = match parse_agent_id(&id) {
        Ok(id) => id,
        Err(_) => return bad_id_response(),
    };

    let mut mgr: tokio::sync::RwLockWriteGuard<'_, DatabaseManager> =
        state.database_manager.write().await;
    match mgr.deprovision(&agent_id) {
        Ok(commands) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "deprovisioned",
                "cleanup_sql": commands,
            })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("{e}"), "code": 500})),
        )
            .into_response(),
    }
}

/// `GET /v1/agents/:id/database` — Get provisioned database info for an agent.
pub async fn database_get_handler(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> axum::response::Response {
    let agent_id = match parse_agent_id(&id) {
        Ok(id) => id,
        Err(_) => return bad_id_response(),
    };

    let mgr: tokio::sync::RwLockReadGuard<'_, DatabaseManager> =
        state.database_manager.read().await;
    match mgr.get(&agent_id) {
        Some(info) => {
            (StatusCode::OK, Json(serde_json::json!({"database": info}))).into_response()
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "No database provisioned for agent", "code": 404})),
        )
            .into_response(),
    }
}

/// `GET /v1/database/stats` — Get database subsystem statistics.
pub async fn database_stats_handler(State(state): State<ApiState>) -> axum::response::Response {
    let mgr: tokio::sync::RwLockReadGuard<'_, DatabaseManager> =
        state.database_manager.read().await;
    let stats = mgr.stats();
    Json(serde_json::json!({"stats": stats})).into_response()
}
