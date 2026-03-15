use anyhow::Result;

use super::mcp_helper::{insert_opt, insert_str, mcp_call};
use crate::interpreter::intent::{Intent, Translation};
use crate::security::PermissionLevel;

pub(crate) fn translate_delta(intent: &Intent) -> Result<Translation> {
    match intent {
        Intent::DeltaCreateRepo { name, description } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "name", name);
            insert_opt(&mut a, "description", description);
            Ok(mcp_call(
                "delta_create_repository",
                a,
                format!("Create Delta repository: {}", name),
                PermissionLevel::SystemWrite,
                "Creates a git repository in Delta via MCP bridge".to_string(),
            ))
        }

        Intent::DeltaListRepos => Ok(mcp_call(
            "delta_list_repositories",
            serde_json::Map::new(),
            "List Delta repositories".to_string(),
            PermissionLevel::Safe,
            "Lists git repositories from Delta via MCP bridge".to_string(),
        )),

        Intent::DeltaPr {
            action,
            repo,
            title,
        } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "action", action);
            insert_opt(&mut a, "repo", repo);
            insert_opt(&mut a, "title", title);
            Ok(mcp_call(
                "delta_pull_request",
                a,
                format!("Delta PR: {}", action),
                if action == "list" {
                    PermissionLevel::Safe
                } else {
                    PermissionLevel::SystemWrite
                },
                format!(
                    "{} pull request in Delta via MCP bridge",
                    match action.as_str() {
                        "create" => "Creates a",
                        "merge" => "Merges a",
                        "close" => "Closes a",
                        _ => "Lists",
                    }
                ),
            ))
        }

        Intent::DeltaPush { repo, branch } => {
            let mut a = serde_json::Map::new();
            insert_opt(&mut a, "repo", repo);
            insert_opt(&mut a, "branch", branch);
            Ok(mcp_call(
                "delta_push",
                a,
                format!(
                    "Push to Delta{}",
                    repo.as_ref().map_or(String::new(), |r| format!(": {}", r))
                ),
                PermissionLevel::SystemWrite,
                "Pushes code to a Delta repository via MCP bridge".to_string(),
            ))
        }

        Intent::DeltaCiStatus { repo } => {
            let mut a = serde_json::Map::new();
            insert_opt(&mut a, "repo", repo);
            Ok(mcp_call(
                "delta_ci_status",
                a,
                format!(
                    "Delta CI status{}",
                    repo.as_ref()
                        .map_or(String::new(), |r| format!(" for {}", r))
                ),
                PermissionLevel::Safe,
                "Retrieves CI pipeline status from Delta via MCP bridge".to_string(),
            ))
        }

        Intent::DeltaBranches { action, repo, name } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "action", action);
            insert_opt(&mut a, "repo", repo);
            insert_opt(&mut a, "name", name);
            Ok(mcp_call(
                "delta_branches",
                a,
                format!(
                    "Delta branches: {}{}",
                    action,
                    name.as_ref().map_or(String::new(), |n| format!(" '{}'", n))
                ),
                match action.as_str() {
                    "list" | "info" => PermissionLevel::Safe,
                    _ => PermissionLevel::SystemWrite,
                },
                "Manages branches via Delta".to_string(),
            ))
        }

        Intent::DeltaReview { action, pr_id } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "action", action);
            insert_opt(&mut a, "pr_id", pr_id);
            Ok(mcp_call(
                "delta_review",
                a,
                format!(
                    "Delta review: {}{}",
                    action,
                    pr_id
                        .as_ref()
                        .map_or(String::new(), |id| format!(" PR #{}", id))
                ),
                match action.as_str() {
                    "list" => PermissionLevel::Safe,
                    _ => PermissionLevel::SystemWrite,
                },
                "Manages code reviews via Delta".to_string(),
            ))
        }

        _ => unreachable!("translate_delta called with non-delta intent"),
    }
}
