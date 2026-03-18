# Photis Nadi

> **Photis Nadi** (Greek photis: light + Sanskrit nadi: path) — AI-native productivity app

| Field | Value |
|-------|-------|
| Status | Released |
| Version | Latest GitHub release |
| Repository | `MacCracken/PhotisNadi` |
| Runtime | flutter (~20MB) |
| Recipe | `recipes/marketplace/photisnadi.toml` |
| MCP Tools | 8 `photis_*` |
| Agnoshi Intents | 8 |
| Port | N/A |

---

## Why First-Party

AI-native productivity requires tight integration between task management, scheduling, and local LLM inference for natural-language interaction. No existing productivity tool combines NL task creation, intelligent scheduling suggestions, and deep calendar integration while keeping all data local. Flutter provides a polished cross-platform desktop UI with the aethersafha theme bridge.

## What It Does

- Task management with natural-language creation and prioritization
- Intelligent scheduling with conflict detection and time-block suggestions
- Deep calendar integration with NL event parsing
- Project and goal tracking with LLM-powered progress summaries
- Cross-platform Flutter desktop UI with AGNOS theme integration

## AGNOS Integration

- **Daimon**: Registers as a Flutter agent; uses memory store for persistent task data and audit APIs
- **Hoosh**: LLM-powered NL task parsing, scheduling suggestions, progress summaries, and priority recommendations
- **MCP Tools**: `photis_task`, `photis_schedule`, `photis_project`, `photis_summary`, `photis_priority`, `photis_search`, `photis_calendar`, `photis_remind`
- **Agnoshi Intents**: `photis add`, `photis list`, `photis schedule`, `photis project`, `photis summary`, `photis search`, `photis remind`, `photis calendar`
- **Marketplace**: Category: productivity. Sandboxed with user data directory access

## Architecture

- **Crates**: Flutter application (Dart, not Rust crates)
- **Dependencies**: flutter, dart, material_design, provider (state management)

## Roadmap

Stable — maintenance mode. Future considerations: collaborative task sharing, Kanban board view, recurring task templates.
