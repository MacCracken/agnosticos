use std::collections::HashMap;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::http_api::state::ApiState;

/// Maximum number of reasoning traces kept per agent.
pub const MAX_REASONING_TRACES_PER_AGENT: usize = 1_000;
/// Maximum serialized size of a single reasoning trace (1 MB).
pub const MAX_REASONING_TRACE_BYTES: usize = 1_048_576;

// ---------------------------------------------------------------------------
// Reasoning trace types
// ---------------------------------------------------------------------------

/// A single reasoning step within a trace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningStep {
    /// The step number (1-indexed).
    pub step: u32,
    /// The type of reasoning (e.g., "observation", "thought", "action", "reflection").
    pub kind: String,
    /// The content/text of this reasoning step.
    pub content: String,
    /// Confidence score for this step (0.0 to 1.0).
    #[serde(default)]
    pub confidence: Option<f64>,
    /// Duration of this step in milliseconds.
    #[serde(default)]
    pub duration_ms: Option<u64>,
    /// Tool used during this step, if any.
    #[serde(default)]
    pub tool: Option<String>,
    /// Tool output, if any.
    #[serde(default)]
    pub tool_output: Option<String>,
}

/// A complete reasoning trace submitted by an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningTrace {
    /// The task or prompt that triggered this reasoning chain.
    pub task: String,
    /// Ordered list of reasoning steps.
    pub steps: Vec<ReasoningStep>,
    /// Final conclusion or answer.
    #[serde(default)]
    pub conclusion: Option<String>,
    /// Overall confidence in the conclusion (0.0 to 1.0).
    #[serde(default)]
    pub confidence: Option<f64>,
    /// Total duration of the reasoning chain in milliseconds.
    pub duration_ms: u64,
    /// The model used for inference, if applicable.
    #[serde(default)]
    pub model: Option<String>,
    /// Total tokens consumed during this reasoning chain.
    #[serde(default)]
    pub tokens_used: Option<u64>,
    /// Arbitrary metadata (e.g., session ID, crew name).
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

/// Stored reasoning trace with server-assigned fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredReasoningTrace {
    /// Server-assigned trace ID.
    pub trace_id: String,
    /// The agent that submitted this trace.
    pub agent_id: String,
    /// When this trace was received.
    pub received_at: DateTime<Utc>,
    /// The reasoning trace data.
    #[serde(flatten)]
    pub trace: ReasoningTrace,
}

/// Query parameters for listing reasoning traces.
#[derive(Debug, Deserialize)]
pub struct ReasoningQueryParams {
    /// Filter by minimum confidence.
    #[serde(default)]
    pub min_confidence: Option<f64>,
    /// Maximum number of results to return.
    #[serde(default)]
    pub limit: Option<usize>,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// POST /v1/agents/:id/reasoning — ingest a reasoning trace for an agent.
pub async fn submit_reasoning_handler(
    State(state): State<ApiState>,
    Path(id): Path<String>,
    Json(trace): Json<ReasoningTrace>,
) -> impl IntoResponse {
    // Validate agent_id is not empty
    if id.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Agent ID must not be empty", "code": 400})),
        )
            .into_response();
    }

    // Validate at least one step
    if trace.steps.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Reasoning trace must contain at least one step", "code": 400})),
        )
            .into_response();
    }

    // Validate task is not empty
    if trace.task.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Task description must not be empty", "code": 400})),
        )
            .into_response();
    }

    // Validate confidence ranges
    if let Some(c) = trace.confidence {
        if !(0.0..=1.0).contains(&c) {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "Confidence must be between 0.0 and 1.0", "code": 400})),
            )
                .into_response();
        }
    }
    for step in &trace.steps {
        if let Some(c) = step.confidence {
            if !(0.0..=1.0).contains(&c) {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({"error": format!("Step {} confidence must be between 0.0 and 1.0", step.step), "code": 400})),
                )
                    .into_response();
            }
        }
    }

    // H9: Check serialized size of the trace
    let trace_size = serde_json::to_vec(&trace).map(|v| v.len()).unwrap_or(0);
    if trace_size > MAX_REASONING_TRACE_BYTES {
        return (
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(serde_json::json!({
                "error": format!(
                    "Reasoning trace too large: {} bytes exceeds {} byte limit",
                    trace_size,
                    MAX_REASONING_TRACE_BYTES
                ),
                "code": 413
            })),
        )
            .into_response();
    }

    let trace_id = uuid::Uuid::new_v4().to_string();
    let step_count = trace.steps.len();
    let duration = trace.duration_ms;

    let stored = StoredReasoningTrace {
        trace_id: trace_id.clone(),
        agent_id: id.clone(),
        received_at: Utc::now(),
        trace,
    };

    info!(
        "Reasoning trace submitted: agent_id={} trace_id={} steps={} duration_ms={}",
        id, trace_id, step_count, duration
    );

    let mut store = state.reasoning_traces.write().await;
    let agent_traces = store.entry(id.clone()).or_default();

    // Evict oldest if at capacity
    if agent_traces.len() >= MAX_REASONING_TRACES_PER_AGENT {
        agent_traces.pop_front();
    }
    agent_traces.push_back(stored);

    let total = agent_traces.len();

    (
        StatusCode::CREATED,
        Json(serde_json::json!({
            "status": "accepted",
            "trace_id": trace_id,
            "agent_id": id,
            "steps_recorded": step_count,
            "total_traces": total
        })),
    )
        .into_response()
}

/// GET /v1/agents/:id/reasoning — list reasoning traces for an agent.
pub async fn list_reasoning_handler(
    State(state): State<ApiState>,
    Path(id): Path<String>,
    Query(params): Query<ReasoningQueryParams>,
) -> impl IntoResponse {
    let store = state.reasoning_traces.read().await;
    let empty = std::collections::VecDeque::new();
    let agent_traces = store.get(&id).unwrap_or(&empty);

    let mut result: Vec<&StoredReasoningTrace> = agent_traces.iter().collect();

    // Filter by minimum confidence if specified
    if let Some(min_conf) = params.min_confidence {
        result.retain(|t| t.trace.confidence.unwrap_or(0.0) >= min_conf);
    }

    // Apply limit (most recent first)
    let limit = params.limit.unwrap_or(100).min(1000);
    let total = result.len();
    if result.len() > limit {
        result = result.into_iter().rev().take(limit).collect();
        result.reverse();
    }

    Json(serde_json::json!({
        "agent_id": id,
        "traces": result,
        "total": total,
        "limit": limit
    }))
}
