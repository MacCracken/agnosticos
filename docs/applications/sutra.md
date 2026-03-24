# Sutra

> **Sutra** (Sanskrit: sūtra — thread, rule, formula) — AI-native infrastructure orchestrator

| Field | Value |
|-------|-------|
| Status | Released |
| Version | `2026.3.18` |
| Repository | `MacCracken/sutra` |
| Runtime | native-binary (Rust) |
| Recipe | `recipes/marketplace/sutra.toml` |
| MCP Tools | 6 `sutra_*` |
| Agnoshi Intents | 6 |
| Port | N/A (CLI tool) |

---

## Why First-Party

No Rust-based orchestrator exists at Ansible's level. Existing tools (Ansible, pyinfra, Salt) are Python-based and bring a heavy runtime dependency. Sutra fills the gap between daimon's agent lifecycle management and full infrastructure-as-code, treating AGNOS subsystems (ark, argonaut, aegis, daimon) as first-class modules rather than shelling out to system commands.

## What It Does

- YAML canonical playbooks with TOML/Markdown/NL input formats
- Dry-run-by-default — `sutra apply` shows a diff, `sutra apply --confirm` executes
- 6 core modules: ark, argonaut, file, shell, user, verify
- Local + SSH + daimon transport (fleet-aware)
- Tera templates, variables/facts, task dependencies, parallel execution
- JSON output, audit trail
- **sutra-community** (5 additional modules: nftables, sysctl, aegis, daimon, edge)

## AGNOS Integration

- **Daimon**: Agent registration, fleet inventory via `/v1/edge/nodes`, audit reporting
- **Hoosh**: NL-to-TOML playbook translation, Markdown-to-TOML parsing, drift explanation
- **MCP Tools**: `sutra_apply`, `sutra_plan`, `sutra_check`, `sutra_inventory`, `sutra_translate`, `sutra_convert`
- **Agnoshi Intents**: deploy, check fleet, translate playbook, show inventory, rollback, convert format
- **Marketplace**: Category: infrastructure. No network access required for local mode

## Architecture

- **Crates**: core (parser, task graph, execution engine), modules (built-in module implementations), transport (SSH, daimon HTTP, local), ai (NL/Markdown translation via hoosh), mcp
- **Dependencies**: tokio, serde, toml, reqwest, russh, comrak, tera, tracing

## Roadmap

Stable — 70 tests passing. Future considerations: custom module loading (dynamic .so or WASM), playbook composition (include/import), drift detection with auto-remediation.
