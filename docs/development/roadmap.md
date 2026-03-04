# AGNOS Development Roadmap

> **Status**: Pre-Alpha (Phase 5) | **Last Updated**: 2026-03-04
> **Current Phase**: Phase 5 - Production (95% Complete — consumer integration features implemented)
> **Next Milestone**: Alpha Release (Target: Q2 2026)

---

## Quick Reference: Remaining Work

### P0 - Critical (Must Complete for Alpha)
| Item | Component | Effort | Owner | Status |
|------|-----------|--------|-------|--------|
| ~~Sandbox enforcement (Landlock/seccomp)~~ | agent-runtime, ai-shell | — | — | ✅ Done 2026-03-03 |
| ~~Agent health checks (real implementation)~~ | agent-runtime | — | — | ✅ Done 2026-03-03 |
| ~~Fix panicking unwrap() in production code~~ | llm-gateway, desktop-env | — | — | ✅ Done 2026-03-03 |
| ~~Input validation enforcement (LLM params)~~ | agnos-common | — | — | ✅ Done 2026-03-03 |
| ~~Resource limit enforcement (cgroups)~~ | agent-runtime | — | — | ✅ Done 2026-03-03 |
| Unit test coverage 65% → 80% | All | 1 week | TBD | 🔄 62% (was 46%, +876 tests on 2026-03-04) |
| CIS benchmarks 75% → 80% | Security | 3 days | TBD | 🔄 8 controls remain |

### P1 - High Priority (Alpha Blockers)
| Item | Component | Effort | Owner | Status |
|------|-----------|--------|-------|--------|
| ~~Wire CLI subcommands to real logic~~ | agent-runtime, llm-gateway | — | — | ✅ Done 2026-03-03 |
| ~~LLM integration for ai-shell~~ | ai-shell | — | — | ✅ Done 2026-03-03 |
| ~~Agent restart on failure~~ | agent-runtime | — | — | ✅ Done 2026-03-03 |
| ~~Task dependency resolution~~ | agent-runtime | — | — | ✅ Done 2026-03-03 |
| ~~Telemetry system info (real values)~~ | agnos-common | — | — | ✅ Done 2026-03-03 |
| ~~Desktop terminal + system status~~ | desktop-environment | — | — | ✅ Done 2026-03-03 |
| Performance benchmark docs + system-level benches | All | 1 week | TBD | 🔄 Micro-benchmarks done |
| Third-party security audit | Security | 2 weeks | External | ⏳ Vendor selection |

### P2 - Medium Priority (Alpha Polish)
| Item | Component | Effort | Owner | Status |
|------|-----------|--------|-------|--------|
| Streaming support for LLM providers | llm-gateway | 3 days | TBD | ⏳ Trait only |
| CI coverage gate (fail below threshold) | CI/CD | 1 day | TBD | ⏳ |
| Structured logging (JSON format) | All | 2-3 days | TBD | ⏳ Text fmt only |
| Cloud provider implementations (OpenAI, Anthropic) | llm-gateway | 3 days | TBD | ⏳ Enum only |
| Cloud provider graceful degradation | llm-gateway | 2 days | TBD | ⏳ No health/retry |
| Rate limiting on external APIs (per-agent/provider) | llm-gateway | 2 days | TBD | ⏳ Semaphore only |
| Agent resource quotas (cgroups) | agent-runtime | 1 week | TBD | ⏳ In-memory only |
| ~~Capability-based task distribution~~ | agent-runtime | — | — | ✅ Done 2026-03-03 (load-aware scoring) |
| Connection backpressure for IPC | agent-runtime | 2 days | TBD | ⏳ |
| Metric dashboards (latency, cache, etc.) | llm-gateway | 3 days | TBD | ⏳ |
| System tests: end-to-end desktop | desktop-environment | 1 week | TBD | ⏳ |
| Load testing: multi-agent stress | agent-runtime | 3 days | TBD | ⏳ |
| Fuzz targets: agent-runtime + llm-gateway | Testing | 3 days | TBD | ⏳ |
| Remaining ai-shell intent translation (8 intents) | ai-shell | 3 days | TBD | ⏳ |
| Output formatting (table, auto) | ai-shell | 1 day | TBD | ⏳ Returns input unchanged |
| Video tutorials | Documentation | 3 days | TBD | ⏳ |

### P3 - Lower Priority (Beta/Post-Alpha)
| Item | Component | Effort | Owner | Status |
|------|-----------|--------|-------|--------|
| Kernel Development Guide | Documentation | 3 days | TBD | ⏳ |
| Support portal | Infrastructure | 2 weeks | TBD | ⏳ |
| Interactive API explorer | Documentation | 1 week | TBD | ⏳ |
| Wayland compositor rendering + input | desktop-environment | 2+ weeks | TBD | ⏳ Full stub |
| ~~Agent pause/resume (real process control)~~ | agent-runtime | — | — | ✅ Done (SIGSTOP/SIGCONT) |
| Dead code cleanup (unused traits, imports) | All | 1 day | TBD | ⏳ |

---

## Executive Summary

AGNOS (AI-Native General Operating System) is in **Phase 5: Production**, focused on security hardening, testing, and release preparation. A March 3 internal audit discovered 30+ stub implementations; two same-day passes eliminated **all P0/P1 stubs** (91%). Phase 6.6 consumer integration then added 9 features (secrets, seccomp profiles, HTTP registration API, load-aware scheduler, HUD, security enforcement, WASM runtime, Docker image, gVisor), bringing completion to **95%**. Remaining Alpha blockers: test coverage (65%→80%), CIS compliance (75%→80%), performance benchmarks, and a third-party security audit.

### Phase Status Overview

| Phase | Status | Completion | Key Deliverables |
|-------|--------|------------|------------------|
| 0-4 | ✅ Complete | 90-100% | Foundation through Desktop |
| 5 | 🔄 In Progress | 95% | Production hardening |
| 6 | 📋 Planned | 0% | Advanced AI & Networking |
| 6.5 | 📋 Planned | 0% | OS-Level Features & Security Hardening |
| 6.6 | ✅ Complete | 100% | Consumer Integration (9 features) |
| 7+ | 📋 Planned | 0% | Ecosystem & Research |

### Alpha Release Criteria (Q2 2026)
- [x] Core features fully wired (not stubbed) — **P0/P1 stubs eliminated March 3**
- [ ] 80%+ test coverage (currently ~65%)
- [x] Integration tests: agent-orchestrator (16 tests added)
- [ ] Performance benchmarks established (micro-benchmarks done, system-level + docs needed)
- [ ] Third-party security audit complete
- [x] Documentation complete (Agent Development Guide created)
- [x] Known issues documented — **this document now serves as the known issues list**

---

## Phase 5: Production

### Phase 5.0 - Foundation (✅ COMPLETE)
**Completion: 100%**

All foundational work is complete. See [CHANGELOG.md](/CHANGELOG.md) for detailed history.

### Phase 5.1 - Core Infrastructure (✅ 100% Complete)
**All P0/P1 stubs eliminated in March 3 implementation passes**

#### ✅ Completed (verified working)
- Agent SDK with message loop (`agnos-sys/src/agent.rs`)
- LLM Gateway HTTP API (OpenAI-compatible, port 8088)
- IPC routing by agent name (`agent-runtime/src/ipc.rs`)
- Landlock/seccomp implementation in `agnos-sys/src/security.rs` (real syscalls)
- **Sandbox enforcement in agent-runtime** — wired to `agnos-sys::security` (Landlock + seccomp + namespace isolation) *(fixed 2026-03-03)*
- **Sandbox enforcement in ai-shell** — wired to `agnos-sys::security` with sensible shell defaults *(fixed 2026-03-03)*
- **Agent lifecycle CLI** — all 5 subcommands wired to Registry/IPC/Orchestrator *(fixed 2026-03-03)*
- **LLM gateway CLI** — all 5 subcommands wired to HTTP API on port 8088 *(fixed 2026-03-03)*
- **ai-shell LLM integration** — connected to LLM Gateway HTTP API with graceful fallback *(fixed 2026-03-03)*

#### ✅ Previously Remaining Stubs — All Resolved
- ~~**LLM syscall stubs** (`agnos-sys/src/llm.rs`)~~: Now delegates to LLM Gateway HTTP API with handle tracking *(fixed 2026-03-03)*
- ~~**Audit logging** (`agnos-sys/src/agent.rs`)~~: Now writes JSON lines to `/var/log/agnos/audit.log` with SHA-256 hash chain *(fixed 2026-03-03)*

### Phase 5.2 - Security & Compliance (🔄 75% Complete)

#### ✅ Completed
- Fuzzing infrastructure (daily automated runs)
- SBOM generation (SPDX & CycloneDX)
- CIS benchmarks validation scripts (20+ new controls added March 2026)
- Dependency vulnerability scanning (cargo-deny, cargo-outdated)

#### ⏳ Remaining

##### P0 - Critical
- [ ] **CIS Benchmarks: 75% → 80% Compliance**
  - Effort: 3 days
  - Owner: TBD
  - Status: 8 controls remaining — mostly kernel config + sysctl changes
  - Quick wins: disable USB_STORAGE/FIREWIRE/THUNDERBOLT, source route rejection, /tmp sticky bit
  - Details: See [docs/security/cis-benchmarks.md](/docs/security/cis-benchmarks.md)

- [x] **Fix panicking unwrap() in production code** *(completed 2026-03-03)*
  - Fixed 6 locations: `llm-gateway/src/http.rs` (SystemTime), `desktop-environment/src/shell.rs` + `ai_features.rs` (NaN partial_cmp), `agnos-sys/src/agent.rs` + `agnos-common/src/telemetry.rs` (.expect → unwrap_or_else)

- [x] **Input validation enforcement** *(completed 2026-03-03)*
  - Added `InferenceRequest::new()` constructor that auto-validates
  - Added `request.validate()` call at start of `LlmGateway::infer()`

##### P1 - High Priority
- [ ] **Third-Party Security Audit**
  - Effort: 2 weeks (external)
  - Owner: External vendor
  - Status: Vendor selection in progress
  - Details: See [docs/security/penetration-testing.md](/docs/security/penetration-testing.md)

### Phase 5.3 - Testing & Quality (🔄 75% Complete)

#### ✅ Completed
- Unit test framework (cargo test)
- ~62% test coverage (tarpaulin), up from ~46% on 2026-03-04 (+876 tests)
- agnos-common: 168 tests passing (up from 104)
- ai-shell: 319 tests passing (up from 185)
- agent-runtime: 239 unit + 16 integration tests passing (up from 116+16)
- llm-gateway: 145 tests passing (up from 47)
- agnos-sys: 125 tests passing
- desktop-environment: 268 tests passing (up from 161)
- Total: 2091 tests across all packages (1215 → 2091, 0 failures, 8 ignored requiring root)
- Performance micro-benchmarks: 29 benchmarks across 4 packages (Criterion)
  - `agent-runtime/benches/bench.rs` — 12 benchmarks
  - `ai-shell/benches/ai_shell.rs` — 7 benchmarks
  - `llm-gateway/benches/llm_gateway.rs` — 5 benchmarks
  - `agnos-common/benches/agnos_common.rs` — 5 benchmarks
- Fixed 4 compilation errors in test code (missing tempfile dep, missing enum variants, wrong test assertions)

#### ⏳ Remaining

##### P0 - Critical (Alpha Blockers)
- [ ] **Unit Test Coverage: 62% → 80%**
  - Effort: 1 week (was 2 weeks, reduced by March 4 push)
  - Owner: TBD
  - Priority components:
    1. agnos-sys (needs root-gated tests for audit/LUKS/netns/dmverity)
    2. agent-runtime (supervisor process management, sandbox apply paths)
    3. llm-gateway (HTTP handler integration tests)

##### P1 - High Priority
- [ ] **Performance Benchmarks: System-Level + Documentation**
  - Effort: 1 week
  - Owner: TBD
  - What exists: 29 micro-benchmarks (unit-level operations)
  - What's missing:
    - System-level benchmarks (agent spawn lifecycle, IPC round-trip, LLM e2e)
    - Boot time measurement (currently "N/A")
    - Performance documentation (`docs/development/performance-benchmarks.md`)
    - CI integration for regression tracking
  - Metrics to establish:
    - Boot time (<10 seconds)
    - Agent spawn time (<500ms, currently ~300ms)
    - Shell response time (<100ms, currently ~50ms)
    - Memory overhead (<2GB, currently ~1.2GB)

- [ ] **Load Testing: Multi-Agent Stress**
  - Effort: 3 days
  - Owner: TBD
  - Scenarios:
    - 100 concurrent agents
    - Memory pressure testing
    - CPU saturation handling
    - IPC throughput limits

##### P2 - Medium Priority
- [ ] **System Tests: End-to-End Desktop**
  - Effort: 1 week
  - Owner: TBD
  - Coverage: Desktop environment, agent HUD, security UI integration

### Phase 5.4 - Documentation (✅ 95% Complete)

#### ✅ Completed
- README.md, CONTRIBUTING.md, SECURITY.md
- ARCHITECTURE.md, AGENT_RUNTIME.md, DESKTOP_ENVIRONMENT.md
- API documentation and examples
- ADR-001 through ADR-007
- Testing guide, Security guide, CIS benchmarks
- Troubleshooting guide
- **Agent Development Guide** (`docs/development/agent-development.md`) — completed March 2026

#### ⏳ Remaining

##### P2 - Medium Priority
- [ ] **Video Tutorials**
  - Effort: 3 days
  - Owner: TBD
  - Topics:
    - Installation walkthrough
    - Basic usage (5 min)
    - Creating your first agent (10 min)
    - Security features overview (5 min)

##### P3 - Lower Priority
- [ ] **Kernel Development Guide**
  - Effort: 3 days
  - Owner: TBD
  - Scope: For kernel hackers contributing to AGNOS kernel modules

- [ ] **Interactive API Explorer**
  - Effort: 1 week
  - Owner: TBD
  - Scope: Web-based API documentation with try-it-now functionality

### Phase 5.5 - Release Infrastructure (✅ COMPLETE)
**Completion: 100%**

#### ✅ Completed
- Package signing with GPG (`scripts/sign-packages.sh`)
- Delta update system with rollback (`scripts/agnos-update.sh`)
- Release automation (`.github/workflows/release-automation.yml`)
- Telemetry system - opt-in (`agnos-common/src/telemetry.rs`)

#### ⏳ Remaining

##### P3 - Lower Priority
- [ ] **Support Portal**
  - Effort: 2 weeks
  - Owner: TBD
  - Scope: Issue tracking and community forums
  - Note: Can use GitHub Issues/Discussions for Alpha

### Phase 5.6 - Internal Implementation Gaps (identified March 3, 2026 audit)
**Completion: 100% (all P0 and P1 stubs eliminated)**

These are features where the public API/interface exists but the implementation behind it is a stub, returns fake data, or is disconnected from the actual system. This phase must be substantially complete before Alpha.

#### P0 — Security-Critical Stubs

| Gap | Component | Status |
|-----|-----------|--------|
| ~~Sandbox: agent-runtime~~ | agent-runtime/src/sandbox.rs | ✅ Wired to `agnos-sys::security` (Landlock + seccomp + namespaces) |
| ~~Sandbox: ai-shell~~ | ai-shell/src/sandbox.rs | ✅ Wired to `agnos-sys::security` with shell defaults |
| ~~Health monitoring~~ | agent-runtime/src/supervisor.rs | ✅ Process liveness (`kill(pid, 0)`) + IPC socket connectivity |
| ~~Agent restart on failure~~ | agent-runtime/src/supervisor.rs | ✅ Exponential backoff (2^n sec, max 5 attempts) |
| ~~Resource enforcement (cgroups)~~ | agent-runtime/src/supervisor.rs | ✅ CgroupController writes memory.max/cpu.max, adds PID to cgroup.procs, reads usage from counters |

#### P1 — Functionality Stubs

| Gap | Component | Status |
|-----|-----------|--------|
| ~~Agent CLI commands~~ | agent-runtime/src/main.rs | ✅ 5 commands wired to Registry/IPC |
| ~~LLM gateway CLI commands~~ | llm-gateway/src/main.rs | ✅ 5 commands wired to HTTP API |
| ~~LLM integration (ai-shell)~~ | ai-shell/src/llm.rs | ✅ Connected to LLM Gateway with graceful fallback |
| ~~Task dependency resolution~~ | agent-runtime/src/orchestrator.rs | ✅ Scheduler checks deps before scheduling |
| ~~Telemetry system info~~ | agnos-common/src/telemetry.rs | ✅ Reads /etc/os-release, /proc/meminfo, /proc/version |
| ~~Agent resource monitoring~~ | agent-runtime/src/agent.rs | ✅ Reads VmRSS, CPU time, FD count, thread count from /proc/{pid}/ |
| ~~Audit logging (agent SDK)~~ | agnos-sys/src/agent.rs | ✅ JSON lines to /var/log/agnos/audit.log with SHA-256 hash chain |
| ~~LLM syscall stubs~~ | agnos-sys/src/llm.rs | ✅ Delegates to LLM Gateway HTTP API with handle tracking |

#### P1 — Desktop Stubs

| Gap | Component | Status |
|-----|-----------|--------|
| ~~Terminal command execution~~ | desktop-env/src/apps.rs | ✅ Uses `tokio::process::Command` with stdout/stderr capture |
| ~~System status~~ | desktop-env/src/main.rs | ✅ Reads /proc/stat, /proc/meminfo, libc::statvfs |
| ~~Agent manager app~~ | desktop-env/src/apps.rs | ✅ Scans /run/agnos/agents/ sockets, probes connectivity |
| ~~Audit viewer app~~ | desktop-env/src/apps.rs | ✅ Reads /var/log/agnos/audit.log with time/category filters |
| ~~Model manager app~~ | desktop-env/src/apps.rs | ✅ Queries LLM Gateway /v1/models, Ollama /api/pull |

#### P2 — Incomplete Features

| Gap | Current Behavior | Fix Required | Component | Effort |
|-----|-----------------|--------------|-----------|--------|
| Intent translation | 5 of 13 intents translate, 8 return error | Implement remaining: FindFiles, SearchContent, Remove, KillProcess, NetworkInfo, DiskUsage, InstallPackage, Ambiguous | ai-shell/src/interpreter.rs | 3 days |
| Output formatting | `format_table()` + `format_auto()` return input unchanged | Implement column detection + alignment | ai-shell/src/output.rs | 1 day |
| Streaming inference | Both providers return empty `mpsc::Receiver` | Implement SSE for Ollama + llama.cpp | llm-gateway/src/providers.rs | 3 days |
| Cloud LLM providers | `ProviderType::OpenAi/Anthropic/Google` enum exists | Implement actual HTTP clients | llm-gateway/src/providers.rs | 3 days |
| ~~Load balancing~~ | ~~Round-robin only~~ | ~~Implement capability/load-aware assignment~~ | ~~agent-runtime/src/orchestrator.rs~~ | ✅ Done (load-aware scoring) |

#### P3 — Low Priority

| Gap | Current Behavior | Fix Required | Component | Effort |
|-----|-----------------|--------------|-----------|--------|
| Wayland compositor | `render()` empty, `handle_input()` logs only | Full Wayland protocol implementation | desktop-env/src/compositor.rs | 2+ weeks |
| ~~Agent pause/resume~~ | ~~Sets enum status, no process control~~ | ~~`SIGSTOP`/`SIGCONT`~~ | ~~agent-runtime/src/agent.rs~~ | ✅ Done |
| `AgentControl` trait | Defined but never implemented | Implement on Agent type or remove | agent-runtime/src/supervisor.rs | 1 day |
| Prompt right-side | `render_right()` returns `None` | Implement time/status display | ai-shell/src/prompt.rs | 1 day |
| Feature flags | `wayland`, `ai` defined but gate nothing | Wire to `cfg` attributes or remove | desktop-env/Cargo.toml | 1 day |
| Dead code cleanup | Unused traits, imports, cache/accounting fns | Remove or expose via endpoints | All | 1 day |
| GPU vendor detection | Only detects NVIDIA | Add AMD, Intel detection | agent-runtime/src/resource.rs | 1 day |
| Redundant security wrappers | `create_landlock_ruleset()` + `restrict_filesystem()` duplicate `apply_landlock()` | Consolidate or remove | agnos-sys/src/security.rs | 1 day |

---

## Future Phases (Post-Alpha)

### Phase 6: Advanced AI & Networking (Planned Q3 2026)

#### Hardware Acceleration
- [ ] NPU support (Apple Silicon, Intel NPU)
- [ ] GPU optimization (CUDA, ROCm, Metal)
- [ ] Quantization support (4-bit, 8-bit inference)
- [ ] Model sharding for large models

#### Agent Intelligence
- [ ] Distributed agent computing
- [ ] Swarm intelligence protocols
- [ ] Agent learning and adaptation
- [ ] Multi-modal agents (vision, audio)

#### Networking Toolkit (Kali Linux-Inspired)

AGNOS will include a comprehensive, curated networking toolkit inspired by Kali Linux, pre-configured for agent-driven analysis and automation. All tools operate within the AGNOS sandbox and audit framework — every invocation is logged to the cryptographic audit chain.

**Network Reconnaissance & Scanning**
- [ ] `nmap` — port scanning and service/version detection
- [ ] `masscan` — high-speed network scanning
- [ ] `netdiscover` — ARP network scanning
- [ ] `arp-scan` — local network discovery
- [ ] `p0f` — passive OS fingerprinting

**Traffic Analysis & Capture**
- [ ] `tcpdump` — packet capture and analysis
- [ ] `wireshark` / `tshark` — deep packet inspection
- [ ] `termshark` — TUI Wireshark frontend
- [ ] `bettercap` — network monitoring and MITM analysis framework
- [ ] `ngrep` — network grep

**Network Utilities**
- [ ] `netcat` / `ncat` — TCP/UDP toolbox
- [ ] `socat` — bidirectional data relay
- [ ] `curl` + `httpie` — HTTP clients
- [ ] `mtr` — network diagnostics (traceroute + ping)
- [ ] `iperf3` — bandwidth measurement
- [ ] `nethogs` / `iftop` — per-process/per-connection bandwidth monitoring
- [ ] `ss` / `iproute2` — socket statistics and routing

**DNS Tooling**
- [ ] `dig` / `drill` — DNS lookup
- [ ] `dnsx` — fast DNS toolkit
- [ ] `dnsrecon` — DNS enumeration
- [ ] `fierce` — DNS zone traversal

**Web & Application Layer**
- [ ] `nikto` — web server scanner
- [ ] `gobuster` / `ffuf` — directory and subdomain fuzzing
- [ ] `wfuzz` — web fuzzer
- [ ] `sqlmap` — SQL injection detection (sandboxed, requires explicit agent approval)
- [ ] `nuclei` — template-based vulnerability scanner

**Wireless**
- [ ] `aircrack-ng` suite — 802.11 analysis
- [ ] `kismet` — wireless network detector

**Agent Integration**
- [ ] Each tool wrapped with an AGNOS agent API for programmatic invocation
- [ ] AI Shell (`agnsh`) understands natural language queries like "scan 192.168.1.0/24 for open ports"
- [ ] Results piped through LLM Gateway for automated interpretation and reporting
- [ ] All tool invocations require user approval for sensitive operations (per Human Sovereignty principle)
- [ ] Audit trail for every network operation

### Phase 6.5: OS-Level Features & Security Hardening (Planned Q3-Q4 2026)

These are gaps identified in the March 2026 comprehensive audit. They are required to
bring AGNOS from an application framework to a genuine operating system.

#### Missing OS-Level Features

| Feature | Description | Effort | Priority |
|---------|-------------|--------|----------|
| Init system integration | PID 1, service supervision, dependency ordering | 2 weeks | P1 |
| Package manager | Agent distribution, versioning, dependency resolution | 3 weeks | P1 |
| Filesystem integration | FUSE mount for agent-managed virtual filesystems | 1 week | P2 |
| Device management | udev rules, hardware abstraction layer for agents | 1 week | P2 |
| User/session management | PAM integration, multi-user agent isolation | 2 weeks | P1 |
| A/B system updates | Atomic OS updates with automatic rollback | 2 weeks | P2 |
| Power management | Suspend/hibernate with agent state serialization | 1 week | P3 |
| Network stack integration | NetworkManager/systemd-networkd integration, agent-aware firewall | 2 weeks | P1 |
| System logging (journald) | Unified logging across kernel + agents + userland | 1 week | P2 |
| Bootloader integration | GRUB/systemd-boot config for custom kernel | 3 days | P2 |
| Hardware-accelerated crypto | OpenSSL/BoringSSL engine for agent TLS | 1 week | P3 |

#### Missing Security Hardening

| Feature | Description | Effort | Priority |
|---------|-------------|--------|----------|
| ~~dm-verity~~ | ~~Read-only rootfs integrity verification~~ | — | ✅ Done 2026-03-04 |
| IMA/EVM | Integrity Measurement Architecture for file integrity | 2 weeks | P1 |
| TPM 2.0 integration | Measured boot, sealed secrets for agents | 2 weeks | P2 |
| ~~LUKS volumes for agent data~~ | ~~Encrypted-at-rest storage for each agent sandbox~~ | — | ✅ Done 2026-03-04 |
| ~~AppArmor/SELinux profiles~~ | ~~Mandatory access control profiles per agent type~~ | — | ✅ Done 2026-03-04 |
| ~~Audit subsystem (auditd)~~ | ~~Kernel-level audit events integrated with AGNOS audit chain~~ | — | ✅ Done 2026-03-04 |
| Key management service | Agent key rotation, certificate lifecycle management | 2 weeks | P2 |
| Certificate pinning | Pin TLS certs for cloud LLM API providers | 3 days | P2 |
| Memory encryption awareness | AMD SEV / Intel TDX support for confidential agents | 2 weeks | P3 |
| Secure boot chain | UEFI Secure Boot with custom kernel signing | 1 week | P2 |
| ~~Network segmentation~~ | ~~Per-agent network namespaces with firewall rules~~ | — | ✅ Done 2026-03-04 |
| ~~Secrets management~~ | ~~Vault-style secrets injection for agent env vars~~ | — | ✅ Done 2026-03-03 |

### Phase 6.6: Consumer Project Integration (Planned Q3–Q4 2026)

AGNOS serves as the base platform for two consumer projects. Their requirements
drive prioritisation of OS-level features and security hardening in Phases 6–6.5.

#### Consumer: AGNOSTIC (QA Automation Platform — Python/CrewAI)

AGNOSTIC's 6-agent QA team runs on AGNOS and routes inference through the LLM Gateway.

| Requirement | AGNOS Component | Phase | Status |
|-------------|-----------------|-------|--------|
| LLM Gateway HTTP API (port 8088) | llm-gateway | 5.1 | ✅ Done |
| Token accounting per agent (`X-Agent-Id` header) | llm-gateway | 5.1 | ✅ Done |
| Response caching + rate limiting | llm-gateway | 5.1 | ✅ Done |
| Container sandbox (Landlock + seccomp + namespaces) | agnos-sys | 5.1 | ✅ Done |
| cgroups v2 resource enforcement per agent | agent-runtime | 5.6 | ✅ Done |
| Audit trail integration (hash chain) | agnos-sys | 5.6 | ✅ Done |
| Agent registration HTTP API (port 8090) | agent-runtime | 6.6 | ✅ Done 2026-03-03 |
| Agent HUD visibility in desktop | desktop-environment | 6.6 | ✅ Done 2026-03-03 |
| Security UI (permission manager, kill switch) | desktop-environment | 6.6 | ✅ Done 2026-03-03 |
| Multi-agent resource scheduler | agent-runtime | 6.6 | ✅ Done 2026-03-03 |

**Current integration**: Phase 1 (LLM Gateway only). Config: `AGNOS_LLM_GATEWAY_ENABLED=true`, `PRIMARY_MODEL_PROVIDER=agnos_gateway`.

#### Consumer: SecureYeoman (Sovereign AI Agent Platform — TypeScript/Bun)

SecureYeoman intends to use AGNOS as its base Docker image once security
hardening is complete. Currently uses `debian:bookworm-slim`.

| Requirement | AGNOS Component | Phase | Status |
|-------------|-----------------|-------|--------|
| Landlock filesystem restrictions (`CONFIG_SECURITY_LANDLOCK=y`) | kernel config | 5.1 | ✅ Done |
| Seccomp-BPF syscall filtering (`CONFIG_SECCOMP_FILTER=y`) | kernel config | 5.1 | ✅ Done |
| cgroups v2 mount at `/sys/fs/cgroup` | kernel + supervisor | 5.6 | ✅ Done |
| User namespaces (`CONFIG_USER_NS=y`) | kernel config | 5.1 | ✅ Done |
| Network/PID namespaces | kernel config | 5.1 | ✅ Done |
| Pre-compiled seccomp profiles (Python, Node.js, Shell, WASM) | agent-runtime | 6.6 | ✅ Done 2026-03-03 |
| gVisor `runsc` pre-installed (opt-in) | Dockerfile | 6.6 | ✅ Done 2026-03-03 |
| WASM runtime (Wasmtime, feature-gated) | agent-runtime | 6.6 | ✅ Done 2026-03-03 |
| Audit subsystem (auditd + AGNOS hash chain) | kernel + agnos-sys | 6.5 | ✅ Done 2026-03-04 |
| dm-verity read-only rootfs | kernel | 6.5 | ✅ Done 2026-03-04 |
| LUKS encrypted agent data volumes | kernel + tools | 6.5 | ✅ Done 2026-03-04 |
| AppArmor/SELinux profiles per agent type | kernel + config | 6.5 | ✅ Done 2026-03-04 |
| Secrets management (Vault/Env/File injection) | agnos-common | 6.6 | ✅ Done 2026-03-03 |
| Network segmentation (per-agent netns + firewall) | agent-runtime | 6.5 | ✅ Done 2026-03-04 |
| Hardened base Docker image (`agnos-base:latest`) | Dockerfile | 6.6 | ✅ Done 2026-03-03 |
| Artifact sandbox scoping (task-scoped `/tmp` via Landlock) | agnos-sys | 6.5 | ⏳ Planned |
| Process resource metrics export (for anomaly detection) | agent-runtime | 6.5 | ⏳ Planned |

**Target Dockerfile**:
```dockerfile
FROM agnos-base:latest  # Linux 6.6 LTS, Landlock, seccomp, cgroups v2, gVisor, auditd
COPY dist/secureyeoman-linux-x64 /usr/local/bin/secureyeoman
USER secureyeoman
EXPOSE 18789
ENTRYPOINT ["secureyeoman"]
```

#### Priority-Driven Ordering

Based on consumer needs, the following Phase 6.5 items are **promoted to P0** for
the consumer integration milestone:

| Item | Original Priority | New Priority | Reason |
|------|-------------------|--------------|--------|
| ~~Audit subsystem (auditd)~~ | ~~P1~~ | ~~**P0**~~ | ✅ Done 2026-03-04 |
| ~~Network segmentation~~ | ~~P1~~ | ~~**P0**~~ | ✅ Done 2026-03-04 |
| ~~AppArmor/SELinux profiles~~ | ~~P1~~ | ~~**P0**~~ | ✅ Done 2026-03-04 |
| ~~dm-verity~~ | ~~P1~~ | ~~**P0**~~ | ✅ Done 2026-03-04 |
| ~~LUKS volumes~~ | ~~P1~~ | ~~**P0**~~ | ✅ Done 2026-03-04 |
| ~~Secrets management~~ | ~~P2~~ | ~~**P1**~~ | ✅ Done 2026-03-03 |
| ~~Hardened base Docker image~~ | ~~—~~ | ~~**P1**~~ | ✅ Done 2026-03-03 |

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

**Criteria:**
- [x] Phase 5.6 P0 items complete (sandbox, health, restart, unwrap, validation, cgroups) — **all done**
- [x] Phase 5.6 P1 items complete (CLI wiring, LLM integration, desktop, resource monitoring, audit, LLM syscalls) — **all done**
- [ ] 80% test coverage
- [ ] Performance benchmarks established (system-level + documentation)
- [ ] Third-party security audit complete
- [x] Documentation complete
- [x] Known issues documented

**Target Date**: End of Q2 2026
**Confidence**: High (95% complete, consumer integration features implemented, only test coverage + audit remain)

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

### Current Status

| Metric | Target | Current | Status | Priority |
|--------|--------|---------|--------|----------|
| Code Coverage | >80% | ~62% | 🔄 | P0 |
| Test Pass Rate | 100% | 100% | ✅ | - |
| Total Tests | 400+ | 2235 | ✅ | - |
| Agent Spawn Time | <500ms | ~300ms | ✅ | - |
| Shell Response Time | <100ms | ~50ms | ✅ | - |
| Memory Overhead | <2GB | ~1.2GB | ✅ | - |
| Boot Time | <10s | N/A | ⏳ | P1 |
| CIS Compliance | >80% | ~75% | 🔄 | P0 |
| Stub Implementations (P0) | 0 | 0 | ✅ | - |
| Stub Implementations (P1) | 0 | 0 | ✅ | - |

### By Component

| Component | Tests | Stubs Remaining | Notes |
|-----------|-------|-----------------|-------|
| agnos-common | 168 | 0 | Secrets management ✅, telemetry ✅, LLM types ✅ |
| agnos-sys | 125 | 0 | LLM gateway delegation ✅, audit hash chain ✅ |
| agent-runtime | 255 | 0 | HTTP API ✅, seccomp profiles ✅, WASM runtime ✅, load-aware scheduler ✅ |
| llm-gateway | 145 | 0 (P2 only) | HTTP handler integration tests ✅, streaming + cloud providers are P2 |
| ai-shell | 319 | 0 (P2 only) | Session/prompt/approval ✅, 8 intents + output formatting are P2 |
| desktop-environment | 268 | 0 | HUD ✅, security enforcement ✅, apps ✅, system metrics ✅ |

---

## Architecture Decision Records

### Completed
1. ✅ ADR-001: Rust as Primary Implementation Language
2. ✅ ADR-002: Wayland for Desktop Environment
3. ✅ ADR-003: Multi-Agent Orchestration Architecture
4. ✅ ADR-004: LLM Gateway Service Design
5. ✅ ADR-005: Security Model and Human Override
6. ✅ ADR-006: Testing Strategy and CI/CD
7. ✅ ADR-007: OpenAI-compatible HTTP API for LLM Gateway

---

## Contributing

### Priority Contribution Areas

1. **Testing (P0)** - Increase coverage to 80%+
   - Good first issues: Add tests to ai-shell and desktop-environment
   - See [docs/development/testing.md](/docs/development/testing.md)

2. **CIS Compliance (P0)** - 8 kernel config/sysctl controls remaining
   - Quick wins: disable USB_STORAGE/FIREWIRE/THUNDERBOLT, source route rejection, /tmp sticky bit
   - See [docs/security/cis-benchmarks.md](/docs/security/cis-benchmarks.md)

3. **Performance (P1)** - System-level benchmarks and documentation
   - Micro-benchmarks exist, need integration-level tests
   - Create `docs/development/performance-benchmarks.md`

4. **P2 Feature Gaps** - Streaming, cloud providers, ai-shell intents
   - See Phase 5.6 P2 table for full list (~10 days of work)

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

## Completed Work History

For detailed history of all completed work, see [CHANGELOG.md](/CHANGELOG.md).

### Recent Major Achievements

**Phase 5.1-5.5 (Partial):**
- Core infrastructure (agent SDK, HTTP API, IPC routing)
- Security & compliance (fuzzing, SBOM, CIS benchmarks, scanning)
- Release infrastructure (signing, updates, automation, telemetry)
- Testing improvements (45% → 65% coverage, 350+ tests passing)
- Agent Development Guide created
- 16 integration tests for agent-orchestrator
- 29 micro-benchmarks across 4 packages

**March 2026 Comprehensive Audit (P0+P1 Fixes Applied):**
- Fixed critical errno handling in syscall wrappers
- Replaced stub Landlock/seccomp with real Linux syscall implementations in `agnos-sys`
- Fixed Supervisor clone creating empty state maps (health monitoring broken)
- Fixed Orchestrator disconnected message loop + unbounded result growth
- Added length-prefixed IPC framing, socket permissions, Drop cleanup
- Added request body limits (1MB), connection pooling, input validation to LLM gateway
- Replaced raw string cache keys with hashed keys for O(1) lookups
- Switched telemetry to VecDeque for O(1) eviction
- Added bounded collections to SecurityUI, DesktopShell, MessageBus
- Redacted API keys and auth tokens from Debug output (custom impls)
- Added constant-time token comparison (subtle crate)
- Fixed ai-shell: shlex parsing, path traversal prevention, 64KB input limit
- Shared reqwest::Client across all helper functions (connection reuse)
- Sanitized error messages in HTTP responses to prevent info leakage
- Added OS-level feature gaps and security hardening items to roadmap

**March 3, 2026 Internal Audit:**
- Discovered 30+ stub implementations behind complete-looking interfaces
- Revised Phase 5 completion from 88% to 75%
- Added Phase 5.6 (Internal Implementation Gaps) to roadmap

**March 3, 2026 P0/P1 Implementation:**
- Wired sandbox enforcement (Landlock + seccomp + namespaces) in agent-runtime and ai-shell to `agnos-sys::security`
- Implemented real agent health checks (process liveness + IPC socket probe) in supervisor
- Implemented agent restart with exponential backoff (2^n sec, max 5 retries) in supervisor
- Fixed 6 panicking `.unwrap()`/`.expect()` across llm-gateway, desktop-environment, agnos-sys, agnos-common
- Enforced `InferenceRequest::validate()` at LLM gateway entry point; added `InferenceRequest::new()` constructor
- Wired all 10 CLI subcommands (5 agent-runtime + 5 llm-gateway) to real backend logic
- Connected ai-shell LLM client to LLM Gateway HTTP API (port 8088) with graceful fallback
- Implemented task dependency checking in orchestrator scheduler loop
- Added real system info to telemetry: OS version, memory, kernel version from /proc
- Wired desktop terminal to `tokio::process::Command` with stdout/stderr capture
- Wired system status to read from /proc/stat, /proc/meminfo, libc::statvfs
- Test count: 402+ tests, 0 failures across all packages
- Revised Phase 5 completion from 75% to 82%

**March 3, 2026 P0/P1 Implementation Pass #2:**
- Implemented cgroups v2 resource enforcement: `CgroupController` manages per-agent cgroups, sets `memory.max`/`cpu.max`, adds PID to `cgroup.procs`, reads counters for real usage
- Implemented real agent resource monitoring: reads VmRSS, CPU time (utime+stime), FD count, thread count from `/proc/{pid}/`
- Implemented agent pause/resume via SIGSTOP/SIGCONT signals
- Implemented audit logging with SHA-256 hash chain to `/var/log/agnos/audit.log` (JSON lines, flock-based concurrent write safety)
- Implemented LLM syscalls via LLM Gateway HTTP delegation: `load_model()`, `unload_model()`, `inference()` with handle tracking and input validation
- Wired desktop Agent Manager to scan `/run/agnos/agents/` sockets with connectivity probing
- Wired desktop Audit Viewer to read and filter real audit log entries
- Wired desktop Model Manager to query LLM Gateway `/v1/models` and Ollama `/api/pull`
- Added 9 new tests for LLM syscall implementation
- Test count: 420+ tests, 0 failures across all packages
- P0 stubs: 1 → 0, P1 stubs: 6 → 0 (all eliminated)
- Revised Phase 5 completion from 82% to 91%

---

*Last Updated: 2026-03-04 (test coverage push 46%→62%, 876 new tests, roadmap + changelog updated) | Next Review: 2026-03-10*
