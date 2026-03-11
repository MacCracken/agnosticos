//! AI-integrated WebView — embedded browser with hoosh LLM bridge
//!
//! Provides a WebView component backed by wry/tao that integrates with
//! the hoosh LLM gateway for AI-assisted browsing, content extraction,
//! and agent-driven web interaction.
//!
//! Architecture:
//!   - `WebViewManager` manages multiple named WebView instances
//!   - Each instance has a hoosh bridge for LLM-powered features
//!   - IPC via custom protocol (`agnos://`) for agent ↔ webview communication
//!   - Security: sandboxed rendering, content-security-policy, agent permissions
//!
//! This module defines the data structures and management layer. The actual
//! wry/tao rendering is optional and gated behind the `webview` feature flag
//! to avoid pulling in heavy GUI dependencies on headless systems.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tracing::{debug, info};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for the WebView subsystem.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebViewConfig {
    /// hoosh gateway URL for LLM features.
    pub hoosh_url: String,
    /// Default window dimensions.
    pub default_width: u32,
    pub default_height: u32,
    /// Enable DevTools (development only).
    pub devtools: bool,
    /// Custom user agent string.
    pub user_agent: Option<String>,
    /// Data directory for persistent storage (cookies, cache).
    pub data_dir: PathBuf,
    /// Content Security Policy applied to all pages.
    pub content_security_policy: Option<String>,
    /// Maximum number of concurrent WebView instances.
    pub max_instances: usize,
    /// Allow navigation to external URLs (if false, only agnos:// and localhost).
    pub allow_external_navigation: bool,
    /// JavaScript injection for hoosh bridge (auto-injected into pages).
    pub bridge_script: String,
}

impl Default for WebViewConfig {
    fn default() -> Self {
        Self {
            hoosh_url: std::env::var("AGNOS_GATEWAY_BIND")
                .unwrap_or_else(|_| "http://127.0.0.1:8088".to_string()),
            default_width: 1280,
            default_height: 720,
            devtools: false,
            user_agent: Some("AGNOS WebView/1.0".to_string()),
            data_dir: PathBuf::from("/var/lib/agnos/webview"),
            content_security_policy: Some(
                "default-src 'self' http://127.0.0.1:* https:; \
                 script-src 'self' 'unsafe-inline'; \
                 connect-src 'self' http://127.0.0.1:* ws://127.0.0.1:*"
                    .to_string(),
            ),
            max_instances: 10,
            allow_external_navigation: false,
            bridge_script: default_bridge_script(),
        }
    }
}

/// Generate the default JavaScript bridge that connects pages to hoosh.
fn default_bridge_script() -> String {
    r#"
// AGNOS hoosh bridge — injected into WebView pages
window.agnos = {
    _hooshUrl: null,

    init: function(hooshUrl) {
        this._hooshUrl = hooshUrl;
    },

    // Chat completion via hoosh
    chat: async function(messages, options) {
        const resp = await fetch(this._hooshUrl + '/v1/chat/completions', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
                messages: messages,
                model: (options && options.model) || 'default',
                stream: false,
                ...options
            })
        });
        return resp.json();
    },

    // List available models
    models: async function() {
        const resp = await fetch(this._hooshUrl + '/v1/models');
        return resp.json();
    },

    // Summarize the current page content
    summarize: async function() {
        const text = document.body.innerText.substring(0, 8000);
        return this.chat([
            { role: 'system', content: 'Summarize the following web page content concisely.' },
            { role: 'user', content: text }
        ]);
    },

    // Extract structured data from the page
    extract: async function(schema) {
        const text = document.body.innerText.substring(0, 8000);
        return this.chat([
            { role: 'system', content: 'Extract structured data from the web page. Return valid JSON matching this schema: ' + JSON.stringify(schema) },
            { role: 'user', content: text }
        ]);
    },

    // Ask a question about the page
    ask: async function(question) {
        const text = document.body.innerText.substring(0, 8000);
        return this.chat([
            { role: 'system', content: 'Answer questions about the following web page content.' },
            { role: 'user', content: 'Page content:\n' + text + '\n\nQuestion: ' + question }
        ]);
    },

    // Send an IPC message to the agent runtime
    send: function(action, payload) {
        window.ipc.postMessage(JSON.stringify({ action: action, payload: payload }));
    }
};
"#
    .to_string()
}

// ---------------------------------------------------------------------------
// WebView instance
// ---------------------------------------------------------------------------

/// Unique identifier for a WebView instance.
pub type WebViewId = String;

/// State of a WebView instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WebViewState {
    /// Created but not yet shown.
    Created,
    /// Visible and active.
    Active,
    /// Loading a page.
    Loading,
    /// Hidden but still alive.
    Hidden,
    /// Closed and resources released.
    Closed,
}

impl std::fmt::Display for WebViewState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Created => write!(f, "created"),
            Self::Active => write!(f, "active"),
            Self::Loading => write!(f, "loading"),
            Self::Hidden => write!(f, "hidden"),
            Self::Closed => write!(f, "closed"),
        }
    }
}

/// Metadata and state for a WebView instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebViewInstance {
    /// Unique instance ID.
    pub id: WebViewId,
    /// Owning agent ID (for permission checks).
    pub agent_id: Option<String>,
    /// Human-readable label.
    pub label: String,
    /// Current state.
    pub state: WebViewState,
    /// Current URL.
    pub current_url: Option<String>,
    /// Page title (from <title> tag).
    pub title: Option<String>,
    /// Window dimensions.
    pub width: u32,
    pub height: u32,
    /// Whether DevTools are enabled for this instance.
    pub devtools: bool,
    /// Creation timestamp.
    pub created_at: String,
    /// Navigation history (URLs).
    pub history: Vec<String>,
    /// Custom initialization script (in addition to bridge).
    pub init_script: Option<String>,
}

/// Request to create a new WebView instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateWebViewRequest {
    /// Optional instance ID (auto-generated if not provided).
    pub id: Option<String>,
    /// Owning agent ID.
    pub agent_id: Option<String>,
    /// Human-readable label.
    pub label: Option<String>,
    /// Initial URL to load.
    pub url: Option<String>,
    /// Initial HTML content (alternative to URL).
    pub html: Option<String>,
    /// Window width.
    pub width: Option<u32>,
    /// Window height.
    pub height: Option<u32>,
    /// Enable DevTools.
    pub devtools: Option<bool>,
    /// Custom initialization script.
    pub init_script: Option<String>,
    /// Visible on creation.
    pub visible: Option<bool>,
}

/// Navigation request for a WebView instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NavigateRequest {
    /// URL to navigate to.
    pub url: Option<String>,
    /// HTML content to load directly.
    pub html: Option<String>,
}

/// JavaScript evaluation request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalRequest {
    /// JavaScript code to evaluate.
    pub script: String,
}

/// IPC message from WebView to agent runtime.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebViewIpcMessage {
    /// Source WebView instance ID.
    pub webview_id: String,
    /// Action name.
    pub action: String,
    /// Payload (arbitrary JSON).
    pub payload: serde_json::Value,
}

// ---------------------------------------------------------------------------
// AI features
// ---------------------------------------------------------------------------

/// AI-powered WebView features bridged through hoosh.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiFeature {
    /// Summarize page content.
    Summarize,
    /// Extract structured data from page.
    Extract,
    /// Answer questions about page content.
    Ask,
    /// Translate page content.
    Translate,
    /// Fill form fields intelligently.
    FormFill,
    /// Generate alt-text for images.
    AltText,
}

impl std::fmt::Display for AiFeature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Summarize => write!(f, "summarize"),
            Self::Extract => write!(f, "extract"),
            Self::Ask => write!(f, "ask"),
            Self::Translate => write!(f, "translate"),
            Self::FormFill => write!(f, "form_fill"),
            Self::AltText => write!(f, "alt_text"),
        }
    }
}

/// Request to invoke an AI feature on a WebView.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiFeatureRequest {
    /// Target WebView instance.
    pub webview_id: String,
    /// Feature to invoke.
    pub feature: AiFeature,
    /// Feature-specific parameters (e.g., question for Ask, schema for Extract).
    pub params: Option<serde_json::Value>,
    /// LLM model to use (default: hoosh default).
    pub model: Option<String>,
}

/// Result of an AI feature invocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiFeatureResult {
    /// Source WebView instance.
    pub webview_id: String,
    /// Feature that was invoked.
    pub feature: AiFeature,
    /// Result content.
    pub content: String,
    /// Token usage from hoosh.
    pub tokens_used: Option<u64>,
    /// Duration in milliseconds.
    pub duration_ms: u64,
}

// ---------------------------------------------------------------------------
// Permission model
// ---------------------------------------------------------------------------

/// Permission grant for an agent to use WebView features.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebViewPermission {
    /// Agent ID.
    pub agent_id: String,
    /// Allowed navigation domains (empty = all allowed if external nav enabled).
    pub allowed_domains: Vec<String>,
    /// Allowed AI features.
    pub allowed_features: Vec<AiFeature>,
    /// Maximum concurrent instances for this agent.
    pub max_instances: usize,
    /// Expiry timestamp (None = no expiry).
    pub expires_at: Option<String>,
    /// Can execute arbitrary JavaScript.
    pub allow_js_eval: bool,
}

impl Default for WebViewPermission {
    fn default() -> Self {
        Self {
            agent_id: String::new(),
            allowed_domains: Vec::new(),
            allowed_features: vec![AiFeature::Summarize, AiFeature::Ask],
            max_instances: 3,
            expires_at: None,
            allow_js_eval: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Manager
// ---------------------------------------------------------------------------

/// Manages WebView instances and their lifecycle.
///
/// The manager tracks metadata and state for all instances. Actual rendering
/// is delegated to the platform backend (wry/tao when the `webview` feature
/// is enabled, or headless mode for testing/server environments).
pub struct WebViewManager {
    config: WebViewConfig,
    instances: HashMap<WebViewId, WebViewInstance>,
    permissions: HashMap<String, WebViewPermission>,
    ipc_queue: Vec<WebViewIpcMessage>,
}

impl WebViewManager {
    /// Create a new WebView manager.
    pub fn new(config: WebViewConfig) -> Self {
        info!("WebView manager initialized (hoosh: {})", config.hoosh_url);
        Self {
            config,
            instances: HashMap::new(),
            permissions: HashMap::new(),
            ipc_queue: Vec::new(),
        }
    }

    /// Create a new instance with default config.
    pub fn with_defaults() -> Self {
        Self::new(WebViewConfig::default())
    }

    /// Create a new WebView instance.
    pub fn create(&mut self, req: CreateWebViewRequest) -> Result<WebViewInstance, WebViewError> {
        if self.instances.len() >= self.config.max_instances {
            return Err(WebViewError::TooManyInstances {
                max: self.config.max_instances,
            });
        }

        // Check agent permissions
        if let Some(ref agent_id) = req.agent_id {
            self.check_permission(agent_id, None)?;

            let agent_count = self
                .instances
                .values()
                .filter(|i| {
                    i.agent_id.as_deref() == Some(agent_id.as_str())
                        && i.state != WebViewState::Closed
                })
                .count();
            if let Some(perm) = self.permissions.get(agent_id) {
                if agent_count >= perm.max_instances {
                    return Err(WebViewError::TooManyInstances {
                        max: perm.max_instances,
                    });
                }
            }
        }

        let id = req.id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        if self.instances.contains_key(&id) {
            return Err(WebViewError::InstanceExists { id: id.clone() });
        }

        let instance = WebViewInstance {
            id: id.clone(),
            agent_id: req.agent_id,
            label: req.label.unwrap_or_else(|| {
                let short = if id.len() >= 8 { &id[..8] } else { &id };
                format!("webview-{}", short)
            }),
            state: if req.visible.unwrap_or(true) {
                WebViewState::Active
            } else {
                WebViewState::Created
            },
            current_url: req.url.clone(),
            title: None,
            width: req.width.unwrap_or(self.config.default_width),
            height: req.height.unwrap_or(self.config.default_height),
            devtools: req.devtools.unwrap_or(self.config.devtools),
            created_at: chrono::Utc::now().to_rfc3339(),
            history: req.url.into_iter().collect(),
            init_script: req.init_script,
        };

        info!(
            "Created WebView instance: {} ({})",
            instance.id, instance.label
        );
        self.instances.insert(id.clone(), instance.clone());
        Ok(instance)
    }

    /// Navigate a WebView to a new URL.
    pub fn navigate(
        &mut self,
        id: &str,
        req: NavigateRequest,
    ) -> Result<&WebViewInstance, WebViewError> {
        let instance = self
            .instances
            .get_mut(id)
            .ok_or_else(|| WebViewError::NotFound { id: id.to_string() })?;

        if instance.state == WebViewState::Closed {
            return Err(WebViewError::InstanceClosed { id: id.to_string() });
        }

        // Check navigation permissions
        if let Some(ref url) = req.url {
            if !self.config.allow_external_navigation {
                let is_local = url.starts_with("http://127.0.0.1")
                    || url.starts_with("http://localhost")
                    || url.starts_with("agnos://");
                if !is_local {
                    return Err(WebViewError::NavigationBlocked {
                        url: url.clone(),
                        reason: "external navigation disabled".into(),
                    });
                }
            }

            if let Some(ref agent_id) = instance.agent_id {
                if let Some(perm) = self.permissions.get(agent_id) {
                    if !perm.allowed_domains.is_empty() {
                        let domain_ok = perm.allowed_domains.iter().any(|d| url.contains(d));
                        if !domain_ok {
                            return Err(WebViewError::NavigationBlocked {
                                url: url.clone(),
                                reason: "domain not in allowlist".into(),
                            });
                        }
                    }
                }
            }

            instance.current_url = Some(url.clone());
            instance.history.push(url.clone());
        }

        instance.state = WebViewState::Loading;
        debug!("Navigating {} to {:?}", id, req.url);
        Ok(instance)
    }

    /// Close a WebView instance.
    pub fn close(&mut self, id: &str) -> Result<(), WebViewError> {
        let instance = self
            .instances
            .get_mut(id)
            .ok_or_else(|| WebViewError::NotFound { id: id.to_string() })?;

        instance.state = WebViewState::Closed;
        info!("Closed WebView instance: {}", id);
        Ok(())
    }

    /// Get a WebView instance by ID.
    pub fn get(&self, id: &str) -> Option<&WebViewInstance> {
        self.instances.get(id)
    }

    /// List all active WebView instances.
    pub fn list(&self) -> Vec<&WebViewInstance> {
        self.instances
            .values()
            .filter(|i| i.state != WebViewState::Closed)
            .collect()
    }

    /// List all instances for a specific agent.
    pub fn list_for_agent(&self, agent_id: &str) -> Vec<&WebViewInstance> {
        self.instances
            .values()
            .filter(|i| i.agent_id.as_deref() == Some(agent_id) && i.state != WebViewState::Closed)
            .collect()
    }

    /// Grant WebView permissions to an agent.
    pub fn grant_permission(&mut self, perm: WebViewPermission) {
        info!("Granted WebView permission to agent {}", perm.agent_id);
        self.permissions.insert(perm.agent_id.clone(), perm);
    }

    /// Revoke an agent's WebView permissions.
    pub fn revoke_permission(&mut self, agent_id: &str) {
        self.permissions.remove(agent_id);
        info!("Revoked WebView permission for agent {}", agent_id);
    }

    /// Get permissions for an agent.
    pub fn get_permission(&self, agent_id: &str) -> Option<&WebViewPermission> {
        self.permissions.get(agent_id)
    }

    /// Queue an IPC message from a WebView.
    pub fn queue_ipc(&mut self, msg: WebViewIpcMessage) {
        debug!("IPC from {}: {}", msg.webview_id, msg.action);
        self.ipc_queue.push(msg);
    }

    /// Drain queued IPC messages.
    pub fn drain_ipc(&mut self) -> Vec<WebViewIpcMessage> {
        std::mem::take(&mut self.ipc_queue)
    }

    /// Get manager statistics.
    pub fn stats(&self) -> WebViewStats {
        let active = self
            .instances
            .values()
            .filter(|i| i.state == WebViewState::Active || i.state == WebViewState::Loading)
            .count();
        let total = self.instances.len();
        let agents_with_permissions = self.permissions.len();

        WebViewStats {
            active_instances: active,
            total_instances: total,
            max_instances: self.config.max_instances,
            agents_with_permissions,
            hoosh_url: self.config.hoosh_url.clone(),
            pending_ipc: self.ipc_queue.len(),
        }
    }

    fn check_permission(
        &self,
        agent_id: &str,
        _feature: Option<AiFeature>,
    ) -> Result<(), WebViewError> {
        // Agents without explicit permission entries get default access
        // (create/navigate/close but no JS eval or AI features beyond summarize/ask)
        if let Some(perm) = self.permissions.get(agent_id) {
            // Check expiry
            if let Some(ref expires) = perm.expires_at {
                let now = chrono::Utc::now().to_rfc3339();
                if now > *expires {
                    return Err(WebViewError::PermissionExpired {
                        agent_id: agent_id.to_string(),
                    });
                }
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Stats
// ---------------------------------------------------------------------------

/// WebView subsystem statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebViewStats {
    pub active_instances: usize,
    pub total_instances: usize,
    pub max_instances: usize,
    pub agents_with_permissions: usize,
    pub hoosh_url: String,
    pub pending_ipc: usize,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// WebView errors.
#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
pub enum WebViewError {
    #[error("WebView instance not found: {id}")]
    NotFound { id: String },

    #[error("WebView instance already exists: {id}")]
    InstanceExists { id: String },

    #[error("WebView instance is closed: {id}")]
    InstanceClosed { id: String },

    #[error("Too many WebView instances (max: {max})")]
    TooManyInstances { max: usize },

    #[error("Navigation blocked to {url}: {reason}")]
    NavigationBlocked { url: String, reason: String },

    #[error("Permission expired for agent {agent_id}")]
    PermissionExpired { agent_id: String },

    #[error("AI feature not permitted: {feature}")]
    FeatureNotPermitted { feature: String },

    #[error("JavaScript evaluation not permitted for agent {agent_id}")]
    JsEvalNotPermitted { agent_id: String },
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> WebViewConfig {
        WebViewConfig {
            max_instances: 5,
            ..WebViewConfig::default()
        }
    }

    #[test]
    fn test_default_config() {
        let config = WebViewConfig::default();
        assert_eq!(config.hoosh_url, "http://127.0.0.1:8088");
        assert_eq!(config.default_width, 1280);
        assert_eq!(config.default_height, 720);
        assert!(!config.devtools);
        assert_eq!(config.max_instances, 10);
        assert!(!config.allow_external_navigation);
        assert!(config.content_security_policy.is_some());
    }

    #[test]
    fn test_bridge_script_contains_hoosh_methods() {
        let script = default_bridge_script();
        assert!(script.contains("window.agnos"));
        assert!(script.contains("chat:"));
        assert!(script.contains("models:"));
        assert!(script.contains("summarize:"));
        assert!(script.contains("extract:"));
        assert!(script.contains("ask:"));
        assert!(script.contains("/v1/chat/completions"));
        assert!(script.contains("/v1/models"));
    }

    #[test]
    fn test_create_webview() {
        let mut mgr = WebViewManager::new(test_config());
        let result = mgr.create(CreateWebViewRequest {
            id: Some("test-1".into()),
            agent_id: None,
            label: Some("Test View".into()),
            url: Some("http://127.0.0.1:8090/v1/health".into()),
            html: None,
            width: Some(800),
            height: Some(600),
            devtools: None,
            init_script: None,
            visible: Some(true),
        });

        let instance = result.unwrap();
        assert_eq!(instance.id, "test-1");
        assert_eq!(instance.label, "Test View");
        assert_eq!(instance.state, WebViewState::Active);
        assert_eq!(instance.width, 800);
        assert_eq!(instance.height, 600);
        assert_eq!(
            instance.current_url,
            Some("http://127.0.0.1:8090/v1/health".into())
        );
    }

    #[test]
    fn test_create_webview_defaults() {
        let mut mgr = WebViewManager::new(test_config());
        let result = mgr.create(CreateWebViewRequest {
            id: None,
            agent_id: None,
            label: None,
            url: None,
            html: None,
            width: None,
            height: None,
            devtools: None,
            init_script: None,
            visible: None,
        });

        let instance = result.unwrap();
        assert!(!instance.id.is_empty());
        assert!(instance.label.starts_with("webview-"));
        assert_eq!(instance.width, 1280);
        assert_eq!(instance.height, 720);
    }

    #[test]
    fn test_create_duplicate_id() {
        let mut mgr = WebViewManager::new(test_config());
        mgr.create(CreateWebViewRequest {
            id: Some("dup".into()),
            ..default_create_req()
        })
        .unwrap();

        let result = mgr.create(CreateWebViewRequest {
            id: Some("dup".into()),
            ..default_create_req()
        });
        assert!(matches!(result, Err(WebViewError::InstanceExists { .. })));
    }

    #[test]
    fn test_too_many_instances() {
        let mut mgr = WebViewManager::new(WebViewConfig {
            max_instances: 2,
            ..WebViewConfig::default()
        });

        mgr.create(CreateWebViewRequest {
            id: Some("a".into()),
            ..default_create_req()
        })
        .unwrap();
        mgr.create(CreateWebViewRequest {
            id: Some("b".into()),
            ..default_create_req()
        })
        .unwrap();

        let result = mgr.create(CreateWebViewRequest {
            id: Some("c".into()),
            ..default_create_req()
        });
        assert!(matches!(
            result,
            Err(WebViewError::TooManyInstances { max: 2 })
        ));
    }

    #[test]
    fn test_navigate() {
        let mut mgr = WebViewManager::new(test_config());
        mgr.create(CreateWebViewRequest {
            id: Some("nav".into()),
            ..default_create_req()
        })
        .unwrap();

        let result = mgr.navigate(
            "nav",
            NavigateRequest {
                url: Some("http://127.0.0.1:8088/v1/models".into()),
                html: None,
            },
        );
        let instance = result.unwrap();
        assert_eq!(instance.state, WebViewState::Loading);
        assert_eq!(
            instance.current_url,
            Some("http://127.0.0.1:8088/v1/models".into())
        );
    }

    #[test]
    fn test_navigate_external_blocked() {
        let mut mgr = WebViewManager::new(test_config());
        mgr.create(CreateWebViewRequest {
            id: Some("ext".into()),
            ..default_create_req()
        })
        .unwrap();

        let result = mgr.navigate(
            "ext",
            NavigateRequest {
                url: Some("https://evil.example.com".into()),
                html: None,
            },
        );
        assert!(matches!(
            result,
            Err(WebViewError::NavigationBlocked { .. })
        ));
    }

    #[test]
    fn test_navigate_localhost_allowed() {
        let mut mgr = WebViewManager::new(test_config());
        mgr.create(CreateWebViewRequest {
            id: Some("local".into()),
            ..default_create_req()
        })
        .unwrap();

        let result = mgr.navigate(
            "local",
            NavigateRequest {
                url: Some("http://localhost:3000".into()),
                html: None,
            },
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_navigate_agnos_protocol_allowed() {
        let mut mgr = WebViewManager::new(test_config());
        mgr.create(CreateWebViewRequest {
            id: Some("proto".into()),
            ..default_create_req()
        })
        .unwrap();

        let result = mgr.navigate(
            "proto",
            NavigateRequest {
                url: Some("agnos://dashboard".into()),
                html: None,
            },
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_navigate_not_found() {
        let mut mgr = WebViewManager::new(test_config());
        let result = mgr.navigate(
            "nonexistent",
            NavigateRequest {
                url: Some("http://127.0.0.1".into()),
                html: None,
            },
        );
        assert!(matches!(result, Err(WebViewError::NotFound { .. })));
    }

    #[test]
    fn test_close() {
        let mut mgr = WebViewManager::new(test_config());
        mgr.create(CreateWebViewRequest {
            id: Some("close-me".into()),
            ..default_create_req()
        })
        .unwrap();

        mgr.close("close-me").unwrap();
        let instance = mgr.get("close-me").unwrap();
        assert_eq!(instance.state, WebViewState::Closed);
    }

    #[test]
    fn test_navigate_closed_instance() {
        let mut mgr = WebViewManager::new(test_config());
        mgr.create(CreateWebViewRequest {
            id: Some("closed".into()),
            ..default_create_req()
        })
        .unwrap();
        mgr.close("closed").unwrap();

        let result = mgr.navigate(
            "closed",
            NavigateRequest {
                url: Some("http://127.0.0.1".into()),
                html: None,
            },
        );
        assert!(matches!(result, Err(WebViewError::InstanceClosed { .. })));
    }

    #[test]
    fn test_list_active() {
        let mut mgr = WebViewManager::new(test_config());
        mgr.create(CreateWebViewRequest {
            id: Some("a".into()),
            ..default_create_req()
        })
        .unwrap();
        mgr.create(CreateWebViewRequest {
            id: Some("b".into()),
            ..default_create_req()
        })
        .unwrap();
        mgr.create(CreateWebViewRequest {
            id: Some("c".into()),
            ..default_create_req()
        })
        .unwrap();
        mgr.close("b").unwrap();

        let active = mgr.list();
        assert_eq!(active.len(), 2);
    }

    #[test]
    fn test_list_for_agent() {
        let mut mgr = WebViewManager::new(test_config());
        mgr.create(CreateWebViewRequest {
            id: Some("x".into()),
            agent_id: Some("agent-1".into()),
            ..default_create_req()
        })
        .unwrap();
        mgr.create(CreateWebViewRequest {
            id: Some("y".into()),
            agent_id: Some("agent-2".into()),
            ..default_create_req()
        })
        .unwrap();
        mgr.create(CreateWebViewRequest {
            id: Some("z".into()),
            agent_id: Some("agent-1".into()),
            ..default_create_req()
        })
        .unwrap();

        let agent1 = mgr.list_for_agent("agent-1");
        assert_eq!(agent1.len(), 2);

        let agent2 = mgr.list_for_agent("agent-2");
        assert_eq!(agent2.len(), 1);
    }

    #[test]
    fn test_permissions() {
        let mut mgr = WebViewManager::new(test_config());

        mgr.grant_permission(WebViewPermission {
            agent_id: "agent-1".into(),
            allowed_domains: vec!["example.com".into()],
            allowed_features: vec![AiFeature::Summarize],
            max_instances: 2,
            expires_at: None,
            allow_js_eval: false,
        });

        let perm = mgr.get_permission("agent-1").unwrap();
        assert_eq!(perm.max_instances, 2);
        assert_eq!(perm.allowed_domains, vec!["example.com"]);
        assert!(!perm.allow_js_eval);

        mgr.revoke_permission("agent-1");
        assert!(mgr.get_permission("agent-1").is_none());
    }

    #[test]
    fn test_agent_instance_limit() {
        let mut mgr = WebViewManager::new(test_config());
        mgr.grant_permission(WebViewPermission {
            agent_id: "limited".into(),
            max_instances: 1,
            ..WebViewPermission::default()
        });

        mgr.create(CreateWebViewRequest {
            id: Some("first".into()),
            agent_id: Some("limited".into()),
            ..default_create_req()
        })
        .unwrap();

        let result = mgr.create(CreateWebViewRequest {
            id: Some("second".into()),
            agent_id: Some("limited".into()),
            ..default_create_req()
        });
        assert!(matches!(
            result,
            Err(WebViewError::TooManyInstances { max: 1 })
        ));
    }

    #[test]
    fn test_domain_allowlist() {
        let mut mgr = WebViewManager::new(WebViewConfig {
            allow_external_navigation: true,
            ..test_config()
        });
        mgr.grant_permission(WebViewPermission {
            agent_id: "restricted".into(),
            allowed_domains: vec!["docs.agnos.org".into()],
            ..WebViewPermission::default()
        });
        mgr.create(CreateWebViewRequest {
            id: Some("restricted-view".into()),
            agent_id: Some("restricted".into()),
            ..default_create_req()
        })
        .unwrap();

        // Allowed domain
        let ok = mgr.navigate(
            "restricted-view",
            NavigateRequest {
                url: Some("https://docs.agnos.org/guide".into()),
                html: None,
            },
        );
        assert!(ok.is_ok());

        // Blocked domain
        let blocked = mgr.navigate(
            "restricted-view",
            NavigateRequest {
                url: Some("https://evil.com".into()),
                html: None,
            },
        );
        assert!(matches!(
            blocked,
            Err(WebViewError::NavigationBlocked { .. })
        ));
    }

    #[test]
    fn test_ipc_queue() {
        let mut mgr = WebViewManager::new(test_config());
        mgr.queue_ipc(WebViewIpcMessage {
            webview_id: "wv-1".into(),
            action: "click".into(),
            payload: serde_json::json!({"x": 100, "y": 200}),
        });
        mgr.queue_ipc(WebViewIpcMessage {
            webview_id: "wv-1".into(),
            action: "submit".into(),
            payload: serde_json::json!(null),
        });

        let messages = mgr.drain_ipc();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].action, "click");
        assert_eq!(messages[1].action, "submit");

        // Queue should be empty after drain
        assert!(mgr.drain_ipc().is_empty());
    }

    #[test]
    fn test_stats() {
        let mut mgr = WebViewManager::new(test_config());
        mgr.create(CreateWebViewRequest {
            id: Some("s1".into()),
            ..default_create_req()
        })
        .unwrap();
        mgr.create(CreateWebViewRequest {
            id: Some("s2".into()),
            ..default_create_req()
        })
        .unwrap();
        mgr.close("s2").unwrap();

        let stats = mgr.stats();
        assert_eq!(stats.active_instances, 1);
        assert_eq!(stats.total_instances, 2);
        assert_eq!(stats.max_instances, 5);
    }

    #[test]
    fn test_webview_state_display() {
        assert_eq!(WebViewState::Created.to_string(), "created");
        assert_eq!(WebViewState::Active.to_string(), "active");
        assert_eq!(WebViewState::Loading.to_string(), "loading");
        assert_eq!(WebViewState::Hidden.to_string(), "hidden");
        assert_eq!(WebViewState::Closed.to_string(), "closed");
    }

    #[test]
    fn test_ai_feature_display() {
        assert_eq!(AiFeature::Summarize.to_string(), "summarize");
        assert_eq!(AiFeature::Extract.to_string(), "extract");
        assert_eq!(AiFeature::Ask.to_string(), "ask");
        assert_eq!(AiFeature::Translate.to_string(), "translate");
        assert_eq!(AiFeature::FormFill.to_string(), "form_fill");
        assert_eq!(AiFeature::AltText.to_string(), "alt_text");
    }

    #[test]
    fn test_navigation_history() {
        let mut mgr = WebViewManager::new(test_config());
        mgr.create(CreateWebViewRequest {
            id: Some("hist".into()),
            url: Some("http://127.0.0.1:8090".into()),
            ..default_create_req()
        })
        .unwrap();

        mgr.navigate(
            "hist",
            NavigateRequest {
                url: Some("http://127.0.0.1:8088".into()),
                html: None,
            },
        )
        .unwrap();
        mgr.navigate(
            "hist",
            NavigateRequest {
                url: Some("http://127.0.0.1:8090/v1/agents".into()),
                html: None,
            },
        )
        .unwrap();

        let instance = mgr.get("hist").unwrap();
        assert_eq!(instance.history.len(), 3);
    }

    #[test]
    fn test_hidden_webview() {
        let mut mgr = WebViewManager::new(test_config());
        let instance = mgr
            .create(CreateWebViewRequest {
                id: Some("hidden".into()),
                visible: Some(false),
                ..default_create_req()
            })
            .unwrap();
        assert_eq!(instance.state, WebViewState::Created);
    }

    #[test]
    fn test_default_permission() {
        let perm = WebViewPermission::default();
        assert_eq!(perm.max_instances, 3);
        assert!(!perm.allow_js_eval);
        assert!(perm.allowed_domains.is_empty());
        assert_eq!(perm.allowed_features.len(), 2);
    }

    #[test]
    fn test_webview_error_display() {
        let err = WebViewError::NotFound { id: "abc".into() };
        assert!(err.to_string().contains("abc"));

        let err = WebViewError::TooManyInstances { max: 5 };
        assert!(err.to_string().contains("5"));

        let err = WebViewError::NavigationBlocked {
            url: "https://evil.com".into(),
            reason: "blocked".into(),
        };
        assert!(err.to_string().contains("evil.com"));
    }

    #[test]
    fn test_webview_serialization() {
        let instance = WebViewInstance {
            id: "ser-1".into(),
            agent_id: Some("agent-1".into()),
            label: "Test".into(),
            state: WebViewState::Active,
            current_url: Some("http://127.0.0.1".into()),
            title: Some("Test Page".into()),
            width: 800,
            height: 600,
            devtools: false,
            created_at: "2026-03-10T00:00:00Z".into(),
            history: vec!["http://127.0.0.1".into()],
            init_script: None,
        };

        let json = serde_json::to_string(&instance).unwrap();
        assert!(json.contains("\"active\""));
        assert!(json.contains("agent-1"));

        let deser: WebViewInstance = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.id, "ser-1");
        assert_eq!(deser.state, WebViewState::Active);
    }

    #[test]
    fn test_with_defaults() {
        let mgr = WebViewManager::with_defaults();
        assert_eq!(mgr.config.max_instances, 10);
    }

    // Helper
    fn default_create_req() -> CreateWebViewRequest {
        CreateWebViewRequest {
            id: None,
            agent_id: None,
            label: None,
            url: None,
            html: None,
            width: None,
            height: None,
            devtools: None,
            init_script: None,
            visible: None,
        }
    }
}
