use tracing::{info, warn};
use uuid::Uuid;

use super::super::helpers::{
    extract_required_string, get_optional_string_arg, success_result, validate_enum_opt,
};
use super::super::types::McpToolResult;
use super::bridge::HttpBridge;

// ---------------------------------------------------------------------------
// SecureYeoman Agent Platform Bridge
// ---------------------------------------------------------------------------

pub(crate) fn yeoman_bridge() -> HttpBridge {
    HttpBridge::new(
        "YEOMAN_URL",
        "http://127.0.0.1:18789",
        "YEOMAN_API_KEY",
        "SecureYeoman",
    )
}

// ---------------------------------------------------------------------------
// SecureYeoman Tool Implementations (bridged)
// ---------------------------------------------------------------------------

pub(crate) async fn handle_yeoman_agents(args: &serde_json::Value) -> McpToolResult {
    let action = match extract_required_string(args, "action") {
        Ok(a) => a,
        Err(e) => return e,
    };

    let action_opt = Some(action.clone());
    if let Err(e) = validate_enum_opt(
        &action_opt,
        "action",
        &["list", "deploy", "stop", "status", "info"],
    ) {
        return e;
    }

    let agent_id = get_optional_string_arg(args, "agent_id");
    let name = get_optional_string_arg(args, "name");
    let template = get_optional_string_arg(args, "template");
    let bridge = yeoman_bridge();

    match action.as_str() {
        "list" | "status" | "info" => {
            let mut query = Vec::new();
            if let Some(ref id) = agent_id {
                query.push(("agent_id".to_string(), id.clone()));
            }
            if let Some(ref n) = name {
                query.push(("name".to_string(), n.clone()));
            }
            query.push(("action".to_string(), action.clone()));
            match bridge.get("/api/v1/agents", &query).await {
                Ok(response) => {
                    info!("SecureYeoman: {} agents (bridged)", action);
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "SecureYeoman bridge: falling back to mock for agents {}", action);
                    success_result(serde_json::json!({
                        "agents": [],
                        "total": 0,
                        "_source": "mock",
                    }))
                }
            }
        }
        op @ ("deploy" | "stop") => {
            let body = serde_json::json!({
                "action": op,
                "agent_id": agent_id,
                "name": name,
                "template": template,
            });
            match bridge.post("/api/v1/agents", body).await {
                Ok(response) => {
                    info!(action = %op, "SecureYeoman: {} agent (bridged)", op);
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "SecureYeoman bridge: falling back to mock for {} agent", op);
                    let id = agent_id.unwrap_or_else(|| Uuid::new_v4().to_string());
                    success_result(serde_json::json!({
                        "agent_id": id,
                        "action": op,
                        "name": name.unwrap_or_else(|| "unnamed-agent".to_string()),
                        "template": template,
                        "status": "ok",
                        "_source": "mock",
                    }))
                }
            }
        }
        _ => unreachable!(),
    }
}

pub(crate) async fn handle_yeoman_tasks(args: &serde_json::Value) -> McpToolResult {
    let action = match extract_required_string(args, "action") {
        Ok(a) => a,
        Err(e) => return e,
    };

    let action_opt = Some(action.clone());
    if let Err(e) = validate_enum_opt(
        &action_opt,
        "action",
        &["assign", "list", "status", "cancel"],
    ) {
        return e;
    }

    let agent_id = get_optional_string_arg(args, "agent_id");
    let description = get_optional_string_arg(args, "description");
    let task_id = get_optional_string_arg(args, "task_id");
    let priority = get_optional_string_arg(args, "priority");

    if let Some(ref p) = priority {
        let p_opt = Some(p.clone());
        if let Err(e) = validate_enum_opt(&p_opt, "priority", &["low", "medium", "high"]) {
            return e;
        }
    }

    let bridge = yeoman_bridge();

    match action.as_str() {
        "list" | "status" => {
            let mut query = Vec::new();
            if let Some(ref id) = agent_id {
                query.push(("agent_id".to_string(), id.clone()));
            }
            if let Some(ref tid) = task_id {
                query.push(("task_id".to_string(), tid.clone()));
            }
            query.push(("action".to_string(), action.clone()));
            match bridge.get("/api/v1/tasks", &query).await {
                Ok(response) => {
                    info!("SecureYeoman: {} tasks (bridged)", action);
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "SecureYeoman bridge: falling back to mock for tasks {}", action);
                    success_result(serde_json::json!({
                        "tasks": [],
                        "total": 0,
                        "_source": "mock",
                    }))
                }
            }
        }
        op @ ("assign" | "cancel") => {
            let body = serde_json::json!({
                "action": op,
                "agent_id": agent_id,
                "description": description,
                "task_id": task_id,
                "priority": priority,
            });
            match bridge.post("/api/v1/tasks", body).await {
                Ok(response) => {
                    info!(action = %op, "SecureYeoman: {} task (bridged)", op);
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "SecureYeoman bridge: falling back to mock for {} task", op);
                    let id = task_id.unwrap_or_else(|| Uuid::new_v4().to_string());
                    success_result(serde_json::json!({
                        "task_id": id,
                        "action": op,
                        "agent_id": agent_id,
                        "description": description,
                        "priority": priority.unwrap_or_else(|| "medium".to_string()),
                        "status": "ok",
                        "_source": "mock",
                    }))
                }
            }
        }
        _ => unreachable!(),
    }
}

pub(crate) async fn handle_yeoman_tools(args: &serde_json::Value) -> McpToolResult {
    let action = match extract_required_string(args, "action") {
        Ok(a) => a,
        Err(e) => return e,
    };

    let action_opt = Some(action.clone());
    if let Err(e) = validate_enum_opt(
        &action_opt,
        "action",
        &["list", "search", "info", "categories"],
    ) {
        return e;
    }

    let query_str = get_optional_string_arg(args, "query");
    let category = get_optional_string_arg(args, "category");
    let name = get_optional_string_arg(args, "name");
    let bridge = yeoman_bridge();

    let mut query = Vec::new();
    query.push(("action".to_string(), action.clone()));
    if let Some(ref q) = query_str {
        query.push(("query".to_string(), q.clone()));
    }
    if let Some(ref c) = category {
        query.push(("category".to_string(), c.clone()));
    }
    if let Some(ref n) = name {
        query.push(("name".to_string(), n.clone()));
    }

    match bridge.get("/api/v1/tools", &query).await {
        Ok(response) => {
            info!("SecureYeoman: {} tools (bridged)", action);
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "SecureYeoman bridge: falling back to mock for tools {}", action);
            success_result(serde_json::json!({
                "tools": [],
                "total": 0,
                "categories": [],
                "_source": "mock",
            }))
        }
    }
}

pub(crate) async fn handle_yeoman_integrations(args: &serde_json::Value) -> McpToolResult {
    let action = match extract_required_string(args, "action") {
        Ok(a) => a,
        Err(e) => return e,
    };

    let action_opt = Some(action.clone());
    if let Err(e) = validate_enum_opt(
        &action_opt,
        "action",
        &["list", "enable", "disable", "status"],
    ) {
        return e;
    }

    let name = get_optional_string_arg(args, "name");
    let config = get_optional_string_arg(args, "config");
    let bridge = yeoman_bridge();

    match action.as_str() {
        "list" | "status" => {
            let mut query = Vec::new();
            if let Some(ref n) = name {
                query.push(("name".to_string(), n.clone()));
            }
            query.push(("action".to_string(), action.clone()));
            match bridge.get("/api/v1/integrations", &query).await {
                Ok(response) => {
                    info!("SecureYeoman: {} integrations (bridged)", action);
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "SecureYeoman bridge: falling back to mock for integrations {}", action);
                    success_result(serde_json::json!({
                        "integrations": [],
                        "total": 0,
                        "_source": "mock",
                    }))
                }
            }
        }
        op @ ("enable" | "disable") => {
            let body = serde_json::json!({
                "action": op,
                "name": name,
                "config": config,
            });
            match bridge.post("/api/v1/integrations", body).await {
                Ok(response) => {
                    info!(action = %op, "SecureYeoman: {} integration (bridged)", op);
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "SecureYeoman bridge: falling back to mock for {} integration", op);
                    success_result(serde_json::json!({
                        "action": op,
                        "name": name.unwrap_or_else(|| "unknown".to_string()),
                        "status": "ok",
                        "_source": "mock",
                    }))
                }
            }
        }
        _ => unreachable!(),
    }
}

pub(crate) async fn handle_yeoman_status(args: &serde_json::Value) -> McpToolResult {
    let detail = get_optional_string_arg(args, "detail");

    if let Some(ref d) = detail {
        let d_opt = Some(d.clone());
        if let Err(e) = validate_enum_opt(&d_opt, "detail", &["brief", "full"]) {
            return e;
        }
    }

    let bridge = yeoman_bridge();
    let mut query = Vec::new();
    if let Some(ref d) = detail {
        query.push(("detail".to_string(), d.clone()));
    }

    match bridge.get("/api/v1/status", &query).await {
        Ok(response) => {
            info!("SecureYeoman: platform status (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "SecureYeoman bridge: falling back to mock for platform status");
            success_result(serde_json::json!({
                "healthy": false,
                "active_agents": 0,
                "total_tools": 279,
                "integrations_enabled": 0,
                "message": "SecureYeoman not reachable",
                "_source": "mock",
            }))
        }
    }
}

pub(crate) async fn handle_yeoman_logs(args: &serde_json::Value) -> McpToolResult {
    let action = match extract_required_string(args, "action") {
        Ok(a) => a,
        Err(e) => return e,
    };

    let action_opt = Some(action.clone());
    if let Err(e) = validate_enum_opt(
        &action_opt,
        "action",
        &["query", "stream", "tail", "search"],
    ) {
        return e;
    }

    let agent_id = get_optional_string_arg(args, "agent_id");
    let level = get_optional_string_arg(args, "level");
    let limit = get_optional_string_arg(args, "limit");
    let query_str = get_optional_string_arg(args, "query");

    if let Err(e) = validate_enum_opt(&level, "level", &["debug", "info", "warn", "error"]) {
        return e;
    }

    let bridge = yeoman_bridge();
    let mut query = Vec::new();
    query.push(("action".to_string(), action.clone()));
    if let Some(ref id) = agent_id {
        query.push(("agent_id".to_string(), id.clone()));
    }
    if let Some(ref l) = level {
        query.push(("level".to_string(), l.clone()));
    }
    if let Some(ref lim) = limit {
        query.push(("limit".to_string(), lim.clone()));
    }
    if let Some(ref q) = query_str {
        query.push(("query".to_string(), q.clone()));
    }

    match bridge.get("/api/v1/logs", &query).await {
        Ok(response) => {
            info!("SecureYeoman: {} logs (bridged)", action);
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "SecureYeoman bridge: falling back to mock for logs {}", action);
            success_result(serde_json::json!({
                "entries": [],
                "total": 0,
                "_source": "mock",
            }))
        }
    }
}

pub(crate) async fn handle_yeoman_workflows(args: &serde_json::Value) -> McpToolResult {
    let action = match extract_required_string(args, "action") {
        Ok(a) => a,
        Err(e) => return e,
    };

    let action_opt = Some(action.clone());
    if let Err(e) = validate_enum_opt(
        &action_opt,
        "action",
        &["list", "create", "run", "stop", "status", "delete"],
    ) {
        return e;
    }

    let name = get_optional_string_arg(args, "name");
    let workflow_id = get_optional_string_arg(args, "workflow_id");
    let bridge = yeoman_bridge();

    match action.as_str() {
        "list" | "status" => {
            let mut query = Vec::new();
            if let Some(ref id) = workflow_id {
                query.push(("workflow_id".to_string(), id.clone()));
            }
            if let Some(ref n) = name {
                query.push(("name".to_string(), n.clone()));
            }
            query.push(("action".to_string(), action.clone()));
            match bridge.get("/api/v1/workflows", &query).await {
                Ok(response) => {
                    info!("SecureYeoman: {} workflows (bridged)", action);
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "SecureYeoman bridge: falling back to mock for workflows {}", action);
                    success_result(serde_json::json!({
                        "workflows": [],
                        "total": 0,
                        "_source": "mock",
                    }))
                }
            }
        }
        op @ ("create" | "run" | "stop" | "delete") => {
            let body = serde_json::json!({
                "action": op,
                "name": name,
                "workflow_id": workflow_id,
            });
            match bridge.post("/api/v1/workflows", body).await {
                Ok(response) => {
                    info!(action = %op, "SecureYeoman: {} workflow (bridged)", op);
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "SecureYeoman bridge: falling back to mock for {} workflow", op);
                    let id = workflow_id.unwrap_or_else(|| Uuid::new_v4().to_string());
                    success_result(serde_json::json!({
                        "workflow_id": id,
                        "action": op,
                        "name": name.unwrap_or_else(|| "unnamed-workflow".to_string()),
                        "status": "ok",
                        "_source": "mock",
                    }))
                }
            }
        }
        _ => unreachable!(),
    }
}
