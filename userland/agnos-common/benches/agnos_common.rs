use agnos_common::{
    AgentConfig, AgentId, AgentType, InferenceRequest, Permission, ResourceLimits, SandboxConfig,
};
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn benchmark_agent_id_generation(c: &mut Criterion) {
    c.bench_function("agent_id_new", |b| {
        b.iter(AgentId::new);
    });
}

fn benchmark_agent_id_parsing(c: &mut Criterion) {
    let id = AgentId::new();
    let _id_str = id.to_string();

    c.bench_function("agent_id_to_string", |b| {
        b.iter(|| black_box(&id).to_string());
    });

    c.bench_function("agent_id_display", |b| {
        b.iter(|| format!("{}", black_box(&id)));
    });
}

fn benchmark_inference_request_serialization(c: &mut Criterion) {
    let request = InferenceRequest {
        prompt: "Hello, how are you?".to_string(),
        model: "llama2".to_string(),
        max_tokens: 512,
        temperature: 0.7,
        top_p: 0.9,
        presence_penalty: 0.0,
        frequency_penalty: 0.0,
    };

    c.bench_function("inference_request_serialize", |b| {
        b.iter(|| serde_json::to_string(black_box(&request)));
    });

    c.bench_function("inference_request_deserialize", |b| {
        let json = r#"{"prompt":"test","model":"llama2","max_tokens":512,"temperature":0.7,"top_p":1.0,"presence_penalty":0.0,"frequency_penalty":0.0}"#;
        b.iter(|| serde_json::from_str::<InferenceRequest>(black_box(json)));
    });
}

fn benchmark_agent_config(c: &mut Criterion) {
    c.bench_function("agent_config_default", |b| {
        b.iter(AgentConfig::default);
    });

    c.bench_function("agent_config_new", |b| {
        b.iter(|| AgentConfig {
            name: "test-agent".to_string(),
            agent_type: AgentType::Service,
            resource_limits: ResourceLimits::default(),
            sandbox: SandboxConfig::default(),
            permissions: vec![Permission::FileRead, Permission::NetworkAccess],
            metadata: serde_json::json!({}),
        });
    });
}

fn benchmark_json_serialization(c: &mut Criterion) {
    let config = AgentConfig::default();

    c.bench_function("serde_json_to_string", |b| {
        b.iter(|| serde_json::to_string(black_box(&config)));
    });

    c.bench_function("serde_json_to_vec", |b| {
        b.iter(|| serde_json::to_vec(black_box(&config)));
    });
}

criterion_group!(
    benches,
    benchmark_agent_id_generation,
    benchmark_agent_id_parsing,
    benchmark_inference_request_serialization,
    benchmark_agent_config,
    benchmark_json_serialization
);
criterion_main!(benches);
