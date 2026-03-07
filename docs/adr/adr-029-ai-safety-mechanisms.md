# ADR-029: AI Safety Mechanisms

**Status:** Accepted

**Date:** 2026-03-07

**Authors:** AGNOS Team

## Context

AGNOS agents execute actions with real-world consequences: file operations, process
management, network requests, privilege escalation. Without safety guardrails:

1. A compromised or hallucinating agent could delete critical files
2. Prompt injection could override agent instructions
3. Runaway agents could exhaust system resources
4. Agents could generate harmful or toxic output

The AGNOS security model (ADR-005) establishes Human Sovereignty — humans must approve
sensitive operations. This ADR extends that principle with automated safety mechanisms
that enforce constraints even when human approval is not immediately available.

## Decision

### Safety Policies

Declarative safety rules with configurable enforcement:
- **Block**: Action is prevented, violation recorded
- **Warn**: Action proceeds with warning logged
- **AuditOnly**: Action proceeds, violation recorded for review

Rule types cover the full attack surface:
- Resource limits, forbidden actions, approval requirements
- Rate limiting, content filtering, scope restrictions
- Privilege escalation controls, output validation

### Prompt Injection Detection

`PromptInjectionDetector` analyzes agent inputs for common injection patterns:
- "Ignore previous instructions" variants
- System prompt markers and role confusion
- Encoded payloads (base64 heuristics)
- Excessive special characters

### Safety Circuit Breaker

Per-agent circuit breaker (Closed → Open → HalfOpen) automatically blocks agents
that accumulate too many safety violations in a time window. Prevents cascade failures
from compromised or malfunctioning agents.

### Default Policies

Sensible defaults that protect against catastrophic actions:
- Block destructive system commands (rm -rf /, mkfs, dd)
- Require approval for privilege escalation
- Rate limit system commands (60/minute)
- Deny write access to sensitive files (/etc/shadow, /etc/passwd)

### Safety Scoring

Each agent maintains a safety score (0.0-1.0) based on violation history.
Low scores trigger alerts and can restrict agent capabilities.

## Consequences

### What becomes easier
- Preventing catastrophic agent actions (defense in depth beyond sandboxing)
- Detecting and blocking prompt injection attacks
- Automatic containment of misbehaving agents
- Compliance with AI safety frameworks

### What becomes harder
- False positives may block legitimate actions (mitigated by tunable policies)
- Safety checks add latency to every action (~0.1ms per check)
- Policy management adds operational complexity

## References

- ADR-005: Security Model and Human Override
- ADR-020: Aegis — System Security Daemon
- OWASP Top 10 for LLM Applications
- NIST AI Risk Management Framework
