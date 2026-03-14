use tracing::{info, warn};
use uuid::Uuid;

use super::super::helpers::{
    extract_required_string, get_optional_string_arg, success_result, validate_enum_opt,
};
use super::super::types::McpToolResult;

// ---------------------------------------------------------------------------
// Tazama Video Editor Agent Bridge
// ---------------------------------------------------------------------------

/// Bridge that proxies MCP tool calls to the Tazama video editor API.
///
/// When Tazama is running at its configured endpoint, requests are forwarded
/// to its REST API. When the service is unavailable, mock data is returned.
#[derive(Debug, Clone)]
pub struct TazamaBridge {
    base_url: String,
    api_key: Option<String>,
}

impl Default for TazamaBridge {
    fn default() -> Self {
        Self::new()
    }
}

impl TazamaBridge {
    pub fn new() -> Self {
        Self {
            base_url: std::env::var("TAZAMA_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8092".to_string()),
            api_key: std::env::var("TAZAMA_API_KEY").ok(),
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
            return Err(format!("Tazama API error: {}", resp.status()));
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
            return Err(format!("Tazama API error: {}", resp.status()));
        }
        resp.json().await.map_err(|e| e.to_string())
    }
}

// ---------------------------------------------------------------------------
// Tazama Tool Implementations (bridged)
// ---------------------------------------------------------------------------

pub(crate) async fn handle_tazama_project(args: &serde_json::Value) -> McpToolResult {
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
    let bridge = TazamaBridge::new();

    match action.as_str() {
        "list" | "info" => {
            let mut query = Vec::new();
            if let Some(ref n) = name {
                query.push(("name".to_string(), n.clone()));
            }
            match bridge.get("/api/v1/projects", &query).await {
                Ok(response) => {
                    info!("Tazama: {} projects (bridged)", action);
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "Tazama bridge: falling back to mock for project {}", action);
                    success_result(serde_json::json!({
                        "projects": [],
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
            });
            match bridge.post("/api/v1/projects", body).await {
                Ok(response) => {
                    info!(action = %op, "Tazama: {} project (bridged)", op);
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "Tazama bridge: falling back to mock for project {}", op);
                    let project_id = Uuid::new_v4().to_string();
                    success_result(serde_json::json!({
                        "id": project_id,
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

pub(crate) async fn handle_tazama_timeline(args: &serde_json::Value) -> McpToolResult {
    let action = match extract_required_string(args, "action") {
        Ok(a) => a,
        Err(e) => return e,
    };

    let action_opt = Some(action.clone());
    if let Err(e) = validate_enum_opt(
        &action_opt,
        "action",
        &["add", "remove", "split", "trim", "list", "reorder"],
    ) {
        return e;
    }

    let clip_id = get_optional_string_arg(args, "clip_id");
    let position = args.get("position").and_then(|v| v.as_f64());
    let duration = args.get("duration").and_then(|v| v.as_f64());
    let bridge = TazamaBridge::new();

    match action.as_str() {
        "list" => match bridge.get("/api/v1/timeline", &[]).await {
            Ok(response) => {
                info!("Tazama: list timeline clips (bridged)");
                success_result(response)
            }
            Err(e) => {
                warn!(error = %e, "Tazama bridge: falling back to mock for list timeline");
                success_result(serde_json::json!({
                    "clips": [
                        {"id": "clip-1", "position": 0.0, "duration": 10.0, "name": "Placeholder"},
                    ],
                    "total": 1,
                    "_source": "mock",
                }))
            }
        },
        op @ ("add" | "remove" | "split" | "trim" | "reorder") => {
            let body = serde_json::json!({
                "action": op,
                "clip_id": clip_id,
                "position": position,
                "duration": duration,
            });
            match bridge.post("/api/v1/timeline", body).await {
                Ok(response) => {
                    info!(action = %op, "Tazama: {} timeline clip (bridged)", op);
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "Tazama bridge: falling back to mock for {} timeline", op);
                    let cid = clip_id.unwrap_or_else(|| Uuid::new_v4().to_string());
                    success_result(serde_json::json!({
                        "clip_id": cid,
                        "action": op,
                        "status": "ok",
                        "_source": "mock",
                    }))
                }
            }
        }
        _ => unreachable!(),
    }
}

pub(crate) async fn handle_tazama_effects(args: &serde_json::Value) -> McpToolResult {
    let action = match extract_required_string(args, "action") {
        Ok(a) => a,
        Err(e) => return e,
    };

    let action_opt = Some(action.clone());
    if let Err(e) = validate_enum_opt(
        &action_opt,
        "action",
        &["apply", "remove", "list", "preview"],
    ) {
        return e;
    }

    let effect_type = get_optional_string_arg(args, "effect_type");
    if let Err(e) = validate_enum_opt(
        &effect_type,
        "effect_type",
        &["transition", "color_grade", "filter", "text_overlay"],
    ) {
        return e;
    }

    let name = get_optional_string_arg(args, "name");
    let clip_id = get_optional_string_arg(args, "clip_id");
    let bridge = TazamaBridge::new();

    match action.as_str() {
        "list" => {
            let mut query = Vec::new();
            if let Some(ref et) = effect_type {
                query.push(("effect_type".to_string(), et.clone()));
            }
            match bridge.get("/api/v1/effects", &query).await {
                Ok(response) => {
                    info!("Tazama: list effects (bridged)");
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "Tazama bridge: falling back to mock for list effects");
                    success_result(serde_json::json!({
                        "effects": [],
                        "total": 0,
                        "_source": "mock",
                    }))
                }
            }
        }
        op @ ("apply" | "remove" | "preview") => {
            let body = serde_json::json!({
                "action": op,
                "effect_type": effect_type,
                "name": name,
                "clip_id": clip_id,
            });
            match bridge.post("/api/v1/effects", body).await {
                Ok(response) => {
                    info!(action = %op, "Tazama: {} effect (bridged)", op);
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "Tazama bridge: falling back to mock for {} effect", op);
                    let effect_id = Uuid::new_v4().to_string();
                    success_result(serde_json::json!({
                        "effect_id": effect_id,
                        "action": op,
                        "status": "ok",
                        "_source": "mock",
                    }))
                }
            }
        }
        _ => unreachable!(),
    }
}

pub(crate) async fn handle_tazama_ai(args: &serde_json::Value) -> McpToolResult {
    let action = match extract_required_string(args, "action") {
        Ok(a) => a,
        Err(e) => return e,
    };

    let action_opt = Some(action.clone());
    if let Err(e) = validate_enum_opt(
        &action_opt,
        "action",
        &[
            "scene_detect",
            "auto_cut",
            "subtitle_gen",
            "style_transfer",
            "color_grade",
            "smart_transition",
        ],
    ) {
        return e;
    }

    let options = get_optional_string_arg(args, "options");
    let bridge = TazamaBridge::new();

    let body = serde_json::json!({
        "action": action,
        "options": options,
    });

    match bridge.post("/api/v1/ai", body).await {
        Ok(response) => {
            info!(action = %action, "Tazama: AI {} (bridged)", action);
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Tazama bridge: falling back to mock for AI {}", action);
            success_result(serde_json::json!({
                "action": action,
                "status": "processing",
                "message": format!("AI {} task queued", action),
                "_source": "mock",
            }))
        }
    }
}

pub(crate) async fn handle_tazama_export(args: &serde_json::Value) -> McpToolResult {
    let path = get_optional_string_arg(args, "path");
    let format = get_optional_string_arg(args, "format");
    let resolution = get_optional_string_arg(args, "resolution");
    let quality = get_optional_string_arg(args, "quality");

    if let Some(ref fmt) = format {
        let fmt_opt = Some(fmt.clone());
        if let Err(e) = validate_enum_opt(&fmt_opt, "format", &["mp4", "webm", "mov", "avi", "mkv"])
        {
            return e;
        }
    }

    if let Err(e) = validate_enum_opt(&quality, "quality", &["low", "medium", "high", "lossless"]) {
        return e;
    }

    let bridge = TazamaBridge::new();
    let body = serde_json::json!({
        "path": path,
        "format": format,
        "resolution": resolution,
        "quality": quality,
    });

    match bridge.post("/api/v1/export", body).await {
        Ok(response) => {
            info!("Tazama: export (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Tazama bridge: falling back to mock for export");
            let export_path = path.unwrap_or_else(|| "~/export.mp4".to_string());
            let export_format = format.unwrap_or_else(|| "mp4".to_string());
            success_result(serde_json::json!({
                "path": export_path,
                "format": export_format,
                "status": "ok",
                "message": "Tazama service not reachable",
                "_source": "mock",
            }))
        }
    }
}

pub(crate) async fn handle_tazama_media(args: &serde_json::Value) -> McpToolResult {
    let action = match extract_required_string(args, "action") {
        Ok(a) => a,
        Err(e) => return e,
    };

    let action_opt = Some(action.clone());
    if let Err(e) = validate_enum_opt(
        &action_opt,
        "action",
        &["import", "list", "info", "delete", "transcode"],
    ) {
        return e;
    }

    let path = get_optional_string_arg(args, "path");
    let media_id = get_optional_string_arg(args, "media_id");
    let format = get_optional_string_arg(args, "format");
    let bridge = TazamaBridge::new();

    match action.as_str() {
        "list" | "info" => {
            let mut query = Vec::new();
            if let Some(ref mid) = media_id {
                query.push(("media_id".to_string(), mid.clone()));
            }
            match bridge.get("/api/v1/media", &query).await {
                Ok(response) => {
                    info!("Tazama: {} media (bridged)", action);
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "Tazama bridge: falling back to mock for media {}", action);
                    success_result(serde_json::json!({
                        "media": [],
                        "total": 0,
                        "_source": "mock",
                    }))
                }
            }
        }
        op @ ("import" | "delete" | "transcode") => {
            let body = serde_json::json!({
                "action": op,
                "path": path,
                "media_id": media_id,
                "format": format,
            });
            match bridge.post("/api/v1/media", body).await {
                Ok(response) => {
                    info!(action = %op, "Tazama: {} media (bridged)", op);
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "Tazama bridge: falling back to mock for {} media", op);
                    let mid = media_id.unwrap_or_else(|| Uuid::new_v4().to_string());
                    success_result(serde_json::json!({
                        "media_id": mid,
                        "action": op,
                        "status": "ok",
                        "_source": "mock",
                    }))
                }
            }
        }
        _ => unreachable!(),
    }
}

pub(crate) async fn handle_tazama_subtitles(args: &serde_json::Value) -> McpToolResult {
    let action = match extract_required_string(args, "action") {
        Ok(a) => a,
        Err(e) => return e,
    };

    let action_opt = Some(action.clone());
    if let Err(e) = validate_enum_opt(
        &action_opt,
        "action",
        &["generate", "edit", "export", "import", "list"],
    ) {
        return e;
    }

    let language = get_optional_string_arg(args, "language");
    let format = get_optional_string_arg(args, "format");
    let path = get_optional_string_arg(args, "path");

    if let Err(e) = validate_enum_opt(&format, "format", &["srt", "vtt", "ass"]) {
        return e;
    }

    let bridge = TazamaBridge::new();

    match action.as_str() {
        "list" => {
            let mut query = Vec::new();
            if let Some(ref lang) = language {
                query.push(("language".to_string(), lang.clone()));
            }
            match bridge.get("/api/v1/subtitles", &query).await {
                Ok(response) => {
                    info!("Tazama: list subtitles (bridged)");
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "Tazama bridge: falling back to mock for list subtitles");
                    success_result(serde_json::json!({
                        "subtitles": [],
                        "total": 0,
                        "_source": "mock",
                    }))
                }
            }
        }
        op @ ("generate" | "edit" | "export" | "import") => {
            let body = serde_json::json!({
                "action": op,
                "language": language,
                "format": format,
                "path": path,
            });
            match bridge.post("/api/v1/subtitles", body).await {
                Ok(response) => {
                    info!(action = %op, "Tazama: {} subtitles (bridged)", op);
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "Tazama bridge: falling back to mock for {} subtitles", op);
                    let subtitle_id = Uuid::new_v4().to_string();
                    success_result(serde_json::json!({
                        "subtitle_id": subtitle_id,
                        "action": op,
                        "language": language.unwrap_or_else(|| "en".to_string()),
                        "format": format.unwrap_or_else(|| "srt".to_string()),
                        "status": "ok",
                        "_source": "mock",
                    }))
                }
            }
        }
        _ => unreachable!(),
    }
}
