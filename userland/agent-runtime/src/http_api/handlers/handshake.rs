//! Handshake endpoints for consumer service integration.
//!
//! Provides service discovery, batch agent registration, event streaming,
//! pub/sub HTTP API, and sandbox profile listing — designed for deep
//! integration with SecureYeoman and other consumer platforms.

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::IntoResponse;
use axum::Json;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;
use tracing::info;
use uuid::Uuid;

use crate::http_api::state::{ApiState, RegisteredAgentEntry};
use crate::http_api::types::*;
use crate::marketplace::sandbox_profiles::{
    build_photis_nadi_profile, build_profile_for_preset, SandboxPreset,
};

// ---------------------------------------------------------------------------
// Service discovery
// ---------------------------------------------------------------------------

/// GET /v1/discover — returns AGNOS service capabilities, versions, and
/// available API surface areas. Allows consumers like SecureYeoman to
/// auto-configure their integration at startup.
pub async fn service_discovery_handler(State(state): State<ApiState>) -> impl IntoResponse {
    let agents = state.agents_read().await;
    let uptime = (Utc::now() - state.started_at()).num_seconds().max(0) as u64;

    let capabilities = vec![
        "agent-registry",
        "mcp-tools",
        "audit-chain",
        "vector-store",
        "rag-pipeline",
        "knowledge-base",
        "rpc-registry",
        "anomaly-detection",
        "dashboard-sync",
        "environment-profiles",
        "screen-capture",
        "screen-recording",
        "marketplace",
        "pubsub-events",
        "batch-registration",
        "database-provisioning",
    ];

    let endpoints = serde_json::json!({
        "agents": "/v1/agents",
        "agents_register": "/v1/agents/register",
        "agents_register_batch": "/v1/agents/register/batch",
        "health": "/v1/health",
        "health_consumers": "/v1/health/consumers",
        "metrics": "/v1/metrics",
        "metrics_prometheus": "/v1/metrics/prometheus",
        "discover": "/v1/discover",
        "mcp_tools": "/v1/mcp/tools",
        "mcp_tools_call": "/v1/mcp/tools/call",
        "audit_forward": "/v1/audit/forward",
        "audit_query": "/v1/audit",
        "audit_chain_verify": "/v1/audit/chain/verify",
        "dashboard_sync": "/v1/dashboard/sync",
        "dashboard_latest": "/v1/dashboard/latest",
        "vectors_search": "/v1/vectors/search",
        "vectors_insert": "/v1/vectors/insert",
        "vectors_collections": "/v1/vectors/collections",
        "rag_ingest": "/v1/rag/ingest",
        "rag_query": "/v1/rag/query",
        "knowledge_search": "/v1/knowledge/search",
        "rpc_methods": "/v1/rpc/methods",
        "rpc_call": "/v1/rpc/call",
        "rpc_register": "/v1/rpc/register",
        "events_subscribe": "/v1/events/subscribe",
        "events_publish": "/v1/events/publish",
        "events_topics": "/v1/events/topics",
        "sandbox_profiles_list": "/v1/sandbox/profiles/list",
        "profiles": "/v1/profiles",
        "marketplace_installed": "/v1/marketplace/installed",
        "marketplace_search": "/v1/marketplace/search",
        "screen_capture": "/v1/screen/capture",
        "screen_recording": "/v1/screen/recording/start",
        "reasoning": "/v1/agents/:id/reasoning",
        "memory": "/v1/agents/:id/memory",
        "database": "/v1/agents/:id/database",
    });

    Json(serde_json::json!({
        "service": "agnos-agent-runtime",
        "codename": "daimon",
        "version": env!("CARGO_PKG_VERSION"),
        "protocol_version": "1.0",
        "uptime_seconds": uptime,
        "agents_registered": agents.len(),
        "capabilities": capabilities,
        "endpoints": endpoints,
        "auth": {
            "type": if state.api_key.is_some() { "bearer" } else { "none" },
            "header": "Authorization",
        },
        "companion_services": {
            "llm_gateway": {
                "codename": "hoosh",
                "default_url": "http://127.0.0.1:8088",
                "env_var": "AGNOS_GATEWAY_URL",
            },
            "agent_runtime": {
                "codename": "daimon",
                "default_url": "http://127.0.0.1:8090",
                "env_var": "AGNOS_RUNTIME_URL",
            },
        },
    }))
}

// ---------------------------------------------------------------------------
// Batch agent registration
// ---------------------------------------------------------------------------

/// Request to register multiple agents in a single call.
#[derive(Debug, Deserialize)]
pub struct BatchRegisterRequest {
    /// Source service identifier (e.g., "secureyeoman", "agnostic").
    pub source: String,
    /// Agents to register.
    pub agents: Vec<RegisterAgentRequest>,
}

/// Result of a single agent registration within a batch.
#[derive(Debug, Serialize)]
pub struct BatchRegisterResult {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Uuid>,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// POST /v1/agents/register/batch — register multiple agents in one call.
/// Designed for SecureYeoman to register its internal agent fleet on startup.
/// Skips duplicates (returns existing ID) instead of failing the whole batch.
pub async fn batch_register_handler(
    State(state): State<ApiState>,
    Json(req): Json<BatchRegisterRequest>,
) -> impl IntoResponse {
    if req.agents.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "No agents provided", "code": 400})),
        )
            .into_response();
    }

    if req.agents.len() > 100 {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Maximum 100 agents per batch", "code": 400})),
        )
            .into_response();
    }

    let mut agents = state.agents_write().await;
    let mut results = Vec::new();
    let now = Utc::now();

    for agent_req in &req.agents {
        if agent_req.name.is_empty() {
            results.push(BatchRegisterResult {
                name: String::new(),
                id: None,
                status: "error".to_string(),
                error: Some("Agent name is required".to_string()),
            });
            continue;
        }

        // Check for existing agent with same name — return existing ID (idempotent)
        if let Some((existing_id, _)) = agents
            .iter()
            .find(|(_, a)| a.detail.name == agent_req.name)
        {
            results.push(BatchRegisterResult {
                name: agent_req.name.clone(),
                id: Some(*existing_id),
                status: "already_registered".to_string(),
                error: None,
            });
            continue;
        }

        let id = Uuid::new_v4();
        let mut metadata = agent_req.metadata.clone();
        metadata.insert("source".to_string(), req.source.clone());

        let detail = AgentDetail {
            id,
            name: agent_req.name.clone(),
            status: "registered".to_string(),
            capabilities: agent_req.capabilities.clone(),
            resource_needs: agent_req.resource_needs.clone(),
            metadata,
            registered_at: now,
            last_heartbeat: None,
            current_task: None,
            cpu_percent: None,
            memory_mb: None,
        };

        agents.insert(id, RegisteredAgentEntry { detail });

        results.push(BatchRegisterResult {
            name: agent_req.name.clone(),
            id: Some(id),
            status: "registered".to_string(),
            error: None,
        });
    }

    let registered_count = results.iter().filter(|r| r.status == "registered").count();
    let existing_count = results
        .iter()
        .filter(|r| r.status == "already_registered")
        .count();

    info!(
        "Batch registration from '{}': {} new, {} existing, {} total",
        req.source,
        registered_count,
        existing_count,
        results.len()
    );

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "source": req.source,
            "registered": registered_count,
            "already_registered": existing_count,
            "errors": results.iter().filter(|r| r.status == "error").count(),
            "results": results,
        })),
    )
        .into_response()
}

// ---------------------------------------------------------------------------
// Event stream (SSE) — pub/sub over HTTP
// ---------------------------------------------------------------------------

/// Query parameters for event subscription.
#[derive(Debug, Deserialize)]
pub struct EventSubscribeQuery {
    /// Comma-separated list of topics to subscribe to (supports wildcards, e.g., "agent.*,task.completed").
    pub topics: String,
    /// Optional subscriber name for logging.
    #[serde(default)]
    pub subscriber: Option<String>,
}

/// GET /v1/events/subscribe?topics=agent.*,task.completed — SSE stream of pub/sub events.
/// SecureYeoman can subscribe to real-time AGNOS events without polling.
pub async fn events_subscribe_handler(
    State(state): State<ApiState>,
    Query(query): Query<EventSubscribeQuery>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, std::convert::Infallible>>> {
    let topics: Vec<String> = query
        .topics
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let subscriber_name = query
        .subscriber
        .unwrap_or_else(|| format!("sse-{}", &Uuid::new_v4().to_string()[..8]));

    info!(
        "SSE subscriber '{}' connecting for topics: {:?}",
        subscriber_name, topics
    );

    let (tx, rx) = tokio::sync::mpsc::channel(64);
    let agent_id = agnos_common::AgentId::new();

    // Register with the broker and subscribe to requested topics
    let broker = state.topic_broker.clone();
    {
        let broker = broker.read().await;
        broker.register(agent_id, tx).await;
        for topic in &topics {
            broker.subscribe(agent_id, topic).await;
        }
    }

    // Convert the mpsc receiver into an SSE stream
    let stream = ReceiverStream::new(rx).map(move |msg| {
        let data = serde_json::json!({
            "topic": msg.topic,
            "sender": msg.sender.to_string(),
            "payload": msg.payload,
            "correlation_id": msg.correlation_id,
            "timestamp": msg.timestamp,
        });
        Ok(Event::default()
            .event(&msg.topic)
            .data(data.to_string()))
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// Publish request for the HTTP pub/sub API.
#[derive(Debug, Deserialize)]
pub struct EventPublishRequest {
    /// Topic to publish to.
    pub topic: String,
    /// Sender identifier (agent name or service name).
    pub sender: String,
    /// JSON payload.
    pub payload: serde_json::Value,
    /// Optional correlation ID for request/reply patterns.
    #[serde(default)]
    pub correlation_id: Option<String>,
    /// Optional reply-to topic.
    #[serde(default)]
    pub reply_to: Option<String>,
}

/// POST /v1/events/publish — publish an event to the pub/sub broker.
/// Allows SecureYeoman to emit events that other AGNOS agents can subscribe to.
pub async fn events_publish_handler(
    State(state): State<ApiState>,
    Json(req): Json<EventPublishRequest>,
) -> impl IntoResponse {
    if req.topic.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Topic is required", "code": 400})),
        )
            .into_response();
    }

    let sender_id = agnos_common::AgentId::new();
    let mut msg = crate::pubsub::TopicMessage::new(req.topic.clone(), sender_id, req.payload);
    msg.correlation_id = req.correlation_id;
    msg.reply_to = req.reply_to;

    let broker = state.topic_broker.read().await;
    let delivered = broker.publish(msg).await;

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "topic": req.topic,
            "delivered_to": delivered,
        })),
    )
        .into_response()
}

/// GET /v1/events/topics — list all active pub/sub topics with subscriber counts.
pub async fn events_topics_handler(State(state): State<ApiState>) -> impl IntoResponse {
    let broker = state.topic_broker.read().await;
    let topics = broker.list_topics().await;
    let recent = broker.recent_messages(20).await;

    let topics_list: Vec<serde_json::Value> = topics
        .into_iter()
        .map(|(topic, count)| {
            serde_json::json!({
                "topic": topic,
                "subscribers": count,
            })
        })
        .collect();

    let recent_list: Vec<serde_json::Value> = recent
        .into_iter()
        .map(|msg| {
            serde_json::json!({
                "topic": msg.topic,
                "sender": msg.sender.to_string(),
                "timestamp": msg.timestamp,
                "correlation_id": msg.correlation_id,
            })
        })
        .collect();

    Json(serde_json::json!({
        "topics": topics_list,
        "total_topics": topics_list.len(),
        "recent_messages": recent_list,
    }))
}

// ---------------------------------------------------------------------------
// Sandbox profile listing
// ---------------------------------------------------------------------------

/// GET /v1/sandbox/profiles/list — list all predefined sandbox profiles.
/// Allows SecureYeoman to discover available sandbox presets.
pub async fn list_sandbox_profiles_handler() -> impl IntoResponse {
    let presets = [
        SandboxPreset::PhotoEditor,
        SandboxPreset::ProductivityApp,
        SandboxPreset::Browser,
        SandboxPreset::GameApp,
        SandboxPreset::CliTool,
        SandboxPreset::Custom,
    ];

    let mut profiles: Vec<serde_json::Value> = presets
        .iter()
        .map(|preset| {
            let profile =
                build_profile_for_preset(*preset, "example-app", "~/.local/share/example-app/");
            serde_json::json!({
                "preset": preset.to_string(),
                "seccomp_mode": profile.seccomp_mode,
                "network_enabled": profile.network.enabled,
                "max_memory_mb": profile.max_memory_mb,
                "allow_process_spawn": profile.allow_process_spawn,
                "landlock_rules_count": profile.landlock_rules.len(),
            })
        })
        .collect();

    // Add well-known app-specific profiles
    let photis_profile = build_photis_nadi_profile();
    profiles.push(serde_json::json!({
        "preset": "photis-nadi",
        "app_specific": true,
        "seccomp_mode": photis_profile.seccomp_mode,
        "network_enabled": photis_profile.network.enabled,
        "allowed_hosts": photis_profile.network.allowed_hosts,
        "max_memory_mb": photis_profile.max_memory_mb,
        "allow_process_spawn": photis_profile.allow_process_spawn,
        "landlock_rules_count": photis_profile.landlock_rules.len(),
    }));

    Json(serde_json::json!({
        "profiles": profiles,
        "total": profiles.len(),
    }))
}

// ---------------------------------------------------------------------------
// Correlation ID middleware types
// ---------------------------------------------------------------------------

/// Headers that consumers should send for distributed tracing.
#[derive(Debug, Serialize)]
pub struct TracingHeaders {
    /// W3C Trace Context traceparent header.
    pub traceparent: String,
    /// Optional correlation ID for cross-service request correlation.
    pub correlation_id: String,
}
