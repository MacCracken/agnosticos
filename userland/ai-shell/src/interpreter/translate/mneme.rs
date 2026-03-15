use anyhow::Result;

use super::mcp_helper::{insert_opt, insert_str, mcp_call};
use crate::interpreter::intent::{Intent, Translation};
use crate::security::PermissionLevel;

pub(crate) fn translate_mneme(intent: &Intent) -> Result<Translation> {
    match intent {
        Intent::MnemeNotebook { action, name } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "action", action);
            insert_opt(&mut a, "name", name);
            Ok(mcp_call(
                "mneme_notebook",
                a,
                format!(
                    "Mneme notebook: {}{}",
                    action,
                    name.as_ref().map_or(String::new(), |n| format!(" '{}'", n))
                ),
                match action.as_str() {
                    "list" | "info" => PermissionLevel::Safe,
                    _ => PermissionLevel::SystemWrite,
                },
                format!(
                    "{} Mneme notebooks via MCP bridge",
                    match action.as_str() {
                        "create" => "Creates",
                        "open" => "Opens",
                        "delete" => "Deletes",
                        "list" => "Lists",
                        _ => "Queries",
                    }
                ),
            ))
        }

        Intent::MnemeNotes {
            action,
            title,
            notebook_id,
        } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "action", action);
            insert_opt(&mut a, "title", title);
            insert_opt(&mut a, "notebook_id", notebook_id);
            Ok(mcp_call(
                "mneme_notes",
                a,
                format!(
                    "Mneme notes: {}{}",
                    action,
                    title
                        .as_ref()
                        .map_or(String::new(), |t| format!(" '{}'", t))
                ),
                match action.as_str() {
                    "list" | "get" => PermissionLevel::Safe,
                    _ => PermissionLevel::SystemWrite,
                },
                format!(
                    "{} notes in Mneme via MCP bridge",
                    match action.as_str() {
                        "create" => "Creates",
                        "edit" => "Edits",
                        "delete" => "Deletes",
                        "list" => "Lists",
                        _ => "Manages",
                    }
                ),
            ))
        }

        Intent::MnemeSearch { query, mode } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "query", query);
            insert_opt(&mut a, "mode", mode);
            Ok(mcp_call(
                "mneme_search",
                a,
                format!(
                    "Mneme search: '{}'{}",
                    query,
                    mode.as_ref().map_or(String::new(), |m| format!(" ({})", m))
                ),
                PermissionLevel::Safe,
                "Searches Mneme knowledge base via MCP bridge".to_string(),
            ))
        }

        Intent::MnemeAi { action, note_id } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "action", action);
            insert_opt(&mut a, "note_id", note_id);
            Ok(mcp_call(
                "mneme_ai",
                a,
                format!(
                    "Mneme AI: {}{}",
                    action,
                    note_id
                        .as_ref()
                        .map_or(String::new(), |id| format!(" ({})", id))
                ),
                PermissionLevel::SystemWrite,
                format!("Runs AI {} on Mneme knowledge via MCP bridge", action),
            ))
        }

        Intent::MnemeGraph { action, node_id } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "action", action);
            insert_opt(&mut a, "node_id", node_id);
            Ok(mcp_call(
                "mneme_graph",
                a,
                format!(
                    "Mneme graph: {}{}",
                    action,
                    node_id
                        .as_ref()
                        .map_or(String::new(), |id| format!(" ({})", id))
                ),
                match action.as_str() {
                    "view" | "stats" => PermissionLevel::Safe,
                    _ => PermissionLevel::SystemWrite,
                },
                "Manages Mneme knowledge graph via MCP bridge".to_string(),
            ))
        }

        Intent::MnemeImport { action, path } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "action", action);
            insert_opt(&mut a, "path", path);
            Ok(mcp_call(
                "mneme_import",
                a,
                format!(
                    "Mneme import: {}{}",
                    action,
                    path.as_ref().map_or(String::new(), |p| format!(" '{}'", p))
                ),
                match action.as_str() {
                    "status" => PermissionLevel::Safe,
                    _ => PermissionLevel::SystemWrite,
                },
                "Imports documents into Mneme".to_string(),
            ))
        }

        Intent::MnemeTags { action, tag } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "action", action);
            insert_opt(&mut a, "tag", tag);
            Ok(mcp_call(
                "mneme_tags",
                a,
                format!(
                    "Mneme tags: {}{}",
                    action,
                    tag.as_ref().map_or(String::new(), |t| format!(" '{}'", t))
                ),
                match action.as_str() {
                    "list" | "search" => PermissionLevel::Safe,
                    _ => PermissionLevel::SystemWrite,
                },
                "Manages tags via Mneme".to_string(),
            ))
        }

        _ => unreachable!("translate_mneme called with non-mneme intent"),
    }
}
