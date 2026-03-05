// SPDX-License-Identifier: GPL-3.0
//! System-level benchmarks for AGNOS agent-runtime.
//!
//! These benchmarks measure end-to-end operations rather than isolated micro-ops:
//!   - Agent spawn + register + unregister lifecycle
//!   - IPC MessageBus round-trip (subscribe, publish, receive)
//!   - Orchestrator task submission + scheduling throughput
//!   - ResourceManager memory reserve / release cycle

use std::sync::Arc;

use agent_runtime::agent::Agent;
use agent_runtime::ipc::MessageBus;
use agent_runtime::orchestrator::{Orchestrator, Task, TaskPriority, TaskRequirements, TaskResult};
use agent_runtime::registry::AgentRegistry;
use agent_runtime::resource::ResourceManager;
use agnos_common::{
    AgentConfig, AgentId, AgentType, Message, MessageType, ResourceLimits, SandboxConfig,
};
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use tokio::runtime::Runtime;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_config(name: &str) -> AgentConfig {
    AgentConfig {
        name: name.to_string(),
        agent_type: AgentType::User,
        sandbox: SandboxConfig::default(),
        resource_limits: ResourceLimits::default(),
        permissions: vec![],
        metadata: serde_json::Value::Null,
    }
}

fn make_task(priority: TaskPriority) -> Task {
    Task {
        id: Uuid::new_v4().to_string(),
        priority,
        target_agents: vec![],
        payload: serde_json::json!({"action": "bench"}),
        created_at: chrono::Utc::now(),
        deadline: None,
        dependencies: vec![],
        requirements: TaskRequirements::default(),
    }
}

fn make_message(source: &str, target: &str) -> Message {
    Message {
        id: Uuid::new_v4().to_string(),
        source: source.to_string(),
        target: target.to_string(),
        message_type: MessageType::Command,
        payload: serde_json::json!({"data": "bench-payload"}),
        timestamp: chrono::Utc::now(),
    }
}

// ---------------------------------------------------------------------------
// 1. Agent lifecycle: create -> register -> unregister
// ---------------------------------------------------------------------------

fn bench_agent_lifecycle(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("system/agent_lifecycle_create_register_unregister", |b| {
        b.iter(|| {
            rt.block_on(async {
                let registry = Arc::new(AgentRegistry::new());
                let config = make_config("lifecycle-bench");
                let (agent, _rx) = Agent::new(config.clone()).await.unwrap();
                let id = agent.id();

                let handle = registry.register(&agent, config).await.unwrap();
                black_box(&handle);

                registry.unregister(id).await.unwrap();
            });
        });
    });
}

fn bench_agent_create_only(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("system/agent_create", |b| {
        b.iter(|| {
            rt.block_on(async {
                let config = make_config("create-bench");
                let (agent, _rx) = Agent::new(config).await.unwrap();
                black_box(agent.id());
            });
        });
    });
}

fn bench_registry_register_unregister(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("system/registry_register_unregister");
    for count in [1, 10, 50] {
        group.throughput(Throughput::Elements(count as u64));
        group.bench_with_input(BenchmarkId::from_parameter(count), &count, |b, &n| {
            b.iter(|| {
                rt.block_on(async {
                    let registry = Arc::new(AgentRegistry::new());
                    let mut ids = Vec::with_capacity(n);

                    // Register N agents
                    for i in 0..n {
                        let config = make_config(&format!("reg-bench-{}", i));
                        let (agent, _rx) = Agent::new(config.clone()).await.unwrap();
                        let id = agent.id();
                        registry.register(&agent, config).await.unwrap();
                        ids.push(id);
                    }

                    // Unregister all
                    for id in ids {
                        registry.unregister(id).await.unwrap();
                    }
                });
            });
        });
    }
    group.finish();
}

// ---------------------------------------------------------------------------
// 2. IPC MessageBus round-trip
// ---------------------------------------------------------------------------

fn bench_ipc_messagebus_roundtrip(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("system/ipc_messagebus_roundtrip", |b| {
        b.iter(|| {
            rt.block_on(async {
                let bus = MessageBus::new();
                let agent_a = AgentId::new();
                let agent_b = AgentId::new();

                let (tx_a, mut rx_a) = tokio::sync::mpsc::channel::<Message>(16);
                let (tx_b, mut rx_b) = tokio::sync::mpsc::channel::<Message>(16);

                bus.subscribe(agent_a, tx_a).await;
                bus.subscribe(agent_b, tx_b).await;

                // A -> B
                let msg = make_message(&agent_a.to_string(), &agent_b.to_string());
                bus.send_to(agent_b, msg).await.unwrap();

                let received = rx_b.recv().await.unwrap();
                black_box(received);

                // B -> A
                let msg = make_message(&agent_b.to_string(), &agent_a.to_string());
                bus.send_to(agent_a, msg).await.unwrap();

                let received = rx_a.recv().await.unwrap();
                black_box(received);
            });
        });
    });
}

fn bench_ipc_messagebus_publish_broadcast(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("system/ipc_broadcast");
    for subscriber_count in [1, 5, 20] {
        group.throughput(Throughput::Elements(subscriber_count as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(subscriber_count),
            &subscriber_count,
            |b, &n| {
                b.iter(|| {
                    rt.block_on(async {
                        let bus = MessageBus::new();
                        let mut receivers = Vec::with_capacity(n);

                        for _ in 0..n {
                            let id = AgentId::new();
                            let (tx, rx) = tokio::sync::mpsc::channel::<Message>(16);
                            bus.subscribe(id, tx).await;
                            receivers.push(rx);
                        }

                        let msg = make_message("system", "broadcast");
                        bus.publish(msg).await.unwrap();

                        for rx in &mut receivers {
                            let received = rx.recv().await.unwrap();
                            black_box(received);
                        }
                    });
                });
            },
        );
    }
    group.finish();
}

// ---------------------------------------------------------------------------
// 3. Orchestrator task submission + scheduling
// ---------------------------------------------------------------------------

fn bench_orchestrator_submit_tasks(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("system/orchestrator_submit");
    for count in [1, 10, 100] {
        group.throughput(Throughput::Elements(count as u64));
        group.bench_with_input(BenchmarkId::from_parameter(count), &count, |b, &n| {
            b.iter(|| {
                rt.block_on(async {
                    let registry = Arc::new(AgentRegistry::new());
                    let orchestrator = Orchestrator::new(registry);

                    for _ in 0..n {
                        let task = make_task(TaskPriority::Normal);
                        orchestrator.submit_task(task).await.unwrap();
                    }

                    let stats = orchestrator.get_queue_stats().await;
                    black_box(stats);
                });
            });
        });
    }
    group.finish();
}

fn bench_orchestrator_submit_mixed_priorities(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("system/orchestrator_submit_mixed_priorities_50", |b| {
        b.iter(|| {
            rt.block_on(async {
                let registry = Arc::new(AgentRegistry::new());
                let orchestrator = Orchestrator::new(registry);

                let priorities = [
                    TaskPriority::Critical,
                    TaskPriority::High,
                    TaskPriority::Normal,
                    TaskPriority::Low,
                    TaskPriority::Background,
                ];

                for i in 0..50 {
                    let task = make_task(priorities[i % priorities.len()]);
                    orchestrator.submit_task(task).await.unwrap();
                }

                let stats = orchestrator.get_queue_stats().await;
                black_box(stats);
            });
        });
    });
}

fn bench_orchestrator_submit_and_store_result(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("system/orchestrator_submit_and_result", |b| {
        b.iter(|| {
            rt.block_on(async {
                let registry = Arc::new(AgentRegistry::new());
                let orchestrator = Orchestrator::new(registry);

                let task = make_task(TaskPriority::Normal);
                let task_id = orchestrator.submit_task(task).await.unwrap();

                let result = TaskResult {
                    task_id: task_id.clone(),
                    agent_id: AgentId::new(),
                    success: true,
                    result: Some(serde_json::json!({"output": "done"})),
                    error: None,
                    completed_at: chrono::Utc::now(),
                    duration_ms: 42,
                };

                orchestrator.store_result(result).await.unwrap();

                let stored = orchestrator.get_result(&task_id).await;
                black_box(stored);
            });
        });
    });
}

// ---------------------------------------------------------------------------
// 4. ResourceManager memory reserve / release cycle
// ---------------------------------------------------------------------------

fn bench_resource_memory_cycle(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("system/resource_memory_reserve_release", |b| {
        b.iter(|| {
            rt.block_on(async {
                let rm = ResourceManager::new().await.unwrap();
                let agent = AgentId::new();

                // Reserve 64 MB
                let bytes: u64 = 64 * 1024 * 1024;
                rm.reserve_memory(agent, bytes).await.unwrap();
                let avail = rm.available_memory().await;
                black_box(avail);

                // Release
                rm.release_memory(bytes).await;
                let avail = rm.available_memory().await;
                black_box(avail);
            });
        });
    });
}

fn bench_resource_memory_concurrent_reserves(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("system/resource_memory_concurrent");
    for count in [1, 5, 20] {
        group.throughput(Throughput::Elements(count as u64));
        group.bench_with_input(BenchmarkId::from_parameter(count), &count, |b, &n| {
            b.iter(|| {
                rt.block_on(async {
                    let rm = Arc::new(ResourceManager::new().await.unwrap());
                    let chunk: u64 = 1024 * 1024; // 1 MB each

                    let mut handles = Vec::with_capacity(n);
                    for _ in 0..n {
                        let rm = Arc::clone(&rm);
                        handles.push(tokio::spawn(async move {
                            let agent = AgentId::new();
                            rm.reserve_memory(agent, chunk).await.unwrap();
                        }));
                    }

                    for h in handles {
                        h.await.unwrap();
                    }

                    black_box(rm.available_memory().await);
                });
            });
        });
    }
    group.finish();
}

// ---------------------------------------------------------------------------
// Groups
// ---------------------------------------------------------------------------

criterion_group!(
    agent_lifecycle,
    bench_agent_lifecycle,
    bench_agent_create_only,
    bench_registry_register_unregister
);

criterion_group!(
    ipc_benches,
    bench_ipc_messagebus_roundtrip,
    bench_ipc_messagebus_publish_broadcast
);

criterion_group!(
    orchestrator_benches,
    bench_orchestrator_submit_tasks,
    bench_orchestrator_submit_mixed_priorities,
    bench_orchestrator_submit_and_store_result
);

criterion_group!(
    resource_benches,
    bench_resource_memory_cycle,
    bench_resource_memory_concurrent_reserves
);

criterion_main!(
    agent_lifecycle,
    ipc_benches,
    orchestrator_benches,
    resource_benches
);
