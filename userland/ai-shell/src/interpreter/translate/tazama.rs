use anyhow::Result;

use crate::interpreter::intent::{Intent, Translation};
use crate::security::PermissionLevel;

pub(crate) fn translate_tazama(intent: &Intent) -> Result<Translation> {
    match intent {
        Intent::TazamaProject { action, name } => {
            let mut args_json = serde_json::Map::new();
            args_json.insert(
                "action".to_string(),
                serde_json::Value::String(action.clone()),
            );
            if let Some(n) = name {
                args_json.insert("name".to_string(), serde_json::Value::String(n.clone()));
            }
            let body = serde_json::json!({"name": "tazama_project", "arguments": args_json});
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
                    "Tazama project: {}{}",
                    action,
                    name.as_ref().map_or(String::new(), |n| format!(" '{}'", n))
                ),
                permission: match action.as_str() {
                    "info" | "list" => PermissionLevel::Safe,
                    _ => PermissionLevel::SystemWrite,
                },
                explanation: format!(
                    "{} a Tazama video project via MCP bridge",
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

        Intent::TazamaTimeline {
            action,
            clip_id,
            position,
        } => {
            let mut args_json = serde_json::Map::new();
            args_json.insert(
                "action".to_string(),
                serde_json::Value::String(action.clone()),
            );
            if let Some(c) = clip_id {
                args_json.insert("clip_id".to_string(), serde_json::Value::String(c.clone()));
            }
            if let Some(p) = position {
                args_json.insert(
                    "position".to_string(),
                    serde_json::Value::String(p.to_string()),
                );
            }
            let body = serde_json::json!({"name": "tazama_timeline", "arguments": args_json});
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
                    "Tazama timeline: {}{}",
                    action,
                    clip_id
                        .as_ref()
                        .map_or(String::new(), |c| format!(" clip '{}'", c))
                ),
                permission: if action == "list" {
                    PermissionLevel::Safe
                } else {
                    PermissionLevel::SystemWrite
                },
                explanation: format!(
                    "{} clips on Tazama timeline via MCP bridge",
                    match action.as_str() {
                        "add" => "Adds",
                        "remove" => "Removes",
                        "split" => "Splits",
                        "trim" => "Trims",
                        "list" => "Lists",
                        _ => "Manages",
                    }
                ),
            })
        }

        Intent::TazamaEffects {
            action,
            effect_type,
            clip_id,
        } => {
            let mut args_json = serde_json::Map::new();
            args_json.insert(
                "action".to_string(),
                serde_json::Value::String(action.clone()),
            );
            if let Some(e) = effect_type {
                args_json.insert(
                    "effect_type".to_string(),
                    serde_json::Value::String(e.clone()),
                );
            }
            if let Some(c) = clip_id {
                args_json.insert("clip_id".to_string(), serde_json::Value::String(c.clone()));
            }
            let body = serde_json::json!({"name": "tazama_effects", "arguments": args_json});
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
                    "Tazama effects: {}{}",
                    action,
                    effect_type
                        .as_ref()
                        .map_or(String::new(), |e| format!(" '{}'", e))
                ),
                permission: if action == "list" {
                    PermissionLevel::Safe
                } else {
                    PermissionLevel::SystemWrite
                },
                explanation: format!(
                    "{} effects in Tazama via MCP bridge",
                    match action.as_str() {
                        "apply" => "Applies",
                        "remove" => "Removes",
                        "list" => "Lists",
                        "preview" => "Previews",
                        _ => "Manages",
                    }
                ),
            })
        }

        Intent::TazamaAi { action, options } => {
            let mut args_json = serde_json::Map::new();
            args_json.insert(
                "action".to_string(),
                serde_json::Value::String(action.clone()),
            );
            if let Some(o) = options {
                args_json.insert("options".to_string(), serde_json::Value::String(o.clone()));
            }
            let body = serde_json::json!({"name": "tazama_ai", "arguments": args_json});
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
                description: format!("Tazama AI: {}", action),
                permission: PermissionLevel::SystemWrite,
                explanation: format!("Runs AI {} on Tazama video via MCP bridge", action),
            })
        }

        Intent::TazamaExport { path, format } => {
            let mut args_json = serde_json::Map::new();
            if let Some(p) = path {
                args_json.insert("path".to_string(), serde_json::Value::String(p.clone()));
            }
            if let Some(f) = format {
                args_json.insert("format".to_string(), serde_json::Value::String(f.clone()));
            }
            let body = serde_json::json!({"name": "tazama_export", "arguments": args_json});
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
                    "Tazama export{}",
                    format
                        .as_ref()
                        .map_or(String::new(), |f| format!(" as {}", f))
                ),
                permission: PermissionLevel::SystemWrite,
                explanation: "Exports/renders Tazama video project via MCP bridge".to_string(),
            })
        }

        Intent::TazamaMedia { action, path } => {
            let mut args_json = serde_json::Map::new();
            args_json.insert(
                "action".to_string(),
                serde_json::Value::String(action.clone()),
            );
            if let Some(p) = path {
                args_json.insert("path".to_string(), serde_json::Value::String(p.clone()));
            }
            let body = serde_json::json!({"name": "tazama_media", "arguments": args_json});
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
                    "Tazama media: {}{}",
                    action,
                    path.as_ref().map_or(String::new(), |p| format!(" '{}'", p))
                ),
                permission: match action.as_str() {
                    "list" | "info" => PermissionLevel::Safe,
                    _ => PermissionLevel::SystemWrite,
                },
                explanation: "Manages media library via Tazama".to_string(),
            })
        }

        Intent::TazamaSubtitles { action, language } => {
            let mut args_json = serde_json::Map::new();
            args_json.insert(
                "action".to_string(),
                serde_json::Value::String(action.clone()),
            );
            if let Some(l) = language {
                args_json.insert("language".to_string(), serde_json::Value::String(l.clone()));
            }
            let body = serde_json::json!({"name": "tazama_subtitles", "arguments": args_json});
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
                    "Tazama subtitles: {}{}",
                    action,
                    language
                        .as_ref()
                        .map_or(String::new(), |l| format!(" ({})", l))
                ),
                permission: match action.as_str() {
                    "list" => PermissionLevel::Safe,
                    _ => PermissionLevel::SystemWrite,
                },
                explanation: "Manages subtitles via Tazama".to_string(),
            })
        }

        _ => unreachable!("translate_tazama called with non-tazama intent"),
    }
}
