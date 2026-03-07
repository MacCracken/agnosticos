# ADR-006: Observability and Operations

**Status:** Accepted
**Date:** 2026-03-07

## Context

AGNOS must integrate into existing monitoring infrastructure and provide end-to-end visibility across all components. Internal telemetry, external trace export, CI/CD, and cross-project observability are unified here.

## Decisions

### OpenTelemetry Trace Export

- **Protocol**: OTLP over gRPC and HTTP/protobuf
- **Configuration**: endpoint, protocol, export interval, sampling rate (1.0 dev, 0.1 prod)
- **Exports**: traces (spans), metrics (counters, histograms), structured logs
- **Resource attributes**: `service.name`, `service.version`, `host.name`, `agnos.agent.id`

### Distributed Tracing

W3C Trace Context (`traceparent` header) propagated through:
- AI Shell -> Agent Runtime (HTTP header)
- Agent Runtime -> LLM Gateway (HTTP header)
- Agent Runtime -> Agent process (env var `AGNOS_TRACE_PARENT`)
- IPC messages (optional `trace_id` field)

Span hierarchy:
```
ai-shell:command_execute
  agent-runtime:task_dispatch
    agent-runtime:agent_execute
      llm-gateway:chat_completion
```

External agents (Python, TypeScript) submit OTLP spans that stitch into the same trace tree.

### Prometheus Metrics

`GET /metrics` on both ports (8090, 8088). Manual formatting (no global registry dependency).

**Agent Runtime metrics:**
- `agnos_agents_total{status}`, `agnos_tasks_total{status}`, `agnos_task_duration_seconds`
- `agnos_agent_restarts_total{agent_id}`, `agnos_ipc_messages_total{type}`
- `agnos_circuit_breaker_state{agent_id}`

**LLM Gateway metrics:**
- `agnos_llm_requests_total{provider,model,status}`, `agnos_llm_request_duration_seconds{provider}`
- `agnos_llm_tokens_total{provider,model,direction}`, `agnos_llm_cache_hits_total`
- `agnos_llm_rate_limit_rejections_total{agent_id}`

Cardinality limited to top-50 most active agents.

### Resource Forecasting

Linear regression on trailing 1-hour per-agent CPU/memory samples (from `/proc`). If projected usage crosses the resource limit within 15 minutes, emit `ResourceWarning` to pub/sub + audit. Updated every 30 seconds. Disabled gracefully if `/proc` unavailable.

### Cryptographic Audit Chain

All agent actions recorded in an append-only log at `/var/log/agnos/audit.log`:
- Each entry hashed with SHA-256, chaining to the previous entry
- External agents can submit events via `POST /v1/audit/events` (rate-limited per source)
- Events correlated with traces via `correlation_id`

### Testing Strategy and CI/CD

Multi-layer testing:

| Layer | Tools | Target |
|-------|-------|--------|
| Unit | `cargo test` | 80% coverage |
| Integration | `cargo test --test` | Component interactions |
| Security | `cargo-audit`, `cargo-deny`, semgrep | 100% checks pass |
| Lint | `clippy`, `rustfmt` | 0 warnings |
| Performance | `criterion` benchmarks | Regression detection |
| Fuzzing | `cargo-fuzz` | Security-critical parsers |

CI pipeline: lint -> build (x86_64 + aarch64) -> test -> security audit -> SBOM -> coverage -> package -> sign -> release. GitHub Actions with `dtolnay/rust-toolchain`.

## Consequences

### Positive
- AGNOS integrates into Grafana, Datadog, PagerDuty, and any OTLP-compatible system
- End-to-end request tracing across all components including external agents
- Capacity planning via resource forecasting
- Immutable audit trail with cryptographic integrity

### Negative
- Telemetry in every request path (mitigated by sampling)
- Prometheus metrics must be kept in sync with code changes
- Linear regression is naive for bursty workloads (acceptable for alpha)
- CI pipeline requires cross-compilation infrastructure for aarch64
