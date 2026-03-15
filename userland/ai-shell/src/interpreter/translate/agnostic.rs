use anyhow::Result;

use super::mcp_helper::{insert_opt, insert_str, mcp_call};
use crate::interpreter::intent::{Intent, Translation};
use crate::security::PermissionLevel;

pub(crate) fn translate_agnostic(intent: &Intent) -> Result<Translation> {
    match intent {
        Intent::AgnosticRunSuite { suite, target_url } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "suite", suite);
            insert_opt(&mut a, "target_url", target_url);
            Ok(mcp_call(
                "agnostic_run_suite",
                a,
                format!("Run QA suite: {}", suite),
                PermissionLevel::SystemWrite,
                "Runs a QA test suite in Agnostic via MCP bridge".to_string(),
            ))
        }

        Intent::AgnosticTestStatus { run_id } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "run_id", run_id);
            Ok(mcp_call(
                "agnostic_test_status",
                a,
                format!("Test run status: {}", run_id),
                PermissionLevel::Safe,
                "Gets test run status from Agnostic via MCP bridge".to_string(),
            ))
        }

        Intent::AgnosticTestReport { run_id, format } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "run_id", run_id);
            insert_opt(&mut a, "format", format);
            Ok(mcp_call(
                "agnostic_test_report",
                a,
                format!("Test report: {}", run_id),
                PermissionLevel::Safe,
                "Gets test report from Agnostic via MCP bridge".to_string(),
            ))
        }

        Intent::AgnosticListSuites { category } => {
            let mut a = serde_json::Map::new();
            insert_opt(&mut a, "category", category);
            Ok(mcp_call(
                "agnostic_list_suites",
                a,
                format!(
                    "List QA suites{}",
                    category
                        .as_ref()
                        .map_or(String::new(), |c| format!(" ({})", c))
                ),
                PermissionLevel::Safe,
                "Lists available QA test suites from Agnostic via MCP bridge".to_string(),
            ))
        }

        Intent::AgnosticAgentStatus { agent_type } => {
            let mut a = serde_json::Map::new();
            insert_opt(&mut a, "agent_type", agent_type);
            Ok(mcp_call(
                "agnostic_agent_status",
                a,
                format!(
                    "QA agent status{}",
                    agent_type
                        .as_ref()
                        .map_or(String::new(), |t| format!(" ({})", t))
                ),
                PermissionLevel::Safe,
                "Gets QA agent status from Agnostic via MCP bridge".to_string(),
            ))
        }

        Intent::AgnosticCoverage { action, suite } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "action", action);
            insert_opt(&mut a, "suite", suite);
            Ok(mcp_call(
                "agnostic_coverage",
                a,
                format!(
                    "Agnostic coverage: {}{}",
                    action,
                    suite
                        .as_ref()
                        .map_or(String::new(), |s| format!(" '{}'", s))
                ),
                PermissionLevel::Safe,
                "Gets coverage report via Agnostic".to_string(),
            ))
        }

        Intent::AgnosticSchedule { action, suite } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "action", action);
            insert_opt(&mut a, "suite", suite);
            Ok(mcp_call(
                "agnostic_schedule",
                a,
                format!(
                    "Agnostic schedule: {}{}",
                    action,
                    suite
                        .as_ref()
                        .map_or(String::new(), |s| format!(" '{}'", s))
                ),
                match action.as_str() {
                    "list" => PermissionLevel::Safe,
                    _ => PermissionLevel::SystemWrite,
                },
                "Manages test schedules via Agnostic".to_string(),
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
