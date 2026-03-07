# ADR-019: Sigil — System-Wide Trust Verification

**Status:** Accepted
**Date:** 2026-03-07

## Context

AGNOS has trust primitives scattered across modules:
- `marketplace/trust.rs` — Ed25519 signing for marketplace packages
- `marketplace/transparency.rs` — append-only publish log
- `integrity.rs` — SHA-256 file hash verification
- `agnos-sys/ima.rs` — IMA/EVM kernel integrity
- `agnos-sys/tpm.rs` — TPM measured boot

These need to be unified into a single trust chain from boot to runtime. Without this, trust verification is ad-hoc and inconsistent — some packages are signed, some aren't; agent binaries aren't verified before execution; there's no revocation mechanism.

## Decision

Create `sigil` — a system-wide trust verification module that:

### Trust Levels
Five levels (highest to lowest):
- **SystemCore** — kernel, init, core AGNOS binaries (signed by AGNOS project key)
- **Verified** — signed by a trusted publisher with valid key
- **Community** — signed but by an unverified publisher
- **Unverified** — no signature present
- **Revoked** — explicitly revoked (compromised, malicious, or withdrawn)

### Trust Policy
Configurable enforcement modes:
- **Strict** — only Verified or SystemCore artifacts can run/install
- **Permissive** — warns on unverified but allows execution
- **AuditOnly** — logs trust status but never blocks

### Verification Chain
```
Boot:     TPM PCR → sigil verifies boot components → argonaut starts
Install:  ark download → sigil verifies .ark signature + hash → install
Execute:  agent spawn → sigil verifies binary hash + signature → launch
Runtime:  periodic integrity check via IntegrityVerifier → alert on mismatch
```

### Integration Points
- **ark** — calls `sigil.verify_package()` before installing
- **agent-runtime** — calls `sigil.verify_agent_binary()` before spawning
- **argonaut** (future) — calls `sigil.verify_boot_chain()` at boot
- **aegis** (future) — monitors runtime integrity via sigil
- **marketplace** — existing trust.rs and transparency.rs become sigil's foundation

### Revocation
- `RevocationList` maintained locally, synced from `packages.agnos.org/revocations.json`
- Revoke by key_id (compromised publisher) or content_hash (specific bad artifact)
- Revocation is checked on every verify operation

### Architecture
```
sigil.rs (SigilVerifier)
  ├── PublisherKeyring (from marketplace/trust.rs)
  ├── TransparencyLog (from marketplace/transparency.rs)
  ├── IntegrityVerifier (from integrity.rs)
  ├── RevocationList (new)
  ├── TrustPolicy (new)
  └── TrustStore (HashMap<hash, TrustedArtifact>)
```

## Consequences

### Positive
- Single source of truth for all trust decisions
- Every artifact has a clear trust level
- Revocation provides rapid response to compromised packages/keys
- Configurable policy allows different deployment contexts (dev vs production)
- Builds on existing code — no rewrite needed

### Negative
- All package/agent operations now have a verification step (small latency cost)
- Revocation list must be distributed and kept current
- Strict mode may frustrate development (use AuditOnly for dev)

### Mitigations
- Cache verification results (TrustStore) — only re-verify on file change
- RevocationList sync is async background task
- Development profile defaults to AuditOnly enforcement

## Alternatives Considered

### Keep trust ad-hoc per module
Rejected. Inconsistent trust decisions across the system. No unified revocation.

### Use Notary/TUF (The Update Framework)
Rejected for now. TUF is designed for software repositories, not OS-level trust chains. May adopt TUF concepts for package repo in Phase 8C.

## Related
- ADR-018: LFS-Native Distribution (sigil signs .ark packages)
- ADR-015: Agent Marketplace (trust.rs is sigil's foundation)
- ADR-013: Zero-Trust Security Hardening
