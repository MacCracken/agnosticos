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

| Milestone | Date | Days from Start |
|-----------|------|----------------|
| First commit | 2026-02-11 | 0 |
| Alpha release | 2026-03-05 | 22 |
| First ISO build | 2026-03-22 | 39 |
| First clean multi-arch release | 2026-03-31 | 48 |
| Monolith dismantled | 2026-04-01 | 49 |
| Cyrius self-hosting compiler | 2026-04-04 | 52 |
| AGNOS kernel compiled by Cyrius | 2026-04-04 | 52 |
| Cyrius ecosystem (stdlib, tools, crate rewrites) | 2026-04-05 | 53 |
| **Target: Beltane release** | **2026-05-01** | **79** |

From initial commit to self-hosting sovereign language with its own kernel in **52 days**. Full timeline: [History & Timeline](history.md).

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
| Systems language | **cyrius** | 1.0 | Cyrius | Sovereign systems language — self-hosting from 29KB seed, 128KB compiler, 251 tests |
| Build tool | **cyrb** | 1.0 | Cyrius | Build system written in Cyrius: 20+ commands, compile, test, audit, self-host |

### Cyrius — The Language

**C.Y.R.I.U.S.** — *Consciousness Yields Righteous Intelligence Unveiling Self*

AGNOS's sovereign systems language. Named after **Cyrus the Great**, the king who decreed the rebuilding of the Temple of Solomon — the only non-Jewish figure called *Mashiach* in the Hebrew Bible (Isaiah 45:1).

Cyrius frees the OS from dependency on external toolchains, registries, and governance bodies. Zero external dependencies — no C compiler, no Rust, no Python, no LLVM, no libc in any path.

#### Compiler

Self-hosting modular compiler: 128KB binary, dual architecture (x86_64 + aarch64, byte-identical on Raspberry Pi). Compiles itself in ~11ms. Full bootstrap from 29KB seed in ~42ms. Byte-exact reproducibility. 251 tests, 0 failures. `cyrb audit` → 10/10.

**Migration scope**: 107 Rust repos (~1M total lines of code) across the AGNOS ecosystem. 5 crates already rewritten in Cyrius (agnostik, agnosys, kybernet, nous, ark). 102 remaining, in 5 waves gated by language feature milestones (v1.1 through v1.4).

#### Language Features

Structs, typed pointers (element-size scaling), enums, switch/match, for loops, for-in ranges, break/continue, elif, logical && / || with short-circuit, inline assembly (raw bytes + 18 mnemonics), progressive type annotations, function pointers, nested structs with chained dot access, global initializers, heap allocator, argc/argv, include system, buffered I/O, pattern matching, closures/lambdas, module system (mod/use/pub), trait impl blocks, block scoping, floating point (f64 via SSE2), methods on structs, operator overloading, feature flags (#define/#ifdef), enum constructors with payloads, C FFI header generation, subprocess bridge.

#### Standard Library

20 modules — built from scratch in Cyrius:
string, alloc, str, vec, io, fmt, args, fnptr, hashmap, json, regex, process, filesystem, networking, tagged unions, traits, benchmarking, bounds checking, assertions, callbacks.

#### Developer Tools

8 tools, all written in Cyrius:
- **cyrb** — build system (compile, test, self-host, suite runner, 18 commands)
- **cyrfmt** — code formatter
- **cyrlint** — linter
- **cyrdoc** — documentation generator
- **cyrc** — compiler CLI
- **ark** — package manager (rewritten from Rust)
- installer + version manager

#### Kernel

58KB AGNOS kernel compiled by Cyrius:
Multiboot1 boot → 32-to-64 bit shim → long mode, serial console, GDT, IDT (256 vectors), PIC remap, PIT timer (100Hz), keyboard input, page tables (16MB identity map, 2MB pages), physical memory manager (bitmap, 4096 pages), virtual memory manager (map/unmap/alloc), process table, syscall interface (exit, write, getpid). Built in 5ms.

#### Programs

58 programs compiled by Cyrius (userspace + kernel + tools + algorithms). Userspace programs are 10-233x smaller than GNU equivalents and match or beat GNU on speed (wc 2.4x faster after buffered I/O).

#### Crate Rewrites

5 Rust crates rewritten in Cyrius, eliminating Rust dependencies:
- **agnostik** — shared types (6 modules, 54 tests)
- **agnosys** — syscall bindings (50 constants, 20+ wrappers)
- **kybernet** — PID 1 init (7 modules, 38 tests)
- **nous** — package resolver
- **ark** — package manager

#### Architectures

x86_64 (primary) + aarch64 (byte-identical self-hosting on Raspberry Pi hardware). Future targets: RISC-V (ESP32-C3), MIPS (Hidizs AP80), Xtensa (ESP32-S3).

#### Benchmarks

38 benchmarks across 3 tiers with CSV regression tracking. Binary sizes 10-233x smaller than GNU. Compile speed faster than process fork+exec. 251 tests, 0 failures. `cyrb audit` → 10/10.

#### Bootstrap Chain

```
29KB seed (committed binary, auditable)
  → 12KB stage1f (bootstrap compiler)
    → 128KB cc2 (full self-hosting compiler, v1.0)
      → 62KB kernel (AGNOS, VM + processes + syscalls)
      → 58 programs (userspace + kernel + tools)
      → 20 stdlib modules
      → 5 of 107 Rust crate rewrites complete
      → 8 developer tools (20+ cyrb commands)
      → Dual architecture: x86_64 + aarch64 (byte-identical)
Total: ~230KB from void to sovereign OS
Migration target: 107 repos, ~1M lines of Rust → Cyrius
```

Full details: [Cyrius README](https://github.com/MacCracken/cyrius) | [Cyrius Roadmap](https://github.com/MacCracken/cyrius/blob/main/docs/development/roadmap.md) | [Migration Plan](cyrius-lang-migration.md)

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

77 total shared crates — 56 at v1.0+ stable, 20 pre-1.0. Spanning OS infrastructure, science & knowledge (25 crates), media & audio (10), language & navigation (5), and physics & engineering (5). Full registry: [Library Registry](../applications/libs/README.md).

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

Each application follows the First-Party Standards including MCP tool registration, agnoshi intent patterns, marketplace recipes, and daimon integration.

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
- [History & Timeline](history.md) — full project timeline with dated milestones
- Development Roadmap — phases, blockers, release targets
- Application Development Roadmap — planned first-party applications
- First-Party Application Standards — conventions for consumer apps
- [Library Registry](../applications/libs/README.md) — ecosystem library registry
- CI/CD Architecture — build and release pipeline
- Network Evolution — TCP/HTTP → QUIC → binary agent protocol
- Performance Benchmarks — comparison data

---

*Last Updated: 2026-04-03*
