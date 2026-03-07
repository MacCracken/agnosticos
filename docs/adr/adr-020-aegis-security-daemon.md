# ADR-020: Aegis — System Security Daemon

**Status:** Accepted
**Date:** 2026-03-07

## Context

AGNOS security primitives are distributed across multiple modules:
- `sandbox.rs` + `seccomp_profiles.rs` — process sandboxing
- `learning.rs` — anomaly detection for agent behavior
- `integrity.rs` — file hash verification
- `sigil.rs` (ADR-019) — trust verification and signing
- `agnos-sys/` — IMA, TPM, dm-verity, Landlock, seccomp, MAC, audit, nftables

There is no central coordination point that:
1. Correlates security events across subsystems
2. Makes automated response decisions (quarantine, rate-limit, terminate)
3. Provides a unified threat dashboard
4. Scans packages and agent binaries on install/execute

## Decision

Create `aegis` — a unified security coordination daemon that:

### Event Pipeline
```
Security Event Sources:
  sigil (trust violation)  ──┐
  integrity (file tamper)  ──┤
  anomaly (behavior drift) ──┼──> aegis ──> classify ──> respond
  sandbox (escape attempt) ──┤        │
  nftables (network block) ──┘        ├──> quarantine (if critical/high)
                                      ├──> alert (dashboard + audit log)
                                      └──> resolve (manual or auto-timeout)
```

### Threat Levels
Five levels driving automated response:
- **Critical** — immediate quarantine + terminate. Examples: sandbox escape, trust revocation
- **High** — quarantine + suspend. Examples: integrity mismatch, persistent anomalies
- **Medium** — alert + rate-limit. Examples: resource spikes, unusual network patterns
- **Low** — log + monitor. Examples: first-time anomaly, minor policy drift
- **Info** — informational. Examples: scan completion, agent lifecycle events

### Quarantine System
Quarantined agents are:
- Suspended (SIGSTOP) or terminated depending on severity
- Network isolated (nftables DROP all)
- Filesystem restricted (Landlock revoked to read-only)
- Logged in audit chain

Release is either:
- Manual (operator via dashboard or CLI)
- Automatic after configurable timeout (for transient issues)

### Scanning
Two trigger points:
- **On-install**: aegis scans .ark/.agnos-agent packages before ark installs them
- **On-execute**: aegis checks agent binary before supervisor spawns it

Scanning checks:
- File permissions (no world-writable, no SUID)
- Binary size limits
- Known-bad hash lookup (revocation list via sigil)
- Sandbox profile validation

### Integration
- **sigil** — aegis calls sigil for trust verification during scans
- **ark** — ark calls aegis.scan_package() before install
- **supervisor** — supervisor calls aegis.scan_agent() before spawn
- **anomaly detector** — feeds BehaviorSample events to aegis
- **desktop-environment** — security dashboard reads aegis events/quarantine status

## Consequences

### Positive
- Single point of security coordination — no scattered ad-hoc checks
- Automated threat response reduces operator burden
- Quarantine prevents compromised agents from causing further damage
- Unified event log enables correlation across security subsystems
- Scanning catches issues before they reach runtime

### Negative
- Additional latency on agent spawn (scan step)
- Quarantine false positives can disrupt legitimate agents
- Event volume may be high in large agent fleets

### Mitigations
- Scan results cached by content hash — repeat spawns skip re-scan
- Auto-release timeout prevents permanent false-positive quarantine
- Event pruning keeps memory bounded (configurable max_events)
- AuditOnly mode for development environments

## Related
- ADR-019: Sigil — System-Wide Trust Verification
- ADR-013: Zero-Trust Security Hardening
- ADR-018: LFS-Native Distribution (aegis scans .ark packages)
