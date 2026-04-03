# Dhara

> **Dhara** (Sanskrit: stream/flow) — AI-native self-hosted media streaming server

| Field | Value |
|-------|-------|
| Status | Planned |
| Version | — |
| Repository | `MacCracken/dhara` |
| Runtime | native-binary |
| Recipe | `recipes/marketplace/dhara.toml` |
| MCP Tools | TBD `dhara_*` |
| Agnoshi Intents | TBD |
| Port | 8078 |

---

## Why First-Party

No self-hosted media server has AI-powered content discovery, automatic metadata enrichment via local LLM, or agent-native library management. Jellyfin/Plex rely on external metadata scrapers and have no concept of natural language queries or semantic search. Dhara is built on tarang for hardware-accelerated transcoding without ffmpeg, integrates with hoosh for "find that movie where the guy..." queries, and uses daimon agents for automatic library organization, subtitle fetching, and watch-party coordination.

## What It Does

- **Media server**: Serves music, video, and photo libraries over HTTP/HTTPS to local and remote clients
- **Transcoding**: Real-time adaptive transcoding via tarang with hardware acceleration (NVENC, VA-API, QSV, AMF) through ai-hwaccel
- **Library management**: Automatic scanning, metadata enrichment (TMDb, MusicBrainz, local LLM fallback), genre/mood classification
- **Multi-user**: Per-user libraries, watch history, recommendations, parental controls
- **Streaming protocols**: HLS/DASH adaptive streaming, DLNA/UPnP for local devices, Chromecast support
- **Music streaming**: Gapless playback, Subsonic API compatibility (works with existing mobile apps), smart playlists
- **AI features**: NL search ("play something like Interstellar"), content-based recommendations, auto-generated collections, scene/chapter detection for video, audio fingerprint matching
- **Offline sync**: Clients can pin content for offline playback with quality presets

## AGNOS Integration

- **Daimon**: Registers as a persistent agent; library metadata indexed in RAG; agents can query/control playback
- **Hoosh**: NL search, content recommendations, automatic tagging and description generation, subtitle translation
- **Tarang**: All transcoding and codec operations — no ffmpeg dependency
- **ai-hwaccel**: GPU detection and hardware encoder selection for transcoding
- **Kavach**: Sandboxed with network access, read-only media directories, PipeWire socket for direct audio output
- **Jalwa**: Native client — Jalwa connects to Dhara as a streaming backend in addition to local library
- **Mela**: Media category; marketplace recipe with configurable media paths and transcoding presets
- **MCP Tools**: `dhara_search`, `dhara_play`, `dhara_library`, `dhara_transcode`, `dhara_users`, `dhara_collections`, `dhara_sync`, `dhara_status`
- **Agnoshi Intents**: `dhara search <query>`, `dhara play <title>`, `dhara library scan`, `dhara status`, `dhara recommend`, `dhara collection <action>`, `dhara sync <device>`, `dhara transcode <preset>`

## Architecture

- **Crates**:
  - `dhara-core` — library database (SQLite), media scanner, metadata engine, user management
  - `dhara-stream` — HTTP streaming server, HLS/DASH segment generation, adaptive bitrate logic
  - `dhara-transcode` — tarang integration, transcoding pipeline, quality profiles, hardware encoder selection
  - `dhara-discover` — DLNA/UPnP advertisement, Chromecast discovery, Subsonic API compatibility layer
  - `dhara-ai` — daimon/hoosh integration, NL search, recommendation engine, auto-tagging
  - `dhara-ui` — web dashboard (served by dhara itself), admin panel, playback UI
- **Dependencies**: tarang (media framework), ai-hwaccel (GPU detection), SQLite (library), hyper (HTTP server), tokio (async runtime)

## Roadmap

- Phase 1: Core server — library scanning, metadata, HLS streaming, web UI, single-user
- Phase 2: Multi-user, transcoding profiles, Subsonic API, DLNA
- Phase 3: AI features — NL search, recommendations, auto-collections, scene detection
- Phase 4: Jalwa integration, offline sync, watch-party (federated), live TV/DVR (HDHomeRun)
