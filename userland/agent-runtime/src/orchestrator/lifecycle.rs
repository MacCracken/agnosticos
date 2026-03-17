//! Task lifecycle — submission, cancellation, status, results, pruning.

use std::collections::HashMap;

use anyhow::{Context, Result};
use tracing::info;
use uuid::Uuid;

use super::types::{Task, TaskResult, TaskStatus};
use super::Orchestrator;

impl Orchestrator {
    /// Maximum number of completed task results to retain.
    pub(crate) const MAX_RESULTS: usize = 10_000;

    /// Submit a new task for execution
    pub async fn submit_task(&self, mut task: Task) -> Result<String> {
        task.id = Uuid::new_v4().to_string();
        task.created_at = chrono::Utc::now();

        info!(
            "Submitting task {} with priority {:?}",
            task.id, task.priority
        );

        let mut state = self.state.write().await;
        state.queued_task_ids.insert(task.id.clone());
        state
            .task_queues
            .get_mut(&task.priority)
            .context("Invalid task priority")?
            .push_back(task.clone());

        Ok(task.id)
    }

    /// Get task status
    pub async fn get_task_status(&self, task_id: &str) -> Option<TaskStatus> {
        let state = self.state.read().await;

        if state.running_tasks.contains_key(task_id) {
            return Some(TaskStatus::Running);
        }

        if let Some(result) = state.results.get(task_id) {
            return Some(TaskStatus::Completed(result.clone()));
        }

        if state.queued_task_ids.contains(task_id) {
            return Some(TaskStatus::Queued);
        }

        None
    }

    /// Get task result
    pub async fn get_result(&self, task_id: &str) -> Option<TaskResult> {
        self.state.read().await.results.get(task_id).cloned()
    }

    /// Handle task result and prune old results to prevent unbounded growth.
    /// Also releases any GPU allocation held by the completing agent.
    pub(crate) async fn handle_result(&self, result: TaskResult) {
        let task_id = result.task_id.clone();
        let agent_id = result.agent_id;
        info!(
            "Task {} completed by agent {}: success={}",
            task_id, agent_id, result.success
        );

        // Release GPU allocation if resource manager is attached.
        // We release on every completion — release_gpu is a no-op if
        // the agent has no GPU allocation.
        if let Some(ref rm) = self.resource_manager {
            if let Err(e) = rm.release_gpu(agent_id).await {
                tracing::warn!(
                    "Failed to release GPU for agent {} after task {}: {}",
                    agent_id,
                    task_id,
                    e
                );
            }
        }

        let mut state = self.state.write().await;
        state.results.insert(task_id.clone(), result);
        Self::prune_results(&mut state.results);
        state.running_tasks.remove(&task_id);
    }

    /// Prune results map if it exceeds MAX_RESULTS, keeping the most recent.
    /// Uses O(n) partial sort instead of O(n log n) full sort.
    pub(crate) fn prune_results(results: &mut HashMap<String, TaskResult>) {
        let excess = results.len().saturating_sub(Self::MAX_RESULTS);
        if excess == 0 {
            return;
        }
        let mut entries: Vec<_> = results
            .iter()
            .map(|(k, v)| (k.clone(), v.completed_at))
            .collect();
        entries.select_nth_unstable_by_key(excess - 1, |(_, t)| *t);
        for (key, _) in &entries[..excess] {
            results.remove(key);
        }
    }

    /// Store task result (for testing)
    pub async fn store_result(&self, result: TaskResult) -> Result<()> {
        self.handle_result(result).await;
        Ok(())
    }

    /// Cancel a task (for testing)
    pub async fn cancel_task(&self, task_id: &str) -> Result<()> {
        let mut state = self.state.write().await;
        for queue in state.task_queues.values_mut() {
            queue.retain(|t| t.id != task_id);
        }
        state.queued_task_ids.remove(task_id);
        state.running_tasks.remove(task_id);

        Ok(())
    }
}
