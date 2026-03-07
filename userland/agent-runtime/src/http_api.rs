//! Agent Registration HTTP API
//!
//! Axum HTTP server on port 8090 providing REST endpoints for external
//! consumers (AGNOSTIC, SecureYeoman) to register agents, send heartbeats,
//! and query agent status.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::middleware::{self, Next};
use axum::response::IntoResponse;
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use std::path::PathBuf;
use tracing::{debug, info, warn};
use uuid::Uuid;

use agnos_common::{
    AgentId, FsAccess, FilesystemRule, NetworkAccess, NetworkPolicy, SandboxConfig, SeccompAction,
    SeccompRule,
    audit::AuditChain,
    telemetry::{SpanCollector, TraceContext},
};

use crate::ipc::RpcRegistry;
use crate::learning::{AnomalyDetector, BehaviorSample};

use crate::rag::{RagPipeline, RagConfig};
use crate::knowledge_base::{KnowledgeBase, KnowledgeSource};
#[allow(unused_imports)]
use crate::file_watcher::FileWatcher;

/// Default listen port for the agent registration API.
pub const DEFAULT_PORT: u16 = 8090;

/// Maximum number of trace entries kept in memory.
pub const MAX_TRACES: usize = 10_000;
/// Maximum number of audit events kept in memory.
pub const MAX_AUDIT_BUFFER: usize = 100_000;
/// Maximum number of webhook registrations allowed.
pub const MAX_WEBHOOKS: usize = 1_000;

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

fn default_top_k() -> usize { 5 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeSearchRequest {
    pub query: String,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize { 10 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeIndexRequest {
    pub path: String,
    #[serde(default)]
    pub source: Option<String>,
}

// ---------------------------------------------------------------------------
// Shared state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct RegisteredAgentEntry {
    pub detail: AgentDetail,
}

/// In-memory per-agent key-value store for the REST API bridge.
/// Maps agent_id -> key -> value.
#[derive(Debug, Clone, Default)]
pub struct ApiMemoryStore {
    data: Arc<RwLock<HashMap<String, HashMap<String, serde_json::Value>>>>,
}

impl ApiMemoryStore {
    pub fn new() -> Self {
        Self {
            data: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn get(&self, agent_id: &str, key: &str) -> Option<serde_json::Value> {
        let data = self.data.read().await;
        data.get(agent_id).and_then(|m| m.get(key).cloned())
    }

    pub async fn set(&self, agent_id: &str, key: &str, value: serde_json::Value) {
        let mut data = self.data.write().await;
        data.entry(agent_id.to_string())
            .or_default()
            .insert(key.to_string(), value);
    }

    pub async fn delete(&self, agent_id: &str, key: &str) -> bool {
        let mut data = self.data.write().await;
        if let Some(agent_map) = data.get_mut(agent_id) {
            agent_map.remove(key).is_some()
        } else {
            false
        }
    }

    pub async fn list_keys(&self, agent_id: &str) -> Vec<String> {
        let data = self.data.read().await;
        data.get(agent_id)
            .map(|m| {
                let mut keys: Vec<String> = m.keys().cloned().collect();
                keys.sort();
                keys
            })
            .unwrap_or_default()
    }
}

#[derive(Clone)]
pub struct ApiState {
    agents: Arc<RwLock<HashMap<Uuid, RegisteredAgentEntry>>>,
    started_at: DateTime<Utc>,
    pub webhooks: Arc<RwLock<Vec<WebhookRegistration>>>,
    pub audit_buffer: Arc<RwLock<Vec<AuditEvent>>>,
    pub audit_chain: Arc<RwLock<AuditChain>>,
    pub memory_store: ApiMemoryStore,
    pub traces: Arc<RwLock<Vec<serde_json::Value>>>,
    pub rag_pipeline: Arc<RwLock<RagPipeline>>,
    pub knowledge_base: Arc<RwLock<KnowledgeBase>>,
    /// Distributed tracing span collector (OpenTelemetry-like).
    pub span_collector: Arc<SpanCollector>,
    /// Agent-to-agent RPC method registry.
    pub rpc_registry: Arc<RwLock<RpcRegistry>>,
    /// Behavior anomaly detector for agent monitoring.
    pub anomaly_detector: Arc<RwLock<AnomalyDetector>>,
    /// Marketplace local registry for package management.
    pub marketplace_registry: Arc<RwLock<crate::marketplace::local_registry::LocalRegistry>>,
    /// Optional Bearer token for API authentication.
    /// When `Some`, all endpoints except `GET /v1/health` require it.
    pub api_key: Option<String>,
}

impl std::fmt::Debug for ApiState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApiState")
            .field("started_at", &self.started_at)
            .finish()
    }
}

impl ApiState {
    pub fn new() -> Self {
        let api_key = std::env::var("AGNOS_RUNTIME_API_KEY").ok().filter(|k| !k.is_empty());
        let marketplace_dir = std::env::var("AGNOS_MARKETPLACE_DIR")
            .unwrap_or_else(|_| crate::marketplace::local_registry::DEFAULT_MARKETPLACE_DIR.to_string());
        let marketplace_registry = crate::marketplace::local_registry::LocalRegistry::new(
            std::path::Path::new(&marketplace_dir),
        )
        .unwrap_or_else(|_| {
            // Fallback to temp dir if default path is not writable
            crate::marketplace::local_registry::LocalRegistry::new(
                &std::env::temp_dir().join("agnos-marketplace"),
            )
            .expect("Failed to create marketplace registry")
        });
        Self {
            agents: Arc::new(RwLock::new(HashMap::new())),
            started_at: Utc::now(),
            webhooks: Arc::new(RwLock::new(Vec::new())),
            audit_buffer: Arc::new(RwLock::new(Vec::new())),
            audit_chain: Arc::new(RwLock::new(AuditChain::new())),
            memory_store: ApiMemoryStore::new(),
            traces: Arc::new(RwLock::new(Vec::new())),
            rag_pipeline: Arc::new(RwLock::new(RagPipeline::new(RagConfig::default()))),
            knowledge_base: Arc::new(RwLock::new(KnowledgeBase::new())),
            span_collector: Arc::new(SpanCollector::new()),
            rpc_registry: Arc::new(RwLock::new(RpcRegistry::new())),
            anomaly_detector: Arc::new(RwLock::new(AnomalyDetector::new(100, 2.0))),
            marketplace_registry: Arc::new(RwLock::new(marketplace_registry)),
            api_key,
        }
    }

    /// Create a new `ApiState` with an explicit API key (useful for testing).
    pub fn with_api_key(api_key: Option<String>) -> Self {
        let tmp_marketplace = crate::marketplace::local_registry::LocalRegistry::new(
            &std::env::temp_dir().join("agnos-marketplace-test"),
        )
        .expect("Failed to create test marketplace registry");
        Self {
            agents: Arc::new(RwLock::new(HashMap::new())),
            started_at: Utc::now(),
            webhooks: Arc::new(RwLock::new(Vec::new())),
            audit_buffer: Arc::new(RwLock::new(Vec::new())),
            audit_chain: Arc::new(RwLock::new(AuditChain::new())),
            memory_store: ApiMemoryStore::new(),
            traces: Arc::new(RwLock::new(Vec::new())),
            rag_pipeline: Arc::new(RwLock::new(RagPipeline::new(RagConfig::default()))),
            knowledge_base: Arc::new(RwLock::new(KnowledgeBase::new())),
            span_collector: Arc::new(SpanCollector::new()),
            rpc_registry: Arc::new(RwLock::new(RpcRegistry::new())),
            anomaly_detector: Arc::new(RwLock::new(AnomalyDetector::new(100, 2.0))),
            marketplace_registry: Arc::new(RwLock::new(tmp_marketplace)),
            api_key,
        }
    }

    /// Acquire a read lock on the agents map.
    pub async fn agents_read(
        &self,
    ) -> tokio::sync::RwLockReadGuard<'_, HashMap<Uuid, RegisteredAgentEntry>> {
        self.agents.read().await
    }

    /// Acquire a write lock on the agents map.
    pub async fn agents_write(
        &self,
    ) -> tokio::sync::RwLockWriteGuard<'_, HashMap<Uuid, RegisteredAgentEntry>> {
        self.agents.write().await
    }

    /// Return the instant the API state was created.
    pub fn started_at(&self) -> DateTime<Utc> {
        self.started_at
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

    let trace_ctx = TraceContext::new_root("agent-runtime");
    let trace_headers = trace_ctx.inject_headers();

    let mut request_builder = client.get(format!("{}/v1/health", gateway_url));
    for (key, value) in &trace_headers {
        request_builder = request_builder.header(key.as_str(), value.as_str());
    }

    match request_builder.send().await
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
// Authentication middleware
// ---------------------------------------------------------------------------

/// Bearer token authentication middleware.
///
/// If the `ApiState` has an `api_key` set, all requests (except `GET /v1/health`)
/// must include `Authorization: Bearer <token>`. When no key is configured the
/// middleware is a pass-through (dev mode).
async fn auth_middleware(
    State(state): State<ApiState>,
    req: axum::extract::Request,
    next: Next,
) -> axum::response::Response {
    let api_key = match &state.api_key {
        Some(key) => key,
        None => return next.run(req).await, // dev mode — no auth
    };

    // Allow health endpoint without auth
    if req.uri().path() == "/v1/health" && req.method() == axum::http::Method::GET {
        return next.run(req).await;
    }

    let auth_header = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());

    match auth_header {
        Some(value) if value.starts_with("Bearer ") => {
            let token = &value[7..];
            if token == api_key.as_str() {
                next.run(req).await
            } else {
                (
                    StatusCode::UNAUTHORIZED,
                    Json(serde_json::json!({"error": "Invalid bearer token", "code": 401})),
                )
                    .into_response()
            }
        }
        _ => (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "Missing or malformed Authorization header", "code": 401})),
        )
            .into_response(),
    }
}

// ---------------------------------------------------------------------------
// RPC request/response types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcRegisterRequest {
    pub agent_id: String,
    pub methods: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcCallRequest {
    pub method: String,
    pub params: serde_json::Value,
    #[serde(default = "default_rpc_timeout")]
    pub timeout_ms: u64,
    #[serde(default)]
    pub sender_id: Option<String>,
}

fn default_rpc_timeout() -> u64 { 5000 }

// ---------------------------------------------------------------------------
// RPC handlers
// ---------------------------------------------------------------------------

async fn rpc_list_methods_handler(
    State(state): State<ApiState>,
) -> impl IntoResponse {
    let registry = state.rpc_registry.read().await;
    let methods: Vec<_> = registry.all_methods()
        .into_iter()
        .map(|(method, agent_id)| serde_json::json!({
            "method": method,
            "handler_agent": agent_id.to_string(),
        }))
        .collect();
    Json(serde_json::json!({
        "methods": methods,
    }))
}

async fn rpc_agent_methods_handler(
    State(state): State<ApiState>,
    Path(agent_id): Path<String>,
) -> impl IntoResponse {
    let parsed = match Uuid::parse_str(&agent_id) {
        Ok(u) => AgentId(u),
        Err(_) => {
            return (StatusCode::BAD_REQUEST, Json(serde_json::json!({
                "error": "invalid agent_id UUID",
                "agent_id": agent_id,
            }))).into_response();
        }
    };
    let registry = state.rpc_registry.read().await;
    Json(serde_json::json!({
        "agent_id": agent_id,
        "methods": registry.list_methods(&parsed),
    })).into_response()
}

async fn rpc_register_handler(
    State(state): State<ApiState>,
    Json(req): Json<RpcRegisterRequest>,
) -> impl IntoResponse {
    let parsed = match Uuid::parse_str(&req.agent_id) {
        Ok(u) => AgentId(u),
        Err(_) => {
            return (StatusCode::BAD_REQUEST, Json(serde_json::json!({
                "error": "invalid agent_id UUID",
                "agent_id": req.agent_id,
            }))).into_response();
        }
    };
    let mut registry = state.rpc_registry.write().await;
    for method in &req.methods {
        registry.register_method(parsed, method);
    }
    (StatusCode::OK, Json(serde_json::json!({
        "status": "registered",
        "agent_id": req.agent_id,
        "methods": req.methods,
    }))).into_response()
}

async fn rpc_call_handler(
    State(state): State<ApiState>,
    Json(req): Json<RpcCallRequest>,
) -> impl IntoResponse {
    let registry = state.rpc_registry.read().await;
    match registry.find_handler(&req.method) {
        Some(handler_id) => {
            Json(serde_json::json!({
                "status": "routed",
                "method": req.method,
                "handler_agent": handler_id.to_string(),
                "message": "RPC call dispatched (async response pending)"
            })).into_response()
        }
        None => {
            (StatusCode::NOT_FOUND, Json(serde_json::json!({
                "error": "method_not_found",
                "method": req.method,
            }))).into_response()
        }
    }
}

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

async fn anomaly_submit_handler(
    State(state): State<ApiState>,
    Json(req): Json<BehaviorSampleRequest>,
) -> impl IntoResponse {
    let parsed = match Uuid::parse_str(&req.agent_id) {
        Ok(u) => AgentId(u),
        Err(_) => {
            return (StatusCode::BAD_REQUEST, Json(serde_json::json!({
                "error": "invalid agent_id UUID",
                "agent_id": req.agent_id,
            }))).into_response();
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
                audit.push(AuditEvent {
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
    })).into_response()
}

async fn anomaly_alerts_handler(
    State(state): State<ApiState>,
) -> impl IntoResponse {
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

async fn anomaly_baseline_handler(
    State(state): State<ApiState>,
    Path(agent_id): Path<String>,
) -> impl IntoResponse {
    let parsed = match Uuid::parse_str(&agent_id) {
        Ok(u) => AgentId(u),
        Err(_) => {
            return (StatusCode::BAD_REQUEST, Json(serde_json::json!({
                "error": "invalid agent_id UUID",
                "agent_id": agent_id,
            }))).into_response();
        }
    };
    let detector = state.anomaly_detector.read().await;
    match detector.get_baseline(&parsed) {
        Some(baseline) => Json(serde_json::json!({
            "agent_id": agent_id,
            "sample_count": baseline.sample_count(),
            "has_baseline": baseline.sample_count() > 0,
        })).into_response(),
        None => (StatusCode::NOT_FOUND, Json(serde_json::json!({
            "error": "no baseline for agent",
            "agent_id": agent_id,
        }))).into_response(),
    }
}

async fn anomaly_clear_handler(
    State(state): State<ApiState>,
    Path(agent_id): Path<String>,
) -> impl IntoResponse {
    let parsed = match Uuid::parse_str(&agent_id) {
        Ok(u) => AgentId(u),
        Err(_) => {
            return (StatusCode::BAD_REQUEST, Json(serde_json::json!({
                "error": "invalid agent_id UUID",
                "agent_id": agent_id,
            }))).into_response();
        }
    };
    let mut detector = state.anomaly_detector.write().await;
    detector.clear_alerts(&parsed);
    Json(serde_json::json!({
        "status": "cleared",
        "agent_id": agent_id,
    })).into_response()
}

// ---------------------------------------------------------------------------
// RAG & Knowledge Base handlers
// ---------------------------------------------------------------------------

async fn rag_ingest_handler(
    State(state): State<ApiState>,
    Json(req): Json<RagIngestRequest>,
) -> impl IntoResponse {
    let metadata = serde_json::to_value(&req.metadata).unwrap_or_default();
    let mut pipeline = state.rag_pipeline.write().await;
    match pipeline.ingest_text(&req.text, metadata) {
        Ok(ids) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "ingested",
                "chunks": ids.len()
            })),
        ).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ).into_response(),
    }
}

async fn rag_query_handler(
    State(state): State<ApiState>,
    Json(req): Json<RagQueryRequest>,
) -> impl IntoResponse {
    let pipeline = state.rag_pipeline.read().await;
    let context = pipeline.query_text(&req.query);
    Json(serde_json::json!({
        "query": req.query,
        "chunks": context.chunks.iter().map(|c| serde_json::json!({
            "content": c.content,
            "score": c.score,
            "metadata": c.metadata,
        })).collect::<Vec<_>>(),
        "formatted_context": context.formatted_context,
        "token_estimate": context.total_tokens_estimate,
    }))
}

async fn rag_stats_handler(
    State(state): State<ApiState>,
) -> impl IntoResponse {
    let pipeline = state.rag_pipeline.read().await;
    Json(serde_json::json!({
        "index_size": pipeline.index.len(),
        "config": {
            "top_k": pipeline.config.top_k,
            "chunk_size": pipeline.config.chunk_size,
            "overlap": pipeline.config.overlap,
            "min_relevance_score": pipeline.config.min_relevance_score,
        }
    }))
}

async fn knowledge_search_handler(
    State(state): State<ApiState>,
    Json(req): Json<KnowledgeSearchRequest>,
) -> impl IntoResponse {
    let kb = state.knowledge_base.read().await;
    let results = if let Some(ref src) = req.source {
        let source = match src.as_str() {
            "manpage" => KnowledgeSource::ManPage,
            "manifest" => KnowledgeSource::AgentManifest,
            "audit" => KnowledgeSource::AuditLog,
            "config" => KnowledgeSource::ConfigFile,
            other => KnowledgeSource::Custom(other.to_string()),
        };
        kb.search_by_source(&source, req.limit)
            .into_iter()
            .map(|entry| crate::knowledge_base::KnowledgeResult {
                relevance_score: 1.0,
                entry,
            })
            .collect::<Vec<_>>()
    } else {
        kb.search(&req.query, req.limit)
    };
    Json(serde_json::json!({
        "query": req.query,
        "results": results.iter().map(|r| serde_json::json!({
            "id": r.entry.id.to_string(),
            "source": format!("{:?}", r.entry.source),
            "path": r.entry.path,
            "relevance": r.relevance_score,
            "content_preview": &r.entry.content[..r.entry.content.len().min(200)],
        })).collect::<Vec<_>>(),
        "total": results.len(),
    }))
}

async fn knowledge_stats_handler(
    State(state): State<ApiState>,
) -> impl IntoResponse {
    let kb = state.knowledge_base.read().await;
    let stats = kb.stats();
    Json(serde_json::json!({
        "total_entries": stats.total_entries,
        "total_bytes": stats.total_bytes,
        "by_source": stats.entries_by_source,
    }))
}

async fn knowledge_index_handler(
    State(state): State<ApiState>,
    Json(req): Json<KnowledgeIndexRequest>,
) -> impl IntoResponse {
    let source = match req.source.as_deref() {
        Some("manpage") => KnowledgeSource::ManPage,
        Some("manifest") => KnowledgeSource::AgentManifest,
        Some("audit") => KnowledgeSource::AuditLog,
        Some("config") => KnowledgeSource::ConfigFile,
        Some(other) => KnowledgeSource::Custom(other.to_string()),
        None => KnowledgeSource::ConfigFile,
    };
    let mut kb = state.knowledge_base.write().await;
    match kb.index_directory(std::path::Path::new(&req.path), source) {
        Ok(count) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "indexed",
                "path": req.path,
                "entries_added": count,
            })),
        ).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ).into_response(),
    }
}

// ---------------------------------------------------------------------------
// Marketplace handlers
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct MarketplaceSearchQuery {
    #[serde(default)]
    pub q: String,
}

async fn marketplace_installed_handler(
    State(state): State<ApiState>,
) -> impl IntoResponse {
    let registry = state.marketplace_registry.read().await;
    let packages: Vec<serde_json::Value> = registry
        .list_installed()
        .iter()
        .map(|p| {
            serde_json::json!({
                "name": p.name(),
                "version": p.version(),
                "publisher": p.publisher(),
                "category": format!("{}", p.manifest.category),
                "installed_at": p.installed_at.to_rfc3339(),
                "installed_size": p.installed_size,
            })
        })
        .collect();
    Json(serde_json::json!({
        "packages": packages,
        "total": packages.len(),
    }))
}

async fn marketplace_search_handler(
    State(state): State<ApiState>,
    Query(params): Query<MarketplaceSearchQuery>,
) -> impl IntoResponse {
    let registry = state.marketplace_registry.read().await;
    let results: Vec<serde_json::Value> = registry
        .search(&params.q)
        .iter()
        .map(|p| {
            serde_json::json!({
                "name": p.name(),
                "version": p.version(),
                "publisher": p.publisher(),
                "description": p.manifest.agent.description,
                "category": format!("{}", p.manifest.category),
            })
        })
        .collect();
    Json(serde_json::json!({
        "results": results,
        "total": results.len(),
        "query": params.q,
    }))
}

#[derive(Debug, Deserialize)]
pub struct MarketplaceInstallRequest {
    pub path: String,
}

async fn marketplace_install_handler(
    State(state): State<ApiState>,
    Json(req): Json<MarketplaceInstallRequest>,
) -> impl IntoResponse {
    let mut registry = state.marketplace_registry.write().await;
    let path = std::path::Path::new(&req.path);

    match registry.install_package(path) {
        Ok(result) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "installed",
                "name": result.name,
                "version": result.version,
                "install_dir": result.install_dir.to_string_lossy(),
                "upgraded_from": result.upgraded_from,
            })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

async fn marketplace_uninstall_handler(
    State(state): State<ApiState>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let mut registry = state.marketplace_registry.write().await;
    match registry.uninstall_package(&name) {
        Ok(result) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "uninstalled",
                "name": result.name,
                "version": result.version,
                "files_removed": result.files_removed,
            })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

async fn marketplace_info_handler(
    State(state): State<ApiState>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let registry = state.marketplace_registry.read().await;
    match registry.get_package(&name) {
        Some(pkg) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "name": pkg.name(),
                "version": pkg.version(),
                "publisher": pkg.publisher(),
                "description": pkg.manifest.agent.description,
                "category": format!("{}", pkg.manifest.category),
                "runtime": pkg.manifest.runtime,
                "installed_at": pkg.installed_at.to_rfc3339(),
                "installed_size": pkg.installed_size,
                "auto_update": pkg.auto_update,
                "package_hash": pkg.package_hash,
                "tags": pkg.manifest.tags,
                "dependencies": pkg.manifest.dependencies,
            })),
        )
            .into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": format!("Package '{}' not found", name)})),
        )
            .into_response(),
    }
}

// ---------------------------------------------------------------------------
// Router & server
// ---------------------------------------------------------------------------

/// Build the Axum router for the agent registration API.
pub fn build_router(state: ApiState) -> Router {
    Router::new()
        .route("/v1/health", get(health_handler))
        .route("/v1/metrics", get(metrics_handler))
        .route("/v1/metrics/prometheus", get(prometheus_metrics_handler))
        .route("/v1/agents/register", post(register_agent_handler))
        .route("/v1/agents/:id/heartbeat", post(heartbeat_handler))
        .route("/v1/agents", get(list_agents_handler))
        .route("/v1/agents/:id", get(get_agent_handler))
        .route("/v1/agents/:id", delete(deregister_agent_handler))
        .route("/v1/webhooks", post(register_webhook_handler))
        .route("/v1/webhooks", get(list_webhooks_handler))
        .route("/v1/webhooks/:id", delete(delete_webhook_handler))
        .route("/v1/audit/forward", post(forward_audit_handler))
        .route("/v1/audit", get(list_audit_handler))
        .route("/v1/audit/chain", get(audit_chain_handler))
        .route("/v1/audit/chain/verify", get(audit_chain_verify_handler))
        .route("/v1/agents/:id/memory", get(memory_list_handler))
        .route("/v1/agents/:id/memory/:key", get(memory_get_handler))
        .route("/v1/agents/:id/memory/:key", put(memory_set_handler))
        .route("/v1/agents/:id/memory/:key", delete(memory_delete_handler))
        .route("/v1/traces", post(submit_trace_handler))
        .route("/v1/traces", get(list_traces_handler))
        .route("/v1/traces/spans", get(list_spans_handler))
        .route("/v1/mcp/tools", get(crate::mcp_server::mcp_tools_handler))
        .route("/v1/mcp/tools/call", post(crate::mcp_server::mcp_tool_call_handler))
        .route("/v1/sandbox/profiles", post(translate_sandbox_profile_handler))
        .route("/v1/sandbox/profiles/default", get(default_sandbox_profile_handler))
        .route("/v1/sandbox/profiles/validate", post(validate_sandbox_profile_handler))
        // Agent-to-agent RPC routes
        .route("/v1/rpc/methods", get(rpc_list_methods_handler))
        .route("/v1/rpc/methods/:agent_id", get(rpc_agent_methods_handler))
        .route("/v1/rpc/register", post(rpc_register_handler))
        .route("/v1/rpc/call", post(rpc_call_handler))
        // Behavior anomaly detection routes
        .route("/v1/anomaly/sample", post(anomaly_submit_handler))
        .route("/v1/anomaly/alerts", get(anomaly_alerts_handler))
        .route("/v1/anomaly/baseline/:agent_id", get(anomaly_baseline_handler))
        .route("/v1/anomaly/alerts/:agent_id", delete(anomaly_clear_handler))
        // RAG pipeline routes
        .route("/v1/rag/ingest", post(rag_ingest_handler))
        .route("/v1/rag/query", post(rag_query_handler))
        .route("/v1/rag/stats", get(rag_stats_handler))
        // Knowledge base routes
        .route("/v1/knowledge/search", post(knowledge_search_handler))
        .route("/v1/knowledge/stats", get(knowledge_stats_handler))
        .route("/v1/knowledge/index", post(knowledge_index_handler))
        // Marketplace routes
        .route("/v1/marketplace/installed", get(marketplace_installed_handler))
        .route("/v1/marketplace/search", get(marketplace_search_handler))
        .route("/v1/marketplace/install", post(marketplace_install_handler))
        .route("/v1/marketplace/:name", get(marketplace_info_handler))
        .route("/v1/marketplace/:name", delete(marketplace_uninstall_handler))
        .layer(middleware::from_fn_with_state(state.clone(), auth_middleware))
        .with_state(state)
}

/// Start the HTTP API server on the given port.
pub async fn start_server(port: u16) -> anyhow::Result<()> {
    let state = ApiState::new();
    if state.api_key.is_none() {
        warn!(
            "AGNOS_RUNTIME_API_KEY is not set — Agent Runtime API (port {}) is running WITHOUT authentication (dev mode)",
            port
        );
    }
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

// ===========================================================================
// 3a. Prometheus Metrics Endpoint
// ===========================================================================

async fn prometheus_metrics_handler(State(state): State<ApiState>) -> impl IntoResponse {
    let agents = state.agents.read().await;
    let total = agents.len();

    let mut by_status: HashMap<String, usize> = HashMap::new();
    for entry in agents.values() {
        *by_status.entry(entry.detail.status.clone()).or_default() += 1;
    }

    let mut lines = Vec::new();
    lines.push("# HELP agnos_agents_total Total registered agents".to_string());
    lines.push("# TYPE agnos_agents_total gauge".to_string());
    lines.push(format!("agnos_agents_total {}", total));

    lines.push("# HELP agnos_agent_status Agent status breakdown".to_string());
    lines.push("# TYPE agnos_agent_status gauge".to_string());
    for (status, count) in &by_status {
        lines.push(format!("agnos_agent_status{{status=\"{}\"}} {}", status, count));
    }

    let uptime = (Utc::now() - state.started_at).num_seconds().max(0) as u64;
    lines.push("# HELP agnos_uptime_seconds Uptime in seconds".to_string());
    lines.push("# TYPE agnos_uptime_seconds gauge".to_string());
    lines.push(format!("agnos_uptime_seconds {}", uptime));

    (
        StatusCode::OK,
        [(axum::http::header::CONTENT_TYPE, "text/plain; version=0.0.4; charset=utf-8")],
        lines.join("\n"),
    )
}

// ===========================================================================
// 3b. Webhook/Event Sink
// ===========================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookRegistration {
    pub id: Uuid,
    pub url: String,
    pub events: Vec<String>,
    pub secret: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterWebhookRequest {
    pub url: String,
    #[serde(default)]
    pub events: Vec<String>,
    #[serde(default)]
    pub secret: Option<String>,
}

async fn register_webhook_handler(
    State(state): State<ApiState>,
    Json(req): Json<RegisterWebhookRequest>,
) -> impl IntoResponse {
    if req.url.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Webhook URL is required", "code": 400})),
        )
            .into_response();
    }

    let mut webhooks = state.webhooks.write().await;

    if webhooks.len() >= MAX_WEBHOOKS {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": format!("Maximum webhook limit reached ({})", MAX_WEBHOOKS),
                "code": 503
            })),
        )
            .into_response();
    }

    let wh = WebhookRegistration {
        id: Uuid::new_v4(),
        url: req.url,
        events: req.events,
        secret: req.secret,
        created_at: Utc::now(),
    };

    let id = wh.id;
    webhooks.push(wh);
    info!("Webhook registered: {}", id);

    (
        StatusCode::CREATED,
        Json(serde_json::json!({"id": id.to_string(), "status": "registered"})),
    )
        .into_response()
}

async fn list_webhooks_handler(State(state): State<ApiState>) -> impl IntoResponse {
    let webhooks = state.webhooks.read().await;
    let list: Vec<serde_json::Value> = webhooks
        .iter()
        .map(|w| {
            serde_json::json!({
                "id": w.id.to_string(),
                "url": w.url,
                "events": w.events,
                "created_at": w.created_at.to_rfc3339(),
            })
        })
        .collect();
    Json(serde_json::json!({"webhooks": list, "total": list.len()}))
}

async fn delete_webhook_handler(
    State(state): State<ApiState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let mut webhooks = state.webhooks.write().await;
    let before = webhooks.len();
    webhooks.retain(|w| w.id != id);
    if webhooks.len() < before {
        info!("Webhook deleted: {}", id);
        (
            StatusCode::OK,
            Json(serde_json::json!({"status": "deleted", "id": id.to_string()})),
        )
            .into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": format!("Webhook {} not found", id), "code": 404})),
        )
            .into_response()
    }
}

// ===========================================================================
// 3c. Audit Log Forwarding
// ===========================================================================

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

async fn forward_audit_handler(
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
            buffer.remove(0);
        }
        buffer.push(event);
    }

    (
        StatusCode::OK,
        Json(serde_json::json!({"status": "accepted", "events_received": count})),
    )
}

#[derive(Debug, Deserialize)]
pub struct AuditQueryParams {
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
}

async fn list_audit_handler(
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
    if let Some(limit) = params.limit {
        events.truncate(limit);
    }

    let result: Vec<&AuditEvent> = events;
    Json(serde_json::json!({"events": result, "total": result.len()}))
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

async fn audit_chain_handler(
    State(state): State<ApiState>,
    Query(params): Query<AuditChainQueryParams>,
) -> impl IntoResponse {
    let chain = state.audit_chain.read().await;
    let entries = chain.entries();
    let total = entries.len();
    let start = params.offset.min(total);
    let end = (start + params.limit).min(total);
    let page = &entries[start..end];
    Json(serde_json::json!({
        "entries": page,
        "total": total,
        "offset": start,
        "limit": params.limit,
    }))
}

async fn audit_chain_verify_handler(
    State(state): State<ApiState>,
) -> impl IntoResponse {
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

// ===========================================================================
// 3d. Agent Memory Bridge REST API
// ===========================================================================

async fn memory_get_handler(
    State(state): State<ApiState>,
    Path((id, key)): Path<(String, String)>,
) -> impl IntoResponse {
    match state.memory_store.get(&id, &key).await {
        Some(value) => (
            StatusCode::OK,
            Json(serde_json::json!({"key": key, "agent_id": id, "value": value})),
        )
            .into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": format!("Key '{}' not found", key), "code": 404})),
        )
            .into_response(),
    }
}

#[derive(Debug, Deserialize)]
pub struct MemorySetRequest {
    pub value: serde_json::Value,
    #[serde(default)]
    pub tags: Vec<String>,
}

async fn memory_set_handler(
    State(state): State<ApiState>,
    Path((id, key)): Path<(String, String)>,
    Json(req): Json<MemorySetRequest>,
) -> impl IntoResponse {
    state.memory_store.set(&id, &key, req.value).await;
    (
        StatusCode::OK,
        Json(serde_json::json!({"status": "stored", "key": key})),
    )
}

async fn memory_list_handler(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let keys = state.memory_store.list_keys(&id).await;
    let total = keys.len();
    Json(serde_json::json!({"keys": keys, "total": total}))
}

async fn memory_delete_handler(
    State(state): State<ApiState>,
    Path((id, key)): Path<(String, String)>,
) -> impl IntoResponse {
    if state.memory_store.delete(&id, &key).await {
        (
            StatusCode::OK,
            Json(serde_json::json!({"status": "deleted", "key": key})),
        )
            .into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": format!("Key '{}' not found", key), "code": 404})),
        )
            .into_response()
    }
}

// ===========================================================================
// 3e. Reasoning Trace Submission
// ===========================================================================

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

async fn submit_trace_handler(
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
        traces.remove(0);
    }
    traces.push(trace_value);

    (
        StatusCode::CREATED,
        Json(serde_json::json!({"status": "accepted", "trace_count": traces.len()})),
    )
        .into_response()
}

#[derive(Debug, Deserialize)]
pub struct TraceQueryParams {
    #[serde(default)]
    pub agent_id: Option<String>,
}

async fn list_traces_handler(
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

async fn list_spans_handler(
    State(state): State<ApiState>,
) -> impl IntoResponse {
    let json = state.span_collector.export_json();
    Json(serde_json::json!({
        "spans": json,
        "format": "otlp-like"
    }))
}

// ===========================================================================
// 5. Sandbox Profile Mapping API
// ===========================================================================

/// Well-known x86_64 syscall names used for validation.
const KNOWN_SYSCALLS: &[&str] = &[
    "read", "write", "open", "close", "stat", "fstat", "lstat", "poll", "lseek",
    "mmap", "mprotect", "munmap", "brk", "ioctl", "access", "pipe", "select",
    "sched_yield", "mremap", "msync", "mincore", "madvise", "shmget", "shmat",
    "shmctl", "dup", "dup2", "pause", "nanosleep", "getitimer", "alarm",
    "setitimer", "getpid", "sendfile", "socket", "connect", "accept", "sendto",
    "recvfrom", "sendmsg", "recvmsg", "shutdown", "bind", "listen",
    "getsockname", "getpeername", "socketpair", "setsockopt", "getsockopt",
    "clone", "fork", "vfork", "execve", "exit", "wait4", "kill", "uname",
    "fcntl", "flock", "fsync", "fdatasync", "truncate", "ftruncate",
    "getdents", "getcwd", "chdir", "fchdir", "rename", "mkdir", "rmdir",
    "creat", "link", "unlink", "symlink", "readlink", "chmod", "fchmod",
    "chown", "fchown", "lchown", "umask", "gettimeofday", "getrlimit",
    "getrusage", "sysinfo", "times", "ptrace", "getuid", "syslog", "getgid",
    "setuid", "setgid", "geteuid", "getegid", "setpgid", "getppid",
    "getpgrp", "setsid", "setreuid", "setregid", "getgroups", "setgroups",
    "setresuid", "getresuid", "setresgid", "getresgid", "getpgid", "setfsuid",
    "setfsgid", "getsid", "capget", "capset", "rt_sigpending",
    "rt_sigtimedwait", "rt_sigqueueinfo", "rt_sigsuspend", "sigaltstack",
    "utime", "mknod", "personality", "statfs", "fstatfs", "sysfs",
    "getpriority", "setpriority", "sched_setparam", "sched_getparam",
    "sched_setscheduler", "sched_getscheduler", "sched_get_priority_max",
    "sched_get_priority_min", "sched_rr_get_interval", "mlock", "munlock",
    "mlockall", "munlockall", "vhangup", "pivot_root", "prctl",
    "arch_prctl", "adjtimex", "setrlimit", "chroot", "sync", "acct",
    "settimeofday", "mount", "umount2", "swapon", "swapoff", "reboot",
    "sethostname", "setdomainname", "ioperm", "iopl", "create_module",
    "init_module", "delete_module", "clock_gettime", "clock_settime",
    "clock_getres", "clock_nanosleep", "exit_group", "epoll_wait",
    "epoll_ctl", "tgkill", "utimes", "openat", "mkdirat", "fchownat",
    "unlinkat", "renameat", "linkat", "symlinkat", "readlinkat", "fchmodat",
    "faccessat", "pselect6", "ppoll", "set_robust_list", "get_robust_list",
    "splice", "tee", "sync_file_range", "vmsplice", "move_pages",
    "epoll_pwait", "signalfd", "timerfd_create", "eventfd", "fallocate",
    "timerfd_settime", "timerfd_gettime", "accept4", "signalfd4", "eventfd2",
    "epoll_create1", "dup3", "pipe2", "inotify_init1", "preadv", "pwritev",
    "rt_tgsigqueueinfo", "perf_event_open", "recvmmsg", "fanotify_init",
    "fanotify_mark", "prlimit64", "name_to_handle_at", "open_by_handle_at",
    "syncfs", "sendmmsg", "setns", "getcpu", "process_vm_readv",
    "process_vm_writev", "kcmp", "finit_module", "sched_setattr",
    "sched_getattr", "renameat2", "seccomp", "getrandom", "memfd_create",
    "bpf", "execveat", "membarrier", "mlock2", "copy_file_range",
    "preadv2", "pwritev2", "statx", "io_uring_setup", "io_uring_enter",
    "io_uring_register", "pidfd_open", "clone3", "close_range",
    "openat2", "pidfd_getfd", "faccessat2", "epoll_pwait2",
];

fn is_known_syscall(name: &str) -> bool {
    KNOWN_SYSCALLS.contains(&name)
}

fn default_network_mode() -> String {
    "none".to_string()
}

#[derive(Debug, Deserialize)]
struct ExternalSandboxProfile {
    /// Human-readable name
    name: String,
    /// Filesystem paths and their access levels
    #[serde(default)]
    filesystem: Vec<ExternalFsRule>,
    /// Network mode: "none", "localhost", "restricted", "full"
    #[serde(default = "default_network_mode")]
    network_mode: String,
    /// Allowed outbound hosts (for restricted mode)
    #[serde(default)]
    allowed_hosts: Vec<String>,
    /// Allowed outbound ports (for restricted mode)
    #[serde(default)]
    allowed_ports: Vec<u16>,
    /// Blocked syscalls
    #[serde(default)]
    blocked_syscalls: Vec<String>,
    /// Whether to isolate in network namespace
    #[serde(default)]
    isolate_network: Option<bool>,
    /// MAC profile name (optional)
    #[serde(default)]
    mac_profile: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ExternalFsRule {
    path: String,
    /// "none", "read", "readonly", "readwrite", "rw"
    access: String,
}

#[derive(Debug, Serialize)]
struct ValidationResponse {
    valid: bool,
    warnings: Vec<String>,
    errors: Vec<String>,
}

fn map_fs_access(s: &str) -> Option<FsAccess> {
    match s.to_lowercase().as_str() {
        "none" => Some(FsAccess::NoAccess),
        "read" | "readonly" => Some(FsAccess::ReadOnly),
        "readwrite" | "rw" => Some(FsAccess::ReadWrite),
        _ => None,
    }
}

fn map_network_access(s: &str) -> Option<NetworkAccess> {
    match s.to_lowercase().as_str() {
        "none" => Some(NetworkAccess::None),
        "localhost" => Some(NetworkAccess::LocalhostOnly),
        "restricted" => Some(NetworkAccess::Restricted),
        "full" => Some(NetworkAccess::Full),
        _ => None,
    }
}

fn path_has_traversal(p: &str) -> bool {
    let path = std::path::Path::new(p);
    path.components().any(|c| matches!(c, std::path::Component::ParentDir))
}

/// POST /v1/sandbox/profiles — translate an external sandbox profile to AGNOS SandboxConfig.
async fn translate_sandbox_profile_handler(
    Json(req): Json<ExternalSandboxProfile>,
) -> impl IntoResponse {
    // Validate name
    if req.name.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Profile name is required", "code": 400})),
        )
            .into_response();
    }

    // Map filesystem rules
    let mut filesystem_rules = Vec::new();
    for fs in &req.filesystem {
        if path_has_traversal(&fs.path) {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": format!("Path traversal not allowed: {}", fs.path),
                    "code": 400
                })),
            )
                .into_response();
        }
        let access = match map_fs_access(&fs.access) {
            Some(a) => a,
            None => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "error": format!("Invalid filesystem access '{}'; expected none, read, readonly, readwrite, or rw", fs.access),
                        "code": 400
                    })),
                )
                    .into_response();
            }
        };
        filesystem_rules.push(FilesystemRule {
            path: PathBuf::from(&fs.path),
            access,
        });
    }

    // Map network access
    let network_access = match map_network_access(&req.network_mode) {
        Some(na) => na,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": format!("Invalid network_mode '{}'; expected none, localhost, restricted, or full", req.network_mode),
                    "code": 400
                })),
            )
                .into_response();
        }
    };

    // Map blocked syscalls to Deny rules
    let mut seccomp_rules = Vec::new();
    for sc in &req.blocked_syscalls {
        if !is_known_syscall(sc) {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": format!("Unknown syscall '{}' in blocked_syscalls", sc),
                    "code": 400
                })),
            )
                .into_response();
        }
        seccomp_rules.push(SeccompRule {
            syscall: sc.clone(),
            action: SeccompAction::Deny,
        });
    }

    // Build network policy for restricted mode
    let network_policy = if network_access == NetworkAccess::Restricted {
        Some(NetworkPolicy {
            allowed_outbound_ports: req.allowed_ports.clone(),
            allowed_outbound_hosts: req.allowed_hosts.clone(),
            allowed_inbound_ports: Vec::new(),
            enable_nat: true,
        })
    } else {
        None
    };

    let isolate_network = req.isolate_network.unwrap_or(network_access != NetworkAccess::Full);

    let config = SandboxConfig {
        filesystem_rules,
        network_access,
        seccomp_rules,
        isolate_network,
        network_policy,
        mac_profile: req.mac_profile.clone(),
        encrypted_storage: None,
    };

    (StatusCode::OK, Json(serde_json::json!(config))).into_response()
}

/// GET /v1/sandbox/profiles/default — return the default SandboxConfig.
async fn default_sandbox_profile_handler() -> impl IntoResponse {
    let config = SandboxConfig::default();
    Json(serde_json::json!(config))
}

/// POST /v1/sandbox/profiles/validate — validate a SandboxConfig for issues.
async fn validate_sandbox_profile_handler(
    Json(config): Json<SandboxConfig>,
) -> impl IntoResponse {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    // Check filesystem rules for path traversal
    for rule in &config.filesystem_rules {
        let p = rule.path.to_string_lossy();
        if path_has_traversal(&p) {
            errors.push(format!("Path traversal detected in filesystem rule: {}", p));
        }
        if !rule.path.is_absolute() {
            warnings.push(format!("Relative path in filesystem rule: {} — should be absolute", p));
        }
    }

    // Validate syscall names
    for rule in &config.seccomp_rules {
        if !is_known_syscall(&rule.syscall) {
            errors.push(format!("Unknown syscall: {}", rule.syscall));
        }
    }

    // Check network config consistency
    if config.network_access == NetworkAccess::Restricted && config.network_policy.is_none() {
        warnings.push("network_access is Restricted but no network_policy is provided".to_string());
    }
    if config.network_access != NetworkAccess::Restricted && config.network_policy.is_some() {
        warnings.push("network_policy is set but network_access is not Restricted — policy will be ignored".to_string());
    }
    if config.network_access == NetworkAccess::Full && config.isolate_network {
        warnings.push("isolate_network is true with Full network access — this may cause unexpected behavior".to_string());
    }
    if config.network_access == NetworkAccess::None && !config.isolate_network {
        warnings.push("network_access is None but isolate_network is false — consider enabling isolation".to_string());
    }

    let valid = errors.is_empty();

    Json(ValidationResponse {
        valid,
        warnings,
        errors,
    })
}

// ===========================================================================
// Tests
// ===========================================================================

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

    // ==================================================================
    // Phase 6.8: Prometheus, Webhooks, Audit, Memory, Traces tests
    // ==================================================================

    // --- 3a. Prometheus metrics ---

    #[tokio::test]
    async fn test_prometheus_metrics_empty() {
        let app = test_app();
        let req = Request::builder()
            .uri("/v1/metrics/prometheus")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let text = String::from_utf8(body.to_vec()).unwrap();
        assert!(text.contains("# HELP agnos_agents_total"));
        assert!(text.contains("# TYPE agnos_agents_total gauge"));
        assert!(text.contains("agnos_agents_total 0"));
        assert!(text.contains("agnos_uptime_seconds"));
    }

    #[tokio::test]
    async fn test_prometheus_metrics_with_agents() {
        let state = test_state();
        let app = build_router(state.clone());

        // Register an agent
        let req = Request::builder()
            .method("POST")
            .uri("/v1/agents/register")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&serde_json::json!({"name": "prom-agent"})).unwrap()))
            .unwrap();
        app.clone().oneshot(req).await.unwrap();

        let req = Request::builder()
            .uri("/v1/metrics/prometheus")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let text = String::from_utf8(body.to_vec()).unwrap();
        assert!(text.contains("agnos_agents_total 1"));
        assert!(text.contains("agnos_agent_status"));
    }

    // --- 3b. Webhook tests ---

    #[tokio::test]
    async fn test_register_webhook() {
        let state = test_state();
        let app = build_router(state.clone());

        let req_body = serde_json::json!({
            "url": "https://example.com/hook",
            "events": ["agent.registered", "agent.heartbeat"],
            "secret": "s3cret"
        });

        let req = Request::builder()
            .method("POST")
            .uri("/v1/webhooks")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["id"].as_str().is_some());
        assert_eq!(json["status"], "registered");
    }

    #[tokio::test]
    async fn test_register_webhook_empty_url() {
        let app = test_app();
        let req_body = serde_json::json!({"url": "", "events": []});

        let req = Request::builder()
            .method("POST")
            .uri("/v1/webhooks")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_list_webhooks() {
        let state = test_state();
        let app = build_router(state.clone());

        // Register a webhook
        let req_body = serde_json::json!({"url": "https://example.com/hook", "events": ["test"]});
        let req = Request::builder()
            .method("POST")
            .uri("/v1/webhooks")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        app.clone().oneshot(req).await.unwrap();

        // List
        let req = Request::builder()
            .uri("/v1/webhooks")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["total"], 1);
    }

    #[tokio::test]
    async fn test_delete_webhook() {
        let state = test_state();
        let app = build_router(state.clone());

        // Register
        let req_body = serde_json::json!({"url": "https://example.com/hook"});
        let req = Request::builder()
            .method("POST")
            .uri("/v1/webhooks")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let id = json["id"].as_str().unwrap();

        // Delete
        let req = Request::builder()
            .method("DELETE")
            .uri(format!("/v1/webhooks/{}", id))
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // Verify empty
        let req = Request::builder()
            .uri("/v1/webhooks")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["total"], 0);
    }

    #[tokio::test]
    async fn test_delete_webhook_not_found() {
        let app = test_app();
        let req = Request::builder()
            .method("DELETE")
            .uri(format!("/v1/webhooks/{}", Uuid::new_v4()))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_list_webhooks_empty() {
        let app = test_app();
        let req = Request::builder()
            .uri("/v1/webhooks")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["total"], 0);
    }

    // --- 3c. Audit tests ---

    #[tokio::test]
    async fn test_forward_audit_events() {
        let state = test_state();
        let app = build_router(state.clone());

        let req_body = serde_json::json!({
            "source": "agnostic-python",
            "correlation_id": "corr-123",
            "events": [
                {
                    "timestamp": "2026-03-06T12:00:00Z",
                    "action": "file.read",
                    "agent": "researcher",
                    "details": {"path": "/tmp/data.csv"},
                    "outcome": "success"
                },
                {
                    "timestamp": "2026-03-06T12:01:00Z",
                    "action": "llm.query",
                    "details": {},
                    "outcome": "success"
                }
            ]
        });

        let req = Request::builder()
            .method("POST")
            .uri("/v1/audit/forward")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["events_received"], 2);
    }

    #[tokio::test]
    async fn test_list_audit_events() {
        let state = test_state();
        let app = build_router(state.clone());

        // Forward some events
        let req_body = serde_json::json!({
            "source": "test",
            "events": [
                {"timestamp": "t1", "action": "read", "agent": "a1", "details": {}, "outcome": "ok"},
                {"timestamp": "t2", "action": "write", "agent": "a2", "details": {}, "outcome": "ok"},
                {"timestamp": "t3", "action": "read", "agent": "a1", "details": {}, "outcome": "fail"}
            ]
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/audit/forward")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        app.clone().oneshot(req).await.unwrap();

        // List all
        let req = Request::builder()
            .uri("/v1/audit")
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["total"], 3);

        // Filter by agent
        let req = Request::builder()
            .uri("/v1/audit?agent=a1")
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["total"], 2);

        // Filter by action
        let req = Request::builder()
            .uri("/v1/audit?action=write")
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["total"], 1);

        // Limit
        let req = Request::builder()
            .uri("/v1/audit?limit=1")
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["total"], 1);
    }

    #[tokio::test]
    async fn test_list_audit_empty() {
        let app = test_app();
        let req = Request::builder()
            .uri("/v1/audit")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["total"], 0);
    }

    #[tokio::test]
    async fn test_forward_audit_empty_events() {
        let app = test_app();
        let req_body = serde_json::json!({"source": "test", "events": []});
        let req = Request::builder()
            .method("POST")
            .uri("/v1/audit/forward")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["events_received"], 0);
    }

    #[test]
    fn test_audit_event_serialization() {
        let event = AuditEvent {
            timestamp: "2026-03-06T00:00:00Z".to_string(),
            action: "test".to_string(),
            agent: Some("agent-1".to_string()),
            details: serde_json::json!({"key": "value"}),
            outcome: "success".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        let deser: AuditEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.action, "test");
        assert_eq!(deser.agent, Some("agent-1".to_string()));
    }

    // --- 3d. Memory bridge tests ---

    #[tokio::test]
    async fn test_memory_set_and_get() {
        let state = test_state();
        let app = build_router(state.clone());
        let id = Uuid::new_v4();

        // Set
        let req_body = serde_json::json!({"value": {"greeting": "hello"}, "tags": ["test"]});
        let req = Request::builder()
            .method("PUT")
            .uri(format!("/v1/agents/{}/memory/mykey", id))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // Get
        let req = Request::builder()
            .uri(format!("/v1/agents/{}/memory/mykey", id))
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["key"], "mykey");
        assert_eq!(json["value"]["greeting"], "hello");
    }

    #[tokio::test]
    async fn test_memory_get_not_found() {
        let state = test_state();
        let app = build_router(state.clone());
        let id = Uuid::new_v4();

        let req = Request::builder()
            .uri(format!("/v1/agents/{}/memory/nonexistent", id))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_memory_list_keys() {
        let state = test_state();
        let app = build_router(state.clone());
        let id = Uuid::new_v4();

        // Set two keys
        for key in ["alpha", "beta"] {
            let req_body = serde_json::json!({"value": 1});
            let req = Request::builder()
                .method("PUT")
                .uri(format!("/v1/agents/{}/memory/{}", id, key))
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
                .unwrap();
            app.clone().oneshot(req).await.unwrap();
        }

        // List
        let req = Request::builder()
            .uri(format!("/v1/agents/{}/memory", id))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["total"], 2);
    }

    #[tokio::test]
    async fn test_memory_delete_key() {
        let state = test_state();
        let app = build_router(state.clone());
        let id = Uuid::new_v4();

        // Set
        let req_body = serde_json::json!({"value": "data"});
        let req = Request::builder()
            .method("PUT")
            .uri(format!("/v1/agents/{}/memory/delme", id))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        app.clone().oneshot(req).await.unwrap();

        // Delete
        let req = Request::builder()
            .method("DELETE")
            .uri(format!("/v1/agents/{}/memory/delme", id))
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // Verify gone
        let req = Request::builder()
            .uri(format!("/v1/agents/{}/memory/delme", id))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_memory_delete_not_found() {
        let state = test_state();
        let app = build_router(state.clone());
        let id = Uuid::new_v4();

        let req = Request::builder()
            .method("DELETE")
            .uri(format!("/v1/agents/{}/memory/ghost", id))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_memory_list_empty() {
        let state = test_state();
        let app = build_router(state.clone());
        let id = Uuid::new_v4();

        let req = Request::builder()
            .uri(format!("/v1/agents/{}/memory", id))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["total"], 0);
    }

    #[tokio::test]
    async fn test_memory_isolation_between_agents() {
        let state = test_state();
        let app = build_router(state.clone());
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        // Set same key for different agents
        let req_body = serde_json::json!({"value": "agent1-data"});
        let req = Request::builder()
            .method("PUT")
            .uri(format!("/v1/agents/{}/memory/shared", id1))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        app.clone().oneshot(req).await.unwrap();

        let req_body = serde_json::json!({"value": "agent2-data"});
        let req = Request::builder()
            .method("PUT")
            .uri(format!("/v1/agents/{}/memory/shared", id2))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        app.clone().oneshot(req).await.unwrap();

        // Verify isolation
        let req = Request::builder()
            .uri(format!("/v1/agents/{}/memory/shared", id1))
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["value"], "agent1-data");
    }

    // --- 3e. Traces tests ---

    #[tokio::test]
    async fn test_submit_trace() {
        let state = test_state();
        let app = build_router(state.clone());

        let req_body = serde_json::json!({
            "agent_id": "research-agent",
            "input": "What is AGNOS?",
            "steps": [
                {
                    "name": "search",
                    "rationale": "Need to find information",
                    "tool": "web_search",
                    "output": "Found docs",
                    "duration_ms": 150,
                    "success": true
                },
                {
                    "name": "summarize",
                    "rationale": "Condense results",
                    "duration_ms": 200,
                    "success": true
                }
            ],
            "result": "AGNOS is an AI-native operating system.",
            "duration_ms": 350
        });

        let req = Request::builder()
            .method("POST")
            .uri("/v1/traces")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "accepted");
    }

    #[tokio::test]
    async fn test_list_traces() {
        let state = test_state();
        let app = build_router(state.clone());

        // Submit two traces
        for agent in ["agent-a", "agent-b"] {
            let req_body = serde_json::json!({
                "agent_id": agent,
                "input": "test",
                "steps": [],
                "duration_ms": 100
            });
            let req = Request::builder()
                .method("POST")
                .uri("/v1/traces")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
                .unwrap();
            app.clone().oneshot(req).await.unwrap();
        }

        // List all
        let req = Request::builder()
            .uri("/v1/traces")
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["total"], 2);

        // Filter by agent_id
        let req = Request::builder()
            .uri("/v1/traces?agent_id=agent-a")
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["total"], 1);
    }

    #[tokio::test]
    async fn test_list_traces_empty() {
        let app = test_app();
        let req = Request::builder()
            .uri("/v1/traces")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["total"], 0);
    }

    #[test]
    fn test_trace_step_serialization() {
        let step = TraceStep {
            name: "analyze".to_string(),
            rationale: "need to check".to_string(),
            tool: Some("grep".to_string()),
            output: Some("found 5 matches".to_string()),
            duration_ms: 50,
            success: true,
        };
        let json = serde_json::to_string(&step).unwrap();
        let deser: TraceStep = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.name, "analyze");
        assert!(deser.success);
    }

    #[test]
    fn test_webhook_registration_serialization() {
        let wh = WebhookRegistration {
            id: Uuid::new_v4(),
            url: "https://example.com/hook".to_string(),
            events: vec!["test".to_string()],
            secret: Some("key".to_string()),
            created_at: Utc::now(),
        };
        let json = serde_json::to_string(&wh).unwrap();
        let deser: WebhookRegistration = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.url, "https://example.com/hook");
    }

    // --- Audit chain HTTP endpoint tests ---

    #[tokio::test]
    async fn test_audit_chain_empty() {
        let app = test_app();
        let req = Request::builder()
            .uri("/v1/audit/chain")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["total"], 0);
        assert_eq!(json["entries"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn test_audit_chain_verify_empty() {
        let app = test_app();
        let req = Request::builder()
            .uri("/v1/audit/chain/verify")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["valid"], true);
    }

    #[tokio::test]
    async fn test_audit_chain_populated_via_forward() {
        let state = test_state();
        let app = build_router(state.clone());

        // Forward two events
        let req_body = serde_json::json!({
            "source": "test",
            "events": [
                {"timestamp": "2026-03-06T12:00:00Z", "action": "read", "agent": "a1", "details": {}, "outcome": "success"},
                {"timestamp": "2026-03-06T12:01:00Z", "action": "write", "details": {}, "outcome": "success"}
            ]
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/audit/forward")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        app.clone().oneshot(req).await.unwrap();

        // Check chain has 2 entries
        let req = Request::builder()
            .uri("/v1/audit/chain")
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["total"], 2);
        let entries = json["entries"].as_array().unwrap();
        assert_eq!(entries.len(), 2);
        // Second entry's previous_hash should match first entry's entry_hash
        assert_eq!(entries[1]["previous_hash"], entries[0]["entry_hash"]);

        // Verify chain
        let req = Request::builder()
            .uri("/v1/audit/chain/verify")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["valid"], true);
        assert_eq!(json["entries"], 2);
    }

    #[tokio::test]
    async fn test_audit_chain_pagination() {
        let state = test_state();
        let app = build_router(state.clone());

        // Forward 5 events
        let events: Vec<serde_json::Value> = (0..5).map(|i| {
            serde_json::json!({
                "timestamp": format!("2026-03-06T12:0{}:00Z", i),
                "action": format!("action_{}", i),
                "details": {},
                "outcome": "success"
            })
        }).collect();
        let req_body = serde_json::json!({"source": "test", "events": events});
        let req = Request::builder()
            .method("POST")
            .uri("/v1/audit/forward")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        app.clone().oneshot(req).await.unwrap();

        // Page: offset=1, limit=2
        let req = Request::builder()
            .uri("/v1/audit/chain?offset=1&limit=2")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["total"], 5);
        assert_eq!(json["offset"], 1);
        assert_eq!(json["limit"], 2);
        assert_eq!(json["entries"].as_array().unwrap().len(), 2);
    }

    // -----------------------------------------------------------------------
    // Sandbox Profile Mapping Tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_sandbox_translate_basic() {
        let app = test_app();
        let body = serde_json::json!({
            "name": "test-profile",
            "filesystem": [
                {"path": "/tmp", "access": "readwrite"},
                {"path": "/etc", "access": "read"}
            ],
            "network_mode": "localhost",
            "blocked_syscalls": ["ptrace", "mount"]
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/sandbox/profiles")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["network_access"], "LocalhostOnly");
        assert_eq!(json["isolate_network"], true);
        let fs_rules = json["filesystem_rules"].as_array().unwrap();
        assert_eq!(fs_rules.len(), 2);
        assert_eq!(fs_rules[0]["access"], "ReadWrite");
        assert_eq!(fs_rules[1]["access"], "ReadOnly");
        let seccomp = json["seccomp_rules"].as_array().unwrap();
        assert_eq!(seccomp.len(), 2);
        assert_eq!(seccomp[0]["syscall"], "ptrace");
        assert_eq!(seccomp[0]["action"], "Deny");
    }

    #[tokio::test]
    async fn test_sandbox_translate_empty_name() {
        let app = test_app();
        let body = serde_json::json!({
            "name": "",
            "network_mode": "none"
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/sandbox/profiles")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_sandbox_translate_path_traversal() {
        let app = test_app();
        let body = serde_json::json!({
            "name": "evil",
            "filesystem": [{"path": "/tmp/../etc/shadow", "access": "read"}]
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/sandbox/profiles")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["error"].as_str().unwrap().contains("traversal"));
    }

    #[tokio::test]
    async fn test_sandbox_translate_invalid_access() {
        let app = test_app();
        let body = serde_json::json!({
            "name": "bad-access",
            "filesystem": [{"path": "/tmp", "access": "execute"}]
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/sandbox/profiles")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_sandbox_translate_invalid_network_mode() {
        let app = test_app();
        let body = serde_json::json!({
            "name": "bad-net",
            "network_mode": "bridged"
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/sandbox/profiles")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_sandbox_translate_unknown_syscall() {
        let app = test_app();
        let body = serde_json::json!({
            "name": "bad-syscall",
            "blocked_syscalls": ["read", "totally_fake_syscall"]
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/sandbox/profiles")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["error"].as_str().unwrap().contains("totally_fake_syscall"));
    }

    #[tokio::test]
    async fn test_sandbox_translate_restricted_with_policy() {
        let app = test_app();
        let body = serde_json::json!({
            "name": "restricted-profile",
            "network_mode": "restricted",
            "allowed_hosts": ["api.example.com"],
            "allowed_ports": [443, 8080]
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/sandbox/profiles")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["network_access"], "Restricted");
        let policy = &json["network_policy"];
        assert!(policy.is_object());
        assert_eq!(policy["allowed_outbound_hosts"][0], "api.example.com");
        assert_eq!(policy["allowed_outbound_ports"][0], 443);
        assert_eq!(policy["allowed_outbound_ports"][1], 8080);
    }

    #[tokio::test]
    async fn test_sandbox_default_profile() {
        let app = test_app();
        let req = Request::builder()
            .uri("/v1/sandbox/profiles/default")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["network_access"], "LocalhostOnly");
        assert_eq!(json["isolate_network"], true);
        let fs = json["filesystem_rules"].as_array().unwrap();
        assert_eq!(fs.len(), 1);
        assert_eq!(fs[0]["path"], "/tmp");
        assert_eq!(fs[0]["access"], "ReadWrite");
    }

    #[tokio::test]
    async fn test_sandbox_validate_valid_config() {
        let app = test_app();
        let config = serde_json::json!({
            "filesystem_rules": [{"path": "/tmp", "access": "ReadWrite"}],
            "network_access": "LocalhostOnly",
            "seccomp_rules": [{"syscall": "ptrace", "action": "Deny"}],
            "isolate_network": true,
            "network_policy": null,
            "mac_profile": null,
            "encrypted_storage": null
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/sandbox/profiles/validate")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&config).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["valid"], true);
        assert!(json["errors"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_sandbox_validate_path_traversal_and_unknown_syscall() {
        let app = test_app();
        let config = serde_json::json!({
            "filesystem_rules": [
                {"path": "/tmp/../etc/shadow", "access": "ReadOnly"},
                {"path": "relative/path", "access": "ReadWrite"}
            ],
            "network_access": "Restricted",
            "seccomp_rules": [{"syscall": "bogus_call", "action": "Deny"}],
            "isolate_network": true,
            "network_policy": null,
            "mac_profile": null,
            "encrypted_storage": null
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/sandbox/profiles/validate")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&config).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["valid"], false);
        let errors = json["errors"].as_array().unwrap();
        assert!(errors.iter().any(|e| e.as_str().unwrap().contains("traversal")));
        assert!(errors.iter().any(|e| e.as_str().unwrap().contains("bogus_call")));
        let warnings = json["warnings"].as_array().unwrap();
        assert!(warnings.iter().any(|e| e.as_str().unwrap().contains("Relative path")));
        assert!(warnings.iter().any(|e| e.as_str().unwrap().contains("no network_policy")));
    }

    #[tokio::test]
    async fn test_sandbox_validate_inconsistent_network() {
        let app = test_app();
        let config = serde_json::json!({
            "filesystem_rules": [],
            "network_access": "Full",
            "seccomp_rules": [],
            "isolate_network": true,
            "network_policy": {
                "allowed_outbound_ports": [80],
                "allowed_outbound_hosts": [],
                "allowed_inbound_ports": [],
                "enable_nat": true
            },
            "mac_profile": null,
            "encrypted_storage": null
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/sandbox/profiles/validate")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&config).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["valid"], true);
        let warnings = json["warnings"].as_array().unwrap();
        assert!(warnings.iter().any(|w| w.as_str().unwrap().contains("not Restricted")));
        assert!(warnings.iter().any(|w| w.as_str().unwrap().contains("Full network access")));
    }

    #[tokio::test]
    async fn test_sandbox_translate_full_network_no_isolation() {
        let app = test_app();
        let body = serde_json::json!({
            "name": "full-net",
            "network_mode": "full",
            "isolate_network": false
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/sandbox/profiles")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["network_access"], "Full");
        assert_eq!(json["isolate_network"], false);
        assert!(json["network_policy"].is_null());
    }
}
