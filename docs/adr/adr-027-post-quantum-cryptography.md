# ADR-027: Post-Quantum Cryptography

**Status:** Accepted

**Date:** 2026-03-07

**Authors:** AGNOS Team

## Context

Quantum computers capable of breaking RSA, ECDSA, and X25519 are projected within
10-15 years. NIST standardized post-quantum algorithms in 2024:
- **FIPS 203 (ML-KEM)**: Module-Lattice-Based Key-Encapsulation Mechanism (formerly CRYSTALS-Kyber)
- **FIPS 204 (ML-DSA)**: Module-Lattice-Based Digital Signature Algorithm (formerly CRYSTALS-Dilithium)

AGNOS uses Ed25519 (sigil.rs) for artifact signing and X25519 (mTLS) for key exchange.
These must be augmented with PQC algorithms before quantum computers arrive — "harvest
now, decrypt later" attacks mean data encrypted today is already at risk.

## Decision

### Hybrid Approach

AGNOS adopts **hybrid cryptography**: every operation uses both a classical algorithm
and a PQC algorithm. Security depends on the stronger of the two.

| Operation | Classical | Post-Quantum | Combination |
|-----------|-----------|-------------|-------------|
| Key Exchange | X25519 | ML-KEM-768/1024 | SHA-256(classical_ss ‖ pqc_ss) |
| Signatures | Ed25519 | ML-DSA-44/65/87 | Both must verify (AND logic) |

### Algorithm Selection

| Algorithm | NIST Level | Use Case |
|-----------|-----------|----------|
| ML-KEM-768 | 3 (default) | Agent-to-agent, federation |
| ML-KEM-1024 | 5 | High-security contexts |
| ML-DSA-65 | 3 (default) | Artifact signing, trust verification |
| ML-DSA-87 | 5 | System-critical signatures |

### Migration Path

Three modes via `PqcConfig`:
1. **Disabled** — classical only (current behavior preserved)
2. **Hybrid** — both classical + PQC (default for new installations)
3. **PqcOnly** — PQC only (future, when classical is deprecated)

`PqcMigrationStatus` tracks transition progress per agent and federation node.

### Implementation Strategy

The module defines correct type signatures and interfaces with simulated PQC
operations (SHA-256 based). All simulation is isolated in 6 `sim_*` functions.
When Rust `ml-kem` and `ml-dsa` crates reach stable, swapping requires changing
only these functions — no API changes.

### Integration Points

- **sigil.rs**: Hybrid signatures for artifact verification
- **mtls.rs**: Hybrid key exchange for federation and agent-to-agent TLS
- **aegis.rs**: PQC migration compliance monitoring
- **federation.rs**: PQC-aware node authentication

## Consequences

### What becomes easier
- AGNOS is quantum-resistant before quantum computers are practical
- Hybrid approach means no security regression if PQC algorithms have undiscovered weaknesses
- Gradual migration without breaking existing agents

### What becomes harder
- PQC keys are larger (ML-KEM-768 public key: 1184 bytes vs X25519: 32 bytes)
- PQC signatures are larger (ML-DSA-65: 3309 bytes vs Ed25519: 64 bytes)
- Key exchange and signing are slower (~3-10x for KEM, ~2-5x for signatures)

## References

- NIST FIPS 203: ML-KEM (Module-Lattice-Based Key-Encapsulation Mechanism)
- NIST FIPS 204: ML-DSA (Module-Lattice-Based Digital Signature Algorithm)
- Hybrid key exchange: draft-ietf-tls-hybrid-design
- ADR-019: Sigil Trust System (Ed25519 signing)
- ADR-013: Zero-Trust Security Hardening (mTLS)
