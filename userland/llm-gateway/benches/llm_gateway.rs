use agnos_common::{FinishReason, InferenceRequest, InferenceResponse, TokenUsage};
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::collections::HashMap;

fn benchmark_cache_key_generation(c: &mut Criterion) {
    let request = InferenceRequest {
        prompt: "Hello, how are you today? I hope you're having a great day!".to_string(),
        model: "llama2-7b".to_string(),
        max_tokens: 512,
        temperature: 0.7,
        top_p: 0.9,
        presence_penalty: 0.0,
        frequency_penalty: 0.0,
    };

    c.bench_function("cache_key_generation", |b| {
        b.iter(|| {
            let key = format!(
                "{}:{}:{:.2}:{:.2}:{}",
                black_box(&request.model),
                black_box(&request.prompt),
                black_box(request.temperature),
                black_box(request.top_p),
                black_box(request.max_tokens)
            );
            key
        });
    });
}

fn benchmark_hashmap_operations(c: &mut Criterion) {
    let mut map: HashMap<String, String> = HashMap::new();

    // Insert
    c.bench_function("hashmap_insert_100", |b| {
        b.iter(|| {
            let mut m: HashMap<String, String> = HashMap::new();
            for i in 0..100 {
                m.insert(format!("key{}", i), format!("value{}", i));
            }
        });
    });

    // Lookup
    for i in 0..100 {
        map.insert(format!("key{}", i), format!("value{}", i));
    }

    c.bench_function("hashmap_lookup_100", |b| {
        b.iter(|| {
            for i in 0..100 {
                black_box(map.get(&format!("key{}", i)));
            }
        });
    });
}

fn benchmark_json_parse(c: &mut Criterion) {
    let json_strings = vec![
        r#"{"prompt":"test","model":"llama2","max_tokens":512,"temperature":0.7}"#,
        r#"{"prompt":"hello world","model":"gpt-4","max_tokens":1000,"temperature":0.9}"#,
        r#"{"prompt":"a very long prompt that contains many words and should take more time to parse because it is longer than the others","model":"llama2","max_tokens":2048,"temperature":0.5}"#,
    ];

    c.bench_function("json_parse_small", |b| {
        b.iter(|| {
            for json in &json_strings {
                let _ = serde_json::from_str::<InferenceRequest>(black_box(*json));
            }
        });
    });
}

fn benchmark_token_usage(c: &mut Criterion) {
    c.bench_function("token_usage_default", |b| {
        b.iter(|| TokenUsage::default());
    });

    c.bench_function("token_usage_calculation", |b| {
        b.iter(|| {
            let mut usage = TokenUsage::default();
            usage.prompt_tokens = black_box(100);
            usage.completion_tokens = black_box(200);
            usage.total_tokens = usage.prompt_tokens + usage.completion_tokens;
            usage
        });
    });
}

fn benchmark_response_formatting(c: &mut Criterion) {
    let response = InferenceResponse {
        text: "This is a generated response from the LLM.".to_string(),
        tokens_generated: 25,
        finish_reason: FinishReason::Stop,
        model: "llama2".to_string(),
        usage: TokenUsage {
            prompt_tokens: 10,
            completion_tokens: 25,
            total_tokens: 35,
        },
    };

    c.bench_function("response_serialize", |b| {
        b.iter(|| serde_json::to_string(black_box(&response)));
    });
}

criterion_group!(
    benches,
    benchmark_cache_key_generation,
    benchmark_hashmap_operations,
    benchmark_json_parse,
    benchmark_token_usage,
    benchmark_response_formatting
);
criterion_main!(benches);
