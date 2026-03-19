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
    let limit = params.limit.min(1000); // cap at 1000 per page
    let page: Vec<&_> = entries.iter().skip(start).take(limit).collect();
    Json(serde_json::json!({
        "entries": page,
        "total": total,
        "offset": start,
        "limit": limit,
    }))
}

// ---------------------------------------------------------------------------
// Sutra playbook run record ingestion (T3)
// ---------------------------------------------------------------------------

/// A single task result from a sutra playbook run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SutraTaskResult {
    pub module: String,
    pub action: String,
    #[serde(default)]
    pub changed: bool,
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub duration_ms: Option<u64>,
}

/// A sutra RunRecord — the result of executing a playbook.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SutraRunRecord {
    pub run_id: String,
    pub playbook: String,
    #[serde(default)]
    pub started_at: Option<String>,
    #[serde(default)]
    pub finished_at: Option<String>,
    #[serde(default)]
    pub node_id: Option<String>,
    #[serde(default)]
    pub success: bool,
    #[serde(default)]
    pub tasks: Vec<SutraTaskResult>,
}

/// `POST /v1/audit/runs` — accept a sutra RunRecord for centralized audit.
pub async fn audit_runs_handler(
    State(state): State<ApiState>,
    Json(record): Json<SutraRunRecord>,
) -> impl IntoResponse {
    // Validate required fields
    if record.run_id.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "run_id is required"})),
        );
    }
    if record.playbook.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "playbook name is required"})),
        );
    }

    let run_id = record.run_id.clone();
    let task_count = record.tasks.len();

    info!(
        "Sutra audit: run_id={} playbook={} node={:?} success={} tasks={}",
        record.run_id, record.playbook, record.node_id, record.success, task_count
    );

    // Build an audit event from the run record
    let audit_event = AuditEvent {
        timestamp: record
            .finished_at
            .clone()
            .unwrap_or_else(|| chrono::Utc::now().to_rfc3339()),
        action: format!(
            "sutra.playbook.{}",
            if record.success { "success" } else { "failure" }
        ),
        agent: record.node_id.clone(),
        details: serde_json::to_value(&record).unwrap_or_default(),
        outcome: if record.success {
            "success".to_string()
        } else {
            "failure".to_string()
        },
    };

    // Append to the audit buffer and cryptographic chain
    let mut buffer = state.audit_buffer.write().await;
    let mut chain = state.audit_chain.write().await;

    let chain_event = agnos_common::audit::AuditEvent {
        sequence: 0,
        timestamp: record
            .finished_at
            .as_deref()
            .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(chrono::Utc::now),
        event_type: agnos_common::audit::AuditEventType::ExternalAudit,
        agent_id: None,
        user_id: agnos_common::UserId::new(),
        action: format!("sutra.run.{}", record.playbook),
        resource: record.node_id.unwrap_or_default(),
        result: if record.success {
            agnos_common::audit::AuditResult::Success
        } else {
            agnos_common::audit::AuditResult::Failure
        },
        details: serde_json::to_value(&record.tasks).unwrap_or_default(),
    };
    chain.append(chain_event);

    if buffer.len() >= MAX_AUDIT_BUFFER {
        buffer.pop_front();
    }
    buffer.push_back(audit_event);

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "accepted": true,
            "run_id": run_id,
            "tasks_recorded": task_count,
        })),
    )
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
