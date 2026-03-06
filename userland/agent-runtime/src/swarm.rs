//! Swarm Intelligence Protocols
//!
//! Enables coordinated behavior across multiple agents through:
//! - Consensus protocols (majority voting, quorum-based decisions)
//! - Task decomposition (split large tasks into subtasks for parallel execution)
//! - Collective decision-making (weighted voting, ranked choice)
//! - Emergent coordination (pheromone-style stigmergy via pub/sub topics)

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use tracing::{debug, info, warn};
use uuid::Uuid;

use agnos_common::AgentId;

/// A proposal submitted to the swarm for collective decision-making.
#[derive(Debug, Clone)]
pub struct SwarmProposal {
    pub id: String,
    pub proposer: AgentId,
    pub description: String,
    pub options: Vec<String>,
    pub created_at: Instant,
    pub deadline: Duration,
    pub quorum: QuorumRule,
    pub votes: HashMap<AgentId, Vote>,
    pub status: ProposalStatus,
}

/// How a quorum is determined.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuorumRule {
    /// Simple majority (>50%)
    Majority,
    /// Two-thirds supermajority
    SuperMajority,
    /// All participants must agree
    Unanimous,
    /// Minimum number of votes required
    MinVotes(usize),
    /// Minimum percentage of eligible voters
    MinPercent(u8),
}

/// A vote cast by an agent.
#[derive(Debug, Clone)]
pub struct Vote {
    pub agent_id: AgentId,
    pub choice: usize,
    pub weight: f64,
    pub reason: Option<String>,
    pub cast_at: Instant,
}

/// Status of a proposal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProposalStatus {
    /// Voting is open.
    Open,
    /// Quorum reached, decision made.
    Decided { winning_option: usize },
    /// Deadline passed without quorum.
    Expired,
    /// Proposal was cancelled by the proposer.
    Cancelled,
    /// Tied — no clear winner.
    Tied,
}

/// A subtask created by decomposing a larger task.
#[derive(Debug, Clone)]
pub struct SubTask {
    pub id: String,
    pub parent_task_id: String,
    pub description: String,
    pub payload: serde_json::Value,
    pub assigned_to: Option<AgentId>,
    pub status: SubTaskStatus,
    pub result: Option<serde_json::Value>,
    pub priority: u8,
    pub dependencies: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubTaskStatus {
    Pending,
    Assigned,
    Running,
    Completed,
    Failed,
}

/// A task decomposition plan.
#[derive(Debug, Clone)]
pub struct DecompositionPlan {
    pub task_id: String,
    pub subtasks: Vec<SubTask>,
    pub strategy: DecompositionStrategy,
    pub aggregation: AggregationStrategy,
    pub created_at: Instant,
}

/// How a task is decomposed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecompositionStrategy {
    /// Split input data into chunks (map-reduce style).
    DataParallel,
    /// Split by processing pipeline stages.
    PipelineParallel,
    /// Each agent handles a different aspect/domain.
    FunctionalSplit,
    /// Redundant execution for fault tolerance.
    Redundant { copies: usize },
}

/// How subtask results are combined.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AggregationStrategy {
    /// Concatenate all results.
    Concatenate,
    /// Take the first successful result.
    FirstSuccess,
    /// Majority vote among results.
    MajorityVote,
    /// Merge results (union of outputs).
    Merge,
    /// Custom aggregation (handled by caller).
    Custom,
}

/// A stigmergy signal — indirect coordination through shared state.
/// Inspired by ant pheromone trails.
#[derive(Debug, Clone)]
pub struct StigmergySignal {
    pub topic: String,
    pub emitter: AgentId,
    pub strength: f64,
    pub data: serde_json::Value,
    pub created_at: Instant,
    pub decay_rate: f64,
}

impl StigmergySignal {
    /// Current effective strength after time decay.
    pub fn current_strength(&self) -> f64 {
        let elapsed = self.created_at.elapsed().as_secs_f64();
        self.strength * (-self.decay_rate * elapsed).exp()
    }

    /// Whether the signal has effectively decayed to zero.
    pub fn is_expired(&self) -> bool {
        self.current_strength() < 0.01
    }
}

/// Manages swarm intelligence protocols.
pub struct SwarmCoordinator {
    proposals: HashMap<String, SwarmProposal>,
    decompositions: HashMap<String, DecompositionPlan>,
    signals: Vec<StigmergySignal>,
    eligible_voters: HashSet<AgentId>,
    max_signals: usize,
}

impl SwarmCoordinator {
    pub fn new() -> Self {
        Self {
            proposals: HashMap::new(),
            decompositions: HashMap::new(),
            signals: Vec::new(),
            eligible_voters: HashSet::new(),
            max_signals: 1000,
        }
    }

    /// Register an agent as eligible to participate in swarm decisions.
    pub fn register_participant(&mut self, agent_id: AgentId) {
        self.eligible_voters.insert(agent_id);
    }

    /// Remove an agent from swarm participation.
    pub fn unregister_participant(&mut self, agent_id: &AgentId) {
        self.eligible_voters.remove(agent_id);
        // Remove their votes from open proposals
        for proposal in self.proposals.values_mut() {
            if proposal.status == ProposalStatus::Open {
                proposal.votes.remove(agent_id);
            }
        }
    }

    /// Create a new proposal for collective decision-making.
    pub fn create_proposal(
        &mut self,
        proposer: AgentId,
        description: String,
        options: Vec<String>,
        deadline: Duration,
        quorum: QuorumRule,
    ) -> Result<String> {
        if options.len() < 2 {
            return Err(anyhow!("Proposal must have at least 2 options"));
        }
        if options.len() > 20 {
            return Err(anyhow!("Proposal cannot have more than 20 options"));
        }

        let id = Uuid::new_v4().to_string();
        let proposal = SwarmProposal {
            id: id.clone(),
            proposer,
            description,
            options,
            created_at: Instant::now(),
            deadline,
            quorum,
            votes: HashMap::new(),
            status: ProposalStatus::Open,
        };

        info!(proposal_id = %id, "Created swarm proposal");
        self.proposals.insert(id.clone(), proposal);
        Ok(id)
    }

    /// Cast a vote on a proposal.
    pub fn cast_vote(
        &mut self,
        proposal_id: &str,
        agent_id: AgentId,
        choice: usize,
        weight: f64,
        reason: Option<String>,
    ) -> Result<()> {
        let proposal = self.proposals.get_mut(proposal_id)
            .ok_or_else(|| anyhow!("Proposal not found: {}", proposal_id))?;

        if proposal.status != ProposalStatus::Open {
            return Err(anyhow!("Proposal is not open for voting"));
        }

        if proposal.created_at.elapsed() > proposal.deadline {
            proposal.status = ProposalStatus::Expired;
            return Err(anyhow!("Proposal voting deadline has passed"));
        }

        if choice >= proposal.options.len() {
            return Err(anyhow!("Invalid choice index: {}", choice));
        }

        if weight <= 0.0 || weight > 10.0 {
            return Err(anyhow!("Vote weight must be in (0, 10]"));
        }

        let vote = Vote {
            agent_id,
            choice,
            weight,
            reason,
            cast_at: Instant::now(),
        };

        proposal.votes.insert(agent_id, vote);
        debug!(proposal_id, %agent_id, choice, "Vote cast");

        // Check if quorum is reached
        self.check_quorum(proposal_id);
        Ok(())
    }

    /// Check if a proposal has reached quorum and resolve it.
    fn check_quorum(&mut self, proposal_id: &str) {
        let proposal = match self.proposals.get_mut(proposal_id) {
            Some(p) if p.status == ProposalStatus::Open => p,
            _ => return,
        };

        let vote_count = proposal.votes.len();
        let eligible_count = self.eligible_voters.len().max(1);
        let vote_pct = (vote_count as f64 / eligible_count as f64 * 100.0).round() as u8;

        let quorum_met = match proposal.quorum {
            QuorumRule::Majority => vote_count * 2 > eligible_count,
            QuorumRule::SuperMajority => vote_count * 3 > eligible_count * 2,
            QuorumRule::Unanimous => vote_count == eligible_count,
            QuorumRule::MinVotes(min) => vote_count >= min,
            QuorumRule::MinPercent(pct) => vote_pct >= pct,
        };

        if !quorum_met {
            return;
        }

        // Tally weighted votes
        let mut tallies: HashMap<usize, f64> = HashMap::new();
        for vote in proposal.votes.values() {
            *tallies.entry(vote.choice).or_insert(0.0) += vote.weight;
        }

        let max_weight = tallies.values().cloned().fold(0.0_f64, f64::max);
        let winners: Vec<usize> = tallies
            .iter()
            .filter(|(_, w)| (**w - max_weight).abs() < f64::EPSILON)
            .map(|(c, _)| *c)
            .collect();

        if winners.len() == 1 {
            proposal.status = ProposalStatus::Decided {
                winning_option: winners[0],
            };
            info!(proposal_id, winner = winners[0], "Proposal decided");
        } else {
            proposal.status = ProposalStatus::Tied;
            warn!(proposal_id, "Proposal resulted in a tie");
        }
    }

    /// Get the status of a proposal.
    pub fn get_proposal(&self, proposal_id: &str) -> Option<&SwarmProposal> {
        self.proposals.get(proposal_id)
    }

    /// Cancel a proposal (only the proposer can cancel).
    pub fn cancel_proposal(&mut self, proposal_id: &str, agent_id: AgentId) -> Result<()> {
        let proposal = self.proposals.get_mut(proposal_id)
            .ok_or_else(|| anyhow!("Proposal not found"))?;
        if proposal.proposer != agent_id {
            return Err(anyhow!("Only the proposer can cancel"));
        }
        proposal.status = ProposalStatus::Cancelled;
        Ok(())
    }

    /// Expire proposals that have passed their deadline.
    pub fn expire_stale_proposals(&mut self) -> usize {
        let mut expired = 0;
        for proposal in self.proposals.values_mut() {
            if proposal.status == ProposalStatus::Open
                && proposal.created_at.elapsed() > proposal.deadline
            {
                proposal.status = ProposalStatus::Expired;
                expired += 1;
            }
        }
        expired
    }

    /// Decompose a task into subtasks for parallel execution.
    pub fn decompose_task(
        &mut self,
        task_id: String,
        subtask_descriptions: Vec<(String, serde_json::Value, Vec<String>)>,
        strategy: DecompositionStrategy,
        aggregation: AggregationStrategy,
    ) -> Result<String> {
        if subtask_descriptions.is_empty() {
            return Err(anyhow!("Must provide at least one subtask"));
        }

        let subtasks: Vec<SubTask> = subtask_descriptions
            .into_iter()
            .enumerate()
            .map(|(i, (desc, payload, deps))| SubTask {
                id: format!("{}-sub-{}", task_id, i),
                parent_task_id: task_id.clone(),
                description: desc,
                payload,
                assigned_to: None,
                status: SubTaskStatus::Pending,
                result: None,
                priority: match strategy {
                    DecompositionStrategy::PipelineParallel => i as u8,
                    _ => 0,
                },
                dependencies: deps,
            })
            .collect();

        // For redundant strategy, duplicate subtasks
        let subtasks = if let DecompositionStrategy::Redundant { copies } = strategy {
            let mut expanded = Vec::new();
            for subtask in &subtasks {
                for copy in 0..copies {
                    let mut clone = subtask.clone();
                    clone.id = format!("{}-copy-{}", subtask.id, copy);
                    expanded.push(clone);
                }
            }
            expanded
        } else {
            subtasks
        };

        let plan = DecompositionPlan {
            task_id: task_id.clone(),
            subtasks,
            strategy,
            aggregation,
            created_at: Instant::now(),
        };

        info!(task_id = %task_id, subtask_count = plan.subtasks.len(), ?strategy, "Task decomposed");
        self.decompositions.insert(task_id.clone(), plan);
        Ok(task_id)
    }

    /// Assign a subtask to an agent.
    pub fn assign_subtask(
        &mut self,
        task_id: &str,
        subtask_id: &str,
        agent_id: AgentId,
    ) -> Result<()> {
        let plan = self.decompositions.get_mut(task_id)
            .ok_or_else(|| anyhow!("Decomposition plan not found: {}", task_id))?;

        // Find the subtask index and collect its dependencies
        let subtask_idx = plan.subtasks.iter()
            .position(|s| s.id == subtask_id)
            .ok_or_else(|| anyhow!("Subtask not found: {}", subtask_id))?;

        let deps: Vec<String> = plan.subtasks[subtask_idx].dependencies.clone();

        // Build O(1) lookup for subtask statuses
        let status_map: HashMap<&str, &SubTaskStatus> = plan.subtasks
            .iter()
            .map(|s| (s.id.as_str(), &s.status))
            .collect();

        // Check dependencies are completed
        for dep_id in &deps {
            if status_map.get(dep_id.as_str()) != Some(&&SubTaskStatus::Completed) {
                return Err(anyhow!("Dependency {} not yet completed", dep_id));
            }
        }

        plan.subtasks[subtask_idx].assigned_to = Some(agent_id);
        plan.subtasks[subtask_idx].status = SubTaskStatus::Assigned;
        Ok(())
    }

    /// Mark a subtask as completed with a result.
    pub fn complete_subtask(
        &mut self,
        task_id: &str,
        subtask_id: &str,
        result: serde_json::Value,
    ) -> Result<bool> {
        let plan = self.decompositions.get_mut(task_id)
            .ok_or_else(|| anyhow!("Decomposition plan not found"))?;

        let subtask = plan.subtasks.iter_mut()
            .find(|s| s.id == subtask_id)
            .ok_or_else(|| anyhow!("Subtask not found"))?;

        subtask.status = SubTaskStatus::Completed;
        subtask.result = Some(result);

        // Check if all subtasks are done
        let all_done = plan.subtasks.iter().all(|s| {
            s.status == SubTaskStatus::Completed || s.status == SubTaskStatus::Failed
        });

        Ok(all_done)
    }

    /// Get ready subtasks (dependencies met, not yet assigned).
    pub fn get_ready_subtasks(&self, task_id: &str) -> Vec<&SubTask> {
        let plan = match self.decompositions.get(task_id) {
            Some(p) => p,
            None => return Vec::new(),
        };

        // Build O(1) lookup for subtask statuses
        let status_map: HashMap<&str, &SubTaskStatus> = plan.subtasks
            .iter()
            .map(|s| (s.id.as_str(), &s.status))
            .collect();

        plan.subtasks
            .iter()
            .filter(|s| s.status == SubTaskStatus::Pending)
            .filter(|s| {
                s.dependencies.iter().all(|dep_id| {
                    status_map.get(dep_id.as_str()) == Some(&&SubTaskStatus::Completed)
                })
            })
            .collect()
    }

    /// Aggregate results from completed subtasks.
    pub fn aggregate_results(&self, task_id: &str) -> Result<serde_json::Value> {
        let plan = self.decompositions.get(task_id)
            .ok_or_else(|| anyhow!("Decomposition plan not found"))?;

        let results: Vec<&serde_json::Value> = plan.subtasks.iter()
            .filter_map(|s| s.result.as_ref())
            .collect();

        if results.is_empty() {
            return Err(anyhow!("No completed subtask results to aggregate"));
        }

        match plan.aggregation {
            AggregationStrategy::Concatenate => {
                Ok(serde_json::Value::Array(results.into_iter().cloned().collect()))
            }
            AggregationStrategy::FirstSuccess => {
                Ok(results[0].clone())
            }
            AggregationStrategy::MajorityVote => {
                // Count occurrences of each distinct result
                let mut counts: HashMap<String, (usize, serde_json::Value)> = HashMap::new();
                for r in &results {
                    let key = r.to_string();
                    counts.entry(key).or_insert((0, (*r).clone())).0 += 1;
                }
                let winner = counts.into_values()
                    .max_by_key(|(count, _)| *count)
                    .map(|(_, val)| val)
                    .unwrap();
                Ok(winner)
            }
            AggregationStrategy::Merge => {
                let mut merged = serde_json::Map::new();
                for r in &results {
                    if let serde_json::Value::Object(obj) = r {
                        for (k, v) in obj {
                            merged.insert(k.clone(), v.clone());
                        }
                    }
                }
                Ok(serde_json::Value::Object(merged))
            }
            AggregationStrategy::Custom => {
                Ok(serde_json::Value::Array(results.into_iter().cloned().collect()))
            }
        }
    }

    /// Emit a stigmergy signal for indirect coordination.
    pub fn emit_signal(
        &mut self,
        topic: String,
        emitter: AgentId,
        strength: f64,
        data: serde_json::Value,
        decay_rate: f64,
    ) {
        let signal = StigmergySignal {
            topic,
            emitter,
            strength: strength.clamp(0.0, 100.0),
            data,
            created_at: Instant::now(),
            decay_rate: decay_rate.clamp(0.001, 10.0),
        };
        self.signals.push(signal);

        // Prune expired signals
        if self.signals.len() > self.max_signals {
            self.signals.retain(|s| !s.is_expired());
        }
    }

    /// Read signals on a topic, sorted by effective strength.
    pub fn read_signals(&self, topic: &str) -> Vec<&StigmergySignal> {
        let mut matching: Vec<&StigmergySignal> = self.signals
            .iter()
            .filter(|s| s.topic == topic && !s.is_expired())
            .collect();
        matching.sort_by(|a, b| b.current_strength().partial_cmp(&a.current_strength()).unwrap_or(std::cmp::Ordering::Equal));
        matching
    }

    /// Get the strongest signal on a topic.
    pub fn strongest_signal(&self, topic: &str) -> Option<&StigmergySignal> {
        self.read_signals(topic).first().copied()
    }

    /// Get decomposition plan.
    pub fn get_plan(&self, task_id: &str) -> Option<&DecompositionPlan> {
        self.decompositions.get(task_id)
    }

    /// Number of active proposals.
    pub fn active_proposal_count(&self) -> usize {
        self.proposals.values().filter(|p| p.status == ProposalStatus::Open).count()
    }

    /// Number of active decompositions.
    pub fn active_decomposition_count(&self) -> usize {
        self.decompositions.len()
    }

    /// Number of registered participants.
    pub fn participant_count(&self) -> usize {
        self.eligible_voters.len()
    }
}

impl Default for SwarmCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(n: u8) -> AgentId {
        AgentId(Uuid::from_bytes([n; 16]))
    }

    #[test]
    fn test_create_proposal() {
        let mut coord = SwarmCoordinator::new();
        let id = coord.create_proposal(
            agent(1),
            "Which model?".into(),
            vec!["GPT-4".into(), "Claude".into()],
            Duration::from_secs(60),
            QuorumRule::Majority,
        ).unwrap();
        assert!(coord.get_proposal(&id).is_some());
        assert_eq!(coord.active_proposal_count(), 1);
    }

    #[test]
    fn test_proposal_requires_two_options() {
        let mut coord = SwarmCoordinator::new();
        let result = coord.create_proposal(
            agent(1), "Bad".into(), vec!["Only one".into()],
            Duration::from_secs(60), QuorumRule::Majority,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_voting_and_decision() {
        let mut coord = SwarmCoordinator::new();
        coord.register_participant(agent(1));
        coord.register_participant(agent(2));
        coord.register_participant(agent(3));

        let id = coord.create_proposal(
            agent(1), "Choose".into(),
            vec!["A".into(), "B".into()],
            Duration::from_secs(60), QuorumRule::Majority,
        ).unwrap();

        coord.cast_vote(&id, agent(1), 0, 1.0, None).unwrap();
        coord.cast_vote(&id, agent(2), 0, 1.0, None).unwrap();

        let proposal = coord.get_proposal(&id).unwrap();
        assert_eq!(proposal.status, ProposalStatus::Decided { winning_option: 0 });
    }

    #[test]
    fn test_voting_tie() {
        let mut coord = SwarmCoordinator::new();
        coord.register_participant(agent(1));
        coord.register_participant(agent(2));

        let id = coord.create_proposal(
            agent(1), "Choose".into(),
            vec!["A".into(), "B".into()],
            Duration::from_secs(60), QuorumRule::MinVotes(2),
        ).unwrap();

        coord.cast_vote(&id, agent(1), 0, 1.0, None).unwrap();
        coord.cast_vote(&id, agent(2), 1, 1.0, None).unwrap();

        assert_eq!(coord.get_proposal(&id).unwrap().status, ProposalStatus::Tied);
    }

    #[test]
    fn test_weighted_voting() {
        let mut coord = SwarmCoordinator::new();
        coord.register_participant(agent(1));
        coord.register_participant(agent(2));

        let id = coord.create_proposal(
            agent(1), "Choose".into(),
            vec!["A".into(), "B".into()],
            Duration::from_secs(60), QuorumRule::MinVotes(2),
        ).unwrap();

        // Agent 1 votes A with weight 1, Agent 2 votes B with weight 5
        coord.cast_vote(&id, agent(1), 0, 1.0, None).unwrap();
        coord.cast_vote(&id, agent(2), 1, 5.0, None).unwrap();

        assert_eq!(
            coord.get_proposal(&id).unwrap().status,
            ProposalStatus::Decided { winning_option: 1 }
        );
    }

    #[test]
    fn test_invalid_vote_choice() {
        let mut coord = SwarmCoordinator::new();
        coord.register_participant(agent(1));
        let id = coord.create_proposal(
            agent(1), "Choose".into(),
            vec!["A".into(), "B".into()],
            Duration::from_secs(60), QuorumRule::MinVotes(1),
        ).unwrap();

        assert!(coord.cast_vote(&id, agent(1), 5, 1.0, None).is_err());
    }

    #[test]
    fn test_cancel_proposal() {
        let mut coord = SwarmCoordinator::new();
        let id = coord.create_proposal(
            agent(1), "Choose".into(),
            vec!["A".into(), "B".into()],
            Duration::from_secs(60), QuorumRule::Majority,
        ).unwrap();

        // Non-proposer can't cancel
        assert!(coord.cancel_proposal(&id, agent(2)).is_err());
        // Proposer can cancel
        coord.cancel_proposal(&id, agent(1)).unwrap();
        assert_eq!(coord.get_proposal(&id).unwrap().status, ProposalStatus::Cancelled);
    }

    #[test]
    fn test_unregister_removes_votes() {
        let mut coord = SwarmCoordinator::new();
        coord.register_participant(agent(1));
        coord.register_participant(agent(2));

        let id = coord.create_proposal(
            agent(1), "Choose".into(),
            vec!["A".into(), "B".into()],
            Duration::from_secs(60), QuorumRule::MinVotes(3),
        ).unwrap();

        coord.cast_vote(&id, agent(1), 0, 1.0, None).unwrap();
        coord.unregister_participant(&agent(1));

        assert!(coord.get_proposal(&id).unwrap().votes.is_empty());
    }

    #[test]
    fn test_task_decomposition() {
        let mut coord = SwarmCoordinator::new();
        let task_id = coord.decompose_task(
            "task-1".into(),
            vec![
                ("Part A".into(), serde_json::json!({"chunk": 0}), vec![]),
                ("Part B".into(), serde_json::json!({"chunk": 1}), vec![]),
                ("Combine".into(), serde_json::json!({}), vec!["task-1-sub-0".into(), "task-1-sub-1".into()]),
            ],
            DecompositionStrategy::DataParallel,
            AggregationStrategy::Concatenate,
        ).unwrap();

        let plan = coord.get_plan(&task_id).unwrap();
        assert_eq!(plan.subtasks.len(), 3);

        // Only first two are ready (third depends on them)
        let ready = coord.get_ready_subtasks(&task_id);
        assert_eq!(ready.len(), 2);
    }

    #[test]
    fn test_subtask_assignment_and_completion() {
        let mut coord = SwarmCoordinator::new();
        coord.decompose_task(
            "t1".into(),
            vec![
                ("A".into(), serde_json::json!({}), vec![]),
                ("B".into(), serde_json::json!({}), vec![]),
            ],
            DecompositionStrategy::DataParallel,
            AggregationStrategy::Concatenate,
        ).unwrap();

        coord.assign_subtask("t1", "t1-sub-0", agent(1)).unwrap();
        let all_done = coord.complete_subtask("t1", "t1-sub-0", serde_json::json!("result-a")).unwrap();
        assert!(!all_done);

        coord.assign_subtask("t1", "t1-sub-1", agent(2)).unwrap();
        let all_done = coord.complete_subtask("t1", "t1-sub-1", serde_json::json!("result-b")).unwrap();
        assert!(all_done);
    }

    #[test]
    fn test_dependency_enforcement() {
        let mut coord = SwarmCoordinator::new();
        coord.decompose_task(
            "t1".into(),
            vec![
                ("First".into(), serde_json::json!({}), vec![]),
                ("Second".into(), serde_json::json!({}), vec!["t1-sub-0".into()]),
            ],
            DecompositionStrategy::PipelineParallel,
            AggregationStrategy::Concatenate,
        ).unwrap();

        // Can't assign second before first completes
        assert!(coord.assign_subtask("t1", "t1-sub-1", agent(1)).is_err());

        // Complete first, then second can be assigned
        coord.assign_subtask("t1", "t1-sub-0", agent(1)).unwrap();
        coord.complete_subtask("t1", "t1-sub-0", serde_json::json!("done")).unwrap();
        assert!(coord.assign_subtask("t1", "t1-sub-1", agent(2)).is_ok());
    }

    #[test]
    fn test_redundant_decomposition() {
        let mut coord = SwarmCoordinator::new();
        coord.decompose_task(
            "t1".into(),
            vec![("Task".into(), serde_json::json!({}), vec![])],
            DecompositionStrategy::Redundant { copies: 3 },
            AggregationStrategy::MajorityVote,
        ).unwrap();

        let plan = coord.get_plan("t1").unwrap();
        assert_eq!(plan.subtasks.len(), 3);
    }

    #[test]
    fn test_aggregate_concatenate() {
        let mut coord = SwarmCoordinator::new();
        coord.decompose_task(
            "t1".into(),
            vec![
                ("A".into(), serde_json::json!({}), vec![]),
                ("B".into(), serde_json::json!({}), vec![]),
            ],
            DecompositionStrategy::DataParallel,
            AggregationStrategy::Concatenate,
        ).unwrap();

        coord.assign_subtask("t1", "t1-sub-0", agent(1)).unwrap();
        coord.complete_subtask("t1", "t1-sub-0", serde_json::json!("r1")).unwrap();
        coord.assign_subtask("t1", "t1-sub-1", agent(2)).unwrap();
        coord.complete_subtask("t1", "t1-sub-1", serde_json::json!("r2")).unwrap();

        let result = coord.aggregate_results("t1").unwrap();
        assert_eq!(result, serde_json::json!(["r1", "r2"]));
    }

    #[test]
    fn test_aggregate_merge() {
        let mut coord = SwarmCoordinator::new();
        coord.decompose_task(
            "t1".into(),
            vec![
                ("A".into(), serde_json::json!({}), vec![]),
                ("B".into(), serde_json::json!({}), vec![]),
            ],
            DecompositionStrategy::FunctionalSplit,
            AggregationStrategy::Merge,
        ).unwrap();

        coord.assign_subtask("t1", "t1-sub-0", agent(1)).unwrap();
        coord.complete_subtask("t1", "t1-sub-0", serde_json::json!({"hosts": 5})).unwrap();
        coord.assign_subtask("t1", "t1-sub-1", agent(2)).unwrap();
        coord.complete_subtask("t1", "t1-sub-1", serde_json::json!({"ports": 80})).unwrap();

        let result = coord.aggregate_results("t1").unwrap();
        assert_eq!(result, serde_json::json!({"hosts": 5, "ports": 80}));
    }

    #[test]
    fn test_stigmergy_signals() {
        let mut coord = SwarmCoordinator::new();
        coord.emit_signal(
            "scan.target".into(), agent(1), 10.0,
            serde_json::json!({"ip": "10.0.0.1"}), 0.1,
        );
        coord.emit_signal(
            "scan.target".into(), agent(2), 5.0,
            serde_json::json!({"ip": "10.0.0.2"}), 0.1,
        );

        let signals = coord.read_signals("scan.target");
        assert_eq!(signals.len(), 2);
        // Stronger signal first
        assert!(signals[0].strength > signals[1].strength);
    }

    #[test]
    fn test_signal_decay() {
        let signal = StigmergySignal {
            topic: "test".into(),
            emitter: agent(1),
            strength: 10.0,
            data: serde_json::json!(null),
            created_at: Instant::now() - Duration::from_secs(100),
            decay_rate: 1.0,
        };
        // After 100 seconds with decay_rate 1.0, strength should be ~0
        assert!(signal.is_expired());
    }

    #[test]
    fn test_signal_no_expired_in_read() {
        let mut coord = SwarmCoordinator::new();
        coord.signals.push(StigmergySignal {
            topic: "test".into(),
            emitter: agent(1),
            strength: 10.0,
            data: serde_json::json!(null),
            created_at: Instant::now() - Duration::from_secs(1000),
            decay_rate: 1.0,
        });
        assert!(coord.read_signals("test").is_empty());
    }

    #[test]
    fn test_expire_stale_proposals() {
        let mut coord = SwarmCoordinator::new();
        coord.proposals.insert("old".into(), SwarmProposal {
            id: "old".into(),
            proposer: agent(1),
            description: "Old".into(),
            options: vec!["A".into(), "B".into()],
            created_at: Instant::now() - Duration::from_secs(120),
            deadline: Duration::from_secs(60),
            quorum: QuorumRule::Majority,
            votes: HashMap::new(),
            status: ProposalStatus::Open,
        });

        let expired = coord.expire_stale_proposals();
        assert_eq!(expired, 1);
    }

    #[test]
    fn test_supermajority_quorum() {
        let mut coord = SwarmCoordinator::new();
        for i in 0..6 {
            coord.register_participant(agent(i));
        }

        let id = coord.create_proposal(
            agent(0), "Choose".into(),
            vec!["A".into(), "B".into()],
            Duration::from_secs(60), QuorumRule::SuperMajority,
        ).unwrap();

        // 3/6 = 50%, not enough for super majority
        coord.cast_vote(&id, agent(0), 0, 1.0, None).unwrap();
        coord.cast_vote(&id, agent(1), 0, 1.0, None).unwrap();
        coord.cast_vote(&id, agent(2), 0, 1.0, None).unwrap();
        assert_eq!(coord.get_proposal(&id).unwrap().status, ProposalStatus::Open);

        // 4/6 = 66.7%, not > 66.7% (need strictly > 2/3)
        coord.cast_vote(&id, agent(3), 0, 1.0, None).unwrap();
        assert_eq!(coord.get_proposal(&id).unwrap().status, ProposalStatus::Open);

        // 5/6 > 2/3
        coord.cast_vote(&id, agent(4), 0, 1.0, None).unwrap();
        assert!(matches!(coord.get_proposal(&id).unwrap().status, ProposalStatus::Decided { .. }));
    }
}
