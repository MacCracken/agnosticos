use tracing::{info, warn};
use uuid::Uuid;

use super::super::helpers::{
    extract_required_string, get_optional_string_arg, success_result, validate_enum_opt,
};
use super::super::types::McpToolResult;
use super::bridge::HttpBridge;

// ---------------------------------------------------------------------------
// Rasa Image Editor Agent Bridge
// ---------------------------------------------------------------------------

pub(crate) fn rasa_bridge() -> HttpBridge {
    HttpBridge::new("RASA_URL", "http://127.0.0.1:8093", "RASA_API_KEY", "Rasa")
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
    let bridge = rasa_bridge();

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
        if let Err(e) = validate_enum_opt(
            &kind_opt,
            "kind",
            &["raster", "vector", "text", "adjustment"],
        ) {
            return e;
        }
    }

    let bridge = rasa_bridge();

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
    let bridge = rasa_bridge();

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
        &[
            "inpaint",
            "upscale",
            "remove_bg",
            "gen_fill",
            "style_transfer",
            "text_to_image",
            "smart_select",
        ],
    ) {
        return e;
    }

    let prompt = get_optional_string_arg(args, "prompt");
    let options = get_optional_string_arg(args, "options");
    let bridge = rasa_bridge();

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
        if let Err(e) = validate_enum_opt(
            &fmt_opt,
            "format",
            &["png", "jpg", "webp", "svg", "tiff", "psd"],
        ) {
            return e;
        }
    }

    let bridge = rasa_bridge();
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
                "message": "Rasa service not reachable",
                "_source": "mock",
            }))
        }
    }
}

pub(crate) async fn handle_rasa_batch(args: &serde_json::Value) -> McpToolResult {
    let action = match extract_required_string(args, "action") {
        Ok(a) => a,
        Err(e) => return e,
    };

    let action_opt = Some(action.clone());
    if let Err(e) = validate_enum_opt(
        &action_opt,
        "action",
        &["resize", "convert", "optimize", "watermark", "list"],
    ) {
        return e;
    }

    let path = get_optional_string_arg(args, "path");
    let output = get_optional_string_arg(args, "output");
    let format = get_optional_string_arg(args, "format");
    let width = get_optional_string_arg(args, "width");
    let height = get_optional_string_arg(args, "height");
    let bridge = rasa_bridge();

    match action.as_str() {
        "list" => {
            let mut query = Vec::new();
            if let Some(ref p) = path {
                query.push(("path".to_string(), p.clone()));
            }
            match bridge.get("/api/v1/batch", &query).await {
                Ok(response) => {
                    info!("Rasa: list batch jobs (bridged)");
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "Rasa bridge: falling back to mock for list batch");
                    success_result(serde_json::json!({
                        "jobs": [],
                        "total": 0,
                        "_source": "mock",
                    }))
                }
            }
        }
        op @ ("resize" | "convert" | "optimize" | "watermark") => {
            let body = serde_json::json!({
                "action": op,
                "path": path,
                "output": output,
                "format": format,
                "width": width,
                "height": height,
            });
            match bridge.post("/api/v1/batch", body).await {
                Ok(response) => {
                    info!(action = %op, "Rasa: batch {} (bridged)", op);
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "Rasa bridge: falling back to mock for batch {}", op);
                    let job_id = Uuid::new_v4().to_string();
                    success_result(serde_json::json!({
                        "job_id": job_id,
                        "action": op,
                        "status": "queued",
                        "_source": "mock",
                    }))
                }
            }
        }
        _ => unreachable!(),
    }
}

pub(crate) async fn handle_rasa_templates(args: &serde_json::Value) -> McpToolResult {
    let action = match extract_required_string(args, "action") {
        Ok(a) => a,
        Err(e) => return e,
    };

    let action_opt = Some(action.clone());
    if let Err(e) = validate_enum_opt(
        &action_opt,
        "action",
        &["list", "create", "apply", "delete", "info"],
    ) {
        return e;
    }

    let name = get_optional_string_arg(args, "name");
    let category = get_optional_string_arg(args, "category");
    let template_id = get_optional_string_arg(args, "template_id");

    if let Err(e) = validate_enum_opt(&category, "category", &["social", "print", "web", "banner"])
    {
        return e;
    }

    let bridge = rasa_bridge();

    match action.as_str() {
        "list" | "info" => {
            let mut query = Vec::new();
            if let Some(ref cat) = category {
                query.push(("category".to_string(), cat.clone()));
            }
            if let Some(ref tid) = template_id {
                query.push(("template_id".to_string(), tid.clone()));
            }
            match bridge.get("/api/v1/templates", &query).await {
                Ok(response) => {
                    info!("Rasa: {} templates (bridged)", action);
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "Rasa bridge: falling back to mock for templates {}", action);
                    success_result(serde_json::json!({
                        "templates": [],
                        "total": 0,
                        "categories": ["social", "print", "web", "banner"],
                        "_source": "mock",
                    }))
                }
            }
        }
        op @ ("create" | "apply" | "delete") => {
            let body = serde_json::json!({
                "action": op,
                "name": name,
                "category": category,
                "template_id": template_id,
            });
            match bridge.post("/api/v1/templates", body).await {
                Ok(response) => {
                    info!(action = %op, "Rasa: {} template (bridged)", op);
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "Rasa bridge: falling back to mock for {} template", op);
                    let tid = template_id.unwrap_or_else(|| Uuid::new_v4().to_string());
                    success_result(serde_json::json!({
                        "template_id": tid,
                        "action": op,
                        "name": name.unwrap_or_else(|| "Untitled".to_string()),
                        "category": category.unwrap_or_else(|| "social".to_string()),
                        "status": "ok",
                        "_source": "mock",
                    }))
                }
            }
        }
        _ => unreachable!(),
    }
}

pub(crate) async fn handle_rasa_adjustments(args: &serde_json::Value) -> McpToolResult {
    let action = match extract_required_string(args, "action") {
        Ok(a) => a,
        Err(e) => return e,
    };

    let action_opt = Some(action.clone());
    if let Err(e) = validate_enum_opt(&action_opt, "action", &["add", "set", "remove", "list"]) {
        return e;
    }

    let adjustment_type = get_optional_string_arg(args, "type");
    let document_id = get_optional_string_arg(args, "document_id");
    let layer_id = get_optional_string_arg(args, "layer_id");
    let params = get_optional_string_arg(args, "params");

    if let Some(ref t) = adjustment_type {
        let t_opt = Some(t.clone());
        if let Err(e) = validate_enum_opt(
            &t_opt,
            "type",
            &["brightness_contrast", "hue_saturation", "curves", "levels"],
        ) {
            return e;
        }
    }

    let bridge = rasa_bridge();

    match action.as_str() {
        "list" => {
            let mut query = Vec::new();
            if let Some(ref did) = document_id {
                query.push(("document_id".to_string(), did.clone()));
            }
            match bridge.get("/api/v1/adjustments", &query).await {
                Ok(response) => {
                    info!("Rasa: list adjustments (bridged)");
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "Rasa bridge: falling back to mock for list adjustments");
                    success_result(serde_json::json!({
                        "adjustments": [],
                        "total": 0,
                        "_source": "mock",
                    }))
                }
            }
        }
        op @ ("add" | "set" | "remove") => {
            let body = serde_json::json!({
                "action": op,
                "type": adjustment_type,
                "document_id": document_id,
                "layer_id": layer_id,
                "params": params,
            });
            match bridge.post("/api/v1/adjustments", body).await {
                Ok(response) => {
                    info!(action = %op, "Rasa: {} adjustment (bridged)", op);
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "Rasa bridge: falling back to mock for {} adjustment", op);
                    let lid = layer_id.unwrap_or_else(|| Uuid::new_v4().to_string());
                    success_result(serde_json::json!({
                        "layer_id": lid,
                        "action": op,
                        "type": adjustment_type.unwrap_or_else(|| "brightness_contrast".to_string()),
                        "status": "ok",
                        "_source": "mock",
                    }))
                }
            }
        }
        _ => unreachable!(),
    }
}
