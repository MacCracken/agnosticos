use criterion::{black_box, criterion_group, criterion_main, Criterion};

use llm_gateway::acceleration::{
    AcceleratorRegistry, QuantizationLevel,
};
use llm_gateway::rate_limiter::GatewayMetrics;

fn bench_gateway_metrics_record(c: &mut Criterion) {
    let mut group = c.benchmark_group("gateway_metrics");

    let metrics = GatewayMetrics::new();

    group.bench_function("record_request", |b| {
        b.iter(|| {
            metrics.record_request(
                black_box("gpt-4"),
                black_box(500),
                black_box(200),
                black_box(300),
                black_box(true),
            );
        });
    });

    group.bench_function("record_cache_hit", |b| {
        b.iter(|| {
            metrics.record_cache_hit(black_box("gpt-4"));
        });
    });

    group.bench_function("cache_hit_rate", |b| {
        // Seed some data
        for _ in 0..100 {
            metrics.record_cache_hit("gpt-4");
            metrics.record_cache_miss("gpt-4");
        }
        b.iter(|| black_box(metrics.cache_hit_rate("gpt-4")));
    });

    group.bench_function("export_prometheus_10_models", |b| {
        let m = GatewayMetrics::new();
        for i in 0..10 {
            let model = format!("model-{}", i);
            for _ in 0..50 {
                m.record_request(&model, 100, 50, 200, true);
            }
            m.record_cache_hit(&model);
            m.record_rate_limit(&format!("agent-{}", i));
        }
        b.iter(|| black_box(m.export_prometheus()));
    });

    group.finish();
}

fn bench_accelerator_registry(c: &mut Criterion) {
    let mut group = c.benchmark_group("acceleration");

    group.bench_function("estimate_memory_7b_fp16", |b| {
        b.iter(|| {
            black_box(AcceleratorRegistry::estimate_memory(
                black_box(7_000_000_000),
                black_box(&QuantizationLevel::Float16),
            ))
        });
    });

    group.bench_function("plan_sharding_7b_int4", |b| {
        let reg = AcceleratorRegistry::new();
        b.iter(|| {
            black_box(reg.plan_sharding(
                black_box(7_000_000_000),
                black_box(&QuantizationLevel::Int4),
            ))
        });
    });

    group.bench_function("plan_sharding_70b_fp32", |b| {
        let reg = AcceleratorRegistry::new();
        b.iter(|| {
            black_box(reg.plan_sharding(
                black_box(70_000_000_000),
                black_box(&QuantizationLevel::None),
            ))
        });
    });

    group.bench_function("best_available", |b| {
        let reg = AcceleratorRegistry::detect_available();
        b.iter(|| black_box(reg.best_available()));
    });

    group.bench_function("quantization_memory_reduction", |b| {
        let levels = [
            QuantizationLevel::None,
            QuantizationLevel::Float16,
            QuantizationLevel::BFloat16,
            QuantizationLevel::Int8,
            QuantizationLevel::Int4,
        ];
        b.iter(|| {
            for level in &levels {
                black_box(level.memory_reduction_factor());
            }
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_gateway_metrics_record,
    bench_accelerator_registry,
);
criterion_main!(benches);
