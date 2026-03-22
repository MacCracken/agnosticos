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

### Joshua — Game Engine & Simulation Core

| Field | Value |
|-------|-------|
| Status | Planned |
| Priority | 4 |
| Spec | [joshua.md](joshua.md) |

**Why first-party**: AI-native game engine where NPCs are daimon agents with LLM brains. Headless simulation mode for AI training — agents prove themselves in virtual environments before real deployment. Composes dhvani (audio), ranga (image), tarang (video), majra (networking), kavach (scripting sandbox), t-ron (NPC security).

**Scope**: ECS core, wgpu rendering, rapier physics, LLM-powered NPCs, deterministic replay, headless simulation at unlimited speed, TOML scene format, visual editor (egui), multiplayer via majra.

**Effort**: Very High — but built on existing crates, not from scratch

---

### AI Training Studio

| Field | Value |
|-------|-------|
| Status | Planned |
| Priority | 6 |

Visual fine-tuning and dataset management. GUI on existing Irfan/finetune APIs.

**Effort**: Medium

---

## Shared Crates (Ecosystem Infrastructure)

Standalone crates extracted from AGNOS that the entire ecosystem depends on.
Published to crates.io, used by AGNOS, Irfan, AgnosAI, SecureYeoman, and consumer apps.

| Crate | Version | Description | Consumers |
|-------|---------|-------------|-----------|
| [ai-hwaccel](https://github.com/MacCracken/ai-hwaccel) | 0.21.3 | Universal AI hardware accelerator detection (13 families), quantisation, sharding, training memory estimation | hoosh, daimon, Irfan, AgnosAI, murti, tazama |
| [tarang](https://github.com/MacCracken/tarang) | 0.20.3 | AI-native media framework — 18-33x faster than GStreamer. Audio/video decode, encode, mux, fingerprint, analysis | jalwa, tazama, shruti, aethersafta |
| [aethersafta](https://github.com/MacCracken/aethersafta) | 0.20.3 | Real-time media compositing — scene graph, multi-source capture, HW encoding, streaming output | aethersafha, streaming app, tazama, SY, selah |
| [hoosh](https://github.com/MacCracken/hoosh) | 0.21.3 | AI inference gateway — 15 LLM providers, OpenAI-compatible API, token budgets, caching. Uses murti for local inference | daimon, tarang, aethersafta, agnoshi, AgnosAI, all consumer apps |
| [ranga](https://github.com/MacCracken/ranga) | 0.21.4 | Core image processing — color spaces, blend modes, pixel buffers, filters, GPU compute | rasa, tazama, aethersafta, streaming app |
| [dhvani](https://github.com/MacCracken/dhvani) | 0.20.4 | Core audio engine — buffers, DSP, mixing, resampling, analysis, synthesis, MIDI, clock, PipeWire capture | shruti, jalwa, aethersafta, tarang, hoosh, streaming app |
| [majra](https://github.com/MacCracken/majra) | 0.21.3 | Distributed queue & multiplex engine — pub/sub, priority queues, DAG scheduling, heartbeat FSM, relay, IPC, rate limiting | daimon, AgnosAI, hoosh, sutra, stiva, aethersafta, streaming app |
| [kavach](https://github.com/MacCracken/kavach) | 0.21.3 | Sandbox execution framework — 8 backends (process, gVisor, Firecracker, WASM, OCI, SGX, SEV, SY-AGNOS), strength scoring, policy engine, credential proxy | SY, daimon, stiva, AgnosAI, aethersafta |
| [libro](https://github.com/MacCracken/libro) | 0.21.3 | Cryptographic audit chain — tamper-proof SHA-256 hash-linked event logging, severity levels, agent tracking | daimon, aegis, stiva, sigil, ark |
| [bote](https://github.com/MacCracken/bote) | 0.21.3 | MCP core service — JSON-RPC 2.0, tool registry, schema validation, dispatch. Eliminates 23 duplicate MCP implementations | all consumer apps with MCP tools |
| [szál](https://github.com/MacCracken/szal) | 0.21.3 | Workflow engine — step/flow execution with branching, retry, rollback, parallel stages | daimon, AgnosAI, sutra |
| **murti** | **0.1.0** | **Core model runtime — registry, store, pull, 15 inference backends, GPU allocation. Extracted from Irfan** | **hoosh, Irfan** |
| **stiva** | **0.1.0** | **OCI container runtime — image management, container lifecycle, overlay FS. Builds on kavach + majra** | **daimon, sutra** |
| **nein** | **0.1.0** | **Programmatic nftables firewall — rule builder, NAT, network policies, container networking** | **stiva, daimon, aegis, sutra** |

### Ranga — Shared Image Processing Core

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

### Recently Completed / Scaffolded Crates

The following crates were previously listed as "future" but are now done or actively scaffolded:

| Crate | Status | Notes |
|-------|--------|-------|
| **majra** | Released (0.21.3) | Replaced planned "sluice" crate. Pub/sub, priority queues, DAG scheduling, heartbeat FSM, relay, IPC, rate limiting, SQLite persistence. Benchmarked, proptested |
| **kavach** | Released (0.21.3) | 8 sandbox backends, strength scoring (0-100), policy engine, credential proxy, lifecycle management, externalization gate |
| **nein** | Scaffolded (0.1.0) | nftables rule builder, NAT, network policies, container bridge builders. 24 tests. [README](https://github.com/MacCracken/nein) |
| **stiva** | Scaffolded (0.1.0) | OCI container runtime. Builds on kavach + majra. 17 tests. [README](https://github.com/MacCracken/stiva) |
| **murti** | Scaffolded (0.1.0) | Core model runtime, extracted from Irfan. 21 tests. [Spec](murti.md) |

See [k8s-roadmap.md](../k8s-roadmap.md) for how stiva + nein + majra + kavach compose into a k8s-equivalent orchestration platform.

#### Stiva — Rust-Native Container Runtime

| Field | Value |
|-------|-------|
| Status | Scaffolded (0.1.0) |
| Priority | Infrastructure — accelerated to pre-v1.0 |
| Spec | [stiva.md](stiva.md) |

**Problem**: AGNOS depends on Docker/Podman (100MB+ daemon) for container workloads — sy-agnos sandbox images, edge deployment, development containers. The container runtime is the one major system component that isn't Rust-native.

**What it would own**:
- OCI image format — pull, unpack, layer management (replace `docker pull`)
- Container lifecycle — create, start, exec, stop, remove (replace `docker run`)
- Namespace/cgroup isolation — direct kernel API, no shim process
- Image building — Dockerfile-compatible or native TOML format
- Registry client — pull/push from OCI registries (GHCR, Docker Hub)
- Network — bridge/host/none modes via nein (Rust firewall)
- Storage — overlayfs layer management, snapshotting

**What already exists in the ecosystem**:
- **kavach** (v0.21.3) — 8 sandbox backends (Process, gVisor, Firecracker, WASM, OCI, SGX, SEV, SyAgnos) with strength scoring, seccomp/Landlock/namespace isolation, externalization gate
- **argonaut** — process lifecycle, service management, init sequencing
- **ark** — package format with signing and verification (`.ark`, `.agnos-agent`)
- **nein** (0.1.0) — nftables firewall for container network policy (24 tests)
- **majra** — container IPC, event bus, health monitoring

**Architecture**: stiva becomes a thin orchestration layer over kavach (isolation), nein (networking), and ark (image format). The actual container = kavach sandbox + nein network namespace + ark image layers.

```
stiva (container runtime)
  ├── kavach  (isolation: namespaces, cgroups, seccomp, landlock, caps)
  ├── nein    (networking: bridge, port mapping, DNS)
  ├── ark     (images: layers, registry, signing)
  └── libro   (audit: container lifecycle events)
```

**Why not just keep Docker**: Docker works. But a 100MB Go daemon managing containers that run Rust binaries is the same abstraction mismatch as GStreamer managing media pipelines. stiva would be <5MB, start in milliseconds, and speak the same types as every other AGNOS component.

**Security uplift for sy-agnos**: When stiva replaces docker/podman as the sy-agnos container runtime, the sandbox strength score increases from 80–88 to **92–95**. The gains come from eliminating trust boundaries that docker/podman introduce:

| Feature | Docker/Podman | Stiva | Strength Boost |
|---------|--------------|-------|---------------|
| Runtime attestation | None — trust the daemon binary | Signed binary hash verified at launch | +3 |
| Image verification | Registry trust (MITM-able) | ark-signed squashfs, reject unsigned images | +2 |
| Seccomp enforcement | Runtime-applied, overridable via config | Baked into runtime binary, no override API | +2 |
| Escape hatches | `--privileged`, `--cap-add`, etc. | No privilege escalation flags exist | +2 |
| Daemon attack surface | dockerd: ~50MB Go daemon, root, REST API | Daemonless single binary, <5MB | +2 |
| Syscall surface | containerd → runc shim chain (3 processes) | Direct clone() → exec, no shims | +1 |

Docker/podman are general-purpose: they're designed for developer ergonomics, not adversarial isolation. `runc` has had repeated CVEs (Leaky Vessels, CVE-2024-21626). Stiva eliminates this entire class of vulnerabilities by:
1. No configuration overrides — the runtime enforces kavach policy, period
2. No daemon — no long-running root process to attack
3. Image = signed squashfs — no layer unpacking, no registry trust, no manifest poisoning
4. Runtime itself is attested — signed hash verified before first container launches

These layers are **composable**, not mutually exclusive. The strongest possible configuration stacks all of them:

```
Firecracker (KVM microVM)        — hardware isolation boundary
  └── jailer (cgroup, seccomp, chroot) — privilege reduction
      └── stiva (attested runtime)     — no daemon, signed binary, no overrides
          └── sy-agnos (OS sandbox)    — immutable rootfs, baked seccomp/nftables
              └── TPM measured boot    — hardware-attested integrity chain
```

**Strength scoring for composed configurations:**
```
Firecracker alone                           = 90
Firecracker + jailer                        = 93
sy-agnos tpm_measured + stiva               = 95
Firecracker + jailer + stiva + sy-agnos TPM = 98  (near-theoretical max)
```

The top configuration achieves defense-in-depth from hardware (KVM + TPM) through runtime (jailer + stiva) to OS (sy-agnos), with no general-purpose layer where attackers can find configuration mistakes or known CVEs. Every layer is purpose-built, attested, and policy-enforced by kavach.

**Kavach integration**: kavach's SyAgnos backend already detects docker/podman and will detect stiva as a first-class runtime when available. The `SyAgnosTier` enum maps to strength scores, and stiva adds a runtime attestation modifier on top:
```
sy-agnos minimal + docker  = 80
sy-agnos minimal + stiva   = 92  (+12: runtime attestation, image signing, no overrides, no daemon)
sy-agnos dmverity + stiva  = 94
sy-agnos tpm_measured + stiva = 95
```

**Why this matters for AGI**:

The infrastructure AGI runs on cannot be the infrastructure built for web apps. Fifty years of software engineering taught us what to stop accepting:

| Era | What we accepted | What AGNOS does instead |
|-----|-----------------|------------------------|
| 1970s | C memory unsafety | Rust ownership — entire classes of CVEs eliminated at compile time |
| 1990s | Shell out to CLI tools | Direct API calls — tarang 33x over GStreamer, ai-hwaccel replaces vulkaninfo |
| 2000s | 100MB runtime daemons | <5MB purpose-built binaries — stiva replaces Docker |
| 2010s | "Secure by configuration" | Secure by construction — kavach has no override flags |
| 2015s | Python for everything | Rust for everything — 227,000x fleet messaging over CrewAI |
| 2020s | Trust the container runtime | Attest the container runtime — libro audit chain + TPM measured boot |

AGI agents need infrastructure where the orchestration overhead is zero, the security is provable, the audit trail is tamper-proof, and the entire stack is attested from hardware to application. That's not Docker + Python + Redis. That's stiva + kavach + majra + hoosh + libro — purpose-built, composed, verified.

stiva isn't just a Docker replacement. It's the runtime layer that makes trustworthy autonomous agent execution possible. An AGI system that can't prove its own integrity can't be trusted with autonomous action. stiva + kavach + libro + TPM gives you that proof.

**Timeline**: Scaffolded 2026-03-21. Phase 1 (image pull) targeted for v1.0. Full container execution post-v1.0.

---

## Implementation Notes

- All apps follow [First-Party Standards](first-party-standards.md)
- Priority 1 items should be addressed before beta (Q4 2026)
- Priority 2-3 items strengthen the daily-driver story
- Priority 4-6 items are post-v1.0 or community-contributed
- When an app reaches first release, move its doc from here to `docs/applications/{name}.md`

---

*Last Updated: 2026-03-22*
