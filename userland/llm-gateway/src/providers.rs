//! LLM provider implementations

use async_trait::async_trait;
use futures::StreamExt;
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

// ---------------------------------------------------------------------------
// Ollama
// ---------------------------------------------------------------------------

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

    async fn infer_stream(&self, request: InferenceRequest) -> anyhow::Result<mpsc::Receiver<anyhow::Result<String>>> {
        let (tx, rx) = mpsc::channel(100);
        let url = format!("{}/api/generate", self.base_url);
        let client = self.client.clone();

        tokio::spawn(async move {
            let resp = client
                .post(&url)
                .json(&serde_json::json!({
                    "model": request.model,
                    "prompt": request.prompt,
                    "stream": true,
                    "options": {
                        "temperature": request.temperature,
                        "top_p": request.top_p,
                        "num_predict": request.max_tokens,
                    }
                }))
                .send()
                .await;

            let resp = match resp {
                Ok(r) => r,
                Err(e) => { let _ = tx.send(Err(e.into())).await; return; }
            };

            let mut stream = resp.bytes_stream();
            let mut buffer = String::new();

            while let Some(chunk) = stream.next().await {
                match chunk {
                    Ok(bytes) => {
                        buffer.push_str(&String::from_utf8_lossy(&bytes));
                        // Ollama streams newline-delimited JSON
                        while let Some(pos) = buffer.find('\n') {
                            let line = buffer[..pos].to_string();
                            buffer = buffer[pos + 1..].to_string();
                            if line.trim().is_empty() { continue; }
                            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&line) {
                                if let Some(text) = json["response"].as_str() {
                                    if !text.is_empty() {
                                        if tx.send(Ok(text.to_string())).await.is_err() { return; }
                                    }
                                }
                                if json["done"].as_bool() == Some(true) { return; }
                            }
                        }
                    }
                    Err(e) => { let _ = tx.send(Err(e.into())).await; return; }
                }
            }
        });

        Ok(rx)
    }

    async fn load_model(&self, model_id: &str) -> anyhow::Result<agnos_common::ModelInfo> {
        let _ = self.client
            .post(format!("{}/api/pull", self.base_url))
            .json(&serde_json::json!({ "name": model_id }))
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

// ---------------------------------------------------------------------------
// llama.cpp
// ---------------------------------------------------------------------------

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

    async fn infer_stream(&self, request: InferenceRequest) -> anyhow::Result<mpsc::Receiver<anyhow::Result<String>>> {
        let (tx, rx) = mpsc::channel(100);
        let url = format!("{}/completion", self.base_url);
        let client = self.client.clone();

        tokio::spawn(async move {
            let resp = client
                .post(&url)
                .json(&serde_json::json!({
                    "prompt": request.prompt,
                    "temperature": request.temperature,
                    "top_p": request.top_p,
                    "n_predict": request.max_tokens,
                    "stream": true,
                }))
                .send()
                .await;

            let resp = match resp {
                Ok(r) => r,
                Err(e) => { let _ = tx.send(Err(e.into())).await; return; }
            };

            let mut stream = resp.bytes_stream();
            let mut buffer = String::new();

            while let Some(chunk) = stream.next().await {
                match chunk {
                    Ok(bytes) => {
                        buffer.push_str(&String::from_utf8_lossy(&bytes));
                        // llama.cpp streams SSE: "data: {...}\n\n"
                        while let Some(pos) = buffer.find("\n\n") {
                            let event = buffer[..pos].to_string();
                            buffer = buffer[pos + 2..].to_string();
                            for line in event.lines() {
                                if let Some(data) = line.strip_prefix("data: ") {
                                    if data.trim() == "[DONE]" { return; }
                                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                                        if let Some(text) = json["content"].as_str() {
                                            if !text.is_empty() {
                                                if tx.send(Ok(text.to_string())).await.is_err() { return; }
                                            }
                                        }
                                        if json["stop"].as_bool() == Some(true) { return; }
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => { let _ = tx.send(Err(e.into())).await; return; }
                }
            }
        });

        Ok(rx)
    }

    async fn load_model(&self, _model_id: &str) -> anyhow::Result<agnos_common::ModelInfo> {
        anyhow::bail!("llama.cpp requires model at startup")
    }

    async fn unload_model(&self, _model_id: &str) -> anyhow::Result<()> {
        Ok(())
    }

    async fn list_models(&self) -> anyhow::Result<Vec<agnos_common::ModelInfo>> {
        Ok(vec![])
    }
}

// ---------------------------------------------------------------------------
// OpenAI
// ---------------------------------------------------------------------------

/// Wrapper to redact API key from Debug output
struct RedactedKey(String);

impl std::fmt::Debug for RedactedKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.0.len() > 8 {
            write!(f, "{}...{}", &self.0[..4], &self.0[self.0.len()-4..])
        } else {
            write!(f, "[REDACTED]")
        }
    }
}

pub struct OpenAiProvider {
    base_url: String,
    api_key: RedactedKey,
    client: reqwest::Client,
}

impl OpenAiProvider {
    pub fn new(api_key: String, base_url: Option<String>) -> anyhow::Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .pool_max_idle_per_host(4)
            .build()?;
        Ok(Self {
            base_url: base_url.unwrap_or_else(|| "https://api.openai.com/v1".to_string()),
            api_key: RedactedKey(api_key),
            client,
        })
    }
}

#[async_trait]
impl LlmProvider for OpenAiProvider {
    async fn infer(&self, request: InferenceRequest) -> anyhow::Result<InferenceResponse> {
        let response = self.client
            .post(format!("{}/chat/completions", self.base_url))
            .bearer_auth(&self.api_key.0)
            .json(&serde_json::json!({
                "model": request.model,
                "messages": [{"role": "user", "content": request.prompt}],
                "max_tokens": request.max_tokens,
                "temperature": request.temperature,
                "top_p": request.top_p,
                "presence_penalty": request.presence_penalty,
                "frequency_penalty": request.frequency_penalty,
            }))
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("OpenAI API error ({}): {}", status, body);
        }

        let result: serde_json::Value = response.json().await?;
        let choice = &result["choices"][0];
        let message_text = choice["message"]["content"].as_str().unwrap_or("").to_string();
        let finish = match choice["finish_reason"].as_str() {
            Some("length") => agnos_common::FinishReason::Length,
            _ => agnos_common::FinishReason::Stop,
        };
        let usage = &result["usage"];

        Ok(InferenceResponse {
            text: message_text,
            tokens_generated: usage["completion_tokens"].as_u64().unwrap_or(0) as u32,
            finish_reason: finish,
            model: result["model"].as_str().unwrap_or(&request.model).to_string(),
            usage: agnos_common::TokenUsage {
                prompt_tokens: usage["prompt_tokens"].as_u64().unwrap_or(0) as u32,
                completion_tokens: usage["completion_tokens"].as_u64().unwrap_or(0) as u32,
                total_tokens: usage["total_tokens"].as_u64().unwrap_or(0) as u32,
            },
        })
    }

    async fn infer_stream(&self, request: InferenceRequest) -> anyhow::Result<mpsc::Receiver<anyhow::Result<String>>> {
        let (tx, rx) = mpsc::channel(100);
        let url = format!("{}/chat/completions", self.base_url);
        let api_key = self.api_key.0.clone();
        let client = self.client.clone();

        tokio::spawn(async move {
            let resp = client
                .post(&url)
                .bearer_auth(&api_key)
                .json(&serde_json::json!({
                    "model": request.model,
                    "messages": [{"role": "user", "content": request.prompt}],
                    "max_tokens": request.max_tokens,
                    "temperature": request.temperature,
                    "stream": true,
                }))
                .send()
                .await;

            let resp = match resp {
                Ok(r) => r,
                Err(e) => { let _ = tx.send(Err(e.into())).await; return; }
            };

            let mut stream = resp.bytes_stream();
            let mut buffer = String::new();

            while let Some(chunk) = stream.next().await {
                match chunk {
                    Ok(bytes) => {
                        buffer.push_str(&String::from_utf8_lossy(&bytes));
                        while let Some(pos) = buffer.find("\n\n") {
                            let event = buffer[..pos].to_string();
                            buffer = buffer[pos + 2..].to_string();
                            for line in event.lines() {
                                if let Some(data) = line.strip_prefix("data: ") {
                                    if data.trim() == "[DONE]" { return; }
                                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                                        if let Some(text) = json["choices"][0]["delta"]["content"].as_str() {
                                            if !text.is_empty() {
                                                if tx.send(Ok(text.to_string())).await.is_err() { return; }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => { let _ = tx.send(Err(e.into())).await; return; }
                }
            }
        });

        Ok(rx)
    }

    async fn load_model(&self, _model_id: &str) -> anyhow::Result<agnos_common::ModelInfo> {
        // Cloud-managed, no-op
        anyhow::bail!("OpenAI models are cloud-managed and cannot be loaded locally")
    }

    async fn unload_model(&self, _model_id: &str) -> anyhow::Result<()> {
        Ok(())
    }

    async fn list_models(&self) -> anyhow::Result<Vec<agnos_common::ModelInfo>> {
        let response = self.client
            .get(format!("{}/models", self.base_url))
            .bearer_auth(&self.api_key.0)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            anyhow::bail!("OpenAI list models failed ({})", status);
        }

        let result: serde_json::Value = response.json().await?;
        let models = result["data"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .filter_map(|m| {
                Some(agnos_common::ModelInfo {
                    id: m["id"].as_str()?.to_string(),
                    name: m["id"].as_str()?.to_string(),
                    provider: agnos_common::Provider::OpenAi,
                    capabilities: vec![agnos_common::ModelCapability::TextGeneration],
                    max_tokens: 4096,
                    size_bytes: 0,
                    loaded: true,
                })
            })
            .collect();

        Ok(models)
    }
}

// ---------------------------------------------------------------------------
// Anthropic
// ---------------------------------------------------------------------------

pub struct AnthropicProvider {
    base_url: String,
    api_key: RedactedKey,
    client: reqwest::Client,
}

impl AnthropicProvider {
    pub fn new(api_key: String, base_url: Option<String>) -> anyhow::Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .pool_max_idle_per_host(4)
            .build()?;
        Ok(Self {
            base_url: base_url.unwrap_or_else(|| "https://api.anthropic.com/v1".to_string()),
            api_key: RedactedKey(api_key),
            client,
        })
    }
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    async fn infer(&self, request: InferenceRequest) -> anyhow::Result<InferenceResponse> {
        let response = self.client
            .post(format!("{}/messages", self.base_url))
            .header("x-api-key", &self.api_key.0)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&serde_json::json!({
                "model": request.model,
                "messages": [{"role": "user", "content": request.prompt}],
                "max_tokens": request.max_tokens,
                "temperature": request.temperature,
                "top_p": request.top_p,
            }))
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Anthropic API error ({}): {}", status, body);
        }

        let result: serde_json::Value = response.json().await?;
        let text = result["content"]
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(|c| c["text"].as_str())
            .unwrap_or("")
            .to_string();

        let finish = match result["stop_reason"].as_str() {
            Some("max_tokens") => agnos_common::FinishReason::Length,
            _ => agnos_common::FinishReason::Stop,
        };

        let input_tokens = result["usage"]["input_tokens"].as_u64().unwrap_or(0) as u32;
        let output_tokens = result["usage"]["output_tokens"].as_u64().unwrap_or(0) as u32;

        Ok(InferenceResponse {
            text,
            tokens_generated: output_tokens,
            finish_reason: finish,
            model: result["model"].as_str().unwrap_or(&request.model).to_string(),
            usage: agnos_common::TokenUsage {
                prompt_tokens: input_tokens,
                completion_tokens: output_tokens,
                total_tokens: input_tokens + output_tokens,
            },
        })
    }

    async fn infer_stream(&self, request: InferenceRequest) -> anyhow::Result<mpsc::Receiver<anyhow::Result<String>>> {
        let (tx, rx) = mpsc::channel(100);
        let url = format!("{}/messages", self.base_url);
        let api_key = self.api_key.0.clone();
        let client = self.client.clone();

        tokio::spawn(async move {
            let resp = client
                .post(&url)
                .header("x-api-key", &api_key)
                .header("anthropic-version", "2023-06-01")
                .header("content-type", "application/json")
                .json(&serde_json::json!({
                    "model": request.model,
                    "messages": [{"role": "user", "content": request.prompt}],
                    "max_tokens": request.max_tokens,
                    "temperature": request.temperature,
                    "stream": true,
                }))
                .send()
                .await;

            let resp = match resp {
                Ok(r) => r,
                Err(e) => { let _ = tx.send(Err(e.into())).await; return; }
            };

            let mut stream = resp.bytes_stream();
            let mut buffer = String::new();

            while let Some(chunk) = stream.next().await {
                match chunk {
                    Ok(bytes) => {
                        buffer.push_str(&String::from_utf8_lossy(&bytes));
                        while let Some(pos) = buffer.find("\n\n") {
                            let event = buffer[..pos].to_string();
                            buffer = buffer[pos + 2..].to_string();
                            // Anthropic SSE: "event: <type>\ndata: <json>"
                            let mut data_str = None;
                            for line in event.lines() {
                                if let Some(d) = line.strip_prefix("data: ") {
                                    data_str = Some(d.to_string());
                                }
                            }
                            if let Some(data) = data_str {
                                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&data) {
                                    // content_block_delta events contain the text
                                    if json["type"].as_str() == Some("content_block_delta") {
                                        if let Some(text) = json["delta"]["text"].as_str() {
                                            if !text.is_empty() {
                                                if tx.send(Ok(text.to_string())).await.is_err() { return; }
                                            }
                                        }
                                    }
                                    if json["type"].as_str() == Some("message_stop") { return; }
                                }
                            }
                        }
                    }
                    Err(e) => { let _ = tx.send(Err(e.into())).await; return; }
                }
            }
        });

        Ok(rx)
    }

    async fn load_model(&self, _model_id: &str) -> anyhow::Result<agnos_common::ModelInfo> {
        anyhow::bail!("Anthropic models are cloud-managed and cannot be loaded locally")
    }

    async fn unload_model(&self, _model_id: &str) -> anyhow::Result<()> {
        Ok(())
    }

    async fn list_models(&self) -> anyhow::Result<Vec<agnos_common::ModelInfo>> {
        // Anthropic doesn't have a list models endpoint; return known models
        Ok(vec![
            agnos_common::ModelInfo {
                id: "claude-sonnet-4-20250514".to_string(),
                name: "Claude Sonnet 4".to_string(),
                provider: agnos_common::Provider::Anthropic,
                capabilities: vec![agnos_common::ModelCapability::TextGeneration],
                max_tokens: 8192,
                size_bytes: 0,
                loaded: true,
            },
            agnos_common::ModelInfo {
                id: "claude-haiku-4-20250414".to_string(),
                name: "Claude Haiku 4".to_string(),
                provider: agnos_common::Provider::Anthropic,
                capabilities: vec![agnos_common::ModelCapability::TextGeneration],
                max_tokens: 8192,
                size_bytes: 0,
                loaded: true,
            },
        ])
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

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

    #[test]
    fn test_provider_type_debug() {
        assert_eq!(format!("{:?}", ProviderType::Ollama), "Ollama");
        assert_eq!(format!("{:?}", ProviderType::LlamaCpp), "LlamaCpp");
        assert_eq!(format!("{:?}", ProviderType::OpenAi), "OpenAi");
        assert_eq!(format!("{:?}", ProviderType::Anthropic), "Anthropic");
        assert_eq!(format!("{:?}", ProviderType::Google), "Google");
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
        assert_eq!(set.len(), 5);
    }

    #[test]
    fn test_provider_type_ne_exhaustive() {
        let variants = [
            ProviderType::Ollama, ProviderType::LlamaCpp,
            ProviderType::OpenAi, ProviderType::Anthropic, ProviderType::Google,
        ];
        for (i, a) in variants.iter().enumerate() {
            for (j, b) in variants.iter().enumerate() {
                if i == j { assert_eq!(a, b); } else { assert_ne!(a, b); }
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
        assert!(provider.unload_model("any-model").await.is_ok());
    }

    #[tokio::test]
    async fn test_llama_cpp_infer_stream_returns_receiver() {
        let provider = LlamaCppProvider::new().await.unwrap();
        let request = InferenceRequest::default();
        let rx = provider.infer_stream(request).await;
        assert!(rx.is_ok());
    }

    #[tokio::test]
    async fn test_ollama_infer_stream_returns_receiver() {
        let provider = OllamaProvider::new().await.unwrap();
        let request = InferenceRequest::default();
        let rx = provider.infer_stream(request).await;
        assert!(rx.is_ok());
    }

    #[tokio::test]
    async fn test_llama_cpp_load_model_error_message() {
        let provider = LlamaCppProvider::new().await.unwrap();
        let err = provider.load_model("anything").await.unwrap_err();
        assert!(err.to_string().contains("llama.cpp requires model at startup"));
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
    }

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
        assert!(provider.infer(request).await.is_err());
    }

    #[tokio::test]
    async fn test_ollama_infer_error_is_descriptive() {
        let provider = OllamaProvider::new().await.unwrap();
        let err = provider.infer(InferenceRequest::default()).await.unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("error") || msg.contains("connect") || msg.contains("Connection"));
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
        assert!(provider.infer(request).await.is_err());
    }

    #[tokio::test]
    async fn test_llama_cpp_infer_error_is_descriptive() {
        let provider = LlamaCppProvider::new().await.unwrap();
        let err = provider.infer(InferenceRequest::default()).await.unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("error") || msg.contains("connect") || msg.contains("Connection"));
    }

    #[tokio::test]
    async fn test_ollama_list_models_fails_without_server() {
        let provider = OllamaProvider::new().await.unwrap();
        assert!(provider.list_models().await.is_err());
    }

    #[tokio::test]
    async fn test_ollama_load_model_fails_without_server() {
        let provider = OllamaProvider::new().await.unwrap();
        assert!(provider.load_model("nonexistent-model").await.is_err());
    }

    #[tokio::test]
    async fn test_llama_cpp_load_model_error_message_content() {
        let provider = LlamaCppProvider::new().await.unwrap();
        let err = provider.load_model("model-xyz").await.unwrap_err();
        assert_eq!(err.to_string(), "llama.cpp requires model at startup");
    }

    #[tokio::test]
    async fn test_provider_trait_ollama_as_dyn() {
        let provider: Box<dyn LlmProvider> = Box::new(OllamaProvider::new().await.unwrap());
        assert!(provider.unload_model("anything").await.is_ok());
    }

    #[tokio::test]
    async fn test_provider_trait_llama_cpp_as_dyn() {
        let provider: Box<dyn LlmProvider> = Box::new(LlamaCppProvider::new().await.unwrap());
        assert!(provider.unload_model("anything").await.is_ok());
        assert!(provider.list_models().await.unwrap().is_empty());
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
        assert!(provider.load_model("m").await.unwrap_err().to_string().contains("startup"));
    }

    // --- OpenAI provider tests ---

    #[test]
    fn test_openai_provider_new() {
        let provider = OpenAiProvider::new("sk-test-key".to_string(), None);
        assert!(provider.is_ok());
        let p = provider.unwrap();
        assert_eq!(p.base_url, "https://api.openai.com/v1");
    }

    #[test]
    fn test_openai_provider_custom_base_url() {
        let provider = OpenAiProvider::new(
            "sk-test".to_string(),
            Some("http://localhost:4000".to_string()),
        ).unwrap();
        assert_eq!(provider.base_url, "http://localhost:4000");
    }

    #[tokio::test]
    async fn test_openai_unload_is_noop() {
        let provider = OpenAiProvider::new("sk-test".to_string(), None).unwrap();
        assert!(provider.unload_model("gpt-4").await.is_ok());
    }

    #[tokio::test]
    async fn test_openai_load_model_fails() {
        let provider = OpenAiProvider::new("sk-test".to_string(), None).unwrap();
        let err = provider.load_model("gpt-4").await.unwrap_err();
        assert!(err.to_string().contains("cloud-managed"));
    }

    #[tokio::test]
    async fn test_openai_as_dyn_trait() {
        let provider: Box<dyn LlmProvider> = Box::new(
            OpenAiProvider::new("sk-test".to_string(), None).unwrap()
        );
        assert!(provider.unload_model("x").await.is_ok());
    }

    #[tokio::test]
    async fn test_openai_infer_stream_returns_receiver() {
        let provider = OpenAiProvider::new("sk-test".to_string(), None).unwrap();
        let rx = provider.infer_stream(InferenceRequest::default()).await;
        assert!(rx.is_ok());
    }

    #[test]
    fn test_redacted_key_debug_long() {
        let key = RedactedKey("sk-1234567890abcdef".to_string());
        let dbg = format!("{:?}", key);
        assert!(dbg.contains("sk-1"));
        assert!(dbg.contains("cdef"));
        assert!(!dbg.contains("1234567890abcdef"));
    }

    #[test]
    fn test_redacted_key_debug_short() {
        let key = RedactedKey("short".to_string());
        assert_eq!(format!("{:?}", key), "[REDACTED]");
    }

    // --- Anthropic provider tests ---

    #[test]
    fn test_anthropic_provider_new() {
        let provider = AnthropicProvider::new("ant-key".to_string(), None);
        assert!(provider.is_ok());
        assert_eq!(provider.unwrap().base_url, "https://api.anthropic.com/v1");
    }

    #[test]
    fn test_anthropic_provider_custom_base_url() {
        let provider = AnthropicProvider::new(
            "ant-key".to_string(),
            Some("http://localhost:5000".to_string()),
        ).unwrap();
        assert_eq!(provider.base_url, "http://localhost:5000");
    }

    #[tokio::test]
    async fn test_anthropic_unload_is_noop() {
        let provider = AnthropicProvider::new("ant-key".to_string(), None).unwrap();
        assert!(provider.unload_model("claude-3").await.is_ok());
    }

    #[tokio::test]
    async fn test_anthropic_load_model_fails() {
        let provider = AnthropicProvider::new("ant-key".to_string(), None).unwrap();
        let err = provider.load_model("claude-3").await.unwrap_err();
        assert!(err.to_string().contains("cloud-managed"));
    }

    #[tokio::test]
    async fn test_anthropic_list_models_returns_known() {
        let provider = AnthropicProvider::new("ant-key".to_string(), None).unwrap();
        let models = provider.list_models().await.unwrap();
        assert!(models.len() >= 2);
        assert!(models.iter().any(|m| m.id.contains("claude")));
    }

    #[tokio::test]
    async fn test_anthropic_as_dyn_trait() {
        let provider: Box<dyn LlmProvider> = Box::new(
            AnthropicProvider::new("ant-key".to_string(), None).unwrap()
        );
        assert!(provider.unload_model("x").await.is_ok());
    }

    #[tokio::test]
    async fn test_anthropic_infer_stream_returns_receiver() {
        let provider = AnthropicProvider::new("ant-key".to_string(), None).unwrap();
        let rx = provider.infer_stream(InferenceRequest::default()).await;
        assert!(rx.is_ok());
    }

    // ------------------------------------------------------------------
    // RedactedKey boundary and edge-case tests
    // ------------------------------------------------------------------

    #[test]
    fn test_redacted_key_exactly_8_chars() {
        // Boundary: len == 8, should print [REDACTED]
        let key = RedactedKey("12345678".to_string());
        assert_eq!(format!("{:?}", key), "[REDACTED]");
    }

    #[test]
    fn test_redacted_key_9_chars() {
        // Boundary: len == 9, should show first 4 and last 4
        let key = RedactedKey("123456789".to_string());
        let dbg = format!("{:?}", key);
        assert_eq!(dbg, "1234...6789");
    }

    #[test]
    fn test_redacted_key_empty() {
        let key = RedactedKey("".to_string());
        assert_eq!(format!("{:?}", key), "[REDACTED]");
    }

    #[test]
    fn test_redacted_key_one_char() {
        let key = RedactedKey("x".to_string());
        assert_eq!(format!("{:?}", key), "[REDACTED]");
    }

    #[test]
    fn test_redacted_key_very_long() {
        let key = RedactedKey("sk-abcdefghijklmnopqrstuvwxyz0123456789".to_string());
        let dbg = format!("{:?}", key);
        assert!(dbg.starts_with("sk-a"));
        assert!(dbg.ends_with("6789"));
        assert!(dbg.contains("..."));
        // Full key should NOT appear
        assert!(!dbg.contains("abcdefghijklmnopqrstuvwxyz0123456789"));
    }

    // ------------------------------------------------------------------
    // Anthropic list_models detail checks
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_anthropic_list_models_model_ids() {
        let provider = AnthropicProvider::new("ant-key".to_string(), None).unwrap();
        let models = provider.list_models().await.unwrap();
        let ids: Vec<&str> = models.iter().map(|m| m.id.as_str()).collect();
        assert!(ids.contains(&"claude-sonnet-4-20250514"));
        assert!(ids.contains(&"claude-haiku-4-20250414"));
    }

    #[tokio::test]
    async fn test_anthropic_list_models_provider_type() {
        let provider = AnthropicProvider::new("ant-key".to_string(), None).unwrap();
        let models = provider.list_models().await.unwrap();
        for model in &models {
            assert_eq!(model.provider, agnos_common::Provider::Anthropic);
            assert_eq!(model.max_tokens, 8192);
            assert!(model.loaded);
            assert_eq!(model.size_bytes, 0);
            assert!(model.capabilities.contains(&agnos_common::ModelCapability::TextGeneration));
        }
    }

    #[tokio::test]
    async fn test_anthropic_list_models_names() {
        let provider = AnthropicProvider::new("ant-key".to_string(), None).unwrap();
        let models = provider.list_models().await.unwrap();
        let names: Vec<&str> = models.iter().map(|m| m.name.as_str()).collect();
        assert!(names.contains(&"Claude Sonnet 4"));
        assert!(names.contains(&"Claude Haiku 4"));
    }

    // ------------------------------------------------------------------
    // OpenAI/Anthropic error paths without server
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_openai_infer_fails_without_server() {
        let provider = OpenAiProvider::new(
            "sk-fake".to_string(),
            Some("http://127.0.0.1:19999".to_string()),
        ).unwrap();
        let request = InferenceRequest {
            model: "gpt-4".to_string(),
            prompt: "Hello".to_string(),
            max_tokens: 10,
            temperature: 0.7,
            top_p: 1.0,
            presence_penalty: 0.0,
            frequency_penalty: 0.0,
        };
        let result = provider.infer(request).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_openai_list_models_fails_without_server() {
        let provider = OpenAiProvider::new(
            "sk-fake".to_string(),
            Some("http://127.0.0.1:19999".to_string()),
        ).unwrap();
        assert!(provider.list_models().await.is_err());
    }

    #[tokio::test]
    async fn test_anthropic_infer_fails_without_server() {
        let provider = AnthropicProvider::new(
            "ant-fake".to_string(),
            Some("http://127.0.0.1:19999".to_string()),
        ).unwrap();
        let request = InferenceRequest {
            model: "claude-3-opus".to_string(),
            prompt: "Hello".to_string(),
            max_tokens: 10,
            temperature: 0.7,
            top_p: 1.0,
            presence_penalty: 0.0,
            frequency_penalty: 0.0,
        };
        let result = provider.infer(request).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_anthropic_load_model_error_message_content() {
        let provider = AnthropicProvider::new("ant-key".to_string(), None).unwrap();
        let err = provider.load_model("claude-3").await.unwrap_err();
        assert_eq!(
            err.to_string(),
            "Anthropic models are cloud-managed and cannot be loaded locally"
        );
    }

    #[tokio::test]
    async fn test_openai_load_model_error_message_content() {
        let provider = OpenAiProvider::new("sk-test".to_string(), None).unwrap();
        let err = provider.load_model("gpt-4").await.unwrap_err();
        assert_eq!(
            err.to_string(),
            "OpenAI models are cloud-managed and cannot be loaded locally"
        );
    }

    // ------------------------------------------------------------------
    // Provider construction: verify base_url and api_key stored correctly
    // ------------------------------------------------------------------

    #[test]
    fn test_openai_provider_stores_api_key() {
        let provider = OpenAiProvider::new("sk-my-secret-key".to_string(), None).unwrap();
        // We can only verify via RedactedKey Debug — the raw key is private
        let dbg = format!("{:?}", provider.api_key);
        assert!(dbg.contains("sk-m"));
        assert!(dbg.contains("-key"));
    }

    #[test]
    fn test_anthropic_provider_stores_api_key() {
        let provider = AnthropicProvider::new("ant-my-secret-key".to_string(), None).unwrap();
        let dbg = format!("{:?}", provider.api_key);
        assert!(dbg.contains("ant-"));
        assert!(dbg.contains("-key"));
    }

    #[test]
    fn test_openai_provider_empty_api_key() {
        let provider = OpenAiProvider::new("".to_string(), None).unwrap();
        let dbg = format!("{:?}", provider.api_key);
        assert_eq!(dbg, "[REDACTED]");
    }

    #[test]
    fn test_anthropic_provider_empty_api_key() {
        let provider = AnthropicProvider::new("".to_string(), None).unwrap();
        let dbg = format!("{:?}", provider.api_key);
        assert_eq!(dbg, "[REDACTED]");
    }

    // ------------------------------------------------------------------
    // Unload model is always a no-op for cloud providers
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_openai_unload_model_multiple_times() {
        let provider = OpenAiProvider::new("sk-test".to_string(), None).unwrap();
        assert!(provider.unload_model("gpt-4").await.is_ok());
        assert!(provider.unload_model("gpt-4").await.is_ok());
        assert!(provider.unload_model("").await.is_ok());
    }

    #[tokio::test]
    async fn test_anthropic_unload_model_multiple_times() {
        let provider = AnthropicProvider::new("ant-key".to_string(), None).unwrap();
        assert!(provider.unload_model("claude-3").await.is_ok());
        assert!(provider.unload_model("claude-3").await.is_ok());
        assert!(provider.unload_model("").await.is_ok());
    }

    // ------------------------------------------------------------------
    // Arc-wrapped cloud providers (trait object tests)
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_openai_arc_provider_list_models_err() {
        use std::sync::Arc;
        let provider: Arc<dyn LlmProvider> = Arc::new(
            OpenAiProvider::new(
                "sk-fake".to_string(),
                Some("http://127.0.0.1:19999".to_string()),
            ).unwrap()
        );
        // list_models requires HTTP — will fail without server
        assert!(provider.list_models().await.is_err());
    }

    #[tokio::test]
    async fn test_anthropic_arc_provider_list_models_ok() {
        use std::sync::Arc;
        let provider: Arc<dyn LlmProvider> = Arc::new(
            AnthropicProvider::new("ant-key".to_string(), None).unwrap()
        );
        // Anthropic list_models returns hardcoded models — always succeeds
        let models = provider.list_models().await.unwrap();
        assert_eq!(models.len(), 2);
    }

    // ------------------------------------------------------------------
    // OpenAI/Anthropic infer_stream error paths
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_openai_infer_stream_sends_error_on_connection_failure() {
        let provider = OpenAiProvider::new(
            "sk-fake".to_string(),
            Some("http://127.0.0.1:19999".to_string()),
        ).unwrap();
        let mut rx = provider.infer_stream(InferenceRequest::default()).await.unwrap();
        // The spawned task should send an error through the channel
        let result = rx.recv().await;
        assert!(result.is_some());
        assert!(result.unwrap().is_err());
    }

    #[tokio::test]
    async fn test_anthropic_infer_stream_sends_error_on_connection_failure() {
        let provider = AnthropicProvider::new(
            "ant-fake".to_string(),
            Some("http://127.0.0.1:19999".to_string()),
        ).unwrap();
        let mut rx = provider.infer_stream(InferenceRequest::default()).await.unwrap();
        let result = rx.recv().await;
        assert!(result.is_some());
        assert!(result.unwrap().is_err());
    }

    // ------------------------------------------------------------------
    // ProviderType: Display-like patterns (via Debug), Copy semantics
    // ------------------------------------------------------------------

    #[test]
    fn test_provider_type_copy_semantics() {
        let a = ProviderType::Anthropic;
        let b = a; // Copy
        let c = a; // Copy again — a is still valid
        assert_eq!(a, b);
        assert_eq!(b, c);
    }

    #[test]
    fn test_provider_type_in_vec() {
        let types = vec![
            ProviderType::Ollama,
            ProviderType::LlamaCpp,
            ProviderType::OpenAi,
            ProviderType::Anthropic,
            ProviderType::Google,
        ];
        assert_eq!(types.len(), 5);
        assert!(types.contains(&ProviderType::Google));
    }
}
