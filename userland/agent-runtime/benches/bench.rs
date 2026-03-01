use agent_runtime::agent::AgentHandle;
use agent_runtime::orchestrator::{Task, TaskPriority, TaskResult};
use agent_runtime::registry::AgentRegistry;
use agnos_common::{AgentConfig, AgentId, AgentStatus, AgentType, ResourceLimits, SandboxConfig};
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use uuid::Uuid;

fn benchmark_agent_id_generation(c: &mut Criterion) {
    c.bench_function("agent_id_generation", |b| {
        b.iter(|| black_box(AgentId::new()));
    });
}

fn benchmark_agent_config_creation(c: &mut Criterion) {
    c.bench_function("agent_config_creation", |b| {
        b.iter(|| {
            let config = AgentConfig {
                name: "bench-agent".to_string(),
                agent_type: AgentType::User,
                sandbox: SandboxConfig::default(),
                resource_limits: ResourceLimits::default(),
                permissions: vec![],
                metadata: serde_json::Value::Null,
            };
            black_box(config);
        });
    });
}

fn benchmark_orchestrator_task_creation(c: &mut Criterion) {
    c.bench_function("orchestrator_task_creation", |b| {
        b.iter(|| {
            let task = Task {
                id: Uuid::new_v4().to_string(),
                priority: TaskPriority::Normal,
                target_agents: vec![],
                payload: serde_json::json!({"action": "test"}),
                created_at: chrono::Utc::now(),
                deadline: None,
                dependencies: vec![],
            };
            black_box(task);
        });
    });
}

fn benchmark_agent_handle_clone(c: &mut Criterion) {
    let handle = AgentHandle {
        id: AgentId::new(),
        name: "agent-1".to_string(),
        status: AgentStatus::Running,
        created_at: chrono::Utc::now(),
        started_at: None,
        resource_usage: agnos_common::ResourceUsage::default(),
    };

    c.bench_function("agent_handle_clone", |b| {
        b.iter(|| black_box(handle.clone()));
    });
}

fn benchmark_task_priority_ordering(c: &mut Criterion) {
    c.bench_function("task_priority_ordering", |b| {
        b.iter(|| {
            let priorities = vec![
                TaskPriority::Critical,
                TaskPriority::High,
                TaskPriority::Normal,
                TaskPriority::Low,
                TaskPriority::Background,
            ];
            black_box(priorities);
        });
    });
}

fn benchmark_task_result_serialization(c: &mut Criterion) {
    let result = TaskResult {
        task_id: "task-1".to_string(),
        agent_id: AgentId::new(),
        success: true,
        result: Some(serde_json::json!({"status": "done"})),
        error: None,
        completed_at: chrono::Utc::now(),
        duration_ms: 100,
    };

    c.bench_function("task_result_serialize", |b| {
        b.iter(|| serde_json::to_string(black_box(&result)));
    });
}

fn benchmark_task_payload_json(c: &mut Criterion) {
    c.bench_function("task_payload_json_parse", |b| {
        let payload = r#"{"action": "test", "data": {"key": "value", "items": [1, 2,3]}}"#;
        b.iter(|| serde_json::from_str::<serde_json::Value>(black_box(payload)));
    });
}

fn benchmark_agent_status_enum(c: &mut Criterion) {
    c.bench_function("agent_status_clone", |b| {
        let status = AgentStatus::Running;
        b.iter(|| black_box(status.clone()));
    });
}

fn benchmark_resource_usage_default(c: &mut Criterion) {
    c.bench_function("resource_usage_default", |b| {
        b.iter(|| black_box(agnos_common::ResourceUsage::default()));
    });
}

fn benchmark_agent_handle_creation(c: &mut Criterion) {
    c.bench_function("agent_handle_creation", |b| {
        b.iter(|| {
            let handle = AgentHandle {
                id: AgentId::new(),
                name: "bench-agent".to_string(),
                status: AgentStatus::Running,
                created_at: chrono::Utc::now(),
                started_at: None,
                resource_usage: agnos_common::ResourceUsage::default(),
            };
            black_box(handle);
        });
    });
}

fn benchmark_task_result_creation(c: &mut Criterion) {
    c.bench_function("task_result_creation", |b| {
        b.iter(|| {
            let result = TaskResult {
                task_id: Uuid::new_v4().to_string(),
                agent_id: AgentId::new(),
                success: true,
                result: Some(serde_json::json!({"status": "done"})),
                error: None,
                completed_at: chrono::Utc::now(),
                duration_ms: 100,
            };
            black_box(result);
        });
    });
}

criterion_group!(
    benches,
    benchmark_agent_id_generation,
    benchmark_agent_config_creation,
    benchmark_orchestrator_task_creation,
    benchmark_agent_handle_clone,
    benchmark_task_priority_ordering,
    benchmark_task_result_serialization,
    benchmark_task_payload_json,
    benchmark_agent_status_enum,
    benchmark_resource_usage_default,
    benchmark_agent_handle_creation,
    benchmark_task_result_creation
);
criterion_main!(benches);
