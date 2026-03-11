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
}

impl EdgeFleetManager {
    /// Create a new fleet manager.
    pub fn new(config: EdgeFleetConfig) -> Self {
        info!("Edge fleet manager initialized (max_nodes={})", config.max_nodes);
        Self {
            config,
            nodes: HashMap::new(),
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
    NodeBusy { node_id: String, active_tasks: u32 },
    NotUpdating(String),
}

impl fmt::Display for EdgeFleetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FleetFull { max } => write!(f, "fleet is full (max {} nodes)", max),
            Self::InvalidName(msg) => write!(f, "invalid node name: {}", msg),
            Self::DuplicateName(name) => write!(f, "duplicate node name: {}", name),
            Self::NodeNotFound(id) => write!(f, "edge node not found: {}", id),
            Self::NodeDecommissioned(id) => write!(f, "edge node decommissioned: {}", id),
            Self::NodeBusy {
                node_id,
                active_tasks,
            } => write!(
                f,
                "edge node {} has {} active tasks",
                node_id, active_tasks
            ),
            Self::NotUpdating(id) => write!(f, "edge node {} is not in updating state", id),
        }
    }
}

impl std::error::Error for EdgeFleetError {}

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
}
