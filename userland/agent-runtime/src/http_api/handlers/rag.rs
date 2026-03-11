use std::time::Instant;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;

use crate::http_api::state::ApiState;
use crate::http_api::types::*;
use crate::knowledge_base::KnowledgeSource;

// ---------------------------------------------------------------------------
// RAG & Knowledge Base handlers
// ---------------------------------------------------------------------------

/// Maximum allowed size for RAG ingest text (1 MB).
const RAG_INGEST_MAX_TEXT_BYTES: usize = 1_048_576;
/// Maximum allowed size for RAG query string (10 KB).
const RAG_QUERY_MAX_BYTES: usize = 10_240;
/// Maximum RAG ingestions per agent per window.
const RAG_INGEST_RATE_LIMIT: u32 = 100;
/// Rate limit window duration in seconds.
const RAG_INGEST_RATE_WINDOW_SECS: u64 = 60;
/// Maximum length for a knowledge source name.
const MAX_KNOWLEDGE_SOURCE_NAME_LEN: usize = 128;

/// Validate a knowledge source name: alphanumeric, hyphens, underscores only; max 128 chars.
fn validate_knowledge_source_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("Knowledge source name must not be empty".to_string());
    }
    if name.len() > MAX_KNOWLEDGE_SOURCE_NAME_LEN {
        return Err(format!(
            "Knowledge source name too long: {} chars exceeds {} char limit",
            name.len(),
            MAX_KNOWLEDGE_SOURCE_NAME_LEN
        ));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(
            "Knowledge source name may only contain alphanumeric characters, hyphens, and underscores"
                .to_string(),
        );
    }
    Ok(())
}

pub async fn rag_ingest_handler(
    State(state): State<ApiState>,
    Json(req): Json<RagIngestRequest>,
) -> impl IntoResponse {
    if req.text.len() > RAG_INGEST_MAX_TEXT_BYTES {
        return (
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(serde_json::json!({
                "error": format!(
                    "Ingest text too large: {} bytes exceeds {} byte limit",
                    req.text.len(),
                    RAG_INGEST_MAX_TEXT_BYTES
                ),
                "code": 413
            })),
        )
            .into_response();
    }

    // Per-agent rate limiting (H6)
    let agent_key = req.agent_id.as_deref().unwrap_or("anonymous").to_string();
    {
        let mut limits = state.rag_ingest_rate_limits.lock().await;
        let now = Instant::now();
        let entry = limits.entry(agent_key.clone()).or_insert((0, now));
        // Reset window if expired
        if now.duration_since(entry.1).as_secs() >= RAG_INGEST_RATE_WINDOW_SECS {
            entry.0 = 0;
            entry.1 = now;
        }
        if entry.0 >= RAG_INGEST_RATE_LIMIT {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                Json(serde_json::json!({
                    "error": format!(
                        "Rate limit exceeded: max {} ingestions per {} seconds per agent",
                        RAG_INGEST_RATE_LIMIT,
                        RAG_INGEST_RATE_WINDOW_SECS
                    ),
                    "agent_id": agent_key,
                    "code": 429
                })),
            )
                .into_response();
        }
        entry.0 += 1;
    }

    let metadata = serde_json::to_value(&req.metadata).unwrap_or_default();
    let mut pipeline = state.rag_pipeline.write().await;
    match pipeline.ingest_text(&req.text, metadata) {
        Ok(ids) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "ingested",
                "chunks": ids.len()
            })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

pub async fn rag_query_handler(
    State(state): State<ApiState>,
    Json(req): Json<RagQueryRequest>,
) -> impl IntoResponse {
    if req.query.len() > RAG_QUERY_MAX_BYTES {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": format!(
                    "Query too large: {} bytes exceeds {} byte limit",
                    req.query.len(),
                    RAG_QUERY_MAX_BYTES
                ),
                "code": 400
            })),
        )
            .into_response();
    }

    let pipeline = state.rag_pipeline.read().await;
    let context = pipeline.query_text(&req.query);
    Json(serde_json::json!({
        "query": req.query,
        "chunks": context.chunks.iter().map(|c| serde_json::json!({
            "content": c.content,
            "score": c.score,
            "metadata": c.metadata,
        })).collect::<Vec<_>>(),
        "formatted_context": context.formatted_context,
        "token_estimate": context.total_tokens_estimate,
    }))
    .into_response()
}

pub async fn rag_stats_handler(State(state): State<ApiState>) -> impl IntoResponse {
    let pipeline = state.rag_pipeline.read().await;
    Json(serde_json::json!({
        "index_size": pipeline.index.len(),
        "config": {
            "top_k": pipeline.config.top_k,
            "chunk_size": pipeline.config.chunk_size,
            "overlap": pipeline.config.overlap,
            "min_relevance_score": pipeline.config.min_relevance_score,
        }
    }))
}

pub async fn knowledge_search_handler(
    State(state): State<ApiState>,
    Json(req): Json<KnowledgeSearchRequest>,
) -> impl IntoResponse {
    let kb = state.knowledge_base.read().await;
    let results = if let Some(ref src) = req.source {
        let source = match src.as_str() {
            "manpage" => KnowledgeSource::ManPage,
            "manifest" => KnowledgeSource::AgentManifest,
            "audit" => KnowledgeSource::AuditLog,
            "config" => KnowledgeSource::ConfigFile,
            other => {
                // H10: Validate custom source names to prevent injection
                if let Err(e) = validate_knowledge_source_name(other) {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(serde_json::json!({"error": e, "code": 400})),
                    )
                        .into_response();
                }
                KnowledgeSource::Custom(other.to_string())
            }
        };
        kb.search_by_source(&source, req.limit)
            .into_iter()
            .map(|entry| crate::knowledge_base::KnowledgeResult {
                relevance_score: 1.0,
                entry,
            })
            .collect::<Vec<_>>()
    } else {
        kb.search(&req.query, req.limit)
    };
    Json(serde_json::json!({
        "query": req.query,
        "results": results.iter().map(|r| serde_json::json!({
            "id": r.entry.id.to_string(),
            "source": format!("{:?}", r.entry.source),
            "path": r.entry.path,
            "relevance": r.relevance_score,
            "content_preview": r.entry.content.char_indices()
                .take_while(|&(i, _)| i < 200)
                .last()
                .map(|(i, c)| &r.entry.content[..i + c.len_utf8()])
                .unwrap_or(&r.entry.content),
        })).collect::<Vec<_>>(),
        "total": results.len(),
    }))
    .into_response()
}

pub async fn knowledge_stats_handler(State(state): State<ApiState>) -> impl IntoResponse {
    let kb = state.knowledge_base.read().await;
    let stats = kb.stats();
    Json(serde_json::json!({
        "total_entries": stats.total_entries,
        "total_bytes": stats.total_bytes,
        "by_source": stats.entries_by_source,
    }))
}

pub async fn knowledge_index_handler(
    State(state): State<ApiState>,
    Json(req): Json<KnowledgeIndexRequest>,
) -> impl IntoResponse {
    let source = match req.source.as_deref() {
        Some("manpage") => KnowledgeSource::ManPage,
        Some("manifest") => KnowledgeSource::AgentManifest,
        Some("audit") => KnowledgeSource::AuditLog,
        Some("config") => KnowledgeSource::ConfigFile,
        Some(other) => {
            // H10: Validate custom source names to prevent injection
            if let Err(e) = validate_knowledge_source_name(other) {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({"error": e, "code": 400})),
                )
                    .into_response();
            }
            KnowledgeSource::Custom(other.to_string())
        }
        None => KnowledgeSource::ConfigFile,
    };
    // Validate and canonicalize the path to prevent traversal attacks
    let index_path = std::path::Path::new(&req.path);
    let canonical = match index_path.canonicalize() {
        Ok(p) => p,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "Invalid or inaccessible path"})),
            )
                .into_response();
        }
    };
    // Restrict indexing to safe directories
    let allowed_prefixes = ["/var/agnos/", "/usr/share/agnos/", "/etc/agnos/"];
    if !allowed_prefixes
        .iter()
        .any(|prefix| canonical.starts_with(prefix))
    {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": "Path not in allowed indexing roots",
                "allowed": allowed_prefixes,
            })),
        )
            .into_response();
    }

    // index_directory does blocking recursive I/O — run off the async thread
    let kb = state.knowledge_base.clone();
    let req_path = req.path.clone();
    let result = tokio::task::spawn_blocking(move || {
        let mut kb = kb.blocking_write();
        kb.index_directory(&canonical, source)
            .map(|count| (req_path, count))
            .map_err(|e| e.to_string())
    })
    .await;

    match result {
        Ok(Ok((path, count))) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "indexed",
                "path": path,
                "entries_added": count,
            })),
        )
            .into_response(),
        Ok(Err(e)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Index task failed: {}", e)})),
        )
            .into_response(),
    }
}
