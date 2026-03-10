use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;

use crate::http_api::state::ApiState;

// ---------------------------------------------------------------------------
// Memory types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct MemorySetRequest {
    pub value: serde_json::Value,
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Maximum allowed serialized size of a memory value (1 MB).
const MEMORY_VALUE_MAX_BYTES: usize = 1_048_576;

// ---------------------------------------------------------------------------
// Memory handlers
// ---------------------------------------------------------------------------

pub async fn memory_get_handler(
    State(state): State<ApiState>,
    Path((id, key)): Path<(String, String)>,
) -> impl IntoResponse {
    match state.memory_store.get(&id, &key).await {
        Some(value) => (
            StatusCode::OK,
            Json(serde_json::json!({"key": key, "agent_id": id, "value": value})),
        )
            .into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": format!("Key '{}' not found", key), "code": 404})),
        )
            .into_response(),
    }
}

pub async fn memory_set_handler(
    State(state): State<ApiState>,
    Path((id, key)): Path<(String, String)>,
    Json(req): Json<MemorySetRequest>,
) -> impl IntoResponse {
    let serialized_size = serde_json::to_string(&req.value)
        .map(|s| s.len())
        .unwrap_or(0);
    if serialized_size > MEMORY_VALUE_MAX_BYTES {
        return (
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(serde_json::json!({
                "error": format!(
                    "Memory value too large: {} bytes exceeds {} byte limit",
                    serialized_size, MEMORY_VALUE_MAX_BYTES
                ),
                "code": 413
            })),
        )
            .into_response();
    }

    state.memory_store.set(&id, &key, req.value).await;
    (
        StatusCode::OK,
        Json(serde_json::json!({"status": "stored", "key": key})),
    )
        .into_response()
}

pub async fn memory_list_handler(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let keys = state.memory_store.list_keys(&id).await;
    let total = keys.len();
    Json(serde_json::json!({"keys": keys, "total": total}))
}

pub async fn memory_delete_handler(
    State(state): State<ApiState>,
    Path((id, key)): Path<(String, String)>,
) -> impl IntoResponse {
    if state.memory_store.delete(&id, &key).await {
        (
            StatusCode::OK,
            Json(serde_json::json!({"status": "deleted", "key": key})),
        )
            .into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": format!("Key '{}' not found", key), "code": 404})),
        )
            .into_response()
    }
}
