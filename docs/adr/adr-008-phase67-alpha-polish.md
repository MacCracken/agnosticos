# ADR-008: Phase 6.7 Alpha Polish — Core Experience Gaps

**Status:** Accepted

**Date:** 2026-03-06

**Authors:** AGNOS Team

## Context

AGNOS reached Phase 5 (99% complete) with comprehensive security, agent orchestration,
networking, and LLM integration. However, a thorough gap analysis before alpha release
identified 14 functional holes that materially affect the core user and operator experience:

1. **The Question intent is a stub** — the most natural user interaction ("what is X?")
   returns a placeholder instead of querying the LLM Gateway.
2. **Agents are stateless** — no persistent memory across restarts. An agent that "learns"
   loses all knowledge when its process stops.
3. **No operator visibility** — no live dashboard, no structured log querying, no agent
   output capture. Operators cannot reason about agent fleet behavior.
4. **No configuration agility** — changing a resource limit requires a full agent restart;
   no fleet-level declarative configuration.
5. **Shell friction** — no tab-completion, no aliases, no pipeline chaining.

These gaps do not block alpha per se (the sole blocker is the third-party audit), but they
significantly reduce the quality of the alpha experience and would be the first items
reported by testers.

## Decision

We will implement Phase 6.7 as a set of 14 items organized in four categories:

### AI Shell & User Interaction
| Item | Description |
|------|-------------|
| Wire Question intent | Route `Intent::Question` through `LlmClient::answer_question()` with graceful fallback |
| Tab-completion | `CompletionEngine` with built-in commands, intent keywords, network tools, and dynamic agent/service names |
| Pipeline support | New `Intent::Pipeline` variant; `cmd1 \| cmd2` and NL "X then Y" chains |
| Aliases | `AliasManager` with per-user persistent aliases via `ShellConfig.aliases` |

### Agent Intelligence & Memory
| Item | Description |
|------|-------------|
| Persistent KV store | `AgentMemoryStore` — per-agent file-backed JSON store under `/var/lib/agnos/agent-memory/`, atomic writes via rename, path-traversal-safe keys |
| Conversation context | `ConversationContext` — sliding window of recent interactions per agent, exportable for LLM injection |
| Reasoning traces | `TraceBuilder` + `ReasoningTrace` — structured chain-of-thought logging: input → steps → tool calls → output |

### Observability & Debugging
| Item | Description |
|------|-------------|
| Activity dashboard | `DashboardState` TUI — htop-style agent fleet view with CPU, memory, task count, errors |
| Structured log viewer | `AuditViewer` with `AuditFilter` — query by agent, action, approval, time range |
| Output capture | `OutputCapture` ring buffer — per-agent stdout/stderr with tail/filter/format |
| Enriched health endpoint | `/v1/health` returns component status (LLM Gateway, disk, memory) + system metrics |

### Configuration & Operations
| Item | Description |
|------|-------------|
| Hot-reload | `LifecycleEvent::ConfigReloaded` + `reload_config()` — diff-based config application without restart |
| Fleet config | `FleetConfig` + `ReconciliationPlan` — TOML-defined desired state with start/stop reconciliation |
| Environment profiles | `EnvironmentProfile` (dev/staging/prod) — named config presets for bind address, log level, security strictness |

### Design Principles Applied

- **No new crate dependencies** — all features use existing deps (serde, tokio, chrono, reqwest, uuid).
  Exception: `hostname` crate for system health (already available in workspace).
- **Filesystem-backed, not SQLite** — the KV store uses individual JSON files per key.
  This avoids adding a C dependency (sqlite3) and provides natural crash-safety via atomic rename.
  If performance becomes an issue at scale, ADR-009 may propose switching to `sled` or SQLite.
- **Graceful degradation** — all new features handle unavailability (LLM Gateway down, /proc not readable,
  agent not found) without panicking. Fallback messages are always provided.
- **Test-first** — every item ships with 10-20+ tests covering happy path, edge cases, and error handling.

## Consequences

### What becomes easier
- Users can ask natural language questions and get LLM-powered answers
- Operators can monitor agent fleet health at a glance
- Agents can maintain state across restarts
- Debugging agent decisions has a structured trace format
- Configuration changes don't require agent downtime
- Fleet management matches the declarative model users expect (docker-compose style)

### What becomes harder
- The ai-shell crate surface area increases (6 new modules)
- The agent-runtime crate surface area increases (1 new module)
- Filesystem-backed KV store may need migration to a DB if agent count exceeds ~1000

### Risks
- `AgentMemoryStore` filesystem access adds I/O in the agent hot path — mitigated by async I/O and atomic writes
- Fleet reconciliation is one-way (desired → actual); no drift detection yet — acceptable for alpha

## Alternatives Considered

### SQLite for agent memory
Rejected: adds a C dependency and build complexity. The file-per-key approach is sufficient
for alpha/beta scale and simpler to reason about for crash safety.

### D-Bus for hot-reload signals
Rejected: SIGHUP is simpler and doesn't require a D-Bus daemon. The API-triggered reload
path provides a clean alternative for remote management.

### Separate dashboard binary
Rejected: integrating the dashboard into ai-shell keeps the tool count minimal and leverages
the existing reqwest client for API access.

## References

- Phase 6.7 roadmap items: `docs/development/roadmap.md` (lines 353-390)
- Gap analysis session: 2026-03-06
- Existing LLM client: `userland/ai-shell/src/llm.rs`
- Existing lifecycle manager: `userland/agent-runtime/src/lifecycle.rs`
- Existing service manager: `userland/agent-runtime/src/service_manager.rs`
