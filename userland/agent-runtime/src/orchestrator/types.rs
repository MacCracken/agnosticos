//! Orchestrator types — tasks, priorities, results, and state.

use std::collections::{HashMap, HashSet, VecDeque};

use serde::{Deserialize, Serialize};

use agnos_common::AgentId;

/// Resource/capability requirements for a task.
#[derive(Debug, Clone, Default)]
pub struct TaskRequirements {
    /// Minimum memory in bytes the agent must have available.
    pub min_memory: u64,
    /// Minimum CPU shares.
    pub min_cpu_shares: u32,
    /// Capabilities the agent must possess.
    pub required_capabilities: Vec<String>,
    /// Preferred agent name (affinity bonus).
    pub preferred_agent: Option<String>,
}

/// Task priority levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TaskPriority {
    Critical = 0,
    High = 1,
    Normal = 2,
    Low = 3,
    Background = 4,
}

/// Represents a task to be executed by an agent
#[derive(Debug, Clone)]
pub struct Task {
    pub id: String,
    pub priority: TaskPriority,
    pub target_agents: Vec<AgentId>,
    pub payload: serde_json::Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub deadline: Option<chrono::DateTime<chrono::Utc>>,
    pub dependencies: Vec<String>,
    /// Resource/capability requirements for scoring.
    pub requirements: TaskRequirements,
}

/// Task execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    pub task_id: String,
    pub agent_id: AgentId,
    pub success: bool,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
    pub completed_at: chrono::DateTime<chrono::Utc>,
    pub duration_ms: u64,
}

/// Consolidated mutable state for the orchestrator.
///
/// Grouping these fields under a single `RwLock` eliminates per-operation
/// multi-lock acquisition (e.g. `submit_task` previously locked both
/// `task_queues` and `queued_task_ids` separately). It also makes compound
/// operations like `cancel_task` and `get_task_status` atomic with respect
/// to the task lifecycle.
///
/// **Why `message_rx` stays separate:** it is a one-shot `Option::take` used
/// only during `start()`. Including it here would force every hot-path
/// operation to share a lock with an effectively inert field.
#[derive(Debug)]
pub struct OrchestratorState {
    /// Task queues by priority.
    pub task_queues: HashMap<TaskPriority, VecDeque<Task>>,
    /// Currently running tasks.
    pub running_tasks: HashMap<String, Task>,
    /// Completed task results.
    pub results: HashMap<String, TaskResult>,
    /// O(1) lookup set for queued task IDs.
    pub queued_task_ids: HashSet<String>,
}

/// Queue statistics
#[derive(Debug, Clone)]
pub struct QueueStats {
    pub total_tasks: usize,
    pub running_tasks: usize,
    pub queued_tasks: usize,
}

/// Agent statistics for orchestrator
#[derive(Debug, Clone)]
pub struct AgentOrchestratorStats {
    pub registered_agents: usize,
    pub total_tasks_processed: usize,
}

/// Task status
#[derive(Debug, Clone)]
pub enum TaskStatus {
    Queued,
    Running,
    Completed(TaskResult),
}
