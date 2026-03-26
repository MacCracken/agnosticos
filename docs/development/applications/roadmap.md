# Application Development Roadmap

> **Status**: Active | **Last Updated**: 2026-03-24
>
> Future first-party applications planned for the AGNOS ecosystem.
> All follow the [First-Party Standards](first-party-standards.md).
> Released applications are documented in [docs/applications/](../../applications/).
> Shared crates and infrastructure: [shared-crates.md](shared-crates.md).
> Orchestration platform (k8s-equivalent): [k8s-roadmap.md](../k8s-roadmap.md).
> Monolith extraction plan: [monolith-extraction.md](../monolith-extraction.md).
> Network evolution (TCP→QUIC→AAP): [network-evolution.md](../network-evolution.md).

---

## Priority 1 — Essential Desktop (needed for daily-driver OS)

### PDF / Document Suite

| Field | Value |
|-------|-------|
| Status | Planned |
| Priority | 1 — ship before beta |

**Why first-party**: AI-native document viewer with OCR, summarization, translation, document Q&A, and table extraction. Cannot be achieved by wrapping Zathura or Evince.

**Scope**:
- Reader, annotator, form filler, digital signatures
- AI: OCR (Tesseract — already in Aequi), summarization, translation, document Q&A, table extraction via hoosh
- Infrastructure: poppler recipe done (`recipes/desktop/poppler.toml`), mupdf Rust bindings exist

**Interim**: Zathura (lightweight) and Evince (full-featured, bazaar) shipping now.

**Effort**: Medium

---

### Email Client

| Field | Value |
|-------|-------|
| Status | Planned |
| Priority | 1 — critical for desktop completeness |

**Why first-party**: Smart compose, priority inbox, thread summarization, phishing detection (aegis/phylax integration), auto-categorization. No existing email client provides LLM-powered triage or OS-level phishing detection.

**Scope**:
- Local-first, privacy-respecting, IMAP/SMTP
- AI: Smart compose via hoosh, priority inbox, thread summarization, phishing detection, auto-categorization

**Effort**: High — email protocols are complex, but notmuch/aerc patterns exist

---

### File Manager

| Field | Value |
|-------|-------|
| Status | Planned |
| Priority | 1 — ship before beta |

**Why first-party**: Semantic file finding via RAG, duplicate detection, auto-tagging by content, predictive organization. No existing file manager has vector search or NL file queries.

**Scope**:
- Dual-pane GUI, thumbnail preview, batch rename
- AI: Semantic search via daimon vector store, duplicate detection, auto-tagging, predictive organization ("you usually put invoices in ~/Documents/Finance")
- Infrastructure: FUSE in agnos-sys, inotify available

**Interim**: yazi (TUI, beta) and Thunar (GUI, bazaar) shipping now.

**Effort**: Medium — egui + filesystem ops

---

### Backup Manager

| Field | Value |
|-------|-------|
| Status | Planned |
| Priority | 1 — data safety non-negotiable |

**Why first-party**: Priority-based restore suggestions, anomaly detection (unexpected large changes), smart scheduling (backup when idle). OS-level integration with LUKS, dm-verity, ark package format.

**Scope**:
- Incremental, encrypted, local + remote targets (SSH, S3-compatible)
- AI: Priority-based restore, anomaly detection, smart scheduling
- Infrastructure: LUKS/dm-verity in agnos-sys

**Effort**: Medium — restic/borg patterns, Rust implementation

---

## Priority 2 — Strong Utility (significant daily value)

### Sutra — Infrastructure Orchestrator

| Field | Value |
|-------|-------|
| Status | Released |
| Version | `2026.3.18` |
| Priority | 2 — critical for fleet + self-hosting |
| Spec | [sutra.md](sutra.md) |

**Released**. 5 crates, 70 tests, 6 MCP tools, 6 core modules. [sutra-community](https://github.com/MacCracken/sutra-community) has 5 additional modules.

---

### Murti — Core Model Runtime

| Field | Value |
|-------|-------|
| Status | Scaffolded |
| Version | `0.1.0` |
| Priority | 2 — foundational for hoosh + Irfan, Ollama replacement |
| Spec | [murti.md](murti.md) |

**Why first-party**: Extracts model lifecycle from Irfan into shared crate. Enables hoosh as full Ollama replacement. See spec for architecture.

---

### Tanur — Desktop LLM Studio

| Field | Value |
|-------|-------|
| Status | Scaffolded |
| Version | `0.1.0` |
| Priority | 2 — desktop experience for Irfan, LM Studio replacement |
| Spec | [tanur.md](tanur.md) |

**Why first-party**: Native GUI client for Irfan. Connects over Unix socket. Supersedes `ifran-desktop` Tauri crate. See spec for full panel mapping.

---

### Disk Analyzer

| Field | Value |
|-------|-------|
| Status | Planned |
| Priority | 2 |

**Why first-party**: AI cleanup suggestions ("these 3 GB of build artifacts haven't been touched in 6 months"), safe-delete confidence scoring. Integrates with daimon metrics.

**Scope**: Treemap visualization, duplicate finder, large file finder.

**Effort**: Low-Medium

---

### Network Manager GUI

| Field | Value |
|-------|-------|
| Status | Planned |
| Priority | 2 |

**Why first-party**: AI security recommendations ("this network has no encryption"), auto-VPN triggers. Deep integration with nftables and aegis.

**Scope**: Visual WiFi/Ethernet/VPN management, connection profiles, firewall rule editor, bandwidth monitoring.

**Interim**: nm-applet recipe shipping for basic WiFi/VPN.

**Effort**: Medium — NetworkManager D-Bus bindings

---

### Log Viewer

| Field | Value |
|-------|-------|
| Status | Planned |
| Priority | 2 |

**Why first-party**: Pattern detection ("this error correlates with that service restart"), anomaly highlighting, root cause suggestions via hoosh. Aggregates all AGNOS log sources.

**Scope**: Aggregates journald + daimon audit + phylax findings + agent logs. Timeline view, filtering, search, tail mode.

**Effort**: Low-Medium — mostly reading existing APIs

---

### RSS / Feed Reader

| Field | Value |
|-------|-------|
| Status | Planned |
| Priority | 2 |

**Why first-party**: Article summarization, topic clustering, priority sorting, "daily briefing" generation via hoosh. Pairs well with Mneme for knowledge capture.

**Scope**: Local-first, offline-capable. Atom/RSS parsing + AI features.

**Effort**: Low

---

## Priority 3 — Developer & Power User Tools

### Database Browser

| Field | Value |
|-------|-------|
| Status | Planned |
| Priority | 3 |

**Why first-party**: NL-to-SQL ("show me users who signed up last month"), explain query plans, suggest indexes. Useful for debugging agent databases managed by argonaut.

**Scope**: GUI for SQLite, PostgreSQL, Redis. Schema visualization, query editor.

**Effort**: Medium

---

### API Client

| Field | Value |
|-------|-------|
| Status | Planned |
| Priority | 3 |

**Why first-party**: Generate requests from API docs, analyze responses, detect breaking changes. Ships with daimon/hoosh API as built-in collection.

**Scope**: HTTP client with request builder, collections, environments.

**Effort**: Medium

---

### Terminal Multiplexer

| Field | Value |
|-------|-------|
| Status | Planned |
| Priority | 3 |

**Why first-party**: Session suggestions, command prediction, context-aware shell history. Native agnoshi integration.

**Scope**: tmux/zellij alternative with AGNOS integration.

**Effort**: High — terminal emulation is complex

---

### Presentation Tool

| Field | Value |
|-------|-------|
| Status | Planned |
| Priority | 3 |

**Why first-party**: NL-to-slides ("make a 5-slide pitch deck about X"), auto-layout, speaker notes generation, image suggestions via Rasa.

**Scope**: Slide deck creation from Markdown or NL. PDF export.

**Effort**: Medium

---

### Live Streaming / Broadcast Studio

| Field | Value |
|-------|-------|
| Status | Planned |
| Priority | 3 |

**Why first-party**: AI scene switching (auto-cut on silence, speaker detection), real-time chat moderation via hoosh, stream health monitoring via nazar, overlay generation, highlight clipping. No existing streaming tool has LLM-powered production assistance or OS-level media pipeline integration.

**Scope**:
- Scene management, transitions, overlays, multi-source mixing
- RTMP/SRT/WHIP output (Twitch, YouTube, custom)
- Media pipeline: tarang for encoding/muxing (18-33x faster than GStreamer pipeline setup)
- Compositing: **aethersafta** crate — scene graph, multi-source capture, hardware-accelerated encoding
- Audio: PipeWire capture via aethersafta, per-source mixing, noise suppression
- AI: Auto scene switching (voice activity, face detection), chat moderation, highlight detection, real-time transcription/captioning via hoosh, stream analytics
- Hardware acceleration: ai-hwaccel for GPU/NPU-aware encoding (NVENC, VA-API, QSV)

**Infrastructure**: [aethersafta](https://github.com/MacCracken/aethersafta) (compositing engine, crates.io), [tarang](https://crates.io/crates/tarang) (encoding), [ai-hwaccel](https://crates.io/crates/ai-hwaccel) (hardware detection), hoosh (AI), nazar (monitoring), PipeWire (audio)

**Prerequisites**: aethersafta v0.8.0+ (RTMP/SRT output). The compositing engine is a standalone crate (`aethersafta`) extracted from aethersafha (AGNOS Phase 16F). This app builds the production UI on top.

**Effort**: Medium-High — aethersafta delivers the compositing backend. Primary remaining work is the production UI (scene management, preview/program monitors, stream controls, chat integration).

---

## Priority 4 — Creative & Interactive

### 3D Modeler / CAD

| Field | Value |
|-------|-------|
| Status | Planned |
| Priority | 4 |

**Why first-party**: Text-to-3D, parametric suggestions, topology optimization. No open-source AI-native CAD exists.

**Effort**: Very High

---

### Font Manager

| Field | Value |
|-------|-------|
| Status | Planned |
| Priority | 4 |

**Why first-party**: Font pairing suggestions, similarity search, mood/style classification via hoosh.

**Effort**: Low

---

### Color Picker / Palette Generator

| Field | Value |
|-------|-------|
| Status | Planned |
| Priority | 4 |

**Why first-party**: Auto-generate palettes from images, accessibility contrast checker, mood-based palette generation.

**Effort**: Low

---

### Kiran — Game Engine

| Field | Value |
|-------|-------|
| Status | In Development (v0.4) |
| Priority | 4 |
| Spec | [kiran.md](kiran.md) |

**Why first-party**: AI-native game engine core. ECS architecture, game loop, scene management, rendering (aethersafta/wgpu), audio (dhvani), input handling. Thin orchestration layer over existing AGNOS shared crates. Does NOT own physics (impetus), simulation (joshua), or AI NPCs (joshua).

**Scope**: ECS, game loop, scene format (TOML), rendering integration, audio integration, input handling, hot reload.

**Effort**: High — but built on existing crates, not from scratch

---

### Joshua — Game Manager & Simulation Core

| Field | Value |
|-------|-------|
| Status | Planned |
| Priority | 4 |
| Spec | [joshua.md](joshua.md) |

**Why first-party**: AI-native game manager and simulation runtime. NPCs are daimon agents with LLM brains. Headless simulation mode for AI training — agents prove themselves in virtual environments before real deployment. Sits on top of kiran (engine) and impetus (physics). Composes agnosai (NPCs), majra (multiplayer), kavach (scripting sandbox), t-ron (NPC security).

**Scope**: Simulation runner, AI NPCs via agnosai, deterministic replay, headless mode at unlimited speed, visual editor (egui), multiplayer via majra.

**Effort**: Very High — but built on kiran + existing crates

---

## Priority 5 — Communication & Collaboration

### Chat / Messaging

| Field | Value |
|-------|-------|
| Status | Planned |
| Priority | 5 |

**Why first-party**: Agent-to-human and human-to-human communication with PQC encryption. Leverages federation module and pubsub broker.

**Effort**: High — E2EE messaging done right is complex

---

### Video Conferencing

| Field | Value |
|-------|-------|
| Status | Planned |
| Priority | 5 |

**Why first-party**: Real-time transcription, meeting summarization, action item extraction via hoosh.

**Effort**: Very High

---

## Priority 6 — Future / Exploratory

### Voice Assistant Shell (Vansh)

| Field | Value |
|-------|-------|
| Status | Planned |
| Priority | 6 |

Already in named subsystems. TTS/STT voice interface for agnoshi.

**Effort**: High

---

### IoT Dashboard

| Field | Value |
|-------|-------|
| Status | Planned |
| Priority | 6 |

Visual management for edge fleet beyond Nazar's system focus.

**Effort**: Medium

---

### AI Training Studio

| Field | Value |
|-------|-------|
| Status | Planned |
| Priority | 6 |

Visual fine-tuning and dataset management. GUI on existing Irfan/finetune APIs.

**Effort**: Medium

---

## Shared Crates & Infrastructure

See **[shared-crates.md](shared-crates.md)** for the full ecosystem crate registry, version table, extraction status, and ranga details.

**1.0+ stable** (17): hisab (1.1.0), bhava (1.1.0), prakash (1.1.0), impetus (1.1.0), ushma (1.1.0), pravash (1.1.0), kimiya (1.0.0), kavach (1.0.1), stiva (1.0.0), bijli (1.0.0), goonj (1.0.0), pavan (1.0.0), dravya (1.0.0), badal (1.0.0), khanij (1.0.0).
**Published on crates.io** (21): kiran (0.26.3), abaco (0.22.4), yukti (0.25.3), phylax (0.22.3), ai-hwaccel (0.23.3), dhvani (0.22.4), t-ron (0.22.4), selah (0.24.3), raasta (0.26.3), soorat (0.24.3), agnosai (0.25.3), nein (0.24.3), ranga (0.24.3), tarang (0.21.3), hoosh (0.25.3), aethersafta (0.25.3), majra (0.22.3), libro (0.25.3), bote (0.22.3), szal (0.23.4), muharrir (0.23.5).
**GitHub release only**: agnostik (2026.3.26), agnosys (0.25.4).
**Not yet published**: murti, kana, salai.
**In progress**: jantu (creature behavior).
**Scaffolded**: tara (stellar), falak (orbital), jyotish (astrology), joshua (game manager), daimon (agent orchestrator).

Science stack: prakash (optics), kana (quantum), bijli (EM), ushma (thermo), pravash (fluids), kimiya (chemistry), goonj (acoustics), pavan (aero), dravya (materials), badal (weather), khanij (geology). Scaffolded: tara (stellar), falak (orbital), jyotish (astrology). See [shared-crates.md](shared-crates.md).

See **[stiva.md](stiva.md)** for the full container runtime design, security analysis, and composable isolation architecture.

See **[k8s-roadmap.md](../k8s-roadmap.md)** for how stiva + nein + majra + kavach compose into a k8s-equivalent orchestration platform.

---

## Implementation Notes

- All apps follow [First-Party Standards](first-party-standards.md)
- Priority 1 items should be addressed before beta (Q4 2026)
- Priority 2-3 items strengthen the daily-driver story
- Priority 4-6 items are post-v1.0 or community-contributed
- When an app reaches first release, move its doc from here to `docs/applications/{name}.md`

---

*Last Updated: 2026-03-25*
