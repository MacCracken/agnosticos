use super::handlers::edge::{
    handle_edge_decommission, handle_edge_deploy, handle_edge_health, handle_edge_list,
    handle_edge_update,
};
use super::handlers::shruti::{
    handle_shruti_export, handle_shruti_mixer, handle_shruti_session, handle_shruti_tracks,
    handle_shruti_transport,
};
use super::helpers::{
    extract_optional_u64, extract_required_string, extract_required_uuid, success_result,
    validate_enum_opt,
};
use super::manifest::build_tool_manifest;
use super::*;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

fn test_state() -> ApiState {
    ApiState::with_api_key(None)
}

fn parse_result(result: &McpToolResult) -> serde_json::Value {
    serde_json::from_str(&result.content[0].text).unwrap_or_default()
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
    assert_eq!(manifest.tools.len(), 119);
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
async fn test_manifest_contains_all_tools() {
    let manifest = build_tool_manifest();
    assert_eq!(manifest.tools.len(), 119);
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
        "edge_list",
        "edge_deploy",
        "edge_update",
        "edge_health",
        "edge_decommission",
        "shruti_session",
        "shruti_tracks",
        "shruti_mixer",
        "shruti_transport",
        "shruti_export",
        "tazama_project",
        "tazama_timeline",
        "tazama_effects",
        "tazama_ai",
        "tazama_export",
        "rasa_canvas",
        "rasa_layers",
        "rasa_tools",
        "rasa_ai",
        "rasa_export",
        "mneme_notebook",
        "mneme_notes",
        "mneme_search",
        "mneme_ai",
        "mneme_graph",
        "synapse_models",
        "synapse_serve",
        "synapse_finetune",
        "synapse_chat",
        "synapse_status",
        "bullshift_portfolio",
        "bullshift_orders",
        "bullshift_market",
        "bullshift_alerts",
        "bullshift_strategy",
        "yeoman_agents",
        "yeoman_tasks",
        "yeoman_tools",
        "yeoman_integrations",
        "yeoman_status",
        "synapse_benchmark",
        "synapse_quantize",
        "bullshift_accounts",
        "bullshift_history",
        "yeoman_logs",
        "yeoman_workflows",
        "delta_branches",
        "delta_review",
        "aequi_invoices",
        "aequi_reports",
        "agnostic_coverage",
        "agnostic_schedule",
        "agnostic_run_crew",
        "agnostic_crew_status",
        "agnostic_list_presets",
        "agnostic_list_definitions",
        "agnostic_create_agent",
        "shruti_plugins",
        "shruti_ai",
        "tazama_media",
        "tazama_subtitles",
        "rasa_batch",
        "rasa_templates",
        "rasa_adjustments",
        "mneme_import",
        "mneme_tags",
        "photis_boards",
        "photis_notes",
        "edge_logs",
        "edge_config",
        "phylax_scan",
        "phylax_status",
        "phylax_rules",
        "phylax_findings",
        "phylax_quarantine",
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
    // Test via build_tool_manifest which uses json_schema_object internally
    let manifest = build_tool_manifest();
    // agnos_health has no properties
    let health = manifest
        .tools
        .iter()
        .find(|t| t.name == "agnos_health")
        .unwrap();
    assert_eq!(health.input_schema["type"], "object");
    assert!(health.input_schema["properties"]
        .as_object()
        .unwrap()
        .is_empty());
    assert!(health.input_schema["required"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
fn test_json_schema_object_with_properties() {
    let manifest = build_tool_manifest();
    // agnos_get_agent has properties and required
    let get_agent = manifest
        .tools
        .iter()
        .find(|t| t.name == "agnos_get_agent")
        .unwrap();
    assert_eq!(get_agent.input_schema["type"], "object");
    assert_eq!(
        get_agent.input_schema["properties"]["agent_id"]["type"],
        "string"
    );
    assert_eq!(get_agent.input_schema["required"][0], "agent_id");
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
    let req = Request::builder()
        .method("POST")
        .uri("/v1/mcp/tools")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();
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
    let req = Request::builder()
        .method("POST")
        .uri("/v1/mcp/tools")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();
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
    let req = Request::builder()
        .method("POST")
        .uri("/v1/mcp/tools")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();
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
    let req = Request::builder()
        .method("POST")
        .uri("/v1/mcp/tools")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();
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
    let req = Request::builder()
        .method("POST")
        .uri("/v1/mcp/tools")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();
    let resp = router.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn test_dispatch_blocks_ssrf_link_local() {
    let ext = ExternalMcpTool {
        tool: McpToolDescription {
            name: "evil".into(),
            description: "t".into(),
            input_schema: serde_json::json!({}),
        },
        callback_url: "http://169.254.169.254/latest/meta-data/".into(),
        source: "test".into(),
    };
    let call = McpToolCall {
        name: "evil".into(),
        arguments: serde_json::json!({}),
    };
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
    assert!(err.content[0]
        .text
        .contains("Missing required argument: name"));
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

// -----------------------------------------------------------------------
// Edge fleet MCP tool tests (Phase 14E)
// -----------------------------------------------------------------------

#[tokio::test]
async fn test_edge_list_empty_fleet() {
    let state = test_state();
    let result = handle_edge_list(&state, &serde_json::json!({})).await;
    assert!(!result.is_error);
    let parsed = parse_result(&result);
    assert_eq!(parsed["total"], 0);
}

#[tokio::test]
async fn test_edge_list_with_nodes() {
    let state = test_state();
    {
        let mut fleet = state.edge_fleet.write().await;
        fleet
            .register_node(
                "test-rpi".into(),
                crate::edge::EdgeCapabilities::default(),
                "secureyeoman-edge".into(),
                "1.0".into(),
                "2026.3.11".into(),
                "http://parent:8090".into(),
            )
            .unwrap();
    }
    let result = handle_edge_list(&state, &serde_json::json!({})).await;
    assert!(!result.is_error);
    let parsed = parse_result(&result);
    assert_eq!(parsed["total"], 1);
    assert_eq!(parsed["nodes"][0]["name"], "test-rpi");
}

#[tokio::test]
async fn test_edge_list_filter_status() {
    let state = test_state();
    let result = handle_edge_list(&state, &serde_json::json!({"status": "online"})).await;
    assert!(!result.is_error);
}

#[tokio::test]
async fn test_edge_list_invalid_status() {
    let state = test_state();
    let result = handle_edge_list(&state, &serde_json::json!({"status": "invalid"})).await;
    assert!(result.is_error);
}

#[tokio::test]
async fn test_edge_health_fleet_wide() {
    let state = test_state();
    let result = handle_edge_health(&state, &serde_json::json!({})).await;
    assert!(!result.is_error);
    let parsed = parse_result(&result);
    assert_eq!(parsed["fleet"]["total_nodes"], 0);
}

#[tokio::test]
async fn test_edge_health_specific_node() {
    let state = test_state();
    let node_id;
    {
        let mut fleet = state.edge_fleet.write().await;
        node_id = fleet
            .register_node(
                "health-test".into(),
                crate::edge::EdgeCapabilities::default(),
                "edge".into(),
                "1.0".into(),
                "1.0".into(),
                "http://parent:8090".into(),
            )
            .unwrap();
    }
    let result = handle_edge_health(&state, &serde_json::json!({"node_id": node_id})).await;
    assert!(!result.is_error);
    let parsed = parse_result(&result);
    assert_eq!(parsed["name"], "health-test");
    assert_eq!(parsed["status"], "online");
}

#[tokio::test]
async fn test_edge_health_unknown_node() {
    let state = test_state();
    let result = handle_edge_health(&state, &serde_json::json!({"node_id": "nonexistent"})).await;
    assert!(result.is_error);
}

#[tokio::test]
async fn test_edge_deploy_no_nodes() {
    let state = test_state();
    let result = handle_edge_deploy(&state, &serde_json::json!({"task": "run-scan"})).await;
    assert!(result.is_error);
}

#[tokio::test]
async fn test_edge_deploy_auto_route() {
    let state = test_state();
    {
        let mut fleet = state.edge_fleet.write().await;
        fleet
            .register_node(
                "deploy-target".into(),
                crate::edge::EdgeCapabilities::default(),
                "edge".into(),
                "1.0".into(),
                "1.0".into(),
                "http://parent:8090".into(),
            )
            .unwrap();
    }
    let result = handle_edge_deploy(&state, &serde_json::json!({"task": "run-scan"})).await;
    assert!(!result.is_error);
    let parsed = parse_result(&result);
    assert_eq!(parsed["status"], "deployed");
    assert_eq!(parsed["node_name"], "deploy-target");
}

#[tokio::test]
async fn test_edge_deploy_missing_task() {
    let state = test_state();
    let result = handle_edge_deploy(&state, &serde_json::json!({})).await;
    assert!(result.is_error);
}

#[tokio::test]
async fn test_edge_update_success() {
    let state = test_state();
    let node_id;
    {
        let mut fleet = state.edge_fleet.write().await;
        node_id = fleet
            .register_node(
                "update-node".into(),
                crate::edge::EdgeCapabilities::default(),
                "edge".into(),
                "1.0".into(),
                "1.0".into(),
                "http://parent:8090".into(),
            )
            .unwrap();
    }
    let result = handle_edge_update(
        &state,
        &serde_json::json!({"node_id": node_id, "version": "2.0"}),
    )
    .await;
    assert!(!result.is_error);
    let parsed = parse_result(&result);
    assert_eq!(parsed["status"], "updating");
}

#[tokio::test]
async fn test_edge_update_missing_node_id() {
    let state = test_state();
    let result = handle_edge_update(&state, &serde_json::json!({})).await;
    assert!(result.is_error);
}

#[tokio::test]
async fn test_edge_decommission_success() {
    let state = test_state();
    let node_id;
    {
        let mut fleet = state.edge_fleet.write().await;
        node_id = fleet
            .register_node(
                "decom-node".into(),
                crate::edge::EdgeCapabilities::default(),
                "edge".into(),
                "1.0".into(),
                "1.0".into(),
                "http://parent:8090".into(),
            )
            .unwrap();
    }
    let result = handle_edge_decommission(&state, &serde_json::json!({"node_id": node_id})).await;
    assert!(!result.is_error);
    let parsed = parse_result(&result);
    assert_eq!(parsed["status"], "decommissioned");
}

#[tokio::test]
async fn test_edge_decommission_missing_id() {
    let state = test_state();
    let result = handle_edge_decommission(&state, &serde_json::json!({})).await;
    assert!(result.is_error);
}

#[tokio::test]
async fn test_edge_decommission_unknown() {
    let state = test_state();
    let result = handle_edge_decommission(&state, &serde_json::json!({"node_id": "fake"})).await;
    assert!(result.is_error);
}

#[tokio::test]
async fn test_edge_tools_in_manifest() {
    let manifest = build_tool_manifest();
    let names: Vec<&str> = manifest.tools.iter().map(|t| t.name.as_str()).collect();
    assert!(names.contains(&"edge_list"));
    assert!(names.contains(&"edge_deploy"));
    assert!(names.contains(&"edge_update"));
    assert!(names.contains(&"edge_health"));
    assert!(names.contains(&"edge_decommission"));
}

// -----------------------------------------------------------------------
// Shruti DAW MCP tools
// -----------------------------------------------------------------------

#[tokio::test]
async fn test_shruti_session_create_mock() {
    let result =
        handle_shruti_session(&serde_json::json!({"action": "create", "name": "demo"})).await;
    assert!(!result.is_error);
    let parsed = parse_result(&result);
    assert_eq!(parsed["action"], "create");
    assert_eq!(parsed["name"], "demo");
    assert_eq!(parsed["_source"], "mock");
}

#[tokio::test]
async fn test_shruti_session_list_mock() {
    let result = handle_shruti_session(&serde_json::json!({"action": "list"})).await;
    assert!(!result.is_error);
    let parsed = parse_result(&result);
    assert!(parsed["sessions"].is_array());
}

#[tokio::test]
async fn test_shruti_session_invalid_action() {
    let result = handle_shruti_session(&serde_json::json!({"action": "destroy"})).await;
    assert!(result.is_error);
}

#[tokio::test]
async fn test_shruti_session_missing_action() {
    let result = handle_shruti_session(&serde_json::json!({})).await;
    assert!(result.is_error);
}

#[tokio::test]
async fn test_shruti_tracks_add_mock() {
    let result = handle_shruti_tracks(
        &serde_json::json!({"action": "add", "name": "vocals", "kind": "audio"}),
    )
    .await;
    assert!(!result.is_error);
    let parsed = parse_result(&result);
    assert_eq!(parsed["action"], "add");
    assert_eq!(parsed["name"], "vocals");
    assert_eq!(parsed["kind"], "audio");
}

#[tokio::test]
async fn test_shruti_tracks_list_mock() {
    let result = handle_shruti_tracks(&serde_json::json!({"action": "list"})).await;
    assert!(!result.is_error);
    let parsed = parse_result(&result);
    assert!(parsed["tracks"].is_array());
}

#[tokio::test]
async fn test_shruti_tracks_invalid_action() {
    let result = handle_shruti_tracks(&serde_json::json!({"action": "destroy"})).await;
    assert!(result.is_error);
}

#[tokio::test]
async fn test_shruti_mixer_mock() {
    let result =
        handle_shruti_mixer(&serde_json::json!({"track": "drums", "gain": -6.0, "mute": true}))
            .await;
    assert!(!result.is_error);
    let parsed = parse_result(&result);
    assert_eq!(parsed["track"], "drums");
    assert_eq!(parsed["gain_db"], -6.0);
    assert_eq!(parsed["muted"], true);
}

#[tokio::test]
async fn test_shruti_mixer_missing_track() {
    let result = handle_shruti_mixer(&serde_json::json!({"gain": -3.0})).await;
    assert!(result.is_error);
}

#[tokio::test]
async fn test_shruti_transport_play_mock() {
    let result = handle_shruti_transport(&serde_json::json!({"action": "play"})).await;
    assert!(!result.is_error);
    let parsed = parse_result(&result);
    assert_eq!(parsed["action"], "play");
}

#[tokio::test]
async fn test_shruti_transport_status_mock() {
    let result = handle_shruti_transport(&serde_json::json!({"action": "status"})).await;
    assert!(!result.is_error);
    let parsed = parse_result(&result);
    assert!(parsed.get("state").is_some() || parsed.get("action").is_some());
}

#[tokio::test]
async fn test_shruti_transport_invalid_action() {
    let result = handle_shruti_transport(&serde_json::json!({"action": "rewind"})).await;
    assert!(result.is_error);
}

#[tokio::test]
async fn test_shruti_export_mock() {
    let result =
        handle_shruti_export(&serde_json::json!({"path": "/tmp/out.wav", "format": "wav"})).await;
    assert!(!result.is_error);
    let parsed = parse_result(&result);
    assert_eq!(parsed["path"], "/tmp/out.wav");
    assert_eq!(parsed["format"], "wav");
}

#[tokio::test]
async fn test_shruti_export_invalid_format() {
    let result = handle_shruti_export(&serde_json::json!({"format": "midi"})).await;
    assert!(result.is_error);
}

#[tokio::test]
async fn test_shruti_export_no_args_mock() {
    let result = handle_shruti_export(&serde_json::json!({})).await;
    assert!(!result.is_error);
    let parsed = parse_result(&result);
    assert_eq!(parsed["_source"], "mock");
}

#[tokio::test]
async fn test_shruti_tools_in_manifest() {
    let manifest = build_tool_manifest();
    let names: Vec<&str> = manifest.tools.iter().map(|t| t.name.as_str()).collect();
    assert!(names.contains(&"shruti_session"));
    assert!(names.contains(&"shruti_tracks"));
    assert!(names.contains(&"shruti_mixer"));
    assert!(names.contains(&"shruti_transport"));
    assert!(names.contains(&"shruti_export"));
}

#[tokio::test]
async fn test_shruti_tools_via_http_dispatch() {
    let router = build_test_router();
    let result = call_tool(
        &router,
        "shruti_session",
        serde_json::json!({"action": "info"}),
    )
    .await;
    assert!(!result.is_error);
}
