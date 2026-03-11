use std::collections::HashMap;
use std::fmt;

use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Error response helpers — eliminates repeated json!({"error":..., "code":...})
// ---------------------------------------------------------------------------

/// Build a standard error response with the given HTTP status code and message.
pub fn error_response(status: StatusCode, msg: impl fmt::Display) -> impl IntoResponse {
    (
        status,
        Json(serde_json::json!({
            "error": msg.to_string(),
            "code": status.as_u16(),
        })),
    )
        .into_response()
}

/// 400 Bad Request convenience helper.
pub fn bad_request(msg: impl fmt::Display) -> impl IntoResponse {
    error_response(StatusCode::BAD_REQUEST, msg)
}

/// 404 Not Found convenience helper.
pub fn not_found(msg: impl fmt::Display) -> impl IntoResponse {
    error_response(StatusCode::NOT_FOUND, msg)
}

/// 409 Conflict convenience helper.
pub fn conflict(msg: impl fmt::Display) -> impl IntoResponse {
    error_response(StatusCode::CONFLICT, msg)
}

/// 500 Internal Server Error convenience helper.
pub fn internal_error(msg: impl fmt::Display) -> impl IntoResponse {
    error_response(StatusCode::INTERNAL_SERVER_ERROR, msg)
}

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterAgentRequest {
    pub name: String,
    /// Optional client-specified UUID. If provided and not already taken, it will
    /// be used as the agent's ID; otherwise a new UUID is generated server-side.
    #[serde(default)]
    pub id: Option<Uuid>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub resource_needs: ResourceNeeds,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

/// Request to deregister multiple agents in a single call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchDeregisterRequest {
    /// Deregister by source identifier (matches metadata "source" field).
    #[serde(default)]
    pub source: Option<String>,
    /// Deregister by explicit list of UUIDs.
    #[serde(default)]
    pub ids: Option<Vec<Uuid>>,
}

/// Result of a single agent deregistration within a batch.
#[derive(Debug, Serialize)]
pub struct BatchDeregisterResult {
    pub id: Uuid,
    pub name: String,
    pub status: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResourceNeeds {
    #[serde(default)]
    pub min_memory_mb: u64,
    #[serde(default)]
    pub min_cpu_shares: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterAgentResponse {
    pub id: Uuid,
    pub name: String,
    pub status: String,
    pub registered_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatRequest {
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub current_task: Option<String>,
    #[serde(default)]
    pub cpu_percent: Option<f32>,
    #[serde(default)]
    pub memory_mb: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDetail {
    pub id: Uuid,
    pub name: String,
    pub status: String,
    pub capabilities: Vec<String>,
    pub resource_needs: ResourceNeeds,
    pub metadata: HashMap<String, String>,
    pub registered_at: DateTime<Utc>,
    pub last_heartbeat: Option<DateTime<Utc>>,
    pub current_task: Option<String>,
    pub cpu_percent: Option<f32>,
    pub memory_mb: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentListResponse {
    pub agents: Vec<AgentDetail>,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub service: String,
    pub version: String,
    pub agents_registered: usize,
    pub uptime_seconds: u64,
    #[serde(default)]
    pub components: HashMap<String, ComponentHealth>,
    #[serde(default)]
    pub system: Option<SystemHealth>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentHealth {
    pub status: String,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemHealth {
    pub hostname: String,
    pub load_average: [f64; 3],
    pub memory_total_mb: u64,
    pub memory_available_mb: u64,
    pub disk_free_mb: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
    pub code: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagIngestRequest {
    pub text: String,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
    /// Agent ID for per-agent rate limiting. Defaults to "anonymous".
    #[serde(default)]
    pub agent_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagQueryRequest {
    pub query: String,
    #[serde(default = "default_top_k")]
    pub top_k: usize,
}

pub(crate) fn default_top_k() -> usize {
    5
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeSearchRequest {
    pub query: String,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

pub(crate) fn default_limit() -> usize {
    10
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeIndexRequest {
    pub path: String,
    #[serde(default)]
    pub source: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMetricsResponse {
    pub total_agents: usize,
    pub agents_by_status: HashMap<String, usize>,
    pub uptime_seconds: u64,
    pub avg_cpu_percent: Option<f32>,
    pub total_memory_mb: u64,
}

// ---------------------------------------------------------------------------
// SSRF URL validation
// ---------------------------------------------------------------------------

/// Validate a URL to prevent Server-Side Request Forgery (SSRF) attacks.
///
/// Rejects:
/// - Non-HTTP(S) schemes
/// - URLs containing userinfo (credentials)
/// - Private/internal IP addresses and ranges:
///   - 127.0.0.0/8, 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16, 169.254.0.0/16
///   - ::1, fc00::/7, fe80::/10
/// - Localhost hostnames
///
/// Returns `Ok(())` if the URL is safe, or `Err(reason)` with a human-readable
/// message describing why the URL was rejected.
pub fn validate_url_no_ssrf(url_str: &str) -> Result<(), String> {
    let parsed = url::Url::parse(url_str).map_err(|e| format!("Invalid URL: {e}"))?;

    // Scheme must be http or https
    match parsed.scheme() {
        "http" | "https" => {}
        other => {
            return Err(format!(
                "Disallowed URL scheme '{other}'; only http and https are permitted"
            ))
        }
    }

    // Reject URLs with embedded credentials (user:pass@host)
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("URLs with embedded credentials are not permitted".to_string());
    }

    // Must have a host
    let host = parsed
        .host_str()
        .ok_or_else(|| "URL has no host".to_string())?;

    // Reject localhost variants
    let host_lower = host.to_lowercase();
    if host_lower == "localhost"
        || host_lower == "localhost."
        || host_lower.ends_with(".localhost")
        || host_lower.ends_with(".localhost.")
    {
        return Err("URLs targeting localhost are not permitted".to_string());
    }

    // Check for IP-based hosts
    match parsed.host() {
        Some(url::Host::Ipv4(ip)) => {
            if is_private_ipv4(ip) {
                return Err(format!(
                    "URL targets a private/internal IPv4 address ({ip})"
                ));
            }
        }
        Some(url::Host::Ipv6(ip)) => {
            if is_private_ipv6(ip) {
                return Err(format!(
                    "URL targets a private/internal IPv6 address ({ip})"
                ));
            }
        }
        _ => {
            // Domain name — could resolve to a private IP at runtime, but
            // DNS-rebinding protection is out of scope for this layer.
            // We already blocked localhost above.
        }
    }

    Ok(())
}

/// Returns `true` if the IPv4 address is in a private, loopback, or
/// link-local range that should not be reachable from SSRF contexts.
fn is_private_ipv4(ip: std::net::Ipv4Addr) -> bool {
    ip.is_loopback()          // 127.0.0.0/8
        || ip.is_private()    // 10/8, 172.16/12, 192.168/16
        || ip.is_link_local() // 169.254.0.0/16
        || ip.is_broadcast()  // 255.255.255.255
        || ip.is_unspecified() // 0.0.0.0
}

/// Returns `true` if the IPv6 address is loopback, link-local, or in the
/// unique-local range (fc00::/7).
fn is_private_ipv6(ip: std::net::Ipv6Addr) -> bool {
    if ip.is_loopback() {
        return true; // ::1
    }
    // Unique local addresses: fc00::/7 (first byte 0xfc or 0xfd)
    let octets = ip.octets();
    if octets[0] == 0xfc || octets[0] == 0xfd {
        return true;
    }
    // Link-local: fe80::/10
    if octets[0] == 0xfe && (octets[1] & 0xc0) == 0x80 {
        return true;
    }
    // Unspecified (::)
    if ip.is_unspecified() {
        return true;
    }
    false
}

#[cfg(test)]
mod url_validation_tests {
    use super::*;

    #[test]
    fn accepts_valid_https_url() {
        assert!(validate_url_no_ssrf("https://example.com/path").is_ok());
    }

    #[test]
    fn accepts_valid_http_url() {
        assert!(validate_url_no_ssrf("http://example.com:8080/api").is_ok());
    }

    #[test]
    fn rejects_non_http_scheme() {
        let err = validate_url_no_ssrf("ftp://example.com/file").unwrap_err();
        assert!(err.contains("scheme"), "got: {err}");
    }

    #[test]
    fn rejects_file_scheme() {
        let err = validate_url_no_ssrf("file:///etc/passwd").unwrap_err();
        assert!(err.contains("scheme"), "got: {err}");
    }

    #[test]
    fn rejects_credentials_in_url() {
        let err = validate_url_no_ssrf("https://user:pass@example.com").unwrap_err();
        assert!(err.contains("credentials"), "got: {err}");
    }

    #[test]
    fn rejects_username_only_in_url() {
        let err = validate_url_no_ssrf("https://admin@example.com").unwrap_err();
        assert!(err.contains("credentials"), "got: {err}");
    }

    #[test]
    fn rejects_localhost() {
        let err = validate_url_no_ssrf("http://localhost/secret").unwrap_err();
        assert!(err.contains("localhost"), "got: {err}");
    }

    #[test]
    fn rejects_localhost_with_port() {
        let err = validate_url_no_ssrf("http://localhost:8090/v1/health").unwrap_err();
        assert!(err.contains("localhost"), "got: {err}");
    }

    #[test]
    fn rejects_loopback_ipv4() {
        let err = validate_url_no_ssrf("http://127.0.0.1/secret").unwrap_err();
        assert!(err.contains("private"), "got: {err}");
    }

    #[test]
    fn rejects_loopback_ipv4_high() {
        let err = validate_url_no_ssrf("http://127.255.0.1/").unwrap_err();
        assert!(err.contains("private"), "got: {err}");
    }

    #[test]
    fn rejects_private_10_range() {
        let err = validate_url_no_ssrf("http://10.0.0.1/admin").unwrap_err();
        assert!(err.contains("private"), "got: {err}");
    }

    #[test]
    fn rejects_private_172_range() {
        let err = validate_url_no_ssrf("http://172.16.0.1/").unwrap_err();
        assert!(err.contains("private"), "got: {err}");
    }

    #[test]
    fn rejects_private_192_168_range() {
        let err = validate_url_no_ssrf("http://192.168.1.1/").unwrap_err();
        assert!(err.contains("private"), "got: {err}");
    }

    #[test]
    fn rejects_link_local_ipv4() {
        let err = validate_url_no_ssrf("http://169.254.1.1/").unwrap_err();
        assert!(err.contains("private"), "got: {err}");
    }

    #[test]
    fn rejects_ipv6_loopback() {
        let err = validate_url_no_ssrf("http://[::1]/secret").unwrap_err();
        assert!(err.contains("private"), "got: {err}");
    }

    #[test]
    fn rejects_ipv6_unique_local() {
        let err = validate_url_no_ssrf("http://[fd12::1]/").unwrap_err();
        assert!(err.contains("private"), "got: {err}");
    }

    #[test]
    fn rejects_ipv6_link_local() {
        let err = validate_url_no_ssrf("http://[fe80::1]/").unwrap_err();
        assert!(err.contains("private"), "got: {err}");
    }

    #[test]
    fn rejects_invalid_url() {
        assert!(validate_url_no_ssrf("not a url at all").is_err());
    }

    #[test]
    fn accepts_public_ip() {
        assert!(validate_url_no_ssrf("https://8.8.8.8/dns").is_ok());
    }

    #[test]
    fn accepts_public_ipv6() {
        assert!(validate_url_no_ssrf("https://[2607:f8b0:4004:800::200e]/").is_ok());
    }
}
