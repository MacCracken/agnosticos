use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;

// ---------------------------------------------------------------------------
// System update types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct SystemUpdateCheckRequest {
    /// Override the update URL (optional).
    update_url: Option<String>,
}

// ---------------------------------------------------------------------------
// System update handlers
// ---------------------------------------------------------------------------

/// GET /v1/system/update/status — current update state
pub async fn system_update_status_handler() -> impl IntoResponse {
    let config = agnos_sys::update::UpdateConfig::default();
    match agnos_sys::update::get_update_state(&config) {
        Ok(state) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "current_slot": format!("{}", state.current_slot),
                "current_version": state.current_version,
                "pending_update": state.pending_update,
                "last_update": state.last_update,
                "rollback_available": state.rollback_available,
                "boot_count_since_update": state.boot_count_since_update,
            })),
        ),
        Err(_) => {
            // No state file yet — return defaults
            let version = env!("CARGO_PKG_VERSION");
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "current_slot": "A",
                    "current_version": version,
                    "pending_update": null,
                    "last_update": null,
                    "rollback_available": false,
                    "boot_count_since_update": 0,
                })),
            )
        }
    }
}

/// POST /v1/system/update/check — check for available updates
pub async fn system_update_check_handler(
    Json(req): Json<SystemUpdateCheckRequest>,
) -> impl IntoResponse {
    let mut config = agnos_sys::update::UpdateConfig::default();
    if let Some(url) = req.update_url {
        // Prevent SSRF: only allow HTTPS URLs to known update server domains.
        // Extract host by stripping "https://" prefix and taking up to next '/' or end.
        let allowed_hosts: &[&str] = &["updates.agnos.org", "releases.agnos.org"];
        let is_allowed = url.strip_prefix("https://").is_some_and(|rest| {
            let host = rest.split('/').next().unwrap_or("");
            // Reject URLs with userinfo (user:pass@host), port, or empty host
            !host.contains('@') && !host.contains(':') && allowed_hosts.contains(&host)
        });
        if !is_allowed {
            return (
                StatusCode::BAD_REQUEST,
                Json(
                    serde_json::json!({"error": "Invalid or disallowed update URL. Only HTTPS URLs to official update servers are permitted."}),
                ),
            );
        }
        config.update_url = url;
    }
    let current_version = env!("CARGO_PKG_VERSION");

    match agnos_sys::update::check_for_update(&config, current_version) {
        Ok(Some(manifest)) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "update_available": true,
                "version": manifest.version,
                "channel": format!("{}", manifest.channel),
                "release_date": manifest.release_date,
                "changelog": manifest.changelog,
                "files": manifest.files.len(),
            })),
        ),
        Ok(None) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "update_available": false,
                "current_version": current_version,
            })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": format!("{e}"),
            })),
        ),
    }
}

/// POST /v1/system/update/apply — apply an update from a manifest
pub async fn system_update_apply_handler(
    Json(manifest_json): Json<serde_json::Value>,
) -> impl IntoResponse {
    let manifest_str = manifest_json.to_string();
    let manifest = match agnos_sys::update::parse_update_manifest(&manifest_str) {
        Ok(m) => m,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": format!("Invalid manifest: {e}")})),
            )
        }
    };

    if let Err(e) = agnos_sys::update::verify_manifest(&manifest) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": format!("Manifest verification failed: {e}")})),
        );
    }

    let config = agnos_sys::update::UpdateConfig::default();
    match agnos_sys::update::apply_update(&config, &manifest) {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "applied",
                "version": manifest.version,
                "message": "Update applied. Reboot to activate.",
            })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Apply failed: {e}")})),
        ),
    }
}

/// POST /v1/system/update/rollback — rollback to previous slot
pub async fn system_update_rollback_handler() -> impl IntoResponse {
    let config = agnos_sys::update::UpdateConfig::default();
    match agnos_sys::update::rollback(&config) {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "rolled_back",
                "message": "Rolled back to previous slot. Reboot to activate.",
            })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Rollback failed: {e}")})),
        ),
    }
}

/// POST /v1/system/update/confirm — mark current boot as successful
pub async fn system_update_confirm_handler() -> impl IntoResponse {
    let config = agnos_sys::update::UpdateConfig::default();
    match agnos_sys::update::mark_boot_successful(&config) {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "confirmed",
                "message": "Current boot marked as successful.",
            })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Confirm failed: {e}")})),
        ),
    }
}
