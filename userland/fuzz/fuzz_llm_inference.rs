#![no_main]

use agnostik::InferenceRequest;
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
});

fn validate_request(request: &InferenceRequest) {
    let _ = request.model.len();
    let _ = request.prompt.len();
    if let Some(max) = request.max_tokens {
        let _ = max.min(100000);
    }
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
