//! Edge — Fleet management: registration, heartbeat, health, listing.

use std::collections::HashMap;

use chrono::Utc;
use tracing::{debug, info, warn};
use uuid::Uuid;

use super::types::{
    EdgeCapabilities, EdgeFleetConfig, EdgeFleetError, EdgeFleetStats, EdgeNode, EdgeNodeStatus,
    WireguardConfig, WireguardPeer,
};

/// Fleet manager for edge nodes. Tracks registration, health, and
/// capability-based task routing.
#[derive(Debug)]
pub struct EdgeFleetManager {
    pub config: EdgeFleetConfig,
    pub nodes: HashMap<String, EdgeNode>,
    /// Peers discovered via mDNS/Avahi or added programmatically (Phase 14B).
    pub discovered_peers: Vec<String>,
    /// SHA-256 hash of the parent node's TLS certificate for cert pinning (Phase 14D).
    pub parent_cert_pin: Option<String>,
}

impl EdgeFleetManager {
    /// Create a new fleet manager.
    pub fn new(config: EdgeFleetConfig) -> Self {
        info!(
            "Edge fleet manager initialized (max_nodes={})",
            config.max_nodes
        );
        Self {
            config,
            nodes: HashMap::new(),
            discovered_peers: Vec::new(),
            parent_cert_pin: None,
        }
    }

    /// Register a new edge node. Returns the assigned node ID.
    pub fn register_node(
        &mut self,
        name: String,
        capabilities: EdgeCapabilities,
        agent_binary: String,
        agent_version: String,
        os_version: String,
        parent_url: String,
    ) -> Result<String, EdgeFleetError> {
        if self.nodes.len() >= self.config.max_nodes {
            return Err(EdgeFleetError::FleetFull {
                max: self.config.max_nodes,
            });
        }

        if name.is_empty() {
            return Err(EdgeFleetError::InvalidName(
                "node name cannot be empty".into(),
            ));
        }

        // Check for duplicate names among active nodes.
        let name_exists = self
            .nodes
            .values()
            .any(|n| n.name == name && n.status != EdgeNodeStatus::Decommissioned);
        if name_exists {
            return Err(EdgeFleetError::DuplicateName(name));
        }

        if self.config.require_tpm {
            // TPM attestation would be verified here in production.
            // For now, we just log the requirement.
            debug!(node = %name, "TPM attestation required but not yet verified");
        }

        let id = Uuid::new_v4().to_string();
        let now = Utc::now();

        let node = EdgeNode {
            id: id.clone(),
            name: name.clone(),
            status: EdgeNodeStatus::Online,
            capabilities,
            agent_binary,
            agent_version,
            os_version,
            parent_url,
            last_heartbeat: now,
            registered_at: now,
            active_tasks: 0,
            tasks_completed: 0,
            tpm_attested: false,
            update_signature: None,
            gpu_utilization_pct: None,
            gpu_memory_used_mb: None,
            gpu_temperature_c: None,
            loaded_models: Vec::new(),
        };

        info!(id = %id, name = %name, "Edge node registered");
        self.nodes.insert(id.clone(), node);
        Ok(id)
    }

    /// Process a heartbeat from an edge node, including optional GPU telemetry
    /// and the list of models currently loaded on the node (G3.2).
    #[allow(clippy::too_many_arguments)]
    pub fn heartbeat(
        &mut self,
        node_id: &str,
        active_tasks: u32,
        tasks_completed: u64,
        gpu_utilization_pct: Option<f32>,
        gpu_memory_used_mb: Option<u64>,
        gpu_temperature_c: Option<f32>,
        loaded_models: Option<Vec<String>>,
    ) -> Result<(), EdgeFleetError> {
        let node = self
            .nodes
            .get_mut(node_id)
            .ok_or_else(|| EdgeFleetError::NodeNotFound(node_id.to_string()))?;

        if node.status == EdgeNodeStatus::Decommissioned {
            return Err(EdgeFleetError::NodeDecommissioned(node_id.to_string()));
        }

        node.last_heartbeat = Utc::now();
        node.active_tasks = active_tasks;
        node.tasks_completed = tasks_completed;

        // Update GPU telemetry (only overwrite if provided and valid).
        if let Some(pct) = gpu_utilization_pct {
            if pct.is_finite() && (0.0..=100.0).contains(&pct) {
                node.gpu_utilization_pct = Some(pct);
            }
        }
        if gpu_memory_used_mb.is_some() {
            node.gpu_memory_used_mb = gpu_memory_used_mb;
        }
        if let Some(temp) = gpu_temperature_c {
            if temp.is_finite() && (-50.0..=200.0).contains(&temp) {
                node.gpu_temperature_c = Some(temp);
            }
        }

        // G3.2: Update locally-loaded models list when provided.
        // Cap at 200 entries / 256 chars per name to prevent DoS.
        if let Some(models) = loaded_models {
            node.loaded_models = models
                .into_iter()
                .take(200)
                .map(|m| {
                    if m.len() > 256 {
                        m[..256].to_string()
                    } else {
                        m
                    }
                })
                .collect();
        }

        // Restore from suspect/offline if heartbeat arrives.
        if node.status == EdgeNodeStatus::Suspect || node.status == EdgeNodeStatus::Offline {
            info!(id = %node_id, "Edge node back online");
            node.status = EdgeNodeStatus::Online;
        }

        Ok(())
    }

    /// Update node statuses based on heartbeat age.
    pub fn check_health(&mut self) {
        let now = Utc::now();
        for node in self.nodes.values_mut() {
            if node.status == EdgeNodeStatus::Decommissioned
                || node.status == EdgeNodeStatus::Updating
            {
                continue;
            }

            let elapsed = (now - node.last_heartbeat).num_seconds().unsigned_abs();

            if elapsed > self.config.offline_threshold_secs {
                if node.status != EdgeNodeStatus::Offline {
                    warn!(id = %node.id, name = %node.name, elapsed_s = %elapsed, "Edge node offline");
                    node.status = EdgeNodeStatus::Offline;
                }
            } else if elapsed > self.config.suspect_threshold_secs
                && node.status != EdgeNodeStatus::Suspect
            {
                warn!(id = %node.id, name = %node.name, elapsed_s = %elapsed, "Edge node suspect");
                node.status = EdgeNodeStatus::Suspect;
            }
        }
    }

    /// Decommission an edge node (mark for removal).
    pub fn decommission(&mut self, node_id: &str) -> Result<EdgeNode, EdgeFleetError> {
        let node = self
            .nodes
            .get_mut(node_id)
            .ok_or_else(|| EdgeFleetError::NodeNotFound(node_id.to_string()))?;

        if node.status == EdgeNodeStatus::Decommissioned {
            return Err(EdgeFleetError::NodeDecommissioned(node_id.to_string()));
        }

        info!(id = %node_id, name = %node.name, "Edge node decommissioned");
        node.status = EdgeNodeStatus::Decommissioned;
        Ok(node.clone())
    }

    /// Get a node by ID.
    pub fn get_node(&self, node_id: &str) -> Option<&EdgeNode> {
        self.nodes.get(node_id)
    }

    /// List all nodes, optionally filtering by status.
    pub fn list_nodes(&self, status_filter: Option<EdgeNodeStatus>) -> Vec<&EdgeNode> {
        self.nodes
            .values()
            .filter(|n| status_filter.map_or(true, |s| n.status == s))
            .collect()
    }

    /// Update a node's reported capabilities (e.g. after a heartbeat with
    /// updated network quality metrics).
    pub fn update_capabilities(
        &mut self,
        node_id: &str,
        capabilities: EdgeCapabilities,
    ) -> Result<(), EdgeFleetError> {
        let node = self
            .nodes
            .get_mut(node_id)
            .ok_or_else(|| EdgeFleetError::NodeNotFound(node_id.to_string()))?;

        if node.status == EdgeNodeStatus::Decommissioned {
            return Err(EdgeFleetError::NodeDecommissioned(node_id.to_string()));
        }

        debug!(id = %node_id, name = %node.name, "Edge node capabilities updated");
        node.capabilities = capabilities;
        Ok(())
    }

    /// Fleet statistics.
    pub fn stats(&self) -> EdgeFleetStats {
        let mut online = 0u32;
        let mut suspect = 0u32;
        let mut offline = 0u32;
        let mut updating = 0u32;
        let mut decommissioned = 0u32;
        let mut total_tasks = 0u32;
        let mut total_completed = 0u64;

        for node in self.nodes.values() {
            match node.status {
                EdgeNodeStatus::Online => online += 1,
                EdgeNodeStatus::Suspect => suspect += 1,
                EdgeNodeStatus::Offline => offline += 1,
                EdgeNodeStatus::Updating => updating += 1,
                EdgeNodeStatus::Decommissioned => decommissioned += 1,
            }
            total_tasks += node.active_tasks;
            total_completed += node.tasks_completed;
        }

        EdgeFleetStats {
            total_nodes: self.nodes.len() as u32,
            online,
            suspect,
            offline,
            updating,
            decommissioned,
            active_tasks: total_tasks,
            tasks_completed: total_completed,
        }
    }

    // -----------------------------------------------------------------------
    // Phase 14B: A2A & Sub-Agent Networking
    // -----------------------------------------------------------------------

    /// Discover peers via mDNS/Avahi simulation.
    ///
    /// Checks the `AGNOS_MDNS_PEERS` environment variable for a
    /// comma-separated list of `host:port` addresses and merges them
    /// with any programmatically added peers.  Returns the full list
    /// of discovered peer addresses (deduplicated).
    /// Maximum number of discovered peers to prevent unbounded growth.
    const MAX_DISCOVERED_PEERS: usize = 256;

    pub fn discover_peers(&mut self) -> Vec<String> {
        // Read from environment (simulates mDNS/Avahi response).
        if let Ok(env_val) = std::env::var("AGNOS_MDNS_PEERS") {
            for addr in env_val.split(',') {
                let addr = addr.trim().to_string();
                if !addr.is_empty()
                    && Self::is_valid_peer_addr(&addr)
                    && !self.discovered_peers.contains(&addr)
                    && self.discovered_peers.len() < Self::MAX_DISCOVERED_PEERS
                {
                    info!(peer = %addr, "Discovered peer via mDNS");
                    self.discovered_peers.push(addr);
                }
            }
        }

        self.discovered_peers.clone()
    }

    /// Validate a peer address looks like host:port.
    fn is_valid_peer_addr(addr: &str) -> bool {
        if addr.len() > 253 {
            return false;
        }
        // Must contain a colon separating host and port
        if let Some(colon_pos) = addr.rfind(':') {
            let port_str = &addr[colon_pos + 1..];
            // Port must be a valid number
            if port_str.parse::<u16>().is_err() {
                return false;
            }
            let host = &addr[..colon_pos];
            // Host must be non-empty and contain only valid chars
            !host.is_empty()
                && host
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ':'))
        } else {
            false
        }
    }

    /// Programmatically add a discovery peer address (e.g. from a
    /// configuration file or manual operator input).
    pub fn add_discovery_peer(&mut self, addr: String) {
        if !addr.is_empty()
            && Self::is_valid_peer_addr(&addr)
            && !self.discovered_peers.contains(&addr)
            && self.discovered_peers.len() < Self::MAX_DISCOVERED_PEERS
        {
            info!(peer = %addr, "Added discovery peer");
            self.discovered_peers.push(addr);
        }
    }

    /// Auto-register a node during edge boot (called by argonaut).
    ///
    /// Creates a node with `EdgeNodeStatus::Online`, using the hostname as
    /// both the node name and label, with default agent metadata.  Returns
    /// the generated node ID on success.
    pub fn auto_register_node(
        &mut self,
        hostname: &str,
        capabilities: EdgeCapabilities,
    ) -> Result<String, EdgeFleetError> {
        if hostname.is_empty() {
            return Err(EdgeFleetError::InvalidName(
                "hostname cannot be empty".into(),
            ));
        }

        info!(hostname = %hostname, "Auto-registering edge node on boot");

        self.register_node(
            hostname.to_string(),
            capabilities,
            "agnos-edge".into(),
            env!("CARGO_PKG_VERSION").to_string(),
            env!("CARGO_PKG_VERSION").to_string(),
            "http://localhost:8090".into(),
        )
    }

    /// Generate a WireGuard mesh configuration for the given node.
    ///
    /// Builds a [`WireguardConfig`] with placeholder keys (actual key
    /// exchange is handled by WireGuard itself).  Each other active node
    /// in the fleet becomes a peer entry.
    pub fn generate_wireguard_config(
        &self,
        node_id: &str,
    ) -> Result<WireguardConfig, EdgeFleetError> {
        let node = self
            .nodes
            .get(node_id)
            .ok_or_else(|| EdgeFleetError::NodeNotFound(node_id.to_string()))?;

        if node.status == EdgeNodeStatus::Decommissioned {
            return Err(EdgeFleetError::NodeDecommissioned(node_id.to_string()));
        }

        let peers: Vec<WireguardPeer> = self
            .nodes
            .values()
            .filter(|n| {
                n.id != node_id
                    && n.status != EdgeNodeStatus::Decommissioned
                    && n.status != EdgeNodeStatus::Offline
            })
            .map(|n| {
                // Derive a deterministic placeholder public key from the node ID.
                let key_hash = format!("wg-pubkey-{}", &n.id[..8.min(n.id.len())]);
                WireguardPeer {
                    public_key: key_hash,
                    endpoint: format!("{}:51820", n.name),
                    allowed_ips: vec!["10.100.0.0/24".to_string()],
                }
            })
            .collect();

        info!(
            node_id = %node_id,
            peer_count = peers.len(),
            "Generated WireGuard mesh config"
        );

        Ok(WireguardConfig {
            private_key_path: format!("/run/agnos/wireguard/{}.key", node_id),
            listen_port: 51820,
            peers,
        })
    }

    /// Check for stale nodes whose last heartbeat exceeds `timeout_secs`.
    ///
    /// Transitions stale nodes to [`EdgeNodeStatus::Offline`] and returns
    /// the IDs of nodes that were moved offline.
    pub fn check_stale_nodes(&mut self, timeout_secs: u64) -> Vec<String> {
        let now = Utc::now();
        let mut stale_ids = Vec::new();

        for node in self.nodes.values_mut() {
            // Skip nodes already offline, decommissioned, or updating.
            if node.status == EdgeNodeStatus::Offline
                || node.status == EdgeNodeStatus::Decommissioned
                || node.status == EdgeNodeStatus::Updating
            {
                continue;
            }

            let elapsed = (now - node.last_heartbeat).num_seconds().unsigned_abs();
            if elapsed > timeout_secs {
                warn!(
                    id = %node.id,
                    name = %node.name,
                    elapsed_s = %elapsed,
                    timeout_s = %timeout_secs,
                    "Heartbeat watchdog: node stale, marking offline"
                );
                node.status = EdgeNodeStatus::Offline;
                stale_ids.push(node.id.clone());
            }
        }

        stale_ids
    }
}
