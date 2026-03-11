use std::collections::HashMap;

use tracing::{debug, info};
use uuid::Uuid;

use crate::http_api::{ApiState, AuditEvent, RegisterAgentRequest, ResourceNeeds};
use super::super::helpers::{
    error_result, extract_required_string, extract_required_uuid, get_optional_string_arg,
    get_string_arg, success_result,
};
use super::super::types::McpToolResult;

pub(crate) async fn handle_health(state: &ApiState) -> McpToolResult {
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

pub(crate) async fn handle_list_agents(state: &ApiState) -> McpToolResult {
    let agents = state.agents_read().await;
    let agent_list: Vec<_> = agents.values().map(|a| &a.detail).collect();
    let total = agent_list.len();

    success_result(serde_json::json!({
        "agents": agent_list,
        "total": total,
    }))
}

pub(crate) async fn handle_get_agent(state: &ApiState, args: &serde_json::Value) -> McpToolResult {
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

pub(crate) async fn handle_register_agent(state: &ApiState, args: &serde_json::Value) -> McpToolResult {
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

pub(crate) async fn handle_deregister_agent(state: &ApiState, args: &serde_json::Value) -> McpToolResult {
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

pub(crate) async fn handle_heartbeat(state: &ApiState, args: &serde_json::Value) -> McpToolResult {
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

pub(crate) async fn handle_get_metrics(state: &ApiState) -> McpToolResult {
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

pub(crate) async fn handle_forward_audit(state: &ApiState, args: &serde_json::Value) -> McpToolResult {
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

pub(crate) async fn handle_memory_get(state: &ApiState, args: &serde_json::Value) -> McpToolResult {
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

pub(crate) async fn handle_memory_set(state: &ApiState, args: &serde_json::Value) -> McpToolResult {
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
