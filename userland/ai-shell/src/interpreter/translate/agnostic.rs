use anyhow::Result;

use crate::interpreter::intent::{Intent, Translation};
use crate::security::PermissionLevel;

pub(crate) fn translate_agnostic(intent: &Intent) -> Result<Translation> {
    match intent {
        Intent::AgnosticRunSuite { suite, target_url } => {
            let mut args_json = serde_json::Map::new();
            args_json.insert(
                "suite".to_string(),
                serde_json::Value::String(suite.clone()),
            );
            if let Some(url) = target_url {
                args_json.insert(
                    "target_url".to_string(),
                    serde_json::Value::String(url.clone()),
                );
            }
            let body = serde_json::json!({"name": "agnostic_run_suite", "arguments": args_json});
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
                description: format!("Run QA suite: {}", suite),
                permission: PermissionLevel::SystemWrite,
                explanation: "Runs a QA test suite in Agnostic via MCP bridge".to_string(),
            })
        }

        Intent::AgnosticTestStatus { run_id } => {
            let mut args_json = serde_json::Map::new();
            args_json.insert(
                "run_id".to_string(),
                serde_json::Value::String(run_id.clone()),
            );
            let body = serde_json::json!({"name": "agnostic_test_status", "arguments": args_json});
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
                description: format!("Test run status: {}", run_id),
                permission: PermissionLevel::Safe,
                explanation: "Gets test run status from Agnostic via MCP bridge".to_string(),
            })
        }

        Intent::AgnosticTestReport { run_id, format } => {
            let mut args_json = serde_json::Map::new();
            args_json.insert(
                "run_id".to_string(),
                serde_json::Value::String(run_id.clone()),
            );
            if let Some(f) = format {
                args_json.insert("format".to_string(), serde_json::Value::String(f.clone()));
            }
            let body = serde_json::json!({"name": "agnostic_test_report", "arguments": args_json});
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
                description: format!("Test report: {}", run_id),
                permission: PermissionLevel::Safe,
                explanation: "Gets test report from Agnostic via MCP bridge".to_string(),
            })
        }

        Intent::AgnosticListSuites { category } => {
            let mut args_json = serde_json::Map::new();
            if let Some(c) = category {
                args_json.insert("category".to_string(), serde_json::Value::String(c.clone()));
            }
            let body = serde_json::json!({"name": "agnostic_list_suites", "arguments": args_json});
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
                    "List QA suites{}",
                    category
                        .as_ref()
                        .map_or(String::new(), |c| format!(" ({})", c))
                ),
                permission: PermissionLevel::Safe,
                explanation: "Lists available QA test suites from Agnostic via MCP bridge"
                    .to_string(),
            })
        }

        Intent::AgnosticAgentStatus { agent_type } => {
            let mut args_json = serde_json::Map::new();
            if let Some(t) = agent_type {
                args_json.insert(
                    "agent_type".to_string(),
                    serde_json::Value::String(t.clone()),
                );
            }
            let body = serde_json::json!({"name": "agnostic_agent_status", "arguments": args_json});
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
                    "QA agent status{}",
                    agent_type
                        .as_ref()
                        .map_or(String::new(), |t| format!(" ({})", t))
                ),
                permission: PermissionLevel::Safe,
                explanation: "Gets QA agent status from Agnostic via MCP bridge".to_string(),
            })
        }

        Intent::AgnosticCoverage { action, suite } => {
            let mut args_json = serde_json::Map::new();
            args_json.insert(
                "action".to_string(),
                serde_json::Value::String(action.clone()),
            );
            if let Some(s) = suite {
                args_json.insert("suite".to_string(), serde_json::Value::String(s.clone()));
            }
            let body = serde_json::json!({"name": "agnostic_coverage", "arguments": args_json});
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
                    "Agnostic coverage: {}{}",
                    action,
                    suite
                        .as_ref()
                        .map_or(String::new(), |s| format!(" '{}'", s))
                ),
                permission: PermissionLevel::Safe,
                explanation: "Gets coverage report via Agnostic".to_string(),
            })
        }

        Intent::AgnosticSchedule { action, suite } => {
            let mut args_json = serde_json::Map::new();
            args_json.insert(
                "action".to_string(),
                serde_json::Value::String(action.clone()),
            );
            if let Some(s) = suite {
                args_json.insert("suite".to_string(), serde_json::Value::String(s.clone()));
            }
            let body = serde_json::json!({"name": "agnostic_schedule", "arguments": args_json});
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
                    "Agnostic schedule: {}{}",
                    action,
                    suite
                        .as_ref()
                        .map_or(String::new(), |s| format!(" '{}'", s))
                ),
                permission: match action.as_str() {
                    "list" => PermissionLevel::Safe,
                    _ => PermissionLevel::SystemWrite,
                },
                explanation: "Manages test schedules via Agnostic".to_string(),
            })
        }

        _ => unreachable!("translate_agnostic called with non-agnostic intent"),
    }
}
