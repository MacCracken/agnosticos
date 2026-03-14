use tracing::{info, warn};
use uuid::Uuid;

use super::super::helpers::{
    extract_required_string, get_optional_string_arg, success_result, validate_enum_opt,
};
use super::super::types::McpToolResult;

// ---------------------------------------------------------------------------
// Shruti DAW Agent Bridge
// ---------------------------------------------------------------------------

/// Bridge that proxies MCP tool calls to the Shruti DAW API.
///
/// When Shruti is running at its configured endpoint, requests are forwarded
/// to its REST API. When the service is unavailable, mock data is returned.
#[derive(Debug, Clone)]
pub struct ShrutiBridge {
    base_url: String,
    api_key: Option<String>,
}

impl Default for ShrutiBridge {
    fn default() -> Self {
        Self::new()
    }
}

impl ShrutiBridge {
    pub fn new() -> Self {
        Self {
            base_url: std::env::var("SHRUTI_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8091".to_string()),
            api_key: std::env::var("SHRUTI_API_KEY").ok(),
        }
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
            return Err(format!("Shruti API error: {}", resp.status()));
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
            return Err(format!("Shruti API error: {}", resp.status()));
        }
        resp.json().await.map_err(|e| e.to_string())
    }
}

// ---------------------------------------------------------------------------
// Shruti Tool Implementations (bridged)
// ---------------------------------------------------------------------------

pub(crate) async fn handle_shruti_session(args: &serde_json::Value) -> McpToolResult {
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
    let bridge = ShrutiBridge::new();

    match action.as_str() {
        "list" | "info" => {
            let mut query = Vec::new();
            if let Some(ref n) = name {
                query.push(("name".to_string(), n.clone()));
            }
            match bridge.get("/api/v1/sessions", &query).await {
                Ok(response) => {
                    info!("Shruti: {} sessions (bridged)", action);
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "Shruti bridge: falling back to mock for session {}", action);
                    success_result(serde_json::json!({
                        "sessions": [],
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
            match bridge.post("/api/v1/sessions", body).await {
                Ok(response) => {
                    info!(action = %op, "Shruti: {} session (bridged)", op);
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "Shruti bridge: falling back to mock for session {}", op);
                    let session_id = Uuid::new_v4().to_string();
                    success_result(serde_json::json!({
                        "id": session_id,
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

pub(crate) async fn handle_shruti_tracks(args: &serde_json::Value) -> McpToolResult {
    let action = match extract_required_string(args, "action") {
        Ok(a) => a,
        Err(e) => return e,
    };

    let action_opt = Some(action.clone());
    if let Err(e) = validate_enum_opt(&action_opt, "action", &["add", "remove", "list", "rename"]) {
        return e;
    }

    let name = get_optional_string_arg(args, "name");
    let kind = get_optional_string_arg(args, "kind");
    let bridge = ShrutiBridge::new();

    match action.as_str() {
        "list" => match bridge.get("/api/v1/tracks", &[]).await {
            Ok(response) => {
                info!("Shruti: list tracks (bridged)");
                success_result(response)
            }
            Err(e) => {
                warn!(error = %e, "Shruti bridge: falling back to mock for list tracks");
                success_result(serde_json::json!({
                    "tracks": [
                        {"id": "trk-1", "name": "Master", "kind": "master"},
                    ],
                    "total": 1,
                    "_source": "mock",
                }))
            }
        },
        op @ ("add" | "remove" | "rename") => {
            let body = serde_json::json!({
                "action": op,
                "name": name,
                "kind": kind,
            });
            match bridge.post("/api/v1/tracks", body).await {
                Ok(response) => {
                    info!(action = %op, "Shruti: {} track (bridged)", op);
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "Shruti bridge: falling back to mock for {} track", op);
                    let track_id = Uuid::new_v4().to_string();
                    success_result(serde_json::json!({
                        "id": track_id,
                        "action": op,
                        "name": name.unwrap_or_else(|| "Track 1".to_string()),
                        "kind": kind.unwrap_or_else(|| "audio".to_string()),
                        "status": "ok",
                        "_source": "mock",
                    }))
                }
            }
        }
        _ => unreachable!(),
    }
}

pub(crate) async fn handle_shruti_mixer(args: &serde_json::Value) -> McpToolResult {
    let track = match extract_required_string(args, "track") {
        Ok(t) => t,
        Err(e) => return e,
    };

    let gain = args.get("gain").and_then(|v| v.as_f64());
    let mute = args.get("mute").and_then(|v| v.as_bool());
    let solo = args.get("solo").and_then(|v| v.as_bool());

    let bridge = ShrutiBridge::new();
    let body = serde_json::json!({
        "track": track,
        "gain": gain,
        "mute": mute,
        "solo": solo,
    });

    match bridge.post("/api/v1/mixer", body).await {
        Ok(response) => {
            info!(track = %track, "Shruti: mixer update (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Shruti bridge: falling back to mock for mixer");
            success_result(serde_json::json!({
                "track": track,
                "gain_db": gain.unwrap_or(0.0),
                "muted": mute.unwrap_or(false),
                "soloed": solo.unwrap_or(false),
                "status": "ok",
                "_source": "mock",
            }))
        }
    }
}

pub(crate) async fn handle_shruti_transport(args: &serde_json::Value) -> McpToolResult {
    let action = match extract_required_string(args, "action") {
        Ok(a) => a,
        Err(e) => return e,
    };

    let action_opt = Some(action.clone());
    if let Err(e) = validate_enum_opt(
        &action_opt,
        "action",
        &["play", "pause", "stop", "seek", "set_tempo", "status"],
    ) {
        return e;
    }

    let value = get_optional_string_arg(args, "value");
    let bridge = ShrutiBridge::new();

    match action.as_str() {
        "status" => match bridge.get("/api/v1/transport", &[]).await {
            Ok(response) => {
                info!("Shruti: transport status (bridged)");
                success_result(response)
            }
            Err(e) => {
                warn!(error = %e, "Shruti bridge: falling back to mock for transport status");
                success_result(serde_json::json!({
                    "state": "stopped",
                    "position_seconds": 0.0,
                    "tempo_bpm": 120.0,
                    "_source": "mock",
                }))
            }
        },
        op => {
            let body = serde_json::json!({
                "action": op,
                "value": value,
            });
            match bridge.post("/api/v1/transport", body).await {
                Ok(response) => {
                    info!(action = %op, "Shruti: transport {} (bridged)", op);
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "Shruti bridge: falling back to mock for transport {}", op);
                    success_result(serde_json::json!({
                        "action": op,
                        "status": "ok",
                        "value": value,
                        "_source": "mock",
                    }))
                }
            }
        }
    }
}

pub(crate) async fn handle_shruti_export(args: &serde_json::Value) -> McpToolResult {
    let path = get_optional_string_arg(args, "path");
    let format = get_optional_string_arg(args, "format");

    if let Some(ref fmt) = format {
        let fmt_opt = Some(fmt.clone());
        if let Err(e) = validate_enum_opt(&fmt_opt, "format", &["wav", "flac", "mp3", "aac"]) {
            return e;
        }
    }

    let bridge = ShrutiBridge::new();
    let body = serde_json::json!({
        "path": path,
        "format": format,
    });

    match bridge.post("/api/v1/export", body).await {
        Ok(response) => {
            info!("Shruti: export (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Shruti bridge: falling back to mock for export");
            let export_path = path.unwrap_or_else(|| "~/export.wav".to_string());
            let export_format = format.unwrap_or_else(|| "wav".to_string());
            success_result(serde_json::json!({
                "path": export_path,
                "format": export_format,
                "status": "ok",
                "message": "Shruti service not reachable",
                "_source": "mock",
            }))
        }
    }
}

pub(crate) async fn handle_shruti_plugins(args: &serde_json::Value) -> McpToolResult {
    let action = match extract_required_string(args, "action") {
        Ok(a) => a,
        Err(e) => return e,
    };

    let action_opt = Some(action.clone());
    if let Err(e) = validate_enum_opt(
        &action_opt,
        "action",
        &["list", "load", "unload", "scan", "info"],
    ) {
        return e;
    }

    let name = get_optional_string_arg(args, "name");
    let format = get_optional_string_arg(args, "format");
    let path = get_optional_string_arg(args, "path");

    if let Err(e) = validate_enum_opt(&format, "format", &["vst3", "clap", "lv2"]) {
        return e;
    }

    let bridge = ShrutiBridge::new();

    match action.as_str() {
        "list" | "info" => {
            let mut query = Vec::new();
            if let Some(ref n) = name {
                query.push(("name".to_string(), n.clone()));
            }
            if let Some(ref f) = format {
                query.push(("format".to_string(), f.clone()));
            }
            match bridge.get("/api/v1/plugins", &query).await {
                Ok(response) => {
                    info!("Shruti: {} plugins (bridged)", action);
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "Shruti bridge: falling back to mock for plugins {}", action);
                    success_result(serde_json::json!({
                        "plugins": [],
                        "total": 0,
                        "formats": ["vst3", "clap", "lv2"],
                        "_source": "mock",
                    }))
                }
            }
        }
        op @ ("load" | "unload" | "scan") => {
            let body = serde_json::json!({
                "action": op,
                "name": name,
                "format": format,
                "path": path,
            });
            match bridge.post("/api/v1/plugins", body).await {
                Ok(response) => {
                    info!(action = %op, "Shruti: {} plugin (bridged)", op);
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "Shruti bridge: falling back to mock for {} plugin", op);
                    let plugin_id = Uuid::new_v4().to_string();
                    success_result(serde_json::json!({
                        "id": plugin_id,
                        "action": op,
                        "name": name.unwrap_or_else(|| "unknown".to_string()),
                        "format": format.unwrap_or_else(|| "vst3".to_string()),
                        "status": "ok",
                        "_source": "mock",
                    }))
                }
            }
        }
        _ => unreachable!(),
    }
}

pub(crate) async fn handle_shruti_ai(args: &serde_json::Value) -> McpToolResult {
    let action = match extract_required_string(args, "action") {
        Ok(a) => a,
        Err(e) => return e,
    };

    let action_opt = Some(action.clone());
    if let Err(e) = validate_enum_opt(
        &action_opt,
        "action",
        &[
            "mix_suggest",
            "master",
            "stem_split",
            "denoise",
            "transcribe",
            "generate",
        ],
    ) {
        return e;
    }

    let track = get_optional_string_arg(args, "track");
    let options = get_optional_string_arg(args, "options");
    let bridge = ShrutiBridge::new();

    let body = serde_json::json!({
        "action": action,
        "track": track,
        "options": options,
    });

    match bridge.post("/api/v1/ai", body).await {
        Ok(response) => {
            info!(action = %action, "Shruti: AI {} (bridged)", action);
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Shruti bridge: falling back to mock for AI {}", action);
            success_result(serde_json::json!({
                "action": action,
                "status": "ok",
                "message": "Shruti AI not reachable",
                "_source": "mock",
            }))
        }
    }
}
