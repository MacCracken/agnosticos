# Application Development Roadmap

> **Status**: Active | **Last Updated**: 2026-03-18
>
> Future first-party applications planned for the AGNOS ecosystem.
> All follow the [First-Party Standards](first-party-standards.md).
> Released applications are documented in [docs/applications/](../../applications/).

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
| Status | Planned |
| Priority | 2 — critical for fleet + self-hosting |
| Spec | [sutra.md](sutra.md) |

**Why first-party**: No Rust orchestrator exists. AI-native NL/Markdown→TOML playbooks with dry-run-by-default. Deep integration with daimon fleet, ark, argonaut. User owns TOML as IaC source of truth — AI assists, never auto-applies.

**Scope**: Declarative TOML playbooks, 11 modules (ark, argonaut, aegis, file, daimon, edge, shell, user, nftables, sysctl, verify), SSH + daimon transport, static/dynamic inventory, rollback.

**Effort**: Medium-High — core engine is straightforward, module breadth takes time

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

## Priority 4 — Creative & Specialized

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

### Game Engine / Runtime

| Field | Value |
|-------|-------|
| Status | Planned |
| Priority | 6 |

2D/3D game runtime with AI NPCs.

**Effort**: Very High

---

### AI Training Studio

| Field | Value |
|-------|-------|
| Status | Planned |
| Priority | 6 |

Visual fine-tuning and dataset management. GUI on existing Synapse/finetune APIs.

**Effort**: Medium

---

## Shared Crates (Ecosystem Infrastructure)

Standalone crates extracted from AGNOS that the entire ecosystem depends on.
Published to crates.io, used by AGNOS, Synapse, AgnosAI, SecureYeoman, and consumer apps.

| Crate | Version | Description | Consumers |
|-------|---------|-------------|-----------|
| [ai-hwaccel](https://github.com/MacCracken/ai-hwaccel) | 0.20.3 | Universal AI hardware accelerator detection (13 families), quantisation, sharding, training memory estimation | hoosh, daimon, Synapse, AgnosAI, tazama |
| [tarang](https://github.com/MacCracken/tarang) | 0.20.3 | AI-native media framework — 18-33x faster than GStreamer. Audio/video decode, encode, mux, fingerprint, analysis | jalwa, tazama, shruti, aethersafta |
| [aethersafta](https://github.com/MacCracken/aethersafta) | 0.20.3 | Real-time media compositing — scene graph, multi-source capture, HW encoding, streaming output | aethersafha, streaming app, tazama, SY, selah |
| [hoosh](https://github.com/MacCracken/hoosh) | 0.20.4 | AI inference gateway — 14 LLM providers, OpenAI-compatible API, token budgets, whisper STT, caching | daimon, tarang, aethersafta, agnoshi, AgnosAI, all consumer apps |
| [ranga](https://github.com/MacCracken/ranga) | 0.20.3 | Core image processing — color spaces, blend modes, pixel buffers, filters, GPU compute | rasa, tazama, aethersafta, streaming app |
| [nada](https://github.com/MacCracken/nada) | 0.20.3 | Core audio engine — buffers, DSP, mixing, resampling, analysis, clock, PipeWire capture | shruti, jalwa, aethersafta, tarang, hoosh, streaming app |

### Ranga — Shared Image Processing Core (NEW)

| Field | Value |
|-------|-------|
| Status | **Scaffolding** |
| Priority | Infrastructure — enables dedup across rasa, tazama, aethersafta |
| Repository | `MacCracken/ranga` |

**Why**: Rasa, tazama, and aethersafta all implement overlapping image processing: color space conversions (BT.601 in 3 different implementations), alpha blending (Porter-Duff in 2 implementations), pixel buffer types (3 incompatible types), and color correction (histogram analysis duplicated). Extracting a shared crate eliminates ~2000 lines of duplicate code and ensures consistent behavior.

**What gets extracted**:
- Color math: sRGB↔linear, HSL, BT.601/709 YUV↔RGB, ICC profiles (from rasa-core)
- Blend modes: 12 Porter-Duff modes (from rasa-engine)
- Pixel buffers: unified RGBA/RGB/YUV buffer type with format conversion (replaces 3 types)
- CPU filters: brightness, contrast, saturation, levels, curves (from rasa-engine)
- GPU compute: wgpu abstraction for portable Vulkan/Metal shaders (from rasa-gpu)
- SIMD: SSE2/AVX2/NEON alpha blending (from aethersafta)

**Consumers after extraction**:
- **rasa** → drops rasa-core color math, uses `ranga::color`, `ranga::blend`, `ranga::filter`
- **tazama** → drops manual BT.601, uses `ranga::convert`, `ranga::color_correct`
- **aethersafta** → drops custom alpha blend + color conversion, uses `ranga::blend`, `ranga::convert`

### Future Shared Crates (Planned)

Ideas for additional extractions as the ecosystem matures. Not yet scaffolded.

| Crate (working name) | Domain | Extracts from | Would serve |
|----------------------|--------|---------------|-------------|
| **sluice** | Queue multiplexing, distributed state, fleet messaging | daimon (pubsub, IPC, fleet relay), AgnosAI (fleet placement, task queue) | daimon, AgnosAI, hoosh (request routing), sutra (parallel execution), aethersafta (frame pipeline), streaming app |
| **nein** (German: nine / "no") | Rust-native firewall (neintables) | daimon (nftables rules), aegis (network policy), sutra (nftables module) | AGNOS network stack, edge fleet, sy-agnos sandbox |

#### Sluice — Distributed Queue & Multiplex Engine

**Problem**: Multiple projects implement overlapping queue/messaging patterns:
- **daimon**: PubSub broker (topic matching, subscriber fan-out), fleet relay (dedup, broadcast), agent IPC (Unix sockets, message routing)
- **AgnosAI**: Task queue (priority, DAG scheduling), fleet placement (node ranking, affinity), crew coordination (message passing)
- **hoosh**: Request routing (provider selection, round-robin, failover), response streaming (SSE fan-out)
- **sutra**: Parallel task execution, result collection, dependency resolution

**What it would own**:
- Lock-free MPMC queue (multi-producer, multi-consumer) with priority support
- Topic-based pub/sub with wildcard matching (MQTT-style `+` and `#`)
- Multiplexed connection pool with health checking and failover
- Distributed state primitives (CRDTs or similar for fleet consensus)
- Backpressure and flow control for streaming pipelines
- Message deduplication (bloom filter or seen-set)

**Prior art in the ecosystem**:
- **SecureYeoman A2A network** — battle-tested agent-to-agent protocol: authenticated handshakes, capability discovery, tool delegation, event streaming. Proven at scale with 279 MCP tools across multi-node deployments. Sluice absorbs the protocol patterns.
- **daimon pubsub** — MQTT-style topic matching (`+`/`#` wildcards), subscriber fan-out, fleet relay with dedup. Proven in edge fleet management. Sluice absorbs the routing engine.
- **AgnosAI fleet** — 220ns message overhead, priority DAG scheduling, GPU-aware placement. Proven in benchmarks. Sluice absorbs the raw speed.

Three implementations, three strengths: SY solved auth + discovery, daimon solved topic routing, AgnosAI solved raw speed. Sluice unifies all three.

**Why not just use Redis/NATS/ZeroMQ**: Same reason we built tarang instead of using GStreamer — Rust-native, zero-copy, no external process, no serialization boundary. A shared crate makes 220ns messaging available to everyone without each project reinventing the queue.

**When**: Post-v1.0. Current implementations work. Extraction makes sense once we have 3+ consumers hitting the same patterns.

#### Nein — Rust-Native Firewall (neintables)

**Problem**: AGNOS currently shells out to `nftables` for all network policy — daimon's CORS rules, aegis network isolation, sy-agnos sandbox default-deny, edge fleet policy, sutra's nftables module. Every call spawns a process, parses text output, and hopes the nftables syntax hasn't changed.

**What it would own**:
- Rust-native netfilter interface (nfnetlink sockets, no `nft` CLI dependency)
- Declarative rule builder API: `Nein::chain("input").match_tcp(8090).accept()`
- Atomic rule replacement (transaction-based, like nftables but without the CLI)
- Per-agent network policy (integrated with daimon sandbox profiles)
- CIDR/IP set matching, rate limiting, connection tracking
- Audit integration — all rule changes logged to cryptographic chain

**Why not just keep nftables**: nftables works. But shelling out to `nft` from Rust for every policy change is the same antipattern as shelling out to `vulkaninfo` — process spawn, text parsing, no type safety. A Rust-native firewall speaks nfnetlink directly, same as `nft` does internally, but without the CLI overhead.

**When**: Post-v1.0. nftables serves well through v1.0. Nein becomes interesting when agent-level network policy needs to change at microsecond scale (agent spawn → firewall rule in the same syscall, not a subprocess).

---

## Implementation Notes

- All apps follow [First-Party Standards](first-party-standards.md)
- Priority 1 items should be addressed before beta (Q4 2026)
- Priority 2-3 items strengthen the daily-driver story
- Priority 4-6 items are post-v1.0 or community-contributed
- When an app reaches first release, move its doc from here to `docs/applications/{name}.md`

---

*Last Updated: 2026-03-20*
