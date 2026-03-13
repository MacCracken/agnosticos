use anyhow::Result;

use crate::interpreter::intent::{Intent, Translation};
use crate::security::PermissionLevel;

pub(crate) fn translate_shruti(intent: &Intent) -> Result<Translation> {
    match intent {
        Intent::ShrutiSession { action, name } => {
            let mut args_json = serde_json::Map::new();
            args_json.insert(
                "action".to_string(),
                serde_json::Value::String(action.clone()),
            );
            if let Some(n) = name {
                args_json.insert("name".to_string(), serde_json::Value::String(n.clone()));
            }
            let body = serde_json::json!({"name": "shruti_session", "arguments": args_json});
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
                    "Shruti session: {}{}",
                    action,
                    name.as_ref().map_or(String::new(), |n| format!(" '{}'", n))
                ),
                permission: match action.as_str() {
                    "info" => PermissionLevel::Safe,
                    _ => PermissionLevel::SystemWrite,
                },
                explanation: format!(
                    "{} a Shruti DAW session via MCP bridge",
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

        Intent::ShrutiTrack { action, name, kind } => {
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
            let body = serde_json::json!({"name": "shruti_tracks", "arguments": args_json});
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
                    "Shruti track: {}{}",
                    action,
                    name.as_ref().map_or(String::new(), |n| format!(" '{}'", n))
                ),
                permission: if action == "list" {
                    PermissionLevel::Safe
                } else {
                    PermissionLevel::SystemWrite
                },
                explanation: format!(
                    "{} track in Shruti via MCP bridge",
                    match action.as_str() {
                        "add" => "Adds a",
                        "remove" => "Removes a",
                        "list" => "Lists",
                        "rename" => "Renames a",
                        _ => "Manages a",
                    }
                ),
            })
        }

        Intent::ShrutiMixer {
            track,
            gain,
            mute,
            solo,
        } => {
            let mut args_json = serde_json::Map::new();
            args_json.insert(
                "track".to_string(),
                serde_json::Value::String(track.clone()),
            );
            if let Some(g) = gain {
                args_json.insert(
                    "gain".to_string(),
                    serde_json::Value::Number(serde_json::Number::from_f64(*g).unwrap()),
                );
            }
            if let Some(m) = mute {
                args_json.insert("mute".to_string(), serde_json::Value::Bool(*m));
            }
            if let Some(s) = solo {
                args_json.insert("solo".to_string(), serde_json::Value::Bool(*s));
            }
            let body = serde_json::json!({"name": "shruti_mixer", "arguments": args_json});
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
                description: format!("Shruti mixer: {}", track),
                permission: PermissionLevel::SystemWrite,
                explanation: format!(
                    "Controls mixer for track '{}' in Shruti via MCP bridge",
                    track
                ),
            })
        }

        Intent::ShrutiTransport { action, value } => {
            let mut args_json = serde_json::Map::new();
            args_json.insert(
                "action".to_string(),
                serde_json::Value::String(action.clone()),
            );
            if let Some(v) = value {
                args_json.insert("value".to_string(), serde_json::Value::String(v.clone()));
            }
            let body = serde_json::json!({"name": "shruti_transport", "arguments": args_json});
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
                    "Shruti transport: {}{}",
                    action,
                    value
                        .as_ref()
                        .map_or(String::new(), |v| format!(" ({})", v))
                ),
                permission: match action.as_str() {
                    "status" => PermissionLevel::Safe,
                    _ => PermissionLevel::SystemWrite,
                },
                explanation: format!(
                    "{} Shruti transport via MCP bridge",
                    match action.as_str() {
                        "play" => "Starts playback on",
                        "pause" => "Pauses",
                        "stop" => "Stops",
                        "seek" => "Seeks position on",
                        "set_tempo" => "Sets tempo on",
                        _ => "Controls",
                    }
                ),
            })
        }

        Intent::ShrutiExport { path, format } => {
            let mut args_json = serde_json::Map::new();
            if let Some(p) = path {
                args_json.insert("path".to_string(), serde_json::Value::String(p.clone()));
            }
            if let Some(f) = format {
                args_json.insert("format".to_string(), serde_json::Value::String(f.clone()));
            }
            let body = serde_json::json!({"name": "shruti_export", "arguments": args_json});
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
                    "Shruti export{}",
                    format
                        .as_ref()
                        .map_or(String::new(), |f| format!(" as {}", f))
                ),
                permission: PermissionLevel::SystemWrite,
                explanation: "Exports/bounces Shruti session to audio file via MCP bridge"
                    .to_string(),
            })
        }

        _ => unreachable!("translate_shruti called with non-shruti intent"),
    }
}
