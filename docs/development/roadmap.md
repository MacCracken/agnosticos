# AGNOS Development Roadmap

> **Status**: Pre-Beta | **Last Updated**: 2026-03-17
> **Userland complete** — 11000+ tests (3900+ agent-runtime, 1554 ai-shell), ~84% coverage, 0 warnings
> **Recipes**: 115 base + 69 desktop + 25 AI + 9 network + 8 browser + 18 marketplace + 4 python + 3 database + 30 edge = 281 total
> **Build order**: 176 packages in `recipes/build-order.txt` (base + desktop, dependency-ordered)
> **Phases 10–14 complete** | **Phase 15A**: Core scanning done (phylax) | **Audit**: 16 rounds
> **MCP Tools**: 144 built-in + external registration
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

### 16E — Aethersafha Configurability

| # | Item | Status | Notes |
|---|------|--------|-------|
| 1 | User-facing config file | Not started | Hyprland-style config DSL or TOML for keybinds, gaps, borders, animations |
| 2 | Session selector in argonaut | Not started | TTY chooser or mini display manager. Select aethersafha, sway, hyprland (from bazaar) |
| 3 | Hot-reload config | Not started | Watch config file, apply changes without restart |
| 4 | Plugin API for bars/widgets | Not started | External status bars (waybar) can integrate via IPC protocol |

---

## Bazaar — Community Package Repository

**Subsystem**: bazaar (Persian: بازار). Repo: `github.com/MacCracken/bazaar`. Recipe: `recipes/base/bazaar.toml`.

**43 seed recipes** across 8 categories:

| Category | Count | Highlights |
|----------|-------|------------|
| AI | 11 | ollama, llama.cpp, whisper.cpp, stable-diffusion.cpp, onnxruntime, vllm, piper-tts, aider, open-webui, comfyui, fabric, lmstudio, pytorch |
| Desktops | 17 | Sway (5), Hyprland (8), shared Wayland tools (4: waybar, wofi, grim, slurp, wl-clipboard) |
| Tools | 14 | ripgrep, fd, bat, eza, fzf, tmux, htop, btop, lazygit, starship, zoxide, dust, tokei, hyperfine, git-delta |
| Editors | 3 | neovim, vim, micro |
| Networking | 3 | wireguard-tools, bandwhich, mtr |
| Security | 1 | keepassxc |
| Media | 2 | ffmpeg, yt-dlp |

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
| 8 | ESP32-S3 (MCU) | xtensa | Edge/IoT | Planned | MQTT agent, sensor telemetry, TinyML. Recipe: `recipes/edge/esp32-agent.toml` |
| 9 | ESP32-C3 (MCU) | riscv32 | Edge/IoT | Planned | RISC-V core, lowest power, WiFi + Thread/Zigbee |

---

## Phase 13G — Consumer App Validation

| # | App | MCP Tools | Intents | Release | Bundle Test | Notes |
|---|-----|-----------|---------|---------|-------------|-------|
| 1 | SecureYeoman | 7 yeoman_* | 7 | Yes | Not started | Flagship |
| 2 | Photis Nadi | 8 photis_* | 8 | Yes | Not started | Flutter |
| 3 | BullShift | 7 bullshift_* | 7 | Yes | Not started | Trading |
| 4 | AGNOSTIC | 23 agnostic_* | 14 | Yes | Not started | Python |
| 5 | Delta | 7 delta_* | 7 | Yes | Not started | Code hosting |
| 6 | Aequi | 7 aequi_* | 7 | Yes | Not started | Accounting |
| 7 | Synapse | 7 synapse_* | 7 | Yes | Not started | LLM management |
| 8 | Shruti | 7 shruti_* | 7 | Yes | Not started | DAW |
| 9 | Tazama | 7 tazama_* | 7 | Yes | Not started | Video editor |
| 10 | Rasa | 9 rasa_* | 9 | Yes | Not started | Image editor |
| 11 | Mneme | 7 mneme_* | 7 | Yes | Not started | Knowledge base |
| 12 | Nazar | 5 nazar_* | — | Yes | Not started | System monitor |
| 13 | Selah | 5 selah_* | — | Yes (MVP) | Not started | Screenshot, no AI integration yet |
| 14 | Abaco | 5 abaco_* | — | Yes | Not started | Calculator |
| 15 | Rahd | 5 rahd_* | — | Yes | Not started | Calendar |
| 16 | Tarang | 8 tarang_* | 8 | Yes | Not started | Media framework (73 tests) |
| 17 | Jalwa | 8 jalwa_* | 8 | Yes | Not started | Media player (110+ tests), built on tarang. Priority 1 in os_long_term |

---

## SecureYeoman & Agnostic Integration

*Cross-project integration items for the AGNOS ecosystem.*

### Agnostic Integration — Complete

*All 13 items resolved. Data APIs + aethersafha HUD widgets all implemented.*

---

## Engineering Backlog

### Module Refactoring — Complete

All 10 large modules (>2000 lines) have been split into focused module directories. Pattern: `mod.rs` re-exports, old monolith deleted, `#[cfg(test)] mod tests;` in dedicated file. Avoid naming submodules `core` (rustfmt conflict).

### Active — Build & Distribution

| # | Priority | Item | Notes |
|---|----------|------|-------|
| B1 | High | Selfhost pipeline builds all 176 packages | `selfhost-build.yml` updated, needs first full run |
| B2 | High | RPi4 hardware boot test | Firmware blobs added, needs physical validation |
| B3 | Medium | SHA256 checksums for all recipes | Most recipes have empty `sha256 = ""` — fill from upstream |
| B4 | Medium | Debian removal from installer scripts | `build-installer.sh` / `build-sdcard.sh` still fall back to debootstrap when no base rootfs |
| B5 | Medium | Bazaar community repo infrastructure | Git-based community recipe index (like AUR). `ark bazaar` subcommand. Recipe: `recipes/base/bazaar.toml`. `Community` variant in `PackageSource`. Persian: بازار (marketplace/gathering) |

### Active — ESP32 Edge/IoT

| # | Priority | Item | Notes |
|---|----------|------|-------|
| E1 | Medium | ESP32 agent scaffold | Rust agent binary via esp-rs/esp-hal. MQTT to daimon. WiFi provisioning. Recipe: `recipes/edge/esp32-agent.toml` |
| E2 | Medium | MQTT bridge in daimon | Accept MQTT heartbeats from MCUs alongside HTTP. Translate to existing edge fleet model |
| E3 | Low | ESP32-CAM integration | Snap images on motion → daimon screen capture API |
| E4 | Low | TinyML on ESP32-S3 | Keyword spotting / gesture recognition via vector extensions. Report inferences to daimon |

### Active — Sandbox & Security

| # | Priority | Item | Notes |
|---|----------|------|-------|
| S1 | Medium | gVisor/Firecracker runtime execution | Config generation + OCI/VM lifecycle done, needs actual process spawning via `tokio::process::Command` |
| S2 | Medium | SGX/SEV hardware validation | Backends implemented, need hardware to test |
| S3 | **High** | **sy-agnos sandbox image (Phase 1)** | Hardened AGNOS OCI image for SY sandbox use. See below |
| S4 | Medium | sy-agnos dm-verity (Phase 2) | Enable dm-verity verified rootfs on sy-agnos image |
| S5 | Low | sy-agnos TPM measured boot (Phase 3) | TPM 2.0 attestation for sy-agnos — requires tpm2-tools on host |

---

## sy-agnos — OS-Level Sandbox for SecureYeoman

**Priority**: High — Cross-project. See [SY ADR 044](https://github.com/MacCracken/secureyeoman/blob/main/docs/adr/044-sy-agnos-sandbox.md).

**Goal**: Build a purpose-built, hardened AGNOS image (`sy-agnos`) that SecureYeoman launches as an execution sandbox. The OS IS the sandbox — immutable rootfs, no shell, baked seccomp, OS-level nftables. Scores 80-88 on SY's sandbox strength scale (between Firecracker 90 and gVisor 70). AGNOS owns the image build; SY owns the driver.

### Phase 1 — sy-agnos Minimal (SY strength 80)

**New recipes** (`recipes/sandbox/`):

- [ ] **`sy-agnos-rootfs.toml`** — Multi-stage image build: edge base → strip (remove /bin/sh, /bin/bash, all package managers, SSH, debug tools, man pages, docs) → install Node.js runtime + SY agent binary → bake seccomp BPF filter → bake nftables default-deny rules → squashfs rootfs
- [ ] **`sy-agnos-init.toml`** — Minimal argonaut init config: 3-process tree only (argonaut → sy-agent → health-check). No TTY, no login prompt, no getty. Agent starts automatically on boot
- [ ] **`sy-agnos-nftables.toml`** — Boot-baked nftables ruleset: default-deny egress, configurable allowlist via `/etc/sy-agnos/network-policy.conf`, DNS restricted to specified resolvers, no listening sockets except health endpoint (port 8099)

**Build infrastructure:**

- [ ] **`scripts/build-sy-agnos.sh`** — Builds OCI image from recipes. Inputs: SY agent binary path, network policy (optional). Outputs: `sy-agnos.tar` OCI image
- [ ] **`/etc/sy-agnos-release`** — JSON metadata: `{ "version": "2026.X.X", "hardening": "minimal", "dmverity": false, "tpm_measured": false, "strength": 80 }`
- [ ] **CI workflow** — `build-sy-agnos.yml`: builds image, publishes to GHCR (`ghcr.io/maccracken/sy-agnos:latest`), signs with cosign
- [ ] **Dockerfile.sy-agnos** — Alternative Docker-based build path for users without the full AGNOS build system

**Reuses existing components:**
- nftables (`recipes/edge/nftables.toml`)
- libseccomp (`recipes/base/libseccomp.toml`)
- glibc, openssl, ca-certificates (base recipes)
- argonaut init (`agent-runtime/src/argonaut.rs`)
- read_only_rootfs (edge profile pattern)

### Phase 2 — dm-verity (SY strength 85)

- [ ] **dm-verity rootfs** — Enable `agnos-sys/src/dmverity.rs` on sy-agnos image build. Hash tree generated at build time, verified at boot
- [ ] **Tamper detection** — If rootfs verification fails, refuse to start agent process (exit code 78 — EX_CONFIG)
- [ ] **Update `/etc/sy-agnos-release`** — `"dmverity": true, "strength": 85`

### Phase 3 — Measured Boot + TPM (SY strength 88)

- [ ] **TPM 2.0 boot measurement** — Extend PCRs at each boot stage using tpm2-tools (already in `recipes/edge/tpm2-tools.toml`)
- [ ] **Attestation endpoint** — `/v1/attestation` returns signed boot measurements (PCR values + event log). SY verifies before dispatching tasks
- [ ] **Update `/etc/sy-agnos-release`** — `"tpm_measured": true, "strength": 88`

### Resolved (2026.3.17)

| Category | Items | Summary |
|----------|-------|---------|
| Module splits (10) | orchestrator, argonaut, agnova, network_tools, ark, service_manager, federation, sigil, edge, safety | ~25,000 lines → focused module directories. sandbox_mod `core.rs` → `sandbox_core.rs` (rustfmt fix). All >2000-line monoliths eliminated |
| GPU awareness (G1–G4) | Scheduling, hoosh routing, edge fleet, consumer apps | `TaskRequirements` GPU fields, `score_gpu()`, `AcceleratorRegistry`, privacy routing, VRAM budgets, auto-quantization, edge VRAM/CC filtering, fleet model registry, `tarang_hw_accel`, `synapse_finetune` GPU hints |
| SY integration (4) | GPU telemetry, local models, Firecracker passthrough, fleet heartbeat | `agnos_gpu_status`, `agnos_local_models` MCP tools, `device_passthrough`, heartbeat GPU metrics + dashboard aggregation |
| Sandbox wiring (S1–S3) | Credential proxy, externalization gate, offender→sigil | `CredentialProxyManager` in agent lifecycle, `ExternalizationGate` in sandbox with 11 patterns, `OffenderTracker` feeds trust demotions to `SigilVerifier` revocation list |
| Agnostic (13) | Crew mgmt, crew GPU, RPC, GPU probe/intents/placement/budget, fleet GPU, event forwarding, 3 HUD widgets | All data APIs + `CrewStatusWidget`, `DomainFilterWidget`, `GpuStatusWidget` in aethersafha `hud/` module. 144 MCP tools |
| Toolchain | Go 1.24.1 → 1.26.1 | Unblocked cliphist, Kitty, modern Go modules |

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
| MCP Tools | — | 144 | Complete (14 agnos + 5 aequi + 24 agnostic + 7 delta + 8 photis + 5 edge + 7 shruti + 9 tarang + 8 jalwa + 9 rasa + 7 mneme + 7 synapse + 7 bullshift + 7 yeoman + 5 phylax + others) |
| Consumer Apps | 6 | 17 | 11 released + 6 scaffolded |
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

## Named Subsystems (19)

| Name | Role | Component |
|------|------|-----------|
| **hoosh** | LLM inference gateway (port 8088, 15 providers) | `llm-gateway/` |
| **daimon** | Agent orchestrator (port 8090, 144 MCP tools) | `agent-runtime/` |
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
| **bazaar** | Community package repository (Persian: marketplace/gathering) | `recipes/base/bazaar.toml` |
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
