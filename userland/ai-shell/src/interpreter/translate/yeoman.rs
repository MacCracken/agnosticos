use anyhow::Result;

use crate::interpreter::intent::{Intent, Translation};
use crate::security::PermissionLevel;

pub(crate) fn translate_yeoman(intent: &Intent) -> Result<Translation> {
    match intent {
        Intent::YeomanAgents {
            action,
            agent_id,
            name,
        } => {
            let mut args_json = serde_json::Map::new();
            args_json.insert(
                "action".to_string(),
                serde_json::Value::String(action.clone()),
            );
            if let Some(id) = agent_id {
                args_json.insert(
                    "agent_id".to_string(),
                    serde_json::Value::String(id.clone()),
                );
            }
            if let Some(n) = name {
                args_json.insert("name".to_string(), serde_json::Value::String(n.clone()));
            }
            let body = serde_json::json!({"name": "yeoman_agents", "arguments": args_json});
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
                    serde_json::to_string(&body).unwrap(),
                ],
                description: format!(
                    "SecureYeoman agents: {}{}",
                    action,
                    name.as_ref()
                        .map_or(String::new(), |n| format!(" '{}'", n))
                ),
                permission: match action.as_str() {
                    "list" | "status" | "info" => PermissionLevel::Safe,
                    _ => PermissionLevel::SystemWrite,
                },
                explanation:
                    "Lists/Deploys/Stops/Queries agents via SecureYeoman MCP bridge".to_string(),
            })
        }

        Intent::YeomanTasks {
            action,
            description,
            task_id,
        } => {
            let mut args_json = serde_json::Map::new();
            args_json.insert(
                "action".to_string(),
                serde_json::Value::String(action.clone()),
            );
            if let Some(d) = description {
                args_json.insert(
                    "description".to_string(),
                    serde_json::Value::String(d.clone()),
                );
            }
            if let Some(id) = task_id {
                args_json.insert(
                    "task_id".to_string(),
                    serde_json::Value::String(id.clone()),
                );
            }
            let body = serde_json::json!({"name": "yeoman_tasks", "arguments": args_json});
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
                    serde_json::to_string(&body).unwrap(),
                ],
                description: format!(
                    "SecureYeoman task: {}{}",
                    action,
                    task_id
                        .as_ref()
                        .map_or(String::new(), |id| format!(" '{}'", id))
                ),
                permission: match action.as_str() {
                    "list" | "status" => PermissionLevel::Safe,
                    _ => PermissionLevel::SystemWrite,
                },
                explanation:
                    "Assigns/Lists/Checks/Cancels tasks via SecureYeoman MCP bridge".to_string(),
            })
        }

        Intent::YeomanTools { action, query } => {
            let mut args_json = serde_json::Map::new();
            args_json.insert(
                "action".to_string(),
                serde_json::Value::String(action.clone()),
            );
            if let Some(q) = query {
                args_json.insert("query".to_string(), serde_json::Value::String(q.clone()));
            }
            let body = serde_json::json!({"name": "yeoman_tools", "arguments": args_json});
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
                    serde_json::to_string(&body).unwrap(),
                ],
                description: format!(
                    "SecureYeoman tools: {}{}",
                    action,
                    query
                        .as_ref()
                        .map_or(String::new(), |q| format!(" '{}'", q))
                ),
                permission: PermissionLevel::Safe,
                explanation: "Queries MCP tools catalog via SecureYeoman MCP bridge".to_string(),
            })
        }

        Intent::YeomanIntegrations { action, name } => {
            let mut args_json = serde_json::Map::new();
            args_json.insert(
                "action".to_string(),
                serde_json::Value::String(action.clone()),
            );
            if let Some(n) = name {
                args_json.insert("name".to_string(), serde_json::Value::String(n.clone()));
            }
            let body =
                serde_json::json!({"name": "yeoman_integrations", "arguments": args_json});
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
                    serde_json::to_string(&body).unwrap(),
                ],
                description: format!(
                    "SecureYeoman integration: {}{}",
                    action,
                    name.as_ref()
                        .map_or(String::new(), |n| format!(" '{}'", n))
                ),
                permission: match action.as_str() {
                    "list" | "status" => PermissionLevel::Safe,
                    _ => PermissionLevel::SystemWrite,
                },
                explanation:
                    "Lists/Enables/Disables/Checks integrations via SecureYeoman MCP bridge"
                        .to_string(),
            })
        }

        Intent::YeomanStatus => {
            let args_json = serde_json::Map::new();
            let body = serde_json::json!({"name": "yeoman_status", "arguments": args_json});
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
                    serde_json::to_string(&body).unwrap(),
                ],
                description: "SecureYeoman status".to_string(),
                permission: PermissionLevel::Safe,
                explanation: "Checks SecureYeoman platform health via MCP bridge".to_string(),
            })
        }

        _ => unreachable!("translate_yeoman called with non-yeoman intent"),
    }
}
