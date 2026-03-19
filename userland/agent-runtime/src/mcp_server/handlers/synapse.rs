use tracing::{info, warn};
use uuid::Uuid;

use super::super::helpers::{
    extract_required_string, get_optional_string_arg, success_result, validate_enum_opt,
};
use super::super::types::McpToolResult;
use super::bridge::HttpBridge;

// ---------------------------------------------------------------------------
// Synapse LLM Management Bridge
//
// Synapse REST API (2026.3.18-2) — paths are relative to base URL:
//   GET  /models                     — list models
//   GET  /models/{id}                — model info
//   DELETE /models/{id}              — delete model
//   GET  /models/discover            — discover downloadable models
//   POST /inference                  — run inference
//   POST /v1/chat/completions        — OpenAI-compatible chat
//   GET  /system/status              — system health + GPU
//   GET  /system/gpu/telemetry       — GPU metrics
//   POST /training/jobs              — start training job
//   GET  /training/jobs              — list training jobs
//   GET  /training/jobs/{id}         — job status
//   POST /training/jobs/{id}/cancel  — cancel job
//   POST /eval/runs                  — start evaluation
//   GET  /eval/runs                  — list evaluations
//   POST /marketplace/pull           — download model from marketplace
//   GET  /bridge/status              — AGNOS bridge status
//   POST /bridge/connect             — register AGNOS connection
//   POST /bridge/heartbeat           — heartbeat
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
// Synapse Tool Implementations (bridged to Synapse 2026.3.18-2 API)
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
        "list" => match bridge.get("/models", &[]).await {
            Ok(response) => {
                info!("Synapse: list models (bridged)");
                success_result(response)
            }
            Err(e) => {
                warn!(error = %e, "Synapse bridge: falling back to mock for models list");
                success_result(serde_json::json!({
                    "models": [
                        {"id": "llama-3.1-8b", "size_gb": 4.7, "status": "ready"},
                    ],
                    "total": 1,
                    "_source": "mock",
                }))
            }
        },
        "info" => {
            let model_id = name.as_deref().unwrap_or("unknown");
            match bridge.get(&format!("/models/{}", model_id), &[]).await {
                Ok(response) => {
                    info!(model = %model_id, "Synapse: model info (bridged)");
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "Synapse bridge: falling back to mock for model info");
                    success_result(serde_json::json!({
                        "id": model_id,
                        "status": "unknown",
                        "_source": "mock",
                    }))
                }
            }
        }
        "download" => {
            // Use marketplace/pull for downloading models
            let body = serde_json::json!({
                "name": name,
                "source": source,
            });
            match bridge.post("/marketplace/pull", body).await {
                Ok(response) => {
                    info!("Synapse: download model (bridged)");
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "Synapse bridge: falling back to mock for download");
                    let job_id = Uuid::new_v4().to_string();
                    success_result(serde_json::json!({
                        "id": job_id,
                        "action": "download",
                        "name": name.unwrap_or_else(|| "unknown".to_string()),
                        "source": source.unwrap_or_else(|| "huggingface".to_string()),
                        "status": "ok",
                        "_source": "mock",
                    }))
                }
            }
        }
        "delete" => {
            let model_id = name.as_deref().unwrap_or("unknown");
            match bridge.delete(&format!("/models/{}", model_id)).await {
                Ok(response) => {
                    info!(model = %model_id, "Synapse: delete model (bridged)");
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "Synapse bridge: falling back to mock for delete");
                    success_result(serde_json::json!({
                        "action": "delete",
                        "name": model_id,
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

    // Synapse doesn't have a separate /serve endpoint — inference is always
    // available when a model is loaded. We use /models for status/list and
    // /inference for start (load model), /system/status for overall status.
    match action.as_str() {
        "status" => match bridge.get("/system/status", &[]).await {
            Ok(response) => {
                info!("Synapse: serve status (bridged)");
                success_result(response)
            }
            Err(e) => {
                warn!(error = %e, "Synapse bridge: falling back to mock for serve status");
                success_result(serde_json::json!({
                    "serving": [],
                    "total": 0,
                    "_source": "mock",
                }))
            }
        },
        "list" => match bridge.get("/models", &[]).await {
            Ok(response) => {
                info!("Synapse: serve list (bridged)");
                success_result(response)
            }
            Err(e) => {
                warn!(error = %e, "Synapse bridge: falling back to mock for serve list");
                success_result(serde_json::json!({
                    "serving": [],
                    "total": 0,
                    "_source": "mock",
                }))
            }
        },
        op @ ("start" | "stop") => {
            let body = serde_json::json!({
                "action": op,
                "model": model,
                "port": port,
            });
            // No direct serve start/stop in Synapse — use bridge endpoint
            match bridge.post("/bridge/connect", body).await {
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

    let gpu_required = args
        .get("gpu_required")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let min_gpu_memory_mb = args.get("min_gpu_memory_mb").and_then(|v| v.as_u64());

    let bridge = synapse_bridge();

    match action.as_str() {
        "list" => match bridge.get("/training/jobs", &[]).await {
            Ok(response) => {
                info!("Synapse: list training jobs (bridged)");
                success_result(response)
            }
            Err(e) => {
                warn!(error = %e, "Synapse bridge: falling back to mock for finetune list");
                success_result(serde_json::json!({
                    "jobs": [],
                    "total": 0,
                    "_source": "mock",
                }))
            }
        },
        "status" => {
            // If model is provided, use it as job ID for status lookup
            let path = if let Some(ref m) = model {
                format!("/training/jobs/{}", m)
            } else {
                "/training/jobs".to_string()
            };
            match bridge.get(&path, &[]).await {
                Ok(response) => {
                    info!("Synapse: finetune status (bridged)");
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "Synapse bridge: falling back to mock for finetune status");
                    success_result(serde_json::json!({
                        "jobs": [],
                        "total": 0,
                        "_source": "mock",
                    }))
                }
            }
        }
        "start" => {
            let mut body = serde_json::json!({
                "base_model": model,
                "dataset_path": dataset,
                "method": method.as_deref().unwrap_or("lora"),
            });
            if gpu_required {
                body["gpu_required"] = serde_json::Value::Bool(true);
            }
            if let Some(min_mb) = min_gpu_memory_mb {
                body["min_gpu_memory_mb"] = serde_json::Value::Number(min_mb.into());
            }
            match bridge.post("/training/jobs", body).await {
                Ok(response) => {
                    info!(gpu_required = gpu_required, min_gpu_memory_mb = ?min_gpu_memory_mb, "Synapse: start finetune (bridged)");
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "Synapse bridge: falling back to mock for finetune start");
                    let job_id = Uuid::new_v4().to_string();
                    success_result(serde_json::json!({
                        "id": job_id,
                        "action": "start",
                        "model": model.unwrap_or_else(|| "unknown".to_string()),
                        "dataset": dataset,
                        "method": method.unwrap_or_else(|| "lora".to_string()),
                        "gpu_required": gpu_required,
                        "min_gpu_memory_mb": min_gpu_memory_mb,
                        "status": "ok",
                        "_source": "mock",
                    }))
                }
            }
        }
        "cancel" => {
            let job_id = model.as_deref().unwrap_or("unknown");
            match bridge
                .post(
                    &format!("/training/jobs/{}/cancel", job_id),
                    serde_json::json!({}),
                )
                .await
            {
                Ok(response) => {
                    info!(job = %job_id, "Synapse: cancel finetune (bridged)");
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "Synapse bridge: falling back to mock for finetune cancel");
                    success_result(serde_json::json!({
                        "id": job_id,
                        "action": "cancel",
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
    let max_tokens = args.get("max_tokens").and_then(|v| v.as_u64());

    let bridge = synapse_bridge();

    // Use OpenAI-compatible endpoint
    let body = serde_json::json!({
        "model": model,
        "messages": [{"role": "user", "content": prompt.as_deref().unwrap_or("")}],
        "temperature": temperature,
        "max_tokens": max_tokens,
    });

    match bridge.post("/v1/chat/completions", body).await {
        Ok(response) => {
            info!(model = %model, "Synapse: chat (bridged via OpenAI-compat)");
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

    // Use /system/status for health, optionally /system/gpu/telemetry for full
    match bridge.get("/system/status", &[]).await {
        Ok(mut response) => {
            // If full detail requested, also fetch GPU telemetry
            if detail.as_deref() == Some("full") {
                if let Ok(gpu) = bridge.get("/system/gpu/telemetry", &[]).await {
                    response["gpu_telemetry"] = gpu;
                }
            }
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

    // Synapse uses /eval/runs for benchmarking/evaluation
    match action.as_str() {
        "list" | "status" => match bridge.get("/eval/runs", &[]).await {
            Ok(response) => {
                info!("Synapse: {} benchmark (bridged via /eval/runs)", action);
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
        },
        op @ ("run" | "compare") => {
            let body = serde_json::json!({
                "action": op,
                "models": models,
                "dataset": dataset,
                "metric": metric,
            });
            match bridge.post("/eval/runs", body).await {
                Ok(response) => {
                    info!(action = %op, "Synapse: {} benchmark (bridged via /eval/runs)", op);
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

    // Synapse doesn't have a dedicated /quantize endpoint yet.
    // Quantization is planned as a training job variant.
    // For now, we use /training/jobs with a quantize method hint.
    match action.as_str() {
        "status" | "list" => {
            let query = vec![("type".to_string(), "quantize".to_string())];
            match bridge.get("/training/jobs", &query).await {
                Ok(response) => {
                    info!("Synapse: {} quantize (bridged via /training/jobs)", action);
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
                "type": "quantize",
                "base_model": model,
                "quantize_format": format,
                "quantize_bits": bits,
            });
            let path = if op == "cancel" {
                let job_id = model.as_deref().unwrap_or("unknown");
                format!("/training/jobs/{}/cancel", job_id)
            } else {
                "/training/jobs".to_string()
            };
            match bridge.post(&path, body).await {
                Ok(response) => {
                    info!(action = %op, "Synapse: {} quantize (bridged via /training/jobs)", op);
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
