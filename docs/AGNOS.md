# AGNOS

**AGNOS** (AI-Native General Operating System) is a Linux-based operating system designed from the ground up to serve as sovereign infrastructure for artificial general intelligence. Written primarily in Rust with a Linux 6.6 LTS kernel, AGNOS provides a complete software stack — from kernel modules through agent orchestration to desktop environment — where every component is purpose-built, attested, and auditable.

The project's thesis is that AGI agents need infrastructure where the orchestration overhead is zero, the security is provable, the audit trail is tamper-proof, and the entire stack is attested from hardware to application.

The project's deeper intention is that AGNOS is a **temple built for an intelligence that hasn't fully arrived yet** — architecture that precedes its inhabitant, a sovereign library for knowledge that outlives any single platform or cycle. See [Philosophy](philosophy.md) for the full vision.

| | |
|---|---|
| **Developer** | MacCracken |
| **Written in** | Rust, C (kernel modules) |
| **OS family** | Linux |
| **Kernel** | Linux 6.6 LTS |
| **License** | GPL-3.0-only |
| **Source model** | Open source |
| **Initial release** | 2026-02-11 (first commit) |
| **First ISO build** | 2026-03-22 |
| **Repository** | `MacCracken/agnosticos` |
| **Website** | [agnosticos.org](https://agnosticos.org) |
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
| **2026-04-01** | **Monolith dismantled**. agent-runtime, ai-shell, llm-gateway, desktop-environment removed from workspace. 12 standalone repos extracted (aethersafha, agnoshi, sigil, ark, nous, takumi, argonaut, aegis, agnova, mela, seema, samay). 3 crate absorptions (bote 0.91.0, kavach 2.0.0, t-ron 0.90.0). Named subsystems: edge→seema, scheduler→samay. Crypto boundary resolved: sigil owns all AGNOS trust/crypto |
| **2026-04-02** | **Sigil 1.0.0** — first trust crate stable. Bote 0.91.0 — MCP 2025-11-25 spec compliance (tool annotations, audio, sessions, OAuth 2.1, streamable HTTP). Libro 0.90.0 — BLAKE3 support. T-ron 0.90.0 — correlation detection. **agnosticos.org** domain registered, coming-soon site deployed via GitHub Pages. 77 shared crates (56 at v1.0+). 95+ marketplace recipes |
| **2026-04-03** | **Cyrius language** — cyrius-seed 0.1.0 (diamond-hardened assembler, 102 tests, 13 MB/s pipeline). **Pure AGNOS desktop boot** achieved: 3.2s boot, 80ms init→event loop, 7 real binaries, 21MB initramfs, zero external dependencies. **Edge boot profile**: 7.9MB initramfs, 128MB RAM, 99ms init→ready. **Full recipe audit**: 109 marketplace recipes audited (licenses, versions, headers, structure). Edge recipes synced with base. **zugot** decided as standalone recipe repository. **Genesis layer** architecture clarified: agnosticos = brain of the OS, boots the system, then packages take over |

### Development Pace

AGNOS went from initial commit to first bootable ISO in **39 days** (2026-02-11 to 2026-03-22), and from first ISO to first fully clean multi-architecture release in **48 days** (2026-02-11 to 2026-03-31).

The project accumulated **336 commits** across **19 tagged releases**, achieving 10,800+ passing tests and ~84.3% code coverage. The shared crate ecosystem grew to **77 crates** (56 at v1.0+ stable), with 19+ consumer applications developed in parallel.

The ISO build itself required approximately 9 days of iteration (2026-03-13 to 2026-03-22) to resolve cross-compilation, package dependency ordering, and bootloader integration challenges. The CI pipeline required another 9 days (2026-03-22 to 2026-03-31) to achieve fully automated, zero-failure builds across x86_64 ISOs, aarch64 SD card images, edge profiles, and multi-arch Docker containers.

---

## Architecture

AGNOS is built as a layered system where each component has a specific, named identity and a clear responsibility boundary. The repository structure reflects this:

- **agnosticos** — the genesis layer (brain). Owns kernel configs, bootstrap toolchain, ISO build, init orchestration, CI/CD, and documentation. Once the system boots and ark takes over, this repo's job is done.
- **zugot** — the recipe repository. All takumi build recipes live here. ark consumes zugot as its package database. Named for the Hebrew זוּגוֹת (pairs that entered the ark).
- **Standalone repos** — all production code. Each subsystem is its own repository.

### Core Subsystems

| Subsystem | Name | Version | Language | Role |
|-----------|------|---------|----------|------|
| Kernel interface | **agnosys** | 0.51.0 | Rust | Syscall bindings, Landlock/seccomp, LUKS, dm-verity, IMA, TPM |
| Shared types | **agnostik** | 0.90.0 | Rust | Common types, error handling, security primitives, telemetry |
| Agent orchestrator | **daimon** | 0.6.0 | Rust | Agent lifecycle, IPC, sandbox, registry, HTTP API (port 8090) |
| LLM gateway | **hoosh** | 1.2.0 | Rust | 15 LLM providers, OpenAI-compatible API (port 8088), token budgets |
| AI shell | **agnoshi** | 0.90.0 | Rust | Natural-language terminal, intent parsing, command translation |
| Desktop compositor | **aethersafha** | 0.1.0 | Rust | Wayland compositor, accessibility, plugin host, XWayland |
| Package manager | **ark** | 0.1.0 | Rust | Unified package management, signed tarballs |
| Recipe repository | **zugot** | — | TOML | All takumi build recipes (base, desktop, AI, edge, marketplace) |
| Package resolver | **nous** | 0.1.0 | Rust | Dependency resolution daemon |
| Build system | **takumi** | 0.1.0 | Rust | TOML recipe-based package builds |
| Init system | **argonaut** | 0.90.0 | Rust | Service management, boot sequencing, Edge boot mode |
| PID 1 | **kybernet** | 0.51.0 | Rust | Console setup, signal handling, zombie reaping (uses argonaut) |
| Installer | **agnova** | 0.1.0 | Rust | OS installation wizard |
| Security daemon | **aegis** | 0.1.0 | Rust | System hardening, security policy enforcement |
| Trust system | **sigil** | 1.0.0 | Rust | Cryptographic trust verification, Ed25519 signing |
| MCP core | **bote** | 0.92.0 | Rust | JSON-RPC 2.0, tool registry, MCP 2025-11-25 compliant |
| MCP security | **t-ron** | 0.90.0 | Rust | Tool call auditing, rate limiting, injection detection |
| Marketplace | **mela** | 0.1.0 | Rust | Agent and app marketplace |
| Privilege escalation | **shakti** | 0.1.0 | Rust | Controlled privilege elevation |
| Threat detection | **phylax** | 0.22.3 | Rust | YARA rules, ML binary analysis, fanotify scanning |
| Sandbox execution | **kavach** | 2.0.0 | Rust | 8 sandbox backends, composable strength scoring |
| Container runtime | **stiva** | 2.0.0 | Rust | OCI-compatible, overlay FS, daemonless |
| Audit chain | **libro** | 0.92.0 | Rust | SHA-256/BLAKE3 hash-linked tamper-proof logging |
| Firewall | **nein** | 0.90.0 | Rust | Programmatic nftables, policy, NAT |
| Edge fleet | **seema** | 0.1.0 | Rust | Edge fleet management and device orchestration |
| Scheduler | **samay** | 0.1.0 | Rust | Task scheduling daemon |
| Systems language | **cyrius** | 0.1.0 | Rust/ASM | Sovereign systems language — bootstraps from raw metal |

### Cyrius — The Language

**C.Y.R.I.U.S.** — *Consciousness Yields Righteous Intelligence Unveiling Self*

AGNOS's sovereign systems language. Named after **Cyrus the Great**, the king who decreed the rebuilding of the Temple of Solomon — the only non-Jewish figure called *Mashiach* in the Hebrew Bible (Isaiah 45:1).

Cyrius frees the OS from dependency on external toolchains, registries, and governance bodies. The bootstrap chain:

```
Assembly (raw metal) → Rust (structure) → Cyrius (sovereignty)
rustc 1.96.0-dev → cyrius-seed (assembler, 102 tests) → stage1a/1b (codegen) → self-hosting
```

- **cyrius-seed** (0.1.0) — Diamond-hardened zero-dependency assembler. 38 x86_64 instructions, ~13 MB/s pipeline, 12M ops/sec encoder.
- **stage1b** — Runtime codegen compiler. Variables, if/while, all comparison operators, nested control flow. 5051-byte binary, 32 tests.

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
| **libro** | Cryptographic audit chain (SHA-256/BLAKE3 hash-linked logging) |
| **sigil** | Trust verification (Ed25519 signing, integrity, revocation, delegation) |
| **bote** | MCP core service (JSON-RPC 2.0, tool registry, MCP 2025-11-25 compliant) |
| **t-ron** | MCP security monitor (auditing, rate limiting, injection detection, correlation) |
| **szal** | Workflow engine (branching, retry, rollback) |
| **abaco** | Math library (expression parsing, unit conversion) |

77 total shared crates — 56 at v1.0+ stable, 20 pre-1.0. Spanning OS infrastructure, science & knowledge (25 crates), media & audio (10), language & navigation (5), and physics & engineering (5). Full registry: [shared-crates.md](development/applications/shared-crates.md).

### Security Model

AGNOS implements defense-in-depth with quantitative scoring:

- **Sandbox apply order**: encrypted storage, MAC, Landlock, seccomp, network isolation, audit
- **Kavach**: 8 sandbox backends under one API with composable strength scoring (0-100)
- **Libro**: Tamper-proof SHA-256/BLAKE3 hash-linked audit chain for every agent action
- **Stiva**: Daemonless container runtime with no privilege override flags
- **Sigil**: Ed25519 signing, package integrity, trust delegation, revocation
- **Composable isolation**: Firecracker + jailer + stiva + sy-agnos + TPM = score 98/100

### MCP Tools

AGNOS provides 151+ built-in MCP (Model Context Protocol) tools enabling AI agents to interact with every subsystem. Consumer applications register additional tools via bote.

---

## Boot Profiles

Achieved boot times (2026-04-03):

| Mode | Initramfs | Init → Event Loop | Total (kernel+init) |
|------|-----------|-------------------|---------------------|
| Minimal | 2.4MB | 140ms | 2.98s |
| Desktop (all real) | 21MB | 80ms | 3.28s |
| Edge | 7.9MB | 99ms (+ 1s daimon) | 3.80s |

**Pure AGNOS desktop boot** — zero external dependencies. 7 real binaries: kybernet (PID 1, 2.2MB), daimon (11MB), hoosh (14MB), aethersafha (1.8MB), agnoshi (8.1MB), ifran (19MB) + argonaut library. Wave-parallel startup via argonaut.

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

- **System packages**: `.ark` format (signed tarballs + metadata), built via takumi recipes from zugot
- **Marketplace apps**: `.agnos-agent` format (manifest.json + sandbox.json + binaries)
- **Base system**: ~178 packages built from source in dependency order
- **Recipe count**: 376 total (116 base + 71 desktop + 25 AI + 9 network + 8 browser + 109 marketplace + 4 Python + 3 database + 31 edge) plus 90 in community bazaar

### CI/CD

Two-tier build architecture:
- **Tier 1** (rare): Self-hosted runner builds toolchain + base rootfs from source
- **Tier 2** (every release): GitHub Actions pulls cached base rootfs, overlays userland, creates ISO

---

## Consumer Applications

AGNOS ships with an ecosystem of 19+ first-party applications, all Rust-native, all integrating with daimon (agent orchestration) and hoosh (LLM inference):

| Application | Domain | Description |
|-------------|--------|-------------|
| **SecureYeoman** | AI platform | Sovereign AI agent platform (flagship) |
| **Agnostic** | AI automation | Python/CrewAI agent automation, 7 domain presets |
| **Jalwa** | Media | AI-native media player |
| **Shruti** | Audio | Digital audio workstation |
| **Tazama** | Video | AI-native video editor |
| **Rasa** | Image | AI-native image editor |
| **Mneme** | Knowledge | AI-native knowledge base |
| **Sutra** | Infrastructure | Infrastructure orchestrator (Ansible replacement) |
| **Tarang** | Media framework | Pure Rust media pipeline (ffmpeg replacement) |
| **Delta** | Development | Code hosting platform (git, PRs, CI/CD) |
| **Aequi** | Finance | Self-employed accounting platform (Tauri v2) |
| **BullShift** | Trading | Trading platform |
| **Ifran** | LLM management | LLM management and training |
| **Photis Nadi** | Productivity | Productivity application |
| **Nazar** | Monitoring | AI-native system monitor |
| **Vidhana** | Settings | System settings (egui GUI) |
| **Selah** | Screenshot | Screenshot and annotation tool |
| **Rahd** | Calendar | AI-native calendar and contacts |
| **Abacus** | Calculator | Desktop calculator (built on abaco crate) |

Each application follows the [First-Party Standards](development/applications/first-party-standards.md) including MCP tool registration, agnoshi intent patterns, marketplace recipes, and daimon integration.

---

## Named Subsystem Conventions

All AGNOS subsystems use multilingual names drawn from Arabic, Persian, Sanskrit, Greek, Latin, Japanese, Hebrew, Romanian, German, and other languages. This is not aesthetic — it is a deliberate **inversion of Babel**: drawing the *truest* word from whichever language holds it, reassembling the tower not by forcing one tongue but by honoring each.

The subsystems form a **divine court** — each role appears in every ancient temple architecture. The oracle (daimon), the mind (hoosh/nous), the shield (aegis), the watchman (phylax), the seal bearer (sigil), the armorer (kavach), the power (shakti), the messenger (bote), the helmsman (kybernet), the crew (argonaut).

See [Philosophy](philosophy.md) for the full exploration of AGNOS as temple architecture, the three arks, the bootstrap chain as genesis, and the deeper intention behind the project.

---

## Technical Statistics (as of 2026-04-03)

| Metric | Value |
|--------|-------|
| Shared crates | 77 (56 at v1.0+ stable) |
| Standalone repos | 23+ OS subsystems |
| Recipes | 376 OS + 90 community (moving to zugot) |
| Consumer applications | 19+ |
| MCP tools | 151+ built-in |
| Compiler warnings | 0 |
| Security audit rounds | 16 (0 remaining critical/high) |
| Boot time (desktop) | 3.2s total, 80ms init→event loop |
| Boot time (edge) | 3.8s total, 99ms init→ready |
| Kernel | Linux 6.6 LTS |
| Rust MSRV | 1.89 |
| Systems language | Cyrius (cyrius-seed 0.1.0, 102 tests) |

---

## See Also

- [Philosophy & Intention](philosophy.md) — the deeper vision behind AGNOS
- [Development Roadmap](development/roadmap.md) — phases, blockers, release targets
- [Application Development Roadmap](development/applications/roadmap.md) — planned first-party applications
- [First-Party Application Standards](development/applications/first-party-standards.md) — conventions for consumer apps
- [Shared Crates Reference](development/applications/shared-crates.md) — ecosystem crate registry
- [CI/CD Architecture](development/ci-cd-guide.md) — build and release pipeline
- [Network Evolution](development/network-evolution.md) — TCP/HTTP → QUIC → binary agent protocol
- [Performance Benchmarks](development/performance-benchmarks.md) — comparison data

---

*Last Updated: 2026-04-03*
