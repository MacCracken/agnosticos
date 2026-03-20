# AGNOS Development Roadmap

> **Status**: Pre-Beta | **Last Updated**: 2026-03-18
> **Userland complete** — 11000+ tests (3900+ agent-runtime, 1554 ai-shell), ~84% coverage, 0 warnings
> **Recipes**: 113 base + 71 desktop + 25 AI + 9 network + 8 browser + 22 marketplace + 4 python + 3 database + 31 edge = 286 OS (+ 90 bazaar community)
> **Build order**: 178 packages in `recipes/build-order.txt` (base + desktop, dependency-ordered)
> **Phases 10–14 complete** | **Phase 15A**: Core scanning done (phylax) | **Audit**: 16 rounds
> **MCP Tools**: 144 built-in + external registration
> **Consumer Projects**: 19 released (including Vidhana v1, Sutra v1)
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

**Strategy**: Package existing open-source tools via takumi recipes to provide a complete desktop experience *now*. AI-native replacements come later (see `docs/development/applications/roadmap.md`).

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
| 1 | System Settings UI | **Vidhana** | **Done** | AI-native, 6 crates, 76+ tests, 5 MCP tools, NL control, egui GUI, port 8099. `/home/macro/Repos/vidhana` |
| 2 | Network Manager GUI | nm-applet | **Bazaar** | Recipe in bazaar community repo (`ark bazaar install network-manager-applet`) |
| 3 | Bluetooth Manager | blueman | **Bazaar** | Recipe in bazaar (`ark bazaar install blueman`). BlueZ daemon in OS |
| 4 | Display Settings | Vidhana display panel | **Done** | Brightness, theme, scaling, night light, refresh rate — integrated in Vidhana |
| 5 | Sound Settings | Vidhana audio panel | **Done** | Volume, mute, device selection — integrated in Vidhana. pavucontrol available via bazaar |
| 6 | Firewall GUI | firewall-config | **Bazaar** | Recipe in bazaar (`ark bazaar install firewall-config`). nftables daemon in OS |

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
| 8 | ESP32-S3 (MCU) | xtensa | Edge/IoT | Recipe done | MQTT agent, sensor telemetry, TinyML. Recipe: `recipes/edge/esp32-agent.toml`. Needs source repo + hardware flash test |
| 9 | ESP32-C3 (MCU) | riscv32 | Edge/IoT | Recipe done | RISC-V core, lowest power, WiFi + Thread/Zigbee. Same recipe, secondary target |

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
| 7 | Synapse | 7 synapse_* | 7 | Yes | Not started | LLM management |
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

### Agnostic Integration — Complete

*All 13 items resolved. Data APIs + aethersafha HUD widgets all implemented.*

---

## Engineering Backlog

### Active — Build & Distribution

| # | Priority | Item | Notes |
|---|----------|------|-------|
| B1 | High | Selfhost pipeline builds all 176 packages | `selfhost-build.yml` updated, needs first full run |
| B2 | High | RPi4 hardware boot test | Firmware blobs added, needs physical validation |
| B3 | **Done** | SHA256 checksums — all 264 filled | 100%. intel-ucode `20260227`, amd-ucode `20260309`, gVisor `latest` — all verified from upstream |
### Active — ESP32 Edge/IoT

| # | Priority | Item | Notes |
|---|----------|------|-------|
| E1 | Medium | ESP32 agent scaffold | **Recipe created** (`recipes/edge/esp32-agent.toml`). Dual-target: ESP32-S3 (xtensa) + ESP32-C3 (riscv32). no_std esp-hal, MQTT to daimon, WiFi provisioning (SoftAP/SmartConfig), sensor collection, deep sleep, OTA, flash helper script. MQTT bridge done (E2). Pending: source repo (`MacCracken/esp32-agent`) |
| E2 | Medium | MQTT bridge in daimon | **DONE**. `agent-runtime/src/edge/mqtt_bridge.rs` — rumqttc subscriber on `agnos/+/{heartbeat,telemetry,status}`, auto-registers MCU nodes into fleet, translates ESP32 heartbeats/OTA/sleep lifecycle to EdgeNode model, WiFi RSSI → network_quality, 14 tests |
| E3 | **Done** | ESP32-CAM integration | **DONE**. Recipe `[camera]` config section (resolution, JPEG quality, motion sensitivity, PIR GPIO, cooldown). MQTT bridge subscribes to `agnos/+/camera/{frame,motion}`, stores `CameraCaptureEvent` in ring buffer (200 cap), tags fleet nodes `camera`/`motion_detect`. Payload types: `McuCameraFrame` (base64 JPEG, trigger, dimensions), `McuMotionEvent` (intensity, source, optional snapshot). Oversized frames >1MB rejected. 13 tests. Pending: firmware-side camera driver in esp32-agent source repo |
| E4 | Low | TinyML on ESP32-S3 | **Daimon side done**. MQTT bridge handles `agnos/+/inference/{result,status}` topics. `McuInferenceResult` (model_name, label, confidence, latency_ms, input_type) + `McuInferenceStatus` (model_loaded, memory_used_bytes, inference_count). Fleet nodes auto-tagged `tinyml` + `tinyml:{model_name}`. ESP32-S3 recipe has `[tinyml]` config section (model_path, model_type: kws/gesture/anomaly, SIMD acceleration, confidence threshold). 10 tests. Pending: firmware-side TFLite Micro integration in esp32-agent source repo |

### Active — Sandbox & Security

| # | Priority | Item | Notes |
|---|----------|------|-------|
| S1 | **Done** | gVisor/Firecracker runtime execution | `run_task()` async methods with `tokio::process::Command`, timeout enforcement, kill-on-timeout, full BackendResult |
| S2 | Medium | SGX/SEV hardware validation | Backends implemented, need hardware to test |
| S3 | **High** | **sy-agnos sandbox image (Phase 1)** | **Done** — 3 recipes, build script, Dockerfile created |
| S4 | **Done** | sy-agnos dm-verity (Phase 2) | **Done** — veritysetup format in build-sy-agnos.sh, hash tree in OCI image, boot verification, strength 85, graceful skip if no veritysetup |
| S5 | **Done** | sy-agnos TPM measured boot (Phase 3) | **Done** — tpm2_pcrextend in boot script (PCR 8/9/10), `/v1/attestation` endpoint, event log, strength 88, graceful skip if no tpm2-tools |

### Active — Sutra Integration

Sutra (infrastructure orchestrator) needs daimon to expose a remote execution API so playbooks can orchestrate fleet nodes via `transport = "daimon"`.

| # | Priority | Item | Notes |
|---|----------|------|-------|
| T1 | **Done** | Daimon remote exec API | `POST /v1/agents/{id}/exec` — execute a shell command on a fleet node via its daimon agent. Request: `{ "command": "...", "timeout_secs": 30 }`. Response: `{ "exit_code": 0, "stdout": "...", "stderr": "...", "duration_ms": 42 }`. Shell metacharacter injection prevention, timeout enforcement (default 30s, max 300s), full audit trail. 10 tests |
| T2 | **Done** | Daimon file transfer API | `PUT /v1/agents/{id}/files/*path` — write file to agent data dir. `GET /v1/agents/{id}/files/*path` — read file from agent data dir. Scoped to `/var/lib/agnos/agents/{id}/`, strict path traversal protection (no `..`, no absolute, no symlinks), 10 MB size limit, audit logging. 13 tests |
| T3 | **Done** | Daimon playbook audit ingestion | `POST /v1/audit/runs` — accepts sutra RunRecord JSON (run_id, playbook, tasks, success). Validates structure, appends to audit buffer + cryptographic chain. 5 tests |
| T4 | **Done** | Hoosh playbook generation tuning | `x-sutra-playbook: true` header on `/v1/chat/completions` injects playbook-aware system prompt with 3 few-shot TOML examples (deploy, harden, setup). Module reference included |
| T5 | **Done** | sutra-community marketplace recipe | `recipes/marketplace/sutra-community.toml` — installs community modules (nftables, sysctl, aegis, daimon, edge) as ark package. Source: `MacCracken/sutra-community` |

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

## sy-agnos — OS-Level Sandbox for SecureYeoman

**Priority**: High — Cross-project. See [SY ADR 044](https://github.com/MacCracken/secureyeoman/blob/main/docs/adr/044-sy-agnos-sandbox.md).

**Goal**: Build a purpose-built, hardened AGNOS image (`sy-agnos`) that SecureYeoman launches as an execution sandbox. The OS IS the sandbox — immutable rootfs, no shell, baked seccomp, OS-level nftables. Scores 80-88 on SY's sandbox strength scale (between Firecracker 90 and gVisor 70). AGNOS owns the image build; SY owns the driver.

### Phase 1 — sy-agnos Minimal (SY strength 80)

**New recipes** (`recipes/sandbox/`):

- [x] **`sy-agnos-rootfs.toml`** — Multi-stage image build: edge base → strip (remove /bin/sh, /bin/bash, all package managers, SSH, debug tools, man pages, docs) → install Node.js runtime + SY agent binary → bake seccomp BPF filter → bake nftables default-deny rules → squashfs rootfs
- [x] **`sy-agnos-init.toml`** — Minimal argonaut init config: 3-process tree only (argonaut → sy-agent → health-check). No TTY, no login prompt, no getty. Agent starts automatically on boot
- [x] **`sy-agnos-nftables.toml`** — Boot-baked nftables ruleset: default-deny egress, configurable allowlist via `/etc/sy-agnos/network-policy.conf`, DNS restricted to specified resolvers, no listening sockets except health endpoint (port 8099)

**Build infrastructure:**

- [x] **`scripts/build-sy-agnos.sh`** — Builds OCI image from recipes. Inputs: SY agent binary path, network policy (optional). Outputs: `sy-agnos.tar` OCI image
- [x] **`/etc/sy-agnos-release`** — JSON metadata: `{ "version": "2026.X.X", "hardening": "minimal", "dmverity": false, "tpm_measured": false, "strength": 80 }` (baked by build script)
- [x] **CI workflow** — `build-sy-agnos.yml`: builds image, publishes to GHCR (`ghcr.io/maccracken/sy-agnos:latest`), signs with cosign (Dockerfile provides the CI build path)
- [x] **Dockerfile.sy-agnos** — Alternative Docker-based build path for users without the full AGNOS build system

**Reuses existing components:**
- nftables (`recipes/edge/nftables.toml`)
- libseccomp (`recipes/base/libseccomp.toml`)
- glibc, openssl, ca-certificates (base recipes)
- argonaut init (`agent-runtime/src/argonaut.rs`)
- read_only_rootfs (edge profile pattern)

### Phase 2 — dm-verity (SY strength 85) — DONE

- [x] **dm-verity rootfs** — `build-sy-agnos.sh` runs `veritysetup format` after squashfs creation, generates hash tree, saves root hash. Hash tree included as OCI layer. Graceful skip if `veritysetup` not installed
- [x] **Tamper detection** — Init script verifies rootfs via `veritysetup verify` at boot. Refuses to start agent (exit 78 EX_CONFIG) on verification failure. Standalone `verify-rootfs.sh` script baked into rootfs
- [x] **Update `/etc/sy-agnos-release`** — `"dmverity": true, "strength": 85, "hardening": "verified"` when verity is enabled. Features list includes `"dm-verity"`. OCI labels updated

### Phase 3 — Measured Boot + TPM (SY strength 88) — DONE

- [x] **TPM 2.0 boot measurement** — Boot script (`/usr/lib/agnos/tpm-measure-boot.sh`) extends PCR 8 (kernel hash), PCR 9 (rootfs hash), PCR 10 (agent binary hash) via `tpm2_pcrextend`. Event log written to `/var/log/agnos/tpm-event-log.json`. Graceful skip if no TPM device or tpm2-tools
- [x] **Attestation endpoint** — `GET /v1/attestation` returns PCR values (via `tpm2_pcrread`), boot event log, sy-agnos-release metadata, and HMAC-SHA256 signature over measurements (keyed by machine-id). Returns `{"tpm_available": false}` when TPM absent. Handler: `agent-runtime/src/http_api/handlers/attestation.rs` (12 tests)
- [x] **Update `/etc/sy-agnos-release`** — `"tpm_measured": true, "strength": 88, "hardening": "measured"` when tpm2-tools available. Features list includes `"tpm-measured-boot"`. OCI labels include `com.secureyeoman.sandbox.tpm_measured`

### Resolved (2026.3.20)

| Category | Items | Summary |
|----------|-------|---------|
| Shared crates | 4 new crates extracted | `ai-hwaccel` (hardware detection, crates.io), `tarang` (media framework, crates.io), `aethersafta` (compositing engine, scaffolded), `hoosh` (inference gateway, scaffolded) |
| ai-hwaccel integration | hoosh + daimon wired | `acceleration.rs` replaced with ai-hwaccel re-exports (549 tests). `scheduler.rs` `gpu: bool` → `AcceleratorRequirement` + TPU/Gaudi support. `finetune.rs` TPU/Gaudi memory estimation via ai-hwaccel |
| ark-bundle fixes | 23/23 bundles passing | Fixed 14 broken asset patterns, added raw binary handling (SY), source-only skip. All marketplace recipes validated against GitHub releases |
| Recipe updates | 10 recipes created/updated | `agnosai.toml` (new), `ai-hwaccel.toml` (new), `aequi.toml` (org fix), `jalwa` → 2026.3.19, `synapse` → 2026.3.19, `shruti` → 2026.3.19, `tazama` → 2026.3.19 (GStreamer dropped), `tarang` (crates.io + binary), `sutra-community` (runtime fix), `aethersafta` → 0.20.4 |
| Roadmap | Phase 16F + streaming app | Aethersafha media ingestion (10 items), live streaming/broadcast studio (Priority 3), shared crates section in app roadmap |
| Release CI | tarang + ai-hwaccel pipelines | Multi-arch binary packaging (amd64 + arm64) + crates.io publish + GitHub release with SHA256 |

### Pending (2026.3.20)

| # | Item | Status | Notes |
|---|------|--------|-------|
| 1 | Bump ai-hwaccel to 0.20.3 in hoosh + daimon | Pending | Currently pinned to 0.19.x; update after ai-hwaccel 0.20.3 release |
| 2 | Workspace compile + full test run | Pending | After ai-hwaccel dep bump lands |

### Resolved (2026.3.18)

| Category | Items | Summary |
|----------|-------|---------|
| Documentation | First-party standards, app docs, roadmap split | `docs/development/applications/first-party-standards.md`, 18 individual app docs in `docs/applications/`, third-party docs, app development roadmap. `os_long_term.md` deleted — content migrated |
| Sutra | Infrastructure orchestrator v1 | 5 crates, 70 tests, 6 MCP tools (with handlers), 6 core modules (ark, argonaut, file, shell, user, verify), SSH transport, Tera templating, parallel execution (-j), JSON output, variables/facts, error recovery (on_error), task dependencies (depends_on), sutra-community repo (5 modules: nftables, sysctl, aegis, daimon, edge). Named subsystem #20 |
| CI/CD fixes | build-iso.yml permissions, python_runtime race | `sudo chown` after all 6 build jobs. Test no longer uses process-global env vars |
| Recipe updates | 4 consumer projects | PhotisNadi `2026.3.18`, Aequi `2026.3.18`, Synapse `2026.3.18-2`, Vidhana v1 `2026.3.18` |
| Synapse integration | Bridge paths + tests + delete method | All 7 bridge paths corrected to Synapse 2026.3.18-2 API. `HttpBridge::delete()` added. 21 handler tests. Chat uses OpenAI-compat `/v1/chat/completions`. Finetune uses `/training/jobs`. R1-R7 closed |
| SHA256 checksums (B3) | 20 recipes filled | 261/264 (98.9%). 3 remaining need upstream version bumps |
| Developer tooling | Claude Code hooks | PostToolUse hook: auto `cargo fmt` + `cargo clippy` on userland Write/Edit |
| Debian removal (B4) | build-installer.sh + build-sdcard.sh | debootstrap fully removed. Scripts require AGNOS base rootfs via `--base-rootfs`, cache, or GitHub release auto-download |
| ESP32 scaffold (E1) | `recipes/edge/esp32-agent.toml` | Dual-target (S3 xtensa + C3 riscv32), esp-rs/esp-hal no_std, MQTT, WiFi provisioning, flash helper, reference config |
| sy-agnos Phase 1 (S3) | 3 recipes + build script + Dockerfile | `recipes/sandbox/sy-agnos-{rootfs,init,nftables}.toml`, `scripts/build-sy-agnos.sh`, `docker/Dockerfile.sy-agnos`. SY strength 80 |
| sy-agnos Phase 2 (S4) | dm-verity in build-sy-agnos.sh | `veritysetup format` after squashfs, hash tree in OCI image, boot verification (exit 78 on failure), `verify-rootfs.sh` script, strength 85, graceful skip |
| gVisor/Firecracker exec (S1) | `run_task()` on both backends | `tokio::process::Command` spawning, timeout + kill, OCI bundle lifecycle (gVisor), config-file startup (Firecracker), 47 tests passing |
| SHA256 complete (B3) | All 264 recipes | intel-ucode `20250311`→`20260227`, amd-ucode `20250311`→`20260309` (CDN URL fix), gVisor `20250310.0`→`latest`. 100% coverage |
| sy-agnos Phase 3 (S5) | TPM measured boot + attestation | Boot measurement script (PCR 8/9/10 via tpm2_pcrextend), `/v1/attestation` endpoint with HMAC signature, event log, strength 88, graceful skip |

### Resolved (2026.3.17)

10 module splits (~25K lines), GPU awareness (G1-G4), SY integration (4), sandbox wiring (S1-S3), Agnostic integration (13 items), Go 1.24→1.26. See CHANGELOG `[2026.3.17]` for details.

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
- [ ] AI-native desktop replacements for Priority 1 items (see `docs/development/applications/roadmap.md`)
- [x] Python runtime management
- [x] Enterprise features: SSO, audit logging, mTLS
- [ ] 6 months of beta testing with no critical bugs
- [ ] Commercial support available

---

## Key Performance Indicators (KPIs)

### Current Status (as of 2026-03-18)

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
| Base System Recipes | ~108 | 113 | Complete |
| Desktop Recipes | ~62 | 71 | Complete (lean OS, optional in bazaar) |
| Edge Recipes | ~30 | 31 | Complete |
| Marketplace Recipes | 11 | 22 | Complete (18 released) |
| Bazaar Community | — | 90 | Seed recipes across 8 categories |
| MCP Tools | — | 144 | Complete (14 agnos + 5 aequi + 24 agnostic + 7 delta + 8 photis + 5 edge + 7 shruti + 9 tarang + 8 jalwa + 9 rasa + 7 mneme + 7 synapse + 7 bullshift + 7 yeoman + 5 phylax + others) |
| Consumer Apps | 6 | 19 | 19 released (incl. Vidhana v1, Sutra scaffolded) |
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

## Named Subsystems (20)

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
| **sutra** | Infrastructure orchestrator (Sanskrit: thread/rule/formula) | `MacCracken/sutra` |
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
- **Long-term app roadmap**: [docs/development/applications/roadmap.md](/docs/development/applications/roadmap.md)
- **LFS Reference**: https://www.linuxfromscratch.org/lfs/view/stable/
- **BLFS Reference**: https://www.linuxfromscratch.org/blfs/view/stable/

---

*Last Updated: 2026-03-18 | Next Review: 2026-03-24*
