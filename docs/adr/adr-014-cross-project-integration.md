# ADR-014: Cross-Project Integration Architecture

**Status:** Accepted

**Date:** 2026-03-06

**Authors:** AGNOS Team

## Context

AGNOS is not a standalone product — it is the foundation for a family of projects:

- **AGNOSTIC** (`/home/macro/Repos/agnostic`) — Python/CrewAI QA platform with 6 specialized agents,
  already integrating via LLM Gateway (port 8088) and Agent Runtime API (port 8090).
- **SecureYeoman** (`/home/macro/Repos/secureyeoman`) — TypeScript/Bun security tooling,
  planned to use AGNOS as a Docker base image.

Phase 6.6 established the connection points (LLM Gateway routing, agent HUD registration,
heartbeat lifecycle). Phase 6.8 deepens the integration from "connected" to "unified":

1. External agents can write to AGNOS's cryptographic audit chain
2. External agents share persistent memory through AGNOS
3. Traces from Python/TypeScript flow into the same observability pipeline
4. Docker base images make AGNOS the runtime for sibling projects
5. External agents participate in AGNOS fleet management and capability discovery

The guiding principle: **AGNOS is the platform, sibling projects are tenants**. Tenants get
the same security, observability, and lifecycle guarantees as native agents.

## Decision

### Unified Audit Log Forwarding (`agent-runtime/http_api.rs`)

- **Endpoint**: `POST /v1/audit/events`
- **Payload**:
  ```json
  {
    "source": "agnostic",
    "agent_id": "qa-security-auditor",
    "action": "test_execution",
    "details": { "test_suite": "owasp_top10", "passed": 42, "failed": 3 },
    "timestamp": "2026-03-06T14:30:00Z",
    "correlation_id": "trace-abc-123"
  }
  ```
- **Processing**: Events are validated, stamped with receive time, and appended to the
  AGNOS cryptographic audit chain (`agnos-common/audit.rs`). The external `source` field
  distinguishes them from native events.
- **Correlation**: If `correlation_id` matches an existing trace ID, the event is linked
  to that trace in the observability pipeline.
- **Authentication**: Bearer token (existing mechanism). External projects must register
  an API key via `POST /v1/auth/tokens`.
- **Rate limit**: Separate rate limit pool for audit ingestion (default 100 events/sec per source).

### External Agent Memory Bridge (`agent-runtime/memory_store.rs`)

- **REST API** (extends existing `AgentMemoryStore`):
  - `PUT /v1/agents/{id}/memory/{key}` — store a value
  - `GET /v1/agents/{id}/memory/{key}` — retrieve a value
  - `DELETE /v1/agents/{id}/memory/{key}` — delete a value
  - `GET /v1/agents/{id}/memory` — list all keys (with optional prefix filter)
- **Storage**: Same file-backed JSON store as native agents. External agents get the same
  `/var/lib/agnos/agent-memory/{agent_id}/` directory.
- **Access control**: An agent can only access its own memory. The `{id}` in the URL must
  match the authenticated agent's ID.
- **Size limits**: 1MB per value, 100MB total per agent (configurable).
- **Use case**: AGNOSTIC's QA agents store learned test patterns, failure signatures, and
  coverage maps across sessions without managing their own persistence layer.

### Shared Observability Pipeline (`agnos-common/telemetry.rs`)

- **OTLP collector endpoint**: The AGNOS OpenTelemetry export (ADR-011) is bidirectional —
  AGNOS exports its own traces AND accepts traces from external agents.
- **Ingestion**: `POST /v1/traces` accepts OTLP-formatted trace data from external agents.
  Python agents use `opentelemetry-sdk`, TypeScript agents use `@opentelemetry/sdk-trace-node`.
- **Trace stitching**: External spans with a `parent_span_id` matching an AGNOS span are
  linked into the same trace tree. This gives end-to-end visibility:
  ```
  agnos:agent-runtime:task_dispatch
    agnostic:qa-agent:test_execution      <-- Python span
      agnos:llm-gateway:chat_completion   <-- Rust span (via gateway)
  ```
- **Metric aggregation**: External agents' Prometheus metrics are proxied through AGNOS's
  `/metrics` endpoint with a `source="agnostic"` label prefix.

### Cross-Project Reasoning Traces (`agent-runtime/tool_analysis.rs`)

- **Endpoint**: `POST /v1/agents/{id}/traces`
- **Payload**: `ReasoningTrace` structure (from Phase 6.7), submitted by external agents.
- **Use case**: AGNOSTIC's QA decision chains (test plan selection -> execution -> analysis ->
  verdict) are recorded as structured traces, visible in the AGNOS dashboard alongside
  native agent reasoning.
- **Storage**: Same trace store as native agents. Indexed by agent ID and timestamp.

### LLM Gateway Token Budget Sharing (`llm-gateway/accounting.rs`)

- **Budget pools**: Named token budget pools that multiple projects draw from:
  ```toml
  [token_budgets]
  shared_pool = { tokens_per_hour = 1_000_000, tokens_per_day = 10_000_000 }

  [token_budgets.allocations]
  agnos_native = { pool = "shared_pool", weight = 0.5 }
  agnostic = { pool = "shared_pool", weight = 0.3 }
  secureyeoman = { pool = "shared_pool", weight = 0.2 }
  ```
- **Enforcement**: Weighted fair queuing. When the pool is under capacity, all projects get
  full throughput. When contended, each project gets at least its weight share.
- **Rebalancing**: Unused allocation from one project is redistributed to others (work-conserving).
- **Reporting**: `GET /v1/budgets` returns current usage per pool and per allocation.

### Docker Base Images (`docker/`)

- **`agnos:base`** — minimal AGNOS userland (agent-runtime, llm-gateway, agnos-sys).
  Based on `debian:bookworm-slim`. Includes Landlock, seccomp, audit chain.
- **`agnos:python3.12`** — `agnos:base` + Python 3.12 + pip. For AGNOSTIC and other
  Python agent projects. Agent-runtime runs as sidecar managing the Python process.
- **`agnos:node20`** — `agnos:base` + Node.js 20 LTS + npm. For SecureYeoman.
- **Entrypoint**: `docker/entrypoint.sh` starts agent-runtime, optionally starts llm-gateway
  (if `AGNOS_LLM_GATEWAY=true`), then execs the user's command.
- **Security**: All images run as non-root. Seccomp profile applied by default. Landlock
  restricts filesystem to declared paths. Audit chain writes to `/var/log/agnos/`.

### Fleet Config for External Agents (`agent-runtime/service_manager.rs`)

- **Extension**: `FleetConfig` now supports `type = "container"` services:
  ```toml
  [[services]]
  name = "qa-security-auditor"
  type = "container"
  image = "agnostic-qa:latest"
  agent_id = "qa-security-auditor"
  sandbox = "standard"
  schedule = "0 */6 * * *"
  ```
- **Reconciliation**: The reconciliation engine manages container lifecycle (pull, start, stop,
  health check) alongside native agent services. Uses Docker/Podman CLI.
- **Constraints**: Container services get the same circuit breaker, rate limiting, and
  observability as native agents.

### Capability Federation (`agent-runtime/registry.rs`)

- **Cross-project capabilities**: External agents registered via the HTTP API can declare
  capabilities just like native agents:
  ```json
  {
    "agent_id": "qa-security-auditor",
    "capabilities": ["security_audit", "owasp_scanning", "load_testing"]
  }
  ```
- **Discovery**: `GET /v1/capabilities/security_audit/agents` returns both native and external
  agents that provide the capability.
- **Routing**: The orchestrator can route a task to an external agent if it has the best
  capability match and availability. The task is dispatched via HTTP webhook to the external
  agent's registered callback URL.

## Consequences

### What becomes easier
- Sibling projects get enterprise-grade audit, secrets, observability, and lifecycle for free
- Single pane of glass: all agent activity (native + external) visible in one dashboard
- Token budgets prevent one project from exhausting shared LLM capacity
- Docker images provide consistent security baseline across all projects

### What becomes harder
- API surface area increases significantly (audit, memory, traces, budgets, fleet)
- Container lifecycle management adds Docker/Podman as a soft dependency
- Token budget fairness algorithm must handle edge cases (empty pools, weight changes)
- Cross-project traces require careful span ID management to avoid collisions

### Risks
- External audit event injection: a compromised external project could flood the audit chain.
  Mitigated by per-source rate limits and separate authentication.
- Memory bridge as a data exfiltration path: Agent A stores sensitive data, Agent B reads it.
  Mitigated by per-agent isolation (agents can only access their own memory namespace).
- Docker image supply chain: base images must be built from verified sources and signed.
  Image provenance is tracked in the audit chain.

## Alternatives Considered

### Separate integration service (API gateway / reverse proxy)
Rejected: adds a new service to deploy and manage. The Agent Runtime already has an HTTP API;
extending it with integration endpoints is simpler and avoids network hops.

### GraphQL instead of REST for cross-project APIs
Rejected: REST is simpler, better understood by sibling project teams, and sufficient for
the integration patterns (CRUD + events). GraphQL adds schema complexity without clear benefit.

### Shared database for cross-project state
Rejected: violates the principle of each project owning its data. The memory bridge provides
a scoped, per-agent API. A shared database would require schema coordination and create
tight coupling between projects.

## References

- Phase 6.8 roadmap: `docs/development/roadmap.md` (Cross-Project Integration section)
- AGNOSTIC integration doc: `docs/AGNOSTIC_INTEGRATION.md`
- ADR-007: Agnostic QA Platform Integration
- Existing HTTP API: `userland/agent-runtime/src/http_api.rs`
- Existing memory store: `userland/agent-runtime/src/memory_store.rs`
- Existing accounting: `userland/llm-gateway/src/accounting.rs`
- Existing fleet config: `userland/agent-runtime/src/service_manager.rs`
