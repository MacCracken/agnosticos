# AGNOS

**AGNOS** (AI-Native General Operating System) is a Linux-based operating system designed from the ground up to serve as infrastructure for artificial general intelligence. Written primarily in Rust with a Linux 6.6 LTS kernel, AGNOS provides a complete software stack — from kernel modules through agent orchestration to desktop environment — where every component is purpose-built, attested, and auditable. The project's thesis is that AGI agents need infrastructure where the orchestration overhead is zero, the security is provable, the audit trail is tamper-proof, and the entire stack is attested from hardware to application.

| | |
|---|---|
| **Developer** | MacCracken |
| **Written in** | Rust, C (kernel modules) |
| **OS family** | Linux |
| **Kernel** | Linux 6.6 LTS |
| **License** | GPL-3.0 |
| **Source model** | Open source |
| **Initial release** | 2026-02-11 (first commit) |
| **First ISO build** | 2026-03-22 |
| **Repository** | `MacCracken/agnosticos` |
| **Status** | Pre-Beta |

---

## Thesis

The infrastructure AGI runs on cannot be the infrastructure built for web applications. Fifty years of software engineering produced a stack of compromises — C memory unsafety, shell-out-to-CLI integration, 100MB runtime daemons, "secure by configuration" defaults, Python for everything, trust-the-container-runtime isolation. Each layer was acceptable in its era. None is acceptable for autonomous AI agents that make consequential decisions.

AGNOS replaces each of these layers with purpose-built, Rust-native alternatives:

| Era | What was accepted | What AGNOS does instead |
|-----|-----------------|------------------------|
| 1970s | C memory unsafety | Rust ownership — entire classes of CVEs eliminated at compile time |
| 1990s | Shell out to CLI tools | Direct API calls — tarang 33x faster than GStreamer pipeline setup |
| 2000s | 100MB runtime daemons | <5MB purpose-built binaries — stiva replaces Docker |
| 2010s | "Secure by configuration" | Secure by construction — kavach has no override flags |
| 2015s | Python for everything | Rust for everything — 227,000x faster fleet messaging than CrewAI |
| 2020s | Trust the container runtime | Attest the container runtime — libro audit chain + TPM measured boot |

An AGI system that cannot prove its own integrity cannot be trusted with autonomous action. AGNOS provides that proof through composable, quantitatively-scored isolation from hardware (TPM) through runtime (stiva) to application (kavach), with every action recorded in a tamper-proof cryptographic audit chain (libro).

---

## History

### Timeline

| Date | Event |
|------|-------|
| **2026-02-11** | Initial commit. Kernel configuration, Phase 1 (Core OS bootable base), Phase 2 (AI Shell with human oversight), and Phase 5 (Production scaffolding) completed on Day 1 |
| **2026-02-16** | Continued Phase 5 development — production hardening and stabilization |
| **2026-02-22** | Core OS updates and refinement |
| **2026-02-26** | First code audit round — tests, fixes, quality gates |
| **2026-03-04** | Coverage expansion begins |
| **2026-03-05** | **Alpha release** (tag `2026.3.5`) — first tagged release, CalVer versioning adopted |
| **2026-03-06** | Phases 6-7 completed. Code audit work begins in earnest. Marketplace module scaffolded |
| **2026-03-07** | Alpha Docker image published (`ghcr.io/maccracken/agnosticos`). CI/CD pipeline established on GitHub Actions. Multiple audit rounds |
| **2026-03-08** | Release workflow automated (auto-publish instead of draft). Ark package recipes begin |
| **2026-03-09** | Browser builds (Firefox ESR, Chromium), CI integration, database recipe integration |
| **2026-03-10** | Full coverage infrastructure. gRPC, service mesh, OIDC modules. Multiple audit cycles |
| **2026-03-11** | Phase 14 (Edge OS Profile) added to roadmap. Continued audit and repair rounds |
| **2026-03-13** | First ISO build work begins — `build-installer.sh` development |
| **2026-03-14** | aarch64 ISO work — RPi4 ARM64 support |
| **2026-03-15** | RPi4 build fixes. Edge fleet management. Version and release patches |
| **2026-03-16** | Self-hosted runner setup begins for Tier 1 builds. Shared crates published to crates.io |
| **2026-03-17** | Audit completion rounds. Release `2026.3.17` |
| **2026-03-18** | Release `2026.3.18` — major milestone. Photis Nadi migrated from Flutter to Rust native. Consumer app packages updated. Sutra released (v2026.3.18) |
| **2026-03-19** | Recipe updates and fixes across marketplace |
| **2026-03-20** | Self-hosted runner repaired. ISO build pipeline work continues |
| **2026-03-21** | Build improvements. stiva, nein, t-ron, impetus scaffolded. Multiple ISO build iterations |
| **2026-03-22** | **First successful ISO build** (early morning, after ~9 days of iteration). Abacus desktop calculator released. 266 commits, 298 recipes, 10,800+ tests, ~84.3% coverage |
| **2026-03-24** | Science stack push: 9 crates reach v1.0 in one session (impetus, hisab, bhava, bodh, sangha, and others). Agnosys integration ready for consumers |
| **2026-03-25** | Massive session: process refinement, SY migration planning, NPO groundwork |
| **2026-03-28** | AgnosAI benchmarks (4/5 wins vs CrewAI, 2000-4500x faster cached). Release `2026.3.29` |
| **2026-03-31** | **First fully clean release** (`2026.3.31`). All 17 artifacts built successfully — x86_64 ISO (desktop + minimal + edge), aarch64 SD card images (desktop + minimal + edge), userland tarballs, multi-arch Docker container. First release with zero build failures across all architectures. 80 shared crates (45 at v1.0+). 3 new science crates scaffolded (mastishk, rasayan, varna). 336 commits, 19 tagged releases |

### Development Pace

AGNOS went from initial commit to first bootable ISO in **39 days** (2026-02-11 to 2026-03-22), and from first ISO to first fully clean multi-architecture release in **48 days** (2026-02-11 to 2026-03-31).

The project accumulated **336 commits** across **19 tagged releases**, achieving 10,800+ passing tests and ~84.3% code coverage. The shared crate ecosystem grew to **80 crates** (45 at v1.0+ stable), with 18+ consumer applications developed in parallel.

The ISO build itself required approximately 9 days of iteration (2026-03-13 to 2026-03-22) to resolve cross-compilation, package dependency ordering, and bootloader integration challenges. The CI pipeline required another 9 days (2026-03-22 to 2026-03-31) to achieve fully automated, zero-failure builds across x86_64 ISOs, aarch64 SD card images, edge profiles, and multi-arch Docker containers.

---

## Architecture

AGNOS is built as a layered system where each component has a specific, named identity and a clear responsibility boundary.

### Core Subsystems

| Subsystem | Name | Language | Role |
|-----------|------|----------|------|
| Kernel interface | **agnosys** | Rust | Syscall bindings, Landlock/seccomp, LUKS, dm-verity, IMA, TPM |
| Shared types | **agnostik** | Rust | Common types, error handling, security primitives, telemetry |
| Agent orchestrator | **daimon** | Rust | Agent lifecycle, IPC, sandbox, registry, HTTP API (port 8090) |
| LLM gateway | **hoosh** | Rust | 15 LLM providers, OpenAI-compatible API (port 8088), token budgets |
| AI shell | **agnoshi** | Rust | Natural-language terminal, intent parsing, command translation |
| Desktop compositor | **aethersafha** | Rust | Wayland compositor, accessibility, plugin host, XWayland |
| Package manager | **ark** | Rust | Unified package management, signed tarballs |
| Package resolver | **nous** | Rust | Dependency resolution daemon |
| Build system | **takumi** | Rust | TOML recipe-based package builds |
| Init system | **argonaut** | Rust | Service management, boot sequencing, Edge boot mode |
| Installer | **agnova** | Rust | OS installation wizard |
| Security daemon | **aegis** | Rust | System hardening, security policy enforcement |
| Trust system | **sigil** | Rust | Cryptographic trust verification |
| Marketplace | **mela** | Rust | Agent and app marketplace |
| Privilege escalation | **shakti** | Rust | Controlled privilege elevation |
| Threat detection | **phylax** | Rust | YARA rules, ML binary analysis, fanotify scanning |

### Shared Crates (crates.io)

AGNOS extracts reusable infrastructure into standalone crates published on crates.io. These are consumed by both the OS and the consumer application ecosystem:

| Crate | Purpose |
|-------|---------|
| **ai-hwaccel** | Universal AI hardware accelerator detection (13 families) |
| **tarang** | AI-native media framework (18-33x faster than GStreamer) |
| **aethersafta** | Real-time media compositing and scene graph |
| **ranga** | Core image processing (color spaces, blend modes, GPU compute) |
| **dhvani** | Core audio engine (DSP, mixing, synthesis, PipeWire) |
| **hoosh** | LLM inference client (15 providers, token budgets) |
| **majra** | Distributed queue and multiplex engine |
| **kavach** | Sandbox execution framework (8 backends, quantitative scoring) |
| **libro** | Cryptographic audit chain (SHA-256 hash-linked logging) |
| **bote** | MCP core service (JSON-RPC 2.0, tool registry) |
| **szal** | Workflow engine (branching, retry, rollback) |
| **abaco** | Math library (expression parsing, unit conversion) |

Scaffolded (pre-release): **murti** (model runtime), **stiva** (container runtime), **nein** (nftables firewall), **impetus** (physics engine), **soorat** (rendering engine). Published: **hisab** (higher math), **bhava** (emotion/personality), **yukti** (device abstraction), **phylax** (threat detection), **prakash** (optics/light), **t-ron** (MCP security).

### Security Model

AGNOS implements defense-in-depth with quantitative scoring:

- **Sandbox apply order**: encrypted storage, MAC, Landlock, seccomp, network isolation, audit
- **Kavach**: 8 sandbox backends under one API with composable strength scoring (0-100)
- **Libro**: Tamper-proof SHA-256 hash-linked audit chain for every agent action
- **Stiva** (planned): Daemonless container runtime with no privilege override flags
- **Composable isolation**: Firecracker + jailer + stiva + sy-agnos + TPM = score 98/100

### MCP Tools

AGNOS provides 151+ built-in MCP (Model Context Protocol) tools enabling AI agents to interact with every subsystem. Consumer applications register additional tools via bote.

---

## Distribution

### Build Artifacts

| Artifact | Architecture | Use Case |
|----------|-------------|----------|
| ISO | x86_64 | Desktop/server installation |
| SD card image | aarch64 | Raspberry Pi / ARM edge devices |
| Edge image | x86_64, aarch64 | dm-verity hardened LFS edge nodes |
| Docker image | x86_64 | `ghcr.io/maccracken/agnosticos` — CI base, development |

### Packaging

- **System packages**: `.ark` format (signed tarballs + metadata), built via takumi recipes
- **Marketplace apps**: `.agnos-agent` format (manifest.json + sandbox.json + binaries)
- **Base system**: ~174 packages built from source in dependency order
- **Recipe count**: 298 total (113 base + 71 desktop + 25 AI + 9 network + 8 browser + 34 marketplace + 4 Python + 3 database + 31 edge) plus 90 in community bazaar

### CI/CD

Two-tier build architecture:
- **Tier 1** (rare): Self-hosted runner builds toolchain + base rootfs from source
- **Tier 2** (every release): GitHub Actions pulls cached base rootfs, overlays userland, creates ISO

---

## Consumer Applications

AGNOS ships with an ecosystem of 18+ first-party applications, all Rust-native, all integrating with daimon (agent orchestration) and hoosh (LLM inference):

| Application | Domain | Description |
|-------------|--------|-------------|
| **SecureYeoman** | AI platform | TypeScript/Bun AI agent platform (flagship) |
| **Agnostic** | AI automation | Python/CrewAI agent automation, 7 domain presets |
| **Jalwa** | Media | AI-native media player, 110+ tests |
| **Shruti** | Audio | Digital audio workstation |
| **Tazama** | Video | AI-native video editor |
| **Rasa** | Image | AI-native image editor |
| **Mneme** | Knowledge | AI-native knowledge base |
| **Sutra** | Infrastructure | Infrastructure orchestrator (Ansible replacement) |
| **Tarang** | Media framework | Pure Rust media pipeline (ffmpeg replacement) |
| **Delta** | Development | Code hosting platform (git, PRs, CI/CD) |
| **Aequi** | Finance | Self-employed accounting platform |
| **BullShift** | Trading | Trading platform |
| **Photis Nadi** | Productivity | Productivity application |
| **Nazar** | Monitoring | AI-native system monitor |
| **Selah** | Screenshot | Screenshot and annotation tool |
| **Rahd** | Calendar | AI-native calendar and contacts |
| **Abacus** | Calculator | Desktop calculator (built on abaco crate) |
| **Synapse** | LLM management | LLM management and training |

Each application follows the [First-Party Standards](development/applications/first-party-standards.md) including MCP tool registration, agnoshi intent patterns, marketplace recipes, and daimon integration.

---

## Named Subsystem Conventions

All AGNOS subsystems use multilingual names drawn from Arabic, Persian, Sanskrit, Greek, Latin, Japanese, Hebrew, Romanian, German, and other languages. This reflects the project's identity as an *agnostic* operating system — not tied to any single language, culture, or vendor.

Examples: **hoosh** (Persian: intelligence), **daimon** (Greek: guiding spirit), **tarang** (Sanskrit: wave), **takumi** (Japanese: master craftsman), **kavach** (Hindi: armor), **libro** (Italian: book), **stiva** (Romanian: stack), **nein** (German: no), **hisab** (Sanskrit: mathematics), **kiran** (Sanskrit: ray of light).

---

## Technical Statistics (as of 2026-03-22)

| Metric | Value |
|--------|-------|
| Total commits | 266 |
| Tagged releases | 17 |
| Tests passing | 10,800+ |
| Code coverage | ~84.3% (tarpaulin) |
| Compiler warnings | 0 |
| Shared crates on crates.io | 13 |
| MCP tools | 151+ built-in |
| Marketplace recipes | 298 (+90 community) |
| Consumer applications | 18+ |
| Kernel | Linux 6.6 LTS |
| Rust MSRV | 1.89 |

---

## See Also

- [Application Development Roadmap](development/applications/roadmap.md) — planned first-party applications
- [First-Party Application Standards](development/applications/first-party-standards.md) — conventions for consumer apps
- [Shared Crates Reference](development/applications/shared-crates.md) — ecosystem crate registry
- [CI/CD Architecture](development/ci-cd-guide.md) — build and release pipeline
- [Monolith Extraction Plan](development/monolith-extraction.md) — path from monolithic userland to independent services
- [Network Evolution](development/network-evolution.md) — TCP/HTTP → QUIC → binary agent protocol
- [Performance Benchmarks](development/performance-benchmarks.md) — comparison data

---

*Last Updated: 2026-03-22*
