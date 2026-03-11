// SPDX-License-Identifier: GPL-3.0
//! Intent parsing throughput benchmarks for AGNOS AI Shell.
//!
//! Measures how quickly the interpreter classifies diverse natural language
//! inputs into structured intents, covering common filesystem, process,
//! package, consumer-app, and edge/DAW domains.

use ai_shell::interpreter::Interpreter;
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};

// ---------------------------------------------------------------------------
// Inputs
// ---------------------------------------------------------------------------

/// Core intent inputs spanning multiple domains.
const INTENT_INPUTS: &[&str] = &[
    // Filesystem
    "show me all files",
    "find files named foo",
    // Package management
    "install package vim",
    // Process
    "show running processes",
    // Shruti (DAW)
    "mute track vocals",
    // Edge / federation
    "list edge nodes",
    // Shruti session
    "shruti create session test",
    // Marketplace
    "search marketplace for agnostic",
    // Knowledge / RAG
    "search knowledge base for networking",
    // Ark
    "ark install htop",
    // Delta
    "delta list repos",
    // Photis Nadi
    "list tasks",
    // Aequi
    "show balance",
    // Network scan
    "scan ports on 192.168.1.1",
    // System info
    "show system information",
];

// ---------------------------------------------------------------------------
// 1. Single-input parsing latency for each representative intent
// ---------------------------------------------------------------------------

fn bench_parse_individual_intents(c: &mut Criterion) {
    let interpreter = Interpreter::new();

    let mut group = c.benchmark_group("intent_parsing/individual");
    for input in INTENT_INPUTS {
        group.bench_with_input(BenchmarkId::from_parameter(input), input, |b, &input| {
            b.iter(|| black_box(interpreter.parse(input)));
        });
    }
    group.finish();
}

// ---------------------------------------------------------------------------
// 2. Batch throughput: parse all inputs in a loop
// ---------------------------------------------------------------------------

fn bench_parse_batch_throughput(c: &mut Criterion) {
    let interpreter = Interpreter::new();

    let mut group = c.benchmark_group("intent_parsing/batch");
    for count in [15, 100, 500] {
        group.throughput(Throughput::Elements(count as u64));
        group.bench_with_input(BenchmarkId::from_parameter(count), &count, |b, &n| {
            b.iter(|| {
                for i in 0..n {
                    let input = INTENT_INPUTS[i % INTENT_INPUTS.len()];
                    black_box(interpreter.parse(input));
                }
            });
        });
    }
    group.finish();
}

// ---------------------------------------------------------------------------
// 3. Parse + translate pipeline throughput
// ---------------------------------------------------------------------------

fn bench_parse_translate_pipeline(c: &mut Criterion) {
    let interpreter = Interpreter::new();

    let mut group = c.benchmark_group("intent_parsing/parse_translate");
    for count in [15, 100, 500] {
        group.throughput(Throughput::Elements(count as u64));
        group.bench_with_input(BenchmarkId::from_parameter(count), &count, |b, &n| {
            b.iter(|| {
                for i in 0..n {
                    let input = INTENT_INPUTS[i % INTENT_INPUTS.len()];
                    let intent = interpreter.parse(input);
                    let _ = black_box(interpreter.translate(&intent));
                }
            });
        });
    }
    group.finish();
}

// ---------------------------------------------------------------------------
// 4. Edge-case inputs: empty, whitespace, very long, gibberish
// ---------------------------------------------------------------------------

fn bench_parse_edge_cases(c: &mut Criterion) {
    let interpreter = Interpreter::new();

    let edge_cases: Vec<&str> = vec![
        "",
        "   ",
        "a",
        "this is a very long natural language input that does not match any known pattern whatsoever and should fall through to the unknown intent handler after checking every single matcher in the chain",
        "!@#$%^&*()",
        "show me all files; rm -rf /",
        "install package vim && install package htop",
    ];

    c.bench_function("intent_parsing/edge_cases", |b| {
        b.iter(|| {
            for input in &edge_cases {
                black_box(interpreter.parse(input));
            }
        });
    });
}

// ---------------------------------------------------------------------------
// Groups
// ---------------------------------------------------------------------------

criterion_group!(
    intent_parsing,
    bench_parse_individual_intents,
    bench_parse_batch_throughput,
    bench_parse_translate_pipeline,
    bench_parse_edge_cases,
);
criterion_main!(intent_parsing);
