//! Multi-Agent Orchestrator
//!
//! Handles agent coordination, task distribution, workload balancing, and conflict resolution.
//!
//! Submodules:
//! - **types**: Task, priority, result, and state types
//! - **lifecycle**: Task submission, cancellation, status, and result management
//! - **scheduling**: Scheduler loop, message processing, periodic maintenance
//! - **scoring**: Load-aware agent scoring and auto-assignment
//! - **routing**: Task distribution and broadcasting
//! - **state**: Queue statistics and query operations

pub mod types;

mod lifecycle;
mod routing;
mod scheduling;
mod scoring;
mod state;

#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

use anyhow::Result;
use tokio::sync::{mpsc, RwLock};
use tracing::info;

use crate::registry::AgentRegistry;

pub use types::{
    AgentOrchestratorStats, OrchestratorState, QueueStats, Task, TaskPriority, TaskRequirements,
    TaskResult, TaskStatus,
};

/// Orchestrator for multi-agent coordination
///
/// All mutable task-lifecycle state lives in a single `Arc<RwLock<OrchestratorState>>`
/// so that the orchestrator can be cheaply cloned and passed to background tasks
/// (e.g. the scheduler loop) while still sharing the same underlying data.
#[derive(Clone)]
pub struct Orchestrator {
    pub(crate) registry: Arc<AgentRegistry>,
    /// Unified mutable state (shared across clones)
    pub(crate) state: Arc<RwLock<OrchestratorState>>,
    /// Communication bus sender (cheap to clone)
    pub(crate) message_bus: mpsc::Sender<agnos_common::Message>,
    /// Receiver held until `start()` spawns the message loop.
    /// Kept separate from `OrchestratorState` because it is a one-shot take
    /// used only in `start()` and would needlessly widen the hot-path lock.
    pub(crate) message_rx: Arc<RwLock<Option<mpsc::Receiver<agnos_common::Message>>>>,
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
            state: Arc::new(RwLock::new(OrchestratorState {
                task_queues: queues,
                running_tasks: HashMap::new(),
                results: HashMap::new(),
                queued_task_ids: HashSet::new(),
            })),
            message_bus,
            message_rx: Arc::new(RwLock::new(Some(message_rx))),
        }
    }

    /// Start the orchestrator
    pub async fn start(&self) -> Result<()> {
        info!("Starting multi-agent orchestrator...");

        // Start the message processing loop with shared state
        if let Some(rx) = self.message_rx.write().await.take() {
            let state = self.state.clone();
            tokio::spawn(Self::message_loop(rx, state));
        }

        // Start the task scheduler
        tokio::spawn(Self::scheduler_loop(self.clone()));

        info!("Multi-agent orchestrator started");
        Ok(())
    }
}
