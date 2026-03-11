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
        // Prevent SSRF: reject private IPs, non-http(s) schemes, credentials,
        // and localhost targets using the shared URL validator.
        if let Err(reason) = crate::http_api::types::validate_url_no_ssrf(&url) {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": format!("Invalid update URL: {reason}")
                })),
            );
        }

        // Additionally restrict to HTTPS and known official update hosts.
        let allowed_hosts: &[&str] = &["updates.agnos.org", "releases.agnos.org"];
        let parsed = url::Url::parse(&url).expect("already validated");
        if parsed.scheme() != "https" {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "Only HTTPS URLs are permitted for system updates"
                })),
            );
        }
        let host = parsed.host_str().unwrap_or("");
        if !allowed_hosts.contains(&host) {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": format!(
                        "Disallowed update host '{host}'. Only official update servers are permitted: {}",
                        allowed_hosts.join(", ")
                    )
                })),
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

#[cfg(test)]
mod tests {
    use crate::http_api::types::validate_url_no_ssrf;

    #[test]
    fn update_url_rejects_private_ip() {
        assert!(validate_url_no_ssrf("https://10.0.0.1/update").is_err());
        assert!(validate_url_no_ssrf("https://192.168.1.1/update").is_err());
        assert!(validate_url_no_ssrf("https://172.16.0.1/update").is_err());
        assert!(validate_url_no_ssrf("https://127.0.0.1/update").is_err());
    }

    #[test]
    fn update_url_rejects_localhost() {
        assert!(validate_url_no_ssrf("https://localhost/update").is_err());
        assert!(validate_url_no_ssrf("http://localhost:8090/v1/health").is_err());
    }

    #[test]
    fn update_url_rejects_non_https() {
        assert!(validate_url_no_ssrf("ftp://updates.agnos.org/latest").is_err());
        assert!(validate_url_no_ssrf("file:///etc/passwd").is_err());
    }

    #[test]
    fn update_url_rejects_credentials() {
        assert!(validate_url_no_ssrf("https://user:pass@updates.agnos.org/latest").is_err());
    }

    #[test]
    fn update_url_rejects_ipv6_loopback() {
        assert!(validate_url_no_ssrf("https://[::1]/update").is_err());
    }

    #[test]
    fn update_url_rejects_link_local() {
        assert!(validate_url_no_ssrf("https://169.254.1.1/update").is_err());
    }

    #[test]
    fn update_url_accepts_valid_public_url() {
        assert!(validate_url_no_ssrf("https://updates.agnos.org/v1/latest").is_ok());
        assert!(validate_url_no_ssrf("https://releases.agnos.org/manifest.json").is_ok());
    }
}
