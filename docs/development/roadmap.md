# AGNOS Development Roadmap

> **Status**: Pre-Beta | **Last Updated**: 2026-03-17-1
> **Userland complete** — 11000+ tests (3800+ agent-runtime, 1554 ai-shell), ~84% coverage, 0 warnings
> **Recipes**: 115 base + 69 desktop + 25 AI + 9 network + 8 browser + 18 marketplace + 4 python + 3 database + 30 edge = 281 total
> **Build order**: 176 packages in `recipes/build-order.txt` (base + desktop, dependency-ordered)
> **Phases 10–14 complete** | **Phase 15A**: Core scanning done (phylax) | **Audit**: 16 rounds
> **MCP Tools**: 140 built-in + external registration
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
| 6 | CI automation | In progress | GitHub Actions: `publish-toolchain.yml`, `selfhost-build.yml`, `selfhost-validation.yml` — bootstrap toolchain added to CI/CD |

**Critical path**: Download tarballs → bootstrap-toolchain.sh → enter-chroot.sh → ark-build recipes → cargo build userland → selfhost-validate

**To attempt now**: `sudo LFS=/mnt/agnos ./scripts/build-selfhosting-iso.sh`

---

## Phase 16 — Desktop Completeness (NEW)

**Strategy**: Package existing open-source tools via takumi recipes to provide a complete desktop experience *now*. AI-native replacements come later (see `os_long_term.md`).

### 16A — Essential Desktop Packages (ship-with-ISO)

These must be in the ISO image for AGNOS to function as a daily-driver desktop.

| # | Need | Package | Recipe | Status | Notes |
|---|------|---------|--------|--------|-------|
| 1 | File Manager | yazi | `recipes/desktop/yazi.toml` | **Done** | Modern Rust TUI file manager, async, rich previews, zero GUI deps. Thunar deferred (heavy Xfce dep chain) |
| 2 | Terminal Emulator | Foot | `recipes/desktop/foot.toml` | **Done** | Wayland-native, fast, minimal deps. Kitty deferred to post-beta (needs Go 1.26+) |
| 3 | Text Editor | Helix | `recipes/desktop/helix.toml` | **Done** | Modern, Rust-native, default config included |
| 4 | PDF Viewer | Zathura | `recipes/desktop/zathura.toml` | **Done** | Lightweight, plugin-based (PDF/DJVU/PS) |
| 5 | Image Viewer | imv | `recipes/desktop/imv.toml` | **Done** | Wayland-native, fast, HEIF/SVG/WebP support |
| 6 | Media Player | mpv | `recipes/desktop/mpv.toml` | **Done** | PipeWire audio, Vulkan GPU-next, Wayland, VA-API hwdec |
| 7 | Notification Daemon | mako | `recipes/desktop/mako.toml` | **Done** | Wayland-native, lightweight, systemd user service + default config |
| 8 | Clipboard Manager | cliphist | `recipes/desktop/cliphist.toml` | **Done** | Go-based, wl-clipboard + systemd user service |
| 9 | App Launcher | fuzzel | `recipes/desktop/fuzzel.toml` | **Done** | Wayland-native dmenu/rofi alternative |
| 10 | Archive Manager | ark CLI | — | **Done** | Already supported via `ark extract`/`ark compress` in daimon + libarchive in base. No GUI recipe needed |

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
| 6 | MCP tools | **Done** | `phylax_scan`, `phylax_status`, `phylax_rules`, `phylax_findings`, `phylax_quarantine` (122 total) |
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
| 16 | Tarang | 8 tarang_* | 8 | Yes | Not started | Media framework (73 tests) |
| 17 | Jalwa | 8 jalwa_* | 8 | Yes | Not started | Media player (110+ tests), built on tarang. Priority 1 in os_long_term |

---

## SecureYeoman & Agnostic Integration

*Cross-project integration items for the AGNOS ecosystem.*

### SecureYeoman Integration

*SY's GPU-aware inference routing works standalone (probes local nvidia-smi/rocm-smi directly). These items let AGNOS expose fleet GPU data that SY can optionally consume for distributed routing.*

| # | Item | Effort | Status | Notes |
|---|------|--------|--------|-------|
| 1 | GPU telemetry MCP tool | Small | **Done** | `agnos_gpu_status` — probes NVIDIA/AMD/Intel GPUs via `ResourceManager::detect_gpus()`. Returns id, name, VRAM total/available, compute capability |
| 2 | Local model inventory MCP tool | Small | **Done** | `agnos_local_models` — queries hoosh `GET /v1/models` for locally available models (Ollama, llama.cpp, etc.). Graceful fallback when hoosh offline |
| 3 | Firecracker GPU passthrough | Medium | **Done** | `BackendConfig.device_passthrough` field. VM config conditionally enables PCI bus and adds VFIO device entries when GPU paths provided (e.g. `/dev/nvidia0`) |
| 4 | Fleet GPU heartbeat | Medium | **Done** | `HeartbeatRequest` accepts `gpu_utilization_pct`, `gpu_memory_used_mb`, `gpu_temperature_c`. Stored on `EdgeNode`, aggregated in `GET /v1/edge/dashboard` (avg utilization, total VRAM used, reporting node count) |

### Agnostic Integration

| # | Item | Effort | Status | Notes |
|---|------|--------|--------|-------|
| 1 | Crew GPU resource requirements | Small | Not started | Allow crew definitions to specify GPU requirements. Orchestrator routes to agents with matching GPU capability |
| 2 | Agnostic crew listing from AGNOS | Small | **Done** | `agnostic_list_crews` MCP tool + `AgnosticListCrews` agnoshi intent. Status filter + pagination via `GET /crews` |
| 3 | Agnostic crew cancellation from AGNOS | Small | **Done** | `agnostic_cancel_crew` MCP tool + `AgnosticCancelCrew` agnoshi intent. `POST /crews/{crew_id}/cancel` |
| 4 | Agnostic crew status in AGNOS HUD | Medium | Not started | Surface active Agnostic crews in aethersafha HUD with real-time status from `GET /crews` endpoint |

---

## GPU Awareness

**Goal**: Make GPU resources a first-class concept across the stack — from hardware detection through agent scheduling, inference routing, and fleet telemetry.

**Existing infrastructure**: `resource.rs` (GPU detection for NVIDIA/AMD/Intel, allocation/release per agent), `acceleration.rs` (CUDA/ROCm/Metal accelerator types, quantization routing), NVIDIA/AMD/Intel driver recipes in 13B.

### G1 — Orchestrator GPU-Aware Scheduling

| # | Item | Status | Notes |
|---|------|--------|-------|
| 1 | GPU requirement in `TaskRequirements` | **Done** | `gpu_required`, `min_gpu_memory`, `required_compute_capability` fields. Weights rebalance: 35/25/15/15/10 when GPU required |
| 2 | GPU headroom in `score_agent` | **Done** | `score_gpu()` evaluates VRAM headroom + compute capability filtering. Best GPU ratio used as score (0.0–1.0) |
| 3 | GPU allocation on task dispatch | **Done** | `auto_assign_task()` allocates via `ResourceManager::allocate_gpu()`. `handle_result()` releases on completion |

### G2 — Hoosh Inference GPU Routing

| # | Item | Status | Notes |
|---|------|--------|-------|
| 1 | GPU-aware model placement | Not started | Hoosh selects provider based on GPU availability. Local Ollama/llama.cpp preferred when GPU has capacity |
| 2 | VRAM budget per model | Not started | Track VRAM consumption per loaded model. Prevent OOM by rejecting loads when VRAM budget exceeded |
| 3 | Privacy-aware GPU routing | Not started | Route sensitive inference to local GPU when available, cloud only when privacy policy allows |
| 4 | Quantization auto-select | Not started | Auto-select quantization level based on available VRAM (Q4 for <8GB, Q8 for <16GB, FP16 for 16GB+) |

### G3 — Edge Fleet GPU Telemetry

| # | Item | Status | Notes |
|---|------|--------|-------|
| 1 | GPU metrics in edge heartbeat | Not started | Include GPU utilization, VRAM usage, temperature in edge node heartbeat payload |
| 2 | Fleet GPU dashboard | Not started | `/v1/edge/dashboard` aggregates GPU stats across fleet. Surface in aethersafha |
| 3 | GPU capability routing | Not started | Edge fleet routes inference requests to nodes with matching GPU capability (CUDA compute version, VRAM) |
| 4 | Local model registry sync | Not started | Edge nodes advertise locally-loaded models to hoosh. Smart routing offloads inference to nodes with warm models |

### G4 — Consumer App GPU Integration

| # | Item | Status | Notes |
|---|------|--------|-------|
| 1 | Crew GPU resource requirements | Not started | Agnostic crew definitions specify GPU requirements. Orchestrator routes to agents with matching GPU capability |
| 2 | GPU capability probe MCP tool | Not started | `agnos_gpu_info` MCP tool exposes detected GPUs (vendor, VRAM, compute capability) to consumer apps |
| 3 | Synapse training GPU allocation | Not started | Synapse fine-tuning jobs request GPU allocation via daimon. VRAM-aware batch size selection |
| 4 | Tarang/Jalwa hardware decode | Not started | Expose VA-API/NVDEC availability to tarang for hardware-accelerated video decode on GPU-equipped nodes |

---

## Engineering Backlog

### Active — Module Refactoring

Large single-file modules (>1500 lines) that should be split into module directories for maintainability. Prioritized by size and complexity.

| # | Priority | Module | Lines | Proposed Split | Effort |
|---|----------|--------|-------|----------------|--------|
| R1 | Medium | `argonaut.rs` | 3873 | `argonaut/` → boot, services, runlevels, edge_boot, tests | Medium |
| R2 | Medium | `agnova.rs` | 3603 | `agnova/` → partitioning, rootfs, config, validation, tests | Medium |
| R3 | Medium | `network_tools.rs` | 3398 | `network_tools/` → nmap, nftables, dns, wifi, capture, tests | Medium |
| R4 | ~~P0~~ | ~~`orchestrator.rs`~~ | ~~3259~~ | **Done** (2026.3.17-1) — `orchestrator/` → mod, types, lifecycle, scheduling, scoring, routing, state, tests (8 files) | — |
| R5 | Low | `ark.rs` | 2873 | `ark/` → resolver, installer, manifest, signing, tests | Medium |
| R6 | Low | `service_manager.rs` | 2630 | `service_manager/` → lifecycle, systemd, health, tests | Small |
| R7 | Low | `federation.rs` | 2565 | `federation/` → discovery, sync, vector_store, gossip, tests | Medium |
| R8 | Low | `sigil.rs` | 2123 | `sigil/` → verify, chain, policy, tests | Small |
| R9 | Low | `edge.rs` | 2075 | `edge/` → fleet, ota, telemetry, routing, tests | Small |
| R10 | Low | `safety.rs` | 2062 | `safety/` → injection, guardrails, policy, tests | Small |

**Pattern to follow**: `sandbox_mod/` (completed in 2026.3.17) and `orchestrator/` (completed in 2026.3.17-1) — re-exports in `mod.rs`, old files deleted. Note: avoid naming submodules `core` (conflicts with Rust's `core` crate in rustfmt).

### Active — Build & Distribution

| # | Priority | Item | Notes |
|---|----------|------|-------|
| B1 | High | Selfhost pipeline builds all 176 packages | `selfhost-build.yml` updated, needs first full run |
| B2 | High | RPi4 hardware boot test | Firmware blobs added, needs physical validation |
| B3 | Medium | SHA256 checksums for all recipes | Most recipes have empty `sha256 = ""` — fill from upstream |
| B4 | Medium | Debian removal from installer scripts | `build-installer.sh` / `build-sdcard.sh` still fall back to debootstrap when no base rootfs |

### Active — Sandbox & Security

| # | Priority | Item | Notes |
|---|----------|------|-------|
| S1 | High | Wire credential proxy to sandbox lifecycle | `CredentialProxyManager` exists, needs integration with agent spawn in `orchestrator.rs` |
| S2 | High | Wire externalization gate to network egress | `ExternalizationGate` exists, needs integration at network boundary |
| S3 | Medium | gVisor/Firecracker runtime execution | Config generation + OCI/VM lifecycle done, needs actual process spawning via `tokio::process::Command` |
| S4 | Medium | SGX/SEV hardware validation | Backends implemented, need hardware to test |
| S5 | Low | Offender tracker → sigil trust integration | `OffenderTracker` trust scores should feed into sigil's trust chain |

### Resolved

| # | Item | Resolution |
|---|------|------------|
| 1 | Go toolchain bump (1.24.1 → 1.26+) | **Done** — `recipes/ai/go.toml` updated to 1.26.1 |
| 2 | Sandbox module consolidation | **Done** — 7 files → `sandbox_mod/` (2026.3.17). 303 tests, >95% coverage |
| 3 | Agnostic MCP API realignment | **Done** — 21 tools updated to match Agnostic v2026.3.16 API (2026.3.17) |

---

## Release Roadmap

### Beta Release — Q4 2026

**Critical path**: 13A → 16A → 13C → Beta

**Criteria:**
- [x] Phase 10 complete — 108 base system recipes, self-hosting toolchain
- [x] Phase 11 complete — 88 desktop, networking & AI/ML recipes
- [x] Phase 12 complete — Argonaut init, ark package manager, agnova installer
- [x] Phase 13B complete — GPU drivers, WiFi, Bluetooth, Thunderbolt, printing
- [x] Phase 13D complete — 17 consumer apps integrated
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
| Desktop/AI Stack Recipes | ~62 | 79 | Complete |
| Edge Recipes | ~30 | 30 | Complete |
| Marketplace Recipes | 11 | 18 | Complete (11 released + 7 scaffolded) |
| MCP Tools | — | 140 | Complete (12 agnos + 5 aequi + 23 agnostic + 7 delta + 8 photis + 5 edge + 7 shruti + 8 tarang + 8 jalwa + 9 rasa + 7 mneme + 7 synapse + 7 bullshift + 7 yeoman + 5 phylax + others) |
| Consumer Apps | 6 | 17 | 11 released + 6 scaffolded |
| Recipe Validation Errors | 0 | 0 | Complete |
| Security Audit Rounds | 15 | 16 | Complete |
| Self-Hosting | Yes | Pending | Phase 13A — THE blocker |

### By Component

| Component | Tests | Notes |
|-----------|-------|-------|
| agnos-common | 307 | Secrets, telemetry, LLM types, manifest, rate limits, audit chain |
| agnos-sys | 750+ | 16 modules: audit, mac, netns, dmverity, luks, ima, tpm, secureboot, certpin, bootloader, journald, udev, fuse, pam, update, llm |
| agent-runtime | 3723+ | Phylax (65), 122 MCP tools, orchestrator, IPC, sandbox, registry, marketplace, federation, migration, scheduler, PQC, safety, finetune, formal_verify, sandbox_v2, rl_optimizer, cloud, collaboration, sigil, aegis, takumi, argonaut, agnova, ark, edge, grpc, service_mesh, oidc, delegation, vector_rest, marketplace_backend, selfhost, webview, python_runtime |
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

## Named Subsystems (18)

| Name | Role | Component |
|------|------|-----------|
| **hoosh** | LLM inference gateway (port 8088, 15 providers) | `llm-gateway/` |
| **daimon** | Agent orchestrator (port 8090, 122 MCP tools) | `agent-runtime/` |
| **agnosys** | Kernel interface | `agnos-sys/` |
| **agnostik** | Shared types library | `agnos-common/` |
| **shakti** | Privilege escalation | `agnos-sudo/` |
| **agnoshi** | AI shell (`agnsh`, 61+ intents) | `ai-shell/` |
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

*Last Updated: 2026-03-17 | Next Review: 2026-03-24*
