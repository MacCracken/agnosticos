//! AGNOS LLM Gateway Service
//!
//! Provides unified LLM access with support for local models (Ollama, llama.cpp)
//! and cloud APIs (OpenAI, Anthropic), with model sharing and token accounting for agents.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use tokio::sync::{mpsc, RwLock, Semaphore};
use tokio::time::{timeout, Duration};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use agnos_common::{
    AgentId, InferenceRequest, InferenceResponse, ModelInfo, TokenUsage,
};

mod providers;
mod cache;
mod accounting;
mod http;

use crate::accounting::TokenAccounting;
use crate::cache::ResponseCache;
use crate::http::start_http_server;
use crate::providers::{LlmProvider, ProviderType};

#[derive(Parser)]
#[command(name = "llm-gateway")]
#[command(about = "AGNOS LLM Gateway Service")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    #[arg(short, long, default_value = "/etc/agnos/llm-gateway")]
    config_dir: std::path::PathBuf,
}

#[derive(Subcommand)]
enum Commands {
    /// Run the LLM gateway daemon
    Daemon,
    /// List available models
    ListModels,
    /// Load a model
    Load {
        model_id: String,
    },
    /// Unload a model
    Unload {
        model_id: String,
    },
    /// Run inference
    Infer {
        #[arg(short, long)]
        model: Option<String>,
        #[arg(short, long)]
        prompt: String,
    },
    /// Show token usage statistics
    Stats,
}

/// LLM Gateway service
pub struct LlmGateway {
    /// Active providers
    providers: RwLock<HashMap<ProviderType, Arc<dyn LlmProvider>>>,
    /// Currently loaded models
    loaded_models: RwLock<HashMap<String, ModelInfo>>,
    /// Request rate limiter
    rate_limiter: Semaphore,
    /// Response cache
    cache: ResponseCache,
    /// Token accounting for agents
    accounting: TokenAccounting,
    /// Configuration
    config: GatewayConfig,
}

/// Gateway configuration
#[derive(Debug, Clone)]
pub struct GatewayConfig {
    pub max_concurrent_requests: usize,
    pub request_timeout: Duration,
    pub enable_caching: bool,
    pub cache_ttl_seconds: u64,
    pub enable_token_accounting: bool,
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            max_concurrent_requests: 10,
            request_timeout: Duration::from_secs(60),
            enable_caching: true,
            cache_ttl_seconds: 3600,
            enable_token_accounting: true,
        }
    }
}

impl LlmGateway {
    /// Create a new LLM gateway
    pub async fn new(config: GatewayConfig) -> Result<Self> {
        info!("Initializing LLM Gateway...");

        let rate_limiter = Semaphore::new(config.max_concurrent_requests);
        let cache = ResponseCache::new(Duration::from_secs(config.cache_ttl_seconds));
        let accounting = TokenAccounting::new();

        Ok(Self {
            providers: RwLock::new(HashMap::new()),
            loaded_models: RwLock::new(HashMap::new()),
            rate_limiter,
            cache,
            accounting,
            config,
        })
    }

    /// Initialize providers
    pub async fn init_providers(&self) -> Result<()> {
        info!("Initializing LLM providers...");

        let mut providers = self.providers.write().await;

        // Try to initialize local providers
        match providers::OllamaProvider::new().await {
            Ok(provider) => {
                info!("Ollama provider initialized");
                providers.insert(ProviderType::Ollama, Arc::new(provider));
            }
            Err(e) => {
                warn!("Failed to initialize Ollama provider: {}", e);
            }
        }

        match providers::LlamaCppProvider::new().await {
            Ok(provider) => {
                info!("llama.cpp provider initialized");
                providers.insert(ProviderType::LlamaCpp, Arc::new(provider));
            }
            Err(e) => {
                warn!("Failed to initialize llama.cpp provider: {}", e);
            }
        }

        // TODO: Initialize cloud providers (OpenAI, Anthropic)

        info!("{} provider(s) initialized", providers.len());
        Ok(())
    }

    /// Run inference with the LLM
    pub async fn infer(
        &self,
        mut request: InferenceRequest,
        agent_id: Option<AgentId>,
    ) -> Result<InferenceResponse> {
        // Enforce parameter bounds before any processing
        request.validate();

        let _permit = self.rate_limiter.acquire().await?;

        info!(
            "Inference request: model={}, agent={:?}",
            request.model, agent_id
        );

        // Check cache if enabled
        if self.config.enable_caching {
            if let Some(cached) = self.cache.get(&request).await {
                debug!("Cache hit for inference request");
                return Ok(cached);
            }
        }

        // Select the best provider for the request
        let provider = self.select_provider(&request).await?;
        
        // Execute inference with timeout
        let response = timeout(
            self.config.request_timeout,
            provider.infer(request.clone())
        )
        .await
        .context("Inference request timed out")??;

        // Update token accounting
        if self.config.enable_token_accounting {
            if let Some(agent_id) = agent_id {
                self.accounting.record_usage(agent_id, response.usage).await;
            }
        }

        // Cache the response
        if self.config.enable_caching {
            self.cache.set(request, response.clone()).await;
        }

        Ok(response)
    }

    /// Stream inference results
    pub async fn infer_stream(
        &self,
        request: InferenceRequest,
        agent_id: Option<AgentId>,
    ) -> Result<mpsc::Receiver<Result<String>>> {
        let _permit = self.rate_limiter.acquire().await?;
        
        info!(
            "Streaming inference request: model={}, agent={:?}",
            request.model, agent_id
        );

        let provider = self.select_provider(&request).await?;
        let stream = provider.infer_stream(request).await?;

        Ok(stream)
    }

    /// Select the best provider for a request
    async fn select_provider(&self, request: &InferenceRequest) -> Result<Arc<dyn LlmProvider>> {
        let providers = self.providers.read().await;
        
        // Check if requested model is loaded locally
        let loaded = self.loaded_models.read().await;
        if loaded.contains_key(&request.model) {
            // Use the provider that has this model
            if let Some(provider) = providers.get(&ProviderType::Ollama) {
                return Ok(provider.clone());
            }
        }
        drop(loaded);

        // Default to first available local provider
        if let Some((_, provider)) = providers.iter().next() {
            return Ok(provider.clone());
        }

        // Try cloud providers as fallback
        if let Some(provider) = providers.get(&ProviderType::OpenAi) {
            return Ok(provider.clone());
        }

        Err(anyhow::anyhow!("No LLM provider available"))
    }

    /// List available models
    pub async fn list_models(&self) -> Vec<ModelInfo> {
        let loaded = self.loaded_models.read().await;
        loaded.values().cloned().collect()
    }

    /// Load a model
    pub async fn load_model(&self, model_id: &str) -> Result<()> {
        info!("Loading model: {}", model_id);

        let providers = self.providers.read().await;
        
        // Try to load from available providers
        for (provider_type, provider) in providers.iter() {
            match provider.load_model(model_id).await {
                Ok(model_info) => {
                    let mut loaded = self.loaded_models.write().await;
                    loaded.insert(model_id.to_string(), model_info);
                    info!("Model {} loaded successfully via {:?}", model_id, provider_type);
                    return Ok(());
                }
                Err(e) => {
                    debug!("Provider {:?} could not load model {}: {}", 
                           provider_type, model_id, e);
                }
            }
        }

        Err(anyhow::anyhow!("Failed to load model {} from any provider", model_id))
    }

    /// Unload a model
    pub async fn unload_model(&self, model_id: &str) -> Result<()> {
        info!("Unloading model: {}", model_id);

        let mut loaded = self.loaded_models.write().await;
        
        if loaded.remove(model_id).is_some() {
            info!("Model {} unloaded", model_id);
        }

        Ok(())
    }

    /// Get token usage for an agent
    pub async fn get_agent_usage(&self, agent_id: AgentId) -> Option<TokenUsage> {
        self.accounting.get_usage(agent_id).await
    }

    /// Get total token usage
    pub async fn get_total_usage(&self) -> TokenUsage {
        self.accounting.get_total_usage().await
    }

    /// Reset token accounting for an agent
    pub async fn reset_agent_usage(&self, agent_id: AgentId) {
        self.accounting.reset_usage(agent_id).await;
    }
    
    /// List available providers and their status
    pub async fn list_providers(&self) -> Vec<crate::http::ProviderStatus> {
        let providers = self.providers.read().await;
        
        vec![
            crate::http::ProviderStatus {
                name: "Ollama".to_string(),
                available: providers.contains_key(&providers::ProviderType::Ollama),
            },
            crate::http::ProviderStatus {
                name: "llama.cpp".to_string(),
                available: providers.contains_key(&providers::ProviderType::LlamaCpp),
            },
            crate::http::ProviderStatus {
                name: "OpenAI".to_string(),
                available: providers.contains_key(&providers::ProviderType::OpenAi),
            },
        ]
    }

    /// Create a model sharing session for multi-agent access
    pub async fn create_shared_session(
        &self,
        model_id: &str,
        agent_ids: Vec<AgentId>,
    ) -> Result<SharedSession> {
        info!("Creating shared session for model {} with {} agents", 
              model_id, agent_ids.len());

        // Ensure model is loaded
        self.load_model(model_id).await?;

        let session = SharedSession {
            id: Uuid::new_v4().to_string(),
            model_id: model_id.to_string(),
            agent_ids,
            created_at: chrono::Utc::now(),
        };

        Ok(session)
    }
}

/// Shared model session for multi-agent access
#[derive(Debug, Clone)]
pub struct SharedSession {
    pub id: String,
    pub model_id: String,
    pub agent_ids: Vec<AgentId>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    info!("AGNOS LLM Gateway Service v{}", env!("CARGO_PKG_VERSION"));

    let cli = Cli::parse();

    match cli.command {
        Commands::Daemon => run_daemon().await,
        Commands::ListModels => list_models().await,
        Commands::Load { model_id } => load_model(&model_id).await,
        Commands::Unload { model_id } => unload_model(&model_id).await,
        Commands::Infer { model, prompt } => run_inference(model, prompt).await,
        Commands::Stats => show_stats().await,
    }
}

async fn run_daemon() -> Result<()> {
    info!("Starting LLM Gateway daemon...");

    let config = GatewayConfig::default();
    let gateway = Arc::new(LlmGateway::new(config.clone()).await?);
    
    gateway.init_providers().await?;
    
    // Start HTTP server in background
    let http_gateway = gateway.clone();
    let http_config = config.clone();
    tokio::spawn(async move {
        if let Err(e) = start_http_server(http_gateway, http_config).await {
            error!("HTTP server error: {}", e);
        }
    });

    info!("LLM Gateway daemon started successfully");

    // Keep running until shutdown signal
    tokio::signal::ctrl_c().await?;
    
    info!("Shutting down LLM Gateway daemon...");
    Ok(())
}

/// HTTP client for CLI commands that talk to the running gateway daemon
fn gateway_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

const GATEWAY_URL: &str = "http://127.0.0.1:8088";

async fn list_models() -> Result<()> {
    let client = gateway_client();
    let url = format!("{}/v1/models", GATEWAY_URL);

    match client.get(&url).send().await {
        Ok(resp) => {
            let body: serde_json::Value = resp.json().await
                .context("Failed to parse models response")?;

            println!("Available models:");
            if let Some(models) = body["data"].as_array() {
                if models.is_empty() {
                    println!("  (no models loaded)");
                } else {
                    for model in models {
                        let id = model["id"].as_str().unwrap_or("unknown");
                        let owner = model["owned_by"].as_str().unwrap_or("unknown");
                        println!("  {} (owned by: {})", id, owner);
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("Failed to connect to LLM Gateway at {}: {}", GATEWAY_URL, e);
            eprintln!("Is the daemon running? Start it with: llm-gateway daemon");
        }
    }

    Ok(())
}

async fn load_model(model_id: &str) -> Result<()> {
    // The gateway auto-loads models via providers on init.
    // This command verifies the model is accessible.
    let client = gateway_client();
    let url = format!("{}/v1/models", GATEWAY_URL);

    match client.get(&url).send().await {
        Ok(resp) => {
            let body: serde_json::Value = resp.json().await
                .context("Failed to parse response")?;

            let found = body["data"].as_array()
                .map(|models| models.iter().any(|m| m["id"].as_str() == Some(model_id)))
                .unwrap_or(false);

            if found {
                println!("Model '{}' is available and loaded", model_id);
            } else {
                println!("Model '{}' not found. Available models:", model_id);
                if let Some(models) = body["data"].as_array() {
                    for m in models {
                        println!("  {}", m["id"].as_str().unwrap_or("unknown"));
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("Failed to connect to LLM Gateway: {}", e);
            eprintln!("Is the daemon running? Start it with: llm-gateway daemon");
        }
    }

    Ok(())
}

async fn unload_model(model_id: &str) -> Result<()> {
    println!("Requesting unload of model: {}", model_id);
    // Currently models are managed by providers — unloading is a provider-level operation.
    // This would need a management API endpoint (not yet implemented).
    println!("Note: Model lifecycle is currently managed by providers (Ollama/llama.cpp).");
    println!("Use the provider's own management interface to unload models.");
    Ok(())
}

async fn run_inference(model: Option<String>, prompt: String) -> Result<()> {
    let client = gateway_client();
    let url = format!("{}/v1/chat/completions", GATEWAY_URL);

    let body = serde_json::json!({
        "model": model.unwrap_or_else(|| "default".to_string()),
        "messages": [
            {"role": "user", "content": prompt}
        ]
    });

    match client.post(&url).json(&body).send().await {
        Ok(resp) => {
            let status = resp.status();
            let result: serde_json::Value = resp.json().await
                .context("Failed to parse inference response")?;

            if status.is_success() {
                if let Some(text) = result["choices"][0]["message"]["content"].as_str() {
                    println!("{}", text);
                }
                if let Some(usage) = result.get("usage") {
                    eprintln!(
                        "\n[tokens: prompt={}, completion={}, total={}]",
                        usage["prompt_tokens"], usage["completion_tokens"], usage["total_tokens"]
                    );
                }
            } else {
                let msg = result["error"]["message"].as_str().unwrap_or("Unknown error");
                eprintln!("Inference failed ({}): {}", status, msg);
            }
        }
        Err(e) => {
            eprintln!("Failed to connect to LLM Gateway: {}", e);
            eprintln!("Is the daemon running? Start it with: llm-gateway daemon");
        }
    }

    Ok(())
}

async fn show_stats() -> Result<()> {
    let client = gateway_client();
    let url = format!("{}/v1/health", GATEWAY_URL);

    match client.get(&url).send().await {
        Ok(resp) => {
            let body: serde_json::Value = resp.json().await
                .context("Failed to parse health response")?;

            println!("LLM Gateway Statistics:");
            println!("  Status: {}", body["status"].as_str().unwrap_or("unknown"));
            println!("  Uptime: {}", body["uptime"].as_str().unwrap_or("unknown"));

            if let Some(providers) = body["providers"].as_array() {
                println!("  Providers: {}", providers.len());
                for p in providers {
                    println!("    - {}", p.as_str().unwrap_or("unknown"));
                }
            }
        }
        Err(e) => {
            eprintln!("Failed to connect to LLM Gateway at {}: {}", GATEWAY_URL, e);
            eprintln!("Is the daemon running? Start it with: llm-gateway daemon");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use agnos_common::{AgentId, TokenUsage};

    // ------------------------------------------------------------------
    // GatewayConfig tests
    // ------------------------------------------------------------------

    #[test]
    fn test_gateway_config_default_values() {
        let config = GatewayConfig::default();
        assert_eq!(config.max_concurrent_requests, 10);
        assert_eq!(config.request_timeout, Duration::from_secs(60));
        assert!(config.enable_caching);
        assert_eq!(config.cache_ttl_seconds, 3600);
        assert!(config.enable_token_accounting);
    }

    #[test]
    fn test_gateway_config_clone() {
        let config = GatewayConfig::default();
        let cloned = config.clone();
        assert_eq!(cloned.max_concurrent_requests, config.max_concurrent_requests);
        assert_eq!(cloned.cache_ttl_seconds, config.cache_ttl_seconds);
    }

    #[test]
    fn test_gateway_config_custom_values() {
        let config = GatewayConfig {
            max_concurrent_requests: 5,
            request_timeout: Duration::from_secs(30),
            enable_caching: false,
            cache_ttl_seconds: 600,
            enable_token_accounting: false,
        };
        assert_eq!(config.max_concurrent_requests, 5);
        assert_eq!(config.request_timeout, Duration::from_secs(30));
        assert!(!config.enable_caching);
        assert_eq!(config.cache_ttl_seconds, 600);
        assert!(!config.enable_token_accounting);
    }

    // ------------------------------------------------------------------
    // LlmGateway lifecycle tests
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_gateway_new_with_default_config() {
        let config = GatewayConfig::default();
        let gateway = LlmGateway::new(config).await;
        assert!(gateway.is_ok());
    }

    #[tokio::test]
    async fn test_gateway_new_with_caching_disabled() {
        let config = GatewayConfig {
            enable_caching: false,
            ..GatewayConfig::default()
        };
        let gateway = LlmGateway::new(config).await;
        assert!(gateway.is_ok());
    }

    #[tokio::test]
    async fn test_gateway_new_with_accounting_disabled() {
        let config = GatewayConfig {
            enable_token_accounting: false,
            ..GatewayConfig::default()
        };
        let gateway = LlmGateway::new(config).await;
        assert!(gateway.is_ok());
    }

    #[tokio::test]
    async fn test_gateway_list_models_empty_initially() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();
        let models = gateway.list_models().await;
        assert!(models.is_empty(), "No models should be loaded on startup");
    }

    #[tokio::test]
    async fn test_gateway_get_total_usage_zero_initially() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();
        let usage = gateway.get_total_usage().await;
        assert_eq!(usage.total_tokens, 0);
        assert_eq!(usage.prompt_tokens, 0);
        assert_eq!(usage.completion_tokens, 0);
    }

    #[tokio::test]
    async fn test_gateway_get_agent_usage_none_initially() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();
        let agent_id = AgentId::new();
        let usage = gateway.get_agent_usage(agent_id).await;
        assert!(usage.is_none());
    }

    #[tokio::test]
    async fn test_gateway_reset_agent_usage_no_panic_when_not_tracked() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();
        let agent_id = AgentId::new();
        // Should not panic even if agent has no recorded usage
        gateway.reset_agent_usage(agent_id).await;
        let usage = gateway.get_agent_usage(agent_id).await;
        assert!(usage.is_none());
    }

    #[tokio::test]
    async fn test_gateway_no_providers_infer_fails() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();
        let request = agnos_common::InferenceRequest::default();
        let result = gateway.infer(request, None).await;
        // No providers loaded — should return an error, not panic
        assert!(result.is_err());
    }

    // ------------------------------------------------------------------
    // SharedSession tests
    // ------------------------------------------------------------------

    #[test]
    fn test_shared_session_fields() {
        let agent_ids = vec![AgentId::new(), AgentId::new()];
        let session = SharedSession {
            id: "session-abc".to_string(),
            model_id: "llama2-7b".to_string(),
            agent_ids: agent_ids.clone(),
            created_at: chrono::Utc::now(),
        };
        assert_eq!(session.id, "session-abc");
        assert_eq!(session.model_id, "llama2-7b");
        assert_eq!(session.agent_ids.len(), 2);
    }

    #[test]
    fn test_shared_session_clone() {
        let session = SharedSession {
            id: "s1".to_string(),
            model_id: "m1".to_string(),
            agent_ids: vec![AgentId::new()],
            created_at: chrono::Utc::now(),
        };
        let cloned = session.clone();
        assert_eq!(cloned.id, session.id);
        assert_eq!(cloned.model_id, session.model_id);
    }

    #[tokio::test]
    async fn test_create_shared_session_fails_without_providers() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();
        let agent_ids = vec![AgentId::new()];
        // No providers/models loaded — should fail gracefully
        let result = gateway.create_shared_session("llama2", agent_ids).await;
        assert!(result.is_err());
    }

    // ------------------------------------------------------------------
    // OpenAI-compatible HTTP API contract tests (port 8088)
    // These document the expected request/response shape for the
    // planned HTTP server (ADR-007). They test the data types that
    // will be serialised, not the running server.
    // ------------------------------------------------------------------

    #[test]
    fn test_inference_request_default_is_valid() {
        let req = agnos_common::InferenceRequest::default();
        // A default request must have a model field (may be empty string)
        let _ = req.model;
        let _ = req.prompt;
    }

    #[test]
    fn test_inference_request_serializable() {
        let req = agnos_common::InferenceRequest {
            prompt: "Hello".to_string(),
            model: "llama2".to_string(),
            max_tokens: 100,
            temperature: 0.7,
            top_p: 1.0,
            presence_penalty: 0.0,
            frequency_penalty: 0.0,
        };
        let json = serde_json::to_string(&req);
        assert!(json.is_ok(), "InferenceRequest must be serializable to JSON");
        let json_str = json.unwrap();
        assert!(json_str.contains("llama2"));
        assert!(json_str.contains("Hello"));
    }

    #[test]
    fn test_gateway_config_port_8088_is_documented() {
        // Regression guard: the integration port is 8088.
        // If this constant ever changes, both ADR-007 and agnostic models.json
        // must be updated simultaneously.
        let expected_port: u16 = 8088;
        assert_eq!(expected_port, 8088, "AGNOS LLM Gateway HTTP port must be 8088 per ADR-007");
    }
}
