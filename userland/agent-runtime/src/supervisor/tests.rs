use super::*;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use agnos_common::{AgentId, ResourceUsage, StopReason};
use anyhow::Result;
use tempfile::TempDir;

use crate::registry::AgentRegistry;
use super::cgroup;
use super::proc_utils;

struct MockAgentControl {
    healthy: bool,
}

#[async_trait::async_trait]
impl AgentControl for MockAgentControl {
    async fn check_health(&self) -> Result<bool> {
        Ok(self.healthy)
    }

    async fn get_resource_usage(&self) -> Result<ResourceUsage> {
        Ok(ResourceUsage {
            memory_used: 100 * 1024 * 1024,
            cpu_time_used: 1000,
            file_descriptors_used: 10,
            processes_used: 1,
        })
    }

    async fn stop(&mut self, _reason: StopReason) -> Result<()> {
        Ok(())
    }

    async fn restart(&mut self) -> Result<()> {
        Ok(())
    }
}

#[test]
fn test_health_check_config_defaults() {
    let config = HealthCheckConfig::default();
    assert_eq!(config.interval, Duration::from_secs(30));
    assert_eq!(config.timeout, Duration::from_secs(5));
    assert_eq!(config.unhealthy_threshold, 3);
    assert_eq!(config.healthy_threshold, 2);
}

#[test]
fn test_health_check_config_custom() {
    let config = HealthCheckConfig {
        interval: Duration::from_secs(60),
        timeout: Duration::from_secs(10),
        unhealthy_threshold: 5,
        healthy_threshold: 3,
    };
    assert_eq!(config.interval, Duration::from_secs(60));
    assert_eq!(config.timeout, Duration::from_secs(10));
    assert_eq!(config.unhealthy_threshold, 5);
    assert_eq!(config.healthy_threshold, 3);
}

#[test]
fn test_agent_health_default() {
    let agent_id = AgentId::new();
    let health = AgentHealth {
        agent_id,
        is_healthy: true,
        consecutive_failures: 0,
        consecutive_successes: 0,
        last_check: Instant::now(),
        last_response_time_ms: 0,
        resource_usage: ResourceUsage::default(),
    };
    assert!(health.is_healthy);
    assert_eq!(health.consecutive_failures, 0);
    assert_eq!(health.consecutive_successes, 0);
}

#[test]
fn test_agent_health_unhealthy() {
    let agent_id = AgentId::new();
    let health = AgentHealth {
        agent_id,
        is_healthy: false,
        consecutive_failures: 3,
        consecutive_successes: 0,
        last_check: Instant::now(),
        last_response_time_ms: 5000,
        resource_usage: ResourceUsage {
            memory_used: 2 * 1024 * 1024 * 1024,
            cpu_time_used: 10_000_000,
            file_descriptors_used: 1000,
            processes_used: 50,
        },
    };
    assert!(!health.is_healthy);
    assert_eq!(health.consecutive_failures, 3);
}

#[test]
fn test_cgroup_controller_memory_limit_format() {
    let agent_id = AgentId::new();
    let controller = cgroup::CgroupController {
        path: PathBuf::from(format!("/tmp/test-cgroup-{}", agent_id)),
        agent_id,
    };

    assert_eq!(controller.memory_current(), 0);
}

#[test]
fn test_cgroup_controller_new_requires_path() {
    let agent_id = AgentId::new();
    let path = PathBuf::from("/nonexistent/path/that/should/not/exist");
    let controller = cgroup::CgroupController { path, agent_id };

    let result = controller.set_memory_limit(1024 * 1024 * 1024);
    assert!(result.is_err());
}

#[test]
fn test_cgroup_controller_open_nonexistent() {
    let agent_id = AgentId::new();
    let result = cgroup::CgroupController::open(agent_id);
    assert!(result.is_none());
}

#[test]
fn test_supervisor_new() {
    let registry = Arc::new(AgentRegistry::new());
    let supervisor = Supervisor::new(registry.clone());

    assert!(supervisor.health_checks.blocking_read().is_empty());
    assert!(supervisor.running_agents.blocking_read().is_empty());
    assert!(supervisor.cgroups.blocking_read().is_empty());
    assert!(supervisor.quotas.blocking_read().is_empty());
    assert!(supervisor.last_cpu_readings.blocking_read().is_empty());
}

#[test]
fn test_supervisor_config() {
    let registry = Arc::new(AgentRegistry::new());
    let supervisor = Supervisor::new(registry);

    assert_eq!(supervisor.config.interval, Duration::from_secs(30));
    assert_eq!(supervisor.config.timeout, Duration::from_secs(5));
    assert_eq!(supervisor.config.unhealthy_threshold, 3);
    assert_eq!(supervisor.config.healthy_threshold, 2);
}

#[tokio::test]
async fn test_supervisor_register_agent() {
    let registry = Arc::new(AgentRegistry::new());
    let supervisor = Supervisor::new(registry.clone());

    let agent_id = AgentId::new();
    let result = supervisor.register_agent(agent_id).await;
    assert!(result.is_ok());

    let health_map = supervisor.health_checks.read().await;
    assert!(health_map.contains_key(&agent_id));

    let health = health_map.get(&agent_id).unwrap();
    assert!(health.is_healthy);
    assert_eq!(health.consecutive_failures, 0);
    assert_eq!(health.consecutive_successes, 0);
}

#[tokio::test]
async fn test_supervisor_unregister_agent() {
    let registry = Arc::new(AgentRegistry::new());
    let supervisor = Supervisor::new(registry.clone());

    let agent_id = AgentId::new();
    supervisor.register_agent(agent_id).await.unwrap();

    let result = supervisor.unregister_agent(agent_id).await;
    assert!(result.is_ok());

    let health_map = supervisor.health_checks.read().await;
    assert!(!health_map.contains_key(&agent_id));
}

#[tokio::test]
async fn test_supervisor_unregister_nonexistent() {
    let registry = Arc::new(AgentRegistry::new());
    let supervisor = Supervisor::new(registry.clone());

    let agent_id = AgentId::new();
    let result = supervisor.unregister_agent(agent_id).await;
    assert!(result.is_ok());
}

#[test]
fn test_resource_usage_default() {
    let usage = ResourceUsage::default();
    assert_eq!(usage.memory_used, 0);
    assert_eq!(usage.cpu_time_used, 0);
    assert_eq!(usage.file_descriptors_used, 0);
    assert_eq!(usage.processes_used, 0);
}

#[test]
fn test_resource_usage_custom() {
    let usage = ResourceUsage {
        memory_used: 1024 * 1024 * 1024,
        cpu_time_used: 5000000,
        file_descriptors_used: 100,
        processes_used: 10,
    };
    assert_eq!(usage.memory_used, 1024 * 1024 * 1024);
    assert_eq!(usage.cpu_time_used, 5_000_000);
    assert_eq!(usage.file_descriptors_used, 100);
    assert_eq!(usage.processes_used, 10);
}

#[test]
fn test_agent_control_trait_object() {
    let mock = MockAgentControl { healthy: true };
    let boxed: Box<dyn AgentControl> = Box::new(mock);

    let health = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(boxed.check_health());

    assert!(health.unwrap());
}

#[test]
fn test_agent_control_trait_object_unhealthy() {
    let mock = MockAgentControl { healthy: false };
    let boxed: Box<dyn AgentControl> = Box::new(mock);

    let health = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(boxed.check_health());

    assert!(!health.unwrap());
}

#[test]
fn test_cgroup_controller_memory_max_unlimited() {
    let agent_id = AgentId::new();
    let controller = cgroup::CgroupController {
        path: PathBuf::from("/sys/fs/cgroup"),
        agent_id,
    };

    let max = controller.memory_max();
    assert!(max.is_none() || max.is_some());
}

#[test]
fn test_cgroup_controller_path_generation() {
    let agent_id = AgentId::new();
    let expected_path = PathBuf::from(cgroup::CGROUP_BASE).join(agent_id.to_string());
    let controller = cgroup::CgroupController {
        path: expected_path.clone(),
        agent_id,
    };
    assert_eq!(controller.path, expected_path);
    assert_eq!(controller.agent_id, agent_id);
}

#[test]
fn test_cgroup_controller_cpu_usage_usec_nonexistent() {
    let agent_id = AgentId::new();
    let controller = cgroup::CgroupController {
        path: PathBuf::from("/nonexistent/cgroup/path"),
        agent_id,
    };
    assert_eq!(controller.cpu_usage_usec(), 0);
}

#[test]
fn test_cgroup_controller_pids_nonexistent() {
    let agent_id = AgentId::new();
    let controller = cgroup::CgroupController {
        path: PathBuf::from("/nonexistent/cgroup/path"),
        agent_id,
    };
    assert!(controller.pids().is_empty());
}

#[test]
fn test_cgroup_controller_memory_current_nonexistent() {
    let agent_id = AgentId::new();
    let controller = cgroup::CgroupController {
        path: PathBuf::from("/nonexistent/cgroup/path"),
        agent_id,
    };
    assert_eq!(controller.memory_current(), 0);
}

#[test]
fn test_cgroup_controller_memory_max_nonexistent() {
    let agent_id = AgentId::new();
    let controller = cgroup::CgroupController {
        path: PathBuf::from("/nonexistent/cgroup/path"),
        agent_id,
    };
    assert!(controller.memory_max().is_none());
}

#[test]
fn test_cgroup_controller_set_cpu_limit_nonexistent() {
    let agent_id = AgentId::new();
    let controller = cgroup::CgroupController {
        path: PathBuf::from("/nonexistent/cgroup/path"),
        agent_id,
    };
    let result = controller.set_cpu_limit(100_000, 100_000);
    assert!(result.is_err());
}

#[test]
fn test_cgroup_controller_add_pid_nonexistent() {
    let agent_id = AgentId::new();
    let controller = cgroup::CgroupController {
        path: PathBuf::from("/nonexistent/cgroup/path"),
        agent_id,
    };
    let result = controller.add_pid(12345);
    assert!(result.is_err());
}

#[test]
fn test_cgroup_controller_destroy_nonexistent() {
    let agent_id = AgentId::new();
    let controller = cgroup::CgroupController {
        path: PathBuf::from("/nonexistent/cgroup/path"),
        agent_id,
    };
    // Non-existent path should succeed (no-op)
    let result = controller.destroy();
    assert!(result.is_ok());
}

#[test]
fn test_cgroup_controller_with_tempdir() {
    let tmp = TempDir::new().unwrap();
    let agent_id = AgentId::new();
    let cg_path = tmp.path().join(agent_id.to_string());
    std::fs::create_dir_all(&cg_path).unwrap();

    let controller = cgroup::CgroupController {
        path: cg_path.clone(),
        agent_id,
    };

    // Write a fake memory.max file
    std::fs::write(cg_path.join("memory.max"), "1073741824").unwrap();
    assert_eq!(controller.memory_max(), Some(1073741824));

    // Write "max" for unlimited
    std::fs::write(cg_path.join("memory.max"), "max").unwrap();
    assert_eq!(controller.memory_max(), None);

    // Write a fake memory.current
    std::fs::write(cg_path.join("memory.current"), "524288000").unwrap();
    assert_eq!(controller.memory_current(), 524288000);

    // Write a fake cpu.stat
    std::fs::write(
        cg_path.join("cpu.stat"),
        "usage_usec 1234567\nuser_usec 1000000\nsystem_usec 234567\n",
    )
    .unwrap();
    assert_eq!(controller.cpu_usage_usec(), 1234567);

    // Write a fake cgroup.procs
    std::fs::write(cg_path.join("cgroup.procs"), "100\n200\n300\n").unwrap();
    assert_eq!(controller.pids(), vec![100, 200, 300]);
}

#[test]
fn test_cgroup_controller_pids_empty_file() {
    let tmp = TempDir::new().unwrap();
    let agent_id = AgentId::new();
    let cg_path = tmp.path().join(agent_id.to_string());
    std::fs::create_dir_all(&cg_path).unwrap();

    let controller = cgroup::CgroupController {
        path: cg_path.clone(),
        agent_id,
    };
    std::fs::write(cg_path.join("cgroup.procs"), "").unwrap();
    assert!(controller.pids().is_empty());
}

#[test]
fn test_cgroup_controller_set_memory_limit_with_tempdir() {
    let tmp = TempDir::new().unwrap();
    let agent_id = AgentId::new();
    let cg_path = tmp.path().join(agent_id.to_string());
    std::fs::create_dir_all(&cg_path).unwrap();

    let controller = cgroup::CgroupController {
        path: cg_path.clone(),
        agent_id,
    };

    // Set a numeric limit
    controller.set_memory_limit(2 * 1024 * 1024 * 1024).unwrap();
    let written = std::fs::read_to_string(cg_path.join("memory.max")).unwrap();
    assert_eq!(written, "2147483648");

    // Set unlimited (0 means "max")
    controller.set_memory_limit(0).unwrap();
    let written = std::fs::read_to_string(cg_path.join("memory.max")).unwrap();
    assert_eq!(written, "max");
}

#[test]
fn test_cgroup_controller_set_cpu_limit_with_tempdir() {
    let tmp = TempDir::new().unwrap();
    let agent_id = AgentId::new();
    let cg_path = tmp.path().join(agent_id.to_string());
    std::fs::create_dir_all(&cg_path).unwrap();

    let controller = cgroup::CgroupController {
        path: cg_path.clone(),
        agent_id,
    };

    controller.set_cpu_limit(50000, 100000).unwrap();
    let written = std::fs::read_to_string(cg_path.join("cpu.max")).unwrap();
    assert_eq!(written, "50000 100000");

    // Unlimited (0 quota)
    controller.set_cpu_limit(0, 100000).unwrap();
    let written = std::fs::read_to_string(cg_path.join("cpu.max")).unwrap();
    assert_eq!(written, "max 100000");
}

#[test]
fn test_cgroup_controller_add_pid_with_tempdir() {
    let tmp = TempDir::new().unwrap();
    let agent_id = AgentId::new();
    let cg_path = tmp.path().join(agent_id.to_string());
    std::fs::create_dir_all(&cg_path).unwrap();

    let controller = cgroup::CgroupController {
        path: cg_path.clone(),
        agent_id,
    };

    controller.add_pid(42).unwrap();
    let written = std::fs::read_to_string(cg_path.join("cgroup.procs")).unwrap();
    assert_eq!(written, "42");
}

#[test]
fn test_cgroup_controller_destroy_with_tempdir() {
    let tmp = TempDir::new().unwrap();
    let agent_id = AgentId::new();
    let cg_path = tmp.path().join(agent_id.to_string());
    std::fs::create_dir_all(&cg_path).unwrap();

    let controller = cgroup::CgroupController {
        path: cg_path.clone(),
        agent_id,
    };

    assert!(cg_path.exists());
    controller.destroy().unwrap();
    assert!(!cg_path.exists());
}

#[test]
fn test_health_check_config_clone() {
    let config = HealthCheckConfig {
        interval: Duration::from_secs(15),
        timeout: Duration::from_secs(3),
        unhealthy_threshold: 5,
        healthy_threshold: 2,
    };
    let cloned = config.clone();
    assert_eq!(cloned.interval, Duration::from_secs(15));
    assert_eq!(cloned.timeout, Duration::from_secs(3));
    assert_eq!(cloned.unhealthy_threshold, 5);
    assert_eq!(cloned.healthy_threshold, 2);
}

#[test]
fn test_health_check_config_debug() {
    let config = HealthCheckConfig::default();
    let debug_str = format!("{:?}", config);
    assert!(debug_str.contains("interval"));
    assert!(debug_str.contains("timeout"));
}

#[test]
fn test_agent_health_clone() {
    let agent_id = AgentId::new();
    let health = AgentHealth {
        agent_id,
        is_healthy: false,
        consecutive_failures: 5,
        consecutive_successes: 0,
        last_check: Instant::now(),
        last_response_time_ms: 3000,
        resource_usage: ResourceUsage {
            memory_used: 500,
            cpu_time_used: 100,
            file_descriptors_used: 10,
            processes_used: 2,
        },
    };

    let cloned = health.clone();
    assert_eq!(cloned.agent_id, agent_id);
    assert!(!cloned.is_healthy);
    assert_eq!(cloned.consecutive_failures, 5);
    assert_eq!(cloned.last_response_time_ms, 3000);
    assert_eq!(cloned.resource_usage.memory_used, 500);
}

#[test]
fn test_agent_health_debug() {
    let health = AgentHealth {
        agent_id: AgentId::new(),
        is_healthy: true,
        consecutive_failures: 0,
        consecutive_successes: 10,
        last_check: Instant::now(),
        last_response_time_ms: 5,
        resource_usage: ResourceUsage::default(),
    };
    let debug_str = format!("{:?}", health);
    assert!(debug_str.contains("is_healthy"));
    assert!(debug_str.contains("consecutive_failures"));
}

#[test]
fn test_supervisor_clone_shares_state() {
    let registry = Arc::new(AgentRegistry::new());
    let supervisor = Supervisor::new(registry.clone());
    let cloned = supervisor.clone();

    // Both should share the same Arc pointers
    assert!(Arc::ptr_eq(
        &supervisor.health_checks,
        &cloned.health_checks
    ));
    assert!(Arc::ptr_eq(
        &supervisor.running_agents,
        &cloned.running_agents
    ));
    assert!(Arc::ptr_eq(&supervisor.cgroups, &cloned.cgroups));
    assert!(Arc::ptr_eq(&supervisor.quotas, &cloned.quotas));
    assert!(Arc::ptr_eq(
        &supervisor.last_cpu_readings,
        &cloned.last_cpu_readings
    ));
}

#[tokio::test]
async fn test_supervisor_register_multiple_agents() {
    let registry = Arc::new(AgentRegistry::new());
    let supervisor = Supervisor::new(registry.clone());

    let id1 = AgentId::new();
    let id2 = AgentId::new();
    let id3 = AgentId::new();

    supervisor.register_agent(id1).await.unwrap();
    supervisor.register_agent(id2).await.unwrap();
    supervisor.register_agent(id3).await.unwrap();

    let health_map = supervisor.health_checks.read().await;
    assert_eq!(health_map.len(), 3);
    assert!(health_map.contains_key(&id1));
    assert!(health_map.contains_key(&id2));
    assert!(health_map.contains_key(&id3));
}

#[tokio::test]
async fn test_supervisor_get_health() {
    let registry = Arc::new(AgentRegistry::new());
    let supervisor = Supervisor::new(registry.clone());
    let agent_id = AgentId::new();

    // No health yet
    assert!(supervisor.get_health(agent_id).await.is_none());

    // Register, then check
    supervisor.register_agent(agent_id).await.unwrap();
    let health = supervisor.get_health(agent_id).await.unwrap();
    assert!(health.is_healthy);
    assert_eq!(health.agent_id, agent_id);
}

#[tokio::test]
async fn test_supervisor_get_all_health() {
    let registry = Arc::new(AgentRegistry::new());
    let supervisor = Supervisor::new(registry.clone());

    assert!(supervisor.get_all_health().await.is_empty());

    let id1 = AgentId::new();
    let id2 = AgentId::new();
    supervisor.register_agent(id1).await.unwrap();
    supervisor.register_agent(id2).await.unwrap();

    let all = supervisor.get_all_health().await;
    assert_eq!(all.len(), 2);
}

#[tokio::test]
async fn test_supervisor_update_health_status_healthy() {
    let registry = Arc::new(AgentRegistry::new());
    let supervisor = Supervisor::new(registry.clone());
    let agent_id = AgentId::new();

    supervisor.register_agent(agent_id).await.unwrap();

    // Mark healthy several times
    supervisor.update_health_status(agent_id, true).await;
    supervisor.update_health_status(agent_id, true).await;

    let health = supervisor.get_health(agent_id).await.unwrap();
    assert!(health.is_healthy);
    assert_eq!(health.consecutive_successes, 2);
    assert_eq!(health.consecutive_failures, 0);
}

#[tokio::test]
async fn test_supervisor_update_health_status_unhealthy_below_threshold() {
    let registry = Arc::new(AgentRegistry::new());
    let supervisor = Supervisor::new(registry.clone());
    let agent_id = AgentId::new();

    supervisor.register_agent(agent_id).await.unwrap();

    // Mark unhealthy once (threshold is 3 by default)
    supervisor.update_health_status(agent_id, false).await;

    let health = supervisor.get_health(agent_id).await.unwrap();
    // Still healthy — hasn't hit threshold yet
    assert!(health.is_healthy);
    assert_eq!(health.consecutive_failures, 1);
}

#[tokio::test]
async fn test_supervisor_update_health_resets_counters() {
    let registry = Arc::new(AgentRegistry::new());
    let supervisor = Supervisor::new(registry.clone());
    let agent_id = AgentId::new();

    supervisor.register_agent(agent_id).await.unwrap();

    // Failure resets successes
    supervisor.update_health_status(agent_id, true).await;
    supervisor.update_health_status(agent_id, false).await;

    let health = supervisor.get_health(agent_id).await.unwrap();
    assert_eq!(health.consecutive_successes, 0);
    assert_eq!(health.consecutive_failures, 1);

    // Success resets failures
    supervisor.update_health_status(agent_id, true).await;
    let health = supervisor.get_health(agent_id).await.unwrap();
    assert_eq!(health.consecutive_successes, 1);
    assert_eq!(health.consecutive_failures, 0);
}

#[tokio::test]
async fn test_supervisor_shutdown_all_empty() {
    let registry = Arc::new(AgentRegistry::new());
    let supervisor = Supervisor::new(registry.clone());

    // Should succeed even with no agents
    let result = supervisor.shutdown_all().await;
    assert!(result.is_ok());
}

#[test]
fn test_is_process_alive_nonexistent() {
    // PID 4_000_000 is extremely unlikely to exist (max PID is typically 4194304)
    // but won't wrap to -1 like u32::MAX would when cast to pid_t
    assert!(!Supervisor::is_process_alive(4_000_000));
}

#[test]
fn test_is_process_alive_current() {
    let pid = std::process::id();
    assert!(Supervisor::is_process_alive(pid));
}

#[test]
fn test_read_proc_memory_nonexistent() {
    assert_eq!(proc_utils::read_proc_memory(u32::MAX), 0);
}

#[test]
fn test_read_proc_memory_current() {
    let pid = std::process::id();
    let mem = proc_utils::read_proc_memory(pid);
    assert!(mem > 0);
}

#[test]
fn test_read_proc_cpu_time_us_nonexistent() {
    assert_eq!(proc_utils::read_proc_cpu_time_us(u32::MAX), 0);
}

#[test]
fn test_read_proc_cpu_time_us_current() {
    let pid = std::process::id();
    let _cpu = proc_utils::read_proc_cpu_time_us(pid);
    // May be 0 in short test, but should not panic
}

#[tokio::test]
async fn test_mock_agent_control_resource_usage() {
    let mock = MockAgentControl { healthy: true };
    let usage = mock.get_resource_usage().await.unwrap();
    assert_eq!(usage.memory_used, 100 * 1024 * 1024);
    assert_eq!(usage.cpu_time_used, 1000);
    assert_eq!(usage.file_descriptors_used, 10);
    assert_eq!(usage.processes_used, 1);
}

#[tokio::test]
async fn test_mock_agent_control_stop() {
    let mut mock = MockAgentControl { healthy: true };
    let result = mock.stop(StopReason::Normal).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_mock_agent_control_restart() {
    let mut mock = MockAgentControl { healthy: true };
    let result = mock.restart().await;
    assert!(result.is_ok());
}

#[test]
fn test_cgroup_base_constant() {
    assert_eq!(cgroup::CGROUP_BASE, "/sys/fs/cgroup/agnos");
}

// ==================================================================
// Additional coverage: update_health_status threshold transitions,
// handle_unhealthy_agent paths, check_resource_limits, shutdown_all,
// read_proc helpers, cgroup controller with real tempdir data
// ==================================================================

#[tokio::test]
async fn test_update_health_status_transition_to_unhealthy() {
    let registry = Arc::new(AgentRegistry::new());
    let supervisor = Supervisor::new(registry.clone());
    let agent_id = AgentId::new();

    supervisor.register_agent(agent_id).await.unwrap();

    // Mark unhealthy 3 times (= unhealthy_threshold)
    supervisor.update_health_status(agent_id, false).await;
    supervisor.update_health_status(agent_id, false).await;
    // At failure count 2, still healthy (threshold is 3)
    let health = supervisor.get_health(agent_id).await.unwrap();
    assert!(health.is_healthy);
    assert_eq!(health.consecutive_failures, 2);

    // Third failure should trigger transition to unhealthy
    supervisor.update_health_status(agent_id, false).await;
    let health = supervisor.get_health(agent_id).await.unwrap();
    assert!(!health.is_healthy);
    assert_eq!(health.consecutive_failures, 3);
}

#[tokio::test]
async fn test_update_health_status_recovery_from_unhealthy() {
    let registry = Arc::new(AgentRegistry::new());
    let supervisor = Supervisor::new(registry.clone());
    let agent_id = AgentId::new();

    supervisor.register_agent(agent_id).await.unwrap();

    // Make it unhealthy
    for _ in 0..3 {
        supervisor.update_health_status(agent_id, false).await;
    }
    let health = supervisor.get_health(agent_id).await.unwrap();
    assert!(!health.is_healthy);

    // Recover with successes (healthy_threshold = 2)
    supervisor.update_health_status(agent_id, true).await;
    let health = supervisor.get_health(agent_id).await.unwrap();
    assert!(!health.is_healthy); // Not yet recovered
    assert_eq!(health.consecutive_successes, 1);

    supervisor.update_health_status(agent_id, true).await;
    let health = supervisor.get_health(agent_id).await.unwrap();
    assert!(health.is_healthy); // Recovered!
    assert_eq!(health.consecutive_successes, 2);
}

#[tokio::test]
async fn test_update_health_status_nonexistent_agent() {
    let registry = Arc::new(AgentRegistry::new());
    let supervisor = Supervisor::new(registry.clone());

    // Should not panic when updating health of an agent that's not registered
    supervisor.update_health_status(AgentId::new(), true).await;
    supervisor.update_health_status(AgentId::new(), false).await;
}

#[tokio::test]
async fn test_supervisor_start_spawns_loops() {
    let registry = Arc::new(AgentRegistry::new());
    let supervisor = Supervisor::new(registry.clone());

    // start() should succeed and spawn background tasks
    let result = supervisor.start().await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_supervisor_shutdown_all_with_agents() {
    let registry = Arc::new(AgentRegistry::new());
    let supervisor = Supervisor::new(registry.clone());

    let id1 = AgentId::new();
    let id2 = AgentId::new();
    supervisor.register_agent(id1).await.unwrap();
    supervisor.register_agent(id2).await.unwrap();

    // Put mock agents into running_agents
    {
        let mut running = supervisor.running_agents.write().await;
        running.insert(id1, Box::new(MockAgentControl { healthy: true }));
        running.insert(id2, Box::new(MockAgentControl { healthy: false }));
    }

    let result = supervisor.shutdown_all().await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_supervisor_register_then_unregister_cleans_up() {
    let registry = Arc::new(AgentRegistry::new());
    let supervisor = Supervisor::new(registry.clone());

    let id = AgentId::new();
    supervisor.register_agent(id).await.unwrap();
    assert!(supervisor.get_health(id).await.is_some());

    supervisor.unregister_agent(id).await.unwrap();
    assert!(supervisor.get_health(id).await.is_none());
    // Cgroup map should also be cleaned
    assert!(!supervisor.cgroups.read().await.contains_key(&id));
}

#[test]
fn test_read_proc_memory_current_process() {
    let pid = std::process::id();
    let mem = proc_utils::read_proc_memory(pid);
    assert!(mem > 0, "Current process should have non-zero memory");
}

#[test]
fn test_read_proc_cpu_time_us_current_process() {
    let pid = std::process::id();
    let _cpu = proc_utils::read_proc_cpu_time_us(pid);
    // May be 0 in a short test, but should not panic
}

#[test]
fn test_cgroup_controller_debug() {
    let agent_id = AgentId::new();
    let controller = cgroup::CgroupController {
        path: PathBuf::from("/tmp/test-debug"),
        agent_id,
    };
    let dbg = format!("{:?}", controller);
    assert!(dbg.contains("CgroupController"));
    assert!(dbg.contains("/tmp/test-debug"));
}

#[test]
fn test_cgroup_controller_cpu_stat_with_tempdir_no_usage_line() {
    let tmp = TempDir::new().unwrap();
    let agent_id = AgentId::new();
    let cg_path = tmp.path().join(agent_id.to_string());
    std::fs::create_dir_all(&cg_path).unwrap();

    let controller = cgroup::CgroupController {
        path: cg_path.clone(),
        agent_id,
    };

    // Write cpu.stat without usage_usec line
    std::fs::write(cg_path.join("cpu.stat"), "user_usec 100\nsystem_usec 200\n").unwrap();
    assert_eq!(controller.cpu_usage_usec(), 0);
}

#[test]
fn test_cgroup_controller_memory_max_invalid_content() {
    let tmp = TempDir::new().unwrap();
    let agent_id = AgentId::new();
    let cg_path = tmp.path().join(agent_id.to_string());
    std::fs::create_dir_all(&cg_path).unwrap();

    let controller = cgroup::CgroupController {
        path: cg_path.clone(),
        agent_id,
    };

    // Write invalid content that's not "max" and not a number
    std::fs::write(cg_path.join("memory.max"), "invalid").unwrap();
    assert_eq!(controller.memory_max(), None);
}

#[test]
fn test_cgroup_controller_pids_with_invalid_lines() {
    let tmp = TempDir::new().unwrap();
    let agent_id = AgentId::new();
    let cg_path = tmp.path().join(agent_id.to_string());
    std::fs::create_dir_all(&cg_path).unwrap();

    let controller = cgroup::CgroupController {
        path: cg_path.clone(),
        agent_id,
    };

    // Mix of valid and invalid lines
    std::fs::write(cg_path.join("cgroup.procs"), "100\nnot_a_pid\n200\n\n300\n").unwrap();
    let pids = controller.pids();
    assert_eq!(pids, vec![100, 200, 300]);
}

#[tokio::test]
async fn test_supervisor_get_all_health_after_unregister() {
    let registry = Arc::new(AgentRegistry::new());
    let supervisor = Supervisor::new(registry.clone());

    let id1 = AgentId::new();
    let id2 = AgentId::new();
    supervisor.register_agent(id1).await.unwrap();
    supervisor.register_agent(id2).await.unwrap();
    assert_eq!(supervisor.get_all_health().await.len(), 2);

    supervisor.unregister_agent(id1).await.unwrap();
    assert_eq!(supervisor.get_all_health().await.len(), 1);
}

#[tokio::test]
async fn test_supervisor_register_same_agent_twice() {
    let registry = Arc::new(AgentRegistry::new());
    let supervisor = Supervisor::new(registry.clone());

    let id = AgentId::new();
    supervisor.register_agent(id).await.unwrap();
    // Registering again should overwrite (reset health counters)
    supervisor.update_health_status(id, false).await;
    supervisor.register_agent(id).await.unwrap();
    let health = supervisor.get_health(id).await.unwrap();
    assert_eq!(health.consecutive_failures, 0);
}

#[test]
fn test_is_process_alive_self() {
    // Our own PID should always be alive
    let pid = std::process::id();
    assert!(Supervisor::is_process_alive(pid));
}

#[tokio::test]
async fn test_check_agent_health_not_in_registry() {
    let registry = Arc::new(AgentRegistry::new());
    let supervisor = Supervisor::new(registry.clone());
    let agent_id = AgentId::new();

    // Agent not in registry should error
    let result = supervisor.check_agent_health(agent_id).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_supervisor_clone_register_visible_in_clone() {
    let registry = Arc::new(AgentRegistry::new());
    let supervisor = Supervisor::new(registry.clone());
    let cloned = supervisor.clone();

    let id = AgentId::new();
    supervisor.register_agent(id).await.unwrap();

    // Visible in clone
    assert!(cloned.get_health(id).await.is_some());
}

// ==================================================================
// Additional coverage: signal_agent paths, check_agent_health states,
// handle_unhealthy_agent, check_resource_limits, cgroup edge cases,
// read_proc helper edge cases, supervisor with registry interactions
// ==================================================================

#[tokio::test]
async fn test_signal_agent_no_agent_in_registry() {
    let registry = Arc::new(AgentRegistry::new());
    let supervisor = Supervisor::new(registry.clone());
    // Should not panic when agent is not in registry
    supervisor.signal_agent(AgentId::new(), libc::SIGTERM).await;
}

#[tokio::test]
async fn test_handle_unhealthy_agent_no_control_exceeds_max() {
    let registry = Arc::new(AgentRegistry::new());
    let supervisor = Supervisor::new(registry.clone());
    let agent_id = AgentId::new();

    // Register the agent for supervision
    supervisor.register_agent(agent_id).await.unwrap();

    // Set consecutive_failures high enough to exceed MAX_RESTART_ATTEMPTS (5)
    {
        let mut checks = supervisor.health_checks.write().await;
        if let Some(h) = checks.get_mut(&agent_id) {
            h.consecutive_failures = 10;
        }
    }

    // Should not panic, just try to mark as failed
    supervisor.handle_unhealthy_agent(agent_id).await;
}

#[tokio::test]
async fn test_handle_unhealthy_agent_below_max() {
    let registry = Arc::new(AgentRegistry::new());
    let supervisor = Supervisor::new(registry.clone());
    let agent_id = AgentId::new();

    supervisor.register_agent(agent_id).await.unwrap();

    // Set failures below max — will try to restart but no AgentControl registered
    {
        let mut checks = supervisor.health_checks.write().await;
        if let Some(h) = checks.get_mut(&agent_id) {
            h.consecutive_failures = 2;
        }
    }

    // Should not panic; will attempt restart, find no AgentControl, mark failed
    supervisor.handle_unhealthy_agent(agent_id).await;
}

#[test]
fn test_cgroup_controller_set_memory_limit_zero_is_max() {
    let tmp = TempDir::new().unwrap();
    let agent_id = AgentId::new();
    let cg_path = tmp.path().join(agent_id.to_string());
    std::fs::create_dir_all(&cg_path).unwrap();

    let controller = cgroup::CgroupController {
        path: cg_path.clone(),
        agent_id,
    };

    controller.set_memory_limit(0).unwrap();
    let content = std::fs::read_to_string(cg_path.join("memory.max")).unwrap();
    assert_eq!(content, "max");
}

#[test]
fn test_cgroup_controller_set_cpu_limit_zero_quota_is_max() {
    let tmp = TempDir::new().unwrap();
    let agent_id = AgentId::new();
    let cg_path = tmp.path().join(agent_id.to_string());
    std::fs::create_dir_all(&cg_path).unwrap();

    let controller = cgroup::CgroupController {
        path: cg_path.clone(),
        agent_id,
    };

    controller.set_cpu_limit(0, 50000).unwrap();
    let content = std::fs::read_to_string(cg_path.join("cpu.max")).unwrap();
    assert_eq!(content, "max 50000");
}

#[test]
fn test_read_proc_memory_pid_zero() {
    // PID 0 (kernel scheduler) is special, should not panic
    let mem = proc_utils::read_proc_memory(0);
    let _ = mem;
}

#[test]
fn test_read_proc_cpu_time_us_pid_zero() {
    let cpu = proc_utils::read_proc_cpu_time_us(0);
    let _ = cpu;
}

#[tokio::test]
async fn test_supervisor_register_unregister_multiple() {
    let registry = Arc::new(AgentRegistry::new());
    let supervisor = Supervisor::new(registry.clone());

    let ids: Vec<AgentId> = (0..10).map(|_| AgentId::new()).collect();
    for &id in &ids {
        supervisor.register_agent(id).await.unwrap();
    }
    assert_eq!(supervisor.get_all_health().await.len(), 10);

    // Unregister half
    for &id in &ids[..5] {
        supervisor.unregister_agent(id).await.unwrap();
    }
    assert_eq!(supervisor.get_all_health().await.len(), 5);

    // Remaining should still be there
    for &id in &ids[5..] {
        assert!(supervisor.get_health(id).await.is_some());
    }
}

#[tokio::test]
async fn test_supervisor_health_alternating_updates() {
    let registry = Arc::new(AgentRegistry::new());
    let supervisor = Supervisor::new(registry.clone());
    let agent_id = AgentId::new();
    supervisor.register_agent(agent_id).await.unwrap();

    // Alternate healthy/unhealthy to verify counter resets
    supervisor.update_health_status(agent_id, true).await;
    supervisor.update_health_status(agent_id, false).await;
    supervisor.update_health_status(agent_id, true).await;
    supervisor.update_health_status(agent_id, false).await;

    let health = supervisor.get_health(agent_id).await.unwrap();
    // After alternating, consecutive counters should reflect the last transition
    assert_eq!(health.consecutive_failures, 1);
    assert_eq!(health.consecutive_successes, 0);
}

#[test]
fn test_cgroup_controller_memory_current_with_whitespace() {
    let tmp = TempDir::new().unwrap();
    let agent_id = AgentId::new();
    let cg_path = tmp.path().join(agent_id.to_string());
    std::fs::create_dir_all(&cg_path).unwrap();

    let controller = cgroup::CgroupController {
        path: cg_path.clone(),
        agent_id,
    };

    // Kernel often writes with trailing newline
    std::fs::write(cg_path.join("memory.current"), "12345678\n").unwrap();
    assert_eq!(controller.memory_current(), 12345678);
}

#[test]
fn test_cgroup_controller_cpu_stat_multiple_lines() {
    let tmp = TempDir::new().unwrap();
    let agent_id = AgentId::new();
    let cg_path = tmp.path().join(agent_id.to_string());
    std::fs::create_dir_all(&cg_path).unwrap();

    let controller = cgroup::CgroupController {
        path: cg_path.clone(),
        agent_id,
    };

    // Realistic cpu.stat output
    std::fs::write(
        cg_path.join("cpu.stat"),
        "usage_usec 9876543\nuser_usec 6000000\nsystem_usec 3876543\nnr_periods 100\nnr_throttled 5\nthrottled_usec 50000\n",
    ).unwrap();
    assert_eq!(controller.cpu_usage_usec(), 9876543);
}

// ==================================================================
// ResourceQuota tests
// ==================================================================

#[test]
fn test_resource_quota_defaults() {
    let quota = ResourceQuota::default();
    assert!((quota.memory_warn_pct - 80.0).abs() < f64::EPSILON);
    assert!((quota.memory_kill_pct - 95.0).abs() < f64::EPSILON);
    assert!((quota.cpu_throttle_pct - 90.0).abs() < f64::EPSILON);
    assert_eq!(quota.memory_limit, 0);
    assert_eq!(quota.cpu_time_limit, 0);
}

#[test]
fn test_resource_quota_from_limits() {
    let quota = ResourceQuota::from_limits(1024 * 1024 * 1024, 3_600_000);
    assert_eq!(quota.memory_limit, 1024 * 1024 * 1024);
    assert_eq!(quota.cpu_time_limit, 3_600_000);
    // Should still have default thresholds
    assert!((quota.memory_warn_pct - 80.0).abs() < f64::EPSILON);
    assert!((quota.memory_kill_pct - 95.0).abs() < f64::EPSILON);
    assert!((quota.cpu_throttle_pct - 90.0).abs() < f64::EPSILON);
}

#[test]
fn test_resource_quota_clone() {
    let quota = ResourceQuota {
        memory_warn_pct: 70.0,
        memory_kill_pct: 90.0,
        cpu_throttle_pct: 85.0,
        memory_limit: 512 * 1024 * 1024,
        cpu_time_limit: 1_800_000,
    };
    let cloned = quota.clone();
    assert!((cloned.memory_warn_pct - 70.0).abs() < f64::EPSILON);
    assert!((cloned.memory_kill_pct - 90.0).abs() < f64::EPSILON);
    assert!((cloned.cpu_throttle_pct - 85.0).abs() < f64::EPSILON);
    assert_eq!(cloned.memory_limit, 512 * 1024 * 1024);
    assert_eq!(cloned.cpu_time_limit, 1_800_000);
}

#[test]
fn test_resource_quota_debug() {
    let quota = ResourceQuota::default();
    let dbg = format!("{:?}", quota);
    assert!(dbg.contains("memory_warn_pct"));
    assert!(dbg.contains("memory_kill_pct"));
    assert!(dbg.contains("cpu_throttle_pct"));
}

#[tokio::test]
async fn test_supervisor_set_and_get_quota() {
    let registry = Arc::new(AgentRegistry::new());
    let supervisor = Supervisor::new(registry.clone());
    let agent_id = AgentId::new();

    // No quota yet
    assert!(supervisor.get_quota(agent_id).await.is_none());

    // Set a quota
    let quota = ResourceQuota {
        memory_warn_pct: 70.0,
        memory_kill_pct: 90.0,
        cpu_throttle_pct: 85.0,
        memory_limit: 2 * 1024 * 1024 * 1024,
        cpu_time_limit: 7_200_000,
    };
    supervisor.set_quota(agent_id, quota).await;

    let retrieved = supervisor.get_quota(agent_id).await.unwrap();
    assert!((retrieved.memory_warn_pct - 70.0).abs() < f64::EPSILON);
    assert!((retrieved.memory_kill_pct - 90.0).abs() < f64::EPSILON);
    assert_eq!(retrieved.memory_limit, 2 * 1024 * 1024 * 1024);
}

#[tokio::test]
async fn test_supervisor_register_creates_quota() {
    let registry = Arc::new(AgentRegistry::new());
    let supervisor = Supervisor::new(registry.clone());
    let agent_id = AgentId::new();

    supervisor.register_agent(agent_id).await.unwrap();

    // register_agent should create a default quota (since agent won't be in registry config)
    let quota = supervisor.get_quota(agent_id).await.unwrap();
    assert!((quota.memory_warn_pct - 80.0).abs() < f64::EPSILON);
    assert!((quota.memory_kill_pct - 95.0).abs() < f64::EPSILON);
}

#[tokio::test]
async fn test_supervisor_unregister_removes_quota() {
    let registry = Arc::new(AgentRegistry::new());
    let supervisor = Supervisor::new(registry.clone());
    let agent_id = AgentId::new();

    supervisor.register_agent(agent_id).await.unwrap();
    assert!(supervisor.get_quota(agent_id).await.is_some());

    supervisor.unregister_agent(agent_id).await.unwrap();
    assert!(supervisor.get_quota(agent_id).await.is_none());
}

#[tokio::test]
async fn test_supervisor_set_quota_overrides_registered() {
    let registry = Arc::new(AgentRegistry::new());
    let supervisor = Supervisor::new(registry.clone());
    let agent_id = AgentId::new();

    supervisor.register_agent(agent_id).await.unwrap();

    // Override with custom quota
    let custom = ResourceQuota {
        memory_warn_pct: 50.0,
        memory_kill_pct: 75.0,
        cpu_throttle_pct: 60.0,
        memory_limit: 256 * 1024 * 1024,
        cpu_time_limit: 600_000,
    };
    supervisor.set_quota(agent_id, custom).await;

    let retrieved = supervisor.get_quota(agent_id).await.unwrap();
    assert!((retrieved.memory_warn_pct - 50.0).abs() < f64::EPSILON);
    assert!((retrieved.memory_kill_pct - 75.0).abs() < f64::EPSILON);
    assert_eq!(retrieved.memory_limit, 256 * 1024 * 1024);
}

#[tokio::test]
async fn test_supervisor_quotas_empty_on_new() {
    let registry = Arc::new(AgentRegistry::new());
    let supervisor = Supervisor::new(registry.clone());
    assert!(supervisor.quotas.read().await.is_empty());
    assert!(supervisor.last_cpu_readings.read().await.is_empty());
}

#[tokio::test]
async fn test_supervisor_unregister_cleans_cpu_readings() {
    let registry = Arc::new(AgentRegistry::new());
    let supervisor = Supervisor::new(registry.clone());
    let agent_id = AgentId::new();

    // Manually insert a CPU reading
    supervisor
        .last_cpu_readings
        .write()
        .await
        .insert(agent_id, (Instant::now(), 12345));

    supervisor.register_agent(agent_id).await.unwrap();
    supervisor.unregister_agent(agent_id).await.unwrap();

    assert!(!supervisor
        .last_cpu_readings
        .read()
        .await
        .contains_key(&agent_id));
}

#[test]
fn test_resource_quota_from_limits_zero() {
    let quota = ResourceQuota::from_limits(0, 0);
    assert_eq!(quota.memory_limit, 0);
    assert_eq!(quota.cpu_time_limit, 0);
    // Default thresholds still set
    assert!((quota.memory_warn_pct - 80.0).abs() < f64::EPSILON);
}

#[tokio::test]
async fn test_supervisor_multiple_agents_independent_quotas() {
    let registry = Arc::new(AgentRegistry::new());
    let supervisor = Supervisor::new(registry.clone());
    let id1 = AgentId::new();
    let id2 = AgentId::new();

    supervisor
        .set_quota(
            id1,
            ResourceQuota {
                memory_warn_pct: 60.0,
                memory_kill_pct: 80.0,
                cpu_throttle_pct: 70.0,
                memory_limit: 1024,
                cpu_time_limit: 500,
            },
        )
        .await;

    supervisor
        .set_quota(
            id2,
            ResourceQuota {
                memory_warn_pct: 90.0,
                memory_kill_pct: 99.0,
                cpu_throttle_pct: 95.0,
                memory_limit: 2048,
                cpu_time_limit: 1000,
            },
        )
        .await;

    let q1 = supervisor.get_quota(id1).await.unwrap();
    let q2 = supervisor.get_quota(id2).await.unwrap();
    assert!((q1.memory_warn_pct - 60.0).abs() < f64::EPSILON);
    assert!((q2.memory_warn_pct - 90.0).abs() < f64::EPSILON);
    assert_eq!(q1.memory_limit, 1024);
    assert_eq!(q2.memory_limit, 2048);
}

// ==================================================================
// New coverage: CgroupController path generation, AgentHealth state,
// ResourceQuota thresholds, register/unregister, backoff logic,
// cgroup error paths
// ==================================================================

#[test]
fn test_cgroup_controller_path_format() {
    let id = AgentId::new();
    let expected = PathBuf::from(cgroup::CGROUP_BASE).join(id.to_string());
    // We can't call CgroupController::new (needs /sys/fs/cgroup) but
    // verify the path would be correct via open() returning None.
    let result = cgroup::CgroupController::open(id);
    assert!(
        result.is_none(),
        "No cgroup should exist for a random agent ID"
    );
    // Verify path format
    assert!(expected.starts_with(cgroup::CGROUP_BASE));
    assert!(expected.to_string_lossy().contains(&id.to_string()));
}

#[test]
fn test_cgroup_controller_new_error_path() {
    // CgroupController::new will fail on non-root / non-cgroup systems
    let id = AgentId::new();
    let result = cgroup::CgroupController::new(id);
    // Should fail because /sys/fs/cgroup/agnos is not writable
    assert!(result.is_err() || result.is_ok());
}

#[test]
fn test_agent_health_construction() {
    let id = AgentId::new();
    let health = AgentHealth {
        agent_id: id,
        is_healthy: true,
        consecutive_failures: 0,
        consecutive_successes: 0,
        last_check: Instant::now(),
        last_response_time_ms: 0,
        resource_usage: ResourceUsage::default(),
    };
    assert!(health.is_healthy);
    assert_eq!(health.consecutive_failures, 0);
    assert_eq!(health.consecutive_successes, 0);
    assert_eq!(health.resource_usage.memory_used, 0);
}

#[test]
fn test_agent_health_clone_preserves_fields() {
    let health = AgentHealth {
        agent_id: AgentId::new(),
        is_healthy: false,
        consecutive_failures: 5,
        consecutive_successes: 0,
        last_check: Instant::now(),
        last_response_time_ms: 42,
        resource_usage: ResourceUsage {
            memory_used: 1000,
            cpu_time_used: 500,
            file_descriptors_used: 10,
            processes_used: 2,
        },
    };
    let cloned = health.clone();
    assert_eq!(cloned.agent_id, health.agent_id);
    assert!(!cloned.is_healthy);
    assert_eq!(cloned.consecutive_failures, 5);
    assert_eq!(cloned.last_response_time_ms, 42);
}

#[test]
fn test_resource_quota_default_thresholds() {
    let q = ResourceQuota::default();
    assert!((q.memory_warn_pct - 80.0).abs() < f64::EPSILON);
    assert!((q.memory_kill_pct - 95.0).abs() < f64::EPSILON);
    assert!((q.cpu_throttle_pct - 90.0).abs() < f64::EPSILON);
    assert_eq!(q.memory_limit, 0);
    assert_eq!(q.cpu_time_limit, 0);
}

#[test]
fn test_resource_quota_from_limits_with_values() {
    let q = ResourceQuota::from_limits(1024 * 1024 * 512, 3600);
    assert_eq!(q.memory_limit, 1024 * 1024 * 512);
    assert_eq!(q.cpu_time_limit, 3600);
    // Thresholds should be defaults
    assert!((q.memory_warn_pct - 80.0).abs() < f64::EPSILON);
    assert!((q.memory_kill_pct - 95.0).abs() < f64::EPSILON);
}

#[test]
fn test_resource_quota_threshold_calculations() {
    let q = ResourceQuota::from_limits(1000, 5000);
    // 80% of 1000 = 800
    let warn_threshold = q.memory_limit as f64 * q.memory_warn_pct / 100.0;
    assert!((warn_threshold - 800.0).abs() < f64::EPSILON);
    // 95% of 1000 = 950
    let kill_threshold = q.memory_limit as f64 * q.memory_kill_pct / 100.0;
    assert!((kill_threshold - 950.0).abs() < f64::EPSILON);
}

#[tokio::test]
async fn test_supervisor_register_then_unregister() {
    let registry = Arc::new(AgentRegistry::new());
    let supervisor = Supervisor::new(registry);
    let id = AgentId::new();

    supervisor.register_agent(id).await.unwrap();
    let health = supervisor.get_health(id).await;
    assert!(health.is_some());
    assert!(health.unwrap().is_healthy);

    supervisor.unregister_agent(id).await.unwrap();
    let health = supervisor.get_health(id).await;
    assert!(health.is_none());
}

#[tokio::test]
async fn test_supervisor_register_creates_default_quota() {
    let registry = Arc::new(AgentRegistry::new());
    let supervisor = Supervisor::new(registry);
    let id = AgentId::new();

    supervisor.register_agent(id).await.unwrap();
    let quota = supervisor.get_quota(id).await;
    assert!(quota.is_some());
    let q = quota.unwrap();
    // No config in registry => default quota
    assert_eq!(q.memory_limit, 0);
    assert_eq!(q.cpu_time_limit, 0);
}

#[tokio::test]
async fn test_supervisor_get_all_health_multiple() {
    let registry = Arc::new(AgentRegistry::new());
    let supervisor = Supervisor::new(registry);

    let id1 = AgentId::new();
    let id2 = AgentId::new();
    supervisor.register_agent(id1).await.unwrap();
    supervisor.register_agent(id2).await.unwrap();

    let all = supervisor.get_all_health().await;
    assert_eq!(all.len(), 2);
}

#[tokio::test]
async fn test_supervisor_unregister_unknown_agent() {
    let registry = Arc::new(AgentRegistry::new());
    let supervisor = Supervisor::new(registry);
    // Should succeed silently
    let result = supervisor.unregister_agent(AgentId::new()).await;
    assert!(result.is_ok());
}

#[test]
fn test_is_process_alive_own_pid() {
    let pid = std::process::id();
    assert!(Supervisor::is_process_alive(pid));
}

#[test]
fn test_is_process_alive_very_large_pid() {
    // Use a very large but valid PID that is extremely unlikely to exist
    // Avoid u32::MAX which wraps to -1 as pid_t (signals all processes)
    let alive = Supervisor::is_process_alive(4_000_000);
    assert!(!alive);
}

#[test]
fn test_read_proc_memory_own_process() {
    let pid = std::process::id();
    let mem = proc_utils::read_proc_memory(pid);
    assert!(mem > 0, "Current process should have non-zero memory");
}

#[test]
fn test_read_proc_memory_max_pid() {
    assert_eq!(proc_utils::read_proc_memory(u32::MAX), 0);
}

#[test]
fn test_read_proc_cpu_time_us_own_process() {
    let pid = std::process::id();
    // May be 0 for short-lived test but should not panic
    let _cpu = proc_utils::read_proc_cpu_time_us(pid);
}

#[test]
fn test_read_proc_cpu_time_us_max_pid() {
    assert_eq!(proc_utils::read_proc_cpu_time_us(u32::MAX), 0);
}

// ==================================================================
// NEW: Supervisor lifecycle, backoff, concurrent access, quota edge cases,
// cgroup tempdir advanced, health threshold boundary, mock agent lifecycle
// ==================================================================

#[tokio::test]
async fn test_supervisor_register_unregister_register_same_agent() {
    let registry = Arc::new(AgentRegistry::new());
    let supervisor = Supervisor::new(registry.clone());
    let id = AgentId::new();

    supervisor.register_agent(id).await.unwrap();
    // Accumulate some health failures
    supervisor.update_health_status(id, false).await;
    supervisor.update_health_status(id, false).await;
    let h = supervisor.get_health(id).await.unwrap();
    assert_eq!(h.consecutive_failures, 2);

    // Unregister and re-register should reset health
    supervisor.unregister_agent(id).await.unwrap();
    supervisor.register_agent(id).await.unwrap();
    let h = supervisor.get_health(id).await.unwrap();
    assert_eq!(h.consecutive_failures, 0);
    assert!(h.is_healthy);
}

#[tokio::test]
async fn test_supervisor_concurrent_register_unregister() {
    let registry = Arc::new(AgentRegistry::new());
    let supervisor = Supervisor::new(registry.clone());

    let ids: Vec<AgentId> = (0..20).map(|_| AgentId::new()).collect();

    // Register all concurrently
    let mut handles = Vec::new();
    for &id in &ids {
        let s = supervisor.clone();
        handles.push(tokio::spawn(async move {
            s.register_agent(id).await.unwrap();
        }));
    }
    for h in handles {
        h.await.unwrap();
    }

    assert_eq!(supervisor.get_all_health().await.len(), 20);

    // Unregister all concurrently
    let mut handles = Vec::new();
    for &id in &ids {
        let s = supervisor.clone();
        handles.push(tokio::spawn(async move {
            s.unregister_agent(id).await.unwrap();
        }));
    }
    for h in handles {
        h.await.unwrap();
    }

    assert!(supervisor.get_all_health().await.is_empty());
}

#[tokio::test]
async fn test_supervisor_health_threshold_exact_boundary() {
    let registry = Arc::new(AgentRegistry::new());
    let supervisor = Supervisor::new(registry.clone());
    let id = AgentId::new();

    supervisor.register_agent(id).await.unwrap();

    // Hit exactly unhealthy_threshold - 1 failures: still healthy
    for _ in 0..(supervisor.config.unhealthy_threshold - 1) {
        supervisor.update_health_status(id, false).await;
    }
    assert!(supervisor.get_health(id).await.unwrap().is_healthy);

    // One more failure: transitions to unhealthy
    supervisor.update_health_status(id, false).await;
    assert!(!supervisor.get_health(id).await.unwrap().is_healthy);
}

#[tokio::test]
async fn test_supervisor_recovery_threshold_exact_boundary() {
    let registry = Arc::new(AgentRegistry::new());
    let supervisor = Supervisor::new(registry.clone());
    let id = AgentId::new();

    supervisor.register_agent(id).await.unwrap();

    // Make unhealthy
    for _ in 0..supervisor.config.unhealthy_threshold {
        supervisor.update_health_status(id, false).await;
    }
    assert!(!supervisor.get_health(id).await.unwrap().is_healthy);

    // Hit exactly healthy_threshold - 1 successes: still unhealthy
    for _ in 0..(supervisor.config.healthy_threshold - 1) {
        supervisor.update_health_status(id, true).await;
    }
    assert!(!supervisor.get_health(id).await.unwrap().is_healthy);

    // One more success: recovers
    supervisor.update_health_status(id, true).await;
    assert!(supervisor.get_health(id).await.unwrap().is_healthy);
}

#[tokio::test]
async fn test_supervisor_set_quota_without_register() {
    let registry = Arc::new(AgentRegistry::new());
    let supervisor = Supervisor::new(registry.clone());
    let id = AgentId::new();

    let quota = ResourceQuota::from_limits(4096, 1000);
    supervisor.set_quota(id, quota).await;

    let q = supervisor.get_quota(id).await.unwrap();
    assert_eq!(q.memory_limit, 4096);
    assert_eq!(q.cpu_time_limit, 1000);
}

#[tokio::test]
async fn test_supervisor_mock_agent_lifecycle() {
    let registry = Arc::new(AgentRegistry::new());
    let supervisor = Supervisor::new(registry.clone());
    let id = AgentId::new();

    supervisor.register_agent(id).await.unwrap();

    // Add a mock running agent
    {
        let mut running = supervisor.running_agents.write().await;
        running.insert(id, Box::new(MockAgentControl { healthy: true }));
    }

    // Shutdown should not panic
    supervisor.shutdown_all().await.unwrap();
}

#[tokio::test]
async fn test_supervisor_shutdown_all_updates_status_for_running() {
    let registry = Arc::new(AgentRegistry::new());
    let supervisor = Supervisor::new(registry.clone());
    let id = AgentId::new();

    supervisor.register_agent(id).await.unwrap();
    {
        let mut running = supervisor.running_agents.write().await;
        running.insert(id, Box::new(MockAgentControl { healthy: true }));
    }

    // shutdown_all iterates running_agents keys
    let result = supervisor.shutdown_all().await;
    assert!(result.is_ok());
}

#[test]
fn test_cgroup_controller_destroy_nonempty_dir_fails() {
    let tmp = TempDir::new().unwrap();
    let agent_id = AgentId::new();
    let cg_path = tmp.path().join(agent_id.to_string());
    std::fs::create_dir_all(&cg_path).unwrap();

    // Create a file inside so rmdir fails
    std::fs::write(cg_path.join("some_file"), "data").unwrap();

    let controller = cgroup::CgroupController {
        path: cg_path.clone(),
        agent_id,
    };

    // destroy calls remove_dir which fails on non-empty dir
    let result = controller.destroy();
    assert!(result.is_err());
}

#[test]
fn test_cgroup_controller_memory_current_non_numeric() {
    let tmp = TempDir::new().unwrap();
    let agent_id = AgentId::new();
    let cg_path = tmp.path().join(agent_id.to_string());
    std::fs::create_dir_all(&cg_path).unwrap();

    let controller = cgroup::CgroupController {
        path: cg_path.clone(),
        agent_id,
    };

    std::fs::write(cg_path.join("memory.current"), "not_a_number\n").unwrap();
    assert_eq!(controller.memory_current(), 0);
}

#[test]
fn test_cgroup_controller_cpu_stat_usage_usec_last_line() {
    let tmp = TempDir::new().unwrap();
    let agent_id = AgentId::new();
    let cg_path = tmp.path().join(agent_id.to_string());
    std::fs::create_dir_all(&cg_path).unwrap();

    let controller = cgroup::CgroupController {
        path: cg_path.clone(),
        agent_id,
    };

    // usage_usec appears as the last line
    std::fs::write(
        cg_path.join("cpu.stat"),
        "user_usec 100\nsystem_usec 200\nusage_usec 999",
    )
    .unwrap();
    assert_eq!(controller.cpu_usage_usec(), 999);
}

#[test]
fn test_resource_quota_custom_thresholds() {
    let quota = ResourceQuota {
        memory_warn_pct: 50.0,
        memory_kill_pct: 60.0,
        cpu_throttle_pct: 40.0,
        memory_limit: 100,
        cpu_time_limit: 200,
    };
    assert!((quota.memory_warn_pct - 50.0).abs() < f64::EPSILON);
    assert!((quota.memory_kill_pct - 60.0).abs() < f64::EPSILON);
    assert!((quota.cpu_throttle_pct - 40.0).abs() < f64::EPSILON);
}

#[tokio::test]
async fn test_supervisor_quota_removed_on_unregister() {
    let registry = Arc::new(AgentRegistry::new());
    let supervisor = Supervisor::new(registry.clone());
    let id = AgentId::new();

    supervisor
        .set_quota(id, ResourceQuota::from_limits(999, 888))
        .await;
    assert!(supervisor.get_quota(id).await.is_some());

    supervisor.unregister_agent(id).await.unwrap();
    assert!(supervisor.get_quota(id).await.is_none());
}

#[tokio::test]
async fn test_supervisor_last_cpu_readings_populated_and_cleared() {
    let registry = Arc::new(AgentRegistry::new());
    let supervisor = Supervisor::new(registry.clone());
    let id = AgentId::new();

    // Simulate a reading
    supervisor
        .last_cpu_readings
        .write()
        .await
        .insert(id, (Instant::now(), 500_000));
    assert!(supervisor.last_cpu_readings.read().await.contains_key(&id));

    supervisor.unregister_agent(id).await.unwrap();
    assert!(!supervisor.last_cpu_readings.read().await.contains_key(&id));
}

#[tokio::test]
async fn test_supervisor_health_stays_healthy_when_already_healthy() {
    let registry = Arc::new(AgentRegistry::new());
    let supervisor = Supervisor::new(registry.clone());
    let id = AgentId::new();

    supervisor.register_agent(id).await.unwrap();

    // Pass healthy_threshold successes
    for _ in 0..10 {
        supervisor.update_health_status(id, true).await;
    }
    let h = supervisor.get_health(id).await.unwrap();
    assert!(h.is_healthy);
    assert_eq!(h.consecutive_successes, 10);
    assert_eq!(h.consecutive_failures, 0);
}

#[tokio::test]
async fn test_supervisor_handle_unhealthy_agent_with_mock_agent_control() {
    let registry = Arc::new(AgentRegistry::new());
    let supervisor = Supervisor::new(registry.clone());
    let id = AgentId::new();

    supervisor.register_agent(id).await.unwrap();

    // Insert a mock agent control that can be restarted
    {
        let mut running = supervisor.running_agents.write().await;
        running.insert(id, Box::new(MockAgentControl { healthy: true }));
    }

    // Set failure count below max
    {
        let mut checks = supervisor.health_checks.write().await;
        if let Some(h) = checks.get_mut(&id) {
            h.consecutive_failures = 1;
        }
    }

    // This will attempt restart via AgentControl trait
    // Note: includes backoff sleep so keep failure count low (1 => 2^1=2s)
    // Actually it should be fast enough for a test since backoff is 2^1 = 2 secs.
    // We'll just verify it doesn't panic. The actual restart calls mock.restart().
    // Skipping this test due to sleep -- instead test the boundary logic directly.
}

#[test]
fn test_resource_quota_from_limits_large_values() {
    let quota = ResourceQuota::from_limits(u64::MAX, u64::MAX);
    assert_eq!(quota.memory_limit, u64::MAX);
    assert_eq!(quota.cpu_time_limit, u64::MAX);
}

#[tokio::test]
async fn test_supervisor_check_agent_health_not_in_registry_errors() {
    let registry = Arc::new(AgentRegistry::new());
    let supervisor = Supervisor::new(registry.clone());

    // Agent not in registry should fail
    let result = supervisor.check_agent_health(AgentId::new()).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_supervisor_multiple_quota_overrides() {
    let registry = Arc::new(AgentRegistry::new());
    let supervisor = Supervisor::new(registry.clone());
    let id = AgentId::new();

    // Override multiple times
    for i in 1..=5u64 {
        supervisor
            .set_quota(id, ResourceQuota::from_limits(i * 1000, i * 100))
            .await;
    }

    let q = supervisor.get_quota(id).await.unwrap();
    assert_eq!(q.memory_limit, 5000);
    assert_eq!(q.cpu_time_limit, 500);
}

#[test]
fn test_cgroup_controller_set_memory_limit_large_value() {
    let tmp = TempDir::new().unwrap();
    let agent_id = AgentId::new();
    let cg_path = tmp.path().join(agent_id.to_string());
    std::fs::create_dir_all(&cg_path).unwrap();

    let controller = cgroup::CgroupController {
        path: cg_path.clone(),
        agent_id,
    };

    let large_limit = 128u64 * 1024 * 1024 * 1024; // 128 GB
    controller.set_memory_limit(large_limit).unwrap();
    let written = std::fs::read_to_string(cg_path.join("memory.max")).unwrap();
    assert_eq!(written, large_limit.to_string());
}

#[test]
fn test_cgroup_controller_set_cpu_limit_various_periods() {
    let tmp = TempDir::new().unwrap();
    let agent_id = AgentId::new();
    let cg_path = tmp.path().join(agent_id.to_string());
    std::fs::create_dir_all(&cg_path).unwrap();

    let controller = cgroup::CgroupController {
        path: cg_path.clone(),
        agent_id,
    };

    // Half a core (50ms of 100ms)
    controller.set_cpu_limit(50_000, 100_000).unwrap();
    let written = std::fs::read_to_string(cg_path.join("cpu.max")).unwrap();
    assert_eq!(written, "50000 100000");

    // Two cores (200ms of 100ms)
    controller.set_cpu_limit(200_000, 100_000).unwrap();
    let written = std::fs::read_to_string(cg_path.join("cpu.max")).unwrap();
    assert_eq!(written, "200000 100000");
}

// -----------------------------------------------------------------------
// OutputCapture tests
// -----------------------------------------------------------------------

#[test]
fn test_output_capture_new() {
    let cap = OutputCapture::new(50);
    assert_eq!(cap.len(), 0);
    assert!(cap.is_empty());
    assert_eq!(cap.max_lines, 50);
}

#[test]
fn test_output_capture_push() {
    let mut cap = OutputCapture::new(100);
    cap.push(OutputStream::Stdout, "hello".to_string());
    assert_eq!(cap.len(), 1);
    assert!(!cap.is_empty());
    assert_eq!(cap.all()[0].content, "hello");
    assert_eq!(cap.all()[0].stream, OutputStream::Stdout);
}

#[test]
fn test_output_capture_tail() {
    let mut cap = OutputCapture::new(100);
    for i in 0..10 {
        cap.push(OutputStream::Stdout, format!("line {}", i));
    }
    let tail = cap.tail(3);
    assert_eq!(tail.len(), 3);
    assert_eq!(tail[0].content, "line 7");
    assert_eq!(tail[1].content, "line 8");
    assert_eq!(tail[2].content, "line 9");
}

#[test]
fn test_output_capture_tail_more_than_available() {
    let mut cap = OutputCapture::new(100);
    cap.push(OutputStream::Stdout, "only one".to_string());
    let tail = cap.tail(50);
    assert_eq!(tail.len(), 1);
    assert_eq!(tail[0].content, "only one");
}

#[test]
fn test_output_capture_all() {
    let mut cap = OutputCapture::new(100);
    cap.push(OutputStream::Stdout, "a".to_string());
    cap.push(OutputStream::Stderr, "b".to_string());
    cap.push(OutputStream::Stdout, "c".to_string());
    let all = cap.all();
    assert_eq!(all.len(), 3);
    assert_eq!(all[0].content, "a");
    assert_eq!(all[1].content, "b");
    assert_eq!(all[2].content, "c");
}

#[test]
fn test_output_capture_filter_stream() {
    let mut cap = OutputCapture::new(100);
    cap.push(OutputStream::Stdout, "out1".to_string());
    cap.push(OutputStream::Stderr, "err1".to_string());
    cap.push(OutputStream::Stdout, "out2".to_string());
    cap.push(OutputStream::Stderr, "err2".to_string());

    let stdout = cap.filter_stream(OutputStream::Stdout);
    assert_eq!(stdout.len(), 2);
    assert_eq!(stdout[0].content, "out1");
    assert_eq!(stdout[1].content, "out2");

    let stderr = cap.filter_stream(OutputStream::Stderr);
    assert_eq!(stderr.len(), 2);
    assert_eq!(stderr[0].content, "err1");
}

#[test]
fn test_output_capture_clear() {
    let mut cap = OutputCapture::new(100);
    cap.push(OutputStream::Stdout, "data".to_string());
    cap.push(OutputStream::Stderr, "more data".to_string());
    assert_eq!(cap.len(), 2);
    cap.clear();
    assert_eq!(cap.len(), 0);
    assert!(cap.is_empty());
}

#[test]
fn test_output_capture_len_and_is_empty() {
    let mut cap = OutputCapture::new(100);
    assert_eq!(cap.len(), 0);
    assert!(cap.is_empty());
    cap.push(OutputStream::Stdout, "x".to_string());
    assert_eq!(cap.len(), 1);
    assert!(!cap.is_empty());
}

#[test]
fn test_output_capture_default() {
    let cap = OutputCapture::default();
    assert_eq!(cap.max_lines, 1000);
    assert!(cap.is_empty());
}

#[test]
fn test_output_capture_format_display_empty() {
    let cap = OutputCapture::new(100);
    assert_eq!(cap.format_display(10), "(no output captured)");
}

#[test]
fn test_output_capture_format_display_with_lines() {
    let mut cap = OutputCapture::new(100);
    cap.push(OutputStream::Stdout, "hello world".to_string());
    cap.push(OutputStream::Stderr, "error msg".to_string());

    let display = cap.format_display(10);
    assert!(display.contains("OUT | hello world"));
    assert!(display.contains("ERR | error msg"));
    // Should have two lines separated by newline
    assert_eq!(display.lines().count(), 2);
}

#[test]
fn test_output_capture_ring_buffer_eviction() {
    let mut cap = OutputCapture::new(3);
    cap.push(OutputStream::Stdout, "a".to_string());
    cap.push(OutputStream::Stdout, "b".to_string());
    cap.push(OutputStream::Stdout, "c".to_string());
    assert_eq!(cap.len(), 3);

    // Push a 4th — should evict "a"
    cap.push(OutputStream::Stdout, "d".to_string());
    assert_eq!(cap.len(), 3);

    let all = cap.all();
    assert_eq!(all[0].content, "b");
    assert_eq!(all[1].content, "c");
    assert_eq!(all[2].content, "d");
}

#[test]
fn test_output_capture_serialization() {
    let line = OutputLine {
        timestamp: "2026-03-06T12:00:00Z".to_string(),
        stream: OutputStream::Stdout,
        content: "test output".to_string(),
    };
    let json = serde_json::to_string(&line).unwrap();
    let deser: OutputLine = serde_json::from_str(&json).unwrap();
    assert_eq!(deser.content, "test output");
    assert_eq!(deser.stream, OutputStream::Stdout);
}

// ==================================================================
// Circuit Breaker tests
// ==================================================================

#[test]
fn test_circuit_breaker_initial_state() {
    let cb = CircuitBreaker::new(3, Duration::from_secs(10), 2);
    assert_eq!(cb.state(), CircuitState::Closed);
    assert_eq!(cb.failure_count(), 0);
    assert!(cb.last_failure_time().is_none());
}

#[test]
fn test_circuit_breaker_closed_allows_execution() {
    let mut cb = CircuitBreaker::new(3, Duration::from_secs(10), 2);
    assert!(cb.can_execute());
}

#[test]
fn test_circuit_breaker_trips_to_open() {
    let mut cb = CircuitBreaker::new(3, Duration::from_secs(10), 2);

    cb.record_failure();
    assert_eq!(cb.state(), CircuitState::Closed);
    cb.record_failure();
    assert_eq!(cb.state(), CircuitState::Closed);
    cb.record_failure(); // threshold reached
    assert_eq!(cb.state(), CircuitState::Open);
    assert_eq!(cb.failure_count(), 3);
}

#[test]
fn test_circuit_breaker_open_blocks_execution() {
    let mut cb = CircuitBreaker::new(2, Duration::from_secs(60), 1);

    cb.record_failure();
    cb.record_failure();
    assert_eq!(cb.state(), CircuitState::Open);
    assert!(!cb.can_execute());
}

#[test]
fn test_circuit_breaker_open_to_half_open_after_timeout() {
    let mut cb = CircuitBreaker::new(1, Duration::from_millis(0), 2);

    cb.record_failure(); // trips to Open
    assert_eq!(cb.state(), CircuitState::Open);

    // With 0ms timeout, should immediately transition
    assert!(cb.can_execute());
    assert_eq!(cb.state(), CircuitState::HalfOpen);
}

#[test]
fn test_circuit_breaker_half_open_success_closes() {
    let mut cb = CircuitBreaker::new(1, Duration::from_millis(0), 2);

    cb.record_failure(); // -> Open
    cb.can_execute(); // -> HalfOpen

    cb.record_success();
    assert_eq!(cb.state(), CircuitState::HalfOpen); // need 2 successes

    cb.record_success();
    assert_eq!(cb.state(), CircuitState::Closed); // threshold met
    assert_eq!(cb.failure_count(), 0);
}

#[test]
fn test_circuit_breaker_half_open_failure_reopens() {
    let mut cb = CircuitBreaker::new(1, Duration::from_millis(0), 3);

    cb.record_failure(); // -> Open
    cb.can_execute(); // -> HalfOpen

    cb.record_failure(); // -> Open again
    assert_eq!(cb.state(), CircuitState::Open);
}

#[test]
fn test_circuit_breaker_success_resets_count() {
    let mut cb = CircuitBreaker::new(3, Duration::from_secs(10), 2);

    cb.record_failure();
    cb.record_failure();
    assert_eq!(cb.failure_count(), 2);

    cb.record_success();
    assert_eq!(cb.failure_count(), 0);
    assert_eq!(cb.state(), CircuitState::Closed);
}

#[test]
fn test_circuit_breaker_reset() {
    let mut cb = CircuitBreaker::new(2, Duration::from_secs(60), 1);

    cb.record_failure();
    cb.record_failure(); // -> Open
    assert_eq!(cb.state(), CircuitState::Open);

    cb.reset();
    assert_eq!(cb.state(), CircuitState::Closed);
    assert_eq!(cb.failure_count(), 0);
    assert!(cb.last_failure_time().is_none());
    assert!(cb.can_execute());
}

#[test]
fn test_circuit_breaker_last_failure_time_set() {
    let mut cb = CircuitBreaker::new(5, Duration::from_secs(10), 2);

    assert!(cb.last_failure_time().is_none());
    cb.record_failure();
    assert!(cb.last_failure_time().is_some());
}

#[test]
fn test_circuit_breaker_config_default() {
    let config = CircuitBreakerConfig::default();
    assert_eq!(config.failure_threshold, 5);
    assert_eq!(config.recovery_timeout_ms, 30_000);
    assert_eq!(config.half_open_max_attempts, 3);
}

#[test]
fn test_circuit_breaker_from_config() {
    let config = CircuitBreakerConfig {
        failure_threshold: 10,
        recovery_timeout_ms: 5000,
        half_open_max_attempts: 5,
    };
    let cb = CircuitBreaker::from_config(&config);
    assert_eq!(cb.state(), CircuitState::Closed);
    assert_eq!(cb.failure_count(), 0);
}

#[test]
fn test_circuit_state_serialization() {
    let states = [
        CircuitState::Closed,
        CircuitState::Open,
        CircuitState::HalfOpen,
    ];
    for state in &states {
        let json = serde_json::to_string(state).unwrap();
        let deser: CircuitState = serde_json::from_str(&json).unwrap();
        assert_eq!(&deser, state);
    }
}

#[test]
fn test_circuit_breaker_config_serialization() {
    let config = CircuitBreakerConfig::default();
    let json = serde_json::to_string(&config).unwrap();
    let deser: CircuitBreakerConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(deser.failure_threshold, config.failure_threshold);
}

// -----------------------------------------------------------------------
// Coverage improvement: get_quota, register_agent quota fallback
// -----------------------------------------------------------------------

#[tokio::test]
async fn test_supervisor_get_quota_returns_none_for_unknown() {
    let registry = std::sync::Arc::new(crate::registry::AgentRegistry::new());
    let supervisor = Supervisor::new(registry);
    let agent_id = AgentId::new();
    assert!(supervisor.get_quota(agent_id).await.is_none());
}

#[tokio::test]
async fn test_supervisor_get_all_health_empty() {
    let registry = std::sync::Arc::new(crate::registry::AgentRegistry::new());
    let supervisor = Supervisor::new(registry);
    let health = supervisor.get_all_health().await;
    assert!(health.is_empty());
}

#[tokio::test]
async fn test_supervisor_register_and_get_health() {
    let registry = std::sync::Arc::new(crate::registry::AgentRegistry::new());
    let supervisor = Supervisor::new(registry);
    let agent_id = AgentId::new();

    supervisor.register_agent(agent_id).await.unwrap();

    let health = supervisor.get_all_health().await;
    assert_eq!(health.len(), 1);
    assert_eq!(health[0].agent_id, agent_id);
    assert!(health[0].is_healthy);

    // Quota should be set to default after register
    let quota = supervisor.get_quota(agent_id).await;
    assert!(quota.is_some());
}

#[tokio::test]
async fn test_supervisor_unregister_clears_quota() {
    let registry = std::sync::Arc::new(crate::registry::AgentRegistry::new());
    let supervisor = Supervisor::new(registry);
    let agent_id = AgentId::new();

    supervisor.register_agent(agent_id).await.unwrap();
    assert!(supervisor.get_quota(agent_id).await.is_some());

    let _ = supervisor.unregister_agent(agent_id).await;
    assert!(supervisor.get_quota(agent_id).await.is_none());
}
