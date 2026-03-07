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
- ~~Sandbox partial application rollback~~ — Done: `apply()` now calls `teardown()` on failure
- ~~Secret zeroing optimization~~ — Done: `zeroize` crate integrated, `SecretValue::drop` uses volatile zeroing
- ~~Thread-unsafe env var manipulation in secrets.rs~~ — Done: Mutex guard + safety docs
- ~~Various `let _ =` swallowed errors in IPC/service manager~~ — Done: logged in ipc.rs, service_manager.rs, supervisor.rs
- ~~Desktop environment blanket `#![allow(dead_code)]`~~ — Done: narrowed to lib.rs only, main.rs improved docs
- ~~AI Shell Question intent stub~~ — addressed in Phase 6.7
- ~~Placeholder certificate pins~~ — Done: runtime warning + detailed docs on `default_agnos_pins()`
- ~~Streaming parser O(n²) String::drain pattern~~ — Done: replaced with `split_off` in all 5 providers

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
| 6.7 | Complete | 100% | Alpha Polish (14 items: question intent, tab-completion, pipelines, aliases, agent KV store, conversation context, reasoning traces, dashboard, log viewer, output capture, enriched health, hot-reload, fleet config, environment profiles) |
| 6.8 | Complete | 100% | Beta Features (34 items: vector store, RAG pipeline, knowledge base, file watcher, agent RPC, templates, capability negotiation, circuit breaker, cron tasks, OpenTelemetry, resource forecasting, Prometheus metrics, webhooks, audit forwarding, memory bridge REST, trace submission REST, accessibility, clipboard, window badges, popups, gestures, anomaly detection, mTLS, secrets rotation, integrity attestation, token budgets, gateway metrics, Docker base images, envoy sidecar, cross-project integration APIs) |
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

#### Docker Base Images (Post-Alpha)

Publish runtime-specific base images for consumer projects (SecureYeoman, AGNOSTIC, BullShift).

Blockers before migration:
- [ ] **Alpha release** — third-party security audit must complete
- [ ] **Node.js 20 runtime layer** — publish `agnos:node20` variant (exists as `docker/Dockerfile.node`)
- [ ] **Node.js 22 runtime layer** — publish `agnos:node22` variant for SecureYeoman (currently on `node:22-slim`)
- [ ] **Python runtime layer** — publish `agnos:python3.12` variant (exists as `docker/Dockerfile.python`)
- [ ] **Rust runtime layer** — publish `agnos:rust` variant for BullShift (currently on `debian:bookworm-slim`)

### Phase 6.7: Alpha Polish — Core Experience Gaps (Complete) — [ADR-008](../adr/adr-008-phase67-alpha-polish.md)

All 14 items implemented. See [Phase 6.7 Developer Guide](phase67-guide.md) for usage.

#### AI Shell & User Interaction (4/4 Complete)

| Item | Component | Status |
|------|-----------|--------|
| Wire Question intent to LLM | ai-shell/session.rs | Done |
| Shell tab-completion for agents/services | ai-shell/completion.rs | Done (16 tests) |
| Shell pipeline support | ai-shell/interpreter.rs | Done |
| Shell aliases & user macros | ai-shell/aliases.rs | Done (12 tests) |

#### Agent Intelligence & Memory (3/3 Complete)

| Item | Component | Status |
|------|-----------|--------|
| Agent persistent memory (KV store) | agent-runtime/memory_store.rs | Done (20 tests) |
| Agent conversation context window | agent-runtime/learning.rs | Done (10 tests) |
| Structured reasoning traces | agent-runtime/tool_analysis.rs | Done (12 tests) |

#### Observability & Debugging (4/4 Complete)

| Item | Component | Status |
|------|-----------|--------|
| Agent activity dashboard (TUI) | ai-shell/dashboard.rs | Done (14 tests) |
| Structured event log viewer | ai-shell/audit.rs | Done (8 tests) |
| Agent stdout/stderr capture & replay | agent-runtime/supervisor.rs | Done (10 tests) |
| Health check endpoint enrichment | agent-runtime/http_api.rs | Done |

#### Configuration & Operations (3/3 Complete)

| Item | Component | Status |
|------|-----------|--------|
| Agent hot-reload (config change without restart) | agent-runtime/lifecycle.rs | Done (8 tests) |
| Declarative agent fleet config (TOML) | agent-runtime/service_manager.rs | Done (12 tests) |
| Environment profiles (dev/staging/prod) | agnos-common/config.rs | Done (16 tests) |

---

### Phase 6.8: Beta Features — Depth & Differentiation (Complete)

All 34 items implemented. Features that make AGNOS meaningfully better than running agents on a generic Linux box.

#### RAG & Knowledge (The Killer Feature Gap)

| Item | Component | Effort | Priority | Description |
|------|-----------|--------|----------|-------------|
| Embedded vector store | agnos-common or new crate | 1 week | P1 | Local vector DB (usearch, qdrant-lite, or custom HNSW) for semantic search. Agents can index documents, code, logs and query by meaning |
| RAG pipeline integration | llm-gateway | 1 week | P1 | Automatic context retrieval: query vector store → inject relevant chunks → LLM call. Configurable per agent (chunk size, top-k, reranking) |
| System knowledge base | agent-runtime | 3 days | P2 | Auto-index system docs, man pages, agent manifests, audit logs. Every agent gets a searchable understanding of the OS |
| File watcher + auto-indexing | agnos-sys/inotify | 2 days | P2 | inotify-based file change detection → automatic re-indexing into vector store. Keeps knowledge current |

#### Advanced Agent Capabilities

| Item | Component | Effort | Priority | Description |
|------|-----------|--------|----------|-------------|
| Agent-to-agent RPC (typed messages) | agent-runtime/ipc.rs | 3 days | P1 | Beyond pub/sub: request-response pattern with typed schemas, timeouts, and error propagation. Enables agent composition (agent A calls agent B's capabilities) |
| Agent templates & scaffolding | agent-runtime/package_manager.rs | 2 days | P2 | `agnos new --template web-scanner` generates a ready-to-run agent with manifest, sandbox config, and tests. Lowers barrier to agent creation |
| Agent capability negotiation | agent-runtime/registry.rs | 2 days | P2 | Agents advertise capabilities (e.g., "I can parse PDFs"). Orchestrator routes tasks to capable agents automatically |
| Circuit breaker for agent failures | agent-runtime/supervisor.rs | 1 day | P2 | After N consecutive failures, circuit opens (agent paused), half-open after cooldown. Prevents cascade failures in agent chains |
| Scheduled/cron agent tasks | agent-runtime/service_manager.rs | 2 days | P2 | Time-based task triggers (cron syntax). "Run vulnerability scan every Sunday at 02:00" without external scheduler |

#### Observability Stack

| Item | Component | Effort | Priority | Description |
|------|-----------|--------|----------|-------------|
| OpenTelemetry trace export | agnos-common/telemetry.rs | 3 days | P1 | OTLP export of traces/metrics/logs. Enables integration with Grafana, Jaeger, Datadog — standard observability |
| Distributed tracing (cross-service) | all crates | 2 days | P1 | Trace ID propagated through AI Shell → Agent Runtime → LLM Gateway → agent process. See the full lifecycle of a request |
| Prometheus metrics endpoint | agent-runtime + llm-gateway | 2 days | P2 | `/metrics` in Prometheus exposition format: agent counts, task latencies, LLM token rates, cache hit ratios, error rates |
| Resource usage forecasting | agent-runtime/resource.rs | 3 days | P3 | Predict memory/CPU needs based on trailing usage patterns. Alert before OOM, suggest resource limit adjustments |

#### Desktop & Accessibility

| Item | Component | Effort | Priority | Description |
|------|-----------|--------|----------|-------------|
| Accessibility (a11y) foundation | desktop-environment | 1 week | P1 | AT-SPI2 bridge for screen readers, keyboard navigation for all UI, high-contrast theme, focus indicators. Non-negotiable for a real OS |
| Clipboard integration | desktop-environment/compositor.rs | 2 days | P2 | wl_data_device protocol: copy/paste between agents, desktop apps, and AI Shell |
| Agent window ownership indicators | desktop-environment/security_ui.rs | 1 day | P2 | Visual badge on windows showing which agent owns them, trust level, sandbox status |
| XDG popup/positioner support | desktop-environment/wayland.rs | 2 days | P3 | Currently noted as unimplemented — needed for right-click menus, tooltips, dropdowns |
| Multi-touch gesture support | desktop-environment/wayland.rs | 2 days | P3 | Pinch-to-zoom, swipe between workspaces — expected on modern touch devices |

#### Security Hardening

| Item | Component | Effort | Priority | Description |
|------|-----------|--------|----------|-------------|
| Agent behavior anomaly detection | agent-runtime/learning.rs | 3 days | P1 | Baseline normal syscall/network/file patterns per agent. Alert on deviation (compromised agent, misbehavior). Uses existing telemetry data |
| Zero-trust agent networking (mTLS) | agent-runtime/ipc.rs + agnos-sys | 3 days | P2 | Mutual TLS for all agent-to-agent and agent-to-gateway communication. Certificate per agent, auto-rotated |
| Secrets rotation automation | agnos-common/secrets.rs | 2 days | P2 | Automatic rotation of API keys, tokens, certificates with configurable schedules and zero-downtime swap |
| Runtime integrity attestation | agnos-sys/ima.rs + tpm.rs | 2 days | P3 | Periodic runtime measurement: verify agent binaries, configs, and sandbox state haven't been tampered with post-boot |

#### Networking & Integration

| Item | Component | Effort | Priority | Description |
|------|-----------|--------|----------|-------------|
| Webhook/event sink support | agent-runtime/http_api.rs | 2 days | P2 | POST agent events (task complete, alert, approval needed) to external URLs. Enables integration with Slack, PagerDuty, CI/CD |
| gRPC API (alongside REST) | agent-runtime + llm-gateway | 1 week | P3 | Protocol Buffers + gRPC for high-performance agent communication. Streaming, bidirectional, typed contracts |
| Service mesh readiness (sidecar pattern) | agent-runtime | 3 days | P3 | Envoy/Linkerd sidecar injection for agents. Enables traffic shaping, retries, observability at network layer |

#### Cross-Project Integration

AGNOS-side infrastructure that consumer projects (AGNOSTIC, SecureYeoman, BullShift) connect to.

| Item | Component | Effort | Priority | Description |
|------|-----------|--------|----------|-------------|
| Unified audit log forwarding | agent-runtime/http_api.rs | 2 days | P1 | Accept structured audit events from external agents and append to cryptographic audit chain. Shared correlation IDs |
| External agent memory bridge | agent-runtime/memory_store.rs | 2 days | P1 | REST API for AgentMemoryStore — external agents persist/retrieve KV state through AGNOS |
| Shared observability pipeline | agnos-common/telemetry.rs + agent-runtime | 2 days | P1 | OpenTelemetry collector accepts traces from external services. Unified distributed traces |
| Python runtime base image | docker/ | 3 days | P2 | Publish `agnos:python3.12` Docker image with agent-runtime sidecar and sandbox |
| Node.js runtime base image | docker/ | 2 days | P2 | Publish `agnos:node20` Docker image with same sandbox/audit benefits |
| Fleet config for external agents | agent-runtime/service_manager.rs | 1 day | P2 | Extend FleetConfig for containerized external agents in `fleet.toml` |
| Cross-project reasoning traces | agent-runtime/tool_analysis.rs | 1 day | P2 | Accept ReasoningTrace submissions via REST API from external agents |
| LLM Gateway token budget sharing | llm-gateway | 2 days | P2 | Shared token budget pools with automatic rebalancing across consumers |
| Agent capability federation | agent-runtime/registry.rs | 2 days | P3 | External agents advertise capabilities through AGNOS capability negotiation |

#### SecureYeoman-Specific Integration ✅ ALL COMPLETE

| Item | Component | Effort | Priority | Status |
|------|-----------|--------|----------|--------|
| MCP server wrapper for agent runtime | agent-runtime/mcp_server.rs | 2 days | P1 | Done (20 tests) — 10 tools via `/v1/mcp/tools` and `/v1/mcp/tools/call` |
| Sandbox profile mapping API | agent-runtime/http_api.rs | 2 days | P2 | Done (12 tests) — `/v1/sandbox/profiles`, `/v1/sandbox/profiles/default`, `/v1/sandbox/profiles/validate` |
| LLM routing adapter | llm-gateway/http.rs | 1 day | P2 | Done (10 tests) — `X-Personality-Id`, `X-Source-Service`, `X-Request-Id`, `X-Token-Usage` headers; tools/tool_choice/response_format fields |
| Cryptographic audit chain bridge | agnos-common/audit.rs + http_api.rs | 2 days | P2 | Done (13 tests) — `ExternalAudit` event type, `AuditChain` struct, `/v1/audit/chain` and `/v1/audit/chain/verify` endpoints |

#### Future: Full Convergence

*Demand-gated — after Alpha release and integration phases stable.*

- [ ] **Unified SSO/OIDC provider** — AGNOS as OIDC-aware service. Single identity across consumer projects.
- [ ] **Cross-project agent delegation** — External orchestrator → A2A → AGNOS sandbox. Full chain with process isolation, resource quotas, and audit.
- [ ] **Shared vector store federation** — AGNOS embedded vector store queryable via REST from external services.
- [ ] **Unified agent marketplace backend** — AGNOS registry as single source of truth for agent capabilities.

---

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

#### Federation & Scale
- [ ] Multi-node agent federation (agents span AGNOS instances)
- [ ] Distributed task scheduling with consensus
- [ ] Agent migration/checkpointing (pause, serialize, resume elsewhere)
- [ ] Shared vector store across federated nodes

### Phase 8: Research (Planned Q1 2027)

#### Advanced Research
- [ ] Formal verification of security-critical components
- [ ] Novel sandboxing architectures
- [ ] AI safety mechanisms
- [ ] Human-AI collaboration research
- [ ] Post-quantum cryptography (CRYSTALS-Kyber, Dilithium)
- [ ] Agent explainability framework (attention visualization, decision trees)
- [ ] Reinforcement learning loop for agent policy optimization
- [ ] Fine-tuning pipeline (adapt local models to agent-specific tasks)

---

## Release Roadmap

### Alpha Release - Q2 2026

**Current version**: `2026.3.6` (CalVer: `YYYY.D.M`, patches as `-#N`)

**Remaining criteria:**
- [ ] Third-party security audit complete

**Phase 6.7 Alpha Polish (all 14 items complete):**
- [x] Wire Question intent to LLM Gateway
- [x] Agent persistent memory (KV store)
- [x] Agent activity dashboard (TUI)
- [x] Structured reasoning traces
- [x] Tab-completion, pipelines, aliases, log viewer, output capture, enriched health, hot-reload, fleet config, environment profiles

**Target Date**: End of Q2 2026

### Beta Release - Q3 2026

**Completed (wired in 2026.3.6):**
- [x] RAG pipeline operational — 6 HTTP endpoints (`/v1/rag/*`, `/v1/knowledge/*`), shell intents, file watcher background task
- [x] OpenTelemetry integration live — SpanCollector initialized, W3C traceparent injection/extraction in HTTP calls, `/v1/traces/spans` export
- [x] Accessibility foundation in desktop — AccessibilityTree wired to compositor (window create/close/focus sync, Tab navigation, announcements), `--accessibility` and `--high-contrast` CLI flags, HighContrastTheme in renderer
- [x] Agent-to-agent RPC — RpcRegistry + RpcRouter in ApiState, 4 HTTP endpoints (`/v1/rpc/*`), method discovery and invocation
- [x] Behavior anomaly detection — AnomalyDetector in ApiState, 4 HTTP endpoints (`/v1/anomaly/*`), audit log integration for alerts
- [x] Performance benchmarks — criterion benchmark suites for agent-runtime (vector search, RAG, RPC, anomaly), llm-gateway (metrics, acceleration), desktop-environment (renderer, scene graph, framebuffer)
- [x] HTTP API server startup — `run_daemon()` now spawns the HTTP API server on port 8090

**Remaining:**
- [ ] Community testing program
- [ ] Bug fixes from alpha feedback
- [ ] Performance optimized based on benchmarks
- [ ] Update system operational and tested
- [ ] Support channels open (Discord, forum)
- [ ] Video tutorials published

**Target Date**: Mid-Q3 2026

### v1.0 Release - Q4 2026

**Criteria:**
- Production ready (all critical bugs resolved)
- Enterprise features complete (SSO, audit logging, mTLS)
- Certifications complete (if pursued)
- Commercial support available
- Migration guides published
- Full observability stack (Prometheus, tracing, dashboards)
- Agent marketplace MVP (ADR-015)
- Secrets rotation automation
- Scheduled agent tasks
- Flutter runtime support (Wayland backend)
- Photis Nadi desktop integration (see below)

### Photis Nadi Desktop Support — v1.0+

#### Prerequisite 1: Agent Marketplace MVP (ADR-015)

Distribution channel for agents and desktop apps. Six sub-phases.

**Phase 1A — Manifest Extensions** (14 tests) ✓
- [x] `MarketplaceManifest` type in `marketplace/mod.rs` — extends `AgentManifest` with publisher, category, runtime, screenshots, changelog, min_agnos_version, dependencies, tags
- [x] `PublisherInfo` struct, `MarketplaceCategory` enum (6 variants), qualified naming
- [x] Manifest validation: name format, semver, required fields, length checks

**Phase 1B — Trust & Signing** (22 tests) ✓
- [x] `ed25519-dalek` + `rand` dependencies
- [x] `marketplace/trust.rs`: `PublisherKeyring`, `KeyVersion` with validity windows, Ed25519 sign/verify, key generation, hex encoding
- [x] `marketplace/transparency.rs`: `TransparencyLog` append-only hash chain, `LogEntry`, chain verification, JSON import/export
- [x] SHA-256 content hashing via existing `sha2` crate

**Phase 1C — Local Registry** (14 tests) ✓
- [x] `marketplace/local_registry.rs`: `LocalRegistry` — file-backed index at `/var/lib/agnos/marketplace/`
- [x] `install_package()` — extract gzip tarball, validate manifest, register; `uninstall_package()` — remove files, deregister
- [x] `flate2` + `tar` dependencies; path traversal protection in extraction
- [x] Storage quota enforcement, persistence (save/load JSON index), upgrade detection

**Phase 1D — Remote Client** (12 tests) ✓
- [x] `marketplace/remote_client.rs`: `RegistryClient` — HTTP client for remote registry
- [x] `search()`, `fetch_manifest()`, `download_package()`, `check_updates()` with offline mode
- [x] Local caching of search results and manifests; cert pinning via reqwest TLS

**Phase 1E — CLI Integration** (5 intents + 5 HTTP endpoints) ✓
- [x] `agnsh` intents: `install package`, `uninstall package`, `search marketplace`, `list packages`, `update packages`
- [x] HTTP API: `GET /v1/marketplace/installed`, `GET /v1/marketplace/search`, `POST /v1/marketplace/install`, `GET /v1/marketplace/:name`, `DELETE /v1/marketplace/:name`

**Phase 1F — Dependency Resolution** (7 tests) ✓
- [x] `DependencyGraph` in `marketplace/mod.rs` — DAG with topological sort, cycle detection, missing dependency checks
- [x] Diamond dependency support, deterministic install ordering

**Marketplace total: 88 tests (all passing)**

---

#### Prerequisite 2: Flutter Wayland Support

Enable Flutter desktop apps to run natively on AGNOS compositor. Four sub-phases.

**Phase 2A — Wayland Protocol Completeness** ✅ Done (49 tests)
- [x] Implement missing Wayland protocols in `desktop-environment/src/wayland.rs`:
  - `wl_data_device_manager` / `wl_data_device` — clipboard and drag-and-drop
  - `zwp_text_input_v3` — IME/text input for Flutter text fields
  - `xdg_decoration_unstable_v1` — server-side decorations
  - `wp_viewporter` — surface viewport scaling
  - `wp_fractional_scale_v1` — fractional DPI scaling
- [x] Protocol dispatch integration with existing `WaylandServer` and `Dispatch` traits
- [x] Tests: protocol negotiation, capability advertisement, event sequencing

**Phase 2B — Plugin Host Infrastructure** ✅ Done (31 tests)
- [x] Create `desktop-environment/src/plugin_host.rs` (per ADR-017):
  - `PluginHost` — manages plugin lifecycle over Unix domain sockets
  - `PluginProcess` — spawn, monitor, restart plugin processes
  - `PluginSandbox` — Landlock + seccomp profile for desktop plugins
  - Plugin types: `Theme`, `PanelWidget`, `AppLauncher`, `Notification`, `DesktopApp`
- [x] IPC protocol: JSON-RPC over UDS at `/run/agnos/plugins/{plugin_id}.sock`
- [x] Resource limits: CPU/memory cgroup constraints per plugin
- [x] Health monitoring: heartbeat, OOM detection, crash recovery with backoff
- [x] Per-type sandbox profiles with Landlock/seccomp/network rules

**Phase 2C — Flutter App Packaging Spec** ✅ Done (21 tests)
- [x] Define `.agnos-agent` layout for Flutter apps:
  - `bin/flutter_engine.so` — Flutter engine shared library
  - `bin/<app_name>` — AOT-compiled app binary
  - `assets/flutter_assets/` — Dart assets, fonts, shaders
  - `manifest.json` — category: `DesktopApp`, `runtime: "flutter"`, Wayland requirements
  - `sandbox.json` — per-app Landlock/seccomp/network rules
- [x] `FlutterAppManifest` validation, `WaylandRequirement` matching, backend determination
- [x] Launch integration: `build_launch_config()` with `GDK_BACKEND=wayland`, compositor socket
- [x] Environment variable generation for Wayland and XWayland backends

**Phase 2D — XWayland Fallback** ✅ Done (20 tests)
- [x] XWayland compatibility layer in `desktop-environment/src/xwayland.rs`:
  - `XWaylandManager` — spawn/manage Xwayland process
  - `XWaylandSurface` — map X11 windows to Wayland surfaces
  - Window property translation: X11 `_NET_WM_*` → compositor window states
- [x] Fallback detection: if Flutter app requests X11-only features, auto-launch XWayland
- [x] Security: XWayland runs in dedicated sandbox, no access to native Wayland clients' surfaces
- [x] Configuration: `xwayland_enabled: bool` in compositor config (default: false, opt-in)

**Flutter Wayland total: 121 tests (estimated 58)**

---

#### Integration Items (post-prerequisites) ✅ ALL COMPLETE

Once marketplace and Flutter Wayland support are complete:

- [x] Flutter app `.agpkg` packaging spec — bundle engine + app binary + assets, sandbox manifest (15 tests)
- [x] Photis Nadi sandbox profile — Landlock rules for `~/.local/share/photisnadi/` (Hive DB), network for Supabase, no process spawn (18 tests)
- [x] Desktop shell integration — map `system_tray` → AGNOS shell panel, `window_manager` → compositor window management, notifications → `DesktopShell::show_notification()` (26 tests)
- [x] MCP agent bridge — wire Photis Nadi's 6 MCP tools (list_tasks, create_task, update_task, get_rituals, analytics, sync) into AGNOS agent runtime via `/v1/mcp/tools` (20 tests)
- [x] AI Shell intents — natural language task management ("show my tasks", "create task: fix login bug", "how are my rituals today") (22 tests)
- [x] High-contrast theme sync — propagate AGNOS `HighContrastTheme` to Flutter's `ThemeData` via platform channel (18 tests)

---

## Key Performance Indicators (KPIs)

### Current Status (as of 2026-03-07)

| Metric | Target | Current | Status |
|--------|--------|---------|--------|
| Code Coverage | >80% | ~82% | Met |
| Test Pass Rate | 100% | 100% | Met |
| Total Tests | 400+ | 8098+ | Met |
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
| agent-runtime | 1672 + 16 integration + 15 load | Service manager, lifecycle, pub/sub, rollback, package manager, quotas, IPC, WASM, network tools (100), swarm (20), learning (13), multimodal (15), tool analysis (12), marketplace (88), flutter packaging (21), flutter agpkg (15), sandbox profiles (18), MCP Photis bridge (20), **sigil (35), aegis (40), takumi (43), argonaut (46), agnova (41)** |
| llm-gateway | 249 + 423 | 5 providers, rate limiting, streaming, graceful degradation, cert pinning, hardware acceleration (43) |
| ai-shell | 577 + 555 | 25+ intents: file ops, audit, agent, service, network scan, journal, device, mount, boot, update, marketplace, tasks, rituals, productivity |
| desktop-environment | 792 + 562 + 40 E2E | Wayland protocol types + Dispatch traits (63), protocol extensions (49), plugin host (31), xwayland (20), shell integration (26), theme bridge (18), HUD, security, apps, compositor, system tests |

---

## Architecture Decision Records

1. ADR-001: Rust as Primary Implementation Language
2. ADR-002: Wayland for Desktop Environment
3. ADR-003: Multi-Agent Orchestration Architecture
4. ADR-004: LLM Gateway Service Design
5. ADR-005: Security Model and Human Override
6. ADR-006: Testing Strategy and CI/CD
7. ADR-007: OpenAI-compatible HTTP API for LLM Gateway
8. ADR-008: Phase 6.7 Alpha Polish — Core Experience Gaps
9. ADR-009: RAG & Embedded Knowledge Pipeline
10. ADR-010: Advanced Agent Capabilities & Lifecycle
11. ADR-011: Observability Stack
12. ADR-012: Desktop Accessibility & Interaction Foundations
13. ADR-013: Zero-Trust Security Hardening
14. ADR-014: Cross-Project Integration Architecture
15. ADR-015: Agent Marketplace Architecture (Proposed — Phase 7)
16. ADR-016: Multi-Node Agent Federation (Proposed — Phase 7)
17. ADR-017: Desktop Plugin Architecture (Proposed — Phase 7)
18. ADR-018: LFS-Native Distribution — Dropping Debian Dependency (Accepted)
19. ADR-019: Sigil — System-Wide Trust Verification (Accepted)
20. ADR-020: Aegis — System Security Daemon (Accepted)
21. ADR-021: Takumi — Package Build System (Accepted)
22. ADR-022: Argonaut — Custom Init System (Accepted)
23. ADR-023: Agnova — OS Installer (Accepted)

---

## Named Subsystems

AGNOS uses named subsystems for major cross-cutting concerns. See [ARCHITECTURE.md](/docs/ARCHITECTURE.md) for full details.

| Name | Role | Status | Components |
|------|------|--------|------------|
| **ark** | Unified package manager | Done (Phase 8A) | `ark.rs`, `nous.rs`, `/v1/ark/*` API, agnsh intents |
| **nous** | Package resolver daemon | Done (Phase 8A) | `nous.rs`, source detection, SystemPackageDb |
| **takumi** | Package build system | Done (Phase 8C) | `takumi.rs`: TOML recipes, .ark format, security hardening, dependency resolution (43 tests) |
| **mela** | Agent/app marketplace | Done (Phase 7) | `marketplace/` module (trust, registry, transparency, flutter, sandbox profiles) |
| **aegis** | System security daemon | Done (Phase 8F) | `aegis.rs`: threat events, quarantine, scanning, auto-response (40 tests) |
| **argonaut** | Init system | Done (Phase 8D) | `argonaut.rs`: boot modes, service management, health checks (46 tests) |
| **agnova** | OS installer | Done (Phase 8E) | `agnova.rs`: disk layout, encryption, bootloader, package deployment (41 tests) |
| **sigil** | Trust system | Done (Phase 8B) | `sigil.rs`: SigilVerifier, TrustLevel, TrustPolicy, RevocationList, boot chain verification (35 tests) — [ADR-019](../adr/adr-019-sigil-trust-system.md) |

### Implementation Roadmap

**Phase 8A — ark + nous** ✅ Complete
- [x] nous resolver: source detection, SystemPackageDb, unified search
- [x] ark CLI: command parser, install planner, output formatter
- [x] HTTP API: `/v1/ark/*` routes
- [x] AI Shell: `ark install/remove/search/info/update/upgrade/status` intents

**Phase 8B — sigil** ✅ Complete (35 tests) — [ADR-019](../adr/adr-019-sigil-trust-system.md)
- [x] Promote trust primitives to `sigil.rs` as system-wide trust module
- [x] TrustLevel hierarchy: SystemCore > Verified > Community > Unverified > Revoked
- [x] TrustPolicy with enforcement modes: Strict, Permissive, AuditOnly
- [x] Agent binary verification (`verify_agent_binary()`) before execution
- [x] Package verification (`verify_package()`) for ark install
- [x] Boot chain verification (`verify_boot_chain()`) for argonaut
- [x] Artifact signing with Ed25519 (`sign_artifact()`)
- [x] RevocationList: revoke by key_id or content_hash, JSON persist
- [x] TrustStore: cache verified artifacts by content hash

**Phase 8C — takumi + .ark packages** ✅ Complete (43 tests) — [ADR-021](../adr/adr-021-takumi-build-system.md)
- [x] `.ark` package format: ArkManifest, ArkFileEntry, ArkPackage types
- [x] TOML recipe system: BuildRecipe with PackageMetadata, SourceSpec, DependencySpec, BuildSteps, SecurityFlags
- [x] Security hardening flags: PIE, RELRO, FullRelro, Fortify, StackProtector, Bindnow
- [x] CFLAGS/LDFLAGS generation from SecurityFlags
- [x] Build dependency resolution: topological sort with cycle detection
- [x] File list generation: recursive directory walk with SHA-256 per file
- [x] Build pipeline stages: Pending → Downloading → Extracting → Configuring → Building → Testing → Installing → Packaging → Signing → Complete
- [x] Recipe validation with warnings
- [x] Recipe loading from .toml files

**Phase 8D — argonaut** ✅ Complete (46 tests) — [ADR-022](../adr/adr-022-argonaut-init-system.md)
- [x] Three boot modes: Server (headless), Desktop (compositor), Minimal (container)
- [x] Boot sequence: 9 ordered stages from MountFilesystems to BootComplete
- [x] Default service definitions per mode (agent-runtime, llm-gateway, aethersafha, agnoshi)
- [x] Service dependency resolution: topological sort with cycle detection
- [x] Service state machine: Stopped → Starting → Running → Stopping → Failed → Restarting
- [x] Health checks: HTTP, TCP, Command, ProcessAlive
- [x] Ready checks: block boot until service responds
- [x] Restart policies: Always, OnFailure, Never
- [x] Shutdown ordering: reverse of startup order
- [x] Boot duration tracking and statistics

**Phase 8E — agnova** ✅ Complete (41 tests) — [ADR-023](../adr/adr-023-agnova-installer.md)
- [x] Install modes: Server, Desktop, Minimal, Custom
- [x] Disk layout: GPT partitioning, ESP + root, LUKS2 encryption
- [x] Bootloader config: systemd-boot and GRUB2 support
- [x] Network config: DHCP and static IP
- [x] User config: default shell `/usr/bin/agnoshi`, groups, SSH keys
- [x] Security config: LUKS, Secure Boot, TPM, dm-verity, firewall
- [x] Package selection: base packages + mode-specific + extra
- [x] Install pipeline: 14 phases from ValidateConfig to Complete
- [x] Config validation with error reporting
- [x] System generation: machine-id, hostname, fstab, kernel cmdline
- [x] Install time estimation per mode
- [x] Default kernel params from security config

**Phase 8F — aegis** ✅ Complete (40 tests) — [ADR-020](../adr/adr-020-aegis-security-daemon.md)
- [x] Unified security daemon: AegisSecurityDaemon
- [x] Threat levels: Critical, High, Medium, Low, Info with ordering
- [x] Security event pipeline: 10 event types (IntegrityViolation, SandboxEscape, etc.)
- [x] Auto-quarantine on Critical/High threats
- [x] Quarantine actions: Suspend, Terminate, Isolate, RateLimit
- [x] Agent scanning on install and execute
- [x] Quarantine management: quarantine, release, auto-release timeout
- [x] Event filtering: by agent, by threat level, unresolved
- [x] Event resolution tracking
- [x] Threat summary and statistics

---

## Contributing

### Priority Contribution Areas

1. **Third-party security audit (P1)** - External vendor engagement
2. **RAG / vector store integration (P1)** - Embedded semantic search for agents — the key differentiator
3. **Accessibility foundation (P1)** - AT-SPI2, keyboard nav, screen reader support
4. **OpenTelemetry integration (P1)** - Distributed tracing and metrics export
5. **Agent persistent memory (P1)** - SQLite/sled KV store per agent
6. **Video tutorials (P2)** - Installation, usage, agent creation, security overview
7. **Agent activity dashboard TUI (P2)** - Live operational view of agent fleet
8. **Kernel Development Guide (P3)** - For kernel hackers contributing to AGNOS kernel modules
9. **Support portal (P3)** - Community support channels (can use GitHub Issues/Discussions for Alpha)

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

*Last Updated: 2026-03-06 | Next Review: 2026-03-13*
