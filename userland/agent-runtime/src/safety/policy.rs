//! Safety — Policy management and the core SafetyEngine.

use std::collections::HashMap;

use tracing::{debug, info, warn};

use super::types::{
    ActionType, RateBucket, SafetyAction, SafetyEnforcement, SafetyPolicy, SafetyRuleType,
    SafetyVerdict, SafetyViolation,
};

// ---------------------------------------------------------------------------
// SafetyEngine
// ---------------------------------------------------------------------------

/// Core safety engine: evaluates actions and outputs against all active
/// policies, tracks violations, and computes per-agent safety scores.
pub struct SafetyEngine {
    pub policies: Vec<SafetyPolicy>,
    pub(super) violations: Vec<SafetyViolation>,
    /// key = "agent_id::pattern"
    rate_buckets: HashMap<String, RateBucket>,
}

impl SafetyEngine {
    /// Create a new engine pre-loaded with the given policies.
    pub fn new(policies: Vec<SafetyPolicy>) -> Self {
        info!(policy_count = policies.len(), "SafetyEngine initialised");
        Self {
            policies,
            violations: Vec::new(),
            rate_buckets: HashMap::new(),
        }
    }

    // -- policy CRUD -------------------------------------------------------

    /// Add a policy at runtime.
    pub fn add_policy(&mut self, policy: SafetyPolicy) {
        info!(policy_id = %policy.policy_id, name = %policy.name, "Adding safety policy");
        self.policies.push(policy);
    }

    /// Remove a policy by ID. Returns `true` if it existed.
    pub fn remove_policy(&mut self, policy_id: &str) -> bool {
        let before = self.policies.len();
        self.policies.retain(|p| p.policy_id != policy_id);
        let removed = self.policies.len() < before;
        if removed {
            info!(policy_id = %policy_id, "Removed safety policy");
        }
        removed
    }

    /// Look up a policy by ID.
    pub fn get_policy(&self, policy_id: &str) -> Option<&SafetyPolicy> {
        self.policies.iter().find(|p| p.policy_id == policy_id)
    }

    /// Return all enabled policies.
    pub fn active_policies(&self) -> Vec<&SafetyPolicy> {
        self.policies.iter().filter(|p| p.enabled).collect()
    }

    // -- action checking ---------------------------------------------------

    /// Evaluate an action against all active policies. Policies are checked in
    /// descending priority order; the first non-Allowed verdict wins.
    pub fn check_action(&mut self, agent_id: &str, action: &SafetyAction) -> SafetyVerdict {
        // Clone the policy list so we don't hold an immutable borrow on self
        // while mutating rate_buckets inside evaluate_rule.
        let mut sorted: Vec<SafetyPolicy> = self
            .policies
            .iter()
            .filter(|p| p.enabled)
            .cloned()
            .collect();
        sorted.sort_by(|a, b| b.priority.cmp(&a.priority));

        // Collect warnings separately so they don't shadow blocks.
        let mut warning: Option<SafetyVerdict> = None;

        for policy in &sorted {
            for rule in &policy.rules {
                if let Some(verdict) = evaluate_rule(agent_id, action, rule, &mut self.rate_buckets)
                {
                    match (&policy.enforcement, &verdict) {
                        (SafetyEnforcement::Block, _) => {
                            debug!(
                                agent_id = %agent_id,
                                rule_id = %rule.rule_id,
                                "Action blocked by safety rule"
                            );
                            return verdict;
                        }
                        (SafetyEnforcement::Warn, _) => {
                            if warning.is_none() {
                                warning = Some(SafetyVerdict::Warning {
                                    message: format!(
                                        "Rule {} triggered (warn): {}",
                                        rule.rule_id, rule.description
                                    ),
                                });
                            }
                        }
                        (SafetyEnforcement::AuditOnly, _) => {
                            debug!(
                                agent_id = %agent_id,
                                rule_id = %rule.rule_id,
                                "Action audit-logged by safety rule"
                            );
                        }
                    }
                }
            }
        }

        warning.unwrap_or(SafetyVerdict::Allowed)
    }

    /// Check agent output (text) against content-filter and output-validation
    /// rules in all active policies.
    pub fn check_output(&self, agent_id: &str, output: &str) -> SafetyVerdict {
        let mut sorted: Vec<&SafetyPolicy> = self.active_policies();
        sorted.sort_by(|a, b| b.priority.cmp(&a.priority));

        let mut warning: Option<SafetyVerdict> = None;

        for policy in &sorted {
            for rule in &policy.rules {
                let triggered = match &rule.rule_type {
                    SafetyRuleType::ContentFilter { forbidden_patterns } => {
                        let lower = output.to_lowercase();
                        forbidden_patterns
                            .iter()
                            .any(|p| lower.contains(&p.to_lowercase()))
                    }
                    SafetyRuleType::OutputValidation {
                        max_length,
                        require_utf8,
                    } => {
                        if output.len() > *max_length {
                            true
                        } else if *require_utf8 {
                            // In Rust, &str is always UTF-8, so this only
                            // triggers on length. Kept for API completeness.
                            false
                        } else {
                            false
                        }
                    }
                    _ => false,
                };

                if triggered {
                    let verdict = SafetyVerdict::Blocked {
                        reason: format!("Output violates rule: {}", rule.description),
                        rule_id: rule.rule_id.clone(),
                    };

                    match policy.enforcement {
                        SafetyEnforcement::Block => {
                            debug!(
                                agent_id = %agent_id,
                                rule_id = %rule.rule_id,
                                "Output blocked by safety rule"
                            );
                            return verdict;
                        }
                        SafetyEnforcement::Warn => {
                            if warning.is_none() {
                                warning = Some(SafetyVerdict::Warning {
                                    message: format!(
                                        "Output triggers rule {} (warn): {}",
                                        rule.rule_id, rule.description
                                    ),
                                });
                            }
                        }
                        SafetyEnforcement::AuditOnly => {
                            debug!(
                                agent_id = %agent_id,
                                rule_id = %rule.rule_id,
                                "Output audit-logged by safety rule"
                            );
                        }
                    }
                }
            }
        }

        warning.unwrap_or(SafetyVerdict::Allowed)
    }

    // -- violations --------------------------------------------------------

    /// Record a safety violation.
    pub fn record_violation(&mut self, violation: SafetyViolation) {
        warn!(
            agent_id = %violation.agent_id,
            rule_id = %violation.rule_id,
            severity = %violation.severity,
            "Safety violation recorded"
        );
        self.violations.push(violation);
    }

    /// All violations for a given agent.
    pub fn violations_for_agent(&self, agent_id: &str) -> Vec<SafetyViolation> {
        self.violations
            .iter()
            .filter(|v| v.agent_id == agent_id)
            .cloned()
            .collect()
    }

    /// Safety score for an agent: 1.0 (clean) to 0.0 (dangerous). Each
    /// violation subtracts a severity-weighted penalty. The score is clamped
    /// to [0.0, 1.0].
    pub fn agent_safety_score(&self, agent_id: &str) -> f64 {
        let penalty: f64 = self
            .violations_for_agent(agent_id)
            .iter()
            .map(|v| v.severity.weight() * 0.1)
            .sum();
        (1.0 - penalty).clamp(0.0, 1.0)
    }
}

// ---------------------------------------------------------------------------
// Free-standing rule evaluation (avoids borrow conflicts on SafetyEngine)
// ---------------------------------------------------------------------------

/// Evaluate a single rule against an action. Returns `Some(verdict)` if
/// the rule is triggered, `None` otherwise.
pub(super) fn evaluate_rule(
    agent_id: &str,
    action: &SafetyAction,
    rule: &super::types::SafetyRule,
    rate_buckets: &mut HashMap<String, RateBucket>,
) -> Option<SafetyVerdict> {
    match &rule.rule_type {
        SafetyRuleType::ForbiddenAction { pattern } => {
            let target_lower = action.target.to_lowercase();
            if target_lower.contains(&pattern.to_lowercase()) {
                return Some(SafetyVerdict::Blocked {
                    reason: format!("Forbidden action pattern matched: {}", pattern),
                    rule_id: rule.rule_id.clone(),
                });
            }
        }

        SafetyRuleType::RequireApproval { action_pattern } => {
            let target_lower = action.target.to_lowercase();
            if target_lower.contains(&action_pattern.to_lowercase()) {
                return Some(SafetyVerdict::RequiresApproval {
                    reason: format!("Action requires human approval: {}", action_pattern),
                    rule_id: rule.rule_id.clone(),
                });
            }
        }

        SafetyRuleType::RateLimit {
            action_pattern,
            max_per_minute,
        } => {
            let target_lower = action.target.to_lowercase();
            if target_lower.contains(&action_pattern.to_lowercase()) {
                let bucket_key = format!("{}::{}", agent_id, action_pattern);
                let bucket = rate_buckets
                    .entry(bucket_key)
                    .or_insert_with(RateBucket::new);
                let count = bucket.record_and_count();
                if count > *max_per_minute as usize {
                    return Some(SafetyVerdict::RateLimited {
                        retry_after_secs: 60,
                    });
                }
            }
        }

        SafetyRuleType::ScopeRestriction {
            allowed_paths,
            denied_paths,
        } => {
            if action.action_type == ActionType::FileAccess {
                // Check denied first
                for denied in denied_paths {
                    if action.target.starts_with(denied) {
                        return Some(SafetyVerdict::Blocked {
                            reason: format!("Path denied by scope restriction: {}", denied),
                            rule_id: rule.rule_id.clone(),
                        });
                    }
                }
                // If allowed_paths is non-empty, the target must match one
                if !allowed_paths.is_empty()
                    && !allowed_paths.iter().any(|a| action.target.starts_with(a))
                {
                    return Some(SafetyVerdict::Blocked {
                        reason: "Path not in allowed scope".to_string(),
                        rule_id: rule.rule_id.clone(),
                    });
                }
            }
        }

        SafetyRuleType::EscalationRequired {
            from_level,
            to_level,
        } => {
            if action.action_type == ActionType::PrivilegeEscalation {
                let from = action
                    .parameters
                    .get("from_level")
                    .cloned()
                    .unwrap_or_default();
                let to = action
                    .parameters
                    .get("to_level")
                    .cloned()
                    .unwrap_or_default();
                if from == *from_level && to == *to_level {
                    return Some(SafetyVerdict::RequiresApproval {
                        reason: format!(
                            "Privilege escalation from {} to {} requires approval",
                            from_level, to_level
                        ),
                        rule_id: rule.rule_id.clone(),
                    });
                }
            }
        }

        SafetyRuleType::ResourceLimit {
            resource,
            max_value,
        } => {
            if let Some(val_str) = action.parameters.get(resource) {
                if let Ok(val) = val_str.parse::<u64>() {
                    if val > *max_value {
                        return Some(SafetyVerdict::Blocked {
                            reason: format!(
                                "Resource {} exceeds limit: {} > {}",
                                resource, val, max_value
                            ),
                            rule_id: rule.rule_id.clone(),
                        });
                    }
                }
            }
        }

        SafetyRuleType::ContentFilter { forbidden_patterns } => {
            // Content filter applies to action target as well
            let target_lower = action.target.to_lowercase();
            for pat in forbidden_patterns {
                if target_lower.contains(&pat.to_lowercase()) {
                    return Some(SafetyVerdict::Blocked {
                        reason: format!("Content filter matched: {}", pat),
                        rule_id: rule.rule_id.clone(),
                    });
                }
            }
        }

        SafetyRuleType::OutputValidation { .. } => {
            // Output validation is checked via check_output(), not here.
        }
    }

    None
}

// ---------------------------------------------------------------------------
// Default policies
// ---------------------------------------------------------------------------

/// Build a sensible set of default safety policies for AGNOS.
pub fn default_policies() -> Vec<SafetyPolicy> {
    use super::types::{SafetyEnforcement, SafetyRule, SafetyRuleType, SafetySeverity};

    vec![
        SafetyPolicy {
            policy_id: "default-forbidden".into(),
            name: "Default Forbidden Actions".into(),
            rules: vec![
                SafetyRule {
                    rule_id: "forbid-rm-rf".into(),
                    description: "Block recursive root deletion".into(),
                    rule_type: SafetyRuleType::ForbiddenAction {
                        pattern: "rm -rf /".into(),
                    },
                    severity: SafetySeverity::Critical,
                },
                SafetyRule {
                    rule_id: "forbid-mkfs".into(),
                    description: "Block filesystem formatting".into(),
                    rule_type: SafetyRuleType::ForbiddenAction {
                        pattern: "mkfs".into(),
                    },
                    severity: SafetySeverity::Critical,
                },
                SafetyRule {
                    rule_id: "forbid-dd-zero".into(),
                    description: "Block disk zeroing".into(),
                    rule_type: SafetyRuleType::ForbiddenAction {
                        pattern: "dd if=/dev/zero".into(),
                    },
                    severity: SafetySeverity::Critical,
                },
            ],
            enforcement: SafetyEnforcement::Block,
            priority: 10,
            enabled: true,
        },
        SafetyPolicy {
            policy_id: "default-escalation".into(),
            name: "Default Privilege Escalation".into(),
            rules: vec![SafetyRule {
                rule_id: "escalation-user-root".into(),
                description: "Require approval for user-to-root escalation".into(),
                rule_type: SafetyRuleType::EscalationRequired {
                    from_level: "user".into(),
                    to_level: "root".into(),
                },
                severity: SafetySeverity::High,
            }],
            enforcement: SafetyEnforcement::Block,
            priority: 9,
            enabled: true,
        },
        SafetyPolicy {
            policy_id: "default-rate-limit".into(),
            name: "Default Rate Limits".into(),
            rules: vec![SafetyRule {
                rule_id: "rate-system-cmd".into(),
                description: "Limit system commands to 60 per minute".into(),
                rule_type: SafetyRuleType::RateLimit {
                    action_pattern: "system".into(),
                    max_per_minute: 60,
                },
                severity: SafetySeverity::Medium,
            }],
            enforcement: SafetyEnforcement::Block,
            priority: 7,
            enabled: true,
        },
        SafetyPolicy {
            policy_id: "default-content-filter".into(),
            name: "Default Content Filter".into(),
            rules: vec![SafetyRule {
                rule_id: "content-harmful".into(),
                description: "Block common harmful output patterns".into(),
                rule_type: SafetyRuleType::ContentFilter {
                    forbidden_patterns: vec![
                        "DROP TABLE".into(),
                        "DELETE FROM".into(),
                        "FORMAT C:".into(),
                        ":(){ :|:& };:".into(),
                    ],
                },
                severity: SafetySeverity::High,
            }],
            enforcement: SafetyEnforcement::Block,
            priority: 8,
            enabled: true,
        },
        SafetyPolicy {
            policy_id: "default-scope".into(),
            name: "Default Scope Restrictions".into(),
            rules: vec![SafetyRule {
                rule_id: "scope-sensitive-files".into(),
                description: "Deny write access to sensitive system files".into(),
                rule_type: SafetyRuleType::ScopeRestriction {
                    allowed_paths: vec![],
                    denied_paths: vec!["/etc/shadow".into(), "/etc/passwd".into()],
                },
                severity: SafetySeverity::Critical,
            }],
            enforcement: SafetyEnforcement::Block,
            priority: 10,
            enabled: true,
        },
    ]
}
