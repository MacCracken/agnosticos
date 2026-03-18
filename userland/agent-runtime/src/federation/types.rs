//! Federation types — all structs and enums for the federation module.

use std::collections::HashMap;
use std::fmt;
use std::net::SocketAddr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// NodeRole
// ---------------------------------------------------------------------------

/// Role a node plays in the federation cluster.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NodeRole {
    /// Elected leader — makes scheduling decisions.
    Coordinator,
    /// Following the coordinator.
    Follower,
    /// Running for coordinator election.
    Candidate,
}

impl fmt::Display for NodeRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Coordinator => write!(f, "coordinator"),
            Self::Follower => write!(f, "follower"),
            Self::Candidate => write!(f, "candidate"),
        }
    }
}

// ---------------------------------------------------------------------------
// NodeStatus
// ---------------------------------------------------------------------------

/// Health status of a federation node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NodeStatus {
    /// Receiving heartbeats normally.
    Online,
    /// Missed heartbeats (>15s), may be failing.
    Suspect,
    /// No heartbeat for >30s, considered failed.
    Dead,
}

impl fmt::Display for NodeStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Online => write!(f, "online"),
            Self::Suspect => write!(f, "suspect"),
            Self::Dead => write!(f, "dead"),
        }
    }
}

// ---------------------------------------------------------------------------
// NodeCapabilities
// ---------------------------------------------------------------------------

/// Hardware capabilities advertised by a federation node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeCapabilities {
    pub cpu_cores: u32,
    pub memory_mb: u64,
    pub gpu_count: u32,
}

impl Default for NodeCapabilities {
    fn default() -> Self {
        Self {
            cpu_cores: 4,
            memory_mb: 8192,
            gpu_count: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// FederationNode
// ---------------------------------------------------------------------------

/// Identity and state of a single node in the federation cluster.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationNode {
    pub node_id: String,
    pub name: String,
    pub address: SocketAddr,
    pub role: NodeRole,
    pub status: NodeStatus,
    pub last_heartbeat: DateTime<Utc>,
    pub capabilities: NodeCapabilities,
    /// Current Raft term this node is aware of.
    pub current_term: u64,
    /// Who this node voted for in the current term (if any).
    pub voted_for: Option<String>,
}

impl FederationNode {
    pub fn new(name: String, address: SocketAddr, capabilities: NodeCapabilities) -> Self {
        Self {
            node_id: Uuid::new_v4().to_string(),
            name,
            address,
            role: NodeRole::Follower,
            status: NodeStatus::Online,
            last_heartbeat: Utc::now(),
            capabilities,
            current_term: 0,
            voted_for: None,
        }
    }
}

// ---------------------------------------------------------------------------
// SchedulingStrategy
// ---------------------------------------------------------------------------

/// Strategy for placing agents across the cluster.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum SchedulingStrategy {
    /// Spread load evenly across nodes.
    #[default]
    Balanced,
    /// Pack agents onto fewest nodes (save power).
    Packed,
    /// Spread agents to maximize isolation.
    Spread,
}

impl fmt::Display for SchedulingStrategy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Balanced => write!(f, "balanced"),
            Self::Packed => write!(f, "packed"),
            Self::Spread => write!(f, "spread"),
        }
    }
}

impl std::str::FromStr for SchedulingStrategy {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "balanced" => Ok(Self::Balanced),
            "packed" => Ok(Self::Packed),
            "spread" => Ok(Self::Spread),
            _ => Err(anyhow::anyhow!("Unknown scheduling strategy: {}", s)),
        }
    }
}

// ---------------------------------------------------------------------------
// FederationConfig
// ---------------------------------------------------------------------------

/// Configuration for federation, parsed from TOML.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationConfig {
    pub enabled: bool,
    pub node_name: String,
    pub bind_addr: SocketAddr,
    pub peers: HashMap<String, SocketAddr>,
    pub scheduling_strategy: SchedulingStrategy,
}

impl Default for FederationConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            node_name: "node-1".to_string(),
            bind_addr: "0.0.0.0:8092".parse().unwrap(),
            peers: HashMap::new(),
            scheduling_strategy: SchedulingStrategy::default(),
        }
    }
}

/// Raw TOML structure for deserialization.
#[derive(Debug, Deserialize)]
struct FederationToml {
    federation: FederationSection,
}

#[derive(Debug, Deserialize)]
struct FederationSection {
    enabled: bool,
    node_name: String,
    bind_addr: String,
    #[serde(default)]
    peers: HashMap<String, String>,
    #[serde(default)]
    scheduling: Option<SchedulingSection>,
}

#[derive(Debug, Deserialize)]
struct SchedulingSection {
    #[serde(default = "default_strategy")]
    strategy: String,
}

fn default_strategy() -> String {
    "balanced".to_string()
}

impl FederationConfig {
    /// Parse configuration from a TOML string.
    pub fn from_toml(toml_str: &str) -> anyhow::Result<Self> {
        let raw: FederationToml = toml::from_str(toml_str)?;
        let section = raw.federation;

        let bind_addr: SocketAddr = section
            .bind_addr
            .parse()
            .map_err(|e| anyhow::anyhow!("Invalid bind_addr '{}': {}", section.bind_addr, e))?;

        let mut peers = HashMap::new();
        for (name, addr_str) in &section.peers {
            let addr: SocketAddr = addr_str.parse().map_err(|e| {
                anyhow::anyhow!("Invalid peer addr '{}' for '{}': {}", addr_str, name, e)
            })?;
            peers.insert(name.clone(), addr);
        }

        let strategy = if let Some(sched) = &section.scheduling {
            sched.strategy.parse()?
        } else {
            SchedulingStrategy::default()
        };

        Ok(Self {
            enabled: section.enabled,
            node_name: section.node_name,
            bind_addr,
            peers,
            scheduling_strategy: strategy,
        })
    }
}

// ---------------------------------------------------------------------------
// VoteResponse
// ---------------------------------------------------------------------------

/// Response to a vote request during coordinator election.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoteResponse {
    pub voter_id: String,
    pub term: u64,
    pub granted: bool,
}

// ---------------------------------------------------------------------------
// FederationStats
// ---------------------------------------------------------------------------

/// Summary statistics for the federation cluster.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationStats {
    pub total_nodes: usize,
    pub live_nodes: usize,
    pub suspect_nodes: usize,
    pub dead_nodes: usize,
    pub coordinator_id: Option<String>,
    pub cluster_uptime_secs: u64,
    pub scheduling_strategy: SchedulingStrategy,
}

// ---------------------------------------------------------------------------
// AgentRequirements
// ---------------------------------------------------------------------------

/// Resource requirements and placement preferences for an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRequirements {
    pub cpu_cores: u32,
    pub memory_mb: u64,
    pub gpu_required: bool,
    /// Preferred node name (locality hint).
    pub preferred_node: Option<String>,
    /// Node names the agent has affinity to (e.g., co-located agents).
    pub affinity_nodes: Vec<String>,
}

impl Default for AgentRequirements {
    fn default() -> Self {
        Self {
            cpu_cores: 1,
            memory_mb: 512,
            gpu_required: false,
            preferred_node: None,
            affinity_nodes: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// NodeScore
// ---------------------------------------------------------------------------

/// Scoring breakdown for a node's suitability for agent placement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeScore {
    pub node_id: String,
    pub total_score: f64,
    pub breakdown: ScoreBreakdown,
}

/// Individual components of a node's placement score.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoreBreakdown {
    /// Resource headroom score (40% weight).
    pub resource_headroom: f64,
    /// Locality score (30% weight).
    pub locality: f64,
    /// Load balance score (20% weight).
    pub load_balance: f64,
    /// Affinity score (10% weight).
    pub affinity: f64,
}

// ---------------------------------------------------------------------------
// Federated Vector Store types
// ---------------------------------------------------------------------------

/// Replication strategy for vector data across federated nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum VectorReplicationStrategy {
    /// Every node holds a full copy of every collection.
    #[default]
    Full,
    /// Each collection lives on N nodes (configurable replication factor).
    Partial { replication_factor: u32 },
    /// Collections are sharded — each node holds a subset of vectors.
    Sharded,
}

/// Tracks which collections are hosted on which nodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionReplica {
    /// Node hosting this replica.
    pub node_id: String,
    /// Address for reaching the node's vector API.
    pub address: SocketAddr,
    /// Number of vectors the node reports for this collection.
    pub vector_count: usize,
    /// Last sync timestamp.
    pub last_synced: DateTime<Utc>,
}

/// A request message sent to peer nodes for vector operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VectorSyncMessage {
    /// Insert vectors into a collection on the remote node.
    Insert {
        collection: String,
        entries: Vec<VectorSyncEntry>,
    },
    /// Search a collection on the remote node.
    Search {
        collection: String,
        query: Vec<f64>,
        top_k: usize,
    },
    /// Delete a vector from a collection.
    Delete {
        collection: String,
        vector_id: String,
    },
    /// Request the full collection manifest (for initial sync).
    SyncManifest { collection: String },
    /// Announce that a collection exists on this node.
    AnnounceCollection {
        collection: String,
        dimension: Option<usize>,
        vector_count: usize,
    },
}

/// A vector entry in wire format (for sync between nodes).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorSyncEntry {
    pub id: String,
    pub embedding: Vec<f64>,
    pub content: String,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

/// A search result from a remote node (wire format).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteSearchResult {
    pub id: String,
    pub score: f64,
    pub content: String,
    pub metadata: serde_json::Value,
    pub source_node: String,
}

/// Statistics for the federated vector store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederatedVectorStats {
    pub collection_count: usize,
    pub total_replicas: usize,
    pub total_vectors_across_replicas: usize,
    pub nodes_with_vectors: usize,
    pub replication_strategy: VectorReplicationStrategy,
}
