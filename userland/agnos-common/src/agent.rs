//! Agent-related types and traits

use crate::{AgentId, AgentStatus, ResourceUsage};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Agent information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInfo {
    pub id: AgentId,
    pub name: String,
    pub status: AgentStatus,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub resource_usage: ResourceUsage,
    pub metadata: HashMap<String, String>,
}

/// Agent statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentStats {
    pub total_tasks_completed: u64,
    pub total_tasks_failed: u64,
    pub total_inference_calls: u64,
    pub total_file_operations: u64,
    pub total_network_operations: u64,
}

/// Agent event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentEvent {
    Started {
        agent_id: AgentId,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    Stopped {
        agent_id: AgentId,
        timestamp: chrono::DateTime<chrono::Utc>,
        reason: StopReason,
    },
    TaskCompleted {
        agent_id: AgentId,
        task_id: String,
        duration_ms: u64,
    },
    TaskFailed {
        agent_id: AgentId,
        task_id: String,
        error: String,
    },
    ResourceLimitExceeded {
        agent_id: AgentId,
        resource: String,
        limit: u64,
        actual: u64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StopReason {
    Normal,
    Error(String),
    UserRequest,
    ResourceLimit,
    Timeout,
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_agent_info() {
        let info = AgentInfo {
            id: AgentId::new(),
            name: "test-agent".to_string(),
            status: AgentStatus::Running,
            created_at: chrono::Utc::now(),
            started_at: Some(chrono::Utc::now()),
            resource_usage: ResourceUsage::default(),
            metadata: HashMap::new(),
        };
        assert_eq!(info.name, "test-agent");
        assert_eq!(info.status, AgentStatus::Running);
    }

    #[test]
    fn test_agent_stats_default() {
        let stats = AgentStats::default();
        assert_eq!(stats.total_tasks_completed, 0);
        assert_eq!(stats.total_tasks_failed, 0);
    }

    #[test]
    fn test_agent_stats_values() {
        let mut stats = AgentStats::default();
        stats.total_tasks_completed = 100;
        stats.total_tasks_failed = 5;
        stats.total_inference_calls = 50;

        assert_eq!(stats.total_tasks_completed, 100);
        assert_eq!(stats.total_tasks_failed, 5);
        assert_eq!(stats.total_inference_calls, 50);
    }

    #[test]
    fn test_agent_event_started() {
        let event = AgentEvent::Started {
            agent_id: AgentId::new(),
            timestamp: chrono::Utc::now(),
        };

        if let AgentEvent::Started { agent_id, .. } = event {
            assert_ne!(agent_id.0, Uuid::nil());
        } else {
            panic!("Expected Started event");
        }
    }

    #[test]
    fn test_agent_event_stopped() {
        let event = AgentEvent::Stopped {
            agent_id: AgentId::new(),
            timestamp: chrono::Utc::now(),
            reason: StopReason::Normal,
        };

        if let AgentEvent::Stopped { reason, .. } = event {
            assert!(matches!(reason, StopReason::Normal));
        } else {
            panic!("Expected Stopped event");
        }
    }

    #[test]
    fn test_agent_event_task_completed() {
        let event = AgentEvent::TaskCompleted {
            agent_id: AgentId::new(),
            task_id: "task-123".to_string(),
            duration_ms: 1500,
        };

        if let AgentEvent::TaskCompleted {
            task_id,
            duration_ms,
            ..
        } = event
        {
            assert_eq!(task_id, "task-123");
            assert_eq!(duration_ms, 1500);
        } else {
            panic!("Expected TaskCompleted event");
        }
    }

    #[test]
    fn test_agent_event_task_failed() {
        let event = AgentEvent::TaskFailed {
            agent_id: AgentId::new(),
            task_id: "task-456".to_string(),
            error: "Out of memory".to_string(),
        };

        if let AgentEvent::TaskFailed { error, .. } = event {
            assert_eq!(error, "Out of memory");
        } else {
            panic!("Expected TaskFailed event");
        }
    }

    #[test]
    fn test_agent_event_resource_limit() {
        let event = AgentEvent::ResourceLimitExceeded {
            agent_id: AgentId::new(),
            resource: "memory".to_string(),
            limit: 1024,
            actual: 2048,
        };

        if let AgentEvent::ResourceLimitExceeded {
            resource,
            limit,
            actual,
            ..
        } = event
        {
            assert_eq!(resource, "memory");
            assert_eq!(limit, 1024);
            assert_eq!(actual, 2048);
        } else {
            panic!("Expected ResourceLimitExceeded event");
        }
    }

    #[test]
    fn test_stop_reason_variants() {
        assert!(matches!(StopReason::Normal, StopReason::Normal));
        assert!(matches!(StopReason::UserRequest, StopReason::UserRequest));
        assert!(matches!(
            StopReason::ResourceLimit,
            StopReason::ResourceLimit
        ));

        if let StopReason::Error(msg) = StopReason::Error("test error".to_string()) {
            assert_eq!(msg, "test error");
        } else {
            panic!("Expected Error variant");
        }
    }
}
