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
    client: reqwest::Client,
}

impl OllamaProvider {
    pub async fn new() -> anyhow::Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .pool_max_idle_per_host(4)
            .build()?;
        Ok(Self {
            base_url: "http://localhost:11434".to_string(),
            client,
        })
    }
}

#[async_trait]
impl LlmProvider for OllamaProvider {
    async fn infer(&self, request: InferenceRequest) -> anyhow::Result<InferenceResponse> {
        let response = self.client
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
        let _ = self.client
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
        let response = self.client
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
    client: reqwest::Client,
}

impl LlamaCppProvider {
    pub async fn new() -> anyhow::Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .pool_max_idle_per_host(4)
            .build()?;
        Ok(Self {
            base_url: "http://localhost:8080".to_string(),
            client,
        })
    }
}

#[async_trait]
impl LlmProvider for LlamaCppProvider {
    async fn infer(&self, request: InferenceRequest) -> anyhow::Result<InferenceResponse> {
        let response = self.client
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

    // ------------------------------------------------------------------
    // Additional coverage: ProviderType traits and provider fields
    // ------------------------------------------------------------------

    #[test]
    fn test_provider_type_debug() {
        let dbg = format!("{:?}", ProviderType::Ollama);
        assert_eq!(dbg, "Ollama");
        let dbg = format!("{:?}", ProviderType::LlamaCpp);
        assert_eq!(dbg, "LlamaCpp");
        let dbg = format!("{:?}", ProviderType::OpenAi);
        assert_eq!(dbg, "OpenAi");
        let dbg = format!("{:?}", ProviderType::Anthropic);
        assert_eq!(dbg, "Anthropic");
        let dbg = format!("{:?}", ProviderType::Google);
        assert_eq!(dbg, "Google");
    }

    #[test]
    fn test_provider_type_clone() {
        let a = ProviderType::Ollama;
        let b = a;
        assert_eq!(a, b);
    }

    #[test]
    fn test_provider_type_copy() {
        let a = ProviderType::Google;
        let b = a;
        // a is still valid (Copy)
        assert_eq!(a, b);
    }

    #[test]
    fn test_provider_type_all_variants_distinct() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(ProviderType::Ollama);
        set.insert(ProviderType::LlamaCpp);
        set.insert(ProviderType::OpenAi);
        set.insert(ProviderType::Anthropic);
        set.insert(ProviderType::Google);
        assert_eq!(set.len(), 5, "All 5 provider types must be distinct");
    }

    #[test]
    fn test_provider_type_ne_exhaustive() {
        let variants = [
            ProviderType::Ollama,
            ProviderType::LlamaCpp,
            ProviderType::OpenAi,
            ProviderType::Anthropic,
            ProviderType::Google,
        ];
        for (i, a) in variants.iter().enumerate() {
            for (j, b) in variants.iter().enumerate() {
                if i == j {
                    assert_eq!(a, b);
                } else {
                    assert_ne!(a, b);
                }
            }
        }
    }

    #[tokio::test]
    async fn test_ollama_provider_base_url() {
        let provider = OllamaProvider::new().await.unwrap();
        assert_eq!(provider.base_url, "http://localhost:11434");
    }

    #[tokio::test]
    async fn test_llama_cpp_provider_base_url() {
        let provider = LlamaCppProvider::new().await.unwrap();
        assert_eq!(provider.base_url, "http://localhost:8080");
    }

    #[tokio::test]
    async fn test_ollama_provider_unload_model_is_noop() {
        let provider = OllamaProvider::new().await.unwrap();
        // Ollama manages loading automatically — unload is always Ok
        let result = provider.unload_model("any-model").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_llama_cpp_infer_stream_returns_receiver() {
        let provider = LlamaCppProvider::new().await.unwrap();
        let request = InferenceRequest::default();
        let rx = provider.infer_stream(request).await;
        assert!(rx.is_ok(), "infer_stream should return a receiver");
    }

    #[tokio::test]
    async fn test_ollama_infer_stream_returns_receiver() {
        let provider = OllamaProvider::new().await.unwrap();
        let request = InferenceRequest::default();
        let rx = provider.infer_stream(request).await;
        assert!(rx.is_ok(), "infer_stream should return a receiver");
    }

    #[tokio::test]
    async fn test_llama_cpp_load_model_error_message() {
        let provider = LlamaCppProvider::new().await.unwrap();
        let err = provider.load_model("anything").await.unwrap_err();
        assert!(
            err.to_string().contains("llama.cpp requires model at startup"),
            "Error should explain startup requirement, got: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_llama_cpp_unload_model_always_ok() {
        let provider = LlamaCppProvider::new().await.unwrap();
        assert!(provider.unload_model("nonexistent").await.is_ok());
        assert!(provider.unload_model("").await.is_ok());
    }

    #[tokio::test]
    async fn test_llama_cpp_list_models_returns_empty_vec() {
        let provider = LlamaCppProvider::new().await.unwrap();
        let models = provider.list_models().await.unwrap();
        assert_eq!(models.len(), 0);
    }

    #[test]
    fn test_provider_type_used_as_hashmap_key() {
        use std::collections::HashMap;
        let mut map: HashMap<ProviderType, &str> = HashMap::new();
        map.insert(ProviderType::Ollama, "ollama");
        map.insert(ProviderType::LlamaCpp, "llamacpp");
        map.insert(ProviderType::OpenAi, "openai");
        map.insert(ProviderType::Anthropic, "anthropic");
        map.insert(ProviderType::Google, "google");
        assert_eq!(map.len(), 5);
        assert_eq!(map[&ProviderType::Ollama], "ollama");
        assert_eq!(map[&ProviderType::Google], "google");
    }

    // ==================================================================
    // Additional coverage: provider infer() error paths, trait coverage
    // ==================================================================

    #[tokio::test]
    async fn test_ollama_infer_fails_gracefully_without_server() {
        let provider = OllamaProvider::new().await.unwrap();
        let request = InferenceRequest {
            model: "llama2".to_string(),
            prompt: "Hello".to_string(),
            max_tokens: 10,
            temperature: 0.7,
            top_p: 1.0,
            presence_penalty: 0.0,
            frequency_penalty: 0.0,
        };
        let result = provider.infer(request).await;
        // Should fail since Ollama is not running in test env
        assert!(result.is_err(), "infer should fail without running Ollama server");
    }

    #[tokio::test]
    async fn test_ollama_infer_error_is_descriptive() {
        let provider = OllamaProvider::new().await.unwrap();
        let request = InferenceRequest::default();
        let err = provider.infer(request).await.unwrap_err();
        let msg = err.to_string();
        // Error should mention connection failure
        assert!(
            msg.contains("error") || msg.contains("connect") || msg.contains("Connection"),
            "Error message should be descriptive, got: {}",
            msg
        );
    }

    #[tokio::test]
    async fn test_llama_cpp_infer_fails_gracefully_without_server() {
        let provider = LlamaCppProvider::new().await.unwrap();
        let request = InferenceRequest {
            model: "gguf-model".to_string(),
            prompt: "Test prompt".to_string(),
            max_tokens: 50,
            temperature: 0.5,
            top_p: 0.9,
            presence_penalty: 0.0,
            frequency_penalty: 0.0,
        };
        let result = provider.infer(request).await;
        assert!(result.is_err(), "infer should fail without running llama.cpp server");
    }

    #[tokio::test]
    async fn test_llama_cpp_infer_error_is_descriptive() {
        let provider = LlamaCppProvider::new().await.unwrap();
        let request = InferenceRequest::default();
        let err = provider.infer(request).await.unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("error") || msg.contains("connect") || msg.contains("Connection"),
            "Error message should be descriptive, got: {}",
            msg
        );
    }

    #[tokio::test]
    async fn test_ollama_list_models_fails_without_server() {
        let provider = OllamaProvider::new().await.unwrap();
        let result = provider.list_models().await;
        assert!(result.is_err(), "list_models should fail without running Ollama server");
    }

    #[tokio::test]
    async fn test_ollama_load_model_fails_without_server() {
        let provider = OllamaProvider::new().await.unwrap();
        let result = provider.load_model("nonexistent-model").await;
        assert!(result.is_err(), "load_model should fail without running Ollama server");
    }

    #[tokio::test]
    async fn test_llama_cpp_load_model_error_message_content() {
        let provider = LlamaCppProvider::new().await.unwrap();
        let err = provider.load_model("model-xyz").await.unwrap_err();
        let msg = err.to_string();
        assert_eq!(msg, "llama.cpp requires model at startup");
    }

    #[tokio::test]
    async fn test_ollama_infer_stream_returns_channel() {
        let provider = OllamaProvider::new().await.unwrap();
        let request = InferenceRequest::default();
        let result = provider.infer_stream(request).await;
        assert!(result.is_ok());
        let mut rx = result.unwrap();
        // Channel sender is dropped immediately in stub, so recv returns None
        assert!(rx.recv().await.is_none());
    }

    #[tokio::test]
    async fn test_llama_cpp_infer_stream_returns_channel() {
        let provider = LlamaCppProvider::new().await.unwrap();
        let request = InferenceRequest::default();
        let result = provider.infer_stream(request).await;
        assert!(result.is_ok());
        let mut rx = result.unwrap();
        assert!(rx.recv().await.is_none());
    }

    #[tokio::test]
    async fn test_provider_trait_ollama_as_dyn() {
        let provider: Box<dyn LlmProvider> = Box::new(OllamaProvider::new().await.unwrap());
        // Verify trait object works
        let result = provider.unload_model("anything").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_provider_trait_llama_cpp_as_dyn() {
        let provider: Box<dyn LlmProvider> = Box::new(LlamaCppProvider::new().await.unwrap());
        let result = provider.unload_model("anything").await;
        assert!(result.is_ok());
        let models = provider.list_models().await.unwrap();
        assert!(models.is_empty());
    }

    #[tokio::test]
    async fn test_provider_trait_arc_ollama() {
        use std::sync::Arc;
        let provider: Arc<dyn LlmProvider> = Arc::new(OllamaProvider::new().await.unwrap());
        assert!(provider.unload_model("m").await.is_ok());
    }

    #[tokio::test]
    async fn test_provider_trait_arc_llama_cpp() {
        use std::sync::Arc;
        let provider: Arc<dyn LlmProvider> = Arc::new(LlamaCppProvider::new().await.unwrap());
        let err = provider.load_model("m").await.unwrap_err();
        assert!(err.to_string().contains("startup"));
    }
}
