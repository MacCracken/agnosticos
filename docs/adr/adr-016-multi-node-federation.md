# ADR-016: Multi-Node Agent Federation

**Status:** Proposed

**Date:** 2026-03-06

**Authors:** AGNOS Team

## Context

AGNOS currently operates as a single-node system. All agents, the LLM Gateway, and the
Agent Runtime run on one machine. This is sufficient for personal use and small deployments,
but limits AGNOS in several ways:

1. **Scale** — a single node has finite CPU, memory, and GPU. Large agent fleets or
   compute-intensive agents (vision, code generation) exhaust resources.
2. **Availability** — if the node goes down, all agents stop. No failover.
3. **Locality** — agents may need to run close to specific resources (a database,
   a network segment, a GPU cluster).
4. **Collaboration** — multiple users cannot share an agent fleet across machines.

Phase 7 introduces **federation**: multiple AGNOS nodes form a cluster where agents
can be scheduled across nodes, migrated between them, and share state.

## Decision

### Federation Model

- **Peer-to-peer with coordinator**: One node is elected coordinator (Raft consensus
  among a configurable set of nodes). The coordinator manages scheduling decisions;
  all nodes can accept API requests and forward to the coordinator.
- **Node identity**: Each node has an Ed25519 keypair generated on first boot. Nodes
  authenticate via mTLS using certificates signed by a shared federation CA.
- **Discovery**: Nodes find each other via:
  - Static config (`federation.toml` with node addresses)
  - mDNS/DNS-SD on local networks (optional, for home/lab setups)
  - DNS SRV records (for production deployments)
- **Minimum cluster size**: 1 (single-node mode, no federation overhead). Federation
  features activate when 2+ nodes join.

### Distributed Task Scheduling

- **Scheduler**: The coordinator node runs a scheduler that assigns tasks to nodes based on:
  - Resource availability (CPU, memory, GPU headroom)
  - Agent locality preferences (`manifest.toml: prefer_node = "gpu-node-1"`)
  - Data locality (agent's vector store, memory store location)
  - Network affinity (agents that communicate frequently should be co-located)
- **Scheduling algorithm**: Two-phase — first filter eligible nodes (resource fit,
  capability match), then score by weighted criteria (resource headroom 40%,
  locality 30%, load balance 20%, affinity 10%).
- **Preemption**: Higher-priority agents can preempt lower-priority ones. Preempted
  agents are migrated to another node or queued.

### Agent Migration & Checkpointing

- **Checkpoint format**: Agent state serialized as:
  - Process memory snapshot (CRIU-based on Linux)
  - Memory store (KV pairs from `memory_store.rs`)
  - Vector store indices (if agent-local)
  - In-flight IPC messages (drained before checkpoint)
  - Sandbox configuration (recreated on destination)
- **Migration flow**:
  1. Source node signals agent to quiesce (drain IPC, flush state)
  2. Source creates checkpoint
  3. Checkpoint transferred to destination node (compressed, encrypted in transit)
  4. Destination recreates sandbox and restores agent from checkpoint
  5. Source releases agent's resources
  6. IPC routing updated (pub/sub subscribers, RPC endpoints)
- **Live migration** (stretch goal): Iterative memory copy while agent continues running,
  final freeze only for the last dirty pages. Requires CRIU `--pre-dump` support.
- **Downtime target**: <500ms for warm migration (checkpoint + restore), <5s for cold
  migration (full image transfer).

### Shared State

- **Distributed vector store**: Vector store collections can be replicated across nodes.
  Writes go to the primary (determined by collection's home node), reads can go to any
  replica. Eventual consistency with configurable replication factor.
- **Memory store replication**: Agent KV stores are replicated to 1+ backup nodes.
  On node failure, the backup promotes to primary. Raft log for write ordering.
- **Audit chain**: Each node maintains its own audit chain. A federation-level audit
  aggregator merges chains with Lamport timestamps for causal ordering.
- **LLM Gateway**: Each node runs its own gateway. Token budgets are coordinated via
  the coordinator to prevent over-consumption across the cluster.

### Network Architecture

- **Control plane**: gRPC between nodes (coordinator election, scheduling, health).
  Port 8092 (configurable via `AGNOS_FEDERATION_PORT`).
- **Data plane**: Agent-to-agent RPC across nodes uses the same UDS protocol tunneled
  over mTLS TCP. The IPC layer transparently routes cross-node messages.
- **Health checking**: Nodes send heartbeats to the coordinator every 5s. After 3 missed
  heartbeats (15s), the node is marked suspect. After 30s, marked dead and its agents
  are rescheduled.

### Federation Configuration

```toml
# /etc/agnos/federation.toml
[federation]
enabled = true
node_name = "node-1"
bind_addr = "0.0.0.0:8092"

[federation.peers]
"node-2" = "192.168.1.102:8092"
"node-3" = "192.168.1.103:8092"

[federation.scheduling]
strategy = "balanced"  # balanced | packed | spread
gpu_weight = 2.0       # prefer GPU nodes for GPU-capable agents
```

## Consequences

### What becomes easier
- AGNOS scales beyond a single machine
- Agent fleets survive node failures
- GPU-heavy agents can target GPU nodes while lightweight agents run elsewhere
- Multiple users share a federated agent cluster

### What becomes harder
- Consensus protocol adds latency to scheduling decisions (~10-50ms)
- Agent migration requires CRIU (Linux-specific, complex)
- Distributed state introduces eventual consistency semantics
- Network partitions require careful handling (split-brain prevention)

### Risks
- CRIU compatibility: not all processes checkpoint cleanly (open file descriptors,
  device mappings). Mitigated by graceful quiesce step and per-agent migration
  eligibility flag in manifest.
- Split-brain: network partition could create two coordinators. Mitigated by Raft
  requiring majority quorum — minority partition cannot elect a leader.
- Cross-node IPC latency: agents communicating across nodes see ~1ms RTT instead of
  ~10us for local UDS. Mitigated by affinity scoring that co-locates communicating agents.

## Alternatives Considered

### Kubernetes as orchestrator
Rejected: AGNOS *is* the OS. Running Kubernetes on AGNOS to manage AGNOS agents adds
enormous complexity and defeats the purpose of a purpose-built agent runtime. The
scheduling and health-checking logic is simpler than K8s and tailored to agent semantics.

### Gossip protocol (SWIM) instead of Raft
Rejected for coordinator election: gossip provides membership but not consensus.
Scheduling decisions require a single coordinator to avoid conflicts. Gossip could
complement Raft for membership discovery in large clusters (>50 nodes).

### Shared filesystem (NFS/CephFS) for state replication
Rejected: adds external infrastructure dependency. AGNOS should be self-contained.
The built-in replication via Raft log is simpler for the expected cluster sizes (3-10 nodes).

## References

- Phase 7 roadmap: `docs/development/roadmap.md` (Federation & Scale section)
- ADR-013: Zero-Trust Security (mTLS infrastructure reused for federation)
- ADR-009: RAG & Knowledge Pipeline (vector store replication)
- Raft consensus: https://raft.github.io/
- CRIU: https://criu.org/Main_Page
