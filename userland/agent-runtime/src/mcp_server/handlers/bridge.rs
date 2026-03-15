//! Generic HTTP bridge for proxying MCP tool calls to consumer service APIs.
//!
//! All consumer project bridges (Synapse, BullShift, Delta, etc.) share
//! identical HTTP client logic. This module extracts that into a reusable
//! struct to eliminate ~700 lines of boilerplate across 11 handler files.

/// Generic HTTP bridge that proxies MCP tool calls to a consumer service.
///
/// Constructed from environment variable names for URL and API key.
/// Falls back to a default localhost URL when the env var is unset.
/// SSRF protection: rejects non-HTTP schemes and cloud metadata IPs.
#[derive(Debug, Clone)]
pub struct HttpBridge {
    pub(crate) base_url: String,
    api_key: Option<String>,
    service_name: &'static str,
    client: reqwest::Client,
}

/// Validate a bridge URL is safe (no SSRF vectors).
///
/// Rejects:
/// - Non-HTTP(S) schemes (file://, ftp://, etc.)
/// - Cloud metadata IPs (169.254.x.x, fd00::)
/// - Link-local addresses
fn validate_bridge_url(url: &str) -> bool {
    // Must start with http:// or https://
    if !url.starts_with("http://") && !url.starts_with("https://") {
        tracing::warn!(url = %url, "MCP bridge: rejecting non-HTTP URL");
        return false;
    }

    // Extract host portion
    let host_part = url
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .split('/')
        .next()
        .unwrap_or("")
        .split(':')
        .next()
        .unwrap_or("");

    // Reject cloud metadata / link-local IPs
    if host_part.starts_with("169.254.")
        || host_part == "metadata.google.internal"
        || host_part.starts_with("fd00:")
        || host_part.starts_with("fe80:")
    {
        tracing::warn!(url = %url, "MCP bridge: rejecting cloud metadata / link-local URL");
        return false;
    }

    true
}

impl HttpBridge {
    /// Create a new bridge from environment variables.
    ///
    /// - `url_var`: env var name for the base URL (e.g., `"SYNAPSE_URL"`)
    /// - `default_url`: fallback URL (e.g., `"http://127.0.0.1:8080"`)
    /// - `key_var`: env var name for the API key (e.g., `"SYNAPSE_API_KEY"`)
    /// - `service_name`: human-readable name for log messages (e.g., `"Synapse"`)
    ///
    /// If the env var URL fails SSRF validation, falls back to `default_url`.
    pub fn new(
        url_var: &str,
        default_url: &str,
        key_var: &str,
        service_name: &'static str,
    ) -> Self {
        let base_url = match std::env::var(url_var) {
            Ok(url) if validate_bridge_url(&url) => url,
            Ok(url) => {
                tracing::warn!(
                    service = service_name,
                    url = %url,
                    "MCP bridge: env var URL failed SSRF validation, using default"
                );
                default_url.to_string()
            }
            Err(_) => default_url.to_string(),
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .connect_timeout(std::time::Duration::from_secs(2))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            base_url,
            api_key: std::env::var(key_var).ok(),
            service_name,
            client,
        }
    }

    pub async fn get(
        &self,
        path: &str,
        query: &[(String, String)],
    ) -> Result<serde_json::Value, String> {
        let client = &self.client;
        let url = format!("{}{}", self.base_url, path);
        let mut req = client.get(&url).query(query);
        if let Some(ref key) = self.api_key {
            req = req.header("Authorization", format!("Bearer {}", key));
        }
        let resp = req.send().await.map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!(
                "{} API error: {}",
                self.service_name,
                resp.status()
            ));
        }
        resp.json().await.map_err(|e| e.to_string())
    }

    pub async fn post(
        &self,
        path: &str,
        body: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let client = &self.client;
        let url = format!("{}{}", self.base_url, path);
        let mut req = client.post(&url).json(&body);
        if let Some(ref key) = self.api_key {
            req = req.header("Authorization", format!("Bearer {}", key));
        }
        let resp = req.send().await.map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!(
                "{} API error: {}",
                self.service_name,
                resp.status()
            ));
        }
        resp.json().await.map_err(|e| e.to_string())
    }
}
