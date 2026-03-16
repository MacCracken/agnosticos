# AGNOS Development Roadmap

> **Status**: Pre-Beta | **Last Updated**: 2026-03-16
> **Userland complete** — 10900+ tests (3723+ agent-runtime, 1554 ai-shell), ~84% coverage, 0 warnings
> **Recipes**: 109 base + 53 desktop + 25 AI + 9 network + 8 browser + 15 marketplace + 4 python + 3 database + 29 edge = 255 total
> **Phases 10–14 complete** | **Phase 15A**: Core scanning done (phylax) | **Audit**: 16 rounds
> **MCP Tools**: 106 built-in + external registration

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

---

## Phase 13A — Self-Hosting Validation (BETA BLOCKER)

**This is the single most important remaining work.** Without it, AGNOS is a Debian overlay.

### Infrastructure (Done)

| # | Item | Status | Notes |
|---|------|--------|-------|
| 1 | Bootstrap toolchain script | **Done** | `scripts/bootstrap-toolchain.sh` — LFS Ch. 5-6 cross-compiler (binutils, GCC, glibc, libstdc++) |
| 2 | Chroot enter script | **Done** | `scripts/enter-chroot.sh` — mounts /proc /sys /dev, interactive or command mode |
| 3 | Package build engine | **Done** | `scripts/ark-build.sh` — cross-compilation, signing, hardening, deterministic builds |
| 4 | Selfhost validator (shell) | **Done** | `scripts/selfhost-validate.sh` — 4-phase validation (toolchain, kernel, userland, packages) |
| 5 | Selfhost validator (Rust) | **Done** | `agent-runtime/src/selfhost.rs` — 38 tests, programmatic validation |
| 6 | Base recipes | **Done** | 109 recipes in `recipes/base/` (GCC 15.2, Rust 1.89, Linux 6.6.72, glibc 2.42) |
| 7 | Source tree in ISO | **Done** | `build-iso.sh` bundles `/usr/src/agnos` with recipes, scripts, userland source, kernel |
| 8 | Multi-stage build script | **Done** | `scripts/build-selfhosting-iso.sh` — 5-stage pipeline (download → bootstrap → chroot build → userland → ISO) |

### Validation (Remaining — requires real hardware/QEMU execution)

| # | Item | Status | Notes |
|---|------|--------|-------|
| 1 | Run bootstrap-toolchain.sh end-to-end | Not started | Build cross-compiler from source tarballs |
| 2 | Build base system in chroot | Not started | ark-build all 109 base recipes in order |
| 3 | Build AGNOS userland on target | Not started | `cargo build --release --workspace` inside AGNOS |
| 4 | Build kernel modules on target | Not started | Compile AGNOS kernel modules without host |
| 5 | Selfhost-validate passes all phases | Not started | Run `selfhost-validate --phase all` on booted ISO |
| 6 | CI automation | Not started | GitHub Actions: build ISO → boot QEMU → validate |

**Critical path**: Download tarballs → bootstrap-toolchain.sh → enter-chroot.sh → ark-build recipes → cargo build userland → selfhost-validate

**To attempt now**: `sudo LFS=/mnt/agnos ./scripts/build-selfhosting-iso.sh`

---

## Phase 16 — Desktop Completeness (NEW)

**Strategy**: Package existing open-source tools via takumi recipes to provide a complete desktop experience *now*. AI-native replacements come later (see `os_long_term.md`).

### 16A — Essential Desktop Packages (ship-with-ISO)

These must be in the ISO image for AGNOS to function as a daily-driver desktop.

| # | Need | Package | Recipe | Status | Notes |
|---|------|---------|--------|--------|-------|
| 1 | File Manager | Thunar | `recipes/desktop/thunar.toml` | Not started | Xfce file manager, lightweight, Wayland via XWayland |
| 2 | Terminal Emulator | Foot | `recipes/desktop/foot.toml` | Not started | Wayland-native, fast, minimal deps |
| 3 | Text Editor | Helix | `recipes/desktop/helix.toml` | Not started | Modern, Rust-native, no config needed |
| 4 | PDF Viewer | Zathura | `recipes/desktop/zathura.toml` | Not started | Lightweight, plugin-based (PDF/DJVU/PS) |
| 5 | Image Viewer | imv | `recipes/desktop/imv.toml` | Not started | Wayland-native, fast |
| 6 | Media Player | mpv | `recipes/desktop/mpv.toml` | Not started | PipeWire audio, Wayland, GPU decode |
| 7 | Notification Daemon | mako | `recipes/desktop/mako.toml` | Not started | Wayland-native, lightweight |
| 8 | Clipboard Manager | cliphist | `recipes/desktop/cliphist.toml` | Not started | wl-clipboard + history |
| 9 | App Launcher | fuzzel | `recipes/desktop/fuzzel.toml` | Not started | Wayland-native dmenu/rofi alternative |
| 10 | Archive Manager | file-roller | `recipes/desktop/file-roller.toml` | Not started | Or bsdtar CLI |

### 16B — Input & Hardware Detection

| # | Need | Approach | Status | Notes |
|---|------|----------|--------|-------|
| 1 | Touchscreen detection | libinput + udev rules | Not started | Auto-detect touch devices, enable tap-to-click, gesture support in aethersafha |
| 2 | Touch gestures | libinput-gestures or custom | Not started | Pinch-zoom, swipe between workspaces, three-finger drag |
| 3 | On-screen keyboard | squeekboard or custom | Not started | Required for tablet/all-in-one without physical keyboard |
| 4 | HiDPI / scaling | Wayland fractional scaling | Not started | Auto-detect display DPI, set appropriate scale factor |
| 5 | Stylus / pen input | libinput tablet support | Not started | Pressure sensitivity, palm rejection |

### 16C — System Configuration

| # | Need | Approach | Status | Notes |
|---|------|----------|--------|-------|
| 1 | System Settings UI | Build custom (AGNOS-specific) | Not started | Cannot package — must be native to AGNOS config model |
| 2 | Network Manager GUI | iwgtk or nm-applet | Not started | WiFi/VPN management for non-CLI users |
| 3 | Bluetooth Manager | blueman | Not started | BlueZ recipe already exists |
| 4 | Display Settings | wlr-randr + custom UI | Not started | Multi-monitor, resolution, scaling |
| 5 | Sound Settings | pavucontrol / helvum | Not started | PipeWire routing and volume |
| 6 | Firewall GUI | Custom (wraps nftables) | Not started | AGNOS-specific security model |

### 16D — Desktop Polish

| # | Need | Package | Status | Notes |
|---|------|---------|--------|-------|
| 1 | Wallpaper/Themes | Custom for aethersafha | Not started | Default AGNOS theme, wallpaper selector |
| 2 | Fonts | Noto + Liberation + JetBrains Mono | Partial | Some fonts in existing recipes |
| 3 | Icons | Papirus or custom | Not started | Icon theme for desktop |
| 4 | Cursor theme | Adwaita or custom | Not started | Wayland cursor theme |
| 5 | GTK theme | Adwaita dark variant | Not started | For XWayland GTK apps |
| 6 | Keyring / Secrets | gnome-keyring or KeePassXC | Not started | Integrates with agnos-common secrets |
| 7 | Printing GUI | system-config-printer | Not started | cups.toml recipe exists |
| 8 | Disk utility | GNOME Disks or custom | Not started | Partition management GUI |

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

### 15A — Core Scanning Engine

| # | Item | Status | Notes |
|---|------|--------|-------|
| 1 | YARA-compatible rule engine | **Done** | Native Rust parser for hex patterns; no libyara dependency. 5 built-in rules, 65 tests |
| 2 | File content inspection | **Done** | Magic bytes (ELF, PE, shebang), embedded payloads, polyglot detection, entropy analysis |
| 3 | Signature database (`.phylax-db`) | Not started | Signed, versioned threat definitions distributed via ark |
| 4 | On-access scanning (fanotify) | Not started | Real-time filesystem monitoring via `agnos-sys` fanotify bindings |
| 5 | Scan API endpoints | **Done** | `/v1/scan/file`, `/v1/scan/bytes`, `/v1/scan/status`, `/v1/scan/history`, `/v1/scan/rules` |
| 6 | MCP tools | **Done** | `phylax_scan`, `phylax_status`, `phylax_rules`, `phylax_findings`, `phylax_quarantine` (106 total) |
| 7 | agnoshi intents | **Done** | "scan /path for threats", "show threat findings", "scanner status", "list rules", "scan history" |

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

---

## Phase 13G — Consumer App Validation

| # | App | MCP Tools | Intents | Release | Bundle Test | Notes |
|---|-----|-----------|---------|---------|-------------|-------|
| 1 | SecureYeoman | 7 yeoman_* | 7 | Yes | Not started | Flagship |
| 2 | Photis Nadi | 8 photis_* | 8 | Yes | Not started | Flutter |
| 3 | BullShift | 7 bullshift_* | 7 | Yes | Not started | Trading |
| 4 | AGNOSTIC | 5 agnostic_* | 5 | Yes | Not started | Python |
| 5 | Delta | 7 delta_* | 7 | Yes | Not started | Code hosting |
| 6 | Aequi | 7 aequi_* | 7 | Yes | Not started | Accounting |
| 7 | Synapse | 7 synapse_* | 7 | Yes | Not started | LLM management |
| 8 | Shruti | 7 shruti_* | 7 | Yes | Not started | DAW |
| 9 | Tazama | 7 tazama_* | 7 | Yes | Not started | Video editor |
| 10 | Rasa | 9 rasa_* | 9 | Yes | Not started | Image editor |
| 11 | Mneme | 7 mneme_* | 7 | Yes | Not started | Knowledge base |
| 12 | Nazar | 5 nazar_* | — | Scaffolded | Not started | System monitor |
| 13 | Selah | 5 selah_* | — | Scaffolded | Not started | Screenshot |
| 14 | Abaco | 5 abaco_* | — | Scaffolded | Not started | Calculator |
| 15 | Rahd | 5 rahd_* | — | Scaffolded | Not started | Calendar |

---

## Engineering Backlog

### Active

| # | Priority | Item | Notes |
|---|----------|------|-------|
| | | | |

---

## Release Roadmap

### Beta Release — Q4 2026

**Critical path**: 13A → 16A → 13C → Beta

**Criteria:**
- [x] Phase 10 complete — 108 base system recipes, self-hosting toolchain
- [x] Phase 11 complete — 88 desktop, networking & AI/ML recipes
- [x] Phase 12 complete — Argonaut init, ark package manager, agnova installer
- [x] Phase 13B complete — GPU drivers, WiFi, Bluetooth, Thunderbolt, printing
- [x] Phase 13D complete — 11 consumer apps integrated
- [x] Phase 15A partial — Phylax core scanning engine
- [x] AGNOS boots from ISO on bare metal (UEFI) and QEMU
- [ ] **Self-hosting: can rebuild itself from source (13A)** ← PRIMARY BLOCKER
- [ ] **Desktop essentials packaged (16A)** — file manager, terminal, PDF viewer, etc.
- [ ] Third-party security audit complete
- [ ] Community testing program active

### v1.0 Release — Q2 2027

**Criteria:**
- [ ] Phase 13C complete — Documentation, community
- [ ] Phase 16 complete — Full desktop experience
- [ ] All consumer apps published to mela
- [ ] AI-native desktop replacements for Priority 1 items (see `os_long_term.md`)
- [x] Python runtime management
- [x] Enterprise features: SSO, audit logging, mTLS
- [ ] 6 months of beta testing with no critical bugs
- [ ] Commercial support available

---

## Key Performance Indicators (KPIs)

### Current Status (as of 2026-03-16)

| Metric | Target | Current | Status |
|--------|--------|---------|--------|
| Code Coverage | >80% | ~84.3% | Met |
| Test Pass Rate | 100% | 100% | Met |
| Total Tests | 400+ | 10900+ | Met |
| Agent Spawn Time | <500ms | ~300ms | Met |
| Shell Response Time | <100ms | ~50ms | Met |
| Memory Overhead | <2GB | ~1.2GB | Met |
| Boot Time | <10s | N/A | Pending (Phase 13A) |
| CIS Compliance | >80% | ~85% | Met |
| Stub Implementations | 0 | 0 | Met |
| Compiler Warnings | 0 | 0 | Met |
| Base System Recipes | ~108 | 109 | Complete |
| Desktop/AI Stack Recipes | ~62 | 88 | Complete |
| Edge Recipes | ~30 | 29 | Complete |
| Marketplace Recipes | 11 | 15 | Complete (11 released + 4 scaffolded) |
| MCP Tools | — | 106 | Complete (10 agnos + 5 aequi + 5 agnostic + 5 delta + 8 photis + 5 edge + 5 shruti + 5 tazama + 5 rasa + 5 mneme + 5 synapse + 7 bullshift + 7 yeoman + 5 phylax + others) |
| Consumer Apps | 6 | 15 | 11 released + 4 scaffolded |
| Recipe Validation Errors | 0 | 0 | Complete |
| Security Audit Rounds | 15 | 16 | Complete |
| Self-Hosting | Yes | Pending | Phase 13A — THE blocker |

### By Component

| Component | Tests | Notes |
|-----------|-------|-------|
| agnos-common | 307 | Secrets, telemetry, LLM types, manifest, rate limits, audit chain |
| agnos-sys | 750+ | 16 modules: audit, mac, netns, dmverity, luks, ima, tpm, secureboot, certpin, bootloader, journald, udev, fuse, pam, update, llm |
| agent-runtime | 3723+ | Phylax (65), 106 MCP tools, orchestrator, IPC, sandbox, registry, marketplace, federation, migration, scheduler, PQC, safety, finetune, formal_verify, sandbox_v2, rl_optimizer, cloud, collaboration, sigil, aegis, takumi, argonaut, agnova, ark, edge, grpc, service_mesh, oidc, delegation, vector_rest, marketplace_backend, selfhost, webview, python_runtime |
| llm-gateway | 860 | 15 providers, rate limiting, streaming, cert pinning, hardware acceleration, token budgets |
| ai-shell | 1554 | 55+ intents (including 5 phylax), approval workflow, dashboard, aliases |
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

## Named Subsystems (18)

| Name | Role | Component |
|------|------|-----------|
| **hoosh** | LLM inference gateway (port 8088, 15 providers) | `llm-gateway/` |
| **daimon** | Agent orchestrator (port 8090, 106 MCP tools) | `agent-runtime/` |
| **agnosys** | Kernel interface | `agnos-sys/` |
| **agnostik** | Shared types library | `agnos-common/` |
| **shakti** | Privilege escalation | `agnos-sudo/` |
| **agnoshi** | AI shell (`agnsh`, 55+ intents) | `ai-shell/` |
| **aethersafha** | Desktop compositor | `desktop-environment/` |
| **ark** | Unified package manager | `ark.rs`, `/v1/ark/*` |
| **nous** | Package resolver daemon | `nous.rs` |
| **takumi** | Package build system | `takumi.rs` |
| **mela** | Agent marketplace | `marketplace/` module |
| **aegis** | System security daemon | `aegis.rs` |
| **sigil** | Trust verification | `sigil.rs` |
| **argonaut** | Init system | `argonaut.rs` |
| **agnova** | OS installer | `agnova.rs` |
| **phylax** | Threat detection engine | `phylax.rs` |
| **vansh** | Voice AI shell (planned) | TBD |
| **AGNOS** | The OS itself | — |

---

## Contributing

### Priority Contribution Areas

1. **Self-hosting on-target (Phase 13A)** — Build AGNOS on AGNOS — THE beta blocker
2. **Desktop recipes (Phase 16A)** — Package Thunar, Foot, Zathura, mpv, mako, etc.
3. **SHA256 verification** — Fill in real checksums for all recipes
4. **Documentation (Phase 13C)** — Video tutorials, support portal
5. **Community testing** — Beta tester enrollment + bug tracker setup

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
- **Long-term app roadmap**: [os_long_term.md](/os_long_term.md)
- **LFS Reference**: https://www.linuxfromscratch.org/lfs/view/stable/
- **BLFS Reference**: https://www.linuxfromscratch.org/blfs/view/stable/

---

*Last Updated: 2026-03-16 | Next Review: 2026-03-23*
