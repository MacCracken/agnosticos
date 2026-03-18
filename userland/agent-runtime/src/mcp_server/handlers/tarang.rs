use std::sync::atomic::Ordering;

use tracing::{info, warn};

use super::super::helpers::{extract_required_string, get_optional_string_arg, success_result};
use super::super::types::McpToolResult;
use super::bridge::HttpBridge;
use crate::resource::ResourceManager;

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

pub(crate) async fn handle_tarang_fingerprint_index(args: &serde_json::Value) -> McpToolResult {
    let path = match extract_required_string(args, "path") {
        Ok(p) => p,
        Err(e) => return e,
    };

    let bridge = tarang_bridge();
    let body = serde_json::json!({ "path": path });

    match bridge.post("/api/v1/fingerprint/index", body).await {
        Ok(response) => {
            info!(path = %path, "Tarang: fingerprint index (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Tarang bridge: falling back to mock for fingerprint_index");
            success_result(serde_json::json!({
                "path": path,
                "status": "unavailable",
                "message": "Tarang service not reachable",
                "_source": "mock",
            }))
        }
    }
}

pub(crate) async fn handle_tarang_search_similar(args: &serde_json::Value) -> McpToolResult {
    let path = match extract_required_string(args, "path") {
        Ok(p) => p,
        Err(e) => return e,
    };

    let top_k = args.get("top_k").and_then(|v| v.as_u64()).unwrap_or(5);
    let bridge = tarang_bridge();
    let body = serde_json::json!({ "path": path, "top_k": top_k });

    match bridge.post("/api/v1/fingerprint/search", body).await {
        Ok(response) => {
            info!(path = %path, top_k = top_k, "Tarang: search similar (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Tarang bridge: falling back to mock for search_similar");
            success_result(serde_json::json!({
                "path": path,
                "results": [],
                "status": "unavailable",
                "message": "Tarang service not reachable",
                "_source": "mock",
            }))
        }
    }
}

pub(crate) async fn handle_tarang_describe(args: &serde_json::Value) -> McpToolResult {
    let path = match extract_required_string(args, "path") {
        Ok(p) => p,
        Err(e) => return e,
    };

    let bridge = tarang_bridge();
    let body = serde_json::json!({ "path": path });

    match bridge.post("/api/v1/describe", body).await {
        Ok(response) => {
            info!(path = %path, "Tarang: AI content description (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Tarang bridge: falling back to mock for describe");
            success_result(serde_json::json!({
                "path": path,
                "description": null,
                "status": "unavailable",
                "message": "Tarang service not reachable",
                "_source": "mock",
            }))
        }
    }
}

/// Probe hardware video decode capabilities available to Tarang/Jalwa.
///
/// Checks for GPU presence via `ResourceManager::detect_gpus()`, then
/// probes for VA-API (open `/dev/dri/renderD128` or `/usr/lib/dri/`)
/// and NVDEC (implied by an NVIDIA GPU being present). The result lets
/// Tarang and Jalwa select the fastest decode path at runtime.
pub(crate) async fn handle_tarang_hw_accel(_args: &serde_json::Value) -> McpToolResult {
    // Detect GPUs — used both to report devices and to infer NVDEC.
    let gpus = match ResourceManager::detect_gpus().await {
        Ok(g) => g,
        Err(e) => {
            warn!(error = %e, "Tarang hw_accel: GPU detection failed, continuing without GPU info");
            vec![]
        }
    };

    let gpu_list: Vec<serde_json::Value> = gpus
        .iter()
        .map(|g| {
            serde_json::json!({
                "id": g.id,
                "name": g.name,
                "total_memory_bytes": g.total_memory,
                "available_memory_bytes": g.available_memory.load(Ordering::Relaxed),
                "compute_capability": g.compute_capability,
            })
        })
        .collect();

    // NVDEC is available on any NVIDIA GPU (Kepler/GK1xx and newer).
    let nvdec_available = gpus
        .iter()
        .any(|g| g.name.to_ascii_lowercase().contains("nvidia"));

    // VA-API: prefer the primary DRM render node; fall back to checking
    // whether the Mesa/Intel/AMD driver directory exists.
    let vaapi_render_node = std::path::Path::new("/dev/dri/renderD128").exists();
    let vaapi_driver_dir = std::path::Path::new("/usr/lib/dri").exists();
    let vaapi_available = vaapi_render_node || vaapi_driver_dir;

    info!(
        gpus = gpu_list.len(),
        vaapi = vaapi_available,
        nvdec = nvdec_available,
        "Tarang: hardware decode capability probe"
    );

    success_result(serde_json::json!({
        "gpus": gpu_list,
        "gpu_count": gpu_list.len(),
        "vaapi": {
            "available": vaapi_available,
            "render_node": vaapi_render_node,
            "driver_dir": vaapi_driver_dir,
        },
        "nvdec": {
            "available": nvdec_available,
        },
        "recommended_decode_path": if nvdec_available {
            "nvdec"
        } else if vaapi_available {
            "vaapi"
        } else {
            "software"
        },
    }))
}
