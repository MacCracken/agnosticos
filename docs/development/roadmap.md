# AGNOS Development Roadmap

> **Status**: Pre-Beta | **Last Updated**: 2026-03-10
> **Userland complete** — 10000+ tests (3204 agent-runtime), ~82% coverage, 0 warnings
> **Recipes**: 109 base + 53 desktop + 25 AI + 9 network + 8 browser + 7 marketplace + 4 python + 3 database = 218 total, 0 validation errors
> **Phases 10–12 complete** | **Phase 13**: 13A(infra)/13B/13D/13E done | **Audit**: 15 rounds

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

### Performance & Memory (P1)

| # | Item | Component | Effort | Notes |
|---|------|-----------|--------|-------|
| 1 | Replace `.to_lowercase().to_string()` with `(?i)` regex flags in parse hot path | ai-shell | Done | Saves allocation per parse call |
| 2 | Reduce string cloning in federation (23 clones → refs/Cow) | agent-runtime | Small | `federation.rs` lines 232, 290-311 |
| 3 | Single-pass node status counting in `stats()` | agent-runtime | Done | Was 3× O(n), now O(n) |
| 4 | Add intent parsing throughput benchmark | ai-shell | Small | Critical hot path, no bench yet |
| 5 | Add screen capture performance benchmark | desktop-environment | Small | PNG encoding, pixel copy |
| 6 | Add vector search scaling benchmark (1K/10K/100K) | agent-runtime | Small | Currently O(N*D) brute force |
| 7 | Cache vote tally in swarm (incremental vs recompute) | agent-runtime | Small | `swarm.rs:299-309` |

### Code Quality (P2)

| # | Item | Component | Effort | Notes |
|---|------|-----------|--------|-------|
| 1 | Extract HTTP error response helpers | agent-runtime | Done | `bad_request()`, `not_found()`, etc. |
| 2 | Extract MCP tool manifest to data-driven format | agent-runtime | Medium | `build_tool_manifest()` is 340 lines |
| 3 | Consolidate `Arc<RwLock<>>` state in orchestrator | agent-runtime | Medium | 5 separate locks → unified state |
| 4 | Split `check_resource_limits()` (110 lines) | agent-runtime | Small | Into memory, CPU, tracking helpers |
| 5 | Split `handle_unhealthy_agent()` (120 lines) | agent-runtime | Small | Into backoff, restart, recovery |
| 6 | Resolve `#[allow(dead_code)]` markers | agent-runtime | Small | supervisor.rs, pqc.rs, nous.rs |
| 7 | Normalize Delta API bridge response format | agent-runtime | Done | Bare array → wrapped `{repositories:[]}` |

### Security (P2)

| # | Item | Component | Effort | Notes |
|---|------|-----------|--------|-------|
| 1 | Expand plugin sandbox base syscall whitelist | desktop-environment | Done | Added epoll, futex, clock_gettime, etc. |
| 2 | Enforce plugin resource limits (max_memory, max_cpu) | desktop-environment | Medium | Stored but never checked |
| 3 | Escalate audit log failures from warn → error | agent-runtime | Small | `supervisor.rs:708-714` |
| 4 | Add plugin socket permission hardening (0600) | desktop-environment | Small | `plugin_host.rs:226` |

### Installer — agnova (P1)

| # | Item | Status | Notes |
|---|------|--------|-------|
| 1 | `plan_mount_ops()` — mount partitions at target | Done | Sorted by depth, swap activation |
| 2 | `plan_install_base_ops()` — deploy base system | Done | Tarball + ark fallback |
| 3 | `plan_install_packages_ops()` — mode-specific packages | Done | ark install with mode packages |
| 4 | `plan_security_ops()` — firewall + sysctl + IMA | Done | nftables, kernel hardening |
| 5 | `plan_first_boot_ops()` — argonaut service enable | Done | Per-mode service list, desktop compositor |
| 6 | `plan_cleanup_ops()` — unmount + LUKS close | Done | Reverse depth unmount, swap deactivation |
| 7 | UEFI/BIOS detection + GRUB BIOS support | Done | `is_uefi_system()`, `--target=i386-pc` |
| 8 | Parameterize kernel version in boot entries | Done | `kernel_version()` method, no hardcoding |
| 9 | systemd-boot loader.conf + entry files | Done | `agnos.conf`, `agnos-rescue.conf` |
| 10 | Refactor `partition_device()` to shared helper | Done | Deduplicated nvme/mmcblk logic |
| 11 | LUKS password stdin piping / key file support | Not started | `cryptsetup luksFormat` hangs without it |
| 12 | MBR partition count validation (max 4 primary) | Not started | Silent failure on >4 partitions |
| 13 | Static IP + gateway network configuration | Not started | `plan_network_ops()` incomplete |

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

### Current Status (as of 2026-03-10)

| Metric | Target | Current | Status |
|--------|--------|---------|--------|
| Code Coverage | >80% | ~82% | Met |
| Test Pass Rate | 100% | 100% | Met |
| Total Tests | 400+ | 10200+ | Met |
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
| agent-runtime | 3191+ | 31 MCP tools, orchestrator, IPC, sandbox, registry, marketplace, federation, migration, scheduler, PQC, safety, finetune, formal_verify, sandbox_v2, rl_optimizer, cloud, collaboration, sigil, aegis, takumi, argonaut (117), agnova (99), ark (49), grpc, service_mesh, oidc, delegation, vector_rest, marketplace_backend, selfhost (38), webview (28), python_runtime (36) |
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
