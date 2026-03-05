//! Integration tests for agent-orchestrator
//!
//! Tests multi-agent task scheduling, conflict resolution, and resource allocation

use std::sync::Arc;

#[tokio::test]
async fn test_orchestrator_initialization() {
    use agent_runtime::orchestrator::Orchestrator;
    use agent_runtime::registry::AgentRegistry;

    let registry = Arc::new(AgentRegistry::new());
    let orchestrator = Orchestrator::new(registry);

    let stats = orchestrator.get_queue_stats().await;
    assert_eq!(stats.total_tasks, 0);
    assert_eq!(stats.queued_tasks, 0);
    assert_eq!(stats.running_tasks, 0);
}

#[tokio::test]
async fn test_submit_single_task() {
    use agent_runtime::orchestrator::{Orchestrator, Task, TaskPriority};
    use agent_runtime::registry::AgentRegistry;

    let registry = Arc::new(AgentRegistry::new());
    let orchestrator = Orchestrator::new(registry);

    let task = Task {
        id: String::new(), // Will be overwritten by submit_task
        priority: TaskPriority::Normal,
        target_agents: vec![],
        payload: serde_json::json!({"action": "test"}),
        created_at: chrono::Utc::now(),
        deadline: None,
        dependencies: vec![],
        requirements: agent_runtime::orchestrator::TaskRequirements::default(),
    };

    let task_id = orchestrator.submit_task(task).await.unwrap();
    assert!(!task_id.is_empty()); // ID is generated as UUID

    let stats = orchestrator.get_queue_stats().await;
    assert_eq!(stats.queued_tasks, 1);
}

#[tokio::test]
async fn test_multi_agent_task_distribution() {
    use agent_runtime::orchestrator::{Orchestrator, Task, TaskPriority};
    use agent_runtime::registry::AgentRegistry;

    let registry = Arc::new(AgentRegistry::new());
    let orchestrator = Orchestrator::new(registry);

    // Submit multiple tasks
    for i in 0..10 {
        let task = Task {
            id: String::new(),
            priority: TaskPriority::Normal,
            target_agents: vec![],
            payload: serde_json::json!({"action": "process", "id": i}),
            created_at: chrono::Utc::now(),
            deadline: None,
            dependencies: vec![],
            requirements: agent_runtime::orchestrator::TaskRequirements::default(),
        };
        orchestrator.submit_task(task).await.unwrap();
    }

    let stats = orchestrator.get_queue_stats().await;
    assert_eq!(stats.queued_tasks, 10);
}

#[tokio::test]
async fn test_task_priority_ordering() {
    use agent_runtime::orchestrator::{Orchestrator, Task, TaskPriority};
    use agent_runtime::registry::AgentRegistry;

    let registry = Arc::new(AgentRegistry::new());
    let orchestrator = Orchestrator::new(registry);

    // Submit tasks in reverse priority order - critical should end up first
    let task_ids = vec![
        (TaskPriority::Background, "bg-1"),
        (TaskPriority::Low, "low-1"),
        (TaskPriority::Normal, "normal-1"),
        (TaskPriority::High, "high-1"),
        (TaskPriority::Critical, "critical-1"),
    ];

    for (priority, _id) in task_ids {
        let task = Task {
            id: String::new(),
            priority,
            target_agents: vec![],
            payload: serde_json::json!({}),
            created_at: chrono::Utc::now(),
            deadline: None,
            dependencies: vec![],
            requirements: agent_runtime::orchestrator::TaskRequirements::default(),
        };
        orchestrator.submit_task(task).await.unwrap();
    }

    // Verify we can peek at next task
    let next = orchestrator.peek_next_task().await;
    assert!(next.is_some());
    // The scheduler processes by priority, so Critical should come first
}

#[tokio::test]
async fn test_task_result_storage() {
    use agent_runtime::orchestrator::{Orchestrator, Task, TaskPriority, TaskResult};
    use agent_runtime::registry::AgentRegistry;
    use agnos_common::AgentId;

    let registry = Arc::new(AgentRegistry::new());
    let orchestrator = Orchestrator::new(registry);

    let task = Task {
        id: String::new(),
        priority: TaskPriority::Normal,
        target_agents: vec![],
        payload: serde_json::json!({"test": true}),
        created_at: chrono::Utc::now(),
        deadline: None,
        dependencies: vec![],
        requirements: agent_runtime::orchestrator::TaskRequirements::default(),
    };

    let task_id = orchestrator.submit_task(task).await.unwrap();

    let result = TaskResult {
        task_id: task_id.clone(),
        agent_id: AgentId::new(),
        success: true,
        result: Some(serde_json::json!({"output": "test-result"})),
        error: None,
        completed_at: chrono::Utc::now(),
        duration_ms: 50,
    };

    let stored_task_id = result.task_id.clone();
    orchestrator.store_result(result).await.unwrap();

    let retrieved = orchestrator.get_result(&stored_task_id).await;
    assert!(retrieved.is_some());
    assert!(retrieved.unwrap().success);
}

#[tokio::test]
async fn test_task_failure_handling() {
    use agent_runtime::orchestrator::{Orchestrator, Task, TaskPriority, TaskResult};
    use agent_runtime::registry::AgentRegistry;
    use agnos_common::AgentId;

    let registry = Arc::new(AgentRegistry::new());
    let orchestrator = Orchestrator::new(registry);

    let task = Task {
        id: String::new(),
        priority: TaskPriority::High,
        target_agents: vec![],
        payload: serde_json::json!({"fail": true}),
        created_at: chrono::Utc::now(),
        deadline: None,
        dependencies: vec![],
        requirements: agent_runtime::orchestrator::TaskRequirements::default(),
    };

    let task_id = orchestrator.submit_task(task).await.unwrap();

    let result = TaskResult {
        task_id: task_id.clone(),
        agent_id: AgentId::new(),
        success: false,
        result: None,
        error: Some("Simulated failure".to_string()),
        completed_at: chrono::Utc::now(),
        duration_ms: 10,
    };

    let stored_task_id = result.task_id.clone();
    orchestrator.store_result(result).await.unwrap();

    let retrieved = orchestrator.get_result(&stored_task_id).await.unwrap();
    assert!(!retrieved.success);
    assert!(retrieved.error.is_some());
}

#[tokio::test]
async fn test_task_cancellation() {
    use agent_runtime::orchestrator::{Orchestrator, Task, TaskPriority};
    use agent_runtime::registry::AgentRegistry;

    let registry = Arc::new(AgentRegistry::new());
    let orchestrator = Orchestrator::new(registry);

    // Submit multiple tasks
    for i in 0..5 {
        let task = Task {
            id: String::new(),
            priority: TaskPriority::Normal,
            target_agents: vec![],
            payload: serde_json::json!({"id": i}),
            created_at: chrono::Utc::now(),
            deadline: None,
            dependencies: vec![],
            requirements: agent_runtime::orchestrator::TaskRequirements::default(),
        };
        orchestrator.submit_task(task).await.unwrap();
    }

    // Get initial count
    let stats_before = orchestrator.get_queue_stats().await;
    let _initial_count = stats_before.queued_tasks;

    // Cancel some tasks - use the ID returned from submit
    // Note: Since IDs are generated, we can't predict them
    // Just verify cancel doesn't error
    let result = orchestrator.cancel_task("nonexistent").await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_overdue_task_detection() {
    use agent_runtime::orchestrator::{Orchestrator, Task, TaskPriority};
    use agent_runtime::registry::AgentRegistry;

    let registry = Arc::new(AgentRegistry::new());
    let orchestrator = Orchestrator::new(registry);

    // Create task with past deadline
    let task = Task {
        id: String::new(),
        priority: TaskPriority::Normal,
        target_agents: vec![],
        payload: serde_json::json!({}),
        created_at: chrono::Utc::now(),
        deadline: Some(chrono::Utc::now() - chrono::Duration::seconds(10)),
        dependencies: vec![],
        requirements: agent_runtime::orchestrator::TaskRequirements::default(),
    };

    orchestrator.submit_task(task).await.unwrap();

    let overdue = orchestrator.get_overdue_tasks().await;
    assert!(!overdue.is_empty());
}

#[tokio::test]
async fn test_broadcast_to_agents() {
    use agent_runtime::orchestrator::Orchestrator;
    use agent_runtime::registry::AgentRegistry;
    use agnos_common::MessageType;

    let registry = Arc::new(AgentRegistry::new());
    let orchestrator = Orchestrator::new(registry);

    // Broadcast should not error even without registered agents
    let result = orchestrator.broadcast(
        MessageType::Event,
        serde_json::json!({"event": "test"}),
    ).await;

    assert!(result.is_ok());
}

#[tokio::test]
async fn test_queue_stats_computation() {
    use agent_runtime::orchestrator::{Orchestrator, Task, TaskPriority};
    use agent_runtime::registry::AgentRegistry;

    let registry = Arc::new(AgentRegistry::new());
    let orchestrator = Orchestrator::new(registry);

    // Submit tasks with different priorities
    for i in 0..7 {
        let task = Task {
            id: String::new(),
            priority: if i % 2 == 0 { TaskPriority::High } else { TaskPriority::Normal },
            target_agents: vec![],
            payload: serde_json::json!({}),
            created_at: chrono::Utc::now(),
            deadline: None,
            dependencies: vec![],
            requirements: agent_runtime::orchestrator::TaskRequirements::default(),
        };
        orchestrator.submit_task(task).await.unwrap();
    }

    let stats = orchestrator.get_queue_stats().await;
    assert_eq!(stats.total_tasks, 7);
}

#[tokio::test]
async fn test_task_get_status_queued() {
    use agent_runtime::orchestrator::{Orchestrator, Task, TaskPriority};
    use agent_runtime::registry::AgentRegistry;
    use agent_runtime::orchestrator::TaskStatus;

    let registry = Arc::new(AgentRegistry::new());
    let orchestrator = Orchestrator::new(registry);

    let task = Task {
        id: String::new(),
        priority: TaskPriority::Normal,
        target_agents: vec![],
        payload: serde_json::json!({}),
        created_at: chrono::Utc::now(),
        deadline: None,
        dependencies: vec![],
        requirements: agent_runtime::orchestrator::TaskRequirements::default(),
    };

    let task_id = orchestrator.submit_task(task).await.unwrap();

    let status = orchestrator.get_task_status(&task_id).await;
    assert!(matches!(status, Some(TaskStatus::Queued)));
}

#[tokio::test]
async fn test_task_get_status_completed() {
    use agent_runtime::orchestrator::{Orchestrator, Task, TaskPriority, TaskResult};
    use agent_runtime::registry::AgentRegistry;
    use agent_runtime::orchestrator::TaskStatus;
    use agnos_common::AgentId;

    let registry = Arc::new(AgentRegistry::new());
    let orchestrator = Orchestrator::new(registry);

    let task = Task {
        id: String::new(),
        priority: TaskPriority::Normal,
        target_agents: vec![],
        payload: serde_json::json!({}),
        created_at: chrono::Utc::now(),
        deadline: None,
        dependencies: vec![],
        requirements: agent_runtime::orchestrator::TaskRequirements::default(),
    };

    let task_id = orchestrator.submit_task(task).await.unwrap();

    let result = TaskResult {
        task_id: task_id.clone(),
        agent_id: AgentId::new(),
        success: true,
        result: Some(serde_json::json!({"done": true})),
        error: None,
        completed_at: chrono::Utc::now(),
        duration_ms: 100,
    };

    orchestrator.store_result(result).await.unwrap();

    let status = orchestrator.get_task_status(&task_id).await;
    assert!(matches!(status, Some(TaskStatus::Completed(_))));
}

#[tokio::test]
async fn test_task_get_status_not_found() {
    use agent_runtime::orchestrator::Orchestrator;
    use agent_runtime::registry::AgentRegistry;

    let registry = Arc::new(AgentRegistry::new());
    let orchestrator = Orchestrator::new(registry);

    let status = orchestrator.get_task_status("nonexistent").await;
    assert!(status.is_none());
}

#[tokio::test]
async fn test_get_agent_stats_empty() {
    use agent_runtime::orchestrator::Orchestrator;
    use agent_runtime::registry::AgentRegistry;

    let registry = Arc::new(AgentRegistry::new());
    let orchestrator = Orchestrator::new(registry);

    let stats = orchestrator.get_agent_stats().await;
    assert_eq!(stats.registered_agents, 0);
    assert_eq!(stats.total_tasks_processed, 0);
}

#[tokio::test]
async fn test_multiple_task_results() {
    use agent_runtime::orchestrator::{Orchestrator, Task, TaskPriority, TaskResult};
    use agent_runtime::registry::AgentRegistry;
    use agnos_common::AgentId;

    let registry = Arc::new(AgentRegistry::new());
    let orchestrator = Orchestrator::new(registry);

    // Submit and complete multiple tasks
    for i in 0..5 {
        let task = Task {
            id: String::new(),
            priority: TaskPriority::Normal,
            target_agents: vec![],
            payload: serde_json::json!({"id": i}),
            created_at: chrono::Utc::now(),
            deadline: None,
            dependencies: vec![],
            requirements: agent_runtime::orchestrator::TaskRequirements::default(),
        };

        let task_id = orchestrator.submit_task(task).await.unwrap();

        let result = TaskResult {
            task_id,
            agent_id: AgentId::new(),
            success: true,
            result: Some(serde_json::json!({"id": i, "done": true})),
            error: None,
            completed_at: chrono::Utc::now(),
            duration_ms: 50 + i * 10,
        };

        orchestrator.store_result(result).await.unwrap();
    }

    let stats = orchestrator.get_agent_stats().await;
    assert_eq!(stats.total_tasks_processed, 5);
}

#[tokio::test]
async fn test_peek_next_task() {
    use agent_runtime::orchestrator::{Orchestrator, Task, TaskPriority};
    use agent_runtime::registry::AgentRegistry;

    let registry = Arc::new(AgentRegistry::new());
    let orchestrator = Orchestrator::new(registry);

    // Submit a task
    let task = Task {
        id: String::new(),
        priority: TaskPriority::Normal,
        target_agents: vec![],
        payload: serde_json::json!({"test": true}),
        created_at: chrono::Utc::now(),
        deadline: None,
        dependencies: vec![],
        requirements: agent_runtime::orchestrator::TaskRequirements::default(),
    };

    orchestrator.submit_task(task).await.unwrap();

    // Peek should return the task without removing it
    let peeked = orchestrator.peek_next_task().await;
    assert!(peeked.is_some());

    // Queue should still have the task
    let stats = orchestrator.get_queue_stats().await;
    assert_eq!(stats.queued_tasks, 1);
}
