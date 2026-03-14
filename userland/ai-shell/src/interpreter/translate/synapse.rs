use anyhow::Result;

use crate::interpreter::intent::{Intent, Translation};
use crate::security::PermissionLevel;

pub(crate) fn translate_synapse(intent: &Intent) -> Result<Translation> {
    match intent {
        Intent::SynapseModels {
            action,
            name,
            source,
        } => {
            let mut args_json = serde_json::Map::new();
            args_json.insert(
                "action".to_string(),
                serde_json::Value::String(action.clone()),
            );
            if let Some(n) = name {
                args_json.insert("name".to_string(), serde_json::Value::String(n.clone()));
            }
            if let Some(s) = source {
                args_json.insert("source".to_string(), serde_json::Value::String(s.clone()));
            }
            let body = serde_json::json!({"name": "synapse_models", "arguments": args_json});
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
                    "Synapse models: {}{}",
                    action,
                    name.as_ref().map_or(String::new(), |n| format!(" '{}'", n))
                ),
                permission: match action.as_str() {
                    "list" | "info" => PermissionLevel::Safe,
                    _ => PermissionLevel::SystemWrite,
                },
                explanation: format!(
                    "{} model(s) via Synapse MCP bridge",
                    match action.as_str() {
                        "download" => "Downloads",
                        "delete" => "Deletes",
                        "list" => "Lists",
                        _ => "Queries",
                    }
                ),
            })
        }

        Intent::SynapseServe { action, model } => {
            let mut args_json = serde_json::Map::new();
            args_json.insert(
                "action".to_string(),
                serde_json::Value::String(action.clone()),
            );
            if let Some(m) = model {
                args_json.insert("model".to_string(), serde_json::Value::String(m.clone()));
            }
            let body = serde_json::json!({"name": "synapse_serve", "arguments": args_json});
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
                    "Synapse serve: {}{}",
                    action,
                    model
                        .as_ref()
                        .map_or(String::new(), |m| format!(" '{}'", m))
                ),
                permission: match action.as_str() {
                    "status" | "list" => PermissionLevel::Safe,
                    _ => PermissionLevel::SystemWrite,
                },
                explanation: format!(
                    "{} model serving via Synapse MCP bridge",
                    match action.as_str() {
                        "start" => "Starts",
                        "stop" => "Stops",
                        _ => "Queries",
                    }
                ),
            })
        }

        Intent::SynapseFinetune {
            action,
            model,
            method,
        } => {
            let mut args_json = serde_json::Map::new();
            args_json.insert(
                "action".to_string(),
                serde_json::Value::String(action.clone()),
            );
            if let Some(m) = model {
                args_json.insert("model".to_string(), serde_json::Value::String(m.clone()));
            }
            if let Some(mt) = method {
                args_json.insert("method".to_string(), serde_json::Value::String(mt.clone()));
            }
            let body = serde_json::json!({"name": "synapse_finetune", "arguments": args_json});
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
                    "Synapse finetune: {}{}",
                    action,
                    model
                        .as_ref()
                        .map_or(String::new(), |m| format!(" '{}'", m))
                ),
                permission: match action.as_str() {
                    "status" | "list" => PermissionLevel::Safe,
                    _ => PermissionLevel::SystemWrite,
                },
                explanation: format!(
                    "{} fine-tuning job via Synapse MCP bridge",
                    match action.as_str() {
                        "start" => "Starts",
                        "cancel" => "Cancels",
                        "list" => "Lists",
                        _ => "Queries",
                    }
                ),
            })
        }

        Intent::SynapseChat { model, prompt } => {
            let mut args_json = serde_json::Map::new();
            args_json.insert(
                "model".to_string(),
                serde_json::Value::String(model.clone()),
            );
            if let Some(p) = prompt {
                args_json.insert("prompt".to_string(), serde_json::Value::String(p.clone()));
            }
            let body = serde_json::json!({"name": "synapse_chat", "arguments": args_json});
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
                description: format!("Synapse chat: {}", model),
                permission: PermissionLevel::SystemWrite,
                explanation: "Runs inference via Synapse MCP bridge".to_string(),
            })
        }

        Intent::SynapseStatus => {
            let args_json = serde_json::Map::new();
            let body = serde_json::json!({"name": "synapse_status", "arguments": args_json});
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
                description: "Synapse status".to_string(),
                permission: PermissionLevel::Safe,
                explanation: "Checks Synapse health and GPU status via MCP bridge".to_string(),
            })
        }

        Intent::SynapseBenchmark { action, models } => {
            let mut args_json = serde_json::Map::new();
            args_json.insert(
                "action".to_string(),
                serde_json::Value::String(action.clone()),
            );
            if let Some(m) = models {
                args_json.insert("models".to_string(), serde_json::Value::String(m.clone()));
            }
            let body = serde_json::json!({"name": "synapse_benchmark", "arguments": args_json});
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
                    "Synapse benchmark: {}{}",
                    action,
                    models
                        .as_ref()
                        .map_or(String::new(), |m| format!(" '{}'", m))
                ),
                permission: match action.as_str() {
                    "list" | "status" => PermissionLevel::Safe,
                    _ => PermissionLevel::SystemWrite,
                },
                explanation: "Benchmarks/compares models via Synapse".to_string(),
            })
        }

        Intent::SynapseQuantize {
            action,
            model,
            format,
        } => {
            let mut args_json = serde_json::Map::new();
            args_json.insert(
                "action".to_string(),
                serde_json::Value::String(action.clone()),
            );
            if let Some(m) = model {
                args_json.insert("model".to_string(), serde_json::Value::String(m.clone()));
            }
            if let Some(f) = format {
                args_json.insert("format".to_string(), serde_json::Value::String(f.clone()));
            }
            let body = serde_json::json!({"name": "synapse_quantize", "arguments": args_json});
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
                    "Synapse quantize: {}{}",
                    action,
                    model
                        .as_ref()
                        .map_or(String::new(), |m| format!(" '{}'", m))
                ),
                permission: match action.as_str() {
                    "status" | "list" => PermissionLevel::Safe,
                    _ => PermissionLevel::SystemWrite,
                },
                explanation: "Quantizes/converts model via Synapse".to_string(),
            })
        }

        _ => unreachable!("translate_synapse called with non-synapse intent"),
    }
}
