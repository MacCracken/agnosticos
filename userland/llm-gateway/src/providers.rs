//! LLM provider implementations

use async_trait::async_trait;
use tokio::sync::mpsc;

use agnos_common::{InferenceRequest, InferenceResponse};

/// Trait for LLM providers
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Run inference
    async fn infer(&self, request: InferenceRequest) -> anyhow::Result<InferenceResponse>;
    
    /// Stream inference results
    async fn infer_stream(&self, request: InferenceRequest) -> anyhow::Result<mpsc::Receiver<anyhow::Result<String>>>;
    
    /// Load a model
    async fn load_model(&self, model_id: &str) -> anyhow::Result<agnos_common::ModelInfo>;
    
    /// Unload a model
    async fn unload_model(&self, model_id: &str) -> anyhow::Result<()>;
    
    /// List available models
    async fn list_models(&self) -> anyhow::Result<Vec<agnos_common::ModelInfo>>;
}

/// Provider types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProviderType {
    Ollama,
    LlamaCpp,
    OpenAi,
    Anthropic,
    Google,
}

/// Ollama provider implementation
pub struct OllamaProvider {
    base_url: String,
}

impl OllamaProvider {
    pub async fn new() -> anyhow::Result<Self> {
        Ok(Self {
            base_url: "http://localhost:11434".to_string(),
        })
    }
}

#[async_trait]
impl LlmProvider for OllamaProvider {
    async fn infer(&self, request: InferenceRequest) -> anyhow::Result<InferenceResponse> {
        use reqwest;
        
        let client = reqwest::Client::new();
        let response = client
            .post(format!("{}/api/generate", self.base_url))
            .json(&serde_json::json!({
                "model": request.model,
                "prompt": request.prompt,
                "stream": false,
                "options": {
                    "temperature": request.temperature,
                    "top_p": request.top_p,
                    "num_predict": request.max_tokens,
                }
            }))
            .send()
            .await?;
        
        let result: serde_json::Value = response.json().await?;
        
        Ok(InferenceResponse {
            text: result["response"].as_str().unwrap_or("").to_string(),
            tokens_generated: result["eval_count"].as_u64().unwrap_or(0) as u32,
            finish_reason: agnos_common::FinishReason::Stop,
            model: request.model,
            usage: agnos_common::TokenUsage {
                prompt_tokens: result["prompt_eval_count"].as_u64().unwrap_or(0) as u32,
                completion_tokens: result["eval_count"].as_u64().unwrap_or(0) as u32,
                total_tokens: result["eval_count"].as_u64().unwrap_or(0) as u32 
                    + result["prompt_eval_count"].as_u64().unwrap_or(0) as u32,
            },
        })
    }
    
    async fn infer_stream(&self, _request: InferenceRequest) -> anyhow::Result<mpsc::Receiver<anyhow::Result<String>>> {
        let (tx, rx) = mpsc::channel(100);
        // TODO: Implement streaming
        Ok(rx)
    }
    
    async fn load_model(&self, model_id: &str) -> anyhow::Result<agnos_common::ModelInfo> {
        // Pull the model if not already available
        let client = reqwest::Client::new();
        let _ = client
            .post(format!("{}/api/pull", self.base_url))
            .json(&serde_json::json!({
                "name": model_id,
            }))
            .send()
            .await?;
        
        Ok(agnos_common::ModelInfo {
            id: model_id.to_string(),
            name: model_id.to_string(),
            provider: agnos_common::Provider::Local,
            capabilities: vec![agnos_common::ModelCapability::TextGeneration],
            max_tokens: 4096,
            size_bytes: 0,
            loaded: true,
        })
    }
    
    async fn unload_model(&self, _model_id: &str) -> anyhow::Result<()> {
        // Ollama manages model loading/unloading automatically
        Ok(())
    }
    
    async fn list_models(&self) -> anyhow::Result<Vec<agnos_common::ModelInfo>> {
        let client = reqwest::Client::new();
        let response = client
            .get(format!("{}/api/tags", self.base_url))
            .send()
            .await?;
        
        let result: serde_json::Value = response.json().await?;
        let models: Vec<agnos_common::ModelInfo> = result["models"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .filter_map(|m| {
                Some(agnos_common::ModelInfo {
                    id: m["name"].as_str()?.to_string(),
                    name: m["name"].as_str()?.to_string(),
                    provider: agnos_common::Provider::Local,
                    capabilities: vec![agnos_common::ModelCapability::TextGeneration],
                    max_tokens: 4096,
                    size_bytes: m["size"].as_u64().unwrap_or(0),
                    loaded: true,
                })
            })
            .collect();
        
        Ok(models)
    }
}

/// llama.cpp provider implementation
pub struct LlamaCppProvider {
    base_url: String,
}

impl LlamaCppProvider {
    pub async fn new() -> anyhow::Result<Self> {
        Ok(Self {
            base_url: "http://localhost:8080".to_string(),
        })
    }
}

#[async_trait]
impl LlmProvider for LlamaCppProvider {
    async fn infer(&self, request: InferenceRequest) -> anyhow::Result<InferenceResponse> {
        use reqwest;
        
        let client = reqwest::Client::new();
        let response = client
            .post(format!("{}/completion", self.base_url))
            .json(&serde_json::json!({
                "prompt": request.prompt,
                "temperature": request.temperature,
                "top_p": request.top_p,
                "n_predict": request.max_tokens,
            }))
            .send()
            .await?;
        
        let result: serde_json::Value = response.json().await?;
        
        Ok(InferenceResponse {
            text: result["content"].as_str().unwrap_or("").to_string(),
            tokens_generated: result["tokens_predicted"].as_u64().unwrap_or(0) as u32,
            finish_reason: agnos_common::FinishReason::Stop,
            model: request.model,
            usage: agnos_common::TokenUsage {
                prompt_tokens: result["tokens_evaluated"].as_u64().unwrap_or(0) as u32,
                completion_tokens: result["tokens_predicted"].as_u64().unwrap_or(0) as u32,
                total_tokens: result["tokens_evaluated"].as_u64().unwrap_or(0) as u32 
                    + result["tokens_predicted"].as_u64().unwrap_or(0) as u32,
            },
        })
    }
    
    async fn infer_stream(&self, _request: InferenceRequest) -> anyhow::Result<mpsc::Receiver<anyhow::Result<String>>> {
        let (tx, rx) = mpsc::channel(100);
        Ok(rx)
    }
    
    async fn load_model(&self, _model_id: &str) -> anyhow::Result<agnos_common::ModelInfo> {
        // llama.cpp loads models at startup
        anyhow::bail!("llama.cpp requires model at startup")
    }
    
    async fn unload_model(&self, _model_id: &str) -> anyhow::Result<()> {
        Ok(())
    }
    
    async fn list_models(&self) -> anyhow::Result<Vec<agnos_common::ModelInfo>> {
        // llama.cpp typically runs one model at a time
        Ok(vec![])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_type_variants() {
        assert!(matches!(ProviderType::Ollama, ProviderType::Ollama));
        assert!(matches!(ProviderType::LlamaCpp, ProviderType::LlamaCpp));
        assert!(matches!(ProviderType::OpenAi, ProviderType::OpenAi));
        assert!(matches!(ProviderType::Anthropic, ProviderType::Anthropic));
        assert!(matches!(ProviderType::Google, ProviderType::Google));
    }

    #[test]
    fn test_provider_type_equality() {
        assert_eq!(ProviderType::Ollama, ProviderType::Ollama);
        assert_ne!(ProviderType::Ollama, ProviderType::LlamaCpp);
    }

    #[test]
    fn test_provider_type_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(ProviderType::Ollama);
        set.insert(ProviderType::Ollama);
        assert_eq!(set.len(), 1);
    }

    #[tokio::test]
    async fn test_ollama_provider_new() {
        let provider = OllamaProvider::new().await;
        assert!(provider.is_ok());
    }

    #[tokio::test]
    async fn test_llama_cpp_provider_new() {
        let provider = LlamaCppProvider::new().await;
        assert!(provider.is_ok());
    }

    #[tokio::test]
    async fn test_llama_cpp_load_model() {
        let provider = LlamaCppProvider::new().await.unwrap();
        let result = provider.load_model("test-model").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_llama_cpp_unload_model() {
        let provider = LlamaCppProvider::new().await.unwrap();
        let result = provider.unload_model("test-model").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_llama_cpp_list_models() {
        let provider = LlamaCppProvider::new().await.unwrap();
        let models = provider.list_models().await.unwrap();
        assert!(models.is_empty());
    }
}
