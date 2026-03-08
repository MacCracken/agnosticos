use std::collections::HashMap;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::http_api::state::ApiState;

// ---------------------------------------------------------------------------
// Environment profile types
// ---------------------------------------------------------------------------

/// An environment profile containing overrides for a named environment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentProfile {
    /// Profile name (e.g., "dev", "staging", "prod").
    pub name: String,
    /// Environment variable overrides for this profile.
    pub env_vars: HashMap<String, String>,
    /// Optional description of this profile.
    #[serde(default)]
    pub description: Option<String>,
    /// Whether this profile is currently active.
    #[serde(default)]
    pub active: bool,
}

/// Request to create or update a profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpsertProfileRequest {
    /// Environment variable overrides.
    pub env_vars: HashMap<String, String>,
    /// Optional description.
    #[serde(default)]
    pub description: Option<String>,
}

// ---------------------------------------------------------------------------
// Default profiles
// ---------------------------------------------------------------------------

pub fn default_profiles() -> HashMap<String, EnvironmentProfile> {
    let mut profiles = HashMap::new();

    profiles.insert(
        "dev".to_string(),
        EnvironmentProfile {
            name: "dev".to_string(),
            env_vars: HashMap::from([
                ("AGNOS_LOG_LEVEL".to_string(), "debug".to_string()),
                ("AGNOS_TELEMETRY_ENABLED".to_string(), "true".to_string()),
                ("AGNOS_SANDBOX_MODE".to_string(), "permissive".to_string()),
                ("AGNOS_CACHE_TTL".to_string(), "60".to_string()),
                ("AGNOS_RATE_LIMIT_ENABLED".to_string(), "false".to_string()),
            ]),
            description: Some(
                "Development environment with permissive security and verbose logging".to_string(),
            ),
            active: false,
        },
    );

    profiles.insert(
        "staging".to_string(),
        EnvironmentProfile {
            name: "staging".to_string(),
            env_vars: HashMap::from([
                ("AGNOS_LOG_LEVEL".to_string(), "info".to_string()),
                ("AGNOS_TELEMETRY_ENABLED".to_string(), "true".to_string()),
                ("AGNOS_SANDBOX_MODE".to_string(), "standard".to_string()),
                ("AGNOS_CACHE_TTL".to_string(), "300".to_string()),
                ("AGNOS_RATE_LIMIT_ENABLED".to_string(), "true".to_string()),
            ]),
            description: Some(
                "Staging environment with standard security and moderate logging".to_string(),
            ),
            active: false,
        },
    );

    profiles.insert(
        "prod".to_string(),
        EnvironmentProfile {
            name: "prod".to_string(),
            env_vars: HashMap::from([
                ("AGNOS_LOG_LEVEL".to_string(), "warn".to_string()),
                ("AGNOS_TELEMETRY_ENABLED".to_string(), "true".to_string()),
                ("AGNOS_SANDBOX_MODE".to_string(), "strict".to_string()),
                ("AGNOS_CACHE_TTL".to_string(), "600".to_string()),
                ("AGNOS_RATE_LIMIT_ENABLED".to_string(), "true".to_string()),
                ("AGNOS_AUDIT_LEVEL".to_string(), "full".to_string()),
            ]),
            description: Some("Production environment with strict security, audit logging, and conservative settings".to_string()),
            active: false,
        },
    );

    profiles
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// GET /v1/profiles/:name — get environment profile by name.
pub async fn get_profile_handler(
    State(state): State<ApiState>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let profiles = state.environment_profiles.read().await;
    match profiles.get(&name) {
        Some(profile) => {
            (StatusCode::OK, Json(serde_json::to_value(profile).unwrap())).into_response()
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": format!("Profile '{}' not found", name),
                "code": 404,
                "available_profiles": profiles.keys().collect::<Vec<_>>()
            })),
        )
            .into_response(),
    }
}

/// GET /v1/profiles — list all environment profiles.
pub async fn list_profiles_handler(State(state): State<ApiState>) -> impl IntoResponse {
    let profiles = state.environment_profiles.read().await;
    let mut list: Vec<&EnvironmentProfile> = profiles.values().collect();
    list.sort_by_key(|p| &p.name);

    Json(serde_json::json!({
        "profiles": list,
        "total": list.len()
    }))
}

/// PUT /v1/profiles/:name — create or update a named environment profile.
pub async fn upsert_profile_handler(
    State(state): State<ApiState>,
    Path(name): Path<String>,
    Json(req): Json<UpsertProfileRequest>,
) -> impl IntoResponse {
    if name.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Profile name must not be empty", "code": 400})),
        )
            .into_response();
    }

    if name.len() > 64 {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Profile name must be 64 characters or fewer", "code": 400})),
        )
            .into_response();
    }

    let mut profiles = state.environment_profiles.write().await;
    let is_update = profiles.contains_key(&name);

    let profile = EnvironmentProfile {
        name: name.clone(),
        env_vars: req.env_vars,
        description: req.description,
        active: false,
    };

    info!(
        "Environment profile {}: name={}",
        if is_update { "updated" } else { "created" },
        name
    );

    profiles.insert(name.clone(), profile);

    let status = if is_update {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };

    (
        status,
        Json(serde_json::json!({
            "status": if is_update { "updated" } else { "created" },
            "profile": name
        })),
    )
        .into_response()
}
