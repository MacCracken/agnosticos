# ADR-028: Agent Explainability Framework

**Status:** Accepted

**Date:** 2026-03-07

**Authors:** AGNOS Team

## Context

AGNOS agents make autonomous decisions — choosing tools, scheduling tasks, responding to
security events, allocating resources. Users, administrators, and auditors need to understand
*why* an agent made a specific decision. Without explainability:

1. Users cannot trust agent decisions they don't understand
2. Debugging agent misbehavior requires guesswork
3. Compliance requirements (SOC2, GDPR Art. 22) demand decision transparency
4. Security incidents require post-hoc analysis of agent reasoning

## Decision

### Decision Records

Every significant agent decision is captured as a `DecisionRecord` containing:
- The action taken and reasoning
- Confidence score (0.0-1.0)
- Factors that influenced the decision (weighted)
- Alternatives considered and why they were rejected
- Outcome (recorded asynchronously after execution)

### Human-Readable Explanations

`ExplainabilityEngine::explain_decision()` transforms raw records into natural language:
- Summary: "Agent X chose to Y because Z"
- Factor breakdown with contribution percentages
- Alternatives summary
- Confidence label (Low/Medium/High) with review recommendation for low-confidence decisions

### Decision Trees

For complex multi-factor decisions, a `DecisionTree` provides visual structure:
- Nodes represent factor conditions
- Leaves represent chosen actions
- Text-based rendering for terminal/log output

### Audit Integration

Decision records link to the cryptographic audit chain via `AuditTrail`, enabling
correlation between agent decisions and system-level audit events.

### Agent Statistics

Per-agent decision analytics: average confidence, success rate, most common actions,
factor frequency, and count of decisions needing human review.

## Consequences

### What becomes easier
- Users understand and trust agent behavior
- Debugging agent issues through decision history
- Compliance auditing with full decision provenance
- Identifying agents with consistently low confidence (need retraining or reconfiguration)

### What becomes harder
- Storage grows with decision volume (mitigated by retention policies)
- Recording decisions adds small overhead per agent action (~1ms)

## References

- ADR-010: Advanced Agent Capabilities & Lifecycle
- ADR-011: Observability Stack
- GDPR Article 22: Automated decision-making
