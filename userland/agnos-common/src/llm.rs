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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudProviderConfig {
    pub name: String,
    pub api_key: String,
    pub base_url: String,
    pub priority: u32,
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
