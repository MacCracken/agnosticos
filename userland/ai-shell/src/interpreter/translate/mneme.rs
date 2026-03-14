use anyhow::Result;

use crate::interpreter::intent::{Intent, Translation};
use crate::security::PermissionLevel;

pub(crate) fn translate_mneme(intent: &Intent) -> Result<Translation> {
    match intent {
        Intent::MnemeNotebook { action, name } => {
            let mut args_json = serde_json::Map::new();
            args_json.insert(
                "action".to_string(),
                serde_json::Value::String(action.clone()),
            );
            if let Some(n) = name {
                args_json.insert("name".to_string(), serde_json::Value::String(n.clone()));
            }
            let body = serde_json::json!({"name": "mneme_notebook", "arguments": args_json});
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
                    "Mneme notebook: {}{}",
                    action,
                    name.as_ref().map_or(String::new(), |n| format!(" '{}'", n))
                ),
                permission: match action.as_str() {
                    "list" | "info" => PermissionLevel::Safe,
                    _ => PermissionLevel::SystemWrite,
                },
                explanation: format!(
                    "{} Mneme notebooks via MCP bridge",
                    match action.as_str() {
                        "create" => "Creates",
                        "open" => "Opens",
                        "delete" => "Deletes",
                        "list" => "Lists",
                        _ => "Queries",
                    }
                ),
            })
        }

        Intent::MnemeNotes {
            action,
            title,
            notebook_id,
        } => {
            let mut args_json = serde_json::Map::new();
            args_json.insert(
                "action".to_string(),
                serde_json::Value::String(action.clone()),
            );
            if let Some(t) = title {
                args_json.insert("title".to_string(), serde_json::Value::String(t.clone()));
            }
            if let Some(nb) = notebook_id {
                args_json.insert(
                    "notebook_id".to_string(),
                    serde_json::Value::String(nb.clone()),
                );
            }
            let body = serde_json::json!({"name": "mneme_notes", "arguments": args_json});
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
                    "Mneme notes: {}{}",
                    action,
                    title
                        .as_ref()
                        .map_or(String::new(), |t| format!(" '{}'", t))
                ),
                permission: match action.as_str() {
                    "list" | "get" => PermissionLevel::Safe,
                    _ => PermissionLevel::SystemWrite,
                },
                explanation: format!(
                    "{} notes in Mneme via MCP bridge",
                    match action.as_str() {
                        "create" => "Creates",
                        "edit" => "Edits",
                        "delete" => "Deletes",
                        "list" => "Lists",
                        _ => "Manages",
                    }
                ),
            })
        }

        Intent::MnemeSearch { query, mode } => {
            let mut args_json = serde_json::Map::new();
            args_json.insert(
                "query".to_string(),
                serde_json::Value::String(query.clone()),
            );
            if let Some(m) = mode {
                args_json.insert("mode".to_string(), serde_json::Value::String(m.clone()));
            }
            let body = serde_json::json!({"name": "mneme_search", "arguments": args_json});
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
                    "Mneme search: '{}'{}",
                    query,
                    mode.as_ref()
                        .map_or(String::new(), |m| format!(" ({})", m))
                ),
                permission: PermissionLevel::Safe,
                explanation: "Searches Mneme knowledge base via MCP bridge".to_string(),
            })
        }

        Intent::MnemeAi { action, note_id } => {
            let mut args_json = serde_json::Map::new();
            args_json.insert(
                "action".to_string(),
                serde_json::Value::String(action.clone()),
            );
            if let Some(id) = note_id {
                args_json.insert(
                    "note_id".to_string(),
                    serde_json::Value::String(id.clone()),
                );
            }
            let body = serde_json::json!({"name": "mneme_ai", "arguments": args_json});
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
                    "Mneme AI: {}{}",
                    action,
                    note_id
                        .as_ref()
                        .map_or(String::new(), |id| format!(" ({})", id))
                ),
                permission: PermissionLevel::SystemWrite,
                explanation: format!(
                    "Runs AI {} on Mneme knowledge via MCP bridge",
                    action
                ),
            })
        }

        Intent::MnemeGraph { action, node_id } => {
            let mut args_json = serde_json::Map::new();
            args_json.insert(
                "action".to_string(),
                serde_json::Value::String(action.clone()),
            );
            if let Some(id) = node_id {
                args_json.insert(
                    "node_id".to_string(),
                    serde_json::Value::String(id.clone()),
                );
            }
            let body = serde_json::json!({"name": "mneme_graph", "arguments": args_json});
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
                    "Mneme graph: {}{}",
                    action,
                    node_id
                        .as_ref()
                        .map_or(String::new(), |id| format!(" ({})", id))
                ),
                permission: match action.as_str() {
                    "view" | "stats" => PermissionLevel::Safe,
                    _ => PermissionLevel::SystemWrite,
                },
                explanation: "Manages Mneme knowledge graph via MCP bridge".to_string(),
            })
        }

        _ => unreachable!("translate_mneme called with non-mneme intent"),
    }
}
