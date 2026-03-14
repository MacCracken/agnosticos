use anyhow::Result;

use crate::interpreter::intent::{Intent, Translation};
use crate::security::PermissionLevel;

pub(crate) fn translate_rasa(intent: &Intent) -> Result<Translation> {
    match intent {
        Intent::RasaCanvas { action, name } => {
            let mut args_json = serde_json::Map::new();
            args_json.insert(
                "action".to_string(),
                serde_json::Value::String(action.clone()),
            );
            if let Some(n) = name {
                args_json.insert("name".to_string(), serde_json::Value::String(n.clone()));
            }
            let body = serde_json::json!({"name": "rasa_canvas", "arguments": args_json});
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
                    "Rasa canvas: {}{}",
                    action,
                    name.as_ref().map_or(String::new(), |n| format!(" '{}'", n))
                ),
                permission: match action.as_str() {
                    "info" | "list" => PermissionLevel::Safe,
                    _ => PermissionLevel::SystemWrite,
                },
                explanation: format!(
                    "{} a Rasa image canvas via MCP bridge",
                    match action.as_str() {
                        "create" => "Creates",
                        "open" => "Opens",
                        "save" => "Saves",
                        "close" => "Closes",
                        _ => "Queries",
                    }
                ),
            })
        }

        Intent::RasaLayers { action, name, kind } => {
            let mut args_json = serde_json::Map::new();
            args_json.insert(
                "action".to_string(),
                serde_json::Value::String(action.clone()),
            );
            if let Some(n) = name {
                args_json.insert("name".to_string(), serde_json::Value::String(n.clone()));
            }
            if let Some(k) = kind {
                args_json.insert("kind".to_string(), serde_json::Value::String(k.clone()));
            }
            let body = serde_json::json!({"name": "rasa_layers", "arguments": args_json});
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
                    "Rasa layers: {}{}",
                    action,
                    name.as_ref().map_or(String::new(), |n| format!(" '{}'", n))
                ),
                permission: if action == "list" {
                    PermissionLevel::Safe
                } else {
                    PermissionLevel::SystemWrite
                },
                explanation: format!(
                    "{} layers in Rasa via MCP bridge",
                    match action.as_str() {
                        "add" => "Adds",
                        "remove" => "Removes",
                        "reorder" => "Reorders",
                        "merge" => "Merges",
                        "list" => "Lists",
                        _ => "Manages",
                    }
                ),
            })
        }

        Intent::RasaTools { action, params } => {
            let mut args_json = serde_json::Map::new();
            args_json.insert(
                "action".to_string(),
                serde_json::Value::String(action.clone()),
            );
            if let Some(p) = params {
                args_json.insert("params".to_string(), serde_json::Value::String(p.clone()));
            }
            let body = serde_json::json!({"name": "rasa_tools", "arguments": args_json});
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
                    "Rasa tools: {}{}",
                    action,
                    params
                        .as_ref()
                        .map_or(String::new(), |p| format!(" ({})", p))
                ),
                permission: PermissionLevel::SystemWrite,
                explanation: format!("Applies {} tool in Rasa via MCP bridge", action),
            })
        }

        Intent::RasaAi { action, prompt } => {
            let mut args_json = serde_json::Map::new();
            args_json.insert(
                "action".to_string(),
                serde_json::Value::String(action.clone()),
            );
            if let Some(p) = prompt {
                args_json.insert("prompt".to_string(), serde_json::Value::String(p.clone()));
            }
            let body = serde_json::json!({"name": "rasa_ai", "arguments": args_json});
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
                    "Rasa AI: {}{}",
                    action,
                    prompt
                        .as_ref()
                        .map_or(String::new(), |p| format!(" '{}'", p))
                ),
                permission: PermissionLevel::SystemWrite,
                explanation: format!("Runs AI {} on Rasa image via MCP bridge", action),
            })
        }

        Intent::RasaExport { path, format } => {
            let mut args_json = serde_json::Map::new();
            if let Some(p) = path {
                args_json.insert("path".to_string(), serde_json::Value::String(p.clone()));
            }
            if let Some(f) = format {
                args_json.insert("format".to_string(), serde_json::Value::String(f.clone()));
            }
            let body = serde_json::json!({"name": "rasa_export", "arguments": args_json});
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
                    "Rasa export{}",
                    format
                        .as_ref()
                        .map_or(String::new(), |f| format!(" as {}", f))
                ),
                permission: PermissionLevel::SystemWrite,
                explanation: "Exports Rasa image via MCP bridge".to_string(),
            })
        }

        Intent::RasaBatch { action, path } => {
            let mut args_json = serde_json::Map::new();
            args_json.insert(
                "action".to_string(),
                serde_json::Value::String(action.clone()),
            );
            if let Some(p) = path {
                args_json.insert("path".to_string(), serde_json::Value::String(p.clone()));
            }
            let body = serde_json::json!({"name": "rasa_batch", "arguments": args_json});
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
                    "Rasa batch: {}{}",
                    action,
                    path.as_ref().map_or(String::new(), |p| format!(" '{}'", p))
                ),
                permission: match action.as_str() {
                    "list" => PermissionLevel::Safe,
                    _ => PermissionLevel::SystemWrite,
                },
                explanation: "Batch image operations via Rasa".to_string(),
            })
        }

        Intent::RasaTemplates { action, name } => {
            let mut args_json = serde_json::Map::new();
            args_json.insert(
                "action".to_string(),
                serde_json::Value::String(action.clone()),
            );
            if let Some(n) = name {
                args_json.insert("name".to_string(), serde_json::Value::String(n.clone()));
            }
            let body = serde_json::json!({"name": "rasa_templates", "arguments": args_json});
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
                    "Rasa templates: {}{}",
                    action,
                    name.as_ref().map_or(String::new(), |n| format!(" '{}'", n))
                ),
                permission: match action.as_str() {
                    "list" | "info" => PermissionLevel::Safe,
                    _ => PermissionLevel::SystemWrite,
                },
                explanation: "Manages design templates via Rasa".to_string(),
            })
        }

        _ => unreachable!("translate_rasa called with non-rasa intent"),
    }
}
