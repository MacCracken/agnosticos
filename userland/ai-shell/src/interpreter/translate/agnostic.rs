use anyhow::Result;

use super::mcp_helper::{insert_opt, insert_str, mcp_call};
use crate::interpreter::intent::{Intent, Translation};
use crate::security::PermissionLevel;

pub(crate) fn translate_agnostic(intent: &Intent) -> Result<Translation> {
    match intent {
        Intent::AgnosticSubmitTask {
            title,
            description,
            target_url,
        } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "title", title);
            insert_str(&mut a, "description", description.as_deref().unwrap_or(title));
            insert_opt(&mut a, "target_url", target_url);
            Ok(mcp_call(
                "agnostic_submit_task",
                a,
                format!("Submit QA task: {}", title),
                PermissionLevel::SystemWrite,
                "Submits a QA task to Agnostic via MCP bridge".to_string(),
            ))
        }

        Intent::AgnosticTaskStatus { task_id } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "task_id", task_id);
            Ok(mcp_call(
                "agnostic_task_status",
                a,
                format!("Task status: {}", task_id),
                PermissionLevel::Safe,
                "Gets task status from Agnostic via MCP bridge".to_string(),
            ))
        }

        Intent::AgnosticStructuredResults {
            session_id,
            result_type,
        } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "session_id", session_id);
            insert_opt(&mut a, "result_type", result_type);
            Ok(mcp_call(
                "agnostic_structured_results",
                a,
                format!("Results: {}", session_id),
                PermissionLevel::Safe,
                "Gets structured results from Agnostic via MCP bridge".to_string(),
            ))
        }

        Intent::AgnosticListPresets { domain } => {
            let mut a = serde_json::Map::new();
            insert_opt(&mut a, "domain", domain);
            Ok(mcp_call(
                "agnostic_list_presets",
                a,
                "Agnostic: list presets".to_string(),
                PermissionLevel::Safe,
                "Lists crew presets from Agnostic via MCP bridge".to_string(),
            ))
        }

        Intent::AgnosticAgentStatus => {
            let a = serde_json::Map::new();
            Ok(mcp_call(
                "agnostic_agent_status",
                a,
                "QA agent status".to_string(),
                PermissionLevel::Safe,
                "Gets QA agent status from Agnostic via MCP bridge".to_string(),
            ))
        }

        Intent::AgnosticDashboard { section } => {
            let mut a = serde_json::Map::new();
            insert_opt(&mut a, "section", section);
            Ok(mcp_call(
                "agnostic_dashboard",
                a,
                format!(
                    "Agnostic dashboard{}",
                    section
                        .as_ref()
                        .map_or(String::new(), |s| format!(" ({})", s))
                ),
                PermissionLevel::Safe,
                "Gets QA dashboard snapshot from Agnostic".to_string(),
            ))
        }

        Intent::AgnosticTrends => {
            let a = serde_json::Map::new();
            Ok(mcp_call(
                "agnostic_trends",
                a,
                "Agnostic: quality trends".to_string(),
                PermissionLevel::Safe,
                "Gets quality metric trends from Agnostic test history".to_string(),
            ))
        }

        Intent::AgnosticCompare {
            session_a,
            session_b,
        } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "session_a", session_a);
            insert_str(&mut a, "session_b", session_b);
            Ok(mcp_call(
                "agnostic_compare",
                a,
                format!("Agnostic: compare {} vs {}", session_a, session_b),
                PermissionLevel::Safe,
                "Compares two test sessions side-by-side via Agnostic".to_string(),
            ))
        }

        Intent::AgnosticRunCrew { title, preset } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "title", title);
            insert_str(&mut a, "description", title);
            insert_opt(&mut a, "preset", preset);
            Ok(mcp_call(
                "agnostic_run_crew",
                a,
                format!("Agnostic: run crew '{}'", title),
                PermissionLevel::SystemWrite,
                "Runs an agent crew via Agnostic MCP bridge".to_string(),
            ))
        }

        Intent::AgnosticCrewStatus { crew_id } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "crew_id", crew_id);
            Ok(mcp_call(
                "agnostic_crew_status",
                a,
                format!("Agnostic: crew status {}", crew_id),
                PermissionLevel::Safe,
                "Checks crew run status via Agnostic MCP bridge".to_string(),
            ))
        }

        Intent::AgnosticListPresets { domain } => {
            let mut a = serde_json::Map::new();
            insert_opt(&mut a, "domain", domain);
            Ok(mcp_call(
                "agnostic_list_presets",
                a,
                "Agnostic: list presets".to_string(),
                PermissionLevel::Safe,
                "Lists agent crew presets via Agnostic MCP bridge".to_string(),
            ))
        }

        Intent::AgnosticListDefinitions { domain } => {
            let mut a = serde_json::Map::new();
            insert_opt(&mut a, "domain", domain);
            Ok(mcp_call(
                "agnostic_list_definitions",
                a,
                "Agnostic: list agent definitions".to_string(),
                PermissionLevel::Safe,
                "Lists agent definitions via Agnostic MCP bridge".to_string(),
            ))
        }

        Intent::AgnosticCreateAgent {
            agent_key,
            name,
            role,
        } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "agent_key", agent_key);
            insert_str(&mut a, "name", name);
            insert_str(&mut a, "role", role);
            insert_str(&mut a, "goal", role);
            a.insert(
                "backstory".to_string(),
                serde_json::Value::String(format!("Agent specializing in {}", role)),
            );
            Ok(mcp_call(
                "agnostic_create_agent",
                a,
                format!("Agnostic: create agent '{}'", agent_key),
                PermissionLevel::SystemWrite,
                "Creates a new agent definition via Agnostic MCP bridge".to_string(),
            ))
        }

        _ => unreachable!("translate_agnostic called with non-agnostic intent"),
    }
}
