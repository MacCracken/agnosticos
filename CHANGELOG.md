# Changelog

All notable changes to AGNOS will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [2026.3.7] - 2026-03-07

### Added — Phase 7: Ecosystem — Federation & Scale (199 tests)

#### Phase 7A — Agent Ratings & Reviews (43 tests) — [ADR-024](docs/adr/adr-024-agent-ratings-reviews.md)
- **marketplace/ratings.rs** — Rating/review system: 1-5 star ratings, text reviews (max 2000 chars), RatingStore with per-agent deduplication, RatingStats with score distribution, RatingFilter (min_score, package, agent, date range), top_rated with min_ratings threshold, JSON persistence

#### Phase 7B — Multi-Node Federation (55 tests) — [ADR-016](docs/adr/adr-016-multi-node-federation.md)
- **agent-runtime/federation.rs** — Federation cluster: FederationNode with role/status/capabilities, simplified Raft coordinator election (term numbers, vote counting, split-brain prevention), node health tracking (online→suspect→dead), NodeScorer with weighted criteria (resource 40%, locality 30%, load 20%, affinity 10%), AgentPlacement, TOML config parsing, 3 scheduling strategies (Balanced/Packed/Spread)

#### Phase 7C — Agent Migration & Checkpointing (54 tests) — [ADR-025](docs/adr/adr-025-agent-migration-checkpointing.md)
- **agent-runtime/migration.rs** — Checkpoint/restore: Checkpoint with memory snapshot, vector indices, IPC queue, sandbox config; 3 migration types (Warm <500ms, Cold <5s, Live); 8-state migration state machine with validated transitions; MigrationTracker for lifecycle management and history; compression (~60% size reduction); checkpoint validation

#### Phase 7D — Distributed Task Scheduling (47 tests) — [ADR-026](docs/adr/adr-026-distributed-task-scheduling.md)
- **agent-runtime/scheduler.rs** — Task scheduler: priority-based scheduling (Normal/High/Critical/Emergency), resource-aware node placement, preemption logic, NodeCapacity tracking with utilization, CronScheduler for recurring tasks, task status state machine, SchedulerStats

### Added — Phase 9: Cloud Services & Human-AI Collaboration (169 tests)

#### Phase 9A — Cloud Services (82 tests) — [ADR-034](docs/adr/adr-034-cloud-services.md)
- **agent-runtime/cloud.rs** — Optional cloud connectivity: CloudConnection with config validation and health checks, CloudDeploymentManager with 4 resource tiers (Free/Standard/Performance/Custom) and cost tracking, SyncEngine with SHA-256 checksummed items and conflict resolution (LocalWins/RemoteWins/Manual/Merge), WorkspaceManager with role-based access (Owner/Admin/Editor/Viewer), BillingTracker with per-workspace/agent usage attribution

#### Phase 9B — Human-AI Collaboration Research (87 tests) — [ADR-035](docs/adr/adr-035-human-ai-collaboration.md)
- **agent-runtime/collaboration.rs** — Collaboration framework: 5 CollaborationModes (FullAutonomy/Supervised/Paired/HumanLed/TeachingMode), SharedTask with ownership and status state machine, HandoffManager for human↔agent task transfers, TrustCalibrator with EMA-based metrics and calibration error tracking, CognitiveLoadManager (overload detection, break suggestions, adaptive batch sizing), FeedbackCollector (5 types with rating validation and application tracking), CollaborationAnalyzer with session analytics and mode effectiveness

### Added — Phase 8K-8M: Research — Verification, Sandboxing & RL (221 tests)

#### Phase 8K — Formal Verification Framework (76 tests) — [ADR-031](docs/adr/adr-031-formal-verification.md)
- **agent-runtime/formal_verify.rs** — Property-based verification: 6 property types (Invariant/Precondition/Postcondition/Safety/Liveness/Refinement), PropertyChecker with invariant testing and counterexample detection, state machine verification (reachability, deadlock, determinism, unreachable states via BFS), trace refinement checking, InvariantMonitor for runtime verification, 15 built-in AGNOS security properties, VerificationReport with per-component coverage

#### Phase 8L — Novel Sandboxing Architectures (77 tests) — [ADR-032](docs/adr/adr-032-novel-sandboxing.md)
- **agent-runtime/sandbox_v2.rs** — Next-gen sandboxing: object-capability tokens (10 capability types, delegation chains, time-bounded, revocable), information flow control (5 security labels, no-downward-flow enforcement, data lineage tracking), TimeBoundedSandbox (wall-clock/CPU/operation budgets), PolicyLearner (derive sandbox profiles from observed behavior, tightening suggestions), ComposableSandbox (layered rules, most-restrictive-wins, merge), SandboxMetrics with security scoring

#### Phase 8M — Reinforcement Learning Optimization (68 tests) — [ADR-033](docs/adr/adr-033-reinforcement-learning.md)
- **agent-runtime/rl_optimizer.rs** — RL framework: State with feature vectors and Euclidean distance, Experience replay buffer (circular, prioritized sampling), QTable with Bellman updates, EpsilonGreedy exploration (decaying ε with floor), PolicyGradient (softmax + REINFORCE), RlOptimizer orchestrating train/select/episode lifecycle, RewardShaper with weighted components, OptimizerStats

### Added — Phase 8H-8J: Research — AI Explainability, Safety & Fine-Tuning (209 tests)

#### Phase 8H — Agent Explainability Framework (59 tests) — [ADR-028](docs/adr/adr-028-agent-explainability.md)
- **agent-runtime/explainability.rs** — Decision transparency: DecisionRecord with factors/alternatives/outcomes, ExplainabilityEngine with human-readable explanations (factor breakdown, confidence labels, review recommendations), DecisionFilter and AgentDecisionStats, DecisionTree builder with text rendering, AuditTrail linking decisions to audit events

#### Phase 8I — AI Safety Mechanisms (77 tests) — [ADR-029](docs/adr/adr-029-ai-safety-mechanisms.md)
- **agent-runtime/safety.rs** — Safety enforcement: 8 rule types (ResourceLimit, ForbiddenAction, RequireApproval, RateLimit, ContentFilter, ScopeRestriction, EscalationRequired, OutputValidation), SafetyEngine with policy CRUD and action/output checking, PromptInjectionDetector (6 pattern categories), SafetyCircuitBreaker (Closed/Open/HalfOpen), default_policies() with sensible defaults, per-agent safety scoring

#### Phase 8J — Fine-Tuning Pipeline (73 tests) — [ADR-030](docs/adr/adr-030-fine-tuning-pipeline.md)
- **agent-runtime/finetune.rs** — Model adaptation: TrainingDataset with quality-scored examples from 4 sources, FineTuneConfig with 4 methods (Full/LoRA/QLoRA/Prefix), FineTuneJob with validated state machine, JobProgress with percentage/ETA, FineTunePipeline for full lifecycle orchestration, ModelRegistry with best_model_for_agent selection, VRAM estimation per method

### Added — Phase 8G: Post-Quantum Cryptography (68 tests) — [ADR-027](docs/adr/adr-027-post-quantum-cryptography.md)
- **agent-runtime/pqc.rs** — Hybrid classical+PQC cryptography: ML-KEM-768/1024 (FIPS 203) key encapsulation + ML-DSA-44/65/87 (FIPS 204) digital signatures, hybrid KEM (X25519+ML-KEM with SHA-256 secret combination), hybrid signatures (Ed25519+ML-DSA with AND verification), PqcKeyStore with CRUD and JSON persistence, PqcConfig with 3 migration modes (Disabled/Hybrid/PqcOnly), PqcMigrationStatus tracking. Simulated PQC ops isolated in 6 swap-ready functions.

### Added — Phase 8B-8F: AGNOS Distribution Subsystems (205 tests)

#### Phase 8B — Sigil: System-Wide Trust Verification (35 tests) — [ADR-019](docs/adr/adr-019-sigil-trust-system.md)
- **agent-runtime/sigil.rs** — Unified trust module: TrustLevel hierarchy (SystemCore/Verified/Community/Unverified/Revoked), TrustPolicy with enforcement modes (Strict/Permissive/AuditOnly), SigilVerifier for artifact/agent/package/boot-chain verification, Ed25519 signing, RevocationList (revoke by key_id or content_hash), TrustStore cache

#### Phase 8C — Takumi: Package Build System (43 tests) — [ADR-021](docs/adr/adr-021-takumi-build-system.md)
- **agent-runtime/takumi.rs** — TOML recipe build system: BuildRecipe parser, .ark package format (ArkManifest, ArkFileEntry, ArkPackage), security hardening flags (PIE/RELRO/Fortify/StackProtector/Bindnow), CFLAGS/LDFLAGS generation, build dependency topological sort with cycle detection, recursive file list with SHA-256 hashing, build pipeline stages

#### Phase 8D — Argonaut: Init System (46 tests) — [ADR-022](docs/adr/adr-022-argonaut-init-system.md)
- **agent-runtime/argonaut.rs** — Custom init system: 3 boot modes (Server/Desktop/Minimal), 9-stage boot sequence, service dependency resolution, service state machine, health checks (HTTP/TCP/Command/ProcessAlive), ready checks, restart policies (Always/OnFailure/Never), shutdown ordering, boot duration tracking

#### Phase 8E — Agnova: OS Installer (41 tests) — [ADR-023](docs/adr/adr-023-agnova-installer.md)
- **agent-runtime/agnova.rs** — OS installer: 4 install modes, GPT disk partitioning with LUKS2 encryption, bootloader config (systemd-boot/GRUB2), network/user/security configuration, package selection per mode, 14-phase install pipeline, config validation, fstab/hostname/machine-id generation, kernel cmdline construction

#### Phase 8F — Aegis: Security Daemon (40 tests) — [ADR-020](docs/adr/adr-020-aegis-security-daemon.md)
- **agent-runtime/aegis.rs** — Unified security daemon: 5 threat levels with auto-response, 10 security event types, quarantine system (Suspend/Terminate/Isolate/RateLimit), agent/package scanning, auto-quarantine on Critical/High threats, auto-release timeouts, event filtering and resolution tracking

### Fixed — Code Audit: All 5 New Modules (54 additional tests)
- **aegis.rs** — Quarantine escalation (no overwrite), scan config flags respected, metadata error reporting, empty agent_id warning
- **argonaut.rs** — State machine transition validation, dependency-state checks before Starting, register preserves runtime state, boot timestamps, missing deps error, shutdown_order returns Result
- **sigil.rs** — AuditOnly mode blocks revoked artifacts, policy flags enforced, sign_artifact trust level checks, RevocationEntry validation
- **takumi.rs** — Package name path traversal prevention, URL scheme validation, SHA-256 format validation, duplicate recipe warning, flag deduplication
- **agnova.rs** — Kernel param injection validation, device path validation, hostname/username validation, non-recoverable phase blocking, DHCP/static IP validation

### Status Update
| Metric | Value |
|--------|-------|
| Total Tests | 9061+ (0 failures) |
| Compiler Warnings | 0 |
| New ADRs | 019-023 |
| New modules | sigil, aegis, takumi, argonaut, agnova |

---

## [2026.3.6] - 2026-03-06

### Current Status
| Metric | Value |
|--------|-------|
| Phase 5 Completion | 99% |
| Phase 6 Completion | 100% |
| Phase 6.5 Completion | 100% |
| Phase 6.8 Completion | 100% |
| Test Coverage | ~82% (7200+ tests, 0 failures) |
| Compiler Warnings | 0 |
| Clippy Warnings | 0 |
| CIS Compliance | ~85% |
| Phase 6.7 Completion | 100% |
| Alpha Blocker | Third-party security audit (vendor selection) |

### Added — Phase 6.8: Beta Features (34 Items)

#### RAG & Knowledge (4 items)
- **agent-runtime/vector_store.rs** — Embedded vector store: cosine similarity search, VectorIndex with insert/search/remove, JSON persistence, dimension validation (24 tests)
- **agent-runtime/rag.rs** — RAG pipeline: text chunking with overlap, bag-of-words TF embeddings, retrieval-augmented context formatting for LLM injection (16 tests)
- **agent-runtime/knowledge_base.rs** — System knowledge base: recursive directory indexing, keyword search with TF scoring, source filtering (ManPage/AgentManifest/AuditLog/ConfigFile), stats (17 tests)
- **agent-runtime/file_watcher.rs** — Polling-based file watcher: mtime comparison, recursive watching, glob pattern filtering, WatchEvent stream (15 tests)

#### Advanced Agent Capabilities (5 items)
- **agent-runtime/ipc.rs** — Agent-to-agent RPC: typed RpcRequest/RpcResponse, RpcRegistry for method advertisement, RpcRouter with oneshot channels and timeout support (22 tests)
- **agent-runtime/package_manager.rs** — Agent templates & scaffolding: TemplateRegistry with built-in scanner/monitor templates, variable substitution ({{agent_name}}/{{timestamp}}), file generation (13 tests)
- **agent-runtime/capability.rs** — Capability negotiation: CapabilityRegistry for agent capability advertisement/discovery, schema-compatible matching, supports external agents (AGNOSTIC QA) (20 tests)
- **agent-runtime/supervisor.rs** — Circuit breaker: Closed/Open/HalfOpen states, configurable failure threshold, automatic recovery timeout, half-open probing (14 tests)
- **agent-runtime/service_manager.rs** — Scheduled/cron tasks: 5-field cron expression parser, TaskScheduler with due_tasks/mark_completed, next_run computation (18 tests)

#### Observability Stack (4 items)
- **agnos-common/telemetry.rs** — OpenTelemetry traces: TraceId/SpanId, W3C traceparent header injection/extraction, SpanCollector with OTLP-like JSON export, distributed tracing context propagation (32 tests)
- **agent-runtime/resource_forecast.rs** — Resource forecasting: linear regression on CPU/memory samples, trend detection (Rising/Stable/Falling), breach prediction alerts (22 tests)
- **agent-runtime/http_api.rs** — Prometheus `/v1/metrics/prometheus` endpoint with exposition format; webhook event sink (register/list/delete); audit log forwarding from external agents; agent memory bridge REST API (GET/PUT/DELETE); reasoning trace submission REST API (24 tests)

#### Desktop & Accessibility (5 items)
- **desktop-environment/accessibility.rs** — AT-SPI2 foundation: AccessibilityTree with 24 roles, keyboard navigation (next/prev), screen reader announcements, focus management, high-contrast themes (28 tests)
- **desktop-environment/compositor.rs** — Clipboard integration: ClipboardManager with text/HTML/image/files support, history ring buffer, MIME type detection (12 tests)
- **desktop-environment/security_ui.rs** — Window ownership badges: TrustLevel (System/Verified/Unverified/Untrusted/Sandboxed), SandboxStatus indicators, WindowBadgeManager (12 tests)
- **desktop-environment/wayland.rs** — XDG popup/positioner: PopupManager with create/dismiss/reposition, Edge enum, ConstraintAdjustment flags (14 tests)
- **desktop-environment/gestures.rs** — Multi-touch gestures: GestureRecognizer for tap/double-tap/long-press/swipe/pinch/rotate, configurable thresholds (17 tests)

#### Security Hardening (4 items)
- **agent-runtime/learning.rs** — Behavior anomaly detection: BehaviorBaseline with sliding window stats, sigma-based anomaly detection (Low/Medium/High/Critical severity), multi-agent AnomalyDetector (17 tests)
- **agent-runtime/mtls.rs** — Zero-trust agent networking: CertificateAuthority for issuing/verifying/revoking/rotating agent certificates, expiry detection, MtlsPolicy enforcement (20 tests)
- **agnos-common/secrets.rs** — Secrets rotation automation: RotationPolicy with interval/max_age/notify_before, SecretRotationManager with status checking and scheduling, RotationLog audit trail (20 tests)
- **agent-runtime/integrity.rs** — Runtime integrity attestation: SHA-256 file verification, IntegrityVerifier with baseline creation and tamper detection, IntegrityReport with clean/mismatch/error summary (18 tests)

#### Networking & Integration (3 items + 9 cross-project)
- **llm-gateway/accounting.rs** — LLM token budget sharing: BudgetPool with per-project allocation/consumption/rebalancing, period-based reset, BudgetManager for multi-pool management (22 tests)
- **llm-gateway/rate_limiter.rs** — Gateway Prometheus metrics: GatewayMetrics with per-model request/token/latency/cache tracking, Prometheus exposition format export (12 tests)
- **docker/Dockerfile.python** — Python 3.12 base image with AGNOS agent directories and OpenTelemetry
- **docker/Dockerfile.node** — Node.js 20 base image with AGNOS agent directories
- **docker/envoy-sidecar.yaml** — Envoy sidecar proxy config for service mesh readiness

### Roadmap — Cross-Project Integration (AGNOSTIC + AGNOS)

- Added **9 cross-project integration items** to Phase 6.8 roadmap: unified audit log forwarding, external agent memory bridge, shared OpenTelemetry pipeline, Python/Node.js base images, fleet config for external agents, cross-project reasoning traces, LLM token budget sharing, capability federation
- Added **integration status table** to Consumer Integration section tracking Phase 1-4 progress
- Updated AGNOSTIC roadmap (`/home/macro/Repos/agnostic/docs/development/roadmap.md`) with matching Phase 3 (deep integration, 7 items) and Phase 4 (Docker migration, 3 items) sections
- Both roadmaps now reference shared items with aligned priorities and component mappings

### Added — Phase 6.7: Alpha Polish (14 Items) — [ADR-008](docs/adr/adr-008-phase67-alpha-polish.md)

#### AI Shell & User Interaction
- **ai-shell/completion.rs** — Tab-completion engine: BTreeSet-based prefix matching for built-in commands, intent keywords, all 34 network tools, dynamic agent/service names; context-aware completions after `start`/`stop`/`agent`/`mode` (16 tests)
- **ai-shell/aliases.rs** — Shell alias manager: set/remove/expand/list/contains, persistent aliases via `ShellConfig.aliases` in `~/.agnsh_config.toml` (12 tests)
- **ai-shell/interpreter.rs** — Pipeline support: `Intent::Pipeline` variant for `cmd1 | cmd2` pipe chains and NL `then` keyword chaining; executed via `sh -c` at `SystemWrite` permission level
- **ai-shell/session.rs** — Question intent wired: `Intent::Question` now routes through `LlmClient::answer_question()` with graceful fallback when LLM Gateway is unreachable

#### Agent Intelligence & Memory
- **agent-runtime/memory_store.rs** — Per-agent persistent KV store: file-backed JSON under `/var/lib/agnos/agent-memory/<agent-id>/`, atomic writes via temp+rename, path traversal prevention (`..`/`/` rejected), 1 MB value limit, 256-byte key limit, tag-based listing, usage tracking (20 tests)
- **agent-runtime/learning.rs** — Conversation context window: `ConversationContext` sliding window of recent interactions per agent, `format_for_llm()` export for LLM injection, import/export for persistence (10 tests)
- **agent-runtime/tool_analysis.rs** — Structured reasoning traces: `TraceBuilder` + `ReasoningTrace` chain-of-thought logging with per-step rationale, tool calls, output, timing, and status; `format_trace()` for human-readable display (12 tests)

#### Observability & Debugging
- **ai-shell/dashboard.rs** — Agent activity dashboard: htop-style TUI view with agent ID, status, CPU%, memory, task count, errors, last action; `DashboardClient` fetches from Agent Runtime API `/v1/agents` (14 tests)
- **ai-shell/audit.rs** — Structured event log viewer: `AuditViewer` with `AuditFilter` (agent, action, approved, time range, limit); tabular output formatting (8 tests)
- **agent-runtime/supervisor.rs** — Agent output capture: `OutputCapture` ring buffer for stdout/stderr with `tail(n)`, `filter_stream()`, `format_display()` (10 tests)
- **agent-runtime/http_api.rs** — Enriched health endpoint: `/v1/health` now returns component status (LLM Gateway reachability, agent registry), system metrics (hostname, load average, memory, disk)

#### Configuration & Operations
- **agent-runtime/lifecycle.rs** — Agent hot-reload: `LifecycleEvent::ConfigReloaded` variant + `reload_config()` method with diff-based change detection; hooks notified of changed fields without agent restart (8 tests)
- **agent-runtime/service_manager.rs** — Declarative fleet config: `FleetConfig` TOML-defined desired state with `ReconciliationPlan` (start/stop/unchanged); `from_file()` async loader, `reconcile()` against running services (12 tests)
- **agnos-common/config.rs** — Environment profiles: `EnvironmentProfile` (dev/staging/prod) with bind address, log level, security mode, auto-approve, max agents, audit verbosity; `from_env()` reads `AGNOS_PROFILE` env var (16 tests)

### Added — Phase 6: Agent Intelligence & Multi-Modal (3 New Modules)

- **agent-runtime/swarm.rs** — Swarm intelligence protocols: consensus voting (Majority/SuperMajority/Unanimous/MinVotes/MinPercent quorum rules), task decomposition (DataParallel/Pipeline/FunctionalSplit/Redundant strategies), stigmergy signals with exponential decay, aggregation strategies (Merge/Vote/Concatenate/BestScore) (19 tests)
- **agent-runtime/learning.rs** — Agent learning and adaptation: performance profiling with action outcome tracking, UCB1 (Upper Confidence Bound) strategy selection for exploration/exploitation balance, capability scoring with exponential moving average, score trend detection (13 tests)
- **agent-runtime/multimodal.rs** — Multi-modal agent support: Modality enum (Text/Vision/Audio/ToolUse/StructuredData/Code), ModalityProfile with cost estimation, ContentBlock for mixed-content messages, ModalityRegistry with factory methods (text_only/vision_capable/full_multimodal), message validation against modality profiles (14 tests)

### Added — LLM Tool Output Analysis Pipeline

- **agent-runtime/tool_analysis.rs** — LLM-based network tool output analysis: tool-specific system prompts for port scan/DNS/vuln scan/trace/capture analysis, structured response parsing (SUMMARY/RISK/FINDINGS/RECOMMENDATIONS format), FindingSeverity levels (Critical/High/Medium/Low/Info), HTTP integration with LLM Gateway on port 8088 (15 tests)

### Added — Wayland Dispatch Traits (Wire Protocol Handlers)

- **desktop-environment/wayland.rs** — Full `wayland-server` `Dispatch` trait implementations behind `wayland` feature flag:
  - `wl_compositor` — creates wl_surface instances with per-surface SurfaceData
  - `wl_surface` — commit, destroy, attach, damage, frame callbacks
  - `wl_shm` / `wl_shm_pool` / `wl_buffer` — shared memory buffer pipeline
  - `wl_seat` — input device capability advertising
  - `wl_output` — screen geometry/mode/scale via bind() hook
  - `xdg_wm_base` — XDG Shell entry point, xdg_surface creation
  - `xdg_surface` — get_toplevel, ack_configure
  - `xdg_toplevel` — title, app_id, maximize/fullscreen/minimize, move/resize, size constraints
  - `init_globals()` for registering all protocol objects on the display
  - `dispatch()` now calls `display.dispatch_clients()` for real wire protocol

### Added — Network Tool Agent Wrappers (7 Wrapper Structs)

- **agent-runtime/network_tools.rs** — Dedicated agent wrapper structs with typed builder APIs:
  - `PortScanner` — wraps nmap/masscan with ScanProfile (Quick/Standard/Thorough/Stealth), custom ports, returns `Vec<DiscoveredHost>` (5 tests)
  - `DnsInvestigator` — wraps dig/dnsrecon with record type filtering, custom nameserver, enumeration mode, returns `Vec<DnsRecord>` (3 tests)
  - `NetworkProber` — wraps traceroute/mtr/nmap with max hops, ping count, ping sweep, returns `Vec<TraceHop>` or `Vec<DiscoveredHost>` (2 tests)
  - `VulnAssessor` — wraps nuclei/nikto with severity filter, tag filter (2 tests)
  - `TrafficAnalyzer` — wraps tcpdump/tshark/ngrep with interface, BPF filters, packet count (3 tests)
  - `WebFuzzer` — wraps gobuster/ffuf with wordlist, extensions, threads, status codes (2 tests)
  - `SocketInspector` — wraps ss with listening/TCP/UDP filters, returns `Vec<SocketEntry>` (3 tests)
  - Network tools tests: 60 → 81

### Added — Hardware Acceleration Module

- **llm-gateway/acceleration.rs** — GPU/NPU detection and inference optimization:
  - `AcceleratorType` enum: Cpu, CudaGpu, RocmGpu, MetalGpu, IntelNpu, AppleNpu with throughput multipliers
  - `QuantizationLevel` enum: FP32/FP16/BF16/Int8/Int4 with memory reduction factors
  - `ShardingStrategy` enum: None/PipelineParallel/TensorParallel/DataParallel with min device counts
  - `AcceleratorRegistry`: system probing (nvidia-smi, rocm-smi, /sys/class/misc/intel_npu, /proc/device-tree), best device selection, memory estimation, automatic sharding plan generation
  - `ShardingPlan` with memory fitting validation
  - `InferenceConfig` for per-request acceleration settings
  - 43 tests

### Added — Remaining Network Tools (9 New Variants)

- **agent-runtime/network_tools.rs** — Expanded from 23 to 32 tool variants:
  - `netdiscover` (ARP scanning, Medium risk), `termshark` (TUI packet inspection, Critical), `bettercap` (network MITM analysis, Critical, --caplet validation)
  - `dnsx` (fast DNS toolkit, Medium), `fierce` (DNS zone traversal, High)
  - `wfuzz` (web fuzzer, High), `sqlmap` (SQL injection, Critical, --os-shell/--os-cmd/--file-write validation)
  - `aircrack-ng` (802.11 analysis, Critical, NET_RAW+NET_ADMIN), `kismet` (wireless detector, Critical, NET_RAW+NET_ADMIN)
  - Network tools tests: 81 → 100

### Added — Interactive API Explorer

- **docs/api/explorer.html** — Self-contained HTML/CSS/JS interactive API documentation:
  - Dark theme, search/filter, color-coded method badges
  - 11 endpoints: LLM Gateway (port 8088, 4 endpoints) + Agent Runtime (port 8090, 7 endpoints)
  - Per-endpoint: request/response schemas, example JSON, "Try It" panel with editable URL/body and live fetch()
  - No external dependencies

### Added — Phase 6.5: OS-Level Features (12 New Modules)

- **agnos-sys/bootloader.rs** — systemd-boot + GRUB2 auto-detection, boot entry parsing, kernel cmdline validation, timeout management (27 tests)
- **agnos-sys/journald.rs** — systemd journal integration, JSON entry parsing, filtering by unit/priority/time/boot, vacuum, streaming follow (30 tests)
- **agnos-sys/udev.rs** — udev device management, `udevadm` output parsing, rule rendering/validation, device monitoring (26 tests)
- **agnos-sys/fuse.rs** — FUSE mount management, `/proc/mounts` parsing, overlayfs for agent sandboxing, option rendering (32 tests)
- **agnos-sys/pam.rs** — PAM authentication, user/session management, `/etc/passwd` parsing, PAM config parse/render roundtrip (40 tests)
- **agnos-sys/update.rs** — A/B system update slot management, CalVer version comparison, manifest verification, rollback logic (37 tests)
- **agnos-sys/certpin.rs** — TLS certificate pinning with SPKI SHA-256 pins, ASN.1 DER parsing, pin verification, HPKP header generation, default pins for OpenAI/Anthropic/Google (38 tests)
- **agnos-sys/ima.rs** — IMA/EVM file integrity, measurement parsing from sysfs, policy rule builder, xattr verification (31 tests)
- **agnos-sys/tpm.rs** — TPM 2.0 PCR read/extend, sealed secrets, measured boot verification, hardware RNG (23 tests)
- **agnos-sys/secureboot.rs** — UEFI secure boot state detection, key enrollment, kernel module signing verification (18 tests)
- **agent-runtime/network_tools.rs** — Network tool framework with 32 tool types, 7 typed agent wrappers (PortScanner, DnsInvestigator, NetworkProber, VulnAssessor, TrafficAnalyzer, WebFuzzer, SocketInspector), target validation, dangerous arg rejection, risk levels, sandboxed execution, output parsers (100 tests)
- **desktop-environment/wayland.rs** — Wayland protocol abstraction layer (feature-gated), SHM buffers, xdg_shell tracking, surface map, seat capabilities, input event mapping, full Dispatch trait implementations for wl_compositor/wl_surface/wl_shm/wl_seat/wl_output/xdg_wm_base/xdg_surface/xdg_toplevel (58 tests)

### Added — LLM Gateway Certificate Pinning Integration

- Wired `certpin` module into LLM Gateway provider health checks
- `CertPinSet` loaded at startup (default pins for cloud LLM providers)
- Pin verification runs during background health check loop
- Support for enforce mode (hard fail) and report-only mode (log warnings)
- Startup warning for pins expiring within 30 days (12 tests)

### Added — AI Shell: 5 New Natural Language Intents

- **JournalView** — "show journal logs", "view logs for llm-gateway", "show error logs since 1h"
- **DeviceInfo** — "list devices", "show usb devices", "device info for /dev/sda"
- **MountControl** — "list mounts", "unmount /mnt/agent-data", "show fuse mounts"
- **BootConfig** — "list boot entries", "show bootloader", "set default boot entry"
- **SystemUpdate** — "check for updates", "apply system update", "rollback update"
- Total AI Shell intents: 25+ (41 new tests)

### Added — Engineering Backlog (32 Items, All Complete)

- **6 P0 crash/security fixes**: unwrap panics, injection, path traversal, secret zeroing
- **12 P1 performance/correctness fixes**: atomic rate limiting, borrow-based inference API, lock snapshotting, rollback size bounds, TOCTOU elimination, overflow checks, audit chain verification
- **14 P2 polish items**: blit clipping, O(1) task lookup, O(n) result pruning, dead agent eviction, Debug derives, audit log rotation, crypto hash, TimedOut variant

### Security — Code Audit Cycle (March 6, 2026)

Comprehensive security, performance, and correctness audit across all 6 crates. 80+ findings identified, prioritized by severity, all Critical and High items fixed.

#### Critical Security Fixes
- **Shell injection in certpin.rs**: Replaced `sh -c` with direct process spawning + stdin pipes for openssl commands; SPKI hash now computed in Rust via `sha2::Sha256` instead of piping through sha256sum
- **nftables rule injection in netns.rs**: Validate `remote_addr` as IP/CIDR, reject shell metacharacters (`;|&\`$(){}` etc.), skip rules with invalid addresses
- **Seccomp per-agent rules not wired**: `apply_seccomp()` ignored `SandboxConfig.seccomp_rules`; now builds custom BPF filter from per-agent Allow/Deny/Trap rules via new `create_custom_seccomp_filter()` + `syscall_name_to_nr()` mapping (100+ syscalls)
- **Predictable temp files in netns.rs**: Replaced `/tmp/agnos-nft-{name}.conf` with UUID-based paths under `/run/agnos/`

#### High Security Fixes
- **LLM Gateway bound to 0.0.0.0**: Now defaults to `127.0.0.1` (configurable via `AGNOS_GATEWAY_BIND` env var)
- **Agent Runtime API bound to 0.0.0.0**: Now defaults to `127.0.0.1` (configurable via `AGNOS_RUNTIME_BIND` env var)
- **CORS allowed any origin**: Restricted to `http://localhost*` and `http://127.0.0.1*` origins
- **Bearer token unwrap panic**: Replaced `auth_str.strip_prefix("Bearer ").unwrap()` with safe `unwrap_or("")`
- **Production unwraps in HTTP API**: Replaced `serde_json::to_value().unwrap()` with proper error responses (500)
- **Edited commands bypassed risk re-assessment**: `approval.rs` now re-runs `analyze_command_permission()` when the command binary changes during editing
- **Exponential backoff unbounded**: Capped at 300 seconds (5 minutes) to prevent u64 overflow and unreasonable delays

#### Performance Fixes
- **Cache write lock contention**: `LlmCache::get()` now uses `read()` instead of `write()` lock
- **Small type Copy derives**: Added `Copy` to `WindowState` and `Rectangle` (4-8 bytes each)
- **O(n) voter membership**: `eligible_voters` changed from `Vec` to `HashSet` for O(1) contains
- **O(n²) dependency checks**: `get_ready_subtasks()` and `assign_subtask()` build `HashMap` for O(1) dependency lookup
- **Repeated syscall**: `sysconf(_SC_CLK_TCK)` cached in `OnceLock` (value never changes at runtime)
- **O(n) front removal**: `recent_outcomes` changed from `Vec` to `VecDeque` for O(1) pop_front

#### Correctness Fixes
- **Swallowed audit log errors**: All 6 `let _ = audit_log()` sites in supervisor.rs now log warnings on failure
- **LUKS key failure silently ignored**: `sandbox.rs` now returns `Err` instead of `Ok(())` when `LuksKey::generate()` fails
- **Vote percentage truncation bias**: `swarm.rs` now uses `.round()` before `as u8` cast to prevent systematic rounding-down
- **Unused variable warning**: Fixed `s1` in desktop-environment wayland test

#### New in agnos-sys/security.rs
- `pub fn syscall_name_to_nr(name: &str) -> Option<u32>` — maps 100+ common x86_64 syscall names to numbers
- `pub fn create_custom_seccomp_filter(base_allowed, extra_allowed, denied) -> Result<Vec<u8>>` — builds BPF filter with per-syscall Allow/Kill/Trap actions
- `pub const SECCOMP_RET_ALLOW/SECCOMP_RET_KILL_PROCESS/SECCOMP_RET_TRAP` — now public for use by agent-runtime

### Changed

- Phase 6 completion: 30% → 100% (agent intelligence, multi-modal, swarm, LLM analysis, tool wrappers, hardware acceleration, all networking tools)
- agent-runtime lib.rs: added module declarations and re-exports for swarm, learning, multimodal
- agent-runtime tests: 719 → 843 (lib, +2 ignored seccomp integration tests)
- agnos-sys tests: 825 → 831 (lib, +6 new custom seccomp/syscall mapping tests)
- llm-gateway lib.rs: added acceleration module and AcceleratorRegistry re-export
- llm-gateway tests: 206 → 249 (lib)
- desktop-environment tests: 576 → 593 (lib)
- NetworkToolRunner now derives Debug
- ALL_TOOLS expanded from 23 to 32 variants
- Roadmap fully updated to reflect Phase 6.5 completion and all new modules
- Clippy warnings: 82 → 0 (auto-fix + manual fixes)
- `DeviceSubsystem::from_str` / `DeviceEvent::from_str` renamed to `::parse` (clippy `should_implement_trait`)
- `SerialCounter::next` renamed to `::next_serial` (clippy `should_implement_trait`)
- `SeatCapabilities::to_bitmask` / `ModifierState::to_raw` now take `self` by value (Copy types)
- `&PathBuf` → `&Path` in agent-runtime CLI handlers

## [2026.3.5] - 2026-03-05

### Current Status
| Metric | Value |
|--------|-------|
| Phase 5 Completion | 98% |
| Test Coverage | ~80% (4581 tests, 0 failures, 7 ignored) |
| Compiler Warnings | 0 |
| CIS Compliance | ~85% |
| P0/P1 Stubs | 0 |
| Alpha Blocker | Third-party security audit (vendor selection) |

### Changed (Versioning)
- Adopted Calendar Versioning (CalVer) scheme: `YYYY.D.M` format, patches as `-#N`
- Created `VERSION` file at repository root as single source of truth
- Shell scripts (`build-iso.sh`, `agnos-update.sh`, `agpkg`, `entrypoint.sh`) now read from `VERSION` file
- Makefile reads version from `VERSION` file
- Dockerfile copies `VERSION` to `/etc/agnos/VERSION` for runtime access
- Updated all hardcoded `0.1.0` references across Cargo.toml, kernel modules, package specs, CI workflows, and docs

### Added (March 5, 2026 — Final Coverage Push to ~80%)

- **Final test coverage push (+169 tests, 4412 → 4581)**:
  - agnos-sys: +30 (security 15, llm 15), agent-runtime: +14 (resource), ai-shell: +14 (llm)
  - agnos-common: +29 (error 14, lib 15), desktop-environment: +26 (compositor 13, ai_features 13)
  - Fixed flaky `test_next_handle_never_reuses` (global atomic race in parallel tests)
  - Estimated coverage: ~79% → ~80%

### Added (March 5, 2026 — System Tests & Load Tests)

- **End-to-end system tests** (`agent-runtime/tests/system_tests.rs`, 15 tests):
  - Full agent lifecycle via HTTP API (register → heartbeat → get → list → deregister)
  - Multi-agent registration (10 agents), concurrent registrations (50), health endpoint under load (100 calls)
  - Orchestrator + HTTP integration, task lifecycle, priority scheduling, overdue detection
  - Metrics aggregation validation, input validation (empty name, long name)

- **Load/stress tests** (`agent-runtime/tests/load_tests.rs`, 15 tests):
  - 100 concurrent agent registrations, 100 concurrent task submissions
  - Mixed priority flood (200 tasks), rapid heartbeats (1000 total)
  - Register-deregister churn (50 cycles), concurrent task cancellation
  - Queue stats consistency under concurrent ops, large payload handling (1MB JSON)
  - Overdue detection (100 tasks), agent metrics aggregation (100 agents)
  - Concurrent result storage (200 results), task dependency chains

- **Desktop E2E system tests** (expanded `desktop-environment/src/system_tests.rs`, +29 tests, 40 total):
  - Full desktop startup sequence, multi-window workspace management
  - Security alert escalation, permission request flows, override request flows
  - AI context with 5 agents, HUD lifecycle, screen lock interactions
  - Security level transitions, emergency kill switch, file manager navigation
  - Quick settings toggle, compositor window operations, full teardown sequence
  - Cross-component: context detection, smart placement, HUD overlay, security+AI combined

### Added (March 5, 2026 — P3 Completions, Test Coverage Push)

- **AgentControl trait implemented** (`agent-runtime/src/agent.rs`):
  - `check_health()`: process liveness via `kill(pid, 0)` signal check
  - `get_resource_usage()`: delegates to existing `resource_usage()` method
  - `stop(reason)`: delegates to `Agent::stop()`
  - `restart()`: stop + reset to Pending + start sequence

- **Prompt right-side confirmed complete** (`ai-shell/src/prompt.rs:351`):
  - `render_right()` displays execution time + HH:MM:SS clock
  - Already tested — no code changes needed

- **Test coverage push (+1187 tests, 3166 → 4353)** across two rounds:
  - Round 1 (+513): agnos-sys (+65), llm-gateway (+92), agent-runtime (+80), ai-shell (+51), desktop-environment (+44)
  - Round 2 (+674): agnos-common (+65: secrets 18, telemetry 22, audit 25), agnos-sys (+37: agent 16, security 21), agent-runtime (+85: supervisor 20, sandbox 25, orchestrator 20, resource 20), llm-gateway (+76: providers 25, main 20, cache 15, accounting 16), ai-shell (+61: interpreter 20, session 21, security 20), desktop-environment (+53: security_ui 18, apps 17, shell 18)
  - Estimated coverage: ~62% → ~78% (target: 80% for Alpha)
  - All 4,353 tests passing, 0 failures, 0 warnings, 7 ignored (require root)

### Added (March 5, 2026 — CIS Hardening, Security Cleanup, Roadmap Cleanup)

- **CIS benchmark compliance raised to ~85%** (from ~75%):
  - **Kernel config hardening** (all 3 defconfigs: 6.6-lts, 6.x-stable, config/):
    - `CONFIG_USB_STORAGE=n` — CIS 1.1.6 (attack vector reduction)
    - `CONFIG_FIREWIRE=n`, `CONFIG_FIREWIRE_OHCI=n` — CIS 1.1.7 (DMA attack prevention)
    - `CONFIG_THUNDERBOLT=n` — CIS 1.1.8 (DMA attack prevention)
    - `CONFIG_SCTP=n`, `CONFIG_RDS=n`, `CONFIG_TIPC=n`, `CONFIG_DCCP=n` — CIS 3.4.x
    - 7 unused filesystems disabled: CRAMFS, FREEVXFS, JFFS2, HFS, HFSPLUS, UDF, NFSD
    - `CONFIG_SECURITY_APPARMOR=y` added
    - Boot cmdline: `audit=1 audit_backlog_limit=8192` added
  - **New sysctl hardening config** (`config/sysctl/99-agnos-hardening.conf`):
    - CIS 3.1.x: source route rejection, ICMP broadcast ignore, SYN cookies, reverse path filter, martian logging
    - CIS 3.2.x: IPv6 source route, redirect, router advertisement controls
    - Kernel hardening: `dmesg_restrict=1`, `kptr_restrict=2`, `yama.ptrace_scope=2`, `unprivileged_bpf_disabled=1`, `perf_event_paranoid=3`
    - Filesystem: `suid_dumpable=0`, protected symlinks/hardlinks/fifos/regular
  - **Init script updated** (`config/init/agnos-init.sh`): loads sysctl config, sets /tmp sticky bit (CIS 1.1.10)
  - **CIS benchmarks doc updated** (`docs/security/cis-benchmarks.md`): added controls 1.1.6-1.1.10, 3.1.4-3.1.9, 3.2.3, sysctl hardening section

- **Redundant security wrapper removed** (`agnos-sys/src/security.rs`):
  - Removed `enter_network_namespace()` — specialized duplicate of `create_namespace(NamespaceFlags::NETWORK)`, called nowhere except its own `#[ignore]` test
  - Removed corresponding test (ignored tests: 8 → 7)

- **Roadmap P3 items resolved**:
  - GPU vendor detection: confirmed already implemented (NVIDIA/AMD/Intel via nvidia-smi, rocm-smi, sysfs)
  - Feature flags wiring: confirmed N/A (no feature flags exist in desktop-environment)
  - Redundant security wrappers: removed (see above)

### Added (March 5, 2026 — System Benchmarks, Metrics, Dead Code Cleanup)

- **System-level performance benchmarks**:
  - `llm-gateway/benches/system_bench.rs` (699 lines, 6 benchmark groups):
    cache throughput (10/100/500 entries), cache hit/miss ratio, token accounting
    throughput (1/10/50 agents), provider selection overhead, end-to-end inference
    pipeline (mock), cache expiry cleanup
  - `ai-shell/benches/system_bench.rs` (514 lines, 6 benchmark groups):
    session lifecycle, multi-command pipeline (10/50/100 commands), prompt rendering
    pipeline, intent classification throughput (10/50/100/500 inputs), history search
    (100/500/1000/5000 entries), explain pipeline
  - `llm-gateway/src/lib.rs`: New library target re-exporting `cache`, `accounting`,
    `providers` modules for benchmark access
  - Total benchmarks: 36 micro + 22 system-level = 58 across 7 bench executables

- **Performance benchmarks documentation** (`docs/development/performance-benchmarks.md`):
  - Running benchmarks (all, per-package, filtered, baselines, CI mode)
  - Micro-benchmark inventory (36 benchmarks across 4 packages)
  - System-level benchmark inventory (22 benchmarks across 3 packages)
  - Performance targets table (agent spawn, shell response, IPC, cache, memory, boot)
  - CI integration guidance (Criterion HTML reports, regression tracking)

- **Metric dashboard endpoints**:
  - LLM Gateway `GET /v1/metrics` (port 8088): cache stats (total/active/expired
    entries), token accounting (agents, prompt/completion/total tokens), provider
    health (name, available, healthy, consecutive failures)
  - Agent Runtime `GET /v1/metrics` (port 8090): total agents, agents by status,
    uptime, average CPU percent, total memory MB
  - `LlmGateway::cache_stats()` and `accounting_stats()` public methods
  - `AgentMetricsResponse` struct with serde support
  - 4 new tests for metrics endpoints (empty, with agents + heartbeats)

### Fixed (March 5, 2026 — Dead Code Cleanup)

- **Eliminated all 118 compiler warnings** (0 remaining):
  - Removed unused imports across all 6 crates:
    - agent-runtime: `debug`/`error`/`Uuid`/`AgentType`/`MessageType`/`HashMap`/`warn`/`PathBuf`/`Path`/`AgentEvent`/`Agent`
    - ai-shell: `anyhow`/`Style`/`Confirm`/`MultiSelect`/`FsAccess`/`NamespaceFlags`/`Write`/`PermissionLevel`/`Input`
    - desktop-environment: `error`/`warn` from tracing in 5 files
    - Added `#[cfg(test)]` imports for `Uuid`/`MessageType` in `agent-runtime/src/ipc.rs`
  - Removed unused `show_battery: bool` field from `PromptConfig` (ai-shell/src/prompt.rs)
  - Removed unused optional dependencies `git2` and `battery` from ai-shell/Cargo.toml
  - Removed unused `chrono` import from ai-shell/src/prompt.rs
  - Fixed unreachable pattern: `libc::EAGAIN | libc::EWOULDBLOCK` → `libc::EAGAIN` (same value on Linux) in agnos-sys/src/lib.rs
  - Fixed `let mut child` → `let child` in agent-runtime/src/agent.rs
  - Prefixed unused struct fields with underscore: `_config` (audit.rs), `_ipc`/`_message_rx` (agent.rs, ipc.rs), `_uid`/`_gid`/`_euid` (security.rs), `_config`/`_security`/`_output` (session.rs), `_theme` (ui.rs)
  - Added `#[allow(dead_code)]` for API surface items: `check_result`, `memory_max`/`pids`, `unload_model`, `GoogleProvider::new`, `clear`, `reset_all`/`list_agents`, `block_pattern`/`set_auto_approve_low_risk`/`batch_approve`, approval enum variants
  - Added `#![allow(dead_code, unused_mut, unused_imports)]` to desktop-environment (P3 Wayland compositor stub)
  - Prefixed unused variables with underscore: `_context`, `_app_id`, `_from`, `_p`, `_risk`, `_request`, `_config` (wasm_runtime)

- **Test count**: 3056 → 3166 (+110 tests), 0 failures, 8 ignored (require root)

### Added (March 5, 2026 — P2 Implementation Pass)

- **Google Gemini LLM provider** (`llm-gateway/src/providers.rs`):
  - Full HTTP API integration: `infer()`, `infer_stream()` (SSE), `list_models()` via Gemini REST API
  - Supports `generateContent` and `streamGenerateContent` endpoints
  - 14 new tests covering construction, inference, streaming, error paths, trait object usage

- **Cloud provider graceful degradation** (`llm-gateway/src/main.rs`):
  - `ProviderHealth` struct tracking per-provider health (consecutive failures, last check time)
  - `select_provider()` now skips unhealthy providers, tries healthy ones first
  - `infer()` retries up to 2 additional providers on failure with automatic health recording
  - Background health check loop (every 30s) pings providers via `list_models()`
  - 3 consecutive failures → mark unhealthy; 1 success → restore healthy
  - 20 new tests

- **Agent resource quota enforcement** (`agent-runtime/src/supervisor.rs`):
  - `ResourceQuota` struct with configurable thresholds: `memory_warn_pct` (80%), `memory_kill_pct` (95%), `cpu_throttle_pct` (90%)
  - `check_resource_limits()` now enforces quotas: warning + audit at warn threshold, SIGKILL + audit at kill threshold
  - CPU rate tracking between monitoring intervals for throttle detection
  - `set_quota()`/`get_quota()` for runtime tuning
  - Quota cleanup on agent unregister
  - 15 new tests

- **Structured logging** (all binaries):
  - Converted key log sites in llm-gateway to structured tracing fields (`model=`, `agent_id=`, `tokens=`, `error=`)
  - All 4 binaries support `AGNOS_LOG_FORMAT=json` for production JSON output

- **IPC connection backpressure** (`agent-runtime/src/ipc.rs`):
  - ACK/NACK wire protocol: server sends 1-byte response (0x01 ACK, 0x02 NACK_QUEUE_FULL, 0x03 NACK_INVALID)
  - Connection semaphore (max 64 concurrent connections per agent socket)
  - `try_send()` for non-blocking queue-full detection
  - 4 new tests (backpressure NACK, constants, connection limit)

- **Expanded fuzz targets** (`userland/fuzz/`):
  - `fuzz_ipc_framing.rs` — length-prefixed message framing with round-trip validation
  - `fuzz_provider_response.rs` — response parsing for all 5 LLM providers
  - `fuzz_secrets_parse.rs` — SecretValue JSON parsing round-trip
  - Fixed pre-existing compile error in `fuzz_llm_inference.rs`

- **Test count**: 2235 → 3056 (+821 tests), 0 failures, 8 ignored (require root)

### Fixed (March 4, 2026 — Test Suite & Compilation Fixes)

- **Fixed 4 compilation errors in test code**:
  - `agent-runtime/Cargo.toml`: Added missing `tempfile = "3"` dev-dependency
  - `desktop-environment/src/compositor.rs`: Added `Default` impl for `WindowState` enum (returns `Normal`)
  - `desktop-environment/src/compositor.rs`: Added `Window`, `Application`, `System`, `User` variants to `ContextType` enum
  - `desktop-environment/src/security_ui.rs`: Added `Info` variant to `ThreatLevel` enum (lowest severity)
  - `desktop-environment/src/main.rs`: Fixed `test_window_state_defaults` to test enum equality instead of nonexistent struct fields

### Added (March 4, 2026 — Test Coverage Push: 46% → 65%+)

- **1020 new tests** across all packages (1215 → 2235 total), 0 failures, 8 ignored (require root)
- **ai-shell** (185 → 319 tests):
  - `session.rs`: 67 tests covering Session creation, builtins (help/mode/history/clear/exit), mode switching (human/ai/auto/strict), shell command execution, cd handling, process spawning, build_prompt
  - `prompt.rs`: 32 tests covering all 7 PromptModule types (AiMode, Directory, GitBranch, ExecutionTime, ExitStatus, Character, Context), PromptRenderer, PromptConfig defaults, parse_format
  - `ui.rs`: 13 tests covering Ui construction, all show_* output methods
  - `approval.rs`: 21 tests covering RiskLevel, is_blocked for all request types, assess_risk for system paths and batch sizes, auto-approve/auto-deny async paths
  - `interpreter.rs`: 30 tests covering parse for all intent types, translate error paths, explain for mv/cp/top/du/grep/find
- **desktop-environment** (161 → 268 tests):
  - `main.rs`: 16 tests covering parse_cpu_line, read_memory_usage, read_disk_usage, read_cpu_usage, DesktopEnvironment new/initialize/shutdown
  - `apps.rs`: 55 tests covering AppWindow, TerminalApp, FileManagerApp, AgentManagerApp, AuditViewerApp, ModelManagerApp, DesktopApplications, all AppError/AppType variants
  - `ai_features.rs`: 30 tests covering detect_context_type, update_context events, proactive_suggestions filtering, dismiss_suggestion, context history cap, smart_window_placement, agent HUD
- **agent-runtime** (116 → 239 tests):
  - `agent.rs`: 17 tests covering AgentHandle, AgentStatus variants, /proc/self reads (VmRSS, CPU time, FDs, threads), Agent::new, message passing
  - `resource.rs`: 17 tests covering ResourceManager memory/CPU/GPU allocation and release, GpuDevice
  - `supervisor.rs`: 30 tests covering CgroupController (path generation, read/write with tempdir, destroy), HealthCheckConfig, Supervisor clone/register/health, process alive checks
  - `orchestrator.rs`: 27 tests covering TaskPriority ordering, TaskResult serialization, QueueStats, submit/cancel/status, score_agent
  - `registry.rs`: 20 tests covering extract_capabilities, Registry CRUD, RegistryStats, async register/unregister/update
- **agnos-common** (104 → 168 tests):
  - `secrets.rs`: 20 tests covering EnvBackend, FileBackend (encrypt/decrypt roundtrip, wrong key), VaultBackend URLs, SecretInjector
  - `telemetry.rs`: 24 tests covering TelemetryCollector, event/counter/gauge/timing recording, VecDeque eviction, crash reports, flush, system info helpers, serde roundtrips
  - `llm.rs`: 14 tests covering InferenceRequest validate() clamping, prompt truncation, InferenceResponse, ModelInfo, CloudProvider debug redaction, TokenUsage
- **llm-gateway** (47 → 145 tests):
  - `http.rs`: 37 tests including 14 axum integration tests (GET /v1/health, GET /v1/models, POST /v1/chat/completions with auth, 401/404/500 error paths)
  - `providers.rs`: 22 tests covering OllamaProvider/LlamaCppProvider (infer graceful failure, stream channels, trait objects, Arc usage)
  - `main.rs`: 46 tests covering LlmGateway methods, multi-agent token accounting, config edge cases, SharedSession

### Added (March 4, 2026 — Phase 6.5 P0 Kernel Security Features)

- **Audit subsystem bindings** (`agnos-sys/src/audit.rs`):
  - `AuditHandle` wrapping Linux netlink audit socket (AF_NETLINK, NETLINK_AUDIT)
  - `AuditConfig` with netlink and `/proc/agnos/audit` support
  - `AuditStatus` query via AUDIT_GET, enable/disable via AUDIT_SET
  - `AuditRule` add/delete (FileWatch, SyscallWatch types) with validation
  - `RawAuditEntry` parsing from AGNOS proc interface (JSON hash chain)
  - `agnos_audit_log_syscall()` — custom AGNOS syscall (nr 520) fast path
  - 15 tests (8 ignored requiring CAP_AUDIT_CONTROL)
- **MAC profiles (AppArmor/SELinux)** (`agnos-sys/src/mac.rs`):
  - `detect_mac_system()` — auto-detects active LSM from `/sys/kernel/security/lsm`
  - `MacSystem` enum: SELinux, AppArmor, None
  - `AgentMacProfile` with SELinux contexts (`system_u:system_r:agnos_agent_{type}_t:s0`) and AppArmor profiles
  - SELinux: get/set mode, get/set context (current + on_exec), load/remove modules (semodule)
  - AppArmor: load profiles (`.load` interface), change_profile (`/proc/self/attr/current`)
  - `apply_agent_mac_profile()` — one-call auto-detect + apply
  - 20 tests
- **Network segmentation** (`agnos-sys/src/netns.rs`):
  - Per-agent network namespaces with veth pairs and IP configuration
  - `NetNamespaceConfig` with auto-generated IPs (10.100.{hash%255}.{1,2}/30)
  - `FirewallPolicy` + `FirewallRule` with nftables integration
  - `generate_nftables_ruleset()` — pure function for fully testable nft rules
  - NAT support, DNS forwarding, established connection tracking
  - 18 tests (1 ignored requiring root)
- **dm-verity rootfs integrity** (`agnos-sys/src/dmverity.rs`):
  - `VerityConfig` with SHA-256/SHA-512 hash algorithms
  - `verity_format()`, `verity_open()`, `verity_close()`, `verity_status()`, `verity_verify()`
  - Root hash validation (hex-only, correct length for algorithm)
  - `verity_supported()` — checks kernel module + veritysetup availability
  - `read_stored_root_hash()` — reads from `/etc/agnos/verity-root-hash`
  - 12 tests (1 ignored requiring root)
- **LUKS encrypted volumes** (`agnos-sys/src/luks.rs`):
  - Per-agent LUKS2-encrypted loopback volumes (aes-xts-plain64, argon2id)
  - `LuksConfig`, `LuksCipher`, `LuksPbkdf`, `LuksFilesystem` (ext4/xfs/btrfs)
  - `LuksKey` — wraps `Vec<u8>` with zeroing on Drop, `generate()` via `getrandom`
  - `setup_agent_volume()` / `teardown_agent_volume()` — high-level lifecycle
  - Volume naming: `agnos-agent-{id}`, mount at `/var/lib/agnos/agents/{id}/data/`
  - 16 tests (1 ignored requiring root)
- **Sandbox integration** (`agent-runtime/src/sandbox.rs`):
  - `Sandbox` struct gains `netns_handle` and `luks_name` fields
  - New apply ordering: encrypted storage → MAC → Landlock → seccomp → network → audit
  - `apply_encrypted_storage()` — LUKS mount before Landlock locks filesystem
  - `apply_mac_profile()` — MAC context before seccomp blocks /proc/self/attr/ writes
  - `build_firewall_policy()` — translates `NetworkPolicy` to nftables `FirewallPolicy`
  - `emit_audit_event()` — logs sandbox lifecycle via AGNOS audit syscall
  - `teardown()` — cleans up netns + LUKS on agent unregistration
  - `NetworkAccess::Restricted` now creates full netns with nftables (replaces TODO)
  - 12 new integration tests (backward-compatible serialization verified)
- **Supervisor audit integration** (`agent-runtime/src/supervisor.rs`):
  - `unregister_agent()` now cleans up network namespaces and LUKS volumes
  - `handle_unhealthy_agent()` emits audit events
  - Audit events for agent_unregistered and agent_unhealthy
- **New types in `agnos-common/src/lib.rs`**:
  - `NetworkPolicy` — per-agent outbound/inbound port/host firewall rules
  - `EncryptedStorageConfig` — LUKS volume enable/size/filesystem
  - `SandboxConfig` gains: `network_policy`, `mac_profile`, `encrypted_storage` (all `#[serde(default)]`)

### Added (March 3, 2026 — Phase 6.6 Consumer Integration)

- **Secrets management** (`agnos-common/src/secrets.rs`):
  - `SecretBackend` trait with `get_secret()`, `set_secret()`, `delete_secret()`, `list_secrets()`
  - `EnvSecretBackend` — reads from environment variables (dev/simple use)
  - `FileSecretBackend` — AES-256-GCM encrypted file store with random nonces and path sanitization
  - `VaultSecretBackend` — HTTP client to HashiCorp Vault KV v2 API
  - `SecretInjector` — injects secrets into agent environments before spawn
- **Pre-compiled seccomp profiles** (`agent-runtime/src/seccomp_profiles.rs`):
  - `SeccompProfile` enum: Python (~76 syscalls), Node (~72), Shell (~52), Wasm (~44), Custom
  - Per-profile allowlists built on shared `base_syscalls()` foundation
  - `build_seccomp_filter()` → `BpfFilterSpec`, `validate_profile()` checks essential syscalls
  - Wired into `Sandbox::apply_with_profile()` for profile-based sandboxing
- **Agent Registration HTTP API** (`agent-runtime/src/http_api.rs`):
  - Axum HTTP server on port 8090 with REST endpoints
  - `POST /v1/agents/register`, `POST /v1/agents/:id/heartbeat`, `GET /v1/agents`, `GET /v1/agents/:id`, `DELETE /v1/agents/:id`, `GET /v1/health`
  - Input validation: empty name, name length, duplicate detection
- **Multi-agent resource scheduler** (`agent-runtime/src/orchestrator.rs`):
  - `TaskRequirements` struct: min_memory, min_cpu_shares, required_capabilities, preferred_agent
  - `score_agent()` with weighted scoring: memory headroom (40%), CPU headroom (30%), capability match (20%), affinity bonus (10%)
  - Fair-share scheduling with consumption penalty
- **Agent HUD visibility** (`desktop-environment/src/ai_features.rs`, `compositor.rs`):
  - `start_hud_polling(interval)` — periodic GET to agent registration API
  - `render_hud_overlay()` — text-based box-drawing overlay with status icons
- **Security UI enforcement** (`desktop-environment/src/security_ui.rs`):
  - `emergency_kill_agent()` — SIGKILL via libc, cgroup removal, API deregistration, audit log
  - `grant_permission_enforced()` — validates against definitions, blocks in Lockdown for confirmation-required perms
  - `revoke_permission_enforced()` — removes permission, sends SIGHUP
- **WASM runtime** (`agent-runtime/src/wasm_runtime.rs`):
  - `WasmAgent` with `load()` and `run()` using Wasmtime + WASI
  - Feature-gated behind `wasm` feature flag
  - Config: memory limit, fuel metering, preopened directories, env vars
- **Hardened Docker image** (`Dockerfile`, `docker/entrypoint.sh`):
  - Multi-stage build: `rust:1.77-bookworm` builder → `debian:bookworm-slim` runtime
  - Non-root user `agnos` (UID 1000), tini as PID 1
  - Optional gVisor via `--build-arg GVISOR=1`
  - Health check on LLM gateway port 8088, exposes ports 8088 + 8090
- **gVisor configuration** (`docker/gvisor-config.toml`):
  - Default config: platform=systrap, network=sandbox, rootless=true

### Fixed (March 3, 2026 — Phase 6.6)

- **Deadlock in `Compositor::set_window_state()`**: acquired read lock then write lock on same `RwLock<HashMap>` — fixed to single write lock
- **Deadlock in `Compositor::move_window_to_workspace()`**: same read-then-write lock pattern — fixed to single write lock
- **Deadlock in `AIDesktopFeatures::update_context()`**: held write lock on `current_context` while calling `detect_context_type()` which also acquires write lock — fixed with explicit scope drop
- **Duplicate syscall in Python seccomp profile**: `set_tid_address` appeared in both `base_syscalls()` and `python_syscalls()` — removed from profile-specific list
- **Axum route syntax**: HTTP API routes used `{id}` (axum 0.8 syntax) but project uses axum 0.7 which requires `:id` — fixed all parameterized routes
- **Missing tokio runtime for test**: `test_emergency_kill_agent_no_pid` used `#[test]` but calls `tokio::task::spawn` — changed to `#[tokio::test]`

### Added (March 3, 2026 — P0/P1 Implementation Pass #2)

- **Cgroups v2 resource enforcement** (`agent-runtime/src/supervisor.rs`):
  - New `CgroupController` manages per-agent cgroup at `/sys/fs/cgroup/agnos/{agent_id}/`
  - `setup_cgroup()` sets `memory.max`, `cpu.max`, and adds PID to `cgroup.procs`
  - `check_resource_limits()` reads real usage from cgroup counters (`memory.current`, `cpu.stat`)
  - Enforces limits: warns at 90% usage, sends SIGTERM when exceeded
  - Falls back to `/proc/{pid}/` reads when cgroups are unavailable
  - Cgroup cleanup on agent unregistration via `destroy()`
- **Real agent resource monitoring** (`agent-runtime/src/agent.rs`):
  - `resource_usage()` reads VmRSS from `/proc/{pid}/status` (memory in bytes)
  - Reads utime+stime from `/proc/{pid}/stat` (CPU time in ms, clock-tick adjusted)
  - Counts open FDs from `/proc/{pid}/fd/`
  - Counts threads from `/proc/{pid}/task/`
- **Agent pause/resume via signals** (`agent-runtime/src/agent.rs`):
  - `pause()` sends SIGSTOP to actually suspend the process
  - `resume()` sends SIGCONT to resume the process
- **Audit logging with hash chain** (`agnos-sys/src/agent.rs`):
  - `audit_log()` writes JSON lines to `/var/log/agnos/audit.log`
  - Each entry includes SHA-256 hash chaining (hash of previous_hash + timestamp + event + details)
  - File locking via `flock()` for concurrent writer safety
  - Auto-creates log directory if missing
  - `read_last_hash()` reads chain tail for integrity verification
- **Real resource checking** (`agnos-sys/src/agent.rs`):
  - `check_resources()` reads from `/proc/self/` for memory, CPU, FDs, threads
- **LLM syscall implementation via gateway** (`agnos-sys/src/llm.rs`):
  - `load_model()` registers model with LLM Gateway, returns unique handle
  - `unload_model()` deregisters model handle with validation
  - `inference()` sends prompt to `/v1/chat/completions`, writes UTF-8 response to output buffer
  - Thread-safe handle tracking via `RwLock<HashMap>` + `AtomicU64`
  - Input validation: empty model_id, invalid handles, non-UTF-8 input
  - Added 9 new tests (handles, load/unload, inference edge cases)
- **Desktop Agent Manager wired to IPC** (`desktop-environment/src/apps.rs`):
  - `list_agents()` scans `/run/agnos/agents/` for `.sock` files
  - Probes each socket with `UnixStream::connect()` to determine Running/Unresponsive status
  - Merges discovered agents with locally tracked state
- **Desktop Audit Viewer reads real log** (`desktop-environment/src/apps.rs`):
  - `get_logs()` parses JSON lines from `/var/log/agnos/audit.log`
  - Applies time range filters (LastHour, LastDay, LastWeek, Custom)
  - Applies category filters (agent, security, system)
  - `filter_cutoff()` computes time range boundaries
- **Desktop Model Manager queries gateway** (`desktop-environment/src/apps.rs`):
  - `list_models()` fetches from `/v1/models` and merges with local cache
  - `download_model()` uses Ollama-compatible `/api/pull` endpoint
  - `select_model()` validates model exists locally or in gateway before setting active

### Documentation (March 3, 2026 — Consumer Integration)
- **Phase 6.6 added to roadmap**: Consumer Project Integration section tracking AGNOSTIC (QA platform) and SecureYeoman (sovereign AI agent platform) dependencies on AGNOS
- **AGNOSTIC integration**: 6/10 requirements already met (LLM Gateway, caching, cgroups, sandbox, audit), 4 planned for Phase 6.6 (agent registration, HUD, security UI, scheduler)
- **SecureYeoman base image**: 5/17 requirements already met (Landlock, seccomp, cgroups, namespaces), 12 planned across Phase 6.5–6.6 (gVisor, WASM, auditd, dm-verity, LUKS, AppArmor/SELinux, secrets, netns, hardened image)
- **Priority promotions**: 5 Phase 6.5 items promoted to P0 based on consumer needs (auditd, network segmentation, AppArmor/SELinux, dm-verity, LUKS)

### Changed (March 3, 2026 — P0/P1 Pass #2)
- `sha2` crate added to workspace dependencies for audit hash chain
- `reqwest` blocking feature added to `agnos-sys` and `desktop-environment`
- Test count: 402+ → 420+ (9 new LLM tests, updated desktop tests)
- agnos-sys tests: 30 → 36
- P0 stubs remaining: 1 → 0 (cgroups enforcement completed)
- P1 stubs remaining: 6 → 0 (all implemented)
- Phase 5 completion: 82% → 91%

### Security
- **Sandbox enforcement wired to real syscalls** (`agent-runtime/src/sandbox.rs`, `ai-shell/src/sandbox.rs`):
  - `apply_landlock()` and `apply_seccomp()` now delegate to `agnos_sys::security` (previously returned `Ok(())`)
  - agent-runtime: converts `agnos_common::FilesystemRule` to `agnos_sys::security::FilesystemRule` for Landlock, applies seccomp filter, creates network namespaces based on `NetworkAccess` config
  - ai-shell: applies sensible shell defaults (read-only /usr, /lib, /bin, /sbin, /etc; read-write /tmp, /var/tmp)
  - Both degrade gracefully on unsupported kernels (warn but don't fail)
- **Fixed 6 panicking `.unwrap()`/`.expect()` calls in production code**:
  - `llm-gateway/src/http.rs`: `SystemTime::duration_since().unwrap()` → `.unwrap_or_default()` (2 occurrences)
  - `desktop-environment/src/shell.rs`: `partial_cmp().unwrap()` → `.unwrap_or(Ordering::Equal)` (NaN safety)
  - `desktop-environment/src/ai_features.rs`: same NaN fix
  - `agnos-sys/src/agent.rs`: `.expect("failed to build reqwest client")` → `.unwrap_or_else()` with fallback
  - `agnos-common/src/telemetry.rs`: `.expect()` → `.unwrap_or_else()` with fallback (shared reqwest client)
- **Input validation enforcement** (`agnos-common/src/llm.rs`, `llm-gateway/src/main.rs`):
  - Added `InferenceRequest::new()` constructor that auto-validates parameters
  - Added `request.validate()` call at entry point of `LlmGateway::infer()`

### Added
- **Agent health checks** (`agent-runtime/src/supervisor.rs`): Real health monitoring using process liveness check via `kill(pid, 0)` plus IPC socket connectivity probe with 5-second timeout (previously always returned `true`)
- **Agent restart with backoff** (`agent-runtime/src/supervisor.rs`): `handle_unhealthy_agent()` now implements exponential backoff restart (2^n seconds, max 5 attempts). Resets health counters on success, marks agent as permanently Failed after max attempts (previously only logged)
- **Agent-runtime CLI commands wired** (`agent-runtime/src/main.rs`):
  - `start_agent()`: Creates Agent, registers with AgentRegistry, prints status + PID
  - `stop_agent()`: Connects to IPC socket at `/run/agnos/agents/{id}.sock`, sends shutdown
  - `list_agents()`: Enumerates `.sock` files in `/run/agnos/agents/`
  - `get_status()`: Checks socket existence + connectivity with 5s timeout
  - `send_message()`: Validates JSON payload, sends length-prefixed message via Unix socket
- **LLM gateway CLI commands wired** (`llm-gateway/src/main.rs`):
  - `list_models()`: GET `/v1/models`, displays model IDs
  - `load_model()`: Checks model availability via `/v1/models`
  - `run_inference()`: POST `/v1/chat/completions` with messages format
  - `show_stats()`: GET `/v1/health`
- **ai-shell LLM integration** (`ai-shell/src/llm.rs`): Full rewrite connecting to LLM Gateway HTTP API on port 8088:
  - `suggest_command()`: System prompt for shell command generation
  - `explain_command()`: System prompt for command explanation
  - `answer_question()`: General Q&A with AGNOS context
  - All methods fall back gracefully when gateway unavailable
- **Task dependency checking** (`agent-runtime/src/orchestrator.rs`): Scheduler loop now checks `task.dependencies` against completed results before scheduling a task (previously the field was ignored)
- **Real telemetry system info** (`agnos-common/src/telemetry.rs`):
  - `read_os_version()`: Reads PRETTY_NAME from `/etc/os-release`
  - `read_memory_mb()`: Reads MemTotal from `/proc/meminfo`
  - `read_kernel_version()`: Reads kernel version from `/proc/version`
- **Desktop terminal real execution** (`desktop-environment/src/apps.rs`): `TerminalApp::execute_command()` now uses `tokio::process::Command` with stdout/stderr capture (previously returned `"Executed: {cmd}"`)
- **Desktop system status from /proc** (`desktop-environment/src/main.rs`): CPU, memory, and disk usage now read from `/proc/stat`, `/proc/meminfo`, and `libc::statvfs` (previously hardcoded 25%/40%/60%)
- **`pid` field on `AgentHandle`** (`agent-runtime/src/agent.rs`): Added `pid: Option<u32>` field extracted from child process
- **`libc` dependency** added to `desktop-environment/Cargo.toml` for `statvfs` calls

### Changed
- **Roadmap updated** (`docs/development/roadmap.md`): Phase 5.6 P0/P1 items marked complete, Phase 5 revised from 75% to 82%, test counts updated to 402+, Alpha confidence raised to Medium-High

### Metrics
| Metric | Before (March 3 AM) | After (March 3 PM) | Target |
|--------|---------------------|---------------------|--------|
| Phase 5 Completion | 75% | 82% | 100% |
| P0 Stubs Remaining | 7 | 3 | 0 |
| P1 Stubs Remaining | 13 | 6 | 0 |
| Total Tests | 350+ | 402+ | 400+ |
| Test Pass Rate | 100% | 100% | 100% |

### Added
- **Performance benchmarks** (`agent-runtime/benches/bench.rs`): Added 11 benchmarks covering agent ID generation, config creation, task creation/serialization, agent handle operations, task priority ordering, and resource usage
- **Performance benchmarks** (`ai-shell/benches/ai_shell.rs`): Added 7 benchmarks covering interpreter parsing, command translation, and explanation functions
- **Unit test coverage improvements**: Added tests to ai-shell interpreter and history modules, increased test count to 111
- **Integration tests: agent-orchestrator** (`agent-runtime/tests/integration.rs`): Added 16 integration tests covering:
  - Orchestrator initialization and task submission
  - Multi-agent task distribution
  - Task priority ordering
  - Task result storage and retrieval
  - Task failure handling
  - Task cancellation
  - Overdue task detection
  - Queue statistics computation
  - Agent stats tracking
  - Broadcast functionality
- **CIS benchmarks enhanced**: Added 20+ new CIS control checks:
  - Filesystem: USB storage, FireWire, Thunderbolt, /tmp sticky bit
  - Network: source packet routing, ICMP broadcast, SYN cookies, IPv6 source routing
  - Logging: audit rules, logrotate configuration
  - Authentication: password complexity, PAM configuration, shell timeout
  - System maintenance: SSH permissions, account locking
  - AGNOS-specific: kernel lockdown, IMA/EVM, Yama, SafeSetID, AppArmor, User namespaces

### Documentation
- **TODO.md removed**: Consolidated all remaining TODO items into `docs/development/roadmap.md`
- **Agent Development Guide**: `docs/development/agent-development.md` created

### Fixed
- **Critical: `Orchestrator` clone loses shared state** (`agent-runtime/src/orchestrator.rs`): `task_queues`, `running_tasks`, and `results` fields were plain `RwLock<...>` values. When the orchestrator was cloned for the scheduler background task, each clone got fresh empty maps, so the scheduler could never see tasks submitted to the original. Fixed by wrapping all shared interior state in `Arc<RwLock<...>>` and deriving `Clone` instead of a manual impl.
- **Deadlock risk in `cancel_task`** (`agent-runtime/src/orchestrator.rs`): The method held the `task_queues` write lock while attempting to acquire the `running_tasks` write lock, creating a potential deadlock with the scheduler loop. Fixed by dropping `queues` before acquiring `running_tasks`.
- **`get_queue_stats` wrong total** (`agent-runtime/src/orchestrator.rs`): `total_tasks` only summed queued tasks but then tried to subtract running tasks from it. Fixed to correctly compute `total = queued + running`.
- **`is_retriable()` too broad for IO errors** (`agnos-common/src/error.rs`): Not all `std::io::Error` variants are transient. Permanent errors like `PermissionDenied` and `NotFound` were incorrectly marked as retriable. Now only transient IO error kinds (e.g., `TimedOut`, `WouldBlock`, `Interrupted`, `ConnectionReset`) are retriable.
- **`RegistryStats` manual `Clone` impl** (`agent-runtime/src/registry.rs`): Replaced redundant manual `Clone` impl with `#[derive(Clone)]`.
- **CI: Deprecated `actions-rs/toolchain@v1`** (`.github/workflows/ci.yml`): Replaced with the maintained `dtolnay/rust-toolchain@stable`.
- **CI: Deprecated `codeql-action/upload-sarif@v2`** (`.github/workflows/ci.yml`): Updated to `@v3`.
- **CI: Deprecated `codecov-action@v3`** (`.github/workflows/ci.yml`): Updated to `@v4`.
- **CI: Deprecated `returntocorp/semgrep-action@v1`** (`.github/workflows/ci.yml`): Updated to `semgrep/semgrep-action@v1`.
- **CI: aarch64 cross-compilation not set up** (`.github/workflows/ci.yml`): Build matrix now installs `gcc-aarch64-linux-gnu` and `cross` for cross-compiled targets; native builds remain as-is.
- **CI: `actions/cache@v3`** (`.github/workflows/ci.yml`): Updated to `actions/cache@v4`.
- **CI: docs job checked for `TODO.md` existence only** (`.github/workflows/ci.yml`): Now also verifies `docs/development/roadmap.md` exists (the canonical roadmap location).
- **`Cargo.lock` in `.gitignore`** (`.gitignore`): `Cargo.lock` was incorrectly gitignored. For binary/OS crates it must be committed for reproducible builds. Removed from `.gitignore`.
- **README development status stale** (`README.md`): Status section still said "Current Phase: Foundation (Phase 0)". Updated to reflect Phase 5 (Production, 85% complete) and actual Alpha release timeline.
- **README security badge broken link** (`README.md`): Badge linked to `docs/security/security-model.md` which does not exist; corrected to `docs/security/security-guide.md`.

### Added
- Initial project scaffolding and documentation
- README.md, TODO.md, CONTRIBUTING.md, SECURITY.md
- ARCHITECTURE.md with system architecture
- LICENSE (GPL v3.0)
- CI/CD pipeline with GitHub Actions
- Security scanning and build automation
- IPC module (`agent-runtime/src/ipc.rs`): `AgentIpc` and `MessageBus` with full test coverage
- NL interpreter (`ai-shell/src/interpreter.rs`): intent parsing and command translation with full test coverage
- AI shell security, config, and permissions modules with tests
- Desktop environment modules: compositor, shell, apps, AI features, security UI with tests
- LLM gateway providers module with test coverage
- **Agent SDK message loop** (`agnos-sys/src/agent.rs`): Implemented `AgentRuntime::run` with message loop and LLM gateway helper functions
- **LLM Gateway HTTP server** (`llm-gateway/src/http.rs`): OpenAI-compatible API on port 8088 with `/v1/chat/completions`, `/v1/models`, and `/v1/health` endpoints
- **Landlock/seccomp sandbox** (`agnos-sys/src/security.rs`): Full implementation with `NamespaceFlags`, filesystem rules, and seccomp filter generation
- **IPC routing by agent name** (`agent-runtime/src/ipc.rs`): `MessageBus` now routes messages to agents by registered name

### Documentation
- **Architecture Decision Records**: ADR-007 documenting OpenAI-compatible HTTP API for LLM Gateway
- **Integration Guide**: `docs/AGNOSTIC_INTEGRATION.md` for Agnostic platform integration
- **Development Roadmap**: Moved and reorganized `TODO.md` → `docs/development/roadmap.md` with priority-based structure (P0/P1/P2/P3)
- **README Updates**: Updated all references to point to new roadmap location, added package security section
- **CIS Benchmarks**: Complete compliance documentation with validation scripts

### Security & Compliance
- **Fuzzing infrastructure** (`.github/workflows/fuzzing.yml`): Automated daily fuzz testing for critical components
- **SBOM generation** (`scripts/generate-sbom.sh`): SPDX and CycloneDX format support with CI integration
- **CIS benchmarks validation** (`docs/security/cis-benchmarks.md`, `scripts/cis-validate.sh`): Automated compliance checking
- **Dependency vulnerability scanning**: cargo-deny and cargo-outdated integration in CI

### Release Infrastructure
- **Package signing** (`scripts/sign-packages.sh`): GPG signing for all release packages with signature verification
- **Delta update system** (`scripts/agnos-update.sh`): Delta patches with xdelta3/bsdiff, rollback capability, and automatic backups
- **Telemetry system** (`agnos-common/src/telemetry.rs`): Opt-in crash reporting and metrics collection (disabled by default)
- **Release automation** (`.github/workflows/release-automation.yml`): Automated release creation, SBOM attachment, and CHANGELOG updates

### Testing (Early Phase 5)
- Initial test infrastructure setup with tokio async tests
- Early coverage push from ~45% to ~65%
- Foundation tests for agnos-common, ai-shell, agnos-sys

### Fixed
- `agnos-examples` crate: added missing workspace dependencies (`anyhow`, `async-trait`, `tracing`, `tracing-subscriber`) so `file_manager_agent` and `quick_start` examples compile cleanly
- Removed stray `use async_trait::async_trait` import placed after entry-point macro in `file-manager-agent.rs`
- Removed unused `use serde_json::json` import from `file-manager-agent.rs`
- Fixed compilation errors in `agnos-sys`, `agent-runtime`, and `llm-gateway`
- Fixed duplicate test in `agnos-sys/src/security.rs`
- Fixed quote escaping in ai-shell output tests

### Changed
- **Project Structure**: Reorganized roadmap from `TODO.md` to `docs/development/roadmap.md` with clear priority levels (P0-P3)
- **README**: Updated status badge and documentation links to reference new roadmap location
- **Dependency Management**: Upgraded nix crate from 0.27 to 0.31 across all packages to resolve version conflicts

## Release Planning

Versioning follows CalVer: `YYYY.D.M` (e.g., `2026.3.5`). Patch releases use `-#N` suffix.

### Alpha - Target Q2 2026
- Phase 5 production hardening complete
- 80%+ test coverage, 4581 tests, 0 warnings
- Third-party security audit
- CIS compliance ~85%
- All P0/P1 stubs eliminated

### Beta - Target Q3 2026
- Community testing feedback incorporated
- Performance optimization based on benchmarks
- Video tutorials published
- Support channels operational

### Stable - Target Q4 2026
- Production ready
- Enterprise features (SSO, audit logging)
- Certifications complete
- Commercial support available

---

## Template

### [X.Y.Z] - YYYY-MM-DD

#### Added
- New features

#### Changed
- Changes to existing functionality

#### Deprecated
- Soon-to-be removed features

#### Removed
- Removed features

#### Fixed
- Bug fixes

#### Security
- Security improvements and fixes

---

*Note: This project is in pre-alpha development. All versions prior to 1.0.0 are considered unstable and should not be used in production environments.*
