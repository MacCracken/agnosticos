# Selah

> **Selah** (Hebrew: pause/capture) — AI-native screenshot and annotation tool

| Field | Value |
|-------|-------|
| Status | Released |
| Version | See [`VERSION`](https://github.com/MacCracken/selah/blob/main/VERSION) |
| Repository | `MacCracken/selah` |
| Runtime | Rust library crate — a flat `[lib]` since 0.24.3 (2026-03-24), when the binary target was removed |
| Recipe | `zugot/marketplace/selah.cyml` |
| MCP Tools | `selah_*` (behind the crate's `mcp` feature) |
| Agnoshi Intents | None (the current agnoshi has no per-app intents) |
| Port | N/A |

---

## Why First-Party

No existing screenshot tool has AI-powered automatic redaction of sensitive content or content-aware annotation suggestions. Selah wraps the daimon screen capture API for native Wayland capture and uses hoosh for OCR text extraction, PII detection, and intelligent annotation placement. All processing stays local.

## What It Does

- Screen capture via daimon's screen capture API (Wayland-native, per-agent permissions)
- AI-powered automatic redaction of sensitive content (passwords, keys, PII)
- OCR text extraction from captured screenshots
- SVG-based annotations (arrows, highlights, text boxes, blur regions)
- Export to PNG, clipboard, and file with metadata stripping

## AGNOS Integration

- **Daimon**: Uses `/v1/screen/capture` API; registers for per-agent capture permissions; leverages screen capture history
- **Hoosh**: LLM inference for OCR interpretation, PII detection, redaction suggestions, and content-aware annotation placement
- **MCP Tools**: `selah_capture`, `selah_annotate`, `selah_redact`, `selah_ocr`, `selah_history`
- **Agnoshi Intents** (the Rust era's planned set — not in the current agnoshi): `selah capture`, `selah annotate`, `selah redact`, `selah ocr`, `selah export`
- **Marketplace**: Category: utilities/capture. Sandboxed with screen capture permission and user pictures directory access

## Architecture

- **Modules** (one flat crate since 0.24.3; the `selah-core` / `-capture` / `-annotate` / `-ai` / `-mcp` sub-crates became modules): core, capture, annotate, ai, mcp
- **Dependencies**: image, ranga, hisab (geometry), tokio, reqwest (daimon client); bote (MCP, `mcp` feature), hoosh (`ai` feature)

## Roadmap

Stable. Future considerations: video clip capture (via screen recording API), scrolling capture, direct sharing to Delta.
