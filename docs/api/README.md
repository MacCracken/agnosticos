# AGNOS API Reference

> **Last Updated:** 2026-03-08
> **Version:** 2026.3.8

AGNOS exposes two HTTP/JSON services for interacting with the system. Both bind to `127.0.0.1` by default.

| Service | Subsystem Name | Default Port | Description |
|---------|---------------|--------------|-------------|
| Agent Runtime | **daimon** | 8090 | Agent orchestration, memory, RAG, marketplace, RPC, anomaly detection |
| LLM Gateway | **hoosh** | 8088 | OpenAI-compatible inference proxy with caching, accounting, rate limiting |

---

## Authentication and Security

- **Bearer token**: All requests require an `Authorization: Bearer <token>` header.
- **CORS**: Restricted to `localhost` origins only.
- **Bind address**: Configurable via environment variables:
  - `AGNOS_RUNTIME_BIND` (daimon, default `127.0.0.1:8090`)
  - `AGNOS_GATEWAY_BIND` (hoosh, default `127.0.0.1:8088`)

---

## Agent Runtime API (daimon -- port 8090)

### Agents

| Method | Path | Description |
|--------|------|-------------|
| POST | `/v1/agents/register` | Register a new agent with capabilities and sandbox profile |
| GET | `/v1/agents` | List all registered agents |
| GET | `/v1/agents/:id` | Get details for a specific agent |
| POST | `/v1/agents/:id/heartbeat` | Send agent heartbeat to keep registration alive |

### Health and Metrics

| Method | Path | Description |
|--------|------|-------------|
| GET | `/v1/health` | Health check (returns 200 when healthy) |
| GET | `/v1/metrics` | Internal metrics (JSON) |
| GET | `/v1/metrics/prometheus` | Prometheus-format metrics scrape endpoint |

### Memory

| Method | Path | Description |
|--------|------|-------------|
| GET | `/v1/agents/:id/memory` | List all memory keys for an agent |
| GET | `/v1/agents/:id/memory/:key` | Read a specific memory value |
| PUT | `/v1/agents/:id/memory/:key` | Write a memory value |
| DELETE | `/v1/agents/:id/memory/:key` | Delete a memory value |

### RAG (Retrieval-Augmented Generation)

| Method | Path | Description |
|--------|------|-------------|
| POST | `/v1/rag/ingest` | Ingest documents into the vector store |
| POST | `/v1/rag/query` | Query ingested documents with semantic search |
| GET | `/v1/rag/stats` | Vector store statistics (document count, index size) |

### Knowledge Base

| Method | Path | Description |
|--------|------|-------------|
| POST | `/v1/knowledge/search` | Search the knowledge base |
| GET | `/v1/knowledge/stats` | Knowledge base statistics |
| POST | `/v1/knowledge/index` | Trigger re-indexing of knowledge sources |

### RPC (Inter-Agent Remote Procedure Calls)

| Method | Path | Description |
|--------|------|-------------|
| GET | `/v1/rpc/methods` | List all registered RPC methods |
| GET | `/v1/rpc/methods/:agent_id` | List RPC methods exposed by a specific agent |
| POST | `/v1/rpc/register` | Register an RPC method for an agent |
| POST | `/v1/rpc/call` | Invoke an RPC method on a target agent |

### Anomaly Detection

| Method | Path | Description |
|--------|------|-------------|
| POST | `/v1/anomaly/sample` | Submit a telemetry sample for anomaly scoring |
| GET | `/v1/anomaly/alerts` | List all active anomaly alerts |
| GET | `/v1/anomaly/baseline/:agent_id` | Get the behavioral baseline for an agent |
| GET | `/v1/anomaly/alerts/:agent_id` | List anomaly alerts for a specific agent |

### Traces

| Method | Path | Description |
|--------|------|-------------|
| GET | `/v1/traces` | Query distributed traces |
| POST | `/v1/traces/spans` | Submit trace spans |
| GET | `/v1/traces/otlp-config` | Get OTLP collector configuration for external consumers |

### Environment Profiles

| Method | Path | Description |
|--------|------|-------------|
| GET | `/v1/profiles` | List all environment profiles |
| GET | `/v1/profiles/:name` | Get a specific environment profile by name (dev, staging, prod, or custom) |
| PUT | `/v1/profiles/:name` | Create or update a named environment profile |

### Dashboard Sync

| Method | Path | Description |
|--------|------|-------------|
| POST | `/v1/dashboard/sync` | Submit a dashboard sync snapshot (agent status, session, metrics) |
| GET | `/v1/dashboard/latest` | Get the most recent dashboard snapshot |

### Reasoning Traces

| Method | Path | Description |
|--------|------|-------------|
| POST | `/v1/agents/:id/reasoning` | Submit a reasoning trace for an agent |
| GET | `/v1/agents/:id/reasoning` | List reasoning traces for an agent (supports `?min_confidence=` and `?limit=`) |

### Marketplace (mela)

| Method | Path | Description |
|--------|------|-------------|
| GET | `/v1/marketplace/installed` | List locally installed marketplace packages |
| POST | `/v1/marketplace/search` | Search the marketplace catalog |
| POST | `/v1/marketplace/install` | Install a package from the marketplace |
| GET | `/v1/marketplace/:name` | Get details for a specific marketplace package |

### Package Management (ark)

| Method | Path | Description |
|--------|------|-------------|
| GET | `/v1/ark/*` | Ark package management endpoints (install, remove, query, update) |

### Vector Search

| Method | Path | Description |
|--------|------|-------------|
| POST | `/v1/vectors/search` | Search vectors by embedding similarity (cosine) |
| POST | `/v1/vectors/insert` | Insert vectors into a collection (auto-creates if needed) |
| GET | `/v1/vectors/collections` | List all vector collections |
| POST | `/v1/vectors/collections` | Create a new vector collection |
| DELETE | `/v1/vectors/collections/:name` | Delete a vector collection |

### Screen Capture and Recording

| Method | Path | Description |
|--------|------|-------------|
| POST | `/v1/screen/capture` | Take a screenshot (full screen, window, or region). Returns base64-encoded image. |
| POST | `/v1/screen/permissions` | Grant capture permission to an agent (target kinds, expiry, rate limit) |
| GET | `/v1/screen/permissions` | List all capture permissions |
| DELETE | `/v1/screen/permissions/:agent_id` | Revoke an agent's capture permission |
| GET | `/v1/screen/history` | List recent capture history (last 100 entries, metadata only) |
| POST | `/v1/screen/recording/start` | Start a recording session (returns session ID) |
| POST | `/v1/screen/recording/:id/frame` | Capture the next frame in a recording |
| POST | `/v1/screen/recording/:id/pause` | Pause a recording session |
| POST | `/v1/screen/recording/:id/resume` | Resume a paused recording |
| POST | `/v1/screen/recording/:id/stop` | Stop and finalize a recording |
| GET | `/v1/screen/recording/:id` | Get recording session metadata |
| GET | `/v1/screen/recording/:id/frames` | Poll frames since a sequence number (`?since=N`) for streaming |
| GET | `/v1/screen/recording/:id/latest` | Get the most recent frame (live view) |
| GET | `/v1/screen/recordings` | List all recording sessions |

**Capture formats:** `png` (default), `bmp`, `raw_argb`

**Security:** Captures are blocked when secure mode is active. Agent captures require explicit permission grants via `/v1/screen/permissions`. Permissions support time-based expiry and rate limiting.

**Streaming pattern:** Agents poll `/v1/screen/recording/:id/frames?since=N` where `N` is the last sequence number received. Each frame includes a monotonically increasing `sequence` field. For live view, use `/v1/screen/recording/:id/latest`.

### Additional Endpoints

| Method | Path | Description |
|--------|------|-------------|
| GET/POST | `/v1/mcp/tools` | MCP (Model Context Protocol) tool discovery and invocation |
| GET/POST | `/v1/sandbox/profiles` | Sandbox profile management |
| GET/POST | `/v1/webhooks` | Webhook registration and management |
| GET | `/v1/audit` | Query the cryptographic audit log |

---

## LLM Gateway API (hoosh -- port 8088)

hoosh provides an OpenAI-compatible HTTP API so any client library or tool that speaks the OpenAI protocol works out of the box.

### Supported Providers (14)

| Provider | Type | Default Base URL | API Key Env Var |
|----------|------|------------------|-----------------|
| Ollama | Local | `http://localhost:11434` | — (auto-detected) |
| llama.cpp | Local | `http://localhost:8080` | — (auto-detected) |
| OpenAI | Cloud | `https://api.openai.com/v1` | `OPENAI_API_KEY` |
| Anthropic | Cloud | `https://api.anthropic.com/v1` | `ANTHROPIC_API_KEY` |
| Google (Gemini) | Cloud | `https://generativelanguage.googleapis.com/v1beta` | `GOOGLE_API_KEY` |
| DeepSeek | Cloud | `https://api.deepseek.com/v1` | `DEEPSEEK_API_KEY` |
| Mistral AI | Cloud | `https://api.mistral.ai/v1` | `MISTRAL_API_KEY` |
| Grok (x.ai) | Cloud | `https://api.x.ai/v1` | `XAI_API_KEY` |
| Groq | Cloud | `https://api.groq.com/openai/v1` | `GROQ_API_KEY` |
| OpenRouter | Cloud | `https://openrouter.ai/api/v1` | `OPENROUTER_API_KEY` |
| LM Studio | Local | `http://localhost:1234/v1` | — |
| LocalAI | Local | `http://localhost:8080/v1` | — |
| OpenCode | Cloud | `https://api.open-code.dev/v1` | `OPENCODE_API_KEY` |
| Letta | Cloud/Local | `https://app.letta.com/v1` | `LETTA_API_KEY` |

All cloud providers support optional `*_BASE_URL` environment variable overrides. Local providers (LM Studio, LocalAI) are initialized when their `*_BASE_URL` env var is set. Letta supports `LETTA_LOCAL=true` for self-hosted mode at `localhost:8283`.

### Endpoints

| Method | Path | Description |
|--------|------|-------------|
| POST | `/v1/chat/completions` | Chat completion (multi-turn, streaming supported) |
| POST | `/v1/completions` | Single-turn text completion |
| GET | `/v1/models` | List available models across all configured providers |
| GET | `/v1/health` | Gateway health check |

### Token Budget Management

| Method | Path | Description |
|--------|------|-------------|
| POST | `/v1/tokens/check` | Check whether a project has enough token budget remaining |
| POST | `/v1/tokens/reserve` | Allocate tokens for a project in a budget pool (auto-creates pool if needed) |
| POST | `/v1/tokens/report` | Report actual token consumption against a project's budget |
| POST | `/v1/tokens/release` | Release a project's allocation from a budget pool |

### Custom Headers

hoosh accepts the following custom headers for agent-aware routing, accounting, and tracing:

| Header | Description |
|--------|-------------|
| `X-Agent-Id` | Identifies the calling agent (used for per-agent rate limiting) |
| `X-Personality-Id` | Selects a system personality/persona for the request |
| `X-Source-Service` | Identifies the originating service (for audit) |
| `X-Request-Id` | Client-provided request ID for correlation |
| `X-Token-Usage` | Returned in responses; reports prompt and completion token counts |

---

## IPC

Agents communicate locally over Unix domain sockets at `/run/agnos/agents/{agent_id}.sock`. The wire format is length-prefixed JSON over the socket. See the `agent-runtime` crate documentation for message schemas.

---

## Interactive API Explorer

A self-contained interactive API explorer is available at:

```
docs/api/explorer.html
```

Open it in any browser to browse endpoints, view request/response schemas, and try sample requests.

---

## Error Responses

All endpoints return errors in a consistent JSON envelope:

```json
{
  "error": {
    "code": "not_found",
    "message": "Agent with id abc123 not found"
  }
}
```

Standard HTTP status codes are used: 400 (bad request), 401 (unauthorized), 404 (not found), 429 (rate limited), 500 (internal error).
