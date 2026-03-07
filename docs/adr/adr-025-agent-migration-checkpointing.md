# ADR-025: Agent Migration & Checkpointing

**Status:** Accepted

**Date:** 2026-03-07

**Authors:** AGNOS Team

## Context

With multi-node federation (ADR-016), agents need to move between nodes for load balancing,
failover, and resource optimization. This requires a robust checkpoint/restore mechanism
that captures agent state (memory store, vector indices, in-flight IPC messages, sandbox
config) and transfers it to another node with minimal downtime.

Without migration, a node failure means all its agents are lost and must be restarted from
scratch — losing in-memory state, conversation context, and learned behavior.

## Decision

### Checkpoint Format

An agent checkpoint captures:
- **Memory snapshot**: KV store dump (HashMap<String, serde_json::Value>)
- **Vector indices**: List of index names to transfer
- **IPC queue**: Drained in-flight messages (sender, recipient, payload, timestamp)
- **Sandbox config**: Serialized sandbox configuration for recreation on destination
- **Metadata**: agent_id, checkpoint_id (UUID), source_node, created_at, total_size_bytes

Checkpoints support compression (~60% size reduction) for network transfer.

### Migration Types

| Type | Flow | Target Downtime |
|------|------|----------------|
| **Warm** | Quiesce → checkpoint → transfer → restore | <500ms |
| **Cold** | Stop → full image → transfer → start | <5s |
| **Live** | Iterative copy while running (stretch goal) | <100ms |

### Migration State Machine

```
Pending → Quiescing → Checkpointing → Transferring → Restoring → Verifying → Complete
                                                                              ↓
Any state ──────────────────────────────────────────────────────────→ Failed
```

State transitions are validated — invalid transitions are rejected.

### Migration Tracking

A `MigrationTracker` maintains:
- Active migrations with current state and state history
- Migration history per agent (for auditing and debugging)
- Migration records with timing data (started_at, completed_at)

### Validation

Checkpoints are validated before transfer:
- Agent ID must be non-empty
- Must contain at least memory or IPC data
- Size must be within configurable limits
- Compressed checkpoints must be decompressed before restore

## Consequences

### What becomes easier
- Transparent agent mobility across federated nodes
- Zero-downtime node maintenance (drain agents before shutdown)
- Automatic failover on node failure (restore from last checkpoint)
- Load rebalancing without losing agent state

### What becomes harder
- Checkpoint size grows with agent state — large vector indices are expensive to transfer
- Not all agent state is serializable (open file descriptors, network connections)
- Migration adds temporary resource overhead on both source and destination

### Risks
- Checkpoint consistency: agent must be quiesced to ensure atomic state capture
- Transfer failures: network interruption during migration requires retry or rollback
- Mitigated by state machine validation and explicit failure states

## References

- ADR-016: Multi-Node Agent Federation
- ADR-010: Advanced Agent Capabilities & Lifecycle
- CRIU project: https://criu.org/Main_Page
