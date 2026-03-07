# Running Agnostic on AGNOS OS

> **Last Updated**: 2026-03-07

This guide explains how to run the [Agnostic QA platform](https://github.com/MacCracken/agnostic) on AGNOS OS so that Agnostic's six AI agents use the AGNOS LLM Gateway for inference, gaining OS-level token accounting, caching, rate limiting, and the unified security audit trail.

---

## Architecture

```
┌─────────────────────────── AGNOS OS ───────────────────────────────┐
│                                                                      │
│  ┌──────────────── Agnostic (Docker Compose) ──────────────────┐   │
│  │  QA Manager ──┐                                              │   │
│  │  Senior QA  ──┤                                              │   │
│  │  Junior QA  ──┼── Redis + RabbitMQ ── WebGUI (:8000)        │   │
│  │  QA Analyst ──┤                                              │   │
│  │  Security   ──┤                                              │   │
│  │  Performance──┘                                              │   │
│  │       │ LLM requests (OpenAI-compatible HTTP)               │   │
│  └───────┼──────────────────────────────────────────────────────┘   │
│          │                                                           │
│          ▼ :8088/v1/chat/completions                                │
│  ┌─────────────────────────────────────────┐                        │
│  │  AGNOS LLM Gateway (llm-gateway daemon) │                        │
│  │  ┌─────────────┬──────────────────────┐ │                        │
│  │  │ Token acct  │  Response cache      │ │                        │
│  │  │ Rate limit  │  Model sharing       │ │                        │
│  │  └──────┬──────┴──────────────────────┘ │                        │
│  │         │                               │                        │
│  │  ┌──────┴────────────────────────────┐  │                        │
│  │  │ Ollama (:11434) │ llama.cpp (:8080)│  │                        │
│  │  │ OpenAI          │ Anthropic        │  │                        │
│  │  └───────────────────────────────────┘  │                        │
│  └─────────────────────────────────────────┘                        │
│                                                                      │
│  AGNOS Security: Landlock + seccomp-bpf applied to all containers  │
└──────────────────────────────────────────────────────────────────────┘
```

---

## Prerequisites

- AGNOS OS running (or agnosticos development build)
- `llm-gateway` daemon started: `llm-gateway daemon` (listens on port 8088)
- Ollama running (managed by agnosticos or standalone)
- Agnostic cloned and `.env` configured

---

## Quick Start

### 1. Start the AGNOS LLM Gateway

```bash
# On the agnosticos host
llm-gateway daemon
# Gateway now listening at http://localhost:8088
```

Verify it is up:

```bash
curl http://localhost:8088/v1/health
# {"status":"ok","providers":["ollama"],"models_loaded":2}
```

### 2. Configure Agnostic to Use the Gateway

In Agnostic's `.env`:

```env
# Enable AGNOS Gateway integration
AGNOS_LLM_GATEWAY_ENABLED=true
AGNOS_LLM_GATEWAY_URL=http://localhost:8088
AGNOS_LLM_GATEWAY_API_KEY=agnos-local   # or your configured key

# Route all agents through the gateway
PRIMARY_MODEL_PROVIDER=agnos_gateway
FALLBACK_MODEL_PROVIDERS=ollama,openai  # fallback if gateway unreachable
```

The `agnos_gateway` provider is pre-configured in `config/models.json` — no further code changes are needed.

### 3. Start Agnostic

```bash
cd agnostic
docker compose up -d
```

Agent LLM calls will now flow through the AGNOS gateway. Token usage per agent is visible in agnosticos stats:

```bash
llm-gateway stats
# Agent usage breakdown:
#   qa-manager:      1,234 tokens
#   senior-qa:       3,456 tokens
#   ...
```

---

## Per-Agent Model Routing

You can keep agent-specific cloud providers while routing local models through the gateway. Edit `config/models.json`:

```json
"agent_specific_models": {
  "qa-manager": {
    "preferred_provider": "agnos_gateway",
    "model": "gpt-4",
    "fallback_providers": ["openai"]
  },
  "junior-qa": {
    "preferred_provider": "agnos_gateway",
    "model": "llama2",
    "fallback_providers": ["ollama"]
  }
}
```

---

## Security

When running on AGNOS OS, all Docker containers (including Agnostic's agents) are automatically subject to:

- **Landlock**: Filesystem access restricted to declared paths
- **seccomp-bpf**: Syscall filtering based on per-agent policy
- **Namespaces**: Network and PID isolation

No Agnostic configuration is required. Policies are managed via the agnosticos security UI or `agnos-cli security`.

---

## Port Reference

| Port | Service | Notes |
|------|---------|-------|
| 8088 | AGNOS LLM Gateway | OpenAI-compatible `/v1` API |
| 11434 | Ollama | Managed by agnosticos |
| 8080 | llama.cpp | Optional, managed by agnosticos |
| 8000 | Agnostic WebGUI | Chainlit + FastAPI |
| 6379 | Redis | Agnostic internal |
| 5672 | RabbitMQ | Agnostic internal |

---

## Troubleshooting

**Gateway not reachable**
```bash
# Check daemon is running
systemctl status agnos-llm-gateway
# Or if running manually
ps aux | grep llm-gateway
```

**Agnostic falls back to OpenAI instead of gateway**
- Verify `AGNOS_LLM_GATEWAY_ENABLED=true` is set
- Check `PRIMARY_MODEL_PROVIDER=agnos_gateway` in `.env`
- Confirm no other service is on port 8088

**Model not found via gateway**
```bash
# List models the gateway knows about
llm-gateway list-models
# Load a model
llm-gateway load llama2
```

---

## Implementation Status

| Feature | Status |
|---------|--------|
| `agnos_gateway` provider in Agnostic config | ✅ Complete |
| AGNOS Gateway OpenAI-compatible HTTP server | ✅ Operational (port 8088) |
| Token accounting per `X-Agent-Id` | ✅ Complete |
| Per-agent rate limiting | ✅ Complete |
| Response caching | ✅ Complete |
| Agnostic agent registration with akd | 📋 Future (Phase 6+) |

See [ADR-101](adr/adr-101-foundation-and-architecture.md) for the full implementation plan.
