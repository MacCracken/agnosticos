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

use crate::accounting::TokenAccounting;
use crate::cache::ResponseCache;
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
        request: InferenceRequest,
        agent_id: Option<AgentId>,
    ) -> Result<InferenceResponse> {
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
    let gateway = Arc::new(LlmGateway::new(config).await?);
    
    gateway.init_providers().await?;

    info!("LLM Gateway daemon started successfully");

    // Keep running until shutdown signal
    tokio::signal::ctrl_c().await?;
    
    info!("Shutting down LLM Gateway daemon...");
    Ok(())
}

async fn list_models() -> Result<()> {
    println!("Available models:");
    println!("  (none loaded - run 'llm-gateway daemon' first)");
    Ok(())
}

async fn load_model(model_id: &str) -> Result<()> {
    println!("Loading model: {}", model_id);
    Ok(())
}

async fn unload_model(model_id: &str) -> Result<()> {
    println!("Unloading model: {}", model_id);
    Ok(())
}

async fn run_inference(model: Option<String>, prompt: String) -> Result<()> {
    let request = InferenceRequest {
        prompt,
        model: model.unwrap_or_else(|| "default".to_string()),
        ..Default::default()
    };

    println!("Running inference with model: {}", request.model);
    println!("Prompt: {}", request.prompt);
    
    Ok(())
}

async fn show_stats() -> Result<()> {
    println!("Token usage statistics:");
    println!("  (no data available)");
    Ok(())
}
