# AGNOS Development Roadmap

> **Status**: Pre-Beta | **Last Updated**: 2026-03-29
> **Userland complete** — 11000+ tests (3900+ agent-runtime, 1554 ai-shell), ~84% coverage, 0 warnings
> **Recipes**: 116 base + 71 desktop + 25 AI + 9 network + 8 browser + 59 marketplace + 4 python + 3 database + 31 edge + 3 sandbox = 330 OS (+ 90 bazaar community)
> **Build order**: 178 packages in `recipes/build-order.txt` (base + desktop, dependency-ordered)
> **Phases 10–14 complete** | **Phase 15A**: Core scanning done (phylax) | **Phase 16A**: Desktop essentials done | **Phase 17**: Local inference optimization (planned) | **Audit**: 16 rounds
> **MCP Tools**: 151 built-in + external registration
> **Consumer Projects**: 19+ released (including Vidhana v1, Sutra v1, Abacus)
> **Shared Crates**: 63 library crates — 34 at v1.0+ stable, 29 pre-1.0
> **Sandbox**: 7 backends (Native, gVisor, Firecracker, WASM, SGX, SEV, Noop) + credential proxy + externalization gate

---

## Strategic Vision

AGNOS becomes a real operating system in two stages:

1. **OS Independence** (Beta) — AGNOS boots and builds itself without any host distro. Self-hosting LFS-style base, takumi recipes for the full stack, ark as sole package manager. This is the foundation.

2. **Desktop Completeness** (v1.0) — Ship a complete desktop experience by packaging existing open-source tools first (Thunar, Zathura, Alacritty, etc.), then progressively replace with AI-native alternatives where the AI is the primary value.

**Priority order**: OS identity → desktop essentials via recipes → AI-native apps

---

## Beta Goal

AGNOS boots as an **independent Linux distribution** — no Debian, no Ubuntu, no
host distro. A self-hosting LFS-style base system built entirely from source via
takumi recipes, with ark as the sole package manager. The userland (daimon,
hoosh, agnoshi, aethersafha, etc.) runs on top of a base system we control from
toolchain to init.

Reference: [Linux From Scratch 12.4](https://www.linuxfromscratch.org/lfs/view/stable/)
(77 packages) + [Beyond LFS](https://www.linuxfromscratch.org/blfs/view/stable/)
for desktop/networking/GPU stack.

---

## Critical Path to Beta

```
Phase 13A (self-hosting) ──→ Phase 16 (desktop recipes) ──→ Phase 13C (community) ──→ BETA
         │                            │
         │                            └── Package existing tools (Thunar, Zathura, etc.)
         │                                so the desktop is usable
         │
         └── AGNOS builds AGNOS: toolchain, kernel, userland, packages
             This is THE beta blocker
```

---

## Completed Phases (Summary)

| Phase | Key Deliverables |
|-------|------------------|
| 0-4 | Foundation through Desktop |
| 5-5.6 | Production hardening, all stubs eliminated |
| 6-6.8 | Hardware acceleration, swarm, networking tools, RAG, RPC, OpenTelemetry |
| 7 | Federation, migration, scheduling, ratings |
| 8A-8M | Distribution, PQC, AI safety, formal verification, RL |
| 9-9.5 | Cloud services, human-AI collaboration, OIDC, delegation, vector REST, marketplace |
| **10** | **LFS base system** — 108 recipes (cross-toolchain, core utils, system libs, security, init, build tools, kernel) |
| **11** | **Desktop & networking stack** — 88 recipes (graphics, audio, networking, desktop libs, AI/ML infra) |
| **12** | **System integration** — argonaut init (117 tests), ark package manager (49 tests), agnova installer (91 tests), CI/CD |
| **13B** | **Hardware support** — NVIDIA (proprietary + nouveau), AMD, Intel, WiFi, Bluetooth, Thunderbolt, printing |
| **13D** | **Consumer app integration** — 11 apps with MCP tools + agnoshi intents |
| **13E** | **CI, WebView, containers, Python** — browser-ark CI, marketplace-publish CI, Docker base images, Python runtime |
| **14** | **Edge OS Profile** — Edge boot mode, fleet management, 29 edge recipes, Docker container (35.5 MB) |
| **15A** | **Phylax core** — YARA engine (65 tests), entropy analysis, magic bytes, 5 API endpoints, 5 MCP tools, 5 agnoshi intents |
| **16A** | **Desktop essentials** — 10 packaged tools: foot, helix, yazi, fuzzel, mako, zathura, imv, mpv, cliphist + ark CLI |
| **16C** | **System configuration** — Vidhana v1 (system settings), display/audio panels. nm-applet/blueman/firewall in bazaar |

---

## Phase 13A — Self-Hosting Validation (BETA BLOCKER)

**This is the single most important remaining work.** Without it, AGNOS is a Debian overlay.

### Infrastructure (COMPLETE)

All 8 infra items done: bootstrap-toolchain.sh, enter-chroot.sh, ark-build.sh, selfhost-validate.sh (+ Rust module, 38 tests), 116 base recipes, source tree in ISO, build-selfhosting-iso.sh.

### Validation (Remaining — requires real hardware/QEMU execution)

| # | Item | Status | Notes |
|---|------|--------|-------|
| 1 | Run bootstrap-toolchain.sh end-to-end | Not started | Build cross-compiler from source tarballs |
| 2 | Build base system in chroot | Not started | ark-build all 109 base recipes in order |
| 3 | Build AGNOS userland on target | Not started | `cargo build --release --workspace` inside AGNOS |
| 4 | Build kernel modules on target | Not started | Compile AGNOS kernel modules without host |
| 5 | Selfhost-validate passes all phases | Not started | Run `selfhost-validate --phase all` on booted ISO |
| 6 | CI automation | In progress | GitHub Actions: `publish-toolchain.yml`, `selfhost-build.yml`, `selfhost-validation.yml` — bootstrap toolchain added to CI/CD |

**Critical path**: Download tarballs → bootstrap-toolchain.sh → enter-chroot.sh → ark-build recipes → cargo build userland → selfhost-validate

**To attempt now**: `sudo LFS=/mnt/agnos ./scripts/build-selfhosting-iso.sh`

---

## Phase 16 — Desktop Completeness (NEW)

**Strategy**: Package existing open-source tools via takumi recipes to provide a complete desktop experience *now*. AI-native replacements come later (see `docs/development/applications/roadmap.md`).

### 16A — Essential Desktop Packages (COMPLETE)

All 10 ship-with-ISO packages done: yazi (file manager), foot (terminal), helix (editor), zathura (PDF), imv (images), mpv (media), mako (notifications), cliphist (clipboard), fuzzel (launcher), ark CLI (archives).

### 16B — Input & Hardware Detection

| # | Need | Approach | Status | Notes |
|---|------|----------|--------|-------|
| 1 | Touchscreen detection | libinput + udev rules | Not started | Auto-detect touch devices, enable tap-to-click, gesture support in aethersafha |
| 2 | Touch gestures | libinput-gestures or custom | Not started | Pinch-zoom, swipe between workspaces, three-finger drag |
| 3 | On-screen keyboard | squeekboard or custom | Not started | Required for tablet/all-in-one without physical keyboard |
| 4 | HiDPI / scaling | Wayland fractional scaling | Not started | Auto-detect display DPI, set appropriate scale factor |
| 5 | Stylus / pen input | libinput tablet support | Not started | Pressure sensitivity, palm rejection |

### 16C — System Configuration (COMPLETE)

Vidhana v1 covers settings UI, display, and audio panels. Network/Bluetooth/firewall GUIs available via bazaar (`ark bazaar install nm-applet`, `blueman`, `firewall-config`).

### 16D — Desktop Polish

| # | Need | Package | Status | Notes |
|---|------|---------|--------|-------|
| 1 | Wallpaper/Themes | Custom for aethersafha | Not started | Default AGNOS theme, wallpaper selector |
| 2 | Fonts | Noto + Liberation + JetBrains Mono | Partial | Some fonts in existing recipes |
| 3 | Icons | Papirus or custom | Not started | Icon theme for desktop |
| 4 | Cursor theme | Adwaita or custom | Not started | Wayland cursor theme |
| 5 | GTK theme | Adwaita dark variant | Not started | For XWayland GTK apps |
| 6 | Keyring / Secrets | KeePassXC | **Bazaar** | `ark bazaar install keepassxc` |
| 7 | Printing GUI | system-config-printer | **Bazaar** | `ark bazaar install system-config-printer`. CUPS daemon in OS |
| 8 | Disk utility | GParted / GNOME Disks | **Bazaar** | `ark bazaar install gparted` |

### 16E — Aethersafha Configurability

| # | Item | Status | Notes |
|---|------|--------|-------|
| 1 | User-facing config file | Not started | Hyprland-style config DSL or TOML for keybinds, gaps, borders, animations |
| 2 | Session selector in argonaut | Not started | TTY chooser or mini display manager. Select aethersafha, sway, hyprland (from bazaar) |
| 3 | Hot-reload config | Not started | Watch config file, apply changes without restart |
| 4 | Plugin API for bars/widgets | Not started | External status bars (waybar) can integrate via IPC protocol |

### 16F — Aethersafha Media Ingestion & Compositing

Upgrades to `ScreenCaptureManager` and `ScreenRecordingManager` to support real-time compositing, multi-source capture, and encoding — enabling aethersafha to serve as the capture/compositing backend for streaming (OBS-style), screen recording, and video conferencing workloads.

**Extracted as**: [**aethersafta**](https://github.com/MacCracken/aethersafta) — standalone crates.io crate (`aethersafta = "0.20"`). Repo scaffolded with scene graph, source/encode/output traits, timing, CLI, CI/CD, architecture docs, and v0.5–v1.0 roadmap. Consumers: aethersafha, streaming app, tazama, video conferencing, SecureYeoman (sandbox session recording + security overlay annotations).

| # | Item | Status | Notes |
|---|------|--------|-------|
| 1 | Multi-source capture | Not started | Capture multiple surfaces simultaneously (windows, monitors, cameras). Current `ScreenCaptureManager` does single-surface snapshots |
| 2 | Camera device ingestion | Not started | V4L2 camera capture (webcam, capture cards) as compositor surfaces. Expose via `/v1/screen/sources` API |
| 3 | Real-time compositing layer | Not started | Scene graph with z-ordered layers (screen, camera, overlay, text). Alpha blending, positioning, scaling. Frame-accurate mixing at output framerate |
| 4 | PipeWire audio capture | Not started | Per-source audio capture alongside video. Mix multiple audio streams (mic, desktop, app). Pairs with shruti DSP for noise suppression |
| 5 | Hardware-accelerated encoding | Not started | Integrate ai-hwaccel for GPU-aware encode path selection (NVENC, VA-API, QSV, AMF). Route through tarang for container muxing |
| 6 | Streaming output (RTMP/SRT) | Not started | Network output from composited frames + mixed audio. RTMP for Twitch/YouTube, SRT for low-latency. Tarang handles muxing |
| 7 | Recording output | Not started | Extend `ScreenRecordingManager` to record composited scenes (not just raw frames). Tarang encodes to MP4/MKV/WebM |
| 8 | Overlay rendering | Not started | Text overlays, image watermarks, animated transitions. Rendered in compositor before encoding |
| 9 | Source switching API | Not started | Scene presets with instant/animated transitions. IPC commands for external control (stream deck, agnoshi "switch to camera scene") |
| 10 | Latency budget tracking | Not started | Per-frame timing: capture → composite → encode → output. Alert when total pipeline exceeds target (e.g. 33ms for 30fps). Nazar integration |

---

## Bazaar — Community Package Repository

**Subsystem**: bazaar (Persian: بازار). Repo: `github.com/MacCracken/bazaar`. Recipe: `recipes/base/bazaar.toml`.

**90 recipes** across 8 categories:

| Category | Count | Highlights |
|----------|-------|------------|
| AI | 13 | ollama, llama.cpp, whisper.cpp, stable-diffusion.cpp, onnxruntime, vllm, piper-tts, aider, open-webui, comfyui, fabric, lmstudio, pytorch |
| Desktops | 35 | Sway (5), Hyprland (8), shared Wayland tools, Thunar, Evince, Blueman, nm-applet, dunst, feh, GParted, GNOME Disks, firewall-config, system-config-printer, GTK3/Qt5/libadwaita libs |
| Tools | 21 | ripgrep, fd, bat, eza, fzf, tmux, htop, btop, lazygit, starship, zoxide, dust, tokei, hyperfine, git-delta, docker, podman, k9s, syncthing, gimp, inkscape, libreoffice |
| Editors | 3 | neovim, vim, micro |
| Networking | 4 | wireguard-tools, bandwhich, mtr, tailscale |
| Security | 3 | keepassxc, age, pass |
| Media | 4 | ffmpeg, yt-dlp, obs-studio, audacity |
| Games | 1 | retroarch |

---

## Phase 13C — Community & Documentation

| # | Item | Status | Notes |
|---|------|--------|-------|
| 1 | Video tutorials | Not started | Installation, usage, agent creation (needs recording) |
| 2 | Support portal | Not started | Discord + forum (needs external setup) |
| 3 | Community testing program | Not started | Beta tester enrollment (needs external setup) |
| 4 | Third-party security audit | Not started | External vendor (needs procurement) |

---

## Phase 15 — Threat Detection & Scanning

**Subsystem**: **phylax** (Greek: guardian/watchman) — `agent-runtime/src/phylax.rs`

### 15A — Core Scanning Engine (5/7 COMPLETE)

Done: YARA engine (65 tests), file content inspection, 5 scan API endpoints, 5 MCP tools, 5 agnoshi intents.

| # | Item | Status | Notes |
|---|------|--------|-------|
| 3 | Signature database (`.phylax-db`) | Not started | Signed, versioned threat definitions distributed via ark |
| 4 | On-access scanning (fanotify) | Not started | Real-time filesystem monitoring via `agnos-sys` fanotify bindings |

### 15B — AI-Powered Analysis

| # | Item | Status | Notes |
|---|------|--------|-------|
| 1 | ML binary classifier | Not started | ONNX model for static binary analysis (benign vs suspicious) |
| 2 | LLM-assisted triage | Not started | Route suspicious findings through hoosh for natural language explanation |
| 3 | Behavioral fingerprinting | Not started | Per-agent behavioral profiles; extends anomaly.rs baselines |
| 4 | Entropy analysis | Not started | Detect ransomware patterns (rapid encryption, high-entropy writes) |
| 5 | Supply chain analysis | Not started | Dependency graph scanning for known-vulnerable libs in `.ark`/`.agnos-agent` |

### 15C — Response & Remediation

| # | Item | Status | Notes |
|---|------|--------|-------|
| 1 | Enhanced quarantine | Not started | Extend aegis quarantine with content isolation + forensic snapshot |
| 2 | Automated response policies | Not started | Configurable actions per threat level (alert/quarantine/kill/rollback) |
| 3 | Threat intelligence feeds | Not started | Optional external feed ingestion (STIX/TAXII compatible) |
| 4 | Scan reports & dashboard | Not started | `/v1/scan/reports`, integration with aethersafha security UI |
| 5 | Edge-optimized scanning | Not started | Lightweight rule subset for constrained devices; fleet-wide threat propagation |

### Architecture

```
┌─────────────────────────────────────────────────────┐
│  phylax — Threat Detection Engine                   │
├──────────┬──────────┬──────────┬────────────────────┤
│  YARA    │  ML      │  Entropy │  Behavioral        │
│  Rules   │  Binary  │  Analysis│  Fingerprint       │
│  Engine  │  Classif.│         │                    │
├──────────┴──────────┴──────────┴────────────────────┤
│  fanotify (real-time)  │  on-demand  │  periodic    │
├────────────────────────┴────────────┴───────────────┤
│  aegis (policy + quarantine)  │  hoosh (LLM triage) │
└───────────────────────────────┴─────────────────────┘
```

---

## Phase 13F — Hardware Testing Matrix

| # | Target | Arch | Profile | Status | Notes |
|---|--------|------|---------|--------|-------|
| 1 | QEMU x86_64 | x86_64 | Desktop | Done | Verified 2026-03-13, live boot, all binaries functional |
| 2 | Raspberry Pi 4 | aarch64 | Full | Ready | Binary ready; `build-iso-aarch64.sh` created; dd to microSD |
| 3 | Intel NUC (bare metal) | x86_64 | Desktop | Not started | UEFI boot, GPU driver validation |
| 4 | Older x86_64 (~2014 era) | x86_64 | CLI | Not started | Minimum viable hardware floor test |
| 5 | Older desktop w/ touchscreen (~2014) | x86_64 | Desktop | Not started | Touch input + Wayland validation |
| 6 | AWS DeepLens | x86_64 | Edge | Ready | Intel Atom x5-Z8350, 8GB RAM |
| 7 | ARM64 SBC (QEMU) | aarch64 | Edge | Not started | QEMU aarch64 virt machine validation |
| 8 | ESP32-S3 (MCU) | xtensa | Edge/IoT | Recipe done | MQTT agent, sensor telemetry, TinyML. Recipe: `recipes/edge/esp32-agent.toml`. Needs source repo + hardware flash test |
| 9 | ESP32-C3 (MCU) | riscv32 | Edge/IoT | Recipe done | RISC-V core, lowest power, WiFi + Thread/Zigbee. Same recipe, secondary target |
| 10 | Tiiny AI Pocket Lab | aarch64/riscv64? | Edge+AI | Not started | Pocket AI inference appliance. ~16-32GB LPDDR5X, custom SoC (possibly ARM or RISC-V with NPU). Runs 120B int4 @ 20 tok/s stock. Target: boot AGNOS Edge, run hoosh+murti, join fleet. See Phase 17D |

---

## Phase 13G — Consumer App Validation

| # | App | MCP Tools | Intents | Release | Bundle Test | Notes |
|---|-----|-----------|---------|---------|-------------|-------|
| 1 | SecureYeoman | 14 yeoman_* | 14 | Yes | Not started | Flagship (7 core + 7 bridge: tools, brain, tokens, events, swarm) |
| 2 | Photis Nadi | 8 photis_* | 8 | Yes (2026.3.18-1) | Not started | Native binary (migrated from Flutter) |
| 3 | BullShift | 7 bullshift_* | 7 | Yes | Not started | Trading |
| 4 | Agnostic | 23 agnostic_* | 14 | Yes | Not started | Agent automation (Python/CrewAI) → native binary migration planned |
| 5 | Delta | 7 delta_* | 7 | Yes | Not started | Code hosting |
| 6 | Aequi | 7 aequi_* | 7 | Yes (2026.3.18) | Not started | Accounting |
| 7 | Irfan | 7 irfan_* | 7 | Yes | Not started | LLM management (formerly Synapse) |
| 8 | Shruti | 7 shruti_* | 7 | Yes | Not started | DAW |
| 9 | Tazama | 7 tazama_* | 7 | Yes (2026.3.18-1) | Not started | Video editor |
| 10 | Rasa | 9 rasa_* | 9 | Yes | Not started | Image editor |
| 11 | Mneme | 7 mneme_* | 7 | Yes | Not started | Knowledge base |
| 12 | Nazar | 5 nazar_* | — | Yes | Not started | System monitor |
| 13 | Selah | 5 selah_* | — | Yes (MVP) | Not started | Screenshot, no AI integration yet |
| 14 | Abaco | 5 abaco_* | — | Yes | Not started | Calculator |
| 15 | Rahd | 5 rahd_* | — | Yes | Not started | Calendar |
| 16 | Tarang | 8 tarang_* | 8 | Yes | Not started | Media framework (73 tests) |
| 17 | Jalwa | 8 jalwa_* | 8 | Yes | Not started | Media player (110+ tests), built on tarang |
| 18 | Vidhana | 5 vidhana_* | 5 | Yes (v1 2026.3.18) | Not started | System settings (76+ tests), egui GUI, NL control, port 8099 |
| 19 | Sutra | 6 sutra_* | 6 | v1 | Not started | Infrastructure orchestrator (70 tests), 6 core modules, SSH transport, Tera templating, parallel execution, JSON output, MCP handlers, sutra-community (5 modules) |

---

## SecureYeoman & Agnostic Integration

*Cross-project integration items for the AGNOS ecosystem.*

### SecureYeoman Shared Crate Adoption

SY currently bundles its own implementations for media, inference, hardware detection, and agent networking. As the shared crate ecosystem matures, SY adopts them — reducing SY's codebase while improving capability.

| # | Priority | Item | SY replaces | With crate | Status |
|---|----------|------|-------------|------------|--------|
| SY1 | High | Hardware detection | Internal GPU detection | `ai-hwaccel` | Ready — ai-hwaccel 0.20.3 published |
| SY2 | High | Inference gateway | Internal LLM routing | `hoosh` (client) | Ready — hoosh 0.20.4 published |
| SY3 | Medium | Sandbox session recording | Custom screen capture | `aethersafta` | Pending — aethersafta 0.20.3 publishes tomorrow |
| SY4 | Medium | Media processing in tasks | Internal ffmpeg shelling | `tarang` | Ready — tarang 0.20.3 published |
| SY5 | Medium | Image processing in tools | Internal sharp/jimp | `ranga` (via WASM or FFI) | Planned — ranga is Rust-native, needs WASM or FFI bridge for SY's Bun runtime |
| SY6 | Low | Audio in agent workflows | None (not supported) | `dhvani` | Planned — enables audio analysis in SY agent tasks |
| SY7 | Low | Agent-to-agent protocol | Custom A2A implementation | `sluice` (future) | Planned — SY's A2A patterns feed into sluice design, then SY adopts sluice |

### SecureYeoman → Ecosystem Handoff

Patterns SY has proven that should flow back into shared crates:

| Pattern | Current home | Target crate | What SY proved |
|---------|-------------|--------------|----------------|
| A2A authenticated handshake | SY agent protocol | sluice | Mutual auth, capability exchange, trust scoring between agents |
| A2A tool delegation | SY MCP bridge | sluice | Remote tool invocation with sandboxed execution and result streaming |
| A2A event streaming | SY SSE bus | sluice | Real-time event fan-out across nodes with backpressure |
| Sandbox strength scoring | SY sandbox framework | daimon/aegis | Quantitative security scoring (0-100) for execution environments |
| MCP tool discovery | SY 279-tool registry | daimon mela | Dynamic tool registration, capability querying, version negotiation |
| Agent observability | SY dashboard | nazar | Real-time agent metrics, task timeline, resource usage visualization |

### Agnostic Integration — COMPLETE

*All 13 items resolved. See CHANGELOG `[2026.3.17]`.*

---

## Engineering Backlog

*Completed items archived in [sprint-history.md](sprint-history.md).*

### Active — Build & Distribution

| # | Priority | Item | Notes |
|---|----------|------|-------|
| B1 | High | Selfhost pipeline builds all 176 packages | `selfhost-build.yml` updated, needs first full run |
| B2 | High | RPi4 hardware boot test | Firmware blobs added, needs physical validation |

### Active — ESP32 Edge/IoT

| # | Priority | Item | Notes |
|---|----------|------|-------|
| E1 | Medium | ESP32 agent source repo | Recipe created (`recipes/edge/esp32-agent.toml`), MQTT bridge done (E2). Pending: source repo (`MacCracken/esp32-agent`) + firmware code |

### Active — Sandbox & Security

| # | Priority | Item | Notes |
|---|----------|------|-------|
| S2 | Medium | SGX/SEV hardware validation | Backends implemented, need hardware to test |

*Completed backlog items archived in [sprint-history.md](sprint-history.md).*

### Blocked — AgnosAI Integration

**AgnosAI** — Rust-native agent orchestration engine (`/home/macro/Repos/agnosai`). Replaces Python/CrewAI with compiled Rust: real concurrency (tokio), <50MB binary, <2s boot, task DAGs with priority + preemption, native fleet distribution, sandboxed tool execution (WASM/seccomp/Landlock/OCI). 9 crates. Will be open-sourced as a CrewAI competitor.

**Blocked on**: AgnosAI v1 release + Agnostic integration testing.

| # | Priority | Item | Notes |
|---|----------|------|-------|
| A1 | High | AgnosAI marketplace recipe | `recipes/marketplace/agnosai.toml` — native-binary, `MacCracken/agnosai` |
| A2 | High | AgnosAI MCP tools in daimon | Replace/extend `agnostic_*` tools with native AgnosAI bridge (crew management, task dispatch, fleet coordination) |
| A3 | High | AgnosAI agnoshi intents | NL crew control via agnoshi → AgnosAI MCP tools |
| A4 | Medium | Agnostic native binary migration | Agnostic swaps Python/CrewAI for AgnosAI engine. Recipe runtime: `python-container` → `native-binary` |
| A5 | Medium | AgnosAI ↔ hoosh integration | AgnosAI's `agnosai-llm` crate routes through hoosh for unified token budgeting, provider selection, and cost tracking |
| A6 | Low | AgnosAI fleet ↔ daimon edge | AgnosAI's `agnosai-fleet` coordinates with daimon edge module for distributed crew execution across AGNOS nodes |

---

## sy-agnos — OS-Level Sandbox for SecureYeoman (COMPLETE)

All 3 phases complete. SY strength 88. See [SY ADR 044](https://github.com/MacCracken/secureyeoman/blob/main/docs/adr/044-sy-agnos-sandbox.md) and CHANGELOG entries `[2026.3.18]` for details.

- **Phase 1** — Immutable rootfs, baked seccomp + nftables (strength 80). 3 recipes + build script + Dockerfile
- **Phase 2** — dm-verity tamper detection (strength 85)
- **Phase 3** — TPM measured boot + `/v1/attestation` endpoint (strength 88)

---

## Release Roadmap

### Beta Release — Q4 2026

**Critical path**: 13A → 16B-E (polish) → 13C → Beta

**Criteria:**
- [x] Phase 10 complete — 108 base system recipes, self-hosting toolchain
- [x] Phase 11 complete — 88 desktop, networking & AI/ML recipes
- [x] Phase 12 complete — Argonaut init, ark package manager, agnova installer
- [x] Phase 13B complete — GPU drivers, WiFi, Bluetooth, Thunderbolt, printing
- [x] Phase 13D complete — 19+ consumer apps integrated
- [x] Phase 15A partial — Phylax core scanning engine
- [x] AGNOS boots from ISO on bare metal (UEFI) and QEMU
- [ ] **Self-hosting: can rebuild itself from source (13A)** ← PRIMARY BLOCKER
- [x] **Desktop essentials packaged (16A)** — foot, helix, yazi, fuzzel, mako, zathura, imv, mpv, cliphist (9/9 + ark CLI)
- [ ] Third-party security audit complete
- [ ] Community testing program active

### v1.0 Release — Q2 2027

**Criteria:**
- [ ] Phase 13C complete — Documentation, community
- [ ] Phase 16 complete — Full desktop experience
- [ ] All consumer apps published to mela
- [ ] AI-native desktop replacements for Priority 1 items (see `docs/development/applications/roadmap.md`)
- [x] Python runtime management
- [x] Enterprise features: SSO, audit logging, mTLS
- [ ] 6 months of beta testing with no critical bugs
- [ ] Commercial support available

### v2.0 Vision — 2028+

**The Rust Kernel Release.**

- [ ] Phase 20A-C complete — agnostic-kernel boots, runs agents, IPC works
- [ ] Phase 20D-E complete — drivers, Linux compat layer, existing userland runs
- [ ] Phase 20F-G complete — real hardware, self-hosting
- [ ] Dual-kernel support: users choose Linux or agnostic-kernel at install
- [ ] Agent IPC < 100ns (10x faster than Linux)
- [ ] Zero-seccomp sandboxing (capability model replaces syscall filtering)
- [ ] GPU/TPU-aware kernel scheduler (ai-hwaccel in ring 0)

---

## Key Performance Indicators (KPIs)

### Current Status (as of 2026-03-25)

| Metric | Target | Current | Status |
|--------|--------|---------|--------|
| Code Coverage | >80% | ~84.3% | Met |
| Test Pass Rate | 100% | 100% | Met |
| Total Tests | 400+ | 11000+ | Met |
| Agent Spawn Time | <500ms | ~300ms | Met |
| Shell Response Time | <100ms | ~50ms | Met |
| Memory Overhead | <2GB | ~1.2GB | Met |
| Boot Time | <10s | N/A | Pending (Phase 13A) |
| CIS Compliance | >80% | ~85% | Met |
| Stub Implementations | 0 | 0 | Met |
| Compiler Warnings | 0 | 0 | Met |
| Base System Recipes | ~108 | 116 | Complete |
| Desktop Recipes | ~62 | 71 | Complete (lean OS, optional in bazaar) |
| Edge Recipes | ~30 | 31 | Complete |
| Marketplace Recipes | 11 | 59 | Complete (19+ released + shared crate recipes) |
| Bazaar Community | — | 90 | Seed recipes across 8 categories |
| MCP Tools | — | 151 | Complete (14 agnos + 5 aequi + 24 agnostic + 7 delta + 8 photis + 5 edge + 7 shruti + 9 tarang + 8 jalwa + 9 rasa + 7 mneme + 7 irfan + 7 bullshift + 7 yeoman + 5 phylax + others) |
| Consumer Apps | 6 | 19+ | 19+ released (incl. Vidhana v1, Sutra v1, Abacus) |
| Shared Crates | — | 63 library crates | 34 at v1.0+ stable, 29 pre-1.0 |
| Recipe Validation Errors | 0 | 0 | Complete |
| Security Audit Rounds | 15 | 16 | Complete |
| Self-Hosting | Yes | Pending | Phase 13A — THE blocker |

### By Component

| Component | Tests | Notes |
|-----------|-------|-------|
| agnos-common | 307 | Secrets, telemetry, LLM types, manifest, rate limits, audit chain |
| agnos-sys | 750+ | 16 modules: audit, mac, netns, dmverity, luks, ima, tpm, secureboot, certpin, bootloader, journald, udev, fuse, pam, update, llm |
| agent-runtime | 3900+ | Phylax (65), 140 MCP tools, orchestrator (127, GPU-aware scoring), IPC, sandbox, registry, marketplace, federation, migration, scheduler, PQC, safety, finetune, formal_verify, sandbox_v2, rl_optimizer, cloud, collaboration, sigil, aegis, takumi, argonaut, agnova, ark, edge (GPU heartbeat), grpc, service_mesh, oidc, delegation, vector_rest, marketplace_backend, selfhost, webview, python_runtime |
| llm-gateway | 860 | 15 providers, rate limiting, streaming, cert pinning, hardware acceleration, token budgets |
| ai-shell | 1554 | 61+ intents (including 5 phylax, 8 tarang, 8 jalwa), approval workflow, dashboard, aliases |
| desktop-environment | 1692 | Wayland protocol, screen capture, screen recording, plugin host, xwayland, shell integration, theme bridge |

---

## Architecture Decision Records

| # | ADR | Status |
|---|-----|--------|
| 001 | Foundation and Architecture | Accepted |
| 002 | Agent Runtime and Lifecycle | Accepted |
| 003 | Security and Trust | Accepted |
| 004 | Distribution, Build, and Installation | Accepted |
| 005 | Desktop Environment | Accepted |
| 006 | Observability and Operations | Accepted |
| 007 | Scale, Collaboration, and Future | Accepted |

---

## Named Subsystems (21)

| Name | Role | Component |
|------|------|-----------|
| **hoosh** | LLM inference gateway (port 8088, 15 providers) | `llm-gateway/` |
| **daimon** | Agent orchestrator (port 8090, 151 MCP tools) | `agent-runtime/` |
| **agnosys** | Kernel interface | `agnos-sys/` |
| **agnostik** | Shared types library | `agnos-common/` |
| **shakti** | Privilege escalation | `agnos-sudo/` |
| **agnoshi** | AI shell (`agnsh`, 61+ intents) | `ai-shell/` |
| **aethersafha** | Desktop compositor | `desktop-environment/` |
| **mabda** | GPU foundation (Arabic: origin/principle) — device, buffers, compute, textures | `MacCracken/mabda` |
| **ark** | Unified package manager | `ark.rs`, `/v1/ark/*` |
| **nous** | Package resolver daemon | `nous.rs` |
| **takumi** | Package build system | `takumi.rs` |
| **mela** | Agent marketplace | `marketplace/` module |
| **aegis** | System security daemon | `aegis.rs` |
| **sigil** | Trust verification | `sigil.rs` |
| **argonaut** | Init system | `argonaut.rs` |
| **agnova** | OS installer | `agnova.rs` |
| **phylax** | Threat detection engine | `phylax.rs` |
| **bazaar** | Community package repository (Persian: marketplace/gathering) | `recipes/base/bazaar.toml` |
| **sutra** | Infrastructure orchestrator (Sanskrit: thread/rule/formula) | `MacCracken/sutra` |
| **vansh** | Voice AI shell (planned) | TBD |
| **AGNOS** | The OS itself | — |

---

## Phase 17 — Local Inference Optimization (Post-Beta)

**Goal**: Make hoosh + murti competitive with — or better than — PowerInfer-class engines on consumer hardware. AGNOS owns the full stack from kernel to inference; a proprietary inference appliance (Tiiny AI Pocket Lab) shouldn't beat us on hardware we control.

**Key insight from PowerInfer**: LLM neurons follow a power-law activation distribution. ~10% of neurons ("hot") are activated on every input; ~90% ("cold") are input-dependent and rarely needed. Splitting hot→GPU, cold→CPU eliminates most GPU↔CPU data transfer and lets 40B–175B models run on a single consumer GPU.

**Limitation to watch**: PowerInfer only works with ReLU/ReGLU activation functions — not the SwiGLU/GELU used by most frontier models (LLaMA-3, Mistral, Qwen, GPT-4). Their TurboSparse research converts models to high-sparsity ReLU variants for ~$100K. As sparsification matures and more models adopt ReLU variants, the technique becomes broadly applicable.

**Why AGNOS can do better**: A commodity inference appliance runs a generic Linux + llama.cpp fork. AGNOS controls the kernel (scheduler, memory, I/O), the init system (argonaut), the sandbox (agnosys), the GPU allocator (ai-hwaccel), the model runtime (murti), and the inference gateway (hoosh). We can co-design across all layers:

- **Kernel-level VRAM management** — agnosys can pin GPU memory, prevent OOM-killer interference, and provide huge-page-backed model buffers via custom sysctl profiles
- **NUMA-aware neuron placement** — ai-hwaccel already detects topology; murti can place hot neurons on GPU-local NUMA nodes to minimize PCIe latency for cold neuron CPU fallback
- **Zero-copy IPC** — daimon's Unix socket IPC + shared memory regions mean inference results reach agents without serialization overhead
- **Sandboxed inference isolation** — agnosys Landlock + seccomp per-model process means we can run untrusted community models safely, something PowerInfer can't offer
- **Integrated scheduling** — argonaut + scheduler can co-schedule inference with agent workloads, yielding GPU time intelligently rather than fighting for it

### 17A — Activation Sparsity Engine (murti)

| # | Item | Status | Notes |
|---|------|--------|-------|
| 1 | Neuron activation profiler | Not started | Profile models offline to identify hot/cold neuron sets per layer. Output: activation stats TOML alongside GGUF weights. Inspired by PowerInfer's profiling step |
| 2 | Sparse FFN operator (CPU) | Not started | Skip inactive neurons in feed-forward layers on CPU. AVX2/NEON SIMD for sparse matrix ops. Only compute neurons predicted to activate |
| 3 | Sparse FFN operator (GPU) | Not started | CUDA/ROCm kernels that skip cold neurons. Hot neurons preloaded in persistent GPU memory |
| 4 | Adaptive neuron predictor | Not started | Lightweight predictor (bundled in model config) that predicts which neurons activate for a given input. Accuracy target: >95% to avoid quality loss |
| 5 | GPU-CPU hybrid scheduler | Not started | Split FFN layers: hot neurons → GPU, cold neurons → CPU. Dense layers (attention) stay fully on GPU. `--vram-budget` flag for memory cap |
| 6 | PowerInfer GGUF compatibility | Not started | Read PowerInfer-format GGUF files (predictor weights + activation stats embedded). Import path for existing PowerInfer models |

### 17B — Advanced Inference Techniques (murti + hoosh)

| # | Item | Status | Notes |
|---|------|--------|-------|
| 1 | Speculative decoding | Not started | Draft model (small/fast) generates candidates, verify model (large/accurate) accepts/rejects in parallel. 2-3x throughput for autoregressive generation. Already planned in murti Phase 3 |
| 2 | Prefix caching | Not started | Cache KV states for common system prompts across agents. Hoosh routes identical prefixes to cached slots. Massive win for fleet workloads where many agents share prompts |
| 3 | Continuous batching | Not started | Dynamically batch inference requests across agents. Hoosh's rate limiter already knows request timing — extend to batch formation |
| 4 | LoRA adapter hot-swap | Not started | Switch adapters without reloading base model weights. Already planned in murti Phase 3 |
| 5 | Model-aware OOM prevention | Not started | agnosys + murti coordinate: query available VRAM before loading, graceful degradation (quantize down, shed layers to CPU) instead of crash |
| 6 | TurboSparse model conversion | Not started | Watch upstream maturity. When TurboSparse-style SwiGLU→ReLU conversion stabilizes, integrate as `murti quantize --sparsify` command. Unlocks sparsity for LLaMA-3/Mistral/Qwen |

### 17C — Kernel & System Co-optimization

| # | Item | Status | Notes |
|---|------|--------|-------|
| 1 | Huge-page model buffers | Not started | agnosys provides 2MB/1GB huge-page allocation for model weight tensors. Reduces TLB misses during inference |
| 2 | GPU memory pinning | Not started | agnosys prevents hot neuron GPU memory from being reclaimed. Persistent allocation survives model idle periods |
| 3 | NUMA-aware placement | Not started | ai-hwaccel topology detection → murti places CPU-side cold neurons on GPU-local NUMA node. Minimizes PCIe round-trips |
| 4 | Inference-priority scheduling | Not started | argonaut cgroup profiles for inference processes: elevated CPU priority, memory reservation, I/O bandwidth guarantee |
| 5 | Thermal-aware throttling | Not started | ai-hwaccel monitors GPU/CPU thermals. When approaching limits, hoosh shifts load to cloud providers before performance degrades. Smooth handoff, no stutter |
| 6 | Edge inference profiles | Not started | Constrained-device profiles (Raspberry Pi, Pocket Lab-class hardware): aggressive quantization + full CPU sparsity + memory-mapped weights. Daimon edge fleet distributes optimal profile per device class |

### 17D — Pocket AI Appliance Porting (Tiiny AI Pocket Lab)

**Goal**: Acquire a Tiiny AI Pocket Lab (or similar pocket inference appliance), reverse-engineer its boot process, flash AGNOS Edge, and demonstrate that full-stack AGNOS outperforms the stock generic-Linux + PowerInfer setup on identical hardware.

**Why this matters**: If a $300-500 pocket device can run 120B int4 at 20 tok/s on stock firmware, AGNOS should match or beat that — and add agent fleet participation, security, multi-model management, and cloud overflow that the stock OS can't provide. This is the proof point for Phase 17.

#### Reconnaissance (before purchase)

| # | Item | Status | Notes |
|---|------|--------|-------|
| 1 | Confirm SoC identity | Not started | ARM or RISC-V? Custom NPU? Check FCC filings, CES teardowns, Tiiny AI developer docs. Determines kernel config + ai-hwaccel backend needed |
| 2 | Identify boot chain | Not started | U-Boot? Custom bootloader? Locked/signed? Determines flash strategy (dd, fastboot, JTAG, UART) |
| 3 | Check for developer/root access | Not started | SSH? Serial console? Does the stock OS expose a shell? Some appliances ship with adb or UART pads |
| 4 | NPU driver availability | Not started | Open-source drivers? Vendor SDK? Binary blobs only? This is the biggest risk — if the NPU needs proprietary firmware with no docs, we're limited to CPU inference |
| 5 | RAM/storage confirmation | Not started | 16GB or 32GB LPDDR5X? eMMC or UFS? Determines which models fit and whether we need swap/zram |

#### Bring-up (after hardware in hand)

| # | Item | Status | Notes |
|---|------|--------|-------|
| 1 | Serial console access | Not started | Find UART pads, attach serial adapter, capture boot log. Identify kernel version, rootfs layout, partition table |
| 2 | Dump stock firmware | Not started | Full backup before flashing anything. dd the eMMC/UFS. Preserve PowerInfer binaries for benchmarking |
| 3 | Stock baseline benchmarks | Not started | Run their inference stack: measure tok/s, latency (p50/p99), power draw, thermal throttle point. Multiple models (7B, 13B, 70B, 120B). This is the number to beat |
| 4 | Cross-compile AGNOS Edge | Not started | Adapt `build-edge.sh` for the device's SoC. Kernel config for the specific ARM/RISC-V chip. dm-verity rootfs |
| 5 | First boot AGNOS | Not started | Flash to eMMC/SD, boot to argonaut, verify daimon + hoosh start, network connectivity works |
| 6 | ai-hwaccel NPU backend | Not started | If custom NPU: implement `AcceleratorType::CustomNpu` in ai-hwaccel. VRAM/memory queries, layer offload. Feature-gated behind `pocket-lab` |
| 7 | murti on-device inference | Not started | Load model via murti, run inference through hoosh API. Start with CPU-only, then enable NPU if driver available |

#### Benchmarks (AGNOS vs stock)

| # | Metric | Stock Baseline | AGNOS Target | Notes |
|---|--------|---------------|--------------|-------|
| 1 | Boot to inference-ready | TBD | < 5s | argonaut minimal boot vs their init system |
| 2 | tok/s (120B int4) | ~20 tok/s (claimed) | ≥ 20 tok/s | Match with murti sparsity. Beat with system co-optimization (17C) |
| 3 | tok/s (7B int4) | TBD | Target: 100+ tok/s | Small model should fly on this hardware |
| 4 | Memory overhead (idle) | TBD | < 200MB | argonaut + daimon + hoosh. Stock probably runs systemd + bloat |
| 5 | Multi-model switching | Not possible (stock) | < 2s | murti ModelPool LRU — load second model without killing first |
| 6 | Fleet join time | N/A (stock has no fleet) | < 1s | daimon edge node registration + heartbeat |
| 7 | Power draw (inference) | TBD | ≤ stock | Same workload, equal or less power. Sparsity skipping = less compute = less watts |
| 8 | Thermal throttle headroom | TBD | > stock | Thermal-aware throttling (17C-5) should keep temps lower by proactively shedding to cloud |

#### AGNOS Advantages on This Hardware

Things the stock firmware **cannot do** that AGNOS provides out of the box:

| Capability | Stock | AGNOS Edge |
|-----------|-------|------------|
| Run untrusted community models safely | No sandbox | aegis + agnosys Landlock/seccomp per-model |
| Cloud overflow when local saturates | No | hoosh routes to 15 cloud providers |
| Participate in desktop inference fleet | No | daimon edge fleet node, swarm scheduling |
| Remote model deployment | Manual | sutra playbook: `sutra apply deploy-model.yaml --target pocket-lab` |
| Multi-model serving | One model at a time | murti ModelPool with LRU eviction by RAM budget |
| Secure API access | Open | nein firewall + Bearer auth + mTLS |
| OTA updates | Unknown | ark + daimon system_update module |
| Monitoring | None | nazar agent, `/v1/metrics/prometheus` |
| Natural language control | None | agnoshi: "switch to codellama on the pocket lab" |

### Maturity Watch List

Track these upstream projects — adopt techniques as they stabilize:

| Project | What to Watch | When to Act |
|---------|--------------|-------------|
| [PowerInfer](https://github.com/Tiiny-AI/PowerInfer) | ReLU-only limitation; TurboSparse SwiGLU→ReLU conversion; SmallThinker models | When TurboSparse covers top-5 open models (LLaMA-3, Mistral, Qwen, Gemma, Phi) |
| [TurboSparse](https://arxiv.org/abs/2406.05955) | Sparsification quality vs original model; cost reduction below $100K | When conversion is automated and quality gap < 2% on MMLU/HumanEval |
| [vLLM](https://github.com/vllm-project/vllm) | Continuous batching, PagedAttention, prefix caching | Already mature — integrate via murti vLLM backend |
| [llama.cpp](https://github.com/ggerganov/llama.cpp) | Speculative decoding, flash attention, CUDA graph | Track as murti's default backend; upstream improvements land for free |
| [Candle](https://github.com/huggingface/candle) | Pure Rust GGUF runtime maturity | When it matches llama.cpp throughput within 20% — murti Candle backend |
| [MLX](https://github.com/ml-explore/mlx) | Apple Silicon optimization | Relevant for macOS AGNOS builds; murti Metal backend |
| [Tiiny AI Pocket Lab](https://github.com/Tiiny-AI) | SDK/developer docs, FCC teardowns, NPU driver availability, hacking community | When dev access confirmed — triggers 17D bring-up |

---

## Phase 18 — Immersive Communication (Post-Beta)

**Goal**: Video conferencing that transcends flat screens — connect to virtual spaces, spatial audio, avatar presence. Not a Zoom clone; a portal into shared environments where AGNOS agents and humans coexist.

**Design philosophy**: The LLM thinks, the crates do everything else. Hoosh decides what to say. Dhvani speaks it. Goonj makes the room sound right. Soorat renders the space. Bhava drives the avatar's expression. The LLM never touches audio encoding, 3D rendering, or spatial math — it just *decides*.

### 18A — Core Video Conferencing

| # | Item | Status | Notes |
|---|------|--------|-------|
| 1 | Peer-to-peer encrypted A/V | Not started | nein (networking) + pqc (post-quantum encryption) + tarang (encode/decode). WebRTC-compatible signaling, SRT/QUIC transport |
| 2 | Spatial audio mixing | Not started | dhvani (audio engine) + goonj (room acoustics). Each participant has a position; audio is spatialized based on virtual seating |
| 3 | Screen sharing as texture | Not started | aethersafta captures screen → tarang encodes → transmitted as video stream → rendered as floating panel or wall texture in virtual space |
| 4 | Camera feed compositing | Not started | aethersafta (V4L2 camera capture) → tarang (encode) → soorat (renders as billboard or avatar face texture) |
| 5 | Voice activity detection | Not started | dhvani analysis (onset detection, energy threshold) → UI highlights active speaker |
| 6 | Meeting recording | Not started | dhvani (audio) + tarang (mux to MP4/MKV) + aethersafta (composited scene recording) |

### 18B — Virtual World Integration

| # | Item | Status | Notes |
|---|------|--------|-------|
| 1 | Virtual meeting rooms | Not started | soorat renders 3D environment. Preset rooms: conference, amphitheater, cafe, outdoor. Custom rooms from mesh import |
| 2 | Avatar system | Not started | Minimal avatar (head + hands). Driven by camera pose estimation (future) or manual controls. Bhava mood → facial expression |
| 3 | Room acoustics from geometry | Not started | goonj computes impulse response from virtual room mesh → dhvani applies convolution reverb to all voice streams. Cathedral sounds like a cathedral |
| 4 | Avatar navigation | Not started | raasta pathfinding in virtual space. Walk to whiteboard, sit at table, stand at podium |
| 5 | Physics interaction | Not started | impetus for avatar collision, object manipulation (pick up virtual pen, draw on whiteboard) |
| 6 | Agent participants | Not started | Daimon agents join as participants. Speak through dhvani voice synth (personality-shaped by bhava). Render as avatars in-world. Human and AI in the same virtual room |

### 18C — AI Meeting Intelligence

| # | Item | Status | Notes |
|---|------|--------|-------|
| 1 | Real-time transcription | Not started | dhvani audio capture → hoosh (Whisper STT) → live captions in virtual space |
| 2 | Meeting summarization | Not started | hoosh processes transcript → action items, decisions, key points |
| 3 | Translation | Not started | hoosh translates → dhvani voice synth speaks translated audio with original speaker's prosody (bhava preserves emotional tone) |
| 4 | Smart muting | Not started | dhvani analysis detects typing, coughing, background noise → auto-mute with visual indicator |

**Consumers**: All AGNOS users. Every consumer project can embed virtual meetings. SY agents participate as first-class meeting attendees.

---

## Phase 19 — Computational Architecture Optimization (Post-Beta)

**Design philosophy**: Remove all quantitative work from the LLM. The superbrain doesn't calculate — it *decides*. Every deterministic operation (math, physics, crypto, audio, rendering, memory recall) runs in specialized crates at nanosecond speed. The LLM handles only reasoning, intent, and judgment.

### 19A — Core-Affinity Neural Network Scheduling

**Insight**: Instead of fleet/multi-VM distribution for neural network inference, bifurcate CPU cores so each core or core pair handles specific network nodes. Weights stay hot in L1/L2 cache. No cross-socket NUMA penalties. No network hops.

| # | Item | Status | Notes |
|---|------|--------|-------|
| 1 | Core topology mapping | Not started | agnosys + ai-hwaccel: enumerate cores, cache sizes (L1/L2/L3), NUMA nodes, P-core vs E-core. Build topology graph |
| 2 | Layer-to-core assignment | Not started | murti: given model architecture, assign layers/heads to cores based on cache size and data locality. Attention heads on P-cores (compute-heavy), FFN cold neurons on E-cores (memory-heavy) |
| 3 | Core pinning API | Not started | agnosys: `pin_thread_to_core(thread, core_id)`. Argonaut cgroup integration for inference process core isolation |
| 4 | Cache-aware weight placement | Not started | murti: ensure layer weights fit in assigned core's L2. If weights exceed L2, split across adjacent cores sharing L3. Never cross NUMA boundary |
| 5 | CoreAffinityPlan | Not started | murti type: `{ layer_assignments: Vec<(LayerId, CoreSet)>, hot_neuron_cores: CoreSet, cold_neuron_cores: CoreSet, attention_cores: CoreSet }`. Computed at model load time, static during inference |
| 6 | Benchmark: pinned vs unpinned | Not started | Prove cache hit rate improvement and tok/s gain from core affinity. Expect 10-30% improvement on CPU-bound inference |

### 19B — ASIC / Hardware Cryptographic Acceleration

**Insight**: At fleet scale (10K+ nodes, 1M+ agents), cryptographic operations (SHA-256 hash chains, signature verification, PQC lattice ops) become the bottleneck. SHA-256 ASICs (repurposed Bitcoin mining hardware) and AES-NI instructions can offload this at wire speed.

| # | Item | Status | Notes |
|---|------|--------|-------|
| 1 | CryptoAsic accelerator type | Not started | ai-hwaccel: `AcceleratorType::CryptoAsic { hash_rate: u64 }`. Detect USB ASIC miners, FPGA cards, AES-NI CPU instructions |
| 2 | libro ASIC offload | Not started | libro: route hash chain operations to detected ASIC when available. Fallback to CPU SHA-256. Transparent to callers |
| 3 | sigil ASIC offload | Not started | sigil: route signature verification to hardware. Ed25519 on CPU, SHA-256 chain verification on ASIC |
| 4 | pqc hardware acceleration | Not started | pqc: lattice-based operations (Kyber, Dilithium) are matrix-heavy. Route to GPU compute or FPGA when available |
| 5 | Audit chain at scale benchmark | Not started | Target: 1 billion hashes/sec on commodity ASIC vs ~50M/sec on CPU. 20x throughput for fleet audit chains |
| 6 | USB ASIC integration | Not started | agnosys udev rules for USB ASIC miner detection. Auto-register as crypto accelerator. argonaut service for ASIC management |

### 19C — LLM Cognitive Offloading Architecture

**Principle**: The LLM is the reasoning engine. Everything else is offloaded to deterministic, auditable, benchmarked crates. The LLM never computes what a crate can compute faster and more reliably.

| Operation | Before (LLM does it) | After (Crate does it) | Speedup |
|-----------|----------------------|----------------------|---------|
| Math/physics | LLM approximates | hisab/bijli/pravash/ushma exact | ~∞ (deterministic vs probabilistic) |
| Audio synthesis | Neural TTS (ElevenLabs, OpenAI) | dhvani formant synthesis | No network latency, no vendor, sub-ms |
| Voice personality | Prompt engineering for "speak softly" | bhava mood → dhvani prosody params | Deterministic, real-time modulation |
| Room acoustics | "Add reverb" (guess) | goonj computes from geometry | Physically accurate, not approximated |
| Navigation | LLM describes path | raasta A*/HPA*/navmesh | Optimal, benchmarked, deterministic |
| Cryptography | N/A (LLM can't) | libro/sigil/pqc + ASIC | Hardware-accelerated, auditable |
| Memory recall | Re-read context window | Audit chain + vector store + actr | Persistent, searchable, no hallucination |
| Scheduling | LLM suggests times | argonaut + scheduler + circadian | System-aware, resource-aware |
| Weather effects | LLM describes weather | badal computes from atmospheric model | Physically simulated, feeds bhava |
| Material properties | LLM guesses Young's modulus | dravya lookup with real data | Exact, engineering-grade |

**The result**: The LLM's context window is freed from computational tasks. It spends tokens on *thinking* — reasoning about what to do, understanding user intent, making creative decisions. Everything else flows through the crate stack at speeds the LLM can never match.

This is why AGNOS builds every crate. Each one removes a quantitative burden from the superbrain, leaving it to do what only it can do: *understand and decide*.

---

## AGNOS Foundation — Non-Profit Organization

**Goal**: Establish a non-profit organization (NPO) to steward AGNOS, its science crate ecosystem, and ongoing research. Not a commercial venture — a research foundation funded by donations, grants, and community support.

**Why NPO, not commercial**:
- AGNOS is GPL-3.0. The code belongs to the community.
- The science crates (hisab, prakash, bijli, pravash, ushma, kimiya, goonj, pavan, dravya, badal, bhava, raasta, impetus) are computational infrastructure that benefits everyone — researchers, educators, game developers, engineers.
- Consumer projects (SY, Agnostic) have their own commercial paths (AGPL + commercial dual-license). The OS and science stack stay open.
- Donations align incentives: the community funds what the community uses. No venture capital, no exit pressure, no enshittification.

### Structure

| Element | Details |
|---------|---------|
| **Legal entity** | 501(c)(3) non-profit (US) or equivalent |
| **Name** | AGNOS Foundation (or "Agnostikos Foundation") |
| **Mission** | Advance open-source AI-native operating systems and computational science libraries |
| **Scope** | AGNOS OS, all shared crates, science stack, community infrastructure (bazaar), documentation, education |
| **Funding** | Donations (GitHub Sponsors, Open Collective, direct), grants (NSF, DARPA, private research foundations), corporate sponsorships |
| **Governance** | Small board (founder + 2-4 community members). Technical decisions by maintainers. Financial transparency (public reports) |

### Revenue Streams (all non-commercial)

| Source | Description |
|--------|-------------|
| **GitHub Sponsors** | Individual and corporate monthly sponsorships |
| **Open Collective** | Transparent donation platform with expense tracking |
| **Research grants** | NSF, DARPA, EU Horizon — the science crates qualify as computational research infrastructure |
| **Academic partnerships** | Universities using AGNOS crates in coursework/research → institutional support |
| **Conference talks** | Speaking fees donated back to foundation |
| **Bounties** | Community-funded bounties for specific features/crates |

### What the Foundation Funds

| Area | Examples |
|------|---------|
| **Infrastructure** | CI/CD runners, crates.io publishing, documentation hosting |
| **Research** | External research step (P(-1) step 5) — fund domain experts to review science crate accuracy |
| **Hardware** | Test hardware (Pocket Lab, Raspberry Pi fleet, GPU test rigs) for Phase 13F/17D |
| **Community** | Documentation, video tutorials, conference attendance, beta testing programs |
| **Maintainer support** | Stipends for active maintainers (optional — founder not taking any) |

### What Stays Commercial (separate from foundation)

| Project | Model |
|---------|-------|
| **SecureYeoman** | AGPL-3.0 + commercial license (existing) |
| **Agnostic** | AGPL-3.0 + commercial license |
| **Consumer apps** (BullShift, Delta, Aequi, etc.) | Individual project licensing |

The foundation owns the commons. Commercial projects build on top. Clean separation.

### Timeline

| # | Item | Status |
|---|------|--------|
| 1 | Choose legal structure (501c3 vs fiscal sponsor) | Not started |
| 2 | File incorporation papers | Not started |
| 3 | Set up GitHub Sponsors + Open Collective | Not started |
| 4 | Write mission statement and bylaws | Not started |
| 5 | Recruit initial board members (2-4 community members) | Not started |
| 6 | Apply for research grants (NSF, private foundations) | Not started |
| 7 | Public announcement with donation page | Not started |

**Priority**: After beta. The code speaks first. The organization formalizes what the code already proved.

---

## Phase 20 — AGNOS Kernel (Post-v1.0, Exploratory)

**Codename**: agnostic-kernel — a Rust-native microkernel purpose-built for AI agent workloads.

### Motivation

AGNOS currently runs on Linux 6.6 LTS. Linux is proven, stable, and battle-tested — and it's the right choice through v1.0. But the AGNOS userland has demonstrated what happens when you own every layer in Rust:

- **AgnosAI**: 227,000x faster fleet messaging than Python/CrewAI
- **tarang**: 18-33x faster media operations than GStreamer
- **aethersafta**: 10x compositor speedup from SIMD, 30fps 1080p software-only pipeline
- **daimon**: nanosecond-scale agent orchestration, sub-microsecond IPC
- **ai-hwaccel**: 14µs full hardware detection, 44ns placement decisions

Linux's process model, syscall interface, and scheduler were designed for general-purpose computing. Agents are modelled as processes. Sandboxing is bolted on (Landlock, seccomp, namespaces). IPC goes through the kernel even when both endpoints are AGNOS agents. The abstraction mismatch costs performance and complexity.

A Rust kernel could make agents a **first-class kernel primitive** — not processes pretending to be agents.

### Architecture Vision

```
┌──────────────────────────────────────────────────────────────┐
│  agnostic-kernel (Rust microkernel)                           │
├──────────────────────────────────────────────────────────────┤
│  Agent Scheduler        │  Agent objects as kernel primitives │
│  ├─ Priority + DAG      │  ├─ Built-in sandbox (no seccomp)  │
│  ├─ GPU/TPU-aware       │  ├─ Native IPC (zero-copy, typed)  │
│  └─ Preemption          │  ├─ Resource quotas (CPU/mem/GPU)  │
│                         │  └─ Cryptographic audit at sched    │
├─────────────────────────┼─────────────────────────────────────┤
│  Memory                 │  Hardware Abstraction               │
│  ├─ Per-agent heaps     │  ├─ ai-hwaccel in-kernel            │
│  ├─ Zero-copy IPC       │  ├─ GPU/TPU dispatch from sched     │
│  └─ Capability-based    │  └─ IOMMU agent isolation           │
├─────────────────────────┴─────────────────────────────────────┤
│  Driver model: Rust async drivers in userspace (like Fuchsia) │
│  Linux compat: personality layer for existing apps            │
└──────────────────────────────────────────────────────────────┘
```

### Phased Approach

| Phase | Milestone | Scope |
|-------|-----------|-------|
| **20A** | Research & proof-of-concept | Minimal Rust kernel that boots on QEMU, prints to serial, runs one agent. Study Redox, Theseus, Tock, Fuchsia |
| **20B** | Agent primitives | Agent as kernel object (create, destroy, suspend, resume). Per-agent memory regions. Capability-based security model |
| **20C** | IPC & scheduling | Zero-copy typed IPC between agents. Priority scheduler with DAG awareness. GPU/TPU resource integration via ai-hwaccel |
| **20D** | Driver framework | Async Rust drivers in userspace. VIRTIO for QEMU. Basic NVMe, NIC, GPU passthrough |
| **20E** | Userland compatibility | Run existing AGNOS userland (daimon, hoosh, agnoshi) on the new kernel. Linux syscall compatibility layer for third-party apps |
| **20F** | Hardware bring-up | Boot on real x86_64 + aarch64 hardware. UEFI, ACPI, interrupt routing, multi-core |
| **20G** | Self-hosting | agnostic-kernel builds agnostic-kernel. Full dogfooding |

### Prior Art

| Project | Language | Key insight for AGNOS |
|---------|----------|----------------------|
| **Redox OS** | Rust | Microkernel in Rust is viable. Scheme-based URLs for IPC. 10+ years of development |
| **Theseus** | Rust | Live kernel evolution — swap components without reboot. Cell-based isolation |
| **Tock** | Rust | Embedded Rust kernel. Capability-based, grant regions for untrusted apps |
| **Fuchsia** | C++/Rust | Zircon microkernel. Capability objects. Userspace drivers. Component model |
| **seL4** | C (verified) | Formally verified microkernel. Capability-based security proof |

### Branch Strategy

All kernel work lives on a dedicated branch — never touches `main`:

```
main              → Linux 6.6 LTS (beta → v1.0 → v1.x production)
agnostic-kernel   → Phase 20 R&D (parallel track, no merge until 20E)
```

Merge criteria: Phase 20E passes — existing AGNOS userland (daimon, hoosh, agnoshi, aethersafha) runs on agnostic-kernel with equivalent or better performance. Until then, two worlds, one repo, zero risk to shipping.

### Non-Blockers

This does NOT block any AGNOS release:
- **Beta (Q4 2026)**: Linux 6.6 LTS
- **v1.0 (Q2 2027)**: Linux 6.6 LTS
- **v1.x**: Linux kernel, production-hardened
- **v2.0+**: agnostic-kernel option alongside Linux

The kernel is a parallel research track. AGNOS ships on Linux until the Rust kernel is proven on real hardware with real workloads.

### Success Criteria (Phase 20A exit gate)

- [ ] Boots on QEMU x86_64 to a Rust `main()`
- [ ] Creates and destroys an "agent" kernel object
- [ ] Two agents communicate via zero-copy IPC
- [ ] Measured IPC latency < 100ns (vs Linux ~1µs for pipe/socket)
- [ ] Agent isolation: one agent crash doesn't take down the kernel
- [ ] The proof-of-concept is < 10,000 lines of Rust

---

## Version Sweep — Remaining Items (2026-03-29)

Completed sweep updated ~106 recipes + 13 Rust workspace crates. The following items
need manual attention before the next build:

### Browser sha256 Verification
- [ ] **Firefox ESR 140.9.0** — sha256 set to `"VERIFY"` (tarball too large for automated download)
- [ ] **Chromium 146.0.7680.169** — sha256 set to `"VERIFY"` (tarball too large for automated download)

### Skipped — Version Not Found Upstream
These versions were reported by web search but returned 404 when fetched.
Verify correct latest version manually:
- [ ] **pango** — reported 1.57.2, current stays 1.56.1
- [ ] **libxkbcommon** — reported 1.13.1, current stays 1.11.0
- [ ] **gtk3** — reported 3.24.52, current stays 3.24.43
- [ ] **fontconfig** — reported 2.17.1, current stays 2.16.0
- [ ] **NetworkManager** — reported 1.56.0, current stays 1.51.4
- [ ] **binutils** — reported 2.46, current stays 2.45
- [ ] **gettext** — reported 1.0, current stays 0.26
- [ ] **grub** — reported 2.14, current stays 2.12

### Major Jumps Deferred — Need Compatibility Evaluation
- [ ] **nvidia-cuda-toolkit** 12.8.1 → 13.2.0 (major version)
- [ ] **rocm** 6.4.0 → 7.2.1 (major version)
- [ ] **nvidia-driver** 570.133.07 → 595.58.03 (new driver branch)
- [ ] **ffmpeg** 7.1.1 → 8.1 (major version)

### Edge Recipes — Behind Base
Some edge recipes pin older versions than base for stability. Evaluate whether
to sync or keep intentionally conservative:
- [ ] **edge/openssl** 3.4.1 — base now 3.5.5
- [ ] **edge/glibc** 2.40 — base now 2.42
- [ ] **edge/bash** 5.2.37 — base now 5.3
- [ ] **edge/iproute2** 6.12.0 — base now 6.19.0

---

## Contributing

### Priority Contribution Areas

1. **Self-hosting on-target (Phase 13A)** — Build AGNOS on AGNOS — THE beta blocker
2. **Desktop polish (Phase 16B-E)** — Touch input, HiDPI, compositor config, themes/icons
3. **Documentation (Phase 13C)** — Video tutorials, support portal
4. **Community testing** — Beta tester enrollment + bug tracker setup
5. **Hardware testing (Phase 13F)** — RPi4, Intel NUC, older hardware validation

### Getting Started

See [CONTRIBUTING.md](/CONTRIBUTING.md) for:
- Development environment setup
- Code style and testing requirements
- Git workflow and commit conventions
- Pull request process

---

## Resources

- **Repository**: https://github.com/MacCracken/agnosticos
- **Documentation**: https://docs.agnos.org (planned)
- **Changelog**: [CHANGELOG.md](/CHANGELOG.md)
- **Sprint history**: [docs/development/sprint-history.md](/docs/development/sprint-history.md)
- **Long-term app roadmap**: [docs/development/applications/roadmap.md](/docs/development/applications/roadmap.md)
- **LFS Reference**: https://www.linuxfromscratch.org/lfs/view/stable/
- **BLFS Reference**: https://www.linuxfromscratch.org/blfs/view/stable/

---

*Last Updated: 2026-03-29 | Next Review: 2026-04-05*
