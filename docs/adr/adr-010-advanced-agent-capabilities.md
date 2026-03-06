# ADR-010: Advanced Agent Capabilities & Lifecycle

**Status:** Accepted

**Date:** 2026-03-06

**Authors:** AGNOS Team

## Context

Phase 6.7 gave agents persistent memory, conversation context, and reasoning traces. But agents
still interact through loosely-coupled pub/sub messaging. Several capability gaps remain:

1. **No request-response IPC** — pub/sub is fire-and-forget. Agent A cannot call Agent B's
   capability and await a typed response. This blocks agent composition patterns.
2. **High barrier to agent creation** — developers must manually create manifests, sandbox configs,
   and boilerplate. No scaffolding or templates exist.
3. **No capability discovery** — the orchestrator assigns tasks based on static configuration.
   Agents cannot advertise what they can do, and the system cannot auto-route.
4. **No failure isolation** — a repeatedly crashing agent gets restarted indefinitely. There is
   no circuit breaker to prevent cascade failures in agent chains.
5. **No scheduled execution** — time-based tasks require external cron. An OS should handle this natively.

## Decision

### Agent-to-Agent RPC (`agent-runtime/ipc.rs`)

- **Protocol**: Request-response over existing Unix domain sockets, layered on top of pub/sub.
- **Message format**: `RpcRequest { id: Uuid, method: String, params: serde_json::Value, timeout_ms: u64 }`
  and `RpcResponse { id: Uuid, result: Result<Value, RpcError> }`.
- **Routing**: Caller specifies target agent ID. The IPC layer delivers via the agent's existing socket.
- **Timeouts**: Per-call timeout (default 30s). Caller receives `RpcError::Timeout` on expiry.
- **Concurrency**: Agents handle RPC calls on a dedicated handler task, separate from pub/sub.
  Max concurrent inbound RPCs configurable (default 16).
- **Type safety**: Optional schema validation via JSON Schema in agent manifest `rpc_methods` field.
  Schema violations return `RpcError::InvalidParams`.

### Agent Templates (`agent-runtime/package_manager.rs`)

- **Command**: `agnos new --template <name> <agent-name>`
- **Built-in templates**: `minimal`, `web-scanner`, `file-processor`, `monitor`, `llm-tool`
- **Template contents**: `manifest.toml`, `sandbox.toml`, `src/main.rs` (or `main.py`),
  `tests/`, `README.md`
- **Template storage**: Bundled in the agent-runtime binary as embedded files (`include_str!`).
  Custom templates can be added to `/etc/agnos/agent-templates/`.
- **Variables**: `{{agent_name}}`, `{{agent_id}}`, `{{author}}`, `{{version}}` replaced at generation time.

### Capability Negotiation (`agent-runtime/registry.rs`)

- **Advertisement**: Agents declare capabilities in their manifest:
  ```toml
  [capabilities]
  provides = ["pdf_parsing", "image_ocr", "security_audit"]
  requires = ["llm_access", "filesystem_read"]
  ```
- **Registry index**: `CapabilityIndex` — a `HashMap<String, Vec<AgentId>>` mapping capability
  names to agents that provide them. Updated on agent register/deregister.
- **Discovery API**: `GET /v1/capabilities` lists all available capabilities.
  `GET /v1/capabilities/{name}/agents` returns agents providing that capability.
- **Orchestrator integration**: `find_capable_agents(capability) -> Vec<AgentId>` for task routing.
  When multiple agents provide a capability, selection uses load + learning score.

### Circuit Breaker (`agent-runtime/supervisor.rs`)

- **States**: `Closed` (normal) -> `Open` (failures exceeded threshold) -> `HalfOpen` (cooldown expired, probe).
- **Thresholds**: Configurable per agent (default: 5 consecutive failures opens the circuit).
- **Cooldown**: Default 60s. Exponential backoff on repeated trips (60s, 120s, 240s, max 600s).
- **Half-open probe**: Supervisor starts the agent once. If it succeeds, circuit closes. If it fails,
  circuit re-opens with increased cooldown.
- **Notification**: `CircuitOpened` / `CircuitClosed` events emitted to pub/sub and audit log.
- **Manual override**: `POST /v1/agents/{id}/circuit { "state": "closed" }` to force-reset.

### Scheduled Tasks (`agent-runtime/service_manager.rs`)

- **Syntax**: Standard cron expressions in service definition:
  ```toml
  [service.vuln-scan]
  schedule = "0 2 * * 0"  # Every Sunday at 02:00
  agent = "vulnerability-scanner"
  task = "full-scan"
  ```
- **Implementation**: Lightweight scheduler task that evaluates cron expressions against wall clock.
  Uses `tokio::time::sleep_until` for next-fire calculation. No external cron daemon.
- **Overlap policy**: `skip` (default — skip if previous run still active), `queue`, `kill-and-restart`.
- **Timezone**: UTC by default, configurable via `timezone = "America/New_York"` in service config.
- **Missed runs**: If the system was down when a scheduled run should have fired, it runs once
  on startup (configurable: `run_on_missed = true`).

## Consequences

### What becomes easier
- Agent composition: Agent A calls Agent B's PDF parser, awaits result, continues
- New agent development: 30-second scaffolding instead of manual boilerplate
- Task routing: orchestrator auto-selects the best agent for a capability
- Fleet resilience: failing agents are isolated, not endlessly restarted
- Recurring tasks: native scheduling without external cron

### What becomes harder
- IPC protocol surface area increases (RPC + pub/sub + direct socket)
- Circuit breaker adds state that must be persisted across supervisor restarts
- Template maintenance: built-in templates must be updated when manifest schema changes

### Risks
- RPC deadlock: Agent A calls Agent B which calls Agent A. Mitigated by timeout and max
  concurrent RPC depth (default 4). Deep chains logged as warnings.
- Capability squatting: malicious agent advertises capabilities it doesn't have. Mitigated by
  sandbox enforcement — the agent can only access resources its sandbox permits, regardless
  of advertised capabilities.

## Alternatives Considered

### gRPC for agent RPC
Rejected for Phase 6.8: adds protobuf compilation step and heavy dependencies. JSON-over-UDS
is sufficient for local IPC. gRPC is planned for Phase 7 (external/network agent communication).

### Kubernetes CronJob-style scheduling
Rejected: AGNOS is not Kubernetes. A lightweight in-process cron evaluator is simpler and
doesn't require a separate scheduler service. The service manager already manages agent lifecycle.

### Capability negotiation via DNS-SD / mDNS
Rejected: overkill for single-node agent discovery. A simple in-memory index is faster and
doesn't require network stack involvement. Federation (Phase 7) may revisit for multi-node.

## References

- Phase 6.8 roadmap: `docs/development/roadmap.md` (Advanced Agent Capabilities section)
- Existing IPC: `userland/agent-runtime/src/ipc.rs`
- Existing pub/sub: `userland/agent-runtime/src/pubsub.rs`
- Existing supervisor: `userland/agent-runtime/src/supervisor.rs`
- Existing package manager: `userland/agent-runtime/src/package_manager.rs`
- Circuit breaker pattern: Michael Nygard, "Release It!", 2nd ed.
