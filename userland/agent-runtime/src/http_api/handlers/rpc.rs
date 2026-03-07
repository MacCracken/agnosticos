use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use agnos_common::AgentId;

use crate::http_api::state::ApiState;

// ---------------------------------------------------------------------------
// RPC request/response types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcRegisterRequest {
    pub agent_id: String,
    pub methods: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcCallRequest {
    pub method: String,
    pub params: serde_json::Value,
    #[serde(default = "default_rpc_timeout")]
    pub timeout_ms: u64,
    #[serde(default)]
    pub sender_id: Option<String>,
}

fn default_rpc_timeout() -> u64 {
    5000
}

// ---------------------------------------------------------------------------
// RPC handlers
// ---------------------------------------------------------------------------

pub async fn rpc_list_methods_handler(State(state): State<ApiState>) -> impl IntoResponse {
    let registry = state.rpc_registry.read().await;
    let methods: Vec<_> = registry
        .all_methods()
        .into_iter()
        .map(|(method, agent_id)| {
            serde_json::json!({
                "method": method,
                "handler_agent": agent_id.to_string(),
            })
        })
        .collect();
    Json(serde_json::json!({
        "methods": methods,
    }))
}

pub async fn rpc_agent_methods_handler(
    State(state): State<ApiState>,
    Path(agent_id): Path<String>,
) -> impl IntoResponse {
    let parsed = match Uuid::parse_str(&agent_id) {
        Ok(u) => AgentId(u),
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "invalid agent_id UUID",
                    "agent_id": agent_id,
                })),
            )
                .into_response();
        }
    };
    let registry = state.rpc_registry.read().await;
    Json(serde_json::json!({
        "agent_id": agent_id,
        "methods": registry.list_methods(&parsed),
    }))
    .into_response()
}

pub async fn rpc_register_handler(
    State(state): State<ApiState>,
    Json(req): Json<RpcRegisterRequest>,
) -> impl IntoResponse {
    let parsed = match Uuid::parse_str(&req.agent_id) {
        Ok(u) => AgentId(u),
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "invalid agent_id UUID",
                    "agent_id": req.agent_id,
                })),
            )
                .into_response();
        }
    };
    let mut registry = state.rpc_registry.write().await;
    for method in &req.methods {
        registry.register_method(parsed, method);
    }
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "registered",
            "agent_id": req.agent_id,
            "methods": req.methods,
        })),
    )
        .into_response()
}

pub async fn rpc_call_handler(
    State(state): State<ApiState>,
    Json(req): Json<RpcCallRequest>,
) -> impl IntoResponse {
    let registry = state.rpc_registry.read().await;
    match registry.find_handler(&req.method) {
        Some(handler_id) => Json(serde_json::json!({
            "status": "routed",
            "method": req.method,
            "handler_agent": handler_id.to_string(),
            "message": "RPC call dispatched (async response pending)"
        }))
        .into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "method_not_found",
                "method": req.method,
            })),
        )
            .into_response(),
    }
}
