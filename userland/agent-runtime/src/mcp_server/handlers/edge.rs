use tracing::info;

use super::super::helpers::{
    error_result, extract_required_string, get_optional_string_arg, success_result,
    validate_enum_opt,
};
use super::super::types::McpToolResult;
use crate::http_api::ApiState;

pub(crate) async fn handle_edge_list(state: &ApiState, args: &serde_json::Value) -> McpToolResult {
    let status_filter = get_optional_string_arg(args, "status");

    if let Some(ref s) = status_filter {
        if let Err(e) = validate_enum_opt(
            &status_filter,
            "status",
            &["online", "suspect", "offline", "updating", "decommissioned"],
        ) {
            return e;
        }
        let _ = s; // used above
    }

    let fleet = state.edge_fleet.read().await;
    let filter = status_filter.as_deref().and_then(|s| match s {
        "online" => Some(crate::edge::EdgeNodeStatus::Online),
        "suspect" => Some(crate::edge::EdgeNodeStatus::Suspect),
        "offline" => Some(crate::edge::EdgeNodeStatus::Offline),
        "updating" => Some(crate::edge::EdgeNodeStatus::Updating),
        "decommissioned" => Some(crate::edge::EdgeNodeStatus::Decommissioned),
        _ => None,
    });

    let nodes = fleet.list_nodes(filter);
    let node_list: Vec<serde_json::Value> = nodes
        .iter()
        .map(|n| {
            serde_json::json!({
                "id": n.id,
                "name": n.name,
                "status": n.status.to_string(),
                "arch": n.capabilities.arch,
                "agent_binary": n.agent_binary,
                "agent_version": n.agent_version,
                "os_version": n.os_version,
                "active_tasks": n.active_tasks,
                "tpm_attested": n.tpm_attested,
                "last_heartbeat": n.last_heartbeat.to_rfc3339(),
            })
        })
        .collect();

    info!(count = node_list.len(), "Edge: list nodes");
    success_result(serde_json::json!({
        "nodes": node_list,
        "total": node_list.len(),
    }))
}

pub(crate) async fn handle_edge_deploy(
    state: &ApiState,
    args: &serde_json::Value,
) -> McpToolResult {
    let task = match extract_required_string(args, "task") {
        Ok(t) => t,
        Err(e) => return e,
    };

    let node_id = get_optional_string_arg(args, "node_id");
    let required_tags: Vec<String> = args
        .get("required_tags")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    let require_gpu = args
        .get("require_gpu")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let fleet = state.edge_fleet.read().await;

    let target = if let Some(ref nid) = node_id {
        match fleet.get_node(nid) {
            Some(n) if n.status == crate::edge::EdgeNodeStatus::Online => Some(n),
            Some(_) => return error_result(format!("Node {} is not online", nid)),
            None => return error_result(format!("Node {} not found", nid)),
        }
    } else {
        fleet
            .route_task(&required_tags, require_gpu, None)
            .into_iter()
            .next()
    };

    match target {
        Some(node) => {
            info!(task = %task, node = %node.id, name = %node.name, "Edge: deploy task");
            success_result(serde_json::json!({
                "status": "deployed",
                "task": task,
                "node_id": node.id,
                "node_name": node.name,
                "arch": node.capabilities.arch,
            }))
        }
        None => error_result("No suitable edge node available for this task".into()),
    }
}

pub(crate) async fn handle_edge_update(
    state: &ApiState,
    args: &serde_json::Value,
) -> McpToolResult {
    let node_id = match extract_required_string(args, "node_id") {
        Ok(id) => id,
        Err(e) => return e,
    };
    let version = get_optional_string_arg(args, "version").unwrap_or_else(|| "latest".into());

    let mut fleet = state.edge_fleet.write().await;
    match fleet.start_update(&node_id) {
        Ok(()) => {
            info!(node = %node_id, version = %version, "Edge: OTA update started");
            success_result(serde_json::json!({
                "status": "updating",
                "node_id": node_id,
                "target_version": version,
            }))
        }
        Err(e) => error_result(format!("Failed to start update: {}", e)),
    }
}

pub(crate) async fn handle_edge_health(
    state: &ApiState,
    args: &serde_json::Value,
) -> McpToolResult {
    let node_id = get_optional_string_arg(args, "node_id");

    if let Some(nid) = node_id {
        // Per-node query: read lock only, no health sweep
        let fleet = state.edge_fleet.read().await;
        match fleet.get_node(&nid) {
            Some(node) => {
                info!(node = %nid, status = %node.status, "Edge: node health");
                success_result(serde_json::json!({
                    "node_id": node.id,
                    "name": node.name,
                    "status": node.status.to_string(),
                    "last_heartbeat": node.last_heartbeat.to_rfc3339(),
                    "active_tasks": node.active_tasks,
                    "tasks_completed": node.tasks_completed,
                    "tpm_attested": node.tpm_attested,
                    "capabilities": {
                        "arch": node.capabilities.arch,
                        "cpu_cores": node.capabilities.cpu_cores,
                        "memory_mb": node.capabilities.memory_mb,
                        "disk_mb": node.capabilities.disk_mb,
                        "has_gpu": node.capabilities.has_gpu,
                        "network_quality": node.capabilities.network_quality,
                    },
                }))
            }
            None => error_result(format!("Node {} not found", nid)),
        }
    } else {
        // Fleet-wide query: write lock needed for check_health sweep
        let mut fleet = state.edge_fleet.write().await;
        fleet.check_health();
        let stats = fleet.stats();
        info!(
            total = stats.total_nodes,
            online = stats.online,
            "Edge: fleet health"
        );
        success_result(serde_json::json!({
            "fleet": {
                "total_nodes": stats.total_nodes,
                "online": stats.online,
                "suspect": stats.suspect,
                "offline": stats.offline,
                "updating": stats.updating,
                "decommissioned": stats.decommissioned,
                "active_tasks": stats.active_tasks,
                "tasks_completed": stats.tasks_completed,
            }
        }))
    }
}

pub(crate) async fn handle_edge_decommission(
    state: &ApiState,
    args: &serde_json::Value,
) -> McpToolResult {
    let node_id = match extract_required_string(args, "node_id") {
        Ok(id) => id,
        Err(e) => return e,
    };

    let mut fleet = state.edge_fleet.write().await;
    match fleet.decommission(&node_id) {
        Ok(node) => {
            info!(node = %node_id, name = %node.name, "Edge: node decommissioned");
            success_result(serde_json::json!({
                "status": "decommissioned",
                "node_id": node.id,
                "node_name": node.name,
            }))
        }
        Err(e) => error_result(format!("Failed to decommission: {}", e)),
    }
}
