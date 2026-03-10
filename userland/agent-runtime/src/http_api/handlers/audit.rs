use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::http_api::state::ApiState;
use crate::http_api::MAX_AUDIT_BUFFER;

// ---------------------------------------------------------------------------
// Audit types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub timestamp: String,
    pub action: String,
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub details: serde_json::Value,
    #[serde(default = "default_outcome")]
    pub outcome: String,
}

fn default_outcome() -> String {
    "unknown".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditForwardRequest {
    pub events: Vec<AuditEvent>,
    pub source: String,
    #[serde(default)]
    pub correlation_id: Option<String>,
}

/// Maximum number of audit events returned in a single list request.
const AUDIT_LIST_MAX_LIMIT: usize = 1000;

#[derive(Debug, Deserialize)]
pub struct AuditQueryParams {
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub offset: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct AuditChainQueryParams {
    #[serde(default = "default_chain_limit")]
    pub limit: usize,
    #[serde(default)]
    pub offset: usize,
}

fn default_chain_limit() -> usize {
    100
}

// ---------------------------------------------------------------------------
// Audit handlers
// ---------------------------------------------------------------------------

pub async fn forward_audit_handler(
    State(state): State<ApiState>,
    Json(req): Json<AuditForwardRequest>,
) -> impl IntoResponse {
    let count = req.events.len();
    info!(
        "Received {} audit events from source={} correlation_id={:?}",
        count, req.source, req.correlation_id
    );

    let mut buffer = state.audit_buffer.write().await;
    let mut chain = state.audit_chain.write().await;
    for event in req.events {
        info!(
            "Audit: action={} agent={:?} outcome={}",
            event.action, event.agent, event.outcome
        );

        // Also append to the cryptographic audit chain
        let chain_event = agnos_common::audit::AuditEvent {
            sequence: 0, // overwritten by AuditChain::append
            timestamp: chrono::DateTime::parse_from_rfc3339(&event.timestamp)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now()),
            event_type: agnos_common::audit::AuditEventType::ExternalAudit,
            agent_id: None,
            user_id: agnos_common::UserId::new(),
            action: event.action.clone(),
            resource: event.agent.clone().unwrap_or_default(),
            result: if event.outcome == "success" {
                agnos_common::audit::AuditResult::Success
            } else {
                agnos_common::audit::AuditResult::Failure
            },
            details: event.details.clone(),
        };
        chain.append(chain_event);

        if buffer.len() >= MAX_AUDIT_BUFFER {
            buffer.pop_front();
        }
        buffer.push_back(event);
    }

    (
        StatusCode::OK,
        Json(serde_json::json!({"status": "accepted", "events_received": count})),
    )
}

pub async fn list_audit_handler(
    State(state): State<ApiState>,
    Query(params): Query<AuditQueryParams>,
) -> impl IntoResponse {
    let buffer = state.audit_buffer.read().await;
    let mut events: Vec<&AuditEvent> = buffer.iter().collect();

    if let Some(ref agent) = params.agent {
        events.retain(|e| e.agent.as_deref() == Some(agent.as_str()));
    }
    if let Some(ref action) = params.action {
        events.retain(|e| e.action == *action);
    }

    let total = events.len();
    let offset = params.offset.unwrap_or(0).min(total);
    let limit = params
        .limit
        .unwrap_or(AUDIT_LIST_MAX_LIMIT)
        .min(AUDIT_LIST_MAX_LIMIT);
    let page: Vec<&AuditEvent> = events.into_iter().skip(offset).take(limit).collect();
    Json(serde_json::json!({"events": page, "total": total, "offset": offset, "limit": limit}))
}

pub async fn audit_chain_handler(
    State(state): State<ApiState>,
    Query(params): Query<AuditChainQueryParams>,
) -> impl IntoResponse {
    let chain = state.audit_chain.read().await;
    let entries = chain.entries();
    let total = entries.len();
    let start = params.offset.min(total);
    let end = (start + params.limit).min(total);
    let page = &entries[start..end];
    Json(serde_json::json!({
        "entries": page,
        "total": total,
        "offset": start,
        "limit": params.limit,
    }))
}

pub async fn audit_chain_verify_handler(State(state): State<ApiState>) -> impl IntoResponse {
    let chain = state.audit_chain.read().await;
    match chain.verify() {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({"valid": true, "entries": chain.len()})),
        ),
        Err(e) => (
            StatusCode::CONFLICT,
            Json(serde_json::json!({
                "valid": false,
                "error": e.to_string(),
                "position": e.position,
            })),
        ),
    }
}
