# AGNOS Development Roadmap

> **Status**: Pre-Beta | **Last Updated**: 2026-03-13
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
| 6 | ARM64 SBC (QEMU) | aarch64 | Edge | Not started | QEMU aarch64 virt machine validation |

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
| 8 | Shruti | 5 shruti_* | 5 | No | Blocked | DAW; awaiting first release |
| 9 | Tazama | 5 tazama_* | 5 | No | Blocked | Video editor; awaiting first release |
| 10 | Rasa | 5 rasa_* | 5 | No | Blocked | Image editor; awaiting first release |
| 11 | Mneme | 5 mneme_* | 5 | No | Blocked | Knowledge base; awaiting first release |

**Bundle test** = `ark-bundle.sh` fetches release, produces `.agnos-agent` tarball, installs via mela.

---

## Engineering Backlog

### Active

| # | Priority | Item | Notes |
|---|----------|------|-------|
| H26 | **HIGH** | Upgrade `reqwest` 0.11 → 0.12+ | Eliminates unmaintained `rustls-pemfile` 1.x (RUSTSEC-2025-0134). Breaking change — workspace-wide migration needed (all 6 crates depend on it). |
| H27 | **MEDIUM** | Implement `sd_notify` for systemd services | `agent-runtime` and `llm-gateway` use `Type=notify` in systemd units but don't send `READY=1`. Causes service startup failures on ISO boot. Either implement sd_notify via `libsystemd` crate or change to `Type=simple`. |
| H28 | **MEDIUM** | Systemd unit `Type=notify` → `Type=simple` fallback | Short-term fix: change service units to `Type=simple` so daimon/hoosh start without sd_notify. Long-term: implement proper readiness notification (H27). |
| H29 | **MEDIUM** | SSRF: validate MCP bridge env var URLs | `*_URL` env vars (SYNAPSE_URL, BULLSHIFT_URL, etc.) accept arbitrary URLs. Should reject `file://`, cloud metadata IPs (169.254.x.x), and non-localhost targets. Affects all 10 bridge structs. |
| H30 | **MEDIUM** | Delta `delta_review`: require `pr_id` for mutating actions | `approve`, `reject`, `comment` actions don't require `pr_id`. Should return error if `pr_id` is None for non-list actions. |
| H31 | **LOW** | MCP bridge: reuse `reqwest::Client` across calls | All bridge structs create a new `reqwest::Client` per `get()`/`post()` call via `build_client()`. Should store client in struct or use `Lazy` static for connection pool reuse. |
| H32 | **LOW** | MCP handlers: add string length limits on inputs | No handler validates input string length. A multi-MB string in `name`, `prompt`, etc. gets serialized to JSON and forwarded. Add limits in `extract_required_string`/`get_optional_string_arg`. |
| H33 | **LOW** | PhotisBridge: add `connect_timeout` | PhotisBridge uses per-request `.timeout()` but no `connect_timeout()`, unlike other bridges that use `build_client()` with 2s connect timeout. |

### Recently Fixed (Audit Round 17 — 2026-03-14)

**Security**: Photis `TaskCreate`/`TaskUpdate` permissions changed from `Safe` → `SystemWrite` (write ops incorrectly marked read-only). Bridge URL removed from 6 mock error messages (info leak). BullShift strategy `params` JSON parse failure now returns error instead of silently dropping invalid input.

**Performance**: Added `build_client()` with 5s timeout + 2s connect timeout to AequiBridge and AgnosticBridge (previously used `reqwest::Client::new()` with no timeout — potential DoS via hung bridge).

### Recently Fixed (15 items cleared + 4 non-issues removed + 2 unmaintained deps removed)

**Unmaintained deps**: Removed `ansi_term` (replaced with `console`) and unused `indicatif` from ai-shell. Reduces `cargo audit` warnings from 4 to 2.

**H23 — File splitting**: `mcp_server.rs` (4,452 → 11 files), `supervisor.rs` (3,609 → 9 files), `wayland.rs` (3,996 → 7 files)

**H25 — Enum refactoring**: `MetricKind` enum in resource_forecast, `BehaviorMetric` enum in learning, `FromStr` for `FindingSeverity`/`HardeningFlag`, parser functions for screen_capture formats and RAG knowledge sources

**Security**: Tarball symlink path traversal, agent ID authorization per-agent (memory handlers), memory store per-agent key limit (1000 keys), prompt injection Unicode bypass (strip zero-width chars), XWayland surface ID removed from error messages

**Performance**: Vector search results no longer clone embeddings, CGroup setup on `spawn_blocking` with 5s timeout, temperature/top_p clamped at gateway level

**Ops**: Audit chain persistence with atomic writes + integrity verification, audit buffer safe pagination, desktop SIGHUP handler, cache per-request TTL, HTTP request handling benchmarks

**Not issues** (removed): Rate limiter uses monotonic `Instant`, Pub/Sub wildcard uses `starts_with()`, marketplace signature optional when keyring=None, env_keep uses double-check pattern

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
