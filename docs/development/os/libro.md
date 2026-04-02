# Libro

> **Libro** (Latin: book/record) — Cryptographic audit chain

| Field | Value |
|-------|-------|
| Status | Released |
| Version | `0.25.3` |
| Repository | `MacCracken/libro` |
| Runtime | library crate (Rust) |
| Recipe | `recipes/marketplace/libro.toml` |
| crates.io | [libro](https://crates.io/crates/libro) |

---

## What It Does

- Tamper-proof event logging with hash-linked chain entries
- Cryptographic verification of log integrity (detect insertions, deletions, modifications)
- Structured audit events with timestamps, actor IDs, and action metadata
- Chain export and import for compliance archival
- Efficient append-only storage with configurable backends

## Consumers

- **daimon** — Agent orchestrator (agent audit trail in `/var/log/agnos/audit.log`)
- **aegis** — Security daemon (security event chain)
- All compliance-critical AGNOS applications

## Architecture

- SHA-256 hash chain with each entry linking to its predecessor
- Append-only log with optional rotation and compaction
- Dependencies: serde, sha2, chrono

## Roadmap

Stable — published on crates.io. Future: Merkle tree indexing for fast range verification, remote attestation support, SIEM export format.
