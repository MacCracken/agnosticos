use anyhow::Result;

use crate::interpreter::intent::{Intent, Translation};
use crate::security::PermissionLevel;

pub(crate) fn translate_edge(intent: &Intent) -> Result<Translation> {
    match intent {
        Intent::EdgeListNodes { status } => {
            let mut args_json = serde_json::Map::new();
            if let Some(s) = status {
                args_json.insert("status".to_string(), serde_json::Value::String(s.clone()));
            }
            let body = serde_json::json!({"name": "edge_list", "arguments": args_json});
            Ok(Translation {
                command: "curl".to_string(),
                args: vec![
                    "-s".to_string(),
                    "-X".to_string(),
                    "POST".to_string(),
                    "http://127.0.0.1:8090/v1/mcp/tools/call".to_string(),
                    "-H".to_string(),
                    "Content-Type: application/json".to_string(),
                    "-d".to_string(),
                    serde_json::to_string(&body)?,
                ],
                description: format!(
                    "List edge nodes{}",
                    status
                        .as_ref()
                        .map_or(String::new(), |s| format!(" ({})", s))
                ),
                permission: PermissionLevel::Safe,
                explanation: "Lists edge nodes in the fleet via MCP bridge".to_string(),
            })
        }

        Intent::EdgeDeploy { task, node } => {
            let mut args_json = serde_json::Map::new();
            args_json.insert("task".to_string(), serde_json::Value::String(task.clone()));
            if let Some(n) = node {
                args_json.insert("node_id".to_string(), serde_json::Value::String(n.clone()));
            }
            let body = serde_json::json!({"name": "edge_deploy", "arguments": args_json});
            Ok(Translation {
                command: "curl".to_string(),
                args: vec![
                    "-s".to_string(),
                    "-X".to_string(),
                    "POST".to_string(),
                    "http://127.0.0.1:8090/v1/mcp/tools/call".to_string(),
                    "-H".to_string(),
                    "Content-Type: application/json".to_string(),
                    "-d".to_string(),
                    serde_json::to_string(&body)?,
                ],
                description: format!("Deploy to edge: {}", task),
                permission: PermissionLevel::SystemWrite,
                explanation: "Deploys a task to an edge node via MCP bridge".to_string(),
            })
        }

        Intent::EdgeUpdate { node, version } => {
            let mut args_json = serde_json::Map::new();
            args_json.insert(
                "node_id".to_string(),
                serde_json::Value::String(node.clone()),
            );
            if let Some(v) = version {
                args_json.insert("version".to_string(), serde_json::Value::String(v.clone()));
            } else {
                args_json.insert(
                    "version".to_string(),
                    serde_json::Value::String("latest".to_string()),
                );
            }
            let body = serde_json::json!({"name": "edge_update", "arguments": args_json});
            Ok(Translation {
                command: "curl".to_string(),
                args: vec![
                    "-s".to_string(),
                    "-X".to_string(),
                    "POST".to_string(),
                    "http://127.0.0.1:8090/v1/mcp/tools/call".to_string(),
                    "-H".to_string(),
                    "Content-Type: application/json".to_string(),
                    "-d".to_string(),
                    serde_json::to_string(&body)?,
                ],
                description: format!("Update edge node: {}", node),
                permission: PermissionLevel::SystemWrite,
                explanation: "Triggers OTA update on an edge node via MCP bridge".to_string(),
            })
        }

        Intent::EdgeHealth { node } => {
            let mut args_json = serde_json::Map::new();
            if let Some(n) = node {
                args_json.insert("node_id".to_string(), serde_json::Value::String(n.clone()));
            }
            let body = serde_json::json!({"name": "edge_health", "arguments": args_json});
            Ok(Translation {
                command: "curl".to_string(),
                args: vec![
                    "-s".to_string(),
                    "-X".to_string(),
                    "POST".to_string(),
                    "http://127.0.0.1:8090/v1/mcp/tools/call".to_string(),
                    "-H".to_string(),
                    "Content-Type: application/json".to_string(),
                    "-d".to_string(),
                    serde_json::to_string(&body)?,
                ],
                description: format!(
                    "Edge health{}",
                    node.as_ref().map_or(String::new(), |n| format!(": {}", n))
                ),
                permission: PermissionLevel::Safe,
                explanation: "Gets edge node health status via MCP bridge".to_string(),
            })
        }

        Intent::EdgeDecommission { node } => {
            let args_json = serde_json::json!({"node_id": node});
            let body = serde_json::json!({"name": "edge_decommission", "arguments": args_json});
            Ok(Translation {
                command: "curl".to_string(),
                args: vec![
                    "-s".to_string(),
                    "-X".to_string(),
                    "POST".to_string(),
                    "http://127.0.0.1:8090/v1/mcp/tools/call".to_string(),
                    "-H".to_string(),
                    "Content-Type: application/json".to_string(),
                    "-d".to_string(),
                    serde_json::to_string(&body)?,
                ],
                description: format!("Decommission edge node: {}", node),
                permission: PermissionLevel::SystemWrite,
                explanation: "Decommissions an edge node via MCP bridge".to_string(),
            })
        }

        Intent::EdgeLogs { action, node } => {
            let mut args_json = serde_json::Map::new();
            args_json.insert(
                "action".to_string(),
                serde_json::Value::String(action.clone()),
            );
            if let Some(n) = node {
                args_json.insert("node_id".to_string(), serde_json::Value::String(n.clone()));
            }
            let body = serde_json::json!({"name": "edge_logs", "arguments": args_json});
            Ok(Translation {
                command: "curl".to_string(),
                args: vec![
                    "-s".to_string(),
                    "-X".to_string(),
                    "POST".to_string(),
                    "http://127.0.0.1:8090/v1/mcp/tools/call".to_string(),
                    "-H".to_string(),
                    "Content-Type: application/json".to_string(),
                    "-d".to_string(),
                    serde_json::to_string(&body)?,
                ],
                description: format!(
                    "Edge logs: {}{}",
                    action,
                    node.as_ref().map_or(String::new(), |n| format!(" ({})", n))
                ),
                permission: PermissionLevel::Safe,
                explanation: "Queries edge node logs".to_string(),
            })
        }

        Intent::EdgeConfig { action, node, key } => {
            let mut args_json = serde_json::Map::new();
            args_json.insert(
                "action".to_string(),
                serde_json::Value::String(action.clone()),
            );
            if let Some(n) = node {
                args_json.insert("node_id".to_string(), serde_json::Value::String(n.clone()));
            }
            if let Some(k) = key {
                args_json.insert("key".to_string(), serde_json::Value::String(k.clone()));
            }
            let body = serde_json::json!({"name": "edge_config", "arguments": args_json});
            Ok(Translation {
                command: "curl".to_string(),
                args: vec![
                    "-s".to_string(),
                    "-X".to_string(),
                    "POST".to_string(),
                    "http://127.0.0.1:8090/v1/mcp/tools/call".to_string(),
                    "-H".to_string(),
                    "Content-Type: application/json".to_string(),
                    "-d".to_string(),
                    serde_json::to_string(&body)?,
                ],
                description: format!(
                    "Edge config: {}{}",
                    action,
                    key.as_ref().map_or(String::new(), |k| format!(" '{}'", k))
                ),
                permission: match action.as_str() {
                    "get" | "list" => PermissionLevel::Safe,
                    _ => PermissionLevel::SystemWrite,
                },
                explanation: "Manages edge node config".to_string(),
            })
        }

        _ => unreachable!("translate_edge called with non-edge intent"),
    }
}
