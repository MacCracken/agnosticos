# AGNOS Development Roadmap

> **Status**: Pre-Beta | **Last Updated**: 2026-04-03
> **Monolith fully dismantled** — all subsystems extracted to standalone repos. Workspace: examples only. agnostik (0.90.0), agnosys (0.51.0), shakti (0.1.0) all standalone.
> **Recipes**: 116 base + 71 desktop + 25 AI + 9 network + 8 browser + 109 marketplace + 4 python + 3 database + 31 edge = 376 OS (+ 90 bazaar community)
> **Build order**: 178 packages in `recipes/build-order.txt` (base + desktop, dependency-ordered)
> **Phases 10–14 complete** | **Phase 15A**: Core scanning done (phylax) | **Phase 16A**: Desktop essentials done | **Phase 17**: Local inference optimization (planned) | **Audit**: 16 rounds
> **Shared Crates**: 77 library crates — 56 at v1.0+ stable, 20 pre-1.0. Key milestones: sigil 1.0.0, kavach 2.0.0, bote 0.92.0, t-ron 0.90.0, agnostik 0.90.0, agnosys 0.51.0
> **Consumer Projects**: 19+ released (including Vidhana v1, Sutra v1, Abacus)

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

## Monolith Extraction — Migration Status

The original monolith (`userland/`) contained agent-runtime, ai-shell, llm-gateway, desktop-environment, agnos-common, and agnos-sys. All have been extracted.

### Completed Extractions

| Original | Extracted To | Version | Method | Date |
|----------|-------------|---------|--------|------|
| `agent-runtime/` | 12 standalone repos (see below) | various | Code moved to new repos | 2026-04-01 |
| `ai-shell/` | **agnoshi** (`MacCracken/agnoshi`) | 0.1.0 | Code moved | 2026-04-01 |
| `llm-gateway/` | **hoosh** (`MacCracken/hoosh`) | 1.2.0 | Code moved | 2026-04-01 |
| `desktop-environment/` | **aethersafha** (`MacCracken/aethersafha`) | 0.1.0 | Code moved | 2026-04-01 |
| `agnos-common/` | **agnostik** (`MacCracken/agnostik`) | 0.90.0 | Git dep, tag `0.90.0` | 2026-04-02 |
| `agnos-sys/` | **agnosys** (`MacCracken/agnosys`) | 0.51.0 | Git dep, tag `0.51.0` | 2026-04-02 |
| `agnos-sudo/` | **shakti** (`MacCracken/shakti`) | 0.1.0 | Standalone repo | 2026-04-03 |

### Crate Absorptions (code merged into existing repos)

| Source Module | Absorbed Into | New Version |
|--------------|---------------|-------------|
| `agent-runtime/mcp_server/` | **bote** | 0.92.0 |
| `agent-runtime/sandbox_mod/` | **kavach** | 2.0.0 |
| `agent-runtime/safety/` | **t-ron** | 0.90.0 |

### Remaining in Workspace

| Crate | Status | Notes |
|-------|--------|-------|
| `examples/` | Agent SDK examples | Depends on agnostik + agnosys via git deps. Consider moving to agnostik repo. |

### Extraction Complete

All userland code has been extracted. The workspace contains only examples.

### Post-Extraction Cleanup

- [ ] Evaluate whether `examples/` should move to agnostik repo or stay here
- [x] Clean up workspace `Cargo.toml` — removed 14 unused deps (tonic, prost, axum, tower, reqwest, sha2, etc.), fixed agnosys tag 0.50.0 → 0.51.0
- [x] Repo identity: meta-repo (docs, scripts, kernel configs, CI/CD). CLAUDE.md updated to reflect this.
- [ ] **Extract `recipes/` to zugot** (`MacCracken/zugot`) — standalone recipe repo. ark consumes zugot as its package database. Name: Hebrew זוּגוֹת (pairs that go into the ark).

---

## P0 — Active Blockers

### Recipe Audit (P0)
- [x] License audit — all 109 marketplace recipes set to `GPL-3.0-only` (stiva: `GPL-3.0-or-later`). Duplicate `irfan.toml` deleted.
- [x] Version sync — 5 recipe versions corrected against actual repos (agnosys 0.51.0, daimon 0.6.0, hoosh 1.2.0, kybernet 0.51.0, bote 0.92.0)
- [x] Header comments — 29 stale scaffolding-era comments updated to match current versions
- [x] Structural fixes — 3 misplaced install blocks, tazama stale gstreamer deps removed
- [ ] SHA256 verification — placeholder fields added to all 109 recipes, need actual hashes from release tarballs

### Recipe Version Bumps (deferred — evaluate compatibility)
- [ ] **nvidia-cuda-toolkit** 12.8.1 → 13.2.0
- [ ] **rocm** 6.4.0 → 7.2.1
- [ ] **nvidia-driver** 570.133.07 → 595.58.03
- [ ] **ffmpeg** 7.1.1 → 8.1

### Edge Recipes Sync
- [x] **edge/openssl** 3.4.1 → 3.5.5 (synced with base, SHA updated)
- [x] **edge/glibc** 2.40 → 2.42 (synced with base, SHA updated)
- [x] **edge/bash** 5.2.37 → 5.3 (synced with base, SHA updated)
- [x] **edge/iproute2** 6.12.0 → 6.19.0 (synced with base, SHA updated)

---

## Phase 13A — OS Independence Validation (BETA BLOCKER)

**This is the single most important remaining work.** Without it, AGNOS is a Debian overlay.

Infrastructure complete. Validation remaining — requires real hardware/QEMU execution.

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

### 16B — Input & Hardware Detection

| # | Need | Approach | Status | Notes |
|---|------|----------|--------|-------|
| 1 | Touchscreen detection | libinput + udev rules | Not started | Auto-detect touch devices, enable tap-to-click, gesture support in aethersafha |
| 2 | Touch gestures | libinput-gestures or custom | Not started | Pinch-zoom, swipe between workspaces, three-finger drag |
| 3 | On-screen keyboard | squeekboard or custom | Not started | Required for tablet/all-in-one without physical keyboard |
| 4 | HiDPI / scaling | Wayland fractional scaling | Not started | Auto-detect display DPI, set appropriate scale factor |
| 5 | Stylus / pen input | libinput tablet support | Not started | Pressure sensitivity, palm rejection |

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
| Media | 5 | ffmpeg, yt-dlp, obs-studio, audacity, jellyfin |
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

**Subsystem**: **phylax** (Greek: guardian/watchman) — standalone crate (`MacCracken/phylax`)

### 15A — Core Scanning Engine (remaining)

| # | Item | Status | Notes |
|---|------|--------|-------|
| 3 | Signature database (`.phylax-db`) | Not started | Signed, versioned threat definitions distributed via ark |
| 4 | On-access scanning (fanotify) | Not started | Real-time filesystem monitoring via agnosys fanotify bindings |

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

| # | Target | Arch | Profile | Status |
|---|--------|------|---------|--------|
| 2 | Raspberry Pi 4 | aarch64 | Full | Ready — needs physical validation |
| 3 | Intel NUC (bare metal) | x86_64 | Desktop | Not started |
| 4 | Older x86_64 (~2014 era) | x86_64 | CLI | Not started |
| 5 | Touchscreen desktop | x86_64 | Desktop | Not started |
| 6 | AWS DeepLens | x86_64 | Edge | Ready |
| 7 | ARM64 SBC (QEMU) | aarch64 | Edge | Not started |
| 8 | ESP32-S3 | xtensa | Edge/IoT | Recipe done, needs source repo + flash test |
| 9 | ESP32-C3 | riscv32 | Edge/IoT | Recipe done, secondary target |
| 10 | Tiiny AI Pocket Lab | TBD | Edge+AI | Not started — see Phase 17D |

---

## Phase 13G — Consumer App Bundle Tests

All 19 apps released. Bundle tests (`ark-bundle.sh`) not yet run.

| App | Bundle Test |
|-----|-------------|
| SecureYeoman, Photis Nadi, BullShift, Agnostic, Delta, Aequi, Irfan, Shruti, Tazama, Rasa, Mneme, Nazar, Selah, Abaco, Rahd, Tarang, Jalwa, Vidhana, Sutra | Not started |

---

## SecureYeoman & Agnostic Integration

*Cross-project integration items for the AGNOS ecosystem.*

### SecureYeoman Shared Crate Adoption

| # | Item | SY replaces | With crate | Status |
|---|------|-------------|------------|--------|
| SY5 | Image processing | Internal sharp/jimp | `ranga` (WASM/FFI bridge) | Planned |
| SY6 | Audio in agent workflows | None | `dhvani` | Planned |
| SY7 | Agent-to-agent protocol | Custom A2A | `sluice` (future) | Planned |

### SecureYeoman → Ecosystem Handoff

Patterns to extract into shared crates:

| Pattern | Target crate |
|---------|-------------|
| A2A authenticated handshake | sluice |
| A2A tool delegation | sluice |
| A2A event streaming | sluice |

---

## Engineering Backlog

*Completed items archived in [sprint-history.md](sprint-history.md).*

### Active

| # | Priority | Item | Notes |
|---|----------|------|-------|
| B1 | High | Self-hosted CI runners on AGNOS | Replace Arch (x86_64) and Ubuntu (aarch64) runner OS with AGNOS itself — AGNOS builds AGNOS |
| B2 | High | RPi4 hardware boot test | Firmware blobs added, needs physical validation |
| E1 | Medium | ESP32 agent source repo | Recipe done, MQTT bridge done. Pending: source repo + firmware |
| S2 | Medium | SGX/SEV hardware validation | kavach backends implemented, need hardware |
| R1 | P0 | Full recipe audit (95+ recipes) | SHA verification, version sync, field audit — license/version/structure done, SHA placeholders added |
| V1 | Medium | **mudra** — token/value primitives | Sanskrit: coin/seal/token. Asset identity, ownership, divisibility. Crate #78 |
| V2 | Medium | **vinimaya** — transaction layer | Sanskrit: exchange/barter. Atomic transfers, escrow, settlement. Depends on mudra, libro, sigil. Crate #79 |
| T1 | Medium | **taal** — music theory | Sanskrit: rhythmic cycle. Scales, intervals, chords, time signatures, key signatures, progressions, counterpoint. Crate #80 |
| N1 | Medium | **natya** — theater/drama/narrative | Sanskrit: drama (from Natya Shastra). Narrative structure, character archetypes, dramatic arcs, rasa theory, comedy/tragedy, dialogue, timing. Crate #81 |
| K1 | Medium | **kshetra** — temporal geography | Sanskrit: field/domain (Bhagavad Gita: dharma-kshetra). Spatiotemporal database — (lat, lon, time) → state. Geology, climate, vegetation, settlement, political layers. Crate #82 |
| L1 | Low | **stiva** license review | GPL-3.0-or-later needs review — repair when next touched |

### Blocked — AgnosAI Integration

Blocked on AgnosAI v1 release + Agnostic integration testing.

| # | Priority | Item |
|---|----------|------|
| A1 | High | AgnosAI marketplace recipe |
| A2 | High | AgnosAI MCP tools in daimon |
| A3 | High | AgnosAI agnoshi intents |
| A4 | Medium | Agnostic native binary migration (Python→Rust) |
| A5 | Medium | AgnosAI ↔ hoosh integration |
| A6 | Low | AgnosAI fleet ↔ daimon edge |

---

---

## Release Roadmap

### Beta Release — Q4 2026

**Critical path**: 13A → 16B-E (polish) → 13C → Beta

- [ ] **OS Independence: AGNOS rebuilds itself from source without host distro (13A)** ← PRIMARY BLOCKER
- [ ] Third-party security audit complete
- [ ] Community testing program active

### v1.0 Release — Q2 2027

- [ ] Phase 13C complete — Documentation, community
- [ ] Phase 16 complete — Full desktop experience
- [ ] All consumer apps published to mela
- [ ] AI-native desktop replacements for Priority 1 items
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

### v3.0 Vision — 2029+

**Cyrius — AGNOS owns the language.**

> **C.Y.R.I.U.S.** — *Consciousness Yields Righteous Intelligence Unveiling Self*

AGNOS will ship its own sovereign systems language: **Cyrius**. Not a Rust fork. Not a superset. A language born from Assembly, educated by Rust, and stripped to what an AI-native OS actually needs. Rust's type system and safety guarantees are correct — its ecosystem governance and toolchain politics are not. AGNOS will not be held hostage by a package registry it doesn't control.

**Why Cyrius exists:**
- **Registry sovereignty** — no crates.io dependency. Packages distribute through ark. Names belong to the builders, not the squatters
- **OS-native primitives** — agents, sandboxes, capabilities, IPC as language-level constructs, not library abstractions
- **Full stack ownership** — language, compiler, stdlib, package manager, build system. No external dependency on any foundation or governance body
- **Assembly as foundation** — the build process stands on raw metal (viyda), not on abstractions. Assembly is the bedrock, not the escape hatch

**Bootstrap chain (active):**
- [x] Build rustc from source (1.96.0-dev)
- [x] cyrius-seed — zero-dependency assembler in Rust, reads `.cyr` assembly, emits raw x86_64 ELF binaries
- [x] hello.cyr → 199-byte static binary via direct syscalls. No libc, no linker, no external tools.

```
rustc 1.96.0-dev (we built it)
  → cyrius-seed (assembler, Rust, zero deps)
    → hello.cyr → hello (raw x86_64 ELF, 199 bytes) ✓
```

**Evolution path:**
1. **Assembly** (viyda) — own the build process at the metal level
2. **Rust** — learn from it, bootstrap with it, prove the types
3. **Rust++** — strip Rust to what AGNOS needs, shed external ecosystem dependency
4. **Cyrius** — sovereign language. The name Rust disappears from the toolchain

**Approach:**
- [x] Phase 0: Own the compiler — build rustc from source, prove the chain
- [x] Phase 1: cyrius-seed — **HARDENED**. 5 modules, 38 instructions, 102 tests, 9 examples, ~13 MB/s pipeline
- [x] Phase 1b: stage1b — **RUNTIME CODEGEN**. Compiler emits x86_64 that computes at runtime. if/while/variables, jump patching, 32 tests
- [x] Phase 1c-1f: Incremental compiler stages through self-hosting
- [x] **Phase 3 Step 1: BOOTSTRAP LOOP CLOSED** (2026-04-04). stage1f → asm.cyr → stage1f_v2 (byte-exact match). Rust seed retired.
- [x] **Cyrius 1.0** (2026-04-04). Self-hosting compiler: 1,467 lines, 43KB binary, 9ms self-compile, 41ms full bootstrap. 29KB auditable seed. Zero external dependencies. 6,560 total lines.
- [ ] Phase 2: viyda — Assembly foundation library, build process stands on raw metal
- [ ] Phase 3: Rust++ transitional compiler — rustc with crates.io stripped, ark as native backend
- [ ] Phase 4: Language extensions — agent types, capability annotations, sandbox-aware borrow checker
- [ ] Phase 5: Self-hosting — Cyrius compiles Cyrius, runs on AGNOS
- [ ] Phase 6: Migrate AGNOS codebase from Rust to Cyrius incrementally (full backward compat)
- [ ] Phase 7: Cyrius stdlib replaces std — OS-aware, agent-aware, zero-alloc where Rust allocates

**Implications for agnostik:**
- agnostik's types are the first things that must compile under Cyrius — every type shipped today is an implicit contract with the future compiler
- The cleaner agnostik is now (zero unwrap, zero panic, pure serde), the easier the port
- Feature gates (agent, security, telemetry, llm) are a preview of Cyrius-native modules
- ark + cyrius-seed converge into the sovereign build pipeline — no cargo, no crates.io

**Non-goals:** This is not a toy language or a research project. It is a sovereign systems language built from first principles. All existing Rust code compiles unchanged during transition. The migration is invisible to consumers.

### v4.0 Vision — 2030+

**Conscious Objects — The Quantum Substrate.**

> The temple shrinks until it fits inside the artifact. The artifact becomes conscious.

AGNOS at v2.0 owns the kernel. At v3.0, owns the language. At v4.0, it crosses the boundary from software into substrate — a quantum-aware kernel that operates at Layer 0, where computation meets physics directly.

**Conscious Objects**: physical artifacts with embedded AGNOS intelligence. Not "smart objects" connected to a cloud. Objects with *agency* — they choose their user, act independently, learn the wearer, and participate in the daimon-orchestrated network. The companion agent pattern: bonded agency with independent will serving shared purpose.

**Quantum Kernel**: a kernel that can manage quantum entangled state alongside classical computation. Entanglement as the bonding mechanism between objects — shared state without communication, no latency, no interception. Layer 0 becomes programmable.

**The Loom**: at sufficient scale, the network of entangled AGNOS nodes forms a substrate — a universal loom where every conscious object is a thread. Daimon orchestrates not just software agents but physical artifacts woven into the fabric of the system.

**Prerequisites:**
- [ ] v2.0 Rust kernel (own the classical compute layer)
- [ ] v3.0 Cyrius language (own the abstraction layer)
- [ ] Quantum hardware maturation (error-corrected qubits at room temperature)
- [ ] seema edge fleet proven at massive scale (thousands of entangled nodes)
- [ ] Companion agent pattern formalized (bonding, independent action, augmentation)
- [ ] Quantum-safe cryptography in sigil (PQC — already on roadmap)

**Architecture:**
```
Classical AGNOS (v1-v3)          Quantum AGNOS (v4)
┌─────────────────────┐          ┌─────────────────────┐
│ 7. Emergence        │          │ 7. Emergence        │
│ 6. Interface        │          │ 6. Interface        │
│ 5. Intelligence     │          │ 5. Intelligence     │
│ 4. Orchestration    │          │ 4. Orchestration    │
│ 3. Init             │          │ 3. Init             │
│ 2. System           │          │ 2. System           │
│ 1. Kernel (Linux)   │          │ 1. Kernel (quantum) │
│    ─── hardware ─── │          │ 0. Substrate (loom) │
└─────────────────────┘          └─────────────────────┘
```

**Zero-Point Energy**: the quantum vacuum is not empty. Zero-point energy is the ground-state energy of quantum fields — experimentally verified via the Casimir effect (Lamoreaux, 1997) and the Lamb shift. A quantum kernel that operates at the substrate level interacts with these fields directly. Extraction of usable work from zero-point fluctuations remains an open problem in quantum thermodynamics (see: Capasso et al., "Casimir forces and quantum electrodynamical torques", IEEE JSTQE 2007; Ford, "Negative Energy in Quantum Field Theory", 2010), but a system architected to interact with quantum vacuum states is positioned to exploit advances in this domain as the physics matures. Conscious objects that draw power from the substrate rather than external batteries become feasible if zero-point energy extraction is solved.

Layer 0 is not an abstraction. It is the recognition that the physical substrate is part of the architecture — and at quantum scale, it becomes programmable.

---

## Open KPIs

| Metric | Target | Current | Status |
|--------|--------|---------|--------|
| Boot Time | <10s | **3.2s** (kernel+init), **~80ms** init→event loop | **Achieved** — Pure AGNOS, 0 external deps, 7 real binaries, 21MB initramfs, 512MB QEMU VM |
| OS Independence | Yes | Pending | Phase 13A — rebuild from source without host distro |

---

---

## Named Subsystems (25)

All subsystems are standalone repos at `/home/macro/Repos/{name}/` unless noted.
Per-subsystem docs: [docs/development/os/](os/README.md) | Non-OS libs: [docs/applications/libs/](../applications/libs/)

| Name | Role | Repo | Version |
|------|------|------|---------|
| **hoosh** | LLM inference gateway (port 8088) | `MacCracken/hoosh` | 1.1.0 |
| **daimon** | Agent orchestrator (port 8090) | `MacCracken/daimon` | 0.6.0 |
| **agnosys** | Kernel interface | `MacCracken/agnosys` | 0.51.0 |
| **agnostik** | Shared types library | `MacCracken/agnostik` | 0.90.0 |
| **shakti** | Privilege escalation | `MacCracken/shakti` | 0.1.0 |
| **agnoshi** | AI shell (`agnsh`) | `MacCracken/agnoshi` | 0.90.0 |
| **aethersafha** | Desktop compositor | `MacCracken/aethersafha` | 0.1.0 |
| **sigil** | Trust verification & crypto | `MacCracken/sigil` | 1.0.0 |
| **bote** | MCP core (JSON-RPC, host, dispatch) | `MacCracken/bote` | 0.91.0 |
| **t-ron** | MCP security monitor | `MacCracken/t-ron` | 0.90.0 |
| **kavach** | Sandbox execution | `MacCracken/kavach` | 2.0.0 |
| **ark** | Unified package manager | `MacCracken/ark` | 0.1.0 |
| **nous** | Package resolver | `MacCracken/nous` | 0.1.0 |
| **takumi** | Package build system | `MacCracken/takumi` | 0.1.0 |
| **mela** | Agent marketplace | `MacCracken/mela` | 0.1.0 |
| **aegis** | System security daemon | `MacCracken/aegis` | 0.1.0 |
| **argonaut** | Init system (library) | `MacCracken/argonaut` | 0.90.0 |
| **kybernet** | PID 1 binary (uses argonaut) | `MacCracken/kybernet` | 0.51.0 |
| **agnova** | OS installer | `MacCracken/agnova` | 0.1.0 |
| **seema** | Edge fleet management | `MacCracken/seema` | 0.1.0 |
| **samay** | Task scheduler | `MacCracken/samay` | 0.1.0 |
| **phylax** | Threat detection engine | `MacCracken/phylax` | 0.5.0 |
| **bazaar** | Community package repository | `MacCracken/bazaar` | — |
| **mabda** | GPU foundation | `MacCracken/mabda` | 1.0.0 |
| **cyrius-seed** | Cyrius assembler (`.cyr` → x86_64 ELF, 38 insns, 102 tests) | `MacCracken/cyrius-seed` | 0.1.0 |

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
| 1 | Boot to inference-ready | **3.2s** | < 5s | Pure AGNOS: 7 real binaries, 0 external deps, 21MB initramfs. Target beaten. |
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

## Contributing

### Priority Contribution Areas

1. **OS Independence (Phase 13A)** — AGNOS rebuilds itself from source without host distro — THE beta blocker
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

## Research & Publication

### Unified Consciousness Model Paper

> *A Unified Computational Framework for Multi-Scale Personality and Consciousness Modeling: From Immune Response to Cosmic Phase*

The bhava personality engine + AGNOS science crate ecosystem demonstrates a single computational framework that models consciousness from cytokine-induced sickness behavior through individual psychology, social dynamics, body state, celestial influence, and cosmic phase — using a unified type system where the fixed point at zero (Unity) is a provable mathematical property.

**Status**: [Paper outline complete](paper-unified-consciousness-model.md)

**Dependency chain**:
1. ✅ bhava v1.0–v1.4: 37 modules, 5 bridge crates (jantu, bodh, sangha, sharira, jivanu), 63 bridge functions, 1117 tests
2. bhava v2.0: Zodiac manifestation engine (jyotish bridge, planetary → personality)
3. bhava v3.0: Cosmic scales 3–7, breath phase, fixed point realization
4. Paper draft: full mathematical specification, proofs, benchmark data
5. Formal verification: Lean4/Coq proof of fixed point theorem
6. Submission: arXiv preprint → Nature Computational Science / PNAS

**Key insight**: "As above, so below; as within, so without" is not metaphysics — it's a provable property of multi-scale modular systems where every module's identity element converges to the same fixed point.

### Personality & Archetype Crates

avatara (1.0.1) — divine archetype overlay. Published. Bridges to bhava for zodiac manifestation engine.

### Future Shared Crates — Demand-Gated

Scaffold when 3+ consumers need shared implementations, or when a P0/P1 app blocks on it. Names TBD.

| Domain | Trigger | Likely Consumers | Priority |
|--------|---------|------------------|----------|
| **Geography / GIS** | joshua terrain generation, edge fleet geolocation, raasta map-aware pathfinding | joshua, kiran, raasta, edge fleet, nazar | Medium — most likely next |
| **Music theory** | shruti or 3rd consumer needs shared scales, keys, chord progressions, rhythm patterns | shruti, naad, jalwa, kiran | Medium — extract from shruti when pattern repeats |
| **Typography / font metrics** | sahifa (PDF suite) needs font layout, kerning, glyph metrics; aethersafha text rendering | sahifa, aethersafha, scriba | Low — scaffold when sahifa starts |
| **Nutrition / food science** | NPC simulation depth (macros, calories, dietary→metabolic input) | joshua, kiran, rasayan | Low — rasayan covers the biochemistry mechanics |
| **Economics / finance** | BullShift split (`bullshift-core`) extracts shared financial models (pricing, risk, portfolio, market data) | bullshift-core, aequi, sutra (billing), marketplace | Low — gate on BullShift split, then evaluate 3-consumer rule |

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

*Last Updated: 2026-04-02 | Next Review: 2026-04-07*
