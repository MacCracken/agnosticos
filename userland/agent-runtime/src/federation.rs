//! Federation — Multi-Node Agent Clusters for AGNOS
//!
//! Implements peer-to-peer federation with coordinator election (simplified Raft),
//! node health monitoring, agent placement scoring, and cluster management.
//! Architecture defined in ADR-016.

use std::collections::HashMap;
use std::fmt;
use std::net::SocketAddr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SchedulingStrategy {
    /// Spread load evenly across nodes.
    Balanced,
    /// Pack agents onto fewest nodes (save power).
    Packed,
    /// Spread agents to maximize isolation.
    Spread,
}

impl Default for SchedulingStrategy {
    fn default() -> Self {
        Self::Balanced
    }
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
// FederationCluster
// ---------------------------------------------------------------------------

/// Manages a set of federation nodes, heartbeat tracking, and coordinator election.
pub struct FederationCluster {
    /// All known nodes indexed by node_id.
    nodes: HashMap<String, FederationNode>,
    /// This node's ID.
    local_node_id: String,
    /// Current coordinator node_id (if elected).
    coordinator_id: Option<String>,
    /// Cluster creation time.
    created_at: DateTime<Utc>,
    /// Votes received in current election (candidate_id -> set of voter_ids).
    votes_received: HashMap<String, Vec<String>>,
    /// Scheduling strategy.
    scheduling_strategy: SchedulingStrategy,
    /// Suspect threshold in seconds.
    suspect_threshold_secs: i64,
    /// Dead threshold in seconds.
    dead_threshold_secs: i64,
}

impl FederationCluster {
    /// Create a new cluster with a local node.
    pub fn new(local_node: FederationNode) -> Self {
        let local_id = local_node.node_id.clone();
        let mut nodes = HashMap::new();
        nodes.insert(local_node.node_id.clone(), local_node);

        info!(node_id = %local_id, "Federation cluster initialised with local node");

        Self {
            nodes,
            local_node_id: local_id,
            coordinator_id: None,
            created_at: Utc::now(),
            votes_received: HashMap::new(),
            scheduling_strategy: SchedulingStrategy::default(),
            suspect_threshold_secs: 15,
            dead_threshold_secs: 30,
        }
    }

    /// Create a cluster from configuration.
    pub fn from_config(config: &FederationConfig, capabilities: NodeCapabilities) -> Self {
        let local_node =
            FederationNode::new(config.node_name.clone(), config.bind_addr, capabilities);
        let mut cluster = Self::new(local_node);
        cluster.scheduling_strategy = config.scheduling_strategy;
        cluster
    }

    /// Register a new node in the cluster.
    pub fn register_node(&mut self, node: FederationNode) -> anyhow::Result<()> {
        if self.nodes.contains_key(&node.node_id) {
            return Err(anyhow::anyhow!(
                "Node '{}' already registered",
                node.node_id
            ));
        }
        info!(node_id = %node.node_id, name = %node.name, "Registered federation node");
        self.nodes.insert(node.node_id.clone(), node);
        Ok(())
    }

    /// Remove a node from the cluster.
    pub fn remove_node(&mut self, node_id: &str) -> anyhow::Result<()> {
        if node_id == self.local_node_id {
            return Err(anyhow::anyhow!("Cannot remove local node"));
        }
        if self.nodes.remove(node_id).is_none() {
            return Err(anyhow::anyhow!("Node '{}' not found", node_id));
        }
        if self.coordinator_id.as_deref() == Some(node_id) {
            warn!(node_id = %node_id, "Removed node was coordinator, clearing coordinator");
            self.coordinator_id = None;
        }
        info!(node_id = %node_id, "Removed federation node");
        Ok(())
    }

    /// Get a node by ID.
    pub fn get_node(&self, node_id: &str) -> Option<&FederationNode> {
        self.nodes.get(node_id)
    }

    /// Get a mutable reference to a node.
    pub fn get_node_mut(&mut self, node_id: &str) -> Option<&mut FederationNode> {
        self.nodes.get_mut(node_id)
    }

    /// Get the local node ID.
    pub fn local_node_id(&self) -> &str {
        &self.local_node_id
    }

    /// Get the current coordinator node ID.
    pub fn coordinator_id(&self) -> Option<&str> {
        self.coordinator_id.as_deref()
    }

    /// Total number of nodes in the cluster.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// All nodes.
    pub fn all_nodes(&self) -> Vec<&FederationNode> {
        self.nodes.values().collect()
    }

    // -----------------------------------------------------------------------
    // Heartbeat & Health
    // -----------------------------------------------------------------------

    /// Record a heartbeat from a node.
    pub fn record_heartbeat(&mut self, node_id: &str) -> anyhow::Result<()> {
        let node = self
            .nodes
            .get_mut(node_id)
            .ok_or_else(|| anyhow::anyhow!("Unknown node '{}'", node_id))?;
        node.last_heartbeat = Utc::now();
        if node.status != NodeStatus::Online {
            info!(node_id = %node_id, old_status = %node.status, "Node recovered to online");
            node.status = NodeStatus::Online;
        }
        debug!(node_id = %node_id, "Heartbeat recorded");
        Ok(())
    }

    /// Check health of all nodes and update their status based on heartbeat age.
    pub fn check_health(&mut self) {
        let now = Utc::now();
        let suspect_threshold = self.suspect_threshold_secs;
        let dead_threshold = self.dead_threshold_secs;

        for node in self.nodes.values_mut() {
            let elapsed = (now - node.last_heartbeat).num_seconds();
            let old_status = node.status;

            if elapsed > dead_threshold {
                node.status = NodeStatus::Dead;
            } else if elapsed > suspect_threshold {
                node.status = NodeStatus::Suspect;
            } else {
                node.status = NodeStatus::Online;
            }

            if old_status != node.status {
                warn!(
                    node_id = %node.node_id,
                    old = %old_status,
                    new = %node.status,
                    elapsed_secs = elapsed,
                    "Node status changed"
                );
            }
        }
    }

    /// Get all nodes with Online status.
    pub fn get_live_nodes(&self) -> Vec<&FederationNode> {
        self.nodes
            .values()
            .filter(|n| n.status == NodeStatus::Online)
            .collect()
    }

    /// Set a node's heartbeat to a specific time (for testing).
    pub fn set_heartbeat_time(&mut self, node_id: &str, time: DateTime<Utc>) -> anyhow::Result<()> {
        let node = self
            .nodes
            .get_mut(node_id)
            .ok_or_else(|| anyhow::anyhow!("Unknown node '{}'", node_id))?;
        node.last_heartbeat = time;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Coordinator Election (simplified Raft)
    // -----------------------------------------------------------------------

    /// Start an election — the local node becomes a candidate and votes for itself.
    /// Returns the new term number.
    pub fn start_election(&mut self) -> anyhow::Result<u64> {
        let local = self
            .nodes
            .get_mut(&self.local_node_id)
            .ok_or_else(|| anyhow::anyhow!("Local node not found"))?;

        let new_term = local.current_term + 1;
        local.current_term = new_term;
        local.role = NodeRole::Candidate;
        local.voted_for = Some(self.local_node_id.clone());

        // Record self-vote
        self.votes_received
            .insert(self.local_node_id.clone(), vec![self.local_node_id.clone()]);

        info!(
            node_id = %self.local_node_id,
            term = new_term,
            "Started election"
        );

        // Check if single-node cluster — auto-win
        if self.nodes.len() == 1 {
            self.become_coordinator(&self.local_node_id.clone())?;
        }

        Ok(new_term)
    }

    /// Process a vote request from a candidate. Returns a VoteResponse.
    /// A node grants its vote if:
    /// 1. The candidate's term >= the node's current term
    /// 2. The node hasn't voted for someone else in this term
    pub fn receive_vote_request(
        &mut self,
        candidate_id: &str,
        candidate_term: u64,
    ) -> VoteResponse {
        let voter_id = self.local_node_id.clone();
        let local = match self.nodes.get_mut(&self.local_node_id) {
            Some(node) => node,
            None => {
                warn!(
                    voter = %voter_id,
                    candidate = %candidate_id,
                    "Local node not found in cluster map — rejecting vote"
                );
                return VoteResponse {
                    voter_id,
                    term: candidate_term,
                    granted: false,
                };
            }
        };

        // If candidate has a stale term, reject
        if candidate_term < local.current_term {
            debug!(
                voter = %voter_id,
                candidate = %candidate_id,
                candidate_term = candidate_term,
                local_term = local.current_term,
                "Rejecting vote — stale term"
            );
            return VoteResponse {
                voter_id,
                term: local.current_term,
                granted: false,
            };
        }

        // If candidate's term is higher, step down and update term
        if candidate_term > local.current_term {
            local.current_term = candidate_term;
            local.voted_for = None;
            if local.role == NodeRole::Coordinator || local.role == NodeRole::Candidate {
                local.role = NodeRole::Follower;
            }
        }

        // Grant vote if we haven't voted in this term, or already voted for this candidate
        let grant = match &local.voted_for {
            None => true,
            Some(voted) => voted == candidate_id,
        };

        if grant {
            local.voted_for = Some(candidate_id.to_string());
            debug!(
                voter = %voter_id,
                candidate = %candidate_id,
                term = candidate_term,
                "Vote granted"
            );
        } else {
            debug!(
                voter = %voter_id,
                candidate = %candidate_id,
                term = candidate_term,
                "Vote denied — already voted for {:?}",
                local.voted_for
            );
        }

        VoteResponse {
            voter_id,
            term: local.current_term,
            granted: grant,
        }
    }

    /// Record a vote received by a candidate.
    /// Returns true if the candidate now has a majority and should become coordinator.
    pub fn receive_vote(&mut self, candidate_id: &str, vote: VoteResponse) -> bool {
        if !vote.granted {
            return false;
        }

        let voters = self
            .votes_received
            .entry(candidate_id.to_string())
            .or_insert_with(Vec::new);

        if !voters.contains(&vote.voter_id) {
            voters.push(vote.voter_id);
        }

        let vote_count = voters.len();
        let majority = self.nodes.len() / 2 + 1;

        debug!(
            candidate = %candidate_id,
            votes = vote_count,
            needed = majority,
            "Vote tally updated"
        );

        vote_count >= majority
    }

    /// Promote a node to coordinator. All other nodes become followers.
    pub fn become_coordinator(&mut self, node_id: &str) -> anyhow::Result<()> {
        if !self.nodes.contains_key(node_id) {
            return Err(anyhow::anyhow!("Node '{}' not found", node_id));
        }

        let term = self.nodes.get(node_id).unwrap().current_term;

        for node in self.nodes.values_mut() {
            if node.node_id == node_id {
                node.role = NodeRole::Coordinator;
            } else {
                node.role = NodeRole::Follower;
                // Update followers' terms to match the new coordinator
                node.current_term = term;
                node.voted_for = None;
            }
        }

        self.coordinator_id = Some(node_id.to_string());
        self.votes_received.clear();

        info!(
            coordinator = %node_id,
            term = term,
            "Node elected as coordinator"
        );

        Ok(())
    }

    /// Step down from coordinator/candidate to follower (e.g., on seeing higher term).
    pub fn step_down(&mut self, node_id: &str, new_term: u64) -> anyhow::Result<()> {
        let node = self
            .nodes
            .get_mut(node_id)
            .ok_or_else(|| anyhow::anyhow!("Node '{}' not found", node_id))?;

        node.role = NodeRole::Follower;
        node.current_term = new_term;
        node.voted_for = None;

        if self.coordinator_id.as_deref() == Some(node_id) {
            self.coordinator_id = None;
        }

        info!(node_id = %node_id, term = new_term, "Node stepped down");
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Stats
    // -----------------------------------------------------------------------

    /// Get cluster statistics.
    pub fn stats(&self) -> FederationStats {
        let live_count = self
            .nodes
            .values()
            .filter(|n| n.status == NodeStatus::Online)
            .count();

        let uptime_secs = (Utc::now() - self.created_at).num_seconds().max(0) as u64;

        FederationStats {
            total_nodes: self.nodes.len(),
            live_nodes: live_count,
            suspect_nodes: self
                .nodes
                .values()
                .filter(|n| n.status == NodeStatus::Suspect)
                .count(),
            dead_nodes: self
                .nodes
                .values()
                .filter(|n| n.status == NodeStatus::Dead)
                .count(),
            coordinator_id: self.coordinator_id.clone(),
            cluster_uptime_secs: uptime_secs,
            scheduling_strategy: self.scheduling_strategy,
        }
    }
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
// NodeScorer
// ---------------------------------------------------------------------------

/// Scores nodes for agent placement using weighted criteria.
pub struct NodeScorer {
    /// Current load on each node (node_id -> number of agents).
    node_loads: HashMap<String, u32>,
}

impl NodeScorer {
    pub fn new() -> Self {
        Self {
            node_loads: HashMap::new(),
        }
    }

    /// Set the current agent load for a node.
    pub fn set_load(&mut self, node_id: &str, agent_count: u32) {
        self.node_loads.insert(node_id.to_string(), agent_count);
    }

    /// Get the current load for a node.
    pub fn get_load(&self, node_id: &str) -> u32 {
        self.node_loads.get(node_id).copied().unwrap_or(0)
    }

    /// Score a single node for a given set of agent requirements.
    ///
    /// Weights: resource headroom 40%, locality 30%, load balance 20%, affinity 10%.
    pub fn score_node(&self, node: &FederationNode, requirements: &AgentRequirements) -> NodeScore {
        let resource_headroom = self.score_resource_headroom(node, requirements);
        let locality = self.score_locality(node, requirements);
        let load_balance = self.score_load_balance(node);
        let affinity = self.score_affinity(node, requirements);

        let total_score =
            resource_headroom * 0.4 + locality * 0.3 + load_balance * 0.2 + affinity * 0.1;

        NodeScore {
            node_id: node.node_id.clone(),
            total_score,
            breakdown: ScoreBreakdown {
                resource_headroom,
                locality,
                load_balance,
                affinity,
            },
        }
    }

    /// Resource headroom: ratio of remaining resources after placing this agent,
    /// accounting for current load on the node.
    fn score_resource_headroom(
        &self,
        node: &FederationNode,
        requirements: &AgentRequirements,
    ) -> f64 {
        let caps = &node.capabilities;
        let current_load = self.get_load(&node.node_id) as u32;

        // Estimate resources already consumed by existing agents.
        // Each running agent is assumed to use 1 CPU core and 512 MB memory
        // (matching AgentRequirements defaults).
        let estimated_cpu_used = current_load;
        let estimated_mem_used = current_load as u64 * 512;

        let effective_cpu = caps.cpu_cores.saturating_sub(estimated_cpu_used);
        let effective_mem = caps.memory_mb.saturating_sub(estimated_mem_used);

        // Check minimum fitness against effective (load-adjusted) resources
        if requirements.cpu_cores > effective_cpu {
            return 0.0;
        }
        if requirements.memory_mb > effective_mem {
            return 0.0;
        }
        if requirements.gpu_required && caps.gpu_count == 0 {
            return 0.0;
        }

        let cpu_headroom =
            (effective_cpu - requirements.cpu_cores) as f64 / caps.cpu_cores.max(1) as f64;
        let mem_headroom =
            (effective_mem - requirements.memory_mb) as f64 / caps.memory_mb.max(1) as f64;

        // Average of CPU and memory headroom
        (cpu_headroom + mem_headroom) / 2.0
    }

    /// Locality: 1.0 if the node matches the preferred node, 0.0 otherwise.
    fn score_locality(&self, node: &FederationNode, requirements: &AgentRequirements) -> f64 {
        match &requirements.preferred_node {
            Some(preferred) if node.name == *preferred => 1.0,
            Some(_) => 0.0,
            None => 0.5, // No preference — neutral
        }
    }

    /// Load balance: lower load = higher score.
    fn score_load_balance(&self, node: &FederationNode) -> f64 {
        let load = self.get_load(&node.node_id);
        // Diminishing returns: each additional agent reduces the score
        1.0 / (1.0 + load as f64)
    }

    /// Affinity: 1.0 if the node is in the affinity set, 0.0 otherwise.
    fn score_affinity(&self, node: &FederationNode, requirements: &AgentRequirements) -> f64 {
        if requirements.affinity_nodes.is_empty() {
            return 0.5; // No affinity — neutral
        }
        if requirements.affinity_nodes.contains(&node.name) {
            1.0
        } else {
            0.0
        }
    }
}

// ---------------------------------------------------------------------------
// AgentPlacement
// ---------------------------------------------------------------------------

/// Agent placement engine — selects the best node for a new agent.
pub struct AgentPlacement {
    scorer: NodeScorer,
}

impl AgentPlacement {
    pub fn new(scorer: NodeScorer) -> Self {
        Self { scorer }
    }

    /// Place an agent on the best available node.
    ///
    /// Filters nodes to those that are online and meet resource requirements,
    /// then scores and returns the best.
    pub fn place_agent(
        &self,
        cluster: &FederationCluster,
        requirements: &AgentRequirements,
    ) -> anyhow::Result<NodeScore> {
        let eligible: Vec<&FederationNode> = cluster
            .get_live_nodes()
            .into_iter()
            .filter(|n| self.node_eligible(n, requirements))
            .collect();

        if eligible.is_empty() {
            return Err(anyhow::anyhow!("No eligible nodes for agent placement"));
        }

        let mut scores: Vec<NodeScore> = eligible
            .iter()
            .map(|n| self.scorer.score_node(n, requirements))
            .collect();

        scores.sort_by(|a, b| {
            b.total_score
                .partial_cmp(&a.total_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let best = scores.into_iter().next().unwrap();

        info!(
            node_id = %best.node_id,
            score = best.total_score,
            "Agent placed on node"
        );

        Ok(best)
    }

    /// Check if a node meets the minimum requirements for an agent.
    fn node_eligible(&self, node: &FederationNode, requirements: &AgentRequirements) -> bool {
        let caps = &node.capabilities;

        if requirements.cpu_cores > caps.cpu_cores {
            return false;
        }
        if requirements.memory_mb > caps.memory_mb {
            return false;
        }
        if requirements.gpu_required && caps.gpu_count == 0 {
            return false;
        }

        true
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn make_node(name: &str, addr: &str) -> FederationNode {
        FederationNode::new(
            name.to_string(),
            addr.parse().unwrap(),
            NodeCapabilities::default(),
        )
    }

    fn make_node_with_caps(name: &str, addr: &str, cpu: u32, mem: u64, gpu: u32) -> FederationNode {
        FederationNode::new(
            name.to_string(),
            addr.parse().unwrap(),
            NodeCapabilities {
                cpu_cores: cpu,
                memory_mb: mem,
                gpu_count: gpu,
            },
        )
    }

    // -------------------------------------------------------------------
    // Node registration
    // -------------------------------------------------------------------

    #[test]
    fn test_node_creation() {
        let node = make_node("test-node", "127.0.0.1:8092");
        assert_eq!(node.name, "test-node");
        assert_eq!(node.role, NodeRole::Follower);
        assert_eq!(node.status, NodeStatus::Online);
        assert_eq!(node.current_term, 0);
        assert!(node.voted_for.is_none());
    }

    #[test]
    fn test_cluster_creation() {
        let node = make_node("node-1", "127.0.0.1:8092");
        let cluster = FederationCluster::new(node);
        assert_eq!(cluster.node_count(), 1);
        assert!(cluster.coordinator_id().is_none());
    }

    #[test]
    fn test_register_node() {
        let local = make_node("node-1", "127.0.0.1:8092");
        let mut cluster = FederationCluster::new(local);

        let peer = make_node("node-2", "127.0.0.2:8092");
        cluster.register_node(peer).unwrap();
        assert_eq!(cluster.node_count(), 2);
    }

    #[test]
    fn test_register_duplicate_node_fails() {
        let local = make_node("node-1", "127.0.0.1:8092");
        let local_id = local.node_id.clone();
        let mut cluster = FederationCluster::new(local);

        // Try to register with the same ID
        let mut dup = make_node("node-1-dup", "127.0.0.1:8093");
        dup.node_id = local_id;
        assert!(cluster.register_node(dup).is_err());
    }

    #[test]
    fn test_remove_node() {
        let local = make_node("node-1", "127.0.0.1:8092");
        let mut cluster = FederationCluster::new(local);

        let peer = make_node("node-2", "127.0.0.2:8092");
        let peer_id = peer.node_id.clone();
        cluster.register_node(peer).unwrap();
        assert_eq!(cluster.node_count(), 2);

        cluster.remove_node(&peer_id).unwrap();
        assert_eq!(cluster.node_count(), 1);
    }

    #[test]
    fn test_remove_local_node_fails() {
        let local = make_node("node-1", "127.0.0.1:8092");
        let local_id = local.node_id.clone();
        let mut cluster = FederationCluster::new(local);
        assert!(cluster.remove_node(&local_id).is_err());
    }

    #[test]
    fn test_remove_unknown_node_fails() {
        let local = make_node("node-1", "127.0.0.1:8092");
        let mut cluster = FederationCluster::new(local);
        assert!(cluster.remove_node("nonexistent").is_err());
    }

    #[test]
    fn test_get_node() {
        let local = make_node("node-1", "127.0.0.1:8092");
        let local_id = local.node_id.clone();
        let cluster = FederationCluster::new(local);

        let node = cluster.get_node(&local_id).unwrap();
        assert_eq!(node.name, "node-1");
        assert!(cluster.get_node("nonexistent").is_none());
    }

    // -------------------------------------------------------------------
    // Heartbeat tracking
    // -------------------------------------------------------------------

    #[test]
    fn test_record_heartbeat() {
        let local = make_node("node-1", "127.0.0.1:8092");
        let local_id = local.node_id.clone();
        let mut cluster = FederationCluster::new(local);

        let before = Utc::now();
        cluster.record_heartbeat(&local_id).unwrap();
        let after = Utc::now();

        let node = cluster.get_node(&local_id).unwrap();
        assert!(node.last_heartbeat >= before);
        assert!(node.last_heartbeat <= after);
    }

    #[test]
    fn test_heartbeat_unknown_node_fails() {
        let local = make_node("node-1", "127.0.0.1:8092");
        let mut cluster = FederationCluster::new(local);
        assert!(cluster.record_heartbeat("nonexistent").is_err());
    }

    #[test]
    fn test_heartbeat_recovers_suspect_node() {
        let local = make_node("node-1", "127.0.0.1:8092");
        let mut cluster = FederationCluster::new(local);

        let mut peer = make_node("node-2", "127.0.0.2:8092");
        peer.status = NodeStatus::Suspect;
        let peer_id = peer.node_id.clone();
        cluster.register_node(peer).unwrap();

        cluster.record_heartbeat(&peer_id).unwrap();
        assert_eq!(
            cluster.get_node(&peer_id).unwrap().status,
            NodeStatus::Online
        );
    }

    // -------------------------------------------------------------------
    // Health transitions
    // -------------------------------------------------------------------

    #[test]
    fn test_health_online_stays_online() {
        let local = make_node("node-1", "127.0.0.1:8092");
        let mut cluster = FederationCluster::new(local);
        cluster.check_health();
        let nodes = cluster.get_live_nodes();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].status, NodeStatus::Online);
    }

    #[test]
    fn test_health_online_to_suspect() {
        let local = make_node("node-1", "127.0.0.1:8092");
        let mut cluster = FederationCluster::new(local);

        let peer = make_node("node-2", "127.0.0.2:8092");
        let peer_id = peer.node_id.clone();
        cluster.register_node(peer).unwrap();

        // Set peer heartbeat to 20 seconds ago
        let old_time = Utc::now() - Duration::seconds(20);
        cluster.set_heartbeat_time(&peer_id, old_time).unwrap();

        cluster.check_health();
        assert_eq!(
            cluster.get_node(&peer_id).unwrap().status,
            NodeStatus::Suspect
        );
    }

    #[test]
    fn test_health_online_to_dead() {
        let local = make_node("node-1", "127.0.0.1:8092");
        let mut cluster = FederationCluster::new(local);

        let peer = make_node("node-2", "127.0.0.2:8092");
        let peer_id = peer.node_id.clone();
        cluster.register_node(peer).unwrap();

        // Set peer heartbeat to 35 seconds ago
        let old_time = Utc::now() - Duration::seconds(35);
        cluster.set_heartbeat_time(&peer_id, old_time).unwrap();

        cluster.check_health();
        assert_eq!(cluster.get_node(&peer_id).unwrap().status, NodeStatus::Dead);
    }

    #[test]
    fn test_health_suspect_to_dead() {
        let local = make_node("node-1", "127.0.0.1:8092");
        let mut cluster = FederationCluster::new(local);

        let mut peer = make_node("node-2", "127.0.0.2:8092");
        peer.status = NodeStatus::Suspect;
        let peer_id = peer.node_id.clone();
        cluster.register_node(peer).unwrap();

        // Set heartbeat well past dead threshold
        let old_time = Utc::now() - Duration::seconds(45);
        cluster.set_heartbeat_time(&peer_id, old_time).unwrap();

        cluster.check_health();
        assert_eq!(cluster.get_node(&peer_id).unwrap().status, NodeStatus::Dead);
    }

    #[test]
    fn test_get_live_nodes_filters_dead() {
        let local = make_node("node-1", "127.0.0.1:8092");
        let mut cluster = FederationCluster::new(local);

        let peer = make_node("node-2", "127.0.0.2:8092");
        let peer_id = peer.node_id.clone();
        cluster.register_node(peer).unwrap();

        // Mark peer dead via old heartbeat
        let old_time = Utc::now() - Duration::seconds(60);
        cluster.set_heartbeat_time(&peer_id, old_time).unwrap();
        cluster.check_health();

        let live = cluster.get_live_nodes();
        assert_eq!(live.len(), 1);
        assert_eq!(live[0].name, "node-1");
    }

    // -------------------------------------------------------------------
    // Coordinator election
    // -------------------------------------------------------------------

    #[test]
    fn test_single_node_election() {
        let local = make_node("node-1", "127.0.0.1:8092");
        let local_id = local.node_id.clone();
        let mut cluster = FederationCluster::new(local);

        let term = cluster.start_election().unwrap();
        assert_eq!(term, 1);
        assert_eq!(cluster.coordinator_id(), Some(local_id.as_str()));
        assert_eq!(
            cluster.get_node(&local_id).unwrap().role,
            NodeRole::Coordinator
        );
    }

    #[test]
    fn test_two_node_election_with_vote() {
        let local = make_node("node-1", "127.0.0.1:8092");
        let local_id = local.node_id.clone();
        let mut cluster = FederationCluster::new(local);

        let peer = make_node("node-2", "127.0.0.2:8092");
        let peer_id = peer.node_id.clone();
        cluster.register_node(peer).unwrap();

        let term = cluster.start_election().unwrap();
        assert_eq!(term, 1);

        // Candidate has 1 self-vote, needs 2 (majority of 2 = 2)
        assert!(
            cluster.coordinator_id().is_none()
                || cluster.coordinator_id() == Some(local_id.as_str())
        );

        // Simulate peer voting for local
        let vote = VoteResponse {
            voter_id: peer_id.clone(),
            term: 1,
            granted: true,
        };
        let has_majority = cluster.receive_vote(&local_id, vote);
        assert!(has_majority);

        cluster.become_coordinator(&local_id).unwrap();
        assert_eq!(cluster.coordinator_id(), Some(local_id.as_str()));
        assert_eq!(
            cluster.get_node(&local_id).unwrap().role,
            NodeRole::Coordinator
        );
        assert_eq!(cluster.get_node(&peer_id).unwrap().role, NodeRole::Follower);
    }

    #[test]
    fn test_three_node_election_majority() {
        let local = make_node("node-1", "127.0.0.1:8092");
        let local_id = local.node_id.clone();
        let mut cluster = FederationCluster::new(local);

        let peer2 = make_node("node-2", "127.0.0.2:8092");
        let peer2_id = peer2.node_id.clone();
        cluster.register_node(peer2).unwrap();

        let peer3 = make_node("node-3", "127.0.0.3:8092");
        let _peer3_id = peer3.node_id.clone();
        cluster.register_node(peer3).unwrap();

        cluster.start_election().unwrap();

        // Self-vote gives 1 out of 3 — not majority
        assert!(cluster.coordinator_id().is_none());

        // One more vote gives majority (2 of 3)
        let vote = VoteResponse {
            voter_id: peer2_id.clone(),
            term: 1,
            granted: true,
        };
        let has_majority = cluster.receive_vote(&local_id, vote);
        assert!(has_majority);
    }

    #[test]
    fn test_competing_candidates_higher_term_wins() {
        let local = make_node("node-1", "127.0.0.1:8092");
        let local_id = local.node_id.clone();
        let mut cluster = FederationCluster::new(local);

        let peer = make_node("node-2", "127.0.0.2:8092");
        let _peer_id = peer.node_id.clone();
        cluster.register_node(peer).unwrap();

        // Local starts election at term 1
        cluster.start_election().unwrap();
        assert_eq!(cluster.get_node(&local_id).unwrap().current_term, 1);

        // Peer requests vote at term 2 — local should step down and grant
        let response = cluster.receive_vote_request("external-candidate", 2);
        assert!(response.granted);
        assert_eq!(
            cluster.get_node(&local_id).unwrap().role,
            NodeRole::Follower
        );
        assert_eq!(cluster.get_node(&local_id).unwrap().current_term, 2);
    }

    #[test]
    fn test_stale_term_vote_rejected() {
        let local = make_node("node-1", "127.0.0.1:8092");
        let mut cluster = FederationCluster::new(local);

        // Advance term
        cluster.start_election().unwrap();

        // Request vote with stale term 0
        let response = cluster.receive_vote_request("stale-candidate", 0);
        assert!(!response.granted);
        assert_eq!(response.term, 1);
    }

    #[test]
    fn test_double_vote_same_term_denied() {
        let local = make_node("node-1", "127.0.0.1:8092");
        let mut cluster = FederationCluster::new(local);

        // Vote for candidate-A in term 1
        let resp1 = cluster.receive_vote_request("candidate-a", 1);
        assert!(resp1.granted);

        // Try to vote for candidate-B in same term 1
        let resp2 = cluster.receive_vote_request("candidate-b", 1);
        assert!(!resp2.granted);
    }

    #[test]
    fn test_vote_for_same_candidate_twice_ok() {
        let local = make_node("node-1", "127.0.0.1:8092");
        let mut cluster = FederationCluster::new(local);

        let resp1 = cluster.receive_vote_request("candidate-a", 1);
        assert!(resp1.granted);

        // Same candidate, same term — should still be granted
        let resp2 = cluster.receive_vote_request("candidate-a", 1);
        assert!(resp2.granted);
    }

    #[test]
    fn test_term_advancement_on_election() {
        let local = make_node("node-1", "127.0.0.1:8092");
        let local_id = local.node_id.clone();
        let mut cluster = FederationCluster::new(local);

        let t1 = cluster.start_election().unwrap();
        assert_eq!(t1, 1);

        // Start another election
        // Reset role to follower first
        cluster.step_down(&local_id, 1).unwrap();
        let t2 = cluster.start_election().unwrap();
        assert_eq!(t2, 2);
    }

    #[test]
    fn test_step_down_clears_coordinator() {
        let local = make_node("node-1", "127.0.0.1:8092");
        let local_id = local.node_id.clone();
        let mut cluster = FederationCluster::new(local);

        cluster.start_election().unwrap();
        assert!(cluster.coordinator_id().is_some());

        cluster.step_down(&local_id, 2).unwrap();
        assert!(cluster.coordinator_id().is_none());
        assert_eq!(
            cluster.get_node(&local_id).unwrap().role,
            NodeRole::Follower
        );
    }

    #[test]
    fn test_remove_coordinator_clears_coordinator_id() {
        let local = make_node("node-1", "127.0.0.1:8092");
        let mut cluster = FederationCluster::new(local);

        let peer = make_node("node-2", "127.0.0.2:8092");
        let peer_id = peer.node_id.clone();
        cluster.register_node(peer).unwrap();

        // Make peer the coordinator
        cluster.get_node_mut(&peer_id).unwrap().current_term = 1;
        cluster.become_coordinator(&peer_id).unwrap();
        assert_eq!(cluster.coordinator_id(), Some(peer_id.as_str()));

        cluster.remove_node(&peer_id).unwrap();
        assert!(cluster.coordinator_id().is_none());
    }

    #[test]
    fn test_denied_vote_not_counted() {
        let local = make_node("node-1", "127.0.0.1:8092");
        let local_id = local.node_id.clone();
        let mut cluster = FederationCluster::new(local);

        let peer = make_node("node-2", "127.0.0.2:8092");
        let peer_id = peer.node_id.clone();
        cluster.register_node(peer).unwrap();

        cluster.start_election().unwrap();

        let vote = VoteResponse {
            voter_id: peer_id,
            term: 1,
            granted: false,
        };
        let has_majority = cluster.receive_vote(&local_id, vote);
        assert!(!has_majority);
    }

    // -------------------------------------------------------------------
    // Node scoring
    // -------------------------------------------------------------------

    #[test]
    fn test_score_node_basic() {
        let node = make_node_with_caps("node-1", "127.0.0.1:8092", 8, 16384, 0);
        let scorer = NodeScorer::new();
        let reqs = AgentRequirements::default();

        let score = scorer.score_node(&node, &reqs);
        assert!(score.total_score > 0.0);
        assert!(score.total_score <= 1.0);
    }

    #[test]
    fn test_score_insufficient_cpu() {
        let node = make_node_with_caps("node-1", "127.0.0.1:8092", 1, 16384, 0);
        let scorer = NodeScorer::new();
        let reqs = AgentRequirements {
            cpu_cores: 4,
            ..Default::default()
        };

        let score = scorer.score_node(&node, &reqs);
        assert_eq!(score.breakdown.resource_headroom, 0.0);
    }

    #[test]
    fn test_score_insufficient_memory() {
        let node = make_node_with_caps("node-1", "127.0.0.1:8092", 8, 256, 0);
        let scorer = NodeScorer::new();
        let reqs = AgentRequirements {
            memory_mb: 512,
            ..Default::default()
        };

        let score = scorer.score_node(&node, &reqs);
        assert_eq!(score.breakdown.resource_headroom, 0.0);
    }

    #[test]
    fn test_score_gpu_required_but_absent() {
        let node = make_node_with_caps("node-1", "127.0.0.1:8092", 8, 16384, 0);
        let scorer = NodeScorer::new();
        let reqs = AgentRequirements {
            gpu_required: true,
            ..Default::default()
        };

        let score = scorer.score_node(&node, &reqs);
        assert_eq!(score.breakdown.resource_headroom, 0.0);
    }

    #[test]
    fn test_score_locality_preferred_match() {
        let node = make_node_with_caps("gpu-node", "127.0.0.1:8092", 8, 16384, 2);
        let scorer = NodeScorer::new();
        let reqs = AgentRequirements {
            preferred_node: Some("gpu-node".to_string()),
            ..Default::default()
        };

        let score = scorer.score_node(&node, &reqs);
        assert_eq!(score.breakdown.locality, 1.0);
    }

    #[test]
    fn test_score_locality_preferred_mismatch() {
        let node = make_node_with_caps("cpu-node", "127.0.0.1:8092", 8, 16384, 0);
        let scorer = NodeScorer::new();
        let reqs = AgentRequirements {
            preferred_node: Some("gpu-node".to_string()),
            ..Default::default()
        };

        let score = scorer.score_node(&node, &reqs);
        assert_eq!(score.breakdown.locality, 0.0);
    }

    #[test]
    fn test_score_locality_no_preference() {
        let node = make_node_with_caps("node-1", "127.0.0.1:8092", 8, 16384, 0);
        let scorer = NodeScorer::new();
        let reqs = AgentRequirements::default();

        let score = scorer.score_node(&node, &reqs);
        assert_eq!(score.breakdown.locality, 0.5);
    }

    #[test]
    fn test_score_load_balance() {
        let node = make_node_with_caps("node-1", "127.0.0.1:8092", 8, 16384, 0);
        let mut scorer = NodeScorer::new();

        let reqs = AgentRequirements::default();

        // No load — full score
        let score0 = scorer.score_node(&node, &reqs);
        assert_eq!(score0.breakdown.load_balance, 1.0);

        // Some load
        scorer.set_load(&node.node_id, 3);
        let score3 = scorer.score_node(&node, &reqs);
        assert!(score3.breakdown.load_balance < score0.breakdown.load_balance);
    }

    #[test]
    fn test_score_affinity_match() {
        let node = make_node_with_caps("node-1", "127.0.0.1:8092", 8, 16384, 0);
        let scorer = NodeScorer::new();
        let reqs = AgentRequirements {
            affinity_nodes: vec!["node-1".to_string()],
            ..Default::default()
        };

        let score = scorer.score_node(&node, &reqs);
        assert_eq!(score.breakdown.affinity, 1.0);
    }

    #[test]
    fn test_score_affinity_no_match() {
        let node = make_node_with_caps("node-1", "127.0.0.1:8092", 8, 16384, 0);
        let scorer = NodeScorer::new();
        let reqs = AgentRequirements {
            affinity_nodes: vec!["node-2".to_string()],
            ..Default::default()
        };

        let score = scorer.score_node(&node, &reqs);
        assert_eq!(score.breakdown.affinity, 0.0);
    }

    // -------------------------------------------------------------------
    // Agent placement
    // -------------------------------------------------------------------

    #[test]
    fn test_place_agent_single_node() {
        let local = make_node_with_caps("node-1", "127.0.0.1:8092", 8, 16384, 0);
        let local_id = local.node_id.clone();
        let cluster = FederationCluster::new(local);

        let scorer = NodeScorer::new();
        let placement = AgentPlacement::new(scorer);
        let reqs = AgentRequirements::default();

        let result = placement.place_agent(&cluster, &reqs).unwrap();
        assert_eq!(result.node_id, local_id);
    }

    #[test]
    fn test_place_agent_prefers_better_node() {
        let local = make_node_with_caps("small-node", "127.0.0.1:8092", 2, 2048, 0);
        let mut cluster = FederationCluster::new(local);

        let big = make_node_with_caps("big-node", "127.0.0.2:8092", 16, 65536, 0);
        let big_id = big.node_id.clone();
        cluster.register_node(big).unwrap();

        let scorer = NodeScorer::new();
        let placement = AgentPlacement::new(scorer);
        let reqs = AgentRequirements {
            cpu_cores: 2,
            memory_mb: 1024,
            ..Default::default()
        };

        let result = placement.place_agent(&cluster, &reqs).unwrap();
        // Big node should score higher due to more headroom
        assert_eq!(result.node_id, big_id);
    }

    #[test]
    fn test_place_agent_no_eligible_nodes() {
        let local = make_node_with_caps("tiny", "127.0.0.1:8092", 1, 512, 0);
        let cluster = FederationCluster::new(local);

        let scorer = NodeScorer::new();
        let placement = AgentPlacement::new(scorer);
        let reqs = AgentRequirements {
            cpu_cores: 8,
            memory_mb: 32768,
            ..Default::default()
        };

        assert!(placement.place_agent(&cluster, &reqs).is_err());
    }

    #[test]
    fn test_place_agent_respects_gpu_requirement() {
        let cpu_node = make_node_with_caps("cpu-node", "127.0.0.1:8092", 16, 65536, 0);
        let mut cluster = FederationCluster::new(cpu_node);

        let gpu_node = make_node_with_caps("gpu-node", "127.0.0.2:8092", 8, 32768, 2);
        let gpu_id = gpu_node.node_id.clone();
        cluster.register_node(gpu_node).unwrap();

        let scorer = NodeScorer::new();
        let placement = AgentPlacement::new(scorer);
        let reqs = AgentRequirements {
            gpu_required: true,
            ..Default::default()
        };

        let result = placement.place_agent(&cluster, &reqs).unwrap();
        assert_eq!(result.node_id, gpu_id);
    }

    #[test]
    fn test_place_agent_dead_nodes_excluded() {
        let local = make_node_with_caps("node-1", "127.0.0.1:8092", 4, 8192, 0);
        let local_id = local.node_id.clone();
        let mut cluster = FederationCluster::new(local);

        let peer = make_node_with_caps("node-2", "127.0.0.2:8092", 16, 65536, 0);
        let peer_id = peer.node_id.clone();
        cluster.register_node(peer).unwrap();

        // Kill peer
        let old_time = Utc::now() - Duration::seconds(60);
        cluster.set_heartbeat_time(&peer_id, old_time).unwrap();
        cluster.check_health();

        let scorer = NodeScorer::new();
        let placement = AgentPlacement::new(scorer);
        let reqs = AgentRequirements::default();

        let result = placement.place_agent(&cluster, &reqs).unwrap();
        assert_eq!(result.node_id, local_id);
    }

    // -------------------------------------------------------------------
    // Config parsing
    // -------------------------------------------------------------------

    #[test]
    fn test_config_from_toml_full() {
        let toml_str = r#"
[federation]
enabled = true
node_name = "node-1"
bind_addr = "0.0.0.0:8092"

[federation.peers]
"node-2" = "192.168.1.102:8092"
"node-3" = "192.168.1.103:8092"

[federation.scheduling]
strategy = "packed"
"#;
        let config = FederationConfig::from_toml(toml_str).unwrap();
        assert!(config.enabled);
        assert_eq!(config.node_name, "node-1");
        assert_eq!(
            config.bind_addr,
            "0.0.0.0:8092".parse::<SocketAddr>().unwrap()
        );
        assert_eq!(config.peers.len(), 2);
        assert_eq!(config.scheduling_strategy, SchedulingStrategy::Packed);
    }

    #[test]
    fn test_config_from_toml_minimal() {
        let toml_str = r#"
[federation]
enabled = false
node_name = "solo"
bind_addr = "127.0.0.1:8092"
"#;
        let config = FederationConfig::from_toml(toml_str).unwrap();
        assert!(!config.enabled);
        assert_eq!(config.node_name, "solo");
        assert!(config.peers.is_empty());
        assert_eq!(config.scheduling_strategy, SchedulingStrategy::Balanced);
    }

    #[test]
    fn test_config_from_toml_invalid_addr() {
        let toml_str = r#"
[federation]
enabled = true
node_name = "bad"
bind_addr = "not-an-addr"
"#;
        assert!(FederationConfig::from_toml(toml_str).is_err());
    }

    #[test]
    fn test_config_from_toml_invalid_strategy() {
        let toml_str = r#"
[federation]
enabled = true
node_name = "bad"
bind_addr = "0.0.0.0:8092"

[federation.scheduling]
strategy = "yolo"
"#;
        assert!(FederationConfig::from_toml(toml_str).is_err());
    }

    #[test]
    fn test_config_default() {
        let config = FederationConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.scheduling_strategy, SchedulingStrategy::Balanced);
    }

    // -------------------------------------------------------------------
    // Scheduling strategy parsing
    // -------------------------------------------------------------------

    #[test]
    fn test_scheduling_strategy_from_str() {
        assert_eq!(
            "balanced".parse::<SchedulingStrategy>().unwrap(),
            SchedulingStrategy::Balanced
        );
        assert_eq!(
            "packed".parse::<SchedulingStrategy>().unwrap(),
            SchedulingStrategy::Packed
        );
        assert_eq!(
            "spread".parse::<SchedulingStrategy>().unwrap(),
            SchedulingStrategy::Spread
        );
        assert_eq!(
            "BALANCED".parse::<SchedulingStrategy>().unwrap(),
            SchedulingStrategy::Balanced
        );
        assert!("invalid".parse::<SchedulingStrategy>().is_err());
    }

    // -------------------------------------------------------------------
    // Stats
    // -------------------------------------------------------------------

    #[test]
    fn test_stats_single_node() {
        let local = make_node("node-1", "127.0.0.1:8092");
        let cluster = FederationCluster::new(local);

        let stats = cluster.stats();
        assert_eq!(stats.total_nodes, 1);
        assert_eq!(stats.live_nodes, 1);
        assert_eq!(stats.suspect_nodes, 0);
        assert_eq!(stats.dead_nodes, 0);
        assert!(stats.coordinator_id.is_none());
    }

    #[test]
    fn test_stats_with_dead_node() {
        let local = make_node("node-1", "127.0.0.1:8092");
        let mut cluster = FederationCluster::new(local);

        let peer = make_node("node-2", "127.0.0.2:8092");
        let peer_id = peer.node_id.clone();
        cluster.register_node(peer).unwrap();

        let old_time = Utc::now() - Duration::seconds(60);
        cluster.set_heartbeat_time(&peer_id, old_time).unwrap();
        cluster.check_health();

        let stats = cluster.stats();
        assert_eq!(stats.total_nodes, 2);
        assert_eq!(stats.live_nodes, 1);
        assert_eq!(stats.dead_nodes, 1);
    }

    #[test]
    fn test_stats_with_coordinator() {
        let local = make_node("node-1", "127.0.0.1:8092");
        let local_id = local.node_id.clone();
        let mut cluster = FederationCluster::new(local);

        cluster.start_election().unwrap();
        let stats = cluster.stats();
        assert_eq!(stats.coordinator_id, Some(local_id));
    }

    // -------------------------------------------------------------------
    // Edge cases
    // -------------------------------------------------------------------

    #[test]
    fn test_from_config() {
        let config = FederationConfig {
            enabled: true,
            node_name: "test-node".to_string(),
            bind_addr: "0.0.0.0:8092".parse().unwrap(),
            peers: HashMap::new(),
            scheduling_strategy: SchedulingStrategy::Spread,
        };
        let caps = NodeCapabilities {
            cpu_cores: 16,
            memory_mb: 65536,
            gpu_count: 4,
        };
        let cluster = FederationCluster::from_config(&config, caps);

        assert_eq!(cluster.node_count(), 1);
        let local = cluster.get_node(cluster.local_node_id()).unwrap();
        assert_eq!(local.name, "test-node");
        assert_eq!(local.capabilities.gpu_count, 4);
    }

    #[test]
    fn test_node_capabilities_default() {
        let caps = NodeCapabilities::default();
        assert_eq!(caps.cpu_cores, 4);
        assert_eq!(caps.memory_mb, 8192);
        assert_eq!(caps.gpu_count, 0);
    }

    #[test]
    fn test_become_coordinator_unknown_node_fails() {
        let local = make_node("node-1", "127.0.0.1:8092");
        let mut cluster = FederationCluster::new(local);
        assert!(cluster.become_coordinator("nonexistent").is_err());
    }

    #[test]
    fn test_display_traits() {
        assert_eq!(format!("{}", NodeRole::Coordinator), "coordinator");
        assert_eq!(format!("{}", NodeRole::Follower), "follower");
        assert_eq!(format!("{}", NodeRole::Candidate), "candidate");
        assert_eq!(format!("{}", NodeStatus::Online), "online");
        assert_eq!(format!("{}", NodeStatus::Suspect), "suspect");
        assert_eq!(format!("{}", NodeStatus::Dead), "dead");
        assert_eq!(format!("{}", SchedulingStrategy::Balanced), "balanced");
        assert_eq!(format!("{}", SchedulingStrategy::Packed), "packed");
        assert_eq!(format!("{}", SchedulingStrategy::Spread), "spread");
    }
}
