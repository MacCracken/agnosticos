use anyhow::Result;

use super::mcp_helper::{insert_opt, insert_str, mcp_call};
use crate::interpreter::intent::{Intent, Translation};
use crate::security::PermissionLevel;

pub(crate) fn translate_yeoman(intent: &Intent) -> Result<Translation> {
    match intent {
        Intent::YeomanAgents {
            action,
            agent_id,
            name,
        } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "action", action);
            insert_opt(&mut a, "agent_id", agent_id);
            insert_opt(&mut a, "name", name);
            Ok(mcp_call(
                "yeoman_agents",
                a,
                format!(
                    "SecureYeoman agents: {}{}",
                    action,
                    name.as_ref().map_or(String::new(), |n| format!(" '{}'", n))
                ),
                match action.as_str() {
                    "list" | "status" | "info" => PermissionLevel::Safe,
                    _ => PermissionLevel::SystemWrite,
                },
                "Lists/Deploys/Stops/Queries agents via SecureYeoman MCP bridge".to_string(),
            ))
        }

        Intent::YeomanTasks {
            action,
            description,
            task_id,
        } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "action", action);
            insert_opt(&mut a, "description", description);
            insert_opt(&mut a, "task_id", task_id);
            Ok(mcp_call(
                "yeoman_tasks",
                a,
                format!(
                    "SecureYeoman task: {}{}",
                    action,
                    task_id
                        .as_ref()
                        .map_or(String::new(), |id| format!(" '{}'", id))
                ),
                match action.as_str() {
                    "list" | "status" => PermissionLevel::Safe,
                    _ => PermissionLevel::SystemWrite,
                },
                "Assigns/Lists/Checks/Cancels tasks via SecureYeoman MCP bridge".to_string(),
            ))
        }

        Intent::YeomanTools { action, query } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "action", action);
            insert_opt(&mut a, "query", query);
            Ok(mcp_call(
                "yeoman_tools",
                a,
                format!(
                    "SecureYeoman tools: {}{}",
                    action,
                    query
                        .as_ref()
                        .map_or(String::new(), |q| format!(" '{}'", q))
                ),
                PermissionLevel::Safe,
                "Queries MCP tools catalog via SecureYeoman MCP bridge".to_string(),
            ))
        }

        Intent::YeomanIntegrations { action, name } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "action", action);
            insert_opt(&mut a, "name", name);
            Ok(mcp_call(
                "yeoman_integrations",
                a,
                format!(
                    "SecureYeoman integration: {}{}",
                    action,
                    name.as_ref().map_or(String::new(), |n| format!(" '{}'", n))
                ),
                match action.as_str() {
                    "list" | "status" => PermissionLevel::Safe,
                    _ => PermissionLevel::SystemWrite,
                },
                "Lists/Enables/Disables/Checks integrations via SecureYeoman MCP bridge"
                    .to_string(),
            ))
        }

        Intent::YeomanStatus => Ok(mcp_call(
            "yeoman_status",
            serde_json::Map::new(),
            "SecureYeoman status".to_string(),
            PermissionLevel::Safe,
            "Checks SecureYeoman platform health via MCP bridge".to_string(),
        )),

        Intent::YeomanLogs { action, agent_id } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "action", action);
            insert_opt(&mut a, "agent_id", agent_id);
            Ok(mcp_call(
                "yeoman_logs",
                a,
                format!(
                    "SecureYeoman logs: {}{}",
                    action,
                    agent_id
                        .as_ref()
                        .map_or(String::new(), |id| format!(" ({})", id))
                ),
                PermissionLevel::Safe,
                "Queries agent logs via SecureYeoman".to_string(),
            ))
        }

        Intent::YeomanWorkflows { action, name } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "action", action);
            insert_opt(&mut a, "name", name);
            Ok(mcp_call(
                "yeoman_workflows",
                a,
                format!(
                    "SecureYeoman workflow: {}{}",
                    action,
                    name.as_ref().map_or(String::new(), |n| format!(" '{}'", n))
                ),
                match action.as_str() {
                    "list" | "status" => PermissionLevel::Safe,
                    _ => PermissionLevel::SystemWrite,
                },
                "Manages workflows via SecureYeoman".to_string(),
            ))
        }

        Intent::YeomanRegisterTools { action } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "action", action);
            Ok(mcp_call(
                "yeoman_register_tools",
                a,
                format!("SecureYeoman register tools: {}", action),
                PermissionLevel::SystemWrite,
                "Registers SecureYeoman MCP tool catalog into daimon registry".to_string(),
            ))
        }

        Intent::YeomanToolExecute { tool_name, args } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "tool_name", tool_name);
            insert_opt(&mut a, "args", args);
            Ok(mcp_call(
                "yeoman_tool_execute",
                a,
                format!("SecureYeoman execute tool: {}", tool_name),
                PermissionLevel::SystemWrite,
                "Executes a SecureYeoman tool by name via bridge".to_string(),
            ))
        }

        Intent::YeomanBrainQuery { query, limit } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "query", query);
            insert_opt(&mut a, "limit", limit);
            Ok(mcp_call(
                "yeoman_brain_query",
                a,
                format!("SecureYeoman brain query: {}", query),
                PermissionLevel::Safe,
                "Queries SecureYeoman knowledge brain".to_string(),
            ))
        }

        Intent::YeomanBrainSync { action, topic } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "action", action);
            insert_opt(&mut a, "topic", topic);
            Ok(mcp_call(
                "yeoman_brain_sync",
                a,
                format!("SecureYeoman brain sync: {}", action),
                PermissionLevel::SystemWrite,
                "Syncs knowledge between SecureYeoman and AGNOS RAG".to_string(),
            ))
        }

        Intent::YeomanTokenBudget {
            action,
            pool,
            amount,
        } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "action", action);
            insert_opt(&mut a, "pool", pool);
            insert_opt(&mut a, "amount", amount);
            Ok(mcp_call(
                "yeoman_token_budget",
                a,
                format!("SecureYeoman token budget: {}", action),
                match action.as_str() {
                    "list" | "check" => PermissionLevel::Safe,
                    _ => PermissionLevel::SystemWrite,
                },
                "Manages SecureYeoman agent token budgets via hoosh".to_string(),
            ))
        }

        Intent::YeomanEvents { action, limit } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "action", action);
            insert_opt(&mut a, "limit", limit);
            Ok(mcp_call(
                "yeoman_events",
                a,
                format!("SecureYeoman events: {}", action),
                PermissionLevel::Safe,
                "Queries SecureYeoman event stream".to_string(),
            ))
        }

        Intent::YeomanSwarm {
            action,
            swarm_id,
            capability,
        } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "action", action);
            insert_opt(&mut a, "swarm_id", swarm_id);
            insert_opt(&mut a, "capability", capability);
            Ok(mcp_call(
                "yeoman_swarm",
                a,
                format!("SecureYeoman swarm: {}", action),
                match action.as_str() {
                    "handoff" => PermissionLevel::SystemWrite,
                    _ => PermissionLevel::Safe,
                },
                "Queries SecureYeoman swarm topology".to_string(),
            ))
        }

        _ => unreachable!("translate_yeoman called with non-yeoman intent"),
    }
}
