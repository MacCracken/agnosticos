use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tracing::info;
use uuid::Uuid;

use crate::http_api::state::ApiState;
use crate::vector_store::{VectorEntry, VectorIndex};

// ---------------------------------------------------------------------------
// Vector API types
// ---------------------------------------------------------------------------

/// Request to search a vector collection.
#[derive(Debug, Clone, Deserialize)]
pub struct VectorSearchRequest {
    /// The query embedding vector.
    pub embedding: Vec<f64>,
    /// Number of results to return (default: 10).
    #[serde(default = "default_top_k")]
    pub top_k: usize,
    /// Collection to search (default: "default").
    #[serde(default = "default_collection")]
    pub collection: String,
    /// Minimum similarity score threshold (0.0 to 1.0).
    #[serde(default)]
    pub min_score: Option<f64>,
}

fn default_top_k() -> usize {
    10
}

fn default_collection() -> String {
    "default".to_string()
}

/// A single result from a vector search.
#[derive(Debug, Clone, Serialize)]
pub struct VectorSearchResult {
    pub id: String,
    pub score: f64,
    pub content: String,
    pub metadata: serde_json::Value,
}

/// Request to insert vectors into a collection.
#[derive(Debug, Clone, Deserialize)]
pub struct VectorInsertRequest {
    /// Collection to insert into (default: "default").
    #[serde(default = "default_collection")]
    pub collection: String,
    /// Vectors to insert.
    pub vectors: Vec<VectorInsertItem>,
}

/// A single vector to insert.
#[derive(Debug, Clone, Deserialize)]
pub struct VectorInsertItem {
    /// The embedding vector.
    pub embedding: Vec<f64>,
    /// The textual content.
    pub content: String,
    /// Arbitrary metadata.
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// Response to a collections list request.
#[derive(Debug, Clone, Serialize)]
pub struct CollectionInfo {
    pub name: String,
    pub vector_count: usize,
    pub dimension: Option<usize>,
}

/// Request to create a new collection.
#[derive(Debug, Clone, Deserialize)]
pub struct CreateCollectionRequest {
    pub name: String,
    /// Pre-set dimensionality (optional; inferred from first insert if omitted).
    #[serde(default)]
    pub dimension: Option<usize>,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// POST /v1/vectors/search — search vectors by embedding similarity.
pub async fn vector_search_handler(
    State(state): State<ApiState>,
    Json(req): Json<VectorSearchRequest>,
) -> impl IntoResponse {
    if req.embedding.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Embedding must not be empty", "code": 400})),
        )
            .into_response();
    }

    let collections = state.vector_collections.read().await;
    match collections.get(&req.collection) {
        Some(index) => {
            let results = index.search(&req.embedding, req.top_k);
            let mut items: Vec<VectorSearchResult> = results
                .into_iter()
                .map(|r| VectorSearchResult {
                    id: r.entry.id.to_string(),
                    score: r.score,
                    content: r.entry.content,
                    metadata: r.entry.metadata,
                })
                .collect();

            // Apply min_score filter if specified
            if let Some(min) = req.min_score {
                items.retain(|r| r.score >= min);
            }

            let total = items.len();
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "collection": req.collection,
                    "results": items,
                    "total": total
                })),
            )
                .into_response()
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": format!("Collection '{}' not found", req.collection),
                "code": "collection_not_found"
            })),
        )
            .into_response(),
    }
}

/// POST /v1/vectors/insert — insert vectors into a collection.
pub async fn vector_insert_handler(
    State(state): State<ApiState>,
    Json(req): Json<VectorInsertRequest>,
) -> impl IntoResponse {
    if req.vectors.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Vectors list must not be empty", "code": 400})),
        )
            .into_response();
    }

    let mut collections = state.vector_collections.write().await;
    let index = collections
        .entry(req.collection.clone())
        .or_insert_with(VectorIndex::new);

    let mut inserted_ids = Vec::new();
    for item in &req.vectors {
        let entry = VectorEntry {
            id: Uuid::new_v4(),
            embedding: item.embedding.clone(),
            metadata: item.metadata.clone(),
            content: item.content.clone(),
            created_at: Utc::now(),
        };
        match index.insert(entry) {
            Ok(id) => inserted_ids.push(id.to_string()),
            Err(e) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "error": format!("Insert failed: {}", e),
                        "code": "insert_error",
                        "inserted_before_error": inserted_ids.len()
                    })),
                )
                    .into_response();
            }
        }
    }

    info!(
        "Vectors inserted: collection={} count={}",
        req.collection,
        inserted_ids.len()
    );

    (
        StatusCode::CREATED,
        Json(serde_json::json!({
            "status": "inserted",
            "collection": req.collection,
            "ids": inserted_ids,
            "count": inserted_ids.len()
        })),
    )
        .into_response()
}

/// GET /v1/vectors/collections — list all vector collections.
pub async fn vector_collections_handler(
    State(state): State<ApiState>,
) -> impl IntoResponse {
    let collections = state.vector_collections.read().await;
    let mut infos: Vec<CollectionInfo> = collections
        .iter()
        .map(|(name, index)| CollectionInfo {
            name: name.clone(),
            vector_count: index.len(),
            dimension: index.dimension(),
        })
        .collect();
    infos.sort_by(|a, b| a.name.cmp(&b.name));

    Json(serde_json::json!({
        "collections": infos,
        "total": infos.len()
    }))
}

/// POST /v1/vectors/collections — create a new vector collection.
pub async fn create_collection_handler(
    State(state): State<ApiState>,
    Json(req): Json<CreateCollectionRequest>,
) -> impl IntoResponse {
    if req.name.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Collection name must not be empty", "code": 400})),
        )
            .into_response();
    }

    if req.name.len() > 128 {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Collection name must be 128 characters or fewer", "code": 400})),
        )
            .into_response();
    }

    let mut collections = state.vector_collections.write().await;
    if collections.contains_key(&req.name) {
        return (
            StatusCode::CONFLICT,
            Json(serde_json::json!({"error": format!("Collection '{}' already exists", req.name), "code": "exists"})),
        )
            .into_response();
    }

    let index = match req.dimension {
        Some(dim) => VectorIndex::with_dimension(dim),
        None => VectorIndex::new(),
    };

    info!("Vector collection created: name={}", req.name);
    collections.insert(req.name.clone(), index);

    (
        StatusCode::CREATED,
        Json(serde_json::json!({
            "status": "created",
            "collection": req.name,
            "dimension": req.dimension
        })),
    )
        .into_response()
}

/// DELETE /v1/vectors/collections/:name — delete a vector collection.
pub async fn delete_collection_handler(
    State(state): State<ApiState>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let mut collections = state.vector_collections.write().await;
    if collections.remove(&name).is_some() {
        info!("Vector collection deleted: name={}", name);
        (
            StatusCode::OK,
            Json(serde_json::json!({"status": "deleted", "collection": name})),
        )
            .into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": format!("Collection '{}' not found", name), "code": 404})),
        )
            .into_response()
    }
}
