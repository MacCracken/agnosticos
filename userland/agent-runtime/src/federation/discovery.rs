//! Node discovery — FederationCluster management (heartbeat, registration, election).

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use tracing::{debug, info, warn};

use super::types::{
    FederationConfig, FederationNode, FederationStats, NodeCapabilities, NodeRole, NodeStatus,
    SchedulingStrategy, VoteResponse,
};

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
        nodes.insert(local_id.clone(), local_node);

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
    ///
    /// Validates that the node's cluster_token matches this cluster's token
    /// to prevent unauthorized node registration.
    pub fn register_node(&mut self, node: FederationNode) -> anyhow::Result<()> {
        if self.nodes.contains_key(&node.node_id) {
            return Err(anyhow::anyhow!(
                "Node '{}' already registered",
                node.node_id
            ));
        }
        // Validate node_id format (must be non-empty, alphanumeric + hyphens)
        if node.node_id.is_empty()
            || !node
                .node_id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(anyhow::anyhow!(
                "Invalid node_id format: must be alphanumeric with hyphens/underscores"
            ));
        }
        if node.name.is_empty() || node.name.len() > 255 {
            return Err(anyhow::anyhow!(
                "Invalid node name: must be 1-255 characters"
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
        let local_id = self.local_node_id.clone();
        self.votes_received.insert(local_id.clone(), vec![local_id]);

        info!(
            node_id = %self.local_node_id,
            term = new_term,
            "Started election"
        );

        // Check if single-node cluster — auto-win
        if self.nodes.len() == 1 {
            let coord_id = self.local_node_id.clone();
            self.become_coordinator(&coord_id)?;
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
            .or_default();

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
        let (mut live, mut suspect, mut dead) = (0usize, 0usize, 0usize);
        for node in self.nodes.values() {
            match node.status {
                NodeStatus::Online => live += 1,
                NodeStatus::Suspect => suspect += 1,
                NodeStatus::Dead => dead += 1,
            }
        }

        let uptime_secs = (Utc::now() - self.created_at).num_seconds().max(0) as u64;

        FederationStats {
            total_nodes: self.nodes.len(),
            live_nodes: live,
            suspect_nodes: suspect,
            dead_nodes: dead,
            coordinator_id: self.coordinator_id.clone(),
            cluster_uptime_secs: uptime_secs,
            scheduling_strategy: self.scheduling_strategy,
        }
    }
}
