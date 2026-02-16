#![no_main]

use agnos_common::llm::{InferenceRequest, InferenceRequestBuilder};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(json_str) = std::str::from_utf8(data) {
        // Try to parse as InferenceRequest
        if let Ok(request) = serde_json::from_str::<InferenceRequest>(json_str) {
            validate_request(&request);
        }

        // Try various field combinations
        if let Ok(builder) = serde_json::from_str::<serde_json::Value>(json_str) {
            test_builder_patterns(&builder);
        }
    }

    // Test with various model names
    test_model_names();

    // Test parameter bounds
    test_parameter_bounds();
});

fn validate_request(request: &InferenceRequest) {
    // Validate temperature is in range
    if request.temperature < 0.0 || request.temperature > 2.0 {
        // Should be clamped
        let _ = request.temperature.clamp(0.0, 2.0);
    }

    // Validate max_tokens
    if request.max_tokens > 100000 {
        // Too large
    }

    // Validate top_p
    if request.top_p < 0.0 || request.top_p > 1.0 {
        let _ = request.top_p.clamp(0.0, 1.0);
    }
}

fn test_builder_patterns(builder: &serde_json::Value) {
    // Test various JSON structures
    if let Some(obj) = builder.as_object() {
        for (key, value) in obj {
            let _ = format!("{:?}", value);
        }
    }
}

fn test_model_names() {
    let model_names = [
        "llama2",
        "gpt-4",
        "",
        "a".repeat(1000).as_str(),
        "model with spaces",
        "model/with/slashes",
        "model\twith\ttabs",
    ];

    for name in model_names {
        let _ = validate_model_name(name);
    }
}

fn validate_model_name(name: &str) -> bool {
    !name.is_empty() && name.len() < 256 && !name.contains('\0') && !name.contains('\n')
}

fn test_parameter_bounds() {
    // Test temperature bounds
    let temps = [-1.0, 0.0, 0.5, 1.0, 1.5, 2.0, 2.5, 100.0];
    for temp in temps {
        let _ = temp.clamp(0.0, 2.0);
    }

    // Test max_tokens bounds
    let tokens = [0, 1, 100, 1000, 10000, 100000, usize::MAX];
    for tok in tokens {
        let _ = tok.min(100000);
    }
}
