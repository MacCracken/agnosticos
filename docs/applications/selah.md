# Selah

> **Selah** (Hebrew: pause/capture) — AI-native screenshot and annotation tool

| Field | Value |
|-------|-------|
| Status | Released |
| Version | Latest GitHub release |
| Repository | `MacCracken/selah` |
| Runtime | native-binary (Rust) |
| Recipe | `recipes/marketplace/selah.toml` |
| MCP Tools | 5 `selah_*` |
| Agnoshi Intents | 5 |
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
- **MCP Tools**: `selah_capture`, `selah_annotate`, `selah_redact`, `selah_ocr`, `selah_export`
- **Agnoshi Intents**: `selah capture`, `selah annotate`, `selah redact`, `selah ocr`, `selah export`
- **Marketplace**: Category: utilities/capture. Sandboxed with screen capture permission and user pictures directory access

## Architecture

- **Crates**: core, capture, annotate, ai
- **Dependencies**: image, resvg (SVG rendering), tesseract (OCR), tokio, reqwest (daimon client)

## Roadmap

Stable — 50 tests passing. Future considerations: video clip capture (via screen recording API), scrolling capture, direct sharing to Delta.
