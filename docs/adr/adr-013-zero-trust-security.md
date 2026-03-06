# ADR-013: Zero-Trust Security Hardening

**Status:** Accepted

**Date:** 2026-03-06

**Authors:** AGNOS Team

## Context

AGNOS already has a strong security foundation: Landlock + seccomp sandboxing, MAC profiles
(SELinux/AppArmor), encrypted storage (LUKS), dm-verity, IMA/EVM, TPM-sealed secrets, certificate
pinning, and a cryptographic audit chain. However, the current model is largely **static** — security
policies are configured at deploy time and assume agents behave as expected.

Phase 6.8 adds four **dynamic** security capabilities:

1. **Behavior anomaly detection** — agents that deviate from their baseline are flagged
2. **Mutual TLS** — all agent-to-agent and agent-to-gateway communication is authenticated
3. **Secrets rotation** — API keys and certificates rotate automatically
4. **Runtime integrity attestation** — periodic verification that agents haven't been tampered with

Together, these move AGNOS from "secure by configuration" to "zero-trust at runtime."

## Decision

### Agent Behavior Anomaly Detection (`agent-runtime/learning.rs`)

- **Baseline collection**: During an agent's first N hours (configurable, default 24h), the
  system records a behavioral profile:
  - Syscall frequency distribution (top-20 syscalls, normalized)
  - Network connection patterns (destination IPs/ports, bytes in/out)
  - File access patterns (directories accessed, read/write ratio)
  - IPC message rates and destinations
  - Resource usage profile (CPU/memory mean and stddev)
- **Storage**: Profile stored as JSON under `/var/lib/agnos/agent-profiles/{agent_id}.json`.
- **Detection**: After baseline period, each metric is compared to the baseline using a
  configurable z-score threshold (default 3.0). Deviation beyond the threshold triggers
  an `AnomalyDetected` event.
- **Anomaly types**: `UnusualSyscall`, `UnexpectedNetwork`, `FileAccessAnomaly`,
  `ResourceSpike`, `IpcPatternChange`.
- **Response policy** (configurable per agent):
  - `alert` (default) — emit event to audit log and pub/sub, no action
  - `restrict` — tighten sandbox (drop network, restrict filesystem) pending human review
  - `suspend` — pause agent, await manual approval to resume
- **No automatic termination** — the Human Sovereignty principle requires human decision
  for destructive actions.

### Mutual TLS (`agent-runtime/mtls.rs` + `agnos-sys/certpin.rs`)

- **Certificate authority**: AGNOS-internal CA, generated on first boot and sealed to TPM.
  CA key never leaves TPM-protected storage.
- **Per-agent certificates**: Each agent receives a certificate signed by the AGNOS CA at
  registration time. Certificate CN = agent ID, SAN = agent name.
- **Certificate lifecycle**:
  - Generated on `POST /v1/agents/register`
  - Stored in agent's encrypted storage volume
  - Default validity: 90 days
  - Automatic renewal 30 days before expiry
  - Revocation via CRL (Certificate Revocation List) distributed to all agents
- **Enforcement points**:
  - Agent -> LLM Gateway: mTLS required (agent presents cert, gateway validates against CA)
  - Agent -> Agent Runtime API: mTLS required
  - Agent -> Agent (RPC/pub/sub over UDS): mTLS optional (UDS already provides process identity
    via `SO_PEERCRED`; mTLS adds defense-in-depth)
- **Fallback**: If mTLS is disabled in dev profile (`Environment::Development`), connections
  fall back to UDS peer credential checking with a log warning.

### Secrets Rotation (`agnos-common/secrets.rs`)

- **Rotation schedule**: Configurable per secret type:
  ```toml
  [secrets.rotation]
  api_keys = "30d"
  tls_certificates = "90d"
  agent_tokens = "7d"
  ```
- **Rotation process**:
  1. Generate new secret value
  2. Store as pending (both old and new are valid during grace period)
  3. Notify dependent agents via pub/sub `SecretRotated { secret_id, grace_period }`
  4. After grace period (default 5 minutes), revoke old value
  5. Audit log entry for each rotation
- **Zero-downtime**: The dual-valid grace period ensures no agent is left with an invalid secret.
  Agents that fail to pick up the new secret within the grace period are flagged.
- **Manual trigger**: `POST /v1/secrets/{id}/rotate` for immediate rotation (e.g., after compromise).
- **Integration**: Builds on existing `SecretValue` type. Adds `RotationPolicy` and
  `RotationState` (Current, Pending, Revoked).

### Runtime Integrity Attestation (`agnos-sys/integrity.rs`)

- **What is measured**:
  - Agent binary hashes (compared to manifest `binary_hash` field)
  - Agent config file hashes
  - Sandbox policy files (Landlock rules, seccomp BPF)
  - Shared library hashes (for dynamically linked agents)
- **Measurement frequency**: Configurable (default every 5 minutes).
- **Measurement method**: IMA extended attributes where available, direct SHA-256 otherwise.
  Results compared against the TPM PCR-based boot-time measurements when TPM is available.
- **Attestation report**: `IntegrityReport { agent_id, timestamp, measurements: Vec<Measurement>, status }`.
  Status is `Verified`, `Modified`, or `Missing`.
- **On failure**: `IntegrityViolation` event emitted. Response policy same as anomaly detection
  (alert / restrict / suspend). Modified binary with `suspend` policy will halt the agent
  immediately.
- **Remote attestation**: Optional `GET /v1/agents/{id}/attestation` endpoint returns the latest
  integrity report, signed by the AGNOS CA. External verifiers can validate the signature.

## Consequences

### What becomes easier
- Detect compromised or misbehaving agents at runtime
- All inter-service communication is cryptographically authenticated
- Secrets management is automated, reducing human error and stale credentials
- Operators can prove agent integrity to external auditors

### What becomes harder
- Certificate management adds operational complexity (CA, CRL, renewal)
- Anomaly detection requires a baseline period; new agents have no baseline
- Secrets rotation grace period introduces a window where two values are valid
- Integrity measurement adds I/O load (hashing files every 5 minutes)

### Risks
- False positive anomalies during legitimate workload changes — mitigated by configurable
  z-score threshold and `alert`-only default policy. Operators can retrain baselines.
- CA key compromise would undermine all mTLS — mitigated by TPM sealing. If TPM is unavailable,
  CA key is encrypted at rest with a passphrase derived from boot entropy.
- Integrity check race condition (agent binary updated while being hashed) — mitigated by
  checking twice with a 1-second delay; both must match to pass.

## Alternatives Considered

### SPIFFE/SPIRE for agent identity
Rejected for Phase 6.8: SPIFFE is excellent but adds a SPIRE server dependency. AGNOS's
internal CA is simpler for single-node deployments. Phase 7 (federation) may adopt SPIFFE
for cross-node agent identity.

### eBPF for behavior monitoring instead of /proc sampling
Rejected: eBPF requires kernel 5.x+ features and adds kernel-space complexity. `/proc` sampling
combined with seccomp audit events provides sufficient granularity for anomaly detection.
eBPF is a candidate for Phase 8 (research).

### HashiCorp Vault for secrets rotation
Rejected: external dependency. AGNOS should manage its own secrets as an OS. The rotation
logic is straightforward and integrates directly with the existing `SecretValue` type.

## References

- Phase 6.8 roadmap: `docs/development/roadmap.md` (Security Hardening section)
- Existing security model: `docs/security/security-guide.md`
- Existing IMA/EVM: `userland/agnos-sys/src/ima.rs`
- Existing TPM: `userland/agnos-sys/src/tpm.rs`
- Existing secrets: `userland/agnos-common/src/secrets.rs`
- Existing certificate pinning: `userland/agnos-sys/src/certpin.rs`
- Zero-trust architecture: NIST SP 800-207
