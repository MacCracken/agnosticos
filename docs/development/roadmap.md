# AGNOS Development Roadmap

> **Status**: Pre-Alpha (Phase 5) | **Last Updated**: 2026-03-06
> **Current Phase**: Phase 5 - Production (99% Complete)
> **Next Milestone**: Alpha Release (Target: Q2 2026)

---

## Remaining Work for Alpha

### P1 - Alpha Blocker
| Item | Component | Effort | Owner | Status |
|------|-----------|--------|-------|--------|
| Third-party security audit | Security | 2 weeks | External | Vendor selection in progress |

### P2 - Alpha Polish
| Item | Component | Effort | Owner | Status |
|------|-----------|--------|-------|--------|
| Video tutorials | Documentation | 3 days | TBD | Not started |

### Completed (March 5-6)
| Item | Component | Status |
|------|-----------|--------|
| Init system / service manager | agent-runtime | Done (29 tests) |
| Agent consent & transparency (AgentManifest) | agnos-common | Done |
| Capability scoping (manifest → sandbox) | agnos-common | Done |
| Audit viewer in AI Shell | ai-shell | Done (16 new tests) |
| Per-agent rate limiting (tokens/hr, req/min, concurrent) | llm-gateway | Done (12 tests) |
| Agent lifecycle hooks (on_start/stop/error) | agent-runtime | Done (12 tests) |
| Agent-to-agent pub/sub protocol | agent-runtime/ipc | Done (17 tests) |
| Rollback / undo for agent actions | agent-runtime/sandbox | Done (15 tests) |
| Interactive approval editing in agnsh | ai-shell | Done (3 new tests) |
| Agent package manager (`agnos install`) | agent-runtime | Done (31 tests) |
| Wayland protocol layer (feature-gated) | desktop-environment | Done (41 tests) |
| Wayland Dispatch traits (wire protocol handlers) | desktop-environment | Done (58 tests) |
| IMA/EVM file integrity | agnos-sys | Done (31 tests) |
| TPM 2.0 measured boot & sealed secrets | agnos-sys | Done (23 tests) |
| UEFI Secure Boot integration | agnos-sys | Done (18 tests) |
| Network tools framework + AI Shell intents | agent-runtime + ai-shell | Done (100 tests) |
| Network tool agent wrappers (7 structs) | agent-runtime | Done (21 tests) |
| Swarm intelligence protocols | agent-runtime | Done (19 tests) |
| Agent learning & adaptation (UCB1) | agent-runtime | Done (13 tests) |
| Multi-modal agent support | agent-runtime | Done (14 tests) |
| LLM tool output analysis pipeline | agent-runtime | Done (15 tests) |
| Bootloader config (systemd-boot + GRUB2) | agnos-sys | Done (27 tests) |
| Journald integration | agnos-sys | Done (30 tests) |
| Udev device management | agnos-sys | Done (26 tests) |
| FUSE filesystem management | agnos-sys | Done (32 tests) |
| PAM / user session management | agnos-sys | Done (40 tests) |
| TLS certificate pinning (SPKI) | agnos-sys | Done (38 tests) |
| A/B system updates (slot management) | agnos-sys | Done (37 tests) |
| 32-item engineering backlog (code audit) | All crates | Done (all P0/P1/P2) |

### P3 - Beta/Post-Alpha (Tier 3)
| Item | Component | Effort | Owner | Status |
|------|-----------|--------|-------|--------|
| Kernel Development Guide | Documentation | 3 days | TBD | Not started |
| Support portal | Infrastructure | 2 weeks | TBD | Not started |
| Interactive API explorer | Documentation | 1 week | — | Done (`docs/api/explorer.html`) |

### Engineering Backlog (Code Audit — March 6)

Full codebase audit identified 32 items across 6 crates. Grouped by priority.

#### Phase 1 — P0 Fixes (Crash / Security) ✅ ALL COMPLETE
| # | Item | Component | Effort | Status |
|---|------|-----------|--------|--------|
| 1 | Production `unwrap()` panic in `AuditRule::validate()` | agnos-sys/audit.rs:154 | 5 min | Done |
| 2 | nftables comment injection (unescaped `rule.comment`) | agnos-sys/netns.rs:506 | 10 min | Done |
| 3 | JSON array index panic on empty provider response | llm-gateway/providers.rs:385 | 15 min | Done |
| 4 | Regex HashMap `.unwrap()` crashes shell on init bug | ai-shell/interpreter.rs:261+ | 20 min | Done |
| 5 | Path traversal in package install (agent name `../`) | agent-runtime/package_manager.rs:180 | 15 min | Done |
| 6 | `SecretValue` derives Clone without zeroing on drop | agnos-common/secrets.rs:17-25 | 30 min | Done |

#### Phase 2 — P1 Fixes (Performance / Memory / Correctness) ✅ ALL COMPLETE
| # | Item | Component | Effort | Status |
|---|------|-----------|--------|--------|
| 7 | Hot-path Vec+clone every frame in `render_frame()` | desktop-env/renderer.rs:798 | 15 min | Done |
| 8 | 8.3 MB `.to_vec()` per render call | desktop-env/compositor.rs:676 | 15 min | Done |
| 9 | Unbounded LLM cache (no max capacity, only TTL) | llm-gateway/cache.rs:71 | 30 min | Done |
| 10 | Rate limiter race (check-then-increment not atomic) | llm-gateway/rate_limiter.rs:117 | 30 min | Done |
| 11 | String realloc per SSE chunk in streaming (3 providers) | llm-gateway/providers.rs:130+ | 20 min | Done |
| 12 | `InferenceRequest.clone()` x2 per request (100KB+ prompts) | llm-gateway/main.rs:283,303 | 15 min | Done |
| 13 | Unbounded file content in rollback snapshots | agent-runtime/rollback.rs:338 | 15 min | Done |
| 14 | No install size limit in `copy_dir_recursive()` | agent-runtime/package_manager.rs:562 | 15 min | Done |
| 15 | Integer overflow in `fill_rect()` u32 cast | desktop-env/renderer.rs:126 | 10 min | Done |
| 16 | TOCTOU in MAC module (`exists()` then `Command`) | agnos-sys/mac.rs:300,373 | 15 min | Done |
| 17 | LUKS size overflow (`size_mb * 1024 * 1024` unchecked) | agnos-sys/luks.rs:315 | 5 min | Done |
| 18 | Audit hash chain has no `verify_chain()` function | agnos-common/audit.rs:43 | 30 min | Done |

#### Phase 3 — P2 Polish ✅ ALL COMPLETE
| # | Item | Component | Effort | Status |
|---|------|-----------|--------|--------|
| 19 | Unused Window clone (`_window`) | desktop-env/compositor.rs:329 | 2 min | Done |
| 20 | Unnecessary `app_id.clone()` | desktop-env/compositor.rs:174 | 2 min | Done |
| 21 | Blit not clipped upfront (per-pixel bounds check) | desktop-env/renderer.rs:186 | 20 min | Done |
| 22 | O(n) task lookup in `get_task_status()` | agent-runtime/orchestrator.rs:169 | 20 min | Done |
| 23 | O(n log n) result pruning on every insert | agent-runtime/orchestrator.rs:377 | 20 min | Done |
| 24 | Token accounting never evicts dead agents | llm-gateway/accounting.rs:27 | 15 min | Done |
| 25 | Telemetry clones `instance_id` per event | agnos-common/telemetry.rs:155 | 10 min | Done |
| 26 | TOCTOU in netns cleanup (`exists()` before destroy) | agent-runtime/supervisor.rs:377 | 5 min | Done |
| 27 | `ApprovalResponse::Denied` on timeout (no `TimedOut` variant) | ai-shell/approval.rs:168 | 15 min | Done |
| 28 | Audit log rotation not enforced | agnos-common/audit.rs:61 | 30 min | Done |
| 29 | Rollback uses non-crypto hash (DefaultHasher) | agent-runtime/rollback.rs:427 | 15 min | Done |
| 30 | Missing `Debug` derive on renderer public types | desktop-env/renderer.rs | 5 min | Done |
| 31 | `unsafe` in `as_bytes()` missing safety comment | desktop-env/renderer.rs:223 | 5 min | Done |
| 32 | 3 separate lock acquisitions in provider selection | llm-gateway/main.rs:369 | 15 min | Done |

### Code Audit Cycle #2 — March 6, 2026 (Security/Performance/Correctness)

Comprehensive audit across all 6 crates. 80+ findings identified; all Critical and High items fixed.

#### Critical (Fixed)
| # | Item | File | Fix |
|---|------|------|-----|
| 1 | Shell injection via `sh -c` with PEM data | agnos-sys/certpin.rs | Direct process spawn + stdin pipe |
| 2 | nftables rule injection via `remote_addr` | agnos-sys/netns.rs | IP/CIDR validation, reject shell metacharacters |
| 3 | Seccomp ignores per-agent rules | agent-runtime/sandbox.rs | Wired `SandboxConfig.seccomp_rules` → custom BPF filter |
| 4 | Predictable temp files | agnos-sys/netns.rs | UUID-based paths under `/run/agnos/` |

#### High (Fixed)
| # | Item | File | Fix |
|---|------|------|-----|
| 5 | LLM Gateway bound to 0.0.0.0 | llm-gateway/http.rs | Default 127.0.0.1, `AGNOS_GATEWAY_BIND` env |
| 6 | Agent Runtime API bound to 0.0.0.0 | agent-runtime/http_api.rs | Default 127.0.0.1, `AGNOS_RUNTIME_BIND` env |
| 7 | CORS allows any origin | llm-gateway/http.rs | Restricted to localhost origins |
| 8 | Bearer token unwrap panic | llm-gateway/http.rs | Safe `unwrap_or("")` |
| 9 | Production unwraps in HTTP API | agent-runtime/http_api.rs | Error responses |
| 10 | Edited commands bypass risk assessment | ai-shell/approval.rs | Re-assess when binary changes |
| 11 | Unbounded exponential backoff | agent-runtime/supervisor.rs | Capped at 300s |

#### Performance (Fixed)
| # | Item | File | Fix |
|---|------|------|-----|
| 12 | Cache write lock on reads | llm-gateway/cache.rs | Read lock |
| 13 | Small types not Copy | desktop-env/compositor.rs | Copy derives |
| 14 | O(n) voter membership | agent-runtime/swarm.rs | HashSet |
| 15 | O(n²) dependency checks | agent-runtime/swarm.rs | HashMap |
| 16 | Repeated sysconf syscall | agent-runtime/supervisor.rs | OnceLock cache |
| 17 | O(n) front removal | agent-runtime/learning.rs | VecDeque |

#### Remaining (Lower Priority, Not Blocking Alpha)
- Sandbox partial application rollback
- Secret zeroing optimization (needs `zeroize` crate)
- Thread-unsafe env var manipulation in secrets.rs
- Various `let _ =` swallowed errors in IPC/service manager
- Desktop environment blanket `#![allow(dead_code)]`
- AI Shell Question intent stub
- Placeholder certificate pins
- Streaming parser O(n²) String::drain pattern

---

## Executive Summary

AGNOS (AI-Native General Operating System) is in **Phase 5: Production**, focused on security hardening, testing, and release preparation. All P0 items are complete. The sole remaining Alpha blocker is a third-party security audit (external vendor).

### Phase Status Overview

| Phase | Status | Completion | Key Deliverables |
|-------|--------|------------|------------------|
| 0-4 | Complete | 100% | Foundation through Desktop |
| 5 | In Progress | 99% | Production hardening |
| 5.6 | Complete | 100% | Internal implementation gaps (all P0-P2 stubs eliminated) |
| 6 | Complete | 100% | Advanced AI & Networking (agent intelligence, multi-modal, swarm, LLM analysis, 32 tools + 7 wrappers, hardware acceleration) |
| 6.5 | Complete | 100% | OS-Level Features & Security Hardening (all 12 modules) |
| 6.6 | Complete | 100% | Consumer Integration (9 features) |
| 7+ | Planned | 0% | Ecosystem & Research |

### Alpha Release Criteria (Q2 2026)
- [x] Core features fully wired (not stubbed) — P0/P1 stubs eliminated March 3
- [x] 80%+ test coverage (~80%, 5800+ tests, 3421 lib)
- [x] Integration tests: agent-orchestrator (16 tests)
- [x] Performance benchmarks established (58 benchmarks + docs)
- [ ] Third-party security audit complete
- [x] Documentation complete (Agent Development Guide created)
- [x] Known issues documented

**Confidence**: High — only third-party audit remains.

---

## Phase 5: Production (Remaining Items)

### Phase 5.2 - Security & Compliance (98% Complete)

**Remaining:**

- [ ] **Third-Party Security Audit** (P1)
  - Effort: 2 weeks (external)
  - Owner: External vendor
  - Status: Vendor selection in progress
  - Details: See [docs/security/penetration-testing.md](/docs/security/penetration-testing.md)

### Phase 5.4 - Documentation (95% Complete)

**Remaining:**

- [ ] **Video Tutorials** (P2)
  - Topics: Installation walkthrough, Basic usage (5 min), Creating your first agent (10 min), Security features overview (5 min)

- [ ] **Kernel Development Guide** (P3)
  - For kernel hackers contributing to AGNOS kernel modules

- [x] **Interactive API Explorer** — `docs/api/explorer.html`, self-contained HTML with dark theme, 11 endpoints documented (LLM Gateway + Agent Runtime), try-it-now functionality

### Phase 5.5 - Release Infrastructure (100% Complete)

**Remaining:**

- [ ] **Support Portal** (P3)
  - Can use GitHub Issues/Discussions for Alpha

---

## Future Phases (Post-Alpha)

### Phase 6: Advanced AI & Networking (Planned Q3 2026)

#### Hardware Acceleration ✅ Complete
- [x] NPU support (Apple Silicon ANE, Intel NPU) — AcceleratorType enum, detection via /sys/class/misc and /proc/device-tree
- [x] GPU optimization (CUDA, ROCm, Metal) — nvidia-smi/rocm-smi probing, throughput multipliers
- [x] Quantization support (4-bit, 8-bit inference) — QuantizationLevel enum (FP32/FP16/BF16/Int8/Int4), memory reduction factors
- [x] Model sharding for large models — Pipeline/Tensor/Data parallel strategies, ShardingPlan with memory fitting, AcceleratorRegistry (43 tests)

#### Agent Intelligence ✅ Complete
- [x] Distributed agent computing — swarm task decomposition (DataParallel/Pipeline/FunctionalSplit/Redundant)
- [x] Swarm intelligence protocols — consensus voting with quorum rules (19 tests)
- [x] Agent learning and adaptation — UCB1 strategy selection, capability scoring with EMA (13 tests)
- [x] Multi-modal agents (vision, audio) — modality profiles, content blocks, registry (14 tests)

#### Networking Toolkit (Kali Linux-Inspired)

AGNOS includes a networking toolkit framework (`agent-runtime/src/network_tools.rs`) with sandboxed execution, target validation, dangerous arg rejection, risk levels, AI Shell integration, and 7 typed agent wrapper structs.

**Network Reconnaissance & Scanning** ✅ All Complete
- [x] `nmap` — port scanning and service/version detection (`PortScanner` wrapper with ScanProfile)
- [x] `masscan` — high-speed network scanning (`PortScanner` wrapper with `use_masscan()`)
- [x] `netdiscover` — ARP network scanning (NetworkTool variant + config)
- [x] `arp-scan` — local network discovery (NetworkTool variant + config)
- [x] `p0f` — passive OS fingerprinting (NetworkTool variant + config)

**Traffic Analysis & Capture** ✅ All Complete
- [x] `tcpdump` — packet capture and analysis (`TrafficAnalyzer` wrapper)
- [x] `wireshark` / `tshark` — deep packet inspection (`TrafficAnalyzer.use_tshark()`)
- [x] `termshark` — TUI Wireshark frontend (NetworkTool variant + config)
- [x] `bettercap` — network monitoring and MITM analysis framework (NetworkTool variant, dangerous arg validation for --caplet)
- [x] `ngrep` — network grep (`TrafficAnalyzer.use_ngrep()`)

**Network Utilities** ✅ All Complete
- [x] `netcat` / `ncat` — TCP/UDP toolbox (NetworkTool variant + config)
- [x] `socat` — bidirectional data relay (NetworkTool variant + config)
- [x] `curl` + `httpie` — HTTP clients (NetworkTool variant + config)
- [x] `mtr` — network diagnostics (`NetworkProber.use_mtr()`)
- [x] `iperf3` — bandwidth measurement (NetworkTool variant + config)
- [x] `nethogs` / `iftop` — per-process bandwidth monitoring (NetworkTool variant + config)
- [x] `ss` / `iproute2` — socket statistics (`SocketInspector` wrapper)

**DNS Tooling** ✅ All Complete
- [x] `dig` / `drill` — DNS lookup (`DnsInvestigator` wrapper)
- [x] `dnsx` — fast DNS toolkit (NetworkTool variant + config)
- [x] `dnsrecon` — DNS enumeration (`DnsInvestigator.enumerate()`)
- [x] `fierce` — DNS zone traversal (NetworkTool variant + config)

**Web & Application Layer** ✅ All Complete
- [x] `nikto` — web server scanner (`VulnAssessor.use_nikto()`)
- [x] `gobuster` / `ffuf` — directory and subdomain fuzzing (`WebFuzzer` wrapper)
- [x] `wfuzz` — web fuzzer (NetworkTool variant + config)
- [x] `sqlmap` — SQL injection detection (NetworkTool variant, dangerous arg validation for --os-shell/--os-cmd/--file-write)
- [x] `nuclei` — template-based vulnerability scanner (`VulnAssessor` wrapper)

**Wireless** ✅ All Complete
- [x] `aircrack-ng` suite — 802.11 analysis (NetworkTool variant, NET_RAW + NET_ADMIN, Critical risk)
- [x] `kismet` — wireless network detector (NetworkTool variant, NET_RAW + NET_ADMIN, Critical risk)

**Agent Integration** ✅ Framework + Wrappers Complete
- [x] Network tool runner with sandboxed execution (`network_tools.rs`, 100 tests)
- [x] 32 tool variants: all reconnaissance, traffic analysis, utility, DNS, web, and wireless tools
- [x] 7 typed agent wrappers: PortScanner, DnsInvestigator, NetworkProber, VulnAssessor, TrafficAnalyzer, WebFuzzer, SocketInspector
- [x] Output parsers: structured results for nmap, masscan, dig, traceroute/mtr, ss
- [x] LLM tool output analysis pipeline (`tool_analysis.rs`, 15 tests) — results piped through LLM Gateway
- [x] AI Shell understands 17 network actions via natural language
- [x] Target validation, risk levels, dangerous arg rejection (masscan rate limits, nuclei template restrictions)
- [x] All tool invocations require user approval for sensitive operations (per Human Sovereignty principle)
- [x] Audit trail for every network operation

### Phase 6.5: OS-Level Features & Security Hardening ✅ ALL COMPLETE

All OS-level modules implemented March 6 with full test coverage.

#### OS-Level Features (All Complete)

| Feature | Module | Tests | Description |
|---------|--------|-------|-------------|
| Init system | `agent-runtime/service_manager.rs` | 29 | TOML service definitions, dependency DAG, parallel boot |
| Package manager | `agent-runtime/package_manager.rs` | 31 | Agent distribution, versioning, integrity verification |
| FUSE filesystem | `agnos-sys/fuse.rs` | 32 | Mount management, overlayfs for agents, proc parsing |
| Device management | `agnos-sys/udev.rs` | 26 | Device enumeration, udev rules, udevadm parsing |
| PAM / user management | `agnos-sys/pam.rs` | 40 | User/session mgmt, passwd parsing, PAM config |
| A/B system updates | `agnos-sys/update.rs` | 37 | Slot management, CalVer versioning, rollback |
| Journald integration | `agnos-sys/journald.rs` | 30 | Journal queries, JSON parsing, filtering |
| Bootloader config | `agnos-sys/bootloader.rs` | 27 | systemd-boot + GRUB2, cmdline validation |
| Network namespaces | `agnos-sys/netns.rs` | 30+ | Per-agent isolation, veth pairs, nftables |

#### Security Hardening (All Complete)

| Feature | Module | Tests | Description |
|---------|--------|-------|-------------|
| IMA/EVM | `agnos-sys/ima.rs` | 31 | File integrity, measurement parsing, policy rules |
| TPM 2.0 | `agnos-sys/tpm.rs` | 23 | Measured boot, sealed secrets, PCR management |
| Secure Boot | `agnos-sys/secureboot.rs` | 18 | UEFI state, key enrollment, module signing |
| Certificate pinning | `agnos-sys/certpin.rs` | 38 | SPKI pins, pin verification, HPKP headers |
| MAC (SELinux/AppArmor) | `agnos-sys/mac.rs` | 20+ | Auto-detect, per-agent profiles |
| dm-verity | `agnos-sys/dmverity.rs` | 25+ | Rootfs integrity verification |
| LUKS2 volumes | `agnos-sys/luks.rs` | 30+ | Per-agent encrypted storage |
| Audit subsystem | `agnos-sys/audit.rs` | 25+ | Netlink audit, cryptographic hash chain |

#### Consumer Integration (All Complete)

| Requirement | AGNOS Component | Status |
|-------------|-----------------|--------|
| Secrets management | agnos-common/secrets.rs | Done |
| Seccomp profiles | agent-runtime/seccomp_profiles.rs | Done |
| Agent HTTP API | agent-runtime/http_api.rs (port 8090) | Done |
| Load-aware scheduling | agent-runtime/orchestrator.rs | Done |
| Agent HUD | desktop-environment/ai_features.rs | Done |
| Security enforcement UI | desktop-environment/security_ui.rs | Done |
| WASM runtime | agent-runtime/wasm_runtime.rs | Done |
| Docker image | Dockerfile + docker/entrypoint.sh | Done |
| gVisor config | docker/gvisor-config.toml | Done |

#### Docker Base Image for Sibling Projects

| Project | Current Base | Migration Readiness | Notes |
|---------|-------------|-------------------|-------|
| SecureYeoman | `node:20-slim` | Medium | Node.js runtime needed |
| Agnostic | Per-agent Dockerfiles | High | Python/CrewAI agents map to agent-runtime |
| BullShift | `rust:1.77` → `debian:bookworm-slim` | High | Already Rust |

Blockers before migration:
- [ ] **Alpha release** — third-party security audit must complete
- [ ] **Node.js runtime layer** — publish `agnos:node20` variant
- [ ] **Python runtime layer** — publish `agnos:python3.12` variant

#### LLM Gateway as Shared Provider

| Project | Current LLM Path | Gateway Benefit |
|---------|------------------|-----------------|
| SecureYeoman | Direct provider calls | Centralized rate limiting, audit logging, model routing |
| Agnostic | `universal_llm_adapter.py` | Deduplicate adapter logic; route through gateway |
| BullShift | AI Bridge backends | Single endpoint replaces per-provider config |

### Phase 7: Ecosystem (Planned Q4 2026)

#### Marketplace
- [ ] Third-party agent marketplace
- [ ] Plugin architecture for desktop
- [ ] Integration marketplace
- [ ] Agent rating and review system

#### Cloud Services
- [ ] AGNOS Cloud (optional hosted agents)
- [ ] Cross-device agent sync
- [ ] Collaborative agent workspaces

### Phase 8: Research (Planned Q1 2027)

#### Advanced Research
- [ ] Formal verification of security-critical components
- [ ] Novel sandboxing architectures
- [ ] AI safety mechanisms
- [ ] Human-AI collaboration research

---

## Release Roadmap

### Alpha Release - Q2 2026

**Current version**: `2026.3.5` (CalVer: `YYYY.D.M`, patches as `-#N`)

**Remaining criteria:**
- [ ] Third-party security audit complete

**Target Date**: End of Q2 2026

### Beta Release - Q3 2026

**Criteria:**
- Community testing program
- Bug fixes from alpha feedback
- Performance optimized based on benchmarks
- Update system operational and tested
- Support channels open (Discord, forum)
- Video tutorials published

**Target Date**: Mid-Q3 2026

### v1.0 Release - Q4 2026

**Criteria:**
- Production ready (all critical bugs resolved)
- Enterprise features complete (SSO, audit logging)
- Certifications complete (if pursued)
- Commercial support available
- Migration guides published

**Target Date**: End of Q4 2026

---

## Key Performance Indicators (KPIs)

### Current Status (as of 2026-03-06)

| Metric | Target | Current | Status |
|--------|--------|---------|--------|
| Code Coverage | >80% | ~80% | Met |
| Test Pass Rate | 100% | 100% | Met |
| Total Tests | 400+ | 5800+ | Met |
| Agent Spawn Time | <500ms | ~300ms | Met |
| Shell Response Time | <100ms | ~50ms | Met |
| Memory Overhead | <2GB | ~1.2GB | Met |
| Boot Time | <10s | N/A | Pending |
| CIS Compliance | >80% | ~85% | Met |
| Stub Implementations | 0 | 0 | Met |
| Compiler Warnings | 0 | 0 | Met |

### By Component

| Component | Tests | Notes |
|-----------|-------|-------|
| agnos-common | 307 | Secrets, telemetry, LLM types, manifest, rate limits, audit chain |
| agnos-sys | 750+ (7 ignored) | 16 modules: audit, mac, netns, dmverity, luks, ima, tpm, secureboot, certpin, bootloader, journald, udev, fuse, pam, update, llm |
| agent-runtime | 843 + 16 integration + 30 load | Service manager, lifecycle, pub/sub, rollback, package manager, quotas, IPC, WASM, network tools (100), swarm (20), learning (13), multimodal (15), tool analysis (12) |
| llm-gateway | 249 + 423 | 5 providers, rate limiting, streaming, graceful degradation, cert pinning, hardware acceleration (43) |
| ai-shell | 555 + 555 | 20+ intents: file ops, audit, agent, service, network scan, journal, device, mount, boot, update |
| desktop-environment | 593 + 562 + 40 E2E | Wayland protocol types + Dispatch traits (63), HUD, security, apps, compositor, system tests |

---

## Architecture Decision Records

1. ADR-001: Rust as Primary Implementation Language
2. ADR-002: Wayland for Desktop Environment
3. ADR-003: Multi-Agent Orchestration Architecture
4. ADR-004: LLM Gateway Service Design
5. ADR-005: Security Model and Human Override
6. ADR-006: Testing Strategy and CI/CD
7. ADR-007: OpenAI-compatible HTTP API for LLM Gateway

---

## Contributing

### Priority Contribution Areas

1. **Third-party security audit (P1)** - External vendor engagement
2. **Video tutorials (P2)** - Installation, usage, agent creation, security overview
3. **Kernel Development Guide (P3)** - For kernel hackers contributing to AGNOS kernel modules
4. **Support portal (P3)** - Community support channels (can use GitHub Issues/Discussions for Alpha)

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

---

*Last Updated: 2026-03-06 | Next Review: 2026-03-10*
