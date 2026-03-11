# AGNOS Development Roadmap

> **Status**: Pre-Beta | **Last Updated**: 2026-03-11
> **Userland complete** — 10500+ tests (3374 agent-runtime, 763 ai-shell), ~84% coverage, 0 warnings
> **Recipes**: 109 base + 53 desktop + 25 AI + 9 network + 8 browser + 8 marketplace + 4 python + 3 database = 219 total, 0 validation errors
> **Phases 10–12 complete** | **Phase 13**: 13A(infra)/13B/13D/13E done | **Audit**: 15 rounds
> **Engineering backlog**: Cleared (16/16 items complete)

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

### Completed in Phase 13

**13A — Self-Hosting Infra (4/8)**: QEMU boot validation (`qemu-boot-test.sh`), self-hosting validation scripts (`selfhost-validate.sh`, 4 phases), CI workflow (`selfhost-validation.yml`, weekly + manual), Rust module (`selfhost.rs`, 38 tests)

**13B — Hardware Support (8/8)**: NVIDIA proprietary (`nvidia-driver.toml`), NVIDIA nouveau (Mesa), AMD radeonsi (Mesa), Intel iris (Mesa), WiFi firmware (`linux-firmware.toml`), Bluetooth (`bluez.toml`), USB/Thunderbolt (`bolt.toml`), Printer support (`cups.toml`)

**13C — Documentation (3/7)**: Installation guide (`docs/installation/`), Kernel dev guide (`docs/development/kernel-guide.md`), Issue templates (bug, feature, security + config.yml)

**13D — Consumer App Integration (6/6)**: SecureYeoman, Photis Nadi, BullShift (ready), AGNOSTIC (5 MCP tools + 5 agnoshi intents), Delta (5 MCP tools + 5 agnoshi intents + CI), Aequi (5 MCP tools + 5 agnoshi intents + recipe)

**13E — CI Workflows (2/2)**: browser-ark CI (ready), marketplace-publish CI (7 apps: SY, BS, PN, AGNOSTIC, Synapse, Delta, Aequi)

**13E — WebView & Containers (2/2)**: AI-integrated WebView (`webview.rs`, 28 tests), AGNOS base Docker images (`docker/Dockerfile.agnos-base` + `scripts/ark-install.sh`)

**13E — Python Runtime Management (1/1)**: Python runtime manager (`python_runtime.rs`, 36 tests) — version discovery, `.python-version` file resolution, venv CRUD, pip proxy with audit trail, shim script generation, free-threaded Python 3.13t support

**Consumer API Improvements (5/5)**: External MCP tool registration (`POST/DELETE /v1/mcp/tools`), sandbox profile CRUD (`/v1/sandbox/profiles/custom/*`), event publish sender resolution, batch deregister (`POST /v1/agents/deregister/batch`), client-specified agent IDs in registration (13 tests)

---

## Engineering Backlog

Identified via code audit (2026-03-10). Prioritized by impact.

### Completed

**Performance & Memory**: `.to_lowercase()` → `(?i)` regex (ai-shell), single-pass `stats()` (agent-runtime), federation string clone reduction (3 clones eliminated), swarm vote tally single-pass optimization, 3 criterion benchmark suites added (intent parsing, screen capture, vector search scaling)

**Code Quality**: HTTP error response helpers, Delta API response normalization, MCP tool manifest refactored to data-driven `tool!` macro (121 lines saved), `Arc<RwLock<>>` consolidated in orchestrator (4 locks → 1 `OrchestratorState`), `check_resource_limits()` split into `check_memory_limits()` + `check_cpu_limits()`, `handle_unhealthy_agent()` split into `calculate_restart_backoff()` + `attempt_restart()`, `#[allow(dead_code)]` resolved (supervisor: `#[cfg(test)]`, pqc: hex module `#[cfg(test)]`, nous: `cache_dir()` accessor)

**Security**: Plugin sandbox syscall whitelist expanded, plugin resource limits enforced via `setrlimit` (RLIMIT_AS + RLIMIT_CPU), audit log failures escalated warn→error (6 occurrences), plugin socket directory hardened (0o700 dir + 0o600 helper)

**Installer — agnova (13/13)**: All items complete. mount ops, base/package install, security ops, first boot, cleanup, UEFI/BIOS, kernel version parameterization, systemd-boot, partition_device refactor, LUKS password stdin piping (`--batch-mode` + `--key-file=-` + `stdin` field in SystemOp), MBR partition count validation (max 4 primary), static IP via systemd-networkd (`10-static.network`)

### Remaining

*Engineering backlog cleared.*

---

## Phase 14 — Edge OS Profile (Planned)

> Target: Post-beta | Aligned with SecureYeoman 2026.3.11 edge binary

AGNOS as a minimal edge OS for running the SecureYeoman edge binary as an A2A
sub-agent on constrained hardware (Raspberry Pi, NUCs, IoT gateways, edge
servers). The edge binary connects upstream to a full SY instance via A2A
protocol, receives delegated tasks, executes locally, and reports back.

### 14A — Minimal Edge Boot Profile

| # | Item | Effort | Notes |
|---|------|--------|-------|
| 1 | Argonaut `Edge` boot mode (4th mode) | Medium | Skip compositor, shell optional, boot → daimon + SY edge only |
| 2 | Edge recipe set (~30 packages) | Medium | Kernel + coreutils + networking + TLS + SY edge binary — no desktop, no browser, no AI/ML stack |
| 3 | Target <256 MB disk, <128 MB RAM | Small | Strip debug symbols, minimal firmware, no man pages |
| 4 | Boot time target <5s to agent-ready | Small | argonaut already targets <3s; edge skips more stages |
| 5 | Read-only rootfs (dm-verity) | Medium | Immutable base, writable overlay for `/var/lib/secureyeoman` |

### 14B — A2A & Sub-Agent Networking

| # | Item | Effort | Notes |
|---|------|--------|-------|
| 1 | mDNS peer discovery (avahi/custom) | Medium | Auto-discover parent SY instance on LAN — replace stub in SY A2A |
| 2 | Auto-registration on boot | Small | `secureyeoman edge --register <parent-url>` in argonaut service chain |
| 3 | Mesh networking (WireGuard tunnel) | Medium | Edge ↔ main encrypted tunnel for remote/cross-network deployment |
| 4 | Heartbeat watchdog integration | Small | argonaut monitors SY edge process, auto-restart on failure |
| 5 | Bandwidth-aware task acceptance | Small | Edge advertises connection quality; parent routes accordingly |

### 14C — Hardware Targets

| # | Item | Effort | Notes |
|---|------|--------|-------|
| 1 | Raspberry Pi 4/5 (aarch64) image | Medium | Pre-built `.img` with edge profile, flash-and-go |
| 2 | x86_64 NUC/mini-PC image | Small | ISO with edge profile auto-selected |
| 3 | RISC-V (SiFive, StarFive) | Large | Cross-compile toolchain + kernel config |
| 4 | OCI container image (edge) | Small | `docker run agnos-edge` for existing Linux hosts |

### 14D — Edge Security

| # | Item | Effort | Notes |
|---|------|--------|-------|
| 1 | Hardware attestation (TPM 2.0) | Medium | Edge proves integrity to parent before receiving tasks |
| 2 | Minimal Landlock + seccomp profile | Small | Tight syscall allowlist for edge binary only |
| 3 | Encrypted local state (LUKS) | Small | Already supported; ensure edge profile enables by default |
| 4 | Signed OTA updates via ark | Medium | Parent pushes updates to fleet of edge nodes |
| 5 | Certificate pinning to parent | Small | Edge only trusts its registered parent instance |

### 14E — Fleet Management

| # | Item | Effort | Notes |
|---|------|--------|-------|
| 1 | Edge node registry in daimon | Medium | Track fleet: health, capabilities, load, location |
| 2 | agnoshi intents for edge fleet | Small | `list edge nodes`, `deploy to edge`, `update edge fleet` |
| 3 | MCP tools for edge management (5) | Medium | `edge_list`, `edge_deploy`, `edge_update`, `edge_health`, `edge_decommission` |
| 4 | Dashboard panel in SY | Medium | Edge fleet topology, health, task distribution (SY 2026.3.11+) |
| 5 | Capability-based task routing | Small | Parent auto-routes tasks to edge nodes by advertised capabilities (GPU, network, location) |

---

## Release Roadmap

### Beta Release — Q4 2026

**Criteria:**
- [x] Phase 10 complete — 108 base system recipes, self-hosting toolchain
- [x] Phase 11 complete — 88 desktop, networking & AI/ML recipes
- [x] Phase 12 complete — Argonaut init, ark package manager, agnova installer
- [x] Phase 13B complete — GPU drivers, WiFi, Bluetooth, Thunderbolt, printing
- [x] Phase 13D complete — All 6 consumer apps integrated
- [ ] AGNOS boots from ISO on bare metal (UEFI) and QEMU
- [ ] Self-hosting: can rebuild itself from source
- [ ] Third-party security audit complete
- [ ] Community testing program active

### v1.0 Release — Q2 2027

**Criteria:**
- [ ] Phase 13 complete — Documentation, community
- [ ] All consumer apps published to mela
- [x] Python runtime management
- [ ] Enterprise features: SSO (done), audit logging (done), mTLS (done)
- [ ] 6 months of beta testing with no critical bugs
- [ ] Commercial support available

### Edge OS Profile — Post-Beta (aligned with SY 2026.3.11+)

**Criteria:**
- [ ] Phase 14A complete — Edge boot mode, minimal recipe set, <256 MB disk
- [ ] Phase 14B complete — mDNS discovery, auto-registration, WireGuard mesh
- [ ] Phase 14C complete — Raspberry Pi + x86_64 + OCI images
- [ ] Phase 14D complete — TPM attestation, signed OTA, certificate pinning
- [ ] Phase 14E complete — Fleet management (daimon registry, MCP tools, SY dashboard)

---

## Key Performance Indicators (KPIs)

### Current Status (as of 2026-03-10)

| Metric | Target | Current | Status |
|--------|--------|---------|--------|
| Code Coverage | >80% | ~84.3% | Met |
| Test Pass Rate | 100% | 100% | Met |
| Total Tests | 400+ | 10400+ | Met |
| Agent Spawn Time | <500ms | ~300ms | Met |
| Shell Response Time | <100ms | ~50ms | Met |
| Memory Overhead | <2GB | ~1.2GB | Met |
| Boot Time | <10s | N/A | Pending (Phase 13A) |
| CIS Compliance | >80% | ~85% | Met |
| Stub Implementations | 0 | 0 | Met |
| Compiler Warnings | 0 | 0 | Met |
| Base System Recipes | ~108 | 109 | Complete |
| Desktop/AI Stack Recipes | ~62 | 88 | Complete |
| Hardware Recipes | 8 | 8 | Complete |
| Consumer Apps | 6 | 6 | Complete |
| MCP Tools | — | 31 | Complete |
| Recipe Validation Errors | 0 | 0 | Complete |
| Security Audit Rounds | 15 | 15 | Complete |
| Self-Hosting Infra | Yes | Yes | Phase 13A (infra done, actual validation pending) |

### By Component

| Component | Tests | Notes |
|-----------|-------|-------|
| agnos-common | 307 | Secrets, telemetry, LLM types, manifest, rate limits, audit chain |
| agnos-sys | 750+ | 16 modules: audit, mac, netns, dmverity, luks, ima, tpm, secureboot, certpin, bootloader, journald, udev, fuse, pam, update, llm |
| agent-runtime | 3369+ | 31 MCP tools, orchestrator, IPC, sandbox, registry, marketplace, federation, migration, scheduler, PQC, safety, finetune, formal_verify, sandbox_v2, rl_optimizer, cloud, collaboration, sigil, aegis, takumi, argonaut (117), agnova (99), ark (49), grpc, service_mesh, oidc, delegation, vector_rest, marketplace_backend, selfhost (38), webview (28), python_runtime (36) |
| llm-gateway | 710 | 15 providers, rate limiting, streaming, cert pinning, hardware acceleration, token budgets |
| ai-shell | 1472 | 30+ intents (5 Aequi, 5 Agnostic, 5 Delta, 5 Photis, 10+ system), approval workflow, dashboard, aliases |
| desktop-environment | 1447+ | Wayland protocol, screen capture (31), screen recording (22+), plugin host (31), xwayland (20), shell integration (26), theme bridge (18) |

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
| **daimon** | Agent orchestrator (port 8090, 31 MCP tools) | `agent-runtime/` |
| **agnosys** | Kernel interface | `agnos-sys/` |
| **agnostik** | Shared types library | `agnos-common/` |
| **shakti** | Privilege escalation | `agnos-sudo/` |
| **agnoshi** | AI shell (`agnsh`, 30+ intents) | `ai-shell/` |
| **aethersafha** | Desktop compositor | `desktop-environment/` |
| **ark** | Unified package manager | `ark.rs`, `/v1/ark/*` |
| **nous** | Package resolver daemon | `nous.rs` |
| **takumi** | Package build system | `takumi.rs` |
| **mela** | Agent marketplace | `marketplace/` module |
| **aegis** | System security daemon | `aegis.rs` |
| **sigil** | Trust verification | `sigil.rs` |
| **argonaut** | Init system | `argonaut.rs` |
| **agnova** | OS installer | `agnova.rs` |
| **vansh** | Voice AI shell (planned) | TBD |

---

## Contributing

### Priority Contribution Areas

1. **Self-hosting on-target (Phase 13A)** — Build AGNOS on AGNOS with actual ISO boot
2. **SHA256 verification** — Fill in real checksums for all 218 recipes
3. **Documentation (Phase 13C)** — Installation guide, kernel dev guide, video tutorials
4. **SHA256 verification** — Fill in real checksums for all 218 recipes
5. **Community testing** — Beta tester enrollment + bug tracker setup

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

*Last Updated: 2026-03-10 | Next Review: 2026-03-17*
