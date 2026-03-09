//! Agent Registration HTTP API
//!
//! Axum HTTP server on port 8090 providing REST endpoints for external
//! consumers (AGNOSTIC, SecureYeoman) to register agents, send heartbeats,
//! and query agent status.

pub mod handlers;
pub mod middleware;
pub mod state;
pub mod types;

#[cfg(test)]
mod tests;

use std::net::SocketAddr;

use axum::middleware as axum_mw;
use axum::routing::{delete, get, post, put};
use axum::Router;
use tracing::{info, warn};

pub use state::*;
pub use types::*;

// Re-export handler-specific public types that were public in the original module
pub use handlers::anomaly::BehaviorSampleRequest;
pub use handlers::audit::{
    AuditChainQueryParams, AuditEvent, AuditForwardRequest, AuditQueryParams,
};
pub use handlers::dashboard::{DashboardSyncRequest, StoredDashboardSnapshot};
pub use handlers::marketplace::{MarketplaceInstallRequest, MarketplaceSearchQuery};
pub use handlers::memory::MemorySetRequest;
pub use handlers::profiles::EnvironmentProfile;
pub use handlers::reasoning::{
    ReasoningQueryParams, ReasoningStep, ReasoningTrace, StoredReasoningTrace,
};
pub use handlers::rpc::{RpcCallRequest, RpcRegisterRequest};
pub use handlers::traces::{TraceQueryParams, TraceStep, TraceSubmitRequest};
pub use handlers::screen_capture::{
    FramesQuery, GrantPermissionRequest, ScreenCaptureRequest, ScreenCaptureResponse,
    StartRecordingRequest,
};
pub use handlers::webhooks::{RegisterWebhookRequest, WebhookRegistration};

/// Default listen port for the agent registration API.
pub const DEFAULT_PORT: u16 = 8090;

/// Maximum number of trace entries kept in memory.
pub const MAX_TRACES: usize = 10_000;
/// Maximum number of audit events kept in memory.
pub const MAX_AUDIT_BUFFER: usize = 100_000;
/// Maximum number of webhook registrations allowed.
pub const MAX_WEBHOOKS: usize = 1_000;

// ---------------------------------------------------------------------------
// Router & server
// ---------------------------------------------------------------------------

/// Build the Axum router for the agent registration API.
pub fn build_router(state: ApiState) -> Router {
    use handlers::agents::*;
    use handlers::anomaly::*;
    use handlers::ark::*;
    use handlers::audit::*;
    use handlers::database::*;
    use handlers::dashboard::*;
    use handlers::marketplace::*;
    use handlers::memory::*;
    use handlers::profiles::*;
    use handlers::rag::*;
    use handlers::reasoning::*;
    use handlers::rpc::*;
    use handlers::sandbox::*;
    use handlers::screen_capture::*;
    use handlers::system_update::*;
    use handlers::traces::*;
    use handlers::vectors::*;
    use handlers::webhooks::*;

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
        // Reasoning trace routes
        .route("/v1/agents/:id/reasoning", post(submit_reasoning_handler))
        .route("/v1/agents/:id/reasoning", get(list_reasoning_handler))
        // Dashboard sync routes
        .route("/v1/dashboard/sync", post(dashboard_sync_handler))
        .route("/v1/dashboard/latest", get(dashboard_latest_handler))
        // Environment profile routes
        .route("/v1/profiles", get(list_profiles_handler))
        .route("/v1/profiles/:name", get(get_profile_handler))
        .route("/v1/profiles/:name", put(upsert_profile_handler))
        // Vector search routes
        .route("/v1/vectors/search", post(vector_search_handler))
        .route("/v1/vectors/insert", post(vector_insert_handler))
        .route("/v1/vectors/collections", get(vector_collections_handler))
        .route("/v1/vectors/collections", post(create_collection_handler))
        .route(
            "/v1/vectors/collections/:name",
            delete(delete_collection_handler),
        )
        .route("/v1/agents/:id/memory", get(memory_list_handler))
        .route("/v1/agents/:id/memory/:key", get(memory_get_handler))
        .route("/v1/agents/:id/memory/:key", put(memory_set_handler))
        .route("/v1/agents/:id/memory/:key", delete(memory_delete_handler))
        .route("/v1/traces", post(submit_trace_handler))
        .route("/v1/traces", get(list_traces_handler))
        .route("/v1/traces/spans", get(list_spans_handler))
        .route("/v1/traces/otlp-config", get(otlp_config_handler))
        .route("/v1/mcp/tools", get(crate::mcp_server::mcp_tools_handler))
        .route(
            "/v1/mcp/tools/call",
            post(crate::mcp_server::mcp_tool_call_handler),
        )
        .route(
            "/v1/sandbox/profiles",
            post(translate_sandbox_profile_handler),
        )
        .route(
            "/v1/sandbox/profiles/default",
            get(default_sandbox_profile_handler),
        )
        .route(
            "/v1/sandbox/profiles/validate",
            post(validate_sandbox_profile_handler),
        )
        // Agent-to-agent RPC routes
        .route("/v1/rpc/methods", get(rpc_list_methods_handler))
        .route("/v1/rpc/methods/:agent_id", get(rpc_agent_methods_handler))
        .route("/v1/rpc/register", post(rpc_register_handler))
        .route("/v1/rpc/call", post(rpc_call_handler))
        // Behavior anomaly detection routes
        .route("/v1/anomaly/sample", post(anomaly_submit_handler))
        .route("/v1/anomaly/alerts", get(anomaly_alerts_handler))
        .route(
            "/v1/anomaly/baseline/:agent_id",
            get(anomaly_baseline_handler),
        )
        .route(
            "/v1/anomaly/alerts/:agent_id",
            delete(anomaly_clear_handler),
        )
        // RAG pipeline routes
        .route("/v1/rag/ingest", post(rag_ingest_handler))
        .route("/v1/rag/query", post(rag_query_handler))
        .route("/v1/rag/stats", get(rag_stats_handler))
        // Knowledge base routes
        .route("/v1/knowledge/search", post(knowledge_search_handler))
        .route("/v1/knowledge/stats", get(knowledge_stats_handler))
        .route("/v1/knowledge/index", post(knowledge_index_handler))
        // Ark unified package manager routes
        .route("/v1/ark/install", post(ark_install_handler))
        .route("/v1/ark/remove", post(ark_remove_handler))
        .route("/v1/ark/search", get(ark_search_handler))
        .route("/v1/ark/info/:package", get(ark_info_handler))
        .route("/v1/ark/update", post(ark_update_handler))
        .route("/v1/ark/upgrade", post(ark_upgrade_handler))
        .route("/v1/ark/status", get(ark_status_handler))
        // System update routes
        .route(
            "/v1/system/update/status",
            get(system_update_status_handler),
        )
        .route("/v1/system/update/check", post(system_update_check_handler))
        .route("/v1/system/update/apply", post(system_update_apply_handler))
        .route(
            "/v1/system/update/rollback",
            post(system_update_rollback_handler),
        )
        .route(
            "/v1/system/update/confirm",
            post(system_update_confirm_handler),
        )
        // Marketplace routes
        .route(
            "/v1/marketplace/installed",
            get(marketplace_installed_handler),
        )
        .route("/v1/marketplace/search", get(marketplace_search_handler))
        .route("/v1/marketplace/install", post(marketplace_install_handler))
        .route("/v1/marketplace/:name", get(marketplace_info_handler))
        .route(
            "/v1/marketplace/:name",
            delete(marketplace_uninstall_handler),
        )
        // Screen capture routes
        .route("/v1/screen/capture", post(screen_capture_handler))
        .route(
            "/v1/screen/permissions",
            post(screen_grant_permission_handler),
        )
        .route(
            "/v1/screen/permissions",
            get(screen_list_permissions_handler),
        )
        .route(
            "/v1/screen/permissions/:agent_id",
            delete(screen_revoke_permission_handler),
        )
        .route("/v1/screen/history", get(screen_history_handler))
        // Screen recording routes
        .route(
            "/v1/screen/recording/start",
            post(recording_start_handler),
        )
        .route(
            "/v1/screen/recording/:id/frame",
            post(recording_frame_handler),
        )
        .route(
            "/v1/screen/recording/:id/pause",
            post(recording_pause_handler),
        )
        .route(
            "/v1/screen/recording/:id/resume",
            post(recording_resume_handler),
        )
        .route(
            "/v1/screen/recording/:id/stop",
            post(recording_stop_handler),
        )
        .route(
            "/v1/screen/recording/:id",
            get(recording_get_handler),
        )
        .route(
            "/v1/screen/recording/:id/frames",
            get(recording_frames_handler),
        )
        .route(
            "/v1/screen/recording/:id/latest",
            get(recording_latest_handler),
        )
        .route("/v1/screen/recordings", get(recording_list_handler))
        // Database provisioning routes
        .route(
            "/v1/agents/:id/database",
            post(database_provision_handler),
        )
        .route(
            "/v1/agents/:id/database",
            delete(database_deprovision_handler),
        )
        .route(
            "/v1/agents/:id/database",
            get(database_get_handler),
        )
        .route("/v1/database/stats", get(database_stats_handler))
        .layer(axum_mw::from_fn_with_state(
            state.clone(),
            middleware::auth_middleware,
        ))
        .layer(axum::extract::DefaultBodyLimit::max(10 * 1024 * 1024)) // 10 MB
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
        .unwrap_or(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST));
    let addr = SocketAddr::new(bind_addr, port);
    info!("Agent Registration API listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
