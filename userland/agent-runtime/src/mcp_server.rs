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

use crate::http_api::{
    AuditEvent, ApiState, RegisterAgentRequest, ResourceNeeds,
};

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

/// Build the static MCP tool manifest for the agent runtime.
pub fn build_tool_manifest() -> McpToolManifest {
    let tools = vec![
        McpToolDescription {
            name: "agnos_health".to_string(),
            description: "Check agent runtime health status".to_string(),
            input_schema: json_schema_object(serde_json::json!({}), vec![]),
        },
        McpToolDescription {
            name: "agnos_list_agents".to_string(),
            description: "List all registered agents".to_string(),
            input_schema: json_schema_object(serde_json::json!({}), vec![]),
        },
        McpToolDescription {
            name: "agnos_get_agent".to_string(),
            description: "Get details for a specific agent by ID".to_string(),
            input_schema: json_schema_object(
                serde_json::json!({
                    "agent_id": {"type": "string", "description": "UUID of the agent"}
                }),
                vec!["agent_id"],
            ),
        },
        McpToolDescription {
            name: "agnos_register_agent".to_string(),
            description: "Register a new agent with the runtime".to_string(),
            input_schema: json_schema_object(
                serde_json::json!({
                    "name": {"type": "string", "description": "Agent name"},
                    "capabilities": {"type": "array", "items": {"type": "string"}, "description": "Agent capabilities"},
                    "metadata": {"type": "object", "description": "Additional key-value metadata"}
                }),
                vec!["name"],
            ),
        },
        McpToolDescription {
            name: "agnos_deregister_agent".to_string(),
            description: "Deregister an agent by ID".to_string(),
            input_schema: json_schema_object(
                serde_json::json!({
                    "agent_id": {"type": "string", "description": "UUID of the agent to deregister"}
                }),
                vec!["agent_id"],
            ),
        },
        McpToolDescription {
            name: "agnos_heartbeat".to_string(),
            description: "Send a heartbeat for an agent".to_string(),
            input_schema: json_schema_object(
                serde_json::json!({
                    "agent_id": {"type": "string", "description": "UUID of the agent"},
                    "status": {"type": "string", "description": "Optional status update"},
                    "current_task": {"type": "string", "description": "Optional current task description"}
                }),
                vec!["agent_id"],
            ),
        },
        McpToolDescription {
            name: "agnos_get_metrics".to_string(),
            description: "Get agent runtime metrics".to_string(),
            input_schema: json_schema_object(serde_json::json!({}), vec![]),
        },
        McpToolDescription {
            name: "agnos_forward_audit".to_string(),
            description: "Forward an audit event to the runtime".to_string(),
            input_schema: json_schema_object(
                serde_json::json!({
                    "action": {"type": "string", "description": "Audit action name"},
                    "agent": {"type": "string", "description": "Optional agent name or ID"},
                    "details": {"type": "object", "description": "Arbitrary event details"},
                    "outcome": {"type": "string", "description": "Event outcome (e.g. success, failure)"},
                    "source": {"type": "string", "description": "Source identifier for the audit event"}
                }),
                vec!["action", "source"],
            ),
        },
        McpToolDescription {
            name: "agnos_memory_get".to_string(),
            description: "Get a memory value for an agent by key".to_string(),
            input_schema: json_schema_object(
                serde_json::json!({
                    "agent_id": {"type": "string", "description": "UUID of the agent"},
                    "key": {"type": "string", "description": "Memory key to retrieve"}
                }),
                vec!["agent_id", "key"],
            ),
        },
        McpToolDescription {
            name: "agnos_memory_set".to_string(),
            description: "Set a memory value for an agent by key".to_string(),
            input_schema: json_schema_object(
                serde_json::json!({
                    "agent_id": {"type": "string", "description": "UUID of the agent"},
                    "key": {"type": "string", "description": "Memory key to set"},
                    "value": {"description": "Value to store (any JSON value)"}
                }),
                vec!["agent_id", "key", "value"],
            ),
        },
    ];

    McpToolManifest { tools }
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// `GET /v1/mcp/tools` — return the MCP tool manifest.
pub async fn mcp_tools_handler() -> impl IntoResponse {
    let manifest = build_tool_manifest();
    Json(manifest)
}

/// `POST /v1/mcp/tools/call` — dispatch an MCP tool call to the appropriate logic.
pub async fn mcp_tool_call_handler(
    State(state): State<ApiState>,
    Json(call): Json<McpToolCall>,
) -> impl IntoResponse {
    info!(tool = %call.name, "MCP tool call received");
    debug!(tool = %call.name, arguments = %call.arguments, "MCP tool call details");

    let result = dispatch_tool_call(&state, &call).await;
    Json(result)
}

async fn dispatch_tool_call(state: &ApiState, call: &McpToolCall) -> McpToolResult {
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
        unknown => {
            warn!(tool = %unknown, "Unknown MCP tool called");
            error_result(format!("Unknown tool: {}", unknown))
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
    args.get(key).and_then(|v| v.as_str()).map(|s| s.to_string())
}

fn get_optional_string_arg(args: &serde_json::Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(|v| if v.is_null() { None } else { v.as_str() })
        .map(|s| s.to_string())
}

// ---------------------------------------------------------------------------
// Tool Implementations
// ---------------------------------------------------------------------------

async fn handle_health(state: &ApiState) -> McpToolResult {
    let agents = state.agents_read().await;
    let uptime = (chrono::Utc::now() - state.started_at()).num_seconds().max(0) as u64;

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
    let agent_id = match get_string_arg(args, "agent_id") {
        Some(id) => id,
        None => return error_result("Missing required argument: agent_id".to_string()),
    };

    let uuid = match Uuid::parse_str(&agent_id) {
        Ok(id) => id,
        Err(_) => return error_result(format!("Invalid UUID: {}", agent_id)),
    };

    let agents = state.agents_read().await;
    match agents.get(&uuid) {
        Some(entry) => success_result(serde_json::to_value(&entry.detail).unwrap_or_default()),
        None => error_result(format!("Agent {} not found", uuid)),
    }
}

async fn handle_register_agent(state: &ApiState, args: &serde_json::Value) -> McpToolResult {
    let name = match get_string_arg(args, "name") {
        Some(n) => n,
        None => return error_result("Missing required argument: name".to_string()),
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

    let req = RegisterAgentRequest {
        name: name.clone(),
        capabilities,
        resource_needs: ResourceNeeds::default(),
        metadata,
    };

    let mut agents = state.agents_write().await;

    // Check for duplicate names
    if agents.values().any(|a| a.detail.name == req.name) {
        return error_result(format!("Agent '{}' already registered", req.name));
    }

    let id = Uuid::new_v4();
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

    agents.insert(id, crate::http_api::RegisteredAgentEntry {
        detail: detail.clone(),
    });

    info!(agent_name = %req.name, agent_id = %id, "Agent registered via MCP");

    success_result(serde_json::json!({
        "id": id.to_string(),
        "name": req.name,
        "status": "registered",
        "registered_at": now.to_rfc3339(),
    }))
}

async fn handle_deregister_agent(state: &ApiState, args: &serde_json::Value) -> McpToolResult {
    let agent_id = match get_string_arg(args, "agent_id") {
        Some(id) => id,
        None => return error_result("Missing required argument: agent_id".to_string()),
    };

    let uuid = match Uuid::parse_str(&agent_id) {
        Ok(id) => id,
        Err(_) => return error_result(format!("Invalid UUID: {}", agent_id)),
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
    let agent_id = match get_string_arg(args, "agent_id") {
        Some(id) => id,
        None => return error_result("Missing required argument: agent_id".to_string()),
    };

    let uuid = match Uuid::parse_str(&agent_id) {
        Ok(id) => id,
        Err(_) => return error_result(format!("Invalid UUID: {}", agent_id)),
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
    let uptime = (chrono::Utc::now() - state.started_at()).num_seconds().max(0) as u64;

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
    let action = match get_string_arg(args, "action") {
        Some(a) => a,
        None => return error_result("Missing required argument: action".to_string()),
    };

    let source = match get_string_arg(args, "source") {
        Some(s) => s,
        None => return error_result("Missing required argument: source".to_string()),
    };

    let agent = get_optional_string_arg(args, "agent");
    let outcome = get_optional_string_arg(args, "outcome")
        .unwrap_or_else(|| "unknown".to_string());
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
        buffer.remove(0);
    }
    buffer.push(event);

    success_result(serde_json::json!({
        "status": "accepted",
        "buffered": buffer.len(),
    }))
}

async fn handle_memory_get(state: &ApiState, args: &serde_json::Value) -> McpToolResult {
    let agent_id = match get_string_arg(args, "agent_id") {
        Some(id) => id,
        None => return error_result("Missing required argument: agent_id".to_string()),
    };

    let key = match get_string_arg(args, "key") {
        Some(k) => k,
        None => return error_result("Missing required argument: key".to_string()),
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
    let agent_id = match get_string_arg(args, "agent_id") {
        Some(id) => id,
        None => return error_result("Missing required argument: agent_id".to_string()),
    };

    let key = match get_string_arg(args, "key") {
        Some(k) => k,
        None => return error_result("Missing required argument: key".to_string()),
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

    async fn call_tool(router: &axum::Router, name: &str, args: serde_json::Value) -> McpToolResult {
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
        let body = axum::body::to_bytes(resp.into_body(), 1_048_576).await.unwrap();
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

        let body = axum::body::to_bytes(resp.into_body(), 1_048_576).await.unwrap();
        let manifest: McpToolManifest = serde_json::from_slice(&body).unwrap();
        assert_eq!(manifest.tools.len(), 10);
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
        let result = call_tool(
            &router,
            "agnos_register_agent",
            serde_json::json!({}),
        )
        .await;
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
}
