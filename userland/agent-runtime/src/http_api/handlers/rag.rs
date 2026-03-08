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

pub async fn rag_ingest_handler(
    State(state): State<ApiState>,
    Json(req): Json<RagIngestRequest>,
) -> impl IntoResponse {
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
            other => KnowledgeSource::Custom(other.to_string()),
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
        Some(other) => KnowledgeSource::Custom(other.to_string()),
        None => KnowledgeSource::ConfigFile,
    };
    // Validate and canonicalize the path to prevent traversal attacks
    let index_path = std::path::Path::new(&req.path);
    let canonical = match index_path.canonicalize() {
        Ok(p) => p,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": format!("Invalid path: {}", e)})),
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
