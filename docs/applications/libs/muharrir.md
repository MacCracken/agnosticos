# Muharrir

> **Muharrir** (Arabic: editor) — Shared editor primitives for creative applications

| Field | Value |
|-------|-------|
| Status | Pre-1.0 |
| Version | `0.23.5` |
| Repository | `MacCracken/muharrir` |
| Runtime | library crate (Rust) |

---

## What It Does

- Undo/redo history with tamper-evident audit chain (via libro)
- Command pattern with undo/redo stacks and compound commands
- Math expression evaluation for property fields (via abaco)
- Hardware detection and quality tier classification (via ai-hwaccel)
- Generic parent-child hierarchy tree building and flattening
- Property sheet inspector for editor panels
- Toast notifications and persistent notification log
- Generic selection tracking and panel visibility management
- Modified/dirty state tracking with save-point support
- Recent files list with configurable cap and persistence
- Preferences storage with JSON I/O
- All modules feature-gated: `history`, `expr`, `hw`, `hierarchy`, `inspector`, `command`, `notification`, `selection`, `dirty`, `recent`, `prefs`

## Consumers

- **salai** — game editor (scene hierarchy, undo/redo, inspector panels)
- **rasa** — image editor (undo/redo, tool selection, property sheets)
- **tazama** — video editor (timeline undo/redo, effect inspector)
- **shruti** — DAW (track hierarchy, undo/redo, mixer inspector)
