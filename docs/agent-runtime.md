# Daimon: AGNOS Agent Runtime

> **Named subsystem:** daimon (Greek: guiding spirit/daemon)
> **Port:** 8090
> **Crate:** `agent-runtime`
> **Last Updated:** 2026-03-11
> **Version:** 2026.3.11

## Overview

Daimon is the multi-agent orchestration runtime at the heart of AGNOS. It manages the full lifecycle of AI agents — registration, scheduling, sandboxing, inter-process communication, and health monitoring — while also hosting an extensive set of subsystems for package management, security, marketplace, AI safety, cloud deployment, and more.

All agents communicate over Unix domain sockets at `/run/agnos/agents/{agent_id}.sock`. External consumers (AGNOSTIC, SecureYeoman, Photis Nadi) interact via the REST API on port 8090 or through the MCP tool interface.

## Core Architecture

```
+---------------------------------------------------------------------+
|                         Daimon (port 8090)                          |
|  +------------+  +-------------+  +------------+  +--------------+  |
|  |  Registry  |  | Orchestrator|  | Supervisor |  |  Scheduler   |  |
|  | (discovery)|  | (task dist) |  | (health)   |  | (placement)  |  |
|  +------------+  +-------------+  +------------+  +--------------+  |
|                                                                     |
|  +--------------------+  +--------------+  +---------------------+  |
|  | Sandbox (v1 + v2)  |  |   Seccomp    |  |  Capability System  |  |
|  | Landlock, MAC, net |  |   Profiles   |  |  per-agent grants   |  |
|  +--------------------+  +--------------+  +---------------------+  |
|                                                                     |
|  +--------------------------------------------------------------+  |
|  |     IPC Layer (Unix Sockets / PubSub / RPC / Message Bus)     |  |
|  +--------------------------------------------------------------+  |
+---------------------------------------------------------------------+
         |               |               |              |
    +--------+     +--------+     +--------+     +--------+
    | Agent1 |     | Agent2 |     | Agent3 |     | AgentN |
    +--------+     +--------+     +--------+     +--------+
```

### Core Modules

| Module | Purpose |
|---|---|
| `orchestrator.rs` | Task distribution, priority queues, workload balancing, dependency resolution |
| `supervisor/` | Health check loop, failure detection, automatic recovery, graceful shutdown (module directory) |
| `registry.rs` | Agent lookup by ID/name/capability, status tracking, discovery |
| `ipc.rs` | Unix domain sockets, RPC registry/router, message routing, pub/sub |
| `sandbox.rs` | Landlock, seccomp-bpf, network namespaces, configurable access levels |
| `sandbox_v2.rs` | Next-gen sandbox with enhanced isolation (79 tests) |
| `seccomp_profiles.rs` | Basic filter (20 syscalls) or custom per-agent BPF programs |
| `capability.rs` | Fine-grained permission grants, delegation, revocation |
| `resource.rs` | GPU detection/allocation (NVIDIA), CPU cores, memory reservation |
| `resource_forecast.rs` | Predictive resource allocation |
| `lifecycle.rs` | Agent state machine management |
| `service_manager.rs` | Systemd-style service orchestration |
| `mtls.rs` | Mutual TLS for agent-to-agent communication |
| `integrity.rs` | Runtime integrity verification |

## Agent Lifecycle

```
register --> start --> running --> stop --> deregistered
                         |
                         +--> paused --> running
                         |
                         +--> failed --> restart (supervisor)
```

1. **Register** -- Agent calls `POST /v1/agents/register` with name, capabilities, resource needs, metadata. Receives a UUID.
2. **Start** -- Runtime applies sandbox, allocates resources, launches the agent process.
3. **Running** -- Agent sends periodic heartbeats (`POST /v1/agents/:id/heartbeat`). Supervisor monitors health.
4. **Stop** -- Graceful shutdown via signal or `DELETE /v1/agents/:id`. Resources freed, socket removed.

## Sandbox Model

Sandbox layers are applied in strict order:

| Order | Layer | Purpose |
|---|---|---|
| 1 | Encrypted storage | LUKS-backed agent data directories |
| 2 | MAC | Mandatory Access Control policies |
| 3 | Landlock | Filesystem path restrictions |
| 4 | seccomp | Syscall allowlist/BPF filtering |
| 5 | Network | Namespace isolation (None/Localhost/Restricted/Full) |
| 6 | Audit | All actions logged to cryptographic hash chain |

## HTTP API (Port 8090)

All endpoints are prefixed with `/v1`. Bearer token authentication and localhost-only CORS.

### Agents

| Method | Endpoint | Description |
|---|---|---|
| POST | `/agents/register` | Register a new agent |
| POST | `/agents/register/batch` | Register multiple agents in one call (max 100) |
| GET | `/agents` | List all registered agents |
| GET | `/agents/:id` | Get agent details |
| DELETE | `/agents/:id` | Deregister an agent |
| POST | `/agents/:id/heartbeat` | Send heartbeat with status/metrics |
| POST | `/agents/heartbeat/batch` | Batch heartbeat for multiple agents (max 100) |

### Health and Metrics

| Method | Endpoint | Description |
|---|---|---|
| GET | `/health` | Runtime health check |
| GET | `/metrics` | JSON metrics |
| GET | `/metrics/prometheus` | Prometheus-format metrics |

### Agent Memory

| Method | Endpoint | Description |
|---|---|---|
| GET | `/agents/:id/memory` | List all memory keys |
| GET | `/agents/:id/memory/:key` | Get value by key |
| PUT | `/agents/:id/memory/:key` | Set value by key |
| DELETE | `/agents/:id/memory/:key` | Delete key |

### RAG Pipeline

| Method | Endpoint | Description |
|---|---|---|
| POST | `/rag/ingest` | Ingest documents into vector store |
| POST | `/rag/query` | Semantic query over ingested docs |
| GET | `/rag/stats` | Pipeline statistics |

### Knowledge Base

| Method | Endpoint | Description |
|---|---|---|
| POST | `/knowledge/search` | Search indexed knowledge |
| GET | `/knowledge/stats` | Index statistics |
| POST | `/knowledge/index` | Index new knowledge sources |

### Agent-to-Agent RPC

| Method | Endpoint | Description |
|---|---|---|
| GET | `/rpc/methods` | List all registered RPC methods |
| GET | `/rpc/methods/:agent_id` | List methods for a specific agent |
| POST | `/rpc/register` | Register an RPC method |
| POST | `/rpc/call` | Call a remote method |

### Anomaly Detection

| Method | Endpoint | Description |
|---|---|---|
| POST | `/anomaly/sample` | Submit a behavior sample |
| GET | `/anomaly/alerts` | List all active anomaly alerts |
| GET | `/anomaly/baseline/:agent_id` | Get behavioral baseline for an agent |
| DELETE | `/anomaly/alerts/:agent_id` | Clear alerts for an agent |

### Distributed Traces

| Method | Endpoint | Description |
|---|---|---|
| POST | `/traces` | Submit trace data |
| GET | `/traces` | List traces |
| GET | `/traces/spans` | List spans |

### Environment Profiles

| Method | Endpoint | Description |
|---|---|---|
| GET | `/profiles` | List all environment profiles |
| GET | `/profiles/:name` | Get env var overrides for a named profile (dev, staging, prod, custom) |
| PUT | `/profiles/:name` | Create or update a named environment profile |

Default profiles (dev, staging, prod) ship with sensible defaults for log levels, sandbox mode, cache TTL, rate limiting, and audit levels.

### Dashboard Sync

| Method | Endpoint | Description |
|---|---|---|
| POST | `/dashboard/sync` | Submit agent status, session info, and metrics from external consumers |
| GET | `/dashboard/latest` | Get the most recent dashboard snapshot |

### Reasoning Traces

| Method | Endpoint | Description |
|---|---|---|
| POST | `/agents/:id/reasoning` | Submit a reasoning trace for an agent |
| GET | `/agents/:id/reasoning` | List reasoning traces for an agent |

Reasoning traces capture the step-by-step thought process of AI agents (observations, thoughts, actions, reflections). Used by AGNOSTIC's `shared/agnos_reasoning.py` to submit `ReasoningTrace` payloads for observability and debugging.

Query parameters for GET: `min_confidence` (filter by minimum confidence score), `limit` (max results, default 100).

### Vector Search

| Method | Endpoint | Description |
|---|---|---|
| POST | `/vectors/search` | Search vectors by embedding similarity (cosine, supports `min_score` and `top_k`) |
| POST | `/vectors/insert` | Insert vectors into a collection (auto-creates if needed) |
| GET | `/vectors/collections` | List all vector collections with counts and dimensions |
| POST | `/vectors/collections` | Create a new named vector collection |
| DELETE | `/vectors/collections/:name` | Delete a vector collection |

### Ark Package Management

| Method | Endpoint | Description |
|---|---|---|
| POST | `/ark/install` | Install a package |
| POST | `/ark/remove` | Remove a package |
| GET | `/ark/search` | Search packages |
| GET | `/ark/info/:package` | Get package info |
| POST | `/ark/update` | Update package index |
| POST | `/ark/upgrade` | Upgrade installed packages |
| GET | `/ark/status` | Package manager status |

### Marketplace (Mela)

| Method | Endpoint | Description |
|---|---|---|
| GET | `/marketplace/installed` | List installed marketplace packages |
| GET | `/marketplace/search` | Search marketplace catalog |
| POST | `/marketplace/install` | Install from marketplace |
| GET | `/marketplace/:name` | Get package details |
| DELETE | `/marketplace/:name` | Uninstall package |

### Sandbox Profiles

| Method | Endpoint | Description |
|---|---|---|
| POST | `/sandbox/profiles` | Translate a sandbox profile |
| GET | `/sandbox/profiles/default` | Get default sandbox profile |
| POST | `/sandbox/profiles/validate` | Validate a sandbox profile |

### Webhooks

| Method | Endpoint | Description |
|---|---|---|
| POST | `/webhooks` | Register a webhook |
| GET | `/webhooks` | List webhooks |
| DELETE | `/webhooks/:id` | Delete a webhook |

### Screen Capture and Recording

| Method | Endpoint | Description |
|---|---|---|
| POST | `/screen/capture` | Take a screenshot (full screen, window, or region) |
| POST | `/screen/permissions` | Grant capture permission to an agent |
| GET | `/screen/permissions` | List all capture permissions |
| DELETE | `/screen/permissions/:agent_id` | Revoke capture permission |
| GET | `/screen/history` | Recent capture history |
| POST | `/screen/recording/start` | Start a recording session |
| POST | `/screen/recording/:id/frame` | Capture next frame in a recording |
| POST | `/screen/recording/:id/pause` | Pause recording |
| POST | `/screen/recording/:id/resume` | Resume recording |
| POST | `/screen/recording/:id/stop` | Stop recording |
| GET | `/screen/recording/:id` | Get session metadata |
| GET | `/screen/recording/:id/frames` | Poll frames for streaming (`?since=N`) |
| GET | `/screen/recording/:id/latest` | Get most recent frame |
| GET | `/screen/recordings` | List all recording sessions |

Screen capture and recording is driven by the desktop environment's `ScreenCaptureManager` and `ScreenRecordingManager`. Supports PNG, BMP, and raw ARGB8888 formats. Secure mode blocks all captures. Agent captures require explicit permission grants with configurable rate limits and expiry. Recording uses a poll-based streaming model where agents fetch new frames via sequence numbers.

### Audit

| Method | Endpoint | Description |
|---|---|---|
| POST | `/audit/forward` | Forward an audit event |
| GET | `/audit` | List audit events |
| GET | `/audit/chain` | Get audit hash chain |
| GET | `/audit/chain/verify` | Verify chain integrity |

### MCP (Model Context Protocol)

| Method | Endpoint | Description |
|---|---|---|
| GET | `/mcp/tools` | List available MCP tools |
| POST | `/mcp/tools/call` | Execute an MCP tool call |

### Handshake and Events (Consumer Integration)

| Method | Endpoint | Description |
|---|---|---|
| GET | `/discover` | Service discovery — capabilities, endpoints, companion services |
| POST | `/events/publish` | Publish an event to a topic (pub/sub) |
| GET | `/events/subscribe` | Subscribe to topics via SSE stream (supports wildcards) |
| GET | `/events/topics` | List active topics with subscriber counts |
| GET | `/sandbox/profiles/list` | List predefined sandbox profiles |

## MCP Server (41 Tools)

Daimon exposes its capabilities via the Model Context Protocol for integration with external AI services.

**AGNOS tools (10):**

| Tool | Description |
|---|---|
| `agnos_health` | Check runtime health |
| `agnos_list_agents` | List all registered agents |
| `agnos_get_agent` | Get agent details by ID |
| `agnos_register_agent` | Register a new agent |
| `agnos_deregister_agent` | Remove an agent |
| `agnos_heartbeat` | Send agent heartbeat |
| `agnos_get_metrics` | Get runtime metrics |
| `agnos_forward_audit` | Forward audit event |
| `agnos_memory_get` | Get agent memory value |
| `agnos_memory_set` | Set agent memory value |

**Photis Nadi bridge tools (6):**

| Tool | Description |
|---|---|
| `photis_list_tasks` | List tasks with filters |
| `photis_create_task` | Create a new task |
| `photis_update_task` | Update an existing task |
| `photis_get_rituals` | Get daily rituals/habits |
| `photis_analytics` | Get productivity analytics |
| `photis_sync` | Trigger Supabase sync |

**Consumer bridge tools (20):**

| Prefix | Count | Description |
|---|---|---|
| `aequi_*` | 5 | Accounting: tax estimate, Schedule C, bank import, balances, receipts |
| `agnostic_*` | 5 | QA platform: run suite, test status, report, list suites, agent status |
| `delta_*` | 5 | Code hosting: repos, pull requests, push, CI status |
| `shruti_*` | 5 | DAW: track list, mix, record, playback, AI assist |

**Edge tools (5):**

| Tool | Description |
|---|---|
| `edge_list` | List edge fleet nodes |
| `edge_deploy` | Deploy task to edge node |
| `edge_update` | OTA update edge node |
| `edge_health` | Check edge node health |
| `edge_decommission` | Decommission edge node |

## Major Subsystems

### Marketplace / Mela (Sanskrit: festive gathering)

Agent and application marketplace with trust verification, transparency scoring, local registry, remote client, Flutter `.agpkg` support, and sandbox profiles for marketplace packages.

- **Modules:** `marketplace/` (trust, transparency, local_registry, remote_client, flutter_agpkg, sandbox_profiles, ratings)
- **Package format:** `.agnos-agent` for marketplace distribution
- **Dependencies:** `ed25519-dalek`, `flate2`, `tar`, `rand`

### Sigil (Latin: seal) -- Trust Verification

Cryptographic trust chain for agent packages. Signature verification, trust scoring, certificate pinning. (46 tests)

### Aegis (Greek: shield) -- Security Daemon

Real-time security monitoring: threat detection, quarantine, vulnerability scanning, security event correlation. (55 tests)

### Takumi (Japanese: master craftsman) -- Build System

Build system for `.ark` packages from source recipes. Handles dependencies, compilation, packaging. (56 tests)

### Argonaut (Greek: heroes of the Argo) -- Init System

System initialization: boot stages, service dependency ordering, health checks, ready checks, restart policies. Includes Edge boot mode. (132 tests)

### Agnova (agnos + Latin nova) -- Installer

OS installer: disk layout, partitioning, bootloader config, package selection, network config, security setup. (91 tests)

### Ark + Nous -- Package Management

- **Ark:** Unified package manager CLI and API. Handles `.ark` signed tarballs with metadata. Install, remove, search, update, upgrade.
- **Nous:** Package resolver daemon. Dependency resolution, version constraints, conflict detection.

### Federation (73 tests)

Multi-node agent federation. Cluster management, node scoring, scheduling strategies across federated AGNOS nodes.

### Migration (54 tests)

Live agent migration between nodes with state transfer and rollback support.

### Scheduler (51 tests)

Advanced scheduling with placement constraints, affinity rules, and resource-aware bin packing.

### Post-Quantum Cryptography / PQC (68 tests)

Post-quantum cryptographic primitives for future-proof agent authentication and signing.

### Explainability (59 tests)

Decision explanation engine: records agent decisions with factors, alternatives, confidence labels, and full audit trails.

### Safety (77 tests)

AI safety subsystem: guardrails, content filtering, action validation, safety policy enforcement.

### Fine-Tuning / Finetune (73 tests)

On-device model fine-tuning pipeline: dataset management, LoRA/QLoRA methods, VRAM estimation, model registry, job tracking.

### Formal Verification (76 tests)

Runtime invariant monitoring, state machine property checking, verification reports.

### Sandbox V2 (79 tests)

Next-generation sandbox with enhanced isolation, tighter seccomp policies, and per-workload profiles.

### RL Optimizer (68 tests)

Reinforcement-learning-based optimizer for agent scheduling and resource allocation decisions.

### Cloud (82 tests)

Cloud deployment manager: multi-region support, billing tracking, sync engine, workspace management.

### Collaboration (87 tests)

Human-AI collaboration: session management, handoff protocols, trust calibration, shared task ownership, feedback collection.

### WASM Runtime

WebAssembly sandbox for running untrusted agent plugins in a memory-safe isolated environment.

### Network Tools (32 tools, 7 wrappers)

Kali-style networking toolkit. Every tool is agent-wrapped with audit logging, approval gates, and risk classification.

**32 tools:** PortScan, PingSweep, DnsLookup, TraceRoute, BandwidthTest, PacketCapture, HttpClient, NetcatConnect, ServiceScan, WebScan, DirBust, MassScan, ArpScan, NetworkDiag, DataRelay, DeepInspect, NetworkGrep, SocketStats, DnsEnum, DirFuzz, VulnScanner, BandwidthMonitor, PassiveFingerprint, NetDiscover, TermShark, BetterCap, DnsX, Fierce, WebAppFuzz, SqlMap, AircrackNg, Kismet

**7 typed wrappers:** PortScanner, DnsInvestigator, NetworkProber, VulnAssessor, TrafficAnalyzer, WebFuzzer, SocketInspector

Risk levels (Low/Medium/High/Critical) determine whether explicit user approval is required before execution.

### Swarm Intelligence

Swarm coordination protocols for multi-agent collaborative problem solving.

### Learning and Anomaly Detection

Behavioral learning per agent. Anomaly detector builds baselines and raises alerts on deviation.

### Multimodal

Vision, audio, and multi-modal input processing for agents.

### RAG Pipeline, Vector Store, Knowledge Base, Memory Store

Full retrieval-augmented generation stack:
- **RAG:** Document ingestion, chunking, embedding, semantic retrieval
- **Vector store:** In-process vector similarity search
- **Knowledge base:** Indexed knowledge sources with search
- **Memory store:** Per-agent key-value persistent memory

### PubSub and Rollback

- **PubSub:** Topic-based message broadcasting between agents
- **Rollback:** State rollback for failed agent operations

## Module Index

50+ modules declared in `lib.rs`:

| Module | Module | Module |
|---|---|---|
| aegis | agent | agnova |
| argonaut | ark | capability |
| cloud | collaboration | delegation |
| edge | explainability | federation |
| file_watcher | finetune | formal_verify |
| grpc | http_api | integrity |
| ipc | knowledge_base | learning |
| lifecycle | marketplace | marketplace_backend |
| mcp_server | memory_store | migration |
| mtls | multimodal | network_tools |
| nous | oidc | orchestrator |
| package_manager | pqc | pubsub |
| rag | registry | resource |
| resource_forecast | rl_optimizer | rollback |
| safety | sandbox | sandbox_v2 |
| scheduler | seccomp_profiles | service_manager |
| service_mesh | sigil | supervisor |
| swarm | takumi | tool_analysis |
| vector_rest | vector_store | wasm_runtime |

## Testing

- **Lib tests:** 3638 passed, 0 failed
- **Coverage:** ~84% (tarpaulin)
- **Compiler warnings:** 0

Selected subsystem test counts:

| Subsystem | Tests |
|---|---|
| Argonaut | 132 |
| Agnova | 91 |
| Collaboration | 87 |
| Cloud | 82 |
| Sandbox V2 | 79 |
| Safety | 77 |
| Formal Verify | 76 |
| Finetune | 73 |
| Federation | 73 |
| PQC | 68 |
| RL Optimizer | 68 |
| Explainability | 59 |
| Takumi | 56 |
| Aegis | 55 |
| Migration | 54 |
| Scheduler | 51 |
| Sigil | 46 |

## Performance

Criterion benchmark suite in `benches/`. Measures:

- Agent registration and deregistration throughput
- Task scheduling latency
- IPC message round-trip time
- Sandbox apply overhead
- Memory store read/write operations

## Configuration

### Systemd Service

```ini
[Service]
Type=notify
ExecStart=/usr/bin/agent-runtime daemon
User=agnos
Group=agnos
MemoryMax=512M
CPUQuota=50%
```

### Agent Configuration (JSON)

```json
{
  "name": "example-agent",
  "agent_type": "Service",
  "resource_limits": {
    "max_memory": 1073741824,
    "max_cpu_time": 3600000,
    "max_file_descriptors": 1024,
    "max_processes": 64
  },
  "sandbox": {
    "filesystem_rules": [
      {"path": "/tmp", "access": "ReadWrite"}
    ],
    "network_access": "LocalhostOnly",
    "isolate_network": true
  },
  "permissions": ["FileRead", "FileWrite", "NetworkAccess"]
}
```

## Security

- **Auth:** Bearer token on all endpoints
- **CORS:** Localhost-only
- **Audit:** Cryptographic hash chain at `/var/log/agnos/audit.log`
- **Temp files:** UUID-based under `/run/agnos/`
- **Network validation:** nftables IP/CIDR validation
- **Cert pinning:** Rust SHA-256 SPKI pinning (stdin pipe, no shell injection)
- **Sensitive ops:** Require explicit user approval

## Consumer Integration

| Project | Integration |
|---|---|
| AGNOSTIC | REST API on port 8090, agent registration, CrewAI QA workflows, 5 MCP tools (`agnostic_*`) |
| SecureYeoman | AGNOS as base Docker image, agent-runtime for orchestration |
| Photis Nadi | MCP bridge (6 tools), marketplace `.agpkg` packages, Flutter desktop |
| Aequi | Accounting platform, 5 MCP tools (`aequi_*`), marketplace recipe |
| Delta | Code hosting platform, 5 MCP tools (`delta_*`), marketplace recipe |
| Shruti | DAW, 5 MCP tools (`shruti_*`), marketplace recipe |
| BullShift | Trading platform, marketplace recipe |
| Synapse | LLM management, marketplace recipe |

## Related Subsystems

- **Hoosh** (LLM Gateway, port 8088) -- inference backend for agents
- **Agnoshi** (AI Shell) -- NL shell invoking agents
- **Aethersafha** (Desktop) -- agent status in compositor UI
- **Shakti** (agnos-sudo) -- privilege escalation for agent ops
- **Agnosys** (agnos-sys) -- kernel interface, Landlock/seccomp bindings
- **Agnostik** (agnos-common) -- shared types, error handling, telemetry
