//! HTTP server for OpenAI-compatible API
//!
//! Provides REST API endpoints compatible with OpenAI's API format.

use std::sync::Arc;

use axum::{
    extract::State,
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use tower_http::cors::CorsLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tracing::{error, info};

use crate::{LlmGateway, GatewayConfig};

const HTTP_PORT: u16 = 8088;

#[derive(Clone)]
struct AppState {
    gateway: Arc<LlmGateway>,
    api_key: Option<String>,
}

pub async fn start_http_server(gateway: Arc<LlmGateway>, config: GatewayConfig) -> Result<(), Box<dyn std::error::Error>> {
    let api_key = std::env::var("AGNOS_GATEWAY_API_KEY").ok();
    
    let state = AppState {
        gateway,
        api_key,
    };
    
    let cors = CorsLayer::new()
        .allow_origin(tower_http::cors::Any)
        .allow_methods(tower_http::cors::Any)
        .allow_headers(tower_http::cors::Any);
    
    // 1 MB request body limit to prevent DoS via oversized payloads
    let body_limit = RequestBodyLimitLayer::new(1024 * 1024);

    let app = Router::new()
        .route("/v1/chat/completions", post(chat_completions))
        .route("/v1/models", get(list_models))
        .route("/v1/health", get(health))
        .layer(body_limit)
        .layer(cors)
        .with_state(state);
    
    let addr = format!("0.0.0.0:{}", HTTP_PORT);
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
) -> Result<Json<ChatCompletionResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Check authentication
    if let Some(ref api_key) = state.api_key {
        match headers.get("authorization") {
            Some(auth) => {
                let auth_str = auth.to_str().unwrap_or("");
                if !auth_str.starts_with("Bearer ") || auth_str.strip_prefix("Bearer ").unwrap() != api_key {
                    return Err((
                        StatusCode::UNAUTHORIZED,
                        Json(ErrorResponse {
                            error: ErrorDetail {
                                message: "Invalid API key".to_string(),
                                r#type: "invalid_request_error".to_string(),
                                code: Some("invalid_api_key".to_string()),
                            },
                        }),
                    ));
                }
            }
            None => {
                return Err((
                    StatusCode::UNAUTHORIZED,
                    Json(ErrorResponse {
                        error: ErrorDetail {
                            message: "Missing authorization header".to_string(),
                            r#type: "invalid_request_error".to_string(),
                            code: Some("missing_authorization".to_string()),
                        },
                    }),
                ));
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
                .unwrap()
                .as_secs();
            
            Ok(Json(ChatCompletionResponse {
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
                    prompt_tokens: response.usage.prompt_tokens as u32,
                    completion_tokens: response.usage.completion_tokens as u32,
                    total_tokens: response.usage.total_tokens as u32,
                },
            }))
        }
        Err(e) => {
            // Log full error internally but return sanitized message to client
            error!("Inference failed: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: ErrorDetail {
                        message: "Inference request failed. Check server logs for details.".to_string(),
                        r#type: "internal_error".to_string(),
                        code: None,
                    },
                }),
            ))
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
        };
        
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("test-id"));
        assert!(json.contains("llama2"));
        assert!(json.contains("Hello!"));
    }
}
