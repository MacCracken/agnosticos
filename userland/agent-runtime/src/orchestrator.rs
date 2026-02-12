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
}

/// Task status
#[derive(Debug, Clone)]
pub enum TaskStatus {
    Queued,
    Running,
    Completed(TaskResult),
}

use std::sync::Arc;
