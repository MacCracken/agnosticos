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

/// Maximum length for an RPC method name.
const MAX_RPC_METHOD_NAME_LEN: usize = 256;

/// Validate an RPC method name: alphanumeric + dots + underscores + hyphens, max 256 chars.
/// Rejects control characters, null bytes, and names exceeding the length limit.
fn validate_rpc_method_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("RPC method name must not be empty".to_string());
    }
    if name.len() > MAX_RPC_METHOD_NAME_LEN {
        return Err(format!(
            "RPC method name too long: {} chars exceeds {} char limit",
            name.len(),
            MAX_RPC_METHOD_NAME_LEN
        ));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-')
    {
        return Err(
            "RPC method name may only contain alphanumeric characters, dots, underscores, and hyphens"
                .to_string(),
        );
    }
    Ok(())
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
    // H28: Parse and canonicalize UUID to prevent reflection of unsanitized input
    let parsed = match Uuid::parse_str(&agent_id) {
        Ok(u) => AgentId(u),
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "invalid agent_id UUID",
                })),
            )
                .into_response();
        }
    };
    // Reflect the canonical lowercase-hyphenated form, not the raw user input
    let canonical_id = parsed.0.to_string();
    let registry = state.rpc_registry.read().await;
    Json(serde_json::json!({
        "agent_id": canonical_id,
        "methods": registry.list_methods(&parsed),
    }))
    .into_response()
}

pub async fn rpc_register_handler(
    State(state): State<ApiState>,
    Json(req): Json<RpcRegisterRequest>,
) -> impl IntoResponse {
    // H28: Parse and canonicalize UUID
    let parsed = match Uuid::parse_str(&req.agent_id) {
        Ok(u) => AgentId(u),
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "invalid agent_id UUID",
                })),
            )
                .into_response();
        }
    };
    let canonical_id = parsed.0.to_string();
    // H11: Validate all method names before registering
    for method in &req.methods {
        if let Err(e) = validate_rpc_method_name(method) {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": e,
                    "method": method,
                    "code": 400
                })),
            )
                .into_response();
        }
    }

    let mut registry = state.rpc_registry.write().await;
    for method in &req.methods {
        registry.register_method(parsed, method);
    }
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "registered",
            "agent_id": canonical_id,
            "methods": req.methods,
        })),
    )
        .into_response()
}

pub async fn rpc_call_handler(
    State(state): State<ApiState>,
    Json(req): Json<RpcCallRequest>,
) -> impl IntoResponse {
    // H11: Validate method name
    if let Err(e) = validate_rpc_method_name(&req.method) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": e,
                "method": req.method,
                "code": 400
            })),
        )
            .into_response();
    }

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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    fn build_test_router() -> axum::Router {
        let state = ApiState::with_api_key(None);
        crate::http_api::build_router(state)
    }

    /// H28: Verify that a non-canonical UUID is canonicalized in the response.
    #[tokio::test]
    async fn test_rpc_agent_methods_canonicalizes_uuid() {
        let router = build_test_router();
        let req = Request::builder()
            .method("GET")
            .uri("/v1/rpc/methods/550E8400-E29B-41D4-A716-446655440000")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), 1_048_576)
            .await
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            parsed["agent_id"].as_str().unwrap(),
            "550e8400-e29b-41d4-a716-446655440000"
        );
    }

    /// H28: Verify that invalid UUID is rejected with 400 and raw input not reflected.
    #[tokio::test]
    async fn test_rpc_agent_methods_rejects_invalid_uuid() {
        let router = build_test_router();
        let req = Request::builder()
            .method("GET")
            .uri("/v1/rpc/methods/not-a-uuid")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = axum::body::to_bytes(resp.into_body(), 1_048_576)
            .await
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(parsed.get("agent_id").is_none());
        assert!(parsed["error"].as_str().unwrap().contains("invalid"));
    }
}
