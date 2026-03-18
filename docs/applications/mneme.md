# Mneme

> **Mneme** (Greek: memory) — AI-native knowledge base

| Field | Value |
|-------|-------|
| Status | Released |
| Version | `2026.3.13` |
| Repository | `MacCracken/mneme` |
| Runtime | native-binary (~17MB amd64, ~16MB arm64) |
| Recipe | `recipes/marketplace/mneme.toml` |
| MCP Tools | 7 `mneme_*` |
| Agnoshi Intents | 7 |
| Port | N/A (desktop app + CLI) |

---

## Why First-Party

Mneme leverages daimon's RAG and vector store APIs natively, providing semantic search and LLM-powered Q&A over personal knowledge without any external service dependency. No existing knowledge base tool (Obsidian, Notion, Logseq) offers local-first semantic search backed by a vector store and LLM question answering as built-in features.

## What It Does

- Note-taking and document management with rich text and markdown support
- Semantic search across all notes using daimon's vector store
- LLM-powered Q&A: ask questions and get answers grounded in your knowledge base
- Bidirectional linking between notes with automatic relationship discovery
- Import/export in multiple formats (markdown, JSON, HTML, PDF)

## AGNOS Integration

- **Daimon**: Registers as an agent; ingests all notes into RAG pipeline; uses vector store for semantic search; subscribes to knowledge update events
- **Hoosh**: LLM Q&A over knowledge base, automatic tagging, summarization, relationship suggestion
- **MCP Tools**: `mneme_notebook`, `mneme_search`, `mneme_ai`, `mneme_tag`, `mneme_export`, `mneme_import`, `mneme_link`
- **Agnoshi Intents**: `mneme search <query>`, `mneme ask <question>`, `mneme note <action>`, `mneme tag <action>`, `mneme export <format>`, `mneme import <file>`, `mneme link <action>`
- **Marketplace**: Productivity/Knowledge category; sandbox profile allows read-write document directories, network for sync

## Architecture

- **Crates**:
  - `mneme-core` — note model, metadata, bidirectional links
  - `mneme-store` — SQLite storage, full-text search index
  - `mneme-search` — semantic search via daimon vector store, hybrid ranking
  - `mneme-ai` — LLM Q&A, auto-tagging, summarization, relationship discovery
  - `mneme-api` — REST API for programmatic access
  - `mneme-ui` — desktop GUI, editor, graph view
  - `mneme-mcp` — MCP tool definitions and handlers
  - `mneme-io` — import/export: markdown, JSON, HTML, PDF
- **Dependencies**: SQLite (storage), daimon vector store API, hoosh LLM API

## Roadmap

- Real-time collaboration via federation
- Spaced repetition system for review
- Handwriting/OCR ingestion via selah integration
- Knowledge graph visualization improvements
