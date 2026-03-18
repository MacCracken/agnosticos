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

## Implementation Notes

- All apps follow [First-Party Standards](first-party-standards.md)
- Priority 1 items should be addressed before beta (Q4 2026)
- Priority 2-3 items strengthen the daily-driver story
- Priority 4-6 items are post-v1.0 or community-contributed
- When an app reaches first release, move its doc from here to `docs/applications/{name}.md`

---

*Last Updated: 2026-03-18*
