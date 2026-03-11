use tracing::{debug, info, warn};
use uuid::Uuid;

use super::super::helpers::{
    error_result, extract_optional_u64, extract_required_string, get_optional_string_arg,
    success_result, validate_enum_opt,
};
use super::super::types::McpToolResult;

// ---------------------------------------------------------------------------
// Delta Code Hosting Agent Bridge
// ---------------------------------------------------------------------------

/// Bridge that proxies MCP tool calls to the Delta code hosting API.
///
/// When Delta is running at its configured endpoint, requests are forwarded to
/// its REST API. When the service is unavailable, mock data is returned.
#[derive(Debug, Clone)]
pub struct DeltaBridge {
    /// Base URL for the Delta API (default: `http://127.0.0.1:8070`).
    base_url: String,
    /// API key for authenticating with Delta.
    api_key: Option<String>,
}

impl Default for DeltaBridge {
    fn default() -> Self {
        Self::new()
    }
}

impl DeltaBridge {
    pub fn new() -> Self {
        Self {
            base_url: std::env::var("DELTA_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8070".to_string()),
            api_key: std::env::var("DELTA_API_KEY").ok(),
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
            return Err(format!("Delta API error: {}", resp.status()));
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
            return Err(format!("Delta API error: {}", resp.status()));
        }
        resp.json().await.map_err(|e| e.to_string())
    }

    #[allow(dead_code)]
    async fn health_check(&self) -> bool {
        let client = reqwest::Client::new();
        let url = format!("{}/api/v1/health", self.base_url);
        match client
            .get(&url)
            .timeout(std::time::Duration::from_secs(2))
            .send()
            .await
        {
            Ok(r) => r.status().is_success(),
            Err(e) => {
                debug!(url = %url, error = %e, "Delta health check failed");
                false
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Delta Tool Implementations (bridged)
// ---------------------------------------------------------------------------

pub(crate) async fn handle_delta_create_repository(args: &serde_json::Value) -> McpToolResult {
    let name = match extract_required_string(args, "name") {
        Ok(n) => n,
        Err(e) => return e,
    };

    if name.is_empty() {
        return error_result("Repository name cannot be empty".to_string());
    }

    let description = get_optional_string_arg(args, "description");
    let visibility =
        get_optional_string_arg(args, "visibility").unwrap_or_else(|| "private".to_string());

    let vis_opt = Some(visibility.clone());
    if let Err(e) = validate_enum_opt(&vis_opt, "visibility", &["public", "private"]) {
        return e;
    }

    let bridge = DeltaBridge::new();
    let mut body = serde_json::json!({
        "name": name,
        "visibility": visibility,
    });
    if let Some(desc) = description {
        body["description"] = serde_json::json!(desc);
    }

    match bridge.post("/api/v1/repos", body).await {
        Ok(response) => {
            info!(name = %name, "Delta: create repository (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Delta bridge: falling back to mock for create_repository");
            let repo_id = Uuid::new_v4().to_string();
            success_result(serde_json::json!({
                "id": repo_id,
                "name": name,
                "visibility": visibility,
                "default_branch": "main",
                "created_at": chrono::Utc::now().to_rfc3339(),
                "_source": "mock",
            }))
        }
    }
}

pub(crate) async fn handle_delta_list_repositories(args: &serde_json::Value) -> McpToolResult {
    let owner = get_optional_string_arg(args, "owner");
    let limit = extract_optional_u64(args, "limit", 20) as usize;

    let bridge = DeltaBridge::new();
    let mut query = Vec::new();
    if let Some(ref o) = owner {
        query.push(("owner".to_string(), o.clone()));
    }
    query.push(("limit".to_string(), limit.to_string()));

    match bridge.get("/api/v1/repos", &query).await {
        Ok(response) => {
            info!("Delta: list repositories (bridged)");
            // Normalize: Delta API returns a bare array; wrap it for consistency
            let repos = if response.is_array() {
                response
            } else {
                response
                    .get("repositories")
                    .cloned()
                    .unwrap_or(serde_json::json!([]))
            };
            let total = repos.as_array().map(|a| a.len()).unwrap_or(0);
            success_result(serde_json::json!({
                "repositories": repos,
                "total": total,
                "_source": "bridge",
            }))
        }
        Err(e) => {
            warn!(error = %e, "Delta bridge: falling back to mock for list_repositories");
            let repos = vec![
                serde_json::json!({"id": "repo-001", "name": "my-project", "visibility": "private", "default_branch": "main"}),
                serde_json::json!({"id": "repo-002", "name": "shared-lib", "visibility": "public", "default_branch": "main"}),
            ];
            success_result(serde_json::json!({
                "repositories": repos,
                "total": repos.len(),
                "_source": "mock",
            }))
        }
    }
}

pub(crate) async fn handle_delta_pull_request(args: &serde_json::Value) -> McpToolResult {
    let action = match extract_required_string(args, "action") {
        Ok(a) => a,
        Err(e) => return e,
    };

    let action_opt = Some(action.clone());
    if let Err(e) = validate_enum_opt(&action_opt, "action", &["list", "create", "merge", "close"])
    {
        return e;
    }

    let repo = get_optional_string_arg(args, "repo");
    let bridge = DeltaBridge::new();

    match action.as_str() {
        "list" => {
            let mut query = Vec::new();
            if let Some(ref r) = repo {
                query.push(("repo".to_string(), r.clone()));
            }
            match bridge.get("/api/v1/pulls", &query).await {
                Ok(response) => {
                    info!("Delta: list pull requests (bridged)");
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "Delta bridge: falling back to mock for list PRs");
                    let prs = vec![
                        serde_json::json!({"id": "pr-1", "title": "Add feature X", "status": "open", "source_branch": "feature/x", "target_branch": "main"}),
                    ];
                    success_result(serde_json::json!({
                        "pull_requests": prs,
                        "total": prs.len(),
                        "_source": "mock",
                    }))
                }
            }
        }
        "create" => {
            let title =
                get_optional_string_arg(args, "title").unwrap_or_else(|| "Untitled PR".to_string());
            let source_branch = get_optional_string_arg(args, "source_branch")
                .unwrap_or_else(|| "feature".to_string());
            let target_branch = get_optional_string_arg(args, "target_branch")
                .unwrap_or_else(|| "main".to_string());

            let body = serde_json::json!({
                "title": title,
                "source_branch": source_branch,
                "target_branch": target_branch,
                "repo": repo,
            });
            match bridge.post("/api/v1/pulls", body).await {
                Ok(response) => {
                    info!(title = %title, "Delta: create PR (bridged)");
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "Delta bridge: falling back to mock for create PR");
                    let pr_id = Uuid::new_v4().to_string();
                    success_result(serde_json::json!({
                        "id": pr_id,
                        "title": title,
                        "status": "open",
                        "source_branch": source_branch,
                        "target_branch": target_branch,
                        "created_at": chrono::Utc::now().to_rfc3339(),
                        "_source": "mock",
                    }))
                }
            }
        }
        op @ ("merge" | "close") => {
            let pr_id = match get_optional_string_arg(args, "pr_id") {
                Some(id) => id,
                None => return error_result("Missing required argument: pr_id".to_string()),
            };
            let body = serde_json::json!({"action": op});
            match bridge.post(&format!("/api/v1/pulls/{}", pr_id), body).await {
                Ok(response) => {
                    info!(pr_id = %pr_id, action = %op, "Delta: {} PR (bridged)", op);
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "Delta bridge: falling back to mock for {} PR", op);
                    success_result(serde_json::json!({
                        "id": pr_id,
                        "status": if op == "merge" { "merged" } else { "closed" },
                        "updated_at": chrono::Utc::now().to_rfc3339(),
                        "_source": "mock",
                    }))
                }
            }
        }
        _ => unreachable!(),
    }
}

pub(crate) async fn handle_delta_push(args: &serde_json::Value) -> McpToolResult {
    let repo = get_optional_string_arg(args, "repo");
    let branch = get_optional_string_arg(args, "branch");

    let bridge = DeltaBridge::new();
    let body = serde_json::json!({
        "repo": repo,
        "branch": branch.as_deref().unwrap_or("main"),
    });

    match bridge.post("/api/v1/git/push", body).await {
        Ok(response) => {
            info!("Delta: push (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Delta bridge: falling back to mock for push");
            success_result(serde_json::json!({
                "status": "pushed",
                "repo": repo,
                "branch": branch.unwrap_or_else(|| "main".to_string()),
                "message": format!("Delta not reachable at {}", bridge.base_url()),
                "_source": "mock",
            }))
        }
    }
}

pub(crate) async fn handle_delta_ci_status(args: &serde_json::Value) -> McpToolResult {
    let repo = get_optional_string_arg(args, "repo");
    let pipeline_id = get_optional_string_arg(args, "pipeline_id");

    let bridge = DeltaBridge::new();
    let mut query = Vec::new();
    if let Some(ref r) = repo {
        query.push(("repo".to_string(), r.clone()));
    }
    if let Some(ref p) = pipeline_id {
        query.push(("pipeline_id".to_string(), p.clone()));
    }

    match bridge.get("/api/v1/ci/pipelines", &query).await {
        Ok(response) => {
            info!("Delta: CI status (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Delta bridge: falling back to mock for ci_status");
            let pipelines = vec![
                serde_json::json!({"id": "pipe-001", "repo": repo.as_deref().unwrap_or("unknown"), "status": "passed", "branch": "main", "duration_seconds": 142}),
            ];
            success_result(serde_json::json!({
                "pipelines": pipelines,
                "total": pipelines.len(),
                "_source": "mock",
            }))
        }
    }
}
