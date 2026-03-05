//! Load / stress tests for the agent-runtime.
//!
//! These are **correctness** tests under heavy concurrent load, NOT benchmarks.
//! Every test spawns many concurrent tokio tasks and asserts that invariants hold.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use chrono::Utc;
use serde_json::json;
use tower::ServiceExt; // for `oneshot`

use agent_runtime::http_api::{
    AgentListResponse, AgentMetricsResponse, ApiState, build_router,
};
use agent_runtime::orchestrator::{Orchestrator, Task, TaskPriority, TaskRequirements, TaskResult};
use agent_runtime::registry::AgentRegistry;
use agnos_common::AgentId;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_task(priority: TaskPriority) -> Task {
    Task {
        id: String::new(), // overwritten by submit_task
        priority,
        target_agents: vec![],
        payload: json!({"action": "load-test"}),
        created_at: Utc::now(),
        deadline: None,
        dependencies: vec![],
        requirements: TaskRequirements::default(),
    }
}

fn make_task_with_deadline(deadline: chrono::DateTime<Utc>) -> Task {
    Task {
        id: String::new(),
        priority: TaskPriority::Normal,
        target_agents: vec![],
        payload: json!({"action": "deadline-test"}),
        created_at: Utc::now(),
        deadline: Some(deadline),
        dependencies: vec![],
        requirements: TaskRequirements::default(),
    }
}

fn shared_orchestrator() -> Arc<Orchestrator> {
    let registry = Arc::new(AgentRegistry::new());
    Arc::new(Orchestrator::new(registry))
}

/// Register a uniquely-named agent via HTTP POST, return the response body as JSON.
async fn register_via_http(
    app: &axum::Router,
    name: &str,
) -> (StatusCode, serde_json::Value) {
    let body = json!({
        "name": name,
        "capabilities": ["load-test"],
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
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or(json!({}));
    (status, json)
}

// ---------------------------------------------------------------------------
// 1. 100 concurrent agent registrations via HTTP
// ---------------------------------------------------------------------------
#[tokio::test]
async fn load_100_concurrent_http_registrations() {
    let state = ApiState::new();
    let app = build_router(state);

    let mut handles = Vec::new();
    for i in 0..100 {
        let router = app.clone();
        handles.push(tokio::spawn(async move {
            let name = format!("agent-reg-{}", i);
            let body = json!({"name": name, "capabilities": ["test"]});
            let req = Request::builder()
                .method("POST")
                .uri("/v1/agents/register")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap();
            let resp = router.oneshot(req).await.unwrap();
            resp.status()
        }));
    }

    let statuses: Vec<StatusCode> = futures_collect(handles).await;
    let created = statuses.iter().filter(|s| **s == StatusCode::CREATED).count();
    assert_eq!(created, 100, "All 100 registrations should succeed");

    // Verify via list endpoint
    let list_req = Request::builder()
        .uri("/v1/agents")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(list_req).await.unwrap();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let list: AgentListResponse = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(list.total, 100);
}

// ---------------------------------------------------------------------------
// 2. 100 concurrent task submissions
// ---------------------------------------------------------------------------
#[tokio::test]
async fn load_100_concurrent_task_submissions() {
    let orch = shared_orchestrator();
    let mut handles = Vec::new();

    for _ in 0..100 {
        let o = orch.clone();
        handles.push(tokio::spawn(async move {
            o.submit_task(make_task(TaskPriority::Normal)).await.unwrap();
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    let stats = orch.get_queue_stats().await;
    assert_eq!(stats.queued_tasks, 100);
}

// ---------------------------------------------------------------------------
// 3. Mixed priority flood (200 tasks, 40 per level)
// ---------------------------------------------------------------------------
#[tokio::test]
async fn load_mixed_priority_flood() {
    let orch = shared_orchestrator();

    let priorities = [
        TaskPriority::Critical,
        TaskPriority::High,
        TaskPriority::Normal,
        TaskPriority::Low,
        TaskPriority::Background,
    ];

    let mut handles = Vec::new();
    for priority in &priorities {
        for _ in 0..40 {
            let o = orch.clone();
            let p = *priority;
            handles.push(tokio::spawn(async move {
                o.submit_task(make_task(p)).await.unwrap();
            }));
        }
    }

    for h in handles {
        h.await.unwrap();
    }

    let stats = orch.get_queue_stats().await;
    assert_eq!(stats.queued_tasks, 200);

    // Peek should return a Critical task first
    let next = orch.peek_next_task().await;
    assert!(next.is_some());
    assert_eq!(next.unwrap().priority, TaskPriority::Critical);
}

// ---------------------------------------------------------------------------
// 4. Rapid heartbeats (10 agents x 100 heartbeats = 1000)
// ---------------------------------------------------------------------------
#[tokio::test]
async fn load_rapid_heartbeats() {
    let state = ApiState::new();
    let app = build_router(state);

    // Register 10 agents, collect their IDs
    let mut agent_ids = Vec::new();
    for i in 0..10 {
        let (status, body) = register_via_http(&app, &format!("hb-agent-{}", i)).await;
        assert_eq!(status, StatusCode::CREATED);
        let id = body["id"].as_str().unwrap().to_string();
        agent_ids.push(id);
    }

    // 10 agents x 100 heartbeats concurrently
    let mut handles = Vec::new();
    for agent_id in &agent_ids {
        for _ in 0..100 {
            let router = app.clone();
            let id = agent_id.clone();
            handles.push(tokio::spawn(async move {
                let hb = json!({"status": "running", "cpu_percent": 42.0, "memory_mb": 128});
                let req = Request::builder()
                    .method("POST")
                    .uri(format!("/v1/agents/{}/heartbeat", id))
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&hb).unwrap()))
                    .unwrap();
                let resp = router.oneshot(req).await.unwrap();
                resp.status()
            }));
        }
    }

    let statuses: Vec<StatusCode> = futures_collect(handles).await;
    assert!(
        statuses.iter().all(|s| *s == StatusCode::OK),
        "All 1000 heartbeats should succeed"
    );
}

// ---------------------------------------------------------------------------
// 5. Register-deregister churn (50 cycles)
// ---------------------------------------------------------------------------
#[tokio::test]
async fn load_register_deregister_churn() {
    let state = ApiState::new();
    let app = build_router(state);

    for i in 0..50 {
        // Register
        let (status, body) = register_via_http(&app, &format!("churn-{}", i)).await;
        assert_eq!(status, StatusCode::CREATED);
        let id = body["id"].as_str().unwrap().to_string();

        // Deregister
        let req = Request::builder()
            .method("DELETE")
            .uri(format!("/v1/agents/{}", id))
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    // Verify empty
    let list_req = Request::builder()
        .uri("/v1/agents")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(list_req).await.unwrap();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let list: AgentListResponse = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(list.total, 0, "All agents should be deregistered");
}

// ---------------------------------------------------------------------------
// 6. Task submission + result storage race (50 tasks)
// ---------------------------------------------------------------------------
#[tokio::test]
async fn load_task_submit_and_result_race() {
    let orch = shared_orchestrator();

    // Submit 50 tasks, collect their IDs
    let mut task_ids = Vec::new();
    for _ in 0..50 {
        let id = orch.submit_task(make_task(TaskPriority::Normal)).await.unwrap();
        task_ids.push(id);
    }

    // Store results concurrently
    let mut handles = Vec::new();
    for tid in &task_ids {
        let o = orch.clone();
        let tid = tid.clone();
        handles.push(tokio::spawn(async move {
            let result = TaskResult {
                task_id: tid,
                agent_id: AgentId::new(),
                success: true,
                result: Some(json!({"done": true})),
                error: None,
                completed_at: Utc::now(),
                duration_ms: 10,
            };
            o.store_result(result).await.unwrap();
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    // All results must be retrievable
    for tid in &task_ids {
        let r = orch.get_result(tid).await;
        assert!(r.is_some(), "Result for task {} should exist", tid);
        assert!(r.unwrap().success);
    }
}

// ---------------------------------------------------------------------------
// 7. Concurrent task cancellation
// ---------------------------------------------------------------------------
#[tokio::test]
async fn load_concurrent_task_cancellation() {
    let orch = shared_orchestrator();

    // Submit 100 tasks
    let mut ids = Vec::new();
    for _ in 0..100 {
        let id = orch.submit_task(make_task(TaskPriority::Normal)).await.unwrap();
        ids.push(id);
    }
    assert_eq!(orch.get_queue_stats().await.queued_tasks, 100);

    // Cancel first 50 concurrently while submitting 50 more
    let mut handles = Vec::new();
    for id in ids.iter().take(50) {
        let o = orch.clone();
        let id = id.clone();
        handles.push(tokio::spawn(async move {
            o.cancel_task(&id).await.unwrap();
        }));
    }
    for _ in 0..50 {
        let o = orch.clone();
        handles.push(tokio::spawn(async move {
            o.submit_task(make_task(TaskPriority::High)).await.unwrap();
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    // 100 - 50 cancelled + 50 new = 100
    let stats = orch.get_queue_stats().await;
    assert_eq!(stats.queued_tasks, 100, "Should have 100 tasks after cancel+submit");
}

// ---------------------------------------------------------------------------
// 8. Orchestrator broadcast under load (100 messages)
// ---------------------------------------------------------------------------
#[tokio::test]
async fn load_broadcast_under_load() {
    let registry = Arc::new(AgentRegistry::new());
    // No agents registered, so broadcast sends 0 messages per call but should not panic.
    let orch = Arc::new(Orchestrator::new(registry));

    let mut handles = Vec::new();
    for i in 0..100 {
        let o = orch.clone();
        handles.push(tokio::spawn(async move {
            o.broadcast(
                agnos_common::MessageType::Event,
                json!({"event": "ping", "seq": i}),
            )
            .await
        }));
    }

    for h in handles {
        let result = h.await.unwrap();
        assert!(result.is_ok(), "Broadcast should succeed: {:?}", result);
    }
}

// ---------------------------------------------------------------------------
// 9. Queue stats consistency under concurrent ops
// ---------------------------------------------------------------------------
#[tokio::test]
async fn load_queue_stats_consistency() {
    let orch = shared_orchestrator();

    // Spawn writers
    let mut handles = Vec::new();
    for _ in 0..50 {
        let o = orch.clone();
        handles.push(tokio::spawn(async move {
            o.submit_task(make_task(TaskPriority::Normal)).await.unwrap();
        }));
    }

    // Spawn readers concurrently
    for _ in 0..50 {
        let o = orch.clone();
        handles.push(tokio::spawn(async move {
            let stats = o.get_queue_stats().await;
            // Invariant: no field is ever negative (they are usize, so this checks
            // that the arithmetic didn't wrap around via underflow).
            assert!(stats.total_tasks <= 1_000_000, "total_tasks looks wrong");
            assert!(stats.queued_tasks <= 1_000_000, "queued_tasks looks wrong");
            assert!(stats.running_tasks <= 1_000_000, "running_tasks looks wrong");
            // total = queued + running must always hold
            assert_eq!(stats.total_tasks, stats.queued_tasks + stats.running_tasks);
        }));
    }

    for h in handles {
        h.await.unwrap();
    }
}

// ---------------------------------------------------------------------------
// 10. Large payload handling (1 MB JSON payloads)
// ---------------------------------------------------------------------------
#[tokio::test]
async fn load_large_payload_handling() {
    let orch = shared_orchestrator();

    // Create a ~1 MB string payload
    let big_string = "x".repeat(1_000_000);

    let mut handles = Vec::new();
    for i in 0..5 {
        let o = orch.clone();
        let payload = json!({"data": big_string, "index": i});
        handles.push(tokio::spawn(async move {
            let mut task = make_task(TaskPriority::Normal);
            task.payload = payload;
            let id = o.submit_task(task).await.unwrap();
            id
        }));
    }

    let ids: Vec<String> = futures_collect(handles).await;
    assert_eq!(ids.len(), 5);

    let stats = orch.get_queue_stats().await;
    assert_eq!(stats.queued_tasks, 5);

    // Verify payloads survive round-trip via peek
    let next = orch.peek_next_task().await.unwrap();
    let data = next.payload["data"].as_str().unwrap();
    assert_eq!(data.len(), 1_000_000);
}

// ---------------------------------------------------------------------------
// 11. Overdue task detection under load (100 tasks with past deadlines)
// ---------------------------------------------------------------------------
#[tokio::test]
async fn load_overdue_task_detection() {
    let orch = shared_orchestrator();

    let past = Utc::now() - chrono::Duration::hours(1);

    let mut handles = Vec::new();
    for _ in 0..100 {
        let o = orch.clone();
        handles.push(tokio::spawn(async move {
            o.submit_task(make_task_with_deadline(past)).await.unwrap();
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    let overdue = orch.get_overdue_tasks().await;
    assert_eq!(overdue.len(), 100, "All 100 tasks should be overdue");
}

// ---------------------------------------------------------------------------
// 12. Agent metrics aggregation (100 agents with CPU/memory heartbeats)
// ---------------------------------------------------------------------------
#[tokio::test]
async fn load_agent_metrics_aggregation() {
    let state = ApiState::new();
    let app = build_router(state);

    // Register 100 agents and send heartbeats with varied metrics
    let mut agent_ids = Vec::new();
    for i in 0..100 {
        let (status, body) = register_via_http(&app, &format!("metrics-agent-{}", i)).await;
        assert_eq!(status, StatusCode::CREATED);
        agent_ids.push(body["id"].as_str().unwrap().to_string());
    }

    // Send heartbeats with cpu=i, memory=10 for all
    let mut handles = Vec::new();
    for (i, agent_id) in agent_ids.iter().enumerate() {
        let router = app.clone();
        let id = agent_id.clone();
        let cpu = i as f32;
        handles.push(tokio::spawn(async move {
            let hb = json!({"status": "running", "cpu_percent": cpu, "memory_mb": 10});
            let req = Request::builder()
                .method("POST")
                .uri(format!("/v1/agents/{}/heartbeat", id))
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&hb).unwrap()))
                .unwrap();
            let resp = router.oneshot(req).await.unwrap();
            assert_eq!(resp.status(), StatusCode::OK);
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    // Check metrics endpoint
    let req = Request::builder()
        .uri("/v1/metrics")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let metrics: AgentMetricsResponse = serde_json::from_slice(&bytes).unwrap();

    assert_eq!(metrics.total_agents, 100);
    assert_eq!(metrics.total_memory_mb, 100 * 10);

    // avg_cpu = sum(0..100) / 100 = 49.5
    let avg = metrics.avg_cpu_percent.expect("avg_cpu should be set");
    assert!((avg - 49.5).abs() < 0.1, "Expected avg ~49.5, got {}", avg);
}

// ---------------------------------------------------------------------------
// 13. Rapid register-get cycles (100 concurrent)
// ---------------------------------------------------------------------------
#[tokio::test]
async fn load_rapid_register_get_cycles() {
    let state = ApiState::new();
    let app = build_router(state);

    let mut handles = Vec::new();
    for i in 0..100 {
        let router = app.clone();
        handles.push(tokio::spawn(async move {
            // Register
            let name = format!("rg-agent-{}", i);
            let body = json!({"name": name, "capabilities": ["test"]});
            let req = Request::builder()
                .method("POST")
                .uri("/v1/agents/register")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap();
            let resp = router.clone().oneshot(req).await.unwrap();
            assert_eq!(resp.status(), StatusCode::CREATED);

            let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
            let reg: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            let id = reg["id"].as_str().unwrap().to_string();

            // Immediately GET by id
            let get_req = Request::builder()
                .uri(format!("/v1/agents/{}", id))
                .body(Body::empty())
                .unwrap();
            let get_resp = router.oneshot(get_req).await.unwrap();
            assert_eq!(get_resp.status(), StatusCode::OK);

            let get_bytes = axum::body::to_bytes(get_resp.into_body(), usize::MAX).await.unwrap();
            let detail: serde_json::Value = serde_json::from_slice(&get_bytes).unwrap();
            assert_eq!(detail["name"], name);
        }));
    }

    for h in handles {
        h.await.unwrap();
    }
}

// ---------------------------------------------------------------------------
// 14. Task dependency chain (10 tasks, each depends on previous)
// ---------------------------------------------------------------------------
#[tokio::test]
async fn load_task_dependency_chain() {
    let orch = shared_orchestrator();

    // Submit a chain: task_0 has no deps, task_1 depends on task_0, etc.
    let mut prev_id: Option<String> = None;
    let mut task_ids = Vec::new();

    for _ in 0..10 {
        let mut task = make_task(TaskPriority::Normal);
        if let Some(ref dep) = prev_id {
            task.dependencies = vec![dep.clone()];
        }
        let id = orch.submit_task(task).await.unwrap();
        prev_id = Some(id.clone());
        task_ids.push(id);
    }

    let stats = orch.get_queue_stats().await;
    assert_eq!(stats.queued_tasks, 10);

    // The first task has no dependencies, so it should be peek-able.
    // (All are Normal priority; peek returns the first in the Normal queue.)
    let next = orch.peek_next_task().await.unwrap();
    assert!(next.dependencies.is_empty(), "First task should have no dependencies");

    // Now simulate completing task_0 and store its result
    let result = TaskResult {
        task_id: task_ids[0].clone(),
        agent_id: AgentId::new(),
        success: true,
        result: Some(json!({"ok": true})),
        error: None,
        completed_at: Utc::now(),
        duration_ms: 5,
    };
    orch.store_result(result).await.unwrap();

    // Verify the result is accessible (dependency satisfied for task_1)
    let r = orch.get_result(&task_ids[0]).await;
    assert!(r.is_some());
}

// ---------------------------------------------------------------------------
// 15. Concurrent result pruning (200 results stored concurrently)
// ---------------------------------------------------------------------------
#[tokio::test]
async fn load_concurrent_result_storage() {
    let orch = shared_orchestrator();

    let mut handles = Vec::new();
    for i in 0..200 {
        let o = orch.clone();
        handles.push(tokio::spawn(async move {
            let result = TaskResult {
                task_id: format!("prune-task-{}", i),
                agent_id: AgentId::new(),
                success: i % 3 != 0, // mix of successes and failures
                result: Some(json!({"i": i})),
                error: if i % 3 == 0 {
                    Some("simulated failure".to_string())
                } else {
                    None
                },
                completed_at: Utc::now(),
                duration_ms: i as u64,
            };
            o.store_result(result).await.unwrap();
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    // All 200 should be stored (MAX_RESULTS is 10_000 so no pruning yet)
    let mut found = 0;
    for i in 0..200 {
        if orch.get_result(&format!("prune-task-{}", i)).await.is_some() {
            found += 1;
        }
    }
    assert_eq!(found, 200, "All 200 results should be stored");
}

// ---------------------------------------------------------------------------
// Utility: collect JoinHandle results
// ---------------------------------------------------------------------------
async fn futures_collect<T: Send + 'static>(
    handles: Vec<tokio::task::JoinHandle<T>>,
) -> Vec<T> {
    let mut results = Vec::with_capacity(handles.len());
    for h in handles {
        results.push(h.await.unwrap());
    }
    results
}
