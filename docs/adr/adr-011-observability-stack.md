# ADR-011: Observability Stack

**Status:** Accepted

**Date:** 2026-03-06

**Authors:** AGNOS Team

## Context

AGNOS has internal telemetry (`agnos-common/telemetry.rs`), an audit log with cryptographic hash
chain, per-agent output capture, and a TUI dashboard. However, these are all **closed-loop** — the
data stays inside AGNOS. There is no way to:

1. **Export traces to external systems** — operators using Grafana, Jaeger, or Datadog cannot
   see AGNOS agent activity alongside their other services.
2. **Correlate requests across services** — a user command in AI Shell triggers work in Agent
   Runtime and LLM Gateway, but there is no shared trace ID linking them.
3. **Expose Prometheus metrics** — the standard for infrastructure monitoring. Without a
   `/metrics` endpoint, AGNOS cannot be monitored by existing alerting pipelines.
4. **Predict resource exhaustion** — agents run until they OOM. No forecasting exists.

For AGNOS to be taken seriously as infrastructure, it must speak the standard observability
protocols.

## Decision

### OpenTelemetry Trace Export (`agnos-common/telemetry.rs`)

- **Protocol**: OTLP (OpenTelemetry Protocol) over gRPC and HTTP/protobuf.
- **Dependency**: `opentelemetry` + `opentelemetry-otlp` Rust crates (well-maintained, Apache-2.0).
- **Configuration**:
  ```toml
  [telemetry.otlp]
  enabled = true
  endpoint = "http://localhost:4317"  # gRPC collector
  protocol = "grpc"                    # or "http/protobuf"
  export_interval_ms = 5000
  ```
- **What is exported**: Traces (spans), metrics (counters, histograms), logs (structured events).
- **Sampling**: Configurable trace sampling rate (default 1.0 in dev, 0.1 in prod profile).
- **Resource attributes**: `service.name`, `service.version`, `host.name`, `agnos.agent.id`.

### Distributed Tracing (`all crates`)

- **Trace propagation**: W3C Trace Context (`traceparent` header) propagated through:
  - AI Shell -> Agent Runtime HTTP API (request header)
  - Agent Runtime -> LLM Gateway HTTP API (request header)
  - Agent Runtime -> Agent process (environment variable `AGNOS_TRACE_PARENT`)
  - IPC messages (optional `trace_id` field in pub/sub and RPC envelopes)
- **Span hierarchy**:
  ```
  ai-shell:command_execute
    agent-runtime:task_dispatch
      agent-runtime:agent_execute
        llm-gateway:chat_completion
          llm-gateway:provider_call
  ```
- **Context injection**: `TraceContext::inject(&mut headers)` / `TraceContext::extract(&headers)`
  utility functions available to all crates via `agnos-common`.

### Prometheus Metrics Endpoint (`agent-runtime` + `llm-gateway`)

- **Endpoint**: `GET /metrics` on both services (ports 8090 and 8088).
- **Format**: Prometheus exposition format (text/plain).
- **Metrics exported**:

  Agent Runtime:
  - `agnos_agents_total{status}` — gauge of registered agents by status
  - `agnos_tasks_total{status}` — counter of tasks by completion status
  - `agnos_task_duration_seconds` — histogram of task execution time
  - `agnos_agent_restarts_total{agent_id}` — counter of agent restarts
  - `agnos_ipc_messages_total{type}` — counter of IPC messages (pubsub, rpc)
  - `agnos_circuit_breaker_state{agent_id}` — gauge (0=closed, 1=open, 2=half-open)

  LLM Gateway:
  - `agnos_llm_requests_total{provider,model,status}` — counter
  - `agnos_llm_request_duration_seconds{provider}` — histogram
  - `agnos_llm_tokens_total{provider,model,direction}` — counter (input/output)
  - `agnos_llm_cache_hits_total` / `agnos_llm_cache_misses_total` — counters
  - `agnos_llm_rate_limit_rejections_total{agent_id}` — counter

- **Implementation**: Manual metric collection (no `prometheus` crate dependency). Format is
  simple enough to emit directly. Avoids adding a global metric registry.

### Resource Usage Forecasting (`agent-runtime/resource_forecast.rs`)

- **Data source**: Trailing 1-hour window of per-agent CPU and memory samples (from `/proc`).
- **Algorithm**: Linear regression on the trailing window. If projected usage crosses the
  resource limit within the next 15 minutes, emit a `ResourceWarning` event.
- **Granularity**: Per-agent forecasts, updated every 30 seconds.
- **Actions**: Warning event to pub/sub + audit log. No automatic action (operators decide).
  Future: auto-scale agent resource limits if permitted by policy.
- **Fallback**: If `/proc` is unavailable (container without procfs), forecasting is disabled
  with a startup warning.

## Consequences

### What becomes easier
- AGNOS integrates into existing monitoring stacks (Grafana, Datadog, PagerDuty)
- End-to-end request tracing across all AGNOS components
- Capacity planning with resource forecasting
- SLA monitoring via standard Prometheus alerting rules

### What becomes harder
- Telemetry code path is now in every request (mitigated by sampling)
- `opentelemetry` crate adds ~3 new transitive dependencies
- Prometheus metrics must be kept in sync with code changes (no auto-discovery)

### Risks
- OTLP export failure should not block normal operation — mitigated by async export with
  bounded queue (drop oldest on overflow)
- Metric cardinality explosion if agent IDs are used as labels — mitigated by limiting
  per-agent metrics to top-50 most active agents in Prometheus output
- Linear regression is naive for bursty workloads — acceptable for alpha; can add EWMA or
  Holt-Winters in beta

## Alternatives Considered

### `metrics` crate + `metrics-exporter-prometheus`
Rejected: the `metrics` crate uses a global recorder pattern that conflicts with AGNOS's
explicit dependency injection. Manual Prometheus formatting is ~50 lines and avoids the global state.

### StatsD instead of Prometheus
Rejected: Prometheus pull-based model is better suited for infrastructure. StatsD push model
requires a running StatsD daemon. Prometheus is the de facto standard for OS-level metrics.

### No resource forecasting (just alerts on current usage)
Rejected: reactive alerts fire too late for memory pressure. By the time an agent hits 90%
of its memory limit, it may be too late to gracefully shed load. Forecasting provides lead time.

## References

- Phase 6.8 roadmap: `docs/development/roadmap.md` (Observability Stack section)
- Existing telemetry: `userland/agnos-common/src/telemetry.rs`
- OpenTelemetry Rust SDK: https://github.com/open-telemetry/opentelemetry-rust
- W3C Trace Context: https://www.w3.org/TR/trace-context/
- Prometheus exposition format: https://prometheus.io/docs/instrumenting/exposition_formats/
