use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use std::path::Path;

use agent_runtime::knowledge_base::{KnowledgeBase, KnowledgeSource};
use agent_runtime::learning::{AnomalyDetector, BehaviorSample};
use agent_runtime::rag::{RagConfig, RagPipeline};
use agent_runtime::vector_store::{VectorEntry, VectorIndex};
use agent_runtime::RpcRegistry;
use agnos_common::AgentId;

fn bench_vector_search(c: &mut Criterion) {
    let mut group = c.benchmark_group("vector_store");

    for size in [100, 500, 1000] {
        let mut index = VectorIndex::new();
        let dim = 64;
        for i in 0..size {
            let embedding: Vec<f64> = (0..dim).map(|j| ((i * dim + j) as f64).sin()).collect();
            let entry = VectorEntry {
                id: uuid::Uuid::new_v4(),
                embedding,
                metadata: serde_json::json!({"index": i}),
                content: format!("Document {}", i),
                created_at: chrono::Utc::now(),
            };
            index.insert(entry).unwrap();
        }

        let query: Vec<f64> = (0..dim).map(|j| (j as f64 * 0.1).cos()).collect();

        group.bench_with_input(BenchmarkId::new("search_top10", size), &size, |b, _| {
            b.iter(|| black_box(index.search(&query, 10)));
        });
    }
    group.finish();
}

fn bench_rag_query(c: &mut Criterion) {
    let mut pipeline = RagPipeline::new(RagConfig::default());

    for i in 0..20 {
        let text = format!(
            "This is document number {}. It contains information about topic {} and relates to \
             various system components. The agent runtime manages lifecycle events and coordinates \
             between multiple subsystems.",
            i,
            i % 5
        );
        let _ = pipeline.ingest_text(&text, serde_json::json!({"doc": i}));
    }

    c.bench_function("rag_query", |b| {
        b.iter(|| black_box(pipeline.query_text("agent runtime lifecycle")));
    });
}

fn bench_knowledge_search(c: &mut Criterion) {
    let mut kb = KnowledgeBase::new();

    let subsystems = ["network", "storage", "compute", "security", "telemetry"];
    for i in 0..100 {
        let content = format!(
            "Configuration file {} defines parameters for the {} subsystem. Key settings include \
             timeout values, buffer sizes, and retry policies for handling distributed agent \
             communication.",
            i,
            subsystems[i % 5]
        );
        let _ = kb.index_text(
            &content,
            KnowledgeSource::ConfigFile,
            Path::new(&format!("/etc/agnos/config_{}.toml", i)),
        );
    }

    c.bench_function("knowledge_search_100docs", |b| {
        b.iter(|| black_box(kb.search("network timeout retry", 10)));
    });
}

fn bench_rpc_registry(c: &mut Criterion) {
    let mut group = c.benchmark_group("rpc");

    let mut registry = RpcRegistry::new();
    let mut agent_ids = Vec::new();
    for _ in 0..100 {
        agent_ids.push(AgentId::new());
    }
    for (i, agent_id) in agent_ids.iter().enumerate() {
        for j in 0..5 {
            registry.register_method(*agent_id, &format!("method_{}_{}", i, j));
        }
    }

    group.bench_function("find_handler_500methods", |b| {
        b.iter(|| black_box(registry.find_handler("method_50_3")));
    });

    group.bench_function("list_all_methods", |b| {
        b.iter(|| black_box(registry.all_methods()));
    });

    group.finish();
}

fn bench_anomaly_detection(c: &mut Criterion) {
    let mut group = c.benchmark_group("anomaly");

    let agent_id = AgentId::new();
    let anomaly_agent_id = AgentId::new();

    let mut detector = AnomalyDetector::new(100, 2.0);
    for i in 0..50u64 {
        let sample = BehaviorSample {
            timestamp: chrono::Utc::now(),
            syscall_count: 1000 + (i % 100),
            network_bytes: 50000 + (i * 100),
            file_ops: 200 + (i % 50),
            cpu_percent: 25.0 + (i as f64 % 10.0),
            memory_bytes: 100_000_000 + (i * 1000),
        };
        let _ = detector.record_behavior(agent_id, sample);
    }

    group.bench_function("record_behavior_normal", |b| {
        b.iter(|| {
            let sample = BehaviorSample {
                timestamp: chrono::Utc::now(),
                syscall_count: 1050,
                network_bytes: 52000,
                file_ops: 220,
                cpu_percent: 28.0,
                memory_bytes: 100_050_000,
            };
            black_box(detector.record_behavior(agent_id, sample))
        });
    });

    group.bench_function("record_behavior_anomalous", |b| {
        b.iter(|| {
            let sample = BehaviorSample {
                timestamp: chrono::Utc::now(),
                syscall_count: 99999,
                network_bytes: 999999999,
                file_ops: 99999,
                cpu_percent: 99.0,
                memory_bytes: 999_999_999,
            };
            black_box(detector.record_behavior(anomaly_agent_id, sample))
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_vector_search,
    bench_rag_query,
    bench_knowledge_search,
    bench_rpc_registry,
    bench_anomaly_detection,
);
criterion_main!(benches);
