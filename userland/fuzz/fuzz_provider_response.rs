#![no_main]

use libfuzzer_sys::fuzz_target;

/// Fuzz parsing of LLM provider HTTP response JSON.
/// Exercises the extraction paths used by all providers
/// (Ollama, llama.cpp, OpenAI, Anthropic, Google) to ensure
/// malformed API responses never cause panics.
fuzz_target!(|data: &[u8]| {
    if let Ok(json_str) = std::str::from_utf8(data) {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(json_str) {
            // Ollama response path
            parse_ollama_response(&val);
            // llama.cpp response path
            parse_llamacpp_response(&val);
            // OpenAI response path
            parse_openai_response(&val);
            // Anthropic response path
            parse_anthropic_response(&val);
            // Google Gemini response path
            parse_google_response(&val);
        }
    }
});

fn parse_ollama_response(val: &serde_json::Value) {
    let _ = val["response"].as_str().unwrap_or("");
    let _ = val["done"].as_bool().unwrap_or(false);
    let _ = val["eval_count"].as_u64().unwrap_or(0);
    let _ = val["prompt_eval_count"].as_u64().unwrap_or(0);
}

fn parse_llamacpp_response(val: &serde_json::Value) {
    let _ = val["content"].as_str().unwrap_or("");
    let _ = val["stop"].as_bool().unwrap_or(false);
    let _ = val["tokens_predicted"].as_u64().unwrap_or(0);
    let _ = val["tokens_evaluated"].as_u64().unwrap_or(0);
}

fn parse_openai_response(val: &serde_json::Value) {
    let text = val["choices"]
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|c| c["message"]["content"].as_str())
        .unwrap_or("");
    let _ = text.len();
    let _ = val["usage"]["prompt_tokens"].as_u64().unwrap_or(0);
    let _ = val["usage"]["completion_tokens"].as_u64().unwrap_or(0);
    let _ = val["model"].as_str().unwrap_or("");
}

fn parse_anthropic_response(val: &serde_json::Value) {
    let text = val["content"]
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|c| c["text"].as_str())
        .unwrap_or("");
    let _ = text.len();
    let _ = val["stop_reason"].as_str();
    let _ = val["usage"]["input_tokens"].as_u64().unwrap_or(0);
    let _ = val["usage"]["output_tokens"].as_u64().unwrap_or(0);
}

fn parse_google_response(val: &serde_json::Value) {
    let text = val["candidates"]
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|c| c["content"]["parts"].as_array())
        .and_then(|parts| parts.first())
        .and_then(|p| p["text"].as_str())
        .unwrap_or("");
    let _ = text.len();
    let _ = val["candidates"]
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|c| c["finishReason"].as_str());
    let _ = val["usageMetadata"]["promptTokenCount"].as_u64().unwrap_or(0);
    let _ = val["usageMetadata"]["candidatesTokenCount"].as_u64().unwrap_or(0);
}
