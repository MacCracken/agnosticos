# Phylax

> **Phylax** (Greek: guardian/watchman) — AI-native threat detection engine

| Field | Value |
|-------|-------|
| Status | Released |
| Version | `0.22.3` |
| Repository | `MacCracken/phylax` |
| Runtime | library crate (Rust) |
| Recipe | `recipes/marketplace/phylax.toml` |
| crates.io | N/A (not yet published) |

---

## What It Does

- YARA rule engine for signature-based threat matching
- Entropy analysis to detect packed, encrypted, or obfuscated binaries
- Magic bytes identification for file type verification regardless of extension
- ML-based binary classification for unknown threat detection
- fanotify real-time filesystem scanning with LLM triage via hoosh

## Consumers

- **daimon** — Agent orchestrator (scanning agent outputs and file writes)
- **aegis** — Security daemon (integrated threat response)
- Complements t-ron (t-ron guards MCP inputs, phylax scans outputs/files)

## Architecture

- 5 crates: core (engine), rules (YARA), ml (classifier), scan (fanotify), mcp (tool interface)
- Scan pipeline: magic bytes -> entropy -> YARA -> ML -> LLM triage
- Dependencies: tokio, serde, hoosh (LLM triage)

## Roadmap

Pre-release — scaffolded at v0.1.0, core 15A done. Future: ClamAV signature import, network traffic scanning, threat intelligence feed ingestion.
