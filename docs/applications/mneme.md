# Mneme

> **Mneme** (Greek: memory) — AI-native knowledge base, in Cyrius (a port of the original Rust workspace, frozen at `rust-old/` as its parity oracle)

| Field | Value |
|-------|-------|
| Status | Released |
| Version | See [`VERSION`](https://github.com/MacCracken/mneme/blob/main/VERSION) |
| Repository | `MacCracken/mneme` |
| Runtime | native-binary (Cyrius) — terminal UI, plus `mneme serve` |
| Recipe | `zugot/marketplace/mneme.cyml` |
| MCP Tools | `mneme_*`, served by `mneme serve` |
| Agnoshi Intents | None (the current agnoshi has no per-app intents) |
| Port | 8100 — `mneme serve [port]`'s default (MCP endpoint, 1.1.0); the TUI needs none |

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

- **Daimon**: Registers as an agent; ingests all notes into RAG pipeline; uses vector store for semantic search; subscribes to knowledge update events — ⚠ in the Cyrius port, daimon embeddings are live when a daimon is reachable (full-text only otherwise) and `mneme serve` registers its MCP tools with daimon, while the RAG / event-bus / Q&A client paths are deferred to a local fallback
- **Hoosh**: LLM Q&A over knowledge base, automatic tagging, summarization, relationship suggestion
- **MCP Tools**: `mneme_create_note`, `mneme_get_note`, `mneme_update_note`, `mneme_search`, `mneme_search_feedback`, `mneme_query_graph`, `mneme_list_vaults`, `mneme_switch_vault`
- **Agnoshi Intents** (the Rust era's planned set — not in the current agnoshi): `mneme search <query>`, `mneme ask <question>`, `mneme note <action>`, `mneme tag <action>`, `mneme export <format>`, `mneme import <file>`, `mneme link <action>`
- **Marketplace**: Productivity/Knowledge category; sandbox profile allows read-write document directories, network for sync

## Architecture

- **Modules** (pure Cyrius, `src/`, ported one-for-one from the Rust crates): `core_*` (note model, links, tags, graph), `store_*` (patra-backed metadata — was SQLite — plus file storage and the multi-vault registry), `search_*` (full-text + semantic, retrieval optimizer), `ai_*` (Q&A bridge, tagging, summarization, linking, clustering), `api_server` (HTTP router), `ui_*` (terminal UI, ported from the Rust `mneme-ui`'s ratatui app), `mcp_*` (MCP server), `io_*` (markdown, HTML, PDF, Obsidian/Notion import)
- **Dependencies**: patra (storage), bayan (JSON/TOML), sandhi (HTTP), sigil, daimon embeddings API, hoosh LLM API — see the repo's `cyrius.cyml`
- The original Rust workspace (`mneme-core` / `-store` / `-search` / `-ai` / `-api` / `-ui` / `-mcp` / `-io`; SQLite, Tantivy) is frozen at `rust-old/`

## Roadmap

- Real-time collaboration via federation
- Spaced repetition system for review
- Handwriting/OCR ingestion via selah integration
- Knowledge graph visualization improvements
