# T-Ron

> **T-Ron** (Tron: the security program that fights MCP) — MCP security monitor

| Field | Value |
|-------|-------|
| Status | Released |
| Version | `0.22.4` |
| Repository | `MacCracken/t-ron` |
| Runtime | library crate (Rust) |
| Recipe | `recipes/marketplace/t-ron.toml` |
| crates.io | N/A (not yet published) |

---

## What It Does

- Tool call auditing with full request/response logging via libro
- Rate limiting per agent, per tool, and per time window
- Injection detection for prompt injection and tool argument manipulation
- Anomaly analysis on tool call patterns (frequency, sequence, parameter drift)
- Middleware layer that wraps bote tool dispatch with security checks

## Consumers

- **daimon** — Agent orchestrator (MCP security layer)
- **bote** — MCP core service (t-ron wraps bote dispatch)
- Complements phylax (phylax scans outputs/files, t-ron guards MCP inputs)

## Architecture

- Middleware architecture: intercepts tool calls before and after execution
- Rule engine with configurable policies per tool and agent
- Dependencies: tokio, serde, bote, libro

## Roadmap

Pre-release — scaffolded at v0.1.0. Future: ML-based anomaly baseline learning, real-time alert dashboard, cross-agent correlation analysis.
