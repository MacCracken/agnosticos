use axum::extract::State;
use axum::http::{header, StatusCode};
use axum::middleware::Next;
use axum::response::IntoResponse;
use axum::Json;

use super::state::ApiState;

/// Bearer token authentication middleware.
///
/// If the `ApiState` has an `api_key` set, all requests (except `GET /v1/health`)
/// must include `Authorization: Bearer <token>`. When no key is configured the
/// middleware is a pass-through (dev mode).
pub async fn auth_middleware(
    State(state): State<ApiState>,
    req: axum::extract::Request,
    next: Next,
) -> axum::response::Response {
    let api_key = match &state.api_key {
        Some(key) => key,
        None => return next.run(req).await, // dev mode — no auth
    };

    // Allow health endpoint without auth
    if req.uri().path() == "/v1/health" && req.method() == axum::http::Method::GET {
        return next.run(req).await;
    }

    let auth_header = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());

    match auth_header {
        Some(value) if value.starts_with("Bearer ") => {
            let token = &value[7..];
            // Constant-time comparison to prevent timing side-channel attacks
            let token_bytes = token.as_bytes();
            let key_bytes = api_key.as_bytes();
            let matches = token_bytes.len() == key_bytes.len()
                && token_bytes
                    .iter()
                    .zip(key_bytes.iter())
                    .fold(0u8, |acc, (a, b)| acc | (a ^ b))
                    == 0;
            if matches {
                next.run(req).await
            } else {
                (
                    StatusCode::UNAUTHORIZED,
                    Json(serde_json::json!({"error": "Invalid bearer token", "code": 401})),
                )
                    .into_response()
            }
        }
        _ => (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "Missing or malformed Authorization header", "code": 401})),
        )
            .into_response(),
    }
}
