# AGNOS Development Roadmap

> **Status**: Pre-Beta | **Last Updated**: 2026-03-11
> **Userland complete** — 10876+ tests (3622+ agent-runtime, 1554 ai-shell), ~84% coverage, 0 warnings
> **Recipes**: 109 base + 53 desktop + 25 AI + 9 network + 8 browser + 8 marketplace + 4 python + 3 database + 29 edge = 248 total, 0 validation errors
> **Phases 10–14 complete** | **Phase 13**: 13A(infra)/13B/13D/13E done | **Phase 14**: Edge OS Profile done | **Audit**: 16 rounds
> **Audit round 16**: 14 CRITICAL + 27 HIGH fixed, 2 HIGH + 17 MEDIUM remaining

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
| **13D** | **Consumer app integration** — SecureYeoman, Photis Nadi, BullShift, AGNOSTIC, Delta, Aequi (all with MCP tools + agnoshi intents) |
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

---

## Engineering Backlog

### HIGH — Remaining (2)

| # | Category | Issue | File(s) | Effort |
|---|----------|-------|---------|--------|
| H23 | Quality | 4 monolithic files >3600 lines need splitting | `wayland.rs`, `main.rs`, `mcp_server.rs`, `supervisor.rs` | Large |
| H25 | Quality | String matching where enums should be used | Multiple | Medium |

### Recently Fixed (13 items cleared)

**Security**: Tarball symlink path traversal (reject symlinks + verify resolved paths in `local_registry.rs`), agent ID authorization per-agent (memory handlers require registered agent), memory store per-agent key limit (1000 keys max), prompt injection Unicode bypass (strip zero-width/invisible chars before matching), XWayland surface ID removed from error messages

**Performance**: Vector search results no longer clone embeddings (copy only id/metadata/content), CGroup setup runs on `spawn_blocking` with 5s timeout, temperature clamped to [0.0, 2.0] and top_p to [0.0, 1.0] at gateway level

**Ops**: Audit chain persistence (`save_to_file`/`load_from_file` with atomic writes + integrity verification on load), audit buffer pagination uses safe iterator-based `skip().take()` with 1000 per-page cap, desktop environment SIGHUP handler for config reload, cache TTL now per-request via `set_with_ttl()`, HTTP request handling benchmarks (health, agents, memory set/get)

**Not issues** (removed from backlog): Rate limiter correctly uses monotonic `Instant`, Pub/Sub wildcard uses efficient `starts_with()` prefix matching, marketplace signature already optional when keyring=None, env_keep uses double-check pattern (allowlist + blocklist)

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
| Consumer Apps | 6 | 6 | Complete |
| MCP Tools | — | 36 | Complete (5 edge tools added) |
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
| ai-shell | 1554 | 35+ intents (5 Aequi, 5 Agnostic, 5 Delta, 5 Photis, 5 Edge, 10+ system), approval workflow, dashboard, aliases |
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
| **vansh** | Voice AI shell (planned) | TBD |

---

## Contributing

### Priority Contribution Areas

1. **Self-hosting on-target (Phase 13A)** — Build AGNOS on AGNOS with actual ISO boot
2. **SHA256 verification** — Fill in real checksums for all 248 recipes
3. **Documentation (Phase 13C)** — Video tutorials, support portal
4. **Community testing** — Beta tester enrollment + bug tracker setup
5. **Engineering backlog** — H23 file splitting, H25 enum refactoring, 17 MEDIUM items

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

*Last Updated: 2026-03-11 | Next Review: 2026-03-18*
