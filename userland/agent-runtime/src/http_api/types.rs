use std::collections::HashMap;
use std::fmt;

use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Error response helpers — eliminates repeated json!({"error":..., "code":...})
// ---------------------------------------------------------------------------

/// Build a standard error response with the given HTTP status code and message.
pub fn error_response(status: StatusCode, msg: impl fmt::Display) -> impl IntoResponse {
    (
        status,
        Json(serde_json::json!({
            "error": msg.to_string(),
            "code": status.as_u16(),
        })),
    )
        .into_response()
}

/// 400 Bad Request convenience helper.
pub fn bad_request(msg: impl fmt::Display) -> impl IntoResponse {
    error_response(StatusCode::BAD_REQUEST, msg)
}

/// 404 Not Found convenience helper.
pub fn not_found(msg: impl fmt::Display) -> impl IntoResponse {
    error_response(StatusCode::NOT_FOUND, msg)
}

/// 409 Conflict convenience helper.
pub fn conflict(msg: impl fmt::Display) -> impl IntoResponse {
    error_response(StatusCode::CONFLICT, msg)
}

/// 500 Internal Server Error convenience helper.
pub fn internal_error(msg: impl fmt::Display) -> impl IntoResponse {
    error_response(StatusCode::INTERNAL_SERVER_ERROR, msg)
}

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterAgentRequest {
    pub name: String,
    /// Optional client-specified UUID. If provided and not already taken, it will
    /// be used as the agent's ID; otherwise a new UUID is generated server-side.
    #[serde(default)]
    pub id: Option<Uuid>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub resource_needs: ResourceNeeds,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

/// Request to deregister multiple agents in a single call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchDeregisterRequest {
    /// Deregister by source identifier (matches metadata "source" field).
    #[serde(default)]
    pub source: Option<String>,
    /// Deregister by explicit list of UUIDs.
    #[serde(default)]
    pub ids: Option<Vec<Uuid>>,
}

/// Result of a single agent deregistration within a batch.
#[derive(Debug, Serialize)]
pub struct BatchDeregisterResult {
    pub id: Uuid,
    pub name: String,
    pub status: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResourceNeeds {
    #[serde(default)]
    pub min_memory_mb: u64,
    #[serde(default)]
    pub min_cpu_shares: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterAgentResponse {
    pub id: Uuid,
    pub name: String,
    pub status: String,
    pub registered_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatRequest {
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub current_task: Option<String>,
    #[serde(default)]
    pub cpu_percent: Option<f32>,
    #[serde(default)]
    pub memory_mb: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDetail {
    pub id: Uuid,
    pub name: String,
    pub status: String,
    pub capabilities: Vec<String>,
    pub resource_needs: ResourceNeeds,
    pub metadata: HashMap<String, String>,
    pub registered_at: DateTime<Utc>,
    pub last_heartbeat: Option<DateTime<Utc>>,
    pub current_task: Option<String>,
    pub cpu_percent: Option<f32>,
    pub memory_mb: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentListResponse {
    pub agents: Vec<AgentDetail>,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub service: String,
    pub version: String,
    pub agents_registered: usize,
    pub uptime_seconds: u64,
    #[serde(default)]
    pub components: HashMap<String, ComponentHealth>,
    #[serde(default)]
    pub system: Option<SystemHealth>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentHealth {
    pub status: String,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemHealth {
    pub hostname: String,
    pub load_average: [f64; 3],
    pub memory_total_mb: u64,
    pub memory_available_mb: u64,
    pub disk_free_mb: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
    pub code: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagIngestRequest {
    pub text: String,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagQueryRequest {
    pub query: String,
    #[serde(default = "default_top_k")]
    pub top_k: usize,
}

pub(crate) fn default_top_k() -> usize {
    5
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeSearchRequest {
    pub query: String,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

pub(crate) fn default_limit() -> usize {
    10
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeIndexRequest {
    pub path: String,
    #[serde(default)]
    pub source: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMetricsResponse {
    pub total_agents: usize,
    pub agents_by_status: HashMap<String, usize>,
    pub uptime_seconds: u64,
    pub avg_cpu_percent: Option<f32>,
    pub total_memory_mb: u64,
}
