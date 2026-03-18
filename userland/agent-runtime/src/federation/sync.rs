//! Agent placement and node scoring logic.

use std::collections::HashMap;

use tracing::info;

use super::discovery::FederationCluster;
use super::types::{AgentRequirements, FederationNode, NodeScore, ScoreBreakdown};

// ---------------------------------------------------------------------------
// NodeScorer
// ---------------------------------------------------------------------------

/// Scores nodes for agent placement using weighted criteria.
pub struct NodeScorer {
    /// Current load on each node (node_id -> number of agents).
    node_loads: HashMap<String, u32>,
}

impl Default for NodeScorer {
    fn default() -> Self {
        Self::new()
    }
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
        let current_load = self.get_load(&node.node_id);

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
