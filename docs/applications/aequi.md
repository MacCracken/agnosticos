# Aequi

> **Aequi** (Latin: fair/equal) — AI-native self-employed accounting platform

| Field | Value |
|-------|-------|
| Status | Released |
| Version | Latest GitHub release |
| Repository | `anomalyco/aequi` |
| Runtime | native-binary (Rust/Tauri v2) |
| Recipe | `recipes/marketplace/aequi.toml` |
| MCP Tools | 5 `aequi_*` |
| Agnoshi Intents | 5 |
| Port | N/A |

---

## Why First-Party

No existing accounting tool integrates with a local LLM for AI-native receipt scanning, automatic categorization, and natural-language queries. Aequi is built from scratch to leverage hoosh for tasks like "how much did I spend on office supplies this quarter?" while keeping all financial data local and private. The Tauri v2 desktop shell provides a native experience without Electron overhead.

## What It Does

- Double-entry bookkeeping with automated transaction categorization via LLM
- OCR receipt and invoice scanning with automatic data extraction
- Natural-language financial queries and reporting
- Multi-format import (CSV, OFX, QIF) with intelligent field mapping
- PDF invoice generation and export

## AGNOS Integration

- **Daimon**: Registers as an agent on port 8090; uses agent lifecycle, memory store, and audit APIs
- **Hoosh**: LLM-powered receipt OCR interpretation, transaction categorization, and NL query answering
- **MCP Tools**: `aequi_categorize`, `aequi_query`, `aequi_import`, `aequi_report`, `aequi_receipt`
- **Agnoshi Intents**: `aequi balance`, `aequi expense`, `aequi import`, `aequi report`, `aequi receipt`
- **Marketplace**: Category: productivity/finance. Sandboxed with filesystem access limited to user data directory

## Architecture

- **Crates**: core, storage, app, import, ocr, pdf, mcp, server (+ Tauri app crate)
- **Dependencies**: tauri v2, serde, rusqlite (SQLite storage), image, tesseract (OCR), printpdf

## Roadmap

Phases 1-8 complete. Stable — maintenance mode. Future considerations: multi-currency support, tax-filing integrations, collaborative bookkeeping.
