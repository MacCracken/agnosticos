# AGNOS Long-Term Application Ecosystem

Roadmap for expanding the AGNOS desktop and tool ecosystem with AI-native applications.
All items follow the consumer project pattern: separate repo, Rust crates, MCP tools, agnoshi intents, marketplace recipe.

> **Last Updated**: 2026-03-16

---

## Completed (Active Consumer Projects)

These are built, integrated, and have marketplace recipes. Listed for reference — not on the roadmap.

| App | Domain | Status |
|-----|--------|--------|
| Tazama | Video editor | Released, 7 MCP tools |
| Rasa | Image editor | Released, 9 MCP tools |
| Mneme | Knowledge base | Released, 7 MCP tools |
| Shruti | DAW / audio | Released, 7 MCP tools |
| Synapse | LLM management | Released, 7 MCP tools |
| BullShift | Trading | Released, 7 MCP tools |
| Delta | Code hosting | Released, 7 MCP tools |
| Aequi | Accounting | Released, 5 MCP tools |
| AGNOSTIC | QA platform | Released, 5 MCP tools |
| SecureYeoman | AI agents | Released, 7 MCP tools |
| Photis Nadi | Productivity | Released, 8 MCP tools |

## Scaffolded (Repos created, CI/CD, tests passing — awaiting first release)

| App | Domain | Tests | Repo |
|-----|--------|-------|------|
| Nazar | System monitor | 27 | `/home/macro/Repos/nazar` |
| Selah | Screenshot / annotation | 50 | `/home/macro/Repos/selah` |
| Abaco | Calculator / units | 61 | `/home/macro/Repos/abaco` |
| Rahd | Calendar / contacts | 49 | `/home/macro/Repos/rahd` |

---

## Priority 1 — Essential Desktop (needed for daily-driver OS)

### Media Player — Jalwa (SCAFFOLDED)
- **Jalwa** (Persian: manifestation/display) — `/home/macro/Repos/jalwa`
- Audio + video playback, playlist management, library browser, smart playlists
- Built on **tarang** — pure Rust audio (symphonia), C FFI video (dav1d/openh264/libvpx)
- AI: Content classification, smart playlists, recommendations, transcription (via hoosh), library insights
- Infrastructure: tarang for decode, PipeWire for output (recipe exists), Wayland for video display (aethersafha)
- 4 crates (core, playback, ui, ai), 66 tests, 5 MCP tools, CI/CD ready
- **Next steps**: Full tarang-audio decode pipeline, PipeWire output, SQLite library persistence, desktop UI
- **Shared with**: Tazama (video), Shruti (audio) — all three use tarang as decode backend

### PDF / Document Suite
- Reader, annotator, form filler, digital signatures
- AI: OCR, summarization, translation, document Q&A, table extraction
- Infrastructure: Tesseract already in Aequi, poppler/mupdf recipes possible
- **Need**: Every OS needs a document viewer. Currently no recipe exists
- **Effort**: Medium — mupdf bindings exist in Rust

### Email Client
- Local-first, privacy-respecting, IMAP/SMTP
- AI: Smart compose, priority inbox, thread summarization, phishing detection (aegis/phylax integration), auto-categorization
- **Need**: Critical for desktop completeness. No email recipe exists
- **Effort**: High — email protocols are complex, but notmuch/aerc patterns exist

### File Manager
- Dual-pane, thumbnail preview, batch rename
- AI: Smart search (semantic file finding via RAG), duplicate detection, auto-tagging by content, predictive organization ("you usually put invoices in ~/Documents/Finance")
- Infrastructure: FUSE already in agnos-sys, inotify available
- **Need**: Every desktop needs one. Currently relying on CLI
- **Effort**: Medium — egui + filesystem ops

### Backup Manager
- Incremental, encrypted, local + remote targets (SSH, S3-compatible)
- AI: Priority-based restore suggestions, anomaly detection (unexpected large changes), smart scheduling (backup when idle)
- Infrastructure: LUKS/dm-verity in agnos-sys, ark package format
- **Need**: Data safety is non-negotiable for a production OS
- **Effort**: Medium — restic/borg patterns, Rust implementation

---

## Priority 2 — Strong Utility (significant daily value)

### Disk Analyzer
- Treemap visualization of disk usage, duplicate finder, large file finder
- AI: Cleanup suggestions ("these 3 GB of build artifacts haven't been touched in 6 months"), safe-delete confidence scoring
- Infrastructure: Existing `/v1/metrics` disk data, filesystem scanning
- **Need**: Storage management is constant on any system
- **Effort**: Low-Medium — mostly filesystem walking + visualization

### Network Manager GUI
- Visual frontend for WiFi/Ethernet/VPN connection management
- Connection profiles, firewall rule editor, bandwidth monitoring
- AI: Security recommendations ("this network has no encryption"), auto-VPN triggers
- Infrastructure: NetworkManager/iwd recipes exist, nftables in kernel
- **Need**: WiFi management without CLI is essential for non-technical users
- **Effort**: Medium — NetworkManager D-Bus bindings

### Log Viewer
- Aggregates journald + daimon audit + phylax findings + agent logs
- Timeline view, filtering, search, tail mode
- AI: Pattern detection ("this error correlates with that service restart"), anomaly highlighting, root cause suggestions via hoosh
- Infrastructure: journald bindings in agnos-sys, audit chain in daimon
- **Need**: Debugging and observability for both users and agents
- **Effort**: Low-Medium — mostly reading existing APIs

### RSS / Feed Reader
- Local-first, offline-capable
- AI: Article summarization, topic clustering, priority sorting, "daily briefing" generation via hoosh, sentiment analysis
- Infrastructure: reqwest for HTTP, RAG pipeline for indexing articles
- **Need**: Information consumption tool, pairs well with Mneme
- **Effort**: Low — Atom/RSS parsing is simple, AI features are the value-add

---

## Priority 3 — Developer & Power User Tools

### Database Browser
- GUI for SQLite, PostgreSQL, Redis (all supported by argonaut)
- Schema visualization, query editor with syntax highlighting
- AI: NL-to-SQL ("show me users who signed up last month"), explain query plans, suggest indexes
- Infrastructure: rusqlite already a dependency, database manager in daimon
- **Need**: Developer tool, useful for debugging agent databases
- **Effort**: Medium

### API Client
- HTTP client with request builder, collections, environments
- AI: Generate requests from API docs, analyze responses, detect breaking changes, mock server generation
- Infrastructure: reqwest, daimon API as built-in collection
- **Need**: Developer tool for working with daimon/hoosh/external APIs
- **Effort**: Medium

### Terminal Multiplexer
- tmux/zellij alternative with native AGNOS integration
- AI: Session suggestions ("you usually open these 3 panes for this project"), command prediction, context-aware shell history
- Infrastructure: agnoshi integration, PTY handling
- **Need**: Power user tool, enhances agnoshi experience
- **Effort**: High — terminal emulation is complex

### Presentation Tool
- Slide deck creation from Markdown or NL
- AI: NL-to-slides ("make a 5-slide pitch deck about X"), auto-layout, speaker notes generation, image suggestions via Rasa
- Infrastructure: Cairo/SVG rendering, PDF export
- **Need**: Nice-to-have, but high AI differentiation
- **Effort**: Medium

---

## Priority 4 — Creative & Specialized

### 3D Modeler / CAD
- Parametric design, mesh editing, STL export
- AI: Text-to-3D, parametric suggestions, topology optimization
- Infrastructure: Vulkan + Mesa ready, GPU compute available
- **Need**: Niche but extremely high AI leverage. No open-source AI-native CAD exists
- **Effort**: Very High — 3D is inherently complex

### Font Manager
- Preview, classify, organize system fonts
- AI: Font pairing suggestions, similarity search, mood/style classification
- Infrastructure: fontconfig recipes exist
- **Need**: Creative tool complement for Rasa/Tazama users
- **Effort**: Low

### Color Picker / Palette Generator
- Screen color sampling, palette creation, gradient builder
- AI: Auto-generate palettes from images, accessibility contrast checker, mood-based palette generation
- Infrastructure: Screen capture API, Wayland color management
- **Need**: Small utility but useful for creative workflow
- **Effort**: Low

---

## Priority 5 — Communication & Collaboration

### Chat / Messaging
- Local-first encrypted messaging between AGNOS users and agents
- AI: Message summarization, smart replies, translation, sentiment analysis
- Infrastructure: PQC crypto in agnos-common, federation module, pubsub broker
- **Need**: Agent-to-human and human-to-human communication layer
- **Effort**: High — E2EE messaging done right is complex

### Video Conferencing
- WebRTC-based, local-network optimized
- AI: Real-time transcription, meeting summarization, action item extraction, background blur/replacement
- Infrastructure: PipeWire/GStreamer recipes, hoosh for transcription
- **Need**: Remote collaboration, but can defer to third-party initially
- **Effort**: Very High

---

## Priority 6 — Future / Exploratory

### Voice Assistant Shell (Vansh)
- Already in named subsystems — TTS/STT voice interface for agnoshi
- AI: Voice commands, dictation, conversational agent interaction
- Infrastructure: PipeWire audio, hoosh for speech models
- **Need**: Accessibility and hands-free operation
- **Effort**: High — speech recognition quality matters

### IoT Dashboard
- Visual management for edge fleet beyond Nazar's system focus
- Device provisioning, sensor data visualization, automation rules
- AI: Predictive maintenance, anomaly detection across fleet
- Infrastructure: Edge module already in daimon, MQTT recipes possible
- **Need**: Expands edge OS profile use case
- **Effort**: Medium

### Game Engine / Runtime
- 2D/3D game runtime with AI NPCs
- AI: Procedural generation, NPC behavior, dynamic difficulty
- Infrastructure: Vulkan, GPU compute, Wayland compositor
- **Need**: Platform stickiness, community building
- **Effort**: Very High

### AI Training Studio
- Visual fine-tuning and dataset management
- AI: Auto-labeling, data augmentation, experiment tracking
- Infrastructure: Synapse integration, finetune module in daimon
- **Need**: Completes the AI-native story — not just inference, but training
- **Effort**: Medium — mostly GUI on existing Synapse/finetune APIs

---

## OS Expectations — Package vs Build

Many desktop essentials already exist as open-source packages. Not everything needs an AI-native rewrite. Strategy:

**Ship as takumi recipe (package existing software):**
Use the existing tool, add a marketplace recipe, optionally wrap with agnoshi intents and MCP tools for AI integration.

| Need | Package | Recipe Status | AI Integration |
|------|---------|---------------|----------------|
| Web Browser | Firefox ESR, Chromium | ✓ Done (`recipes/browser/`) | Aegis phishing, sandboxed |
| Text Editor | Helix, Neovim | Not started | Agnoshi "edit file" intent exists |
| Terminal | Foot (beta), Kitty (post-beta) | Not started | Foot is Wayland-native, minimal. Kitty needs Go 1.26+ (GPU-accelerated, richer features) |
| File Manager | yazi (beta), Thunar (post-beta) | Not started | yazi is Rust TUI, zero GUI deps. Thunar deferred (4 Xfce libs). AI-native GUI file manager is Priority 1 long-term |
| PDF Viewer | Zathura, Evince | Not started | Lightweight, solves 80% of PDF need |
| Archive Manager | File-roller, Xarchiver | Not started | Basic utility |
| Media Player | mpv (via tarang) | Not started | Tarang replaces ffmpeg as media backend; mpv recipe uses tarang for decode pipeline. PipeWire recipes exist |
| Image Viewer | imv, feh, Eye of GNOME | Not started | Selah handles screenshots |
| Disk Utility | GParted, GNOME Disks | Not started | Partition management |
| Bluetooth Manager | Blueman | Not started | BlueZ recipe exists |
| Network Manager | nm-applet, iwgtk | Not started | WiFi/VPN GUI |
| System Settings | Custom needed | Not started | AGNOS-specific config UI |
| Notification Daemon | mako, dunst | Not started | Wayland-native |
| Clipboard Manager | wl-clipboard, cliphist | Not started | Wayland-native |
| Keyring / Secrets | GNOME Keyring, KeePassXC | Not started | Integrates with agnos-common secrets |
| Firewall GUI | Gufw equivalent | Not started | Wraps nftables |
| Printing | CUPS + system-config-printer | Not started | cups.toml recipe exists |
| Fonts | fontconfig + font packages | Partial | Noto, Liberation in recipes |
| Wallpaper / Themes | Custom for aethersafha | Not started | Compositor integration |

**Build AI-native (the long-term roadmap above):**
Only when the AI integration is the *primary value proposition* and can't be achieved by wrapping an existing tool.

**Hybrid approach:**
Some items benefit from both — ship the package now, build AI-native later. For example:
- Ship Zathura now → build AI-native PDF suite later (Priority 1)
- Ship nm-applet now → build AI network manager later (Priority 2)
- Ship Thunar now → build AI file manager later (Priority 1)

---

## Implementation Notes

- All apps follow the proven consumer project pattern: separate repo, Rust crates, 5+ MCP tools, 5+ agnoshi intents, marketplace recipe, `.agnos-agent` bundle, CI/CD with GitHub Actions release pipeline
- Names follow project convention (multilingual: Arabic, Persian, Hebrew, Sanskrit, Greek, Japanese, Swahili, Latin, Italian, Spanish)
- `ark-bundle.sh` auto-fetches latest GitHub releases via `github_release` field in recipes
- Priority 1 items should be addressed before beta (Q4 2026)
- Priority 2-3 items strengthen the daily-driver story
- Priority 4-6 items are post-v1.0 or community-contributed
