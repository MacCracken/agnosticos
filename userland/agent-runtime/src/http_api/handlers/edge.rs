//! HTTP handlers for edge fleet management API endpoints.
//!
//! Provides REST endpoints for managing AGNOS edge nodes: registration,
//! heartbeat, decommissioning, OTA updates, fleet statistics, and
//! capability-based task routing.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::edge::{EdgeCapabilities, EdgeFleetError, EdgeNodeStatus};
use crate::http_api::state::ApiState;

// ---------------------------------------------------------------------------
// Request / response types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct RegisterEdgeNodeRequest {
    /// Human-readable name for the edge node.
    pub name: String,
    /// Hardware and software capabilities of the node.
    #[serde(default)]
    pub capabilities: EdgeCapabilities,
    /// Agent binary running on the node (e.g. "secureyeoman-edge").
    pub agent_binary: String,
    /// Agent binary version.
    pub agent_version: String,
    /// AGNOS version running on the node.
    pub os_version: String,
    /// URL of the parent instance this node reports to.
    pub parent_url: String,
}

#[derive(Debug, Deserialize)]
pub struct HeartbeatRequest {
    /// Number of tasks currently running on this node.
    #[serde(default)]
    pub active_tasks: u32,
    /// Total tasks completed since registration.
    #[serde(default)]
    pub tasks_completed: u64,
    /// GPU utilization percentage (0.0–100.0), if GPU present.
    #[serde(default)]
    pub gpu_utilization_pct: Option<f32>,
    /// GPU memory used in MB, if GPU present.
    #[serde(default)]
    pub gpu_memory_used_mb: Option<u64>,
    /// GPU temperature in Celsius, if available.
    #[serde(default)]
    pub gpu_temperature_c: Option<f32>,
    /// Models currently loaded on the node (G3.2 — used for capability routing).
    #[serde(default)]
    pub loaded_models: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct StartUpdateRequest {
    /// Target version for the OTA update.
    pub version: String,
}

#[derive(Debug, Deserialize)]
pub struct CompleteUpdateRequest {
    /// Version the node is now running after the update.
    pub new_version: String,
}

#[derive(Debug, Deserialize)]
pub struct RouteTaskRequest {
    /// Tags the target node must have (e.g. ["camera", "bluetooth"]).
    #[serde(default)]
    pub required_tags: Vec<String>,
    /// Whether the target node must have a GPU.
    #[serde(default)]
    pub require_gpu: bool,
    /// Preferred geographic location label.
    pub preferred_location: Option<String>,
    /// Minimum GPU VRAM required in MB (G3.1).
    #[serde(default)]
    pub min_gpu_memory_mb: Option<u64>,
    /// Required CUDA compute capability string, e.g. "8.6" (G3.1).
    #[serde(default)]
    pub required_compute_capability: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct EdgeNodeListQuery {
    /// Optional status filter (online, suspect, offline, updating, decommissioned).
    pub status: Option<String>,
}

/// Serializable node summary returned by list and get endpoints.
#[derive(Debug, Serialize)]
struct EdgeNodeResponse {
    id: String,
    name: String,
    status: String,
    capabilities: EdgeCapabilities,
    agent_binary: String,
    agent_version: String,
    os_version: String,
    parent_url: String,
    last_heartbeat: String,
    registered_at: String,
    active_tasks: u32,
    tasks_completed: u64,
    tpm_attested: bool,
}

impl EdgeNodeResponse {
    fn from_node(node: &crate::edge::EdgeNode) -> Self {
        Self {
            id: node.id.clone(),
            name: node.name.clone(),
            status: node.status.to_string(),
            capabilities: node.capabilities.clone(),
            agent_binary: node.agent_binary.clone(),
            agent_version: node.agent_version.clone(),
            os_version: node.os_version.clone(),
            parent_url: node.parent_url.clone(),
            last_heartbeat: node.last_heartbeat.to_rfc3339(),
            registered_at: node.registered_at.to_rfc3339(),
            active_tasks: node.active_tasks,
            tasks_completed: node.tasks_completed,
            tpm_attested: node.tpm_attested,
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parse a status string into an `EdgeNodeStatus`.
fn parse_status_filter(s: &str) -> Option<EdgeNodeStatus> {
    match s.to_lowercase().as_str() {
        "online" => Some(EdgeNodeStatus::Online),
        "suspect" => Some(EdgeNodeStatus::Suspect),
        "offline" => Some(EdgeNodeStatus::Offline),
        "updating" => Some(EdgeNodeStatus::Updating),
        "decommissioned" => Some(EdgeNodeStatus::Decommissioned),
        _ => None,
    }
}

/// Map an `EdgeFleetError` to an appropriate HTTP status code and JSON body.
fn fleet_error_response(err: EdgeFleetError) -> (StatusCode, Json<serde_json::Value>) {
    let (status, msg) = match &err {
        EdgeFleetError::NodeNotFound(_) => (StatusCode::NOT_FOUND, err.to_string()),
        EdgeFleetError::FleetFull { .. } => (StatusCode::CONFLICT, err.to_string()),
        EdgeFleetError::DuplicateName(_) => (StatusCode::CONFLICT, err.to_string()),
        EdgeFleetError::InvalidName(_) => (StatusCode::BAD_REQUEST, err.to_string()),
        EdgeFleetError::NodeDecommissioned(_) => (StatusCode::GONE, err.to_string()),
        EdgeFleetError::NodeNotOnline(_) => (StatusCode::CONFLICT, err.to_string()),
        EdgeFleetError::NodeBusy { .. } => (StatusCode::CONFLICT, err.to_string()),
        EdgeFleetError::NotUpdating(_) => (StatusCode::CONFLICT, err.to_string()),
        EdgeFleetError::InsufficientBandwidth { .. } => {
            (StatusCode::UNPROCESSABLE_ENTITY, err.to_string())
        }
        EdgeFleetError::InsufficientResources { .. } => {
            (StatusCode::UNPROCESSABLE_ENTITY, err.to_string())
        }
    };
    (status, Json(serde_json::json!({ "error": msg })))
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// `GET /v1/edge/nodes`
///
/// List edge nodes, optionally filtered by status query parameter.
pub async fn edge_list_nodes_handler(
    State(state): State<ApiState>,
    Query(params): Query<EdgeNodeListQuery>,
) -> impl IntoResponse {
    let fleet = state.edge_fleet.read().await;

    // Parse optional status filter.
    let status_filter = match &params.status {
        Some(s) => match parse_status_filter(s) {
            Some(status) => Some(status),
            None => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "error": format!("Invalid status filter: '{}'. Valid values: online, suspect, offline, updating, decommissioned", s),
                    })),
                )
                    .into_response();
            }
        },
        None => None,
    };

    let nodes: Vec<EdgeNodeResponse> = fleet
        .list_nodes(status_filter)
        .iter()
        .map(|n| EdgeNodeResponse::from_node(n))
        .collect();

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "nodes": nodes,
            "total": nodes.len(),
        })),
    )
        .into_response()
}

/// `POST /v1/edge/nodes`
///
/// Register a new edge node in the fleet.
pub async fn edge_register_node_handler(
    State(state): State<ApiState>,
    Json(req): Json<RegisterEdgeNodeRequest>,
) -> impl IntoResponse {
    // Validate required fields are non-empty
    for (field, value) in [
        ("name", &req.name),
        ("agent_binary", &req.agent_binary),
        ("agent_version", &req.agent_version),
        ("os_version", &req.os_version),
        ("parent_url", &req.parent_url),
    ] {
        if value.is_empty() {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": format!("{} cannot be empty", field) })),
            )
                .into_response();
        }
    }

    // Enforce maximum string lengths to prevent memory abuse
    const MAX_FIELD_LEN: usize = 255;
    const MAX_URL_LEN: usize = 2048;
    for (field, value, max_len) in [
        ("name", &req.name, MAX_FIELD_LEN),
        ("agent_binary", &req.agent_binary, MAX_FIELD_LEN),
        ("agent_version", &req.agent_version, MAX_FIELD_LEN),
        ("os_version", &req.os_version, MAX_FIELD_LEN),
        ("parent_url", &req.parent_url, MAX_URL_LEN),
    ] {
        if value.len() > max_len {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": format!("{} exceeds maximum length of {} characters", field, max_len)
                })),
            )
                .into_response();
        }
    }

    let mut fleet = state.edge_fleet.write().await;
    match fleet.register_node(
        req.name.clone(),
        req.capabilities,
        req.agent_binary,
        req.agent_version,
        req.os_version,
        req.parent_url,
    ) {
        Ok(id) => {
            info!(id = %id, name = %req.name, "Edge node registered via API");
            (
                StatusCode::CREATED,
                Json(serde_json::json!({
                    "id": id,
                    "name": req.name,
                    "status": "registered",
                })),
            )
                .into_response()
        }
        Err(e) => {
            warn!(name = %req.name, error = %e, "Edge node registration failed");
            fleet_error_response(e).into_response()
        }
    }
}

/// `GET /v1/edge/nodes/:id`
///
/// Get details for a single edge node.
pub async fn edge_get_node_handler(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let fleet = state.edge_fleet.read().await;
    match fleet.get_node(&id) {
        Some(node) => (
            StatusCode::OK,
            Json(serde_json::json!(EdgeNodeResponse::from_node(node))),
        )
            .into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Edge node not found" })),
        )
            .into_response(),
    }
}

/// `POST /v1/edge/nodes/:id/heartbeat`
///
/// Process a heartbeat from an edge node.
pub async fn edge_heartbeat_handler(
    State(state): State<ApiState>,
    Path(id): Path<String>,
    Json(req): Json<HeartbeatRequest>,
) -> impl IntoResponse {
    let mut fleet = state.edge_fleet.write().await;
    match fleet.heartbeat(
        &id,
        req.active_tasks,
        req.tasks_completed,
        req.gpu_utilization_pct,
        req.gpu_memory_used_mb,
        req.gpu_temperature_c,
        req.loaded_models,
    ) {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "ok",
                "node_id": id,
            })),
        )
            .into_response(),
        Err(e) => fleet_error_response(e).into_response(),
    }
}

/// `POST /v1/edge/nodes/:id/decommission`
///
/// Decommission an edge node (mark for removal from fleet).
pub async fn edge_decommission_handler(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let mut fleet = state.edge_fleet.write().await;
    match fleet.decommission(&id) {
        Ok(node) => {
            info!(id = %id, name = %node.name, "Edge node decommissioned via API");
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "status": "decommissioned",
                    "node_id": id,
                    "name": node.name,
                })),
            )
                .into_response()
        }
        Err(e) => {
            warn!(id = %id, error = %e, "Edge node decommission failed");
            fleet_error_response(e).into_response()
        }
    }
}

/// `GET /v1/edge/stats`
///
/// Return fleet-wide statistics.
pub async fn edge_stats_handler(State(state): State<ApiState>) -> impl IntoResponse {
    let fleet = state.edge_fleet.read().await;
    let stats = fleet.stats();
    Json(serde_json::json!(stats))
}

/// `POST /v1/edge/nodes/:id/update`
///
/// Start an OTA update on an edge node. The node must be online and idle.
pub async fn edge_start_update_handler(
    State(state): State<ApiState>,
    Path(id): Path<String>,
    Json(req): Json<StartUpdateRequest>,
) -> impl IntoResponse {
    if req.version.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Version cannot be empty" })),
        )
            .into_response();
    }

    let mut fleet = state.edge_fleet.write().await;
    match fleet.start_update(&id) {
        Ok(()) => {
            info!(id = %id, target_version = %req.version, "Edge node OTA update started via API");
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "status": "updating",
                    "node_id": id,
                    "target_version": req.version,
                })),
            )
                .into_response()
        }
        Err(e) => {
            warn!(id = %id, error = %e, "Edge node update start failed");
            fleet_error_response(e).into_response()
        }
    }
}

/// `POST /v1/edge/nodes/:id/update/complete`
///
/// Mark an OTA update as complete. The node returns to online status.
pub async fn edge_complete_update_handler(
    State(state): State<ApiState>,
    Path(id): Path<String>,
    Json(req): Json<CompleteUpdateRequest>,
) -> impl IntoResponse {
    if req.new_version.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "New version cannot be empty" })),
        )
            .into_response();
    }

    let mut fleet = state.edge_fleet.write().await;
    match fleet.complete_update(&id, req.new_version.clone()) {
        Ok(()) => {
            info!(id = %id, new_version = %req.new_version, "Edge node OTA update completed via API");
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "status": "online",
                    "node_id": id,
                    "new_version": req.new_version,
                })),
            )
                .into_response()
        }
        Err(e) => {
            warn!(id = %id, error = %e, "Edge node update complete failed");
            fleet_error_response(e).into_response()
        }
    }
}

/// `POST /v1/edge/route`
///
/// Route a task to the best available edge node based on capability
/// requirements (tags, GPU, location preference).
pub async fn edge_route_task_handler(
    State(state): State<ApiState>,
    Json(req): Json<RouteTaskRequest>,
) -> impl IntoResponse {
    let fleet = state.edge_fleet.read().await;
    let candidates = fleet.route_task(
        &req.required_tags,
        req.require_gpu,
        req.preferred_location.as_deref(),
        req.min_gpu_memory_mb,
        req.required_compute_capability.as_deref(),
    );

    let nodes: Vec<EdgeNodeResponse> = candidates
        .iter()
        .map(|n| EdgeNodeResponse::from_node(n))
        .collect();

    Json(serde_json::json!({
        "candidates": nodes,
        "total": nodes.len(),
        "query": {
            "required_tags": req.required_tags,
            "require_gpu": req.require_gpu,
            "preferred_location": req.preferred_location,
            "min_gpu_memory_mb": req.min_gpu_memory_mb,
            "required_compute_capability": req.required_compute_capability,
        },
    }))
}

// ---------------------------------------------------------------------------
// Dashboard + Capability routing (Phase 14E-4/5)
// ---------------------------------------------------------------------------

/// Summary response for the edge fleet dashboard.
#[derive(Debug, Serialize)]
pub struct EdgeDashboardSummary {
    pub total_nodes: u32,
    pub active_nodes: u32,
    pub offline_nodes: u32,
    pub updating_nodes: u32,
    pub total_gpu_nodes: u32,
    pub avg_memory_mb: f64,
    pub fleet_health_score: f64,
    /// Average GPU utilization across nodes reporting GPU metrics (0.0–100.0).
    pub avg_gpu_utilization_pct: f64,
    /// Total GPU memory used across fleet in MB.
    pub total_gpu_memory_used_mb: u64,
    /// Number of nodes actively reporting GPU telemetry.
    pub gpu_reporting_nodes: u32,
}

/// `GET /v1/edge/dashboard`
///
/// Return an aggregated dashboard summary of the edge fleet including node
/// counts by status, GPU availability, average memory, and a fleet health
/// score (ratio of online nodes to total non-decommissioned nodes).
pub async fn edge_dashboard_handler(State(state): State<ApiState>) -> impl IntoResponse {
    let fleet = state.edge_fleet.read().await;
    let stats = fleet.stats();
    let all_nodes = fleet.list_nodes(None);

    let total_gpu_nodes = all_nodes.iter().filter(|n| n.capabilities.has_gpu).count() as u32;

    // GPU telemetry aggregation
    let (gpu_util_sum, gpu_mem_sum, gpu_count) =
        all_nodes
            .iter()
            .fold((0.0_f64, 0_u64, 0_u32), |(util, mem, cnt), n| {
                if let Some(u) = n.gpu_utilization_pct {
                    (
                        util + u as f64,
                        mem + n.gpu_memory_used_mb.unwrap_or(0),
                        cnt + 1,
                    )
                } else {
                    (util, mem, cnt)
                }
            });
    let avg_gpu_utilization_pct = if gpu_count > 0 {
        gpu_util_sum / gpu_count as f64
    } else {
        0.0
    };

    let (total_mem, mem_count) = all_nodes.iter().fold((0u64, 0u64), |(sum, count), n| {
        (sum + n.capabilities.memory_mb, count + 1)
    });
    let avg_memory_mb = if mem_count > 0 {
        total_mem as f64 / mem_count as f64
    } else {
        0.0
    };

    // Health score: online / (total - decommissioned), or 1.0 if no active nodes.
    let active_total = stats.total_nodes.saturating_sub(stats.decommissioned);
    let fleet_health_score = if active_total > 0 {
        stats.online as f64 / active_total as f64
    } else {
        1.0
    };

    let summary = EdgeDashboardSummary {
        total_nodes: stats.total_nodes,
        active_nodes: stats.online,
        offline_nodes: stats.offline,
        updating_nodes: stats.updating,
        total_gpu_nodes,
        avg_memory_mb,
        fleet_health_score,
        avg_gpu_utilization_pct,
        total_gpu_memory_used_mb: gpu_mem_sum,
        gpu_reporting_nodes: gpu_count,
    };

    Json(serde_json::json!(summary))
}

/// Request body for capability-based node routing.
#[derive(Debug, Deserialize)]
pub struct CapabilityRouteRequest {
    /// Tags the target node must have.
    #[serde(default)]
    pub required_tags: Vec<String>,
    /// Whether the target node must have a GPU.
    #[serde(default)]
    pub require_gpu: bool,
    /// Minimum memory in MB the target node must have.
    pub min_memory_mb: Option<u64>,
    /// Minimum network bandwidth quality (0.0–1.0).
    pub min_bandwidth: Option<f64>,
    /// Preferred geographic location label.
    pub preferred_location: Option<String>,
}

/// `POST /v1/edge/capabilities/route`
///
/// Find edge nodes that match a set of capability requirements including
/// tags, GPU, memory, bandwidth, and location preference. Uses the
/// constrained routing method when bandwidth or memory filters are provided.
pub async fn edge_capability_route_handler(
    State(state): State<ApiState>,
    Json(req): Json<CapabilityRouteRequest>,
) -> impl IntoResponse {
    let fleet = state.edge_fleet.read().await;

    let min_bw = req.min_bandwidth.unwrap_or(0.0);
    let min_mem = req.min_memory_mb.unwrap_or(0);

    let candidates = if min_bw > 0.0 || min_mem > 0 {
        fleet.route_task_with_constraints(
            &req.required_tags,
            req.require_gpu,
            req.preferred_location.as_deref(),
            min_bw,
            min_mem,
        )
    } else {
        fleet.route_task(
            &req.required_tags,
            req.require_gpu,
            req.preferred_location.as_deref(),
            None,
            None,
        )
    };

    let nodes: Vec<EdgeNodeResponse> = candidates
        .iter()
        .map(|n| EdgeNodeResponse::from_node(n))
        .collect();

    Json(serde_json::json!({
        "matched_nodes": nodes,
        "total": nodes.len(),
        "query": {
            "required_tags": req.required_tags,
            "require_gpu": req.require_gpu,
            "min_memory_mb": req.min_memory_mb,
            "min_bandwidth": req.min_bandwidth,
            "preferred_location": req.preferred_location,
        },
    }))
}

// ---------------------------------------------------------------------------
// G3.2: Fleet model registry
// ---------------------------------------------------------------------------

/// `GET /v1/edge/models`
///
/// Return a deduplicated, sorted list of all model names currently loaded
/// across online edge nodes, for advertising to hoosh for local inference
/// routing.  Each entry in `nodes_with_model` maps node IDs to the models
/// they have loaded.
pub async fn edge_fleet_models_handler(State(state): State<ApiState>) -> impl IntoResponse {
    let fleet = state.edge_fleet.read().await;
    let all_models = fleet.fleet_loaded_models();
    let nodes_by_model = fleet.nodes_by_model();

    Json(serde_json::json!({
        "loaded_models": all_models,
        "total": all_models.len(),
        "nodes_by_model": nodes_by_model,
    }))
}

// ---------------------------------------------------------------------------
// #8: Fleet GPU inventory
// ---------------------------------------------------------------------------

/// Per-node GPU entry in the fleet GPU inventory response.
#[derive(Debug, Serialize)]
pub struct FleetGpuNode {
    /// Node identifier.
    pub node_id: String,
    /// Node name.
    pub node_name: String,
    /// Node status.
    pub node_status: String,
    /// Total GPU VRAM in MB as advertised in node capabilities.
    pub gpu_memory_mb: u64,
    /// CUDA compute capability, if known.
    pub gpu_compute_capability: Option<String>,
    /// Latest GPU utilization percentage from heartbeat, if reported.
    pub gpu_utilization_pct: Option<f32>,
    /// Latest GPU memory used in MB from heartbeat, if reported.
    pub gpu_memory_used_mb: Option<u64>,
    /// Latest GPU temperature in Celsius from heartbeat, if reported.
    pub gpu_temperature_c: Option<f32>,
}

/// `GET /v1/edge/gpu`
///
/// Aggregate GPU status across all edge fleet nodes into a fleet-wide GPU
/// inventory.  Only nodes that advertise `has_gpu = true` are included.
///
/// Response fields:
/// - `total_gpu_nodes`  — number of nodes with GPUs (all statuses).
/// - `online_gpu_nodes` — GPU nodes currently online.
/// - `total_vram_mb`    — sum of `gpu_memory_mb` across all GPU nodes.
/// - `vram_used_mb`     — sum of `gpu_memory_used_mb` for nodes reporting it.
/// - `avg_utilization_pct` — average GPU utilization across reporting nodes.
/// - `nodes`            — per-node GPU detail list.
pub async fn edge_fleet_gpu_handler(State(state): State<ApiState>) -> impl IntoResponse {
    let fleet = state.edge_fleet.read().await;
    let all_nodes = fleet.list_nodes(None);

    let gpu_nodes: Vec<_> = all_nodes
        .iter()
        .filter(|n| n.capabilities.has_gpu)
        .collect();

    let total_gpu_nodes = gpu_nodes.len() as u32;
    let online_gpu_nodes = gpu_nodes
        .iter()
        .filter(|n| matches!(n.status, crate::edge::EdgeNodeStatus::Online))
        .count() as u32;

    let total_vram_mb: u64 = gpu_nodes
        .iter()
        .map(|n| n.capabilities.gpu_memory_mb.unwrap_or(0))
        .sum();

    let vram_used_mb: u64 = gpu_nodes.iter().filter_map(|n| n.gpu_memory_used_mb).sum();

    let (util_sum, util_count) = gpu_nodes.iter().fold((0.0_f64, 0u32), |(sum, cnt), n| {
        if let Some(u) = n.gpu_utilization_pct {
            (sum + u as f64, cnt + 1)
        } else {
            (sum, cnt)
        }
    });
    let avg_utilization_pct = if util_count > 0 {
        util_sum / util_count as f64
    } else {
        0.0
    };

    let nodes: Vec<FleetGpuNode> = gpu_nodes
        .iter()
        .map(|n| FleetGpuNode {
            node_id: n.id.clone(),
            node_name: n.name.clone(),
            node_status: format!("{:?}", n.status).to_lowercase(),
            gpu_memory_mb: n.capabilities.gpu_memory_mb.unwrap_or(0),
            gpu_compute_capability: n.capabilities.gpu_compute_capability.clone(),
            gpu_utilization_pct: n.gpu_utilization_pct,
            gpu_memory_used_mb: n.gpu_memory_used_mb,
            gpu_temperature_c: n.gpu_temperature_c,
        })
        .collect();

    Json(serde_json::json!({
        "total_gpu_nodes": total_gpu_nodes,
        "online_gpu_nodes": online_gpu_nodes,
        "total_vram_mb": total_vram_mb,
        "vram_used_mb": vram_used_mb,
        "avg_utilization_pct": avg_utilization_pct,
        "nodes": nodes,
    }))
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edge::{EdgeFleetConfig, EdgeFleetManager};
    use axum::body::Body;
    use axum::http::Request;
    use axum::routing::{get, post};
    use axum::Router;
    use std::sync::Arc;
    use tokio::sync::RwLock;
    use tower::ServiceExt;

    /// Build a test router with all edge endpoints wired up.
    fn test_router() -> Router {
        let state = test_state();
        Router::new()
            .route("/v1/edge/nodes", get(edge_list_nodes_handler))
            .route("/v1/edge/nodes", post(edge_register_node_handler))
            .route("/v1/edge/nodes/:id", get(edge_get_node_handler))
            .route("/v1/edge/nodes/:id/heartbeat", post(edge_heartbeat_handler))
            .route(
                "/v1/edge/nodes/:id/decommission",
                post(edge_decommission_handler),
            )
            .route("/v1/edge/stats", get(edge_stats_handler))
            .route("/v1/edge/nodes/:id/update", post(edge_start_update_handler))
            .route(
                "/v1/edge/nodes/:id/update/complete",
                post(edge_complete_update_handler),
            )
            .route("/v1/edge/route", post(edge_route_task_handler))
            .with_state(state)
    }

    /// Build a minimal `ApiState` with an `edge_fleet` field.
    fn test_state() -> ApiState {
        let mut state = ApiState::new();
        state.edge_fleet = Arc::new(RwLock::new(EdgeFleetManager::new(
            EdgeFleetConfig::default(),
        )));
        state
    }

    /// Helper: register a node via the API and return its ID.
    async fn register_node(app: &Router, name: &str) -> String {
        let body = serde_json::json!({
            "name": name,
            "agent_binary": "secureyeoman-edge",
            "agent_version": "2026.3.11",
            "os_version": "2026.3.11",
            "parent_url": "http://parent:8090",
            "capabilities": {
                "arch": "aarch64",
                "cpu_cores": 4,
                "memory_mb": 2048,
                "disk_mb": 16384,
                "has_gpu": false,
                "network_quality": 0.9,
                "location": "office",
                "tags": ["camera", "bluetooth"]
            }
        });

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/edge/nodes")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::CREATED);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        json["id"].as_str().unwrap().to_string()
    }

    /// Helper: parse JSON response body.
    async fn response_json(resp: axum::http::Response<Body>) -> serde_json::Value {
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    // --- Registration ---

    #[tokio::test]
    async fn register_node_success() {
        let app = test_router();
        let id = register_node(&app, "rpi-kitchen").await;
        assert!(!id.is_empty());
    }

    #[tokio::test]
    async fn register_node_returns_201() {
        let app = test_router();
        let body = serde_json::json!({
            "name": "test-node",
            "agent_binary": "edge-agent",
            "agent_version": "1.0.0",
            "os_version": "2026.3.11",
            "parent_url": "http://parent:8090",
        });

        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/edge/nodes")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::CREATED);
        let json = response_json(resp).await;
        assert_eq!(json["status"], "registered");
        assert_eq!(json["name"], "test-node");
    }

    #[tokio::test]
    async fn register_node_empty_name_rejected() {
        let app = test_router();
        let body = serde_json::json!({
            "name": "",
            "agent_binary": "edge-agent",
            "agent_version": "1.0.0",
            "os_version": "2026.3.11",
            "parent_url": "http://parent:8090",
        });

        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/edge/nodes")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn register_node_empty_agent_binary_rejected() {
        let app = test_router();
        let body = serde_json::json!({
            "name": "test-node",
            "agent_binary": "",
            "agent_version": "1.0.0",
            "os_version": "2026.3.11",
            "parent_url": "http://parent:8090",
        });

        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/edge/nodes")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn register_node_empty_parent_url_rejected() {
        let app = test_router();
        let body = serde_json::json!({
            "name": "test-node",
            "agent_binary": "edge-agent",
            "agent_version": "1.0.0",
            "os_version": "2026.3.11",
            "parent_url": "",
        });

        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/edge/nodes")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn register_duplicate_name_rejected() {
        let app = test_router();
        let _ = register_node(&app, "dup-node").await;

        // Second registration with same name should fail.
        let body = serde_json::json!({
            "name": "dup-node",
            "agent_binary": "edge-agent",
            "agent_version": "1.0.0",
            "os_version": "2026.3.11",
            "parent_url": "http://parent:8090",
        });

        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/edge/nodes")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::CONFLICT);
    }

    // --- List nodes ---

    #[tokio::test]
    async fn list_nodes_empty_fleet() {
        let app = test_router();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/v1/edge/nodes")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let json = response_json(resp).await;
        assert_eq!(json["total"], 0);
        assert!(json["nodes"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn list_nodes_returns_registered() {
        let app = test_router();
        let _ = register_node(&app, "node-a").await;

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/edge/nodes")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let json = response_json(resp).await;
        assert_eq!(json["total"], 1);
    }

    #[tokio::test]
    async fn list_nodes_with_status_filter() {
        let app = test_router();
        let _ = register_node(&app, "node-a").await;

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/edge/nodes?status=online")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let json = response_json(resp).await;
        assert_eq!(json["total"], 1);
    }

    #[tokio::test]
    async fn list_nodes_invalid_status_filter() {
        let app = test_router();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/v1/edge/nodes?status=bogus")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let json = response_json(resp).await;
        assert!(json["error"].as_str().unwrap().contains("Invalid status"));
    }

    // --- Get node ---

    #[tokio::test]
    async fn get_node_success() {
        let app = test_router();
        let id = register_node(&app, "node-a").await;

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/v1/edge/nodes/{}", id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let json = response_json(resp).await;
        assert_eq!(json["id"], id);
        assert_eq!(json["name"], "node-a");
        assert_eq!(json["status"], "online");
    }

    #[tokio::test]
    async fn get_node_not_found() {
        let app = test_router();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/v1/edge/nodes/nonexistent")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    // --- Heartbeat ---

    #[tokio::test]
    async fn heartbeat_success() {
        let app = test_router();
        let id = register_node(&app, "node-a").await;

        let body = serde_json::json!({
            "active_tasks": 2,
            "tasks_completed": 50,
        });

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/v1/edge/nodes/{}/heartbeat", id))
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let json = response_json(resp).await;
        assert_eq!(json["status"], "ok");
        assert_eq!(json["node_id"], id);
    }

    #[tokio::test]
    async fn heartbeat_unknown_node() {
        let app = test_router();
        let body = serde_json::json!({
            "active_tasks": 0,
            "tasks_completed": 0,
        });

        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/edge/nodes/nonexistent/heartbeat")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    // --- Decommission ---

    #[tokio::test]
    async fn decommission_success() {
        let app = test_router();
        let id = register_node(&app, "node-a").await;

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/v1/edge/nodes/{}/decommission", id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let json = response_json(resp).await;
        assert_eq!(json["status"], "decommissioned");
    }

    #[tokio::test]
    async fn decommission_unknown_node() {
        let app = test_router();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/edge/nodes/nonexistent/decommission")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn decommission_already_decommissioned() {
        let app = test_router();
        let id = register_node(&app, "node-a").await;

        // First decommission.
        let _ = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/v1/edge/nodes/{}/decommission", id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Second decommission should fail.
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/v1/edge/nodes/{}/decommission", id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::GONE);
    }

    // --- Stats ---

    #[tokio::test]
    async fn stats_empty_fleet() {
        let app = test_router();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/v1/edge/stats")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let json = response_json(resp).await;
        assert_eq!(json["total_nodes"], 0);
        assert_eq!(json["online"], 0);
    }

    #[tokio::test]
    async fn stats_with_nodes() {
        let app = test_router();
        let _ = register_node(&app, "node-a").await;
        let _ = register_node(&app, "node-b").await;

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/edge/stats")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let json = response_json(resp).await;
        assert_eq!(json["total_nodes"], 2);
        assert_eq!(json["online"], 2);
    }

    // --- OTA Update ---

    #[tokio::test]
    async fn start_update_success() {
        let app = test_router();
        let id = register_node(&app, "node-a").await;

        let body = serde_json::json!({ "version": "2026.4.0" });
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/v1/edge/nodes/{}/update", id))
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let json = response_json(resp).await;
        assert_eq!(json["status"], "updating");
        assert_eq!(json["target_version"], "2026.4.0");
    }

    #[tokio::test]
    async fn start_update_empty_version_rejected() {
        let app = test_router();
        let id = register_node(&app, "node-a").await;

        let body = serde_json::json!({ "version": "" });
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/v1/edge/nodes/{}/update", id))
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn start_update_unknown_node() {
        let app = test_router();
        let body = serde_json::json!({ "version": "2026.4.0" });
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/edge/nodes/nonexistent/update")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn complete_update_success() {
        let app = test_router();
        let id = register_node(&app, "node-a").await;

        // Start update first.
        let start_body = serde_json::json!({ "version": "2026.4.0" });
        let _ = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/v1/edge/nodes/{}/update", id))
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&start_body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        // Complete update.
        let complete_body = serde_json::json!({ "new_version": "2026.4.0" });
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/v1/edge/nodes/{}/update/complete", id))
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&complete_body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let json = response_json(resp).await;
        assert_eq!(json["status"], "online");
        assert_eq!(json["new_version"], "2026.4.0");
    }

    #[tokio::test]
    async fn complete_update_empty_version_rejected() {
        let app = test_router();
        let id = register_node(&app, "node-a").await;

        let body = serde_json::json!({ "new_version": "" });
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/v1/edge/nodes/{}/update/complete", id))
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn complete_update_not_updating_rejected() {
        let app = test_router();
        let id = register_node(&app, "node-a").await;

        // Try to complete without starting.
        let body = serde_json::json!({ "new_version": "2026.4.0" });
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/v1/edge/nodes/{}/update/complete", id))
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::CONFLICT);
    }

    // --- Task routing ---

    #[tokio::test]
    async fn route_task_empty_fleet() {
        let app = test_router();
        let body = serde_json::json!({
            "required_tags": [],
            "require_gpu": false,
        });

        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/edge/route")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let json = response_json(resp).await;
        assert_eq!(json["total"], 0);
        assert!(json["candidates"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn route_task_returns_candidates() {
        let app = test_router();
        let _ = register_node(&app, "node-a").await;
        let _ = register_node(&app, "node-b").await;

        let body = serde_json::json!({
            "required_tags": [],
            "require_gpu": false,
        });

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/edge/route")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let json = response_json(resp).await;
        assert_eq!(json["total"], 2);
    }

    #[tokio::test]
    async fn route_task_with_tag_filter() {
        let app = test_router();
        // Default test node has tags: ["camera", "bluetooth"]
        let _ = register_node(&app, "has-tags").await;

        let body = serde_json::json!({
            "required_tags": ["camera"],
            "require_gpu": false,
        });

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/edge/route")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let json = response_json(resp).await;
        assert_eq!(json["total"], 1);
    }

    #[tokio::test]
    async fn route_task_with_gpu_requirement() {
        let app = test_router();
        // Default test node has has_gpu: false
        let _ = register_node(&app, "no-gpu").await;

        let body = serde_json::json!({
            "required_tags": [],
            "require_gpu": true,
        });

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/edge/route")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let json = response_json(resp).await;
        assert_eq!(json["total"], 0); // No GPU nodes registered
    }

    #[tokio::test]
    async fn route_task_includes_query_echo() {
        let app = test_router();
        let body = serde_json::json!({
            "required_tags": ["camera"],
            "require_gpu": true,
            "preferred_location": "office",
        });

        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/edge/route")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let json = response_json(resp).await;
        assert_eq!(json["query"]["require_gpu"], true);
        assert_eq!(json["query"]["preferred_location"], "office");
    }

    // --- Dashboard (Phase 14E-4) ---

    fn dashboard_router() -> Router {
        let state = test_state();
        Router::new()
            .route("/v1/edge/nodes", post(edge_register_node_handler))
            .route("/v1/edge/dashboard", get(edge_dashboard_handler))
            .route(
                "/v1/edge/capabilities/route",
                post(edge_capability_route_handler),
            )
            .with_state(state)
    }

    #[tokio::test]
    async fn dashboard_empty_fleet() {
        let app = dashboard_router();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/v1/edge/dashboard")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let json = response_json(resp).await;
        assert_eq!(json["total_nodes"], 0);
        assert_eq!(json["active_nodes"], 0);
        assert_eq!(json["fleet_health_score"], 1.0);
    }

    #[tokio::test]
    async fn dashboard_with_nodes() {
        let app = dashboard_router();
        let _ = register_node(&app, "dash-node-a").await;
        let _ = register_node(&app, "dash-node-b").await;

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/edge/dashboard")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let json = response_json(resp).await;
        assert_eq!(json["total_nodes"], 2);
        assert_eq!(json["active_nodes"], 2);
        assert_eq!(json["offline_nodes"], 0);
        assert_eq!(json["updating_nodes"], 0);
        assert_eq!(json["fleet_health_score"], 1.0);
        // Test nodes have memory_mb = 2048
        assert_eq!(json["avg_memory_mb"], 2048.0);
        // Test nodes have has_gpu = false
        assert_eq!(json["total_gpu_nodes"], 0);
    }

    // --- Capability routing (Phase 14E-5) ---

    #[tokio::test]
    async fn capability_route_empty_fleet() {
        let app = dashboard_router();
        let body = serde_json::json!({
            "required_tags": [],
            "require_gpu": false,
        });

        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/edge/capabilities/route")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let json = response_json(resp).await;
        assert_eq!(json["total"], 0);
    }

    #[tokio::test]
    async fn capability_route_with_memory_filter() {
        let app = dashboard_router();
        // Test nodes have memory_mb = 2048
        let _ = register_node(&app, "cap-node").await;

        // Should match: min_memory_mb <= 2048
        let body = serde_json::json!({
            "required_tags": [],
            "require_gpu": false,
            "min_memory_mb": 1024,
        });

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/edge/capabilities/route")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let json = response_json(resp).await;
        assert_eq!(json["total"], 1);

        // Should NOT match: min_memory_mb > 2048
        let body_high = serde_json::json!({
            "required_tags": [],
            "require_gpu": false,
            "min_memory_mb": 8192,
        });

        let resp2 = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/edge/capabilities/route")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&body_high).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        let json2 = response_json(resp2).await;
        assert_eq!(json2["total"], 0);
    }

    #[tokio::test]
    async fn capability_route_includes_query_echo() {
        let app = dashboard_router();
        let body = serde_json::json!({
            "required_tags": ["camera"],
            "require_gpu": true,
            "min_memory_mb": 4096,
            "min_bandwidth": 0.8,
            "preferred_location": "warehouse",
        });

        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/edge/capabilities/route")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let json = response_json(resp).await;
        assert_eq!(json["query"]["require_gpu"], true);
        assert_eq!(json["query"]["min_memory_mb"], 4096);
        assert_eq!(json["query"]["min_bandwidth"], 0.8);
        assert_eq!(json["query"]["preferred_location"], "warehouse");
    }

    #[tokio::test]
    async fn capability_route_with_bandwidth_filter() {
        let app = dashboard_router();
        // Test nodes have network_quality = 0.9
        let _ = register_node(&app, "bw-node").await;

        let body = serde_json::json!({
            "required_tags": [],
            "require_gpu": false,
            "min_bandwidth": 0.95,
        });

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/edge/capabilities/route")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let json = response_json(resp).await;
        // 0.9 < 0.95, so no match
        assert_eq!(json["total"], 0);
    }

    // --- G3.1: GPU capability routing via HTTP ---

    fn gpu_router() -> Router {
        let state = test_state();
        Router::new()
            .route("/v1/edge/nodes", post(edge_register_node_handler))
            .route("/v1/edge/nodes/:id/heartbeat", post(edge_heartbeat_handler))
            .route("/v1/edge/route", post(edge_route_task_handler))
            .route("/v1/edge/models", get(edge_fleet_models_handler))
            .with_state(state)
    }

    /// Register a GPU-capable node with given VRAM and compute capability.
    async fn register_gpu_node(
        app: &Router,
        name: &str,
        gpu_memory_mb: u64,
        compute_capability: &str,
    ) -> String {
        let body = serde_json::json!({
            "name": name,
            "agent_binary": "edge-gpu-agent",
            "agent_version": "2026.3.17",
            "os_version": "2026.3.17",
            "parent_url": "http://parent:8090",
            "capabilities": {
                "arch": "x86_64",
                "cpu_cores": 32,
                "memory_mb": 65536,
                "disk_mb": 1048576,
                "has_gpu": true,
                "gpu_memory_mb": gpu_memory_mb,
                "gpu_compute_capability": compute_capability,
                "network_quality": 0.99,
                "location": "dc-east",
                "tags": ["cuda"]
            }
        });

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/edge/nodes")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::CREATED);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        json["id"].as_str().unwrap().to_string()
    }

    #[tokio::test]
    async fn route_task_gpu_vram_filter_via_http() {
        let app = gpu_router();
        let _ = register_gpu_node(&app, "h100", 81920, "9.0").await;
        let _ = register_gpu_node(&app, "a10g", 24576, "8.6").await;

        // Request nodes with at least 40 GB VRAM — only h100 qualifies.
        let body = serde_json::json!({
            "required_tags": [],
            "require_gpu": true,
            "min_gpu_memory_mb": 40960,
        });

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/edge/route")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let json = response_json(resp).await;
        assert_eq!(json["total"], 1);
        assert_eq!(json["candidates"][0]["name"], "h100");
    }

    #[tokio::test]
    async fn route_task_compute_capability_filter_via_http() {
        let app = gpu_router();
        let _ = register_gpu_node(&app, "hopper", 80000, "9.0").await;
        let _ = register_gpu_node(&app, "ampere", 10240, "8.6").await;

        // Request only Hopper (9.0) nodes.
        let body = serde_json::json!({
            "required_tags": [],
            "require_gpu": true,
            "required_compute_capability": "9.0",
        });

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/edge/route")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let json = response_json(resp).await;
        assert_eq!(json["total"], 1);
        assert_eq!(json["candidates"][0]["name"], "hopper");
    }

    #[tokio::test]
    async fn route_task_gpu_query_echo_includes_new_fields() {
        let app = gpu_router();
        let body = serde_json::json!({
            "required_tags": [],
            "require_gpu": true,
            "min_gpu_memory_mb": 16384_u64,
            "required_compute_capability": "8.9",
        });

        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/edge/route")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let json = response_json(resp).await;
        assert_eq!(json["query"]["min_gpu_memory_mb"], 16384);
        assert_eq!(json["query"]["required_compute_capability"], "8.9");
    }

    // --- G3.2: Fleet model registry via HTTP ---

    #[tokio::test]
    async fn fleet_models_empty_fleet() {
        let app = gpu_router();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/v1/edge/models")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let json = response_json(resp).await;
        assert_eq!(json["total"], 0);
        assert!(json["loaded_models"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn fleet_models_after_heartbeat_with_models() {
        let app = gpu_router();
        let id = register_node(&app, "model-carrier").await;

        // Send heartbeat with models.
        let hb_body = serde_json::json!({
            "active_tasks": 1,
            "tasks_completed": 10,
            "loaded_models": ["llama3.2:3b", "mistral:7b"],
        });

        let _ = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/v1/edge/nodes/{}/heartbeat", id))
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&hb_body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/edge/models")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let json = response_json(resp).await;
        assert_eq!(json["total"], 2);
        let models = json["loaded_models"].as_array().unwrap();
        assert!(models.iter().any(|m| m == "llama3.2:3b"));
        assert!(models.iter().any(|m| m == "mistral:7b"));
        // nodes_by_model should list the node.
        assert!(json["nodes_by_model"][&id].is_array());
    }

    #[tokio::test]
    async fn fleet_models_deduplicates_across_nodes() {
        let app = gpu_router();
        let id_a = register_node(&app, "node-models-a").await;
        let id_b = register_node(&app, "node-models-b").await;

        for (id, models) in [
            (&id_a, vec!["llama3.2:3b", "phi3:mini"]),
            (&id_b, vec!["llama3.2:3b", "gemma2:9b"]),
        ] {
            let hb_body = serde_json::json!({
                "active_tasks": 0,
                "tasks_completed": 0,
                "loaded_models": models,
            });
            let _ = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri(format!("/v1/edge/nodes/{}/heartbeat", id))
                        .header("content-type", "application/json")
                        .body(Body::from(serde_json::to_string(&hb_body).unwrap()))
                        .unwrap(),
                )
                .await
                .unwrap();
        }

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/edge/models")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let json = response_json(resp).await;
        // llama3.2:3b appears on both nodes but should only be listed once.
        assert_eq!(json["total"], 3);
        let models: Vec<String> = json["loaded_models"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        assert_eq!(models, vec!["gemma2:9b", "llama3.2:3b", "phi3:mini"]);
    }

    #[tokio::test]
    async fn heartbeat_with_loaded_models_is_accepted() {
        let app = gpu_router();
        let id = register_node(&app, "hb-models-node").await;

        let body = serde_json::json!({
            "active_tasks": 0,
            "tasks_completed": 0,
            "loaded_models": ["deepseek-r1:7b"],
        });

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/v1/edge/nodes/{}/heartbeat", id))
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let json = response_json(resp).await;
        assert_eq!(json["status"], "ok");
    }
}
