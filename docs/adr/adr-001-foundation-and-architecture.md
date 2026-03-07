# ADR-001: Foundation and Architecture

**Status:** Accepted
**Date:** 2026-03-07

## Context

AGNOS is a Linux-based operating system designed for AI agents and human-AI collaboration. Foundational choices — programming language, display protocol, agent orchestration model, and LLM access pattern — shape every subsequent decision. This record captures those choices.

## Decisions

### Rust as Primary Language

All userland components are written in Rust. Kernel modules use C where required. Rust provides memory safety without garbage collection, zero-cost abstractions, and a strong type system — critical for an OS handling untrusted agent code. The async runtime is tokio (full features). Error handling uses `thiserror` for libraries, `anyhow` for applications.

**Alternatives rejected:** C (no memory safety), C++ (weaker guarantees), Go (GC latency), Zig (immature ecosystem).

### Multi-Agent Orchestration (daimon)

A centralized orchestrator daemon (`daimon`, port 8090) manages agent lifecycle:

- **Orchestrator** — schedules tasks, manages resources
- **Registry** — tracks agent capabilities and status
- **IPC bus** — message passing via Unix domain sockets at `/run/agnos/agents/{agent_id}.sock`
- **Supervisor** — monitors health, enforces policies, circuit breakers

Agents are first-class OS citizens with manifests declaring capabilities, resource limits, and sandbox profiles. The orchestrator supports agent-to-agent RPC, pub/sub messaging, templates, and cron scheduling.

**Alternative rejected:** Fully distributed mesh (harder to audit), Kubernetes (too heavy for desktop OS).

### LLM Gateway (hoosh)

A standalone gateway service (`hoosh`, port 8088) provides an OpenAI-compatible HTTP API:

```
POST /v1/chat/completions  — chat inference
GET  /v1/models            — available models
GET  /v1/health            — service status
```

- **Provider abstraction** — Ollama, llama.cpp, OpenAI, Anthropic behind a unified interface
- **Local-first** — prefers local models, falls back to cloud
- **Per-agent accounting** — token tracking via `X-Agent-Id` header
- **Response caching** — TTL-based, reduces redundant inference
- **Rate limiting** — configurable per-agent concurrent limits

Any OpenAI-compatible client (Python, TypeScript, CrewAI) can route through hoosh by pointing `base_url` to `http://localhost:8088/v1`.

### Cross-Project Integration

AGNOS serves as the platform for sibling projects:

- **AGNOSTIC** — Python/CrewAI QA platform, routes inference through hoosh
- **SecureYeoman** — TypeScript security tooling
- **Photis Nadi** — Flutter productivity app

External agents register via the HTTP API, get the same audit trail, memory store, observability, and lifecycle guarantees as native agents. Token budget pools with weighted fair queuing prevent one project from exhausting shared LLM capacity.

## Consequences

### Positive
- Memory safety across the entire userland
- Centralized policy enforcement and audit
- Any OpenAI-compatible app integrates with zero code changes
- Single observability pane for native and external agents

### Negative
- Steeper learning curve for Rust contributors
- Centralized orchestrator is a potential bottleneck at very high agent counts
- Gateway adds ~1-5ms latency vs direct provider calls

## Named Subsystems

| Name | Role | Port |
|------|------|------|
| **hoosh** | LLM gateway | 8088 |
| **daimon** | Agent orchestrator | 8090 |
| **agnosys** | Kernel interface | — |
| **agnostik** | Shared types library | — |
| **shakti** | Privilege escalation | — |
| **agnoshi** | AI shell (agnsh) | — |
| **aethersafha** | Desktop compositor | — |
| **ark** / **nous** | Package manager / resolver | — |
| **takumi** | Package build system | — |
| **mela** | Agent marketplace | — |
| **aegis** | Security daemon | — |
| **sigil** | Trust verification | — |
| **argonaut** | Init system | — |
| **agnova** | OS installer | — |
