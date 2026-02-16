#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(input) = std::str::from_utf8(data) {
        // Test various input patterns that might cause issues
        test_input_sanitization(input);
        test_intent_parsing(input);
    }
});

fn test_input_sanitization(input: &str) {
    // Test for injection patterns
    let dangerous = [
        "; rm -rf /",
        "| cat /etc/passwd",
        "`ls`",
        "$(whoami)",
        "\n--help",
        "\0null",
    ];

    for pat in dangerous {
        let combined = format!("{} {}", input, pat);
        let _ = sanitize_input(&combined);
    }
}

fn sanitize_input(input: &str) -> String {
    input
        .chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .collect()
}

fn test_intent_parsing(input: &str) {
    // Test various natural language patterns
    let _ = classify_intent(input);

    // Test short inputs
    if input.len() < 3 {
        let _ = classify_intent(input);
    }

    // Test very long inputs
    if input.len() > 1000 {
        let _ = classify_intent(input);
    }
}

fn classify_intent(input: &str) -> &'static str {
    let lower = input.to_lowercase();

    if lower.contains("create") && lower.contains("agent") {
        return "create_agent";
    }
    if lower.contains("list") || lower.contains("show") {
        return "list";
    }
    if lower.contains("delete") || lower.contains("remove") {
        return "delete";
    }
    if lower.contains("help") || lower.contains("?") {
        return "help";
    }

    "unknown"
}
