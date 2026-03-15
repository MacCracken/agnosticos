use anyhow::Result;

use super::mcp_helper::{insert_opt, insert_str, mcp_call};
use crate::interpreter::intent::{Intent, Translation};
use crate::security::PermissionLevel;

pub(crate) fn translate_tazama(intent: &Intent) -> Result<Translation> {
    match intent {
        Intent::TazamaProject { action, name } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "action", action);
            insert_opt(&mut a, "name", name);
            Ok(mcp_call(
                "tazama_project",
                a,
                format!(
                    "Tazama project: {}{}",
                    action,
                    name.as_ref().map_or(String::new(), |n| format!(" '{}'", n))
                ),
                match action.as_str() {
                    "info" | "list" => PermissionLevel::Safe,
                    _ => PermissionLevel::SystemWrite,
                },
                format!(
                    "{} a Tazama video project via MCP bridge",
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

        Intent::TazamaTimeline {
            action,
            clip_id,
            position,
        } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "action", action);
            insert_opt(&mut a, "clip_id", clip_id);
            if let Some(p) = position {
                a.insert(
                    "position".to_string(),
                    serde_json::Value::String(p.to_string()),
                );
            }
            Ok(mcp_call(
                "tazama_timeline",
                a,
                format!(
                    "Tazama timeline: {}{}",
                    action,
                    clip_id
                        .as_ref()
                        .map_or(String::new(), |c| format!(" clip '{}'", c))
                ),
                if action == "list" {
                    PermissionLevel::Safe
                } else {
                    PermissionLevel::SystemWrite
                },
                format!(
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
            ))
        }

        Intent::TazamaEffects {
            action,
            effect_type,
            clip_id,
        } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "action", action);
            insert_opt(&mut a, "effect_type", effect_type);
            insert_opt(&mut a, "clip_id", clip_id);
            Ok(mcp_call(
                "tazama_effects",
                a,
                format!(
                    "Tazama effects: {}{}",
                    action,
                    effect_type
                        .as_ref()
                        .map_or(String::new(), |e| format!(" '{}'", e))
                ),
                if action == "list" {
                    PermissionLevel::Safe
                } else {
                    PermissionLevel::SystemWrite
                },
                format!(
                    "{} effects in Tazama via MCP bridge",
                    match action.as_str() {
                        "apply" => "Applies",
                        "remove" => "Removes",
                        "list" => "Lists",
                        "preview" => "Previews",
                        _ => "Manages",
                    }
                ),
            ))
        }

        Intent::TazamaAi { action, options } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "action", action);
            insert_opt(&mut a, "options", options);
            Ok(mcp_call(
                "tazama_ai",
                a,
                format!("Tazama AI: {}", action),
                PermissionLevel::SystemWrite,
                format!("Runs AI {} on Tazama video via MCP bridge", action),
            ))
        }

        Intent::TazamaExport { path, format } => {
            let mut a = serde_json::Map::new();
            insert_opt(&mut a, "path", path);
            insert_opt(&mut a, "format", format);
            Ok(mcp_call(
                "tazama_export",
                a,
                format!(
                    "Tazama export{}",
                    format
                        .as_ref()
                        .map_or(String::new(), |f| format!(" as {}", f))
                ),
                PermissionLevel::SystemWrite,
                "Exports/renders Tazama video project via MCP bridge".to_string(),
            ))
        }

        Intent::TazamaMedia { action, path } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "action", action);
            insert_opt(&mut a, "path", path);
            Ok(mcp_call(
                "tazama_media",
                a,
                format!(
                    "Tazama media: {}{}",
                    action,
                    path.as_ref().map_or(String::new(), |p| format!(" '{}'", p))
                ),
                match action.as_str() {
                    "list" | "info" => PermissionLevel::Safe,
                    _ => PermissionLevel::SystemWrite,
                },
                "Manages media library via Tazama".to_string(),
            ))
        }

        Intent::TazamaSubtitles { action, language } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "action", action);
            insert_opt(&mut a, "language", language);
            Ok(mcp_call(
                "tazama_subtitles",
                a,
                format!(
                    "Tazama subtitles: {}{}",
                    action,
                    language
                        .as_ref()
                        .map_or(String::new(), |l| format!(" ({})", l))
                ),
                match action.as_str() {
                    "list" => PermissionLevel::Safe,
                    _ => PermissionLevel::SystemWrite,
                },
                "Manages subtitles via Tazama".to_string(),
            ))
        }

        _ => unreachable!("translate_tazama called with non-tazama intent"),
    }
}
