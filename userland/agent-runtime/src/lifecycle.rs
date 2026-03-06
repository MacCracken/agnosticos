//! Agent Lifecycle Hooks
//!
//! Provides a callback system for agent lifecycle events: start, stop, error,
//! and approval_denied. Hooks can be registered per-agent or globally.
//! Hooks are async and execute in order — a failing hook logs a warning but
//! does not block the lifecycle transition.

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use anyhow::Result;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use agnos_common::{AgentId, AgentStatus, StopReason};

/// Events emitted during agent lifecycle transitions.
#[derive(Debug, Clone)]
pub enum LifecycleEvent {
    /// Agent process is about to start (sandbox applied, binary resolved).
    Starting { agent_id: AgentId, name: String },
    /// Agent process has started successfully.
    Started { agent_id: AgentId, pid: Option<u32> },
    /// Agent is stopping (before SIGTERM).
    Stopping { agent_id: AgentId, reason: StopReason },
    /// Agent has stopped (process exited).
    Stopped { agent_id: AgentId, exit_code: Option<i32> },
    /// Agent encountered a non-fatal error.
    Error { agent_id: AgentId, error: String },
    /// Agent status changed.
    StatusChanged {
        agent_id: AgentId,
        from: AgentStatus,
        to: AgentStatus,
    },
    /// A requested operation was denied by approval system.
    ApprovalDenied {
        agent_id: AgentId,
        operation: String,
    },
    /// Agent is being restarted (after stop, before start).
    Restarting {
        agent_id: AgentId,
        attempt: u32,
    },
}

impl fmt::Display for LifecycleEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Starting { agent_id, name } => {
                write!(f, "agent {} ({}) starting", agent_id, name)
            }
            Self::Started { agent_id, pid } => {
                write!(
                    f,
                    "agent {} started (pid: {})",
                    agent_id,
                    pid.map_or("N/A".to_string(), |p| p.to_string())
                )
            }
            Self::Stopping { agent_id, reason } => {
                write!(f, "agent {} stopping: {:?}", agent_id, reason)
            }
            Self::Stopped { agent_id, exit_code } => {
                write!(
                    f,
                    "agent {} stopped (exit: {})",
                    agent_id,
                    exit_code.map_or("N/A".to_string(), |c| c.to_string())
                )
            }
            Self::Error { agent_id, error } => {
                write!(f, "agent {} error: {}", agent_id, error)
            }
            Self::StatusChanged { agent_id, from, to } => {
                write!(f, "agent {} status: {:?} → {:?}", agent_id, from, to)
            }
            Self::ApprovalDenied { agent_id, operation } => {
                write!(f, "agent {} approval denied: {}", agent_id, operation)
            }
            Self::Restarting { agent_id, attempt } => {
                write!(f, "agent {} restarting (attempt {})", agent_id, attempt)
            }
        }
    }
}

/// A lifecycle hook callback. Returns Ok(()) to continue, Err to log and continue.
pub type HookFn = Arc<dyn Fn(LifecycleEvent) -> Result<()> + Send + Sync>;

/// Manages lifecycle hooks for agents.
pub struct LifecycleManager {
    /// Per-agent hooks: agent_id → list of hooks.
    agent_hooks: RwLock<HashMap<AgentId, Vec<HookFn>>>,
    /// Global hooks (called for every agent).
    global_hooks: RwLock<Vec<HookFn>>,
    /// Event log (last N events for debugging).
    event_log: RwLock<Vec<LifecycleEvent>>,
    /// Maximum events to retain in the log.
    max_log_size: usize,
}

impl LifecycleManager {
    pub fn new() -> Self {
        Self {
            agent_hooks: RwLock::new(HashMap::new()),
            global_hooks: RwLock::new(Vec::new()),
            event_log: RwLock::new(Vec::new()),
            max_log_size: 1000,
        }
    }

    /// Register a hook for a specific agent.
    pub async fn on_agent(&self, agent_id: AgentId, hook: HookFn) {
        let mut hooks = self.agent_hooks.write().await;
        hooks.entry(agent_id).or_default().push(hook);
        debug!("Registered lifecycle hook for agent {}", agent_id);
    }

    /// Register a global hook (fires for all agents).
    pub async fn on_all(&self, hook: HookFn) {
        self.global_hooks.write().await.push(hook);
        debug!("Registered global lifecycle hook");
    }

    /// Remove all hooks for an agent.
    pub async fn remove_agent_hooks(&self, agent_id: AgentId) {
        self.agent_hooks.write().await.remove(&agent_id);
    }

    /// Emit a lifecycle event. Calls all matching hooks and logs the event.
    pub async fn emit(&self, event: LifecycleEvent) {
        info!("Lifecycle: {}", event);

        // Log the event
        {
            let mut log = self.event_log.write().await;
            if log.len() >= self.max_log_size {
                log.remove(0);
            }
            log.push(event.clone());
        }

        // Get the agent_id for hook lookup
        let agent_id = event_agent_id(&event);

        // Call agent-specific hooks
        {
            let hooks = self.agent_hooks.read().await;
            if let Some(agent_hooks) = hooks.get(&agent_id) {
                for hook in agent_hooks {
                    if let Err(e) = hook(event.clone()) {
                        warn!("Lifecycle hook error for agent {}: {}", agent_id, e);
                    }
                }
            }
        }

        // Call global hooks
        {
            let hooks = self.global_hooks.read().await;
            for hook in hooks.iter() {
                if let Err(e) = hook(event.clone()) {
                    warn!("Global lifecycle hook error: {}", e);
                }
            }
        }
    }

    /// Get recent events (most recent last).
    pub async fn recent_events(&self, count: usize) -> Vec<LifecycleEvent> {
        let log = self.event_log.read().await;
        let start = log.len().saturating_sub(count);
        log[start..].to_vec()
    }

    /// Get events for a specific agent.
    pub async fn agent_events(&self, agent_id: AgentId) -> Vec<LifecycleEvent> {
        let log = self.event_log.read().await;
        log.iter()
            .filter(|e| event_agent_id(e) == agent_id)
            .cloned()
            .collect()
    }

    /// Clear the event log.
    pub async fn clear_log(&self) {
        self.event_log.write().await.clear();
    }
}

impl Default for LifecycleManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Extract the agent_id from any lifecycle event.
fn event_agent_id(event: &LifecycleEvent) -> AgentId {
    match event {
        LifecycleEvent::Starting { agent_id, .. }
        | LifecycleEvent::Started { agent_id, .. }
        | LifecycleEvent::Stopping { agent_id, .. }
        | LifecycleEvent::Stopped { agent_id, .. }
        | LifecycleEvent::Error { agent_id, .. }
        | LifecycleEvent::StatusChanged { agent_id, .. }
        | LifecycleEvent::ApprovalDenied { agent_id, .. }
        | LifecycleEvent::Restarting { agent_id, .. } => *agent_id,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[tokio::test]
    async fn test_lifecycle_manager_new() {
        let mgr = LifecycleManager::new();
        let events = mgr.recent_events(10).await;
        assert!(events.is_empty());
    }

    #[tokio::test]
    async fn test_emit_event_logs() {
        let mgr = LifecycleManager::new();
        let id = AgentId::new();

        mgr.emit(LifecycleEvent::Starting {
            agent_id: id,
            name: "test".into(),
        })
        .await;

        let events = mgr.recent_events(10).await;
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], LifecycleEvent::Starting { .. }));
    }

    #[tokio::test]
    async fn test_agent_hook_called() {
        let mgr = LifecycleManager::new();
        let id = AgentId::new();
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        mgr.on_agent(
            id,
            Arc::new(move |_event| {
                counter_clone.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }),
        )
        .await;

        mgr.emit(LifecycleEvent::Started {
            agent_id: id,
            pid: Some(1234),
        })
        .await;

        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_global_hook_called() {
        let mgr = LifecycleManager::new();
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        mgr.on_all(Arc::new(move |_event| {
            counter_clone.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }))
        .await;

        let id1 = AgentId::new();
        let id2 = AgentId::new();

        mgr.emit(LifecycleEvent::Started {
            agent_id: id1,
            pid: None,
        })
        .await;
        mgr.emit(LifecycleEvent::Started {
            agent_id: id2,
            pid: None,
        })
        .await;

        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn test_hook_error_does_not_block() {
        let mgr = LifecycleManager::new();
        let id = AgentId::new();

        mgr.on_agent(
            id,
            Arc::new(|_| Err(anyhow::anyhow!("hook failure"))),
        )
        .await;

        // Should not panic
        mgr.emit(LifecycleEvent::Error {
            agent_id: id,
            error: "test error".into(),
        })
        .await;

        let events = mgr.recent_events(10).await;
        assert_eq!(events.len(), 1);
    }

    #[tokio::test]
    async fn test_remove_agent_hooks() {
        let mgr = LifecycleManager::new();
        let id = AgentId::new();
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        mgr.on_agent(
            id,
            Arc::new(move |_| {
                counter_clone.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }),
        )
        .await;

        mgr.remove_agent_hooks(id).await;

        mgr.emit(LifecycleEvent::Started {
            agent_id: id,
            pid: None,
        })
        .await;

        assert_eq!(counter.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn test_agent_events_filter() {
        let mgr = LifecycleManager::new();
        let id1 = AgentId::new();
        let id2 = AgentId::new();

        mgr.emit(LifecycleEvent::Started {
            agent_id: id1,
            pid: None,
        })
        .await;
        mgr.emit(LifecycleEvent::Started {
            agent_id: id2,
            pid: None,
        })
        .await;
        mgr.emit(LifecycleEvent::Stopped {
            agent_id: id1,
            exit_code: Some(0),
        })
        .await;

        let events = mgr.agent_events(id1).await;
        assert_eq!(events.len(), 2);
    }

    #[tokio::test]
    async fn test_event_log_cap() {
        let mut mgr = LifecycleManager::new();
        mgr.max_log_size = 5;
        let id = AgentId::new();

        for _ in 0..10 {
            mgr.emit(LifecycleEvent::Started {
                agent_id: id,
                pid: None,
            })
            .await;
        }

        let events = mgr.recent_events(100).await;
        assert_eq!(events.len(), 5);
    }

    #[tokio::test]
    async fn test_clear_log() {
        let mgr = LifecycleManager::new();
        let id = AgentId::new();

        mgr.emit(LifecycleEvent::Started {
            agent_id: id,
            pid: None,
        })
        .await;
        mgr.clear_log().await;

        let events = mgr.recent_events(10).await;
        assert!(events.is_empty());
    }

    #[test]
    fn test_lifecycle_event_display() {
        let id = AgentId::new();
        let event = LifecycleEvent::Starting {
            agent_id: id,
            name: "my-agent".into(),
        };
        let s = event.to_string();
        assert!(s.contains("my-agent"));
        assert!(s.contains("starting"));
    }

    #[test]
    fn test_lifecycle_event_display_all_variants() {
        let id = AgentId::new();

        let events = vec![
            LifecycleEvent::Starting { agent_id: id, name: "a".into() },
            LifecycleEvent::Started { agent_id: id, pid: Some(42) },
            LifecycleEvent::Stopping { agent_id: id, reason: StopReason::Normal },
            LifecycleEvent::Stopped { agent_id: id, exit_code: Some(0) },
            LifecycleEvent::Error { agent_id: id, error: "oops".into() },
            LifecycleEvent::StatusChanged {
                agent_id: id,
                from: AgentStatus::Running,
                to: AgentStatus::Stopped,
            },
            LifecycleEvent::ApprovalDenied { agent_id: id, operation: "rm -rf".into() },
            LifecycleEvent::Restarting { agent_id: id, attempt: 3 },
        ];

        for event in events {
            let s = event.to_string();
            assert!(!s.is_empty());
        }
    }

    #[test]
    fn test_event_agent_id() {
        let id = AgentId::new();
        let event = LifecycleEvent::Error {
            agent_id: id,
            error: "test".into(),
        };
        assert_eq!(event_agent_id(&event), id);
    }
}
