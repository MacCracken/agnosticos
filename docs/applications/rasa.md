# Rasa

> **Rasa** (Sanskrit: essence) — AI-native image editor

| Field | Value |
|-------|-------|
| Status | Released |
| Version | `2026.3.15` |
| Repository | `MacCracken/rasa` |
| Runtime | native-binary (~3.2MB amd64, ~3.0MB arm64) |
| Recipe | `recipes/marketplace/rasa.toml` |
| MCP Tools | 9 `rasa_*` |
| Agnoshi Intents | 9 |
| Port | N/A (desktop app) |

---

## Why First-Party

AI-native image editing with natural language commands ("remove the background", "enhance contrast", "select everything blue") is impossible to add to GIMP or Inkscape without a deep fork. Rasa is designed from the ground up with LLM-driven editing as a core feature, not an afterthought. It also ships its own MCP server crate for direct tool-level integration with other AGNOS agents.

## What It Does

- Layer-based image editing with non-destructive filter pipeline
- NL-driven editing commands interpreted through hoosh
- Selection tools with AI-assisted edge detection and content-aware fill
- Color management, transform operations, and export to multiple formats
- Own MCP server (`rasa-mcp` crate) exposing 5 native tools + 5 agnoshi intents

## AGNOS Integration

- **Daimon**: Registers as an agent; uses RAG for asset discovery; publishes edit history
- **Hoosh**: NL command interpretation, content-aware suggestions, image description
- **MCP Tools**: `rasa_canvas`, `rasa_layers`, `rasa_tools`, `rasa_export`, `rasa_filters`, `rasa_selection`, `rasa_transform`, `rasa_color`, `rasa_ai`
- **Agnoshi Intents**: `rasa canvas <action>`, `rasa layers <action>`, `rasa tools <tool>`, `rasa export <format>`, `rasa filters <filter>`, `rasa selection <mode>`, `rasa transform <op>`, `rasa color <action>`, `rasa ai <command>`
- **Marketplace**: Graphics/Creative category; sandbox profile allows GPU access, read-write project directories, network for asset fetching

## Architecture

- **Crates**:
  - `rasa-core` — canvas, layer engine, pixel operations, undo/redo
  - `rasa-filters` — image filters, color adjustments, blur, sharpen
  - `rasa-tools` — selection tools, brushes, transform operations
  - `rasa-ai` — daimon/hoosh integration, NL command parsing, content-aware features
  - `rasa-mcp` — standalone MCP server with 5 native tools + 5 agnoshi intents
  - `rasa-ui` — desktop GUI, viewport, tool panels
- **Dependencies**: image (Rust image processing), wgpu (GPU rendering), SQLite (asset catalog)

## Roadmap

- Vector drawing tools (SVG overlay layer)
- Batch processing via MCP tool chaining
- RAW format support (camera sensor data)
- AI inpainting and generative fill via hoosh
