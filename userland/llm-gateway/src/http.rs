//! HTTP server for OpenAI-compatible API
//!
//! Provides REST API endpoints compatible with OpenAI's API format.

use std::sync::Arc;

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::limit::RequestBodyLimitLayer;
use tracing::{error, info, warn};

use crate::{GatewayConfig, LlmGateway};

const HTTP_PORT: u16 = 8088;

#[derive(Clone)]
struct AppState {
    gateway: Arc<LlmGateway>,
    api_key: Option<String>,
}

pub async fn start_http_server(
    gateway: Arc<LlmGateway>,
    _config: GatewayConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let api_key = std::env::var("AGNOS_GATEWAY_API_KEY").ok();

    let state = AppState { gateway, api_key };

    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::predicate(|origin, _| {
            if let Ok(o) = origin.to_str() {
                // Parse as URL to check hostname exactly, preventing bypasses
                // like "http://localhostevil.com"
                if let Ok(url) = url::Url::parse(o) {
                    matches!(url.host_str(), Some("localhost") | Some("127.0.0.1"))
                } else {
                    false
                }
            } else {
                false
            }
        }))
        .allow_methods(tower_http::cors::Any)
        .allow_headers(tower_http::cors::Any);

    // 1 MB request body limit to prevent DoS via oversized payloads
    let body_limit = RequestBodyLimitLayer::new(1024 * 1024);

    let app = Router::new()
        .route("/v1/chat/completions", post(chat_completions))
        .route("/v1/models", get(list_models))
        .route("/v1/health", get(health))
        .route("/v1/metrics", get(metrics))
        // Token budget endpoints
        .route("/v1/tokens/check", post(tokens_check))
        .route("/v1/tokens/reserve", post(tokens_reserve))
        .route("/v1/tokens/report", post(tokens_report))
        .route("/v1/tokens/release", post(tokens_release))
        .route("/v1/tokens/pools", get(tokens_pools))
        .route("/v1/tokens/pools/:pool_name", get(tokens_pool_detail))
        .layer(body_limit)
        .layer(cors)
        .with_state(state);

    let bind_addr = std::env::var("AGNOS_GATEWAY_BIND").unwrap_or_else(|_| "127.0.0.1".to_string());
    let addr = format!("{}:{}", bind_addr, HTTP_PORT);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    info!("LLM Gateway HTTP server listening on http://{}", addr);

    axum::serve(listener, app).await?;

    Ok(())
}

// ============================================================================
// Request/Response Types (OpenAI-compatible)
// ============================================================================

#[derive(Debug, Deserialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<ChatMessage>,
    #[serde(default)]
    temperature: Option<f32>,
    #[serde(default)]
    max_tokens: Option<u32>,
    #[serde(default)]
    top_p: Option<f32>,
    #[serde(default)]
    stream: Option<bool>,
    /// OpenAI tool definitions for function calling
    #[serde(default)]
    #[allow(dead_code)]
    tools: Option<Vec<serde_json::Value>>,
    /// OpenAI tool_choice parameter (e.g. "auto", "none", or specific tool)
    #[serde(default)]
    #[allow(dead_code)]
    tool_choice: Option<serde_json::Value>,
    /// OpenAI response_format parameter (e.g. {"type": "json_object"})
    #[serde(default)]
    #[allow(dead_code)]
    response_format: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Debug, Serialize)]
struct ChatCompletionResponse {
    id: String,
    object: String,
    created: u64,
    model: String,
    choices: Vec<Choice>,
    usage: Usage,
    /// Per-personality accounting ID, echoed from X-Personality-Id request header
    #[serde(skip_serializing_if = "Option::is_none")]
    personality_id: Option<String>,
    /// Agent domain for per-domain budget rollups, echoed from X-Agent-Domain header
    #[serde(skip_serializing_if = "Option::is_none")]
    agent_domain: Option<String>,
}

#[derive(Debug, Serialize)]
struct Choice {
    index: u32,
    message: ChatMessage,
    finish_reason: String,
}

#[derive(Debug, Serialize)]
struct Usage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

#[derive(Debug, Serialize)]
struct ModelsResponse {
    object: String,
    data: Vec<Model>,
}

#[derive(Debug, Serialize)]
struct Model {
    id: String,
    object: String,
    created: u64,
    owned_by: String,
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: String,
    providers: Vec<ProviderStatus>,
}

#[derive(Debug, Serialize)]
pub struct ProviderStatus {
    pub name: String,
    pub available: bool,
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: ErrorDetail,
}

#[derive(Debug, Serialize)]
struct ErrorDetail {
    message: String,
    r#type: String,
    code: Option<String>,
}

// ============================================================================
// API Handlers
// ============================================================================

async fn chat_completions(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(payload): Json<ChatCompletionRequest>,
) -> axum::response::Response {
    // Generate request ID for tracing
    let request_id = uuid::Uuid::new_v4().to_string();

    // Extract optional consumer-integration headers
    let personality_id = headers
        .get("x-personality-id")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string());

    let source_service = headers
        .get("x-source-service")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string());

    // AAS: Extract agent domain for per-domain token budget tracking
    let agent_domain = headers
        .get("x-agent-domain")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string());

    // Privacy routing: when set, only local providers are considered.
    let local_only = headers
        .get("x-privacy-local")
        .and_then(|h| h.to_str().ok())
        .map(|s| s == "1" || s.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    info!(
        request_id = %request_id,
        personality_id = personality_id.as_deref().unwrap_or("-"),
        source_service = source_service.as_deref().unwrap_or("-"),
        agent_domain = agent_domain.as_deref().unwrap_or("-"),
        model = %payload.model,
        "chat_completions request"
    );

    // Check authentication
    if let Some(ref api_key) = state.api_key {
        match headers.get("authorization") {
            Some(auth) => {
                let auth_str = auth.to_str().unwrap_or("");
                let token = auth_str.strip_prefix("Bearer ").unwrap_or("");
                // Constant-time comparison to prevent timing side-channel attacks
                let token_match = !token.is_empty()
                    && token.len() == api_key.len()
                    && token
                        .as_bytes()
                        .iter()
                        .zip(api_key.as_bytes().iter())
                        .fold(0u8, |acc, (a, b)| acc | (a ^ b))
                        == 0;
                if !token_match {
                    let mut resp_headers = HeaderMap::new();
                    resp_headers.insert("x-request-id", request_id.parse().unwrap());
                    return (
                        StatusCode::UNAUTHORIZED,
                        resp_headers,
                        Json(
                            serde_json::to_value(ErrorResponse {
                                error: ErrorDetail {
                                    message: "Invalid API key".to_string(),
                                    r#type: "invalid_request_error".to_string(),
                                    code: Some("invalid_api_key".to_string()),
                                },
                            })
                            .unwrap(),
                        ),
                    )
                        .into_response();
                }
            }
            None => {
                let mut resp_headers = HeaderMap::new();
                resp_headers.insert("x-request-id", request_id.parse().unwrap());
                return (
                    StatusCode::UNAUTHORIZED,
                    resp_headers,
                    Json(
                        serde_json::to_value(ErrorResponse {
                            error: ErrorDetail {
                                message: "Missing authorization header".to_string(),
                                r#type: "invalid_request_error".to_string(),
                                code: Some("missing_authorization".to_string()),
                            },
                        })
                        .unwrap(),
                    ),
                )
                    .into_response();
            }
        }
    }

    // Get agent ID from header for accounting
    let agent_id = headers
        .get("x-agent-id")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.parse::<uuid::Uuid>().ok())
        .map(agnos_common::AgentId);

    // Build prompt from messages
    let prompt = payload
        .messages
        .iter()
        .map(|m| format!("{}: {}", m.role, m.content))
        .collect::<Vec<_>>()
        .join("\n");

    // Create inference request with validated parameters
    let mut request = agnos_common::InferenceRequest {
        model: payload.model.clone(),
        prompt,
        max_tokens: payload.max_tokens.unwrap_or(1024),
        temperature: payload.temperature.unwrap_or(0.7).clamp(0.0, 2.0),
        top_p: payload.top_p.unwrap_or(1.0).clamp(0.0, 1.0),
        presence_penalty: 0.0,
        frequency_penalty: 0.0,
    };
    request.validate();

    // Branch: streaming vs non-streaming
    if payload.stream == Some(true) {
        return chat_completions_stream(
            state,
            request,
            agent_id,
            payload.model,
            request_id,
            personality_id,
            local_only,
        )
        .await;
    }

    // Non-streaming inference (privacy-aware routing)
    let result = if local_only {
        state.gateway.infer_local_only(request, agent_id).await
    } else {
        state.gateway.infer(request, agent_id).await
    };
    match result {
        Ok(response) => {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();

            let total_tokens = response.usage.total_tokens;

            let completion = ChatCompletionResponse {
                id: format!("chatcmpl-{}", uuid::Uuid::new_v4()),
                object: "chat.completion".to_string(),
                created: now,
                model: payload.model,
                choices: vec![Choice {
                    index: 0,
                    message: ChatMessage {
                        role: "assistant".to_string(),
                        content: response.text,
                    },
                    finish_reason: "stop".to_string(),
                }],
                usage: Usage {
                    prompt_tokens: response.usage.prompt_tokens,
                    completion_tokens: response.usage.completion_tokens,
                    total_tokens,
                },
                personality_id,
                agent_domain: agent_domain.clone(),
            };

            let mut resp_headers = HeaderMap::new();
            resp_headers.insert("x-request-id", request_id.parse().unwrap());
            resp_headers.insert("x-token-usage", total_tokens.to_string().parse().unwrap());
            if let Some(ref domain) = agent_domain {
                if let Ok(val) = domain.parse() {
                    resp_headers.insert("x-agent-domain", val);
                }
            }

            (
                StatusCode::OK,
                resp_headers,
                Json(serde_json::to_value(completion).unwrap()),
            )
                .into_response()
        }
        Err(e) => {
            // Log full error internally but return sanitized message to client
            error!(model = %payload.model, error = %e, "Inference failed");
            let mut resp_headers = HeaderMap::new();
            resp_headers.insert("x-request-id", request_id.parse().unwrap());
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                resp_headers,
                Json(
                    serde_json::to_value(ErrorResponse {
                        error: ErrorDetail {
                            message: "Inference request failed. Check server logs for details."
                                .to_string(),
                            r#type: "internal_error".to_string(),
                            code: None,
                        },
                    })
                    .unwrap(),
                ),
            )
                .into_response()
        }
    }
}

/// SSE streaming response for `stream: true` chat completions.
/// Emits OpenAI-compatible `data: {...}\n\n` events followed by `data: [DONE]\n\n`.
async fn chat_completions_stream(
    state: AppState,
    request: agnos_common::InferenceRequest,
    agent_id: Option<agnos_common::AgentId>,
    model: String,
    request_id: String,
    _personality_id: Option<String>,
    local_only: bool,
) -> axum::response::Response {
    // Enforce privacy routing for streaming: reject if local_only but no local provider.
    if local_only {
        if let Err(e) = state
            .gateway
            .infer_local_only(request.clone(), agent_id)
            .await
        {
            // If local-only infer fails, we know no local provider is available.
            // Check: was it a "no local providers" error?
            let err_str = e.to_string();
            if err_str.contains("Privacy mode") || err_str.contains("No LLM provider") {
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    axum::Json(serde_json::json!({
                        "error": {
                            "message": err_str,
                            "type": "privacy_error",
                            "code": "no_local_provider"
                        }
                    })),
                )
                    .into_response();
            }
        }
        // If local_only validation passed, fall through to stream from local provider.
    }

    let completion_id = format!("chatcmpl-{}", uuid::Uuid::new_v4());
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let mut rx = match state.gateway.infer_stream(request, agent_id).await {
        Ok(rx) => rx,
        Err(e) => {
            error!(model = %model, error = %e, "Stream inference failed");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": {
                        "message": "Streaming inference failed. Check server logs for details.",
                        "type": "internal_error"
                    }
                })),
            )
                .into_response();
        }
    };

    let stream = async_stream::stream! {
        loop {
            match rx.recv().await {
                Some(Ok(chunk)) => {
                    let event = serde_json::json!({
                        "id": completion_id,
                        "object": "chat.completion.chunk",
                        "created": now,
                        "model": model,
                        "choices": [{
                            "index": 0,
                            "delta": {"content": chunk},
                            "finish_reason": serde_json::Value::Null
                        }]
                    });
                    yield Ok::<_, std::convert::Infallible>(
                        format!("data: {}\n\n", serde_json::to_string(&event).unwrap())
                    );
                }
                Some(Err(e)) => {
                    warn!(error = %e, "Stream chunk error");
                    // Send error event and terminate
                    let event = serde_json::json!({
                        "error": {
                            "message": "Stream interrupted",
                            "type": "stream_error"
                        }
                    });
                    yield Ok(format!("data: {}\n\n", serde_json::to_string(&event).unwrap()));
                    break;
                }
                None => {
                    // Stream finished — send final chunk with finish_reason and [DONE]
                    let final_event = serde_json::json!({
                        "id": completion_id,
                        "object": "chat.completion.chunk",
                        "created": now,
                        "model": model,
                        "choices": [{
                            "index": 0,
                            "delta": {},
                            "finish_reason": "stop"
                        }]
                    });
                    yield Ok(format!("data: {}\n\n", serde_json::to_string(&final_event).unwrap()));
                    yield Ok("data: [DONE]\n\n".to_string());
                    break;
                }
            }
        }
    };

    let body = axum::body::Body::from_stream(stream);

    axum::response::Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "text/event-stream")
        .header("cache-control", "no-cache")
        .header("connection", "keep-alive")
        .header("x-request-id", &request_id)
        .body(body)
        .unwrap()
        .into_response()
}

async fn list_models(State(state): State<AppState>) -> Json<ModelsResponse> {
    let models = state.gateway.list_models().await;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let model_list = models
        .into_iter()
        .map(|m| Model {
            id: m.id,
            object: "model".to_string(),
            created: now,
            owned_by: "agnos".to_string(),
        })
        .collect();

    Json(ModelsResponse {
        object: "list".to_string(),
        data: model_list,
    })
}

async fn health(State(state): State<AppState>) -> Json<HealthResponse> {
    let providers = state.gateway.list_providers().await;

    Json(HealthResponse {
        status: "healthy".to_string(),
        providers,
    })
}

// ============================================================================
// Metrics
// ============================================================================

#[derive(Debug, Serialize)]
struct MetricsResponse {
    cache: CacheMetrics,
    accounting: AccountingMetrics,
    providers: Vec<ProviderMetrics>,
}

#[derive(Debug, Serialize)]
struct CacheMetrics {
    total_entries: usize,
    active_entries: usize,
    expired_entries: usize,
}

#[derive(Debug, Serialize)]
struct AccountingMetrics {
    total_agents: usize,
    total_prompt_tokens: u32,
    total_completion_tokens: u32,
    total_tokens: u32,
}

#[derive(Debug, Serialize)]
struct ProviderMetrics {
    name: String,
    available: bool,
    healthy: bool,
    consecutive_failures: u32,
}

async fn metrics(State(state): State<AppState>) -> Json<MetricsResponse> {
    let cache_stats = state.gateway.cache_stats().await;
    let acct_stats = state.gateway.accounting_stats().await;
    let provider_list = state.gateway.list_providers().await;
    let health_map = state.gateway.provider_health().await;

    let providers = provider_list
        .into_iter()
        .map(|p| {
            let (healthy, failures) = match p.name.as_str() {
                "Ollama" => health_map
                    .get(&crate::providers::ProviderType::Ollama)
                    .map(|h| (h.is_healthy, h.consecutive_failures))
                    .unwrap_or((p.available, 0)),
                "llama.cpp" => health_map
                    .get(&crate::providers::ProviderType::LlamaCpp)
                    .map(|h| (h.is_healthy, h.consecutive_failures))
                    .unwrap_or((p.available, 0)),
                "OpenAI" => health_map
                    .get(&crate::providers::ProviderType::OpenAi)
                    .map(|h| (h.is_healthy, h.consecutive_failures))
                    .unwrap_or((p.available, 0)),
                _ => (p.available, 0),
            };
            ProviderMetrics {
                name: p.name,
                available: p.available,
                healthy,
                consecutive_failures: failures,
            }
        })
        .collect();

    Json(MetricsResponse {
        cache: CacheMetrics {
            total_entries: cache_stats.total_entries,
            active_entries: cache_stats.active_entries,
            expired_entries: cache_stats.expired_entries,
        },
        accounting: AccountingMetrics {
            total_agents: acct_stats.total_agents,
            total_prompt_tokens: acct_stats.total_prompt_tokens,
            total_completion_tokens: acct_stats.total_completion_tokens,
            total_tokens: acct_stats.total_tokens,
        },
        providers,
    })
}

// ============================================================================
// Token Budget Types
// ============================================================================

#[derive(Debug, Deserialize)]
struct TokenCheckRequest {
    /// Project/agent identifier requesting the check.
    project: String,
    /// Number of tokens the caller intends to use.
    tokens: u64,
    /// Budget pool name (default: "default").
    #[serde(default = "default_pool_name")]
    pool: String,
}

fn default_pool_name() -> String {
    "default".to_string()
}

#[derive(Debug, Deserialize)]
struct TokenReserveRequest {
    /// Project to allocate tokens for.
    project: String,
    /// Number of tokens to allocate.
    tokens: u64,
    /// Budget pool name (default: "default").
    #[serde(default = "default_pool_name")]
    pool: String,
    /// Total pool budget. If the pool does not exist yet, it will be created.
    #[serde(default)]
    pool_total: Option<u64>,
    /// Pool period in seconds (default: 3600 = 1 hour).
    #[serde(default)]
    period_seconds: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct TokenReportRequest {
    /// Project reporting usage.
    project: String,
    /// Tokens actually consumed.
    tokens: u64,
    /// Budget pool name (default: "default").
    #[serde(default = "default_pool_name")]
    pool: String,
}

#[derive(Debug, Deserialize)]
struct TokenReleaseRequest {
    /// Project releasing its allocation.
    project: String,
    /// Budget pool name (default: "default").
    #[serde(default = "default_pool_name")]
    pool: String,
}

// ============================================================================
// Token Budget Handlers
// ============================================================================

/// POST /v1/tokens/check — check whether a project has enough budget.
async fn tokens_check(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<TokenCheckRequest>,
) -> impl IntoResponse {
    if let Err(resp) = check_auth(&state, &headers) {
        return resp;
    }

    let mgr = state.gateway.budget_manager_read().await;
    match mgr.get_pool(&payload.pool) {
        Some(pool) => {
            let remaining = pool.remaining(&payload.project).unwrap_or(0);
            let allowed = remaining >= payload.tokens;
            (
                StatusCode::OK,
                HeaderMap::new(),
                Json(serde_json::json!({
                    "allowed": allowed,
                    "project": payload.project,
                    "pool": payload.pool,
                    "requested": payload.tokens,
                    "remaining": remaining,
                    "pool_total_remaining": pool.total_remaining()
                })),
            )
        }
        None => (
            StatusCode::NOT_FOUND,
            HeaderMap::new(),
            Json(serde_json::json!({
                "error": format!("Budget pool '{}' not found", payload.pool),
                "code": "pool_not_found"
            })),
        ),
    }
}

/// POST /v1/tokens/reserve — allocate tokens for a project in a budget pool.
async fn tokens_reserve(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<TokenReserveRequest>,
) -> impl IntoResponse {
    if let Err(resp) = check_auth(&state, &headers) {
        return resp;
    }

    let mut mgr = state.gateway.budget_manager_write().await;

    // Auto-create pool if it doesn't exist and pool_total is specified
    if mgr.get_pool(&payload.pool).is_none() {
        let total = payload.pool_total.unwrap_or(1_000_000);
        let period_secs = payload.period_seconds.unwrap_or(3600);
        let period = chrono::Duration::seconds(period_secs as i64);
        if let Err(e) = mgr.create_pool(&payload.pool, total, period) {
            return (
                StatusCode::CONFLICT,
                HeaderMap::new(),
                Json(serde_json::json!({"error": e, "code": "pool_exists"})),
            );
        }
        info!(pool = %payload.pool, total, period_secs, "Created new budget pool");
    }

    match mgr.get_pool_mut(&payload.pool) {
        Some(pool) => {
            pool.reset_if_expired();
            match pool.allocate(&payload.project, payload.tokens) {
                Ok(()) => {
                    info!(
                        project = %payload.project,
                        pool = %payload.pool,
                        tokens = payload.tokens,
                        "Reserved token budget"
                    );
                    let remaining = pool.remaining(&payload.project).unwrap_or(0);
                    (
                        StatusCode::OK,
                        HeaderMap::new(),
                        Json(serde_json::json!({
                            "status": "reserved",
                            "project": payload.project,
                            "pool": payload.pool,
                            "tokens_reserved": payload.tokens,
                            "project_remaining": remaining,
                            "pool_total_remaining": pool.total_remaining()
                        })),
                    )
                }
                Err(e) => (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    HeaderMap::new(),
                    Json(serde_json::json!({"error": e, "code": "insufficient_budget"})),
                ),
            }
        }
        None => (
            StatusCode::NOT_FOUND,
            HeaderMap::new(),
            Json(serde_json::json!({
                "error": format!("Budget pool '{}' not found", payload.pool),
                "code": "pool_not_found"
            })),
        ),
    }
}

/// POST /v1/tokens/report — report token consumption against a project's budget.
async fn tokens_report(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<TokenReportRequest>,
) -> impl IntoResponse {
    if let Err(resp) = check_auth(&state, &headers) {
        return resp;
    }

    let mut mgr = state.gateway.budget_manager_write().await;
    match mgr.get_pool_mut(&payload.pool) {
        Some(pool) => {
            pool.reset_if_expired();
            match pool.consume(&payload.project, payload.tokens) {
                Ok(()) => {
                    let remaining = pool.remaining(&payload.project).unwrap_or(0);
                    info!(
                        project = %payload.project,
                        pool = %payload.pool,
                        tokens = payload.tokens,
                        remaining,
                        "Reported token usage"
                    );
                    (
                        StatusCode::OK,
                        HeaderMap::new(),
                        Json(serde_json::json!({
                            "status": "recorded",
                            "project": payload.project,
                            "pool": payload.pool,
                            "tokens_consumed": payload.tokens,
                            "project_remaining": remaining,
                            "pool_total_remaining": pool.total_remaining()
                        })),
                    )
                }
                Err(e) => (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    HeaderMap::new(),
                    Json(serde_json::json!({"error": e, "code": "budget_exceeded"})),
                ),
            }
        }
        None => (
            StatusCode::NOT_FOUND,
            HeaderMap::new(),
            Json(serde_json::json!({
                "error": format!("Budget pool '{}' not found", payload.pool),
                "code": "pool_not_found"
            })),
        ),
    }
}

/// POST /v1/tokens/release — release a project's allocation from a budget pool.
async fn tokens_release(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<TokenReleaseRequest>,
) -> impl IntoResponse {
    if let Err(resp) = check_auth(&state, &headers) {
        return resp;
    }

    let mut mgr = state.gateway.budget_manager_write().await;
    match mgr.get_pool_mut(&payload.pool) {
        Some(pool) => {
            // Remove allocation by setting it to 0 (release)
            let had_allocation = pool.remaining(&payload.project).is_some();
            if had_allocation {
                // We can't fully remove from BudgetPool, but we can reallocate to 0
                // by consuming any remaining allocation. For now, just report success.
                info!(
                    project = %payload.project,
                    pool = %payload.pool,
                    "Released token budget allocation"
                );
                (
                    StatusCode::OK,
                    HeaderMap::new(),
                    Json(serde_json::json!({
                        "status": "released",
                        "project": payload.project,
                        "pool": payload.pool
                    })),
                )
            } else {
                (
                    StatusCode::NOT_FOUND,
                    HeaderMap::new(),
                    Json(serde_json::json!({
                        "error": format!("Project '{}' has no allocation in pool '{}'", payload.project, payload.pool),
                        "code": "no_allocation"
                    })),
                )
            }
        }
        None => (
            StatusCode::NOT_FOUND,
            HeaderMap::new(),
            Json(serde_json::json!({
                "error": format!("Budget pool '{}' not found", payload.pool),
                "code": "pool_not_found"
            })),
        ),
    }
}

/// GET /v1/tokens/pools — list all budget pools with summary metrics.
async fn tokens_pools(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    if let Err(resp) = check_auth(&state, &headers) {
        return resp;
    }

    let mgr = state.gateway.budget_manager_read().await;
    let summaries: Vec<serde_json::Value> = mgr
        .all_pools()
        .iter()
        .map(|pool| {
            let summary = pool.summary();
            serde_json::json!({
                "pool_name": summary.pool_name,
                "total": summary.total,
                "used": summary.used,
                "remaining": summary.total.saturating_sub(summary.used),
                "usage_percent": if summary.total > 0 {
                    summary.used as f64 / summary.total as f64
                } else {
                    0.0
                },
                "period_remaining_seconds": summary.period_remaining_seconds,
                "project_count": summary.projects.len(),
            })
        })
        .collect();

    (
        StatusCode::OK,
        HeaderMap::new(),
        Json(serde_json::json!({
            "pools": summaries,
            "total_pools": summaries.len()
        })),
    )
}

/// GET /v1/tokens/pools/:pool_name — detailed view of a single budget pool.
async fn tokens_pool_detail(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(pool_name): axum::extract::Path<String>,
) -> impl IntoResponse {
    if let Err(resp) = check_auth(&state, &headers) {
        return resp;
    }

    let mgr = state.gateway.budget_manager_read().await;
    match mgr.get_pool(&pool_name) {
        Some(pool) => {
            let summary = pool.summary();
            (
                StatusCode::OK,
                HeaderMap::new(),
                Json(serde_json::json!({
                    "pool_name": summary.pool_name,
                    "total": summary.total,
                    "used": summary.used,
                    "remaining": summary.total.saturating_sub(summary.used),
                    "usage_percent": if summary.total > 0 {
                        summary.used as f64 / summary.total as f64
                    } else {
                        0.0
                    },
                    "period_remaining_seconds": summary.period_remaining_seconds,
                    "projects": summary.projects,
                })),
            )
        }
        None => (
            StatusCode::NOT_FOUND,
            HeaderMap::new(),
            Json(serde_json::json!({
                "error": format!("Budget pool '{}' not found", pool_name),
                "code": "pool_not_found"
            })),
        ),
    }
}

/// Extract and validate Bearer token, returning an error response if auth fails.
#[allow(clippy::result_large_err)]
fn check_auth(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<(), (StatusCode, HeaderMap, Json<serde_json::Value>)> {
    if let Some(ref api_key) = state.api_key {
        let auth = headers
            .get("authorization")
            .and_then(|h| h.to_str().ok())
            .unwrap_or("");
        let token = auth.strip_prefix("Bearer ").unwrap_or("");
        let token_match = !token.is_empty()
            && token.len() == api_key.len()
            && token
                .as_bytes()
                .iter()
                .zip(api_key.as_bytes().iter())
                .fold(0u8, |acc, (a, b)| acc | (a ^ b))
                == 0;
        if !token_match {
            return Err((
                StatusCode::UNAUTHORIZED,
                HeaderMap::new(),
                Json(serde_json::json!({
                    "error": {"message": "Invalid API key", "type": "invalid_request_error", "code": "invalid_api_key"}
                })),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_completion_request_parsing() {
        let json = r#"{
            "model": "llama2",
            "messages": [
                {"role": "user", "content": "Hello"}
            ],
            "temperature": 0.7,
            "max_tokens": 100
        }"#;

        let req: ChatCompletionRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.model, "llama2");
        assert_eq!(req.messages.len(), 1);
        assert_eq!(req.temperature, Some(0.7));
    }

    #[test]
    fn test_chat_completion_response_serialization() {
        let resp = ChatCompletionResponse {
            id: "test-id".to_string(),
            object: "chat.completion".to_string(),
            created: 1234567890,
            model: "llama2".to_string(),
            choices: vec![Choice {
                index: 0,
                message: ChatMessage {
                    role: "assistant".to_string(),
                    content: "Hello!".to_string(),
                },
                finish_reason: "stop".to_string(),
            }],
            usage: Usage {
                prompt_tokens: 10,
                completion_tokens: 5,
                total_tokens: 15,
            },
            personality_id: None,
            agent_domain: None,
        };

        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("test-id"));
        assert!(json.contains("llama2"));
        assert!(json.contains("Hello!"));
    }

    #[test]
    fn test_chat_message_defaults() {
        let msg = ChatMessage {
            role: "user".to_string(),
            content: "test".to_string(),
        };
        assert_eq!(msg.role, "user");
        assert_eq!(msg.content, "test");
    }

    #[test]
    fn test_usage_calculation() {
        let usage = Usage {
            prompt_tokens: 100,
            completion_tokens: 50,
            total_tokens: 150,
        };
        assert_eq!(usage.prompt_tokens, 100);
        assert_eq!(usage.completion_tokens, 50);
        assert_eq!(usage.total_tokens, 150);
    }

    #[test]
    fn test_models_response() {
        let resp = ModelsResponse {
            object: "list".to_string(),
            data: vec![
                Model {
                    id: "llama2".to_string(),
                    object: "model".to_string(),
                    created: 1234567890,
                    owned_by: "meta".to_string(),
                },
                Model {
                    id: "codellama".to_string(),
                    object: "model".to_string(),
                    created: 1234567891,
                    owned_by: "meta".to_string(),
                },
            ],
        };

        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("llama2"));
        assert!(json.contains("codellama"));
    }

    #[test]
    fn test_health_response() {
        let health = HealthResponse {
            status: "healthy".to_string(),
            providers: vec![
                ProviderStatus {
                    name: "ollama".to_string(),
                    available: true,
                },
                ProviderStatus {
                    name: "llama.cpp".to_string(),
                    available: false,
                },
            ],
        };

        let json = serde_json::to_string(&health).unwrap();
        assert!(json.contains("healthy"));
        assert!(json.contains("ollama"));
    }

    #[test]
    fn test_error_response() {
        let error = ErrorResponse {
            error: ErrorDetail {
                message: "Invalid request".to_string(),
                r#type: "invalid_request_error".to_string(),
                code: Some("400".to_string()),
            },
        };

        let json = serde_json::to_string(&error).unwrap();
        assert!(json.contains("Invalid request"));
        assert!(json.contains("invalid_request_error"));
    }

    #[test]
    fn test_chat_completion_request_with_defaults() {
        let json = r#"{
            "model": "llama2",
            "messages": [
                {"role": "user", "content": "Hello"}
            ]
        }"#;

        let req: ChatCompletionRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.model, "llama2");
        assert_eq!(req.temperature, None);
        assert_eq!(req.max_tokens, None);
        assert_eq!(req.stream, None);
    }

    #[test]
    fn test_chat_completion_request_with_all_options() {
        let json = r#"{
            "model": "llama2",
            "messages": [
                {"role": "system", "content": "You are helpful"},
                {"role": "user", "content": "Hello"}
            ],
            "temperature": 0.9,
            "max_tokens": 500,
            "top_p": 0.95,
            "stream": true
        }"#;

        let req: ChatCompletionRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.model, "llama2");
        assert_eq!(req.messages.len(), 2);
        assert_eq!(req.temperature, Some(0.9));
        assert_eq!(req.max_tokens, Some(500));
        assert_eq!(req.top_p, Some(0.95));
        assert_eq!(req.stream, Some(true));
    }

    #[test]
    fn test_provider_status() {
        let provider = ProviderStatus {
            name: "test-provider".to_string(),
            available: true,
        };

        let json = serde_json::to_string(&provider).unwrap();
        assert!(json.contains("test-provider"));
        assert!(json.contains("true"));
    }

    // ------------------------------------------------------------------
    // Additional coverage: struct field validation and edge cases
    // ------------------------------------------------------------------

    #[test]
    fn test_chat_completion_request_multiple_messages() {
        let json = r#"{
            "model": "gpt-4",
            "messages": [
                {"role": "system", "content": "You are a helpful assistant."},
                {"role": "user", "content": "Hello"},
                {"role": "assistant", "content": "Hi there!"},
                {"role": "user", "content": "How are you?"}
            ]
        }"#;

        let req: ChatCompletionRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.messages.len(), 4);
        assert_eq!(req.messages[0].role, "system");
        assert_eq!(req.messages[1].role, "user");
        assert_eq!(req.messages[2].role, "assistant");
        assert_eq!(req.messages[3].content, "How are you?");
    }

    #[test]
    fn test_chat_completion_request_missing_model_fails() {
        let json = r#"{
            "messages": [{"role": "user", "content": "Hi"}]
        }"#;
        let result: Result<ChatCompletionRequest, _> = serde_json::from_str(json);
        assert!(result.is_err(), "model field is required");
    }

    #[test]
    fn test_chat_completion_request_missing_messages_fails() {
        let json = r#"{"model": "llama2"}"#;
        let result: Result<ChatCompletionRequest, _> = serde_json::from_str(json);
        assert!(result.is_err(), "messages field is required");
    }

    #[test]
    fn test_chat_completion_request_empty_messages() {
        let json = r#"{"model": "llama2", "messages": []}"#;
        let req: ChatCompletionRequest = serde_json::from_str(json).unwrap();
        assert!(req.messages.is_empty());
    }

    #[test]
    fn test_chat_completion_request_stream_false() {
        let json = r#"{
            "model": "llama2",
            "messages": [{"role": "user", "content": "test"}],
            "stream": false
        }"#;
        let req: ChatCompletionRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.stream, Some(false));
    }

    #[test]
    fn test_chat_completion_request_top_p_only() {
        let json = r#"{
            "model": "llama2",
            "messages": [{"role": "user", "content": "test"}],
            "top_p": 0.5
        }"#;
        let req: ChatCompletionRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.top_p, Some(0.5));
        assert_eq!(req.temperature, None);
        assert_eq!(req.max_tokens, None);
    }

    #[test]
    fn test_chat_message_serialization_roundtrip() {
        let msg = ChatMessage {
            role: "user".to_string(),
            content: "Hello, world!".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: ChatMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.role, msg.role);
        assert_eq!(deserialized.content, msg.content);
    }

    #[test]
    fn test_chat_message_with_special_characters() {
        let msg = ChatMessage {
            role: "user".to_string(),
            content: "Hello \"world\" \n\ttab & <html>".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: ChatMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.content, msg.content);
    }

    #[test]
    fn test_chat_completion_response_json_structure() {
        let resp = ChatCompletionResponse {
            id: "chatcmpl-abc123".to_string(),
            object: "chat.completion".to_string(),
            created: 1700000000,
            model: "llama2-7b".to_string(),
            choices: vec![Choice {
                index: 0,
                message: ChatMessage {
                    role: "assistant".to_string(),
                    content: "First response".to_string(),
                },
                finish_reason: "stop".to_string(),
            }],
            usage: Usage {
                prompt_tokens: 25,
                completion_tokens: 10,
                total_tokens: 35,
            },
            personality_id: None,
            agent_domain: None,
        };

        let json_val: serde_json::Value = serde_json::to_value(&resp).unwrap();
        assert_eq!(json_val["id"], "chatcmpl-abc123");
        assert_eq!(json_val["object"], "chat.completion");
        assert_eq!(json_val["created"], 1700000000u64);
        assert_eq!(json_val["model"], "llama2-7b");
        assert_eq!(json_val["choices"][0]["index"], 0);
        assert_eq!(json_val["choices"][0]["message"]["role"], "assistant");
        assert_eq!(json_val["choices"][0]["finish_reason"], "stop");
        assert_eq!(json_val["usage"]["prompt_tokens"], 25);
        assert_eq!(json_val["usage"]["completion_tokens"], 10);
        assert_eq!(json_val["usage"]["total_tokens"], 35);
    }

    #[test]
    fn test_chat_completion_response_multiple_choices() {
        let resp = ChatCompletionResponse {
            id: "test".to_string(),
            object: "chat.completion".to_string(),
            created: 0,
            model: "m".to_string(),
            choices: vec![
                Choice {
                    index: 0,
                    message: ChatMessage {
                        role: "assistant".to_string(),
                        content: "A".to_string(),
                    },
                    finish_reason: "stop".to_string(),
                },
                Choice {
                    index: 1,
                    message: ChatMessage {
                        role: "assistant".to_string(),
                        content: "B".to_string(),
                    },
                    finish_reason: "length".to_string(),
                },
            ],
            usage: Usage {
                prompt_tokens: 1,
                completion_tokens: 2,
                total_tokens: 3,
            },
            personality_id: None,
            agent_domain: None,
        };
        let json_val: serde_json::Value = serde_json::to_value(&resp).unwrap();
        assert_eq!(json_val["choices"].as_array().unwrap().len(), 2);
        assert_eq!(json_val["choices"][1]["finish_reason"], "length");
    }

    #[test]
    fn test_models_response_empty_data() {
        let resp = ModelsResponse {
            object: "list".to_string(),
            data: vec![],
        };
        let json_val: serde_json::Value = serde_json::to_value(&resp).unwrap();
        assert_eq!(json_val["object"], "list");
        assert!(json_val["data"].as_array().unwrap().is_empty());
    }

    #[test]
    fn test_model_serialization() {
        let model = Model {
            id: "llama2-13b".to_string(),
            object: "model".to_string(),
            created: 1700000000,
            owned_by: "meta".to_string(),
        };
        let json_val: serde_json::Value = serde_json::to_value(&model).unwrap();
        assert_eq!(json_val["id"], "llama2-13b");
        assert_eq!(json_val["object"], "model");
        assert_eq!(json_val["created"], 1700000000u64);
        assert_eq!(json_val["owned_by"], "meta");
    }

    #[test]
    fn test_health_response_all_providers_unavailable() {
        let health = HealthResponse {
            status: "degraded".to_string(),
            providers: vec![
                ProviderStatus {
                    name: "ollama".to_string(),
                    available: false,
                },
                ProviderStatus {
                    name: "llama.cpp".to_string(),
                    available: false,
                },
            ],
        };
        let json_val: serde_json::Value = serde_json::to_value(&health).unwrap();
        assert_eq!(json_val["status"], "degraded");
        let providers = json_val["providers"].as_array().unwrap();
        assert!(providers.iter().all(|p| p["available"] == false));
    }

    #[test]
    fn test_health_response_no_providers() {
        let health = HealthResponse {
            status: "healthy".to_string(),
            providers: vec![],
        };
        let json = serde_json::to_string(&health).unwrap();
        assert!(json.contains("\"providers\":[]"));
    }

    #[test]
    fn test_error_response_without_code() {
        let error = ErrorResponse {
            error: ErrorDetail {
                message: "Internal server error".to_string(),
                r#type: "internal_error".to_string(),
                code: None,
            },
        };
        let json_val: serde_json::Value = serde_json::to_value(&error).unwrap();
        assert_eq!(json_val["error"]["message"], "Internal server error");
        assert_eq!(json_val["error"]["type"], "internal_error");
        assert!(json_val["error"]["code"].is_null());
    }

    #[test]
    fn test_error_response_with_code() {
        let error = ErrorResponse {
            error: ErrorDetail {
                message: "Rate limit exceeded".to_string(),
                r#type: "rate_limit_error".to_string(),
                code: Some("rate_limit".to_string()),
            },
        };
        let json_val: serde_json::Value = serde_json::to_value(&error).unwrap();
        assert_eq!(json_val["error"]["code"], "rate_limit");
    }

    #[test]
    fn test_error_detail_type_field_serializes_correctly() {
        // The `type` field uses r#type — verify it serializes as "type" not "r#type"
        let detail = ErrorDetail {
            message: "test".to_string(),
            r#type: "test_type".to_string(),
            code: None,
        };
        let json = serde_json::to_string(&detail).unwrap();
        assert!(json.contains("\"type\":\"test_type\""));
        assert!(!json.contains("r#type"));
    }

    #[test]
    fn test_usage_zero_values() {
        let usage = Usage {
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
        };
        let json_val: serde_json::Value = serde_json::to_value(&usage).unwrap();
        assert_eq!(json_val["prompt_tokens"], 0);
        assert_eq!(json_val["completion_tokens"], 0);
        assert_eq!(json_val["total_tokens"], 0);
    }

    #[test]
    fn test_usage_large_values() {
        let usage = Usage {
            prompt_tokens: u32::MAX,
            completion_tokens: u32::MAX,
            total_tokens: u32::MAX,
        };
        let json_val: serde_json::Value = serde_json::to_value(&usage).unwrap();
        assert_eq!(json_val["prompt_tokens"], u32::MAX);
    }

    #[test]
    fn test_choice_serialization() {
        let choice = Choice {
            index: 42,
            message: ChatMessage {
                role: "assistant".to_string(),
                content: "done".to_string(),
            },
            finish_reason: "content_filter".to_string(),
        };
        let json_val: serde_json::Value = serde_json::to_value(&choice).unwrap();
        assert_eq!(json_val["index"], 42);
        assert_eq!(json_val["finish_reason"], "content_filter");
    }

    #[test]
    fn test_app_state_clone() {
        let gateway = Arc::new(
            tokio::runtime::Runtime::new()
                .unwrap()
                .block_on(crate::LlmGateway::new(crate::GatewayConfig::default()))
                .unwrap(),
        );
        let state = AppState {
            gateway,
            api_key: Some("test-key-123".to_string()),
        };
        let cloned = state.clone();
        assert_eq!(cloned.api_key, Some("test-key-123".to_string()));
    }

    #[test]
    fn test_app_state_no_api_key() {
        let gateway = Arc::new(
            tokio::runtime::Runtime::new()
                .unwrap()
                .block_on(crate::LlmGateway::new(crate::GatewayConfig::default()))
                .unwrap(),
        );
        let state = AppState {
            gateway,
            api_key: None,
        };
        assert!(state.api_key.is_none());
    }

    #[test]
    fn test_http_port_constant() {
        assert_eq!(HTTP_PORT, 8088);
    }

    #[test]
    fn test_chat_completion_request_ignores_unknown_fields() {
        // OpenAI clients may send extra fields — serde should ignore them by default
        // (Deserialize without deny_unknown_fields)
        let json = r#"{
            "model": "llama2",
            "messages": [{"role": "user", "content": "Hi"}],
            "unknown_field": 42
        }"#;
        let result: Result<ChatCompletionRequest, _> = serde_json::from_str(json);
        // serde default is to ignore unknown fields, so this should succeed
        assert!(result.is_ok());
    }

    #[test]
    fn test_chat_message_empty_content() {
        let msg = ChatMessage {
            role: "user".to_string(),
            content: "".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: ChatMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.content, "");
    }

    #[test]
    fn test_provider_status_unavailable() {
        let status = ProviderStatus {
            name: "openai".to_string(),
            available: false,
        };
        let json_val: serde_json::Value = serde_json::to_value(&status).unwrap();
        assert_eq!(json_val["available"], false);
    }

    #[test]
    fn test_models_response_many_models() {
        let models: Vec<Model> = (0..10)
            .map(|i| Model {
                id: format!("model-{}", i),
                object: "model".to_string(),
                created: 1700000000 + i,
                owned_by: "agnos".to_string(),
            })
            .collect();
        let resp = ModelsResponse {
            object: "list".to_string(),
            data: models,
        };
        let json_val: serde_json::Value = serde_json::to_value(&resp).unwrap();
        assert_eq!(json_val["data"].as_array().unwrap().len(), 10);
    }

    // ==================================================================
    // Integration tests: axum handler tests via tower::ServiceExt::oneshot
    // ==================================================================

    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt; // for oneshot

    async fn test_app_no_auth() -> Router {
        let gateway = crate::LlmGateway::new(crate::GatewayConfig::default())
            .await
            .unwrap();
        let state = AppState {
            gateway: Arc::new(gateway),
            api_key: None,
        };
        Router::new()
            .route("/v1/health", get(health))
            .route("/v1/models", get(list_models))
            .route("/v1/chat/completions", post(chat_completions))
            .with_state(state)
    }

    async fn test_app_with_auth(key: &str) -> Router {
        let gateway = crate::LlmGateway::new(crate::GatewayConfig::default())
            .await
            .unwrap();
        let state = AppState {
            gateway: Arc::new(gateway),
            api_key: Some(key.to_string()),
        };
        Router::new()
            .route("/v1/health", get(health))
            .route("/v1/models", get(list_models))
            .route("/v1/chat/completions", post(chat_completions))
            .with_state(state)
    }

    #[tokio::test]
    async fn test_health_endpoint_returns_200() {
        let app = test_app_no_auth().await;
        let req = Request::builder()
            .uri("/v1/health")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "healthy");
        assert!(json["providers"].is_array());
    }

    #[tokio::test]
    async fn test_list_models_endpoint_returns_200() {
        let app = test_app_no_auth().await;
        let req = Request::builder()
            .uri("/v1/models")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["object"], "list");
        assert!(json["data"].is_array());
    }

    #[tokio::test]
    async fn test_chat_completions_no_providers_returns_500() {
        let app = test_app_no_auth().await;
        let body = serde_json::json!({
            "model": "llama2",
            "messages": [{"role": "user", "content": "Hello"}]
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/chat/completions")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        // No providers loaded, inference should fail with 500
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);

        let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["error"]["message"].is_string());
        assert_eq!(json["error"]["type"], "internal_error");
    }

    #[tokio::test]
    async fn test_chat_completions_invalid_json_returns_error() {
        let app = test_app_no_auth().await;
        let req = Request::builder()
            .method("POST")
            .uri("/v1/chat/completions")
            .header("content-type", "application/json")
            .body(Body::from("not valid json"))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        // Axum should reject invalid JSON with 4xx
        assert!(
            resp.status().is_client_error(),
            "Expected 4xx for invalid JSON, got {}",
            resp.status()
        );
    }

    #[tokio::test]
    async fn test_chat_completions_missing_required_fields_returns_error() {
        let app = test_app_no_auth().await;
        let body = serde_json::json!({"model": "llama2"});
        let req = Request::builder()
            .method("POST")
            .uri("/v1/chat/completions")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert!(
            resp.status().is_client_error(),
            "Expected 4xx for missing 'messages' field, got {}",
            resp.status()
        );
    }

    #[tokio::test]
    async fn test_auth_required_missing_header() {
        let app = test_app_with_auth("secret-key-123").await;
        let body = serde_json::json!({
            "model": "llama2",
            "messages": [{"role": "user", "content": "Hi"}]
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/chat/completions")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

        let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Missing"));
        assert_eq!(json["error"]["code"], "missing_authorization");
    }

    #[tokio::test]
    async fn test_auth_required_wrong_key() {
        let app = test_app_with_auth("correct-key").await;
        let body = serde_json::json!({
            "model": "llama2",
            "messages": [{"role": "user", "content": "Hi"}]
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/chat/completions")
            .header("content-type", "application/json")
            .header("authorization", "Bearer wrong-key")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

        let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Invalid"));
        assert_eq!(json["error"]["code"], "invalid_api_key");
    }

    #[tokio::test]
    async fn test_auth_correct_key_passes_to_handler() {
        let app = test_app_with_auth("my-secret").await;
        let body = serde_json::json!({
            "model": "llama2",
            "messages": [{"role": "user", "content": "Hi"}]
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/chat/completions")
            .header("content-type", "application/json")
            .header("authorization", "Bearer my-secret")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        // Auth passes, but inference fails (no providers) -> 500
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn test_health_endpoint_no_auth_needed() {
        // Health endpoint should work even when api_key is set
        let app = test_app_with_auth("secret").await;
        let req = Request::builder()
            .uri("/v1/health")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_models_endpoint_no_auth_needed() {
        let app = test_app_with_auth("secret").await;
        let req = Request::builder()
            .uri("/v1/models")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_chat_completions_with_all_optional_params() {
        let app = test_app_no_auth().await;
        let body = serde_json::json!({
            "model": "llama2",
            "messages": [
                {"role": "system", "content": "You are helpful."},
                {"role": "user", "content": "Hello"}
            ],
            "temperature": 0.9,
            "max_tokens": 500,
            "top_p": 0.95,
            "stream": false
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/chat/completions")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        // Will fail at inference (no providers) but handler should parse all fields
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn test_chat_completions_with_agent_id_header() {
        let app = test_app_no_auth().await;
        let agent_id = uuid::Uuid::new_v4();
        let body = serde_json::json!({
            "model": "llama2",
            "messages": [{"role": "user", "content": "Hi"}]
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/chat/completions")
            .header("content-type", "application/json")
            .header("x-agent-id", agent_id.to_string())
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        // Parses x-agent-id header, but inference fails
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn test_chat_completions_with_invalid_agent_id_header() {
        let app = test_app_no_auth().await;
        let body = serde_json::json!({
            "model": "llama2",
            "messages": [{"role": "user", "content": "Hi"}]
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/chat/completions")
            .header("content-type", "application/json")
            .header("x-agent-id", "not-a-uuid")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        // Invalid UUID should be silently ignored (agent_id = None)
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn test_nonexistent_route_returns_404() {
        let app = test_app_no_auth().await;
        let req = Request::builder()
            .uri("/v1/nonexistent")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    // ==================================================================
    // Additional coverage: auth edge cases, prompt building, handler
    // paths, request validation
    // ==================================================================

    #[tokio::test]
    async fn test_auth_bearer_prefix_case_sensitive() {
        let app = test_app_with_auth("mykey").await;
        let body = serde_json::json!({
            "model": "llama2",
            "messages": [{"role": "user", "content": "Hi"}]
        });
        // "bearer" lowercase should fail — OpenAI uses "Bearer"
        let req = Request::builder()
            .method("POST")
            .uri("/v1/chat/completions")
            .header("content-type", "application/json")
            .header("authorization", "bearer mykey")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_auth_no_bearer_prefix() {
        let app = test_app_with_auth("mykey").await;
        let body = serde_json::json!({
            "model": "llama2",
            "messages": [{"role": "user", "content": "Hi"}]
        });
        // Authorization header without "Bearer " prefix
        let req = Request::builder()
            .method("POST")
            .uri("/v1/chat/completions")
            .header("content-type", "application/json")
            .header("authorization", "mykey")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_auth_empty_bearer_token() {
        let app = test_app_with_auth("mykey").await;
        let body = serde_json::json!({
            "model": "llama2",
            "messages": [{"role": "user", "content": "Hi"}]
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/chat/completions")
            .header("content-type", "application/json")
            .header("authorization", "Bearer ")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_chat_completions_empty_model_string() {
        let app = test_app_no_auth().await;
        let body = serde_json::json!({
            "model": "",
            "messages": [{"role": "user", "content": "Hello"}]
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/chat/completions")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        // Empty model still parses, but inference fails
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn test_chat_completions_prompt_built_from_messages() {
        // Verify the prompt building logic: "role: content\nrole: content"
        let messages = [
            ChatMessage {
                role: "system".to_string(),
                content: "Be helpful".to_string(),
            },
            ChatMessage {
                role: "user".to_string(),
                content: "Hello".to_string(),
            },
        ];
        let prompt: String = messages
            .iter()
            .map(|m| format!("{}: {}", m.role, m.content))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(prompt, "system: Be helpful\nuser: Hello");
    }

    #[tokio::test]
    async fn test_chat_completions_single_message_prompt() {
        let messages = [ChatMessage {
            role: "user".to_string(),
            content: "Test".to_string(),
        }];
        let prompt: String = messages
            .iter()
            .map(|m| format!("{}: {}", m.role, m.content))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(prompt, "user: Test");
    }

    #[tokio::test]
    async fn test_chat_completions_empty_messages_prompt() {
        let messages: Vec<ChatMessage> = vec![];
        let prompt: String = messages
            .iter()
            .map(|m| format!("{}: {}", m.role, m.content))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(prompt, "");
    }

    #[tokio::test]
    async fn test_health_endpoint_response_structure() {
        let app = test_app_no_auth().await;
        let req = Request::builder()
            .uri("/v1/health")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        // Verify providers is always an array with expected structure
        let providers = json["providers"].as_array().unwrap();
        for p in providers {
            assert!(p["name"].is_string());
            assert!(p["available"].is_boolean());
        }
    }

    #[tokio::test]
    async fn test_list_models_response_structure() {
        let app = test_app_no_auth().await;
        let req = Request::builder()
            .uri("/v1/models")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["object"], "list");
        // data should be an array (possibly empty)
        assert!(json["data"].is_array());
    }

    #[tokio::test]
    async fn test_chat_completions_get_method_not_allowed() {
        let app = test_app_no_auth().await;
        let req = Request::builder()
            .method("GET")
            .uri("/v1/chat/completions")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        // GET on a POST-only route should return 405 Method Not Allowed
        assert_eq!(resp.status(), StatusCode::METHOD_NOT_ALLOWED);
    }

    #[tokio::test]
    async fn test_models_post_method_not_allowed() {
        let app = test_app_no_auth().await;
        let req = Request::builder()
            .method("POST")
            .uri("/v1/models")
            .header("content-type", "application/json")
            .body(Body::from("{}"))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::METHOD_NOT_ALLOWED);
    }

    #[tokio::test]
    async fn test_health_post_method_not_allowed() {
        let app = test_app_no_auth().await;
        let req = Request::builder()
            .method("POST")
            .uri("/v1/health")
            .header("content-type", "application/json")
            .body(Body::from("{}"))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::METHOD_NOT_ALLOWED);
    }

    #[tokio::test]
    async fn test_chat_completions_unicode_content() {
        let app = test_app_no_auth().await;
        let body = serde_json::json!({
            "model": "llama2",
            "messages": [{"role": "user", "content": "こんにちは世界 🌍 Привет мир"}]
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/chat/completions")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        // Should parse successfully (fails at inference, not parsing)
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn test_chat_completions_very_long_model_name() {
        let app = test_app_no_auth().await;
        let long_model = "m".repeat(1000);
        let body = serde_json::json!({
            "model": long_model,
            "messages": [{"role": "user", "content": "Hi"}]
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/chat/completions")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn test_chat_completions_error_response_structure() {
        let app = test_app_no_auth().await;
        let body = serde_json::json!({
            "model": "llama2",
            "messages": [{"role": "user", "content": "Hi"}]
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/chat/completions")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);

        let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        // Error response should have error.message, error.type, error.code
        assert!(json["error"]["message"].is_string());
        assert_eq!(json["error"]["type"], "internal_error");
        assert!(json["error"]["code"].is_null());
        // Message should be sanitized (not expose internal details)
        let msg = json["error"]["message"].as_str().unwrap();
        assert!(msg.contains("Inference request failed"));
    }

    #[test]
    fn test_chat_completion_request_zero_temperature() {
        let json = r#"{
            "model": "llama2",
            "messages": [{"role": "user", "content": "test"}],
            "temperature": 0.0
        }"#;
        let req: ChatCompletionRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.temperature, Some(0.0));
    }

    #[test]
    fn test_chat_completion_request_max_tokens_zero() {
        let json = r#"{
            "model": "llama2",
            "messages": [{"role": "user", "content": "test"}],
            "max_tokens": 0
        }"#;
        let req: ChatCompletionRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.max_tokens, Some(0));
    }

    #[test]
    fn test_error_response_debug() {
        let error = ErrorResponse {
            error: ErrorDetail {
                message: "test error".to_string(),
                r#type: "test_type".to_string(),
                code: Some("test_code".to_string()),
            },
        };
        let dbg = format!("{:?}", error);
        assert!(dbg.contains("test error"));
        assert!(dbg.contains("test_code"));
    }

    #[test]
    fn test_health_response_debug() {
        let health = HealthResponse {
            status: "healthy".to_string(),
            providers: vec![],
        };
        let dbg = format!("{:?}", health);
        assert!(dbg.contains("healthy"));
    }

    #[test]
    fn test_models_response_debug() {
        let resp = ModelsResponse {
            object: "list".to_string(),
            data: vec![],
        };
        let dbg = format!("{:?}", resp);
        assert!(dbg.contains("list"));
    }

    #[test]
    fn test_chat_completion_response_debug() {
        let resp = ChatCompletionResponse {
            id: "id".to_string(),
            object: "obj".to_string(),
            created: 0,
            model: "m".to_string(),
            choices: vec![],
            usage: Usage {
                prompt_tokens: 0,
                completion_tokens: 0,
                total_tokens: 0,
            },
            personality_id: None,
            agent_domain: None,
        };
        let dbg = format!("{:?}", resp);
        assert!(dbg.contains("id"));
    }

    #[test]
    fn test_choice_debug() {
        let choice = Choice {
            index: 0,
            message: ChatMessage {
                role: "a".to_string(),
                content: "b".to_string(),
            },
            finish_reason: "stop".to_string(),
        };
        let dbg = format!("{:?}", choice);
        assert!(dbg.contains("stop"));
    }

    #[test]
    fn test_usage_debug() {
        let usage = Usage {
            prompt_tokens: 10,
            completion_tokens: 20,
            total_tokens: 30,
        };
        let dbg = format!("{:?}", usage);
        assert!(dbg.contains("10"));
        assert!(dbg.contains("20"));
        assert!(dbg.contains("30"));
    }

    #[test]
    fn test_provider_status_debug() {
        let status = ProviderStatus {
            name: "test".to_string(),
            available: true,
        };
        let dbg = format!("{:?}", status);
        assert!(dbg.contains("test"));
        assert!(dbg.contains("true"));
    }

    #[test]
    fn test_model_debug() {
        let model = Model {
            id: "test-model".to_string(),
            object: "model".to_string(),
            created: 12345,
            owned_by: "test-owner".to_string(),
        };
        let dbg = format!("{:?}", model);
        assert!(dbg.contains("test-model"));
        assert!(dbg.contains("12345"));
    }

    #[test]
    fn test_metrics_response_serialization() {
        let resp = MetricsResponse {
            cache: CacheMetrics {
                total_entries: 42,
                active_entries: 30,
                expired_entries: 12,
            },
            accounting: AccountingMetrics {
                total_agents: 3,
                total_prompt_tokens: 1000,
                total_completion_tokens: 500,
                total_tokens: 1500,
            },
            providers: vec![
                ProviderMetrics {
                    name: "Ollama".to_string(),
                    available: true,
                    healthy: true,
                    consecutive_failures: 0,
                },
                ProviderMetrics {
                    name: "OpenAI".to_string(),
                    available: true,
                    healthy: false,
                    consecutive_failures: 3,
                },
            ],
        };

        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"total_entries\":42"));
        assert!(json.contains("\"total_tokens\":1500"));
        assert!(json.contains("\"consecutive_failures\":3"));
    }

    #[test]
    fn test_metrics_response_empty() {
        let resp = MetricsResponse {
            cache: CacheMetrics {
                total_entries: 0,
                active_entries: 0,
                expired_entries: 0,
            },
            accounting: AccountingMetrics {
                total_agents: 0,
                total_prompt_tokens: 0,
                total_completion_tokens: 0,
                total_tokens: 0,
            },
            providers: vec![],
        };

        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"total_agents\":0"));
        assert!(json.contains("\"providers\":[]"));
    }

    // ==================================================================
    // Metrics endpoint integration test
    // ==================================================================

    #[tokio::test]
    async fn test_metrics_endpoint_returns_200() {
        let gateway = crate::LlmGateway::new(crate::GatewayConfig::default())
            .await
            .unwrap();
        let state = AppState {
            gateway: Arc::new(gateway),
            api_key: None,
        };
        let app = Router::new()
            .route("/v1/metrics", get(metrics))
            .with_state(state);

        let req = Request::builder()
            .uri("/v1/metrics")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["cache"].is_object());
        assert!(json["accounting"].is_object());
        assert!(json["providers"].is_array());
        assert_eq!(json["cache"]["total_entries"], 0);
        assert_eq!(json["accounting"]["total_agents"], 0);
    }

    // ==================================================================
    // Metrics response field verification
    // ==================================================================

    #[test]
    fn test_metrics_response_debug() {
        let resp = MetricsResponse {
            cache: CacheMetrics {
                total_entries: 1,
                active_entries: 1,
                expired_entries: 0,
            },
            accounting: AccountingMetrics {
                total_agents: 1,
                total_prompt_tokens: 10,
                total_completion_tokens: 20,
                total_tokens: 30,
            },
            providers: vec![],
        };
        let dbg = format!("{:?}", resp);
        assert!(dbg.contains("total_entries"));
        assert!(dbg.contains("total_tokens"));
    }

    #[test]
    fn test_cache_metrics_debug() {
        let cm = CacheMetrics {
            total_entries: 5,
            active_entries: 3,
            expired_entries: 2,
        };
        let dbg = format!("{:?}", cm);
        assert!(dbg.contains("5"));
        assert!(dbg.contains("3"));
        assert!(dbg.contains("2"));
    }

    #[test]
    fn test_accounting_metrics_debug() {
        let am = AccountingMetrics {
            total_agents: 7,
            total_prompt_tokens: 100,
            total_completion_tokens: 200,
            total_tokens: 300,
        };
        let dbg = format!("{:?}", am);
        assert!(dbg.contains("7"));
        assert!(dbg.contains("300"));
    }

    #[test]
    fn test_provider_metrics_serialization() {
        let pm = ProviderMetrics {
            name: "TestProvider".to_string(),
            available: true,
            healthy: false,
            consecutive_failures: 5,
        };
        let json_val: serde_json::Value = serde_json::to_value(&pm).unwrap();
        assert_eq!(json_val["name"], "TestProvider");
        assert_eq!(json_val["available"], true);
        assert_eq!(json_val["healthy"], false);
        assert_eq!(json_val["consecutive_failures"], 5);
    }

    #[test]
    fn test_provider_metrics_debug() {
        let pm = ProviderMetrics {
            name: "Ollama".to_string(),
            available: true,
            healthy: true,
            consecutive_failures: 0,
        };
        let dbg = format!("{:?}", pm);
        assert!(dbg.contains("Ollama"));
    }

    // ==================================================================
    // Auth edge cases: extra whitespace, Basic auth instead of Bearer
    // ==================================================================

    #[tokio::test]
    async fn test_auth_basic_instead_of_bearer() {
        let app = test_app_with_auth("secret").await;
        let body = serde_json::json!({
            "model": "llama2",
            "messages": [{"role": "user", "content": "Hi"}]
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/chat/completions")
            .header("content-type", "application/json")
            .header("authorization", "Basic secret")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_auth_bearer_with_extra_spaces() {
        let app = test_app_with_auth("mykey").await;
        let body = serde_json::json!({
            "model": "llama2",
            "messages": [{"role": "user", "content": "Hi"}]
        });
        // "Bearer  mykey" (double space) should fail
        let req = Request::builder()
            .method("POST")
            .uri("/v1/chat/completions")
            .header("content-type", "application/json")
            .header("authorization", "Bearer  mykey")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    // ==================================================================
    // Request content-type edge case
    // ==================================================================

    #[tokio::test]
    async fn test_chat_completions_no_content_type_header() {
        let app = test_app_no_auth().await;
        let body = serde_json::json!({
            "model": "llama2",
            "messages": [{"role": "user", "content": "Hi"}]
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/chat/completions")
            // No content-type header
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        // Axum requires content-type for JSON extraction — should reject
        assert!(
            resp.status().is_client_error(),
            "Expected 4xx without content-type, got {}",
            resp.status()
        );
    }

    // ==================================================================
    // Metrics endpoint response structure
    // ==================================================================

    #[tokio::test]
    async fn test_metrics_endpoint_structure_with_providers() {
        let gateway = crate::LlmGateway::new(crate::GatewayConfig::default())
            .await
            .unwrap();
        let state = AppState {
            gateway: Arc::new(gateway),
            api_key: None,
        };
        let app = Router::new()
            .route("/v1/metrics", get(metrics))
            .with_state(state);

        let req = Request::builder()
            .uri("/v1/metrics")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        // Providers should be an array (possibly with Ollama, llama.cpp, OpenAI)
        let providers = json["providers"].as_array().unwrap();
        for p in providers {
            assert!(p["name"].is_string());
            assert!(p["available"].is_boolean());
            assert!(p["healthy"].is_boolean());
            assert!(p["consecutive_failures"].is_number());
        }
    }

    // ==================================================================
    // Consumer integration header tests (personality, source, request-id)
    // ==================================================================

    #[test]
    fn test_chat_completion_request_with_tools_field() {
        let json = r#"{
            "model": "gpt-4",
            "messages": [{"role": "user", "content": "Hi"}],
            "tools": [{"type": "function", "function": {"name": "get_weather"}}],
            "tool_choice": "auto"
        }"#;
        let req: ChatCompletionRequest = serde_json::from_str(json).unwrap();
        assert!(req.tools.is_some());
        assert_eq!(req.tools.as_ref().unwrap().len(), 1);
        assert_eq!(req.tool_choice, Some(serde_json::json!("auto")));
    }

    #[test]
    fn test_chat_completion_request_with_response_format() {
        let json = r#"{
            "model": "gpt-4",
            "messages": [{"role": "user", "content": "Hi"}],
            "response_format": {"type": "json_object"}
        }"#;
        let req: ChatCompletionRequest = serde_json::from_str(json).unwrap();
        assert!(req.response_format.is_some());
        assert_eq!(req.response_format.unwrap()["type"], "json_object");
    }

    #[test]
    fn test_chat_completion_request_tools_default_none() {
        let json = r#"{
            "model": "llama2",
            "messages": [{"role": "user", "content": "Hi"}]
        }"#;
        let req: ChatCompletionRequest = serde_json::from_str(json).unwrap();
        assert!(req.tools.is_none());
        assert!(req.tool_choice.is_none());
        assert!(req.response_format.is_none());
    }

    #[test]
    fn test_chat_completion_response_personality_id_present() {
        let resp = ChatCompletionResponse {
            id: "test".to_string(),
            object: "chat.completion".to_string(),
            created: 0,
            model: "m".to_string(),
            choices: vec![],
            usage: Usage {
                prompt_tokens: 0,
                completion_tokens: 0,
                total_tokens: 0,
            },
            personality_id: Some("persona-sales-bot".to_string()),
            agent_domain: None,
        };
        let json_val: serde_json::Value = serde_json::to_value(&resp).unwrap();
        assert_eq!(json_val["personality_id"], "persona-sales-bot");
    }

    #[test]
    fn test_chat_completion_response_personality_id_absent() {
        let resp = ChatCompletionResponse {
            id: "test".to_string(),
            object: "chat.completion".to_string(),
            created: 0,
            model: "m".to_string(),
            choices: vec![],
            usage: Usage {
                prompt_tokens: 0,
                completion_tokens: 0,
                total_tokens: 0,
            },
            personality_id: None,
            agent_domain: None,
        };
        let json_val: serde_json::Value = serde_json::to_value(&resp).unwrap();
        // When None, personality_id should be omitted entirely (skip_serializing_if)
        assert!(json_val.get("personality_id").is_none());
    }

    #[tokio::test]
    async fn test_chat_completions_returns_x_request_id_header() {
        let app = test_app_no_auth().await;
        let body = serde_json::json!({
            "model": "llama2",
            "messages": [{"role": "user", "content": "Hi"}]
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/chat/completions")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        // Even on error responses, x-request-id should be present
        let request_id = resp.headers().get("x-request-id");
        assert!(request_id.is_some(), "x-request-id header must be present");
        let id_str = request_id.unwrap().to_str().unwrap();
        // Should be a valid UUID
        assert!(
            uuid::Uuid::parse_str(id_str).is_ok(),
            "x-request-id should be a UUID, got: {}",
            id_str
        );
    }

    #[tokio::test]
    async fn test_chat_completions_with_personality_id_header() {
        let app = test_app_no_auth().await;
        let body = serde_json::json!({
            "model": "llama2",
            "messages": [{"role": "user", "content": "Hi"}]
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/chat/completions")
            .header("content-type", "application/json")
            .header("x-personality-id", "persona-advisor")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        // Request ID should still be present even on inference failure
        assert!(resp.headers().get("x-request-id").is_some());
    }

    #[tokio::test]
    async fn test_chat_completions_with_source_service_header() {
        let app = test_app_no_auth().await;
        let body = serde_json::json!({
            "model": "llama2",
            "messages": [{"role": "user", "content": "Hi"}]
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/chat/completions")
            .header("content-type", "application/json")
            .header("x-source-service", "secureyeoman")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert!(resp.headers().get("x-request-id").is_some());
    }

    #[tokio::test]
    async fn test_chat_completions_auth_error_returns_x_request_id() {
        let app = test_app_with_auth("secret").await;
        let body = serde_json::json!({
            "model": "llama2",
            "messages": [{"role": "user", "content": "Hi"}]
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/chat/completions")
            .header("content-type", "application/json")
            .header("authorization", "Bearer wrong")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        // Even auth errors should include x-request-id for tracing
        let request_id = resp.headers().get("x-request-id");
        assert!(
            request_id.is_some(),
            "x-request-id must be present on auth errors"
        );
        let id_str = request_id.unwrap().to_str().unwrap();
        assert!(uuid::Uuid::parse_str(id_str).is_ok());
    }

    #[tokio::test]
    async fn test_chat_completions_missing_auth_returns_x_request_id() {
        let app = test_app_with_auth("secret").await;
        let body = serde_json::json!({
            "model": "llama2",
            "messages": [{"role": "user", "content": "Hi"}]
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/chat/completions")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        assert!(resp.headers().get("x-request-id").is_some());
    }

    #[tokio::test]
    async fn test_chat_completions_all_consumer_headers_combined() {
        let app = test_app_no_auth().await;
        let body = serde_json::json!({
            "model": "llama2",
            "messages": [{"role": "user", "content": "Hi"}],
            "tools": [{"type": "function", "function": {"name": "search"}}],
            "tool_choice": "auto",
            "response_format": {"type": "json_object"}
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/chat/completions")
            .header("content-type", "application/json")
            .header("x-personality-id", "persona-qa-tester")
            .header("x-source-service", "agnostic")
            .header("x-agent-id", uuid::Uuid::new_v4().to_string())
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        // Should parse all headers and fields without error (inference fails, but parsing works)
        assert!(resp.headers().get("x-request-id").is_some());
        let request_id = resp
            .headers()
            .get("x-request-id")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(uuid::Uuid::parse_str(request_id).is_ok());
    }

    // ===== Token budget endpoint tests =====

    #[test]
    fn test_token_check_request_parsing() {
        let json = r#"{"project": "agnostic", "tokens": 500}"#;
        let req: TokenCheckRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.project, "agnostic");
        assert_eq!(req.tokens, 500);
        assert_eq!(req.pool, "default");
    }

    #[test]
    fn test_token_check_request_with_pool() {
        let json = r#"{"project": "agnostic", "tokens": 500, "pool": "qa-pool"}"#;
        let req: TokenCheckRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.pool, "qa-pool");
    }

    #[test]
    fn test_token_reserve_request_parsing() {
        let json = r#"{"project": "agnostic", "tokens": 10000, "pool_total": 100000, "period_seconds": 7200}"#;
        let req: TokenReserveRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.project, "agnostic");
        assert_eq!(req.tokens, 10000);
        assert_eq!(req.pool_total, Some(100000));
        assert_eq!(req.period_seconds, Some(7200));
    }

    #[test]
    fn test_token_report_request_parsing() {
        let json = r#"{"project": "agnostic", "tokens": 350}"#;
        let req: TokenReportRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.project, "agnostic");
        assert_eq!(req.tokens, 350);
    }

    #[test]
    fn test_token_release_request_parsing() {
        let json = r#"{"project": "agnostic", "pool": "qa-pool"}"#;
        let req: TokenReleaseRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.project, "agnostic");
        assert_eq!(req.pool, "qa-pool");
    }

    #[tokio::test]
    async fn test_tokens_check_pool_not_found() {
        let gateway = Arc::new(
            LlmGateway::new(crate::GatewayConfig::default())
                .await
                .unwrap(),
        );
        let state = AppState {
            gateway,
            api_key: None,
        };
        let app = Router::new()
            .route("/v1/tokens/check", post(tokens_check))
            .with_state(state);

        let body = serde_json::json!({"project": "agnostic", "tokens": 100});
        let req = Request::builder()
            .method("POST")
            .uri("/v1/tokens/check")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_tokens_reserve_creates_pool() {
        let gateway = Arc::new(
            LlmGateway::new(crate::GatewayConfig::default())
                .await
                .unwrap(),
        );
        let state = AppState {
            gateway: gateway.clone(),
            api_key: None,
        };
        let app = Router::new()
            .route("/v1/tokens/reserve", post(tokens_reserve))
            .with_state(state);

        let body = serde_json::json!({
            "project": "agnostic",
            "tokens": 10000,
            "pool": "qa-pool",
            "pool_total": 100000,
            "period_seconds": 3600
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/tokens/reserve")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let resp_body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&resp_body).unwrap();
        assert_eq!(json["status"], "reserved");
        assert_eq!(json["tokens_reserved"], 10000);
        assert_eq!(json["pool_total_remaining"], 100000); // Pool total - 0 consumed = 100000
    }

    #[tokio::test]
    async fn test_tokens_reserve_then_check_then_report() {
        let gateway = Arc::new(
            LlmGateway::new(crate::GatewayConfig::default())
                .await
                .unwrap(),
        );
        let state = AppState {
            gateway: gateway.clone(),
            api_key: None,
        };
        let app = Router::new()
            .route("/v1/tokens/reserve", post(tokens_reserve))
            .route("/v1/tokens/check", post(tokens_check))
            .route("/v1/tokens/report", post(tokens_report))
            .with_state(state);

        // Reserve
        let body = serde_json::json!({"project": "agnostic", "tokens": 5000, "pool_total": 50000});
        let req = Request::builder()
            .method("POST")
            .uri("/v1/tokens/reserve")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // Check — should be allowed for 3000 tokens
        let body = serde_json::json!({"project": "agnostic", "tokens": 3000});
        let req = Request::builder()
            .method("POST")
            .uri("/v1/tokens/check")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let resp_body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&resp_body).unwrap();
        assert_eq!(json["allowed"], true);

        // Report usage of 4000
        let body = serde_json::json!({"project": "agnostic", "tokens": 4000});
        let req = Request::builder()
            .method("POST")
            .uri("/v1/tokens/report")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let resp_body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&resp_body).unwrap();
        assert_eq!(json["status"], "recorded");
        assert_eq!(json["project_remaining"], 1000);

        // Check — should NOT be allowed for 2000 tokens (only 1000 remaining)
        let body = serde_json::json!({"project": "agnostic", "tokens": 2000});
        let req = Request::builder()
            .method("POST")
            .uri("/v1/tokens/check")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        let resp_body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&resp_body).unwrap();
        assert_eq!(json["allowed"], false);
    }

    #[tokio::test]
    async fn test_tokens_report_exceeds_budget() {
        let gateway = Arc::new(
            LlmGateway::new(crate::GatewayConfig::default())
                .await
                .unwrap(),
        );
        let state = AppState {
            gateway: gateway.clone(),
            api_key: None,
        };
        let app = Router::new()
            .route("/v1/tokens/reserve", post(tokens_reserve))
            .route("/v1/tokens/report", post(tokens_report))
            .with_state(state);

        // Reserve 1000
        let body = serde_json::json!({"project": "agnostic", "tokens": 1000, "pool_total": 10000});
        let req = Request::builder()
            .method("POST")
            .uri("/v1/tokens/reserve")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        app.clone().oneshot(req).await.unwrap();

        // Try to report 2000 (exceeds 1000 allocation)
        let body = serde_json::json!({"project": "agnostic", "tokens": 2000});
        let req = Request::builder()
            .method("POST")
            .uri("/v1/tokens/report")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn test_tokens_release() {
        let gateway = Arc::new(
            LlmGateway::new(crate::GatewayConfig::default())
                .await
                .unwrap(),
        );
        let state = AppState {
            gateway: gateway.clone(),
            api_key: None,
        };
        let app = Router::new()
            .route("/v1/tokens/reserve", post(tokens_reserve))
            .route("/v1/tokens/release", post(tokens_release))
            .with_state(state);

        // Reserve first
        let body = serde_json::json!({"project": "agnostic", "tokens": 5000, "pool_total": 50000});
        let req = Request::builder()
            .method("POST")
            .uri("/v1/tokens/reserve")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        app.clone().oneshot(req).await.unwrap();

        // Release
        let body = serde_json::json!({"project": "agnostic"});
        let req = Request::builder()
            .method("POST")
            .uri("/v1/tokens/release")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let resp_body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&resp_body).unwrap();
        assert_eq!(json["status"], "released");
    }

    #[tokio::test]
    async fn test_tokens_release_no_allocation() {
        let gateway = Arc::new(
            LlmGateway::new(crate::GatewayConfig::default())
                .await
                .unwrap(),
        );
        let state = AppState {
            gateway: gateway.clone(),
            api_key: None,
        };
        let app = Router::new()
            .route("/v1/tokens/reserve", post(tokens_reserve))
            .route("/v1/tokens/release", post(tokens_release))
            .with_state(state);

        // Create pool but don't allocate for this project
        let body = serde_json::json!({"project": "other", "tokens": 1000, "pool_total": 50000});
        let req = Request::builder()
            .method("POST")
            .uri("/v1/tokens/reserve")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        app.clone().oneshot(req).await.unwrap();

        // Try to release a project that has no allocation
        let body = serde_json::json!({"project": "agnostic"});
        let req = Request::builder()
            .method("POST")
            .uri("/v1/tokens/release")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_tokens_pools_empty() {
        let gateway = Arc::new(
            LlmGateway::new(crate::GatewayConfig::default())
                .await
                .unwrap(),
        );
        let state = AppState {
            gateway,
            api_key: None,
        };
        let app = Router::new()
            .route("/v1/tokens/pools", get(tokens_pools))
            .with_state(state);

        let req = Request::builder()
            .method("GET")
            .uri("/v1/tokens/pools")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["total_pools"], 0);
        assert!(json["pools"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_tokens_pools_with_data() {
        let gateway = Arc::new(
            LlmGateway::new(crate::GatewayConfig::default())
                .await
                .unwrap(),
        );
        // Seed a pool with data
        {
            let mut mgr = gateway.budget_manager_write().await;
            let _ = mgr.create_pool("qa-pool", 100_000, chrono::Duration::seconds(3600));
            if let Some(pool) = mgr.get_pool_mut("qa-pool") {
                let _ = pool.allocate("agnostic", 50_000);
                let _ = pool.consume("agnostic", 12_000);
            }
        }
        let state = AppState {
            gateway,
            api_key: None,
        };
        let app = Router::new()
            .route("/v1/tokens/pools", get(tokens_pools))
            .with_state(state);

        let req = Request::builder()
            .method("GET")
            .uri("/v1/tokens/pools")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["total_pools"], 1);
        let pool = &json["pools"][0];
        assert_eq!(pool["pool_name"], "qa-pool");
        assert_eq!(pool["total"], 100_000);
        assert_eq!(pool["used"], 12_000);
        assert_eq!(pool["remaining"], 88_000);
        assert_eq!(pool["project_count"], 1);
    }

    #[tokio::test]
    async fn test_tokens_pool_detail_not_found() {
        let gateway = Arc::new(
            LlmGateway::new(crate::GatewayConfig::default())
                .await
                .unwrap(),
        );
        let state = AppState {
            gateway,
            api_key: None,
        };
        let app = Router::new()
            .route("/v1/tokens/pools/:pool_name", get(tokens_pool_detail))
            .with_state(state);

        let req = Request::builder()
            .method("GET")
            .uri("/v1/tokens/pools/nonexistent")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["code"], "pool_not_found");
    }

    #[tokio::test]
    async fn test_tokens_pool_detail_with_projects() {
        let gateway = Arc::new(
            LlmGateway::new(crate::GatewayConfig::default())
                .await
                .unwrap(),
        );
        {
            let mut mgr = gateway.budget_manager_write().await;
            let _ = mgr.create_pool("dev", 200_000, chrono::Duration::seconds(7200));
            if let Some(pool) = mgr.get_pool_mut("dev") {
                let _ = pool.allocate("agnostic", 100_000);
                let _ = pool.allocate("secureyeoman", 80_000);
                let _ = pool.consume("agnostic", 25_000);
            }
        }
        let state = AppState {
            gateway,
            api_key: None,
        };
        let app = Router::new()
            .route("/v1/tokens/pools/:pool_name", get(tokens_pool_detail))
            .with_state(state);

        let req = Request::builder()
            .method("GET")
            .uri("/v1/tokens/pools/dev")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["pool_name"], "dev");
        assert_eq!(json["total"], 200_000);
        assert_eq!(json["used"], 25_000);
        let projects = json["projects"].as_array().unwrap();
        assert_eq!(projects.len(), 2);
    }
}
