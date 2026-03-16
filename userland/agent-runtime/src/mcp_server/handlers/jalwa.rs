use tracing::{info, warn};

use super::super::helpers::{
    extract_required_string, get_optional_string_arg, success_result,
};
use super::super::types::McpToolResult;
use super::bridge::HttpBridge;

// ---------------------------------------------------------------------------
// Jalwa Media Player Agent Bridge
// ---------------------------------------------------------------------------

pub(crate) fn jalwa_bridge() -> HttpBridge {
    HttpBridge::new(
        "JALWA_URL",
        "http://127.0.0.1:8093",
        "JALWA_API_KEY",
        "Jalwa",
    )
}

// ---------------------------------------------------------------------------
// Jalwa Tool Implementations (bridged)
// ---------------------------------------------------------------------------

pub(crate) async fn handle_jalwa_play(args: &serde_json::Value) -> McpToolResult {
    let path = match extract_required_string(args, "path") {
        Ok(p) => p,
        Err(e) => return e,
    };

    let bridge = jalwa_bridge();
    let body = serde_json::json!({
        "path": path,
    });

    match bridge.post("/api/v1/play", body).await {
        Ok(response) => {
            info!(path = %path, "Jalwa: play media (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Jalwa bridge: falling back to mock for play");
            success_result(serde_json::json!({
                "path": path,
                "status": "unavailable",
                "message": "Jalwa service not reachable",
                "_source": "mock",
            }))
        }
    }
}

pub(crate) async fn handle_jalwa_pause(_args: &serde_json::Value) -> McpToolResult {
    let bridge = jalwa_bridge();
    let body = serde_json::json!({});

    match bridge.post("/api/v1/pause", body).await {
        Ok(response) => {
            info!("Jalwa: pause playback (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Jalwa bridge: falling back to mock for pause");
            success_result(serde_json::json!({
                "status": "unavailable",
                "message": "Jalwa service not reachable",
                "_source": "mock",
            }))
        }
    }
}

pub(crate) async fn handle_jalwa_status(_args: &serde_json::Value) -> McpToolResult {
    let bridge = jalwa_bridge();

    match bridge.get("/api/v1/status", &[]).await {
        Ok(response) => {
            info!("Jalwa: get playback status (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Jalwa bridge: falling back to mock for status");
            success_result(serde_json::json!({
                "state": "stopped",
                "current_track": null,
                "position_seconds": 0.0,
                "duration_seconds": 0.0,
                "_source": "mock",
            }))
        }
    }
}

pub(crate) async fn handle_jalwa_search(args: &serde_json::Value) -> McpToolResult {
    let query = match extract_required_string(args, "query") {
        Ok(q) => q,
        Err(e) => return e,
    };

    let bridge = jalwa_bridge();
    let params = vec![("query".to_string(), query.clone())];

    match bridge.get("/api/v1/search", &params).await {
        Ok(response) => {
            info!(query = %query, "Jalwa: search library (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Jalwa bridge: falling back to mock for search");
            success_result(serde_json::json!({
                "query": query,
                "results": [],
                "total": 0,
                "_source": "mock",
            }))
        }
    }
}

pub(crate) async fn handle_jalwa_recommend(args: &serde_json::Value) -> McpToolResult {
    let item_id = match extract_required_string(args, "item_id") {
        Ok(id) => id,
        Err(e) => return e,
    };

    let max = get_optional_string_arg(args, "max");
    let bridge = jalwa_bridge();

    let mut params = vec![("item_id".to_string(), item_id.clone())];
    if let Some(ref m) = max {
        params.push(("max".to_string(), m.clone()));
    }

    match bridge.get("/api/v1/recommend", &params).await {
        Ok(response) => {
            info!(item_id = %item_id, "Jalwa: get recommendations (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Jalwa bridge: falling back to mock for recommend");
            success_result(serde_json::json!({
                "item_id": item_id,
                "recommendations": [],
                "total": 0,
                "_source": "mock",
            }))
        }
    }
}
