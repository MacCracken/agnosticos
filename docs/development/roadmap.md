# AGNOS Development Roadmap

> **Status**: Pre-Beta | **Last Updated**: 2026-03-16
> **Userland complete** — 10876+ tests (3622+ agent-runtime, 1554 ai-shell), ~84% coverage, 0 warnings
> **Recipes**: 109 base + 53 desktop + 25 AI + 9 network + 8 browser + 11 marketplace + 4 python + 3 database + 29 edge = 251 total, 0 validation errors
> **Phases 10–14 complete** | **Phase 13**: 13A(infra)/13B/13D/13E done | **Phase 14**: Edge OS Profile done | **Audit**: 16 rounds
> **Audit round 16**: 14 CRITICAL + 29 HIGH fixed, 0 HIGH remaining, 17 MEDIUM backlog cleared

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
| **13D** | **Consumer app integration** — SecureYeoman, Photis Nadi, BullShift, AGNOSTIC, Delta, Aequi, Shruti, Synapse, Tazama, Rasa, Mneme (all with MCP tools + agnoshi intents) |
| **13E** | **CI, WebView, containers, Python** — browser-ark CI, marketplace-publish CI (7 apps), WebView (28 tests), Docker base images, Python runtime (36 tests) |
| **14** | **Edge OS Profile** — Edge boot mode, edge seccomp, fleet management (37 tests), 5 MCP tools, 14 agnoshi tests, Docker container (35.5 MB), 29 edge recipes, SecureYeoman Edge IoT recipe |

---

## Phase 13 — Beta Polish (Active)

### 13A — Self-Hosting Validation (remaining — requires bootable ISO)

| # | Item | Status | Notes |
|---|------|--------|-------|
| 1 | Build AGNOS on AGNOS | Not started | Full bootstrap: compile GCC, Rust, kernel on the built system |
| 2 | Kernel module build on target | Not started | Compile AGNOS kernel modules without host |
| 3 | Userland rebuild on target | Not started | `cargo build` of agent-runtime, llm-gateway, etc. |
| 4 | Package rebuild on target | Not started | `ark-build.sh` works inside AGNOS |

### 13C — Community & Documentation (remaining — requires external setup)

| # | Item | Status | Notes |
|---|------|--------|-------|
| 1 | Video tutorials | Not started | Installation, usage, agent creation (needs recording) |
| 2 | Support portal | Not started | Discord + forum (needs external setup) |
| 3 | Community testing program | Not started | Beta tester enrollment (needs external setup) |
| 4 | Third-party security audit | Not started | External vendor (needs procurement) |

### 13F — Hardware Testing Matrix

| # | Target | Arch | Profile | Status | Notes |
|---|--------|------|---------|--------|-------|
| 1 | QEMU x86_64 | x86_64 | Desktop | Done | Verified 2026-03-13, live boot, all binaries functional |
| 2 | Raspberry Pi 4 | aarch64 | Full | Ready | Binary ready; `build-iso-aarch64.sh` created; dd to microSD |
| 3 | Intel NUC (bare metal) | x86_64 | Desktop | Not started | UEFI boot, GPU driver validation |
| 4 | Older x86_64 (~2014 era) | x86_64 | CLI | Not started | Minimum viable hardware floor test |
| 5 | Older desktop w/ touchscreen (~2014) | x86_64 | Desktop | Not started | Touch input + Wayland validation; tests aethersafha touch events |
| 6 | AWS DeepLens | x86_64 | Edge | Ready | Intel Atom x5-Z8350, 8GB RAM, UVC camera. Kernel config: `edge-deeplens.config`. Build: `edge-image.sh x86_64` |
| 7 | ARM64 SBC (QEMU) | aarch64 | Edge | Not started | QEMU aarch64 virt machine validation |

**aarch64 image builder**: `scripts/build-iso-aarch64.sh` — full AGNOS system (Debian arm64 + AGNOS userland) for RPi4/5 microSD. Uses:
- Debian Trixie arm64 debootstrap (foreign mode with qemu-user-static)
- Cross-compiled userland via Cross.toml (aarch64-unknown-linux-gnu)
- RPi boot partition (config.txt, DTBs, kernel, initrd)
- 2 GB image (256 MB FAT32 boot + ext4 root, expandable after flash)

### 13G — Consumer App Validation

| # | App | MCP Tools | Intents | Release | Bundle Test | Notes |
|---|-----|-----------|---------|---------|-------------|-------|
| 1 | SecureYeoman | 5 yeoman_* | 5 | Yes | Not started | Flagship; ports 18789/3000/3001 |
| 2 | Photis Nadi | 6 photis_* | 5 | Yes | Not started | Flutter runtime |
| 3 | BullShift | 5 bullshift_* | 5 | Yes | Not started | Native binary ~2.8 MB |
| 4 | AGNOSTIC | 5 agnostic_* | 5 | Yes | Not started | Python container ~472 KB |
| 5 | Delta | 5 delta_* | 5 | Yes | Not started | Port 8070 |
| 6 | Aequi | 5 aequi_* | 5 | Yes | Not started | Tauri v2 |
| 7 | Synapse | 5 synapse_* | 5 | Yes (2026.3.14) | Not started | Port 8080; LLM management |
| 8 | Shruti | 7 shruti_* | 7 | Yes (2026.3.14-1) | Not started | DAW; ~8.9 MB linux, 5 platforms |
| 9 | Tazama | 7 tazama_* | 7 | Yes (2026.3.14) | Not started | Video editor; ~5.7 MB linux binary |
| 10 | Rasa | 9 rasa_* | 9 | Yes (2026.3.15) | Not started | Image editor; ~3.2 MB linux, amd64+arm64 |
| 11 | Mneme | 7 mneme_* | 7 | Yes (2026.3.13) | Not started | Knowledge base; ~17 MB linux, amd64+arm64 |

**Bundle test** = `ark-bundle.sh` fetches release, produces `.agnos-agent` tarball, installs via mela.

---

## Phase 15 — Threat Detection & Scanning (Planned)

**Subsystem**: **phylax** (Greek: guardian/watchman) — `agent-runtime/src/phylax.rs`

AGNOS currently excels at **containment** (sandbox-first: Landlock, seccomp, namespaces) but lacks a
**detection** layer. Phase 15 adds native threat scanning that leverages the AI-native architecture —
no ClamAV dependency, no external AV engine. Pure Rust + ML-powered.

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

**Key design decisions:**
- No external AV dependency — pure Rust scanning engine
- AI-native: ML classifier + LLM triage are first-class, not bolted on
- Integrates with existing aegis quarantine and anomaly detection
- Threat definitions distributed as signed ark packages
- Edge-aware: minimal rule subset for constrained devices

---

## Engineering Backlog

### Active

| # | Priority | Item | Notes |
|---|----------|------|-------|
| | | | |

---

## Release Roadmap

### Beta Release — Q4 2026

**Criteria:**
- [x] Phase 10 complete — 108 base system recipes, self-hosting toolchain
- [x] Phase 11 complete — 88 desktop, networking & AI/ML recipes
- [x] Phase 12 complete — Argonaut init, ark package manager, agnova installer
- [x] Phase 13B complete — GPU drivers, WiFi, Bluetooth, Thunderbolt, printing
- [x] Phase 13D complete — All 6 consumer apps integrated
- [x] AGNOS boots from ISO on bare metal (UEFI) and QEMU — verified 2026-03-13 (QEMU live boot, all binaries functional)
- [ ] Self-hosting: can rebuild itself from source
- [ ] Third-party security audit complete
- [ ] Community testing program active

### v1.0 Release — Q2 2027

**Criteria:**
- [ ] Phase 13 complete — Documentation, community
- [ ] All consumer apps published to mela
- [x] Python runtime management
- [x] Enterprise features: SSO, audit logging, mTLS
- [ ] 6 months of beta testing with no critical bugs
- [ ] Commercial support available

---

## Key Performance Indicators (KPIs)

### Current Status (as of 2026-03-11)

| Metric | Target | Current | Status |
|--------|--------|---------|--------|
| Code Coverage | >80% | ~84.3% | Met |
| Test Pass Rate | 100% | 100% | Met |
| Total Tests | 400+ | 10876+ | Met |
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
| Hardware Recipes | 8 | 8 | Complete |
| Consumer Apps | 6 | 11 | Complete (6 original + Shruti + Synapse + Tazama + Rasa + Mneme) |
| MCP Tools | — | 71 | Complete (10 agnos + 5 aequi + 5 agnostic + 5 delta + 6 photis + 5 edge + 5 shruti + 5 tazama + 5 rasa + 5 mneme + 5 synapse + 5 bullshift + 5 yeoman) |
| Recipe Validation Errors | 0 | 0 | Complete |
| Security Audit Rounds | 15 | 16 | Complete |
| Self-Hosting Infra | Yes | Yes | Phase 13A (infra done, actual validation pending) |

### By Component

| Component | Tests | Notes |
|-----------|-------|-------|
| agnos-common | 307 | Secrets, telemetry, LLM types, manifest, rate limits, audit chain |
| agnos-sys | 750+ | 16 modules: audit, mac, netns, dmverity, luks, ima, tpm, secureboot, certpin, bootloader, journald, udev, fuse, pam, update, llm |
| agent-runtime | 3622+ | 36 MCP tools (incl. 5 edge), orchestrator, IPC, sandbox, registry, marketplace, federation, migration, scheduler, PQC, safety, finetune, formal_verify, sandbox_v2, rl_optimizer, cloud, collaboration, sigil, aegis, takumi, argonaut (117), agnova (99), ark (49), edge (37), grpc, service_mesh, oidc, delegation, vector_rest, marketplace_backend, selfhost (38), webview (28), python_runtime (36) |
| llm-gateway | 860 | 15 providers, rate limiting, streaming, cert pinning, hardware acceleration, token budgets |
| ai-shell | 1554 | 50+ intents (5 Aequi, 5 Agnostic, 5 Delta, 5 Photis, 5 Edge, 5 Shruti, 5 Tazama, 5 Rasa, 5 Mneme, 10+ system), approval workflow, dashboard, aliases |
| desktop-environment | 1692 | Wayland protocol, screen capture (31), screen recording (22+), plugin host (31), xwayland (20), shell integration (26), theme bridge (18) |

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

## Named Subsystems

| Name | Role | Component |
|------|------|-----------|
| **hoosh** | LLM inference gateway (port 8088, 15 providers) | `llm-gateway/` |
| **daimon** | Agent orchestrator (port 8090, 36 MCP tools) | `agent-runtime/` |
| **agnosys** | Kernel interface | `agnos-sys/` |
| **agnostik** | Shared types library | `agnos-common/` |
| **shakti** | Privilege escalation | `agnos-sudo/` |
| **agnoshi** | AI shell (`agnsh`, 35+ intents) | `ai-shell/` |
| **aethersafha** | Desktop compositor | `desktop-environment/` |
| **ark** | Unified package manager | `ark.rs`, `/v1/ark/*` |
| **nous** | Package resolver daemon | `nous.rs` |
| **takumi** | Package build system | `takumi.rs` |
| **mela** | Agent marketplace | `marketplace/` module |
| **aegis** | System security daemon | `aegis.rs` |
| **sigil** | Trust verification | `sigil.rs` |
| **argonaut** | Init system | `argonaut.rs` |
| **agnova** | OS installer | `agnova.rs` |
| **phylax** | Threat detection engine (planned) | `phylax.rs` |
| **vansh** | Voice AI shell (planned) | TBD |

---

## Contributing

### Priority Contribution Areas

1. **Self-hosting on-target (Phase 13A)** — Build AGNOS on AGNOS with actual ISO boot
2. **SHA256 verification** — Fill in real checksums for all 248 recipes
3. **Documentation (Phase 13C)** — Video tutorials, support portal
4. **Community testing** — Beta tester enrollment + bug tracker setup
5. **Engineering backlog** — All HIGH items resolved; future items tracked here

### Getting Started

See [CONTRIBUTING.md](/CONTRIBUTING.md) for:
- Development environment setup
- Code style and testing requirements
- Git workflow and commit conventions
- Pull request process

---

## Resources

- **Repository**: https://github.com/agnostos/agnos
- **Documentation**: https://docs.agnos.org (planned)
- **Issue Tracker**: https://github.com/agnostos/agnos/issues
- **Changelog**: [CHANGELOG.md](/CHANGELOG.md)
- **LFS Reference**: https://www.linuxfromscratch.org/lfs/view/stable/
- **BLFS Reference**: https://www.linuxfromscratch.org/blfs/view/stable/

---

*Last Updated: 2026-03-13 | Next Review: 2026-03-20*
