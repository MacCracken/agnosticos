# AGNOS API Reference

> **Last Updated:** 2026-03-07
> **Version:** 2026.3.7

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

### Endpoints

| Method | Path | Description |
|--------|------|-------------|
| POST | `/v1/chat/completions` | Chat completion (multi-turn, streaming supported) |
| POST | `/v1/completions` | Single-turn text completion |
| GET | `/v1/models` | List available models across all configured providers |
| GET | `/v1/health` | Gateway health check |

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
