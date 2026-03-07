# ADR-032: Novel Sandboxing Architectures

**Status:** Accepted

**Date:** 2026-03-07

**Authors:** AGNOS Team

## Context

AGNOS v1 sandboxing uses Landlock (filesystem) + seccomp (syscalls) + network namespaces.
This works well but has limitations:

1. **Coarse granularity**: Landlock paths are static; seccomp is binary allow/deny
2. **No delegation**: An agent cannot grant a subset of its permissions to a sub-agent
3. **No data flow tracking**: Once data leaves a sandbox, its sensitivity label is lost
4. **Static policies**: Sandbox profiles don't adapt to agent behavior
5. **No time bounds**: Sandboxes persist indefinitely; no resource budgets

## Decision

### Capability-Based Security (Object-Capability Model)

Replace static permission checks with unforgeable capability tokens:
- Fine-grained: `FileRead("/data/agent-1/*")` rather than "filesystem access"
- Time-bounded: capabilities expire after configurable duration
- Delegatable: agents can delegate subsets of their capabilities to collaborators
- Revocable: capabilities can be individually revoked without restarting agents

### Information Flow Control

Mandatory taint tracking for sensitive data:
- `SecurityLabel` hierarchy: Public < Internal < Confidential < Secret < TopSecret
- Data can only flow upward (Public → Confidential ok, Confidential → Public blocked)
- `FlowTracker` maintains data lineage across agent interactions
- Prevents data exfiltration even through multi-agent chains

### Time-Bounded Sandboxes

Sandboxes with resource budgets:
- Maximum wall-clock duration, CPU seconds, and operation count
- Auto-terminates agents that exceed budgets
- Prevents runaway agents from consuming unlimited resources

### Learned Sandbox Policies

Derive sandbox profiles from observed behavior:
- `PolicyLearner` collects behavioral observations during development/testing
- `generate_policy()` produces allow/deny rules from observations
- `suggest_tightening()` identifies rarely-used permissions for removal
- Moves toward least-privilege automatically

### Composable Sandbox Layers

Stack multiple sandbox layers with clear composition semantics:
- Each layer (filesystem, network, process, IPC, resource) has independent rules
- Composition: most restrictive verdict wins (Deny > AuditLog > Allow)
- Merge operation combines profiles from different sources

## Consequences

### What becomes easier
- Least-privilege enforcement without manual policy writing
- Tracking sensitive data across agent boundaries
- Dynamic permission management without agent restarts
- Defense in depth through layered sandboxing

### What becomes harder
- Capability token management adds complexity
- Information flow tracking has runtime overhead (~5% for labeled data)
- Learned policies may be too permissive if training data is incomplete

## References

- ADR-005: Security Model and Human Override
- ADR-013: Zero-Trust Security Hardening
- Object-capability model: https://en.wikipedia.org/wiki/Object-capability_model
- Decentralized Information Flow Control: Myers & Liskov, SOSP 1997
