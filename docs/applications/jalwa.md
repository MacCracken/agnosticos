# Jalwa

> **Jalwa** (Persian: manifestation/display) — AI-native media player built on tarang

| Field | Value |
|-------|-------|
| Status | Released |
| Version | `2026.3.16-1` |
| Repository | `MacCracken/jalwa` |
| Runtime | native-binary (~9.9MB) |
| Recipe | `recipes/marketplace/jalwa.toml` |
| MCP Tools | 8 `jalwa_*` |
| Agnoshi Intents | 8 |
| Port | N/A (desktop app) |

---

## Why First-Party

No existing media player has AI-powered music recommendation, library management via natural language, or deep integration with a local LLM gateway. Jalwa is built on tarang for native codec support without relying on ffmpeg. Its tight coupling with daimon and hoosh enables features like "play something energetic" or "find songs similar to this one" that would be impossible to retrofit into VLC or Audacious.

## What It Does

- Desktop GUI and TUI for audio/video playback with PipeWire audio output
- Library management backed by SQLite with playlist support (M3U import/export)
- Playback queue with shuffle, repeat, and 10-band parametric equalizer
- MPRIS2/D-Bus integration for system-level media key control
- AI-powered recommendations, NL library search, and audio fingerprint matching via tarang

## AGNOS Integration

- **Daimon**: Registers as an agent; ingests library metadata into RAG; indexes audio fingerprints via tarang MCP tools
- **Hoosh**: LLM-powered music recommendations, NL library queries, content description
- **MCP Tools**: `jalwa_play`, `jalwa_pause`, `jalwa_status`, `jalwa_search`, `jalwa_recommend`, `jalwa_queue`, `jalwa_library`, `jalwa_playlist`
- **Agnoshi Intents**: `jalwa play <query>`, `jalwa pause`, `jalwa status`, `jalwa search <term>`, `jalwa recommend`, `jalwa queue <action>`, `jalwa library <action>`, `jalwa playlist <action>`
- **Marketplace**: Media category; sandbox profile allows PipeWire socket, D-Bus session bus, read-only media directories, network for metadata fetching

## Architecture

- **Crates**:
  - `jalwa-core` — player state machine, library database, playlist engine
  - `jalwa-playback` — audio/video decoding via tarang, PipeWire output, gapless playback
  - `jalwa-ui` — TUI interface (ratatui)
  - `jalwa-ai` — daimon/hoosh integration, recommendation engine, NL query handling
  - `jalwa-gui` — desktop GUI (egui), visualizations, album art
- **Dependencies**: tarang (media framework), symphonia (audio decode), PipeWire (audio output), SQLite (library), D-Bus (MPRIS2)

## Roadmap

- Streaming radio/podcast support
- Collaborative playlists via federation
- Video playback improvements (subtitle rendering, hardware decode)
- Smart playlists based on listening history and AI classification
