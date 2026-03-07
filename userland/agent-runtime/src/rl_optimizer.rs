//! Reinforcement Learning Loop for Agent Policy Optimization
//!
//! Optimizes agent decision-making through experience-driven learning:
//! - Q-learning with tabular value functions
//! - Policy gradient (REINFORCE-like) for continuous action spaces
//! - Replay buffers with prioritized sampling
//! - Epsilon-greedy and softmax exploration strategies
//! - Reward shaping with weighted multi-objective components

use std::collections::{HashMap, HashSet};

use anyhow::{bail, Result};
use chrono::{DateTime, Utc};
use rand::Rng;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

// ---------------------------------------------------------------------------
// Core data structures
// ---------------------------------------------------------------------------

/// Observable state of the environment at a point in time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct State {
    pub state_id: String,
    pub features: HashMap<String, f64>,
    pub timestamp: DateTime<Utc>,
}

impl State {
    /// Returns the feature values sorted by key for deterministic ordering.
    pub fn feature_vector(&self) -> Vec<f64> {
        let mut keys: Vec<&String> = self.features.keys().collect();
        keys.sort();
        keys.iter().map(|k| self.features[*k]).collect()
    }

    /// Euclidean distance between two states' feature vectors, aligned by key.
    ///
    /// Computes distance over the union of keys, treating missing keys as 0.0.
    pub fn distance(&self, other: &State) -> f64 {
        let mut all_keys: HashSet<&String> = self.features.keys().collect();
        all_keys.extend(other.features.keys());
        let mut sum = 0.0_f64;
        for key in all_keys {
            let a = self.features.get(key).copied().unwrap_or(0.0);
            let b = other.features.get(key).copied().unwrap_or(0.0);
            let d = a - b;
            sum += d * d;
        }
        sum.sqrt()
    }
}

/// An action the agent can take.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Action {
    pub action_id: String,
    pub name: String,
    pub parameters: HashMap<String, String>,
}

/// Scalar reward with component breakdown.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reward {
    pub value: f64,
    pub components: HashMap<String, f64>,
    pub timestamp: DateTime<Utc>,
}

/// A single SARS (state, action, reward, next_state) experience tuple.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Experience {
    pub state: State,
    pub action: Action,
    pub reward: Reward,
    pub next_state: State,
    pub done: bool,
    pub episode_id: String,
}

// ---------------------------------------------------------------------------
// Replay buffer
// ---------------------------------------------------------------------------

/// Stores experiences for off-policy training with optional prioritisation.
pub struct ReplayBuffer {
    capacity: usize,
    buffer: Vec<Experience>,
    priorities: Vec<f64>,
    position: usize,
    full: bool,
}

impl ReplayBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            buffer: Vec::with_capacity(capacity),
            priorities: Vec::with_capacity(capacity),
            position: 0,
            full: false,
        }
    }

    /// Add an experience with default priority 1.0.
    pub fn add(&mut self, experience: Experience) {
        self.add_with_priority(experience, 1.0);
    }

    /// Add an experience with explicit priority.
    pub fn add_with_priority(&mut self, experience: Experience, priority: f64) {
        let prio = if priority <= 0.0 { 1e-6 } else { priority };
        if self.buffer.len() < self.capacity {
            self.buffer.push(experience);
            self.priorities.push(prio);
        } else {
            self.buffer[self.position] = experience;
            self.priorities[self.position] = prio;
        }
        self.position = (self.position + 1) % self.capacity;
        if self.position == 0 && self.buffer.len() == self.capacity {
            self.full = true;
        }
    }

    /// Uniformly random sample of `batch_size` experiences.
    pub fn sample(&self, batch_size: usize) -> Vec<&Experience> {
        if self.buffer.is_empty() {
            return Vec::new();
        }
        let mut rng = rand::thread_rng();
        let n = self.buffer.len();
        let effective = batch_size.min(n);
        let mut out = Vec::with_capacity(effective);
        for _ in 0..effective {
            let idx = rng.gen_range(0..n);
            out.push(&self.buffer[idx]);
        }
        out
    }

    /// Sample biased by stored priorities (higher = more likely).
    pub fn sample_prioritized(&self, batch_size: usize) -> Vec<&Experience> {
        if self.buffer.is_empty() {
            return Vec::new();
        }
        let n = self.buffer.len();
        let effective = batch_size.min(n);
        let total: f64 = self.priorities[..n].iter().sum();
        if total <= 0.0 {
            return self.sample(batch_size);
        }
        let mut rng = rand::thread_rng();
        let mut out = Vec::with_capacity(effective);
        for _ in 0..effective {
            let threshold = rng.gen::<f64>() * total;
            let mut cumulative = 0.0;
            let mut chosen = 0;
            for (i, &p) in self.priorities[..n].iter().enumerate() {
                cumulative += p;
                if cumulative >= threshold {
                    chosen = i;
                    break;
                }
            }
            out.push(&self.buffer[chosen]);
        }
        out
    }

    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    pub fn clear(&mut self) {
        self.buffer.clear();
        self.priorities.clear();
        self.position = 0;
        self.full = false;
    }
}

// ---------------------------------------------------------------------------
// Q-table (tabular Q-learning)
// ---------------------------------------------------------------------------

/// Tabular Q-value store mapping (state_id, action_id) pairs to values.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct QTable {
    values: HashMap<(String, String), f64>,
}

impl QTable {
    pub fn new() -> Self {
        Self::default()
    }

    /// Retrieve Q(s, a); defaults to 0.0 for unseen pairs.
    pub fn get_q(&self, state_id: &str, action_id: &str) -> f64 {
        self.values
            .get(&(state_id.to_string(), action_id.to_string()))
            .copied()
            .unwrap_or(0.0)
    }

    /// Bellman update: Q(s,a) <- Q(s,a) + lr * [reward + gamma * max_q_next - Q(s,a)]
    pub fn update_q(
        &mut self,
        state_id: &str,
        action_id: &str,
        reward: f64,
        next_state_max_q: f64,
        learning_rate: f64,
        discount_factor: f64,
    ) {
        let key = (state_id.to_string(), action_id.to_string());
        let current = self.values.get(&key).copied().unwrap_or(0.0);
        let td_target = reward + discount_factor * next_state_max_q;
        let new_value = current + learning_rate * (td_target - current);
        self.values.insert(key, new_value);
    }

    /// Action with the highest Q-value for the given state.
    pub fn best_action(&self, state_id: &str, available_actions: &[String]) -> Option<String> {
        if available_actions.is_empty() {
            return None;
        }
        let mut best: Option<(&String, f64)> = None;
        for a in available_actions {
            let q = self.get_q(state_id, a);
            match best {
                None => best = Some((a, q)),
                Some((_, bq)) if q > bq => best = Some((a, q)),
                _ => {}
            }
        }
        best.map(|(a, _)| a.clone())
    }

    /// Maximum Q-value per state that appears in the table.
    pub fn state_values(&self) -> HashMap<String, f64> {
        let mut out: HashMap<String, f64> = HashMap::new();
        for ((s, _), &v) in &self.values {
            let entry = out.entry(s.clone()).or_insert(f64::NEG_INFINITY);
            if v > *entry {
                *entry = v;
            }
        }
        out
    }

    pub fn len(&self) -> usize {
        self.values.len()
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Exploration strategies
// ---------------------------------------------------------------------------

/// Epsilon-greedy exploration with annealing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpsilonGreedy {
    pub epsilon: f64,
    pub epsilon_decay: f64,
    pub epsilon_min: f64,
}

impl EpsilonGreedy {
    pub fn new(epsilon: f64, epsilon_decay: f64, epsilon_min: f64) -> Self {
        let epsilon = epsilon.clamp(0.0, 1.0);
        let epsilon_min = epsilon_min.clamp(0.0, 1.0);
        let epsilon_min = epsilon_min.min(epsilon);
        let epsilon_decay = epsilon_decay.clamp(f64::MIN_POSITIVE, 1.0);
        Self {
            epsilon,
            epsilon_decay,
            epsilon_min,
        }
    }

    /// Choose an action: returns (action_id, was_exploration).
    pub fn select_action(
        &self,
        q_table: &QTable,
        state_id: &str,
        available_actions: &[String],
    ) -> (String, bool) {
        if available_actions.is_empty() {
            return (String::new(), false);
        }
        let mut rng = rand::thread_rng();
        if rng.gen::<f64>() < self.epsilon {
            // Explore
            let idx = rng.gen_range(0..available_actions.len());
            (available_actions[idx].clone(), true)
        } else {
            // Exploit
            let action = q_table
                .best_action(state_id, available_actions)
                .unwrap_or_else(|| available_actions[0].clone());
            (action, false)
        }
    }

    /// Decay epsilon towards the minimum.
    pub fn decay(&mut self) {
        self.epsilon = (self.epsilon * self.epsilon_decay).max(self.epsilon_min);
    }
}

// ---------------------------------------------------------------------------
// Policy gradient (REINFORCE-like)
// ---------------------------------------------------------------------------

/// Simplified policy gradient with softmax action selection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyGradient {
    pub policy_weights: HashMap<String, f64>,
}

impl PolicyGradient {
    pub fn new() -> Self {
        Self {
            policy_weights: HashMap::new(),
        }
    }

    /// Softmax probability for a single action.
    pub fn action_probability(&self, action_id: &str) -> f64 {
        let w = self.policy_weights.get(action_id).copied().unwrap_or(0.0);
        if self.policy_weights.is_empty() {
            return 1.0;
        }
        let max_w = self
            .policy_weights
            .values()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        let sum_exp: f64 = self
            .policy_weights
            .values()
            .map(|&v| (v - max_w).exp())
            .sum();
        if sum_exp == 0.0 {
            return 1.0 / self.policy_weights.len() as f64;
        }
        (w - max_w).exp() / sum_exp
    }

    /// Full softmax distribution over the given actions.
    fn action_probabilities(&self, available_actions: &[String]) -> Vec<f64> {
        if available_actions.is_empty() {
            return Vec::new();
        }
        let weights: Vec<f64> = available_actions
            .iter()
            .map(|a| self.policy_weights.get(a).copied().unwrap_or(0.0))
            .collect();
        let max_w = weights.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let exps: Vec<f64> = weights.iter().map(|&w| (w - max_w).exp()).collect();
        let sum: f64 = exps.iter().sum();
        if sum == 0.0 {
            let uniform = 1.0 / available_actions.len() as f64;
            return vec![uniform; available_actions.len()];
        }
        exps.iter().map(|&e| e / sum).collect()
    }

    /// Sample an action from the softmax distribution.
    pub fn select_action(&self, available_actions: &[String]) -> String {
        if available_actions.is_empty() {
            return String::new();
        }
        if available_actions.len() == 1 {
            return available_actions[0].clone();
        }
        let probs = self.action_probabilities(available_actions);
        let mut rng = rand::thread_rng();
        let r: f64 = rng.gen();
        let mut cumulative = 0.0;
        for (i, &p) in probs.iter().enumerate() {
            cumulative += p;
            if r < cumulative {
                return available_actions[i].clone();
            }
        }
        available_actions.last().unwrap().clone()
    }

    /// REINFORCE update: w += lr * G * grad_log_pi.
    /// For softmax: grad_log_pi(a) = 1 - pi(a) for the chosen action, -pi(a') for others.
    pub fn update_weights(&mut self, episode: &[Experience], learning_rate: f64) {
        if episode.is_empty() {
            return;
        }

        // Collect all unique action ids from the episode for the softmax denominator.
        let all_actions: Vec<String> = {
            let mut s: Vec<String> = self.policy_weights.keys().cloned().collect();
            for exp in episode {
                if !s.contains(&exp.action.action_id) {
                    s.push(exp.action.action_id.clone());
                }
            }
            s
        };

        // Ensure all actions have weights.
        for a in &all_actions {
            self.policy_weights.entry(a.clone()).or_insert(0.0);
        }

        // Compute discounted returns G_t for each step.
        let gamma = 0.99_f64;
        let n = episode.len();
        let mut returns = vec![0.0_f64; n];
        returns[n - 1] = episode[n - 1].reward.value;
        for t in (0..n - 1).rev() {
            returns[t] = episode[t].reward.value + gamma * returns[t + 1];
        }

        // Update weights for each step.
        for (t, exp) in episode.iter().enumerate() {
            let g = returns[t];
            let probs = self.action_probabilities(&all_actions);
            for (i, a) in all_actions.iter().enumerate() {
                let grad = if *a == exp.action.action_id {
                    1.0 - probs[i]
                } else {
                    -probs[i]
                };
                *self.policy_weights.entry(a.clone()).or_insert(0.0) += learning_rate * g * grad;
            }
        }

        debug!(
            actions = all_actions.len(),
            steps = n,
            "policy gradient weights updated"
        );
    }
}

impl Default for PolicyGradient {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Reward shaping
// ---------------------------------------------------------------------------

/// A named, weighted component of the reward signal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RewardComponent {
    pub name: String,
    pub weight: f64,
    pub compute_fn: String,
}

/// Computes shaped rewards from raw multi-objective signal components.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RewardShaper {
    pub components: Vec<RewardComponent>,
}

impl RewardShaper {
    pub fn new() -> Self {
        Self {
            components: Vec::new(),
        }
    }

    pub fn add_component(&mut self, name: &str, weight: f64) {
        self.components.push(RewardComponent {
            name: name.to_string(),
            weight,
            compute_fn: format!("raw[{name}]"),
        });
    }

    /// Normalise weights so they sum to 1.0.
    pub fn normalize_weights(&mut self) {
        let total: f64 = self.components.iter().map(|c| c.weight.abs()).sum();
        if total > 0.0 {
            for c in &mut self.components {
                c.weight /= total;
            }
        }
    }

    /// Compute the weighted reward from raw component values.
    pub fn compute_reward(&self, raw_components: &HashMap<String, f64>) -> Reward {
        let mut value = 0.0_f64;
        let mut components = HashMap::new();
        for rc in &self.components {
            if let Some(&raw) = raw_components.get(&rc.name) {
                let weighted = raw * rc.weight;
                value += weighted;
                components.insert(rc.name.clone(), weighted);
            }
        }
        Reward {
            value,
            components,
            timestamp: Utc::now(),
        }
    }
}

impl Default for RewardShaper {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// RL algorithm selector
// ---------------------------------------------------------------------------

/// Which RL algorithm to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RlAlgorithm {
    QLearning,
    PolicyGradient,
    Bandit,
}

/// Configuration for the RL optimizer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RlConfig {
    pub algorithm: RlAlgorithm,
    pub learning_rate: f64,
    pub discount_factor: f64,
    pub epsilon: f64,
    pub epsilon_decay: f64,
    pub epsilon_min: f64,
    pub replay_buffer_size: usize,
    pub batch_size: usize,
    pub train_interval: u64,
}

impl Default for RlConfig {
    fn default() -> Self {
        Self {
            algorithm: RlAlgorithm::QLearning,
            learning_rate: 0.1,
            discount_factor: 0.99,
            epsilon: 1.0,
            epsilon_decay: 0.995,
            epsilon_min: 0.01,
            replay_buffer_size: 10_000,
            batch_size: 32,
            train_interval: 1,
        }
    }
}

// ---------------------------------------------------------------------------
// Training & optimizer stats
// ---------------------------------------------------------------------------

/// Result of a single training step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingResult {
    pub loss: f64,
    pub q_value_mean: f64,
    pub epsilon_current: f64,
    pub experiences_used: usize,
}

/// Aggregate optimizer statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizerStats {
    pub total_experiences: u64,
    pub total_episodes: u64,
    pub total_train_steps: u64,
    pub average_reward: f64,
    pub best_episode_reward: f64,
    pub current_epsilon: f64,
    pub q_table_size: usize,
    pub unique_states_seen: usize,
    pub unique_actions_taken: usize,
}

// ---------------------------------------------------------------------------
// RlOptimizer — the main orchestrator
// ---------------------------------------------------------------------------

/// Central RL optimiser that ties together Q-table / policy gradient, replay
/// buffer, exploration strategy, and statistics tracking.
pub struct RlOptimizer {
    config: RlConfig,
    q_table: QTable,
    policy: PolicyGradient,
    epsilon_greedy: EpsilonGreedy,
    replay_buffer: ReplayBuffer,
    episode_experiences: HashMap<String, Vec<Experience>>,
    total_experiences: u64,
    total_episodes: u64,
    total_train_steps: u64,
    reward_sum: f64,
    best_episode_reward: f64,
    unique_states: HashMap<String, bool>,
    unique_actions: HashMap<String, bool>,
}

impl RlOptimizer {
    pub fn new(config: RlConfig) -> Self {
        let epsilon_greedy =
            EpsilonGreedy::new(config.epsilon, config.epsilon_decay, config.epsilon_min);
        let replay_buffer = ReplayBuffer::new(config.replay_buffer_size);
        Self {
            config,
            q_table: QTable::new(),
            policy: PolicyGradient::new(),
            epsilon_greedy,
            replay_buffer,
            episode_experiences: HashMap::new(),
            total_experiences: 0,
            total_episodes: 0,
            total_train_steps: 0,
            reward_sum: 0.0,
            best_episode_reward: f64::NEG_INFINITY,
            unique_states: HashMap::new(),
            unique_actions: HashMap::new(),
        }
    }

    /// Record a new experience into the replay buffer and episode map.
    pub fn record_experience(&mut self, experience: Experience) {
        self.unique_states
            .entry(experience.state.state_id.clone())
            .or_insert(true);
        self.unique_states
            .entry(experience.next_state.state_id.clone())
            .or_insert(true);
        self.unique_actions
            .entry(experience.action.action_id.clone())
            .or_insert(true);
        self.reward_sum += experience.reward.value;
        self.total_experiences += 1;

        let episode_id = experience.episode_id.clone();
        self.replay_buffer.add(experience.clone());
        self.episode_experiences
            .entry(episode_id)
            .or_default()
            .push(experience);

        debug!(
            total = self.total_experiences,
            buffer_len = self.replay_buffer.len(),
            "recorded experience"
        );
    }

    /// Run one training step using the configured algorithm.
    pub fn train_step(&mut self) -> Result<TrainingResult> {
        if self.replay_buffer.is_empty() {
            bail!("replay buffer is empty — cannot train");
        }

        let batch = self.replay_buffer.sample(self.config.batch_size);
        let experiences_used = batch.len();
        let mut total_loss = 0.0_f64;
        let mut total_q = 0.0_f64;

        match self.config.algorithm {
            RlAlgorithm::QLearning | RlAlgorithm::Bandit => {
                for exp in &batch {
                    let next_max_q = if exp.done {
                        0.0
                    } else {
                        // Collect actions seen for the next state.
                        let next_actions: Vec<String> =
                            self.unique_actions.keys().cloned().collect();
                        next_actions
                            .iter()
                            .map(|a| self.q_table.get_q(&exp.next_state.state_id, a))
                            .fold(0.0_f64, f64::max)
                    };

                    let old_q = self
                        .q_table
                        .get_q(&exp.state.state_id, &exp.action.action_id);

                    self.q_table.update_q(
                        &exp.state.state_id,
                        &exp.action.action_id,
                        exp.reward.value,
                        next_max_q,
                        self.config.learning_rate,
                        self.config.discount_factor,
                    );

                    let new_q = self
                        .q_table
                        .get_q(&exp.state.state_id, &exp.action.action_id);
                    total_loss += (new_q - old_q).abs();
                    total_q += new_q;
                }
            }
            RlAlgorithm::PolicyGradient => {
                // Gather a full episode from the most recent completed episode, or
                // fall back to the batch as individual steps.
                let episodes: Vec<Vec<Experience>> =
                    self.episode_experiences.values().cloned().collect();
                if let Some(ep) = episodes.last() {
                    self.policy.update_weights(ep, self.config.learning_rate);
                    total_loss =
                        ep.iter().map(|e| e.reward.value.abs()).sum::<f64>() / ep.len() as f64;
                    total_q = 0.0;
                }
            }
        }

        self.total_train_steps += 1;
        let q_value_mean = if experiences_used > 0 {
            total_q / experiences_used as f64
        } else {
            0.0
        };

        info!(
            step = self.total_train_steps,
            loss = total_loss,
            q_mean = q_value_mean,
            "training step complete"
        );

        Ok(TrainingResult {
            loss: total_loss,
            q_value_mean,
            epsilon_current: self.epsilon_greedy.epsilon,
            experiences_used,
        })
    }

    /// Select an action for the given state using the configured strategy.
    pub fn select_action(&self, state: &State, available_actions: &[Action]) -> Action {
        if available_actions.is_empty() {
            return Action {
                action_id: String::new(),
                name: String::new(),
                parameters: HashMap::new(),
            };
        }

        match self.config.algorithm {
            RlAlgorithm::QLearning | RlAlgorithm::Bandit => {
                let action_ids: Vec<String> = available_actions
                    .iter()
                    .map(|a| a.action_id.clone())
                    .collect();
                let (chosen_id, was_explore) =
                    self.epsilon_greedy
                        .select_action(&self.q_table, &state.state_id, &action_ids);
                debug!(
                    action = %chosen_id,
                    exploration = was_explore,
                    "action selected (epsilon-greedy)"
                );
                available_actions
                    .iter()
                    .find(|a| a.action_id == chosen_id)
                    .cloned()
                    .unwrap_or_else(|| available_actions[0].clone())
            }
            RlAlgorithm::PolicyGradient => {
                let action_ids: Vec<String> = available_actions
                    .iter()
                    .map(|a| a.action_id.clone())
                    .collect();
                let chosen_id = self.policy.select_action(&action_ids);
                debug!(action = %chosen_id, "action selected (policy gradient)");
                available_actions
                    .iter()
                    .find(|a| a.action_id == chosen_id)
                    .cloned()
                    .unwrap_or_else(|| available_actions[0].clone())
            }
        }
    }

    /// Finalise an episode: compute returns, update stats, and decay epsilon.
    ///
    /// Only increments episode count and decays epsilon if the episode is known
    /// (i.e., has recorded experiences). Unknown episodes are logged as warnings.
    pub fn episode_complete(&mut self, episode_id: &str) {
        if let Some(experiences) = self.episode_experiences.get(episode_id) {
            let episode_reward: f64 = experiences.iter().map(|e| e.reward.value).sum();
            if episode_reward > self.best_episode_reward {
                self.best_episode_reward = episode_reward;
            }
            info!(
                episode = %episode_id,
                reward = episode_reward,
                steps = experiences.len(),
                "episode complete"
            );
            self.total_episodes += 1;
            self.epsilon_greedy.decay();
        } else {
            warn!(episode = %episode_id, "episode_complete called for unknown episode");
        }
    }

    /// Current optimizer statistics.
    pub fn optimizer_stats(&self) -> OptimizerStats {
        let average_reward = if self.total_experiences > 0 {
            self.reward_sum / self.total_experiences as f64
        } else {
            0.0
        };
        OptimizerStats {
            total_experiences: self.total_experiences,
            total_episodes: self.total_episodes,
            total_train_steps: self.total_train_steps,
            average_reward,
            best_episode_reward: if self.best_episode_reward == f64::NEG_INFINITY {
                0.0
            } else {
                self.best_episode_reward
            },
            current_epsilon: self.epsilon_greedy.epsilon,
            q_table_size: self.q_table.len(),
            unique_states_seen: self.unique_states.len(),
            unique_actions_taken: self.unique_actions.len(),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- helpers --

    fn make_state(id: &str, features: &[(&str, f64)]) -> State {
        State {
            state_id: id.to_string(),
            features: features.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
            timestamp: Utc::now(),
        }
    }

    fn make_action(id: &str, name: &str) -> Action {
        Action {
            action_id: id.to_string(),
            name: name.to_string(),
            parameters: HashMap::new(),
        }
    }

    fn make_reward(value: f64) -> Reward {
        Reward {
            value,
            components: HashMap::new(),
            timestamp: Utc::now(),
        }
    }

    fn make_experience(
        state_id: &str,
        action_id: &str,
        reward: f64,
        next_state_id: &str,
        done: bool,
        episode: &str,
    ) -> Experience {
        Experience {
            state: make_state(state_id, &[("x", 1.0)]),
            action: make_action(action_id, action_id),
            reward: make_reward(reward),
            next_state: make_state(next_state_id, &[("x", 2.0)]),
            done,
            episode_id: episode.to_string(),
        }
    }

    // -----------------------------------------------------------------------
    // State tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_state_feature_vector_sorted() {
        let s = make_state("s1", &[("z", 3.0), ("a", 1.0), ("m", 2.0)]);
        assert_eq!(s.feature_vector(), vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn test_state_feature_vector_empty() {
        let s = make_state("s1", &[]);
        assert!(s.feature_vector().is_empty());
    }

    #[test]
    fn test_state_distance_identical() {
        let s = make_state("s1", &[("a", 1.0), ("b", 2.0)]);
        assert!((s.distance(&s) - 0.0).abs() < 1e-9);
    }

    #[test]
    fn test_state_distance_known() {
        let s1 = make_state("s1", &[("a", 0.0), ("b", 0.0)]);
        let s2 = make_state("s2", &[("a", 3.0), ("b", 4.0)]);
        assert!((s1.distance(&s2) - 5.0).abs() < 1e-9);
    }

    #[test]
    fn test_state_distance_different_dimensions() {
        let s1 = make_state("s1", &[("a", 1.0)]);
        let s2 = make_state("s2", &[("a", 1.0), ("b", 3.0)]);
        // distance = sqrt(0 + 9) = 3
        assert!((s1.distance(&s2) - 3.0).abs() < 1e-9);
    }

    // -----------------------------------------------------------------------
    // Experience creation
    // -----------------------------------------------------------------------

    #[test]
    fn test_experience_creation() {
        let exp = make_experience("s0", "a0", 1.0, "s1", false, "ep1");
        assert_eq!(exp.state.state_id, "s0");
        assert_eq!(exp.action.action_id, "a0");
        assert!((exp.reward.value - 1.0).abs() < 1e-9);
        assert_eq!(exp.next_state.state_id, "s1");
        assert!(!exp.done);
        assert_eq!(exp.episode_id, "ep1");
    }

    #[test]
    fn test_experience_done_flag() {
        let exp = make_experience("s0", "a0", 0.0, "s1", true, "ep1");
        assert!(exp.done);
    }

    // -----------------------------------------------------------------------
    // Replay buffer tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_replay_buffer_add_and_len() {
        let mut buf = ReplayBuffer::new(10);
        assert!(buf.is_empty());
        buf.add(make_experience("s0", "a0", 1.0, "s1", false, "ep1"));
        assert_eq!(buf.len(), 1);
        assert!(!buf.is_empty());
    }

    #[test]
    fn test_replay_buffer_capacity_overflow() {
        let mut buf = ReplayBuffer::new(3);
        for i in 0..5 {
            buf.add(make_experience(
                &format!("s{i}"),
                "a0",
                i as f64,
                "s_next",
                false,
                "ep1",
            ));
        }
        assert_eq!(buf.len(), 3);
    }

    #[test]
    fn test_replay_buffer_circular_overwrites_oldest() {
        let mut buf = ReplayBuffer::new(2);
        buf.add(make_experience("first", "a0", 0.0, "s1", false, "ep1"));
        buf.add(make_experience("second", "a0", 0.0, "s1", false, "ep1"));
        buf.add(make_experience("third", "a0", 0.0, "s1", false, "ep1"));
        assert_eq!(buf.len(), 2);
        // "first" should have been overwritten.
        let ids: Vec<&str> = buf
            .buffer
            .iter()
            .map(|e| e.state.state_id.as_str())
            .collect();
        assert!(ids.contains(&"third"));
        assert!(ids.contains(&"second") || ids.contains(&"third"));
        assert!(!ids.contains(&"first") || buf.len() < 3);
    }

    #[test]
    fn test_replay_buffer_sample_empty() {
        let buf = ReplayBuffer::new(10);
        let s = buf.sample(5);
        assert!(s.is_empty());
    }

    #[test]
    fn test_replay_buffer_sample_returns_correct_count() {
        let mut buf = ReplayBuffer::new(100);
        for i in 0..50 {
            buf.add(make_experience(
                &format!("s{i}"),
                "a0",
                1.0,
                "sn",
                false,
                "ep",
            ));
        }
        let s = buf.sample(10);
        assert_eq!(s.len(), 10);
    }

    #[test]
    fn test_replay_buffer_sample_capped_by_len() {
        let mut buf = ReplayBuffer::new(100);
        buf.add(make_experience("s0", "a0", 1.0, "s1", false, "ep"));
        let s = buf.sample(10);
        assert_eq!(s.len(), 1);
    }

    #[test]
    fn test_replay_buffer_clear() {
        let mut buf = ReplayBuffer::new(10);
        buf.add(make_experience("s0", "a0", 1.0, "s1", false, "ep"));
        buf.clear();
        assert!(buf.is_empty());
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_replay_buffer_prioritized_add() {
        let mut buf = ReplayBuffer::new(10);
        buf.add_with_priority(make_experience("s0", "a0", 1.0, "s1", false, "ep"), 10.0);
        assert_eq!(buf.len(), 1);
        assert!((buf.priorities[0] - 10.0).abs() < 1e-9);
    }

    #[test]
    fn test_replay_buffer_prioritized_sample_nonempty() {
        let mut buf = ReplayBuffer::new(100);
        for i in 0..20 {
            buf.add_with_priority(
                make_experience(&format!("s{i}"), "a0", 1.0, "sn", false, "ep"),
                (i + 1) as f64,
            );
        }
        let s = buf.sample_prioritized(5);
        assert_eq!(s.len(), 5);
    }

    #[test]
    fn test_replay_buffer_prioritized_sample_empty() {
        let buf = ReplayBuffer::new(10);
        let s = buf.sample_prioritized(5);
        assert!(s.is_empty());
    }

    #[test]
    fn test_replay_buffer_prioritized_bias() {
        // High-priority item should be sampled more frequently.
        let mut buf = ReplayBuffer::new(10);
        buf.add_with_priority(make_experience("low", "a0", 0.0, "sn", false, "ep"), 0.001);
        buf.add_with_priority(
            make_experience("high", "a0", 1.0, "sn", false, "ep"),
            1000.0,
        );
        let mut high_count = 0;
        for _ in 0..100 {
            let s = buf.sample_prioritized(1);
            if s[0].state.state_id == "high" {
                high_count += 1;
            }
        }
        assert!(
            high_count > 80,
            "high priority should dominate: got {high_count}/100"
        );
    }

    // -----------------------------------------------------------------------
    // Q-table tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_qtable_default_zero() {
        let q = QTable::new();
        assert!((q.get_q("s", "a") - 0.0).abs() < 1e-9);
    }

    #[test]
    fn test_qtable_update_and_get() {
        let mut q = QTable::new();
        // Q = 0 + 0.1 * (1.0 + 0.99*0.0 - 0.0) = 0.1
        q.update_q("s0", "a0", 1.0, 0.0, 0.1, 0.99);
        assert!((q.get_q("s0", "a0") - 0.1).abs() < 1e-9);
    }

    #[test]
    fn test_qtable_bellman_update() {
        let mut q = QTable::new();
        // Pre-set a value so we can test the full formula.
        q.values.insert(("s0".into(), "a0".into()), 5.0);
        // Q = 5.0 + 0.5 * (2.0 + 0.9 * 10.0 - 5.0) = 5.0 + 0.5 * (2+9-5) = 5.0 + 3.0 = 8.0
        q.update_q("s0", "a0", 2.0, 10.0, 0.5, 0.9);
        assert!((q.get_q("s0", "a0") - 8.0).abs() < 1e-9);
    }

    #[test]
    fn test_qtable_best_action() {
        let mut q = QTable::new();
        q.values.insert(("s0".into(), "a1".into()), 1.0);
        q.values.insert(("s0".into(), "a2".into()), 5.0);
        q.values.insert(("s0".into(), "a3".into()), 3.0);
        let best = q
            .best_action("s0", &["a1".into(), "a2".into(), "a3".into()])
            .unwrap();
        assert_eq!(best, "a2");
    }

    #[test]
    fn test_qtable_best_action_empty() {
        let q = QTable::new();
        assert!(q.best_action("s0", &[]).is_none());
    }

    #[test]
    fn test_qtable_best_action_all_equal() {
        let q = QTable::new();
        // All unseen => all 0.0 => first should win.
        let best = q.best_action("s0", &["a1".into(), "a2".into()]).unwrap();
        assert_eq!(best, "a1");
    }

    #[test]
    fn test_qtable_state_values() {
        let mut q = QTable::new();
        q.values.insert(("s0".into(), "a1".into()), 1.0);
        q.values.insert(("s0".into(), "a2".into()), 5.0);
        q.values.insert(("s1".into(), "a1".into()), 3.0);
        let sv = q.state_values();
        assert!((sv["s0"] - 5.0).abs() < 1e-9);
        assert!((sv["s1"] - 3.0).abs() < 1e-9);
    }

    #[test]
    fn test_qtable_len() {
        let mut q = QTable::new();
        assert_eq!(q.len(), 0);
        q.update_q("s", "a", 1.0, 0.0, 0.1, 0.99);
        assert_eq!(q.len(), 1);
    }

    // -----------------------------------------------------------------------
    // Epsilon-greedy tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_epsilon_greedy_pure_exploration() {
        let eg = EpsilonGreedy::new(1.0, 0.99, 0.01);
        let q = QTable::new();
        let actions: Vec<String> = vec!["a1".into(), "a2".into()];
        // With epsilon=1.0, everything is exploration.
        let mut explore_count = 0;
        for _ in 0..100 {
            let (_, was_explore) = eg.select_action(&q, "s0", &actions);
            if was_explore {
                explore_count += 1;
            }
        }
        assert_eq!(explore_count, 100);
    }

    #[test]
    fn test_epsilon_greedy_pure_exploitation() {
        let eg = EpsilonGreedy::new(0.0, 0.99, 0.0);
        let mut q = QTable::new();
        q.values.insert(("s0".into(), "a1".into()), 10.0);
        q.values.insert(("s0".into(), "a2".into()), 1.0);
        let actions: Vec<String> = vec!["a1".into(), "a2".into()];
        for _ in 0..50 {
            let (action, was_explore) = eg.select_action(&q, "s0", &actions);
            assert!(!was_explore);
            assert_eq!(action, "a1");
        }
    }

    #[test]
    fn test_epsilon_greedy_decay() {
        let mut eg = EpsilonGreedy::new(1.0, 0.5, 0.01);
        eg.decay();
        assert!((eg.epsilon - 0.5).abs() < 1e-9);
        eg.decay();
        assert!((eg.epsilon - 0.25).abs() < 1e-9);
    }

    #[test]
    fn test_epsilon_greedy_decay_floor() {
        let mut eg = EpsilonGreedy::new(0.02, 0.5, 0.01);
        eg.decay(); // 0.01
        assert!((eg.epsilon - 0.01).abs() < 1e-9);
        eg.decay(); // should stay at 0.01
        assert!((eg.epsilon - 0.01).abs() < 1e-9);
    }

    #[test]
    fn test_epsilon_greedy_empty_actions() {
        let eg = EpsilonGreedy::new(0.5, 0.99, 0.01);
        let q = QTable::new();
        let (action, _) = eg.select_action(&q, "s0", &[]);
        assert!(action.is_empty());
    }

    #[test]
    fn test_epsilon_greedy_single_action() {
        let eg = EpsilonGreedy::new(0.5, 0.99, 0.01);
        let q = QTable::new();
        let actions: Vec<String> = vec!["only".into()];
        for _ in 0..20 {
            let (action, _) = eg.select_action(&q, "s0", &actions);
            assert_eq!(action, "only");
        }
    }

    // -----------------------------------------------------------------------
    // Policy gradient tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_policy_gradient_probabilities_sum_to_one() {
        let mut pg = PolicyGradient::new();
        pg.policy_weights.insert("a1".into(), 1.0);
        pg.policy_weights.insert("a2".into(), 2.0);
        pg.policy_weights.insert("a3".into(), 0.5);
        let actions: Vec<String> = vec!["a1".into(), "a2".into(), "a3".into()];
        let probs = pg.action_probabilities(&actions);
        let sum: f64 = probs.iter().sum();
        assert!((sum - 1.0).abs() < 1e-9, "sum={sum}");
    }

    #[test]
    fn test_policy_gradient_equal_weights_uniform() {
        let mut pg = PolicyGradient::new();
        pg.policy_weights.insert("a1".into(), 0.0);
        pg.policy_weights.insert("a2".into(), 0.0);
        let p1 = pg.action_probability("a1");
        let p2 = pg.action_probability("a2");
        assert!((p1 - 0.5).abs() < 1e-9);
        assert!((p2 - 0.5).abs() < 1e-9);
    }

    #[test]
    fn test_policy_gradient_higher_weight_higher_prob() {
        let mut pg = PolicyGradient::new();
        pg.policy_weights.insert("good".into(), 5.0);
        pg.policy_weights.insert("bad".into(), -5.0);
        assert!(pg.action_probability("good") > pg.action_probability("bad"));
    }

    #[test]
    fn test_policy_gradient_select_action_returns_valid() {
        let mut pg = PolicyGradient::new();
        pg.policy_weights.insert("a1".into(), 1.0);
        pg.policy_weights.insert("a2".into(), 1.0);
        let actions: Vec<String> = vec!["a1".into(), "a2".into()];
        for _ in 0..50 {
            let chosen = pg.select_action(&actions);
            assert!(actions.contains(&chosen));
        }
    }

    #[test]
    fn test_policy_gradient_select_single_action() {
        let pg = PolicyGradient::new();
        let actions: Vec<String> = vec!["only".into()];
        assert_eq!(pg.select_action(&actions), "only");
    }

    #[test]
    fn test_policy_gradient_select_empty() {
        let pg = PolicyGradient::new();
        assert!(pg.select_action(&[]).is_empty());
    }

    #[test]
    fn test_policy_gradient_update_weights() {
        let mut pg = PolicyGradient::new();
        pg.policy_weights.insert("a1".into(), 0.0);
        pg.policy_weights.insert("a2".into(), 0.0);
        // Episode: action a1 with positive reward.
        let episode = vec![make_experience("s0", "a1", 10.0, "s1", true, "ep1")];
        pg.update_weights(&episode, 0.1);
        // a1 should have increased weight relative to a2.
        assert!(pg.policy_weights["a1"] > pg.policy_weights["a2"]);
    }

    #[test]
    fn test_policy_gradient_update_empty_episode() {
        let mut pg = PolicyGradient::new();
        pg.policy_weights.insert("a1".into(), 1.0);
        let before = pg.policy_weights["a1"];
        pg.update_weights(&[], 0.1);
        assert!((pg.policy_weights["a1"] - before).abs() < 1e-9);
    }

    // -----------------------------------------------------------------------
    // Reward shaping tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_reward_shaper_weighted_sum() {
        let mut rs = RewardShaper::new();
        rs.add_component("speed", 0.5);
        rs.add_component("accuracy", 0.5);
        let mut raw = HashMap::new();
        raw.insert("speed".to_string(), 2.0);
        raw.insert("accuracy".to_string(), 4.0);
        let reward = rs.compute_reward(&raw);
        // 0.5*2 + 0.5*4 = 3.0
        assert!((reward.value - 3.0).abs() < 1e-9);
    }

    #[test]
    fn test_reward_shaper_missing_component() {
        let mut rs = RewardShaper::new();
        rs.add_component("speed", 1.0);
        rs.add_component("missing", 1.0);
        let mut raw = HashMap::new();
        raw.insert("speed".to_string(), 5.0);
        let reward = rs.compute_reward(&raw);
        assert!((reward.value - 5.0).abs() < 1e-9);
    }

    #[test]
    fn test_reward_shaper_normalize_weights() {
        let mut rs = RewardShaper::new();
        rs.add_component("a", 2.0);
        rs.add_component("b", 8.0);
        rs.normalize_weights();
        assert!((rs.components[0].weight - 0.2).abs() < 1e-9);
        assert!((rs.components[1].weight - 0.8).abs() < 1e-9);
    }

    #[test]
    fn test_reward_shaper_normalize_zero_weights() {
        let mut rs = RewardShaper::new();
        rs.add_component("a", 0.0);
        rs.normalize_weights();
        assert!((rs.components[0].weight - 0.0).abs() < 1e-9);
    }

    #[test]
    fn test_reward_shaper_empty() {
        let rs = RewardShaper::new();
        let raw = HashMap::new();
        let r = rs.compute_reward(&raw);
        assert!((r.value - 0.0).abs() < 1e-9);
    }

    #[test]
    fn test_reward_components_breakdown() {
        let mut rs = RewardShaper::new();
        rs.add_component("latency", -1.0);
        rs.add_component("success", 1.0);
        let mut raw = HashMap::new();
        raw.insert("latency".to_string(), 0.3);
        raw.insert("success".to_string(), 1.0);
        let reward = rs.compute_reward(&raw);
        assert!((reward.components["latency"] - (-0.3)).abs() < 1e-9);
        assert!((reward.components["success"] - 1.0).abs() < 1e-9);
        assert!((reward.value - 0.7).abs() < 1e-9);
    }

    // -----------------------------------------------------------------------
    // Config defaults
    // -----------------------------------------------------------------------

    #[test]
    fn test_config_defaults() {
        let cfg = RlConfig::default();
        assert_eq!(cfg.algorithm, RlAlgorithm::QLearning);
        assert!((cfg.learning_rate - 0.1).abs() < 1e-9);
        assert!((cfg.discount_factor - 0.99).abs() < 1e-9);
        assert!((cfg.epsilon - 1.0).abs() < 1e-9);
        assert!((cfg.epsilon_decay - 0.995).abs() < 1e-9);
        assert!((cfg.epsilon_min - 0.01).abs() < 1e-9);
        assert_eq!(cfg.replay_buffer_size, 10_000);
        assert_eq!(cfg.batch_size, 32);
        assert_eq!(cfg.train_interval, 1);
    }

    // -----------------------------------------------------------------------
    // RlOptimizer tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_optimizer_new() {
        let opt = RlOptimizer::new(RlConfig::default());
        let stats = opt.optimizer_stats();
        assert_eq!(stats.total_experiences, 0);
        assert_eq!(stats.total_episodes, 0);
        assert_eq!(stats.total_train_steps, 0);
    }

    #[test]
    fn test_optimizer_record_experience() {
        let mut opt = RlOptimizer::new(RlConfig::default());
        opt.record_experience(make_experience("s0", "a0", 1.0, "s1", false, "ep1"));
        let stats = opt.optimizer_stats();
        assert_eq!(stats.total_experiences, 1);
        assert_eq!(stats.unique_states_seen, 2); // s0 and s1
        assert_eq!(stats.unique_actions_taken, 1);
    }

    #[test]
    fn test_optimizer_train_step_empty_fails() {
        let mut opt = RlOptimizer::new(RlConfig::default());
        assert!(opt.train_step().is_err());
    }

    #[test]
    fn test_optimizer_train_step_qlearning() {
        let mut opt = RlOptimizer::new(RlConfig::default());
        for i in 0..10 {
            opt.record_experience(make_experience(
                &format!("s{i}"),
                "a0",
                1.0,
                &format!("s{}", i + 1),
                false,
                "ep1",
            ));
        }
        let result = opt.train_step().unwrap();
        assert!(result.experiences_used > 0);
        assert!((result.epsilon_current - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_optimizer_train_step_policy_gradient() {
        let mut cfg = RlConfig::default();
        cfg.algorithm = RlAlgorithm::PolicyGradient;
        let mut opt = RlOptimizer::new(cfg);
        for i in 0..5 {
            opt.record_experience(make_experience(
                &format!("s{i}"),
                "a0",
                1.0,
                &format!("s{}", i + 1),
                false,
                "ep1",
            ));
        }
        let result = opt.train_step().unwrap();
        assert!(result.experiences_used > 0);
    }

    #[test]
    fn test_optimizer_select_action_qlearning() {
        let opt = RlOptimizer::new(RlConfig::default());
        let state = make_state("s0", &[("x", 1.0)]);
        let actions = vec![make_action("a1", "go"), make_action("a2", "stop")];
        let chosen = opt.select_action(&state, &actions);
        assert!(["a1", "a2"].contains(&chosen.action_id.as_str()));
    }

    #[test]
    fn test_optimizer_select_action_policy_gradient() {
        let mut cfg = RlConfig::default();
        cfg.algorithm = RlAlgorithm::PolicyGradient;
        let opt = RlOptimizer::new(cfg);
        let state = make_state("s0", &[("x", 1.0)]);
        let actions = vec![make_action("a1", "go")];
        let chosen = opt.select_action(&state, &actions);
        assert_eq!(chosen.action_id, "a1");
    }

    #[test]
    fn test_optimizer_select_action_empty() {
        let opt = RlOptimizer::new(RlConfig::default());
        let state = make_state("s0", &[]);
        let chosen = opt.select_action(&state, &[]);
        assert!(chosen.action_id.is_empty());
    }

    #[test]
    fn test_optimizer_episode_complete() {
        let mut opt = RlOptimizer::new(RlConfig::default());
        opt.record_experience(make_experience("s0", "a0", 5.0, "s1", false, "ep1"));
        opt.record_experience(make_experience("s1", "a0", 3.0, "s2", true, "ep1"));
        opt.episode_complete("ep1");
        let stats = opt.optimizer_stats();
        assert_eq!(stats.total_episodes, 1);
        assert!((stats.best_episode_reward - 8.0).abs() < 1e-9);
    }

    #[test]
    fn test_optimizer_episode_complete_unknown() {
        let mut opt = RlOptimizer::new(RlConfig::default());
        opt.episode_complete("nonexistent");
        // Unknown episodes should NOT increment stats or decay epsilon
        assert_eq!(opt.optimizer_stats().total_episodes, 0);
    }

    #[test]
    fn test_optimizer_epsilon_decays_on_episode_complete() {
        let mut opt = RlOptimizer::new(RlConfig::default());
        // Record an experience so the episode is known
        opt.record_experience(make_experience("s0", "a0", 1.0, "s1", true, "ep1"));
        let before = opt.epsilon_greedy.epsilon;
        opt.episode_complete("ep1");
        assert!(opt.epsilon_greedy.epsilon < before);
    }

    #[test]
    fn test_optimizer_stats_average_reward() {
        let mut opt = RlOptimizer::new(RlConfig::default());
        opt.record_experience(make_experience("s0", "a0", 2.0, "s1", false, "ep1"));
        opt.record_experience(make_experience("s1", "a0", 4.0, "s2", false, "ep1"));
        let stats = opt.optimizer_stats();
        assert!((stats.average_reward - 3.0).abs() < 1e-9);
    }

    #[test]
    fn test_optimizer_stats_best_episode_default() {
        let opt = RlOptimizer::new(RlConfig::default());
        let stats = opt.optimizer_stats();
        assert!((stats.best_episode_reward - 0.0).abs() < 1e-9);
    }

    #[test]
    fn test_optimizer_multiple_episodes() {
        let mut opt = RlOptimizer::new(RlConfig::default());
        opt.record_experience(make_experience("s0", "a0", 10.0, "s1", true, "ep1"));
        opt.episode_complete("ep1");
        opt.record_experience(make_experience("s0", "a0", 20.0, "s1", true, "ep2"));
        opt.episode_complete("ep2");
        let stats = opt.optimizer_stats();
        assert_eq!(stats.total_episodes, 2);
        assert!((stats.best_episode_reward - 20.0).abs() < 1e-9);
    }

    #[test]
    fn test_optimizer_qtable_grows() {
        let mut opt = RlOptimizer::new(RlConfig::default());
        for i in 0..5 {
            opt.record_experience(make_experience(
                &format!("s{i}"),
                &format!("a{i}"),
                1.0,
                &format!("s{}", i + 1),
                false,
                "ep1",
            ));
        }
        let _ = opt.train_step();
        assert!(opt.optimizer_stats().q_table_size > 0);
    }

    #[test]
    fn test_optimizer_bandit_mode() {
        let mut cfg = RlConfig::default();
        cfg.algorithm = RlAlgorithm::Bandit;
        let mut opt = RlOptimizer::new(cfg);
        opt.record_experience(make_experience("s0", "a0", 1.0, "s0", true, "ep1"));
        let result = opt.train_step().unwrap();
        assert!(result.experiences_used > 0);
    }

    // -----------------------------------------------------------------------
    // Serialisation round-trips
    // -----------------------------------------------------------------------

    #[test]
    fn test_state_serialization() {
        let s = make_state("s0", &[("a", 1.0)]);
        let json = serde_json::to_string(&s).unwrap();
        let s2: State = serde_json::from_str(&json).unwrap();
        assert_eq!(s2.state_id, "s0");
    }

    #[test]
    fn test_action_serialization() {
        let a = make_action("a0", "go");
        let json = serde_json::to_string(&a).unwrap();
        let a2: Action = serde_json::from_str(&json).unwrap();
        assert_eq!(a2.action_id, "a0");
    }

    #[test]
    fn test_reward_serialization() {
        let r = make_reward(3.14);
        let json = serde_json::to_string(&r).unwrap();
        let r2: Reward = serde_json::from_str(&json).unwrap();
        assert!((r2.value - 3.14).abs() < 1e-9);
    }

    #[test]
    fn test_rl_algorithm_enum() {
        assert_ne!(RlAlgorithm::QLearning, RlAlgorithm::PolicyGradient);
        assert_ne!(RlAlgorithm::PolicyGradient, RlAlgorithm::Bandit);
    }

    #[test]
    fn test_training_result_fields() {
        let tr = TrainingResult {
            loss: 0.5,
            q_value_mean: 1.2,
            epsilon_current: 0.9,
            experiences_used: 32,
        };
        assert!((tr.loss - 0.5).abs() < 1e-9);
        assert_eq!(tr.experiences_used, 32);
    }
}
