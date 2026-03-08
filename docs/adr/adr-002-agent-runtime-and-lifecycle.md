# ADR-002: Agent Runtime and Lifecycle

**Status:** Accepted
**Date:** 2026-03-07

## Context

Agents in AGNOS have complex lifecycles: they are registered, spawned in sandboxes, communicate via IPC, consume resources, can be migrated across nodes, and must be monitorable. This record covers the runtime capabilities, lifecycle management, and intelligence features.

## Decisions

### Agent Lifecycle

Agents declare their identity and requirements in a manifest (`manifest.toml`). The runtime manages:

1. **Registration** — agent registers capabilities, resource limits, sandbox profile
2. **Spawning** — sandbox created (Landlock + seccomp + namespaces), binary verified by sigil, process launched
3. **Execution** — agent runs, communicates via IPC, calls LLM gateway
4. **Monitoring** — health checks, circuit breakers (closed/open/half-open), resource tracking
5. **Shutdown** — graceful drain of IPC, state checkpoint, resource cleanup

### RAG and Knowledge Pipeline

An embedded HNSW vector store provides semantic search over system documentation, agent manifests, and user knowledge:

- **Ingestion** — `POST /v1/rag/ingest` accepts documents, chunks them, generates embeddings
- **Query** — `POST /v1/rag/query` returns ranked results with similarity scores
- **File watcher** — monitors configured directories, auto-indexes on change
- **Knowledge search** — AI shell intent `KnowledgeSearch` queries the store naturally

### Agent-to-Agent Communication

- **Pub/Sub** — topic-based messaging with subscriber groups
- **RPC** — request/response between agents with timeout and retry
- **IPC transport** — Unix domain sockets, JSON-encoded messages
- **Cross-node** — transparent routing over mTLS TCP for federated deployments

### Marketplace (mela)

A trust-enforced distribution system for third-party agents:

- **Package format** — `.agnos-agent` signed bundles with manifest, binary, and resources
- **Publisher verification** — Ed25519 signing, transparency log
- **Ratings and reviews** — 1-5 stars per version, one rating per reviewer per package
- **Sandbox profiles** — marketplace agents run in standard sandbox by default
- **Local registry** — installed agents tracked with version, hash, trust level

### Agent Intelligence

- **Explainability** — decision records with confidence scores, factor breakdowns, natural language explanations
- **Safety mechanisms** — declarative policies (Block/Warn/AuditOnly), prompt injection detection, safety circuit breaker
- **Fine-tuning pipeline** — training example collection from feedback, LoRA/QLoRA support, model registry with lineage tracking
- **RL optimization** — UCB1 + Q-learning for tool selection, policy gradients for resource tuning, constrained by safety policies

### Federation, Migration, and Scheduling

- **Federation** — peer-to-peer with Raft coordinator, mTLS authentication, mDNS/DNS-SD discovery
- **Migration** — warm/cold/live agent checkpointing (memory store, vector indices, IPC queues, sandbox config), <500ms target for warm migration
- **Scheduling** — resource-aware placement across nodes (CPU/memory/GPU headroom), locality preferences, preemption with priority queues

### Reasoning Trace Ingestion

Agents can submit structured reasoning traces for observability:

- **Ingest** — `POST /v1/agents/:id/reasoning` accepts `ReasoningTrace` payloads with ordered steps (observation, thought, action, reflection), confidence scores, tool usage, and metadata
- **Query** — `GET /v1/agents/:id/reasoning` lists stored traces with optional confidence filtering
- **Storage** — Per-agent circular buffer (1,000 traces max per agent)
- **Integration** — Designed for AGNOSTIC's `shared/agnos_reasoning.py` module

### Alpha Polish Features

- Persistent agent memory (KV store at `/var/lib/agnos/agent-memory/`)
- Operator dashboard (TUI with agent status, resource usage, recent events)
- Tab completion, shell aliases, pipeline operators in AI shell
- Configuration hot-reload without restart

## Consequences

### Positive
- Agents are full OS citizens with declared capabilities and sandboxed execution
- Semantic search over all system knowledge
- Agents scale across multiple nodes transparently
- Marketplace enables ecosystem growth with trust guarantees

### Negative
- Complexity of federation (Raft consensus, CRIU checkpointing)
- Marketplace trust model requires key management and revocation infrastructure
- RL and fine-tuning features require careful safety constraints
