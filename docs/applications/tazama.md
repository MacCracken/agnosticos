# Tazama

> **Tazama** (Swahili: to watch) — AI-native video editor

| Field | Value |
|-------|-------|
| Status | Released |
| Version | `2026.3.14` |
| Repository | `MacCracken/tazama` |
| Runtime | native-binary (~5.7MB) |
| Recipe | `zugot/marketplace/tazama.cyml` |
| MCP Tools | 7 `tazama_*` |
| Agnoshi Intents | None (the current agnoshi has no per-app intents) |
| Port | N/A (desktop app) |

---

## Why First-Party

AI-native video editing with automatic scene detection, intelligent auto-cut, and content-aware transitions are not available as native features in any existing open-source editor. Tazama is built on tarang for codec support and integrates with hoosh for LLM-driven editing decisions, enabling workflows like "cut to the beat" or "remove all silent segments" through natural language.

## What It Does

- Timeline-based non-linear video editing with multi-track support
- AI-powered scene detection and automatic cut point suggestion
- Content-aware transitions and effect recommendations
- NL-driven editing commands ("trim the intro", "add a crossfade here")
- Export to multiple formats via tarang encode pipeline

## AGNOS Integration

- **Daimon**: Registers as an agent; indexes project assets into RAG; uses vector store for clip similarity search
- **Hoosh**: LLM-driven edit suggestions, scene classification, content description for accessibility
- **MCP Tools**: `tazama_import`, `tazama_cut`, `tazama_effect`, `tazama_export`, `tazama_timeline`, `tazama_analyze`, `tazama_generate`
- **Agnoshi Intents** (the Rust era's planned set — not in the current agnoshi): `tazama import <file>`, `tazama cut <params>`, `tazama effect <name>`, `tazama export <format>`, `tazama timeline <action>`, `tazama analyze <clip>`, `tazama generate <description>`
- **Marketplace**: Video/Creative category; sandbox profile allows GPU access, read-write project directories, PipeWire socket for audio preview

## Architecture

- **Crates**:
  - `tazama-core` — timeline engine, clip management, project state
  - `tazama-edit` — cut, trim, splice operations, transition engine
  - `tazama-effects` — video filters, color grading, motion graphics
  - `tazama-ai` — scene detection, auto-cut, content-aware features, daimon/hoosh integration
  - `tazama-ui` — desktop GUI, timeline view, preview player, inspector panels
- **Dependencies**: tarang (media decode/encode), wgpu (GPU rendering), SQLite (project database)

## Roadmap

- Motion tracking and stabilization
- Multi-camera editing workflow
- Collaborative editing via federation
- AI voice-over generation via hoosh
