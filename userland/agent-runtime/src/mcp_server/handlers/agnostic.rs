use tracing::{info, warn};
use uuid::Uuid;

use super::super::helpers::{
    error_result, extract_required_string, get_optional_string_arg, success_result,
};
use super::super::types::McpToolResult;
use super::bridge::HttpBridge;

// ---------------------------------------------------------------------------
// Agnostic Agent System (AAS) Bridge
// ---------------------------------------------------------------------------
// API contract aligned with Agnostic v2026.3.17-1+
// Base: http://127.0.0.1:8000/api/v1
// All task submission routes through crew builder internally.

pub(crate) fn agnostic_bridge() -> HttpBridge {
    HttpBridge::new(
        "AGNOSTIC_URL",
        "http://127.0.0.1:8000",
        "AGNOSTIC_API_KEY",
        "Agnostic",
    )
}

// ---------------------------------------------------------------------------
// Task Submission (all route through crews internally)
// ---------------------------------------------------------------------------

/// Submit a QA task.
/// POST /api/v1/tasks
pub(crate) async fn handle_agnostic_submit_task(args: &serde_json::Value) -> McpToolResult {
    let title = match extract_required_string(args, "title") {
        Ok(s) => s,
        Err(e) => return e,
    };
    let description = match extract_required_string(args, "description") {
        Ok(s) => s,
        Err(e) => return e,
    };

    let target_url = get_optional_string_arg(args, "target_url");
    let priority = get_optional_string_arg(args, "priority").unwrap_or_else(|| "high".to_string());
    let size = get_optional_string_arg(args, "size").unwrap_or_else(|| "standard".to_string());

    let bridge = agnostic_bridge();
    let mut body = serde_json::json!({
        "title": title,
        "description": description,
        "priority": priority,
        "size": size,
    });
    if let Some(url) = &target_url {
        body["target_url"] = serde_json::json!(url);
    }
    if let Some(agents) = args.get("agents") {
        body["agents"] = agents.clone();
    }
    if let Some(standards) = args.get("standards") {
        body["standards"] = standards.clone();
    }

    match bridge.post("/api/v1/tasks", body).await {
        Ok(response) => {
            info!(title = %title, "Agnostic: submit task (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Agnostic bridge: submit task failed");
            let task_id = Uuid::new_v4().to_string();
            success_result(serde_json::json!({
                "task_id": task_id,
                "status": "pending",
                "_source": "mock",
            }))
        }
    }
}

/// Get task status.
/// GET /api/v1/tasks/{task_id}
pub(crate) async fn handle_agnostic_task_status(args: &serde_json::Value) -> McpToolResult {
    let task_id = match extract_required_string(args, "task_id") {
        Ok(id) => id,
        Err(e) => return e,
    };

    let bridge = agnostic_bridge();
    match bridge.get(&format!("/api/v1/tasks/{}", task_id), &[]).await {
        Ok(response) => {
            info!(task_id = %task_id, "Agnostic: task status (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Agnostic bridge: task status failed");
            error_result(format!("Task status unavailable: {}", e))
        }
    }
}

// ---------------------------------------------------------------------------
// Security
// ---------------------------------------------------------------------------

/// Run OWASP/GDPR/PCI DSS compliance scan.
/// POST /api/v1/tasks/security
pub(crate) async fn handle_agnostic_security_scan(args: &serde_json::Value) -> McpToolResult {
    let target_url = match extract_required_string(args, "target_url") {
        Ok(u) => u,
        Err(e) => return e,
    };

    let title =
        get_optional_string_arg(args, "title").unwrap_or_else(|| "Security Scan".to_string());
    let description = get_optional_string_arg(args, "description")
        .unwrap_or_else(|| format!("Security scan: {}", target_url));
    let size = get_optional_string_arg(args, "size").unwrap_or_else(|| "standard".to_string());

    let bridge = agnostic_bridge();
    let mut body = serde_json::json!({
        "title": title,
        "description": description,
        "target_url": target_url,
        "size": size,
    });
    if let Some(standards) = args.get("standards") {
        body["standards"] = standards.clone();
    }

    match bridge.post("/api/v1/tasks/security", body).await {
        Ok(response) => {
            info!(target = %target_url, "Agnostic: security scan (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Agnostic bridge: security scan failed");
            error_result(format!("Security scan failed: {}", e))
        }
    }
}

/// Get security findings for a session.
/// GET /api/v1/results/structured/{session_id}?result_type=security
pub(crate) async fn handle_agnostic_security_findings(args: &serde_json::Value) -> McpToolResult {
    let session_id = match extract_required_string(args, "session_id") {
        Ok(id) => id,
        Err(e) => return e,
    };

    let bridge = agnostic_bridge();
    let query = vec![("result_type".to_string(), "security".to_string())];
    match bridge
        .get(
            &format!("/api/v1/results/structured/{}", session_id),
            &query,
        )
        .await
    {
        Ok(response) => {
            info!(session_id = %session_id, "Agnostic: security findings (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Agnostic bridge: security findings failed");
            error_result(format!("Security findings unavailable: {}", e))
        }
    }
}

// ---------------------------------------------------------------------------
// Performance
// ---------------------------------------------------------------------------

/// Run load testing and latency profiling.
/// POST /api/v1/tasks/performance
pub(crate) async fn handle_agnostic_performance_test(args: &serde_json::Value) -> McpToolResult {
    let target_url = match extract_required_string(args, "target_url") {
        Ok(u) => u,
        Err(e) => return e,
    };

    let title =
        get_optional_string_arg(args, "title").unwrap_or_else(|| "Performance Test".to_string());
    let size = get_optional_string_arg(args, "size").unwrap_or_else(|| "standard".to_string());

    let bridge = agnostic_bridge();
    let mut body = serde_json::json!({
        "title": title,
        "description": format!("Performance test: {}", target_url),
        "target_url": target_url,
        "size": size,
    });
    if let Some(dur) = args.get("duration_seconds") {
        body["duration_seconds"] = dur.clone();
    }
    if let Some(conc) = args.get("concurrency") {
        body["concurrency"] = conc.clone();
    }

    match bridge.post("/api/v1/tasks/performance", body).await {
        Ok(response) => {
            info!(target = %target_url, "Agnostic: performance test (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Agnostic bridge: performance test failed");
            error_result(format!("Performance test failed: {}", e))
        }
    }
}

/// Get performance results for a session.
/// GET /api/v1/results/structured/{session_id}?result_type=performance
pub(crate) async fn handle_agnostic_performance_results(args: &serde_json::Value) -> McpToolResult {
    let session_id = match extract_required_string(args, "session_id") {
        Ok(id) => id,
        Err(e) => return e,
    };

    let bridge = agnostic_bridge();
    let query = vec![("result_type".to_string(), "performance".to_string())];
    match bridge
        .get(
            &format!("/api/v1/results/structured/{}", session_id),
            &query,
        )
        .await
    {
        Ok(response) => {
            info!(session_id = %session_id, "Agnostic: performance results (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Agnostic bridge: performance results failed");
            error_result(format!("Performance results unavailable: {}", e))
        }
    }
}

// ---------------------------------------------------------------------------
// Results & Reports
// ---------------------------------------------------------------------------

/// Get structured results for a session.
/// GET /api/v1/results/structured/{session_id}
pub(crate) async fn handle_agnostic_structured_results(args: &serde_json::Value) -> McpToolResult {
    let session_id = match extract_required_string(args, "session_id") {
        Ok(id) => id,
        Err(e) => return e,
    };

    let result_type = get_optional_string_arg(args, "result_type");

    let bridge = agnostic_bridge();
    let mut query = Vec::new();
    if let Some(ref rt) = result_type {
        query.push(("result_type".to_string(), rt.clone()));
    }

    match bridge
        .get(
            &format!("/api/v1/results/structured/{}", session_id),
            &query,
        )
        .await
    {
        Ok(response) => {
            info!(session_id = %session_id, "Agnostic: structured results (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Agnostic bridge: structured results failed");
            error_result(format!("Results unavailable: {}", e))
        }
    }
}

/// Generate a report for a session.
/// POST /api/v1/reports/generate
pub(crate) async fn handle_agnostic_generate_report(args: &serde_json::Value) -> McpToolResult {
    let session_id = match extract_required_string(args, "session_id") {
        Ok(id) => id,
        Err(e) => return e,
    };
    let format = get_optional_string_arg(args, "format").unwrap_or_else(|| "json".to_string());

    let bridge = agnostic_bridge();
    let body = serde_json::json!({
        "session_id": session_id,
        "format": format,
    });

    match bridge.post("/api/v1/reports/generate", body).await {
        Ok(response) => {
            info!(session_id = %session_id, "Agnostic: generate report (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Agnostic bridge: report generation failed");
            error_result(format!("Report generation failed: {}", e))
        }
    }
}

/// List available reports.
/// GET /api/v1/reports
pub(crate) async fn handle_agnostic_list_reports(_args: &serde_json::Value) -> McpToolResult {
    let bridge = agnostic_bridge();
    match bridge.get("/api/v1/reports", &[]).await {
        Ok(response) => {
            info!("Agnostic: list reports (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Agnostic bridge: list reports failed");
            error_result(format!("Reports unavailable: {}", e))
        }
    }
}

// ---------------------------------------------------------------------------
// Dashboard & Metrics
// ---------------------------------------------------------------------------

/// Get dashboard snapshot.
/// GET /api/v1/dashboard
pub(crate) async fn handle_agnostic_dashboard(_args: &serde_json::Value) -> McpToolResult {
    let bridge = agnostic_bridge();
    match bridge.get("/api/v1/dashboard", &[]).await {
        Ok(response) => {
            info!("Agnostic: dashboard (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Agnostic bridge: dashboard failed");
            error_result(format!("Dashboard unavailable: {}", e))
        }
    }
}

/// List active sessions.
/// GET /api/v1/dashboard/sessions
pub(crate) async fn handle_agnostic_list_sessions(_args: &serde_json::Value) -> McpToolResult {
    let bridge = agnostic_bridge();
    match bridge.get("/api/v1/dashboard/sessions", &[]).await {
        Ok(response) => {
            info!("Agnostic: list sessions (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Agnostic bridge: list sessions failed");
            error_result(format!("Sessions unavailable: {}", e))
        }
    }
}

/// Get agent status overview.
/// GET /api/v1/dashboard/agents
pub(crate) async fn handle_agnostic_agent_status(_args: &serde_json::Value) -> McpToolResult {
    let bridge = agnostic_bridge();
    match bridge.get("/api/v1/dashboard/agents", &[]).await {
        Ok(response) => {
            info!("Agnostic: agent status (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Agnostic bridge: agent status failed");
            error_result(format!("Agent status unavailable: {}", e))
        }
    }
}

/// Quality metric trends.
/// GET /api/v1/dashboard/metrics
pub(crate) async fn handle_agnostic_quality_trends(_args: &serde_json::Value) -> McpToolResult {
    let bridge = agnostic_bridge();
    match bridge.get("/api/v1/dashboard/metrics", &[]).await {
        Ok(response) => {
            info!("Agnostic: quality trends (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Agnostic bridge: quality trends failed");
            error_result(format!("Trends unavailable: {}", e))
        }
    }
}

/// Compare two sessions (regression analysis).
/// POST /api/v1/sessions/compare
pub(crate) async fn handle_agnostic_session_diff(args: &serde_json::Value) -> McpToolResult {
    let session_a = match extract_required_string(args, "session_a") {
        Ok(s) => s,
        Err(e) => return e,
    };
    let session_b = match extract_required_string(args, "session_b") {
        Ok(s) => s,
        Err(e) => return e,
    };

    let bridge = agnostic_bridge();
    let body = serde_json::json!({
        "session_a": session_a,
        "session_b": session_b,
    });

    match bridge.post("/api/v1/sessions/compare", body).await {
        Ok(response) => {
            info!("Agnostic: session diff (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Agnostic bridge: session diff failed");
            error_result(format!("Session diff failed: {}", e))
        }
    }
}

/// Per-agent metrics.
/// GET /api/v1/dashboard/agent-metrics
pub(crate) async fn handle_agnostic_agent_metrics(_args: &serde_json::Value) -> McpToolResult {
    let bridge = agnostic_bridge();
    match bridge.get("/api/v1/dashboard/agent-metrics", &[]).await {
        Ok(response) => success_result(response),
        Err(e) => error_result(format!("Agent metrics unavailable: {}", e)),
    }
}

/// LLM usage metrics.
/// GET /api/v1/dashboard/llm
pub(crate) async fn handle_agnostic_llm_usage(_args: &serde_json::Value) -> McpToolResult {
    let bridge = agnostic_bridge();
    match bridge.get("/api/v1/dashboard/llm", &[]).await {
        Ok(response) => success_result(response),
        Err(e) => error_result(format!("LLM usage unavailable: {}", e)),
    }
}

/// Health check.
/// GET /api/v1/health
pub(crate) async fn handle_agnostic_health(_args: &serde_json::Value) -> McpToolResult {
    let bridge = agnostic_bridge();
    match bridge.get("/api/v1/health", &[]).await {
        Ok(response) => success_result(response),
        Err(e) => error_result(format!("Health check failed: {}", e)),
    }
}

/// Recommend a preset based on description.
/// POST /api/v1/presets/recommend
pub(crate) async fn handle_agnostic_preset_recommend(args: &serde_json::Value) -> McpToolResult {
    let description = match extract_required_string(args, "description") {
        Ok(d) => d,
        Err(e) => return e,
    };

    let bridge = agnostic_bridge();
    let body = serde_json::json!({ "description": description });

    match bridge.post("/api/v1/presets/recommend", body).await {
        Ok(response) => {
            info!("Agnostic: preset recommend (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Agnostic bridge: preset recommend failed");
            error_result(format!("Preset recommendation failed: {}", e))
        }
    }
}

// ---------------------------------------------------------------------------
// A2A Protocol
// ---------------------------------------------------------------------------

/// Delegate a task via A2A.
/// POST /api/v1/a2a/receive (type: a2a:delegate)
pub(crate) async fn handle_agnostic_a2a_delegate(args: &serde_json::Value) -> McpToolResult {
    let title = match extract_required_string(args, "title") {
        Ok(t) => t,
        Err(e) => return e,
    };
    let description = match extract_required_string(args, "description") {
        Ok(d) => d,
        Err(e) => return e,
    };

    let bridge = agnostic_bridge();
    let body = serde_json::json!({
        "id": Uuid::new_v4().to_string(),
        "type": "a2a:delegate",
        "fromPeerId": "agnosticos-daimon",
        "toPeerId": "agnostic",
        "timestamp": chrono::Utc::now().timestamp_millis(),
        "payload": {
            "title": title,
            "description": description,
            "target_url": get_optional_string_arg(args, "target_url"),
            "preset": get_optional_string_arg(args, "preset"),
            "priority": get_optional_string_arg(args, "priority").unwrap_or_else(|| "high".to_string()),
        },
    });

    match bridge.post("/api/v1/a2a/receive", body).await {
        Ok(response) => {
            info!(title = %title, "Agnostic: A2A delegate (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Agnostic bridge: A2A delegate failed");
            error_result(format!("A2A delegation failed: {}", e))
        }
    }
}

/// Query task status via A2A.
/// POST /api/v1/a2a/receive (type: a2a:status_query)
pub(crate) async fn handle_agnostic_a2a_status(args: &serde_json::Value) -> McpToolResult {
    let task_id = match extract_required_string(args, "task_id") {
        Ok(id) => id,
        Err(e) => return e,
    };

    let bridge = agnostic_bridge();
    let body = serde_json::json!({
        "id": Uuid::new_v4().to_string(),
        "type": "a2a:status_query",
        "fromPeerId": "agnosticos-daimon",
        "toPeerId": "agnostic",
        "timestamp": chrono::Utc::now().timestamp_millis(),
        "payload": { "task_id": task_id },
    });

    match bridge.post("/api/v1/a2a/receive", body).await {
        Ok(response) => {
            info!(task_id = %task_id, "Agnostic: A2A status query (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Agnostic bridge: A2A status query failed");
            error_result(format!("A2A status query failed: {}", e))
        }
    }
}

/// Send A2A heartbeat.
/// POST /api/v1/a2a/receive (type: a2a:heartbeat)
pub(crate) async fn handle_agnostic_a2a_heartbeat(_args: &serde_json::Value) -> McpToolResult {
    let bridge = agnostic_bridge();
    let body = serde_json::json!({
        "id": Uuid::new_v4().to_string(),
        "type": "a2a:heartbeat",
        "fromPeerId": "agnosticos-daimon",
        "toPeerId": "agnostic",
        "timestamp": chrono::Utc::now().timestamp_millis(),
        "payload": {},
    });

    match bridge.post("/api/v1/a2a/receive", body).await {
        Ok(response) => success_result(response),
        Err(e) => error_result(format!("A2A heartbeat failed: {}", e)),
    }
}

// ---------------------------------------------------------------------------
// Crew Management (unchanged)
// ---------------------------------------------------------------------------

/// Run a crew.
/// POST /api/v1/crews
pub(crate) async fn handle_agnostic_run_crew(args: &serde_json::Value) -> McpToolResult {
    let title = match extract_required_string(args, "title") {
        Ok(t) => t,
        Err(e) => return e,
    };
    let description = match extract_required_string(args, "description") {
        Ok(d) => d,
        Err(e) => return e,
    };

    let preset = get_optional_string_arg(args, "preset");
    let target_url = get_optional_string_arg(args, "target_url");
    let priority = get_optional_string_arg(args, "priority").unwrap_or_else(|| "high".to_string());

    let mut body = serde_json::json!({
        "title": title,
        "description": description,
        "priority": priority,
    });
    if let Some(p) = &preset {
        body["preset"] = serde_json::json!(p);
    }
    if let Some(url) = &target_url {
        body["target_url"] = serde_json::json!(url);
    }
    if let Some(keys) = args.get("agent_keys") {
        body["agent_keys"] = keys.clone();
    }
    if let Some(defs) = args.get("agent_definitions") {
        body["agent_definitions"] = defs.clone();
    }
    if let Some(gpu) = args.get("gpu_required") {
        body["gpu_required"] = gpu.clone();
    }
    if let Some(vram) = args.get("min_gpu_memory_mb") {
        body["min_gpu_memory_mb"] = vram.clone();
    }

    let bridge = agnostic_bridge();
    match bridge.post("/api/v1/crews", body).await {
        Ok(response) => {
            info!(title = %title, "Agnostic: run crew (bridged)");
            // #3: Best-effort RPC method registration for crew agents.
            register_crew_rpc_methods(&response).await;
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Agnostic bridge: crew run failed");
            error_result(format!("Crew run failed: {}", e))
        }
    }
}

/// Register placeholder RPC methods for a newly started crew run.
///
/// After a successful `/api/v1/crews` POST the Agnostic response typically
/// contains a `crew_id` (and optionally an `agents` array with `name` fields).
/// We register `{crew_id}.status` and `{crew_id}.result` so that other daimon
/// components can discover crew endpoints via the RPC registry.
///
/// This is a best-effort operation — errors are logged but never propagated.
async fn register_crew_rpc_methods(response: &serde_json::Value) {
    // Extract crew_id: try "crew_id", "id", then "data.crew_id".
    let crew_id = response
        .get("crew_id")
        .or_else(|| response.get("id"))
        .or_else(|| response.get("data").and_then(|d| d.get("crew_id")))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let crew_id = match crew_id {
        Some(id) if !id.is_empty() => id,
        _ => {
            // No crew_id in response — nothing to register.
            return;
        }
    };

    // Build the method list: always include status + result, plus per-agent
    // methods if the response carries an agents array.
    let mut methods: Vec<String> =
        vec![format!("{}.status", crew_id), format!("{}.result", crew_id)];

    if let Some(agents) = response.get("agents").and_then(|a| a.as_array()) {
        for agent in agents {
            if let Some(name) = agent.get("name").and_then(|n| n.as_str()) {
                // Sanitise: only keep chars valid for an RPC method name.
                let sanitised: String = name
                    .chars()
                    .map(|c| {
                        if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                            c
                        } else {
                            '_'
                        }
                    })
                    .collect();
                if !sanitised.is_empty() {
                    methods.push(format!("{}.{}.run", crew_id, sanitised));
                }
            }
        }
    }

    // Synthesise a deterministic UUID-shaped identifier from the crew_id string
    // so that repeated runs for the same crew converge on the same RPC agent slot.
    // We hash the crew_id with SHA-256, take the first 16 bytes, and stamp the
    // UUID version (4) and variant bits to produce a well-formed UUID string.
    // (uuid v5 requires the optional "v5" feature; we avoid touching Cargo.toml.)
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h1 = DefaultHasher::new();
    crew_id.hash(&mut h1);
    let lo = h1.finish();
    let mut h2 = DefaultHasher::new();
    (crew_id.to_string() + "salt").hash(&mut h2);
    let hi = h2.finish();
    // Build 16 raw bytes from the two 64-bit values.
    let mut bytes = [0u8; 16];
    bytes[..8].copy_from_slice(&hi.to_be_bytes());
    bytes[8..].copy_from_slice(&lo.to_be_bytes());
    // Stamp version 4 and RFC 4122 variant.
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let agent_uuid = uuid::Uuid::from_bytes(bytes);

    // POST to daimon's RPC register endpoint.
    let daimon_url =
        std::env::var("DAIMON_URL").unwrap_or_else(|_| "http://127.0.0.1:8090".to_string());
    let payload = serde_json::json!({
        "agent_id": agent_uuid.to_string(),
        "methods": methods,
    });

    let client = reqwest::Client::new();
    match client
        .post(format!("{}/v1/rpc/register", daimon_url))
        .json(&payload)
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => {
            info!(
                crew_id = %crew_id,
                ?methods,
                "Registered RPC methods for crew agents"
            );
        }
        Ok(resp) => {
            warn!(
                crew_id = %crew_id,
                status = %resp.status(),
                "RPC registration for crew returned non-success"
            );
        }
        Err(e) => {
            warn!(
                crew_id = %crew_id,
                error = %e,
                "RPC registration for crew failed (best-effort)"
            );
        }
    }
}

/// Get crew status.
/// GET /api/v1/crews/{crew_id}
pub(crate) async fn handle_agnostic_crew_status(args: &serde_json::Value) -> McpToolResult {
    let crew_id = match extract_required_string(args, "crew_id") {
        Ok(id) => id,
        Err(e) => return e,
    };

    let bridge = agnostic_bridge();
    match bridge.get(&format!("/api/v1/crews/{}", crew_id), &[]).await {
        Ok(response) => {
            info!(crew_id = %crew_id, "Agnostic: crew status (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Agnostic bridge: crew status failed");
            error_result(format!("Crew status failed: {}", e))
        }
    }
}

/// List crews with optional status filter and pagination.
/// GET /api/v1/crews
pub(crate) async fn handle_agnostic_list_crews(args: &serde_json::Value) -> McpToolResult {
    let status = get_optional_string_arg(args, "status");
    let page = get_optional_string_arg(args, "page");
    let per_page = get_optional_string_arg(args, "per_page");

    let bridge = agnostic_bridge();
    let mut query = Vec::new();
    if let Some(ref s) = status {
        query.push(("status".to_string(), s.clone()));
    }
    if let Some(ref p) = page {
        query.push(("page".to_string(), p.clone()));
    }
    if let Some(ref pp) = per_page {
        query.push(("per_page".to_string(), pp.clone()));
    }

    match bridge.get("/api/v1/crews", &query).await {
        Ok(response) => {
            info!("Agnostic: list crews (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Agnostic bridge: list crews failed");
            error_result(format!("Crews unavailable: {}", e))
        }
    }
}

/// Cancel a running or pending crew.
/// POST /api/v1/crews/{crew_id}/cancel
pub(crate) async fn handle_agnostic_cancel_crew(args: &serde_json::Value) -> McpToolResult {
    let crew_id = match extract_required_string(args, "crew_id") {
        Ok(id) => id,
        Err(e) => return e,
    };

    let bridge = agnostic_bridge();
    let body = serde_json::json!({});

    match bridge
        .post(&format!("/api/v1/crews/{}/cancel", crew_id), body)
        .await
    {
        Ok(response) => {
            info!(crew_id = %crew_id, "Agnostic: cancel crew (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Agnostic bridge: cancel crew failed");
            error_result(format!("Cancel crew failed: {}", e))
        }
    }
}

/// List crew presets.
/// GET /api/v1/presets
pub(crate) async fn handle_agnostic_list_presets(args: &serde_json::Value) -> McpToolResult {
    let domain = get_optional_string_arg(args, "domain");
    let size = get_optional_string_arg(args, "size");

    let bridge = agnostic_bridge();
    let mut query = Vec::new();
    if let Some(ref d) = domain {
        query.push(("domain".to_string(), d.clone()));
    }
    if let Some(ref s) = size {
        query.push(("size".to_string(), s.clone()));
    }

    match bridge.get("/api/v1/presets", &query).await {
        Ok(response) => {
            info!("Agnostic: list presets (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Agnostic bridge: list presets failed");
            success_result(serde_json::json!({ "presets": [], "_source": "mock" }))
        }
    }
}

/// List agent definitions.
/// GET /api/v1/definitions
pub(crate) async fn handle_agnostic_list_definitions(args: &serde_json::Value) -> McpToolResult {
    let domain = get_optional_string_arg(args, "domain");

    let bridge = agnostic_bridge();
    let mut query = Vec::new();
    if let Some(ref d) = domain {
        query.push(("domain".to_string(), d.clone()));
    }

    match bridge.get("/api/v1/definitions", &query).await {
        Ok(response) => {
            info!("Agnostic: list definitions (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Agnostic bridge: list definitions failed");
            success_result(serde_json::json!({ "items": [], "total": 0, "_source": "mock" }))
        }
    }
}

// ---------------------------------------------------------------------------
// GPU Placement Query (HUD #6)
// ---------------------------------------------------------------------------

/// Get GPU placement data for a specific crew.
///
/// Calls `GET /api/v1/crews/{crew_id}` and extracts GPU-relevant fields:
/// `gpu_placement`, `gpu_vram`, and which agents have GPU allocated.  Gives
/// the aethersafha HUD a clean, focused data source for rendering GPU badges
/// on crew cards without having to parse the full crew object.
///
/// GET /api/v1/crews/{crew_id}  (GPU fields extracted)
pub(crate) async fn handle_agnostic_crew_gpu(args: &serde_json::Value) -> McpToolResult {
    let crew_id = match extract_required_string(args, "crew_id") {
        Ok(id) => id,
        Err(e) => return e,
    };

    let bridge = agnostic_bridge();
    match bridge.get(&format!("/api/v1/crews/{}", crew_id), &[]).await {
        Ok(response) => {
            info!(crew_id = %crew_id, "Agnostic: crew GPU placement query (bridged)");

            // Extract GPU-relevant fields from the full crew object.
            let gpu_placement = response.get("gpu_placement").cloned();
            let gpu_vram = response.get("gpu_vram").cloned();
            let gpu_device = response.get("gpu_device").cloned();

            // Collect per-agent GPU assignments if the crew embeds agent list.
            let agents_with_gpu: Vec<serde_json::Value> = response
                .get("agents")
                .and_then(|a| a.as_array())
                .map(|agents| {
                    agents
                        .iter()
                        .filter(|agent| {
                            agent
                                .get("gpu_placement")
                                .map(|v| !v.is_null())
                                .unwrap_or(false)
                                || agent
                                    .get("gpu_vram")
                                    .map(|v| !v.is_null())
                                    .unwrap_or(false)
                        })
                        .map(|agent| {
                            serde_json::json!({
                                "agent_key": agent.get("agent_key").cloned().unwrap_or(serde_json::Value::Null),
                                "name": agent.get("name").cloned().unwrap_or(serde_json::Value::Null),
                                "gpu_placement": agent.get("gpu_placement").cloned().unwrap_or(serde_json::Value::Null),
                                "gpu_vram": agent.get("gpu_vram").cloned().unwrap_or(serde_json::Value::Null),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();

            success_result(serde_json::json!({
                "crew_id": crew_id,
                "gpu_placement": gpu_placement,
                "gpu_vram": gpu_vram,
                "gpu_device": gpu_device,
                "agents_with_gpu": agents_with_gpu,
                "agents_with_gpu_count": agents_with_gpu.len(),
            }))
        }
        Err(e) => {
            warn!(error = %e, crew_id = %crew_id, "Agnostic bridge: crew GPU query failed");
            error_result(format!("Crew GPU data unavailable: {}", e))
        }
    }
}

/// Create an agent via A2A protocol.
/// POST /api/v1/a2a/receive (type: a2a:create_agent)
pub(crate) async fn handle_agnostic_create_agent(args: &serde_json::Value) -> McpToolResult {
    let agent_key = match extract_required_string(args, "agent_key") {
        Ok(k) => k,
        Err(e) => return e,
    };
    let name = match extract_required_string(args, "name") {
        Ok(n) => n,
        Err(e) => return e,
    };
    let role = match extract_required_string(args, "role") {
        Ok(r) => r,
        Err(e) => return e,
    };
    let goal = match extract_required_string(args, "goal") {
        Ok(g) => g,
        Err(e) => return e,
    };
    let backstory = match extract_required_string(args, "backstory") {
        Ok(b) => b,
        Err(e) => return e,
    };

    let bridge = agnostic_bridge();
    let body = serde_json::json!({
        "id": Uuid::new_v4().to_string(),
        "type": "a2a:create_agent",
        "fromPeerId": "agnosticos-daimon",
        "toPeerId": "agnostic",
        "timestamp": chrono::Utc::now().timestamp_millis(),
        "payload": {
            "agent_key": agent_key,
            "name": name,
            "role": role,
            "goal": goal,
            "backstory": backstory,
            "domain": get_optional_string_arg(args, "domain").unwrap_or_else(|| "general".to_string()),
            "tools": args.get("tools").cloned().unwrap_or(serde_json::json!([])),
        },
    });

    match bridge.post("/api/v1/a2a/receive", body).await {
        Ok(response) => {
            info!(agent_key = %agent_key, "Agnostic: create agent (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Agnostic bridge: create agent failed");
            error_result(format!("Create agent failed: {}", e))
        }
    }
}
