use anyhow::Result;

use super::mcp_helper::{insert_opt, insert_str, mcp_call};
use crate::interpreter::intent::{Intent, Translation};
use crate::security::PermissionLevel;

pub(crate) fn translate_edge(intent: &Intent) -> Result<Translation> {
    match intent {
        Intent::EdgeListNodes { status } => {
            let mut a = serde_json::Map::new();
            insert_opt(&mut a, "status", status);
            Ok(mcp_call(
                "edge_list",
                a,
                format!(
                    "List edge nodes{}",
                    status
                        .as_ref()
                        .map_or(String::new(), |s| format!(" ({})", s))
                ),
                PermissionLevel::Safe,
                "Lists edge nodes in the fleet via MCP bridge".to_string(),
            ))
        }

        Intent::EdgeDeploy { task, node } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "task", task);
            insert_opt(&mut a, "node_id", node);
            Ok(mcp_call(
                "edge_deploy",
                a,
                format!("Deploy to edge: {}", task),
                PermissionLevel::SystemWrite,
                "Deploys a task to an edge node via MCP bridge".to_string(),
            ))
        }

        Intent::EdgeUpdate { node, version } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "node_id", node);
            if let Some(v) = version {
                a.insert("version".to_string(), serde_json::Value::String(v.clone()));
            } else {
                a.insert(
                    "version".to_string(),
                    serde_json::Value::String("latest".to_string()),
                );
            }
            Ok(mcp_call(
                "edge_update",
                a,
                format!("Update edge node: {}", node),
                PermissionLevel::SystemWrite,
                "Triggers OTA update on an edge node via MCP bridge".to_string(),
            ))
        }

        Intent::EdgeHealth { node } => {
            let mut a = serde_json::Map::new();
            insert_opt(&mut a, "node_id", node);
            Ok(mcp_call(
                "edge_health",
                a,
                format!(
                    "Edge health{}",
                    node.as_ref().map_or(String::new(), |n| format!(": {}", n))
                ),
                PermissionLevel::Safe,
                "Gets edge node health status via MCP bridge".to_string(),
            ))
        }

        Intent::EdgeDecommission { node } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "node_id", node);
            Ok(mcp_call(
                "edge_decommission",
                a,
                format!("Decommission edge node: {}", node),
                PermissionLevel::SystemWrite,
                "Decommissions an edge node via MCP bridge".to_string(),
            ))
        }

        Intent::EdgeLogs { action, node } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "action", action);
            insert_opt(&mut a, "node_id", node);
            Ok(mcp_call(
                "edge_logs",
                a,
                format!(
                    "Edge logs: {}{}",
                    action,
                    node.as_ref().map_or(String::new(), |n| format!(" ({})", n))
                ),
                PermissionLevel::Safe,
                "Queries edge node logs".to_string(),
            ))
        }

        Intent::EdgeConfig { action, node, key } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "action", action);
            insert_opt(&mut a, "node_id", node);
            insert_opt(&mut a, "key", key);
            Ok(mcp_call(
                "edge_config",
                a,
                format!(
                    "Edge config: {}{}",
                    action,
                    key.as_ref().map_or(String::new(), |k| format!(" '{}'", k))
                ),
                match action.as_str() {
                    "get" | "list" => PermissionLevel::Safe,
                    _ => PermissionLevel::SystemWrite,
                },
                "Manages edge node config".to_string(),
            ))
        }

        _ => unreachable!("translate_edge called with non-edge intent"),
    }
}
