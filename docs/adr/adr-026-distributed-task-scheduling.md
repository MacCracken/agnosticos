# ADR-026: Distributed Task Scheduling

**Status:** Accepted

**Date:** 2026-03-07

**Authors:** AGNOS Team

## Context

AGNOS agents execute tasks, but scheduling is currently first-come-first-served within a
single node. With federation (ADR-016), tasks must be distributed across nodes intelligently.
Additionally, users need:

1. **Priority-based scheduling** — critical security scans should preempt low-priority indexing
2. **Resource-aware placement** — GPU tasks go to GPU nodes, memory-heavy tasks avoid loaded nodes
3. **Cron scheduling** — recurring tasks (daily scans, weekly reports) without external cron
4. **Preemption** — higher-priority tasks can bump lower-priority ones

## Decision

### Task Model

Each task has:
- **Priority**: Normal (1-3), High (4-6), Critical (7-9), Emergency (10)
- **Resource requirements**: CPU cores, memory MB, GPU, network, disk
- **Status**: Queued → Scheduled → Running → Completed/Failed/Cancelled/Preempted
- **Optional deadline**: tasks with deadlines are prioritized as deadline approaches
- **Node preference**: optional hint for locality (e.g., "gpu-node-1")

### Scheduling Algorithm

1. **Filter**: Eliminate nodes that cannot fit the task's resource requirements
2. **Score**: Rank eligible nodes by:
   - Resource headroom (40% weight)
   - Node preference match (30% weight)
   - Load balance — prefer less-utilized nodes (20% weight)
   - Affinity — co-locate communicating agents (10% weight)
3. **Assign**: Task goes to highest-scoring node

Pending tasks are scheduled in priority order (highest first), then by creation time
(oldest first) for equal priorities.

### Preemption

A task can preempt another if:
- Its priority level is strictly higher (Emergency > Critical > High > Normal)
- The preempted task is Running (not Completed/Failed)
- The preempting task cannot fit on any node without preemption

Preempted tasks are re-queued for scheduling on another node.

### Cron Scheduling

Simple interval-based and time-of-day scheduling:
- `CronEntry`: name, interval (seconds) or specific hour:minute, task template, enabled flag
- `CronScheduler`: checks due entries, generates tasks from templates
- No external cron dependency — fully self-contained

### Node Capacity

Each node reports:
- Total and available CPU, memory, GPU
- Running task count
- Utilization ratio (0.0 to 1.0)

Capacity is updated on each heartbeat in federation protocol.

## Consequences

### What becomes easier
- Automated resource-optimal task placement across cluster
- Time-based automation without external schedulers
- Graceful handling of overload via preemption
- Priority inversion prevention

### What becomes harder
- Scheduling decisions add latency (~1-5ms per decision)
- Preemption complicates task lifecycle (agents must handle being preempted)
- Cron entries need persistence across restarts

### Risks
- Starvation: low-priority tasks may never run if high-priority tasks keep arriving.
  Mitigated by aging — tasks waiting too long get priority boost (future enhancement).
- Thundering herd: many cron entries firing simultaneously. Mitigated by jitter on cron
  fire times (±10% of interval).

## Alternatives Considered

### External scheduler (Celery, Airflow)
Rejected: adds Python/Java dependency, doesn't integrate with AGNOS agent semantics,
and federation-aware scheduling needs deep integration with node health and capabilities.

### Simple round-robin
Rejected: ignores resource constraints and locality. Fine for homogeneous tasks but
AGNOS agents have wildly different resource profiles (text agents vs vision agents).

## References

- ADR-016: Multi-Node Agent Federation (scheduling section)
- ADR-010: Advanced Agent Capabilities & Lifecycle
- Kubernetes scheduler: https://kubernetes.io/docs/concepts/scheduling/
