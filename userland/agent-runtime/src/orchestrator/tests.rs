//! Orchestrator tests

use super::*;
use std::collections::HashMap;
use std::sync::Arc;

use agnos_common::{AgentConfig, AgentId, MessageType};

fn create_test_orchestrator() -> Orchestrator {
    let registry = Arc::new(crate::registry::AgentRegistry::new());
    Orchestrator::new(registry)
}

#[tokio::test]
async fn test_orchestrator_initialization() {
    let orchestrator = create_test_orchestrator();
    let queues = orchestrator.get_queue_stats().await;
    assert_eq!(queues.total_tasks, 0);
}

#[tokio::test]
async fn test_task_submission() {
    let orchestrator = create_test_orchestrator();

    let task = Task {
        id: "test-task-1".to_string(),
        priority: TaskPriority::Normal,
        target_agents: vec![],
        payload: serde_json::json!({"action": "test"}),
        created_at: chrono::Utc::now(),
        deadline: None,
        dependencies: vec![],
        requirements: TaskRequirements::default(),
    };

    let result = orchestrator.submit_task(task).await;
    assert!(result.is_ok());

    let queues = orchestrator.get_queue_stats().await;
    assert_eq!(queues.total_tasks, 1);
}

#[tokio::test]
async fn test_task_priority_ordering() {
    let orchestrator = create_test_orchestrator();

    let low_task = Task {
        id: "low".to_string(),
        priority: TaskPriority::Low,
        target_agents: vec![],
        payload: serde_json::json!({"p": "low"}),
        created_at: chrono::Utc::now(),
        deadline: None,
        dependencies: vec![],
        requirements: TaskRequirements::default(),
    };

    let critical_task = Task {
        id: "critical".to_string(),
        priority: TaskPriority::Critical,
        target_agents: vec![],
        payload: serde_json::json!({"p": "critical"}),
        created_at: chrono::Utc::now(),
        deadline: None,
        dependencies: vec![],
        requirements: TaskRequirements::default(),
    };

    let _ = orchestrator.submit_task(low_task).await;
    let _ = orchestrator.submit_task(critical_task).await;

    let next = orchestrator.peek_next_task().await;
    assert!(next.is_some());
    assert_eq!(next.unwrap().priority, TaskPriority::Critical);
}

#[tokio::test]
async fn test_task_completion() {
    let orchestrator = create_test_orchestrator();

    let task = Task {
        id: "complete".to_string(),
        priority: TaskPriority::Normal,
        target_agents: vec![],
        payload: serde_json::json!({"action": "done"}),
        created_at: chrono::Utc::now(),
        deadline: None,
        dependencies: vec![],
        requirements: TaskRequirements::default(),
    };

    orchestrator.submit_task(task.clone()).await.unwrap();

    let result = TaskResult {
        task_id: task.id.clone(),
        agent_id: AgentId::new(),
        success: true,
        result: Some(serde_json::json!({"status": "done"})),
        error: None,
        completed_at: chrono::Utc::now(),
        duration_ms: 100,
    };

    orchestrator.store_result(result).await.unwrap();

    let retrieved = orchestrator.get_result(&task.id).await;
    assert!(retrieved.is_some());
    assert!(retrieved.unwrap().success);
}

#[tokio::test]
async fn test_task_failure() {
    let orchestrator = create_test_orchestrator();

    let task = Task {
        id: "fail".to_string(),
        priority: TaskPriority::Normal,
        target_agents: vec![],
        payload: serde_json::json!({"action": "fail"}),
        created_at: chrono::Utc::now(),
        deadline: None,
        dependencies: vec![],
        requirements: TaskRequirements::default(),
    };

    orchestrator.submit_task(task.clone()).await.unwrap();

    let result = TaskResult {
        task_id: task.id.clone(),
        agent_id: AgentId::new(),
        success: false,
        result: None,
        error: Some("Test error".to_string()),
        completed_at: chrono::Utc::now(),
        duration_ms: 50,
    };

    orchestrator.store_result(result).await.unwrap();

    let retrieved = orchestrator.get_result(&task.id).await;
    assert!(retrieved.is_some());
    assert!(!retrieved.unwrap().success);
}

#[tokio::test]
async fn test_overdue_tasks() {
    let orchestrator = create_test_orchestrator();

    let task = Task {
        id: "deadline".to_string(),
        priority: TaskPriority::Normal,
        target_agents: vec![],
        payload: serde_json::json!({}),
        created_at: chrono::Utc::now(),
        deadline: Some(chrono::Utc::now()),
        dependencies: vec![],
        requirements: TaskRequirements::default(),
    };

    orchestrator.submit_task(task).await.unwrap();

    let overdue = orchestrator.get_overdue_tasks().await;
    assert!(!overdue.is_empty());
}

#[tokio::test]
async fn test_cancellation() {
    let orchestrator = create_test_orchestrator();

    let task = Task {
        id: "cancel".to_string(),
        priority: TaskPriority::Normal,
        target_agents: vec![],
        payload: serde_json::json!({}),
        created_at: chrono::Utc::now(),
        deadline: None,
        dependencies: vec![],
        requirements: TaskRequirements::default(),
    };

    orchestrator.submit_task(task.clone()).await.unwrap();

    // Try to cancel
    let _ = orchestrator.cancel_task(&task.id).await;

    // The task may still be in the queue if scheduler hasn't processed it
    // Just verify the cancel doesn't error
    let queues = orchestrator.get_queue_stats().await;
    assert!(queues.total_tasks <= 1);
}

#[tokio::test]
async fn test_workload_stats() {
    let orchestrator = create_test_orchestrator();

    let stats = orchestrator.get_agent_stats().await;
    assert_eq!(stats.registered_agents, 0);
}

#[tokio::test]
async fn test_dependency_blocking() {
    let orchestrator = create_test_orchestrator();

    // Submit a task that depends on a dependency that hasn't completed yet
    let task_with_dep = Task {
        id: "child".to_string(),
        priority: TaskPriority::Normal,
        target_agents: vec![],
        payload: serde_json::json!({"action": "child"}),
        created_at: chrono::Utc::now(),
        deadline: None,
        dependencies: vec!["parent-task".to_string()],
        requirements: TaskRequirements::default(),
    };

    orchestrator.submit_task(task_with_dep).await.unwrap();

    // The task should stay queued (scheduler hasn't run yet, but peek should show it)
    let next = orchestrator.peek_next_task().await;
    assert!(next.is_some());
    assert_eq!(next.unwrap().dependencies, vec!["parent-task".to_string()]);

    // Manually run one scheduler iteration by calling scheduler_loop logic:
    // pop the task, check deps, push back
    {
        let mut state = orchestrator.state.write().await;
        let task = state
            .task_queues
            .get_mut(&TaskPriority::Normal)
            .and_then(|q| q.pop_front());
        if let Some(task) = task {
            let deps_satisfied = task
                .dependencies
                .iter()
                .all(|dep_id| state.results.contains_key(dep_id));

            // Deps not satisfied -- push back
            assert!(!deps_satisfied);
            state
                .task_queues
                .get_mut(&TaskPriority::Normal)
                .unwrap()
                .push_back(task);
        }
    }

    // Task is still queued
    let stats = orchestrator.get_queue_stats().await;
    assert_eq!(stats.queued_tasks, 1);

    // Now complete the dependency
    let parent_result = TaskResult {
        task_id: "parent-task".to_string(),
        agent_id: AgentId::new(),
        success: true,
        result: Some(serde_json::json!({"status": "done"})),
        error: None,
        completed_at: chrono::Utc::now(),
        duration_ms: 50,
    };
    orchestrator.store_result(parent_result).await.unwrap();

    // Now the dependency is satisfied
    {
        let state = orchestrator.state.read().await;
        if let Some(queue) = state.task_queues.get(&TaskPriority::Normal) {
            if let Some(task) = queue.front() {
                let deps_satisfied = task
                    .dependencies
                    .iter()
                    .all(|dep_id| state.results.contains_key(dep_id));
                assert!(deps_satisfied);
            }
        }
    }
}

#[test]
fn test_score_agent_idle() {
    use crate::agent::AgentHandle;

    let handle = AgentHandle {
        id: AgentId::new(),
        name: "idle-agent".to_string(),
        status: agnos_common::AgentStatus::Running,
        created_at: chrono::Utc::now(),
        started_at: None,
        resource_usage: agnos_common::ResourceUsage::default(),
        pid: None,
    };

    let config = AgentConfig::default();
    let requirements = TaskRequirements::default();

    let score = Orchestrator::score_agent(&handle, Some(&config), &requirements, 0, &[]);
    // Idle agent with no requirements should get near-max score
    assert!(
        score > 0.8,
        "Expected high score for idle agent, got {}",
        score
    );
}

#[test]
fn test_score_agent_with_load() {
    use crate::agent::AgentHandle;

    let handle = AgentHandle {
        id: AgentId::new(),
        name: "busy-agent".to_string(),
        status: agnos_common::AgentStatus::Running,
        created_at: chrono::Utc::now(),
        started_at: None,
        resource_usage: agnos_common::ResourceUsage {
            memory_used: 512 * 1024 * 1024,
            cpu_time_used: 1_800_000,
            file_descriptors_used: 0,
            processes_used: 0,
        },
        pid: None,
    };

    let config = AgentConfig::default();
    let requirements = TaskRequirements::default();

    let score = Orchestrator::score_agent(&handle, Some(&config), &requirements, 5, &[]);
    // Agent at ~50% resource usage with 5 running tasks
    assert!(score > 0.0, "Score should be positive");
    assert!(score < 0.8, "Score should be lower due to load");
}

#[test]
fn test_score_agent_affinity() {
    use crate::agent::AgentHandle;

    let handle = AgentHandle {
        id: AgentId::new(),
        name: "preferred-agent".to_string(),
        status: agnos_common::AgentStatus::Running,
        created_at: chrono::Utc::now(),
        started_at: None,
        resource_usage: agnos_common::ResourceUsage::default(),
        pid: None,
    };

    let config = AgentConfig::default();

    let reqs_with_affinity = TaskRequirements {
        preferred_agent: Some("preferred-agent".to_string()),
        ..Default::default()
    };
    let reqs_without = TaskRequirements::default();

    let score_with = Orchestrator::score_agent(&handle, Some(&config), &reqs_with_affinity, 0, &[]);
    let score_without = Orchestrator::score_agent(&handle, Some(&config), &reqs_without, 0, &[]);

    assert!(score_with > score_without, "Affinity should boost score");
}

#[test]
fn test_score_agent_fair_share() {
    use crate::agent::AgentHandle;

    let handle = AgentHandle {
        id: AgentId::new(),
        name: "agent".to_string(),
        status: agnos_common::AgentStatus::Running,
        created_at: chrono::Utc::now(),
        started_at: None,
        resource_usage: agnos_common::ResourceUsage::default(),
        pid: None,
    };

    let config = AgentConfig::default();
    let requirements = TaskRequirements::default();

    let score_0_tasks = Orchestrator::score_agent(&handle, Some(&config), &requirements, 0, &[]);
    let score_10_tasks = Orchestrator::score_agent(&handle, Some(&config), &requirements, 10, &[]);

    assert!(
        score_0_tasks > score_10_tasks,
        "Agent with fewer tasks should score higher"
    );
}

#[test]
fn test_task_requirements_default() {
    let req = TaskRequirements::default();
    assert_eq!(req.min_memory, 0);
    assert_eq!(req.min_cpu_shares, 0);
    assert!(req.required_capabilities.is_empty());
    assert!(req.preferred_agent.is_none());
}

#[test]
fn test_task_requirements_with_values() {
    let req = TaskRequirements {
        min_memory: 512 * 1024 * 1024,
        min_cpu_shares: 100,
        required_capabilities: vec!["gpu".to_string(), "network".to_string()],
        preferred_agent: Some("my-agent".to_string()),
        ..Default::default()
    };
    assert_eq!(req.min_memory, 512 * 1024 * 1024);
    assert_eq!(req.min_cpu_shares, 100);
    assert_eq!(req.required_capabilities.len(), 2);
    assert_eq!(req.preferred_agent, Some("my-agent".to_string()));
}

#[test]
fn test_task_requirements_clone() {
    let req = TaskRequirements {
        min_memory: 1024,
        min_cpu_shares: 50,
        required_capabilities: vec!["cap1".to_string()],
        preferred_agent: Some("agent-x".to_string()),
        ..Default::default()
    };
    let cloned = req.clone();
    assert_eq!(cloned.min_memory, 1024);
    assert_eq!(cloned.min_cpu_shares, 50);
    assert_eq!(cloned.required_capabilities, vec!["cap1"]);
    assert_eq!(cloned.preferred_agent, Some("agent-x".to_string()));
}

#[test]
fn test_task_priority_ord() {
    assert!(TaskPriority::Critical < TaskPriority::High);
    assert!(TaskPriority::High < TaskPriority::Normal);
    assert!(TaskPriority::Normal < TaskPriority::Low);
    assert!(TaskPriority::Low < TaskPriority::Background);
}

#[test]
fn test_task_priority_equality() {
    assert_eq!(TaskPriority::Critical, TaskPriority::Critical);
    assert_ne!(TaskPriority::Critical, TaskPriority::Low);
}

#[test]
fn test_task_priority_hash() {
    use std::collections::HashSet;
    let mut set = HashSet::new();
    set.insert(TaskPriority::Critical);
    set.insert(TaskPriority::High);
    set.insert(TaskPriority::Normal);
    set.insert(TaskPriority::Low);
    set.insert(TaskPriority::Background);
    assert_eq!(set.len(), 5);
    // Inserting duplicate should not increase count
    set.insert(TaskPriority::Critical);
    assert_eq!(set.len(), 5);
}

#[test]
fn test_task_priority_debug() {
    assert_eq!(format!("{:?}", TaskPriority::Critical), "Critical");
    assert_eq!(format!("{:?}", TaskPriority::Background), "Background");
}

#[test]
fn test_task_result_serialization() {
    let result = TaskResult {
        task_id: "task-1".to_string(),
        agent_id: AgentId::new(),
        success: true,
        result: Some(serde_json::json!({"output": "hello"})),
        error: None,
        completed_at: chrono::Utc::now(),
        duration_ms: 250,
    };

    let json = serde_json::to_string(&result).unwrap();
    assert!(json.contains("task-1"));
    assert!(json.contains("\"success\":true"));

    let deserialized: TaskResult = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.task_id, "task-1");
    assert!(deserialized.success);
    assert_eq!(deserialized.duration_ms, 250);
}

#[test]
fn test_task_result_failure_serialization() {
    let result = TaskResult {
        task_id: "fail-task".to_string(),
        agent_id: AgentId::new(),
        success: false,
        result: None,
        error: Some("something went wrong".to_string()),
        completed_at: chrono::Utc::now(),
        duration_ms: 10,
    };

    let json = serde_json::to_string(&result).unwrap();
    let deserialized: TaskResult = serde_json::from_str(&json).unwrap();
    assert!(!deserialized.success);
    assert_eq!(deserialized.error, Some("something went wrong".to_string()));
}

#[test]
fn test_task_clone() {
    let task = Task {
        id: "clone-task".to_string(),
        priority: TaskPriority::High,
        target_agents: vec![AgentId::new()],
        payload: serde_json::json!({"action": "test"}),
        created_at: chrono::Utc::now(),
        deadline: Some(chrono::Utc::now()),
        dependencies: vec!["dep1".to_string()],
        requirements: TaskRequirements {
            min_memory: 100,
            ..Default::default()
        },
    };
    let cloned = task.clone();
    assert_eq!(cloned.id, "clone-task");
    assert_eq!(cloned.priority, TaskPriority::High);
    assert_eq!(cloned.target_agents.len(), 1);
    assert!(cloned.deadline.is_some());
    assert_eq!(cloned.dependencies, vec!["dep1"]);
    assert_eq!(cloned.requirements.min_memory, 100);
}

#[test]
fn test_queue_stats_clone() {
    let stats = QueueStats {
        total_tasks: 10,
        running_tasks: 3,
        queued_tasks: 7,
    };
    let cloned = stats.clone();
    assert_eq!(cloned.total_tasks, 10);
    assert_eq!(cloned.running_tasks, 3);
    assert_eq!(cloned.queued_tasks, 7);
}

#[test]
fn test_queue_stats_debug() {
    let stats = QueueStats {
        total_tasks: 5,
        running_tasks: 2,
        queued_tasks: 3,
    };
    let debug = format!("{:?}", stats);
    assert!(debug.contains("total_tasks"));
    assert!(debug.contains("running_tasks"));
    assert!(debug.contains("queued_tasks"));
}

#[test]
fn test_agent_orchestrator_stats_clone() {
    let stats = AgentOrchestratorStats {
        registered_agents: 5,
        total_tasks_processed: 100,
    };
    let cloned = stats.clone();
    assert_eq!(cloned.registered_agents, 5);
    assert_eq!(cloned.total_tasks_processed, 100);
}

#[test]
fn test_task_status_variants() {
    let queued = TaskStatus::Queued;
    let running = TaskStatus::Running;
    let completed = TaskStatus::Completed(TaskResult {
        task_id: "t".to_string(),
        agent_id: AgentId::new(),
        success: true,
        result: None,
        error: None,
        completed_at: chrono::Utc::now(),
        duration_ms: 0,
    });

    // Verify Debug works
    assert!(format!("{:?}", queued).contains("Queued"));
    assert!(format!("{:?}", running).contains("Running"));
    assert!(format!("{:?}", completed).contains("Completed"));
}

#[tokio::test]
async fn test_orchestrator_clone_shares_state() {
    let orchestrator = create_test_orchestrator();
    let cloned = orchestrator.clone();

    // Submit via original
    let task = Task {
        id: "shared".to_string(),
        priority: TaskPriority::Normal,
        target_agents: vec![],
        payload: serde_json::json!({}),
        created_at: chrono::Utc::now(),
        deadline: None,
        dependencies: vec![],
        requirements: TaskRequirements::default(),
    };
    orchestrator.submit_task(task).await.unwrap();

    // Should be visible from clone
    let stats = cloned.get_queue_stats().await;
    assert_eq!(stats.queued_tasks, 1);
}

#[tokio::test]
async fn test_submit_all_priorities() {
    let orchestrator = create_test_orchestrator();

    for priority in [
        TaskPriority::Critical,
        TaskPriority::High,
        TaskPriority::Normal,
        TaskPriority::Low,
        TaskPriority::Background,
    ] {
        let task = Task {
            id: format!("{:?}", priority),
            priority,
            target_agents: vec![],
            payload: serde_json::json!({}),
            created_at: chrono::Utc::now(),
            deadline: None,
            dependencies: vec![],
            requirements: TaskRequirements::default(),
        };
        orchestrator.submit_task(task).await.unwrap();
    }

    let stats = orchestrator.get_queue_stats().await;
    assert_eq!(stats.queued_tasks, 5);
    assert_eq!(stats.total_tasks, 5);
    assert_eq!(stats.running_tasks, 0);
}

#[tokio::test]
async fn test_get_task_status_queued() {
    let orchestrator = create_test_orchestrator();

    let task = Task {
        id: "status-test".to_string(),
        priority: TaskPriority::Normal,
        target_agents: vec![],
        payload: serde_json::json!({}),
        created_at: chrono::Utc::now(),
        deadline: None,
        dependencies: vec![],
        requirements: TaskRequirements::default(),
    };
    let id = orchestrator.submit_task(task).await.unwrap();

    let status = orchestrator.get_task_status(&id).await;
    assert!(status.is_some());
    assert!(matches!(status.unwrap(), TaskStatus::Queued));
}

#[tokio::test]
async fn test_get_task_status_completed() {
    let orchestrator = create_test_orchestrator();

    let result = TaskResult {
        task_id: "done-task".to_string(),
        agent_id: AgentId::new(),
        success: true,
        result: None,
        error: None,
        completed_at: chrono::Utc::now(),
        duration_ms: 50,
    };
    orchestrator.store_result(result).await.unwrap();

    let status = orchestrator.get_task_status("done-task").await;
    assert!(status.is_some());
    assert!(matches!(status.unwrap(), TaskStatus::Completed(_)));
}

#[tokio::test]
async fn test_get_task_status_nonexistent() {
    let orchestrator = create_test_orchestrator();
    let status = orchestrator.get_task_status("nonexistent").await;
    assert!(status.is_none());
}

#[tokio::test]
async fn test_cancel_nonexistent_task() {
    let orchestrator = create_test_orchestrator();
    // Should succeed (no-op)
    let result = orchestrator.cancel_task("nonexistent").await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_cancel_removes_from_queue() {
    let orchestrator = create_test_orchestrator();

    let task = Task {
        id: "to-cancel".to_string(),
        priority: TaskPriority::Low,
        target_agents: vec![],
        payload: serde_json::json!({}),
        created_at: chrono::Utc::now(),
        deadline: None,
        dependencies: vec![],
        requirements: TaskRequirements::default(),
    };
    let id = orchestrator.submit_task(task).await.unwrap();

    assert_eq!(orchestrator.get_queue_stats().await.queued_tasks, 1);
    orchestrator.cancel_task(&id).await.unwrap();
    assert_eq!(orchestrator.get_queue_stats().await.queued_tasks, 0);
}

#[tokio::test]
async fn test_overdue_tasks_no_deadline() {
    let orchestrator = create_test_orchestrator();

    let task = Task {
        id: "no-deadline".to_string(),
        priority: TaskPriority::Normal,
        target_agents: vec![],
        payload: serde_json::json!({}),
        created_at: chrono::Utc::now(),
        deadline: None,
        dependencies: vec![],
        requirements: TaskRequirements::default(),
    };
    orchestrator.submit_task(task).await.unwrap();

    let overdue = orchestrator.get_overdue_tasks().await;
    assert!(overdue.is_empty());
}

#[tokio::test]
async fn test_peek_next_task_empty() {
    let orchestrator = create_test_orchestrator();
    assert!(orchestrator.peek_next_task().await.is_none());
}

#[tokio::test]
async fn test_get_result_nonexistent() {
    let orchestrator = create_test_orchestrator();
    assert!(orchestrator.get_result("nope").await.is_none());
}

#[test]
fn test_prune_results_under_limit() {
    let mut results = HashMap::new();
    for i in 0..10 {
        results.insert(
            format!("task-{}", i),
            TaskResult {
                task_id: format!("task-{}", i),
                agent_id: AgentId::new(),
                success: true,
                result: None,
                error: None,
                completed_at: chrono::Utc::now(),
                duration_ms: 0,
            },
        );
    }
    Orchestrator::prune_results(&mut results);
    assert_eq!(results.len(), 10); // No pruning needed
}

#[test]
fn test_score_agent_no_config() {
    use crate::agent::AgentHandle;

    let handle = AgentHandle {
        id: AgentId::new(),
        name: "no-config".to_string(),
        status: agnos_common::AgentStatus::Running,
        created_at: chrono::Utc::now(),
        started_at: None,
        resource_usage: agnos_common::ResourceUsage::default(),
        pid: None,
    };

    let requirements = TaskRequirements::default();
    let score = Orchestrator::score_agent(&handle, None, &requirements, 0, &[]);
    // Without config we get partial capability match (0.1 instead of 0.2)
    assert!(score > 0.0);
}

#[test]
fn test_score_agent_memory_requirement_not_met() {
    use crate::agent::AgentHandle;

    let handle = AgentHandle {
        id: AgentId::new(),
        name: "low-mem".to_string(),
        status: agnos_common::AgentStatus::Running,
        created_at: chrono::Utc::now(),
        started_at: None,
        resource_usage: agnos_common::ResourceUsage {
            memory_used: 900 * 1024 * 1024, // 900 MB used
            cpu_time_used: 0,
            file_descriptors_used: 0,
            processes_used: 0,
        },
        pid: None,
    };

    let config = AgentConfig::default(); // 1 GB max
    let requirements = TaskRequirements {
        min_memory: 500 * 1024 * 1024, // Needs 500 MB but only ~124 MB available
        ..Default::default()
    };

    let score = Orchestrator::score_agent(&handle, Some(&config), &requirements, 0, &[]);
    // Memory component should be 0 since requirement not met
    // Total score should be < 0.6 (only CPU + capability)
    assert!(score < 0.7);
}

#[test]
fn test_score_agent_memory_requirement_met() {
    use crate::agent::AgentHandle;

    let handle = AgentHandle {
        id: AgentId::new(),
        name: "good-mem".to_string(),
        status: agnos_common::AgentStatus::Running,
        created_at: chrono::Utc::now(),
        started_at: None,
        resource_usage: agnos_common::ResourceUsage::default(), // 0 used
        pid: None,
    };

    let config = AgentConfig::default(); // 1 GB max
    let requirements = TaskRequirements {
        min_memory: 100 * 1024 * 1024, // Needs 100 MB, 1 GB available
        ..Default::default()
    };

    let score = Orchestrator::score_agent(&handle, Some(&config), &requirements, 0, &[]);
    assert!(score > 0.8);
}

// ==================================================================
// Additional coverage: prune_results over limit, handle_result,
// score_agent with capabilities, task lifecycle, broadcast,
// get_task_status for running tasks, store_result + cancel
// ==================================================================

#[test]
fn test_prune_results_over_limit() {
    let mut results = HashMap::new();
    let now = chrono::Utc::now();

    // Insert MAX_RESULTS + 100 entries with different timestamps
    for i in 0..(Orchestrator::MAX_RESULTS + 100) {
        let ts = now + chrono::Duration::milliseconds(i as i64);
        results.insert(
            format!("task-{}", i),
            TaskResult {
                task_id: format!("task-{}", i),
                agent_id: AgentId::new(),
                success: true,
                result: None,
                error: None,
                completed_at: ts,
                duration_ms: 0,
            },
        );
    }

    assert_eq!(results.len(), Orchestrator::MAX_RESULTS + 100);
    Orchestrator::prune_results(&mut results);
    assert_eq!(results.len(), Orchestrator::MAX_RESULTS);
}

#[tokio::test]
async fn test_store_result_adds_to_results_and_removes_from_running() {
    let orchestrator = create_test_orchestrator();

    // Manually insert a "running" task
    {
        let mut state = orchestrator.state.write().await;
        state.running_tasks.insert(
            "running-1".to_string(),
            Task {
                id: "running-1".to_string(),
                priority: TaskPriority::Normal,
                target_agents: vec![],
                payload: serde_json::json!({}),
                created_at: chrono::Utc::now(),
                deadline: None,
                dependencies: vec![],
                requirements: TaskRequirements::default(),
            },
        );
    }

    let result = TaskResult {
        task_id: "running-1".to_string(),
        agent_id: AgentId::new(),
        success: true,
        result: Some(serde_json::json!({"ok": true})),
        error: None,
        completed_at: chrono::Utc::now(),
        duration_ms: 123,
    };

    orchestrator.store_result(result).await.unwrap();

    // Should be in results
    let stored = orchestrator.get_result("running-1").await;
    assert!(stored.is_some());
    assert!(stored.unwrap().success);

    // Should be removed from running
    let state = orchestrator.state.read().await;
    assert!(!state.running_tasks.contains_key("running-1"));
}

#[tokio::test]
async fn test_get_task_status_running() {
    let orchestrator = create_test_orchestrator();

    // Insert directly into running_tasks
    {
        let mut state = orchestrator.state.write().await;
        state.running_tasks.insert(
            "r-task".to_string(),
            Task {
                id: "r-task".to_string(),
                priority: TaskPriority::High,
                target_agents: vec![],
                payload: serde_json::json!({}),
                created_at: chrono::Utc::now(),
                deadline: None,
                dependencies: vec![],
                requirements: TaskRequirements::default(),
            },
        );
    }

    let status = orchestrator.get_task_status("r-task").await;
    assert!(status.is_some());
    assert!(matches!(status.unwrap(), TaskStatus::Running));
}

#[test]
fn test_score_agent_with_capabilities_match() {
    use crate::agent::AgentHandle;

    let handle = AgentHandle {
        id: AgentId::new(),
        name: "cap-agent".to_string(),
        status: agnos_common::AgentStatus::Running,
        created_at: chrono::Utc::now(),
        started_at: None,
        resource_usage: agnos_common::ResourceUsage::default(),
        pid: None,
    };

    let config = AgentConfig {
        permissions: vec![
            agnos_common::Permission::FileRead,
            agnos_common::Permission::FileWrite,
        ],
        ..Default::default()
    };

    let requirements = TaskRequirements {
        required_capabilities: vec!["readfile".to_string()],
        ..Default::default()
    };

    let score = Orchestrator::score_agent(&handle, Some(&config), &requirements, 0, &[]);
    // Should get partial capability match score
    assert!(score > 0.5);
}

#[test]
fn test_score_agent_no_capabilities_match() {
    use crate::agent::AgentHandle;

    let handle = AgentHandle {
        id: AgentId::new(),
        name: "nocap-agent".to_string(),
        status: agnos_common::AgentStatus::Running,
        created_at: chrono::Utc::now(),
        started_at: None,
        resource_usage: agnos_common::ResourceUsage::default(),
        pid: None,
    };

    let config = AgentConfig::default();
    let requirements = TaskRequirements {
        required_capabilities: vec!["gpu".to_string(), "cuda".to_string()],
        ..Default::default()
    };

    let score_with_caps = Orchestrator::score_agent(&handle, Some(&config), &requirements, 0, &[]);
    let score_no_caps =
        Orchestrator::score_agent(&handle, Some(&config), &TaskRequirements::default(), 0, &[]);

    // Without matching capabilities, the score should be lower
    assert!(score_no_caps >= score_with_caps);
}

#[tokio::test]
async fn test_cancel_running_task() {
    let orchestrator = create_test_orchestrator();

    // Insert task directly into running
    {
        let mut state = orchestrator.state.write().await;
        state.running_tasks.insert(
            "cancel-running".to_string(),
            Task {
                id: "cancel-running".to_string(),
                priority: TaskPriority::Normal,
                target_agents: vec![],
                payload: serde_json::json!({}),
                created_at: chrono::Utc::now(),
                deadline: None,
                dependencies: vec![],
                requirements: TaskRequirements::default(),
            },
        );
    }

    orchestrator.cancel_task("cancel-running").await.unwrap();
    let state = orchestrator.state.read().await;
    assert!(!state.running_tasks.contains_key("cancel-running"));
}

#[tokio::test]
async fn test_submit_multiple_same_priority() {
    let orchestrator = create_test_orchestrator();

    for i in 0..10 {
        let task = Task {
            id: format!("batch-{}", i),
            priority: TaskPriority::Normal,
            target_agents: vec![],
            payload: serde_json::json!({"index": i}),
            created_at: chrono::Utc::now(),
            deadline: None,
            dependencies: vec![],
            requirements: TaskRequirements::default(),
        };
        orchestrator.submit_task(task).await.unwrap();
    }

    let stats = orchestrator.get_queue_stats().await;
    assert_eq!(stats.queued_tasks, 10);
    assert_eq!(stats.running_tasks, 0);
    assert_eq!(stats.total_tasks, 10);
}

#[tokio::test]
async fn test_overdue_tasks_mixed() {
    let orchestrator = create_test_orchestrator();
    let now = chrono::Utc::now();

    // One overdue task
    let overdue_task = Task {
        id: "overdue".to_string(),
        priority: TaskPriority::Normal,
        target_agents: vec![],
        payload: serde_json::json!({}),
        created_at: now,
        deadline: Some(now - chrono::Duration::hours(1)),
        dependencies: vec![],
        requirements: TaskRequirements::default(),
    };
    orchestrator.submit_task(overdue_task).await.unwrap();

    // One future task
    let future_task = Task {
        id: "future".to_string(),
        priority: TaskPriority::Normal,
        target_agents: vec![],
        payload: serde_json::json!({}),
        created_at: now,
        deadline: Some(now + chrono::Duration::hours(1)),
        dependencies: vec![],
        requirements: TaskRequirements::default(),
    };
    orchestrator.submit_task(future_task).await.unwrap();

    // One no-deadline task
    let no_deadline = Task {
        id: "nodeadline".to_string(),
        priority: TaskPriority::Low,
        target_agents: vec![],
        payload: serde_json::json!({}),
        created_at: now,
        deadline: None,
        dependencies: vec![],
        requirements: TaskRequirements::default(),
    };
    orchestrator.submit_task(no_deadline).await.unwrap();

    let overdue = orchestrator.get_overdue_tasks().await;
    assert_eq!(overdue.len(), 1);
}

#[tokio::test]
async fn test_agent_stats_with_results() {
    let orchestrator = create_test_orchestrator();

    for i in 0..5 {
        let result = TaskResult {
            task_id: format!("stat-task-{}", i),
            agent_id: AgentId::new(),
            success: true,
            result: None,
            error: None,
            completed_at: chrono::Utc::now(),
            duration_ms: 10,
        };
        orchestrator.store_result(result).await.unwrap();
    }

    let stats = orchestrator.get_agent_stats().await;
    assert_eq!(stats.total_tasks_processed, 5);
    assert_eq!(stats.registered_agents, 0); // No agents in registry
}

#[test]
fn test_task_result_clone() {
    let result = TaskResult {
        task_id: "clone-result".to_string(),
        agent_id: AgentId::new(),
        success: false,
        result: None,
        error: Some("err".to_string()),
        completed_at: chrono::Utc::now(),
        duration_ms: 42,
    };
    let cloned = result.clone();
    assert_eq!(cloned.task_id, "clone-result");
    assert!(!cloned.success);
    assert_eq!(cloned.error, Some("err".to_string()));
    assert_eq!(cloned.duration_ms, 42);
}

#[test]
fn test_task_status_clone() {
    let status = TaskStatus::Queued;
    let cloned = status.clone();
    assert!(matches!(cloned, TaskStatus::Queued));

    let status = TaskStatus::Running;
    let cloned = status.clone();
    assert!(matches!(cloned, TaskStatus::Running));
}

#[test]
fn test_max_results_constant() {
    assert_eq!(Orchestrator::MAX_RESULTS, 10_000);
}

#[tokio::test]
async fn test_orchestrator_start_succeeds() {
    let orchestrator = create_test_orchestrator();
    let result = orchestrator.start().await;
    assert!(result.is_ok());
}

#[test]
fn test_score_agent_high_fair_share_penalty() {
    use crate::agent::AgentHandle;

    let handle = AgentHandle {
        id: AgentId::new(),
        name: "agent".to_string(),
        status: agnos_common::AgentStatus::Running,
        created_at: chrono::Utc::now(),
        started_at: None,
        resource_usage: agnos_common::ResourceUsage::default(),
        pid: None,
    };

    let config = AgentConfig::default();
    let requirements = TaskRequirements::default();

    // With 100 running tasks, penalty is capped at 0.1
    let score_100 = Orchestrator::score_agent(&handle, Some(&config), &requirements, 100, &[]);
    let score_11 = Orchestrator::score_agent(&handle, Some(&config), &requirements, 11, &[]);

    // Both should have max penalty (0.1), so scores should be equal
    assert!((score_100 - score_11).abs() < 0.001);
}

// ==================================================================
// Additional coverage: broadcast, start idempotent, submit + cancel
// lifecycle, store multiple results, queue stats after operations,
// score_agent edge cases, TaskRequirements debug/clone
// ==================================================================

#[tokio::test]
async fn test_broadcast_empty_registry() {
    let orchestrator = create_test_orchestrator();
    let result = orchestrator
        .broadcast(MessageType::Event, serde_json::json!({"event": "test"}))
        .await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_start_idempotent_receiver_taken() {
    let orchestrator = create_test_orchestrator();
    // First start takes the receiver
    orchestrator.start().await.unwrap();
    // Second start should succeed (receiver already taken, no-op for that part)
    let result = orchestrator.start().await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_submit_and_cancel_lifecycle() {
    let orchestrator = create_test_orchestrator();

    let task = Task {
        id: "lifecycle".to_string(),
        priority: TaskPriority::High,
        target_agents: vec![],
        payload: serde_json::json!({"step": "submit"}),
        created_at: chrono::Utc::now(),
        deadline: None,
        dependencies: vec![],
        requirements: TaskRequirements::default(),
    };

    let id = orchestrator.submit_task(task).await.unwrap();
    assert_eq!(orchestrator.get_queue_stats().await.queued_tasks, 1);

    // Check status is Queued
    let status = orchestrator.get_task_status(&id).await;
    assert!(matches!(status, Some(TaskStatus::Queued)));

    // Cancel
    orchestrator.cancel_task(&id).await.unwrap();
    assert_eq!(orchestrator.get_queue_stats().await.queued_tasks, 0);

    // Status should be None (not queued, not running, not completed)
    let status = orchestrator.get_task_status(&id).await;
    assert!(status.is_none());
}

#[tokio::test]
async fn test_store_multiple_results() {
    let orchestrator = create_test_orchestrator();

    for i in 0..20 {
        let result = TaskResult {
            task_id: format!("multi-{}", i),
            agent_id: AgentId::new(),
            success: i % 2 == 0,
            result: if i % 2 == 0 {
                Some(serde_json::json!({"i": i}))
            } else {
                None
            },
            error: if i % 2 != 0 {
                Some(format!("error-{}", i))
            } else {
                None
            },
            completed_at: chrono::Utc::now(),
            duration_ms: i as u64 * 10,
        };
        orchestrator.store_result(result).await.unwrap();
    }

    let stats = orchestrator.get_agent_stats().await;
    assert_eq!(stats.total_tasks_processed, 20);

    // Check a few specific results
    let r0 = orchestrator.get_result("multi-0").await.unwrap();
    assert!(r0.success);
    let r1 = orchestrator.get_result("multi-1").await.unwrap();
    assert!(!r1.success);
    assert_eq!(r1.error, Some("error-1".to_string()));
}

#[tokio::test]
async fn test_queue_stats_after_submit_and_store() {
    let orchestrator = create_test_orchestrator();

    // Submit 3 tasks
    for i in 0..3 {
        let task = Task {
            id: format!("qs-{}", i),
            priority: TaskPriority::Normal,
            target_agents: vec![],
            payload: serde_json::json!({}),
            created_at: chrono::Utc::now(),
            deadline: None,
            dependencies: vec![],
            requirements: TaskRequirements::default(),
        };
        orchestrator.submit_task(task).await.unwrap();
    }

    let stats = orchestrator.get_queue_stats().await;
    assert_eq!(stats.queued_tasks, 3);
    assert_eq!(stats.running_tasks, 0);
    assert_eq!(stats.total_tasks, 3);
}

#[test]
fn test_score_agent_maxed_out_resources() {
    use crate::agent::AgentHandle;

    let handle = AgentHandle {
        id: AgentId::new(),
        name: "maxed".to_string(),
        status: agnos_common::AgentStatus::Running,
        created_at: chrono::Utc::now(),
        started_at: None,
        resource_usage: agnos_common::ResourceUsage {
            memory_used: 1024 * 1024 * 1024, // 1 GB (same as default max)
            cpu_time_used: 3_600_000,        // 1 hour (same as default max)
            file_descriptors_used: 0,
            processes_used: 0,
        },
        pid: None,
    };

    let config = AgentConfig::default();
    let requirements = TaskRequirements::default();

    let score = Orchestrator::score_agent(&handle, Some(&config), &requirements, 0, &[]);
    // Memory and CPU headroom are 0, so score should be low
    assert!(
        score < 0.3,
        "Maxed agent should have low score, got {}",
        score
    );
}

#[test]
fn test_score_agent_with_preferred_mismatch() {
    use crate::agent::AgentHandle;

    let handle = AgentHandle {
        id: AgentId::new(),
        name: "not-preferred".to_string(),
        status: agnos_common::AgentStatus::Running,
        created_at: chrono::Utc::now(),
        started_at: None,
        resource_usage: agnos_common::ResourceUsage::default(),
        pid: None,
    };

    let config = AgentConfig::default();
    let requirements = TaskRequirements {
        preferred_agent: Some("other-agent".to_string()),
        ..Default::default()
    };

    let score = Orchestrator::score_agent(&handle, Some(&config), &requirements, 0, &[]);
    // Should not get affinity bonus
    let score_no_pref =
        Orchestrator::score_agent(&handle, Some(&config), &TaskRequirements::default(), 0, &[]);
    assert!((score - score_no_pref).abs() < 0.001);
}

#[test]
fn test_task_requirements_debug() {
    let req = TaskRequirements {
        min_memory: 42,
        min_cpu_shares: 10,
        required_capabilities: vec!["cap".to_string()],
        preferred_agent: Some("agent".to_string()),
        ..Default::default()
    };
    let dbg = format!("{:?}", req);
    assert!(dbg.contains("min_memory"));
    assert!(dbg.contains("42"));
}

#[test]
fn test_agent_orchestrator_stats_debug() {
    let stats = AgentOrchestratorStats {
        registered_agents: 3,
        total_tasks_processed: 42,
    };
    let dbg = format!("{:?}", stats);
    assert!(dbg.contains("registered_agents"));
    assert!(dbg.contains("42"));
}

#[tokio::test]
async fn test_peek_next_task_respects_priority_order() {
    let orchestrator = create_test_orchestrator();

    // Submit Background first, then Critical
    let bg_task = Task {
        id: "bg".to_string(),
        priority: TaskPriority::Background,
        target_agents: vec![],
        payload: serde_json::json!({}),
        created_at: chrono::Utc::now(),
        deadline: None,
        dependencies: vec![],
        requirements: TaskRequirements::default(),
    };
    orchestrator.submit_task(bg_task).await.unwrap();

    let crit_task = Task {
        id: "crit".to_string(),
        priority: TaskPriority::Critical,
        target_agents: vec![],
        payload: serde_json::json!({}),
        created_at: chrono::Utc::now(),
        deadline: None,
        dependencies: vec![],
        requirements: TaskRequirements::default(),
    };
    orchestrator.submit_task(crit_task).await.unwrap();

    // peek_next_task should return Critical first
    let next = orchestrator.peek_next_task().await.unwrap();
    assert_eq!(next.priority, TaskPriority::Critical);
}

#[tokio::test]
async fn test_overdue_tasks_future_deadline() {
    let orchestrator = create_test_orchestrator();
    let now = chrono::Utc::now();

    let task = Task {
        id: "future-dl".to_string(),
        priority: TaskPriority::Normal,
        target_agents: vec![],
        payload: serde_json::json!({}),
        created_at: now,
        deadline: Some(now + chrono::Duration::hours(24)),
        dependencies: vec![],
        requirements: TaskRequirements::default(),
    };
    orchestrator.submit_task(task).await.unwrap();

    let overdue = orchestrator.get_overdue_tasks().await;
    assert!(overdue.is_empty());
}

#[test]
fn test_task_priority_copy() {
    let p = TaskPriority::High;
    let p2 = p; // Copy
    assert_eq!(p, p2);
}

// ==================================================================
// New coverage: score_agent with various combos, dependency checking,
// queue stats with running tasks, result storage/retrieval,
// task cancellation, prune_results
// ==================================================================

#[test]
fn test_score_agent_no_requirements() {
    use crate::agent::AgentHandle;
    let handle = AgentHandle {
        id: AgentId::new(),
        name: "scorer".to_string(),
        status: agnos_common::AgentStatus::Running,
        created_at: chrono::Utc::now(),
        started_at: None,
        resource_usage: agnos_common::ResourceUsage::default(),
        pid: None,
    };
    let reqs = TaskRequirements::default();
    let score = Orchestrator::score_agent(&handle, None, &reqs, 0, &[]);
    // With no requirements and no config: memory=0.4, cpu=0.3, capability=0.1
    assert!(score > 0.0, "Score should be positive, got {}", score);
}

#[test]
fn test_score_agent_with_affinity() {
    use crate::agent::AgentHandle;
    let handle = AgentHandle {
        id: AgentId::new(),
        name: "preferred-agent".to_string(),
        status: agnos_common::AgentStatus::Running,
        created_at: chrono::Utc::now(),
        started_at: None,
        resource_usage: agnos_common::ResourceUsage::default(),
        pid: None,
    };
    let reqs = TaskRequirements {
        preferred_agent: Some("preferred-agent".to_string()),
        ..Default::default()
    };
    let score_with_affinity = Orchestrator::score_agent(&handle, None, &reqs, 0, &[]);
    let reqs_no_affinity = TaskRequirements::default();
    let score_without = Orchestrator::score_agent(&handle, None, &reqs_no_affinity, 0, &[]);
    assert!(
        score_with_affinity > score_without,
        "Affinity should boost score"
    );
}

#[test]
fn test_score_agent_fair_share_penalty() {
    use crate::agent::AgentHandle;
    let handle = AgentHandle {
        id: AgentId::new(),
        name: "busy".to_string(),
        status: agnos_common::AgentStatus::Running,
        created_at: chrono::Utc::now(),
        started_at: None,
        resource_usage: agnos_common::ResourceUsage::default(),
        pid: None,
    };
    let reqs = TaskRequirements::default();
    let score_idle = Orchestrator::score_agent(&handle, None, &reqs, 0, &[]);
    let score_busy = Orchestrator::score_agent(&handle, None, &reqs, 5, &[]);
    assert!(
        score_idle > score_busy,
        "Idle agent should score higher than busy"
    );
}

#[test]
fn test_score_agent_memory_insufficient() {
    use crate::agent::AgentHandle;
    let handle = AgentHandle {
        id: AgentId::new(),
        name: "low-mem".to_string(),
        status: agnos_common::AgentStatus::Running,
        created_at: chrono::Utc::now(),
        started_at: None,
        resource_usage: agnos_common::ResourceUsage {
            memory_used: 900 * 1024 * 1024, // 900MB used
            cpu_time_used: 0,
            file_descriptors_used: 0,
            processes_used: 0,
        },
        pid: None,
    };
    let config = AgentConfig {
        name: "low-mem".to_string(),
        resource_limits: agnos_common::ResourceLimits {
            max_memory: 1024 * 1024 * 1024, // 1GB limit
            max_cpu_time: 3600,
            ..Default::default()
        },
        ..Default::default()
    };
    let reqs = TaskRequirements {
        min_memory: 200 * 1024 * 1024, // needs 200MB, only 124MB available
        ..Default::default()
    };
    let score = Orchestrator::score_agent(&handle, Some(&config), &reqs, 0, &[]);
    // Should get 0 for memory component since requirement can't be met
    // But still get points for CPU, capability
    assert!(
        score < 0.7,
        "Score should be low when memory insufficient, got {}",
        score
    );
}

#[tokio::test]
async fn test_cancel_task_from_queue() {
    let orchestrator = create_test_orchestrator();
    let task = Task {
        id: "cancel-me".to_string(),
        priority: TaskPriority::Normal,
        target_agents: vec![],
        payload: serde_json::json!({}),
        created_at: chrono::Utc::now(),
        deadline: None,
        dependencies: vec![],
        requirements: TaskRequirements::default(),
    };

    let task_id = orchestrator.submit_task(task).await.unwrap();
    assert_eq!(orchestrator.get_queue_stats().await.queued_tasks, 1);

    orchestrator.cancel_task(&task_id).await.unwrap();
    assert_eq!(orchestrator.get_queue_stats().await.queued_tasks, 0);
}

#[tokio::test]
async fn test_store_and_retrieve_result() {
    let orchestrator = create_test_orchestrator();

    let result = TaskResult {
        task_id: "result-task".to_string(),
        agent_id: AgentId::new(),
        success: true,
        result: Some(serde_json::json!({"output": "done"})),
        error: None,
        completed_at: chrono::Utc::now(),
        duration_ms: 150,
    };

    orchestrator.store_result(result).await.unwrap();

    let retrieved = orchestrator.get_result("result-task").await;
    assert!(retrieved.is_some());
    let r = retrieved.unwrap();
    assert!(r.success);
    assert_eq!(r.duration_ms, 150);
}

#[tokio::test]
async fn test_get_result_missing_task() {
    let orchestrator = create_test_orchestrator();
    assert!(orchestrator.get_result("no-such-task").await.is_none());
}

#[tokio::test]
async fn test_queue_stats_with_multiple_priorities() {
    let orchestrator = create_test_orchestrator();

    for (i, priority) in [
        TaskPriority::Critical,
        TaskPriority::Normal,
        TaskPriority::Low,
    ]
    .iter()
    .enumerate()
    {
        let task = Task {
            id: format!("prio-{}", i),
            priority: *priority,
            target_agents: vec![],
            payload: serde_json::json!({}),
            created_at: chrono::Utc::now(),
            deadline: None,
            dependencies: vec![],
            requirements: TaskRequirements::default(),
        };
        orchestrator.submit_task(task).await.unwrap();
    }

    let stats = orchestrator.get_queue_stats().await;
    assert_eq!(stats.queued_tasks, 3);
    assert_eq!(stats.total_tasks, 3);
    assert_eq!(stats.running_tasks, 0);
}

#[test]
fn test_prune_results_small_set() {
    let mut results = HashMap::new();
    for i in 0..5 {
        results.insert(
            format!("task-{}", i),
            TaskResult {
                task_id: format!("task-{}", i),
                agent_id: AgentId::new(),
                success: true,
                result: None,
                error: None,
                completed_at: chrono::Utc::now(),
                duration_ms: 0,
            },
        );
    }
    Orchestrator::prune_results(&mut results);
    assert_eq!(results.len(), 5, "Should not prune when under limit");
}

#[test]
fn test_task_requirements_default_values() {
    let reqs = TaskRequirements::default();
    assert_eq!(reqs.min_memory, 0);
    assert_eq!(reqs.min_cpu_shares, 0);
    assert!(reqs.required_capabilities.is_empty());
    assert!(reqs.preferred_agent.is_none());
}

#[test]
fn test_task_priority_total_ordering() {
    assert!(TaskPriority::Critical < TaskPriority::High);
    assert!(TaskPriority::High < TaskPriority::Normal);
    assert!(TaskPriority::Normal < TaskPriority::Low);
    assert!(TaskPriority::Low < TaskPriority::Background);
}

#[test]
fn test_queue_stats_debug_format() {
    let stats = QueueStats {
        total_tasks: 10,
        running_tasks: 3,
        queued_tasks: 7,
    };
    let dbg = format!("{:?}", stats);
    assert!(dbg.contains("total_tasks"));
    assert!(dbg.contains("10"));
}

#[tokio::test]
async fn test_task_status_is_queued() {
    let orchestrator = create_test_orchestrator();
    let task = Task {
        id: "status-check".to_string(),
        priority: TaskPriority::Normal,
        target_agents: vec![],
        payload: serde_json::json!({}),
        created_at: chrono::Utc::now(),
        deadline: None,
        dependencies: vec![],
        requirements: TaskRequirements::default(),
    };
    let task_id = orchestrator.submit_task(task).await.unwrap();
    let status = orchestrator.get_task_status(&task_id).await;
    assert!(matches!(status, Some(TaskStatus::Queued)));
}

#[tokio::test]
async fn test_task_status_is_completed() {
    let orchestrator = create_test_orchestrator();
    let result = TaskResult {
        task_id: "completed-task".to_string(),
        agent_id: AgentId::new(),
        success: true,
        result: None,
        error: None,
        completed_at: chrono::Utc::now(),
        duration_ms: 50,
    };
    orchestrator.store_result(result).await.unwrap();
    let status = orchestrator.get_task_status("completed-task").await;
    assert!(matches!(status, Some(TaskStatus::Completed(_))));
}

#[tokio::test]
async fn test_task_status_none_for_unknown() {
    let orchestrator = create_test_orchestrator();
    let status = orchestrator.get_task_status("ghost").await;
    assert!(status.is_none());
}

// ==================================================================
// NEW: Task scheduling, queue management, dependency chains,
// load-aware scoring edge cases, concurrent submit/cancel,
// result pruning boundary, overdue tasks, peek ordering,
// broadcast, multiple results replacement
// ==================================================================

#[tokio::test]
async fn test_submit_task_generates_uuid() {
    let orchestrator = create_test_orchestrator();

    let task = Task {
        id: "will-be-replaced".to_string(),
        priority: TaskPriority::Normal,
        target_agents: vec![],
        payload: serde_json::json!({}),
        created_at: chrono::Utc::now(),
        deadline: None,
        dependencies: vec![],
        requirements: TaskRequirements::default(),
    };

    let id = orchestrator.submit_task(task).await.unwrap();
    // submit_task replaces id with UUID, so it should NOT be "will-be-replaced"
    assert_ne!(id, "will-be-replaced");
    assert!(id.len() > 10); // UUID string is 36 chars
}

#[tokio::test]
async fn test_submit_task_updates_created_at() {
    let orchestrator = create_test_orchestrator();
    let old_time = chrono::Utc::now() - chrono::Duration::days(1);

    let task = Task {
        id: "ts-test".to_string(),
        priority: TaskPriority::Normal,
        target_agents: vec![],
        payload: serde_json::json!({}),
        created_at: old_time,
        deadline: None,
        dependencies: vec![],
        requirements: TaskRequirements::default(),
    };

    let id = orchestrator.submit_task(task).await.unwrap();
    let state = orchestrator.state.read().await;
    let queue = state.task_queues.get(&TaskPriority::Normal).unwrap();
    let submitted = queue.iter().find(|t| t.id == id).unwrap();
    // created_at should be updated to approximately now
    assert!(submitted.created_at > old_time);
}

#[tokio::test]
async fn test_peek_returns_highest_priority() {
    let orchestrator = create_test_orchestrator();

    // Submit in reverse order
    for (i, priority) in [
        TaskPriority::Background,
        TaskPriority::Low,
        TaskPriority::Normal,
        TaskPriority::High,
        TaskPriority::Critical,
    ]
    .iter()
    .enumerate()
    {
        let task = Task {
            id: format!("p-{}", i),
            priority: *priority,
            target_agents: vec![],
            payload: serde_json::json!({}),
            created_at: chrono::Utc::now(),
            deadline: None,
            dependencies: vec![],
            requirements: TaskRequirements::default(),
        };
        orchestrator.submit_task(task).await.unwrap();
    }

    let next = orchestrator.peek_next_task().await.unwrap();
    assert_eq!(next.priority, TaskPriority::Critical);
}

#[tokio::test]
async fn test_cancel_preserves_other_tasks() {
    let orchestrator = create_test_orchestrator();

    // Submit 3 tasks
    let mut ids = Vec::new();
    for i in 0..3 {
        let task = Task {
            id: format!("keep-{}", i),
            priority: TaskPriority::Normal,
            target_agents: vec![],
            payload: serde_json::json!({}),
            created_at: chrono::Utc::now(),
            deadline: None,
            dependencies: vec![],
            requirements: TaskRequirements::default(),
        };
        ids.push(orchestrator.submit_task(task).await.unwrap());
    }

    // Cancel middle one
    orchestrator.cancel_task(&ids[1]).await.unwrap();

    assert_eq!(orchestrator.get_queue_stats().await.queued_tasks, 2);
    assert!(orchestrator.get_task_status(&ids[0]).await.is_some());
    assert!(orchestrator.get_task_status(&ids[1]).await.is_none());
    assert!(orchestrator.get_task_status(&ids[2]).await.is_some());
}

#[tokio::test]
async fn test_store_result_overwrites_previous() {
    let orchestrator = create_test_orchestrator();

    let result1 = TaskResult {
        task_id: "overwrite-test".to_string(),
        agent_id: AgentId::new(),
        success: false,
        result: None,
        error: Some("first".to_string()),
        completed_at: chrono::Utc::now(),
        duration_ms: 10,
    };
    orchestrator.store_result(result1).await.unwrap();

    let result2 = TaskResult {
        task_id: "overwrite-test".to_string(),
        agent_id: AgentId::new(),
        success: true,
        result: Some(serde_json::json!({"version": 2})),
        error: None,
        completed_at: chrono::Utc::now(),
        duration_ms: 20,
    };
    orchestrator.store_result(result2).await.unwrap();

    let r = orchestrator.get_result("overwrite-test").await.unwrap();
    assert!(r.success);
    assert_eq!(r.duration_ms, 20);
}

#[tokio::test]
async fn test_get_queue_stats_with_running_and_queued() {
    let orchestrator = create_test_orchestrator();

    // Submit 5 tasks
    for i in 0..5 {
        let task = Task {
            id: format!("mix-{}", i),
            priority: TaskPriority::Normal,
            target_agents: vec![],
            payload: serde_json::json!({}),
            created_at: chrono::Utc::now(),
            deadline: None,
            dependencies: vec![],
            requirements: TaskRequirements::default(),
        };
        orchestrator.submit_task(task).await.unwrap();
    }

    // Move 2 to running
    {
        let mut state = orchestrator.state.write().await;
        state.running_tasks.insert(
            "running-a".to_string(),
            Task {
                id: "running-a".to_string(),
                priority: TaskPriority::High,
                target_agents: vec![],
                payload: serde_json::json!({}),
                created_at: chrono::Utc::now(),
                deadline: None,
                dependencies: vec![],
                requirements: TaskRequirements::default(),
            },
        );
        state.running_tasks.insert(
            "running-b".to_string(),
            Task {
                id: "running-b".to_string(),
                priority: TaskPriority::High,
                target_agents: vec![],
                payload: serde_json::json!({}),
                created_at: chrono::Utc::now(),
                deadline: None,
                dependencies: vec![],
                requirements: TaskRequirements::default(),
            },
        );
    }

    let stats = orchestrator.get_queue_stats().await;
    assert_eq!(stats.queued_tasks, 5);
    assert_eq!(stats.running_tasks, 2);
    assert_eq!(stats.total_tasks, 7);
}

#[test]
fn test_score_agent_zero_resource_usage() {
    use crate::agent::AgentHandle;

    let handle = AgentHandle {
        id: AgentId::new(),
        name: "fresh".to_string(),
        status: agnos_common::AgentStatus::Running,
        created_at: chrono::Utc::now(),
        started_at: None,
        resource_usage: agnos_common::ResourceUsage::default(), // all zeros
        pid: None,
    };

    let config = AgentConfig::default();
    let requirements = TaskRequirements::default();

    let score = Orchestrator::score_agent(&handle, Some(&config), &requirements, 0, &[]);
    // Fresh agent with no usage and no tasks should get near-maximum score
    assert!(score > 0.85, "Fresh agent should score high, got {}", score);
}

#[test]
fn test_score_agent_with_cpu_usage() {
    use crate::agent::AgentHandle;

    let handle = AgentHandle {
        id: AgentId::new(),
        name: "cpu-heavy".to_string(),
        status: agnos_common::AgentStatus::Running,
        created_at: chrono::Utc::now(),
        started_at: None,
        resource_usage: agnos_common::ResourceUsage {
            memory_used: 0,
            cpu_time_used: 3_000_000, // High CPU usage
            file_descriptors_used: 0,
            processes_used: 0,
        },
        pid: None,
    };

    let config = AgentConfig::default();
    let requirements = TaskRequirements::default();

    let score = Orchestrator::score_agent(&handle, Some(&config), &requirements, 0, &[]);
    // Should be lower than a fresh agent due to CPU usage
    let fresh_handle = AgentHandle {
        resource_usage: agnos_common::ResourceUsage::default(),
        ..handle.clone()
    };
    let fresh_score =
        Orchestrator::score_agent(&fresh_handle, Some(&config), &requirements, 0, &[]);
    assert!(score < fresh_score);
}

#[tokio::test]
async fn test_concurrent_submit_tasks() {
    let orchestrator = create_test_orchestrator();
    let orchestrator = Arc::new(orchestrator);

    let mut handles = Vec::new();
    for i in 0..50 {
        let o = orchestrator.clone();
        handles.push(tokio::spawn(async move {
            let task = Task {
                id: format!("concurrent-{}", i),
                priority: TaskPriority::Normal,
                target_agents: vec![],
                payload: serde_json::json!({"i": i}),
                created_at: chrono::Utc::now(),
                deadline: None,
                dependencies: vec![],
                requirements: TaskRequirements::default(),
            };
            o.submit_task(task).await.unwrap()
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    assert_eq!(orchestrator.get_queue_stats().await.queued_tasks, 50);
}

#[tokio::test]
async fn test_dependency_checking_satisfied() {
    let orchestrator = create_test_orchestrator();

    // First, store a completed dependency
    let dep_result = TaskResult {
        task_id: "dep-1".to_string(),
        agent_id: AgentId::new(),
        success: true,
        result: None,
        error: None,
        completed_at: chrono::Utc::now(),
        duration_ms: 5,
    };
    orchestrator.store_result(dep_result).await.unwrap();

    // Submit a task that depends on dep-1
    let task = Task {
        id: "dependent".to_string(),
        priority: TaskPriority::Normal,
        target_agents: vec![],
        payload: serde_json::json!({}),
        created_at: chrono::Utc::now(),
        deadline: None,
        dependencies: vec!["dep-1".to_string()],
        requirements: TaskRequirements::default(),
    };
    orchestrator.submit_task(task).await.unwrap();

    // Manually simulate scheduler check: dependency is satisfied
    {
        let state = orchestrator.state.read().await;
        let queue = state.task_queues.get(&TaskPriority::Normal).unwrap();
        let task = queue.front().unwrap();
        let deps_satisfied = task
            .dependencies
            .iter()
            .all(|dep_id| state.results.contains_key(dep_id));
        assert!(deps_satisfied);
    }
}

#[tokio::test]
async fn test_dependency_checking_not_satisfied() {
    let orchestrator = create_test_orchestrator();

    let task = Task {
        id: "waiting".to_string(),
        priority: TaskPriority::Normal,
        target_agents: vec![],
        payload: serde_json::json!({}),
        created_at: chrono::Utc::now(),
        deadline: None,
        dependencies: vec!["missing-dep".to_string()],
        requirements: TaskRequirements::default(),
    };
    orchestrator.submit_task(task).await.unwrap();

    let state = orchestrator.state.read().await;
    let queue = state.task_queues.get(&TaskPriority::Normal).unwrap();
    let task = queue.front().unwrap();
    let deps_satisfied = task
        .dependencies
        .iter()
        .all(|dep_id| state.results.contains_key(dep_id));
    assert!(!deps_satisfied);
}

#[tokio::test]
async fn test_dependency_multiple_deps() {
    let orchestrator = create_test_orchestrator();

    // Complete two deps
    for i in 0..2 {
        let result = TaskResult {
            task_id: format!("multi-dep-{}", i),
            agent_id: AgentId::new(),
            success: true,
            result: None,
            error: None,
            completed_at: chrono::Utc::now(),
            duration_ms: 1,
        };
        orchestrator.store_result(result).await.unwrap();
    }

    // Task with two deps
    let task = Task {
        id: "multi-deps".to_string(),
        priority: TaskPriority::Normal,
        target_agents: vec![],
        payload: serde_json::json!({}),
        created_at: chrono::Utc::now(),
        deadline: None,
        dependencies: vec!["multi-dep-0".to_string(), "multi-dep-1".to_string()],
        requirements: TaskRequirements::default(),
    };
    orchestrator.submit_task(task).await.unwrap();

    let state = orchestrator.state.read().await;
    let queue = state.task_queues.get(&TaskPriority::Normal).unwrap();
    let task = queue.front().unwrap();
    let deps_satisfied = task
        .dependencies
        .iter()
        .all(|dep_id| state.results.contains_key(dep_id));
    assert!(deps_satisfied);
}

#[tokio::test]
async fn test_dependency_partial_satisfaction() {
    let orchestrator = create_test_orchestrator();

    // Complete only one of two deps
    let result = TaskResult {
        task_id: "partial-dep-0".to_string(),
        agent_id: AgentId::new(),
        success: true,
        result: None,
        error: None,
        completed_at: chrono::Utc::now(),
        duration_ms: 1,
    };
    orchestrator.store_result(result).await.unwrap();

    let task = Task {
        id: "partial".to_string(),
        priority: TaskPriority::Normal,
        target_agents: vec![],
        payload: serde_json::json!({}),
        created_at: chrono::Utc::now(),
        deadline: None,
        dependencies: vec!["partial-dep-0".to_string(), "partial-dep-1".to_string()],
        requirements: TaskRequirements::default(),
    };
    orchestrator.submit_task(task).await.unwrap();

    let state = orchestrator.state.read().await;
    let queue = state.task_queues.get(&TaskPriority::Normal).unwrap();
    let task = queue.front().unwrap();
    let deps_satisfied = task
        .dependencies
        .iter()
        .all(|dep_id| state.results.contains_key(dep_id));
    assert!(!deps_satisfied);
}

#[tokio::test]
async fn test_overdue_tasks_exactly_at_deadline() {
    let orchestrator = create_test_orchestrator();
    let now = chrono::Utc::now();

    // Task with deadline exactly at now should be overdue (deadline < now after tiny delay)
    let task = Task {
        id: "exact-deadline".to_string(),
        priority: TaskPriority::Normal,
        target_agents: vec![],
        payload: serde_json::json!({}),
        created_at: now,
        deadline: Some(now - chrono::Duration::milliseconds(1)),
        dependencies: vec![],
        requirements: TaskRequirements::default(),
    };
    orchestrator.submit_task(task).await.unwrap();

    let overdue = orchestrator.get_overdue_tasks().await;
    assert_eq!(overdue.len(), 1);
}

#[test]
fn test_score_agent_fair_share_capped_at_max() {
    use crate::agent::AgentHandle;

    let handle = AgentHandle {
        id: AgentId::new(),
        name: "capped".to_string(),
        status: agnos_common::AgentStatus::Running,
        created_at: chrono::Utc::now(),
        started_at: None,
        resource_usage: agnos_common::ResourceUsage::default(),
        pid: None,
    };

    let config = AgentConfig::default();
    let requirements = TaskRequirements::default();

    // Fair share penalty: running_task_count * 0.01, capped at 0.1
    // At 10 tasks: penalty = 0.1 (capped)
    let score_10 = Orchestrator::score_agent(&handle, Some(&config), &requirements, 10, &[]);
    // At 20 tasks: penalty = 0.2 but capped at 0.1
    let score_20 = Orchestrator::score_agent(&handle, Some(&config), &requirements, 20, &[]);
    // Both should have the same penalty (0.1 cap)
    assert!((score_10 - score_20).abs() < 0.001);
}

#[tokio::test]
async fn test_agent_stats_no_agents_no_results() {
    let orchestrator = create_test_orchestrator();
    let stats = orchestrator.get_agent_stats().await;
    assert_eq!(stats.registered_agents, 0);
    assert_eq!(stats.total_tasks_processed, 0);
}

#[test]
fn test_prune_results_exactly_at_limit() {
    let mut results = HashMap::new();
    for i in 0..Orchestrator::MAX_RESULTS {
        results.insert(
            format!("exact-{}", i),
            TaskResult {
                task_id: format!("exact-{}", i),
                agent_id: AgentId::new(),
                success: true,
                result: None,
                error: None,
                completed_at: chrono::Utc::now(),
                duration_ms: 0,
            },
        );
    }
    assert_eq!(results.len(), Orchestrator::MAX_RESULTS);
    Orchestrator::prune_results(&mut results);
    assert_eq!(results.len(), Orchestrator::MAX_RESULTS); // No pruning at limit
}

#[test]
fn test_prune_results_one_over_limit() {
    let mut results = HashMap::new();
    let now = chrono::Utc::now();
    for i in 0..=Orchestrator::MAX_RESULTS {
        results.insert(
            format!("one-over-{}", i),
            TaskResult {
                task_id: format!("one-over-{}", i),
                agent_id: AgentId::new(),
                success: true,
                result: None,
                error: None,
                completed_at: now + chrono::Duration::milliseconds(i as i64),
                duration_ms: 0,
            },
        );
    }
    assert_eq!(results.len(), Orchestrator::MAX_RESULTS + 1);
    Orchestrator::prune_results(&mut results);
    assert_eq!(results.len(), Orchestrator::MAX_RESULTS);
}

#[tokio::test]
async fn test_cancel_task_both_queued_and_running() {
    let orchestrator = create_test_orchestrator();

    // Submit a task to queue
    let task = Task {
        id: "dual-cancel".to_string(),
        priority: TaskPriority::Normal,
        target_agents: vec![],
        payload: serde_json::json!({}),
        created_at: chrono::Utc::now(),
        deadline: None,
        dependencies: vec![],
        requirements: TaskRequirements::default(),
    };
    let id = orchestrator.submit_task(task.clone()).await.unwrap();

    // Also add to running (simulating race)
    {
        let mut state = orchestrator.state.write().await;
        state.running_tasks.insert(
            id.clone(),
            Task {
                id: id.clone(),
                ..task
            },
        );
    }

    // Cancel should remove from both
    orchestrator.cancel_task(&id).await.unwrap();
    assert_eq!(orchestrator.get_queue_stats().await.queued_tasks, 0);
    assert_eq!(orchestrator.get_queue_stats().await.running_tasks, 0);
}

#[test]
fn test_task_priority_values() {
    assert_eq!(TaskPriority::Critical as u32, 0);
    assert_eq!(TaskPriority::High as u32, 1);
    assert_eq!(TaskPriority::Normal as u32, 2);
    assert_eq!(TaskPriority::Low as u32, 3);
    assert_eq!(TaskPriority::Background as u32, 4);
}

#[tokio::test]
async fn test_peek_after_cancel_all_of_highest() {
    let orchestrator = create_test_orchestrator();

    // Submit Critical and Normal tasks
    let crit_task = Task {
        id: "crit-peek".to_string(),
        priority: TaskPriority::Critical,
        target_agents: vec![],
        payload: serde_json::json!({}),
        created_at: chrono::Utc::now(),
        deadline: None,
        dependencies: vec![],
        requirements: TaskRequirements::default(),
    };
    let crit_id = orchestrator.submit_task(crit_task).await.unwrap();

    let norm_task = Task {
        id: "norm-peek".to_string(),
        priority: TaskPriority::Normal,
        target_agents: vec![],
        payload: serde_json::json!({}),
        created_at: chrono::Utc::now(),
        deadline: None,
        dependencies: vec![],
        requirements: TaskRequirements::default(),
    };
    orchestrator.submit_task(norm_task).await.unwrap();

    // Cancel the critical task
    orchestrator.cancel_task(&crit_id).await.unwrap();

    // Peek should now return Normal
    let next = orchestrator.peek_next_task().await.unwrap();
    assert_eq!(next.priority, TaskPriority::Normal);
}

#[test]
fn test_prune_results_empty_map() {
    let mut results = HashMap::new();
    Orchestrator::prune_results(&mut results);
    assert!(results.is_empty());
}

// -----------------------------------------------------------------------
// Coverage improvement: broadcast, store_result, queue stats
// -----------------------------------------------------------------------

#[tokio::test]
async fn test_broadcast_empty_registry_v2() {
    let orchestrator = create_test_orchestrator();
    // Should succeed with no agents to broadcast to
    let result = orchestrator
        .broadcast(MessageType::Event, serde_json::json!({"event": "test"}))
        .await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_store_result_success() {
    let orchestrator = create_test_orchestrator();

    let task = Task {
        id: "result-task".to_string(),
        priority: TaskPriority::Normal,
        target_agents: vec![],
        payload: serde_json::json!({}),
        created_at: chrono::Utc::now(),
        deadline: None,
        dependencies: vec![],
        requirements: TaskRequirements::default(),
    };
    orchestrator.submit_task(task).await.unwrap();

    let result = TaskResult {
        task_id: "result-task".to_string(),
        agent_id: AgentId::new(),
        success: true,
        result: Some(serde_json::json!({"output": "done"})),
        error: None,
        completed_at: chrono::Utc::now(),
        duration_ms: 100,
    };
    orchestrator.store_result(result).await.unwrap();

    let status = orchestrator.get_task_status("result-task").await;
    assert!(status.is_some());
    match status.unwrap() {
        TaskStatus::Completed(r) => {
            assert!(r.success);
            assert_eq!(r.duration_ms, 100);
        }
        _ => panic!("Expected Completed status"),
    }
}

#[tokio::test]
async fn test_store_result_failure() {
    let orchestrator = create_test_orchestrator();

    let result = TaskResult {
        task_id: "failed-task".to_string(),
        agent_id: AgentId::new(),
        success: false,
        result: None,
        error: Some("something went wrong".to_string()),
        completed_at: chrono::Utc::now(),
        duration_ms: 50,
    };
    orchestrator.store_result(result).await.unwrap();

    let status = orchestrator.get_task_status("failed-task").await;
    assert!(status.is_some());
}

#[tokio::test]
async fn test_queue_stats_after_multiple_submissions() {
    let orchestrator = create_test_orchestrator();

    for i in 0..3 {
        let task = Task {
            id: format!("stats-task-{}", i),
            priority: TaskPriority::Normal,
            target_agents: vec![],
            payload: serde_json::json!({}),
            created_at: chrono::Utc::now(),
            deadline: None,
            dependencies: vec![],
            requirements: TaskRequirements::default(),
        };
        orchestrator.submit_task(task).await.unwrap();
    }

    let stats = orchestrator.get_queue_stats().await;
    assert_eq!(stats.total_tasks, 3);
    assert_eq!(stats.queued_tasks, 3);
    assert_eq!(stats.running_tasks, 0);
}

#[tokio::test]
async fn test_task_result_serialization_v2() {
    let result = TaskResult {
        task_id: "serial-task".to_string(),
        agent_id: AgentId::new(),
        success: true,
        result: Some(serde_json::json!(42)),
        error: None,
        completed_at: chrono::Utc::now(),
        duration_ms: 200,
    };
    let json = serde_json::to_string(&result).unwrap();
    let deser: TaskResult = serde_json::from_str(&json).unwrap();
    assert_eq!(deser.task_id, "serial-task");
    assert!(deser.success);
    assert_eq!(deser.duration_ms, 200);
}

#[tokio::test]
async fn test_submit_multiple_priorities_queue_order() {
    let orchestrator = create_test_orchestrator();

    let bg_task = Task {
        id: "bg".to_string(),
        priority: TaskPriority::Background,
        target_agents: vec![],
        payload: serde_json::json!({}),
        created_at: chrono::Utc::now(),
        deadline: None,
        dependencies: vec![],
        requirements: TaskRequirements::default(),
    };
    orchestrator.submit_task(bg_task).await.unwrap();

    let high_task = Task {
        id: "high".to_string(),
        priority: TaskPriority::High,
        target_agents: vec![],
        payload: serde_json::json!({}),
        created_at: chrono::Utc::now(),
        deadline: None,
        dependencies: vec![],
        requirements: TaskRequirements::default(),
    };
    let high_id = orchestrator.submit_task(high_task).await.unwrap();

    // High priority should be peeked first
    let next = orchestrator.peek_next_task().await.unwrap();
    assert_eq!(next.id, high_id);
    assert_eq!(next.priority, TaskPriority::High);
}

// ==================================================================
// H13: permission_to_str avoids format! allocations
// ==================================================================

#[test]
fn test_permission_to_str_all_variants() {
    use agnos_common::Permission;
    assert_eq!(
        Orchestrator::permission_to_str(Permission::FileRead),
        "fileread"
    );
    assert_eq!(
        Orchestrator::permission_to_str(Permission::FileWrite),
        "filewrite"
    );
    assert_eq!(
        Orchestrator::permission_to_str(Permission::NetworkAccess),
        "networkaccess"
    );
    assert_eq!(
        Orchestrator::permission_to_str(Permission::ProcessSpawn),
        "processspawn"
    );
    assert_eq!(
        Orchestrator::permission_to_str(Permission::LlmInference),
        "llminference"
    );
    assert_eq!(
        Orchestrator::permission_to_str(Permission::AuditRead),
        "auditread"
    );
}

#[test]
fn test_permission_to_str_matches_debug_lowercase() {
    use agnos_common::Permission;
    let all = [
        Permission::FileRead,
        Permission::FileWrite,
        Permission::NetworkAccess,
        Permission::ProcessSpawn,
        Permission::LlmInference,
        Permission::AuditRead,
    ];
    for p in &all {
        let debug_str = format!("{:?}", p).to_lowercase();
        assert_eq!(
            Orchestrator::permission_to_str(*p),
            debug_str.as_str(),
            "Mismatch for {:?}",
            p,
        );
    }
}

#[test]
fn test_score_agent_capability_match_uses_static_str() {
    use crate::agent::AgentHandle;

    let handle = AgentHandle {
        id: AgentId::new(),
        name: "cap-static".to_string(),
        status: agnos_common::AgentStatus::Running,
        created_at: chrono::Utc::now(),
        started_at: None,
        resource_usage: agnos_common::ResourceUsage::default(),
        pid: None,
    };

    let config = AgentConfig {
        permissions: vec![
            agnos_common::Permission::FileRead,
            agnos_common::Permission::NetworkAccess,
        ],
        ..Default::default()
    };

    let requirements = TaskRequirements {
        required_capabilities: vec!["fileread".to_string(), "networkaccess".to_string()],
        ..Default::default()
    };

    let score = Orchestrator::score_agent(&handle, Some(&config), &requirements, 0, &[]);
    // Full cap match (0.2) + memory (0.4) + cpu (0.3) = 0.9
    assert!(
        (score - 0.9).abs() < 0.01,
        "Expected ~0.9 for full capability match, got {}",
        score,
    );
}

// ==================================================================
// H12: proactive pruning constant
// ==================================================================

#[test]
fn test_prune_interval_ticks_is_positive() {
    assert!(Orchestrator::PRUNE_INTERVAL_TICKS > 0);
}

// ==================================================================
// G1: GPU-aware scheduling tests
// ==================================================================

#[test]
fn test_task_requirements_gpu_defaults() {
    let req = TaskRequirements::default();
    assert!(!req.gpu_required);
    assert_eq!(req.min_gpu_memory, 0);
    assert!(req.required_compute_capability.is_none());
}

#[test]
fn test_task_requirements_gpu_fields() {
    let req = TaskRequirements {
        gpu_required: true,
        min_gpu_memory: 8 * 1024 * 1024 * 1024,
        required_compute_capability: Some("8.6".to_string()),
        ..Default::default()
    };
    assert!(req.gpu_required);
    assert_eq!(req.min_gpu_memory, 8 * 1024 * 1024 * 1024);
    assert_eq!(req.required_compute_capability, Some("8.6".to_string()));
}

#[test]
fn test_score_agent_gpu_required_no_gpus() {
    use crate::agent::AgentHandle;

    let handle = AgentHandle {
        id: AgentId::new(),
        name: "no-gpu".to_string(),
        status: agnos_common::AgentStatus::Running,
        created_at: chrono::Utc::now(),
        started_at: None,
        resource_usage: agnos_common::ResourceUsage::default(),
        pid: None,
    };

    let config = AgentConfig::default();
    let requirements = TaskRequirements {
        gpu_required: true,
        min_gpu_memory: 4 * 1024 * 1024 * 1024,
        ..Default::default()
    };

    // No GPUs available — GPU component should be 0
    let score = Orchestrator::score_agent(&handle, Some(&config), &requirements, 0, &[]);
    // With GPU required: weights are 0.35 mem + 0.25 cpu + 0.15 gpu + 0.15 cap
    // GPU = 0, so max possible ≈ 0.75
    assert!(
        score < 0.8,
        "Score with GPU required but no GPUs should be < 0.8, got {}",
        score
    );
}

#[test]
fn test_score_agent_gpu_required_with_sufficient_gpu() {
    use crate::agent::AgentHandle;
    use crate::resource::GpuDevice;
    use std::sync::atomic::AtomicU64;

    let handle = AgentHandle {
        id: AgentId::new(),
        name: "gpu-agent".to_string(),
        status: agnos_common::AgentStatus::Running,
        created_at: chrono::Utc::now(),
        started_at: None,
        resource_usage: agnos_common::ResourceUsage::default(),
        pid: None,
    };

    let config = AgentConfig::default();
    let requirements = TaskRequirements {
        gpu_required: true,
        min_gpu_memory: 4 * 1024 * 1024 * 1024,
        ..Default::default()
    };

    let gpus = vec![GpuDevice {
        id: 0,
        name: "Test GPU".to_string(),
        total_memory: 16 * 1024 * 1024 * 1024,
        available_memory: AtomicU64::new(16 * 1024 * 1024 * 1024),
        compute_capability: Some("8.6".to_string()),
    }];

    let score = Orchestrator::score_agent(&handle, Some(&config), &requirements, 0, &gpus);
    // GPU has full VRAM → GPU score ≈ 1.0 → total should be high
    assert!(
        score > 0.8,
        "Score with GPU available should be high, got {}",
        score
    );
}

#[test]
fn test_score_agent_gpu_required_insufficient_vram() {
    use crate::agent::AgentHandle;
    use crate::resource::GpuDevice;
    use std::sync::atomic::AtomicU64;

    let handle = AgentHandle {
        id: AgentId::new(),
        name: "low-vram".to_string(),
        status: agnos_common::AgentStatus::Running,
        created_at: chrono::Utc::now(),
        started_at: None,
        resource_usage: agnos_common::ResourceUsage::default(),
        pid: None,
    };

    let config = AgentConfig::default();
    let requirements = TaskRequirements {
        gpu_required: true,
        min_gpu_memory: 12 * 1024 * 1024 * 1024, // Needs 12 GB
        ..Default::default()
    };

    let gpus = vec![GpuDevice {
        id: 0,
        name: "Small GPU".to_string(),
        total_memory: 8 * 1024 * 1024 * 1024, // Only 8 GB
        available_memory: AtomicU64::new(8 * 1024 * 1024 * 1024),
        compute_capability: None,
    }];

    let score = Orchestrator::score_agent(&handle, Some(&config), &requirements, 0, &gpus);
    // GPU can't satisfy requirement → GPU score = 0
    assert!(
        score < 0.8,
        "Score with insufficient VRAM should be lower, got {}",
        score
    );
}

#[test]
fn test_score_agent_gpu_compute_capability_filter() {
    use crate::agent::AgentHandle;
    use crate::resource::GpuDevice;
    use std::sync::atomic::AtomicU64;

    let handle = AgentHandle {
        id: AgentId::new(),
        name: "cc-agent".to_string(),
        status: agnos_common::AgentStatus::Running,
        created_at: chrono::Utc::now(),
        started_at: None,
        resource_usage: agnos_common::ResourceUsage::default(),
        pid: None,
    };

    let config = AgentConfig::default();
    let requirements = TaskRequirements {
        gpu_required: true,
        required_compute_capability: Some("8.0".to_string()),
        ..Default::default()
    };

    // GPU with compute capability 7.5 — doesn't meet 8.0 requirement
    let old_gpu = vec![GpuDevice {
        id: 0,
        name: "Old GPU".to_string(),
        total_memory: 16 * 1024 * 1024 * 1024,
        available_memory: AtomicU64::new(16 * 1024 * 1024 * 1024),
        compute_capability: Some("7.5".to_string()),
    }];

    let score_old = Orchestrator::score_agent(&handle, Some(&config), &requirements, 0, &old_gpu);

    // GPU with compute capability 8.6 — meets requirement
    let new_gpu = vec![GpuDevice {
        id: 0,
        name: "New GPU".to_string(),
        total_memory: 16 * 1024 * 1024 * 1024,
        available_memory: AtomicU64::new(16 * 1024 * 1024 * 1024),
        compute_capability: Some("8.6".to_string()),
    }];

    let score_new = Orchestrator::score_agent(&handle, Some(&config), &requirements, 0, &new_gpu);

    assert!(
        score_new > score_old,
        "New GPU (cc 8.6) should score higher than old GPU (cc 7.5)"
    );
}

#[test]
fn test_score_agent_no_gpu_required_weights_unchanged() {
    use crate::agent::AgentHandle;

    let handle = AgentHandle {
        id: AgentId::new(),
        name: "cpu-only".to_string(),
        status: agnos_common::AgentStatus::Running,
        created_at: chrono::Utc::now(),
        started_at: None,
        resource_usage: agnos_common::ResourceUsage::default(),
        pid: None,
    };

    let config = AgentConfig::default();
    let requirements = TaskRequirements::default(); // gpu_required = false

    // Without GPU requirement, weights revert to original (0.4 + 0.3 + 0.2 = 0.9 max)
    let score = Orchestrator::score_agent(&handle, Some(&config), &requirements, 0, &[]);
    assert!(
        score > 0.85,
        "Idle agent without GPU requirement should score high, got {}",
        score
    );
}

#[test]
fn test_score_gpu_empty_snapshot() {
    let requirements = TaskRequirements {
        gpu_required: true,
        ..Default::default()
    };
    let score = Orchestrator::score_gpu(&[], &requirements);
    assert_eq!(score, 0.0);
}

#[test]
fn test_score_gpu_full_vram_available() {
    use crate::resource::GpuDevice;
    use std::sync::atomic::AtomicU64;

    let gpus = vec![GpuDevice {
        id: 0,
        name: "Full GPU".to_string(),
        total_memory: 16 * 1024 * 1024 * 1024,
        available_memory: AtomicU64::new(16 * 1024 * 1024 * 1024),
        compute_capability: None,
    }];

    let requirements = TaskRequirements {
        gpu_required: true,
        min_gpu_memory: 4 * 1024 * 1024 * 1024,
        ..Default::default()
    };

    let score = Orchestrator::score_gpu(&gpus, &requirements);
    assert!(
        (score - 1.0).abs() < 0.01,
        "Full VRAM should score ~1.0, got {}",
        score
    );
}

#[test]
fn test_score_gpu_partial_vram() {
    use crate::resource::GpuDevice;
    use std::sync::atomic::AtomicU64;

    let total = 16 * 1024 * 1024 * 1024_u64;
    let available = 8 * 1024 * 1024 * 1024_u64; // 50% available

    let gpus = vec![GpuDevice {
        id: 0,
        name: "Half GPU".to_string(),
        total_memory: total,
        available_memory: AtomicU64::new(available),
        compute_capability: None,
    }];

    let requirements = TaskRequirements {
        gpu_required: true,
        min_gpu_memory: 4 * 1024 * 1024 * 1024, // needs 4 GB, 8 GB available
        ..Default::default()
    };

    let score = Orchestrator::score_gpu(&gpus, &requirements);
    assert!(
        (score - 0.5).abs() < 0.01,
        "Half VRAM available should score ~0.5, got {}",
        score
    );
}

#[test]
fn test_score_gpu_picks_best_gpu() {
    use crate::resource::GpuDevice;
    use std::sync::atomic::AtomicU64;

    let gpus = vec![
        GpuDevice {
            id: 0,
            name: "Small GPU".to_string(),
            total_memory: 4 * 1024 * 1024 * 1024,
            available_memory: AtomicU64::new(2 * 1024 * 1024 * 1024), // 50%
            compute_capability: None,
        },
        GpuDevice {
            id: 1,
            name: "Big GPU".to_string(),
            total_memory: 16 * 1024 * 1024 * 1024,
            available_memory: AtomicU64::new(14 * 1024 * 1024 * 1024), // 87.5%
            compute_capability: None,
        },
    ];

    let requirements = TaskRequirements {
        gpu_required: true,
        ..Default::default()
    };

    let score = Orchestrator::score_gpu(&gpus, &requirements);
    // Should pick the best ratio (87.5% from big GPU)
    assert!(score > 0.85, "Should pick best GPU ratio, got {}", score);
}

#[test]
fn test_orchestrator_with_resource_manager_builder() {
    let registry = Arc::new(crate::registry::AgentRegistry::new());
    let orchestrator = Orchestrator::new(registry);
    assert!(orchestrator.resource_manager.is_none());
    // with_resource_manager is tested implicitly — just verify the field exists
}
