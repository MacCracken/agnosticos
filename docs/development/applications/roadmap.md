# Application Development Roadmap

> **Status**: Active | **Last Updated**: 2026-03-31
>
> Future first-party applications planned for the AGNOS ecosystem.
> All follow the [First-Party Standards](first-party-standards.md).
> Released applications are documented in [docs/applications/](../../applications/).
> Shared crates: [shared-crates.md](shared-crates.md) — 81 total (45 at v1.0+, 25 pre-1.0, 16 unpublished).
> Monolith extraction: [monolith-extraction.md](../monolith-extraction.md).

---

## Priority 0 — Active Development

### Sahifa + Scriba — PDF / Document Suite

| Field | Value |
|-------|-------|
| Status | **P0 — Design complete, ready to scaffold** |
| Engine | **sahifa** (صحيفة — page/document) — PDF engine, GPL-3.0-only |
| GUI | **scriba** (Latin: scribe) — desktop app, AGPL-3.0-only |
| Spec | [pdf-suite.md](pdf-suite.md) |

7 feature areas from real Acrobat Pro user: editing, AI assistant, conversion, security/redaction, e-signatures/forms, scan/OCR, document comparison.

---

## Priority 1 — Essential Desktop (needed for daily-driver OS)

---

### Email Client

| Field | Value |
|-------|-------|
| Status | Planned |
| Priority | 1 — critical for desktop completeness |

**Why first-party**: Smart compose, priority inbox, thread summarization, phishing detection (aegis/phylax integration), auto-categorization.

**Scope**: Local-first, privacy-respecting, IMAP/SMTP. AI features via hoosh.

**Effort**: High

---

### File Manager

| Field | Value |
|-------|-------|
| Status | Planned |
| Priority | 1 — ship before beta |

**Why first-party**: Semantic file finding via RAG, duplicate detection, auto-tagging by content, predictive organization.

**Scope**: Dual-pane GUI, thumbnail preview, batch rename. AI via daimon vector store.

**Interim**: yazi (TUI, beta) and Thunar (GUI, bazaar) shipping now.

---

### Backup Manager

| Field | Value |
|-------|-------|
| Status | Planned |
| Priority | 1 — data safety non-negotiable |

**Why first-party**: Priority-based restore, anomaly detection, smart scheduling. OS-level LUKS/dm-verity integration.

**Scope**: Incremental, encrypted, local + remote (SSH, S3-compatible).

---

## Priority 2 — Strong Utility (significant daily value)

### Murti — Core Model Runtime

| Field | Value |
|-------|-------|
| Status | Scaffolded — 0.1.0 |
| Priority | 2 — foundational for hoosh + ifran, Ollama replacement |
| Spec | [murti.md](murti.md) |

Extracts model lifecycle from ifran into shared crate. Enables hoosh as full Ollama replacement.

---

### Tanur — Desktop LLM Studio

| Field | Value |
|-------|-------|
| Status | Scaffolded — 0.1.0 |
| Priority | 2 — desktop GUI for ifran |
| Spec | [tanur.md](tanur.md) |

Native GUI client for ifran. Connects over Unix socket. LM Studio replacement.

---

### Disk Analyzer

| Field | Value |
|-------|-------|
| Status | Planned |
| Priority | 2 |

AI cleanup suggestions, safe-delete confidence scoring, treemap visualization, duplicate finder.

---

### Network Manager GUI

| Field | Value |
|-------|-------|
| Status | Planned |
| Priority | 2 |

Visual WiFi/Ethernet/VPN management, firewall rule editor, bandwidth monitoring. Deep nftables + aegis integration.

**Interim**: nm-applet recipe shipping.

---

### Log Viewer

| Field | Value |
|-------|-------|
| Status | Planned |
| Priority | 2 |

Aggregates journald + daimon audit + phylax findings + agent logs. Pattern detection, anomaly highlighting, root cause suggestions via hoosh.

---

### RSS / Feed Reader

| Field | Value |
|-------|-------|
| Status | Planned |
| Priority | 2 |

Article summarization, topic clustering, priority sorting, daily briefing via hoosh. Pairs with Mneme.

---

## Priority 3 — Developer & Power User Tools

### Database Browser

| Field | Value |
|-------|-------|
| Status | Planned |
| Priority | 3 |

NL-to-SQL, query plan explanation, index suggestions. GUI for SQLite, PostgreSQL, Redis.

---

### API Client

| Field | Value |
|-------|-------|
| Status | Planned |
| Priority | 3 |

HTTP client with request builder, collections, environments. Ships with daimon/hoosh API as built-in collection.

---

### Terminal Multiplexer

| Field | Value |
|-------|-------|
| Status | Planned |
| Priority | 3 |

Session suggestions, command prediction, context-aware shell history. Native agnoshi integration. tmux/zellij alternative.

---

### Presentation Tool

| Field | Value |
|-------|-------|
| Status | Planned |
| Priority | 3 |

NL-to-slides, auto-layout, speaker notes generation, image suggestions via rasa.

---

### Live Streaming / Broadcast Studio

| Field | Value |
|-------|-------|
| Status | Planned |
| Priority | 3 |

AI scene switching, real-time chat moderation, stream health monitoring. Built on aethersafta (compositing), tarang (encoding), ai-hwaccel (GPU detection).

**Prerequisites**: aethersafta v0.8.0+ (RTMP/SRT output).

---

## Priority 4 — Creative & Interactive

### 3D Modeler / CAD

| Field | Value |
|-------|-------|
| Status | Planned |
| Priority | 4 |

Text-to-3D, parametric suggestions, topology optimization. Very High effort.

---

### Font Manager

| Field | Value |
|-------|-------|
| Status | Planned |
| Priority | 4 |

Font pairing suggestions, similarity search, mood/style classification. Low effort.

---

### Color Picker / Palette Generator

| Field | Value |
|-------|-------|
| Status | Planned |
| Priority | 4 |

Auto-generate palettes from images, accessibility contrast checker, mood-based generation. Low effort.

---

### Joshua — Game Manager & Simulation Core

| Field | Value |
|-------|-------|
| Status | Scaffolded — 0.1.0 |
| Priority | 4 |
| Spec | [joshua.md](joshua.md) |

AI-native game manager. NPCs are daimon agents with LLM brains. Headless simulation mode. Sits on kiran (engine) + impetus (physics). Very High effort but built on existing crates.

---

## Priority 5 — Communication & Collaboration

### Chat / Messaging

| Field | Value |
|-------|-------|
| Status | Planned |
| Priority | 5 |

Agent-to-human and human-to-human with PQC encryption. Leverages federation module and pubsub broker.

---

### Video Conferencing

| Field | Value |
|-------|-------|
| Status | Planned |
| Priority | 5 |

Real-time transcription, meeting summarization, action item extraction via hoosh. Very High effort.

---

## Priority 6 — Future / Exploratory

### Voice Assistant Shell (Vansh)

| Field | Value |
|-------|-------|
| Status | Planned |
| Priority | 6 |

TTS/STT voice interface for agnoshi. Already in named subsystems.

---

### IoT Dashboard

| Field | Value |
|-------|-------|
| Status | Planned |
| Priority | 6 |

Visual management for edge fleet beyond Nazar's system focus.

---

### AI Training Studio

| Field | Value |
|-------|-------|
| Status | Planned |
| Priority | 6 |

Visual fine-tuning and dataset management. GUI on ifran/finetune APIs.

---

## Recent Shared Crate Additions (2026-03-31)

New library crates scaffolded this session — these support future apps and deepen the science/culture stack:

| Crate | Domain | Key Consumers |
|-------|--------|---------------|
| **mastishk** | Neuroscience — neurotransmitters, sleep, HPA, DMN | bhava v1.8, bodh, joshua |
| **rasayan** | Biochemistry — enzyme kinetics, metabolism, membrane transport | mastishk, sharira, jivanu |
| **varna** _(1.0.0)_ | Multilingual language — phonemes, scripts, grammar, gematria data (1.3+) | shabda, shabdakosh, sankhya |
| **itihas** | World history — civilizations, eras, events, calendars | sankhya, avatara, joshua |
| **avatara** | Divine archetypes — mythological personality templates | bhava v2.0, joshua, kiran |

Planned (demand-gated, see [main roadmap](../roadmap.md)):
- Geography/GIS, Music theory, Typography/fonts, Nutrition, Economics/finance

---

## Notes

Released applications are documented in [docs/applications/](../../applications/).
Library crates are documented in [docs/applications/libs/](../../applications/libs/).

- All apps follow [First-Party Standards](first-party-standards.md)
- Priority 1 items before beta
- Priority 2-3 strengthen daily-driver story
- Priority 4-6 are post-v1.0 or community-contributed
- Shared crate registry: [shared-crates.md](shared-crates.md) — 81 crates
- Orchestration platform: [k8s-roadmap.md](../k8s-roadmap.md)

---

*Last Updated: 2026-03-31*
