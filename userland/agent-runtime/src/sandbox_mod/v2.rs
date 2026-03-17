//! Next-Generation Sandboxing Architectures (Phase 8L)
//!
//! Advanced sandbox isolation techniques beyond traditional Landlock/seccomp:
//!
//! - **Capability-based security**: fine-grained, delegatable object-capability tokens
//! - **Information flow control**: taint tracking with mandatory security labels
//! - **Time-bounded sandboxes**: auto-expiring sandboxes with resource budgets
//! - **Learned sandbox policies**: derive allow/deny rules from observed behavior
//! - **Composable sandbox profiles**: layered, mergeable sandbox configurations

use std::collections::{HashMap, HashSet};

use anyhow::{bail, Result};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Capability-Based Security
// ---------------------------------------------------------------------------

/// Fine-grained, object-capability style permission.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SandboxCapability {
    /// Read files matching a glob pattern.
    FileRead { path_pattern: String },
    /// Write files matching a glob pattern.
    FileWrite { path_pattern: String },
    /// Execute files matching a glob pattern.
    FileExecute { path_pattern: String },
    /// Connect to a network host/port range.
    NetworkConnect {
        host_pattern: String,
        port_range: (u16, u16),
    },
    /// Spawn specific binaries.
    ProcessSpawn { allowed_binaries: Vec<String> },
    /// Send IPC messages to specific agents.
    IpcSend { target_agents: Vec<String> },
    /// Receive IPC messages from specific agents.
    IpcReceive { source_agents: Vec<String> },
    /// Access GPU with a VRAM limit.
    GpuAccess { max_vram_mb: u64 },
    /// Invoke specific system calls.
    SystemCall { allowed_syscalls: Vec<String> },
    /// User-defined capability with arbitrary parameters.
    Custom {
        name: String,
        parameters: HashMap<String, String>,
    },
}

/// A capability token granted to an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityToken {
    /// Unique identifier for this token.
    pub token_id: Uuid,
    /// The agent this token is granted to.
    pub agent_id: String,
    /// The capability this token authorizes.
    pub capability: SandboxCapability,
    /// When this token was granted.
    pub granted_at: DateTime<Utc>,
    /// Optional expiration time.
    pub expires_at: Option<DateTime<Utc>>,
    /// Whether this token can be delegated to another agent.
    pub delegatable: bool,
    /// Whether this token has been revoked.
    pub revoked: bool,
    /// Parent token ID if this was delegated from another token.
    pub parent_token: Option<String>,
}

/// Result of checking a capability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CapabilityVerdict {
    /// The agent holds an active token for this capability.
    Allowed,
    /// No matching token found or token is revoked.
    Denied,
    /// A matching token exists but has expired.
    Expired,
}

/// Manages capability tokens for all agents.
#[derive(Debug, Default)]
pub struct CapabilityStore {
    tokens: Vec<CapabilityToken>,
}

impl SandboxCapability {
    /// Check whether this (granted) capability matches a requested capability.
    ///
    /// For File* variants, a granted pattern ending in `*` matches any requested
    /// path that starts with the prefix before the `*`. For NetworkConnect, the
    /// requested port must fall within the granted range and the host patterns
    /// must match (exact or wildcard prefix). For list-based variants
    /// (ProcessSpawn, IpcSend, IpcReceive, SystemCall), the requested items must
    /// be a subset of the granted items. Other variants use exact equality.
    pub fn matches(&self, requested: &SandboxCapability) -> bool {
        match (self, requested) {
            // File variants: glob-prefix matching
            (
                SandboxCapability::FileRead {
                    path_pattern: granted,
                },
                SandboxCapability::FileRead { path_pattern: req },
            )
            | (
                SandboxCapability::FileWrite {
                    path_pattern: granted,
                },
                SandboxCapability::FileWrite { path_pattern: req },
            )
            | (
                SandboxCapability::FileExecute {
                    path_pattern: granted,
                },
                SandboxCapability::FileExecute { path_pattern: req },
            ) => {
                if granted == req {
                    return true;
                }
                if let Some(prefix) = granted.strip_suffix('*') {
                    req.starts_with(prefix)
                } else {
                    false
                }
            }
            // NetworkConnect: host pattern match + port in range
            (
                SandboxCapability::NetworkConnect {
                    host_pattern: granted_host,
                    port_range: granted_range,
                },
                SandboxCapability::NetworkConnect {
                    host_pattern: req_host,
                    port_range: req_range,
                },
            ) => {
                // Port: requested range must be within granted range
                let port_ok = req_range.0 >= granted_range.0 && req_range.1 <= granted_range.1;
                // Host: exact match or wildcard prefix match
                let host_ok = if granted_host == req_host {
                    true
                } else if let Some(suffix) = granted_host.strip_prefix('*') {
                    req_host.ends_with(suffix)
                } else {
                    false
                };
                host_ok && port_ok
            }
            // List-based: requested items must be subset of granted
            (
                SandboxCapability::ProcessSpawn {
                    allowed_binaries: granted,
                },
                SandboxCapability::ProcessSpawn {
                    allowed_binaries: req,
                },
            ) => req.iter().all(|r| granted.contains(r)),
            (
                SandboxCapability::IpcSend {
                    target_agents: granted,
                },
                SandboxCapability::IpcSend { target_agents: req },
            ) => req.iter().all(|r| granted.contains(r)),
            (
                SandboxCapability::IpcReceive {
                    source_agents: granted,
                },
                SandboxCapability::IpcReceive { source_agents: req },
            ) => req.iter().all(|r| granted.contains(r)),
            (
                SandboxCapability::SystemCall {
                    allowed_syscalls: granted,
                },
                SandboxCapability::SystemCall {
                    allowed_syscalls: req,
                },
            ) => req.iter().all(|r| granted.contains(r)),
            // Exact-match variants
            (
                SandboxCapability::GpuAccess {
                    max_vram_mb: granted,
                },
                SandboxCapability::GpuAccess { max_vram_mb: req },
            ) => req <= granted,
            (
                SandboxCapability::Custom {
                    name: gn,
                    parameters: gp,
                },
                SandboxCapability::Custom {
                    name: rn,
                    parameters: rp,
                },
            ) => gn == rn && gp == rp,
            // Mismatched variant types
            _ => false,
        }
    }
}

/// Validate that a path pattern does not contain path traversal sequences.
fn validate_path_pattern(pattern: &str) -> Result<()> {
    if pattern.contains("..") {
        bail!("Path pattern must not contain '..': {}", pattern);
    }
    Ok(())
}

impl CapabilityStore {
    /// Create an empty capability store.
    pub fn new() -> Self {
        Self { tokens: Vec::new() }
    }

    /// Grant a capability to an agent.
    ///
    /// If `duration` is `Some`, the token expires after that duration from now.
    /// Rejects File* capabilities whose path pattern contains `..`.
    pub fn grant(
        &mut self,
        agent_id: &str,
        capability: SandboxCapability,
        duration: Option<Duration>,
        delegatable: bool,
    ) -> Result<CapabilityToken> {
        // Validate path patterns for File* capabilities
        match &capability {
            SandboxCapability::FileRead { path_pattern }
            | SandboxCapability::FileWrite { path_pattern }
            | SandboxCapability::FileExecute { path_pattern } => {
                validate_path_pattern(path_pattern)?;
            }
            _ => {}
        }

        let now = Utc::now();
        let token = CapabilityToken {
            token_id: Uuid::new_v4(),
            agent_id: agent_id.to_string(),
            capability,
            granted_at: now,
            expires_at: duration.map(|d| now + d),
            delegatable,
            revoked: false,
            parent_token: None,
        };
        info!(
            token_id = %token.token_id,
            agent_id = agent_id,
            "Capability token granted"
        );
        self.tokens.push(token.clone());
        Ok(token)
    }

    /// Revoke a capability token by its ID, including all child tokens that
    /// were delegated from it (cascade revocation).
    pub fn revoke(&mut self, token_id: Uuid) -> Result<()> {
        let token = self
            .tokens
            .iter_mut()
            .find(|t| t.token_id == token_id)
            .ok_or_else(|| anyhow::anyhow!("Token not found: {}", token_id))?;

        if token.revoked {
            bail!("Token already revoked: {}", token_id);
        }

        token.revoked = true;
        info!(token_id = %token_id, "Capability token revoked");

        // Cascade revocation to child tokens
        let token_id_str = token_id.to_string();
        let child_ids: Vec<Uuid> = self
            .tokens
            .iter()
            .filter(|t| t.parent_token.as_deref() == Some(&token_id_str) && !t.revoked)
            .map(|t| t.token_id)
            .collect();

        for child_id in child_ids {
            // Recursively revoke children (ignore errors for already-revoked)
            let _ = self.revoke(child_id);
        }

        Ok(())
    }

    /// Check whether an agent holds an active capability.
    pub fn check(&self, agent_id: &str, capability: &SandboxCapability) -> CapabilityVerdict {
        let now = Utc::now();
        for token in &self.tokens {
            if token.agent_id != agent_id || !token.capability.matches(capability) {
                continue;
            }
            if token.revoked {
                continue;
            }
            if let Some(exp) = token.expires_at {
                if exp < now {
                    debug!(token_id = %token.token_id, "Capability token expired");
                    return CapabilityVerdict::Expired;
                }
            }
            return CapabilityVerdict::Allowed;
        }
        CapabilityVerdict::Denied
    }

    /// Delegate a token to another agent.
    ///
    /// The parent token must be delegatable and not revoked/expired.
    pub fn delegate(
        &mut self,
        parent_token_id: Uuid,
        to_agent_id: &str,
    ) -> Result<CapabilityToken> {
        let now = Utc::now();
        let parent = self
            .tokens
            .iter()
            .find(|t| t.token_id == parent_token_id)
            .ok_or_else(|| anyhow::anyhow!("Parent token not found: {}", parent_token_id))?;

        if parent.revoked {
            bail!("Cannot delegate revoked token: {}", parent_token_id);
        }
        if !parent.delegatable {
            bail!("Token is not delegatable: {}", parent_token_id);
        }
        if let Some(exp) = parent.expires_at {
            if exp < now {
                bail!("Cannot delegate expired token: {}", parent_token_id);
            }
        }
        if parent.agent_id == to_agent_id {
            bail!("Cannot delegate token to the same agent");
        }

        let child = CapabilityToken {
            token_id: Uuid::new_v4(),
            agent_id: to_agent_id.to_string(),
            capability: parent.capability.clone(),
            granted_at: now,
            expires_at: parent.expires_at,
            delegatable: false, // delegated tokens are not re-delegatable
            revoked: false,
            parent_token: Some(parent_token_id.to_string()),
        };

        info!(
            parent_token = %parent_token_id,
            child_token = %child.token_id,
            to_agent = to_agent_id,
            "Capability token delegated"
        );
        self.tokens.push(child.clone());
        Ok(child)
    }

    /// Return all tokens for a given agent.
    pub fn tokens_for_agent(&self, agent_id: &str) -> Vec<&CapabilityToken> {
        self.tokens
            .iter()
            .filter(|t| t.agent_id == agent_id)
            .collect()
    }

    /// Return all active (non-revoked, non-expired) tokens.
    pub fn active_tokens(&self) -> Vec<&CapabilityToken> {
        let now = Utc::now();
        self.tokens
            .iter()
            .filter(|t| {
                if t.revoked {
                    return false;
                }
                if let Some(exp) = t.expires_at {
                    if exp < now {
                        return false;
                    }
                }
                true
            })
            .collect()
    }

    /// Remove expired tokens from the store. Returns the number removed.
    pub fn expired_cleanup(&mut self) -> usize {
        let now = Utc::now();
        let before = self.tokens.len();
        self.tokens.retain(|t| {
            if let Some(exp) = t.expires_at {
                if exp < now {
                    debug!(token_id = %t.token_id, "Cleaning up expired token");
                    return false;
                }
            }
            true
        });
        let removed = before - self.tokens.len();
        if removed > 0 {
            info!(count = removed, "Expired capability tokens cleaned up");
        }
        removed
    }
}

// ---------------------------------------------------------------------------
// Information Flow Control
// ---------------------------------------------------------------------------

/// Mandatory security label for taint tracking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum SecurityLabel {
    Public = 0,
    Internal = 1,
    Confidential = 2,
    Secret = 3,
    TopSecret = 4,
}

impl SecurityLabel {
    /// Numeric level for comparison.
    fn level(self) -> u8 {
        self as u8
    }
}

/// Policy governing information flow between security labels.
#[derive(Debug, Default)]
pub struct FlowPolicy;

impl FlowPolicy {
    /// Check whether data can flow from one label to another.
    ///
    /// Data can only flow to the same level or higher (no downward flow).
    pub fn can_flow(from: SecurityLabel, to: SecurityLabel) -> bool {
        to.level() >= from.level()
    }
}

/// A piece of data with a security label attached.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaintedData {
    /// Unique identifier for this data.
    pub data_id: String,
    /// Security classification of this data.
    pub label: SecurityLabel,
    /// The agent that originally produced this data.
    pub source_agent: String,
    /// When the taint label was applied.
    pub created_at: DateTime<Utc>,
}

/// Result of an information flow check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FlowVerdict {
    /// The flow is permitted.
    Allowed,
    /// The flow is blocked with a reason.
    Blocked { reason: String },
}

/// A recorded flow event for lineage tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowEvent {
    /// The data that was involved.
    pub data_id: String,
    /// Source agent of the flow.
    pub from_agent: String,
    /// Destination agent of the flow.
    pub to_agent: String,
    /// Label of the source.
    pub from_label: SecurityLabel,
    /// Label of the destination.
    pub to_label: SecurityLabel,
    /// Whether the flow was allowed.
    pub allowed: bool,
    /// When the flow occurred.
    pub timestamp: DateTime<Utc>,
}

/// Tracks tainted data and its flow between agents.
#[derive(Debug, Default)]
pub struct FlowTracker {
    data: HashMap<String, TaintedData>,
    events: Vec<FlowEvent>,
    /// Maps agent_id -> SecurityLabel (clearance level).
    agent_labels: HashMap<String, SecurityLabel>,
}

impl FlowTracker {
    /// Create an empty flow tracker.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the security clearance label for an agent.
    pub fn set_agent_label(&mut self, agent_id: &str, label: SecurityLabel) {
        self.agent_labels.insert(agent_id.to_string(), label);
    }

    /// Label a piece of data with a security classification.
    pub fn label_data(&mut self, data_id: &str, label: SecurityLabel, source_agent: &str) {
        let tainted = TaintedData {
            data_id: data_id.to_string(),
            label,
            source_agent: source_agent.to_string(),
            created_at: Utc::now(),
        };
        info!(
            data_id = data_id,
            label = ?label,
            source = source_agent,
            "Data labeled"
        );
        self.data.insert(data_id.to_string(), tainted);
    }

    /// Check whether data can flow to an agent at a given label.
    pub fn check_flow(&mut self, data_id: &str, target_agent: &str) -> FlowVerdict {
        let tainted = match self.data.get(data_id) {
            Some(t) => t,
            None => return FlowVerdict::Allowed, // unlabeled data flows freely
        };

        let target_label = self
            .agent_labels
            .get(target_agent)
            .copied()
            .unwrap_or(SecurityLabel::Public);

        let allowed = FlowPolicy::can_flow(tainted.label, target_label);

        let event = FlowEvent {
            data_id: data_id.to_string(),
            from_agent: tainted.source_agent.clone(),
            to_agent: target_agent.to_string(),
            from_label: tainted.label,
            to_label: target_label,
            allowed,
            timestamp: Utc::now(),
        };
        self.events.push(event);

        if allowed {
            debug!(data_id = data_id, target = target_agent, "Flow allowed");
            FlowVerdict::Allowed
        } else {
            warn!(
                data_id = data_id,
                from = ?tainted.label,
                to = ?target_label,
                "Information flow blocked: downward flow prohibited"
            );
            FlowVerdict::Blocked {
                reason: format!(
                    "Cannot flow {:?} data to {:?} agent (downward flow prohibited)",
                    tainted.label, target_label
                ),
            }
        }
    }

    /// Propagate taint when an agent derives new data from one or more
    /// existing data items.  The derived data inherits the **highest**
    /// security label among all its sources (transitive upward propagation).
    ///
    /// If a source `data_id` is itself derived from other data, its label
    /// already reflects its ancestry, so a single pass over the direct
    /// sources suffices — the labels are monotonically non-decreasing by
    /// construction.
    ///
    /// Sources that are unlabeled are silently skipped (treated as
    /// [`SecurityLabel::Public`]).
    pub fn propagate_taint(
        &mut self,
        derived_data_id: &str,
        source_data_ids: &[&str],
        producing_agent: &str,
    ) {
        // Compute the highest label across all sources.
        let mut max_label = SecurityLabel::Public;
        for &src_id in source_data_ids {
            if let Some(src) = self.data.get(src_id) {
                if src.label > max_label {
                    max_label = src.label;
                }
            }
        }

        info!(
            derived = derived_data_id,
            sources = ?source_data_ids,
            label = ?max_label,
            agent = producing_agent,
            "Taint propagated to derived data"
        );

        self.label_data(derived_data_id, max_label, producing_agent);
    }

    /// Return the lineage (flow history) for a piece of data.
    pub fn data_lineage(&self, data_id: &str) -> Vec<&FlowEvent> {
        self.events
            .iter()
            .filter(|e| e.data_id == data_id)
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Time-Bounded Sandbox
// ---------------------------------------------------------------------------

/// Configuration for a time-bounded sandbox.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeBoundedConfig {
    /// The agent this sandbox belongs to.
    pub agent_id: String,
    /// When the sandbox was created.
    pub start_time: DateTime<Utc>,
    /// Maximum wall-clock duration.
    pub max_duration: chrono::Duration,
    /// Maximum CPU seconds allowed.
    pub max_cpu_seconds: u64,
    /// Maximum number of operations allowed.
    pub max_operations: u64,
}

/// Status of the sandbox resource budget.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BudgetStatus {
    /// All resources are within budget.
    WithinBudget,
    /// A resource has been exhausted.
    Exhausted { resource: String },
}

/// Remaining budget for a time-bounded sandbox.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetRemaining {
    /// Remaining wall-clock time in seconds.
    pub time_seconds: i64,
    /// Remaining CPU seconds.
    pub cpu_seconds: f64,
    /// Remaining operations.
    pub operations: u64,
}

/// A sandbox with time and resource budgets that auto-expires.
#[derive(Debug)]
pub struct TimeBoundedSandbox {
    pub config: TimeBoundedConfig,
    cpu_used: f64,
    operations_used: u64,
}

impl TimeBoundedSandbox {
    /// Create a new time-bounded sandbox.
    pub fn new(config: TimeBoundedConfig) -> Self {
        info!(
            agent_id = %config.agent_id,
            max_duration = ?config.max_duration,
            max_ops = config.max_operations,
            "Time-bounded sandbox created"
        );
        Self {
            config,
            cpu_used: 0.0,
            operations_used: 0,
        }
    }

    /// Check whether the sandbox is still within budget.
    pub fn check_budget(&self) -> BudgetStatus {
        if self.is_expired() {
            return BudgetStatus::Exhausted {
                resource: "wall_clock_time".to_string(),
            };
        }
        if self.cpu_used >= self.config.max_cpu_seconds as f64 {
            return BudgetStatus::Exhausted {
                resource: "cpu_seconds".to_string(),
            };
        }
        if self.operations_used >= self.config.max_operations {
            return BudgetStatus::Exhausted {
                resource: "operations".to_string(),
            };
        }
        BudgetStatus::WithinBudget
    }

    /// Record one operation.
    pub fn record_operation(&mut self) {
        self.operations_used += 1;
        debug!(
            ops = self.operations_used,
            max = self.config.max_operations,
            "Operation recorded"
        );
    }

    /// Record CPU time usage. Negative values are clamped to 0.0.
    pub fn record_cpu_time(&mut self, seconds: f64) {
        self.cpu_used += seconds.max(0.0);
        debug!(
            cpu_used = self.cpu_used,
            max = self.config.max_cpu_seconds,
            "CPU time recorded"
        );
    }

    /// Remaining budget across all resources.
    pub fn remaining(&self) -> BudgetRemaining {
        let now = Utc::now();
        let elapsed = now - self.config.start_time;
        let time_remaining = self.config.max_duration - elapsed;

        BudgetRemaining {
            time_seconds: time_remaining.num_seconds(),
            cpu_seconds: (self.config.max_cpu_seconds as f64) - self.cpu_used,
            operations: self
                .config
                .max_operations
                .saturating_sub(self.operations_used),
        }
    }

    /// Check if the sandbox has expired (wall-clock time exceeded).
    pub fn is_expired(&self) -> bool {
        let now = Utc::now();
        let elapsed = now - self.config.start_time;
        elapsed >= self.config.max_duration
    }
}

// ---------------------------------------------------------------------------
// Learned Sandbox Policy
// ---------------------------------------------------------------------------

/// An observed agent behavior for policy learning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehaviorObservation {
    /// Agent that performed the action.
    pub agent_id: String,
    /// The action that was performed (e.g. "file_read", "net_connect").
    pub action: String,
    /// The resource that was accessed (e.g. "/etc/passwd", "10.0.0.1:443").
    pub resource: String,
    /// When the action occurred.
    pub timestamp: DateTime<Utc>,
    /// Whether the action was allowed.
    pub allowed: bool,
}

/// A sandbox policy derived from observed behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearnedPolicy {
    /// Actions that should be allowed.
    pub allowed_actions: HashSet<String>,
    /// Actions that should be denied.
    pub denied_actions: HashSet<String>,
    /// Confidence in the learned policy (0.0..=1.0).
    pub confidence: f64,
    /// Number of observations used to generate this policy.
    pub observation_count: usize,
}

impl LearnedPolicy {
    /// Suggest actions that were allowed but rarely used (< 5% of observations).
    ///
    /// These are candidates for tightening the policy.
    pub fn suggest_tightening(&self, observations: &[BehaviorObservation]) -> Vec<String> {
        if observations.is_empty() {
            return Vec::new();
        }

        let total = observations.len() as f64;
        let mut action_counts: HashMap<String, usize> = HashMap::new();
        for obs in observations {
            if obs.allowed {
                *action_counts.entry(obs.action.clone()).or_default() += 1;
            }
        }

        let threshold = total * 0.05;
        let mut suggestions: Vec<String> = action_counts
            .into_iter()
            .filter(|(_, count)| (*count as f64) < threshold)
            .map(|(action, _)| action)
            .collect();
        suggestions.sort();
        suggestions
    }
}

/// Collects behavior observations and generates learned sandbox policies.
#[derive(Debug, Default)]
pub struct PolicyLearner {
    observations: Vec<BehaviorObservation>,
}

impl PolicyLearner {
    /// Create an empty policy learner.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record an observation.
    pub fn observe(&mut self, observation: BehaviorObservation) {
        debug!(
            agent = %observation.agent_id,
            action = %observation.action,
            allowed = observation.allowed,
            "Behavior observed"
        );
        self.observations.push(observation);
    }

    /// Return the current observations.
    pub fn observations(&self) -> &[BehaviorObservation] {
        &self.observations
    }

    /// Generate a policy from the collected observations.
    ///
    /// Requires at least `min_observations` before generating.
    pub fn generate_policy(&self, min_observations: usize) -> Result<LearnedPolicy> {
        if self.observations.len() < min_observations {
            bail!(
                "Not enough observations: have {}, need {}",
                self.observations.len(),
                min_observations
            );
        }

        let mut allowed_actions = HashSet::new();
        let mut denied_actions = HashSet::new();
        let mut action_allowed_count: HashMap<String, usize> = HashMap::new();
        let mut action_denied_count: HashMap<String, usize> = HashMap::new();

        for obs in &self.observations {
            let key = format!("{}:{}", obs.action, obs.resource);
            if obs.allowed {
                *action_allowed_count.entry(key.clone()).or_default() += 1;
            } else {
                *action_denied_count.entry(key.clone()).or_default() += 1;
            }
        }

        // An action is allowed if it was allowed more times than denied.
        // An action is denied if it was denied more times than allowed.
        let all_keys: HashSet<String> = action_allowed_count
            .keys()
            .chain(action_denied_count.keys())
            .cloned()
            .collect();

        for key in &all_keys {
            let allowed = action_allowed_count.get(key).copied().unwrap_or(0);
            let denied = action_denied_count.get(key).copied().unwrap_or(0);
            if allowed >= denied {
                allowed_actions.insert(key.clone());
            } else {
                denied_actions.insert(key.clone());
            }
        }

        // Confidence: proportion of observations that are unambiguous
        let total = self.observations.len() as f64;
        let unambiguous: usize = all_keys
            .iter()
            .map(|k| {
                let a = action_allowed_count.get(k).copied().unwrap_or(0);
                let d = action_denied_count.get(k).copied().unwrap_or(0);
                a.max(d)
            })
            .sum();
        let confidence = (unambiguous as f64 / total).min(1.0);

        info!(
            allowed = allowed_actions.len(),
            denied = denied_actions.len(),
            confidence = confidence,
            observations = self.observations.len(),
            "Learned policy generated"
        );

        Ok(LearnedPolicy {
            allowed_actions,
            denied_actions,
            confidence,
            observation_count: self.observations.len(),
        })
    }
}

// ---------------------------------------------------------------------------
// Composable Sandbox
// ---------------------------------------------------------------------------

/// Type of sandbox layer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LayerType {
    Filesystem,
    Network,
    Process,
    Ipc,
    Resource,
    Custom,
}

/// Verdict for a single rule.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RuleVerdict {
    /// Allow the action.
    Allow = 0,
    /// Allow but log for audit.
    AuditLog = 1,
    /// Deny the action.
    Deny = 2,
}

/// A single sandbox rule within a layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxRule {
    /// Pattern to match against the action (substring match).
    pub action_pattern: String,
    /// What to do when the pattern matches.
    pub verdict: RuleVerdict,
}

/// A named layer of sandbox rules.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxLayer {
    /// Name of this layer.
    pub name: String,
    /// What type of isolation this layer provides.
    pub layer_type: LayerType,
    /// Rules in this layer.
    pub rules: Vec<SandboxRule>,
}

/// Combined verdict from evaluating all layers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompositeVerdict {
    /// All layers allow.
    Allow,
    /// At least one layer wants to audit-log, but none deny.
    AuditLog,
    /// At least one layer denies.
    Deny,
    /// No rules matched in any layer.
    NoMatch,
}

/// A sandbox composed of multiple layers, evaluated with most-restrictive-wins.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComposableSandbox {
    pub layers: Vec<SandboxLayer>,
}

impl ComposableSandbox {
    /// Create an empty composable sandbox.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a layer.
    pub fn add_layer(&mut self, layer: SandboxLayer) {
        info!(layer = %layer.name, "Sandbox layer added");
        self.layers.push(layer);
    }

    /// Remove a layer by name. Returns true if a layer was removed.
    pub fn remove_layer(&mut self, name: &str) -> bool {
        let before = self.layers.len();
        self.layers.retain(|l| l.name != name);
        let removed = self.layers.len() < before;
        if removed {
            info!(layer = name, "Sandbox layer removed");
        }
        removed
    }

    /// Get a reference to a layer by name.
    pub fn get_layer(&self, name: &str) -> Option<&SandboxLayer> {
        self.layers.iter().find(|l| l.name == name)
    }

    /// Evaluate an action against all layers. Most restrictive verdict wins:
    /// Deny > AuditLog > Allow.
    pub fn evaluate(&self, action: &str) -> CompositeVerdict {
        let mut most_restrictive: Option<RuleVerdict> = None;

        for layer in &self.layers {
            for rule in &layer.rules {
                if action.contains(&rule.action_pattern) {
                    most_restrictive = Some(match most_restrictive {
                        None => rule.verdict.clone(),
                        Some(current) => {
                            if rule.verdict > current {
                                rule.verdict.clone()
                            } else {
                                current
                            }
                        }
                    });
                }
            }
        }

        match most_restrictive {
            None => CompositeVerdict::NoMatch,
            Some(RuleVerdict::Allow) => CompositeVerdict::Allow,
            Some(RuleVerdict::AuditLog) => CompositeVerdict::AuditLog,
            Some(RuleVerdict::Deny) => CompositeVerdict::Deny,
        }
    }

    /// Merge two sandbox profiles. All layers from both are combined.
    pub fn merge(&self, other: &ComposableSandbox) -> ComposableSandbox {
        let mut merged = self.clone();
        for layer in &other.layers {
            // If a layer with the same name exists, merge its rules.
            if let Some(existing) = merged.layers.iter_mut().find(|l| l.name == layer.name) {
                existing.rules.extend(layer.rules.clone());
            } else {
                merged.layers.push(layer.clone());
            }
        }
        info!(layers = merged.layers.len(), "Sandbox profiles merged");
        merged
    }
}

// ---------------------------------------------------------------------------
// Sandbox Metrics
// ---------------------------------------------------------------------------

/// Per-agent sandbox statistics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SandboxMetrics {
    /// Number of allowed actions.
    pub allowed_count: u64,
    /// Number of denied actions.
    pub denied_count: u64,
    /// Number of audit-logged actions.
    pub audit_count: u64,
    /// Most frequently denied actions with counts.
    pub most_denied_actions: Vec<(String, usize)>,
    /// Percentage of granted capabilities that were actually used.
    pub capability_utilization: f64,
}

impl SandboxMetrics {
    /// Create new empty metrics.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record an allowed action.
    pub fn record_allowed(&mut self) {
        self.allowed_count += 1;
    }

    /// Record a denied action.
    pub fn record_denied(&mut self, action: &str) {
        self.denied_count += 1;
        if let Some(entry) = self
            .most_denied_actions
            .iter_mut()
            .find(|(a, _)| a == action)
        {
            entry.1 += 1;
        } else {
            self.most_denied_actions.push((action.to_string(), 1));
        }
        // Keep sorted by count descending
        self.most_denied_actions.sort_by(|a, b| b.1.cmp(&a.1));
    }

    /// Record an audit-logged action.
    pub fn record_audit(&mut self) {
        self.audit_count += 1;
    }

    /// Security score: higher means more restrictive.
    ///
    /// Computed as denied / (denied + allowed). Returns 0.0 if no actions recorded.
    pub fn security_score(&self) -> f64 {
        let total = self.denied_count + self.allowed_count;
        if total == 0 {
            return 0.0;
        }
        self.denied_count as f64 / total as f64
    }
}

// ---------------------------------------------------------------------------
// Environment-Tiered Sandbox Profiles
// ---------------------------------------------------------------------------
// Inspired by SecureYeoman's dev/staging/prod/high-security presets.

/// Environment tier for sandbox configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SandboxEnvironment {
    /// Development — permissive, all tools allowed, full network.
    Dev,
    /// Staging — moderate restrictions, some tools blocked.
    Staging,
    /// Production — strict, credential proxy required, limited tools.
    Prod,
    /// High security — maximum isolation, no network, minimal tools.
    HighSecurity,
}

impl std::fmt::Display for SandboxEnvironment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Dev => write!(f, "dev"),
            Self::Staging => write!(f, "staging"),
            Self::Prod => write!(f, "prod"),
            Self::HighSecurity => write!(f, "high-security"),
        }
    }
}

/// Environment-tiered sandbox profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentProfile {
    /// Environment tier.
    pub environment: SandboxEnvironment,
    /// Maximum memory in MB.
    pub max_memory_mb: u64,
    /// CPU quota as percentage (0-100).
    pub cpu_quota_pct: u8,
    /// Network access level.
    pub network: EnvironmentNetworkPolicy,
    /// Whether credential proxy is required.
    pub require_credential_proxy: bool,
    /// MCP tools blocked in this environment.
    pub blocked_tools: Vec<String>,
    /// Seccomp mode: "basic", "strict", "lockdown".
    pub seccomp_mode: String,
    /// Whether externalization gate is enabled.
    pub externalization_gate: bool,
    /// Sandbox backend preference.
    pub sandbox_backend: SandboxBackend,
}

/// Network policy for an environment tier.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EnvironmentNetworkPolicy {
    /// Unrestricted network access.
    Unrestricted,
    /// Allow only specific ports.
    AllowPorts(Vec<u16>),
    /// No network access.
    None,
}

/// Sandbox backend preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SandboxBackend {
    /// Standard Landlock + seccomp.
    Native,
    /// gVisor (runsc) container isolation.
    GVisor,
    /// Firecracker microVM.
    Firecracker,
    /// Auto-select best available.
    Auto,
}

impl std::fmt::Display for SandboxBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Native => write!(f, "native"),
            Self::GVisor => write!(f, "gvisor"),
            Self::Firecracker => write!(f, "firecracker"),
            Self::Auto => write!(f, "auto"),
        }
    }
}

impl EnvironmentProfile {
    /// Build the dev profile — permissive for development.
    pub fn dev() -> Self {
        Self {
            environment: SandboxEnvironment::Dev,
            max_memory_mb: 4096,
            cpu_quota_pct: 90,
            network: EnvironmentNetworkPolicy::Unrestricted,
            require_credential_proxy: false,
            blocked_tools: vec![],
            seccomp_mode: "basic".to_string(),
            externalization_gate: false,
            sandbox_backend: SandboxBackend::Native,
        }
    }

    /// Build the staging profile — moderate restrictions.
    pub fn staging() -> Self {
        Self {
            environment: SandboxEnvironment::Staging,
            max_memory_mb: 2048,
            cpu_quota_pct: 70,
            network: EnvironmentNetworkPolicy::AllowPorts(vec![80, 443, 5432, 6379, 8088, 8090]),
            require_credential_proxy: false,
            blocked_tools: vec![],
            seccomp_mode: "basic".to_string(),
            externalization_gate: true,
            sandbox_backend: SandboxBackend::Native,
        }
    }

    /// Build the prod profile — strict, credential proxy required.
    pub fn prod() -> Self {
        Self {
            environment: SandboxEnvironment::Prod,
            max_memory_mb: 1024,
            cpu_quota_pct: 50,
            network: EnvironmentNetworkPolicy::AllowPorts(vec![443, 8088, 8090]),
            require_credential_proxy: true,
            blocked_tools: vec![
                "shell_exec".to_string(),
                "file_delete".to_string(),
                "docker_exec".to_string(),
            ],
            seccomp_mode: "strict".to_string(),
            externalization_gate: true,
            sandbox_backend: SandboxBackend::Auto,
        }
    }

    /// Build the high-security profile — maximum isolation.
    pub fn high_security() -> Self {
        Self {
            environment: SandboxEnvironment::HighSecurity,
            max_memory_mb: 512,
            cpu_quota_pct: 25,
            network: EnvironmentNetworkPolicy::None,
            require_credential_proxy: true,
            blocked_tools: vec![
                "shell_exec".to_string(),
                "file_delete".to_string(),
                "file_write".to_string(),
                "docker_exec".to_string(),
                "docker_run".to_string(),
                "browser_navigate".to_string(),
                "network_request".to_string(),
            ],
            seccomp_mode: "lockdown".to_string(),
            externalization_gate: true,
            sandbox_backend: SandboxBackend::Firecracker,
        }
    }

    /// Get a profile by name.
    pub fn by_name(name: &str) -> Option<Self> {
        match name {
            "dev" | "development" => Some(Self::dev()),
            "staging" | "stage" => Some(Self::staging()),
            "prod" | "production" => Some(Self::prod()),
            "high-security" | "highsec" => Some(Self::high_security()),
            _ => None,
        }
    }

    /// Check if a tool is blocked in this environment.
    pub fn is_tool_blocked(&self, tool_name: &str) -> bool {
        self.blocked_tools.iter().any(|t| t == tool_name)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Capability CRUD --

    #[test]
    fn test_grant_capability() {
        let mut store = CapabilityStore::new();
        let token = store
            .grant(
                "agent-1",
                SandboxCapability::FileRead {
                    path_pattern: "/tmp/*".to_string(),
                },
                None,
                false,
            )
            .unwrap();
        assert_eq!(token.agent_id, "agent-1");
        assert!(!token.revoked);
        assert!(token.expires_at.is_none());
    }

    #[test]
    fn test_grant_with_duration() {
        let mut store = CapabilityStore::new();
        let token = store
            .grant(
                "agent-1",
                SandboxCapability::FileRead {
                    path_pattern: "/tmp/*".to_string(),
                },
                Some(Duration::hours(1)),
                true,
            )
            .unwrap();
        assert!(token.expires_at.is_some());
        assert!(token.delegatable);
    }

    #[test]
    fn test_revoke_capability() {
        let mut store = CapabilityStore::new();
        let token = store
            .grant(
                "agent-1",
                SandboxCapability::FileRead {
                    path_pattern: "/tmp/*".to_string(),
                },
                None,
                false,
            )
            .unwrap();
        assert!(store.revoke(token.token_id).is_ok());
        assert!(store.tokens[0].revoked);
    }

    #[test]
    fn test_revoke_nonexistent_token() {
        let mut store = CapabilityStore::new();
        assert!(store.revoke(Uuid::new_v4()).is_err());
    }

    #[test]
    fn test_revoke_already_revoked() {
        let mut store = CapabilityStore::new();
        let token = store
            .grant(
                "agent-1",
                SandboxCapability::FileRead {
                    path_pattern: "/tmp/*".to_string(),
                },
                None,
                false,
            )
            .unwrap();
        store.revoke(token.token_id).unwrap();
        assert!(store.revoke(token.token_id).is_err());
    }

    #[test]
    fn test_check_allowed() {
        let mut store = CapabilityStore::new();
        let cap = SandboxCapability::FileRead {
            path_pattern: "/tmp/*".to_string(),
        };
        store.grant("agent-1", cap.clone(), None, false).unwrap();
        assert_eq!(store.check("agent-1", &cap), CapabilityVerdict::Allowed);
    }

    #[test]
    fn test_check_denied() {
        let store = CapabilityStore::new();
        let cap = SandboxCapability::FileRead {
            path_pattern: "/tmp/*".to_string(),
        };
        assert_eq!(store.check("agent-1", &cap), CapabilityVerdict::Denied);
    }

    #[test]
    fn test_check_revoked_is_denied() {
        let mut store = CapabilityStore::new();
        let cap = SandboxCapability::FileRead {
            path_pattern: "/tmp/*".to_string(),
        };
        let token = store.grant("agent-1", cap.clone(), None, false).unwrap();
        store.revoke(token.token_id).unwrap();
        assert_eq!(store.check("agent-1", &cap), CapabilityVerdict::Denied);
    }

    #[test]
    fn test_check_expired() {
        let mut store = CapabilityStore::new();
        let cap = SandboxCapability::FileRead {
            path_pattern: "/tmp/*".to_string(),
        };
        // Grant with negative duration -> already expired
        store
            .grant("agent-1", cap.clone(), Some(Duration::seconds(-1)), false)
            .unwrap();
        assert_eq!(store.check("agent-1", &cap), CapabilityVerdict::Expired);
    }

    #[test]
    fn test_delegate_token() {
        let mut store = CapabilityStore::new();
        let parent = store
            .grant(
                "agent-1",
                SandboxCapability::FileRead {
                    path_pattern: "/data/*".to_string(),
                },
                None,
                true,
            )
            .unwrap();
        let child = store.delegate(parent.token_id, "agent-2").unwrap();
        assert_eq!(child.agent_id, "agent-2");
        assert_eq!(child.parent_token, Some(parent.token_id.to_string()));
        assert!(!child.delegatable); // delegated tokens are not re-delegatable
    }

    #[test]
    fn test_delegate_non_delegatable() {
        let mut store = CapabilityStore::new();
        let parent = store
            .grant(
                "agent-1",
                SandboxCapability::FileRead {
                    path_pattern: "/data/*".to_string(),
                },
                None,
                false, // not delegatable
            )
            .unwrap();
        assert!(store.delegate(parent.token_id, "agent-2").is_err());
    }

    #[test]
    fn test_delegate_revoked_parent() {
        let mut store = CapabilityStore::new();
        let parent = store
            .grant(
                "agent-1",
                SandboxCapability::FileRead {
                    path_pattern: "/data/*".to_string(),
                },
                None,
                true,
            )
            .unwrap();
        store.revoke(parent.token_id).unwrap();
        assert!(store.delegate(parent.token_id, "agent-2").is_err());
    }

    #[test]
    fn test_delegate_expired_parent() {
        let mut store = CapabilityStore::new();
        let parent = store
            .grant(
                "agent-1",
                SandboxCapability::FileRead {
                    path_pattern: "/data/*".to_string(),
                },
                Some(Duration::seconds(-1)), // already expired
                true,
            )
            .unwrap();
        assert!(store.delegate(parent.token_id, "agent-2").is_err());
    }

    #[test]
    fn test_self_delegation_denied() {
        let mut store = CapabilityStore::new();
        let parent = store
            .grant(
                "agent-1",
                SandboxCapability::FileRead {
                    path_pattern: "/data/*".to_string(),
                },
                None,
                true,
            )
            .unwrap();
        assert!(store.delegate(parent.token_id, "agent-1").is_err());
    }

    #[test]
    fn test_delegation_chain() {
        let mut store = CapabilityStore::new();
        let root = store
            .grant(
                "agent-1",
                SandboxCapability::FileRead {
                    path_pattern: "/data/*".to_string(),
                },
                None,
                true,
            )
            .unwrap();
        let child = store.delegate(root.token_id, "agent-2").unwrap();
        // Child is not re-delegatable by default
        assert!(!child.delegatable);
        assert!(store.delegate(child.token_id, "agent-3").is_err());
    }

    #[test]
    fn test_tokens_for_agent() {
        let mut store = CapabilityStore::new();
        store
            .grant(
                "agent-1",
                SandboxCapability::FileRead {
                    path_pattern: "/a".to_string(),
                },
                None,
                false,
            )
            .unwrap();
        store
            .grant(
                "agent-1",
                SandboxCapability::FileWrite {
                    path_pattern: "/b".to_string(),
                },
                None,
                false,
            )
            .unwrap();
        store
            .grant(
                "agent-2",
                SandboxCapability::FileRead {
                    path_pattern: "/c".to_string(),
                },
                None,
                false,
            )
            .unwrap();
        assert_eq!(store.tokens_for_agent("agent-1").len(), 2);
        assert_eq!(store.tokens_for_agent("agent-2").len(), 1);
        assert_eq!(store.tokens_for_agent("agent-3").len(), 0);
    }

    #[test]
    fn test_active_tokens() {
        let mut store = CapabilityStore::new();
        let t1 = store
            .grant(
                "agent-1",
                SandboxCapability::FileRead {
                    path_pattern: "/a".to_string(),
                },
                None,
                false,
            )
            .unwrap();
        store
            .grant(
                "agent-1",
                SandboxCapability::FileWrite {
                    path_pattern: "/b".to_string(),
                },
                Some(Duration::seconds(-1)), // expired
                false,
            )
            .unwrap();
        let t3 = store
            .grant(
                "agent-2",
                SandboxCapability::FileRead {
                    path_pattern: "/c".to_string(),
                },
                None,
                false,
            )
            .unwrap();
        store.revoke(t3.token_id).unwrap();

        // Only t1 should be active (t2 expired, t3 revoked)
        let active = store.active_tokens();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].token_id, t1.token_id);
    }

    #[test]
    fn test_expired_cleanup() {
        let mut store = CapabilityStore::new();
        store
            .grant(
                "agent-1",
                SandboxCapability::FileRead {
                    path_pattern: "/a".to_string(),
                },
                Some(Duration::seconds(-10)), // expired
                false,
            )
            .unwrap();
        store
            .grant(
                "agent-1",
                SandboxCapability::FileWrite {
                    path_pattern: "/b".to_string(),
                },
                Some(Duration::seconds(-5)), // expired
                false,
            )
            .unwrap();
        store
            .grant(
                "agent-2",
                SandboxCapability::FileRead {
                    path_pattern: "/c".to_string(),
                },
                None, // no expiry
                false,
            )
            .unwrap();
        let removed = store.expired_cleanup();
        assert_eq!(removed, 2);
        assert_eq!(store.tokens.len(), 1);
    }

    #[test]
    fn test_expired_cleanup_none_expired() {
        let mut store = CapabilityStore::new();
        store
            .grant(
                "agent-1",
                SandboxCapability::FileRead {
                    path_pattern: "/a".to_string(),
                },
                None,
                false,
            )
            .unwrap();
        assert_eq!(store.expired_cleanup(), 0);
    }

    // -- Capability enum variants --

    #[test]
    fn test_capability_network_connect() {
        let mut store = CapabilityStore::new();
        let cap = SandboxCapability::NetworkConnect {
            host_pattern: "*.example.com".to_string(),
            port_range: (80, 443),
        };
        store.grant("agent-1", cap.clone(), None, false).unwrap();
        assert_eq!(store.check("agent-1", &cap), CapabilityVerdict::Allowed);
    }

    #[test]
    fn test_capability_process_spawn() {
        let mut store = CapabilityStore::new();
        let cap = SandboxCapability::ProcessSpawn {
            allowed_binaries: vec!["/usr/bin/ls".to_string()],
        };
        store.grant("agent-1", cap.clone(), None, false).unwrap();
        assert_eq!(store.check("agent-1", &cap), CapabilityVerdict::Allowed);
    }

    #[test]
    fn test_capability_ipc_send() {
        let mut store = CapabilityStore::new();
        let cap = SandboxCapability::IpcSend {
            target_agents: vec!["agent-2".to_string()],
        };
        store.grant("agent-1", cap.clone(), None, false).unwrap();
        assert_eq!(store.check("agent-1", &cap), CapabilityVerdict::Allowed);
    }

    #[test]
    fn test_capability_ipc_receive() {
        let mut store = CapabilityStore::new();
        let cap = SandboxCapability::IpcReceive {
            source_agents: vec!["agent-1".to_string()],
        };
        store.grant("agent-2", cap.clone(), None, false).unwrap();
        assert_eq!(store.check("agent-2", &cap), CapabilityVerdict::Allowed);
    }

    #[test]
    fn test_capability_gpu_access() {
        let mut store = CapabilityStore::new();
        let cap = SandboxCapability::GpuAccess { max_vram_mb: 4096 };
        store.grant("agent-1", cap.clone(), None, false).unwrap();
        assert_eq!(store.check("agent-1", &cap), CapabilityVerdict::Allowed);
    }

    #[test]
    fn test_capability_syscall() {
        let mut store = CapabilityStore::new();
        let cap = SandboxCapability::SystemCall {
            allowed_syscalls: vec!["read".to_string(), "write".to_string()],
        };
        store.grant("agent-1", cap.clone(), None, false).unwrap();
        assert_eq!(store.check("agent-1", &cap), CapabilityVerdict::Allowed);
    }

    #[test]
    fn test_capability_custom() {
        let mut store = CapabilityStore::new();
        let mut params = HashMap::new();
        params.insert("limit".to_string(), "100".to_string());
        let cap = SandboxCapability::Custom {
            name: "custom_op".to_string(),
            parameters: params,
        };
        store.grant("agent-1", cap.clone(), None, false).unwrap();
        assert_eq!(store.check("agent-1", &cap), CapabilityVerdict::Allowed);
    }

    #[test]
    fn test_capability_file_execute() {
        let mut store = CapabilityStore::new();
        let cap = SandboxCapability::FileExecute {
            path_pattern: "/usr/bin/*".to_string(),
        };
        store.grant("agent-1", cap.clone(), None, false).unwrap();
        assert_eq!(store.check("agent-1", &cap), CapabilityVerdict::Allowed);
    }

    // -- Information Flow Control --

    #[test]
    fn test_flow_policy_same_level() {
        assert!(FlowPolicy::can_flow(
            SecurityLabel::Confidential,
            SecurityLabel::Confidential
        ));
    }

    #[test]
    fn test_flow_policy_upward_allowed() {
        assert!(FlowPolicy::can_flow(
            SecurityLabel::Public,
            SecurityLabel::Secret
        ));
        assert!(FlowPolicy::can_flow(
            SecurityLabel::Internal,
            SecurityLabel::TopSecret
        ));
    }

    #[test]
    fn test_flow_policy_downward_blocked() {
        assert!(!FlowPolicy::can_flow(
            SecurityLabel::Secret,
            SecurityLabel::Public
        ));
        assert!(!FlowPolicy::can_flow(
            SecurityLabel::TopSecret,
            SecurityLabel::Confidential
        ));
    }

    #[test]
    fn test_flow_policy_all_levels() {
        // Public can flow anywhere
        assert!(FlowPolicy::can_flow(
            SecurityLabel::Public,
            SecurityLabel::Public
        ));
        assert!(FlowPolicy::can_flow(
            SecurityLabel::Public,
            SecurityLabel::TopSecret
        ));
        // TopSecret can only flow to TopSecret
        assert!(FlowPolicy::can_flow(
            SecurityLabel::TopSecret,
            SecurityLabel::TopSecret
        ));
        assert!(!FlowPolicy::can_flow(
            SecurityLabel::TopSecret,
            SecurityLabel::Secret
        ));
    }

    #[test]
    fn test_label_data() {
        let mut tracker = FlowTracker::new();
        tracker.label_data("doc-1", SecurityLabel::Secret, "agent-1");
        assert!(tracker.data.contains_key("doc-1"));
        assert_eq!(tracker.data["doc-1"].label, SecurityLabel::Secret);
    }

    #[test]
    fn test_check_flow_allowed() {
        let mut tracker = FlowTracker::new();
        tracker.label_data("doc-1", SecurityLabel::Internal, "agent-1");
        tracker.set_agent_label("agent-2", SecurityLabel::Secret);
        assert_eq!(tracker.check_flow("doc-1", "agent-2"), FlowVerdict::Allowed);
    }

    #[test]
    fn test_check_flow_blocked_downward() {
        let mut tracker = FlowTracker::new();
        tracker.label_data("doc-1", SecurityLabel::Secret, "agent-1");
        tracker.set_agent_label("agent-2", SecurityLabel::Public);
        assert!(matches!(
            tracker.check_flow("doc-1", "agent-2"),
            FlowVerdict::Blocked { .. }
        ));
    }

    #[test]
    fn test_check_flow_unlabeled_data() {
        let mut tracker = FlowTracker::new();
        // Unlabeled data should flow freely
        assert_eq!(
            tracker.check_flow("unknown-data", "agent-1"),
            FlowVerdict::Allowed
        );
    }

    #[test]
    fn test_check_flow_unlabeled_agent() {
        let mut tracker = FlowTracker::new();
        tracker.label_data("doc-1", SecurityLabel::Internal, "agent-1");
        // Agent without label defaults to Public => Internal->Public blocked
        assert!(matches!(
            tracker.check_flow("doc-1", "unlabeled-agent"),
            FlowVerdict::Blocked { .. }
        ));
    }

    #[test]
    fn test_data_lineage() {
        let mut tracker = FlowTracker::new();
        tracker.label_data("doc-1", SecurityLabel::Confidential, "agent-1");
        tracker.set_agent_label("agent-2", SecurityLabel::Secret);
        tracker.set_agent_label("agent-3", SecurityLabel::Public);
        tracker.check_flow("doc-1", "agent-2"); // allowed
        tracker.check_flow("doc-1", "agent-3"); // blocked

        let lineage = tracker.data_lineage("doc-1");
        assert_eq!(lineage.len(), 2);
        assert!(lineage[0].allowed);
        assert!(!lineage[1].allowed);
    }

    #[test]
    fn test_data_lineage_empty() {
        let tracker = FlowTracker::new();
        assert!(tracker.data_lineage("nonexistent").is_empty());
    }

    #[test]
    fn test_flow_event_fields() {
        let mut tracker = FlowTracker::new();
        tracker.label_data("doc-1", SecurityLabel::Secret, "producer");
        tracker.set_agent_label("consumer", SecurityLabel::TopSecret);
        tracker.check_flow("doc-1", "consumer");

        let events = tracker.data_lineage("doc-1");
        assert_eq!(events.len(), 1);
        let e = &events[0];
        assert_eq!(e.data_id, "doc-1");
        assert_eq!(e.from_agent, "producer");
        assert_eq!(e.to_agent, "consumer");
        assert_eq!(e.from_label, SecurityLabel::Secret);
        assert_eq!(e.to_label, SecurityLabel::TopSecret);
        assert!(e.allowed);
    }

    // -- Taint propagation --

    #[test]
    fn test_propagate_taint_single_source() {
        let mut tracker = FlowTracker::new();
        tracker.label_data("src-1", SecurityLabel::Secret, "agent-a");
        tracker.propagate_taint("derived-1", &["src-1"], "agent-b");
        assert_eq!(tracker.data["derived-1"].label, SecurityLabel::Secret);
        assert_eq!(tracker.data["derived-1"].source_agent, "agent-b");
    }

    #[test]
    fn test_propagate_taint_takes_highest_label() {
        let mut tracker = FlowTracker::new();
        tracker.label_data("src-1", SecurityLabel::Internal, "agent-a");
        tracker.label_data("src-2", SecurityLabel::TopSecret, "agent-b");
        tracker.label_data("src-3", SecurityLabel::Secret, "agent-c");
        tracker.propagate_taint("derived-1", &["src-1", "src-2", "src-3"], "agent-d");
        assert_eq!(tracker.data["derived-1"].label, SecurityLabel::TopSecret);
    }

    #[test]
    fn test_propagate_taint_transitive_chain() {
        let mut tracker = FlowTracker::new();
        // A is Secret
        tracker.label_data("a", SecurityLabel::Secret, "agent-1");
        // B derived from A => inherits Secret
        tracker.propagate_taint("b", &["a"], "agent-2");
        assert_eq!(tracker.data["b"].label, SecurityLabel::Secret);
        // C derived from B => transitively inherits Secret
        tracker.propagate_taint("c", &["b"], "agent-3");
        assert_eq!(tracker.data["c"].label, SecurityLabel::Secret);
    }

    #[test]
    fn test_propagate_taint_unknown_sources_default_public() {
        let mut tracker = FlowTracker::new();
        tracker.propagate_taint("derived-1", &["nonexistent"], "agent-x");
        assert_eq!(tracker.data["derived-1"].label, SecurityLabel::Public);
    }

    #[test]
    fn test_propagate_taint_mixed_known_unknown() {
        let mut tracker = FlowTracker::new();
        tracker.label_data("src-1", SecurityLabel::Confidential, "agent-a");
        tracker.propagate_taint("derived-1", &["src-1", "unknown"], "agent-b");
        assert_eq!(tracker.data["derived-1"].label, SecurityLabel::Confidential);
    }

    // -- Time-Bounded Sandbox --

    #[test]
    fn test_time_bounded_within_budget() {
        let sandbox = TimeBoundedSandbox::new(TimeBoundedConfig {
            agent_id: "agent-1".to_string(),
            start_time: Utc::now(),
            max_duration: Duration::hours(1),
            max_cpu_seconds: 100,
            max_operations: 1000,
        });
        assert_eq!(sandbox.check_budget(), BudgetStatus::WithinBudget);
    }

    #[test]
    fn test_time_bounded_expired() {
        let sandbox = TimeBoundedSandbox::new(TimeBoundedConfig {
            agent_id: "agent-1".to_string(),
            start_time: Utc::now() - Duration::hours(2),
            max_duration: Duration::hours(1),
            max_cpu_seconds: 100,
            max_operations: 1000,
        });
        assert!(sandbox.is_expired());
        assert_eq!(
            sandbox.check_budget(),
            BudgetStatus::Exhausted {
                resource: "wall_clock_time".to_string()
            }
        );
    }

    #[test]
    fn test_time_bounded_cpu_exhausted() {
        let mut sandbox = TimeBoundedSandbox::new(TimeBoundedConfig {
            agent_id: "agent-1".to_string(),
            start_time: Utc::now(),
            max_duration: Duration::hours(1),
            max_cpu_seconds: 10,
            max_operations: 1000,
        });
        sandbox.record_cpu_time(10.0);
        assert_eq!(
            sandbox.check_budget(),
            BudgetStatus::Exhausted {
                resource: "cpu_seconds".to_string()
            }
        );
    }

    #[test]
    fn test_time_bounded_ops_exhausted() {
        let mut sandbox = TimeBoundedSandbox::new(TimeBoundedConfig {
            agent_id: "agent-1".to_string(),
            start_time: Utc::now(),
            max_duration: Duration::hours(1),
            max_cpu_seconds: 100,
            max_operations: 5,
        });
        for _ in 0..5 {
            sandbox.record_operation();
        }
        assert_eq!(
            sandbox.check_budget(),
            BudgetStatus::Exhausted {
                resource: "operations".to_string()
            }
        );
    }

    #[test]
    fn test_time_bounded_remaining() {
        let mut sandbox = TimeBoundedSandbox::new(TimeBoundedConfig {
            agent_id: "agent-1".to_string(),
            start_time: Utc::now(),
            max_duration: Duration::hours(1),
            max_cpu_seconds: 100,
            max_operations: 1000,
        });
        sandbox.record_cpu_time(25.0);
        sandbox.record_operation();
        sandbox.record_operation();

        let remaining = sandbox.remaining();
        assert!(remaining.time_seconds > 0);
        assert!((remaining.cpu_seconds - 75.0).abs() < 0.01);
        assert_eq!(remaining.operations, 998);
    }

    #[test]
    fn test_time_bounded_record_operation_increments() {
        let mut sandbox = TimeBoundedSandbox::new(TimeBoundedConfig {
            agent_id: "agent-1".to_string(),
            start_time: Utc::now(),
            max_duration: Duration::hours(1),
            max_cpu_seconds: 100,
            max_operations: 1000,
        });
        sandbox.record_operation();
        sandbox.record_operation();
        sandbox.record_operation();
        assert_eq!(sandbox.operations_used, 3);
    }

    #[test]
    fn test_time_bounded_record_cpu_accumulates() {
        let mut sandbox = TimeBoundedSandbox::new(TimeBoundedConfig {
            agent_id: "agent-1".to_string(),
            start_time: Utc::now(),
            max_duration: Duration::hours(1),
            max_cpu_seconds: 100,
            max_operations: 1000,
        });
        sandbox.record_cpu_time(1.5);
        sandbox.record_cpu_time(2.5);
        assert!((sandbox.cpu_used - 4.0).abs() < 0.01);
    }

    // -- Learned Sandbox Policy --

    fn make_observation(action: &str, resource: &str, allowed: bool) -> BehaviorObservation {
        BehaviorObservation {
            agent_id: "agent-1".to_string(),
            action: action.to_string(),
            resource: resource.to_string(),
            timestamp: Utc::now(),
            allowed,
        }
    }

    #[test]
    fn test_observe_collects() {
        let mut learner = PolicyLearner::new();
        learner.observe(make_observation("file_read", "/tmp/a", true));
        learner.observe(make_observation("net_connect", "10.0.0.1:80", false));
        assert_eq!(learner.observations().len(), 2);
    }

    #[test]
    fn test_generate_policy_min_observations() {
        let mut learner = PolicyLearner::new();
        learner.observe(make_observation("file_read", "/tmp/a", true));
        assert!(learner.generate_policy(5).is_err());
    }

    #[test]
    fn test_generate_policy_allowed() {
        let mut learner = PolicyLearner::new();
        for _ in 0..10 {
            learner.observe(make_observation("file_read", "/tmp/a", true));
        }
        let policy = learner.generate_policy(5).unwrap();
        assert!(policy.allowed_actions.contains("file_read:/tmp/a"));
        assert!(policy.denied_actions.is_empty());
        assert_eq!(policy.observation_count, 10);
    }

    #[test]
    fn test_generate_policy_denied() {
        let mut learner = PolicyLearner::new();
        for _ in 0..10 {
            learner.observe(make_observation("net_connect", "evil.com:80", false));
        }
        let policy = learner.generate_policy(5).unwrap();
        assert!(policy.denied_actions.contains("net_connect:evil.com:80"));
        assert!(policy.allowed_actions.is_empty());
    }

    #[test]
    fn test_generate_policy_mixed() {
        let mut learner = PolicyLearner::new();
        // 7 allowed, 3 denied => should be allowed
        for _ in 0..7 {
            learner.observe(make_observation("file_read", "/tmp/a", true));
        }
        for _ in 0..3 {
            learner.observe(make_observation("file_read", "/tmp/a", false));
        }
        let policy = learner.generate_policy(5).unwrap();
        assert!(policy.allowed_actions.contains("file_read:/tmp/a"));
    }

    #[test]
    fn test_generate_policy_confidence() {
        let mut learner = PolicyLearner::new();
        for _ in 0..10 {
            learner.observe(make_observation("file_read", "/tmp/a", true));
        }
        let policy = learner.generate_policy(1).unwrap();
        assert!(policy.confidence > 0.0);
        assert!(policy.confidence <= 1.0);
    }

    #[test]
    fn test_suggest_tightening() {
        let mut learner = PolicyLearner::new();
        // "file_read" happens 100 times, "rare_op" happens once
        for _ in 0..100 {
            learner.observe(make_observation("file_read", "/tmp/a", true));
        }
        learner.observe(make_observation("rare_op", "/tmp/b", true));

        let policy = learner.generate_policy(5).unwrap();
        let suggestions = policy.suggest_tightening(learner.observations());
        assert!(suggestions.contains(&"rare_op".to_string()));
        assert!(!suggestions.contains(&"file_read".to_string()));
    }

    #[test]
    fn test_suggest_tightening_empty_observations() {
        let policy = LearnedPolicy {
            allowed_actions: HashSet::new(),
            denied_actions: HashSet::new(),
            confidence: 0.0,
            observation_count: 0,
        };
        assert!(policy.suggest_tightening(&[]).is_empty());
    }

    #[test]
    fn test_suggest_tightening_all_frequent() {
        let mut learner = PolicyLearner::new();
        for _ in 0..50 {
            learner.observe(make_observation("file_read", "/tmp/a", true));
        }
        for _ in 0..50 {
            learner.observe(make_observation("net_connect", "10.0.0.1:80", true));
        }
        let policy = learner.generate_policy(5).unwrap();
        // Both actions are > 5% so no tightening suggestions
        assert!(policy.suggest_tightening(learner.observations()).is_empty());
    }

    // -- Composable Sandbox --

    fn make_layer(
        name: &str,
        layer_type: LayerType,
        rules: Vec<(&str, RuleVerdict)>,
    ) -> SandboxLayer {
        SandboxLayer {
            name: name.to_string(),
            layer_type,
            rules: rules
                .into_iter()
                .map(|(pattern, verdict)| SandboxRule {
                    action_pattern: pattern.to_string(),
                    verdict,
                })
                .collect(),
        }
    }

    #[test]
    fn test_add_layer() {
        let mut sandbox = ComposableSandbox::new();
        sandbox.add_layer(make_layer(
            "fs",
            LayerType::Filesystem,
            vec![("file_read", RuleVerdict::Allow)],
        ));
        assert_eq!(sandbox.layers.len(), 1);
    }

    #[test]
    fn test_remove_layer() {
        let mut sandbox = ComposableSandbox::new();
        sandbox.add_layer(make_layer("fs", LayerType::Filesystem, vec![]));
        sandbox.add_layer(make_layer("net", LayerType::Network, vec![]));
        assert!(sandbox.remove_layer("fs"));
        assert_eq!(sandbox.layers.len(), 1);
        assert_eq!(sandbox.layers[0].name, "net");
    }

    #[test]
    fn test_remove_nonexistent_layer() {
        let mut sandbox = ComposableSandbox::new();
        assert!(!sandbox.remove_layer("nope"));
    }

    #[test]
    fn test_get_layer() {
        let mut sandbox = ComposableSandbox::new();
        sandbox.add_layer(make_layer("fs", LayerType::Filesystem, vec![]));
        assert!(sandbox.get_layer("fs").is_some());
        assert!(sandbox.get_layer("nope").is_none());
    }

    #[test]
    fn test_evaluate_allow() {
        let mut sandbox = ComposableSandbox::new();
        sandbox.add_layer(make_layer(
            "fs",
            LayerType::Filesystem,
            vec![("file_read", RuleVerdict::Allow)],
        ));
        assert_eq!(
            sandbox.evaluate("file_read /tmp/a"),
            CompositeVerdict::Allow
        );
    }

    #[test]
    fn test_evaluate_deny() {
        let mut sandbox = ComposableSandbox::new();
        sandbox.add_layer(make_layer(
            "fs",
            LayerType::Filesystem,
            vec![("file_read", RuleVerdict::Deny)],
        ));
        assert_eq!(sandbox.evaluate("file_read /tmp/a"), CompositeVerdict::Deny);
    }

    #[test]
    fn test_evaluate_audit() {
        let mut sandbox = ComposableSandbox::new();
        sandbox.add_layer(make_layer(
            "fs",
            LayerType::Filesystem,
            vec![("file_read", RuleVerdict::AuditLog)],
        ));
        assert_eq!(
            sandbox.evaluate("file_read /tmp/a"),
            CompositeVerdict::AuditLog
        );
    }

    #[test]
    fn test_evaluate_no_match() {
        let mut sandbox = ComposableSandbox::new();
        sandbox.add_layer(make_layer(
            "fs",
            LayerType::Filesystem,
            vec![("file_read", RuleVerdict::Allow)],
        ));
        assert_eq!(
            sandbox.evaluate("net_connect 10.0.0.1"),
            CompositeVerdict::NoMatch
        );
    }

    #[test]
    fn test_evaluate_most_restrictive_wins() {
        let mut sandbox = ComposableSandbox::new();
        sandbox.add_layer(make_layer(
            "fs",
            LayerType::Filesystem,
            vec![("file_read", RuleVerdict::Allow)],
        ));
        sandbox.add_layer(make_layer(
            "sec",
            LayerType::Custom,
            vec![("file_read", RuleVerdict::Deny)],
        ));
        // Deny wins over Allow
        assert_eq!(sandbox.evaluate("file_read /tmp/a"), CompositeVerdict::Deny);
    }

    #[test]
    fn test_evaluate_audit_over_allow() {
        let mut sandbox = ComposableSandbox::new();
        sandbox.add_layer(make_layer(
            "fs",
            LayerType::Filesystem,
            vec![("file_read", RuleVerdict::Allow)],
        ));
        sandbox.add_layer(make_layer(
            "audit",
            LayerType::Custom,
            vec![("file_read", RuleVerdict::AuditLog)],
        ));
        assert_eq!(
            sandbox.evaluate("file_read /tmp/a"),
            CompositeVerdict::AuditLog
        );
    }

    #[test]
    fn test_evaluate_all_layers_allow() {
        let mut sandbox = ComposableSandbox::new();
        sandbox.add_layer(make_layer(
            "fs",
            LayerType::Filesystem,
            vec![("file_read", RuleVerdict::Allow)],
        ));
        sandbox.add_layer(make_layer(
            "sec",
            LayerType::Custom,
            vec![("file_read", RuleVerdict::Allow)],
        ));
        assert_eq!(
            sandbox.evaluate("file_read /tmp/a"),
            CompositeVerdict::Allow
        );
    }

    #[test]
    fn test_merge_disjoint() {
        let mut s1 = ComposableSandbox::new();
        s1.add_layer(make_layer(
            "fs",
            LayerType::Filesystem,
            vec![("file_read", RuleVerdict::Allow)],
        ));

        let mut s2 = ComposableSandbox::new();
        s2.add_layer(make_layer(
            "net",
            LayerType::Network,
            vec![("net_connect", RuleVerdict::Deny)],
        ));

        let merged = s1.merge(&s2);
        assert_eq!(merged.layers.len(), 2);
    }

    #[test]
    fn test_merge_overlapping_layers() {
        let mut s1 = ComposableSandbox::new();
        s1.add_layer(make_layer(
            "fs",
            LayerType::Filesystem,
            vec![("file_read", RuleVerdict::Allow)],
        ));

        let mut s2 = ComposableSandbox::new();
        s2.add_layer(make_layer(
            "fs",
            LayerType::Filesystem,
            vec![("file_write", RuleVerdict::Deny)],
        ));

        let merged = s1.merge(&s2);
        assert_eq!(merged.layers.len(), 1);
        assert_eq!(merged.layers[0].rules.len(), 2);
    }

    #[test]
    fn test_merge_empty() {
        let s1 = ComposableSandbox::new();
        let s2 = ComposableSandbox::new();
        let merged = s1.merge(&s2);
        assert!(merged.layers.is_empty());
    }

    // -- Sandbox Metrics --

    #[test]
    fn test_metrics_record_allowed() {
        let mut metrics = SandboxMetrics::new();
        metrics.record_allowed();
        metrics.record_allowed();
        assert_eq!(metrics.allowed_count, 2);
    }

    #[test]
    fn test_metrics_record_denied() {
        let mut metrics = SandboxMetrics::new();
        metrics.record_denied("file_write");
        metrics.record_denied("file_write");
        metrics.record_denied("net_connect");
        assert_eq!(metrics.denied_count, 3);
        assert_eq!(
            metrics.most_denied_actions[0],
            ("file_write".to_string(), 2)
        );
        assert_eq!(
            metrics.most_denied_actions[1],
            ("net_connect".to_string(), 1)
        );
    }

    #[test]
    fn test_metrics_record_audit() {
        let mut metrics = SandboxMetrics::new();
        metrics.record_audit();
        assert_eq!(metrics.audit_count, 1);
    }

    #[test]
    fn test_security_score_no_actions() {
        let metrics = SandboxMetrics::new();
        assert!((metrics.security_score() - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_security_score_all_denied() {
        let mut metrics = SandboxMetrics::new();
        metrics.record_denied("a");
        metrics.record_denied("b");
        assert!((metrics.security_score() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_security_score_mixed() {
        let mut metrics = SandboxMetrics::new();
        metrics.record_allowed();
        metrics.record_denied("a");
        // 1 denied / 2 total = 0.5
        assert!((metrics.security_score() - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_security_score_all_allowed() {
        let mut metrics = SandboxMetrics::new();
        metrics.record_allowed();
        metrics.record_allowed();
        assert!((metrics.security_score() - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_metrics_capability_utilization() {
        let mut metrics = SandboxMetrics::new();
        metrics.capability_utilization = 0.75;
        assert!((metrics.capability_utilization - 0.75).abs() < f64::EPSILON);
    }
}
