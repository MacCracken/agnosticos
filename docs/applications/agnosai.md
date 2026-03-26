# AgnosAI

> **AgnosAI** (AGNOS + AI) — Provider-agnostic AI orchestration framework

| Field | Value |
|-------|-------|
| Status | Released |
| Version | `0.25.3` |
| Repository | `MacCracken/agnosai` |
| Runtime | library crate (Rust) |
| Recipe | `recipes/marketplace/agnosai.toml` |
| crates.io | [agnosai](https://crates.io/crates/agnosai) |

---

## What It Does

- Crew management with role-based agent assignment and lifecycle
- Task DAG execution with dependency resolution and parallel stages
- Tool execution framework with sandboxed invocation
- Fleet distribution for spreading crews across nodes
- CrewAI replacement written in pure Rust for the AGNOS ecosystem

## Consumers

- **agnostic** — Python/CrewAI agent automation platform (primary consumer)
- **daimon** — Agent orchestrator (crew scheduling)
- **joshua** — Game manager (NPC crew behavior and AI teams)

## Architecture

- Crew/Task/Tool abstraction layers with trait-based extensibility
- DAG scheduler with topological sort and cycle detection
- Dependencies: tokio, serde, hoosh (LLM calls)

## Roadmap

Stable — published on crates.io. Future: streaming task results, crew checkpointing and resume, cost estimation per crew run.
