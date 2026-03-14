use anyhow::Result;

use crate::interpreter::intent::{Intent, Translation};
use crate::security::PermissionLevel;

pub(crate) fn translate_photis(intent: &Intent) -> Result<Translation> {
    match intent {
        Intent::TaskList { status } => {
            let mut args_json = serde_json::Map::new();
            if let Some(s) = status {
                args_json.insert("status".to_string(), serde_json::Value::String(s.clone()));
            }
            let body = serde_json::json!({"name": "photis_list_tasks", "arguments": args_json});
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
                    "List tasks{}",
                    status
                        .as_ref()
                        .map_or(String::new(), |s| format!(" with status {}", s))
                ),
                permission: PermissionLevel::Safe,
                explanation: "Lists tasks from Photis Nadi via MCP bridge".to_string(),
            })
        }

        Intent::TaskCreate { title, priority } => {
            let mut args_json = serde_json::Map::new();
            args_json.insert(
                "title".to_string(),
                serde_json::Value::String(title.clone()),
            );
            if let Some(p) = priority {
                args_json.insert("priority".to_string(), serde_json::Value::String(p.clone()));
            }
            let body = serde_json::json!({"name": "photis_create_task", "arguments": args_json});
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
                description: format!("Create task: {}", title),
                permission: PermissionLevel::SystemWrite,
                explanation: "Creates a new task in Photis Nadi via MCP bridge".to_string(),
            })
        }

        Intent::TaskUpdate { task_id, status } => {
            let mut args_json = serde_json::Map::new();
            args_json.insert(
                "task_id".to_string(),
                serde_json::Value::String(task_id.clone()),
            );
            if let Some(s) = status {
                args_json.insert("status".to_string(), serde_json::Value::String(s.clone()));
            }
            let body = serde_json::json!({"name": "photis_update_task", "arguments": args_json});
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
                description: format!("Update task {}", task_id),
                permission: PermissionLevel::SystemWrite,
                explanation: "Updates a task in Photis Nadi via MCP bridge".to_string(),
            })
        }

        Intent::RitualCheck { date } => {
            let mut args_json = serde_json::Map::new();
            if let Some(d) = date {
                args_json.insert("date".to_string(), serde_json::Value::String(d.clone()));
            }
            let body = serde_json::json!({"name": "photis_get_rituals", "arguments": args_json});
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
                description: "Check daily rituals".to_string(),
                permission: PermissionLevel::Safe,
                explanation: "Retrieves ritual/habit status from Photis Nadi via MCP bridge"
                    .to_string(),
            })
        }

        Intent::ProductivityStats { period } => {
            let mut args_json = serde_json::Map::new();
            if let Some(p) = period {
                args_json.insert("period".to_string(), serde_json::Value::String(p.clone()));
            }
            let body = serde_json::json!({"name": "photis_analytics", "arguments": args_json});
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
                    "Productivity analytics{}",
                    period
                        .as_ref()
                        .map_or(String::new(), |p| format!(" for {}", p))
                ),
                permission: PermissionLevel::Safe,
                explanation: "Retrieves productivity analytics from Photis Nadi via MCP bridge"
                    .to_string(),
            })
        }

        Intent::PhotoisBoards { action, name } => {
            let mut args_json = serde_json::Map::new();
            args_json.insert(
                "action".to_string(),
                serde_json::Value::String(action.clone()),
            );
            if let Some(n) = name {
                args_json.insert("name".to_string(), serde_json::Value::String(n.clone()));
            }
            let body = serde_json::json!({"name": "photis_boards", "arguments": args_json});
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
                    "Photis boards: {}{}",
                    action,
                    name.as_ref().map_or(String::new(), |n| format!(" '{}'", n))
                ),
                permission: match action.as_str() {
                    "list" | "info" => PermissionLevel::Safe,
                    _ => PermissionLevel::SystemWrite,
                },
                explanation: "Manages boards via Photis Nadi".to_string(),
            })
        }

        Intent::PhotoisNotes { action, content } => {
            let mut args_json = serde_json::Map::new();
            args_json.insert(
                "action".to_string(),
                serde_json::Value::String(action.clone()),
            );
            if let Some(c) = content {
                args_json.insert("content".to_string(), serde_json::Value::String(c.clone()));
            }
            let body = serde_json::json!({"name": "photis_notes", "arguments": args_json});
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
                    "Photis notes: {}{}",
                    action,
                    content
                        .as_ref()
                        .map_or(String::new(), |c| format!(" '{}'", c))
                ),
                permission: match action.as_str() {
                    "list" | "get" | "search" => PermissionLevel::Safe,
                    _ => PermissionLevel::SystemWrite,
                },
                explanation: "Manages notes via Photis Nadi".to_string(),
            })
        }

        _ => unreachable!("translate_photis called with non-photis intent"),
    }
}
