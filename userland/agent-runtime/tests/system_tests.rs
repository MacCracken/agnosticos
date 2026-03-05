//! End-to-end system tests for the agent-runtime.
//!
//! These tests exercise the full stack: HTTP API, orchestrator, registry,
//! and supervisor interactions working together as an integrated system.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use chrono::Utc;
use serde_json::Value;
use tower::ServiceExt;
use uuid::Uuid;

use agnos_common::AgentId;
use agent_runtime::http_api::{ApiState, build_router};
use agent_runtime::orchestrator::{
    Orchestrator, Task, TaskPriority, TaskRequirements, TaskResult, TaskStatus,
};
use agent_runtime::registry::AgentRegistry;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn fresh_app() -> (ApiState, Router) {
    let state = ApiState::new();
    let router = build_router(state.clone());
    (state, router)
}

async fn register_agent(app: &Router, name: &str) -> (StatusCode, Value) {
    let body = serde_json::json!({
        "name": name,
        "capabilities": ["test"],
    });
    let req = Request::builder()
        .method("POST")
        .uri("/v1/agents/register")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap();
    (status, json)
}

async fn heartbeat(
    app: &Router,
    agent_id: &str,
    payload: Value,
) -> (StatusCode, Value) {
    let req = Request::builder()
        .method("POST")
        .uri(format!("/v1/agents/{}/heartbeat", agent_id))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&payload).unwrap()))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap();
    (status, json)
}

async fn get_agent(app: &Router, agent_id: &str) -> (StatusCode, Value) {
    let req = Request::builder()
        .uri(format!("/v1/agents/{}", agent_id))
        .body(Body::empty())
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap();
    (status, json)
}

async fn list_agents(app: &Router) -> (StatusCode, Value) {
    let req = Request::builder()
        .uri("/v1/agents")
        .body(Body::empty())
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap();
    (status, json)
}

async fn deregister_agent(app: &Router, agent_id: &str) -> (StatusCode, Value) {
    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/v1/agents/{}", agent_id))
        .body(Body::empty())
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap();
    (status, json)
}

async fn get_health(app: &Router) -> (StatusCode, Value) {
    let req = Request::builder()
        .uri("/v1/health")
        .body(Body::empty())
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap();
    (status, json)
}

async fn get_metrics(app: &Router) -> (StatusCode, Value) {
    let req = Request::builder()
        .uri("/v1/metrics")
        .body(Body::empty())
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap();
    (status, json)
}

fn make_task(priority: TaskPriority) -> Task {
    Task {
        id: String::new(), // will be set by submit_task
        priority,
        target_agents: vec![],
        payload: serde_json::json!({"action": "test"}),
        created_at: Utc::now(),
        deadline: None,
        dependencies: vec![],
        requirements: TaskRequirements::default(),
    }
}

// ---------------------------------------------------------------------------
// 1. Full agent lifecycle via HTTP
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_full_agent_lifecycle() {
    let (_state, app) = fresh_app();

    // Register
    let (status, reg) = register_agent(&app, "lifecycle-agent").await;
    assert_eq!(status, StatusCode::CREATED);
    let agent_id = reg["id"].as_str().unwrap().to_string();
    assert_eq!(reg["name"], "lifecycle-agent");
    assert_eq!(reg["status"], "registered");

    // Heartbeat
    let (status, _) = heartbeat(
        &app,
        &agent_id,
        serde_json::json!({"status": "running", "cpu_percent": 10.0, "memory_mb": 128}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Get agent — verify heartbeat data is reflected
    let (status, detail) = get_agent(&app, &agent_id).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(detail["status"], "running");
    assert_eq!(detail["name"], "lifecycle-agent");
    assert!(detail["last_heartbeat"].as_str().is_some());

    // List agents — should include our agent
    let (status, list) = list_agents(&app).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list["total"], 1);
    assert_eq!(list["agents"][0]["name"], "lifecycle-agent");

    // Deregister
    let (status, _) = deregister_agent(&app, &agent_id).await;
    assert_eq!(status, StatusCode::OK);

    // Verify gone
    let (status, _) = get_agent(&app, &agent_id).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ---------------------------------------------------------------------------
// 2. Multi-agent registration
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_multi_agent_registration() {
    let (_state, app) = fresh_app();

    for i in 0..10 {
        let (status, _) = register_agent(&app, &format!("agent-{}", i)).await;
        assert_eq!(status, StatusCode::CREATED, "agent-{} registration failed", i);
    }

    // Verify list returns all 10
    let (status, list) = list_agents(&app).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list["total"], 10);
    let agents = list["agents"].as_array().unwrap();
    assert_eq!(agents.len(), 10);

    // Verify metrics counts
    let (status, metrics) = get_metrics(&app).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(metrics["total_agents"], 10);
}

// ---------------------------------------------------------------------------
// 3. Agent heartbeat updates
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_heartbeat_updates_reflected() {
    let (_state, app) = fresh_app();

    let (_, reg) = register_agent(&app, "hb-test-agent").await;
    let agent_id = reg["id"].as_str().unwrap().to_string();

    // Send heartbeat with CPU/memory data
    let (status, _) = heartbeat(
        &app,
        &agent_id,
        serde_json::json!({
            "status": "busy",
            "current_task": "processing-data",
            "cpu_percent": 75.5,
            "memory_mb": 1024
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Verify the updates
    let (status, detail) = get_agent(&app, &agent_id).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(detail["status"], "busy");
    assert_eq!(detail["current_task"], "processing-data");
    assert_eq!(detail["cpu_percent"], 75.5);
    assert_eq!(detail["memory_mb"], 1024);
    assert!(detail["last_heartbeat"].as_str().is_some());
}

// ---------------------------------------------------------------------------
// 4. Duplicate agent rejection
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_duplicate_agent_rejection() {
    let (_state, app) = fresh_app();

    let (status, _) = register_agent(&app, "unique-name").await;
    assert_eq!(status, StatusCode::CREATED);

    // Same name again
    let (status, body) = register_agent(&app, "unique-name").await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(body["error"].as_str().unwrap().contains("already registered"));
}

// ---------------------------------------------------------------------------
// 5. Heartbeat to nonexistent agent
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_heartbeat_nonexistent_agent() {
    let (_state, app) = fresh_app();
    let fake_id = Uuid::new_v4().to_string();

    let (status, body) = heartbeat(&app, &fake_id, serde_json::json!({})).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body["error"].as_str().unwrap().contains("not found"));
}

// ---------------------------------------------------------------------------
// 6. Deregister then heartbeat
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_deregister_then_heartbeat() {
    let (_state, app) = fresh_app();

    let (_, reg) = register_agent(&app, "soon-gone").await;
    let agent_id = reg["id"].as_str().unwrap().to_string();

    // Deregister
    let (status, _) = deregister_agent(&app, &agent_id).await;
    assert_eq!(status, StatusCode::OK);

    // Heartbeat should now 404
    let (status, _) = heartbeat(&app, &agent_id, serde_json::json!({})).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ---------------------------------------------------------------------------
// 7. Concurrent agent registrations
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_concurrent_agent_registrations() {
    let (_state, app) = fresh_app();

    let mut handles = Vec::new();
    for i in 0..50 {
        let app_clone = app.clone();
        handles.push(tokio::spawn(async move {
            register_agent(&app_clone, &format!("concurrent-{}", i)).await
        }));
    }

    let mut success_count = 0;
    for handle in handles {
        let (status, _) = handle.await.unwrap();
        if status == StatusCode::CREATED {
            success_count += 1;
        }
    }
    assert_eq!(success_count, 50, "All 50 concurrent registrations should succeed");

    let (_, list) = list_agents(&app).await;
    assert_eq!(list["total"], 50);
}

// ---------------------------------------------------------------------------
// 8. Health endpoint under load
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_health_endpoint_under_load() {
    let (_state, app) = fresh_app();

    let mut handles = Vec::new();
    for _ in 0..100 {
        let app_clone = app.clone();
        handles.push(tokio::spawn(async move {
            get_health(&app_clone).await
        }));
    }

    for handle in handles {
        let (status, body) = handle.await.unwrap();
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "ok");
    }
}

// ---------------------------------------------------------------------------
// 9. Orchestrator + HTTP integration
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_orchestrator_http_integration() {
    let (_state, app) = fresh_app();

    // Register agents via HTTP
    let mut agent_ids = Vec::new();
    for i in 0..3 {
        let (status, reg) = register_agent(&app, &format!("orch-agent-{}", i)).await;
        assert_eq!(status, StatusCode::CREATED);
        agent_ids.push(reg["id"].as_str().unwrap().to_string());
    }

    // Create an orchestrator with its own registry
    let registry = Arc::new(AgentRegistry::new());
    let orchestrator = Orchestrator::new(registry);

    // Submit tasks to orchestrator
    let task = make_task(TaskPriority::Normal);
    let _task_id = orchestrator.submit_task(task).await.unwrap();

    // Verify stats
    let stats = orchestrator.get_queue_stats().await;
    assert_eq!(stats.queued_tasks, 1);
    assert_eq!(stats.total_tasks, 1);

    // Verify HTTP side still working independently
    let (status, list) = list_agents(&app).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list["total"], 3);
}

// ---------------------------------------------------------------------------
// 10. Task lifecycle with orchestrator
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_task_lifecycle() {
    let registry = Arc::new(AgentRegistry::new());
    let orchestrator = Orchestrator::new(registry);

    // Submit a task
    let task = make_task(TaskPriority::Normal);
    let task_id = orchestrator.submit_task(task).await.unwrap();

    // Check status — should be Queued
    let status = orchestrator.get_task_status(&task_id).await;
    assert!(matches!(status, Some(TaskStatus::Queued)));

    // Store a result
    let result = TaskResult {
        task_id: task_id.clone(),
        agent_id: AgentId::new(),
        success: true,
        result: Some(serde_json::json!({"output": "done"})),
        error: None,
        completed_at: Utc::now(),
        duration_ms: 150,
    };
    orchestrator.store_result(result).await.unwrap();

    // Check status — should be Completed
    let status = orchestrator.get_task_status(&task_id).await;
    assert!(matches!(status, Some(TaskStatus::Completed(_))));

    // Retrieve the result
    let result = orchestrator.get_result(&task_id).await;
    assert!(result.is_some());
    let r = result.unwrap();
    assert!(r.success);
    assert_eq!(r.duration_ms, 150);
}

// ---------------------------------------------------------------------------
// 11. Priority scheduling with multiple tasks
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_priority_scheduling() {
    let registry = Arc::new(AgentRegistry::new());
    let orchestrator = Orchestrator::new(registry);

    // Submit tasks in non-priority order
    let _bg_id = orchestrator
        .submit_task(make_task(TaskPriority::Background))
        .await
        .unwrap();
    let _normal_id = orchestrator
        .submit_task(make_task(TaskPriority::Normal))
        .await
        .unwrap();
    let critical_id = orchestrator
        .submit_task(make_task(TaskPriority::Critical))
        .await
        .unwrap();

    // Peek should return the critical task
    let next = orchestrator.peek_next_task().await;
    assert!(next.is_some());
    assert_eq!(next.unwrap().id, critical_id);

    // Stats should show 3 queued
    let stats = orchestrator.get_queue_stats().await;
    assert_eq!(stats.queued_tasks, 3);
    assert_eq!(stats.total_tasks, 3);
}

// ---------------------------------------------------------------------------
// 12. Task with deadline — overdue detection
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_task_overdue_detection() {
    let registry = Arc::new(AgentRegistry::new());
    let orchestrator = Orchestrator::new(registry);

    // Submit a task with a deadline in the past
    let mut task = make_task(TaskPriority::Normal);
    task.deadline = Some(Utc::now() - chrono::Duration::hours(1));
    let task_id = orchestrator.submit_task(task).await.unwrap();

    // Submit a task with no deadline
    let _no_deadline_id = orchestrator
        .submit_task(make_task(TaskPriority::Low))
        .await
        .unwrap();

    // Overdue should return exactly 1 task
    let overdue = orchestrator.get_overdue_tasks().await;
    assert_eq!(overdue.len(), 1);
    assert_eq!(overdue[0].id, task_id);
}

// ---------------------------------------------------------------------------
// 13. Metrics after operations
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_metrics_after_operations() {
    let (_state, app) = fresh_app();

    // Register 3 agents with heartbeats reporting different CPU/memory
    let cpu_values: [f32; 3] = [20.0, 40.0, 60.0];
    let mem_values: [u64; 3] = [256, 512, 768];

    for i in 0..3 {
        let (_, reg) = register_agent(&app, &format!("metrics-agent-{}", i)).await;
        let agent_id = reg["id"].as_str().unwrap().to_string();

        let (status, _) = heartbeat(
            &app,
            &agent_id,
            serde_json::json!({
                "status": "running",
                "cpu_percent": cpu_values[i],
                "memory_mb": mem_values[i]
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }

    // Verify metrics aggregation
    let (status, metrics) = get_metrics(&app).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(metrics["total_agents"], 3);

    // Average CPU should be (20 + 40 + 60) / 3 = 40.0
    let avg_cpu = metrics["avg_cpu_percent"].as_f64().unwrap();
    assert!((avg_cpu - 40.0).abs() < 0.01, "avg CPU should be ~40.0, got {}", avg_cpu);

    // Total memory should be 256 + 512 + 768 = 1536
    assert_eq!(metrics["total_memory_mb"], 1536);

    // All agents should be "running"
    let by_status = metrics["agents_by_status"].as_object().unwrap();
    assert_eq!(by_status["running"], 3);
}

// ---------------------------------------------------------------------------
// 14. Empty agent name validation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_empty_agent_name_validation() {
    let (_state, app) = fresh_app();

    let body = serde_json::json!({"name": ""});
    let req = Request::builder()
        .method("POST")
        .uri("/v1/agents/register")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap();
    assert!(json["error"].as_str().unwrap().contains("required"));
}

// ---------------------------------------------------------------------------
// 15. Long agent name validation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_long_agent_name_validation() {
    let (_state, app) = fresh_app();

    let long_name = "x".repeat(257);
    let body = serde_json::json!({"name": long_name});
    let req = Request::builder()
        .method("POST")
        .uri("/v1/agents/register")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap();
    assert!(json["error"].as_str().unwrap().contains("too long"));
}
