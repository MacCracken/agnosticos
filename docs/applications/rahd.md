# Rahd

> **Rahd** (Persian ruznam ahd: daily record + Arabic: appointment) — AI-native calendar and contacts

| Field | Value |
|-------|-------|
| Status | Released |
| Version | Latest GitHub release |
| Repository | `MacCracken/rahd` |
| Runtime | native-binary (Rust) |
| Recipe | `recipes/marketplace/rahd.toml` |
| MCP Tools | 5 `rahd_*` |
| Agnoshi Intents | 5 |
| Port | N/A |

---

## Why First-Party

Calendar and contacts are core PIM functionality that benefits enormously from local LLM integration. Rahd parses natural-language event descriptions ("lunch with Alex next Thursday at noon"), detects scheduling conflicts, finds free time slots, and suggests optimal meeting times — all via hoosh with no cloud dependency. Deep integration with daimon enables cross-agent scheduling and reminders.

## What It Does

- Natural-language event creation and modification ("move my 3pm to Friday")
- Automatic conflict detection with resolution suggestions
- Free slot finder with configurable working hours and preferences
- Contact management with relationship context for scheduling suggestions
- SQLite-backed local storage with CalDAV import/export

## AGNOS Integration

- **Daimon**: Registers as an agent; uses memory store for contact data and audit APIs for event changes
- **Hoosh**: LLM inference for NL event parsing, conflict resolution suggestions, and intelligent scheduling recommendations
- **MCP Tools**: `rahd_event`, `rahd_schedule`, `rahd_contacts`, `rahd_free`, `rahd_remind`
- **Agnoshi Intents**: `rahd add`, `rahd schedule`, `rahd contacts`, `rahd free`, `rahd remind`
- **Marketplace**: Category: productivity/calendar. Sandboxed with user data directory access

## Architecture

- **Crates**: core, store (SQLite), schedule (conflict detection, free slots), ai (NL parsing), mcp
- **Dependencies**: rusqlite, chrono, serde, tokio, reqwest (hoosh client)

## Roadmap

Stable — 49 tests passing. Future considerations: recurring event patterns, timezone-aware scheduling, CalDAV sync with external servers.
