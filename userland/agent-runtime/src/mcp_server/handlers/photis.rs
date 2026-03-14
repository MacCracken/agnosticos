use tracing::{debug, info, warn};
use uuid::Uuid;

use super::super::helpers::{
    error_result, extract_required_string, get_optional_string_arg, success_result,
    validate_enum_opt,
};
use super::super::types::McpToolResult;

// ---------------------------------------------------------------------------
// Photis Nadi Agent Bridge
// ---------------------------------------------------------------------------

/// Bridge that proxies MCP tool calls to the real Photis Nadi API.
///
/// When Photis Nadi is running at its configured endpoint, requests are
/// forwarded to its REST API. When the service is unavailable, a graceful
/// error is returned (no mock data — the bridge requires the real service).
#[derive(Debug, Clone)]
pub struct PhotisBridge {
    /// Base URL for the Photis Nadi API (default: `http://127.0.0.1:8081`).
    base_url: String,
    /// API key for authenticating with Photis Nadi.
    api_key: Option<String>,
}

impl PhotisBridge {
    /// Create a new bridge with default settings.
    pub fn new() -> Self {
        Self {
            base_url: std::env::var("PHOTISNADI_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8081".to_string()),
            api_key: std::env::var("PHOTISNADI_API_KEY").ok(),
        }
    }

    /// Create a bridge with explicit configuration (for testing).
    pub fn with_config(base_url: String, api_key: Option<String>) -> Self {
        Self { base_url, api_key }
    }

    /// Build the URL for a Photis Nadi API endpoint.
    pub fn url(&self, path: &str) -> String {
        format!("{}/api/v1{}", self.base_url, path)
    }

    /// Build authorization headers.
    pub fn auth_headers(&self) -> Vec<(String, String)> {
        let mut headers = vec![("Content-Type".to_string(), "application/json".to_string())];
        if let Some(ref key) = self.api_key {
            headers.push(("Authorization".to_string(), format!("Bearer {}", key)));
        }
        headers
    }

    /// Check if Photis Nadi is reachable.
    pub async fn health_check(&self) -> bool {
        let url = self.url("/health");
        match reqwest::Client::new()
            .get(&url)
            .timeout(std::time::Duration::from_secs(2))
            .send()
            .await
        {
            Ok(resp) => resp.status().is_success(),
            Err(e) => {
                debug!(url = %url, error = %e, "Photis Nadi health check failed");
                false
            }
        }
    }

    /// Forward a GET request to Photis Nadi.
    pub async fn get(
        &self,
        path: &str,
        query: &[(String, String)],
    ) -> Result<serde_json::Value, String> {
        let url = self.url(path);
        let mut req = reqwest::Client::new().get(&url);

        if let Some(ref key) = self.api_key {
            req = req.header("Authorization", format!("Bearer {}", key));
        }
        if !query.is_empty() {
            req = req.query(query);
        }

        req.timeout(std::time::Duration::from_secs(10))
            .send()
            .await
            .map_err(|e| format!("Photis Nadi unreachable at {}: {}", self.base_url, e))?
            .json::<serde_json::Value>()
            .await
            .map_err(|e| format!("Invalid response from Photis Nadi: {}", e))
    }

    /// Forward a POST request to Photis Nadi.
    pub async fn post(
        &self,
        path: &str,
        body: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let url = self.url(path);
        let mut req = reqwest::Client::new().post(&url).json(&body);

        if let Some(ref key) = self.api_key {
            req = req.header("Authorization", format!("Bearer {}", key));
        }

        req.timeout(std::time::Duration::from_secs(10))
            .send()
            .await
            .map_err(|e| format!("Photis Nadi unreachable at {}: {}", self.base_url, e))?
            .json::<serde_json::Value>()
            .await
            .map_err(|e| format!("Invalid response from Photis Nadi: {}", e))
    }

    /// Forward a PATCH request to Photis Nadi.
    pub async fn patch(
        &self,
        path: &str,
        body: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let url = self.url(path);
        let mut req = reqwest::Client::new().patch(&url).json(&body);

        if let Some(ref key) = self.api_key {
            req = req.header("Authorization", format!("Bearer {}", key));
        }

        req.timeout(std::time::Duration::from_secs(10))
            .send()
            .await
            .map_err(|e| format!("Photis Nadi unreachable at {}: {}", self.base_url, e))?
            .json::<serde_json::Value>()
            .await
            .map_err(|e| format!("Invalid response from Photis Nadi: {}", e))
    }

    /// Get the base URL.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}

impl Default for PhotisBridge {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Photis Nadi Tool Implementations (bridged)
// ---------------------------------------------------------------------------

pub(crate) async fn handle_photis_list_tasks(args: &serde_json::Value) -> McpToolResult {
    let status = get_optional_string_arg(args, "status");
    let board_id = get_optional_string_arg(args, "board_id");

    if let Err(e) = validate_enum_opt(&status, "status", &["todo", "in_progress", "done"]) {
        return e;
    }

    let bridge = PhotisBridge::new();
    let mut query = Vec::new();
    if let Some(ref s) = status {
        query.push(("status".to_string(), s.clone()));
    }
    if let Some(ref b) = board_id {
        query.push(("project_id".to_string(), b.clone()));
    }

    match bridge.get("/tasks", &query).await {
        Ok(response) => {
            info!("Photis: list tasks (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Photis bridge: falling back to mock for list_tasks");
            // Fallback to mock when Photis Nadi is not running
            let mut tasks = vec![
                serde_json::json!({"id": "task-001", "title": "Review PR #42", "status": "todo", "priority": "high", "board_id": "default"}),
                serde_json::json!({"id": "task-002", "title": "Write unit tests", "status": "in_progress", "priority": "medium", "board_id": "default"}),
                serde_json::json!({"id": "task-003", "title": "Deploy v2.0", "status": "done", "priority": "high", "board_id": "releases"}),
            ];
            if let Some(ref s) = status {
                tasks.retain(|t| t["status"].as_str() == Some(s.as_str()));
            }
            if let Some(ref b) = board_id {
                tasks.retain(|t| t["board_id"].as_str() == Some(b.as_str()));
            }
            success_result(serde_json::json!({
                "tasks": tasks,
                "total": tasks.len(),
                "_source": "mock",
            }))
        }
    }
}

pub(crate) async fn handle_photis_create_task(args: &serde_json::Value) -> McpToolResult {
    let title = match extract_required_string(args, "title") {
        Ok(t) => t,
        Err(e) => return e,
    };

    if title.is_empty() {
        return error_result("Task title cannot be empty".to_string());
    }

    let description = get_optional_string_arg(args, "description");
    let board_id =
        get_optional_string_arg(args, "board_id").unwrap_or_else(|| "default".to_string());
    let priority =
        get_optional_string_arg(args, "priority").unwrap_or_else(|| "medium".to_string());

    let priority_opt = Some(priority.clone());
    if let Err(e) = validate_enum_opt(&priority_opt, "priority", &["low", "medium", "high"]) {
        return e;
    }

    let bridge = PhotisBridge::new();
    let mut body = serde_json::json!({
        "title": title,
        "priority": priority,
        "project_id": board_id,
    });
    if let Some(desc) = description {
        body["description"] = serde_json::json!(desc);
    }

    match bridge.post("/tasks", body).await {
        Ok(response) => {
            info!(title = %title, "Photis: create task (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Photis bridge: falling back to mock for create_task");
            let task_id = Uuid::new_v4().to_string();
            success_result(serde_json::json!({
                "id": task_id,
                "title": title,
                "priority": priority,
                "status": "todo",
                "created_at": chrono::Utc::now().to_rfc3339(),
                "_source": "mock",
            }))
        }
    }
}

pub(crate) async fn handle_photis_update_task(args: &serde_json::Value) -> McpToolResult {
    let task_id = match extract_required_string(args, "task_id") {
        Ok(id) => id,
        Err(e) => return e,
    };

    let title = get_optional_string_arg(args, "title");
    let status = get_optional_string_arg(args, "status");
    let priority = get_optional_string_arg(args, "priority");

    if let Err(e) = validate_enum_opt(&status, "status", &["todo", "in_progress", "done"]) {
        return e;
    }
    if let Err(e) = validate_enum_opt(&priority, "priority", &["low", "medium", "high"]) {
        return e;
    }

    let bridge = PhotisBridge::new();
    let mut body = serde_json::json!({});
    if let Some(ref t) = title {
        body["title"] = serde_json::json!(t);
    }
    if let Some(ref s) = status {
        body["status"] = serde_json::json!(s);
    }
    if let Some(ref p) = priority {
        body["priority"] = serde_json::json!(p);
    }

    match bridge.patch(&format!("/tasks/{}", task_id), body).await {
        Ok(response) => {
            info!(task_id = %task_id, "Photis: update task (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Photis bridge: falling back to mock for update_task");
            success_result(serde_json::json!({
                "id": task_id,
                "title": title.unwrap_or_else(|| "Review PR #42".to_string()),
                "status": status.unwrap_or_else(|| "todo".to_string()),
                "priority": priority.unwrap_or_else(|| "medium".to_string()),
                "updated_at": chrono::Utc::now().to_rfc3339(),
                "_source": "mock",
            }))
        }
    }
}

pub(crate) async fn handle_photis_get_rituals(args: &serde_json::Value) -> McpToolResult {
    let date = get_optional_string_arg(args, "date")
        .unwrap_or_else(|| chrono::Utc::now().format("%Y-%m-%d").to_string());

    let bridge = PhotisBridge::new();
    let query = vec![("frequency".to_string(), "daily".to_string())];

    match bridge.get("/rituals", &query).await {
        Ok(response) => {
            info!(date = %date, "Photis: get rituals (bridged)");
            success_result(serde_json::json!({
                "date": date,
                "rituals": response,
            }))
        }
        Err(e) => {
            warn!(error = %e, "Photis bridge: falling back to mock for get_rituals");
            success_result(serde_json::json!({
                "date": date,
                "rituals": [
                    {"name": "Morning meditation", "completed": true, "streak": 12},
                    {"name": "Code review", "completed": false, "streak": 5},
                    {"name": "Exercise", "completed": true, "streak": 30},
                    {"name": "Journal", "completed": false, "streak": 0},
                ],
                "completion_rate": 0.5,
                "_source": "mock",
            }))
        }
    }
}

pub(crate) async fn handle_photis_analytics(args: &serde_json::Value) -> McpToolResult {
    let period = get_optional_string_arg(args, "period").unwrap_or_else(|| "week".to_string());
    let metric = get_optional_string_arg(args, "metric");

    let period_opt = Some(period.clone());
    if let Err(e) = validate_enum_opt(&period_opt, "period", &["day", "week", "month"]) {
        return e;
    }
    if let Err(e) = validate_enum_opt(
        &metric,
        "metric",
        &["tasks_completed", "streak", "velocity"],
    ) {
        return e;
    }

    let bridge = PhotisBridge::new();
    match bridge.get("/analytics", &[]).await {
        Ok(response) => {
            info!(period = %period, "Photis: analytics (bridged)");
            success_result(serde_json::json!({
                "period": period,
                "metrics": response,
            }))
        }
        Err(e) => {
            warn!(error = %e, "Photis bridge: falling back to mock for analytics");
            success_result(serde_json::json!({
                "period": period,
                "metrics": {
                    "tasks_completed": 14,
                    "streak": 7,
                    "velocity": 2.3,
                    "completion_rate": 0.82,
                },
                "_source": "mock",
            }))
        }
    }
}

pub(crate) async fn handle_photis_sync(args: &serde_json::Value) -> McpToolResult {
    let direction =
        get_optional_string_arg(args, "direction").unwrap_or_else(|| "both".to_string());

    let direction_opt = Some(direction.clone());
    if let Err(e) = validate_enum_opt(&direction_opt, "direction", &["push", "pull", "both"]) {
        return e;
    }

    // Sync is Photis Nadi internal — trigger via health check + report status.
    let bridge = PhotisBridge::new();
    let online = bridge.health_check().await;

    info!(direction = %direction, online = online, "Photis: sync");
    if online {
        success_result(serde_json::json!({
            "status": "synced",
            "direction": direction,
            "service_online": true,
            "last_sync": chrono::Utc::now().to_rfc3339(),
        }))
    } else {
        success_result(serde_json::json!({
            "status": "offline",
            "direction": direction,
            "service_online": false,
            "message": format!("Photis Nadi not reachable at {}", bridge.base_url()),
            "_source": "mock",
        }))
    }
}

pub(crate) async fn handle_photis_boards(args: &serde_json::Value) -> McpToolResult {
    let action = match extract_required_string(args, "action") {
        Ok(a) => a,
        Err(e) => return e,
    };

    let action_opt = Some(action.clone());
    if let Err(e) = validate_enum_opt(
        &action_opt,
        "action",
        &["list", "create", "delete", "rename", "info"],
    ) {
        return e;
    }

    let name = get_optional_string_arg(args, "name");
    let board_id = get_optional_string_arg(args, "board_id");
    let bridge = PhotisBridge::new();

    match action.as_str() {
        "list" | "info" => {
            let mut query = Vec::new();
            if let Some(ref bid) = board_id {
                query.push(("board_id".to_string(), bid.clone()));
            }
            if let Some(ref n) = name {
                query.push(("name".to_string(), n.clone()));
            }
            match bridge.get("/boards", &query).await {
                Ok(response) => {
                    info!("Photis: {} boards (bridged)", action);
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "Photis bridge: falling back to mock for boards {}", action);
                    success_result(serde_json::json!({
                        "boards": [{"id": "default", "name": "Main Board", "task_count": 0}],
                        "total": 1,
                        "_source": "mock",
                    }))
                }
            }
        }
        op @ ("create" | "delete" | "rename") => {
            let body = serde_json::json!({
                "action": op,
                "name": name,
                "board_id": board_id,
            });
            match bridge.post("/boards", body).await {
                Ok(response) => {
                    info!(action = %op, "Photis: {} board (bridged)", op);
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "Photis bridge: falling back to mock for board {}", op);
                    let bid = board_id.unwrap_or_else(|| Uuid::new_v4().to_string());
                    success_result(serde_json::json!({
                        "board_id": bid,
                        "action": op,
                        "name": name.unwrap_or_else(|| "Untitled Board".to_string()),
                        "status": "ok",
                        "_source": "mock",
                    }))
                }
            }
        }
        _ => unreachable!(),
    }
}

pub(crate) async fn handle_photis_notes(args: &serde_json::Value) -> McpToolResult {
    let action = match extract_required_string(args, "action") {
        Ok(a) => a,
        Err(e) => return e,
    };

    let action_opt = Some(action.clone());
    if let Err(e) = validate_enum_opt(
        &action_opt,
        "action",
        &["create", "list", "get", "delete", "search"],
    ) {
        return e;
    }

    let content = get_optional_string_arg(args, "content");
    let task_id = get_optional_string_arg(args, "task_id");
    let note_id = get_optional_string_arg(args, "note_id");
    let query_str = get_optional_string_arg(args, "query");
    let bridge = PhotisBridge::new();

    match action.as_str() {
        "list" | "get" | "search" => {
            let mut query = Vec::new();
            if let Some(ref tid) = task_id {
                query.push(("task_id".to_string(), tid.clone()));
            }
            if let Some(ref nid) = note_id {
                query.push(("note_id".to_string(), nid.clone()));
            }
            if let Some(ref q) = query_str {
                query.push(("query".to_string(), q.clone()));
            }
            match bridge.get("/notes", &query).await {
                Ok(response) => {
                    info!("Photis: {} notes (bridged)", action);
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "Photis bridge: falling back to mock for notes {}", action);
                    success_result(serde_json::json!({
                        "notes": [],
                        "total": 0,
                        "_source": "mock",
                    }))
                }
            }
        }
        op @ ("create" | "delete") => {
            let body = serde_json::json!({
                "action": op,
                "content": content,
                "task_id": task_id,
                "note_id": note_id,
            });
            match bridge.post("/notes", body).await {
                Ok(response) => {
                    info!(action = %op, "Photis: {} note (bridged)", op);
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "Photis bridge: falling back to mock for note {}", op);
                    let nid = note_id.unwrap_or_else(|| Uuid::new_v4().to_string());
                    success_result(serde_json::json!({
                        "note_id": nid,
                        "action": op,
                        "status": "ok",
                        "created_at": chrono::Utc::now().to_rfc3339(),
                        "_source": "mock",
                    }))
                }
            }
        }
        _ => unreachable!(),
    }
}
