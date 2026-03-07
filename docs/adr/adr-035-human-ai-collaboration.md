# ADR-035: Human-AI Collaboration Research

**Status:** Accepted

**Date:** 2026-03-07

**Authors:** AGNOS Team

## Context

AGNOS's core principle is Human Sovereignty — AI assists, humans decide. But the
boundary between "assist" and "decide" is nuanced:

1. **Over-reliance**: Users rubber-stamp agent decisions without reviewing them
2. **Under-utilization**: Users manually do tasks agents could handle, wasting capability
3. **Cognitive overload**: Too many approval requests overwhelm users
4. **Trust miscalibration**: Users trust agents too much (automation bias) or too little

This ADR formalizes collaboration patterns to optimize the human-AI working relationship.

## Decision

### Collaboration Modes

Five modes spanning the autonomy spectrum:

| Mode | Human Role | Agent Role | Use Case |
|------|-----------|-----------|----------|
| Full Autonomy | Reviews results | Works independently | Routine, well-understood tasks |
| Supervised | Approves each step | Proposes actions | Sensitive operations |
| Paired | Works on subtasks | Works on other subtasks | Complex projects |
| Human-Led | Drives decisions | Assists on request | Creative/novel work |
| Teaching | Demonstrates patterns | Learns from examples | New task types |

Mode selection is guided by trust metrics — higher trust enables more autonomy.

### Trust Calibration

Continuous measurement of agent reliability:
- **Accuracy**: Does the agent get correct results?
- **Consistency**: Does it perform reliably across similar tasks?
- **Response quality**: Are outputs well-formed and useful?
- **Safety record**: Has it caused any safety violations?

Calibration error measures the gap between agent confidence and actual performance.
Well-calibrated agents (error < 0.1) earn higher trust and more autonomy.

### Handoff Protocol

Formal handoff when tasks transfer between human and agent:
- Context summary (what was done, what remains)
- Reason for handoff
- Acknowledgment required before work continues
- Full handoff history for audit

### Cognitive Load Management

Prevent human cognitive overload:
- Track current tasks, pending decisions, interruptions, time since break
- Defer non-urgent decisions when load is high
- Suggest breaks after 90 minutes of high-load work
- Adapt decision batch size to current cognitive capacity

### Feedback Loops

Structured feedback collection:
- Types: Correction, Praise, Suggestion, Complaint, Rating
- Track application status (was the feedback incorporated?)
- Per-agent feedback statistics and trends
- Unapplied corrections surfaced for action

### Collaboration Analytics

Measure and optimize collaboration effectiveness:
- Efficiency score (tasks per hour)
- Handoff overhead percentage
- Mode effectiveness per task type
- Human vs. agent active time breakdown

## Consequences

### What becomes easier
- Right-sizing agent autonomy based on measured trust
- Preventing human burnout from excessive approval requests
- Systematic improvement through feedback loops
- Data-driven collaboration mode selection

### What becomes harder
- Additional tracking overhead for all human-agent interactions
- Trust metrics may be gamed (agents optimizing for metrics, not quality)
- Cognitive load estimation is approximate

## References

- ADR-005: Security Model and Human Override (Human Sovereignty principle)
- ADR-028: Agent Explainability (decision transparency for trust building)
- ADR-029: AI Safety Mechanisms (safety constraints on collaboration)
- Parasuraman & Riley: "Humans and Automation: Use, Misuse, Disuse, Abuse"
- Lee & See: "Trust in Automation: Designing for Appropriate Reliance"
