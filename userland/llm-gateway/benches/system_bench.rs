// SPDX-License-Identifier: GPL-3.0
//! System-level benchmarks for AGNOS LLM Gateway.
//!
//! These benchmarks measure end-to-end operations rather than isolated micro-ops:
//!   - Cache throughput (concurrent set/get with varying entry counts)
//!   - Cache hit vs miss response time impact
//!   - Token accounting throughput (concurrent multi-agent recording)
//!   - Provider selection overhead (health-check + fallback)
//!   - End-to-end inference pipeline (mock: validate -> cache -> accounting -> response)
//!   - Cache expiry cleanup performance at varying sizes

use std::sync::Arc;

use agnos_common::{AgentId, FinishReason, InferenceRequest, InferenceResponse, TokenUsage};
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use llm_gateway::cache::ResponseCache;
use llm_gateway::accounting::TokenAccounting;
use tokio::runtime::Runtime;
use tokio::time::Duration;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_request(i: usize) -> InferenceRequest {
    InferenceRequest {
        prompt: format!("benchmark prompt number {}", i),
        model: "bench-model".to_string(),
        max_tokens: 128,
        temperature: 0.7,
        top_p: 1.0,
        presence_penalty: 0.0,
        frequency_penalty: 0.0,
    }
}

fn make_response(i: usize) -> InferenceResponse {
    InferenceResponse {
        text: format!("benchmark response number {}", i),
        tokens_generated: 10,
        finish_reason: FinishReason::Stop,
        model: "bench-model".to_string(),
        usage: TokenUsage {
            prompt_tokens: 5,
            completion_tokens: 10,
            total_tokens: 15,
        },
    }
}

fn make_usage() -> TokenUsage {
    TokenUsage {
        prompt_tokens: 50,
        completion_tokens: 100,
        total_tokens: 150,
    }
}

// ---------------------------------------------------------------------------
// 1. Cache throughput: concurrent set/get with varying entry counts
// ---------------------------------------------------------------------------

fn bench_cache_set_throughput(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("system/cache_set_throughput");
    for count in [10, 100, 500] {
        group.throughput(Throughput::Elements(count as u64));
        group.bench_with_input(BenchmarkId::from_parameter(count), &count, |b, &n| {
            b.iter(|| {
                rt.block_on(async {
                    let cache = ResponseCache::new(Duration::from_secs(300));
                    for i in 0..n {
                        cache.set(&make_request(i), make_response(i)).await;
                    }
                    let stats = cache.stats().await;
                    black_box(stats);
                });
            });
        });
    }
    group.finish();
}

fn bench_cache_get_throughput(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("system/cache_get_throughput");
    for count in [10, 100, 500] {
        group.throughput(Throughput::Elements(count as u64));
        group.bench_with_input(BenchmarkId::from_parameter(count), &count, |b, &n| {
            // Pre-populate the cache
            let cache = rt.block_on(async {
                let cache = ResponseCache::new(Duration::from_secs(300));
                for i in 0..n {
                    cache.set(&make_request(i), make_response(i)).await;
                }
                cache
            });

            b.iter(|| {
                rt.block_on(async {
                    for i in 0..n {
                        let result = cache.get(&make_request(i)).await;
                        black_box(result);
                    }
                });
            });
        });
    }
    group.finish();
}

fn bench_cache_concurrent_set_get(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("system/cache_concurrent_set_get");
    for count in [10, 100, 500] {
        group.throughput(Throughput::Elements(count as u64 * 2)); // set + get per entry
        group.bench_with_input(BenchmarkId::from_parameter(count), &count, |b, &n| {
            b.iter(|| {
                rt.block_on(async {
                    let cache = Arc::new(ResponseCache::new(Duration::from_secs(300)));
                    let mut handles = Vec::with_capacity(n * 2);

                    // Spawn concurrent setters
                    for i in 0..n {
                        let cache = Arc::clone(&cache);
                        handles.push(tokio::spawn(async move {
                            cache.set(&make_request(i), make_response(i)).await;
                        }));
                    }

                    // Spawn concurrent getters (some will hit, some will miss)
                    for i in 0..n {
                        let cache = Arc::clone(&cache);
                        handles.push(tokio::spawn(async move {
                            black_box(cache.get(&make_request(i)).await);
                        }));
                    }

                    for h in handles {
                        h.await.unwrap();
                    }
                });
            });
        });
    }
    group.finish();
}

// ---------------------------------------------------------------------------
// 2. Cache hit vs miss: measure hit ratio impact on response time
// ---------------------------------------------------------------------------

fn bench_cache_hit_vs_miss(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    // Pre-populate cache with entries 0..100
    let cache = rt.block_on(async {
        let cache = ResponseCache::new(Duration::from_secs(300));
        for i in 0..100 {
            cache.set(&make_request(i), make_response(i)).await;
        }
        cache
    });

    let mut group = c.benchmark_group("system/cache_hit_vs_miss");

    // 100% hit rate: look up entries 0..100
    group.bench_function("hit_rate_100pct", |b| {
        b.iter(|| {
            rt.block_on(async {
                for i in 0..100 {
                    let result = cache.get(&make_request(i)).await;
                    black_box(result);
                }
            });
        });
    });

    // 50% hit rate: look up 0..100 (hits) interleaved with 1000..1100 (misses)
    group.bench_function("hit_rate_50pct", |b| {
        b.iter(|| {
            rt.block_on(async {
                for i in 0..100 {
                    // hit
                    let result = cache.get(&make_request(i)).await;
                    black_box(result);
                    // miss
                    let result = cache.get(&make_request(i + 1000)).await;
                    black_box(result);
                }
            });
        });
    });

    // 0% hit rate: all misses
    group.bench_function("hit_rate_0pct", |b| {
        b.iter(|| {
            rt.block_on(async {
                for i in 2000..2100 {
                    let result = cache.get(&make_request(i)).await;
                    black_box(result);
                }
            });
        });
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// 3. Token accounting throughput: concurrent agent usage recording
// ---------------------------------------------------------------------------

fn bench_accounting_record_usage(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("system/accounting_record_usage");
    for agent_count in [1, 10, 50] {
        group.throughput(Throughput::Elements(agent_count as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(agent_count),
            &agent_count,
            |b, &n| {
                let agents: Vec<AgentId> = (0..n).map(|_| AgentId::new()).collect();

                b.iter(|| {
                    rt.block_on(async {
                        let accounting = TokenAccounting::new();
                        for agent_id in &agents {
                            accounting.record_usage(*agent_id, make_usage()).await;
                        }
                        let stats = accounting.stats().await;
                        black_box(stats);
                    });
                });
            },
        );
    }
    group.finish();
}

fn bench_accounting_concurrent_recording(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("system/accounting_concurrent_recording");
    for agent_count in [1, 10, 50] {
        group.throughput(Throughput::Elements(agent_count as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(agent_count),
            &agent_count,
            |b, &n| {
                let agents: Vec<AgentId> = (0..n).map(|_| AgentId::new()).collect();

                b.iter(|| {
                    rt.block_on(async {
                        let accounting = Arc::new(TokenAccounting::new());
                        let mut handles = Vec::with_capacity(n);

                        for agent_id in &agents {
                            let accounting = Arc::clone(&accounting);
                            let agent_id = *agent_id;
                            handles.push(tokio::spawn(async move {
                                accounting.record_usage(agent_id, make_usage()).await;
                            }));
                        }

                        for h in handles {
                            h.await.unwrap();
                        }

                        let total = accounting.get_total_usage().await;
                        black_box(total);
                    });
                });
            },
        );
    }
    group.finish();
}

fn bench_accounting_read_after_write(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("system/accounting_read_after_write");
    for agent_count in [1, 10, 50] {
        group.throughput(Throughput::Elements(agent_count as u64 * 2)); // write + read per agent
        group.bench_with_input(
            BenchmarkId::from_parameter(agent_count),
            &agent_count,
            |b, &n| {
                let agents: Vec<AgentId> = (0..n).map(|_| AgentId::new()).collect();

                b.iter(|| {
                    rt.block_on(async {
                        let accounting = TokenAccounting::new();
                        for agent_id in &agents {
                            accounting.record_usage(*agent_id, make_usage()).await;
                            let usage = accounting.get_usage(*agent_id).await;
                            black_box(usage);
                        }
                    });
                });
            },
        );
    }
    group.finish();
}

// ---------------------------------------------------------------------------
// 4. Provider selection: measure provider health-check + fallback overhead
//
// We benchmark the ProviderHealth state machine and the overhead of tracking
// multiple providers, since select_providers_ordered is private in main.rs.
// ---------------------------------------------------------------------------

fn bench_provider_health_tracking(c: &mut Criterion) {
    use std::collections::HashMap;
    use llm_gateway::providers::ProviderType;

    // Simulate the ProviderHealth struct inline (it lives in main.rs, not lib).
    // We measure the cost of HashMap lookups + mutation for health tracking.
    #[derive(Clone)]
    struct ProviderHealth {
        is_healthy: bool,
        consecutive_failures: u32,
    }

    impl ProviderHealth {
        fn new() -> Self {
            Self {
                is_healthy: true,
                consecutive_failures: 0,
            }
        }

        fn record_failure(&mut self) {
            self.consecutive_failures += 1;
            if self.consecutive_failures >= 3 {
                self.is_healthy = false;
            }
        }

        fn record_success(&mut self) {
            self.consecutive_failures = 0;
            self.is_healthy = true;
        }
    }

    let provider_types = [
        ProviderType::Ollama,
        ProviderType::LlamaCpp,
        ProviderType::OpenAi,
        ProviderType::Anthropic,
        ProviderType::Google,
    ];

    let mut group = c.benchmark_group("system/provider_health_tracking");

    // Benchmark: classify healthy vs unhealthy from a pool of 5 providers
    group.bench_function("classify_5_providers", |b| {
        let mut health: HashMap<ProviderType, ProviderHealth> = HashMap::new();
        for &pt in &provider_types {
            health.insert(pt, ProviderHealth::new());
        }
        // Mark two as unhealthy
        health.get_mut(&ProviderType::LlamaCpp).unwrap().record_failure();
        health.get_mut(&ProviderType::LlamaCpp).unwrap().record_failure();
        health.get_mut(&ProviderType::LlamaCpp).unwrap().record_failure();
        health.get_mut(&ProviderType::Google).unwrap().record_failure();
        health.get_mut(&ProviderType::Google).unwrap().record_failure();
        health.get_mut(&ProviderType::Google).unwrap().record_failure();

        b.iter(|| {
            let mut healthy = Vec::new();
            let mut unhealthy = Vec::new();
            for &pt in &provider_types {
                let h = health.get(&pt).unwrap();
                if h.is_healthy {
                    healthy.push(pt);
                } else {
                    unhealthy.push(pt);
                }
            }
            healthy.extend(unhealthy);
            black_box(healthy);
        });
    });

    // Benchmark: failure recording + recovery cycle
    group.bench_function("failure_recovery_cycle", |b| {
        b.iter(|| {
            let mut h = ProviderHealth::new();
            // 3 failures -> unhealthy
            h.record_failure();
            h.record_failure();
            h.record_failure();
            assert!(!h.is_healthy);
            // 1 success -> healthy again
            h.record_success();
            assert!(h.is_healthy);
            black_box(h);
        });
    });

    // Benchmark: iterate + select from 5 providers with mixed health
    group.bench_function("select_fallback_candidates", |b| {
        let mut health: HashMap<ProviderType, ProviderHealth> = HashMap::new();
        for (i, &pt) in provider_types.iter().enumerate() {
            let mut ph = ProviderHealth::new();
            // Odd-indexed providers are unhealthy
            if i % 2 == 1 {
                for _ in 0..3 {
                    ph.record_failure();
                }
            }
            health.insert(pt, ph);
        }

        b.iter(|| {
            let mut candidates: Vec<(ProviderType, bool)> = provider_types
                .iter()
                .map(|&pt| {
                    let is_healthy = health.get(&pt).map(|h| h.is_healthy).unwrap_or(true);
                    (pt, is_healthy)
                })
                .collect();
            // Sort healthy first (stable sort preserves order within groups)
            candidates.sort_by_key(|&(_, healthy)| !healthy);
            black_box(candidates);
        });
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// 5. End-to-end inference pipeline (mock): validate -> cache -> accounting -> response
// ---------------------------------------------------------------------------

fn bench_inference_pipeline_mock(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("system/inference_pipeline_cache_miss", |b| {
        b.iter(|| {
            rt.block_on(async {
                let cache = ResponseCache::new(Duration::from_secs(300));
                let accounting = TokenAccounting::new();
                let agent_id = AgentId::new();

                // Step 1: Build and validate request
                let mut request = InferenceRequest {
                    prompt: "What is the meaning of life?".to_string(),
                    model: "bench-model".to_string(),
                    max_tokens: 256,
                    temperature: 0.8,
                    top_p: 0.95,
                    presence_penalty: 0.0,
                    frequency_penalty: 0.0,
                };
                request.validate();

                // Step 2: Cache check (miss)
                let cached = cache.get(&request).await;
                assert!(cached.is_none());

                // Step 3: Simulate provider response (no actual LLM call)
                let response = InferenceResponse {
                    text: "The meaning of life is a deep philosophical question.".to_string(),
                    tokens_generated: 12,
                    finish_reason: FinishReason::Stop,
                    model: "bench-model".to_string(),
                    usage: TokenUsage {
                        prompt_tokens: 8,
                        completion_tokens: 12,
                        total_tokens: 20,
                    },
                };

                // Step 4: Record accounting
                accounting.record_usage(agent_id, response.usage).await;

                // Step 5: Cache the response
                cache.set(&request, response.clone()).await;

                black_box(response);
            });
        });
    });

    c.bench_function("system/inference_pipeline_cache_hit", |b| {
        // Pre-populate cache
        let cache = rt.block_on(async {
            let cache = ResponseCache::new(Duration::from_secs(300));
            let request = InferenceRequest {
                prompt: "What is the meaning of life?".to_string(),
                model: "bench-model".to_string(),
                max_tokens: 256,
                temperature: 0.8,
                top_p: 0.95,
                presence_penalty: 0.0,
                frequency_penalty: 0.0,
            };
            let response = InferenceResponse {
                text: "The meaning of life is a deep philosophical question.".to_string(),
                tokens_generated: 12,
                finish_reason: FinishReason::Stop,
                model: "bench-model".to_string(),
                usage: TokenUsage {
                    prompt_tokens: 8,
                    completion_tokens: 12,
                    total_tokens: 20,
                },
            };
            cache.set(&request, response).await;
            cache
        });

        b.iter(|| {
            rt.block_on(async {
                let mut request = InferenceRequest {
                    prompt: "What is the meaning of life?".to_string(),
                    model: "bench-model".to_string(),
                    max_tokens: 256,
                    temperature: 0.8,
                    top_p: 0.95,
                    presence_penalty: 0.0,
                    frequency_penalty: 0.0,
                };
                request.validate();

                // Cache hit — skip provider call entirely
                let cached = cache.get(&request).await;
                assert!(cached.is_some());
                black_box(cached);
            });
        });
    });

    // Pipeline with multiple sequential requests (batch-like)
    let mut group = c.benchmark_group("system/inference_pipeline_batch");
    for batch_size in [1, 10, 50] {
        group.throughput(Throughput::Elements(batch_size as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(batch_size),
            &batch_size,
            |b, &n| {
                b.iter(|| {
                    rt.block_on(async {
                        let cache = ResponseCache::new(Duration::from_secs(300));
                        let accounting = TokenAccounting::new();
                        let agent_id = AgentId::new();

                        for i in 0..n {
                            let mut request = make_request(i);
                            request.validate();

                            // Cache miss on first pass
                            let cached = cache.get(&request).await;
                            let response = if let Some(resp) = cached {
                                resp
                            } else {
                                let resp = make_response(i);
                                cache.set(&request, resp.clone()).await;
                                resp
                            };

                            accounting.record_usage(agent_id, response.usage).await;
                            black_box(&response);
                        }
                    });
                });
            },
        );
    }
    group.finish();
}

// ---------------------------------------------------------------------------
// 6. Cache expiry cleanup: measure cleanup performance at varying sizes
// ---------------------------------------------------------------------------

fn bench_cache_expiry_cleanup(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("system/cache_expiry_cleanup");
    for size in [100, 500, 1000] {
        group.throughput(Throughput::Elements(size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, &n| {
            b.iter(|| {
                rt.block_on(async {
                    // Use a tiny TTL so entries expire quickly
                    let cache = ResponseCache::new(Duration::from_millis(1));

                    // Fill cache with n entries
                    for i in 0..n {
                        cache.set(&make_request(i), make_response(i)).await;
                    }

                    // Wait for entries to expire
                    tokio::time::sleep(Duration::from_millis(5)).await;

                    // Trigger cleanup by checking stats (reads expired entries)
                    let stats = cache.stats().await;
                    black_box(stats);

                    // Force cleanup: clear reclaims memory
                    cache.clear().await;
                });
            });
        });
    }
    group.finish();
}

fn bench_cache_mixed_expiry(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("system/cache_mixed_expiry_half_expired", |b| {
        b.iter(|| {
            rt.block_on(async {
                // Short TTL cache
                let cache = ResponseCache::new(Duration::from_millis(10));

                // Insert first half (will expire)
                for i in 0..250 {
                    cache.set(&make_request(i), make_response(i)).await;
                }

                // Wait for those to expire
                tokio::time::sleep(Duration::from_millis(15)).await;

                // Insert second half (still fresh)
                let cache2 = ResponseCache::new(Duration::from_secs(300));
                for i in 250..500 {
                    cache2.set(&make_request(i), make_response(i)).await;
                }

                // Stats on mixed cache (the short-TTL one)
                let stats = cache.stats().await;
                black_box(stats);

                // Stats on fresh cache
                let stats2 = cache2.stats().await;
                black_box(stats2);
            });
        });
    });
}

// ---------------------------------------------------------------------------
// Groups
// ---------------------------------------------------------------------------

criterion_group!(
    cache_throughput_benches,
    bench_cache_set_throughput,
    bench_cache_get_throughput,
    bench_cache_concurrent_set_get
);

criterion_group!(
    cache_hit_miss_benches,
    bench_cache_hit_vs_miss
);

criterion_group!(
    accounting_benches,
    bench_accounting_record_usage,
    bench_accounting_concurrent_recording,
    bench_accounting_read_after_write
);

criterion_group!(
    provider_benches,
    bench_provider_health_tracking
);

criterion_group!(
    pipeline_benches,
    bench_inference_pipeline_mock
);

criterion_group!(
    cleanup_benches,
    bench_cache_expiry_cleanup,
    bench_cache_mixed_expiry
);

criterion_main!(
    cache_throughput_benches,
    cache_hit_miss_benches,
    accounting_benches,
    provider_benches,
    pipeline_benches,
    cleanup_benches
);
