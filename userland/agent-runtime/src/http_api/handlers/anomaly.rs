use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use agnos_common::AgentId;

use crate::http_api::handlers::audit::AuditEvent;
use crate::http_api::state::ApiState;
use crate::http_api::MAX_AUDIT_BUFFER;
use crate::learning::BehaviorSample;

// ---------------------------------------------------------------------------
// Anomaly detection request/response types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehaviorSampleRequest {
    pub agent_id: String,
    pub syscall_count: u64,
    pub network_bytes: u64,
    pub file_ops: u64,
    pub cpu_percent: f64,
    pub memory_bytes: u64,
}

// ---------------------------------------------------------------------------
// Anomaly detection handlers
// ---------------------------------------------------------------------------

pub async fn anomaly_submit_handler(
    State(state): State<ApiState>,
    Json(req): Json<BehaviorSampleRequest>,
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
    let sample = BehaviorSample {
        timestamp: chrono::Utc::now(),
        syscall_count: req.syscall_count,
        network_bytes: req.network_bytes,
        file_ops: req.file_ops,
        cpu_percent: req.cpu_percent,
        memory_bytes: req.memory_bytes,
    };
    let mut detector = state.anomaly_detector.write().await;
    let alerts = detector.record_behavior(parsed, sample);

    // Log alerts to audit buffer if any
    if !alerts.is_empty() {
        let mut audit = state.audit_buffer.write().await;
        for alert in &alerts {
            if audit.len() < MAX_AUDIT_BUFFER {
                audit.push_back(AuditEvent {
                    timestamp: chrono::Utc::now().to_rfc3339(),
                    action: format!("anomaly_detected:{}", alert.metric),
                    agent: Some(req.agent_id.clone()),
                    details: serde_json::json!({
                        "severity": format!("{:?}", alert.severity),
                        "metric": alert.metric,
                        "current_value": alert.current_value,
                        "baseline_mean": alert.baseline_mean,
                        "deviation_sigmas": alert.deviation_sigmas,
                    }),
                    outcome: "alert".to_string(),
                });
            }
        }
    }

    Json(serde_json::json!({
        "status": "recorded",
        "agent_id": req.agent_id,
        "alerts": alerts.iter().map(|a| serde_json::json!({
            "metric": a.metric,
            "severity": format!("{:?}", a.severity),
            "current_value": a.current_value,
            "baseline_mean": a.baseline_mean,
            "deviation_sigmas": a.deviation_sigmas,
        })).collect::<Vec<_>>(),
    }))
    .into_response()
}

pub async fn anomaly_alerts_handler(State(state): State<ApiState>) -> impl IntoResponse {
    let detector = state.anomaly_detector.read().await;
    let alerts = detector.active_alerts();
    Json(serde_json::json!({
        "alerts": alerts.iter().map(|a| serde_json::json!({
            "agent_id": a.agent_id.to_string(),
            "metric": a.metric,
            "severity": format!("{:?}", a.severity),
            "current_value": a.current_value,
            "baseline_mean": a.baseline_mean,
            "baseline_stddev": a.baseline_stddev,
            "deviation_sigmas": a.deviation_sigmas,
            "timestamp": a.timestamp.to_rfc3339(),
        })).collect::<Vec<_>>(),
        "total": alerts.len(),
    }))
}

pub async fn anomaly_baseline_handler(
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
    let detector = state.anomaly_detector.read().await;
    match detector.get_baseline(&parsed) {
        Some(baseline) => Json(serde_json::json!({
            "agent_id": agent_id,
            "sample_count": baseline.sample_count(),
            "has_baseline": baseline.sample_count() > 0,
        }))
        .into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "no baseline for agent",
                "agent_id": agent_id,
            })),
        )
            .into_response(),
    }
}

pub async fn anomaly_clear_handler(
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
    let mut detector = state.anomaly_detector.write().await;
    detector.clear_alerts(&parsed);
    Json(serde_json::json!({
        "status": "cleared",
        "agent_id": agent_id,
    }))
    .into_response()
}
