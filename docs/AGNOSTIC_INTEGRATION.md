# Running Agnostic on AGNOS OS

> **Last Updated**: 2026-03-08

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
│  │  │ Local: Ollama, llama.cpp,       │  │                        │
│  │  │        LM Studio, LocalAI       │  │                        │
│  │  │ Cloud: OpenAI, Anthropic,       │  │                        │
│  │  │        Google, DeepSeek,        │  │                        │
│  │  │        Mistral, Grok, Groq,     │  │                        │
│  │  │        OpenRouter, OpenCode,    │  │                        │
│  │  │        Letta                    │  │                        │
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

## Reasoning Trace Ingestion

Agnostic agents can submit step-by-step reasoning traces to daimon for observability and debugging. This integrates with Agnostic's `shared/agnos_reasoning.py` module.

### Submitting a Reasoning Trace

```bash
curl -X POST http://localhost:8090/v1/agents/qa-manager/reasoning \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $AGNOS_RUNTIME_API_KEY" \
  -d '{
    "task": "Analyze authentication module",
    "steps": [
      {"step": 1, "kind": "observation", "content": "Reading auth source", "confidence": 0.9, "duration_ms": 200},
      {"step": 2, "kind": "thought", "content": "Uses constant-time comparison", "confidence": 0.95, "duration_ms": 150},
      {"step": 3, "kind": "action", "content": "Running static analysis", "duration_ms": 500, "tool": "clippy"}
    ],
    "conclusion": "Auth module is well-structured",
    "confidence": 0.92,
    "duration_ms": 850,
    "model": "llama2",
    "tokens_used": 1500,
    "metadata": {"session_id": "sess-123", "crew": "qa-crew"}
  }'
```

### Querying Reasoning Traces

```bash
# List all traces for an agent
curl http://localhost:8090/v1/agents/qa-manager/reasoning

# Filter by minimum confidence
curl "http://localhost:8090/v1/agents/qa-manager/reasoning?min_confidence=0.8&limit=50"
```

---

## Token Budget Management

Agnostic's `config/agnos_token_budget.py` can manage token budgets via hoosh's budget pool endpoints. Budget pools reset on a configurable period (default: 1 hour).

### Reserve a Budget

```bash
curl -X POST http://localhost:8088/v1/tokens/reserve \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $AGNOS_GATEWAY_API_KEY" \
  -d '{
    "project": "agnostic",
    "tokens": 50000,
    "pool": "qa-pool",
    "pool_total": 500000,
    "period_seconds": 3600
  }'
```

### Check Budget Before Inference

```bash
curl -X POST http://localhost:8088/v1/tokens/check \
  -H "Content-Type: application/json" \
  -d '{"project": "agnostic", "tokens": 1000, "pool": "qa-pool"}'
# {"allowed": true, "remaining": 50000, ...}
```

### Report Usage After Inference

```bash
curl -X POST http://localhost:8088/v1/tokens/report \
  -H "Content-Type: application/json" \
  -d '{"project": "agnostic", "tokens": 850, "pool": "qa-pool"}'
```

### Release Allocation

```bash
curl -X POST http://localhost:8088/v1/tokens/release \
  -H "Content-Type: application/json" \
  -d '{"project": "agnostic", "pool": "qa-pool"}'
```

---

## OTLP Collector Configuration

Agnostic's `shared/telemetry.py` exports OpenTelemetry traces via OTLP. Configure it to send traces to the AGNOS collector.

### Discover OTLP Configuration

```bash
curl http://localhost:8090/v1/traces/otlp-config
# {
#   "endpoint": "http://127.0.0.1:4317",
#   "protocol": "grpc",
#   "export_interval_seconds": 5,
#   "sampling_rate": 1.0,
#   "resource_attributes": {"service.name": "agnos-agent-runtime", ...},
#   "enabled": true
# }
```

### Configure Agnostic

In Agnostic's `.env`:

```env
OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317
OTEL_EXPORTER_OTLP_PROTOCOL=grpc
OTEL_TRACES_SAMPLER=parentbased_traceidratio
OTEL_TRACES_SAMPLER_ARG=1.0
OTEL_SERVICE_NAME=agnostic-qa
```

### Environment Variables (AGNOS side)

| Variable | Default | Description |
|----------|---------|-------------|
| `OTEL_EXPORTER_OTLP_ENDPOINT` | `http://127.0.0.1:4317` | OTLP collector endpoint |
| `OTEL_EXPORTER_OTLP_PROTOCOL` | `grpc` | Protocol (`grpc` or `http/protobuf`) |
| `OTEL_BSP_SCHEDULE_DELAY` | `5000` | Batch export interval in milliseconds |
| `OTEL_TRACES_SAMPLER_ARG` | `1.0` | Sampling rate (1.0 = 100%, 0.1 = 10%) |
| `AGNOS_OTLP_ENABLED` | `true` | Enable/disable OTLP export |

---

## Implementation Status

| Feature | Status |
|---------|--------|
| `agnos_gateway` provider in Agnostic config | ✅ Complete |
| AGNOS Gateway OpenAI-compatible HTTP server | ✅ Operational (port 8088) |
| Token accounting per `X-Agent-Id` | ✅ Complete |
| Per-agent rate limiting | ✅ Complete |
| Response caching | ✅ Complete |
| Reasoning trace ingest endpoint | ✅ Complete |
| Token budget endpoints | ✅ Complete |
| Dashboard sync endpoint | ✅ Complete |
| Environment profiles endpoint | ✅ Complete |
| Vector search REST API | ✅ Complete |
| OTLP collector configuration | ✅ Complete |
| Agnostic agent registration with akd | 📋 Future (Phase 6+) |

See [ADR-001](adr/adr-001-foundation-and-architecture.md) for the full implementation plan.
