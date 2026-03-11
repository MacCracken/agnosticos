// SPDX-License-Identifier: GPL-3.0
//! Vector search scaling benchmarks for the AGNOS agent-runtime.
//!
//! Measures vector insert and search performance across different index sizes
//! (100, 1K, 10K) and embedding dimensions (128-dim, 384-dim).

use agent_runtime::vector_store::{VectorEntry, VectorIndex};
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Generate a deterministic embedding vector of the given dimensionality.
fn make_embedding(dim: usize, seed: usize) -> Vec<f64> {
    (0..dim)
        .map(|j| ((seed * dim + j) as f64 * 0.0137).sin())
        .collect()
}

/// Build a pre-populated VectorIndex with `count` entries of `dim` dimensions.
fn build_index(count: usize, dim: usize) -> VectorIndex {
    let mut index = VectorIndex::new();
    for i in 0..count {
        let entry = VectorEntry {
            id: uuid::Uuid::new_v4(),
            embedding: make_embedding(dim, i),
            metadata: serde_json::json!({"i": i}),
            content: format!("Document {} (dim={})", i, dim),
            created_at: chrono::Utc::now(),
        };
        index.insert(entry).unwrap();
    }
    index
}

/// Build a query vector for the given dimensionality.
fn make_query(dim: usize) -> Vec<f64> {
    (0..dim).map(|j| (j as f64 * 0.042).cos()).collect()
}

// ---------------------------------------------------------------------------
// 1. Search latency at different scales and dimensions
// ---------------------------------------------------------------------------

fn bench_search_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("vector_search/search_top10");

    for &dim in &[128, 384] {
        for &count in &[100, 1_000, 10_000] {
            let index = build_index(count, dim);
            let query = make_query(dim);

            let label = format!("{}d_{}docs", dim, count);
            group.bench_with_input(BenchmarkId::from_parameter(&label), &(), |b, _| {
                b.iter(|| black_box(index.search(&query, 10)));
            });
        }
    }
    group.finish();
}

fn bench_search_top_k_variation(c: &mut Criterion) {
    let dim = 128;
    let count = 1_000;
    let index = build_index(count, dim);
    let query = make_query(dim);

    let mut group = c.benchmark_group("vector_search/top_k_variation_1k_128d");
    for &k in &[1, 5, 10, 50, 100] {
        group.bench_with_input(BenchmarkId::from_parameter(k), &k, |b, &k| {
            b.iter(|| black_box(index.search(&query, k)));
        });
    }
    group.finish();
}

// ---------------------------------------------------------------------------
// 2. Insert throughput at different scales
// ---------------------------------------------------------------------------

fn bench_insert_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("vector_search/insert");

    for &dim in &[128, 384] {
        for &count in &[100, 1_000] {
            let label = format!("{}d_{}entries", dim, count);
            group.throughput(Throughput::Elements(count as u64));
            group.bench_with_input(BenchmarkId::from_parameter(&label), &(), |b, _| {
                b.iter(|| {
                    let mut index = VectorIndex::new();
                    for i in 0..count {
                        let entry = VectorEntry {
                            id: uuid::Uuid::new_v4(),
                            embedding: make_embedding(dim, i),
                            metadata: serde_json::json!({"i": i}),
                            content: format!("Doc {}", i),
                            created_at: chrono::Utc::now(),
                        };
                        index.insert(entry).unwrap();
                    }
                    black_box(&index);
                });
            });
        }
    }
    group.finish();
}

// ---------------------------------------------------------------------------
// 3. Single insert into pre-populated index (amortized cost)
// ---------------------------------------------------------------------------

fn bench_insert_single(c: &mut Criterion) {
    let mut group = c.benchmark_group("vector_search/insert_single");

    for &dim in &[128, 384] {
        for &pre_count in &[100, 1_000, 10_000] {
            let mut index = build_index(pre_count, dim);
            let label = format!("{}d_into_{}docs", dim, pre_count);
            group.bench_with_input(BenchmarkId::from_parameter(&label), &(), |b, _| {
                b.iter(|| {
                    let entry = VectorEntry {
                        id: uuid::Uuid::new_v4(),
                        embedding: make_embedding(dim, pre_count + 1),
                        metadata: serde_json::json!({"i": "new"}),
                        content: "New document".to_string(),
                        created_at: chrono::Utc::now(),
                    };
                    black_box(index.insert(entry).unwrap());
                });
            });
        }
    }
    group.finish();
}

// ---------------------------------------------------------------------------
// 4. Combined insert + search workflow
// ---------------------------------------------------------------------------

fn bench_insert_then_search(c: &mut Criterion) {
    let mut group = c.benchmark_group("vector_search/insert_then_search");

    for &dim in &[128, 384] {
        let count = 500;
        let label = format!("{}d_{}docs", dim, count);
        group.throughput(Throughput::Elements(count as u64));
        group.bench_with_input(BenchmarkId::from_parameter(&label), &(), |b, _| {
            b.iter(|| {
                let mut index = VectorIndex::new();
                for i in 0..count {
                    let entry = VectorEntry {
                        id: uuid::Uuid::new_v4(),
                        embedding: make_embedding(dim, i),
                        metadata: serde_json::json!({"i": i}),
                        content: format!("Doc {}", i),
                        created_at: chrono::Utc::now(),
                    };
                    index.insert(entry).unwrap();
                }
                let query = make_query(dim);
                black_box(index.search(&query, 10));
            });
        });
    }
    group.finish();
}

// ---------------------------------------------------------------------------
// Groups
// ---------------------------------------------------------------------------

criterion_group!(
    vector_search,
    bench_search_scaling,
    bench_search_top_k_variation,
    bench_insert_throughput,
    bench_insert_single,
    bench_insert_then_search,
);
criterion_main!(vector_search);
