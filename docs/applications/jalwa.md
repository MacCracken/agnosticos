# Jalwa

> **Jalwa** (Persian: manifestation/display) — AI-native media player, in Cyrius (a port of the original Rust workspace, complete at 1.0.0 on 2026-07-10)

| Field | Value |
|-------|-------|
| Status | Released (audio; video playback not yet ported) |
| Version | See [`VERSION`](https://github.com/MacCracken/jalwa/blob/main/VERSION) |
| Repository | `MacCracken/jalwa` |
| Runtime | native-binary (Cyrius) |
| Recipe | `zugot/marketplace/jalwa.cyml` |
| MCP Tools | `jalwa_*` over `jalwa mcp` (stdio JSON-RPC) |
| Agnoshi Intents | None (the current agnoshi has no per-app intents) |
| Port | N/A (desktop app) |

---

## Why First-Party

No existing media player has AI-powered music recommendation, library management via natural language, or deep integration with a local LLM gateway. Jalwa decodes audio through the sovereign stack — shravan codecs, dhvani DSP — without relying on ffmpeg. (The Rust line was built on tarang; tarang, still Rust, is video-only in the port and is what video playback waits on.) Its tight coupling with daimon and hoosh enables features like "play something energetic" or "find songs similar to this one" that would be impossible to retrofit into VLC or Audacious.

## What It Does

- Terminal UI and desktop GUI for audio playback; audio goes out through vani (ALSA / agnos), shared via the mishran mixer. The GUI is a Wayland window on Linux; its agnos backend speaks setu but is **unproven** — the one run that showed jalwa as a window on agnos was a false green under the since-deleted `AETHERSAFHA_SETU_SELFTEST` hook and stays retracted. Video playback is not yet ported
- Library management backed by patra with playlist support (M3U import/export)
- Playback queue with shuffle, repeat, and a 10-band graphic equalizer (dhvani)
- MPRIS2 media-key command surface — its D-Bus export is stubbed in the port (samvada is D-Bus client-only)
- AI-powered recommendations and NL library search; audio fingerprinting is a stub, blocked on tarang's AI kernel

## AGNOS Integration

- **Daimon**: Registers as an agent; ingests library metadata into RAG — ⚠ in the Cyrius port the daimon/hoosh client's network calls are still stubs (config, auth and request builders are ported)
- **Hoosh**: LLM-powered music recommendations, NL library queries, content description
- **MCP Tools**: `jalwa_play`, `jalwa_pause`, `jalwa_status`, `jalwa_search`, `jalwa_recommend`, `jalwa_queue`, `jalwa_library`, `jalwa_playlist`
- **Agnoshi Intents** (the Rust era's planned set — not in the current agnoshi): `jalwa play <query>`, `jalwa pause`, `jalwa status`, `jalwa search <term>`, `jalwa recommend`, `jalwa queue <action>`, `jalwa library <action>`, `jalwa playlist <action>`
- **Marketplace**: Media category; sandbox profile allows PipeWire socket, D-Bus session bus, read-only media directories, network for metadata fetching

## Architecture

- **Modules** (pure Cyrius, `src/`):
  - `core` — library, playlists, play queue, playback state, search; scanner (audio probe + tags via shravan), DB (patra), watcher (inotify), hardware (yukti)
  - `playback` — decode → output pipeline (shravan decode → dhvani EQ/normalize → vani out), threaded engine, transport
  - `ai` — recommendations, daimon, fingerprint stub
  - `ui` — terminal UI over darshana
  - `gui` — draw-command IR → CPU rasterizer (kashi font) → Wayland present on Linux / a setu backend for agnos (unproven, above); album art via chitra
  - `mcp` — stdio JSON-RPC 2.0 server
- **Dependencies**: dhvani, shravan, vani, mishran, patra, darshana, kashi, setu, rupa, chitra, yukti, abaco, sankoch — see the repo's `cyrius.cyml`
- The original 5-crate Rust workspace (`jalwa-core` / `-playback` / `-ui` / `-ai` / `-gui`; tarang, symphonia, PipeWire, SQLite, D-Bus) is frozen at `rust-old/` as the port's parity oracle

## Roadmap

- **Video playback** — not yet ported; blocked on tarang and a video surface (aethersafta), both still Rust. Top of jalwa's own roadmap
- **Dhara integration** — connect to [Dhara](dhara.md) as streaming backend for remote library access, transcoded playback, and multi-device sync
- Streaming radio/podcast support
- Collaborative playlists via federation
- Smart playlists based on listening history and AI classification
