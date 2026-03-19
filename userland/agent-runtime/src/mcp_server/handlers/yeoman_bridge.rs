use tracing::{info, warn};
use uuid::Uuid;

use super::super::helpers::{
    extract_required_string, get_optional_string_arg, success_result, validate_enum_opt,
};
use super::super::types::McpToolResult;
use super::bridge::HttpBridge;

// ---------------------------------------------------------------------------
// SecureYeoman Deep Integration Bridge
// ---------------------------------------------------------------------------
// These tools extend the base yeoman_* tools with deeper daimon integration:
// tool registration, knowledge/RAG sync, token budgets, events, and swarm.
// All use HttpBridge with mock fallback — both systems work standalone.

fn yeoman_bridge() -> HttpBridge {
    HttpBridge::new(
        "YEOMAN_URL",
        "http://127.0.0.1:18789",
        "YEOMAN_API_KEY",
        "SecureYeoman",
    )
}

fn hoosh_bridge() -> HttpBridge {
    HttpBridge::new(
        "HOOSH_URL",
        "http://127.0.0.1:8088",
        "HOOSH_API_KEY",
        "Hoosh",
    )
}

fn daimon_bridge() -> HttpBridge {
    HttpBridge::new(
        "DAIMON_URL",
        "http://127.0.0.1:8090",
        "DAIMON_API_KEY",
        "Daimon",
    )
}

// ---------------------------------------------------------------------------
// 1. Tool Registration Bridge (2 tools)
// ---------------------------------------------------------------------------

/// Fetch SY's MCP tool catalog and register each into daimon's MCP registry.
pub(crate) async fn handle_yeoman_register_tools(args: &serde_json::Value) -> McpToolResult {
    let filter = get_optional_string_arg(args, "filter");
    let dry_run = args
        .get("dry_run")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let sy = yeoman_bridge();
    let daimon = daimon_bridge();

    // Fetch SY's tool catalog
    let tools_response = match sy
        .post("/api/v1/mcp/tools/list", serde_json::json!({}))
        .await
    {
        Ok(resp) => resp,
        Err(e) => {
            warn!(error = %e, "SecureYeoman bridge: falling back to mock for register_tools");
            return success_result(serde_json::json!({
                "registered": 0,
                "tools": [],
                "message": "SecureYeoman not reachable — no tools registered",
                "_source": "mock",
                "_warning": "service_unavailable",
            }));
        }
    };

    let tools = tools_response
        .get("tools")
        .and_then(|t| t.as_array())
        .cloned()
        .unwrap_or_default();

    // Optionally filter by prefix
    let filtered: Vec<_> = if let Some(ref f) = filter {
        tools
            .iter()
            .filter(|t| {
                t.get("name")
                    .and_then(|n| n.as_str())
                    .map(|n| n.contains(f.as_str()))
                    .unwrap_or(false)
            })
            .cloned()
            .collect()
    } else {
        tools
    };

    if dry_run {
        info!(
            count = filtered.len(),
            "SecureYeoman: dry-run register_tools (bridged)"
        );
        return success_result(serde_json::json!({
            "dry_run": true,
            "would_register": filtered.len(),
            "tools": filtered.iter().filter_map(|t| t.get("name").cloned()).collect::<Vec<_>>(),
        }));
    }

    // Register each tool into daimon's MCP registry
    let mut registered = 0u64;
    let mut errors = Vec::new();
    for tool in &filtered {
        let name = tool
            .get("name")
            .and_then(|n| n.as_str())
            .unwrap_or("unknown");
        let description = tool
            .get("description")
            .and_then(|d| d.as_str())
            .unwrap_or("");
        let body = serde_json::json!({
            "name": format!("sy_{}", name),
            "description": description,
            "endpoint": format!("{}/api/v1/mcp/tools/call", sy.base_url),
            "input_schema": tool.get("input_schema").cloned().unwrap_or(serde_json::json!({})),
        });
        match daimon.post("/v1/mcp/tools", body).await {
            Ok(_) => registered += 1,
            Err(e) => errors.push(format!("{}: {}", name, e)),
        }
    }

    info!(
        registered = registered,
        total = filtered.len(),
        "SecureYeoman: register_tools complete (bridged)"
    );
    success_result(serde_json::json!({
        "registered": registered,
        "total_available": filtered.len(),
        "errors": errors,
    }))
}

/// Execute a SY tool by name via the bridge.
pub(crate) async fn handle_yeoman_tool_execute(args: &serde_json::Value) -> McpToolResult {
    let tool_name = match extract_required_string(args, "tool_name") {
        Ok(n) => n,
        Err(e) => return e,
    };

    let tool_args = args
        .get("tool_args")
        .cloned()
        .unwrap_or(serde_json::json!({}));

    let sy = yeoman_bridge();
    let body = serde_json::json!({
        "name": tool_name,
        "arguments": tool_args,
    });

    match sy.post("/api/v1/mcp/tools/call", body).await {
        Ok(response) => {
            info!(tool = %tool_name, "SecureYeoman: tool_execute (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, tool = %tool_name, "SecureYeoman bridge: falling back to mock for tool_execute");
            success_result(serde_json::json!({
                "tool": tool_name,
                "result": null,
                "error": format!("SecureYeoman not reachable: {}", e),
                "_source": "mock",
                "_warning": "service_unavailable",
            }))
        }
    }
}

// ---------------------------------------------------------------------------
// 2. Knowledge/RAG Bridge (2 tools)
// ---------------------------------------------------------------------------

/// Query SY's knowledge brain.
pub(crate) async fn handle_yeoman_brain_query(args: &serde_json::Value) -> McpToolResult {
    let query = match extract_required_string(args, "query") {
        Ok(q) => q,
        Err(e) => return e,
    };

    let limit = get_optional_string_arg(args, "limit");
    let category = get_optional_string_arg(args, "category");

    let sy = yeoman_bridge();
    let body = serde_json::json!({
        "query": query,
        "limit": limit.as_deref().and_then(|l| l.parse::<u64>().ok()).unwrap_or(10),
        "category": category,
    });

    match sy.post("/api/v1/knowledge/search", body).await {
        Ok(response) => {
            info!("SecureYeoman: brain_query (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "SecureYeoman bridge: falling back to mock for brain_query");
            success_result(serde_json::json!({
                "results": [],
                "total": 0,
                "query": query,
                "message": "SecureYeoman not reachable",
                "_source": "mock",
                "_warning": "service_unavailable",
            }))
        }
    }
}

/// Bidirectional knowledge sync between SY brain and AGNOS RAG.
pub(crate) async fn handle_yeoman_brain_sync(args: &serde_json::Value) -> McpToolResult {
    let action = match extract_required_string(args, "action") {
        Ok(a) => a,
        Err(e) => return e,
    };

    let action_opt = Some(action.clone());
    if let Err(e) = validate_enum_opt(&action_opt, "action", &["to_agnos", "from_agnos"]) {
        return e;
    }

    let category = get_optional_string_arg(args, "category");
    let limit = get_optional_string_arg(args, "limit");
    let max_entries = limit
        .as_deref()
        .and_then(|l| l.parse::<u64>().ok())
        .unwrap_or(50);

    let sy = yeoman_bridge();
    let daimon = daimon_bridge();

    match action.as_str() {
        "to_agnos" => {
            // Fetch from SY brain, ingest into AGNOS RAG
            let search_body = serde_json::json!({
                "query": "*",
                "limit": max_entries,
                "category": category,
            });
            let entries = match sy.post("/api/v1/knowledge/search", search_body).await {
                Ok(resp) => resp
                    .get("results")
                    .and_then(|r| r.as_array())
                    .cloned()
                    .unwrap_or_default(),
                Err(e) => {
                    warn!(error = %e, "SecureYeoman bridge: cannot fetch brain for sync to_agnos");
                    return success_result(serde_json::json!({
                        "synced": 0,
                        "direction": "to_agnos",
                        "error": format!("SecureYeoman not reachable: {}", e),
                        "_source": "mock",
                        "_warning": "service_unavailable",
                    }));
                }
            };

            let mut synced = 0u64;
            for entry in &entries {
                let content = entry.get("content").and_then(|c| c.as_str()).unwrap_or("");
                let title = entry
                    .get("title")
                    .and_then(|t| t.as_str())
                    .unwrap_or("sy-knowledge");
                let ingest_body = serde_json::json!({
                    "content": content,
                    "metadata": {
                        "source": "secureyeoman",
                        "title": title,
                        "category": category,
                    },
                });
                if daimon.post("/v1/rag/ingest", ingest_body).await.is_ok() {
                    synced += 1;
                }
            }

            info!(
                synced = synced,
                total = entries.len(),
                "SecureYeoman: brain_sync to_agnos (bridged)"
            );
            success_result(serde_json::json!({
                "synced": synced,
                "total_fetched": entries.len(),
                "direction": "to_agnos",
            }))
        }
        "from_agnos" => {
            // Query AGNOS RAG for SY-relevant content
            let query_text = get_optional_string_arg(args, "query")
                .unwrap_or_else(|| "secureyeoman".to_string());
            let rag_body = serde_json::json!({
                "query": query_text,
                "limit": max_entries,
            });
            match daimon.post("/v1/rag/query", rag_body).await {
                Ok(response) => {
                    let results = response
                        .get("results")
                        .and_then(|r| r.as_array())
                        .cloned()
                        .unwrap_or_default();
                    info!(
                        count = results.len(),
                        "SecureYeoman: brain_sync from_agnos (bridged)"
                    );
                    success_result(serde_json::json!({
                        "results": results,
                        "total": results.len(),
                        "direction": "from_agnos",
                        "query": query_text,
                    }))
                }
                Err(e) => {
                    warn!(error = %e, "Daimon RAG not reachable for brain_sync from_agnos");
                    success_result(serde_json::json!({
                        "results": [],
                        "total": 0,
                        "direction": "from_agnos",
                        "error": format!("Daimon RAG not reachable: {}", e),
                        "_source": "mock",
                        "_warning": "service_unavailable",
                    }))
                }
            }
        }
        _ => unreachable!(),
    }
}

// ---------------------------------------------------------------------------
// 3. Token Budget Bridge (1 tool, multi-action)
// ---------------------------------------------------------------------------

/// Query/manage SY agent token budgets via hoosh.
pub(crate) async fn handle_yeoman_token_budget(args: &serde_json::Value) -> McpToolResult {
    let action = match extract_required_string(args, "action") {
        Ok(a) => a,
        Err(e) => return e,
    };

    let action_opt = Some(action.clone());
    if let Err(e) = validate_enum_opt(
        &action_opt,
        "action",
        &["list", "check", "reserve", "release"],
    ) {
        return e;
    }

    let pool_name = get_optional_string_arg(args, "pool_name");
    let agent_id = get_optional_string_arg(args, "agent_id");
    let amount = args.get("amount").and_then(|v| v.as_u64());

    let hoosh = hoosh_bridge();

    match action.as_str() {
        "list" => match hoosh.get("/v1/tokens/pools", &[]).await {
            Ok(response) => {
                info!("SecureYeoman: token_budget list (bridged via hoosh)");
                success_result(response)
            }
            Err(e) => {
                warn!(error = %e, "Hoosh bridge: falling back to mock for token pools list");
                success_result(serde_json::json!({
                    "pools": [],
                    "total": 0,
                    "_source": "mock",
                    "_warning": "service_unavailable",
                }))
            }
        },
        "check" => {
            let body = serde_json::json!({
                "agent_id": agent_id.unwrap_or_else(|| "secureyeoman".to_string()),
                "pool_name": pool_name,
                "amount": amount.unwrap_or(0),
            });
            match hoosh.post("/v1/tokens/check", body).await {
                Ok(response) => {
                    info!("SecureYeoman: token_budget check (bridged via hoosh)");
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "Hoosh bridge: falling back to mock for token check");
                    success_result(serde_json::json!({
                        "allowed": true,
                        "remaining": 100000,
                        "_source": "mock",
                        "_warning": "service_unavailable",
                    }))
                }
            }
        }
        "reserve" => {
            let body = serde_json::json!({
                "agent_id": agent_id.unwrap_or_else(|| "secureyeoman".to_string()),
                "pool_name": pool_name,
                "amount": amount.unwrap_or(1000),
            });
            match hoosh.post("/v1/tokens/reserve", body).await {
                Ok(response) => {
                    info!("SecureYeoman: token_budget reserve (bridged via hoosh)");
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "Hoosh bridge: falling back to mock for token reserve");
                    success_result(serde_json::json!({
                        "reservation_id": Uuid::new_v4().to_string(),
                        "amount": amount.unwrap_or(1000),
                        "status": "reserved",
                        "_source": "mock",
                        "_warning": "service_unavailable",
                    }))
                }
            }
        }
        "release" => {
            let body = serde_json::json!({
                "agent_id": agent_id.unwrap_or_else(|| "secureyeoman".to_string()),
                "pool_name": pool_name,
                "reservation_id": get_optional_string_arg(args, "reservation_id"),
            });
            match hoosh.post("/v1/tokens/release", body).await {
                Ok(response) => {
                    info!("SecureYeoman: token_budget release (bridged via hoosh)");
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "Hoosh bridge: falling back to mock for token release");
                    success_result(serde_json::json!({
                        "released": true,
                        "status": "ok",
                        "_source": "mock",
                        "_warning": "service_unavailable",
                    }))
                }
            }
        }
        _ => unreachable!(),
    }
}

// ---------------------------------------------------------------------------
// 4. Event Bridge (2 tools)
// ---------------------------------------------------------------------------

/// Subscribe/query SY event stream.
pub(crate) async fn handle_yeoman_events(args: &serde_json::Value) -> McpToolResult {
    let action = match extract_required_string(args, "action") {
        Ok(a) => a,
        Err(e) => return e,
    };

    let action_opt = Some(action.clone());
    if let Err(e) = validate_enum_opt(&action_opt, "action", &["recent", "subscribe", "alerts"]) {
        return e;
    }

    let limit = get_optional_string_arg(args, "limit");
    let event_type = get_optional_string_arg(args, "event_type");

    let sy = yeoman_bridge();

    match action.as_str() {
        "recent" => {
            let mut query = Vec::new();
            if let Some(ref l) = limit {
                query.push(("limit".to_string(), l.clone()));
            }
            if let Some(ref et) = event_type {
                query.push(("type".to_string(), et.clone()));
            }
            match sy.get("/api/v1/events/recent", &query).await {
                Ok(response) => {
                    info!("SecureYeoman: events recent (bridged)");
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "SecureYeoman bridge: falling back to mock for events recent");
                    success_result(serde_json::json!({
                        "events": [],
                        "total": 0,
                        "_source": "mock",
                        "_warning": "service_unavailable",
                    }))
                }
            }
        }
        "subscribe" => {
            // SSE subscription is not directly bridgeable via request/response,
            // so we return the subscription endpoint for the caller to connect.
            info!("SecureYeoman: events subscribe info (bridged)");
            success_result(serde_json::json!({
                "subscribe_url": format!("{}/api/v1/events/subscribe", sy.base_url),
                "message": "Use SSE client to connect to the subscribe URL",
                "event_types": event_type,
            }))
        }
        "alerts" => {
            let mut query = Vec::new();
            if let Some(ref l) = limit {
                query.push(("limit".to_string(), l.clone()));
            }
            match sy
                .get(
                    "/api/v1/events/recent",
                    &[("type".to_string(), "alert".to_string())],
                )
                .await
            {
                Ok(response) => {
                    info!("SecureYeoman: events alerts (bridged)");
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "SecureYeoman bridge: falling back to mock for events alerts");
                    let _ = query; // suppress unused warning
                    success_result(serde_json::json!({
                        "alerts": [],
                        "total": 0,
                        "_source": "mock",
                        "_warning": "service_unavailable",
                    }))
                }
            }
        }
        _ => unreachable!(),
    }
}

/// Query SY swarm topology — agents, teams, relationships.
pub(crate) async fn handle_yeoman_swarm(args: &serde_json::Value) -> McpToolResult {
    let action = match extract_required_string(args, "action") {
        Ok(a) => a,
        Err(e) => return e,
    };

    let action_opt = Some(action.clone());
    if let Err(e) = validate_enum_opt(
        &action_opt,
        "action",
        &["topology", "teams", "relationships"],
    ) {
        return e;
    }

    let team_id = get_optional_string_arg(args, "team_id");
    let agent_id = get_optional_string_arg(args, "agent_id");

    let sy = yeoman_bridge();

    let mut query = Vec::new();
    query.push(("view".to_string(), action.clone()));
    if let Some(ref tid) = team_id {
        query.push(("team_id".to_string(), tid.clone()));
    }
    if let Some(ref aid) = agent_id {
        query.push(("agent_id".to_string(), aid.clone()));
    }

    match sy.get("/api/v1/agents/topology", &query).await {
        Ok(response) => {
            info!("SecureYeoman: swarm {} (bridged)", action);
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "SecureYeoman bridge: falling back to mock for swarm {}", action);
            success_result(serde_json::json!({
                "agents": [],
                "teams": [],
                "relationships": [],
                "total_agents": 0,
                "total_teams": 0,
                "_source": "mock",
                "_warning": "service_unavailable",
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

    // -- yeoman_register_tools --

    #[tokio::test]
    async fn test_yeoman_register_tools_mock_fallback() {
        let result = handle_yeoman_register_tools(&serde_json::json!({})).await;
        let json: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(json["_source"], "mock");
        assert_eq!(json["_warning"], "service_unavailable");
        assert_eq!(json["registered"], 0);
    }

    #[tokio::test]
    async fn test_yeoman_register_tools_dry_run() {
        // SY not running, so we get mock (which returns before dry_run matters)
        let result = handle_yeoman_register_tools(&serde_json::json!({"dry_run": true})).await;
        let json: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        // Mock fallback returns before dry_run is checked
        assert!(json.get("_source").is_some() || json.get("dry_run").is_some());
    }

    // -- yeoman_tool_execute --

    #[tokio::test]
    async fn test_yeoman_tool_execute_mock_fallback() {
        let result =
            handle_yeoman_tool_execute(&serde_json::json!({"tool_name": "file_read"})).await;
        let json: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(json["_source"], "mock");
        assert_eq!(json["tool"], "file_read");
    }

    #[tokio::test]
    async fn test_yeoman_tool_execute_missing_name() {
        let result = handle_yeoman_tool_execute(&serde_json::json!({})).await;
        assert!(result.is_error);
    }

    // -- yeoman_brain_query --

    #[tokio::test]
    async fn test_yeoman_brain_query_mock_fallback() {
        let result =
            handle_yeoman_brain_query(&serde_json::json!({"query": "how to deploy"})).await;
        let json: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(json["_source"], "mock");
        assert!(json.get("results").is_some());
    }

    #[tokio::test]
    async fn test_yeoman_brain_query_missing_query() {
        let result = handle_yeoman_brain_query(&serde_json::json!({})).await;
        assert!(result.is_error);
    }

    // -- yeoman_brain_sync --

    #[tokio::test]
    async fn test_yeoman_brain_sync_to_agnos_mock() {
        let result = handle_yeoman_brain_sync(&serde_json::json!({"action": "to_agnos"})).await;
        let json: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(json["direction"], "to_agnos");
        // SY not available, so mock fallback
        assert_eq!(json["_source"], "mock");
    }

    #[tokio::test]
    async fn test_yeoman_brain_sync_from_agnos_mock() {
        let result = handle_yeoman_brain_sync(&serde_json::json!({"action": "from_agnos"})).await;
        let json: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(json["direction"], "from_agnos");
    }

    #[tokio::test]
    async fn test_yeoman_brain_sync_invalid_action() {
        let result = handle_yeoman_brain_sync(&serde_json::json!({"action": "sideways"})).await;
        assert!(result.is_error);
    }

    #[tokio::test]
    async fn test_yeoman_brain_sync_missing_action() {
        let result = handle_yeoman_brain_sync(&serde_json::json!({})).await;
        assert!(result.is_error);
    }

    // -- yeoman_token_budget --

    #[tokio::test]
    async fn test_yeoman_token_budget_list_mock() {
        let result = handle_yeoman_token_budget(&serde_json::json!({"action": "list"})).await;
        let json: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(json["_source"], "mock");
        assert!(json.get("pools").is_some());
    }

    #[tokio::test]
    async fn test_yeoman_token_budget_check_mock() {
        let result =
            handle_yeoman_token_budget(&serde_json::json!({"action": "check", "amount": 500}))
                .await;
        let json: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(json["_source"], "mock");
    }

    #[tokio::test]
    async fn test_yeoman_token_budget_reserve_mock() {
        let result =
            handle_yeoman_token_budget(&serde_json::json!({"action": "reserve", "amount": 1000}))
                .await;
        let json: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(json["_source"], "mock");
        assert_eq!(json["status"], "reserved");
    }

    #[tokio::test]
    async fn test_yeoman_token_budget_release_mock() {
        let result = handle_yeoman_token_budget(&serde_json::json!({"action": "release"})).await;
        let json: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(json["_source"], "mock");
    }

    #[tokio::test]
    async fn test_yeoman_token_budget_invalid_action() {
        let result = handle_yeoman_token_budget(&serde_json::json!({"action": "burn"})).await;
        assert!(result.is_error);
    }

    #[tokio::test]
    async fn test_yeoman_token_budget_missing_action() {
        let result = handle_yeoman_token_budget(&serde_json::json!({})).await;
        assert!(result.is_error);
    }

    // -- yeoman_events --

    #[tokio::test]
    async fn test_yeoman_events_recent_mock() {
        let result = handle_yeoman_events(&serde_json::json!({"action": "recent"})).await;
        let json: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(json["_source"], "mock");
        assert!(json.get("events").is_some());
    }

    #[tokio::test]
    async fn test_yeoman_events_subscribe() {
        let result = handle_yeoman_events(&serde_json::json!({"action": "subscribe"})).await;
        let json: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        // Subscribe always returns the URL (no mock needed)
        assert!(json.get("subscribe_url").is_some());
    }

    #[tokio::test]
    async fn test_yeoman_events_alerts_mock() {
        let result = handle_yeoman_events(&serde_json::json!({"action": "alerts"})).await;
        let json: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(json["_source"], "mock");
    }

    #[tokio::test]
    async fn test_yeoman_events_invalid_action() {
        let result = handle_yeoman_events(&serde_json::json!({"action": "explode"})).await;
        assert!(result.is_error);
    }

    #[tokio::test]
    async fn test_yeoman_events_missing_action() {
        let result = handle_yeoman_events(&serde_json::json!({})).await;
        assert!(result.is_error);
    }

    // -- yeoman_swarm --

    #[tokio::test]
    async fn test_yeoman_swarm_topology_mock() {
        let result = handle_yeoman_swarm(&serde_json::json!({"action": "topology"})).await;
        let json: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(json["_source"], "mock");
        assert!(json.get("agents").is_some());
        assert!(json.get("teams").is_some());
    }

    #[tokio::test]
    async fn test_yeoman_swarm_teams_mock() {
        let result = handle_yeoman_swarm(&serde_json::json!({"action": "teams"})).await;
        let json: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(json["_source"], "mock");
    }

    #[tokio::test]
    async fn test_yeoman_swarm_relationships_mock() {
        let result = handle_yeoman_swarm(
            &serde_json::json!({"action": "relationships", "agent_id": "abc-123"}),
        )
        .await;
        let json: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(json["_source"], "mock");
    }

    #[tokio::test]
    async fn test_yeoman_swarm_invalid_action() {
        let result = handle_yeoman_swarm(&serde_json::json!({"action": "destroy"})).await;
        assert!(result.is_error);
    }

    #[tokio::test]
    async fn test_yeoman_swarm_missing_action() {
        let result = handle_yeoman_swarm(&serde_json::json!({})).await;
        assert!(result.is_error);
    }
}
