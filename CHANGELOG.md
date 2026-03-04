# Changelog

All notable changes to AGNOS will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

### Testing
- **Test Coverage**: Increased from ~45% to ~65% (target: 80% for Alpha)
- **agnos-common**: 93 tests passing (types, error, telemetry modules fully tested)
- **ai-shell**: 99 tests passing (added 25+ new tests):
  - `sandbox.rs`: 6 new tests
  - `output.rs`: 8 new tests  
  - `audit.rs`: 5 new tests
  - `llm.rs`: 6 new tests
- **agnos-sys**: 29 tests passing (security module with landlock/seccomp tests)
- **Total codebase**: 350+ tests across all packages
- **Test Infrastructure**: All async tests properly configured with tokio

### Metrics
| Metric | Before | After | Target |
|--------|--------|-------|--------|
| Code Coverage | ~45% | ~65% | 80% |
| Total Tests | ~250 | 350+ | 400+ |
| Test Pass Rate | ~95% | 100% | 100% |

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

### [0.1.0] - Phase 1: Core OS - Target Q2 2026
- Bootable hardened Linux base
- Package management system (agpkg)
- Basic userland and init system
- Initial security modules

### [0.2.0] - Phase 2: AI Shell - Target Q3 2026
- Natural language command interface
- LLM Gateway service
- Local and cloud model support
- Bash compatibility layer

### [0.3.0] - Phase 3: Agent Runtime - Target Q4 2026
- Agent Kernel Module
- Multi-agent orchestration
- Agent SDK and templates
- Sandboxing implementation

### [0.4.0] - Phase 4: Desktop - Target Q1 2027
- Wayland-based compositor
- AI-augmented desktop environment
- Essential applications
- Human oversight interface

### [1.0.0] - Phase 5: Production - Target Q2 2027
- Security certifications
- Enterprise features
- Long-term support
- General availability

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
