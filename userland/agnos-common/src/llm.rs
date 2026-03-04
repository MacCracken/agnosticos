//! LLM-related types and structures

use serde::{Deserialize, Serialize};

/// LLM inference request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceRequest {
    pub prompt: String,
    pub model: String,
    pub max_tokens: u32,
    pub temperature: f32,
    pub top_p: f32,
    pub presence_penalty: f32,
    pub frequency_penalty: f32,
}

/// Maximum prompt length (256 KB) to prevent DoS via memory exhaustion.
pub const MAX_PROMPT_LENGTH: usize = 256 * 1024;
/// Maximum tokens that can be requested in a single inference call.
pub const MAX_TOKENS_LIMIT: u32 = 128_000;

impl Default for InferenceRequest {
    fn default() -> Self {
        Self {
            prompt: String::new(),
            model: "default".into(),
            max_tokens: 1024,
            temperature: 0.7,
            top_p: 1.0,
            presence_penalty: 0.0,
            frequency_penalty: 0.0,
        }
    }
}

impl InferenceRequest {
    /// Create a new request with validated parameters.
    ///
    /// This is the preferred constructor — it ensures all parameters are
    /// within safe ranges before the request can be used.
    pub fn new(prompt: String, model: String) -> Self {
        let mut req = Self {
            prompt,
            model,
            ..Default::default()
        };
        req.validate();
        req
    }

    /// Validate the request parameters, clamping values to safe ranges.
    ///
    /// - `temperature`: clamped to `[0.0, 2.0]`
    /// - `top_p`: clamped to `(0.0, 1.0]`
    /// - `max_tokens`: clamped to `[1, MAX_TOKENS_LIMIT]`
    /// - `prompt`: truncated to `MAX_PROMPT_LENGTH` bytes
    pub fn validate(&mut self) {
        self.temperature = self.temperature.clamp(0.0, 2.0);
        self.top_p = self.top_p.clamp(f32::MIN_POSITIVE, 1.0);
        self.max_tokens = self.max_tokens.clamp(1, MAX_TOKENS_LIMIT);
        self.presence_penalty = self.presence_penalty.clamp(-2.0, 2.0);
        self.frequency_penalty = self.frequency_penalty.clamp(-2.0, 2.0);
        if self.prompt.len() > MAX_PROMPT_LENGTH {
            self.prompt.truncate(MAX_PROMPT_LENGTH);
        }
    }
}

/// LLM inference response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceResponse {
    pub text: String,
    pub tokens_generated: u32,
    pub finish_reason: FinishReason,
    pub model: String,
    pub usage: TokenUsage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FinishReason {
    Stop,
    Length,
    ContentFilter,
    Error,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// Model information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub provider: Provider,
    pub capabilities: Vec<ModelCapability>,
    pub max_tokens: u32,
    pub size_bytes: u64,
    pub loaded: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Provider {
    Local,
    OpenAi,
    Anthropic,
    Google,
    Custom(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelCapability {
    TextGeneration,
    CodeGeneration,
    FunctionCalling,
    Vision,
    Embeddings,
}

/// LLM gateway configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub default_model: String,
    pub local_models_path: String,
    pub max_concurrent_requests: u32,
    pub request_timeout_seconds: u64,
    pub enable_cloud_fallback: bool,
    pub cloud_providers: Vec<CloudProviderConfig>,
}

/// Cloud provider configuration.
///
/// The `api_key` field is redacted in `Debug` output to prevent accidental
/// exposure in logs. Use `Serialize` with care — the key will appear in
/// serialized output (use `#[serde(skip)]` if persisting to untrusted storage).
#[derive(Clone, Serialize, Deserialize)]
pub struct CloudProviderConfig {
    pub name: String,
    pub api_key: String,
    pub base_url: String,
    pub priority: u32,
}

impl std::fmt::Debug for CloudProviderConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CloudProviderConfig")
            .field("name", &self.name)
            .field("api_key", &"[REDACTED]")
            .field("base_url", &self.base_url)
            .field("priority", &self.priority)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inference_request_default() {
        let req = InferenceRequest::default();
        assert_eq!(req.model, "default");
        assert_eq!(req.max_tokens, 1024);
        assert_eq!(req.temperature, 0.7);
    }

    #[test]
    fn test_inference_request_custom() {
        let req = InferenceRequest {
            prompt: "Hello".to_string(),
            model: "llama2".to_string(),
            max_tokens: 512,
            temperature: 0.5,
            top_p: 0.9,
            presence_penalty: 0.1,
            frequency_penalty: 0.2,
        };
        assert_eq!(req.prompt, "Hello");
        assert_eq!(req.temperature, 0.5);
    }

    #[test]
    fn test_token_usage_default() {
        let usage = TokenUsage::default();
        assert_eq!(usage.total_tokens, 0);
    }

    #[test]
    fn test_token_usage_calculation() {
        let mut usage = TokenUsage::default();
        usage.prompt_tokens = 100;
        usage.completion_tokens = 200;
        usage.total_tokens = usage.prompt_tokens + usage.completion_tokens;
        assert_eq!(usage.total_tokens, 300);
    }

    #[test]
    fn test_finish_reason_variants() {
        assert!(matches!(FinishReason::Stop, FinishReason::Stop));
        assert!(matches!(FinishReason::Length, FinishReason::Length));
        assert!(matches!(
            FinishReason::ContentFilter,
            FinishReason::ContentFilter
        ));
    }

    #[test]
    fn test_provider_variants() {
        assert_eq!(Provider::Local, Provider::Local);
        assert_eq!(Provider::OpenAi, Provider::OpenAi);
        assert_eq!(Provider::Anthropic, Provider::Anthropic);

        let custom = Provider::Custom("custom-provider".to_string());
        if let Provider::Custom(name) = custom {
            assert_eq!(name, "custom-provider");
        }
    }

    #[test]
    fn test_model_capability_variants() {
        assert!(matches!(
            ModelCapability::TextGeneration,
            ModelCapability::TextGeneration
        ));
        assert!(matches!(
            ModelCapability::CodeGeneration,
            ModelCapability::CodeGeneration
        ));
        assert!(matches!(ModelCapability::Vision, ModelCapability::Vision));
    }

    #[test]
    fn test_model_info() {
        let model = ModelInfo {
            id: "llama2-7b".to_string(),
            name: "Llama 2 7B".to_string(),
            provider: Provider::Local,
            capabilities: vec![
                ModelCapability::TextGeneration,
                ModelCapability::CodeGeneration,
            ],
            max_tokens: 4096,
            size_bytes: 3_800_000_000,
            loaded: false,
        };
        assert_eq!(model.id, "llama2-7b");
        assert!(!model.loaded);
    }

    #[test]
    fn test_llm_config() {
        let config = LlmConfig {
            default_model: "llama2".to_string(),
            local_models_path: "/var/lib/agnos/models".to_string(),
            max_concurrent_requests: 10,
            request_timeout_seconds: 60,
            enable_cloud_fallback: true,
            cloud_providers: vec![],
        };
        assert_eq!(config.max_concurrent_requests, 10);
        assert!(config.enable_cloud_fallback);
    }

    #[test]
    fn test_cloud_provider_config() {
        let provider = CloudProviderConfig {
            name: "openai".to_string(),
            api_key: "sk-xxx".to_string(),
            base_url: "https://api.openai.com".to_string(),
            priority: 1,
        };
        assert_eq!(provider.name, "openai");
        assert_eq!(provider.priority, 1);
    }

    // --- Additional llm.rs coverage tests ---

    #[test]
    fn test_inference_request_new_constructor() {
        let req = InferenceRequest::new("Hello world".to_string(), "gpt-4".to_string());
        assert_eq!(req.prompt, "Hello world");
        assert_eq!(req.model, "gpt-4");
        // Defaults should be applied via validate()
        assert_eq!(req.temperature, 0.7);
        assert_eq!(req.max_tokens, 1024);
    }

    #[test]
    fn test_inference_request_validate_clamps_temperature() {
        let mut req = InferenceRequest::default();
        req.temperature = 5.0;
        req.validate();
        assert_eq!(req.temperature, 2.0);

        req.temperature = -1.0;
        req.validate();
        assert_eq!(req.temperature, 0.0);
    }

    #[test]
    fn test_inference_request_validate_clamps_top_p() {
        let mut req = InferenceRequest::default();
        req.top_p = 0.0;
        req.validate();
        assert!(req.top_p > 0.0); // clamped to f32::MIN_POSITIVE

        req.top_p = 2.0;
        req.validate();
        assert_eq!(req.top_p, 1.0);
    }

    #[test]
    fn test_inference_request_validate_clamps_max_tokens() {
        let mut req = InferenceRequest::default();
        req.max_tokens = 0;
        req.validate();
        assert_eq!(req.max_tokens, 1);

        req.max_tokens = 999_999;
        req.validate();
        assert_eq!(req.max_tokens, MAX_TOKENS_LIMIT);
    }

    #[test]
    fn test_inference_request_validate_clamps_penalties() {
        let mut req = InferenceRequest::default();
        req.presence_penalty = -5.0;
        req.frequency_penalty = 5.0;
        req.validate();
        assert_eq!(req.presence_penalty, -2.0);
        assert_eq!(req.frequency_penalty, 2.0);
    }

    #[test]
    fn test_inference_request_validate_truncates_prompt() {
        let long_prompt = "A".repeat(MAX_PROMPT_LENGTH + 1000);
        let mut req = InferenceRequest {
            prompt: long_prompt,
            ..Default::default()
        };
        req.validate();
        assert_eq!(req.prompt.len(), MAX_PROMPT_LENGTH);
    }

    #[test]
    fn test_inference_request_serialization_roundtrip() {
        let req = InferenceRequest::new("test prompt".to_string(), "llama3".to_string());
        let json = serde_json::to_string(&req).unwrap();
        let deserialized: InferenceRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.prompt, "test prompt");
        assert_eq!(deserialized.model, "llama3");
    }

    #[test]
    fn test_inference_response_construction() {
        let resp = InferenceResponse {
            text: "Hello!".to_string(),
            tokens_generated: 5,
            finish_reason: FinishReason::Stop,
            model: "llama2-7b".to_string(),
            usage: TokenUsage {
                prompt_tokens: 10,
                completion_tokens: 5,
                total_tokens: 15,
            },
        };
        assert_eq!(resp.text, "Hello!");
        assert_eq!(resp.tokens_generated, 5);
        assert_eq!(resp.finish_reason, FinishReason::Stop);
        assert_eq!(resp.usage.total_tokens, 15);
    }

    #[test]
    fn test_inference_response_serialization() {
        let resp = InferenceResponse {
            text: "output".to_string(),
            tokens_generated: 3,
            finish_reason: FinishReason::Length,
            model: "test".to_string(),
            usage: TokenUsage {
                prompt_tokens: 50,
                completion_tokens: 3,
                total_tokens: 53,
            },
        };
        let json = serde_json::to_string(&resp).unwrap();
        let deserialized: InferenceResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.finish_reason, FinishReason::Length);
        assert_eq!(deserialized.usage.prompt_tokens, 50);
    }

    #[test]
    fn test_finish_reason_error_variant() {
        let reason = FinishReason::Error;
        assert_eq!(reason, FinishReason::Error);
        assert_ne!(reason, FinishReason::Stop);
    }

    #[test]
    fn test_model_info_with_capabilities() {
        let model = ModelInfo {
            id: "gpt-4-vision".to_string(),
            name: "GPT-4 Vision".to_string(),
            provider: Provider::OpenAi,
            capabilities: vec![
                ModelCapability::TextGeneration,
                ModelCapability::Vision,
                ModelCapability::FunctionCalling,
            ],
            max_tokens: 128_000,
            size_bytes: 0,
            loaded: true,
        };
        assert_eq!(model.capabilities.len(), 3);
        assert!(model.capabilities.contains(&ModelCapability::Vision));
        assert!(model.capabilities.contains(&ModelCapability::FunctionCalling));
        assert!(model.loaded);
    }

    #[test]
    fn test_model_info_serialization() {
        let model = ModelInfo {
            id: "test-model".to_string(),
            name: "Test Model".to_string(),
            provider: Provider::Custom("my-provider".to_string()),
            capabilities: vec![ModelCapability::Embeddings],
            max_tokens: 2048,
            size_bytes: 100_000,
            loaded: false,
        };
        let json = serde_json::to_string(&model).unwrap();
        let deserialized: ModelInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.provider, Provider::Custom("my-provider".to_string()));
        assert!(deserialized.capabilities.contains(&ModelCapability::Embeddings));
    }

    #[test]
    fn test_provider_google_variant() {
        let provider = Provider::Google;
        assert_eq!(provider, Provider::Google);
        assert_ne!(provider, Provider::Local);
    }

    #[test]
    fn test_cloud_provider_debug_redacts_api_key() {
        let provider = CloudProviderConfig {
            name: "openai".to_string(),
            api_key: "sk-super-secret-key-12345".to_string(),
            base_url: "https://api.openai.com".to_string(),
            priority: 1,
        };
        let debug_output = format!("{:?}", provider);
        assert!(debug_output.contains("[REDACTED]"));
        assert!(!debug_output.contains("sk-super-secret-key-12345"));
    }

    #[test]
    fn test_token_usage_serialization() {
        let usage = TokenUsage {
            prompt_tokens: 100,
            completion_tokens: 50,
            total_tokens: 150,
        };
        let json = serde_json::to_string(&usage).unwrap();
        let deserialized: TokenUsage = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.prompt_tokens, 100);
        assert_eq!(deserialized.completion_tokens, 50);
        assert_eq!(deserialized.total_tokens, 150);
    }
}
