//! MCP tool proxy for the LLM gateway (hoosh).
//!
//! When a chat completion request includes `tools`, this module:
//! 1. Queries the agent runtime for the agent's allowed tool set
//! 2. Filters the request's tools to only those permitted
//! 3. Passes the filtered request to the LLM provider
//! 4. Returns the result with metadata about filtered tools
//!
//! This makes tool location and permissions invisible to the LLM —
//! it only sees tools it's allowed to use.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Configuration for the MCP proxy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpProxyConfig {
    /// Whether the proxy is enabled.
    pub enabled: bool,
    /// URL of the agent runtime (daimon) to query for permissions.
    pub agent_runtime_url: String,
    /// API key for the agent runtime.
    pub runtime_api_key: Option<String>,
    /// Timeout for runtime queries in milliseconds.
    pub timeout_ms: u64,
    /// Cache TTL for tool permissions in seconds.
    pub cache_ttl_secs: u64,
}

impl Default for McpProxyConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            agent_runtime_url: "http://127.0.0.1:8090".into(),
            runtime_api_key: None,
            timeout_ms: 5000,
            cache_ttl_secs: 60,
        }
    }
}

/// Represents a tool definition in an OpenAI-compatible chat request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatTool {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: ChatToolFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatToolFunction {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub parameters: serde_json::Value,
}

/// Result of tool filtering.
#[derive(Debug, Clone, Serialize)]
pub struct FilteredToolsResult {
    /// Tools that passed the filter.
    pub allowed: Vec<ChatTool>,
    /// Tool names that were removed.
    pub denied: Vec<String>,
    /// Total tools before filtering.
    pub total_requested: usize,
    /// Agent ID used for filtering (if any).
    pub agent_id: Option<String>,
}

/// Percent-encode a path segment using the `url` crate.
fn encode_path_segment(segment: &str) -> String {
    url::form_urlencoded::byte_serialize(segment.as_bytes()).collect()
}

/// Query the agent runtime for an agent's allowed tool prefixes.
async fn fetch_agent_permissions(
    config: &McpProxyConfig,
    agent_id: &str,
) -> Result<Vec<String>, String> {
    let client = reqwest::Client::new();
    let url = format!(
        "{}/v1/agents/{}/permissions",
        config.agent_runtime_url,
        encode_path_segment(agent_id)
    );

    let mut req = client
        .get(&url)
        .timeout(std::time::Duration::from_millis(config.timeout_ms));

    if let Some(key) = &config.runtime_api_key {
        req = req.header("Authorization", format!("Bearer {}", key));
    }

    let res = req
        .send()
        .await
        .map_err(|e| format!("Failed to fetch agent permissions: {}", e))?;

    if !res.status().is_success() {
        return Err(format!("Agent runtime returned HTTP {}", res.status()));
    }

    #[derive(Deserialize)]
    struct PermissionsResponse {
        #[serde(default)]
        allowed_prefixes: Vec<String>,
    }

    let body: PermissionsResponse = res
        .json()
        .await
        .map_err(|e| format!("Invalid permissions response: {}", e))?;

    Ok(body.allowed_prefixes)
}

/// Filter chat tools based on agent permissions.
///
/// If no agent_id is provided or permissions can't be fetched,
/// all tools pass through (fail-open for backward compatibility).
pub async fn filter_tools(
    config: &McpProxyConfig,
    tools: Vec<ChatTool>,
    agent_id: Option<&str>,
) -> FilteredToolsResult {
    let total_requested = tools.len();

    // If proxy disabled or no agent_id, pass all tools through
    if !config.enabled {
        return FilteredToolsResult {
            allowed: tools,
            denied: vec![],
            total_requested,
            agent_id: agent_id.map(String::from),
        };
    }

    let agent_id_str = match agent_id {
        Some(id) if !id.is_empty() => id,
        _ => {
            return FilteredToolsResult {
                allowed: tools,
                denied: vec![],
                total_requested,
                agent_id: None,
            };
        }
    };

    // Fetch permissions from agent runtime
    let allowed_prefixes = match fetch_agent_permissions(config, agent_id_str).await {
        Ok(prefixes) => prefixes,
        Err(_) => {
            // Fail open: if we can't reach the runtime, allow all tools
            return FilteredToolsResult {
                allowed: tools,
                denied: vec![],
                total_requested,
                agent_id: Some(agent_id_str.to_string()),
            };
        }
    };

    // Empty prefixes = allow all
    if allowed_prefixes.is_empty() {
        return FilteredToolsResult {
            allowed: tools,
            denied: vec![],
            total_requested,
            agent_id: Some(agent_id_str.to_string()),
        };
    }

    // Filter tools by prefix matching
    let prefix_set: HashSet<&str> = allowed_prefixes.iter().map(|s| s.as_str()).collect();
    let mut allowed = Vec::new();
    let mut denied = Vec::new();

    for tool in tools {
        let name = &tool.function.name;
        if prefix_set.iter().any(|prefix| name.starts_with(prefix)) {
            allowed.push(tool);
        } else {
            denied.push(name.clone());
        }
    }

    FilteredToolsResult {
        allowed,
        denied,
        total_requested,
        agent_id: Some(agent_id_str.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool(name: &str) -> ChatTool {
        ChatTool {
            tool_type: "function".into(),
            function: ChatToolFunction {
                name: name.into(),
                description: format!("Test tool {}", name),
                parameters: serde_json::json!({}),
            },
        }
    }

    fn disabled_config() -> McpProxyConfig {
        McpProxyConfig {
            enabled: false,
            ..Default::default()
        }
    }

    fn enabled_config() -> McpProxyConfig {
        McpProxyConfig {
            enabled: true,
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn test_disabled_proxy_passes_all() {
        let tools = vec![tool("phylax_scan"), tool("docker_ps"), tool("edge_list")];
        let result = filter_tools(&disabled_config(), tools, Some("agent-1")).await;
        assert_eq!(result.allowed.len(), 3);
        assert!(result.denied.is_empty());
        assert_eq!(result.total_requested, 3);
    }

    #[tokio::test]
    async fn test_no_agent_id_passes_all() {
        let tools = vec![tool("phylax_scan"), tool("docker_ps")];
        let result = filter_tools(&enabled_config(), tools, None).await;
        assert_eq!(result.allowed.len(), 2);
        assert!(result.denied.is_empty());
    }

    #[tokio::test]
    async fn test_empty_agent_id_passes_all() {
        let tools = vec![tool("phylax_scan"), tool("docker_ps")];
        let result = filter_tools(&enabled_config(), tools, Some("")).await;
        assert_eq!(result.allowed.len(), 2);
    }

    #[tokio::test]
    async fn test_runtime_unreachable_fails_open() {
        // Config points to non-existent server — should fail open
        let config = McpProxyConfig {
            enabled: true,
            agent_runtime_url: "http://127.0.0.1:1".into(),
            timeout_ms: 100,
            ..Default::default()
        };
        let tools = vec![tool("phylax_scan"), tool("docker_ps")];
        let result = filter_tools(&config, tools, Some("agent-1")).await;
        assert_eq!(result.allowed.len(), 2);
        assert!(result.denied.is_empty());
    }

    #[test]
    fn test_default_config() {
        let config = McpProxyConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.agent_runtime_url, "http://127.0.0.1:8090");
        assert_eq!(config.timeout_ms, 5000);
        assert_eq!(config.cache_ttl_secs, 60);
    }

    #[test]
    fn test_chat_tool_serialization() {
        let t = tool("test_tool");
        let json = serde_json::to_string(&t).unwrap();
        assert!(json.contains("test_tool"));
        assert!(json.contains("function"));

        let parsed: ChatTool = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.function.name, "test_tool");
    }

    #[test]
    fn test_filtered_result_serialization() {
        let result = FilteredToolsResult {
            allowed: vec![tool("a")],
            denied: vec!["b".into()],
            total_requested: 2,
            agent_id: Some("agent-1".into()),
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("agent-1"));
        assert!(json.contains("total_requested"));
    }
}
