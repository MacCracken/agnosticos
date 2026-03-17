//! Credential Proxy — Secrets never enter sandbox memory
//!
//! A parent-process HTTP proxy that injects authentication headers into
//! outbound requests from sandboxed agents. The sandboxed process only
//! sees `http_proxy=http://127.0.0.1:{port}` — never raw API keys,
//! tokens, or credentials.
//!
//! Inspired by SecureYeoman's credential-proxy pattern.
//!
//! Architecture:
//!   1. Parent process starts proxy on an ephemeral localhost port
//!   2. Sandbox env gets `http_proxy` / `https_proxy` pointing to it
//!   3. Proxy allowlists outbound hosts, injects Authorization headers
//!   4. HTTPS traffic uses CONNECT tunneling (host-level allowlist only)
//!   5. Proxy logs all requests to audit trail

use std::collections::HashMap;
use std::net::SocketAddr;

use serde::{Deserialize, Serialize};
use tracing::{debug, info};

// ---------------------------------------------------------------------------
// Credential Rules
// ---------------------------------------------------------------------------

/// A rule that maps a host pattern to credentials to inject.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialRule {
    /// Host pattern (exact match or glob, e.g., "api.openai.com", "*.anthropic.com").
    pub host_pattern: String,
    /// HTTP header name to inject (e.g., "Authorization", "X-API-Key").
    pub header_name: String,
    /// Header value (e.g., "Bearer sk-..."). Stored encrypted at rest.
    #[serde(skip_serializing)]
    pub header_value: String,
    /// Optional description for audit logs.
    pub description: Option<String>,
}

/// Proxy configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialProxyConfig {
    /// Credential injection rules.
    pub rules: Vec<CredentialRule>,
    /// Allowed outbound hosts (requests to unlisted hosts get 403).
    /// Empty = allow all (credentials still injected for matching rules).
    pub allowed_hosts: Vec<String>,
    /// Whether to block requests to hosts not in the allow list.
    pub enforce_allowlist: bool,
    /// Maximum request body size in bytes (prevents exfiltration of large payloads).
    pub max_request_body_bytes: usize,
    /// Whether to log full request URLs (may leak path info).
    pub log_urls: bool,
    /// Bind address (default: 127.0.0.1:0 for ephemeral port).
    pub bind_addr: Option<String>,
}

impl Default for CredentialProxyConfig {
    fn default() -> Self {
        Self {
            rules: vec![],
            allowed_hosts: vec![],
            enforce_allowlist: true,
            max_request_body_bytes: 10 * 1024 * 1024, // 10 MB
            log_urls: false,
            bind_addr: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Proxy State
// ---------------------------------------------------------------------------

/// A running credential proxy instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialProxyHandle {
    /// Proxy listen address (127.0.0.1:{port}).
    pub listen_addr: SocketAddr,
    /// Environment variables to set in the sandboxed process.
    pub env_vars: HashMap<String, String>,
    /// Number of credential rules active.
    pub rule_count: usize,
    /// Number of allowed hosts.
    pub allowed_host_count: usize,
}

/// Audit log entry for a proxied request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyAuditEntry {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub agent_id: String,
    pub method: String,
    pub host: String,
    pub path: Option<String>,
    pub status: ProxyDecision,
    /// Which credential rule matched (if any).
    pub credential_injected: Option<String>,
    /// Response status code from upstream.
    pub upstream_status: Option<u16>,
}

/// Proxy decision for a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProxyDecision {
    /// Request allowed and forwarded.
    Allowed,
    /// Request allowed with credential injection.
    AllowedWithCredentials,
    /// Request blocked — host not in allowlist.
    BlockedHost,
    /// Request blocked — body too large.
    BlockedPayloadSize,
    /// Request blocked — CONNECT to disallowed host.
    BlockedConnect,
}

impl std::fmt::Display for ProxyDecision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Allowed => write!(f, "allowed"),
            Self::AllowedWithCredentials => write!(f, "allowed+creds"),
            Self::BlockedHost => write!(f, "blocked:host"),
            Self::BlockedPayloadSize => write!(f, "blocked:payload_size"),
            Self::BlockedConnect => write!(f, "blocked:connect"),
        }
    }
}

// ---------------------------------------------------------------------------
// Host matching
// ---------------------------------------------------------------------------

/// Check if a host matches a pattern (supports leading wildcard).
fn host_matches(host: &str, pattern: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if let Some(suffix) = pattern.strip_prefix("*.") {
        host == suffix || host.ends_with(&format!(".{}", suffix))
    } else {
        host == pattern
    }
}

/// Check if a host is in an allowlist.
fn host_allowed(host: &str, allowlist: &[String]) -> bool {
    if allowlist.is_empty() {
        return true; // empty = allow all
    }
    allowlist.iter().any(|p| host_matches(host, p))
}

/// Find the credential rule matching a host.
fn find_credential_rule<'a>(host: &str, rules: &'a [CredentialRule]) -> Option<&'a CredentialRule> {
    rules.iter().find(|r| host_matches(host, &r.host_pattern))
}

// ---------------------------------------------------------------------------
// Credential Proxy Manager
// ---------------------------------------------------------------------------

/// Manages credential proxy instances for sandboxed agents.
#[derive(Debug, Clone)]
pub struct CredentialProxyManager {
    config: CredentialProxyConfig,
    /// Active proxies: agent_id → handle.
    active_proxies: HashMap<String, CredentialProxyHandle>,
    /// Audit log (ring buffer).
    audit_log: Vec<ProxyAuditEntry>,
    max_audit_entries: usize,
}

impl CredentialProxyManager {
    pub fn new(config: CredentialProxyConfig) -> Self {
        Self {
            config,
            active_proxies: HashMap::new(),
            audit_log: Vec::new(),
            max_audit_entries: 10_000,
        }
    }

    /// Start a credential proxy for an agent's sandbox.
    ///
    /// Returns env vars to inject into the sandbox process.
    /// The actual TCP listener is started by the caller (via hyper/tokio).
    pub fn prepare_proxy(&mut self, agent_id: &str) -> CredentialProxyHandle {
        // In production, this would bind a TCP listener.
        // For now, return the configuration that the sandbox launcher uses.
        let port = 0u16; // ephemeral — real impl uses tokio::net::TcpListener
        let addr: SocketAddr = ([127, 0, 0, 1], port).into();

        let mut env_vars = HashMap::new();
        let proxy_url = format!("http://{}", addr);
        env_vars.insert("http_proxy".to_string(), proxy_url.clone());
        env_vars.insert("https_proxy".to_string(), proxy_url.clone());
        env_vars.insert("HTTP_PROXY".to_string(), proxy_url.clone());
        env_vars.insert("HTTPS_PROXY".to_string(), proxy_url);
        // Ensure localhost traffic bypasses the proxy
        env_vars.insert(
            "no_proxy".to_string(),
            "localhost,127.0.0.1,::1".to_string(),
        );

        let handle = CredentialProxyHandle {
            listen_addr: addr,
            env_vars,
            rule_count: self.config.rules.len(),
            allowed_host_count: self.config.allowed_hosts.len(),
        };

        self.active_proxies
            .insert(agent_id.to_string(), handle.clone());
        info!(
            agent_id = %agent_id,
            rules = self.config.rules.len(),
            "Credential proxy prepared"
        );

        handle
    }

    /// Evaluate a request from a sandboxed agent.
    /// Returns the decision and optionally the credential header name+value to inject.
    pub fn evaluate_request(
        &mut self,
        agent_id: &str,
        method: &str,
        host: &str,
        content_length: Option<usize>,
    ) -> (ProxyDecision, Option<(String, String)>) {
        // Check allowlist
        if self.config.enforce_allowlist && !host_allowed(host, &self.config.allowed_hosts) {
            self.record_audit(agent_id, method, host, ProxyDecision::BlockedHost, None);
            return (ProxyDecision::BlockedHost, None);
        }

        // Check payload size
        if let Some(size) = content_length {
            if size > self.config.max_request_body_bytes {
                self.record_audit(
                    agent_id,
                    method,
                    host,
                    ProxyDecision::BlockedPayloadSize,
                    None,
                );
                return (ProxyDecision::BlockedPayloadSize, None);
            }
        }

        // Find matching credential rule — clone results to release borrow
        let cred_match = find_credential_rule(host, &self.config.rules).map(|rule| {
            (
                rule.header_name.clone(),
                rule.header_value.clone(),
                rule.description.clone(),
            )
        });

        if let Some((header_name, header_value, desc)) = cred_match {
            self.record_audit(
                agent_id,
                method,
                host,
                ProxyDecision::AllowedWithCredentials,
                desc.as_deref(),
            );
            return (
                ProxyDecision::AllowedWithCredentials,
                Some((header_name, header_value)),
            );
        }

        self.record_audit(agent_id, method, host, ProxyDecision::Allowed, None);
        (ProxyDecision::Allowed, None)
    }

    /// Stop the credential proxy for an agent.
    pub fn stop_proxy(&mut self, agent_id: &str) {
        if self.active_proxies.remove(agent_id).is_some() {
            info!(agent_id = %agent_id, "Credential proxy stopped");
        }
    }

    /// Get audit log entries for an agent.
    pub fn audit_entries(&self, agent_id: &str) -> Vec<&ProxyAuditEntry> {
        self.audit_log
            .iter()
            .filter(|e| e.agent_id == agent_id)
            .collect()
    }

    /// Get all active proxy handles.
    pub fn active_proxies(&self) -> &HashMap<String, CredentialProxyHandle> {
        &self.active_proxies
    }

    fn record_audit(
        &mut self,
        agent_id: &str,
        method: &str,
        host: &str,
        status: ProxyDecision,
        credential_desc: Option<&str>,
    ) {
        let entry = ProxyAuditEntry {
            timestamp: chrono::Utc::now(),
            agent_id: agent_id.to_string(),
            method: method.to_string(),
            host: host.to_string(),
            path: None,
            status,
            credential_injected: credential_desc.map(|s| s.to_string()),
            upstream_status: None,
        };

        debug!(
            agent = %agent_id,
            host = %host,
            decision = %status,
            "Proxy request"
        );

        if self.audit_log.len() >= self.max_audit_entries {
            self.audit_log.remove(0);
        }
        self.audit_log.push(entry);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> CredentialProxyConfig {
        CredentialProxyConfig {
            rules: vec![
                CredentialRule {
                    host_pattern: "api.openai.com".to_string(),
                    header_name: "Authorization".to_string(),
                    header_value: "Bearer sk-test-key".to_string(),
                    description: Some("OpenAI API".to_string()),
                },
                CredentialRule {
                    host_pattern: "*.anthropic.com".to_string(),
                    header_name: "x-api-key".to_string(),
                    header_value: "sk-ant-test".to_string(),
                    description: Some("Anthropic API".to_string()),
                },
            ],
            allowed_hosts: vec![
                "api.openai.com".to_string(),
                "*.anthropic.com".to_string(),
                "localhost".to_string(),
            ],
            enforce_allowlist: true,
            max_request_body_bytes: 1024 * 1024,
            log_urls: false,
            bind_addr: None,
        }
    }

    #[test]
    fn test_host_matches_exact() {
        assert!(host_matches("api.openai.com", "api.openai.com"));
        assert!(!host_matches("evil.com", "api.openai.com"));
    }

    #[test]
    fn test_host_matches_wildcard() {
        assert!(host_matches("api.anthropic.com", "*.anthropic.com"));
        assert!(host_matches("anthropic.com", "*.anthropic.com"));
        assert!(!host_matches("evil.com", "*.anthropic.com"));
    }

    #[test]
    fn test_host_matches_star() {
        assert!(host_matches("anything.com", "*"));
    }

    #[test]
    fn test_evaluate_allowed_with_credentials() {
        let mut mgr = CredentialProxyManager::new(test_config());
        mgr.prepare_proxy("agent-1");

        let (decision, creds) = mgr.evaluate_request("agent-1", "GET", "api.openai.com", None);
        assert_eq!(decision, ProxyDecision::AllowedWithCredentials);
        let (name, value) = creds.unwrap();
        assert_eq!(name.as_str(), "Authorization");
        assert_eq!(value.as_str(), "Bearer sk-test-key");
    }

    #[test]
    fn test_evaluate_blocked_host() {
        let mut mgr = CredentialProxyManager::new(test_config());
        mgr.prepare_proxy("agent-1");

        let (decision, _) = mgr.evaluate_request("agent-1", "POST", "evil.com", None);
        assert_eq!(decision, ProxyDecision::BlockedHost);
    }

    #[test]
    fn test_evaluate_blocked_payload_size() {
        let mut mgr = CredentialProxyManager::new(test_config());
        mgr.prepare_proxy("agent-1");

        let (decision, _) =
            mgr.evaluate_request("agent-1", "POST", "api.openai.com", Some(10_000_000));
        assert_eq!(decision, ProxyDecision::BlockedPayloadSize);
    }

    #[test]
    fn test_evaluate_allowed_no_creds() {
        let mut mgr = CredentialProxyManager::new(test_config());
        mgr.prepare_proxy("agent-1");

        let (decision, creds) = mgr.evaluate_request("agent-1", "GET", "localhost", None);
        assert_eq!(decision, ProxyDecision::Allowed);
        assert!(creds.is_none());
    }

    #[test]
    fn test_wildcard_credential_injection() {
        let mut mgr = CredentialProxyManager::new(test_config());
        mgr.prepare_proxy("agent-1");

        let (decision, creds) = mgr.evaluate_request("agent-1", "GET", "api.anthropic.com", None);
        assert_eq!(decision, ProxyDecision::AllowedWithCredentials);
        let (name, _) = creds.unwrap();
        assert_eq!(name.as_str(), "x-api-key");
    }

    #[test]
    fn test_audit_log() {
        let mut mgr = CredentialProxyManager::new(test_config());
        mgr.prepare_proxy("agent-1");

        mgr.evaluate_request("agent-1", "GET", "api.openai.com", None);
        mgr.evaluate_request("agent-1", "GET", "evil.com", None);

        let entries = mgr.audit_entries("agent-1");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].status, ProxyDecision::AllowedWithCredentials);
        assert_eq!(entries[1].status, ProxyDecision::BlockedHost);
    }

    #[test]
    fn test_stop_proxy() {
        let mut mgr = CredentialProxyManager::new(test_config());
        mgr.prepare_proxy("agent-1");
        assert_eq!(mgr.active_proxies().len(), 1);

        mgr.stop_proxy("agent-1");
        assert_eq!(mgr.active_proxies().len(), 0);
    }

    #[test]
    fn test_empty_allowlist_permits_all() {
        let mut config = test_config();
        config.allowed_hosts = vec![];
        config.enforce_allowlist = true;
        let mut mgr = CredentialProxyManager::new(config);
        mgr.prepare_proxy("agent-1");

        let (decision, _) = mgr.evaluate_request("agent-1", "GET", "any-host.com", None);
        assert_eq!(decision, ProxyDecision::Allowed);
    }

    #[test]
    fn test_prepare_proxy_env_vars() {
        let mut mgr = CredentialProxyManager::new(test_config());
        let handle = mgr.prepare_proxy("agent-1");

        assert!(handle.env_vars.contains_key("http_proxy"));
        assert!(handle.env_vars.contains_key("https_proxy"));
        assert!(handle.env_vars.contains_key("no_proxy"));
        assert_eq!(handle.rule_count, 2);
        assert_eq!(handle.allowed_host_count, 3);
    }

    #[test]
    fn test_default_config() {
        let config = CredentialProxyConfig::default();
        assert!(config.rules.is_empty());
        assert!(config.allowed_hosts.is_empty());
        assert!(config.enforce_allowlist);
        assert_eq!(config.max_request_body_bytes, 10 * 1024 * 1024);
        assert!(!config.log_urls);
        assert!(config.bind_addr.is_none());
    }

    #[test]
    fn test_proxy_decision_display() {
        assert_eq!(format!("{}", ProxyDecision::Allowed), "allowed");
        assert_eq!(
            format!("{}", ProxyDecision::AllowedWithCredentials),
            "allowed+creds"
        );
        assert_eq!(format!("{}", ProxyDecision::BlockedHost), "blocked:host");
        assert_eq!(
            format!("{}", ProxyDecision::BlockedPayloadSize),
            "blocked:payload_size"
        );
        assert_eq!(
            format!("{}", ProxyDecision::BlockedConnect),
            "blocked:connect"
        );
    }

    #[test]
    fn test_audit_log_ring_buffer() {
        let config = CredentialProxyConfig {
            allowed_hosts: vec!["*".to_string()],
            enforce_allowlist: true,
            ..Default::default()
        };
        let mut mgr = CredentialProxyManager {
            config,
            active_proxies: HashMap::new(),
            audit_log: Vec::new(),
            max_audit_entries: 3,
        };
        mgr.prepare_proxy("agent-1");
        mgr.evaluate_request("agent-1", "GET", "a.com", None);
        mgr.evaluate_request("agent-1", "GET", "b.com", None);
        mgr.evaluate_request("agent-1", "GET", "c.com", None);
        mgr.evaluate_request("agent-1", "GET", "d.com", None);
        // Ring buffer should have evicted the oldest
        assert_eq!(mgr.audit_log.len(), 3);
        assert_eq!(mgr.audit_log[0].host, "b.com");
    }
}
