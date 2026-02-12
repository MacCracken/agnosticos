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
