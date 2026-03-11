//! Agent Learning and Adaptation
//!
//! Tracks agent performance over time and enables adaptive behavior:
//! - Performance profiling (success rates, latency, resource usage)
//! - Strategy selection (choose best approach based on history)
//! - Reward signals for reinforcement-style feedback
//! - Capability scoring (dynamic confidence per skill)

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
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
        self.avg_duration_ms =
            self.avg_duration_ms + (dur_ms - self.avg_duration_ms) / self.total_attempts as f64;
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
        let profile = self.profiles.entry(key).or_insert_with(|| {
            PerformanceProfile::new(outcome.agent_id, outcome.action_type.clone())
        });
        profile.record(&outcome);

        // Update strategy stats
        let action_strategies = self
            .strategies
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
        let mut caps: Vec<&CapabilityScore> = self
            .capabilities
            .iter()
            .filter(|((aid, _), _)| *aid == agent_id)
            .map(|(_, v)| v)
            .collect();
        caps.sort_by(|a, b| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
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

/// Conversation context window for multi-turn agent reasoning.
/// Maintains a sliding window of recent interactions per agent,
/// persisted to the memory store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextEntry {
    pub role: String, // "user", "agent", "system"
    pub content: String,
    pub timestamp: String,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// Per-agent context window manager
pub struct ConversationContext {
    /// Maximum entries per agent
    max_entries: usize,
    /// In-memory context windows
    contexts: HashMap<AgentId, VecDeque<ContextEntry>>,
}

impl ConversationContext {
    pub fn new(max_entries: usize) -> Self {
        Self {
            max_entries,
            contexts: HashMap::new(),
        }
    }

    /// Add a message to an agent's context window
    pub fn push(&mut self, agent_id: AgentId, entry: ContextEntry) {
        let window = self.contexts.entry(agent_id).or_default();
        window.push_back(entry);
        while window.len() > self.max_entries {
            window.pop_front();
        }
    }

    /// Get the current context window for an agent
    pub fn get(&self, agent_id: AgentId) -> Vec<&ContextEntry> {
        self.contexts
            .get(&agent_id)
            .map(|w| w.iter().collect())
            .unwrap_or_default()
    }

    /// Get context as a formatted string for LLM injection
    pub fn format_for_llm(&self, agent_id: AgentId) -> String {
        let entries = self.get(agent_id);
        if entries.is_empty() {
            return String::new();
        }

        entries
            .iter()
            .map(|e| format!("[{}] {}: {}", e.timestamp, e.role, e.content))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Clear context for an agent
    pub fn clear(&mut self, agent_id: AgentId) {
        self.contexts.remove(&agent_id);
    }

    /// Get the number of entries in an agent's context
    pub fn len(&self, agent_id: AgentId) -> usize {
        self.contexts.get(&agent_id).map(|w| w.len()).unwrap_or(0)
    }

    /// Check if an agent has any context
    pub fn is_empty(&self, agent_id: AgentId) -> bool {
        self.len(agent_id) == 0
    }

    /// Number of agents with active context
    pub fn active_agents(&self) -> usize {
        self.contexts.len()
    }

    /// Export context for persistence
    pub fn export(&self, agent_id: AgentId) -> Vec<ContextEntry> {
        self.contexts
            .get(&agent_id)
            .map(|w| w.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Import context (e.g., from memory store on agent restart)
    pub fn import(&mut self, agent_id: AgentId, entries: Vec<ContextEntry>) {
        let mut window = VecDeque::from(entries);
        while window.len() > self.max_entries {
            window.pop_front();
        }
        self.contexts.insert(agent_id, window);
    }
}

impl Default for ConversationContext {
    fn default() -> Self {
        Self::new(50)
    }
}

// ---------------------------------------------------------------------------
// Agent Behavior Anomaly Detection
// ---------------------------------------------------------------------------

/// Named behavior metrics tracked per sample.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BehaviorMetric {
    SyscallCount,
    NetworkBytes,
    FileOps,
    CpuPercent,
    MemoryBytes,
}

impl BehaviorMetric {
    /// All known metric variants.
    pub const ALL: &'static [BehaviorMetric] = &[
        BehaviorMetric::SyscallCount,
        BehaviorMetric::NetworkBytes,
        BehaviorMetric::FileOps,
        BehaviorMetric::CpuPercent,
        BehaviorMetric::MemoryBytes,
    ];
}

impl std::fmt::Display for BehaviorMetric {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BehaviorMetric::SyscallCount => write!(f, "syscall_count"),
            BehaviorMetric::NetworkBytes => write!(f, "network_bytes"),
            BehaviorMetric::FileOps => write!(f, "file_ops"),
            BehaviorMetric::CpuPercent => write!(f, "cpu_percent"),
            BehaviorMetric::MemoryBytes => write!(f, "memory_bytes"),
        }
    }
}

impl std::str::FromStr for BehaviorMetric {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "syscall_count" => Ok(Self::SyscallCount),
            "network_bytes" => Ok(Self::NetworkBytes),
            "file_ops" => Ok(Self::FileOps),
            "cpu_percent" => Ok(Self::CpuPercent),
            "memory_bytes" => Ok(Self::MemoryBytes),
            other => Err(format!("unknown behavior metric: {other}")),
        }
    }
}

/// A single behavior sample from an agent at a point in time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehaviorSample {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub syscall_count: u64,
    pub network_bytes: u64,
    pub file_ops: u64,
    pub cpu_percent: f64,
    pub memory_bytes: u64,
}

impl BehaviorSample {
    /// Extract the value of a metric.
    fn metric_value(&self, metric: BehaviorMetric) -> f64 {
        match metric {
            BehaviorMetric::SyscallCount => self.syscall_count as f64,
            BehaviorMetric::NetworkBytes => self.network_bytes as f64,
            BehaviorMetric::FileOps => self.file_ops as f64,
            BehaviorMetric::CpuPercent => self.cpu_percent,
            BehaviorMetric::MemoryBytes => self.memory_bytes as f64,
        }
    }
}

/// Severity classification for anomaly alerts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnomalySeverity {
    /// 1-2 standard deviations
    Low,
    /// 2-3 standard deviations
    Medium,
    /// 3-5 standard deviations
    High,
    /// 5+ standard deviations
    Critical,
}

impl AnomalySeverity {
    fn from_sigmas(sigmas: f64) -> Self {
        let abs = sigmas.abs();
        if abs >= 5.0 {
            AnomalySeverity::Critical
        } else if abs >= 3.0 {
            AnomalySeverity::High
        } else if abs >= 2.0 {
            AnomalySeverity::Medium
        } else {
            AnomalySeverity::Low
        }
    }
}

/// An alert raised when agent behavior deviates significantly from baseline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnomalyAlert {
    pub agent_id: AgentId,
    pub metric: String,
    pub current_value: f64,
    pub baseline_mean: f64,
    pub baseline_stddev: f64,
    pub deviation_sigmas: f64,
    pub severity: AnomalySeverity,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Tracks a sliding window of behavior samples and computes baseline statistics.
#[derive(Debug, Clone)]
pub struct BehaviorBaseline {
    agent_id: AgentId,
    window_size: usize,
    samples: VecDeque<BehaviorSample>,
}

impl BehaviorBaseline {
    /// Create a new baseline tracker for an agent.
    pub fn new(agent_id: AgentId, window_size: usize) -> Self {
        Self {
            agent_id,
            window_size,
            samples: VecDeque::with_capacity(window_size),
        }
    }

    /// Record a new behavior sample into the sliding window.
    pub fn record(&mut self, sample: BehaviorSample) {
        self.samples.push_back(sample);
        while self.samples.len() > self.window_size {
            self.samples.pop_front();
        }
    }

    /// Compute the mean of a metric across the window.
    pub fn mean(&self, metric: BehaviorMetric) -> Option<f64> {
        if self.samples.is_empty() {
            return None;
        }
        let sum: f64 = self.samples.iter().map(|s| s.metric_value(metric)).sum();
        Some(sum / self.samples.len() as f64)
    }

    /// Compute the standard deviation of a metric across the window.
    pub fn stddev(&self, metric: BehaviorMetric) -> Option<f64> {
        let m = self.mean(metric)?;
        if self.samples.len() < 2 {
            return Some(0.0);
        }
        let sum_sq: f64 = self
            .samples
            .iter()
            .map(|s| {
                let v = s.metric_value(metric);
                (v - m) * (v - m)
            })
            .sum();
        Some((sum_sq / (self.samples.len() - 1) as f64).sqrt())
    }

    /// Check if a sample is anomalous (any metric exceeds threshold_sigmas from mean).
    pub fn is_anomalous(
        &self,
        sample: &BehaviorSample,
        threshold_sigmas: f64,
    ) -> Vec<AnomalyAlert> {
        let mut alerts = Vec::new();
        if self.samples.len() < 2 {
            return alerts;
        }

        for &metric in BehaviorMetric::ALL {
            let mean = match self.mean(metric) {
                Some(m) => m,
                None => continue,
            };
            let sd = match self.stddev(metric) {
                Some(s) if s > 0.0 => s,
                _ => continue,
            };
            let value = sample.metric_value(metric);

            let deviation = (value - mean) / sd;
            if deviation.abs() >= threshold_sigmas {
                alerts.push(AnomalyAlert {
                    agent_id: self.agent_id,
                    metric: metric.to_string(),
                    current_value: value,
                    baseline_mean: mean,
                    baseline_stddev: sd,
                    deviation_sigmas: deviation,
                    severity: AnomalySeverity::from_sigmas(deviation),
                    timestamp: sample.timestamp,
                });
            }
        }
        alerts
    }

    /// Number of samples in the window.
    pub fn sample_count(&self) -> usize {
        self.samples.len()
    }
}

/// Manages anomaly detection baselines for multiple agents.
pub struct AnomalyDetector {
    baselines: HashMap<AgentId, BehaviorBaseline>,
    alerts: Vec<AnomalyAlert>,
    default_window: usize,
    default_threshold: f64,
}

impl AnomalyDetector {
    /// Create a new detector with default window size and sigma threshold.
    pub fn new(default_window: usize, default_threshold: f64) -> Self {
        Self {
            baselines: HashMap::new(),
            alerts: Vec::new(),
            default_window,
            default_threshold,
        }
    }

    /// Record a behavior sample for an agent and check for anomalies.
    pub fn record_behavior(
        &mut self,
        agent_id: AgentId,
        sample: BehaviorSample,
    ) -> Vec<AnomalyAlert> {
        let baseline = self
            .baselines
            .entry(agent_id)
            .or_insert_with(|| BehaviorBaseline::new(agent_id, self.default_window));

        let new_alerts = baseline.is_anomalous(&sample, self.default_threshold);
        self.alerts.extend(new_alerts.clone());
        baseline.record(sample);
        new_alerts
    }

    /// Get the baseline for an agent.
    pub fn get_baseline(&self, agent_id: &AgentId) -> Option<&BehaviorBaseline> {
        self.baselines.get(agent_id)
    }

    /// Get all recent alerts.
    pub fn active_alerts(&self) -> Vec<&AnomalyAlert> {
        self.alerts.iter().collect()
    }

    /// Clear alerts for a specific agent.
    pub fn clear_alerts(&mut self, agent_id: &AgentId) {
        self.alerts.retain(|a| a.agent_id != *agent_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn agent(n: u8) -> AgentId {
        AgentId(Uuid::from_bytes([n; 16]))
    }

    fn outcome(
        agent_id: AgentId,
        action: &str,
        strategy: &str,
        success: bool,
        reward: f64,
    ) -> ActionOutcome {
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
        learner
            .strategies
            .entry("scan".into())
            .or_default()
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

    // ── ConversationContext tests ──────────────────────────────────────

    fn make_entry(role: &str, content: &str) -> ContextEntry {
        ContextEntry {
            role: role.to_string(),
            content: content.to_string(),
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            metadata: serde_json::Value::Null,
        }
    }

    #[test]
    fn test_context_push_and_get() {
        let mut ctx = ConversationContext::new(10);
        let id = agent(1);
        ctx.push(id, make_entry("user", "hello"));
        ctx.push(id, make_entry("agent", "hi there"));

        let entries = ctx.get(id);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].role, "user");
        assert_eq!(entries[1].content, "hi there");
    }

    #[test]
    fn test_context_sliding_window_eviction() {
        let mut ctx = ConversationContext::new(3);
        let id = agent(1);
        for i in 0..5 {
            ctx.push(id, make_entry("user", &format!("msg{}", i)));
        }
        let entries = ctx.get(id);
        assert_eq!(entries.len(), 3);
        // Oldest two should be evicted
        assert_eq!(entries[0].content, "msg2");
        assert_eq!(entries[1].content, "msg3");
        assert_eq!(entries[2].content, "msg4");
    }

    #[test]
    fn test_context_clear() {
        let mut ctx = ConversationContext::new(10);
        let id = agent(1);
        ctx.push(id, make_entry("user", "test"));
        ctx.clear(id);
        assert!(ctx.is_empty(id));
        assert_eq!(ctx.active_agents(), 0);
    }

    #[test]
    fn test_format_for_llm_empty() {
        let ctx = ConversationContext::new(10);
        assert_eq!(ctx.format_for_llm(agent(1)), "");
    }

    #[test]
    fn test_format_for_llm_populated() {
        let mut ctx = ConversationContext::new(10);
        let id = agent(1);
        ctx.push(
            id,
            ContextEntry {
                role: "user".into(),
                content: "what is 2+2?".into(),
                timestamp: "T1".into(),
                metadata: serde_json::Value::Null,
            },
        );
        ctx.push(
            id,
            ContextEntry {
                role: "agent".into(),
                content: "4".into(),
                timestamp: "T2".into(),
                metadata: serde_json::Value::Null,
            },
        );

        let formatted = ctx.format_for_llm(id);
        assert!(formatted.contains("[T1] user: what is 2+2?"));
        assert!(formatted.contains("[T2] agent: 4"));
    }

    #[test]
    fn test_context_is_empty() {
        let mut ctx = ConversationContext::new(10);
        let id = agent(1);
        assert!(ctx.is_empty(id));
        ctx.push(id, make_entry("user", "hi"));
        assert!(!ctx.is_empty(id));
    }

    #[test]
    fn test_context_active_agents() {
        let mut ctx = ConversationContext::new(10);
        assert_eq!(ctx.active_agents(), 0);
        ctx.push(agent(1), make_entry("user", "a"));
        ctx.push(agent(2), make_entry("user", "b"));
        assert_eq!(ctx.active_agents(), 2);
    }

    #[test]
    fn test_context_export_import_roundtrip() {
        let mut ctx = ConversationContext::new(10);
        let id = agent(1);
        ctx.push(id, make_entry("user", "msg1"));
        ctx.push(id, make_entry("agent", "msg2"));

        let exported = ctx.export(id);
        assert_eq!(exported.len(), 2);

        let mut ctx2 = ConversationContext::new(10);
        ctx2.import(id, exported);

        let entries = ctx2.get(id);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].content, "msg1");
        assert_eq!(entries[1].content, "msg2");
    }

    #[test]
    fn test_context_default_max() {
        let ctx = ConversationContext::default();
        assert_eq!(ctx.max_entries, 50);
    }

    #[test]
    fn test_context_len_tracking() {
        let mut ctx = ConversationContext::new(100);
        let id = agent(1);
        assert_eq!(ctx.len(id), 0);
        ctx.push(id, make_entry("user", "a"));
        assert_eq!(ctx.len(id), 1);
        ctx.push(id, make_entry("agent", "b"));
        assert_eq!(ctx.len(id), 2);
    }

    #[test]
    fn test_context_multiple_agents_isolated() {
        let mut ctx = ConversationContext::new(10);
        let id1 = agent(1);
        let id2 = agent(2);

        ctx.push(id1, make_entry("user", "agent1-msg"));
        ctx.push(id2, make_entry("user", "agent2-msg"));

        let entries1 = ctx.get(id1);
        let entries2 = ctx.get(id2);

        assert_eq!(entries1.len(), 1);
        assert_eq!(entries2.len(), 1);
        assert_eq!(entries1[0].content, "agent1-msg");
        assert_eq!(entries2[0].content, "agent2-msg");
    }

    #[test]
    fn test_context_import_truncates() {
        let mut ctx = ConversationContext::new(3);
        let id = agent(1);

        let entries: Vec<ContextEntry> = (0..10)
            .map(|i| make_entry("user", &format!("msg{}", i)))
            .collect();
        ctx.import(id, entries);

        assert_eq!(ctx.len(id), 3);
        let got = ctx.get(id);
        // Should keep the last 3 (oldest evicted from front)
        assert_eq!(got[0].content, "msg7");
        assert_eq!(got[1].content, "msg8");
        assert_eq!(got[2].content, "msg9");
    }

    // ── Anomaly Detection tests ──────────────────────────────────────

    fn make_sample(syscall: u64, net: u64, file_ops: u64, cpu: f64, mem: u64) -> BehaviorSample {
        BehaviorSample {
            timestamp: chrono::Utc::now(),
            syscall_count: syscall,
            network_bytes: net,
            file_ops: file_ops,
            cpu_percent: cpu,
            memory_bytes: mem,
        }
    }

    #[test]
    fn test_baseline_empty_mean() {
        let baseline = BehaviorBaseline::new(agent(1), 100);
        assert!(baseline.mean(BehaviorMetric::SyscallCount).is_none());
        assert!(baseline.stddev(BehaviorMetric::SyscallCount).is_none());
    }

    #[test]
    fn test_baseline_single_sample_mean() {
        let mut baseline = BehaviorBaseline::new(agent(1), 100);
        baseline.record(make_sample(100, 200, 10, 50.0, 1024));
        assert!((baseline.mean(BehaviorMetric::SyscallCount).unwrap() - 100.0).abs() < 0.01);
        assert!((baseline.mean(BehaviorMetric::CpuPercent).unwrap() - 50.0).abs() < 0.01);
    }

    #[test]
    fn test_baseline_mean_multiple_samples() {
        let mut baseline = BehaviorBaseline::new(agent(1), 100);
        baseline.record(make_sample(100, 0, 0, 10.0, 0));
        baseline.record(make_sample(200, 0, 0, 30.0, 0));
        baseline.record(make_sample(300, 0, 0, 50.0, 0));
        assert!((baseline.mean(BehaviorMetric::SyscallCount).unwrap() - 200.0).abs() < 0.01);
        assert!((baseline.mean(BehaviorMetric::CpuPercent).unwrap() - 30.0).abs() < 0.01);
    }

    #[test]
    fn test_baseline_stddev_identical_values() {
        let mut baseline = BehaviorBaseline::new(agent(1), 100);
        for _ in 0..10 {
            baseline.record(make_sample(100, 0, 0, 50.0, 0));
        }
        assert!((baseline.stddev(BehaviorMetric::SyscallCount).unwrap()).abs() < 0.01);
        assert!((baseline.stddev(BehaviorMetric::CpuPercent).unwrap()).abs() < 0.01);
    }

    #[test]
    fn test_baseline_stddev_known_values() {
        let mut baseline = BehaviorBaseline::new(agent(1), 100);
        // Values: 2, 4, 4, 4, 5, 5, 7, 9 -> mean=5, sample stddev=2.138
        for v in [2, 4, 4, 4, 5, 5, 7, 9] {
            baseline.record(make_sample(v, 0, 0, 0.0, 0));
        }
        let sd = baseline.stddev(BehaviorMetric::SyscallCount).unwrap();
        assert!((sd - 2.138).abs() < 0.01);
    }

    #[test]
    fn test_baseline_window_eviction() {
        let mut baseline = BehaviorBaseline::new(agent(1), 5);
        for i in 0..10 {
            baseline.record(make_sample(i * 10, 0, 0, 0.0, 0));
        }
        assert_eq!(baseline.sample_count(), 5);
        // Window should contain: 50, 60, 70, 80, 90 -> mean=70
        assert!((baseline.mean(BehaviorMetric::SyscallCount).unwrap() - 70.0).abs() < 0.01);
    }

    #[test]
    fn test_anomaly_detection_no_anomaly() {
        let mut baseline = BehaviorBaseline::new(agent(1), 100);
        for _ in 0..20 {
            baseline.record(make_sample(100, 1000, 10, 50.0, 4096));
        }
        // Normal sample within baseline
        let normal = make_sample(102, 1010, 11, 51.0, 4100);
        let alerts = baseline.is_anomalous(&normal, 2.0);
        assert!(alerts.is_empty());
    }

    #[test]
    fn test_anomaly_detection_high_cpu() {
        let mut baseline = BehaviorBaseline::new(agent(1), 100);
        for _ in 0..20 {
            baseline.record(make_sample(100, 1000, 10, 10.0, 4096));
        }
        // Inject a little variance so stddev > 0
        baseline.record(make_sample(100, 1000, 10, 11.0, 4096));
        baseline.record(make_sample(100, 1000, 10, 9.0, 4096));

        // Massive CPU spike
        let spike = make_sample(100, 1000, 10, 99.0, 4096);
        let alerts = baseline.is_anomalous(&spike, 2.0);
        assert!(!alerts.is_empty());
        let cpu_alert = alerts.iter().find(|a| a.metric == "cpu_percent").unwrap();
        assert!(cpu_alert.deviation_sigmas > 2.0);
    }

    #[test]
    fn test_anomaly_severity_classification() {
        assert_eq!(AnomalySeverity::from_sigmas(1.5), AnomalySeverity::Low);
        assert_eq!(AnomalySeverity::from_sigmas(2.5), AnomalySeverity::Medium);
        assert_eq!(AnomalySeverity::from_sigmas(4.0), AnomalySeverity::High);
        assert_eq!(AnomalySeverity::from_sigmas(6.0), AnomalySeverity::Critical);
        assert_eq!(
            AnomalySeverity::from_sigmas(-5.5),
            AnomalySeverity::Critical
        );
    }

    #[test]
    fn test_anomaly_too_few_samples() {
        let mut baseline = BehaviorBaseline::new(agent(1), 100);
        baseline.record(make_sample(100, 0, 0, 0.0, 0));
        // With only 1 sample, stddev=0, no anomaly can be detected
        let spike = make_sample(99999, 0, 0, 0.0, 0);
        let alerts = baseline.is_anomalous(&spike, 2.0);
        assert!(alerts.is_empty());
    }

    #[test]
    fn test_behavior_metric_from_str() {
        assert_eq!(
            "syscall_count".parse::<BehaviorMetric>().unwrap(),
            BehaviorMetric::SyscallCount
        );
        assert_eq!(
            "cpu_percent".parse::<BehaviorMetric>().unwrap(),
            BehaviorMetric::CpuPercent
        );
        assert!("nonexistent".parse::<BehaviorMetric>().is_err());
    }

    #[test]
    fn test_detector_record_and_baseline() {
        let mut detector = AnomalyDetector::new(100, 2.0);
        let id = agent(1);
        detector.record_behavior(id, make_sample(100, 0, 0, 0.0, 0));
        assert!(detector.get_baseline(&id).is_some());
        assert_eq!(detector.get_baseline(&id).unwrap().sample_count(), 1);
    }

    #[test]
    fn test_detector_multi_agent() {
        let mut detector = AnomalyDetector::new(100, 2.0);
        detector.record_behavior(agent(1), make_sample(100, 0, 0, 0.0, 0));
        detector.record_behavior(agent(2), make_sample(200, 0, 0, 0.0, 0));
        assert!(detector.get_baseline(&agent(1)).is_some());
        assert!(detector.get_baseline(&agent(2)).is_some());
        assert!(detector.get_baseline(&agent(3)).is_none());
    }

    #[test]
    fn test_detector_generates_alerts() {
        let mut detector = AnomalyDetector::new(100, 2.0);
        let id = agent(1);
        // Build baseline with some variance
        for i in 0..20 {
            detector.record_behavior(
                id,
                make_sample(100 + (i % 3), 0, 0, 10.0 + (i as f64 % 2.0), 0),
            );
        }
        // Send anomalous sample
        let alerts = detector.record_behavior(id, make_sample(100, 0, 0, 999.0, 0));
        assert!(!alerts.is_empty());
        assert!(!detector.active_alerts().is_empty());
    }

    #[test]
    fn test_detector_clear_alerts() {
        let mut detector = AnomalyDetector::new(100, 2.0);
        let id = agent(1);
        for i in 0..20 {
            detector.record_behavior(id, make_sample(100, 0, 0, 10.0 + (i as f64 % 2.0), 0));
        }
        detector.record_behavior(id, make_sample(100, 0, 0, 999.0, 0));
        assert!(!detector.active_alerts().is_empty());
        detector.clear_alerts(&id);
        assert!(detector.active_alerts().is_empty());
    }

    #[test]
    fn test_behavior_sample_all_metrics() {
        let sample = make_sample(1, 2, 3, 4.0, 5);
        assert_eq!(sample.metric_value(BehaviorMetric::SyscallCount), 1.0);
        assert_eq!(sample.metric_value(BehaviorMetric::NetworkBytes), 2.0);
        assert_eq!(sample.metric_value(BehaviorMetric::FileOps), 3.0);
        assert_eq!(sample.metric_value(BehaviorMetric::CpuPercent), 4.0);
        assert_eq!(sample.metric_value(BehaviorMetric::MemoryBytes), 5.0);
    }

    #[test]
    fn test_baseline_single_sample_stddev() {
        let mut baseline = BehaviorBaseline::new(agent(1), 100);
        baseline.record(make_sample(100, 0, 0, 0.0, 0));
        // Single sample -> stddev=0
        assert!((baseline.stddev(BehaviorMetric::SyscallCount).unwrap()).abs() < 0.01);
    }

    #[test]
    fn test_anomaly_alert_fields() {
        let mut baseline = BehaviorBaseline::new(agent(1), 100);
        for i in 0..20 {
            baseline.record(make_sample(100 + (i % 3), 0, 0, 10.0 + (i as f64 % 2.0), 0));
        }
        let spike = make_sample(100, 0, 0, 999.0, 0);
        let alerts = baseline.is_anomalous(&spike, 2.0);
        let cpu_alert = alerts.iter().find(|a| a.metric == "cpu_percent");
        assert!(cpu_alert.is_some());
        let alert = cpu_alert.unwrap();
        assert_eq!(alert.agent_id, agent(1));
        assert!(alert.current_value > 900.0);
        assert!(alert.baseline_mean < 20.0);
        assert!(alert.baseline_stddev > 0.0);
    }
}
