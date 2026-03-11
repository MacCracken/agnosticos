//! Screen capture HTTP API handlers for agent-runtime.
//!
//! Provides REST endpoints for agents and system tools to request screenshots,
//! manage capture permissions, and query capture history.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::http_api::state::ApiState;

// ---------------------------------------------------------------------------
// Request / response types
// ---------------------------------------------------------------------------

/// Request body for POST /v1/screen/capture
#[derive(Debug, Deserialize)]
pub struct ScreenCaptureRequest {
    /// What to capture: "full_screen", or object with type
    pub target: CaptureTargetRequest,
    /// Output format: "raw_argb", "png", "bmp" (default: "png")
    #[serde(default = "default_format")]
    pub format: String,
    /// Agent ID requesting the capture (None = system/privileged)
    pub agent_id: Option<String>,
}

fn default_format() -> String {
    "png".to_string()
}

/// Capture target in API request format.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CaptureTargetRequest {
    FullScreen,
    Window {
        surface_id: String,
    },
    Region {
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    },
}

/// Request body for granting capture permission.
#[derive(Debug, Deserialize)]
pub struct GrantPermissionRequest {
    pub agent_id: String,
    /// Allowed target kinds: "full_screen", "window", "region"
    pub allowed_targets: Vec<String>,
    /// Optional expiry in seconds from now
    pub expires_in_secs: Option<u64>,
    /// Max captures per minute (default: 30)
    #[serde(default = "default_rate_limit")]
    pub max_captures_per_minute: u32,
}

fn default_rate_limit() -> u32 {
    30
}

/// Response for a successful capture (metadata only — data returned as base64 or binary).
#[derive(Debug, Serialize)]
pub struct ScreenCaptureResponse {
    pub id: String,
    pub width: u32,
    pub height: u32,
    pub format: String,
    pub data_size: usize,
    pub captured_at: String,
    pub requesting_agent: Option<String>,
    /// Base64-encoded image data.
    pub data_base64: String,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// POST /v1/screen/capture — take a screenshot.
pub async fn screen_capture_handler(
    State(state): State<ApiState>,
    Json(req): Json<ScreenCaptureRequest>,
) -> impl IntoResponse {
    use base64::Engine;

    let manager = state.screen_capture_manager.read().await;

    // Parse format
    let format = match parse_format(&req.format) {
        Ok(f) => f,
        Err(resp) => return resp.into_response(),
    };

    // Parse target
    let target = match req.target {
        CaptureTargetRequest::FullScreen => desktop_environment::CaptureTarget::FullScreen,
        CaptureTargetRequest::Window { ref surface_id } => {
            match uuid::Uuid::parse_str(surface_id) {
                Ok(id) => desktop_environment::CaptureTarget::Window { surface_id: id },
                Err(_) => {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(serde_json::json!({
                            "error": format!("Invalid surface_id '{}' — expected UUID", surface_id),
                            "code": 400
                        })),
                    )
                        .into_response();
                }
            }
        }
        CaptureTargetRequest::Region {
            x,
            y,
            width,
            height,
        } => desktop_environment::CaptureTarget::Region {
            x,
            y,
            width,
            height,
        },
    };

    // Get compositor (read-only reference)
    let compositor = state.compositor.read().await;

    let result = manager.capture(&compositor, target, format, req.agent_id.as_deref());

    match result {
        Ok(capture) => {
            let data_b64 = base64::engine::general_purpose::STANDARD.encode(&capture.data);
            let resp = ScreenCaptureResponse {
                id: capture.id.to_string(),
                width: capture.width,
                height: capture.height,
                format: format!("{:?}", capture.format).to_lowercase(),
                data_size: capture.data_size,
                captured_at: capture.captured_at.to_rfc3339(),
                requesting_agent: capture.requesting_agent,
                data_base64: data_b64,
            };
            (StatusCode::OK, Json(serde_json::json!(resp))).into_response()
        }
        Err(e) => {
            let (status, code) = match &e {
                desktop_environment::CaptureError::SecureModeActive => (StatusCode::FORBIDDEN, 403),
                desktop_environment::CaptureError::PermissionDenied(_)
                | desktop_environment::CaptureError::TargetNotAllowed(_, _)
                | desktop_environment::CaptureError::PermissionExpired(_) => {
                    (StatusCode::FORBIDDEN, 403)
                }
                desktop_environment::CaptureError::RateLimitExceeded(_, _) => {
                    (StatusCode::TOO_MANY_REQUESTS, 429)
                }
                desktop_environment::CaptureError::WindowNotFound(_) => {
                    (StatusCode::NOT_FOUND, 404)
                }
                desktop_environment::CaptureError::RegionOutOfBounds => {
                    (StatusCode::BAD_REQUEST, 400)
                }
                desktop_environment::CaptureError::EncodingError(_) => {
                    (StatusCode::INTERNAL_SERVER_ERROR, 500)
                }
            };
            (
                status,
                Json(serde_json::json!({"error": e.to_string(), "code": code})),
            )
                .into_response()
        }
    }
}

/// POST /v1/screen/permissions — grant capture permission to an agent.
pub async fn screen_grant_permission_handler(
    State(state): State<ApiState>,
    Json(req): Json<GrantPermissionRequest>,
) -> impl IntoResponse {
    if req.agent_id.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "agent_id is required", "code": 400})),
        )
            .into_response();
    }

    let mut targets = Vec::new();
    for t in &req.allowed_targets {
        match parse_target_kind(t) {
            Ok(kind) => targets.push(kind),
            Err(resp) => return resp.into_response(),
        }
    }

    if targets.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "At least one allowed_target is required", "code": 400})),
        )
            .into_response();
    }

    let expires_at = req
        .expires_in_secs
        .map(|s| chrono::Utc::now() + chrono::Duration::seconds(s as i64));

    let perm = desktop_environment::CapturePermission {
        agent_id: req.agent_id.clone(),
        allowed_targets: targets,
        granted_at: chrono::Utc::now(),
        expires_at,
        max_captures_per_minute: req.max_captures_per_minute,
    };

    let manager = state.screen_capture_manager.read().await;
    manager.grant_permission(perm);

    (
        StatusCode::CREATED,
        Json(serde_json::json!({
            "status": "granted",
            "agent_id": req.agent_id,
        })),
    )
        .into_response()
}

/// DELETE /v1/screen/permissions/:agent_id — revoke capture permission.
pub async fn screen_revoke_permission_handler(
    State(state): State<ApiState>,
    Path(agent_id): Path<String>,
) -> impl IntoResponse {
    let manager = state.screen_capture_manager.read().await;
    let removed = manager.revoke_permission(&agent_id);
    if removed {
        (
            StatusCode::OK,
            Json(serde_json::json!({"status": "revoked", "agent_id": agent_id})),
        )
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "No permission found for agent", "code": 404})),
        )
    }
}

/// GET /v1/screen/permissions — list all capture permissions.
pub async fn screen_list_permissions_handler(State(state): State<ApiState>) -> impl IntoResponse {
    let manager = state.screen_capture_manager.read().await;
    let perms = manager.list_permissions();
    Json(serde_json::json!({"permissions": perms}))
}

/// GET /v1/screen/history — list recent capture history.
pub async fn screen_history_handler(State(state): State<ApiState>) -> impl IntoResponse {
    let manager = state.screen_capture_manager.read().await;
    let history = manager.capture_history();
    Json(serde_json::json!({"captures": history, "count": history.len()}))
}

// ---------------------------------------------------------------------------
// Screen recording handlers
// ---------------------------------------------------------------------------

/// Request body for POST /v1/screen/recording/start
#[derive(Debug, Deserialize)]
pub struct StartRecordingRequest {
    pub target: CaptureTargetRequest,
    #[serde(default = "default_format")]
    pub format: String,
    pub agent_id: Option<String>,
    /// Frame interval in milliseconds (default: 100 = 10fps)
    pub frame_interval_ms: Option<u32>,
    /// Max frames to capture (default: 600)
    pub max_frames: Option<u32>,
    /// Max duration in seconds (default: 60)
    pub max_duration_secs: Option<u64>,
}

/// Query params for GET /v1/screen/recording/:id/frames
#[derive(Debug, Deserialize)]
pub struct FramesQuery {
    /// Return frames with sequence > since
    pub since: Option<u32>,
}

fn parse_capture_target(
    target: &CaptureTargetRequest,
) -> Result<desktop_environment::CaptureTarget, (StatusCode, Json<serde_json::Value>)> {
    match target {
        CaptureTargetRequest::FullScreen => Ok(desktop_environment::CaptureTarget::FullScreen),
        CaptureTargetRequest::Window { surface_id } => uuid::Uuid::parse_str(surface_id)
            .map(|id| desktop_environment::CaptureTarget::Window { surface_id: id })
            .map_err(|_| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "error": format!("Invalid surface_id '{}' — expected UUID", surface_id),
                        "code": 400
                    })),
                )
            }),
        CaptureTargetRequest::Region {
            x,
            y,
            width,
            height,
        } => Ok(desktop_environment::CaptureTarget::Region {
            x: *x,
            y: *y,
            width: *width,
            height: *height,
        }),
    }
}

fn parse_target_kind(
    s: &str,
) -> Result<desktop_environment::CaptureTargetKind, (StatusCode, Json<serde_json::Value>)> {
    match s {
        "full_screen" => Ok(desktop_environment::CaptureTargetKind::FullScreen),
        "window" => Ok(desktop_environment::CaptureTargetKind::Window),
        "region" => Ok(desktop_environment::CaptureTargetKind::Region),
        other => Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": format!("Unknown target kind '{}'; expected full_screen, window, or region", other),
                "code": 400
            })),
        )),
    }
}

fn parse_format(
    format_str: &str,
) -> Result<desktop_environment::CaptureFormat, (StatusCode, Json<serde_json::Value>)> {
    match format_str {
        "raw_argb" | "raw" => Ok(desktop_environment::CaptureFormat::RawArgb),
        "bmp" => Ok(desktop_environment::CaptureFormat::Bmp),
        "png" | "" => Ok(desktop_environment::CaptureFormat::Png),
        other => Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": format!("Unknown format '{}'; expected png, bmp, or raw_argb", other),
                "code": 400
            })),
        )),
    }
}

fn recording_error_response(
    e: &desktop_environment::RecordingError,
) -> (StatusCode, Json<serde_json::Value>) {
    let (status, code) = match e {
        desktop_environment::RecordingError::SecureModeActive => (StatusCode::FORBIDDEN, 403),
        desktop_environment::RecordingError::PermissionDenied(_) => (StatusCode::FORBIDDEN, 403),
        desktop_environment::RecordingError::SessionNotFound(_) => (StatusCode::NOT_FOUND, 404),
        desktop_environment::RecordingError::AlreadyRecording(_) => (StatusCode::CONFLICT, 409),
        desktop_environment::RecordingError::MaxFramesReached => {
            (StatusCode::UNPROCESSABLE_ENTITY, 422)
        }
        desktop_environment::RecordingError::MaxDurationReached => {
            (StatusCode::UNPROCESSABLE_ENTITY, 422)
        }
        desktop_environment::RecordingError::CaptureError(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, 500)
        }
        desktop_environment::RecordingError::NoFramesAvailable => (StatusCode::NOT_FOUND, 404),
    };
    (
        status,
        Json(serde_json::json!({"error": e.to_string(), "code": code})),
    )
}

/// POST /v1/screen/recording/start — start a recording session.
pub async fn recording_start_handler(
    State(state): State<ApiState>,
    Json(req): Json<StartRecordingRequest>,
) -> impl IntoResponse {
    let target = match parse_capture_target(&req.target) {
        Ok(t) => t,
        Err(e) => return e.into_response(),
    };
    let format = match parse_format(&req.format) {
        Ok(f) => f,
        Err(e) => return e.into_response(),
    };

    let config = desktop_environment::RecordingConfig {
        target,
        frame_interval_ms: req.frame_interval_ms.unwrap_or(100),
        max_frames: Some(req.max_frames.unwrap_or(600)),
        max_duration_secs: Some(req.max_duration_secs.unwrap_or(60)),
        format,
        agent_id: req.agent_id.clone(),
    };

    let compositor = state.compositor.read().await;
    let capture_mgr = state.screen_capture_manager.read().await;
    let recording_mgr = state.screen_recording_manager.read().await;

    match recording_mgr.start_recording(&compositor, &capture_mgr, config) {
        Ok(id) => (
            StatusCode::CREATED,
            Json(serde_json::json!({
                "status": "recording",
                "recording_id": id.to_string(),
            })),
        )
            .into_response(),
        Err(e) => recording_error_response(&e).into_response(),
    }
}

/// POST /v1/screen/recording/:id/frame — capture next frame.
pub async fn recording_frame_handler(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    use base64::Engine;

    let recording_id = match uuid::Uuid::parse_str(&id) {
        Ok(id) => id,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "Invalid recording_id", "code": 400})),
            )
                .into_response()
        }
    };

    let compositor = state.compositor.read().await;
    let capture_mgr = state.screen_capture_manager.read().await;
    let recording_mgr = state.screen_recording_manager.read().await;

    match recording_mgr.capture_frame(&compositor, &capture_mgr, recording_id) {
        Ok(frame) => {
            let data_b64 = base64::engine::general_purpose::STANDARD.encode(&frame.data);
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "sequence": frame.sequence,
                    "width": frame.width,
                    "height": frame.height,
                    "data_size": frame.data_size,
                    "captured_at": frame.captured_at.to_rfc3339(),
                    "data_base64": data_b64,
                })),
            )
                .into_response()
        }
        Err(e) => recording_error_response(&e).into_response(),
    }
}

/// POST /v1/screen/recording/:id/pause — pause recording.
pub async fn recording_pause_handler(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let recording_id = match uuid::Uuid::parse_str(&id) {
        Ok(id) => id,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "Invalid recording_id", "code": 400})),
            )
                .into_response()
        }
    };

    let recording_mgr = state.screen_recording_manager.read().await;
    match recording_mgr.pause_recording(recording_id) {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({"status": "paused", "recording_id": id})),
        )
            .into_response(),
        Err(e) => recording_error_response(&e).into_response(),
    }
}

/// POST /v1/screen/recording/:id/resume — resume recording.
pub async fn recording_resume_handler(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let recording_id = match uuid::Uuid::parse_str(&id) {
        Ok(id) => id,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "Invalid recording_id", "code": 400})),
            )
                .into_response()
        }
    };

    let recording_mgr = state.screen_recording_manager.read().await;
    match recording_mgr.resume_recording(recording_id) {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({"status": "recording", "recording_id": id})),
        )
            .into_response(),
        Err(e) => recording_error_response(&e).into_response(),
    }
}

/// POST /v1/screen/recording/:id/stop — stop recording.
pub async fn recording_stop_handler(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let recording_id = match uuid::Uuid::parse_str(&id) {
        Ok(id) => id,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "Invalid recording_id", "code": 400})),
            )
                .into_response()
        }
    };

    let recording_mgr = state.screen_recording_manager.read().await;
    match recording_mgr.stop_recording(recording_id) {
        Ok(session) => (StatusCode::OK, Json(serde_json::json!(session))).into_response(),
        Err(e) => recording_error_response(&e).into_response(),
    }
}

/// GET /v1/screen/recording/:id — get session metadata.
pub async fn recording_get_handler(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let recording_id = match uuid::Uuid::parse_str(&id) {
        Ok(id) => id,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "Invalid recording_id", "code": 400})),
            )
                .into_response()
        }
    };

    let recording_mgr = state.screen_recording_manager.read().await;
    match recording_mgr.get_session(recording_id) {
        Some(session) => (StatusCode::OK, Json(serde_json::json!(session))).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Recording session not found", "code": 404})),
        )
            .into_response(),
    }
}

/// GET /v1/screen/recording/:id/frames — poll frames for streaming.
pub async fn recording_frames_handler(
    State(state): State<ApiState>,
    Path(id): Path<String>,
    axum::extract::Query(query): axum::extract::Query<FramesQuery>,
) -> impl IntoResponse {
    use base64::Engine;

    let recording_id = match uuid::Uuid::parse_str(&id) {
        Ok(id) => id,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "Invalid recording_id", "code": 400})),
            )
                .into_response()
        }
    };

    let recording_mgr = state.screen_recording_manager.read().await;
    match recording_mgr.get_frames(recording_id, query.since) {
        Ok(frames) => {
            let encoded: Vec<serde_json::Value> = frames
                .iter()
                .map(|f| {
                    serde_json::json!({
                        "sequence": f.sequence,
                        "width": f.width,
                        "height": f.height,
                        "data_size": f.data_size,
                        "captured_at": f.captured_at.to_rfc3339(),
                        "data_base64": base64::engine::general_purpose::STANDARD.encode(&f.data),
                    })
                })
                .collect();
            (
                StatusCode::OK,
                Json(serde_json::json!({"frames": encoded, "count": encoded.len()})),
            )
                .into_response()
        }
        Err(e) => recording_error_response(&e).into_response(),
    }
}

/// GET /v1/screen/recording/:id/latest — get most recent frame.
pub async fn recording_latest_handler(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    use base64::Engine;

    let recording_id = match uuid::Uuid::parse_str(&id) {
        Ok(id) => id,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "Invalid recording_id", "code": 400})),
            )
                .into_response()
        }
    };

    let recording_mgr = state.screen_recording_manager.read().await;
    match recording_mgr.get_latest_frame(recording_id) {
        Ok(frame) => {
            let data_b64 = base64::engine::general_purpose::STANDARD.encode(&frame.data);
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "sequence": frame.sequence,
                    "width": frame.width,
                    "height": frame.height,
                    "data_size": frame.data_size,
                    "captured_at": frame.captured_at.to_rfc3339(),
                    "data_base64": data_b64,
                })),
            )
                .into_response()
        }
        Err(e) => recording_error_response(&e).into_response(),
    }
}

/// GET /v1/screen/recordings — list all recording sessions.
pub async fn recording_list_handler(State(state): State<ApiState>) -> impl IntoResponse {
    let recording_mgr = state.screen_recording_manager.read().await;
    let sessions = recording_mgr.list_sessions();
    Json(serde_json::json!({"recordings": sessions, "count": sessions.len()}))
}
