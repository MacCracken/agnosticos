use anyhow::Result;

use super::mcp_helper::{insert_opt, insert_str, mcp_call};
use crate::interpreter::intent::{Intent, Translation};
use crate::security::PermissionLevel;

pub(crate) fn translate_shruti(intent: &Intent) -> Result<Translation> {
    match intent {
        Intent::ShrutiSession { action, name } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "action", action);
            insert_opt(&mut a, "name", name);
            Ok(mcp_call(
                "shruti_session",
                a,
                format!(
                    "Shruti session: {}{}",
                    action,
                    name.as_ref().map_or(String::new(), |n| format!(" '{}'", n))
                ),
                match action.as_str() {
                    "info" => PermissionLevel::Safe,
                    _ => PermissionLevel::SystemWrite,
                },
                format!(
                    "{} a Shruti DAW session via MCP bridge",
                    match action.as_str() {
                        "create" => "Creates",
                        "open" => "Opens",
                        "save" => "Saves",
                        "close" => "Closes",
                        _ => "Queries",
                    }
                ),
            ))
        }

        Intent::ShrutiTrack { action, name, kind } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "action", action);
            insert_opt(&mut a, "name", name);
            insert_opt(&mut a, "kind", kind);
            Ok(mcp_call(
                "shruti_tracks",
                a,
                format!(
                    "Shruti track: {}{}",
                    action,
                    name.as_ref().map_or(String::new(), |n| format!(" '{}'", n))
                ),
                if action == "list" {
                    PermissionLevel::Safe
                } else {
                    PermissionLevel::SystemWrite
                },
                format!(
                    "{} track in Shruti via MCP bridge",
                    match action.as_str() {
                        "add" => "Adds a",
                        "remove" => "Removes a",
                        "list" => "Lists",
                        "rename" => "Renames a",
                        _ => "Manages a",
                    }
                ),
            ))
        }

        Intent::ShrutiMixer {
            track,
            gain,
            mute,
            solo,
        } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "track", track);
            if let Some(g) = gain {
                a.insert(
                    "gain".to_string(),
                    serde_json::Value::Number(serde_json::Number::from_f64(*g).unwrap()),
                );
            }
            if let Some(m) = mute {
                a.insert("mute".to_string(), serde_json::Value::Bool(*m));
            }
            if let Some(s) = solo {
                a.insert("solo".to_string(), serde_json::Value::Bool(*s));
            }
            Ok(mcp_call(
                "shruti_mixer",
                a,
                format!("Shruti mixer: {}", track),
                PermissionLevel::SystemWrite,
                format!(
                    "Controls mixer for track '{}' in Shruti via MCP bridge",
                    track
                ),
            ))
        }

        Intent::ShrutiTransport { action, value } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "action", action);
            insert_opt(&mut a, "value", value);
            Ok(mcp_call(
                "shruti_transport",
                a,
                format!(
                    "Shruti transport: {}{}",
                    action,
                    value
                        .as_ref()
                        .map_or(String::new(), |v| format!(" ({})", v))
                ),
                match action.as_str() {
                    "status" => PermissionLevel::Safe,
                    _ => PermissionLevel::SystemWrite,
                },
                format!(
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
            ))
        }

        Intent::ShrutiExport { path, format } => {
            let mut a = serde_json::Map::new();
            insert_opt(&mut a, "path", path);
            insert_opt(&mut a, "format", format);
            Ok(mcp_call(
                "shruti_export",
                a,
                format!(
                    "Shruti export{}",
                    format
                        .as_ref()
                        .map_or(String::new(), |f| format!(" as {}", f))
                ),
                PermissionLevel::SystemWrite,
                "Exports/bounces Shruti session to audio file via MCP bridge".to_string(),
            ))
        }

        Intent::ShrutiPlugins { action, name } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "action", action);
            insert_opt(&mut a, "name", name);
            Ok(mcp_call(
                "shruti_plugins",
                a,
                format!(
                    "Shruti plugins: {}{}",
                    action,
                    name.as_ref().map_or(String::new(), |n| format!(" '{}'", n))
                ),
                match action.as_str() {
                    "list" | "info" => PermissionLevel::Safe,
                    _ => PermissionLevel::SystemWrite,
                },
                "Manages audio plugins via Shruti".to_string(),
            ))
        }

        Intent::ShrutiAi { action, track } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "action", action);
            insert_opt(&mut a, "track", track);
            Ok(mcp_call(
                "shruti_ai",
                a,
                format!(
                    "Shruti AI: {}{}",
                    action,
                    track
                        .as_ref()
                        .map_or(String::new(), |t| format!(" on '{}'", t))
                ),
                PermissionLevel::SystemWrite,
                "Runs AI audio features via Shruti".to_string(),
            ))
        }

        _ => unreachable!("translate_shruti called with non-shruti intent"),
    }
}
