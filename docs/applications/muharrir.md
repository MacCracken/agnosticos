# Muharrir

> **Muharrir** (Arabic: editor/writer) — Shared editor primitives

| Field | Value |
|-------|-------|
| Status | Released |
| Version | `0.23.5` |
| Repository | `MacCracken/muharrir` |
| Runtime | library crate (Rust) |
| Recipe | `recipes/marketplace/muharrir.toml` |
| crates.io | N/A (not yet published) |

---

## What It Does

- Text buffer with rope data structure for efficient editing of large documents
- Undo/redo with operation-based history (not snapshot-based)
- Syntax highlighting with tree-sitter grammar support
- Document model abstraction supporting text, image, audio, and video content
- Selection and cursor management with multi-cursor support

## Consumers

- **rasa** — Image editor (text overlays, metadata editing)
- **tazama** — Video editor (subtitle editing, project notes)
- **shruti** — DAW (lyric editing, session notes)
- All AGNOS creative applications needing editor primitives

## Architecture

- Rope-based text buffer with O(log n) insert/delete
- Plugin-friendly: highlight and completion providers are trait objects
- Dependencies: serde, tree-sitter (optional)

## Roadmap

Pre-release — available but not yet published on crates.io. Future: collaborative editing (CRDT), LSP client integration, RTL/bidirectional text support.
