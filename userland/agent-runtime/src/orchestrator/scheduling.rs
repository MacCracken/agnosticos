//! Task scheduling — scheduler loop, message processing, periodic maintenance.

use std::sync::Arc;

use tokio::sync::RwLock;
use tracing::{debug, error, info};

use agnos_common::{Message, MessageType};

use super::types::{OrchestratorState, TaskPriority, TaskResult};
use super::Orchestrator;

impl Orchestrator {
    /// How often (in scheduler ticks) to proactively prune results.
    /// With a 100 ms tick interval this triggers roughly every 10 seconds.
    pub(crate) const PRUNE_INTERVAL_TICKS: u64 = 100;

    /// Message processing loop — receives messages from agents and processes
    /// task results, routing them into the shared state.
    pub(crate) async fn message_loop(
        mut rx: tokio::sync::mpsc::Receiver<Message>,
        state: Arc<RwLock<OrchestratorState>>,
    ) {
        while let Some(message) = rx.recv().await {
            debug!("Orchestrator received message: {:?}", message);

            match message.message_type {
                MessageType::Response => {
                    if let Ok(result) =
                        serde_json::from_value::<TaskResult>(message.payload.clone())
                    {
                        let task_id = result.task_id.clone();
                        info!(
                            "Task {} completed by agent {}: success={}",
                            task_id, result.agent_id, result.success
                        );

                        let mut s = state.write().await;
                        s.results.insert(task_id.clone(), result);
                        Self::prune_results(&mut s.results);
                        s.running_tasks.remove(&task_id);
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
    pub(crate) async fn scheduler_loop(orchestrator: Self) {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(100));
        let mut tick_count: u64 = 0;

        loop {
            interval.tick().await;
            tick_count = tick_count.wrapping_add(1);

            // Proactive pruning — ensures results map stays bounded even
            // when no new results arrive (e.g., long-running tasks).
            if tick_count % Self::PRUNE_INTERVAL_TICKS == 0 {
                let mut state = orchestrator.state.write().await;
                Self::prune_results(&mut state.results);
            }

            let priorities = [
                TaskPriority::Critical,
                TaskPriority::High,
                TaskPriority::Normal,
                TaskPriority::Low,
                TaskPriority::Background,
            ];

            for priority in &priorities {
                let task_to_distribute = {
                    let mut state = orchestrator.state.write().await;

                    let task = match state.task_queues.get_mut(priority) {
                        Some(q) => q.pop_front(),
                        None => continue,
                    };

                    if let Some(task) = task {
                        if !task.dependencies.is_empty() {
                            let deps_satisfied = task
                                .dependencies
                                .iter()
                                .all(|dep_id| state.results.contains_key(dep_id));

                            if !deps_satisfied {
                                debug!("Task {} has unsatisfied dependencies, deferring", task.id);
                                // Push back -- re-borrow queue now that results borrow is done
                                if let Some(q) = state.task_queues.get_mut(priority) {
                                    q.push_back(task);
                                }
                                continue;
                            }
                        }

                        state.queued_task_ids.remove(&task.id);
                        state.running_tasks.insert(task.id.clone(), task.clone());

                        Some(task)
                    } else {
                        None
                    }
                };

                if let Some(task) = task_to_distribute {
                    if let Err(e) = orchestrator.distribute_task(&task).await {
                        error!("Failed to distribute task {}: {}", task.id, e);
                    }
                }
            }
        }
    }
}
