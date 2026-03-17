//! State queries — queue stats, peek, overdue tasks, agent stats.

use super::types::{AgentOrchestratorStats, QueueStats, Task, TaskPriority};
use super::Orchestrator;

impl Orchestrator {
    /// Get queue statistics (for testing)
    pub async fn get_queue_stats(&self) -> QueueStats {
        let state = self.state.read().await;
        let queued_tasks: usize = state.task_queues.values().map(|q| q.len()).sum();
        let running_tasks = state.running_tasks.len();

        QueueStats {
            total_tasks: queued_tasks + running_tasks,
            running_tasks,
            queued_tasks,
        }
    }

    /// Peek at next task (for testing)
    pub async fn peek_next_task(&self) -> Option<Task> {
        let state = self.state.read().await;

        for priority in [
            TaskPriority::Critical,
            TaskPriority::High,
            TaskPriority::Normal,
            TaskPriority::Low,
            TaskPriority::Background,
        ] {
            if let Some(queue) = state.task_queues.get(&priority) {
                if let Some(task) = queue.front() {
                    return Some(task.clone());
                }
            }
        }
        None
    }

    /// Get overdue tasks (for testing)
    pub async fn get_overdue_tasks(&self) -> Vec<Task> {
        let state = self.state.read().await;
        let now = chrono::Utc::now();

        let mut overdue = Vec::new();
        for queue in state.task_queues.values() {
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
        let state = self.state.read().await;

        AgentOrchestratorStats {
            registered_agents: agents.len(),
            total_tasks_processed: state.results.len(),
        }
    }
}
