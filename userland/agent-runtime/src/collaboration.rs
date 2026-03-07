//! Human-AI Collaboration Framework
//!
//! Models and optimizes how humans and AI agents work together. Implements
//! shared task ownership, trust calibration, handoff protocols, cognitive load
//! management, and feedback loops — all grounded in the Human Sovereignty
//! principle: AI assists, but humans decide.

use std::collections::HashMap;
use std::fmt;

use anyhow::{bail, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{debug, info};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// CollaborationMode
// ---------------------------------------------------------------------------

/// How a human and AI agent collaborate within a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CollaborationMode {
    /// Agent works independently; human reviews results.
    FullAutonomy,
    /// Agent proposes; human approves each step.
    Supervised,
    /// Human and agent work simultaneously on different subtasks.
    Paired,
    /// Human drives; agent assists on request.
    HumanLed,
    /// Human demonstrates; agent learns patterns.
    TeachingMode,
}

impl fmt::Display for CollaborationMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CollaborationMode::FullAutonomy => write!(f, "Full Autonomy"),
            CollaborationMode::Supervised => write!(f, "Supervised"),
            CollaborationMode::Paired => write!(f, "Paired"),
            CollaborationMode::HumanLed => write!(f, "Human-Led"),
            CollaborationMode::TeachingMode => write!(f, "Teaching Mode"),
        }
    }
}

// ---------------------------------------------------------------------------
// CollaborationSession
// ---------------------------------------------------------------------------

/// A bounded collaboration session between one human and one AI agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollaborationSession {
    pub session_id: Uuid,
    pub human_id: String,
    pub agent_id: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub mode: CollaborationMode,
    pub tasks_completed: u32,
    pub handoffs: u32,
    pub interventions: u32,
}

impl CollaborationSession {
    /// Create a new collaboration session.
    pub fn new(human_id: String, agent_id: String, mode: CollaborationMode) -> Self {
        let session = Self {
            session_id: Uuid::new_v4(),
            human_id,
            agent_id,
            started_at: Utc::now(),
            ended_at: None,
            mode,
            tasks_completed: 0,
            handoffs: 0,
            interventions: 0,
        };
        info!(
            session_id = %session.session_id,
            mode = %mode,
            "collaboration session started"
        );
        session
    }

    /// End the session, recording the finish time.
    pub fn end(&mut self) {
        self.ended_at = Some(Utc::now());
        info!(
            session_id = %self.session_id,
            tasks = self.tasks_completed,
            handoffs = self.handoffs,
            interventions = self.interventions,
            "collaboration session ended"
        );
    }

    /// Duration of the session in minutes (returns 0.0 if still active).
    pub fn duration_mins(&self) -> f64 {
        let end = self.ended_at.unwrap_or_else(Utc::now);
        let dur = end.signed_duration_since(self.started_at);
        dur.num_seconds().max(0) as f64 / 60.0
    }
}

// ---------------------------------------------------------------------------
// TaskOwner / SharedTaskStatus / SharedTask
// ---------------------------------------------------------------------------

/// Who owns a shared task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TaskOwner {
    Human,
    Agent,
    Shared,
}

impl TaskOwner {
    /// Whether this ownership type requires explicit human approval before
    /// the task result is accepted.
    pub fn requires_human_approval(&self) -> bool {
        matches!(self, TaskOwner::Agent | TaskOwner::Shared)
    }
}

/// Lifecycle status of a shared task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SharedTaskStatus {
    Open,
    InProgress,
    AwaitingReview,
    AwaitingApproval,
    Complete,
    Blocked,
}

impl SharedTaskStatus {
    /// Whether transitioning from `self` to `target` is valid.
    pub fn valid_transition(&self, target: &SharedTaskStatus) -> bool {
        use SharedTaskStatus::*;
        matches!(
            (self, target),
            (Open, InProgress)
                | (Open, Blocked)
                | (InProgress, AwaitingReview)
                | (InProgress, AwaitingApproval)
                | (InProgress, Blocked)
                | (AwaitingReview, AwaitingApproval)
                | (AwaitingReview, InProgress) // reviewer requests rework
                | (AwaitingApproval, Complete)
                | (AwaitingApproval, InProgress) // approval denied, rework
                | (Blocked, Open)
                | (Blocked, InProgress)
        )
    }
}

/// A task that is shared between a human and an AI agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedTask {
    pub task_id: Uuid,
    pub title: String,
    pub description: String,
    pub owner: TaskOwner,
    pub status: SharedTaskStatus,
    /// Fraction of work done by the human (0.0–1.0).
    pub human_contribution: f64,
    /// Fraction of work done by the agent (0.0–1.0).
    pub agent_contribution: f64,
    pub created_at: DateTime<Utc>,
    pub deadline: Option<DateTime<Utc>>,
}

impl SharedTask {
    /// Create a new shared task.
    pub fn new(title: String, description: String, owner: TaskOwner) -> Self {
        Self {
            task_id: Uuid::new_v4(),
            title,
            description,
            owner,
            status: SharedTaskStatus::Open,
            human_contribution: 0.0,
            agent_contribution: 0.0,
            created_at: Utc::now(),
            deadline: None,
        }
    }

    /// Attempt a status transition; returns an error if invalid.
    pub fn transition(&mut self, target: SharedTaskStatus) -> Result<()> {
        if !self.status.valid_transition(&target) {
            bail!(
                "invalid task transition from {:?} to {:?}",
                self.status,
                target
            );
        }
        debug!(
            task_id = %self.task_id,
            from = ?self.status,
            to = ?target,
            "task status transition"
        );
        self.status = target;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Handoff Protocol
// ---------------------------------------------------------------------------

/// Which party in a handoff.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HandoffParty {
    Human,
    Agent,
}

impl fmt::Display for HandoffParty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HandoffParty::Human => write!(f, "Human"),
            HandoffParty::Agent => write!(f, "Agent"),
        }
    }
}

/// A single handoff event between human and agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Handoff {
    pub handoff_id: Uuid,
    pub task_id: String,
    pub from: HandoffParty,
    pub to: HandoffParty,
    pub reason: String,
    pub context_summary: String,
    pub timestamp: DateTime<Utc>,
    pub acknowledged: bool,
}

/// Manages handoffs between human and agent participants.
#[derive(Debug, Default)]
pub struct HandoffManager {
    handoffs: Vec<Handoff>,
}

impl HandoffManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Initiate a handoff of a task from one party to another.
    pub fn initiate_handoff(
        &mut self,
        task_id: String,
        from: HandoffParty,
        to: HandoffParty,
        reason: String,
        context_summary: String,
    ) -> Result<Handoff> {
        if from == to {
            bail!("cannot hand off to the same party");
        }
        let handoff = Handoff {
            handoff_id: Uuid::new_v4(),
            task_id,
            from,
            to,
            reason,
            context_summary,
            timestamp: Utc::now(),
            acknowledged: false,
        };
        info!(
            handoff_id = %handoff.handoff_id,
            from = %handoff.from,
            to = %handoff.to,
            "handoff initiated"
        );
        self.handoffs.push(handoff.clone());
        Ok(handoff)
    }

    /// Mark a handoff as acknowledged by the receiving party.
    pub fn acknowledge_handoff(&mut self, handoff_id: Uuid) -> Result<()> {
        let handoff = self
            .handoffs
            .iter_mut()
            .find(|h| h.handoff_id == handoff_id)
            .ok_or_else(|| anyhow::anyhow!("handoff not found: {}", handoff_id))?;
        if handoff.acknowledged {
            bail!("handoff already acknowledged: {}", handoff_id);
        }
        handoff.acknowledged = true;
        debug!(handoff_id = %handoff_id, "handoff acknowledged");
        Ok(())
    }

    /// Return all unacknowledged handoffs.
    pub fn pending_handoffs(&self) -> Vec<&Handoff> {
        self.handoffs.iter().filter(|h| !h.acknowledged).collect()
    }

    /// Return all handoffs for a given task, in chronological order.
    pub fn handoff_history(&self, task_id: &str) -> Vec<&Handoff> {
        self.handoffs
            .iter()
            .filter(|h| h.task_id == task_id)
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Trust Calibration
// ---------------------------------------------------------------------------

/// Direction of trust change over time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrustTrend {
    Improving,
    Stable,
    Declining,
    InsufficientData,
}

/// Quantitative trust metrics for an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustMetrics {
    pub agent_id: String,
    pub accuracy: f64,
    pub consistency: f64,
    pub response_quality: f64,
    pub safety_record: f64,
    pub overall_trust: f64,
    pub sample_count: u64,
}

impl TrustMetrics {
    fn new(agent_id: String) -> Self {
        Self {
            agent_id,
            accuracy: 0.5,
            consistency: 0.5,
            response_quality: 0.5,
            safety_record: 1.0,
            overall_trust: 0.5,
            sample_count: 0,
        }
    }

    /// Recalculate overall trust as a weighted average.
    fn recalculate_overall(&mut self) {
        self.overall_trust = self.accuracy * 0.35
            + self.consistency * 0.20
            + self.response_quality * 0.20
            + self.safety_record * 0.25;
    }
}

/// Calibration outcome record (predicted confidence vs actual success).
#[derive(Debug, Clone)]
struct CalibrationSample {
    predicted_confidence: f64,
    actual_success: bool,
}

/// Tracks and calibrates trust between humans and AI agents.
#[derive(Debug, Default)]
pub struct TrustCalibrator {
    metrics: HashMap<String, TrustMetrics>,
    samples: HashMap<String, Vec<CalibrationSample>>,
    /// Stores recent overall_trust values for trend detection.
    history: HashMap<String, Vec<f64>>,
}

impl TrustCalibrator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a calibration outcome: the agent predicted `predicted_confidence`
    /// and the actual result was `actual_success`.
    pub fn record_outcome(
        &mut self,
        agent_id: &str,
        predicted_confidence: f64,
        actual_success: bool,
    ) {
        let samples = self
            .samples
            .entry(agent_id.to_string())
            .or_default();
        samples.push(CalibrationSample {
            predicted_confidence,
            actual_success,
        });

        // Ensure metrics entry exists.
        let metrics = self
            .metrics
            .entry(agent_id.to_string())
            .or_insert_with(|| TrustMetrics::new(agent_id.to_string()));
        metrics.sample_count = samples.len() as u64;

        // Update accuracy based on all samples.
        let total = samples.len() as f64;
        let successes = samples.iter().filter(|s| s.actual_success).count() as f64;
        metrics.accuracy = successes / total;
        metrics.recalculate_overall();

        // Record history point.
        self.history
            .entry(agent_id.to_string())
            .or_default()
            .push(metrics.overall_trust);

        debug!(
            agent_id,
            sample_count = metrics.sample_count,
            accuracy = metrics.accuracy,
            "trust calibration outcome recorded"
        );
    }

    /// Get current trust metrics for an agent.
    pub fn get_trust(&self, agent_id: &str) -> Option<&TrustMetrics> {
        self.metrics.get(agent_id)
    }

    /// Update individual metrics using exponential moving average (alpha = 0.3).
    pub fn update_metrics(
        &mut self,
        agent_id: &str,
        accuracy: f64,
        consistency: f64,
        quality: f64,
        safety: f64,
    ) {
        let metrics = self
            .metrics
            .entry(agent_id.to_string())
            .or_insert_with(|| TrustMetrics::new(agent_id.to_string()));

        const ALPHA: f64 = 0.3;
        metrics.accuracy = (metrics.accuracy * (1.0 - ALPHA) + accuracy * ALPHA).clamp(0.0, 1.0);
        metrics.consistency =
            (metrics.consistency * (1.0 - ALPHA) + consistency * ALPHA).clamp(0.0, 1.0);
        metrics.response_quality =
            (metrics.response_quality * (1.0 - ALPHA) + quality * ALPHA).clamp(0.0, 1.0);
        metrics.safety_record =
            (metrics.safety_record * (1.0 - ALPHA) + safety * ALPHA).clamp(0.0, 1.0);
        metrics.recalculate_overall();

        self.history
            .entry(agent_id.to_string())
            .or_default()
            .push(metrics.overall_trust);

        info!(
            agent_id,
            overall_trust = metrics.overall_trust,
            "trust metrics updated"
        );
    }

    /// Absolute calibration error: |mean predicted confidence - actual accuracy|.
    pub fn calibration_error(&self, agent_id: &str) -> Option<f64> {
        let samples = self.samples.get(agent_id)?;
        if samples.is_empty() {
            return None;
        }
        let mean_predicted =
            samples.iter().map(|s| s.predicted_confidence).sum::<f64>() / samples.len() as f64;
        let actual_accuracy =
            samples.iter().filter(|s| s.actual_success).count() as f64 / samples.len() as f64;
        Some((mean_predicted - actual_accuracy).abs())
    }

    /// Whether the agent is well-calibrated (calibration error < 0.1).
    pub fn is_well_calibrated(&self, agent_id: &str) -> bool {
        self.calibration_error(agent_id)
            .map(|e| e < 0.1)
            .unwrap_or(false)
    }

    /// Detect the trend of trust over the last few data points.
    pub fn trust_trend(&self, agent_id: &str) -> TrustTrend {
        let history = match self.history.get(agent_id) {
            Some(h) if h.len() >= 3 => h,
            _ => return TrustTrend::InsufficientData,
        };

        // Compare the average of the last 3 points to the 3 before them.
        let len = history.len();
        let recent: f64 = history[len.saturating_sub(3)..].iter().sum::<f64>() / 3.0;

        if len < 6 {
            // Only 3-5 points: compare to first value.
            let first = history[0];
            let diff = recent - first;
            if diff > 0.05 {
                TrustTrend::Improving
            } else if diff < -0.05 {
                TrustTrend::Declining
            } else {
                TrustTrend::Stable
            }
        } else {
            let earlier: f64 =
                history[len.saturating_sub(6)..len.saturating_sub(3)].iter().sum::<f64>() / 3.0;
            let diff = recent - earlier;
            if diff > 0.05 {
                TrustTrend::Improving
            } else if diff < -0.05 {
                TrustTrend::Declining
            } else {
                TrustTrend::Stable
            }
        }
    }

    /// Recommend a collaboration mode based on the agent's overall trust level.
    pub fn recommend_mode(&self, agent_id: &str) -> CollaborationMode {
        match self.metrics.get(agent_id) {
            None => CollaborationMode::Supervised,
            Some(m) => {
                if m.overall_trust >= 0.85 {
                    CollaborationMode::FullAutonomy
                } else if m.overall_trust >= 0.70 {
                    CollaborationMode::Paired
                } else if m.overall_trust >= 0.50 {
                    CollaborationMode::Supervised
                } else {
                    CollaborationMode::HumanLed
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Cognitive Load Management
// ---------------------------------------------------------------------------

/// Snapshot of a human's cognitive load.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CognitiveLoad {
    pub human_id: String,
    pub current_tasks: u32,
    pub pending_decisions: u32,
    pub interruption_count: u32,
    pub time_since_break_mins: u32,
    pub estimated_load: f64,
}

/// Manages cognitive load observations for humans and derives recommendations.
#[derive(Debug, Default)]
pub struct LoadManager {
    loads: HashMap<String, CognitiveLoad>,
}

impl LoadManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record or update a cognitive load snapshot for a human.
    pub fn update_load(&mut self, human_id: &str, load: CognitiveLoad) {
        debug!(
            human_id,
            estimated_load = load.estimated_load,
            "cognitive load updated"
        );
        self.loads.insert(human_id.to_string(), load);
    }

    /// Get the current cognitive load for a human.
    pub fn get_load(&self, human_id: &str) -> Option<&CognitiveLoad> {
        self.loads.get(human_id)
    }

    /// Whether the human is considered overloaded (estimated_load > 0.8).
    pub fn is_overloaded(&self, human_id: &str) -> bool {
        self.loads
            .get(human_id)
            .map(|l| l.estimated_load > 0.8)
            .unwrap_or(false)
    }

    /// Whether non-urgent decisions should be deferred.
    pub fn should_defer(&self, human_id: &str) -> bool {
        self.is_overloaded(human_id)
    }

    /// Whether a break should be suggested (time > 90 min and load > 0.6).
    pub fn suggest_break(&self, human_id: &str) -> bool {
        self.loads
            .get(human_id)
            .map(|l| l.time_since_break_mins > 90 && l.estimated_load > 0.6)
            .unwrap_or(false)
    }

    /// Optimal number of decisions to present at once. Fewer when loaded.
    pub fn optimal_batch_size(&self, human_id: &str) -> u32 {
        match self.loads.get(human_id) {
            None => 5, // default
            Some(l) => {
                if l.estimated_load > 0.8 {
                    1
                } else if l.estimated_load > 0.6 {
                    2
                } else if l.estimated_load > 0.4 {
                    3
                } else {
                    5
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Feedback Loop
// ---------------------------------------------------------------------------

/// Category of feedback.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FeedbackType {
    Correction,
    Praise,
    Suggestion,
    Complaint,
    /// Numeric rating (1–5).
    Rating(u8),
}

/// A piece of human feedback about an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Feedback {
    pub feedback_id: Uuid,
    pub session_id: String,
    pub agent_id: String,
    pub feedback_type: FeedbackType,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    pub applied: bool,
}

/// Aggregated feedback statistics for an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedbackStats {
    pub total: usize,
    pub by_type: HashMap<String, usize>,
    pub avg_rating: f64,
    pub applied_count: usize,
    pub application_rate: f64,
}

/// Collects and manages human feedback.
#[derive(Debug, Default)]
pub struct FeedbackCollector {
    feedback: Vec<Feedback>,
}

impl FeedbackCollector {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a piece of feedback. Validates rating values (must be 1–5).
    pub fn record_feedback(&mut self, feedback: Feedback) -> Result<()> {
        if let FeedbackType::Rating(r) = &feedback.feedback_type {
            if *r < 1 || *r > 5 {
                bail!("rating must be between 1 and 5, got {}", r);
            }
        }
        info!(
            feedback_id = %feedback.feedback_id,
            agent_id = %feedback.agent_id,
            feedback_type = ?feedback.feedback_type,
            "feedback recorded"
        );
        self.feedback.push(feedback);
        Ok(())
    }

    /// All feedback for a given agent.
    pub fn feedback_for_agent(&self, agent_id: &str) -> Vec<&Feedback> {
        self.feedback
            .iter()
            .filter(|f| f.agent_id == agent_id)
            .collect()
    }

    /// All feedback for a given session.
    pub fn feedback_for_session(&self, session_id: &str) -> Vec<&Feedback> {
        self.feedback
            .iter()
            .filter(|f| f.session_id == session_id)
            .collect()
    }

    /// Mark a feedback item as applied.
    pub fn mark_applied(&mut self, feedback_id: Uuid) -> Result<()> {
        let fb = self
            .feedback
            .iter_mut()
            .find(|f| f.feedback_id == feedback_id)
            .ok_or_else(|| anyhow::anyhow!("feedback not found: {}", feedback_id))?;
        fb.applied = true;
        debug!(feedback_id = %feedback_id, "feedback marked as applied");
        Ok(())
    }

    /// Aggregated statistics for an agent.
    pub fn feedback_stats(&self, agent_id: &str) -> FeedbackStats {
        let agent_fb: Vec<&Feedback> = self.feedback_for_agent(agent_id);
        let total = agent_fb.len();

        let mut by_type: HashMap<String, usize> = HashMap::new();
        let mut rating_sum: f64 = 0.0;
        let mut rating_count: usize = 0;
        let mut applied_count: usize = 0;

        for fb in &agent_fb {
            let type_key = match &fb.feedback_type {
                FeedbackType::Correction => "Correction".to_string(),
                FeedbackType::Praise => "Praise".to_string(),
                FeedbackType::Suggestion => "Suggestion".to_string(),
                FeedbackType::Complaint => "Complaint".to_string(),
                FeedbackType::Rating(_) => "Rating".to_string(),
            };
            *by_type.entry(type_key).or_insert(0) += 1;

            if let FeedbackType::Rating(r) = &fb.feedback_type {
                rating_sum += *r as f64;
                rating_count += 1;
            }
            if fb.applied {
                applied_count += 1;
            }
        }

        let avg_rating = if rating_count > 0 {
            rating_sum / rating_count as f64
        } else {
            0.0
        };
        let application_rate = if total > 0 {
            applied_count as f64 / total as f64
        } else {
            0.0
        };

        FeedbackStats {
            total,
            by_type,
            avg_rating,
            applied_count,
            application_rate,
        }
    }

    /// Corrections that have not yet been applied.
    pub fn unapplied_corrections(&self, agent_id: &str) -> Vec<&Feedback> {
        self.feedback
            .iter()
            .filter(|f| {
                f.agent_id == agent_id
                    && f.feedback_type == FeedbackType::Correction
                    && !f.applied
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Collaboration Analytics
// ---------------------------------------------------------------------------

/// Analytics for a single collaboration session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionAnalytics {
    pub session_id: Uuid,
    pub total_duration_mins: f64,
    pub human_active_mins: f64,
    pub agent_active_mins: f64,
    pub efficiency_score: f64,
    pub handoff_overhead_percent: f64,
    pub mode_effectiveness: f64,
    pub tasks_completed: u32,
    pub mode: CollaborationMode,
}

/// Aggregate statistics across multiple sessions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverallStats {
    pub total_sessions: usize,
    pub total_tasks: u32,
    pub avg_efficiency: f64,
    pub avg_handoff_overhead: f64,
    pub most_effective_mode: Option<CollaborationMode>,
    pub total_human_hours: f64,
    pub total_agent_hours: f64,
}

/// Analyzes collaboration effectiveness.
pub struct CollaborationAnalyzer;

impl CollaborationAnalyzer {
    /// Analyze a single session given its tasks and handoffs.
    pub fn analyze_session(
        session: &CollaborationSession,
        tasks: &[SharedTask],
        handoffs: &[Handoff],
    ) -> SessionAnalytics {
        let total_duration_mins = session.duration_mins();

        // Estimate active times from contribution ratios.
        let (human_contrib_sum, agent_contrib_sum) = tasks.iter().fold((0.0_f64, 0.0_f64), |acc, t| {
            (acc.0 + t.human_contribution, acc.1 + t.agent_contribution)
        });
        let total_contrib = human_contrib_sum + agent_contrib_sum;
        let (human_active_mins, agent_active_mins) = if total_contrib > 0.0 {
            (
                total_duration_mins * (human_contrib_sum / total_contrib),
                total_duration_mins * (agent_contrib_sum / total_contrib),
            )
        } else {
            (total_duration_mins * 0.5, total_duration_mins * 0.5)
        };

        // Efficiency: tasks completed per minute (handle zero duration).
        let efficiency_score = if total_duration_mins > 0.0 {
            session.tasks_completed as f64 / total_duration_mins
        } else {
            0.0
        };

        // Handoff overhead: estimate 2 minutes per handoff.
        let handoff_time = handoffs.len() as f64 * 2.0;
        let handoff_overhead_percent = if total_duration_mins > 0.0 {
            (handoff_time / total_duration_mins) * 100.0
        } else {
            0.0
        };

        // Mode effectiveness: ratio of completed tasks to total tasks.
        let mode_effectiveness = if tasks.is_empty() {
            0.0
        } else {
            let completed = tasks
                .iter()
                .filter(|t| t.status == SharedTaskStatus::Complete)
                .count();
            completed as f64 / tasks.len() as f64
        };

        let tasks_completed = tasks
            .iter()
            .filter(|t| t.status == SharedTaskStatus::Complete)
            .count() as u32;

        SessionAnalytics {
            session_id: session.session_id,
            total_duration_mins,
            human_active_mins,
            agent_active_mins,
            efficiency_score,
            handoff_overhead_percent,
            mode_effectiveness,
            tasks_completed,
            mode: session.mode,
        }
    }

    /// Given a list of (description, mode, effectiveness) tuples, determine
    /// which mode works best for each keyword found in descriptions.
    pub fn best_mode_for_task_type(
        task_descriptions: &[(&str, CollaborationMode, f64)],
    ) -> HashMap<String, CollaborationMode> {
        // Extract keywords and track best effectiveness per keyword.
        let mut keyword_modes: HashMap<String, (CollaborationMode, f64)> = HashMap::new();

        for (desc, mode, effectiveness) in task_descriptions {
            // Split description into lowercase keywords.
            for word in desc.split_whitespace() {
                let keyword = word.to_lowercase();
                if keyword.len() < 3 {
                    continue; // skip short words
                }
                let entry = keyword_modes
                    .entry(keyword)
                    .or_insert((*mode, *effectiveness));
                if *effectiveness > entry.1 {
                    *entry = (*mode, *effectiveness);
                }
            }
        }

        keyword_modes
            .into_iter()
            .map(|(k, (mode, _))| (k, mode))
            .collect()
    }

    /// Aggregate statistics across multiple session analytics.
    pub fn overall_stats(sessions: &[SessionAnalytics]) -> OverallStats {
        if sessions.is_empty() {
            return OverallStats {
                total_sessions: 0,
                total_tasks: 0,
                avg_efficiency: 0.0,
                avg_handoff_overhead: 0.0,
                most_effective_mode: None,
                total_human_hours: 0.0,
                total_agent_hours: 0.0,
            };
        }

        let n = sessions.len() as f64;
        let total_human_hours = sessions.iter().map(|s| s.human_active_mins).sum::<f64>() / 60.0;
        let total_agent_hours = sessions.iter().map(|s| s.agent_active_mins).sum::<f64>() / 60.0;
        let avg_efficiency = sessions.iter().map(|s| s.efficiency_score).sum::<f64>() / n;
        let avg_handoff_overhead =
            sessions.iter().map(|s| s.handoff_overhead_percent).sum::<f64>() / n;

        let total_tasks: u32 = sessions.iter().map(|s| s.tasks_completed).sum();

        // Group sessions by mode and compute average mode_effectiveness per mode.
        let mut mode_totals: HashMap<CollaborationMode, (f64, usize)> = HashMap::new();
        for s in sessions {
            let entry = mode_totals.entry(s.mode).or_insert((0.0, 0));
            entry.0 += s.mode_effectiveness;
            entry.1 += 1;
        }
        let most_effective_mode = mode_totals
            .into_iter()
            .map(|(mode, (sum, count))| (mode, sum / count as f64))
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(mode, _)| mode);

        OverallStats {
            total_sessions: sessions.len(),
            total_tasks,
            avg_efficiency,
            avg_handoff_overhead,
            most_effective_mode,
            total_human_hours,
            total_agent_hours,
        }
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -- CollaborationMode --------------------------------------------------

    #[test]
    fn test_mode_display_full_autonomy() {
        assert_eq!(CollaborationMode::FullAutonomy.to_string(), "Full Autonomy");
    }

    #[test]
    fn test_mode_display_supervised() {
        assert_eq!(CollaborationMode::Supervised.to_string(), "Supervised");
    }

    #[test]
    fn test_mode_display_paired() {
        assert_eq!(CollaborationMode::Paired.to_string(), "Paired");
    }

    #[test]
    fn test_mode_display_human_led() {
        assert_eq!(CollaborationMode::HumanLed.to_string(), "Human-Led");
    }

    #[test]
    fn test_mode_display_teaching() {
        assert_eq!(CollaborationMode::TeachingMode.to_string(), "Teaching Mode");
    }

    #[test]
    fn test_mode_equality() {
        assert_eq!(CollaborationMode::Paired, CollaborationMode::Paired);
        assert_ne!(CollaborationMode::Paired, CollaborationMode::Supervised);
    }

    // -- CollaborationSession -----------------------------------------------

    #[test]
    fn test_session_creation() {
        let s = CollaborationSession::new(
            "human-1".into(),
            "agent-1".into(),
            CollaborationMode::Supervised,
        );
        assert_eq!(s.human_id, "human-1");
        assert_eq!(s.agent_id, "agent-1");
        assert_eq!(s.mode, CollaborationMode::Supervised);
        assert_eq!(s.tasks_completed, 0);
        assert_eq!(s.handoffs, 0);
        assert_eq!(s.interventions, 0);
        assert!(s.ended_at.is_none());
    }

    #[test]
    fn test_session_end() {
        let mut s = CollaborationSession::new(
            "h".into(),
            "a".into(),
            CollaborationMode::FullAutonomy,
        );
        s.end();
        assert!(s.ended_at.is_some());
    }

    #[test]
    fn test_session_duration() {
        let mut s = CollaborationSession::new(
            "h".into(),
            "a".into(),
            CollaborationMode::HumanLed,
        );
        // Duration of an active session should be non-negative.
        assert!(s.duration_mins() >= 0.0);
        s.end();
        assert!(s.duration_mins() >= 0.0);
    }

    // -- TaskOwner ----------------------------------------------------------

    #[test]
    fn test_human_owner_no_approval_required() {
        assert!(!TaskOwner::Human.requires_human_approval());
    }

    #[test]
    fn test_agent_owner_requires_approval() {
        assert!(TaskOwner::Agent.requires_human_approval());
    }

    #[test]
    fn test_shared_owner_requires_approval() {
        assert!(TaskOwner::Shared.requires_human_approval());
    }

    // -- SharedTaskStatus transitions ---------------------------------------

    #[test]
    fn test_valid_transition_open_to_in_progress() {
        assert!(SharedTaskStatus::Open.valid_transition(&SharedTaskStatus::InProgress));
    }

    #[test]
    fn test_valid_transition_open_to_blocked() {
        assert!(SharedTaskStatus::Open.valid_transition(&SharedTaskStatus::Blocked));
    }

    #[test]
    fn test_valid_transition_in_progress_to_awaiting_review() {
        assert!(
            SharedTaskStatus::InProgress.valid_transition(&SharedTaskStatus::AwaitingReview)
        );
    }

    #[test]
    fn test_valid_transition_in_progress_to_awaiting_approval() {
        assert!(
            SharedTaskStatus::InProgress.valid_transition(&SharedTaskStatus::AwaitingApproval)
        );
    }

    #[test]
    fn test_valid_transition_awaiting_review_to_approval() {
        assert!(
            SharedTaskStatus::AwaitingReview.valid_transition(&SharedTaskStatus::AwaitingApproval)
        );
    }

    #[test]
    fn test_valid_transition_awaiting_approval_to_complete() {
        assert!(
            SharedTaskStatus::AwaitingApproval.valid_transition(&SharedTaskStatus::Complete)
        );
    }

    #[test]
    fn test_valid_transition_blocked_to_open() {
        assert!(SharedTaskStatus::Blocked.valid_transition(&SharedTaskStatus::Open));
    }

    #[test]
    fn test_valid_transition_blocked_to_in_progress() {
        assert!(SharedTaskStatus::Blocked.valid_transition(&SharedTaskStatus::InProgress));
    }

    #[test]
    fn test_valid_transition_rework_from_review() {
        assert!(
            SharedTaskStatus::AwaitingReview.valid_transition(&SharedTaskStatus::InProgress)
        );
    }

    #[test]
    fn test_valid_transition_rework_from_approval() {
        assert!(
            SharedTaskStatus::AwaitingApproval.valid_transition(&SharedTaskStatus::InProgress)
        );
    }

    #[test]
    fn test_invalid_transition_open_to_complete() {
        assert!(!SharedTaskStatus::Open.valid_transition(&SharedTaskStatus::Complete));
    }

    #[test]
    fn test_invalid_transition_complete_to_open() {
        assert!(!SharedTaskStatus::Complete.valid_transition(&SharedTaskStatus::Open));
    }

    #[test]
    fn test_invalid_transition_same_state() {
        assert!(
            !SharedTaskStatus::InProgress.valid_transition(&SharedTaskStatus::InProgress)
        );
    }

    // -- SharedTask ---------------------------------------------------------

    #[test]
    fn test_shared_task_creation() {
        let t = SharedTask::new("Fix bug".into(), "Segfault in parser".into(), TaskOwner::Shared);
        assert_eq!(t.title, "Fix bug");
        assert_eq!(t.status, SharedTaskStatus::Open);
        assert_eq!(t.human_contribution, 0.0);
        assert_eq!(t.agent_contribution, 0.0);
    }

    #[test]
    fn test_shared_task_valid_transition() {
        let mut t = SharedTask::new("t".into(), "d".into(), TaskOwner::Agent);
        assert!(t.transition(SharedTaskStatus::InProgress).is_ok());
        assert_eq!(t.status, SharedTaskStatus::InProgress);
    }

    #[test]
    fn test_shared_task_invalid_transition() {
        let mut t = SharedTask::new("t".into(), "d".into(), TaskOwner::Human);
        assert!(t.transition(SharedTaskStatus::Complete).is_err());
        assert_eq!(t.status, SharedTaskStatus::Open); // unchanged
    }

    // -- Handoff lifecycle --------------------------------------------------

    #[test]
    fn test_handoff_initiate() {
        let mut mgr = HandoffManager::new();
        let h = mgr
            .initiate_handoff(
                "task-1".into(),
                HandoffParty::Agent,
                HandoffParty::Human,
                "need clarification".into(),
                "agent completed 50%".into(),
            )
            .unwrap();
        assert!(!h.acknowledged);
        assert_eq!(h.task_id, "task-1");
    }

    #[test]
    fn test_handoff_same_party_error() {
        let mut mgr = HandoffManager::new();
        let result = mgr.initiate_handoff(
            "t".into(),
            HandoffParty::Human,
            HandoffParty::Human,
            "r".into(),
            "c".into(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_handoff_acknowledge() {
        let mut mgr = HandoffManager::new();
        let h = mgr
            .initiate_handoff(
                "t".into(),
                HandoffParty::Human,
                HandoffParty::Agent,
                "r".into(),
                "c".into(),
            )
            .unwrap();
        assert!(mgr.acknowledge_handoff(h.handoff_id).is_ok());
    }

    #[test]
    fn test_handoff_double_acknowledge_error() {
        let mut mgr = HandoffManager::new();
        let h = mgr
            .initiate_handoff(
                "t".into(),
                HandoffParty::Human,
                HandoffParty::Agent,
                "r".into(),
                "c".into(),
            )
            .unwrap();
        mgr.acknowledge_handoff(h.handoff_id).unwrap();
        assert!(mgr.acknowledge_handoff(h.handoff_id).is_err());
    }

    #[test]
    fn test_handoff_acknowledge_unknown_id() {
        let mut mgr = HandoffManager::new();
        assert!(mgr.acknowledge_handoff(Uuid::new_v4()).is_err());
    }

    #[test]
    fn test_pending_handoffs() {
        let mut mgr = HandoffManager::new();
        let h1 = mgr
            .initiate_handoff(
                "t1".into(),
                HandoffParty::Agent,
                HandoffParty::Human,
                "r".into(),
                "c".into(),
            )
            .unwrap();
        let _h2 = mgr
            .initiate_handoff(
                "t2".into(),
                HandoffParty::Human,
                HandoffParty::Agent,
                "r".into(),
                "c".into(),
            )
            .unwrap();
        assert_eq!(mgr.pending_handoffs().len(), 2);
        mgr.acknowledge_handoff(h1.handoff_id).unwrap();
        assert_eq!(mgr.pending_handoffs().len(), 1);
    }

    #[test]
    fn test_handoff_history() {
        let mut mgr = HandoffManager::new();
        mgr.initiate_handoff(
            "task-A".into(),
            HandoffParty::Agent,
            HandoffParty::Human,
            "r".into(),
            "c".into(),
        )
        .unwrap();
        mgr.initiate_handoff(
            "task-A".into(),
            HandoffParty::Human,
            HandoffParty::Agent,
            "r".into(),
            "c".into(),
        )
        .unwrap();
        mgr.initiate_handoff(
            "task-B".into(),
            HandoffParty::Agent,
            HandoffParty::Human,
            "r".into(),
            "c".into(),
        )
        .unwrap();
        assert_eq!(mgr.handoff_history("task-A").len(), 2);
        assert_eq!(mgr.handoff_history("task-B").len(), 1);
        assert_eq!(mgr.handoff_history("task-C").len(), 0);
    }

    // -- Trust Calibration --------------------------------------------------

    #[test]
    fn test_trust_record_outcome_success() {
        let mut cal = TrustCalibrator::new();
        cal.record_outcome("a1", 0.8, true);
        let m = cal.get_trust("a1").unwrap();
        assert_eq!(m.sample_count, 1);
        assert_eq!(m.accuracy, 1.0);
    }

    #[test]
    fn test_trust_record_outcome_failure() {
        let mut cal = TrustCalibrator::new();
        cal.record_outcome("a1", 0.9, false);
        let m = cal.get_trust("a1").unwrap();
        assert_eq!(m.accuracy, 0.0);
    }

    #[test]
    fn test_trust_record_multiple_outcomes() {
        let mut cal = TrustCalibrator::new();
        cal.record_outcome("a1", 0.7, true);
        cal.record_outcome("a1", 0.7, false);
        let m = cal.get_trust("a1").unwrap();
        assert!((m.accuracy - 0.5).abs() < 1e-9);
        assert_eq!(m.sample_count, 2);
    }

    #[test]
    fn test_trust_unknown_agent() {
        let cal = TrustCalibrator::new();
        assert!(cal.get_trust("nonexistent").is_none());
    }

    #[test]
    fn test_trust_update_metrics_ema() {
        let mut cal = TrustCalibrator::new();
        // Seed with a record so initial values are known.
        cal.record_outcome("a1", 0.5, true);
        let before = cal.get_trust("a1").unwrap().overall_trust;
        cal.update_metrics("a1", 1.0, 1.0, 1.0, 1.0);
        let after = cal.get_trust("a1").unwrap().overall_trust;
        assert!(after > before);
    }

    #[test]
    fn test_calibration_error_well_calibrated() {
        let mut cal = TrustCalibrator::new();
        // 10 outcomes at 0.7 confidence, 7 successes → error ≈ 0.0
        for i in 0..10 {
            cal.record_outcome("a1", 0.7, i < 7);
        }
        let err = cal.calibration_error("a1").unwrap();
        assert!(err < 0.1, "calibration error was {}", err);
        assert!(cal.is_well_calibrated("a1"));
    }

    #[test]
    fn test_calibration_error_poorly_calibrated() {
        let mut cal = TrustCalibrator::new();
        // All predictions at 0.9 but only 20% success.
        for i in 0..10 {
            cal.record_outcome("a1", 0.9, i < 2);
        }
        let err = cal.calibration_error("a1").unwrap();
        assert!(err > 0.5);
        assert!(!cal.is_well_calibrated("a1"));
    }

    #[test]
    fn test_calibration_error_unknown_agent() {
        let cal = TrustCalibrator::new();
        assert!(cal.calibration_error("x").is_none());
        assert!(!cal.is_well_calibrated("x"));
    }

    #[test]
    fn test_trust_trend_insufficient_data() {
        let mut cal = TrustCalibrator::new();
        cal.record_outcome("a1", 0.5, true);
        assert_eq!(cal.trust_trend("a1"), TrustTrend::InsufficientData);
    }

    #[test]
    fn test_trust_trend_improving() {
        let mut cal = TrustCalibrator::new();
        // Start low, get better.
        cal.update_metrics("a1", 0.2, 0.2, 0.2, 0.2);
        cal.update_metrics("a1", 0.3, 0.3, 0.3, 0.3);
        cal.update_metrics("a1", 0.4, 0.4, 0.4, 0.4);
        cal.update_metrics("a1", 0.6, 0.6, 0.6, 0.6);
        cal.update_metrics("a1", 0.8, 0.8, 0.8, 0.8);
        cal.update_metrics("a1", 0.95, 0.95, 0.95, 0.95);
        assert_eq!(cal.trust_trend("a1"), TrustTrend::Improving);
    }

    #[test]
    fn test_trust_trend_declining() {
        let mut cal = TrustCalibrator::new();
        cal.update_metrics("a1", 0.95, 0.95, 0.95, 0.95);
        cal.update_metrics("a1", 0.9, 0.9, 0.9, 0.9);
        cal.update_metrics("a1", 0.85, 0.85, 0.85, 0.85);
        cal.update_metrics("a1", 0.4, 0.4, 0.4, 0.4);
        cal.update_metrics("a1", 0.2, 0.2, 0.2, 0.2);
        cal.update_metrics("a1", 0.1, 0.1, 0.1, 0.1);
        assert_eq!(cal.trust_trend("a1"), TrustTrend::Declining);
    }

    #[test]
    fn test_trust_trend_stable() {
        let mut cal = TrustCalibrator::new();
        for _ in 0..6 {
            cal.update_metrics("a1", 0.7, 0.7, 0.7, 0.7);
        }
        assert_eq!(cal.trust_trend("a1"), TrustTrend::Stable);
    }

    #[test]
    fn test_recommend_mode_unknown_agent() {
        let cal = TrustCalibrator::new();
        assert_eq!(cal.recommend_mode("x"), CollaborationMode::Supervised);
    }

    #[test]
    fn test_recommend_mode_high_trust() {
        let mut cal = TrustCalibrator::new();
        // Apply EMA repeatedly to converge toward high trust.
        for _ in 0..20 {
            cal.update_metrics("a1", 0.99, 0.99, 0.99, 0.99);
        }
        assert_eq!(cal.recommend_mode("a1"), CollaborationMode::FullAutonomy);
    }

    #[test]
    fn test_recommend_mode_medium_trust() {
        let mut cal = TrustCalibrator::new();
        // Converge toward ~0.75 overall trust.
        for _ in 0..20 {
            cal.update_metrics("a1", 0.75, 0.75, 0.75, 0.75);
        }
        assert_eq!(cal.recommend_mode("a1"), CollaborationMode::Paired);
    }

    #[test]
    fn test_recommend_mode_low_trust() {
        let mut cal = TrustCalibrator::new();
        // Converge toward very low trust.
        for _ in 0..20 {
            cal.update_metrics("a1", 0.1, 0.1, 0.1, 0.1);
        }
        assert_eq!(cal.recommend_mode("a1"), CollaborationMode::HumanLed);
    }

    // -- Cognitive Load Management ------------------------------------------

    #[test]
    fn test_cognitive_load_update_and_get() {
        let mut mgr = LoadManager::new();
        let load = CognitiveLoad {
            human_id: "h1".into(),
            current_tasks: 5,
            pending_decisions: 3,
            interruption_count: 2,
            time_since_break_mins: 60,
            estimated_load: 0.7,
        };
        mgr.update_load("h1", load);
        let l = mgr.get_load("h1").unwrap();
        assert_eq!(l.current_tasks, 5);
    }

    #[test]
    fn test_cognitive_load_unknown_human() {
        let mgr = LoadManager::new();
        assert!(mgr.get_load("nobody").is_none());
        assert!(!mgr.is_overloaded("nobody"));
    }

    #[test]
    fn test_overloaded_true() {
        let mut mgr = LoadManager::new();
        mgr.update_load(
            "h1",
            CognitiveLoad {
                human_id: "h1".into(),
                current_tasks: 10,
                pending_decisions: 8,
                interruption_count: 5,
                time_since_break_mins: 120,
                estimated_load: 0.95,
            },
        );
        assert!(mgr.is_overloaded("h1"));
    }

    #[test]
    fn test_overloaded_false() {
        let mut mgr = LoadManager::new();
        mgr.update_load(
            "h1",
            CognitiveLoad {
                human_id: "h1".into(),
                current_tasks: 2,
                pending_decisions: 1,
                interruption_count: 0,
                time_since_break_mins: 30,
                estimated_load: 0.3,
            },
        );
        assert!(!mgr.is_overloaded("h1"));
    }

    #[test]
    fn test_should_defer_when_overloaded() {
        let mut mgr = LoadManager::new();
        mgr.update_load(
            "h1",
            CognitiveLoad {
                human_id: "h1".into(),
                current_tasks: 10,
                pending_decisions: 8,
                interruption_count: 5,
                time_since_break_mins: 120,
                estimated_load: 0.9,
            },
        );
        assert!(mgr.should_defer("h1"));
    }

    #[test]
    fn test_should_not_defer_when_light() {
        let mut mgr = LoadManager::new();
        mgr.update_load(
            "h1",
            CognitiveLoad {
                human_id: "h1".into(),
                current_tasks: 1,
                pending_decisions: 0,
                interruption_count: 0,
                time_since_break_mins: 10,
                estimated_load: 0.2,
            },
        );
        assert!(!mgr.should_defer("h1"));
    }

    #[test]
    fn test_suggest_break_true() {
        let mut mgr = LoadManager::new();
        mgr.update_load(
            "h1",
            CognitiveLoad {
                human_id: "h1".into(),
                current_tasks: 5,
                pending_decisions: 3,
                interruption_count: 4,
                time_since_break_mins: 100,
                estimated_load: 0.7,
            },
        );
        assert!(mgr.suggest_break("h1"));
    }

    #[test]
    fn test_suggest_break_false_short_time() {
        let mut mgr = LoadManager::new();
        mgr.update_load(
            "h1",
            CognitiveLoad {
                human_id: "h1".into(),
                current_tasks: 5,
                pending_decisions: 3,
                interruption_count: 4,
                time_since_break_mins: 30,
                estimated_load: 0.7,
            },
        );
        assert!(!mgr.suggest_break("h1"));
    }

    #[test]
    fn test_suggest_break_false_low_load() {
        let mut mgr = LoadManager::new();
        mgr.update_load(
            "h1",
            CognitiveLoad {
                human_id: "h1".into(),
                current_tasks: 1,
                pending_decisions: 0,
                interruption_count: 0,
                time_since_break_mins: 120,
                estimated_load: 0.3,
            },
        );
        assert!(!mgr.suggest_break("h1"));
    }

    #[test]
    fn test_optimal_batch_size_low_load() {
        let mut mgr = LoadManager::new();
        mgr.update_load(
            "h1",
            CognitiveLoad {
                human_id: "h1".into(),
                current_tasks: 1,
                pending_decisions: 0,
                interruption_count: 0,
                time_since_break_mins: 10,
                estimated_load: 0.2,
            },
        );
        assert_eq!(mgr.optimal_batch_size("h1"), 5);
    }

    #[test]
    fn test_optimal_batch_size_high_load() {
        let mut mgr = LoadManager::new();
        mgr.update_load(
            "h1",
            CognitiveLoad {
                human_id: "h1".into(),
                current_tasks: 10,
                pending_decisions: 8,
                interruption_count: 6,
                time_since_break_mins: 150,
                estimated_load: 0.9,
            },
        );
        assert_eq!(mgr.optimal_batch_size("h1"), 1);
    }

    #[test]
    fn test_optimal_batch_size_unknown_human() {
        let mgr = LoadManager::new();
        assert_eq!(mgr.optimal_batch_size("nobody"), 5);
    }

    #[test]
    fn test_optimal_batch_size_medium_load() {
        let mut mgr = LoadManager::new();
        mgr.update_load(
            "h1",
            CognitiveLoad {
                human_id: "h1".into(),
                current_tasks: 4,
                pending_decisions: 2,
                interruption_count: 1,
                time_since_break_mins: 50,
                estimated_load: 0.5,
            },
        );
        assert_eq!(mgr.optimal_batch_size("h1"), 3);
    }

    // -- Feedback Loop ------------------------------------------------------

    fn make_feedback(agent_id: &str, ft: FeedbackType) -> Feedback {
        Feedback {
            feedback_id: Uuid::new_v4(),
            session_id: "sess-1".into(),
            agent_id: agent_id.into(),
            feedback_type: ft,
            content: "test feedback".into(),
            timestamp: Utc::now(),
            applied: false,
        }
    }

    #[test]
    fn test_record_feedback_ok() {
        let mut col = FeedbackCollector::new();
        let fb = make_feedback("a1", FeedbackType::Praise);
        assert!(col.record_feedback(fb).is_ok());
    }

    #[test]
    fn test_record_feedback_valid_rating() {
        let mut col = FeedbackCollector::new();
        let fb = make_feedback("a1", FeedbackType::Rating(5));
        assert!(col.record_feedback(fb).is_ok());
    }

    #[test]
    fn test_record_feedback_invalid_rating_zero() {
        let mut col = FeedbackCollector::new();
        let fb = make_feedback("a1", FeedbackType::Rating(0));
        assert!(col.record_feedback(fb).is_err());
    }

    #[test]
    fn test_record_feedback_invalid_rating_six() {
        let mut col = FeedbackCollector::new();
        let fb = make_feedback("a1", FeedbackType::Rating(6));
        assert!(col.record_feedback(fb).is_err());
    }

    #[test]
    fn test_feedback_for_agent() {
        let mut col = FeedbackCollector::new();
        col.record_feedback(make_feedback("a1", FeedbackType::Praise)).unwrap();
        col.record_feedback(make_feedback("a2", FeedbackType::Suggestion)).unwrap();
        col.record_feedback(make_feedback("a1", FeedbackType::Correction)).unwrap();
        assert_eq!(col.feedback_for_agent("a1").len(), 2);
        assert_eq!(col.feedback_for_agent("a2").len(), 1);
        assert_eq!(col.feedback_for_agent("a3").len(), 0);
    }

    #[test]
    fn test_feedback_for_session() {
        let mut col = FeedbackCollector::new();
        col.record_feedback(make_feedback("a1", FeedbackType::Praise)).unwrap();
        assert_eq!(col.feedback_for_session("sess-1").len(), 1);
        assert_eq!(col.feedback_for_session("other").len(), 0);
    }

    #[test]
    fn test_mark_applied() {
        let mut col = FeedbackCollector::new();
        let fb = make_feedback("a1", FeedbackType::Correction);
        let id = fb.feedback_id;
        col.record_feedback(fb).unwrap();
        assert!(col.mark_applied(id).is_ok());
        assert!(col.feedback_for_agent("a1")[0].applied);
    }

    #[test]
    fn test_mark_applied_unknown_id() {
        let mut col = FeedbackCollector::new();
        assert!(col.mark_applied(Uuid::new_v4()).is_err());
    }

    #[test]
    fn test_feedback_stats() {
        let mut col = FeedbackCollector::new();
        col.record_feedback(make_feedback("a1", FeedbackType::Correction)).unwrap();
        col.record_feedback(make_feedback("a1", FeedbackType::Rating(4))).unwrap();
        col.record_feedback(make_feedback("a1", FeedbackType::Rating(2))).unwrap();
        col.record_feedback(make_feedback("a1", FeedbackType::Praise)).unwrap();

        // Mark one as applied.
        let first_id = col.feedback_for_agent("a1")[0].feedback_id;
        col.mark_applied(first_id).unwrap();

        let stats = col.feedback_stats("a1");
        assert_eq!(stats.total, 4);
        assert_eq!(stats.applied_count, 1);
        assert!((stats.avg_rating - 3.0).abs() < 1e-9);
        assert!((stats.application_rate - 0.25).abs() < 1e-9);
        assert_eq!(*stats.by_type.get("Correction").unwrap(), 1);
        assert_eq!(*stats.by_type.get("Rating").unwrap(), 2);
        assert_eq!(*stats.by_type.get("Praise").unwrap(), 1);
    }

    #[test]
    fn test_feedback_stats_empty() {
        let col = FeedbackCollector::new();
        let stats = col.feedback_stats("nobody");
        assert_eq!(stats.total, 0);
        assert_eq!(stats.avg_rating, 0.0);
        assert_eq!(stats.application_rate, 0.0);
    }

    #[test]
    fn test_unapplied_corrections() {
        let mut col = FeedbackCollector::new();
        let c1 = make_feedback("a1", FeedbackType::Correction);
        let c1_id = c1.feedback_id;
        col.record_feedback(c1).unwrap();
        col.record_feedback(make_feedback("a1", FeedbackType::Correction)).unwrap();
        col.record_feedback(make_feedback("a1", FeedbackType::Praise)).unwrap();

        assert_eq!(col.unapplied_corrections("a1").len(), 2);
        col.mark_applied(c1_id).unwrap();
        assert_eq!(col.unapplied_corrections("a1").len(), 1);
    }

    // -- Collaboration Analytics --------------------------------------------

    #[test]
    fn test_analyze_session_basic() {
        let mut session = CollaborationSession::new(
            "h".into(),
            "a".into(),
            CollaborationMode::Paired,
        );
        session.tasks_completed = 3;
        session.handoffs = 1;
        session.end();

        let mut t1 = SharedTask::new("t1".into(), "d".into(), TaskOwner::Shared);
        t1.human_contribution = 0.6;
        t1.agent_contribution = 0.4;
        t1.status = SharedTaskStatus::Complete;

        let analytics = CollaborationAnalyzer::analyze_session(&session, &[t1], &[]);
        assert!(analytics.total_duration_mins >= 0.0);
        assert!(analytics.mode_effectiveness > 0.0);
    }

    #[test]
    fn test_analyze_session_no_tasks() {
        let mut session = CollaborationSession::new(
            "h".into(),
            "a".into(),
            CollaborationMode::Supervised,
        );
        session.end();

        let analytics = CollaborationAnalyzer::analyze_session(&session, &[], &[]);
        assert_eq!(analytics.mode_effectiveness, 0.0);
    }

    #[test]
    fn test_analyze_session_zero_duration() {
        let mut session = CollaborationSession::new(
            "h".into(),
            "a".into(),
            CollaborationMode::FullAutonomy,
        );
        // End immediately — duration might be 0.
        session.ended_at = Some(session.started_at);

        let analytics = CollaborationAnalyzer::analyze_session(&session, &[], &[]);
        assert_eq!(analytics.efficiency_score, 0.0);
        assert_eq!(analytics.handoff_overhead_percent, 0.0);
    }

    #[test]
    fn test_best_mode_for_task_type() {
        let data: Vec<(&str, CollaborationMode, f64)> = vec![
            ("code review refactor", CollaborationMode::Paired, 0.9),
            ("code deployment", CollaborationMode::Supervised, 0.7),
            ("review documentation", CollaborationMode::HumanLed, 0.5),
        ];
        let best = CollaborationAnalyzer::best_mode_for_task_type(&data);
        // "code" appears in two entries; Paired has higher effectiveness.
        assert_eq!(*best.get("code").unwrap(), CollaborationMode::Paired);
        // "review" also appears twice; Paired (0.9) > HumanLed (0.5).
        assert_eq!(*best.get("review").unwrap(), CollaborationMode::Paired);
    }

    #[test]
    fn test_best_mode_empty_input() {
        let best = CollaborationAnalyzer::best_mode_for_task_type(&[]);
        assert!(best.is_empty());
    }

    #[test]
    fn test_overall_stats_basic() {
        let a1 = SessionAnalytics {
            session_id: Uuid::new_v4(),
            total_duration_mins: 60.0,
            human_active_mins: 30.0,
            agent_active_mins: 30.0,
            efficiency_score: 0.1,
            handoff_overhead_percent: 5.0,
            mode_effectiveness: 0.8,
            tasks_completed: 6,
            mode: CollaborationMode::Paired,
        };
        let a2 = SessionAnalytics {
            session_id: Uuid::new_v4(),
            total_duration_mins: 120.0,
            human_active_mins: 80.0,
            agent_active_mins: 40.0,
            efficiency_score: 0.05,
            handoff_overhead_percent: 10.0,
            mode_effectiveness: 0.6,
            tasks_completed: 6,
            mode: CollaborationMode::Supervised,
        };
        let stats = CollaborationAnalyzer::overall_stats(&[a1, a2]);
        assert_eq!(stats.total_sessions, 2);
        assert!((stats.avg_efficiency - 0.075).abs() < 1e-9);
        assert!((stats.avg_handoff_overhead - 7.5).abs() < 1e-9);
        assert!((stats.total_human_hours - (110.0 / 60.0)).abs() < 1e-9);
        assert!((stats.total_agent_hours - (70.0 / 60.0)).abs() < 1e-9);
    }

    #[test]
    fn test_overall_stats_empty() {
        let stats = CollaborationAnalyzer::overall_stats(&[]);
        assert_eq!(stats.total_sessions, 0);
        assert_eq!(stats.total_tasks, 0);
        assert_eq!(stats.avg_efficiency, 0.0);
        assert!(stats.most_effective_mode.is_none());
    }

    // -- Edge cases ---------------------------------------------------------

    #[test]
    fn test_handoff_party_display() {
        assert_eq!(HandoffParty::Human.to_string(), "Human");
        assert_eq!(HandoffParty::Agent.to_string(), "Agent");
    }

    #[test]
    fn test_all_modes_serialize_roundtrip() {
        let modes = vec![
            CollaborationMode::FullAutonomy,
            CollaborationMode::Supervised,
            CollaborationMode::Paired,
            CollaborationMode::HumanLed,
            CollaborationMode::TeachingMode,
        ];
        for mode in modes {
            let json = serde_json::to_string(&mode).unwrap();
            let back: CollaborationMode = serde_json::from_str(&json).unwrap();
            assert_eq!(mode, back);
        }
    }

    #[test]
    fn test_shared_task_serialize_roundtrip() {
        let t = SharedTask::new("title".into(), "desc".into(), TaskOwner::Human);
        let json = serde_json::to_string(&t).unwrap();
        let back: SharedTask = serde_json::from_str(&json).unwrap();
        assert_eq!(back.title, "title");
        assert_eq!(back.owner, TaskOwner::Human);
    }

    #[test]
    fn test_trust_metrics_serialize() {
        let m = TrustMetrics::new("agent-x".into());
        let json = serde_json::to_string(&m).unwrap();
        assert!(json.contains("agent-x"));
    }

    #[test]
    fn test_feedback_stats_serialize() {
        let stats = FeedbackStats {
            total: 0,
            by_type: HashMap::new(),
            avg_rating: 0.0,
            applied_count: 0,
            application_rate: 0.0,
        };
        let json = serde_json::to_string(&stats).unwrap();
        assert!(json.contains("\"total\":0"));
    }
}
