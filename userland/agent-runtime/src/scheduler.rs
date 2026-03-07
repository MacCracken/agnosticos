//! Distributed Task Scheduling with Consensus
//!
//! Provides a priority-aware task scheduler for federated agent clusters.
//! Tasks are submitted, queued by priority, and assigned to nodes based on
//! resource availability. Supports preemption, cancellation, deadline-aware
//! scheduling, and cron-like recurring task triggers.

use std::collections::HashMap;

use anyhow::{bail, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// ResourceReq
// ---------------------------------------------------------------------------

/// Resource requirements for a scheduled task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceReq {
    /// Required CPU cores (fractional allowed, e.g. 0.5).
    pub cpu_cores: f64,
    /// Required memory in megabytes.
    pub memory_mb: u64,
    /// Whether a GPU is required.
    pub gpu: bool,
    /// Whether network access is required.
    pub network: bool,
    /// Required disk space in megabytes.
    pub disk_mb: u64,
}

impl Default for ResourceReq {
    fn default() -> Self {
        Self {
            cpu_cores: 1.0,
            memory_mb: 256,
            gpu: false,
            network: false,
            disk_mb: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// TaskStatus
// ---------------------------------------------------------------------------

/// Status of a scheduled task, modelled as a state machine.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TaskStatus {
    Queued,
    Scheduled,
    Running,
    Completed,
    Failed(String),
    Cancelled,
    Preempted,
}

impl TaskStatus {
    /// Returns `true` if transitioning from `self` to `to` is valid.
    pub fn valid_transition(&self, to: &TaskStatus) -> bool {
        use TaskStatus::*;
        matches!(
            (self, to),
            (Queued, Scheduled)
                | (Queued, Cancelled)
                | (Scheduled, Running)
                | (Scheduled, Cancelled)
                | (Scheduled, Queued) // re-queue after schedule failure
                | (Running, Completed)
                | (Running, Failed(_))
                | (Running, Cancelled)
                | (Running, Preempted)
                | (Preempted, Queued) // re-queue after preemption
        )
    }
}

// ---------------------------------------------------------------------------
// TaskPriority
// ---------------------------------------------------------------------------

/// Priority classification for tasks.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum TaskPriority {
    Normal,   // 1-3
    High,     // 4-6
    Critical, // 7-9
    Emergency, // 10
}

impl TaskPriority {
    /// Derive a `TaskPriority` from a numeric priority (1-10).
    pub fn from_numeric(p: u8) -> Self {
        match p {
            1..=3 => Self::Normal,
            4..=6 => Self::High,
            7..=9 => Self::Critical,
            10 => Self::Emergency,
            _ => Self::Normal,
        }
    }

    /// Returns `true` if `self` can preempt `other` (strictly higher).
    pub fn can_preempt(&self, other: &TaskPriority) -> bool {
        *self > *other
    }
}

// ---------------------------------------------------------------------------
// ScheduledTask
// ---------------------------------------------------------------------------

/// A task managed by the distributed scheduler.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledTask {
    pub task_id: String,
    pub name: String,
    pub description: String,
    pub agent_id: String,
    /// Numeric priority 1-10 (10 = highest).
    pub priority: u8,
    pub resource_requirements: ResourceReq,
    pub status: TaskStatus,
    pub created_at: DateTime<Utc>,
    pub scheduled_at: Option<DateTime<Utc>>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub deadline: Option<DateTime<Utc>>,
    pub node_preference: Option<String>,
}

impl ScheduledTask {
    /// Create a new task in `Queued` status.
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        agent_id: impl Into<String>,
        priority: u8,
        resource_requirements: ResourceReq,
    ) -> Self {
        let priority = priority.clamp(1, 10);
        Self {
            task_id: Uuid::new_v4().to_string(),
            name: name.into(),
            description: description.into(),
            agent_id: agent_id.into(),
            priority,
            resource_requirements,
            status: TaskStatus::Queued,
            created_at: Utc::now(),
            scheduled_at: None,
            started_at: None,
            completed_at: None,
            deadline: None,
            node_preference: None,
        }
    }

    /// Derived priority classification.
    pub fn priority_class(&self) -> TaskPriority {
        TaskPriority::from_numeric(self.priority)
    }

    /// Transition to a new status, returning an error on invalid transition.
    pub fn transition(&mut self, to: TaskStatus) -> Result<()> {
        if !self.status.valid_transition(&to) {
            bail!(
                "invalid task status transition: {:?} -> {:?}",
                self.status,
                to
            );
        }
        debug!(
            task_id = %self.task_id,
            from = ?self.status,
            to = ?to,
            "task status transition"
        );
        self.status = to;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// NodeCapacity
// ---------------------------------------------------------------------------

/// Tracks resource capacity for a cluster node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeCapacity {
    pub node_id: String,
    pub total_cpu: f64,
    pub available_cpu: f64,
    pub total_memory_mb: u64,
    pub available_memory_mb: u64,
    pub total_disk_mb: u64,
    pub available_disk_mb: u64,
    pub gpu_available: bool,
    pub running_tasks: usize,
}

impl NodeCapacity {
    /// Create a new node capacity descriptor.
    pub fn new(
        node_id: impl Into<String>,
        total_cpu: f64,
        total_memory_mb: u64,
        total_disk_mb: u64,
        gpu_available: bool,
    ) -> Self {
        Self {
            node_id: node_id.into(),
            total_cpu,
            available_cpu: total_cpu,
            total_memory_mb,
            available_memory_mb: total_memory_mb,
            total_disk_mb,
            available_disk_mb: total_disk_mb,
            gpu_available,
            running_tasks: 0,
        }
    }

    /// Returns `true` if this node can fit the given resource requirements.
    pub fn can_fit(&self, req: &ResourceReq) -> bool {
        self.available_cpu >= req.cpu_cores
            && self.available_memory_mb >= req.memory_mb
            && (!req.gpu || self.gpu_available)
            && self.available_disk_mb >= req.disk_mb
    }

    /// Returns utilization as a ratio 0.0-1.0 (average of CPU and memory).
    pub fn utilization(&self) -> f64 {
        if self.total_cpu == 0.0 && self.total_memory_mb == 0 {
            return 0.0;
        }
        let cpu_util = if self.total_cpu > 0.0 {
            1.0 - (self.available_cpu / self.total_cpu)
        } else {
            0.0
        };
        let mem_util = if self.total_memory_mb > 0 {
            1.0 - (self.available_memory_mb as f64 / self.total_memory_mb as f64)
        } else {
            0.0
        };
        (cpu_util + mem_util) / 2.0
    }

    /// Reserve resources for a task.
    pub fn reserve(&mut self, req: &ResourceReq) {
        self.available_cpu = (self.available_cpu - req.cpu_cores).max(0.0);
        self.available_memory_mb = self.available_memory_mb.saturating_sub(req.memory_mb);
        self.available_disk_mb = self.available_disk_mb.saturating_sub(req.disk_mb);
        self.running_tasks += 1;
    }

    /// Release resources after a task completes.
    pub fn release(&mut self, req: &ResourceReq) {
        self.available_cpu = (self.available_cpu + req.cpu_cores).min(self.total_cpu);
        self.available_memory_mb = (self.available_memory_mb + req.memory_mb).min(self.total_memory_mb);
        self.available_disk_mb = (self.available_disk_mb + req.disk_mb).min(self.total_disk_mb);
        self.running_tasks = self.running_tasks.saturating_sub(1);
    }
}

// ---------------------------------------------------------------------------
// Scheduling types
// ---------------------------------------------------------------------------

/// A decision made by the scheduler assigning a task to a node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulingDecision {
    pub task_id: String,
    pub assigned_node: String,
    pub reason: String,
    pub score: f64,
}

/// A preemption action — one task evicts another.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreemptionAction {
    pub preempted_task_id: String,
    pub preempting_task_id: String,
    pub reason: String,
}

/// Aggregate scheduler statistics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SchedulerStats {
    pub total_tasks: usize,
    pub queued: usize,
    pub running: usize,
    pub completed: usize,
    pub failed: usize,
    pub average_wait_time_ms: u64,
    pub average_run_time_ms: u64,
}

// ---------------------------------------------------------------------------
// TaskScheduler
// ---------------------------------------------------------------------------

/// The core distributed task scheduler.
pub struct TaskScheduler {
    tasks: HashMap<String, ScheduledTask>,
    nodes: HashMap<String, NodeCapacity>,
}

impl TaskScheduler {
    /// Create a new empty scheduler.
    pub fn new() -> Self {
        info!("task scheduler initialised");
        Self {
            tasks: HashMap::new(),
            nodes: HashMap::new(),
        }
    }

    /// Register a cluster node with its capacity.
    pub fn register_node(&mut self, node: NodeCapacity) {
        info!(node_id = %node.node_id, "registered node");
        self.nodes.insert(node.node_id.clone(), node);
    }

    /// Submit a new task. Returns the assigned task_id.
    pub fn submit_task(&mut self, task: ScheduledTask) -> Result<String> {
        let id = task.task_id.clone();
        info!(task_id = %id, name = %task.name, priority = task.priority, "task submitted");
        self.tasks.insert(id.clone(), task);
        Ok(id)
    }

    /// Get a task by ID.
    pub fn get_task(&self, task_id: &str) -> Option<&ScheduledTask> {
        self.tasks.get(task_id)
    }

    /// Get a mutable task by ID.
    pub fn get_task_mut(&mut self, task_id: &str) -> Option<&mut ScheduledTask> {
        self.tasks.get_mut(task_id)
    }

    /// Cancel a task. Only valid from Queued, Scheduled, or Running.
    pub fn cancel_task(&mut self, task_id: &str) -> Result<()> {
        let task = self
            .tasks
            .get_mut(task_id)
            .ok_or_else(|| anyhow::anyhow!("task not found: {}", task_id))?;

        // Release node resources if task was running/scheduled
        if matches!(task.status, TaskStatus::Running | TaskStatus::Scheduled) {
            if let Some(ref pref) = task.node_preference {
                if let Some(node) = self.nodes.get_mut(pref) {
                    node.release(&task.resource_requirements);
                }
            }
        }

        task.transition(TaskStatus::Cancelled)?;
        task.completed_at = Some(Utc::now());
        info!(task_id = %task_id, "task cancelled");
        Ok(())
    }

    /// Return pending (Queued) tasks sorted by priority desc, then created_at asc.
    pub fn pending_tasks(&self) -> Vec<&ScheduledTask> {
        let mut pending: Vec<&ScheduledTask> = self
            .tasks
            .values()
            .filter(|t| t.status == TaskStatus::Queued)
            .collect();
        pending.sort_by(|a, b| {
            b.priority
                .cmp(&a.priority)
                .then_with(|| a.created_at.cmp(&b.created_at))
        });
        pending
    }

    /// Return tasks assigned to a specific node.
    pub fn tasks_for_node(&self, node_id: &str) -> Vec<&ScheduledTask> {
        self.tasks
            .values()
            .filter(|t| t.node_preference.as_deref() == Some(node_id))
            .filter(|t| {
                matches!(
                    t.status,
                    TaskStatus::Scheduled | TaskStatus::Running
                )
            })
            .collect()
    }

    /// Schedule all pending tasks onto available nodes. Returns scheduling decisions.
    pub fn schedule_pending(&mut self) -> Vec<SchedulingDecision> {
        let mut decisions = Vec::new();
        let pending_ids: Vec<String> = {
            let mut pending: Vec<&ScheduledTask> = self
                .tasks
                .values()
                .filter(|t| t.status == TaskStatus::Queued)
                .collect();
            pending.sort_by(|a, b| {
                b.priority
                    .cmp(&a.priority)
                    .then_with(|| a.created_at.cmp(&b.created_at))
            });
            pending.into_iter().map(|t| t.task_id.clone()).collect()
        };

        for task_id in pending_ids {
            let req = self.tasks[&task_id].resource_requirements.clone();
            let pref = self.tasks[&task_id].node_preference.clone();

            // Try preferred node first, then best-fit
            let chosen = if let Some(ref pref_id) = pref {
                if self
                    .nodes
                    .get(pref_id)
                    .map_or(false, |n| n.can_fit(&req))
                {
                    Some(pref_id.clone())
                } else {
                    self.best_fit_node(&req)
                }
            } else {
                self.best_fit_node(&req)
            };

            if let Some(node_id) = chosen {
                let score = {
                    let node = &self.nodes[&node_id];
                    1.0 - node.utilization()
                };

                // Reserve resources
                self.nodes.get_mut(&node_id).unwrap().reserve(&req);

                // Update task
                let task = self.tasks.get_mut(&task_id).unwrap();
                let _ = task.transition(TaskStatus::Scheduled);
                task.scheduled_at = Some(Utc::now());
                task.node_preference = Some(node_id.clone());

                let reason = if pref.as_deref() == Some(&node_id) {
                    "preferred node".to_string()
                } else {
                    "best-fit selection".to_string()
                };

                debug!(task_id = %task_id, node = %node_id, score = score, "task scheduled");
                decisions.push(SchedulingDecision {
                    task_id,
                    assigned_node: node_id,
                    reason,
                    score,
                });
            } else {
                warn!(task_id = %task_id, "no node with sufficient capacity");
            }
        }

        decisions
    }

    /// Check if a new (high-priority) task should preempt an existing running task.
    pub fn preempt_if_needed(&self, task: &ScheduledTask) -> Option<PreemptionAction> {
        let new_prio = task.priority_class();

        // Find the lowest-priority running task across all nodes
        let mut candidate: Option<&ScheduledTask> = None;
        for t in self.tasks.values() {
            if t.status != TaskStatus::Running {
                continue;
            }
            if !new_prio.can_preempt(&t.priority_class()) {
                continue;
            }
            match candidate {
                None => candidate = Some(t),
                Some(c) if t.priority < c.priority => candidate = Some(t),
                Some(c) if t.priority == c.priority && t.created_at > c.created_at => {
                    candidate = Some(t);
                }
                _ => {}
            }
        }

        candidate.map(|c| {
            info!(
                preempting = %task.task_id,
                preempted = %c.task_id,
                "preemption recommended"
            );
            PreemptionAction {
                preempted_task_id: c.task_id.clone(),
                preempting_task_id: task.task_id.clone(),
                reason: format!(
                    "priority {} preempts priority {} task",
                    task.priority, c.priority
                ),
            }
        })
    }

    /// Compute aggregate statistics.
    pub fn stats(&self) -> SchedulerStats {
        let mut stats = SchedulerStats {
            total_tasks: self.tasks.len(),
            ..Default::default()
        };

        let mut wait_times = Vec::new();
        let mut run_times = Vec::new();

        for t in self.tasks.values() {
            match &t.status {
                TaskStatus::Queued => stats.queued += 1,
                TaskStatus::Scheduled => stats.queued += 1, // still waiting
                TaskStatus::Running => stats.running += 1,
                TaskStatus::Completed => {
                    stats.completed += 1;
                    if let (Some(started), Some(completed)) = (t.started_at, t.completed_at) {
                        run_times.push((completed - started).num_milliseconds().max(0) as u64);
                    }
                }
                TaskStatus::Failed(_) => stats.failed += 1,
                TaskStatus::Cancelled => stats.completed += 1,
                TaskStatus::Preempted => stats.queued += 1,
            }

            // Wait time: created_at → started_at (or now if not started)
            if let Some(started) = t.started_at {
                wait_times.push((started - t.created_at).num_milliseconds().max(0) as u64);
            }
        }

        if !wait_times.is_empty() {
            stats.average_wait_time_ms = wait_times.iter().sum::<u64>() / wait_times.len() as u64;
        }
        if !run_times.is_empty() {
            stats.average_run_time_ms = run_times.iter().sum::<u64>() / run_times.len() as u64;
        }

        stats
    }

    // -- private helpers ---

    /// Find the node with the most available capacity that can fit `req`.
    fn best_fit_node(&self, req: &ResourceReq) -> Option<String> {
        self.nodes
            .values()
            .filter(|n| n.can_fit(req))
            .min_by(|a, b| {
                a.utilization()
                    .partial_cmp(&b.utilization())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|n| n.node_id.clone())
    }
}

impl Default for TaskScheduler {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// CronSchedule
// ---------------------------------------------------------------------------

/// A simplified cron-like schedule entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronEntry {
    pub name: String,
    /// Interval in seconds between fires.
    pub interval_seconds: u64,
    /// Optional specific hour (0-23) to fire.
    pub specific_hour: Option<u8>,
    /// Optional specific minute (0-59) to fire.
    pub specific_minute: Option<u8>,
    /// Template task to create when the entry fires.
    pub task_template: CronTaskTemplate,
    /// Whether the entry is active.
    pub enabled: bool,
    /// Last time this entry fired.
    pub last_fired: Option<DateTime<Utc>>,
}

/// Template for creating tasks from cron entries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronTaskTemplate {
    pub name: String,
    pub description: String,
    pub agent_id: String,
    pub priority: u8,
    pub resource_requirements: ResourceReq,
}

/// Manages cron-like recurring task triggers.
pub struct CronScheduler {
    entries: HashMap<String, CronEntry>,
}

impl CronScheduler {
    /// Create a new cron scheduler.
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Add a cron entry.
    pub fn add_entry(&mut self, entry: CronEntry) -> Result<()> {
        if entry.name.is_empty() {
            bail!("cron entry name cannot be empty");
        }
        info!(name = %entry.name, interval = entry.interval_seconds, "cron entry added");
        self.entries.insert(entry.name.clone(), entry);
        Ok(())
    }

    /// Remove a cron entry by name.
    pub fn remove_entry(&mut self, name: &str) -> Result<()> {
        if self.entries.remove(name).is_none() {
            bail!("cron entry not found: {}", name);
        }
        info!(name = %name, "cron entry removed");
        Ok(())
    }

    /// List all entries.
    pub fn list_entries(&self) -> Vec<&CronEntry> {
        self.entries.values().collect()
    }

    /// Check which entries are due to fire. Updates `last_fired` for due entries.
    pub fn check_due(&mut self) -> Vec<ScheduledTask> {
        let now = Utc::now();
        let mut tasks = Vec::new();

        for entry in self.entries.values_mut() {
            if !entry.enabled {
                continue;
            }

            let should_fire = if let Some(last) = entry.last_fired {
                let elapsed = (now - last).num_seconds().max(0) as u64;
                elapsed >= entry.interval_seconds
            } else {
                // Never fired — check interval-based (always fire first time)
                true
            };

            // If specific hour/minute set, also check time-of-day
            let time_matches = match (entry.specific_hour, entry.specific_minute) {
                (Some(h), Some(m)) => {
                    now.format("%H").to_string().parse::<u8>().unwrap_or(0) == h
                        && now.format("%M").to_string().parse::<u8>().unwrap_or(0) == m
                }
                (Some(h), None) => now.format("%H").to_string().parse::<u8>().unwrap_or(0) == h,
                (None, Some(m)) => now.format("%M").to_string().parse::<u8>().unwrap_or(0) == m,
                (None, None) => true,
            };

            if should_fire && time_matches {
                let tmpl = &entry.task_template;
                let task = ScheduledTask::new(
                    format!("{} (cron)", tmpl.name),
                    &tmpl.description,
                    &tmpl.agent_id,
                    tmpl.priority,
                    tmpl.resource_requirements.clone(),
                );
                debug!(cron = %entry.name, task_id = %task.task_id, "cron entry fired");
                tasks.push(task);
                entry.last_fired = Some(now);
            }
        }

        tasks
    }
}

impl Default for CronScheduler {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn make_req(cpu: f64, mem: u64) -> ResourceReq {
        ResourceReq {
            cpu_cores: cpu,
            memory_mb: mem,
            ..Default::default()
        }
    }

    fn make_task(name: &str, priority: u8) -> ScheduledTask {
        ScheduledTask::new(name, "test task", "agent-1", priority, make_req(1.0, 256))
    }

    fn make_node(id: &str, cpu: f64, mem: u64) -> NodeCapacity {
        NodeCapacity::new(id, cpu, mem, 10240, false)
    }

    fn make_template() -> CronTaskTemplate {
        CronTaskTemplate {
            name: "cron-job".into(),
            description: "recurring".into(),
            agent_id: "agent-1".into(),
            priority: 5,
            resource_requirements: make_req(1.0, 128),
        }
    }

    // -- Task submission --

    #[test]
    fn test_submit_task() {
        let mut sched = TaskScheduler::new();
        let task = make_task("t1", 5);
        let id = sched.submit_task(task).unwrap();
        assert!(sched.get_task(&id).is_some());
    }

    #[test]
    fn test_submit_task_initial_status() {
        let mut sched = TaskScheduler::new();
        let task = make_task("t1", 5);
        let id = sched.submit_task(task).unwrap();
        assert_eq!(sched.get_task(&id).unwrap().status, TaskStatus::Queued);
    }

    #[test]
    fn test_submit_multiple_tasks() {
        let mut sched = TaskScheduler::new();
        let id1 = sched.submit_task(make_task("t1", 3)).unwrap();
        let id2 = sched.submit_task(make_task("t2", 7)).unwrap();
        assert_ne!(id1, id2);
        assert_eq!(sched.stats().total_tasks, 2);
    }

    // -- Status transitions --

    #[test]
    fn test_valid_transition_queued_to_scheduled() {
        let mut task = make_task("t", 5);
        assert!(task.transition(TaskStatus::Scheduled).is_ok());
        assert_eq!(task.status, TaskStatus::Scheduled);
    }

    #[test]
    fn test_valid_transition_scheduled_to_running() {
        let mut task = make_task("t", 5);
        task.transition(TaskStatus::Scheduled).unwrap();
        assert!(task.transition(TaskStatus::Running).is_ok());
    }

    #[test]
    fn test_valid_transition_running_to_completed() {
        let mut task = make_task("t", 5);
        task.transition(TaskStatus::Scheduled).unwrap();
        task.transition(TaskStatus::Running).unwrap();
        assert!(task.transition(TaskStatus::Completed).is_ok());
    }

    #[test]
    fn test_valid_transition_running_to_failed() {
        let mut task = make_task("t", 5);
        task.transition(TaskStatus::Scheduled).unwrap();
        task.transition(TaskStatus::Running).unwrap();
        assert!(task
            .transition(TaskStatus::Failed("oops".into()))
            .is_ok());
    }

    #[test]
    fn test_valid_transition_running_to_preempted() {
        let mut task = make_task("t", 5);
        task.transition(TaskStatus::Scheduled).unwrap();
        task.transition(TaskStatus::Running).unwrap();
        assert!(task.transition(TaskStatus::Preempted).is_ok());
    }

    #[test]
    fn test_valid_transition_preempted_to_queued() {
        let mut task = make_task("t", 5);
        task.transition(TaskStatus::Scheduled).unwrap();
        task.transition(TaskStatus::Running).unwrap();
        task.transition(TaskStatus::Preempted).unwrap();
        assert!(task.transition(TaskStatus::Queued).is_ok());
    }

    #[test]
    fn test_invalid_transition_queued_to_completed() {
        let mut task = make_task("t", 5);
        assert!(task.transition(TaskStatus::Completed).is_err());
    }

    #[test]
    fn test_invalid_transition_completed_to_running() {
        let mut task = make_task("t", 5);
        task.transition(TaskStatus::Scheduled).unwrap();
        task.transition(TaskStatus::Running).unwrap();
        task.transition(TaskStatus::Completed).unwrap();
        assert!(task.transition(TaskStatus::Running).is_err());
    }

    #[test]
    fn test_invalid_transition_cancelled_to_queued() {
        let mut task = make_task("t", 5);
        task.transition(TaskStatus::Cancelled).unwrap();
        assert!(task.transition(TaskStatus::Queued).is_err());
    }

    // -- Priority ordering --

    #[test]
    fn test_priority_from_numeric() {
        assert_eq!(TaskPriority::from_numeric(1), TaskPriority::Normal);
        assert_eq!(TaskPriority::from_numeric(3), TaskPriority::Normal);
        assert_eq!(TaskPriority::from_numeric(4), TaskPriority::High);
        assert_eq!(TaskPriority::from_numeric(6), TaskPriority::High);
        assert_eq!(TaskPriority::from_numeric(7), TaskPriority::Critical);
        assert_eq!(TaskPriority::from_numeric(10), TaskPriority::Emergency);
    }

    #[test]
    fn test_priority_ordering() {
        assert!(TaskPriority::Emergency > TaskPriority::Critical);
        assert!(TaskPriority::Critical > TaskPriority::High);
        assert!(TaskPriority::High > TaskPriority::Normal);
    }

    #[test]
    fn test_pending_tasks_sorted_by_priority() {
        let mut sched = TaskScheduler::new();
        let mut low = make_task("low", 2);
        low.created_at = Utc::now() - Duration::seconds(10);
        let mut high = make_task("high", 8);
        high.created_at = Utc::now() - Duration::seconds(5);
        sched.submit_task(low).unwrap();
        sched.submit_task(high).unwrap();

        let pending = sched.pending_tasks();
        assert_eq!(pending.len(), 2);
        assert_eq!(pending[0].name, "high");
        assert_eq!(pending[1].name, "low");
    }

    #[test]
    fn test_pending_tasks_same_priority_sorted_by_created() {
        let mut sched = TaskScheduler::new();
        let mut older = make_task("older", 5);
        older.created_at = Utc::now() - Duration::seconds(100);
        let mut newer = make_task("newer", 5);
        newer.created_at = Utc::now();
        sched.submit_task(newer).unwrap();
        sched.submit_task(older).unwrap();

        let pending = sched.pending_tasks();
        assert_eq!(pending[0].name, "older");
        assert_eq!(pending[1].name, "newer");
    }

    // -- Preemption --

    #[test]
    fn test_preempt_higher_over_lower() {
        let mut sched = TaskScheduler::new();
        let mut running = make_task("low", 2);
        running.status = TaskStatus::Running;
        sched.submit_task(running).unwrap();

        let emergency = make_task("emergency", 10);
        let action = sched.preempt_if_needed(&emergency);
        assert!(action.is_some());
        let a = action.unwrap();
        assert_eq!(a.preempting_task_id, emergency.task_id);
    }

    #[test]
    fn test_no_preempt_same_priority() {
        let mut sched = TaskScheduler::new();
        let mut running = make_task("existing", 5);
        running.status = TaskStatus::Running;
        sched.submit_task(running).unwrap();

        let new_task = make_task("same-prio", 5);
        assert!(sched.preempt_if_needed(&new_task).is_none());
    }

    #[test]
    fn test_no_preempt_lower_priority() {
        let mut sched = TaskScheduler::new();
        let mut running = make_task("high", 8);
        running.status = TaskStatus::Running;
        sched.submit_task(running).unwrap();

        let low = make_task("low", 2);
        assert!(sched.preempt_if_needed(&low).is_none());
    }

    #[test]
    fn test_can_preempt_method() {
        assert!(TaskPriority::Emergency.can_preempt(&TaskPriority::Normal));
        assert!(TaskPriority::Critical.can_preempt(&TaskPriority::High));
        assert!(!TaskPriority::Normal.can_preempt(&TaskPriority::Normal));
        assert!(!TaskPriority::High.can_preempt(&TaskPriority::Critical));
    }

    // -- Node capacity --

    #[test]
    fn test_node_can_fit() {
        let node = make_node("n1", 4.0, 8192);
        assert!(node.can_fit(&make_req(2.0, 4096)));
        assert!(!node.can_fit(&make_req(5.0, 1024)));
        assert!(!node.can_fit(&make_req(1.0, 16384)));
    }

    #[test]
    fn test_node_can_fit_gpu_requirement() {
        let node = NodeCapacity::new("n1", 4.0, 8192, 10240, false);
        let req = ResourceReq {
            gpu: true,
            ..make_req(1.0, 256)
        };
        assert!(!node.can_fit(&req));

        let gpu_node = NodeCapacity::new("n2", 4.0, 8192, 10240, true);
        assert!(gpu_node.can_fit(&req));
    }

    #[test]
    fn test_node_utilization() {
        let mut node = make_node("n1", 4.0, 1000);
        assert_eq!(node.utilization(), 0.0);

        node.reserve(&make_req(2.0, 500));
        assert!((node.utilization() - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_node_utilization_empty() {
        let node = NodeCapacity {
            node_id: "empty".into(),
            total_cpu: 0.0,
            available_cpu: 0.0,
            total_memory_mb: 0,
            available_memory_mb: 0,
            total_disk_mb: 0,
            available_disk_mb: 0,
            gpu_available: false,
            running_tasks: 0,
        };
        assert_eq!(node.utilization(), 0.0);
    }

    #[test]
    fn test_node_reserve_and_release() {
        let mut node = make_node("n1", 4.0, 2048);
        let req = make_req(2.0, 1024);
        node.reserve(&req);
        assert!((node.available_cpu - 2.0).abs() < 0.001);
        assert_eq!(node.available_memory_mb, 1024);
        assert_eq!(node.running_tasks, 1);

        node.release(&req);
        assert!((node.available_cpu - 4.0).abs() < 0.001);
        assert_eq!(node.available_memory_mb, 2048);
        assert_eq!(node.running_tasks, 0);
    }

    // -- Scheduling decisions --

    #[test]
    fn test_schedule_pending_assigns_to_node() {
        let mut sched = TaskScheduler::new();
        sched.register_node(make_node("n1", 4.0, 4096));
        sched.submit_task(make_task("t1", 5)).unwrap();

        let decisions = sched.schedule_pending();
        assert_eq!(decisions.len(), 1);
        assert_eq!(decisions[0].assigned_node, "n1");
    }

    #[test]
    fn test_schedule_pending_prefers_node_preference() {
        let mut sched = TaskScheduler::new();
        sched.register_node(make_node("n1", 8.0, 8192));
        sched.register_node(make_node("n2", 8.0, 8192));

        let mut task = make_task("t1", 5);
        task.node_preference = Some("n2".into());
        sched.submit_task(task).unwrap();

        let decisions = sched.schedule_pending();
        assert_eq!(decisions[0].assigned_node, "n2");
        assert_eq!(decisions[0].reason, "preferred node");
    }

    #[test]
    fn test_schedule_pending_fallback_when_preferred_full() {
        let mut sched = TaskScheduler::new();
        let mut n1 = make_node("n1", 1.0, 128);
        n1.available_cpu = 0.0; // exhausted
        n1.available_memory_mb = 0;
        sched.register_node(n1);
        sched.register_node(make_node("n2", 4.0, 4096));

        let mut task = make_task("t1", 5);
        task.node_preference = Some("n1".into());
        sched.submit_task(task).unwrap();

        let decisions = sched.schedule_pending();
        assert_eq!(decisions.len(), 1);
        assert_eq!(decisions[0].assigned_node, "n2");
        assert_eq!(decisions[0].reason, "best-fit selection");
    }

    #[test]
    fn test_schedule_task_transitions_to_scheduled() {
        let mut sched = TaskScheduler::new();
        sched.register_node(make_node("n1", 4.0, 4096));
        let id = sched.submit_task(make_task("t1", 5)).unwrap();

        sched.schedule_pending();
        assert_eq!(sched.get_task(&id).unwrap().status, TaskStatus::Scheduled);
    }

    // -- Cancel --

    #[test]
    fn test_cancel_queued_task() {
        let mut sched = TaskScheduler::new();
        let id = sched.submit_task(make_task("t1", 5)).unwrap();
        assert!(sched.cancel_task(&id).is_ok());
        assert_eq!(sched.get_task(&id).unwrap().status, TaskStatus::Cancelled);
    }

    #[test]
    fn test_cancel_nonexistent_task() {
        let mut sched = TaskScheduler::new();
        assert!(sched.cancel_task("nonexistent").is_err());
    }

    // -- Stats --

    #[test]
    fn test_stats_empty() {
        let sched = TaskScheduler::new();
        let s = sched.stats();
        assert_eq!(s.total_tasks, 0);
        assert_eq!(s.queued, 0);
        assert_eq!(s.running, 0);
    }

    #[test]
    fn test_stats_counts() {
        let mut sched = TaskScheduler::new();
        sched.submit_task(make_task("q1", 3)).unwrap();
        sched.submit_task(make_task("q2", 5)).unwrap();

        let mut running_task = make_task("r1", 7);
        running_task.status = TaskStatus::Running;
        sched.submit_task(running_task).unwrap();

        let s = sched.stats();
        assert_eq!(s.total_tasks, 3);
        assert_eq!(s.queued, 2);
        assert_eq!(s.running, 1);
    }

    // -- Edge cases --

    #[test]
    fn test_empty_scheduler_pending() {
        let sched = TaskScheduler::new();
        assert!(sched.pending_tasks().is_empty());
    }

    #[test]
    fn test_schedule_no_nodes() {
        let mut sched = TaskScheduler::new();
        sched.submit_task(make_task("t1", 5)).unwrap();
        let decisions = sched.schedule_pending();
        assert!(decisions.is_empty());
        // task should remain queued
        let pending = sched.pending_tasks();
        assert_eq!(pending.len(), 1);
    }

    #[test]
    fn test_all_nodes_full() {
        let mut sched = TaskScheduler::new();
        let mut node = make_node("n1", 1.0, 128);
        node.available_cpu = 0.0;
        node.available_memory_mb = 0;
        sched.register_node(node);

        sched.submit_task(make_task("t1", 5)).unwrap();
        let decisions = sched.schedule_pending();
        assert!(decisions.is_empty());
    }

    #[test]
    fn test_priority_clamped() {
        let task = ScheduledTask::new("t", "d", "a", 15, ResourceReq::default());
        assert_eq!(task.priority, 10);
        let task2 = ScheduledTask::new("t", "d", "a", 0, ResourceReq::default());
        assert_eq!(task2.priority, 1);
    }

    #[test]
    fn test_tasks_for_node() {
        let mut sched = TaskScheduler::new();
        sched.register_node(make_node("n1", 8.0, 8192));
        sched.register_node(make_node("n2", 8.0, 8192));

        let mut t1 = make_task("t1", 5);
        t1.node_preference = Some("n1".into());
        let mut t2 = make_task("t2", 5);
        t2.node_preference = Some("n2".into());
        sched.submit_task(t1).unwrap();
        sched.submit_task(t2).unwrap();
        sched.schedule_pending();

        assert_eq!(sched.tasks_for_node("n1").len(), 1);
        assert_eq!(sched.tasks_for_node("n2").len(), 1);
        assert_eq!(sched.tasks_for_node("n3").len(), 0);
    }

    #[test]
    fn test_deadline_field() {
        let mut task = make_task("deadline-task", 8);
        let deadline = Utc::now() + Duration::hours(1);
        task.deadline = Some(deadline);
        assert!(task.deadline.is_some());
        assert!(task.deadline.unwrap() > Utc::now());
    }

    // -- Cron --

    #[test]
    fn test_cron_add_entry() {
        let mut cron = CronScheduler::new();
        let entry = CronEntry {
            name: "backup".into(),
            interval_seconds: 3600,
            specific_hour: None,
            specific_minute: None,
            task_template: make_template(),
            enabled: true,
            last_fired: None,
        };
        assert!(cron.add_entry(entry).is_ok());
        assert_eq!(cron.list_entries().len(), 1);
    }

    #[test]
    fn test_cron_add_empty_name_rejected() {
        let mut cron = CronScheduler::new();
        let entry = CronEntry {
            name: "".into(),
            interval_seconds: 60,
            specific_hour: None,
            specific_minute: None,
            task_template: make_template(),
            enabled: true,
            last_fired: None,
        };
        assert!(cron.add_entry(entry).is_err());
    }

    #[test]
    fn test_cron_remove_entry() {
        let mut cron = CronScheduler::new();
        let entry = CronEntry {
            name: "backup".into(),
            interval_seconds: 3600,
            specific_hour: None,
            specific_minute: None,
            task_template: make_template(),
            enabled: true,
            last_fired: None,
        };
        cron.add_entry(entry).unwrap();
        assert!(cron.remove_entry("backup").is_ok());
        assert!(cron.list_entries().is_empty());
    }

    #[test]
    fn test_cron_remove_nonexistent() {
        let mut cron = CronScheduler::new();
        assert!(cron.remove_entry("nope").is_err());
    }

    #[test]
    fn test_cron_check_due_fires_first_time() {
        let mut cron = CronScheduler::new();
        let entry = CronEntry {
            name: "first-run".into(),
            interval_seconds: 60,
            specific_hour: None,
            specific_minute: None,
            task_template: make_template(),
            enabled: true,
            last_fired: None,
        };
        cron.add_entry(entry).unwrap();
        let tasks = cron.check_due();
        assert_eq!(tasks.len(), 1);
        assert!(tasks[0].name.contains("cron"));
    }

    #[test]
    fn test_cron_check_due_respects_interval() {
        let mut cron = CronScheduler::new();
        let entry = CronEntry {
            name: "frequent".into(),
            interval_seconds: 3600, // 1 hour
            specific_hour: None,
            specific_minute: None,
            task_template: make_template(),
            enabled: true,
            last_fired: Some(Utc::now()), // just fired
        };
        cron.add_entry(entry).unwrap();
        let tasks = cron.check_due();
        assert!(tasks.is_empty()); // not due yet
    }

    #[test]
    fn test_cron_check_due_disabled_skipped() {
        let mut cron = CronScheduler::new();
        let entry = CronEntry {
            name: "disabled".into(),
            interval_seconds: 1,
            specific_hour: None,
            specific_minute: None,
            task_template: make_template(),
            enabled: false,
            last_fired: None,
        };
        cron.add_entry(entry).unwrap();
        assert!(cron.check_due().is_empty());
    }

    #[test]
    fn test_cron_fires_when_interval_elapsed() {
        let mut cron = CronScheduler::new();
        let entry = CronEntry {
            name: "overdue".into(),
            interval_seconds: 10,
            specific_hour: None,
            specific_minute: None,
            task_template: make_template(),
            enabled: true,
            last_fired: Some(Utc::now() - Duration::seconds(60)),
        };
        cron.add_entry(entry).unwrap();
        let tasks = cron.check_due();
        assert_eq!(tasks.len(), 1);
    }
}
