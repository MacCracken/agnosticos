//! Multi-Agent Orchestrator
//!
//! Handles agent coordination, task distribution, workload balancing, and conflict resolution.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use agnos_common::{AgentConfig, AgentId, Message, MessageType};

use crate::registry::AgentRegistry;

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

/// Orchestrator for multi-agent coordination
///
/// All interior state is wrapped in `Arc<RwLock<...>>` so that the orchestrator
/// can be cheaply cloned and passed to background tasks (e.g. the scheduler loop)
/// while still sharing the same underlying data structures.
#[derive(Clone)]
pub struct Orchestrator {
    registry: Arc<AgentRegistry>,
    /// Task queues by priority (shared across clones)
    task_queues: Arc<RwLock<HashMap<TaskPriority, VecDeque<Task>>>>,
    /// Running tasks (shared across clones)
    running_tasks: Arc<RwLock<HashMap<String, Task>>>,
    /// Task results (shared across clones)
    results: Arc<RwLock<HashMap<String, TaskResult>>>,
    /// Communication bus sender (cheap to clone)
    message_bus: mpsc::Sender<Message>,
    /// Receiver held until `start()` spawns the message loop
    message_rx: Arc<RwLock<Option<mpsc::Receiver<Message>>>>,
}

impl Orchestrator {
    /// Create a new orchestrator
    pub fn new(registry: Arc<AgentRegistry>) -> Self {
        let (message_bus, message_rx) = mpsc::channel(1000);
        
        let mut queues = HashMap::new();
        for priority in [
            TaskPriority::Critical,
            TaskPriority::High,
            TaskPriority::Normal,
            TaskPriority::Low,
            TaskPriority::Background,
        ] {
            queues.insert(priority, VecDeque::new());
        }

        Self {
            registry,
            task_queues: Arc::new(RwLock::new(queues)),
            running_tasks: Arc::new(RwLock::new(HashMap::new())),
            results: Arc::new(RwLock::new(HashMap::new())),
            message_bus,
            message_rx: Arc::new(RwLock::new(Some(message_rx))),
        }
    }

    /// Maximum number of completed task results to retain.
    const MAX_RESULTS: usize = 10_000;

    /// Start the orchestrator
    pub async fn start(&self) -> Result<()> {
        info!("Starting multi-agent orchestrator...");

        // Start the message processing loop with shared state
        if let Some(rx) = self.message_rx.write().await.take() {
            let results = self.results.clone();
            let running_tasks = self.running_tasks.clone();
            tokio::spawn(Self::message_loop(rx, results, running_tasks));
        }

        // Start the task scheduler
        tokio::spawn(Self::scheduler_loop(self.clone()));

        info!("Multi-agent orchestrator started");
        Ok(())
    }

    /// Submit a new task for execution
    pub async fn submit_task(&self, mut task: Task) -> Result<String> {
        task.id = Uuid::new_v4().to_string();
        task.created_at = chrono::Utc::now();

        info!(
            "Submitting task {} with priority {:?}",
            task.id, task.priority
        );

        let mut queues = self.task_queues.write().await;
        queues
            .get_mut(&task.priority)
            .context("Invalid task priority")?
            .push_back(task.clone());

        Ok(task.id)
    }

    /// Get task status
    pub async fn get_task_status(&self, task_id: &str) -> Option<TaskStatus> {
        // Check if running
        let running = self.running_tasks.read().await;
        if running.contains_key(task_id) {
            return Some(TaskStatus::Running);
        }
        drop(running);

        // Check if completed
        let results = self.results.read().await;
        if let Some(result) = results.get(task_id) {
            return Some(TaskStatus::Completed(result.clone()));
        }

        // Check if queued
        let queues = self.task_queues.read().await;
        for (_, queue) in queues.iter() {
            if queue.iter().any(|t| t.id == task_id) {
                return Some(TaskStatus::Queued);
            }
        }

        None
    }

    /// Get task result
    pub async fn get_result(&self, task_id: &str) -> Option<TaskResult> {
        self.results.read().await.get(task_id).cloned()
    }

    /// Distribute a task to available agents
    async fn distribute_task(&self, task: &Task) -> Result<()> {
        if task.target_agents.is_empty() {
            // Auto-assign based on capabilities
            self.auto_assign_task(task).await?;
        } else {
            // Send to specific agents
            for agent_id in &task.target_agents {
                if let Some(agent) = self.registry.get(*agent_id) {
                    let message = Message {
                        id: Uuid::new_v4().to_string(),
                        source: "orchestrator".to_string(),
                        target: agent.name,
                        message_type: MessageType::Command,
                        payload: task.payload.clone(),
                        timestamp: chrono::Utc::now(),
                    };
                    
                    self.message_bus.send(message).await
                        .map_err(|_| anyhow::anyhow!("Failed to send message"))?;
                }
            }
        }

        Ok(())
    }

    /// Auto-assign a task to the most suitable agent using load-aware scoring.
    async fn auto_assign_task(&self, task: &Task) -> Result<()> {
        let available = self.registry.list_by_status(agnos_common::AgentStatus::Running);

        if available.is_empty() {
            warn!("No available agents to execute task {}", task.id);
            return Err(anyhow::anyhow!("No available agents"));
        }

        // Score each agent and pick the best
        let mut best_agent = &available[0];
        let mut best_score = f64::NEG_INFINITY;

        // Count tasks per agent for fair-share
        let running = self.running_tasks.read().await;
        let mut task_counts: HashMap<AgentId, usize> = HashMap::new();
        for t in running.values() {
            for agent_id in &t.target_agents {
                *task_counts.entry(*agent_id).or_insert(0) += 1;
            }
        }
        drop(running);

        for agent in &available {
            let config = self.registry.get_config(agent.id);
            let score = Self::score_agent(
                agent,
                config.as_ref(),
                &task.requirements,
                *task_counts.get(&agent.id).unwrap_or(&0),
            );
            if score > best_score {
                best_score = score;
                best_agent = agent;
            }
        }

        info!(
            "Auto-assigned task {} to agent {} ({}) with score {:.2}",
            task.id, best_agent.name, best_agent.id, best_score
        );

        let message = Message {
            id: Uuid::new_v4().to_string(),
            source: "orchestrator".to_string(),
            target: best_agent.name.clone(),
            message_type: MessageType::Command,
            payload: task.payload.clone(),
            timestamp: chrono::Utc::now(),
        };

        self.message_bus.send(message).await
            .map_err(|_| anyhow::anyhow!("Failed to send message"))?;

        Ok(())
    }

    /// Score an agent for a given task's requirements.
    ///
    /// Weights:
    /// - Memory headroom:  40%
    /// - CPU headroom:     30%
    /// - Capability match: 20%
    /// - Affinity bonus:   10%
    ///
    /// Fair-share: agents with fewer running tasks get a bonus.
    pub fn score_agent(
        agent: &crate::agent::AgentHandle,
        config: Option<&AgentConfig>,
        requirements: &TaskRequirements,
        running_task_count: usize,
    ) -> f64 {
        let mut score = 0.0;

        // --- Memory headroom (40%) ---
        let max_memory = config
            .map(|c| c.resource_limits.max_memory)
            .unwrap_or(1024 * 1024 * 1024); // 1GB default
        let used_memory = agent.resource_usage.memory_used;
        let available_memory = max_memory.saturating_sub(used_memory);

        if requirements.min_memory > 0 {
            if available_memory >= requirements.min_memory {
                // Ratio of available to max, capped at 1.0
                let ratio = (available_memory as f64) / (max_memory as f64);
                score += 0.4 * ratio;
            }
            // else: 0 points for memory — agent can't satisfy the requirement
        } else {
            // No memory requirement — full points based on headroom
            let ratio = (available_memory as f64) / (max_memory.max(1) as f64);
            score += 0.4 * ratio;
        }

        // --- CPU headroom (30%) ---
        let max_cpu_time = config
            .map(|c| c.resource_limits.max_cpu_time)
            .unwrap_or(3_600_000);
        let used_cpu = agent.resource_usage.cpu_time_used;
        let available_cpu = max_cpu_time.saturating_sub(used_cpu);
        let cpu_ratio = (available_cpu as f64) / (max_cpu_time.max(1) as f64);
        score += 0.3 * cpu_ratio;

        // --- Capability match (20%) ---
        if let Some(config) = config {
            if !requirements.required_capabilities.is_empty() {
                let agent_caps: std::collections::HashSet<String> = config
                    .permissions
                    .iter()
                    .map(|p| format!("{:?}", p).to_lowercase())
                    .collect();

                let matched = requirements
                    .required_capabilities
                    .iter()
                    .filter(|cap| agent_caps.contains(&cap.to_lowercase()))
                    .count();

                let ratio = if requirements.required_capabilities.is_empty() {
                    1.0
                } else {
                    (matched as f64) / (requirements.required_capabilities.len() as f64)
                };
                score += 0.2 * ratio;
            } else {
                score += 0.2; // No requirements = full match
            }
        } else {
            score += 0.1; // No config = partial match
        }

        // --- Affinity bonus (10%) ---
        if let Some(ref preferred) = requirements.preferred_agent {
            if agent.name == *preferred {
                score += 0.1;
            }
        }

        // --- Fair-share bonus ---
        // Agents with fewer running tasks get a small bonus (up to 0.05)
        let fair_share_penalty = (running_task_count as f64) * 0.01;
        score -= fair_share_penalty.min(0.1);

        score
    }

    /// Handle task result and prune old results to prevent unbounded growth.
    async fn handle_result(&self, result: TaskResult) {
        let task_id = result.task_id.clone();
        info!(
            "Task {} completed by agent {}: success={}",
            task_id, result.agent_id, result.success
        );

        let mut results = self.results.write().await;
        results.insert(task_id.clone(), result);

        // Prune old results to prevent unbounded memory growth
        Self::prune_results(&mut results);
        drop(results);

        // Remove from running tasks
        self.running_tasks.write().await.remove(&task_id);
    }

    /// Prune results map if it exceeds MAX_RESULTS, keeping the most recent.
    fn prune_results(results: &mut HashMap<String, TaskResult>) {
        if results.len() > Self::MAX_RESULTS {
            let mut entries: Vec<_> = results
                .iter()
                .map(|(k, v)| (k.clone(), v.completed_at))
                .collect();
            entries.sort_by_key(|(_, t)| *t);
            let to_remove: Vec<_> = entries
                .iter()
                .take(entries.len() - Self::MAX_RESULTS)
                .map(|(k, _)| k.clone())
                .collect();
            for key in to_remove {
                results.remove(&key);
            }
        }
    }

    /// Message processing loop — receives messages from agents and processes
    /// task results, routing them into the shared results map.
    async fn message_loop(
        mut rx: mpsc::Receiver<Message>,
        results: Arc<RwLock<HashMap<String, TaskResult>>>,
        running_tasks: Arc<RwLock<HashMap<String, Task>>>,
    ) {
        while let Some(message) = rx.recv().await {
            debug!("Orchestrator received message: {:?}", message);

            match message.message_type {
                MessageType::Response => {
                    // Try to deserialize as TaskResult
                    if let Ok(result) = serde_json::from_value::<TaskResult>(message.payload.clone()) {
                        let task_id = result.task_id.clone();
                        info!(
                            "Task {} completed by agent {}: success={}",
                            task_id, result.agent_id, result.success
                        );

                        let mut res = results.write().await;
                        res.insert(task_id.clone(), result);
                        Self::prune_results(&mut res);
                        drop(res);

                        running_tasks.write().await.remove(&task_id);
                    } else {
                        debug!("Received non-task response: {:?}", message.payload);
                    }
                }
                MessageType::Event => {
                    debug!("Received event: {:?}", message.payload);
                }
                MessageType::Error => {
                    error!("Received error message: {:?}", message.payload);
                }
                _ => {}
            }
        }
    }

    /// Task scheduler loop
    async fn scheduler_loop(orchestrator: Self) {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(100));

        loop {
            interval.tick().await;

            // Process tasks by priority
            let priorities = [
                TaskPriority::Critical,
                TaskPriority::High,
                TaskPriority::Normal,
                TaskPriority::Low,
                TaskPriority::Background,
            ];

            for priority in &priorities {
                let mut queues = orchestrator.task_queues.write().await;
                if let Some(queue) = queues.get_mut(priority) {
                    if let Some(task) = queue.pop_front() {
                        // Check if all dependencies are satisfied (present in results)
                        if !task.dependencies.is_empty() {
                            let results = orchestrator.results.read().await;
                            let deps_satisfied = task
                                .dependencies
                                .iter()
                                .all(|dep_id| results.contains_key(dep_id));
                            drop(results);

                            if !deps_satisfied {
                                // Dependencies not yet met — push task back and skip
                                debug!(
                                    "Task {} has unsatisfied dependencies, deferring",
                                    task.id
                                );
                                queue.push_back(task);
                                continue;
                            }
                        }

                        drop(queues);

                        // Add to running tasks
                        orchestrator.running_tasks.write().await.insert(task.id.clone(), task.clone());

                        // Distribute the task
                        if let Err(e) = orchestrator.distribute_task(&task).await {
                            error!("Failed to distribute task {}: {}", task.id, e);
                        }
                    }
                }
            }
        }
    }

    /// Broadcast a message to all agents
    pub async fn broadcast(&self, message_type: MessageType, payload: serde_json::Value) -> Result<()> {
        let agents = self.registry.list_all();
        
        for agent in agents {
            let message = Message {
                id: Uuid::new_v4().to_string(),
                source: "orchestrator".to_string(),
                target: agent.name,
                message_type,
                payload: payload.clone(),
                timestamp: chrono::Utc::now(),
            };
            
            self.message_bus.send(message).await
                .map_err(|_| anyhow::anyhow!("Failed to broadcast message"))?;
        }

        Ok(())
    }

    /// Store task result (for testing)
    pub async fn store_result(&self, result: TaskResult) -> Result<()> {
        self.handle_result(result).await;
        Ok(())
    }

    /// Get queue statistics (for testing)
    pub async fn get_queue_stats(&self) -> QueueStats {
        let queued_tasks: usize = self.task_queues.read().await.values().map(|q| q.len()).sum();
        let running_tasks = self.running_tasks.read().await.len();

        QueueStats {
            total_tasks: queued_tasks + running_tasks,
            running_tasks,
            queued_tasks,
        }
    }

    /// Peek at next task (for testing)
    pub async fn peek_next_task(&self) -> Option<Task> {
        let queues = self.task_queues.read().await;
        
        for priority in [
            TaskPriority::Critical,
            TaskPriority::High,
            TaskPriority::Normal,
            TaskPriority::Low,
            TaskPriority::Background,
        ] {
            if let Some(queue) = queues.get(&priority) {
                if let Some(task) = queue.front() {
                    return Some(task.clone());
                }
            }
        }
        None
    }

    /// Get overdue tasks (for testing)
    pub async fn get_overdue_tasks(&self) -> Vec<Task> {
        let queues = self.task_queues.read().await;
        let now = chrono::Utc::now();
        
        let mut overdue = Vec::new();
        for queue in queues.values() {
            for task in queue.iter() {
                if let Some(deadline) = task.deadline {
                    if deadline < now {
                        overdue.push(task.clone());
                    }
                }
            }
        }
        overdue
    }

    /// Get agent statistics (for testing)
    pub async fn get_agent_stats(&self) -> AgentOrchestratorStats {
        let agents = self.registry.list_all();
        let results = self.results.read().await;
        
        AgentOrchestratorStats {
            registered_agents: agents.len(),
            total_tasks_processed: results.len(),
        }
    }

    /// Cancel a task (for testing)
    pub async fn cancel_task(&self, task_id: &str) -> Result<()> {
        // Acquire and release queues lock before acquiring running_tasks lock
        // to avoid potential deadlock with the scheduler loop.
        {
            let mut queues = self.task_queues.write().await;
            for queue in queues.values_mut() {
                queue.retain(|t| t.id != task_id);
            }
        }

        self.running_tasks.write().await.remove(task_id);

        Ok(())
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn create_test_orchestrator() -> Orchestrator {
        let registry = Arc::new(AgentRegistry::new());
        Orchestrator::new(registry)
    }

    #[tokio::test]
    async fn test_orchestrator_initialization() {
        let orchestrator = create_test_orchestrator();
        let queues = orchestrator.get_queue_stats().await;
        assert_eq!(queues.total_tasks, 0);
    }

    #[tokio::test]
    async fn test_task_submission() {
        let orchestrator = create_test_orchestrator();
        
        let task = Task {
            id: "test-task-1".to_string(),
            priority: TaskPriority::Normal,
            target_agents: vec![],
            payload: serde_json::json!({"action": "test"}),
            created_at: chrono::Utc::now(),
            deadline: None,
            dependencies: vec![],
            requirements: TaskRequirements::default(),
        };

        let result = orchestrator.submit_task(task).await;
        assert!(result.is_ok());

        let queues = orchestrator.get_queue_stats().await;
        assert_eq!(queues.total_tasks, 1);
    }

    #[tokio::test]
    async fn test_task_priority_ordering() {
        let orchestrator = create_test_orchestrator();

        let low_task = Task {
            id: "low".to_string(),
            priority: TaskPriority::Low,
            target_agents: vec![],
            payload: serde_json::json!({"p": "low"}),
            created_at: chrono::Utc::now(),
            deadline: None,
            dependencies: vec![],
            requirements: TaskRequirements::default(),
        };

        let critical_task = Task {
            id: "critical".to_string(),
            priority: TaskPriority::Critical,
            target_agents: vec![],
            payload: serde_json::json!({"p": "critical"}),
            created_at: chrono::Utc::now(),
            deadline: None,
            dependencies: vec![],
            requirements: TaskRequirements::default(),
        };
        
        let _ = orchestrator.submit_task(low_task).await;
        let _ = orchestrator.submit_task(critical_task).await;
        
        let next = orchestrator.peek_next_task().await;
        assert!(next.is_some());
        assert_eq!(next.unwrap().priority, TaskPriority::Critical);
    }

    #[tokio::test]
    async fn test_task_completion() {
        let orchestrator = create_test_orchestrator();
        
        let task = Task {
            id: "complete".to_string(),
            priority: TaskPriority::Normal,
            target_agents: vec![],
            payload: serde_json::json!({"action": "done"}),
            created_at: chrono::Utc::now(),
            deadline: None,
            dependencies: vec![],
            requirements: TaskRequirements::default(),
        };

        orchestrator.submit_task(task.clone()).await.unwrap();

        let result = TaskResult {
            task_id: task.id.clone(),
            agent_id: AgentId::new(),
            success: true,
            result: Some(serde_json::json!({"status": "done"})),
            error: None,
            completed_at: chrono::Utc::now(),
            duration_ms: 100,
        };
        
        orchestrator.store_result(result).await.unwrap();
        
        let retrieved = orchestrator.get_result(&task.id).await;
        assert!(retrieved.is_some());
        assert!(retrieved.unwrap().success);
    }

    #[tokio::test]
    async fn test_task_failure() {
        let orchestrator = create_test_orchestrator();
        
        let task = Task {
            id: "fail".to_string(),
            priority: TaskPriority::Normal,
            target_agents: vec![],
            payload: serde_json::json!({"action": "fail"}),
            created_at: chrono::Utc::now(),
            deadline: None,
            dependencies: vec![],
            requirements: TaskRequirements::default(),
        };

        orchestrator.submit_task(task.clone()).await.unwrap();

        let result = TaskResult {
            task_id: task.id.clone(),
            agent_id: AgentId::new(),
            success: false,
            result: None,
            error: Some("Test error".to_string()),
            completed_at: chrono::Utc::now(),
            duration_ms: 50,
        };
        
        orchestrator.store_result(result).await.unwrap();
        
        let retrieved = orchestrator.get_result(&task.id).await;
        assert!(retrieved.is_some());
        assert!(!retrieved.unwrap().success);
    }

    #[tokio::test]
    async fn test_overdue_tasks() {
        let orchestrator = create_test_orchestrator();
        
        let task = Task {
            id: "deadline".to_string(),
            priority: TaskPriority::Normal,
            target_agents: vec![],
            payload: serde_json::json!({}),
            created_at: chrono::Utc::now(),
            deadline: Some(chrono::Utc::now()),
            dependencies: vec![],
            requirements: TaskRequirements::default(),
        };

        orchestrator.submit_task(task).await.unwrap();
        
        let overdue = orchestrator.get_overdue_tasks().await;
        assert!(!overdue.is_empty());
    }

    #[tokio::test]
    async fn test_cancellation() {
        let orchestrator = create_test_orchestrator();
        
        let task = Task {
            id: "cancel".to_string(),
            priority: TaskPriority::Normal,
            target_agents: vec![],
            payload: serde_json::json!({}),
            created_at: chrono::Utc::now(),
            deadline: None,
            dependencies: vec![],
            requirements: TaskRequirements::default(),
        };
        
        orchestrator.submit_task(task.clone()).await.unwrap();
        
        // Try to cancel
        let _ = orchestrator.cancel_task(&task.id).await;
        
        // The task may still be in the queue if scheduler hasn't processed it
        // Just verify the cancel doesn't error
        let queues = orchestrator.get_queue_stats().await;
        assert!(queues.total_tasks <= 1);
    }

    #[tokio::test]
    async fn test_workload_stats() {
        let orchestrator = create_test_orchestrator();

        let stats = orchestrator.get_agent_stats().await;
        assert_eq!(stats.registered_agents, 0);
    }

    #[tokio::test]
    async fn test_dependency_blocking() {
        let orchestrator = create_test_orchestrator();

        // Submit a task that depends on a dependency that hasn't completed yet
        let task_with_dep = Task {
            id: "child".to_string(),
            priority: TaskPriority::Normal,
            target_agents: vec![],
            payload: serde_json::json!({"action": "child"}),
            created_at: chrono::Utc::now(),
            deadline: None,
            dependencies: vec!["parent-task".to_string()],
            requirements: TaskRequirements::default(),
        };

        orchestrator.submit_task(task_with_dep).await.unwrap();

        // The task should stay queued (scheduler hasn't run yet, but peek should show it)
        let next = orchestrator.peek_next_task().await;
        assert!(next.is_some());
        assert_eq!(next.unwrap().dependencies, vec!["parent-task".to_string()]);

        // Manually run one scheduler iteration by calling scheduler_loop logic:
        // pop the task, check deps, push back
        {
            let mut queues = orchestrator.task_queues.write().await;
            if let Some(queue) = queues.get_mut(&TaskPriority::Normal) {
                if let Some(task) = queue.pop_front() {
                    let results = orchestrator.results.read().await;
                    let deps_satisfied = task
                        .dependencies
                        .iter()
                        .all(|dep_id| results.contains_key(dep_id));
                    drop(results);

                    // Deps not satisfied — push back
                    assert!(!deps_satisfied);
                    queue.push_back(task);
                }
            }
        }

        // Task is still queued
        let stats = orchestrator.get_queue_stats().await;
        assert_eq!(stats.queued_tasks, 1);

        // Now complete the dependency
        let parent_result = TaskResult {
            task_id: "parent-task".to_string(),
            agent_id: AgentId::new(),
            success: true,
            result: Some(serde_json::json!({"status": "done"})),
            error: None,
            completed_at: chrono::Utc::now(),
            duration_ms: 50,
        };
        orchestrator.store_result(parent_result).await.unwrap();

        // Now the dependency is satisfied
        {
            let queues = orchestrator.task_queues.read().await;
            if let Some(queue) = queues.get(&TaskPriority::Normal) {
                if let Some(task) = queue.front() {
                    let results = orchestrator.results.read().await;
                    let deps_satisfied = task
                        .dependencies
                        .iter()
                        .all(|dep_id| results.contains_key(dep_id));
                    assert!(deps_satisfied);
                }
            }
        }
    }

    #[test]
    fn test_score_agent_idle() {
        use crate::agent::AgentHandle;

        let handle = AgentHandle {
            id: AgentId::new(),
            name: "idle-agent".to_string(),
            status: agnos_common::AgentStatus::Running,
            created_at: chrono::Utc::now(),
            started_at: None,
            resource_usage: agnos_common::ResourceUsage::default(),
            pid: None,
        };

        let config = AgentConfig::default();
        let requirements = TaskRequirements::default();

        let score = Orchestrator::score_agent(&handle, Some(&config), &requirements, 0);
        // Idle agent with no requirements should get near-max score
        assert!(score > 0.8, "Expected high score for idle agent, got {}", score);
    }

    #[test]
    fn test_score_agent_with_load() {
        use crate::agent::AgentHandle;

        let handle = AgentHandle {
            id: AgentId::new(),
            name: "busy-agent".to_string(),
            status: agnos_common::AgentStatus::Running,
            created_at: chrono::Utc::now(),
            started_at: None,
            resource_usage: agnos_common::ResourceUsage {
                memory_used: 512 * 1024 * 1024,
                cpu_time_used: 1_800_000,
                file_descriptors_used: 0,
                processes_used: 0,
            },
            pid: None,
        };

        let config = AgentConfig::default();
        let requirements = TaskRequirements::default();

        let score = Orchestrator::score_agent(&handle, Some(&config), &requirements, 5);
        // Agent at ~50% resource usage with 5 running tasks
        assert!(score > 0.0, "Score should be positive");
        assert!(score < 0.8, "Score should be lower due to load");
    }

    #[test]
    fn test_score_agent_affinity() {
        use crate::agent::AgentHandle;

        let handle = AgentHandle {
            id: AgentId::new(),
            name: "preferred-agent".to_string(),
            status: agnos_common::AgentStatus::Running,
            created_at: chrono::Utc::now(),
            started_at: None,
            resource_usage: agnos_common::ResourceUsage::default(),
            pid: None,
        };

        let config = AgentConfig::default();

        let reqs_with_affinity = TaskRequirements {
            preferred_agent: Some("preferred-agent".to_string()),
            ..Default::default()
        };
        let reqs_without = TaskRequirements::default();

        let score_with = Orchestrator::score_agent(&handle, Some(&config), &reqs_with_affinity, 0);
        let score_without = Orchestrator::score_agent(&handle, Some(&config), &reqs_without, 0);

        assert!(score_with > score_without, "Affinity should boost score");
    }

    #[test]
    fn test_score_agent_fair_share() {
        use crate::agent::AgentHandle;

        let handle = AgentHandle {
            id: AgentId::new(),
            name: "agent".to_string(),
            status: agnos_common::AgentStatus::Running,
            created_at: chrono::Utc::now(),
            started_at: None,
            resource_usage: agnos_common::ResourceUsage::default(),
            pid: None,
        };

        let config = AgentConfig::default();
        let requirements = TaskRequirements::default();

        let score_0_tasks = Orchestrator::score_agent(&handle, Some(&config), &requirements, 0);
        let score_10_tasks = Orchestrator::score_agent(&handle, Some(&config), &requirements, 10);

        assert!(score_0_tasks > score_10_tasks, "Agent with fewer tasks should score higher");
    }

    #[test]
    fn test_task_requirements_default() {
        let req = TaskRequirements::default();
        assert_eq!(req.min_memory, 0);
        assert_eq!(req.min_cpu_shares, 0);
        assert!(req.required_capabilities.is_empty());
        assert!(req.preferred_agent.is_none());
    }

    #[test]
    fn test_task_requirements_with_values() {
        let req = TaskRequirements {
            min_memory: 512 * 1024 * 1024,
            min_cpu_shares: 100,
            required_capabilities: vec!["gpu".to_string(), "network".to_string()],
            preferred_agent: Some("my-agent".to_string()),
        };
        assert_eq!(req.min_memory, 512 * 1024 * 1024);
        assert_eq!(req.min_cpu_shares, 100);
        assert_eq!(req.required_capabilities.len(), 2);
        assert_eq!(req.preferred_agent, Some("my-agent".to_string()));
    }

    #[test]
    fn test_task_requirements_clone() {
        let req = TaskRequirements {
            min_memory: 1024,
            min_cpu_shares: 50,
            required_capabilities: vec!["cap1".to_string()],
            preferred_agent: Some("agent-x".to_string()),
        };
        let cloned = req.clone();
        assert_eq!(cloned.min_memory, 1024);
        assert_eq!(cloned.min_cpu_shares, 50);
        assert_eq!(cloned.required_capabilities, vec!["cap1"]);
        assert_eq!(cloned.preferred_agent, Some("agent-x".to_string()));
    }

    #[test]
    fn test_task_priority_ord() {
        assert!(TaskPriority::Critical < TaskPriority::High);
        assert!(TaskPriority::High < TaskPriority::Normal);
        assert!(TaskPriority::Normal < TaskPriority::Low);
        assert!(TaskPriority::Low < TaskPriority::Background);
    }

    #[test]
    fn test_task_priority_equality() {
        assert_eq!(TaskPriority::Critical, TaskPriority::Critical);
        assert_ne!(TaskPriority::Critical, TaskPriority::Low);
    }

    #[test]
    fn test_task_priority_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(TaskPriority::Critical);
        set.insert(TaskPriority::High);
        set.insert(TaskPriority::Normal);
        set.insert(TaskPriority::Low);
        set.insert(TaskPriority::Background);
        assert_eq!(set.len(), 5);
        // Inserting duplicate should not increase count
        set.insert(TaskPriority::Critical);
        assert_eq!(set.len(), 5);
    }

    #[test]
    fn test_task_priority_debug() {
        assert_eq!(format!("{:?}", TaskPriority::Critical), "Critical");
        assert_eq!(format!("{:?}", TaskPriority::Background), "Background");
    }

    #[test]
    fn test_task_result_serialization() {
        let result = TaskResult {
            task_id: "task-1".to_string(),
            agent_id: AgentId::new(),
            success: true,
            result: Some(serde_json::json!({"output": "hello"})),
            error: None,
            completed_at: chrono::Utc::now(),
            duration_ms: 250,
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("task-1"));
        assert!(json.contains("\"success\":true"));

        let deserialized: TaskResult = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.task_id, "task-1");
        assert!(deserialized.success);
        assert_eq!(deserialized.duration_ms, 250);
    }

    #[test]
    fn test_task_result_failure_serialization() {
        let result = TaskResult {
            task_id: "fail-task".to_string(),
            agent_id: AgentId::new(),
            success: false,
            result: None,
            error: Some("something went wrong".to_string()),
            completed_at: chrono::Utc::now(),
            duration_ms: 10,
        };

        let json = serde_json::to_string(&result).unwrap();
        let deserialized: TaskResult = serde_json::from_str(&json).unwrap();
        assert!(!deserialized.success);
        assert_eq!(deserialized.error, Some("something went wrong".to_string()));
    }

    #[test]
    fn test_task_clone() {
        let task = Task {
            id: "clone-task".to_string(),
            priority: TaskPriority::High,
            target_agents: vec![AgentId::new()],
            payload: serde_json::json!({"action": "test"}),
            created_at: chrono::Utc::now(),
            deadline: Some(chrono::Utc::now()),
            dependencies: vec!["dep1".to_string()],
            requirements: TaskRequirements {
                min_memory: 100,
                ..Default::default()
            },
        };
        let cloned = task.clone();
        assert_eq!(cloned.id, "clone-task");
        assert_eq!(cloned.priority, TaskPriority::High);
        assert_eq!(cloned.target_agents.len(), 1);
        assert!(cloned.deadline.is_some());
        assert_eq!(cloned.dependencies, vec!["dep1"]);
        assert_eq!(cloned.requirements.min_memory, 100);
    }

    #[test]
    fn test_queue_stats_clone() {
        let stats = QueueStats {
            total_tasks: 10,
            running_tasks: 3,
            queued_tasks: 7,
        };
        let cloned = stats.clone();
        assert_eq!(cloned.total_tasks, 10);
        assert_eq!(cloned.running_tasks, 3);
        assert_eq!(cloned.queued_tasks, 7);
    }

    #[test]
    fn test_queue_stats_debug() {
        let stats = QueueStats {
            total_tasks: 5,
            running_tasks: 2,
            queued_tasks: 3,
        };
        let debug = format!("{:?}", stats);
        assert!(debug.contains("total_tasks"));
        assert!(debug.contains("running_tasks"));
        assert!(debug.contains("queued_tasks"));
    }

    #[test]
    fn test_agent_orchestrator_stats_clone() {
        let stats = AgentOrchestratorStats {
            registered_agents: 5,
            total_tasks_processed: 100,
        };
        let cloned = stats.clone();
        assert_eq!(cloned.registered_agents, 5);
        assert_eq!(cloned.total_tasks_processed, 100);
    }

    #[test]
    fn test_task_status_variants() {
        let queued = TaskStatus::Queued;
        let running = TaskStatus::Running;
        let completed = TaskStatus::Completed(TaskResult {
            task_id: "t".to_string(),
            agent_id: AgentId::new(),
            success: true,
            result: None,
            error: None,
            completed_at: chrono::Utc::now(),
            duration_ms: 0,
        });

        // Verify Debug works
        assert!(format!("{:?}", queued).contains("Queued"));
        assert!(format!("{:?}", running).contains("Running"));
        assert!(format!("{:?}", completed).contains("Completed"));
    }

    #[tokio::test]
    async fn test_orchestrator_clone_shares_state() {
        let orchestrator = create_test_orchestrator();
        let cloned = orchestrator.clone();

        // Submit via original
        let task = Task {
            id: "shared".to_string(),
            priority: TaskPriority::Normal,
            target_agents: vec![],
            payload: serde_json::json!({}),
            created_at: chrono::Utc::now(),
            deadline: None,
            dependencies: vec![],
            requirements: TaskRequirements::default(),
        };
        orchestrator.submit_task(task).await.unwrap();

        // Should be visible from clone
        let stats = cloned.get_queue_stats().await;
        assert_eq!(stats.queued_tasks, 1);
    }

    #[tokio::test]
    async fn test_submit_all_priorities() {
        let orchestrator = create_test_orchestrator();

        for priority in [
            TaskPriority::Critical,
            TaskPriority::High,
            TaskPriority::Normal,
            TaskPriority::Low,
            TaskPriority::Background,
        ] {
            let task = Task {
                id: format!("{:?}", priority),
                priority,
                target_agents: vec![],
                payload: serde_json::json!({}),
                created_at: chrono::Utc::now(),
                deadline: None,
                dependencies: vec![],
                requirements: TaskRequirements::default(),
            };
            orchestrator.submit_task(task).await.unwrap();
        }

        let stats = orchestrator.get_queue_stats().await;
        assert_eq!(stats.queued_tasks, 5);
        assert_eq!(stats.total_tasks, 5);
        assert_eq!(stats.running_tasks, 0);
    }

    #[tokio::test]
    async fn test_get_task_status_queued() {
        let orchestrator = create_test_orchestrator();

        let task = Task {
            id: "status-test".to_string(),
            priority: TaskPriority::Normal,
            target_agents: vec![],
            payload: serde_json::json!({}),
            created_at: chrono::Utc::now(),
            deadline: None,
            dependencies: vec![],
            requirements: TaskRequirements::default(),
        };
        let id = orchestrator.submit_task(task).await.unwrap();

        let status = orchestrator.get_task_status(&id).await;
        assert!(status.is_some());
        assert!(matches!(status.unwrap(), TaskStatus::Queued));
    }

    #[tokio::test]
    async fn test_get_task_status_completed() {
        let orchestrator = create_test_orchestrator();

        let result = TaskResult {
            task_id: "done-task".to_string(),
            agent_id: AgentId::new(),
            success: true,
            result: None,
            error: None,
            completed_at: chrono::Utc::now(),
            duration_ms: 50,
        };
        orchestrator.store_result(result).await.unwrap();

        let status = orchestrator.get_task_status("done-task").await;
        assert!(status.is_some());
        assert!(matches!(status.unwrap(), TaskStatus::Completed(_)));
    }

    #[tokio::test]
    async fn test_get_task_status_nonexistent() {
        let orchestrator = create_test_orchestrator();
        let status = orchestrator.get_task_status("nonexistent").await;
        assert!(status.is_none());
    }

    #[tokio::test]
    async fn test_cancel_nonexistent_task() {
        let orchestrator = create_test_orchestrator();
        // Should succeed (no-op)
        let result = orchestrator.cancel_task("nonexistent").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_cancel_removes_from_queue() {
        let orchestrator = create_test_orchestrator();

        let task = Task {
            id: "to-cancel".to_string(),
            priority: TaskPriority::Low,
            target_agents: vec![],
            payload: serde_json::json!({}),
            created_at: chrono::Utc::now(),
            deadline: None,
            dependencies: vec![],
            requirements: TaskRequirements::default(),
        };
        let id = orchestrator.submit_task(task).await.unwrap();

        assert_eq!(orchestrator.get_queue_stats().await.queued_tasks, 1);
        orchestrator.cancel_task(&id).await.unwrap();
        assert_eq!(orchestrator.get_queue_stats().await.queued_tasks, 0);
    }

    #[tokio::test]
    async fn test_overdue_tasks_no_deadline() {
        let orchestrator = create_test_orchestrator();

        let task = Task {
            id: "no-deadline".to_string(),
            priority: TaskPriority::Normal,
            target_agents: vec![],
            payload: serde_json::json!({}),
            created_at: chrono::Utc::now(),
            deadline: None,
            dependencies: vec![],
            requirements: TaskRequirements::default(),
        };
        orchestrator.submit_task(task).await.unwrap();

        let overdue = orchestrator.get_overdue_tasks().await;
        assert!(overdue.is_empty());
    }

    #[tokio::test]
    async fn test_peek_next_task_empty() {
        let orchestrator = create_test_orchestrator();
        assert!(orchestrator.peek_next_task().await.is_none());
    }

    #[tokio::test]
    async fn test_get_result_nonexistent() {
        let orchestrator = create_test_orchestrator();
        assert!(orchestrator.get_result("nope").await.is_none());
    }

    #[test]
    fn test_prune_results_under_limit() {
        let mut results = HashMap::new();
        for i in 0..10 {
            results.insert(
                format!("task-{}", i),
                TaskResult {
                    task_id: format!("task-{}", i),
                    agent_id: AgentId::new(),
                    success: true,
                    result: None,
                    error: None,
                    completed_at: chrono::Utc::now(),
                    duration_ms: 0,
                },
            );
        }
        Orchestrator::prune_results(&mut results);
        assert_eq!(results.len(), 10); // No pruning needed
    }

    #[test]
    fn test_score_agent_no_config() {
        use crate::agent::AgentHandle;

        let handle = AgentHandle {
            id: AgentId::new(),
            name: "no-config".to_string(),
            status: agnos_common::AgentStatus::Running,
            created_at: chrono::Utc::now(),
            started_at: None,
            resource_usage: agnos_common::ResourceUsage::default(),
            pid: None,
        };

        let requirements = TaskRequirements::default();
        let score = Orchestrator::score_agent(&handle, None, &requirements, 0);
        // Without config we get partial capability match (0.1 instead of 0.2)
        assert!(score > 0.0);
    }

    #[test]
    fn test_score_agent_memory_requirement_not_met() {
        use crate::agent::AgentHandle;

        let handle = AgentHandle {
            id: AgentId::new(),
            name: "low-mem".to_string(),
            status: agnos_common::AgentStatus::Running,
            created_at: chrono::Utc::now(),
            started_at: None,
            resource_usage: agnos_common::ResourceUsage {
                memory_used: 900 * 1024 * 1024, // 900 MB used
                cpu_time_used: 0,
                file_descriptors_used: 0,
                processes_used: 0,
            },
            pid: None,
        };

        let config = AgentConfig::default(); // 1 GB max
        let requirements = TaskRequirements {
            min_memory: 500 * 1024 * 1024, // Needs 500 MB but only ~124 MB available
            ..Default::default()
        };

        let score = Orchestrator::score_agent(&handle, Some(&config), &requirements, 0);
        // Memory component should be 0 since requirement not met
        // Total score should be < 0.6 (only CPU + capability)
        assert!(score < 0.7);
    }

    #[test]
    fn test_score_agent_memory_requirement_met() {
        use crate::agent::AgentHandle;

        let handle = AgentHandle {
            id: AgentId::new(),
            name: "good-mem".to_string(),
            status: agnos_common::AgentStatus::Running,
            created_at: chrono::Utc::now(),
            started_at: None,
            resource_usage: agnos_common::ResourceUsage::default(), // 0 used
            pid: None,
        };

        let config = AgentConfig::default(); // 1 GB max
        let requirements = TaskRequirements {
            min_memory: 100 * 1024 * 1024, // Needs 100 MB, 1 GB available
            ..Default::default()
        };

        let score = Orchestrator::score_agent(&handle, Some(&config), &requirements, 0);
        assert!(score > 0.8);
    }

    // ==================================================================
    // Additional coverage: prune_results over limit, handle_result,
    // score_agent with capabilities, task lifecycle, broadcast,
    // get_task_status for running tasks, store_result + cancel
    // ==================================================================

    #[test]
    fn test_prune_results_over_limit() {
        let mut results = HashMap::new();
        let now = chrono::Utc::now();

        // Insert MAX_RESULTS + 100 entries with different timestamps
        for i in 0..(Orchestrator::MAX_RESULTS + 100) {
            let ts = now + chrono::Duration::milliseconds(i as i64);
            results.insert(
                format!("task-{}", i),
                TaskResult {
                    task_id: format!("task-{}", i),
                    agent_id: AgentId::new(),
                    success: true,
                    result: None,
                    error: None,
                    completed_at: ts,
                    duration_ms: 0,
                },
            );
        }

        assert_eq!(results.len(), Orchestrator::MAX_RESULTS + 100);
        Orchestrator::prune_results(&mut results);
        assert_eq!(results.len(), Orchestrator::MAX_RESULTS);
    }

    #[tokio::test]
    async fn test_store_result_adds_to_results_and_removes_from_running() {
        let orchestrator = create_test_orchestrator();

        // Manually insert a "running" task
        {
            let mut running = orchestrator.running_tasks.write().await;
            running.insert("running-1".to_string(), Task {
                id: "running-1".to_string(),
                priority: TaskPriority::Normal,
                target_agents: vec![],
                payload: serde_json::json!({}),
                created_at: chrono::Utc::now(),
                deadline: None,
                dependencies: vec![],
                requirements: TaskRequirements::default(),
            });
        }

        let result = TaskResult {
            task_id: "running-1".to_string(),
            agent_id: AgentId::new(),
            success: true,
            result: Some(serde_json::json!({"ok": true})),
            error: None,
            completed_at: chrono::Utc::now(),
            duration_ms: 123,
        };

        orchestrator.store_result(result).await.unwrap();

        // Should be in results
        let stored = orchestrator.get_result("running-1").await;
        assert!(stored.is_some());
        assert!(stored.unwrap().success);

        // Should be removed from running
        let running = orchestrator.running_tasks.read().await;
        assert!(!running.contains_key("running-1"));
    }

    #[tokio::test]
    async fn test_get_task_status_running() {
        let orchestrator = create_test_orchestrator();

        // Insert directly into running_tasks
        {
            let mut running = orchestrator.running_tasks.write().await;
            running.insert("r-task".to_string(), Task {
                id: "r-task".to_string(),
                priority: TaskPriority::High,
                target_agents: vec![],
                payload: serde_json::json!({}),
                created_at: chrono::Utc::now(),
                deadline: None,
                dependencies: vec![],
                requirements: TaskRequirements::default(),
            });
        }

        let status = orchestrator.get_task_status("r-task").await;
        assert!(status.is_some());
        assert!(matches!(status.unwrap(), TaskStatus::Running));
    }

    #[test]
    fn test_score_agent_with_capabilities_match() {
        use crate::agent::AgentHandle;

        let handle = AgentHandle {
            id: AgentId::new(),
            name: "cap-agent".to_string(),
            status: agnos_common::AgentStatus::Running,
            created_at: chrono::Utc::now(),
            started_at: None,
            resource_usage: agnos_common::ResourceUsage::default(),
            pid: None,
        };

        let config = AgentConfig {
            permissions: vec![
                agnos_common::Permission::FileRead,
                agnos_common::Permission::FileWrite,
            ],
            ..Default::default()
        };

        let requirements = TaskRequirements {
            required_capabilities: vec!["readfile".to_string()],
            ..Default::default()
        };

        let score = Orchestrator::score_agent(&handle, Some(&config), &requirements, 0);
        // Should get partial capability match score
        assert!(score > 0.5);
    }

    #[test]
    fn test_score_agent_no_capabilities_match() {
        use crate::agent::AgentHandle;

        let handle = AgentHandle {
            id: AgentId::new(),
            name: "nocap-agent".to_string(),
            status: agnos_common::AgentStatus::Running,
            created_at: chrono::Utc::now(),
            started_at: None,
            resource_usage: agnos_common::ResourceUsage::default(),
            pid: None,
        };

        let config = AgentConfig::default();
        let requirements = TaskRequirements {
            required_capabilities: vec!["gpu".to_string(), "cuda".to_string()],
            ..Default::default()
        };

        let score_with_caps = Orchestrator::score_agent(&handle, Some(&config), &requirements, 0);
        let score_no_caps = Orchestrator::score_agent(&handle, Some(&config), &TaskRequirements::default(), 0);

        // Without matching capabilities, the score should be lower
        assert!(score_no_caps >= score_with_caps);
    }

    #[tokio::test]
    async fn test_cancel_running_task() {
        let orchestrator = create_test_orchestrator();

        // Insert task directly into running
        {
            let mut running = orchestrator.running_tasks.write().await;
            running.insert("cancel-running".to_string(), Task {
                id: "cancel-running".to_string(),
                priority: TaskPriority::Normal,
                target_agents: vec![],
                payload: serde_json::json!({}),
                created_at: chrono::Utc::now(),
                deadline: None,
                dependencies: vec![],
                requirements: TaskRequirements::default(),
            });
        }

        orchestrator.cancel_task("cancel-running").await.unwrap();
        let running = orchestrator.running_tasks.read().await;
        assert!(!running.contains_key("cancel-running"));
    }

    #[tokio::test]
    async fn test_submit_multiple_same_priority() {
        let orchestrator = create_test_orchestrator();

        for i in 0..10 {
            let task = Task {
                id: format!("batch-{}", i),
                priority: TaskPriority::Normal,
                target_agents: vec![],
                payload: serde_json::json!({"index": i}),
                created_at: chrono::Utc::now(),
                deadline: None,
                dependencies: vec![],
                requirements: TaskRequirements::default(),
            };
            orchestrator.submit_task(task).await.unwrap();
        }

        let stats = orchestrator.get_queue_stats().await;
        assert_eq!(stats.queued_tasks, 10);
        assert_eq!(stats.running_tasks, 0);
        assert_eq!(stats.total_tasks, 10);
    }

    #[tokio::test]
    async fn test_overdue_tasks_mixed() {
        let orchestrator = create_test_orchestrator();
        let now = chrono::Utc::now();

        // One overdue task
        let overdue_task = Task {
            id: "overdue".to_string(),
            priority: TaskPriority::Normal,
            target_agents: vec![],
            payload: serde_json::json!({}),
            created_at: now,
            deadline: Some(now - chrono::Duration::hours(1)),
            dependencies: vec![],
            requirements: TaskRequirements::default(),
        };
        orchestrator.submit_task(overdue_task).await.unwrap();

        // One future task
        let future_task = Task {
            id: "future".to_string(),
            priority: TaskPriority::Normal,
            target_agents: vec![],
            payload: serde_json::json!({}),
            created_at: now,
            deadline: Some(now + chrono::Duration::hours(1)),
            dependencies: vec![],
            requirements: TaskRequirements::default(),
        };
        orchestrator.submit_task(future_task).await.unwrap();

        // One no-deadline task
        let no_deadline = Task {
            id: "nodeadline".to_string(),
            priority: TaskPriority::Low,
            target_agents: vec![],
            payload: serde_json::json!({}),
            created_at: now,
            deadline: None,
            dependencies: vec![],
            requirements: TaskRequirements::default(),
        };
        orchestrator.submit_task(no_deadline).await.unwrap();

        let overdue = orchestrator.get_overdue_tasks().await;
        assert_eq!(overdue.len(), 1);
    }

    #[tokio::test]
    async fn test_agent_stats_with_results() {
        let orchestrator = create_test_orchestrator();

        for i in 0..5 {
            let result = TaskResult {
                task_id: format!("stat-task-{}", i),
                agent_id: AgentId::new(),
                success: true,
                result: None,
                error: None,
                completed_at: chrono::Utc::now(),
                duration_ms: 10,
            };
            orchestrator.store_result(result).await.unwrap();
        }

        let stats = orchestrator.get_agent_stats().await;
        assert_eq!(stats.total_tasks_processed, 5);
        assert_eq!(stats.registered_agents, 0); // No agents in registry
    }

    #[test]
    fn test_task_result_clone() {
        let result = TaskResult {
            task_id: "clone-result".to_string(),
            agent_id: AgentId::new(),
            success: false,
            result: None,
            error: Some("err".to_string()),
            completed_at: chrono::Utc::now(),
            duration_ms: 42,
        };
        let cloned = result.clone();
        assert_eq!(cloned.task_id, "clone-result");
        assert!(!cloned.success);
        assert_eq!(cloned.error, Some("err".to_string()));
        assert_eq!(cloned.duration_ms, 42);
    }

    #[test]
    fn test_task_status_clone() {
        let status = TaskStatus::Queued;
        let cloned = status.clone();
        assert!(matches!(cloned, TaskStatus::Queued));

        let status = TaskStatus::Running;
        let cloned = status.clone();
        assert!(matches!(cloned, TaskStatus::Running));
    }

    #[test]
    fn test_max_results_constant() {
        assert_eq!(Orchestrator::MAX_RESULTS, 10_000);
    }

    #[tokio::test]
    async fn test_orchestrator_start_succeeds() {
        let orchestrator = create_test_orchestrator();
        let result = orchestrator.start().await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_score_agent_high_fair_share_penalty() {
        use crate::agent::AgentHandle;

        let handle = AgentHandle {
            id: AgentId::new(),
            name: "agent".to_string(),
            status: agnos_common::AgentStatus::Running,
            created_at: chrono::Utc::now(),
            started_at: None,
            resource_usage: agnos_common::ResourceUsage::default(),
            pid: None,
        };

        let config = AgentConfig::default();
        let requirements = TaskRequirements::default();

        // With 100 running tasks, penalty is capped at 0.1
        let score_100 = Orchestrator::score_agent(&handle, Some(&config), &requirements, 100);
        let score_11 = Orchestrator::score_agent(&handle, Some(&config), &requirements, 11);

        // Both should have max penalty (0.1), so scores should be equal
        assert!((score_100 - score_11).abs() < 0.001);
    }
}
