# ADR-003: Multi-Agent Orchestration Architecture

**Status:** Accepted

**Date:** 2026-02-08

**Authors:** AGNOS Team

## Context

AGNOS must support multiple AI agents running simultaneously with:
- Resource allocation and management
- Isolation between agents
- Communication between agents
- Human oversight and control
- Fault tolerance and recovery

## Decision

We will implement a **centralized orchestrator** with:
1. **Agent Runtime Daemon (akd)** - Manages agent lifecycle
2. **Orchestrator** - Schedules tasks and manages resources
3. **Registry** - Tracks agent capabilities and status
4. **IPC Bus** - Message passing between agents
5. **Supervisor** - Monitors health and enforces policies

## Consequences

### Positive
- Centralized control enables consistent policy enforcement
- Easier to implement resource quotas and limits
- Simpler human oversight interface
- Better audit trail and logging
- Easier debugging and monitoring

### Negative
- Single point of failure (mitigated by supervisor)
- Potential bottleneck at high agent counts
- More complex to scale horizontally

## Alternatives Considered

### Fully Distributed (Mesh)
**Rejected:** While more scalable, distributed consensus adds complexity and makes human oversight harder.

### Microservices
**Rejected:** Overkill for MVP. Each agent is already a separate process.

### Kubernetes-style
**Rejected:** Too heavy for desktop OS. Designed for server clusters, not single-user systems.

## Architecture Details

```
┌─────────────────────────────────────┐
│         Orchestrator               │
│  ┌──────────┐  ┌──────────────┐   │
│  │ Scheduler│  │   Registry    │   │
│  └────┬─────┘  └──────┬───────┘   │
│       │               │           │
│  ┌────┴───────────────┴───────┐   │
│  │       Agent Pool            │   │
│  │  ┌─────┐ ┌─────┐ ┌─────┐   │   │
│  │  │Agent│ │Agent│ │Agent│   │   │
│  │  │  1  │ │  2  │ │  3  │   │   │
│  │  └─────┘ └─────┘ └─────┘   │   │
│  └─────────────────────────────┘   │
└─────────────────────────────────────┘
```

## References

- [Actor Model](https://en.wikipedia.org/wiki/Actor_model)
- [Kubernetes Architecture](https://kubernetes.io/docs/concepts/architecture/)
- [Erlang/OTP Supervisors](https://erlang.org/doc/design_principles/sup_princ.html)
