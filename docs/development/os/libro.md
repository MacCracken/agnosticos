# Libro

> **Libro** (Italian/Spanish: book/record) — Cryptographic audit chain

| Field | Value |
|-------|-------|
| Status | Released — pre-1.0 hardening |
| Version | `0.90.0` |
| Repository | `MacCracken/libro` |
| Runtime | library crate (Rust) |
| Recipe | `recipes/marketplace/libro.toml` |
| crates.io | [libro](https://crates.io/crates/libro) |

---

## What It Does

- Tamper-proof event logging with hash-linked chain entries
- **SHA-256 and BLAKE3** hash algorithms (extensible for future additions)
- Cryptographic verification of log integrity (detect insertions, deletions, modifications)
- Structured audit events with timestamps, actor IDs, and action metadata
- Chain export and import for compliance archival
- Efficient append-only storage with configurable backends
- Only local-first audit chain crate on crates.io

## Consumers

- **daimon** — Agent orchestrator (agent audit trail)
- **aegis** — Security daemon (security event chain)
- **sigil** — Trust verification (signing audit log)
- **t-ron** — MCP security (tool call audit trail)
- **stiva** — Container runtime (container lifecycle audit)

## Architecture

- Hash chain with each entry linking to its predecessor (SHA-256 or BLAKE3)
- Append-only log with optional rotation and compaction
- Extensible `HashAlgorithm` enum for future hash types
