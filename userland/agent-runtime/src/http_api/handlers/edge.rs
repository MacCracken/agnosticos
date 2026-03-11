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
        Some(node) => (StatusCode::OK, Json(serde_json::json!(EdgeNodeResponse::from_node(node))))
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
    match fleet.heartbeat(&id, req.active_tasks, req.tasks_completed) {
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
}

/// `GET /v1/edge/dashboard`
///
/// Return an aggregated dashboard summary of the edge fleet including node
/// counts by status, GPU availability, average memory, and a fleet health
/// score (ratio of online nodes to total non-decommissioned nodes).
pub async fn edge_dashboard_handler(
    State(state): State<ApiState>,
) -> impl IntoResponse {
    let fleet = state.edge_fleet.read().await;
    let stats = fleet.stats();
    let all_nodes = fleet.list_nodes(None);

    let total_gpu_nodes = all_nodes
        .iter()
        .filter(|n| n.capabilities.has_gpu)
        .count() as u32;

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
            .route(
                "/v1/edge/nodes/:id/heartbeat",
                post(edge_heartbeat_handler),
            )
            .route(
                "/v1/edge/nodes/:id/decommission",
                post(edge_decommission_handler),
            )
            .route("/v1/edge/stats", get(edge_stats_handler))
            .route(
                "/v1/edge/nodes/:id/update",
                post(edge_start_update_handler),
            )
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
    async fn response_json(
        resp: axum::http::Response<Body>,
    ) -> serde_json::Value {
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
                    .uri(&format!("/v1/edge/nodes/{}", id))
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
                    .uri(&format!("/v1/edge/nodes/{}/heartbeat", id))
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
                    .uri(&format!("/v1/edge/nodes/{}/decommission", id))
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
                    .uri(&format!("/v1/edge/nodes/{}/decommission", id))
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
                    .uri(&format!("/v1/edge/nodes/{}/decommission", id))
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
                    .uri(&format!("/v1/edge/nodes/{}/update", id))
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
                    .uri(&format!("/v1/edge/nodes/{}/update", id))
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
                    .uri(&format!("/v1/edge/nodes/{}/update", id))
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
                    .uri(&format!("/v1/edge/nodes/{}/update/complete", id))
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
                    .uri(&format!("/v1/edge/nodes/{}/update/complete", id))
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
                    .uri(&format!("/v1/edge/nodes/{}/update/complete", id))
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
}
