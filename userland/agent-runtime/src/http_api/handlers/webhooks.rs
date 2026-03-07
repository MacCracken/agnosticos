use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::info;
use uuid::Uuid;

use crate::http_api::state::ApiState;
use crate::http_api::MAX_WEBHOOKS;

// ---------------------------------------------------------------------------
// Webhook types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookRegistration {
    pub id: Uuid,
    pub url: String,
    pub events: Vec<String>,
    pub secret: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterWebhookRequest {
    pub url: String,
    #[serde(default)]
    pub events: Vec<String>,
    #[serde(default)]
    pub secret: Option<String>,
}

// ---------------------------------------------------------------------------
// Webhook handlers
// ---------------------------------------------------------------------------

pub async fn register_webhook_handler(
    State(state): State<ApiState>,
    Json(req): Json<RegisterWebhookRequest>,
) -> impl IntoResponse {
    if req.url.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Webhook URL is required", "code": 400})),
        )
            .into_response();
    }

    let mut webhooks = state.webhooks.write().await;

    if webhooks.len() >= MAX_WEBHOOKS {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": format!("Maximum webhook limit reached ({})", MAX_WEBHOOKS),
                "code": 503
            })),
        )
            .into_response();
    }

    let wh = WebhookRegistration {
        id: Uuid::new_v4(),
        url: req.url,
        events: req.events,
        secret: req.secret,
        created_at: Utc::now(),
    };

    let id = wh.id;
    webhooks.push(wh);
    info!("Webhook registered: {}", id);

    (
        StatusCode::CREATED,
        Json(serde_json::json!({"id": id.to_string(), "status": "registered"})),
    )
        .into_response()
}

pub async fn list_webhooks_handler(State(state): State<ApiState>) -> impl IntoResponse {
    let webhooks = state.webhooks.read().await;
    let list: Vec<serde_json::Value> = webhooks
        .iter()
        .map(|w| {
            serde_json::json!({
                "id": w.id.to_string(),
                "url": w.url,
                "events": w.events,
                "created_at": w.created_at.to_rfc3339(),
            })
        })
        .collect();
    Json(serde_json::json!({"webhooks": list, "total": list.len()}))
}

pub async fn delete_webhook_handler(
    State(state): State<ApiState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let mut webhooks = state.webhooks.write().await;
    let before = webhooks.len();
    webhooks.retain(|w| w.id != id);
    if webhooks.len() < before {
        info!("Webhook deleted: {}", id);
        (
            StatusCode::OK,
            Json(serde_json::json!({"status": "deleted", "id": id.to_string()})),
        )
            .into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": format!("Webhook {} not found", id), "code": 404})),
        )
            .into_response()
    }
}
