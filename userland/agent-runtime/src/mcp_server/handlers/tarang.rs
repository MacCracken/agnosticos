use tracing::{info, warn};

use super::super::helpers::{
    extract_required_string, get_optional_string_arg, success_result,
};
use super::super::types::McpToolResult;
use super::bridge::HttpBridge;

// ---------------------------------------------------------------------------
// Tarang Media Framework Agent Bridge
// ---------------------------------------------------------------------------

pub(crate) fn tarang_bridge() -> HttpBridge {
    HttpBridge::new(
        "TARANG_URL",
        "http://127.0.0.1:8092",
        "TARANG_API_KEY",
        "Tarang",
    )
}

// ---------------------------------------------------------------------------
// Tarang Tool Implementations (bridged)
// ---------------------------------------------------------------------------

pub(crate) async fn handle_tarang_probe(args: &serde_json::Value) -> McpToolResult {
    let path = match extract_required_string(args, "path") {
        Ok(p) => p,
        Err(e) => return e,
    };

    let bridge = tarang_bridge();
    let query = vec![("path".to_string(), path.clone())];

    match bridge.get("/api/v1/probe", &query).await {
        Ok(response) => {
            info!(path = %path, "Tarang: probe media file (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Tarang bridge: falling back to mock for probe");
            success_result(serde_json::json!({
                "path": path,
                "streams": [],
                "format": null,
                "duration_seconds": null,
                "_source": "mock",
            }))
        }
    }
}

pub(crate) async fn handle_tarang_analyze(args: &serde_json::Value) -> McpToolResult {
    let path = match extract_required_string(args, "path") {
        Ok(p) => p,
        Err(e) => return e,
    };

    let bridge = tarang_bridge();
    let query = vec![("path".to_string(), path.clone())];

    match bridge.get("/api/v1/analyze", &query).await {
        Ok(response) => {
            info!(path = %path, "Tarang: AI content analysis (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Tarang bridge: falling back to mock for analyze");
            success_result(serde_json::json!({
                "path": path,
                "analysis": null,
                "status": "unavailable",
                "message": "Tarang service not reachable",
                "_source": "mock",
            }))
        }
    }
}

pub(crate) async fn handle_tarang_codecs(_args: &serde_json::Value) -> McpToolResult {
    let bridge = tarang_bridge();

    match bridge.get("/api/v1/codecs", &[]).await {
        Ok(response) => {
            info!("Tarang: list codecs (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Tarang bridge: falling back to mock for codecs");
            success_result(serde_json::json!({
                "codecs": [],
                "total": 0,
                "_source": "mock",
            }))
        }
    }
}

pub(crate) async fn handle_tarang_transcribe(args: &serde_json::Value) -> McpToolResult {
    let path = match extract_required_string(args, "path") {
        Ok(p) => p,
        Err(e) => return e,
    };

    let language = get_optional_string_arg(args, "language");
    let bridge = tarang_bridge();

    let body = serde_json::json!({
        "path": path,
        "language": language,
    });

    match bridge.post("/api/v1/transcribe", body).await {
        Ok(response) => {
            info!(path = %path, "Tarang: prepare transcription (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Tarang bridge: falling back to mock for transcribe");
            success_result(serde_json::json!({
                "path": path,
                "language": language.unwrap_or_else(|| "en".to_string()),
                "status": "unavailable",
                "message": "Tarang service not reachable",
                "_source": "mock",
            }))
        }
    }
}

pub(crate) async fn handle_tarang_formats(args: &serde_json::Value) -> McpToolResult {
    let path = match extract_required_string(args, "path") {
        Ok(p) => p,
        Err(e) => return e,
    };

    let bridge = tarang_bridge();
    let query = vec![("path".to_string(), path.clone())];

    match bridge.get("/api/v1/formats", &query).await {
        Ok(response) => {
            info!(path = %path, "Tarang: detect format (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Tarang bridge: falling back to mock for formats");
            success_result(serde_json::json!({
                "path": path,
                "detected_format": null,
                "magic_bytes": null,
                "_source": "mock",
            }))
        }
    }
}
