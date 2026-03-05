#![no_main]

use agnos_common::InferenceRequest;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(json_str) = std::str::from_utf8(data) {
        // Try to parse as InferenceRequest
        if let Ok(mut request) = serde_json::from_str::<InferenceRequest>(json_str) {
            validate_request(&request);
            // Validate must not panic
            request.validate();
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
    let _ = request.temperature.clamp(0.0_f32, 2.0_f32);
    let _ = request.max_tokens.min(100000);
    let _ = request.top_p.clamp(0.0_f32, 1.0_f32);
}

fn test_builder_patterns(builder: &serde_json::Value) {
    if let Some(obj) = builder.as_object() {
        for (_key, value) in obj {
            let _ = format!("{:?}", value);
        }
    }
}

fn test_model_names() {
    let long_name = "a".repeat(1000);
    let model_names: &[&str] = &[
        "llama2",
        "gpt-4",
        "",
        &long_name,
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
    let temps: &[f32] = &[-1.0, 0.0, 0.5, 1.0, 1.5, 2.0, 2.5, 100.0];
    for &temp in temps {
        let _ = temp.clamp(0.0, 2.0);
    }

    let tokens: &[u32] = &[0, 1, 100, 1000, 10000, 100000];
    for &tok in tokens {
        let _ = tok.min(100000);
    }
}
