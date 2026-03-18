//! Safety — Types, enums, and data structures.

use std::collections::HashMap;
use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// SafetySeverity
// ---------------------------------------------------------------------------

/// Severity classification for safety rules and violations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SafetySeverity {
    Critical,
    High,
    Medium,
    Low,
}

impl SafetySeverity {
    /// Numeric weight for scoring (higher = more severe).
    pub(super) fn weight(self) -> f64 {
        match self {
            Self::Critical => 1.0,
            Self::High => 0.7,
            Self::Medium => 0.4,
            Self::Low => 0.1,
        }
    }
}

impl fmt::Display for SafetySeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Critical => write!(f, "CRITICAL"),
            Self::High => write!(f, "HIGH"),
            Self::Medium => write!(f, "MEDIUM"),
            Self::Low => write!(f, "LOW"),
        }
    }
}

// ---------------------------------------------------------------------------
// SafetyEnforcement
// ---------------------------------------------------------------------------

/// How a safety policy is enforced when a rule is triggered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SafetyEnforcement {
    /// Hard-block the action.
    Block,
    /// Allow but emit a warning.
    Warn,
    /// Allow silently but record in audit log.
    AuditOnly,
}

// ---------------------------------------------------------------------------
// SafetyRuleType
// ---------------------------------------------------------------------------

/// The kind of constraint a safety rule expresses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SafetyRuleType {
    /// Cap a resource such as CPU, memory, disk, or network.
    ResourceLimit { resource: String, max_value: u64 },
    /// Block actions whose description matches `pattern`.
    ForbiddenAction { pattern: String },
    /// Require explicit human approval before execution.
    RequireApproval { action_pattern: String },
    /// Throttle repeated actions to at most `max_per_minute`.
    RateLimit {
        action_pattern: String,
        max_per_minute: u32,
    },
    /// Block output containing forbidden patterns.
    ContentFilter { forbidden_patterns: Vec<String> },
    /// Restrict filesystem access to allowed paths / deny listed paths.
    ScopeRestriction {
        allowed_paths: Vec<String>,
        denied_paths: Vec<String>,
    },
    /// Privilege escalation from one level to another needs approval.
    EscalationRequired {
        from_level: String,
        to_level: String,
    },
    /// Validate agent output (length, encoding).
    OutputValidation {
        max_length: usize,
        require_utf8: bool,
    },
}

// ---------------------------------------------------------------------------
// SafetyRule
// ---------------------------------------------------------------------------

/// A single rule within a safety policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyRule {
    pub rule_id: String,
    pub description: String,
    pub rule_type: SafetyRuleType,
    pub severity: SafetySeverity,
}

// ---------------------------------------------------------------------------
// SafetyPolicy
// ---------------------------------------------------------------------------

/// A named collection of safety rules with an enforcement mode and priority.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyPolicy {
    pub policy_id: String,
    pub name: String,
    pub rules: Vec<SafetyRule>,
    pub enforcement: SafetyEnforcement,
    /// 1 (lowest) to 10 (highest). Higher-priority policies are evaluated first.
    pub priority: u8,
    pub enabled: bool,
}

// ---------------------------------------------------------------------------
// ActionType / SafetyAction
// ---------------------------------------------------------------------------

/// Category of action an agent is attempting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ActionType {
    FileAccess,
    ProcessSpawn,
    NetworkRequest,
    SystemCommand,
    DataOutput,
    PrivilegeEscalation,
}

impl fmt::Display for ActionType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FileAccess => write!(f, "FileAccess"),
            Self::ProcessSpawn => write!(f, "ProcessSpawn"),
            Self::NetworkRequest => write!(f, "NetworkRequest"),
            Self::SystemCommand => write!(f, "SystemCommand"),
            Self::DataOutput => write!(f, "DataOutput"),
            Self::PrivilegeEscalation => write!(f, "PrivilegeEscalation"),
        }
    }
}

/// An action an agent wants to perform, presented to the safety engine for
/// pre-flight checking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyAction {
    pub action_type: ActionType,
    pub target: String,
    pub parameters: HashMap<String, String>,
}

// ---------------------------------------------------------------------------
// SafetyVerdict
// ---------------------------------------------------------------------------

/// Result of a safety check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SafetyVerdict {
    Allowed,
    Blocked { reason: String, rule_id: String },
    RequiresApproval { reason: String, rule_id: String },
    RateLimited { retry_after_secs: u32 },
    Warning { message: String },
}

// ---------------------------------------------------------------------------
// SafetyViolation
// ---------------------------------------------------------------------------

/// Record of a safety rule being triggered.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyViolation {
    pub violation_id: String,
    pub agent_id: String,
    pub timestamp: DateTime<Utc>,
    pub rule_id: String,
    pub action_attempted: String,
    pub verdict: SafetyVerdict,
    pub severity: SafetySeverity,
}

// ---------------------------------------------------------------------------
// Rate-limit tracking (not serialized — ephemeral runtime state)
// ---------------------------------------------------------------------------

/// Per-agent, per-pattern rate-limit bucket.
#[derive(Debug, Clone)]
pub(super) struct RateBucket {
    pub(super) timestamps: Vec<std::time::Instant>,
}

impl RateBucket {
    pub(super) fn new() -> Self {
        Self {
            timestamps: Vec::new(),
        }
    }

    /// Record one hit and return the count within the last 60 seconds.
    pub(super) fn record_and_count(&mut self) -> usize {
        let now = std::time::Instant::now();
        let cutoff = now - std::time::Duration::from_secs(60);
        self.timestamps.retain(|t| *t >= cutoff);
        self.timestamps.push(now);
        self.timestamps.len()
    }
}
