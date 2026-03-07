use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::http_api::state::ApiState;
use crate::http_api::MAX_TRACES;

// ---------------------------------------------------------------------------
// Trace types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceStep {
    pub name: String,
    pub rationale: String,
    #[serde(default)]
    pub tool: Option<String>,
    #[serde(default)]
    pub output: Option<String>,
    pub duration_ms: u64,
    pub success: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceSubmitRequest {
    pub agent_id: String,
    pub input: String,
    pub steps: Vec<TraceStep>,
    #[serde(default)]
    pub result: Option<String>,
    pub duration_ms: u64,
}

#[derive(Debug, Deserialize)]
pub struct TraceQueryParams {
    #[serde(default)]
    pub agent_id: Option<String>,
}

// ---------------------------------------------------------------------------
// Trace handlers
// ---------------------------------------------------------------------------

pub async fn submit_trace_handler(
    State(state): State<ApiState>,
    Json(req): Json<TraceSubmitRequest>,
) -> impl IntoResponse {
    info!(
        "Trace submitted: agent_id={} steps={} duration_ms={}",
        req.agent_id,
        req.steps.len(),
        req.duration_ms
    );

    let trace_value = match serde_json::to_value(&req) {
        Ok(v) => v,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": format!("Serialization error: {}", e), "code": 400})),
            )
                .into_response();
        }
    };

    let mut traces = state.traces.write().await;
    if traces.len() >= MAX_TRACES {
        traces.pop_front();
    }
    traces.push_back(trace_value);

    (
        StatusCode::CREATED,
        Json(serde_json::json!({"status": "accepted", "trace_count": traces.len()})),
    )
        .into_response()
}

pub async fn list_traces_handler(
    State(state): State<ApiState>,
    Query(params): Query<TraceQueryParams>,
) -> impl IntoResponse {
    let traces = state.traces.read().await;
    let mut result: Vec<&serde_json::Value> = traces.iter().collect();

    if let Some(ref agent_id) = params.agent_id {
        result.retain(|t| t.get("agent_id").and_then(|v| v.as_str()) == Some(agent_id.as_str()));
    }

    Json(serde_json::json!({"traces": result, "total": result.len()}))
}

pub async fn list_spans_handler(State(state): State<ApiState>) -> impl IntoResponse {
    let json = state.span_collector.export_json();
    Json(serde_json::json!({
        "spans": json,
        "format": "otlp-like"
    }))
}
