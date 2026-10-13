# Agnostic

> **Agnostic** — AI-native agent automation platform. Built on CrewAI through its Python line; now being ported to Cyrius on the AgnosAI engine (pre-1.0)

| Field | Value |
|-------|-------|
| Status | Released — the Cyrius port is pre-1.0 (its milestones ship together as 1.0.0); the Python/CrewAI line (CalVer, last `2026.3.18`) is kept in `python-port/` as its parity oracle |
| Version | See [`VERSION`](https://github.com/MacCracken/agnostic/blob/main/VERSION) |
| Repository | `MacCracken/agnostic` |
| Runtime | native-binary (Cyrius HTTP server). The Python line ran as a python-container |
| Recipe | `zugot/marketplace/agnostic.cyml` — still packages the Python line's `2026.3.18` |
| MCP Tools | None in the Cyrius port yet (the `agnostic_*` tools below are the Python line's) |
| Agnoshi Intents | None (the current agnoshi has no per-app intents) |
| Port | 8000 by default (`AGNOSTIC_PORT` / `PORT` override — `agnostic/src/config.cyr:52,214-216`) |

---

## Why First-Party

AI-native agent automation requires deep integration with the daimon agent lifecycle — multi-domain crew orchestration (quality, security, performance, data-engineering, devops, design, software-engineering) needs OS-level sandboxing and orchestration. No existing agent framework combines multi-agent crews with local LLM inference, AGNOS trust verification, and A2A delegation. The Python line also demonstrated the Python container runtime for marketplace apps; the Cyrius port retires that runtime for Agnostic itself.

## What It Does

The Python line's feature set — the Cyrius port re-derives it on AgnosAI; the repo's `handoff.md` records what has landed:

- Multi-domain agent crews with 7 domain presets (quality, security, performance, data-engineering, devops, design, software-engineering)
- Crew orchestration with automatic task delegation and A2A handoff
- NL-driven crew creation and execution via agnoshi
- GPU-aware crew placement with VRAM budgets
- Structured reporting with confidence scores, dashboards, and HUD widgets

## AGNOS Integration

- **Daimon**: the Python line registered as a Python container agent (agent lifecycle, sandbox, audit APIs); the Cyrius port defers daimon registration — repo ADR 0002: daimon has no agent heartbeat route yet
- **AgnosAI**: the Cyrius port's engine tier — crews, scheduling, LLM routing through hoosh, the tool registry and its sandbox tiers, the audit chain
- **Hoosh**: LLM inference for test generation, coverage analysis, and result interpretation
- **MCP Tools** (Python line): `agnostic_generate`, `agnostic_run`, `agnostic_report`, `agnostic_coverage`, `agnostic_analyze`
- **Agnoshi Intents** (the Rust era's planned set — not in the current agnoshi): `agnostic test`, `agnostic generate`, `agnostic coverage`, `agnostic report`, `agnostic analyze`
- **Marketplace**: Category: agent-automation. Sandboxed Python container with limited filesystem access

## Architecture

- **Cyrius port** (`src/`): `engine` (crews, definitions, presets, outcomes, audit, stores), `http` + `routes` + `server` (the REST API), `auth` (users, API keys, HS256 tokens, role → permission table, tenancy, webhook signatures)
- **Dependencies**: agnosai (engine), bote, majra, ai-hwaccel, tyche, patra (persistence), sigil (Argon2id, SHA-256, HMAC), sandhi (HTTP), plus the Cyrius stdlib — see the repo's `cyrius.cyml`
- **Python line** (`python-port/`, the parity oracle): a single Python package on CrewAI — crewai, pydantic, httpx (daimon client), pytest

## Roadmap

The Cyrius port's milestones ship together as **1.0.0**; live status is the repo's [`docs/development/handoff.md`](https://github.com/MacCracken/agnostic/blob/main/docs/development/handoff.md) and `state.md`. *Through the Python line* the plan was maintenance mode, watching crewAI 1.11.0 RC for Docker-mandatory CodeInterpreterTool changes and A2A auth updates.
