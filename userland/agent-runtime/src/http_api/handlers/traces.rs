use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::http_api::state::ApiState;
use crate::http_api::MAX_TRACES;

// ---------------------------------------------------------------------------
// OTLP configuration types
// ---------------------------------------------------------------------------

/// OTLP collector configuration returned to external consumers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OtlpConfig {
    /// The OTLP endpoint URL for trace export.
    pub endpoint: String,
    /// Protocol to use ("grpc" or "http/protobuf").
    pub protocol: String,
    /// Export interval in seconds.
    pub export_interval_seconds: u64,
    /// Sampling rate (0.0 to 1.0).
    pub sampling_rate: f64,
    /// Resource attributes to include in exported spans.
    pub resource_attributes: std::collections::HashMap<String, String>,
    /// Whether the OTLP collector is enabled.
    pub enabled: bool,
}

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

/// GET /v1/traces/otlp-config — return OTLP collector configuration for external consumers.
pub async fn otlp_config_handler() -> impl IntoResponse {
    let endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
        .unwrap_or_else(|_| "http://127.0.0.1:4317".to_string());
    let protocol = std::env::var("OTEL_EXPORTER_OTLP_PROTOCOL")
        .unwrap_or_else(|_| "grpc".to_string());
    let export_interval: u64 = std::env::var("OTEL_BSP_SCHEDULE_DELAY")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5000)
        / 1000;
    let sampling_rate: f64 = std::env::var("OTEL_TRACES_SAMPLER_ARG")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1.0);
    let enabled = std::env::var("AGNOS_OTLP_ENABLED")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(true);

    let mut resource_attributes = std::collections::HashMap::new();
    resource_attributes.insert("service.name".to_string(), "agnos-agent-runtime".to_string());
    resource_attributes.insert("service.version".to_string(), env!("CARGO_PKG_VERSION").to_string());

    if let Ok(hostname) = std::env::var("HOSTNAME") {
        resource_attributes.insert("host.name".to_string(), hostname);
    }

    let config = OtlpConfig {
        endpoint,
        protocol,
        export_interval_seconds: export_interval,
        sampling_rate,
        resource_attributes,
        enabled,
    };

    Json(serde_json::to_value(config).unwrap())
}

pub async fn list_spans_handler(State(state): State<ApiState>) -> impl IntoResponse {
    let json = state.span_collector.export_json();
    Json(serde_json::json!({
        "spans": json,
        "format": "otlp-like"
    }))
}
