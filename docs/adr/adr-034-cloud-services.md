# ADR-034: Cloud Services

**Status:** Accepted

**Date:** 2026-03-07

**Authors:** AGNOS Team

## Context

AGNOS is designed as a local-first OS — agents run on the user's hardware with full
privacy and sovereignty. However, some use cases benefit from cloud connectivity:

1. **Resource overflow**: Local GPU insufficient for large models; cloud GPUs available
2. **Cross-device**: User has AGNOS on laptop and desktop; agents should sync seamlessly
3. **Collaboration**: Teams sharing agent configurations and workspaces
4. **Always-on**: Some agents need 24/7 uptime that a laptop cannot provide

All cloud features are strictly opt-in. AGNOS works fully offline.

## Decision

### Cloud Agent Deployment

Agents can be deployed to cloud instances with configurable resource tiers:
- **Free**: 1 CPU, 512MB, no GPU (for lightweight agents)
- **Standard**: 2 CPU, 4GB, no GPU (general purpose)
- **Performance**: 4 CPU, 16GB, 1 GPU (ML/vision agents)
- **Custom**: User-defined resources

Cloud agents maintain the same sandbox, trust, and audit guarantees as local agents.

### Cross-Device Sync

State synchronization across AGNOS instances:
- Sync items: agent configs, memory stores, preferences, sandbox profiles
- Version-based conflict detection with SHA-256 checksums
- Conflict resolution: local-wins, remote-wins, manual, or merge
- Encryption required by default for all synced data

### Collaborative Workspaces

Shared environments for team use:
- Role-based access: Owner, Admin, Editor, Viewer
- Shared agent memory and vector stores (configurable)
- Full audit trail for all workspace actions
- Member limits and agent limits per workspace

### Billing

Usage tracking for cloud resources:
- Compute, storage, network egress, sync operations
- Per-workspace and per-agent cost attribution
- Monthly cost estimation for deployments

## Consequences

### What becomes easier
- Running resource-intensive agents without local hardware
- Seamless agent experience across multiple devices
- Team collaboration on shared agent fleets
- 24/7 agent availability for monitoring/automation

### What becomes harder
- Privacy: cloud deployment means data leaves the device (mitigated by encryption)
- Latency: cloud agents have higher latency than local
- Cost: cloud resources have ongoing costs
- Complexity: sync conflicts require resolution logic

## References

- ADR-016: Multi-Node Federation (cloud nodes as federation peers)
- ADR-025: Agent Migration & Checkpointing (deploy = migrate to cloud)
- ADR-013: Zero-Trust Security (cloud connections require mTLS)
