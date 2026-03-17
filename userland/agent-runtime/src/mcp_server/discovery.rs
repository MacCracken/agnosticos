//! Dynamic MCP server discovery.
//!
//! Discovers MCP-compatible services on the local network via mDNS
//! and auto-registers their tools with the agent runtime. This allows
//! AGNOS to automatically find and use tools from SecureYeoman, Shruti,
//! Rasa, and other ecosystem services without manual configuration.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use chrono::{DateTime, Utc};

/// A discovered MCP server on the network.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DiscoveredMcpServer {
    /// Service name from mDNS (e.g. "secureyeoman-mcp").
    pub service_name: String,
    /// Hostname resolved from mDNS.
    pub hostname: String,
    /// Port number.
    pub port: u16,
    /// TXT record metadata (version, profile, tool_count, etc).
    pub metadata: HashMap<String, String>,
    /// When this server was first discovered.
    pub discovered_at: DateTime<Utc>,
    /// When we last successfully probed this server.
    pub last_seen: DateTime<Utc>,
    /// Whether we've successfully fetched tools from this server.
    pub tools_registered: bool,
    /// Number of tools registered from this server.
    pub tool_count: usize,
    /// Current status.
    pub status: DiscoveryStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DiscoveryStatus {
    /// Just discovered, tools not yet fetched.
    Discovered,
    /// Tools successfully fetched and registered.
    Active,
    /// Server was seen but tool fetch failed.
    Degraded,
    /// Server hasn't responded to recent probes.
    Stale,
    /// Manually disabled by the user.
    Disabled,
}

/// Configuration for MCP discovery.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DiscoveryConfig {
    /// Whether discovery is enabled.
    pub enabled: bool,
    /// mDNS service type to search for (default: "_mcp._tcp.local.").
    pub mdns_service_type: String,
    /// How often to probe for new servers (seconds).
    pub probe_interval_secs: u64,
    /// How long before a server is marked stale (seconds).
    pub stale_threshold_secs: u64,
    /// Whether to auto-register tools from discovered servers.
    pub auto_register_tools: bool,
    /// Maximum number of external servers to track.
    pub max_servers: usize,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            mdns_service_type: "_mcp._tcp.local.".into(),
            probe_interval_secs: 30,
            stale_threshold_secs: 300,
            auto_register_tools: true,
            max_servers: 50,
        }
    }
}

/// Manager for discovered MCP servers.
#[derive(Debug, Clone)]
pub struct McpDiscoveryManager {
    pub config: DiscoveryConfig,
    servers: Arc<RwLock<HashMap<String, DiscoveredMcpServer>>>,
}

impl McpDiscoveryManager {
    pub fn new(config: DiscoveryConfig) -> Self {
        Self {
            config,
            servers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a discovered server (called by mDNS listener or manual registration).
    pub async fn register_server(&self, server: DiscoveredMcpServer) -> Result<(), String> {
        let mut servers = self.servers.write().await;
        if servers.len() >= self.config.max_servers
            && !servers.contains_key(&server.service_name)
        {
            return Err(format!(
                "Maximum server limit ({}) reached",
                self.config.max_servers
            ));
        }
        servers.insert(server.service_name.clone(), server);
        Ok(())
    }

    /// Update a server's status and last_seen timestamp.
    pub async fn update_server_status(
        &self,
        service_name: &str,
        status: DiscoveryStatus,
        tool_count: Option<usize>,
    ) {
        let mut servers = self.servers.write().await;
        if let Some(server) = servers.get_mut(service_name) {
            server.status = status;
            server.last_seen = Utc::now();
            if let Some(count) = tool_count {
                server.tool_count = count;
                server.tools_registered = count > 0;
            }
        }
    }

    /// Remove a server from tracking.
    pub async fn remove_server(&self, service_name: &str) -> bool {
        let mut servers = self.servers.write().await;
        servers.remove(service_name).is_some()
    }

    /// List all discovered servers.
    pub async fn list_servers(&self) -> Vec<DiscoveredMcpServer> {
        let servers = self.servers.read().await;
        servers.values().cloned().collect()
    }

    /// Get a specific server by service name.
    pub async fn get_server(&self, service_name: &str) -> Option<DiscoveredMcpServer> {
        let servers = self.servers.read().await;
        servers.get(service_name).cloned()
    }

    /// Mark stale servers based on last_seen threshold.
    pub async fn mark_stale_servers(&self) -> usize {
        let threshold = chrono::Duration::seconds(self.config.stale_threshold_secs as i64);
        let cutoff = Utc::now() - threshold;
        let mut servers = self.servers.write().await;
        let mut count = 0;
        for server in servers.values_mut() {
            if server.last_seen < cutoff && server.status == DiscoveryStatus::Active {
                server.status = DiscoveryStatus::Stale;
                count += 1;
            }
        }
        count
    }

    /// Get servers that need tool registration (discovered but not yet active).
    pub async fn pending_servers(&self) -> Vec<DiscoveredMcpServer> {
        let servers = self.servers.read().await;
        servers
            .values()
            .filter(|s| s.status == DiscoveryStatus::Discovered)
            .cloned()
            .collect()
    }

    /// Build the URL for a discovered server's MCP tools endpoint.
    pub fn tools_url(server: &DiscoveredMcpServer) -> String {
        let scheme = if server.port == 443 { "https" } else { "http" };
        format!("{}://{}:{}/v1/mcp/tools", scheme, server.hostname, server.port)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_server(name: &str) -> DiscoveredMcpServer {
        DiscoveredMcpServer {
            service_name: name.into(),
            hostname: "192.168.1.100".into(),
            port: 3001,
            metadata: HashMap::from([
                ("version".into(), "2026.3.15".into()),
                ("profile".into(), "full".into()),
            ]),
            discovered_at: Utc::now(),
            last_seen: Utc::now(),
            tools_registered: false,
            tool_count: 0,
            status: DiscoveryStatus::Discovered,
        }
    }

    #[tokio::test]
    async fn test_register_and_list() {
        let mgr = McpDiscoveryManager::new(DiscoveryConfig::default());
        mgr.register_server(sample_server("sy-mcp")).await.unwrap();
        mgr.register_server(sample_server("shruti-mcp")).await.unwrap();

        let servers = mgr.list_servers().await;
        assert_eq!(servers.len(), 2);
    }

    #[tokio::test]
    async fn test_update_status() {
        let mgr = McpDiscoveryManager::new(DiscoveryConfig::default());
        mgr.register_server(sample_server("sy-mcp")).await.unwrap();

        mgr.update_server_status("sy-mcp", DiscoveryStatus::Active, Some(477))
            .await;

        let server = mgr.get_server("sy-mcp").await.unwrap();
        assert_eq!(server.status, DiscoveryStatus::Active);
        assert_eq!(server.tool_count, 477);
        assert!(server.tools_registered);
    }

    #[tokio::test]
    async fn test_remove_server() {
        let mgr = McpDiscoveryManager::new(DiscoveryConfig::default());
        mgr.register_server(sample_server("sy-mcp")).await.unwrap();

        assert!(mgr.remove_server("sy-mcp").await);
        assert!(!mgr.remove_server("nonexistent").await);
        assert!(mgr.list_servers().await.is_empty());
    }

    #[tokio::test]
    async fn test_max_servers_limit() {
        let config = DiscoveryConfig {
            max_servers: 2,
            ..Default::default()
        };
        let mgr = McpDiscoveryManager::new(config);
        mgr.register_server(sample_server("a")).await.unwrap();
        mgr.register_server(sample_server("b")).await.unwrap();

        let result = mgr.register_server(sample_server("c")).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_max_servers_allows_update() {
        let config = DiscoveryConfig {
            max_servers: 1,
            ..Default::default()
        };
        let mgr = McpDiscoveryManager::new(config);
        mgr.register_server(sample_server("a")).await.unwrap();

        // Re-registering the same name should succeed (update, not add)
        let result = mgr.register_server(sample_server("a")).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_mark_stale() {
        let config = DiscoveryConfig {
            stale_threshold_secs: 0, // immediate staleness for testing
            ..Default::default()
        };
        let mgr = McpDiscoveryManager::new(config);

        let mut server = sample_server("sy-mcp");
        server.status = DiscoveryStatus::Active;
        server.last_seen = Utc::now() - chrono::Duration::seconds(10);
        mgr.register_server(server).await.unwrap();

        let stale_count = mgr.mark_stale_servers().await;
        assert_eq!(stale_count, 1);

        let server = mgr.get_server("sy-mcp").await.unwrap();
        assert_eq!(server.status, DiscoveryStatus::Stale);
    }

    #[tokio::test]
    async fn test_pending_servers() {
        let mgr = McpDiscoveryManager::new(DiscoveryConfig::default());
        let mut s1 = sample_server("a");
        s1.status = DiscoveryStatus::Discovered;
        let mut s2 = sample_server("b");
        s2.status = DiscoveryStatus::Active;
        mgr.register_server(s1).await.unwrap();
        mgr.register_server(s2).await.unwrap();

        let pending = mgr.pending_servers().await;
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].service_name, "a");
    }

    #[test]
    fn test_tools_url_http() {
        let server = sample_server("test");
        let url = McpDiscoveryManager::tools_url(&server);
        assert_eq!(url, "http://192.168.1.100:3001/v1/mcp/tools");
    }

    #[test]
    fn test_tools_url_https() {
        let mut server = sample_server("test");
        server.port = 443;
        let url = McpDiscoveryManager::tools_url(&server);
        assert_eq!(url, "https://192.168.1.100:443/v1/mcp/tools");
    }

    #[test]
    fn test_default_config() {
        let config = DiscoveryConfig::default();
        assert!(config.enabled);
        assert_eq!(config.probe_interval_secs, 30);
        assert_eq!(config.stale_threshold_secs, 300);
        assert!(config.auto_register_tools);
        assert_eq!(config.max_servers, 50);
    }
}
