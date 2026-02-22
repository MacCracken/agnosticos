# ADR-007: Agnostic QA Platform Integration

**Status**: Proposed
**Date**: 2026-02-22

---

## Context

**Agnostic** (`agnostic` repository) is a containerised multi-agent QA platform (CrewAI, Python) that is designed to run on top of AGNOS OS. It currently calls LLM providers directly and uses Redis + RabbitMQ for inter-agent messaging.

To provide the full AGNOS value proposition — OS-managed AI resources, unified audit trail, OS-level sandboxing — the AGNOS LLM Gateway must expose an interface that Agnostic (and any other Python/non-Rust application) can consume without modification to their LLM client code.

The de-facto standard for LLM HTTP APIs is the **OpenAI Chat Completions API** format. All major LLM clients and frameworks (LangChain, CrewAI, LiteLLM, OpenAI SDK) support it natively.

---

## Decision

The AGNOS LLM Gateway will expose an **OpenAI-compatible HTTP REST API** on port **8088**, in addition to its existing Unix-socket-based internal IPC.

### API Surface

```
POST /v1/chat/completions    — OpenAI-compatible chat inference
GET  /v1/models              — list available/loaded models
GET  /v1/health              — gateway health and provider status
```

The `/v1/chat/completions` endpoint:
- Accepts the standard OpenAI request body (`model`, `messages`, `temperature`, `max_tokens`, `stream`, etc.)
- Translates internally to whichever provider has the requested model loaded (Ollama, llama.cpp, OpenAI, Anthropic)
- Returns a standard OpenAI response object
- Optionally accepts `X-Agent-Id` header for per-agent token accounting
- Optionally accepts `Authorization: Bearer <token>` (env `AGNOS_GATEWAY_API_KEY`)

### Port Assignment

| Port | Service |
|------|---------|
| 8088 | AGNOS LLM Gateway HTTP (OpenAI-compatible) |
| 11434 | Ollama (managed provider) |
| 8080 | llama.cpp (managed provider) |

Port 8088 does not conflict with any other default AGNOS or Agnostic service.

### Authentication

When `AGNOS_GATEWAY_API_KEY` is set, the gateway requires `Authorization: Bearer <key>`. Default for local-only deployments is no auth (or a static `agnos-local` key).

---

## Agnostic Integration

Agnostic configures the gateway via `config/models.json`:

```json
"agnos_gateway": {
  "type": "openai",
  "base_url": "http://localhost:8088/v1",
  "api_key": "agnos-local",
  "model": "default",
  "enabled": false
}
```

Set `PRIMARY_MODEL_PROVIDER=agnos_gateway` in Agnostic's `.env` to route all inference through AGNOS OS. Fallback providers (`ollama`, `openai`) remain available if the gateway is unreachable.

### Benefits to Agnostic When Routed Through Gateway

| Feature | Without Gateway | With AGNOS Gateway |
|---------|-----------------|--------------------|
| Token accounting per agent | ❌ None | ✅ Per `X-Agent-Id` |
| Response caching | ❌ None | ✅ TTL cache (1h default) |
| Rate limiting | ❌ None | ✅ Configurable concurrent limit |
| Model sharing | ❌ Each agent loads separately | ✅ Shared loaded model |
| Audit trail | ❌ Not logged by OS | ✅ agnosticos audit log |

---

## Future: Agent Runtime Integration

In a subsequent phase, Agnostic's CrewAI agents can register with the agnosticos agent runtime daemon (akd) via `agnos-sys` SDK. This would surface them in:
- The agnosticos Agent HUD (real-time monitoring)
- The agnosticos security UI (permission manager, kill switch)
- The multi-agent orchestrator resource scheduler

This is not required for Phase 1 (LLM Gateway) and is deferred to Phase 6+ of agnosticos.

---

## Implementation Plan

### Phase 1 — HTTP Gateway (Current Sprint)
- [ ] Add `axum` or `actix-web` HTTP server to `llm-gateway`
- [ ] Implement `POST /v1/chat/completions` translating to `InferenceRequest`
- [ ] Implement `GET /v1/models` from `loaded_models`
- [ ] Implement `GET /v1/health` with provider connectivity checks
- [ ] Add `X-Agent-Id` header parsing for token accounting
- [ ] Optional Bearer token auth
- [ ] Integration test: Agnostic `agnos_gateway` provider → AGNOS gateway → Ollama

### Phase 2 — Agent Runtime Integration (Future)
- [ ] `agnos-sys` Python bindings or gRPC bridge for agent registration
- [ ] Agnostic agent containers registered as agnosticos agents
- [ ] Unified security policy for Agnostic agent containers

---

## Consequences

### Positive
- Any OpenAI-compatible application (not just Agnostic) can route through AGNOS OS LLM Gateway
- No changes needed to Agnostic's Python code — pure configuration
- Preserves all existing fallback paths

### Negative
- `axum`/`actix-web` dependency added to `llm-gateway` (acceptable, well-maintained crates)
- HTTP server adds a small latency overhead vs. direct Ollama calls (~1-5ms)

### Neutral
- The gateway already has the core `infer()` logic; the HTTP layer is an adapter only

---

## Related

- [ADR-021 in agnostic](../../agnostic/docs/adr/021-agnosticos-integration.md): Agnostic-side integration decisions
- [ADR-004: LLM Gateway Service Design](adr-004-llm-gateway.md): Original agnosticos gateway architecture
- `userland/llm-gateway/src/main.rs`: Implementation target
