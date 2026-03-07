//! Agent Explainability Framework for AGNOS
//!
//! Makes agent decisions transparent and interpretable. Every decision an agent
//! makes — task execution, tool selection, security response — is recorded with
//! its reasoning, alternatives considered, and contributing factors. Users can
//! then query human-readable explanations for trust, debugging, and compliance.

use std::collections::HashMap;
use std::fmt;

use anyhow::{bail, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// FactorType
// ---------------------------------------------------------------------------

/// Category of a factor that influenced a decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FactorType {
    ResourceAvailability,
    SecurityPolicy,
    UserPreference,
    HistoricalSuccess,
    Priority,
    Deadline,
    CostEfficiency,
}

impl fmt::Display for FactorType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ResourceAvailability => write!(f, "ResourceAvailability"),
            Self::SecurityPolicy => write!(f, "SecurityPolicy"),
            Self::UserPreference => write!(f, "UserPreference"),
            Self::HistoricalSuccess => write!(f, "HistoricalSuccess"),
            Self::Priority => write!(f, "Priority"),
            Self::Deadline => write!(f, "Deadline"),
            Self::CostEfficiency => write!(f, "CostEfficiency"),
        }
    }
}

// ---------------------------------------------------------------------------
// ConfidenceLabel
// ---------------------------------------------------------------------------

/// Human-friendly confidence classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConfidenceLabel {
    Low,
    Medium,
    High,
}

impl ConfidenceLabel {
    /// Derive label from a confidence score in `[0.0, 1.0]`.
    pub fn from_score(score: f64) -> Self {
        if score < 0.3 {
            Self::Low
        } else if score <= 0.7 {
            Self::Medium
        } else {
            Self::High
        }
    }
}

impl fmt::Display for ConfidenceLabel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Low => write!(f, "Low"),
            Self::Medium => write!(f, "Medium"),
            Self::High => write!(f, "High"),
        }
    }
}

// ---------------------------------------------------------------------------
// DecisionFactor
// ---------------------------------------------------------------------------

/// A single factor that contributed to an agent's decision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionFactor {
    /// Human-readable name for this factor.
    pub name: String,
    /// Relative weight of this factor (0.0–1.0).
    pub weight: f64,
    /// Observed value of this factor.
    pub value: f64,
    /// Description of what this factor represents.
    pub description: String,
    /// Category of the factor.
    pub factor_type: FactorType,
}

// ---------------------------------------------------------------------------
// Alternative
// ---------------------------------------------------------------------------

/// An alternative action that was considered but not chosen.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alternative {
    /// The action that was considered.
    pub action: String,
    /// How well this alternative scored.
    pub score: f64,
    /// Why this alternative was rejected.
    pub rejection_reason: String,
}

// ---------------------------------------------------------------------------
// DecisionOutcome
// ---------------------------------------------------------------------------

/// Recorded outcome of a decision after execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionOutcome {
    /// Whether the action succeeded.
    pub success: bool,
    /// Summary of the result.
    pub result_summary: String,
    /// How long the action took, in milliseconds.
    pub duration_ms: u64,
    /// Side effects produced by the action.
    pub side_effects: Vec<String>,
}

// ---------------------------------------------------------------------------
// DecisionRecord
// ---------------------------------------------------------------------------

/// Complete record of a single agent decision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionRecord {
    /// Unique identifier for this decision.
    pub decision_id: String,
    /// The agent that made the decision.
    pub agent_id: String,
    /// When the decision was made.
    pub timestamp: DateTime<Utc>,
    /// The action that was taken.
    pub action: String,
    /// The agent's reasoning for this action.
    pub reasoning: String,
    /// Confidence in the decision (0.0–1.0).
    pub confidence: f64,
    /// Summary of the inputs the agent had.
    pub input_summary: String,
    /// Other actions that were evaluated.
    pub alternatives_considered: Vec<Alternative>,
    /// Factors that influenced the decision.
    pub factors: Vec<DecisionFactor>,
    /// Outcome, filled in after execution.
    pub outcome: Option<DecisionOutcome>,
}

// ---------------------------------------------------------------------------
// DecisionExplanation
// ---------------------------------------------------------------------------

/// Factor contribution in a human-readable explanation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactorContribution {
    /// Factor name.
    pub name: String,
    /// Percentage contribution (0.0–100.0).
    pub contribution_pct: f64,
}

/// Human-readable explanation of a decision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionExplanation {
    /// One-sentence summary: "Agent X chose to Y because Z".
    pub summary: String,
    /// Breakdown of each factor's contribution.
    pub factor_breakdown: Vec<FactorContribution>,
    /// Summary of alternatives considered.
    pub alternatives_summary: String,
    /// Confidence classification.
    pub confidence_label: ConfidenceLabel,
    /// If confidence is Low, a review recommendation.
    pub recommendation: Option<String>,
}

// ---------------------------------------------------------------------------
// DecisionFilter
// ---------------------------------------------------------------------------

/// Filter criteria for searching decisions.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DecisionFilter {
    pub agent_id: Option<String>,
    pub min_confidence: Option<f64>,
    pub max_confidence: Option<f64>,
    pub action_contains: Option<String>,
    pub from_time: Option<DateTime<Utc>>,
    pub until_time: Option<DateTime<Utc>>,
    pub has_outcome: Option<bool>,
}

// ---------------------------------------------------------------------------
// AgentDecisionStats
// ---------------------------------------------------------------------------

/// Aggregate statistics for an agent's decisions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDecisionStats {
    pub total_decisions: usize,
    pub average_confidence: f64,
    pub success_rate: f64,
    pub most_common_action: Option<String>,
    pub factor_frequency: HashMap<String, usize>,
    pub decisions_needing_review: usize,
}

// ---------------------------------------------------------------------------
// DecisionNode — simple decision tree
// ---------------------------------------------------------------------------

/// Node in a decision tree for visualization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionNode {
    /// The condition evaluated at this node.
    pub condition: String,
    /// Branch taken when condition is true.
    pub true_branch: Option<Box<DecisionNode>>,
    /// Branch taken when condition is false.
    pub false_branch: Option<Box<DecisionNode>>,
    /// Action if this is a leaf node.
    pub leaf_action: Option<String>,
}

/// Build a simple decision tree from factors, splitting on highest weight first.
///
/// Each factor becomes a node that checks whether its value meets a threshold
/// (weight * 0.5). The leaf of the "all true" path is the chosen action; the
/// "false" leaf at every level is "reject".
pub fn build_tree(factors: &[DecisionFactor], action: &str) -> DecisionNode {
    if factors.is_empty() {
        return DecisionNode {
            condition: String::new(),
            true_branch: None,
            false_branch: None,
            leaf_action: Some(action.to_string()),
        };
    }

    // Sort by weight descending.
    let mut sorted: Vec<&DecisionFactor> = factors.iter().collect();
    sorted.sort_by(|a, b| {
        b.weight
            .partial_cmp(&a.weight)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    build_tree_recursive(&sorted, action)
}

fn build_tree_recursive(factors: &[&DecisionFactor], action: &str) -> DecisionNode {
    if factors.is_empty() {
        return DecisionNode {
            condition: String::new(),
            true_branch: None,
            false_branch: None,
            leaf_action: Some(action.to_string()),
        };
    }

    let factor = factors[0];
    let threshold = factor.weight * 0.5;
    let condition = format!("{} >= {:.2}?", factor.name, threshold);

    let true_branch = if factors.len() > 1 {
        build_tree_recursive(&factors[1..], action)
    } else {
        DecisionNode {
            condition: String::new(),
            true_branch: None,
            false_branch: None,
            leaf_action: Some(action.to_string()),
        }
    };

    let false_branch = DecisionNode {
        condition: String::new(),
        true_branch: None,
        false_branch: None,
        leaf_action: Some("reject".to_string()),
    };

    DecisionNode {
        condition,
        true_branch: Some(Box::new(true_branch)),
        false_branch: Some(Box::new(false_branch)),
        leaf_action: None,
    }
}

/// Render a decision tree as an indented text string.
pub fn render_tree(node: &DecisionNode) -> String {
    let mut buf = String::new();
    render_tree_inner(node, 0, &mut buf);
    buf
}

fn render_tree_inner(node: &DecisionNode, depth: usize, buf: &mut String) {
    let indent = "  ".repeat(depth);
    if let Some(ref action) = node.leaf_action {
        buf.push_str(&format!("{indent}-> {action}\n"));
        return;
    }
    buf.push_str(&format!(
        "{indent}[{condition}]\n",
        condition = node.condition
    ));
    if let Some(ref yes) = node.true_branch {
        buf.push_str(&format!("{indent}  YES:\n"));
        render_tree_inner(yes, depth + 2, buf);
    }
    if let Some(ref no) = node.false_branch {
        buf.push_str(&format!("{indent}  NO:\n"));
        render_tree_inner(no, depth + 2, buf);
    }
}

// ---------------------------------------------------------------------------
// AuditTrail
// ---------------------------------------------------------------------------

/// Links decision records to audit events for compliance traceability.
#[derive(Debug, Clone, Default)]
pub struct AuditTrail {
    /// decision_id -> list of audit event IDs.
    links: HashMap<String, Vec<String>>,
}

impl AuditTrail {
    pub fn new() -> Self {
        Self::default()
    }

    /// Associate an audit event with a decision.
    pub fn link_audit_event(&mut self, decision_id: &str, audit_event_id: &str) {
        debug!(
            decision_id,
            audit_event_id, "linking audit event to decision"
        );
        self.links
            .entry(decision_id.to_string())
            .or_default()
            .push(audit_event_id.to_string());
    }

    /// Return all audit event IDs linked to a decision.
    pub fn trail_for_decision(&self, decision_id: &str) -> Vec<String> {
        self.links.get(decision_id).cloned().unwrap_or_default()
    }
}

// ---------------------------------------------------------------------------
// ExplainabilityEngine
// ---------------------------------------------------------------------------

/// Core engine for recording, querying, and explaining agent decisions.
pub struct ExplainabilityEngine {
    decisions: Vec<DecisionRecord>,
    audit_trail: AuditTrail,
}

impl ExplainabilityEngine {
    /// Create a new empty engine.
    pub fn new() -> Self {
        info!("explainability engine initialized");
        Self {
            decisions: Vec::new(),
            audit_trail: AuditTrail::new(),
        }
    }

    /// Record a decision. Returns the decision_id.
    pub fn record_decision(&mut self, record: DecisionRecord) -> Result<String> {
        if record.agent_id.is_empty() {
            bail!("agent_id must not be empty");
        }
        if record.confidence < 0.0 || record.confidence > 1.0 || record.confidence.is_nan() {
            bail!(
                "confidence must be in [0.0, 1.0], got {}",
                record.confidence
            );
        }
        let id = record.decision_id.clone();
        info!(decision_id = %id, agent_id = %record.agent_id, action = %record.action, "decision recorded");
        if record.confidence < 0.3 {
            warn!(
                decision_id = %id,
                confidence = record.confidence,
                "low-confidence decision recorded — review recommended"
            );
        }
        self.decisions.push(record);
        Ok(id)
    }

    /// Retrieve a decision by ID.
    pub fn get_decision(&self, decision_id: &str) -> Option<&DecisionRecord> {
        self.decisions.iter().find(|d| d.decision_id == decision_id)
    }

    /// Generate a human-readable explanation of a decision.
    pub fn explain_decision(&self, decision_id: &str) -> Result<DecisionExplanation> {
        let record = self
            .get_decision(decision_id)
            .ok_or_else(|| anyhow::anyhow!("decision not found: {}", decision_id))?;

        let confidence_label = ConfidenceLabel::from_score(record.confidence);

        // Build summary.
        let primary_reason = if record.factors.is_empty() {
            record.reasoning.clone()
        } else {
            // Use the highest-weight factor as the primary reason.
            let top = record
                .factors
                .iter()
                .max_by(|a, b| {
                    a.weight
                        .partial_cmp(&b.weight)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .unwrap();
            format!("{} ({})", record.reasoning, top.name)
        };
        let summary = format!(
            "Agent {} chose to {} because {}",
            record.agent_id, record.action, primary_reason
        );

        // Factor breakdown.
        let total_weighted: f64 = record.factors.iter().map(|f| f.weight * f.value).sum();
        let factor_breakdown = if total_weighted == 0.0 {
            record
                .factors
                .iter()
                .map(|f| FactorContribution {
                    name: f.name.clone(),
                    contribution_pct: 0.0,
                })
                .collect()
        } else {
            record
                .factors
                .iter()
                .map(|f| {
                    let pct = (f.weight * f.value / total_weighted) * 100.0;
                    FactorContribution {
                        name: f.name.clone(),
                        contribution_pct: (pct * 100.0).round() / 100.0,
                    }
                })
                .collect()
        };

        // Alternatives summary.
        let alternatives_summary = if record.alternatives_considered.is_empty() {
            "No alternatives were considered.".to_string()
        } else {
            let parts: Vec<String> = record
                .alternatives_considered
                .iter()
                .map(|a| format!("{} (rejected: {})", a.action, a.rejection_reason))
                .collect();
            format!("Considered {}", parts.join(", "))
        };

        // Recommendation for low confidence.
        let recommendation = if confidence_label == ConfidenceLabel::Low {
            Some(format!(
                "Decision confidence is low ({:.2}). Manual review is recommended.",
                record.confidence
            ))
        } else {
            None
        };

        debug!(decision_id, %confidence_label, "explanation generated");

        Ok(DecisionExplanation {
            summary,
            factor_breakdown,
            alternatives_summary,
            confidence_label,
            recommendation,
        })
    }

    /// Return all decisions for a given agent, sorted newest first.
    pub fn decisions_for_agent(&self, agent_id: &str) -> Vec<&DecisionRecord> {
        let mut results: Vec<&DecisionRecord> = self
            .decisions
            .iter()
            .filter(|d| d.agent_id == agent_id)
            .collect();
        results.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        results
    }

    /// Search decisions using a filter.
    pub fn search_decisions(&self, filter: &DecisionFilter) -> Vec<&DecisionRecord> {
        self.decisions
            .iter()
            .filter(|d| {
                if let Some(ref aid) = filter.agent_id {
                    if d.agent_id != *aid {
                        return false;
                    }
                }
                if let Some(min) = filter.min_confidence {
                    if d.confidence < min {
                        return false;
                    }
                }
                if let Some(max) = filter.max_confidence {
                    if d.confidence > max {
                        return false;
                    }
                }
                if let Some(ref needle) = filter.action_contains {
                    if !d.action.contains(needle.as_str()) {
                        return false;
                    }
                }
                if let Some(from) = filter.from_time {
                    if d.timestamp < from {
                        return false;
                    }
                }
                if let Some(until) = filter.until_time {
                    if d.timestamp > until {
                        return false;
                    }
                }
                if let Some(has) = filter.has_outcome {
                    if d.outcome.is_some() != has {
                        return false;
                    }
                }
                true
            })
            .collect()
    }

    /// Record the outcome of a previously made decision.
    pub fn record_outcome(&mut self, decision_id: &str, outcome: DecisionOutcome) -> Result<()> {
        let record = self
            .decisions
            .iter_mut()
            .find(|d| d.decision_id == decision_id)
            .ok_or_else(|| anyhow::anyhow!("decision not found: {}", decision_id))?;
        info!(decision_id, success = outcome.success, "outcome recorded");
        record.outcome = Some(outcome);
        Ok(())
    }

    /// Compute aggregate statistics for an agent.
    pub fn agent_decision_stats(&self, agent_id: &str) -> AgentDecisionStats {
        let agent_decisions: Vec<&DecisionRecord> = self
            .decisions
            .iter()
            .filter(|d| d.agent_id == agent_id)
            .collect();

        let total = agent_decisions.len();
        if total == 0 {
            return AgentDecisionStats {
                total_decisions: 0,
                average_confidence: 0.0,
                success_rate: 0.0,
                most_common_action: None,
                factor_frequency: HashMap::new(),
                decisions_needing_review: 0,
            };
        }

        let avg_conf: f64 =
            agent_decisions.iter().map(|d| d.confidence).sum::<f64>() / total as f64;

        // Success rate from outcomes.
        let with_outcome: Vec<&&DecisionRecord> = agent_decisions
            .iter()
            .filter(|d| d.outcome.is_some())
            .collect();
        let success_rate = if with_outcome.is_empty() {
            0.0
        } else {
            let successes = with_outcome
                .iter()
                .filter(|d| d.outcome.as_ref().unwrap().success)
                .count();
            successes as f64 / with_outcome.len() as f64
        };

        // Most common action.
        let mut action_counts: HashMap<&str, usize> = HashMap::new();
        for d in &agent_decisions {
            *action_counts.entry(&d.action).or_insert(0) += 1;
        }
        let most_common_action = action_counts
            .into_iter()
            .max_by_key(|&(_, c)| c)
            .map(|(a, _)| a.to_string());

        // Factor frequency.
        let mut factor_frequency: HashMap<String, usize> = HashMap::new();
        for d in &agent_decisions {
            for f in &d.factors {
                *factor_frequency.entry(f.name.clone()).or_insert(0) += 1;
            }
        }

        // Decisions needing review (confidence < 0.3).
        let decisions_needing_review = agent_decisions
            .iter()
            .filter(|d| d.confidence < 0.3)
            .count();

        AgentDecisionStats {
            total_decisions: total,
            average_confidence: avg_conf,
            success_rate,
            most_common_action,
            factor_frequency,
            decisions_needing_review,
        }
    }

    /// Access the audit trail.
    pub fn audit_trail(&self) -> &AuditTrail {
        &self.audit_trail
    }

    /// Access the audit trail mutably.
    pub fn audit_trail_mut(&mut self) -> &mut AuditTrail {
        &mut self.audit_trail
    }
}

impl Default for ExplainabilityEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Helper — create a new DecisionRecord with a fresh UUID
// ---------------------------------------------------------------------------

/// Convenience constructor for a `DecisionRecord` with a generated UUID and
/// current timestamp.
pub fn new_decision_record(
    agent_id: &str,
    action: &str,
    reasoning: &str,
    confidence: f64,
    input_summary: &str,
) -> DecisionRecord {
    DecisionRecord {
        decision_id: Uuid::new_v4().to_string(),
        agent_id: agent_id.to_string(),
        timestamp: Utc::now(),
        action: action.to_string(),
        reasoning: reasoning.to_string(),
        confidence,
        input_summary: input_summary.to_string(),
        alternatives_considered: Vec::new(),
        factors: Vec::new(),
        outcome: None,
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    // -- helpers ------------------------------------------------------------

    fn make_factor(name: &str, weight: f64, value: f64, ft: FactorType) -> DecisionFactor {
        DecisionFactor {
            name: name.to_string(),
            weight,
            value,
            description: format!("{name} factor"),
            factor_type: ft,
        }
    }

    fn make_alternative(action: &str, score: f64, reason: &str) -> Alternative {
        Alternative {
            action: action.to_string(),
            score,
            rejection_reason: reason.to_string(),
        }
    }

    fn sample_record(agent: &str, action: &str, confidence: f64) -> DecisionRecord {
        DecisionRecord {
            decision_id: Uuid::new_v4().to_string(),
            agent_id: agent.to_string(),
            timestamp: Utc::now(),
            action: action.to_string(),
            reasoning: "test reasoning".to_string(),
            confidence,
            input_summary: "test input".to_string(),
            alternatives_considered: Vec::new(),
            factors: Vec::new(),
            outcome: None,
        }
    }

    fn sample_record_with_time(
        agent: &str,
        action: &str,
        confidence: f64,
        ts: DateTime<Utc>,
    ) -> DecisionRecord {
        let mut r = sample_record(agent, action, confidence);
        r.timestamp = ts;
        r
    }

    fn engine() -> ExplainabilityEngine {
        ExplainabilityEngine::new()
    }

    // -- record_decision ---------------------------------------------------

    #[test]
    fn record_decision_returns_id() {
        let mut eng = engine();
        let rec = sample_record("agent-1", "deploy", 0.9);
        let id = rec.decision_id.clone();
        let result = eng.record_decision(rec).unwrap();
        assert_eq!(result, id);
    }

    #[test]
    fn record_decision_stores_record() {
        let mut eng = engine();
        let rec = sample_record("agent-1", "deploy", 0.8);
        let id = eng.record_decision(rec).unwrap();
        assert!(eng.get_decision(&id).is_some());
    }

    #[test]
    fn record_decision_rejects_empty_agent_id() {
        let mut eng = engine();
        let rec = sample_record("", "deploy", 0.5);
        assert!(eng.record_decision(rec).is_err());
    }

    #[test]
    fn record_decision_rejects_confidence_below_zero() {
        let mut eng = engine();
        let rec = sample_record("agent-1", "deploy", -0.1);
        assert!(eng.record_decision(rec).is_err());
    }

    #[test]
    fn record_decision_rejects_confidence_above_one() {
        let mut eng = engine();
        let rec = sample_record("agent-1", "deploy", 1.1);
        assert!(eng.record_decision(rec).is_err());
    }

    #[test]
    fn record_decision_accepts_boundary_confidence() {
        let mut eng = engine();
        assert!(eng.record_decision(sample_record("a", "x", 0.0)).is_ok());
        assert!(eng.record_decision(sample_record("a", "x", 1.0)).is_ok());
    }

    // -- get_decision ------------------------------------------------------

    #[test]
    fn get_decision_unknown_returns_none() {
        let eng = engine();
        assert!(eng.get_decision("nonexistent").is_none());
    }

    // -- explain_decision --------------------------------------------------

    #[test]
    fn explain_decision_basic_summary() {
        let mut eng = engine();
        let rec = sample_record("agent-1", "restart-service", 0.85);
        let id = eng.record_decision(rec).unwrap();
        let expl = eng.explain_decision(&id).unwrap();
        assert!(expl.summary.contains("agent-1"));
        assert!(expl.summary.contains("restart-service"));
    }

    #[test]
    fn explain_decision_with_factors() {
        let mut eng = engine();
        let mut rec = sample_record("agent-2", "scale-up", 0.7);
        rec.factors.push(make_factor(
            "cpu-load",
            0.8,
            0.9,
            FactorType::ResourceAvailability,
        ));
        rec.factors.push(make_factor(
            "user-pref",
            0.5,
            0.6,
            FactorType::UserPreference,
        ));
        let id = eng.record_decision(rec).unwrap();
        let expl = eng.explain_decision(&id).unwrap();
        assert_eq!(expl.factor_breakdown.len(), 2);
        // Total contributions should sum to ~100%.
        let sum: f64 = expl
            .factor_breakdown
            .iter()
            .map(|fc| fc.contribution_pct)
            .sum();
        assert!((sum - 100.0).abs() < 0.1);
    }

    #[test]
    fn explain_decision_no_factors_uses_reasoning() {
        let mut eng = engine();
        let mut rec = sample_record("agent-3", "noop", 0.5);
        rec.reasoning = "nothing to do".to_string();
        let id = eng.record_decision(rec).unwrap();
        let expl = eng.explain_decision(&id).unwrap();
        assert!(expl.summary.contains("nothing to do"));
    }

    #[test]
    fn explain_decision_with_alternatives() {
        let mut eng = engine();
        let mut rec = sample_record("agent-4", "pick-A", 0.6);
        rec.alternatives_considered
            .push(make_alternative("pick-B", 0.4, "too slow"));
        rec.alternatives_considered
            .push(make_alternative("pick-C", 0.3, "too costly"));
        let id = eng.record_decision(rec).unwrap();
        let expl = eng.explain_decision(&id).unwrap();
        assert!(expl.alternatives_summary.contains("pick-B"));
        assert!(expl.alternatives_summary.contains("too slow"));
        assert!(expl.alternatives_summary.contains("pick-C"));
    }

    #[test]
    fn explain_decision_no_alternatives_message() {
        let mut eng = engine();
        let rec = sample_record("agent-5", "only-choice", 0.9);
        let id = eng.record_decision(rec).unwrap();
        let expl = eng.explain_decision(&id).unwrap();
        assert!(expl.alternatives_summary.contains("No alternatives"));
    }

    #[test]
    fn explain_decision_not_found() {
        let eng = engine();
        assert!(eng.explain_decision("bogus-id").is_err());
    }

    // -- confidence labels -------------------------------------------------

    #[test]
    fn confidence_label_low() {
        assert_eq!(ConfidenceLabel::from_score(0.0), ConfidenceLabel::Low);
        assert_eq!(ConfidenceLabel::from_score(0.29), ConfidenceLabel::Low);
    }

    #[test]
    fn confidence_label_medium() {
        assert_eq!(ConfidenceLabel::from_score(0.3), ConfidenceLabel::Medium);
        assert_eq!(ConfidenceLabel::from_score(0.5), ConfidenceLabel::Medium);
        assert_eq!(ConfidenceLabel::from_score(0.7), ConfidenceLabel::Medium);
    }

    #[test]
    fn confidence_label_high() {
        assert_eq!(ConfidenceLabel::from_score(0.71), ConfidenceLabel::High);
        assert_eq!(ConfidenceLabel::from_score(1.0), ConfidenceLabel::High);
    }

    #[test]
    fn low_confidence_has_recommendation() {
        let mut eng = engine();
        let rec = sample_record("agent-low", "risky-call", 0.15);
        let id = eng.record_decision(rec).unwrap();
        let expl = eng.explain_decision(&id).unwrap();
        assert_eq!(expl.confidence_label, ConfidenceLabel::Low);
        assert!(expl.recommendation.is_some());
        assert!(expl.recommendation.unwrap().contains("review"));
    }

    #[test]
    fn high_confidence_no_recommendation() {
        let mut eng = engine();
        let rec = sample_record("agent-hi", "safe-call", 0.95);
        let id = eng.record_decision(rec).unwrap();
        let expl = eng.explain_decision(&id).unwrap();
        assert_eq!(expl.confidence_label, ConfidenceLabel::High);
        assert!(expl.recommendation.is_none());
    }

    // -- decisions_for_agent -----------------------------------------------

    #[test]
    fn decisions_for_agent_filtered() {
        let mut eng = engine();
        eng.record_decision(sample_record("a1", "x", 0.5)).unwrap();
        eng.record_decision(sample_record("a2", "y", 0.6)).unwrap();
        eng.record_decision(sample_record("a1", "z", 0.7)).unwrap();
        assert_eq!(eng.decisions_for_agent("a1").len(), 2);
        assert_eq!(eng.decisions_for_agent("a2").len(), 1);
    }

    #[test]
    fn decisions_for_agent_sorted_newest_first() {
        let mut eng = engine();
        let t1 = Utc::now() - Duration::hours(2);
        let t2 = Utc::now() - Duration::hours(1);
        let t3 = Utc::now();
        eng.record_decision(sample_record_with_time("a1", "old", 0.5, t1))
            .unwrap();
        eng.record_decision(sample_record_with_time("a1", "new", 0.6, t3))
            .unwrap();
        eng.record_decision(sample_record_with_time("a1", "mid", 0.7, t2))
            .unwrap();
        let results = eng.decisions_for_agent("a1");
        assert_eq!(results[0].action, "new");
        assert_eq!(results[1].action, "mid");
        assert_eq!(results[2].action, "old");
    }

    #[test]
    fn decisions_for_unknown_agent_empty() {
        let eng = engine();
        assert!(eng.decisions_for_agent("nobody").is_empty());
    }

    // -- search_decisions --------------------------------------------------

    #[test]
    fn search_by_agent_id() {
        let mut eng = engine();
        eng.record_decision(sample_record("a1", "x", 0.5)).unwrap();
        eng.record_decision(sample_record("a2", "y", 0.6)).unwrap();
        let f = DecisionFilter {
            agent_id: Some("a1".to_string()),
            ..Default::default()
        };
        assert_eq!(eng.search_decisions(&f).len(), 1);
    }

    #[test]
    fn search_by_min_confidence() {
        let mut eng = engine();
        eng.record_decision(sample_record("a", "x", 0.3)).unwrap();
        eng.record_decision(sample_record("a", "y", 0.8)).unwrap();
        let f = DecisionFilter {
            min_confidence: Some(0.5),
            ..Default::default()
        };
        assert_eq!(eng.search_decisions(&f).len(), 1);
    }

    #[test]
    fn search_by_max_confidence() {
        let mut eng = engine();
        eng.record_decision(sample_record("a", "x", 0.3)).unwrap();
        eng.record_decision(sample_record("a", "y", 0.8)).unwrap();
        let f = DecisionFilter {
            max_confidence: Some(0.5),
            ..Default::default()
        };
        assert_eq!(eng.search_decisions(&f).len(), 1);
    }

    #[test]
    fn search_by_action_contains() {
        let mut eng = engine();
        eng.record_decision(sample_record("a", "deploy-app", 0.5))
            .unwrap();
        eng.record_decision(sample_record("a", "restart-svc", 0.6))
            .unwrap();
        let f = DecisionFilter {
            action_contains: Some("deploy".to_string()),
            ..Default::default()
        };
        assert_eq!(eng.search_decisions(&f).len(), 1);
    }

    #[test]
    fn search_by_time_range() {
        let mut eng = engine();
        let now = Utc::now();
        eng.record_decision(sample_record_with_time(
            "a",
            "x",
            0.5,
            now - Duration::hours(3),
        ))
        .unwrap();
        eng.record_decision(sample_record_with_time(
            "a",
            "y",
            0.6,
            now - Duration::hours(1),
        ))
        .unwrap();
        let f = DecisionFilter {
            from_time: Some(now - Duration::hours(2)),
            ..Default::default()
        };
        assert_eq!(eng.search_decisions(&f).len(), 1);
    }

    #[test]
    fn search_by_until_time() {
        let mut eng = engine();
        let now = Utc::now();
        eng.record_decision(sample_record_with_time(
            "a",
            "x",
            0.5,
            now - Duration::hours(3),
        ))
        .unwrap();
        eng.record_decision(sample_record_with_time(
            "a",
            "y",
            0.6,
            now - Duration::hours(1),
        ))
        .unwrap();
        let f = DecisionFilter {
            until_time: Some(now - Duration::hours(2)),
            ..Default::default()
        };
        assert_eq!(eng.search_decisions(&f).len(), 1);
    }

    #[test]
    fn search_by_has_outcome_true() {
        let mut eng = engine();
        let mut rec = sample_record("a", "x", 0.5);
        rec.outcome = Some(DecisionOutcome {
            success: true,
            result_summary: "ok".to_string(),
            duration_ms: 100,
            side_effects: Vec::new(),
        });
        eng.record_decision(rec).unwrap();
        eng.record_decision(sample_record("a", "y", 0.6)).unwrap();
        let f = DecisionFilter {
            has_outcome: Some(true),
            ..Default::default()
        };
        assert_eq!(eng.search_decisions(&f).len(), 1);
    }

    #[test]
    fn search_by_has_outcome_false() {
        let mut eng = engine();
        let mut rec = sample_record("a", "x", 0.5);
        rec.outcome = Some(DecisionOutcome {
            success: true,
            result_summary: "ok".to_string(),
            duration_ms: 50,
            side_effects: Vec::new(),
        });
        eng.record_decision(rec).unwrap();
        eng.record_decision(sample_record("a", "y", 0.6)).unwrap();
        let f = DecisionFilter {
            has_outcome: Some(false),
            ..Default::default()
        };
        assert_eq!(eng.search_decisions(&f).len(), 1);
    }

    #[test]
    fn search_combined_filters() {
        let mut eng = engine();
        eng.record_decision(sample_record("a1", "deploy-app", 0.9))
            .unwrap();
        eng.record_decision(sample_record("a1", "deploy-svc", 0.3))
            .unwrap();
        eng.record_decision(sample_record("a2", "deploy-app", 0.9))
            .unwrap();
        let f = DecisionFilter {
            agent_id: Some("a1".to_string()),
            min_confidence: Some(0.5),
            action_contains: Some("deploy".to_string()),
            ..Default::default()
        };
        assert_eq!(eng.search_decisions(&f).len(), 1);
    }

    #[test]
    fn search_empty_filter_returns_all() {
        let mut eng = engine();
        eng.record_decision(sample_record("a", "x", 0.5)).unwrap();
        eng.record_decision(sample_record("b", "y", 0.6)).unwrap();
        let f = DecisionFilter::default();
        assert_eq!(eng.search_decisions(&f).len(), 2);
    }

    // -- record_outcome ----------------------------------------------------

    #[test]
    fn record_outcome_success() {
        let mut eng = engine();
        let rec = sample_record("a", "x", 0.5);
        let id = eng.record_decision(rec).unwrap();
        let outcome = DecisionOutcome {
            success: true,
            result_summary: "completed".to_string(),
            duration_ms: 200,
            side_effects: vec!["log-written".to_string()],
        };
        eng.record_outcome(&id, outcome).unwrap();
        let d = eng.get_decision(&id).unwrap();
        assert!(d.outcome.is_some());
        assert!(d.outcome.as_ref().unwrap().success);
    }

    #[test]
    fn record_outcome_unknown_decision() {
        let mut eng = engine();
        let outcome = DecisionOutcome {
            success: false,
            result_summary: "n/a".to_string(),
            duration_ms: 0,
            side_effects: Vec::new(),
        };
        assert!(eng.record_outcome("bogus", outcome).is_err());
    }

    // -- agent_decision_stats ----------------------------------------------

    #[test]
    fn stats_empty_agent() {
        let eng = engine();
        let stats = eng.agent_decision_stats("nobody");
        assert_eq!(stats.total_decisions, 0);
        assert_eq!(stats.average_confidence, 0.0);
        assert_eq!(stats.success_rate, 0.0);
        assert!(stats.most_common_action.is_none());
        assert_eq!(stats.decisions_needing_review, 0);
    }

    #[test]
    fn stats_total_and_average_confidence() {
        let mut eng = engine();
        eng.record_decision(sample_record("a", "x", 0.4)).unwrap();
        eng.record_decision(sample_record("a", "y", 0.8)).unwrap();
        let stats = eng.agent_decision_stats("a");
        assert_eq!(stats.total_decisions, 2);
        assert!((stats.average_confidence - 0.6).abs() < 0.001);
    }

    #[test]
    fn stats_success_rate() {
        let mut eng = engine();
        let r1 = sample_record("a", "x", 0.5);
        let id1 = eng.record_decision(r1).unwrap();
        let r2 = sample_record("a", "y", 0.6);
        let id2 = eng.record_decision(r2).unwrap();
        eng.record_outcome(
            &id1,
            DecisionOutcome {
                success: true,
                result_summary: "ok".to_string(),
                duration_ms: 10,
                side_effects: Vec::new(),
            },
        )
        .unwrap();
        eng.record_outcome(
            &id2,
            DecisionOutcome {
                success: false,
                result_summary: "fail".to_string(),
                duration_ms: 20,
                side_effects: Vec::new(),
            },
        )
        .unwrap();
        let stats = eng.agent_decision_stats("a");
        assert!((stats.success_rate - 0.5).abs() < 0.001);
    }

    #[test]
    fn stats_most_common_action() {
        let mut eng = engine();
        eng.record_decision(sample_record("a", "deploy", 0.5))
            .unwrap();
        eng.record_decision(sample_record("a", "deploy", 0.6))
            .unwrap();
        eng.record_decision(sample_record("a", "restart", 0.7))
            .unwrap();
        let stats = eng.agent_decision_stats("a");
        assert_eq!(stats.most_common_action, Some("deploy".to_string()));
    }

    #[test]
    fn stats_factor_frequency() {
        let mut eng = engine();
        let mut r1 = sample_record("a", "x", 0.5);
        r1.factors.push(make_factor(
            "cpu",
            0.5,
            0.5,
            FactorType::ResourceAvailability,
        ));
        r1.factors.push(make_factor(
            "mem",
            0.3,
            0.4,
            FactorType::ResourceAvailability,
        ));
        let mut r2 = sample_record("a", "y", 0.6);
        r2.factors.push(make_factor(
            "cpu",
            0.6,
            0.7,
            FactorType::ResourceAvailability,
        ));
        eng.record_decision(r1).unwrap();
        eng.record_decision(r2).unwrap();
        let stats = eng.agent_decision_stats("a");
        assert_eq!(stats.factor_frequency["cpu"], 2);
        assert_eq!(stats.factor_frequency["mem"], 1);
    }

    #[test]
    fn stats_decisions_needing_review() {
        let mut eng = engine();
        eng.record_decision(sample_record("a", "x", 0.1)).unwrap();
        eng.record_decision(sample_record("a", "y", 0.2)).unwrap();
        eng.record_decision(sample_record("a", "z", 0.8)).unwrap();
        let stats = eng.agent_decision_stats("a");
        assert_eq!(stats.decisions_needing_review, 2);
    }

    #[test]
    fn stats_no_outcomes_gives_zero_success_rate() {
        let mut eng = engine();
        eng.record_decision(sample_record("a", "x", 0.5)).unwrap();
        let stats = eng.agent_decision_stats("a");
        assert_eq!(stats.success_rate, 0.0);
    }

    // -- decision tree -----------------------------------------------------

    #[test]
    fn build_tree_empty_factors() {
        let tree = build_tree(&[], "do-thing");
        assert_eq!(tree.leaf_action, Some("do-thing".to_string()));
        assert!(tree.true_branch.is_none());
        assert!(tree.false_branch.is_none());
    }

    #[test]
    fn build_tree_single_factor() {
        let factors = vec![make_factor(
            "cpu",
            0.8,
            0.9,
            FactorType::ResourceAvailability,
        )];
        let tree = build_tree(&factors, "scale-up");
        assert!(tree.condition.contains("cpu"));
        assert!(tree.true_branch.is_some());
        assert!(tree.false_branch.is_some());
        let leaf = tree.true_branch.unwrap();
        assert_eq!(leaf.leaf_action, Some("scale-up".to_string()));
        let reject = tree.false_branch.unwrap();
        assert_eq!(reject.leaf_action, Some("reject".to_string()));
    }

    #[test]
    fn build_tree_multiple_factors_highest_weight_first() {
        let factors = vec![
            make_factor("low-w", 0.2, 0.5, FactorType::Priority),
            make_factor("high-w", 0.9, 0.5, FactorType::SecurityPolicy),
            make_factor("mid-w", 0.5, 0.5, FactorType::Deadline),
        ];
        let tree = build_tree(&factors, "act");
        // Root should split on the highest weight factor.
        assert!(tree.condition.contains("high-w"));
    }

    #[test]
    fn build_tree_depth_matches_factor_count() {
        let factors = vec![
            make_factor("a", 0.9, 0.5, FactorType::Priority),
            make_factor("b", 0.5, 0.5, FactorType::Deadline),
            make_factor("c", 0.2, 0.5, FactorType::CostEfficiency),
        ];
        let tree = build_tree(&factors, "go");
        // Depth: root -> true -> true -> leaf
        let d1 = tree.true_branch.as_ref().unwrap();
        assert!(d1.condition.contains("b"));
        let d2 = d1.true_branch.as_ref().unwrap();
        assert!(d2.condition.contains("c"));
        let d3 = d2.true_branch.as_ref().unwrap();
        assert_eq!(d3.leaf_action, Some("go".to_string()));
    }

    #[test]
    fn render_tree_empty_leaf() {
        let tree = build_tree(&[], "hello");
        let rendered = render_tree(&tree);
        assert!(rendered.contains("hello"));
    }

    #[test]
    fn render_tree_with_branches() {
        let factors = vec![make_factor(
            "cpu",
            0.8,
            0.9,
            FactorType::ResourceAvailability,
        )];
        let tree = build_tree(&factors, "scale");
        let rendered = render_tree(&tree);
        assert!(rendered.contains("cpu"));
        assert!(rendered.contains("YES"));
        assert!(rendered.contains("NO"));
        assert!(rendered.contains("scale"));
        assert!(rendered.contains("reject"));
    }

    #[test]
    fn render_tree_indentation_increases() {
        let factors = vec![
            make_factor("a", 0.9, 0.5, FactorType::Priority),
            make_factor("b", 0.5, 0.5, FactorType::Deadline),
        ];
        let tree = build_tree(&factors, "go");
        let rendered = render_tree(&tree);
        // The deeper nodes should have more leading whitespace.
        let lines: Vec<&str> = rendered.lines().collect();
        assert!(lines.len() > 2);
        // First line should have no leading spaces.
        assert!(lines[0].starts_with('['));
    }

    // -- audit trail -------------------------------------------------------

    #[test]
    fn audit_trail_link_and_retrieve() {
        let mut trail = AuditTrail::new();
        trail.link_audit_event("d1", "audit-001");
        trail.link_audit_event("d1", "audit-002");
        let events = trail.trail_for_decision("d1");
        assert_eq!(events.len(), 2);
        assert!(events.contains(&"audit-001".to_string()));
        assert!(events.contains(&"audit-002".to_string()));
    }

    #[test]
    fn audit_trail_unknown_decision() {
        let trail = AuditTrail::new();
        assert!(trail.trail_for_decision("nope").is_empty());
    }

    #[test]
    fn engine_audit_trail_integration() {
        let mut eng = engine();
        let rec = sample_record("a", "x", 0.5);
        let id = eng.record_decision(rec).unwrap();
        eng.audit_trail_mut().link_audit_event(&id, "evt-100");
        let events = eng.audit_trail().trail_for_decision(&id);
        assert_eq!(events, vec!["evt-100".to_string()]);
    }

    // -- edge cases --------------------------------------------------------

    #[test]
    fn empty_engine_search_returns_empty() {
        let eng = engine();
        let f = DecisionFilter::default();
        assert!(eng.search_decisions(&f).is_empty());
    }

    #[test]
    fn zero_weight_factors_explanation() {
        let mut eng = engine();
        let mut rec = sample_record("a", "x", 0.5);
        rec.factors
            .push(make_factor("f1", 0.0, 0.5, FactorType::Priority));
        rec.factors
            .push(make_factor("f2", 0.0, 0.3, FactorType::Deadline));
        let id = eng.record_decision(rec).unwrap();
        let expl = eng.explain_decision(&id).unwrap();
        // With zero weights, all contributions should be 0.
        for fc in &expl.factor_breakdown {
            assert_eq!(fc.contribution_pct, 0.0);
        }
    }

    #[test]
    fn all_same_confidence_decisions() {
        let mut eng = engine();
        for _ in 0..5 {
            eng.record_decision(sample_record("a", "x", 0.5)).unwrap();
        }
        let stats = eng.agent_decision_stats("a");
        assert!((stats.average_confidence - 0.5).abs() < 0.001);
    }

    #[test]
    fn new_decision_record_helper() {
        let rec = new_decision_record("agent-1", "deploy", "needed", 0.8, "input data");
        assert_eq!(rec.agent_id, "agent-1");
        assert_eq!(rec.action, "deploy");
        assert_eq!(rec.reasoning, "needed");
        assert_eq!(rec.confidence, 0.8);
        assert!(rec.alternatives_considered.is_empty());
        assert!(rec.factors.is_empty());
        assert!(rec.outcome.is_none());
        // UUID should be valid.
        assert!(Uuid::parse_str(&rec.decision_id).is_ok());
    }

    #[test]
    fn factor_type_display() {
        assert_eq!(
            FactorType::ResourceAvailability.to_string(),
            "ResourceAvailability"
        );
        assert_eq!(FactorType::SecurityPolicy.to_string(), "SecurityPolicy");
        assert_eq!(FactorType::UserPreference.to_string(), "UserPreference");
        assert_eq!(
            FactorType::HistoricalSuccess.to_string(),
            "HistoricalSuccess"
        );
        assert_eq!(FactorType::Priority.to_string(), "Priority");
        assert_eq!(FactorType::Deadline.to_string(), "Deadline");
        assert_eq!(FactorType::CostEfficiency.to_string(), "CostEfficiency");
    }

    #[test]
    fn confidence_label_display() {
        assert_eq!(ConfidenceLabel::Low.to_string(), "Low");
        assert_eq!(ConfidenceLabel::Medium.to_string(), "Medium");
        assert_eq!(ConfidenceLabel::High.to_string(), "High");
    }

    #[test]
    fn decision_outcome_side_effects() {
        let mut eng = engine();
        let rec = sample_record("a", "x", 0.5);
        let id = eng.record_decision(rec).unwrap();
        let outcome = DecisionOutcome {
            success: true,
            result_summary: "done".to_string(),
            duration_ms: 42,
            side_effects: vec!["file-created".to_string(), "cache-cleared".to_string()],
        };
        eng.record_outcome(&id, outcome).unwrap();
        let d = eng.get_decision(&id).unwrap();
        let o = d.outcome.as_ref().unwrap();
        assert_eq!(o.side_effects.len(), 2);
        assert_eq!(o.duration_ms, 42);
    }

    #[test]
    fn multiple_agents_stats_independent() {
        let mut eng = engine();
        eng.record_decision(sample_record("a1", "x", 0.9)).unwrap();
        eng.record_decision(sample_record("a1", "y", 0.8)).unwrap();
        eng.record_decision(sample_record("a2", "z", 0.1)).unwrap();
        let s1 = eng.agent_decision_stats("a1");
        let s2 = eng.agent_decision_stats("a2");
        assert_eq!(s1.total_decisions, 2);
        assert_eq!(s2.total_decisions, 1);
        assert_eq!(s1.decisions_needing_review, 0);
        assert_eq!(s2.decisions_needing_review, 1);
    }

    #[test]
    fn search_confidence_range() {
        let mut eng = engine();
        eng.record_decision(sample_record("a", "x", 0.1)).unwrap();
        eng.record_decision(sample_record("a", "y", 0.5)).unwrap();
        eng.record_decision(sample_record("a", "z", 0.9)).unwrap();
        let f = DecisionFilter {
            min_confidence: Some(0.3),
            max_confidence: Some(0.7),
            ..Default::default()
        };
        let results = eng.search_decisions(&f);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].action, "y");
    }
}
