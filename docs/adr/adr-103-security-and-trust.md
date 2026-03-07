# ADR-103: Security and Trust

**Status:** Accepted
**Date:** 2026-03-07

## Context

An AI-native OS must balance agent autonomy with human control. Agents execute untrusted code, access sensitive resources, and communicate across trust boundaries. AGNOS implements defense in depth from kernel to application layer.

## Decisions

### Tiered Permission System

Four permission levels control agent actions:

| Level | Risk | Behavior |
|-------|------|----------|
| **Implicit** | None | Read-only ops proceed silently |
| **Automatic** | Low | Notification only |
| **Confirmation** | Medium | Blocks until human approves |
| **Override** | High | Requires explicit human approval |

Permission categories: `file:read`, `file:write`, `file:delete`, `network:outbound`, `process:spawn`, `agent:delegate`. An emergency kill switch provides immediate all-agent shutdown.

### Sandbox Stack

Applied in order for every agent process:

1. **Encrypted storage** — LUKS for data at rest
2. **MAC** — Mandatory access control policies
3. **Landlock** — Filesystem sandboxing (kernel-level)
4. **Seccomp** — System call filtering (basic 20-syscall filter or custom BPF per agent)
5. **Network namespaces** — Network isolation
6. **Audit** — All actions recorded in cryptographic hash chain

### Sigil Trust Verification

`sigil` is the unified trust system for all artifacts:

**Trust levels** (highest to lowest):
- **SystemCore** — kernel, init, core AGNOS binaries (signed by project key)
- **Verified** — signed by trusted publisher
- **Community** — signed by unverified publisher
- **Unverified** — no signature
- **Revoked** — explicitly revoked

**Enforcement modes**: Strict (only Verified/SystemCore can run), Permissive (warns on unverified), AuditOnly (logs only).

**Verification chain**:
- Boot: TPM PCR verification, argonaut validates boot components
- Install: ark verifies `.ark` package signature + hash before installing
- Execute: agent-runtime verifies binary before spawning
- Runtime: periodic integrity checks, alert on mismatch

**Revocation**: Maintained locally, synced from `packages.agnos.org/revocations.json`. Revoke by key_id or content_hash.

### Aegis Security Daemon

`aegis` is the runtime security coordinator:

- **Event correlation** — aggregates security events from all subsystems
- **Automated response** — quarantines agents on policy violation
- **Package scanning** — verifies packages before installation
- **Threat detection** — behavioral anomaly detection using trailing resource/syscall profiles
- **Compliance monitoring** — CIS benchmark validation, PQC migration status

### Zero-Trust Hardening

- **mTLS** — mutual TLS between all services (agent-to-agent, agent-to-gateway, federation nodes)
- **Secrets rotation** — automatic rotation with configurable intervals
- **Integrity attestation** — TPM-backed measurements at boot and runtime
- **Anomaly detection** — per-agent behavioral baselines, alerts on deviation

### Post-Quantum Cryptography

Hybrid approach: every operation uses both classical and PQC algorithms.

| Operation | Classical | Post-Quantum |
|-----------|-----------|-------------|
| Key Exchange | X25519 | ML-KEM-768/1024 (FIPS 203) |
| Signatures | Ed25519 | ML-DSA-65/87 (FIPS 204) |

Three migration modes: Disabled (classical only), Hybrid (both, default for new installs), PqcOnly (future). Currently uses simulated PQC operations; swap to real `ml-kem`/`ml-dsa` crates when stable.

### Formal Verification

~15 security properties verified systematically:
- Audit chain integrity (no gaps, valid hashes)
- Sandbox isolation (no cross-agent data leaks)
- State machine validity (agent lifecycle transitions)
- Permission enforcement (no privilege escalation)

Runtime monitoring complements static verification.

### Novel Sandboxing

Beyond standard Landlock+seccomp:
- **Capability tokens** — fine-grained, revocable access tokens per resource
- **Information flow control** — security labels on data, preventing cross-boundary leaks
- **Time-bounded sandboxes** — resource budgets with automatic termination
- **Learned policies** — behavioral observation generates sandbox profiles

## Consequences

### Positive
- Defense in depth: multiple independent layers must all be breached
- Human sovereignty maintained via tiered permissions and kill switch
- Quantum-resistant before quantum computers are practical
- Every artifact has a clear, auditable trust level

### Negative
- Security checks add latency to every operation
- PQC keys and signatures are larger (1184 vs 32 bytes for public keys)
- Formal verification is bounded by the properties defined
- Complex key management infrastructure required
