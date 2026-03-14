use tracing::{info, warn};
use uuid::Uuid;

use super::super::helpers::{
    extract_required_string, get_optional_string_arg, success_result, validate_enum_opt,
};
use super::super::types::McpToolResult;

// ---------------------------------------------------------------------------
// Mneme Knowledge Base Agent Bridge
// ---------------------------------------------------------------------------

/// Bridge that proxies MCP tool calls to the Mneme knowledge base API.
///
/// When Mneme is running at its configured endpoint, requests are forwarded
/// to its REST API. When the service is unavailable, mock data is returned.
#[derive(Debug, Clone)]
pub struct MnemeBridge {
    base_url: String,
    api_key: Option<String>,
}

impl Default for MnemeBridge {
    fn default() -> Self {
        Self::new()
    }
}

impl MnemeBridge {
    pub fn new() -> Self {
        Self {
            base_url: std::env::var("MNEME_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8094".to_string()),
            api_key: std::env::var("MNEME_API_KEY").ok(),
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
            return Err(format!("Mneme API error: {}", resp.status()));
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
            return Err(format!("Mneme API error: {}", resp.status()));
        }
        resp.json().await.map_err(|e| e.to_string())
    }
}

// ---------------------------------------------------------------------------
// Mneme Tool Implementations (bridged)
// ---------------------------------------------------------------------------

pub(crate) async fn handle_mneme_notebook(args: &serde_json::Value) -> McpToolResult {
    let action = match extract_required_string(args, "action") {
        Ok(a) => a,
        Err(e) => return e,
    };

    let action_opt = Some(action.clone());
    if let Err(e) = validate_enum_opt(
        &action_opt,
        "action",
        &["create", "open", "delete", "list", "info"],
    ) {
        return e;
    }

    let name = get_optional_string_arg(args, "name");
    let bridge = MnemeBridge::new();

    match action.as_str() {
        "list" | "info" => {
            let mut query = Vec::new();
            if let Some(ref n) = name {
                query.push(("name".to_string(), n.clone()));
            }
            match bridge.get("/api/v1/notebooks", &query).await {
                Ok(response) => {
                    info!("Mneme: {} notebooks (bridged)", action);
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "Mneme bridge: falling back to mock for notebook {}", action);
                    success_result(serde_json::json!({
                        "notebooks": [],
                        "total": 0,
                        "_source": "mock",
                    }))
                }
            }
        }
        op @ ("create" | "open" | "delete") => {
            let body = serde_json::json!({
                "action": op,
                "name": name,
            });
            match bridge.post("/api/v1/notebooks", body).await {
                Ok(response) => {
                    info!(action = %op, "Mneme: {} notebook (bridged)", op);
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "Mneme bridge: falling back to mock for notebook {}", op);
                    let notebook_id = Uuid::new_v4().to_string();
                    success_result(serde_json::json!({
                        "notebook_id": notebook_id,
                        "action": op,
                        "name": name.unwrap_or_else(|| "Untitled".to_string()),
                        "status": "ok",
                        "_source": "mock",
                    }))
                }
            }
        }
        _ => unreachable!(),
    }
}

pub(crate) async fn handle_mneme_notes(args: &serde_json::Value) -> McpToolResult {
    let action = match extract_required_string(args, "action") {
        Ok(a) => a,
        Err(e) => return e,
    };

    let action_opt = Some(action.clone());
    if let Err(e) = validate_enum_opt(
        &action_opt,
        "action",
        &["create", "edit", "delete", "list", "get"],
    ) {
        return e;
    }

    let notebook_id = get_optional_string_arg(args, "notebook_id");
    let title = get_optional_string_arg(args, "title");
    let content = get_optional_string_arg(args, "content");
    let bridge = MnemeBridge::new();

    match action.as_str() {
        "list" | "get" => {
            let mut query = Vec::new();
            if let Some(ref nb) = notebook_id {
                query.push(("notebook_id".to_string(), nb.clone()));
            }
            if let Some(ref t) = title {
                query.push(("title".to_string(), t.clone()));
            }
            match bridge.get("/api/v1/notes", &query).await {
                Ok(response) => {
                    info!("Mneme: {} notes (bridged)", action);
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "Mneme bridge: falling back to mock for notes {}", action);
                    success_result(serde_json::json!({
                        "notes": [],
                        "total": 0,
                        "_source": "mock",
                    }))
                }
            }
        }
        op @ ("create" | "edit" | "delete") => {
            let body = serde_json::json!({
                "action": op,
                "notebook_id": notebook_id,
                "title": title,
                "content": content,
            });
            match bridge.post("/api/v1/notes", body).await {
                Ok(response) => {
                    info!(action = %op, "Mneme: {} note (bridged)", op);
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "Mneme bridge: falling back to mock for note {}", op);
                    let note_id = Uuid::new_v4().to_string();
                    success_result(serde_json::json!({
                        "note_id": note_id,
                        "action": op,
                        "title": title.unwrap_or_else(|| "Untitled Note".to_string()),
                        "status": "ok",
                        "_source": "mock",
                    }))
                }
            }
        }
        _ => unreachable!(),
    }
}

pub(crate) async fn handle_mneme_search(args: &serde_json::Value) -> McpToolResult {
    let query_str = match extract_required_string(args, "query") {
        Ok(q) => q,
        Err(e) => return e,
    };

    let notebook_id = get_optional_string_arg(args, "notebook_id");
    let limit = args.get("limit").and_then(|v| v.as_i64());
    let mode = get_optional_string_arg(args, "mode");

    if let Err(e) = validate_enum_opt(&mode, "mode", &["keyword", "semantic", "hybrid"]) {
        return e;
    }

    let bridge = MnemeBridge::new();
    let body = serde_json::json!({
        "query": query_str,
        "notebook_id": notebook_id,
        "limit": limit,
        "mode": mode,
    });

    match bridge.post("/api/v1/search", body).await {
        Ok(response) => {
            info!("Mneme: search (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Mneme bridge: falling back to mock for search");
            success_result(serde_json::json!({
                "query": query_str,
                "results": [],
                "total": 0,
                "_source": "mock",
            }))
        }
    }
}

pub(crate) async fn handle_mneme_ai(args: &serde_json::Value) -> McpToolResult {
    let action = match extract_required_string(args, "action") {
        Ok(a) => a,
        Err(e) => return e,
    };

    let action_opt = Some(action.clone());
    if let Err(e) = validate_enum_opt(
        &action_opt,
        "action",
        &["summarize", "extract_concepts", "auto_link", "generate", "translate"],
    ) {
        return e;
    }

    let note_id = get_optional_string_arg(args, "note_id");
    let prompt = get_optional_string_arg(args, "prompt");
    let bridge = MnemeBridge::new();

    let body = serde_json::json!({
        "action": action,
        "note_id": note_id,
        "prompt": prompt,
    });

    match bridge.post("/api/v1/ai", body).await {
        Ok(response) => {
            info!(action = %action, "Mneme: AI {} (bridged)", action);
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Mneme bridge: falling back to mock for AI {}", action);
            success_result(serde_json::json!({
                "action": action,
                "status": "processing",
                "message": format!("Mneme not reachable at {}", bridge.base_url()),
                "_source": "mock",
            }))
        }
    }
}

pub(crate) async fn handle_mneme_graph(args: &serde_json::Value) -> McpToolResult {
    let action = match extract_required_string(args, "action") {
        Ok(a) => a,
        Err(e) => return e,
    };

    let action_opt = Some(action.clone());
    if let Err(e) = validate_enum_opt(
        &action_opt,
        "action",
        &["view", "connections", "suggest_links", "stats"],
    ) {
        return e;
    }

    let node_id = get_optional_string_arg(args, "node_id");
    let depth = args.get("depth").and_then(|v| v.as_i64());
    let bridge = MnemeBridge::new();

    match action.as_str() {
        "view" | "stats" => {
            let mut query = Vec::new();
            if let Some(ref nid) = node_id {
                query.push(("node_id".to_string(), nid.clone()));
            }
            if let Some(d) = depth {
                query.push(("depth".to_string(), d.to_string()));
            }
            match bridge.get("/api/v1/graph", &query).await {
                Ok(response) => {
                    info!("Mneme: graph {} (bridged)", action);
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "Mneme bridge: falling back to mock for graph {}", action);
                    success_result(serde_json::json!({
                        "action": action,
                        "nodes": 0,
                        "edges": 0,
                        "_source": "mock",
                    }))
                }
            }
        }
        op @ ("connections" | "suggest_links") => {
            let body = serde_json::json!({
                "action": op,
                "node_id": node_id,
                "depth": depth,
            });
            match bridge.post("/api/v1/graph", body).await {
                Ok(response) => {
                    info!(action = %op, "Mneme: graph {} (bridged)", op);
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "Mneme bridge: falling back to mock for graph {}", op);
                    success_result(serde_json::json!({
                        "action": op,
                        "nodes": 0,
                        "edges": 0,
                        "_source": "mock",
                    }))
                }
            }
        }
        _ => unreachable!(),
    }
}
