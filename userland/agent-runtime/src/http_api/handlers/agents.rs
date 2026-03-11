use std::collections::HashMap;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use chrono::Utc;
use tracing::{debug, info};
use uuid::Uuid;

use agnos_common::telemetry::TraceContext;

use crate::http_api::state::{ApiState, RegisteredAgentEntry};
use crate::http_api::types::*;

// ---------------------------------------------------------------------------
// Health
// ---------------------------------------------------------------------------

pub async fn health_handler(State(state): State<ApiState>) -> impl IntoResponse {
    let agents = state.agents_read().await;
    let uptime = (Utc::now() - state.started_at()).num_seconds().max(0) as u64;

    let mut components = HashMap::new();

    // Check LLM Gateway reachability
    let llm_status = check_llm_gateway().await;
    components.insert("llm_gateway".to_string(), llm_status);

    // Agent runtime status
    components.insert(
        "agent_registry".to_string(),
        ComponentHealth {
            status: "ok".to_string(),
            message: Some(format!("{} agents registered", agents.len())),
        },
    );

    // System health (blocking I/O — run off the async thread)
    let system = tokio::task::spawn_blocking(gather_system_health)
        .await
        .unwrap_or_else(|_| SystemHealth {
            hostname: "unknown".to_string(),
            load_average: [0.0, 0.0, 0.0],
            memory_total_mb: 0,
            memory_available_mb: 0,
            disk_free_mb: 0,
        });

    let overall_status = if components.values().all(|c| c.status == "ok") {
        "ok"
    } else if components.values().any(|c| c.status == "error") {
        "degraded"
    } else {
        "ok"
    };

    Json(HealthResponse {
        status: overall_status.to_string(),
        service: "agnos-agent-runtime".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        agents_registered: agents.len(),
        uptime_seconds: uptime,
        components,
        system: Some(system),
    })
}

async fn check_llm_gateway() -> ComponentHealth {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());

    let gateway_url =
        std::env::var("AGNOS_GATEWAY_URL").unwrap_or_else(|_| "http://127.0.0.1:8088".to_string());

    let trace_ctx = TraceContext::new_root("agent-runtime");
    let trace_headers = trace_ctx.inject_headers();

    let mut request_builder = client.get(format!("{}/v1/health", gateway_url));
    for (key, value) in &trace_headers {
        request_builder = request_builder.header(key.as_str(), value.as_str());
    }

    match request_builder.send().await {
        Ok(resp) if resp.status().is_success() => ComponentHealth {
            status: "ok".to_string(),
            message: Some("LLM Gateway reachable".to_string()),
        },
        Ok(resp) => ComponentHealth {
            status: "degraded".to_string(),
            message: Some(format!("LLM Gateway returned {}", resp.status())),
        },
        Err(_) => ComponentHealth {
            status: "unreachable".to_string(),
            message: Some("LLM Gateway not responding".to_string()),
        },
    }
}

pub(crate) fn gather_system_health() -> SystemHealth {
    let hostname = hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    // Read /proc/loadavg
    let load_average = std::fs::read_to_string("/proc/loadavg")
        .ok()
        .and_then(|s| {
            let parts: Vec<f64> = s
                .split_whitespace()
                .take(3)
                .filter_map(|p| p.parse().ok())
                .collect();
            if parts.len() == 3 {
                Some([parts[0], parts[1], parts[2]])
            } else {
                None
            }
        })
        .unwrap_or([0.0, 0.0, 0.0]);

    // Read /proc/meminfo
    let (mem_total, mem_available) = std::fs::read_to_string("/proc/meminfo")
        .ok()
        .map(|s| {
            let mut total = 0u64;
            let mut avail = 0u64;
            for line in s.lines() {
                if line.starts_with("MemTotal:") {
                    total = line
                        .split_whitespace()
                        .nth(1)
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(0);
                }
                if line.starts_with("MemAvailable:") {
                    avail = line
                        .split_whitespace()
                        .nth(1)
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(0);
                }
            }
            (total / 1024, avail / 1024) // kB to MB
        })
        .unwrap_or((0, 0));

    // Disk free (/)
    let disk_free = std::process::Command::new("df")
        .args(["--output=avail", "-BM", "/"])
        .output()
        .ok()
        .and_then(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .nth(1)
                .and_then(|l| l.trim().trim_end_matches('M').parse::<u64>().ok())
        })
        .unwrap_or(0);

    SystemHealth {
        hostname,
        load_average,
        memory_total_mb: mem_total,
        memory_available_mb: mem_available,
        disk_free_mb: disk_free,
    }
}

// ---------------------------------------------------------------------------
// Agent CRUD
// ---------------------------------------------------------------------------

pub async fn register_agent_handler(
    State(state): State<ApiState>,
    Json(req): Json<RegisterAgentRequest>,
) -> impl IntoResponse {
    if req.name.is_empty() {
        return bad_request("Agent name is required").into_response();
    }

    if req.name.len() > 256 {
        return bad_request("Agent name too long (max 256)").into_response();
    }

    let mut agents = state.agents_write().await;

    // Check for duplicate names
    if agents.values().any(|a| a.detail.name == req.name) {
        return conflict(format!("Agent '{}' already registered", req.name)).into_response();
    }

    // Use client-specified ID if provided and not already taken, else generate
    let id = if let Some(client_id) = req.id {
        if agents.contains_key(&client_id) {
            return conflict(format!("Agent ID {} already in use", client_id)).into_response();
        }
        client_id
    } else {
        Uuid::new_v4()
    };
    let now = Utc::now();

    let detail = AgentDetail {
        id,
        name: req.name.clone(),
        status: "registered".to_string(),
        capabilities: req.capabilities,
        resource_needs: req.resource_needs,
        metadata: req.metadata,
        registered_at: now,
        last_heartbeat: None,
        current_task: None,
        cpu_percent: None,
        memory_mb: None,
    };

    agents.insert(
        id,
        RegisteredAgentEntry {
            detail: detail.clone(),
        },
    );

    info!("Agent registered: {} ({})", req.name, id);

    let resp = RegisterAgentResponse {
        id,
        name: req.name,
        status: "registered".to_string(),
        registered_at: now,
    };

    match serde_json::to_value(resp) {
        Ok(val) => (StatusCode::CREATED, Json(val)).into_response(),
        Err(e) => internal_error(format!("Serialization error: {}", e)).into_response(),
    }
}

pub async fn heartbeat_handler(
    State(state): State<ApiState>,
    Path(id): Path<Uuid>,
    Json(req): Json<HeartbeatRequest>,
) -> impl IntoResponse {
    let mut agents = state.agents_write().await;

    match agents.get_mut(&id) {
        Some(entry) => {
            entry.detail.last_heartbeat = Some(Utc::now());
            if let Some(status) = req.status {
                entry.detail.status = status;
            }
            if let Some(task) = req.current_task {
                entry.detail.current_task = Some(task);
            }
            if let Some(cpu) = req.cpu_percent {
                entry.detail.cpu_percent = Some(cpu);
            }
            if let Some(mem) = req.memory_mb {
                entry.detail.memory_mb = Some(mem);
            }

            debug!("Heartbeat received from agent {}", id);
            (StatusCode::OK, Json(serde_json::json!({"status": "ok"}))).into_response()
        }
        None => not_found(format!("Agent {} not found", id)).into_response(),
    }
}

pub async fn list_agents_handler(State(state): State<ApiState>) -> impl IntoResponse {
    let agents = state.agents_read().await;
    let agent_list: Vec<AgentDetail> = agents.values().map(|a| a.detail.clone()).collect();
    let total = agent_list.len();

    Json(AgentListResponse {
        agents: agent_list,
        total,
    })
}

pub async fn get_agent_handler(
    State(state): State<ApiState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let agents = state.agents_read().await;

    match agents.get(&id) {
        Some(entry) => match serde_json::to_value(&entry.detail) {
            Ok(val) => (StatusCode::OK, Json(val)).into_response(),
            Err(e) => internal_error(format!("Serialization error: {}", e)).into_response(),
        },
        None => not_found(format!("Agent {} not found", id)).into_response(),
    }
}

pub async fn deregister_agent_handler(
    State(state): State<ApiState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let mut agents = state.agents_write().await;

    match agents.remove(&id) {
        Some(entry) => {
            info!("Agent deregistered: {} ({})", entry.detail.name, id);
            (
                StatusCode::OK,
                Json(serde_json::json!({"status": "deregistered", "id": id.to_string()})),
            )
                .into_response()
        }
        None => not_found(format!("Agent {} not found", id)).into_response(),
    }
}

/// POST /v1/agents/deregister/batch — deregister multiple agents by source or ID list.
pub async fn batch_deregister_handler(
    State(state): State<ApiState>,
    Json(req): Json<crate::http_api::types::BatchDeregisterRequest>,
) -> impl IntoResponse {
    if req.source.is_none() && req.ids.is_none() {
        return bad_request("Either 'source' or 'ids' must be provided").into_response();
    }

    let mut agents = state.agents_write().await;
    let mut results = Vec::new();

    // Collect IDs to remove
    let ids_to_remove: Vec<Uuid> = if let Some(ref source) = req.source {
        agents
            .iter()
            .filter(|(_, entry)| entry.detail.metadata.get("source").map(|s| s.as_str()) == Some(source))
            .map(|(id, _)| *id)
            .collect()
    } else if let Some(ref ids) = req.ids {
        ids.clone()
    } else {
        Vec::new()
    };

    for id in &ids_to_remove {
        match agents.remove(id) {
            Some(entry) => {
                info!("Agent deregistered (batch): {} ({})", entry.detail.name, id);
                results.push(crate::http_api::types::BatchDeregisterResult {
                    id: *id,
                    name: entry.detail.name,
                    status: "deregistered".to_string(),
                });
            }
            None => {
                results.push(crate::http_api::types::BatchDeregisterResult {
                    id: *id,
                    name: String::new(),
                    status: "not_found".to_string(),
                });
            }
        }
    }

    let deregistered = results.iter().filter(|r| r.status == "deregistered").count();
    let not_found = results.iter().filter(|r| r.status == "not_found").count();

    info!(
        "Batch deregister: {} removed, {} not found",
        deregistered, not_found
    );

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "deregistered": deregistered,
            "not_found": not_found,
            "results": results,
        })),
    )
        .into_response()
}

// ---------------------------------------------------------------------------
// Metrics
// ---------------------------------------------------------------------------

pub async fn metrics_handler(State(state): State<ApiState>) -> impl IntoResponse {
    let agents = state.agents_read().await;
    let uptime = (Utc::now() - state.started_at()).num_seconds().max(0) as u64;

    let mut by_status: HashMap<String, usize> = HashMap::new();
    let mut total_cpu: f32 = 0.0;
    let mut cpu_count: usize = 0;
    let mut total_mem: u64 = 0;

    for entry in agents.values() {
        *by_status.entry(entry.detail.status.clone()).or_default() += 1;
        if let Some(cpu) = entry.detail.cpu_percent {
            total_cpu += cpu;
            cpu_count += 1;
        }
        if let Some(mem) = entry.detail.memory_mb {
            total_mem += mem;
        }
    }

    let avg_cpu = if cpu_count > 0 {
        Some(total_cpu / cpu_count as f32)
    } else {
        None
    };

    Json(AgentMetricsResponse {
        total_agents: agents.len(),
        agents_by_status: by_status,
        uptime_seconds: uptime,
        avg_cpu_percent: avg_cpu,
        total_memory_mb: total_mem,
    })
}

pub async fn prometheus_metrics_handler(State(state): State<ApiState>) -> impl IntoResponse {
    let agents = state.agents_read().await;
    let total = agents.len();

    let mut by_status: HashMap<String, usize> = HashMap::new();
    for entry in agents.values() {
        *by_status.entry(entry.detail.status.clone()).or_default() += 1;
    }

    let mut lines = Vec::new();
    lines.push("# HELP agnos_agents_total Total registered agents".to_string());
    lines.push("# TYPE agnos_agents_total gauge".to_string());
    lines.push(format!("agnos_agents_total {}", total));

    lines.push("# HELP agnos_agent_status Agent status breakdown".to_string());
    lines.push("# TYPE agnos_agent_status gauge".to_string());
    for (status, count) in &by_status {
        // Sanitize status to prevent Prometheus label injection
        let safe_status: String = status
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
            .take(64)
            .collect();
        lines.push(format!(
            "agnos_agent_status{{status=\"{}\"}} {}",
            safe_status, count
        ));
    }

    let uptime = (Utc::now() - state.started_at()).num_seconds().max(0) as u64;
    lines.push("# HELP agnos_uptime_seconds Uptime in seconds".to_string());
    lines.push("# TYPE agnos_uptime_seconds gauge".to_string());
    lines.push(format!("agnos_uptime_seconds {}", uptime));

    (
        StatusCode::OK,
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        lines.join("\n"),
    )
}
