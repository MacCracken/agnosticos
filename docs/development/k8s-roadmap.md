# AGNOS Orchestration Roadmap — Path to k8s-equivalent

> **Status**: Active | **Last Updated**: 2026-03-21
>
> Mapping AGNOS subsystems to Kubernetes equivalents.
> We're not building k8s — we're building something better: an AI-native orchestration
> layer where agents, models, and services are first-class citizens.

---

## The Stack

```
┌─────────────────────────────────────────────────────────────────┐
│                    Tanur (Desktop GUI)                          │
│         Model studio, training dashboard, fleet monitoring      │
└──────────────────────────┬──────────────────────────────────────┘
                           │ socket
┌──────────────────────────▼──────────────────────────────────────┐
│                    Irfan (LLM Server)                           │
│         Model management, training, eval, marketplace           │
│              ┌── murti (model runtime engine) ──┐               │
└──────────────┼──────────────────────────────────┼───────────────┘
               │ socket                           │ embedded
┌──────────────▼──────────────────────────────────▼───────────────┐
│                    Hoosh (LLM Gateway)                          │
│         Inference routing, caching, token budgets, cloud        │
│              └── murti (local inference) ──┘                    │
└─────────────────────────────────────────────────────────────────┘
               │
┌──────────────▼──────────────────────────────────────────────────┐
│                    Daimon (Agent Runtime)                        │
│         Agent lifecycle, scheduling, fleet, federation           │
│              └── majra (queue/pubsub/relay engine) ──┘          │
└─────────────────────────────────────────────────────────────────┘
               │
┌──────────────▼──────────────────────────────────────────────────┐
│                    Sutra (Infrastructure Orchestrator)           │
│         Playbooks, modules, dry-run, fleet deployment            │
└─────────────────────────────────────────────────────────────────┘
               │
┌──────────────▼──────────────────────────────────────────────────┐
│                    Stiva (Container Runtime) — PLANNED           │
│         OCI containers, image management, namespaces             │
└─────────────────────────────────────────────────────────────────┘
```

---

## Component Matrix

### Container Runtime

| k8s Component | AGNOS Equivalent | Status | Maturity | Notes |
|---|---|---|---|---|
| containerd / CRI-O | **stiva** (scaffolded, 0.1.0) | Scaffolded | 15% | OCI runtime, image pull/store, namespace isolation. 17 tests. Builds on kavach + majra |
| Container images | ark `.ark` packages + marketplace `.agnos-agent` | Done | 90% | Not OCI images — native packages. Stiva adds OCI support |
| Pod sandbox | **sandbox_v2** (daimon module) | Done | 85% | Landlock + seccomp + namespace isolation, 79 tests |
| Container networking | **agnosys** netns module | Done | 80% | Network namespace creation, veth pairs |

**Reference code**:
- `userland/agent-runtime/src/sandbox_v2.rs` — novel sandboxing (79 tests)
- `userland/agnos-sys/src/netns.rs` — network namespace management
- Docker/Podman source for OCI runtime spec compliance

### Scheduling

| k8s Component | AGNOS Equivalent | Status | Maturity | Notes |
|---|---|---|---|---|
| kube-scheduler | **daimon scheduler** | Done | 85% | Task scheduling, cron, training jobs, 51 tests |
| Priority classes | **majra** priority queue | Done | 95% | Multi-tier priority with DAG dependency scheduling |
| Job queue | **majra** managed queue | Done | 95% | SQLite persistence, TTL eviction, rate limiting |
| Pod affinity/anti-affinity | **daimon edge** capability routing | Done | 75% | VRAM-aware placement, capability matching |
| Resource quotas | **hoosh** token budgets | Done | 90% | Per-agent, per-pool token accounting |

**Reference code**:
- `/home/macro/Repos/majra/src/queue.rs` — priority queue with DAG deps
- `/home/macro/Repos/majra/src/heartbeat.rs` — TTL-based Online→Suspect→Offline FSM
- `userland/agent-runtime/src/scheduler.rs` — task scheduling + cron (51 tests)
- `userland/agent-runtime/src/resource_forecast.rs` — resource prediction

### Fleet / Node Management

| k8s Component | AGNOS Equivalent | Status | Maturity | Notes |
|---|---|---|---|---|
| kubelet | **argonaut** (init system) | Done | 90% | Service lifecycle, Edge boot mode, 132 tests |
| Node registration | **daimon edge** node registry | Done | 85% | Register, heartbeat, decommission, 37 tests |
| Node health (Online/Suspect/Offline) | **majra** heartbeat FSM | Done | 95% | TTL-based three-state FSM, auto-eviction |
| Fleet stats | **daimon edge** `/v1/edge/stats` | Done | 85% | Total nodes, GPUs, memory aggregation |
| Node updates | **daimon edge** `/v1/edge/nodes/:id/update` | Done | 80% | Rolling update + completion acknowledgment |
| Capability routing | **daimon edge** `/v1/edge/capabilities/route` | Done | 75% | Route requests to nodes by capability |

**Reference code**:
- `userland/agent-runtime/src/edge.rs` — fleet management (37 tests)
- `/home/macro/Repos/majra/src/heartbeat.rs` — health FSM
- `/home/macro/Repos/secureyeoman/packages/core/src/edge/` — mature fleet: `edge-runtime.ts` (fleet orchestration), `edge-store.ts` (node persistence), `edge-fleet-routes.ts` (HTTP API)
- `/home/macro/Repos/secureyeoman/packages/core/src/ha/` — HA health checks

### Networking & Service Discovery

| k8s Component | AGNOS Equivalent | Status | Maturity | Notes |
|---|---|---|---|---|
| kube-proxy / Services | **nein** (scaffolded, 0.1.0) | Scaffolded | 20% | nftables rule builder, NAT, network policies, container bridge. 24 tests |
| Service mesh (Envoy sidecar) | **daimon service_mesh** | Done | 60% | Envoy/Linkerd support, 20 tests |
| Ingress controller | — | Planned | 0% | Needed for external traffic routing |
| CoreDNS / service discovery | **daimon** `/v1/discover` + handshake | Done | 70% | Agent discovery, SSE events, topic pub/sub |
| Network policies | **agnosys** netns + **nein** (scaffolded) | Partial | 50% | Namespace isolation done, nein rule builder + policies scaffolded (24 tests) |
| mTLS | **daimon mTLS** module | Done | 80% | Certificate pinning, mutual TLS between agents |

**Reference code**:
- `userland/agent-runtime/src/service_mesh.rs` — Envoy/Linkerd (20 tests)
- `userland/agent-runtime/src/mtls.rs` — mutual TLS
- `userland/agnos-sys/src/netns.rs` — network namespaces
- `/home/macro/Repos/secureyeoman/packages/core/src/federation/` — mature federation: crypto, manager, routes, storage (8 files, tested)

### Configuration & Secrets

| k8s Component | AGNOS Equivalent | Status | Maturity | Notes |
|---|---|---|---|---|
| ConfigMaps | **sutra** TOML playbooks | Done | 85% | Tera templates, variables/facts, 70 tests |
| Secrets | **agnostik** secrets module | Done | 80% | Encrypted at rest, scoped per-agent |
| etcd (state store) | **daimon** state + SQLite | Done | 75% | In-memory + SQLite persistence via majra |
| RBAC | **aegis** + **oidc** | Done | 80% | Security daemon (55 tests) + SSO provider (22 tests) |
| Admission controllers | **safety** module | Done | 85% | AI safety, injection detection, 77 tests |

**Reference code**:
- `/home/macro/Repos/sutra/` — infrastructure orchestrator (70 tests)
- `/home/macro/Repos/sutra-community/` — community modules (nftables, sysctl, aegis, daimon, edge)
- `userland/agent-runtime/src/aegis.rs` — security daemon (55 tests)
- `userland/agent-runtime/src/oidc.rs` — SSO/OIDC (22 tests)

### Federation / Multi-Cluster

| k8s Component | AGNOS Equivalent | Status | Maturity | Notes |
|---|---|---|---|---|
| Cluster federation | **daimon federation** | Done | 80% | Multi-node + federated vector store, 73 tests |
| Cross-cluster discovery | **daimon** A2A delegation | Done | 75% | Agent-to-agent delegation, 28 tests |
| State replication | **daimon migration** | Done | 75% | Checkpointing, state transfer, 54 tests |

**Reference code**:
- `userland/agent-runtime/src/federation.rs` — multi-node federation (73 tests)
- `userland/agent-runtime/src/delegation.rs` — A2A (28 tests)
- `userland/agent-runtime/src/migration.rs` — checkpointing (54 tests)
- `/home/macro/Repos/secureyeoman/packages/core/src/federation/` — federation crypto, manager, routes, storage (mature, tested)

### Observability

| k8s Component | AGNOS Equivalent | Status | Maturity | Notes |
|---|---|---|---|---|
| Prometheus + metrics | **daimon** `/v1/metrics/prometheus` | Done | 90% | Native Prometheus endpoint |
| Distributed tracing | **daimon** `/v1/traces` + OTLP | Done | 85% | OpenTelemetry-compatible |
| Logging | **agnosys** journald + audit | Done | 90% | Cryptographic hash chain audit log |
| Dashboard | **nazar** (system monitor) | Done | 80% | egui GUI, connects to daimon + hoosh |

**Reference code**:
- `userland/agent-runtime/src/http/handlers/traces.rs` — trace collection
- `/home/macro/Repos/nazar/` — system monitor (27 tests)

### Package & Deployment

| k8s Component | AGNOS Equivalent | Status | Maturity | Notes |
|---|---|---|---|---|
| Helm charts | **sutra** playbooks | Done | 85% | TOML/Markdown/NL input, Tera templates |
| Container registry | **mela** marketplace + Delta registry | Done | 80% | Local + remote, trust verification |
| Rolling updates | **daimon edge** update flow | Done | 75% | Per-node update + completion ack |
| Rollback | **daimon rollback** module | Done | 70% | State rollback |
| CI/CD | **takumi** build system | Done | 90% | 56 tests, recipe-based builds |

**Reference code**:
- `userland/agent-runtime/src/takumi.rs` — build system (56 tests)
- `userland/agent-runtime/src/marketplace/` — trust, transparency, registry
- `/home/macro/Repos/delta/` — code hosting + artifact registry

### AI-Native (no k8s equivalent)

| Component | AGNOS Subsystem | Status | Maturity | Notes |
|---|---|---|---|---|
| Model serving | **murti** + **hoosh** | Planned/Done | 60% | murti extracts from Irfan, hoosh is gateway |
| Model training | **Irfan** training pipeline | Done | 85% | 6 methods, distributed, 1400+ tests |
| Agent lifecycle | **daimon** orchestrator | Done | 90% | Register, heartbeat, sandbox, deregister |
| LLM routing | **hoosh** 15 providers | Done | 90% | Cloud + local, caching, budgets |
| RAG/vector store | **daimon** vector_store + rag | Done | 80% | Vector search, knowledge ingestion |
| AI safety | **safety** module | Done | 85% | Injection detection, 77 tests |
| Explainability | **explainability** module | Done | 80% | Decision transparency, 59 tests |

---

## Overall Progress

| Category | Estimated Completion |
|----------|---------------------|
| Container runtime (stiva) | **15%** — scaffolded, image/container/runtime/network/storage/registry/compose modules, 17 tests |
| Scheduling | **90%** — majra + daimon scheduler mature |
| Fleet / node management | **85%** — daimon edge + majra heartbeat + SY reference |
| Networking / service discovery | **55%** — basics done, nein scaffolded (24 tests), ingress remaining |
| Configuration / secrets | **85%** — sutra + aegis + oidc |
| Federation | **80%** — daimon + SY reference code |
| Observability | **90%** — Prometheus, OTLP, audit, nazar |
| Package / deployment | **85%** — ark + takumi + marketplace |
| AI-native (unique to AGNOS) | **85%** — murti extraction remaining |
| **Weighted Total** | **~78%** |

## Key Gaps (Priority Order)

### 1. Stiva — Container Runtime (scaffolded, blocks full k8s parity)
OCI-compatible container runtime. Scaffolded at `/home/macro/Repos/stiva/` with image, container, runtime, network, storage, registry, compose modules. 17 tests.

**Remaining**: OCI distribution spec client (image pull), overlay FS assembly, kavach sandbox integration for container execution, cgroup resource limits, container logging. See `stiva/README.md` roadmap.

### 2. Nein — Firewall / Network Policy (scaffolded, blocks service mesh completion)
nftables rule builder. Scaffolded at `/home/macro/Repos/nein/` with rule/table/chain types, NAT (DNAT/SNAT/masquerade), network policies, pre-built builders, apply via `nft -f -`. 24 tests.

**Remaining**: stiva container networking integration, dynamic rule updates for agent lifecycle, real nftables integration tests, nftables sets/maps. See `nein/README.md` roadmap.

### 3. Ingress / External Traffic Routing
No equivalent yet. Internal agent-to-agent routing works (daimon discover + capability routing), but external traffic into the cluster needs an ingress layer.

### 4. Murti Extraction (blocks Ollama replacement)
Extract model runtime from Irfan into shared crate. See [murti spec](applications/murti.md).

---

## Reference Projects for Mature Code

| Project | What to Extract/Reference | Path | Maturity | Notes |
|---------|--------------------------|------|----------|-------|
| **Majra** | Priority queues, DAG scheduling, heartbeat FSM, pub/sub, relay, IPC, rate limiting | `/home/macro/Repos/majra/src/` | **High** | Rust, crates.io (0.21.3), criterion benchmarks, proptest, SQLite persistence. Production-grade — this is the queue/fleet backbone |
| **Daimon** edge/scheduler/federation | Fleet nodes, scheduling, federation, service mesh, sandbox | `userland/agent-runtime/src/` | **High** | Rust, 3897+ tests, 84% coverage. Core of AGNOS orchestration |
| **Irfan** `ifran-core` + `ifran-backends` | Model registry, store, pull, 15 backends, GPU allocation → murti | `/home/macro/Repos/ifran/crates/` | **High** | Rust, 1406 tests, 73% coverage. Mature model lifecycle code to extract into murti |
| **Sutra** + **sutra-community** | Playbook orchestration, nftables/sysctl/aegis/daimon modules | `/home/macro/Repos/sutra/`, `/home/macro/Repos/sutra-community/` | **Medium-High** | Rust, 70 tests. Playbook execution is solid; community modules are newer |
| **SecureYeoman** `edge/` | Fleet orchestration patterns, node store, fleet HTTP routes | `/home/macro/Repos/secureyeoman/packages/core/src/edge/` | **Medium** | TypeScript/Bun. Working fleet patterns but not as mature as majra's Rust heartbeat FSM. Good for architectural reference, less for direct code extraction |
| **SecureYeoman** `federation/` | Federation crypto, manager, routes, storage | `/home/macro/Repos/secureyeoman/packages/core/src/federation/` | **Medium** | TypeScript. Tested (4 test files). Pattern reference for cross-node trust and state sync |
| **SecureYeoman** `ha/` | HA health check patterns | `/home/macro/Repos/secureyeoman/packages/core/src/ha/` | **Medium** | TypeScript. Health check patterns — majra's heartbeat FSM is the Rust equivalent and more mature |

**Priority for ingestion**: majra → daimon → Irfan → sutra → SY (for patterns only, not direct code)

---

*Last Updated: 2026-03-21*
