//! Edge — Fleet Management for AGNOS Edge Nodes
//!
//! Manages a registry of edge nodes running AGNOS in Edge boot mode.
//! Edge nodes are constrained-hardware devices (Raspberry Pi, NUCs, IoT
//! gateways) that run a single agent binary (e.g. SecureYeoman edge) and
//! connect upstream to a parent AGNOS instance via A2A protocol.
//!
//! This module provides:
//! - Edge node registration and decommissioning
//! - Health monitoring with heartbeat tracking
//! - Capability-based task routing
//! - Fleet-wide deployment and update operations
//! - mDNS-based peer discovery (Phase 14B)
//! - Auto-registration on boot (Phase 14B)
//! - WireGuard mesh networking config (Phase 14B)
//! - Heartbeat watchdog with stale node detection (Phase 14B)
//! - TPM 2.0 attestation wiring (Phase 14D)
//! - Signed OTA update verification (Phase 14D)
//! - Certificate pinning for parent-only trust (Phase 14D)

use std::collections::HashMap;
use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Edge node status
// ---------------------------------------------------------------------------

/// Health status of an edge node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EdgeNodeStatus {
    /// Receiving heartbeats, ready for tasks.
    Online,
    /// Missed heartbeats (>30s), may be failing.
    Suspect,
    /// No heartbeat for >60s, considered unreachable.
    Offline,
    /// Actively being updated (OTA in progress).
    Updating,
    /// Marked for removal from fleet.
    Decommissioned,
}

impl fmt::Display for EdgeNodeStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Online => write!(f, "online"),
            Self::Suspect => write!(f, "suspect"),
            Self::Offline => write!(f, "offline"),
            Self::Updating => write!(f, "updating"),
            Self::Decommissioned => write!(f, "decommissioned"),
        }
    }
}

// ---------------------------------------------------------------------------
// Edge node capabilities
// ---------------------------------------------------------------------------

/// Hardware and software capabilities advertised by an edge node.
/// Used for capability-based task routing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EdgeCapabilities {
    /// CPU architecture (e.g. "x86_64", "aarch64", "riscv64").
    pub arch: String,
    /// Number of CPU cores.
    pub cpu_cores: u32,
    /// Available memory in MB.
    pub memory_mb: u64,
    /// Available disk in MB.
    pub disk_mb: u64,
    /// Whether a GPU is available.
    pub has_gpu: bool,
    /// Network bandwidth quality (0.0 = poor, 1.0 = excellent).
    pub network_quality: f64,
    /// Geographic location label (optional, e.g. "us-east", "office-floor-2").
    pub location: Option<String>,
    /// Custom capability tags (e.g. ["camera", "bluetooth", "tpm"]).
    pub tags: Vec<String>,
}

impl Default for EdgeCapabilities {
    fn default() -> Self {
        Self {
            arch: "x86_64".into(),
            cpu_cores: 4,
            memory_mb: 1024,
            disk_mb: 4096,
            has_gpu: false,
            network_quality: 0.8,
            location: None,
            tags: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Edge node
// ---------------------------------------------------------------------------

/// An edge node in the fleet registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeNode {
    /// Unique node identifier.
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Current health status.
    pub status: EdgeNodeStatus,
    /// Hardware/software capabilities.
    pub capabilities: EdgeCapabilities,
    /// The agent binary running on this node (e.g. "secureyeoman-edge").
    pub agent_binary: String,
    /// Agent binary version.
    pub agent_version: String,
    /// AGNOS version running on the node.
    pub os_version: String,
    /// URL of the parent instance this node reports to.
    pub parent_url: String,
    /// Timestamp of last heartbeat received.
    pub last_heartbeat: DateTime<Utc>,
    /// Timestamp when the node registered.
    pub registered_at: DateTime<Utc>,
    /// Number of tasks currently running on this node.
    pub active_tasks: u32,
    /// Total tasks completed since registration.
    pub tasks_completed: u64,
    /// Whether TPM attestation passed.
    pub tpm_attested: bool,
    /// Signature of the last OTA update (for signed OTA verification).
    pub update_signature: Option<String>,
}

// ---------------------------------------------------------------------------
// Edge fleet registry
// ---------------------------------------------------------------------------

/// Configuration for the edge fleet manager.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeFleetConfig {
    /// Seconds before a node is marked suspect.
    pub suspect_threshold_secs: u64,
    /// Seconds before a node is marked offline.
    pub offline_threshold_secs: u64,
    /// Maximum number of edge nodes.
    pub max_nodes: usize,
    /// Whether to require TPM attestation for new nodes.
    pub require_tpm: bool,
}

impl Default for EdgeFleetConfig {
    fn default() -> Self {
        Self {
            suspect_threshold_secs: 30,
            offline_threshold_secs: 60,
            max_nodes: 1000,
            require_tpm: false,
        }
    }
}

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
        info!("Edge fleet manager initialized (max_nodes={})", config.max_nodes);
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
        };

        info!(id = %id, name = %name, "Edge node registered");
        self.nodes.insert(id.clone(), node);
        Ok(id)
    }

    /// Process a heartbeat from an edge node.
    pub fn heartbeat(
        &mut self,
        node_id: &str,
        active_tasks: u32,
        tasks_completed: u64,
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
            } else if elapsed > self.config.suspect_threshold_secs {
                if node.status != EdgeNodeStatus::Suspect {
                    warn!(id = %node.id, name = %node.name, elapsed_s = %elapsed, "Edge node suspect");
                    node.status = EdgeNodeStatus::Suspect;
                }
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

    /// Find the best node for a task based on required capabilities.
    /// Returns nodes sorted by suitability (least loaded, matching caps).
    pub fn route_task(
        &self,
        required_tags: &[String],
        require_gpu: bool,
        preferred_location: Option<&str>,
    ) -> Vec<&EdgeNode> {
        let mut candidates: Vec<&EdgeNode> = self
            .nodes
            .values()
            .filter(|n| {
                // Must be online.
                if n.status != EdgeNodeStatus::Online {
                    return false;
                }
                // Must have GPU if required.
                if require_gpu && !n.capabilities.has_gpu {
                    return false;
                }
                // Must have all required tags.
                for tag in required_tags {
                    if !n.capabilities.tags.contains(tag) {
                        return false;
                    }
                }
                true
            })
            .collect();

        // Sort by: preferred location first, then least loaded, then best network.
        candidates.sort_by(|a, b| {
            // Location preference.
            let a_loc = preferred_location
                .map_or(false, |loc| a.capabilities.location.as_deref() == Some(loc));
            let b_loc = preferred_location
                .map_or(false, |loc| b.capabilities.location.as_deref() == Some(loc));
            if a_loc != b_loc {
                return b_loc.cmp(&a_loc); // preferred location first
            }
            // Least active tasks.
            let task_cmp = a.active_tasks.cmp(&b.active_tasks);
            if task_cmp != std::cmp::Ordering::Equal {
                return task_cmp;
            }
            // Best network quality.
            b.capabilities
                .network_quality
                .partial_cmp(&a.capabilities.network_quality)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        candidates
    }

    /// Mark a node as updating (OTA in progress).
    pub fn start_update(&mut self, node_id: &str) -> Result<(), EdgeFleetError> {
        let node = self
            .nodes
            .get_mut(node_id)
            .ok_or_else(|| EdgeFleetError::NodeNotFound(node_id.to_string()))?;

        if node.status == EdgeNodeStatus::Decommissioned {
            return Err(EdgeFleetError::NodeDecommissioned(node_id.to_string()));
        }

        if node.active_tasks > 0 {
            return Err(EdgeFleetError::NodeBusy {
                node_id: node_id.to_string(),
                active_tasks: node.active_tasks,
            });
        }

        info!(id = %node_id, name = %node.name, "Edge node update started");
        node.status = EdgeNodeStatus::Updating;
        Ok(())
    }

    /// Mark an update as complete, returning node to online status.
    pub fn complete_update(
        &mut self,
        node_id: &str,
        new_version: String,
    ) -> Result<(), EdgeFleetError> {
        let node = self
            .nodes
            .get_mut(node_id)
            .ok_or_else(|| EdgeFleetError::NodeNotFound(node_id.to_string()))?;

        if node.status != EdgeNodeStatus::Updating {
            return Err(EdgeFleetError::NotUpdating(node_id.to_string()));
        }

        info!(id = %node_id, name = %node.name, version = %new_version, "Edge node update complete");
        node.agent_version = new_version;
        node.status = EdgeNodeStatus::Online;
        node.last_heartbeat = Utc::now();
        Ok(())
    }

    /// Check if a node can accept a task based on bandwidth and resource
    /// constraints. Returns `Ok(())` if the node can accept, or an error
    /// describing why it cannot.
    pub fn check_task_acceptance(
        &self,
        node_id: &str,
        min_bandwidth: f64,
        min_memory_mb: u64,
    ) -> Result<(), EdgeFleetError> {
        let node = self
            .nodes
            .get(node_id)
            .ok_or_else(|| EdgeFleetError::NodeNotFound(node_id.to_string()))?;

        if node.status != EdgeNodeStatus::Online {
            return Err(EdgeFleetError::NodeNotOnline(node_id.to_string()));
        }

        if node.capabilities.network_quality < min_bandwidth {
            return Err(EdgeFleetError::InsufficientBandwidth {
                node_id: node_id.to_string(),
                required: min_bandwidth,
                available: node.capabilities.network_quality,
            });
        }

        if node.capabilities.memory_mb < min_memory_mb {
            return Err(EdgeFleetError::InsufficientResources {
                node_id: node_id.to_string(),
                reason: format!(
                    "need {} MB memory, node has {} MB",
                    min_memory_mb, node.capabilities.memory_mb
                ),
            });
        }

        Ok(())
    }

    /// Route a task with bandwidth and resource constraints.
    /// Returns nodes that meet all requirements, sorted by suitability.
    pub fn route_task_with_constraints(
        &self,
        required_tags: &[String],
        require_gpu: bool,
        preferred_location: Option<&str>,
        min_bandwidth: f64,
        min_memory_mb: u64,
    ) -> Vec<&EdgeNode> {
        self.route_task(required_tags, require_gpu, preferred_location)
            .into_iter()
            .filter(|n| {
                n.capabilities.network_quality >= min_bandwidth
                    && n.capabilities.memory_mb >= min_memory_mb
            })
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

    // -----------------------------------------------------------------------
    // Phase 14D: Edge Security
    // -----------------------------------------------------------------------

    /// Mark a node as TPM-attested after successful TPM 2.0 attestation.
    ///
    /// In production this would verify a TPM quote against the node's
    /// endorsement key. For now it sets the `tpm_attested` flag to true.
    pub fn attest_node(&mut self, node_id: &str) -> Result<(), EdgeFleetError> {
        let node = self
            .nodes
            .get_mut(node_id)
            .ok_or_else(|| EdgeFleetError::NodeNotFound(node_id.to_string()))?;

        if node.status == EdgeNodeStatus::Decommissioned {
            return Err(EdgeFleetError::NodeDecommissioned(node_id.to_string()));
        }

        info!(id = %node_id, name = %node.name, "TPM 2.0 attestation passed");
        node.tpm_attested = true;
        Ok(())
    }

    /// Check whether a node has passed TPM attestation.
    pub fn require_attestation(&self, node_id: &str) -> Result<bool, EdgeFleetError> {
        let node = self
            .nodes
            .get(node_id)
            .ok_or_else(|| EdgeFleetError::NodeNotFound(node_id.to_string()))?;
        Ok(node.tpm_attested)
    }

    /// Verify the signature of an OTA update for a given node.
    ///
    /// **STUB**: Currently validates format only (non-empty, valid hex).
    /// Real ed25519 verification is NOT implemented — callers MUST NOT
    /// trust a `true` return in production without replacing this stub.
    pub fn verify_update_signature(
        &self,
        node_id: &str,
        signature: &str,
    ) -> Result<bool, EdgeFleetError> {
        let _node = self
            .nodes
            .get(node_id)
            .ok_or_else(|| EdgeFleetError::NodeNotFound(node_id.to_string()))?;

        if signature.is_empty() {
            debug!(id = %node_id, "OTA signature verification failed: empty signature");
            return Ok(false);
        }

        // Basic format check: signature should be hex-encoded
        if !signature.chars().all(|c| c.is_ascii_hexdigit()) {
            debug!(id = %node_id, "OTA signature verification failed: non-hex characters");
            return Ok(false);
        }

        // SECURITY STUB: This does NOT perform cryptographic verification.
        // TODO: Implement ed25519 verification against update payload hash.
        warn!(id = %node_id, "OTA signature format-checked only (stub — no crypto verification)");
        Ok(true)
    }

    /// Store the update signature on a node (called after a signed OTA is
    /// accepted). Rejects decommissioned nodes.
    pub fn set_update_signature(
        &mut self,
        node_id: &str,
        signature: String,
    ) -> Result<(), EdgeFleetError> {
        let node = self
            .nodes
            .get_mut(node_id)
            .ok_or_else(|| EdgeFleetError::NodeNotFound(node_id.to_string()))?;

        if node.status == EdgeNodeStatus::Decommissioned {
            return Err(EdgeFleetError::NodeDecommissioned(node_id.to_string()));
        }

        node.update_signature = Some(signature);
        Ok(())
    }

    /// Set the SHA-256 hash of the parent node's TLS certificate.
    ///
    /// Edge nodes pin this hash so they only trust their registered parent.
    /// The hash must be exactly 64 hex characters (SHA-256 digest).
    pub fn set_parent_cert_pin(&mut self, pin_hash: String) -> Result<(), EdgeFleetError> {
        if pin_hash.len() != 64 || !pin_hash.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(EdgeFleetError::InvalidName(
                "cert pin must be 64 hex characters (SHA-256)".to_string(),
            ));
        }
        info!(hash = %pin_hash, "Parent certificate pin set");
        self.parent_cert_pin = Some(pin_hash);
        Ok(())
    }

    /// Verify that a given certificate hash matches the pinned parent cert.
    ///
    /// Returns `true` if the hash matches, `false` if it does not match or
    /// no pin has been set.
    pub fn verify_parent_cert(&self, cert_hash: &str) -> bool {
        match &self.parent_cert_pin {
            Some(pin) => pin == cert_hash,
            None => false,
        }
    }
}

/// Fleet-wide statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeFleetStats {
    pub total_nodes: u32,
    pub online: u32,
    pub suspect: u32,
    pub offline: u32,
    pub updating: u32,
    pub decommissioned: u32,
    pub active_tasks: u32,
    pub tasks_completed: u64,
}

// ---------------------------------------------------------------------------
// WireGuard mesh networking (Phase 14B)
// ---------------------------------------------------------------------------

/// A peer in the WireGuard mesh network.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WireguardPeer {
    /// WireGuard public key for this peer.
    pub public_key: String,
    /// Network endpoint (host:port).
    pub endpoint: String,
    /// Allowed IP ranges for this peer.
    pub allowed_ips: Vec<String>,
}

/// WireGuard configuration for a node in the mesh.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WireguardConfig {
    /// Path to the private key file on disk.
    pub private_key_path: String,
    /// UDP port WireGuard listens on.
    pub listen_port: u16,
    /// Peer entries for all other nodes in the mesh.
    pub peers: Vec<WireguardPeer>,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors from edge fleet operations.
#[derive(Debug, Clone, PartialEq)]
pub enum EdgeFleetError {
    FleetFull { max: usize },
    InvalidName(String),
    DuplicateName(String),
    NodeNotFound(String),
    NodeDecommissioned(String),
    NodeNotOnline(String),
    NodeBusy { node_id: String, active_tasks: u32 },
    NotUpdating(String),
    InsufficientBandwidth {
        node_id: String,
        required: f64,
        available: f64,
    },
    InsufficientResources {
        node_id: String,
        reason: String,
    },
}

impl fmt::Display for EdgeFleetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FleetFull { max } => write!(f, "fleet is full (max {} nodes)", max),
            Self::InvalidName(msg) => write!(f, "invalid node name: {}", msg),
            Self::DuplicateName(name) => write!(f, "duplicate node name: {}", name),
            Self::NodeNotFound(id) => write!(f, "edge node not found: {}", id),
            Self::NodeDecommissioned(id) => write!(f, "edge node decommissioned: {}", id),
            Self::NodeNotOnline(id) => write!(f, "edge node {} is not online", id),
            Self::NodeBusy {
                node_id,
                active_tasks,
            } => write!(
                f,
                "edge node {} has {} active tasks",
                node_id, active_tasks
            ),
            Self::NotUpdating(id) => write!(f, "edge node {} is not in updating state", id),
            Self::InsufficientBandwidth {
                node_id,
                required,
                available,
            } => write!(
                f,
                "node {} bandwidth too low (need {:.1}, have {:.1})",
                node_id, required, available
            ),
            Self::InsufficientResources { node_id, reason } => {
                write!(f, "node {} insufficient resources: {}", node_id, reason)
            }
        }
    }
}

impl std::error::Error for EdgeFleetError {}

// ---------------------------------------------------------------------------
// Hardware targets (Phase 14C)
// ---------------------------------------------------------------------------

/// Known hardware targets for AGNOS Edge deployments.
///
/// Each variant carries default resource assumptions and maps to a kernel
/// config fragment (where applicable).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HardwareTarget {
    RaspberryPi4,
    RaspberryPi5,
    IntelNuc,
    GenericX86_64,
    GenericArm64,
    OciContainer,
}

impl HardwareTarget {
    /// Typical RAM in MiB for the target.
    pub fn default_ram_mb(&self) -> u64 {
        match self {
            Self::RaspberryPi4 => 4096,
            Self::RaspberryPi5 => 8192,
            Self::IntelNuc => 16384,
            Self::GenericX86_64 => 8192,
            Self::GenericArm64 => 2048,
            Self::OciContainer => 512,
        }
    }

    /// Typical disk in MiB for the target.
    pub fn default_disk_mb(&self) -> u64 {
        match self {
            Self::RaspberryPi4 => 32768,
            Self::RaspberryPi5 => 65536,
            Self::IntelNuc => 262144,
            Self::GenericX86_64 => 131072,
            Self::GenericArm64 => 16384,
            Self::OciContainer => 256,
        }
    }

    /// CPU architecture string.
    pub fn arch(&self) -> &str {
        match self {
            Self::RaspberryPi4 | Self::RaspberryPi5 | Self::GenericArm64 => "aarch64",
            Self::IntelNuc | Self::GenericX86_64 => "x86_64",
            Self::OciContainer => "x86_64",
        }
    }

    /// Whether the target has a GPU that AGNOS can leverage.
    pub fn supports_gpu(&self) -> bool {
        match self {
            Self::RaspberryPi4 | Self::RaspberryPi5 => true, // VideoCore
            Self::IntelNuc => true,                           // Intel UHD
            Self::GenericX86_64 | Self::GenericArm64 | Self::OciContainer => false,
        }
    }

    /// Path (relative to repo root) to the kernel config fragment, if any.
    pub fn kernel_config_fragment(&self) -> Option<&str> {
        match self {
            Self::RaspberryPi4 => Some("kernel/configs/edge-rpi4.config"),
            Self::RaspberryPi5 => Some("kernel/configs/edge-rpi5.config"),
            Self::IntelNuc => Some("kernel/configs/edge-nuc.config"),
            Self::GenericX86_64 | Self::GenericArm64 => None,
            Self::OciContainer => None,
        }
    }
}

impl fmt::Display for HardwareTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RaspberryPi4 => write!(f, "rpi4"),
            Self::RaspberryPi5 => write!(f, "rpi5"),
            Self::IntelNuc => write!(f, "nuc"),
            Self::GenericX86_64 => write!(f, "x86_64"),
            Self::GenericArm64 => write!(f, "arm64"),
            Self::OciContainer => write!(f, "oci"),
        }
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> EdgeFleetConfig {
        EdgeFleetConfig {
            suspect_threshold_secs: 30,
            offline_threshold_secs: 60,
            max_nodes: 100,
            require_tpm: false,
        }
    }

    fn test_capabilities() -> EdgeCapabilities {
        EdgeCapabilities {
            arch: "aarch64".into(),
            cpu_cores: 4,
            memory_mb: 2048,
            disk_mb: 16384,
            has_gpu: false,
            network_quality: 0.9,
            location: Some("office".into()),
            tags: vec!["camera".into(), "bluetooth".into()],
        }
    }

    fn register_test_node(mgr: &mut EdgeFleetManager, name: &str) -> String {
        mgr.register_node(
            name.into(),
            test_capabilities(),
            "secureyeoman-edge".into(),
            "2026.3.11".into(),
            "2026.3.11".into(),
            "http://parent:8090".into(),
        )
        .unwrap()
    }

    // --- Registration ---

    #[test]
    fn register_node_success() {
        let mut mgr = EdgeFleetManager::new(test_config());
        let id = register_test_node(&mut mgr, "rpi-kitchen");
        assert!(!id.is_empty());
        assert_eq!(mgr.nodes.len(), 1);
        let node = mgr.get_node(&id).unwrap();
        assert_eq!(node.name, "rpi-kitchen");
        assert_eq!(node.status, EdgeNodeStatus::Online);
        assert_eq!(node.agent_binary, "secureyeoman-edge");
    }

    #[test]
    fn register_node_empty_name_rejected() {
        let mut mgr = EdgeFleetManager::new(test_config());
        let err = mgr
            .register_node(
                "".into(),
                test_capabilities(),
                "edge".into(),
                "1.0".into(),
                "1.0".into(),
                "http://parent:8090".into(),
            )
            .unwrap_err();
        assert!(matches!(err, EdgeFleetError::InvalidName(_)));
    }

    #[test]
    fn register_node_duplicate_name_rejected() {
        let mut mgr = EdgeFleetManager::new(test_config());
        register_test_node(&mut mgr, "node-a");
        let err = mgr
            .register_node(
                "node-a".into(),
                test_capabilities(),
                "edge".into(),
                "1.0".into(),
                "1.0".into(),
                "http://parent:8090".into(),
            )
            .unwrap_err();
        assert!(matches!(err, EdgeFleetError::DuplicateName(_)));
    }

    #[test]
    fn register_after_decommission_allows_same_name() {
        let mut mgr = EdgeFleetManager::new(test_config());
        let id = register_test_node(&mut mgr, "node-a");
        mgr.decommission(&id).unwrap();
        // Should succeed since old node is decommissioned.
        let id2 = register_test_node(&mut mgr, "node-a");
        assert_ne!(id, id2);
    }

    #[test]
    fn register_fleet_full() {
        let config = EdgeFleetConfig {
            max_nodes: 2,
            ..test_config()
        };
        let mut mgr = EdgeFleetManager::new(config);
        register_test_node(&mut mgr, "a");
        register_test_node(&mut mgr, "b");
        let err = mgr
            .register_node(
                "c".into(),
                test_capabilities(),
                "edge".into(),
                "1.0".into(),
                "1.0".into(),
                "http://parent:8090".into(),
            )
            .unwrap_err();
        assert!(matches!(err, EdgeFleetError::FleetFull { max: 2 }));
    }

    // --- Heartbeat ---

    #[test]
    fn heartbeat_updates_state() {
        let mut mgr = EdgeFleetManager::new(test_config());
        let id = register_test_node(&mut mgr, "node-a");
        mgr.heartbeat(&id, 3, 100).unwrap();
        let node = mgr.get_node(&id).unwrap();
        assert_eq!(node.active_tasks, 3);
        assert_eq!(node.tasks_completed, 100);
    }

    #[test]
    fn heartbeat_unknown_node() {
        let mut mgr = EdgeFleetManager::new(test_config());
        let err = mgr.heartbeat("nonexistent", 0, 0).unwrap_err();
        assert!(matches!(err, EdgeFleetError::NodeNotFound(_)));
    }

    #[test]
    fn heartbeat_decommissioned_node_rejected() {
        let mut mgr = EdgeFleetManager::new(test_config());
        let id = register_test_node(&mut mgr, "node-a");
        mgr.decommission(&id).unwrap();
        let err = mgr.heartbeat(&id, 0, 0).unwrap_err();
        assert!(matches!(err, EdgeFleetError::NodeDecommissioned(_)));
    }

    // --- Health checks ---

    #[test]
    fn check_health_marks_suspect_and_offline() {
        let mut mgr = EdgeFleetManager::new(test_config());
        let id = register_test_node(&mut mgr, "node-a");

        // Simulate stale heartbeat.
        mgr.nodes.get_mut(&id).unwrap().last_heartbeat =
            Utc::now() - chrono::Duration::seconds(35);
        mgr.check_health();
        assert_eq!(mgr.get_node(&id).unwrap().status, EdgeNodeStatus::Suspect);

        // Simulate very stale heartbeat.
        mgr.nodes.get_mut(&id).unwrap().last_heartbeat =
            Utc::now() - chrono::Duration::seconds(90);
        mgr.check_health();
        assert_eq!(mgr.get_node(&id).unwrap().status, EdgeNodeStatus::Offline);
    }

    #[test]
    fn heartbeat_restores_from_suspect() {
        let mut mgr = EdgeFleetManager::new(test_config());
        let id = register_test_node(&mut mgr, "node-a");
        mgr.nodes.get_mut(&id).unwrap().status = EdgeNodeStatus::Suspect;
        mgr.heartbeat(&id, 0, 0).unwrap();
        assert_eq!(mgr.get_node(&id).unwrap().status, EdgeNodeStatus::Online);
    }

    #[test]
    fn heartbeat_restores_from_offline() {
        let mut mgr = EdgeFleetManager::new(test_config());
        let id = register_test_node(&mut mgr, "node-a");
        mgr.nodes.get_mut(&id).unwrap().status = EdgeNodeStatus::Offline;
        mgr.heartbeat(&id, 0, 0).unwrap();
        assert_eq!(mgr.get_node(&id).unwrap().status, EdgeNodeStatus::Online);
    }

    #[test]
    fn check_health_skips_decommissioned() {
        let mut mgr = EdgeFleetManager::new(test_config());
        let id = register_test_node(&mut mgr, "node-a");
        mgr.decommission(&id).unwrap();
        mgr.nodes.get_mut(&id).unwrap().last_heartbeat =
            Utc::now() - chrono::Duration::seconds(999);
        mgr.check_health();
        assert_eq!(
            mgr.get_node(&id).unwrap().status,
            EdgeNodeStatus::Decommissioned
        );
    }

    #[test]
    fn check_health_skips_updating() {
        let mut mgr = EdgeFleetManager::new(test_config());
        let id = register_test_node(&mut mgr, "node-a");
        mgr.nodes.get_mut(&id).unwrap().status = EdgeNodeStatus::Updating;
        mgr.nodes.get_mut(&id).unwrap().last_heartbeat =
            Utc::now() - chrono::Duration::seconds(999);
        mgr.check_health();
        assert_eq!(
            mgr.get_node(&id).unwrap().status,
            EdgeNodeStatus::Updating
        );
    }

    // --- Decommission ---

    #[test]
    fn decommission_success() {
        let mut mgr = EdgeFleetManager::new(test_config());
        let id = register_test_node(&mut mgr, "node-a");
        let node = mgr.decommission(&id).unwrap();
        assert_eq!(node.status, EdgeNodeStatus::Decommissioned);
    }

    #[test]
    fn decommission_already_decommissioned() {
        let mut mgr = EdgeFleetManager::new(test_config());
        let id = register_test_node(&mut mgr, "node-a");
        mgr.decommission(&id).unwrap();
        let err = mgr.decommission(&id).unwrap_err();
        assert!(matches!(err, EdgeFleetError::NodeDecommissioned(_)));
    }

    #[test]
    fn decommission_unknown_node() {
        let mut mgr = EdgeFleetManager::new(test_config());
        let err = mgr.decommission("fake").unwrap_err();
        assert!(matches!(err, EdgeFleetError::NodeNotFound(_)));
    }

    // --- List and filter ---

    #[test]
    fn list_nodes_all() {
        let mut mgr = EdgeFleetManager::new(test_config());
        register_test_node(&mut mgr, "a");
        register_test_node(&mut mgr, "b");
        register_test_node(&mut mgr, "c");
        assert_eq!(mgr.list_nodes(None).len(), 3);
    }

    #[test]
    fn list_nodes_filter_online() {
        let mut mgr = EdgeFleetManager::new(test_config());
        let id_a = register_test_node(&mut mgr, "a");
        register_test_node(&mut mgr, "b");
        mgr.decommission(&id_a).unwrap();
        let online = mgr.list_nodes(Some(EdgeNodeStatus::Online));
        assert_eq!(online.len(), 1);
        assert_eq!(online[0].name, "b");
    }

    // --- Task routing ---

    #[test]
    fn route_task_basic() {
        let mut mgr = EdgeFleetManager::new(test_config());
        register_test_node(&mut mgr, "a");
        register_test_node(&mut mgr, "b");
        let candidates = mgr.route_task(&[], false, None);
        assert_eq!(candidates.len(), 2);
    }

    #[test]
    fn route_task_excludes_offline() {
        let mut mgr = EdgeFleetManager::new(test_config());
        let id_a = register_test_node(&mut mgr, "a");
        register_test_node(&mut mgr, "b");
        mgr.nodes.get_mut(&id_a).unwrap().status = EdgeNodeStatus::Offline;
        let candidates = mgr.route_task(&[], false, None);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].name, "b");
    }

    #[test]
    fn route_task_requires_gpu() {
        let mut mgr = EdgeFleetManager::new(test_config());
        register_test_node(&mut mgr, "no-gpu");
        let id_gpu = mgr
            .register_node(
                "has-gpu".into(),
                EdgeCapabilities {
                    has_gpu: true,
                    ..test_capabilities()
                },
                "edge".into(),
                "1.0".into(),
                "1.0".into(),
                "http://parent:8090".into(),
            )
            .unwrap();
        let candidates = mgr.route_task(&[], true, None);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].id, id_gpu);
    }

    #[test]
    fn route_task_requires_tags() {
        let mut mgr = EdgeFleetManager::new(test_config());
        register_test_node(&mut mgr, "has-camera-bt"); // has camera + bluetooth
        mgr.register_node(
            "no-tags".into(),
            EdgeCapabilities {
                tags: vec![],
                ..test_capabilities()
            },
            "edge".into(),
            "1.0".into(),
            "1.0".into(),
            "http://parent:8090".into(),
        )
        .unwrap();

        let candidates = mgr.route_task(&["camera".into()], false, None);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].name, "has-camera-bt");
    }

    #[test]
    fn route_task_prefers_location() {
        let mut mgr = EdgeFleetManager::new(test_config());
        mgr.register_node(
            "far".into(),
            EdgeCapabilities {
                location: Some("us-west".into()),
                ..test_capabilities()
            },
            "edge".into(),
            "1.0".into(),
            "1.0".into(),
            "http://parent:8090".into(),
        )
        .unwrap();
        mgr.register_node(
            "near".into(),
            EdgeCapabilities {
                location: Some("office".into()),
                ..test_capabilities()
            },
            "edge".into(),
            "1.0".into(),
            "1.0".into(),
            "http://parent:8090".into(),
        )
        .unwrap();

        let candidates = mgr.route_task(&[], false, Some("office"));
        assert_eq!(candidates[0].name, "near");
    }

    #[test]
    fn route_task_prefers_least_loaded() {
        let mut mgr = EdgeFleetManager::new(test_config());
        let id_a = register_test_node(&mut mgr, "busy");
        register_test_node(&mut mgr, "idle");
        mgr.nodes.get_mut(&id_a).unwrap().active_tasks = 5;

        let candidates = mgr.route_task(&[], false, None);
        assert_eq!(candidates[0].name, "idle");
    }

    // --- Updates ---

    #[test]
    fn start_update_success() {
        let mut mgr = EdgeFleetManager::new(test_config());
        let id = register_test_node(&mut mgr, "a");
        mgr.start_update(&id).unwrap();
        assert_eq!(mgr.get_node(&id).unwrap().status, EdgeNodeStatus::Updating);
    }

    #[test]
    fn start_update_busy_node_rejected() {
        let mut mgr = EdgeFleetManager::new(test_config());
        let id = register_test_node(&mut mgr, "a");
        mgr.nodes.get_mut(&id).unwrap().active_tasks = 2;
        let err = mgr.start_update(&id).unwrap_err();
        assert!(matches!(err, EdgeFleetError::NodeBusy { .. }));
    }

    #[test]
    fn start_update_decommissioned_rejected() {
        let mut mgr = EdgeFleetManager::new(test_config());
        let id = register_test_node(&mut mgr, "a");
        mgr.decommission(&id).unwrap();
        let err = mgr.start_update(&id).unwrap_err();
        assert!(matches!(err, EdgeFleetError::NodeDecommissioned(_)));
    }

    #[test]
    fn complete_update_success() {
        let mut mgr = EdgeFleetManager::new(test_config());
        let id = register_test_node(&mut mgr, "a");
        mgr.start_update(&id).unwrap();
        mgr.complete_update(&id, "2026.4.0".into()).unwrap();
        let node = mgr.get_node(&id).unwrap();
        assert_eq!(node.status, EdgeNodeStatus::Online);
        assert_eq!(node.agent_version, "2026.4.0");
    }

    #[test]
    fn complete_update_not_updating_rejected() {
        let mut mgr = EdgeFleetManager::new(test_config());
        let id = register_test_node(&mut mgr, "a");
        let err = mgr.complete_update(&id, "2.0".into()).unwrap_err();
        assert!(matches!(err, EdgeFleetError::NotUpdating(_)));
    }

    // --- Stats ---

    #[test]
    fn stats_counts_correctly() {
        let mut mgr = EdgeFleetManager::new(test_config());
        let id_a = register_test_node(&mut mgr, "a");
        let id_b = register_test_node(&mut mgr, "b");
        register_test_node(&mut mgr, "c");

        mgr.nodes.get_mut(&id_a).unwrap().active_tasks = 3;
        mgr.nodes.get_mut(&id_a).unwrap().tasks_completed = 50;
        mgr.decommission(&id_b).unwrap();

        let stats = mgr.stats();
        assert_eq!(stats.total_nodes, 3);
        assert_eq!(stats.online, 2);
        assert_eq!(stats.decommissioned, 1);
        assert_eq!(stats.active_tasks, 3);
        assert_eq!(stats.tasks_completed, 50);
    }

    #[test]
    fn stats_empty_fleet() {
        let mgr = EdgeFleetManager::new(test_config());
        let stats = mgr.stats();
        assert_eq!(stats.total_nodes, 0);
        assert_eq!(stats.online, 0);
    }

    // --- Display ---

    #[test]
    fn status_display() {
        assert_eq!(EdgeNodeStatus::Online.to_string(), "online");
        assert_eq!(EdgeNodeStatus::Suspect.to_string(), "suspect");
        assert_eq!(EdgeNodeStatus::Offline.to_string(), "offline");
        assert_eq!(EdgeNodeStatus::Updating.to_string(), "updating");
        assert_eq!(EdgeNodeStatus::Decommissioned.to_string(), "decommissioned");
    }

    #[test]
    fn error_display() {
        assert!(EdgeFleetError::FleetFull { max: 10 }
            .to_string()
            .contains("full"));
        assert!(EdgeFleetError::NodeNotFound("x".into())
            .to_string()
            .contains("not found"));
        assert!(EdgeFleetError::DuplicateName("x".into())
            .to_string()
            .contains("duplicate"));
    }

    #[test]
    fn default_capabilities() {
        let caps = EdgeCapabilities::default();
        assert_eq!(caps.arch, "x86_64");
        assert_eq!(caps.cpu_cores, 4);
        assert!(!caps.has_gpu);
        assert!(caps.tags.is_empty());
    }

    #[test]
    fn default_config() {
        let config = EdgeFleetConfig::default();
        assert_eq!(config.suspect_threshold_secs, 30);
        assert_eq!(config.offline_threshold_secs, 60);
        assert_eq!(config.max_nodes, 1000);
        assert!(!config.require_tpm);
    }

    // --- Bandwidth-aware acceptance (14B) ---

    #[test]
    fn check_task_acceptance_ok() {
        let mut mgr = EdgeFleetManager::new(test_config());
        let id = register_test_node(&mut mgr, "node-a");
        // test_capabilities has network_quality=0.9, memory_mb=2048
        assert!(mgr.check_task_acceptance(&id, 0.5, 512).is_ok());
    }

    #[test]
    fn check_task_acceptance_insufficient_bandwidth() {
        let mut mgr = EdgeFleetManager::new(test_config());
        let id = register_test_node(&mut mgr, "node-a");
        let err = mgr.check_task_acceptance(&id, 0.95, 512).unwrap_err();
        assert!(matches!(err, EdgeFleetError::InsufficientBandwidth { .. }));
    }

    #[test]
    fn check_task_acceptance_insufficient_memory() {
        let mut mgr = EdgeFleetManager::new(test_config());
        let id = register_test_node(&mut mgr, "node-a");
        let err = mgr.check_task_acceptance(&id, 0.5, 8192).unwrap_err();
        assert!(matches!(err, EdgeFleetError::InsufficientResources { .. }));
    }

    #[test]
    fn check_task_acceptance_unknown_node() {
        let mgr = EdgeFleetManager::new(test_config());
        let err = mgr.check_task_acceptance("fake", 0.5, 512).unwrap_err();
        assert!(matches!(err, EdgeFleetError::NodeNotFound(_)));
    }

    #[test]
    fn route_task_with_constraints_filters_bandwidth() {
        let mut mgr = EdgeFleetManager::new(test_config());
        mgr.register_node(
            "fast".into(),
            EdgeCapabilities {
                network_quality: 0.95,
                memory_mb: 4096,
                ..test_capabilities()
            },
            "edge".into(),
            "1.0".into(),
            "1.0".into(),
            "http://parent:8090".into(),
        )
        .unwrap();
        mgr.register_node(
            "slow".into(),
            EdgeCapabilities {
                network_quality: 0.3,
                memory_mb: 4096,
                ..test_capabilities()
            },
            "edge".into(),
            "1.0".into(),
            "1.0".into(),
            "http://parent:8090".into(),
        )
        .unwrap();

        let candidates = mgr.route_task_with_constraints(&[], false, None, 0.5, 1024);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].name, "fast");
    }

    #[test]
    fn route_task_with_constraints_filters_memory() {
        let mut mgr = EdgeFleetManager::new(test_config());
        mgr.register_node(
            "big".into(),
            EdgeCapabilities {
                memory_mb: 8192,
                ..test_capabilities()
            },
            "edge".into(),
            "1.0".into(),
            "1.0".into(),
            "http://parent:8090".into(),
        )
        .unwrap();
        mgr.register_node(
            "small".into(),
            EdgeCapabilities {
                memory_mb: 256,
                ..test_capabilities()
            },
            "edge".into(),
            "1.0".into(),
            "1.0".into(),
            "http://parent:8090".into(),
        )
        .unwrap();

        let candidates = mgr.route_task_with_constraints(&[], false, None, 0.0, 4096);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].name, "big");
    }

    #[test]
    fn update_capabilities() {
        let mut mgr = EdgeFleetManager::new(test_config());
        let id = register_test_node(&mut mgr, "node-a");
        let new_caps = EdgeCapabilities {
            network_quality: 0.1,
            memory_mb: 512,
            ..test_capabilities()
        };
        mgr.update_capabilities(&id, new_caps).unwrap();
        let node = mgr.get_node(&id).unwrap();
        assert!((node.capabilities.network_quality - 0.1).abs() < f64::EPSILON);
        assert_eq!(node.capabilities.memory_mb, 512);
    }

    #[test]
    fn update_capabilities_decommissioned() {
        let mut mgr = EdgeFleetManager::new(test_config());
        let id = register_test_node(&mut mgr, "node-a");
        mgr.decommission(&id).unwrap();
        let err = mgr
            .update_capabilities(&id, test_capabilities())
            .unwrap_err();
        assert!(matches!(err, EdgeFleetError::NodeDecommissioned(_)));
    }

    #[test]
    fn error_display_bandwidth() {
        let err = EdgeFleetError::InsufficientBandwidth {
            node_id: "x".into(),
            required: 0.8,
            available: 0.3,
        };
        assert!(err.to_string().contains("bandwidth"));
    }

    #[test]
    fn error_display_resources() {
        let err = EdgeFleetError::InsufficientResources {
            node_id: "x".into(),
            reason: "low memory".into(),
        };
        assert!(err.to_string().contains("insufficient resources"));
    }

    // --- HardwareTarget (Phase 14C) ---

    #[test]
    fn hardware_target_default_ram() {
        assert_eq!(HardwareTarget::RaspberryPi4.default_ram_mb(), 4096);
        assert_eq!(HardwareTarget::RaspberryPi5.default_ram_mb(), 8192);
        assert_eq!(HardwareTarget::IntelNuc.default_ram_mb(), 16384);
        assert_eq!(HardwareTarget::GenericX86_64.default_ram_mb(), 8192);
        assert_eq!(HardwareTarget::GenericArm64.default_ram_mb(), 2048);
        assert_eq!(HardwareTarget::OciContainer.default_ram_mb(), 512);
    }

    #[test]
    fn hardware_target_default_disk() {
        assert_eq!(HardwareTarget::RaspberryPi4.default_disk_mb(), 32768);
        assert_eq!(HardwareTarget::IntelNuc.default_disk_mb(), 262144);
        assert_eq!(HardwareTarget::OciContainer.default_disk_mb(), 256);
    }

    #[test]
    fn hardware_target_arch() {
        assert_eq!(HardwareTarget::RaspberryPi4.arch(), "aarch64");
        assert_eq!(HardwareTarget::RaspberryPi5.arch(), "aarch64");
        assert_eq!(HardwareTarget::GenericArm64.arch(), "aarch64");
        assert_eq!(HardwareTarget::IntelNuc.arch(), "x86_64");
        assert_eq!(HardwareTarget::GenericX86_64.arch(), "x86_64");
        assert_eq!(HardwareTarget::OciContainer.arch(), "x86_64");
    }

    #[test]
    fn hardware_target_gpu_support() {
        assert!(HardwareTarget::RaspberryPi4.supports_gpu());
        assert!(HardwareTarget::RaspberryPi5.supports_gpu());
        assert!(HardwareTarget::IntelNuc.supports_gpu());
        assert!(!HardwareTarget::GenericX86_64.supports_gpu());
        assert!(!HardwareTarget::GenericArm64.supports_gpu());
        assert!(!HardwareTarget::OciContainer.supports_gpu());
    }

    #[test]
    fn hardware_target_kernel_config_fragment() {
        assert_eq!(
            HardwareTarget::RaspberryPi4.kernel_config_fragment(),
            Some("kernel/configs/edge-rpi4.config")
        );
        assert_eq!(
            HardwareTarget::RaspberryPi5.kernel_config_fragment(),
            Some("kernel/configs/edge-rpi5.config")
        );
        assert_eq!(
            HardwareTarget::IntelNuc.kernel_config_fragment(),
            Some("kernel/configs/edge-nuc.config")
        );
        assert_eq!(HardwareTarget::GenericX86_64.kernel_config_fragment(), None);
        assert_eq!(HardwareTarget::GenericArm64.kernel_config_fragment(), None);
        assert_eq!(HardwareTarget::OciContainer.kernel_config_fragment(), None);
    }

    #[test]
    fn hardware_target_display() {
        assert_eq!(HardwareTarget::RaspberryPi4.to_string(), "rpi4");
        assert_eq!(HardwareTarget::RaspberryPi5.to_string(), "rpi5");
        assert_eq!(HardwareTarget::IntelNuc.to_string(), "nuc");
        assert_eq!(HardwareTarget::GenericX86_64.to_string(), "x86_64");
        assert_eq!(HardwareTarget::GenericArm64.to_string(), "arm64");
        assert_eq!(HardwareTarget::OciContainer.to_string(), "oci");
    }

    #[test]
    fn hardware_target_serde_roundtrip() {
        let target = HardwareTarget::RaspberryPi5;
        let json = serde_json::to_string(&target).unwrap();
        let deserialized: HardwareTarget = serde_json::from_str(&json).unwrap();
        assert_eq!(target, deserialized);
    }

    #[test]
    fn hardware_target_clone_eq() {
        let a = HardwareTarget::IntelNuc;
        let b = a.clone();
        assert_eq!(a, b);
        assert_ne!(HardwareTarget::RaspberryPi4, HardwareTarget::RaspberryPi5);
    }

    // -----------------------------------------------------------------------
    // Phase 14D: Edge Security tests
    // -----------------------------------------------------------------------

    #[test]
    fn attest_node_success() {
        let mut mgr = EdgeFleetManager::new(test_config());
        let id = register_test_node(&mut mgr, "rpi-secure");
        assert!(!mgr.get_node(&id).unwrap().tpm_attested);
        mgr.attest_node(&id).unwrap();
        assert!(mgr.get_node(&id).unwrap().tpm_attested);
    }

    #[test]
    fn attest_node_not_found() {
        let mut mgr = EdgeFleetManager::new(test_config());
        let err = mgr.attest_node("nonexistent").unwrap_err();
        assert!(matches!(err, EdgeFleetError::NodeNotFound(_)));
    }

    #[test]
    fn attest_node_decommissioned_rejected() {
        let mut mgr = EdgeFleetManager::new(test_config());
        let id = register_test_node(&mut mgr, "rpi-old");
        mgr.decommission(&id).unwrap();
        let err = mgr.attest_node(&id).unwrap_err();
        assert!(matches!(err, EdgeFleetError::NodeDecommissioned(_)));
    }

    #[test]
    fn require_attestation_returns_status() {
        let mut mgr = EdgeFleetManager::new(test_config());
        let id = register_test_node(&mut mgr, "node-att");
        assert!(!mgr.require_attestation(&id).unwrap());
        mgr.attest_node(&id).unwrap();
        assert!(mgr.require_attestation(&id).unwrap());
    }

    #[test]
    fn require_attestation_not_found() {
        let mgr = EdgeFleetManager::new(test_config());
        let err = mgr.require_attestation("ghost").unwrap_err();
        assert!(matches!(err, EdgeFleetError::NodeNotFound(_)));
    }

    #[test]
    fn verify_update_signature_valid() {
        let mut mgr = EdgeFleetManager::new(test_config());
        let id = register_test_node(&mut mgr, "node-ota");
        let result = mgr.verify_update_signature(&id, "abc123def456").unwrap();
        assert!(result);
    }

    #[test]
    fn verify_update_signature_empty_rejected() {
        let mut mgr = EdgeFleetManager::new(test_config());
        let id = register_test_node(&mut mgr, "node-ota2");
        let result = mgr.verify_update_signature(&id, "").unwrap();
        assert!(!result);
    }

    #[test]
    fn verify_update_signature_node_not_found() {
        let mgr = EdgeFleetManager::new(test_config());
        let err = mgr.verify_update_signature("fake", "sig").unwrap_err();
        assert!(matches!(err, EdgeFleetError::NodeNotFound(_)));
    }

    #[test]
    fn set_update_signature_stores_value() {
        let mut mgr = EdgeFleetManager::new(test_config());
        let id = register_test_node(&mut mgr, "node-sig");
        assert!(mgr.get_node(&id).unwrap().update_signature.is_none());
        mgr.set_update_signature(&id, "ed25519:abcdef".into()).unwrap();
        assert_eq!(
            mgr.get_node(&id).unwrap().update_signature.as_deref(),
            Some("ed25519:abcdef")
        );
    }

    #[test]
    fn set_parent_cert_pin_and_verify() {
        let mut mgr = EdgeFleetManager::new(test_config());
        let valid_hash = "a".repeat(64);
        let wrong_hash = "b".repeat(64);
        assert!(!mgr.verify_parent_cert(&valid_hash));
        mgr.set_parent_cert_pin(valid_hash.clone()).unwrap();
        assert!(mgr.verify_parent_cert(&valid_hash));
        assert!(!mgr.verify_parent_cert(&wrong_hash));
    }

    #[test]
    fn set_parent_cert_pin_rejects_invalid() {
        let mut mgr = EdgeFleetManager::new(test_config());
        // Too short
        assert!(mgr.set_parent_cert_pin("abc".into()).is_err());
        // Non-hex
        let non_hex = "g".repeat(64);
        assert!(mgr.set_parent_cert_pin(non_hex).is_err());
    }

    #[test]
    fn verify_parent_cert_no_pin_returns_false() {
        let mgr = EdgeFleetManager::new(test_config());
        let hash = "a".repeat(64);
        assert!(!mgr.verify_parent_cert(&hash));
    }

    #[test]
    fn registered_node_has_no_attestation_or_signature() {
        let mut mgr = EdgeFleetManager::new(test_config());
        let id = register_test_node(&mut mgr, "fresh-node");
        let node = mgr.get_node(&id).unwrap();
        assert!(!node.tpm_attested);
        assert!(node.update_signature.is_none());
    }

    // === Phase 14B: A2A & Sub-Agent Networking ===

    // --- mDNS Discovery ---

    #[test]
    fn discover_peers_empty_by_default() {
        let mut mgr = EdgeFleetManager::new(test_config());
        std::env::remove_var("AGNOS_MDNS_PEERS");
        let peers = mgr.discover_peers();
        assert!(peers.is_empty());
    }

    #[test]
    fn add_discovery_peer_programmatic() {
        let mut mgr = EdgeFleetManager::new(test_config());
        mgr.add_discovery_peer("192.168.1.10:8090".into());
        mgr.add_discovery_peer("192.168.1.11:8090".into());
        assert_eq!(mgr.discovered_peers.len(), 2);
        assert!(mgr.discovered_peers.contains(&"192.168.1.10:8090".to_string()));
        assert!(mgr.discovered_peers.contains(&"192.168.1.11:8090".to_string()));
    }

    #[test]
    fn add_discovery_peer_deduplicates() {
        let mut mgr = EdgeFleetManager::new(test_config());
        mgr.add_discovery_peer("192.168.1.10:8090".into());
        mgr.add_discovery_peer("192.168.1.10:8090".into());
        assert_eq!(mgr.discovered_peers.len(), 1);
    }

    #[test]
    fn add_discovery_peer_ignores_empty() {
        let mut mgr = EdgeFleetManager::new(test_config());
        mgr.add_discovery_peer(String::new());
        assert!(mgr.discovered_peers.is_empty());
    }

    #[test]
    fn discover_peers_returns_programmatic() {
        let mut mgr = EdgeFleetManager::new(test_config());
        std::env::remove_var("AGNOS_MDNS_PEERS");
        mgr.add_discovery_peer("manual:8090".into());
        let peers = mgr.discover_peers();
        assert_eq!(peers.len(), 1);
        assert!(peers.contains(&"manual:8090".to_string()));
    }

    // --- Auto-registration on boot ---

    #[test]
    fn auto_register_node_success() {
        let mut mgr = EdgeFleetManager::new(test_config());
        let id = mgr.auto_register_node("edge-rpi-01", test_capabilities()).unwrap();
        assert!(!id.is_empty());
        let node = mgr.get_node(&id).unwrap();
        assert_eq!(node.name, "edge-rpi-01");
        assert_eq!(node.status, EdgeNodeStatus::Online);
        assert_eq!(node.agent_binary, "agnos-edge");
    }

    #[test]
    fn auto_register_node_empty_hostname_rejected() {
        let mut mgr = EdgeFleetManager::new(test_config());
        let err = mgr.auto_register_node("", test_capabilities()).unwrap_err();
        assert!(matches!(err, EdgeFleetError::InvalidName(_)));
    }

    #[test]
    fn auto_register_node_duplicate_rejected() {
        let mut mgr = EdgeFleetManager::new(test_config());
        mgr.auto_register_node("edge-01", test_capabilities()).unwrap();
        let err = mgr.auto_register_node("edge-01", test_capabilities()).unwrap_err();
        assert!(matches!(err, EdgeFleetError::DuplicateName(_)));
    }

    // --- WireGuard mesh config ---

    #[test]
    fn wireguard_config_single_node() {
        let mut mgr = EdgeFleetManager::new(test_config());
        let id = register_test_node(&mut mgr, "solo");
        let wg = mgr.generate_wireguard_config(&id).unwrap();
        assert_eq!(wg.listen_port, 51820);
        assert!(wg.private_key_path.contains(&id));
        assert!(wg.peers.is_empty());
    }

    #[test]
    fn wireguard_config_multi_node() {
        let mut mgr = EdgeFleetManager::new(test_config());
        let id_a = register_test_node(&mut mgr, "node-alpha");
        register_test_node(&mut mgr, "node-beta");
        register_test_node(&mut mgr, "node-gamma");
        let wg = mgr.generate_wireguard_config(&id_a).unwrap();
        assert_eq!(wg.peers.len(), 2);
        for peer in &wg.peers {
            assert!(!peer.endpoint.contains("node-alpha"));
            assert!(!peer.allowed_ips.is_empty());
            assert!(!peer.public_key.is_empty());
        }
    }

    #[test]
    fn wireguard_config_excludes_decommissioned() {
        let mut mgr = EdgeFleetManager::new(test_config());
        let id_a = register_test_node(&mut mgr, "alive-wg1");
        let id_b = register_test_node(&mut mgr, "gone-wg1");
        mgr.decommission(&id_b).unwrap();
        let wg = mgr.generate_wireguard_config(&id_a).unwrap();
        assert!(wg.peers.is_empty());
    }

    #[test]
    fn wireguard_config_excludes_offline() {
        let mut mgr = EdgeFleetManager::new(test_config());
        let id_a = register_test_node(&mut mgr, "alive-wg2");
        let id_b = register_test_node(&mut mgr, "down-wg2");
        mgr.nodes.get_mut(&id_b).unwrap().status = EdgeNodeStatus::Offline;
        let wg = mgr.generate_wireguard_config(&id_a).unwrap();
        assert!(wg.peers.is_empty());
    }

    #[test]
    fn wireguard_config_unknown_node() {
        let mgr = EdgeFleetManager::new(test_config());
        let err = mgr.generate_wireguard_config("nonexistent").unwrap_err();
        assert!(matches!(err, EdgeFleetError::NodeNotFound(_)));
    }

    #[test]
    fn wireguard_config_decommissioned_node_rejected() {
        let mut mgr = EdgeFleetManager::new(test_config());
        let id = register_test_node(&mut mgr, "dead-wg");
        mgr.decommission(&id).unwrap();
        let err = mgr.generate_wireguard_config(&id).unwrap_err();
        assert!(matches!(err, EdgeFleetError::NodeDecommissioned(_)));
    }

    // --- Heartbeat watchdog ---

    #[test]
    fn check_stale_nodes_none_stale() {
        let mut mgr = EdgeFleetManager::new(test_config());
        register_test_node(&mut mgr, "fresh-a14b");
        register_test_node(&mut mgr, "fresh-b14b");
        let stale = mgr.check_stale_nodes(60);
        assert!(stale.is_empty());
    }

    #[test]
    fn check_stale_nodes_marks_offline() {
        let mut mgr = EdgeFleetManager::new(test_config());
        let id = register_test_node(&mut mgr, "old-node-14b");
        mgr.nodes.get_mut(&id).unwrap().last_heartbeat =
            Utc::now() - chrono::Duration::seconds(120);
        let stale = mgr.check_stale_nodes(60);
        assert_eq!(stale.len(), 1);
        assert_eq!(stale[0], id);
        assert_eq!(mgr.get_node(&id).unwrap().status, EdgeNodeStatus::Offline);
    }

    #[test]
    fn check_stale_nodes_skips_already_offline() {
        let mut mgr = EdgeFleetManager::new(test_config());
        let id = register_test_node(&mut mgr, "already-off-14b");
        mgr.nodes.get_mut(&id).unwrap().status = EdgeNodeStatus::Offline;
        mgr.nodes.get_mut(&id).unwrap().last_heartbeat =
            Utc::now() - chrono::Duration::seconds(999);
        let stale = mgr.check_stale_nodes(60);
        assert!(stale.is_empty());
    }

    #[test]
    fn check_stale_nodes_skips_decommissioned() {
        let mut mgr = EdgeFleetManager::new(test_config());
        let id = register_test_node(&mut mgr, "decom-14b");
        mgr.decommission(&id).unwrap();
        mgr.nodes.get_mut(&id).unwrap().last_heartbeat =
            Utc::now() - chrono::Duration::seconds(999);
        let stale = mgr.check_stale_nodes(60);
        assert!(stale.is_empty());
    }

    #[test]
    fn check_stale_nodes_mixed_fleet() {
        let mut mgr = EdgeFleetManager::new(test_config());
        let id_fresh = register_test_node(&mut mgr, "fresh-14b");
        let id_stale = register_test_node(&mut mgr, "stale-14b");
        let id_decom = register_test_node(&mut mgr, "decom2-14b");

        mgr.nodes.get_mut(&id_stale).unwrap().last_heartbeat =
            Utc::now() - chrono::Duration::seconds(300);
        mgr.decommission(&id_decom).unwrap();

        let stale = mgr.check_stale_nodes(60);
        assert_eq!(stale.len(), 1);
        assert_eq!(stale[0], id_stale);
        assert_eq!(
            mgr.get_node(&id_fresh).unwrap().status,
            EdgeNodeStatus::Online
        );
    }
}
