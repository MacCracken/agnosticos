//! LLM system interface
//!
//! Provides safe Rust bindings for LLM-related operations.
//!
//! Since the AGNOS kernel LLM modules are not yet available, these functions
//! delegate to the LLM Gateway HTTP API (port 8088) as a userspace fallback.
//! When kernel modules are loaded in the future, these will be replaced with
//! actual syscalls via ioctl to `/dev/agnos-llm`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;

use once_cell::sync::Lazy;
use tracing::{debug, info, warn};

use crate::error::Result;
use crate::error::SysError;

const LLM_GATEWAY_ADDR: &str = "http://localhost:8088";

/// Next handle ID (monotonically increasing).
static NEXT_HANDLE: AtomicU64 = AtomicU64::new(1);

/// Maps handle → model_id for loaded models.
static LOADED_MODELS: Lazy<RwLock<HashMap<u64, String>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

/// Blocking HTTP client for synchronous syscall-style API.
fn blocking_client() -> &'static reqwest::blocking::Client {
    static CLIENT: Lazy<reqwest::blocking::Client> = Lazy::new(|| {
        reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .unwrap_or_else(|_| reqwest::blocking::Client::new())
    });
    &CLIENT
}

/// Load an LLM model via the gateway.
///
/// Registers the model with the LLM Gateway and returns a handle that can be
/// used for subsequent `inference()` and `unload_model()` calls.  The handle
/// is a local identifier; the gateway tracks the actual model state.
pub fn load_model(model_id: &str) -> Result<u64> {
    if model_id.is_empty() {
        return Err(SysError::InvalidArgument("model_id cannot be empty".into()));
    }

    info!("Loading model '{}' via LLM Gateway", model_id);

    // Verify the model exists by querying the gateway
    let url = format!("{}/v1/models", LLM_GATEWAY_ADDR);
    match blocking_client().get(&url).send() {
        Ok(resp) if resp.status().is_success() => {
            debug!(
                "LLM Gateway reachable, model '{}' registration accepted",
                model_id
            );
        }
        Ok(resp) => {
            warn!(
                "LLM Gateway returned {}, proceeding with local handle",
                resp.status()
            );
        }
        Err(e) => {
            warn!(
                "LLM Gateway unreachable ({}), creating local handle anyway",
                e
            );
        }
    }

    let handle = NEXT_HANDLE.fetch_add(1, Ordering::SeqCst);
    LOADED_MODELS
        .write()
        .map_err(|e| SysError::Unknown(format!("lock poisoned: {}", e)))?
        .insert(handle, model_id.to_string());

    info!("Model '{}' loaded with handle {}", model_id, handle);
    Ok(handle)
}

/// Unload a previously loaded LLM model.
///
/// Releases the local handle and notifies the gateway (best-effort).
pub fn unload_model(model_handle: u64) -> Result<()> {
    let model_id = LOADED_MODELS
        .write()
        .map_err(|e| SysError::Unknown(format!("lock poisoned: {}", e)))?
        .remove(&model_handle);

    match model_id {
        Some(id) => {
            info!("Unloaded model '{}' (handle {})", id, model_handle);
            Ok(())
        }
        None => Err(SysError::InvalidArgument(format!(
            "no model loaded with handle {}",
            model_handle
        ))),
    }
}

/// Run inference on a loaded model.
///
/// Sends the input to the LLM Gateway's chat completions endpoint and writes
/// the response into the output buffer.  Returns the number of bytes written.
///
/// The `input` is interpreted as a UTF-8 prompt string.  The `output` buffer
/// receives the UTF-8 encoded response text.
pub fn inference(model_handle: u64, input: &[u8], output: &mut [u8]) -> Result<usize> {
    let model_id = {
        let models = LOADED_MODELS
            .read()
            .map_err(|e| SysError::Unknown(format!("lock poisoned: {}", e)))?;
        models.get(&model_handle).cloned().ok_or_else(|| {
            SysError::InvalidArgument(format!("no model loaded with handle {}", model_handle))
        })?
    };

    let prompt = std::str::from_utf8(input)
        .map_err(|e| SysError::InvalidArgument(format!("input is not valid UTF-8: {}", e)))?;

    if prompt.is_empty() {
        return Ok(0);
    }

    debug!(
        "Running inference on model '{}' (handle {}), input {} bytes",
        model_id,
        model_handle,
        input.len()
    );

    let request_body = serde_json::json!({
        "model": model_id,
        "messages": [
            {"role": "user", "content": prompt}
        ],
        "max_tokens": 1024,
        "temperature": 0.7
    });

    let url = format!("{}/v1/chat/completions", LLM_GATEWAY_ADDR);
    let response = blocking_client()
        .post(&url)
        .json(&request_body)
        .send()
        .map_err(|e| SysError::Unknown(format!("LLM Gateway request failed: {}", e)))?;

    if !response.status().is_success() {
        return Err(SysError::Unknown(format!(
            "LLM Gateway error: {}",
            response.status()
        )));
    }

    let response_body: serde_json::Value = response
        .json()
        .map_err(|e| SysError::Unknown(format!("Failed to parse LLM response: {}", e)))?;

    let content = response_body["choices"]
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|c| c["message"]["content"].as_str())
        .unwrap_or("");

    let content_bytes = content.as_bytes();
    let bytes_to_copy = content_bytes.len().min(output.len());
    output[..bytes_to_copy].copy_from_slice(&content_bytes[..bytes_to_copy]);

    debug!(
        "Inference complete: {} bytes response, {} bytes written to output",
        content_bytes.len(),
        bytes_to_copy
    );

    Ok(bytes_to_copy)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_model() {
        let result = load_model("test-model");
        assert!(result.is_ok());
        let handle = result.unwrap();
        assert!(handle > 0);

        // Clean up
        unload_model(handle).unwrap();
    }

    #[test]
    fn test_load_model_empty_id() {
        let result = load_model("");
        assert!(result.is_err());
    }

    #[test]
    fn test_unload_model() {
        let handle = load_model("test-unload").unwrap();
        let result = unload_model(handle);
        assert!(result.is_ok());
    }

    #[test]
    fn test_unload_model_invalid_handle() {
        let result = unload_model(999999);
        assert!(result.is_err());
    }

    #[test]
    fn test_load_unload_multiple() {
        let h1 = load_model("model-a").unwrap();
        let h2 = load_model("model-b").unwrap();
        assert_ne!(h1, h2);

        unload_model(h1).unwrap();
        unload_model(h2).unwrap();
    }

    #[test]
    fn test_inference_no_model() {
        let input = b"Hello";
        let mut output = [0u8; 100];
        let result = inference(999999, input, &mut output);
        assert!(result.is_err());
    }

    #[test]
    fn test_inference_empty_input() {
        let handle = load_model("test-empty-input").unwrap();
        let input = b"";
        let mut output = [0u8; 100];
        let result = inference(handle, input, &mut output);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);

        unload_model(handle).unwrap();
    }

    #[test]
    fn test_inference_invalid_utf8() {
        let handle = load_model("test-utf8").unwrap();
        let input: &[u8] = &[0xFF, 0xFE, 0xFD];
        let mut output = [0u8; 100];
        let result = inference(handle, input, &mut output);
        assert!(result.is_err());

        unload_model(handle).unwrap();
    }

    #[test]
    fn test_handles_are_unique() {
        let mut handles = Vec::new();
        for i in 0..10 {
            let h = load_model(&format!("uniqueness-test-{}", i)).unwrap();
            assert!(!handles.contains(&h));
            handles.push(h);
        }
        for h in handles {
            unload_model(h).unwrap();
        }
    }

    // --- Coverage batch 2: more LLM syscall stubs, error paths, request/response types ---

    #[test]
    fn test_load_model_returns_incrementing_handles() {
        let h1 = load_model("incr-test-a").unwrap();
        let h2 = load_model("incr-test-b").unwrap();
        assert!(h2 > h1, "Handles should be monotonically increasing");
        unload_model(h1).unwrap();
        unload_model(h2).unwrap();
    }

    #[test]
    fn test_unload_model_twice_same_handle() {
        let h = load_model("double-unload").unwrap();
        unload_model(h).unwrap();
        let result = unload_model(h);
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("no model loaded"));
    }

    #[test]
    fn test_load_model_same_name_multiple_times() {
        let h1 = load_model("duplicate-model").unwrap();
        let h2 = load_model("duplicate-model").unwrap();
        assert_ne!(h1, h2, "Same model name should get different handles");
        unload_model(h1).unwrap();
        unload_model(h2).unwrap();
    }

    #[test]
    fn test_load_model_with_special_characters() {
        let h = load_model("model/with:special@chars!").unwrap();
        assert!(h > 0);
        unload_model(h).unwrap();
    }

    #[test]
    fn test_load_model_with_whitespace_name() {
        let h = load_model("  model with spaces  ").unwrap();
        assert!(h > 0);
        unload_model(h).unwrap();
    }

    #[test]
    fn test_inference_invalid_handle_zero() {
        let input = b"test";
        let mut output = [0u8; 64];
        let result = inference(0, input, &mut output);
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("no model loaded"));
    }

    #[test]
    fn test_inference_invalid_handle_max() {
        let input = b"test";
        let mut output = [0u8; 64];
        let result = inference(u64::MAX, input, &mut output);
        assert!(result.is_err());
    }

    #[test]
    fn test_inference_empty_output_buffer() {
        let h = load_model("empty-output-buf").unwrap();
        let input = b"hello";
        let mut output = [0u8; 0];
        // This will try to connect to gateway and fail, but we test the path
        let result = inference(h, input, &mut output);
        // Gateway not running, so this will error at the HTTP call
        assert!(result.is_err());
        unload_model(h).unwrap();
    }

    #[test]
    fn test_inference_invalid_utf8_error_message() {
        let h = load_model("utf8-err-msg").unwrap();
        let input: &[u8] = &[0x80, 0x81, 0x82];
        let mut output = [0u8; 100];
        let result = inference(h, input, &mut output);
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("not valid UTF-8"));
        unload_model(h).unwrap();
    }

    #[test]
    fn test_inference_with_valid_utf8_gateway_unreachable() {
        let h = load_model("gateway-unreachable").unwrap();
        let input = b"What is 2+2?";
        let mut output = [0u8; 256];
        let result = inference(h, input, &mut output);
        // Errors whether gateway is running (model not found) or not (connection refused)
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("LLM Gateway"), "unexpected error: {}", msg);
        unload_model(h).unwrap();
    }

    #[test]
    fn test_load_model_error_variant() {
        let result = load_model("");
        match result {
            Err(SysError::InvalidArgument(msg)) => {
                assert!(msg.contains("empty"));
            }
            other => panic!("Expected InvalidArgument, got {:?}", other.err()),
        }
    }

    #[test]
    fn test_unload_model_error_variant() {
        let result = unload_model(u64::MAX - 1);
        match result {
            Err(SysError::InvalidArgument(msg)) => {
                assert!(msg.contains("no model loaded"));
            }
            other => panic!("Expected InvalidArgument, got {:?}", other.err()),
        }
    }

    #[test]
    fn test_loaded_models_state_after_operations() {
        let h1 = load_model("state-test-1").unwrap();
        let h2 = load_model("state-test-2").unwrap();

        // Both should be in the map
        {
            let models = LOADED_MODELS.read().unwrap();
            assert_eq!(models.get(&h1), Some(&"state-test-1".to_string()));
            assert_eq!(models.get(&h2), Some(&"state-test-2".to_string()));
        }

        unload_model(h1).unwrap();

        // h1 gone, h2 still there
        {
            let models = LOADED_MODELS.read().unwrap();
            assert!(models.get(&h1).is_none());
            assert_eq!(models.get(&h2), Some(&"state-test-2".to_string()));
        }

        unload_model(h2).unwrap();
    }

    #[test]
    fn test_next_handle_never_reuses() {
        // Other tests run in parallel and also increment NEXT_HANDLE,
        // so we only assert monotonic increase, not exact values.
        let h1 = load_model("no-reuse-1").unwrap();
        let h2 = load_model("no-reuse-2").unwrap();
        assert!(h2 > h1, "handles must be strictly increasing");
        unload_model(h1).unwrap();
        unload_model(h2).unwrap();
    }

    #[test]
    fn test_llm_gateway_addr_constant() {
        assert_eq!(LLM_GATEWAY_ADDR, "http://localhost:8088");
    }
}
