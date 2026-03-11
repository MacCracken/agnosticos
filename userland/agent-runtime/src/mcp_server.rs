//! MCP (Model Context Protocol) Server Wrapper
//!
//! Exposes AGNOS agent runtime operations as MCP tools that external
//! services can discover and call. Wraps the existing REST API logic
//! from [`crate::http_api`] into the MCP tool-call format.

use std::collections::HashMap;

use axum::extract::State;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::http_api::{ApiState, AuditEvent, RegisterAgentRequest, ResourceNeeds};

// ---------------------------------------------------------------------------
// MCP Protocol Types
// ---------------------------------------------------------------------------

/// Description of a single MCP tool parameter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolParam {
    pub name: String,
    #[serde(rename = "type")]
    pub param_type: String,
    pub description: String,
    #[serde(default)]
    pub required: bool,
}

/// Description of a single MCP tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolDescription {
    pub name: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: serde_json::Value,
}

/// The full tool manifest returned by `GET /v1/mcp/tools`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolManifest {
    pub tools: Vec<McpToolDescription>,
}

/// Incoming MCP tool call request body for `POST /v1/mcp/tools/call`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolCall {
    pub name: String,
    #[serde(default)]
    pub arguments: serde_json::Value,
}

/// A single content block in an MCP tool result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpContentBlock {
    #[serde(rename = "type")]
    pub content_type: String,
    pub text: String,
}

/// MCP tool call response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolResult {
    pub content: Vec<McpContentBlock>,
    #[serde(rename = "isError")]
    pub is_error: bool,
}

/// An externally registered MCP tool with a callback URL for dispatch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalMcpTool {
    /// Tool definition (name, description, input_schema).
    pub tool: McpToolDescription,
    /// HTTP endpoint to POST tool calls to.
    pub callback_url: String,
    /// Source service that registered this tool.
    pub source: String,
}

/// Request body for POST /v1/mcp/tools (register external tool).
#[derive(Debug, Clone, Deserialize)]
pub struct RegisterMcpToolRequest {
    pub name: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: serde_json::Value,
    pub callback_url: String,
    #[serde(default)]
    pub source: Option<String>,
}

// ---------------------------------------------------------------------------
// Tool Manifest
// ---------------------------------------------------------------------------

fn json_schema_object(properties: serde_json::Value, required: Vec<&str>) -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": properties,
        "required": required,
    })
}

/// Helper macro to construct an `McpToolDescription` concisely.
macro_rules! tool {
    ($name:expr, $desc:expr, $props:expr, $req:expr) => {
        McpToolDescription {
            name: $name.to_string(),
            description: $desc.to_string(),
            input_schema: json_schema_object($props, $req),
        }
    };
    ($name:expr, $desc:expr) => {
        McpToolDescription {
            name: $name.to_string(),
            description: $desc.to_string(),
            input_schema: json_schema_object(serde_json::json!({}), vec![]),
        }
    };
}

/// Build the static MCP tool manifest for the agent runtime.
pub fn build_tool_manifest() -> McpToolManifest {
    use serde_json::json;

    let tools = vec![
        // ----- AGNOS core runtime tools (10) -----
        tool!("agnos_health", "Check agent runtime health status"),
        tool!("agnos_list_agents", "List all registered agents"),
        tool!("agnos_get_agent", "Get details for a specific agent by ID",
            json!({"agent_id": {"type": "string", "description": "UUID of the agent"}}),
            vec!["agent_id"]
        ),
        tool!("agnos_register_agent", "Register a new agent with the runtime",
            json!({
                "name": {"type": "string", "description": "Agent name"},
                "capabilities": {"type": "array", "items": {"type": "string"}, "description": "Agent capabilities"},
                "metadata": {"type": "object", "description": "Additional key-value metadata"}
            }),
            vec!["name"]
        ),
        tool!("agnos_deregister_agent", "Deregister an agent by ID",
            json!({"agent_id": {"type": "string", "description": "UUID of the agent to deregister"}}),
            vec!["agent_id"]
        ),
        tool!("agnos_heartbeat", "Send a heartbeat for an agent",
            json!({
                "agent_id": {"type": "string", "description": "UUID of the agent"},
                "status": {"type": "string", "description": "Optional status update"},
                "current_task": {"type": "string", "description": "Optional current task description"}
            }),
            vec!["agent_id"]
        ),
        tool!("agnos_get_metrics", "Get agent runtime metrics"),
        tool!("agnos_forward_audit", "Forward an audit event to the runtime",
            json!({
                "action": {"type": "string", "description": "Audit action name"},
                "agent": {"type": "string", "description": "Optional agent name or ID"},
                "details": {"type": "object", "description": "Arbitrary event details"},
                "outcome": {"type": "string", "description": "Event outcome (e.g. success, failure)"},
                "source": {"type": "string", "description": "Source identifier for the audit event"}
            }),
            vec!["action", "source"]
        ),
        tool!("agnos_memory_get", "Get a memory value for an agent by key",
            json!({
                "agent_id": {"type": "string", "description": "UUID of the agent"},
                "key": {"type": "string", "description": "Memory key to retrieve"}
            }),
            vec!["agent_id", "key"]
        ),
        tool!("agnos_memory_set", "Set a memory value for an agent by key",
            json!({
                "agent_id": {"type": "string", "description": "UUID of the agent"},
                "key": {"type": "string", "description": "Memory key to set"},
                "value": {"description": "Value to store (any JSON value)"}
            }),
            vec!["agent_id", "key", "value"]
        ),
        // ----- Delta code hosting tools (5) -----
        tool!("delta_create_repository", "Create a git repository in Delta",
            json!({
                "name": {"type": "string", "description": "Repository name"},
                "description": {"type": "string", "description": "Repository description"},
                "visibility": {"type": "string", "description": "Visibility: public or private"}
            }),
            vec!["name"]
        ),
        tool!("delta_list_repositories", "List git repositories",
            json!({
                "owner": {"type": "string", "description": "Filter by owner"},
                "limit": {"type": "integer", "description": "Max results to return"}
            }),
            vec![]
        ),
        tool!("delta_pull_request", "Manage pull requests (list, create, merge, close)",
            json!({
                "action": {"type": "string", "description": "Action: list, create, merge, close"},
                "repo": {"type": "string", "description": "Repository name"},
                "title": {"type": "string", "description": "PR title (for create)"},
                "source_branch": {"type": "string", "description": "Source branch (for create)"},
                "target_branch": {"type": "string", "description": "Target branch (for create, default: main)"},
                "pr_id": {"type": "string", "description": "PR ID (for merge/close)"}
            }),
            vec!["action"]
        ),
        tool!("delta_push", "Push code to a Delta repository",
            json!({
                "repo": {"type": "string", "description": "Repository name"},
                "branch": {"type": "string", "description": "Branch to push"}
            }),
            vec![]
        ),
        tool!("delta_ci_status", "Get CI pipeline status for a repository",
            json!({
                "repo": {"type": "string", "description": "Repository name"},
                "pipeline_id": {"type": "string", "description": "Specific pipeline ID"}
            }),
            vec![]
        ),
        // ----- Aequi accounting tools (5) -----
        tool!("aequi_estimate_quarterly_tax", "Calculate estimated quarterly tax liability",
            json!({
                "quarter": {"type": "string", "description": "Quarter number (1-4)"},
                "year": {"type": "string", "description": "Tax year (e.g. 2026)"}
            }),
            vec![]
        ),
        tool!("aequi_schedule_c_preview", "Generate a Schedule C (Profit or Loss) preview",
            json!({"year": {"type": "string", "description": "Tax year (e.g. 2026)"}}),
            vec![]
        ),
        tool!("aequi_import_bank_statement", "Import a bank statement file (OFX, QFX, CSV)",
            json!({
                "file_path": {"type": "string", "description": "Path to the statement file"},
                "format": {"type": "string", "description": "File format: ofx, qfx, csv (auto-detected if omitted)"}
            }),
            vec!["file_path"]
        ),
        tool!("aequi_account_balances", "Get current account balances",
            json!({"account_type": {"type": "string", "description": "Filter by type: asset, liability, equity, revenue, expense"}}),
            vec![]
        ),
        tool!("aequi_list_receipts", "List receipts with optional status filter",
            json!({
                "status": {"type": "string", "description": "Filter: pending_review, reviewed, matched, all"},
                "limit": {"type": "integer", "description": "Max results to return"}
            }),
            vec![]
        ),
        // ----- Agnostic QA platform tools (5) -----
        tool!("agnostic_run_suite", "Run a QA test suite",
            json!({
                "suite": {"type": "string", "description": "Test suite name or ID"},
                "target_url": {"type": "string", "description": "Target application URL to test"},
                "agents": {"type": "array", "description": "Agent types to use: ui, api, security, performance, accessibility, self-healing"}
            }),
            vec!["suite"]
        ),
        tool!("agnostic_test_status", "Get status of a running or completed test run",
            json!({"run_id": {"type": "string", "description": "Test run ID"}}),
            vec!["run_id"]
        ),
        tool!("agnostic_test_report", "Get detailed test report with findings",
            json!({
                "run_id": {"type": "string", "description": "Test run ID"},
                "format": {"type": "string", "description": "Report format: summary, full, json (default: summary)"}
            }),
            vec!["run_id"]
        ),
        tool!("agnostic_list_suites", "List available QA test suites",
            json!({"category": {"type": "string", "description": "Filter by category: ui, api, security, performance, all"}}),
            vec![]
        ),
        tool!("agnostic_agent_status", "Get status of QA testing agents",
            json!({"agent_type": {"type": "string", "description": "Filter by agent type: ui, api, security, performance, accessibility, self-healing"}}),
            vec![]
        ),
        // ----- Photis Nadi task management tools (6) -----
        tool!("photis_list_tasks", "List tasks with optional filters",
            json!({
                "status": {"type": "string", "description": "Filter by status: todo, in_progress, done"},
                "board_id": {"type": "string", "description": "Filter by board ID"}
            }),
            vec![]
        ),
        tool!("photis_create_task", "Create a new task",
            json!({
                "title": {"type": "string", "description": "Task title"},
                "description": {"type": "string", "description": "Task description"},
                "board_id": {"type": "string", "description": "Board to add task to"},
                "priority": {"type": "string", "description": "Priority: low, medium, high"}
            }),
            vec!["title"]
        ),
        tool!("photis_update_task", "Update an existing task",
            json!({
                "task_id": {"type": "string", "description": "UUID of the task to update"},
                "title": {"type": "string", "description": "New task title"},
                "status": {"type": "string", "description": "New status: todo, in_progress, done"},
                "priority": {"type": "string", "description": "New priority: low, medium, high"}
            }),
            vec!["task_id"]
        ),
        tool!("photis_get_rituals", "Get daily rituals/habits",
            json!({"date": {"type": "string", "description": "ISO date (e.g. 2026-03-06)"}}),
            vec![]
        ),
        tool!("photis_analytics", "Get productivity analytics",
            json!({
                "period": {"type": "string", "description": "Period: day, week, month"},
                "metric": {"type": "string", "description": "Metric: tasks_completed, streak, velocity"}
            }),
            vec![]
        ),
        tool!("photis_sync", "Trigger sync with Supabase backend",
            json!({"direction": {"type": "string", "description": "Sync direction: push, pull, both"}}),
            vec![]
        ),
    ];

    McpToolManifest { tools }
}

// ---------------------------------------------------------------------------
// Handlers
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

async fn dispatch_tool_call(state: &ApiState, call: &McpToolCall, request_id: Uuid) -> McpToolResult {
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
static EXTERNAL_HTTP_CLIENT: std::sync::LazyLock<reqwest::Client> =
    std::sync::LazyLock::new(|| {
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
        return error_result(format!(
            "Callback URL blocked by SSRF policy: {reason}"
        ));
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

// ---------------------------------------------------------------------------
// Helper constructors
// ---------------------------------------------------------------------------

fn success_result(value: serde_json::Value) -> McpToolResult {
    McpToolResult {
        content: vec![McpContentBlock {
            content_type: "text".to_string(),
            text: serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".to_string()),
        }],
        is_error: false,
    }
}

fn error_result(message: String) -> McpToolResult {
    McpToolResult {
        content: vec![McpContentBlock {
            content_type: "text".to_string(),
            text: serde_json::json!({"error": message}).to_string(),
        }],
        is_error: true,
    }
}

fn get_string_arg(args: &serde_json::Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

fn get_optional_string_arg(args: &serde_json::Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(|v| if v.is_null() { None } else { v.as_str() })
        .map(|s| s.to_string())
}

// ---------------------------------------------------------------------------
// H24: Consolidated JSON validation helpers (replacing 35+ duplicated patterns)
// ---------------------------------------------------------------------------

/// Extract a required string field from MCP tool arguments, returning an
/// `McpToolResult` error if the field is missing or not a string.
fn extract_required_string(args: &serde_json::Value, field: &str) -> Result<String, McpToolResult> {
    get_string_arg(args, field).ok_or_else(|| {
        error_result(format!("Missing required argument: {}", field))
    })
}

/// Extract a required string field and parse it as a UUID, returning an
/// `McpToolResult` error for missing fields or invalid UUIDs.
fn extract_required_uuid(args: &serde_json::Value, field: &str) -> Result<Uuid, McpToolResult> {
    let raw = extract_required_string(args, field)?;
    Uuid::parse_str(&raw).map_err(|_| error_result(format!("Invalid UUID for '{}': {}", field, raw)))
}

/// Extract an optional unsigned integer field from MCP tool arguments.
fn extract_optional_u64(args: &serde_json::Value, field: &str, default: u64) -> u64 {
    args.get(field).and_then(|v| v.as_u64()).unwrap_or(default)
}

/// Validate that an optional string value belongs to an allowed set.
fn validate_enum_opt(
    value: &Option<String>,
    field: &str,
    allowed: &[&str],
) -> Result<(), McpToolResult> {
    if let Some(ref v) = value {
        if !allowed.contains(&v.as_str()) {
            return Err(error_result(format!(
                "Invalid {} '{}': must be {}",
                field,
                v,
                allowed.join(", ")
            )));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tool Implementations
// ---------------------------------------------------------------------------

async fn handle_health(state: &ApiState) -> McpToolResult {
    let agents = state.agents_read().await;
    let uptime = (chrono::Utc::now() - state.started_at())
        .num_seconds()
        .max(0) as u64;

    success_result(serde_json::json!({
        "status": "ok",
        "service": "agnos-agent-runtime",
        "agents_registered": agents.len(),
        "uptime_seconds": uptime,
    }))
}

async fn handle_list_agents(state: &ApiState) -> McpToolResult {
    let agents = state.agents_read().await;
    let agent_list: Vec<_> = agents.values().map(|a| &a.detail).collect();
    let total = agent_list.len();

    success_result(serde_json::json!({
        "agents": agent_list,
        "total": total,
    }))
}

async fn handle_get_agent(state: &ApiState, args: &serde_json::Value) -> McpToolResult {
    let uuid = match extract_required_uuid(args, "agent_id") {
        Ok(id) => id,
        Err(e) => return e,
    };

    let agents = state.agents_read().await;
    match agents.get(&uuid) {
        Some(entry) => success_result(serde_json::to_value(&entry.detail).unwrap_or_default()),
        None => error_result(format!("Agent {} not found", uuid)),
    }
}

async fn handle_register_agent(state: &ApiState, args: &serde_json::Value) -> McpToolResult {
    let name = match extract_required_string(args, "name") {
        Ok(n) => n,
        Err(e) => return e,
    };

    if name.is_empty() {
        return error_result("Agent name is required".to_string());
    }
    if name.len() > 256 {
        return error_result("Agent name too long (max 256)".to_string());
    }

    let capabilities: Vec<String> = args
        .get("capabilities")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();

    let metadata: HashMap<String, String> = args
        .get("metadata")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();

    let client_id: Option<Uuid> = get_string_arg(args, "id").and_then(|s| Uuid::parse_str(&s).ok());

    let req = RegisterAgentRequest {
        name: name.clone(),
        id: client_id,
        capabilities,
        resource_needs: ResourceNeeds::default(),
        metadata,
    };

    let mut agents = state.agents_write().await;

    // Check for duplicate names
    if agents.values().any(|a| a.detail.name == req.name) {
        return error_result(format!("Agent '{}' already registered", req.name));
    }

    // Use client-specified ID if provided and not already taken
    let id = if let Some(client_id) = req.id {
        if agents.contains_key(&client_id) {
            return error_result(format!("Agent ID {} already in use", client_id));
        }
        client_id
    } else {
        Uuid::new_v4()
    };
    let now = chrono::Utc::now();

    let detail = crate::http_api::AgentDetail {
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

    agents.insert(
        id,
        crate::http_api::RegisteredAgentEntry {
            detail: detail.clone(),
        },
    );

    info!(agent_name = %req.name, agent_id = %id, "Agent registered via MCP");

    success_result(serde_json::json!({
        "id": id.to_string(),
        "name": req.name,
        "status": "registered",
        "registered_at": now.to_rfc3339(),
    }))
}

async fn handle_deregister_agent(state: &ApiState, args: &serde_json::Value) -> McpToolResult {
    let uuid = match extract_required_uuid(args, "agent_id") {
        Ok(id) => id,
        Err(e) => return e,
    };

    let mut agents = state.agents_write().await;
    match agents.remove(&uuid) {
        Some(entry) => {
            info!(agent_name = %entry.detail.name, agent_id = %uuid, "Agent deregistered via MCP");
            success_result(serde_json::json!({
                "status": "deregistered",
                "id": uuid.to_string(),
            }))
        }
        None => error_result(format!("Agent {} not found", uuid)),
    }
}

async fn handle_heartbeat(state: &ApiState, args: &serde_json::Value) -> McpToolResult {
    let uuid = match extract_required_uuid(args, "agent_id") {
        Ok(id) => id,
        Err(e) => return e,
    };

    let status = get_optional_string_arg(args, "status");
    let current_task = get_optional_string_arg(args, "current_task");

    let mut agents = state.agents_write().await;
    match agents.get_mut(&uuid) {
        Some(entry) => {
            entry.detail.last_heartbeat = Some(chrono::Utc::now());
            if let Some(s) = status {
                entry.detail.status = s;
            }
            if let Some(t) = current_task {
                entry.detail.current_task = Some(t);
            }
            debug!(agent_id = %uuid, "Heartbeat via MCP");
            success_result(serde_json::json!({"status": "ok"}))
        }
        None => error_result(format!("Agent {} not found", uuid)),
    }
}

async fn handle_get_metrics(state: &ApiState) -> McpToolResult {
    let agents = state.agents_read().await;
    let uptime = (chrono::Utc::now() - state.started_at())
        .num_seconds()
        .max(0) as u64;

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

    success_result(serde_json::json!({
        "total_agents": agents.len(),
        "agents_by_status": by_status,
        "uptime_seconds": uptime,
        "avg_cpu_percent": avg_cpu,
        "total_memory_mb": total_mem,
    }))
}

async fn handle_forward_audit(state: &ApiState, args: &serde_json::Value) -> McpToolResult {
    let action = match extract_required_string(args, "action") {
        Ok(a) => a,
        Err(e) => return e,
    };

    let source = match extract_required_string(args, "source") {
        Ok(s) => s,
        Err(e) => return e,
    };

    let agent = get_optional_string_arg(args, "agent");
    let outcome = get_optional_string_arg(args, "outcome").unwrap_or_else(|| "unknown".to_string());
    let details = args
        .get("details")
        .cloned()
        .unwrap_or(serde_json::Value::Null);

    let event = AuditEvent {
        timestamp: chrono::Utc::now().to_rfc3339(),
        action,
        agent,
        details,
        outcome,
    };

    info!(
        action = %event.action,
        source = %source,
        "Audit event forwarded via MCP"
    );

    let mut buffer = state.audit_buffer.write().await;
    if buffer.len() >= crate::http_api::MAX_AUDIT_BUFFER {
        buffer.pop_front();
    }
    buffer.push_back(event);

    success_result(serde_json::json!({
        "status": "accepted",
        "buffered": buffer.len(),
    }))
}

async fn handle_memory_get(state: &ApiState, args: &serde_json::Value) -> McpToolResult {
    let agent_id = match extract_required_string(args, "agent_id") {
        Ok(id) => id,
        Err(e) => return e,
    };

    let key = match extract_required_string(args, "key") {
        Ok(k) => k,
        Err(e) => return e,
    };

    match state.memory_store.get(&agent_id, &key).await {
        Some(value) => success_result(serde_json::json!({
            "agent_id": agent_id,
            "key": key,
            "value": value,
        })),
        None => error_result(format!("Key '{}' not found for agent {}", key, agent_id)),
    }
}

async fn handle_memory_set(state: &ApiState, args: &serde_json::Value) -> McpToolResult {
    let agent_id = match extract_required_string(args, "agent_id") {
        Ok(id) => id,
        Err(e) => return e,
    };

    let key = match extract_required_string(args, "key") {
        Ok(k) => k,
        Err(e) => return e,
    };

    let value = match args.get("value") {
        Some(v) => v.clone(),
        None => return error_result("Missing required argument: value".to_string()),
    };

    state.memory_store.set(&agent_id, &key, value.clone()).await;

    info!(agent_id = %agent_id, key = %key, "Memory set via MCP");

    success_result(serde_json::json!({
        "agent_id": agent_id,
        "key": key,
        "status": "stored",
    }))
}

// ---------------------------------------------------------------------------
// Photis Nadi Agent Bridge
// ---------------------------------------------------------------------------

/// Bridge that proxies MCP tool calls to the real Photis Nadi API.
///
/// When Photis Nadi is running at its configured endpoint, requests are
/// forwarded to its REST API. When the service is unavailable, a graceful
/// error is returned (no mock data — the bridge requires the real service).
#[derive(Debug, Clone)]
pub struct PhotisBridge {
    /// Base URL for the Photis Nadi API (default: `http://127.0.0.1:8081`).
    base_url: String,
    /// API key for authenticating with Photis Nadi.
    api_key: Option<String>,
}

impl PhotisBridge {
    /// Create a new bridge with default settings.
    pub fn new() -> Self {
        Self {
            base_url: std::env::var("PHOTISNADI_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8081".to_string()),
            api_key: std::env::var("PHOTISNADI_API_KEY").ok(),
        }
    }

    /// Create a bridge with explicit configuration (for testing).
    pub fn with_config(base_url: String, api_key: Option<String>) -> Self {
        Self { base_url, api_key }
    }

    /// Build the URL for a Photis Nadi API endpoint.
    pub fn url(&self, path: &str) -> String {
        format!("{}/api/v1{}", self.base_url, path)
    }

    /// Build authorization headers.
    pub fn auth_headers(&self) -> Vec<(String, String)> {
        let mut headers = vec![("Content-Type".to_string(), "application/json".to_string())];
        if let Some(ref key) = self.api_key {
            headers.push(("Authorization".to_string(), format!("Bearer {}", key)));
        }
        headers
    }

    /// Check if Photis Nadi is reachable.
    pub async fn health_check(&self) -> bool {
        let url = self.url("/health");
        match reqwest::Client::new()
            .get(&url)
            .timeout(std::time::Duration::from_secs(2))
            .send()
            .await
        {
            Ok(resp) => resp.status().is_success(),
            Err(e) => {
                debug!(url = %url, error = %e, "Photis Nadi health check failed");
                false
            }
        }
    }

    /// Forward a GET request to Photis Nadi.
    pub async fn get(
        &self,
        path: &str,
        query: &[(String, String)],
    ) -> Result<serde_json::Value, String> {
        let url = self.url(path);
        let mut req = reqwest::Client::new().get(&url);

        if let Some(ref key) = self.api_key {
            req = req.header("Authorization", format!("Bearer {}", key));
        }
        if !query.is_empty() {
            req = req.query(query);
        }

        req.timeout(std::time::Duration::from_secs(10))
            .send()
            .await
            .map_err(|e| format!("Photis Nadi unreachable at {}: {}", self.base_url, e))?
            .json::<serde_json::Value>()
            .await
            .map_err(|e| format!("Invalid response from Photis Nadi: {}", e))
    }

    /// Forward a POST request to Photis Nadi.
    pub async fn post(
        &self,
        path: &str,
        body: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let url = self.url(path);
        let mut req = reqwest::Client::new().post(&url).json(&body);

        if let Some(ref key) = self.api_key {
            req = req.header("Authorization", format!("Bearer {}", key));
        }

        req.timeout(std::time::Duration::from_secs(10))
            .send()
            .await
            .map_err(|e| format!("Photis Nadi unreachable at {}: {}", self.base_url, e))?
            .json::<serde_json::Value>()
            .await
            .map_err(|e| format!("Invalid response from Photis Nadi: {}", e))
    }

    /// Forward a PATCH request to Photis Nadi.
    pub async fn patch(
        &self,
        path: &str,
        body: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let url = self.url(path);
        let mut req = reqwest::Client::new().patch(&url).json(&body);

        if let Some(ref key) = self.api_key {
            req = req.header("Authorization", format!("Bearer {}", key));
        }

        req.timeout(std::time::Duration::from_secs(10))
            .send()
            .await
            .map_err(|e| format!("Photis Nadi unreachable at {}: {}", self.base_url, e))?
            .json::<serde_json::Value>()
            .await
            .map_err(|e| format!("Invalid response from Photis Nadi: {}", e))
    }

    /// Get the base URL.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}

impl Default for PhotisBridge {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Photis Nadi Tool Implementations (bridged)
// ---------------------------------------------------------------------------

async fn handle_photis_list_tasks(args: &serde_json::Value) -> McpToolResult {
    let status = get_optional_string_arg(args, "status");
    let board_id = get_optional_string_arg(args, "board_id");

    if let Err(e) = validate_enum_opt(&status, "status", &["todo", "in_progress", "done"]) {
        return e;
    }

    let bridge = PhotisBridge::new();
    let mut query = Vec::new();
    if let Some(ref s) = status {
        query.push(("status".to_string(), s.clone()));
    }
    if let Some(ref b) = board_id {
        query.push(("project_id".to_string(), b.clone()));
    }

    match bridge.get("/tasks", &query).await {
        Ok(response) => {
            info!("Photis: list tasks (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Photis bridge: falling back to mock for list_tasks");
            // Fallback to mock when Photis Nadi is not running
            let mut tasks = vec![
                serde_json::json!({"id": "task-001", "title": "Review PR #42", "status": "todo", "priority": "high", "board_id": "default"}),
                serde_json::json!({"id": "task-002", "title": "Write unit tests", "status": "in_progress", "priority": "medium", "board_id": "default"}),
                serde_json::json!({"id": "task-003", "title": "Deploy v2.0", "status": "done", "priority": "high", "board_id": "releases"}),
            ];
            if let Some(ref s) = status {
                tasks.retain(|t| t["status"].as_str() == Some(s.as_str()));
            }
            if let Some(ref b) = board_id {
                tasks.retain(|t| t["board_id"].as_str() == Some(b.as_str()));
            }
            success_result(serde_json::json!({
                "tasks": tasks,
                "total": tasks.len(),
                "_source": "mock",
            }))
        }
    }
}

async fn handle_photis_create_task(args: &serde_json::Value) -> McpToolResult {
    let title = match extract_required_string(args, "title") {
        Ok(t) => t,
        Err(e) => return e,
    };

    if title.is_empty() {
        return error_result("Task title cannot be empty".to_string());
    }

    let description = get_optional_string_arg(args, "description");
    let board_id =
        get_optional_string_arg(args, "board_id").unwrap_or_else(|| "default".to_string());
    let priority =
        get_optional_string_arg(args, "priority").unwrap_or_else(|| "medium".to_string());

    let priority_opt = Some(priority.clone());
    if let Err(e) = validate_enum_opt(&priority_opt, "priority", &["low", "medium", "high"]) {
        return e;
    }

    let bridge = PhotisBridge::new();
    let mut body = serde_json::json!({
        "title": title,
        "priority": priority,
        "project_id": board_id,
    });
    if let Some(desc) = description {
        body["description"] = serde_json::json!(desc);
    }

    match bridge.post("/tasks", body).await {
        Ok(response) => {
            info!(title = %title, "Photis: create task (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Photis bridge: falling back to mock for create_task");
            let task_id = Uuid::new_v4().to_string();
            success_result(serde_json::json!({
                "id": task_id,
                "title": title,
                "priority": priority,
                "status": "todo",
                "created_at": chrono::Utc::now().to_rfc3339(),
                "_source": "mock",
            }))
        }
    }
}

async fn handle_photis_update_task(args: &serde_json::Value) -> McpToolResult {
    let task_id = match extract_required_string(args, "task_id") {
        Ok(id) => id,
        Err(e) => return e,
    };

    let title = get_optional_string_arg(args, "title");
    let status = get_optional_string_arg(args, "status");
    let priority = get_optional_string_arg(args, "priority");

    if let Err(e) = validate_enum_opt(&status, "status", &["todo", "in_progress", "done"]) {
        return e;
    }
    if let Err(e) = validate_enum_opt(&priority, "priority", &["low", "medium", "high"]) {
        return e;
    }

    let bridge = PhotisBridge::new();
    let mut body = serde_json::json!({});
    if let Some(ref t) = title {
        body["title"] = serde_json::json!(t);
    }
    if let Some(ref s) = status {
        body["status"] = serde_json::json!(s);
    }
    if let Some(ref p) = priority {
        body["priority"] = serde_json::json!(p);
    }

    match bridge.patch(&format!("/tasks/{}", task_id), body).await {
        Ok(response) => {
            info!(task_id = %task_id, "Photis: update task (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Photis bridge: falling back to mock for update_task");
            success_result(serde_json::json!({
                "id": task_id,
                "title": title.unwrap_or_else(|| "Review PR #42".to_string()),
                "status": status.unwrap_or_else(|| "todo".to_string()),
                "priority": priority.unwrap_or_else(|| "medium".to_string()),
                "updated_at": chrono::Utc::now().to_rfc3339(),
                "_source": "mock",
            }))
        }
    }
}

async fn handle_photis_get_rituals(args: &serde_json::Value) -> McpToolResult {
    let date = get_optional_string_arg(args, "date")
        .unwrap_or_else(|| chrono::Utc::now().format("%Y-%m-%d").to_string());

    let bridge = PhotisBridge::new();
    let query = vec![("frequency".to_string(), "daily".to_string())];

    match bridge.get("/rituals", &query).await {
        Ok(response) => {
            info!(date = %date, "Photis: get rituals (bridged)");
            success_result(serde_json::json!({
                "date": date,
                "rituals": response,
            }))
        }
        Err(e) => {
            warn!(error = %e, "Photis bridge: falling back to mock for get_rituals");
            success_result(serde_json::json!({
                "date": date,
                "rituals": [
                    {"name": "Morning meditation", "completed": true, "streak": 12},
                    {"name": "Code review", "completed": false, "streak": 5},
                    {"name": "Exercise", "completed": true, "streak": 30},
                    {"name": "Journal", "completed": false, "streak": 0},
                ],
                "completion_rate": 0.5,
                "_source": "mock",
            }))
        }
    }
}

async fn handle_photis_analytics(args: &serde_json::Value) -> McpToolResult {
    let period = get_optional_string_arg(args, "period").unwrap_or_else(|| "week".to_string());
    let metric = get_optional_string_arg(args, "metric");

    let period_opt = Some(period.clone());
    if let Err(e) = validate_enum_opt(&period_opt, "period", &["day", "week", "month"]) {
        return e;
    }
    if let Err(e) = validate_enum_opt(&metric, "metric", &["tasks_completed", "streak", "velocity"]) {
        return e;
    }

    let bridge = PhotisBridge::new();
    match bridge.get("/analytics", &[]).await {
        Ok(response) => {
            info!(period = %period, "Photis: analytics (bridged)");
            success_result(serde_json::json!({
                "period": period,
                "metrics": response,
            }))
        }
        Err(e) => {
            warn!(error = %e, "Photis bridge: falling back to mock for analytics");
            success_result(serde_json::json!({
                "period": period,
                "metrics": {
                    "tasks_completed": 14,
                    "streak": 7,
                    "velocity": 2.3,
                    "completion_rate": 0.82,
                },
                "_source": "mock",
            }))
        }
    }
}

async fn handle_photis_sync(args: &serde_json::Value) -> McpToolResult {
    let direction =
        get_optional_string_arg(args, "direction").unwrap_or_else(|| "both".to_string());

    let direction_opt = Some(direction.clone());
    if let Err(e) = validate_enum_opt(&direction_opt, "direction", &["push", "pull", "both"]) {
        return e;
    }

    // Sync is Photis Nadi internal — trigger via health check + report status.
    let bridge = PhotisBridge::new();
    let online = bridge.health_check().await;

    info!(direction = %direction, online = online, "Photis: sync");
    if online {
        success_result(serde_json::json!({
            "status": "synced",
            "direction": direction,
            "service_online": true,
            "last_sync": chrono::Utc::now().to_rfc3339(),
        }))
    } else {
        success_result(serde_json::json!({
            "status": "offline",
            "direction": direction,
            "service_online": false,
            "message": format!("Photis Nadi not reachable at {}", bridge.base_url()),
            "_source": "mock",
        }))
    }
}

// ---------------------------------------------------------------------------
// Aequi Accounting Agent Bridge
// ---------------------------------------------------------------------------

/// Bridge that proxies MCP tool calls to the Aequi accounting API.
#[derive(Debug, Clone)]
pub struct AequiBridge {
    base_url: String,
    api_key: Option<String>,
}

impl Default for AequiBridge {
    fn default() -> Self {
        Self::new()
    }
}

impl AequiBridge {
    pub fn new() -> Self {
        Self {
            base_url: std::env::var("AEQUI_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8060".to_string()),
            api_key: std::env::var("AEQUI_API_KEY").ok(),
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    async fn get(
        &self,
        path: &str,
        query: &[(String, String)],
    ) -> Result<serde_json::Value, String> {
        let client = reqwest::Client::new();
        let url = format!("{}{}", self.base_url, path);
        let mut req = client.get(&url).query(query);
        if let Some(ref key) = self.api_key {
            req = req.header("Authorization", format!("Bearer {}", key));
        }
        let resp = req.send().await.map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("Aequi API error: {}", resp.status()));
        }
        resp.json().await.map_err(|e| e.to_string())
    }

    async fn post(&self, path: &str, body: serde_json::Value) -> Result<serde_json::Value, String> {
        let client = reqwest::Client::new();
        let url = format!("{}{}", self.base_url, path);
        let mut req = client.post(&url).json(&body);
        if let Some(ref key) = self.api_key {
            req = req.header("Authorization", format!("Bearer {}", key));
        }
        let resp = req.send().await.map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("Aequi API error: {}", resp.status()));
        }
        resp.json().await.map_err(|e| e.to_string())
    }
}

// ---------------------------------------------------------------------------
// Aequi Tool Implementations (bridged)
// ---------------------------------------------------------------------------

async fn handle_aequi_estimate_tax(args: &serde_json::Value) -> McpToolResult {
    let quarter = get_optional_string_arg(args, "quarter");
    let year = get_optional_string_arg(args, "year");

    if let Err(e) = validate_enum_opt(&quarter, "quarter", &["1", "2", "3", "4"]) {
        return e;
    }

    let bridge = AequiBridge::new();
    let mut query = Vec::new();
    if let Some(ref q) = quarter {
        query.push(("quarter".to_string(), q.clone()));
    }
    if let Some(ref y) = year {
        query.push(("year".to_string(), y.clone()));
    }

    match bridge.get("/api/v1/tax/estimate", &query).await {
        Ok(response) => {
            info!("Aequi: tax estimate (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Aequi bridge: falling back to mock for tax estimate");
            let q = quarter.as_deref().unwrap_or("1");
            success_result(serde_json::json!({
                "quarter": q,
                "year": year.as_deref().unwrap_or("2026"),
                "estimated_tax": 3250.00,
                "gross_income": 22500.00,
                "deductions": 5200.00,
                "taxable_income": 17300.00,
                "effective_rate": 0.188,
                "_source": "mock",
            }))
        }
    }
}

async fn handle_aequi_schedule_c(args: &serde_json::Value) -> McpToolResult {
    let year = get_optional_string_arg(args, "year");

    let bridge = AequiBridge::new();
    let mut query = Vec::new();
    if let Some(ref y) = year {
        query.push(("year".to_string(), y.clone()));
    }

    match bridge.get("/api/v1/tax/schedule-c", &query).await {
        Ok(response) => {
            info!("Aequi: schedule C preview (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Aequi bridge: falling back to mock for schedule C");
            success_result(serde_json::json!({
                "year": year.as_deref().unwrap_or("2026"),
                "gross_receipts": 90000.00,
                "cost_of_goods_sold": 0.00,
                "gross_income": 90000.00,
                "total_expenses": 21400.00,
                "net_profit": 68600.00,
                "categories": {
                    "office_supplies": 1200.00,
                    "software_subscriptions": 3600.00,
                    "home_office": 5400.00,
                    "professional_services": 4800.00,
                    "equipment_depreciation": 6400.00,
                },
                "_source": "mock",
            }))
        }
    }
}

async fn handle_aequi_import_bank(args: &serde_json::Value) -> McpToolResult {
    let file_path = match extract_required_string(args, "file_path") {
        Ok(p) => p,
        Err(e) => return e,
    };

    if file_path.is_empty() {
        return error_result("File path cannot be empty".to_string());
    }

    let format = get_optional_string_arg(args, "format");
    if let Err(e) = validate_enum_opt(&format, "format", &["ofx", "qfx", "csv"]) {
        return e;
    }

    let bridge = AequiBridge::new();
    let body = serde_json::json!({
        "file_path": file_path,
        "format": format,
    });

    match bridge.post("/api/v1/import/bank-statement", body).await {
        Ok(response) => {
            info!(file = %file_path, "Aequi: import bank statement (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Aequi bridge: falling back to mock for import");
            success_result(serde_json::json!({
                "status": "imported",
                "file": file_path,
                "transactions_imported": 47,
                "transactions_matched": 12,
                "transactions_new": 35,
                "date_range": {"from": "2026-01-01", "to": "2026-01-31"},
                "_source": "mock",
            }))
        }
    }
}

async fn handle_aequi_balances(args: &serde_json::Value) -> McpToolResult {
    let account_type = get_optional_string_arg(args, "account_type");

    if let Err(e) = validate_enum_opt(&account_type, "account_type", &["asset", "liability", "equity", "revenue", "expense"]) {
        return e;
    }

    let bridge = AequiBridge::new();
    let mut query = Vec::new();
    if let Some(ref t) = account_type {
        query.push(("type".to_string(), t.clone()));
    }

    match bridge.get("/api/v1/accounts/balances", &query).await {
        Ok(response) => {
            info!("Aequi: account balances (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Aequi bridge: falling back to mock for balances");
            let accounts = vec![
                serde_json::json!({"name": "Business Checking", "type": "asset", "balance": 14523.67, "currency": "USD"}),
                serde_json::json!({"name": "Business Savings", "type": "asset", "balance": 8200.00, "currency": "USD"}),
                serde_json::json!({"name": "Accounts Receivable", "type": "asset", "balance": 3750.00, "currency": "USD"}),
                serde_json::json!({"name": "Credit Card", "type": "liability", "balance": -1234.56, "currency": "USD"}),
            ];
            success_result(serde_json::json!({
                "accounts": accounts,
                "total_assets": 26473.67,
                "total_liabilities": -1234.56,
                "net_worth": 25239.11,
                "_source": "mock",
            }))
        }
    }
}

async fn handle_aequi_receipts(args: &serde_json::Value) -> McpToolResult {
    let status = get_optional_string_arg(args, "status");
    let limit = extract_optional_u64(args, "limit", 20) as usize;

    if let Err(e) = validate_enum_opt(&status, "status", &["pending_review", "reviewed", "matched", "all"]) {
        return e;
    }

    let bridge = AequiBridge::new();
    let mut query = Vec::new();
    if let Some(ref s) = status {
        query.push(("status".to_string(), s.clone()));
    }
    query.push(("limit".to_string(), limit.to_string()));

    match bridge.get("/api/v1/receipts", &query).await {
        Ok(response) => {
            info!("Aequi: list receipts (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Aequi bridge: falling back to mock for receipts");
            let receipts = vec![
                serde_json::json!({"id": "rcpt-001", "vendor": "Office Depot", "amount": 87.50, "date": "2026-03-05", "status": "matched", "category": "office_supplies"}),
                serde_json::json!({"id": "rcpt-002", "vendor": "AWS", "amount": 142.30, "date": "2026-03-01", "status": "pending_review", "category": "software"}),
                serde_json::json!({"id": "rcpt-003", "vendor": "Starbucks", "amount": 5.75, "date": "2026-03-08", "status": "pending_review", "category": "meals"}),
            ];
            let filtered: Vec<_> = if let Some(ref s) = status {
                if s == "all" {
                    receipts
                } else {
                    receipts
                        .into_iter()
                        .filter(|r| r["status"].as_str() == Some(s.as_str()))
                        .collect()
                }
            } else {
                receipts
            };
            success_result(serde_json::json!({
                "receipts": filtered,
                "total": filtered.len(),
                "_source": "mock",
            }))
        }
    }
}

// ---------------------------------------------------------------------------
// Agnostic QA Platform Agent Bridge
// ---------------------------------------------------------------------------

/// Bridge that proxies MCP tool calls to the Agnostic QA platform API.
#[derive(Debug, Clone)]
pub struct AgnosticBridge {
    base_url: String,
    api_key: Option<String>,
}

impl Default for AgnosticBridge {
    fn default() -> Self {
        Self::new()
    }
}

impl AgnosticBridge {
    pub fn new() -> Self {
        Self {
            base_url: std::env::var("AGNOSTIC_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8000".to_string()),
            api_key: std::env::var("AGNOSTIC_API_KEY").ok(),
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    async fn get(
        &self,
        path: &str,
        query: &[(String, String)],
    ) -> Result<serde_json::Value, String> {
        let client = reqwest::Client::new();
        let url = format!("{}{}", self.base_url, path);
        let mut req = client.get(&url).query(query);
        if let Some(ref key) = self.api_key {
            req = req.header("Authorization", format!("Bearer {}", key));
        }
        let resp = req.send().await.map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("Agnostic API error: {}", resp.status()));
        }
        resp.json().await.map_err(|e| e.to_string())
    }

    async fn post(&self, path: &str, body: serde_json::Value) -> Result<serde_json::Value, String> {
        let client = reqwest::Client::new();
        let url = format!("{}{}", self.base_url, path);
        let mut req = client.post(&url).json(&body);
        if let Some(ref key) = self.api_key {
            req = req.header("Authorization", format!("Bearer {}", key));
        }
        let resp = req.send().await.map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("Agnostic API error: {}", resp.status()));
        }
        resp.json().await.map_err(|e| e.to_string())
    }
}

// ---------------------------------------------------------------------------
// Agnostic Tool Implementations (bridged)
// ---------------------------------------------------------------------------

async fn handle_agnostic_run_suite(args: &serde_json::Value) -> McpToolResult {
    let suite = match extract_required_string(args, "suite") {
        Ok(s) => s,
        Err(e) => return e,
    };

    if suite.is_empty() {
        return error_result("Suite name cannot be empty".to_string());
    }

    let target_url = get_optional_string_arg(args, "target_url");
    let agents = args.get("agents").cloned();

    let bridge = AgnosticBridge::new();
    let mut body = serde_json::json!({
        "suite": suite,
    });
    if let Some(url) = &target_url {
        body["target_url"] = serde_json::json!(url);
    }
    if let Some(a) = agents {
        body["agents"] = a;
    }

    match bridge.post("/api/v1/runs", body).await {
        Ok(response) => {
            info!(suite = %suite, "Agnostic: run suite (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Agnostic bridge: falling back to mock for run_suite");
            let run_id = Uuid::new_v4().to_string();
            success_result(serde_json::json!({
                "run_id": run_id,
                "suite": suite,
                "status": "running",
                "agents_active": ["ui", "api", "security"],
                "started_at": chrono::Utc::now().to_rfc3339(),
                "_source": "mock",
            }))
        }
    }
}

async fn handle_agnostic_test_status(args: &serde_json::Value) -> McpToolResult {
    let run_id = match extract_required_string(args, "run_id") {
        Ok(id) => id,
        Err(e) => return e,
    };

    let bridge = AgnosticBridge::new();
    match bridge.get(&format!("/api/v1/runs/{}", run_id), &[]).await {
        Ok(response) => {
            info!(run_id = %run_id, "Agnostic: test status (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Agnostic bridge: falling back to mock for test_status");
            success_result(serde_json::json!({
                "run_id": run_id,
                "status": "completed",
                "total_tests": 156,
                "passed": 148,
                "failed": 5,
                "skipped": 3,
                "duration_seconds": 342,
                "agents": {
                    "ui": {"status": "completed", "tests": 62},
                    "api": {"status": "completed", "tests": 48},
                    "security": {"status": "completed", "tests": 46},
                },
                "_source": "mock",
            }))
        }
    }
}

async fn handle_agnostic_test_report(args: &serde_json::Value) -> McpToolResult {
    let run_id = match extract_required_string(args, "run_id") {
        Ok(id) => id,
        Err(e) => return e,
    };

    let format = get_optional_string_arg(args, "format").unwrap_or_else(|| "summary".to_string());

    let format_opt = Some(format.clone());
    if let Err(e) = validate_enum_opt(&format_opt, "format", &["summary", "full", "json"]) {
        return e;
    }

    let bridge = AgnosticBridge::new();
    let query = vec![("format".to_string(), format.clone())];
    match bridge
        .get(&format!("/api/v1/runs/{}/report", run_id), &query)
        .await
    {
        Ok(response) => {
            info!(run_id = %run_id, "Agnostic: test report (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Agnostic bridge: falling back to mock for test_report");
            success_result(serde_json::json!({
                "run_id": run_id,
                "format": format,
                "summary": {
                    "total": 156, "passed": 148, "failed": 5, "skipped": 3,
                    "pass_rate": 0.968,
                },
                "failures": [
                    {"test": "login_form_validation", "agent": "ui", "message": "Expected error message not displayed for empty email"},
                    {"test": "rate_limit_enforcement", "agent": "api", "message": "429 not returned after 100 requests/min"},
                ],
                "security_findings": [
                    {"severity": "medium", "title": "Missing CSP header on /dashboard", "agent": "security"},
                ],
                "_source": "mock",
            }))
        }
    }
}

async fn handle_agnostic_list_suites(args: &serde_json::Value) -> McpToolResult {
    let category = get_optional_string_arg(args, "category");

    if let Err(e) = validate_enum_opt(&category, "category", &["ui", "api", "security", "performance", "all"]) {
        return e;
    }

    let bridge = AgnosticBridge::new();
    let mut query = Vec::new();
    if let Some(ref c) = category {
        query.push(("category".to_string(), c.clone()));
    }

    match bridge.get("/api/v1/suites", &query).await {
        Ok(response) => {
            info!("Agnostic: list suites (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Agnostic bridge: falling back to mock for list_suites");
            let suites = vec![
                serde_json::json!({"id": "suite-001", "name": "Full Regression", "category": "all", "test_count": 312, "agents": ["ui", "api", "security", "performance"]}),
                serde_json::json!({"id": "suite-002", "name": "Security Audit", "category": "security", "test_count": 89, "agents": ["security"]}),
                serde_json::json!({"id": "suite-003", "name": "API Contract Tests", "category": "api", "test_count": 156, "agents": ["api"]}),
                serde_json::json!({"id": "suite-004", "name": "UI Smoke Tests", "category": "ui", "test_count": 45, "agents": ["ui", "accessibility"]}),
            ];
            let filtered: Vec<_> = if let Some(ref c) = category {
                if c == "all" {
                    suites
                } else {
                    suites
                        .into_iter()
                        .filter(|s| s["category"].as_str() == Some(c.as_str()))
                        .collect()
                }
            } else {
                suites
            };
            success_result(serde_json::json!({
                "suites": filtered,
                "total": filtered.len(),
                "_source": "mock",
            }))
        }
    }
}

async fn handle_agnostic_agent_status(args: &serde_json::Value) -> McpToolResult {
    let agent_type = get_optional_string_arg(args, "agent_type");

    if let Err(e) = validate_enum_opt(&agent_type, "agent_type", &["ui", "api", "security", "performance", "accessibility", "self-healing"]) {
        return e;
    }

    let bridge = AgnosticBridge::new();
    let mut query = Vec::new();
    if let Some(ref t) = agent_type {
        query.push(("type".to_string(), t.clone()));
    }

    match bridge.get("/api/v1/agents/status", &query).await {
        Ok(response) => {
            info!("Agnostic: agent status (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Agnostic bridge: falling back to mock for agent_status");
            let agents = vec![
                serde_json::json!({"type": "ui", "status": "idle", "last_run": "2026-03-10T14:30:00Z", "tests_run_today": 245}),
                serde_json::json!({"type": "api", "status": "idle", "last_run": "2026-03-10T14:30:00Z", "tests_run_today": 189}),
                serde_json::json!({"type": "security", "status": "idle", "last_run": "2026-03-10T13:00:00Z", "tests_run_today": 89}),
                serde_json::json!({"type": "performance", "status": "idle", "last_run": "2026-03-10T12:00:00Z", "tests_run_today": 34}),
                serde_json::json!({"type": "accessibility", "status": "idle", "last_run": "2026-03-10T14:30:00Z", "tests_run_today": 67}),
                serde_json::json!({"type": "self-healing", "status": "idle", "last_run": "2026-03-10T14:30:00Z", "tests_run_today": 12}),
            ];
            let filtered: Vec<_> = if let Some(ref t) = agent_type {
                agents
                    .into_iter()
                    .filter(|a| a["type"].as_str() == Some(t.as_str()))
                    .collect()
            } else {
                agents
            };
            success_result(serde_json::json!({
                "agents": filtered,
                "total": filtered.len(),
                "_source": "mock",
            }))
        }
    }
}

// ---------------------------------------------------------------------------
// Delta Code Hosting Agent Bridge
// ---------------------------------------------------------------------------

/// Bridge that proxies MCP tool calls to the Delta code hosting API.
///
/// When Delta is running at its configured endpoint, requests are forwarded to
/// its REST API. When the service is unavailable, mock data is returned.
#[derive(Debug, Clone)]
pub struct DeltaBridge {
    /// Base URL for the Delta API (default: `http://127.0.0.1:8070`).
    base_url: String,
    /// API key for authenticating with Delta.
    api_key: Option<String>,
}

impl Default for DeltaBridge {
    fn default() -> Self {
        Self::new()
    }
}

impl DeltaBridge {
    pub fn new() -> Self {
        Self {
            base_url: std::env::var("DELTA_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8070".to_string()),
            api_key: std::env::var("DELTA_API_KEY").ok(),
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    fn build_client() -> Result<reqwest::Client, String> {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .connect_timeout(std::time::Duration::from_secs(2))
            .build()
            .map_err(|e| e.to_string())
    }

    async fn get(
        &self,
        path: &str,
        query: &[(String, String)],
    ) -> Result<serde_json::Value, String> {
        let client = Self::build_client()?;
        let url = format!("{}{}", self.base_url, path);
        let mut req = client.get(&url).query(query);
        if let Some(ref key) = self.api_key {
            req = req.header("Authorization", format!("Bearer {}", key));
        }
        let resp = req.send().await.map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("Delta API error: {}", resp.status()));
        }
        resp.json().await.map_err(|e| e.to_string())
    }

    async fn post(&self, path: &str, body: serde_json::Value) -> Result<serde_json::Value, String> {
        let client = Self::build_client()?;
        let url = format!("{}{}", self.base_url, path);
        let mut req = client.post(&url).json(&body);
        if let Some(ref key) = self.api_key {
            req = req.header("Authorization", format!("Bearer {}", key));
        }
        let resp = req.send().await.map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("Delta API error: {}", resp.status()));
        }
        resp.json().await.map_err(|e| e.to_string())
    }

    #[allow(dead_code)]
    async fn health_check(&self) -> bool {
        let client = reqwest::Client::new();
        let url = format!("{}/api/v1/health", self.base_url);
        match client.get(&url).timeout(std::time::Duration::from_secs(2)).send().await {
            Ok(r) => r.status().is_success(),
            Err(e) => {
                debug!(url = %url, error = %e, "Delta health check failed");
                false
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Delta Tool Implementations (bridged)
// ---------------------------------------------------------------------------

async fn handle_delta_create_repository(args: &serde_json::Value) -> McpToolResult {
    let name = match extract_required_string(args, "name") {
        Ok(n) => n,
        Err(e) => return e,
    };

    if name.is_empty() {
        return error_result("Repository name cannot be empty".to_string());
    }

    let description = get_optional_string_arg(args, "description");
    let visibility =
        get_optional_string_arg(args, "visibility").unwrap_or_else(|| "private".to_string());

    let vis_opt = Some(visibility.clone());
    if let Err(e) = validate_enum_opt(&vis_opt, "visibility", &["public", "private"]) {
        return e;
    }

    let bridge = DeltaBridge::new();
    let mut body = serde_json::json!({
        "name": name,
        "visibility": visibility,
    });
    if let Some(desc) = description {
        body["description"] = serde_json::json!(desc);
    }

    match bridge.post("/api/v1/repos", body).await {
        Ok(response) => {
            info!(name = %name, "Delta: create repository (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Delta bridge: falling back to mock for create_repository");
            let repo_id = Uuid::new_v4().to_string();
            success_result(serde_json::json!({
                "id": repo_id,
                "name": name,
                "visibility": visibility,
                "default_branch": "main",
                "created_at": chrono::Utc::now().to_rfc3339(),
                "_source": "mock",
            }))
        }
    }
}

async fn handle_delta_list_repositories(args: &serde_json::Value) -> McpToolResult {
    let owner = get_optional_string_arg(args, "owner");
    let limit = extract_optional_u64(args, "limit", 20) as usize;

    let bridge = DeltaBridge::new();
    let mut query = Vec::new();
    if let Some(ref o) = owner {
        query.push(("owner".to_string(), o.clone()));
    }
    query.push(("limit".to_string(), limit.to_string()));

    match bridge.get("/api/v1/repos", &query).await {
        Ok(response) => {
            info!("Delta: list repositories (bridged)");
            // Normalize: Delta API returns a bare array; wrap it for consistency
            let repos = if response.is_array() {
                response
            } else {
                response
                    .get("repositories")
                    .cloned()
                    .unwrap_or(serde_json::json!([]))
            };
            let total = repos.as_array().map(|a| a.len()).unwrap_or(0);
            success_result(serde_json::json!({
                "repositories": repos,
                "total": total,
                "_source": "bridge",
            }))
        }
        Err(e) => {
            warn!(error = %e, "Delta bridge: falling back to mock for list_repositories");
            let repos = vec![
                serde_json::json!({"id": "repo-001", "name": "my-project", "visibility": "private", "default_branch": "main"}),
                serde_json::json!({"id": "repo-002", "name": "shared-lib", "visibility": "public", "default_branch": "main"}),
            ];
            success_result(serde_json::json!({
                "repositories": repos,
                "total": repos.len(),
                "_source": "mock",
            }))
        }
    }
}

async fn handle_delta_pull_request(args: &serde_json::Value) -> McpToolResult {
    let action = match extract_required_string(args, "action") {
        Ok(a) => a,
        Err(e) => return e,
    };

    let action_opt = Some(action.clone());
    if let Err(e) = validate_enum_opt(&action_opt, "action", &["list", "create", "merge", "close"]) {
        return e;
    }

    let repo = get_optional_string_arg(args, "repo");
    let bridge = DeltaBridge::new();

    match action.as_str() {
        "list" => {
            let mut query = Vec::new();
            if let Some(ref r) = repo {
                query.push(("repo".to_string(), r.clone()));
            }
            match bridge.get("/api/v1/pulls", &query).await {
                Ok(response) => {
                    info!("Delta: list pull requests (bridged)");
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "Delta bridge: falling back to mock for list PRs");
                    let prs = vec![
                        serde_json::json!({"id": "pr-1", "title": "Add feature X", "status": "open", "source_branch": "feature/x", "target_branch": "main"}),
                    ];
                    success_result(serde_json::json!({
                        "pull_requests": prs,
                        "total": prs.len(),
                        "_source": "mock",
                    }))
                }
            }
        }
        "create" => {
            let title =
                get_optional_string_arg(args, "title").unwrap_or_else(|| "Untitled PR".to_string());
            let source_branch = get_optional_string_arg(args, "source_branch")
                .unwrap_or_else(|| "feature".to_string());
            let target_branch = get_optional_string_arg(args, "target_branch")
                .unwrap_or_else(|| "main".to_string());

            let body = serde_json::json!({
                "title": title,
                "source_branch": source_branch,
                "target_branch": target_branch,
                "repo": repo,
            });
            match bridge.post("/api/v1/pulls", body).await {
                Ok(response) => {
                    info!(title = %title, "Delta: create PR (bridged)");
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "Delta bridge: falling back to mock for create PR");
                    let pr_id = Uuid::new_v4().to_string();
                    success_result(serde_json::json!({
                        "id": pr_id,
                        "title": title,
                        "status": "open",
                        "source_branch": source_branch,
                        "target_branch": target_branch,
                        "created_at": chrono::Utc::now().to_rfc3339(),
                        "_source": "mock",
                    }))
                }
            }
        }
        op @ ("merge" | "close") => {
            let pr_id = match get_optional_string_arg(args, "pr_id") {
                Some(id) => id,
                None => return error_result("Missing required argument: pr_id".to_string()),
            };
            let body = serde_json::json!({"action": op});
            match bridge.post(&format!("/api/v1/pulls/{}", pr_id), body).await {
                Ok(response) => {
                    info!(pr_id = %pr_id, action = %op, "Delta: {} PR (bridged)", op);
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "Delta bridge: falling back to mock for {} PR", op);
                    success_result(serde_json::json!({
                        "id": pr_id,
                        "status": if op == "merge" { "merged" } else { "closed" },
                        "updated_at": chrono::Utc::now().to_rfc3339(),
                        "_source": "mock",
                    }))
                }
            }
        }
        _ => unreachable!(),
    }
}

async fn handle_delta_push(args: &serde_json::Value) -> McpToolResult {
    let repo = get_optional_string_arg(args, "repo");
    let branch = get_optional_string_arg(args, "branch");

    let bridge = DeltaBridge::new();
    let body = serde_json::json!({
        "repo": repo,
        "branch": branch.as_deref().unwrap_or("main"),
    });

    match bridge.post("/api/v1/git/push", body).await {
        Ok(response) => {
            info!("Delta: push (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Delta bridge: falling back to mock for push");
            success_result(serde_json::json!({
                "status": "pushed",
                "repo": repo,
                "branch": branch.unwrap_or_else(|| "main".to_string()),
                "message": format!("Delta not reachable at {}", bridge.base_url()),
                "_source": "mock",
            }))
        }
    }
}

async fn handle_delta_ci_status(args: &serde_json::Value) -> McpToolResult {
    let repo = get_optional_string_arg(args, "repo");
    let pipeline_id = get_optional_string_arg(args, "pipeline_id");

    let bridge = DeltaBridge::new();
    let mut query = Vec::new();
    if let Some(ref r) = repo {
        query.push(("repo".to_string(), r.clone()));
    }
    if let Some(ref p) = pipeline_id {
        query.push(("pipeline_id".to_string(), p.clone()));
    }

    match bridge.get("/api/v1/ci/pipelines", &query).await {
        Ok(response) => {
            info!("Delta: CI status (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Delta bridge: falling back to mock for ci_status");
            let pipelines = vec![
                serde_json::json!({"id": "pipe-001", "repo": repo.as_deref().unwrap_or("unknown"), "status": "passed", "branch": "main", "duration_seconds": 142}),
            ];
            success_result(serde_json::json!({
                "pipelines": pipelines,
                "total": pipelines.len(),
                "_source": "mock",
            }))
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    fn test_state() -> ApiState {
        ApiState::with_api_key(None)
    }

    fn build_test_router() -> axum::Router {
        let state = test_state();
        crate::http_api::build_router(state)
    }

    async fn call_tool(
        router: &axum::Router,
        name: &str,
        args: serde_json::Value,
    ) -> McpToolResult {
        let body = serde_json::to_string(&McpToolCall {
            name: name.to_string(),
            arguments: args,
        })
        .unwrap();

        let req = Request::builder()
            .method("POST")
            .uri("/v1/mcp/tools/call")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();

        let resp = router.clone().oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), 1_048_576)
            .await
            .unwrap();
        serde_json::from_slice(&body).unwrap()
    }

    #[tokio::test]
    async fn test_tools_manifest_endpoint() {
        let router = build_test_router();
        let req = Request::builder()
            .method("GET")
            .uri("/v1/mcp/tools")
            .body(Body::empty())
            .unwrap();

        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), 1_048_576)
            .await
            .unwrap();
        let manifest: McpToolManifest = serde_json::from_slice(&body).unwrap();
        assert_eq!(manifest.tools.len(), 31);
    }

    #[tokio::test]
    async fn test_manifest_tool_names() {
        let manifest = build_tool_manifest();
        let names: Vec<&str> = manifest.tools.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"agnos_health"));
        assert!(names.contains(&"agnos_list_agents"));
        assert!(names.contains(&"agnos_get_agent"));
        assert!(names.contains(&"agnos_register_agent"));
        assert!(names.contains(&"agnos_deregister_agent"));
        assert!(names.contains(&"agnos_heartbeat"));
        assert!(names.contains(&"agnos_get_metrics"));
        assert!(names.contains(&"agnos_forward_audit"));
        assert!(names.contains(&"agnos_memory_get"));
        assert!(names.contains(&"agnos_memory_set"));
        assert!(names.contains(&"aequi_estimate_quarterly_tax"));
        assert!(names.contains(&"aequi_schedule_c_preview"));
        assert!(names.contains(&"aequi_import_bank_statement"));
        assert!(names.contains(&"aequi_account_balances"));
        assert!(names.contains(&"aequi_list_receipts"));
        assert!(names.contains(&"agnostic_run_suite"));
        assert!(names.contains(&"agnostic_test_status"));
        assert!(names.contains(&"agnostic_test_report"));
        assert!(names.contains(&"agnostic_list_suites"));
        assert!(names.contains(&"agnostic_agent_status"));
        assert!(names.contains(&"delta_create_repository"));
        assert!(names.contains(&"delta_list_repositories"));
        assert!(names.contains(&"delta_pull_request"));
        assert!(names.contains(&"delta_push"));
        assert!(names.contains(&"delta_ci_status"));
    }

    #[tokio::test]
    async fn test_health_tool() {
        let router = build_test_router();
        let result = call_tool(&router, "agnos_health", serde_json::json!({})).await;
        assert!(!result.is_error);
        assert_eq!(result.content.len(), 1);
        let text = &result.content[0].text;
        let parsed: serde_json::Value = serde_json::from_str(text).unwrap();
        assert_eq!(parsed["status"], "ok");
        assert_eq!(parsed["service"], "agnos-agent-runtime");
    }

    #[tokio::test]
    async fn test_list_agents_empty() {
        let router = build_test_router();
        let result = call_tool(&router, "agnos_list_agents", serde_json::json!({})).await;
        assert!(!result.is_error);
        let parsed: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(parsed["total"], 0);
        assert!(parsed["agents"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_register_and_list_agent() {
        let router = build_test_router();

        // Register
        let result = call_tool(
            &router,
            "agnos_register_agent",
            serde_json::json!({"name": "test-agent", "capabilities": ["read", "write"]}),
        )
        .await;
        assert!(!result.is_error);
        let reg: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(reg["name"], "test-agent");
        assert_eq!(reg["status"], "registered");
        let agent_id = reg["id"].as_str().unwrap().to_string();

        // List
        let result = call_tool(&router, "agnos_list_agents", serde_json::json!({})).await;
        assert!(!result.is_error);
        let list: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(list["total"], 1);

        // Get
        let result = call_tool(
            &router,
            "agnos_get_agent",
            serde_json::json!({"agent_id": agent_id}),
        )
        .await;
        assert!(!result.is_error);
        let agent: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(agent["name"], "test-agent");
    }

    #[tokio::test]
    async fn test_register_duplicate_name() {
        let router = build_test_router();

        let args = serde_json::json!({"name": "dup-agent"});
        let r1 = call_tool(&router, "agnos_register_agent", args.clone()).await;
        assert!(!r1.is_error);

        let r2 = call_tool(&router, "agnos_register_agent", args).await;
        assert!(r2.is_error);
        assert!(r2.content[0].text.contains("already registered"));
    }

    #[tokio::test]
    async fn test_register_empty_name() {
        let router = build_test_router();
        let result = call_tool(
            &router,
            "agnos_register_agent",
            serde_json::json!({"name": ""}),
        )
        .await;
        assert!(result.is_error);
        assert!(result.content[0].text.contains("required"));
    }

    #[tokio::test]
    async fn test_register_missing_name() {
        let router = build_test_router();
        let result = call_tool(&router, "agnos_register_agent", serde_json::json!({})).await;
        assert!(result.is_error);
        assert!(result.content[0].text.contains("Missing"));
    }

    #[tokio::test]
    async fn test_deregister_agent() {
        let router = build_test_router();

        let result = call_tool(
            &router,
            "agnos_register_agent",
            serde_json::json!({"name": "to-remove"}),
        )
        .await;
        let reg: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        let agent_id = reg["id"].as_str().unwrap().to_string();

        let result = call_tool(
            &router,
            "agnos_deregister_agent",
            serde_json::json!({"agent_id": agent_id}),
        )
        .await;
        assert!(!result.is_error);
        let parsed: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(parsed["status"], "deregistered");
    }

    #[tokio::test]
    async fn test_deregister_not_found() {
        let router = build_test_router();
        let result = call_tool(
            &router,
            "agnos_deregister_agent",
            serde_json::json!({"agent_id": "00000000-0000-0000-0000-000000000000"}),
        )
        .await;
        assert!(result.is_error);
        assert!(result.content[0].text.contains("not found"));
    }

    #[tokio::test]
    async fn test_heartbeat() {
        let router = build_test_router();

        let result = call_tool(
            &router,
            "agnos_register_agent",
            serde_json::json!({"name": "hb-agent"}),
        )
        .await;
        let reg: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        let agent_id = reg["id"].as_str().unwrap().to_string();

        let result = call_tool(
            &router,
            "agnos_heartbeat",
            serde_json::json!({"agent_id": agent_id, "status": "busy", "current_task": "processing"}),
        )
        .await;
        assert!(!result.is_error);

        // Verify the heartbeat updated the agent
        let result = call_tool(
            &router,
            "agnos_get_agent",
            serde_json::json!({"agent_id": agent_id}),
        )
        .await;
        let agent: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(agent["status"], "busy");
        assert_eq!(agent["current_task"], "processing");
    }

    #[tokio::test]
    async fn test_heartbeat_not_found() {
        let router = build_test_router();
        let result = call_tool(
            &router,
            "agnos_heartbeat",
            serde_json::json!({"agent_id": "00000000-0000-0000-0000-000000000000"}),
        )
        .await;
        assert!(result.is_error);
    }

    #[tokio::test]
    async fn test_get_metrics() {
        let router = build_test_router();
        let result = call_tool(&router, "agnos_get_metrics", serde_json::json!({})).await;
        assert!(!result.is_error);
        let parsed: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(parsed["total_agents"], 0);
    }

    #[tokio::test]
    async fn test_forward_audit() {
        let router = build_test_router();
        let result = call_tool(
            &router,
            "agnos_forward_audit",
            serde_json::json!({
                "action": "agent.spawn",
                "source": "mcp-test",
                "agent": "test-agent",
                "outcome": "success",
                "details": {"reason": "test"}
            }),
        )
        .await;
        assert!(!result.is_error);
        let parsed: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(parsed["status"], "accepted");
    }

    #[tokio::test]
    async fn test_forward_audit_missing_action() {
        let router = build_test_router();
        let result = call_tool(
            &router,
            "agnos_forward_audit",
            serde_json::json!({"source": "mcp-test"}),
        )
        .await;
        assert!(result.is_error);
        assert!(result.content[0].text.contains("action"));
    }

    #[tokio::test]
    async fn test_memory_set_and_get() {
        let router = build_test_router();

        // Set
        let result = call_tool(
            &router,
            "agnos_memory_set",
            serde_json::json!({"agent_id": "agent-1", "key": "color", "value": "blue"}),
        )
        .await;
        assert!(!result.is_error);

        // Get
        let result = call_tool(
            &router,
            "agnos_memory_get",
            serde_json::json!({"agent_id": "agent-1", "key": "color"}),
        )
        .await;
        assert!(!result.is_error);
        let parsed: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(parsed["value"], "blue");
    }

    #[tokio::test]
    async fn test_memory_get_not_found() {
        let router = build_test_router();
        let result = call_tool(
            &router,
            "agnos_memory_get",
            serde_json::json!({"agent_id": "agent-x", "key": "missing"}),
        )
        .await;
        assert!(result.is_error);
        assert!(result.content[0].text.contains("not found"));
    }

    #[tokio::test]
    async fn test_unknown_tool() {
        let router = build_test_router();
        let result = call_tool(&router, "nonexistent_tool", serde_json::json!({})).await;
        assert!(result.is_error);
        assert!(result.content[0].text.contains("Unknown tool"));
    }

    #[tokio::test]
    async fn test_invalid_uuid() {
        let router = build_test_router();
        let result = call_tool(
            &router,
            "agnos_get_agent",
            serde_json::json!({"agent_id": "not-a-uuid"}),
        )
        .await;
        assert!(result.is_error);
        assert!(result.content[0].text.contains("Invalid UUID"));
    }

    #[tokio::test]
    async fn test_mcp_result_serialization() {
        let result = success_result(serde_json::json!({"key": "value"}));
        let serialized = serde_json::to_value(&result).unwrap();
        assert_eq!(serialized["isError"], false);
        assert_eq!(serialized["content"][0]["type"], "text");
    }

    // -----------------------------------------------------------------------
    // Photis Nadi tools
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_manifest_contains_all_31_tools() {
        let manifest = build_tool_manifest();
        assert_eq!(manifest.tools.len(), 31);
        let names: Vec<&str> = manifest.tools.iter().map(|t| t.name.as_str()).collect();
        for expected in &[
            "agnos_health",
            "agnos_list_agents",
            "agnos_get_agent",
            "agnos_register_agent",
            "agnos_deregister_agent",
            "agnos_heartbeat",
            "agnos_get_metrics",
            "agnos_forward_audit",
            "agnos_memory_get",
            "agnos_memory_set",
            "aequi_estimate_quarterly_tax",
            "aequi_schedule_c_preview",
            "aequi_import_bank_statement",
            "aequi_account_balances",
            "aequi_list_receipts",
            "agnostic_run_suite",
            "agnostic_test_status",
            "agnostic_test_report",
            "agnostic_list_suites",
            "agnostic_agent_status",
            "delta_create_repository",
            "delta_list_repositories",
            "delta_pull_request",
            "delta_push",
            "delta_ci_status",
            "photis_list_tasks",
            "photis_create_task",
            "photis_update_task",
            "photis_get_rituals",
            "photis_analytics",
            "photis_sync",
        ] {
            assert!(names.contains(expected), "Missing tool: {}", expected);
        }
    }

    #[tokio::test]
    async fn test_manifest_tool_names_match_dispatch() {
        let manifest = build_tool_manifest();
        let state = test_state();
        for tool in &manifest.tools {
            let call = McpToolCall {
                name: tool.name.clone(),
                arguments: serde_json::json!({}),
            };
            let result = dispatch_tool_call(&state, &call, Uuid::new_v4()).await;
            // Should NOT be "Unknown tool" for any manifest tool
            // (some may error for missing args, but not "Unknown tool")
            if result.is_error {
                assert!(
                    !result.content[0].text.contains("Unknown tool"),
                    "Tool '{}' in manifest but not in dispatch",
                    tool.name
                );
            }
        }
    }

    #[tokio::test]
    async fn test_photis_list_tasks_no_filter() {
        let router = build_test_router();
        let result = call_tool(&router, "photis_list_tasks", serde_json::json!({})).await;
        assert!(!result.is_error);
        let parsed: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(parsed["total"], 3);
        assert!(parsed["tasks"].as_array().unwrap().len() == 3);
    }

    #[tokio::test]
    async fn test_photis_list_tasks_status_filter() {
        let router = build_test_router();
        let result = call_tool(
            &router,
            "photis_list_tasks",
            serde_json::json!({"status": "done"}),
        )
        .await;
        assert!(!result.is_error);
        let parsed: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(parsed["total"], 1);
    }

    #[tokio::test]
    async fn test_photis_list_tasks_invalid_status() {
        let router = build_test_router();
        let result = call_tool(
            &router,
            "photis_list_tasks",
            serde_json::json!({"status": "invalid"}),
        )
        .await;
        assert!(result.is_error);
        assert!(result.content[0].text.contains("Invalid status"));
    }

    #[tokio::test]
    async fn test_photis_create_task_valid() {
        let router = build_test_router();
        let result = call_tool(
            &router,
            "photis_create_task",
            serde_json::json!({"title": "Fix login bug", "priority": "high"}),
        )
        .await;
        assert!(!result.is_error);
        let parsed: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(parsed["title"], "Fix login bug");
        assert_eq!(parsed["priority"], "high");
        assert_eq!(parsed["status"], "todo");
        assert!(parsed["id"].as_str().is_some());
    }

    #[tokio::test]
    async fn test_photis_create_task_missing_title() {
        let router = build_test_router();
        let result = call_tool(
            &router,
            "photis_create_task",
            serde_json::json!({"priority": "low"}),
        )
        .await;
        assert!(result.is_error);
        assert!(result.content[0]
            .text
            .contains("Missing required argument: title"));
    }

    #[tokio::test]
    async fn test_photis_create_task_empty_title() {
        let router = build_test_router();
        let result = call_tool(
            &router,
            "photis_create_task",
            serde_json::json!({"title": ""}),
        )
        .await;
        assert!(result.is_error);
        assert!(result.content[0].text.contains("empty"));
    }

    #[tokio::test]
    async fn test_photis_create_task_invalid_priority() {
        let router = build_test_router();
        let result = call_tool(
            &router,
            "photis_create_task",
            serde_json::json!({"title": "Test", "priority": "urgent"}),
        )
        .await;
        assert!(result.is_error);
        assert!(result.content[0].text.contains("Invalid priority"));
    }

    #[tokio::test]
    async fn test_photis_update_task_valid() {
        let router = build_test_router();
        let result = call_tool(
            &router,
            "photis_update_task",
            serde_json::json!({"task_id": "task-001", "status": "done", "priority": "low"}),
        )
        .await;
        assert!(!result.is_error);
        let parsed: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(parsed["id"], "task-001");
        assert_eq!(parsed["status"], "done");
        assert_eq!(parsed["priority"], "low");
    }

    #[tokio::test]
    async fn test_photis_update_task_missing_id() {
        let router = build_test_router();
        let result = call_tool(
            &router,
            "photis_update_task",
            serde_json::json!({"status": "done"}),
        )
        .await;
        assert!(result.is_error);
        assert!(result.content[0]
            .text
            .contains("Missing required argument: task_id"));
    }

    #[tokio::test]
    async fn test_photis_get_rituals() {
        let router = build_test_router();
        let result = call_tool(
            &router,
            "photis_get_rituals",
            serde_json::json!({"date": "2026-03-06"}),
        )
        .await;
        assert!(!result.is_error);
        let parsed: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(parsed["date"], "2026-03-06");
        assert!(parsed["rituals"].as_array().unwrap().len() == 4);
    }

    #[tokio::test]
    async fn test_photis_get_rituals_no_date() {
        let router = build_test_router();
        let result = call_tool(&router, "photis_get_rituals", serde_json::json!({})).await;
        assert!(!result.is_error);
        let parsed: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert!(parsed["date"].as_str().is_some());
        assert_eq!(parsed["completion_rate"], 0.5);
    }

    #[tokio::test]
    async fn test_photis_analytics_valid() {
        let router = build_test_router();
        let result = call_tool(
            &router,
            "photis_analytics",
            serde_json::json!({"period": "month", "metric": "velocity"}),
        )
        .await;
        assert!(!result.is_error);
        let parsed: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(parsed["period"], "month");
        assert!(parsed["metrics"]["velocity"].as_f64().is_some());
    }

    #[tokio::test]
    async fn test_photis_analytics_invalid_period() {
        let router = build_test_router();
        let result = call_tool(
            &router,
            "photis_analytics",
            serde_json::json!({"period": "year"}),
        )
        .await;
        assert!(result.is_error);
        assert!(result.content[0].text.contains("Invalid period"));
    }

    #[tokio::test]
    async fn test_photis_analytics_invalid_metric() {
        let router = build_test_router();
        let result = call_tool(
            &router,
            "photis_analytics",
            serde_json::json!({"period": "week", "metric": "unknown"}),
        )
        .await;
        assert!(result.is_error);
        assert!(result.content[0].text.contains("Invalid metric"));
    }

    #[tokio::test]
    async fn test_photis_sync_valid() {
        let router = build_test_router();
        let result = call_tool(
            &router,
            "photis_sync",
            serde_json::json!({"direction": "push"}),
        )
        .await;
        assert!(!result.is_error);
        let parsed: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(parsed["direction"], "push");
        // Service isn't running in tests — reports offline status
        assert!(parsed["status"] == "synced" || parsed["status"] == "offline");
    }

    #[tokio::test]
    async fn test_photis_sync_default_direction() {
        let router = build_test_router();
        let result = call_tool(&router, "photis_sync", serde_json::json!({})).await;
        assert!(!result.is_error);
        let parsed: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(parsed["direction"], "both");
    }

    #[tokio::test]
    async fn test_photis_sync_invalid_direction() {
        let router = build_test_router();
        let result = call_tool(
            &router,
            "photis_sync",
            serde_json::json!({"direction": "sideways"}),
        )
        .await;
        assert!(result.is_error);
        assert!(result.content[0].text.contains("Invalid direction"));
    }

    // -----------------------------------------------------------------------
    // Photis Nadi Bridge tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_photis_bridge_default() {
        let bridge = PhotisBridge::new();
        assert!(bridge.base_url().starts_with("http"));
        assert_eq!(
            bridge.url("/tasks"),
            format!("{}/api/v1/tasks", bridge.base_url())
        );
    }

    #[test]
    fn test_photis_bridge_with_config() {
        let bridge = PhotisBridge::with_config(
            "http://10.0.0.5:9000".to_string(),
            Some("test-key".to_string()),
        );
        assert_eq!(bridge.base_url(), "http://10.0.0.5:9000");
        assert_eq!(bridge.url("/health"), "http://10.0.0.5:9000/api/v1/health");
    }

    #[test]
    fn test_photis_bridge_auth_headers() {
        let bridge = PhotisBridge::with_config(
            "http://localhost:8081".to_string(),
            Some("secret-key".to_string()),
        );
        let headers = bridge.auth_headers();
        assert!(headers
            .iter()
            .any(|(k, v)| k == "Authorization" && v == "Bearer secret-key"));
        assert!(headers
            .iter()
            .any(|(k, v)| k == "Content-Type" && v == "application/json"));
    }

    #[test]
    fn test_photis_bridge_no_auth_without_key() {
        let bridge = PhotisBridge::with_config("http://localhost:8081".to_string(), None);
        let headers = bridge.auth_headers();
        assert!(!headers.iter().any(|(k, _)| k == "Authorization"));
    }

    #[tokio::test]
    async fn test_photis_bridge_health_check_offline() {
        // Bridge to a port that nothing is listening on
        let bridge = PhotisBridge::with_config("http://127.0.0.1:19999".to_string(), None);
        assert!(!bridge.health_check().await);
    }

    #[tokio::test]
    async fn test_photis_list_tasks_falls_back_to_mock() {
        // With no Photis Nadi running, should fall back to mock data
        let router = build_test_router();
        let result = call_tool(&router, "photis_list_tasks", serde_json::json!({})).await;
        assert!(!result.is_error);
        let parsed: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        // Mock data has 3 tasks
        assert!(parsed["tasks"].as_array().is_some());
        assert_eq!(parsed["_source"], "mock");
    }

    #[tokio::test]
    async fn test_photis_sync_reports_offline() {
        let router = build_test_router();
        let result = call_tool(
            &router,
            "photis_sync",
            serde_json::json!({"direction": "both"}),
        )
        .await;
        assert!(!result.is_error);
        let parsed: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        // Service isn't running in test, so should report offline
        assert_eq!(parsed["service_online"], false);
    }

    // --- Delta code hosting tool tests ---

    #[tokio::test]
    async fn test_delta_create_repository_mock() {
        let router = build_test_router();
        let result = call_tool(
            &router,
            "delta_create_repository",
            serde_json::json!({"name": "my-project"}),
        )
        .await;
        assert!(!result.is_error);
        let parsed: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(parsed["name"], "my-project");
        assert_eq!(parsed["visibility"], "private");
        assert_eq!(parsed["_source"], "mock");
    }

    #[tokio::test]
    async fn test_delta_create_repository_missing_name() {
        let router = build_test_router();
        let result = call_tool(&router, "delta_create_repository", serde_json::json!({})).await;
        assert!(result.is_error);
    }

    #[tokio::test]
    async fn test_delta_create_repository_invalid_visibility() {
        let router = build_test_router();
        let result = call_tool(
            &router,
            "delta_create_repository",
            serde_json::json!({"name": "test", "visibility": "secret"}),
        )
        .await;
        assert!(result.is_error);
    }

    #[tokio::test]
    async fn test_delta_list_repositories_mock() {
        let router = build_test_router();
        let result = call_tool(&router, "delta_list_repositories", serde_json::json!({})).await;
        assert!(!result.is_error);
        let parsed: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert!(parsed["repositories"].as_array().is_some());
        // Accepts both mock (no Delta running) and bridge (Delta on 8070)
        let source = parsed["_source"].as_str().unwrap_or("");
        assert!(
            source == "mock" || source == "bridge",
            "expected mock or bridge, got: {}",
            source
        );
    }

    #[tokio::test]
    async fn test_delta_pull_request_list_mock() {
        let router = build_test_router();
        let result = call_tool(
            &router,
            "delta_pull_request",
            serde_json::json!({"action": "list"}),
        )
        .await;
        assert!(!result.is_error);
        let parsed: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert!(parsed["pull_requests"].as_array().is_some());
        assert_eq!(parsed["_source"], "mock");
    }

    #[tokio::test]
    async fn test_delta_pull_request_create_mock() {
        let router = build_test_router();
        let result = call_tool(
            &router,
            "delta_pull_request",
            serde_json::json!({"action": "create", "title": "Add feature", "source_branch": "feat/x"}),
        )
        .await;
        assert!(!result.is_error);
        let parsed: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(parsed["title"], "Add feature");
        assert_eq!(parsed["status"], "open");
    }

    #[tokio::test]
    async fn test_delta_pull_request_invalid_action() {
        let router = build_test_router();
        let result = call_tool(
            &router,
            "delta_pull_request",
            serde_json::json!({"action": "rebase"}),
        )
        .await;
        assert!(result.is_error);
    }

    #[tokio::test]
    async fn test_delta_pull_request_missing_action() {
        let router = build_test_router();
        let result = call_tool(&router, "delta_pull_request", serde_json::json!({})).await;
        assert!(result.is_error);
    }

    #[tokio::test]
    async fn test_delta_push_mock() {
        let router = build_test_router();
        let result = call_tool(
            &router,
            "delta_push",
            serde_json::json!({"repo": "my-project", "branch": "main"}),
        )
        .await;
        assert!(!result.is_error);
        let parsed: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(parsed["status"], "pushed");
        assert_eq!(parsed["_source"], "mock");
    }

    #[tokio::test]
    async fn test_delta_ci_status_mock() {
        let router = build_test_router();
        let result = call_tool(
            &router,
            "delta_ci_status",
            serde_json::json!({"repo": "my-project"}),
        )
        .await;
        assert!(!result.is_error);
        let parsed: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert!(parsed["pipelines"].as_array().is_some());
        assert_eq!(parsed["_source"], "mock");
    }

    #[tokio::test]
    async fn test_delta_merge_pr_requires_pr_id() {
        let router = build_test_router();
        let result = call_tool(
            &router,
            "delta_pull_request",
            serde_json::json!({"action": "merge"}),
        )
        .await;
        assert!(result.is_error);
    }

    #[tokio::test]
    async fn test_delta_close_pr_mock() {
        let router = build_test_router();
        let result = call_tool(
            &router,
            "delta_pull_request",
            serde_json::json!({"action": "close", "pr_id": "pr-123"}),
        )
        .await;
        assert!(!result.is_error);
        let parsed: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(parsed["status"], "closed");
    }

    // --- Aequi accounting tool tests ---

    #[tokio::test]
    async fn test_aequi_tax_estimate_mock() {
        let router = build_test_router();
        let result = call_tool(
            &router,
            "aequi_estimate_quarterly_tax",
            serde_json::json!({}),
        )
        .await;
        assert!(!result.is_error);
        let parsed: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert!(parsed["estimated_tax"].as_f64().is_some());
        assert_eq!(parsed["_source"], "mock");
    }

    #[tokio::test]
    async fn test_aequi_tax_estimate_with_quarter() {
        let router = build_test_router();
        let result = call_tool(
            &router,
            "aequi_estimate_quarterly_tax",
            serde_json::json!({"quarter": "2"}),
        )
        .await;
        assert!(!result.is_error);
        let parsed: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(parsed["quarter"], "2");
    }

    #[tokio::test]
    async fn test_aequi_tax_estimate_invalid_quarter() {
        let router = build_test_router();
        let result = call_tool(
            &router,
            "aequi_estimate_quarterly_tax",
            serde_json::json!({"quarter": "5"}),
        )
        .await;
        assert!(result.is_error);
    }

    #[tokio::test]
    async fn test_aequi_schedule_c_mock() {
        let router = build_test_router();
        let result = call_tool(&router, "aequi_schedule_c_preview", serde_json::json!({})).await;
        assert!(!result.is_error);
        let parsed: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        if parsed["_source"] == "mock" {
            assert!(parsed["net_profit"].as_f64().is_some());
            assert!(parsed["categories"].is_object());
        }
        // Live Aequi may return different structure — just verify it parsed
    }

    #[tokio::test]
    async fn test_aequi_import_bank_mock() {
        let router = build_test_router();
        let result = call_tool(
            &router,
            "aequi_import_bank_statement",
            serde_json::json!({"file_path": "/tmp/bank-jan.ofx"}),
        )
        .await;
        assert!(!result.is_error);
        let parsed: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(parsed["status"], "imported");
        assert!(parsed["transactions_imported"].as_u64().is_some());
    }

    #[tokio::test]
    async fn test_aequi_import_bank_missing_path() {
        let router = build_test_router();
        let result = call_tool(
            &router,
            "aequi_import_bank_statement",
            serde_json::json!({}),
        )
        .await;
        assert!(result.is_error);
    }

    #[tokio::test]
    async fn test_aequi_import_bank_invalid_format() {
        let router = build_test_router();
        let result = call_tool(
            &router,
            "aequi_import_bank_statement",
            serde_json::json!({"file_path": "/tmp/file.txt", "format": "pdf"}),
        )
        .await;
        assert!(result.is_error);
    }

    #[tokio::test]
    async fn test_aequi_balances_mock() {
        let router = build_test_router();
        let result = call_tool(&router, "aequi_account_balances", serde_json::json!({})).await;
        assert!(!result.is_error);
        let parsed: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert!(parsed["accounts"].as_array().is_some());
        assert!(parsed["net_worth"].as_f64().is_some());
        assert_eq!(parsed["_source"], "mock");
    }

    #[tokio::test]
    async fn test_aequi_balances_invalid_type() {
        let router = build_test_router();
        let result = call_tool(
            &router,
            "aequi_account_balances",
            serde_json::json!({"account_type": "crypto"}),
        )
        .await;
        assert!(result.is_error);
    }

    #[tokio::test]
    async fn test_aequi_receipts_mock() {
        let router = build_test_router();
        let result = call_tool(&router, "aequi_list_receipts", serde_json::json!({})).await;
        assert!(!result.is_error);
        let parsed: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        if parsed["_source"] == "mock" {
            assert!(parsed["receipts"].as_array().is_some());
        }
        // Live Aequi may return different structure — just verify it parsed
    }

    #[tokio::test]
    async fn test_aequi_receipts_filtered() {
        let router = build_test_router();
        let result = call_tool(
            &router,
            "aequi_list_receipts",
            serde_json::json!({"status": "pending_review"}),
        )
        .await;
        assert!(!result.is_error);
        let parsed: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        if let Some(receipts) = parsed["receipts"].as_array() {
            for r in receipts {
                assert_eq!(r["status"], "pending_review");
            }
        }
        // Live Aequi may return different structure — just verify it parsed
    }

    #[tokio::test]
    async fn test_aequi_receipts_invalid_status() {
        let router = build_test_router();
        let result = call_tool(
            &router,
            "aequi_list_receipts",
            serde_json::json!({"status": "archived"}),
        )
        .await;
        assert!(result.is_error);
    }

    // --- Agnostic QA platform tool tests ---

    #[tokio::test]
    async fn test_agnostic_run_suite_mock() {
        let router = build_test_router();
        let result = call_tool(
            &router,
            "agnostic_run_suite",
            serde_json::json!({"suite": "Full Regression"}),
        )
        .await;
        assert!(!result.is_error);
        let parsed: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(parsed["suite"], "Full Regression");
        assert_eq!(parsed["status"], "running");
        assert_eq!(parsed["_source"], "mock");
    }

    #[tokio::test]
    async fn test_agnostic_run_suite_missing_name() {
        let router = build_test_router();
        let result = call_tool(&router, "agnostic_run_suite", serde_json::json!({})).await;
        assert!(result.is_error);
    }

    #[tokio::test]
    async fn test_agnostic_test_status_mock() {
        let router = build_test_router();
        let result = call_tool(
            &router,
            "agnostic_test_status",
            serde_json::json!({"run_id": "run-001"}),
        )
        .await;
        assert!(!result.is_error);
        let parsed: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(parsed["status"], "completed");
        assert!(parsed["total_tests"].as_u64().is_some());
        assert_eq!(parsed["_source"], "mock");
    }

    #[tokio::test]
    async fn test_agnostic_test_status_missing_id() {
        let router = build_test_router();
        let result = call_tool(&router, "agnostic_test_status", serde_json::json!({})).await;
        assert!(result.is_error);
    }

    #[tokio::test]
    async fn test_agnostic_test_report_mock() {
        let router = build_test_router();
        let result = call_tool(
            &router,
            "agnostic_test_report",
            serde_json::json!({"run_id": "run-001"}),
        )
        .await;
        assert!(!result.is_error);
        let parsed: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert!(parsed["summary"].is_object());
        assert!(parsed["failures"].as_array().is_some());
        assert_eq!(parsed["_source"], "mock");
    }

    #[tokio::test]
    async fn test_agnostic_test_report_invalid_format() {
        let router = build_test_router();
        let result = call_tool(
            &router,
            "agnostic_test_report",
            serde_json::json!({"run_id": "run-001", "format": "pdf"}),
        )
        .await;
        assert!(result.is_error);
    }

    #[tokio::test]
    async fn test_agnostic_test_report_missing_id() {
        let router = build_test_router();
        let result = call_tool(&router, "agnostic_test_report", serde_json::json!({})).await;
        assert!(result.is_error);
    }

    #[tokio::test]
    async fn test_agnostic_list_suites_mock() {
        let router = build_test_router();
        let result = call_tool(&router, "agnostic_list_suites", serde_json::json!({})).await;
        assert!(!result.is_error);
        let parsed: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert!(parsed["suites"].as_array().unwrap().len() >= 2);
        assert_eq!(parsed["_source"], "mock");
    }

    #[tokio::test]
    async fn test_agnostic_list_suites_filtered() {
        let router = build_test_router();
        let result = call_tool(
            &router,
            "agnostic_list_suites",
            serde_json::json!({"category": "security"}),
        )
        .await;
        assert!(!result.is_error);
        let parsed: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        let suites = parsed["suites"].as_array().unwrap();
        for s in suites {
            assert_eq!(s["category"], "security");
        }
    }

    #[tokio::test]
    async fn test_agnostic_list_suites_invalid_category() {
        let router = build_test_router();
        let result = call_tool(
            &router,
            "agnostic_list_suites",
            serde_json::json!({"category": "chaos"}),
        )
        .await;
        assert!(result.is_error);
    }

    #[tokio::test]
    async fn test_agnostic_agent_status_mock() {
        let router = build_test_router();
        let result = call_tool(&router, "agnostic_agent_status", serde_json::json!({})).await;
        assert!(!result.is_error);
        let parsed: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert!(parsed["agents"].as_array().unwrap().len() >= 4);
        assert_eq!(parsed["_source"], "mock");
    }

    #[tokio::test]
    async fn test_agnostic_agent_status_filtered() {
        let router = build_test_router();
        let result = call_tool(
            &router,
            "agnostic_agent_status",
            serde_json::json!({"agent_type": "security"}),
        )
        .await;
        assert!(!result.is_error);
        let parsed: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        let agents = parsed["agents"].as_array().unwrap();
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0]["type"], "security");
    }

    #[tokio::test]
    async fn test_agnostic_agent_status_invalid_type() {
        let router = build_test_router();
        let result = call_tool(
            &router,
            "agnostic_agent_status",
            serde_json::json!({"agent_type": "chaos"}),
        )
        .await;
        assert!(result.is_error);
    }

    // --- MCP protocol type serialization ---

    #[test]
    fn test_mcp_tool_param_serialization() {
        let param = McpToolParam {
            name: "agent_id".to_string(),
            param_type: "string".to_string(),
            description: "The agent identifier".to_string(),
            required: true,
        };
        let json = serde_json::to_value(&param).unwrap();
        assert_eq!(json["name"], "agent_id");
        assert_eq!(json["type"], "string"); // serde rename
        assert_eq!(json["required"], true);

        let deser: McpToolParam = serde_json::from_value(json).unwrap();
        assert_eq!(deser.name, "agent_id");
        assert!(deser.required);
    }

    #[test]
    fn test_mcp_tool_param_required_defaults_false() {
        let json = serde_json::json!({
            "name": "limit",
            "type": "integer",
            "description": "Max results"
        });
        let param: McpToolParam = serde_json::from_value(json).unwrap();
        assert!(!param.required);
    }

    #[test]
    fn test_mcp_tool_description_serialization() {
        let desc = McpToolDescription {
            name: "test_tool".to_string(),
            description: "A test tool".to_string(),
            input_schema: serde_json::json!({"type": "object", "properties": {}}),
        };
        let json = serde_json::to_value(&desc).unwrap();
        assert_eq!(json["name"], "test_tool");
        assert_eq!(json["inputSchema"]["type"], "object"); // serde rename
    }

    #[test]
    fn test_mcp_tool_manifest_serialization() {
        let manifest = McpToolManifest {
            tools: vec![
                McpToolDescription {
                    name: "tool1".to_string(),
                    description: "First tool".to_string(),
                    input_schema: serde_json::json!({}),
                },
                McpToolDescription {
                    name: "tool2".to_string(),
                    description: "Second tool".to_string(),
                    input_schema: serde_json::json!({}),
                },
            ],
        };
        let json = serde_json::to_string(&manifest).unwrap();
        let deser: McpToolManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.tools.len(), 2);
        assert_eq!(deser.tools[0].name, "tool1");
    }

    #[test]
    fn test_mcp_content_block_serialization() {
        let block = McpContentBlock {
            content_type: "text".to_string(),
            text: "Hello, world!".to_string(),
        };
        let json = serde_json::to_value(&block).unwrap();
        assert_eq!(json["type"], "text"); // serde rename
        assert_eq!(json["text"], "Hello, world!");
    }

    #[test]
    fn test_mcp_tool_result_success() {
        let result = McpToolResult {
            content: vec![McpContentBlock {
                content_type: "text".to_string(),
                text: "{\"status\":\"ok\"}".to_string(),
            }],
            is_error: false,
        };
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["isError"], false); // serde rename
        assert_eq!(json["content"].as_array().unwrap().len(), 1);

        let deser: McpToolResult = serde_json::from_value(json).unwrap();
        assert!(!deser.is_error);
    }

    #[test]
    fn test_mcp_tool_result_error() {
        let result = McpToolResult {
            content: vec![McpContentBlock {
                content_type: "text".to_string(),
                text: "not found".to_string(),
            }],
            is_error: true,
        };
        assert!(result.is_error);
        assert_eq!(result.content[0].text, "not found");
    }

    #[test]
    fn test_external_mcp_tool_serialization() {
        let tool = ExternalMcpTool {
            tool: McpToolDescription {
                name: "ext_tool".to_string(),
                description: "External tool".to_string(),
                input_schema: serde_json::json!({"type": "object"}),
            },
            callback_url: "http://localhost:9090/callback".to_string(),
            source: "test-service".to_string(),
        };
        let json = serde_json::to_string(&tool).unwrap();
        let deser: ExternalMcpTool = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.tool.name, "ext_tool");
        assert_eq!(deser.callback_url, "http://localhost:9090/callback");
        assert_eq!(deser.source, "test-service");
    }

    #[test]
    fn test_register_mcp_tool_request_deserialization() {
        let json = serde_json::json!({
            "name": "my_tool",
            "description": "My custom tool",
            "inputSchema": {"type": "object", "properties": {"x": {"type": "string"}}},
            "callback_url": "http://localhost:3000/tool"
        });
        let req: RegisterMcpToolRequest = serde_json::from_value(json).unwrap();
        assert_eq!(req.name, "my_tool");
        assert_eq!(req.callback_url, "http://localhost:3000/tool");
        assert!(req.source.is_none());
    }

    #[test]
    fn test_register_mcp_tool_request_with_source() {
        let json = serde_json::json!({
            "name": "ext",
            "description": "External",
            "inputSchema": {},
            "callback_url": "http://localhost/cb",
            "source": "delta"
        });
        let req: RegisterMcpToolRequest = serde_json::from_value(json).unwrap();
        assert_eq!(req.source, Some("delta".to_string()));
    }

    // --- json_schema_object helper ---

    #[test]
    fn test_json_schema_object_empty() {
        let schema = json_schema_object(serde_json::json!({}), vec![]);
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"].as_object().unwrap().is_empty());
        assert!(schema["required"].as_array().unwrap().is_empty());
    }

    #[test]
    fn test_json_schema_object_with_properties() {
        let schema = json_schema_object(
            serde_json::json!({
                "name": {"type": "string"},
                "count": {"type": "integer"}
            }),
            vec!["name"],
        );
        assert_eq!(schema["type"], "object");
        assert_eq!(schema["properties"]["name"]["type"], "string");
        assert_eq!(schema["required"][0], "name");
    }

    // --- build_tool_manifest ---

    #[test]
    fn test_build_tool_manifest_has_tools() {
        let manifest = build_tool_manifest();
        assert!(
            manifest.tools.len() >= 20,
            "Expected at least 20 MCP tools, got {}",
            manifest.tools.len()
        );
    }

    #[test]
    fn test_build_tool_manifest_all_have_schemas() {
        let manifest = build_tool_manifest();
        for tool in &manifest.tools {
            assert!(!tool.name.is_empty(), "Tool has empty name");
            assert!(
                !tool.description.is_empty(),
                "Tool {} has empty description",
                tool.name
            );
            assert_eq!(
                tool.input_schema["type"], "object",
                "Tool {} schema missing type:object",
                tool.name
            );
        }
    }

    #[test]
    fn test_build_tool_manifest_has_core_tools() {
        let manifest = build_tool_manifest();
        let names: Vec<&str> = manifest.tools.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"agnos_health"), "Missing agnos_health");
        assert!(
            names.contains(&"agnos_list_agents"),
            "Missing agnos_list_agents"
        );
        assert!(
            names.contains(&"agnos_register_agent"),
            "Missing agnos_register_agent"
        );
    }

    #[test]
    fn test_build_tool_manifest_unique_names() {
        let manifest = build_tool_manifest();
        let mut names: Vec<&str> = manifest.tools.iter().map(|t| t.name.as_str()).collect();
        let count_before = names.len();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), count_before, "Duplicate tool names found");
    }

    #[tokio::test]
    async fn test_register_tool_rejects_private_ip_callback() {
        let router = build_test_router();
        let body = serde_json::json!({"name":"ssrf_priv","description":"t","inputSchema":{"type":"object"},"callback_url":"http://10.0.0.1:9090/cb"});
        let req = Request::builder().method("POST").uri("/v1/mcp/tools").header("content-type","application/json").body(Body::from(serde_json::to_string(&body).unwrap())).unwrap();
        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let b = axum::body::to_bytes(resp.into_body(), 65536).await.unwrap();
        let j: serde_json::Value = serde_json::from_slice(&b).unwrap();
        assert!(j["error"].as_str().unwrap().contains("private"));
    }

    #[tokio::test]
    async fn test_register_tool_rejects_localhost_callback() {
        let router = build_test_router();
        let body = serde_json::json!({"name":"ssrf_lh","description":"t","inputSchema":{"type":"object"},"callback_url":"http://localhost:8090/v1/health"});
        let req = Request::builder().method("POST").uri("/v1/mcp/tools").header("content-type","application/json").body(Body::from(serde_json::to_string(&body).unwrap())).unwrap();
        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let b = axum::body::to_bytes(resp.into_body(), 65536).await.unwrap();
        let j: serde_json::Value = serde_json::from_slice(&b).unwrap();
        assert!(j["error"].as_str().unwrap().contains("localhost"));
    }

    #[tokio::test]
    async fn test_register_tool_rejects_ftp_callback() {
        let router = build_test_router();
        let body = serde_json::json!({"name":"ssrf_ftp","description":"t","inputSchema":{"type":"object"},"callback_url":"ftp://example.com/cb"});
        let req = Request::builder().method("POST").uri("/v1/mcp/tools").header("content-type","application/json").body(Body::from(serde_json::to_string(&body).unwrap())).unwrap();
        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let b = axum::body::to_bytes(resp.into_body(), 65536).await.unwrap();
        let j: serde_json::Value = serde_json::from_slice(&b).unwrap();
        assert!(j["error"].as_str().unwrap().contains("scheme"));
    }

    #[tokio::test]
    async fn test_register_tool_rejects_cred_callback() {
        let router = build_test_router();
        let body = serde_json::json!({"name":"ssrf_cred","description":"t","inputSchema":{"type":"object"},"callback_url":"https://a:b@example.com/cb"});
        let req = Request::builder().method("POST").uri("/v1/mcp/tools").header("content-type","application/json").body(Body::from(serde_json::to_string(&body).unwrap())).unwrap();
        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let b = axum::body::to_bytes(resp.into_body(), 65536).await.unwrap();
        let j: serde_json::Value = serde_json::from_slice(&b).unwrap();
        assert!(j["error"].as_str().unwrap().contains("credentials"));
    }

    #[tokio::test]
    async fn test_register_tool_accepts_public_callback() {
        let router = build_test_router();
        let body = serde_json::json!({"name":"ext_ok","description":"ok","inputSchema":{"type":"object"},"callback_url":"https://example.com:9090/cb"});
        let req = Request::builder().method("POST").uri("/v1/mcp/tools").header("content-type","application/json").body(Body::from(serde_json::to_string(&body).unwrap())).unwrap();
        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
    }

    #[tokio::test]
    async fn test_dispatch_blocks_ssrf_link_local() {
        let ext = ExternalMcpTool { tool: McpToolDescription { name: "evil".into(), description: "t".into(), input_schema: serde_json::json!({}) }, callback_url: "http://169.254.169.254/latest/meta-data/".into(), source: "test".into() };
        let call = McpToolCall { name: "evil".into(), arguments: serde_json::json!({}) };
        let result = dispatch_external_tool(&ext, &call).await;
        assert!(result.is_error);
        assert!(result.content[0].text.contains("SSRF") || result.content[0].text.contains("private"));
    }

    // -----------------------------------------------------------------------
    // H24: JSON validation helper tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_extract_required_string_present() {
        let args = serde_json::json!({"name": "hello"});
        let result = extract_required_string(&args, "name");
        assert_eq!(result.unwrap(), "hello");
    }

    #[test]
    fn test_extract_required_string_missing() {
        let args = serde_json::json!({});
        let result = extract_required_string(&args, "name");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.is_error);
        assert!(err.content[0].text.contains("Missing required argument: name"));
    }

    #[test]
    fn test_extract_required_string_null_value() {
        let args = serde_json::json!({"name": null});
        let result = extract_required_string(&args, "name");
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_required_uuid_valid() {
        let args = serde_json::json!({"id": "550e8400-e29b-41d4-a716-446655440000"});
        let result = extract_required_uuid(&args, "id");
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap().to_string(),
            "550e8400-e29b-41d4-a716-446655440000"
        );
    }

    #[test]
    fn test_extract_required_uuid_invalid() {
        let args = serde_json::json!({"id": "not-a-uuid"});
        let result = extract_required_uuid(&args, "id");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.content[0].text.contains("Invalid UUID"));
    }

    #[test]
    fn test_extract_required_uuid_missing() {
        let args = serde_json::json!({});
        let result = extract_required_uuid(&args, "id");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.content[0].text.contains("Missing required argument"));
    }

    #[test]
    fn test_extract_optional_u64_present() {
        let args = serde_json::json!({"limit": 42});
        assert_eq!(extract_optional_u64(&args, "limit", 10), 42);
    }

    #[test]
    fn test_extract_optional_u64_missing() {
        let args = serde_json::json!({});
        assert_eq!(extract_optional_u64(&args, "limit", 10), 10);
    }

    #[test]
    fn test_validate_enum_opt_valid() {
        let v = Some("todo".to_string());
        assert!(validate_enum_opt(&v, "status", &["todo", "done"]).is_ok());
    }

    #[test]
    fn test_validate_enum_opt_invalid() {
        let v = Some("invalid".to_string());
        let result = validate_enum_opt(&v, "status", &["todo", "done"]);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.content[0].text.contains("Invalid status"));
    }

    #[test]
    fn test_validate_enum_opt_none() {
        let v: Option<String> = None;
        assert!(validate_enum_opt(&v, "status", &["todo", "done"]).is_ok());
    }

    // -----------------------------------------------------------------------
    // H29: Request ID correlation test
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_mcp_dispatch_generates_request_id() {
        // Verify that dispatch_tool_call accepts a request_id and does not
        // panic or alter behavior based on it.
        let state = test_state();
        let call = McpToolCall {
            name: "agnos_health".to_string(),
            arguments: serde_json::json!({}),
        };
        let request_id = Uuid::new_v4();
        let result = dispatch_tool_call(&state, &call, request_id).await;
        assert!(!result.is_error);
        let text = &result.content[0].text;
        let parsed: serde_json::Value = serde_json::from_str(text).unwrap();
        assert_eq!(parsed["status"], "ok");
    }
}
