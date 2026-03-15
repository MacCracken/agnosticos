use anyhow::Result;

use super::mcp_helper::{insert_opt, insert_str, mcp_call};
use crate::interpreter::intent::{Intent, Translation};
use crate::security::PermissionLevel;

pub(crate) fn translate_synapse(intent: &Intent) -> Result<Translation> {
    match intent {
        Intent::SynapseModels {
            action,
            name,
            source,
        } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "action", action);
            insert_opt(&mut a, "name", name);
            insert_opt(&mut a, "source", source);
            Ok(mcp_call(
                "synapse_models",
                a,
                format!(
                    "Synapse models: {}{}",
                    action,
                    name.as_ref().map_or(String::new(), |n| format!(" '{}'", n))
                ),
                match action.as_str() {
                    "list" | "info" => PermissionLevel::Safe,
                    _ => PermissionLevel::SystemWrite,
                },
                format!(
                    "{} model(s) via Synapse MCP bridge",
                    match action.as_str() {
                        "download" => "Downloads",
                        "delete" => "Deletes",
                        "list" => "Lists",
                        _ => "Queries",
                    }
                ),
            ))
        }

        Intent::SynapseServe { action, model } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "action", action);
            insert_opt(&mut a, "model", model);
            Ok(mcp_call(
                "synapse_serve",
                a,
                format!(
                    "Synapse serve: {}{}",
                    action,
                    model
                        .as_ref()
                        .map_or(String::new(), |m| format!(" '{}'", m))
                ),
                match action.as_str() {
                    "status" | "list" => PermissionLevel::Safe,
                    _ => PermissionLevel::SystemWrite,
                },
                format!(
                    "{} model serving via Synapse MCP bridge",
                    match action.as_str() {
                        "start" => "Starts",
                        "stop" => "Stops",
                        _ => "Queries",
                    }
                ),
            ))
        }

        Intent::SynapseFinetune {
            action,
            model,
            method,
        } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "action", action);
            insert_opt(&mut a, "model", model);
            insert_opt(&mut a, "method", method);
            Ok(mcp_call(
                "synapse_finetune",
                a,
                format!(
                    "Synapse finetune: {}{}",
                    action,
                    model
                        .as_ref()
                        .map_or(String::new(), |m| format!(" '{}'", m))
                ),
                match action.as_str() {
                    "status" | "list" => PermissionLevel::Safe,
                    _ => PermissionLevel::SystemWrite,
                },
                format!(
                    "{} fine-tuning job via Synapse MCP bridge",
                    match action.as_str() {
                        "start" => "Starts",
                        "cancel" => "Cancels",
                        "list" => "Lists",
                        _ => "Queries",
                    }
                ),
            ))
        }

        Intent::SynapseChat { model, prompt } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "model", model);
            insert_opt(&mut a, "prompt", prompt);
            Ok(mcp_call(
                "synapse_chat",
                a,
                format!("Synapse chat: {}", model),
                PermissionLevel::SystemWrite,
                "Runs inference via Synapse MCP bridge".to_string(),
            ))
        }

        Intent::SynapseStatus => Ok(mcp_call(
            "synapse_status",
            serde_json::Map::new(),
            "Synapse status".to_string(),
            PermissionLevel::Safe,
            "Checks Synapse health and GPU status via MCP bridge".to_string(),
        )),

        Intent::SynapseBenchmark { action, models } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "action", action);
            insert_opt(&mut a, "models", models);
            Ok(mcp_call(
                "synapse_benchmark",
                a,
                format!(
                    "Synapse benchmark: {}{}",
                    action,
                    models
                        .as_ref()
                        .map_or(String::new(), |m| format!(" '{}'", m))
                ),
                match action.as_str() {
                    "list" | "status" => PermissionLevel::Safe,
                    _ => PermissionLevel::SystemWrite,
                },
                "Benchmarks/compares models via Synapse".to_string(),
            ))
        }

        Intent::SynapseQuantize {
            action,
            model,
            format,
        } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "action", action);
            insert_opt(&mut a, "model", model);
            insert_opt(&mut a, "format", format);
            Ok(mcp_call(
                "synapse_quantize",
                a,
                format!(
                    "Synapse quantize: {}{}",
                    action,
                    model
                        .as_ref()
                        .map_or(String::new(), |m| format!(" '{}'", m))
                ),
                match action.as_str() {
                    "status" | "list" => PermissionLevel::Safe,
                    _ => PermissionLevel::SystemWrite,
                },
                "Quantizes/converts model via Synapse".to_string(),
            ))
        }

        _ => unreachable!("translate_synapse called with non-synapse intent"),
    }
}
