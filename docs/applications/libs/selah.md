# Selah

> **Selah** (Hebrew: pause/capture) — AI-native screenshot capture, annotation, and redaction

| Field | Value |
|-------|-------|
| Status | Pre-1.0 |
| Version | `0.29.4` |
| Repository | `MacCracken/selah` |
| Runtime | library crate (Rust) |

---

## What It Does

- Screenshot capture via daimon screen capture API (CaptureClient)
- Annotation canvas: rectangles, arrows, text, freehand drawing with SVG output
- Image format conversion (PNG, JPEG, BMP, WebP)
- AI-powered features: OCR text extraction, smart crop suggestions, PII redaction suggestions
- Redaction: detect and mask sensitive content in screenshots
- Capture history tracking with persistent storage
- Region-based and monitor-based capture sources
- Base64 image data encoding/decoding
- Geometry primitives (Rect, Vec2 via hisab)
- Daimon and hoosh client integration for agent registration and LLM features
- Optional MCP server (with `mcp` feature via bote)
- Optional AI features (with `ai` feature via hoosh)

## Consumers

- **aethersafta** — desktop compositor (screenshot integration)
- **daimon** — agent runtime (screen capture API backend)
- **agnoshi** — AI shell (screenshot commands)
