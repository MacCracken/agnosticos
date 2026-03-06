//! Agent Registration HTTP API
//!
//! Axum HTTP server on port 8090 providing REST endpoints for external
//! consumers (AGNOSTIC, SecureYeoman) to register agents, send heartbeats,
//! and query agent status.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, info};
use uuid::Uuid;

/// Default listen port for the agent registration API.
pub const DEFAULT_PORT: u16 = 8090;

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterAgentRequest {
    pub name: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub resource_needs: ResourceNeeds,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
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

// ---------------------------------------------------------------------------
// Shared state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct RegisteredAgentEntry {
    pub detail: AgentDetail,
}

#[derive(Debug, Clone)]
pub struct ApiState {
    agents: Arc<RwLock<HashMap<Uuid, RegisteredAgentEntry>>>,
    started_at: DateTime<Utc>,
}

impl ApiState {
    pub fn new() -> Self {
        Self {
            agents: Arc::new(RwLock::new(HashMap::new())),
            started_at: Utc::now(),
        }
    }
}

impl Default for ApiState {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

async fn health_handler(State(state): State<ApiState>) -> impl IntoResponse {
    let agents = state.agents.read().await;
    let uptime = (Utc::now() - state.started_at).num_seconds().max(0) as u64;

    let mut components = HashMap::new();

    // Check LLM Gateway reachability
    let llm_status = check_llm_gateway().await;
    components.insert("llm_gateway".to_string(), llm_status);

    // Agent runtime status
    components.insert(
        "agent_registry".to_string(),
        ComponentHealth {
            status: "ok".to_string(),
            message: Some(format!("{} agents registered", agents.len())),
        },
    );

    // System health
    let system = gather_system_health();

    let overall_status = if components.values().all(|c| c.status == "ok") {
        "ok"
    } else if components.values().any(|c| c.status == "error") {
        "degraded"
    } else {
        "ok"
    };

    Json(HealthResponse {
        status: overall_status.to_string(),
        service: "agnos-agent-runtime".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        agents_registered: agents.len(),
        uptime_seconds: uptime,
        components,
        system: Some(system),
    })
}

async fn check_llm_gateway() -> ComponentHealth {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());

    let gateway_url = std::env::var("AGNOS_GATEWAY_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:8088".to_string());

    match client
        .get(format!("{}/v1/health", gateway_url))
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => ComponentHealth {
            status: "ok".to_string(),
            message: Some("LLM Gateway reachable".to_string()),
        },
        Ok(resp) => ComponentHealth {
            status: "degraded".to_string(),
            message: Some(format!("LLM Gateway returned {}", resp.status())),
        },
        Err(_) => ComponentHealth {
            status: "unreachable".to_string(),
            message: Some("LLM Gateway not responding".to_string()),
        },
    }
}

fn gather_system_health() -> SystemHealth {
    let hostname = hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    // Read /proc/loadavg
    let load_average = std::fs::read_to_string("/proc/loadavg")
        .ok()
        .and_then(|s| {
            let parts: Vec<f64> = s
                .split_whitespace()
                .take(3)
                .filter_map(|p| p.parse().ok())
                .collect();
            if parts.len() == 3 {
                Some([parts[0], parts[1], parts[2]])
            } else {
                None
            }
        })
        .unwrap_or([0.0, 0.0, 0.0]);

    // Read /proc/meminfo
    let (mem_total, mem_available) = std::fs::read_to_string("/proc/meminfo")
        .ok()
        .map(|s| {
            let mut total = 0u64;
            let mut avail = 0u64;
            for line in s.lines() {
                if line.starts_with("MemTotal:") {
                    total = line
                        .split_whitespace()
                        .nth(1)
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(0);
                }
                if line.starts_with("MemAvailable:") {
                    avail = line
                        .split_whitespace()
                        .nth(1)
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(0);
                }
            }
            (total / 1024, avail / 1024) // kB to MB
        })
        .unwrap_or((0, 0));

    // Disk free (/)
    let disk_free = std::process::Command::new("df")
        .args(["--output=avail", "-BM", "/"])
        .output()
        .ok()
        .and_then(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .nth(1)
                .and_then(|l| l.trim().trim_end_matches('M').parse::<u64>().ok())
        })
        .unwrap_or(0);

    SystemHealth {
        hostname,
        load_average,
        memory_total_mb: mem_total,
        memory_available_mb: mem_available,
        disk_free_mb: disk_free,
    }
}

async fn register_agent_handler(
    State(state): State<ApiState>,
    Json(req): Json<RegisterAgentRequest>,
) -> impl IntoResponse {
    if req.name.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Agent name is required", "code": 400})),
        )
            .into_response();
    }

    if req.name.len() > 256 {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Agent name too long (max 256)", "code": 400})),
        )
            .into_response();
    }

    let mut agents = state.agents.write().await;

    // Check for duplicate names
    if agents.values().any(|a| a.detail.name == req.name) {
        return (
            StatusCode::CONFLICT,
            Json(serde_json::json!({"error": format!("Agent '{}' already registered", req.name), "code": 409})),
        )
            .into_response();
    }

    let id = Uuid::new_v4();
    let now = Utc::now();

    let detail = AgentDetail {
        id,
        name: req.name.clone(),
        status: "registered".to_string(),
        capabilities: req.capabilities,
        resource_needs: req.resource_needs,
        metadata: req.metadata,
        registered_at: now,
        last_heartbeat: None,
        current_task: None,
        cpu_percent: None,
        memory_mb: None,
    };

    agents.insert(id, RegisteredAgentEntry {
        detail: detail.clone(),
    });

    info!("Agent registered: {} ({})", req.name, id);

    let resp = RegisterAgentResponse {
        id,
        name: req.name,
        status: "registered".to_string(),
        registered_at: now,
    };

    match serde_json::to_value(resp) {
        Ok(val) => (StatusCode::CREATED, Json(val)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Serialization error: {}", e), "code": 500})),
        ).into_response(),
    }
}

async fn heartbeat_handler(
    State(state): State<ApiState>,
    Path(id): Path<Uuid>,
    Json(req): Json<HeartbeatRequest>,
) -> impl IntoResponse {
    let mut agents = state.agents.write().await;

    match agents.get_mut(&id) {
        Some(entry) => {
            entry.detail.last_heartbeat = Some(Utc::now());
            if let Some(status) = req.status {
                entry.detail.status = status;
            }
            if let Some(task) = req.current_task {
                entry.detail.current_task = Some(task);
            }
            if let Some(cpu) = req.cpu_percent {
                entry.detail.cpu_percent = Some(cpu);
            }
            if let Some(mem) = req.memory_mb {
                entry.detail.memory_mb = Some(mem);
            }

            debug!("Heartbeat received from agent {}", id);
            (StatusCode::OK, Json(serde_json::json!({"status": "ok"}))).into_response()
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": format!("Agent {} not found", id), "code": 404})),
        )
            .into_response(),
    }
}

async fn list_agents_handler(State(state): State<ApiState>) -> impl IntoResponse {
    let agents = state.agents.read().await;
    let agent_list: Vec<AgentDetail> = agents.values().map(|a| a.detail.clone()).collect();
    let total = agent_list.len();

    Json(AgentListResponse {
        agents: agent_list,
        total,
    })
}

async fn get_agent_handler(
    State(state): State<ApiState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let agents = state.agents.read().await;

    match agents.get(&id) {
        Some(entry) => match serde_json::to_value(&entry.detail) {
            Ok(val) => (StatusCode::OK, Json(val)).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("Serialization error: {}", e), "code": 500})),
            ).into_response(),
        },
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": format!("Agent {} not found", id), "code": 404})),
        )
            .into_response(),
    }
}

async fn deregister_agent_handler(
    State(state): State<ApiState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let mut agents = state.agents.write().await;

    match agents.remove(&id) {
        Some(entry) => {
            info!("Agent deregistered: {} ({})", entry.detail.name, id);
            (StatusCode::OK, Json(serde_json::json!({"status": "deregistered", "id": id.to_string()}))).into_response()
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": format!("Agent {} not found", id), "code": 404})),
        )
            .into_response(),
    }
}

// ---------------------------------------------------------------------------
// Metrics
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMetricsResponse {
    pub total_agents: usize,
    pub agents_by_status: HashMap<String, usize>,
    pub uptime_seconds: u64,
    pub avg_cpu_percent: Option<f32>,
    pub total_memory_mb: u64,
}

async fn metrics_handler(State(state): State<ApiState>) -> impl IntoResponse {
    let agents = state.agents.read().await;
    let uptime = (Utc::now() - state.started_at).num_seconds().max(0) as u64;

    let mut by_status: HashMap<String, usize> = HashMap::new();
    let mut total_cpu: f32 = 0.0;
    let mut cpu_count: usize = 0;
    let mut total_mem: u64 = 0;

    for entry in agents.values() {
        *by_status.entry(entry.detail.status.clone()).or_default() += 1;
        if let Some(cpu) = entry.detail.cpu_percent {
            total_cpu += cpu;
            cpu_count += 1;
        }
        if let Some(mem) = entry.detail.memory_mb {
            total_mem += mem;
        }
    }

    let avg_cpu = if cpu_count > 0 {
        Some(total_cpu / cpu_count as f32)
    } else {
        None
    };

    Json(AgentMetricsResponse {
        total_agents: agents.len(),
        agents_by_status: by_status,
        uptime_seconds: uptime,
        avg_cpu_percent: avg_cpu,
        total_memory_mb: total_mem,
    })
}

// ---------------------------------------------------------------------------
// Router & server
// ---------------------------------------------------------------------------

/// Build the Axum router for the agent registration API.
pub fn build_router(state: ApiState) -> Router {
    Router::new()
        .route("/v1/health", get(health_handler))
        .route("/v1/metrics", get(metrics_handler))
        .route("/v1/agents/register", post(register_agent_handler))
        .route("/v1/agents/:id/heartbeat", post(heartbeat_handler))
        .route("/v1/agents", get(list_agents_handler))
        .route("/v1/agents/:id", get(get_agent_handler))
        .route("/v1/agents/:id", delete(deregister_agent_handler))
        .with_state(state)
}

/// Start the HTTP API server on the given port.
pub async fn start_server(port: u16) -> anyhow::Result<()> {
    let state = ApiState::new();
    let app = build_router(state);

    let bind_addr: std::net::IpAddr = std::env::var("AGNOS_RUNTIME_BIND")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST));
    let addr = SocketAddr::new(bind_addr, port);
    info!("Agent Registration API listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    fn test_state() -> ApiState {
        ApiState::new()
    }

    fn test_app() -> Router {
        build_router(test_state())
    }

    #[tokio::test]
    async fn test_health() {
        let app = test_app();
        let req = Request::builder()
            .uri("/v1/health")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: HealthResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(json.service, "agnos-agent-runtime");
        // Components should exist
        assert!(json.components.contains_key("agent_registry"));
        assert!(json.components.contains_key("llm_gateway"));
        // System health should be populated
        assert!(json.system.is_some());
    }

    #[tokio::test]
    async fn test_register_agent() {
        let app = test_app();
        let req_body = serde_json::json!({
            "name": "test-agent",
            "capabilities": ["file:read", "llm:inference"],
            "resource_needs": {"min_memory_mb": 512, "min_cpu_shares": 100}
        });

        let req = Request::builder()
            .method("POST")
            .uri("/v1/agents/register")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["name"], "test-agent");
        assert_eq!(json["status"], "registered");
        assert!(json["id"].as_str().is_some());
    }

    #[tokio::test]
    async fn test_register_empty_name() {
        let app = test_app();
        let req_body = serde_json::json!({"name": ""});

        let req = Request::builder()
            .method("POST")
            .uri("/v1/agents/register")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_register_duplicate_name() {
        let state = test_state();
        let app = build_router(state.clone());

        // Register first
        let req_body = serde_json::json!({"name": "dup-agent"});
        let req = Request::builder()
            .method("POST")
            .uri("/v1/agents/register")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);

        // Duplicate
        let req = Request::builder()
            .method("POST")
            .uri("/v1/agents/register")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn test_list_agents() {
        let state = test_state();
        let app = build_router(state.clone());

        // Register two agents
        for name in ["agent-a", "agent-b"] {
            let req = Request::builder()
                .method("POST")
                .uri("/v1/agents/register")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({"name": name})).unwrap(),
                ))
                .unwrap();
            app.clone().oneshot(req).await.unwrap();
        }

        // List
        let req = Request::builder()
            .uri("/v1/agents")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: AgentListResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(json.total, 2);
    }

    #[tokio::test]
    async fn test_get_agent() {
        let state = test_state();
        let app = build_router(state.clone());

        // Register
        let req = Request::builder()
            .method("POST")
            .uri("/v1/agents/register")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_vec(&serde_json::json!({"name": "get-me"})).unwrap(),
            ))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let reg: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let id = reg["id"].as_str().unwrap();

        // Get
        let req = Request::builder()
            .uri(format!("/v1/agents/{}", id))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_get_agent_not_found() {
        let app = test_app();
        let req = Request::builder()
            .uri(format!("/v1/agents/{}", Uuid::new_v4()))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_heartbeat() {
        let state = test_state();
        let app = build_router(state.clone());

        // Register
        let req = Request::builder()
            .method("POST")
            .uri("/v1/agents/register")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_vec(&serde_json::json!({"name": "hb-agent"})).unwrap(),
            ))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let reg: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let id = reg["id"].as_str().unwrap();

        // Heartbeat
        let hb_body = serde_json::json!({
            "status": "running",
            "current_task": "processing",
            "cpu_percent": 25.5,
            "memory_mb": 512
        });
        let req = Request::builder()
            .method("POST")
            .uri(format!("/v1/agents/{}/heartbeat", id))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&hb_body).unwrap()))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // Verify heartbeat updated the agent
        let req = Request::builder()
            .uri(format!("/v1/agents/{}", id))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let detail: AgentDetail = serde_json::from_slice(&body).unwrap();
        assert_eq!(detail.status, "running");
        assert_eq!(detail.current_task, Some("processing".to_string()));
        assert!(detail.last_heartbeat.is_some());
    }

    #[tokio::test]
    async fn test_heartbeat_not_found() {
        let app = test_app();
        let req = Request::builder()
            .method("POST")
            .uri(format!("/v1/agents/{}/heartbeat", Uuid::new_v4()))
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_vec(&serde_json::json!({})).unwrap(),
            ))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_deregister_agent() {
        let state = test_state();
        let app = build_router(state.clone());

        // Register
        let req = Request::builder()
            .method("POST")
            .uri("/v1/agents/register")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_vec(&serde_json::json!({"name": "delete-me"})).unwrap(),
            ))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let reg: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let id = reg["id"].as_str().unwrap();

        // Delete
        let req = Request::builder()
            .method("DELETE")
            .uri(format!("/v1/agents/{}", id))
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // Verify gone
        let req = Request::builder()
            .uri(format!("/v1/agents/{}", id))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_deregister_not_found() {
        let app = test_app();
        let req = Request::builder()
            .method("DELETE")
            .uri(format!("/v1/agents/{}", Uuid::new_v4()))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn test_api_state_default() {
        let state = ApiState::default();
        assert!(state.started_at <= Utc::now());
    }

    #[test]
    fn test_resource_needs_default() {
        let rn = ResourceNeeds::default();
        assert_eq!(rn.min_memory_mb, 0);
        assert_eq!(rn.min_cpu_shares, 0);
    }

    #[tokio::test]
    async fn test_register_name_too_long() {
        let app = test_app();
        let long_name = "x".repeat(257);
        let req_body = serde_json::json!({"name": long_name});

        let req = Request::builder()
            .method("POST")
            .uri("/v1/agents/register")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_heartbeat_partial_update() {
        let state = test_state();
        let app = build_router(state.clone());

        // Register
        let req = Request::builder()
            .method("POST")
            .uri("/v1/agents/register")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_vec(&serde_json::json!({"name": "partial-hb"})).unwrap(),
            ))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let reg: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let id = reg["id"].as_str().unwrap();

        // Heartbeat with only status (no task, cpu, mem)
        let hb_body = serde_json::json!({"status": "idle"});
        let req = Request::builder()
            .method("POST")
            .uri(format!("/v1/agents/{}/heartbeat", id))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&hb_body).unwrap()))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_list_agents_empty() {
        let app = test_app();
        let req = Request::builder()
            .uri("/v1/agents")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: AgentListResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(json.total, 0);
        assert!(json.agents.is_empty());
    }

    #[tokio::test]
    async fn test_metrics_empty() {
        let app = test_app();
        let req = Request::builder()
            .uri("/v1/metrics")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: AgentMetricsResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(json.total_agents, 0);
        assert!(json.agents_by_status.is_empty());
        assert!(json.uptime_seconds < 5);
        assert!(json.avg_cpu_percent.is_none());
        assert_eq!(json.total_memory_mb, 0);
    }

    #[tokio::test]
    async fn test_metrics_with_agents() {
        let state = test_state();
        let app = build_router(state.clone());

        // Register two agents
        for name in ["metric-a", "metric-b"] {
            let req = Request::builder()
                .method("POST")
                .uri("/v1/agents/register")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({"name": name})).unwrap(),
                ))
                .unwrap();
            let resp = app.clone().oneshot(req).await.unwrap();
            assert_eq!(resp.status(), StatusCode::CREATED);

            // Get agent ID for heartbeat
            let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
            let reg: serde_json::Value = serde_json::from_slice(&body).unwrap();
            let id = reg["id"].as_str().unwrap();

            // Send heartbeat with CPU and memory
            let hb = serde_json::json!({
                "status": "running",
                "cpu_percent": 50.0,
                "memory_mb": 256
            });
            let req = Request::builder()
                .method("POST")
                .uri(format!("/v1/agents/{}/heartbeat", id))
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&hb).unwrap()))
                .unwrap();
            app.clone().oneshot(req).await.unwrap();
        }

        // Check metrics
        let req = Request::builder()
            .uri("/v1/metrics")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: AgentMetricsResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(json.total_agents, 2);
        assert_eq!(json.agents_by_status.get("running"), Some(&2));
        assert_eq!(json.avg_cpu_percent, Some(50.0));
        assert_eq!(json.total_memory_mb, 512);
    }

    // ==================================================================
    // New coverage: request/response types, validation, serialization,
    // heartbeat empty body, register with metadata, name boundary
    // ==================================================================

    #[test]
    fn test_register_request_serialization() {
        let req = RegisterAgentRequest {
            name: "test".to_string(),
            capabilities: vec!["file:read".to_string()],
            resource_needs: ResourceNeeds { min_memory_mb: 256, min_cpu_shares: 50 },
            metadata: {
                let mut m = HashMap::new();
                m.insert("version".to_string(), "1.0".to_string());
                m
            },
        };
        let json = serde_json::to_string(&req).unwrap();
        let deser: RegisterAgentRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.name, "test");
        assert_eq!(deser.capabilities.len(), 1);
        assert_eq!(deser.resource_needs.min_memory_mb, 256);
        assert_eq!(deser.metadata.get("version").unwrap(), "1.0");
    }

    #[test]
    fn test_heartbeat_request_defaults() {
        let json = "{}";
        let req: HeartbeatRequest = serde_json::from_str(json).unwrap();
        assert!(req.status.is_none());
        assert!(req.current_task.is_none());
        assert!(req.cpu_percent.is_none());
        assert!(req.memory_mb.is_none());
    }

    #[test]
    fn test_error_response_serialization() {
        let err = ErrorResponse {
            error: "Not found".to_string(),
            code: 404,
        };
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("Not found"));
        assert!(json.contains("404"));
    }

    #[test]
    fn test_health_response_serialization() {
        let mut components = HashMap::new();
        components.insert(
            "agent_registry".to_string(),
            ComponentHealth {
                status: "ok".to_string(),
                message: Some("5 agents registered".to_string()),
            },
        );
        let resp = HealthResponse {
            status: "ok".to_string(),
            service: "test".to_string(),
            version: "0.1.0".to_string(),
            agents_registered: 5,
            uptime_seconds: 3600,
            components,
            system: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let deser: HealthResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.agents_registered, 5);
        assert_eq!(deser.uptime_seconds, 3600);
        assert!(deser.components.contains_key("agent_registry"));
    }

    #[test]
    fn test_component_health_serialization() {
        let ch = ComponentHealth {
            status: "ok".to_string(),
            message: Some("all good".to_string()),
        };
        let json = serde_json::to_string(&ch).unwrap();
        let deser: ComponentHealth = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.status, "ok");
        assert_eq!(deser.message.unwrap(), "all good");

        // With None message
        let ch2 = ComponentHealth {
            status: "degraded".to_string(),
            message: None,
        };
        let json2 = serde_json::to_string(&ch2).unwrap();
        let deser2: ComponentHealth = serde_json::from_str(&json2).unwrap();
        assert_eq!(deser2.status, "degraded");
        assert!(deser2.message.is_none());
    }

    #[test]
    fn test_system_health_serialization() {
        let sh = SystemHealth {
            hostname: "test-host".to_string(),
            load_average: [1.5, 2.0, 0.5],
            memory_total_mb: 16384,
            memory_available_mb: 8192,
            disk_free_mb: 50000,
        };
        let json = serde_json::to_string(&sh).unwrap();
        let deser: SystemHealth = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.hostname, "test-host");
        assert_eq!(deser.load_average[0], 1.5);
        assert_eq!(deser.memory_total_mb, 16384);
        assert_eq!(deser.memory_available_mb, 8192);
        assert_eq!(deser.disk_free_mb, 50000);
    }

    #[test]
    fn test_gather_system_health() {
        let health = gather_system_health();
        // Should have a non-empty hostname on any system
        assert!(!health.hostname.is_empty());
        // On Linux these should be populated
        if cfg!(target_os = "linux") {
            assert!(health.memory_total_mb > 0);
        }
    }

    #[test]
    fn test_agent_metrics_response_serialization() {
        let resp = AgentMetricsResponse {
            total_agents: 3,
            agents_by_status: {
                let mut m = HashMap::new();
                m.insert("running".to_string(), 2);
                m.insert("idle".to_string(), 1);
                m
            },
            uptime_seconds: 120,
            avg_cpu_percent: Some(42.5),
            total_memory_mb: 1024,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let deser: AgentMetricsResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.total_agents, 3);
        assert_eq!(deser.avg_cpu_percent, Some(42.5));
    }

    #[test]
    fn test_default_port_constant() {
        assert_eq!(DEFAULT_PORT, 8090);
    }

    #[tokio::test]
    async fn test_register_name_exactly_256_chars() {
        let app = test_app();
        let name = "x".repeat(256);
        let req_body = serde_json::json!({"name": name});

        let req = Request::builder()
            .method("POST")
            .uri("/v1/agents/register")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        // 256 chars is exactly the limit, should succeed
        assert_eq!(resp.status(), StatusCode::CREATED);
    }

    #[tokio::test]
    async fn test_register_with_metadata() {
        let state = test_state();
        let app = build_router(state.clone());

        let req_body = serde_json::json!({
            "name": "meta-agent",
            "capabilities": [],
            "metadata": {"runtime": "python", "version": "3.11"}
        });

        let req = Request::builder()
            .method("POST")
            .uri("/v1/agents/register")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();

        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let reg: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let id = reg["id"].as_str().unwrap();

        // Fetch and check metadata was stored
        let req = Request::builder()
            .uri(format!("/v1/agents/{}", id))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let detail: AgentDetail = serde_json::from_slice(&body).unwrap();
        assert_eq!(detail.metadata.get("runtime").unwrap(), "python");
    }

    #[tokio::test]
    async fn test_heartbeat_empty_body_updates_timestamp() {
        let state = test_state();
        let app = build_router(state.clone());

        // Register
        let req = Request::builder()
            .method("POST")
            .uri("/v1/agents/register")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&serde_json::json!({"name": "hb-empty"})).unwrap()))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let reg: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let id = reg["id"].as_str().unwrap();

        // Empty heartbeat
        let req = Request::builder()
            .method("POST")
            .uri(format!("/v1/agents/{}/heartbeat", id))
            .header("content-type", "application/json")
            .body(Body::from(b"{}".to_vec()))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // Verify last_heartbeat was set
        let req = Request::builder()
            .uri(format!("/v1/agents/{}", id))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let detail: AgentDetail = serde_json::from_slice(&body).unwrap();
        assert!(detail.last_heartbeat.is_some());
        // Status should remain "registered" since no status was sent
        assert_eq!(detail.status, "registered");
    }
}
