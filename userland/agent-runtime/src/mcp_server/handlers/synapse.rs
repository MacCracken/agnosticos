use tracing::{info, warn};
use uuid::Uuid;

use super::super::helpers::{
    extract_required_string, get_optional_string_arg, success_result, validate_enum_opt,
};
use super::super::types::McpToolResult;
use super::bridge::HttpBridge;

// ---------------------------------------------------------------------------
// Synapse LLM Management Bridge
// ---------------------------------------------------------------------------

pub(crate) fn synapse_bridge() -> HttpBridge {
    HttpBridge::new(
        "SYNAPSE_URL",
        "http://127.0.0.1:8080",
        "SYNAPSE_API_KEY",
        "Synapse",
    )
}

// ---------------------------------------------------------------------------
// Synapse Tool Implementations (bridged)
// ---------------------------------------------------------------------------

pub(crate) async fn handle_synapse_models(args: &serde_json::Value) -> McpToolResult {
    let action = match extract_required_string(args, "action") {
        Ok(a) => a,
        Err(e) => return e,
    };

    let action_opt = Some(action.clone());
    if let Err(e) = validate_enum_opt(
        &action_opt,
        "action",
        &["download", "delete", "list", "info"],
    ) {
        return e;
    }

    let name = get_optional_string_arg(args, "name");
    let source = get_optional_string_arg(args, "source");

    if let Err(e) = validate_enum_opt(&source, "source", &["huggingface", "ollama", "local"]) {
        return e;
    }

    let bridge = synapse_bridge();

    match action.as_str() {
        "list" | "info" => {
            let mut query = Vec::new();
            if let Some(ref n) = name {
                query.push(("name".to_string(), n.clone()));
            }
            if let Some(ref s) = source {
                query.push(("source".to_string(), s.clone()));
            }
            match bridge.get("/api/v1/models", &query).await {
                Ok(response) => {
                    info!("Synapse: {} models (bridged)", action);
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "Synapse bridge: falling back to mock for models {}", action);
                    success_result(serde_json::json!({
                        "models": [
                            {"id": "llama-3.1-8b", "size_gb": 4.7, "status": "ready"},
                        ],
                        "total": 1,
                        "_source": "mock",
                    }))
                }
            }
        }
        op @ ("download" | "delete") => {
            let body = serde_json::json!({
                "action": op,
                "name": name,
                "source": source,
            });
            match bridge.post("/api/v1/models", body).await {
                Ok(response) => {
                    info!(action = %op, "Synapse: {} model (bridged)", op);
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "Synapse bridge: falling back to mock for {} model", op);
                    let job_id = Uuid::new_v4().to_string();
                    success_result(serde_json::json!({
                        "id": job_id,
                        "action": op,
                        "name": name.unwrap_or_else(|| "unknown".to_string()),
                        "source": source.unwrap_or_else(|| "huggingface".to_string()),
                        "status": "ok",
                        "_source": "mock",
                    }))
                }
            }
        }
        _ => unreachable!(),
    }
}

pub(crate) async fn handle_synapse_serve(args: &serde_json::Value) -> McpToolResult {
    let action = match extract_required_string(args, "action") {
        Ok(a) => a,
        Err(e) => return e,
    };

    let action_opt = Some(action.clone());
    if let Err(e) = validate_enum_opt(&action_opt, "action", &["start", "stop", "status", "list"]) {
        return e;
    }

    let model = get_optional_string_arg(args, "model");
    let port = get_optional_string_arg(args, "port");
    let bridge = synapse_bridge();

    match action.as_str() {
        "status" | "list" => {
            let mut query = Vec::new();
            if let Some(ref m) = model {
                query.push(("model".to_string(), m.clone()));
            }
            match bridge.get("/api/v1/serve", &query).await {
                Ok(response) => {
                    info!("Synapse: {} serve (bridged)", action);
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "Synapse bridge: falling back to mock for serve {}", action);
                    success_result(serde_json::json!({
                        "serving": [],
                        "total": 0,
                        "_source": "mock",
                    }))
                }
            }
        }
        op @ ("start" | "stop") => {
            let body = serde_json::json!({
                "action": op,
                "model": model,
                "port": port,
            });
            match bridge.post("/api/v1/serve", body).await {
                Ok(response) => {
                    info!(action = %op, "Synapse: {} serve (bridged)", op);
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "Synapse bridge: falling back to mock for serve {}", op);
                    let instance_id = Uuid::new_v4().to_string();
                    success_result(serde_json::json!({
                        "id": instance_id,
                        "action": op,
                        "model": model.unwrap_or_else(|| "unknown".to_string()),
                        "port": port,
                        "status": "ok",
                        "_source": "mock",
                    }))
                }
            }
        }
        _ => unreachable!(),
    }
}

pub(crate) async fn handle_synapse_finetune(args: &serde_json::Value) -> McpToolResult {
    let action = match extract_required_string(args, "action") {
        Ok(a) => a,
        Err(e) => return e,
    };

    let action_opt = Some(action.clone());
    if let Err(e) = validate_enum_opt(
        &action_opt,
        "action",
        &["start", "status", "cancel", "list"],
    ) {
        return e;
    }

    let model = get_optional_string_arg(args, "model");
    let dataset = get_optional_string_arg(args, "dataset");
    let method = get_optional_string_arg(args, "method");

    if let Err(e) = validate_enum_opt(&method, "method", &["lora", "qlora", "full", "dpo", "rlhf"])
    {
        return e;
    }

    let bridge = synapse_bridge();

    match action.as_str() {
        "status" | "list" => {
            let mut query = Vec::new();
            if let Some(ref m) = model {
                query.push(("model".to_string(), m.clone()));
            }
            match bridge.get("/api/v1/finetune", &query).await {
                Ok(response) => {
                    info!("Synapse: {} finetune (bridged)", action);
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "Synapse bridge: falling back to mock for finetune {}", action);
                    success_result(serde_json::json!({
                        "jobs": [],
                        "total": 0,
                        "_source": "mock",
                    }))
                }
            }
        }
        op @ ("start" | "cancel") => {
            let body = serde_json::json!({
                "action": op,
                "model": model,
                "dataset": dataset,
                "method": method,
            });
            match bridge.post("/api/v1/finetune", body).await {
                Ok(response) => {
                    info!(action = %op, "Synapse: {} finetune (bridged)", op);
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "Synapse bridge: falling back to mock for finetune {}", op);
                    let job_id = Uuid::new_v4().to_string();
                    success_result(serde_json::json!({
                        "id": job_id,
                        "action": op,
                        "model": model.unwrap_or_else(|| "unknown".to_string()),
                        "dataset": dataset,
                        "method": method.unwrap_or_else(|| "lora".to_string()),
                        "status": "ok",
                        "_source": "mock",
                    }))
                }
            }
        }
        _ => unreachable!(),
    }
}

pub(crate) async fn handle_synapse_chat(args: &serde_json::Value) -> McpToolResult {
    let model = match extract_required_string(args, "model") {
        Ok(m) => m,
        Err(e) => return e,
    };

    let prompt = get_optional_string_arg(args, "prompt");
    let temperature = args.get("temperature").and_then(|v| v.as_f64());
    let max_tokens = get_optional_string_arg(args, "max_tokens");

    let bridge = synapse_bridge();
    let body = serde_json::json!({
        "model": model,
        "prompt": prompt,
        "temperature": temperature,
        "max_tokens": max_tokens,
    });

    match bridge.post("/api/v1/chat", body).await {
        Ok(response) => {
            info!(model = %model, "Synapse: chat (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Synapse bridge: falling back to mock for chat");
            success_result(serde_json::json!({
                "response": "Synapse not reachable",
                "model": model,
                "tokens_used": 0,
                "_source": "mock",
            }))
        }
    }
}

pub(crate) async fn handle_synapse_status(args: &serde_json::Value) -> McpToolResult {
    let detail = get_optional_string_arg(args, "detail");

    if let Err(e) = validate_enum_opt(&detail, "detail", &["brief", "full"]) {
        return e;
    }

    let bridge = synapse_bridge();

    let mut query = Vec::new();
    if let Some(ref d) = detail {
        query.push(("detail".to_string(), d.clone()));
    }

    match bridge.get("/api/v1/status", &query).await {
        Ok(response) => {
            info!("Synapse: status (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Synapse bridge: falling back to mock for status");
            success_result(serde_json::json!({
                "healthy": false,
                "gpu_count": 0,
                "models_loaded": 0,
                "message": "Synapse not reachable",
                "_source": "mock",
            }))
        }
    }
}

pub(crate) async fn handle_synapse_benchmark(args: &serde_json::Value) -> McpToolResult {
    let action = match extract_required_string(args, "action") {
        Ok(a) => a,
        Err(e) => return e,
    };

    let action_opt = Some(action.clone());
    if let Err(e) = validate_enum_opt(&action_opt, "action", &["run", "compare", "list", "status"])
    {
        return e;
    }

    let models = get_optional_string_arg(args, "models");
    let dataset = get_optional_string_arg(args, "dataset");
    let metric = get_optional_string_arg(args, "metric");

    if let Err(e) = validate_enum_opt(
        &metric,
        "metric",
        &["latency", "throughput", "accuracy", "perplexity"],
    ) {
        return e;
    }

    let bridge = synapse_bridge();

    match action.as_str() {
        "list" | "status" => {
            let mut query = Vec::new();
            if let Some(ref m) = models {
                query.push(("models".to_string(), m.clone()));
            }
            if let Some(ref d) = dataset {
                query.push(("dataset".to_string(), d.clone()));
            }
            if let Some(ref mt) = metric {
                query.push(("metric".to_string(), mt.clone()));
            }
            match bridge.get("/api/v1/benchmark", &query).await {
                Ok(response) => {
                    info!("Synapse: {} benchmark (bridged)", action);
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "Synapse bridge: falling back to mock for benchmark {}", action);
                    success_result(serde_json::json!({
                        "benchmarks": [],
                        "total": 0,
                        "_source": "mock",
                    }))
                }
            }
        }
        op @ ("run" | "compare") => {
            let body = serde_json::json!({
                "action": op,
                "models": models,
                "dataset": dataset,
                "metric": metric,
            });
            match bridge.post("/api/v1/benchmark", body).await {
                Ok(response) => {
                    info!(action = %op, "Synapse: {} benchmark (bridged)", op);
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "Synapse bridge: falling back to mock for benchmark {}", op);
                    let job_id = Uuid::new_v4().to_string();
                    success_result(serde_json::json!({
                        "id": job_id,
                        "action": op,
                        "models": models,
                        "dataset": dataset,
                        "metric": metric.unwrap_or_else(|| "latency".to_string()),
                        "status": "ok",
                        "_source": "mock",
                    }))
                }
            }
        }
        _ => unreachable!(),
    }
}

pub(crate) async fn handle_synapse_quantize(args: &serde_json::Value) -> McpToolResult {
    let action = match extract_required_string(args, "action") {
        Ok(a) => a,
        Err(e) => return e,
    };

    let action_opt = Some(action.clone());
    if let Err(e) = validate_enum_opt(
        &action_opt,
        "action",
        &["start", "status", "list", "cancel"],
    ) {
        return e;
    }

    let model = get_optional_string_arg(args, "model");
    let format = get_optional_string_arg(args, "format");
    let bits = get_optional_string_arg(args, "bits");

    if let Err(e) = validate_enum_opt(&format, "format", &["gguf", "gptq", "awq", "bnb"]) {
        return e;
    }
    if let Err(e) = validate_enum_opt(&bits, "bits", &["4", "8"]) {
        return e;
    }

    let bridge = synapse_bridge();

    match action.as_str() {
        "status" | "list" => {
            let mut query = Vec::new();
            if let Some(ref m) = model {
                query.push(("model".to_string(), m.clone()));
            }
            match bridge.get("/api/v1/quantize", &query).await {
                Ok(response) => {
                    info!("Synapse: {} quantize (bridged)", action);
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "Synapse bridge: falling back to mock for quantize {}", action);
                    success_result(serde_json::json!({
                        "jobs": [],
                        "total": 0,
                        "_source": "mock",
                    }))
                }
            }
        }
        op @ ("start" | "cancel") => {
            let body = serde_json::json!({
                "action": op,
                "model": model,
                "format": format,
                "bits": bits,
            });
            match bridge.post("/api/v1/quantize", body).await {
                Ok(response) => {
                    info!(action = %op, "Synapse: {} quantize (bridged)", op);
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "Synapse bridge: falling back to mock for quantize {}", op);
                    let job_id = Uuid::new_v4().to_string();
                    success_result(serde_json::json!({
                        "id": job_id,
                        "action": op,
                        "model": model.unwrap_or_else(|| "unknown".to_string()),
                        "format": format.unwrap_or_else(|| "gguf".to_string()),
                        "bits": bits.unwrap_or_else(|| "4".to_string()),
                        "status": "ok",
                        "_source": "mock",
                    }))
                }
            }
        }
        _ => unreachable!(),
    }
}
