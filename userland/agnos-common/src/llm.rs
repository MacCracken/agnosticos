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
}
