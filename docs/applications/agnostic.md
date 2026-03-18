# AGNOSTIC

> **AGNOSTIC** — AI-native QA automation platform powered by CrewAI

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

AI-native QA automation requires deep integration with the daimon agent lifecycle — crew-based test generation, execution, and reporting all need OS-level sandboxing and orchestration. No existing QA framework combines multi-agent crews with local LLM inference and AGNOS trust verification. AGNOSTIC demonstrates the Python container runtime for marketplace apps.

## What It Does

- Multi-agent QA crews that generate, execute, and evaluate test suites
- Automatic test case generation from natural-language specifications
- Coverage analysis and gap detection via LLM reasoning
- Integration testing against daimon-managed agents
- Structured reporting with confidence scores and actionable suggestions

## AGNOS Integration

- **Daimon**: Registers as a Python container agent; uses agent lifecycle, sandbox, and audit APIs
- **Hoosh**: LLM inference for test generation, coverage analysis, and result interpretation
- **MCP Tools**: `agnostic_generate`, `agnostic_run`, `agnostic_report`, `agnostic_coverage`, `agnostic_analyze`
- **Agnoshi Intents**: `agnostic test`, `agnostic generate`, `agnostic coverage`, `agnostic report`, `agnostic analyze`
- **Marketplace**: Category: development/testing. Sandboxed Python container with limited filesystem access

## Architecture

- **Crates**: Single Python package using CrewAI framework
- **Dependencies**: crewai, pydantic, httpx (daimon client), pytest

## Roadmap

Stable — maintenance mode. Watching crewAI 1.11.0 RC for Docker-mandatory CodeInterpreterTool changes and A2A auth updates.
