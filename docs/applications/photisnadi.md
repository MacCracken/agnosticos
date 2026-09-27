# Photis Nadi

> **Photis Nadi** (Greek photis: light + Sanskrit nadi: path) — AI-native productivity app

| Field | Value |
|-------|-------|
| Status | Released |
| Version | 2026.3.18-1 |
| Repository | `MacCracken/PhotisNadi` |
| Runtime | native-binary (Rust — migrated from Flutter in 2026.3.18-1, per the zugot recipe) |
| Recipe | `zugot/marketplace/photisnadi.cyml` |
| MCP Tools | 8 `photis_*` |
| Agnoshi Intents | None (the current agnoshi has no per-app intents) |
| Port | N/A |

---

## Why First-Party

AI-native productivity requires tight integration between task management, scheduling, and local LLM inference for natural-language interaction. No existing productivity tool combines NL task creation, intelligent scheduling suggestions, and deep calendar integration while keeping all data local. Through 2026.3.18 a Flutter UI provided the cross-platform desktop shell with the aethersafha theme bridge; 2026.3.18-1 migrated the app to a native Rust binary.

## What It Does

- Task management with natural-language creation and prioritization
- Intelligent scheduling with conflict detection and time-block suggestions
- Deep calendar integration with NL event parsing
- Project and goal tracking with LLM-powered progress summaries
- Native desktop UI with AGNOS theme integration (Flutter until 2026.3.18-1)

## AGNOS Integration

- **Daimon**: Registers as an agent; uses memory store for persistent task data and audit APIs
- **Hoosh**: LLM-powered NL task parsing, scheduling suggestions, progress summaries, and priority recommendations
- **MCP Tools**: `photis_task`, `photis_schedule`, `photis_project`, `photis_summary`, `photis_priority`, `photis_search`, `photis_calendar`, `photis_remind`
- **Agnoshi Intents** (the Rust era's planned set — not in the current agnoshi): `photis add`, `photis list`, `photis schedule`, `photis project`, `photis summary`, `photis search`, `photis remind`, `photis calendar`
- **Marketplace**: Category: productivity. Sandboxed with user data directory access

## Architecture

- **Crates**: a Rust workspace since 2026.3.18-1 (the zugot recipe builds it with `cargo build --release --workspace`); through 2026.3.18 it was a Flutter application (Dart)
- **Dependencies**: runtime glibc, openssl, sqlite (per the zugot recipe). The Flutter line used flutter, dart, material_design, provider (state management)

## Roadmap

Stable — maintenance mode. Future considerations: collaborative task sharing, Kanban board view, recurring task templates.
