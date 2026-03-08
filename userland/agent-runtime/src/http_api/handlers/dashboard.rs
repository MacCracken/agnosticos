use std::collections::HashMap;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::http_api::state::ApiState;

/// Maximum number of dashboard snapshots kept in memory.
pub const MAX_DASHBOARD_SNAPSHOTS: usize = 500;

// ---------------------------------------------------------------------------
// Dashboard sync types
// ---------------------------------------------------------------------------

/// Status of a single agent within a dashboard snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStatus {
    /// Agent name or identifier.
    pub name: String,
    /// Current status (e.g., "active", "idle", "error", "stopped").
    pub status: String,
    /// Current task being executed, if any.
    #[serde(default)]
    pub current_task: Option<String>,
    /// CPU usage percentage.
    #[serde(default)]
    pub cpu_percent: Option<f32>,
    /// Memory usage in MB.
    #[serde(default)]
    pub memory_mb: Option<u64>,
    /// Number of tasks completed in this session.
    #[serde(default)]
    pub tasks_completed: Option<u64>,
    /// Number of errors encountered.
    #[serde(default)]
    pub error_count: Option<u64>,
}

/// Session-level metadata for the dashboard.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    /// Session identifier.
    pub session_id: String,
    /// When the session started.
    #[serde(default)]
    pub started_at: Option<DateTime<Utc>>,
    /// Duration of the session in seconds.
    #[serde(default)]
    pub duration_seconds: Option<u64>,
    /// Human-readable session description.
    #[serde(default)]
    pub description: Option<String>,
}

/// Aggregate metrics for the dashboard.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardMetrics {
    /// Total tokens consumed across all agents.
    #[serde(default)]
    pub total_tokens: Option<u64>,
    /// Total tasks completed.
    #[serde(default)]
    pub tasks_completed: Option<u64>,
    /// Total tasks failed.
    #[serde(default)]
    pub tasks_failed: Option<u64>,
    /// Average response time in milliseconds.
    #[serde(default)]
    pub avg_response_ms: Option<f64>,
    /// Arbitrary key-value metrics.
    #[serde(default)]
    pub custom: HashMap<String, serde_json::Value>,
}

/// A full dashboard sync payload submitted by an external consumer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardSyncRequest {
    /// Source project name (e.g., "agnostic").
    pub source: String,
    /// Agent statuses.
    pub agents: Vec<AgentStatus>,
    /// Session info.
    #[serde(default)]
    pub session: Option<SessionInfo>,
    /// Aggregate metrics.
    #[serde(default)]
    pub metrics: Option<DashboardMetrics>,
    /// Arbitrary metadata.
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

/// Stored dashboard snapshot with server-assigned fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredDashboardSnapshot {
    /// Server-assigned snapshot ID.
    pub snapshot_id: String,
    /// When this snapshot was received.
    pub received_at: DateTime<Utc>,
    /// The sync payload.
    #[serde(flatten)]
    pub payload: DashboardSyncRequest,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// POST /v1/dashboard/sync — accept a dashboard sync snapshot.
pub async fn dashboard_sync_handler(
    State(state): State<ApiState>,
    Json(req): Json<DashboardSyncRequest>,
) -> impl IntoResponse {
    // Validate source
    if req.source.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Source must not be empty", "code": 400})),
        )
            .into_response();
    }

    // Validate agents list is not empty
    if req.agents.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Agents list must not be empty", "code": 400})),
        )
            .into_response();
    }

    let snapshot_id = uuid::Uuid::new_v4().to_string();
    let agent_count = req.agents.len();

    info!(
        "Dashboard sync received: source={} agents={} snapshot_id={}",
        req.source, agent_count, snapshot_id
    );

    let stored = StoredDashboardSnapshot {
        snapshot_id: snapshot_id.clone(),
        received_at: Utc::now(),
        payload: req,
    };

    let mut snapshots = state.dashboard_snapshots.write().await;
    if snapshots.len() >= MAX_DASHBOARD_SNAPSHOTS {
        snapshots.pop_front();
    }
    snapshots.push_back(stored);

    (
        StatusCode::CREATED,
        Json(serde_json::json!({
            "status": "accepted",
            "snapshot_id": snapshot_id,
            "agents_synced": agent_count,
            "total_snapshots": snapshots.len()
        })),
    )
        .into_response()
}

/// GET /v1/dashboard/latest — get the most recent dashboard snapshot.
pub async fn dashboard_latest_handler(
    State(state): State<ApiState>,
) -> impl IntoResponse {
    let snapshots = state.dashboard_snapshots.read().await;
    match snapshots.back() {
        Some(latest) => (StatusCode::OK, Json(serde_json::to_value(latest).unwrap())).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "No dashboard snapshots available", "code": 404})),
        )
            .into_response(),
    }
}
