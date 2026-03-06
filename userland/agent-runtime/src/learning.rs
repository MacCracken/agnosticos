//! Agent Learning and Adaptation
//!
//! Tracks agent performance over time and enables adaptive behavior:
//! - Performance profiling (success rates, latency, resource usage)
//! - Strategy selection (choose best approach based on history)
//! - Reward signals for reinforcement-style feedback
//! - Capability scoring (dynamic confidence per skill)

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

use tracing::debug;

use agnos_common::AgentId;

/// A recorded outcome of an agent action.
#[derive(Debug, Clone)]
pub struct ActionOutcome {
    pub agent_id: AgentId,
    pub action_type: String,
    pub strategy: String,
    pub success: bool,
    pub duration: Duration,
    pub reward: f64,
    pub metadata: serde_json::Value,
    pub recorded_at: Instant,
}

/// Aggregated performance statistics for an agent on a specific action type.
#[derive(Debug, Clone)]
pub struct PerformanceProfile {
    pub agent_id: AgentId,
    pub action_type: String,
    pub total_attempts: u64,
    pub successes: u64,
    pub total_reward: f64,
    pub avg_duration_ms: f64,
    pub min_duration_ms: f64,
    pub max_duration_ms: f64,
    pub last_updated: Instant,
}

impl PerformanceProfile {
    fn new(agent_id: AgentId, action_type: String) -> Self {
        Self {
            agent_id,
            action_type,
            total_attempts: 0,
            successes: 0,
            total_reward: 0.0,
            avg_duration_ms: 0.0,
            min_duration_ms: f64::MAX,
            max_duration_ms: 0.0,
            last_updated: Instant::now(),
        }
    }

    /// Success rate as a fraction [0.0, 1.0].
    pub fn success_rate(&self) -> f64 {
        if self.total_attempts == 0 {
            return 0.0;
        }
        self.successes as f64 / self.total_attempts as f64
    }

    /// Average reward per attempt.
    pub fn avg_reward(&self) -> f64 {
        if self.total_attempts == 0 {
            return 0.0;
        }
        self.total_reward / self.total_attempts as f64
    }

    /// Update profile with a new outcome.
    fn record(&mut self, outcome: &ActionOutcome) {
        self.total_attempts += 1;
        if outcome.success {
            self.successes += 1;
        }
        self.total_reward += outcome.reward;

        let dur_ms = outcome.duration.as_secs_f64() * 1000.0;
        self.min_duration_ms = self.min_duration_ms.min(dur_ms);
        self.max_duration_ms = self.max_duration_ms.max(dur_ms);
        // Running average
        self.avg_duration_ms = self.avg_duration_ms
            + (dur_ms - self.avg_duration_ms) / self.total_attempts as f64;
        self.last_updated = Instant::now();
    }
}

/// Per-strategy performance for multi-armed bandit selection.
#[derive(Debug, Clone)]
pub struct StrategyStats {
    pub name: String,
    pub attempts: u64,
    pub successes: u64,
    pub total_reward: f64,
}

impl StrategyStats {
    fn new(name: String) -> Self {
        Self {
            name,
            attempts: 0,
            successes: 0,
            total_reward: 0.0,
        }
    }

    /// Upper confidence bound (UCB1) score for exploration/exploitation balance.
    pub fn ucb1_score(&self, total_attempts: u64) -> f64 {
        if self.attempts == 0 {
            return f64::MAX; // Always try unexplored strategies
        }
        let avg_reward = self.total_reward / self.attempts as f64;
        let exploration = (2.0 * (total_attempts as f64).ln() / self.attempts as f64).sqrt();
        avg_reward + exploration
    }

    /// Simple average reward.
    pub fn avg_reward(&self) -> f64 {
        if self.attempts == 0 {
            return 0.0;
        }
        self.total_reward / self.attempts as f64
    }
}

/// Dynamic capability confidence — how well an agent performs a specific skill.
#[derive(Debug, Clone)]
pub struct CapabilityScore {
    pub capability: String,
    pub confidence: f64,
    pub sample_count: u64,
    pub trend: ScoreTrend,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScoreTrend {
    Improving,
    Stable,
    Declining,
    Unknown,
}

/// Manages agent learning and adaptation across the swarm.
pub struct AgentLearner {
    /// Per-agent, per-action-type performance profiles.
    profiles: HashMap<(AgentId, String), PerformanceProfile>,
    /// Per-action-type strategy statistics for bandit selection.
    strategies: HashMap<String, HashMap<String, StrategyStats>>,
    /// Per-agent capability confidence scores.
    capabilities: HashMap<(AgentId, String), CapabilityScore>,
    /// Recent outcomes for windowed analysis.
    recent_outcomes: VecDeque<ActionOutcome>,
    /// Maximum recent outcomes to retain.
    max_recent: usize,
    /// Exponential moving average decay factor for capability scores.
    ema_alpha: f64,
}

impl AgentLearner {
    pub fn new() -> Self {
        Self {
            profiles: HashMap::new(),
            strategies: HashMap::new(),
            capabilities: HashMap::new(),
            recent_outcomes: VecDeque::new(),
            max_recent: 10_000,
            ema_alpha: 0.1,
        }
    }

    /// Record an action outcome for an agent.
    pub fn record_outcome(&mut self, outcome: ActionOutcome) {
        let key = (outcome.agent_id, outcome.action_type.clone());

        // Update performance profile
        let profile = self.profiles
            .entry(key)
            .or_insert_with(|| PerformanceProfile::new(outcome.agent_id, outcome.action_type.clone()));
        profile.record(&outcome);

        // Update strategy stats
        let action_strategies = self.strategies
            .entry(outcome.action_type.clone())
            .or_default();
        let stats = action_strategies
            .entry(outcome.strategy.clone())
            .or_insert_with(|| StrategyStats::new(outcome.strategy.clone()));
        stats.attempts += 1;
        if outcome.success {
            stats.successes += 1;
        }
        stats.total_reward += outcome.reward;

        // Update capability confidence
        self.update_capability(outcome.agent_id, &outcome.action_type, outcome.success);

        debug!(
            agent_id = %outcome.agent_id,
            action = %outcome.action_type,
            strategy = %outcome.strategy,
            success = outcome.success,
            reward = outcome.reward,
            "Recorded action outcome"
        );

        // Store recent outcome
        self.recent_outcomes.push_back(outcome);
        if self.recent_outcomes.len() > self.max_recent {
            let quarter = self.recent_outcomes.len() / 4;
            for _ in 0..quarter {
                self.recent_outcomes.pop_front();
            }
        }
    }

    /// Update capability confidence using exponential moving average.
    fn update_capability(&mut self, agent_id: AgentId, capability: &str, success: bool) {
        let key = (agent_id, capability.to_string());
        let score = self.capabilities.entry(key).or_insert(CapabilityScore {
            capability: capability.to_string(),
            confidence: 0.5,
            sample_count: 0,
            trend: ScoreTrend::Unknown,
        });

        let old_confidence = score.confidence;
        let observation = if success { 1.0 } else { 0.0 };
        score.confidence = score.confidence * (1.0 - self.ema_alpha) + observation * self.ema_alpha;
        score.sample_count += 1;

        // Determine trend
        if score.sample_count < 5 {
            score.trend = ScoreTrend::Unknown;
        } else if score.confidence > old_confidence + 0.01 {
            score.trend = ScoreTrend::Improving;
        } else if score.confidence < old_confidence - 0.01 {
            score.trend = ScoreTrend::Declining;
        } else {
            score.trend = ScoreTrend::Stable;
        }
    }

    /// Select the best strategy for an action type using UCB1 (exploration/exploitation).
    pub fn select_strategy(&self, action_type: &str) -> Option<String> {
        let strategies = self.strategies.get(action_type)?;
        if strategies.is_empty() {
            return None;
        }

        let total: u64 = strategies.values().map(|s| s.attempts).sum();
        strategies
            .values()
            .max_by(|a, b| {
                a.ucb1_score(total)
                    .partial_cmp(&b.ucb1_score(total))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|s| s.name.clone())
    }

    /// Get the best agent for a specific action type (highest success rate with min samples).
    pub fn best_agent_for(&self, action_type: &str, min_samples: u64) -> Option<AgentId> {
        self.profiles
            .iter()
            .filter(|((_, at), p)| at == action_type && p.total_attempts >= min_samples)
            .max_by(|a, b| {
                a.1.success_rate()
                    .partial_cmp(&b.1.success_rate())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|((agent_id, _), _)| *agent_id)
    }

    /// Get the performance profile for an agent+action.
    pub fn get_profile(&self, agent_id: AgentId, action_type: &str) -> Option<&PerformanceProfile> {
        self.profiles.get(&(agent_id, action_type.to_string()))
    }

    /// Get capability score for an agent.
    pub fn get_capability(&self, agent_id: AgentId, capability: &str) -> Option<&CapabilityScore> {
        self.capabilities.get(&(agent_id, capability.to_string()))
    }

    /// List all capability scores for an agent, sorted by confidence.
    pub fn agent_capabilities(&self, agent_id: AgentId) -> Vec<&CapabilityScore> {
        let mut caps: Vec<&CapabilityScore> = self.capabilities
            .iter()
            .filter(|((aid, _), _)| *aid == agent_id)
            .map(|(_, v)| v)
            .collect();
        caps.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal));
        caps
    }

    /// Get strategy stats for an action type.
    pub fn get_strategies(&self, action_type: &str) -> Vec<&StrategyStats> {
        match self.strategies.get(action_type) {
            Some(map) => map.values().collect(),
            None => Vec::new(),
        }
    }

    /// Number of distinct agent+action profiles.
    pub fn profile_count(&self) -> usize {
        self.profiles.len()
    }

    /// Number of recent outcomes stored.
    pub fn recent_count(&self) -> usize {
        self.recent_outcomes.len()
    }
}

impl Default for AgentLearner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn agent(n: u8) -> AgentId {
        AgentId(Uuid::from_bytes([n; 16]))
    }

    fn outcome(agent_id: AgentId, action: &str, strategy: &str, success: bool, reward: f64) -> ActionOutcome {
        ActionOutcome {
            agent_id,
            action_type: action.to_string(),
            strategy: strategy.to_string(),
            success,
            duration: Duration::from_millis(100),
            reward,
            metadata: serde_json::json!({}),
            recorded_at: Instant::now(),
        }
    }

    #[test]
    fn test_record_outcome() {
        let mut learner = AgentLearner::new();
        learner.record_outcome(outcome(agent(1), "scan", "fast", true, 1.0));

        let profile = learner.get_profile(agent(1), "scan").unwrap();
        assert_eq!(profile.total_attempts, 1);
        assert_eq!(profile.successes, 1);
        assert_eq!(profile.success_rate(), 1.0);
    }

    #[test]
    fn test_success_rate() {
        let mut learner = AgentLearner::new();
        for i in 0..10 {
            learner.record_outcome(outcome(agent(1), "scan", "fast", i < 7, 1.0));
        }
        let profile = learner.get_profile(agent(1), "scan").unwrap();
        assert!((profile.success_rate() - 0.7).abs() < 0.01);
    }

    #[test]
    fn test_best_agent() {
        let mut learner = AgentLearner::new();
        // Agent 1: 90% success
        for i in 0..10 {
            learner.record_outcome(outcome(agent(1), "scan", "fast", i < 9, 1.0));
        }
        // Agent 2: 50% success
        for i in 0..10 {
            learner.record_outcome(outcome(agent(2), "scan", "fast", i < 5, 1.0));
        }

        assert_eq!(learner.best_agent_for("scan", 5), Some(agent(1)));
    }

    #[test]
    fn test_best_agent_min_samples() {
        let mut learner = AgentLearner::new();
        // Agent 1: 100% but only 2 samples
        learner.record_outcome(outcome(agent(1), "scan", "fast", true, 1.0));
        learner.record_outcome(outcome(agent(1), "scan", "fast", true, 1.0));

        // Agent 2: 80% with 10 samples
        for i in 0..10 {
            learner.record_outcome(outcome(agent(2), "scan", "fast", i < 8, 1.0));
        }

        assert_eq!(learner.best_agent_for("scan", 5), Some(agent(2)));
    }

    #[test]
    fn test_strategy_selection_ucb1() {
        let mut learner = AgentLearner::new();
        // Strategy A: tried many times, moderate reward
        for _ in 0..20 {
            learner.record_outcome(outcome(agent(1), "scan", "thorough", true, 0.5));
        }
        // Strategy B: tried few times, high reward
        for _ in 0..3 {
            learner.record_outcome(outcome(agent(1), "scan", "quick", true, 0.9));
        }

        // UCB1 should prefer the less-explored high-reward strategy
        let selected = learner.select_strategy("scan").unwrap();
        assert_eq!(selected, "quick");
    }

    #[test]
    fn test_unexplored_strategy_preferred() {
        let mut learner = AgentLearner::new();
        for _ in 0..10 {
            learner.record_outcome(outcome(agent(1), "scan", "known", true, 1.0));
        }
        // Add an unexplored strategy
        learner.strategies.entry("scan".into()).or_default()
            .insert("new".into(), StrategyStats::new("new".into()));

        let selected = learner.select_strategy("scan").unwrap();
        assert_eq!(selected, "new"); // Unexplored gets MAX score
    }

    #[test]
    fn test_capability_confidence() {
        let mut learner = AgentLearner::new();
        // Many successes → high confidence
        for _ in 0..20 {
            learner.record_outcome(outcome(agent(1), "port_scan", "fast", true, 1.0));
        }
        let score = learner.get_capability(agent(1), "port_scan").unwrap();
        assert!(score.confidence > 0.8);

        // Many failures → low confidence
        for _ in 0..20 {
            learner.record_outcome(outcome(agent(2), "port_scan", "fast", false, 0.0));
        }
        let score2 = learner.get_capability(agent(2), "port_scan").unwrap();
        assert!(score2.confidence < 0.2);
    }

    #[test]
    fn test_capability_trend() {
        let mut learner = AgentLearner::new();
        // Start with failures, then successes → Improving
        for _ in 0..5 {
            learner.record_outcome(outcome(agent(1), "scan", "fast", false, 0.0));
        }
        for _ in 0..10 {
            learner.record_outcome(outcome(agent(1), "scan", "fast", true, 1.0));
        }
        let score = learner.get_capability(agent(1), "scan").unwrap();
        assert_eq!(score.trend, ScoreTrend::Improving);
    }

    #[test]
    fn test_agent_capabilities_sorted() {
        let mut learner = AgentLearner::new();
        for _ in 0..10 {
            learner.record_outcome(outcome(agent(1), "scan", "fast", true, 1.0));
        }
        for _ in 0..10 {
            learner.record_outcome(outcome(agent(1), "dns", "fast", false, 0.0));
        }
        for _ in 0..10 {
            learner.record_outcome(outcome(agent(1), "trace", "fast", true, 0.5));
        }

        let caps = learner.agent_capabilities(agent(1));
        assert_eq!(caps.len(), 3);
        // Sorted by confidence descending
        assert!(caps[0].confidence >= caps[1].confidence);
        assert!(caps[1].confidence >= caps[2].confidence);
    }

    #[test]
    fn test_duration_tracking() {
        let mut learner = AgentLearner::new();
        learner.record_outcome(ActionOutcome {
            agent_id: agent(1),
            action_type: "scan".into(),
            strategy: "fast".into(),
            success: true,
            duration: Duration::from_millis(50),
            reward: 1.0,
            metadata: serde_json::json!({}),
            recorded_at: Instant::now(),
        });
        learner.record_outcome(ActionOutcome {
            agent_id: agent(1),
            action_type: "scan".into(),
            strategy: "fast".into(),
            success: true,
            duration: Duration::from_millis(150),
            reward: 1.0,
            metadata: serde_json::json!({}),
            recorded_at: Instant::now(),
        });

        let profile = learner.get_profile(agent(1), "scan").unwrap();
        assert!((profile.avg_duration_ms - 100.0).abs() < 1.0);
        assert!((profile.min_duration_ms - 50.0).abs() < 1.0);
        assert!((profile.max_duration_ms - 150.0).abs() < 1.0);
    }

    #[test]
    fn test_recent_pruning() {
        let mut learner = AgentLearner::new();
        learner.max_recent = 100;
        for _ in 0..150 {
            learner.record_outcome(outcome(agent(1), "scan", "fast", true, 1.0));
        }
        assert!(learner.recent_count() <= 120); // pruned by 1/4
    }

    #[test]
    fn test_empty_profile() {
        let profile = PerformanceProfile::new(agent(1), "test".into());
        assert_eq!(profile.success_rate(), 0.0);
        assert_eq!(profile.avg_reward(), 0.0);
    }

    #[test]
    fn test_empty_strategy_stats() {
        let stats = StrategyStats::new("test".into());
        assert_eq!(stats.avg_reward(), 0.0);
        assert_eq!(stats.ucb1_score(10), f64::MAX);
    }
}
