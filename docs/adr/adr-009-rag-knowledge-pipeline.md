# ADR-009: RAG & Embedded Knowledge Pipeline

**Status:** Accepted

**Date:** 2026-03-06

**Authors:** AGNOS Team

## Context

AGNOS agents can execute tools, learn strategies (UCB1), and maintain conversation context, but they
lack **semantic understanding of their environment**. An agent tasked with "find the config file that
controls logging" must know the exact path or grep blindly. There is no way to ask "what do the docs
say about audit policy?" and get a grounded answer.

This is the single largest capability gap between AGNOS and commercial AI agent platforms. RAG
(Retrieval-Augmented Generation) closes it by giving every agent access to a searchable, semantically
indexed knowledge base — without requiring fine-tuned models or external vector databases.

Phase 6.8 addresses this with four tightly coupled components:

1. **Embedded vector store** — local HNSW index for semantic similarity search
2. **RAG pipeline** — automatic context retrieval injected into LLM calls
3. **System knowledge base** — auto-indexed OS docs, man pages, agent manifests, audit logs
4. **File watcher** — inotify-based change detection driving automatic re-indexing

## Decision

### Embedded Vector Store (`agent-runtime/vector_store.rs`)

- **Algorithm**: HNSW (Hierarchical Navigable Small World) graph, implemented in pure Rust
  (no C dependencies). If performance proves insufficient, `usearch` FFI can be added later.
- **Embedding**: Delegate to LLM Gateway via a new `/v1/embeddings` endpoint. The gateway
  selects the configured embedding model (e.g., `text-embedding-3-small`, local ONNX model).
- **Storage**: Memory-mapped file per collection under `/var/lib/agnos/vector-store/{collection}/`.
  Each collection has an `index.bin` (HNSW graph) and `metadata.json` (document IDs, chunk offsets).
- **API**: `VectorStore::insert(doc_id, chunks, metadata)`, `VectorStore::query(embedding, top_k, filter)`,
  `VectorStore::delete(doc_id)`.
- **Dimensions**: Configurable per collection (default 1536 for OpenAI-compatible, 384 for lightweight).
- **Persistence**: Flush to disk on insert batch completion and periodic timer (every 60s).

### RAG Pipeline (`llm-gateway/rag.rs`)

- **Integration point**: Middleware in the LLM Gateway request pipeline, between rate limiting
  and provider dispatch.
- **Flow**: Incoming `/v1/chat/completions` request with `rag_config` field ->
  embed the last user message -> query vector store -> prepend top-k chunks as system context ->
  forward to provider.
- **Configuration** (per-agent via manifest or per-request):
  - `chunk_size`: 512 tokens (default)
  - `chunk_overlap`: 64 tokens
  - `top_k`: 5 (default)
  - `similarity_threshold`: 0.7 (default)
  - `reranking`: optional cross-encoder reranking step
  - `collections`: list of vector store collections to search
- **Chunking**: Recursive text splitter (paragraph -> sentence -> token boundaries).
  Code files use AST-aware splitting when language is detected.
- **No RAG by default**: Agents must opt in via manifest `rag_collections` field or per-request config.

### System Knowledge Base (`agent-runtime/knowledge_base.rs`)

- **Auto-indexed sources**:
  - `/usr/share/doc/agnos/` — AGNOS documentation
  - Agent manifests (all registered agents)
  - Audit log summaries (daily digests, not raw entries)
  - Man pages (if available)
  - `/etc/agnos/` configuration files
- **Collection name**: `agnos-system` (always available to all agents)
- **Index schedule**: Full reindex on boot, incremental via file watcher thereafter.
- **Access control**: Read-only for agents. Only the knowledge base service can write to `agnos-system`.

### File Watcher (`agnos-sys/file_watcher.rs`)

- **Backend**: `inotify(7)` on Linux (the only target platform).
- **Events**: `IN_MODIFY`, `IN_CREATE`, `IN_DELETE`, `IN_MOVED_TO`.
- **Debounce**: 500ms window to batch rapid changes (e.g., `git checkout`).
- **Integration**: Emits `FileChanged { path, event_type }` events to a tokio broadcast channel.
  The knowledge base subscribes and triggers re-indexing for changed files.
- **Watch limits**: Respects `/proc/sys/fs/inotify/max_user_watches`. Logs warning if approaching limit.

## Consequences

### What becomes easier
- Agents can answer questions grounded in actual system documentation
- New agents get instant access to OS knowledge without manual configuration
- Document-heavy workflows (compliance auditing, incident response) become viable
- LLM hallucination is reduced by providing retrieved context

### What becomes harder
- LLM Gateway request path gains latency (embedding + vector query, ~50-100ms)
- Disk usage increases (vector indices can be 2-10x the source document size)
- Embedding model must be available — if LLM Gateway is down, RAG is unavailable

### Risks
- HNSW index corruption on unclean shutdown — mitigated by periodic flush + write-ahead metadata
- Embedding model changes invalidate existing indices — mitigated by storing model ID in collection metadata; mismatch triggers full reindex
- inotify watch exhaustion on large file trees — mitigated by watch limit monitoring and selective watching

## Alternatives Considered

### External vector database (Qdrant, Milvus, ChromaDB)
Rejected: adds a heavy external dependency and network hop. AGNOS is an OS, not a cloud service.
Embedded storage keeps the system self-contained and avoids operational complexity.

### SQLite FTS5 for text search
Rejected: keyword search is not semantic search. FTS5 would miss conceptual matches
("how to restrict network access" should match "Landlock filesystem sandboxing"). Vector
similarity search is fundamentally better for natural language queries.

### Embedding computation on-device only
Rejected as a hard requirement: not all devices have capable hardware. The LLM Gateway
already abstracts provider selection, so embedding requests route to whatever is available
(local ONNX, remote API). Device-local is preferred but not mandated.

## References

- Phase 6.8 roadmap: `docs/development/roadmap.md` (RAG & Knowledge section)
- LLM Gateway: `userland/llm-gateway/src/`
- Agent memory store (6.7): `userland/agent-runtime/src/memory_store.rs`
- inotify(7) man page
- HNSW paper: Malkov & Yashunin, 2018
