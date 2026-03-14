use tracing::{info, warn};
use uuid::Uuid;

use super::super::helpers::{
    error_result, extract_required_string, get_optional_string_arg, success_result,
    validate_enum_opt,
};
use super::super::types::McpToolResult;

// ---------------------------------------------------------------------------
// Agnostic Agentics Systems (AAS) Bridge
// ---------------------------------------------------------------------------

/// Bridge that proxies MCP tool calls to the Agnostic platform API.
/// Supports both legacy QA tools and the new multi-domain crew management.
#[derive(Debug, Clone)]
pub struct AgnosticBridge {
    base_url: String,
    api_key: Option<String>,
}

impl Default for AgnosticBridge {
    fn default() -> Self {
        Self::new()
    }
}

impl AgnosticBridge {
    pub fn new() -> Self {
        Self {
            base_url: std::env::var("AGNOSTIC_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8000".to_string()),
            api_key: std::env::var("AGNOSTIC_API_KEY").ok(),
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    fn build_client() -> Result<reqwest::Client, String> {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .connect_timeout(std::time::Duration::from_secs(2))
            .build()
            .map_err(|e| e.to_string())
    }

    async fn get(
        &self,
        path: &str,
        query: &[(String, String)],
    ) -> Result<serde_json::Value, String> {
        let client = Self::build_client()?;
        let url = format!("{}{}", self.base_url, path);
        let mut req = client.get(&url).query(query);
        if let Some(ref key) = self.api_key {
            req = req.header("Authorization", format!("Bearer {}", key));
        }
        let resp = req.send().await.map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("Agnostic API error: {}", resp.status()));
        }
        resp.json().await.map_err(|e| e.to_string())
    }

    async fn post(&self, path: &str, body: serde_json::Value) -> Result<serde_json::Value, String> {
        let client = Self::build_client()?;
        let url = format!("{}{}", self.base_url, path);
        let mut req = client.post(&url).json(&body);
        if let Some(ref key) = self.api_key {
            req = req.header("Authorization", format!("Bearer {}", key));
        }
        let resp = req.send().await.map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("Agnostic API error: {}", resp.status()));
        }
        resp.json().await.map_err(|e| e.to_string())
    }
}

// ---------------------------------------------------------------------------
// Agnostic Tool Implementations (bridged)
// ---------------------------------------------------------------------------

pub(crate) async fn handle_agnostic_run_suite(args: &serde_json::Value) -> McpToolResult {
    let suite = match extract_required_string(args, "suite") {
        Ok(s) => s,
        Err(e) => return e,
    };

    if suite.is_empty() {
        return error_result("Suite name cannot be empty".to_string());
    }

    let target_url = get_optional_string_arg(args, "target_url");
    let agents = args.get("agents").cloned();

    let bridge = AgnosticBridge::new();
    let mut body = serde_json::json!({
        "suite": suite,
    });
    if let Some(url) = &target_url {
        body["target_url"] = serde_json::json!(url);
    }
    if let Some(a) = agents {
        body["agents"] = a;
    }

    match bridge.post("/api/v1/runs", body).await {
        Ok(response) => {
            info!(suite = %suite, "Agnostic: run suite (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Agnostic bridge: falling back to mock for run_suite");
            let run_id = Uuid::new_v4().to_string();
            success_result(serde_json::json!({
                "run_id": run_id,
                "suite": suite,
                "status": "running",
                "agents_active": ["ui", "api", "security"],
                "started_at": chrono::Utc::now().to_rfc3339(),
                "_source": "mock",
            }))
        }
    }
}

pub(crate) async fn handle_agnostic_test_status(args: &serde_json::Value) -> McpToolResult {
    let run_id = match extract_required_string(args, "run_id") {
        Ok(id) => id,
        Err(e) => return e,
    };

    let bridge = AgnosticBridge::new();
    match bridge.get(&format!("/api/v1/runs/{}", run_id), &[]).await {
        Ok(response) => {
            info!(run_id = %run_id, "Agnostic: test status (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Agnostic bridge: falling back to mock for test_status");
            success_result(serde_json::json!({
                "run_id": run_id,
                "status": "completed",
                "total_tests": 156,
                "passed": 148,
                "failed": 5,
                "skipped": 3,
                "duration_seconds": 342,
                "agents": {
                    "ui": {"status": "completed", "tests": 62},
                    "api": {"status": "completed", "tests": 48},
                    "security": {"status": "completed", "tests": 46},
                },
                "_source": "mock",
            }))
        }
    }
}

pub(crate) async fn handle_agnostic_test_report(args: &serde_json::Value) -> McpToolResult {
    let run_id = match extract_required_string(args, "run_id") {
        Ok(id) => id,
        Err(e) => return e,
    };

    let format = get_optional_string_arg(args, "format").unwrap_or_else(|| "summary".to_string());

    let format_opt = Some(format.clone());
    if let Err(e) = validate_enum_opt(&format_opt, "format", &["summary", "full", "json"]) {
        return e;
    }

    let bridge = AgnosticBridge::new();
    let query = vec![("format".to_string(), format.clone())];
    match bridge
        .get(&format!("/api/v1/runs/{}/report", run_id), &query)
        .await
    {
        Ok(response) => {
            info!(run_id = %run_id, "Agnostic: test report (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Agnostic bridge: falling back to mock for test_report");
            success_result(serde_json::json!({
                "run_id": run_id,
                "format": format,
                "summary": {
                    "total": 156, "passed": 148, "failed": 5, "skipped": 3,
                    "pass_rate": 0.968,
                },
                "failures": [
                    {"test": "login_form_validation", "agent": "ui", "message": "Expected error message not displayed for empty email"},
                    {"test": "rate_limit_enforcement", "agent": "api", "message": "429 not returned after 100 requests/min"},
                ],
                "security_findings": [
                    {"severity": "medium", "title": "Missing CSP header on /dashboard", "agent": "security"},
                ],
                "_source": "mock",
            }))
        }
    }
}

pub(crate) async fn handle_agnostic_list_suites(args: &serde_json::Value) -> McpToolResult {
    let category = get_optional_string_arg(args, "category");

    if let Err(e) = validate_enum_opt(
        &category,
        "category",
        &["ui", "api", "security", "performance", "all"],
    ) {
        return e;
    }

    let bridge = AgnosticBridge::new();
    let mut query = Vec::new();
    if let Some(ref c) = category {
        query.push(("category".to_string(), c.clone()));
    }

    match bridge.get("/api/v1/suites", &query).await {
        Ok(response) => {
            info!("Agnostic: list suites (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Agnostic bridge: falling back to mock for list_suites");
            let suites = vec![
                serde_json::json!({"id": "suite-001", "name": "Full Regression", "category": "all", "test_count": 312, "agents": ["ui", "api", "security", "performance"]}),
                serde_json::json!({"id": "suite-002", "name": "Security Audit", "category": "security", "test_count": 89, "agents": ["security"]}),
                serde_json::json!({"id": "suite-003", "name": "API Contract Tests", "category": "api", "test_count": 156, "agents": ["api"]}),
                serde_json::json!({"id": "suite-004", "name": "UI Smoke Tests", "category": "ui", "test_count": 45, "agents": ["ui", "accessibility"]}),
            ];
            let filtered: Vec<_> = if let Some(ref c) = category {
                if c == "all" {
                    suites
                } else {
                    suites
                        .into_iter()
                        .filter(|s| s["category"].as_str() == Some(c.as_str()))
                        .collect()
                }
            } else {
                suites
            };
            success_result(serde_json::json!({
                "suites": filtered,
                "total": filtered.len(),
                "_source": "mock",
            }))
        }
    }
}

pub(crate) async fn handle_agnostic_agent_status(args: &serde_json::Value) -> McpToolResult {
    let agent_type = get_optional_string_arg(args, "agent_type");

    if let Err(e) = validate_enum_opt(
        &agent_type,
        "agent_type",
        &[
            "ui",
            "api",
            "security",
            "performance",
            "accessibility",
            "self-healing",
        ],
    ) {
        return e;
    }

    let bridge = AgnosticBridge::new();
    let mut query = Vec::new();
    if let Some(ref t) = agent_type {
        query.push(("type".to_string(), t.clone()));
    }

    match bridge.get("/api/v1/agents/status", &query).await {
        Ok(response) => {
            info!("Agnostic: agent status (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Agnostic bridge: falling back to mock for agent_status");
            let agents = vec![
                serde_json::json!({"type": "ui", "status": "idle", "last_run": "2026-03-10T14:30:00Z", "tests_run_today": 245}),
                serde_json::json!({"type": "api", "status": "idle", "last_run": "2026-03-10T14:30:00Z", "tests_run_today": 189}),
                serde_json::json!({"type": "security", "status": "idle", "last_run": "2026-03-10T13:00:00Z", "tests_run_today": 89}),
                serde_json::json!({"type": "performance", "status": "idle", "last_run": "2026-03-10T12:00:00Z", "tests_run_today": 34}),
                serde_json::json!({"type": "accessibility", "status": "idle", "last_run": "2026-03-10T14:30:00Z", "tests_run_today": 67}),
                serde_json::json!({"type": "self-healing", "status": "idle", "last_run": "2026-03-10T14:30:00Z", "tests_run_today": 12}),
            ];
            let filtered: Vec<_> = if let Some(ref t) = agent_type {
                agents
                    .into_iter()
                    .filter(|a| a["type"].as_str() == Some(t.as_str()))
                    .collect()
            } else {
                agents
            };
            success_result(serde_json::json!({
                "agents": filtered,
                "total": filtered.len(),
                "_source": "mock",
            }))
        }
    }
}

pub(crate) async fn handle_agnostic_coverage(args: &serde_json::Value) -> McpToolResult {
    let action = match extract_required_string(args, "action") {
        Ok(a) => a,
        Err(e) => return e,
    };

    let action_opt = Some(action.clone());
    if let Err(e) = validate_enum_opt(
        &action_opt,
        "action",
        &["summary", "detail", "diff", "trend"],
    ) {
        return e;
    }

    let suite = get_optional_string_arg(args, "suite");
    let path = get_optional_string_arg(args, "path");
    let threshold = get_optional_string_arg(args, "threshold");

    let bridge = AgnosticBridge::new();
    let mut query = Vec::new();
    query.push(("action".to_string(), action.clone()));
    if let Some(ref s) = suite {
        query.push(("suite".to_string(), s.clone()));
    }
    if let Some(ref p) = path {
        query.push(("path".to_string(), p.clone()));
    }
    if let Some(ref t) = threshold {
        query.push(("threshold".to_string(), t.clone()));
    }

    match bridge.get("/api/v1/coverage", &query).await {
        Ok(response) => {
            info!(action = %action, "Agnostic: coverage (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Agnostic bridge: falling back to mock for coverage {}", action);
            success_result(serde_json::json!({
                "coverage_pct": 0.0,
                "lines_covered": 0,
                "lines_total": 0,
                "_source": "mock",
            }))
        }
    }
}

pub(crate) async fn handle_agnostic_schedule(args: &serde_json::Value) -> McpToolResult {
    let action = match extract_required_string(args, "action") {
        Ok(a) => a,
        Err(e) => return e,
    };

    let action_opt = Some(action.clone());
    if let Err(e) = validate_enum_opt(
        &action_opt,
        "action",
        &["create", "list", "delete", "pause", "resume"],
    ) {
        return e;
    }

    let suite = get_optional_string_arg(args, "suite");
    let cron = get_optional_string_arg(args, "cron");
    let schedule_id = get_optional_string_arg(args, "schedule_id");

    let bridge = AgnosticBridge::new();

    match action.as_str() {
        "list" => {
            let mut query = Vec::new();
            if let Some(ref s) = suite {
                query.push(("suite".to_string(), s.clone()));
            }
            match bridge.get("/api/v1/schedules", &query).await {
                Ok(response) => {
                    info!("Agnostic: list schedules (bridged)");
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "Agnostic bridge: falling back to mock for list schedules");
                    success_result(serde_json::json!({
                        "schedules": [],
                        "total": 0,
                        "_source": "mock",
                    }))
                }
            }
        }
        op @ ("create" | "delete" | "pause" | "resume") => {
            let mut body = serde_json::json!({
                "action": op,
            });
            if let Some(ref s) = suite {
                body["suite"] = serde_json::json!(s);
            }
            if let Some(ref c) = cron {
                body["cron"] = serde_json::json!(c);
            }
            if let Some(ref id) = schedule_id {
                body["schedule_id"] = serde_json::json!(id);
            }
            match bridge.post("/api/v1/schedules", body).await {
                Ok(response) => {
                    info!(action = %op, "Agnostic: {} schedule (bridged)", op);
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "Agnostic bridge: falling back to mock for {} schedule", op);
                    let id = schedule_id.unwrap_or_else(|| Uuid::new_v4().to_string());
                    success_result(serde_json::json!({
                        "schedule_id": id,
                        "action": op,
                        "status": "ok",
                        "updated_at": chrono::Utc::now().to_rfc3339(),
                        "_source": "mock",
                    }))
                }
            }
        }
        _ => unreachable!(),
    }
}

// ---------------------------------------------------------------------------
// AAS Crew Management Tools (Phase 2+)
// ---------------------------------------------------------------------------

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

    let bridge = AgnosticBridge::new();
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

pub(crate) async fn handle_agnostic_crew_status(args: &serde_json::Value) -> McpToolResult {
    let crew_id = match extract_required_string(args, "crew_id") {
        Ok(id) => id,
        Err(e) => return e,
    };

    let bridge = AgnosticBridge::new();
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

pub(crate) async fn handle_agnostic_list_presets(args: &serde_json::Value) -> McpToolResult {
    let domain = get_optional_string_arg(args, "domain");

    let bridge = AgnosticBridge::new();
    let mut query = Vec::new();
    if let Some(ref d) = domain {
        query.push(("domain".to_string(), d.clone()));
    }

    match bridge.get("/api/v1/presets", &query).await {
        Ok(response) => {
            info!("Agnostic: list presets (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Agnostic bridge: list presets failed");
            success_result(serde_json::json!({
                "presets": [
                    {"name": "qa-standard", "domain": "qa", "agent_count": 6},
                    {"name": "data-engineering", "domain": "data-engineering", "agent_count": 3},
                    {"name": "devops", "domain": "devops", "agent_count": 3},
                ],
                "_source": "mock",
            }))
        }
    }
}

pub(crate) async fn handle_agnostic_list_definitions(args: &serde_json::Value) -> McpToolResult {
    let domain = get_optional_string_arg(args, "domain");

    let bridge = AgnosticBridge::new();
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
            success_result(serde_json::json!({
                "items": [],
                "total": 0,
                "_source": "mock",
            }))
        }
    }
}

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

    // Route through A2A create_agent message
    let bridge = AgnosticBridge::new();
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
