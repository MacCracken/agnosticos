//! MCP (Model Context Protocol) Server Wrapper
//!
//! Exposes AGNOS agent runtime operations as MCP tools that external
//! services can discover and call. Wraps the existing REST API logic
//! from [`crate::http_api`] into the MCP tool-call format.

pub mod types;
pub mod helpers;
pub mod manifest;
pub(crate) mod handlers;

#[cfg(test)]
mod tests;

// Re-export all public types so callers can still use `crate::mcp_server::X`.
pub use types::{
    ExternalMcpTool, McpContentBlock, McpToolCall, McpToolDescription, McpToolManifest,
    McpToolParam, McpToolResult, RegisterMcpToolRequest,
};
pub use manifest::build_tool_manifest;
pub use handlers::photis::PhotisBridge;
pub use handlers::aequi::AequiBridge;
pub use handlers::agnostic::AgnosticBridge;
pub use handlers::delta::DeltaBridge;

use axum::extract::State;
use axum::response::IntoResponse;
use axum::Json;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::http_api::ApiState;

use helpers::error_result;
use handlers::agnos::*;
use handlers::aequi::*;
use handlers::agnostic::*;
use handlers::delta::*;
use handlers::edge::*;
use handlers::photis::*;

// ---------------------------------------------------------------------------
// HTTP Handlers
// ---------------------------------------------------------------------------

/// `GET /v1/mcp/tools` — return the MCP tool manifest (built-in + external).
pub async fn mcp_tools_handler(State(state): State<ApiState>) -> impl IntoResponse {
    let mut manifest = build_tool_manifest();
    let external = state.external_mcp_tools.read().await;
    for ext in external.iter() {
        manifest.tools.push(ext.tool.clone());
    }
    Json(manifest)
}

/// `POST /v1/mcp/tools` — register an external MCP tool.
pub async fn mcp_register_tool_handler(
    State(state): State<ApiState>,
    Json(req): Json<RegisterMcpToolRequest>,
) -> impl IntoResponse {
    if req.name.is_empty() {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Tool name is required", "code": 400})),
        )
            .into_response();
    }

    if req.callback_url.is_empty() {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "callback_url is required", "code": 400})),
        )
            .into_response();
    }

    // Prevent SSRF: reject private IPs, non-http(s) schemes, credentials,
    // and localhost targets before storing the callback URL.
    if let Err(reason) = crate::http_api::types::validate_url_no_ssrf(&req.callback_url) {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": format!("Invalid callback_url: {reason}"),
                "code": 400
            })),
        )
            .into_response();
    }

    // Reject names that collide with built-in tools
    let manifest = build_tool_manifest();
    if manifest.tools.iter().any(|t| t.name == req.name) {
        return (
            axum::http::StatusCode::CONFLICT,
            Json(serde_json::json!({"error": format!("Tool '{}' conflicts with a built-in tool", req.name), "code": 409})),
        )
            .into_response();
    }

    let mut external = state.external_mcp_tools.write().await;

    // Replace if already registered with same name
    external.retain(|t| t.tool.name != req.name);

    let source = req.source.unwrap_or_else(|| "external".to_string());
    let tool = McpToolDescription {
        name: req.name.clone(),
        description: req.description,
        input_schema: req.input_schema,
    };

    external.push(ExternalMcpTool {
        tool,
        callback_url: req.callback_url.clone(),
        source: source.clone(),
    });

    info!(tool = %req.name, source = %source, "External MCP tool registered");

    (
        axum::http::StatusCode::CREATED,
        Json(serde_json::json!({
            "name": req.name,
            "callback_url": req.callback_url,
            "source": source,
            "status": "registered",
        })),
    )
        .into_response()
}

/// `DELETE /v1/mcp/tools/:name` — deregister an external MCP tool.
pub async fn mcp_deregister_tool_handler(
    State(state): State<ApiState>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> impl IntoResponse {
    let mut external = state.external_mcp_tools.write().await;
    let before = external.len();
    external.retain(|t| t.tool.name != name);
    if external.len() < before {
        info!(tool = %name, "External MCP tool deregistered");
        (
            axum::http::StatusCode::OK,
            Json(serde_json::json!({"status": "deregistered", "name": name})),
        )
            .into_response()
    } else {
        (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": format!("External tool '{}' not found", name), "code": 404})),
        )
            .into_response()
    }
}

/// `POST /v1/mcp/tools/call` — dispatch an MCP tool call to the appropriate logic.
///
/// H27/H29: All responses use the standard MCP envelope (`McpToolResult` with
/// `content` + `isError`). A per-request UUID is generated for log correlation.
pub async fn mcp_tool_call_handler(
    State(state): State<ApiState>,
    Json(call): Json<McpToolCall>,
) -> impl IntoResponse {
    let request_id = Uuid::new_v4();
    info!(request_id = %request_id, tool = %call.name, "MCP tool call received");
    debug!(request_id = %request_id, tool = %call.name, arguments = %call.arguments, "MCP tool call details");

    let result = dispatch_tool_call(&state, &call, request_id).await;

    if result.is_error {
        debug!(request_id = %request_id, tool = %call.name, "MCP tool call completed with error");
    } else {
        debug!(request_id = %request_id, tool = %call.name, "MCP tool call completed successfully");
    }

    Json(result)
}

// ---------------------------------------------------------------------------
// Dispatch
// ---------------------------------------------------------------------------

async fn dispatch_tool_call(
    state: &ApiState,
    call: &McpToolCall,
    request_id: Uuid,
) -> McpToolResult {
    debug!(request_id = %request_id, tool = %call.name, "Dispatching MCP tool call");
    match call.name.as_str() {
        "agnos_health" => handle_health(state).await,
        "agnos_list_agents" => handle_list_agents(state).await,
        "agnos_get_agent" => handle_get_agent(state, &call.arguments).await,
        "agnos_register_agent" => handle_register_agent(state, &call.arguments).await,
        "agnos_deregister_agent" => handle_deregister_agent(state, &call.arguments).await,
        "agnos_heartbeat" => handle_heartbeat(state, &call.arguments).await,
        "agnos_get_metrics" => handle_get_metrics(state).await,
        "agnos_forward_audit" => handle_forward_audit(state, &call.arguments).await,
        "agnos_memory_get" => handle_memory_get(state, &call.arguments).await,
        "agnos_memory_set" => handle_memory_set(state, &call.arguments).await,
        "aequi_estimate_quarterly_tax" => handle_aequi_estimate_tax(&call.arguments).await,
        "aequi_schedule_c_preview" => handle_aequi_schedule_c(&call.arguments).await,
        "aequi_import_bank_statement" => handle_aequi_import_bank(&call.arguments).await,
        "aequi_account_balances" => handle_aequi_balances(&call.arguments).await,
        "aequi_list_receipts" => handle_aequi_receipts(&call.arguments).await,
        "agnostic_run_suite" => handle_agnostic_run_suite(&call.arguments).await,
        "agnostic_test_status" => handle_agnostic_test_status(&call.arguments).await,
        "agnostic_test_report" => handle_agnostic_test_report(&call.arguments).await,
        "agnostic_list_suites" => handle_agnostic_list_suites(&call.arguments).await,
        "agnostic_agent_status" => handle_agnostic_agent_status(&call.arguments).await,
        "delta_create_repository" => handle_delta_create_repository(&call.arguments).await,
        "delta_list_repositories" => handle_delta_list_repositories(&call.arguments).await,
        "delta_pull_request" => handle_delta_pull_request(&call.arguments).await,
        "delta_push" => handle_delta_push(&call.arguments).await,
        "delta_ci_status" => handle_delta_ci_status(&call.arguments).await,
        "photis_list_tasks" => handle_photis_list_tasks(&call.arguments).await,
        "photis_create_task" => handle_photis_create_task(&call.arguments).await,
        "photis_update_task" => handle_photis_update_task(&call.arguments).await,
        "photis_get_rituals" => handle_photis_get_rituals(&call.arguments).await,
        "photis_analytics" => handle_photis_analytics(&call.arguments).await,
        "photis_sync" => handle_photis_sync(&call.arguments).await,
        "edge_list" => handle_edge_list(state, &call.arguments).await,
        "edge_deploy" => handle_edge_deploy(state, &call.arguments).await,
        "edge_update" => handle_edge_update(state, &call.arguments).await,
        "edge_health" => handle_edge_health(state, &call.arguments).await,
        "edge_decommission" => handle_edge_decommission(state, &call.arguments).await,
        unknown => {
            // Check external tools
            let external = state.external_mcp_tools.read().await;
            if let Some(ext) = external.iter().find(|t| t.tool.name == unknown) {
                debug!(request_id = %request_id, tool = %unknown, callback = %ext.callback_url, "Forwarding to external MCP tool");
                dispatch_external_tool(ext, call).await
            } else {
                warn!(request_id = %request_id, tool = %unknown, "Unknown MCP tool called");
                error_result(format!("Unknown tool: {}", unknown))
            }
        }
    }
}

/// Shared HTTP client for external MCP tool calls — created once, reused for
/// all dispatches to avoid per-call TLS/connection-pool overhead.
static EXTERNAL_HTTP_CLIENT: once_cell::sync::Lazy<reqwest::Client> =
    once_cell::sync::Lazy::new(|| {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new())
    });

/// Proxy an MCP tool call to an externally registered callback URL.
async fn dispatch_external_tool(ext: &ExternalMcpTool, call: &McpToolCall) -> McpToolResult {
    // Defense-in-depth: re-validate the callback URL at dispatch time in case
    // it was stored before the SSRF check was added at registration.
    if let Err(reason) = crate::http_api::types::validate_url_no_ssrf(&ext.callback_url) {
        warn!(
            tool = %call.name,
            url = %ext.callback_url,
            reason = %reason,
            "Blocked SSRF attempt in external MCP tool callback"
        );
        return error_result(format!("Callback URL blocked by SSRF policy: {reason}"));
    }

    let client = &*EXTERNAL_HTTP_CLIENT;

    match client.post(&ext.callback_url).json(call).send().await {
        Ok(resp) if resp.status().is_success() => match resp.json::<McpToolResult>().await {
            Ok(result) => result,
            Err(e) => {
                warn!(tool = %call.name, error = %e, "Failed to parse external tool response");
                error_result(format!("External tool returned invalid response: {}", e))
            }
        },
        Ok(resp) => {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            warn!(tool = %call.name, status = %status, "External tool call failed");
            error_result(format!("External tool returned {}: {}", status, body))
        }
        Err(e) => {
            warn!(tool = %call.name, error = %e, "External tool call failed");
            error_result(format!("Failed to reach external tool: {}", e))
        }
    }
}
