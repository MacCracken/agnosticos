use anyhow::Result;

use crate::interpreter::intent::{Intent, Translation};
use crate::security::PermissionLevel;

pub(crate) fn translate_delta(intent: &Intent) -> Result<Translation> {
    match intent {
        Intent::DeltaCreateRepo { name, description } => {
            let mut args_json = serde_json::Map::new();
            args_json.insert(
                "name".to_string(),
                serde_json::Value::String(name.clone()),
            );
            if let Some(desc) = description {
                args_json.insert(
                    "description".to_string(),
                    serde_json::Value::String(desc.clone()),
                );
            }
            let body =
                serde_json::json!({"name": "delta_create_repository", "arguments": args_json});
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
                description: format!("Create Delta repository: {}", name),
                permission: PermissionLevel::SystemWrite,
                explanation: "Creates a git repository in Delta via MCP bridge".to_string(),
            })
        }

        Intent::DeltaListRepos => {
            let body = serde_json::json!({"name": "delta_list_repositories", "arguments": {}});
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
                description: "List Delta repositories".to_string(),
                permission: PermissionLevel::Safe,
                explanation: "Lists git repositories from Delta via MCP bridge".to_string(),
            })
        }

        Intent::DeltaPr {
            action,
            repo,
            title,
        } => {
            let mut args_json = serde_json::Map::new();
            args_json.insert(
                "action".to_string(),
                serde_json::Value::String(action.clone()),
            );
            if let Some(r) = repo {
                args_json.insert("repo".to_string(), serde_json::Value::String(r.clone()));
            }
            if let Some(t) = title {
                args_json.insert("title".to_string(), serde_json::Value::String(t.clone()));
            }
            let body =
                serde_json::json!({"name": "delta_pull_request", "arguments": args_json});
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
                description: format!("Delta PR: {}", action),
                permission: if action == "list" {
                    PermissionLevel::Safe
                } else {
                    PermissionLevel::SystemWrite
                },
                explanation: format!(
                    "{} pull request in Delta via MCP bridge",
                    match action.as_str() {
                        "create" => "Creates a",
                        "merge" => "Merges a",
                        "close" => "Closes a",
                        _ => "Lists",
                    }
                ),
            })
        }

        Intent::DeltaPush { repo, branch } => {
            let mut args_json = serde_json::Map::new();
            if let Some(r) = repo {
                args_json.insert("repo".to_string(), serde_json::Value::String(r.clone()));
            }
            if let Some(b) = branch {
                args_json.insert("branch".to_string(), serde_json::Value::String(b.clone()));
            }
            let body = serde_json::json!({"name": "delta_push", "arguments": args_json});
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
                    "Push to Delta{}",
                    repo.as_ref().map_or(String::new(), |r| format!(": {}", r))
                ),
                permission: PermissionLevel::SystemWrite,
                explanation: "Pushes code to a Delta repository via MCP bridge".to_string(),
            })
        }

        Intent::DeltaCiStatus { repo } => {
            let mut args_json = serde_json::Map::new();
            if let Some(r) = repo {
                args_json.insert("repo".to_string(), serde_json::Value::String(r.clone()));
            }
            let body = serde_json::json!({"name": "delta_ci_status", "arguments": args_json});
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
                    "Delta CI status{}",
                    repo.as_ref().map_or(String::new(), |r| format!(" for {}", r))
                ),
                permission: PermissionLevel::Safe,
                explanation: "Retrieves CI pipeline status from Delta via MCP bridge".to_string(),
            })
        }

        _ => unreachable!("translate_delta called with non-delta intent"),
    }
}
