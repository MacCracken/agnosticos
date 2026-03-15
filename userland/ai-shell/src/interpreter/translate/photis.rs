use anyhow::Result;

use super::mcp_helper::{insert_opt, insert_str, mcp_call};
use crate::interpreter::intent::{Intent, Translation};
use crate::security::PermissionLevel;

pub(crate) fn translate_photis(intent: &Intent) -> Result<Translation> {
    match intent {
        Intent::TaskList { status } => {
            let mut a = serde_json::Map::new();
            insert_opt(&mut a, "status", status);
            Ok(mcp_call(
                "photis_list_tasks",
                a,
                format!(
                    "List tasks{}",
                    status
                        .as_ref()
                        .map_or(String::new(), |s| format!(" with status {}", s))
                ),
                PermissionLevel::Safe,
                "Lists tasks from Photis Nadi via MCP bridge".to_string(),
            ))
        }

        Intent::TaskCreate { title, priority } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "title", title);
            insert_opt(&mut a, "priority", priority);
            Ok(mcp_call(
                "photis_create_task",
                a,
                format!("Create task: {}", title),
                PermissionLevel::SystemWrite,
                "Creates a new task in Photis Nadi via MCP bridge".to_string(),
            ))
        }

        Intent::TaskUpdate { task_id, status } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "task_id", task_id);
            insert_opt(&mut a, "status", status);
            Ok(mcp_call(
                "photis_update_task",
                a,
                format!("Update task {}", task_id),
                PermissionLevel::SystemWrite,
                "Updates a task in Photis Nadi via MCP bridge".to_string(),
            ))
        }

        Intent::RitualCheck { date } => {
            let mut a = serde_json::Map::new();
            insert_opt(&mut a, "date", date);
            Ok(mcp_call(
                "photis_get_rituals",
                a,
                "Check daily rituals".to_string(),
                PermissionLevel::Safe,
                "Retrieves ritual/habit status from Photis Nadi via MCP bridge".to_string(),
            ))
        }

        Intent::ProductivityStats { period } => {
            let mut a = serde_json::Map::new();
            insert_opt(&mut a, "period", period);
            Ok(mcp_call(
                "photis_analytics",
                a,
                format!(
                    "Productivity analytics{}",
                    period
                        .as_ref()
                        .map_or(String::new(), |p| format!(" for {}", p))
                ),
                PermissionLevel::Safe,
                "Retrieves productivity analytics from Photis Nadi via MCP bridge".to_string(),
            ))
        }

        Intent::PhotoisBoards { action, name } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "action", action);
            insert_opt(&mut a, "name", name);
            Ok(mcp_call(
                "photis_boards",
                a,
                format!(
                    "Photis boards: {}{}",
                    action,
                    name.as_ref().map_or(String::new(), |n| format!(" '{}'", n))
                ),
                match action.as_str() {
                    "list" | "info" => PermissionLevel::Safe,
                    _ => PermissionLevel::SystemWrite,
                },
                "Manages boards via Photis Nadi".to_string(),
            ))
        }

        Intent::PhotoisNotes { action, content } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "action", action);
            insert_opt(&mut a, "content", content);
            Ok(mcp_call(
                "photis_notes",
                a,
                format!(
                    "Photis notes: {}{}",
                    action,
                    content
                        .as_ref()
                        .map_or(String::new(), |c| format!(" '{}'", c))
                ),
                match action.as_str() {
                    "list" | "get" | "search" => PermissionLevel::Safe,
                    _ => PermissionLevel::SystemWrite,
                },
                "Manages notes via Photis Nadi".to_string(),
            ))
        }

        _ => unreachable!("translate_photis called with non-photis intent"),
    }
}
