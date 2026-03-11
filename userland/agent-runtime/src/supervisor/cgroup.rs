//! Cgroups v2 resource enforcement for agents.

use std::path::PathBuf;

use anyhow::Result;
use tracing::{debug, info};

use agnos_common::AgentId;

/// Base path for AGNOS cgroups v2 hierarchy
pub(super) const CGROUP_BASE: &str = "/sys/fs/cgroup/agnos";

/// Manages cgroups v2 resource enforcement for a single agent
#[derive(Debug)]
pub(super) struct CgroupController {
    pub(super) path: PathBuf,
    pub(super) agent_id: AgentId,
}

impl CgroupController {
    /// Create (or ensure existence of) a cgroup for the given agent.
    pub(super) fn new(agent_id: AgentId) -> Result<Self> {
        let path = PathBuf::from(CGROUP_BASE).join(agent_id.to_string());
        std::fs::create_dir_all(&path).map_err(|e| {
            anyhow::anyhow!("Failed to create cgroup dir {}: {}", path.display(), e)
        })?;
        Ok(Self { path, agent_id })
    }

    /// Try to open an existing cgroup without creating it.
    pub(super) fn open(agent_id: AgentId) -> Option<Self> {
        let path = PathBuf::from(CGROUP_BASE).join(agent_id.to_string());
        if path.is_dir() {
            Some(Self { path, agent_id })
        } else {
            None
        }
    }

    /// Set the hard memory limit (memory.max) in bytes.  0 means "max" (unlimited).
    pub(super) fn set_memory_limit(&self, bytes: u64) -> Result<()> {
        let value = if bytes == 0 {
            "max".to_string()
        } else {
            bytes.to_string()
        };
        std::fs::write(self.path.join("memory.max"), &value)
            .map_err(|e| anyhow::anyhow!("cgroup memory.max write: {}", e))?;
        debug!("Agent {} cgroup memory.max set to {}", self.agent_id, value);
        Ok(())
    }

    /// Set the CPU bandwidth limit (cpu.max).
    /// `quota_us` is the allowed microseconds per `period_us` (default 100 000 µs = 100 ms).
    /// Setting quota_us to 0 means "max" (unlimited).
    pub(super) fn set_cpu_limit(&self, quota_us: u64, period_us: u64) -> Result<()> {
        let value = if quota_us == 0 {
            format!("max {}", period_us)
        } else {
            format!("{} {}", quota_us, period_us)
        };
        std::fs::write(self.path.join("cpu.max"), &value)
            .map_err(|e| anyhow::anyhow!("cgroup cpu.max write: {}", e))?;
        debug!("Agent {} cgroup cpu.max set to {}", self.agent_id, value);
        Ok(())
    }

    /// Add a process to this cgroup.
    pub(super) fn add_pid(&self, pid: u32) -> Result<()> {
        std::fs::write(self.path.join("cgroup.procs"), pid.to_string())
            .map_err(|e| anyhow::anyhow!("cgroup add pid {}: {}", pid, e))?;
        info!("Agent {} added pid {} to cgroup", self.agent_id, pid);
        Ok(())
    }

    /// Read current memory usage from memory.current (bytes).
    pub(super) fn memory_current(&self) -> u64 {
        std::fs::read_to_string(self.path.join("memory.current"))
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0)
    }

    /// Read the configured memory limit from memory.max (bytes).
    #[cfg(test)]
    pub(super) fn memory_max(&self) -> Option<u64> {
        let s = std::fs::read_to_string(self.path.join("memory.max")).ok()?;
        let trimmed = s.trim();
        if trimmed == "max" {
            None // unlimited
        } else {
            trimmed.parse().ok()
        }
    }

    /// Read CPU usage from cpu.stat (usage_usec field).
    pub(super) fn cpu_usage_usec(&self) -> u64 {
        std::fs::read_to_string(self.path.join("cpu.stat"))
            .ok()
            .and_then(|contents| {
                for line in contents.lines() {
                    if let Some(val) = line.strip_prefix("usage_usec ") {
                        return val.trim().parse().ok();
                    }
                }
                None
            })
            .unwrap_or(0)
    }

    /// Read the set of PIDs in this cgroup.
    #[cfg(test)]
    pub(super) fn pids(&self) -> Vec<u32> {
        std::fs::read_to_string(self.path.join("cgroup.procs"))
            .ok()
            .map(|s| s.lines().filter_map(|l| l.trim().parse().ok()).collect())
            .unwrap_or_default()
    }

    /// Remove the cgroup directory (must be empty of processes first).
    pub(super) fn destroy(&self) -> Result<()> {
        if self.path.is_dir() {
            std::fs::remove_dir(&self.path)
                .map_err(|e| anyhow::anyhow!("cgroup destroy {}: {}", self.path.display(), e))?;
            debug!("Destroyed cgroup for agent {}", self.agent_id);
        }
        Ok(())
    }
}
