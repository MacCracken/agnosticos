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
use tracing::{error, info};

use crate::{LlmGateway, GatewayConfig};

const HTTP_PORT: u16 = 8088;

#[derive(Clone)]
struct AppState {
    gateway: Arc<LlmGateway>,
    api_key: Option<String>,
}

pub async fn start_http_server(gateway: Arc<LlmGateway>, _config: GatewayConfig) -> Result<(), Box<dyn std::error::Error>> {
    let api_key = std::env::var("AGNOS_GATEWAY_API_KEY").ok();
    
    let state = AppState {
        gateway,
        api_key,
    };
    
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
    #[allow(dead_code)]
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
) -> impl IntoResponse {
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

    info!(
        request_id = %request_id,
        personality_id = personality_id.as_deref().unwrap_or("-"),
        source_service = source_service.as_deref().unwrap_or("-"),
        model = %payload.model,
        "chat_completions request"
    );

    // Check authentication
    if let Some(ref api_key) = state.api_key {
        match headers.get("authorization") {
            Some(auth) => {
                let auth_str = auth.to_str().unwrap_or("");
                let token = auth_str.strip_prefix("Bearer ").unwrap_or("");
                if token.is_empty() || token != api_key {
                    let mut resp_headers = HeaderMap::new();
                    resp_headers.insert("x-request-id", request_id.parse().unwrap());
                    return (
                        StatusCode::UNAUTHORIZED,
                        resp_headers,
                        Json(serde_json::to_value(ErrorResponse {
                            error: ErrorDetail {
                                message: "Invalid API key".to_string(),
                                r#type: "invalid_request_error".to_string(),
                                code: Some("invalid_api_key".to_string()),
                            },
                        }).unwrap()),
                    );
                }
            }
            None => {
                let mut resp_headers = HeaderMap::new();
                resp_headers.insert("x-request-id", request_id.parse().unwrap());
                return (
                    StatusCode::UNAUTHORIZED,
                    resp_headers,
                    Json(serde_json::to_value(ErrorResponse {
                        error: ErrorDetail {
                            message: "Missing authorization header".to_string(),
                            r#type: "invalid_request_error".to_string(),
                            code: Some("missing_authorization".to_string()),
                        },
                    }).unwrap()),
                );
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
        temperature: payload.temperature.unwrap_or(0.7),
        top_p: payload.top_p.unwrap_or(1.0),
        presence_penalty: 0.0,
        frequency_penalty: 0.0,
    };
    request.validate();

    // Run inference
    match state.gateway.infer(request, agent_id).await {
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
            };

            let mut resp_headers = HeaderMap::new();
            resp_headers.insert("x-request-id", request_id.parse().unwrap());
            resp_headers.insert("x-token-usage", total_tokens.to_string().parse().unwrap());

            (
                StatusCode::OK,
                resp_headers,
                Json(serde_json::to_value(completion).unwrap()),
            )
        }
        Err(e) => {
            // Log full error internally but return sanitized message to client
            error!(model = %payload.model, error = %e, "Inference failed");
            let mut resp_headers = HeaderMap::new();
            resp_headers.insert("x-request-id", request_id.parse().unwrap());
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                resp_headers,
                Json(serde_json::to_value(ErrorResponse {
                    error: ErrorDetail {
                        message: "Inference request failed. Check server logs for details.".to_string(),
                        r#type: "internal_error".to_string(),
                        code: None,
                    },
                }).unwrap()),
            )
        }
    }
}

async fn list_models(
    State(state): State<AppState>,
) -> Json<ModelsResponse> {
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

async fn health(
    State(state): State<AppState>,
) -> Json<HealthResponse> {
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

async fn metrics(
    State(state): State<AppState>,
) -> Json<MetricsResponse> {
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
            choices: vec![
                Choice {
                    index: 0,
                    message: ChatMessage {
                        role: "assistant".to_string(),
                        content: "First response".to_string(),
                    },
                    finish_reason: "stop".to_string(),
                },
            ],
            usage: Usage {
                prompt_tokens: 25,
                completion_tokens: 10,
                total_tokens: 35,
            },
            personality_id: None,
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
                Choice { index: 0, message: ChatMessage { role: "assistant".to_string(), content: "A".to_string() }, finish_reason: "stop".to_string() },
                Choice { index: 1, message: ChatMessage { role: "assistant".to_string(), content: "B".to_string() }, finish_reason: "length".to_string() },
            ],
            usage: Usage { prompt_tokens: 1, completion_tokens: 2, total_tokens: 3 },
            personality_id: None,
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
                ProviderStatus { name: "ollama".to_string(), available: false },
                ProviderStatus { name: "llama.cpp".to_string(), available: false },
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
            message: ChatMessage { role: "assistant".to_string(), content: "done".to_string() },
            finish_reason: "content_filter".to_string(),
        };
        let json_val: serde_json::Value = serde_json::to_value(&choice).unwrap();
        assert_eq!(json_val["index"], 42);
        assert_eq!(json_val["finish_reason"], "content_filter");
    }

    #[test]
    fn test_app_state_clone() {
        let gateway = Arc::new(tokio::runtime::Runtime::new().unwrap().block_on(
            crate::LlmGateway::new(crate::GatewayConfig::default())
        ).unwrap());
        let state = AppState {
            gateway,
            api_key: Some("test-key-123".to_string()),
        };
        let cloned = state.clone();
        assert_eq!(cloned.api_key, Some("test-key-123".to_string()));
    }

    #[test]
    fn test_app_state_no_api_key() {
        let gateway = Arc::new(tokio::runtime::Runtime::new().unwrap().block_on(
            crate::LlmGateway::new(crate::GatewayConfig::default())
        ).unwrap());
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
        let gateway = crate::LlmGateway::new(crate::GatewayConfig::default()).await.unwrap();
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
        let gateway = crate::LlmGateway::new(crate::GatewayConfig::default()).await.unwrap();
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

        let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
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

        let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
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

        let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
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

        let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["error"]["message"].as_str().unwrap().contains("Missing"));
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

        let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["error"]["message"].as_str().unwrap().contains("Invalid"));
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
        let messages = [ChatMessage { role: "system".to_string(), content: "Be helpful".to_string() },
            ChatMessage { role: "user".to_string(), content: "Hello".to_string() }];
        let prompt: String = messages
            .iter()
            .map(|m| format!("{}: {}", m.role, m.content))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(prompt, "system: Be helpful\nuser: Hello");
    }

    #[tokio::test]
    async fn test_chat_completions_single_message_prompt() {
        let messages = [ChatMessage { role: "user".to_string(), content: "Test".to_string() }];
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
        let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
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
        let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
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

        let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
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
            usage: Usage { prompt_tokens: 0, completion_tokens: 0, total_tokens: 0 },
            personality_id: None,
        };
        let dbg = format!("{:?}", resp);
        assert!(dbg.contains("id"));
    }

    #[test]
    fn test_choice_debug() {
        let choice = Choice {
            index: 0,
            message: ChatMessage { role: "a".to_string(), content: "b".to_string() },
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
        let gateway = crate::LlmGateway::new(crate::GatewayConfig::default()).await.unwrap();
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

        let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
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
        let gateway = crate::LlmGateway::new(crate::GatewayConfig::default()).await.unwrap();
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
        let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
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
            usage: Usage { prompt_tokens: 0, completion_tokens: 0, total_tokens: 0 },
            personality_id: Some("persona-sales-bot".to_string()),
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
            usage: Usage { prompt_tokens: 0, completion_tokens: 0, total_tokens: 0 },
            personality_id: None,
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
        assert!(uuid::Uuid::parse_str(id_str).is_ok(), "x-request-id should be a UUID, got: {}", id_str);
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
        assert!(request_id.is_some(), "x-request-id must be present on auth errors");
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
        let request_id = resp.headers().get("x-request-id").unwrap().to_str().unwrap();
        assert!(uuid::Uuid::parse_str(request_id).is_ok());
    }
}
