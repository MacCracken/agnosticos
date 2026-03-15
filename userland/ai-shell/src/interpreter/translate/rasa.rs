use anyhow::Result;

use super::mcp_helper::{insert_opt, insert_str, mcp_call};
use crate::interpreter::intent::{Intent, Translation};
use crate::security::PermissionLevel;

pub(crate) fn translate_rasa(intent: &Intent) -> Result<Translation> {
    match intent {
        Intent::RasaCanvas { action, name } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "action", action);
            insert_opt(&mut a, "name", name);
            Ok(mcp_call(
                "rasa_canvas",
                a,
                format!(
                    "Rasa canvas: {}{}",
                    action,
                    name.as_ref().map_or(String::new(), |n| format!(" '{}'", n))
                ),
                match action.as_str() {
                    "info" | "list" => PermissionLevel::Safe,
                    _ => PermissionLevel::SystemWrite,
                },
                format!(
                    "{} a Rasa image canvas via MCP bridge",
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

        Intent::RasaLayers { action, name, kind } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "action", action);
            insert_opt(&mut a, "name", name);
            insert_opt(&mut a, "kind", kind);
            Ok(mcp_call(
                "rasa_layers",
                a,
                format!(
                    "Rasa layers: {}{}",
                    action,
                    name.as_ref().map_or(String::new(), |n| format!(" '{}'", n))
                ),
                if action == "list" {
                    PermissionLevel::Safe
                } else {
                    PermissionLevel::SystemWrite
                },
                format!(
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
            ))
        }

        Intent::RasaTools { action, params } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "action", action);
            insert_opt(&mut a, "params", params);
            Ok(mcp_call(
                "rasa_tools",
                a,
                format!(
                    "Rasa tools: {}{}",
                    action,
                    params
                        .as_ref()
                        .map_or(String::new(), |p| format!(" ({})", p))
                ),
                PermissionLevel::SystemWrite,
                format!("Applies {} tool in Rasa via MCP bridge", action),
            ))
        }

        Intent::RasaAi { action, prompt } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "action", action);
            insert_opt(&mut a, "prompt", prompt);
            Ok(mcp_call(
                "rasa_ai",
                a,
                format!(
                    "Rasa AI: {}{}",
                    action,
                    prompt
                        .as_ref()
                        .map_or(String::new(), |p| format!(" '{}'", p))
                ),
                PermissionLevel::SystemWrite,
                format!("Runs AI {} on Rasa image via MCP bridge", action),
            ))
        }

        Intent::RasaExport { path, format } => {
            let mut a = serde_json::Map::new();
            insert_opt(&mut a, "path", path);
            insert_opt(&mut a, "format", format);
            Ok(mcp_call(
                "rasa_export",
                a,
                format!(
                    "Rasa export{}",
                    format
                        .as_ref()
                        .map_or(String::new(), |f| format!(" as {}", f))
                ),
                PermissionLevel::SystemWrite,
                "Exports Rasa image via MCP bridge".to_string(),
            ))
        }

        Intent::RasaBatch { action, path } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "action", action);
            insert_opt(&mut a, "path", path);
            Ok(mcp_call(
                "rasa_batch",
                a,
                format!(
                    "Rasa batch: {}{}",
                    action,
                    path.as_ref().map_or(String::new(), |p| format!(" '{}'", p))
                ),
                match action.as_str() {
                    "list" => PermissionLevel::Safe,
                    _ => PermissionLevel::SystemWrite,
                },
                "Batch image operations via Rasa".to_string(),
            ))
        }

        Intent::RasaTemplates { action, name } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "action", action);
            insert_opt(&mut a, "name", name);
            Ok(mcp_call(
                "rasa_templates",
                a,
                format!(
                    "Rasa templates: {}{}",
                    action,
                    name.as_ref().map_or(String::new(), |n| format!(" '{}'", n))
                ),
                match action.as_str() {
                    "list" | "info" => PermissionLevel::Safe,
                    _ => PermissionLevel::SystemWrite,
                },
                "Manages design templates via Rasa".to_string(),
            ))
        }

        _ => unreachable!("translate_rasa called with non-rasa intent"),
    }
}
