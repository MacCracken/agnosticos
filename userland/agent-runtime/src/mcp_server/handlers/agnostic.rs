use tracing::{info, warn};
use uuid::Uuid;

use super::super::helpers::{
    error_result, extract_required_string, get_optional_string_arg, success_result,
    validate_enum_opt,
};
use super::super::types::McpToolResult;

// ---------------------------------------------------------------------------
// Agnostic QA Platform Agent Bridge
// ---------------------------------------------------------------------------

/// Bridge that proxies MCP tool calls to the Agnostic QA platform API.
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

    async fn get(
        &self,
        path: &str,
        query: &[(String, String)],
    ) -> Result<serde_json::Value, String> {
        let client = reqwest::Client::new();
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
        let client = reqwest::Client::new();
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
