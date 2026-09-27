# Shruti

> **Shruti** (Sanskrit: that which is heard) — AI-native digital audio workstation

| Field | Value |
|-------|-------|
| Status | Released |
| Version | See [`VERSION`](https://github.com/MacCracken/shruti/blob/main/VERSION) |
| Repository | `MacCracken/shruti` |
| Runtime | native-binary (Rust) |
| Recipe | `zugot/marketplace/shruti.cyml` |
| MCP Tools | `shruti_*` (in `shruti-ai`) |
| Agnoshi Intents | None (the current agnoshi has no per-app intents) |
| Port | 8050 — `shruti serve`'s default (headless agent API); the desktop app needs none |

---

## Why First-Party

No open-source DAW has AI-powered composition assistance, intelligent mixing suggestions, or natural language control over the production workflow. Shruti integrates directly with hoosh for LLM-driven features and with daimon for agent-based automation, enabling workflows like "add a reverb tail to the vocal track" or "generate a drum pattern in 7/8 time" that cannot be bolted onto Ardour or LMMS.

## What It Does

- Multi-track audio recording and editing with non-destructive processing
- DSP engine with built-in effects (EQ, compression, reverb, delay) and plugin hosting
- AI-assisted composition: melody generation, chord suggestion, arrangement assistance
- Intelligent mixing: automatic gain staging, spectral balance, mastering chain suggestions
- Session management with undo history and project file format

## AGNOS Integration

- **Daimon**: Registers as an agent; publishes session metadata; uses RAG for sample/preset discovery
- **Hoosh**: LLM-powered composition assistance, mixing suggestions, NL command interpretation
- **MCP Tools**: `shruti_session`, `shruti_tracks`, `shruti_transport`, `shruti_export`, `shruti_analysis`, `shruti_mixer`, `shruti_hardware`, `shruti_models`
- **Agnoshi Intents** (the Rust era's planned set — not in the current agnoshi): `shruti record <input>`, `shruti mix <action>`, `shruti effect <name> <params>`, `shruti export <format>`, `shruti session <action>`, `shruti analyze <track>`, `shruti generate <description>`
- **Marketplace**: Audio/Creative category; sandbox profile allows PipeWire socket, MIDI device access, read-write project directories

## Architecture

- **Crates**:
  - `shruti-engine` — audio engine, track management, transport control
  - `shruti-dsp` — DSP processing, built-in effects, sample-accurate scheduling
  - `shruti-plugin` — plugin hosting (CLAP, VST3, native Rust plugin API), plugin scanning and sandboxing
  - `shruti-ui` — desktop GUI, waveform display, mixer view, arrangement timeline
  - `shruti-session` — project file format, undo/redo, session state persistence
  - `shruti-ai` — daimon/hoosh integration, composition AI, mixing intelligence
  - `shruti-instruments` — built-in synthesizers and samplers
  - `shruti-ml` — music LLM runtime, tokenizer, and AI player agents
- **Dependencies**: cpal (audio I/O — ALSA/PipeWire on Linux), dhvani (DSP + analysis, the Rust crate), midir (MIDI), rusqlite (SQLite), egui/eframe (UI), axum (`shruti serve`); tarang is an optional feature (e.g. Opus/AAC encode)

## Roadmap

- MIDI sequencer improvements (piano roll, drum grid)
- Collaborative sessions via federation
- AI mastering chain with reference track matching
