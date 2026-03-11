//! Resource quota thresholds and health types for agent supervision.

use std::time::Instant;

use agnos_common::{AgentId, ResourceUsage};

/// Configurable resource quota thresholds for an agent.
///
/// These thresholds control when the supervisor takes action against an agent
/// that is approaching or exceeding its resource limits.
#[derive(Debug, Clone)]
pub struct ResourceQuota {
    /// Memory usage percentage of limit at which a warning is emitted (default 80%).
    pub memory_warn_pct: f64,
    /// Memory usage percentage of limit at which the agent is killed (default 95%).
    pub memory_kill_pct: f64,
    /// CPU usage rate percentage (of one core) at which a throttling warning is emitted (default 90%).
    pub cpu_throttle_pct: f64,
    /// The configured memory limit in bytes (from AgentConfig).
    pub memory_limit: u64,
    /// The configured CPU time limit in ms (from AgentConfig).
    pub cpu_time_limit: u64,
}

impl Default for ResourceQuota {
    fn default() -> Self {
        Self {
            memory_warn_pct: 80.0,
            memory_kill_pct: 95.0,
            cpu_throttle_pct: 90.0,
            memory_limit: 0,
            cpu_time_limit: 0,
        }
    }
}

impl ResourceQuota {
    /// Create a quota from agent resource limits with default thresholds.
    pub fn from_limits(memory_limit: u64, cpu_time_limit: u64) -> Self {
        Self {
            memory_limit,
            cpu_time_limit,
            ..Self::default()
        }
    }
}

/// Health check configuration
#[derive(Debug, Clone)]
pub struct HealthCheckConfig {
    pub interval: std::time::Duration,
    pub timeout: std::time::Duration,
    pub unhealthy_threshold: u32,
    pub healthy_threshold: u32,
}

impl Default for HealthCheckConfig {
    fn default() -> Self {
        Self {
            interval: std::time::Duration::from_secs(30),
            timeout: std::time::Duration::from_secs(5),
            unhealthy_threshold: 3,
            healthy_threshold: 2,
        }
    }
}

/// Agent health status
#[derive(Debug, Clone)]
pub struct AgentHealth {
    pub agent_id: AgentId,
    pub is_healthy: bool,
    pub consecutive_failures: u32,
    pub consecutive_successes: u32,
    pub last_check: Instant,
    pub last_response_time_ms: u64,
    pub resource_usage: ResourceUsage,
}
