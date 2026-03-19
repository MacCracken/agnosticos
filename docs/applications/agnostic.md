# Agnostic

> **Agnostic** — AI-native agent automation platform powered by CrewAI

| Field | Value |
|-------|-------|
| Status | Released |
| Version | Latest GitHub release |
| Repository | `MacCracken/agnostic` |
| Runtime | python-container (~472KB) |
| Recipe | `recipes/marketplace/agnostic.toml` |
| MCP Tools | 5 `agnostic_*` |
| Agnoshi Intents | 5 |
| Port | N/A |

---

## Why First-Party

AI-native agent automation requires deep integration with the daimon agent lifecycle — multi-domain crew orchestration (quality, security, performance, data-engineering, devops, design, software-engineering) needs OS-level sandboxing and orchestration. No existing agent framework combines multi-agent crews with local LLM inference, AGNOS trust verification, and A2A delegation. Agnostic demonstrates the Python container runtime for marketplace apps.

## What It Does

- Multi-domain agent crews with 7 domain presets (quality, security, performance, data-engineering, devops, design, software-engineering)
- Crew orchestration with automatic task delegation and A2A handoff
- NL-driven crew creation and execution via agnoshi
- GPU-aware crew placement with VRAM budgets
- Structured reporting with confidence scores, dashboards, and HUD widgets

## AGNOS Integration

- **Daimon**: Registers as a Python container agent; uses agent lifecycle, sandbox, and audit APIs
- **Hoosh**: LLM inference for test generation, coverage analysis, and result interpretation
- **MCP Tools**: `agnostic_generate`, `agnostic_run`, `agnostic_report`, `agnostic_coverage`, `agnostic_analyze`
- **Agnoshi Intents**: `agnostic test`, `agnostic generate`, `agnostic coverage`, `agnostic report`, `agnostic analyze`
- **Marketplace**: Category: agent-automation. Sandboxed Python container with limited filesystem access

## Architecture

- **Crates**: Single Python package using CrewAI framework
- **Dependencies**: crewai, pydantic, httpx (daimon client), pytest

## Roadmap

Stable — maintenance mode. Watching crewAI 1.11.0 RC for Docker-mandatory CodeInterpreterTool changes and A2A auth updates.
