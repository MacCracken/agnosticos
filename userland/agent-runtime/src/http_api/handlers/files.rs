//! File transfer API handlers for sutra orchestration.
//!
//! Provides `PUT /v1/agents/{id}/files/*path` and `GET /v1/agents/{id}/files/*path`
//! endpoints for reading and writing files within an agent's Landlock data directory.
//!
//! Security:
//! - Strict path traversal protection (no `..`, no absolute paths, no symlink following)
//! - Files scoped to `/var/lib/agnos/agents/{agent_id}/`
//! - 10 MB size limit on PUT bodies
//! - Audit logging for all file operations

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use tracing::info;

use crate::http_api::state::ApiState;

/// Base directory for agent data files.
const AGENT_DATA_BASE: &str = "/var/lib/agnos/agents";

/// Maximum file size for PUT operations (10 MB).
const MAX_FILE_SIZE: usize = 10 * 1024 * 1024;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Validate that an agent ID exists in the registry.
async fn require_registered_agent(state: &ApiState, id: &str) -> Option<axum::response::Response> {
    if let Ok(uuid) = id.parse::<::uuid::Uuid>() {
        let agents = state.agents_read().await;
        if agents.contains_key(&uuid) {
            return None; // Agent exists
        }
    }
    Some(
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Agent not found", "code": 404})),
        )
            .into_response(),
    )
}

/// Validate a relative file path for safety.
///
/// Rejects:
/// - Empty paths
/// - Paths containing `..` components (traversal)
/// - Absolute paths (starting with `/`)
/// - Paths containing null bytes
///
/// Returns the sanitised path on success, or an error response.
fn validate_file_path(path: &str) -> Result<std::path::PathBuf, axum::response::Response> {
    // Reject empty
    if path.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "File path must not be empty", "code": 400})),
        )
            .into_response());
    }

    // Reject null bytes
    if path.contains('\0') {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "File path contains null bytes", "code": 400})),
        )
            .into_response());
    }

    // Reject absolute paths
    if path.starts_with('/') {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Absolute paths are not allowed", "code": 400})),
        )
            .into_response());
    }

    let rel = std::path::Path::new(path);

    // Reject any component that is `..`
    for component in rel.components() {
        match component {
            std::path::Component::ParentDir => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "error": "Path traversal not allowed",
                        "code": 400
                    })),
                )
                    .into_response());
            }
            std::path::Component::RootDir | std::path::Component::Prefix(_) => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "error": "Absolute paths are not allowed",
                        "code": 400
                    })),
                )
                    .into_response());
            }
            _ => {}
        }
    }

    Ok(rel.to_path_buf())
}

/// Resolve the full filesystem path for an agent file, returning the canonical
/// agent data directory and the full target path. Does NOT follow symlinks.
fn resolve_agent_file_path(agent_id: &str, file_path: &std::path::Path) -> std::path::PathBuf {
    let base = std::path::PathBuf::from(AGENT_DATA_BASE);
    base.join(agent_id).join(file_path)
}

/// Guess a content-type from the file extension.
fn guess_content_type(path: &std::path::Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("json") => "application/json",
        Some("toml") => "application/toml",
        Some("yaml" | "yml") => "application/yaml",
        Some("txt" | "log" | "md") => "text/plain; charset=utf-8",
        Some("html" | "htm") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js") => "application/javascript",
        Some("xml") => "application/xml",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("svg") => "image/svg+xml",
        Some("pdf") => "application/pdf",
        Some("tar") => "application/x-tar",
        Some("gz") => "application/gzip",
        Some("zip") => "application/zip",
        Some("wasm") => "application/wasm",
        _ => "application/octet-stream",
    }
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// `PUT /v1/agents/:id/files/*path` — write file content to the agent's data directory.
pub async fn file_put_handler(
    State(state): State<ApiState>,
    Path((id, file_path)): Path<(String, String)>,
    body: Bytes,
) -> impl IntoResponse {
    // Validate agent exists
    if let Some(err) = require_registered_agent(&state, &id).await {
        return err;
    }

    // Validate path safety
    let rel_path = match validate_file_path(&file_path) {
        Ok(p) => p,
        Err(resp) => return resp,
    };

    // Enforce size limit
    if body.len() > MAX_FILE_SIZE {
        return (
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(serde_json::json!({
                "error": format!(
                    "File too large: {} bytes exceeds {} byte limit",
                    body.len(),
                    MAX_FILE_SIZE
                ),
                "code": 413
            })),
        )
            .into_response();
    }

    let full_path = resolve_agent_file_path(&id, &rel_path);

    // Verify the resolved path is still under the agent data dir (defense in depth)
    let agent_dir = std::path::PathBuf::from(AGENT_DATA_BASE).join(&id);
    if !full_path.starts_with(&agent_dir) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Path escapes agent data directory", "code": 400})),
        )
            .into_response();
    }

    // Reject if target is a symlink (no symlink following)
    if full_path.is_symlink() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Symlinks are not allowed", "code": 400})),
        )
            .into_response();
    }

    // Create parent directories
    if let Some(parent) = full_path.parent() {
        if let Err(e) = tokio::fs::create_dir_all(parent).await {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("Failed to create directories: {}", e),
                    "code": 500
                })),
            )
                .into_response();
        }
    }

    // Write file
    let size = body.len();
    match tokio::fs::write(&full_path, &body).await {
        Ok(()) => {
            info!(
                agent_id = %id,
                path = %file_path,
                size = size,
                "File written via transfer API"
            );
            // Audit log
            let audit_event = crate::http_api::handlers::audit::AuditEvent {
                timestamp: chrono::Utc::now().to_rfc3339(),
                action: "file_write".to_string(),
                agent: Some(id.clone()),
                details: serde_json::json!({
                    "path": file_path,
                    "size": size,
                }),
                outcome: "success".to_string(),
            };
            state.push_audit_event(audit_event).await;

            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "written": true,
                    "path": file_path,
                    "size": size
                })),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": format!("Failed to write file: {}", e),
                "code": 500
            })),
        )
            .into_response(),
    }
}

/// `GET /v1/agents/:id/files/*path` — read file content from the agent's data directory.
pub async fn file_get_handler(
    State(state): State<ApiState>,
    Path((id, file_path)): Path<(String, String)>,
) -> impl IntoResponse {
    // Validate agent exists
    if let Some(err) = require_registered_agent(&state, &id).await {
        return err;
    }

    // Validate path safety
    let rel_path = match validate_file_path(&file_path) {
        Ok(p) => p,
        Err(resp) => return resp,
    };

    let full_path = resolve_agent_file_path(&id, &rel_path);

    // Verify the resolved path is still under the agent data dir (defense in depth)
    let agent_dir = std::path::PathBuf::from(AGENT_DATA_BASE).join(&id);
    if !full_path.starts_with(&agent_dir) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Path escapes agent data directory", "code": 400})),
        )
            .into_response();
    }

    // Reject if target is a symlink (no symlink following)
    if full_path.is_symlink() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Symlinks are not allowed", "code": 400})),
        )
            .into_response();
    }

    // Read file
    match tokio::fs::read(&full_path).await {
        Ok(data) => {
            info!(
                agent_id = %id,
                path = %file_path,
                size = data.len(),
                "File read via transfer API"
            );
            // Audit log
            let audit_event = crate::http_api::handlers::audit::AuditEvent {
                timestamp: chrono::Utc::now().to_rfc3339(),
                action: "file_read".to_string(),
                agent: Some(id.clone()),
                details: serde_json::json!({
                    "path": file_path,
                    "size": data.len(),
                }),
                outcome: "success".to_string(),
            };
            state.push_audit_event(audit_event).await;

            let content_type = guess_content_type(&rel_path);
            (StatusCode::OK, [(header::CONTENT_TYPE, content_type)], data).into_response()
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": format!("File not found: {}", file_path),
                "code": 404
            })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": format!("Failed to read file: {}", e),
                "code": 500
            })),
        )
            .into_response(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- Path validation unit tests ---

    #[test]
    fn test_validate_path_normal() {
        assert!(validate_file_path("config/app.toml").is_ok());
        assert!(validate_file_path("data.json").is_ok());
        assert!(validate_file_path("a/b/c/d.txt").is_ok());
    }

    #[test]
    fn test_validate_path_traversal_rejected() {
        assert!(validate_file_path("../etc/passwd").is_err());
        assert!(validate_file_path("foo/../../bar").is_err());
        assert!(validate_file_path("..").is_err());
    }

    #[test]
    fn test_validate_path_absolute_rejected() {
        assert!(validate_file_path("/etc/passwd").is_err());
        assert!(validate_file_path("/tmp/test").is_err());
    }

    #[test]
    fn test_validate_path_empty_rejected() {
        assert!(validate_file_path("").is_err());
    }

    #[test]
    fn test_validate_path_null_byte_rejected() {
        assert!(validate_file_path("foo\0bar").is_err());
    }

    #[test]
    fn test_guess_content_type() {
        assert_eq!(
            guess_content_type(std::path::Path::new("a.json")),
            "application/json"
        );
        assert_eq!(
            guess_content_type(std::path::Path::new("a.toml")),
            "application/toml"
        );
        assert_eq!(
            guess_content_type(std::path::Path::new("a.txt")),
            "text/plain; charset=utf-8"
        );
        assert_eq!(
            guess_content_type(std::path::Path::new("a.png")),
            "image/png"
        );
        assert_eq!(
            guess_content_type(std::path::Path::new("a.unknown")),
            "application/octet-stream"
        );
        assert_eq!(
            guess_content_type(std::path::Path::new("noext")),
            "application/octet-stream"
        );
    }

    #[test]
    fn test_resolve_agent_file_path() {
        let p = resolve_agent_file_path("abc-123", std::path::Path::new("config/app.toml"));
        assert_eq!(
            p,
            std::path::PathBuf::from("/var/lib/agnos/agents/abc-123/config/app.toml")
        );
    }
}
