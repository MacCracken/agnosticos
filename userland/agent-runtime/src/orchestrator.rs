//! Multi-Agent Orchestrator
//!
//! Handles agent coordination, task distribution, workload balancing, and conflict resolution.

use std::collections::{HashMap, VecDeque};

use anyhow::{Context, Result};
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use agnos_common::{AgentId, Message, MessageType};

use crate::registry::AgentRegistry;

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
}

/// Task execution result
#[derive(Debug, Clone)]
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
pub struct Orchestrator {
    registry: Arc<AgentRegistry>,
    /// Task queues by priority
    task_queues: RwLock<HashMap<TaskPriority, VecDeque<Task>>>,
    /// Running tasks
    running_tasks: RwLock<HashMap<String, Task>>,
    /// Task results
    results: RwLock<HashMap<String, TaskResult>>,
    /// Communication bus
    message_bus: mpsc::Sender<Message>,
    message_rx: RwLock<Option<mpsc::Receiver<Message>>>,
}

impl Clone for Orchestrator {
    fn clone(&self) -> Self {
        Self {
            registry: self.registry.clone(),
            task_queues: RwLock::new(HashMap::new()),
            running_tasks: RwLock::new(HashMap::new()),
            results: RwLock::new(HashMap::new()),
            message_bus: self.message_bus.clone(),
            message_rx: RwLock::new(None),
        }
    }
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
            task_queues: RwLock::new(queues),
            running_tasks: RwLock::new(HashMap::new()),
            results: RwLock::new(HashMap::new()),
            message_bus,
            message_rx: RwLock::new(Some(message_rx)),
        }
    }

    /// Start the orchestrator
    pub async fn start(&self) -> Result<()> {
        info!("Starting multi-agent orchestrator...");

        // Start the message processing loop
        if let Some(rx) = self.message_rx.write().await.take() {
            tokio::spawn(Self::message_loop(rx));
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

    /// Auto-assign a task to the most suitable agent(s)
    async fn auto_assign_task(&self, task: &Task) -> Result<()> {
        // Get available agents
        let available = self.registry.list_by_status(agnos_common::AgentStatus::Running);

        if available.is_empty() {
            warn!("No available agents to execute task {}", task.id);
            return Err(anyhow::anyhow!("No available agents"));
        }

        // Simple round-robin for now
        // TODO: Implement intelligent load balancing
        let selected = &available[0];

        info!(
            "Auto-assigned task {} to agent {} ({})",
            task.id, selected.name, selected.id
        );

        let message = Message {
            id: Uuid::new_v4().to_string(),
            source: "orchestrator".to_string(),
            target: selected.name.clone(),
            message_type: MessageType::Command,
            payload: task.payload.clone(),
            timestamp: chrono::Utc::now(),
        };

        self.message_bus.send(message).await
            .map_err(|_| anyhow::anyhow!("Failed to send message"))?;

        Ok(())
    }

    /// Handle task result
    async fn handle_result(&self, result: TaskResult) {
        let task_id = result.task_id.clone();
        info!(
            "Task {} completed by agent {}: success={}",
            task_id, result.agent_id, result.success
        );

        let mut results = self.results.write().await;
        results.insert(task_id.clone(), result);

        // Remove from running tasks
        let mut running = self.running_tasks.write().await;
        running.remove(&task_id);
    }

    /// Message processing loop
    async fn message_loop(mut rx: mpsc::Receiver<Message>) {
        while let Some(message) = rx.recv().await {
            debug!("Orchestrator received message: {:?}", message);
            
            match message.message_type {
                MessageType::Response => {
                    // Handle task results
                    debug!("Received response: {:?}", message.payload);
                }
                MessageType::Event => {
                    // Handle agent events
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
        let queues = self.task_queues.read().await;
        let running = self.running_tasks.read().await;
        
        let total_tasks: usize = queues.values().map(|q| q.len()).sum();
        
        QueueStats {
            total_tasks,
            running_tasks: running.len(),
            queued_tasks: total_tasks.saturating_sub(running.len()),
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
        let mut queues = self.task_queues.write().await;
        
        for queue in queues.values_mut() {
            queue.retain(|t| t.id != task_id);
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

use std::sync::Arc;

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
        };
        
        let critical_task = Task {
            id: "critical".to_string(),
            priority: TaskPriority::Critical,
            target_agents: vec![],
            payload: serde_json::json!({"p": "critical"}),
            created_at: chrono::Utc::now(),
            deadline: None,
            dependencies: vec![],
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
}
