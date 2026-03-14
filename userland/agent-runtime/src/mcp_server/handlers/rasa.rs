use tracing::{info, warn};
use uuid::Uuid;

use super::super::helpers::{
    extract_required_string, get_optional_string_arg, success_result, validate_enum_opt,
};
use super::super::types::McpToolResult;

// ---------------------------------------------------------------------------
// Rasa Image Editor Agent Bridge
// ---------------------------------------------------------------------------

/// Bridge that proxies MCP tool calls to the Rasa image editor API.
///
/// When Rasa is running at its configured endpoint, requests are forwarded
/// to its REST API. When the service is unavailable, mock data is returned.
#[derive(Debug, Clone)]
pub struct RasaBridge {
    base_url: String,
    api_key: Option<String>,
}

impl Default for RasaBridge {
    fn default() -> Self {
        Self::new()
    }
}

impl RasaBridge {
    pub fn new() -> Self {
        Self {
            base_url: std::env::var("RASA_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8093".to_string()),
            api_key: std::env::var("RASA_API_KEY").ok(),
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
            return Err(format!("Rasa API error: {}", resp.status()));
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
            return Err(format!("Rasa API error: {}", resp.status()));
        }
        resp.json().await.map_err(|e| e.to_string())
    }
}

// ---------------------------------------------------------------------------
// Rasa Tool Implementations (bridged)
// ---------------------------------------------------------------------------

pub(crate) async fn handle_rasa_canvas(args: &serde_json::Value) -> McpToolResult {
    let action = match extract_required_string(args, "action") {
        Ok(a) => a,
        Err(e) => return e,
    };

    let action_opt = Some(action.clone());
    if let Err(e) = validate_enum_opt(
        &action_opt,
        "action",
        &["create", "open", "save", "close", "info", "list"],
    ) {
        return e;
    }

    let name = get_optional_string_arg(args, "name");
    let width = args.get("width").and_then(|v| v.as_i64());
    let height = args.get("height").and_then(|v| v.as_i64());
    let bridge = RasaBridge::new();

    match action.as_str() {
        "list" | "info" => {
            let mut query = Vec::new();
            if let Some(ref n) = name {
                query.push(("name".to_string(), n.clone()));
            }
            match bridge.get("/api/v1/canvas", &query).await {
                Ok(response) => {
                    info!("Rasa: {} canvases (bridged)", action);
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "Rasa bridge: falling back to mock for canvas {}", action);
                    success_result(serde_json::json!({
                        "canvases": [],
                        "total": 0,
                        "_source": "mock",
                    }))
                }
            }
        }
        op @ ("create" | "open" | "save" | "close") => {
            let body = serde_json::json!({
                "action": op,
                "name": name,
                "width": width,
                "height": height,
            });
            match bridge.post("/api/v1/canvas", body).await {
                Ok(response) => {
                    info!(action = %op, "Rasa: {} canvas (bridged)", op);
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "Rasa bridge: falling back to mock for canvas {}", op);
                    let canvas_id = Uuid::new_v4().to_string();
                    success_result(serde_json::json!({
                        "canvas_id": canvas_id,
                        "action": op,
                        "name": name.unwrap_or_else(|| "Untitled".to_string()),
                        "width": width.unwrap_or(1920),
                        "height": height.unwrap_or(1080),
                        "status": "ok",
                        "_source": "mock",
                    }))
                }
            }
        }
        _ => unreachable!(),
    }
}

pub(crate) async fn handle_rasa_layers(args: &serde_json::Value) -> McpToolResult {
    let action = match extract_required_string(args, "action") {
        Ok(a) => a,
        Err(e) => return e,
    };

    let action_opt = Some(action.clone());
    if let Err(e) = validate_enum_opt(
        &action_opt,
        "action",
        &["add", "remove", "reorder", "merge", "list", "duplicate"],
    ) {
        return e;
    }

    let layer_id = get_optional_string_arg(args, "layer_id");
    let name = get_optional_string_arg(args, "name");
    let kind = get_optional_string_arg(args, "kind");

    if let Some(ref k) = kind {
        let kind_opt = Some(k.clone());
        if let Err(e) = validate_enum_opt(&kind_opt, "kind", &["raster", "vector", "text", "adjustment"]) {
            return e;
        }
    }

    let bridge = RasaBridge::new();

    match action.as_str() {
        "list" => match bridge.get("/api/v1/layers", &[]).await {
            Ok(response) => {
                info!("Rasa: list layers (bridged)");
                success_result(response)
            }
            Err(e) => {
                warn!(error = %e, "Rasa bridge: falling back to mock for list layers");
                success_result(serde_json::json!({
                    "layers": [
                        {"id": "layer-1", "name": "Background", "kind": "raster"},
                    ],
                    "total": 1,
                    "_source": "mock",
                }))
            }
        },
        op @ ("add" | "remove" | "reorder" | "merge" | "duplicate") => {
            let body = serde_json::json!({
                "action": op,
                "layer_id": layer_id,
                "name": name,
                "kind": kind,
            });
            match bridge.post("/api/v1/layers", body).await {
                Ok(response) => {
                    info!(action = %op, "Rasa: {} layer (bridged)", op);
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "Rasa bridge: falling back to mock for {} layer", op);
                    let lid = Uuid::new_v4().to_string();
                    success_result(serde_json::json!({
                        "layer_id": lid,
                        "action": op,
                        "name": name.unwrap_or_else(|| "Layer".to_string()),
                        "kind": kind.unwrap_or_else(|| "raster".to_string()),
                        "status": "ok",
                        "_source": "mock",
                    }))
                }
            }
        }
        _ => unreachable!(),
    }
}

pub(crate) async fn handle_rasa_tools(args: &serde_json::Value) -> McpToolResult {
    let action = match extract_required_string(args, "action") {
        Ok(a) => a,
        Err(e) => return e,
    };

    let action_opt = Some(action.clone());
    if let Err(e) = validate_enum_opt(
        &action_opt,
        "action",
        &["brush", "select", "crop", "resize", "transform", "fill"],
    ) {
        return e;
    }

    let params = get_optional_string_arg(args, "params");
    let bridge = RasaBridge::new();

    let body = serde_json::json!({
        "action": action,
        "params": params,
    });

    match bridge.post("/api/v1/tools", body).await {
        Ok(response) => {
            info!(action = %action, "Rasa: tool {} (bridged)", action);
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Rasa bridge: falling back to mock for tool {}", action);
            success_result(serde_json::json!({
                "action": action,
                "status": "applied",
                "_source": "mock",
            }))
        }
    }
}

pub(crate) async fn handle_rasa_ai(args: &serde_json::Value) -> McpToolResult {
    let action = match extract_required_string(args, "action") {
        Ok(a) => a,
        Err(e) => return e,
    };

    let action_opt = Some(action.clone());
    if let Err(e) = validate_enum_opt(
        &action_opt,
        "action",
        &["inpaint", "upscale", "remove_bg", "gen_fill", "style_transfer", "text_to_image", "smart_select"],
    ) {
        return e;
    }

    let prompt = get_optional_string_arg(args, "prompt");
    let options = get_optional_string_arg(args, "options");
    let bridge = RasaBridge::new();

    let body = serde_json::json!({
        "action": action,
        "prompt": prompt,
        "options": options,
    });

    match bridge.post("/api/v1/ai", body).await {
        Ok(response) => {
            info!(action = %action, "Rasa: AI {} (bridged)", action);
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Rasa bridge: falling back to mock for AI {}", action);
            success_result(serde_json::json!({
                "action": action,
                "status": "processing",
                "_source": "mock",
            }))
        }
    }
}

pub(crate) async fn handle_rasa_export(args: &serde_json::Value) -> McpToolResult {
    let path = get_optional_string_arg(args, "path");
    let format = get_optional_string_arg(args, "format");
    let quality = args.get("quality").and_then(|v| v.as_i64());

    if let Some(ref fmt) = format {
        let fmt_opt = Some(fmt.clone());
        if let Err(e) = validate_enum_opt(&fmt_opt, "format", &["png", "jpg", "webp", "svg", "tiff", "psd"]) {
            return e;
        }
    }

    let bridge = RasaBridge::new();
    let body = serde_json::json!({
        "path": path,
        "format": format,
        "quality": quality,
    });

    match bridge.post("/api/v1/export", body).await {
        Ok(response) => {
            info!("Rasa: export (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Rasa bridge: falling back to mock for export");
            let export_path = path.unwrap_or_else(|| "~/export.png".to_string());
            let export_format = format.unwrap_or_else(|| "png".to_string());
            success_result(serde_json::json!({
                "path": export_path,
                "format": export_format,
                "quality": quality.unwrap_or(100),
                "status": "ok",
                "message": format!("Rasa not reachable at {}", bridge.base_url()),
                "_source": "mock",
            }))
        }
    }
}
