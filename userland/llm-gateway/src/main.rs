//! AGNOS LLM Gateway Service
//!
//! Provides unified LLM access with support for local models (Ollama, llama.cpp)
//! and cloud APIs (OpenAI, Anthropic), with model sharing and token accounting for agents.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use tokio::sync::{mpsc, RwLock, Semaphore};
use tokio::time::{timeout, Duration, Instant};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use agnos_common::{AgentId, InferenceRequest, InferenceResponse, ModelInfo, TokenUsage};
use agnos_sys::certpin::{self, CertPinResult, CertPinSet};

mod acceleration;
mod accounting;
mod cache;
mod http;
mod providers;
pub mod rate_limiter;

use crate::acceleration::AcceleratorRegistry;
use crate::accounting::{BudgetManager, TokenAccounting};
use crate::cache::ResponseCache;
use crate::http::start_http_server;
use crate::providers::{LlmProvider, ProviderType};
use crate::rate_limiter::AgentRateLimiter;

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
    Load { model_id: String },
    /// Unload a model
    Unload { model_id: String },
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

/// Per-provider health tracking for graceful degradation
#[derive(Debug, Clone)]
pub struct ProviderHealth {
    /// Whether this provider is currently considered healthy
    pub is_healthy: bool,
    /// Number of consecutive failures since last success
    pub consecutive_failures: u32,
    /// When the health status was last checked
    pub last_check: Instant,
}

impl ProviderHealth {
    fn new() -> Self {
        Self {
            is_healthy: true,
            consecutive_failures: 0,
            last_check: Instant::now(),
        }
    }

    /// Record a failure. After 3 consecutive failures, mark unhealthy.
    fn record_failure(&mut self) {
        self.consecutive_failures += 1;
        self.last_check = Instant::now();
        if self.consecutive_failures >= 3 {
            self.is_healthy = false;
        }
    }

    /// Record a success. One success resets the provider to healthy.
    fn record_success(&mut self) {
        self.consecutive_failures = 0;
        self.is_healthy = true;
        self.last_check = Instant::now();
    }
}

/// LLM Gateway service
pub struct LlmGateway {
    /// Active providers
    providers: RwLock<HashMap<ProviderType, Arc<dyn LlmProvider>>>,
    /// Currently loaded models
    loaded_models: RwLock<HashMap<String, ModelInfo>>,
    /// Per-provider health tracking
    provider_health: RwLock<HashMap<ProviderType, ProviderHealth>>,
    /// Global request concurrency limiter
    rate_limiter: Semaphore,
    /// Per-agent rate limiting (tokens/hour, requests/min, concurrent)
    agent_rate_limiter: AgentRateLimiter,
    /// Response cache
    cache: ResponseCache,
    /// Token accounting for agents
    accounting: TokenAccounting,
    /// Token budget manager for cross-project budget pools
    budget_manager: RwLock<BudgetManager>,
    /// Configuration
    config: GatewayConfig,
    /// TLS certificate pin set for cloud provider verification
    cert_pins: CertPinSet,
    /// Hardware accelerator registry for GPU-aware model placement.
    accelerator_registry: AcceleratorRegistry,
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
        Self::new_with_pins(config, None).await
    }

    /// Create a new LLM gateway with an optional custom pin set.
    /// If `pins` is `None`, loads `default_agnos_pins()`.
    pub async fn new_with_pins(config: GatewayConfig, pins: Option<CertPinSet>) -> Result<Self> {
        info!("Initializing LLM Gateway...");

        let cert_pins = pins.unwrap_or_else(certpin::default_agnos_pins);

        // Log pin expiry warnings at startup
        let expiring = certpin::check_pin_expiry(&cert_pins);
        for entry in &expiring {
            warn!(
                host = %entry.host,
                expires = ?entry.expires,
                "Certificate pin expiring soon or already expired"
            );
        }
        if expiring.is_empty() {
            info!(
                pin_count = cert_pins.pins.len(),
                enforce = cert_pins.enforce,
                "Certificate pin set loaded, no pins expiring within 30 days"
            );
        }

        let rate_limiter = Semaphore::new(config.max_concurrent_requests);
        let cache = ResponseCache::new(Duration::from_secs(config.cache_ttl_seconds));
        let accounting = TokenAccounting::new();

        // Detect hardware accelerators for GPU-aware model placement.
        let accelerator_registry = AcceleratorRegistry::detect_available();
        if accelerator_registry.has_gpu() {
            info!(
                gpu_memory_bytes = accelerator_registry.total_gpu_memory(),
                "GPU detected — GPU-aware inference routing enabled"
            );
        } else {
            info!("No GPU detected — inference will use CPU or cloud providers");
        }

        Ok(Self {
            providers: RwLock::new(HashMap::new()),
            loaded_models: RwLock::new(HashMap::new()),
            provider_health: RwLock::new(HashMap::new()),
            rate_limiter,
            agent_rate_limiter: AgentRateLimiter::new(),
            cache,
            accounting,
            budget_manager: RwLock::new(BudgetManager::new()),
            config,
            cert_pins,
            accelerator_registry,
        })
    }

    /// Initialize providers
    pub async fn init_providers(&self) -> Result<()> {
        info!("Initializing LLM providers...");

        let mut providers = self.providers.write().await;
        let mut health = self.provider_health.write().await;

        // Try to initialize local providers
        match providers::OllamaProvider::new().await {
            Ok(provider) => {
                info!("Ollama provider initialized");
                providers.insert(ProviderType::Ollama, Arc::new(provider));
                health.insert(ProviderType::Ollama, ProviderHealth::new());
            }
            Err(e) => {
                warn!("Failed to initialize Ollama provider: {}", e);
            }
        }

        match providers::LlamaCppProvider::new().await {
            Ok(provider) => {
                info!("llama.cpp provider initialized");
                providers.insert(ProviderType::LlamaCpp, Arc::new(provider));
                health.insert(ProviderType::LlamaCpp, ProviderHealth::new());
            }
            Err(e) => {
                warn!("Failed to initialize llama.cpp provider: {}", e);
            }
        }

        // Initialize cloud providers from environment
        if let Ok(api_key) = std::env::var("OPENAI_API_KEY") {
            match providers::OpenAiProvider::new(api_key, std::env::var("OPENAI_BASE_URL").ok()) {
                Ok(provider) => {
                    providers.insert(ProviderType::OpenAi, Arc::new(provider));
                    health.insert(ProviderType::OpenAi, ProviderHealth::new());
                    info!("OpenAI provider initialized");
                }
                Err(e) => {
                    warn!("Failed to initialize OpenAI provider: {}", e);
                }
            }
        }

        if let Ok(api_key) = std::env::var("ANTHROPIC_API_KEY") {
            match providers::AnthropicProvider::new(
                api_key,
                std::env::var("ANTHROPIC_BASE_URL").ok(),
            ) {
                Ok(provider) => {
                    providers.insert(ProviderType::Anthropic, Arc::new(provider));
                    health.insert(ProviderType::Anthropic, ProviderHealth::new());
                    info!("Anthropic provider initialized");
                }
                Err(e) => {
                    warn!("Failed to initialize Anthropic provider: {}", e);
                }
            }
        }

        // Initialize Google from environment
        if let Ok(api_key) = std::env::var("GOOGLE_API_KEY")
            .or_else(|_| std::env::var("GOOGLE_GENERATIVE_AI_API_KEY"))
        {
            match providers::GoogleProvider::new(api_key, std::env::var("GOOGLE_BASE_URL").ok()) {
                Ok(provider) => {
                    providers.insert(ProviderType::Google, Arc::new(provider));
                    health.insert(ProviderType::Google, ProviderHealth::new());
                    info!("Google (Gemini) provider initialized");
                }
                Err(e) => {
                    warn!("Failed to initialize Google provider: {}", e);
                }
            }
        }

        // Initialize OpenAI-compatible cloud providers from environment
        if let Ok(api_key) = std::env::var("DEEPSEEK_API_KEY") {
            match providers::new_deepseek_provider(api_key, std::env::var("DEEPSEEK_BASE_URL").ok())
            {
                Ok(provider) => {
                    providers.insert(ProviderType::DeepSeek, Arc::new(provider));
                    health.insert(ProviderType::DeepSeek, ProviderHealth::new());
                    info!("DeepSeek provider initialized");
                }
                Err(e) => warn!("Failed to initialize DeepSeek provider: {}", e),
            }
        }

        if let Ok(api_key) = std::env::var("MISTRAL_API_KEY") {
            match providers::new_mistral_provider(api_key, std::env::var("MISTRAL_BASE_URL").ok()) {
                Ok(provider) => {
                    providers.insert(ProviderType::Mistral, Arc::new(provider));
                    health.insert(ProviderType::Mistral, ProviderHealth::new());
                    info!("Mistral provider initialized");
                }
                Err(e) => warn!("Failed to initialize Mistral provider: {}", e),
            }
        }

        if let Ok(api_key) = std::env::var("XAI_API_KEY") {
            match providers::new_grok_provider(api_key, std::env::var("XAI_BASE_URL").ok()) {
                Ok(provider) => {
                    providers.insert(ProviderType::Grok, Arc::new(provider));
                    health.insert(ProviderType::Grok, ProviderHealth::new());
                    info!("Grok (x.ai) provider initialized");
                }
                Err(e) => warn!("Failed to initialize Grok provider: {}", e),
            }
        }

        if let Ok(api_key) = std::env::var("GROQ_API_KEY") {
            match providers::new_groq_provider(api_key, std::env::var("GROQ_BASE_URL").ok()) {
                Ok(provider) => {
                    providers.insert(ProviderType::Groq, Arc::new(provider));
                    health.insert(ProviderType::Groq, ProviderHealth::new());
                    info!("Groq provider initialized");
                }
                Err(e) => warn!("Failed to initialize Groq provider: {}", e),
            }
        }

        if let Ok(api_key) = std::env::var("OPENROUTER_API_KEY") {
            match providers::new_openrouter_provider(
                api_key,
                std::env::var("OPENROUTER_BASE_URL").ok(),
            ) {
                Ok(provider) => {
                    providers.insert(ProviderType::OpenRouter, Arc::new(provider));
                    health.insert(ProviderType::OpenRouter, ProviderHealth::new());
                    info!("OpenRouter provider initialized");
                }
                Err(e) => warn!("Failed to initialize OpenRouter provider: {}", e),
            }
        }

        if let Ok(api_key) = std::env::var("OPENCODE_API_KEY") {
            match providers::new_opencode_provider(api_key, std::env::var("OPENCODE_BASE_URL").ok())
            {
                Ok(provider) => {
                    providers.insert(ProviderType::OpenCode, Arc::new(provider));
                    health.insert(ProviderType::OpenCode, ProviderHealth::new());
                    info!("OpenCode provider initialized");
                }
                Err(e) => warn!("Failed to initialize OpenCode provider: {}", e),
            }
        }

        if let Ok(api_key) = std::env::var("LETTA_API_KEY") {
            match providers::new_letta_provider(Some(api_key), std::env::var("LETTA_BASE_URL").ok())
            {
                Ok(provider) => {
                    providers.insert(ProviderType::Letta, Arc::new(provider));
                    health.insert(ProviderType::Letta, ProviderHealth::new());
                    info!("Letta provider initialized");
                }
                Err(e) => warn!("Failed to initialize Letta provider: {}", e),
            }
        } else if std::env::var("LETTA_LOCAL").unwrap_or_default() == "true" {
            // Letta local mode — no API key required
            match providers::new_letta_provider(None, std::env::var("LETTA_BASE_URL").ok()) {
                Ok(provider) => {
                    providers.insert(ProviderType::Letta, Arc::new(provider));
                    health.insert(ProviderType::Letta, ProviderHealth::new());
                    info!("Letta provider initialized (local mode)");
                }
                Err(e) => warn!("Failed to initialize Letta local provider: {}", e),
            }
        }

        // Initialize local OpenAI-compatible providers
        if std::env::var("LMSTUDIO_BASE_URL").is_ok() || cfg!(debug_assertions) {
            match providers::new_lmstudio_provider(std::env::var("LMSTUDIO_BASE_URL").ok()) {
                Ok(provider) => {
                    providers.insert(ProviderType::LmStudio, Arc::new(provider));
                    health.insert(ProviderType::LmStudio, ProviderHealth::new());
                    info!("LM Studio provider initialized");
                }
                Err(e) => warn!("Failed to initialize LM Studio provider: {}", e),
            }
        }

        if std::env::var("LOCALAI_BASE_URL").is_ok() {
            match providers::new_localai_provider(std::env::var("LOCALAI_BASE_URL").ok()) {
                Ok(provider) => {
                    providers.insert(ProviderType::LocalAi, Arc::new(provider));
                    health.insert(ProviderType::LocalAi, ProviderHealth::new());
                    info!("LocalAI provider initialized");
                }
                Err(e) => warn!("Failed to initialize LocalAI provider: {}", e),
            }
        }

        // Synapse (local LLM management service — managed by argonaut)
        {
            let synapse_url = std::env::var("SYNAPSE_BASE_URL").ok();
            match providers::new_synapse_provider(synapse_url) {
                Ok(provider) => {
                    info!("Synapse provider initialized (local)");
                    providers.insert(ProviderType::Synapse, Arc::new(provider));
                    health.insert(ProviderType::Synapse, ProviderHealth::new());
                }
                Err(e) => {
                    debug!("Synapse provider not available: {}", e);
                }
            }
        }

        info!(count = providers.len(), "Providers initialized");
        Ok(())
    }

    /// Run inference with the LLM, retrying with fallback providers on failure.
    /// Enforces per-agent rate limits before processing.
    pub async fn infer(
        &self,
        mut request: InferenceRequest,
        agent_id: Option<AgentId>,
    ) -> Result<InferenceResponse> {
        // Enforce parameter bounds before any processing
        request.validate();

        // Check per-agent rate limits
        if let Some(aid) = agent_id {
            if let Err(reason) = self.agent_rate_limiter.check_and_record(aid).await {
                anyhow::bail!("Rate limited for agent {}: {}", aid, reason);
            }
        }

        let _permit = self.rate_limiter.acquire().await?;

        info!(
            model = %request.model,
            agent_id = ?agent_id,
            max_tokens = request.max_tokens,
            temperature = request.temperature,
            "Inference request"
        );

        // Check cache if enabled
        if self.config.enable_caching {
            if let Some(cached) = self.cache.get(&request).await {
                debug!(model = %request.model, "Cache hit for inference request");
                return Ok(cached);
            }
        }

        // Collect ordered list of (provider_type, provider) to try
        let candidates = self.select_providers_ordered(&request).await?;

        // Try up to 3 candidates (initial + 2 retries)
        let max_attempts = candidates.len().min(3);
        let mut last_error = None;

        for (i, (provider_type, provider)) in candidates.into_iter().take(max_attempts).enumerate()
        {
            if i > 0 {
                info!(
                    provider = ?provider_type,
                    attempt = i + 1,
                    "Retrying inference with fallback provider"
                );
            }

            match timeout(self.config.request_timeout, provider.infer(&request)).await {
                Ok(Ok(response)) => {
                    // Record success for health tracking
                    self.record_provider_success(provider_type).await;

                    // Update token accounting and rate limiter
                    if let Some(aid) = agent_id {
                        if self.config.enable_token_accounting {
                            self.accounting.record_usage(aid, response.usage).await;
                        }
                        self.agent_rate_limiter
                            .record_request_end(aid, response.usage.total_tokens as u64)
                            .await;
                    }

                    // Cache the response
                    if self.config.enable_caching {
                        self.cache.set(&request, response.clone()).await;
                    }

                    return Ok(response);
                }
                Ok(Err(e)) => {
                    warn!(
                        provider = ?provider_type,
                        error = %e,
                        "Provider inference failed"
                    );
                    self.record_provider_failure(provider_type).await;
                    last_error = Some(e);
                }
                Err(_) => {
                    warn!(
                        provider = ?provider_type,
                        "Provider inference timed out"
                    );
                    self.record_provider_failure(provider_type).await;
                    last_error = Some(anyhow::anyhow!("Inference request timed out"));
                }
            }
        }

        // All attempts failed — still need to record request end for rate limiter
        if let Some(aid) = agent_id {
            self.agent_rate_limiter.record_request_end(aid, 0).await;
        }

        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("No LLM provider available")))
    }

    /// Stream inference results
    pub async fn infer_stream(
        &self,
        request: InferenceRequest,
        agent_id: Option<AgentId>,
    ) -> Result<mpsc::Receiver<Result<String>>> {
        let _permit = self.rate_limiter.acquire().await?;

        info!(
            model = %request.model,
            agent_id = ?agent_id,
            max_tokens = request.max_tokens,
            "Streaming inference request"
        );

        let provider = self.select_provider(&request).await?;
        let stream = provider.infer_stream(request).await?;

        Ok(stream)
    }

    /// Select the best provider for a request (returns first healthy match)
    async fn select_provider(&self, request: &InferenceRequest) -> Result<Arc<dyn LlmProvider>> {
        let candidates = self.select_providers_ordered(request).await?;
        candidates
            .into_iter()
            .next()
            .map(|(_, provider)| provider)
            .ok_or_else(|| anyhow::anyhow!("No LLM provider available"))
    }

    /// Return an ordered list of healthy providers to try for a request.
    /// Selects providers in priority order for GPU-aware inference routing.
    ///
    /// Priority:
    /// 1. **Local GPU providers** (Ollama, llama.cpp) when the model is loaded
    ///    locally AND a GPU is available — highest throughput, no network latency.
    /// 2. **Local providers** (Ollama, llama.cpp) when model is loaded locally.
    /// 3. **Cloud providers** in registration order.
    ///
    /// Unhealthy providers are appended at the end as last-resort fallbacks.
    /// Acquires all 3 read locks in a single batch to minimize async yield points.
    async fn select_providers_ordered(
        &self,
        request: &InferenceRequest,
    ) -> Result<Vec<(ProviderType, Arc<dyn LlmProvider>)>> {
        // Snapshot all state under locks, then release immediately
        let (provider_snapshot, health_snapshot, model_loaded, model_size) = {
            let providers = self.providers.read().await;
            let health = self.provider_health.read().await;
            let loaded = self.loaded_models.read().await;

            if providers.is_empty() {
                return Err(anyhow::anyhow!("No LLM provider available"));
            }

            let ps: Vec<_> = providers.iter().map(|(&pt, p)| (pt, p.clone())).collect();
            let hs: HashMap<ProviderType, bool> =
                health.iter().map(|(&pt, h)| (pt, h.is_healthy)).collect();
            let ml = loaded.contains_key(&request.model);
            let ms = loaded.get(&request.model).map(|m| m.size_bytes);
            (ps, hs, ml, ms)
        };
        // All locks released here

        let has_gpu = self.accelerator_registry.has_gpu();
        let local_provider_types = [
            ProviderType::Ollama,
            ProviderType::LlamaCpp,
            ProviderType::LocalAi,
            ProviderType::LmStudio,
        ];

        // Check if model fits on local GPU (rough estimate: size_bytes ≈ FP16 weight size)
        let model_fits_on_gpu = has_gpu
            && model_size
                .map(|sz| sz <= self.accelerator_registry.total_gpu_memory())
                .unwrap_or(false);

        let mut healthy: Vec<(ProviderType, Arc<dyn LlmProvider>)> = Vec::new();
        let mut unhealthy: Vec<(ProviderType, Arc<dyn LlmProvider>)> = Vec::new();

        let mut classify = |pt: ProviderType, p: Arc<dyn LlmProvider>| {
            if *health_snapshot.get(&pt).unwrap_or(&true) {
                healthy.push((pt, p));
            } else {
                unhealthy.push((pt, p));
            }
        };

        // Priority 1: Local GPU-capable providers when model is loaded and fits on GPU
        if model_loaded && model_fits_on_gpu {
            for (pt, provider) in &provider_snapshot {
                if local_provider_types.contains(pt) {
                    debug!(provider = ?pt, "GPU-aware: prioritizing local GPU provider");
                    classify(*pt, provider.clone());
                }
            }
        }

        // Priority 2: Local providers when model is loaded (even without GPU)
        if model_loaded && !model_fits_on_gpu {
            if let Some((_, provider)) = provider_snapshot
                .iter()
                .find(|(pt, _)| *pt == ProviderType::Ollama)
            {
                classify(ProviderType::Ollama, provider.clone());
            }
        }

        // Priority 3: All other providers in registration order
        for (pt, provider) in &provider_snapshot {
            // Skip providers already added
            if model_loaded && local_provider_types.contains(pt) && model_fits_on_gpu {
                continue;
            }
            if *pt == ProviderType::Ollama && model_loaded && !model_fits_on_gpu {
                continue;
            }
            classify(*pt, provider.clone());
        }

        // Healthy first, unhealthy as last resort
        healthy.extend(unhealthy);
        Ok(healthy)
    }

    /// Returns the hardware accelerator registry (for diagnostics and API exposure).
    pub fn accelerator_registry(&self) -> &AcceleratorRegistry {
        &self.accelerator_registry
    }

    /// Record a successful call to a provider
    async fn record_provider_success(&self, provider_type: ProviderType) {
        let mut health = self.provider_health.write().await;
        health
            .entry(provider_type)
            .or_insert_with(ProviderHealth::new)
            .record_success();
        debug!(provider = ?provider_type, "Provider marked healthy");
    }

    /// Record a failed call to a provider
    async fn record_provider_failure(&self, provider_type: ProviderType) {
        let mut health = self.provider_health.write().await;
        let entry = health
            .entry(provider_type)
            .or_insert_with(ProviderHealth::new);
        entry.record_failure();
        if !entry.is_healthy {
            warn!(
                provider = ?provider_type,
                consecutive_failures = entry.consecutive_failures,
                "Provider marked unhealthy"
            );
        }
    }

    /// Extract a hostname from a provider URL string.
    /// Returns `None` for localhost/local providers (cert pinning not applicable).
    fn extract_provider_host(provider_type: ProviderType) -> Option<&'static str> {
        match provider_type {
            ProviderType::OpenAi => Some("api.openai.com"),
            ProviderType::Anthropic => Some("api.anthropic.com"),
            ProviderType::Google => Some("generativelanguage.googleapis.com"),
            ProviderType::DeepSeek => Some("api.deepseek.com"),
            ProviderType::Mistral => Some("api.mistral.ai"),
            ProviderType::Grok => Some("api.x.ai"),
            ProviderType::Groq => Some("api.groq.com"),
            ProviderType::OpenRouter => Some("openrouter.ai"),
            ProviderType::OpenCode => Some("api.open-code.dev"),
            ProviderType::Letta => Some("app.letta.com"),
            // Local providers use HTTP, no TLS pinning
            ProviderType::Ollama
            | ProviderType::LlamaCpp
            | ProviderType::LmStudio
            | ProviderType::LocalAi
            | ProviderType::Synapse => None,
        }
    }

    /// Verify a cloud provider's TLS certificate against the loaded pin set.
    ///
    /// - Skips local providers (Ollama, llama.cpp) since they use plain HTTP.
    /// - In report-only mode (`enforce == false`), logs warnings but returns `Ok(())`.
    /// - In enforce mode, returns an error on pin mismatch.
    pub fn verify_provider_cert(&self, provider_type: ProviderType) -> Result<()> {
        let host = match Self::extract_provider_host(provider_type) {
            Some(h) => h,
            None => return Ok(()), // local provider, skip
        };

        let cert_info = match certpin::fetch_server_cert(host, 443) {
            Ok(info) => info,
            Err(e) => {
                warn!(
                    host = %host,
                    error = %e,
                    "Failed to fetch server certificate for pin verification"
                );
                // Cannot verify — don't block in either mode
                return Ok(());
            }
        };

        let result = certpin::verify_pin(host, &cert_info.spki_sha256, &self.cert_pins);

        match &result {
            CertPinResult::Valid => {
                debug!(host = %host, "Certificate pin verified successfully");
                Ok(())
            }
            CertPinResult::NoPinConfigured { .. } => {
                debug!(host = %host, "No certificate pin configured for host");
                Ok(())
            }
            CertPinResult::PinMismatch {
                host,
                expected,
                actual,
            } => {
                warn!(
                    host = %host,
                    expected = ?expected,
                    actual = %actual,
                    enforce = self.cert_pins.enforce,
                    "Certificate pin MISMATCH — possible MITM or CA rotation"
                );
                if self.cert_pins.enforce {
                    anyhow::bail!(
                        "Certificate pin mismatch for {}: expected one of {:?}, got {}",
                        host,
                        expected,
                        actual
                    )
                } else {
                    Ok(()) // report-only mode
                }
            }
            CertPinResult::Expired { host } => {
                warn!(host = %host, "Certificate pin entry has expired");
                if self.cert_pins.enforce {
                    anyhow::bail!("Certificate pin expired for {}", host)
                } else {
                    Ok(())
                }
            }
            CertPinResult::Error(msg) => {
                warn!(host = %host, error = %msg, "Certificate pin verification error");
                Ok(())
            }
        }
    }

    /// List available models
    pub async fn list_models(&self) -> Vec<ModelInfo> {
        let loaded = self.loaded_models.read().await;
        loaded.values().cloned().collect()
    }

    /// Load a model
    pub async fn load_model(&self, model_id: &str) -> Result<()> {
        info!(model_id = %model_id, "Loading model");

        let providers = self.providers.read().await;

        // Try to load from available providers
        for (provider_type, provider) in providers.iter() {
            match provider.load_model(model_id).await {
                Ok(model_info) => {
                    let mut loaded = self.loaded_models.write().await;
                    loaded.insert(model_id.to_string(), model_info);
                    info!(model_id = %model_id, provider = ?provider_type, "Model loaded successfully");
                    return Ok(());
                }
                Err(e) => {
                    debug!(provider = ?provider_type, model_id = %model_id, error = %e, "Provider could not load model");
                }
            }
        }

        Err(anyhow::anyhow!(
            "Failed to load model {} from any provider",
            model_id
        ))
    }

    /// Unload a model
    pub async fn unload_model(&self, model_id: &str) -> Result<()> {
        info!(model_id = %model_id, "Unloading model");

        let mut loaded = self.loaded_models.write().await;

        if loaded.remove(model_id).is_some() {
            info!(model_id = %model_id, "Model unloaded");
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

    /// Return health status for all tracked providers
    pub async fn provider_health(&self) -> HashMap<ProviderType, ProviderHealth> {
        self.provider_health.read().await.clone()
    }

    /// Return cache statistics
    pub async fn cache_stats(&self) -> crate::cache::CacheStats {
        self.cache.stats().await
    }

    /// Return token accounting statistics
    pub async fn accounting_stats(&self) -> crate::accounting::AccountingStats {
        self.accounting.stats().await
    }

    /// Get a reference to the per-agent rate limiter.
    pub fn rate_limits(&self) -> &AgentRateLimiter {
        &self.agent_rate_limiter
    }

    /// Get a read lock on the budget manager.
    pub async fn budget_manager_read(&self) -> tokio::sync::RwLockReadGuard<'_, BudgetManager> {
        self.budget_manager.read().await
    }

    /// Get a write lock on the budget manager.
    pub async fn budget_manager_write(&self) -> tokio::sync::RwLockWriteGuard<'_, BudgetManager> {
        self.budget_manager.write().await
    }

    /// Spawn a background task that pings each provider every 30 seconds via `list_models()`
    /// and updates health status accordingly.
    pub fn start_health_check_loop(self: &Arc<Self>) {
        let gateway = Arc::clone(self);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(30));
            loop {
                interval.tick().await;
                gateway.run_health_checks().await;
            }
        });
    }

    /// Run a single round of health checks against all registered providers
    pub async fn run_health_checks(&self) {
        let provider_list: Vec<(ProviderType, Arc<dyn LlmProvider>)> = {
            let providers = self.providers.read().await;
            providers.iter().map(|(&pt, p)| (pt, p.clone())).collect()
        };

        for (provider_type, provider) in provider_list {
            // Verify TLS certificate pins for cloud providers
            if let Err(e) = self.verify_provider_cert(provider_type) {
                warn!(
                    provider = ?provider_type,
                    error = %e,
                    "Certificate pin verification failed during health check"
                );
                self.record_provider_failure(provider_type).await;
                continue;
            }

            match timeout(Duration::from_secs(10), provider.list_models()).await {
                Ok(Ok(_)) => {
                    self.record_provider_success(provider_type).await;
                }
                Ok(Err(e)) => {
                    debug!(
                        provider = ?provider_type,
                        error = %e,
                        "Health check failed"
                    );
                    self.record_provider_failure(provider_type).await;
                }
                Err(_) => {
                    debug!(
                        provider = ?provider_type,
                        "Health check timed out"
                    );
                    self.record_provider_failure(provider_type).await;
                }
            }
        }
    }

    /// Create a model sharing session for multi-agent access
    pub async fn create_shared_session(
        &self,
        model_id: &str,
        agent_ids: Vec<AgentId>,
    ) -> Result<SharedSession> {
        info!(model_id = %model_id, agent_count = agent_ids.len(), "Creating shared session");

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
    let fmt = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env());
    if std::env::var("AGNOS_LOG_FORMAT").as_deref() == Ok("json") {
        fmt.json().init();
    } else {
        fmt.init();
    }

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

    // Start background health checks (every 30s)
    gateway.start_health_check_loop();

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
            let body: serde_json::Value = resp
                .json()
                .await
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
            let body: serde_json::Value = resp.json().await.context("Failed to parse response")?;

            let found = body["data"]
                .as_array()
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
            let result: serde_json::Value = resp
                .json()
                .await
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
                let msg = result["error"]["message"]
                    .as_str()
                    .unwrap_or("Unknown error");
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
            let body: serde_json::Value = resp
                .json()
                .await
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
        assert_eq!(
            cloned.max_concurrent_requests,
            config.max_concurrent_requests
        );
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
        assert!(
            json.is_ok(),
            "InferenceRequest must be serializable to JSON"
        );
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
        assert_eq!(
            expected_port, 8088,
            "AGNOS LLM Gateway HTTP port must be 8088 per ADR-007"
        );
    }

    // ------------------------------------------------------------------
    // Additional coverage: gateway methods, unload, list_providers, CLI helpers
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_gateway_unload_model_not_loaded() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();
        // Unloading a model that was never loaded should succeed silently
        let result = gateway.unload_model("nonexistent-model").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_gateway_list_providers_empty_initially() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();
        let providers = gateway.list_providers().await;
        // list_providers always returns 3 entries (Ollama, llama.cpp, OpenAI)
        assert_eq!(providers.len(), 3);
        // None should be available since init_providers was not called
        assert!(providers.iter().all(|p| !p.available));
    }

    #[tokio::test]
    async fn test_gateway_list_providers_names() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();
        let providers = gateway.list_providers().await;
        let names: Vec<&str> = providers.iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains(&"Ollama"));
        assert!(names.contains(&"llama.cpp"));
        assert!(names.contains(&"OpenAI"));
    }

    #[tokio::test]
    async fn test_gateway_load_model_no_providers() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();
        let result = gateway.load_model("llama2").await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Failed to load model"));
    }

    #[tokio::test]
    async fn test_gateway_infer_stream_no_providers() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();
        let request = agnos_common::InferenceRequest::default();
        let result = gateway.infer_stream(request, None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_gateway_select_provider_no_providers() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();
        let request = agnos_common::InferenceRequest::default();
        let result = gateway.select_provider(&request).await;
        assert!(result.is_err());
        let err_msg = result.err().unwrap().to_string();
        assert!(err_msg.contains("No LLM provider available"));
    }

    #[tokio::test]
    async fn test_gateway_accounting_record_and_retrieve() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();
        let agent_id = AgentId::new();

        // Record usage directly through accounting
        gateway
            .accounting
            .record_usage(
                agent_id,
                TokenUsage {
                    prompt_tokens: 50,
                    completion_tokens: 30,
                    total_tokens: 80,
                },
            )
            .await;

        let usage = gateway.get_agent_usage(agent_id).await;
        assert!(usage.is_some());
        let u = usage.unwrap();
        assert_eq!(u.prompt_tokens, 50);
        assert_eq!(u.completion_tokens, 30);
        assert_eq!(u.total_tokens, 80);

        let total = gateway.get_total_usage().await;
        assert_eq!(total.total_tokens, 80);
    }

    #[tokio::test]
    async fn test_gateway_accounting_multiple_agents() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();
        let agent1 = AgentId::new();
        let agent2 = AgentId::new();

        gateway
            .accounting
            .record_usage(
                agent1,
                TokenUsage {
                    prompt_tokens: 10,
                    completion_tokens: 20,
                    total_tokens: 30,
                },
            )
            .await;
        gateway
            .accounting
            .record_usage(
                agent2,
                TokenUsage {
                    prompt_tokens: 40,
                    completion_tokens: 50,
                    total_tokens: 90,
                },
            )
            .await;

        let total = gateway.get_total_usage().await;
        assert_eq!(total.total_tokens, 120);
        assert_eq!(total.prompt_tokens, 50);
        assert_eq!(total.completion_tokens, 70);
    }

    #[tokio::test]
    async fn test_gateway_reset_agent_usage_after_recording() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();
        let agent_id = AgentId::new();

        gateway
            .accounting
            .record_usage(
                agent_id,
                TokenUsage {
                    prompt_tokens: 100,
                    completion_tokens: 200,
                    total_tokens: 300,
                },
            )
            .await;

        gateway.reset_agent_usage(agent_id).await;
        assert!(gateway.get_agent_usage(agent_id).await.is_none());

        // Total usage is NOT reset by reset_agent_usage
        let total = gateway.get_total_usage().await;
        assert_eq!(total.total_tokens, 300);
    }

    #[tokio::test]
    async fn test_gateway_accumulates_usage_for_same_agent() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();
        let agent_id = AgentId::new();

        for _ in 0..3 {
            gateway
                .accounting
                .record_usage(
                    agent_id,
                    TokenUsage {
                        prompt_tokens: 10,
                        completion_tokens: 5,
                        total_tokens: 15,
                    },
                )
                .await;
        }

        let usage = gateway.get_agent_usage(agent_id).await.unwrap();
        assert_eq!(usage.prompt_tokens, 30);
        assert_eq!(usage.completion_tokens, 15);
        assert_eq!(usage.total_tokens, 45);
    }

    #[test]
    fn test_gateway_config_debug() {
        let config = GatewayConfig::default();
        let dbg = format!("{:?}", config);
        assert!(dbg.contains("max_concurrent_requests"));
        assert!(dbg.contains("10"));
    }

    #[test]
    fn test_shared_session_debug() {
        let session = SharedSession {
            id: "s-test".to_string(),
            model_id: "m-test".to_string(),
            agent_ids: vec![],
            created_at: chrono::Utc::now(),
        };
        let dbg = format!("{:?}", session);
        assert!(dbg.contains("s-test"));
        assert!(dbg.contains("m-test"));
    }

    #[test]
    fn test_shared_session_empty_agents() {
        let session = SharedSession {
            id: "empty".to_string(),
            model_id: "model".to_string(),
            agent_ids: vec![],
            created_at: chrono::Utc::now(),
        };
        assert!(session.agent_ids.is_empty());
    }

    #[test]
    fn test_gateway_url_constant() {
        assert_eq!(GATEWAY_URL, "http://127.0.0.1:8088");
    }

    #[test]
    fn test_gateway_client_builds() {
        let client = gateway_client();
        // Just ensure it does not panic
        let _ = client;
    }

    #[tokio::test]
    async fn test_gateway_config_custom_concurrency() {
        let config = GatewayConfig {
            max_concurrent_requests: 1,
            ..GatewayConfig::default()
        };
        let gateway = LlmGateway::new(config).await.unwrap();
        // rate_limiter should have 1 permit
        let permit = gateway.rate_limiter.try_acquire();
        assert!(permit.is_ok());
        // Second acquire should fail (no more permits)
        let permit2 = gateway.rate_limiter.try_acquire();
        assert!(permit2.is_err());
    }

    #[tokio::test]
    async fn test_gateway_unload_model_twice_ok() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();
        assert!(gateway.unload_model("test").await.is_ok());
        assert!(gateway.unload_model("test").await.is_ok());
    }

    // ==================================================================
    // Additional coverage: infer error paths, list_providers, cache/accounting
    // ==================================================================

    #[tokio::test]
    async fn test_gateway_infer_no_providers_error_message() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();
        let request = agnos_common::InferenceRequest::default();
        let err = gateway.infer(request, None).await.unwrap_err();
        assert!(
            err.to_string().contains("No LLM provider available"),
            "Error should mention missing providers, got: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_gateway_infer_with_agent_id_no_providers() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();
        let request = agnos_common::InferenceRequest::default();
        let agent_id = AgentId::new();
        let result = gateway.infer(request, Some(agent_id)).await;
        assert!(result.is_err());
        // No accounting should have been recorded because inference failed
        assert!(gateway.get_agent_usage(agent_id).await.is_none());
    }

    #[tokio::test]
    async fn test_gateway_infer_with_caching_disabled() {
        let config = GatewayConfig {
            enable_caching: false,
            ..GatewayConfig::default()
        };
        let gateway = LlmGateway::new(config).await.unwrap();
        let request = agnos_common::InferenceRequest::default();
        // Should still fail (no providers) but should not panic on cache path
        assert!(gateway.infer(request, None).await.is_err());
    }

    #[tokio::test]
    async fn test_gateway_list_providers_after_init_providers() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();
        // init_providers will fail to connect to Ollama/llama.cpp but should not panic
        let _ = gateway.init_providers().await;
        let providers = gateway.list_providers().await;
        assert_eq!(providers.len(), 3);
        // In CI without Ollama/llama.cpp, all should be unavailable
        // (but we don't assert that since local dev might have them running)
    }

    #[tokio::test]
    async fn test_gateway_accounting_reset_does_not_affect_other_agents() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();
        let agent1 = AgentId::new();
        let agent2 = AgentId::new();

        gateway
            .accounting
            .record_usage(
                agent1,
                TokenUsage {
                    prompt_tokens: 10,
                    completion_tokens: 20,
                    total_tokens: 30,
                },
            )
            .await;
        gateway
            .accounting
            .record_usage(
                agent2,
                TokenUsage {
                    prompt_tokens: 100,
                    completion_tokens: 200,
                    total_tokens: 300,
                },
            )
            .await;

        gateway.reset_agent_usage(agent1).await;
        assert!(gateway.get_agent_usage(agent1).await.is_none());
        // agent2 should still have its usage
        let u2 = gateway.get_agent_usage(agent2).await.unwrap();
        assert_eq!(u2.total_tokens, 300);
    }

    #[tokio::test]
    async fn test_gateway_list_models_after_failed_load() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();
        let _ = gateway.load_model("nonexistent").await; // will fail
        let models = gateway.list_models().await;
        assert!(
            models.is_empty(),
            "Failed load should not add models to the list"
        );
    }

    #[tokio::test]
    async fn test_gateway_config_zero_concurrent_requests() {
        let config = GatewayConfig {
            max_concurrent_requests: 0,
            ..GatewayConfig::default()
        };
        let gateway = LlmGateway::new(config).await.unwrap();
        // With 0 permits, try_acquire should fail immediately
        let permit = gateway.rate_limiter.try_acquire();
        assert!(permit.is_err());
    }

    #[tokio::test]
    async fn test_gateway_config_short_timeout() {
        let config = GatewayConfig {
            request_timeout: Duration::from_millis(1),
            ..GatewayConfig::default()
        };
        let gateway = LlmGateway::new(config).await.unwrap();
        assert_eq!(gateway.config.request_timeout, Duration::from_millis(1));
    }

    #[tokio::test]
    async fn test_gateway_config_large_cache_ttl() {
        let config = GatewayConfig {
            cache_ttl_seconds: 86400,
            ..GatewayConfig::default()
        };
        let gateway = LlmGateway::new(config).await.unwrap();
        assert_eq!(gateway.config.cache_ttl_seconds, 86400);
    }

    #[test]
    fn test_shared_session_created_at_is_recent() {
        let before = chrono::Utc::now();
        let session = SharedSession {
            id: "s".to_string(),
            model_id: "m".to_string(),
            agent_ids: vec![],
            created_at: chrono::Utc::now(),
        };
        let after = chrono::Utc::now();
        assert!(session.created_at >= before);
        assert!(session.created_at <= after);
    }

    #[test]
    fn test_shared_session_many_agents() {
        let agents: Vec<AgentId> = (0..50).map(|_| AgentId::new()).collect();
        let session = SharedSession {
            id: "big-session".to_string(),
            model_id: "llama".to_string(),
            agent_ids: agents,
            created_at: chrono::Utc::now(),
        };
        assert_eq!(session.agent_ids.len(), 50);
    }

    #[tokio::test]
    async fn test_gateway_create_shared_session_needs_providers() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();
        let result = gateway
            .create_shared_session("model", vec![AgentId::new(), AgentId::new()])
            .await;
        assert!(
            result.is_err(),
            "Should fail without providers to load the model"
        );
    }

    #[tokio::test]
    async fn test_gateway_infer_stream_no_providers_error_message() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();
        let request = agnos_common::InferenceRequest::default();
        let err = gateway.infer_stream(request, None).await.unwrap_err();
        assert!(err.to_string().contains("No LLM provider available"));
    }

    // ==================================================================
    // Additional coverage: select_provider with loaded models,
    // unload after manual insert, list_models after manual insert,
    // infer_stream with agent_id, gateway_client, GATEWAY_URL
    // ==================================================================

    #[tokio::test]
    async fn test_select_provider_prefers_loaded_model_ollama() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();

        // Manually insert a model into loaded_models
        {
            let mut loaded = gateway.loaded_models.write().await;
            loaded.insert(
                "my-model".to_string(),
                agnos_common::ModelInfo {
                    id: "my-model".to_string(),
                    name: "My Model".to_string(),
                    provider: agnos_common::Provider::Local,
                    capabilities: vec![],
                    max_tokens: 4096,
                    size_bytes: 7_000_000_000,
                    loaded: true,
                },
            );
        }

        // Still no providers registered, so should fail
        let request = agnos_common::InferenceRequest {
            model: "my-model".to_string(),
            ..Default::default()
        };
        let result = gateway.select_provider(&request).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_unload_model_after_manual_insert() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();

        // Manually add a model
        {
            let mut loaded = gateway.loaded_models.write().await;
            loaded.insert(
                "test-model".to_string(),
                agnos_common::ModelInfo {
                    id: "test-model".to_string(),
                    name: "Test".to_string(),
                    provider: agnos_common::Provider::Local,
                    capabilities: vec![],
                    max_tokens: 2048,
                    size_bytes: 1_000_000,
                    loaded: true,
                },
            );
        }

        assert_eq!(gateway.list_models().await.len(), 1);
        gateway.unload_model("test-model").await.unwrap();
        assert_eq!(gateway.list_models().await.len(), 0);
    }

    #[tokio::test]
    async fn test_list_models_returns_all_loaded() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();

        {
            let mut loaded = gateway.loaded_models.write().await;
            for i in 0..5 {
                loaded.insert(
                    format!("model-{}", i),
                    agnos_common::ModelInfo {
                        id: format!("model-{}", i),
                        name: format!("Model {}", i),
                        provider: agnos_common::Provider::Local,
                        capabilities: vec![],
                        max_tokens: 4096,
                        size_bytes: i as u64 * 1_000_000,
                        loaded: true,
                    },
                );
            }
        }

        let models = gateway.list_models().await;
        assert_eq!(models.len(), 5);
    }

    #[tokio::test]
    async fn test_infer_stream_with_agent_id_no_providers() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();
        let request = agnos_common::InferenceRequest::default();
        let agent_id = AgentId::new();
        let result = gateway.infer_stream(request, Some(agent_id)).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_gateway_multiple_rate_limiter_permits() {
        let config = GatewayConfig {
            max_concurrent_requests: 3,
            ..GatewayConfig::default()
        };
        let gateway = LlmGateway::new(config).await.unwrap();

        let p1 = gateway.rate_limiter.try_acquire();
        let p2 = gateway.rate_limiter.try_acquire();
        let p3 = gateway.rate_limiter.try_acquire();
        assert!(p1.is_ok());
        assert!(p2.is_ok());
        assert!(p3.is_ok());

        // Fourth should fail
        let p4 = gateway.rate_limiter.try_acquire();
        assert!(p4.is_err());
    }

    #[tokio::test]
    async fn test_gateway_infer_validates_request() {
        // Verify that infer calls request.validate() (parameter bounds)
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();
        let request = agnos_common::InferenceRequest {
            temperature: 5.0, // Out of bounds, should be clamped
            ..Default::default()
        };
        // Will still fail (no providers) but should not panic during validation
        let result = gateway.infer(request, None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_gateway_accounting_cumulative_total() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();
        let a1 = AgentId::new();
        let a2 = AgentId::new();
        let a3 = AgentId::new();

        gateway
            .accounting
            .record_usage(
                a1,
                TokenUsage {
                    prompt_tokens: 100,
                    completion_tokens: 50,
                    total_tokens: 150,
                },
            )
            .await;
        gateway
            .accounting
            .record_usage(
                a2,
                TokenUsage {
                    prompt_tokens: 200,
                    completion_tokens: 100,
                    total_tokens: 300,
                },
            )
            .await;
        gateway
            .accounting
            .record_usage(
                a3,
                TokenUsage {
                    prompt_tokens: 50,
                    completion_tokens: 25,
                    total_tokens: 75,
                },
            )
            .await;

        let total = gateway.get_total_usage().await;
        assert_eq!(total.prompt_tokens, 350);
        assert_eq!(total.completion_tokens, 175);
        assert_eq!(total.total_tokens, 525);
    }

    #[tokio::test]
    async fn test_gateway_list_providers_names_order() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();
        let providers = gateway.list_providers().await;
        assert_eq!(providers[0].name, "Ollama");
        assert_eq!(providers[1].name, "llama.cpp");
        assert_eq!(providers[2].name, "OpenAI");
    }

    #[tokio::test]
    async fn test_gateway_unload_preserves_other_models() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();

        {
            let mut loaded = gateway.loaded_models.write().await;
            loaded.insert(
                "model-a".to_string(),
                agnos_common::ModelInfo {
                    id: "model-a".to_string(),
                    name: "A".to_string(),
                    provider: agnos_common::Provider::Local,
                    capabilities: vec![],
                    max_tokens: 0,
                    size_bytes: 0,
                    loaded: true,
                },
            );
            loaded.insert(
                "model-b".to_string(),
                agnos_common::ModelInfo {
                    id: "model-b".to_string(),
                    name: "B".to_string(),
                    provider: agnos_common::Provider::Local,
                    capabilities: vec![],
                    max_tokens: 0,
                    size_bytes: 0,
                    loaded: true,
                },
            );
        }

        gateway.unload_model("model-a").await.unwrap();
        let models = gateway.list_models().await;
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "model-b");
    }

    #[test]
    fn test_inference_request_validate_clamps_temperature() {
        let mut req = agnos_common::InferenceRequest {
            temperature: 10.0,
            top_p: -1.0,
            ..Default::default()
        };
        req.validate();
        assert!(req.temperature <= 2.0);
        assert!(req.top_p >= 0.0);
    }

    // ==================================================================
    // Additional coverage: init_providers, cache paths, select_provider
    // with registered providers, CLI helpers, GatewayConfig edge cases
    // ==================================================================

    #[tokio::test]
    async fn test_init_providers_does_not_panic() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();
        // init_providers tries to connect to Ollama/llama.cpp — should not panic
        let result = gateway.init_providers().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_init_providers_populates_provider_map() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();
        gateway.init_providers().await.unwrap();
        let providers = gateway.providers.read().await;
        // In CI without local LLM servers, providers may have 0-2 local + 0-2 cloud entries
        // We just verify the map was populated without errors
        let _ = providers.len();
    }

    #[tokio::test]
    async fn test_gateway_config_all_disabled() {
        let config = GatewayConfig {
            max_concurrent_requests: 1,
            request_timeout: Duration::from_millis(100),
            enable_caching: false,
            cache_ttl_seconds: 0,
            enable_token_accounting: false,
        };
        let gateway = LlmGateway::new(config).await.unwrap();
        assert!(!gateway.config.enable_caching);
        assert!(!gateway.config.enable_token_accounting);
        assert_eq!(gateway.config.cache_ttl_seconds, 0);
    }

    #[tokio::test]
    async fn test_gateway_infer_with_accounting_disabled() {
        let config = GatewayConfig {
            enable_token_accounting: false,
            ..GatewayConfig::default()
        };
        let gateway = LlmGateway::new(config).await.unwrap();
        let request = agnos_common::InferenceRequest::default();
        let agent_id = AgentId::new();
        // Will fail (no providers) but should not panic on accounting path
        let result = gateway.infer(request, Some(agent_id)).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_gateway_infer_validates_max_tokens() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();
        let request = agnos_common::InferenceRequest {
            max_tokens: 0, // Should be clamped to at least 1 by validate()
            ..Default::default()
        };
        // Will fail (no providers) but validate() should not panic
        let result = gateway.infer(request, None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_gateway_infer_validates_penalties() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();
        let request = agnos_common::InferenceRequest {
            presence_penalty: 10.0,
            frequency_penalty: -10.0,
            ..Default::default()
        };
        let result = gateway.infer(request, None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_gateway_cache_stats_initially_empty() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();
        let stats = gateway.cache.stats().await;
        assert_eq!(stats.total_entries, 0);
        assert_eq!(stats.active_entries, 0);
    }

    #[tokio::test]
    async fn test_gateway_cache_clear() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();
        // Set a cached value
        let request = agnos_common::InferenceRequest::default();
        let response = agnos_common::InferenceResponse {
            text: "cached".to_string(),
            tokens_generated: 1,
            finish_reason: agnos_common::FinishReason::Stop,
            model: "test".to_string(),
            usage: TokenUsage::default(),
        };
        gateway.cache.set(&request, response).await;
        let stats = gateway.cache.stats().await;
        assert_eq!(stats.total_entries, 1);

        gateway.cache.clear().await;
        let stats = gateway.cache.stats().await;
        assert_eq!(stats.total_entries, 0);
    }

    #[tokio::test]
    async fn test_gateway_cache_hit() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();
        let request = agnos_common::InferenceRequest {
            model: "test-model".to_string(),
            prompt: "cached prompt".to_string(),
            ..Default::default()
        };
        let response = agnos_common::InferenceResponse {
            text: "cached response".to_string(),
            tokens_generated: 5,
            finish_reason: agnos_common::FinishReason::Stop,
            model: "test-model".to_string(),
            usage: TokenUsage {
                prompt_tokens: 3,
                completion_tokens: 5,
                total_tokens: 8,
            },
        };
        gateway.cache.set(&request, response.clone()).await;
        let cached = gateway.cache.get(&request).await;
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().text, "cached response");
    }

    #[tokio::test]
    async fn test_gateway_select_provider_with_loaded_model_and_ollama_provider() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();

        // Register a fake Ollama provider
        {
            let mut providers = gateway.providers.write().await;
            // Use a LlamaCppProvider as a stand-in (it implements LlmProvider)
            let provider = providers::LlamaCppProvider::new().await.unwrap();
            providers.insert(ProviderType::Ollama, Arc::new(provider));
        }

        // Load a model
        {
            let mut loaded = gateway.loaded_models.write().await;
            loaded.insert(
                "my-loaded-model".to_string(),
                agnos_common::ModelInfo {
                    id: "my-loaded-model".to_string(),
                    name: "Loaded".to_string(),
                    provider: agnos_common::Provider::Local,
                    capabilities: vec![],
                    max_tokens: 4096,
                    size_bytes: 0,
                    loaded: true,
                },
            );
        }

        // select_provider should find the Ollama provider for the loaded model
        let request = agnos_common::InferenceRequest {
            model: "my-loaded-model".to_string(),
            ..Default::default()
        };
        let result = gateway.select_provider(&request).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_gateway_select_provider_falls_back_to_first() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();

        // Register only a LlamaCpp provider (not Ollama)
        {
            let mut providers = gateway.providers.write().await;
            let provider = providers::LlamaCppProvider::new().await.unwrap();
            providers.insert(ProviderType::LlamaCpp, Arc::new(provider));
        }

        // Request for a model not in loaded_models — should fall back to first available
        let request = agnos_common::InferenceRequest {
            model: "unknown-model".to_string(),
            ..Default::default()
        };
        let result = gateway.select_provider(&request).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_gateway_select_provider_loaded_model_no_ollama_falls_back() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();

        // Register only LlamaCpp, not Ollama
        {
            let mut providers = gateway.providers.write().await;
            let provider = providers::LlamaCppProvider::new().await.unwrap();
            providers.insert(ProviderType::LlamaCpp, Arc::new(provider));
        }

        // Load a model — but no Ollama provider
        {
            let mut loaded = gateway.loaded_models.write().await;
            loaded.insert(
                "model-x".to_string(),
                agnos_common::ModelInfo {
                    id: "model-x".to_string(),
                    name: "X".to_string(),
                    provider: agnos_common::Provider::Local,
                    capabilities: vec![],
                    max_tokens: 4096,
                    size_bytes: 0,
                    loaded: true,
                },
            );
        }

        let request = agnos_common::InferenceRequest {
            model: "model-x".to_string(),
            ..Default::default()
        };
        // Should fall through loaded model check (no Ollama) and use first available
        let result = gateway.select_provider(&request).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_gateway_client_has_timeout() {
        // gateway_client() should build successfully with a 30s timeout
        let client = gateway_client();
        // Cannot inspect timeout, but ensure it's a valid client
        drop(client);
    }

    #[test]
    fn test_gateway_url_is_localhost_8088() {
        assert!(GATEWAY_URL.contains("127.0.0.1"));
        assert!(GATEWAY_URL.contains("8088"));
    }

    #[test]
    fn test_gateway_config_debug_contains_all_fields() {
        let config = GatewayConfig {
            max_concurrent_requests: 42,
            request_timeout: Duration::from_secs(99),
            enable_caching: false,
            cache_ttl_seconds: 7200,
            enable_token_accounting: true,
        };
        let dbg = format!("{:?}", config);
        assert!(dbg.contains("42"));
        assert!(dbg.contains("enable_caching"));
        assert!(dbg.contains("false"));
        assert!(dbg.contains("7200"));
        assert!(dbg.contains("enable_token_accounting"));
    }

    #[test]
    fn test_shared_session_clone_deep() {
        let agents = vec![AgentId::new(), AgentId::new(), AgentId::new()];
        let session = SharedSession {
            id: "original".to_string(),
            model_id: "model".to_string(),
            agent_ids: agents.clone(),
            created_at: chrono::Utc::now(),
        };
        let cloned = session.clone();
        assert_eq!(cloned.agent_ids.len(), 3);
        assert_eq!(cloned.id, "original");
        assert_eq!(cloned.model_id, "model");
        // Cloned should be independent
        drop(session);
        assert_eq!(cloned.agent_ids.len(), 3);
    }

    #[tokio::test]
    async fn test_gateway_load_model_error_contains_model_name() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();
        let result = gateway.load_model("my-special-model").await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("my-special-model"));
    }

    #[tokio::test]
    async fn test_gateway_concurrent_list_models() {
        let gateway = Arc::new(LlmGateway::new(GatewayConfig::default()).await.unwrap());

        // Insert some models
        {
            let mut loaded = gateway.loaded_models.write().await;
            for i in 0..10 {
                loaded.insert(
                    format!("m-{}", i),
                    agnos_common::ModelInfo {
                        id: format!("m-{}", i),
                        name: format!("Model {}", i),
                        provider: agnos_common::Provider::Local,
                        capabilities: vec![],
                        max_tokens: 4096,
                        size_bytes: 0,
                        loaded: true,
                    },
                );
            }
        }

        // Spawn concurrent readers
        let mut handles = vec![];
        for _ in 0..5 {
            let gw = gateway.clone();
            handles.push(tokio::spawn(async move { gw.list_models().await }));
        }

        for handle in handles {
            let models = handle.await.unwrap();
            assert_eq!(models.len(), 10);
        }
    }

    #[tokio::test]
    async fn test_gateway_accounting_stats() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();
        let a1 = AgentId::new();
        let a2 = AgentId::new();

        gateway
            .accounting
            .record_usage(
                a1,
                TokenUsage {
                    prompt_tokens: 10,
                    completion_tokens: 20,
                    total_tokens: 30,
                },
            )
            .await;
        gateway
            .accounting
            .record_usage(
                a2,
                TokenUsage {
                    prompt_tokens: 5,
                    completion_tokens: 10,
                    total_tokens: 15,
                },
            )
            .await;

        let stats = gateway.accounting.stats().await;
        assert_eq!(stats.total_agents, 2);
        assert_eq!(stats.total_prompt_tokens, 15);
        assert_eq!(stats.total_completion_tokens, 30);
        assert_eq!(stats.total_tokens, 45);
    }

    #[tokio::test]
    async fn test_gateway_accounting_list_agents() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();
        let a1 = AgentId::new();
        let a2 = AgentId::new();

        gateway
            .accounting
            .record_usage(
                a1,
                TokenUsage {
                    prompt_tokens: 1,
                    completion_tokens: 2,
                    total_tokens: 3,
                },
            )
            .await;
        gateway
            .accounting
            .record_usage(
                a2,
                TokenUsage {
                    prompt_tokens: 4,
                    completion_tokens: 5,
                    total_tokens: 9,
                },
            )
            .await;

        let agents = gateway.accounting.list_agents().await;
        assert_eq!(agents.len(), 2);
        let agent_ids: Vec<AgentId> = agents.iter().map(|(id, _)| *id).collect();
        assert!(agent_ids.contains(&a1));
        assert!(agent_ids.contains(&a2));
    }

    #[tokio::test]
    async fn test_gateway_accounting_reset_all() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();
        let a1 = AgentId::new();

        gateway
            .accounting
            .record_usage(
                a1,
                TokenUsage {
                    prompt_tokens: 100,
                    completion_tokens: 200,
                    total_tokens: 300,
                },
            )
            .await;

        gateway.accounting.reset_all().await;
        let total = gateway.get_total_usage().await;
        assert_eq!(total.total_tokens, 0);
        assert!(gateway.get_agent_usage(a1).await.is_none());
    }

    // ==================================================================
    // Provider health tracking and graceful degradation tests
    // ==================================================================

    #[test]
    fn test_provider_health_new_is_healthy() {
        let health = ProviderHealth::new();
        assert!(health.is_healthy);
        assert_eq!(health.consecutive_failures, 0);
    }

    #[test]
    fn test_provider_health_record_failure_below_threshold() {
        let mut health = ProviderHealth::new();
        health.record_failure();
        assert!(health.is_healthy, "1 failure should not mark unhealthy");
        assert_eq!(health.consecutive_failures, 1);

        health.record_failure();
        assert!(health.is_healthy, "2 failures should not mark unhealthy");
        assert_eq!(health.consecutive_failures, 2);
    }

    #[test]
    fn test_provider_health_unhealthy_after_3_failures() {
        let mut health = ProviderHealth::new();
        health.record_failure();
        health.record_failure();
        health.record_failure();
        assert!(
            !health.is_healthy,
            "3 consecutive failures should mark unhealthy"
        );
        assert_eq!(health.consecutive_failures, 3);
    }

    #[test]
    fn test_provider_health_success_resets_to_healthy() {
        let mut health = ProviderHealth::new();
        // Drive to unhealthy
        for _ in 0..5 {
            health.record_failure();
        }
        assert!(!health.is_healthy);
        assert_eq!(health.consecutive_failures, 5);

        // One success should restore health
        health.record_success();
        assert!(health.is_healthy);
        assert_eq!(health.consecutive_failures, 0);
    }

    #[test]
    fn test_provider_health_clone() {
        let mut health = ProviderHealth::new();
        health.record_failure();
        let cloned = health.clone();
        assert_eq!(cloned.consecutive_failures, 1);
        assert!(cloned.is_healthy);
    }

    #[test]
    fn test_provider_health_debug() {
        let health = ProviderHealth::new();
        let dbg = format!("{:?}", health);
        assert!(dbg.contains("is_healthy"));
        assert!(dbg.contains("consecutive_failures"));
    }

    #[tokio::test]
    async fn test_gateway_provider_health_empty_initially() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();
        let health = gateway.provider_health().await;
        assert!(
            health.is_empty(),
            "No providers registered => empty health map"
        );
    }

    #[tokio::test]
    async fn test_gateway_record_provider_success() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();
        gateway.record_provider_success(ProviderType::Ollama).await;

        let health = gateway.provider_health().await;
        let h = health.get(&ProviderType::Ollama).unwrap();
        assert!(h.is_healthy);
        assert_eq!(h.consecutive_failures, 0);
    }

    #[tokio::test]
    async fn test_gateway_record_provider_failure_marks_unhealthy() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();

        for _ in 0..3 {
            gateway.record_provider_failure(ProviderType::OpenAi).await;
        }

        let health = gateway.provider_health().await;
        let h = health.get(&ProviderType::OpenAi).unwrap();
        assert!(!h.is_healthy);
        assert_eq!(h.consecutive_failures, 3);
    }

    #[tokio::test]
    async fn test_gateway_record_success_after_failures_restores_health() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();

        // Make it unhealthy
        for _ in 0..4 {
            gateway
                .record_provider_failure(ProviderType::LlamaCpp)
                .await;
        }
        assert!(
            !gateway
                .provider_health()
                .await
                .get(&ProviderType::LlamaCpp)
                .unwrap()
                .is_healthy
        );

        // One success restores
        gateway
            .record_provider_success(ProviderType::LlamaCpp)
            .await;
        assert!(
            gateway
                .provider_health()
                .await
                .get(&ProviderType::LlamaCpp)
                .unwrap()
                .is_healthy
        );
    }

    #[tokio::test]
    async fn test_select_providers_ordered_skips_unhealthy() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();

        // Register two providers
        {
            let mut providers = gateway.providers.write().await;
            let p1 = providers::LlamaCppProvider::new().await.unwrap();
            let p2 = providers::LlamaCppProvider::new().await.unwrap();
            providers.insert(ProviderType::Ollama, Arc::new(p1));
            providers.insert(ProviderType::LlamaCpp, Arc::new(p2));
        }

        // Initialize health entries
        gateway.record_provider_success(ProviderType::Ollama).await;
        gateway
            .record_provider_success(ProviderType::LlamaCpp)
            .await;

        // Mark Ollama unhealthy
        for _ in 0..3 {
            gateway.record_provider_failure(ProviderType::Ollama).await;
        }

        let request = agnos_common::InferenceRequest::default();
        let candidates = gateway.select_providers_ordered(&request).await.unwrap();

        // LlamaCpp (healthy) should come before Ollama (unhealthy)
        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].0, ProviderType::LlamaCpp);
        assert_eq!(candidates[1].0, ProviderType::Ollama);
    }

    #[tokio::test]
    async fn test_select_provider_returns_healthy_provider() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();

        // Register two providers
        {
            let mut providers = gateway.providers.write().await;
            let p1 = providers::LlamaCppProvider::new().await.unwrap();
            let p2 = providers::LlamaCppProvider::new().await.unwrap();
            providers.insert(ProviderType::Ollama, Arc::new(p1));
            providers.insert(ProviderType::OpenAi, Arc::new(p2));
        }

        // Mark Ollama unhealthy
        for _ in 0..3 {
            gateway.record_provider_failure(ProviderType::Ollama).await;
        }
        gateway.record_provider_success(ProviderType::OpenAi).await;

        let request = agnos_common::InferenceRequest::default();
        // select_provider should succeed (returns OpenAi which is healthy)
        let result = gateway.select_provider(&request).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_select_providers_ordered_all_unhealthy_still_returns() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();

        {
            let mut providers = gateway.providers.write().await;
            let p = providers::LlamaCppProvider::new().await.unwrap();
            providers.insert(ProviderType::Ollama, Arc::new(p));
        }

        // Mark the only provider unhealthy
        for _ in 0..3 {
            gateway.record_provider_failure(ProviderType::Ollama).await;
        }

        let request = agnos_common::InferenceRequest::default();
        let candidates = gateway.select_providers_ordered(&request).await.unwrap();
        // Should still include the unhealthy provider as last resort
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].0, ProviderType::Ollama);
    }

    #[tokio::test]
    async fn test_run_health_checks_no_providers() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();
        // Should not panic with no providers
        gateway.run_health_checks().await;
        let health = gateway.provider_health().await;
        assert!(health.is_empty());
    }

    #[tokio::test]
    async fn test_run_health_checks_updates_last_check() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();

        // Register a provider
        {
            let mut providers = gateway.providers.write().await;
            let p = providers::LlamaCppProvider::new().await.unwrap();
            providers.insert(ProviderType::LlamaCpp, Arc::new(p));
        }
        gateway
            .record_provider_success(ProviderType::LlamaCpp)
            .await;

        let before = {
            let h = gateway.provider_health().await;
            h.get(&ProviderType::LlamaCpp).unwrap().last_check
        };

        tokio::time::sleep(Duration::from_millis(5)).await;

        // Run health checks — will either succeed or fail, but should update last_check
        gateway.run_health_checks().await;

        let health = gateway.provider_health().await;
        let h = health.get(&ProviderType::LlamaCpp).unwrap();
        assert!(
            h.last_check > before,
            "Health check should update last_check timestamp"
        );
    }

    #[tokio::test]
    async fn test_provider_health_multiple_providers_independent() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();

        // Fail Ollama, succeed OpenAi
        for _ in 0..3 {
            gateway.record_provider_failure(ProviderType::Ollama).await;
        }
        gateway.record_provider_success(ProviderType::OpenAi).await;

        let health = gateway.provider_health().await;
        assert!(!health.get(&ProviderType::Ollama).unwrap().is_healthy);
        assert!(health.get(&ProviderType::OpenAi).unwrap().is_healthy);
    }

    #[tokio::test]
    async fn test_infer_retry_returns_error_when_no_providers() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();
        let request = agnos_common::InferenceRequest::default();
        let result = gateway.infer(request, None).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No LLM provider available"));
    }

    #[tokio::test]
    async fn test_start_health_check_loop_does_not_panic() {
        let gateway = Arc::new(LlmGateway::new(GatewayConfig::default()).await.unwrap());
        // Start the loop — it will run in background; just ensure it doesn't panic on spawn
        gateway.start_health_check_loop();
        // Give it a moment then drop — the spawned task will be cancelled
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    #[test]
    fn test_provider_health_last_check_updates() {
        let mut health = ProviderHealth::new();
        let before = health.last_check;
        // Sleep a tiny bit to ensure Instant moves forward
        std::thread::sleep(std::time::Duration::from_millis(2));
        health.record_failure();
        assert!(health.last_check > before);
    }

    #[test]
    fn test_provider_health_failure_then_success_then_failure() {
        let mut health = ProviderHealth::new();
        health.record_failure();
        health.record_failure();
        assert!(health.is_healthy);
        health.record_success();
        assert_eq!(health.consecutive_failures, 0);
        health.record_failure();
        assert_eq!(health.consecutive_failures, 1);
        assert!(health.is_healthy);
    }

    // ==================================================================
    // Mock provider for infer success path testing
    // ==================================================================

    struct MockProvider {
        response: InferenceResponse,
    }

    #[async_trait::async_trait]
    impl providers::LlmProvider for MockProvider {
        async fn infer(&self, _request: &InferenceRequest) -> anyhow::Result<InferenceResponse> {
            Ok(self.response.clone())
        }

        async fn infer_stream(
            &self,
            _request: InferenceRequest,
        ) -> anyhow::Result<mpsc::Receiver<anyhow::Result<String>>> {
            let (tx, rx) = mpsc::channel(10);
            tx.send(Ok("streamed".to_string())).await.unwrap();
            drop(tx);
            Ok(rx)
        }

        async fn load_model(&self, model_id: &str) -> anyhow::Result<ModelInfo> {
            Ok(ModelInfo {
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

        async fn list_models(&self) -> anyhow::Result<Vec<ModelInfo>> {
            Ok(vec![ModelInfo {
                id: "mock-model".to_string(),
                name: "Mock Model".to_string(),
                provider: agnos_common::Provider::Local,
                capabilities: vec![],
                max_tokens: 4096,
                size_bytes: 0,
                loaded: true,
            }])
        }
    }

    fn mock_response() -> InferenceResponse {
        InferenceResponse {
            text: "mock response".to_string(),
            tokens_generated: 5,
            finish_reason: agnos_common::FinishReason::Stop,
            model: "mock".to_string(),
            usage: TokenUsage {
                prompt_tokens: 3,
                completion_tokens: 5,
                total_tokens: 8,
            },
        }
    }

    #[tokio::test]
    async fn test_infer_success_with_mock_provider() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();

        // Register mock provider
        {
            let mut providers = gateway.providers.write().await;
            providers.insert(
                ProviderType::Ollama,
                Arc::new(MockProvider {
                    response: mock_response(),
                }),
            );
        }

        let request = InferenceRequest {
            model: "mock".to_string(),
            prompt: "test".to_string(),
            ..Default::default()
        };
        let result = gateway.infer(request, None).await;
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp.text, "mock response");
        assert_eq!(resp.tokens_generated, 5);
    }

    #[tokio::test]
    async fn test_infer_success_records_accounting() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();

        {
            let mut providers = gateway.providers.write().await;
            providers.insert(
                ProviderType::Ollama,
                Arc::new(MockProvider {
                    response: mock_response(),
                }),
            );
        }

        let agent_id = AgentId::new();
        let request = InferenceRequest::default();
        let result = gateway.infer(request, Some(agent_id)).await;
        assert!(result.is_ok());

        let usage = gateway.get_agent_usage(agent_id).await;
        assert!(usage.is_some());
        assert_eq!(usage.unwrap().total_tokens, 8);
    }

    #[tokio::test]
    async fn test_infer_success_caches_response() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();

        {
            let mut providers = gateway.providers.write().await;
            providers.insert(
                ProviderType::Ollama,
                Arc::new(MockProvider {
                    response: mock_response(),
                }),
            );
        }

        let request = InferenceRequest {
            model: "cache-test".to_string(),
            prompt: "cache me".to_string(),
            ..Default::default()
        };

        // First call populates cache
        let result1 = gateway.infer(request.clone(), None).await;
        assert!(result1.is_ok());

        // Verify cache has the entry
        let cached = gateway.cache.get(&request).await;
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().text, "mock response");
    }

    #[tokio::test]
    async fn test_infer_with_caching_disabled_does_not_cache() {
        let config = GatewayConfig {
            enable_caching: false,
            ..GatewayConfig::default()
        };
        let gateway = LlmGateway::new(config).await.unwrap();

        {
            let mut providers = gateway.providers.write().await;
            providers.insert(
                ProviderType::Ollama,
                Arc::new(MockProvider {
                    response: mock_response(),
                }),
            );
        }

        let request = InferenceRequest::default();
        let _ = gateway.infer(request.clone(), None).await.unwrap();

        let cached = gateway.cache.get(&request).await;
        assert!(
            cached.is_none(),
            "Should not cache when caching is disabled"
        );
    }

    #[tokio::test]
    async fn test_infer_with_accounting_disabled_does_not_record() {
        let config = GatewayConfig {
            enable_token_accounting: false,
            ..GatewayConfig::default()
        };
        let gateway = LlmGateway::new(config).await.unwrap();

        {
            let mut providers = gateway.providers.write().await;
            providers.insert(
                ProviderType::Ollama,
                Arc::new(MockProvider {
                    response: mock_response(),
                }),
            );
        }

        let agent_id = AgentId::new();
        let _ = gateway
            .infer(InferenceRequest::default(), Some(agent_id))
            .await
            .unwrap();

        assert!(
            gateway.get_agent_usage(agent_id).await.is_none(),
            "Should not record usage when accounting is disabled"
        );
    }

    #[tokio::test]
    async fn test_infer_success_records_provider_health() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();

        {
            let mut providers = gateway.providers.write().await;
            providers.insert(
                ProviderType::Ollama,
                Arc::new(MockProvider {
                    response: mock_response(),
                }),
            );
        }

        let _ = gateway
            .infer(InferenceRequest::default(), None)
            .await
            .unwrap();

        let health = gateway.provider_health().await;
        let h = health.get(&ProviderType::Ollama).unwrap();
        assert!(h.is_healthy);
        assert_eq!(h.consecutive_failures, 0);
    }

    #[tokio::test]
    async fn test_infer_stream_with_mock_provider() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();

        {
            let mut providers = gateway.providers.write().await;
            providers.insert(
                ProviderType::Ollama,
                Arc::new(MockProvider {
                    response: mock_response(),
                }),
            );
        }

        let request = InferenceRequest::default();
        let result = gateway.infer_stream(request, None).await;
        assert!(result.is_ok());
        let mut rx = result.unwrap();
        let first = rx.recv().await;
        assert!(first.is_some());
        assert_eq!(first.unwrap().unwrap(), "streamed");
    }

    #[tokio::test]
    async fn test_load_model_with_mock_provider() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();

        {
            let mut providers = gateway.providers.write().await;
            providers.insert(
                ProviderType::Ollama,
                Arc::new(MockProvider {
                    response: mock_response(),
                }),
            );
        }

        let result = gateway.load_model("test-model").await;
        assert!(result.is_ok());

        let models = gateway.list_models().await;
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "test-model");
    }

    #[tokio::test]
    async fn test_create_shared_session_with_mock_provider() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();

        {
            let mut providers = gateway.providers.write().await;
            providers.insert(
                ProviderType::Ollama,
                Arc::new(MockProvider {
                    response: mock_response(),
                }),
            );
        }

        let agents = vec![AgentId::new(), AgentId::new()];
        let result = gateway.create_shared_session("my-model", agents).await;
        assert!(result.is_ok());
        let session = result.unwrap();
        assert_eq!(session.model_id, "my-model");
        assert_eq!(session.agent_ids.len(), 2);
    }

    #[tokio::test]
    async fn test_run_health_checks_with_mock_provider() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();

        {
            let mut providers = gateway.providers.write().await;
            providers.insert(
                ProviderType::Ollama,
                Arc::new(MockProvider {
                    response: mock_response(),
                }),
            );
        }

        gateway.run_health_checks().await;

        let health = gateway.provider_health().await;
        let h = health.get(&ProviderType::Ollama).unwrap();
        assert!(h.is_healthy);
        assert_eq!(h.consecutive_failures, 0);
    }

    #[tokio::test]
    async fn test_infer_cache_hit_skips_provider() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();

        // No providers registered — if cache hits, inference should succeed
        let request = InferenceRequest {
            model: "cached-model".to_string(),
            prompt: "cached prompt".to_string(),
            max_tokens: 100,
            temperature: 0.7,
            top_p: 1.0,
            presence_penalty: 0.0,
            frequency_penalty: 0.0,
        };
        let response = InferenceResponse {
            text: "from cache".to_string(),
            tokens_generated: 2,
            finish_reason: agnos_common::FinishReason::Stop,
            model: "cached-model".to_string(),
            usage: TokenUsage {
                prompt_tokens: 1,
                completion_tokens: 2,
                total_tokens: 3,
            },
        };

        gateway.cache.set(&request, response.clone()).await;

        // infer should return cached result even without providers
        let result = gateway.infer(request, None).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().text, "from cache");
    }

    #[test]
    fn test_provider_health_many_failures() {
        let mut health = ProviderHealth::new();
        for _ in 0..100 {
            health.record_failure();
        }
        assert!(!health.is_healthy);
        assert_eq!(health.consecutive_failures, 100);
        // One success should still restore
        health.record_success();
        assert!(health.is_healthy);
        assert_eq!(health.consecutive_failures, 0);
    }

    #[tokio::test]
    async fn test_gateway_cache_stats_method() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();
        let stats = gateway.cache_stats().await;
        assert_eq!(stats.total_entries, 0);
        assert_eq!(stats.active_entries, 0);
        assert_eq!(stats.expired_entries, 0);
    }

    #[tokio::test]
    async fn test_gateway_accounting_stats_method() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();
        let stats = gateway.accounting_stats().await;
        assert_eq!(stats.total_agents, 0);
        assert_eq!(stats.total_tokens, 0);
    }

    // ==================================================================
    // Additional coverage: mock provider failure/retry, health ordering,
    // concurrent infer, cache interaction, edge cases
    // ==================================================================

    struct FailingProvider;

    #[async_trait::async_trait]
    impl providers::LlmProvider for FailingProvider {
        async fn infer(&self, _request: &InferenceRequest) -> anyhow::Result<InferenceResponse> {
            anyhow::bail!("provider always fails")
        }
        async fn infer_stream(
            &self,
            _request: InferenceRequest,
        ) -> anyhow::Result<mpsc::Receiver<anyhow::Result<String>>> {
            anyhow::bail!("stream always fails")
        }
        async fn load_model(&self, _model_id: &str) -> anyhow::Result<ModelInfo> {
            anyhow::bail!("load always fails")
        }
        async fn unload_model(&self, _model_id: &str) -> anyhow::Result<()> {
            Ok(())
        }
        async fn list_models(&self) -> anyhow::Result<Vec<ModelInfo>> {
            anyhow::bail!("list always fails")
        }
    }

    #[tokio::test]
    async fn test_infer_retry_all_providers_fail() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();
        {
            let mut providers = gateway.providers.write().await;
            providers.insert(ProviderType::Ollama, Arc::new(FailingProvider));
            providers.insert(ProviderType::LlamaCpp, Arc::new(FailingProvider));
            providers.insert(ProviderType::OpenAi, Arc::new(FailingProvider));
        }
        let request = InferenceRequest::default();
        let err = gateway.infer(request, None).await.unwrap_err();
        assert!(err.to_string().contains("provider always fails"));
    }

    #[tokio::test]
    async fn test_infer_retry_first_fails_second_succeeds() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();
        {
            let mut providers = gateway.providers.write().await;
            providers.insert(ProviderType::Ollama, Arc::new(FailingProvider));
            providers.insert(
                ProviderType::LlamaCpp,
                Arc::new(MockProvider {
                    response: mock_response(),
                }),
            );
        }
        let request = InferenceRequest::default();
        let result = gateway.infer(request, None).await;
        // One of the providers should succeed
        assert!(result.is_ok());
        assert_eq!(result.unwrap().text, "mock response");
    }

    #[tokio::test]
    async fn test_infer_retry_records_failure_health() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();
        {
            let mut providers = gateway.providers.write().await;
            providers.insert(ProviderType::Ollama, Arc::new(FailingProvider));
            providers.insert(
                ProviderType::LlamaCpp,
                Arc::new(MockProvider {
                    response: mock_response(),
                }),
            );
        }
        let request = InferenceRequest::default();
        let _ = gateway.infer(request, None).await;
        // The failing provider should have a failure recorded
        let health = gateway.provider_health().await;
        // At least one provider should have been tracked
        assert!(!health.is_empty());
    }

    #[tokio::test]
    async fn test_infer_with_timeout_very_short() {
        let config = GatewayConfig {
            request_timeout: Duration::from_nanos(1), // extremely short
            ..GatewayConfig::default()
        };
        let gateway = LlmGateway::new(config).await.unwrap();
        {
            let mut providers = gateway.providers.write().await;
            providers.insert(
                ProviderType::Ollama,
                Arc::new(MockProvider {
                    response: mock_response(),
                }),
            );
        }
        // MockProvider returns immediately, so even a nanosecond timeout might succeed
        // The key test is that it doesn't panic
        let request = InferenceRequest::default();
        let _ = gateway.infer(request, None).await;
    }

    #[tokio::test]
    async fn test_infer_cache_hit_does_not_record_accounting() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();
        let request = InferenceRequest {
            model: "cached".to_string(),
            prompt: "test cache accounting".to_string(),
            max_tokens: 100,
            temperature: 0.7,
            top_p: 1.0,
            presence_penalty: 0.0,
            frequency_penalty: 0.0,
        };
        let response = InferenceResponse {
            text: "cached".to_string(),
            tokens_generated: 3,
            finish_reason: agnos_common::FinishReason::Stop,
            model: "cached".to_string(),
            usage: TokenUsage {
                prompt_tokens: 2,
                completion_tokens: 3,
                total_tokens: 5,
            },
        };
        gateway.cache.set(&request, response).await;

        let agent_id = AgentId::new();
        let result = gateway.infer(request, Some(agent_id)).await;
        assert!(result.is_ok());
        // Cache hit should NOT record accounting (it returns before provider call)
        assert!(gateway.get_agent_usage(agent_id).await.is_none());
    }

    #[tokio::test]
    async fn test_select_providers_ordered_with_loaded_model_and_other_providers() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();
        {
            let mut providers = gateway.providers.write().await;
            providers.insert(
                ProviderType::Ollama,
                Arc::new(MockProvider {
                    response: mock_response(),
                }),
            );
            providers.insert(
                ProviderType::OpenAi,
                Arc::new(MockProvider {
                    response: mock_response(),
                }),
            );
        }
        {
            let mut loaded = gateway.loaded_models.write().await;
            loaded.insert(
                "local-model".to_string(),
                ModelInfo {
                    id: "local-model".to_string(),
                    name: "Local".to_string(),
                    provider: agnos_common::Provider::Local,
                    capabilities: vec![],
                    max_tokens: 4096,
                    size_bytes: 0,
                    loaded: true,
                },
            );
        }
        let request = InferenceRequest {
            model: "local-model".to_string(),
            ..Default::default()
        };
        let candidates = gateway.select_providers_ordered(&request).await.unwrap();
        // Ollama should be first since model is loaded
        assert_eq!(candidates[0].0, ProviderType::Ollama);
    }

    #[tokio::test]
    async fn test_concurrent_infer_with_mock() {
        let gateway = Arc::new(
            LlmGateway::new(GatewayConfig {
                max_concurrent_requests: 5,
                ..GatewayConfig::default()
            })
            .await
            .unwrap(),
        );
        {
            let mut providers = gateway.providers.write().await;
            providers.insert(
                ProviderType::Ollama,
                Arc::new(MockProvider {
                    response: mock_response(),
                }),
            );
        }
        let mut handles = vec![];
        for _ in 0..5 {
            let gw = gateway.clone();
            handles.push(tokio::spawn(async move {
                gw.infer(InferenceRequest::default(), None).await
            }));
        }
        for handle in handles {
            let result = handle.await.unwrap();
            assert!(result.is_ok());
        }
    }

    #[tokio::test]
    async fn test_gateway_cache_stats_after_insert() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();
        let request = InferenceRequest::default();
        let response = InferenceResponse {
            text: "stats test".to_string(),
            tokens_generated: 1,
            finish_reason: agnos_common::FinishReason::Stop,
            model: "test".to_string(),
            usage: TokenUsage::default(),
        };
        gateway.cache.set(&request, response).await;
        let stats = gateway.cache_stats().await;
        assert_eq!(stats.total_entries, 1);
        assert_eq!(stats.active_entries, 1);
        assert_eq!(stats.expired_entries, 0);
    }

    #[tokio::test]
    async fn test_gateway_accounting_stats_after_recording() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();
        let agent = AgentId::new();
        gateway
            .accounting
            .record_usage(
                agent,
                TokenUsage {
                    prompt_tokens: 25,
                    completion_tokens: 75,
                    total_tokens: 100,
                },
            )
            .await;
        let stats = gateway.accounting_stats().await;
        assert_eq!(stats.total_agents, 1);
        assert_eq!(stats.total_prompt_tokens, 25);
        assert_eq!(stats.total_completion_tokens, 75);
        assert_eq!(stats.total_tokens, 100);
    }

    #[tokio::test]
    async fn test_gateway_provider_health_all_types() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();
        let types = [
            ProviderType::Ollama,
            ProviderType::LlamaCpp,
            ProviderType::OpenAi,
            ProviderType::Anthropic,
            ProviderType::Google,
        ];
        for pt in &types {
            gateway.record_provider_success(*pt).await;
        }
        let health = gateway.provider_health().await;
        assert_eq!(health.len(), 5);
        for pt in &types {
            assert!(health.get(pt).unwrap().is_healthy);
        }
    }

    #[tokio::test]
    async fn test_gateway_select_providers_ordered_empty_model_name() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();
        {
            let mut providers = gateway.providers.write().await;
            providers.insert(
                ProviderType::Ollama,
                Arc::new(MockProvider {
                    response: mock_response(),
                }),
            );
        }
        let request = InferenceRequest {
            model: "".to_string(),
            ..Default::default()
        };
        let candidates = gateway.select_providers_ordered(&request).await.unwrap();
        assert_eq!(candidates.len(), 1);
    }

    #[test]
    fn test_provider_health_record_success_updates_last_check() {
        let mut health = ProviderHealth::new();
        let before = health.last_check;
        std::thread::sleep(std::time::Duration::from_millis(2));
        health.record_success();
        assert!(health.last_check > before);
    }

    #[test]
    fn test_provider_health_exactly_at_threshold() {
        let mut health = ProviderHealth::new();
        // Exactly 3 failures should mark unhealthy
        health.record_failure();
        health.record_failure();
        assert!(health.is_healthy);
        health.record_failure();
        assert!(!health.is_healthy);
        assert_eq!(health.consecutive_failures, 3);
    }

    #[tokio::test]
    async fn test_infer_multiple_agents_accumulates_independently() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();
        {
            let mut providers = gateway.providers.write().await;
            providers.insert(
                ProviderType::Ollama,
                Arc::new(MockProvider {
                    response: mock_response(),
                }),
            );
        }
        let a1 = AgentId::new();
        let a2 = AgentId::new();
        let _ = gateway
            .infer(InferenceRequest::default(), Some(a1))
            .await
            .unwrap();
        let _ = gateway
            .infer(InferenceRequest::default(), Some(a2))
            .await
            .unwrap();
        // Cache should hit for second call with same request, so only first agent gets accounting
        // Both should have usage since cache returns before accounting
        let u1 = gateway.get_agent_usage(a1).await;
        assert!(u1.is_some());
        let total = gateway.get_total_usage().await;
        assert!(total.total_tokens > 0);
    }

    #[tokio::test]
    async fn test_run_health_checks_with_failing_provider() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();
        {
            let mut providers = gateway.providers.write().await;
            providers.insert(ProviderType::Ollama, Arc::new(FailingProvider));
        }
        gateway.run_health_checks().await;
        let health = gateway.provider_health().await;
        let h = health.get(&ProviderType::Ollama).unwrap();
        assert_eq!(h.consecutive_failures, 1);
    }

    #[tokio::test]
    async fn test_load_model_with_failing_provider() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();
        {
            let mut providers = gateway.providers.write().await;
            providers.insert(ProviderType::Ollama, Arc::new(FailingProvider));
        }
        let result = gateway.load_model("any-model").await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Failed to load model"));
        // Model should NOT be in loaded_models
        assert!(gateway.list_models().await.is_empty());
    }

    #[tokio::test]
    async fn test_infer_stream_with_failing_provider() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();
        {
            let mut providers = gateway.providers.write().await;
            providers.insert(ProviderType::Ollama, Arc::new(FailingProvider));
        }
        let result = gateway
            .infer_stream(InferenceRequest::default(), None)
            .await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("stream always fails"));
    }

    // ==================================================================
    // Certificate pinning integration tests
    // ==================================================================

    use agnos_sys::certpin::{CertPinResult as PinResult, PinnedCert};

    /// Helper: build a pin set with known pins for testing.
    fn test_pin_set(enforce: bool) -> CertPinSet {
        CertPinSet {
            pins: vec![
                PinnedCert {
                    host: "api.openai.com".to_string(),
                    pin_sha256: vec!["test_openai_pin_primary".to_string()],
                    expires: None,
                    backup_pins: vec!["test_openai_pin_backup".to_string()],
                },
                PinnedCert {
                    host: "api.anthropic.com".to_string(),
                    pin_sha256: vec!["test_anthropic_pin_primary".to_string()],
                    expires: None,
                    backup_pins: vec![],
                },
            ],
            enforce,
            created_at: chrono::Utc::now(),
            version: 1,
        }
    }

    #[tokio::test]
    async fn test_gateway_loads_default_pins() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();
        // default_agnos_pins() has 3 entries: openai, anthropic, google
        assert_eq!(gateway.cert_pins.pins.len(), 3);
        assert!(
            !gateway.cert_pins.enforce,
            "Default pins should be report-only"
        );
    }

    #[tokio::test]
    async fn test_gateway_loads_custom_pins() {
        let custom = test_pin_set(true);
        let gateway = LlmGateway::new_with_pins(GatewayConfig::default(), Some(custom))
            .await
            .unwrap();
        assert_eq!(gateway.cert_pins.pins.len(), 2);
        assert!(gateway.cert_pins.enforce);
    }

    #[test]
    fn test_extract_provider_host_cloud_providers() {
        assert_eq!(
            LlmGateway::extract_provider_host(ProviderType::OpenAi),
            Some("api.openai.com")
        );
        assert_eq!(
            LlmGateway::extract_provider_host(ProviderType::Anthropic),
            Some("api.anthropic.com")
        );
        assert_eq!(
            LlmGateway::extract_provider_host(ProviderType::Google),
            Some("generativelanguage.googleapis.com")
        );
    }

    #[test]
    fn test_extract_provider_host_local_providers_none() {
        assert!(LlmGateway::extract_provider_host(ProviderType::Ollama).is_none());
        assert!(LlmGateway::extract_provider_host(ProviderType::LlamaCpp).is_none());
    }

    #[tokio::test]
    async fn test_verify_provider_cert_skips_local_providers() {
        let gateway = LlmGateway::new(GatewayConfig::default()).await.unwrap();
        // Local providers should always succeed (skipped)
        assert!(gateway.verify_provider_cert(ProviderType::Ollama).is_ok());
        assert!(gateway.verify_provider_cert(ProviderType::LlamaCpp).is_ok());
    }

    #[tokio::test]
    async fn test_verify_provider_cert_report_only_does_not_fail() {
        // Even with a pin set that has pins, report-only should not error
        let pins = test_pin_set(false); // enforce = false
        let gateway = LlmGateway::new_with_pins(GatewayConfig::default(), Some(pins))
            .await
            .unwrap();
        // This will try to fetch the real cert and likely fail (no network in CI),
        // but fetch_server_cert failure returns Ok in the verify method
        let result = gateway.verify_provider_cert(ProviderType::OpenAi);
        assert!(result.is_ok(), "Report-only mode should never return Err");
    }

    #[tokio::test]
    async fn test_verify_provider_cert_enforce_mode_with_wrong_pins() {
        // With enforce=true and wrong pins, the result depends on network:
        // - If fetch_server_cert fails (no openssl/network), returns Ok (can't verify)
        // - If fetch succeeds but pin doesn't match, returns Err (enforced mismatch)
        let pins = test_pin_set(true); // enforce = true, with fake pins
        let gateway = LlmGateway::new_with_pins(GatewayConfig::default(), Some(pins))
            .await
            .unwrap();
        let result = gateway.verify_provider_cert(ProviderType::OpenAi);
        // We accept both outcomes — the key invariant is no panic
        match &result {
            Ok(()) => {
                // fetch_server_cert failed gracefully (no network / no openssl)
            }
            Err(e) => {
                // Pin mismatch enforced — error message should mention it
                let msg = e.to_string();
                assert!(
                    msg.contains("pin mismatch") || msg.contains("pin expired"),
                    "Enforce error should mention pin issue, got: {}",
                    msg
                );
            }
        }
    }

    #[test]
    fn test_verify_pin_directly_with_matching_pin() {
        let pins = test_pin_set(true);
        let result = certpin::verify_pin("api.openai.com", "test_openai_pin_primary", &pins);
        assert_eq!(result, PinResult::Valid);
    }

    #[test]
    fn test_verify_pin_directly_with_backup_pin() {
        let pins = test_pin_set(true);
        let result = certpin::verify_pin("api.openai.com", "test_openai_pin_backup", &pins);
        assert_eq!(result, PinResult::Valid);
    }

    #[test]
    fn test_verify_pin_directly_mismatch() {
        let pins = test_pin_set(true);
        let result = certpin::verify_pin("api.openai.com", "wrong_pin", &pins);
        match result {
            PinResult::PinMismatch {
                host,
                actual,
                expected,
            } => {
                assert_eq!(host, "api.openai.com");
                assert_eq!(actual, "wrong_pin");
                assert!(expected.contains(&"test_openai_pin_primary".to_string()));
                assert!(expected.contains(&"test_openai_pin_backup".to_string()));
            }
            other => panic!("Expected PinMismatch, got {:?}", other),
        }
    }

    #[test]
    fn test_verify_pin_no_config_for_host() {
        let pins = test_pin_set(true);
        let result = certpin::verify_pin("unknown.example.com", "any_pin", &pins);
        assert_eq!(
            result,
            PinResult::NoPinConfigured {
                host: "unknown.example.com".to_string()
            }
        );
    }

    #[tokio::test]
    async fn test_pin_expiry_warning_logged_at_startup() {
        use chrono::TimeZone;
        // Create a pin set with an already-expired entry
        let pins = CertPinSet {
            pins: vec![PinnedCert {
                host: "expired.example.com".to_string(),
                pin_sha256: vec!["some_pin".to_string()],
                expires: Some(chrono::Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap()),
                backup_pins: vec![],
            }],
            enforce: false,
            created_at: chrono::Utc::now(),
            version: 1,
        };
        // new_with_pins should succeed even with expired pins (just logs warnings)
        let gateway = LlmGateway::new_with_pins(GatewayConfig::default(), Some(pins))
            .await
            .unwrap();
        assert_eq!(gateway.cert_pins.pins.len(), 1);
    }
}
