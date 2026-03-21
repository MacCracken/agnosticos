# AGNOS — AI-Native General Operating System

> **A**rtificial **G**eneral **N**etwork **O**perating **S**ystem

[![License](https://img.shields.io/badge/license-GPLv3-blue)](LICENSE)
[![Kernel](https://img.shields.io/badge/kernel-Linux%206.6%20LTS-orange)](https://kernel.org)
[![Rust](https://img.shields.io/badge/rust-1.89-red)](https://www.rust-lang.org/)
[![Tests](https://img.shields.io/badge/tests-11000%2B-green)](docs/development/roadmap.md)
[![Coverage](https://img.shields.io/badge/coverage-~84%25-yellowgreen)](docs/development/roadmap.md)
[![Status](https://img.shields.io/badge/status-pre--beta-yellow)](docs/development/roadmap.md)

**AGNOS** is a Linux-based operating system built from the ground up for AI agents and human-AI collaboration. Security-first, Rust-native, self-hosting. AI agents are first-class citizens — sandboxed, auditable, and controllable.

> *AGI doesn't run on infrastructure built for web apps. It runs on infrastructure built for AGI.*
>
> *A system that can't prove its own integrity can't be trusted with autonomous action.*

---

## Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│  Desktop                │  Agent Runtime          │  Kernel      │
│  ┌────────────────────┐ │  ┌────────────────────┐ │  ┌────────┐ │
│  │ aethersafha        │ │  │ daimon (port 8090) │ │  │ Linux  │ │
│  │ Wayland compositor │ │  │ 144 MCP tools      │ │  │ 6.6    │ │
│  │ + screen capture   │ │  │ Agent orchestrator │ │  │ LTS    │ │
│  ├────────────────────┤ │  ├────────────────────┤ │  ├────────┤ │
│  │ agnoshi (agnsh)    │ │  │ hoosh (port 8088)  │ │  │ Land-  │ │
│  │ AI shell           │ │  │ LLM gateway        │ │  │ lock   │ │
│  │ 61+ NL intents     │ │  │ 15 providers       │ │  │ sec-   │ │
│  ├────────────────────┤ │  ├────────────────────┤ │  │ comp   │ │
│  │ 23 consumer apps   │ │  │ aegis + sigil      │ │  │ IMA    │ │
│  │ marketplace (mela) │ │  │ Security + trust   │ │  │ TPM    │ │
│  └────────────────────┘ │  └────────────────────┘ │  └────────┘ │
└──────────────────────────────────────────────────────────────────┘
```

## Named Subsystems (20)

| Name | Role | Port |
|------|------|------|
| **daimon** | Agent orchestrator, 144 MCP tools, federation, scheduling | 8090 |
| **hoosh** | LLM inference gateway, 15 providers, OpenAI-compatible API | 8088 |
| **agnoshi** | AI shell — natural language + bash, 61+ intents | — |
| **aethersafha** | Wayland compositor, screen capture/recording, plugins | — |
| **argonaut** | Init system, service management, edge boot mode | — |
| **ark** + **nous** | Package manager + resolver daemon | — |
| **takumi** | Package build system (`.ark` format) | — |
| **mela** | Agent/app marketplace | — |
| **aegis** | System security daemon | — |
| **sigil** | Trust verification (package + agent signing) | — |
| **agnova** | OS installer (4 install modes) | — |
| **phylax** | Threat detection engine (YARA, entropy, magic bytes) | — |
| **agnosys** | Kernel interface (Landlock, seccomp, LUKS, dm-verity, TPM) | — |
| **agnostik** | Shared types library | — |
| **shakti** | Privilege escalation | — |

## Shared Crates (crates.io)

Standalone Rust crates extracted from AGNOS for the broader ecosystem:

| Crate | Description |
|-------|-------------|
| [**ai-hwaccel**](https://crates.io/crates/ai-hwaccel) | Universal AI hardware accelerator detection — 13 families (CUDA, ROCm, Metal, Vulkan, TPU, Gaudi, Inferentia/Trainium, Qualcomm, Intel oneAPI/NPU, AMD XDNA) |
| [**tarang**](https://crates.io/crates/tarang) | AI-native media framework — 18-33x faster than GStreamer. Audio/video decode, encode, mux, fingerprint |
| [**aethersafta**](https://github.com/MacCracken/aethersafta) | Real-time media compositing engine — scene graph, multi-source capture, HW encoding |
| [**hoosh**](https://github.com/MacCracken/hoosh) | AI inference gateway — 14 LLM providers, token budgets, whisper STT, OpenAI-compatible |

## Consumer Apps (23)

All ship as `.agnos-agent` marketplace bundles via `ark-bundle.sh`:

| App | Description | MCP Tools |
|-----|-------------|-----------|
| **SecureYeoman** | Sovereign AI agent platform (flagship) | 14 yeoman_* |
| **AgnosAI** | Rust-native agent orchestration engine | — |
| **Agnostic** | Multi-domain AI agent platform (Python/CrewAI) | 23 agnostic_* |
| **Irfan** | LLM management and training | 7 irfan_* |
| **Delta** | Code hosting (git, PRs, CI/CD, artifact registry) | 7 delta_* |
| **Aequi** | Self-employed accounting (Tauri v2) | 7 aequi_* |
| **BullShift** | Trading platform | 7 bullshift_* |
| **Jalwa** | AI-native media player (built on tarang) | 8 jalwa_* |
| **Tazama** | AI-native video editor | 7 tazama_* |
| **Shruti** | Digital audio workstation | 7 shruti_* |
| **Rasa** | AI-native image editor | 9 rasa_* |
| **Mneme** | AI-native knowledge base | 7 mneme_* |
| **Tarang** | Media framework CLI | 8 tarang_* |
| **Photis Nadi** | Productivity app | 8 photis_* |
| **Nazar** | System monitor | 5 nazar_* |
| **Vidhana** | System settings (egui GUI) | 5 vidhana_* |
| **Sutra** | Infrastructure orchestrator | 6 sutra_* |
| **Selah** | Screenshot & annotation | 5 selah_* |
| **Abaco** | Calculator & unit converter | 5 abaco_* |
| **Rahd** | Calendar & contacts | 5 rahd_* |

## Recipes

| Category | Count | Examples |
|----------|-------|---------|
| Base system | 113 | GCC 15.2, Rust 1.89, Linux 6.6.72, glibc 2.42 |
| Desktop | 71 | Mesa, PipeWire, Wayland, foot, helix, mpv |
| AI/ML | 25 | CUDA, ONNX, PyTorch |
| Network | 9 | nftables, iproute2, wireless |
| Browser | 8 | Firefox ESR, Chromium |
| Marketplace | 23 | All consumer apps above |
| Edge | 31 | Fleet management, ESP32, MCU agents |
| Bazaar (community) | 90 | Ollama, Docker, Sway, Hyprland, neovim, OBS |
| **Total** | **290+** | |

## Development Status

**Pre-beta.** Phases 0-14 complete. 11,000+ tests, ~84% coverage, 0 warnings.

| Milestone | Status |
|-----------|--------|
| Userland (daimon, hoosh, agnoshi, aethersafha) | Done |
| LFS base system (113 recipes, self-hosting toolchain) | Done |
| Desktop stack (71 recipes, Wayland, PipeWire, GPU) | Done |
| Init system, package manager, installer | Done |
| Security (aegis, sigil, Landlock, seccomp, PQC) | Done |
| Edge OS profile (fleet, 31 recipes, Docker container) | Done |
| Phylax threat detection (YARA, entropy, magic bytes) | Done (core) |
| 23 consumer apps integrated with MCP + agnoshi | Done |
| ark-bundle marketplace packaging (23/23 bundles) | Done |
| Shared crates on crates.io (ai-hwaccel, tarang) | Done |
| **Self-hosting (AGNOS builds AGNOS)** | **Primary beta blocker** |
| Third-party security audit | Not started |
| Community/docs (video tutorials, support portal) | Not started |

**Beta target: Q4 2026** | **v1.0 target: Q2 2027**

See [docs/development/roadmap.md](docs/development/roadmap.md) for full details.

## Quick Start

### Docker (development)

```bash
docker run -it --privileged \
  -p 8088:8088 -p 8090:8090 \
  ghcr.io/maccracken/agnosticos:latest
```

### Build from source

```bash
git clone https://github.com/MacCracken/agnosticos.git
cd agnosticos/userland
cargo build --release --workspace
cargo test --workspace
```

### AI Shell

```bash
agnsh> show me system status
agnsh> create a new agent called "code-assistant"
agnsh> scan /path for threats
agnsh> play ~/Music/song.flac
agnsh> what agents are currently running?
```

## System Requirements

| | Minimum (CLI) | Recommended (Desktop + LLMs) |
|---|---|---|
| **CPU** | x86_64 or aarch64 | 8+ cores |
| **RAM** | 4 GB | 32 GB+ |
| **Storage** | 20 GB SSD | 100 GB NVMe |
| **GPU** | — | NVIDIA/AMD/Intel discrete |
| **TPM** | — | 2.0 (for measured boot) |

## Security

- **Landlock + seccomp-bpf** mandatory sandboxing for all agents
- **Cryptographic audit chain** — immutable, signed logs of all agent actions
- **7 sandbox backends** — Native, gVisor, Firecracker, WASM, SGX, SEV, Noop
- **dm-verity** rootfs integrity verification
- **TPM 2.0** measured boot (PCR 8/9/10)
- **Post-quantum crypto** (PQC module, 68 tests)
- **16 security audit rounds** — 14 CRITICAL + 29 HIGH fixed, 0 remaining

See [SECURITY.md](SECURITY.md) for vulnerability reporting and security model.

## Documentation

| Document | Description |
|----------|-------------|
| [roadmap.md](docs/development/roadmap.md) | Development roadmap, phase breakdown, KPIs |
| [architecture.md](docs/architecture.md) | System architecture |
| [applications/roadmap.md](docs/development/applications/roadmap.md) | App roadmap + shared crates |
| [CONTRIBUTING.md](CONTRIBUTING.md) | Contribution guidelines |
| [SECURITY.md](SECURITY.md) | Security policies |
| [docs/api/explorer.html](docs/api/explorer.html) | Interactive API explorer |

## License

**GNU General Public License v3.0** (GPLv3). See [LICENSE](LICENSE).

Shared crates (ai-hwaccel, tarang, aethersafta, hoosh) are **AGPL-3.0**.

---

<div align="center">

**AGNOS** — The Operating System for the Age of AI

*Built for agents. Controlled by humans.*

</div>
