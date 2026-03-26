# Szal

> **Szal** (Hungarian: thread/fiber) — Workflow engine

| Field | Value |
|-------|-------|
| Status | Released |
| Version | `0.23.4` |
| Repository | `MacCracken/szal` |
| Runtime | library crate (Rust) |
| Recipe | `recipes/marketplace/szal.toml` |
| crates.io | N/A (not yet published) |

---

## What It Does

- Step and flow execution with typed inputs/outputs between stages
- Branching and conditional logic within workflow definitions
- Retry policies with exponential backoff and circuit breaking
- Rollback support for failed stages (compensating actions)
- Parallel stage execution with join semantics

## Consumers

- **daimon** — Agent orchestrator (agent workflow pipelines)
- **sutra** — Infrastructure orchestrator (playbook execution engine)

## Architecture

- Workflow DAG with topological execution order
- Each step is a trait object with execute/rollback methods
- Dependencies: tokio, serde, thiserror

## Roadmap

Pre-release — available but not yet published on crates.io. Future: visual workflow editor integration, workflow versioning, long-running workflow persistence.
