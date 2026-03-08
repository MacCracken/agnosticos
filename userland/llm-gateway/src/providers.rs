//! LLM provider implementations

use async_trait::async_trait;
use futures::StreamExt;
use tokio::sync::mpsc;

use agnos_common::{InferenceRequest, InferenceResponse};

/// Trait for LLM providers
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Run inference
    async fn infer(&self, request: &InferenceRequest) -> anyhow::Result<InferenceResponse>;

    /// Stream inference results
    async fn infer_stream(
        &self,
        request: InferenceRequest,
    ) -> anyhow::Result<mpsc::Receiver<anyhow::Result<String>>>;

    /// Load a model
    async fn load_model(&self, model_id: &str) -> anyhow::Result<agnos_common::ModelInfo>;

    /// Unload a model
    #[allow(dead_code)]
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
    DeepSeek,
    Mistral,
    Grok,
    Groq,
    OpenRouter,
    LmStudio,
    LocalAi,
    OpenCode,
    Letta,
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
    async fn infer(&self, request: &InferenceRequest) -> anyhow::Result<InferenceResponse> {
        let response = self
            .client
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
            tokens_generated: result["eval_count"]
                .as_u64()
                .unwrap_or(0)
                .min(u32::MAX as u64) as u32,
            finish_reason: agnos_common::FinishReason::Stop,
            model: request.model.clone(),
            usage: agnos_common::TokenUsage {
                prompt_tokens: result["prompt_eval_count"]
                    .as_u64()
                    .unwrap_or(0)
                    .min(u32::MAX as u64) as u32,
                completion_tokens: result["eval_count"]
                    .as_u64()
                    .unwrap_or(0)
                    .min(u32::MAX as u64) as u32,
                total_tokens: result["eval_count"]
                    .as_u64()
                    .unwrap_or(0)
                    .min(u32::MAX as u64) as u32
                    + result["prompt_eval_count"]
                        .as_u64()
                        .unwrap_or(0)
                        .min(u32::MAX as u64) as u32,
            },
        })
    }

    async fn infer_stream(
        &self,
        request: InferenceRequest,
    ) -> anyhow::Result<mpsc::Receiver<anyhow::Result<String>>> {
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
                Err(e) => {
                    let _ = tx.send(Err(e.into())).await;
                    return;
                }
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
                            buffer = buffer.split_off(pos + 1);
                            if line.trim().is_empty() {
                                continue;
                            }
                            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&line) {
                                if let Some(text) = json["response"].as_str() {
                                    if !text.is_empty()
                                        && tx.send(Ok(text.to_string())).await.is_err()
                                    {
                                        return;
                                    }
                                }
                                if json["done"].as_bool() == Some(true) {
                                    return;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(Err(e.into())).await;
                        return;
                    }
                }
            }
        });

        Ok(rx)
    }

    async fn load_model(&self, model_id: &str) -> anyhow::Result<agnos_common::ModelInfo> {
        let _ = self
            .client
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
        let response = self
            .client
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
    async fn infer(&self, request: &InferenceRequest) -> anyhow::Result<InferenceResponse> {
        let response = self
            .client
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
            tokens_generated: result["tokens_predicted"]
                .as_u64()
                .unwrap_or(0)
                .min(u32::MAX as u64) as u32,
            finish_reason: agnos_common::FinishReason::Stop,
            model: request.model.clone(),
            usage: agnos_common::TokenUsage {
                prompt_tokens: result["tokens_evaluated"]
                    .as_u64()
                    .unwrap_or(0)
                    .min(u32::MAX as u64) as u32,
                completion_tokens: result["tokens_predicted"]
                    .as_u64()
                    .unwrap_or(0)
                    .min(u32::MAX as u64) as u32,
                total_tokens: result["tokens_evaluated"]
                    .as_u64()
                    .unwrap_or(0)
                    .min(u32::MAX as u64) as u32
                    + result["tokens_predicted"]
                        .as_u64()
                        .unwrap_or(0)
                        .min(u32::MAX as u64) as u32,
            },
        })
    }

    async fn infer_stream(
        &self,
        request: InferenceRequest,
    ) -> anyhow::Result<mpsc::Receiver<anyhow::Result<String>>> {
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
                Err(e) => {
                    let _ = tx.send(Err(e.into())).await;
                    return;
                }
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
                            buffer = buffer.split_off(pos + 2);
                            for line in event.lines() {
                                if let Some(data) = line.strip_prefix("data: ") {
                                    if data.trim() == "[DONE]" {
                                        return;
                                    }
                                    if let Ok(json) =
                                        serde_json::from_str::<serde_json::Value>(data)
                                    {
                                        if let Some(text) = json["content"].as_str() {
                                            if !text.is_empty()
                                                && tx.send(Ok(text.to_string())).await.is_err()
                                            {
                                                return;
                                            }
                                        }
                                        if json["stop"].as_bool() == Some(true) {
                                            return;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(Err(e.into())).await;
                        return;
                    }
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
pub(crate) struct RedactedKey(String);

impl std::fmt::Debug for RedactedKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.0.len() > 8 {
            write!(f, "{}...{}", &self.0[..4], &self.0[self.0.len() - 4..])
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
    async fn infer(&self, request: &InferenceRequest) -> anyhow::Result<InferenceResponse> {
        let response = self
            .client
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
        let choice = result["choices"]
            .get(0)
            .ok_or_else(|| anyhow::anyhow!("OpenAI response missing choices array"))?;
        let message_text = choice["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let finish = match choice["finish_reason"].as_str() {
            Some("length") => agnos_common::FinishReason::Length,
            _ => agnos_common::FinishReason::Stop,
        };
        let usage = &result["usage"];

        Ok(InferenceResponse {
            text: message_text,
            tokens_generated: usage["completion_tokens"]
                .as_u64()
                .unwrap_or(0)
                .min(u32::MAX as u64) as u32,
            finish_reason: finish,
            model: result["model"]
                .as_str()
                .unwrap_or(&request.model)
                .to_string(),
            usage: agnos_common::TokenUsage {
                prompt_tokens: usage["prompt_tokens"]
                    .as_u64()
                    .unwrap_or(0)
                    .min(u32::MAX as u64) as u32,
                completion_tokens: usage["completion_tokens"]
                    .as_u64()
                    .unwrap_or(0)
                    .min(u32::MAX as u64) as u32,
                total_tokens: usage["total_tokens"]
                    .as_u64()
                    .unwrap_or(0)
                    .min(u32::MAX as u64) as u32,
            },
        })
    }

    async fn infer_stream(
        &self,
        request: InferenceRequest,
    ) -> anyhow::Result<mpsc::Receiver<anyhow::Result<String>>> {
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
                Err(e) => {
                    let _ = tx.send(Err(e.into())).await;
                    return;
                }
            };

            let mut stream = resp.bytes_stream();
            let mut buffer = String::new();

            while let Some(chunk) = stream.next().await {
                match chunk {
                    Ok(bytes) => {
                        buffer.push_str(&String::from_utf8_lossy(&bytes));
                        while let Some(pos) = buffer.find("\n\n") {
                            let event = buffer[..pos].to_string();
                            buffer = buffer.split_off(pos + 2);
                            for line in event.lines() {
                                if let Some(data) = line.strip_prefix("data: ") {
                                    if data.trim() == "[DONE]" {
                                        return;
                                    }
                                    if let Ok(json) =
                                        serde_json::from_str::<serde_json::Value>(data)
                                    {
                                        if let Some(text) =
                                            json["choices"][0]["delta"]["content"].as_str()
                                        {
                                            if !text.is_empty()
                                                && tx.send(Ok(text.to_string())).await.is_err()
                                            {
                                                return;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(Err(e.into())).await;
                        return;
                    }
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
        let response = self
            .client
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
    async fn infer(&self, request: &InferenceRequest) -> anyhow::Result<InferenceResponse> {
        let response = self
            .client
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

        let input_tokens = result["usage"]["input_tokens"]
            .as_u64()
            .unwrap_or(0)
            .min(u32::MAX as u64) as u32;
        let output_tokens = result["usage"]["output_tokens"]
            .as_u64()
            .unwrap_or(0)
            .min(u32::MAX as u64) as u32;

        Ok(InferenceResponse {
            text,
            tokens_generated: output_tokens,
            finish_reason: finish,
            model: result["model"]
                .as_str()
                .unwrap_or(&request.model)
                .to_string(),
            usage: agnos_common::TokenUsage {
                prompt_tokens: input_tokens,
                completion_tokens: output_tokens,
                total_tokens: input_tokens + output_tokens,
            },
        })
    }

    async fn infer_stream(
        &self,
        request: InferenceRequest,
    ) -> anyhow::Result<mpsc::Receiver<anyhow::Result<String>>> {
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
                Err(e) => {
                    let _ = tx.send(Err(e.into())).await;
                    return;
                }
            };

            let mut stream = resp.bytes_stream();
            let mut buffer = String::new();

            while let Some(chunk) = stream.next().await {
                match chunk {
                    Ok(bytes) => {
                        buffer.push_str(&String::from_utf8_lossy(&bytes));
                        while let Some(pos) = buffer.find("\n\n") {
                            let event = buffer[..pos].to_string();
                            buffer = buffer.split_off(pos + 2);
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
                                            if !text.is_empty()
                                                && tx.send(Ok(text.to_string())).await.is_err()
                                            {
                                                return;
                                            }
                                        }
                                    }
                                    if json["type"].as_str() == Some("message_stop") {
                                        return;
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(Err(e.into())).await;
                        return;
                    }
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
// Google (Gemini)
// ---------------------------------------------------------------------------

pub struct GoogleProvider {
    base_url: String,
    api_key: RedactedKey,
    client: reqwest::Client,
}

impl GoogleProvider {
    #[allow(dead_code)]
    pub fn new(api_key: String, base_url: Option<String>) -> anyhow::Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .pool_max_idle_per_host(4)
            .build()?;
        Ok(Self {
            base_url: base_url
                .unwrap_or_else(|| "https://generativelanguage.googleapis.com/v1beta".to_string()),
            api_key: RedactedKey(api_key),
            client,
        })
    }
}

#[async_trait]
impl LlmProvider for GoogleProvider {
    async fn infer(&self, request: &InferenceRequest) -> anyhow::Result<InferenceResponse> {
        let url = format!(
            "{}/models/{}:generateContent?key={}",
            self.base_url, request.model, self.api_key.0
        );

        let response = self
            .client
            .post(&url)
            .header("content-type", "application/json")
            .json(&serde_json::json!({
                "contents": [{"parts": [{"text": request.prompt}]}],
                "generationConfig": {
                    "maxOutputTokens": request.max_tokens,
                    "temperature": request.temperature,
                    "topP": request.top_p,
                }
            }))
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Google Gemini API error ({}): {}", status, body);
        }

        let result: serde_json::Value = response.json().await?;
        let text = result["candidates"]
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(|c| c["content"]["parts"].as_array())
            .and_then(|parts| parts.first())
            .and_then(|p| p["text"].as_str())
            .unwrap_or("")
            .to_string();

        let finish_reason = result["candidates"]
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(|c| c["finishReason"].as_str());

        let finish = match finish_reason {
            Some("MAX_TOKENS") => agnos_common::FinishReason::Length,
            _ => agnos_common::FinishReason::Stop,
        };

        let prompt_tokens = result["usageMetadata"]["promptTokenCount"]
            .as_u64()
            .unwrap_or(0) as u32;
        let completion_tokens = result["usageMetadata"]["candidatesTokenCount"]
            .as_u64()
            .unwrap_or(0) as u32;

        Ok(InferenceResponse {
            text,
            tokens_generated: completion_tokens,
            finish_reason: finish,
            model: request.model.clone(),
            usage: agnos_common::TokenUsage {
                prompt_tokens,
                completion_tokens,
                total_tokens: prompt_tokens + completion_tokens,
            },
        })
    }

    async fn infer_stream(
        &self,
        request: InferenceRequest,
    ) -> anyhow::Result<mpsc::Receiver<anyhow::Result<String>>> {
        let (tx, rx) = mpsc::channel(100);
        let url = format!(
            "{}/models/{}:streamGenerateContent?key={}&alt=sse",
            self.base_url, request.model, self.api_key.0
        );
        let client = self.client.clone();

        tokio::spawn(async move {
            let resp = client
                .post(&url)
                .header("content-type", "application/json")
                .json(&serde_json::json!({
                    "contents": [{"parts": [{"text": request.prompt}]}],
                    "generationConfig": {
                        "maxOutputTokens": request.max_tokens,
                        "temperature": request.temperature,
                        "topP": request.top_p,
                    }
                }))
                .send()
                .await;

            let resp = match resp {
                Ok(r) => r,
                Err(e) => {
                    let _ = tx.send(Err(e.into())).await;
                    return;
                }
            };

            let mut stream = resp.bytes_stream();
            let mut buffer = String::new();

            while let Some(chunk) = stream.next().await {
                match chunk {
                    Ok(bytes) => {
                        buffer.push_str(&String::from_utf8_lossy(&bytes));
                        // Gemini SSE: "data: <json>\n\n"
                        while let Some(pos) = buffer.find("\n\n") {
                            let event = buffer[..pos].to_string();
                            buffer = buffer.split_off(pos + 2);
                            if let Some(data) = event.strip_prefix("data: ") {
                                if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                                    if let Some(text) = json["candidates"]
                                        .as_array()
                                        .and_then(|arr| arr.first())
                                        .and_then(|c| c["content"]["parts"].as_array())
                                        .and_then(|parts| parts.first())
                                        .and_then(|p| p["text"].as_str())
                                    {
                                        if !text.is_empty()
                                            && tx.send(Ok(text.to_string())).await.is_err()
                                        {
                                            return;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(Err(e.into())).await;
                        return;
                    }
                }
            }
        });

        Ok(rx)
    }

    async fn load_model(&self, _model_id: &str) -> anyhow::Result<agnos_common::ModelInfo> {
        anyhow::bail!("Google models are cloud-managed and cannot be loaded locally")
    }

    async fn unload_model(&self, _model_id: &str) -> anyhow::Result<()> {
        Ok(())
    }

    async fn list_models(&self) -> anyhow::Result<Vec<agnos_common::ModelInfo>> {
        let url = format!("{}/models?key={}", self.base_url, self.api_key.0);
        let response = self.client.get(&url).send().await?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Google Gemini list models error ({}): {}", status, body);
        }

        let result: serde_json::Value = response.json().await?;
        let mut models = Vec::new();

        if let Some(items) = result["models"].as_array() {
            for item in items {
                let name = item["name"].as_str().unwrap_or("").to_string();
                let display = item["displayName"].as_str().unwrap_or(&name).to_string();
                let max_out = item["outputTokenLimit"].as_u64().unwrap_or(8192) as u32;
                models.push(agnos_common::ModelInfo {
                    id: name,
                    name: display,
                    provider: agnos_common::Provider::Google,
                    capabilities: vec![agnos_common::ModelCapability::TextGeneration],
                    max_tokens: max_out,
                    size_bytes: 0,
                    loaded: true,
                });
            }
        }

        Ok(models)
    }
}

// ---------------------------------------------------------------------------
// OpenAI-Compatible Provider (generic)
// ---------------------------------------------------------------------------
// Covers: DeepSeek, Mistral, Grok (x.ai), Groq, OpenRouter, LM Studio,
//         LocalAI, OpenCode, Letta — all expose an OpenAI-compatible API.

/// Configuration for an OpenAI-compatible provider.
#[derive(Debug, Clone)]
pub struct OpenAiCompatibleConfig {
    pub provider_name: &'static str,
    #[allow(dead_code)]
    pub provider_type: ProviderType,
    pub common_provider: agnos_common::Provider,
    pub default_base_url: &'static str,
    pub default_max_tokens: u32,
    /// Known model IDs returned when the `/models` endpoint is unavailable.
    pub known_models: &'static [(&'static str, &'static str)], // (id, display_name)
    /// Whether this provider requires an API key (false for local providers).
    pub requires_api_key: bool,
}

pub struct OpenAiCompatibleProvider {
    pub(crate) config: OpenAiCompatibleConfig,
    pub(crate) base_url: String,
    pub(crate) api_key: Option<RedactedKey>,
    client: reqwest::Client,
}

impl std::fmt::Debug for OpenAiCompatibleProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenAiCompatibleProvider")
            .field("provider", &self.config.provider_name)
            .field("base_url", &self.base_url)
            .finish()
    }
}

impl OpenAiCompatibleProvider {
    pub fn new(
        config: OpenAiCompatibleConfig,
        api_key: Option<String>,
        base_url: Option<String>,
    ) -> anyhow::Result<Self> {
        if config.requires_api_key && api_key.is_none() {
            anyhow::bail!("{} requires an API key", config.provider_name);
        }
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .pool_max_idle_per_host(4)
            .build()?;
        Ok(Self {
            base_url: base_url.unwrap_or_else(|| config.default_base_url.to_string()),
            api_key: api_key.map(RedactedKey),
            client,
            config,
        })
    }
}

#[async_trait]
impl LlmProvider for OpenAiCompatibleProvider {
    async fn infer(&self, request: &InferenceRequest) -> anyhow::Result<InferenceResponse> {
        let mut req = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .json(&serde_json::json!({
                "model": request.model,
                "messages": [{"role": "user", "content": request.prompt}],
                "max_tokens": request.max_tokens,
                "temperature": request.temperature,
                "top_p": request.top_p,
                "presence_penalty": request.presence_penalty,
                "frequency_penalty": request.frequency_penalty,
            }));

        if let Some(ref key) = self.api_key {
            req = req.bearer_auth(&key.0);
        }

        let response = req.send().await?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("{} API error ({}): {}", self.config.provider_name, status, body);
        }

        let result: serde_json::Value = response.json().await?;
        let choice = result["choices"]
            .get(0)
            .ok_or_else(|| anyhow::anyhow!("{} response missing choices", self.config.provider_name))?;
        let message_text = choice["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let finish = match choice["finish_reason"].as_str() {
            Some("length") => agnos_common::FinishReason::Length,
            _ => agnos_common::FinishReason::Stop,
        };
        let usage = &result["usage"];

        Ok(InferenceResponse {
            text: message_text,
            tokens_generated: usage["completion_tokens"]
                .as_u64()
                .unwrap_or(0)
                .min(u32::MAX as u64) as u32,
            finish_reason: finish,
            model: result["model"]
                .as_str()
                .unwrap_or(&request.model)
                .to_string(),
            usage: agnos_common::TokenUsage {
                prompt_tokens: usage["prompt_tokens"]
                    .as_u64()
                    .unwrap_or(0)
                    .min(u32::MAX as u64) as u32,
                completion_tokens: usage["completion_tokens"]
                    .as_u64()
                    .unwrap_or(0)
                    .min(u32::MAX as u64) as u32,
                total_tokens: usage["total_tokens"]
                    .as_u64()
                    .unwrap_or(0)
                    .min(u32::MAX as u64) as u32,
            },
        })
    }

    async fn infer_stream(
        &self,
        request: InferenceRequest,
    ) -> anyhow::Result<mpsc::Receiver<anyhow::Result<String>>> {
        let (tx, rx) = mpsc::channel(100);
        let url = format!("{}/chat/completions", self.base_url);
        let api_key = self.api_key.as_ref().map(|k| k.0.clone());
        let client = self.client.clone();

        tokio::spawn(async move {
            let mut req = client
                .post(&url)
                .json(&serde_json::json!({
                    "model": request.model,
                    "messages": [{"role": "user", "content": request.prompt}],
                    "max_tokens": request.max_tokens,
                    "temperature": request.temperature,
                    "stream": true,
                }));

            if let Some(ref key) = api_key {
                req = req.bearer_auth(key);
            }

            let resp = match req.send().await {
                Ok(r) => r,
                Err(e) => {
                    let _ = tx.send(Err(e.into())).await;
                    return;
                }
            };

            let mut stream = resp.bytes_stream();
            let mut buffer = String::new();

            while let Some(chunk) = stream.next().await {
                match chunk {
                    Ok(bytes) => {
                        buffer.push_str(&String::from_utf8_lossy(&bytes));
                        while let Some(pos) = buffer.find("\n\n") {
                            let event = buffer[..pos].to_string();
                            buffer = buffer.split_off(pos + 2);
                            for line in event.lines() {
                                if let Some(data) = line.strip_prefix("data: ") {
                                    if data.trim() == "[DONE]" {
                                        return;
                                    }
                                    if let Ok(json) =
                                        serde_json::from_str::<serde_json::Value>(data)
                                    {
                                        if let Some(text) =
                                            json["choices"][0]["delta"]["content"].as_str()
                                        {
                                            if !text.is_empty()
                                                && tx.send(Ok(text.to_string())).await.is_err()
                                            {
                                                return;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(Err(e.into())).await;
                        return;
                    }
                }
            }
        });

        Ok(rx)
    }

    async fn load_model(&self, _model_id: &str) -> anyhow::Result<agnos_common::ModelInfo> {
        anyhow::bail!(
            "{} models cannot be loaded via this interface",
            self.config.provider_name
        )
    }

    async fn unload_model(&self, _model_id: &str) -> anyhow::Result<()> {
        Ok(())
    }

    async fn list_models(&self) -> anyhow::Result<Vec<agnos_common::ModelInfo>> {
        // Try the standard /models endpoint first
        let mut req = self.client.get(format!("{}/models", self.base_url));
        if let Some(ref key) = self.api_key {
            req = req.bearer_auth(&key.0);
        }

        if let Ok(response) = req.send().await {
            if response.status().is_success() {
                if let Ok(result) = response.json::<serde_json::Value>().await {
                    let models: Vec<agnos_common::ModelInfo> = result["data"]
                        .as_array()
                        .unwrap_or(&vec![])
                        .iter()
                        .filter_map(|m| {
                            Some(agnos_common::ModelInfo {
                                id: m["id"].as_str()?.to_string(),
                                name: m["id"].as_str()?.to_string(),
                                provider: self.config.common_provider.clone(),
                                capabilities: vec![agnos_common::ModelCapability::TextGeneration],
                                max_tokens: self.config.default_max_tokens,
                                size_bytes: 0,
                                loaded: true,
                            })
                        })
                        .collect();

                    if !models.is_empty() {
                        return Ok(models);
                    }
                }
            }
        }

        // Fall back to known models
        Ok(self
            .config
            .known_models
            .iter()
            .map(|(id, name)| agnos_common::ModelInfo {
                id: id.to_string(),
                name: name.to_string(),
                provider: self.config.common_provider.clone(),
                capabilities: vec![agnos_common::ModelCapability::TextGeneration],
                max_tokens: self.config.default_max_tokens,
                size_bytes: 0,
                loaded: true,
            })
            .collect())
    }
}

// ---------------------------------------------------------------------------
// Provider factory functions
// ---------------------------------------------------------------------------

/// DeepSeek — OpenAI-compatible API at api.deepseek.com
pub fn new_deepseek_provider(api_key: String, base_url: Option<String>) -> anyhow::Result<OpenAiCompatibleProvider> {
    OpenAiCompatibleProvider::new(
        OpenAiCompatibleConfig {
            provider_name: "DeepSeek",
            provider_type: ProviderType::DeepSeek,
            common_provider: agnos_common::Provider::DeepSeek,
            default_base_url: "https://api.deepseek.com/v1",
            default_max_tokens: 8192,
            known_models: &[
                ("deepseek-chat", "DeepSeek Chat"),
                ("deepseek-coder", "DeepSeek Coder"),
                ("deepseek-reasoner", "DeepSeek Reasoner"),
            ],
            requires_api_key: true,
        },
        Some(api_key),
        base_url,
    )
}

/// Mistral AI — OpenAI-compatible API at api.mistral.ai
pub fn new_mistral_provider(api_key: String, base_url: Option<String>) -> anyhow::Result<OpenAiCompatibleProvider> {
    OpenAiCompatibleProvider::new(
        OpenAiCompatibleConfig {
            provider_name: "Mistral",
            provider_type: ProviderType::Mistral,
            common_provider: agnos_common::Provider::Mistral,
            default_base_url: "https://api.mistral.ai/v1",
            default_max_tokens: 8192,
            known_models: &[
                ("mistral-large-latest", "Mistral Large"),
                ("mistral-medium-latest", "Mistral Medium"),
                ("mistral-small-latest", "Mistral Small"),
                ("open-mistral-nemo", "Mistral Nemo"),
                ("codestral-latest", "Codestral"),
            ],
            requires_api_key: true,
        },
        Some(api_key),
        base_url,
    )
}

/// Grok (x.ai) — OpenAI-compatible API at api.x.ai
pub fn new_grok_provider(api_key: String, base_url: Option<String>) -> anyhow::Result<OpenAiCompatibleProvider> {
    OpenAiCompatibleProvider::new(
        OpenAiCompatibleConfig {
            provider_name: "Grok",
            provider_type: ProviderType::Grok,
            common_provider: agnos_common::Provider::Grok,
            default_base_url: "https://api.x.ai/v1",
            default_max_tokens: 8192,
            known_models: &[
                ("grok-3", "Grok 3"),
                ("grok-3-mini", "Grok 3 Mini"),
                ("grok-2-1212", "Grok 2"),
                ("grok-2-vision-1212", "Grok 2 Vision"),
            ],
            requires_api_key: true,
        },
        Some(api_key),
        base_url,
    )
}

/// Groq — OpenAI-compatible hosted inference at api.groq.com
pub fn new_groq_provider(api_key: String, base_url: Option<String>) -> anyhow::Result<OpenAiCompatibleProvider> {
    OpenAiCompatibleProvider::new(
        OpenAiCompatibleConfig {
            provider_name: "Groq",
            provider_type: ProviderType::Groq,
            common_provider: agnos_common::Provider::Groq,
            default_base_url: "https://api.groq.com/openai/v1",
            default_max_tokens: 8192,
            known_models: &[
                ("llama-3.3-70b-versatile", "Llama 3.3 70B"),
                ("llama-3.1-8b-instant", "Llama 3.1 8B Instant"),
                ("mixtral-8x7b-32768", "Mixtral 8x7B"),
                ("gemma2-9b-it", "Gemma 2 9B"),
            ],
            requires_api_key: true,
        },
        Some(api_key),
        base_url,
    )
}

/// OpenRouter — multi-provider router at openrouter.ai
pub fn new_openrouter_provider(api_key: String, base_url: Option<String>) -> anyhow::Result<OpenAiCompatibleProvider> {
    OpenAiCompatibleProvider::new(
        OpenAiCompatibleConfig {
            provider_name: "OpenRouter",
            provider_type: ProviderType::OpenRouter,
            common_provider: agnos_common::Provider::OpenRouter,
            default_base_url: "https://openrouter.ai/api/v1",
            default_max_tokens: 4096,
            known_models: &[], // Dynamic — uses /models endpoint
            requires_api_key: true,
        },
        Some(api_key),
        base_url,
    )
}

/// LM Studio — local OpenAI-compatible server (no API key required)
pub fn new_lmstudio_provider(base_url: Option<String>) -> anyhow::Result<OpenAiCompatibleProvider> {
    OpenAiCompatibleProvider::new(
        OpenAiCompatibleConfig {
            provider_name: "LM Studio",
            provider_type: ProviderType::LmStudio,
            common_provider: agnos_common::Provider::LmStudio,
            default_base_url: "http://localhost:1234/v1",
            default_max_tokens: 4096,
            known_models: &[], // Dynamic — uses /models endpoint
            requires_api_key: false,
        },
        None,
        base_url,
    )
}

/// LocalAI — local OpenAI-compatible server (no API key required)
pub fn new_localai_provider(base_url: Option<String>) -> anyhow::Result<OpenAiCompatibleProvider> {
    OpenAiCompatibleProvider::new(
        OpenAiCompatibleConfig {
            provider_name: "LocalAI",
            provider_type: ProviderType::LocalAi,
            common_provider: agnos_common::Provider::LocalAi,
            default_base_url: "http://localhost:8080/v1",
            default_max_tokens: 4096,
            known_models: &[], // Dynamic — uses /models endpoint
            requires_api_key: false,
        },
        None,
        base_url,
    )
}

/// OpenCode Zen — cloud inference at api.open-code.dev
pub fn new_opencode_provider(api_key: String, base_url: Option<String>) -> anyhow::Result<OpenAiCompatibleProvider> {
    OpenAiCompatibleProvider::new(
        OpenAiCompatibleConfig {
            provider_name: "OpenCode",
            provider_type: ProviderType::OpenCode,
            common_provider: agnos_common::Provider::OpenCode,
            default_base_url: "https://api.open-code.dev/v1",
            default_max_tokens: 8192,
            known_models: &[
                ("gpt-5.2", "GPT 5.2"),
                ("claude-sonnet-4-5", "Claude Sonnet 4.5"),
                ("claude-haiku-4-5", "Claude Haiku 4.5"),
                ("gemini-3-flash", "Gemini 3 Flash"),
                ("qwen3-coder", "Qwen 3 Coder"),
            ],
            requires_api_key: true,
        },
        Some(api_key),
        base_url,
    )
}

/// Letta — stateful agent platform with OpenAI-compatible inference
pub fn new_letta_provider(api_key: Option<String>, base_url: Option<String>) -> anyhow::Result<OpenAiCompatibleProvider> {
    let is_local = std::env::var("LETTA_LOCAL").unwrap_or_default() == "true";
    let default_url = if is_local {
        "http://localhost:8283/v1"
    } else {
        "https://app.letta.com/v1"
    };
    OpenAiCompatibleProvider::new(
        OpenAiCompatibleConfig {
            provider_name: "Letta",
            provider_type: ProviderType::Letta,
            common_provider: agnos_common::Provider::Letta,
            default_base_url: default_url,
            default_max_tokens: 4096,
            known_models: &[
                ("openai/gpt-4o", "GPT-4o (via Letta)"),
                ("openai/gpt-4o-mini", "GPT-4o Mini (via Letta)"),
                ("anthropic/claude-sonnet-4-20250514", "Claude Sonnet 4 (via Letta)"),
            ],
            requires_api_key: !is_local,
        },
        api_key,
        base_url,
    )
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
        assert!(err
            .to_string()
            .contains("llama.cpp requires model at startup"));
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
        assert!(provider.infer(&request).await.is_err());
    }

    #[tokio::test]
    async fn test_ollama_infer_error_is_descriptive() {
        let provider = OllamaProvider::new().await.unwrap();
        let err = provider
            .infer(&InferenceRequest::default())
            .await
            .unwrap_err();
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
        assert!(provider.infer(&request).await.is_err());
    }

    #[tokio::test]
    async fn test_llama_cpp_infer_error_is_descriptive() {
        let provider = LlamaCppProvider::new().await.unwrap();
        let err = provider
            .infer(&InferenceRequest::default())
            .await
            .unwrap_err();
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
        assert!(provider
            .load_model("m")
            .await
            .unwrap_err()
            .to_string()
            .contains("startup"));
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
        )
        .unwrap();
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
        let provider: Box<dyn LlmProvider> =
            Box::new(OpenAiProvider::new("sk-test".to_string(), None).unwrap());
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
        )
        .unwrap();
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
        let provider: Box<dyn LlmProvider> =
            Box::new(AnthropicProvider::new("ant-key".to_string(), None).unwrap());
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
            assert!(model
                .capabilities
                .contains(&agnos_common::ModelCapability::TextGeneration));
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
        )
        .unwrap();
        let request = InferenceRequest {
            model: "gpt-4".to_string(),
            prompt: "Hello".to_string(),
            max_tokens: 10,
            temperature: 0.7,
            top_p: 1.0,
            presence_penalty: 0.0,
            frequency_penalty: 0.0,
        };
        let result = provider.infer(&request).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_openai_list_models_fails_without_server() {
        let provider = OpenAiProvider::new(
            "sk-fake".to_string(),
            Some("http://127.0.0.1:19999".to_string()),
        )
        .unwrap();
        assert!(provider.list_models().await.is_err());
    }

    #[tokio::test]
    async fn test_anthropic_infer_fails_without_server() {
        let provider = AnthropicProvider::new(
            "ant-fake".to_string(),
            Some("http://127.0.0.1:19999".to_string()),
        )
        .unwrap();
        let request = InferenceRequest {
            model: "claude-3-opus".to_string(),
            prompt: "Hello".to_string(),
            max_tokens: 10,
            temperature: 0.7,
            top_p: 1.0,
            presence_penalty: 0.0,
            frequency_penalty: 0.0,
        };
        let result = provider.infer(&request).await;
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
            )
            .unwrap(),
        );
        // list_models requires HTTP — will fail without server
        assert!(provider.list_models().await.is_err());
    }

    #[tokio::test]
    async fn test_anthropic_arc_provider_list_models_ok() {
        use std::sync::Arc;
        let provider: Arc<dyn LlmProvider> =
            Arc::new(AnthropicProvider::new("ant-key".to_string(), None).unwrap());
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
        )
        .unwrap();
        let mut rx = provider
            .infer_stream(InferenceRequest::default())
            .await
            .unwrap();
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
        )
        .unwrap();
        let mut rx = provider
            .infer_stream(InferenceRequest::default())
            .await
            .unwrap();
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
        let types = [
            ProviderType::Ollama,
            ProviderType::LlamaCpp,
            ProviderType::OpenAi,
            ProviderType::Anthropic,
            ProviderType::Google,
        ];
        assert_eq!(types.len(), 5);
        assert!(types.contains(&ProviderType::Google));
    }

    // --- Google provider tests ---

    #[test]
    fn test_google_provider_new() {
        let provider = GoogleProvider::new("goog-key".to_string(), None);
        assert!(provider.is_ok());
        assert_eq!(
            provider.unwrap().base_url,
            "https://generativelanguage.googleapis.com/v1beta"
        );
    }

    #[test]
    fn test_google_provider_custom_base_url() {
        let provider = GoogleProvider::new(
            "goog-key".to_string(),
            Some("http://localhost:6000".to_string()),
        )
        .unwrap();
        assert_eq!(provider.base_url, "http://localhost:6000");
    }

    #[tokio::test]
    async fn test_google_unload_is_noop() {
        let provider = GoogleProvider::new("goog-key".to_string(), None).unwrap();
        assert!(provider.unload_model("gemini-pro").await.is_ok());
    }

    #[tokio::test]
    async fn test_google_load_model_fails() {
        let provider = GoogleProvider::new("goog-key".to_string(), None).unwrap();
        let err = provider.load_model("gemini-pro").await.unwrap_err();
        assert!(err.to_string().contains("cloud-managed"));
    }

    #[tokio::test]
    async fn test_google_as_dyn_trait() {
        let provider: Box<dyn LlmProvider> =
            Box::new(GoogleProvider::new("goog-key".to_string(), None).unwrap());
        assert!(provider.unload_model("x").await.is_ok());
    }

    #[tokio::test]
    async fn test_google_infer_stream_returns_receiver() {
        let provider = GoogleProvider::new("goog-key".to_string(), None).unwrap();
        let rx = provider.infer_stream(InferenceRequest::default()).await;
        assert!(rx.is_ok());
    }

    #[tokio::test]
    async fn test_google_infer_fails_without_server() {
        let provider = GoogleProvider::new(
            "goog-fake".to_string(),
            Some("http://127.0.0.1:19999".to_string()),
        )
        .unwrap();
        let request = InferenceRequest {
            model: "gemini-pro".to_string(),
            prompt: "Hello".to_string(),
            max_tokens: 10,
            temperature: 0.7,
            top_p: 1.0,
            presence_penalty: 0.0,
            frequency_penalty: 0.0,
        };
        assert!(provider.infer(&request).await.is_err());
    }

    #[tokio::test]
    async fn test_google_list_models_fails_without_server() {
        let provider = GoogleProvider::new(
            "goog-fake".to_string(),
            Some("http://127.0.0.1:19999".to_string()),
        )
        .unwrap();
        assert!(provider.list_models().await.is_err());
    }

    #[tokio::test]
    async fn test_google_load_model_error_message_content() {
        let provider = GoogleProvider::new("goog-key".to_string(), None).unwrap();
        let err = provider.load_model("gemini-pro").await.unwrap_err();
        assert_eq!(
            err.to_string(),
            "Google models are cloud-managed and cannot be loaded locally"
        );
    }

    #[tokio::test]
    async fn test_google_unload_model_multiple_times() {
        let provider = GoogleProvider::new("goog-key".to_string(), None).unwrap();
        assert!(provider.unload_model("gemini-pro").await.is_ok());
        assert!(provider.unload_model("gemini-pro").await.is_ok());
        assert!(provider.unload_model("").await.is_ok());
    }

    #[tokio::test]
    async fn test_google_arc_provider_unload() {
        use std::sync::Arc;
        let provider: Arc<dyn LlmProvider> =
            Arc::new(GoogleProvider::new("goog-key".to_string(), None).unwrap());
        assert!(provider.unload_model("x").await.is_ok());
    }

    #[tokio::test]
    async fn test_google_infer_stream_sends_error_on_connection_failure() {
        let provider = GoogleProvider::new(
            "goog-fake".to_string(),
            Some("http://127.0.0.1:19999".to_string()),
        )
        .unwrap();
        let mut rx = provider
            .infer_stream(InferenceRequest::default())
            .await
            .unwrap();
        let result = rx.recv().await;
        assert!(result.is_some());
        assert!(result.unwrap().is_err());
    }

    #[test]
    fn test_google_provider_stores_api_key() {
        let provider = GoogleProvider::new("goog-my-secret-key".to_string(), None).unwrap();
        let dbg = format!("{:?}", provider.api_key);
        assert!(dbg.contains("goog"));
        assert!(dbg.contains("-key"));
    }

    #[test]
    fn test_google_provider_empty_api_key() {
        let provider = GoogleProvider::new("".to_string(), None).unwrap();
        let dbg = format!("{:?}", provider.api_key);
        assert_eq!(dbg, "[REDACTED]");
    }

    // ------------------------------------------------------------------
    // Ollama/LlamaCpp infer error paths with detailed assertions
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_ollama_list_models_error_is_connection_related() {
        let provider = OllamaProvider::new().await.unwrap();
        let err = provider.list_models().await.unwrap_err();
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains("error")
                || msg.contains("connect")
                || msg.contains("refused")
                || msg.contains("connection"),
            "Expected connection-related error, got: {}",
            msg
        );
    }

    #[tokio::test]
    async fn test_llama_cpp_infer_error_is_connection_related() {
        let provider = LlamaCppProvider::new().await.unwrap();
        let err = provider
            .infer(&InferenceRequest::default())
            .await
            .unwrap_err();
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains("error") || msg.contains("connect") || msg.contains("refused"),
            "Expected connection-related error, got: {}",
            msg
        );
    }

    // ------------------------------------------------------------------
    // Google provider error details
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_google_infer_error_is_connection_related() {
        let provider = GoogleProvider::new(
            "goog-fake".to_string(),
            Some("http://127.0.0.1:19999".to_string()),
        )
        .unwrap();
        let err = provider
            .infer(&InferenceRequest::default())
            .await
            .unwrap_err();
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains("error") || msg.contains("connect") || msg.contains("refused"),
            "Expected connection-related error, got: {}",
            msg
        );
    }

    #[tokio::test]
    async fn test_google_list_models_error_is_connection_related() {
        let provider = GoogleProvider::new(
            "goog-fake".to_string(),
            Some("http://127.0.0.1:19999".to_string()),
        )
        .unwrap();
        let err = provider.list_models().await.unwrap_err();
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains("error") || msg.contains("connect") || msg.contains("refused"),
            "Expected connection-related error, got: {}",
            msg
        );
    }

    // ------------------------------------------------------------------
    // Trait object collections — Arc<dyn LlmProvider> in HashMap
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_providers_in_hashmap() {
        use std::collections::HashMap;
        use std::sync::Arc;

        let mut map: HashMap<ProviderType, Arc<dyn LlmProvider>> = HashMap::new();
        map.insert(
            ProviderType::Ollama,
            Arc::new(OllamaProvider::new().await.unwrap()),
        );
        map.insert(
            ProviderType::LlamaCpp,
            Arc::new(LlamaCppProvider::new().await.unwrap()),
        );
        map.insert(
            ProviderType::OpenAi,
            Arc::new(OpenAiProvider::new("sk-test".to_string(), None).unwrap()),
        );
        map.insert(
            ProviderType::Anthropic,
            Arc::new(AnthropicProvider::new("ant-test".to_string(), None).unwrap()),
        );
        map.insert(
            ProviderType::Google,
            Arc::new(GoogleProvider::new("goog-test".to_string(), None).unwrap()),
        );

        assert_eq!(map.len(), 5);

        // All should support unload_model
        for provider in map.values() {
            assert!(provider.unload_model("x").await.is_ok());
        }
    }

    // ------------------------------------------------------------------
    // Anthropic provider: infer_stream error detail
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_anthropic_infer_stream_error_detail() {
        let provider = AnthropicProvider::new(
            "ant-fake".to_string(),
            Some("http://127.0.0.1:19999".to_string()),
        )
        .unwrap();
        let mut rx = provider
            .infer_stream(InferenceRequest::default())
            .await
            .unwrap();
        let msg = rx.recv().await.unwrap();
        let err = msg.unwrap_err();
        let err_str = err.to_string().to_lowercase();
        assert!(
            err_str.contains("error") || err_str.contains("connect") || err_str.contains("refused"),
            "Expected connection error, got: {}",
            err_str
        );
    }

    // ------------------------------------------------------------------
    // OpenAI list_models error detail
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_openai_list_models_error_detail() {
        let provider = OpenAiProvider::new(
            "sk-fake".to_string(),
            Some("http://127.0.0.1:19999".to_string()),
        )
        .unwrap();
        let err = provider.list_models().await.unwrap_err();
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains("error") || msg.contains("connect") || msg.contains("refused"),
            "Expected connection error, got: {}",
            msg
        );
    }

    // ------------------------------------------------------------------
    // Ollama/LlamaCpp infer_stream: channel sends error on connect failure
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_ollama_infer_stream_channel_sends_error() {
        let provider = OllamaProvider::new().await.unwrap();
        let mut rx = provider
            .infer_stream(InferenceRequest::default())
            .await
            .unwrap();
        let result = rx.recv().await;
        assert!(result.is_some());
        assert!(result.unwrap().is_err());
    }

    #[tokio::test]
    async fn test_llama_cpp_infer_stream_channel_sends_error() {
        let provider = LlamaCppProvider::new().await.unwrap();
        let mut rx = provider
            .infer_stream(InferenceRequest::default())
            .await
            .unwrap();
        let result = rx.recv().await;
        assert!(result.is_some());
        assert!(result.unwrap().is_err());
    }

    // ------------------------------------------------------------------
    // Google infer_stream error detail
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_google_infer_stream_error_detail() {
        let provider = GoogleProvider::new(
            "goog-fake".to_string(),
            Some("http://127.0.0.1:19999".to_string()),
        )
        .unwrap();
        let mut rx = provider
            .infer_stream(InferenceRequest::default())
            .await
            .unwrap();
        let msg = rx.recv().await.unwrap();
        let err = msg.unwrap_err();
        let err_str = err.to_string().to_lowercase();
        assert!(
            err_str.contains("error") || err_str.contains("connect") || err_str.contains("refused"),
            "Expected connection error in stream, got: {}",
            err_str
        );
    }

    // ------------------------------------------------------------------
    // RedactedKey: boundary at len=7
    // ------------------------------------------------------------------

    #[test]
    fn test_redacted_key_7_chars() {
        let key = RedactedKey("1234567".to_string());
        assert_eq!(format!("{:?}", key), "[REDACTED]");
    }

    // ------------------------------------------------------------------
    // Provider type as HashMap value
    // ------------------------------------------------------------------

    #[test]
    fn test_provider_type_clone_all_variants() {
        let variants = [
            ProviderType::Ollama,
            ProviderType::LlamaCpp,
            ProviderType::OpenAi,
            ProviderType::Anthropic,
            ProviderType::Google,
        ];
        for v in &variants {
            let cloned = *v;
            assert_eq!(*v, cloned);
        }
    }

    // ------------------------------------------------------------------
    // Additional coverage: provider construction edge cases, trait object
    // interactions, concurrent access, default request handling
    // ------------------------------------------------------------------

    #[test]
    fn test_openai_provider_base_url_with_trailing_slash() {
        let provider = OpenAiProvider::new(
            "sk-key".to_string(),
            Some("http://localhost:4000/".to_string()),
        )
        .unwrap();
        // Should store exactly as given
        assert_eq!(provider.base_url, "http://localhost:4000/");
    }

    #[test]
    fn test_anthropic_provider_base_url_with_trailing_slash() {
        let provider = AnthropicProvider::new(
            "ant-key".to_string(),
            Some("http://localhost:5000/".to_string()),
        )
        .unwrap();
        assert_eq!(provider.base_url, "http://localhost:5000/");
    }

    #[test]
    fn test_google_provider_base_url_with_trailing_slash() {
        let provider = GoogleProvider::new(
            "goog-key".to_string(),
            Some("http://localhost:6000/".to_string()),
        )
        .unwrap();
        assert_eq!(provider.base_url, "http://localhost:6000/");
    }

    #[test]
    fn test_redacted_key_exactly_boundary_len_8() {
        // 8 chars => [REDACTED], 9 chars => partial display
        let k8 = RedactedKey("abcdefgh".to_string());
        assert_eq!(format!("{:?}", k8), "[REDACTED]");
        let k9 = RedactedKey("abcdefghi".to_string());
        let dbg = format!("{:?}", k9);
        assert!(dbg.contains("abcd"));
        assert!(dbg.contains("fghi"));
        assert!(dbg.contains("..."));
    }

    #[test]
    fn test_redacted_key_unicode() {
        // Unicode keys should not panic
        let key = RedactedKey("🔑🔐🗝️🔓🔒securekeydata".to_string());
        let dbg = format!("{:?}", key);
        // length > 8 bytes, so it should show partial
        assert!(dbg.contains("...") || dbg == "[REDACTED]");
    }

    #[tokio::test]
    async fn test_ollama_provider_default_request() {
        let provider = OllamaProvider::new().await.unwrap();
        let request = InferenceRequest::default();
        // infer with default request (no server) should produce a connection error
        let err = provider.infer(&request).await.unwrap_err();
        assert!(!err.to_string().is_empty());
    }

    #[tokio::test]
    async fn test_llama_cpp_provider_default_request() {
        let provider = LlamaCppProvider::new().await.unwrap();
        let request = InferenceRequest::default();
        let err = provider.infer(&request).await.unwrap_err();
        assert!(!err.to_string().is_empty());
    }

    #[tokio::test]
    async fn test_openai_provider_empty_prompt() {
        let provider = OpenAiProvider::new(
            "sk-fake".to_string(),
            Some("http://127.0.0.1:19999".to_string()),
        )
        .unwrap();
        let request = InferenceRequest {
            prompt: "".to_string(),
            model: "gpt-4".to_string(),
            max_tokens: 10,
            temperature: 0.7,
            top_p: 1.0,
            presence_penalty: 0.0,
            frequency_penalty: 0.0,
        };
        // Should fail with connection error, not panic on empty prompt
        assert!(provider.infer(&request).await.is_err());
    }

    #[tokio::test]
    async fn test_anthropic_provider_empty_prompt() {
        let provider = AnthropicProvider::new(
            "ant-fake".to_string(),
            Some("http://127.0.0.1:19999".to_string()),
        )
        .unwrap();
        let request = InferenceRequest {
            prompt: "".to_string(),
            model: "claude-3-haiku".to_string(),
            max_tokens: 10,
            temperature: 0.5,
            top_p: 1.0,
            presence_penalty: 0.0,
            frequency_penalty: 0.0,
        };
        assert!(provider.infer(&request).await.is_err());
    }

    #[tokio::test]
    async fn test_google_provider_empty_prompt() {
        let provider = GoogleProvider::new(
            "goog-fake".to_string(),
            Some("http://127.0.0.1:19999".to_string()),
        )
        .unwrap();
        let request = InferenceRequest {
            prompt: "".to_string(),
            model: "gemini-pro".to_string(),
            max_tokens: 10,
            temperature: 0.5,
            top_p: 1.0,
            presence_penalty: 0.0,
            frequency_penalty: 0.0,
        };
        assert!(provider.infer(&request).await.is_err());
    }

    #[tokio::test]
    async fn test_anthropic_list_models_max_tokens() {
        let provider = AnthropicProvider::new("ant-key".to_string(), None).unwrap();
        let models = provider.list_models().await.unwrap();
        for model in &models {
            assert_eq!(
                model.max_tokens, 8192,
                "All Anthropic models should have max_tokens=8192"
            );
        }
    }

    #[tokio::test]
    async fn test_anthropic_list_models_size_bytes_zero() {
        let provider = AnthropicProvider::new("ant-key".to_string(), None).unwrap();
        let models = provider.list_models().await.unwrap();
        for model in &models {
            assert_eq!(
                model.size_bytes, 0,
                "Cloud models should report 0 size_bytes"
            );
        }
    }

    #[tokio::test]
    async fn test_concurrent_provider_creation() {
        let mut handles = vec![];
        for _ in 0..10 {
            handles.push(tokio::spawn(async { OllamaProvider::new().await.is_ok() }));
        }
        for handle in handles {
            let result = handle.await.unwrap();
            assert!(result, "Concurrent OllamaProvider::new() should succeed");
        }
    }

    #[tokio::test]
    async fn test_llama_cpp_concurrent_list_models() {
        let provider = std::sync::Arc::new(LlamaCppProvider::new().await.unwrap());
        let mut handles = vec![];
        for _ in 0..10 {
            let p = provider.clone();
            handles.push(tokio::spawn(async move { p.list_models().await.unwrap() }));
        }
        for handle in handles {
            let models = handle.await.unwrap();
            assert!(models.is_empty());
        }
    }

    #[tokio::test]
    async fn test_provider_trait_object_vec() {
        let providers: Vec<Box<dyn LlmProvider>> = vec![
            Box::new(OllamaProvider::new().await.unwrap()),
            Box::new(LlamaCppProvider::new().await.unwrap()),
            Box::new(OpenAiProvider::new("sk-test".to_string(), None).unwrap()),
            Box::new(AnthropicProvider::new("ant-test".to_string(), None).unwrap()),
            Box::new(GoogleProvider::new("goog-test".to_string(), None).unwrap()),
        ];
        assert_eq!(providers.len(), 5);
        // All should support unload without panic
        for p in &providers {
            assert!(p.unload_model("any").await.is_ok());
        }
    }

    #[tokio::test]
    async fn test_openai_infer_stream_receiver_closes_on_error() {
        let provider = OpenAiProvider::new(
            "sk-fake".to_string(),
            Some("http://127.0.0.1:19999".to_string()),
        )
        .unwrap();
        let mut rx = provider
            .infer_stream(InferenceRequest::default())
            .await
            .unwrap();
        // First message should be error
        let first = rx.recv().await;
        assert!(first.is_some());
        assert!(first.unwrap().is_err());
        // Channel should be closed after error
        let second = rx.recv().await;
        assert!(second.is_none(), "Channel should close after error is sent");
    }

    #[tokio::test]
    async fn test_anthropic_infer_stream_receiver_closes_on_error() {
        let provider = AnthropicProvider::new(
            "ant-fake".to_string(),
            Some("http://127.0.0.1:19999".to_string()),
        )
        .unwrap();
        let mut rx = provider
            .infer_stream(InferenceRequest::default())
            .await
            .unwrap();
        let first = rx.recv().await;
        assert!(first.is_some());
        assert!(first.unwrap().is_err());
        let second = rx.recv().await;
        assert!(second.is_none(), "Channel should close after error is sent");
    }

    #[tokio::test]
    async fn test_google_infer_stream_receiver_closes_on_error() {
        let provider = GoogleProvider::new(
            "goog-fake".to_string(),
            Some("http://127.0.0.1:19999".to_string()),
        )
        .unwrap();
        let mut rx = provider
            .infer_stream(InferenceRequest::default())
            .await
            .unwrap();
        let first = rx.recv().await;
        assert!(first.is_some());
        assert!(first.unwrap().is_err());
        let second = rx.recv().await;
        assert!(second.is_none(), "Channel should close after error is sent");
    }

    #[test]
    fn test_openai_provider_with_very_long_api_key() {
        let long_key = "sk-".to_string() + &"a".repeat(1000);
        let provider = OpenAiProvider::new(long_key, None);
        assert!(provider.is_ok());
        let dbg = format!("{:?}", provider.unwrap().api_key);
        assert!(dbg.contains("sk-a"));
        assert!(dbg.contains("..."));
    }

    #[test]
    fn test_google_provider_with_very_long_api_key() {
        let long_key = "goog-".to_string() + &"b".repeat(500);
        let provider = GoogleProvider::new(long_key, None);
        assert!(provider.is_ok());
        let dbg = format!("{:?}", provider.unwrap().api_key);
        assert!(dbg.contains("..."));
    }

    #[tokio::test]
    async fn test_ollama_load_model_returns_correct_fields() {
        // load_model will fail without server, but test the error path
        let provider = OllamaProvider::new().await.unwrap();
        let result = provider.load_model("llama2:7b").await;
        // Ollama load_model makes HTTP request, will fail without server
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_google_load_model_error_message_exact() {
        let provider = GoogleProvider::new("goog-key".to_string(), None).unwrap();
        let err = provider.load_model("gemini-2.0-flash").await.unwrap_err();
        assert_eq!(
            err.to_string(),
            "Google models are cloud-managed and cannot be loaded locally"
        );
    }

    #[tokio::test]
    async fn test_anthropic_unload_empty_model_id() {
        let provider = AnthropicProvider::new("ant-key".to_string(), None).unwrap();
        assert!(provider.unload_model("").await.is_ok());
    }

    #[tokio::test]
    async fn test_google_unload_empty_model_id() {
        let provider = GoogleProvider::new("goog-key".to_string(), None).unwrap();
        assert!(provider.unload_model("").await.is_ok());
    }

    // -----------------------------------------------------------------------
    // OpenAI-compatible provider tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_provider_type_all_14_variants_distinct() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(ProviderType::Ollama);
        set.insert(ProviderType::LlamaCpp);
        set.insert(ProviderType::OpenAi);
        set.insert(ProviderType::Anthropic);
        set.insert(ProviderType::Google);
        set.insert(ProviderType::DeepSeek);
        set.insert(ProviderType::Mistral);
        set.insert(ProviderType::Grok);
        set.insert(ProviderType::Groq);
        set.insert(ProviderType::OpenRouter);
        set.insert(ProviderType::LmStudio);
        set.insert(ProviderType::LocalAi);
        set.insert(ProviderType::OpenCode);
        set.insert(ProviderType::Letta);
        assert_eq!(set.len(), 14);
    }

    #[test]
    fn test_provider_type_new_variants_debug() {
        assert_eq!(format!("{:?}", ProviderType::DeepSeek), "DeepSeek");
        assert_eq!(format!("{:?}", ProviderType::Mistral), "Mistral");
        assert_eq!(format!("{:?}", ProviderType::Grok), "Grok");
        assert_eq!(format!("{:?}", ProviderType::Groq), "Groq");
        assert_eq!(format!("{:?}", ProviderType::OpenRouter), "OpenRouter");
        assert_eq!(format!("{:?}", ProviderType::LmStudio), "LmStudio");
        assert_eq!(format!("{:?}", ProviderType::LocalAi), "LocalAi");
        assert_eq!(format!("{:?}", ProviderType::OpenCode), "OpenCode");
        assert_eq!(format!("{:?}", ProviderType::Letta), "Letta");
    }

    #[test]
    fn test_deepseek_provider_new() {
        let provider = new_deepseek_provider("ds-key-123".to_string(), None);
        assert!(provider.is_ok());
        let p = provider.unwrap();
        assert_eq!(p.base_url, "https://api.deepseek.com/v1");
        assert_eq!(p.config.provider_name, "DeepSeek");
    }

    #[test]
    fn test_deepseek_provider_custom_url() {
        let provider = new_deepseek_provider(
            "ds-key".to_string(),
            Some("http://custom:9000/v1".to_string()),
        );
        assert!(provider.is_ok());
        assert_eq!(provider.unwrap().base_url, "http://custom:9000/v1");
    }

    #[test]
    fn test_mistral_provider_new() {
        let provider = new_mistral_provider("mist-key".to_string(), None);
        assert!(provider.is_ok());
        let p = provider.unwrap();
        assert_eq!(p.base_url, "https://api.mistral.ai/v1");
        assert_eq!(p.config.provider_name, "Mistral");
    }

    #[test]
    fn test_grok_provider_new() {
        let provider = new_grok_provider("xai-key".to_string(), None);
        assert!(provider.is_ok());
        let p = provider.unwrap();
        assert_eq!(p.base_url, "https://api.x.ai/v1");
        assert_eq!(p.config.provider_name, "Grok");
    }

    #[test]
    fn test_groq_provider_new() {
        let provider = new_groq_provider("groq-key".to_string(), None);
        assert!(provider.is_ok());
        let p = provider.unwrap();
        assert_eq!(p.base_url, "https://api.groq.com/openai/v1");
        assert_eq!(p.config.provider_name, "Groq");
    }

    #[test]
    fn test_openrouter_provider_new() {
        let provider = new_openrouter_provider("or-key".to_string(), None);
        assert!(provider.is_ok());
        let p = provider.unwrap();
        assert_eq!(p.base_url, "https://openrouter.ai/api/v1");
    }

    #[test]
    fn test_lmstudio_provider_new() {
        let provider = new_lmstudio_provider(None);
        assert!(provider.is_ok());
        let p = provider.unwrap();
        assert_eq!(p.base_url, "http://localhost:1234/v1");
        assert!(p.api_key.is_none());
    }

    #[test]
    fn test_lmstudio_provider_custom_url() {
        let provider = new_lmstudio_provider(Some("http://192.168.1.5:1234/v1".to_string()));
        assert!(provider.is_ok());
        assert_eq!(
            provider.unwrap().base_url,
            "http://192.168.1.5:1234/v1"
        );
    }

    #[test]
    fn test_localai_provider_new() {
        let provider = new_localai_provider(None);
        assert!(provider.is_ok());
        let p = provider.unwrap();
        assert_eq!(p.base_url, "http://localhost:8080/v1");
        assert!(p.api_key.is_none());
    }

    #[test]
    fn test_opencode_provider_new() {
        let provider = new_opencode_provider("oc-key".to_string(), None);
        assert!(provider.is_ok());
        let p = provider.unwrap();
        assert_eq!(p.base_url, "https://api.open-code.dev/v1");
    }

    #[test]
    fn test_letta_provider_with_api_key() {
        let provider = new_letta_provider(Some("letta-key".to_string()), None);
        assert!(provider.is_ok());
        let p = provider.unwrap();
        assert!(p.api_key.is_some());
    }

    #[test]
    fn test_openai_compatible_requires_api_key_rejects_none() {
        let config = OpenAiCompatibleConfig {
            provider_name: "TestProvider",
            provider_type: ProviderType::DeepSeek,
            common_provider: agnos_common::Provider::DeepSeek,
            default_base_url: "http://localhost:9999",
            default_max_tokens: 4096,
            known_models: &[],
            requires_api_key: true,
        };
        let result = OpenAiCompatibleProvider::new(config, None, None);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("requires an API key"));
    }

    #[test]
    fn test_openai_compatible_no_key_required_accepts_none() {
        let config = OpenAiCompatibleConfig {
            provider_name: "LocalTest",
            provider_type: ProviderType::LmStudio,
            common_provider: agnos_common::Provider::LmStudio,
            default_base_url: "http://localhost:1234/v1",
            default_max_tokens: 4096,
            known_models: &[],
            requires_api_key: false,
        };
        let result = OpenAiCompatibleProvider::new(config, None, None);
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_deepseek_known_models() {
        let provider = new_deepseek_provider("ds-key-123456789".to_string(), None).unwrap();
        // list_models will fail to reach the API but should fall back to known models
        let models = provider.list_models().await.unwrap();
        assert_eq!(models.len(), 3);
        assert_eq!(models[0].id, "deepseek-chat");
        assert_eq!(models[0].provider, agnos_common::Provider::DeepSeek);
    }

    #[tokio::test]
    async fn test_mistral_known_models() {
        let provider = new_mistral_provider("mist-key-123456789".to_string(), None).unwrap();
        let models = provider.list_models().await.unwrap();
        assert_eq!(models.len(), 5);
        assert_eq!(models[0].id, "mistral-large-latest");
    }

    #[tokio::test]
    async fn test_grok_known_models() {
        let provider = new_grok_provider("xai-key-123456789".to_string(), None).unwrap();
        let models = provider.list_models().await.unwrap();
        assert_eq!(models.len(), 4);
        assert_eq!(models[0].id, "grok-3");
    }

    #[tokio::test]
    async fn test_groq_known_models() {
        let provider = new_groq_provider("groq-key-123456789".to_string(), None).unwrap();
        let models = provider.list_models().await.unwrap();
        assert_eq!(models.len(), 4);
        assert_eq!(models[0].id, "llama-3.3-70b-versatile");
    }

    #[tokio::test]
    async fn test_opencode_known_models() {
        let provider = new_opencode_provider("oc-key-123456789".to_string(), None).unwrap();
        let models = provider.list_models().await.unwrap();
        assert_eq!(models.len(), 5);
    }

    #[tokio::test]
    async fn test_openrouter_known_models_fallback() {
        // OpenRouter has no hardcoded known models; falls back to /models endpoint or empty
        let provider = new_openrouter_provider("or-key-123456789".to_string(), None).unwrap();
        let models = provider.list_models().await.unwrap();
        // May be empty (no known models fallback) or populated (API responded)
        // Just verify it doesn't error — result depends on network availability
        let _ = models.len();
    }

    #[tokio::test]
    async fn test_lmstudio_load_model_error() {
        let provider = new_lmstudio_provider(None).unwrap();
        let err = provider.load_model("test").await.unwrap_err();
        assert!(err.to_string().contains("LM Studio"));
    }

    #[tokio::test]
    async fn test_localai_unload_is_ok() {
        let provider = new_localai_provider(None).unwrap();
        assert!(provider.unload_model("any").await.is_ok());
    }

    #[tokio::test]
    async fn test_openai_compatible_infer_stream_returns_receiver() {
        let provider = new_lmstudio_provider(None).unwrap();
        let request = InferenceRequest::default();
        let rx = provider.infer_stream(request).await;
        assert!(rx.is_ok());
    }

    #[test]
    fn test_provider_type_new_ne_old() {
        assert_ne!(ProviderType::DeepSeek, ProviderType::OpenAi);
        assert_ne!(ProviderType::Mistral, ProviderType::Anthropic);
        assert_ne!(ProviderType::Grok, ProviderType::Google);
        assert_ne!(ProviderType::Groq, ProviderType::Grok);
        assert_ne!(ProviderType::LmStudio, ProviderType::LlamaCpp);
        assert_ne!(ProviderType::LocalAi, ProviderType::Ollama);
    }

    #[test]
    fn test_all_provider_types_as_hashmap_keys() {
        use std::collections::HashMap;
        let mut map: HashMap<ProviderType, &str> = HashMap::new();
        map.insert(ProviderType::Ollama, "ollama");
        map.insert(ProviderType::LlamaCpp, "llamacpp");
        map.insert(ProviderType::OpenAi, "openai");
        map.insert(ProviderType::Anthropic, "anthropic");
        map.insert(ProviderType::Google, "google");
        map.insert(ProviderType::DeepSeek, "deepseek");
        map.insert(ProviderType::Mistral, "mistral");
        map.insert(ProviderType::Grok, "grok");
        map.insert(ProviderType::Groq, "groq");
        map.insert(ProviderType::OpenRouter, "openrouter");
        map.insert(ProviderType::LmStudio, "lmstudio");
        map.insert(ProviderType::LocalAi, "localai");
        map.insert(ProviderType::OpenCode, "opencode");
        map.insert(ProviderType::Letta, "letta");
        assert_eq!(map.len(), 14);
    }

    #[tokio::test]
    async fn test_mistral_custom_url() {
        let provider = new_mistral_provider(
            "mist-key".to_string(),
            Some("http://custom:5000".to_string()),
        )
        .unwrap();
        assert_eq!(provider.base_url, "http://custom:5000");
    }

    #[tokio::test]
    async fn test_groq_custom_url() {
        let provider =
            new_groq_provider("groq-key".to_string(), Some("http://custom:6000".to_string()))
                .unwrap();
        assert_eq!(provider.base_url, "http://custom:6000");
    }

    #[tokio::test]
    async fn test_grok_custom_url() {
        let provider =
            new_grok_provider("xai-key".to_string(), Some("http://custom:7000".to_string()))
                .unwrap();
        assert_eq!(provider.base_url, "http://custom:7000");
    }

    #[tokio::test]
    async fn test_letta_known_models() {
        let provider = new_letta_provider(Some("letta-key-123456789".to_string()), None).unwrap();
        let models = provider.list_models().await.unwrap();
        assert_eq!(models.len(), 3);
        assert_eq!(models[0].id, "openai/gpt-4o");
        assert_eq!(models[0].provider, agnos_common::Provider::Letta);
    }

    #[test]
    fn test_openai_compatible_config_clone() {
        let config = OpenAiCompatibleConfig {
            provider_name: "Test",
            provider_type: ProviderType::DeepSeek,
            common_provider: agnos_common::Provider::DeepSeek,
            default_base_url: "http://test",
            default_max_tokens: 4096,
            known_models: &[("m1", "Model 1")],
            requires_api_key: true,
        };
        let cloned = config.clone();
        assert_eq!(cloned.provider_name, "Test");
        assert_eq!(cloned.known_models.len(), 1);
    }
}
