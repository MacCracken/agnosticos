use tracing::{info, warn};
use uuid::Uuid;

use super::super::helpers::{
    error_result, extract_required_string, get_optional_string_arg, success_result,
};
use super::super::types::McpToolResult;
use super::bridge::HttpBridge;

// ---------------------------------------------------------------------------
// Agnostic Agentics Systems (AAS) Bridge
// ---------------------------------------------------------------------------
// API contract aligned with Agnostic v2026.3.16+
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
    let priority =
        get_optional_string_arg(args, "priority").unwrap_or_else(|| "high".to_string());
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
    match bridge
        .get(&format!("/api/v1/tasks/{}", task_id), &[])
        .await
    {
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

    let title = get_optional_string_arg(args, "title")
        .unwrap_or_else(|| "Security Scan".to_string());
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
pub(crate) async fn handle_agnostic_security_findings(
    args: &serde_json::Value,
) -> McpToolResult {
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
pub(crate) async fn handle_agnostic_performance_test(
    args: &serde_json::Value,
) -> McpToolResult {
    let target_url = match extract_required_string(args, "target_url") {
        Ok(u) => u,
        Err(e) => return e,
    };

    let title = get_optional_string_arg(args, "title")
        .unwrap_or_else(|| "Performance Test".to_string());
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
pub(crate) async fn handle_agnostic_performance_results(
    args: &serde_json::Value,
) -> McpToolResult {
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
pub(crate) async fn handle_agnostic_structured_results(
    args: &serde_json::Value,
) -> McpToolResult {
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
pub(crate) async fn handle_agnostic_generate_report(
    args: &serde_json::Value,
) -> McpToolResult {
    let session_id = match extract_required_string(args, "session_id") {
        Ok(id) => id,
        Err(e) => return e,
    };
    let format =
        get_optional_string_arg(args, "format").unwrap_or_else(|| "json".to_string());

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
pub(crate) async fn handle_agnostic_preset_recommend(
    args: &serde_json::Value,
) -> McpToolResult {
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
    let priority =
        get_optional_string_arg(args, "priority").unwrap_or_else(|| "high".to_string());

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

    let bridge = agnostic_bridge();
    match bridge.post("/api/v1/crews", body).await {
        Ok(response) => {
            info!(title = %title, "Agnostic: run crew (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Agnostic bridge: crew run failed");
            error_result(format!("Crew run failed: {}", e))
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
    match bridge
        .get(&format!("/api/v1/crews/{}", crew_id), &[])
        .await
    {
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
pub(crate) async fn handle_agnostic_list_definitions(
    args: &serde_json::Value,
) -> McpToolResult {
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
